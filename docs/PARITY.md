# FTPRush v2 parity

**English** | [Français](PARITY.fr.md)

Every topic of the FTPRush v2 help file (235 pages, read in full on 2026-11-06) and where
UltraRushFTP stands. The help is used as a functional reference only; nothing is copied
from it and FTPRush itself is not decompiled.

Legend: ✅ done · 🟡 partly done · ⬜ planned · ➖ out of scope (reason given).

Summary (287 topics): ✅ 231 · 🟡 25 · ⬜ 13 · ➖ 18.

## Main window and menus

### File

| Topic | Status | Notes |
|---|---|---|
| New window | ✅ | New tab (`+`, `Ctrl+T`) |
| New window of current | ⬜ | Duplicate a tab with its connections |
| Close current window | ✅ | `✕`, `Ctrl+W` (disconnects) |
| Minimize to system tray | ✅ | Options › Notifications; a click on the icon brings the window back |
| Lock to system tray | ✅ | File › Lock, with the master password (Options › Security) |
| Backup / restore configuration | ✅ | File › Back up / Restore: the files of the data folder in `backup/<date>` |
| Exit | ✅ | |

### Edit and mark

| Topic | Status | Notes |
|---|---|---|
| Search (fast filter) | ✅ | `Ctrl+F`: words, `-word`, masks, regular expression; `F5` shows everything again |
| Rename | ✅ | `F2` |
| Delete | ✅ | `Del`, with confirmation |
| Make directory | ✅ | `F7` |
| Rename multiple files | ✅ | Model (`%n`, `%e`, `%c`), replace (regular expression), case, preview |
| Copy selected names | ✅ | `Ctrl+Shift+C`, one name per line |
| Mark / unmark, unmark all, select marked, export marks | ✅ | `Ctrl+M`; marked rows bold and underlined |
| Select (by mask) | ✅ | `+` / `-`, masks separated by `;` |
| Select all | ✅ | `Ctrl+A` |

### View

| Topic | Status | Notes |
|---|---|---|
| Refresh | ✅ | `F5` |
| Window list | 🟡 | Tab strip only; no list with connection and action columns |
| Queue window | ✅ | Dock panel |
| System log, transfer log, FTP log | ✅ | FTP, transfer and system logs of each tab, with times |
| Task window | ⬜ | Scheduled tasks |
| Remote columns size / date | ✅ | |
| Remote columns attributes / user / group | ✅ | Remote panels; local panels show the Windows attributes |
| Local columns size / date | ✅ | |
| Window list columns | ⬜ | |
| Grid lines | ➖ | Striped rows instead |
| Columns auto size | 🟡 | Columns resizable, no automatic fit |
| Current edit files | ✅ | FTP menu → Files being edited |

### FTP

| Topic | Status | Notes |
|---|---|---|
| Quick connect dialog | ✅ | Quick-connect bar |
| Attributes (CHMOD) | ✅ | `SITE CHMOD` (FTP), `SETSTAT` (SFTP), contents of the folders too |
| Move to | ✅ | Remote (`RNFR`/`RNTO`) and local |
| Modify date of FTP files | ✅ | `MFMT`, otherwise `SITE UTIME`; `SETSTAT` in SFTP |
| Calculate FTP space used | ✅ | Remote and local, with a depth |
| Export directory tree list | ✅ | Indent, sizes, folders only; copy or save |
| Find files at sites | ✅ | Words and masks, exclusions, kind, dates, sizes, depth; to the queue, copy the address |
| Clean site cache / all caches | ✅ | |

### Tools → Queue

| Topic | Status | Notes |
|---|---|---|
| Start / pause queue | ✅ | Pause interrupts the transfers, which resume from their partial file |
| Add the selected to queue | ✅ | Transfer button, `Space`, drag and drop |
| Conditional transfer | ✅ | Masks, dates, sizes, synchronisation, saved schemes |
| Clear current queue files | ✅ | ⋯ Queue → Clear the queue (except running) |
| Move up / down / top / bottom | ✅ | Right-click |
| Edit queue | ✅ | Source, target, mode; invert FXP |
| Reset all failed | ✅ | "Retry failed" |
| Reset selected failed | ✅ | Right-click → Retry |
| Clear all failed | ✅ | |
| Mark selected / all as failed | ✅ | Right-click on the selection, or ⋯ Queue |
| Queue information | ✅ | Counts, sizes, time left |
| Restore queue, load / save queue file | ✅ | Unfinished queue at startup; `.urq` files by hand |
| Schedule queue to task | ⬜ | |
| Limit upload / download speed toggles | 🟡 | Limits in the options; no one-click toggle |

### Tools → other

| Topic | Status | Notes |
|---|---|---|
| Site manager | ✅ | |
| Add to site manager | ✅ | `➕` next to Connect in the quick bar |
| Command (predefined site commands) | ✅ | FTP › Site commands; `%1` asks for a parameter |
| Command designer | ✅ | Options › Interface › Site commands |
| Send RAW command | ✅ | FTP › Send RAW command (`Ctrl+R`) |
| Compare folder contents (name, size, partial, hide same) | ✅ | FTP menu → Compare the panels |
| View raw directory | ✅ | FTP: as sent by the server; SFTP: `ls -l` lines |
| Script designer | 🟡 | Script console in each tab; no editor with files |
| Languages | 🟡 | English and French |
| Hotkey manager | ✅ | Options › Shortcuts |
| Manual download from site, increment URL list | ✅ | File › Manual download; `[1-12]` / `[01-12]` lists |
| Monitor clipboard, auto transfer | ✅ | Options › Interface › Clipboard |
| Transfer mode auto / binary / ASCII | ✅ | Options › Advanced transfer, and per item (Edit) |
| On transfer completed: disconnect / idle | ✅ | Options › Transfers |
| On transfer completed: shut down / standby / hibernate | ✅ | With a countdown that can be cancelled |
| On transfer completed: exit | ✅ | Options › Transfers, and `-rx` |
| Create SFV or MD5 file | ✅ | Right-click in a local panel |
| IP address translate | ⬜ | Hexadecimal / numeric address to dotted form |
| Restart UPnP service | ✅ | FTP menu, and Options › Connection |
| Option | ✅ | Options window |

### Help

| Topic | Status | Notes |
|---|---|---|
| Help index | 🟡 | `F1` lists the shortcuts; full documentation in `docs/` |
| FTPRush wizard | ➖ | Interface customisation wizard of the Delphi toolkit |
| Check new version | ➖ | No online update in this phase |
| About | ✅ | Help › About |

### Main interface and getting started

| Topic | Status | Notes |
|---|---|---|
| Main interface layout (panels, logs, queue, status bar) | 🟡 | Dock with panels, queue, three logs, script; saved layouts; no task window |
| Side indicator | 🟡 | `▸` on the active panel |
| Connect to FTP from the favourites menu | ✅ | `★ Favourites ▾` in the toolbar |
| Disconnect from site | ✅ | |
| Lock / unlock toolbar and dock | ➖ | The dock is rearranged by dragging; no toolbar to lock |
| Quick connect to a site | ✅ | `Tab` between fields, `Enter` connects |
| Customize main interface (styles) | ➖ | Delphi toolkit styles (XP, Office 2003...); dark and light themes instead |

## Dialogs

| Topic | Status | Notes |
|---|---|---|
| Add or edit skip list item | ✅ | Edited in the grid of the options |
| Bookmark dialog | 🟡 | Name, remote and local paths; date tokens (`%yyyy%`, `%mm%`...) not yet |
| Create CRC checksums | ✅ | SFV and MD5 files |
| Condition transfer | ✅ | See *Conditional transfer* |
| Calculate FTP space used | ✅ | |
| Edit queue | ✅ | |
| Export directory tree list | ✅ | |
| Fast search | ✅ | See *Search (fast filter)* |
| Find files at sites | ✅ | |
| Hotkey manager | ✅ | See *Hotkey manager* |
| Send RAW dialog (`Ctrl+R`, several lines, all sites) | ✅ | |
| Manual download | ✅ | |
| Increment URL | ✅ | |

## Site manager

| Topic | Status | Notes |
|---|---|---|
| Toolbar: properties, folder, new site, delete | ✅ | |
| Tree mode | ✅ | |
| List mode, column auto width | 🟡 | List mode, sortable; no automatic column width |
| Import plain text (one URL per line) | ✅ | Detected by the import |
| Import UltraFXP sites and settings | ⬜ | Not done: no sample file of the format |
| Import FTPRush XML | ✅ | Plus FTPRush v1/v3, FileZilla, FlashFXP; passwords are not imported |
| Export plain text / CSV / HTML / FTPRush XML | ✅ | Text, CSV, HTML, FTPRush v2 XML; never the passwords |
| Default site setting | ✅ | Options › New site: protocol, login, mode, limits, folders, listing |
| Security setting (password on the site book, show passwords) | ✅ | Master password (salted hash) for the site manager, the options and the lock; passwords protected by DPAPI |
| Test availability of sites | ✅ | One site or a whole folder, in parallel |

### Site properties

| Topic | Status | Notes |
|---|---|---|
| Protocol: plain FTP, `AUTH TLS`, implicit SSL, `AUTH SSL`, SFTP | ✅ | `AUTH SSL` as fallback of explicit FTPS |
| Several host:port entries | ✅ | Rotation on failure, the address that worked is used first |
| Port, user, password, anonymous | ✅ | |
| Passive mode, active fallback when `PASV` fails | ✅ | Fallback enabled on the Connection page |
| Server behind NAT (use the host address for `PASV`) | ✅ | Option of the site; also automatic when `PASV` returns a private address |
| UTF-8 login, forced UTF-8 | ✅ | Per site (FTP) |
| Time zone | ✅ | Server time zone (UTC−12 to +14) or offset added to the listing dates (FTP) |
| Maximum simultaneous transfers (0 = browsing connection) | ✅ | |
| Remote and local paths | ✅ | |
| Description (short text for menus, notes) | ✅ | Shown next to the name in the site tree |
| Bookmarks | ✅ | |
| HTTP URL mapping, copy HTTP URL | ✅ | Remote panel: right-click → Copy HTTP address |
| Display: site name instead of [1]/[2] in the log | ✅ | Per site, or for every site in Options › Interface |
| Connection: global or custom retries and timeouts | ✅ | |
| Resume support: auto / yes / no | ✅ | Per site |
| Keep alive per site | ✅ | |
| `FEAT` per site | ✅ | Capabilities used chosen per site (`PRET`, UTF-8, `MODE Z`, `XCRC`, `MLSD`, `SSCN`, `CPSV`) |
| `PRET` | ✅ | Used when announced |
| UTF-8 | ✅ | `OPTS UTF8 ON` when announced |
| `CLNT` | ✅ | Client name per site |
| Compression `MODE Z` | ✅ | Global setting (Advanced transfer), switched off per site |
| `XCRC` check after transfer | ✅ | Global setting (Advanced transfer), switched off per site |
| `MLSD` | ✅ | |
| `SSCN` / `CPSV` for encrypted FXP | ✅ | |
| Account: different logins for uploads and downloads | ✅ | With an address and port of their own; browsing keeps the main login |
| Ident: global or custom name | ✅ | Answered while the site connects |
| Proxy: global, bypass, custom; data channel | ✅ | |
| List mode: `STAT -l` | ✅ | Read on the control channel |
| List mode: trim leading spaces | ✅ | Always done for `STAT` |
| List mode: custom list command | ✅ | |
| List mode: UPnP | ✅ | Global option (Options › Connection): listings in active mode use it too |
| Directory cache per site | ✅ | Global setting, or yes / no in the site record |
| Directory charset (non-UTF-8 servers) | ✅ | Commands, replies and listings; 14 character sets |
| Listing parser choice | ✅ | Automatic, Unix or DOS/IIS (`MLSD` is always read as such) |
| SSL: `PBSZ 0` | ✅ | Can be switched off per site |
| SSL: encrypt listings / transfers separately | ✅ | |
| SSL: encrypted FXP | ✅ | Automatic, never or always per site |
| SFTP: private keys | ✅ | Several keys, tried in turn |
| Transfer: file-name case conversion | ✅ | Upload, download and FXP; global or per site |
| Transfer: XDupe | ✅ | Global or per site, with the refresh when XDUPE returns nothing |
| Transfer: `NOOP` during the transfers | ✅ | Global or per site |
| Transfer: FXP method (no `PASV` as source, `PASV` as target) | ✅ | Per site; automatic reversal when `PORT` is refused |
| Transfer: force active mode | ✅ | Transfer mode of the site record; for every site in Advanced transfer |
| Transfer: TCP buffer size | ✅ | Send and receive buffers, global and per site |
| Skip list, priority, file-exists rules per site | ✅ | "Existing file" rules per direction (upload, download, FXP), also globally |
| Skip list export / import | ✅ | JSON, or a text file with one mask per line |
| Event notification: monitor folders for new items | ✅ | In the site record or Options › Notifications; a connection of its own at each check |
| Statistics of this server | ✅ | Last access, connections, sent / received / FXP; copy and reset of the totals |

## Options

### General

| Topic | Status | Notes |
|---|---|---|
| E-mail address for anonymous login | ✅ | |
| Default download path | ✅ | Local panels start there |
| Data folder path | ✅ | `-f`, `URUSHFTP_DATA` |
| Daily backup of the configuration | ✅ | Options › Security; 4 kept |
| Cache FTP directories, update cache of transferring folder | ✅ | Options › Browsing |
| Check new version at startup | ➖ | No online update in this phase |
| Single instance | ✅ | Options › Security |
| Splash screen | ➖ | The window opens immediately |
| Log buffer size | ✅ | Options › Interface |
| Log to disk | ✅ | One file per day and per log |
| Batched log updates | ➖ | The interface already redraws at most once per frame |
| Thread pool | ➖ | Asynchronous runtime (tokio); nothing to tune |
| UPnP | ✅ | Active-mode ports and Ident port 113 opened on the router, public address announced |

### Connection

| Topic | Status | Notes |
|---|---|---|
| Send `FEAT` | ✅ | Per site (site record) |
| Connect timeout, reply timeout | ✅ | |
| Max retries, retry delay | ✅ | |
| Cycle the server IP list on login failure | ✅ | Other addresses of the site record |
| Auto reconnect on matching server replies | ✅ | Wildcard masks, Keep alive page |
| Smart handling of servers behind NAT | ✅ | |
| Bind sockets to an external address (active mode) | ✅ | |
| Bind to a local IP address | ✅ | Direct connections (not the proxy one) |
| Limit local port range | ✅ | |
| Ident: enable, name, system, port | ✅ | |
| Ident: only while connecting | ✅ | Other requests get `NO-USER` |
| Keep alive: command, interval, stop after, log out, reconnect when kicked, random command | ✅ | Global or per site |
| Proxy: list of named proxies | ✅ | Proxy page; chosen by name in the site records |
| Proxy: `NOOP` during transfers, passive with proxy | ✅ | `NOOP`: Connection page; passive mode always used through a proxy |
| Proxy: bypass for local addresses, exceptions | ✅ | |
| Proxy editor | ✅ | Inline in the options and the site record |
| List mode: show hidden files (`LIST -a`) | ✅ | |
| List mode: custom command | ✅ | Global or per site |
| List mode: switch to `PORT` when `PASV` fails | ✅ | For the listings and the transfers |
| List mode: use `MLSD` if available | ✅ | |

### Transfer

| Topic | Status | Notes |
|---|---|---|
| Download / upload speed limits | ✅ | |
| Bypass speed limit for local addresses | ✅ | Private, loopback and link-local addresses |
| File-name case conversion | ✅ | Uploads and downloads separately |
| Force active mode for transfers | ✅ | |
| Stop and resume when no data is received | 🟡 | Reply timeout then retry with resume |
| Roll back when resuming a download | ✅ | |
| Keep free disk space | ✅ | |
| Maximum simultaneous transfers per tab | ✅ | |
| TCP buffer size | ✅ | |
| Transfer mode auto / binary / ASCII | ✅ | See the menu row |
| On transfer completed: idle / disconnect | ✅ | |
| ASCII list, no resume for ASCII files | ✅ | |
| Skip list: enable, zero-byte files, empty folders, folders | ✅ | |
| Skip list: colour of skipped items | ⬜ | |
| Skip list: mode auto / manual / both | ⬜ | Applied to every item today |
| File-exists rules (resume, skip, rename, overwrite, ask) | ✅ | Plus overwrite when resume fails |
| Queue: reload unfinished queue at startup, auto save | ✅ | Ask, load or discard (Options › General) |
| Queue: keep failed items | ✅ | |
| Queue: cycle retry (failed item goes to the bottom) | ✅ | |
| Queue: retry all failed after the queue ends | ✅ | A number of rounds |
| Queue: clear when switching server manually | ➖ | No manual server switching between tabs |
| Queue: reset failed when started automatically | ✅ | |
| Queue: clear when logging out a transferring site | ✅ | Its waiting items are removed |
| Queue: source / destination in the transfer log | ✅ | |
| Queue: retries per direction (upload, download, FXP) | ✅ | A site setting still wins |
| Priority list | ✅ | |
| Compression `MODE Z` (listing, upload, download, exceptions) | 🟡 | Uploads and downloads, with exceptions; listings are not compressed |
| Disk cache for downloads | ➖ | Buffered writes are handled by the runtime |

### Misc.

| Topic | Status | Notes |
|---|---|---|
| Check SFV when downloading, CRC log and status folder, re-add bad files | 🟡 | Check and re-add; results in the log, no status folder |
| Use `XCRC` if available | ✅ | |
| Enter a folder after creating it | ✅ | |
| Remove empty folders left by a transfer | ✅ | |
| Refuse a folder with the same name as its parent | ✅ | |
| One SFV per folder, skip all SFV if several | ✅ | |
| Keep the server date on downloaded files | ✅ | |
| Remove the 0-byte target file when an FXP source refuses | ✅ | |
| New tab: both sides remote | ⬜ | |
| New tab for quick connect / favourites | ✅ | "Open a new tab when the panel is busy" |

### Display

| Topic | Status | Notes |
|---|---|---|
| System tray: minimize, flash while transferring, always show | 🟡 | Minimize and always show; the button flashes when a queue ends, not during the transfers |
| Local window: full path in the address bar | ✅ | |
| Local window: show hidden files | ⬜ | Hidden files are listed today |
| File size display: bytes / KB / dynamic | 🟡 | Dynamic only |
| Flash the window when transfers end | ✅ | When the window is in the background |
| Refresh the size of a file being downloaded | ⬜ | |
| Favourites menu: split, history, quick connect entry, recent sites | 🟡 | Recent sites and every site; no split menu, no history of quick connections |
| Gradient side indicator, hide selection without focus, name separator | ➖ | Cosmetic options of the Delphi interface |
| Language | 🟡 | English and French |
| Sort: remote and local modes | ✅ | Click on a column |
| Sort: links apart, descending by default for date and size | 🟡 | Largest / newest first, folders first or not; links are not apart |
| Highlights (coloured masks) | ✅ | A colour per theme, contrast shown |
| Fonts (application, lists, log) | 🟡 | Size of the whole interface and of the logs; no choice of font |
| Colours (rows, log, progress bar), colour themes | 🟡 | Themes checked for contrast; own log and highlight colours with their contrast; no progress bar colour |
| Layout: tabs, several lines, tabs at the bottom, width | 🟡 | Tabs above or below; one line |
| Layout: caption areas of dock panels | ➖ | egui_dock tabs |
| Layout: tab switching for remote/local, large icons | ⬜ | |
| Layout: performance options | ➖ | Not relevant with the GPU renderer |
| Style (XP theme, flat, Office 2003...) | ➖ | Delphi toolkit styles |
| Status bar: queue times, progress bar format (`%p`, `%s`, `%e`, `%c`) | ✅ | `%p` `%b` `%s` `%e` `%c` |

### Mouse action, associations, events, confirmations

| Topic | Status | Notes |
|---|---|---|
| Double-click actions (file, log, site, side indicator, tab, queue item) | 🟡 | Double-click on a file: transfer, view or nothing; the others are fixed |
| Drag from Explorer to a remote panel | ✅ | Into the folder of the remote panel under the pointer |
| Paste local files into a remote panel (`Ctrl+V`) | ✅ | |
| Explorer context menu in the local panel | ⬜ | |
| File associations: view and edit remote files with external programs | ✅ | Options › Browsing; Notepad when no program matches; an edited file is sent back when saved |
| Event notification: sounds, tray pop-ups, event list | ✅ | Options › Notifications |
| Confirmations: delete local files | ✅ | Always asked |
| Confirmations: exit, shut down, close a tab with connections or queue, delete queue items | ✅ | Exit, close a busy tab, remove queue items; shut down has its countdown |

## Scripting

| Topic | Status | Notes |
|---|---|---|
| Script files `.PAC` / compiled `.PCC` | 🟡 | Source scripts (`-s`, console, mIRC); no compiled form |
| Identifiers, assignments, strings (`#nn`), comments | ✅ | |
| Variables (`var`, implicit) | ✅ | |
| Indexes (`Str[i]`, array properties) | ✅ | Strings from 1, arrays from 0, `A[i, j]`; `Selection.Items[i]` |
| Arrays (constructors, multi-index) | ✅ | `[…]`, nested, `in`, `SetLength`, `Length`, `Low`, `High`; no `VarArrayCreate` |
| `if`, `while`, `repeat`, `for` | ✅ | |
| `case` | ✅ | Labels of any type, ranges |
| User procedures and functions | ✅ | `var` parameters, `Result`, recursion; not nested |
| `RushApp.Language` | ✅ | |
| `RushApp.UI.Bar`, `RushApp.UI.Menu` | ➖ | The help describes no member of them: a script using them gets a clear error |
| `RushApp.UI.Selection` (`Prepare`, `Count`, `Items`, `IsRemoteSide`, `IsFTPConnected`, `FTPName`) | ✅ | `Date` is text |
| `RushApp.FTP.Splitter` | ✅ | |
| `RushApp.FTP.CWD`, `Delete`, `Login`, `Logout`, `RAW`, `RemoveQueue` | ✅ | With their flags |
| `RushApp.FTP.Transfer` | 🟡 | Every parameter but `RS_CHECKLOGIN`; `Timeout` is unused, as in FTPRush |
| `RushApp.FTP.EnableSite`, `DisableSite` | ✅ | |
| Event `RushApp.UI.OnSwitchLayout` | ✅ | When a saved layout is loaded |
| Statistics variables (`TotalDownloadSize`... `DownloadBW`) | ✅ | |
| `RushApp.DeleteEmptyTabs` | ✅ | |
| `MatchExpr`, `MatchString` | ✅ | Case-insensitive, like the transfer filters |
| `ShellExecuteCommand` | ✅ | Asked each time by default (Options › Security); `Flag` not used |
| Example: `InputQuery` for a bookmark | ✅ | Input box; scripts run beside the interface |
| `%F%` / `%D%` prefixes in transfer filters | ✅ | |

## Advanced, FAQ and tips

| Topic | Status | Notes |
|---|---|---|
| Command line `-d`, `-w`, `-c`, `-f`, `-r[t][x]` | ✅ | |
| Command line `-b` (beta licence) | ➖ | No licence in UltraRushFTP; accepted and ignored |
| Wildcards `*` `?` (case-insensitive) | ✅ | |
| Regular expressions | 🟡 | Rust `regex` syntax, close to but not identical with TRegExpr |
| FAQ: cannot delete a folder (hidden files) | ✅ | Connection page › Show the hidden files |
| FAQ: "Protocol not supported (10043)" | ➖ | No OpenSSL DLL: TLS is built in (rustls) |
| FAQ: multi-language file names | ✅ | UTF-8 |
| FAQ: limit connections for a site | ✅ | |
| Tip: `Shift` + connect = ghost login `!user` | ✅ | Site manager; `RS_GHOST` in scripts |
| Tip: `Ctrl` + connect = `-password` | ✅ | Site manager; `RS_INVERT` in scripts |
| Tip: `Shift` + bookmark = both sides | ✅ | Bookmarks menu of the remote panel |
| Tip: `Shift` + double-click folder = both sides | ✅ | |
| Tip: `%F%` / `%D%` in skip and allow lists of `Transfer` | ✅ | |
| mIRC DLL (`rushmirc.dll`, `Invoke`, `RushScript`...) | ✅ | 32 and 64-bit |

## Not part of the help

Pages without features: welcome, acknowledgements, customer support, menu headings (8
pages). UltraRushFTP adds what FTPRush v2 did not have: dark theme, keyboard navigation and
screen-reader names checked by tests, DPAPI-protected passwords, imports from FileZilla
and FlashFXP, the `urftp` command-line tool, IPv4/IPv6 "happy eyeballs" connections and
FXP between IPv6 servers (`EPSV` / `EPRT`).
