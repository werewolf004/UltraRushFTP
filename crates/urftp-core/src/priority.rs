//! Priority list: an item whose name matches a mask moves to the top of the queue; the
//! higher the mask in the list, the higher the priority.

use crate::pattern::Pattern;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PriorityList {
    pub patterns: Vec<Pattern>,
}

impl PriorityList {
    pub fn new(patterns: impl IntoIterator<Item = Pattern>) -> Self {
        Self { patterns: patterns.into_iter().collect() }
    }

    /// Rank of the first matching mask (0 = highest priority), `None` otherwise.
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
