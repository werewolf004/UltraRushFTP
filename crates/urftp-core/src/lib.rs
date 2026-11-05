//! Modèle de données d'UltraRushFTP, indépendant de l'interface et du réseau.
//!
//! - [`queue`] : éléments de file (upload / download / FXP), ordre, états, limites par site
//! - [`skiplist`] : règles allow/skip (joker, regex, `%W%` chemin complet), par sens de transfert
//! - [`priority`] : liste de priorités
//! - [`exists`] : règles « fichier existant »
//! - [`pattern`] : masques joker / regex
//! - [`localpath`] : chemins locaux longs
//!
//! Prévu :
//! - `site` : gestionnaire de sites (groupes, protocole, TLS, proxy, signets)
//! - `container` : onglets (« containers » dans FTPRush), chacun avec deux panneaux et sa file
//! - `stats` : compteurs totaux et de session exposés à `RushApp.*`

pub mod exists;
pub mod import;
pub mod localpath;
pub mod pattern;
pub mod priority;
pub mod queue;
pub mod secret;
pub mod settings;
pub mod site;
pub mod sitestore;
pub mod skiplist;

/// Nom Ident (RFC 1413) répondu par défaut aux serveurs FTP.
pub const DEFAULT_IDENT_NAME: &str = "URUSHFTP";
/// Port TCP par défaut du serveur Ident.
pub const DEFAULT_IDENT_PORT: u16 = 113;

/// Sens d'un transfert, comme dans FTPRush (`RS_UP`, `RS_DOWN`, FXP par défaut).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferKind {
    Upload,
    Download,
    Fxp,
}

/// Protocoles de la phase 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Ftp,
    /// FTPS explicite (`AUTH TLS` sur le port de contrôle).
    FtpsExplicit,
    /// FTPS implicite (TLS dès la connexion, port 990 par défaut).
    FtpsImplicit,
    Sftp,
}

impl Protocol {
    pub const fn default_port(self) -> u16 {
        match self {
            Protocol::Ftp | Protocol::FtpsExplicit => 21,
            Protocol::FtpsImplicit => 990,
            Protocol::Sftp => 22,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ports() {
        assert_eq!(Protocol::Ftp.default_port(), 21);
        assert_eq!(Protocol::FtpsImplicit.default_port(), 990);
        assert_eq!(Protocol::Sftp.default_port(), 22);
    }
}
