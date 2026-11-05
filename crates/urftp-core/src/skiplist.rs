//! Skiplist / allowlist (fichiers et dossiers ignorés), comme dans FTPRush v2.
//!
//! Les règles sont évaluées dans l'ordre : la première qui correspond décide (ignorer ou
//! autoriser). Un masque préfixé par `%W%` porte sur le chemin complet au lieu du nom seul.

use crate::TransferKind;
use crate::pattern::Pattern;

/// Effet d'une règle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleAction {
    Skip,
    Allow,
}

/// Éléments concernés par une règle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleTarget {
    Files,
    Dirs,
    Both,
}

/// Sens de transfert concernés par une règle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Directions {
    pub upload: bool,
    pub download: bool,
    pub fxp: bool,
}

impl Directions {
    pub const ALL: Directions = Directions { upload: true, download: true, fxp: true };

    fn contains(self, kind: TransferKind) -> bool {
        match kind {
            TransferKind::Upload => self.upload,
            TransferKind::Download => self.download,
            TransferKind::Fxp => self.fxp,
        }
    }
}

/// Une règle de skiplist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipRule {
    pub pattern: Pattern,
    pub action: RuleAction,
    pub target: RuleTarget,
    pub directions: Directions,
    /// Le masque porte sur le chemin complet (`%W%` dans la v2).
    pub full_path: bool,
}

impl SkipRule {
    /// Règle « ignorer » depuis un masque joker au format v2 (`%W%` = chemin complet).
    pub fn skip(mask: &str, target: RuleTarget) -> Self {
        Self::from_mask(mask, RuleAction::Skip, target)
    }

    /// Règle « autoriser » depuis un masque joker au format v2.
    pub fn allow(mask: &str, target: RuleTarget) -> Self {
        Self::from_mask(mask, RuleAction::Allow, target)
    }

    fn from_mask(mask: &str, action: RuleAction, target: RuleTarget) -> Self {
        let (full_path, mask) = match mask.strip_prefix("%W%") {
            Some(rest) => (true, rest),
            None => (false, mask),
        };
        Self { pattern: Pattern::wildcard(mask), action, target, directions: Directions::ALL, full_path }
    }

    fn applies(&self, c: &Candidate<'_>) -> bool {
        let target_ok = match self.target {
            RuleTarget::Both => true,
            RuleTarget::Files => !c.is_dir,
            RuleTarget::Dirs => c.is_dir,
        };
        target_ok && self.directions.contains(c.kind)
    }
}

/// Élément à tester.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub name: &'a str,
    pub full_path: &'a str,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub kind: TransferKind,
}

/// Skiplist complète d'un site (ou globale).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipList {
    pub enabled: bool,
    pub rules: Vec<SkipRule>,
    /// Ne pas transférer les fichiers de 0 octet.
    pub skip_zero_byte_files: bool,
    /// Ne pas créer les dossiers qui resteraient vides.
    pub skip_empty_dirs: bool,
}

impl Default for SkipList {
    fn default() -> Self {
        Self { enabled: true, rules: Vec::new(), skip_zero_byte_files: false, skip_empty_dirs: false }
    }
}

impl SkipList {
    /// Raison de l'exclusion, ou `None` si l'élément doit être transféré.
    pub fn check(&self, c: &Candidate<'_>) -> Option<String> {
        if !self.enabled {
            return None;
        }
        for rule in self.rules.iter().filter(|r| r.applies(c)) {
            let subject = if rule.full_path { c.full_path } else { c.name };
            if rule.pattern.matches(subject) {
                return match rule.action {
                    RuleAction::Allow => None,
                    RuleAction::Skip => Some(format!("skiplist : {}", rule.pattern.text())),
                };
            }
        }
        if self.skip_zero_byte_files && !c.is_dir && c.size == Some(0) {
            return Some("fichier de 0 octet".into());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file<'a>(name: &'a str, path: &'a str, size: u64) -> Candidate<'a> {
        Candidate { name, full_path: path, is_dir: false, size: Some(size), kind: TransferKind::Download }
    }

    #[test]
    fn first_matching_rule_wins() {
        let list = SkipList {
            rules: vec![
                SkipRule::allow("important.nfo", RuleTarget::Files),
                SkipRule::skip("*.nfo", RuleTarget::Files),
                SkipRule::skip("sample", RuleTarget::Dirs),
            ],
            ..SkipList::default()
        };
        assert!(list.check(&file("important.nfo", "/r/important.nfo", 5)).is_none());
        assert!(list.check(&file("release.NFO", "/r/release.nfo", 5)).is_some());
        assert!(list.check(&file("movie.mkv", "/r/movie.mkv", 5)).is_none());
        let dir = Candidate { name: "Sample", full_path: "/r/Sample", is_dir: true, size: None, kind: TransferKind::Fxp };
        assert!(list.check(&dir).is_some());
        // La règle « Dirs » ne touche pas un fichier du même nom.
        assert!(list.check(&file("sample", "/r/sample", 1)).is_none());
    }

    #[test]
    fn full_path_directions_zero_bytes_and_disabled() {
        let mut rule = SkipRule::skip("%W%*/proof/*", RuleTarget::Files);
        rule.directions = Directions { upload: false, download: true, fxp: false };
        let mut list = SkipList { rules: vec![rule], skip_zero_byte_files: true, ..SkipList::default() };
        assert!(list.check(&file("a.jpg", "/rel/proof/a.jpg", 9)).is_some());
        let up = Candidate { kind: TransferKind::Upload, ..file("a.jpg", "/rel/proof/a.jpg", 9) };
        assert!(list.check(&up).is_none());
        assert!(list.check(&file("vide.txt", "/vide.txt", 0)).is_some());
        list.enabled = false;
        assert!(list.check(&file("vide.txt", "/vide.txt", 0)).is_none());
    }
}
