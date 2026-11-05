//! Fenêtre « Options » : réglages globaux, enregistrés automatiquement dans `settings.json`.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, RichText};
use urftp_core::exists::{ExistsAction, ExistsRules};
use urftp_core::settings::{GlobalProxy, MaskSetting, Settings, SkipRuleSetting};
use urftp_core::site::{ProxyType, SiteProxy};
use urftp_core::skiplist::{RuleAction, RuleTarget};
use urftp_proto::engine::EngineOptions;
use urftp_proto::proxy::{ProxyConfig, ProxyKind, parse_exceptions};
use urftp_proto::throttle::RateLimiter;

use crate::theme::{ACCENT, LOG_ERROR, LOG_REPLY};

/// Proxy (de site ou global) au format du moteur.
pub fn proxy_config(p: &SiteProxy, bypass_local: bool, exceptions: &str) -> ProxyConfig {
    ProxyConfig {
        kind: match p.kind {
            ProxyType::Socks4 => ProxyKind::Socks4,
            ProxyType::Socks4a => ProxyKind::Socks4a,
            ProxyType::Socks5 => ProxyKind::Socks5,
            ProxyType::HttpTunnel => ProxyKind::HttpTunnel,
        },
        host: p.host.trim().to_string(),
        port: p.port,
        username: (!p.user.is_empty()).then(|| p.user.clone()),
        password: (!p.user.is_empty()).then(|| p.password.clone()),
        apply_to_data: p.data_channel,
        bypass_local,
        exceptions: parse_exceptions(exceptions),
    }
}

/// Proxy global s'il est activé et renseigné.
pub fn global_proxy(g: &GlobalProxy) -> Option<ProxyConfig> {
    (g.enabled && !g.proxy.host.trim().is_empty()).then(|| proxy_config(&g.proxy, g.bypass_local, &g.exceptions))
}

/// Réglages du moteur de file à partir des options.
pub fn engine_options(s: &Settings, download: &Arc<RateLimiter>, upload: &Arc<RateLimiter>) -> EngineOptions {
    EngineOptions {
        retries: s.transfer.retries,
        retry_delay: Duration::from_secs(u64::from(s.transfer.retry_delay_secs)),
        max_total: (s.transfer.max_total > 0).then_some(s.transfer.max_total),
        skiplist: s.skiplist.to_skiplist(),
        priorities: s.priority_list(),
        exists: s.exists,
        download_limit: download.clone(),
        upload_limit: upload.clone(),
        ..EngineOptions::default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    General,
    Transfers,
    Speed,
    Exists,
    Skiplist,
    Priorities,
    Proxy,
    Ident,
}

impl Page {
    const ALL: [Page; 8] =
        [Page::General, Page::Transfers, Page::Speed, Page::Exists, Page::Skiplist, Page::Priorities, Page::Proxy, Page::Ident];

    fn label(self) -> &'static str {
        match self {
            Page::General => "⚙ Général",
            Page::Transfers => "⇅ Transferts",
            Page::Speed => "⏱ Débit",
            Page::Exists => "📄 Fichier existant",
            Page::Skiplist => "🚫 Fichiers ignorés",
            Page::Priorities => "⭐ Priorités",
            Page::Proxy => "🌐 Proxy",
            Page::Ident => "🆔 Ident",
        }
    }
}

pub struct OptionsWindow {
    pub open: bool,
    pub settings: Settings,
    page: Page,
    dirty: bool,
    last_change: f64,
    /// Message d'état du serveur Ident (affiché dans la page Ident).
    pub ident_status: Option<(String, bool)>,
    error: Option<String>,
}

impl OptionsWindow {
    pub fn load() -> Self {
        let (settings, error) = match Settings::load(&Settings::default_path()) {
            Ok(s) => (s, None),
            Err(e) => (Settings::default(), Some(format!("Options illisibles, valeurs par défaut utilisées : {e}"))),
        };
        // Développement : `URUSHFTP_OPEN_OPTIONS=<page>` (0 à 7) ouvre la fenêtre sur cette page.
        let open_on = std::env::var("URUSHFTP_OPEN_OPTIONS").ok().and_then(|v| v.parse::<usize>().ok());
        let page = open_on.and_then(|i| Page::ALL.get(i).copied()).unwrap_or(Page::General);
        Self { open: open_on.is_some(), settings, page, dirty: false, last_change: 0.0, ident_status: None, error }
    }

    pub fn save(&mut self) {
        match self.settings.save(&Settings::default_path()) {
            Ok(()) => self.dirty = false,
            Err(e) => self.error = Some(format!("Enregistrement impossible : {e}")),
        }
    }

    /// Signale une modification faite hors de la fenêtre (thème depuis le menu…).
    pub fn touch(&mut self, ctx: &egui::Context) {
        self.dirty = true;
        self.last_change = ctx.input(|i| i.time);
    }

    /// Affiche la fenêtre ; renvoie `true` si les options ont changé pendant cette image.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let before = self.settings.clone();
        if self.open {
            let mut open = true;
            egui::Window::new("⚙ Options").open(&mut open).default_size([760.0, 520.0]).collapsible(false).show(ctx, |ui| {
                if let Some(e) = &self.error {
                    ui.colored_label(LOG_ERROR, e);
                }
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        for p in Page::ALL {
                            if ui.selectable_label(self.page == p, p.label()).clicked() {
                                self.page = p;
                            }
                        }
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical().id_salt("options-page").show(ui, |ui| self.page_ui(ui));
                    });
                });
            });
            if !open {
                self.open = false;
                if self.dirty {
                    self.save();
                }
            }
        }
        let changed = self.settings != before;
        if changed {
            self.touch(ctx);
        }
        let now = ctx.input(|i| i.time);
        if self.dirty && now - self.last_change > 1.0 {
            self.save();
        } else if self.dirty {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        changed
    }

    fn page_ui(&mut self, ui: &mut egui::Ui) {
        let s = &mut self.settings;
        ui.heading(self.page.label());
        ui.add_space(6.0);
        match self.page {
            Page::General => {
                ui.checkbox(&mut s.dark_theme, "Thème sombre");
                ui.checkbox(&mut s.new_tab_when_busy, "Ouvrir un nouvel onglet quand le panneau droit est déjà connecté");
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Dossier des réglages : {}", urftp_core::sitestore::data_dir().display())).weak());
            }
            Page::Transfers => {
                egui::Grid::new("opt-xfer").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    let t = &mut s.transfer;
                    ui.label("Nouvelles tentatives après un échec");
                    ui.add(egui::DragValue::new(&mut t.retries).range(0..=50));
                    ui.end_row();
                    ui.label("Délai entre deux tentatives (s)");
                    ui.add(egui::DragValue::new(&mut t.retry_delay_secs).range(0..=600));
                    ui.end_row();
                    ui.label("Transferts simultanés au total (0 = sans limite)");
                    ui.add(egui::DragValue::new(&mut t.max_total).range(0..=100));
                    ui.end_row();
                    ui.label("Transferts simultanés en connexion rapide");
                    ui.add(egui::DragValue::new(&mut t.quick_connect_transfers).range(0..=50));
                    ui.end_row();
                    ui.label("Délai de connexion (s)");
                    ui.add(egui::DragValue::new(&mut t.connect_timeout_secs).range(5..=600));
                    ui.end_row();
                    ui.label("Délai sans réponse du serveur (s)");
                    ui.add(egui::DragValue::new(&mut t.io_timeout_secs).range(10..=3600));
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Les délais s'appliquent aux nouvelles connexions.").weak());
            }
            Page::Speed => {
                let t = &mut s.transfer;
                egui::Grid::new("opt-speed").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Téléchargement (Kio/s, 0 = illimité)");
                    ui.add(egui::DragValue::new(&mut t.download_limit_kib).range(0..=10_000_000).speed(16));
                    ui.end_row();
                    ui.label("Envoi (Kio/s, 0 = illimité)");
                    ui.add(egui::DragValue::new(&mut t.upload_limit_kib).range(0..=10_000_000).speed(16));
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Appliqué immédiatement, y compris aux transferts en cours. Le FXP n'est pas concerné (les données ne passent pas par cet ordinateur).").weak());
            }
            Page::Exists => {
                let e = &mut s.exists;
                egui::Grid::new("opt-exists").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    for (label, action) in [
                        ("La cible est plus petite", &mut e.smaller),
                        ("La cible a la même taille", &mut e.same),
                        ("La cible est plus grande", &mut e.larger),
                    ] {
                        ui.label(label);
                        exists_combo(ui, label, action);
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.checkbox(&mut e.overwrite_if_resume_fails, "Recommencer depuis le début si la reprise est refusée");
                if ui.small_button("Valeurs par défaut").clicked() {
                    *e = ExistsRules::default();
                }
            }
            Page::Skiplist => skiplist_ui(ui, s),
            Page::Priorities => priorities_ui(ui, &mut s.priorities),
            Page::Proxy => proxy_ui(ui, &mut s.proxy),
            Page::Ident => {
                let id = &mut s.ident;
                ui.checkbox(&mut id.enabled, "Activer le serveur Ident");
                egui::Grid::new("opt-ident").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Nom répondu");
                    ui.add(egui::TextEdit::singleline(&mut id.name).hint_text(urftp_core::DEFAULT_IDENT_NAME).desired_width(180.0));
                    ui.end_row();
                    ui.label("Système");
                    ui.add(egui::TextEdit::singleline(&mut id.system).desired_width(100.0));
                    ui.end_row();
                    ui.label("Port TCP");
                    ui.add(egui::DragValue::new(&mut id.port).range(1..=65535));
                    ui.end_row();
                });
                ui.add_space(6.0);
                if let Some((msg, ok)) = &self.ident_status {
                    ui.colored_label(if *ok { LOG_REPLY } else { LOG_ERROR }, msg);
                }
                ui.label(
                    RichText::new("Derrière un routeur, redirigez le port choisi vers cet ordinateur. Le nom propre à un site (fiche du site) sera pris en compte dans une prochaine version.")
                        .weak(),
                );
            }
        }
    }
}

fn exists_combo(ui: &mut egui::Ui, id: &str, a: &mut ExistsAction) {
    let label = |a: ExistsAction| match a {
        ExistsAction::Resume => "Reprendre",
        ExistsAction::Skip => "Ignorer",
        ExistsAction::Rename => "Renommer (« nom (1) »)",
        ExistsAction::Overwrite => "Écraser",
        ExistsAction::Ask => "Demander",
    };
    egui::ComboBox::from_id_salt(id).selected_text(label(*a)).width(220.0).show_ui(ui, |ui| {
        for v in [ExistsAction::Resume, ExistsAction::Skip, ExistsAction::Rename, ExistsAction::Overwrite, ExistsAction::Ask] {
            ui.selectable_value(a, v, label(v));
        }
    });
}

/// Champ de masque avec case « regex » et signalement d'une expression invalide.
fn mask_field(ui: &mut egui::Ui, m: &mut MaskSetting, width: f32) {
    let invalid = m.regex && m.compile().is_err();
    let mut edit = egui::TextEdit::singleline(&mut m.mask).desired_width(width).hint_text("*.nfo");
    if invalid {
        edit = edit.text_color(LOG_ERROR);
    }
    ui.add(edit).on_hover_text(if invalid { "Expression régulière invalide" } else { "Joker : * et ?" });
    ui.checkbox(&mut m.regex, "regex");
}

fn skiplist_ui(ui: &mut egui::Ui, s: &mut Settings) {
    let k = &mut s.skiplist;
    ui.checkbox(&mut k.enabled, "Utiliser la liste de fichiers ignorés");
    ui.checkbox(&mut k.skip_zero_byte_files, "Ignorer les fichiers de 0 octet");
    ui.checkbox(&mut k.skip_empty_dirs, "Ne pas créer les dossiers vides");
    ui.add_space(6.0);
    ui.label(RichText::new("Règles appliquées dans l'ordre : la première qui correspond décide. Préfixe %W% = chemin complet.").weak());
    ui.add_space(4.0);
    let mut remove = None;
    let mut swap = None;
    let n = k.rules.len();
    egui::Grid::new("opt-skip").num_columns(8).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
        for (i, r) in k.rules.iter_mut().enumerate() {
            egui::ComboBox::from_id_salt(("skip-act", i))
                .selected_text(if r.action == RuleAction::Skip { "Ignorer" } else { "Autoriser" })
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut r.action, RuleAction::Skip, "Ignorer");
                    ui.selectable_value(&mut r.action, RuleAction::Allow, "Autoriser");
                });
            ui.horizontal(|ui| mask_field(ui, &mut r.mask, 150.0));
            egui::ComboBox::from_id_salt(("skip-tgt", i))
                .selected_text(match r.target {
                    RuleTarget::Files => "Fichiers",
                    RuleTarget::Dirs => "Dossiers",
                    RuleTarget::Both => "Les deux",
                })
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut r.target, RuleTarget::Files, "Fichiers");
                    ui.selectable_value(&mut r.target, RuleTarget::Dirs, "Dossiers");
                    ui.selectable_value(&mut r.target, RuleTarget::Both, "Les deux");
                });
            ui.checkbox(&mut r.directions.upload, "⬆").on_hover_text("Envois");
            ui.checkbox(&mut r.directions.download, "⬇").on_hover_text("Téléchargements");
            ui.checkbox(&mut r.directions.fxp, "↔").on_hover_text("FXP");
            ui.checkbox(&mut r.full_path, "chemin").on_hover_text("Le masque porte sur le chemin complet");
            ui.horizontal(|ui| {
                if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).clicked() {
                    swap = Some((i, i - 1));
                }
                if ui.add_enabled(i + 1 < n, egui::Button::new("⏷").small()).clicked() {
                    swap = Some((i, i + 1));
                }
                if ui.small_button("✕").clicked() {
                    remove = Some(i);
                }
            });
            ui.end_row();
        }
    });
    if let Some((a, b)) = swap {
        k.rules.swap(a, b);
    }
    if let Some(i) = remove {
        k.rules.remove(i);
    }
    if ui.button("➕ Ajouter une règle").clicked() {
        k.rules.push(SkipRuleSetting::default());
    }
}

fn priorities_ui(ui: &mut egui::Ui, list: &mut Vec<MaskSetting>) {
    ui.label(RichText::new("Les éléments correspondants passent en tête de file ; le premier masque est le plus prioritaire.").weak());
    ui.add_space(4.0);
    let mut remove = None;
    let mut swap = None;
    let n = list.len();
    for (i, m) in list.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).color(ACCENT));
            mask_field(ui, m, 220.0);
            if ui.add_enabled(i > 0, egui::Button::new("⏶").small()).clicked() {
                swap = Some((i, i - 1));
            }
            if ui.add_enabled(i + 1 < n, egui::Button::new("⏷").small()).clicked() {
                swap = Some((i, i + 1));
            }
            if ui.small_button("✕").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some((a, b)) = swap {
        list.swap(a, b);
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    if ui.button("➕ Ajouter un masque").clicked() {
        list.push(MaskSetting::default());
    }
}

fn proxy_ui(ui: &mut egui::Ui, g: &mut GlobalProxy) {
    ui.checkbox(&mut g.enabled, "Utiliser un proxy pour les sites réglés sur « Réglage global »");
    ui.add_enabled_ui(g.enabled, |ui| {
        let p = &mut g.proxy;
        egui::Grid::new("opt-proxy").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label("Type");
            ui.horizontal(|ui| {
                for (k, l) in [(ProxyType::Socks4, "SOCKS4"), (ProxyType::Socks4a, "SOCKS4A"), (ProxyType::Socks5, "SOCKS5"), (ProxyType::HttpTunnel, "HTTP Tunnel")] {
                    ui.radio_value(&mut p.kind, k, l);
                }
            });
            ui.end_row();
            ui.label("Hôte et port");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut p.host).desired_width(200.0));
                ui.add(egui::DragValue::new(&mut p.port).range(1..=65535));
            });
            ui.end_row();
            ui.label("Utilisateur");
            ui.add(egui::TextEdit::singleline(&mut p.user).desired_width(160.0));
            ui.end_row();
            ui.label("Mot de passe");
            ui.add(egui::TextEdit::singleline(&mut p.password).password(true).desired_width(160.0));
            ui.end_row();
            ui.label("Exceptions");
            ui.add(egui::TextEdit::singleline(&mut g.exceptions).hint_text("220.67.*;*.lan").desired_width(260.0));
            ui.end_row();
        });
        ui.checkbox(&mut p.data_channel, "Faire passer aussi les transferts par le proxy");
        ui.checkbox(&mut g.bypass_local, "Pas de proxy pour le réseau local");
    });
}
