//! Backup and restore of the configuration: the files of the data folder (sites, options,
//! statistics, layouts, unfinished queue) copied into `backup/<date and time>`.
//!
//! The site passwords in the copy stay protected by Windows (DPAPI): a backup restores on
//! the same Windows account.

use std::io;
use std::path::{Path, PathBuf};

/// Folders of the data folder that are not configuration.
const SKIPPED: [&str; 2] = ["backup", "logs"];

/// Folder of the backups of a data folder.
pub fn backup_root(data: &Path) -> PathBuf {
    data.join("backup")
}

/// Copies the files of `data` (not its folders) into `backup/<stamp>`; returns that folder.
/// `stamp` is a sortable date such as `2026-11-08_143000`.
pub fn backup(data: &Path, stamp: &str) -> io::Result<PathBuf> {
    let dest = backup_root(data).join(stamp);
    std::fs::create_dir_all(&dest)?;
    for e in std::fs::read_dir(data)?.flatten() {
        let name = e.file_name();
        if e.file_type()?.is_file() && !SKIPPED.iter().any(|s| name.eq_ignore_ascii_case(s)) {
            std::fs::copy(e.path(), dest.join(&name))?;
        }
    }
    Ok(dest)
}

/// Backups found, newest first.
pub fn list(data: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(backup_root(data))
        .map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v.reverse();
    v
}

/// Keeps the `keep` newest backups.
pub fn prune(data: &Path, keep: usize) -> io::Result<()> {
    for old in list(data).into_iter().skip(keep) {
        std::fs::remove_dir_all(old)?;
    }
    Ok(())
}

/// A backup of today (`date` = `YYYY-MM-DD`) exists.
pub fn has_backup_of(data: &Path, date: &str) -> bool {
    list(data).iter().any(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(date)))
}

/// Copies the files of a backup back into `data`. The application must stop right after,
/// without saving, or it would write its own state over them.
pub fn restore(data: &Path, from: &Path) -> io::Result<usize> {
    let mut n = 0;
    for e in std::fs::read_dir(from)?.flatten() {
        if e.file_type()?.is_file() {
            std::fs::copy(e.path(), data.join(e.file_name()))?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_prune_and_restore() {
        let data = std::env::temp_dir().join(format!("urftp-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(data.join("logs")).unwrap();
        std::fs::write(data.join("settings.json"), "v1").unwrap();
        std::fs::write(data.join("logs").join("x.log"), "log").unwrap();
        let b1 = backup(&data, "2026-11-07_100000").unwrap();
        assert!(b1.join("settings.json").exists() && !b1.join("logs").exists());
        std::fs::write(data.join("settings.json"), "v2").unwrap();
        backup(&data, "2026-11-08_100000").unwrap();
        backup(&data, "2026-11-08_120000").unwrap();
        assert!(has_backup_of(&data, "2026-11-08") && !has_backup_of(&data, "2026-11-09"));
        prune(&data, 2).unwrap();
        let left: Vec<String> = list(&data).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(left, ["2026-11-08_120000", "2026-11-08_100000"]);
        assert!(!b1.exists(), "the oldest backup was pruned");
        std::fs::write(data.join("settings.json"), "v3").unwrap();
        assert_eq!(restore(&data, &list(&data)[1]).unwrap(), 1);
        assert_eq!(std::fs::read_to_string(data.join("settings.json")).unwrap(), "v2");
        let _ = std::fs::remove_dir_all(&data);
    }
}
