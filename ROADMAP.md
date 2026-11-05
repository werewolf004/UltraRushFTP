# Roadmap

**English** | [Français](ROADMAP.fr.md)

Legend: ✅ done · 🟡 partly done · ⬜ planned. Every large block ends with a tagged
release whose notes come from [CHANGELOG.md](CHANGELOG.md).

## Done

### Phase 1 — protocols and engine (0.1.0)
- ✅ FTP client written for the project: full command control, `LIST` (Unix, DOS/IIS) and
  `MLSD` parsers, long local and remote paths, CRLF injection refused, password masked in
  logs.
- ✅ Explicit and implicit FTPS, `PBSZ`/`PROT`, TLS session reuse, certificate policies
  (Windows store, trusted fingerprint, accept any); active mode `PORT`/`EPRT`.
- ✅ SOCKS4/4A/5 and HTTP proxies (control and data), bypass rules; Ident server.
- ✅ FXP: both role orders, `PRET`, resume, encrypted FXP (`SSCN`/`CPSV`), automatic
  reversal, clean abort.
- ✅ SFTP: password or key (OpenSSH, PEM, PuTTY), host key fingerprints, resume,
  pipelining, proxy.
- ✅ Queue engine: multi-transfer per site, limits, folders, skip list, priorities,
  "existing file" rules, retries, stop, speed limits.

### Phase 2 — application (0.1.0)
- 🟡 Tabbed interface: dockable panels, queue, log, script console, drag and drop,
  shortcuts, trust dialogs, dark and light themes.
- 🟡 Site manager with folders, full site record, DPAPI-protected passwords, import from
  FileZilla, FTPRush v1/v2/v3 and FlashFXP.
- 🟡 Options window (8 pages) saved automatically.

### Phase 3 — automation and compatibility (0.1.0)
- 🟡 Pascal Script interpreter and `RushApp.*` host (`Login`, `Logout`, `CWD`, `RAW`,
  `Delete`, `Transfer`, `RemoveQueue`, `EnableSite`/`DisableSite`, statistics...).
- ✅ mIRC link: `TfmRush` window and `rushmirc.dll` (32/64-bit), tested end to end.
- ✅ Command line: FTPRush v2 startup options and the `urftp` tool.

### Quality (0.1.0)
- ✅ A — `unsafe` isolated in `urftp-sys`, no `unwrap`/`expect` in production, typed
  errors.
- ✅ B — CI/CD on GitHub Actions (format, cargo-deny, Clippy, tests, docs, 32/64-bit DLL,
  tagged releases).
- ✅ C — English code base; English and French interface and CLI.
- ✅ D — Documentation in English and French with screenshots, options and dependency
  references, proprietary licence.

## Next

- ⬜ **E — release 0.1.0**: first tag, binaries and changelog on GitHub.
- ⬜ **F — UI/UX standards and interface tests**: AccessKit labels for every control,
  complete keyboard navigation, WCAG AA contrast checked by a test, automated interface
  tests (`egui_kittest`), consistent spacing and states (focus, hover, disabled, errors).

## Remaining for FTPRush v2 parity

### Connections and sites
- ⬜ Keep-alive (`NOOP`) and automatic reconnection when kicked.
- ⬜ Per-site IP list with rotation on login failure ("Site IP List").
- ⬜ Ghost login (`!user`) from the site record (scripts already support `RS_GHOST`).
- ⬜ Per-site Ident name (stored in the record, not yet answered by the server).
- ⬜ Per-site skip list and "existing file" rules (global only today).
- ⬜ FXP between IPv6 addresses (`EPRT`), IPv4/IPv6 "happy eyeballs" connection.
- ⬜ Validation against real servers (glftpd, vsftpd, ProFTPD, FileZilla Server, IIS,
  OpenSSH).

### Queue and transfers
- ⬜ Queue save/load and scheduling; end-of-queue actions (sleep, hibernate, shut down).
- ⬜ `RS_NOCIRCLE` (folder n+1 waits for folder n), `CompleteFlag`, repeat count and
  folder/file filters of `RushApp.FTP.Transfer`.
- ⬜ XDupe, file-name case conversion, TCP buffer size, forced active mode per direction.
- ⬜ CRC / SFV / MD5 checks.

### Interface
- ⬜ Separate FTP / transfer / system logs; richer status bar.
- ⬜ Directory comparison, quick search and search on sites, tree export, used space.
- ⬜ Raw command designer, shortcut manager, mouse actions, saved layouts, colours.
- ⬜ Event notifications, tray icon and lock, UPnP.
- ⬜ Interactive `InputQuery`, `RushApp.UI.*`, procedures and functions in scripts.

### Distribution
- ⬜ Application manifest (`longPathAware`, DPI awareness), icon and version resources.
- ⬜ Code signing of the binaries.
- ⬜ Installer (optional, in addition to the portable zip).

## Later

Cloud protocols (WebDAV, S3, Google Drive, OneDrive, Dropbox) are outside the initial
scope.
