//! Moteurs de protocole.
//!
//! - [`ftp`] : client FTP écrit pour UltraRushFTP (contrôle complet des commandes :
//!   `PRET`, `MLSD`, commandes brutes, chemins longs) — base du FTPS et du FXP.
//!
//! À venir :
//! - `ftps` : TLS explicite (`AUTH TLS`, `PBSZ 0`, `PROT P/C`) et implicite.
//! - `fxp` : FXP site à site (`PASV`/`PORT` croisés), FXP sécurisé via `SSCN` ou `CPSV`,
//!   inversion automatique du sens si un des serveurs refuse.
//! - `sftp` : client SSH/SFTP (mot de passe et clé publique).

pub mod ftp;
