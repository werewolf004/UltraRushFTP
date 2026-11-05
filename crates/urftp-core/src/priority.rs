//! Liste de priorités : un élément dont le nom correspond à un masque passe en tête de
//! file ; plus le masque est haut dans la liste, plus la priorité est forte.

use crate::pattern::Pattern;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PriorityList {
    pub patterns: Vec<Pattern>,
}

impl PriorityList {
    pub fn new(patterns: impl IntoIterator<Item = Pattern>) -> Self {
        Self { patterns: patterns.into_iter().collect() }
    }

    /// Rang du premier masque correspondant (0 = le plus prioritaire), `None` sinon.
    pub fn rank(&self, name: &str) -> Option<usize> {
        self.patterns.iter().position(|p| p.matches(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks() {
        let p = PriorityList::new([Pattern::wildcard("*.sfv"), Pattern::wildcard("*.nfo")]);
        assert_eq!(p.rank("release.SFV"), Some(0));
        assert_eq!(p.rank("release.nfo"), Some(1));
        assert_eq!(p.rank("release.r00"), None);
    }
}
