//! Main window: site tabs (containers), each with its dock (two file panels, queue, log),
//! quick-connect bar and status bar.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use std::time::Duration;

use eframe::egui::{self, RichText, Sense};
use egui_dock::{DockArea, DockState, NodeIndex, Style as DockStyle, TabViewer};
use egui_extras::{Column, TableBuilder};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use urftp_core::TransferKind;
use urftp_core::queue::{Endpoint, ItemId, ItemState, Queue, QueueItem};
use urftp_proto::engine::{Engine, EngineEvent, EngineOptions};
use urftp_proto::ftp::{FtpTime, LogLine, RemoteEntry, Security, TlsVerify, path as rpath};
use urftp_proto::session::{SiteProfile, SiteProtocol};
use urftp_proto::sftp::HostKeyPolicy;

use crate::browser::{self, BrowserCmd, BrowserEvent, BrowserHandle, TrustKind};
use crate::cmdline::StartupOptions;
use crate::options::{self, OptionsWindow};
use crate::sites::{self, SiteManager};
use crate::theme::{self, human_size};
use urftp_core::itemopts::{FolderEntry, ItemOptions};
use urftp_core::settings::{ConditionScheme, IdentSettings, QueueEnd};
use urftp_core::site::Site;
use urftp_i18n::{Lang, language, t};
use urftp_proto::ident::{IdentConfig, IdentServer};
use urftp_proto::throttle::RateLimiter;

mod favorites;
mod fileops;
mod logs;
mod raw;
mod rushapp;
#[cfg(test)]
mod shots;

use fileops::{ListState, Op, Parts};
use logs::LogChannel;
#[cfg(test)]
mod ui_tests;

// ====================================================================== model

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Left,
    Right,
    Queue,
    Log,
    TransferLog,
    SystemLog,
    Script,
}

#[derive(Clone)]
struct FileRow {
    name: String,
    is_dir: bool,
    size: u64,
    modified: String,
    /// Sort key of the date.
    mtime: i64,
    /// Date in seconds since 1970 (conditional transfer).
    epoch: Option<i64>,
    /// Permissions (`drwxr-xr-x`; local panels: Windows attributes `RHSA`), owner, group.
    perms: String,
    owner: String,
    group: String,
}

impl FileRow {
    fn from_remote(e: &RemoteEntry) -> Self {
        Self {
            name: e.name.clone(),
            is_dir: e.is_dir(),
            size: e.size,
            modified: e.modified.map(fmt_time).unwrap_or_default(),
            mtime: e.modified.map(time_key).unwrap_or(0),
            epoch: e.modified.map(|t| t.epoch(FtpTime::now_epoch())),
            perms: e.permissions.clone().unwrap_or_default(),
            owner: e.owner.clone().unwrap_or_default(),
            group: e.group.clone().unwrap_or_default(),
        }
    }
}

/// Windows attributes of a local item as letters: read-only, hidden, system, archive.
fn local_attributes(meta: &std::fs::Metadata) -> String {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let a = meta.file_attributes();
        [(0x1, 'R'), (0x2, 'H'), (0x4, 'S'), (0x20, 'A')].iter().filter(|(bit, _)| a & bit != 0).map(|(_, c)| *c).collect()
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
        String::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortCol {
    Name,
    Size,
    Modified,
}

/// Sorting of a panel; folders always stay first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sort {
    col: SortCol,
    asc: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self { col: SortCol::Name, asc: true }
    }
}

impl Sort {
    /// Click on a column: same column = reverse order, other column = ascending.
    fn toggle(self, col: SortCol) -> Self {
        if self.col == col { Self { col, asc: !self.asc } } else { Self { col, asc: true } }
    }
}

/// Date as a comparable number (unknown year = recent).
fn time_key(t: FtpTime) -> i64 {
    let y = i64::from(t.year.unwrap_or(9999));
    ((((y * 100 + i64::from(t.month)) * 100 + i64::from(t.day)) * 100 + i64::from(t.hour)) * 100 + i64::from(t.minute)) * 100
        + i64::from(t.second)
}

fn sort_rows(rows: &mut [FileRow], s: Sort) {
    rows.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| {
            let o = match s.col {
                SortCol::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortCol::Size => a.size.cmp(&b.size),
                SortCol::Modified => a.mtime.cmp(&b.mtime),
            };
            if s.asc { o } else { o.reverse() }
        })
    });
}

/// Translated reason why an item was skipped.
fn skip_text(r: &urftp_core::skiplist::SkipReason) -> String {
    use urftp_core::skiplist::SkipReason;
    match r {
        SkipReason::Rule(mask) => t!("skip.reason_rule", mask = mask),
        SkipReason::ZeroByte => t!("skip.reason_zero"),
        SkipReason::AlreadyPresent => t!("skip.reason_present"),
        SkipReason::Dupe => t!("skip.reason_dupe"),
        SkipReason::SeveralSfv => t!("skip.reason_several_sfv"),
    }
}

fn fmt_time(t: FtpTime) -> String {
    match t.year {
        // ISO order in English, day/month in French.
        Some(y) if language() == Lang::En => format!("{y:04}-{:02}-{:02} {:02}:{:02}", t.month, t.day, t.hour, t.minute),
        None if language() == Lang::En => format!("{:02}-{:02} {:02}:{:02}", t.month, t.day, t.hour, t.minute),
        Some(y) => format!("{:02}/{:02}/{y} {:02}:{:02}", t.day, t.month, t.hour, t.minute),
        None => format!("{:02}/{:02} {:02}:{:02}", t.day, t.month, t.hour, t.minute),
    }
}

#[derive(PartialEq)]
enum RemoteState {
    Connecting,
    Connected,
    Failed(String),
    Disconnected,
}

/// Keyboard cursor of a file list (the row that arrows move and Enter opens).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Cursor {
    row: Option<usize>,
    /// Bring the row into view on the next frame.
    scroll: bool,
}

/// Cursor movement in a file list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorMove {
    By(isize),
    First,
    Last,
}

/// Rows moved by Page Up / Page Down.
const PAGE_ROWS: isize = 15;

struct LocalView {
    path: PathBuf,
    path_edit: String,
    /// Rows shown (after the quick filter); [`ListState::all`] has the whole folder.
    rows: Vec<FileRow>,
    list: Box<ListState>,
    selected: BTreeSet<usize>,
    cursor: Cursor,
    error: Option<String>,
    sort: Sort,
}

impl LocalView {
    fn new(path: PathBuf) -> Self {
        let mut v = Self {
            path_edit: path.to_string_lossy().into_owned(),
            path,
            rows: Vec::new(),
            list: Box::default(),
            selected: BTreeSet::new(),
            cursor: Cursor::default(),
            error: None,
            sort: Sort::default(),
        };
        v.reload();
        v
    }

    fn parts(&mut self) -> Parts<'_> {
        Parts { list: &mut self.list, rows: &mut self.rows, selected: &mut self.selected, cursor: &mut self.cursor, sort: &mut self.sort }
    }

    fn reload(&mut self) {
        self.path_edit = self.path.to_string_lossy().into_owned();
        let dir = self.path_edit.clone();
        match std::fs::read_dir(&self.path) {
            Ok(rd) => {
                let rows: Vec<FileRow> = rd
                    .flatten()
                    .filter_map(|e| {
                        let meta = e.metadata().ok()?;
                        let secs = meta
                            .modified()
                            .ok()
                            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                            .and_then(|d| i64::try_from(d.as_secs()).ok());
                        let t = secs.map(urftp_proto::sftp::unix_to_ftptime);
                        Some(FileRow {
                            name: e.file_name().to_string_lossy().into_owned(),
                            is_dir: meta.is_dir(),
                            size: meta.len(),
                            modified: t.map(fmt_time).unwrap_or_default(),
                            mtime: t.map(time_key).unwrap_or(0),
                            epoch: secs,
                            perms: local_attributes(&meta),
                            owner: String::new(),
                            group: String::new(),
                        })
                    })
                    .collect();
                self.parts().load(rows, &dir);
                self.error = None;
            }
            Err(e) => {
                self.parts().load(Vec::new(), &dir);
                self.error = Some(e.to_string());
            }
        }
    }

    fn open(&mut self, name: &str) {
        self.path = self.path.join(name);
        self.reload();
    }

    fn up(&mut self) {
        if let Some(p) = self.path.parent() {
            self.path = p.to_path_buf();
            self.reload();
        }
    }
}

struct RemoteView {
    profile: SiteProfile,
    handle: BrowserHandle,
    dir: String,
    /// Text of the path field (Enter goes there).
    path_edit: String,
    rows: Vec<FileRow>,
    list: Box<ListState>,
    selected: BTreeSet<usize>,
    cursor: Cursor,
    state: RemoteState,
    /// Record of the site manager it came from (to store a trusted fingerprint there).
    site_id: Option<u64>,
    sort: Sort,
    /// Web address of the files of the site ("Copy HTTP URL").
    http: urftp_core::site::HttpMapping,
}

impl RemoteView {
    fn parts(&mut self) -> Parts<'_> {
        Parts { list: &mut self.list, rows: &mut self.rows, selected: &mut self.selected, cursor: &mut self.cursor, sort: &mut self.sort }
    }
}

enum Side {
    Local(LocalView),
    Remote(Box<RemoteView>),
}

impl Side {
    fn parts(&mut self) -> Parts<'_> {
        match self {
            Side::Local(l) => l.parts(),
            Side::Remote(r) => r.parts(),
        }
    }

    fn list(&self) -> &ListState {
        match self {
            Side::Local(l) => &l.list,
            Side::Remote(r) => &r.list,
        }
    }

    fn is_remote(&self) -> bool {
        matches!(self, Side::Remote(_))
    }

    fn rows(&self) -> &[FileRow] {
        match self {
            Side::Local(l) => &l.rows,
            Side::Remote(r) => &r.rows,
        }
    }

    fn selected(&self) -> &BTreeSet<usize> {
        match self {
            Side::Local(l) => &l.selected,
            Side::Remote(r) => &r.selected,
        }
    }

    fn selected_mut(&mut self) -> &mut BTreeSet<usize> {
        self.selection_mut().0
    }

    fn cursor(&self) -> Cursor {
        match self {
            Side::Local(l) => l.cursor,
            Side::Remote(r) => r.cursor,
        }
    }

    fn selection_mut(&mut self) -> (&mut BTreeSet<usize>, &mut Cursor) {
        match self {
            Side::Local(l) => (&mut l.selected, &mut l.cursor),
            Side::Remote(r) => (&mut r.selected, &mut r.cursor),
        }
    }

    /// Moves the keyboard cursor and selects like Explorer: the row alone, or with
    /// `extend` (Shift) every row from the previous cursor position.
    fn move_cursor(&mut self, m: CursorMove, extend: bool) {
        let n = self.rows().len();
        if n == 0 {
            return;
        }
        let last = n - 1;
        let (sel, cur) = self.selection_mut();
        let from = cur.row.map_or(0, |r| r.min(last));
        let to = match (m, cur.row) {
            (CursorMove::By(_), None) => 0,
            (CursorMove::By(d), Some(_)) => from.saturating_add_signed(d).min(last),
            (CursorMove::First, _) => 0,
            (CursorMove::Last, _) => last,
        };
        if extend {
            sel.extend(from.min(to)..=from.max(to));
        } else {
            sel.clear();
            sel.insert(to);
        }
        *cur = Cursor { row: Some(to), scroll: true };
    }

    /// Selects or unselects the row under the cursor (Ctrl+Space).
    fn toggle_cursor(&mut self) {
        let (sel, cur) = self.selection_mut();
        if let Some(r) = cur.row
            && !sel.remove(&r)
        {
            sel.insert(r);
        }
    }

    /// Folder under the cursor, or else the first selected folder (Enter).
    fn folder_to_open(&self) -> Option<String> {
        let at_cursor = self.cursor().row.and_then(|i| self.rows().get(i)).filter(|r| r.is_dir).map(|r| r.name.clone());
        at_cursor.or_else(|| self.picked().into_iter().find(|(_, d)| *d).map(|(n, _)| n))
    }

    /// Transfer end for a name of this panel.
    fn endpoint(&self, name: &str) -> Endpoint {
        match self {
            Side::Local(l) => Endpoint::local(l.path.join(name).to_string_lossy()),
            Side::Remote(r) => Endpoint::remote(r.profile.name.clone(), rpath::join(&r.dir, name)),
        }
    }

    fn title(&self) -> String {
        match self {
            Side::Local(_) => t!("pane.local"),
            Side::Remote(r) => format!("🌐 {}", r.profile.name),
        }
    }

    fn profile(&self) -> Option<&SiteProfile> {
        match self {
            Side::Remote(r) => Some(&r.profile),
            Side::Local(_) => None,
        }
    }

    fn sort(&self) -> Sort {
        match self {
            Side::Local(l) => l.sort,
            Side::Remote(r) => r.sort,
        }
    }

    /// Changes the sort and reorders the list (the selection is cleared).
    fn set_sort(&mut self, s: Sort) {
        let mut p = self.parts();
        *p.sort = s;
        sort_rows(&mut p.list.all, s);
        p.selected.clear();
        *p.cursor = Cursor::default();
        p.show();
    }

    /// Full path of a name (display, clipboard, operations).
    fn full_path(&self, name: &str) -> String {
        match self {
            Side::Local(l) => l.path.join(name).to_string_lossy().into_owned(),
            Side::Remote(r) => rpath::join(&r.dir, name),
        }
    }

    /// Selected items: (name, folder?).
    fn picked(&self) -> Vec<(String, bool)> {
        self.selected().iter().filter_map(|&i| self.rows().get(i)).map(|r| (r.name.clone(), r.is_dir)).collect()
    }
}

/// Drag and drop: selected files of a panel.
#[derive(Debug, Clone, Copy)]
struct DragFiles {
    from: usize,
}

/// Dialog waiting for input or confirmation.
enum Prompt {
    NewFolder { side: usize, name: String },
    Rename { side: usize, from: String, to: String },
    Delete { side: usize, items: Vec<(String, bool)> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Up,
    Down,
    Top,
    Bottom,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LogKind {
    Command,
    Reply,
    Status,
    Error,
}

struct LogEntry {
    side: usize,
    kind: LogKind,
    text: String,
    channel: LogChannel,
    /// Site of the panel when the line was written (empty: local).
    site: String,
    /// Local time, `HH:MM:SS`.
    time: String,
}

struct TrustPrompt {
    side: usize,
    kind: TrustKind,
    fingerprint: String,
    reason: String,
}

/// A site tab (FTPRush "container").
struct Container {
    id: u32,
    dock: DockState<Pane>,
    sides: [Side; 2],
    /// The other view of each panel, kept while the panel shows the first one: the local
    /// folder behind a connection, or the connection (still open) behind the local folder.
    parked: [Option<Side>; 2],
    queue: Arc<Mutex<Queue>>,
    progress: HashMap<ItemId, (u64, Option<u64>)>,
    engine_events: Option<UnboundedReceiver<EngineEvent>>,
    engine_running: Arc<AtomicBool>,
    log: Vec<LogEntry>,
    trust: Option<TrustPrompt>,
    /// Running engine (to stop an item).
    engine: Option<Arc<Engine>>,
    prompt: Option<Prompt>,
    /// Panel active for the keyboard (0 = left, 1 = right).
    focus: usize,
    /// Script console: script text, expression to evaluate, results (success?, text).
    script: String,
    invoke: String,
    script_out: Vec<(bool, String)>,
    /// Replies to raw commands (`RushApp.FTP.RAW`): (site, command, lines, success).
    raw_replies: Vec<(String, String, Vec<String>, bool)>,
    /// Selected rows of the queue, and the row a `Shift` range starts from.
    queue_sel: BTreeSet<ItemId>,
    queue_anchor: Option<ItemId>,
    /// Item being edited ("Edit queue").
    queue_edit: Option<QueueEdit>,
    /// "Queue information" window.
    queue_info: bool,
    /// The engine of the tab has just finished (end-of-queue action).
    just_finished: bool,
    /// `RS_LOGOUT`: log the panels out once the queue is finished.
    logout_when_done: bool,
    /// `RS_DELEMPTYTABS`: close the tab once its queue is finished and nothing is connected.
    close_when_done: bool,
    /// "Conditional transfer" dialog of a panel.
    condition: Option<ConditionDialog>,
    /// Messages of work done in the background (SFV / MD5 files).
    background: Vec<std::sync::mpsc::Receiver<(LogKind, String)>>,
    /// Profiles of the addresses queued by "Manual download" (not in the site manager).
    adhoc: Vec<SiteProfile>,
    /// Dialogs, windows and background work of the file operations.
    tools: fileops::Tools,
}

/// Form of the conditional transfer dialog for the selection of panel `side`.
struct ConditionDialog {
    side: usize,
    form: ConditionScheme,
    error: Option<String>,
}

/// Form of the "Edit queue" dialog: an empty site is the local disk.
#[derive(Debug, Clone, PartialEq, Eq)]
struct QueueEdit {
    id: ItemId,
    mode: urftp_core::TransferMode,
    src_site: String,
    src_path: String,
    dst_site: String,
    dst_path: String,
    /// Shown when both ends are local.
    error: bool,
}

impl QueueEdit {
    fn of(it: &QueueItem) -> Self {
        Self {
            id: it.id,
            mode: it.mode,
            src_site: it.src.site.clone().unwrap_or_default(),
            src_path: it.src.path.clone(),
            dst_site: it.dst.site.clone().unwrap_or_default(),
            dst_path: it.dst.path.clone(),
            error: false,
        }
    }

    fn endpoint(site: &str, path: &str) -> Endpoint {
        if site.is_empty() { Endpoint::local(path.trim()) } else { Endpoint::remote(site, path.trim()) }
    }
}

fn default_dock() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Left]);
    let surface = dock.main_surface_mut();
    let [top, _] =
        surface.split_below(NodeIndex::root(), 0.66, vec![Pane::Queue, Pane::Log, Pane::TransferLog, Pane::SystemLog, Pane::Script]);
    surface.split_right(top, 0.5, vec![Pane::Right]);
    dock
}

/// Default download folder chosen in the options (see [`set_download_dir`]).
static DOWNLOAD_DIR: parking_lot::RwLock<Option<PathBuf>> = parking_lot::const_rwlock(None);

/// Records the default download folder of the options (empty: the user folder).
fn set_download_dir(dir: &str) {
    let dir = dir.trim();
    *DOWNLOAD_DIR.write() = (!dir.is_empty()).then(|| PathBuf::from(dir));
}

/// Starting folder of the local panels: the default download folder when it exists, else
/// the user folder (a scratch folder in the interface tests).
fn home_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(dir) = ui_tests::HOME.get() {
        return dir.clone();
    }
    if let Some(d) = DOWNLOAD_DIR.read().clone().filter(|d| d.is_dir()) {
        return d;
    }
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\"))
}

impl Container {
    fn new(id: u32) -> Self {
        Self {
            id,
            dock: default_dock(),
            sides: [Side::Local(LocalView::new(home_dir())), Side::Local(LocalView::new(home_dir()))],
            parked: [None, None],
            queue: Arc::new(Mutex::new(Queue::new())),
            progress: HashMap::new(),
            engine_events: None,
            engine_running: Arc::new(AtomicBool::new(false)),
            log: Vec::new(),
            trust: None,
            engine: None,
            prompt: None,
            focus: 0,
            script: "RushApp.FTP.RAW('', 'NOOP', 0);".into(),
            invoke: "RushApp.Version".into(),
            script_out: Vec::new(),
            raw_replies: Vec::new(),
            queue_sel: BTreeSet::new(),
            queue_anchor: None,
            queue_edit: None,
            queue_info: false,
            just_finished: false,
            logout_when_done: false,
            close_when_done: false,
            condition: None,
            background: Vec::new(),
            adhoc: Vec::new(),
            tools: fileops::Tools::default(),
        }
    }

    /// Displayed title; `pos` = position of the tab (1, 2, 3...), renumbered on close.
    fn title(&self, pos: usize) -> String {
        let remote = [&self.sides[1], &self.sides[0]]
            .into_iter()
            .chain(self.parked[1].iter().chain(self.parked[0].iter()))
            .find_map(|s| if let Side::Remote(r) = s { Some(r.profile.host.clone()) } else { None });
        remote.unwrap_or_else(|| t!("tabs.default_title", n = pos))
    }

    /// Line of the system log (see [`Container::push_ftp`], [`Container::push_transfer`]).
    fn push_log(&mut self, side: usize, kind: LogKind, text: impl Into<String>) {
        self.push_entry(side, LogChannel::System, kind, text);
    }

    /// Collects the events of the browsing connections and of the engine. Returns the bytes
    /// transferred since the last call, per direction (for statistics and rates).
    /// Source → target of an item, for the log.
    fn item_paths(&self, id: ItemId) -> String {
        let q = self.queue.lock();
        let end = |e: &Endpoint| match &e.site {
            Some(site) => format!("{site}:{}", e.path),
            None => e.path.clone(),
        };
        q.get(id).map(|i| format!("{} → {}", end(&i.src), end(&i.dst))).unwrap_or_default()
    }

    /// Writes an SFV (`md5` false) or MD5 file of the selected files of the local panel
    /// `side`, named after its folder, in the background.
    fn write_checksums(&mut self, side: usize, md5: bool, ctx: &egui::Context) {
        let Side::Local(l) = &self.sides[side] else { return };
        let dir = l.path.clone();
        let files: Vec<String> = self.sides[side].picked().into_iter().filter(|(_, d)| !d).map(|(n, _)| n).collect();
        if files.is_empty() {
            return;
        }
        let base = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "files".into());
        let target = dir.join(format!("{base}.{}", if md5 { "md5" } else { "sfv" }));
        let (tx, rx) = std::sync::mpsc::channel();
        self.background.push(rx);
        self.push_log(side, LogKind::Status, t!("log.checksum_started", n = files.len(), path = target.display()));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            use urftp_core::checksum::{file_crc32, file_md5, write_md5, write_sfv};
            let result = if md5 {
                files
                    .iter()
                    .map(|n| file_md5(&dir.join(n)).map(|s| (n.clone(), s)))
                    .collect::<std::io::Result<Vec<_>>>()
                    .map(|v| write_md5(&v))
            } else {
                files
                    .iter()
                    .map(|n| file_crc32(&dir.join(n)).map(|c| (n.clone(), c)))
                    .collect::<std::io::Result<Vec<_>>>()
                    .map(|v| write_sfv(&v))
            };
            let msg = match result.and_then(|text| std::fs::write(&target, text)) {
                Ok(()) => (LogKind::Status, t!("log.checksum_written", path = target.display())),
                Err(e) => (LogKind::Error, t!("log.checksum_failed", path = target.display(), error = e)),
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    /// `log_paths`: source and target of each item in the log. `invalidate`: the cached
    /// listing of the folder a transfer wrote into is forgotten.
    fn poll(&mut self, log_paths: bool, invalidate: bool) -> Vec<(TransferKind, u64)> {
        let mut done = Vec::new();
        self.background.retain(|rx| match rx.try_recv() {
            Ok(m) => {
                done.push(m);
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => false,
        });
        let refresh = !done.is_empty();
        for (kind, text) in done {
            self.push_log(0, kind, text);
        }
        if refresh {
            self.refresh_both();
        }
        let mut traffic = Vec::new();
        for side in 0..2 {
            let mut events = Vec::new();
            if let Side::Remote(r) = &self.sides[side] {
                while let Ok(e) = r.handle.rx.try_recv() {
                    events.push(e);
                }
            }
            for e in events {
                self.on_browser_event(side, e);
            }
        }
        let mut finished = Vec::new();
        if let Some(rx) = self.engine_events.as_mut() {
            while let Ok(e) = rx.try_recv() {
                finished.push(e);
            }
        }
        for e in finished {
            match e {
                EngineEvent::Progress { id, bytes, total } => {
                    let before = self.progress.insert(id, (bytes, total)).map_or(0, |(b, _)| b);
                    let kind = self.queue.lock().get(id).map(QueueItem::kind);
                    // FXP does not go through this computer: it does not count in the rates.
                    if let Some(k @ (TransferKind::Upload | TransferKind::Download)) = kind {
                        traffic.push((k, bytes.saturating_sub(before)));
                    }
                }
                EngineEvent::Started { id } if log_paths => {
                    let paths = self.item_paths(id);
                    self.push_transfer(0, LogKind::Status, t!("log.item_started", paths = paths));
                }
                EngineEvent::Done { id, bytes } => {
                    if self.queue.lock().get(id).map(QueueItem::kind) == Some(TransferKind::Fxp) {
                        traffic.push((TransferKind::Fxp, bytes));
                    }
                    let dst = self.queue.lock().get(id).map(|i| i.dst.clone());
                    if invalidate && let Some(Endpoint { site: Some(site), path }) = dst {
                        let parent = rpath::split(&path).0.unwrap_or("/").to_string();
                        for s in self.sides.iter().chain(self.parked.iter().flatten()) {
                            if let Side::Remote(r) = s
                                && r.profile.name == site
                            {
                                r.handle.send(BrowserCmd::Invalidate(vec![parent.clone(), path.clone()]));
                            }
                        }
                    }
                    if log_paths {
                        let paths = self.item_paths(id);
                        self.push_transfer(0, LogKind::Status, t!("log.item_done", paths = paths));
                    }
                }
                EngineEvent::Paused { id } if log_paths => {
                    let paths = self.item_paths(id);
                    self.push_transfer(0, LogKind::Status, t!("log.item_paused", paths = paths));
                }
                EngineEvent::CrcChecked { path, ok, .. } => {
                    let (kind, text) = if ok {
                        (LogKind::Status, t!("log.crc_ok", path = path))
                    } else {
                        (LogKind::Error, t!("log.crc_bad", path = path))
                    };
                    self.push_transfer(0, kind, text);
                }
                EngineEvent::SyncDeleted { path, .. } => self.push_transfer(0, LogKind::Status, t!("log.sync_deleted", path = path)),
                EngineEvent::RetryRound { round, items } => {
                    self.push_transfer(0, LogKind::Status, t!("log.retry_round", round = round, n = items))
                }
                EngineEvent::Failed { id, error } => self.push_transfer(0, LogKind::Error, t!("log.item_failed", id = id, error = error)),
                EngineEvent::Retrying { id, attempt, error } => {
                    self.push_transfer(0, LogKind::Status, t!("log.item_retry", id = id, attempt = attempt, error = error))
                }
                _ => {}
            }
        }
        if self.engine_events.is_some() && !self.engine_running.load(Ordering::SeqCst) {
            self.engine_events = None;
            self.engine = None;
            self.push_transfer(0, LogKind::Status, t!("log.queue_finished"));
            self.just_finished = true;
            self.refresh_both();
        }
        traffic
    }

    fn on_browser_event(&mut self, side: usize, e: BrowserEvent) {
        match e {
            BrowserEvent::RawListing { dir, text } => fileops::raw_listing(self, side, dir, text),
            BrowserEvent::Listing { dir, entries, .. } => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.path_edit.clone_from(&dir);
                    r.parts().load(entries.iter().map(FileRow::from_remote).collect(), &dir);
                    r.dir = dir;
                    r.state = RemoteState::Connected;
                }
            }
            BrowserEvent::Log(line) => {
                let (kind, text) = match line {
                    LogLine::Command(c) => (LogKind::Command, c),
                    LogLine::Reply(r) => (if r.code >= 400 { LogKind::Error } else { LogKind::Reply }, r.lines.join("\n")),
                    LogLine::Status(s) => (LogKind::Status, s),
                };
                self.push_ftp(side, kind, text);
            }
            BrowserEvent::Error(msg) => {
                if let Side::Remote(r) = &mut self.sides[side]
                    && r.state == RemoteState::Connecting
                {
                    r.state = RemoteState::Failed(msg.clone());
                }
                self.push_ftp(side, LogKind::Error, msg);
            }
            BrowserEvent::Trust { kind, fingerprint, reason } => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.state = RemoteState::Failed(t!("log.untrusted_server"));
                }
                self.push_ftp(side, LogKind::Error, t!("log.fingerprint", reason = reason, fingerprint = fingerprint));
                self.trust = Some(TrustPrompt { side, kind, fingerprint, reason });
            }
            BrowserEvent::Disconnected => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.state = RemoteState::Disconnected;
                    r.parts().load(Vec::new(), "");
                }
            }
            BrowserEvent::NotConnected => self.push_ftp(side, LogKind::Error, t!("browser.not_connected")),
            BrowserEvent::RawImpossible(command) => self.push_ftp(side, LogKind::Error, t!("browser.raw_impossible", command = command)),
            BrowserEvent::Reconnecting { attempt, of, reason } => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.state = RemoteState::Connecting;
                }
                self.push_ftp(side, LogKind::Status, t!("log.reconnecting", attempt = attempt, of = of, reason = reason));
            }
            BrowserEvent::Reconnected => self.push_ftp(side, LogKind::Status, t!("log.reconnected")),
            BrowserEvent::ReconnectFailed(error) => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.state = RemoteState::Failed(t!("log.reconnect_failed", error = &error));
                }
                self.push_ftp(side, LogKind::Error, t!("log.reconnect_failed", error = error));
            }
            BrowserEvent::KeepAliveStopped { logged_out } => {
                self.push_ftp(side, LogKind::Status, if logged_out { t!("log.keepalive_logout") } else { t!("log.keepalive_stopped") })
            }
            BrowserEvent::RawReply { command, lines, ok } => {
                let site = self.sides[side].profile().map(|p| p.name.clone()).unwrap_or_default();
                self.raw_replies.push((site, command, lines, ok));
                if self.raw_replies.len() > 100 {
                    self.raw_replies.remove(0);
                }
            }
        }
    }

    /// Shows the other view of panel `side`: the local folder behind a connection, or the
    /// connection behind the local folder. The connection stays open meanwhile.
    fn switch_side(&mut self, side: usize) {
        let remote = matches!(self.sides[side], Side::Remote(_));
        if !remote && self.parked[side].is_none() {
            return;
        }
        let next = self.parked[side].take().unwrap_or_else(|| Side::Local(LocalView::new(home_dir())));
        let shown = std::mem::replace(&mut self.sides[side], next);
        self.parked[side] = Some(shown);
        if let Side::Local(l) = &mut self.sides[side] {
            l.reload();
        }
    }

    /// Closes the connection of panel `side` (shown or kept behind the local folder); the
    /// panel shows its local folder.
    fn disconnect_side(&mut self, side: usize) {
        if let Some(Side::Remote(r)) = &self.parked[side] {
            r.handle.send(BrowserCmd::Disconnect);
            self.parked[side] = None;
        }
        if let Side::Remote(r) = &self.sides[side] {
            r.handle.send(BrowserCmd::Disconnect);
            let back = self.parked[side].take().unwrap_or_else(|| Side::Local(LocalView::new(home_dir())));
            self.sides[side] = back;
        }
    }

    /// Text of the switch button of panel `side`, when there is something to switch to.
    fn switch_label(&self, side: usize) -> Option<String> {
        match (&self.sides[side], &self.parked[side]) {
            (Side::Remote(_), _) => Some(t!("conn.to_local")),
            (Side::Local(_), Some(Side::Remote(r))) => Some(t!("conn.to_remote", site = r.profile.name)),
            _ => None,
        }
    }

    fn refresh_both(&mut self) {
        for s in &mut self.sides {
            match s {
                Side::Local(l) => l.reload(),
                Side::Remote(r) => r.handle.send(BrowserCmd::Refresh),
            }
        }
    }

    /// Adds the selection of panel `from` to the queue, towards the folder of the other panel.
    fn enqueue_selection(&mut self, from: usize) {
        self.enqueue_selection_with(from, None);
    }

    /// [`Container::enqueue_selection`] with the options of a conditional transfer: the
    /// selected items themselves must pass its filters and conditions. Returns how many
    /// items were queued.
    fn enqueue_selection_with(&mut self, from: usize, options: Option<Arc<ItemOptions>>) -> usize {
        let to = 1 - from;
        let picks: Vec<FileRow> = self.sides[from].selected().iter().filter_map(|&i| self.sides[from].rows().get(i).cloned()).collect();
        if picks.is_empty() {
            return 0;
        }
        if self.sides[from].profile().is_none() && self.sides[to].profile().is_none() {
            self.push_transfer(from, LogKind::Error, t!("log.local_to_local"));
            return 0;
        }
        let mut q = self.queue.lock();
        let mut added = 0;
        for row in &picks {
            if let Some(o) = &options {
                let entry = FolderEntry {
                    name: row.name.clone(),
                    is_dir: row.is_dir,
                    size: (!row.is_dir).then_some(row.size),
                    modified: row.epoch,
                };
                if !o.filter.allows(&row.name, row.is_dir) || !o.conditions.allows(&entry) {
                    continue;
                }
            }
            let src = self.sides[from].endpoint(&row.name);
            let dst = self.sides[to].endpoint(&row.name);
            let id = q.push(src, dst, row.is_dir, (!row.is_dir).then_some(row.size), None);
            q.set_options(id, options.clone(), None);
            added += 1;
        }
        drop(q);
        self.push_transfer(from, LogKind::Status, t!("log.items_queued", n = added));
        added
    }

    /// Starts the engine. `extra`: profiles of the queue sites missing from the panels
    /// (items added by a script, for example).
    fn start_queue(&mut self, rt: &Handle, ctx: &egui::Context, opts: EngineOptions, extra: &[SiteProfile]) {
        if self.engine_events.is_some() {
            return;
        }
        let mut sites: Vec<SiteProfile> = Vec::new();
        for p in self.sides.iter().chain(self.parked.iter().flatten()).filter_map(Side::profile).chain(extra) {
            if !sites.iter().any(|s| s.name == p.name) {
                sites.push(p.clone());
            }
        }
        let (tx, rx) = unbounded_channel();
        let engine = Engine::new(self.queue.clone(), sites, opts, Some(tx));
        self.engine = Some(engine.clone());
        let running = self.engine_running.clone();
        running.store(true, Ordering::SeqCst);
        let ctx = ctx.clone();
        rt.spawn(async move {
            engine.run().await;
            running.store(false, Ordering::SeqCst);
            ctx.request_repaint();
        });
        self.engine_events = Some(rx);
        self.push_transfer(0, LogKind::Status, t!("log.queue_started"));
    }

    // ------------------------------------------------------------------ file operations

    fn ask_new_folder(&mut self, side: usize) {
        self.prompt = Some(Prompt::NewFolder { side, name: t!("prompt.new_folder_default") });
    }

    fn ask_rename(&mut self, side: usize) {
        if let Some((name, _)) = self.sides[side].picked().into_iter().next() {
            self.prompt = Some(Prompt::Rename { side, to: name.clone(), from: name });
        }
    }

    fn ask_delete(&mut self, side: usize) {
        let items = self.sides[side].picked();
        if !items.is_empty() {
            self.prompt = Some(Prompt::Delete { side, items });
        }
    }

    /// Runs a confirmed dialog (local: at once; remote: through the panel connection).
    /// `enter`: go into a folder just created ("Enter a folder after creating it").
    fn run_prompt(&mut self, p: Prompt, enter: bool) {
        match p {
            Prompt::NewFolder { side, name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                let full = self.sides[side].full_path(&name);
                if let Side::Remote(r) = &self.sides[side] {
                    r.handle.send(BrowserCmd::Mkdir(full.clone()));
                    if enter {
                        r.handle.send(BrowserCmd::Cwd(full));
                    }
                } else {
                    match std::fs::create_dir(&full) {
                        Ok(()) => {
                            if let Side::Local(l) = &mut self.sides[side] {
                                if enter {
                                    l.path = PathBuf::from(&full);
                                }
                                l.reload();
                            }
                        }
                        Err(e) => self.push_log(side, LogKind::Error, t!("log.mkdir_failed", path = full, error = e)),
                    }
                }
            }
            Prompt::Rename { side, from, to } => {
                let to = to.trim().to_string();
                if to.is_empty() || to == from {
                    return;
                }
                let full = self.sides[side].full_path(&from);
                match &self.sides[side] {
                    Side::Local(_) => {
                        let dest = self.sides[side].full_path(&to);
                        match std::fs::rename(&full, &dest) {
                            Ok(()) => {
                                if let Side::Local(l) = &mut self.sides[side] {
                                    l.reload();
                                }
                            }
                            Err(e) => self.push_log(side, LogKind::Error, t!("log.rename_failed", path = full, error = e)),
                        }
                    }
                    Side::Remote(r) => r.handle.send(BrowserCmd::Rename(full, to)),
                }
            }
            Prompt::Delete { side, items } => {
                let full: Vec<(String, bool)> = items.iter().map(|(n, d)| (self.sides[side].full_path(n), *d)).collect();
                match &self.sides[side] {
                    Side::Local(_) => {
                        for (p, is_dir) in &full {
                            let r = if *is_dir { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
                            if let Err(e) = r {
                                self.push_log(side, LogKind::Error, t!("log.delete_failed", path = p, error = e));
                            }
                        }
                        if let Side::Local(l) = &mut self.sides[side] {
                            l.reload();
                        }
                    }
                    Side::Remote(r) => r.handle.send(BrowserCmd::Delete(full)),
                }
            }
        }
    }

    // ------------------------------------------------------------------ queue

    fn move_item(&mut self, id: ItemId, m: Move) {
        let mut q = self.queue.lock();
        match m {
            Move::Up => q.move_up(id),
            Move::Down => q.move_down(id),
            Move::Top => q.move_to_top(id),
            Move::Bottom => q.move_to_bottom(id),
        }
    }

    fn stop_item(&mut self, id: ItemId) {
        if let Some(e) = &self.engine {
            e.stop(id);
        }
    }

    fn stop_all(&mut self) {
        if let Some(e) = &self.engine {
            e.stop_all();
        }
    }

    fn paused(&self) -> bool {
        self.engine.as_ref().is_some_and(|e| e.is_paused())
    }

    /// Click on a queue row: alone, `Ctrl` adds or removes it, `Shift` takes the range from
    /// the previous click.
    fn select_queue_row(&mut self, id: ItemId, toggle: bool, range: bool) {
        let order = self.queue.lock().ids();
        match (range, self.queue_anchor) {
            (true, Some(anchor)) => {
                let pos = |x| order.iter().position(|&i| i == x);
                if let (Some(a), Some(b)) = (pos(anchor), pos(id)) {
                    self.queue_sel = order[a.min(b)..=a.max(b)].iter().copied().collect();
                }
            }
            _ if toggle => {
                if !self.queue_sel.remove(&id) {
                    self.queue_sel.insert(id);
                }
                self.queue_anchor = Some(id);
            }
            _ => {
                self.queue_sel = BTreeSet::from([id]);
                self.queue_anchor = Some(id);
            }
        }
    }

    /// Forgets the selected rows that left the queue.
    fn prune_queue_selection(&mut self) {
        let q = self.queue.lock();
        self.queue_sel.retain(|id| q.get(*id).is_some());
    }

    fn retry_item(&mut self, id: ItemId) {
        if let Some(it) = self.queue.lock().get_mut(id)
            && matches!(it.state, ItemState::Failed(_) | ItemState::Stopped | ItemState::NeedsConfirmation)
        {
            it.state = ItemState::Waiting;
            it.attempts = 0;
        }
    }
}

// ====================================================================== application

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuickProto {
    Ftp,
    FtpsExplicit,
    FtpsImplicit,
    Sftp,
}

impl QuickProto {
    fn label(self) -> String {
        match self {
            QuickProto::Ftp => t!("proto.ftp"),
            QuickProto::FtpsExplicit => t!("proto.ftps_explicit"),
            QuickProto::FtpsImplicit => t!("proto.ftps_implicit"),
            QuickProto::Sftp => t!("proto.sftp"),
        }
    }

    fn default_port(self) -> u16 {
        match self {
            QuickProto::Ftp | QuickProto::FtpsExplicit => 21,
            QuickProto::FtpsImplicit => 990,
            QuickProto::Sftp => 22,
        }
    }
}

struct QuickConnect {
    host: String,
    port: String,
    user: String,
    pass: String,
    proto: QuickProto,
}

pub struct UltraRushApp {
    rt: Handle,
    /// Queues left unfinished last time, waiting for the user's answer, with the ones
    /// ticked to be loaded.
    saved_queue: Option<(urftp_core::queuestore::SavedQueues, Vec<bool>)>,
    /// What was last written to `queue.json`, and when the queues were last compared.
    queue_written: Option<urftp_core::queuestore::SavedQueues>,
    queue_checked: std::time::Instant,
    /// Class of the mIRC link window applied (compatibility mode or not).
    ipc_compat_applied: Option<bool>,
    tabs: Vec<Container>,
    active: usize,
    next_id: u32,
    quick: QuickConnect,
    dark: bool,
    /// Automatic connection on the first frame (`URUSHFTP_CONNECT` variable).
    autoconnect: bool,
    sites: SiteManager,
    options: OptionsWindow,
    /// Speed limiters shared by every tab (set from the options).
    download_limit: Arc<RateLimiter>,
    upload_limit: Arc<RateLimiter>,
    ident: Option<IdentServer>,
    ident_applied: Option<IdentSettings>,
    /// Statistics (stored totals, session, rates) for `RushApp.*` and the status bar.
    stats: rushapp::Stats,
    /// Separator of the site lists in scripts (`RushApp.FTP.Splitter`).
    splitter: String,
    startup_script_done: bool,
    /// mIRC link: `TfmRush` window, requests received, pending `RAW` callbacks.
    ipc: Option<urftp_sys::server::Server>,
    ipc_rx: Option<std::sync::mpsc::Receiver<urftp_sys::server::Request>>,
    /// Callback window of the DLL while a script coming from mIRC runs.
    ipc_callback: Option<isize>,
    /// (site, command, callback window) of the `RAW` whose reply must go back to mIRC.
    raw_callbacks: Vec<(String, String, isize)>,
    /// Command-line options, applied on the first frame.
    startup: Option<StartupOptions>,
    /// `-rx`: quit when nothing is left to transfer.
    exit_when_done: bool,
    /// "Keyboard shortcuts" window (F1).
    shortcuts_open: bool,
    /// "About" window (Help menu).
    about_open: bool,
    /// End-of-queue action waiting for its countdown: (action, when it runs).
    queue_end: Option<(QueueEnd, std::time::Instant)>,
    /// Runs sleep / hibernate / shut down (replaced in the tests).
    power: fn(QueueEnd) -> Result<(), String>,
    /// mIRC link (`TfmRush` window); off in the interface tests.
    ipc_enabled: bool,
    /// `Ctrl+V` was down on the previous frame.
    paste_down: bool,
    /// Last time the log files were written.
    logs_flushed: std::time::Instant,
    /// "Send RAW command" dialog and the replies received.
    raw: raw::RawDialog,
    manual: favorites::ManualDownload,
    /// Clipboard sequence number last looked at (clipboard watch).
    clipboard_seq: Option<u32>,
}

/// `ftp://user:pass@host:port`, `ftps://` (explicit), `ftpsi://` (implicit), `sftp://`.
fn parse_connect_url(url: &str) -> Option<QuickConnect> {
    let (scheme, rest) = url.split_once("://")?;
    let proto = match scheme.to_ascii_lowercase().as_str() {
        "ftp" => QuickProto::Ftp,
        "ftps" | "ftpes" => QuickProto::FtpsExplicit,
        "ftpsi" => QuickProto::FtpsImplicit,
        "sftp" => QuickProto::Sftp,
        _ => return None,
    };
    let rest = rest.trim_end_matches('/');
    let (creds, hostport) = rest.rsplit_once('@').unwrap_or(("", rest));
    let (user, pass) = creds.split_once(':').unwrap_or((creds, ""));
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => (h, p.to_string()),
        _ => (hostport, proto.default_port().to_string()),
    };
    Some(QuickConnect { host: host.into(), port, user: user.into(), pass: pass.into(), proto })
}

/// Keyboard shortcuts shown by F1: (keys, description), as catalogue keys.
const SHORTCUTS: [(&str, &str); 26] = [
    ("shortcuts.k_tab", "shortcuts.tab"),
    ("shortcuts.k_escape", "shortcuts.escape"),
    ("shortcuts.k_f6", "shortcuts.f6"),
    ("shortcuts.k_arrows", "shortcuts.arrows"),
    ("shortcuts.k_shift_arrows", "shortcuts.shift_arrows"),
    ("shortcuts.k_ctrl_space", "shortcuts.ctrl_space"),
    ("shortcuts.k_ctrl_a", "shortcuts.ctrl_a"),
    ("shortcuts.k_enter", "shortcuts.enter"),
    ("shortcuts.k_backspace", "shortcuts.backspace"),
    ("shortcuts.k_space", "shortcuts.space"),
    ("shortcuts.k_f2", "shortcuts.f2"),
    ("shortcuts.k_f5", "shortcuts.f5"),
    ("shortcuts.k_f7", "shortcuts.f7"),
    ("shortcuts.k_delete", "shortcuts.delete"),
    ("shortcuts.k_ctrl_f", "shortcuts.ctrl_f"),
    ("shortcuts.k_ctrl_m", "shortcuts.ctrl_m"),
    ("shortcuts.k_plus_minus", "shortcuts.plus_minus"),
    ("shortcuts.k_ctrl_shift_c", "shortcuts.ctrl_shift_c"),
    ("shortcuts.k_ctrl_v", "shortcuts.ctrl_v"),
    ("shortcuts.k_f3_f4", "shortcuts.f3_f4"),
    ("shortcuts.k_shift_dblclick", "shortcuts.shift_dblclick"),
    ("shortcuts.k_shift_bookmark", "shortcuts.shift_bookmark"),
    ("shortcuts.k_ctrl_t", "shortcuts.ctrl_t"),
    ("shortcuts.k_ctrl_r", "shortcuts.ctrl_r"),
    ("shortcuts.k_ctrl_tab", "shortcuts.ctrl_tab"),
    ("shortcuts.k_f1", "shortcuts.f1"),
];

enum Action {
    /// Operation of the Edit / FTP menus on a panel (see [`fileops::Op`]).
    Op(usize, Op),
    /// "Send RAW command" dialog (`Ctrl+R`).
    RawDialog,
    /// "Manual download" window.
    ManualDownload,
    /// Predefined site command `n` of the FTP menu.
    SiteCommand(usize),
    Open(usize, String),
    Up(usize),
    Refresh(usize),
    Disconnect(usize),
    /// Connects a panel to a site manager record, with the login asked by the modifiers.
    ConnectSite(usize, u64, sites::LoginVariant),
    Reconnect(usize),
    /// Local folder ⇄ connection of a panel.
    SwitchSide(usize),
    /// Path typed in the field of a remote panel.
    RemoteCwd(usize, String),
    OpenSiteManager,
    Transfer(usize),
    StartQueue,
    ClearFinished,
    Focus(usize),
    /// Keyboard cursor of a panel: movement, extend the selection (Shift)?
    Cursor(usize, CursorMove, bool),
    ToggleCursor(usize),
    SelectAll(usize),
    Sort(usize, SortCol),
    NewFolder(usize),
    Rename(usize),
    Delete(usize),
    CopyPath(usize, String),
    CopyHttpUrl(usize, String),
    QueueStop(ItemId),
    QueueStopAll,
    QueueMove(ItemId, Move),
    QueueRemove(ItemId),
    QueueRetry(ItemId),
    QueueRetryAll,
    QueuePause,
    /// Click on a queue row: (item, `Ctrl`, `Shift`).
    QueueSelect(ItemId, bool, bool),
    QueueEdit(ItemId),
    QueueInvert(Vec<ItemId>),
    QueueMarkFailed(Vec<ItemId>),
    QueueMarkAllFailed,
    QueueClearFailed,
    /// Removes every item but the running ones.
    QueueClearAll,
    QueueInfo,
    QueueSaveFile,
    QueueLoadFile,
    /// Opens the conditional transfer dialog for the selection of a panel.
    Conditional(usize),
    /// SFV (`false`) or MD5 (`true`) file of the selection of a local panel.
    Checksums(usize, bool),
    /// Console: run the script of the tab / evaluate its expression.
    RunScript,
    Invoke,
}

impl UltraRushApp {
    pub fn new(rt: Handle, startup: StartupOptions) -> Self {
        let from_env = std::env::var("URUSHFTP_CONNECT").ok().and_then(|u| parse_connect_url(&u));
        let options = OptionsWindow::load();
        // Before anything is displayed or listed (dates and sizes follow the language).
        options::apply_language(options.settings.language);
        set_download_dir(&options.settings.download_dir);
        logs::configure(&options.settings.interface);
        let dl = options.settings.transfer.download_limit_kib * 1024;
        let ul = options.settings.transfer.upload_limit_kib * 1024;
        let mut app = Self {
            rt,
            saved_queue: None,
            queue_written: None,
            queue_checked: std::time::Instant::now(),
            ipc_compat_applied: None,
            tabs: vec![Container::new(1)],
            active: 0,
            next_id: 2,
            autoconnect: from_env.is_some(),
            quick: from_env.unwrap_or(QuickConnect {
                host: String::new(),
                port: "21".into(),
                user: String::new(),
                pass: String::new(),
                proto: QuickProto::Ftp,
            }),
            dark: true,
            sites: SiteManager::load(),
            options,
            download_limit: RateLimiter::new(dl),
            upload_limit: RateLimiter::new(ul),
            ident: None,
            ident_applied: None,
            stats: rushapp::Stats::load(),
            splitter: ";".into(),
            startup_script_done: false,
            ipc: None,
            ipc_rx: None,
            ipc_callback: None,
            raw_callbacks: Vec::new(),
            exit_when_done: false,
            startup: Some(startup),
            // Development (screenshots): `URUSHFTP_OPEN_SHORTCUTS=1` opens the shortcuts window.
            shortcuts_open: std::env::var_os("URUSHFTP_OPEN_SHORTCUTS").is_some(),
            about_open: std::env::var_os("URUSHFTP_OPEN_ABOUT").is_some(),
            queue_end: None,
            power: run_power_action,
            ipc_enabled: true,
            paste_down: false,
            logs_flushed: std::time::Instant::now(),
            raw: raw::RawDialog::default(),
            manual: favorites::ManualDownload::default(),
            clipboard_seq: None,
        };
        // The URUSHFTP_THEME variable wins over the stored option.
        app.dark = if std::env::var_os("URUSHFTP_THEME").is_some() { theme::start_dark() } else { app.options.settings.dark_theme };
        app.load_saved_queue();
        app
    }

    /// Queue left unfinished: loaded, offered or discarded according to the options.
    pub(crate) fn load_saved_queue(&mut self) {
        use urftp_core::queuestore::SavedQueues;
        use urftp_core::settings::QueueAtStart;
        let path = SavedQueues::default_path();
        let saved = match SavedQueues::load(&path) {
            Ok(Some(q)) => q,
            Ok(None) => return,
            Err(e) => {
                self.tabs[self.active].push_transfer(0, LogKind::Error, t!("queue.unreadable", error = e));
                return;
            }
        };
        match self.options.settings.queue_at_start {
            QueueAtStart::Ask => {
                let ticks = vec![true; saved.tabs.len()];
                self.saved_queue = Some((saved, ticks));
            }
            QueueAtStart::Load => self.restore_queue(saved),
            QueueAtStart::Discard => {
                let _ = SavedQueues::default().save(&path);
            }
        }
    }

    /// Puts the saved queues back: the first one in the first tab when its queue is empty,
    /// the others in new tabs.
    fn restore_queue(&mut self, saved: urftp_core::queuestore::SavedQueues) {
        let n = saved.item_count();
        for (i, tab) in saved.tabs.into_iter().enumerate() {
            let target = if i == 0 && self.tabs[0].queue.lock().items().is_empty() {
                0
            } else {
                self.new_tab();
                self.tabs.len() - 1
            };
            *self.tabs[target].queue.lock() = tab.into_queue();
        }
        self.active = 0;
        self.tabs[0].push_transfer(0, LogKind::Status, t!("queue.restored", n = n));
        self.save_queue();
    }

    /// Writes the unfinished queues to disk when they changed (not while the saved queue
    /// waits for an answer: it would be overwritten).
    pub(crate) fn save_queue(&mut self) {
        use urftp_core::queuestore::SavedQueues;
        if self.saved_queue.is_some() {
            return;
        }
        let queues: Vec<(String, Queue)> = self.tabs.iter().enumerate().map(|(i, t)| (t.title(i + 1), t.queue.lock().clone())).collect();
        let now = SavedQueues::snapshot(queues.iter().map(|(title, q)| (title.clone(), q)));
        if self.queue_written.as_ref() == Some(&now) {
            return;
        }
        if now.is_empty() && self.queue_written.is_none() && !SavedQueues::default_path().exists() {
            self.queue_written = Some(now);
            return;
        }
        match now.save(&SavedQueues::default_path()) {
            Ok(()) => self.queue_written = Some(now),
            Err(e) => self.tabs[self.active].push_transfer(0, LogKind::Error, t!("queue.save_failed", error = e)),
        }
    }

    /// "Unfinished queue" dialog at startup: one line per tab, each one ticked to be loaded.
    fn saved_queue_dialog(&mut self, ctx: &egui::Context) {
        let Some((saved, ticks)) = &mut self.saved_queue else { return };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("saved-queue")).show(ctx, |ui| {
            ui.set_max_width(560.0);
            ui.heading(t!("queue.saved_title"));
            ui.label(t!("queue.saved_text", n = saved.item_count(), tabs = saved.tabs.len()));
            ui.add_space(6.0);
            for (i, (tab, tick)) in saved.tabs.iter().zip(ticks.iter_mut()).enumerate() {
                ui.push_id(("saved-tab", i), |ui| {
                    let failed = tab.items.len() - tab.waiting();
                    let label = if failed > 0 {
                        t!("queue.saved_tab_failed", title = tab.title, n = tab.waiting(), failed = failed)
                    } else {
                        t!("queue.saved_tab", title = tab.title, n = tab.waiting())
                    };
                    ui.checkbox(tick, label);
                    let first = tab.items.first().map(|i| i.src_path.as_str()).unwrap_or_default();
                    ui.indent("detail", |ui| {
                        ui.label(RichText::new(t!("queue.saved_detail", sites = tab.sites().join(", "), first = first)).weak());
                    });
                });
            }
            ui.add_space(8.0);
            let any = ticks.iter().any(|t| *t);
            ui.horizontal(|ui| {
                let load = ui.add_enabled(any, egui::Button::new(t!("queue.load_selected")));
                if any && !ui.ctx().memory(|m| m.focused().is_some()) {
                    load.request_focus();
                }
                if load.clicked() {
                    answer = Some(true);
                }
                if ui.button(t!("queue.discard_all")).clicked() {
                    answer = Some(false);
                }
            });
            ui.label(RichText::new(t!("queue.saved_note")).weak());
        });
        match answer {
            Some(true) => {
                if let Some((mut saved, ticks)) = self.saved_queue.take() {
                    let mut keep = ticks.into_iter();
                    saved.tabs.retain(|_| keep.next().unwrap_or(false));
                    self.restore_queue(saved);
                }
            }
            Some(false) => {
                self.saved_queue = None;
                self.queue_written = None;
                self.save_queue();
            }
            None => {}
        }
    }

    /// Window class of the mIRC link: restarts the window when the option changed.
    fn sync_ipc_class(&mut self) {
        let wanted = self.options.settings.ftprush_dll_window;
        if self.ipc_compat_applied == Some(wanted) {
            return;
        }
        let first = self.ipc_compat_applied.is_none();
        self.ipc_compat_applied = Some(wanted);
        urftp_sys::set_ftprush_compatible(wanted);
        if !first {
            self.ipc = None;
            self.ipc_rx = None;
        }
    }

    /// Theme to apply at startup.
    pub fn is_dark(&self) -> bool {
        self.dark
    }

    /// Queue engine options, from the current settings.
    fn engine_options(&self) -> EngineOptions {
        options::engine_options(&self.options.settings, &self.download_limit, &self.upload_limit)
    }

    /// Applies changed options: language, theme, rates (even during transfers), Ident.
    fn apply_settings(&mut self, ctx: &egui::Context) {
        let s = &self.options.settings;
        set_download_dir(&s.download_dir);
        logs::configure(&s.interface);
        let before = urftp_i18n::language();
        options::apply_language(s.language);
        if urftp_i18n::language() != before {
            // Listings hold formatted dates: list them again in the new language.
            for side in self.tabs.iter_mut().flat_map(|t| t.sides.iter_mut()) {
                match side {
                    Side::Local(l) => l.reload(),
                    Side::Remote(r) => r.handle.send(BrowserCmd::Refresh),
                }
            }
        }
        let s = &self.options.settings;
        if s.dark_theme != self.dark {
            self.dark = s.dark_theme;
            theme::apply(ctx, self.dark);
        }
        self.download_limit.set_rate(s.transfer.download_limit_kib * 1024);
        self.upload_limit.set_rate(s.transfer.upload_limit_kib * 1024);
        self.sync_ident();
    }

    /// (Re)starts or stops the Ident server when its settings changed.
    fn sync_ident(&mut self) {
        let wanted = self.options.settings.ident.clone();
        if self.ident_applied.as_ref() == Some(&wanted) {
            return;
        }
        self.ident = None;
        self.ident_applied = Some(wanted.clone());
        if !wanted.enabled {
            self.options.ident_status = Some((t!("ident.stopped"), true));
            return;
        }
        let cfg = IdentConfig {
            name: if wanted.name.trim().is_empty() { urftp_core::DEFAULT_IDENT_NAME.into() } else { wanted.name.trim().into() },
            system: if wanted.system.trim().is_empty() { "UNIX".into() } else { wanted.system.trim().into() },
            bind: std::net::SocketAddr::from(([0, 0, 0, 0], wanted.port)),
            only_registered: wanted.only_while_connecting,
        };
        // The interface does not run inside the runtime: it may block on the startup here.
        match self.rt.block_on(IdentServer::start(cfg)) {
            Ok(srv) => {
                self.options.ident_status = Some((t!("ident.running", port = srv.local_addr().port()), true));
                self.ident = Some(srv);
            }
            Err(e) => self.options.ident_status = Some((t!("ident.bind_failed", port = wanted.port, error = e), false)),
        }
    }

    fn new_tab(&mut self) {
        self.tabs.push(Container::new(self.next_id));
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
    }

    /// Quick-connection profile: a site record without name, with the global options.
    fn quick_profile(&self) -> Option<SiteProfile> {
        self.quick_site().map(|site| self.site_profile(&site))
    }

    /// Connects the right panel of the active tab (like the v2 quick-connect bar).
    fn quick_connect(&mut self, ctx: &egui::Context) {
        let Some(profile) = self.quick_profile() else { return };
        self.connect_profile(profile, None, None, ctx);
    }

    /// Opens a connection in the right panel of the active tab (a new tab when that panel
    /// is already connected), then changes to `start_dir`.
    fn connect_profile(&mut self, profile: SiteProfile, site_id: Option<u64>, start_dir: Option<String>, ctx: &egui::Context) {
        if self.options.settings.new_tab_when_busy && matches!(self.tabs[self.active].sides[1], Side::Remote(_)) {
            self.new_tab();
        }
        self.open_remote(self.active, 1, profile, site_id, start_dir, ctx);
    }

    /// Connects panel `side` of tab `tab` (the previous connection is closed).
    fn open_remote(
        &mut self,
        tab: usize,
        side: usize,
        profile: SiteProfile,
        site_id: Option<u64>,
        start_dir: Option<String>,
        ctx: &egui::Context,
    ) {
        match (&self.tabs[tab].sides[side], &self.tabs[tab].parked[side]) {
            (Side::Remote(r), _) | (Side::Local(_), Some(Side::Remote(r))) => r.handle.send(BrowserCmd::Disconnect),
            _ => {}
        }
        let http = site_id.and_then(|id| self.sites.book.find(id)).map(|s| s.http.clone()).unwrap_or_default();
        let handle = browser::spawn(&self.rt, ctx.clone());
        handle.send(BrowserCmd::Connect(Box::new(profile.clone())));
        if let Some(dir) = start_dir.filter(|d| !d.trim().is_empty()) {
            handle.send(BrowserCmd::Cwd(dir));
        }
        let t = &mut self.tabs[tab];
        t.push_ftp(side, LogKind::Status, t!("log.connecting", site = profile.name));
        let shown = std::mem::replace(&mut t.sides[side], Side::Local(LocalView::new(PathBuf::new())));
        // The local folder stays behind the connection (switch button of the panel).
        t.parked[side] = match shown {
            Side::Local(l) => Some(Side::Local(l)),
            Side::Remote(_) => t.parked[side].take().filter(|p| matches!(p, Side::Local(_))),
        };
        t.sides[side] = Side::Remote(Box::new(RemoteView {
            profile,
            handle,
            dir: "/".into(),
            path_edit: "/".into(),
            rows: Vec::new(),
            list: Box::default(),
            selected: BTreeSet::new(),
            cursor: Cursor::default(),
            state: RemoteState::Connecting,
            site_id,
            sort: Sort::default(),
            http,
        }));
    }

    /// Connection from a site manager record.
    /// Connection profile of a site record, with its own Ident name when the Ident server
    /// runs.
    pub(crate) fn site_profile(&self, site: &Site) -> SiteProfile {
        let mut p = sites::to_profile(site, &self.options.settings);
        let Some(srv) = &self.ident else { return p };
        let own = match &site.ident {
            urftp_core::site::IdentChoice::Custom(name) if !name.trim().is_empty() => Some(name.trim().to_string()),
            _ => None,
        };
        // "Only while connecting": every connection registers itself, with the global name
        // when the site has none of its own.
        let ident = &self.options.settings.ident;
        let global = if ident.name.trim().is_empty() { urftp_core::DEFAULT_IDENT_NAME.to_string() } else { ident.name.trim().to_string() };
        let name = own.or_else(|| ident.only_while_connecting.then_some(global));
        if let Some(name) = name {
            p.ident = Some(urftp_proto::session::SiteIdent { registrar: srv.registrar(), name });
        }
        p
    }

    /// Availability tests asked in the site manager: every site in parallel.
    fn run_probes(&mut self, ctx: &egui::Context) {
        for site in self.sites.take_probe_requests() {
            let profile = self.site_profile(&site);
            let probes = self.sites.probes.clone();
            let ctx = ctx.clone();
            self.rt.spawn(async move {
                let result = match urftp_proto::session::RemoteSession::probe(&profile).await {
                    Ok(((host, port), took)) => sites::Probe::Up { address: format!("{host}:{port}"), millis: took.as_millis() },
                    Err(e) => sites::Probe::Down(e.to_string()),
                };
                probes.lock().insert(site.id, result);
                ctx.request_repaint();
            });
        }
    }

    fn connect_site(&mut self, site: Site, variant: sites::LoginVariant, ctx: &egui::Context) {
        self.note_recent(site.id, ctx);
        let profile = self.site_profile(&site).with_login_variant(variant.ghost, variant.quiet);
        let remote = (!site.remote_dir.is_empty()).then(|| site.remote_dir.clone());
        self.connect_profile(profile, Some(site.id), remote, ctx);
        if !site.local_dir.trim().is_empty()
            && let Side::Local(l) = &mut self.tabs[self.active].sides[0]
        {
            l.path = PathBuf::from(site.local_dir.trim());
            l.reload();
        }
    }

    /// End of the queues: once every tab is idle with nothing left to do, runs the action
    /// of the options (logging out at once, the others after a countdown).
    fn check_queue_end(&mut self, ctx: &egui::Context) {
        let finished: Vec<usize> = (0..self.tabs.len()).filter(|&i| std::mem::take(&mut self.tabs[i].just_finished)).collect();
        // Requests of `RushApp.FTP.Transfer` (RS_LOGOUT, RS_DELEMPTYTABS), last tab first.
        for &i in finished.iter().rev() {
            let t = &mut self.tabs[i];
            if t.queue.lock().has_pending() {
                continue;
            }
            if std::mem::take(&mut t.logout_when_done) {
                t.disconnect_side(0);
                t.disconnect_side(1);
            }
            let local = t.sides.iter().chain(t.parked.iter().flatten()).all(|s| matches!(s, Side::Local(_)));
            if std::mem::take(&mut t.close_when_done) && local && self.tabs.len() > 1 {
                self.tabs.remove(i);
                self.active = self.active.min(self.tabs.len() - 1);
            }
        }
        let finished: Vec<usize> = finished.into_iter().filter(|&i| i < self.tabs.len()).collect();
        let action = self.options.settings.transfer.on_queue_end;
        let idle = self.tabs.iter().all(|t| t.engine_events.is_none() && !t.queue.lock().has_pending());
        if !finished.is_empty() && action == QueueEnd::Disconnect {
            for &i in &finished {
                if !self.tabs[i].queue.lock().has_pending() {
                    self.tabs[i].disconnect_side(0);
                    self.tabs[i].disconnect_side(1);
                }
            }
        } else if !finished.is_empty() && idle && action.needs_countdown() && self.queue_end.is_none() {
            let secs = u64::from(self.options.settings.transfer.queue_end_countdown_secs);
            self.queue_end = Some((action, std::time::Instant::now() + Duration::from_secs(secs)));
        }
        if let Some((action, when)) = self.queue_end {
            if std::time::Instant::now() >= when {
                self.queue_end = None;
                self.perform_queue_end(action, ctx);
            } else {
                ctx.request_repaint_after(Duration::from_millis(250));
            }
        }
    }

    /// Runs an end-of-queue action that needed a countdown.
    fn perform_queue_end(&mut self, action: QueueEnd, ctx: &egui::Context) {
        let a = self.active;
        self.tabs[a].push_transfer(0, LogKind::Status, t!("log.queue_end_run", action = queue_end_label(action)));
        if action == QueueEnd::Exit {
            self.stats.save();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if let Err(e) = (self.power)(action) {
            self.tabs[a].push_transfer(0, LogKind::Error, t!("log.queue_end_failed", action = queue_end_label(action), error = e));
        }
    }

    /// Countdown before the end-of-queue action: run it now or cancel it.
    fn queue_end_dialog(&mut self, ctx: &egui::Context) {
        let Some((action, when)) = self.queue_end else { return };
        let left = when.saturating_duration_since(std::time::Instant::now()).as_secs() + 1;
        let mut choice: Option<bool> = None;
        egui::Modal::new(egui::Id::new("queue-end")).show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.heading(t!("queue_end.title"));
            ui.label(t!("queue_end.countdown", action = queue_end_label(action), n = left));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let cancel = ui.button(t!("common.cancel"));
                // The keyboard starts on Cancel: the action may stop the computer.
                focus_once(ui, &cancel);
                if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    choice = Some(false);
                }
                if theme::danger_button(ui, t!("queue_end.now")).clicked() {
                    choice = Some(true);
                }
            });
        });
        match choice {
            Some(true) => {
                self.queue_end = None;
                self.perform_queue_end(action, ctx);
            }
            Some(false) => {
                self.queue_end = None;
                let a = self.active;
                self.tabs[a].push_transfer(0, LogKind::Status, t!("log.queue_end_cancelled", action = queue_end_label(action)));
            }
            None => {}
        }
    }

    /// Writes the unfinished items of the active tab to a queue file ("Save queue").
    pub(crate) fn save_queue_file(&mut self, path: &Path) {
        let a = self.active;
        let tab = urftp_core::queuestore::SavedTab::of_queue(self.tabs[a].title(a + 1), &self.tabs[a].queue.lock());
        let msg = match tab.save_file(path) {
            Ok(()) => (LogKind::Status, t!("log.queue_saved", n = tab.items.len(), path = path.display())),
            Err(e) => (LogKind::Error, t!("log.queue_save_failed", path = path.display(), error = e)),
        };
        self.tabs[a].push_transfer(0, msg.0, msg.1);
    }

    /// Adds the items of a queue file to the queue of the active tab ("Load queue").
    pub(crate) fn load_queue_file(&mut self, path: &Path) {
        let a = self.active;
        let msg = match urftp_core::queuestore::SavedTab::load_file(path) {
            Ok(saved) => {
                let n = saved.items.len();
                let loaded = saved.into_queue();
                self.tabs[a]
                    .queue
                    .lock()
                    .append(loaded.items().iter().map(|i| (i.src.clone(), i.dst.clone(), i.is_dir, i.size, i.priority, i.state.clone())));
                (LogKind::Status, t!("log.queue_loaded", n = n, path = path.display()))
            }
            Err(e) => (LogKind::Error, t!("log.queue_load_failed", path = path.display(), error = e)),
        };
        self.tabs[a].push_transfer(0, msg.0, msg.1);
    }

    /// "Edit queue" dialog: source and target of an item, FXP inverted by a button.
    fn queue_edit_dialog(&mut self, ctx: &egui::Context) {
        let mut names: Vec<String> = self.sites.book.all_sites().into_iter().map(|(_, s)| s.name.clone()).collect();
        let tab = &mut self.tabs[self.active];
        let Some(form) = &mut tab.queue_edit else { return };
        for n in [&form.src_site, &form.dst_site] {
            if !n.is_empty() && !names.contains(n) {
                names.push(n.clone());
            }
        }
        let mut done: Option<bool> = None;
        egui::Modal::new(egui::Id::new(("queue-edit", tab.id))).show(ctx, |ui| {
            ui.set_min_width(520.0);
            ui.heading(t!("queue.edit_title"));
            ui.add_space(4.0);
            egui::Grid::new("queue-edit-grid").num_columns(3).spacing([10.0, 8.0]).show(ui, |ui| {
                for (label, site, path, salt) in [
                    (t!("queue.edit_source"), &mut form.src_site, &mut form.src_path, "src"),
                    (t!("queue.edit_target"), &mut form.dst_site, &mut form.dst_path, "dst"),
                ] {
                    ui.label(&label);
                    let shown = if site.is_empty() { t!("queue.edit_local") } else { site.clone() };
                    let combo =
                        egui::ComboBox::from_id_salt(("queue-edit-site", salt)).selected_text(shown).width(170.0).show_ui(ui, |ui| {
                            ui.selectable_value(site, String::new(), t!("queue.edit_local"));
                            for n in &names {
                                ui.selectable_value(site, n.clone(), n);
                            }
                        });
                    theme::name(&combo.response, t!("queue.edit_site_of", side = &label));
                    let r = ui.add(theme::single_line(path).desired_width(300.0));
                    theme::name(&r, t!("queue.edit_path_of", side = &label));
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                let l = ui.label(t!("queue.edit_mode"));
                let combo = egui::ComboBox::from_id_salt("queue-edit-mode")
                    .selected_text(options::mode_label(form.mode))
                    .width(170.0)
                    .show_ui(ui, |ui| {
                        for m in [urftp_core::TransferMode::Auto, urftp_core::TransferMode::Binary, urftp_core::TransferMode::Ascii] {
                            ui.selectable_value(&mut form.mode, m, options::mode_label(m));
                        }
                    });
                combo.response.labelled_by(l.id);
            });
            if form.error {
                ui.colored_label(theme::colors(ui).error, t!("queue.edit_local_to_local"));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, t!("common.validate")).clicked() {
                    done = Some(true);
                }
                if ui.button(t!("queue.invert")).on_hover_text(t!("queue.invert_hint")).clicked() {
                    std::mem::swap(&mut form.src_site, &mut form.dst_site);
                    std::mem::swap(&mut form.src_path, &mut form.dst_path);
                }
                if ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) if form.src_site.is_empty() && form.dst_site.is_empty() => form.error = true,
            Some(true) => {
                let src = QueueEdit::endpoint(&form.src_site, &form.src_path);
                let dst = QueueEdit::endpoint(&form.dst_site, &form.dst_path);
                let (id, mode) = (form.id, form.mode);
                let mut q = tab.queue.lock();
                q.edit(id, src, dst);
                q.set_mode(id, mode);
                drop(q);
                tab.queue_edit = None;
            }
            Some(false) => tab.queue_edit = None,
            None => {}
        }
    }

    /// "Conditional transfer" dialog: filters, dates, sizes, synchronisation, schemes.
    fn conditional_dialog(&mut self, ctx: &egui::Context) {
        let a = self.active;
        let rt = self.rt.clone();
        let opts = self.engine_options();
        let extra = self.queue_site_profiles(a);
        let schemes = &mut self.options.settings.condition_schemes;
        let tab = &mut self.tabs[a];
        let Some(dlg) = &mut tab.condition else { return };
        // None: open; Some(None): cancel; Some(Some(start)): queue, then start or not.
        let mut done: Option<Option<bool>> = None;
        let mut schemes_changed = false;
        egui::Modal::new(egui::Id::new(("conditional", tab.id))).show(ctx, |ui| {
            ui.set_min_width(560.0);
            ui.heading(t!("cond.title"));
            let f = &mut dlg.form;
            ui.horizontal(|ui| {
                let l = ui.label(t!("cond.scheme"));
                let combo = egui::ComboBox::from_id_salt("cond-scheme").selected_text(f.name.clone()).width(180.0).show_ui(ui, |ui| {
                    for sc in schemes.iter() {
                        if ui.selectable_label(sc.name == f.name, &sc.name).clicked() {
                            *f = sc.clone();
                        }
                    }
                });
                combo.response.labelled_by(l.id);
                let n = ui.add(theme::single_line(&mut f.name).hint_text(t!("cond.scheme_name")).desired_width(150.0));
                theme::name(&n, t!("cond.scheme_name"));
                if ui.add_enabled(!f.name.trim().is_empty(), egui::Button::new(t!("cond.save"))).clicked() {
                    f.name = f.name.trim().to_string();
                    match schemes.iter_mut().find(|sc| sc.name == f.name) {
                        Some(sc) => *sc = f.clone(),
                        None => schemes.push(f.clone()),
                    }
                    schemes_changed = true;
                }
                if ui.add_enabled(schemes.iter().any(|sc| sc.name == f.name), egui::Button::new(t!("cond.delete"))).clicked() {
                    schemes.retain(|sc| sc.name != f.name);
                    schemes_changed = true;
                }
            });
            ui.separator();
            egui::Grid::new("cond-names").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("");
                ui.label(RichText::new(t!("cond.include")).strong());
                ui.label(RichText::new(t!("cond.exclude")).strong());
                ui.end_row();
                for (label, inc, exc) in [
                    (t!("cond.folders"), &mut f.folder_include, &mut f.folder_exclude),
                    (t!("cond.files"), &mut f.file_include, &mut f.file_exclude),
                ] {
                    ui.label(&label);
                    let i = ui.add(theme::single_line(inc).desired_width(220.0));
                    theme::name(&i, format!("{label} — {}", t!("cond.include")));
                    let e = ui.add(theme::single_line(exc).desired_width(220.0));
                    theme::name(&e, format!("{label} — {}", t!("cond.exclude")));
                    ui.end_row();
                }
            });
            ui.label(RichText::new(t!("cond.masks_note")).weak());
            ui.checkbox(&mut f.regex, t!("cond.regex"));
            ui.add_space(4.0);
            ui.label(RichText::new(t!("cond.advanced")).strong());
            ui.horizontal(|ui| {
                ui.checkbox(&mut f.date_between, t!("cond.date_between"));
                let from = ui
                    .add_enabled(f.date_between, theme::single_line(&mut f.date_from).hint_text(t!("cond.date_hint")).desired_width(100.0));
                theme::name(&from, t!("cond.date_from"));
                let to =
                    ui.add_enabled(f.date_between, theme::single_line(&mut f.date_to).hint_text(t!("cond.date_hint")).desired_width(100.0));
                theme::name(&to, t!("cond.date_to"));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut f.not_older, t!("cond.not_older"));
                let d = ui.add_enabled(f.not_older, egui::DragValue::new(&mut f.not_older_days).range(0..=36500));
                theme::name(&d, t!("cond.not_older"));
                ui.label(t!("cond.days"));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut f.size_range, t!("cond.size"));
                let min = ui.add_enabled(f.size_range, egui::DragValue::new(&mut f.min_kib).speed(16));
                theme::name(&min, t!("cond.size_min"));
                let max = ui.add_enabled(f.size_range, egui::DragValue::new(&mut f.max_kib).speed(16));
                theme::name(&max, t!("cond.size_max"));
                ui.label(t!("cond.kib"));
            });
            ui.checkbox(&mut f.dates_on_folders, t!("cond.dates_on_folders"));
            ui.checkbox(&mut f.no_skiplist, t!("cond.no_skiplist"));
            ui.checkbox(&mut f.sub_dirs, t!("cond.sub_dirs"));
            ui.checkbox(&mut f.disconnect_after, t!("cond.disconnect_after"));
            ui.add_space(4.0);
            ui.checkbox(&mut f.sync, RichText::new(t!("cond.sync")).strong());
            ui.add_enabled_ui(f.sync, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut f.delete_missing, t!("cond.delete_missing"));
                    ui.checkbox(&mut f.compare_date, t!("cond.compare_date"));
                    ui.checkbox(&mut f.compare_size, t!("cond.compare_size"));
                    ui.checkbox(&mut f.existing_only, t!("cond.existing_only"));
                    ui.checkbox(&mut f.both_sides, t!("cond.both_sides"));
                });
            });
            if let Some(e) = &dlg.error {
                ui.colored_label(theme::colors(ui).error, e);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, t!("cond.begin")).clicked() {
                    done = Some(Some(true));
                }
                if ui.button(t!("cond.add")).clicked() {
                    done = Some(Some(false));
                }
                if ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(None);
                }
            });
        });
        match done {
            Some(Some(start)) => match dlg.form.to_options(FtpTime::now_epoch()) {
                Err(e) => dlg.error = Some(e.to_string()),
                Ok(options) => {
                    let (side, logout) = (dlg.side, dlg.form.disconnect_after);
                    tab.condition = None;
                    let options = (!options.is_default()).then(|| Arc::new(options));
                    if tab.enqueue_selection_with(side, options) > 0 {
                        tab.logout_when_done |= logout;
                        if start {
                            tab.start_queue(&rt, ctx, opts, &extra);
                        }
                    }
                }
            },
            Some(None) => tab.condition = None,
            None => {}
        }
        if schemes_changed {
            self.options.touch(ctx);
        }
    }

    /// "Queue information" window: counts, sizes, time left at the current speed.
    fn queue_info_window(&mut self, ctx: &egui::Context) {
        let (down, up) = self.stats.bandwidth();
        let rate = ((down + up) * 1024.0) as u64;
        let tab = &mut self.tabs[self.active];
        if !tab.queue_info {
            return;
        }
        let st = tab.queue.lock().stats();
        let mut open = true;
        egui::Window::new(t!("queue.info_title"))
            .id(egui::Id::new(("queue-info", tab.id)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("queue-info-grid").num_columns(2).spacing([24.0, 4.0]).striped(true).show(ui, |ui| {
                    let rows = [
                        (t!("queue.info_items"), t!("queue.info_items_value", files = st.files, folders = st.folders)),
                        (t!("queue.info_kinds"), t!("queue.info_kinds_value", down = st.downloads, up = st.uploads, fxp = st.fxp)),
                        (t!("state.waiting"), st.waiting.to_string()),
                        (t!("state.running"), st.running.to_string()),
                        (t!("state.done"), st.done.to_string()),
                        (t!("queue.info_skipped"), st.skipped.to_string()),
                        (t!("queue.info_failed"), st.failed.to_string()),
                        (t!("state.stopped"), st.stopped.to_string()),
                        (t!("state.confirm"), st.to_confirm.to_string()),
                        (t!("queue.info_total"), human_size(st.total_bytes)),
                        (t!("queue.info_done"), human_size(st.done_bytes)),
                        (
                            t!("queue.info_left"),
                            if st.unknown_sizes > 0 {
                                t!("queue.info_left_unknown", size = human_size(st.left_bytes), n = st.unknown_sizes)
                            } else {
                                human_size(st.left_bytes)
                            },
                        ),
                        (
                            t!("queue.info_eta"),
                            st.eta(rate).map_or_else(
                                || t!("queue.info_eta_none"),
                                |d| {
                                    let s = d.as_secs();
                                    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
                                },
                            ),
                        ),
                    ];
                    for (k, v) in rows {
                        ui.label(k);
                        ui.label(RichText::new(v).strong());
                        ui.end_row();
                    }
                });
            });
        if !open {
            tab.queue_info = false;
        }
    }

    /// Connects panel `side` of the active tab to a site record (connection bar and menu of
    /// the panel); the local folder of the site goes to the other panel when it is local.
    fn connect_site_in(&mut self, site: Site, variant: sites::LoginVariant, side: usize, ctx: &egui::Context) {
        self.note_recent(site.id, ctx);
        let profile = self.site_profile(&site).with_login_variant(variant.ghost, variant.quiet);
        let remote = (!site.remote_dir.is_empty()).then(|| site.remote_dir.clone());
        let tab = self.active;
        self.open_remote(tab, side, profile, Some(site.id), remote, ctx);
        if !site.local_dir.trim().is_empty()
            && let Side::Local(l) = &mut self.tabs[tab].sides[1 - side]
        {
            l.path = PathBuf::from(site.local_dir.trim());
            l.reload();
        }
    }

    /// Command-line options (`-w`, `-c`, `-s`, `-r[t][x]`, `-d`).
    fn apply_startup(&mut self, o: StartupOptions, ctx: &egui::Context) {
        let mut notes: Vec<(bool, String)> = Vec::new();
        if o.debug {
            notes.push((true, t!("startup.debug")));
        }
        for a in &o.unknown {
            notes.push((false, t!("startup.unknown_option", option = a)));
        }
        if let Some(n) = o.windows {
            while self.tabs.len() < n {
                self.new_tab();
            }
            self.active = 0;
        }
        if let Some(target) = &o.connect {
            // URL, name of a site manager site, or host only (anonymous FTP).
            if let Some(q) = parse_connect_url(target) {
                self.quick = q;
                self.quick_connect(ctx);
            } else if let Some(site) = self.sites.book.find_by_name(target).cloned() {
                self.connect_site(site, sites::LoginVariant::default(), ctx);
            } else if let Some(q) = parse_connect_url(&format!("ftp://{target}")) {
                self.quick = q;
                self.quick_connect(ctx);
            }
        }
        if let Some(path) = &o.script_file {
            match std::fs::read(path) {
                Ok(bytes) => {
                    let src = String::from_utf8(bytes).unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect());
                    notes.push(match self.run_script(&src, ctx) {
                        Ok(()) => (true, t!("startup.script_done", path = path.display())),
                        Err(e) => (false, t!("startup.script_failed", path = path.display(), error = e)),
                    });
                }
                Err(e) => notes.push((false, t!("startup.script_unreadable", path = path.display(), error = e))),
            }
        }
        if o.run_queues {
            let rt = self.rt.clone();
            let opts = self.engine_options();
            let reset = self.options.settings.transfer.reset_failed_on_auto_start;
            for t in 0..self.tabs.len() {
                if reset {
                    self.tabs[t].queue.lock().retry_failed_only();
                }
                if self.tabs[t].queue.lock().has_pending() {
                    let extra = self.queue_site_profiles(t);
                    self.tabs[t].start_queue(&rt, ctx, opts.clone(), &extra);
                }
            }
        }
        if o.minimize {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        self.exit_when_done = o.exit_when_done;
        // Visible when the program is started from a console (debug builds).
        for (_, n) in &notes {
            eprintln!("{n}");
        }
        let t = self.active;
        self.tabs[t].script_out.extend(notes);
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        let entries = self.site_entries();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t!("menu.file"), |ui| {
                if ui.button(t!("menu.site_manager")).clicked() {
                    self.sites.open = true;
                    ui.close();
                }
                if ui.button(t!("menu.new_tab")).clicked() {
                    self.new_tab();
                    ui.close();
                }
                if ui.button(t!("fav.manual")).clicked() {
                    actions.push(Action::ManualDownload);
                    ui.close();
                }
                if ui.button(t!("menu.options")).clicked() {
                    self.options.open = true;
                    ui.close();
                }
                if ui.button(t!("menu.quit")).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            // Acts on the panel that has the keyboard, like the bar of that panel.
            ui.menu_button(t!("menu.connection"), |ui| {
                let tab = &self.tabs[self.active];
                let side = tab.focus;
                connection_menu(ui, &entries, side, &mut actions);
                let remote = matches!(tab.sides[side], Side::Remote(_));
                if ui.add_enabled(remote, egui::Button::new(t!("conn.reconnect"))).clicked() {
                    actions.push(Action::Reconnect(side));
                    ui.close();
                }
                let connected = remote || matches!(tab.parked[side], Some(Side::Remote(_)));
                if ui.add_enabled(connected, egui::Button::new(t!("files.disconnect"))).clicked() {
                    actions.push(Action::Disconnect(side));
                    ui.close();
                }
                let label = tab.switch_label(side);
                if ui.add_enabled(label.is_some(), egui::Button::new(label.unwrap_or_else(|| t!("conn.to_local")))).clicked() {
                    actions.push(Action::SwitchSide(side));
                    ui.close();
                }
            });
            ui.menu_button(t!("menu.edit"), |ui| fileops::edit_menu(ui, &self.tabs[self.active], &mut actions));
            let commands = self.options.settings.site_commands.clone();
            ui.menu_button(t!("menu.ftp"), |ui| {
                fileops::ftp_menu(ui, &mut self.tabs[self.active], &mut actions);
                ui.separator();
                raw::commands_menu(ui, &commands, &mut actions);
            });
            ui.menu_button(t!("menu.view"), |ui| {
                if ui.button(if self.dark { t!("menu.light_theme") } else { t!("menu.dark_theme") }).clicked() {
                    self.dark = !self.dark;
                    theme::apply(ui.ctx(), self.dark);
                    self.options.settings.dark_theme = self.dark;
                    self.options.touch(ui.ctx());
                    ui.close();
                }
            });
            ui.menu_button(t!("menu.help"), |ui| {
                if ui.button(t!("menu.shortcuts")).clicked() {
                    self.shortcuts_open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button(t!("menu.about")).clicked() {
                    self.about_open = true;
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("UltraRushFTP").strong().color(theme::colors(ui).accent_text));
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(t!("quick.sites")).on_hover_text(t!("quick.sites_hint")).clicked() {
                self.sites.open = !self.sites.open;
            }
            if theme::icon_button(ui, "⚙", t!("quick.options_hint")).clicked() {
                self.options.open = !self.options.open;
            }
            ui.separator();
            let mut fav = Vec::new();
            self.favorites_menu(ui, &mut fav);
            actions.append(&mut fav);
            ui.separator();
            ui.label("⚡");
            let before = self.quick.proto;
            let combo = egui::ComboBox::from_id_salt("proto").selected_text(self.quick.proto.label()).width(120.0).show_ui(ui, |ui| {
                for p in [QuickProto::Ftp, QuickProto::FtpsExplicit, QuickProto::FtpsImplicit, QuickProto::Sftp] {
                    ui.selectable_value(&mut self.quick.proto, p, p.label());
                }
            });
            theme::name(&combo.response, t!("quick.protocol"));
            if before != self.quick.proto {
                self.quick.port = self.quick.proto.default_port().to_string();
            }
            // Placeholders are not labels: each field also gets an accessible name.
            let mut enter = false;
            for (value, key, width, password) in [
                (&mut self.quick.host, "common.host", 180.0, false),
                (&mut self.quick.port, "common.port", 48.0, false),
                (&mut self.quick.user, "common.user", 120.0, false),
                (&mut self.quick.pass, "common.password", 120.0, true),
            ] {
                let label = urftp_i18n::text(key);
                let r = ui.add(theme::single_line(value).hint_text(label).password(password).desired_width(width));
                theme::name(&r, label);
                enter |= password && r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            }
            if theme::primary_button(ui, t!("quick.connect")).clicked() || enter {
                self.quick_connect(ui.ctx());
            }
            self.add_quick_button(ui);
        });
        ui.add_space(4.0);
        actions
    }

    /// Sites of the site manager for the connection menus.
    fn site_entries(&self) -> Vec<SiteEntry> {
        self.sites
            .book
            .all_sites()
            .into_iter()
            .map(|(groups, s)| SiteEntry { groups, id: s.id, name: s.name.clone(), enabled: s.enabled })
            .collect()
    }

    /// "About" window: version, licence.
    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.about_open {
            return;
        }
        let mut open = true;
        let mut close = false;
        egui::Window::new(t!("about.title"))
            .id(egui::Id::new("about-window"))
            .default_pos([380.0, 160.0])
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("UltraRushFTP").heading().strong().color(theme::colors(ui).accent_text));
                    ui.label(RichText::new(t!("about.version", version = env!("CARGO_PKG_VERSION"))).strong());
                    ui.label(t!("about.tagline"));
                    ui.add_space(6.0);
                    ui.label(t!("about.build", arch = std::env::consts::ARCH));
                    ui.label("© 2026 werewolf004");
                    ui.label(t!("about.licence"));
                    ui.add_space(6.0);
                    if theme::primary_button(ui, t!("about.close")).clicked() {
                        close = true;
                    }
                });
            });
        if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.about_open = false;
        }
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
        ui.horizontal(|ui| {
            for (i, tab) in self.tabs.iter().enumerate() {
                let busy = tab.engine_events.is_some();
                let text = format!("{}{}", if busy { "⟳ " } else { "" }, tab.title(i + 1));
                if ui.selectable_label(i == self.active, text).clicked() {
                    self.active = i;
                }
                if self.tabs.len() > 1 {
                    let r = theme::icon_button(ui, "✕", t!("tabs.close"));
                    theme::name(&r, t!("tabs.close_named", title = tab.title(i + 1)));
                    if r.clicked() {
                        close = Some(i);
                    }
                }
                ui.separator();
            }
            if theme::icon_button(ui, "+", t!("tabs.new")).clicked() {
                self.new_tab();
            }
        });
        if let Some(i) = close {
            self.close_tab(i);
        }
    }

    /// Closes a tab (the last one stays).
    fn close_tab(&mut self, i: usize) {
        if self.tabs.len() > 1 && i < self.tabs.len() {
            self.tabs.remove(i);
            self.active = self.active.min(self.tabs.len() - 1);
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let running = self.tabs.iter().filter(|t| t.engine_events.is_some()).count();
            let connected = self
                .tabs
                .iter()
                .flat_map(|t| t.sides.iter())
                .filter(|s| matches!(s, Side::Remote(r) if r.state == RemoteState::Connected))
                .count();
            ui.label(t!("status.tabs", n = self.tabs.len()));
            ui.separator();
            ui.label(t!("status.connected", n = connected));
            ui.separator();
            ui.label(t!("status.active_queues", n = running));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
                // Progress of the queue of the tab shown.
                let tab = &self.tabs[self.active];
                let st = tab.queue.lock().stats();
                if tab.engine.is_some() && st.total_bytes > 0 {
                    let (down, up) = self.stats.bandwidth();
                    let rate = ((down + up) * 1024.0) as u64;
                    let text = logs::status_text(&self.options.settings.interface.status_format, &st, rate);
                    let fraction = st.done_bytes as f32 / st.total_bytes as f32;
                    ui.separator();
                    let bar = ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).text(text.clone()).desired_width(320.0));
                    theme::name(&bar, text);
                }
            });
        });
    }

    fn apply(&mut self, actions: Vec<Action>, ctx: &egui::Context) {
        let rt = self.rt.clone();
        let opts = self.engine_options();
        let extra = self.queue_site_profiles(self.active);
        let mut scripts = Vec::new();
        let mut ops = Vec::new();
        let (mut raw_dialog, mut site_commands, mut manual) = (false, Vec::new(), false);
        let (mut connects, mut reconnects, mut open_sites) = (Vec::new(), Vec::new(), false);
        let clear_on_logout = self.options.settings.transfer.clear_queue_on_logout;
        // Queue file: Some(true) = save, Some(false) = load (file dialog after the loop).
        let mut queue_file: Option<bool> = None;
        let tab = &mut self.tabs[self.active];
        for a in actions {
            match a {
                Action::Op(side, op) => ops.push((side, op)),
                Action::RawDialog => raw_dialog = true,
                Action::ManualDownload => manual = true,
                Action::SiteCommand(i) => site_commands.push(i),
                Action::RunScript => scripts.push(false),
                Action::Invoke => scripts.push(true),
                Action::Open(side, name) => match &mut tab.sides[side] {
                    Side::Local(l) => l.open(&name),
                    Side::Remote(r) => r.handle.send(BrowserCmd::Cwd(rpath::join(&r.dir, &name))),
                },
                Action::Up(side) => match &mut tab.sides[side] {
                    Side::Local(l) => l.up(),
                    Side::Remote(r) => r.handle.send(BrowserCmd::Cwd("..".into())),
                },
                Action::Refresh(side) => {
                    tab.sides[side].clear_filter();
                    match &mut tab.sides[side] {
                        Side::Local(l) => l.reload(),
                        Side::Remote(r) => r.handle.send(BrowserCmd::Refresh),
                    }
                }
                Action::Disconnect(side) => {
                    if clear_on_logout {
                        let site =
                            [&tab.sides[side]].into_iter().chain(tab.parked[side].iter()).find_map(|s| s.profile().map(|p| p.name.clone()));
                        if let Some(site) = site {
                            let n = tab.queue.lock().remove_where(|i| {
                                i.state == ItemState::Waiting
                                    && (i.src.site.as_deref() == Some(&site) || i.dst.site.as_deref() == Some(&site))
                            });
                            if n > 0 {
                                tab.push_transfer(side, LogKind::Status, t!("log.queue_cleared_logout", n = n, site = site));
                            }
                            tab.prune_queue_selection();
                        }
                    }
                    tab.disconnect_side(side);
                }
                Action::SwitchSide(side) => tab.switch_side(side),
                Action::RemoteCwd(side, dir) => {
                    if let Side::Remote(r) = &tab.sides[side]
                        && !dir.trim().is_empty()
                    {
                        r.handle.send(BrowserCmd::Cwd(rpath::join(&r.dir, dir.trim())));
                    }
                }
                Action::ConnectSite(side, id, variant) => connects.push((side, id, variant)),
                Action::Reconnect(side) => reconnects.push(side),
                Action::OpenSiteManager => open_sites = true,
                Action::Transfer(side) => {
                    tab.enqueue_selection(side);
                    tab.start_queue(&rt, ctx, opts.clone(), &extra);
                }
                Action::StartQueue => match &tab.engine {
                    Some(e) if e.is_paused() => {
                        e.resume();
                        tab.push_transfer(0, LogKind::Status, t!("log.queue_resumed"));
                    }
                    _ => tab.start_queue(&rt, ctx, opts.clone(), &extra),
                },
                Action::QueuePause => {
                    if let Some(e) = &tab.engine {
                        e.pause();
                        tab.push_transfer(0, LogKind::Status, t!("log.queue_paused"));
                    }
                }
                Action::QueueSelect(id, toggle, range) => tab.select_queue_row(id, toggle, range),
                Action::QueueEdit(id) => tab.queue_edit = tab.queue.lock().get(id).map(QueueEdit::of),
                Action::QueueInvert(ids) => {
                    let mut q = tab.queue.lock();
                    let n = ids.iter().filter(|&&id| q.invert_fxp(id)).count();
                    drop(q);
                    tab.push_transfer(0, LogKind::Status, t!("log.queue_inverted", n = n));
                }
                Action::QueueMarkFailed(ids) => {
                    tab.queue.lock().mark_failed(&ids, &t!("queue.marked_failed"));
                }
                Action::QueueMarkAllFailed => {
                    let mut q = tab.queue.lock();
                    let ids = q.ids();
                    q.mark_failed(&ids, &t!("queue.marked_failed"));
                }
                Action::QueueClearFailed => {
                    tab.queue.lock().clear_failed();
                    tab.prune_queue_selection();
                }
                Action::QueueClearAll => {
                    tab.queue.lock().remove_where(|_| true);
                    tab.prune_queue_selection();
                }
                Action::QueueInfo => tab.queue_info = true,
                Action::QueueSaveFile => queue_file = Some(true),
                Action::Checksums(side, md5) => tab.write_checksums(side, md5, ctx),
                Action::Conditional(side) => {
                    if !tab.sides[side].selected().is_empty() {
                        tab.condition = Some(ConditionDialog { side, form: ConditionScheme::default(), error: None });
                    }
                }
                Action::QueueLoadFile => queue_file = Some(false),
                Action::ClearFinished => {
                    tab.queue.lock().remove_where(|i| i.state.is_finished());
                }
                Action::Focus(side) => tab.focus = side,
                Action::Cursor(side, m, extend) => {
                    tab.focus = side;
                    tab.sides[side].move_cursor(m, extend);
                }
                Action::ToggleCursor(side) => tab.sides[side].toggle_cursor(),
                Action::SelectAll(side) => {
                    tab.focus = side;
                    let n = tab.sides[side].rows().len();
                    tab.sides[side].selected_mut().extend(0..n);
                }
                Action::Sort(side, col) => {
                    let s = tab.sides[side].sort().toggle(col);
                    tab.sides[side].set_sort(s);
                }
                Action::NewFolder(side) => tab.ask_new_folder(side),
                Action::Rename(side) => tab.ask_rename(side),
                Action::Delete(side) => tab.ask_delete(side),
                Action::CopyPath(side, name) => ctx.copy_text(tab.sides[side].full_path(&name)),
                Action::CopyHttpUrl(side, name) => {
                    if let Side::Remote(r) = &tab.sides[side]
                        && let Some(url) = r.http.url_of(&rpath::join(&r.dir, &name))
                    {
                        ctx.copy_text(url);
                    }
                }
                Action::QueueStop(id) => tab.stop_item(id),
                Action::QueueStopAll => tab.stop_all(),
                Action::QueueMove(id, m) => tab.move_item(id, m),
                Action::QueueRemove(id) => {
                    tab.queue.lock().remove_where(|i| i.id == id);
                    tab.prune_queue_selection();
                }
                Action::QueueRetry(id) => {
                    tab.retry_item(id);
                    tab.start_queue(&rt, ctx, opts.clone(), &extra);
                }
                Action::QueueRetryAll => {
                    tab.queue.lock().reset_failed();
                    tab.start_queue(&rt, ctx, opts.clone(), &extra);
                }
            }
        }
        if open_sites {
            self.sites.open = true;
        }
        for (side, op) in ops {
            self.run_op(side, op, ctx);
        }
        if raw_dialog {
            self.raw.open_with(None);
        }
        if manual {
            self.open_manual(None);
        }
        for i in site_commands {
            self.site_command(i);
        }
        match queue_file {
            Some(true) => {
                let name = format!("{}.urq", self.tabs[self.active].title(self.active + 1));
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(t!("queue.save_file"))
                    .add_filter(t!("queue.file_filter"), &["urq", "json"])
                    .set_file_name(name)
                    .save_file()
                {
                    self.save_queue_file(&path);
                }
            }
            Some(false) => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(t!("queue.load_file"))
                    .add_filter(t!("queue.file_filter"), &["urq", "json"])
                    .pick_file()
                {
                    self.load_queue_file(&path);
                }
            }
            None => {}
        }
        // Connections need the site manager: after the actions of the tab.
        for (side, id, variant) in connects {
            if let Some(site) = self.sites.book.find(id).cloned() {
                self.connect_site_in(site, variant, side, ctx);
            }
        }
        for side in reconnects {
            let tab = self.active;
            if let Side::Remote(r) = &self.tabs[tab].sides[side] {
                let (profile, id, dir) = (r.profile.clone(), r.site_id, r.dir.clone());
                self.open_remote(tab, side, profile, id, Some(dir), ctx);
            }
        }
        // Script console: run after the other actions (it acts on the whole application).
        for invoke in scripts {
            let (src, tab) = {
                let t = &self.tabs[self.active];
                (if invoke { t.invoke.clone() } else { t.script.clone() }, self.active)
            };
            let out = if invoke {
                self.invoke(&src, ctx).map(|v| format!("= {v}"))
            } else {
                self.run_script(&src, ctx).map(|()| t!("script.done"))
            };
            let last = self.tabs.len() - 1;
            let t = &mut self.tabs[tab.min(last)];
            t.script_out.push(match out {
                Ok(m) => (true, m),
                Err(e) => (false, e.to_string()),
            });
            if t.script_out.len() > 200 {
                t.script_out.remove(0);
            }
        }
    }

    /// Profiles (from the site manager) of every site named in the queue of a tab.
    fn queue_site_profiles(&self, tab: usize) -> Vec<SiteProfile> {
        let Some(t) = self.tabs.get(tab) else { return Vec::new() };
        let names: BTreeSet<String> =
            t.queue.lock().items().iter().flat_map(|i| [i.src.site.clone(), i.dst.site.clone()]).flatten().collect();
        let mut v: Vec<SiteProfile> = names.iter().filter_map(|n| self.sites.book.find_by_name(n)).map(|s| self.site_profile(s)).collect();
        let known: BTreeSet<String> = v.iter().map(|p| p.name.clone()).collect();
        v.extend(t.adhoc.iter().filter(|p| names.contains(&p.name) && !known.contains(&p.name)).cloned());
        v
    }

    /// Keyboard shortcuts. Application shortcuts work everywhere outside dialogs; the file
    /// list keys apply to the active panel when no control has the keyboard focus (Tab moves
    /// between controls, Escape gives the keys back to the list).
    fn keyboard(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let tab = &self.tabs[self.active];
        if tab.prompt.is_some() || tab.trust.is_some() {
            return Vec::new();
        }
        use egui::{Key, Modifiers};
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(Modifiers::NONE, Key::F1) {
            self.shortcuts_open = !self.shortcuts_open;
        }
        if pressed(Modifiers::COMMAND, Key::T) {
            self.new_tab();
        }
        if pressed(Modifiers::COMMAND, Key::W) {
            self.close_tab(self.active);
        }
        if pressed(Modifiers::COMMAND, Key::R) {
            self.raw.open_with(None);
        }
        let n = self.tabs.len();
        if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::Tab) {
            self.active = (self.active + n - 1) % n;
        } else if pressed(Modifiers::COMMAND, Key::Tab) {
            self.active = (self.active + 1) % n;
        }
        let tab = &self.tabs[self.active];
        let busy = ctx.egui_wants_keyboard_input() || ctx.memory(|m| m.focused().is_some());
        let queue_dialog = self.queue_end.is_some()
            || self.tabs.get(self.active).is_some_and(|t| t.queue_edit.is_some() || t.condition.is_some() || t.tools.busy());
        // `Ctrl+V` with files on the clipboard reaches no text field: it is read from the
        // keyboard state (only on the press).
        let paste = urftp_sys::win::ctrl_v_down() && ctx.input(|i| i.focused);
        let paste_pressed = paste && !std::mem::replace(&mut self.paste_down, paste);
        self.paste_down = paste;
        if busy || queue_dialog || self.sites.open || self.options.open || self.shortcuts_open || self.about_open {
            return Vec::new();
        }
        let side = tab.focus;
        let mut out = Vec::new();
        if pressed(Modifiers::NONE, Key::F6) || pressed(Modifiers::SHIFT, Key::F6) {
            out.push(Action::Focus(1 - side));
        }
        for (key, m) in [
            (Key::ArrowUp, CursorMove::By(-1)),
            (Key::ArrowDown, CursorMove::By(1)),
            (Key::PageUp, CursorMove::By(-PAGE_ROWS)),
            (Key::PageDown, CursorMove::By(PAGE_ROWS)),
            (Key::Home, CursorMove::First),
            (Key::End, CursorMove::Last),
        ] {
            // Shift first: a shortcut without modifiers also matches Shift+key.
            if pressed(Modifiers::SHIFT, key) {
                out.push(Action::Cursor(side, m, true));
            } else if pressed(Modifiers::NONE, key) {
                out.push(Action::Cursor(side, m, false));
            }
        }
        if pressed(Modifiers::COMMAND, Key::Space) {
            out.push(Action::ToggleCursor(side));
        }
        if pressed(Modifiers::COMMAND, Key::A) {
            out.push(Action::SelectAll(side));
        }
        if paste_pressed {
            out.push(Action::Op(side, Op::Paste));
        }
        for (m, key, op) in [
            (Modifiers::COMMAND, Key::F, Op::Filter),
            (Modifiers::COMMAND, Key::M, Op::Mark),
            (Modifiers::COMMAND | Modifiers::SHIFT, Key::C, Op::CopyNames),
            (Modifiers::NONE, Key::Plus, Op::SelectMask(true)),
            (Modifiers::NONE, Key::Minus, Op::SelectMask(false)),
        ] {
            if pressed(m, key) {
                out.push(Action::Op(side, op));
            }
        }
        let file_at_cursor =
            tab.sides[side].cursor().row.and_then(|i| tab.sides[side].rows().get(i)).filter(|r| !r.is_dir).map(|r| r.name.clone());
        if tab.sides[side].is_remote()
            && let Some(name) = file_at_cursor
        {
            if pressed(Modifiers::NONE, Key::F3) {
                out.push(Action::Op(side, Op::View(name.clone())));
            }
            if pressed(Modifiers::NONE, Key::F4) {
                out.push(Action::Op(side, Op::Edit(name)));
            }
        }
        if pressed(Modifiers::NONE, Key::Enter)
            && let Some(n) = tab.sides[side].folder_to_open()
        {
            out.push(Action::Open(side, n));
        }
        for (key, action) in [
            (Key::Backspace, Action::Up(side)),
            (Key::F5, Action::Refresh(side)),
            (Key::F2, Action::Rename(side)),
            (Key::F7, Action::NewFolder(side)),
            (Key::Delete, Action::Delete(side)),
            (Key::Space, Action::Transfer(side)),
            (Key::Insert, Action::Transfer(side)),
        ] {
            if pressed(Modifiers::NONE, key) {
                out.push(action);
            }
        }
        out
    }

    /// "Keyboard shortcuts" window (F1, Help menu).
    fn shortcuts_window(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        let mut open = true;
        egui::Window::new(t!("shortcuts.title"))
            .id(egui::Id::new("shortcuts-window"))
            .default_pos([240.0, 110.0])
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("shortcuts").num_columns(2).spacing([24.0, 6.0]).striped(true).show(ui, |ui| {
                    for (keys, what) in SHORTCUTS {
                        ui.label(RichText::new(urftp_i18n::text(keys)).monospace().strong());
                        ui.label(urftp_i18n::text(what));
                        ui.end_row();
                    }
                });
            });
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.shortcuts_open = false;
        }
    }

    /// Input dialogs (new folder, rename) and confirmation (delete).
    fn prompt_dialog(&mut self, ctx: &egui::Context) {
        let enter = self.options.settings.transfer.enter_new_folder;
        let tab = &mut self.tabs[self.active];
        let Some(prompt) = &mut tab.prompt else { return };
        let mut done: Option<bool> = None;
        egui::Modal::new(egui::Id::new(("prompt", tab.id))).show(ctx, |ui| {
            ui.set_min_width(380.0);
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            match prompt {
                Prompt::NewFolder { name, .. } => {
                    let h = ui.heading(t!("prompt.new_folder"));
                    let r = ui.add(theme::single_line(name).desired_width(f32::INFINITY)).labelled_by(h.id);
                    focus_once(ui, &r);
                }
                Prompt::Rename { from, to, .. } => {
                    let h = ui.heading(t!("prompt.rename"));
                    ui.label(RichText::new(from.as_str()).weak());
                    let r = ui.add(theme::single_line(to).desired_width(f32::INFINITY)).labelled_by(h.id);
                    focus_once(ui, &r);
                }
                Prompt::Delete { items, .. } => {
                    ui.heading(t!("prompt.delete"));
                    let dirs = items.iter().filter(|(_, d)| *d).count();
                    ui.label(if dirs > 0 {
                        t!("prompt.delete_count_dirs", n = items.len(), dirs = dirs)
                    } else {
                        t!("prompt.delete_count", n = items.len())
                    });
                    for (n, d) in items.iter().take(8) {
                        ui.label(RichText::new(format!("{} {n}", if *d { "📁" } else { "📄" })).weak());
                    }
                    if items.len() > 8 {
                        ui.label(RichText::new(t!("prompt.and_more", n = items.len() - 8)).weak());
                    }
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let delete = matches!(prompt, Prompt::Delete { .. });
                let ok =
                    if delete { theme::danger_button(ui, t!("common.delete")) } else { theme::primary_button(ui, t!("common.validate")) };
                if ok.clicked() || (enter && !delete) {
                    done = Some(true);
                }
                let cancel = ui.button(t!("common.cancel"));
                // Deleting is irreversible: the keyboard starts on Cancel.
                if delete {
                    focus_once(ui, &cancel);
                }
                if cancel.clicked() || escape {
                    done = Some(false);
                }
            });
        });
        if let Some(ok) = done
            && let Some(p) = tab.prompt.take()
            && ok
        {
            tab.run_prompt(p, enter);
        }
    }

    fn trust_dialog(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active];
        let Some(prompt) = &tab.trust else { return };
        let mut decision = None;
        egui::Modal::new(egui::Id::new(("trust", tab.id))).show(ctx, |ui| {
            ui.set_max_width(520.0);
            let what = match prompt.kind {
                TrustKind::TlsCertificate => t!("trust.tls"),
                TrustKind::SshHostKey => t!("trust.ssh"),
            };
            ui.heading(t!("trust.title"));
            ui.add_space(6.0);
            ui.label(what);
            ui.label(RichText::new(&prompt.reason).weak());
            ui.add_space(6.0);
            ui.label(t!("trust.fingerprint"));
            ui.code(&prompt.fingerprint);
            ui.add_space(6.0);
            ui.label(t!("trust.advice"));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, t!("trust.accept")).clicked() {
                    decision = Some(true);
                }
                let cancel = ui.button(t!("common.cancel"));
                // Trusting a server is a security decision: the keyboard starts on Cancel.
                focus_once(ui, &cancel);
                if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    decision = Some(false);
                }
            });
        });
        let Some(trust) = decision else { return };
        let Some(prompt) = tab.trust.take() else { return };
        if !trust {
            return;
        }
        if let Side::Remote(r) = &mut tab.sides[prompt.side] {
            match &mut r.profile.protocol {
                SiteProtocol::Ftp(cfg) => {
                    if let Security::Explicit(o) | Security::Implicit(o) = &mut cfg.security {
                        o.verify = TlsVerify::Fingerprint(prompt.fingerprint.clone());
                    }
                }
                SiteProtocol::Sftp { config, .. } => config.host_key = HostKeyPolicy::Known(vec![prompt.fingerprint.clone()]),
            }
            r.state = RemoteState::Connecting;
            r.handle.send(BrowserCmd::Connect(Box::new(r.profile.clone())));
            // Site manager site: the trusted fingerprint is stored in its record.
            if let Some(id) = r.site_id {
                self.sites.remember_trust(id, prompt.kind, &prompt.fingerprint);
            }
        }
    }
}

/// Gives the keyboard focus to a dialog control when nothing has it yet (on opening), so
/// that Tab can then move to the other controls.
fn focus_once(ui: &egui::Ui, r: &egui::Response) {
    if ui.memory(|m| m.focused().is_none()) {
        r.request_focus();
    }
}

/// Sleep, hibernate or shut down the computer.
fn run_power_action(action: QueueEnd) -> Result<(), String> {
    let r = match action {
        QueueEnd::Sleep => urftp_sys::power::sleep(),
        QueueEnd::Hibernate => urftp_sys::power::hibernate(),
        QueueEnd::ShutDown => urftp_sys::power::shut_down(),
        QueueEnd::Nothing | QueueEnd::Disconnect | QueueEnd::Exit => Ok(()),
    };
    r.map_err(|e| e.to_string())
}

/// Translated name of an end-of-queue action.
pub(crate) fn queue_end_label(a: QueueEnd) -> String {
    match a {
        QueueEnd::Nothing => t!("queue_end.nothing"),
        QueueEnd::Disconnect => t!("queue_end.disconnect"),
        QueueEnd::Exit => t!("queue_end.exit"),
        QueueEnd::Sleep => t!("queue_end.sleep"),
        QueueEnd::Hibernate => t!("queue_end.hibernate"),
        QueueEnd::ShutDown => t!("queue_end.shut_down"),
    }
}

impl Drop for UltraRushApp {
    fn drop(&mut self) {
        self.save_queue();
    }
}

impl eframe::App for UltraRushApp {
    // Called on every frame, and also while the window is minimised: transfers, statistics
    // and mIRC requests keep being processed.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let log_paths = self.options.settings.transfer.log_paths;
        let browse = &self.options.settings.browse;
        let invalidate = browse.dir_cache && browse.update_cache_on_transfer;
        let traffic: Vec<(TransferKind, u64)> = self.tabs.iter_mut().flat_map(|t| t.poll(log_paths, invalidate)).collect();
        for t in &mut self.tabs {
            t.poll_edits(&self.options.settings.browse, &self.rt, ctx);
        }
        if self.logs_flushed.elapsed() >= Duration::from_secs(1) || ctx.input(|i| i.viewport().close_requested()) {
            self.logs_flushed = std::time::Instant::now();
            if let Err(e) = logs::flush() {
                let a = self.active;
                self.tabs[a].push_log(0, LogKind::Error, t!("log.log_file_failed", error = e));
            }
        }
        self.stats.record(&traffic);
        self.check_queue_end(ctx);
        self.forward_raw_replies();
        if self.ipc_enabled {
            self.sync_ipc_class();
            self.serve_ipc(ctx);
        }
        // The unfinished queue on disk: every two seconds when it changed, and at closing.
        if self.queue_checked.elapsed() >= Duration::from_secs(2) || ctx.input(|i| i.viewport().close_requested()) {
            self.queue_checked = std::time::Instant::now();
            self.save_queue();
        }
        // `-rx`: close when no queue runs and nothing is left to transfer.
        if self.exit_when_done
            && self.startup.is_none()
            && self.tabs.iter().all(|t| t.engine_events.is_none() && !t.queue.lock().has_pending())
        {
            self.stats.save();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.exit_when_done {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Starts Ident on the first frame, then whenever its settings change.
        if self.ident_applied.is_none() {
            self.sync_ident();
        }
        if std::mem::take(&mut self.autoconnect) {
            self.quick_connect(&ctx);
        }
        if let Some(o) = self.startup.take() {
            self.apply_startup(o, &ctx);
        }
        // `URUSHFTP_SCRIPT`: script run at startup (development, automation).
        if !self.startup_script_done {
            self.startup_script_done = true;
            if let Ok(src) = std::env::var("URUSHFTP_SCRIPT") {
                let r = self.run_script(&src, &ctx);
                let t = self.active;
                self.tabs[t].script_out.push(match r {
                    Ok(()) => (true, t!("startup.env_script_done")),
                    Err(e) => (false, t!("startup.env_script_failed", error = e)),
                });
            }
        }
        if self.tabs.iter().any(|t| t.engine_events.is_some()) {
            ctx.request_repaint_after(Duration::from_millis(150));
        }

        let mut menu_actions = Vec::new();
        egui::Panel::top("top").show(ui, |ui| menu_actions = self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::top("tabs").show(ui, |ui| self.tab_strip(ui));

        let mut actions = self.keyboard(&ctx);
        actions.append(&mut menu_actions);
        let entries = self.site_entries();
        let bookmarks = self.panel_bookmarks();
        egui::CentralPanel::default().show(ui, |ui| {
            let tab = &mut self.tabs[self.active];
            let running = tab.engine.is_some();
            let switch = [tab.switch_label(0), tab.switch_label(1)];
            let parked_remote = [0, 1].map(|s| matches!(tab.parked[s], Some(Side::Remote(_))));
            let paused = tab.paused();
            let ui_opts = &self.options.settings.interface;
            let log_view = (ui_opts.log_site_names, ui_opts.log_times);
            let Container { id, dock, sides, queue, progress, log, focus, script, invoke, script_out, queue_sel, tools, .. } = tab;
            let mut viewer = PaneViewer {
                sides,
                queue,
                progress,
                log,
                actions: &mut actions,
                focus: *focus,
                running,
                script,
                invoke,
                script_out,
                sites: &entries,
                switch,
                parked_remote,
                queue_sel,
                paused,
                list_rect: &mut tools.list_rect,
                bookmarks: &bookmarks,
                log_view,
            };
            let style = DockStyle::from_egui(ui.style().as_ref());
            DockArea::new(dock)
                .id(egui::Id::new(("dock", *id)))
                .style(style)
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut viewer);
        });
        self.apply(actions, &ctx);
        self.files_dropped(&ctx);
        self.prompt_dialog(&ctx);
        self.tools_ui(&ctx);
        self.raw_dialog_window(&ctx);
        self.manual_window(&ctx);
        self.watch_clipboard(&ctx);
        self.saved_queue_dialog(&ctx);
        self.trust_dialog(&ctx);
        self.shortcuts_window(&ctx);
        self.about_window(&ctx);
        self.queue_edit_dialog(&ctx);
        self.queue_info_window(&ctx);
        self.queue_end_dialog(&ctx);
        self.conditional_dialog(&ctx);
        self.sites.show(&ctx, &self.options.settings);
        if self.options.show(&ctx) {
            self.apply_settings(&ctx);
        }
        if let Some((site, variant)) = self.sites.take_connect() {
            self.connect_site(site, variant, &ctx);
        }
        self.run_probes(&ctx);
    }
}

// ====================================================================== panels

struct PaneViewer<'a> {
    sides: &'a mut [Side; 2],
    queue: &'a Arc<Mutex<Queue>>,
    progress: &'a HashMap<ItemId, (u64, Option<u64>)>,
    log: &'a [LogEntry],
    actions: &'a mut Vec<Action>,
    /// Panel active for the keyboard.
    focus: usize,
    /// An engine is running (to enable "Stop").
    running: bool,
    script: &'a mut String,
    invoke: &'a mut String,
    script_out: &'a [(bool, String)],
    /// Sites of the site manager (connection button of the panels).
    sites: &'a [SiteEntry],
    /// Text of the local ⇄ remote button of each panel (`None`: nothing to switch to).
    switch: [Option<String>; 2],
    /// A connection is kept behind the local folder of the panel.
    parked_remote: [bool; 2],
    /// Selected rows of the queue.
    queue_sel: &'a BTreeSet<ItemId>,
    /// The engine of the tab is paused.
    paused: bool,
    /// Screen area of each file list (files dropped from Explorer).
    list_rect: &'a mut [Option<egui::Rect>; 2],
    /// Bookmarks of the site of each panel.
    bookmarks: &'a [Vec<String>; 2],
    /// The logs show (site names, times).
    log_view: (bool, bool),
}

/// A site of the site manager, for the connection menus.
struct SiteEntry {
    groups: Vec<String>,
    id: u64,
    name: String,
    enabled: bool,
}

/// Sites of the site manager as a menu (one sub-menu per group); a click connects panel
/// `side`, `Shift` asks for a ghost login and `Ctrl` for `-password`.
fn connection_menu(ui: &mut egui::Ui, entries: &[SiteEntry], side: usize, actions: &mut Vec<Action>) {
    let all: Vec<&SiteEntry> = entries.iter().collect();
    if all.is_empty() {
        ui.label(RichText::new(t!("conn.no_sites")).italics());
    }
    site_menu_level(ui, &all, 0, side, actions);
    ui.separator();
    if ui.button(t!("menu.site_manager")).clicked() {
        actions.push(Action::OpenSiteManager);
        ui.close();
    }
}

fn site_menu_level(ui: &mut egui::Ui, entries: &[&SiteEntry], depth: usize, side: usize, actions: &mut Vec<Action>) {
    let mut groups: Vec<&str> = Vec::new();
    for e in entries {
        if let Some(g) = e.groups.get(depth)
            && !groups.contains(&g.as_str())
        {
            groups.push(g);
        }
    }
    for g in groups {
        let inner: Vec<&SiteEntry> = entries.iter().copied().filter(|e| e.groups.get(depth).map(String::as_str) == Some(g)).collect();
        ui.menu_button(format!("📁 {g}"), |ui| site_menu_level(ui, &inner, depth + 1, side, actions));
    }
    for e in entries.iter().filter(|e| e.groups.len() == depth) {
        let r = ui.add_enabled(e.enabled, egui::Button::new(format!("🌐 {}", e.name)));
        if r.clicked() {
            let variant = sites::LoginVariant::from_modifiers(ui.input(|i| i.modifiers));
            actions.push(Action::ConnectSite(side, e.id, variant));
            ui.close();
        }
    }
}

impl TabViewer for PaneViewer<'_> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
        let mark = |side: usize| if self.focus == side { "▸ " } else { "" };
        match tab {
            Pane::Left => format!("{}{}", mark(0), self.sides[0].title()).into(),
            Pane::Right => format!("{}{}", mark(1), self.sides[1].title()).into(),
            Pane::Queue => t!("pane.queue").into(),
            Pane::Log => t!("pane.log").into(),
            Pane::TransferLog => t!("pane.transfer_log").into(),
            Pane::SystemLog => t!("pane.system_log").into(),
            Pane::Script => t!("pane.script").into(),
        }
    }

    fn id(&mut self, tab: &mut Pane) -> egui::Id {
        egui::Id::new(*tab as u8)
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Pane) {
        match tab {
            Pane::Left => self.file_pane(ui, 0),
            Pane::Right => self.file_pane(ui, 1),
            Pane::Queue => self.queue_pane(ui),
            Pane::Log => self.log_pane(ui, LogChannel::Ftp),
            Pane::TransferLog => self.log_pane(ui, LogChannel::Transfer),
            Pane::SystemLog => self.log_pane(ui, LogChannel::System),
            Pane::Script => self.script_pane(ui),
        }
    }
}

impl PaneViewer<'_> {
    fn file_pane(&mut self, ui: &mut egui::Ui, side: usize) {
        let arrow = if side == 0 { t!("files.transfer_right") } else { t!("files.transfer_left") };
        let p = theme::colors(ui);
        // Connection bar of the panel (FTPRush: toolbar of each window).
        ui.horizontal(|ui| {
            let sites = self.sites;
            let actions = &mut *self.actions;
            ui.menu_button(t!("conn.connect"), |ui| connection_menu(ui, sites, side, actions))
                .response
                .on_hover_text(t!("conn.connect_hint"));
            let remote = matches!(self.sides[side], Side::Remote(_));
            if ui.add_enabled(remote, egui::Button::new(t!("conn.reconnect"))).clicked() {
                self.actions.push(Action::Reconnect(side));
            }
            if ui.add_enabled(remote || self.parked_remote[side], egui::Button::new(t!("files.disconnect"))).clicked() {
                self.actions.push(Action::Disconnect(side));
            }
            let label = self.switch[side].clone();
            let r = ui
                .add_enabled(label.is_some(), egui::Button::new(label.unwrap_or_else(|| t!("conn.to_local"))))
                .on_hover_text(t!("conn.switch_hint"))
                .on_disabled_hover_text(t!("conn.switch_hint"));
            if r.clicked() {
                self.actions.push(Action::SwitchSide(side));
            }
            fileops::bookmarks_menu(ui, side, &self.bookmarks[side], self.actions);
        });
        ui.horizontal(|ui| {
            if theme::icon_button(ui, "⬆", t!("files.parent")).clicked() {
                self.actions.push(Action::Up(side));
            }
            if theme::icon_button(ui, "⟳", t!("files.refresh")).clicked() {
                self.actions.push(Action::Refresh(side));
            }
            if theme::icon_button(ui, "📁+", t!("files.new_folder")).clicked() {
                self.actions.push(Action::NewFolder(side));
            }
            match &mut self.sides[side] {
                Side::Local(l) => {
                    let r = ui.add(theme::single_line(&mut l.path_edit).desired_width(ui.available_width() - 120.0));
                    theme::name(&r, t!("files.path"));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        l.path = PathBuf::from(l.path_edit.trim());
                        l.reload();
                    }
                }
                Side::Remote(r) => {
                    // The state is told by the symbol and its text, not by the colour alone.
                    let (dot, color, state) = match &r.state {
                        RemoteState::Connected => ("●", p.success, t!("files.state_connected")),
                        RemoteState::Connecting => ("◌", p.warning, t!("files.state_connecting")),
                        _ => ("✕", p.error, t!("files.state_disconnected")),
                    };
                    let d = ui.label(RichText::new(dot).color(color)).on_hover_text(&state);
                    theme::name(&d, state);
                    let field =
                        theme::single_line(&mut r.path_edit).font(egui::TextStyle::Monospace).desired_width(ui.available_width() - 120.0);
                    let f = ui.add(field);
                    theme::name(&f, t!("files.remote_path"));
                    if f.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.actions.push(Action::RemoteCwd(side, r.path_edit.clone()));
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !self.sides[side].selected().is_empty();
                if ui.add_enabled(enabled, egui::Button::new(arrow)).clicked() {
                    self.actions.push(Action::Transfer(side));
                }
            });
        });
        match &self.sides[side] {
            Side::Local(l) if l.error.is_some() => {
                ui.colored_label(p.error, format!("⚠ {}", l.error.as_deref().unwrap_or_default()));
            }
            Side::Remote(r) => match &r.state {
                RemoteState::Connecting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t!("files.connecting"));
                    });
                }
                RemoteState::Failed(e) => {
                    ui.colored_label(p.error, format!("⚠ {e}"));
                }
                _ => {}
            },
            _ => {}
        }
        fileops::filter_bar(ui, &mut self.sides[side]);
        ui.separator();

        let rows: Vec<FileRow> = self.sides[side].rows().to_vec();
        let marks = self.sides[side].list().marks.clone();
        let remote_panel = self.sides[side].is_remote();
        let http = matches!(&self.sides[side], Side::Remote(r) if r.http.is_set());
        let local_panel = matches!(&self.sides[side], Side::Local(_));
        let selected = self.sides[side].selected().clone();
        let sort = self.sides[side].sort();
        let cursor = self.sides[side].cursor();
        // The cursor is drawn in the panel that has the keyboard only.
        let cursor_row = cursor.row.filter(|_| self.focus == side);
        self.sides[side].selection_mut().1.scroll = false;
        let cursor_stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
        let mut clicked: Option<(usize, bool, bool)> = None;
        let mut opened: Option<String> = None;
        let mut sort_by: Option<SortCol> = None;
        let mut menu: Vec<Action> = Vec::new();

        // The whole list is a drop zone: files dragged from the other panel are
        // transferred into it.
        let (zone, dropped) = ui.dnd_drop_zone::<DragFiles, ()>(egui::Frame::new(), |ui| {
            let painter = ui.painter().clone();
            let mut table = TableBuilder::new(ui);
            if cursor.scroll
                && let Some(r) = cursor.row
            {
                table = table.scroll_to_row(r, None);
            }
            table
                .id_salt(("files", side))
                .striped(true)
                // Rows are not in the Tab order: the list is driven by its keyboard cursor.
                .sense(Sense::CLICK | Sense::DRAG)
                .column(Column::remainder().at_least(160.0).clip(true))
                .column(Column::initial(90.0).at_least(60.0))
                .column(Column::initial(130.0).at_least(80.0))
                .columns(Column::initial(80.0).at_least(40.0).clip(true), if remote_panel { 3 } else { 1 })
                .header(22.0, |mut h| {
                    for (col, label) in [
                        (SortCol::Name, t!("files.col_name")),
                        (SortCol::Size, t!("files.col_size")),
                        (SortCol::Modified, t!("files.col_modified")),
                    ] {
                        h.col(|ui| {
                            let arrow = if sort.col == col { if sort.asc { " ⏶" } else { " ⏷" } } else { "" };
                            let r = ui.add(egui::Label::new(RichText::new(format!("{label}{arrow}")).strong()).sense(Sense::click()));
                            if r.on_hover_text(t!("files.sort")).clicked() {
                                sort_by = Some(col);
                            }
                        });
                    }
                    let extra: &[&str] =
                        if remote_panel { &["files.col_perms", "files.col_owner", "files.col_group"] } else { &["files.col_attributes"] };
                    for k in extra {
                        h.col(|ui| {
                            ui.label(RichText::new(urftp_i18n::text(k)).strong());
                        });
                    }
                })
                .body(|body| {
                    body.rows(22.0, rows.len(), |mut row| {
                        let i = row.index();
                        let f = &rows[i];
                        row.set_selected(selected.contains(&i));
                        let marked = marks.contains(&f.name);
                        row.col(|ui| {
                            let icon = if f.is_dir { "📁" } else { "📄" };
                            let text = RichText::new(format!("{icon} {}", f.name));
                            // Marked: bold and underlined, not told by colour alone.
                            ui.label(if marked { text.strong().underline() } else { text });
                        });
                        row.col(|ui| {
                            ui.label(if f.is_dir { String::new() } else { human_size(f.size) });
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(&f.modified).weak());
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(&f.perms).monospace().weak());
                        });
                        if remote_panel {
                            row.col(|ui| {
                                ui.label(RichText::new(&f.owner).weak());
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(&f.group).weak());
                            });
                        }
                        let resp = row.response();
                        if cursor_row == Some(i) {
                            painter.rect_stroke(resp.rect.shrink(1.0), 3.0, cursor_stroke, egui::StrokeKind::Inside);
                        }
                        let size = if f.is_dir { t!("files.folder") } else { human_size(f.size) };
                        let spoken = if marked {
                            format!("{}, {}, {size}, {}", f.name, t!("files.marked"), f.modified)
                        } else {
                            format!("{}, {size}, {}", f.name, f.modified)
                        };
                        let is_selected = selected.contains(&i);
                        resp.ctx.accesskit_node_builder(resp.id, |n| {
                            n.set_label(spoken);
                            n.set_selected(is_selected);
                        });
                        if resp.drag_started() && !selected.contains(&i) {
                            clicked = Some((i, false, false));
                        }
                        resp.dnd_set_drag_payload(DragFiles { from: side });
                        if resp.double_clicked() && f.is_dir {
                            if resp.ctx.input(|i| i.modifiers.shift) {
                                menu.push(Action::Op(side, Op::OpenBoth(f.name.clone())));
                            } else {
                                opened = Some(f.name.clone());
                            }
                        } else if resp.double_clicked() && remote_panel {
                            menu.push(Action::Op(side, Op::View(f.name.clone())));
                        } else if resp.clicked() {
                            let m = resp.ctx.input(|inp| inp.modifiers);
                            clicked = Some((i, m.command, m.shift));
                        } else if resp.secondary_clicked() && !selected.contains(&i) {
                            clicked = Some((i, false, false));
                        }
                        resp.context_menu(|ui| {
                            if ui.button(t!("files.ctx_transfer")).clicked() {
                                menu.push(Action::Transfer(side));
                                ui.close();
                            }
                            if ui.button(t!("files.ctx_conditional")).clicked() {
                                menu.push(Action::Conditional(side));
                                ui.close();
                            }
                            if local_panel {
                                if ui.button(t!("files.ctx_create_sfv")).clicked() {
                                    menu.push(Action::Checksums(side, false));
                                    ui.close();
                                }
                                if ui.button(t!("files.ctx_create_md5")).clicked() {
                                    menu.push(Action::Checksums(side, true));
                                    ui.close();
                                }
                            }
                            if f.is_dir && ui.button(t!("files.ctx_open")).clicked() {
                                menu.push(Action::Open(side, f.name.clone()));
                                ui.close();
                            }
                            ui.separator();
                            fileops::context_menu(ui, side, remote_panel, f, &mut menu);
                            ui.separator();
                            if ui.button(t!("files.ctx_rename")).clicked() {
                                menu.push(Action::Rename(side));
                                ui.close();
                            }
                            if ui.button(t!("files.ctx_delete")).clicked() {
                                menu.push(Action::Delete(side));
                                ui.close();
                            }
                            if ui.button(t!("files.ctx_new_folder")).clicked() {
                                menu.push(Action::NewFolder(side));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(t!("files.ctx_copy_path")).clicked() {
                                menu.push(Action::CopyPath(side, f.name.clone()));
                                ui.close();
                            }
                            if http && ui.button(t!("files.ctx_copy_http")).clicked() {
                                menu.push(Action::CopyHttpUrl(side, f.name.clone()));
                                ui.close();
                            }
                            if ui.button(t!("files.ctx_refresh")).clicked() {
                                menu.push(Action::Refresh(side));
                                ui.close();
                            }
                        });
                    });
                });
        });

        self.list_rect[side] = Some(zone.response.rect);
        if let Some(drop) = dropped
            && drop.from != side
        {
            self.actions.push(Action::Transfer(drop.from));
        }
        if let Some(col) = sort_by {
            self.actions.push(Action::Sort(side, col));
        }
        if let Some(name) = opened {
            self.actions.push(Action::Open(side, name));
        }
        if let Some((i, toggle, range)) = clicked {
            self.actions.push(Action::Focus(side));
            let (sel, cur) = self.sides[side].selection_mut();
            let previous = cur.row;
            *cur = Cursor { row: Some(i), scroll: false };
            if range {
                // Shift: from the cursor (or the last selection) to here.
                let anchor = previous.or_else(|| sel.iter().next_back().copied()).unwrap_or(i);
                let (a, b) = if anchor <= i { (anchor, i) } else { (i, anchor) };
                sel.extend(a..=b);
            } else if toggle {
                if !sel.remove(&i) {
                    sel.insert(i);
                }
            } else {
                sel.clear();
                sel.insert(i);
            }
        }
        self.actions.extend(menu);
    }

    fn queue_pane(&mut self, ui: &mut egui::Ui) {
        let items = self.queue.lock().items().to_vec();
        let p = theme::colors(ui);
        let failed = items.iter().filter(|i| matches!(i.state, ItemState::Failed(_) | ItemState::Stopped)).count();
        let paused = self.paused;
        ui.horizontal(|ui| {
            if ui.button(if paused { t!("queue.resume") } else { t!("queue.start") }).clicked() {
                self.actions.push(Action::StartQueue);
            }
            if ui.add_enabled(self.running && !paused, egui::Button::new(t!("queue.pause"))).on_hover_text(t!("queue.pause_hint")).clicked()
            {
                self.actions.push(Action::QueuePause);
            }
            if ui.add_enabled(self.running, egui::Button::new(t!("queue.stop_all"))).clicked() {
                self.actions.push(Action::QueueStopAll);
            }
            if ui.add_enabled(failed > 0, egui::Button::new(t!("queue.retry_failed", n = failed))).clicked() {
                self.actions.push(Action::QueueRetryAll);
            }
            if ui.button(t!("queue.clear_finished")).clicked() {
                self.actions.push(Action::ClearFinished);
            }
            let actions = &mut *self.actions;
            ui.menu_button(t!("queue.more"), |ui| {
                for (label, action) in [
                    (t!("queue.info"), Action::QueueInfo),
                    (t!("queue.mark_all_failed"), Action::QueueMarkAllFailed),
                    (t!("queue.clear_failed"), Action::QueueClearFailed),
                    (t!("queue.clear_all"), Action::QueueClearAll),
                ] {
                    if ui.button(label).clicked() {
                        actions.push(action);
                        ui.close();
                    }
                }
                ui.separator();
                for (label, action) in [(t!("queue.load_file"), Action::QueueLoadFile), (t!("queue.save_file"), Action::QueueSaveFile)] {
                    if ui.button(label).clicked() {
                        actions.push(action);
                        ui.close();
                    }
                }
            });
            let summary = if paused { t!("queue.summary_paused", n = items.len()) } else { t!("queue.summary", n = items.len()) };
            ui.label(RichText::new(summary).weak());
        });
        ui.separator();
        let running = self.running;
        let mut menu: Vec<Action> = Vec::new();
        TableBuilder::new(ui)
            .id_salt("queue")
            .striped(true)
            .sense(Sense::click())
            .column(Column::initial(28.0))
            .column(Column::remainder().at_least(160.0).clip(true))
            .column(Column::initial(90.0))
            .column(Column::initial(170.0))
            .column(Column::initial(140.0).clip(true))
            .header(22.0, |mut h| {
                for t in [String::new(), t!("queue.col_item"), t!("queue.col_size"), t!("queue.col_progress"), t!("queue.col_state")] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(24.0, items.len(), |mut row| {
                    let it = &items[row.index()];
                    row.set_selected(self.queue_sel.contains(&it.id));
                    let dir = match it.kind() {
                        urftp_core::TransferKind::Upload => "⬆",
                        urftp_core::TransferKind::Download => "⬇",
                        urftp_core::TransferKind::Fxp => "↔",
                    };
                    row.col(|ui| {
                        let d = ui.label(dir);
                        theme::name(
                            &d,
                            match it.kind() {
                                urftp_core::TransferKind::Upload => t!("queue.kind_upload"),
                                urftp_core::TransferKind::Download => t!("queue.kind_download"),
                                urftp_core::TransferKind::Fxp => t!("queue.kind_fxp"),
                            },
                        );
                    });
                    row.col(|ui| {
                        let name = Path::new(&it.src.path)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| it.src.path.clone());
                        ui.label(format!("{}{}", if it.is_dir { "📁 " } else { "" }, name))
                            .on_hover_text(format!("{} → {}", it.src.path, it.dst.path));
                    });
                    row.col(|ui| {
                        ui.label(it.size.map(human_size).unwrap_or_default());
                    });
                    row.col(|ui| {
                        let (done, total) = self.progress.get(&it.id).copied().unwrap_or((0, it.size));
                        let frac = match (&it.state, total) {
                            (ItemState::Done, _) => 1.0,
                            (_, Some(t)) if t > 0 => done as f32 / t as f32,
                            _ => 0.0,
                        };
                        let bar =
                            ui.add(egui::ProgressBar::new(frac.clamp(0.0, 1.0)).fill(p.accent_fill).show_percentage().corner_radius(6));
                        theme::name(&bar, t!("queue.col_progress"));
                    });
                    row.col(|ui| {
                        let (text, color) = match &it.state {
                            ItemState::Waiting => (t!("state.waiting"), p.status),
                            ItemState::Running => (t!("state.running"), p.accent_text),
                            ItemState::Done => (t!("state.done"), p.success),
                            ItemState::Skipped(r) => (t!("state.skipped", reason = skip_text(r)), p.status),
                            ItemState::NeedsConfirmation => (t!("state.confirm"), p.warning),
                            ItemState::Stopped => (t!("state.stopped"), p.status),
                            ItemState::Failed(e) => (t!("state.failed", error = e), p.error),
                        };
                        ui.label(RichText::new(&text).color(color)).on_hover_text(text);
                    });
                    let id = it.id;
                    let response = row.response();
                    if response.clicked() || response.secondary_clicked() && !self.queue_sel.contains(&id) {
                        let m = response.ctx.input(|i| i.modifiers);
                        menu.push(Action::QueueSelect(id, m.command && response.clicked(), m.shift && response.clicked()));
                    }
                    // The menu acts on the selection when the row is part of it.
                    let targets: Vec<&QueueItem> = if self.queue_sel.contains(&id) {
                        items.iter().filter(|i| self.queue_sel.contains(&i.id)).collect()
                    } else {
                        vec![it]
                    };
                    let ids: Vec<ItemId> = targets.iter().map(|i| i.id).collect();
                    let any_running = targets.iter().any(|i| i.state == ItemState::Running);
                    let retryable =
                        targets.iter().any(|i| matches!(i.state, ItemState::Failed(_) | ItemState::Stopped | ItemState::NeedsConfirmation));
                    let to_do =
                        targets.iter().any(|i| matches!(i.state, ItemState::Waiting | ItemState::Stopped | ItemState::NeedsConfirmation));
                    let fxp = targets.iter().any(|i| i.kind() == urftp_core::TransferKind::Fxp && i.state != ItemState::Running);
                    let editable = targets.len() == 1 && !any_running;
                    response.context_menu(|ui| {
                        if ui.add_enabled(running && any_running, egui::Button::new(t!("queue.stop"))).clicked() {
                            menu.extend(targets.iter().filter(|i| i.state == ItemState::Running).map(|i| Action::QueueStop(i.id)));
                            ui.close();
                        }
                        if ui.add_enabled(retryable, egui::Button::new(t!("queue.retry"))).clicked() {
                            menu.extend(ids.iter().map(|&i| Action::QueueRetry(i)));
                            ui.close();
                        }
                        if ui.add_enabled(editable, egui::Button::new(t!("queue.edit"))).clicked() {
                            menu.push(Action::QueueEdit(id));
                            ui.close();
                        }
                        if ui.add_enabled(fxp, egui::Button::new(t!("queue.invert"))).on_hover_text(t!("queue.invert_hint")).clicked() {
                            menu.push(Action::QueueInvert(ids.clone()));
                            ui.close();
                        }
                        if ui.add_enabled(to_do, egui::Button::new(t!("queue.mark_failed"))).clicked() {
                            menu.push(Action::QueueMarkFailed(ids.clone()));
                            ui.close();
                        }
                        ui.separator();
                        for (m, label) in [
                            (Move::Top, t!("queue.top")),
                            (Move::Up, t!("queue.up")),
                            (Move::Down, t!("queue.down")),
                            (Move::Bottom, t!("queue.bottom")),
                        ] {
                            if ui.button(label).clicked() {
                                menu.push(Action::QueueMove(id, m));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui
                            .add_enabled(targets.iter().any(|i| i.state != ItemState::Running), egui::Button::new(t!("queue.remove")))
                            .clicked()
                        {
                            menu.extend(ids.iter().map(|&i| Action::QueueRemove(i)));
                            ui.close();
                        }
                    });
                });
            });
        self.actions.extend(menu);
    }

    /// Script console: run FTPRush Pascal Script or evaluate an expression, like
    /// `RushScript` and `Invoke` from mIRC.
    fn script_pane(&mut self, ui: &mut egui::Ui) {
        let p = theme::colors(ui);
        ui.horizontal(|ui| {
            let r = theme::labelled(
                ui,
                t!("script.invoke"),
                theme::single_line(self.invoke).hint_text("RushApp.TotalDownloadSize").desired_width(320.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button(t!("script.evaluate")).clicked() || enter {
                self.actions.push(Action::Invoke);
            }
            ui.separator();
            if theme::primary_button(ui, t!("script.run")).clicked() {
                self.actions.push(Action::RunScript);
            }
        });
        ui.add_space(4.0);
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().id_salt("script-src").show(&mut cols[0], |ui| {
                let r = ui.add(
                    egui::TextEdit::multiline(self.script)
                        .code_editor()
                        .desired_rows(8)
                        .desired_width(f32::INFINITY)
                        .hint_text("RushApp.FTP.Login('MySite', '/', 0);"),
                );
                theme::name(&r, t!("script.source"));
            });
            egui::ScrollArea::vertical().id_salt("script-out").stick_to_bottom(true).show(&mut cols[1], |ui| {
                if self.script_out.is_empty() {
                    ui.label(RichText::new(t!("script.placeholder")).weak());
                }
                for (ok, line) in self.script_out {
                    ui.label(RichText::new(line).monospace().color(if *ok { p.success } else { p.error }));
                }
            });
        });
    }

    fn log_pane(&mut self, ui: &mut egui::Ui, channel: LogChannel) {
        let p = theme::colors(ui);
        let (names, times) = self.log_view;
        egui::ScrollArea::vertical().id_salt(("log", channel as u8)).auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            for e in self.log.iter().filter(|e| e.channel == channel) {
                let color = match e.kind {
                    LogKind::Command => p.command,
                    LogKind::Reply => p.success,
                    LogKind::Status => p.status,
                    LogKind::Error => p.error,
                };
                ui.label(RichText::new(logs::line_text(e, names, times)).monospace().color(color));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_urls() {
        let q = parse_connect_url("ftps://bob:p@ss@ftp.example.com:2121").unwrap();
        assert!(q.proto == QuickProto::FtpsExplicit && q.host == "ftp.example.com" && q.port == "2121");
        assert_eq!((q.user.as_str(), q.pass.as_str()), ("bob", "p@ss"));
        let q = parse_connect_url("sftp://host").unwrap();
        assert!(q.proto == QuickProto::Sftp && q.port == "22" && q.user.is_empty());
        assert!(parse_connect_url("http://x").is_none());
    }
}
