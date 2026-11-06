# FTPRush v2 parity

**English** | [Français](PARITY.fr.md)

Every topic of the FTPRush v2 help file (235 pages, read in full on 2026-11-06) and where
UltraRushFTP stands. The help is used as a functional reference only; nothing is copied
from it and FTPRush itself is not decompiled.

Legend: ✅ done · 🟡 partly done · ⬜ planned · ➖ out of scope (reason given).

Summary (284 topics): ✅ 69 · 🟡 51 · ⬜ 147 · ➖ 17.

## Main window and menus

### File

| Topic | Status | Notes |
|---|---|---|
| New window | ✅ | New tab (`+`, `Ctrl+T`) |
| New window of current | ⬜ | Duplicate a tab with its connections |
| Close current window | ✅ | `✕`, `Ctrl+W` (disconnects) |
| Minimize to system tray | ⬜ | Tray icon |
| Lock to system tray | ⬜ | Needs a password on the site book (see *Security setting*) |
| Backup / restore configuration | ⬜ | One archive with sites, options and statistics |
| Exit | ✅ | |

### Edit and mark

| Topic | Status | Notes |
|---|---|---|
| Search (fast filter) | ⬜ | `Ctrl+F` filter of a panel, words, `-word`, regex |
| Rename | ✅ | `F2` |
| Delete | ✅ | `Del`, with confirmation |
| Make directory | ✅ | `F7` |
| Rename multiple files | ⬜ | |
| Copy selected names | 🟡 | "Copy path" for one item; several names not yet |
| Mark / unmark, unmark all, select marked, export marks | ⬜ | Marks distinct from the selection |
| Select (by mask) | ⬜ | |
| Select all | ✅ | `Ctrl+A` |

### View

| Topic | Status | Notes |
|---|---|---|
| Refresh | ✅ | `F5` |
| Window list | 🟡 | Tab strip only; no list with connection and action columns |
| Queue window | ✅ | Dock panel |
| System log, transfer log, FTP log | 🟡 | One log per tab with command / reply / status / error colours |
| Task window | ⬜ | Scheduled tasks |
| Remote columns size / date | ✅ | |
| Remote columns attributes / user / group | ⬜ | Parsed by the listing, not shown |
| Local columns size / date | ✅ | |
| Window list columns | ⬜ | |
| Grid lines | ➖ | Striped rows instead |
| Columns auto size | 🟡 | Columns resizable, no automatic fit |
| Current edit files | ⬜ | Comes with *File associations* |

### FTP

| Topic | Status | Notes |
|---|---|---|
| Quick connect dialog | ✅ | Quick-connect bar |
| Attributes (CHMOD) | ⬜ | `SITE CHMOD` |
| Move to | ⬜ | Remote `RNFR`/`RNTO` to another folder |
| Modify date of FTP files | ⬜ | `MFMT` / `SITE UTIME` |
| Calculate FTP space used | ⬜ | Recursive size with a depth |
| Export directory tree list | ⬜ | |
| Find files at sites | ⬜ | Recursive search with filters |
| Clean site cache / all caches | ⬜ | Comes with the directory cache |

### Tools → Queue

| Topic | Status | Notes |
|---|---|---|
| Start / pause queue | 🟡 | Start and stop; no pause that keeps the connections |
| Add the selected to queue | ✅ | Transfer button, `Space`, drag and drop |
| Conditional transfer | ⬜ | Include/exclude masks, dates, sizes, synchronisation |
| Clear current queue files | 🟡 | "Remove finished" and remove one item; not "all waiting" |
| Move up / down / top / bottom | ✅ | Right-click |
| Edit queue | ⬜ | Change source, target, mode; invert FXP |
| Reset all failed | ✅ | "Retry failed" |
| Reset selected failed | ✅ | Right-click → Retry |
| Clear all failed | ⬜ | |
| Mark selected / all as failed | ⬜ | |
| Queue information | ⬜ | Counts, sizes, times |
| Restore queue, load / save queue file | ⬜ | |
| Schedule queue to task | ⬜ | |
| Limit upload / download speed toggles | 🟡 | Limits in the options; no one-click toggle |

### Tools → other

| Topic | Status | Notes |
|---|---|---|
| Site manager | ✅ | |
| Add to site manager | ⬜ | Save a quick connection as a site |
| Command (predefined site commands) | ⬜ | Menu of `SITE` commands per server type |
| Command designer | ⬜ | |
| Send RAW command | 🟡 | `RushApp.FTP.RAW` from the script console; no dialog, no "send to all" |
| Compare folder contents (name, size, partial, hide same) | ⬜ | |
| View raw directory | ⬜ | Unparsed listing |
| Script designer | 🟡 | Script console in each tab; no editor with files |
| Languages | 🟡 | English and French |
| Hotkey manager | ⬜ | Shortcuts are fixed (list: `F1`) |
| Manual download from site, increment URL list | ⬜ | |
| Monitor clipboard, auto transfer | ⬜ | |
| Transfer mode auto / binary / ASCII | 🟡 | `TYPE A` supported by the protocol layer; no setting or ASCII list |
| On transfer completed: disconnect / idle | ⬜ | |
| On transfer completed: shut down / standby / hibernate | ⬜ | |
| On transfer completed: exit | 🟡 | Command line `-rx` only |
| Create SFV or MD5 file | ⬜ | |
| IP address translate | ⬜ | Hexadecimal / numeric address to dotted form |
| Restart UPnP service | ⬜ | Comes with UPnP |
| Option | ✅ | Options window |

### Help

| Topic | Status | Notes |
|---|---|---|
| Help index | 🟡 | `F1` lists the shortcuts; full documentation in `docs/` |
| FTPRush wizard | ➖ | Interface customisation wizard of the Delphi toolkit |
| Check new version | ➖ | No online update in this phase |
| About | ⬜ | Version, licence, notices |

### Main interface and getting started

| Topic | Status | Notes |
|---|---|---|
| Main interface layout (panels, logs, queue, status bar) | 🟡 | Dock with panels, queue, log, script; one log, no task window |
| Side indicator | 🟡 | `▸` on the active panel |
| Connect to FTP from the favourites menu | ⬜ | Drop-down menu of sites in the toolbar |
| Disconnect from site | ✅ | |
| Lock / unlock toolbar and dock | ➖ | The dock is rearranged by dragging; no toolbar to lock |
| Quick connect to a site | ✅ | `Tab` between fields, `Enter` connects |
| Customize main interface (styles) | ➖ | Delphi toolkit styles (XP, Office 2003...); dark and light themes instead |

## Dialogs

| Topic | Status | Notes |
|---|---|---|
| Add or edit skip list item | ✅ | Edited in the grid of the options |
| Bookmark dialog | 🟡 | Name, remote and local paths; date tokens (`%yyyy%`, `%mm%`...) not yet |
| Create CRC checksums | ⬜ | |
| Condition transfer | ⬜ | See *Conditional transfer* |
| Calculate FTP space used | ⬜ | |
| Edit queue | ⬜ | |
| Export directory tree list | ⬜ | |
| Fast search | ⬜ | |
| Find files at sites | ⬜ | |
| Hotkey manager | ⬜ | |
| Send RAW dialog (`Ctrl+R`, several lines, all sites) | ⬜ | |
| Manual download | ⬜ | |
| Increment URL | ⬜ | |

## Site manager

| Topic | Status | Notes |
|---|---|---|
| Toolbar: properties, folder, new site, delete | ✅ | |
| Tree mode | ✅ | |
| List mode, column auto width | ⬜ | |
| Import plain text (one URL per line) | ⬜ | |
| Import UltraFXP sites and settings | ⬜ | |
| Import FTPRush XML | ✅ | Plus FTPRush v1/v3, FileZilla, FlashFXP; passwords are not imported |
| Export plain text / CSV / HTML / FTPRush XML | ⬜ | Without passwords |
| Default site setting | ⬜ | User, password and limits of a new site |
| Security setting (password on the site book, show passwords) | 🟡 | Passwords protected by Windows DPAPI; no master password or lock |
| Test availability of sites | ⬜ | |

### Site properties

| Topic | Status | Notes |
|---|---|---|
| Protocol: plain FTP, `AUTH TLS`, implicit SSL, `AUTH SSL`, SFTP | ✅ | `AUTH SSL` as fallback of explicit FTPS |
| Several host:port entries | 🟡 | Stored (`alt_hosts`); not chosen or rotated yet |
| Port, user, password, anonymous | ✅ | |
| Passive mode, active fallback when `PASV` fails | 🟡 | Passive / active per site; no automatic fallback |
| Server behind NAT (use the host address for `PASV`) | ✅ | Automatic when `PASV` returns a private address |
| Time zone | ⬜ | |
| Maximum simultaneous transfers (0 = browsing connection) | ✅ | |
| Remote and local paths | ✅ | |
| Description (short text for menus, notes) | 🟡 | Notes; no short description |
| Bookmarks | ✅ | |
| HTTP URL mapping, copy HTTP URL | ⬜ | |
| Display: site name instead of [1]/[2] in the log | ⬜ | The log shows Left / Right |
| Connection: global or custom retries and timeouts | 🟡 | Global only |
| Resume support: auto / yes / no | 🟡 | Detected automatically; not forced per site |
| Keep alive per site | ⬜ | |
| `FEAT` per site | 🟡 | Always sent; not configurable |
| `PRET` | ✅ | Used when announced |
| UTF-8 | ✅ | `OPTS UTF8 ON` when announced |
| `CLNT` | ⬜ | |
| Compression `MODE Z` | ⬜ | |
| `XCRC` check after transfer | ⬜ | |
| `MLSD` | ✅ | |
| `SSCN` / `CPSV` for encrypted FXP | ✅ | |
| Account: different logins for uploads and downloads | ⬜ | |
| Ident: global or custom name | 🟡 | Stored in the record; the Ident server answers the global name |
| Proxy: global, bypass, custom; data channel | ✅ | |
| List mode: `STAT -l` | 🟡 | Supported by the protocol layer (`list_command`); no setting |
| List mode: trim leading spaces | ⬜ | |
| List mode: custom list command | 🟡 | Same as `STAT -l` |
| List mode: UPnP | ⬜ | |
| Directory cache per site | ⬜ | |
| Directory charset (non-UTF-8 servers) | ⬜ | |
| Listing parser choice | 🟡 | Automatic detection (Unix, DOS/IIS, `MLSD`); no manual choice |
| SSL: `PBSZ 0` | ✅ | |
| SSL: encrypt listings / transfers separately | 🟡 | One "protect data" setting for both |
| SSL: encrypted FXP | ✅ | |
| SFTP: private keys | 🟡 | One key per site; several keys not yet |
| Transfer: file-name case conversion | ⬜ | |
| Transfer: XDupe | ⬜ | |
| Transfer: FXP method (no `PASV` as source, `PASV` as target) | 🟡 | Automatic reversal when `PORT` is refused; no fixed setting |
| Transfer: force active mode | ⬜ | |
| Transfer: TCP buffer size | ⬜ | |
| Skip list, priority, file-exists rules per site | ⬜ | Global only |
| Event notification: monitor folders for new items | ⬜ | Needs keep alive |
| Statistics of this server | ⬜ | Global and session statistics exist |

## Options

### General

| Topic | Status | Notes |
|---|---|---|
| E-mail address for anonymous login | ⬜ | |
| Default download path | ⬜ | Local panels start in the user folder |
| Data folder path | ✅ | `-f`, `URUSHFTP_DATA` |
| Daily backup of the configuration | ⬜ | |
| Cache FTP directories, update cache of transferring folder | ⬜ | |
| Check new version at startup | ➖ | No online update in this phase |
| Single instance | ⬜ | |
| Splash screen | ➖ | The window opens immediately |
| Log buffer size | 🟡 | Fixed at 5 000 lines |
| Log to disk | ⬜ | |
| Batched log updates | ➖ | The interface already redraws at most once per frame |
| Thread pool | ➖ | Asynchronous runtime (tokio); nothing to tune |
| UPnP | ⬜ | |

### Connection

| Topic | Status | Notes |
|---|---|---|
| Send `FEAT` | 🟡 | Always sent |
| Connect timeout, reply timeout | ✅ | |
| Max retries, retry delay | ✅ | |
| Cycle the server IP list on login failure | ⬜ | |
| Auto reconnect on matching server replies | ⬜ | |
| Smart handling of servers behind NAT | ✅ | |
| Bind sockets to an external address (active mode) | 🟡 | Supported by the protocol layer; no setting |
| Bind to a local IP address | ⬜ | |
| Limit local port range | 🟡 | Supported by the protocol layer; no setting |
| Ident: enable, name, system, port | ✅ | |
| Ident: only while connecting | ⬜ | |
| Keep alive: command, interval, stop after, log out, reconnect when kicked, random command | ⬜ | |
| Proxy: list of named proxies | 🟡 | One global proxy and one custom proxy per site |
| Proxy: `NOOP` during transfers, passive with proxy | ⬜ | |
| Proxy: bypass for local addresses, exceptions | ✅ | |
| Proxy editor | ✅ | Inline in the options and the site record |
| List mode: show hidden files (`LIST -a`) | 🟡 | Supported by the protocol layer; no setting |
| List mode: custom command | 🟡 | Same |
| List mode: switch to `PORT` when `PASV` fails | ⬜ | |
| List mode: use `MLSD` if available | ✅ | |

### Transfer

| Topic | Status | Notes |
|---|---|---|
| Download / upload speed limits | ✅ | |
| Bypass speed limit for local addresses | ⬜ | |
| File-name case conversion | ⬜ | |
| Force active mode for transfers | ⬜ | |
| Stop and resume when no data is received | 🟡 | Reply timeout then retry with resume |
| Roll back when resuming a download | ⬜ | |
| Keep free disk space | ⬜ | |
| Maximum simultaneous transfers per tab | ✅ | |
| TCP buffer size | ⬜ | |
| Transfer mode auto / binary / ASCII | 🟡 | See the menu row |
| On transfer completed: idle / disconnect | ⬜ | |
| ASCII list, no resume for ASCII files | ⬜ | |
| Skip list: enable, zero-byte files, empty folders, folders | ✅ | |
| Skip list: colour of skipped items | ⬜ | |
| Skip list: mode auto / manual / both | ⬜ | Applied to every item today |
| File-exists rules (resume, skip, rename, overwrite, ask) | ✅ | Plus overwrite when resume fails |
| Queue: reload unfinished queue at startup, auto save | ⬜ | |
| Queue: keep failed items | ✅ | |
| Queue: cycle retry (failed item goes to the bottom) | ⬜ | |
| Queue: retry all failed after the queue ends | ⬜ | |
| Queue: clear when switching server manually | ➖ | No manual server switching between tabs |
| Queue: reset failed when started automatically | ⬜ | |
| Queue: clear when logging out a transferring site | ⬜ | |
| Queue: source / destination in the transfer log | ⬜ | |
| Queue: retries per direction (upload, download, FXP) | 🟡 | One value for all |
| Priority list | ✅ | |
| Compression `MODE Z` (listing, upload, download, exceptions) | ⬜ | |
| Disk cache for downloads | ➖ | Buffered writes are handled by the runtime |

### Misc.

| Topic | Status | Notes |
|---|---|---|
| Check SFV when downloading, CRC log and status folder, re-add bad files | ⬜ | |
| Use `XCRC` if available | ⬜ | |
| Enter a folder after creating it | ⬜ | |
| Remove empty folders left by a transfer | ⬜ | |
| Refuse a folder with the same name as its parent | ⬜ | |
| One SFV per folder, skip all SFV if several | ⬜ | |
| Keep the server date on downloaded files | ⬜ | |
| Remove the 0-byte target file when an FXP source refuses | ⬜ | |
| New tab: both sides remote | ⬜ | |
| New tab for quick connect / favourites | ✅ | "Open a new tab when the panel is busy" |

### Display

| Topic | Status | Notes |
|---|---|---|
| System tray: minimize, flash while transferring, always show | ⬜ | |
| Local window: full path in the address bar | ✅ | |
| Local window: show hidden files | ⬜ | Hidden files are listed today |
| File size display: bytes / KB / dynamic | 🟡 | Dynamic only |
| Flash the window when transfers end | ⬜ | |
| Refresh the size of a file being downloaded | ⬜ | |
| Favourites menu: split, history, quick connect entry, recent sites | ⬜ | |
| Gradient side indicator, hide selection without focus, name separator | ➖ | Cosmetic options of the Delphi interface |
| Language | 🟡 | English and French |
| Sort: remote and local modes | ✅ | Click on a column |
| Sort: links apart, descending by default for date and size | ⬜ | |
| Highlights (coloured masks) | ⬜ | |
| Fonts (application, lists, log) | ⬜ | |
| Colours (rows, log, progress bar), colour themes | 🟡 | Dark and light themes checked for contrast; no custom colours |
| Layout: tabs, several lines, tabs at the bottom, width | 🟡 | Tabs on one line at the top |
| Layout: caption areas of dock panels | ➖ | egui_dock tabs |
| Layout: tab switching for remote/local, large icons | ⬜ | |
| Layout: performance options | ➖ | Not relevant with the GPU renderer |
| Style (XP theme, flat, Office 2003...) | ➖ | Delphi toolkit styles |
| Status bar: queue times, progress bar format (`%p`, `%s`, `%e`, `%c`) | 🟡 | Tabs, connections and active queues; no times or progress text |

### Mouse action, associations, events, confirmations

| Topic | Status | Notes |
|---|---|---|
| Double-click actions (file, log, site, side indicator, tab, queue item) | 🟡 | Fixed: opens folders; no choice |
| Drag from Explorer to a remote panel | ⬜ | Drag and drop works between the two panels |
| Paste local files into a remote panel (`Ctrl+V`) | ⬜ | |
| Explorer context menu in the local panel | ⬜ | |
| File associations: view and edit remote files with external programs | ⬜ | |
| Event notification: sounds, tray pop-ups, event list | ⬜ | |
| Confirmations: delete local files | ✅ | Always asked |
| Confirmations: exit, shut down, close a tab with connections or queue, delete queue items | ⬜ | |

## Scripting

| Topic | Status | Notes |
|---|---|---|
| Script files `.PAC` / compiled `.PCC` | 🟡 | Source scripts (`-s`, console, mIRC); no compiled form |
| Identifiers, assignments, strings (`#nn`), comments | ✅ | |
| Variables (`var`, implicit) | ✅ | |
| Indexes (`Str[i]`, array properties) | ⬜ | |
| Arrays (constructors, multi-index) | ⬜ | |
| `if`, `while`, `repeat`, `for` | ✅ | |
| `case` | ⬜ | |
| User procedures and functions | ⬜ | Refused with a clear error today |
| `RushApp.Language` | ✅ | |
| `RushApp.UI.Bar`, `RushApp.UI.Menu` | ⬜ | |
| `RushApp.UI.Selection` (`Prepare`, `Count`, `Items`, `IsRemoteSide`, `IsFTPConnected`, `FTPName`) | ⬜ | |
| `RushApp.FTP.Splitter` | ✅ | |
| `RushApp.FTP.CWD`, `Delete`, `Login`, `Logout`, `RAW`, `RemoveQueue` | ✅ | With their flags |
| `RushApp.FTP.Transfer` | 🟡 | Flags parsed; `RS_NOCIRCLE`, `CompleteFlag`, `RepeatCount`, `Depth`, `SubCount`, sort and folder/file filters not applied yet |
| `RushApp.FTP.EnableSite`, `DisableSite` | ✅ | |
| Event `RushApp.UI.OnSwitchLayout` | ⬜ | Needs saved layouts |
| Statistics variables (`TotalDownloadSize`... `DownloadBW`) | ✅ | |
| `RushApp.DeleteEmptyTabs` | ✅ | |
| `MatchExpr`, `MatchString` | ⬜ | |
| `ShellExecuteCommand` | ⬜ | Needs a confirmation rule before running programs |
| Example: `InputQuery` for a bookmark | 🟡 | `InputQuery` accepted; no input dialog yet |
| `%F%` / `%D%` prefixes in transfer filters | ⬜ | |

## Advanced, FAQ and tips

| Topic | Status | Notes |
|---|---|---|
| Command line `-d`, `-w`, `-c`, `-f`, `-r[t][x]` | ✅ | |
| Command line `-b` (beta licence) | ➖ | No licence in UltraRushFTP; accepted and ignored |
| Wildcards `*` `?` (case-insensitive) | ✅ | |
| Regular expressions | 🟡 | Rust `regex` syntax, close to but not identical with TRegExpr |
| FAQ: cannot delete a folder (hidden files) | 🟡 | Needs the "show hidden files" setting (`LIST -a`) |
| FAQ: "Protocol not supported (10043)" | ➖ | No OpenSSL DLL: TLS is built in (rustls) |
| FAQ: multi-language file names | ✅ | UTF-8 |
| FAQ: limit connections for a site | ✅ | |
| Tip: `Shift` + connect = ghost login `!user` | ⬜ | `RS_GHOST` works in scripts |
| Tip: `Ctrl` + connect = `-password` | ⬜ | `RS_INVERT` works in scripts |
| Tip: `Shift` + bookmark = both sides | ⬜ | |
| Tip: `Shift` + double-click folder = both sides | ⬜ | |
| Tip: `%F%` / `%D%` in skip and allow lists of `Transfer` | ⬜ | |
| mIRC DLL (`rushmirc.dll`, `Invoke`, `RushScript`...) | ✅ | 32 and 64-bit |

## Not part of the help

Pages without features: welcome, acknowledgements, customer support, menu headings (8
pages). UltraRushFTP adds what FTPRush v2 did not have: dark theme, keyboard navigation and
screen-reader names checked by tests, DPAPI-protected passwords, imports from FileZilla
and FlashFXP, the `urftp` command-line tool.
