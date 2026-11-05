//! Enregistrement du carnet de sites sur disque (`sites.json`).
//!
//! Emplacement : `%APPDATA%\UltraRushFTP\` par défaut, ou le dossier donné par la variable
//! `URUSHFTP_DATA` (mode portable, équivalent de `ftprush.exe -f`). Écriture atomique
//! (fichier temporaire puis renommage) et copie de sauvegarde `sites.json.bak`.

use std::io;
use std::path::{Path, PathBuf};

use crate::site::SiteBook;

pub const FILE_NAME: &str = "sites.json";

/// Data folder chosen on the command line (`-f` / `--data`), set once at startup.
static DATA_DIR_OVERRIDE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Uses `dir` as the data folder for the rest of the process (portable mode, `-f`).
/// Returns `false` if a folder was already chosen (the first one is kept).
pub fn set_data_dir(dir: PathBuf) -> bool {
    DATA_DIR_OVERRIDE.set(dir).is_ok()
}

/// Application data folder: command-line choice, then `URUSHFTP_DATA`, then
/// `%APPDATA%/UltraRushFTP`.
pub fn data_dir() -> PathBuf {
    if let Some(d) = DATA_DIR_OVERRIDE.get() {
        return d.clone();
    }
    if let Some(d) = std::env::var_os("URUSHFTP_DATA") {
        return PathBuf::from(d);
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("UltraRushFTP")
}

pub fn default_path() -> PathBuf {
    data_dir().join(FILE_NAME)
}

/// Charge le carnet ; un fichier absent donne un carnet vide.
pub fn load(path: &Path) -> io::Result<SiteBook> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(SiteBook::default()),
        Err(e) => Err(e),
    }
}

pub fn save(path: &Path, book: &SiteBook) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(book).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("json.bak"));
    }
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::site::Site;

    #[test]
    fn save_and_load() {
        let dir = std::env::temp_dir().join(format!("urftp-store-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        assert_eq!(load(&path).unwrap(), SiteBook::default());
        let mut book = SiteBook::default();
        book.add_site(&[], Site { name: "A".into(), password: "pw".into(), ..Site::default() });
        save(&path, &book).unwrap();
        save(&path, &book).unwrap();
        assert!(path.with_extension("json.bak").exists());
        assert_eq!(load(&path).unwrap(), book);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
