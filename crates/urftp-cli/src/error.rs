//! Typed errors of the `urftp` tool; messages follow the selected language.

use std::path::PathBuf;

use urftp_i18n::t;

use urftp_proto::ftp::FtpError;
use urftp_proto::session::SessionError;
use urftp_proto::sftp::SftpError;

/// Invalid command line (exit code 2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    /// An option needs a value.
    #[error("{}", t!("cli.err.missing_value", option = .0))]
    MissingValue(String),
    /// A numeric option received something else.
    #[error("{}", t!("cli.err.not_a_number", option = .option, value = .value))]
    NotANumber {
        /// Option name.
        option: String,
        /// Value received.
        value: String,
    },
    /// Unknown `--exists` action.
    #[error("{}", t!("cli.err.unknown_exists", value = .0))]
    UnknownExistsAction(String),
    /// Unknown `--option`.
    #[error("{}", t!("cli.err.unknown_option", option = .0))]
    UnknownOption(String),
    /// Unknown `--lang` value.
    #[error("{}", t!("cli.err.unknown_lang", value = .0))]
    UnknownLanguage(String),
    /// Unknown sub-command.
    #[error("{}", t!("cli.err.unknown_command", command = .0))]
    UnknownCommand(String),
    /// Wrong number of arguments for a sub-command.
    #[error("{}", t!("cli.err.usage", form = .0))]
    Usage(&'static str),
}

/// Invalid `Site:/path` or URL.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TargetError {
    /// URL scheme other than ftp, ftps, ftpes, ftpsi, sftp.
    #[error("{}", t!("cli.err.unknown_scheme", scheme = .0))]
    UnknownScheme(String),
    /// Port is not a number in 1..=65535.
    #[error("{}", t!("cli.err.invalid_port", port = .0))]
    InvalidPort(String),
    /// URL without host.
    #[error("{}", t!("cli.err.missing_host", target = .0))]
    MissingHost(String),
    /// `:/path` without site name.
    #[error("{}", t!("cli.err.missing_site", target = .0))]
    MissingSite(String),
}

/// Failure of a `urftp` command (exit code 1).
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// Invalid target.
    #[error(transparent)]
    Target(#[from] TargetError),
    /// The site book cannot be read.
    #[error("{}", t!("cli.err.site_book", path = .path.display(), error = .source))]
    SiteBook {
        /// File read.
        path: PathBuf,
        /// I/O or JSON error.
        source: std::io::Error,
    },
    /// No site with this name.
    #[error("{}", t!("cli.err.unknown_site", site = .0))]
    UnknownSite(String),
    /// The site is disabled.
    #[error("{}", t!("cli.err.disabled_site", site = .0))]
    DisabledSite(String),
    /// Connection or login failed.
    #[error("{}{}", t!("cli.err.connect", site = .site, error = .source), fingerprint_hint(.source))]
    Connect {
        /// Site name.
        site: String,
        /// Underlying error.
        source: SessionError,
    },
    /// A remote operation failed.
    #[error("{}", t!("cli.err.remote", path = .path, error = .source))]
    Remote {
        /// Remote path.
        path: String,
        /// Underlying error.
        source: SessionError,
    },
    /// The remote file or folder does not exist.
    #[error("{}", t!("cli.err.not_found", path = .0))]
    NotFound(String),
    /// Local file system error.
    #[error("{}", t!("cli.err.local", path = .path.display(), error = .source))]
    Local {
        /// Local path.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
    /// `raw` on an SFTP site.
    #[error("{}", t!("cli.err.raw_needs_ftp"))]
    RawNeedsFtp,
    /// The server answered at least one raw command with an error code.
    #[error("{}", t!("cli.err.raw_refused"))]
    RawRefused,
    /// FXP between a site and itself.
    #[error("{}", t!("cli.err.fxp_same_site"))]
    FxpSameSite,
    /// Some queue items were not transferred.
    #[error("{}", t!("cli.err.incomplete", n = .0))]
    Incomplete(usize),
    /// The async runtime could not start.
    #[error("{}", t!("cli.err.runtime", error = .0))]
    Runtime(#[source] std::io::Error),
}

/// Suggests `--fingerprint` when a certificate or host key was refused.
fn fingerprint_hint(e: &SessionError) -> String {
    match e {
        SessionError::Ftp(FtpError::Certificate { .. }) | SessionError::Sftp(SftpError::HostKey { .. }) => {
            t!("cli.err.fingerprint_hint")
        }
        _ => String::new(),
    }
}
