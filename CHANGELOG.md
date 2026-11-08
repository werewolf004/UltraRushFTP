# Changelog

**English** | [Français](CHANGELOG.fr.md)

All notable changes to UltraRushFTP. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Each section below becomes the notes of the
GitHub release with the same tag.

## [Unreleased]

Browsing and file operations (block I).

### Added

- **Panels**: quick filter (`Ctrl+F`: words, `-word`, masks, regular expression; `F5`
  shows everything again), marks (`Ctrl+M`, bold and underlined; unmark all, select the
  marked, copy their paths), select / unselect by mask (`+` / `-`), copy the names
  (`Ctrl+Shift+C`), *Rename several…* (model with `%f` `%n` `%e` `%c`, replace with text
  or a regular expression, case, preview), *Move to…*; permissions / owner / group columns
  in the remote panels, Windows attributes in the local ones; new **Edit** and **FTP**
  menus.
- **Remote operations**: *Permissions (CHMOD)* (`SITE CHMOD`, `SETSTAT`, contents of the
  folders too), *Change the date* (`MFMT`, otherwise `SITE UTIME`; `SETSTAT`), *View the
  raw listing*.
- **Folder tools**: *Compare the panels* (name or name and size, extensions ignored, hide
  the same items, select the others), *Space used*, *Export the directory tree* (indent,
  sizes, folders only), *Find files at sites* (words and masks, exclusions, kind, dates,
  sizes, depth; results to the queue or copied as `ftp://` addresses). Walks and
  searches use a connection of their own; local folders are walked too.
- **Directory cache** (Options › Browsing, or per site): folders visited again are shown
  from memory, forgotten when a transfer writes into them; *Clear the cache of this site /
  every cache*.
- **View and edit remote files** (`F3` / `F4`, double-click to view): programs chosen by
  masks in Options › Browsing (Notepad otherwise, never the program of the file type);
  an edited file is sent back each time it is saved; *Files being edited* window.
- **Explorer**: files dragged onto a remote panel or pasted with `Ctrl+V` are sent into its
  folder.
- **Both panels**: `Shift`+double-click opens a folder on both sides; the new *Bookmarks*
  menu of a remote panel goes to a bookmark, with `Shift` its local folder too.

### Fixed

- Deleting a remote folder left the browsing connection inside the deleted folder: the
  panel now goes back to its own folder after every series of operations.

## [0.4.0] - 2026-11-08

Queue and transfers (block H).

### Added

- **Queue**: pause and resume (the interrupted transfers resume from their partial file),
  row selection (`Ctrl`, `Shift`), *Edit…* (source, target, mode), *Invert FXP*, *Mark as
  failed*, *Remove the failed items*, *Clear the queue*, *Queue information* (counts,
  sizes, time left), queue files `.urq` loaded and saved by hand.
- **Retries**: per direction, cycle retry, rounds of retry once the queue ends, retry of
  the failed items when a queue starts automatically.
- **End of the queue**: log out, quit, sleep, hibernate or shut down, with a countdown
  that can be cancelled; log out a site removes its waiting items (option); source and
  target of each item in the log (option).
- **`RushApp.FTP.Transfer`**: every parameter — folder and file filters with `%F%` /
  `%D%`, `RS_EXPR`, `RS_NOSKIP`, `CompleteFlag` and `RepeatCount` (folder listed again),
  `RetryCount`, `FolderSort` / `FileSort`, `Depth`, `SubCount`, `RS_NOCIRCLE`,
  `RS_APPEND` (one tab per pair otherwise), `RS_LOGOUT`, `RS_DELEMPTYTABS`.
- **Conditional transfer** (right-click in a panel): name masks, dates, sizes, skip list
  off, sub-folders, log out after; **synchronisation** (compare sizes and dates, delete
  what the source lacks, existing files only, both sides); saved schemes.
- **Advanced transfer** options: transfer mode and ASCII list (no resume for ASCII files),
  case of the target names, active mode for every site, `XDUPE`, `MODE Z` with exceptions,
  TCP buffer, roll back before resuming, disk space kept free, no speed limit on the local
  network.
- **CRC**: SFV check of the downloads (also when the SFV comes last), `XCRC` after uploads
  and FXP, bad files removed and transferred again, one SFV per folder; *Create an SFV /
  MD5 file* in the local panel.
- **Folder rules**: empty target folders removed, server date kept on downloads, new folder
  entered, sub-folder named like its parent refused, 0-byte target of a refused FXP
  removed.

### Changed

- The documentation screenshots are drawn off screen: no window opens and nothing takes
  the focus while they are made. They no longer show the Windows title bar.

### Fixed

- **Stop all** ends the queue: the waiting items were started as soon as the running ones
  had stopped.

## [0.3.0] - 2026-11-07

Connections and sites (block G): keep alive and reconnection, several hosts per site,
per-site settings, listing and active-mode settings, named proxies, FXP roles, unfinished
queue kept across restarts, connection bar on each panel, validation script; checked
against glftpd.

### Added

- **Connection bar on each file panel**, like FTPRush: *Connect* (sites of the site
  manager, groups as sub-menus, `Shift` ghost login, `Ctrl` `-password`), *Reconnect*,
  *Disconnect*, and a **Local ⇄ site switch** that keeps the connection open behind the
  local folder. Editable path field in the remote panels.
- **Connection menu** in the menu bar, acting on the active panel.
- **Help → About UltraRushFTP**: version, build and licence.
- **Validation against real servers**: `scripts/validate-server.ps1` runs a fixed scenario
  with `urftp` (listing, folder upload and download with several transfers, resume both
  ways, overwrite, FXP, removal), compares every file by SHA-256 and writes an anonymised
  report; guide and checklist per server in [docs/VALIDATION.md](docs/VALIDATION.md). The
  script is also in the release archive.
- **`urftp -v` logs the transfer connections** too, not only the first connection.
- **Unfinished queue kept across restarts**: saved automatically (`queue.json`) and offered
  at the next start (Options › General: ask, load or discard).
- **Compatibility with the original mIRC DLL of FTPRush** (Options › General), off by
  default.
- **Named proxies** (Options › Proxy), chosen by name in the site records.
- **`NOOP` during the transfers** (Options › Connection), for proxies and routers that drop
  an idle control connection.
- **Listings encrypted or not, apart from the transfers** (FTPS site record).
- **Ident only while connecting** (Options › Ident).
- **Several SSH keys per site**, tried in turn.
- **FXP roles per site**: never `PASV` as source, always `PASV` as target.
- **Site description**, **listing format** (automatic, Unix, DOS/IIS) and **web address**
  of a site, with *Copy HTTP address* in the remote panel.

- **Listing settings** (Options › Connection, and per site): hidden files (`LIST -a`),
  `MLSD` on or off, custom listing command, `STAT -l` on the control channel, removal of
  the leading spaces, switch to active mode when the passive mode fails.
- **Time zone offset and character set per site**: dates of the listings moved by the
  offset; names, paths and replies converted for the servers that do not use UTF-8.
- **Active mode and network** (Options › Connection): external address announced with
  `PORT`, local port range, local address the connections leave from.
- **E-mail for anonymous logins** and **default download folder** (Options › General).
- **Model of the new sites** (Options › New site).
- **Settings of a site** replacing the global options: retries and timeouts, keep alive and
  reconnection, resume on or off, `FEAT` on or off, `CLNT` client name, skip list,
  priorities, "existing file" rules, separate accounts for uploads and downloads.
- **Ident name per site**: answered by the Ident server while the site connects.
- **Several addresses per site** (site record → Other addresses): tried in turn when one
  cannot be reached or refuses the login; the address that worked is used first next time.
- **Test availability** of a site or of every site of a folder (site manager toolbar).
- **Ghost login and `-password`** from the site manager (`Shift` / `Ctrl` + Connect);
  `RS_INVERT` is now handled in scripts.
- **IPv4/IPv6 "happy eyeballs"**: a name with both kinds of addresses connects through the
  first that answers, a dead IPv6 route no longer costs a full timeout.
- **FXP between IPv6 servers** (`EPSV` on the passive side, `EPRT` on the active side).
- **Keep alive** (Options → Keep alive): anti-idle commands sent while a connection is
  idle (list, interval, random choice, stop after an idle time, log out).
- **Automatic reconnection** when the server closes the connection (kick, idle timeout,
  `421`) or sends a reply matching a mask; the panel goes back to its folder. The queue
  engine checks a pooled connection that stayed idle before reusing it.

### Changed

- The browsing connection of the panels moved from the interface to `urftp-proto::browse`
  so that it is tested against the test server.
- Error messages of the protocol layer are now all in English (some were in French).

### Fixed

- FTPRush v2 can be started while UltraRushFTP runs: the mIRC link window is now
  `TfmUltraRush` (FTPRush v2 refused to open as long as a `TfmRush` window existed). Our
  `rushmirc.dll` finds both applications.
- A queue stopped before the end was lost when the application was closed.
- The quick-connection bar now follows the network, listing and anonymous-login options.

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

[Unreleased]: https://github.com/werewolf004/UltraRushFTP/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.1.0
