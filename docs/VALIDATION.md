*[Français](VALIDATION.fr.md)*

# Validation against real servers

The automated tests run against servers written for the tests. This guide checks
UltraRushFTP against real server software: glftpd, vsftpd, ProFTPD, FileZilla Server, IIS
and OpenSSH. It has two parts:

1. a **script** that runs a fixed scenario with `urftp` (the command-line client) and
   writes an anonymised report;
2. a **checklist** of what the script cannot check, to do by hand in the interface.

Use test servers or test accounts: the script writes and then removes a folder on the
server.

## 1. The script

### Before starting

- Windows PowerShell 7 (`pwsh`). The script is in `scripts/` of the source tree and at the
  root of the release archive, next to `urftp.exe`.
- In the site manager of the interface, create the site and **enter its password** (the
  script never takes credentials on its command line). Connect once from the interface to
  accept the certificate or the SSH host key, or give `-Insecure` / `-Fingerprint`.
- A remote folder where this account may create folders, upload, download and delete.

### Running it

```powershell
pwsh scripts/validate-server.ps1 -Site "Lab vsftpd" -Folder /upload -Label "vsftpd 3.0.5"
```

With FXP to a second site:

```powershell
pwsh scripts/validate-server.ps1 -Site "Lab glftpd" -Folder /incoming -FxpTo "Lab ProFTPD" -FxpFolder /pub -Label glftpd
```

| Parameter | Meaning |
|---|---|
| `-Site` | Site manager site to check (name, without its group). |
| `-Folder` | Remote folder where the test folder `urftp-validation-<date>` is created. |
| `-FxpTo`, `-FxpFolder` | Second site and its folder: the test folder is copied there by FXP, then read back. |
| `-Label` | Name of the server in the report (e.g. its software and version). |
| `-Insecure` | Accept any TLS certificate (self-signed lab servers). |
| `-Fingerprint` | Expected SHA-256 fingerprint (FTPS certificate or SSH host key). |
| `-SizeMiB` | Size of the large test file (default 8). |
| `-Keep` | Leave the test files on the server. |
| `-Data` | Settings folder (like `-f` of the interface). |
| `-Urftp` | Path of `urftp.exe` (default: next to the script, else `target/release`). |
| `-Out` | Report folder (default: `target/validation/<date>` or `validation/<date>`). |
| `-SelfTest` | Runs the scenario against two local development servers, to check the script itself. |

### What it checks

| Step | Checked |
|---|---|
| Connect and list the folder | Login, TLS, listing of the folder. |
| Server information | `SYST`, `FEAT`, `PWD` (FTP only; a refusal is a warning). |
| Upload a folder | `MKD`, three simultaneous uploads, sub-folders, names with spaces, accents and `#`. |
| List the uploaded folder | The listing shows the names, the accents and the folders. |
| Download the folder | Three simultaneous downloads; every file compared by SHA-256. |
| Resume a download | 40 % of the file present locally: `REST` then the rest, file identical. |
| Overwrite on upload | The remote file is replaced. |
| Resume an upload | 40 % of the file on the server: `REST`+`STOR` or `APPE`, read back and compared. |
| FXP and read back | Site-to-site copy of the folder, then download from the second site and comparison. |
| Remove the test files | `DELE` and `RMD` (FTP and FTPS; with SFTP, remove the folder by hand). |

A step is **ok**, **fail**, **warn** (works, but not as expected: for instance a resume
that downloaded the whole file again) or **skip**. The script exits with code 1 when a step
failed.

### The report

The report folder holds `report.md` (the results table) and `log.txt` (every command and
reply, from every connection). Before they are written, the script hides the host, the user
name, the site names, every IPv4 and IPv6 address, the `PASV`/`PORT` tuples and the Windows
user folder; passwords never appear (`PASS ****`). **Read both files before sending them**:
a welcome banner can still name the site.

## 2. By hand in the interface

Tick what works, note what does not, with the protocol log (select the lines of the
*Log* panel with the mouse, then `Ctrl+C`).

- [ ] **Active mode**: site in active mode (`PORT`/`EPRT`), with and without the external
      address and the port range of *Options → Connection*; listing and download.
- [ ] **PASV refused**: with *Switch to active mode (PORT) when the passive mode fails* ticked, a server or firewall that refuses
      `PASV`.
- [ ] **TLS**: data channel encrypted (`PROT P`) and clear (`PROT C`); listings encrypted
      separately; certificate change detected (fingerprint).
- [ ] **FXP with TLS**: `SSCN` or `CPSV` between two TLS servers; the site's *FXP from this site: never PASV* /
      *FXP to this site: always PASV*.
- [ ] **Listings**: `MLSD` off then on; `STAT -l`; `LIST -a` (hidden files); time zone
      offset; character set of a non-UTF-8 server (e.g. Windows-1252 names).
- [ ] **Keep alive**: leave a connection idle beyond the server timeout; the keep-alive
      commands are sent, or the reconnection happens.
- [ ] **Kicked / reconnection**: kill the session on the server side during a transfer;
      the transfer resumes.
- [ ] **Ident**: Ident server on, the server asks for it (glftpd); with and without *Answer only
      while a connection is being opened*.
- [ ] **Ghost login** (`!user`, `Shift`+connect) and `-password` (`Ctrl`+connect) on
      glftpd.
- [ ] **Several hosts**: first host wrong, the next one is used.
- [ ] **Proxy**: SOCKS5 and HTTP CONNECT, named proxy of the site.
- [ ] **Unfinished queue**: stop a download of several files, close the application (or
      end its process), restart: the queue is offered again, tab by tab.
- [ ] **FTPRush v2 side by side**: UltraRushFTP open, start FTPRush v2: both run; the mIRC
      DLL talks to UltraRushFTP.
- [ ] **SFTP**: password, key, key with a passphrase, Pageant, several keys; host key
      changed on the server: refused with its fingerprint.

## 3. What to look at on each server

| Server | Settings worth trying | Known behaviours to check |
|---|---|---|
| **glftpd** | FTPS explicit, TLS required for data, `-Insecure` for its self-signed certificate. | TLS session reuse on the data channel; `STAT -l` listings; `SSCN` for FXP between glftpd sites; Ident; `!user` and `-password`; `SITE` commands through `urftp raw`. |
| **vsftpd** | `ssl_enable=YES`, `require_ssl_reuse=YES` (default), `pasv_promiscuous=YES` and `port_promiscuous=YES` for FXP. | Data connection refused without TLS session reuse; `hide_ids` (owner `ftp`); no `MLSD` (falls back to `LIST`). |
| **ProFTPD** | `mod_tls` with `TLSRequired on`; `AllowForeignAddress on` for FXP. | TLS session reuse (`NoSessionReuseRequired` absent); `MLSD`; `SITE CHMOD`. |
| **FileZilla Server 1.x** | FTPS explicit and implicit, TLS 1.2 / 1.3. | TLS session resumption required; FXP refused by default (foreign `PORT`); `MLSD`; UTF-8 names. |
| **IIS FTP** | *Directory listing style* Unix **and** MS-DOS; *Require SSL*. | Windows `LIST` format (`11-07-26  08:55PM  <DIR>`); `SYST` = `Windows_NT`; virtual folders; long paths. |
| **OpenSSH** | `Subsystem sftp internal-sftp`, password and key accounts, `ChrootDirectory`. | Host key on first connection; `ed25519`, `rsa-sha2-512`, `ecdsa` keys; resume (offset write); symbolic links. |

## Results

| Server and version | Protocol | Script | By hand | Notes |
|---|---|---|---|---|
| | | | | |

Send `report.md`, `log.txt` and this table (after reading them). Each problem found becomes
a fix with a test that reproduces it on the test servers.
