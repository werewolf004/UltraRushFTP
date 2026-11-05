//! Name masks: wildcard (`*`, `?`) or regular expression, case-insensitive.
//! Used by the skip list, the priority list and the proxy exceptions.

use std::fmt;

/// A compiled mask.
#[derive(Clone)]
pub struct Pattern {
    text: String,
    regex: Option<regex::Regex>,
}

impl Pattern {
    /// Wildcard mask: `*` = any sequence, `?` = one character.
    pub fn wildcard(text: impl Into<String>) -> Self {
        Self { text: text.into(), regex: None }
    }

    /// Regular expression (Perl syntax, case-insensitive).
    pub fn regex(text: impl Into<String>) -> Result<Self, regex::Error> {
        let text = text.into();
        let re = regex::RegexBuilder::new(&text).case_insensitive(true).build()?;
        Ok(Self { text, regex: Some(re) })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_regex(&self) -> bool {
        self.regex.is_some()
    }

    pub fn matches(&self, s: &str) -> bool {
        match &self.regex {
            Some(re) => re.is_match(s),
            None => wildcard_match(&self.text, s),
        }
    }
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text && self.is_regex() == other.is_regex()
    }
}

impl Eq for Pattern {}

impl fmt::Debug for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_regex() { write!(f, "Regex({:?})", self.text) } else { write!(f, "Wildcard({:?})", self.text) }
    }
}

/// Case-insensitive wildcard match: `*` (0..n characters) and `?` (1 character).
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard_match("220.67.*", "220.67.1.2"));
        assert!(!wildcard_match("220.67.*", "220.68.1.2"));
        assert!(wildcard_match("*.example.COM", "ftp.Example.com"));
        assert!(wildcard_match("ftp?.site", "ftp1.site"));
        assert!(!wildcard_match("ftp?.site", "ftp12.site"));
        assert!(wildcard_match("*", "anything") && wildcard_match("*", ""));
        assert!(wildcard_match("*.nfo", "release.NFO"));
    }

    #[test]
    fn regex_and_equality() {
        let r = Pattern::regex(r"^sample|\.(jpg|png)$").unwrap();
        assert!(r.matches("Sample.mkv") && r.matches("cover.JPG") && !r.matches("movie.mkv"));
        assert!(Pattern::regex("(").is_err());
        assert_eq!(Pattern::wildcard("*.nfo"), Pattern::wildcard("*.nfo"));
        assert_ne!(Pattern::wildcard("a"), Pattern::regex("a").unwrap());
    }
}
