//! Chemins distants (séparateur `/`). Aucune limite de longueur n'est imposée ici.

/// Normalise un chemin absolu : supprime `.`, résout `..`, retire les `/` en double.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

/// Résout `path` par rapport au répertoire absolu `base`.
pub fn join(base: &str, path: &str) -> String {
    if path.starts_with('/') {
        normalize(path)
    } else {
        normalize(&format!("{base}/{path}"))
    }
}

/// Sépare un chemin en (répertoire parent, nom). `"a"` → `(None, "a")`, `"/a"` → `(Some("/"), "a")`.
pub fn split(path: &str) -> (Option<&str>, &str) {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => (Some("/"), &trimmed[1..]),
        Some(i) => (Some(&trimmed[..i]), &trimmed[i + 1..]),
        None => (None, trimmed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_join() {
        assert_eq!(normalize("/a/./b//c/../d/"), "/a/b/d");
        assert_eq!(normalize("/.."), "/");
        assert_eq!(join("/pub", "x/y"), "/pub/x/y");
        assert_eq!(join("/pub", "/abs"), "/abs");
        assert_eq!(join("/pub/x", ".."), "/pub");
    }

    #[test]
    fn split_paths() {
        assert_eq!(split("/a/b/c.txt"), (Some("/a/b"), "c.txt"));
        assert_eq!(split("/c.txt"), (Some("/"), "c.txt"));
        assert_eq!(split("c.txt"), (None, "c.txt"));
        assert_eq!(split("/a/dir/"), (Some("/a"), "dir"));
    }
}
