//! Moteurs de protocole.
//!
//! - [`ftp`] : client FTP écrit pour UltraRushFTP (contrôle complet des commandes :
//!   `PRET`, `MLSD`, commandes brutes, chemins longs), FTPS explicite (`AUTH TLS`, repli
//!   `AUTH SSL`) et implicite, `PBSZ 0` / `PROT P|C`, reprise de session TLS sur les canaux
//!   de données, vérification système / empreinte / sans vérification, modes passif et actif,
//!   et FXP site à site ([`ftp::fxp`]) : clair ou chiffré (`SSCN`, `CPSV`), inversion auto.
//! - [`proxy`] : SOCKS4, SOCKS4A, SOCKS5, HTTP Tunnel, règles de contournement.
//! - [`ident`] : serveur Ident (RFC 1413) avec nom global ou par site.
//! - [`sftp`] : client SFTP (SSH) par mot de passe ou clé (OpenSSH, PEM, PuTTY), clé d'hôte
//!   vérifiée par empreinte, proxy.

pub mod ftp;
pub mod ident;
pub mod proxy;
pub mod sftp;
