//! Fenêtre principale : onglets de sites (containers), chacun avec son dock (deux panneaux
//! de fichiers, file d'attente, journal), barre de connexion rapide et barre d'état.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{self, Color32, RichText, Sense};
use egui_dock::{DockArea, DockState, NodeIndex, Style as DockStyle, TabViewer};
use egui_extras::{Column, TableBuilder};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use urftp_core::queue::{Endpoint, ItemId, ItemState, Queue, SiteLimits};
use urftp_proto::engine::{Engine, EngineEvent, EngineOptions};
use urftp_proto::ftp::{FtpConfig, FtpTime, LogLine, RemoteEntry, Security, TlsOptions, TlsVerify, path as rpath};
use urftp_proto::session::{SiteProfile, SiteProtocol};
use urftp_proto::sftp::{HostKeyPolicy, SftpConfig};

use crate::browser::{self, BrowserCmd, BrowserEvent, BrowserHandle, TrustKind};
use crate::sites::{self, SiteManager};
use urftp_core::site::Site;
use crate::theme::{self, ACCENT, LOG_COMMAND, LOG_ERROR, LOG_REPLY, LOG_STATUS, human_size};

// ====================================================================== modèle

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Left,
    Right,
    Queue,
    Log,
}

#[derive(Clone)]
struct FileRow {
    name: String,
    is_dir: bool,
    size: u64,
    modified: String,
    /// Clé de tri de la date.
    mtime: i64,
}

impl FileRow {
    fn from_remote(e: &RemoteEntry) -> Self {
        Self {
            name: e.name.clone(),
            is_dir: e.is_dir(),
            size: e.size,
            modified: e.modified.map(fmt_time).unwrap_or_default(),
            mtime: e.modified.map(time_key).unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortCol {
    Name,
    Size,
    Modified,
}

/// Tri d'un panneau ; les dossiers restent toujours en tête.
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
    /// Clic sur une colonne : même colonne = sens inverse, autre colonne = croissant.
    fn toggle(self, col: SortCol) -> Self {
        if self.col == col { Self { col, asc: !self.asc } } else { Self { col, asc: true } }
    }
}

/// Date sous forme de nombre comparable (année inconnue = récente).
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

fn fmt_time(t: FtpTime) -> String {
    match t.year {
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

struct LocalView {
    path: PathBuf,
    path_edit: String,
    rows: Vec<FileRow>,
    selected: BTreeSet<usize>,
    error: Option<String>,
    sort: Sort,
}

impl LocalView {
    fn new(path: PathBuf) -> Self {
        let mut v = Self {
            path_edit: path.to_string_lossy().into_owned(),
            path,
            rows: Vec::new(),
            selected: BTreeSet::new(),
            error: None,
            sort: Sort::default(),
        };
        v.reload();
        v
    }

    fn reload(&mut self) {
        self.selected.clear();
        self.path_edit = self.path.to_string_lossy().into_owned();
        match std::fs::read_dir(&self.path) {
            Ok(rd) => {
                let mut rows: Vec<FileRow> = rd
                    .flatten()
                    .filter_map(|e| {
                        let meta = e.metadata().ok()?;
                        let t = meta
                            .modified()
                            .ok()
                            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| urftp_proto::sftp::unix_to_ftptime(d.as_secs() as i64));
                        Some(FileRow {
                            name: e.file_name().to_string_lossy().into_owned(),
                            is_dir: meta.is_dir(),
                            size: meta.len(),
                            modified: t.map(fmt_time).unwrap_or_default(),
                            mtime: t.map(time_key).unwrap_or(0),
                        })
                    })
                    .collect();
                sort_rows(&mut rows, self.sort);
                self.rows = rows;
                self.error = None;
            }
            Err(e) => {
                self.rows.clear();
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
    rows: Vec<FileRow>,
    selected: BTreeSet<usize>,
    state: RemoteState,
    /// Fiche du gestionnaire de sites d'origine (pour y mémoriser une empreinte approuvée).
    site_id: Option<u64>,
    sort: Sort,
}

enum Side {
    Local(LocalView),
    Remote(Box<RemoteView>),
}

impl Side {
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
        match self {
            Side::Local(l) => &mut l.selected,
            Side::Remote(r) => &mut r.selected,
        }
    }

    /// Extrémité de transfert pour un nom de ce panneau.
    fn endpoint(&self, name: &str) -> Endpoint {
        match self {
            Side::Local(l) => Endpoint::local(l.path.join(name).to_string_lossy()),
            Side::Remote(r) => Endpoint::remote(r.profile.name.clone(), rpath::join(&r.dir, name)),
        }
    }

    fn title(&self) -> String {
        match self {
            Side::Local(_) => "💻 Local".into(),
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

    /// Change le tri et réordonne la liste (la sélection est vidée).
    fn set_sort(&mut self, s: Sort) {
        let (rows, sort, sel) = match self {
            Side::Local(l) => (&mut l.rows, &mut l.sort, &mut l.selected),
            Side::Remote(r) => (&mut r.rows, &mut r.sort, &mut r.selected),
        };
        *sort = s;
        sort_rows(rows, s);
        sel.clear();
    }

    /// Chemin complet d'un nom (affichage, presse-papiers, opérations).
    fn full_path(&self, name: &str) -> String {
        match self {
            Side::Local(l) => l.path.join(name).to_string_lossy().into_owned(),
            Side::Remote(r) => rpath::join(&r.dir, name),
        }
    }

    /// Éléments sélectionnés : (nom, dossier ?).
    fn picked(&self) -> Vec<(String, bool)> {
        self.selected().iter().filter_map(|&i| self.rows().get(i)).map(|r| (r.name.clone(), r.is_dir)).collect()
    }
}

/// Glisser-déposer : fichiers sélectionnés d'un panneau.
#[derive(Debug, Clone, Copy)]
struct DragFiles {
    from: usize,
}

/// Dialogue en attente de saisie ou de confirmation.
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
}

struct TrustPrompt {
    side: usize,
    kind: TrustKind,
    fingerprint: String,
    reason: String,
}

/// Un onglet de site (« container » de FTPRush).
struct Container {
    id: u32,
    dock: DockState<Pane>,
    sides: [Side; 2],
    queue: Arc<Mutex<Queue>>,
    progress: HashMap<ItemId, (u64, Option<u64>)>,
    engine_events: Option<UnboundedReceiver<EngineEvent>>,
    engine_running: Arc<AtomicBool>,
    log: Vec<LogEntry>,
    trust: Option<TrustPrompt>,
    /// Moteur en cours (pour arrêter un élément).
    engine: Option<Arc<Engine>>,
    prompt: Option<Prompt>,
    /// Panneau actif pour le clavier (0 = gauche, 1 = droit).
    focus: usize,
}

fn default_dock() -> DockState<Pane> {
    let mut dock = DockState::new(vec![Pane::Left]);
    let surface = dock.main_surface_mut();
    let [top, _] = surface.split_below(NodeIndex::root(), 0.66, vec![Pane::Queue, Pane::Log]);
    surface.split_right(top, 0.5, vec![Pane::Right]);
    dock
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\"))
}

impl Container {
    fn new(id: u32) -> Self {
        Self {
            id,
            dock: default_dock(),
            sides: [Side::Local(LocalView::new(home_dir())), Side::Local(LocalView::new(home_dir()))],
            queue: Arc::new(Mutex::new(Queue::new())),
            progress: HashMap::new(),
            engine_events: None,
            engine_running: Arc::new(AtomicBool::new(false)),
            log: Vec::new(),
            trust: None,
            engine: None,
            prompt: None,
            focus: 0,
        }
    }

    /// Titre affiché ; `pos` = position de l'onglet (1, 2, 3…), qui se renumérote à la fermeture.
    fn title(&self, pos: usize) -> String {
        match (&self.sides[0], &self.sides[1]) {
            (_, Side::Remote(r)) | (Side::Remote(r), _) => r.profile.host.clone(),
            _ => format!("Onglet {pos}"),
        }
    }

    fn push_log(&mut self, side: usize, kind: LogKind, text: impl Into<String>) {
        self.log.push(LogEntry { side, kind, text: text.into() });
        if self.log.len() > 5000 {
            self.log.drain(..1000);
        }
    }

    /// Relève les événements des connexions de navigation et du moteur.
    fn poll(&mut self) {
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
                    self.progress.insert(id, (bytes, total));
                }
                EngineEvent::Failed { id, error } => self.push_log(0, LogKind::Error, format!("Élément {id} : {error}")),
                EngineEvent::Retrying { id, attempt, error } => {
                    self.push_log(0, LogKind::Status, format!("Élément {id} : nouvelle tentative {attempt} ({error})"))
                }
                _ => {}
            }
        }
        if self.engine_events.is_some() && !self.engine_running.load(Ordering::SeqCst) {
            self.engine_events = None;
            self.engine = None;
            self.push_log(0, LogKind::Status, "File d'attente terminée");
            self.refresh_both();
        }
    }

    fn on_browser_event(&mut self, side: usize, e: BrowserEvent) {
        match e {
            BrowserEvent::Listing { dir, entries } => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.dir = dir;
                    r.rows = entries.iter().map(FileRow::from_remote).collect();
                    sort_rows(&mut r.rows, r.sort);
                    r.selected.clear();
                    r.state = RemoteState::Connected;
                }
            }
            BrowserEvent::Log(line) => {
                let (kind, text) = match line {
                    LogLine::Command(c) => (LogKind::Command, c),
                    LogLine::Reply(r) => (if r.code >= 400 { LogKind::Error } else { LogKind::Reply }, r.lines.join("\n")),
                    LogLine::Status(s) => (LogKind::Status, s),
                };
                self.push_log(side, kind, text);
            }
            BrowserEvent::Error(msg) => {
                if let Side::Remote(r) = &mut self.sides[side]
                    && r.state == RemoteState::Connecting
                {
                    r.state = RemoteState::Failed(msg.clone());
                }
                self.push_log(side, LogKind::Error, msg);
            }
            BrowserEvent::Trust { kind, fingerprint, reason } => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.state = RemoteState::Failed("serveur non approuvé".into());
                }
                self.push_log(side, LogKind::Error, format!("{reason} — empreinte {fingerprint}"));
                self.trust = Some(TrustPrompt { side, kind, fingerprint, reason });
            }
            BrowserEvent::Disconnected => {
                if let Side::Remote(r) = &mut self.sides[side] {
                    r.state = RemoteState::Disconnected;
                    r.rows.clear();
                }
            }
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

    /// Ajoute la sélection du panneau `from` à la file, vers le dossier de l'autre panneau.
    fn enqueue_selection(&mut self, from: usize) {
        let to = 1 - from;
        let picks: Vec<FileRow> = self.sides[from].selected().iter().filter_map(|&i| self.sides[from].rows().get(i).cloned()).collect();
        if picks.is_empty() {
            return;
        }
        if self.sides[from].profile().is_none() && self.sides[to].profile().is_none() {
            self.push_log(from, LogKind::Error, "Transfert local → local non géré : connectez un site d'un côté");
            return;
        }
        let mut q = self.queue.lock().unwrap();
        for row in &picks {
            let src = self.sides[from].endpoint(&row.name);
            let dst = self.sides[to].endpoint(&row.name);
            q.push(src, dst, row.is_dir, (!row.is_dir).then_some(row.size), None);
        }
        drop(q);
        self.push_log(from, LogKind::Status, format!("{} élément(s) ajouté(s) à la file", picks.len()));
    }

    fn start_queue(&mut self, rt: &Handle, ctx: &egui::Context) {
        if self.engine_events.is_some() {
            return;
        }
        let mut sites: Vec<SiteProfile> = Vec::new();
        for p in self.sides.iter().filter_map(Side::profile) {
            if !sites.iter().any(|s| s.name == p.name) {
                sites.push(p.clone());
            }
        }
        let (tx, rx) = unbounded_channel();
        let engine = Engine::new(self.queue.clone(), sites, EngineOptions::default(), Some(tx));
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
        self.push_log(0, LogKind::Status, "File d'attente démarrée");
    }

    // ------------------------------------------------------------------ opérations sur les fichiers

    fn ask_new_folder(&mut self, side: usize) {
        self.prompt = Some(Prompt::NewFolder { side, name: "Nouveau dossier".into() });
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

    /// Exécute un dialogue validé (local : tout de suite ; distant : via la connexion du panneau).
    fn run_prompt(&mut self, p: Prompt) {
        match p {
            Prompt::NewFolder { side, name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return;
                }
                let full = self.sides[side].full_path(&name);
                if let Side::Remote(r) = &self.sides[side] {
                    r.handle.send(BrowserCmd::Mkdir(full));
                } else {
                    match std::fs::create_dir(&full) {
                        Ok(()) => {
                            if let Side::Local(l) = &mut self.sides[side] {
                                l.reload();
                            }
                        }
                        Err(e) => self.push_log(side, LogKind::Error, format!("Création de {full} : {e}")),
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
                            Err(e) => self.push_log(side, LogKind::Error, format!("Renommage de {full} : {e}")),
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
                                self.push_log(side, LogKind::Error, format!("Suppression de {p} : {e}"));
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

    // ------------------------------------------------------------------ file d'attente

    fn move_item(&mut self, id: ItemId, m: Move) {
        let mut q = self.queue.lock().unwrap();
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

    fn retry_item(&mut self, id: ItemId) {
        if let Some(it) = self.queue.lock().unwrap().get_mut(id)
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
    fn label(self) -> &'static str {
        match self {
            QuickProto::Ftp => "FTP",
            QuickProto::FtpsExplicit => "FTPS explicite",
            QuickProto::FtpsImplicit => "FTPS implicite",
            QuickProto::Sftp => "SFTP",
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
    tabs: Vec<Container>,
    active: usize,
    next_id: u32,
    quick: QuickConnect,
    dark: bool,
    /// Connexion automatique au premier affichage (variable `URUSHFTP_CONNECT`).
    autoconnect: bool,
    sites: SiteManager,
}

/// `ftp://user:pass@hôte:port`, `ftps://` (explicite), `ftpsi://` (implicite), `sftp://`.
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

enum Action {
    Open(usize, String),
    Up(usize),
    Refresh(usize),
    Disconnect(usize),
    Transfer(usize),
    StartQueue,
    ClearFinished,
    Focus(usize),
    Sort(usize, SortCol),
    NewFolder(usize),
    Rename(usize),
    Delete(usize),
    CopyPath(usize, String),
    QueueStop(ItemId),
    QueueStopAll,
    QueueMove(ItemId, Move),
    QueueRemove(ItemId),
    QueueRetry(ItemId),
    QueueRetryAll,
}

impl UltraRushApp {
    pub fn new(rt: Handle) -> Self {
        let from_env = std::env::var("URUSHFTP_CONNECT").ok().and_then(|u| parse_connect_url(&u));
        Self {
            rt,
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
            dark: theme::start_dark(),
            sites: SiteManager::load(),
        }
    }

    fn new_tab(&mut self) {
        self.tabs.push(Container::new(self.next_id));
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
    }

    fn quick_profile(&self) -> Option<SiteProfile> {
        let q = &self.quick;
        let host = q.host.trim();
        if host.is_empty() {
            return None;
        }
        let port = q.port.trim().parse().unwrap_or(q.proto.default_port());
        let user = if q.user.trim().is_empty() { "anonymous".to_string() } else { q.user.trim().to_string() };
        let tls = TlsOptions { verify: TlsVerify::System, ..TlsOptions::default() };
        let protocol = match q.proto {
            QuickProto::Ftp => SiteProtocol::Ftp(FtpConfig::default()),
            QuickProto::FtpsExplicit => SiteProtocol::Ftp(FtpConfig { security: Security::Explicit(tls), ..FtpConfig::default() }),
            QuickProto::FtpsImplicit => SiteProtocol::Ftp(FtpConfig { security: Security::Implicit(tls), ..FtpConfig::default() }),
            QuickProto::Sftp => SiteProtocol::Sftp { config: SftpConfig::default(), key: None },
        };
        Some(SiteProfile {
            name: format!("{user}@{host}:{port}"),
            host: host.to_string(),
            port,
            user,
            password: q.pass.clone(),
            protocol,
            limits: SiteLimits::default(),
        })
    }

    /// Connecte le panneau droit de l'onglet actif (comme la barre de connexion rapide de la v2).
    fn quick_connect(&mut self, ctx: &egui::Context) {
        let Some(profile) = self.quick_profile() else { return };
        self.connect_profile(profile, None, None, ctx);
    }

    /// Ouvre une connexion dans le panneau droit de l'onglet actif (un nouvel onglet si ce
    /// panneau est déjà connecté), puis se place dans `start_dir`.
    fn connect_profile(&mut self, profile: SiteProfile, site_id: Option<u64>, start_dir: Option<String>, ctx: &egui::Context) {
        if matches!(self.tabs[self.active].sides[1], Side::Remote(_)) {
            self.new_tab();
        }
        let handle = browser::spawn(&self.rt, ctx.clone());
        handle.send(BrowserCmd::Connect(Box::new(profile.clone())));
        if let Some(dir) = start_dir.filter(|d| !d.trim().is_empty()) {
            handle.send(BrowserCmd::Cwd(dir));
        }
        let tab = &mut self.tabs[self.active];
        tab.push_log(1, LogKind::Status, format!("Connexion à {}…", profile.name));
        tab.sides[1] = Side::Remote(Box::new(RemoteView {
            profile,
            handle,
            dir: "/".into(),
            rows: Vec::new(),
            selected: BTreeSet::new(),
            state: RemoteState::Connecting,
            site_id,
            sort: Sort::default(),
        }));
    }

    /// Connexion depuis une fiche du gestionnaire de sites.
    fn connect_site(&mut self, site: Site, ctx: &egui::Context) {
        let profile = sites::to_profile(&site);
        let remote = (!site.remote_dir.is_empty()).then(|| site.remote_dir.clone());
        self.connect_profile(profile, Some(site.id), remote, ctx);
        if !site.local_dir.trim().is_empty()
            && let Side::Local(l) = &mut self.tabs[self.active].sides[0]
        {
            l.path = PathBuf::from(site.local_dir.trim());
            l.reload();
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Fichier", |ui| {
                if ui.button("☰ Gestionnaire de sites").clicked() {
                    self.sites.open = true;
                    ui.close();
                }
                if ui.button("Nouvel onglet").clicked() {
                    self.new_tab();
                    ui.close();
                }
                if ui.button("Quitter").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Affichage", |ui| {
                if ui.button(if self.dark { "☀ Thème clair" } else { "🌙 Thème sombre" }).clicked() {
                    self.dark = !self.dark;
                    theme::apply(ui.ctx(), self.dark);
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new("UltraRushFTP").strong().color(ACCENT));
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("☰ Sites").on_hover_text("Gestionnaire de sites").clicked() {
                self.sites.open = !self.sites.open;
            }
            ui.separator();
            ui.label("⚡");
            let before = self.quick.proto;
            egui::ComboBox::from_id_salt("proto").selected_text(self.quick.proto.label()).width(120.0).show_ui(ui, |ui| {
                for p in [QuickProto::Ftp, QuickProto::FtpsExplicit, QuickProto::FtpsImplicit, QuickProto::Sftp] {
                    ui.selectable_value(&mut self.quick.proto, p, p.label());
                }
            });
            if before != self.quick.proto {
                self.quick.port = self.quick.proto.default_port().to_string();
            }
            ui.add(egui::TextEdit::singleline(&mut self.quick.host).hint_text("Hôte").desired_width(180.0));
            ui.add(egui::TextEdit::singleline(&mut self.quick.port).hint_text("Port").desired_width(48.0));
            ui.add(egui::TextEdit::singleline(&mut self.quick.user).hint_text("Utilisateur").desired_width(120.0));
            let pass = ui.add(egui::TextEdit::singleline(&mut self.quick.pass).hint_text("Mot de passe").password(true).desired_width(120.0));
            let enter = pass.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.add(egui::Button::new(RichText::new("Connexion").color(Color32::WHITE)).fill(ACCENT)).clicked() || enter {
                self.quick_connect(ui.ctx());
            }
        });
        ui.add_space(4.0);
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
                if self.tabs.len() > 1 && ui.small_button("✕").on_hover_text("Fermer l'onglet").clicked() {
                    close = Some(i);
                }
                ui.separator();
            }
            if ui.button("+").on_hover_text("Nouvel onglet").clicked() {
                self.new_tab();
            }
        });
        if let Some(i) = close {
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
            ui.label(format!("Onglets : {}", self.tabs.len()));
            ui.separator();
            ui.label(format!("Connectés : {connected}"));
            ui.separator();
            ui.label(format!("Files actives : {running}"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
            });
        });
    }

    fn apply(&mut self, actions: Vec<Action>, ctx: &egui::Context) {
        let rt = self.rt.clone();
        let tab = &mut self.tabs[self.active];
        for a in actions {
            match a {
                Action::Open(side, name) => match &mut tab.sides[side] {
                    Side::Local(l) => l.open(&name),
                    Side::Remote(r) => r.handle.send(BrowserCmd::Cwd(rpath::join(&r.dir, &name))),
                },
                Action::Up(side) => match &mut tab.sides[side] {
                    Side::Local(l) => l.up(),
                    Side::Remote(r) => r.handle.send(BrowserCmd::Cwd("..".into())),
                },
                Action::Refresh(side) => match &mut tab.sides[side] {
                    Side::Local(l) => l.reload(),
                    Side::Remote(r) => r.handle.send(BrowserCmd::Refresh),
                },
                Action::Disconnect(side) => {
                    if let Side::Remote(r) = &tab.sides[side] {
                        r.handle.send(BrowserCmd::Disconnect);
                    }
                    tab.sides[side] = Side::Local(LocalView::new(home_dir()));
                }
                Action::Transfer(side) => {
                    tab.enqueue_selection(side);
                    tab.start_queue(&rt, ctx);
                }
                Action::StartQueue => tab.start_queue(&rt, ctx),
                Action::ClearFinished => {
                    tab.queue.lock().unwrap().remove_where(|i| i.state.is_finished());
                }
                Action::Focus(side) => tab.focus = side,
                Action::Sort(side, col) => {
                    let s = tab.sides[side].sort().toggle(col);
                    tab.sides[side].set_sort(s);
                }
                Action::NewFolder(side) => tab.ask_new_folder(side),
                Action::Rename(side) => tab.ask_rename(side),
                Action::Delete(side) => tab.ask_delete(side),
                Action::CopyPath(side, name) => ctx.copy_text(tab.sides[side].full_path(&name)),
                Action::QueueStop(id) => tab.stop_item(id),
                Action::QueueStopAll => tab.stop_all(),
                Action::QueueMove(id, m) => tab.move_item(id, m),
                Action::QueueRemove(id) => {
                    tab.queue.lock().unwrap().remove_where(|i| i.id == id);
                }
                Action::QueueRetry(id) => {
                    tab.retry_item(id);
                    tab.start_queue(&rt, ctx);
                }
                Action::QueueRetryAll => {
                    tab.queue.lock().unwrap().reset_failed();
                    tab.start_queue(&rt, ctx);
                }
            }
        }
    }

    /// Raccourcis clavier, appliqués au panneau actif quand aucun champ texte n'a le focus.
    fn keyboard(&mut self, ctx: &egui::Context) -> Vec<Action> {
        let tab = &self.tabs[self.active];
        if ctx.egui_wants_keyboard_input() || tab.prompt.is_some() || tab.trust.is_some() || self.sites.open {
            return Vec::new();
        }
        let side = tab.focus;
        let mut out = Vec::new();
        ctx.input(|i| {
            use egui::Key;
            let first_dir = tab.sides[side].picked().into_iter().find(|(_, d)| *d).map(|(n, _)| n);
            if i.key_pressed(Key::Enter)
                && let Some(n) = first_dir
            {
                out.push(Action::Open(side, n));
            }
            if i.key_pressed(Key::Backspace) {
                out.push(Action::Up(side));
            }
            if i.key_pressed(Key::F5) {
                out.push(Action::Refresh(side));
            }
            if i.key_pressed(Key::F2) {
                out.push(Action::Rename(side));
            }
            if i.key_pressed(Key::F7) {
                out.push(Action::NewFolder(side));
            }
            if i.key_pressed(Key::Delete) {
                out.push(Action::Delete(side));
            }
            if i.key_pressed(Key::Space) || i.key_pressed(Key::Insert) {
                out.push(Action::Transfer(side));
            }
            if i.key_pressed(Key::Tab) {
                out.push(Action::Focus(1 - side));
            }
        });
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
            let n = tab.sides[side].rows().len();
            // Sélection directe : pas besoin de passer par une action.
            let sel: BTreeSet<usize> = (0..n).collect();
            out.push(Action::Focus(side));
            self.tabs[self.active].sides[side].selected_mut().extend(sel);
        }
        out
    }

    /// Dialogues de saisie (nouveau dossier, renommer) et de confirmation (supprimer).
    fn prompt_dialog(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active];
        let Some(prompt) = &mut tab.prompt else { return };
        let mut done: Option<bool> = None;
        egui::Modal::new(egui::Id::new(("prompt", tab.id))).show(ctx, |ui| {
            ui.set_min_width(380.0);
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            match prompt {
                Prompt::NewFolder { name, .. } => {
                    ui.heading("📁 Nouveau dossier");
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                    r.request_focus();
                }
                Prompt::Rename { from, to, .. } => {
                    ui.heading("✏ Renommer");
                    ui.label(RichText::new(from.as_str()).weak());
                    let r = ui.add(egui::TextEdit::singleline(to).desired_width(f32::INFINITY));
                    r.request_focus();
                }
                Prompt::Delete { items, .. } => {
                    ui.heading("🗑 Supprimer ?");
                    let dirs = items.iter().filter(|(_, d)| *d).count();
                    ui.label(format!(
                        "{} élément(s) seront supprimés définitivement{}.",
                        items.len(),
                        if dirs > 0 { format!(", dont {dirs} dossier(s) avec tout leur contenu") } else { String::new() }
                    ));
                    for (n, d) in items.iter().take(8) {
                        ui.label(RichText::new(format!("{} {n}", if *d { "📁" } else { "📄" })).weak());
                    }
                    if items.len() > 8 {
                        ui.label(RichText::new(format!("… et {} autre(s)", items.len() - 8)).weak());
                    }
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let (label, color) = match prompt {
                    Prompt::Delete { .. } => ("Supprimer", LOG_ERROR),
                    _ => ("Valider", ACCENT),
                };
                if ui.add(egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(color)).clicked()
                    || (enter && !matches!(prompt, Prompt::Delete { .. }))
                {
                    done = Some(true);
                }
                if ui.button("Annuler").clicked() || escape {
                    done = Some(false);
                }
            });
        });
        if let Some(ok) = done {
            let p = tab.prompt.take().unwrap();
            if ok {
                tab.run_prompt(p);
            }
        }
    }

    fn trust_dialog(&mut self, ctx: &egui::Context) {
        let tab = &mut self.tabs[self.active];
        let Some(prompt) = &tab.trust else { return };
        let mut decision = None;
        egui::Modal::new(egui::Id::new(("trust", tab.id))).show(ctx, |ui| {
            ui.set_max_width(520.0);
            let what = match prompt.kind {
                TrustKind::TlsCertificate => "Le certificat TLS de ce serveur n'est pas reconnu.",
                TrustKind::SshHostKey => "La clé SSH de ce serveur est inconnue.",
            };
            ui.heading("🔒 Faire confiance à ce serveur ?");
            ui.add_space(6.0);
            ui.label(what);
            ui.label(RichText::new(&prompt.reason).weak());
            ui.add_space(6.0);
            ui.label("Empreinte :");
            ui.code(&prompt.fingerprint);
            ui.add_space(6.0);
            ui.label("Ne l'approuvez que si elle correspond à celle donnée par l'administrateur du site.");
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("Faire confiance et se connecter").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                    decision = Some(true);
                }
                if ui.button("Annuler").clicked() {
                    decision = Some(false);
                }
            });
        });
        let Some(trust) = decision else { return };
        let prompt = tab.trust.take().unwrap();
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
            // Site du gestionnaire : l'empreinte approuvée est enregistrée dans sa fiche.
            if let Some(id) = r.site_id {
                self.sites.remember_trust(id, prompt.kind, &prompt.fingerprint);
            }
        }
    }
}

impl eframe::App for UltraRushApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if std::mem::take(&mut self.autoconnect) {
            self.quick_connect(&ctx);
        }
        for t in &mut self.tabs {
            t.poll();
        }
        if self.tabs.iter().any(|t| t.engine_events.is_some()) {
            ctx.request_repaint_after(Duration::from_millis(150));
        }

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::top("tabs").show(ui, |ui| self.tab_strip(ui));

        let mut actions = self.keyboard(&ctx);
        egui::CentralPanel::default().show(ui, |ui| {
            let tab = &mut self.tabs[self.active];
            let running = tab.engine.is_some();
            let Container { id, dock, sides, queue, progress, log, focus, .. } = tab;
            let mut viewer = PaneViewer { sides, queue, progress, log, actions: &mut actions, focus: *focus, running };
            let style = DockStyle::from_egui(ui.style().as_ref());
            DockArea::new(dock)
                .id(egui::Id::new(("dock", *id)))
                .style(style)
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut viewer);
        });
        self.apply(actions, &ctx);
        self.prompt_dialog(&ctx);
        self.trust_dialog(&ctx);
        self.sites.show(&ctx);
        if let Some(site) = self.sites.take_connect() {
            self.connect_site(site, &ctx);
        }
    }
}

// ====================================================================== panneaux

struct PaneViewer<'a> {
    sides: &'a mut [Side; 2],
    queue: &'a Arc<Mutex<Queue>>,
    progress: &'a HashMap<ItemId, (u64, Option<u64>)>,
    log: &'a [LogEntry],
    actions: &'a mut Vec<Action>,
    /// Panneau actif pour le clavier.
    focus: usize,
    /// Un moteur tourne (pour activer « Arrêter »).
    running: bool,
}

impl TabViewer for PaneViewer<'_> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
        let mark = |side: usize| if self.focus == side { "▸ " } else { "" };
        match tab {
            Pane::Left => format!("{}{}", mark(0), self.sides[0].title()).into(),
            Pane::Right => format!("{}{}", mark(1), self.sides[1].title()).into(),
            Pane::Queue => "☰ File d'attente".into(),
            Pane::Log => "📜 Journal".into(),
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
            Pane::Log => self.log_pane(ui),
        }
    }
}

impl PaneViewer<'_> {
    fn file_pane(&mut self, ui: &mut egui::Ui, side: usize) {
        let arrow = if side == 0 { "Transférer ➡" } else { "⬅ Transférer" };
        ui.horizontal(|ui| {
            if ui.button("⬆").on_hover_text("Dossier parent").clicked() {
                self.actions.push(Action::Up(side));
            }
            if ui.button("⟳").on_hover_text("Actualiser (F5)").clicked() {
                self.actions.push(Action::Refresh(side));
            }
            if ui.button("📁+").on_hover_text("Nouveau dossier (F7)").clicked() {
                self.actions.push(Action::NewFolder(side));
            }
            match &mut self.sides[side] {
                Side::Local(l) => {
                    let r = ui.add(egui::TextEdit::singleline(&mut l.path_edit).desired_width(ui.available_width() - 120.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        l.path = PathBuf::from(l.path_edit.trim());
                        l.reload();
                    }
                }
                Side::Remote(r) => {
                    let (dot, color) = match &r.state {
                        RemoteState::Connected => ("●", LOG_REPLY),
                        RemoteState::Connecting => ("●", Color32::from_rgb(0xE8, 0xB3, 0x3C)),
                        _ => ("●", LOG_ERROR),
                    };
                    ui.label(RichText::new(dot).color(color));
                    ui.label(RichText::new(&r.dir).monospace());
                    if ui.small_button("Déconnecter").clicked() {
                        self.actions.push(Action::Disconnect(side));
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
                ui.colored_label(LOG_ERROR, l.error.as_deref().unwrap_or_default());
            }
            Side::Remote(r) => match &r.state {
                RemoteState::Connecting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Connexion…");
                    });
                }
                RemoteState::Failed(e) => {
                    ui.colored_label(LOG_ERROR, e);
                }
                _ => {}
            },
            _ => {}
        }
        ui.separator();

        let rows: Vec<FileRow> = self.sides[side].rows().to_vec();
        let selected = self.sides[side].selected().clone();
        let sort = self.sides[side].sort();
        let mut clicked: Option<(usize, bool, bool)> = None;
        let mut opened: Option<String> = None;
        let mut sort_by: Option<SortCol> = None;
        let mut menu: Vec<Action> = Vec::new();

        // Toute la liste est une zone de dépôt : des fichiers glissés depuis l'autre panneau
        // y sont transférés.
        let (_, dropped) = ui.dnd_drop_zone::<DragFiles, ()>(egui::Frame::new(), |ui| {
            TableBuilder::new(ui)
                .id_salt(("files", side))
                .striped(true)
                .sense(Sense::click_and_drag())
                .column(Column::remainder().at_least(160.0).clip(true))
                .column(Column::initial(90.0).at_least(60.0))
                .column(Column::initial(130.0).at_least(80.0))
                .header(22.0, |mut h| {
                    for (col, label) in [(SortCol::Name, "Nom"), (SortCol::Size, "Taille"), (SortCol::Modified, "Modifié")] {
                        h.col(|ui| {
                            let arrow = if sort.col == col { if sort.asc { " ⏶" } else { " ⏷" } } else { "" };
                            let r = ui.add(egui::Label::new(RichText::new(format!("{label}{arrow}")).strong()).sense(Sense::click()));
                            if r.on_hover_text("Trier").clicked() {
                                sort_by = Some(col);
                            }
                        });
                    }
                })
                .body(|body| {
                    body.rows(22.0, rows.len(), |mut row| {
                        let i = row.index();
                        let f = &rows[i];
                        row.set_selected(selected.contains(&i));
                        row.col(|ui| {
                            let icon = if f.is_dir { "📁" } else { "📄" };
                            ui.label(format!("{icon} {}", f.name));
                        });
                        row.col(|ui| {
                            ui.label(if f.is_dir { String::new() } else { human_size(f.size) });
                        });
                        row.col(|ui| {
                            ui.label(RichText::new(&f.modified).weak());
                        });
                        let resp = row.response();
                        if resp.drag_started() && !selected.contains(&i) {
                            clicked = Some((i, false, false));
                        }
                        resp.dnd_set_drag_payload(DragFiles { from: side });
                        if resp.double_clicked() && f.is_dir {
                            opened = Some(f.name.clone());
                        } else if resp.clicked() {
                            let m = resp.ctx.input(|inp| inp.modifiers);
                            clicked = Some((i, m.command, m.shift));
                        } else if resp.secondary_clicked() && !selected.contains(&i) {
                            clicked = Some((i, false, false));
                        }
                        resp.context_menu(|ui| {
                            if ui.button("⇄ Transférer vers l'autre panneau   Espace").clicked() {
                                menu.push(Action::Transfer(side));
                                ui.close();
                            }
                            if f.is_dir && ui.button("📂 Ouvrir   Entrée").clicked() {
                                menu.push(Action::Open(side, f.name.clone()));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("✏ Renommer…   F2").clicked() {
                                menu.push(Action::Rename(side));
                                ui.close();
                            }
                            if ui.button("🗑 Supprimer…   Suppr").clicked() {
                                menu.push(Action::Delete(side));
                                ui.close();
                            }
                            if ui.button("📁 Nouveau dossier…   F7").clicked() {
                                menu.push(Action::NewFolder(side));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("📋 Copier le chemin").clicked() {
                                menu.push(Action::CopyPath(side, f.name.clone()));
                                ui.close();
                            }
                            if ui.button("⟳ Actualiser   F5").clicked() {
                                menu.push(Action::Refresh(side));
                                ui.close();
                            }
                        });
                    });
                });
        });

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
            let sel = self.sides[side].selected_mut();
            if range {
                // Maj : de la dernière sélection jusqu'ici.
                let anchor = sel.iter().next_back().copied().unwrap_or(i);
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
        let items = self.queue.lock().unwrap().items().to_vec();
        let failed = items.iter().filter(|i| matches!(i.state, ItemState::Failed(_) | ItemState::Stopped)).count();
        ui.horizontal(|ui| {
            if ui.button("▶ Démarrer").clicked() {
                self.actions.push(Action::StartQueue);
            }
            if ui.add_enabled(self.running, egui::Button::new("⏹ Tout arrêter")).clicked() {
                self.actions.push(Action::QueueStopAll);
            }
            if ui.add_enabled(failed > 0, egui::Button::new(format!("↻ Relancer les échecs ({failed})"))).clicked() {
                self.actions.push(Action::QueueRetryAll);
            }
            if ui.button("🗑 Retirer les terminés").clicked() {
                self.actions.push(Action::ClearFinished);
            }
            ui.label(RichText::new(format!("{} élément(s) — clic droit pour plus d'options", items.len())).weak());
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
                for t in ["", "Élément", "Taille", "Progression", "État"] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(24.0, items.len(), |mut row| {
                    let it = &items[row.index()];
                    let dir = match it.kind() {
                        urftp_core::TransferKind::Upload => "⬆",
                        urftp_core::TransferKind::Download => "⬇",
                        urftp_core::TransferKind::Fxp => "↔",
                    };
                    row.col(|ui| {
                        ui.label(dir);
                    });
                    row.col(|ui| {
                        let name = Path::new(&it.src.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| it.src.path.clone());
                        ui.label(format!("{}{}", if it.is_dir { "📁 " } else { "" }, name)).on_hover_text(format!("{} → {}", it.src.path, it.dst.path));
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
                        ui.add(egui::ProgressBar::new(frac.clamp(0.0, 1.0)).fill(ACCENT).show_percentage().corner_radius(6));
                    });
                    row.col(|ui| {
                        let (text, color) = match &it.state {
                            ItemState::Waiting => ("En attente".to_string(), LOG_STATUS),
                            ItemState::Running => ("En cours".to_string(), ACCENT),
                            ItemState::Done => ("Terminé".to_string(), LOG_REPLY),
                            ItemState::Skipped(r) => (format!("Ignoré : {r}"), LOG_STATUS),
                            ItemState::NeedsConfirmation => ("À confirmer".to_string(), Color32::from_rgb(0xE8, 0xB3, 0x3C)),
                            ItemState::Stopped => ("Arrêté".to_string(), LOG_STATUS),
                            ItemState::Failed(e) => (format!("Échec : {e}"), LOG_ERROR),
                        };
                        ui.label(RichText::new(&text).color(color)).on_hover_text(text);
                    });
                    let id = it.id;
                    let is_running = it.state == ItemState::Running;
                    let retryable = matches!(it.state, ItemState::Failed(_) | ItemState::Stopped | ItemState::NeedsConfirmation);
                    row.response().context_menu(|ui| {
                        if ui.add_enabled(running && is_running, egui::Button::new("⏹ Arrêter")).clicked() {
                            menu.push(Action::QueueStop(id));
                            ui.close();
                        }
                        if ui.add_enabled(retryable, egui::Button::new("↻ Relancer")).clicked() {
                            menu.push(Action::QueueRetry(id));
                            ui.close();
                        }
                        ui.separator();
                        for (m, label) in [(Move::Top, "⏫ Tout en haut"), (Move::Up, "⏶ Monter"), (Move::Down, "⏷ Descendre"), (Move::Bottom, "⏬ Tout en bas")] {
                            if ui.button(label).clicked() {
                                menu.push(Action::QueueMove(id, m));
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.add_enabled(!is_running, egui::Button::new("🗑 Retirer de la file")).clicked() {
                            menu.push(Action::QueueRemove(id));
                            ui.close();
                        }
                    });
                });
            });
        self.actions.extend(menu);
    }

    fn log_pane(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
            for e in self.log {
                let color = match e.kind {
                    LogKind::Command => LOG_COMMAND,
                    LogKind::Reply => LOG_REPLY,
                    LogKind::Status => LOG_STATUS,
                    LogKind::Error => LOG_ERROR,
                };
                let side = if e.side == 0 { "G" } else { "D" };
                ui.label(RichText::new(format!("[{side}] {}", e.text)).monospace().color(color));
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
