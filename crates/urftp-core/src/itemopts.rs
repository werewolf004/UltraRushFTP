//! Options of a queue item that its folder passes down to its contents: the parameters of
//! `RushApp.FTP.Transfer` (filters, depth, sub count, sorting, skip list off, retries,
//! refreshes with a "complete" flag). Items queued from the interface have none.

use crate::pattern::Pattern;

/// What a filter mask applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTarget {
    Files,
    Folders,
}

/// Allow and skip masks of a transfer (`FolderAllow`, `FolderSkip`, `FileAllow`,
/// `FileSkip`). Masks are separated by `;`; `%F%` before a mask makes it apply to files only,
/// `%D%` to folders only, whatever the list it is in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferFilter {
    allow: Vec<(FilterTarget, Pattern)>,
    skip: Vec<(FilterTarget, Pattern)>,
}

impl TransferFilter {
    /// `regex`: the masks are regular expressions (`RS_EXPR`), else wildcards.
    pub fn parse(folder_allow: &str, folder_skip: &str, file_allow: &str, file_skip: &str, regex: bool) -> Result<Self, regex::Error> {
        let mut f = Self::default();
        for (list, target, allow) in [
            (folder_allow, FilterTarget::Folders, true),
            (folder_skip, FilterTarget::Folders, false),
            (file_allow, FilterTarget::Files, true),
            (file_skip, FilterTarget::Files, false),
        ] {
            for mask in list.split(';').map(str::trim).filter(|m| !m.is_empty()) {
                let (target, mask) = if let Some(m) = strip_prefix_ci(mask, "%F%") {
                    (FilterTarget::Files, m)
                } else if let Some(m) = strip_prefix_ci(mask, "%D%") {
                    (FilterTarget::Folders, m)
                } else {
                    (target, mask)
                };
                let pattern = if regex { Pattern::regex(mask)? } else { Pattern::wildcard(mask) };
                if allow { f.allow.push((target, pattern)) } else { f.skip.push((target, pattern)) }
            }
        }
        Ok(f)
    }

    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.skip.is_empty()
    }

    /// Is this entry kept? A skip mask removes it; when allow masks exist for its kind, one
    /// of them must match.
    pub fn allows(&self, name: &str, is_dir: bool) -> bool {
        let kind = if is_dir { FilterTarget::Folders } else { FilterTarget::Files };
        if self.skip.iter().any(|(t, p)| *t == kind && p.matches(name)) {
            return false;
        }
        let mut allow = self.allow.iter().filter(|(t, _)| *t == kind).peekable();
        allow.peek().is_none() || allow.any(|(_, p)| p.matches(name))
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len()).filter(|p| p.eq_ignore_ascii_case(prefix)).map(|_| &s[prefix.len()..])
}

/// Sort key of the contents of a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Date,
    Size,
}

/// `FolderSort` / `FileSort`: name ascending by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SortOrder {
    pub key: SortKey,
    pub descending: bool,
}

/// An entry of a folder listing, local or remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    /// Modification time as a sortable number (seconds), when known.
    pub modified: Option<i64>,
}

/// Conditions of a conditional transfer: dates (seconds since 1970) and sizes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Conditions {
    /// Not older than this date.
    pub newer_than: Option<i64>,
    /// Not newer than this date.
    pub older_than: Option<i64>,
    /// The dates apply to folders too (files only otherwise).
    pub dates_on_folders: bool,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
}

impl Conditions {
    /// Is this entry kept? An unknown date or size does not exclude it.
    pub fn allows(&self, e: &FolderEntry) -> bool {
        let dated = !e.is_dir || self.dates_on_folders;
        let date_ok =
            !dated || e.modified.is_none_or(|t| !self.newer_than.is_some_and(|min| t < min) && !self.older_than.is_some_and(|max| t > max));
        let size_ok =
            e.is_dir || e.size.is_none_or(|n| !self.min_size.is_some_and(|min| n < min) && !self.max_size.is_some_and(|max| n > max));
        date_ok && size_ok
    }
}

/// Synchronisation of a folder with its target.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncOptions {
    /// Removes from the target what the source does not have.
    pub delete_missing: bool,
    /// A file is different when the source is newer (by more than a minute).
    pub compare_date: bool,
    /// A file is different when the sizes differ.
    pub compare_size: bool,
    /// Only the files the target already has are transferred.
    pub existing_only: bool,
    /// What only the target has is copied back to the source.
    pub both_sides: bool,
}

/// Dates closer than this are the same (listings are precise to the minute).
const SAME_TIME_SECS: i64 = 60;

impl SyncOptions {
    /// Is the source entry transferred, given the target entry of the same name?
    pub fn transfers(&self, src: &FolderEntry, dst: Option<&FolderEntry>) -> bool {
        let Some(dst) = dst else { return !self.existing_only };
        if src.is_dir || dst.is_dir != src.is_dir {
            return true;
        }
        let size_differs = self.compare_size && src.size != dst.size;
        let newer = self.compare_date && matches!((src.modified, dst.modified), (Some(a), Some(b)) if a > b + SAME_TIME_SECS);
        size_differs || newer
    }
}

/// Seconds since 1970 of a civil date and time (UTC; proleptic Gregorian calendar).
pub fn epoch_seconds(year: i64, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second)
}

/// `YYYY-MM-DD` (start of the day), as typed in the conditional transfer dialog.
pub fn parse_date(text: &str) -> Option<i64> {
    let mut it = text.trim().splitn(3, '-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| epoch_seconds(y, m, d, 0, 0, 0))
}

/// Options of an item and of the contents of its folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemOptions {
    pub filter: TransferFilter,
    /// Levels of sub-folders still entered (`None`: no limit; `Some(0)`: files only).
    pub depth: Option<u32>,
    /// Only the first entries of each folder, after sorting (0 = all).
    pub sub_count: u32,
    pub folder_sort: SortOrder,
    pub file_sort: SortOrder,
    /// The skip list is not applied (`RS_NOSKIP`).
    pub no_skiplist: bool,
    /// Retries of the items (`RetryCount`), instead of the settings.
    pub retries: Option<u32>,
    /// Folder listed again this many times to catch new entries (`RepeatCount`).
    pub repeat: u32,
    /// Stops the refreshes once a name of the folder matches (`CompleteFlag`, regex).
    pub complete_flag: Option<Pattern>,
    /// This item is a refresh of a folder already expanded.
    pub refresh: bool,
    /// Dates and sizes of a conditional transfer.
    pub conditions: Conditions,
    /// Synchronisation with the target.
    pub sync: Option<SyncOptions>,
}

impl ItemOptions {
    /// Do these options change anything?
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Options passed to the contents of a folder (one level deeper, not a refresh).
    pub fn for_contents(&self) -> Self {
        Self { depth: self.depth.map(|d| d.saturating_sub(1)), refresh: false, ..self.clone() }
    }

    /// Options of the refresh of a folder: one refresh less.
    pub fn for_refresh(&self) -> Self {
        Self { repeat: self.repeat.saturating_sub(1), refresh: true, ..self.clone() }
    }

    /// Does a name of the listing say the folder is complete?
    pub fn is_complete(&self, entries: &[FolderEntry]) -> bool {
        self.complete_flag.as_ref().is_some_and(|p| entries.iter().any(|e| p.matches(&e.name)))
    }

    /// Entries of a folder to transfer: filters and depth, sorted (folders first, then
    /// files), cut to the sub count.
    pub fn arrange(&self, entries: Vec<FolderEntry>) -> Vec<FolderEntry> {
        let keep_dirs = self.depth.is_none_or(|d| d > 0);
        let (mut dirs, mut files): (Vec<_>, Vec<_>) = entries
            .into_iter()
            .filter(|e| (keep_dirs || !e.is_dir) && self.filter.allows(&e.name, e.is_dir) && self.conditions.allows(e))
            .partition(|e| e.is_dir);
        sort(&mut dirs, self.folder_sort);
        sort(&mut files, self.file_sort);
        let mut all = dirs;
        all.extend(files);
        if self.sub_count > 0 {
            all.truncate(self.sub_count as usize);
        }
        all
    }
}

fn sort(v: &mut [FolderEntry], order: SortOrder) {
    v.sort_by(|a, b| {
        let c = match order.key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Date => a.modified.cmp(&b.modified),
            SortKey::Size => a.size.cmp(&b.size),
        };
        let c = c.then_with(|| a.name.cmp(&b.name));
        if order.descending { c.reverse() } else { c }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, is_dir: bool, size: u64, modified: i64) -> FolderEntry {
        FolderEntry { name: name.into(), is_dir, size: Some(size), modified: Some(modified) }
    }

    #[test]
    fn filters_with_lists_prefixes_and_regex() {
        let f = TransferFilter::parse("*2004*", "Sample", "*.rar;*.sfv", "%D%*proof*;*.nfo", false).unwrap();
        assert!(f.allows("Album.2004", true));
        assert!(!f.allows("Album.2005", true), "an allow mask exists for folders");
        assert!(!f.allows("Sample", true));
        assert!(!f.allows("x.2004.proof", true), "%D% moves the mask to the folders");
        assert!(f.allows("a.rar", false) && !f.allows("a.nfo", false) && !f.allows("a.mkv", false));
        assert!(TransferFilter::parse("", "", "", "", false).unwrap().allows("anything", true));

        let r = TransferFilter::parse("", "", r"^cd\d$", "", true).unwrap();
        assert!(r.allows("CD1", false) && !r.allows("cd12", false));
        assert!(TransferFilter::parse("(", "", "", "", true).is_err());
    }

    #[test]
    fn arrange_sorts_cuts_and_limits_depth() {
        let entries = vec![e("b", true, 0, 20), e("a", true, 0, 30), e("z.bin", false, 5, 10), e("y.bin", false, 9, 40)];
        let names = |o: &ItemOptions| o.arrange(entries.clone()).into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(&ItemOptions::default()), ["a", "b", "y.bin", "z.bin"]);
        let newest =
            ItemOptions { folder_sort: SortOrder { key: SortKey::Date, descending: true }, sub_count: 1, ..ItemOptions::default() };
        assert_eq!(names(&newest), ["a"], "latest folder only");
        let by_size =
            ItemOptions { file_sort: SortOrder { key: SortKey::Size, descending: false }, depth: Some(0), ..ItemOptions::default() };
        assert_eq!(names(&by_size), ["z.bin", "y.bin"], "depth 0: no sub-folder");
        assert_eq!(by_size.for_contents().depth, Some(0));
        assert_eq!(ItemOptions { depth: Some(2), ..ItemOptions::default() }.for_contents().depth, Some(1));
    }

    #[test]
    fn conditions_and_synchronisation() {
        let day = 86_400;
        let c = Conditions {
            newer_than: Some(10 * day),
            older_than: Some(20 * day),
            min_size: Some(5),
            max_size: Some(100),
            ..Conditions::default()
        };
        assert!(c.allows(&e("ok.bin", false, 50, 15 * day)));
        assert!(!c.allows(&e("old.bin", false, 50, 5 * day)));
        assert!(!c.allows(&e("new.bin", false, 50, 25 * day)));
        assert!(!c.allows(&e("big.bin", false, 500, 15 * day)));
        assert!(c.allows(&e("old-dir", true, 0, 5 * day)), "dates apply to files only by default");
        assert!(!Conditions { dates_on_folders: true, ..c }.allows(&e("old-dir", true, 0, 5 * day)));
        let unknown = FolderEntry { name: "x".into(), is_dir: false, size: None, modified: None };
        assert!(c.allows(&unknown));

        let s = SyncOptions { compare_date: true, compare_size: true, ..SyncOptions::default() };
        let src = e("a", false, 10, 1000);
        assert!(s.transfers(&src, None));
        assert!(!SyncOptions { existing_only: true, ..s }.transfers(&src, None));
        assert!(!s.transfers(&src, Some(&e("a", false, 10, 990))), "same size, date within a minute");
        assert!(s.transfers(&src, Some(&e("a", false, 9, 1000))), "size differs");
        assert!(s.transfers(&src, Some(&e("a", false, 10, 100))), "source newer");
        assert!(!s.transfers(&e("a", false, 10, 100), Some(&e("a", false, 10, 1000))), "target newer: kept");
        assert!(!SyncOptions::default().transfers(&src, Some(&e("a", false, 1, 1))), "nothing compared: the same");

        assert_eq!(epoch_seconds(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(epoch_seconds(2000, 3, 1, 0, 0, 0), 951_868_800);
        assert_eq!(parse_date("2024-02-29"), Some(epoch_seconds(2024, 2, 29, 0, 0, 0)));
        assert_eq!(parse_date("2024-13-01"), None);
        assert_eq!(parse_date("hier"), None);
    }

    #[test]
    fn refreshes_and_complete_flag() {
        let o = ItemOptions { repeat: 2, complete_flag: Some(Pattern::regex(r"\[.*COMPLETE.*\]").unwrap()), ..ItemOptions::default() };
        assert!(!o.is_default());
        let r = o.for_refresh();
        assert_eq!((r.repeat, r.refresh), (1, true));
        assert!(!r.for_contents().refresh);
        assert!(!o.is_complete(&[e("a.rar", false, 1, 1)]));
        assert!(o.is_complete(&[e("[ RLS - 100% COMPLETE ]", true, 0, 1)]));
    }
}
