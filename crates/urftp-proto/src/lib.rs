//! Moteurs de protocole.
//!
//! Prévu :
//! - `ftp` : client FTP écrit pour UltraRushFTP (contrôle complet des commandes :
//!   `PRET`, `STAT -l`, `MLSD`, `XCRC`, `SITE`, commandes brutes) — nécessaire pour le FXP.
//! - `ftps` : TLS explicite (`AUTH TLS`, `PBSZ 0`, `PROT P/C`) et implicite.
//! - `fxp` : FXP site à site (`PASV`/`PORT` croisés), FXP sécurisé via `SSCN` ou `CPSV`,
//!   inversion automatique du sens si un des serveurs refuse.
//! - `sftp` : client SSH/SFTP (mot de passe et clé publique).
