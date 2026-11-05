//! Gestionnaire de sites : arbre de dossiers et de sites, fiche de réglages, import.

use std::path::PathBuf;

use eframe::egui::{self, Color32, RichText};
use urftp_core::import::{self, ImportResult};
use urftp_core::site::{
    Bookmark, CertPolicy, IdentChoice, ProxyChoice, ProxyType, Site, SiteBook, SiteGroup, SiteProtocol, SiteProxy, TransferMode,
};
use urftp_core::sitestore;
use urftp_proto::ftp::{FtpConfig, Security, TlsOptions, TlsVerify};
use urftp_proto::proxy::{ProxyConfig, ProxyKind};
use urftp_proto::session::{SiteProfile, SiteProtocol as ProfileProtocol};
use urftp_proto::sftp::{HostKeyPolicy, SftpAuth, SftpConfig};

use crate::browser::TrustKind;
use crate::theme::{ACCENT, LOG_ERROR, LOG_REPLY};

/// Profil de connexion à partir d'une fiche de site.
pub fn to_profile(site: &Site) -> SiteProfile {
    let proxy = match &site.proxy {
        ProxyChoice::Custom(p) => Some(ProxyConfig {
            kind: match p.kind {
                ProxyType::Socks4 => ProxyKind::Socks4,
                ProxyType::Socks4a => ProxyKind::Socks4a,
                ProxyType::Socks5 => ProxyKind::Socks5,
                ProxyType::HttpTunnel => ProxyKind::HttpTunnel,
            },
            host: p.host.clone(),
            port: p.port,
            username: (!p.user.is_empty()).then(|| p.user.clone()),
            password: (!p.user.is_empty()).then(|| p.password.clone()),
            apply_to_data: p.data_channel,
            bypass_local: false,
            exceptions: Vec::new(),
        }),
        _ => None,
    };
    let tls = TlsOptions {
        verify: match &site.cert {
            CertPolicy::System => TlsVerify::System,
            CertPolicy::AcceptAny => TlsVerify::AcceptAny,
            CertPolicy::Fingerprint(f) => TlsVerify::Fingerprint(f.clone()),
        },
        protect_data: site.protect_data,
        auth_ssl_fallback: true,
    };
    let ftp = |security| {
        ProfileProtocol::Ftp(FtpConfig {
            security,
            active_mode: site.transfer_mode == TransferMode::Active,
            proxy: proxy.clone(),
            ..FtpConfig::default()
        })
    };
    let protocol = match site.protocol {
        SiteProtocol::Ftp => ftp(Security::None),
        SiteProtocol::FtpsExplicit => ftp(Security::Explicit(tls)),
        SiteProtocol::FtpsImplicit => ftp(Security::Implicit(tls)),
        SiteProtocol::Sftp => ProfileProtocol::Sftp {
            config: SftpConfig { host_key: HostKeyPolicy::Known(site.ssh_host_keys.clone()), proxy, ..SftpConfig::default() },
            key: site.ssh_key_file.as_ref().map(|p| SftpAuth::KeyFile {
                path: PathBuf::from(p),
                passphrase: (!site.password.is_empty()).then(|| site.password.clone()),
            }),
        },
    };
    SiteProfile {
        name: site.name.clone(),
        host: site.host.trim().to_string(),
        port: site.port,
        user: site.login_user().to_string(),
        password: if site.anonymous { "anonymous@".into() } else { site.password.clone() },
        protocol,
        limits: site.limits(),
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Selection {
    Site(u64),
    /// Dossier (indices depuis la racine ; vide = racine).
    Group(Vec<usize>),
}

struct Report {
    title: String,
    lines: Vec<String>,
    error: bool,
}

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
    connect: Option<u64>,
}

/// Fichiers connus d'autres clients présents sur cette machine.
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
                Some(Report { title: "Carnet de sites illisible".into(), lines: vec![format!("{} : {e}", path.display())], error: true }),
            ),
        };
        // Développement : `URUSHFTP_OPEN_SITES=<id>` ouvre le gestionnaire sur ce site.
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
        }
    }

    pub fn save(&mut self) {
        match sitestore::save(&self.path, &self.book) {
            Ok(()) => self.dirty = false,
            Err(e) => {
                self.report = Some(Report { title: "Enregistrement impossible".into(), lines: vec![e.to_string()], error: true })
            }
        }
    }

    /// Site dont l'utilisateur a demandé la connexion.
    pub fn take_connect(&mut self) -> Option<Site> {
        let id = self.connect.take()?;
        self.book.find(id).cloned()
    }

    /// Enregistre une empreinte approuvée dans la fiche du site.
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
                let mut lines = vec![format!("{n} site(s) importé(s) depuis {} ({}).", format.label(), path.display())];
                lines.extend(warnings.into_iter().map(|w| format!("• {w}")));
                self.report = Some(Report { title: "Import terminé".into(), lines, error: false });
            }
            Err(e) => {
                self.report = Some(Report { title: "Import impossible".into(), lines: vec![format!("{} : {e}", path.display())], error: true })
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = true;
        egui::Window::new("☰ Gestionnaire de sites")
            .open(&mut open)
            .default_size([900.0, 600.0])
            .min_width(720.0)
            .collapsible(false)
            .show(ctx, |ui| {
                self.toolbar(ui);
                ui.separator();
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(260.0);
                        egui::ScrollArea::vertical().id_salt("site-tree").max_height(ui.available_height() - 4.0).show(ui, |ui| {
                            let mut clicked = None;
                            tree(ui, &self.book.root, &mut Vec::new(), &self.selection, &mut clicked);
                            if let Some(sel) = clicked {
                                self.selection = sel;
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical().id_salt("site-form").show(ui, |ui| self.form(ui));
                    });
                });
            });
        if !open {
            self.open = false;
            if self.dirty {
                self.save();
            }
        }
        // Enregistrement automatique une seconde après la dernière modification.
        let now = ctx.input(|i| i.time);
        if self.dirty && now - self.last_change > 1.0 {
            self.save();
        } else if self.dirty {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        self.dialogs(ctx);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ Nouveau site").clicked() {
                let group = self.current_group_path();
                let id = self.book.add_site(&group, Site::default());
                self.selection = Selection::Site(id);
                self.mark_dirty(ui);
            }
            if ui.button("📁 Nouveau dossier").clicked() {
                let path = self.current_group_path();
                if let Some(g) = self.book.root.group_mut(&path) {
                    g.groups.push(SiteGroup::new("Nouveau dossier"));
                    let mut p = path;
                    p.push(g.groups.len() - 1);
                    self.selection = Selection::Group(p);
                    self.mark_dirty(ui);
                }
            }
            if let Selection::Site(id) = self.selection
                && ui.button("⧉ Dupliquer").clicked()
                && let Some(new) = self.book.duplicate(id)
            {
                self.selection = Selection::Site(new);
                self.mark_dirty(ui);
            }
            let deletable = !matches!(&self.selection, Selection::Group(p) if p.is_empty());
            if ui.add_enabled(deletable, egui::Button::new("🗑 Supprimer")).clicked() {
                self.confirm_delete = Some(self.selection.clone());
            }
            ui.separator();
            ui.menu_button("📥 Importer", |ui| {
                if ui.button("Choisir un fichier…").clicked() {
                    ui.close();
                    if let Some(p) = rfd::FileDialog::new()
                        .set_title("Importer des sites")
                        .add_filter("Carnets de sites (FileZilla, FTPRush, FlashFXP)", &["xml", "json", "ftp"])
                        .add_filter("Tous les fichiers", &["*"])
                        .pick_file()
                    {
                        self.import(p);
                    }
                }
                let found = known_sources();
                if !found.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Trouvés sur cet ordinateur :").weak());
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
                    let btn = egui::Button::new(RichText::new("⚡ Connecter").color(Color32::WHITE)).fill(ACCENT);
                    if ui.add_enabled(enabled, btn).clicked() {
                        self.save();
                        self.connect = Some(id);
                        self.open = false;
                    }
                }
                ui.label(RichText::new(format!("{} site(s)", self.book.root.site_count())).weak());
            });
        });
    }

    fn mark_dirty(&mut self, ui: &egui::Ui) {
        self.dirty = true;
        self.last_change = ui.input(|i| i.time);
    }

    fn form(&mut self, ui: &mut egui::Ui) {
        match self.selection.clone() {
            Selection::Group(path) => {
                let is_root = path.is_empty();
                let Some(g) = self.book.root.group_mut(&path) else { return };
                ui.heading(if is_root { "☰ Tous les sites" } else { "📁 Dossier" });
                ui.add_space(6.0);
                if !is_root {
                    let before = g.name.clone();
                    ui.horizontal(|ui| {
                        ui.label("Nom du dossier");
                        ui.text_edit_singleline(&mut g.name);
                    });
                    if g.name != before {
                        self.mark_dirty(ui);
                    }
                }
                let n = self.book.root.group_mut(&path).map_or(0, |g| g.site_count());
                ui.label(RichText::new(format!("{n} site(s) dans ce dossier.")).weak());
                ui.add_space(8.0);
                ui.label("Sélectionnez un site pour le modifier, ou créez-en un avec « Nouveau site ».");
            }
            Selection::Site(id) => {
                let show_password = &mut self.show_password;
                let Some(site) = self.book.find_mut(id) else { return };
                let before = site.clone();
                site_form(ui, site, show_password);
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
                Selection::Site(id) => format!("le site « {} »", self.book.find(*id).map_or("?", |s| s.name.as_str())),
                Selection::Group(_) => "ce dossier et tous ses sites".to_string(),
            };
            let mut answer = None;
            egui::Modal::new(egui::Id::new("confirm-delete-site")).show(ctx, |ui| {
                ui.heading("Supprimer ?");
                ui.label(format!("Voulez-vous vraiment supprimer {what} ?"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(RichText::new("Supprimer").color(Color32::WHITE)).fill(LOG_ERROR)).clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Annuler").clicked() {
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
                ui.heading(RichText::new(&r.title).color(if r.error { LOG_ERROR } else { LOG_REPLY }));
                ui.add_space(6.0);
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for l in &r.lines {
                        ui.label(l);
                    }
                });
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    close = true;
                }
            });
            if close {
                self.report = None;
            }
        }
    }
}

fn tree(ui: &mut egui::Ui, g: &SiteGroup, path: &mut Vec<usize>, sel: &Selection, clicked: &mut Option<Selection>) {
    let title = if path.is_empty() { "☰ Tous les sites".to_string() } else { format!("📁 {}", g.name) };
    let header = egui::CollapsingHeader::new(RichText::new(title).strong())
        .id_salt(("grp", path.clone()))
        .default_open(path.len() < 2)
        .show(ui, |ui| {
            for (i, child) in g.groups.iter().enumerate() {
                path.push(i);
                tree(ui, child, path, sel, clicked);
                path.pop();
            }
            for s in &g.sites {
                let icon = match s.protocol {
                    SiteProtocol::Sftp => "🔑",
                    p if p.is_tls() => "🔒",
                    _ => "🌐",
                };
                let mut text = RichText::new(format!("{icon} {}", s.name));
                if !s.enabled {
                    text = text.weak().strikethrough();
                }
                if ui.selectable_label(*sel == Selection::Site(s.id), text).on_hover_text(format!("{}:{}", s.host, s.port)).clicked() {
                    *clicked = Some(Selection::Site(s.id));
                }
            }
        });
    if header.header_response.clicked() {
        *clicked = Some(Selection::Group(path.clone()));
    }
}

fn row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.label(label);
    add(ui);
    ui.end_row();
}

fn optional_limit(ui: &mut egui::Ui, value: &mut Option<u32>) {
    let mut on = value.is_some();
    ui.horizontal(|ui| {
        ui.checkbox(&mut on, "");
        let mut v = value.unwrap_or(1);
        ui.add_enabled(on, egui::DragValue::new(&mut v).range(0..=50));
        *value = on.then_some(v);
    });
}

fn site_form(ui: &mut egui::Ui, s: &mut Site, show_password: &mut bool) {
    ui.heading(format!("{} {}", if s.protocol == SiteProtocol::Sftp { "🔑" } else { "🌐" }, s.name));
    ui.add_space(4.0);
    section(ui, "Connexion", |ui| {
        egui::Grid::new("site-conn").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            row(ui, "Nom", |ui| {
                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(320.0));
            });
            row(ui, "Protocole", |ui| {
                let before = s.protocol;
                egui::ComboBox::from_id_salt("site-proto").selected_text(s.protocol.label()).show_ui(ui, |ui| {
                    for p in SiteProtocol::ALL {
                        ui.selectable_value(&mut s.protocol, p, p.label());
                    }
                });
                if s.protocol != before && s.port == before.default_port() {
                    s.port = s.protocol.default_port();
                }
            });
            row(ui, "Hôte", |ui| {
                ui.add(egui::TextEdit::singleline(&mut s.host).hint_text("ftp.exemple.com").desired_width(240.0));
                ui.label("Port");
                ui.add(egui::DragValue::new(&mut s.port).range(1..=65535));
            });
            row(ui, "Utilisateur", |ui| {
                ui.add_enabled(!s.anonymous, egui::TextEdit::singleline(&mut s.user).desired_width(200.0));
                ui.checkbox(&mut s.anonymous, "Anonyme");
            });
            row(ui, if s.ssh_key_file.is_some() { "Phrase de passe" } else { "Mot de passe" }, |ui| {
                ui.add_enabled(!s.anonymous, egui::TextEdit::singleline(&mut s.password).password(!*show_password).desired_width(200.0));
                if ui.small_button(if *show_password { "🙈" } else { "👁" }).on_hover_text("Afficher / masquer").clicked() {
                    *show_password = !*show_password;
                }
            });
            row(ui, "Actif", |ui| {
                ui.checkbox(&mut s.enabled, "Site activé");
            });
        });
    });

    if s.protocol.is_tls() {
        section(ui, "Sécurité TLS", |ui| {
            ui.checkbox(&mut s.protect_data, "Chiffrer aussi les transferts (PROT P)");
            let label = match &s.cert {
                CertPolicy::System => "Vérifier avec les autorités Windows",
                CertPolicy::AcceptAny => "Accepter tout certificat (déconseillé)",
                CertPolicy::Fingerprint(_) => "Certificat approuvé (empreinte)",
            };
            egui::ComboBox::from_id_salt("site-cert").selected_text(label).width(300.0).show_ui(ui, |ui| {
                if ui.selectable_label(s.cert == CertPolicy::System, "Vérifier avec les autorités Windows").clicked() {
                    s.cert = CertPolicy::System;
                }
                if ui.selectable_label(s.cert == CertPolicy::AcceptAny, "Accepter tout certificat (déconseillé)").clicked() {
                    s.cert = CertPolicy::AcceptAny;
                }
                if ui.selectable_label(matches!(s.cert, CertPolicy::Fingerprint(_)), "Certificat approuvé (empreinte)").clicked()
                    && !matches!(s.cert, CertPolicy::Fingerprint(_))
                {
                    s.cert = CertPolicy::Fingerprint(String::new());
                }
            });
            if let CertPolicy::Fingerprint(fp) = &mut s.cert {
                ui.add(egui::TextEdit::singleline(fp).hint_text("AB:CD:… (rempli à la première connexion)").desired_width(f32::INFINITY));
            }
        });
    }

    if s.protocol == SiteProtocol::Sftp {
        section(ui, "SSH", |ui| {
            let mut use_key = s.ssh_key_file.is_some();
            ui.checkbox(&mut use_key, "Se connecter avec une clé privée (OpenSSH, PEM ou PuTTY .ppk)");
            if use_key {
                let key = s.ssh_key_file.get_or_insert_with(String::new);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(key).desired_width(360.0));
                    if ui.button("Parcourir…").clicked()
                        && let Some(p) = rfd::FileDialog::new().set_title("Clé privée").pick_file()
                    {
                        *key = p.to_string_lossy().into_owned();
                    }
                });
            } else {
                s.ssh_key_file = None;
            }
            ui.label(RichText::new(format!("Clés de serveur approuvées : {}", s.ssh_host_keys.len())).weak());
            let mut remove = None;
            for (i, k) in s.ssh_host_keys.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.code(k);
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                s.ssh_host_keys.remove(i);
            }
        });
    }

    section(ui, "Transferts", |ui| {
        egui::Grid::new("site-xfer").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            if s.protocol != SiteProtocol::Sftp {
                row(ui, "Mode", |ui| {
                    ui.radio_value(&mut s.transfer_mode, TransferMode::Passive, "Passif");
                    ui.radio_value(&mut s.transfer_mode, TransferMode::Active, "Actif");
                });
            }
            row(ui, "Transferts simultanés", |ui| {
                ui.add(egui::DragValue::new(&mut s.max_transfers).range(0..=50));
                ui.label(RichText::new("0 = connexion unique").weak());
            });
            row(ui, "Max. téléchargements", |ui| optional_limit(ui, &mut s.max_downloads));
            row(ui, "Max. envois", |ui| optional_limit(ui, &mut s.max_uploads));
            row(ui, "Dossier distant", |ui| {
                ui.add(egui::TextEdit::singleline(&mut s.remote_dir).hint_text("/").desired_width(300.0));
            });
            row(ui, "Dossier local", |ui| {
                ui.add(egui::TextEdit::singleline(&mut s.local_dir).desired_width(300.0));
                if ui.small_button("…").clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_folder()
                {
                    s.local_dir = p.to_string_lossy().into_owned();
                }
            });
        });
    });

    section(ui, "Proxy et Ident", |ui| {
        let label = match &s.proxy {
            ProxyChoice::Global => "Réglage global",
            ProxyChoice::Direct => "Jamais de proxy",
            ProxyChoice::Custom(_) => "Proxy propre à ce site",
        };
        egui::ComboBox::from_id_salt("site-proxy").selected_text(label).show_ui(ui, |ui| {
            if ui.selectable_label(s.proxy == ProxyChoice::Global, "Réglage global").clicked() {
                s.proxy = ProxyChoice::Global;
            }
            if ui.selectable_label(s.proxy == ProxyChoice::Direct, "Jamais de proxy").clicked() {
                s.proxy = ProxyChoice::Direct;
            }
            if ui.selectable_label(matches!(s.proxy, ProxyChoice::Custom(_)), "Proxy propre à ce site").clicked()
                && !matches!(s.proxy, ProxyChoice::Custom(_))
            {
                s.proxy = ProxyChoice::Custom(SiteProxy { port: 1080, data_channel: true, ..SiteProxy::default() });
            }
        });
        if let ProxyChoice::Custom(p) = &mut s.proxy {
            egui::Grid::new("site-proxy-grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                row(ui, "Type", |ui| {
                    for (k, l) in [(ProxyType::Socks4, "SOCKS4"), (ProxyType::Socks4a, "SOCKS4A"), (ProxyType::Socks5, "SOCKS5"), (ProxyType::HttpTunnel, "HTTP Tunnel")] {
                        ui.radio_value(&mut p.kind, k, l);
                    }
                });
                row(ui, "Hôte", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.host).desired_width(200.0));
                    ui.label("Port");
                    ui.add(egui::DragValue::new(&mut p.port).range(1..=65535));
                });
                row(ui, "Utilisateur", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut p.user).desired_width(140.0));
                    ui.label("Mot de passe");
                    ui.add(egui::TextEdit::singleline(&mut p.password).password(true).desired_width(140.0));
                });
                row(ui, "", |ui| {
                    ui.checkbox(&mut p.data_channel, "Appliquer aussi aux transferts");
                });
            });
        }
        ui.add_space(4.0);
        let mut custom = matches!(s.ident, IdentChoice::Custom(_));
        ui.horizontal(|ui| {
            ui.checkbox(&mut custom, "Nom Ident propre à ce site");
            if custom {
                if let IdentChoice::Global = s.ident {
                    s.ident = IdentChoice::Custom(String::new());
                }
                if let IdentChoice::Custom(n) = &mut s.ident {
                    ui.add(egui::TextEdit::singleline(n).hint_text(urftp_core::DEFAULT_IDENT_NAME).desired_width(160.0));
                }
            } else {
                s.ident = IdentChoice::Global;
            }
        });
    });

    section(ui, "Signets", |ui| {
        let mut remove = None;
        egui::Grid::new("site-bm").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
            for (i, b) in s.bookmarks.iter_mut().enumerate() {
                ui.add(egui::TextEdit::singleline(&mut b.name).hint_text("Nom").desired_width(110.0));
                ui.add(egui::TextEdit::singleline(&mut b.remote_dir).hint_text("Distant").desired_width(170.0));
                ui.add(egui::TextEdit::singleline(&mut b.local_dir).hint_text("Local").desired_width(170.0));
                if ui.small_button("✕").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            s.bookmarks.remove(i);
        }
        if ui.small_button("➕ Ajouter un signet").clicked() {
            s.bookmarks.push(Bookmark::default());
        }
    });

    section(ui, "Notes", |ui| {
        ui.add(egui::TextEdit::multiline(&mut s.notes).desired_rows(3).desired_width(f32::INFINITY));
    });
}

fn section(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).corner_radius(10).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(title).strong().color(ACCENT));
        ui.add_space(4.0);
        add(ui);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_from_site() {
        let s = Site {
            name: "Mon site".into(),
            host: " ftp.example.com ".into(),
            port: 2121,
            protocol: SiteProtocol::FtpsExplicit,
            user: "bob".into(),
            password: "pw".into(),
            cert: CertPolicy::Fingerprint("AA:BB".into()),
            transfer_mode: TransferMode::Active,
            max_transfers: 4,
            proxy: ProxyChoice::Custom(SiteProxy { kind: ProxyType::Socks5, host: "p".into(), port: 1080, ..SiteProxy::default() }),
            ..Site::default()
        };
        let p = to_profile(&s);
        assert_eq!((p.host.as_str(), p.port, p.user.as_str(), p.limits.max_transfers), ("ftp.example.com", 2121, "bob", 4));
        let ProfileProtocol::Ftp(cfg) = &p.protocol else { panic!("FTP attendu") };
        assert!(cfg.active_mode && cfg.proxy.is_some());
        assert!(matches!(&cfg.security, Security::Explicit(o) if o.verify == TlsVerify::Fingerprint("AA:BB".into())));

        let anon = to_profile(&Site { anonymous: true, user: "x".into(), ..Site::default() });
        assert_eq!(anon.user, "anonymous");
        let sftp = to_profile(&Site { protocol: SiteProtocol::Sftp, ssh_key_file: Some("k.ppk".into()), password: "phrase".into(), ..Site::default() });
        let ProfileProtocol::Sftp { key: Some(SftpAuth::KeyFile { passphrase, .. }), .. } = &sftp.protocol else { panic!() };
        assert_eq!(passphrase.as_deref(), Some("phrase"));
    }
}
