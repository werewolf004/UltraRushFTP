//! FTP client (RFC 959, 2389, 2428, 3659) written for UltraRushFTP.
//!
//! The control stream and the data streams are [`BoxStream`]s: TLS (FTPS) and proxies
//! plug in without changing the client.

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

/// Generic network stream (TCP, TLS, proxy tunnel...).
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

/// Boxed generic network stream.
pub type BoxStream = Box<dyn AsyncStream>;

/// Errors of the FTP client.
#[derive(Debug, thiserror::Error)]
pub enum FtpError {
    #[error("network error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timed out")]
    Timeout,
    #[error("connection closed by the server")]
    Closed,
    #[error("unexpected reply to \"{command}\": {reply}")]
    Unexpected { command: String, reply: Reply },
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("proxy: {0}")]
    Proxy(#[from] crate::proxy::ProxyError),
    /// Certificate refused; `fingerprint` (SHA-256) allows offering to trust it.
    #[error("server certificate refused ({reason}) — SHA-256 fingerprint: {fingerprint}")]
    Certificate { fingerprint: String, reason: String },
}

impl FtpError {
    /// Server reply code when the error comes from one.
    pub fn reply_code(&self) -> Option<u16> {
        match self {
            FtpError::Unexpected { reply, .. } => Some(reply.code),
            _ => None,
        }
    }
}

pub type Result<T, E = FtpError> = std::result::Result<T, E>;
