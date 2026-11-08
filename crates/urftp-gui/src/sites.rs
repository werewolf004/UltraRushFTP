//! Site manager: tree of folders and sites, settings form, import.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use urftp_i18n::t;

use eframe::egui::{self, RichText};
use urftp_core::import::{self, ImportResult, ImportWarning};
use urftp_core::settings::Settings;
use urftp_core::site::{
    Account, Bookmark, CHARSETS, CertPolicy, IdentChoice, ListCommand, ListParser, ProxyChoice, ProxyType, Site, SiteBook, SiteGroup,
    SiteListing, SiteProtocol, SiteProxy, TransferMode,
};
use urftp_core::sitestore;

use urftp_proto::session::SiteProfile;

use crate::browser::TrustKind;
use crate::theme;

/// Connection profile from a site record and the global options (global proxy,
/// timeouts).
pub fn to_profile(site: &Site, settings: &Settings) -> SiteProfile {
    urftp_proto::profile::site_profile(site, settings)
}

#[derive(Clone, PartialEq, Eq)]
enum Selection {
    Site(u64),
    /// Folder (indices from the root; empty = root).
    Group(Vec<usize>),
}

struct Report {
    title: String,
    lines: Vec<String>,
    error: bool,
}

/// How the user asked to log in (`Shift` / `Ctrl` held on Connect).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoginVariant {
    /// Ghost login `!user`: the server closes the other sessions of the account.
    pub ghost: bool,
    /// `-password`: no welcome messages (glftpd...).
    pub quiet: bool,
}

impl LoginVariant {
    pub fn from_modifiers(m: egui::Modifiers) -> Self {
        Self { ghost: m.shift, quiet: m.command }
    }
}

/// Result of the availability test of a site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Running,
    Up { address: String, millis: u128 },
    Down(String),
}

/// Results of the availability tests, written by the network tasks.
pub type Probes = Arc<Mutex<HashMap<u64, Probe>>>;

pub struct SiteManager {
    pub open: bool,
    pub book: SiteBook,
    path: PathBuf,
    selection: Selection,
    show_password: bool,
    dirty: bool,
    last_change: f64,
    report: Option<Report>,
    confirm_delete: Option<Selection>,
    connect: Option<(u64, LoginVariant)>,
    /// Sites to test, picked up by the application which owns the runtime.
    probe_request: Vec<Site>,
    pub probes: Probes,
}

/// Site books of other clients present on this machine.
fn known_sources() -> Vec<(&'static str, PathBuf)> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let mut v = Vec::new();
    if let Some(a) = &appdata {
        v.push(("FileZilla", a.join("FileZilla").join("sitemanager.xml")));
        v.push(("FTPRush v2", a.join("FTPRush").join("RushSite.xml")));
    }
    if let Some(h) = &home {
        v.push(("FTPRush v3", h.join("FTPRush").join("site.json")));
    }
    v.into_iter().filter(|(_, p)| p.exists()).collect()
}

impl SiteManager {
    pub fn load() -> Self {
        let path = sitestore::default_path();
        let (book, report) = match sitestore::load(&path) {
            Ok(b) => (b, None),
            Err(e) => (
                SiteBook::default(),
                Some(Report { title: t!("sites.book_unreadable"), lines: vec![format!("{} : {e}", path.display())], error: true }),
            ),
        };
        // Development: `URUSHFTP_OPEN_SITES=<id>` opens the manager on this site.
        let open_on = std::env::var("URUSHFTP_OPEN_SITES").ok().and_then(|v| v.parse::<u64>().ok());
        Self {
            open: open_on.is_some(),
            selection: open_on.filter(|id| book.find(*id).is_some()).map_or(Selection::Group(Vec::new()), Selection::Site),
            book,
            path,
            show_password: false,
            dirty: false,
            last_change: 0.0,
            report,
            confirm_delete: None,
            connect: None,
            probe_request: Vec::new(),
            probes: Probes::default(),
        }
    }

    pub fn save(&mut self) {
        match sitestore::save(&self.path, &self.book) {
            Ok(()) => self.dirty = false,
            Err(e) => self.report = Some(Report { title: t!("sites.save_failed"), lines: vec![e.to_string()], error: true }),
        }
    }

    /// Site the user asked to connect to, and how.
    pub fn take_connect(&mut self) -> Option<(Site, LoginVariant)> {
        let (id, variant) = self.connect.take()?;
        self.book.find(id).cloned().map(|s| (s, variant))
    }

    /// Sites whose availability the user asked to test (marked as running).
    pub fn take_probe_requests(&mut self) -> Vec<Site> {
        let sites = std::mem::take(&mut self.probe_request);
        let mut probes = self.probes.lock();
        for s in &sites {
            probes.insert(s.id, Probe::Running);
        }
        sites
    }

    /// Enabled sites of the selection (one site, or a folder and its subfolders).
    fn selected_sites(&self) -> Vec<Site> {
        let usable = |s: &&Site| s.enabled && !s.host.trim().is_empty();
        match &self.selection {
            Selection::Site(id) => self.book.find(*id).filter(usable).cloned().into_iter().collect(),
            Selection::Group(path) => {
                self.book.root.group(path).map(|g| g.sites_recursive().into_iter().filter(usable).cloned().collect()).unwrap_or_default()
            }
        }
    }

    /// Stores a trusted fingerprint in the site record.
    pub fn remember_trust(&mut self, id: u64, kind: TrustKind, fingerprint: &str) {
        if let Some(s) = self.book.find_mut(id) {
            match kind {
                TrustKind::TlsCertificate => s.cert = CertPolicy::Fingerprint(fingerprint.to_string()),
                TrustKind::SshHostKey => {
                    if !s.ssh_host_keys.iter().any(|k| k == fingerprint) {
                        s.ssh_host_keys.push(fingerprint.to_string());
                    }
                }
            }
            self.save();
        }
    }

    fn current_group_path(&self) -> Vec<usize> {
        match &self.selection {
            Selection::Group(p) => p.clone(),
            Selection::Site(id) => self.book.group_path_of(*id).unwrap_or_default(),
        }
    }

    fn import(&mut self, path: PathBuf) {
        match import::import_file(&path) {
            Ok(ImportResult { format, group, warnings }) => {
                let n = group.site_count();
                self.book.merge_import(group);
                self.selection = Selection::Group(vec![self.book.root.groups.len() - 1]);
                self.save();
                let mut lines = vec![t!("sites.imported", n = n, format = format.label(), path = path.display())];
                lines.extend(warnings.iter().map(|w| format!("• {}", warning_text(w))));
                self.report = Some(Report { title: t!("sites.import_done"), lines, error: false });
            }
            Err(e) => {
                self.report =
                    Some(Report { title: t!("sites.import_failed"), lines: vec![format!("{} : {e}", path.display())], error: true })
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, settings: &Settings) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new(t!("sites.title"))
            .id(egui::Id::new("site-manager"))
            .open(&mut open)
            .default_size([900.0, 600.0])
            // Below the menu and connection bars, which stay usable.
            .default_pos([240.0, 110.0])
            .min_width(720.0)
            .collapsible(false)
            .show(ctx, |ui| {
                self.toolbar(ui, settings);
                ui.separator();
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(260.0);
                        egui::ScrollArea::vertical().id_salt("site-tree").max_height(ui.available_height() - 4.0).show(ui, |ui| {
                            let mut clicked = None;
                            let probes = self.probes.lock().clone();
                            tree(ui, &self.book.root, &mut Vec::new(), &self.selection, &probes, &mut clicked);
                            if let Some(sel) = clicked {
                                self.selection = sel;
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical().id_salt("site-form").show(ui, |ui| self.form(ui, settings));
                    });
                });
            });
        if !open {
            self.open = false;
            if self.dirty {
                self.save();
            }
        }
        // Automatic save one second after the last change.
        let now = ctx.input(|i| i.time);
        if self.dirty && now - self.last_change > 1.0 {
            self.save();
        } else if self.dirty {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        self.dialogs(ctx);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        ui.horizontal_wrapped(|ui| {
            if ui.button(t!("sites.new_site")).clicked() {
                let group = self.current_group_path();
                let id = self.book.add_site(&group, settings.make_site(t!("sites.new_site_default")));
                self.selection = Selection::Site(id);
                self.mark_dirty(ui);
            }
            if ui.button(t!("sites.new_folder")).clicked() {
                let path = self.current_group_path();
                if let Some(g) = self.book.root.group_mut(&path) {
                    g.groups.push(SiteGroup::new(t!("sites.new_folder_default")));
                    let mut p = path;
                    p.push(g.groups.len() - 1);
                    self.selection = Selection::Group(p);
                    self.mark_dirty(ui);
                }
            }
            if let Selection::Site(id) = self.selection
                && ui.button(t!("sites.duplicate")).clicked()
                && let Some(new) = self.book.duplicate(id)
            {
                self.selection = Selection::Site(new);
                self.mark_dirty(ui);
            }
            let deletable = !matches!(&self.selection, Selection::Group(p) if p.is_empty());
            if ui.add_enabled(deletable, egui::Button::new(t!("sites.delete"))).clicked() {
                self.confirm_delete = Some(self.selection.clone());
            }
            let to_test = self.selected_sites();
            let test = egui::Button::new(t!("sites.test"));
            if ui.add_enabled(!to_test.is_empty(), test).on_hover_text(t!("sites.test_hint")).clicked() {
                self.probe_request = to_test;
            }
            ui.separator();
            ui.menu_button(t!("sites.import"), |ui| {
                if ui.button(t!("sites.import_pick")).clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new()
                        .set_title(t!("sites.import_dialog"))
                        .add_filter(t!("sites.import_filter"), &["xml", "json", "ftp"])
                        .add_filter(t!("sites.all_files"), &["*"])
                        .pick_file()
                    {
                        self.import(p);
                    }
                }
                let found = known_sources();
                if !found.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(t!("sites.found_here")).weak());
                    for (label, path) in found {
                        if ui.button(format!("{label} — {}", path.display())).clicked() {
                            ui.close();
                            self.import(path);
                        }
                    }
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Selection::Site(id) = self.selection {
                    let enabled = self.book.find(id).is_some_and(|s| s.enabled && !s.host.trim().is_empty());
                    let p = theme::colors(ui);
                    let btn = egui::Button::new(RichText::new(t!("sites.connect")).color(p.on_accent)).fill(p.accent_fill);
                    if ui.add_enabled(enabled, btn).on_hover_text(t!("sites.connect_hint")).clicked() {
                        self.save();
                        self.connect = Some((id, LoginVariant::from_modifiers(ui.input(|i| i.modifiers))));
                        self.open = false;
                    }
                }
                ui.label(RichText::new(t!("sites.count", n = self.book.root.site_count())).weak());
            });
        });
    }

    fn mark_dirty(&mut self, ui: &egui::Ui) {
        self.dirty = true;
        self.last_change = ui.input(|i| i.time);
    }

    fn form(&mut self, ui: &mut egui::Ui, settings: &Settings) {
        match self.selection.clone() {
            Selection::Group(path) => {
                let is_root = path.is_empty();
                let Some(g) = self.book.root.group_mut(&path) else { return };
                ui.heading(if is_root { t!("sites.all_sites") } else { t!("sites.folder") });
                ui.add_space(6.0);
                if !is_root {
                    let before = g.name.clone();
                    ui.horizontal(|ui| {
                        theme::labelled(ui, t!("sites.folder_name"), theme::single_line(&mut g.name));
                    });
                    if g.name != before {
                        self.mark_dirty(ui);
                    }
                }
                let n = self.book.root.group_mut(&path).map_or(0, |g| g.site_count());
                ui.label(RichText::new(t!("sites.folder_count", n = n)).weak());
                ui.add_space(8.0);
                ui.label(t!("sites.select_hint"));
            }
            Selection::Site(id) => {
                let show_password = &mut self.show_password;
                let probe = self.probes.lock().get(&id).cloned();
                let Some(site) = self.book.find_mut(id) else { return };
                let before = site.clone();
                site_form(ui, site, show_password, probe.as_ref(), settings);
                if *site != before {
                    self.dirty = true;
                    self.last_change = ui.input(|i| i.time);
                }
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(sel) = self.confirm_delete.clone() {
            let what = match &sel {
                Selection::Site(id) => t!("sites.delete_site", name = self.book.find(*id).map_or("?", |s| s.name.as_str())),
                Selection::Group(_) => t!("sites.delete_folder"),
            };
            let mut answer = None;
            egui::Modal::new(egui::Id::new("confirm-delete-site")).show(ctx, |ui| {
                ui.heading(t!("sites.delete_title"));
                ui.label(t!("sites.delete_confirm", what = what));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if theme::danger_button(ui, t!("common.delete")).clicked() {
                        answer = Some(true);
                    }
                    let cancel = ui.button(t!("common.cancel"));
                    // Irreversible: the keyboard starts on Cancel.
                    if ui.memory(|m| m.focused().is_none()) {
                        cancel.request_focus();
                    }
                    if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        answer = Some(false);
                    }
                });
            });
            if let Some(yes) = answer {
                self.confirm_delete = None;
                if yes {
                    match sel {
                        Selection::Site(id) => {
                            let parent = self.book.group_path_of(id).unwrap_or_default();
                            self.book.remove(id);
                            self.selection = Selection::Group(parent);
                        }
                        Selection::Group(path) => {
                            if let Some((last, parent)) = path.split_last()
                                && let Some(g) = self.book.root.group_mut(parent)
                            {
                                g.groups.remove(*last);
                                self.selection = Selection::Group(parent.to_vec());
                            }
                        }
                    }
                    self.save();
                }
            }
        }
        if let Some(r) = &self.report {
            let mut close = false;
            egui::Modal::new(egui::Id::new("site-report")).show(ctx, |ui| {
                ui.set_max_width(560.0);
                let p = theme::colors(ui);
                let mark = if r.error { "⚠" } else { "✔" };
                ui.heading(RichText::new(format!("{mark} {}", r.title)).color(if r.error { p.error } else { p.success }));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for l in &r.lines {
                        ui.label(l);
                    }
                });
                ui.add_space(8.0);
                let ok = ui.button(t!("common.ok"));
                if ui.memory(|m| m.focused().is_none()) {
                    ok.request_focus();
                }
                if ok.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    close = true;
                }
            });
            if close {
                self.report = None;
            }
        }
    }
}

/// Short mark of a test result, with its colour and its detail.
fn probe_mark(ui: &egui::Ui, p: &Probe) -> (RichText, String) {
    let c = theme::colors(ui);
    match p {
        Probe::Running => (RichText::new("…").weak(), t!("sites.test_running")),
        Probe::Up { address, millis } => {
            (RichText::new(format!("✔ {millis} ms")).color(c.success), t!("sites.test_up", address = address, ms = millis))
        }
        Probe::Down(e) => (RichText::new("✖").color(c.error), t!("sites.test_down", error = e)),
    }
}

fn tree(
    ui: &mut egui::Ui,
    g: &SiteGroup,
    path: &mut Vec<usize>,
    sel: &Selection,
    probes: &HashMap<u64, Probe>,
    clicked: &mut Option<Selection>,
) {
    let title = if path.is_empty() { t!("sites.all_sites") } else { format!("📁 {}", g.name) };
    let header = egui::CollapsingHeader::new(RichText::new(title).strong())
        .id_salt(("grp", path.clone()))
        .default_open(path.len() < 2)
        .show(ui, |ui| {
            for (i, child) in g.groups.iter().enumerate() {
                path.push(i);
                tree(ui, child, path, sel, probes, clicked);
                path.pop();
            }
            for s in &g.sites {
                let icon = match s.protocol {
                    SiteProtocol::Sftp => "🔑",
                    p if p.is_tls() => "🔒",
                    _ => "🌐",
                };
                let mut text = if s.description.trim().is_empty() {
                    RichText::new(format!("{icon} {}", s.name))
                } else {
                    RichText::new(format!("{icon} {} — {}", s.name, s.description.trim()))
                };
                if !s.enabled {
                    text = text.weak().strikethrough();
                }
                ui.horizontal(|ui| {
                    let hover = if s.description.trim().is_empty() {
                        format!("{}:{}", s.host, s.port)
                    } else {
                        format!("{}\n{}:{}", s.description.trim(), s.host, s.port)
                    };
                    if ui.selectable_label(*sel == Selection::Site(s.id), text).on_hover_text(hover).clicked() {
                        *clicked = Some(Selection::Site(s.id));
                    }
                    if let Some(p) = probes.get(&s.id) {
                        let (mark, detail) = probe_mark(ui, p);
                        ui.label(mark).on_hover_text(detail);
                    }
                });
            }
        });
    if header.header_response.clicked() {
        *clicked = Some(Selection::Group(path.clone()));
    }
}

/// Translated import remark.
fn warning_text(w: &ImportWarning) -> String {
    match w {
        ImportWarning::NoSites => t!("import.no_sites"),
        ImportWarning::TlsIfAvailable => t!("import.tls_if_available"),
        ImportWarning::UnsupportedProtocol { site, source_format, protocol } => {
            t!("import.unsupported_protocol", site = site, format = source_format, protocol = protocol)
        }
        ImportWarning::UnknownCode { site, code, assumed } => {
            t!("import.unknown_code", site = site, code = code, protocol = proto_label(*assumed))
        }
        ImportWarning::PasswordUnreadable(site) => t!("import.password_unreadable", site = site),
        ImportWarning::MasterPassword(site) => t!("import.master_password", site = site),
        ImportWarning::PasswordsNotImported(n) => t!("import.passwords_not_imported", n = n),
    }
}

/// Translated protocol name.
pub fn proto_label(p: SiteProtocol) -> String {
    match p {
        SiteProtocol::Ftp => t!("proto.ftp"),
        SiteProtocol::FtpsExplicit => t!("proto.ftps_explicit"),
        SiteProtocol::FtpsImplicit => t!("proto.ftps_implicit"),
        SiteProtocol::Sftp => t!("proto.sftp"),
    }
}

/// Grid row: a label, then controls; `add` receives the label id, for `labelled_by`.
fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui, egui::Id)) {
    let l = ui.label(label);
    add(ui, l.id);
    ui.end_row();
}

fn optional_limit(ui: &mut egui::Ui, value: &mut Option<u32>, label: egui::Id, name: &str) {
    let mut on = value.is_some();
    ui.horizontal(|ui| {
        let c = ui.checkbox(&mut on, "");
        theme::name(&c, t!("site.limit_on", limit = name));
        let mut v = value.unwrap_or(1);
        ui.add_enabled(on, egui::DragValue::new(&mut v).range(0..=50)).labelled_by(label);
        *value = on.then_some(v);
    });
}

fn site_form(ui: &mut egui::Ui, s: &mut Site, show_password: &mut bool, probe: Option<&Probe>, global: &Settings) {
    ui.heading(format!("{} {}", if s.protocol == SiteProtocol::Sftp { "🔑" } else { "🌐" }, s.name));
    ui.add_space(4.0);
    section(ui, &t!("site.connection"), |ui| {
        egui::Grid::new("site-conn").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            row(ui, &t!("common.name"), |ui, l| {
                ui.add(theme::single_line(&mut s.name).desired_width(320.0)).labelled_by(l);
            });
            row(ui, &t!("site.description"), |ui, l| {
                ui.add(theme::single_line(&mut s.description).hint_text(t!("site.description_hint")).desired_width(320.0)).labelled_by(l);
            });
            row(ui, &t!("site.protocol"), |ui, l| {
                let before = s.protocol;
                egui::ComboBox::from_id_salt("site-proto")
                    .selected_text(proto_label(s.protocol))
                    .show_ui(ui, |ui| {
                        for p in SiteProtocol::ALL {
                            ui.selectable_value(&mut s.protocol, p, proto_label(p));
                        }
                    })
                    .response
                    .labelled_by(l);
                if s.protocol != before && s.port == before.default_port() {
                    s.port = s.protocol.default_port();
                }
            });
            row(ui, &t!("common.host"), |ui, l| {
                ui.add(theme::single_line(&mut s.host).hint_text(t!("site.host_hint")).desired_width(240.0)).labelled_by(l);
                theme::labelled(ui, t!("common.port"), egui::DragValue::new(&mut s.port).range(1..=65535));
            });
            row(ui, &t!("common.user"), |ui, l| {
                ui.add_enabled(!s.anonymous, theme::single_line(&mut s.user).desired_width(200.0)).labelled_by(l);
                ui.checkbox(&mut s.anonymous, t!("site.anonymous"));
            });
            row(ui, &if s.ssh_key_file.is_some() { t!("site.passphrase") } else { t!("common.password") }, |ui, l| {
                ui.add_enabled(!s.anonymous, theme::single_line(&mut s.password).password(!*show_password).desired_width(200.0))
                    .labelled_by(l);
                let label = if *show_password { t!("site.hide_password") } else { t!("site.show_password") };
                if theme::icon_button(ui, if *show_password { "🙈" } else { "👁" }, label).clicked() {
                    *show_password = !*show_password;
                }
            });
            row(ui, &t!("site.alt_hosts"), |ui, l| {
                let mut text = s.alt_hosts.join("\n");
                let edit = egui::TextEdit::multiline(&mut text).desired_rows(2).desired_width(240.0).hint_text(t!("site.alt_hosts_hint"));
                if ui.add(edit).labelled_by(l).on_hover_text(t!("site.alt_hosts_tip")).changed() {
                    s.alt_hosts = if text.is_empty() { Vec::new() } else { text.split('\n').map(String::from).collect() };
                }
            });
            row(ui, &t!("site.active"), |ui, _| {
                ui.checkbox(&mut s.enabled, t!("site.enabled"));
            });
        });
        if let Some(p) = probe {
            let (mark, detail) = probe_mark(ui, p);
            ui.horizontal(|ui| {
                ui.label(mark);
                ui.label(detail);
            });
        }
    });

    if s.protocol.is_tls() {
        section(ui, &t!("site.tls"), |ui| {
            ui.checkbox(&mut s.protect_data, t!("site.protect_data"));
            let mut listings = s.listings_protected();
            if ui.checkbox(&mut listings, t!("site.protect_listings")).changed() {
                s.protect_listings = Some(listings);
            }
            let label = match &s.cert {
                CertPolicy::System => t!("site.cert_system"),
                CertPolicy::AcceptAny => t!("site.cert_any"),
                CertPolicy::Fingerprint(_) => t!("site.cert_pinned"),
            };
            let cert = egui::ComboBox::from_id_salt("site-cert").selected_text(label).width(300.0).show_ui(ui, |ui| {
                if ui.selectable_label(s.cert == CertPolicy::System, t!("site.cert_system")).clicked() {
                    s.cert = CertPolicy::System;
                }
                if ui.selectable_label(s.cert == CertPolicy::AcceptAny, t!("site.cert_any")).clicked() {
                    s.cert = CertPolicy::AcceptAny;
                }
                if ui.selectable_label(matches!(s.cert, CertPolicy::Fingerprint(_)), t!("site.cert_pinned")).clicked()
                    && !matches!(s.cert, CertPolicy::Fingerprint(_))
                {
                    s.cert = CertPolicy::Fingerprint(String::new());
                }
            });
            theme::name(&cert.response, t!("site.cert_policy"));
            if let CertPolicy::Fingerprint(fp) = &mut s.cert {
                let r = ui.add(theme::single_line(fp).hint_text(t!("site.fingerprint_hint")).desired_width(f32::INFINITY));
                theme::name(&r, t!("site.fingerprint"));
            }
        });
    }

    if s.protocol == SiteProtocol::Sftp {
        section(ui, &t!("site.ssh"), |ui| {
            let mut use_key = s.ssh_key_file.is_some();
            ui.checkbox(&mut use_key, t!("site.use_key"));
            if use_key {
                let key = s.ssh_key_file.get_or_insert_with(String::new);
                ui.horizontal(|ui| {
                    let r = ui.add(theme::single_line(key).desired_width(360.0));
                    theme::name(&r, t!("site.key_file"));
                    if ui.button(t!("common.browse")).clicked()
                        && let Some(p) = rfd::FileDialog::new().set_title(t!("site.key_dialog")).pick_file()
                    {
                        *key = p.to_string_lossy().into_owned();
                    }
                });
                ui.label(RichText::new(t!("site.more_keys")).weak());
                let mut remove = None;
                for (i, k) in s.ssh_more_keys.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let r = ui.add(theme::single_line(k).desired_width(360.0));
                        theme::name(&r, t!("site.more_key", n = i + 2));
                        if theme::icon_button(ui, "✕", t!("site.remove_key", n = i + 2)).clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    s.ssh_more_keys.remove(i);
                }
                if ui.button(t!("site.add_key")).clicked()
                    && let Some(p) = rfd::FileDialog::new().set_title(t!("site.key_dialog")).pick_file()
                {
                    s.ssh_more_keys.push(p.to_string_lossy().into_owned());
                }
            } else {
                s.ssh_key_file = None;
                s.ssh_more_keys.clear();
            }
            ui.label(RichText::new(t!("site.host_keys", n = s.ssh_host_keys.len())).weak());
            let mut remove = None;
            for (i, k) in s.ssh_host_keys.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.code(k);
                    if theme::icon_button(ui, "✕", t!("site.remove_host_key")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                s.ssh_host_keys.remove(i);
            }
        });
    }

    section(ui, &t!("site.transfers"), |ui| {
        egui::Grid::new("site-xfer").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            if s.protocol != SiteProtocol::Sftp {
                row(ui, &t!("site.mode"), |ui, _| {
                    ui.radio_value(&mut s.transfer_mode, TransferMode::Passive, t!("site.passive"));
                    ui.radio_value(&mut s.transfer_mode, TransferMode::Active, t!("site.active_mode"));
                });
            }
            row(ui, &t!("site.parallel"), |ui, l| {
                ui.add(egui::DragValue::new(&mut s.max_transfers).range(0..=50)).labelled_by(l);
                ui.label(RichText::new(t!("site.single_connection")).weak());
            });
            row(ui, &t!("site.max_downloads"), |ui, l| optional_limit(ui, &mut s.max_downloads, l, &t!("site.max_downloads")));
            row(ui, &t!("site.max_uploads"), |ui, l| optional_limit(ui, &mut s.max_uploads, l, &t!("site.max_uploads")));
            row(ui, &t!("site.remote_dir"), |ui, l| {
                ui.add(theme::single_line(&mut s.remote_dir).hint_text("/").desired_width(300.0)).labelled_by(l);
            });
            row(ui, &t!("site.local_dir"), |ui, l| {
                ui.add(theme::single_line(&mut s.local_dir).desired_width(300.0)).labelled_by(l);
                if theme::icon_button(ui, "…", t!("site.pick_local_dir")).clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_folder()
                {
                    s.local_dir = p.to_string_lossy().into_owned();
                }
            });
        });
        if s.protocol != SiteProtocol::Sftp {
            ui.checkbox(&mut s.fxp_source_active, t!("site.fxp_source_active"));
            ui.checkbox(&mut s.fxp_target_passive, t!("site.fxp_target_passive"));
        }
    });

    if s.protocol != SiteProtocol::Sftp {
        section(ui, &t!("site.listing"), |ui| listing_ui(ui, &mut s.listing));
    }

    section(ui, &t!("site.http"), |ui| {
        egui::Grid::new("site-http").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            row(ui, &t!("site.http_url"), |ui, l| {
                ui.add(theme::single_line(&mut s.http.base_url).hint_text("https://www.example.com/files").desired_width(320.0))
                    .labelled_by(l);
            });
            row(ui, &t!("site.http_root"), |ui, l| {
                ui.add(theme::single_line(&mut s.http.ftp_root).hint_text("/").desired_width(200.0)).labelled_by(l);
            });
        });
        ui.label(RichText::new(t!("site.http_note")).weak());
    });

    section(ui, &t!("site.proxy_ident"), |ui| {
        let label = match &s.proxy {
            ProxyChoice::Global => t!("site.proxy_global"),
            ProxyChoice::Direct => t!("site.proxy_direct"),
            ProxyChoice::Custom(_) => t!("site.proxy_custom"),
            ProxyChoice::Named(n) if global.named_proxy(n).is_some() => n.clone(),
            ProxyChoice::Named(n) => t!("site.proxy_missing", name = n),
        };
        let proxy = egui::ComboBox::from_id_salt("site-proxy").selected_text(label).show_ui(ui, |ui| {
            if ui.selectable_label(s.proxy == ProxyChoice::Global, t!("site.proxy_global")).clicked() {
                s.proxy = ProxyChoice::Global;
            }
            if ui.selectable_label(s.proxy == ProxyChoice::Direct, t!("site.proxy_direct")).clicked() {
                s.proxy = ProxyChoice::Direct;
            }
            if ui.selectable_label(matches!(s.proxy, ProxyChoice::Custom(_)), t!("site.proxy_custom")).clicked()
                && !matches!(s.proxy, ProxyChoice::Custom(_))
            {
                s.proxy = ProxyChoice::Custom(SiteProxy { port: 1080, data_channel: true, ..SiteProxy::default() });
            }
            for n in &global.proxies {
                let chosen = matches!(&s.proxy, ProxyChoice::Named(x) if *x == n.name);
                if ui.selectable_label(chosen, &n.name).clicked() {
                    s.proxy = ProxyChoice::Named(n.name.clone());
                }
            }
        });
        theme::name(&proxy.response, t!("site.proxy"));
        if let ProxyChoice::Custom(p) = &mut s.proxy {
            egui::Grid::new("site-proxy-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                row(ui, &t!("common.type"), |ui, _| {
                    for (k, l) in [
                        (ProxyType::Socks4, "SOCKS4"),
                        (ProxyType::Socks4a, "SOCKS4A"),
                        (ProxyType::Socks5, "SOCKS5"),
                        (ProxyType::HttpTunnel, "HTTP Tunnel"),
                    ] {
                        ui.radio_value(&mut p.kind, k, l);
                    }
                });
                row(ui, &t!("common.host"), |ui, l| {
                    ui.add(theme::single_line(&mut p.host).desired_width(200.0)).labelled_by(l);
                    theme::labelled(ui, t!("common.port"), egui::DragValue::new(&mut p.port).range(1..=65535));
                });
                row(ui, &t!("common.user"), |ui, l| {
                    ui.add(theme::single_line(&mut p.user).desired_width(140.0)).labelled_by(l);
                    theme::labelled(ui, t!("common.password"), theme::single_line(&mut p.password).password(true).desired_width(140.0));
                });
                row(ui, "", |ui, _| {
                    ui.checkbox(&mut p.data_channel, t!("site.proxy_data"));
                });
            });
        }
        ui.add_space(4.0);
        let mut custom = matches!(s.ident, IdentChoice::Custom(_));
        ui.horizontal(|ui| {
            ui.checkbox(&mut custom, t!("site.ident_custom"));
            if custom {
                if let IdentChoice::Global = s.ident {
                    s.ident = IdentChoice::Custom(String::new());
                }
                if let IdentChoice::Custom(n) = &mut s.ident {
                    let r = ui.add(theme::single_line(n).hint_text(urftp_core::DEFAULT_IDENT_NAME).desired_width(160.0));
                    theme::name(&r, t!("site.ident_name"));
                }
            } else {
                s.ident = IdentChoice::Global;
            }
        });
    });

    ui.push_id("site-own", |ui| own_settings(ui, s, global, show_password));

    section(ui, &t!("site.bookmarks"), |ui| {
        let mut remove = None;
        egui::Grid::new("site-bm").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
            for (i, b) in s.bookmarks.iter_mut().enumerate() {
                for (value, key, width) in [
                    (&mut b.name, "common.name", 110.0),
                    (&mut b.remote_dir, "common.remote", 170.0),
                    (&mut b.local_dir, "common.local", 170.0),
                ] {
                    let label = urftp_i18n::text(key);
                    let r = ui.add(theme::single_line(value).hint_text(label).desired_width(width));
                    theme::name(&r, t!("site.bookmark_field", n = i + 1, field = label));
                }
                if theme::icon_button(ui, "✕", t!("site.remove_bookmark", n = i + 1)).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            s.bookmarks.remove(i);
        }
        if ui.button(t!("site.add_bookmark")).clicked() {
            s.bookmarks.push(Bookmark::default());
        }
    });

    section(ui, &t!("site.notes"), |ui| {
        let r = ui.add(egui::TextEdit::multiline(&mut s.notes).desired_rows(3).desired_width(f32::INFINITY));
        theme::name(&r, t!("site.notes"));
    });
}

/// Checkbox "specific to this site": on, the value starts from the global one; off, the
/// global setting applies again.
fn own<'a, T: Clone>(ui: &mut egui::Ui, value: &'a mut Option<T>, global: &T, label: String) -> Option<&'a mut T> {
    let mut on = value.is_some();
    if ui.checkbox(&mut on, label).changed() {
        *value = on.then(|| global.clone());
    }
    value.as_mut()
}

/// Separate account for one direction.
fn account_ui(ui: &mut egui::Ui, value: &mut Option<Account>, label: String, show_password: bool) {
    let mut on = value.is_some();
    if ui.checkbox(&mut on, label.clone()).changed() {
        *value = on.then(Account::default);
    }
    if let Some(a) = value {
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            let r = ui.add(theme::single_line(&mut a.user).hint_text(t!("common.user")).desired_width(150.0));
            theme::name(&r, format!("{label} — {}", t!("common.user")));
            let r =
                ui.add(theme::single_line(&mut a.password).password(!show_password).hint_text(t!("common.password")).desired_width(150.0));
            theme::name(&r, format!("{label} — {}", t!("common.password")));
        });
    }
}

/// Settings of the site replacing the global options.
fn own_settings(ui: &mut egui::Ui, s: &mut Site, global: &Settings, show_password: &bool) {
    let o = &mut s.settings;
    let is_ftp = s.protocol != SiteProtocol::Sftp;
    // Development: `URUSHFTP_SITE_SETTINGS=1` keeps the form scrolled to this part.
    static SCROLL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *SCROLL.get_or_init(|| std::env::var_os("URUSHFTP_SITE_SETTINGS").is_some()) {
        ui.scroll_to_cursor(Some(egui::Align::TOP));
    }
    section(ui, &t!("site.own_connection"), |ui| {
        ui.label(RichText::new(t!("site.own_note")).weak());
        let mut on = o.retries.is_some();
        if ui.checkbox(&mut on, t!("site.own_retries")).changed() {
            let t = &global.transfer;
            o.retries = on.then_some(t.retries);
            o.retry_delay_secs = on.then_some(t.retry_delay_secs);
            o.connect_timeout_secs = on.then_some(t.connect_timeout_secs);
            o.io_timeout_secs = on.then_some(t.io_timeout_secs);
        }
        if on {
            egui::Grid::new("site-own-retries").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                for (value, key, range) in [
                    (&mut o.retries, "options.retries", 0..=50),
                    (&mut o.retry_delay_secs, "options.retry_delay", 0..=600),
                    (&mut o.connect_timeout_secs, "options.connect_timeout", 5..=600),
                    (&mut o.io_timeout_secs, "options.io_timeout", 10..=3600),
                ] {
                    let v = value.get_or_insert(*range.start());
                    theme::grid_row(ui, urftp_i18n::text(key), egui::DragValue::new(v).range(range));
                }
            });
        }
        ui.checkbox(&mut o.resume, t!("site.resume"));
        if is_ftp {
            ui.checkbox(&mut o.send_feat, t!("site.send_feat"));
            ui.horizontal(|ui| {
                theme::labelled(
                    ui,
                    t!("site.client_name"),
                    theme::single_line(&mut o.client_name).hint_text("UltraRushFTP").desired_width(160.0),
                );
            });
        }
        ui.horizontal(|ui| {
            let l = ui.label(t!("site.dir_cache"));
            let label = |v: Option<bool>| match v {
                None => t!("site.dir_cache_global"),
                Some(true) => t!("common.yes"),
                Some(false) => t!("common.no"),
            };
            let combo = egui::ComboBox::from_id_salt("site-dir-cache").selected_text(label(o.dir_cache)).show_ui(ui, |ui| {
                for v in [None, Some(true), Some(false)] {
                    ui.selectable_value(&mut o.dir_cache, v, label(v));
                }
            });
            combo.response.labelled_by(l.id);
        });
        if let Some(k) = own(ui, &mut o.keep_alive, &global.keep_alive, t!("site.own_keep_alive")) {
            ui.indent("site-own-ka", |ui| crate::options::keep_alive_ui(ui, k));
        }
    });

    section(ui, &t!("site.own_queue"), |ui| {
        if let Some(k) = own(ui, &mut o.skiplist, &global.skiplist, t!("site.own_skiplist")) {
            ui.indent("site-own-skip", |ui| crate::options::skiplist_ui(ui, k));
        }
        if let Some(p) = own(ui, &mut o.priorities, &global.priorities, t!("site.own_priorities")) {
            ui.indent("site-own-prio", |ui| crate::options::priorities_ui(ui, p));
        }
        if let Some(e) = own(ui, &mut o.exists, &global.exists, t!("site.own_exists")) {
            ui.indent("site-own-exists", |ui| crate::options::exists_ui(ui, e));
        }
    });

    section(ui, &t!("site.accounts"), |ui| {
        ui.label(RichText::new(t!("site.accounts_note")).weak());
        account_ui(ui, &mut o.upload_account, t!("site.upload_account"), *show_password);
        account_ui(ui, &mut o.download_account, t!("site.download_account"), *show_password);
    });
}

/// Listing settings of a site (FTP): command, spaces, time zone, character set.
pub(crate) fn listing_ui(ui: &mut egui::Ui, l: &mut SiteListing) {
    let command_label = |c: &ListCommand| match c {
        ListCommand::Global => t!("site.list_global"),
        ListCommand::Stat => "STAT -l".to_string(),
        ListCommand::Custom(_) => t!("site.list_custom"),
    };
    egui::Grid::new("site-listing").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        row(ui, &t!("site.list_command"), |ui, lid| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("site-list-cmd")
                    .selected_text(command_label(&l.command))
                    .show_ui(ui, |ui| {
                        for c in [ListCommand::Global, ListCommand::Stat] {
                            let label = command_label(&c);
                            ui.selectable_value(&mut l.command, c, label);
                        }
                        if ui.selectable_label(matches!(l.command, ListCommand::Custom(_)), t!("site.list_custom")).clicked()
                            && !matches!(l.command, ListCommand::Custom(_))
                        {
                            l.command = ListCommand::Custom("LIST -al".into());
                        }
                    })
                    .response
                    .labelled_by(lid);
                if let ListCommand::Custom(c) = &mut l.command {
                    let r = ui.add(theme::single_line(c).hint_text("LIST -al").desired_width(140.0));
                    theme::name(&r, t!("site.list_custom"));
                }
            });
        });
        row(ui, &t!("site.time_offset"), |ui, lid| {
            let mut hours = f64::from(l.time_offset_mins) / 60.0;
            let r = ui.add(egui::DragValue::new(&mut hours).range(-24.0..=24.0).speed(0.25).max_decimals(2).suffix(" h")).labelled_by(lid);
            if r.changed() {
                l.time_offset_mins = (hours * 60.0).round() as i32;
            }
        });
        row(ui, &t!("site.list_parser"), |ui, lid| {
            let label = |p: ListParser| match p {
                ListParser::Auto => t!("site.parser_auto"),
                ListParser::Unix => "Unix".to_string(),
                ListParser::Dos => "DOS / IIS".to_string(),
            };
            egui::ComboBox::from_id_salt("site-list-parser")
                .selected_text(label(l.parser))
                .show_ui(ui, |ui| {
                    for p in [ListParser::Auto, ListParser::Unix, ListParser::Dos] {
                        ui.selectable_value(&mut l.parser, p, label(p));
                    }
                })
                .response
                .labelled_by(lid);
        });
        row(ui, &t!("site.charset"), |ui, lid| {
            let auto = t!("site.charset_utf8");
            let shown = if l.charset.trim().is_empty() { auto.clone() } else { l.charset.clone() };
            egui::ComboBox::from_id_salt("site-charset")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(l.charset.trim().is_empty(), auto).clicked() {
                        l.charset.clear();
                    }
                    for c in CHARSETS {
                        if ui.selectable_label(l.charset == c, c).clicked() {
                            l.charset = c.to_string();
                        }
                    }
                })
                .response
                .labelled_by(lid);
        });
    });
    ui.checkbox(&mut l.trim_spaces, t!("site.trim_spaces"));
}

/// Model of the new sites (Options › New site).
pub(crate) fn new_site_ui(ui: &mut egui::Ui, s: &mut Site) {
    egui::Grid::new("new-site").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        row(ui, &t!("site.protocol"), |ui, l| {
            ui.horizontal(|ui| {
                let before = s.protocol;
                egui::ComboBox::from_id_salt("new-site-proto")
                    .selected_text(proto_label(s.protocol))
                    .show_ui(ui, |ui| {
                        for p in SiteProtocol::ALL {
                            ui.selectable_value(&mut s.protocol, p, proto_label(p));
                        }
                    })
                    .response
                    .labelled_by(l);
                if s.protocol != before && s.port == before.default_port() {
                    s.port = s.protocol.default_port();
                }
                theme::labelled(ui, t!("common.port"), egui::DragValue::new(&mut s.port).range(1..=65535));
            });
        });
        row(ui, &t!("common.user"), |ui, l| {
            ui.horizontal(|ui| {
                ui.add_enabled(!s.anonymous, theme::single_line(&mut s.user).desired_width(200.0)).labelled_by(l);
                ui.checkbox(&mut s.anonymous, t!("site.anonymous"));
            });
        });
        row(ui, &t!("common.password"), |ui, l| {
            ui.add_enabled(!s.anonymous, theme::single_line(&mut s.password).password(true).desired_width(200.0)).labelled_by(l);
        });
        if s.protocol != SiteProtocol::Sftp {
            row(ui, &t!("site.mode"), |ui, _| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.transfer_mode, TransferMode::Passive, t!("site.passive"));
                    ui.radio_value(&mut s.transfer_mode, TransferMode::Active, t!("site.active_mode"));
                });
            });
        }
        if s.protocol.is_tls() {
            row(ui, &t!("site.tls"), |ui, _| {
                ui.checkbox(&mut s.protect_data, t!("site.protect_data"));
            });
        }
        row(ui, &t!("site.parallel"), |ui, l| {
            ui.add(egui::DragValue::new(&mut s.max_transfers).range(0..=50)).labelled_by(l);
        });
        row(ui, &t!("site.max_downloads"), |ui, l| optional_limit(ui, &mut s.max_downloads, l, &t!("site.max_downloads")));
        row(ui, &t!("site.max_uploads"), |ui, l| optional_limit(ui, &mut s.max_uploads, l, &t!("site.max_uploads")));
        row(ui, &t!("site.remote_dir"), |ui, l| {
            ui.add(theme::single_line(&mut s.remote_dir).hint_text("/").desired_width(300.0)).labelled_by(l);
        });
        row(ui, &t!("site.local_dir"), |ui, l| {
            ui.add(theme::single_line(&mut s.local_dir).desired_width(300.0)).labelled_by(l);
        });
    });
    if s.protocol != SiteProtocol::Sftp {
        ui.add_space(6.0);
        ui.strong(t!("site.listing"));
        listing_ui(ui, &mut s.listing);
    }
    if ui.button(t!("common.defaults")).clicked() {
        *s = Site::default();
    }
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).corner_radius(10).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().color(theme::colors(ui).accent_text));
        ui.add_space(4.0);
        add(ui);
    });
}
