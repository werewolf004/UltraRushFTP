//! User-interface translations of UltraRushFTP.
//!
//! Catalogs are flat JSON objects embedded at compile time (`locales/en.json`,
//! `locales/fr.json`): `"key": "text with {placeholders}"`. English is the reference
//! catalog; a key missing from another language falls back to English, then to the key
//! itself (tests make sure this never happens in a release).
//!
//! ```
//! use urftp_i18n::{Lang, set_language, t};
//! set_language(Lang::En);
//! assert_eq!(t!("common.cancel"), "Cancel");
//! assert_eq!(t!("status.tabs", n = 3), "Tabs: 3");
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// Interface language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    /// English (reference catalog).
    En,
    /// French.
    Fr,
}

impl Lang {
    /// Every supported language, in menu order.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Fr];

    /// Name of the language in that language (for the language menu).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
        }
    }

    /// BCP 47 code.
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Fr => "fr",
        }
    }

    /// Language matching a BCP 47 tag such as `fr-FR`; `None` when unsupported.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let primary = tag.split(['-', '_']).next().unwrap_or_default().to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }

    /// Language of the Windows session, English when unsupported.
    pub fn system() -> Lang {
        sys_locale::get_locales().find_map(|t| Lang::from_tag(&t)).unwrap_or(Lang::En)
    }

    fn index(self) -> u8 {
        match self {
            Lang::En => 0,
            Lang::Fr => 1,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Sets the interface language for the whole process.
pub fn set_language(lang: Lang) {
    CURRENT.store(lang.index(), Ordering::Relaxed);
}

/// Current interface language.
pub fn language() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Fr,
        _ => Lang::En,
    }
}

type Catalog = HashMap<String, String>;

const SOURCES: [(Lang, &str); 2] = [(Lang::En, include_str!("../locales/en.json")), (Lang::Fr, include_str!("../locales/fr.json"))];

fn parse(source: &str) -> Catalog {
    // A malformed catalog yields an empty one: the UI then shows keys, and the
    // `catalogs_are_valid` test fails the build long before a release.
    serde_json::from_str(source).unwrap_or_default()
}

fn catalog(lang: Lang) -> &'static Catalog {
    static CATALOGS: OnceLock<[Catalog; 2]> = OnceLock::new();
    let all = CATALOGS.get_or_init(|| SOURCES.map(|(_, src)| parse(src)));
    &all[usize::from(lang.index())]
}

/// Raw text of `key` in the current language (English fallback, then the key itself).
pub fn text(key: &str) -> &str {
    catalog(language()).get(key).or_else(|| catalog(Lang::En).get(key)).map_or(key, String::as_str)
}

/// `key` with its `{name}` placeholders replaced by `args`.
pub fn format(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = text(key).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// Translated text: `t!("key")` or `t!("key", name = value, ...)`.
#[macro_export]
macro_rules! t {
    ($key:literal) => {
        $crate::text($key).to_string()
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::format($key, &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),+])
    };
}

/// Keys of the English catalog (tests of the crates using it).
pub fn keys() -> impl Iterator<Item = &'static str> {
    catalog(Lang::En).keys().map(String::as_str)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn placeholders(s: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = s;
        while let Some(i) = rest.find('{') {
            let Some(j) = rest[i..].find('}') else { break };
            out.insert(rest[i + 1..i + j].to_string());
            rest = &rest[i + j + 1..];
        }
        out
    }

    #[test]
    fn catalogs_are_valid() {
        for (lang, src) in SOURCES {
            let parsed: Result<Catalog, _> = serde_json::from_str(src);
            assert!(parsed.is_ok(), "{lang:?}: invalid JSON: {parsed:?}");
        }
        let en = catalog(Lang::En);
        assert!(!en.is_empty());
        for lang in Lang::ALL {
            let c = catalog(lang);
            let missing: Vec<_> = en.keys().filter(|k| !c.contains_key(*k)).collect();
            let extra: Vec<_> = c.keys().filter(|k| !en.contains_key(*k)).collect();
            assert!(missing.is_empty() && extra.is_empty(), "{lang:?}: missing {missing:?}, extra {extra:?}");
            for (k, v) in c {
                assert!(!v.trim().is_empty(), "{lang:?}: empty text for {k}");
                assert_eq!(placeholders(v), placeholders(&en[k]), "{lang:?}: placeholders of {k}");
            }
        }
    }

    #[test]
    fn lookup_format_and_fallback() {
        set_language(Lang::Fr);
        assert_eq!(t!("common.cancel"), "Annuler");
        assert_eq!(t!("status.tabs", n = 2), "Onglets : 2");
        assert_eq!(text("no.such.key"), "no.such.key");
        set_language(Lang::En);
        assert_eq!(t!("common.cancel"), "Cancel");
        assert_eq!(language(), Lang::En);
    }

    /// Every `t!("key"…)` / `text("key")` written in the workspace sources must exist.
    #[test]
    fn keys_used_in_sources_exist() {
        fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        walk(&crates, &mut files);
        let en = catalog(Lang::En);
        let mut used = 0;
        let mut unknown = Vec::new();
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap_or_default();
            for marker in ["t!(\"", "text(\""] {
                for (i, _) in src.match_indices(marker) {
                    // `foo_text("…")` is another function: the marker must start a word.
                    if src[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let rest = &src[i + marker.len()..];
                    let Some(end) = rest.find('"') else { continue };
                    let key = &rest[..end];
                    let well_formed = key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
                    if key.contains('.') && well_formed {
                        used += 1;
                        if !en.contains_key(key) && key != "no.such.key" {
                            unknown.push(format!("{}: {key}", f.display()));
                        }
                    }
                }
            }
        }
        assert!(used > 100, "only {used} keys found: wrong source folder?");
        assert!(unknown.is_empty(), "unknown keys: {unknown:#?}");
    }

    #[test]
    fn tags() {
        assert_eq!(Lang::from_tag("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_tag("FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_tag("en_US"), Some(Lang::En));
        assert_eq!(Lang::from_tag("de-DE"), None);
    }
}
