# Options reference

**English** | [Français](OPTIONS.fr.md)

Every setting of UltraRushFTP, with its default value. Contents:
[interface](#interface) · [Options window](#options-window) · [site record](#site-record) ·
[command line](#command-line) · [`urftp` tool](#urftp-tool) ·
[scripts](#scripts-rushapp) · [mIRC DLL](#mirc-dll) ·
[environment variables](#environment-variables) · [files](#files)

## Interface

### Menus and top bar

| Element | Effect |
|---|---|
| File → Site manager | Opens the site manager |
| File → New tab | New empty tab (also `+` next to the tabs) |
| File → New tab of current | New tab with the layout, the local folders and the connections (same sites, same folders) of the tab shown, without its queue (`Ctrl+Shift+T`, or right-click on a tab) |
| File → Options… | Opens the Options window (also `⚙`) |
| File → Manual download… | See *Manual download* |
| File → Back up the configuration / Restore a backup | See *Backups* |
| File → Lock | Locks the window until the master password is given (Options › Security) |
| File → Quit | Quits |
| Connection → *site* | Connects the active panel (marked `▸`) to a site of the site manager; groups are sub-menus; `Shift`: ghost login, `Ctrl`: `-password` |
| Connection → Site manager | Opens the site manager |
| Connection → Reconnect / Disconnect | Acts on the connection of the active panel |
| Connection → Local / *site* | Switches the active panel between its local folder and its connection |
| Edit → Quick filter, Select all, Select / Unselect by mask, Copy the names, Marks, Rename several, Move to, Paste files | Act on the active panel (see *File panels*) |
| FTP → Permissions, Change the date, View the raw listing | Act on the active remote panel |
| FTP → Space used, Export the directory tree, Find files at sites, Compare the panels | See *Folder tools* |
| FTP → Clear the cache of this site / every cache, Files being edited | See *Folder tools* |
| FTP → Send RAW command (`Ctrl+R`), Site commands | See *RAW commands* |
| FTP → IP address translate… | An address written in hexadecimal (`C0A80101`, `0xC0A80101`) or as one number (`3232235777`) in dotted form, with a Copy button |
| FTP → Restart UPnP service | Searches the router again (Options › Connection) |
| View → Window list / Task window | Shows or hides that pane in the tab shown (next to the queue); see *Window list and Task window* |
| View → Window list columns | FTP and Action columns of the window list |
| View → Layouts | Apply, save or delete a layout of the panels; back to the default layout; *Panels as tabs*: both file panels in one area, their titles switch between them |
| View → Light theme / Dark theme | Switches the theme (also stored in the options) |
| Help → Help (F1) | This documentation, built in: index of the sections, search, keyboard shortcuts |
| Help → Keyboard shortcuts | The help on the shortcuts below |
| Help → Check for updates | Asks GitHub at once for the latest version and shows the answer |
| Help → About UltraRushFTP | Version, build and licence |
| `★ Favourites ▾` | Recent sites (10), quick connections (10, without their password: an anonymous one connects at once, else the password field waits), every site of the site manager, *Manual download*: connects the active panel. Split (options): `★ <last site>` connects to it, `⏷` opens the menu |
| `➕` after Connect | Saves the quick connection as a site of the site manager (with its password) |
| Quick connect | Protocol (FTP, FTPS explicit, FTPS implicit, SFTP), host, port, user, password, **Connect**: opens the site in the right panel of the current tab (or in a new tab, see *General*) |
| Tabs | One tab per site container; `✕` closes it, the title is the host or "Tab n"; `Ctrl+T` new, `Ctrl+Shift+T` new of current, `Ctrl+W` close, `Ctrl+Tab` / `Ctrl+Shift+Tab` next / previous |

### File panels

Each panel has its own connection bar, above its folder:

| Button | Effect |
|---|---|
| `🔌 Connect ▾` | Menu of the sites of the site manager (groups as sub-menus) and *Site manager*: connects **this** panel; `Shift`: ghost login, `Ctrl`: `-password`. The local folder of the site, when set, goes to the other panel if it is local. |
| `⟲ Reconnect` | Opens the connection of the panel again, in the same folder |
| `Disconnect` | Closes the connection of the panel (shown, or kept behind the local folder) |
| `Bookmarks ▾` | Bookmarks of the site record (remote panel): goes to the remote folder; `Shift`: the local folder of the bookmark in the other panel too. Date tokens are replaced by today's date: `%yyyy%` / `%yy%` year, `%mm%` / `%m%` month, `%dd%` / `%d%` day, `%ww%` / `%w%` ISO week |
| `💻 Local` / `🌐 site` | Switches the panel between its local folder and its connection; the connection stays open meanwhile, and the tab keeps its name |

In a remote panel, the path field takes a folder (absolute, or relative to the current
one): `Enter` goes there.

| Action | Mouse | Keyboard |
|---|---|---|
| Open a folder | double-click | `Enter` |
| Parent folder | `⬆` | `Backspace` |
| Refresh | `⟳` | `F5` |
| New folder | `📁+` | `F7` |
| Rename | right-click → Rename | `F2` |
| Delete (with confirmation) | right-click → Delete | `Del` |
| Transfer to the other panel | `Transfer` button, drag and drop, right-click | `Space` or `Insert` |
| Switch panel | click | `F6` |
| Move the cursor | — | arrows, `Page Up` / `Page Down`, `Home` / `End` |
| Go to a name | — | type its first letters (a pause of one second starts a new search; the same letter again goes to the next name) |
| Select all / several | `Ctrl`+click, `Shift`+click, right-click › *Select all* | `Ctrl+A`, `Shift`+movement, `Ctrl+Space` (row at the cursor) |
| Copy the full path | right-click → Copy path | — |
| Copy the web address | right-click → Copy HTTP address (remote panel of a site with a web address) | — |
| Conditional transfer | right-click → Conditional transfer… (see below) | — |
| SFV or MD5 file of the selected files | right-click → Create an SFV file / Create an MD5 file (local panel) | — |
| Sort | click a column header (again: reverse order) | — |
| Quick filter: only the names with every word, `-word` leaves out, `*.mask`, `"-word"` as it is, or a regular expression | Edit → Quick filter | `Ctrl+F`; `Esc` closes it, `F5` shows everything again |
| Mark / unmark (bold and underlined) | right-click → Mark / unmark; Edit → Marks (unmark all, select the marked, copy their paths) | `Ctrl+M` |
| Select / unselect by mask (`*.rar;*.r??`) | Edit menu | `+` / `-` |
| Copy the names of the selection, one per line | right-click → Copy the names | `Ctrl+Shift+C` |
| Rename several items: model `%f` (name), `%n` (without extension), `%e` (extension), `%c` (counter, start, step, digits), replace (text or regular expression, `$1`/`${1}` groups), case, preview | right-click → Rename several… | — |
| Move to a folder (relative or absolute; the other panel's folder is offered when it shows the same site) | right-click → Move to… | — |
| Permissions (CHMOD): boxes or octal value, contents of the folders too | right-click → Permissions (remote) | — |
| Change the date (UTC; `MFMT`, otherwise `SITE UTIME`; `SETSTAT` in SFTP) | right-click → Change the date (remote) | — |
| View / edit a remote file with its program (see *Browsing*) | double-click a remote file (view); right-click → View / Edit | `F3` / `F4` |
| Open a folder in both panels | `Shift`+double-click | — |
| Send files from Explorer into the remote panel | drag them onto the panel | `Ctrl+V` after copying them in Explorer |

The file-list keys act on the active panel (marked `▸`, cursor outlined) when no control
has the keyboard focus. `Tab` / `Shift+Tab` move between the controls of the window and
`Esc` gives the keys back to the list.

![Keyboard shortcuts](images/shortcuts.en.png)

Remote panels show the permissions, owner and group columns; local panels the Windows
attributes (`R` read-only, `H` hidden, `S` system, `A` archive).

The SFV and MD5 files are named after the folder (`folder.sfv`, `folder.md5`) and written
in it, in the background; the log says when they are ready.

### Folder tools

| Tool | Effect |
|---|---|
| Compare the panels | Options checked in the sub-menu: *same name* or *same name and size* (folders: name only), *ignore the extensions*, *hide what is the same*, *select what differs*; then *Compare now*. `F5` shows everything again |
| Space used | Files, folders and size of a folder (remote or local), and of each folder it contains; *Depth* 0 = every level, 1 = only the folders inside... |
| Export the directory tree | The tree as text (indent, sizes, folders only), to copy or save |
| Find files at sites | Words separated by `;` (a word with `*` or `?` is a mask of the whole name, the others are looked for inside the names), words to leave out, folder, depth, kind, dates (`YYYY-MM-DD`), sizes (KiB); the sites ticked are searched one after the other, each with its own connection. Right-click on the results: add them (or their folder) to the queue, towards the local panel of the tab, or copy their `ftp://` address |
| View the raw listing | The listing of the folder as the server sent it (FTP), or as `ls -l` lines (SFTP) |
| Clear the cache | Forgets the folders kept by the directory cache (see *Browsing*) |
| Files being edited | Remote files opened with a program: state, *Send back now*, *Stop watching* |

The walks and searches use a connection of their own: the panel stays free meanwhile.

### Logs and status bar

Each tab has three logs: **FTP** (commands, replies, connections of the panels),
**Transfers** (queue, items, CRC checks) and **System** (files written, local
operations, scripts, notifications). Lines show their time and the site (or Left / Right),
see *Interface*. With *Write the logs to files*, each log goes to
`<folder>\<date>-ftp.log`, `-transfer.log`, `-system.log`.

While a queue runs, the status bar shows its progress bar with the text of the
*Interface* option (`%p` percent, `%b` bytes, `%s` speed, `%e` time left, `%c` items).

The **⬇** and **⬆** buttons at the left of the status bar show the download and upload
limits of Options › Speed and turn each one on or off in one click (`∞`: unlimited),
immediately, running transfers included. They are greyed out while no limit is set.

### RAW commands

**FTP → Send RAW command** (`Ctrl+R`): several commands, one per line, sent to the
active panel, to both panels or to every connected site; `Ctrl+Enter` sends. The replies
are listed below the commands and in the FTP log.

**FTP → Site commands**: the commands of the designer (*Options › Commands*, `SITE WHO`,
`SITE NEW`...), in their folders; a command that asks questions opens a dialog first.

### Manual download

**File → Manual download** (also in the favourites): addresses, one per line —
`ftp://user:password@host:port/path`, `ftps://` (explicit), `ftpsi://` (implicit),
`sftp://`; a path ending with `/` is a folder. `[1-12]` or `[01-12]` makes a list
(`file[01-12].rar`). The items go to the queue of the tab, into the chosen folder. With
the clipboard watch (*Interface*), addresses copied anywhere open this window, or are
queued at once.

### Conditional transfer

Right-click → *Conditional transfer…* queues the selection towards the other panel with
conditions:

| Field | Effect |
|---|---|
| Folder names / File names, Include and Exclude | Masks separated by `;` (wildcards, or regular expressions with *Use regular expressions*); `%F%` before a mask makes it apply to files only, `%D%` to folders only |
| Date between | First and last day (`YYYY-MM-DD`, the last day included) |
| Not older than | Number of days |
| File size between | In KiB (maximum 0: no limit) |
| Apply the dates to folders too | Otherwise the dates only filter the files |
| Do not use the global skip list | The skip list of the options is ignored |
| Include sub-folders | Otherwise only the files of the selected folders |
| Log out once transferred | The panels of the tab are logged out when the queue is finished |
| Synchronise | Only what differs is transferred: a missing file, a different size (*Compare sizes*), a newer source (*Compare dates*, more than a minute); *Delete what the source does not have* removes from the target what the source lacks; *Existing files only* updates the files the target already has; *Both sides* copies back what only the target has |

The conditions apply to the selected items and to the contents of the folders. *Scheme*
saves the form under a name (*Save*, *Delete*) in `settings.json`. **Begin transfer**
queues and starts, **Add to queue** only queues.

### Accessibility

- Every control has a name for screen readers (AccessKit), including symbol buttons and
  fields that only show a placeholder; file rows are read as "name, size, date".
- Both themes meet WCAG 2.2 AA contrast (4.5:1 for text, 3:1 for focus and state
  indicators) and pointer targets are at least 24 px or spaced as WCAG 2.5.8 allows; tests
  check both.
- States are never shown by colour alone (symbols, `⚠`, written warnings).
- Confirmations of irreversible actions start on **Cancel**.

### Queue

Buttons: **Start** (**Resume** after a pause), **Pause**, **Stop all**, **Retry failed**,
**Remove finished**, and the **⋯ Queue** menu: *Queue information…*, *Mark all as failed*,
*Remove the failed items*, *Clear the queue (except running)*, *Load a queue file…*, *Save
the queue to a file…*, *Schedule queue to task…* (see *Window list and Task window*).

- **Pause** interrupts the running transfers: they wait again and resume from their partial
  file on *Resume*. **Stop all** stops the running items and ends the queue; the others
  stay waiting.
- **Rows**: click selects, `Ctrl`+click adds or removes a row, `Shift`+click selects a
  range. The right-click menu acts on the selection when the row is part of it: stop, retry,
  *Edit…* (one item: source and target site and path, transfer mode, *Invert FXP*),
  *Invert FXP* (site-to-site items), *Mark as failed*, move to top / up / down / to bottom,
  remove.
- **Queue information**: items, downloads / uploads / FXP, counts per state, total,
  transferred and left sizes, time left at the current speed.
- **Queue files** (`.urq`): the unfinished items of the tab, in the format of `queue.json`;
  loading adds the items of the file (or of a `queue.json`) to the queue of the tab.

States: waiting, running, done, skipped (with the reason), needs confirmation, stopped,
failed.

![Unfinished queue](images/queue-restore.en.png)

The items left to do are saved in `queue.json` (every two seconds when the queue changes,
and when the application closes), tab by tab. At the next start they are offered again (see
*Unfinished queue at startup*): one line per tab, with its title, the number of items left,
the sites and the first item; untick the queues you do not want, the others each come back
in a tab of their own. Running, stopped and to-confirm items come back waiting, failed ones stay
failed, done and skipped ones are dropped. **Start** resumes the queue; a partial file is
resumed according to the *Existing file* rules. The items must use sites of the site
manager: a quick connection is not kept.

### Window list and Task window

![Window list and Task window](images/tasks.en.png)

Two panes added from the **View** menu, in the tab shown (they can be moved like the other
panes and are kept by the saved layouts).

**Window list**: one row per tab: its number and title, what each panel shows (local
folder, or `site:folder`), and the optional columns **FTP** (connections of the tab:
*2 connected*, *1 of 2 connected*) and **Action** (*Transferring (3 running, 12 waiting)*,
*Paused*, *5 waiting*, *Idle*), chosen in View → Window list columns. A click on a row
shows that tab; `✕` closes it (with the confirmation of a busy tab).

**Task window**: the scheduled tasks, with their state (on / off), what they do, when, the
next and the last run. `▶` runs a task now, `✎` opens its dialog.

| Task setting | Values |
|---|---|
| Name, On | Free text; an inactive task keeps its settings and never runs |
| When | **Once** at a date and time (`YYYY-MM-DD HH:MM`); **Every N minutes** (1 to 10,080) from a first time; **Days of the week** at a time (`HH:MM`), on the days ticked |
| Queue task | Created by ⋯ Queue › *Schedule queue to task…* with the unfinished items of the tab; when it runs, the queue opens in a **new tab** and starts (the tab shown does not change); *Close the tab once the queue is finished* |
| Script task | **New script task…**: Pascal Script run like the console (`ShellExecuteCommand` follows Options › Security) |

Times are those of the computer clock. A time that passes while UltraRushFTP is closed is
**not run late**: the task shows *Missed* and waits for its next time (a one-time task turns
off). The tasks are saved in `tasks.json`.

## Options window

Saved automatically in `settings.json` one second after a change.

### General

![General](images/options-general.en.png)

| Option | Default | Effect |
|---|---|---|
| Dark theme | on | Dark or light theme |
| Language | Windows language | Windows language, English or Français; applied immediately |
| Open a new tab when the right panel is already connected | on | Otherwise the new connection replaces the current one |
| Both panels for sites | off | A second connection goes to the left panel (when it shows a local folder) before a new tab is opened: two sites face to face for FXP |
| E-mail for anonymous logins | `urftp@` | Password sent by the sites set to *anonymous* |
| Default download folder | user folder | Folder the local panels start in, and come back to on disconnection; the user folder when it does not exist |
| Unfinished queue at startup | Ask | Ask, Load it or Discard it (see *Queue*) |
| Check for updates (every day / once a week / once a month) | on, once a week | A minute after the start, when that interval has passed, asks GitHub for the latest version (only the program name and version are sent). A newer one opens a window: *Open the release page*, *Later*, or *Skip this version* (not announced again). Nothing is downloaded or installed by itself; the answer is also written in the log |
| Compatibility with the mIRC DLL of FTPRush (TfmRush window) | off | See *mIRC DLL*; while it is on, FTPRush v2 cannot start next to UltraRushFTP |

### Transfers

![Transfers](images/options-transfers.en.png)

| Option | Default | Effect |
|---|---|---|
| Retries after a failure | 3 | Attempts after the first failure of an item |
| Delay between retries (s) | 3 | Wait before a retry |
| Simultaneous transfers in total | 0 (no limit) | Limit across all sites, on top of the per-site limits |
| Simultaneous transfers for quick connect | 2 | Per-site limit used by quick connect |
| Connection timeout (s) | 30 | Control and data connections |
| Server idle timeout (s) | 120 | Maximum wait for a reply or a data block |
| Stop and resume after no data for | 0 s (the timeout above) | A transfer that receives or sends nothing for this time is stopped and resumed by the retries (FTP and SFTP) |
| Downloads / Uploads / FXP: own number of retries | off | Retries of that direction instead of the general value (a site setting still wins) |
| Retry the failed items when the queue ends (rounds) | 0 | Once the queue is finished, the failed items are tried again this many times |
| Cycle retry | off | An item to retry goes to the bottom of the queue, the others run first, without delay |
| Retry the failed items when a queue starts automatically | off | `-r` at startup |
| Logging out a site removes its waiting items | off | *Disconnect* of a panel |
| Source and target of each item in the log | off | A line when an item starts, ends or is paused |
| When every queue is finished | Do nothing | Do nothing, Log out the sites, Quit UltraRushFTP, Sleep, Hibernate, Shut the computer down |
| Seconds to cancel | 30 | Quitting, sleeping, hibernating and shutting down are asked first with this countdown (*Cancel*, *Do it now*) |

### Speed

| Option | Default | Effect |
|---|---|---|
| Download (KiB/s) | 0 (unlimited) | Shared by every download, applied immediately |
| Upload (KiB/s) | 0 (unlimited) | Shared by every upload, applied immediately |
| Apply the download limit / the upload limit | on | Also the ⬇ / ⬆ buttons of the status bar: a limit turned off keeps its value |

FXP is not limited: its data does not go through this computer.

### Existing file

![Existing file](images/options-existing-file.en.png)

Three sets of rules: **Upload**, **Download** and **FXP (FTP to FTP)**.

| Case | Default | Choices |
|---|---|---|
| The target is smaller | Resume | Resume, Skip, Rename ("name (1)"), Overwrite, Ask |
| The target has the same size | Skip | same |
| The target is larger | Rename | same |
| Start over if resuming is refused | on | Overwrite when the server refuses `REST` (every direction) |

The rules of an older version apply to the three directions.

### Skipped files

![Skip list](images/options-skiplist.en.png)

| Option | Default | Effect |
|---|---|---|
| Use the skip list | on | Enables the rules below |
| Skip 0-byte files | off | — |
| Do not create empty folders | off | — |
| Applies to | Both | **Contents of folders (auto)**: the items you choose are always transferred, the rules filter what is found inside the folders; **Items chosen (manual)**: the rules check the items you choose, not the contents of the folders; **Both** |
| Show the items the skip list would skip in this colour | on, `#9AA0A6` (dark) / `#5F6368` (light) | In the panels, also struck through and told by screen readers ("skipped by the skip list"); the contrast is shown |
| Rules | none | Applied in order, the first match decides: **Skip** or **Allow**, mask (wildcard `*` `?`, or regular expression with *regex*), target (files, folders, both), directions (upload ⬆, download ⬇, FXP ↔), *path* = the mask applies to the full path (also written `%W%mask`) |
| Export… / Import… | — | The rules as a JSON file; *Import* also reads a text file with one mask per line (lines starting with `#` ignored) and adds the rules |

### Priorities

Ordered list of masks (`*.sfv`, `*.nfo` by default): matching items move to the top of the
queue, the first mask having the highest priority.

### Proxy

![Proxy](images/options-proxy.en.png)

Used by the sites set to *Global setting*.

| Option | Default | Effect |
|---|---|---|
| Use a proxy… | off | Enables the global proxy |
| Type | SOCKS5 | SOCKS4, SOCKS4A, SOCKS5, HTTP tunnel |
| Host and port | — / 1080 | Proxy address |
| User, password | empty | Authentication (SOCKS5, HTTP Basic); password encrypted with DPAPI |
| Exceptions | empty | Host masks reached directly, `;`-separated (`220.67.*;*.lan`) |
| Also route transfers through the proxy | on | Data channels too |
| No proxy for the local network | on | `localhost`, private addresses, names without a dot |

**Named proxies**: *+ Add a proxy* creates a proxy with a name and the same fields (type,
host, port, login, data channels). A site record chooses it by name in its *Proxy* list;
a site whose proxy was removed from the list follows the global setting. Through a proxy
the data channels always use the passive mode.

### Ident

![Ident](images/options-ident.en.png)

| Option | Default | Effect |
|---|---|---|
| Enable the Ident server | off | RFC 1413 server queried by many FTP servers when you connect |
| Name answered | `URUSHFTP` | User name returned |
| System | `UNIX` | System returned |
| TCP port | 113 | Forward it from your router to this computer when behind NAT |
| Answer only while a connection is being opened | off | The requests of the connections being opened get their name; any other request gets `ERROR : NO-USER` |

### Keep alive

![Keep alive](images/options-keep-alive.en.png)

| Option | Default | Effect |
|---|---|---|
| Send anti-idle commands | on | Keeps idle connections from being kicked by the server |
| Commands | `NOOP` | Sent in turn, separated by `;` (for example `NOOP;PWD`) |
| Every | 60 s | Idle time before a command, then between two commands (5 s minimum) |
| Stop after | 0 (never) | Minutes without any action of the user after which the commands stop |
| Pick the command at random | off | Some servers spot the same command sent again and again |
| Disconnect when the commands stop | off | Logs out once the limit above is reached |
| Reconnect when the server closes the connection | on | Kick, idle timeout, `421`: connects again and goes back to the folder of the panel |
| Also on these replies | — | Wildcard masks on the server reply, separated by `;` (for example `530*too many*`) |

Reconnection makes as many attempts as *Retries* of the Transfers page, waiting *Delay
between retries* before each one. The queue engine also checks a connection that stayed
idle before reusing it, and opens a new one when the server has dropped it.

### Connection

![Connection](images/options-connection.en.png)

| Option | Default | Effect |
|---|---|---|
| Use MLSD when the server offers it | on | `MLSD` gives exact dates and sizes; off: the listing command below |
| Show the hidden files (LIST -a) | off | `LIST -a` instead of `LIST` (files starting with `.`; needed to delete a folder that holds some) |
| Listing command | empty (`LIST`) | Any command (`LIST -al`...); a command starting with `STAT` is read on the control channel. Replaces `MLSD` |
| Switch to active mode (PORT) when the passive mode fails | off | `PASV`/`EPSV` refused or data connection impossible: the connection goes on in active mode |
| External address (sent with PORT) | empty | Public IP announced in active mode, behind a router; empty: the local address |
| Local ports of the active mode | 0 – 0 (any) | Range forwarded by the router to this computer |
| Local address of the connections | empty | IP of the network card the FTP and SFTP connections leave from (not used through a proxy) |
| Send NOOP during the transfers, every | off, 60 s | FTP: keeps the control connection alive during long transfers (proxies, routers); the replies are read with the end of the transfer |
| Use UPnP: open the active-mode ports and the Ident port on the router | off | The router (Internet Gateway Device, found by SSDP) opens each active-mode port while it is used, and port 113 to the Ident server; its public address is sent with `PORT` unless an external address is given. **Restart UPnP service** (also in the FTP menu) searches the router again. The state (router found, public address, Ident port) is shown under the option. IPv4 only |

The listing settings of a site (site record › Listing) win over the first three options.
An invalid address is shown in red and ignored.

### New site

![New site](images/options-new-site.en.png)

Model of the sites created with *New site* in the site manager: protocol and port, user,
password or anonymous, transfer mode, *Also encrypt transfers*, simultaneous transfers and
limits per direction, remote and local folders, listing settings (same fields as the site
record). The host, the other addresses, the bookmarks and the trusted keys are always empty.
*Default values* resets the model.

### Advanced transfer

![Advanced transfer](images/options-advanced.en.png)

| Option | Default | Effect |
|---|---|---|
| Transfer mode | Automatic | Automatic: ASCII for the files of the ASCII list, binary for the others; or always binary / always ASCII. *Edit…* in the queue sets the mode of one item |
| ASCII files | `*.txt;*.nfo;*.diz;*.sfv;*.md5;*.htm;*.html;*.ini;*.log;*.m3u;*.cue;*.xml;*.csv` | Masks of the ASCII list |
| Case of the uploaded / downloaded names, name case in FXP | Unchanged | lower case or UPPER CASE for the names created on the target |
| Never resume ASCII files | on | Their size changes with the line ends: transferred again from the start |
| Active mode (PORT) for every site | off | Data connections in active mode whatever the site says |
| XDUPE | off | `SITE XDUPE 3` after login; the dupes listed in a refused upload are skipped, and the next ones of the folder are not even tried |
| List the folder again when XDUPE returns nothing | off | An upload refused without `X-DUPE` names: when the target exists after all (`SIZE`), the file is skipped as a dupe |
| Compress the transfers (MODE Z) | off | When the server announces `MODE Z`; never for the masks of *Never compressed* (archives, video, pictures by default) |
| Compress: Listings, Downloads, Uploads | all on | What `MODE Z` applies to |
| Send buffer / receive buffer of the data connections (KiB) | 0 (system) | Set apart (the single buffer of an earlier version fills both); a site can replace each one |
| Roll back before resuming a download (KiB) | 0 | The end of a partial file is cut off before resuming |
| Keep free on the local disk (MiB) | 0 | A download that would leave less fails |
| No speed limit for the local network | off | Sites on a private, loopback or link-local address |
| Check the downloads against the SFV file | off | CRC32 of each downloaded file against the SFV of its folder; an SFV downloaded last checks the files already there |
| Keep a status folder beside the checked files | off | An empty folder `[UltraRushFTP] - ( 7 of 10 files OK = 70% ) - INCOMPLETE` (or `COMPLETE`, or `, 2 BAD … - BAD FILES`), replaced after each check |
| Check the uploads and FXP with XCRC | off | When the server knows `XCRC` |
| Remove a file with a bad CRC and transfer it again | on | Otherwise the bad CRC is only logged |
| Several SFV files in a folder: take none | off | |
| Remove the target folders a transfer leaves empty | off | At the end of the queue (skipped contents) |
| Keep the server date on the downloaded files | off | `MDTM` (or the SFTP attributes) |
| Enter a folder after creating it | off | `📁+` / `F7` in a panel |
| Do not transfer a sub-folder named like its parent | off | `release/release`, a common mistake |
| Remove the 0-byte target file when the source of an FXP refuses | on | |

### Browsing

![Browsing](images/options-browse.en.png)

| Option | Default | Effect |
|---|---|---|
| Keep the listings of the folders visited | off | A folder visited again is shown from memory; `F5` always lists from the server. A site record can say yes or no |
| Forget the listing of a folder a transfer writes into | on | With the cache: the target folders of the transfers are listed again |
| Program to view a remote file (`F3`) | — | Lines of masks (`*.nfo;*.txt`) and a program; the first line that matches is used |
| Program to edit a remote file (`F4`) | — | Same; the file is watched and sent back to the server each time the program saves it |

The file is downloaded to `%TEMP%\UltraRushFTP\files`. Without a matching line it opens in
Notepad: never with the program Windows associates with its type, which could run a
downloaded program.

### Interface

![Interface](images/options-interface.en.png)

| Option | Default | Effect |
|---|---|---|
| Name the site instead of Left / Right | on | In the FTP and transfer logs |
| Time of each line | on | |
| Lines kept by each log | 5000 | |
| Also write the logs to files | off | One file per day and per log; folder: `logs` of the data folder when empty |
| Text of the progress bar | `%p · %b · %s · %e` | See *Logs and status bar* |
| Clipboard: addresses open Manual download / queue them at once | off | |
| Size of the interface | 100 % | Everything, text and controls |
| Text size of the logs | 0 (theme) | |
| Layout of the new tabs | — | Name of a saved layout (View → Layouts) |
| Interface / file list / log font | Font of the theme | A TrueType or OpenType font of the Windows fonts folder, by its name; a font that cannot be read is told in the system log and the theme's font stays |
| Tabs below the panels | off | |
| Tabs on several lines when they do not fit | off | |
| Least width of a tab | 0 (its title) | In points |
| Large buttons in the toolbars | off | Top bar, panel bars, queue bar |
| Split Favourites button | off | Its main part connects to the last site |
| File sizes | Dynamic | **Bytes** (`1,234,567 B`), **KB** (`1,206 KB`, rounded up) or **Dynamic** (B, KB, MB, GB…) in the file lists |
| Show hidden files and folders in the local panels | on | Items with the Windows *hidden* attribute |
| Right-click in a local panel: Windows Explorer menu | off | The menu of Windows for the selection (Open with, Send to, Properties…); the menu of UltraRushFTP stays in the Edit and FTP menus, and its *Windows Explorer menu…* item gives the menu of Windows when this is off |
| Read the folder of a running download again every | 0 s (never) | The local panel showing the folder of a running download is listed again, to follow the size of the file |
| Columns sized to their content: Local, Remote, Queue | off | Size, date and attribute columns fitted to what they show |
| Folders before the files | on | |
| List the links apart | off | Links (🔗) between the folders and the files |
| Size and date: largest / newest first | off | First click on these columns |
| Highlighted names | — | Masks and a colour per theme; the contrast against the list backgrounds is shown (4.5:1 at least is advised) |
| My own colours for the log lines | off | Command, reply, status, error, per theme, with their contrast |
| Own colour of the progress bars | off | Per theme, with its contrast |
| Double-click on a file | Transfer | Or view it (remote files), or nothing |
| Double-click on a log line | Copy the line | Or nothing |
| Double-click on a site of the site manager | Connect | Or nothing |
| Double-click on the title of a file panel | Nothing | Refresh, parent folder, or switch between local and remote |
| Double-click on a tab | Nothing | New tab of the same sites, or close the tab |
| Double-click on a queue item | Nothing | Edit, try again (failed or stopped item) or remove it |
| Confirmations | exit, close a busy tab: on; remove queue items: off | Quitting while items are still to transfer; closing a tab with connections or a queue; removing queue items |

### Shortcuts

![Shortcuts](images/options-shortcuts.en.png)

Keys of each command, several separated by commas (`Ctrl+Shift+F, F9`, `Alt+Delete`,
`Plus`, `Minus`, `Space`, `Insert`). A key not understood keeps the default keys; keys
given to several commands are listed. `F1` shows the keys in use. The arrows, `Enter`,
`Esc`, `Tab`, `Ctrl+Tab` and `Ctrl+V` cannot be changed.

### Notifications

![Notifications](images/options-notifications.en.png)

| Option | Default | Effect |
|---|---|---|
| Always show the icon | off | Icon in the notification area; a click shows the window |
| Minimizing hides the window there | off | |
| Flash the taskbar button when a queue ends in the background | on | |
| Flash the taskbar button while transfers run in the background | off | Stops when they end or when the window comes forward |
| Events: sound, pop-up | queue finished and new items: Windows sound and pop-up; transfer failed: pop-up | Sound: empty, `system` or a `.wav` file (`▶` plays it); pop-ups above the icon, or in a corner of the window without it; every event also goes to the system log |
| Monitored folders | — | Site (name in the site manager), folder, interval; each check lists the folder on a connection of its own and the new names are notified (the first listing only takes note) |

### Security

![Security](images/options-security.en.png)

| Option | Default | Effect |
|---|---|---|
| Master password | none | Asked before the site manager and the options (once per session) and to unlock *File → Lock*. Only a salted PBKDF2-SHA256 hash is kept; the site passwords stay protected by Windows (DPAPI). A forgotten master password cannot be recovered: remove `master_hash` from `settings.json` |
| Only one UltraRushFTP | off | A second start shows the window already open (not in the FTPRush compatibility mode of the mIRC link) |
| Back up the configuration each day | off | At the first start of the day, into `backup` of the data folder; 4 kept |
| ShellExecuteCommand | Ask each time | What a script asking to start a program gets: a question showing the script's origin, the program, its parameters and folder (*Start it*, *Always allow this program*, *Refuse*); *Start them without asking*; *Never start a program*. Scripts evaluated by `Invoke` cannot ask: they only start the programs allowed |
| Programs always allowed | none | Programs allowed with *Always allow this program*; `✕` stops allowing one |

### Commands

![Commands](images/options-commands.en.png)

The command designer (FTPRush *FTP Command Designer*). Each command has a **name**, a
**folder** (sub-folders separated by `/`: `glftpd/Users`) that makes a sub-menu, a
**key** that runs it directly (`Ctrl+Alt+W`; keys taken twice, or by a key of the
program, are reported) and its **FTP commands**, one per line. The commands are in
*FTP › Site commands* and in the right-click menu of the remote panels; the name of a
command that asks questions ends with `…`.

| Parameter | Value |
|---|---|
| `%f` | Selected name; the line is sent once per selected item (nothing selected: the command is refused) |
| `%p` | Current folder of the remote panel |
| `%c` | Text of the clipboard |
| `%d[caption]` | Asks for a text when the command runs, with this caption (one field per `%d`, in order) |
| `%1` … `%9` | Answer to the Nth question (a number without its `%d` asks too) |
| `%all%` | Sends to every connected site instead of the active panel |
| `%clearbuf%` | Clears the directory cache and lists the folder again |
| `&window` | Shows the replies in the *Send RAW command* window |
| `%%` | A `%` sign |

Defaults: `SITE HELP`, `SITE WHO`, `SITE NEW`, `SITE SEARCH %d[Search]`, `SITE STAT`,
`SYST` + `FEAT`.

### Backups

**File → Back up the configuration** copies the files of the data folder (sites,
options, statistics, layouts, unfinished queue) into `backup\<date>_<time>`.
**File → Restore a backup** puts the files of one back and closes UltraRushFTP, which
must be started again. The passwords of a backup only work for the same Windows account.

## Site record

![Site manager](images/site-manager.en.png)

In the site tree, a site is moved by dragging it onto a folder (at its end) or onto another
site (before it); a folder is dragged onto another folder to go inside it.

| Field | Default | Notes |
|---|---|---|
| Name | "New site" | Used by scripts, mIRC and `urftp` (case-insensitive) |
| Description | empty | Short text shown next to the name in the site tree |
| Protocol | FTP | FTP, FTPS (explicit), FTPS (implicit), SFTP; the port follows (21, 990, 22) |
| Host, port | — / 21 | Name or IP address; when a name has IPv6 and IPv4 addresses both are tried ("happy eyeballs") |
| Forced UTF-8 | off | FTP: `OPTS UTF8 ON` is sent and the names are read in UTF-8 even when the server does not announce `UTF8` (replaces the character set) |
| Send the login in UTF-8 | off | FTP: `USER` and `PASS` in UTF-8 when the site uses another character set |
| PASV only (no EPSV) | off | FTP: passive mode with `PASV` only. For bouncers: `EPSV` only gives a port, so the data connection goes to the bouncer, which does not forward it; `PASV` gives the address of the real server. Without it, a data connection that fails after `EPSV` switches the connection to `PASV` |
| The server is behind NAT | off | FTP: the data connections always go to the address of the server, whatever `PASV` answers (otherwise only when `PASV` gives a private address for a public server) |
| Other addresses | none | `host:port` (or `[IPv6]:port`), one per line: tried in turn when the main address cannot be reached or refuses the login; the address that worked is used first next time |
| User, password, anonymous | — | Password encrypted with DPAPI; with an SSH key it is the passphrase |
| Site enabled | on | A disabled site cannot be connected (`RushApp.FTP.DisableSite`) |
| Also encrypt transfers (PROT P) | on | FTPS: encrypted data channels of the transfers |
| Also encrypt listings (PROT P) | like the transfers | FTPS: encrypted data channels of the listings |
| Use AUTH SSL instead of AUTH TLS | off | Explicit FTPS: `AUTH SSL` is sent first (`AUTH TLS` if refused), for the servers that hang up on `AUTH TLS` (FTPRush's *AUTH SSL* mode) |
| Send PBSZ 0 after the TLS login | on | FTPS: off for the servers that refuse it |
| Encrypted FXP (FTP to FTP) | Automatic | Automatic (`SSCN` when announced, otherwise `CPSV`), Do not use (clear data channel between the servers), Use |
| Certificate | Verify with the Windows authorities | or Accept any certificate (not recommended), or Trusted certificate (SHA-256 fingerprint, filled in when you trust a server) |
| Private key | none | SFTP: OpenSSH, PEM or PuTTY `.ppk` key file |
| Other keys | none | SFTP: tried in turn when the server refuses the previous one; the password is the passphrase of every key |
| Trusted server keys | none | SFTP host key fingerprints (`SHA256:…`) added when you trust a server |
| Mode | Passive | Passive or Active (`PORT`/`EPRT`); see also the `PORT` fallback of the Connection page |
| Simultaneous transfers | 2 | 0 = single connection |
| Max. downloads / Max. uploads | not set | Extra limits per direction |
| Remote folder, local folder | empty | Opened on connection |
| FXP from this site: never PASV | off | FXP: this site always connects (`PORT`), the other one listens |
| FXP to this site: always PASV | off | FXP: this site always listens; otherwise the source listens and the roles are reversed when `PORT` is refused |
| Proxy | Global setting | Global setting, Never use a proxy, a proxy specific to the site (same fields as the global one), or one of the named proxies |
| Ident name specific to this site | off | Answered by the Ident server while the site connects (the server must be enabled) |
| Bookmarks | none | Name, remote folder, local folder; used by `RS_BOOKMARK` in scripts |
| Notes | empty | Free text |
| Logs: the name of the site instead of Left / Right | off | Whatever the option of the Interface page |
| Notify the new folders and files, folders | off, none | Folders of the site listed again at their interval (minutes); the new names are notified as set in Options › Notifications (same as the monitored folders there) |
| Statistics | — | This server: last access, connections, uploaded, downloaded, FXP from and to it, totals (*Reset this server*). Every server since the first use: totals and average per day (*Copy*, *Reset every server*). This session: totals (*Copy the session*). Kept in `stats.json` |
| Web address: address of the folder, remote folder served there | empty, `/` | For *Copy HTTP address*: `https://www.example.com/files` and `/pub` turn `/pub/a b.zip` into `https://www.example.com/files/a%20b.zip` |

**Listing** (FTP and FTPS):

| Field | Default | Notes |
|---|---|---|
| Listing command | Global setting | Global setting (Connection page), `STAT -l` (on the control channel, no data connection) or a custom command |
| Server time zone | Unknown / do not change | UTC−12:00 to UTC+14:00: fills in the offset below from the local time zone |
| Time zone offset | 0 h | Hours (quarters allowed) added to the dates of the listings, for a server in another time zone |
| Character set | UTF-8 (automatic) | For the servers that do not use UTF-8: windows-1252, ISO-8859-1/2/15, windows-1250/1251, KOI8-R, Shift_JIS, EUC-JP, GBK, gb18030, Big5, EUC-KR, IBM866. Names, paths and replies are converted; `OPTS UTF8 ON` is then not sent, and a name the set cannot write is refused |
| Remove the spaces at the start of the listing lines | off | For servers whose `LIST` lines start with spaces (always done for `STAT`) |
| Listing format | Automatic | Automatic (Unix or DOS/IIS line by line), Unix, DOS / IIS |

![Settings of a site](images/site-settings.en.png)

**Settings of the site** (sections *Connection settings of this site*, *Queue rules of this
site*, *Separate accounts*). A setting left unticked follows the global option.

| Field | Default | Notes |
|---|---|---|
| Own retries and timeouts | off | Retries, delay between retries, connection and reply timeouts; they start from the global values (Transfers page) |
| Resuming interrupted transfers | Automatic | Automatic: resume, and follow *Start over if resuming is refused* when the server refuses `REST`; Yes: always resume (a refused `REST` fails the transfer); No: always from zero |
| Directory cache | Global setting | Yes / No replaces *Keep the listings of the folders visited* for this site |
| Ask the server its features (FEAT) | on | FTP: off for old servers that dislike `FEAT` (`MLSD`, `PRET`, `UTF8` are then not detected) |
| Choose the capabilities used | off (all) | FTP: `PRET`, UTF-8 (`OPTS UTF8`), `MODE Z`, `XCRC`, `MLSD`, `SSCN`, `CPSV`; one unticked is not used even when the server announces it |
| Client name (CLNT) | empty | FTP: sent with `CLNT` after the login (empty: not sent) |
| Transfers specific to this site | Global setting | FTP: case of the uploaded, downloaded and FXP names; XDUPE; List the folder again when XDUPE returns nothing; NOOP during the transfers; send and receive buffers (KiB) |
| Own keep alive and reconnection | off | Same fields as the Keep alive page |
| Own skip list / priorities / "existing file" rules | off | Same fields as the matching pages (the "existing file" rules per direction); the skip list and the priorities of the site the files come from apply, the "existing file" rules, the retries and the resume setting of the site they go to |
| Account for uploads / for downloads | off | Address and port (empty: those of the site), login and password used by the transfers in this direction (and the folders created or listed for them); browsing keeps the main login |

**Connect** (site manager): hold `Shift` for a ghost login (`!user`: the server closes
the other sessions of the account) and `Ctrl` for `-password` (glftpd and others: no
welcome messages). FTP and FTPS only.

**Test availability** (site manager toolbar): connects to the selected site, or to every
enabled site of the selected folder at the same time, then logs out. The tree shows the
time taken (`✔ 120 ms`) or `✖`; the site form shows the address that answered or the
error.

**Import** (site manager toolbar): FileZilla (`sitemanager.xml` or export), FTPRush v3
(`site.json`), FTPRush v1/v2 (`RushSite.xml`), FlashFXP (`.ftp`). The format is detected
automatically; files found on this computer are offered directly. Passwords are not
imported from FTPRush v1/v2 and FlashFXP. A text file with one address per line
(`ftp://user:password@host:port/folder`) is imported too, one site per address.

**Export ▾**: the sites of the selected folder (or every site) as addresses (text), a
table (CSV), a web page (HTML) or an FTPRush v2 `RushSite.xml`. Passwords are never
exported. **☰ List** shows every site in one list sortable by name, host, type or folder
(**📁 Folders** goes back to the tree).

## Command line

Startup options of `UltraRushFTP.exe` (FTPRush v2 syntax, `-` or `/`):

| Option | Effect |
|---|---|
| `-w<n>` | Open n tabs (1 to 50) |
| `-c<url>` | Connect: `ftp://user:pass@host:port/`, `ftps://` (explicit), `ftpsi://` (implicit), `sftp://`, the name of a site of the site manager, or a host alone (anonymous FTP) |
| `-f"folder"` | Settings and sites in this folder; `-f` alone: the program folder (portable mode) |
| `-r[t][x]` | Start the queues at startup; `t`: minimise the window; `x`: quit when nothing is left to transfer |
| `-s"file"` | Run this Pascal Script at startup (UltraRushFTP addition) |
| `-d` | Debug mode (accepted; the log is already complete) |
| `-b` | FTPRush v2 "BetaUser" licence switch: ignored |

Unknown options are reported in the script console.

## `urftp` tool

```text
urftp [options] <command> [arguments]
```

| Command | Effect |
|---|---|
| `sites` | Lists the sites of the site manager |
| `ls <target>` | Lists a remote folder |
| `get <target> [local folder]` | Downloads a file or a folder (current folder by default) |
| `put <file\|folder> <target>` | Uploads into the remote folder |
| `fxp <source> <destination>` | Site-to-site transfer |
| `raw <site> <command>...` | Raw FTP commands, replies printed |

A *target* is `SiteName:/path` (site manager, case-insensitive name; `SiteName` alone =
its remote folder) or a URL `ftp://user:pass@host:port/path` (`ftps://`, `ftpsi://`,
`sftp://`; `%XX` escapes are decoded).

| Option | Effect |
|---|---|
| `-v`, `--verbose` | Protocol log on stderr (password masked) |
| `-n`, `--transfers <n>` | Simultaneous transfers per site |
| `--exists <action>` | Existing file: `overwrite`, `resume`, `skip`, `rename` |
| `--insecure` | Accept any TLS certificate |
| `--fingerprint <fp>` | Expected SHA-256 fingerprint (FTPS certificate or SFTP host key) |
| `--data <folder>` | Settings folder (like `-f`) |
| `--lang <en\|fr>` | Language of the messages (default: the option, then Windows) |
| `-h`, `--help` | Help |

Exit codes: `0` everything transferred, `1` failure (connection, refused command,
incomplete transfer), `2` invalid arguments.

## Scripts (`RushApp`)

Pascal Script as in FTPRush v2, run from the *Script* console, `-s`, `URUSHFTP_SCRIPT` or
mIRC. Each script runs beside the interface: the window stays usable, `InputQuery` opens
an input box and waits for it, and the console shows *Script running…* with a **■ Stop**
button (the script stops at its next call to the application). **Open…**, **Save** and
**Save as…** load and write the text of the console as a `.pas` file. `Invoke` evaluates
its expression at once, so an `InputQuery` in it gets no answer.

Supported language:

- variables (`var a, b;`, types optional and ignored, or implicit), `const NAME = value;`,
  strings (`'…'`, `#13#10`), integers (`$1F`), booleans, `nil`;
- arrays: `A := [2, 4, 6]` (0-based), nested `[[…], […]]` read with `M[0, 2]` or
  `M[0][2]` and assigned with `M[1, 1] := x`; strings indexed from 1 (`S[1] := 'A'`);
  `x in [1, 2]`; `SetLength`, `Length`, `Low`, `High`;
- operators (bitwise `or`/`and`/`xor` on flags), `if/then/else`,
  `case … of` (labels of any type, several per branch, ranges `1..5`, `else`), `while`,
  `repeat/until`, `for … to/downto`, `break`, `continue`, `begin/end`, `exit`;
- `procedure` and `function` declared before the statements, with parameters, `var`
  parameters (by reference), local `var`, `Result` or the function's name for the
  value, recursion (64 levels);
- built-in functions `IntToStr`, `StrToInt`, `StrToIntDef`, `Length`, `UpperCase`,
  `LowerCase`, `Trim`, `Copy`, `Pos`, `Chr`, `Ord`, `Abs`, `Odd`, `ShowMessage`,
  `InputQuery`; FTPRush library: `MatchExpr(text, regex)`, `MatchString(text, wildcards)`
  (case-insensitive, like the transfer filters), `ShellExecuteCommand(program, parameters,
  folder, flag)` (see *Options › Security*; `flag` is not used).

| Procedures | Properties (read) | Properties (write) |
|---|---|---|
| `RushApp.FTP.Login`, `Logout`, `CWD`, `RAW`, `Delete`, `Transfer`, `RemoveQueue`, `EnableSite`, `DisableSite`; `RushApp.DeleteEmptyTabs` (`DeleteEmptyContainers`); `RushApp.UI.Selection.Prepare` | `RushApp.Version`, `Language`, `StartDate`, `DayCount`, `TotalDownloadSize`, `TotalUploadSize`, `TotalFxpSize`, `CurDownloadSize`, `CurUploadSize`, `CurFxpSize`, `DownloadBW`, `UploadBW`, `PoolInfo`, `FTP.Splitter`, `Containers.ContainerCount`, `Containers.ConnectedSiteCount`, `Containers.TransferSiteCount`; `UI.Selection.Count`, `Items[i]` (`Name`, `Size`, `Date`, `Attr`, `Owner`, `Group`, `IsFolder`, `FullPath`), `IsRemoteSide`, `IsFTPConnected`, `FTPName`; `UI.OnSwitchLayout` | `RushApp.FTP.Splitter`, `RushApp.UI.OnSwitchLayout` |

`RushApp.UI.Selection.Prepare` takes the selection of the active panel (the row under the
cursor when nothing is selected); the other `Selection` properties read what it took.
`RushApp.UI.OnSwitchLayout := MyProcedure;` runs `MyProcedure` (with or without a `Sender`
parameter) each time a saved layout is loaded from *View → Layouts*; `:= nil` removes it.
The procedure runs on its own: the global variables of the script that set it are not
kept. `RushApp.UI.Bar` and `RushApp.UI.Menu` answer an error: the FTPRush help describes
no member of them.

Flags handled: `RS_APPEND`, `RS_BOOKMARK`, `RS_CHECKBM`, `RS_CURRENT`, `RS_DELEMPTYTABS`,
`RS_DIRDES`, `RS_DIRSRC`, `RS_DOWN`, `RS_EXPR`, `RS_FAIL`, `RS_FILE`, `RS_FOLDER`,
`RS_FULLPATH`, `RS_GHOST`, `RS_INVERT` (`-password`), `RS_LOGIN`, `RS_LOGOUT`, `RS_NOCIRCLE`,
`RS_NOMKD`, `RS_NOSKIP`, `RS_NORMAL`, `RS_ONE`, `RS_PAIR`, `RS_POPUP`, `RS_SORTDATE`,
`RS_SORTDES`, `RS_SORTSIZE`, `RS_TRANSFER`, `RS_UP`, `RS_WAITING`, `RS_CHECKLOGIN` (the
sites of the transfer are logged in to first, at the same time; the ones that cannot log
in are left out and told in the log).

`RushApp.FTP.Transfer(Handle, SrcList, SrcFolder, SrcItem, DesList, DesFolder, DesItem,
Flag, FolderAllow, FolderSkip, FileAllow, FileSkip, CompleteFlag, RepeatCount, RetryCount,
Timeout, FolderSort, FileSort, Depth, SubCount)`:

- several pairs go each to a new tab and run at the same time; with `RS_APPEND` (or a
  `Handle`) they share one tab; `RS_NOCIRCLE` makes each pair wait until the previous one
  is all over;
- `FolderAllow` … `FileSkip`: masks separated by `;` (`RS_EXPR`: regular expressions),
  `%F%` / `%D%` before a mask for files / folders only; `RS_NOSKIP` ignores the skip list;
- `RepeatCount`: the folder is listed again this many times to catch new entries, until a
  name matches `CompleteFlag` (regular expression); `RetryCount`: retries of the items;
- `FolderSort` / `FileSort`: name ascending by default, `RS_SORTDATE`, `RS_SORTSIZE`,
  `RS_SORTDES`; `SubCount`: only the first entries of each folder; `Depth`: levels of
  sub-folders entered (0: no limit);
- `RS_LOGOUT` logs the panels out and `RS_DELEMPTYTABS` closes the tab once its queue is
  finished; `RS_NOMKD` is how folders are always created (entered when they exist);
  `Timeout` is not used, as in FTPRush.

## mIRC DLL

| Export | Use |
|---|---|
| `LoadDll` / `UnloadDll` | Called by mIRC; the DLL stays loaded |
| `Invoke` | `$dll(rushmirc.dll, Invoke, <expression>)` returns a value |
| `RushScript` | `/dll rushmirc.dll RushScript <script>` runs a script; `RAW` replies come back to mIRC |
| `RushCommand` | Same, without replies |
| `EnableCallback` / `DisableCallback` | Turn the `RAW` replies on (default) or off |

Use `mirc/x86/rushmirc.dll` for 32-bit mIRC and `mirc/x64/rushmirc.dll` for 64-bit mIRC.

The DLL finds the application through a hidden window. UltraRushFTP names it `TfmUltraRush`,
and its DLL looks for `TfmUltraRush` first, then `TfmRush` (FTPRush). FTPRush v2 can
therefore run at the same time. The **original** `rushmirc.dll` of FTPRush only knows
`TfmRush`: to drive UltraRushFTP with it, tick *Compatibility with the mIRC DLL of FTPRush*
(Options › General). FTPRush v2 looks for that same window when it starts and does not open
while it exists. When a `TfmRush` window is already open (FTPRush running), UltraRushFTP
does not create a second one and says so in the log.

## Environment variables

| Variable | Effect |
|---|---|
| `URUSHFTP_DATA` | Settings folder (overridden by `-f` / `--data`) |
| `URUSHFTP_LANG` | `en` or `fr`: forces the language (interface and `urftp`) |
| `URUSHFTP_THEME` | `light` (or `clair`) / anything else: forces the theme at startup |
| `URUSHFTP_CONNECT` | URL connected at startup (like `-c`) |
| `URUSHFTP_SCRIPT` | Script source run at startup |
| `URUSHFTP_IPC_CLASS` | Window class used for the mIRC link instead of `TfmUltraRush` / `TfmRush`, by the application and by the DLL (tests) |
| `URUSHFTP_OPEN_SITES` | Development: opens the site manager on this site id |
| `URUSHFTP_SITE_SETTINGS` | Development: keeps the site form scrolled to the settings of the site |
| `URUSHFTP_OPEN_OPTIONS` | Development: opens the Options window on this page (0 to 16) |
| `URUSHFTP_OPEN_SHORTCUTS` | Development: opens the keyboard shortcuts window |
| `URUSHFTP_SHOW_PANES` | Development: `windows,tasks` adds the window list and the Task window to the first tab |
| `URUSHFTP_OPEN_ABOUT` | Development: opens the About window |

## Files

In the data folder (`%APPDATA%\UltraRushFTP` by default):

| File | Content |
|---|---|
| `sites.json` | Site book (passwords `dpapi:…`); written atomically, previous version kept as `sites.json.bak` |
| `settings.json` | Options above (proxy password `dpapi:…`); missing keys take their default value |
| `stats.json` | Transfer totals and first-use date (`RushApp.Total*`, `StartDate`) |
| `queue.json` | Unfinished queue of every tab (removed when nothing is left to do) |
| `*.urq` | Queue files saved by hand (anywhere), same format as `queue.json` |
| `layouts.json` | Saved layouts of the panels |
| `tasks.json` | Scheduled tasks (Task window) |
| `logs\` | Log files (when written to disk) |
| `backup\` | Backups of the configuration |
