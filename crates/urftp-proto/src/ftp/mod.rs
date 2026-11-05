//! Client FTP (RFC 959, 2389, 2428, 3659) écrit pour UltraRushFTP.
//!
//! Le flux de contrôle et les flux de données sont des [`BoxStream`] : TLS (FTPS) et
//! proxys viendront s'insérer sans changer le client.

mod client;
mod features;
mod fxp;
pub mod listing;
pub mod path;
mod reply;
mod tls;

use tokio::io::{AsyncRead, AsyncWrite};

pub use client::{FtpClient, FtpConfig, LogLine, LogSink, TransferType};
pub use features::Features;
pub use fxp::{FxpMode, FxpOptions, FxpOutcome, FxpSecurity, fxp};
pub use listing::{EntryKind, FtpTime, RemoteEntry};
pub use reply::Reply;
pub use tls::{Security, TlsOptions, TlsVerify, fingerprint_sha256};

/// Flux réseau générique (TCP, TLS, tunnel proxy…).
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

/// Flux réseau générique en boîte.
pub type BoxStream = Box<dyn AsyncStream>;

/// Erreurs du client FTP.
#[derive(Debug, thiserror::Error)]
pub enum FtpError {
    #[error("erreur réseau : {0}")]
    Io(#[from] std::io::Error),
    #[error("délai dépassé")]
    Timeout,
    #[error("connexion fermée par le serveur")]
    Closed,
    #[error("réponse inattendue à « {command} » : {reply}")]
    Unexpected { command: String, reply: Reply },
    #[error("erreur de protocole : {0}")]
    Protocol(String),
    #[error("argument invalide : {0}")]
    InvalidArgument(String),
    #[error("erreur TLS : {0}")]
    Tls(String),
    #[error("proxy : {0}")]
    Proxy(#[from] crate::proxy::ProxyError),
    /// Certificat refusé ; `fingerprint` (SHA-256) permet de proposer de l'approuver.
    #[error("certificat du serveur refusé ({reason}) — empreinte SHA-256 : {fingerprint}")]
    Certificate { fingerprint: String, reason: String },
}

impl FtpError {
    /// Code de la réponse serveur si l'erreur en provient.
    pub fn reply_code(&self) -> Option<u16> {
        match self {
            FtpError::Unexpected { reply, .. } => Some(reply.code),
            _ => None,
        }
    }
}

pub type Result<T, E = FtpError> = std::result::Result<T, E>;
