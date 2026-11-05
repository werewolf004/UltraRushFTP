//! Import of the site books of other FTP clients, with automatic format detection.
//!
//! Supported: FileZilla (`sitemanager.xml` and XML exports), FTPRush v3 (`site.json`),
//! FTPRush v1/v2 (`RushSite.xml`) and FlashFXP (`.ftp`). For the last two, passwords are
//! not imported: they must be entered again.

mod filezilla;
mod flashfxp;
mod ftprush2;
mod ftprush3;

use std::path::Path;

use crate::site::SiteGroup;

/// Detected format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportFormat {
    FileZilla,
    FtpRush3,
    FtpRush2,
    FlashFxp,
}

impl std::fmt::Display for ImportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl ImportFormat {
    pub fn label(self) -> &'static str {
        match self {
            ImportFormat::FileZilla => "FileZilla",
            ImportFormat::FtpRush3 => "FTPRush v3",
            ImportFormat::FtpRush2 => "FTPRush v1/v2",
            ImportFormat::FlashFxp => "FlashFXP",
        }
    }
}

/// Result of an import: a group ready to be grafted into the site book, and remarks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub format: ImportFormat,
    pub group: SiteGroup,
    pub warnings: Vec<ImportWarning>,
}

/// Remark about an import (the interface translates them; `Display` is English).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportWarning {
    /// The file contains no site.
    #[error("no site found")]
    NoSites,
    /// FileZilla "FTP with TLS if available" sites were imported as explicit FTPS.
    #[error(
        "\"FTP with TLS if available\" sites are imported as explicit FTPS; switch them back to FTP if the server does not support TLS"
    )]
    TlsIfAvailable,
    /// A site uses a protocol UltraRushFTP does not support; it was skipped.
    #[error("\"{site}\" skipped: {source_format} protocol {protocol} not supported")]
    UnsupportedProtocol {
        /// Site name.
        site: String,
        /// Client that produced the file.
        source_format: ImportFormat,
        /// Protocol code or name in that file.
        protocol: String,
    },
    /// A protocol or TLS code is unknown; the site was imported with a guess.
    #[error("\"{site}\": unknown code {code}, imported as {assumed}; please check")]
    UnknownCode {
        /// Site name.
        site: String,
        /// Unknown code.
        code: String,
        /// Protocol used instead.
        assumed: crate::site::SiteProtocol,
    },
    /// The stored password could not be decoded.
    #[error("\"{0}\": password unreadable")]
    PasswordUnreadable(String),
    /// FileZilla password protected by its master password.
    #[error("\"{0}\": password protected by the FileZilla master password, not imported")]
    MasterPassword(String),
    /// Formats whose passwords are never imported (FTPRush v1/v2, FlashFXP).
    #[error("passwords not imported: enter them again for the {0} site(s)")]
    PasswordsNotImported(usize),
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("reading the file: {0}")]
    Io(#[from] std::io::Error),
    #[error("unrecognised file format")]
    UnknownFormat,
    #[error("{format} file unreadable: {message}", format = .0.label(), message = .1)]
    Invalid(ImportFormat, String),
}

/// Guesses the format from the name and the contents.
pub fn detect(file_name: &str, text: &str) -> Option<ImportFormat> {
    let head: String = text.chars().take(4096).collect();
    let lower = head.to_lowercase();
    let name = file_name.to_lowercase();
    if lower.contains("<filezilla3") {
        Some(ImportFormat::FileZilla)
    } else if head.trim_start().starts_with('{') && head.contains("\"RootItem\"") {
        Some(ImportFormat::FtpRush3)
    } else if lower.contains("<sites") && head.contains(" UID=\"") {
        // FTPRush v1/v2: every site has a UID attribute.
        Some(ImportFormat::FtpRush2)
    } else if lower.contains("flashfxp") || name.ends_with(".ftp") || (lower.contains("<sites") && head.contains("<ADDRESS>")) {
        Some(ImportFormat::FlashFxp)
    } else if name.contains("rushsite") || lower.contains("ftprush") || lower.contains("rushsite") {
        Some(ImportFormat::FtpRush2)
    } else {
        None
    }
}

/// Imports the text of a file.
pub fn import_text(file_name: &str, text: &str) -> Result<ImportResult, ImportError> {
    let text = text.trim_start_matches('\u{feff}');
    match detect(file_name, text).ok_or(ImportError::UnknownFormat)? {
        ImportFormat::FileZilla => filezilla::import(text),
        ImportFormat::FtpRush3 => ftprush3::import(text),
        ImportFormat::FtpRush2 => ftprush2::import(text),
        ImportFormat::FlashFxp => flashfxp::import(text),
    }
}

/// Imports a file (UTF-8, UTF-8 with BOM or UTF-16).
pub fn import_file(path: &Path) -> Result<ImportResult, ImportError> {
    let bytes = std::fs::read(path)?;
    let text = decode_text(&bytes);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    import_text(&name, &text)
}

fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |le: bool, b: &[u8]| {
        let (pairs, _) = b.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|&c| if le { u16::from_le_bytes(c) } else { u16::from_be_bytes(c) }).collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xFF, 0xFE, rest @ ..] => utf16(true, rest),
        [0xFE, 0xFF, rest @ ..] => utf16(false, rest),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection() {
        assert_eq!(detect("sitemanager.xml", "<?xml?><FileZilla3 version=\"3\">"), Some(ImportFormat::FileZilla));
        assert_eq!(detect("site.json", "{\n \"RootItem\": {}}"), Some(ImportFormat::FtpRush3));
        assert_eq!(detect("RushSite.xml", "<?xml?><root/>"), Some(ImportFormat::FtpRush2));
        assert_eq!(detect("export.ftp", "<x/>"), Some(ImportFormat::FlashFxp));
        assert_eq!(detect("notes.txt", "bonjour"), None);
        // Same <SITES> root: FTPRush (UID attribute) or FlashFXP (ADDRESS tag).
        assert_eq!(detect("export.xml", "<SITES><A NAME=\"a\" UID=\"01\"/></SITES>"), Some(ImportFormat::FtpRush2));
        assert_eq!(detect("export.xml", "<SITES VERSION=\"1.3\"><A><ADDRESS>h</ADDRESS></A></SITES>"), Some(ImportFormat::FlashFxp));
        assert!(import_text("RushSite.xml", "<SITES/>").unwrap().group.sites.is_empty());
    }

    #[test]
    fn utf16_files() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "{\"RootItem\":1}".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_text(&bytes), "{\"RootItem\":1}");
    }
}
