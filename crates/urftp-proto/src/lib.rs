//! Moteurs de protocole.
//!
//! - [`ftp`] : client FTP écrit pour UltraRushFTP (contrôle complet des commandes :
//!   `PRET`, `MLSD`, commandes brutes, chemins longs), FTPS explicite (`AUTH TLS`, repli
//!   `AUTH SSL`) et implicite, `PBSZ 0` / `PROT P|C`, reprise de session TLS sur les canaux
//!   de données, vérification système / empreinte / sans vérification, modes passif et actif.
//! - [`proxy`] : SOCKS4, SOCKS4A, SOCKS5, HTTP Tunnel, règles de contournement.
//! - [`ident`] : serveur Ident (RFC 1413) avec nom global ou par site.
//!
//! À venir :
//! - `fxp` : FXP site à site (`PASV`/`PORT` croisés), FXP sécurisé via `SSCN` ou `CPSV`,
//!   inversion automatique du sens si un des serveurs refuse.
//! - `sftp` : client SSH/SFTP (mot de passe et clé publique).

pub mod ftp;
pub mod ident;
pub mod proxy;
