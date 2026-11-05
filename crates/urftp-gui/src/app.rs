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
}

impl FileRow {
    fn from_remote(e: &RemoteEntry) -> Self {
        Self { name: e.name.clone(), is_dir: e.is_dir(), size: e.size, modified: e.modified.map(fmt_time).unwrap_or_default() }
    }
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
}

impl LocalView {
    fn new(path: PathBuf) -> Self {
        let mut v = Self { path_edit: path.to_string_lossy().into_owned(), path, rows: Vec::new(), selected: BTreeSet::new(), error: None };
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
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| fmt_time(urftp_proto::sftp::unix_to_ftptime(d.as_secs() as i64)))
                            .unwrap_or_default();
                        Some(FileRow { name: e.file_name().to_string_lossy().into_owned(), is_dir: meta.is_dir(), size: meta.len(), modified })
                    })
                    .collect();
                rows.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
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
        }
    }

    fn title(&self) -> String {
        match (&self.sides[0], &self.sides[1]) {
            (_, Side::Remote(r)) | (Side::Remote(r), _) => r.profile.host.clone(),
            _ => format!("Onglet {}", self.id),
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
            dark: true,
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
        let handle = browser::spawn(&self.rt, ctx.clone());
        handle.send(BrowserCmd::Connect(Box::new(profile.clone())));
        let tab = &mut self.tabs[self.active];
        tab.push_log(1, LogKind::Status, format!("Connexion à {}…", profile.name));
        tab.sides[1] = Side::Remote(Box::new(RemoteView {
            profile,
            handle,
            dir: "/".into(),
            rows: Vec::new(),
            selected: BTreeSet::new(),
            state: RemoteState::Connecting,
        }));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Fichier", |ui| {
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
                let text = format!("{}{}", if busy { "⟳ " } else { "" }, tab.title());
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

        let mut actions = Vec::new();
        egui::CentralPanel::default().show(ui, |ui| {
            let tab = &mut self.tabs[self.active];
            let Container { id, dock, sides, queue, progress, log, .. } = tab;
            let mut viewer = PaneViewer { sides, queue, progress, log, actions: &mut actions };
            let style = DockStyle::from_egui(ui.style().as_ref());
            DockArea::new(dock)
                .id(egui::Id::new(("dock", *id)))
                .style(style)
                .show_close_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut viewer);
        });
        self.apply(actions, &ctx);
        self.trust_dialog(&ctx);
    }
}

// ====================================================================== panneaux

struct PaneViewer<'a> {
    sides: &'a mut [Side; 2],
    queue: &'a Arc<Mutex<Queue>>,
    progress: &'a HashMap<ItemId, (u64, Option<u64>)>,
    log: &'a [LogEntry],
    actions: &'a mut Vec<Action>,
}

impl TabViewer for PaneViewer<'_> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Pane) -> egui::WidgetText {
        match tab {
            Pane::Left => self.sides[0].title().into(),
            Pane::Right => self.sides[1].title().into(),
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
            if ui.button("⟳").on_hover_text("Actualiser").clicked() {
                self.actions.push(Action::Refresh(side));
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
        let mut clicked: Option<(usize, bool)> = None;
        let mut opened: Option<String> = None;
        TableBuilder::new(ui)
            .id_salt(("files", side))
            .striped(true)
            .sense(Sense::click())
            .column(Column::remainder().at_least(160.0).clip(true))
            .column(Column::initial(90.0).at_least(60.0))
            .column(Column::initial(130.0).at_least(80.0))
            .header(22.0, |mut h| {
                h.col(|ui| {
                    ui.strong("Nom");
                });
                h.col(|ui| {
                    ui.strong("Taille");
                });
                h.col(|ui| {
                    ui.strong("Modifié");
                });
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
                    if resp.double_clicked() && f.is_dir {
                        opened = Some(f.name.clone());
                    } else if resp.clicked() {
                        clicked = Some((i, resp.ctx.input(|inp| inp.modifiers.command || inp.modifiers.shift)));
                    }
                });
            });
        if let Some(name) = opened {
            self.actions.push(Action::Open(side, name));
        }
        if let Some((i, add)) = clicked {
            let sel = self.sides[side].selected_mut();
            if add {
                if !sel.remove(&i) {
                    sel.insert(i);
                }
            } else {
                sel.clear();
                sel.insert(i);
            }
        }
    }

    fn queue_pane(&mut self, ui: &mut egui::Ui) {
        let items = self.queue.lock().unwrap().items().to_vec();
        ui.horizontal(|ui| {
            if ui.button("▶ Démarrer").clicked() {
                self.actions.push(Action::StartQueue);
            }
            if ui.button("🗑 Retirer les terminés").clicked() {
                self.actions.push(Action::ClearFinished);
            }
            ui.label(RichText::new(format!("{} élément(s)", items.len())).weak());
        });
        ui.separator();
        TableBuilder::new(ui)
            .id_salt("queue")
            .striped(true)
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
                });
            });
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
