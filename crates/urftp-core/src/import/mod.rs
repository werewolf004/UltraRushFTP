//! Import de carnets de sites d'autres clients FTP, avec détection automatique du format.
//!
//! Gérés : FileZilla (`sitemanager.xml` et exports XML), FTPRush v3 (`site.json`),
//! FTPRush v1/v2 (`RushSite.xml`) et FlashFXP (`.ftp`). Pour ces deux derniers, les mots de
//! passe ne sont pas importés : ils sont à ressaisir.

mod filezilla;
mod flashfxp;
mod ftprush2;
mod ftprush3;

use std::path::Path;

use crate::site::SiteGroup;

/// Format détecté.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportFormat {
    FileZilla,
    FtpRush3,
    FtpRush2,
    FlashFxp,
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

/// Résultat d'un import : un groupe prêt à greffer dans le carnet, et les remarques.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub format: ImportFormat,
    pub group: SiteGroup,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("lecture du fichier : {0}")]
    Io(#[from] std::io::Error),
    #[error("format de fichier non reconnu")]
    UnknownFormat,
    #[error("fichier {format} illisible : {message}", format = .0.label(), message = .1)]
    Invalid(ImportFormat, String),
}

/// Devine le format d'après le nom et le contenu.
pub fn detect(file_name: &str, text: &str) -> Option<ImportFormat> {
    let head: String = text.chars().take(4096).collect();
    let lower = head.to_lowercase();
    let name = file_name.to_lowercase();
    if lower.contains("<filezilla3") {
        Some(ImportFormat::FileZilla)
    } else if head.trim_start().starts_with('{') && head.contains("\"RootItem\"") {
        Some(ImportFormat::FtpRush3)
    } else if lower.contains("<sites") && head.contains(" UID=\"") {
        // FTPRush v1/v2 : chaque site porte un attribut UID.
        Some(ImportFormat::FtpRush2)
    } else if lower.contains("flashfxp") || name.ends_with(".ftp") || (lower.contains("<sites") && head.contains("<ADDRESS>")) {
        Some(ImportFormat::FlashFxp)
    } else if name.contains("rushsite") || lower.contains("ftprush") || lower.contains("rushsite") {
        Some(ImportFormat::FtpRush2)
    } else {
        None
    }
}

/// Importe le texte d'un fichier.
pub fn import_text(file_name: &str, text: &str) -> Result<ImportResult, ImportError> {
    let text = text.trim_start_matches('\u{feff}');
    match detect(file_name, text).ok_or(ImportError::UnknownFormat)? {
        ImportFormat::FileZilla => filezilla::import(text),
        ImportFormat::FtpRush3 => ftprush3::import(text),
        ImportFormat::FtpRush2 => ftprush2::import(text),
        ImportFormat::FlashFxp => flashfxp::import(text),
    }
}

/// Importe un fichier (UTF-8, UTF-8 avec BOM ou UTF-16).
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
        // Même racine <SITES> : FTPRush (attribut UID) ou FlashFXP (balise ADDRESS).
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
