//! Browsing and file operations of the panels (block I): quick filter, marks, selection
//! by mask, renaming several items, CHMOD, moving, dates, raw listing, folder comparison,
//! searches and walks of folders (space used, directory tree), directory cache, viewing
//! and editing remote files with other programs, files dropped from Explorer or pasted,
//! and bookmarks for both panels.

use std::sync::mpsc;
use std::time::SystemTime;

use urftp_core::NameCase;
use urftp_core::browsing::{self, CompareBy, QuickFilter, RenameRule, TreeOptions, WalkEntry};
use urftp_core::itemopts::{Conditions, FolderEntry};
use urftp_core::settings::program_for;
use urftp_proto::session::RemoteSession;

use super::*;

// ====================================================================== list of a panel

/// What a panel knows besides the rows it shows: the whole folder, the quick filter, the
/// items a comparison hides, and the marks.
#[derive(Default)]
pub(super) struct ListState {
    /// Every item of the folder, sorted.
    pub(super) all: Vec<FileRow>,
    /// Folder of the listing (marks and filter stay while it does not change).
    dir: String,
    /// Quick filter bar (`Ctrl+F`) shown, focus to give it, text, regular expression.
    pub(super) filter_open: bool,
    filter_focus: bool,
    filter_text: String,
    filter_regex: bool,
    filter_error: Option<String>,
    filter: QuickFilter,
    /// Same items hidden by "Compare folders" (until the next listing).
    hidden: BTreeSet<String>,
    /// Marked names (bold): kept while the folder does not change.
    pub(super) marks: BTreeSet<String>,
}

impl ListState {
    fn clear_filter(&mut self) {
        self.filter_text.clear();
        self.filter = QuickFilter::default();
        self.filter_error = None;
        self.filter_open = false;
    }

    /// Shown rows / all rows, when something is filtered out.
    pub(super) fn counts(&self, shown: usize) -> Option<(usize, usize)> {
        (shown != self.all.len()).then_some((shown, self.all.len()))
    }
}

/// Mutable parts of a panel list.
pub(super) struct Parts<'a> {
    pub(super) list: &'a mut ListState,
    pub(super) rows: &'a mut Vec<FileRow>,
    pub(super) selected: &'a mut BTreeSet<usize>,
    pub(super) cursor: &'a mut Cursor,
    pub(super) sort: &'a mut Sort,
}

impl Parts<'_> {
    /// A new listing of `dir`: in another folder the marks and the filter are dropped.
    pub(super) fn load(mut self, mut all: Vec<FileRow>, dir: &str) {
        sort_rows(&mut all, *self.sort);
        if self.list.dir == dir {
            self.list.marks.retain(|m| all.iter().any(|r| &r.name == m));
        } else {
            self.list.marks.clear();
            self.list.clear_filter();
            self.list.dir = dir.to_string();
            self.selected.clear();
            *self.cursor = Cursor::default();
        }
        self.list.hidden.clear();
        self.list.all = all;
        self.show();
    }

    /// Rebuilds the shown rows; the selected names stay selected.
    pub(super) fn show(&mut self) {
        let picked: BTreeSet<String> = self.selected.iter().filter_map(|&i| self.rows.get(i)).map(|r| r.name.clone()).collect();
        let l = &*self.list;
        *self.rows = l.all.iter().filter(|r| !l.hidden.contains(&r.name) && l.filter.matches(&r.name)).cloned().collect();
        *self.selected = self.rows.iter().enumerate().filter(|(_, r)| picked.contains(&r.name)).map(|(i, _)| i).collect();
        if self.cursor.row.is_some_and(|c| c >= self.rows.len()) {
            *self.cursor = Cursor::default();
        }
    }
}

impl Side {
    /// `F5`: the quick filter goes, the whole folder comes back.
    pub(super) fn clear_filter(&mut self) {
        let mut p = self.parts();
        p.list.clear_filter();
        p.list.hidden.clear();
        p.show();
    }

    /// Selected items, or else the item under the cursor.
    fn picked_or_cursor(&self) -> Vec<(String, bool)> {
        let v = self.picked();
        if !v.is_empty() {
            return v;
        }
        self.cursor().row.and_then(|i| self.rows().get(i)).map(|r| vec![(r.name.clone(), r.is_dir)]).unwrap_or_default()
    }

    /// Current folder (remote path or local path).
    fn here(&self) -> String {
        match self {
            Side::Local(l) => l.path.to_string_lossy().into_owned(),
            Side::Remote(r) => r.dir.clone(),
        }
    }
}

// ====================================================================== operations

/// Operation of the Edit and FTP menus, of the panel menus and shortcuts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Op {
    /// Quick filter bar (`Ctrl+F`).
    Filter,
    /// Marks or unmarks the selection (`Ctrl+M`).
    Mark,
    UnmarkAll,
    SelectMarked,
    /// Full paths of the marked items to the clipboard, one per line.
    ExportMarks,
    /// Select (`true`, `+`) or unselect (`-`) by mask.
    SelectMask(bool),
    /// Names of the selection to the clipboard, one per line (`Ctrl+Shift+C`).
    CopyNames,
    RenameMany,
    Chmod,
    MoveTo,
    SetDate,
    RawList,
    /// Compares the two panels of the tab.
    Compare,
    FindFiles,
    SpaceUsed,
    ExportTree,
    ClearCache,
    ClearAllCaches,
    /// Opens a remote file with the viewer (`F3`) or the editor (`F4`).
    View(String),
    Edit(String),
    /// `Shift`+double-click: the folder in both panels.
    OpenBoth(String),
    /// Bookmark of the site (name); `true` (`Shift`): its local folder in the other panel.
    Bookmark(String, bool),
    /// Files copied in Explorer, sent into the panel (`Ctrl+V`).
    Paste,
    CurrentEdits,
}

/// Modal dialog of an operation.
pub(super) enum Dialog {
    Mask {
        side: usize,
        masks: String,
        select: bool,
    },
    Rename {
        side: usize,
        items: Vec<(String, bool)>,
        form: RenameForm,
    },
    Chmod {
        side: usize,
        items: Vec<(String, bool)>,
        mode: String,
        recursive: bool,
    },
    Move {
        side: usize,
        items: Vec<(String, bool)>,
        target: String,
    },
    Date {
        side: usize,
        items: Vec<(String, bool)>,
        when: String,
    },
    /// Space used (`tree` false) or directory tree export of a folder.
    Walk {
        side: usize,
        tree: bool,
        path: String,
        depth: String,
        tree_opts: TreeOptions,
    },
}

/// Fields of "Rename multiple files" (numbers as typed).
#[derive(Clone)]
pub(super) struct RenameForm {
    pub(super) template: String,
    pub(super) find: String,
    pub(super) replace: String,
    pub(super) regex: bool,
    pub(super) case: NameCase,
    pub(super) start: String,
    pub(super) step: String,
    pub(super) digits: String,
}

impl Default for RenameForm {
    fn default() -> Self {
        let r = RenameRule::default();
        Self {
            template: r.template,
            find: String::new(),
            replace: String::new(),
            regex: false,
            case: NameCase::Keep,
            start: r.counter_start.to_string(),
            step: r.counter_step.to_string(),
            digits: r.counter_digits.to_string(),
        }
    }
}

impl RenameForm {
    fn rule(&self) -> Option<RenameRule> {
        Some(RenameRule {
            template: self.template.clone(),
            find: self.find.clone(),
            replace: self.replace.clone(),
            regex: self.regex,
            case: self.case,
            counter_start: self.start.trim().parse().ok()?,
            counter_step: self.step.trim().parse().ok()?,
            counter_digits: self.digits.trim().parse().ok().filter(|d: &usize| *d <= 12)?,
        })
    }
}

/// Options of "Compare folder contents" (checked in the menu, as in FTPRush).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompareForm {
    pub(super) by: CompareBy,
    pub(super) partial: bool,
    pub(super) hide_same: bool,
    pub(super) select: bool,
}

impl Default for CompareForm {
    fn default() -> Self {
        Self { by: CompareBy::Name, partial: false, hide_same: false, select: true }
    }
}

/// Message of a folder walk.
enum WalkMsg {
    Entry(WalkEntry),
    Error(String),
    Done(Result<(), String>),
}

/// Window of a running or finished walk (space used, directory tree).
struct WalkJob {
    id: u64,
    root: String,
    /// `Some`: directory tree with these options; `None`: space used.
    tree: Option<TreeOptions>,
    rx: mpsc::Receiver<WalkMsg>,
    stop: Arc<AtomicBool>,
    entries: Vec<WalkEntry>,
    errors: Vec<String>,
    done: Option<Result<(), String>>,
    /// Text of the tree, built once at the end.
    text: String,
}

/// Kind of items searched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FindKind {
    #[default]
    Both,
    Folders,
    Files,
}

/// An item found by "Find files at sites".
#[derive(Debug, Clone)]
struct Found {
    site: String,
    host: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: Option<i64>,
}

enum FindMsg {
    Site(String),
    Found(Found),
    Error(String),
    Done,
}

/// "Find files at sites" window.
#[derive(Default)]
struct FindWindow {
    /// Sites of the site manager: (id, name, searched?).
    sites: Vec<(u64, String, bool)>,
    path: String,
    depth: String,
    include: String,
    exclude: String,
    kind: FindKind,
    newer: String,
    older: String,
    min_kib: String,
    max_kib: String,
    error: Option<String>,
    running: Option<(mpsc::Receiver<FindMsg>, Arc<AtomicBool>)>,
    status: String,
    results: Vec<Found>,
    selected: BTreeSet<usize>,
}

/// State of a remote file opened with another program.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EditState {
    Downloading,
    /// Shown by the viewer: not watched.
    Viewing,
    /// Watched: sent back when the program saves it.
    Watching,
    Uploading,
    Failed(String),
}

enum EditMsg {
    Downloaded(Result<(), String>),
    Uploaded(Result<(), String>),
}

/// A remote file being viewed or edited ("Current edit files").
pub(super) struct EditedFile {
    profile: SiteProfile,
    remote: String,
    local: PathBuf,
    edit: bool,
    state: EditState,
    /// Date of the local copy when it was last downloaded or sent.
    stamp: Option<SystemTime>,
    rx: Option<mpsc::Receiver<EditMsg>>,
}

/// Dialogs, windows and background work of the operations of a tab.
#[derive(Default)]
pub(super) struct Tools {
    pub(super) dialog: Option<Dialog>,
    /// Raw listing shown: (site, folder, text).
    raw: Option<(String, String, String)>,
    find: Option<FindWindow>,
    walks: Vec<WalkJob>,
    pub(super) edits: Vec<EditedFile>,
    edits_open: bool,
    pub(super) compare: CompareForm,
    last_mask: String,
    next_id: u64,
    /// Screen area of each file list (files dropped from Explorer).
    pub(super) list_rect: [Option<egui::Rect>; 2],
}

impl Tools {
    /// A window of this tab is open (the list keys are then left to it).
    pub(super) fn busy(&self) -> bool {
        self.dialog.is_some()
    }

    /// Every walk window has its result.
    #[cfg(test)]
    pub(super) fn walks_done(&self) -> bool {
        !self.walks.is_empty() && self.walks.iter().all(|w| w.done.is_some())
    }

    /// Text of the last directory tree.
    #[cfg(test)]
    pub(super) fn last_tree_text(&self) -> String {
        self.walks.iter().rev().find(|w| w.tree.is_some()).map(|w| w.text.clone()).unwrap_or_default()
    }
}

/// Temporary folder of the remote files opened with other programs.
fn edit_dir() -> PathBuf {
    std::env::temp_dir().join("UltraRushFTP").join("files")
}

/// Program for a file: the first matching association, else Notepad (never the program
/// Windows associates with the type, which could run a downloaded program).
fn program(list: &[urftp_core::settings::FileAssociation], name: &str) -> String {
    program_for(list, name).unwrap_or("notepad.exe").to_string()
}

fn modified_of(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// `ftp://host/path` of a found item.
fn url_of(f: &Found) -> String {
    format!("ftp://{}{}", f.host, f.path)
}

/// Matches a search word list (`;`): words with `*`/`?` are masks of the whole name,
/// the others are looked for inside it. Empty: everything.
fn words_match(list: &str, name: &str) -> Option<bool> {
    let lower = name.to_lowercase();
    let words: Vec<String> = list.split(';').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return None;
    }
    Some(
        words
            .iter()
            .any(|w| if w.contains(['*', '?']) { urftp_core::pattern::wildcard_match(w, &lower) } else { lower.contains(w.as_str()) }),
    )
}

impl Container {
    /// Queues the local files and folders `paths` towards the folder of the remote panel
    /// `to` (Explorer drop, paste). Returns how many were queued.
    pub(super) fn enqueue_local_paths(&mut self, paths: &[PathBuf], to: usize) -> usize {
        if !self.sides[to].is_remote() {
            self.push_log(to, LogKind::Error, t!("ops.drop_needs_remote"));
            return 0;
        }
        let mut q = self.queue.lock();
        let mut n = 0;
        for p in paths {
            let Ok(meta) = std::fs::metadata(p) else { continue };
            let Some(name) = p.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            let dst = self.sides[to].endpoint(&name);
            q.push(Endpoint::local(p.to_string_lossy()), dst, meta.is_dir(), (!meta.is_dir()).then_some(meta.len()), None);
            n += 1;
        }
        drop(q);
        self.push_log(to, LogKind::Status, t!("log.items_queued", n = n));
        n
    }

    /// Compares the panels: the same items are hidden and / or the others selected.
    fn compare(&mut self) {
        let form = self.tools.compare;
        let items = |s: &Side| s.list().all.iter().map(|r| (r.name.clone(), r.is_dir, r.size)).collect::<Vec<_>>();
        let (a, b) = (items(&self.sides[0]), items(&self.sides[1]));
        fn view(v: &[(String, bool, u64)]) -> Vec<(&str, bool, u64)> {
            v.iter().map(|(n, d, s)| (n.as_str(), *d, *s)).collect()
        }
        let (same_a, same_b) = browsing::compare_folders(&view(&a), &view(&b), form.by, form.partial);
        let mut differ = 0;
        for (side, (all, same)) in [(a, same_a), (b, same_b)].into_iter().enumerate() {
            let mut p = self.sides[side].parts();
            p.list.hidden = if form.hide_same {
                all.iter().zip(&same).filter(|(_, s)| **s).map(|(r, _)| r.0.clone()).collect()
            } else {
                BTreeSet::new()
            };
            let other: BTreeSet<String> = all.iter().zip(&same).filter(|(_, s)| !**s).map(|(r, _)| r.0.clone()).collect();
            differ += other.len();
            p.show();
            if form.select {
                *p.selected = p.rows.iter().enumerate().filter(|(_, r)| other.contains(&r.name)).map(|(i, _)| i).collect();
            }
        }
        self.push_log(0, LogKind::Status, t!("ops.compared", n = differ));
    }

    /// Starts a walk of `root` in panel `side` and opens its window.
    fn start_walk(&mut self, side: usize, root: String, depth: usize, tree: Option<TreeOptions>, rt: &Handle, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let ctx2 = ctx.clone();
        let send = move |m: WalkMsg| {
            let _ = tx.send(m);
            ctx2.request_repaint();
        };
        match &self.sides[side] {
            Side::Remote(r) => {
                let profile = r.profile.clone();
                let (root, stop) = (root.clone(), stop.clone());
                rt.spawn(async move {
                    match RemoteSession::open(&profile, None).await {
                        Ok(mut s) => {
                            let r = s
                                .walk(&root, depth, &stop, |e| send(WalkMsg::Entry(e)), |d, e| send(WalkMsg::Error(format!("{d}: {e}"))))
                                .await;
                            s.quit().await;
                            send(WalkMsg::Done(r.map_err(|e| e.to_string())));
                        }
                        Err(e) => send(WalkMsg::Done(Err(e.to_string()))),
                    }
                });
            }
            Side::Local(_) => {
                let (root, stop) = (PathBuf::from(&root), stop.clone());
                std::thread::spawn(move || {
                    let r = browsing::walk_local(
                        &root,
                        depth,
                        &stop,
                        |e| send(WalkMsg::Entry(e)),
                        |d, e| send(WalkMsg::Error(format!("{d}: {e}"))),
                    );
                    send(WalkMsg::Done(r.map_err(|e| e.to_string())));
                });
            }
        }
        self.tools.next_id += 1;
        self.tools.walks.push(WalkJob {
            id: self.tools.next_id,
            root,
            tree,
            rx,
            stop,
            entries: Vec::new(),
            errors: Vec::new(),
            done: None,
            text: String::new(),
        });
    }

    /// Runs a confirmed dialog.
    fn run_dialog(&mut self, d: Dialog, rt: &Handle, ctx: &egui::Context) {
        match d {
            Dialog::Mask { side, masks, select } => {
                self.tools.last_mask.clone_from(&masks);
                let s = &mut self.sides[side];
                let hits: Vec<usize> =
                    s.rows().iter().enumerate().filter(|(_, r)| browsing::mask_matches(&masks, &r.name)).map(|(i, _)| i).collect();
                let sel = s.selected_mut();
                for i in hits {
                    if select {
                        sel.insert(i);
                    } else {
                        sel.remove(&i);
                    }
                }
            }
            Dialog::Rename { side, items, form } => {
                let Some(Ok(new)) = form.rule().map(|r| r.apply(&items)) else { return };
                match &self.sides[side] {
                    Side::Remote(r) => {
                        let pairs = items
                            .iter()
                            .zip(&new)
                            .filter(|((o, _), n)| o != *n)
                            .map(|((o, _), n)| (rpath::join(&r.dir, o), rpath::join(&r.dir, n)))
                            .collect();
                        r.handle.send(BrowserCmd::RenameMany(pairs));
                    }
                    Side::Local(l) => {
                        let dir = l.path.clone();
                        for ((old, _), n) in items.iter().zip(&new).filter(|((o, _), n)| o != *n) {
                            if let Err(e) = std::fs::rename(dir.join(old), dir.join(n)) {
                                self.push_log(side, LogKind::Error, t!("log.rename_failed", path = dir.join(old).display(), error = e));
                            }
                        }
                        if let Side::Local(l) = &mut self.sides[side] {
                            l.reload();
                        }
                    }
                }
            }
            Dialog::Chmod { side, items, mode, recursive } => {
                let (Some(mode), Side::Remote(r)) = (browsing::parse_mode(&mode), &self.sides[side]) else { return };
                let items = items.iter().map(|(n, d)| (rpath::join(&r.dir, n), *d)).collect();
                r.handle.send(BrowserCmd::Chmod { items, mode, recursive });
            }
            Dialog::Move { side, items, target } => {
                let target = target.trim();
                if target.is_empty() {
                    return;
                }
                match &self.sides[side] {
                    Side::Remote(r) => {
                        let to = rpath::join(&r.dir, target);
                        let pairs = items.iter().map(|(n, _)| (rpath::join(&r.dir, n), rpath::join(&to, n))).collect();
                        r.handle.send(BrowserCmd::RenameMany(pairs));
                    }
                    Side::Local(l) => {
                        let (dir, to) = (l.path.clone(), l.path.join(target));
                        for (n, _) in &items {
                            if let Err(e) = std::fs::rename(dir.join(n), to.join(n)) {
                                self.push_log(side, LogKind::Error, t!("log.rename_failed", path = dir.join(n).display(), error = e));
                            }
                        }
                        if let Side::Local(l) = &mut self.sides[side] {
                            l.reload();
                        }
                    }
                }
            }
            Dialog::Date { side, items, when } => {
                let (Some(epoch), Side::Remote(r)) = (browsing::parse_datetime(&when), &self.sides[side]) else { return };
                let paths = items.iter().map(|(n, _)| rpath::join(&r.dir, n)).collect();
                r.handle.send(BrowserCmd::SetDate { paths, epoch });
            }
            Dialog::Walk { side, tree, path, depth, tree_opts } => {
                let depth = depth.trim().parse().unwrap_or(0);
                let root = if path.trim().is_empty() { self.sides[side].here() } else { path.trim().to_string() };
                self.start_walk(side, root, depth, tree.then_some(tree_opts), rt, ctx);
            }
        }
    }

    /// Downloads a remote file of panel `side` and opens it with the viewer or the editor.
    fn open_remote_file(&mut self, side: usize, name: &str, edit: bool, rt: &Handle, ctx: &egui::Context) {
        let Side::Remote(r) = &self.sides[side] else { return };
        let (profile, remote) = (r.profile.clone(), rpath::join(&r.dir, name));
        self.tools.next_id += 1;
        let local = edit_dir().join(format!("{}-{}-{}", std::process::id(), self.id, self.tools.next_id)).join(name);
        let (tx, rx) = mpsc::channel();
        let (p2, r2, l2, ctx2) = (profile.clone(), remote.clone(), local.clone(), ctx.clone());
        rt.spawn(async move {
            let result = async {
                if let Some(dir) = l2.parent() {
                    tokio::fs::create_dir_all(dir).await.map_err(|e| e.to_string())?;
                }
                let mut s = RemoteSession::open(&p2, None).await.map_err(|e| e.to_string())?;
                let mut file = tokio::fs::File::create(&l2).await.map_err(|e| e.to_string())?;
                let r = s.retr(&r2, 0, &mut file, |_| {}).await;
                drop(file);
                s.quit().await;
                r.map(drop).map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(EditMsg::Downloaded(result));
            ctx2.request_repaint();
        });
        self.push_log(side, LogKind::Status, t!("ops.downloading_for_edit", path = remote));
        self.tools.edits.push(EditedFile { profile, remote, local, edit, state: EditState::Downloading, stamp: None, rx: Some(rx) });
    }

    /// Sends an edited file back to its site.
    fn upload_edit(&mut self, i: usize, rt: &Handle, ctx: &egui::Context) {
        let Some(e) = self.tools.edits.get_mut(i) else { return };
        let (tx, rx) = mpsc::channel();
        let (p, remote, local, ctx2) = (e.profile.clone(), e.remote.clone(), e.local.clone(), ctx.clone());
        e.state = EditState::Uploading;
        e.stamp = modified_of(&e.local);
        e.rx = Some(rx);
        rt.spawn(async move {
            let result = async {
                let mut s = RemoteSession::open(&p, None).await.map_err(|e| e.to_string())?;
                let mut file = tokio::fs::File::open(&local).await.map_err(|e| e.to_string())?;
                let r = s.stor(&remote, 0, &mut file, |_| {}).await;
                s.quit().await;
                r.map(drop).map_err(|e| e.to_string())
            }
            .await;
            let _ = tx.send(EditMsg::Uploaded(result));
            ctx2.request_repaint();
        });
    }

    /// Follows the opened files: launches the program once downloaded, sends a watched
    /// file back when its date changes.
    pub(super) fn poll_edits(&mut self, settings: &urftp_core::settings::BrowseSettings, rt: &Handle, ctx: &egui::Context) {
        let mut logs = Vec::new();
        let mut to_upload = Vec::new();
        let mut refresh = Vec::new();
        for (i, e) in self.tools.edits.iter_mut().enumerate() {
            let msg = e.rx.as_ref().and_then(|rx| rx.try_recv().ok());
            match msg {
                Some(EditMsg::Downloaded(Ok(()))) => {
                    e.rx = None;
                    e.stamp = modified_of(&e.local);
                    let name = e.local.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let prog = program(if e.edit { &settings.edit } else { &settings.view }, &name);
                    match std::process::Command::new(&prog).arg(&e.local).spawn() {
                        Ok(_) => {
                            e.state = if e.edit { EditState::Watching } else { EditState::Viewing };
                            logs.push((LogKind::Status, t!("ops.opened_with", path = e.remote, program = prog)));
                        }
                        Err(err) => {
                            e.state = EditState::Failed(err.to_string());
                            logs.push((LogKind::Error, t!("ops.program_failed", program = prog, error = err)));
                        }
                    }
                }
                Some(EditMsg::Downloaded(Err(err))) => {
                    e.rx = None;
                    logs.push((LogKind::Error, t!("ops.download_failed", path = e.remote, error = &err)));
                    e.state = EditState::Failed(err);
                }
                Some(EditMsg::Uploaded(r)) => {
                    e.rx = None;
                    match r {
                        Ok(()) => {
                            e.state = EditState::Watching;
                            logs.push((LogKind::Status, t!("ops.edit_uploaded", path = e.remote)));
                            refresh.push(e.profile.name.clone());
                        }
                        Err(err) => {
                            logs.push((LogKind::Error, t!("ops.upload_failed", path = e.remote, error = &err)));
                            e.state = EditState::Failed(err);
                        }
                    }
                }
                None => {
                    if e.state == EditState::Watching && modified_of(&e.local).is_some_and(|m| Some(m) != e.stamp) {
                        to_upload.push(i);
                    }
                }
            }
        }
        for (kind, text) in logs {
            self.push_log(0, kind, text);
        }
        for i in to_upload {
            self.upload_edit(i, rt, ctx);
        }
        for site in refresh {
            for s in &self.sides {
                if let Side::Remote(r) = s
                    && r.profile.name == site
                {
                    r.handle.send(BrowserCmd::Refresh);
                }
            }
        }
    }
}

// ====================================================================== application side

impl UltraRushApp {
    /// Runs an operation on panel `side` of the active tab.
    pub(super) fn run_op(&mut self, side: usize, op: Op, ctx: &egui::Context) {
        let rt = self.rt.clone();
        let entries = self.site_entries();
        let bookmarks = self.bookmarks_of(side);
        let tab = &mut self.tabs[self.active];
        match op {
            Op::Filter => {
                tab.focus = side;
                let l = tab.sides[side].parts().list;
                l.filter_open = true;
                l.filter_focus = true;
            }
            Op::Mark => {
                let names: Vec<String> = tab.sides[side].picked_or_cursor().into_iter().map(|(n, _)| n).collect();
                let marks = &mut tab.sides[side].parts().list.marks;
                for n in names {
                    if !marks.remove(&n) {
                        marks.insert(n);
                    }
                }
            }
            Op::UnmarkAll => tab.sides[side].parts().list.marks.clear(),
            Op::SelectMarked => {
                let p = tab.sides[side].parts();
                *p.selected = p.rows.iter().enumerate().filter(|(_, r)| p.list.marks.contains(&r.name)).map(|(i, _)| i).collect();
            }
            Op::ExportMarks => {
                let s = &tab.sides[side];
                let text: Vec<String> = s.list().marks.iter().map(|n| s.full_path(n)).collect();
                if !text.is_empty() {
                    ctx.copy_text(text.join("\r\n"));
                }
            }
            Op::SelectMask(select) => {
                let masks = if tab.tools.last_mask.is_empty() { "*.*".to_string() } else { tab.tools.last_mask.clone() };
                tab.tools.dialog = Some(Dialog::Mask { side, masks, select });
            }
            Op::CopyNames => {
                let names: Vec<String> = tab.sides[side].picked_or_cursor().into_iter().map(|(n, _)| n).collect();
                if !names.is_empty() {
                    ctx.copy_text(names.join("\r\n"));
                }
            }
            Op::RenameMany => {
                let items = tab.sides[side].picked_or_cursor();
                if !items.is_empty() {
                    tab.tools.dialog = Some(Dialog::Rename { side, items, form: RenameForm::default() });
                }
            }
            Op::Chmod => {
                let items = tab.sides[side].picked_or_cursor();
                if tab.sides[side].is_remote() && !items.is_empty() {
                    let first = tab.sides[side].rows().iter().find(|r| r.name == items[0].0);
                    let mode = first.and_then(|r| browsing::mode_of_permissions(&r.perms)).map(|m| m & 0o7777).unwrap_or(0o644);
                    tab.tools.dialog = Some(Dialog::Chmod { side, items, mode: browsing::mode_text(mode), recursive: false });
                }
            }
            Op::MoveTo => {
                let items = tab.sides[side].picked_or_cursor();
                if !items.is_empty() {
                    // The folder of the other panel when it shows the same place.
                    let other = &tab.sides[1 - side];
                    let same_kind = match (&tab.sides[side], other) {
                        (Side::Remote(a), Side::Remote(b)) => a.profile.name == b.profile.name,
                        (Side::Local(_), Side::Local(_)) => true,
                        _ => false,
                    };
                    let target = if same_kind { other.here() } else { tab.sides[side].here() };
                    tab.tools.dialog = Some(Dialog::Move { side, items, target });
                }
            }
            Op::SetDate => {
                let items = tab.sides[side].picked_or_cursor();
                if tab.sides[side].is_remote() && !items.is_empty() {
                    let first = tab.sides[side].rows().iter().find(|r| r.name == items[0].0).and_then(|r| r.epoch);
                    let when = browsing::format_datetime(first.unwrap_or_else(FtpTime::now_epoch));
                    tab.tools.dialog = Some(Dialog::Date { side, items, when });
                }
            }
            Op::RawList => {
                if let Side::Remote(r) = &tab.sides[side] {
                    r.handle.send(BrowserCmd::RawList);
                }
            }
            Op::Compare => tab.compare(),
            Op::FindFiles => {
                let path = if tab.sides[side].is_remote() { tab.sides[side].here() } else { "/".into() };
                let connected: Vec<String> = tab.sides.iter().filter_map(|s| s.profile().map(|p| p.name.clone())).collect();
                let sites = entries.iter().filter(|e| e.enabled).map(|e| (e.id, e.name.clone(), connected.contains(&e.name))).collect();
                if tab.tools.find.is_none() {
                    tab.tools.find = Some(FindWindow { sites, path, depth: "0".into(), ..FindWindow::default() });
                }
            }
            Op::SpaceUsed | Op::ExportTree => {
                let path = tab.sides[side].here();
                let tree = op == Op::ExportTree;
                tab.tools.dialog = Some(Dialog::Walk { side, tree, path, depth: "0".into(), tree_opts: TreeOptions::default() });
            }
            Op::ClearCache => {
                if let Side::Remote(r) = &tab.sides[side] {
                    r.handle.send(BrowserCmd::ClearCache);
                    tab.push_log(side, LogKind::Status, t!("ops.cache_cleared", site = r.profile.name));
                }
            }
            Op::ClearAllCaches => {
                for t in &self.tabs {
                    for s in t.sides.iter().chain(t.parked.iter().flatten()) {
                        if let Side::Remote(r) = s {
                            r.handle.send(BrowserCmd::ClearCache);
                        }
                    }
                }
                let tab = &mut self.tabs[self.active];
                tab.push_log(side, LogKind::Status, t!("ops.caches_cleared"));
            }
            Op::View(name) => tab.open_remote_file(side, &name, false, &rt, ctx),
            Op::Edit(name) => tab.open_remote_file(side, &name, true, &rt, ctx),
            Op::OpenBoth(name) => {
                for s in [side, 1 - side] {
                    match &mut tab.sides[s] {
                        Side::Local(l) => {
                            if s == side || l.path.join(&name).is_dir() {
                                l.open(&name);
                            }
                        }
                        Side::Remote(r) => r.handle.send(BrowserCmd::Cwd(rpath::join(&r.dir, &name))),
                    }
                }
            }
            Op::Bookmark(name, both) => {
                let Some(bm) = bookmarks.into_iter().find(|b| b.name == name) else { return };
                if let Side::Remote(r) = &tab.sides[side]
                    && !bm.remote_dir.trim().is_empty()
                {
                    r.handle.send(BrowserCmd::Cwd(bm.remote_dir.trim().to_string()));
                }
                if both
                    && !bm.local_dir.trim().is_empty()
                    && let Side::Local(l) = &mut tab.sides[1 - side]
                {
                    l.path = PathBuf::from(bm.local_dir.trim());
                    l.reload();
                }
            }
            Op::Paste => {
                let files = urftp_sys::win::clipboard_files();
                if files.is_empty() {
                    tab.push_log(side, LogKind::Status, t!("ops.paste_nothing"));
                } else if tab.enqueue_local_paths(&files, side) > 0 {
                    let (opts, extra) = (self.engine_options(), self.queue_site_profiles(self.active));
                    self.tabs[self.active].start_queue(&rt, ctx, opts, &extra);
                }
            }
            Op::CurrentEdits => tab.tools.edits_open = true,
        }
    }

    /// Bookmarks of the site record of the remote panel `side`.
    fn bookmarks_of(&self, side: usize) -> Vec<urftp_core::site::Bookmark> {
        match &self.tabs[self.active].sides[side] {
            Side::Remote(r) => r.site_id.and_then(|id| self.sites.book.find(id)).map(|s| s.bookmarks.clone()).unwrap_or_default(),
            Side::Local(_) => Vec::new(),
        }
    }

    /// Names of the bookmarks of each panel (connection bar).
    pub(super) fn panel_bookmarks(&self) -> [Vec<String>; 2] {
        [0, 1].map(|s| self.bookmarks_of(s).into_iter().map(|b| b.name).collect())
    }

    /// Files dropped from Explorer onto a remote panel: queued and sent.
    pub(super) fn files_dropped(&mut self, ctx: &egui::Context) {
        let (files, pos) = ctx.input(|i| {
            (
                i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect::<Vec<_>>(),
                i.pointer.latest_pos(),
            )
        });
        if files.is_empty() {
            return;
        }
        let tab = &mut self.tabs[self.active];
        let side = pos.and_then(|p| (0..2).find(|&s| tab.tools.list_rect[s].is_some_and(|r| r.contains(p)))).unwrap_or(tab.focus);
        if tab.enqueue_local_paths(&files, side) > 0 {
            let (rt, opts, extra) = (self.rt.clone(), self.engine_options(), self.queue_site_profiles(self.active));
            self.tabs[self.active].start_queue(&rt, ctx, opts, &extra);
        }
    }

    /// Dialogs and windows of the operations of the active tab.
    pub(super) fn tools_ui(&mut self, ctx: &egui::Context) {
        self.op_dialog(ctx);
        self.raw_window(ctx);
        self.walk_windows(ctx);
        self.find_window(ctx);
        self.edits_window(ctx);
    }

    fn op_dialog(&mut self, ctx: &egui::Context) {
        let rt = self.rt.clone();
        let tab = &mut self.tabs[self.active];
        let Some(dialog) = &mut tab.tools.dialog else { return };
        let mut done: Option<bool> = None;
        egui::Modal::new(egui::Id::new(("op-dialog", tab.id))).show(ctx, |ui| {
            ui.set_min_width(420.0);
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let mut valid = true;
            match dialog {
                Dialog::Mask { masks, select, .. } => {
                    let h = ui.heading(if *select { t!("ops.select_title") } else { t!("ops.unselect_title") });
                    let r = ui.add(theme::single_line(masks).desired_width(f32::INFINITY)).labelled_by(h.id);
                    focus_once(ui, &r);
                    ui.label(RichText::new(t!("ops.mask_hint")).weak());
                }
                Dialog::Rename { items, form, .. } => {
                    ui.heading(t!("ops.rename_title"));
                    egui::Grid::new("rename-form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        let r = theme::grid_row(ui, t!("ops.rename_template"), theme::single_line(&mut form.template).desired_width(260.0));
                        focus_once(ui, &r);
                        theme::grid_row(ui, t!("ops.rename_find"), theme::single_line(&mut form.find).desired_width(260.0));
                        theme::grid_row(ui, t!("ops.rename_replace"), theme::single_line(&mut form.replace).desired_width(260.0));
                        ui.label("");
                        ui.checkbox(&mut form.regex, t!("ops.rename_regex"));
                        ui.end_row();
                        ui.label(t!("ops.rename_case"));
                        egui::ComboBox::from_id_salt("rename-case")
                            .selected_text(match form.case {
                                NameCase::Keep => t!("case.keep"),
                                NameCase::Lower => t!("case.lower"),
                                NameCase::Upper => t!("case.upper"),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut form.case, NameCase::Keep, t!("case.keep"));
                                ui.selectable_value(&mut form.case, NameCase::Lower, t!("case.lower"));
                                ui.selectable_value(&mut form.case, NameCase::Upper, t!("case.upper"));
                            });
                        ui.end_row();
                        theme::grid_row(ui, t!("ops.rename_start"), theme::single_line(&mut form.start).desired_width(60.0));
                        theme::grid_row(ui, t!("ops.rename_step"), theme::single_line(&mut form.step).desired_width(60.0));
                        theme::grid_row(ui, t!("ops.rename_digits"), theme::single_line(&mut form.digits).desired_width(60.0));
                    });
                    ui.label(RichText::new(t!("ops.rename_hint")).weak());
                    ui.separator();
                    let preview =
                        form.rule().ok_or_else(|| t!("ops.rename_bad_number")).and_then(|r| r.apply(items).map_err(|e| e.to_string()));
                    match &preview {
                        Ok(new) => {
                            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                                egui::Grid::new("rename-preview").num_columns(3).striped(true).show(ui, |ui| {
                                    for ((old, _), n) in items.iter().zip(new) {
                                        ui.label(RichText::new(old).weak());
                                        ui.label("→");
                                        ui.label(n);
                                        ui.end_row();
                                    }
                                });
                            });
                        }
                        Err(e) => {
                            ui.colored_label(theme::colors(ui).error, format!("⚠ {e}"));
                        }
                    }
                    valid = preview.is_ok();
                }
                Dialog::Chmod { items, mode, recursive, .. } => {
                    ui.heading(t!("ops.chmod_title"));
                    ui.label(t!("ops.items", n = items.len()));
                    let mut bits = browsing::parse_mode(mode).unwrap_or(0);
                    let before = bits;
                    egui::Grid::new("chmod-bits").num_columns(4).spacing([16.0, 4.0]).show(ui, |ui| {
                        ui.label("");
                        for k in ["ops.chmod_read", "ops.chmod_write", "ops.chmod_exec"] {
                            ui.label(RichText::new(urftp_i18n::text(k)).strong());
                        }
                        ui.end_row();
                        for (who, shift) in [("ops.chmod_owner", 6), ("ops.chmod_group", 3), ("ops.chmod_others", 0)] {
                            let who = urftp_i18n::text(who);
                            ui.label(who);
                            for (k, bit) in [("ops.chmod_read", 4), ("ops.chmod_write", 2), ("ops.chmod_exec", 1)] {
                                let mut on = bits & (bit << shift) != 0;
                                let r = ui.checkbox(&mut on, "");
                                theme::name(&r, format!("{who} {}", urftp_i18n::text(k)));
                                if on {
                                    bits |= bit << shift;
                                } else {
                                    bits &= !(bit << shift);
                                }
                            }
                            ui.end_row();
                        }
                    });
                    if bits != before {
                        *mode = browsing::mode_text(bits);
                    }
                    let r = theme::labelled(ui, t!("ops.chmod_octal"), theme::single_line(mode).desired_width(60.0));
                    focus_once(ui, &r);
                    if items.iter().any(|(_, d)| *d) {
                        ui.checkbox(recursive, t!("ops.chmod_recursive"));
                    }
                    valid = browsing::parse_mode(mode).is_some();
                    if !valid {
                        ui.colored_label(theme::colors(ui).error, t!("ops.chmod_bad"));
                    }
                }
                Dialog::Move { items, target, .. } => {
                    let h = ui.heading(t!("ops.move_title"));
                    ui.label(t!("ops.items", n = items.len()));
                    let r = ui.add(theme::single_line(target).desired_width(f32::INFINITY)).labelled_by(h.id);
                    focus_once(ui, &r);
                }
                Dialog::Date { items, when, .. } => {
                    let h = ui.heading(t!("ops.date_title"));
                    ui.label(t!("ops.items", n = items.len()));
                    let r = ui.add(theme::single_line(when).desired_width(200.0)).labelled_by(h.id);
                    focus_once(ui, &r);
                    ui.label(RichText::new(t!("ops.date_hint")).weak());
                    valid = browsing::parse_datetime(when).is_some();
                    if !valid {
                        ui.colored_label(theme::colors(ui).error, t!("ops.date_bad"));
                    }
                }
                Dialog::Walk { tree, path, depth, tree_opts, .. } => {
                    ui.heading(if *tree { t!("ops.tree_title") } else { t!("ops.space_title") });
                    egui::Grid::new("walk-form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        let r = theme::grid_row(ui, t!("ops.walk_path"), theme::single_line(path).desired_width(300.0));
                        focus_once(ui, &r);
                        theme::grid_row(ui, t!("ops.walk_depth"), theme::single_line(depth).desired_width(60.0));
                        if *tree {
                            theme::grid_row(ui, t!("ops.tree_indent"), theme::single_line(&mut tree_opts.indent).desired_width(60.0));
                        }
                    });
                    ui.label(RichText::new(t!("ops.walk_depth_hint")).weak());
                    if *tree {
                        ui.checkbox(&mut tree_opts.sizes, t!("ops.tree_sizes"));
                        ui.checkbox(&mut tree_opts.folders_only, t!("ops.tree_folders_only"));
                    }
                    valid = depth.trim().is_empty() || depth.trim().parse::<usize>().is_ok();
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(valid, egui::Button::new(t!("common.validate"))).clicked() || (enter && valid) {
                    done = Some(true);
                }
                if ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        if let Some(ok) = done
            && let Some(d) = tab.tools.dialog.take()
            && ok
        {
            tab.run_dialog(d, &rt, ctx);
        }
    }

    fn raw_window(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active];
        let Some((site, dir, text)) = &tab.tools.raw else { return };
        let mut open = true;
        egui::Window::new(t!("ops.raw_title", site = site, dir = dir))
            .id(egui::Id::new(("raw-listing", tab.id)))
            .default_size([640.0, 380.0])
            .open(&mut open)
            .show(ctx, |ui| {
                if ui.button(t!("ops.copy")).clicked() {
                    ui.ctx().copy_text(text.clone());
                }
                egui::ScrollArea::both().show(ui, |ui| {
                    let mut shown = text.as_str();
                    ui.add(egui::TextEdit::multiline(&mut shown).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                });
            });
        if !open {
            tab.tools.raw = None;
        }
    }

    fn walk_windows(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active];
        let mut close = Vec::new();
        for job in &mut tab.tools.walks {
            while let Ok(m) = job.rx.try_recv() {
                match m {
                    WalkMsg::Entry(e) => job.entries.push(e),
                    WalkMsg::Error(e) => job.errors.push(e),
                    WalkMsg::Done(r) => {
                        if let Some(o) = &job.tree {
                            job.text = browsing::tree_text(&job.root, &job.entries, o);
                        }
                        job.done = Some(r);
                    }
                }
            }
            let mut open = true;
            let title = if job.tree.is_some() { t!("ops.tree_title") } else { t!("ops.space_title") };
            egui::Window::new(format!("{title} — {}", job.root))
                .id(egui::Id::new(("walk", tab.id, job.id)))
                .default_size([520.0, 360.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    let (total, rows) = browsing::space_used(&job.entries, 1);
                    match &job.done {
                        None => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(t!("ops.walking", n = job.entries.len()));
                                if ui.button(t!("ops.stop")).clicked() {
                                    job.stop.store(true, Ordering::SeqCst);
                                }
                            });
                        }
                        Some(Err(e)) => {
                            ui.colored_label(theme::colors(ui).error, format!("⚠ {e}"));
                        }
                        Some(Ok(())) => {}
                    }
                    ui.label(
                        RichText::new(t!("ops.space_total", files = total.files, folders = total.folders, size = human_size(total.bytes)))
                            .strong(),
                    );
                    for e in &job.errors {
                        ui.colored_label(theme::colors(ui).error, e);
                    }
                    ui.separator();
                    if job.tree.is_some() {
                        if job.done.is_some() {
                            ui.horizontal(|ui| {
                                if ui.button(t!("ops.copy")).clicked() {
                                    ui.ctx().copy_text(job.text.clone());
                                }
                                if ui.button(t!("ops.save")).clicked()
                                    && let Some(p) =
                                        rfd::FileDialog::new().set_file_name("tree.txt").add_filter("Text", &["txt"]).save_file()
                                    && let Err(e) = std::fs::write(&p, &job.text)
                                {
                                    job.errors.push(format!("{}: {e}", p.display()));
                                }
                            });
                            egui::ScrollArea::both().show(ui, |ui| {
                                let mut shown = job.text.as_str();
                                ui.add(egui::TextEdit::multiline(&mut shown).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
                            });
                        }
                    } else {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            egui::Grid::new(("space-rows", job.id)).num_columns(4).striped(true).show(ui, |ui| {
                                for k in ["ops.col_folder", "ops.col_files", "ops.col_folders", "files.col_size"] {
                                    ui.label(RichText::new(urftp_i18n::text(k)).strong());
                                }
                                ui.end_row();
                                for r in &rows {
                                    ui.label(&r.path);
                                    ui.label(r.files.to_string());
                                    ui.label(r.folders.to_string());
                                    ui.label(human_size(r.bytes));
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
            if !open {
                job.stop.store(true, Ordering::SeqCst);
                close.push(job.id);
            }
        }
        tab.tools.walks.retain(|j| !close.contains(&j.id));
    }

    /// Starts "Find files at sites" with the form of the window.
    fn start_find(&mut self, ctx: &egui::Context) {
        let tab = self.active;
        let Some(w) = &self.tabs[tab].tools.find else { return };
        let date = |s: &str| if s.trim().is_empty() { Some(None) } else { browsing::parse_datetime(s).map(Some) };
        let kib = |s: &str| if s.trim().is_empty() { Some(None) } else { s.trim().parse::<u64>().ok().map(|k| Some(k * 1024)) };
        let (Some(newer), Some(older), Some(min), Some(max), Ok(depth)) = (
            date(&w.newer),
            date(&w.older),
            kib(&w.min_kib),
            kib(&w.max_kib),
            w.depth.trim().parse::<usize>().or(if w.depth.trim().is_empty() { Ok(0) } else { Err(()) }),
        ) else {
            if let Some(w) = &mut self.tabs[tab].tools.find {
                w.error = Some(t!("ops.find_bad_form"));
            }
            return;
        };
        let cond = Conditions { newer_than: newer, older_than: older, dates_on_folders: false, min_size: min, max_size: max };
        let profiles: Vec<(String, SiteProfile)> = w
            .sites
            .iter()
            .filter(|(_, _, on)| *on)
            .filter_map(|(id, _, _)| self.sites.book.find(*id))
            .map(|s| (s.host.clone(), self.site_profile(s)))
            .collect();
        let (root, include, exclude, kind) = (
            if w.path.trim().is_empty() { "/".to_string() } else { w.path.trim().to_string() },
            w.include.clone(),
            w.exclude.clone(),
            w.kind,
        );
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let (stop2, ctx2) = (stop.clone(), ctx.clone());
        self.rt.spawn(async move {
            let send = |m: FindMsg| {
                let _ = tx.send(m);
                ctx2.request_repaint();
            };
            for (host, p) in profiles {
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                send(FindMsg::Site(p.name.clone()));
                match RemoteSession::open(&p, None).await {
                    Ok(mut s) => {
                        let r = s
                            .walk(
                                &root,
                                depth,
                                &stop2,
                                |e| {
                                    let kind_ok = match kind {
                                        FindKind::Both => true,
                                        FindKind::Folders => e.is_dir,
                                        FindKind::Files => !e.is_dir,
                                    };
                                    let name = e.name().to_string();
                                    let entry =
                                        FolderEntry { name: name.clone(), is_dir: e.is_dir, size: Some(e.size), modified: e.modified };
                                    if kind_ok
                                        && words_match(&include, &name) != Some(false)
                                        && words_match(&exclude, &name) != Some(true)
                                        && cond.allows(&entry)
                                    {
                                        send(FindMsg::Found(Found {
                                            site: p.name.clone(),
                                            host: host.clone(),
                                            path: rpath::join(&root, &e.path),
                                            is_dir: e.is_dir,
                                            size: e.size,
                                            modified: e.modified,
                                        }));
                                    }
                                },
                                |d, e| send(FindMsg::Error(format!("{}: {d}: {e}", p.name))),
                            )
                            .await;
                        if let Err(e) = r {
                            send(FindMsg::Error(format!("{}: {e}", p.name)));
                        }
                        s.quit().await;
                    }
                    Err(e) => send(FindMsg::Error(format!("{}: {e}", p.name))),
                }
            }
            send(FindMsg::Done);
        });
        if let Some(w) = &mut self.tabs[tab].tools.find {
            w.error = None;
            w.results.clear();
            w.selected.clear();
            w.status = t!("ops.find_running");
            w.running = Some((rx, stop));
        }
    }

    fn find_window(&mut self, ctx: &egui::Context) {
        let mut start = false;
        let mut queue: Vec<(Found, bool)> = Vec::new();
        let tab = &mut self.tabs[self.active];
        let mut errors = Vec::new();
        let Some(w) = &mut tab.tools.find else { return };
        if let Some((rx, _)) = &w.running {
            let mut finished = false;
            while let Ok(m) = rx.try_recv() {
                match m {
                    FindMsg::Site(s) => w.status = t!("ops.find_site", site = s),
                    FindMsg::Found(f) => w.results.push(f),
                    FindMsg::Error(e) => errors.push(e),
                    FindMsg::Done => finished = true,
                }
            }
            if finished {
                w.running = None;
                w.status = t!("ops.find_done", n = w.results.len());
            }
        }
        let mut open = true;
        egui::Window::new(t!("ops.find_title")).id(egui::Id::new(("find", tab.id))).default_size([720.0, 520.0]).open(&mut open).show(
            ctx,
            |ui| {
                egui::Grid::new("find-form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    theme::grid_row(ui, t!("ops.find_for"), theme::single_line(&mut w.include).desired_width(320.0));
                    theme::grid_row(ui, t!("ops.find_exclude"), theme::single_line(&mut w.exclude).desired_width(320.0));
                    theme::grid_row(ui, t!("ops.find_in"), theme::single_line(&mut w.path).desired_width(320.0));
                    theme::grid_row(ui, t!("ops.walk_depth"), theme::single_line(&mut w.depth).desired_width(60.0));
                    ui.label(t!("ops.find_kind"));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut w.kind, FindKind::Both, t!("ops.find_both"));
                        ui.radio_value(&mut w.kind, FindKind::Folders, t!("ops.find_folders"));
                        ui.radio_value(&mut w.kind, FindKind::Files, t!("ops.find_files"));
                    });
                    ui.end_row();
                    theme::grid_row(
                        ui,
                        t!("ops.find_newer"),
                        theme::single_line(&mut w.newer).hint_text("YYYY-MM-DD").desired_width(140.0),
                    );
                    theme::grid_row(
                        ui,
                        t!("ops.find_older"),
                        theme::single_line(&mut w.older).hint_text("YYYY-MM-DD").desired_width(140.0),
                    );
                    theme::grid_row(ui, t!("ops.find_min"), theme::single_line(&mut w.min_kib).desired_width(80.0));
                    theme::grid_row(ui, t!("ops.find_max"), theme::single_line(&mut w.max_kib).desired_width(80.0));
                });
                ui.label(RichText::new(t!("ops.find_hint")).weak());
                ui.collapsing(t!("ops.find_sites", n = w.sites.iter().filter(|s| s.2).count()), |ui| {
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        for (_, name, on) in &mut w.sites {
                            ui.checkbox(on, name.as_str());
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let running = w.running.is_some();
                    if ui.add_enabled(!running && w.sites.iter().any(|s| s.2), egui::Button::new(t!("ops.find_start"))).clicked() {
                        start = true;
                    }
                    if ui.add_enabled(running, egui::Button::new(t!("ops.stop"))).clicked()
                        && let Some((_, stop)) = &w.running
                    {
                        stop.store(true, Ordering::SeqCst);
                    }
                    if running {
                        ui.spinner();
                    }
                    ui.label(&w.status);
                });
                if let Some(e) = &w.error {
                    ui.colored_label(theme::colors(ui).error, format!("⚠ {e}"));
                }
                ui.separator();
                let results = w.results.clone();
                let mut click: Option<(usize, bool)> = None;
                TableBuilder::new(ui)
                    .id_salt("find-results")
                    .striped(true)
                    .sense(Sense::click())
                    .column(Column::initial(110.0))
                    .column(Column::remainder().at_least(200.0).clip(true))
                    .column(Column::initial(80.0))
                    .column(Column::initial(130.0))
                    .header(20.0, |mut h| {
                        for k in ["ops.col_site", "ops.col_path", "files.col_size", "files.col_modified"] {
                            h.col(|ui| {
                                ui.label(RichText::new(urftp_i18n::text(k)).strong());
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(20.0, results.len(), |mut row| {
                            let i = row.index();
                            let f = &results[i];
                            row.set_selected(w.selected.contains(&i));
                            row.col(|ui| {
                                ui.label(&f.site);
                            });
                            row.col(|ui| {
                                ui.label(format!("{} {}", if f.is_dir { "📁" } else { "📄" }, f.path));
                            });
                            row.col(|ui| {
                                ui.label(if f.is_dir { String::new() } else { human_size(f.size) });
                            });
                            row.col(|ui| {
                                ui.label(RichText::new(f.modified.map(browsing::format_datetime).unwrap_or_default()).weak());
                            });
                            let resp = row.response();
                            if resp.clicked() || (resp.secondary_clicked() && !w.selected.contains(&i)) {
                                click = Some((i, resp.ctx.input(|i| i.modifiers.command)));
                            }
                            resp.context_menu(|ui| {
                                let picked: Vec<Found> = w.selected.iter().filter_map(|&k| results.get(k).cloned()).collect();
                                let picked = if picked.is_empty() { vec![f.clone()] } else { picked };
                                if ui.button(t!("ops.find_queue")).clicked() {
                                    queue.extend(picked.iter().cloned().map(|f| (f, false)));
                                    ui.close();
                                }
                                if ui.button(t!("ops.find_queue_parent")).clicked() {
                                    queue.extend(picked.iter().cloned().map(|f| (f, true)));
                                    ui.close();
                                }
                                if ui.button(t!("ops.find_copy_url")).clicked() {
                                    ui.ctx().copy_text(picked.iter().map(url_of).collect::<Vec<_>>().join("\r\n"));
                                    ui.close();
                                }
                            });
                        });
                    });
                if let Some((i, toggle)) = click {
                    if toggle {
                        if !w.selected.remove(&i) {
                            w.selected.insert(i);
                        }
                    } else {
                        w.selected = BTreeSet::from([i]);
                    }
                }
            },
        );
        if !open {
            if let Some((_, stop)) = &w.running {
                stop.store(true, Ordering::SeqCst);
            }
            tab.tools.find = None;
        }
        for e in errors {
            tab.push_log(0, LogKind::Error, e);
        }
        if !queue.is_empty() {
            // Into the local folder of the tab (the other panel when one is local).
            let local =
                tab.sides.iter().find_map(|s| if let Side::Local(l) = s { Some(l.path.clone()) } else { None }).unwrap_or_else(home_dir);
            let mut q = tab.queue.lock();
            let mut seen = BTreeSet::new();
            for (f, parent) in queue {
                let (path, is_dir, size) = if parent {
                    (rpath::split(&f.path).0.unwrap_or("/").to_string(), true, None)
                } else {
                    (f.path.clone(), f.is_dir, (!f.is_dir).then_some(f.size))
                };
                if !seen.insert((f.site.clone(), path.clone())) {
                    continue;
                }
                let name = rpath::split(&path).1.to_string();
                q.push(Endpoint::remote(f.site.clone(), path), Endpoint::local(local.join(&name).to_string_lossy()), is_dir, size, None);
            }
            let n = seen.len();
            drop(q);
            tab.push_log(0, LogKind::Status, t!("log.items_queued", n = n));
        }
        if start {
            self.start_find(ctx);
        }
    }

    fn edits_window(&mut self, ctx: &egui::Context) {
        let rt = self.rt.clone();
        let tab = &mut self.tabs[self.active];
        if !tab.tools.edits_open {
            return;
        }
        let mut open = true;
        let (mut remove, mut upload) = (None, None);
        egui::Window::new(t!("ops.edits_title")).id(egui::Id::new(("edits", tab.id))).default_size([560.0, 240.0]).open(&mut open).show(
            ctx,
            |ui| {
                if tab.tools.edits.is_empty() {
                    ui.label(RichText::new(t!("ops.edits_none")).italics());
                }
                egui::Grid::new("edits").num_columns(4).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                    for (i, e) in tab.tools.edits.iter().enumerate() {
                        ui.label(format!("{}:{}", e.profile.name, e.remote));
                        ui.label(match &e.state {
                            EditState::Downloading => t!("ops.edit_downloading"),
                            EditState::Viewing => t!("ops.edit_viewing"),
                            EditState::Watching => t!("ops.edit_watching"),
                            EditState::Uploading => t!("ops.edit_uploading"),
                            EditState::Failed(err) => t!("ops.edit_failed", error = err),
                        });
                        if ui.add_enabled(e.edit && e.rx.is_none(), egui::Button::new(t!("ops.edit_upload_now"))).clicked() {
                            upload = Some(i);
                        }
                        if ui.button(t!("ops.edit_forget")).clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                });
            },
        );
        if let Some(i) = upload {
            tab.upload_edit(i, &rt, ctx);
        }
        if let Some(i) = remove {
            tab.tools.edits.remove(i);
        }
        if !open {
            tab.tools.edits_open = false;
        }
    }
}

/// Raw listing received for panel `side`.
pub(super) fn raw_listing(tab: &mut Container, side: usize, dir: String, text: String) {
    let site = tab.sides[side].profile().map(|p| p.name.clone()).unwrap_or_default();
    tab.tools.raw = Some((site, dir, text));
}

// ====================================================================== menus and bars

/// Quick filter bar of a panel (`Ctrl+F`).
pub(super) fn filter_bar(ui: &mut egui::Ui, side: &mut Side) {
    let shown = side.rows().len();
    let mut p = side.parts();
    if !p.list.filter_open {
        return;
    }
    let mut changed = false;
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label("🔍");
        let r = ui
            .add(theme::single_line(&mut p.list.filter_text).hint_text(t!("ops.filter_hint")).desired_width(ui.available_width() - 220.0));
        theme::name(&r, t!("ops.filter"));
        if std::mem::take(&mut p.list.filter_focus) {
            r.request_focus();
        }
        changed |= r.changed();
        if (r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        changed |= ui.checkbox(&mut p.list.filter_regex, t!("ops.filter_regex")).changed();
        if let Some((n, all)) = p.list.counts(shown) {
            ui.label(RichText::new(t!("ops.filter_count", n = n, all = all)).weak());
        }
        if theme::icon_button(ui, "✕", t!("ops.filter_close")).clicked() {
            close = true;
        }
    });
    if let Some(e) = &p.list.filter_error {
        ui.colored_label(theme::colors(ui).error, format!("⚠ {e}"));
    }
    if close {
        p.list.clear_filter();
        p.show();
    } else if changed {
        match QuickFilter::parse(&p.list.filter_text, p.list.filter_regex) {
            Ok(f) => {
                p.list.filter = f;
                p.list.filter_error = None;
                p.show();
            }
            Err(e) => p.list.filter_error = Some(e.to_string()),
        }
    }
}

/// Bookmarks of the site of a remote panel; `Shift` also opens their local folder.
pub(super) fn bookmarks_menu(ui: &mut egui::Ui, side: usize, names: &[String], actions: &mut Vec<Action>) {
    if names.is_empty() {
        return;
    }
    ui.menu_button(t!("conn.bookmarks"), |ui| {
        for n in names {
            if ui.button(n).on_hover_text(t!("conn.bookmark_hint")).clicked() {
                let both = ui.input(|i| i.modifiers.shift);
                actions.push(Action::Op(side, Op::Bookmark(n.clone(), both)));
                ui.close();
            }
        }
    });
}

fn op_button(ui: &mut egui::Ui, enabled: bool, label: String, side: usize, op: Op, actions: &mut Vec<Action>) {
    if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
        actions.push(Action::Op(side, op));
        ui.close();
    }
}

/// "Edit" menu of the main window, for the panel that has the keyboard.
pub(super) fn edit_menu(ui: &mut egui::Ui, tab: &Container, actions: &mut Vec<Action>) {
    let side = tab.focus;
    let any = !tab.sides[side].rows().is_empty();
    op_button(ui, true, t!("ops.m_filter"), side, Op::Filter, actions);
    ui.separator();
    if ui.button(t!("ops.m_select_all")).clicked() {
        actions.push(Action::SelectAll(side));
        ui.close();
    }
    op_button(ui, any, t!("ops.m_select_mask"), side, Op::SelectMask(true), actions);
    op_button(ui, any, t!("ops.m_unselect_mask"), side, Op::SelectMask(false), actions);
    op_button(ui, any, t!("ops.m_copy_names"), side, Op::CopyNames, actions);
    ui.menu_button(t!("ops.m_marks"), |ui| {
        op_button(ui, any, t!("ops.m_mark"), side, Op::Mark, actions);
        op_button(ui, any, t!("ops.m_unmark_all"), side, Op::UnmarkAll, actions);
        op_button(ui, any, t!("ops.m_select_marked"), side, Op::SelectMarked, actions);
        op_button(ui, any, t!("ops.m_export_marks"), side, Op::ExportMarks, actions);
    });
    ui.separator();
    op_button(ui, any, t!("ops.m_rename_many"), side, Op::RenameMany, actions);
    op_button(ui, any, t!("ops.m_move"), side, Op::MoveTo, actions);
    op_button(ui, tab.sides[side].is_remote(), t!("ops.m_paste"), side, Op::Paste, actions);
}

/// "FTP" menu of the main window: operations on the remote panel that has the keyboard.
pub(super) fn ftp_menu(ui: &mut egui::Ui, tab: &mut Container, actions: &mut Vec<Action>) {
    let side = tab.focus;
    let remote = tab.sides[side].is_remote();
    let any = remote && !tab.sides[side].rows().is_empty();
    op_button(ui, any, t!("ops.m_chmod"), side, Op::Chmod, actions);
    op_button(ui, any, t!("ops.m_date"), side, Op::SetDate, actions);
    op_button(ui, remote, t!("ops.m_raw"), side, Op::RawList, actions);
    ui.separator();
    op_button(ui, true, t!("ops.m_space"), side, Op::SpaceUsed, actions);
    op_button(ui, true, t!("ops.m_tree"), side, Op::ExportTree, actions);
    op_button(ui, true, t!("ops.m_find"), side, Op::FindFiles, actions);
    ui.separator();
    ui.menu_button(t!("ops.m_compare"), |ui| {
        let c = &mut tab.tools.compare;
        let mut size = c.by == CompareBy::NameSize;
        if ui.radio(!size, t!("ops.compare_name")).clicked() {
            size = false;
        }
        if ui.radio(size, t!("ops.compare_size")).clicked() {
            size = true;
        }
        c.by = if size { CompareBy::NameSize } else { CompareBy::Name };
        ui.checkbox(&mut c.partial, t!("ops.compare_partial"));
        ui.checkbox(&mut c.hide_same, t!("ops.compare_hide"));
        ui.checkbox(&mut c.select, t!("ops.compare_select"));
        ui.separator();
        op_button(ui, true, t!("ops.compare_run"), side, Op::Compare, actions);
    });
    ui.separator();
    op_button(ui, remote, t!("ops.m_clear_cache"), side, Op::ClearCache, actions);
    op_button(ui, true, t!("ops.m_clear_caches"), side, Op::ClearAllCaches, actions);
    op_button(ui, true, t!("ops.m_edits"), side, Op::CurrentEdits, actions);
}

/// Entries of the right-click menu of a file list added by the operations.
pub(super) fn context_menu(ui: &mut egui::Ui, side: usize, remote: bool, row: &FileRow, menu: &mut Vec<Action>) {
    if remote && !row.is_dir {
        op_button(ui, true, t!("ops.ctx_view"), side, Op::View(row.name.clone()), menu);
        op_button(ui, true, t!("ops.ctx_edit"), side, Op::Edit(row.name.clone()), menu);
    }
    op_button(ui, true, t!("ops.m_mark"), side, Op::Mark, menu);
    op_button(ui, true, t!("ops.m_copy_names"), side, Op::CopyNames, menu);
    op_button(ui, true, t!("ops.m_rename_many"), side, Op::RenameMany, menu);
    op_button(ui, true, t!("ops.m_move"), side, Op::MoveTo, menu);
    if remote {
        op_button(ui, true, t!("ops.m_chmod"), side, Op::Chmod, menu);
        op_button(ui, true, t!("ops.m_date"), side, Op::SetDate, menu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_words() {
        assert_eq!(words_match("", "a"), None);
        assert_eq!(words_match("rel; *.nfo", "x.NFO"), Some(true));
        assert_eq!(words_match("rel", "Some.Release"), Some(true));
        assert_eq!(words_match("zip", "a.rar"), Some(false));
    }

    #[test]
    fn programs_never_default_to_the_file_type() {
        let list = vec![urftp_core::settings::FileAssociation { masks: "*.nfo".into(), program: "viewer.exe".into() }];
        assert_eq!(program(&list, "a.NFO"), "viewer.exe");
        assert_eq!(program(&list, "setup.exe"), "notepad.exe");
    }
}
