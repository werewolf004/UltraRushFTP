//! Data model of UltraRushFTP, independent of the user interface and the network.
//!
//! - [`queue`]: queue items (upload / download / FXP), order, states, per-site limits
//! - [`skiplist`]: allow/skip rules (wildcard, regex, `%W%` full path), per transfer direction
//! - [`priority`]: priority list
//! - [`exists`]: "existing file" rules
//! - [`pattern`]: wildcard / regex masks
//! - [`localpath`]: long local paths
//!
//! Also: [`site`] / [`sitestore`] (site manager), [`settings`] (options), [`import`]
//! (FileZilla, FTPRush, FlashFXP site books), [`secret`] (DPAPI-protected passwords),
//! [`localpath`] (paths longer than `MAX_PATH`). Tabs (FTPRush "containers") and
//! statistics live in the interface crate.

pub mod exists;
pub mod import;
pub mod localpath;
pub mod pattern;
pub mod priority;
pub mod queue;
pub mod queuestore;
pub mod secret;
pub mod settings;
pub mod site;
pub mod sitestore;
pub mod skiplist;

/// Ident (RFC 1413) name answered to FTP servers by default.
pub const DEFAULT_IDENT_NAME: &str = "URUSHFTP";
/// Default TCP port of the Ident server.
pub const DEFAULT_IDENT_PORT: u16 = 113;

/// Direction of a transfer, as in FTPRush (`RS_UP`, `RS_DOWN`, FXP by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferKind {
    Upload,
    Download,
    Fxp,
}

/// Supported protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    Ftp,
    /// Explicit FTPS (`AUTH TLS` on the control port).
    FtpsExplicit,
    /// Implicit FTPS (TLS from the start, port 990 by default).
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
