//! Update check: when the next check is due, and whether a published version is newer.
//! The request itself is in `urftp_proto::update`; nothing is ever downloaded or installed,
//! the user is only told and sent to the release page.

use serde::{Deserialize, Serialize};

/// GitHub repository of the releases.
pub const REPOSITORY: &str = "werewolf004/UltraRushFTP";

/// How often the releases are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateFrequency {
    Daily,
    #[default]
    Weekly,
    Monthly,
}

impl UpdateFrequency {
    pub const ALL: [UpdateFrequency; 3] = [UpdateFrequency::Daily, UpdateFrequency::Weekly, UpdateFrequency::Monthly];

    /// Interval between two checks, in seconds.
    pub fn seconds(self) -> u64 {
        const DAY: u64 = 24 * 3600;
        match self {
            UpdateFrequency::Daily => DAY,
            UpdateFrequency::Weekly => 7 * DAY,
            UpdateFrequency::Monthly => 30 * DAY,
        }
    }
}

/// A check is due: never checked, the interval passed, or the clock went back.
pub fn is_due(now: u64, last_check: u64, frequency: UpdateFrequency) -> bool {
    last_check == 0 || now < last_check || now - last_check >= frequency.seconds()
}

/// `v1.2.3` or `1.2.3` → (1, 2, 3); `None` for anything else, pre-releases (`-rc1`) included.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let t = text.trim();
    let t = t.strip_prefix('v').unwrap_or(t);
    let mut parts = t.split('.');
    let mut next = || parts.next().filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())).and_then(|p| p.parse().ok());
    let v = (next()?, next()?, next()?);
    parts.next().is_none().then_some(v)
}

/// `published` is a newer version than `current`.
pub fn is_newer(published: &str, current: &str) -> bool {
    matches!((parse_version(published), parse_version(current)), (Some(p), Some(c)) if p > c)
}

/// Tag of the release in the GitHub answer (`releases/latest`); `None` for a draft, a
/// pre-release or an answer that cannot be read.
pub fn latest_tag(json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
    }
    let r: Release = serde_json::from_str(json).ok()?;
    (!r.draft && !r.prerelease && parse_version(&r.tag_name).is_some()).then_some(r.tag_name)
}

/// Page of a release, built from its tag (never from a URL sent by the server).
pub fn release_page(tag: &str) -> Option<String> {
    parse_version(tag)?;
    Some(format!("https://github.com/{REPOSITORY}/releases/tag/{}", tag.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_compared_by_number() {
        assert_eq!(parse_version("v0.15.1"), Some((0, 15, 1)));
        assert_eq!(parse_version("1.2.10"), Some((1, 2, 10)));
        assert_eq!(parse_version("v1.2"), None);
        assert_eq!(parse_version("v1.2.3-rc1"), None);
        assert_eq!(parse_version("v1.2.3.4"), None);
        assert_eq!(parse_version("v1.+2.3"), None);
        assert!(is_newer("v0.15.10", "0.15.9"));
        assert!(is_newer("v1.0.0", "0.99.99"));
        assert!(!is_newer("v0.15.1", "0.15.1"));
        assert!(!is_newer("v0.14.9", "0.15.1"));
        assert!(!is_newer("garbage", "0.15.1"));
    }

    #[test]
    fn checks_follow_the_chosen_frequency() {
        let day = 24 * 3600;
        assert!(is_due(1_000_000, 0, UpdateFrequency::Monthly), "never checked");
        assert!(!is_due(1_000_000 + day - 1, 1_000_000, UpdateFrequency::Daily));
        assert!(is_due(1_000_000 + day, 1_000_000, UpdateFrequency::Daily));
        assert!(!is_due(1_000_000 + 6 * day, 1_000_000, UpdateFrequency::Weekly));
        assert!(is_due(1_000_000 + 7 * day, 1_000_000, UpdateFrequency::Weekly));
        assert!(!is_due(1_000_000 + 29 * day, 1_000_000, UpdateFrequency::Monthly));
        assert!(is_due(999_000, 1_000_000, UpdateFrequency::Monthly), "clock set back");
    }

    #[test]
    fn the_tag_is_read_from_the_github_answer() {
        assert_eq!(latest_tag(r#"{"tag_name":"v0.16.0","draft":false,"prerelease":false,"name":"x"}"#).as_deref(), Some("v0.16.0"));
        assert_eq!(latest_tag(r#"{"tag_name":"v0.16.0","prerelease":true}"#), None);
        assert_eq!(latest_tag(r#"{"tag_name":"nightly"}"#), None);
        assert_eq!(latest_tag("<html>"), None);
    }

    #[test]
    fn the_release_page_comes_from_a_valid_tag_only() {
        assert_eq!(release_page("v0.16.0").as_deref(), Some("https://github.com/werewolf004/UltraRushFTP/releases/tag/v0.16.0"));
        assert_eq!(release_page("v0.16.0/../../evil"), None);
        assert_eq!(release_page("javascript:alert(1)"), None);
    }
}
