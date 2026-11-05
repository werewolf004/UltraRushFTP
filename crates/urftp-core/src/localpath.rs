//! Chemins locaux longs (au-delà de `MAX_PATH` = 260 caractères sous Windows).
//!
//! `std::fs` ajoute déjà le préfixe `\\?\` quand c'est nécessaire. Ce module sert aux
//! appels Win32 directs (shell, dialogues, API qui ne passent pas par `std::fs`) :
//! [`to_verbatim`] produit un chemin absolu normalisé au format `\\?\C:\…` ou `\\?\UNC\…`.

use std::path::{Component, Path, PathBuf, Prefix};

/// Convertit un chemin absolu en chemin « verbatim » Windows, sans limite de longueur.
///
/// Les composants `.` et `..` sont résolus (le format verbatim ne les interprète pas) et les
/// `/` sont remplacés par `\`. Un chemin relatif, déjà verbatim, ou non représentable en
/// UTF-8 est renvoyé tel quel. Hors Windows, la fonction renvoie le chemin inchangé.
pub fn to_verbatim(path: &Path) -> PathBuf {
    if cfg!(windows) { verbatim_windows(path).unwrap_or_else(|| path.to_path_buf()) } else { path.to_path_buf() }
}

fn verbatim_windows(path: &Path) -> Option<PathBuf> {
    let mut head = None;
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => {
                head = Some(match p.kind() {
                    Prefix::Disk(letter) => format!(r"\\?\{}:", char::from(letter)),
                    Prefix::UNC(server, share) => {
                        format!(r"\\?\UNC\{}\{}", server.to_str()?, share.to_str()?)
                    }
                    // Déjà verbatim ou chemin de périphérique : ne pas toucher.
                    _ => return None,
                });
            }
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => {
                parts.pop();
            }
            Component::Normal(n) => parts.push(n.to_str()?),
        }
    }
    let mut out = head?;
    if parts.is_empty() {
        out.push('\\');
    }
    for p in parts {
        out.push('\\');
        out.push_str(p);
    }
    Some(PathBuf::from(out))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn disk_path_is_normalized() {
        assert_eq!(to_verbatim(Path::new(r"C:\a\.\b\..\c/d")), PathBuf::from(r"\\?\C:\a\c\d"));
        assert_eq!(to_verbatim(Path::new(r"D:\")), PathBuf::from(r"\\?\D:\"));
    }

    #[test]
    fn unc_path() {
        assert_eq!(to_verbatim(Path::new(r"\\nas\share\x\y")), PathBuf::from(r"\\?\UNC\nas\share\x\y"));
    }

    #[test]
    fn verbatim_and_relative_are_unchanged() {
        assert_eq!(to_verbatim(Path::new(r"\\?\C:\x")), PathBuf::from(r"\\?\C:\x"));
        assert_eq!(to_verbatim(Path::new(r"rel\x")), PathBuf::from(r"rel\x"));
    }

    /// Crée, écrit, relit et supprime une arborescence de plus de 600 caractères.
    #[test]
    fn std_fs_handles_paths_longer_than_max_path() {
        let root = std::env::temp_dir().join(format!("urftp-long-{}", std::process::id()));
        let mut deep = root.clone();
        for i in 0..12 {
            deep.push(format!("dossier_tres_long_numero_{i:02}_{}", "x".repeat(30)));
        }
        let file = deep.join("fichier.bin");
        assert!(file.as_os_str().len() > 600);

        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(&file, b"UltraRushFTP").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"UltraRushFTP");
        assert_eq!(std::fs::read(to_verbatim(&file)).unwrap(), b"UltraRushFTP");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
