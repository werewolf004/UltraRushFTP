# Changelog

**English** | [Français](CHANGELOG.fr.md)

All notable changes to UltraRushFTP. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Each section below becomes the notes of the
GitHub release with the same tag.

## [Unreleased]

## [0.2.0] - 2026-11-06

Interface standards: keyboard navigation, accessible names, WCAG 2.2 AA contrast and
target sizes checked by tests; FTPRush v2 parity table.

### Added

- **Parity table** [docs/PARITY.md](docs/PARITY.md): every topic of the FTPRush v2 help
  (284) with its status; the roadmap is regrouped into blocks G to K from it.
- **Complete keyboard navigation**: `Tab` / `Shift+Tab` go through every control, `Esc`
  gives the keys back to the file list; in the list, arrows, `Page Up` / `Page Down`,
  `Home` / `End` move a visible cursor (`Shift` extends the selection, `Ctrl+Space`
  toggles a row); `F6` switches panel; `Ctrl+T`, `Ctrl+W`, `Ctrl+Tab` and
  `Ctrl+Shift+Tab` manage the tabs.
- **Help → Keyboard shortcuts** window (`F1`).
- **Accessible names** for every control (screen readers): symbol buttons, fields that only
  had a placeholder, labels linked to their fields, file rows read with their size and
  date.
- **Interface tests** (`egui_kittest`) that drive the real window with the keyboard and
  check, in English and French, that every control is named and that pointer targets meet
  WCAG 2.5.8 (24 px, or enough spacing).
- **Contrast test**: every text and indicator colour of both themes is measured against
  the WCAG 2.2 AA ratios (4.5:1 text, 3:1 focus and state indicators).

### Changed

- Colours now come from one palette per theme; the light theme uses darker shades so that
  log, state and error texts stay readable, and hovered rows get lighter instead of darker.
- Primary and destructive buttons, text fields (24 px high) and the focus outline (2 px)
  look the same everywhere.
- States are no longer shown by colour alone: connection state symbol (`●` `◌` `✕`) with a
  tooltip, `⚠` before errors, written warning for an invalid mask.
- Confirmation dialogs (delete, untrusted server) open with the keyboard on **Cancel**;
  `Esc` cancels them.
- `F6` replaces `Tab` to switch panel (`Tab` now moves between controls, as in Windows).
- The Options, Site manager and shortcuts windows open below the menu and connection bars.

### Fixed

- The **Cancel** buttons of the file dialogs and of the trust dialog were always in
  French.
- Text fields of the file dialogs kept the focus on every frame, so `Tab` could not reach
  the buttons.

## [0.1.0] - 2026-11-05

First version: protocol engine, tabbed interface, site manager, options, scripting, mIRC
link and command line. Not yet validated against real-world servers.

### Added

- **FTP client** written for the project: full command control, `LIST` (Unix, DOS/IIS)
  and `MLSD` parsing, `EPSV` → `PASV` fallback, `PRET`, `REST` resume, long local and
  remote paths (segment-by-segment `CWD` fallback), CRLF injection refused, password
  masked in logs.
- **FTPS** explicit (`AUTH TLS`, `AUTH SSL` fallback) and implicit, `PBSZ`/`PROT`, TLS
  session reuse on data channels; certificates checked against the Windows store, trusted
  by SHA-256 fingerprint or accepted; active mode `PORT`/`EPRT` with external IP and port
  range.
- **Proxies** SOCKS4, SOCKS4A, SOCKS5 and HTTP tunnel for control and data channels, with
  bypass rules; **Ident server** (RFC 1413, `URUSHFTP` by default).
- **FXP** in both role orders, encrypted with `SSCN`/`CPSV`, automatic reversal when
  `PORT` is refused, clean abort and restoration of `PROT`/`SSCN`.
- **SFTP** with password or key (OpenSSH, PEM, PuTTY `.ppk`), host keys trusted by
  fingerprint, resume, request pipelining, proxy.
- **Queue engine**: several transfers in parallel per site, per-site and global limits,
  folders expanded in turn, skip list, priorities, "existing file" rules, retries, stop,
  speed limits.
- **Interface** with tabs (FTPRush "containers"): two file panels, queue, log and script
  console in a dock, quick connect, drag and drop, keyboard shortcuts, trust dialogs for
  certificates and SSH keys, dark and light themes.
- **Site manager** with nested folders, complete site records, bookmarks, passwords
  encrypted with DPAPI, import from FileZilla, FTPRush v1/v2/v3 and FlashFXP (passwords
  of FTPRush v1/v2 and FlashFXP must be entered again).
- **Options window**: transfers, speed, existing file, skip list, priorities, global
  proxy, Ident, theme, language; saved automatically.
- **Pascal Script** interpreter and `RushApp.*` API (`Login`, `Logout`, `CWD`, `RAW`,
  `Delete`, `Transfer`, `RemoveQueue`, `EnableSite`, `DisableSite`, statistics…).
- **mIRC link**: hidden `TfmRush` window and `rushmirc.dll` (32 and 64-bit) compatible
  with FTPRush scripts, `RAW` replies echoed in mIRC.
- **Command line**: FTPRush v2 startup options (`-w`, `-c`, `-f`, `-r[t][x]`, `-d`) plus
  `-s` (startup script); `urftp` tool (`sites`, `ls`, `get`, `put`, `fxp`, `raw`).
- **Languages**: English and French interface and command line, Windows language by
  default.
- **Documentation** in English and French with screenshots, options and dependency
  references; proprietary licence.
- **CI/CD**: formatting, cargo-deny, Clippy, tests, documentation, 32/64-bit DLL export
  checks on every push; tagged releases with binaries, checksums and licence notices.

### Security

- `unsafe` code confined to `urftp-sys` with a documented justification for every block;
  `unwrap`, `expect`, `panic`, `unreachable` forbidden in production code.
- Known advisory accepted: `RUSTSEC-2023-0071` in `rsa` (used for SSH RSA keys); see
  [SECURITY.md](SECURITY.md).

[Unreleased]: https://github.com/werewolf004/UltraRushFTP/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.1.0
