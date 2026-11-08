//! Transfer queue: items, order (priorities then insertion), states, choice of the next
//! item to start according to each site's limits. No I/O here.

use std::collections::HashMap;
use std::sync::Arc;

use crate::itemopts::ItemOptions;
use crate::{TransferKind, TransferMode};

pub type ItemId = u64;

/// Item to insert: (source, destination, folder?, size, priority rank).
pub type NewItem = (Endpoint, Endpoint, bool, Option<u64>, Option<usize>);

/// End of a transfer: a site (by name) or the local machine.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    /// `None` = local disk.
    pub site: Option<String>,
    pub path: String,
}

impl Endpoint {
    pub fn local(path: impl Into<String>) -> Self {
        Self { site: None, path: path.into() }
    }
    pub fn remote(site: impl Into<String>, path: impl Into<String>) -> Self {
        Self { site: Some(site.into()), path: path.into() }
    }
    pub fn is_local(&self) -> bool {
        self.site.is_none()
    }
}

/// State of an item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    Waiting,
    Running,
    Done,
    /// Skipped (skip list, identical file already present...).
    Skipped(crate::skiplist::SkipReason),
    /// The target exists and the rule says "ask": waiting for a decision.
    NeedsConfirmation,
    /// Stopped by the user.
    Stopped,
    Failed(String),
}

impl ItemState {
    pub fn is_finished(&self) -> bool {
        matches!(self, ItemState::Done | ItemState::Skipped(_) | ItemState::Failed(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: ItemId,
    pub src: Endpoint,
    pub dst: Endpoint,
    pub is_dir: bool,
    /// Source size when known.
    pub size: Option<u64>,
    /// Rank in the priority list (0 = highest priority).
    pub priority: Option<usize>,
    pub state: ItemState,
    /// Attempts already made.
    pub attempts: u32,
    /// Bytes transferred during the last attempt.
    pub transferred: u64,
    /// Options from a script, passed down to the contents of a folder.
    pub options: Option<Arc<ItemOptions>>,
    /// Strict order (`RS_NOCIRCLE`): nothing of a sequence starts before every item of the
    /// smaller sequences is finished. The contents of a folder keep its sequence.
    pub sequence: Option<u64>,
    /// Binary, ASCII or by the ASCII list ("Edit queue"); the contents of a folder keep it.
    pub mode: TransferMode,
}

impl QueueItem {
    pub fn kind(&self) -> TransferKind {
        match (self.src.is_local(), self.dst.is_local()) {
            (true, _) => TransferKind::Upload,
            (false, true) => TransferKind::Download,
            (false, false) => TransferKind::Fxp,
        }
    }

    fn rank(&self) -> usize {
        self.priority.unwrap_or(usize::MAX)
    }
}

/// Connection limits of a site ("General" tab of the site properties in v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SiteLimits {
    /// Simultaneous transfers; 0 = single-connection mode (one transfer at a time).
    pub max_transfers: u32,
    /// Maximum transfers where the site is the source (download or FXP source).
    pub max_downloads: Option<u32>,
    /// Maximum transfers where the site is the destination (upload or FXP destination).
    pub max_uploads: Option<u32>,
}

impl Default for SiteLimits {
    fn default() -> Self {
        Self { max_transfers: 2, max_downloads: None, max_uploads: None }
    }
}

impl SiteLimits {
    /// Effective number of transfer connections (at least one).
    pub fn effective_transfers(&self) -> u32 {
        self.max_transfers.max(1)
    }
}

/// Counters of the transfers running for a site.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SiteLoad {
    pub transfers: u32,
    pub downloads: u32,
    pub uploads: u32,
}

/// Current load of every site, to decide whether an item may start.
#[derive(Debug, Clone, Default)]
pub struct Load {
    pub sites: HashMap<String, SiteLoad>,
    pub total: u32,
}

impl Load {
    fn get(&self, site: &str) -> SiteLoad {
        self.sites.get(site).copied().unwrap_or_default()
    }

    /// May the item start without exceeding the limits?
    pub fn allows(&self, item: &QueueItem, limits: &dyn Fn(&str) -> SiteLimits, max_total: Option<u32>) -> bool {
        if max_total.is_some_and(|m| self.total >= m) {
            return false;
        }
        if let Some(s) = &item.src.site {
            let (l, cur) = (limits(s), self.get(s));
            if cur.transfers >= l.effective_transfers() || l.max_downloads.is_some_and(|m| cur.downloads >= m) {
                return false;
            }
        }
        if let Some(d) = &item.dst.site {
            let (l, cur) = (limits(d), self.get(d));
            // Same site on both sides: two connections.
            let extra = u32::from(item.src.site.as_deref() == Some(d.as_str()));
            if cur.transfers + extra >= l.effective_transfers() || l.max_uploads.is_some_and(|m| cur.uploads >= m) {
                return false;
            }
        }
        true
    }

    pub fn start(&mut self, item: &QueueItem) {
        self.total += 1;
        if let Some(s) = &item.src.site {
            let e = self.sites.entry(s.clone()).or_default();
            e.transfers += 1;
            e.downloads += 1;
        }
        if let Some(d) = &item.dst.site {
            let e = self.sites.entry(d.clone()).or_default();
            e.transfers += 1;
            e.uploads += 1;
        }
    }

    pub fn finish(&mut self, item: &QueueItem) {
        self.total = self.total.saturating_sub(1);
        if let Some(s) = &item.src.site {
            let e = self.sites.entry(s.clone()).or_default();
            e.transfers = e.transfers.saturating_sub(1);
            e.downloads = e.downloads.saturating_sub(1);
        }
        if let Some(d) = &item.dst.site {
            let e = self.sites.entry(d.clone()).or_default();
            e.transfers = e.transfers.saturating_sub(1);
            e.uploads = e.uploads.saturating_sub(1);
        }
    }
}

/// The queue of one tab.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    items: Vec<QueueItem>,
    next_id: ItemId,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[QueueItem] {
        &self.items
    }

    pub fn get(&self, id: ItemId) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn get_mut(&mut self, id: ItemId) -> Option<&mut QueueItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    fn position(&self, id: ItemId) -> Option<usize> {
        self.items.iter().position(|i| i.id == id)
    }

    fn make(&mut self, src: Endpoint, dst: Endpoint, is_dir: bool, size: Option<u64>, priority: Option<usize>) -> QueueItem {
        self.next_id += 1;
        QueueItem {
            id: self.next_id,
            src,
            dst,
            is_dir,
            size,
            priority,
            state: ItemState::Waiting,
            attempts: 0,
            transferred: 0,
            options: None,
            sequence: None,
            mode: TransferMode::Auto,
        }
    }

    /// Adds an item: it goes before the waiting items with a lower priority.
    pub fn push(&mut self, src: Endpoint, dst: Endpoint, is_dir: bool, size: Option<u64>, priority: Option<usize>) -> ItemId {
        let item = self.make(src, dst, is_dir, size, priority);
        let pos = self.items.iter().position(|i| i.state == ItemState::Waiting && i.rank() > item.rank()).unwrap_or(self.items.len());
        let id = item.id;
        self.items.insert(pos, item);
        id
    }

    /// Inserts the contents of a folder right after it, sorted by priority (stable order).
    /// They get the sequence of the folder and `options`.
    pub fn insert_children(&mut self, parent: ItemId, children: Vec<NewItem>) -> Vec<ItemId> {
        self.insert_children_with(parent, children, None)
    }

    /// [`Queue::insert_children`] with the options of the contents.
    pub fn insert_children_with(&mut self, parent: ItemId, children: Vec<NewItem>, options: Option<Arc<ItemOptions>>) -> Vec<ItemId> {
        let sequence = self.get(parent).and_then(|p| p.sequence);
        let mode = self.get(parent).map_or(TransferMode::Auto, |p| p.mode);
        let mut made: Vec<QueueItem> = children
            .into_iter()
            .map(|(s, d, dir, size, prio)| {
                let mut it = self.make(s, d, dir, size, prio);
                it.options.clone_from(&options);
                it.sequence = sequence;
                it.mode = mode;
                it
            })
            .collect();
        made.sort_by_key(QueueItem::rank);
        let ids = made.iter().map(|i| i.id).collect();
        let at = self.position(parent).map_or(self.items.len(), |p| p + 1);
        self.items.splice(at..at, made);
        ids
    }

    /// Appends items in this order, with their state (queue loaded from disk).
    pub fn append(&mut self, items: impl IntoIterator<Item = (Endpoint, Endpoint, bool, Option<u64>, Option<usize>, ItemState)>) {
        for (src, dst, is_dir, size, priority, state) in items {
            let mut item = self.make(src, dst, is_dir, size, priority);
            item.state = state;
            self.items.push(item);
        }
    }

    /// First waiting item allowed by `can_start` and by the strict order of the sequences.
    pub fn next_runnable(&self, mut can_start: impl FnMut(&QueueItem) -> bool) -> Option<ItemId> {
        let first_sequence =
            self.items.iter().filter(|i| matches!(i.state, ItemState::Waiting | ItemState::Running)).filter_map(|i| i.sequence).min();
        self.items
            .iter()
            .find(|i| i.state == ItemState::Waiting && (i.sequence.is_none() || i.sequence == first_sequence) && can_start(i))
            .map(|i| i.id)
    }

    /// Options and sequence of an item queued by a script.
    pub fn set_options(&mut self, id: ItemId, options: Option<Arc<ItemOptions>>, sequence: Option<u64>) {
        if let Some(i) = self.get_mut(id) {
            i.options = options;
            i.sequence = sequence;
        }
    }

    /// A new sequence number, after every one already used.
    pub fn next_sequence(&self) -> u64 {
        self.items.iter().filter_map(|i| i.sequence).max().map_or(1, |m| m + 1)
    }

    /// Is this transfer already in the queue, not finished?
    pub fn contains_pending(&self, src: &Endpoint, dst: &Endpoint) -> bool {
        self.items.iter().any(|i| &i.src == src && &i.dst == dst && !i.state.is_finished())
    }

    pub fn set_state(&mut self, id: ItemId, state: ItemState) {
        if let Some(i) = self.get_mut(id) {
            i.state = state;
        }
    }

    pub fn remove(&mut self, id: ItemId) -> Option<QueueItem> {
        self.position(id).map(|p| self.items.remove(p))
    }

    /// Removes the items matching the filter (except running ones); returns their number.
    pub fn remove_where(&mut self, mut pred: impl FnMut(&QueueItem) -> bool) -> usize {
        let before = self.items.len();
        self.items.retain(|i| i.state == ItemState::Running || !pred(i));
        before - self.items.len()
    }

    /// Puts failed or stopped items back to waiting.
    pub fn reset_failed(&mut self) -> usize {
        let mut n = 0;
        for i in &mut self.items {
            if matches!(i.state, ItemState::Failed(_) | ItemState::Stopped) {
                i.state = ItemState::Waiting;
                i.attempts = 0;
                n += 1;
            }
        }
        n
    }

    /// Puts the failed items (not the stopped ones) back to waiting; returns their number.
    pub fn retry_failed_only(&mut self) -> usize {
        let mut n = 0;
        for i in self.items.iter_mut().filter(|i| matches!(i.state, ItemState::Failed(_))) {
            i.state = ItemState::Waiting;
            i.attempts = 0;
            n += 1;
        }
        n
    }

    pub fn move_up(&mut self, id: ItemId) {
        if let Some(p) = self.position(id).filter(|&p| p > 0) {
            self.items.swap(p, p - 1);
        }
    }

    pub fn move_down(&mut self, id: ItemId) {
        if let Some(p) = self.position(id).filter(|&p| p + 1 < self.items.len()) {
            self.items.swap(p, p + 1);
        }
    }

    pub fn move_to_top(&mut self, id: ItemId) {
        if let Some(p) = self.position(id) {
            let it = self.items.remove(p);
            self.items.insert(0, it);
        }
    }

    pub fn move_to_bottom(&mut self, id: ItemId) {
        if let Some(p) = self.position(id) {
            let it = self.items.remove(p);
            self.items.push(it);
        }
    }

    /// Is any item still waiting or running?
    pub fn has_pending(&self) -> bool {
        self.items.iter().any(|i| matches!(i.state, ItemState::Waiting | ItemState::Running))
    }

    /// Changes the source and target of an item that is not running ("Edit queue"); it
    /// waits again, with its attempts reset. `false` when it runs or does not exist.
    pub fn edit(&mut self, id: ItemId, src: Endpoint, dst: Endpoint) -> bool {
        match self.get_mut(id) {
            Some(i) if i.state != ItemState::Running => {
                i.src = src;
                i.dst = dst;
                i.state = ItemState::Waiting;
                i.attempts = 0;
                i.transferred = 0;
                true
            }
            _ => false,
        }
    }

    /// Transfer mode of an item that is not running.
    pub fn set_mode(&mut self, id: ItemId, mode: TransferMode) -> bool {
        match self.get_mut(id) {
            Some(i) if i.state != ItemState::Running => {
                i.mode = mode;
                true
            }
            _ => false,
        }
    }

    /// Swaps source and target of a site-to-site item that is not running ("Invert FXP").
    pub fn invert_fxp(&mut self, id: ItemId) -> bool {
        match self.get_mut(id) {
            Some(i) if i.state != ItemState::Running && i.kind() == TransferKind::Fxp => {
                std::mem::swap(&mut i.src, &mut i.dst);
                i.state = ItemState::Waiting;
                i.attempts = 0;
                true
            }
            _ => false,
        }
    }

    /// Marks as failed the given items that are still to do (waiting, stopped, waiting for
    /// a decision); returns how many changed. Running and finished items are left alone.
    pub fn mark_failed(&mut self, ids: &[ItemId], reason: &str) -> usize {
        let mut n = 0;
        for i in self.items.iter_mut().filter(|i| ids.contains(&i.id)) {
            if matches!(i.state, ItemState::Waiting | ItemState::Stopped | ItemState::NeedsConfirmation) {
                i.state = ItemState::Failed(reason.to_string());
                n += 1;
            }
        }
        n
    }

    /// Ids of every item.
    pub fn ids(&self) -> Vec<ItemId> {
        self.items.iter().map(|i| i.id).collect()
    }

    /// Removes the failed items ("Clear all failed"); returns their number.
    pub fn clear_failed(&mut self) -> usize {
        self.remove_where(|i| matches!(i.state, ItemState::Failed(_)))
    }

    /// Counts and sizes of the queue ("Queue information").
    pub fn stats(&self) -> QueueStats {
        let mut s = QueueStats::default();
        for i in &self.items {
            match &i.state {
                ItemState::Waiting => s.waiting += 1,
                ItemState::Running => s.running += 1,
                ItemState::Done => s.done += 1,
                ItemState::Skipped(_) => s.skipped += 1,
                ItemState::NeedsConfirmation => s.to_confirm += 1,
                ItemState::Stopped => s.stopped += 1,
                ItemState::Failed(_) => s.failed += 1,
            }
            if i.is_dir {
                s.folders += 1;
                continue;
            }
            s.files += 1;
            match i.kind() {
                TransferKind::Upload => s.uploads += 1,
                TransferKind::Download => s.downloads += 1,
                TransferKind::Fxp => s.fxp += 1,
            }
            let size = i.size.unwrap_or(0);
            s.total_bytes += size;
            match i.state {
                ItemState::Done => s.done_bytes += size,
                ItemState::Waiting | ItemState::Running | ItemState::Stopped | ItemState::NeedsConfirmation => {
                    s.left_bytes += size;
                    s.unknown_sizes += u32::from(i.size.is_none());
                }
                _ => {}
            }
        }
        s
    }
}

/// Counts and sizes of a queue (folders not yet expanded count as one item, without size).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueueStats {
    pub waiting: u32,
    pub running: u32,
    pub done: u32,
    pub skipped: u32,
    pub to_confirm: u32,
    pub stopped: u32,
    pub failed: u32,
    pub files: u32,
    pub folders: u32,
    pub uploads: u32,
    pub downloads: u32,
    pub fxp: u32,
    /// Sum of the known file sizes.
    pub total_bytes: u64,
    pub done_bytes: u64,
    /// Known size of the files left to transfer.
    pub left_bytes: u64,
    /// Files left whose size is not known yet.
    pub unknown_sizes: u32,
}

impl QueueStats {
    pub fn items(&self) -> u32 {
        self.files + self.folders
    }

    /// Time left at `rate` bytes per second (`None` without rate or with nothing left).
    pub fn eta(&self, rate: u64) -> Option<std::time::Duration> {
        (rate > 0 && self.left_bytes > 0).then(|| std::time::Duration::from_secs(self.left_bytes.div_ceil(rate)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(q: &mut Queue, name: &str, prio: Option<usize>) -> ItemId {
        q.push(Endpoint::local(format!("C:/{name}")), Endpoint::remote("site", format!("/{name}")), false, None, prio)
    }

    fn names(q: &Queue) -> Vec<String> {
        q.items().iter().map(|i| i.dst.path.trim_start_matches('/').to_string()).collect()
    }

    #[test]
    fn priorities_and_children_order() {
        let mut q = Queue::new();
        up(&mut q, "a.r00", None);
        up(&mut q, "b.r01", None);
        up(&mut q, "c.nfo", Some(1));
        up(&mut q, "d.sfv", Some(0));
        assert_eq!(names(&q), ["d.sfv", "c.nfo", "a.r00", "b.r01"]);

        let dir = q.push(Endpoint::remote("s", "/rel"), Endpoint::local("C:/rel"), true, None, None);
        let kids = vec![
            (Endpoint::remote("s", "/rel/x.rar"), Endpoint::local("C:/rel/x.rar"), false, Some(1), None),
            (Endpoint::remote("s", "/rel/x.sfv"), Endpoint::local("C:/rel/x.sfv"), false, Some(1), Some(0)),
        ];
        q.insert_children(dir, kids);
        let tail: Vec<_> = q.items()[4..].iter().map(|i| i.src.path.clone()).collect();
        assert_eq!(tail, ["/rel", "/rel/x.sfv", "/rel/x.rar"]);
        assert_eq!(q.items()[5].kind(), TransferKind::Download);
    }

    #[test]
    fn limits_and_single_connection_mode() {
        let mut q = Queue::new();
        for n in ["1", "2", "3"] {
            up(&mut q, n, None);
        }
        let limits = |_: &str| SiteLimits { max_transfers: 0, ..SiteLimits::default() };
        let mut load = Load::default();
        let first = q.next_runnable(|i| load.allows(i, &limits, None)).unwrap();
        let item = q.get(first).unwrap().clone();
        load.start(&item);
        q.set_state(first, ItemState::Running);
        assert!(q.next_runnable(|i| load.allows(i, &limits, None)).is_none(), "0 = a single connection");
        load.finish(&item);
        q.set_state(first, ItemState::Done);
        assert!(q.next_runnable(|i| load.allows(i, &limits, None)).is_some());

        let dl_limited = |_: &str| SiteLimits { max_transfers: 5, max_downloads: Some(0), max_uploads: None };
        let fxp = q.make(Endpoint::remote("a", "/x"), Endpoint::remote("b", "/x"), false, None, None);
        assert!(!Load::default().allows(&fxp, &dl_limited, None));
        assert!(!Load { total: 4, ..Load::default() }.allows(&fxp, &|_| SiteLimits::default(), Some(4)));
    }

    #[test]
    fn moves_removal_and_reset() {
        let mut q = Queue::new();
        let a = up(&mut q, "a", None);
        let b = up(&mut q, "b", None);
        let c = up(&mut q, "c", None);
        q.move_to_top(c);
        q.move_down(a);
        assert_eq!(names(&q), ["c", "b", "a"]);
        q.move_to_bottom(c);
        q.move_up(a);
        assert_eq!(names(&q), ["a", "b", "c"]);
        q.set_state(b, ItemState::Failed("x".into()));
        q.set_state(c, ItemState::Running);
        assert_eq!(q.reset_failed(), 1);
        assert_eq!(q.get(b).unwrap().state, ItemState::Waiting);
        assert_eq!(q.remove_where(|_| true), 2, "the running item stays");
        assert_eq!(q.items().len(), 1);
    }

    #[test]
    fn edit_invert_mark_and_clear() {
        let mut q = Queue::new();
        let a = up(&mut q, "a", None);
        let f = q.push(Endpoint::remote("A", "/rel/x"), Endpoint::remote("B", "/in/x"), false, Some(5), None);
        let r = up(&mut q, "r", None);
        q.set_state(r, ItemState::Running);
        q.set_state(a, ItemState::Failed("550".into()));

        assert!(q.edit(a, Endpoint::local("D:/a"), Endpoint::remote("other", "/x/a")));
        let it = q.get(a).unwrap();
        assert_eq!((it.src.path.as_str(), it.dst.site.as_deref(), &it.state), ("D:/a", Some("other"), &ItemState::Waiting));
        assert!(!q.edit(r, Endpoint::local("x"), Endpoint::local("y")), "running items are not edited");

        assert!(!q.invert_fxp(a), "not an FXP item");
        assert!(q.invert_fxp(f));
        let it = q.get(f).unwrap();
        assert_eq!((it.src.site.as_deref(), it.dst.path.as_str()), (Some("B"), "/rel/x"));

        assert_eq!(q.mark_failed(&q.ids(), "marked"), 2, "the running item is left");
        assert_eq!(q.get(r).unwrap().state, ItemState::Running);
        assert_eq!(q.clear_failed(), 2);
        assert_eq!(q.ids(), [r]);
    }

    #[test]
    fn sequences_run_one_after_the_other_with_their_contents() {
        let mut q = Queue::new();
        let a = q.push(Endpoint::remote("s", "/A"), Endpoint::local("C:/A"), true, None, None);
        let b = q.push(Endpoint::remote("s", "/B"), Endpoint::local("C:/B"), true, None, None);
        let free = up(&mut q, "free", None);
        let s1 = q.next_sequence();
        q.set_options(a, None, Some(s1));
        q.set_options(b, None, Some(q.next_sequence()));
        let opts = Some(Arc::new(ItemOptions { sub_count: 3, ..ItemOptions::default() }));
        q.set_state(a, ItemState::Done);
        let kids = q.insert_children_with(a, vec![(Endpoint::remote("s", "/A/x"), Endpoint::local("C:/A/x"), false, None, None)], opts);
        assert_eq!(q.get(kids[0]).unwrap().sequence, Some(s1), "the contents keep the sequence");
        assert_eq!(q.get(kids[0]).unwrap().options.as_ref().map(|o| o.sub_count), Some(3));
        let all = |_: &QueueItem| true;
        assert_eq!(q.next_runnable(all), Some(kids[0]), "B waits for the contents of A");
        q.set_state(kids[0], ItemState::Running);
        assert_eq!(q.next_runnable(all), Some(free), "items without sequence are not held");
        q.set_state(kids[0], ItemState::Done);
        assert_eq!(q.next_runnable(all), Some(b));
        assert!(q.contains_pending(&Endpoint::remote("s", "/B"), &Endpoint::local("C:/B")));
        assert!(!q.contains_pending(&Endpoint::remote("s", "/A/x"), &Endpoint::local("C:/A/x")));
    }

    #[test]
    fn stats_count_states_and_sizes() {
        let mut q = Queue::new();
        let a = q.push(Endpoint::remote("s", "/a"), Endpoint::local("C:/a"), false, Some(100), None);
        q.push(Endpoint::remote("s", "/b"), Endpoint::local("C:/b"), false, Some(50), None);
        q.push(Endpoint::local("C:/c"), Endpoint::remote("s", "/c"), false, None, None);
        q.push(Endpoint::remote("s", "/dir"), Endpoint::local("C:/dir"), true, None, None);
        q.set_state(a, ItemState::Done);
        let s = q.stats();
        assert_eq!((s.files, s.folders, s.items(), s.downloads, s.uploads), (3, 1, 4, 2, 1));
        assert_eq!((s.done, s.waiting), (1, 3));
        assert_eq!((s.total_bytes, s.done_bytes, s.left_bytes, s.unknown_sizes), (150, 100, 50, 1));
        assert_eq!(s.eta(25), Some(std::time::Duration::from_secs(2)));
        assert_eq!(s.eta(0), None);
    }
}
