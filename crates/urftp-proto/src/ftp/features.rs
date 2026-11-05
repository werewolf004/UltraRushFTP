//! Capacités annoncées par le serveur via `FEAT` (RFC 2389).

use std::collections::HashMap;

use super::Reply;

/// Ensemble des fonctionnalités annoncées (`MLST`, `EPSV`, `PRET`, `AUTH`, `SSCN`, `CPSV`…).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Features {
    map: HashMap<String, String>,
}

impl Features {
    /// Analyse une réponse `211` à `FEAT` : chaque ligne intermédiaire est `NOM [paramètres]`.
    pub fn parse(reply: &Reply) -> Self {
        let mut map = HashMap::new();
        let n = reply.lines.len();
        if n >= 3 {
            for line in &reply.lines[1..n - 1] {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let (name, param) = line.split_once(' ').unwrap_or((line, ""));
                map.insert(name.to_ascii_uppercase(), param.trim().to_string());
            }
        }
        Self { map }
    }

    /// Le serveur annonce-t-il cette fonctionnalité (nom insensible à la casse) ?
    pub fn has(&self, name: &str) -> bool {
        self.map.contains_key(&name.to_ascii_uppercase())
    }

    /// Paramètres annoncés pour une fonctionnalité (ex. `TLS` pour `AUTH`).
    pub fn param(&self, name: &str) -> Option<&str> {
        self.map.get(&name.to_ascii_uppercase()).map(String::as_str)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_feat() {
        let reply = Reply {
            code: 211,
            lines: ["211-Features:", " MLST type*;size*;modify*;", " pret", " AUTH TLS", " UTF8", "211 End"]
                .map(String::from)
                .to_vec(),
        };
        let f = Features::parse(&reply);
        assert!(f.has("MLST") && f.has("PRET") && f.has("utf8"));
        assert_eq!(f.param("auth"), Some("TLS"));
        assert!(!f.has("EPSV"));
    }
}
