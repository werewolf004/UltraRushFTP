# UltraRushFTP architecture

**English** | [Français](ARCHITECTURE.fr.md)

Goal: every feature of FTPRush v2, modernised, with **tabbed** site containers and
`rushmirc.dll` compatibility. FTPRush v3 is only a reference for connection types.

## Crates

| Crate | Role | `unsafe` |
|---|---|---|
| `urftp-core` | Data model: sites and bookmarks, site book storage, options, queue and queue files, item options of scripts and conditional transfers (`itemopts`), skip list, priorities, "existing file" rules, SFV / MD5 checksums, panel helpers (`browsing`: quick filter, masks, renaming several items, folder comparison, local walks, space used, directory tree, CHMOD modes, dates), imports, DPAPI-protected secrets | forbidden |
| `urftp-proto` | FTP client, explicit/implicit FTPS, SFTP, FXP, proxies, Ident server, queue engine, speed limiting, site → connection profile conversion | forbidden |
| `urftp-script` | Interpreter of the FTPRush Pascal Script subset (`RushApp.*`, `RS_*` flags) | forbidden |
| `urftp-i18n` | English and French catalogs (`locales/*.json`), `t!` macro, Windows language detection | forbidden |
| `urftp-sys` | **The only crate allowed to contain `unsafe`**: mIRC link window (`TfmUltraRush`, or `TfmRush` in compatibility mode) and shared memory (application side of the mIRC protocol), DPAPI, sleep / hibernate / shut down (`power`), free disk space, files copied in Explorer (`CF_HDROP`) and the `Ctrl+V` key state, `rushmirc.dll` exports (`mirc-dll` feature) | allowed, audited |
| `rushmirc` | mIRC DLL (`cdylib`): re-exports `urftp_sys::mirc`, no code of its own | forbidden |
| `urftp-cli` | `urftp` command-line tool | forbidden |
| `urftp-gui` | `UltraRushFTP.exe` interface (egui / eframe, egui_dock) | forbidden |

```text
urftp-gui ──► urftp-core, urftp-proto, urftp-script, urftp-sys, urftp-i18n
urftp-cli ──► urftp-core, urftp-proto, urftp-i18n
urftp-proto ─► urftp-core ─► urftp-sys (DPAPI)
rushmirc ───► urftp-sys only (the DLL stays small and never loads the engine)
```

## Rules enforced by the compiler

Declared once in `[workspace.lints]` (root `Cargo.toml`) and inherited by every crate:

- `unsafe_code = "forbid"` everywhere except `urftp-sys`, which has stricter rules of its
  own: `undocumented_unsafe_blocks` and `missing_safety_doc` denied (every `unsafe` block
  carries a `// SAFETY:` justification), `unsafe_op_in_unsafe_fn` denied.
- `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `unreachable`,
  `dbg_macro` denied outside tests (`clippy.toml` allows them in tests only).
- Errors are typed with `thiserror` (`FtpError`, `SftpError`, `SessionError`,
  `ProxyError`, `ScriptError`, `HostError`, `ImportError`, `CliError`…); user-facing
  remarks are typed too (`ImportWarning`, `SkipReason`) so that the interface can translate
  them.
- Formatting by `rustfmt.toml`; Clippy runs with `-D warnings` in CI.

## Design

### Threads and data flow

- The interface (egui) runs on the main thread and never blocks on the network. Each remote
  panel owns an *actor*: a tokio task holding the browsing connection, driven by
  `BrowseCmd` messages and answering with `BrowseEvent`s (`urftp-proto::browse`, bridged to
  the interface thread by `urftp-gui::browser`). While idle it sends the anti-idle commands
  of `urftp-proto::keepalive`; when the connection is lost it reconnects and goes back to
  its folder.
- Transfers are run by the queue engine (`urftp-proto::engine`) on a separate multi-thread
  tokio runtime. Each tab has its own `Queue`; the engine keeps a connection pool per site
  and publishes `EngineEvent`s (started, progress, done, skipped, failed…). It pauses
  (running items wait again) and stops for good (*Stop all*); folders are expanded with the
  options of their item (filters, depth, sorting, refreshes, synchronisation against the
  target listing), and file transfers follow `TransferRules` (ASCII, `MODE Z`, names, disk
  space, local network, `XDUPE`, SFV / `XCRC` checks).
- Every connection (panel, engine, availability test) goes through
  `RemoteSession::open`, which tries the other addresses of the site in turn, and opens
  its TCP socket with `urftp-proto::net` (IPv6/IPv4 "happy eyeballs", RFC 8305).
- `App::logic` runs every frame, including while the window is minimised, so transfers,
  statistics and mIRC requests keep being processed.

### Long paths

- **Local**: no `MAX_PATH` (260) limit. File operations go through `std::fs`, which adds the
  `\\?\` prefix when needed; `urftp-core::localpath::to_verbatim` produces `\\?\C:\…` /
  `\\?\UNC\…` paths for direct Win32 calls. Tested with trees longer than 600 characters.
  An application manifest with `longPathAware` is still to be added (see the roadmap).
- **Remote**: no fixed-size buffer. When a server refuses a long absolute `CWD`, the client
  navigates segment by segment, then acts on the bare name.

### Tabs and multi-transfer

- A **tab** is an independent context: two panels, its queue, its log, its script console.
  Several tabs can be connected to different sites or to the same one.
- **Multi-transfer**: from one tab to one site, several files are transferred in parallel
  on transfer connections separate from the browsing connection, within the per-site
  limits (simultaneous transfers, maximum downloads, maximum uploads; 0 = single-connection
  mode) and an optional global limit.
- Queue order: priorities, then insertion order; folders are expanded when their turn
  comes and their contents take their place.

### Connections

- **FTP**, **explicit FTPS** (`AUTH TLS`, `AUTH SSL` fallback), **implicit FTPS**,
  `PBSZ 0` + `PROT P|C` per site, TLS session reuse on the data channels (required by
  vsftpd `require_ssl_reuse`, glftpd…), **SFTP** (russh, ring provider, 16 requests in
  flight).
- **FXP**: source `PASV` / destination `PORT` or the reverse, `PRET`, `REST`, encrypted FXP
  with `SSCN` or `CPSV` (optional clear fallback), automatic role reversal when `PORT` is
  refused, `ABOR` of the other side on failure, `PROT`/`SSCN` restored afterwards.
- **Proxy** (global or per site): SOCKS4, SOCKS4A, SOCKS5 (with authentication), HTTP
  tunnel (`CONNECT`, Basic authentication); optional for the data channels; bypass for
  local addresses and by host masks.
- **Ident** (RFC 1413): server on TCP 113 (configurable), global name (`URUSHFTP` by
  default), "UNIX" system.

### Browsing connection and folder tools

- Each remote panel has one browsing connection (`urftp_proto::browse`): an actor that
  runs the panel commands one after the other (list, change folder, rename, `SITE CHMOD`,
  `MFMT` / `SITE UTIME`, raw listing), keeps the connection alive and reconnects. After a
  series of operations it goes back to the folder of the panel, since walking the
  contents of a folder changes the FTP current folder.
- The directory cache lives in that actor (per connection, per folder); a refresh always
  lists from the server, and the interface makes it forget the folders a transfer wrote
  into.
- The longer walks (space used, directory tree, find files at sites) and the files opened
  with another program use connections of their own (`RemoteSession::walk`, `retr`,
  `stor`), so the panel stays free; their results come back to the interface over
  channels. Local walks run on a thread (`urftp_core::browsing::walk_local`).

### mIRC protocol (`rushmirc.dll`)

Notes taken from the original Delphi source (not redistributed in this repository):

| Element | Value |
|---|---|
| Main window class looked up by the DLL | `TfmUltraRush`, then `TfmRush` (FTPRush, or UltraRushFTP in compatibility mode); `URUSHFTP_IPC_CLASS` replaces both for tests |
| Application ↔ DLL shared memory | `FTPRush`, 4096 bytes |
| DLL → mIRC shared memory | `mIRC`, 4096 bytes, then `WM_USER + 200` |
| `Invoke` | copies the request into `FTPRush`, sends `WM_USER + 300` (`WM_EXCHANGE`), reads the answer back |
| `RushScript` | `WM_COPYDATA`, `dwData = 1000`, `wParam` = callback window when callbacks are on |
| `RushCommand` | `WM_COPYDATA`, `dwData = 1001` |
| Raw-command reply | `WM_COPYDATA` to the callback window, `wParam = 888`, `dwData` 1 = success / 0 = failure; lines: site, command, results |

The original defines `EnableCallback` / `DisableCallback` without exporting them;
UltraRushFTP exports them. CI checks that both DLLs (32 and 64-bit) export exactly
`LoadDll`, `UnloadDll`, `Invoke`, `RushScript`, `RushCommand`, `EnableCallback` and
`DisableCallback`.

### Data and secrets

- Data folder: `%APPDATA%\UltraRushFTP`, or the folder given by `-f` / `--data` /
  `URUSHFTP_DATA`. Files: `sites.json` (atomic write, `sites.json.bak` backup),
  `settings.json`, `stats.json`.
- Passwords (sites, proxies) are stored as `dpapi:<base64>`: Windows DPAPI bound to the
  user account, with application-specific entropy. A file copied to another machine or
  account reveals nothing. Details in [SECURITY.md](../SECURITY.md).

## Testing

- Unit tests next to the code; integration tests in `crates/urftp-proto/tests` against
  in-memory FTP/FTPS, SFTP, SOCKS and HTTP proxy servers (localhost only), plus an
  end-to-end mIRC test (fake mIRC window ↔ DLL ↔ `TfmRush` server) in `crates/urftp-sys`.
- `cargo run -p urftp-proto --example dev_server` starts the same FTP server for manual
  tests (`test` / `secret`).
- `scripts/screenshots.ps1` regenerates the documentation images from demo data. The
  application is drawn off screen by wgpu (the ignored test `app::shots::render_screenshot`
  of `urftp-gui`, with `egui_kittest`): no window opens and nothing takes the focus.

## Functional reference: FTPRush v2

Help pages of v2 to cover (parity): site manager, quick connect, bookmarks/favourites,
queue (editing, priorities, failures, save/load, scheduling), skip list / allow list /
priorities, conditional transfer, quick search and site search, directory comparison,
CRC/SFV/MD5, raw commands and command designer, proxy editor, Ident, UPnP, keep-alive,
event notifications, shortcut manager, mouse actions, layouts, colours/themes,
statistics, tree export, used-space calculation, tray lock, end-of-queue actions (sleep,
hibernate, shut down). Progress is tracked in [ROADMAP.md](../ROADMAP.md).
