# Changelog

**English** | [Français](CHANGELOG.fr.md)

All notable changes to UltraRushFTP. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Each section below becomes the notes of the
GitHub release with the same tag.

## [Unreleased]

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

[Unreleased]: https://github.com/werewolf004/UltraRushFTP/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.1.0
