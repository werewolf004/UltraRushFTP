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
- ✅ Pascal Script interpreter and `RushApp.*` host (`Login`, `Logout`, `CWD`, `RAW`,
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
- ✅ E — Release 0.1.0: first tag, binaries and changelog on GitHub.
- ✅ F — UI/UX standards (0.2.0): accessible name for every control, complete keyboard
  navigation, WCAG 2.2 AA contrast and 24 px targets checked by tests, interface tests
  with `egui_kittest`, consistent states (focus, hover, disabled, errors).

## Remaining for FTPRush v2 parity

Every topic of the FTPRush v2 help, with its status, is in
[docs/PARITY.md](docs/PARITY.md) (287 topics: ✅ 231 · 🟡 25 · ⬜ 13 · ➖ 18). The
planned work is grouped below into blocks, each ending with a release.

### G — Connections and sites
- ✅ Keep alive (command list, interval, random command, stop after, log out) and
  reconnection when kicked or on matching server replies (G1).
- ✅ Several hosts per site with rotation on login failure; test availability of sites (G2).
- ✅ Ghost login (`!user`, `Shift`+connect) and `-password` (`Ctrl`+connect) from the
  interface, `RS_INVERT` in scripts (G2).
- ✅ Per-site settings: Ident name, retries and timeouts, keep alive, resume, `FEAT`,
  `CLNT`, skip list, priorities, "existing file" rules, separate upload/download accounts
  (G3).
- ✅ Listing settings exposed: `LIST -a` (hidden files), `STAT -l`, custom command, trim
  spaces, `PORT` fallback when `PASV` fails, time zone, charset of non-UTF-8 servers (G4).
- ✅ Active-mode settings exposed: external address, local port range, local IP binding (G4).
- ✅ Anonymous e-mail, default download folder, default site settings (G4).
- ✅ Named proxies, `NOOP` during transfers, listings and transfers encrypted separately,
  Ident only while connecting; several SSH keys per site, FXP roles per site; site
  description, listing format, web address and "Copy HTTP address" (G5).
- ✅ FXP between IPv6 addresses (`EPSV` / `EPRT`), IPv4/IPv6 "happy eyeballs" connection (G2).
- ✅ Validation against a real server: glftpd passed; script and checklist
  ([docs/VALIDATION.md](docs/VALIDATION.md)) for vsftpd, ProFTPD, FileZilla Server, IIS and
  OpenSSH, not run yet (G6).

### H — Queue and transfers
- ✅ Pause, edit an item (source, target, mode, invert FXP), mark as failed, clear failed,
  queue information (H1).
- ✅ Unfinished queue saved automatically and offered at startup.
- ✅ Load / save a queue file by hand (H1).
- ✅ Retry options: per direction, cycle retry, retry failed when the queue ends, reset on
  start (H2).
- ✅ End of queue: idle / disconnect, sleep, hibernate, shut down, exit (with
  confirmation) (H2).
- ✅ `RushApp.FTP.Transfer`: `RS_NOCIRCLE`, `CompleteFlag`, `RepeatCount`, `Depth`,
  `SubCount`, sorting, folder/file filters, `%F%` / `%D%` (H3).
- ✅ Conditional transfer and synchronisation (masks, dates, sizes, delete missing) (H4).
- ✅ XDupe, file-name case conversion, ASCII mode and ASCII list, forced active mode, TCP
  buffer, `MODE Z`, resume roll-back, free-disk-space guard, speed-limit bypass for local
  addresses (H5).
- ✅ CRC: SFV check on download, `XCRC`, re-add bad files, create SFV / MD5; SFV rules (H5).
- ✅ Folder rules: remove empty folders left by a transfer, keep the server date on
  downloads, enter a folder after creating it, refuse a folder named like its parent,
  remove the 0-byte target when an FXP source refuses (H6).

### I — Browsing and file operations
- ✅ Quick filter (`Ctrl+F`), marks, select by mask, copy several names, rename several
  files (I1).
- ✅ CHMOD, move to, modify date, view raw listing, attribute / owner / group columns (I2).
- ✅ Compare folders (name, size, partial, hide same), find files on sites, used space,
  directory tree export (I3).
- ✅ Directory cache; view and edit remote files with external programs (I4).
- ✅ Drag from Explorer, paste local files, both-sides navigation (`Shift`+bookmark,
  `Shift`+double-click) (I5).

### J — Interface
- ✅ Separate FTP / transfer / system logs, log to disk, site names in the log; status bar
  with queue times and progress text (J1).
- ✅ RAW dialog (several lines, all sites), predefined site commands, command designer (J2).
- ✅ Favourites menu (sites, recent sites), add a quick connection to the site manager,
  manual download and incremented URL list, clipboard monitoring (J3).
- ✅ Shortcut manager, configurable mouse actions, confirmations (exit, close a busy tab) (J4).
- ✅ Tray icon, lock, notifications (sounds, pop-ups, folder monitoring), window flash (J5).
- 🟡 Highlights, fonts, custom colours (with the contrast test), sort options, tab layout
  options, saved layouts (J6; no choice of font, tabs on one line).
- 🟡 Site manager: list mode, import plain text and UltraFXP, export text / CSV / HTML /
  XML (without passwords), optional master password; backup and restore of the
  configuration, single instance; About window (J7; UltraFXP import not done: no sample file).

### K — Scripting
- ✅ Arrays, indexes, `case`, `break` / `continue`, constants, user procedures and
  functions (`var` parameters, recursion).
- ✅ `RushApp.UI.Selection`, `OnSwitchLayout`; `RushApp.UI.Bar` / `Menu` answer an error
  (the FTPRush help describes no member of them).
- ✅ Scripts run in a thread of their own: interactive `InputQuery`, a Stop button;
  `MatchExpr`, `MatchString`; `ShellExecuteCommand` asked each time by default.
- ✅ Screen readers: the row under the file-list cursor is told by a live region.

### Distribution
- ✅ Application manifest (`longPathAware`, per-monitor DPI), icon and version resources.
- 🟡 Code signing of the binaries: ready in the release workflow, waiting for a
  certificate.
- ✅ Installer (Inno Setup, per user or for all users) in addition to the portable zip.
- ✅ UPnP port mapping (active mode, Ident), Restart UPnP service.

## Later

Cloud protocols (WebDAV, S3, Google Drive, OneDrive, Dropbox) are outside the initial
scope.
