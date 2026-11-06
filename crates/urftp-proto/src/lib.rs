//! Protocol engines.
//!
//! - [`ftp`]: FTP client written for UltraRushFTP (full control of the commands:
//!   `PRET`, `MLSD`, raw commands, long paths), explicit FTPS (`AUTH TLS`, `AUTH SSL`
//!   fallback) and implicit FTPS, `PBSZ 0` / `PROT P|C`, TLS session reuse on the data
//!   channels, system / fingerprint / no verification, passive and active modes, and
//!   site-to-site FXP ([`ftp::fxp`]): clear or encrypted (`SSCN`, `CPSV`), auto-reverse.
//! - [`proxy`]: SOCKS4, SOCKS4A, SOCKS5, HTTP tunnel, bypass rules.
//! - [`ident`]: Ident server (RFC 1413) with a global or per-site name.
//! - [`sftp`]: SFTP (SSH) client with password or key (OpenSSH, PEM, PuTTY), host key
//!   verified by fingerprint, proxy.

//! - [`session`]: common FTP/SFTP session and site profile.
//! - [`keepalive`]: anti-idle commands and automatic reconnection.
//! - [`browse`]: browsing connection of a remote panel (keep alive, reconnection).
//! - [`engine`]: queue engine (multi-transfer, per-site limits, folders, skip list,
//!   priorities, "existing file" rules, retries, stop).
//! - [`throttle`]: speed limiting.

pub mod browse;
pub mod engine;
pub mod ftp;
pub mod ident;
pub mod keepalive;
pub mod profile;
pub mod proxy;
pub mod session;
pub mod sftp;
pub mod throttle;
