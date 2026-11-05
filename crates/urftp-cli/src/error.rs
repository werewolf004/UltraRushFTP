//! Typed errors of the `urftp` tool.

use std::path::PathBuf;

use urftp_proto::ftp::FtpError;
use urftp_proto::session::SessionError;
use urftp_proto::sftp::SftpError;

/// Invalid command line (exit code 2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    /// An option needs a value.
    #[error("{0} : valeur manquante")]
    MissingValue(String),
    /// A numeric option received something else.
    #[error("{option} : nombre attendu, pas « {value} »")]
    NotANumber {
        /// Option name.
        option: String,
        /// Value received.
        value: String,
    },
    /// Unknown `--exists` action.
    #[error("--exists : action inconnue « {0} »")]
    UnknownExistsAction(String),
    /// Unknown `--option`.
    #[error("option inconnue : {0}")]
    UnknownOption(String),
    /// Unknown sub-command.
    #[error("commande inconnue : {0} (urftp --help)")]
    UnknownCommand(String),
    /// Wrong number of arguments for a sub-command.
    #[error("utilisation : urftp {0}")]
    Usage(&'static str),
}

/// Invalid `Site:/path` or URL.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetError {
    /// URL scheme other than ftp, ftps, ftpes, ftpsi, sftp.
    #[error("protocole inconnu : {0}")]
    UnknownScheme(String),
    /// Port is not a number in 1..=65535.
    #[error("port invalide : {0}")]
    InvalidPort(String),
    /// URL without host.
    #[error("hôte manquant : {0}")]
    MissingHost(String),
    /// `:/path` without site name.
    #[error("site manquant : {0}")]
    MissingSite(String),
}

/// Failure of a `urftp` command (exit code 1).
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Invalid target.
    #[error(transparent)]
    Target(#[from] TargetError),
    /// The site book cannot be read.
    #[error("gestionnaire de sites illisible ({path}) : {source}")]
    SiteBook {
        /// File read.
        path: PathBuf,
        /// I/O or JSON error.
        source: std::io::Error,
    },
    /// No site with this name.
    #[error("site inconnu : {0} (voir urftp sites)")]
    UnknownSite(String),
    /// The site is disabled.
    #[error("le site {0} est désactivé")]
    DisabledSite(String),
    /// Connection or login failed.
    #[error("{site} : {source}{}", fingerprint_hint(source))]
    Connect {
        /// Site name.
        site: String,
        /// Underlying error.
        source: SessionError,
    },
    /// A remote operation failed.
    #[error("{path} : {source}")]
    Remote {
        /// Remote path.
        path: String,
        /// Underlying error.
        source: SessionError,
    },
    /// The remote file or folder does not exist.
    #[error("introuvable : {0}")]
    NotFound(String),
    /// Local file system error.
    #[error("{path} : {source}")]
    Local {
        /// Local path.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// `raw` on an SFTP site.
    #[error("raw : commandes brutes disponibles en FTP/FTPS seulement")]
    RawNeedsFtp,
    /// The server answered at least one raw command with an error code.
    #[error("le serveur a refusé au moins une commande")]
    RawRefused,
    /// FXP between a site and itself.
    #[error("FXP : la source et la destination doivent être deux sites différents")]
    FxpSameSite,
    /// Some queue items were not transferred.
    #[error("{0} élément(s) non transféré(s)")]
    Incomplete(usize),
    /// The async runtime could not start.
    #[error("démarrage impossible : {0}")]
    Runtime(#[source] std::io::Error),
}

/// Suggests `--fingerprint` when a certificate or host key was refused.
fn fingerprint_hint(e: &SessionError) -> &'static str {
    match e {
        SessionError::Ftp(FtpError::Certificate { .. }) | SessionError::Sftp(SftpError::HostKey { .. }) => {
            "\n  (vérifiez l'empreinte puis relancez avec --fingerprint <empreinte>)"
        }
        _ => "",
    }
}
