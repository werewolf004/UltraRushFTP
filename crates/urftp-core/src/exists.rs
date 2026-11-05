//! "Existing file" rules: what to do when the target already exists, depending on whether
//! it is smaller than, the same size as, or larger than the source.

/// Action configured for one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExistsAction {
    Resume,
    Skip,
    Rename,
    Overwrite,
    /// Ask the user (the item waits for a decision).
    Ask,
}

/// Decision for a given transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExistsDecision {
    /// Transfer from this byte (0 = from the start, overwriting).
    Transfer {
        offset: u64,
    },
    Skip,
    Rename,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ExistsRules {
    pub smaller: ExistsAction,
    pub same: ExistsAction,
    pub larger: ExistsAction,
    /// When resuming fails (server without `REST`...), start over from the beginning.
    pub overwrite_if_resume_fails: bool,
}

impl Default for ExistsRules {
    fn default() -> Self {
        Self { smaller: ExistsAction::Resume, same: ExistsAction::Skip, larger: ExistsAction::Rename, overwrite_if_resume_fails: true }
    }
}

impl ExistsRules {
    /// `source` / `target`: known sizes; `target = None` when the target does not exist.
    pub fn decide(&self, source: Option<u64>, target: Option<u64>) -> ExistsDecision {
        let Some(t) = target else {
            return ExistsDecision::Transfer { offset: 0 };
        };
        let Some(s) = source else {
            // Unknown source size: nothing to compare, replace.
            return ExistsDecision::Transfer { offset: 0 };
        };
        let action = match t.cmp(&s) {
            std::cmp::Ordering::Less => self.smaller,
            std::cmp::Ordering::Equal => self.same,
            std::cmp::Ordering::Greater => self.larger,
        };
        match action {
            ExistsAction::Resume if t < s => ExistsDecision::Transfer { offset: t },
            // Resuming a complete or larger file makes no sense.
            ExistsAction::Resume if t == s => ExistsDecision::Skip,
            ExistsAction::Resume | ExistsAction::Overwrite => ExistsDecision::Transfer { offset: 0 },
            ExistsAction::Skip => ExistsDecision::Skip,
            ExistsAction::Rename => ExistsDecision::Rename,
            ExistsAction::Ask => ExistsDecision::Ask,
        }
    }
}

/// Renamed name: `film.mkv` → `film (1).mkv`; `README` → `README (1)`.
pub fn renamed(name: &str, n: u32) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{} ({n}){}", &name[..i], &name[i..]),
        _ => format!("{name} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions() {
        let r = ExistsRules::default();
        assert_eq!(r.decide(Some(100), None), ExistsDecision::Transfer { offset: 0 });
        assert_eq!(r.decide(Some(100), Some(40)), ExistsDecision::Transfer { offset: 40 });
        assert_eq!(r.decide(Some(100), Some(100)), ExistsDecision::Skip);
        assert_eq!(r.decide(Some(100), Some(150)), ExistsDecision::Rename);
        assert_eq!(r.decide(None, Some(150)), ExistsDecision::Transfer { offset: 0 });
        let all_resume = ExistsRules { same: ExistsAction::Resume, larger: ExistsAction::Resume, ..r };
        assert_eq!(all_resume.decide(Some(10), Some(10)), ExistsDecision::Skip);
        assert_eq!(all_resume.decide(Some(10), Some(20)), ExistsDecision::Transfer { offset: 0 });
        let ask = ExistsRules { same: ExistsAction::Ask, ..r };
        assert_eq!(ask.decide(Some(1), Some(1)), ExistsDecision::Ask);
    }

    #[test]
    fn rename_names() {
        assert_eq!(renamed("film.mkv", 1), "film (1).mkv");
        assert_eq!(renamed("archive.tar.gz", 2), "archive.tar (2).gz");
        assert_eq!(renamed("README", 1), "README (1)");
        assert_eq!(renamed(".htaccess", 1), ".htaccess (1)");
    }
}
