//! Helpers of the file panels, independent of the interface and the network: quick
//! filter, selection by mask, renaming several files, folder comparison, space used,
//! directory tree export, CHMOD modes and dates typed by the user.

use std::cmp::Ordering;

use crate::NameCase;
use crate::itemopts::epoch_seconds;
use crate::pattern::wildcard_match;

// ====================================================================== quick filter

/// Quick filter of a panel (`Ctrl+F`, FTPRush "Fast search"): every word must appear in the
/// name, a word starting with `-` must not, `*` and `?` make a word a mask of the whole
/// name, and a word between double quotes is taken as it is (`"-b"`). Or a regular
/// expression. Case is ignored.
#[derive(Debug, Clone, Default)]
pub struct QuickFilter {
    words: Vec<String>,
    without: Vec<String>,
    regex: Option<regex::Regex>,
}

impl QuickFilter {
    pub fn parse(text: &str, regex: bool) -> Result<Self, regex::Error> {
        if regex {
            let text = text.trim();
            if text.is_empty() {
                return Ok(Self::default());
            }
            let re = regex::RegexBuilder::new(text).case_insensitive(true).build()?;
            return Ok(Self { regex: Some(re), ..Self::default() });
        }
        let mut f = Self::default();
        for w in text.split_whitespace() {
            let w = w.to_lowercase();
            if w.len() >= 2 && w.starts_with('"') && w.ends_with('"') {
                f.words.push(w[1..w.len() - 1].to_string());
            } else if let Some(rest) = w.strip_prefix('-').filter(|r| !r.is_empty()) {
                f.without.push(rest.to_string());
            } else {
                f.words.push(w);
            }
        }
        Ok(f)
    }

    /// Nothing to filter: every name passes.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.without.is_empty() && self.regex.is_none()
    }

    pub fn matches(&self, name: &str) -> bool {
        if let Some(re) = &self.regex {
            return re.is_match(name);
        }
        let lower = name.to_lowercase();
        self.words.iter().all(|w| term_matches(w, &lower)) && !self.without.iter().any(|w| term_matches(w, &lower))
    }
}

fn term_matches(term: &str, lower_name: &str) -> bool {
    if term.contains(['*', '?']) { wildcard_match(term, lower_name) } else { lower_name.contains(term) }
}

/// The name matches one of the wildcard masks separated by `;` (`*.rar;*.r??`); an empty
/// list matches nothing.
pub fn mask_matches(masks: &str, name: &str) -> bool {
    masks.split(';').map(str::trim).filter(|m| !m.is_empty()).any(|m| wildcard_match(m, name))
}

// ====================================================================== rename several

/// Rule of "Rename multiple files". The model builds each new name from `%f` (whole
/// name), `%n` (name without extension), `%e` (extension, without the dot) and `%c` (a
/// counter); then the search text is replaced (a regular expression when `regex`, where
/// `$1` or `${1}`... are the groups), then the case is changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameRule {
    pub template: String,
    pub find: String,
    pub replace: String,
    pub regex: bool,
    pub case: NameCase,
    pub counter_start: u64,
    pub counter_step: u64,
    /// Minimum number of digits of the counter (zeros in front).
    pub counter_digits: usize,
}

impl Default for RenameRule {
    fn default() -> Self {
        Self {
            template: "%f".into(),
            find: String::new(),
            replace: String::new(),
            regex: false,
            case: NameCase::Keep,
            counter_start: 1,
            counter_step: 1,
            counter_digits: 2,
        }
    }
}

/// Why a set of new names cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenameError {
    #[error("invalid regular expression: {0}")]
    Regex(String),
    #[error("empty name for {0}")]
    Empty(String),
    #[error("forbidden character in {0}")]
    Forbidden(String),
    #[error("{0} would be given twice")]
    Twice(String),
}

/// Characters a name may not contain (Windows and FTP paths).
const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

impl RenameRule {
    /// New names of `items` (name, folder?), in the same order. Folders have no extension.
    pub fn apply(&self, items: &[(String, bool)]) -> Result<Vec<String>, RenameError> {
        let re = if self.regex && !self.find.is_empty() {
            Some(regex::RegexBuilder::new(&self.find).case_insensitive(true).build().map_err(|e| RenameError::Regex(e.to_string()))?)
        } else {
            None
        };
        let mut out = Vec::with_capacity(items.len());
        let mut counter = self.counter_start;
        for (name, is_dir) in items {
            let (stem, ext) = if *is_dir { (name.as_str(), "") } else { split_extension(name) };
            let count = format!("{counter:0width$}", width = self.counter_digits);
            counter = counter.saturating_add(self.counter_step);
            let template = if self.template.trim().is_empty() { "%f" } else { self.template.as_str() };
            let built = expand(template, name, stem, ext, &count);
            let replaced = match &re {
                Some(re) => re.replace_all(&built, self.replace.as_str()).into_owned(),
                None if !self.find.is_empty() => replace_ignore_case(&built, &self.find, &self.replace),
                None => built,
            };
            out.push(self.case.apply(replaced.trim()));
        }
        check_names(items, &out)?;
        Ok(out)
    }
}

/// `name.ext` → (`name`, `ext`); no extension for `name`, `.hidden` or `name.`.
pub fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i + 1..]),
        _ => (name, ""),
    }
}

fn expand(template: &str, full: &str, stem: &str, ext: &str, count: &str) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek().map(char::to_ascii_lowercase) {
            Some('f') => out.push_str(full),
            Some('n') => out.push_str(stem),
            Some('e') => out.push_str(ext),
            Some('c') => out.push_str(count),
            Some('%') => out.push('%'),
            _ => {
                out.push('%');
                continue;
            }
        }
        chars.next();
    }
    // `%n.%e` of a file without extension: no trailing dot.
    if ext.is_empty() && out.ends_with('.') && !template.ends_with("..") { out.trim_end_matches('.').to_string() } else { out }
}

fn replace_ignore_case(text: &str, find: &str, with: &str) -> String {
    let lower = text.to_lowercase();
    let needle = find.to_lowercase();
    // Lower-casing may change byte lengths: fall back to an exact search then.
    if lower.len() != text.len() || needle.len() != find.len() {
        return text.replace(find, with);
    }
    let mut out = String::new();
    let mut last = 0;
    for (i, _) in lower.match_indices(&needle) {
        out.push_str(&text[last..i]);
        out.push_str(with);
        last = i + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

/// The new names are valid and distinct (case ignored), and none takes the name of an
/// item of the folder that is not renamed.
fn check_names(items: &[(String, bool)], new: &[String]) -> Result<(), RenameError> {
    let mut seen = std::collections::HashSet::new();
    for (i, n) in new.iter().enumerate() {
        let old = &items[i].0;
        if n.is_empty() {
            return Err(RenameError::Empty(old.clone()));
        }
        if n.contains(FORBIDDEN) || n == "." || n == ".." {
            return Err(RenameError::Forbidden(n.clone()));
        }
        if !seen.insert(n.to_lowercase()) {
            return Err(RenameError::Twice(n.clone()));
        }
    }
    Ok(())
}

// ====================================================================== comparison

/// What makes two items the same in "Compare folder contents".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompareBy {
    #[default]
    Name,
    /// Name and size (folders: name only).
    NameSize,
}

/// An item of a panel to compare: name, folder?, size.
pub type CompareItem<'a> = (&'a str, bool, u64);

/// Marks the items of each side that have the same item on the other side. `partial`:
/// the extensions of the files are ignored. Case is ignored.
pub fn compare_folders(left: &[CompareItem<'_>], right: &[CompareItem<'_>], by: CompareBy, partial: bool) -> (Vec<bool>, Vec<bool>) {
    let key = |(name, is_dir, _): &CompareItem<'_>| {
        let n = if partial && !*is_dir { split_extension(name).0 } else { name };
        (n.to_lowercase(), *is_dir)
    };
    let same = |a: &CompareItem<'_>, b: &CompareItem<'_>| key(a) == key(b) && (by == CompareBy::Name || a.1 || a.2 == b.2);
    let l = left.iter().map(|a| right.iter().any(|b| same(a, b))).collect();
    let r = right.iter().map(|b| left.iter().any(|a| same(a, b))).collect();
    (l, r)
}

// ====================================================================== folder walks

/// An item found by a recursive walk: path relative to the root (`a/b/c.txt`), level
/// (1 = directly in the root), folder?, size and date (seconds since 1970).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    pub path: String,
    pub depth: usize,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
}

impl WalkEntry {
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// Every item under the local folder `root`, level by level, like the remote walk;
/// `depth` 0 = every level. A sub-folder that cannot be read is reported to `failed`.
pub fn walk_local(
    root: &std::path::Path,
    depth: usize,
    stop: &std::sync::atomic::AtomicBool,
    mut found: impl FnMut(WalkEntry),
    mut failed: impl FnMut(String, std::io::Error),
) -> std::io::Result<()> {
    let mut todo = std::collections::VecDeque::from([(String::new(), 0usize)]);
    while let Some((rel, level)) = todo.pop_front() {
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let dir = if rel.is_empty() { root.to_path_buf() } else { root.join(&rel) };
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if level > 0 => {
                failed(dir.display().to_string(), e);
                continue;
            }
            Err(e) => return Err(e),
        };
        for e in rd.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            let modified = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_secs()).ok());
            found(WalkEntry {
                path: child.clone(),
                depth: level + 1,
                is_dir: meta.is_dir(),
                size: if meta.is_dir() { 0 } else { meta.len() },
                modified,
            });
            if meta.is_dir() && (depth == 0 || level + 1 < depth) {
                todo.push_back((child, level + 1));
            }
        }
    }
    Ok(())
}

/// Totals of a folder ("Calculate space used").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpaceRow {
    /// Path relative to the root (empty: the root itself).
    pub path: String,
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
}

/// Totals of the whole walk, and with `depth` > 0 the totals of each folder of that level
/// (its whole contents).
pub fn space_used(entries: &[WalkEntry], depth: usize) -> (SpaceRow, Vec<SpaceRow>) {
    let mut total = SpaceRow::default();
    for e in entries {
        add(&mut total, e);
    }
    let mut rows: Vec<SpaceRow> = Vec::new();
    if depth > 0 {
        let mut at: std::collections::BTreeMap<String, SpaceRow> = entries
            .iter()
            .filter(|e| e.is_dir && e.depth == depth)
            .map(|e| (e.path.clone(), SpaceRow { path: e.path.clone(), ..SpaceRow::default() }))
            .collect();
        for e in entries.iter().filter(|e| e.depth > depth) {
            let top: String = e.path.split('/').take(depth).collect::<Vec<_>>().join("/");
            if let Some(row) = at.get_mut(&top) {
                add(row, e);
            }
        }
        rows = at.into_values().collect();
    }
    (total, rows)
}

fn add(row: &mut SpaceRow, e: &WalkEntry) {
    if e.is_dir {
        row.folders += 1;
    } else {
        row.files += 1;
        row.bytes += e.size;
    }
}

/// Options of "Export directory tree list".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeOptions {
    /// Text repeated once per level (two spaces by default).
    pub indent: String,
    pub sizes: bool,
    pub folders_only: bool,
}

impl Default for TreeOptions {
    fn default() -> Self {
        Self { indent: "  ".into(), sizes: false, folders_only: false }
    }
}

/// The tree as text: the root, then each item under its folder, sorted by name (folders
/// first). Folder sizes are the total of their files.
pub fn tree_text(root: &str, entries: &[WalkEntry], o: &TreeOptions) -> String {
    let mut sorted: Vec<&WalkEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| tree_order(a, b));
    let folder_size = |dir: &str| -> u64 {
        let prefix = format!("{dir}/");
        entries.iter().filter(|e| !e.is_dir && e.path.starts_with(&prefix)).map(|e| e.size).sum()
    };
    let mut out = String::from(root);
    if o.sizes {
        let all: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
        out.push_str(&format!(" ({})", format_size(all)));
    }
    out.push('\n');
    for e in sorted {
        if o.folders_only && !e.is_dir {
            continue;
        }
        out.push_str(&o.indent.repeat(e.depth));
        out.push_str(e.name());
        if e.is_dir {
            out.push('/');
        }
        if o.sizes {
            let size = if e.is_dir { folder_size(&e.path) } else { e.size };
            out.push_str(&format!(" ({})", format_size(size)));
        }
        out.push('\n');
    }
    out
}

/// Tree order: by path component, folders before files at each level, case ignored.
fn tree_order(a: &WalkEntry, b: &WalkEntry) -> Ordering {
    let ca: Vec<&str> = a.path.split('/').collect();
    let cb: Vec<&str> = b.path.split('/').collect();
    for i in 0..ca.len().min(cb.len()) {
        if ca[i] == cb[i] {
            continue;
        }
        // At this level, an item that goes on deeper is a folder.
        let a_dir = i + 1 < ca.len() || a.is_dir;
        let b_dir = i + 1 < cb.len() || b.is_dir;
        return b_dir.cmp(&a_dir).then_with(|| ca[i].to_lowercase().cmp(&cb[i].to_lowercase())).then_with(|| ca[i].cmp(cb[i]));
    }
    ca.len().cmp(&cb.len())
}

/// Size for people: `512 B`, `1.5 KiB`, `2.0 MiB`...
pub fn format_size(n: u64) -> String {
    const UNITS: [&str; 6] = ["KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64 / 1024.0;
    let mut unit = 0;
    while v >= 1024.0 && unit + 1 < UNITS.len() {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS[unit])
}

// ====================================================================== CHMOD and dates

/// Octal mode typed by the user (`755`, `0644`, `4755`).
pub fn parse_mode(text: &str) -> Option<u32> {
    let t = text.trim();
    if t.is_empty() || t.len() > 4 || !t.chars().all(|c| ('0'..='7').contains(&c)) {
        return None;
    }
    u32::from_str_radix(t, 8).ok()
}

/// Mode of a Unix listing (`drwxr-xr-x`, `-rwsr-x--T`...): the last nine letters.
pub fn mode_of_permissions(perms: &str) -> Option<u32> {
    let c: Vec<char> = perms.chars().collect();
    if c.len() < 9 {
        return None;
    }
    let p = &c[c.len() - 9..];
    let mut mode = 0u32;
    for (i, ch) in p.iter().enumerate() {
        let bit = 1 << (8 - i);
        let special = match (i, ch) {
            (2, 's') | (5, 's') | (8, 't') => {
                mode |= bit;
                true
            }
            (2, 'S') | (5, 'S') | (8, 'T') => true,
            _ => false,
        };
        if special {
            mode |= match i {
                2 => 0o4000,
                5 => 0o2000,
                _ => 0o1000,
            };
        } else if *ch != '-' {
            mode |= bit;
        }
    }
    Some(mode)
}

/// `755` or `4755`.
pub fn mode_text(mode: u32) -> String {
    if mode > 0o777 { format!("{mode:04o}") } else { format!("{mode:03o}") }
}

/// `YYYY-MM-DD`, `YYYY-MM-DD HH:MM` or `YYYY-MM-DD HH:MM:SS` (UTC), seconds since 1970.
pub fn parse_datetime(text: &str) -> Option<i64> {
    let t = text.trim();
    let (date, time) = t.split_once([' ', 'T']).map_or((t, ""), |(d, h)| (d, h.trim()));
    let mut d = date.splitn(3, '-');
    let y: i64 = d.next()?.parse().ok()?;
    let m: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let mut h = time.split(':').filter(|s| !s.is_empty());
    let hour: u32 = h.next().map_or(Some(0), |v| v.parse().ok())?;
    let minute: u32 = h.next().map_or(Some(0), |v| v.parse().ok())?;
    let second: u32 = h.next().map_or(Some(0), |v| v.parse().ok())?;
    let valid = (1..=12).contains(&m) && (1..=31).contains(&day) && hour < 24 && minute < 60 && second < 60;
    valid.then(|| epoch_seconds(y, m, day, hour, minute, second))
}

/// Civil date and time (UTC) of seconds since 1970: (year, month, day, hour, minute, second).
pub fn civil(epoch: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let n = |v: i64| u32::try_from(v).unwrap_or(0);
    (year, n(month), n(day), n(secs / 3600), n(secs % 3600 / 60), n(secs % 60))
}

/// `YYYY-MM-DD HH:MM:SS` (UTC).
pub fn format_datetime(epoch: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(epoch);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// `YYYYMMDDHHMMSS` (UTC), the form of `MFMT` and `SITE UTIME`.
pub fn ftp_timestamp(epoch: i64) -> String {
    let (y, mo, d, h, mi, s) = civil(epoch);
    format!("{y:04}{mo:02}{d:02}{h:02}{mi:02}{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(text: &str) -> QuickFilter {
        QuickFilter::parse(text, false).unwrap_or_default()
    }

    #[test]
    fn quick_filter_words() {
        assert!(f("").is_empty());
        assert!(f("rel").matches("Some.RELEASE-grp"));
        assert!(f("a b").matches("xaxbx") && !f("a b").matches("xax"));
        assert!(f("a -b").matches("xax") && !f("a -b").matches("xab"));
        assert!(f("\"-b\"").matches("a-b") && !f("\"-b\"").matches("ab"));
        assert!(f("*.nfo").matches("x.NFO") && !f("*.nfo").matches("x.nfo.txt"));
        assert!(f("-").matches("a-b"), "a lone dash is a word");
        let re = QuickFilter::parse("^a.*z$", true).unwrap_or_default();
        assert!(re.matches("ABCZ") && !re.matches("abc"));
        assert!(QuickFilter::parse("(", true).is_err());
    }

    #[test]
    fn masks() {
        assert!(mask_matches("*.rar; *.r??", "a.r01"));
        assert!(!mask_matches(" ; ", "a"));
    }

    fn names(v: &[(&str, bool)]) -> Vec<(String, bool)> {
        v.iter().map(|(n, d)| ((*n).to_string(), *d)).collect()
    }

    #[test]
    fn rename_rules() {
        let items = names(&[("a.TXT", false), ("b.txt", false), ("dir.x", true)]);
        let r = RenameRule { template: "%c - %n.%e".into(), ..RenameRule::default() };
        assert_eq!(r.apply(&items), Ok(vec!["01 - a.TXT".into(), "02 - b.txt".into(), "03 - dir.x".into()]));
        let r = RenameRule { find: ".txt".into(), replace: ".nfo".into(), ..RenameRule::default() };
        assert_eq!(r.apply(&items[..2]), Ok(vec!["a.nfo".into(), "b.nfo".into()]));
        let r = RenameRule {
            find: r"^(\w)\.(\w+)$".into(),
            replace: "${2}_$1".into(),
            regex: true,
            case: NameCase::Lower,
            ..RenameRule::default()
        };
        assert_eq!(r.apply(&items[..1]), Ok(vec!["txt_a".into()]));
        let same = RenameRule { template: "x".into(), ..RenameRule::default() };
        assert_eq!(same.apply(&items[..2]), Err(RenameError::Twice("x".into())));
        let bad = RenameRule { template: "a/b".into(), ..RenameRule::default() };
        assert!(matches!(bad.apply(&items[..1]), Err(RenameError::Forbidden(_))));
        let no_ext = RenameRule { template: "%n.%e".into(), ..RenameRule::default() };
        assert_eq!(no_ext.apply(&names(&[("README", false)])), Ok(vec!["README".into()]));
        assert_eq!(split_extension(".hidden"), (".hidden", ""));
    }

    #[test]
    fn compare() {
        let left = [("a.rar", false, 10), ("b.rar", false, 5), ("Dir", true, 0)];
        let right = [("A.rar", false, 10), ("b.rar", false, 6), ("dir", true, 4), ("c.zip", false, 1)];
        assert_eq!(compare_folders(&left, &right, CompareBy::Name, false), (vec![true, true, true], vec![true, true, true, false]));
        assert_eq!(compare_folders(&left, &right, CompareBy::NameSize, false), (vec![true, false, true], vec![true, false, true, false]));
        let l = [("x.r00", false, 1)];
        let r = [("x.r01", false, 1)];
        assert_eq!(compare_folders(&l, &r, CompareBy::Name, true), (vec![true], vec![true]));
    }

    fn walk() -> Vec<WalkEntry> {
        let e = |path: &str, is_dir, size| WalkEntry { path: path.into(), depth: path.split('/').count(), is_dir, size, modified: None };
        vec![e("b.txt", false, 5), e("a", true, 0), e("a/x", true, 0), e("a/x/1.bin", false, 100), e("a/2.bin", false, 50), e("c", true, 0)]
    }

    #[test]
    fn space() {
        let (total, rows) = space_used(&walk(), 0);
        assert_eq!((total.files, total.folders, total.bytes), (3, 3, 155));
        assert!(rows.is_empty());
        let (_, rows) = space_used(&walk(), 1);
        let a = rows.iter().find(|r| r.path == "a").map(|r| (r.files, r.folders, r.bytes));
        assert_eq!(a, Some((2, 1, 150)));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn tree() {
        let text = tree_text("/root", &walk(), &TreeOptions::default());
        assert_eq!(text, "/root\n  a/\n    x/\n      1.bin\n    2.bin\n  c/\n  b.txt\n");
        let o = TreeOptions { indent: "-".into(), sizes: true, folders_only: true };
        assert_eq!(tree_text("/", &walk(), &o), "/ (155 B)\n-a/ (150 B)\n--x/ (100 B)\n-c/ (0 B)\n");
        assert_eq!(format_size(1536), "1.5 KiB");
    }

    #[test]
    fn local_walk() {
        let root = std::env::temp_dir().join(format!("urftp-walk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/x")).unwrap();
        std::fs::write(root.join("a/x/1.bin"), [0u8; 100]).unwrap();
        std::fs::write(root.join("b.txt"), b"hello").unwrap();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut all = Vec::new();
        walk_local(&root, 0, &stop, |e| all.push(e), |_, _| {}).unwrap();
        let mut paths: Vec<_> = all.iter().map(|e| (e.path.as_str(), e.depth, e.size)).collect();
        paths.sort();
        assert_eq!(paths, vec![("a", 1, 0), ("a/x", 2, 0), ("a/x/1.bin", 3, 100), ("b.txt", 1, 5)]);
        let mut top = Vec::new();
        walk_local(&root, 1, &stop, |e| top.push(e.path), |_, _| {}).unwrap();
        top.sort();
        assert_eq!(top, vec!["a".to_string(), "b.txt".to_string()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn modes() {
        assert_eq!(parse_mode("755"), Some(0o755));
        assert_eq!(parse_mode("0644"), Some(0o644));
        assert_eq!(parse_mode("8"), None);
        assert_eq!(parse_mode("77777"), None);
        assert_eq!(mode_of_permissions("drwxr-xr-x"), Some(0o755));
        assert_eq!(mode_of_permissions("-rwsr-x--T"), Some(0o5750));
        assert_eq!(mode_of_permissions("rw"), None);
        assert_eq!(mode_text(0o644), "644");
        assert_eq!(mode_text(0o4755), "4755");
    }

    #[test]
    fn dates() {
        let t = parse_datetime("2024-02-29 13:45:07");
        assert_eq!(t, Some(epoch_seconds(2024, 2, 29, 13, 45, 7)));
        assert_eq!(t.map(format_datetime).as_deref(), Some("2024-02-29 13:45:07"));
        assert_eq!(t.map(ftp_timestamp).as_deref(), Some("20240229134507"));
        assert_eq!(parse_datetime("2024-01-02"), Some(epoch_seconds(2024, 1, 2, 0, 0, 0)));
        assert_eq!(parse_datetime("2024-01-02 25:00"), None);
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(format_datetime(-1), "1969-12-31 23:59:59");
    }
}
