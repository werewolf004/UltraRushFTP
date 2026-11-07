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
| File → Options… | Opens the Options window (also `⚙`) |
| File → Quit | Quits |
| View → Light theme / Dark theme | Switches the theme (also stored in the options) |
| Help → Keyboard shortcuts | Lists the shortcuts below (also `F1`) |
| Quick connect | Protocol (FTP, FTPS explicit, FTPS implicit, SFTP), host, port, user, password, **Connect**: opens the site in the right panel of the current tab (or in a new tab, see *General*) |
| Tabs | One tab per site container; `✕` closes it, the title is the host or "Tab n"; `Ctrl+T` new, `Ctrl+W` close, `Ctrl+Tab` / `Ctrl+Shift+Tab` next / previous |

### File panels

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
| Select all / several | `Ctrl`+click, `Shift`+click | `Ctrl+A`, `Shift`+movement, `Ctrl+Space` (row at the cursor) |
| Copy the full path | right-click → Copy path | — |
| Copy the web address | right-click → Copy HTTP address (remote panel of a site with a web address) | — |
| Sort | click a column header (again: reverse order) | — |

The file-list keys act on the active panel (marked `▸`, cursor outlined) when no control
has the keyboard focus. `Tab` / `Shift+Tab` move between the controls of the window and
`Esc` gives the keys back to the list.

![Keyboard shortcuts](images/shortcuts.en.png)

### Accessibility

- Every control has a name for screen readers (AccessKit), including symbol buttons and
  fields that only show a placeholder; file rows are read as "name, size, date".
- Both themes meet WCAG 2.2 AA contrast (4.5:1 for text, 3:1 for focus and state
  indicators) and pointer targets are at least 24 px or spaced as WCAG 2.5.8 allows; tests
  check both.
- States are never shown by colour alone (symbols, `⚠`, written warnings).
- Confirmations of irreversible actions start on **Cancel**.

### Queue

Buttons: **Start**, **Stop all**, **Retry failed**, **Remove finished**. Right-click on an
item: stop, retry, move to top / up / down / to bottom, remove from the queue. States:
waiting, running, done, skipped (with the reason), needs confirmation, stopped, failed.

## Options window

Saved automatically in `settings.json` one second after a change.

### General

![General](images/options-general.en.png)

| Option | Default | Effect |
|---|---|---|
| Dark theme | on | Dark or light theme |
| Language | Windows language | Windows language, English or Français; applied immediately |
| Open a new tab when the right panel is already connected | on | Otherwise the new connection replaces the current one |
| E-mail for anonymous logins | `urftp@` | Password sent by the sites set to *anonymous* |
| Default download folder | user folder | Folder the local panels start in, and come back to on disconnection; the user folder when it does not exist |

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

### Speed

| Option | Default | Effect |
|---|---|---|
| Download (KiB/s) | 0 (unlimited) | Shared by every download, applied immediately |
| Upload (KiB/s) | 0 (unlimited) | Shared by every upload, applied immediately |

FXP is not limited: its data does not go through this computer.

### Existing file

![Existing file](images/options-existing-file.en.png)

| Case | Default | Choices |
|---|---|---|
| The target is smaller | Resume | Resume, Skip, Rename ("name (1)"), Overwrite, Ask |
| The target has the same size | Skip | same |
| The target is larger | Rename | same |
| Start over if resuming is refused | on | Overwrite when the server refuses `REST` |

### Skipped files

![Skip list](images/options-skiplist.en.png)

| Option | Default | Effect |
|---|---|---|
| Use the skip list | on | Enables the rules below |
| Skip 0-byte files | off | — |
| Do not create empty folders | off | — |
| Rules | none | Applied in order, the first match decides: **Skip** or **Allow**, mask (wildcard `*` `?`, or regular expression with *regex*), target (files, folders, both), directions (upload ⬆, download ⬇, FXP ↔), *path* = the mask applies to the full path (also written `%W%mask`) |

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

The listing settings of a site (site record › Listing) win over the first three options.
An invalid address is shown in red and ignored.

### New site

![New site](images/options-new-site.en.png)

Model of the sites created with *New site* in the site manager: protocol and port, user,
password or anonymous, transfer mode, *Also encrypt transfers*, simultaneous transfers and
limits per direction, remote and local folders, listing settings (same fields as the site
record). The host, the other addresses, the bookmarks and the trusted keys are always empty.
*Default values* resets the model.

## Site record

![Site manager](images/site-manager.en.png)

| Field | Default | Notes |
|---|---|---|
| Name | "New site" | Used by scripts, mIRC and `urftp` (case-insensitive) |
| Description | empty | Short text shown next to the name in the site tree |
| Protocol | FTP | FTP, FTPS (explicit), FTPS (implicit), SFTP; the port follows (21, 990, 22) |
| Host, port | — / 21 | Name or IP address; when a name has IPv6 and IPv4 addresses both are tried ("happy eyeballs") |
| Other addresses | none | `host:port` (or `[IPv6]:port`), one per line: tried in turn when the main address cannot be reached or refuses the login; the address that worked is used first next time |
| User, password, anonymous | — | Password encrypted with DPAPI; with an SSH key it is the passphrase |
| Site enabled | on | A disabled site cannot be connected (`RushApp.FTP.DisableSite`) |
| Also encrypt transfers (PROT P) | on | FTPS: encrypted data channels of the transfers |
| Also encrypt listings (PROT P) | like the transfers | FTPS: encrypted data channels of the listings |
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
| Web address: address of the folder, remote folder served there | empty, `/` | For *Copy HTTP address*: `https://www.example.com/files` and `/pub` turn `/pub/a b.zip` into `https://www.example.com/files/a%20b.zip` |

**Listing** (FTP and FTPS):

| Field | Default | Notes |
|---|---|---|
| Listing command | Global setting | Global setting (Connection page), `STAT -l` (on the control channel, no data connection) or a custom command |
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
| Resume interrupted transfers | on | Off: a partial file is always transferred again from zero (no `REST`) |
| Ask the server its features (FEAT) | on | FTP: off for old servers that dislike `FEAT` (`MLSD`, `PRET`, `UTF8` are then not detected) |
| Client name (CLNT) | empty | FTP: sent with `CLNT` after the login (empty: not sent) |
| Own keep alive and reconnection | off | Same fields as the Keep alive page |
| Own skip list / priorities / "existing file" rules | off | Same fields as the matching pages; the skip list and the priorities of the site the files come from apply, the "existing file" rules, the retries and the resume setting of the site they go to |
| Account for uploads / for downloads | off | Login and password used by the transfers in this direction (and the folders created or listed for them); browsing keeps the main login |

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
imported from FTPRush v1/v2 and FlashFXP.

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
mIRC. Supported language: variables, strings, integers, booleans, operators (bitwise
`or`/`and` on flags), `if/then/else`, `while`, `repeat/until`, `for … to/downto`,
`begin/end`, `exit`; built-in functions `IntToStr`, `StrToInt`, `StrToIntDef`, `Length`,
`UpperCase`, `LowerCase`, `Trim`, `Copy`, `Pos`, `ShowMessage`, `InputQuery`.

| Procedures | Properties (read) | Properties (write) |
|---|---|---|
| `RushApp.FTP.Login`, `Logout`, `CWD`, `RAW`, `Delete`, `Transfer`, `RemoveQueue`, `EnableSite`, `DisableSite`; `RushApp.DeleteEmptyTabs` (`DeleteEmptyContainers`) | `RushApp.Version`, `Language`, `StartDate`, `DayCount`, `TotalDownloadSize`, `TotalUploadSize`, `TotalFxpSize`, `CurDownloadSize`, `CurUploadSize`, `CurFxpSize`, `DownloadBW`, `UploadBW`, `PoolInfo`, `FTP.Splitter`, `Containers.ContainerCount`, `Containers.ConnectedSiteCount`, `Containers.TransferSiteCount` | `RushApp.FTP.Splitter` |

Flags handled: `RS_BOOKMARK`, `RS_CHECKBM`, `RS_CURRENT`, `RS_DIRDES`, `RS_DIRSRC`,
`RS_DOWN`, `RS_EXPR`, `RS_FAIL`, `RS_FILE`, `RS_FOLDER`, `RS_FULLPATH`, `RS_GHOST`,
`RS_INVERT` (`-password`), `RS_LOGIN`, `RS_LOGOUT`, `RS_NORMAL`, `RS_ONE`, `RS_PAIR`,
`RS_POPUP`, `RS_TRANSFER`, `RS_UP`, `RS_WAITING`. The filters and repeat count of
`Transfer` and interactive `InputQuery` are recognised but not supported yet (a message
says so).

## mIRC DLL

| Export | Use |
|---|---|
| `LoadDll` / `UnloadDll` | Called by mIRC; the DLL stays loaded |
| `Invoke` | `$dll(rushmirc.dll, Invoke, <expression>)` returns a value |
| `RushScript` | `/dll rushmirc.dll RushScript <script>` runs a script; `RAW` replies come back to mIRC |
| `RushCommand` | Same, without replies |
| `EnableCallback` / `DisableCallback` | Turn the `RAW` replies on (default) or off |

Use `mirc/x86/rushmirc.dll` for 32-bit mIRC and `mirc/x64/rushmirc.dll` for 64-bit mIRC.

## Environment variables

| Variable | Effect |
|---|---|
| `URUSHFTP_DATA` | Settings folder (overridden by `-f` / `--data`) |
| `URUSHFTP_LANG` | `en` or `fr`: forces the language (interface and `urftp`) |
| `URUSHFTP_THEME` | `light` (or `clair`) / anything else: forces the theme at startup |
| `URUSHFTP_CONNECT` | URL connected at startup (like `-c`) |
| `URUSHFTP_SCRIPT` | Script source run at startup |
| `URUSHFTP_IPC_CLASS` | Window class used for the mIRC link instead of `TfmRush` (tests next to a running FTPRush) |
| `URUSHFTP_OPEN_SITES` | Development: opens the site manager on this site id |
| `URUSHFTP_SITE_SETTINGS` | Development: keeps the site form scrolled to the settings of the site |
| `URUSHFTP_OPEN_OPTIONS` | Development: opens the Options window on this page (0 to 8) |
| `URUSHFTP_OPEN_SHORTCUTS` | Development: opens the keyboard shortcuts window |

## Files

In the data folder (`%APPDATA%\UltraRushFTP` by default):

| File | Content |
|---|---|
| `sites.json` | Site book (passwords `dpapi:…`); written atomically, previous version kept as `sites.json.bak` |
| `settings.json` | Options above (proxy password `dpapi:…`); missing keys take their default value |
| `stats.json` | Transfer totals and first-use date (`RushApp.Total*`, `StartDate`) |
