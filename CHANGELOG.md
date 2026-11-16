# Changelog

**English** | [Français](CHANGELOG.fr.md)

All notable changes to UltraRushFTP. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Each section below becomes the notes of the
GitHub release with the same tag.

## [Unreleased]

### Added

- **Command designer** (*Options › Commands*, FTPRush *FTP Command Designer*): commands
  in folders (sub-menus), a key per command, in the FTP menu and the right-click menu of
  the remote panels; parameters `%f` (once per selected item), `%p`, `%c`,
  `%d[caption]` (asked in a dialog), `%1`…`%9`, `%all%`, `%clearbuf%`, `&window`.

### Changed

- The site commands move from *Options › Interface* to the new *Commands* page; `%1`
  asks its value in a dialog instead of the RAW window.

## [0.14.1] - 2026-11-16

File lists: type-ahead search and a lighter keyboard cursor.

### Added

- Typing the first letters of a name in a list goes to it, like in Explorer.

### Fixed

- The keyboard cursor of the file lists: no more frame on a highlighted row (it covered
  part of the row below); a thin frame remains on a row that is not selected.

## [0.14.0] - 2026-11-15

Connections to more servers: bouncers, `AUTH SSL`, P-521 and 1024-bit RSA certificates,
older SFTP servers.

### Added

- Site option *Use AUTH SSL instead of AUTH TLS* (explicit FTPS), for the servers that
  hang up on `AUTH TLS` (FTPRush's *AUTH SSL* mode).
- Site option *PASV only (no EPSV)*, for the bouncers.

### Changed

- Code signing is not planned: the releases stay unsigned (check the `.sha256` files).
- The UltraFXP import is dropped: the program has been unused for a long time.

### Fixed

- FTP: a data connection that fails after `EPSV` (bouncer that does not forward it)
  switches the connection to `PASV` instead of blocking the listing or the transfer.
- FTPS: servers with an ECDSA P-521 certificate (recent glftpd and OpenSSL) no longer
  abort the TLS handshake (`HandshakeFailure`), and certificates with a 1024-bit RSA key
  (old self-signed ones) are accepted.
- SFTP: older servers (old OpenSSH, NAS, routers, Windows servers without curve25519)
  connect: NIST ECDH and SHA-1 Diffie-Hellman key exchanges, AES-CBC and 3DES ciphers,
  HMAC-SHA1 and DSA host keys are offered last, after the safe algorithms.

## [0.13.0] - 2026-11-14

Advanced transfers (block O) and finishing touches (block P).

### Added

- `MODE Z` for **folder listings** too, and a choice of what is compressed: listings,
  downloads, uploads (Options › Advanced transfer).
- **SFV status folder** beside the checked downloads, kept up to date:
  `[UltraRushFTP] - ( 7 of 10 files OK = 70% ) - INCOMPLETE` (option).
- Scripts: `RushApp.FTP.Transfer` with **`RS_CHECKLOGIN`** first logs in to its sites (at
  the same time, beside the interface) and leaves out the ones that cannot log in.
- **Help window** (`F1`, Help menu): the reference documentation built in (English or
  French), with an index of its sections, a search, and the keyboard shortcuts.
- Script console: **Open…**, **Save**, **Save as…** of `.pas` files; the file name shows
  `•` while the text has changed.
- Options › Transfers: **stop and resume after no data for N seconds** (FTP and SFTP): a
  stalled transfer stops sooner than the reply timeout and is resumed by the retries.

### Fixed

- Right-click menu of the files and of the queue: **Select all**.
- Site manager: sites and folders are moved in the tree by **drag and drop** (onto a
  folder, or before another site).
- After a compressed transfer, a listing on the same connection was read compressed: it
  goes back to `MODE S` (or reads the compressed listing).

### Changed

- The repository is public. CI skips the changes to the documentation only; Dependabot
  leaves out major versions and new egui versions (migrated by hand).
- parking_lot 0.13, flate2 1.1.11.

## [0.12.0] - 2026-11-12

Appearance (block N).

### Added

- Options › Interface: **fonts** of the interface, of the file lists and of the logs (the
  TrueType / OpenType fonts of Windows, by name); **tabs on several lines** and a **least
  width of a tab**; **large buttons** in the toolbars; **links listed apart**, between the
  folders and the files (🔗); **own colour of the progress bars**, per theme, with its
  contrast; a **split Favourites button** whose main part connects to the last site.
- **Double-click actions**: on a log line (copy it), on a site of the site manager
  (connect), on the title of a file panel (refresh, parent folder, switch local / remote),
  on a tab (new tab of the same sites, close) and on a queue item (edit, try again,
  remove).
- Favourites: **quick connections** remembered (ten, never their password); one connects
  again at once when anonymous, else the password field waits for the password.
- View › Layouts › **Panels as tabs**: both file panels in one area, their titles switch
  between them.
- Options › Notifications: **flash the taskbar button while transfers run** in the
  background.

### Fixed

- A click on the name of a file or of a queue item selects it (the text took the click):
  double-clicking a name transfers or opens it again.
- Site manager: the toolbar keeps its size when a site is selected, so the tree does not
  move under the pointer.
- The titles of the file panels have a name for screen readers ("Left panel: Local
  (active)").

## [0.11.0] - 2026-11-12

Everyday comfort (block M).

### Added

- **⬇ / ⬆ speed limit toggles** in the status bar: each limit on or off in one click,
  running transfers included.
- Options › Interface: **file sizes** in bytes, KB or dynamic; **hidden local files** shown
  or not; **columns sized to their content** (local, remote, queue); the local folder of a
  running download **read again every N seconds**; **right-click: Windows Explorer menu**
  in the local panels (also *Windows Explorer menu…* in the panel menu).
- Skip list: **applies to** the contents of folders (auto), the items chosen (manual) or
  both; the items it would skip are **shown in a colour** and struck through.
- **Date tokens** in bookmarks: `%yyyy%`, `%yy%`, `%mm%`, `%m%`, `%dd%`, `%d%`, `%ww%`,
  `%w%` (ISO week).
- FTP › **IP address translate…**: hexadecimal or numeric address to dotted form.

## [0.10.0] - 2026-11-12

Windows, tabs and tasks (block L).

### Added

- **Task window** (View menu): queues and Pascal Scripts run once, every N minutes or on
  chosen days of the week at a time; **Schedule queue to task…** in the ⋯ menu of the queue
  (the queue runs in a new tab, which can close itself when done); run now, turn on / off;
  times missed while the application was closed are shown, not run late (`tasks.json`).
- **Window list** (View menu): one row per tab with its panels, its connections (FTP) and
  what its queue does (Action); a click shows the tab.
- **New tab of current** (File menu, `Ctrl+Shift+T`, right-click on a tab): same layout,
  local folders and connections.
- Options › General › **Both panels for sites**: a second connection goes to the left panel
  before a new tab is opened.

## [0.9.0] - 2026-11-11

Distribution.

### Added

- **Installer** `UltraRushFTP-<version>-windows-x64-setup.exe` (Inno Setup) next to the
  portable zip: for the current user without administrator rights, or for all users;
  English or French; Start menu shortcut, optional desktop shortcut, the mIRC DLLs and the
  documentation; the uninstaller keeps the settings in `%APPDATA%\UltraRushFTP`.
- **Icon** of the application (window, taskbar, notification area, files), **version
  information** in the properties of `UltraRushFTP.exe`, `urftp.exe` and `rushmirc.dll`,
  and an **application manifest**: long paths (`longPathAware`), per-monitor DPI,
  Windows 10/11, current visual style of the system dialogs.
- **UPnP** (Options › Connection): the router opens each active-mode port while it is used
  and port 113 to the Ident server, and its public address is announced with `PORT`;
  **Restart UPnP service** in the FTP menu and on the options page. Written for the
  project (SSDP and SOAP), without a new dependency at run time.
- Code signing ready: releases sign the binaries and the installer as soon as a
  certificate is stored in the repository secrets (`scripts/sign.ps1`); unsigned until
  then.

### Changed

- Releases and CI packages are built by `scripts/package.ps1` (zip, installer, checksums).

### Fixed

- Site manager toolbar: when the window is narrow, **Connect** and the site count move to
  a row of their own instead of covering **Export** and the **List** button.

## [0.8.0] - 2026-11-11

Site properties as complete as those of FTPRush v2.

### Added

- **Site record**: forced UTF-8, login sent in UTF-8, server behind NAT (data connections
  to the address of the server), `PBSZ 0` that can be switched off, encrypted FXP
  automatic / never / always, server time zone, resume Automatic / Yes / No, the
  capabilities used when announced (`PRET`, UTF-8, `MODE Z`, `XCRC`, `MLSD`, `SSCN`,
  `CPSV`), transfers of the site (name case for uploads, downloads and FXP, XDUPE, NOOP
  during the transfers, send and receive buffers), an address and port for the separate
  accounts, the site name in the logs, monitored folders of the site, and the
  **statistics of the server** (last access, connections, sent, received, FXP from and
  to it; copy and reset of the totals).
- **"Existing file" rules per direction** (upload, download, FXP), in the options and per
  site; the rules of earlier versions apply to the three.
- Name case in FXP and *List the folder again when XDUPE returns nothing* (Advanced
  transfer).
- Skip list **Export… / Import…** (JSON, or one mask per line).
- Send and receive buffers set apart in Advanced transfer (the single buffer of earlier
  versions fills both).

### Changed

- A tab connected to a site of the site manager is named after the site instead of its
  address.

## [0.7.0] - 2026-11-08

Scripting (block K).

### Added

- **Pascal Script**: arrays (`[2, 4, 6]`, nested, `M[0, 2]`, `in`, `SetLength`, `Low`,
  `High`), string indexes (`S[1] := 'A'`), `case … of` (labels of any type, ranges),
  `break`, `continue`, `const`, untyped `var`, **procedures and functions** with `var`
  parameters, local variables, `Result` and recursion; `Chr`, `Ord`, `Abs`, `Odd`.
- `RushApp.UI.Selection` (`Prepare`, `Count`, `Items[i]` with `Name`, `Size`, `Date`,
  `Attr`, `Owner`, `Group`, `IsFolder`, `FullPath`, `IsRemoteSide`, `IsFTPConnected`,
  `FTPName`) and the `RushApp.UI.OnSwitchLayout` event (a saved layout is loaded).
- `MatchExpr` (regular expressions) and `MatchString` (wildcards).
- `ShellExecuteCommand`: asked each time by default, with the program, its parameters and
  the origin of the script; Options › Security can start programs without asking, never,
  or keep a list of programs always allowed.
- Screen readers hear the row under the file-list cursor (live region).

### Changed

- Scripts (console, mIRC `RushScript` / `RushCommand`, `-s`, `URUSHFTP_SCRIPT`) run in a
  thread of their own: the window stays usable, `InputQuery` opens an input box and waits
  for it, and the console has a **Stop** button. `Invoke` still answers at once.
- `RushApp.UI.Bar` and `RushApp.UI.Menu` answer a clear error: the FTPRush help describes
  no member of them.

## [0.6.0] - 2026-11-08

Interface (block J).

### Added

- **Logs**: FTP, transfer and system logs in each tab, with times and site names, a number
  of lines of their own, and daily files on disk (option); the status bar shows the
  progress of the queue with a text of your choice (`%p`, `%b`, `%s`, `%e`, `%c`).
- **RAW commands** (`Ctrl+R`): several lines, to the active panel, both panels or every
  connected site, replies listed; predefined **site commands** in the FTP menu, edited in
  the options (`%1` asks for a parameter).
- **Favourites** in the toolbar (recent sites, every site), `➕` saves a quick connection
  as a site; **Manual download** of addresses with incremented lists (`[01-12]`); the
  clipboard watch opens it (or queues at once).
- **Shortcut manager** (Options › Shortcuts, `F1` shows the keys in use); double-click on
  a file: transfer, view or nothing; **confirmations** before quitting with items left,
  closing a busy tab, removing queue items.
- **Notifications**: icon in the notification area (minimize to it), sounds and pop-ups of
  the events (connected, disconnected, queue finished, transfer failed, new items),
  flashing taskbar button at the end of a queue, **monitored folders**.
- **Display**: size of the interface and of the logs, highlighted names and own log
  colours with their contrast shown, folders first or not, largest / newest first, tabs
  below the panels, **saved layouts** (View → Layouts, layout of the new tabs).
- **Site manager**: list mode, import of a text file of addresses, export as text, CSV,
  HTML or FTPRush v2 XML (never the passwords).
- **Security**: optional master password (site manager, options, *File → Lock*), only a
  salted PBKDF2 hash kept; configuration **backups** (now, or each day, 4 kept) and
  restore; **single instance**.

### Fixed

- A test of the availability of a site failed now and then: its result is drawn before
  being checked.

## [0.5.0] - 2026-11-08

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

[Unreleased]: https://github.com/werewolf004/UltraRushFTP/compare/v0.14.1...HEAD
[0.14.1]: https://github.com/werewolf004/UltraRushFTP/compare/v0.14.0...v0.14.1
[0.14.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/werewolf004/UltraRushFTP/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.1.0
