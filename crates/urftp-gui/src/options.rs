//! "Options" window: global settings, saved automatically to `settings.json`.

use std::time::Duration;
use urftp_i18n::{Lang, t};

use eframe::egui::{self, RichText};
use urftp_core::exists::{ExistsAction, ExistsRules, ExistsSet};
use urftp_core::settings::{
    GlobalProxy, KeepAliveSettings, LanguageSetting, MaskSetting, QueueAtStart, QueueEnd, Settings, SkipRuleSetting, SkipSettings,
};
use urftp_core::site::ProxyType;
use urftp_core::skiplist::{RuleAction, RuleTarget};

/// Conversions shared with the `urftp` tool.
pub use urftp_proto::profile::engine_options;

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Page {
    General,
    Transfers,
    Speed,
    Exists,
    Skiplist,
    Priorities,
    Proxy,
    Ident,
    KeepAlive,
    Connection,
    NewSite,
    Advanced,
    Browse,
    Interface,
    Shortcuts,
    Notifications,
    Security,
    Commands,
}

impl Page {
    /// Pages in the order of the list; the new ones at the end (the screenshots open a
    /// page by its index).
    pub(crate) const ALL: [Page; 18] = [
        Page::General,
        Page::Transfers,
        Page::Speed,
        Page::Exists,
        Page::Skiplist,
        Page::Priorities,
        Page::Proxy,
        Page::Ident,
        Page::KeepAlive,
        Page::Connection,
        Page::NewSite,
        Page::Advanced,
        Page::Browse,
        Page::Interface,
        Page::Shortcuts,
        Page::Notifications,
        Page::Security,
        Page::Commands,
    ];

    pub(crate) fn label(self) -> String {
        match self {
            Page::General => t!("options.page_general"),
            Page::Transfers => t!("options.page_transfers"),
            Page::Speed => t!("options.page_speed"),
            Page::Exists => t!("options.page_exists"),
            Page::Skiplist => t!("options.page_skiplist"),
            Page::Priorities => t!("options.page_priorities"),
            Page::Proxy => t!("options.page_proxy"),
            Page::Ident => t!("options.page_ident"),
            Page::KeepAlive => t!("options.page_keepalive"),
            Page::Connection => t!("options.page_connection"),
            Page::NewSite => t!("options.page_new_site"),
            Page::Advanced => t!("options.page_advanced"),
            Page::Browse => t!("options.page_browse"),
            Page::Interface => t!("options.page_interface"),
            Page::Shortcuts => t!("options.page_shortcuts"),
            Page::Notifications => t!("options.page_notifications"),
            Page::Security => t!("options.page_security"),
            Page::Commands => t!("options.page_commands"),
        }
    }
}

pub struct OptionsWindow {
    pub open: bool,
    pub settings: Settings,
    page: Page,
    dirty: bool,
    last_change: f64,
    /// Status message of the Ident server (shown on the Ident page).
    pub ident_status: Option<(String, bool)>,
    /// State of the router (UPnP), shown on the Connection page.
    pub upnp_status: Option<(String, bool)>,
    /// "Restart UPnP service" was pressed (read by the application).
    pub upnp_restart: bool,
    error: Option<String>,
    /// Fields of the master password (never stored).
    master: MasterForm,
}

/// Master password fields of the Security page.
#[derive(Default)]
struct MasterForm {
    current: String,
    new: String,
    again: String,
    /// Result of the last change: (message, error?).
    message: Option<(String, bool)>,
}

impl OptionsWindow {
    pub fn load() -> Self {
        let (settings, error) = match Settings::load(&Settings::default_path()) {
            Ok(s) => (s, None),
            Err(e) => (Settings::default(), Some(t!("options.unreadable", error = e))),
        };
        // Development: `URUSHFTP_OPEN_OPTIONS=<page>` (0 to 16) opens the window on that page.
        let open_on = std::env::var("URUSHFTP_OPEN_OPTIONS").ok().and_then(|v| v.parse::<usize>().ok());
        let page = open_on.and_then(|i| Page::ALL.get(i).copied()).unwrap_or(Page::General);
        Self {
            open: open_on.is_some(),
            settings,
            page,
            dirty: false,
            last_change: 0.0,
            ident_status: None,
            upnp_status: None,
            upnp_restart: false,
            error,
            master: MasterForm::default(),
        }
    }

    pub fn save(&mut self) {
        match self.settings.save(&Settings::default_path()) {
            Ok(()) => self.dirty = false,
            Err(e) => self.error = Some(t!("options.save_failed", error = e)),
        }
    }

    /// Reports a change made outside the window (theme from the menu...).
    pub fn touch(&mut self, ctx: &egui::Context) {
        self.dirty = true;
        self.last_change = ctx.input(|i| i.time);
    }

    /// Shows the window; returns `true` when the options changed during this frame.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let before = self.settings.clone();
        if self.open {
            let mut open = true;
            egui::Window::new(t!("options.title"))
                .id(egui::Id::new("options-window"))
                .open(&mut open)
                .default_size([760.0, 520.0])
                // Below the menu and connection bars, which stay usable.
                .default_pos([240.0, 110.0])
                .collapsible(false)
                .show(ctx, |ui| {
                    if let Some(e) = &self.error {
                        ui.colored_label(theme::colors(ui).error, format!("⚠ {e}"));
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
                ui.checkbox(&mut s.dark_theme, t!("options.dark_theme"));
                language_combo(ui, &mut s.language);
                ui.checkbox(&mut s.new_tab_when_busy, t!("options.new_tab_when_busy"));
                ui.checkbox(&mut s.new_tab_both_remote, t!("options.new_tab_both_remote"));
                ui.add_space(6.0);
                egui::Grid::new("opt-general").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    theme::grid_row(
                        ui,
                        t!("options.anonymous_email"),
                        theme::single_line(&mut s.anonymous_email)
                            .hint_text(urftp_core::settings::DEFAULT_ANONYMOUS_EMAIL)
                            .desired_width(220.0),
                    );
                    let l = ui.label(t!("options.download_dir"));
                    ui.horizontal(|ui| {
                        ui.add(theme::single_line(&mut s.download_dir).hint_text(t!("options.download_dir_hint")).desired_width(300.0))
                            .labelled_by(l.id);
                        if theme::icon_button(ui, "…", t!("options.pick_download_dir")).clicked()
                            && let Some(p) = rfd::FileDialog::new().pick_folder()
                        {
                            s.download_dir = p.to_string_lossy().into_owned();
                        }
                    });
                    ui.end_row();
                    let l = ui.label(t!("options.queue_at_start"));
                    let label = |v: QueueAtStart| match v {
                        QueueAtStart::Ask => t!("options.queue_ask"),
                        QueueAtStart::Load => t!("options.queue_load"),
                        QueueAtStart::Discard => t!("options.queue_discard"),
                    };
                    egui::ComboBox::from_id_salt("opt-queue-start")
                        .selected_text(label(s.queue_at_start))
                        .show_ui(ui, |ui| {
                            for v in [QueueAtStart::Ask, QueueAtStart::Load, QueueAtStart::Discard] {
                                ui.selectable_value(&mut s.queue_at_start, v, label(v));
                            }
                        })
                        .response
                        .labelled_by(l.id);
                    ui.end_row();
                });
                ui.checkbox(&mut s.ftprush_dll_window, t!("options.ftprush_dll_window"));
                ui.label(RichText::new(t!("options.ftprush_dll_window_note")).weak());
                ui.add_space(8.0);
                ui.label(RichText::new(t!("options.data_dir", path = urftp_core::sitestore::data_dir().display())).weak());
            }
            Page::Transfers => {
                egui::Grid::new("opt-xfer").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    let t = &mut s.transfer;
                    theme::grid_row(ui, t!("options.retries"), egui::DragValue::new(&mut t.retries).range(0..=50));
                    theme::grid_row(ui, t!("options.retry_delay"), egui::DragValue::new(&mut t.retry_delay_secs).range(0..=600));
                    theme::grid_row(ui, t!("options.max_total"), egui::DragValue::new(&mut t.max_total).range(0..=100));
                    theme::grid_row(ui, t!("options.quick_transfers"), egui::DragValue::new(&mut t.quick_connect_transfers).range(0..=50));
                    theme::grid_row(ui, t!("options.connect_timeout"), egui::DragValue::new(&mut t.connect_timeout_secs).range(5..=600));
                    theme::grid_row(ui, t!("options.io_timeout"), egui::DragValue::new(&mut t.io_timeout_secs).range(10..=3600));
                    theme::grid_row(ui, t!("options.stall"), egui::DragValue::new(&mut t.stall_secs).range(0..=3600).suffix(" s"));
                });
                ui.label(RichText::new(t!("options.stall_note")).weak());
                ui.add_space(6.0);
                ui.label(RichText::new(t!("options.timeouts_note")).weak());
                ui.add_space(8.0);
                retries_ui(ui, &mut s.transfer);
                ui.add_space(8.0);
                queue_end_ui(ui, &mut s.transfer);
            }
            Page::Speed => {
                let t = &mut s.transfer;
                egui::Grid::new("opt-speed").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    theme::grid_row(
                        ui,
                        t!("options.download_limit"),
                        egui::DragValue::new(&mut t.download_limit_kib).range(0..=10_000_000).speed(16),
                    );
                    theme::grid_row(
                        ui,
                        t!("options.upload_limit"),
                        egui::DragValue::new(&mut t.upload_limit_kib).range(0..=10_000_000).speed(16),
                    );
                });
                ui.add_space(6.0);
                ui.checkbox(&mut t.download_limit_on, t!("options.download_limit_on"));
                ui.checkbox(&mut t.upload_limit_on, t!("options.upload_limit_on"));
                ui.label(RichText::new(t!("options.speed_note")).weak());
                ui.label(RichText::new(t!("options.speed_toggle_note")).weak());
            }
            Page::Exists => exists_ui(ui, &mut s.exists),
            Page::Skiplist => {
                skiplist_ui(ui, &mut s.skiplist);
                ui.add_space(8.0);
                skipped_color_ui(ui, &mut s.skiplist);
            }
            Page::Priorities => priorities_ui(ui, &mut s.priorities),
            Page::Proxy => proxy_ui(ui, &mut s.proxy, &mut s.proxies),
            Page::KeepAlive => keep_alive_ui(ui, &mut s.keep_alive),
            Page::Connection => connection_ui(ui, s, self.upnp_status.as_ref(), &mut self.upnp_restart),
            Page::NewSite => {
                ui.label(RichText::new(t!("options.new_site_note")).weak());
                ui.add_space(4.0);
                ui.push_id("opt-new-site", |ui| crate::sites::new_site_ui(ui, &mut s.new_site));
            }
            Page::Advanced => advanced_ui(ui, &mut s.transfer),
            Page::Browse => browse_ui(ui, &mut s.browse),
            Page::Shortcuts => shortcuts_ui(ui, &mut s.interface),
            Page::Notifications => notifications_ui(ui, &mut s.notify),
            Page::Security => security_ui(ui, s, &mut self.master),
            Page::Interface => interface_ui(ui, &mut s.interface),
            Page::Commands => commands_ui(ui, s),
            Page::Ident => {
                let id = &mut s.ident;
                ui.checkbox(&mut id.enabled, t!("options.ident_enable"));
                ui.checkbox(&mut id.only_while_connecting, t!("options.ident_only_connecting"));
                egui::Grid::new("opt-ident").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    theme::grid_row(
                        ui,
                        t!("options.ident_name"),
                        theme::single_line(&mut id.name).hint_text(urftp_core::DEFAULT_IDENT_NAME).desired_width(180.0),
                    );
                    theme::grid_row(ui, t!("options.ident_system"), theme::single_line(&mut id.system).desired_width(100.0));
                    theme::grid_row(ui, t!("options.ident_port"), egui::DragValue::new(&mut id.port).range(1..=65535));
                });
                ui.add_space(6.0);
                if let Some((msg, ok)) = &self.ident_status {
                    let p = theme::colors(ui);
                    ui.colored_label(if *ok { p.success } else { p.error }, format!("{} {msg}", if *ok { "✔" } else { "⚠" }));
                }
                ui.label(RichText::new(t!("options.ident_note")).weak());
            }
        }
    }
}

/// IP address field: red with a warning when the text is not an address.
fn ip_field(ui: &mut egui::Ui, label: String, value: &mut String, hint: &str) {
    let l = ui.label(&label);
    ui.horizontal(|ui| {
        ui.add(theme::single_line(value).hint_text(hint).desired_width(200.0)).labelled_by(l.id);
        if let Some(Err(_)) = urftp_core::settings::check_ip(value) {
            ui.colored_label(theme::colors(ui).error, format!("⚠ {}", t!("options.invalid_ip")));
        }
    });
    ui.end_row();
}

/// Connection page: listing and active mode.
fn connection_ui(ui: &mut egui::Ui, s: &mut Settings, upnp_status: Option<&(String, bool)>, upnp_restart: &mut bool) {
    let l = &mut s.listing;
    ui.strong(t!("options.listing"));
    ui.checkbox(&mut l.use_mlsd, t!("options.use_mlsd"));
    ui.checkbox(&mut l.show_hidden, t!("options.show_hidden"));
    ui.horizontal(|ui| {
        theme::labelled(ui, t!("options.list_command"), theme::single_line(&mut l.custom_command).hint_text("LIST").desired_width(180.0));
    });
    ui.checkbox(&mut l.port_fallback, t!("options.port_fallback"));
    ui.label(RichText::new(t!("options.listing_note")).weak());
    ui.add_space(10.0);

    let n = &mut s.network;
    ui.strong(t!("options.active_mode"));
    egui::Grid::new("opt-network").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        ip_field(ui, t!("options.external_ip"), &mut n.external_ip, &t!("options.automatic"));
        let l = ui.label(t!("options.port_range"));
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut n.port_min).range(0..=65535)).labelled_by(l.id);
            ui.label("–");
            let r = ui.add(egui::DragValue::new(&mut n.port_max).range(0..=65535));
            theme::name(&r, t!("options.port_range_last"));
        });
        ui.end_row();
        ip_field(ui, t!("options.bind_ip"), &mut n.bind_ip, &t!("options.automatic"));
    });
    if n.port_min > 0 && n.port_range().is_none() {
        ui.colored_label(theme::colors(ui).error, format!("⚠ {}", t!("options.port_range_invalid")));
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut n.noop_during_transfers, t!("options.noop_during_transfers"));
        let r = ui.add_enabled(n.noop_during_transfers, egui::DragValue::new(&mut n.noop_interval_secs).range(5..=600).suffix(" s"));
        theme::name(&r, t!("options.noop_interval"));
    });
    ui.label(RichText::new(t!("options.active_note")).weak());
    ui.add_space(10.0);

    ui.strong("UPnP");
    ui.checkbox(&mut n.upnp, t!("options.upnp"));
    if n.upnp {
        ui.horizontal(|ui| {
            if ui.button(t!("options.upnp_restart")).clicked() {
                *upnp_restart = true;
            }
            if let Some((msg, ok)) = upnp_status {
                let p = theme::colors(ui);
                ui.colored_label(if *ok { p.success } else { p.error }, format!("{} {msg}", if *ok { "✔" } else { "⚠" }));
            }
        });
    }
    ui.label(RichText::new(t!("options.upnp_note")).weak());
}

/// Applies the language setting. `URUSHFTP_LANG=en|fr` overrides it (screenshots, tests).
pub fn apply_language(setting: LanguageSetting) {
    let forced = std::env::var("URUSHFTP_LANG").ok().and_then(|v| Lang::from_tag(&v));
    urftp_i18n::set_language(forced.unwrap_or(match setting {
        LanguageSetting::System => Lang::system(),
        LanguageSetting::English => Lang::En,
        LanguageSetting::French => Lang::Fr,
    }));
}

/// Language selector (applied immediately).
fn language_combo(ui: &mut egui::Ui, value: &mut LanguageSetting) {
    let label = |v: LanguageSetting| match v {
        LanguageSetting::System => t!("options.language_system"),
        LanguageSetting::English => Lang::En.native_name().to_string(),
        LanguageSetting::French => Lang::Fr.native_name().to_string(),
    };
    ui.horizontal(|ui| {
        let l = ui.label(t!("options.language"));
        egui::ComboBox::from_id_salt("opt-language")
            .selected_text(label(*value))
            .show_ui(ui, |ui| {
                for v in [LanguageSetting::System, LanguageSetting::English, LanguageSetting::French] {
                    ui.selectable_value(value, v, label(v));
                }
            })
            .response
            .labelled_by(l.id);
    });
}

fn exists_combo(ui: &mut egui::Ui, id: &str, a: &mut ExistsAction) -> egui::Response {
    let label = |a: ExistsAction| match a {
        ExistsAction::Resume => t!("exists.resume"),
        ExistsAction::Skip => t!("exists.skip"),
        ExistsAction::Rename => t!("exists.rename"),
        ExistsAction::Overwrite => t!("exists.overwrite"),
        ExistsAction::Ask => t!("exists.ask"),
    };
    egui::ComboBox::from_id_salt(id)
        .selected_text(label(*a))
        .width(220.0)
        .show_ui(ui, |ui| {
            for v in [ExistsAction::Resume, ExistsAction::Skip, ExistsAction::Rename, ExistsAction::Overwrite, ExistsAction::Ask] {
                ui.selectable_value(a, v, label(v));
            }
        })
        .response
}

/// Mask field with a "regex" box; an invalid expression is shown in red *and* with a
/// written warning (the colour is never the only signal).
fn mask_field(ui: &mut egui::Ui, m: &mut MaskSetting, width: f32, name: &str) {
    let invalid = m.regex && m.compile().is_err();
    let error = theme::colors(ui).error;
    let mut edit = theme::single_line(&mut m.mask).desired_width(width).hint_text("*.nfo");
    if invalid {
        edit = edit.text_color(error);
    }
    let r = ui.add(edit).on_hover_text(if invalid { t!("options.mask_invalid") } else { t!("options.mask_wildcard") });
    theme::name(&r, name);
    let rx = ui.checkbox(&mut m.regex, "regex");
    theme::name(&rx, t!("options.mask_regex", mask = name));
    if invalid {
        ui.colored_label(error, format!("⚠ {}", t!("options.mask_invalid_short")));
    }
}

/// "Existing file" rules (options page, and site record when the site has its own).
/// "Existing file" rules for the uploads, the downloads and FXP (options page, and site
/// record when the site has its own).
pub(crate) fn exists_ui(ui: &mut egui::Ui, e: &mut ExistsSet) {
    use urftp_core::TransferKind as K;
    for (i, (kind, title)) in
        [(K::Upload, t!("options.exists_upload")), (K::Download, t!("options.exists_download")), (K::Fxp, t!("options.exists_fxp"))]
            .into_iter()
            .enumerate()
    {
        ui.label(RichText::new(&title).strong());
        let r = e.get_mut(kind);
        egui::Grid::new(("opt-exists", i)).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            for (label, action) in [
                (t!("options.exists_smaller"), &mut r.smaller),
                (t!("options.exists_same"), &mut r.same),
                (t!("options.exists_larger"), &mut r.larger),
            ] {
                let l = ui.label(&label);
                exists_combo(ui, &format!("{title} — {label}"), action).labelled_by(l.id);
                ui.end_row();
            }
        });
        ui.add_space(6.0);
    }
    let mut restart = e.upload.overwrite_if_resume_fails;
    if ui.checkbox(&mut restart, t!("options.exists_restart")).changed() {
        for k in [K::Upload, K::Download, K::Fxp] {
            e.get_mut(k).overwrite_if_resume_fails = restart;
        }
    }
    if ui.button(t!("common.defaults")).clicked() {
        *e = ExistsSet::all(ExistsRules::default());
    }
}

/// Skip list (options page, and site record when the site has its own).
pub(crate) fn skiplist_ui(ui: &mut egui::Ui, k: &mut SkipSettings) {
    ui.checkbox(&mut k.enabled, t!("options.skip_enable"));
    ui.horizontal(|ui| {
        let note = ui.id().with("skip-file-note");
        if ui.button(t!("options.skip_export")).clicked()
            && let Some(p) = rfd::FileDialog::new().set_file_name("skiplist.json").add_filter("JSON", &["json"]).save_file()
        {
            let msg = match std::fs::write(&p, urftp_core::settings::skip_rules_to_text(&k.rules)) {
                Ok(()) => t!("options.skip_exported", n = k.rules.len(), path = p.display()),
                Err(e) => t!("options.skip_file_failed", error = e),
            };
            ui.data_mut(|d| d.insert_temp(note, msg));
        }
        if ui.button(t!("options.skip_import")).clicked()
            && let Some(p) = rfd::FileDialog::new().add_filter("JSON / texte", &["json", "txt"]).pick_file()
        {
            let msg = match std::fs::read_to_string(&p) {
                Ok(text) => {
                    let rules = urftp_core::settings::skip_rules_from_text(&text);
                    let n = rules.len();
                    k.rules.extend(rules);
                    t!("options.skip_imported", n = n)
                }
                Err(e) => t!("options.skip_file_failed", error = e),
            };
            ui.data_mut(|d| d.insert_temp(note, msg));
        }
        if let Some(msg) = ui.data(|d| d.get_temp::<String>(note)) {
            ui.label(RichText::new(msg).weak());
        }
    });
    ui.checkbox(&mut k.skip_zero_byte_files, t!("options.skip_zero"));
    ui.checkbox(&mut k.skip_empty_dirs, t!("options.skip_empty_dirs"));
    ui.horizontal(|ui| {
        use urftp_core::skiplist::SkipMode;
        ui.label(t!("options.skip_mode"));
        ui.radio_value(&mut k.mode, SkipMode::Auto, t!("options.skip_mode_auto"));
        ui.radio_value(&mut k.mode, SkipMode::Manual, t!("options.skip_mode_manual"));
        ui.radio_value(&mut k.mode, SkipMode::Both, t!("options.skip_mode_both"));
    });
    ui.label(RichText::new(t!("options.skip_mode_note")).weak());
    ui.add_space(6.0);
    ui.label(RichText::new(t!("options.skip_note")).weak());
    ui.add_space(4.0);
    let mut remove = None;
    let mut swap = None;
    let n = k.rules.len();
    egui::Grid::new("opt-skip").num_columns(8).spacing([6.0, 4.0]).striped(true).show(ui, |ui| {
        for (i, r) in k.rules.iter_mut().enumerate() {
            egui::ComboBox::from_id_salt(("skip-act", i))
                .selected_text(if r.action == RuleAction::Skip { t!("skip.skip") } else { t!("skip.allow") })
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut r.action, RuleAction::Skip, t!("skip.skip"));
                    ui.selectable_value(&mut r.action, RuleAction::Allow, t!("skip.allow"));
                });
            ui.horizontal(|ui| mask_field(ui, &mut r.mask, 150.0, &t!("options.rule_mask", n = i + 1)));
            egui::ComboBox::from_id_salt(("skip-tgt", i))
                .selected_text(match r.target {
                    RuleTarget::Files => t!("skip.files"),
                    RuleTarget::Dirs => t!("skip.dirs"),
                    RuleTarget::Both => t!("skip.both"),
                })
                .width(90.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut r.target, RuleTarget::Files, t!("skip.files"));
                    ui.selectable_value(&mut r.target, RuleTarget::Dirs, t!("skip.dirs"));
                    ui.selectable_value(&mut r.target, RuleTarget::Both, t!("skip.both"));
                });
            for (value, icon, label) in [
                (&mut r.directions.upload, "⬆", t!("options.uploads")),
                (&mut r.directions.download, "⬇", t!("options.downloads")),
                (&mut r.directions.fxp, "↔", t!("queue.kind_fxp")),
            ] {
                let c = ui.checkbox(value, icon).on_hover_text(&label);
                theme::name(&c, label);
            }
            ui.checkbox(&mut r.full_path, t!("options.path")).on_hover_text(t!("options.path_hint"));
            ui.horizontal(|ui| reorder_buttons(ui, i, n, &mut swap, &mut remove));
            ui.end_row();
        }
    });
    if let Some((a, b)) = swap {
        k.rules.swap(a, b);
    }
    if let Some(i) = remove {
        k.rules.remove(i);
    }
    if ui.button(t!("options.skip_add")).clicked() {
        k.rules.push(SkipRuleSetting::default());
    }
}

/// Move up / move down / remove buttons of row `i` out of `n`.
fn reorder_buttons(ui: &mut egui::Ui, i: usize, n: usize, swap: &mut Option<(usize, usize)>, remove: &mut Option<usize>) {
    let up = ui.add_enabled_ui(i > 0, |ui| theme::icon_button(ui, "⏶", t!("common.move_up"))).inner;
    if up.clicked() {
        *swap = Some((i, i - 1));
    }
    let down = ui.add_enabled_ui(i + 1 < n, |ui| theme::icon_button(ui, "⏷", t!("common.move_down"))).inner;
    if down.clicked() {
        *swap = Some((i, i + 1));
    }
    if theme::icon_button(ui, "✕", t!("common.remove")).clicked() {
        *remove = Some(i);
    }
}

pub(crate) fn priorities_ui(ui: &mut egui::Ui, list: &mut Vec<MaskSetting>) {
    ui.label(RichText::new(t!("options.prio_note")).weak());
    ui.add_space(4.0);
    let mut remove = None;
    let mut swap = None;
    let n = list.len();
    for (i, m) in list.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).color(theme::colors(ui).accent_text));
            mask_field(ui, m, 220.0, &t!("options.prio_mask", n = i + 1));
            reorder_buttons(ui, i, n, &mut swap, &mut remove);
        });
    }
    if let Some((a, b)) = swap {
        list.swap(a, b);
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    if ui.button(t!("options.prio_add")).clicked() {
        list.push(MaskSetting::default());
    }
}

/// Keep alive and automatic reconnection.
pub(crate) fn keep_alive_ui(ui: &mut egui::Ui, k: &mut KeepAliveSettings) {
    ui.checkbox(&mut k.enabled, t!("options.ka_enable"));
    ui.add_enabled_ui(k.enabled, |ui| {
        egui::Grid::new("opt-keepalive").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            theme::grid_row(ui, t!("options.ka_commands"), theme::single_line(&mut k.commands).hint_text("NOOP;PWD").desired_width(260.0));
            theme::grid_row(ui, t!("options.ka_interval"), egui::DragValue::new(&mut k.interval_secs).range(5..=3600).suffix(" s"));
            theme::grid_row(ui, t!("options.ka_stop_after"), egui::DragValue::new(&mut k.stop_after_mins).range(0..=1440).suffix(" min"));
        });
        ui.checkbox(&mut k.random, t!("options.ka_random"));
        ui.add_enabled(k.stop_after_mins > 0, egui::Checkbox::new(&mut k.logout_after_stop, t!("options.ka_logout")));
    });
    ui.add_space(10.0);
    ui.label(RichText::new(t!("options.ka_reconnect_title")).strong());
    ui.checkbox(&mut k.reconnect_when_kicked, t!("options.ka_reconnect_kicked"));
    egui::Grid::new("opt-reconnect").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        theme::grid_row(
            ui,
            t!("options.ka_reconnect_replies"),
            theme::single_line(&mut k.reconnect_replies).hint_text("421*;530*too many*").desired_width(260.0),
        );
    });
    ui.add_space(6.0);
    ui.label(RichText::new(t!("options.ka_note")).weak());
}

/// Fields of a proxy (type, host, port, login, data channels).
pub(crate) fn proxy_fields(ui: &mut egui::Ui, id: &str, p: &mut urftp_core::site::SiteProxy) {
    egui::Grid::new(id).num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        ui.label(t!("common.type"));
        ui.horizontal(|ui| {
            for (k, l) in [
                (ProxyType::Socks4, "SOCKS4"),
                (ProxyType::Socks4a, "SOCKS4A"),
                (ProxyType::Socks5, "SOCKS5"),
                (ProxyType::HttpTunnel, "HTTP Tunnel"),
            ] {
                ui.radio_value(&mut p.kind, k, l);
            }
        });
        ui.end_row();
        let l = ui.label(t!("options.proxy_host_port"));
        ui.horizontal(|ui| {
            let h = ui.add(theme::single_line(&mut p.host).desired_width(200.0));
            theme::name(&h, t!("common.host"));
            ui.add(egui::DragValue::new(&mut p.port).range(1..=65535)).labelled_by(l.id);
        });
        ui.end_row();
        theme::grid_row(ui, t!("common.user"), theme::single_line(&mut p.user).desired_width(160.0));
        theme::grid_row(ui, t!("common.password"), theme::single_line(&mut p.password).password(true).desired_width(160.0));
    });
    ui.checkbox(&mut p.data_channel, t!("options.proxy_data"));
}

/// Proxies chosen by name in the site records.
fn named_proxies_ui(ui: &mut egui::Ui, list: &mut Vec<urftp_core::settings::NamedProxy>) {
    ui.strong(t!("options.named_proxies"));
    ui.label(RichText::new(t!("options.named_proxies_note")).weak());
    let mut remove = None;
    for (i, n) in list.iter_mut().enumerate() {
        ui.push_id(("named-proxy", i), |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    theme::labelled(ui, t!("common.name"), theme::single_line(&mut n.name).desired_width(180.0));
                    if theme::icon_button(ui, "✕", t!("options.remove_proxy", name = n.name)).clicked() {
                        remove = Some(i);
                    }
                });
                proxy_fields(ui, "named-proxy-grid", &mut n.proxy);
            });
        });
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    if ui.button(t!("options.add_proxy")).clicked() {
        let mut k = list.len() + 1;
        while list.iter().any(|p| p.name == format!("Proxy {k}")) {
            k += 1;
        }
        list.push(urftp_core::settings::NamedProxy {
            name: format!("Proxy {k}"),
            proxy: urftp_core::site::SiteProxy { port: 1080, data_channel: true, ..Default::default() },
        });
    }
}

/// Retries per direction and the queue rules about failed items.
fn retries_ui(ui: &mut egui::Ui, t: &mut urftp_core::settings::TransferSettings) {
    ui.label(RichText::new(t!("options.retry_section")).strong());
    {
        egui::Grid::new("opt-retries").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            for (label, value) in [
                (t!("options.retries_download"), &mut t.retries_download),
                (t!("options.retries_upload"), &mut t.retries_upload),
                (t!("options.retries_fxp"), &mut t.retries_fxp),
            ] {
                let mut own = value.is_some();
                let c = ui.checkbox(&mut own, &label);
                theme::name(&c, &label);
                ui.horizontal(|ui| {
                    let mut n = value.unwrap_or(t.retries);
                    let r = ui.add_enabled(own, egui::DragValue::new(&mut n).range(0..=50));
                    theme::name(&r, &label);
                    *value = own.then_some(n);
                });
                ui.end_row();
            }
            theme::grid_row(ui, t!("options.retry_rounds"), egui::DragValue::new(&mut t.retry_failed_rounds).range(0..=20));
        });
        ui.checkbox(&mut t.cycle_retry, t!("options.cycle_retry"));
        ui.checkbox(&mut t.reset_failed_on_auto_start, t!("options.reset_failed_auto"));
        ui.checkbox(&mut t.clear_queue_on_logout, t!("options.clear_queue_logout"));
        ui.checkbox(&mut t.log_paths, t!("options.log_paths"));
    }
}

/// Translated name of a transfer mode.
pub(crate) fn mode_label(m: urftp_core::TransferMode) -> String {
    match m {
        urftp_core::TransferMode::Auto => t!("mode.auto"),
        urftp_core::TransferMode::Binary => t!("mode.binary"),
        urftp_core::TransferMode::Ascii => t!("mode.ascii"),
    }
}

fn case_label(c: urftp_core::NameCase) -> String {
    match c {
        urftp_core::NameCase::Keep => t!("case.keep"),
        urftp_core::NameCase::Lower => t!("case.lower"),
        urftp_core::NameCase::Upper => t!("case.upper"),
    }
}

/// "Advanced transfer" page: modes, names, data connections, disk and speed, CRC.
/// "Interface" page: logs and status bar.
pub(crate) fn interface_ui(ui: &mut egui::Ui, s: &mut urftp_core::settings::InterfaceSettings) {
    ui.label(RichText::new(t!("options.ui_logs_section")).strong());
    ui.checkbox(&mut s.log_site_names, t!("options.ui_log_site_names"));
    ui.checkbox(&mut s.log_times, t!("options.ui_log_times"));
    egui::Grid::new("opt-ui-logs").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        theme::grid_row(ui, t!("options.ui_log_lines"), egui::DragValue::new(&mut s.log_lines).range(100..=1_000_000).speed(50));
    });
    ui.checkbox(&mut s.log_to_disk, t!("options.ui_log_to_disk"));
    ui.horizontal(|ui| {
        let l = ui.label(t!("options.ui_log_dir"));
        ui.add_enabled(s.log_to_disk, theme::single_line(&mut s.log_dir).hint_text(t!("options.ui_log_dir_hint")).desired_width(320.0))
            .labelled_by(l.id);
        if ui.add_enabled(s.log_to_disk, egui::Button::new(t!("common.browse"))).clicked()
            && let Some(d) = rfd::FileDialog::new().pick_folder()
        {
            s.log_dir = d.to_string_lossy().into_owned();
        }
    });
    ui.label(RichText::new(t!("options.ui_log_files_note")).weak());
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.ui_status_section")).strong());
    egui::Grid::new("opt-ui-status").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        theme::grid_row(ui, t!("options.ui_status_format"), theme::single_line(&mut s.status_format).desired_width(260.0));
    });
    ui.label(RichText::new(t!("options.ui_status_hint")).weak());
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.ui_clipboard_section")).strong());
    ui.checkbox(&mut s.watch_clipboard, t!("options.ui_watch_clipboard"));
    ui.add_enabled(s.watch_clipboard, egui::Checkbox::new(&mut s.clipboard_auto, t!("options.ui_clipboard_auto")));
    ui.add_space(8.0);
    display_ui(ui, s);
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.ui_mouse_section")).strong());
    double_click_ui(ui, s);
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.ui_confirm_section")).strong());
    ui.checkbox(&mut s.confirm_exit, t!("options.ui_confirm_exit"));
    ui.checkbox(&mut s.confirm_close_tab, t!("options.ui_confirm_close_tab"));
    ui.checkbox(&mut s.confirm_queue_remove, t!("options.ui_confirm_queue"));
}

/// A choice among values in a drop-down list, with its label.
fn choice<T: PartialEq + Copy>(ui: &mut egui::Ui, id: &str, label: &str, value: &mut T, items: &[(T, String)]) {
    ui.horizontal(|ui| {
        let l = ui.label(label);
        let current = items.iter().find(|(v, _)| v == value).map(|(_, t)| t.clone()).unwrap_or_default();
        let combo = egui::ComboBox::from_id_salt(id).selected_text(current).width(220.0).show_ui(ui, |ui| {
            for (v, t) in items {
                ui.selectable_value(value, *v, t.clone());
            }
        });
        combo.response.labelled_by(l.id);
    });
}

/// What the double-clicks do: file, log line, site, side indicator, tab, queue item.
fn double_click_ui(ui: &mut egui::Ui, s: &mut urftp_core::settings::InterfaceSettings) {
    use urftp_core::settings::{
        FileDoubleClick as F, LogDoubleClick as L, QueueDoubleClick as Q, SideDoubleClick as D, SiteDoubleClick as S, TabDoubleClick as T,
    };
    let nothing = t!("options.ui_dbl_nothing");
    let items = [(F::Transfer, t!("options.ui_dbl_transfer")), (F::View, t!("options.ui_dbl_view")), (F::Nothing, nothing.clone())];
    choice(ui, "opt-dbl", &t!("options.ui_double_click"), &mut s.double_click_file, &items);
    let items = [(L::Copy, t!("options.ui_dbl_log_copy")), (L::Nothing, nothing.clone())];
    choice(ui, "opt-dbl-log", &t!("options.ui_dbl_log"), &mut s.double_click_log, &items);
    let items = [(S::Connect, t!("options.ui_dbl_site_connect")), (S::Nothing, nothing.clone())];
    choice(ui, "opt-dbl-site", &t!("options.ui_dbl_site"), &mut s.double_click_site, &items);
    let items = [
        (D::Nothing, nothing.clone()),
        (D::Refresh, t!("options.ui_dbl_side_refresh")),
        (D::Parent, t!("options.ui_dbl_side_parent")),
        (D::Switch, t!("options.ui_dbl_side_switch")),
    ];
    choice(ui, "opt-dbl-side", &t!("options.ui_dbl_side"), &mut s.double_click_side, &items);
    let items =
        [(T::Nothing, nothing.clone()), (T::Duplicate, t!("options.ui_dbl_tab_duplicate")), (T::Close, t!("options.ui_dbl_tab_close"))];
    choice(ui, "opt-dbl-tab", &t!("options.ui_dbl_tab"), &mut s.double_click_tab, &items);
    let items = [
        (Q::Nothing, nothing),
        (Q::Edit, t!("options.ui_dbl_queue_edit")),
        (Q::Retry, t!("options.ui_dbl_queue_retry")),
        (Q::Remove, t!("options.ui_dbl_queue_remove")),
    ];
    choice(ui, "opt-dbl-queue", &t!("options.ui_dbl_queue"), &mut s.double_click_queue, &items);
}

/// Fonts of the interface, the file lists and the logs.
fn fonts_ui(ui: &mut egui::Ui, s: &mut urftp_core::settings::InterfaceSettings) {
    egui::Grid::new("opt-fonts").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        for (key, value, id) in [
            ("options.look_font_app", &mut s.font_app, "opt-font-app"),
            ("options.look_font_lists", &mut s.font_lists, "opt-font-lists"),
            ("options.look_font_log", &mut s.font_log, "opt-font-log"),
        ] {
            let l = ui.label(urftp_i18n::text(key));
            let shown = if value.trim().is_empty() { t!("options.look_font_theme") } else { theme::font_label(value.trim()) };
            // The fonts folder is read when a list is first opened.
            let combo = egui::ComboBox::from_id_salt(id).selected_text(shown).width(260.0).height(320.0).show_ui(ui, |ui| {
                ui.selectable_value(value, String::new(), t!("options.look_font_theme"));
                for (file, name) in theme::system_fonts() {
                    ui.selectable_value(value, file.clone(), name);
                }
            });
            combo.response.labelled_by(l.id);
            ui.end_row();
        }
    });
    ui.label(RichText::new(t!("options.look_font_note")).weak());
}

/// "Security" page: master password, single instance, daily backup.
fn security_ui(ui: &mut egui::Ui, s: &mut Settings, f: &mut MasterForm) {
    use urftp_core::master;
    ui.label(RichText::new(t!("options.sec_master_section")).strong());
    ui.label(RichText::new(t!("options.sec_master_note")).weak());
    let has = !s.master_hash.is_empty();
    ui.label(if has { t!("options.sec_master_set") } else { t!("options.sec_master_none") });
    egui::Grid::new("opt-master").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        if has {
            theme::grid_row(ui, t!("options.sec_current"), theme::single_line(&mut f.current).password(true).desired_width(220.0));
        }
        theme::grid_row(ui, t!("options.sec_new"), theme::single_line(&mut f.new).password(true).desired_width(220.0));
        theme::grid_row(ui, t!("options.sec_again"), theme::single_line(&mut f.again).password(true).desired_width(220.0));
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!f.new.is_empty(), egui::Button::new(if has { t!("options.sec_change") } else { t!("options.sec_set") }))
            .clicked()
        {
            let current_ok = !has || master::verify_password(&f.current, &s.master_hash);
            f.message = Some(if !current_ok {
                (t!("lock.wrong"), true)
            } else if f.new != f.again {
                (t!("options.sec_mismatch"), true)
            } else {
                match master::hash_password(&f.new) {
                    Ok(h) => {
                        s.master_hash = h;
                        (t!("options.sec_saved"), false)
                    }
                    Err(e) => (e.to_string(), true),
                }
            });
            f.current.clear();
            f.new.clear();
            f.again.clear();
        }
        if has && ui.button(t!("options.sec_remove")).clicked() {
            let current_ok = master::verify_password(&f.current, &s.master_hash);
            f.message = Some(if current_ok {
                s.master_hash.clear();
                (t!("options.sec_removed"), false)
            } else {
                (t!("lock.wrong"), true)
            });
            f.current.clear();
        }
    });
    if let Some((m, err)) = &f.message {
        let p = theme::colors(ui);
        ui.colored_label(if *err { p.error } else { p.success }, m);
    }
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.sec_start_section")).strong());
    ui.checkbox(&mut s.single_instance, t!("options.sec_single"));
    ui.label(RichText::new(t!("options.sec_single_note")).weak());
    ui.checkbox(&mut s.backup_daily, t!("options.sec_backup_daily"));
    ui.label(RichText::new(t!("options.sec_backup_note")).weak());
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.sec_shell_section")).strong());
    {
        use urftp_core::settings::ShellPolicy as P;
        let label = |p: P| match p {
            P::Ask => t!("options.sec_shell_ask"),
            P::Allow => t!("options.sec_shell_allow"),
            P::Deny => t!("options.sec_shell_deny"),
        };
        ui.horizontal(|ui| {
            let l = ui.label(t!("options.sec_shell"));
            let combo = egui::ComboBox::from_id_salt("opt-shell").selected_text(label(s.script_shell)).width(300.0).show_ui(ui, |ui| {
                for p in [P::Ask, P::Allow, P::Deny] {
                    ui.selectable_value(&mut s.script_shell, p, label(p));
                }
            });
            combo.response.labelled_by(l.id);
        });
    }
    ui.label(RichText::new(t!("options.sec_shell_note")).weak());
    ui.label(t!("options.sec_shell_programs"));
    if s.script_programs.is_empty() {
        ui.label(RichText::new(t!("options.sec_shell_none")).italics());
    }
    let mut remove = None;
    for (i, prog) in s.script_programs.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(prog).monospace());
            if theme::icon_button(ui, "✕", t!("options.sec_shell_forget", program = prog)).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        s.script_programs.remove(i);
    }
}

/// "Notifications" page: tray icon, events, monitored folders.
fn notifications_ui(ui: &mut egui::Ui, n: &mut urftp_core::settings::NotifySettings) {
    use urftp_core::settings::{EventSetting, FolderMonitor, NotifyEvent};
    ui.label(RichText::new(t!("options.nt_tray_section")).strong());
    ui.checkbox(&mut n.tray_always, t!("options.nt_tray_always"));
    ui.checkbox(&mut n.minimize_to_tray, t!("options.nt_minimize"));
    ui.checkbox(&mut n.flash_on_queue_end, t!("options.nt_flash"));
    ui.checkbox(&mut n.flash_while_transferring, t!("options.nt_flash_transfers"));
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.nt_events_section")).strong());
    for e in NotifyEvent::ALL {
        if !n.events.iter().any(|s| s.event == e) {
            n.events.push(EventSetting::default_for(e));
        }
    }
    egui::Grid::new("opt-events").num_columns(5).spacing([10.0, 6.0]).striped(true).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new(t!("options.nt_sound")).weak());
        ui.label("");
        ui.label("");
        ui.label(RichText::new(t!("options.nt_popup")).weak());
        ui.end_row();
        for s in &mut n.events {
            let name = match s.event {
                NotifyEvent::Connected => t!("notify.ev_connected"),
                NotifyEvent::Disconnected => t!("notify.ev_disconnected"),
                NotifyEvent::QueueFinished => t!("notify.ev_queue_finished"),
                NotifyEvent::TransferFailed => t!("notify.ev_transfer_failed"),
                NotifyEvent::NewItems => t!("notify.ev_new_items"),
            };
            ui.label(&name);
            let r = ui.add(theme::single_line(&mut s.sound).hint_text(t!("options.nt_sound_hint")).desired_width(220.0));
            theme::name(&r, format!("{name}: {}", t!("options.nt_sound")));
            if ui.button(t!("common.browse")).clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("WAV", &["wav"]).pick_file()
            {
                s.sound = p.to_string_lossy().into_owned();
            }
            if theme::icon_button(ui, "▶", format!("{name}: {}", t!("options.nt_try"))).clicked() {
                match s.sound.trim() {
                    "" => {}
                    "system" => urftp_sys::tray::play_sound(None),
                    f => urftp_sys::tray::play_sound(Some(std::path::Path::new(f))),
                }
            }
            let r = ui.checkbox(&mut s.popup, "");
            theme::name(&r, format!("{name}: {}", t!("options.nt_popup")));
            ui.end_row();
        }
    });
    ui.label(RichText::new(t!("options.nt_events_note")).weak());
    ui.add_space(8.0);
    ui.label(RichText::new(t!("options.nt_monitor_section")).strong());
    let mut remove = None;
    egui::Grid::new("opt-monitors").num_columns(5).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new(t!("options.nt_site")).weak());
        ui.label(RichText::new(t!("options.nt_folder")).weak());
        ui.label(RichText::new(t!("options.nt_every")).weak());
        ui.end_row();
        for (i, m) in n.monitors.iter_mut().enumerate() {
            let r = ui.checkbox(&mut m.enabled, "");
            theme::name(&r, format!("{} {}", t!("options.nt_monitor_on"), i + 1));
            let r = ui.add(theme::single_line(&mut m.site).desired_width(150.0));
            theme::name(&r, format!("{} {}", t!("options.nt_site"), i + 1));
            let r = ui.add(theme::single_line(&mut m.path).desired_width(200.0));
            theme::name(&r, format!("{} {}", t!("options.nt_folder"), i + 1));
            let r = ui.add(egui::DragValue::new(&mut m.interval_mins).range(1..=1440).suffix(" min"));
            theme::name(&r, format!("{} {}", t!("options.nt_every"), i + 1));
            if theme::icon_button(ui, "✕", t!("common.remove")).clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        n.monitors.remove(i);
    }
    if ui.button(t!("options.browse_add")).clicked() {
        n.monitors.push(FolderMonitor::default());
    }
    ui.label(RichText::new(t!("options.nt_monitor_note")).weak());
}

/// Colour of the items the skip list would skip, in the panels.
fn skipped_color_ui(ui: &mut egui::Ui, k: &mut SkipSettings) {
    ui.checkbox(&mut k.show_skipped, t!("options.skip_show"));
    ui.add_enabled_ui(k.show_skipped, |ui| {
        egui::Grid::new("opt-skip-color").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(t!("options.look_dark"));
            color_field(ui, &mut k.skipped_dark, true, &format!("{} ({})", t!("options.skip_show"), t!("options.look_dark")));
            ui.end_row();
            ui.label(t!("options.look_light"));
            color_field(ui, &mut k.skipped_light, false, &format!("{} ({})", t!("options.skip_show"), t!("options.look_light")));
            ui.end_row();
        });
    });
    ui.label(RichText::new(t!("options.skip_show_note")).weak());
}

/// A colour field `#RRGGBB` with a sample and its contrast against the list backgrounds of
/// the theme (WCAG AA asks for 4.5:1).
fn color_field(ui: &mut egui::Ui, value: &mut String, dark: bool, name: &str) {
    let r = ui.add(theme::single_line(value).desired_width(80.0).font(egui::TextStyle::Monospace));
    theme::name(&r, name.to_string());
    match theme::parse_color(value) {
        Some(c) => {
            let ratio = theme::list_contrast(c, dark);
            let ok = ratio >= 4.5;
            let mark = if ok { "✔" } else { "⚠" };
            let p = theme::colors(ui);
            ui.label(RichText::new("■■").color(c));
            ui.label(RichText::new(format!("{mark} {ratio:.1}:1")).color(if ok { p.success } else { p.warning }))
                .on_hover_text(t!("options.look_contrast_hint"));
        }
        None => {
            ui.colored_label(theme::colors(ui).error, t!("options.look_bad_color"));
            ui.label("");
        }
    }
}

/// Size, sort, tabs, highlights and log colours.
fn display_ui(ui: &mut egui::Ui, s: &mut urftp_core::settings::InterfaceSettings) {
    ui.label(RichText::new(t!("options.look_section")).strong());
    egui::Grid::new("opt-look").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        theme::grid_row(ui, t!("options.look_zoom"), egui::DragValue::new(&mut s.zoom_percent).range(50..=300).suffix(" %"));
        theme::grid_row(ui, t!("options.look_log_size"), egui::DragValue::new(&mut s.log_text_size).range(0..=40));
        theme::grid_row(ui, t!("options.look_default_layout"), theme::single_line(&mut s.default_layout).desired_width(160.0));
    });
    fonts_ui(ui, s);
    ui.checkbox(&mut s.tabs_bottom, t!("options.look_tabs_bottom"));
    ui.checkbox(&mut s.tabs_multiline, t!("options.look_tabs_multiline"));
    ui.horizontal(|ui| {
        let l = ui.label(t!("options.look_tab_width"));
        ui.add(egui::DragValue::new(&mut s.tab_min_width).range(0..=400)).labelled_by(l.id);
        ui.label(RichText::new(t!("options.look_tab_width_auto")).weak());
    });
    ui.checkbox(&mut s.large_icons, t!("options.look_large_icons"));
    ui.checkbox(&mut s.favorites_split, t!("options.look_favorites_split"));
    ui.horizontal(|ui| {
        use urftp_core::settings::SizeDisplay;
        ui.label(t!("options.look_sizes"));
        ui.radio_value(&mut s.size_display, SizeDisplay::Bytes, t!("options.look_sizes_bytes"));
        ui.radio_value(&mut s.size_display, SizeDisplay::KBytes, t!("options.look_sizes_kb"));
        ui.radio_value(&mut s.size_display, SizeDisplay::Dynamic, t!("options.look_sizes_dynamic"));
    });
    ui.checkbox(&mut s.local_show_hidden, t!("options.look_show_hidden"));
    ui.checkbox(&mut s.local_explorer_menu, t!("options.look_explorer_menu"));
    ui.horizontal(|ui| {
        let l = ui.label(t!("options.look_refresh_download"));
        ui.add(egui::DragValue::new(&mut s.refresh_download_secs).range(0..=3600).suffix(" s")).labelled_by(l.id);
        ui.label(RichText::new(t!("options.look_refresh_download_off")).weak());
    });
    ui.horizontal(|ui| {
        ui.label(t!("options.look_autosize"));
        ui.checkbox(&mut s.autosize_local, t!("options.look_autosize_local"));
        ui.checkbox(&mut s.autosize_remote, t!("options.look_autosize_remote"));
        ui.checkbox(&mut s.autosize_queue, t!("options.look_autosize_queue"));
    });
    ui.checkbox(&mut s.dirs_first, t!("options.look_dirs_first"));
    ui.checkbox(&mut s.links_apart, t!("options.look_links_apart"));
    ui.checkbox(&mut s.sort_desc_first, t!("options.look_desc_first"));
    ui.add_space(6.0);
    ui.label(RichText::new(t!("options.look_highlights")).strong());
    let mut remove = None;
    egui::Grid::new("opt-highlights").num_columns(8).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label(RichText::new(t!("options.browse_masks")).weak());
        ui.label(RichText::new(t!("options.look_dark")).weak());
        ui.label("");
        ui.label("");
        ui.label(RichText::new(t!("options.look_light")).weak());
        ui.end_row();
        for (i, h) in s.highlights.iter_mut().enumerate() {
            let r = ui.add(theme::single_line(&mut h.masks).desired_width(140.0));
            theme::name(&r, format!("{} {}", t!("options.browse_masks"), i + 1));
            color_field(ui, &mut h.dark, true, &format!("{} {}", t!("options.look_dark"), i + 1));
            color_field(ui, &mut h.light, false, &format!("{} {}", t!("options.look_light"), i + 1));
            if theme::icon_button(ui, "✕", t!("common.remove")).clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        s.highlights.remove(i);
    }
    if ui.button(t!("options.browse_add")).clicked() {
        s.highlights.push(urftp_core::settings::Highlight::default());
    }
    ui.add_space(6.0);
    ui.checkbox(&mut s.custom_log_colors, t!("options.look_log_colors"));
    if s.custom_log_colors {
        s.log_colors_dark.resize(4, String::new());
        s.log_colors_light.resize(4, String::new());
        egui::Grid::new("opt-log-colors").num_columns(7).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new(t!("options.look_dark")).weak());
            ui.label("");
            ui.label("");
            ui.label(RichText::new(t!("options.look_light")).weak());
            ui.end_row();
            for (i, k) in ["options.look_command", "options.look_reply", "options.look_status", "options.look_error"].iter().enumerate() {
                let label = urftp_i18n::text(k).to_string();
                ui.label(&label);
                color_field(ui, &mut s.log_colors_dark[i], true, &format!("{label} ({})", t!("options.look_dark")));
                color_field(ui, &mut s.log_colors_light[i], false, &format!("{label} ({})", t!("options.look_light")));
                ui.end_row();
            }
        });
    }
    ui.add_space(6.0);
    ui.checkbox(&mut s.custom_progress, t!("options.look_progress"));
    if s.custom_progress {
        egui::Grid::new("opt-progress").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            let name = t!("options.look_progress");
            ui.label(t!("options.look_dark"));
            color_field(ui, &mut s.progress_dark, true, &format!("{name} ({})", t!("options.look_dark")));
            ui.end_row();
            ui.label(t!("options.look_light"));
            color_field(ui, &mut s.progress_light, false, &format!("{name} ({})", t!("options.look_light")));
            ui.end_row();
        });
    }
}

/// Shortcut manager: the keys of each command, the conflicts.
fn shortcuts_ui(ui: &mut egui::Ui, s: &mut urftp_core::settings::InterfaceSettings) {
    use crate::app::input::{self, Command};
    ui.label(RichText::new(t!("options.keys_note")).weak());
    ui.add_space(4.0);
    egui::Grid::new("opt-keys").num_columns(3).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
        for c in Command::ALL {
            let label = urftp_i18n::text(&c.label_key()).to_string();
            let l = ui.label(&label);
            let mut text = s.shortcuts.get(c.id()).cloned().unwrap_or_else(|| c.default_keys().to_string());
            let valid = input::parse_list(&text).is_some_and(|v| !v.is_empty());
            let mut field = theme::single_line(&mut text).desired_width(200.0);
            if !valid {
                field = field.text_color(theme::colors(ui).error);
            }
            let r = ui.add(field).labelled_by(l.id);
            if r.changed() {
                if text.trim() == c.default_keys() {
                    s.shortcuts.remove(c.id());
                } else {
                    s.shortcuts.insert(c.id().to_string(), text.clone());
                }
            }
            if ui
                .add_enabled(s.shortcuts.contains_key(c.id()), egui::Button::new(t!("common.defaults")))
                .on_hover_text(c.default_keys())
                .clicked()
            {
                s.shortcuts.remove(c.id());
            }
            ui.end_row();
        }
    });
    let bad: Vec<String> = Command::ALL
        .iter()
        .filter(|c| s.shortcuts.get(c.id()).is_some_and(|t| input::parse_list(t).is_none_or(|v| v.is_empty())))
        .map(|c| urftp_i18n::text(&c.label_key()).to_string())
        .collect();
    let p = theme::colors(ui);
    if !bad.is_empty() {
        ui.colored_label(p.error, t!("options.keys_invalid", names = bad.join(", ")));
    }
    for (key, cmds) in input::conflicts(&s.shortcuts) {
        let names: Vec<String> = cmds.iter().map(|c| urftp_i18n::text(&c.label_key()).to_string()).collect();
        ui.colored_label(p.warning, t!("options.keys_conflict", key = key, names = names.join(", ")));
    }
    if ui.button(t!("options.keys_reset")).clicked() {
        s.shortcuts.clear();
    }
}

/// "Commands" page: the command designer (FTPRush "FTP Command Designer").
fn commands_ui(ui: &mut egui::Ui, s: &mut Settings) {
    use crate::app::input;
    use urftp_core::settings::SiteCommand;
    ui.label(RichText::new(t!("options.ui_commands_intro")).weak());
    ui.add_space(4.0);
    let list = &mut s.site_commands;
    let mut remove = None;
    let mut up = None;
    let error = theme::colors(ui).error;
    for (i, c) in list.iter_mut().enumerate() {
        ui.push_id(("command", i), |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let l = ui.label(t!("common.name"));
                    let r = ui.add(theme::single_line(&mut c.name).desired_width(110.0)).labelled_by(l.id);
                    theme::name(&r, format!("{} {}", t!("common.name"), i + 1));
                    let l = ui.label(t!("options.ui_command_folder"));
                    let r = ui.add(theme::single_line(&mut c.folder).desired_width(90.0)).labelled_by(l.id);
                    theme::name(&r, format!("{} {}", t!("options.ui_command_folder"), i + 1));
                    let l = ui.label(t!("options.ui_command_key"));
                    let valid = c.hotkey.trim().is_empty() || input::parse_optional(&c.hotkey).is_some();
                    let mut field = theme::single_line(&mut c.hotkey).desired_width(80.0);
                    if !valid {
                        field = field.text_color(error);
                    }
                    let r = ui.add(field).labelled_by(l.id);
                    theme::name(&r, format!("{} {}", t!("options.ui_command_key"), i + 1));
                    if theme::icon_button(ui, "⏶", t!("common.move_up")).clicked() && i > 0 {
                        up = Some(i);
                    }
                    if theme::icon_button(ui, "✕", t!("common.remove")).clicked() {
                        remove = Some(i);
                    }
                });
                let r = ui.add(
                    egui::TextEdit::multiline(&mut c.lines).font(egui::TextStyle::Monospace).desired_rows(2).desired_width(f32::INFINITY),
                );
                theme::name(&r, format!("{} {}", t!("options.ui_command_lines"), i + 1));
            });
        });
    }
    if let Some(i) = up {
        list.swap(i, i - 1);
    }
    if let Some(i) = remove {
        list.remove(i);
    }
    ui.horizontal(|ui| {
        if ui.button(t!("options.browse_add")).clicked() {
            list.push(SiteCommand::new(&t!("options.ui_command_new"), "SITE "));
        }
        if ui.button(t!("common.defaults")).clicked() {
            *list = urftp_core::settings::default_site_commands();
        }
    });
    // Keys not understood, or taken by another command or by a key of the list.
    let bad: Vec<&str> = list
        .iter()
        .filter(|c| !c.hotkey.trim().is_empty() && input::parse_optional(&c.hotkey).is_none())
        .map(|c| c.name.as_str())
        .collect();
    if !bad.is_empty() {
        ui.colored_label(error, t!("options.keys_invalid", names = bad.join(", ")));
    }
    for (key, names) in command_key_conflicts(list, &s.interface.shortcuts) {
        ui.colored_label(error, t!("options.ui_command_conflict", key = key, names = names.join(", ")));
    }
    ui.add_space(6.0);
    ui.label(RichText::new(t!("options.ui_commands_params")).strong());
    for k in ["f", "p", "c", "d", "n", "all", "clearbuf", "window", "percent"] {
        ui.label(RichText::new(urftp_i18n::text(&format!("options.ui_command_param_{k}"))).monospace());
    }
}

/// Keys used twice: by two commands, or by a command and a key of the program.
fn command_key_conflicts(
    list: &[urftp_core::settings::SiteCommand],
    shortcuts: &std::collections::BTreeMap<String, String>,
) -> Vec<(String, Vec<String>)> {
    use crate::app::input;
    let mut by_key: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for (cmd, b) in input::table(shortcuts) {
        let names = by_key.entry(input::format(&b)).or_default();
        let name = urftp_i18n::text(&cmd.label_key()).to_string();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let builtin: std::collections::BTreeSet<String> = by_key.keys().cloned().collect();
    let mut used: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for c in list {
        if let Some(b) = input::parse_optional(&c.hotkey) {
            used.entry(input::format(&b)).or_default().push(c.name.clone());
        }
    }
    used.into_iter()
        .filter(|(k, v)| v.len() > 1 || builtin.contains(k))
        .map(|(k, mut v)| {
            if let Some(b) = by_key.get(&k) {
                v.extend(b.iter().cloned());
            }
            (k, v)
        })
        .collect()
}

/// "Browsing" page: directory cache and the programs of the remote files.
fn browse_ui(ui: &mut egui::Ui, b: &mut urftp_core::settings::BrowseSettings) {
    ui.label(RichText::new(t!("options.browse_cache_section")).strong());
    ui.checkbox(&mut b.dir_cache, t!("options.browse_dir_cache"));
    ui.add_enabled(b.dir_cache, egui::Checkbox::new(&mut b.update_cache_on_transfer, t!("options.browse_update_cache")));
    ui.label(RichText::new(t!("options.browse_cache_note")).weak());
    ui.add_space(8.0);
    for (list, title, salt) in
        [(&mut b.view, t!("options.browse_view_section"), "view"), (&mut b.edit, t!("options.browse_edit_section"), "edit")]
    {
        ui.label(RichText::new(title).strong());
        let mut remove = None;
        egui::Grid::new(("opt-assoc", salt)).num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new(t!("options.browse_masks")).weak());
            ui.label(RichText::new(t!("options.browse_program")).weak());
            ui.end_row();
            for (i, a) in list.iter_mut().enumerate() {
                let r = ui.add(theme::single_line(&mut a.masks).desired_width(140.0));
                theme::name(&r, t!("options.browse_masks"));
                let r = ui.add(theme::single_line(&mut a.program).desired_width(260.0));
                theme::name(&r, t!("options.browse_program"));
                if ui.button(t!("common.browse")).clicked()
                    && let Some(p) =
                        rfd::FileDialog::new().add_filter(t!("options.browse_programs"), &["exe", "com", "bat", "cmd"]).pick_file()
                {
                    a.program = p.to_string_lossy().into_owned();
                }
                if theme::icon_button(ui, "✕", t!("common.remove")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            list.remove(i);
        }
        if ui.button(t!("options.browse_add")).clicked() {
            list.push(urftp_core::settings::FileAssociation { masks: "*.*".into(), program: String::new() });
        }
        ui.add_space(6.0);
    }
    ui.label(RichText::new(t!("options.browse_assoc_note")).weak());
}

fn advanced_ui(ui: &mut egui::Ui, t: &mut urftp_core::settings::TransferSettings) {
    use urftp_core::{NameCase, TransferMode};
    ui.label(RichText::new(t!("options.adv_mode_section")).strong());
    egui::Grid::new("opt-adv-mode").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        let l = ui.label(t!("options.adv_mode"));
        let combo = egui::ComboBox::from_id_salt("opt-adv-mode-combo").selected_text(mode_label(t.mode)).width(180.0).show_ui(ui, |ui| {
            for m in [TransferMode::Auto, TransferMode::Binary, TransferMode::Ascii] {
                ui.selectable_value(&mut t.mode, m, mode_label(m));
            }
        });
        combo.response.labelled_by(l.id);
        ui.end_row();
        theme::grid_row(ui, t!("options.adv_ascii_masks"), theme::single_line(&mut t.ascii_masks).desired_width(360.0));
        for (label, value, salt) in [
            (t!("options.adv_upload_case"), &mut t.upload_case, "up"),
            (t!("options.adv_download_case"), &mut t.download_case, "down"),
            (t!("options.adv_fxp_case"), &mut t.fxp_case, "fxp"),
        ] {
            let l = ui.label(label);
            let combo =
                egui::ComboBox::from_id_salt(("opt-adv-case", salt)).selected_text(case_label(*value)).width(180.0).show_ui(ui, |ui| {
                    for c in [NameCase::Keep, NameCase::Lower, NameCase::Upper] {
                        ui.selectable_value(value, c, case_label(c));
                    }
                });
            combo.response.labelled_by(l.id);
            ui.end_row();
        }
    });
    ui.checkbox(&mut t.no_resume_ascii, t!("options.adv_no_resume_ascii"));
    ui.add_space(8.0);

    ui.label(RichText::new(t!("options.adv_data_section")).strong());
    ui.checkbox(&mut t.force_active, t!("options.adv_force_active"));
    ui.checkbox(&mut t.xdupe, t!("options.adv_xdupe"));
    ui.add_enabled(t.xdupe, egui::Checkbox::new(&mut t.xdupe_refresh, t!("options.adv_xdupe_refresh")));
    ui.checkbox(&mut t.mode_z, t!("options.adv_mode_z"));
    ui.add_enabled_ui(t.mode_z, |ui| {
        ui.horizontal(|ui| {
            ui.label(t!("options.adv_mode_z_for"));
            ui.checkbox(&mut t.mode_z_listings, t!("options.adv_mode_z_listings"));
            ui.checkbox(&mut t.mode_z_downloads, t!("options.adv_mode_z_downloads"));
            ui.checkbox(&mut t.mode_z_uploads, t!("options.adv_mode_z_uploads"));
        });
    });
    egui::Grid::new("opt-adv-data").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        let l = ui.label(t!("options.adv_mode_z_except"));
        ui.add_enabled(t.mode_z, theme::single_line(&mut t.mode_z_exceptions).desired_width(360.0)).labelled_by(l.id);
        ui.end_row();
        theme::grid_row(ui, t!("options.adv_send_buffer"), egui::DragValue::new(&mut t.send_buffer_kib).range(0..=65_536).speed(8));
        theme::grid_row(ui, t!("options.adv_receive_buffer"), egui::DragValue::new(&mut t.receive_buffer_kib).range(0..=65_536).speed(8));
    });
    ui.add_space(8.0);

    ui.label(RichText::new(t!("options.adv_disk_section")).strong());
    egui::Grid::new("opt-adv-disk").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        theme::grid_row(ui, t!("options.adv_rollback"), egui::DragValue::new(&mut t.resume_rollback_kib).range(0..=1_048_576).speed(8));
        theme::grid_row(ui, t!("options.adv_keep_free"), egui::DragValue::new(&mut t.keep_free_mib).range(0..=10_000_000).speed(16));
    });
    ui.checkbox(&mut t.unlimited_local, t!("options.adv_unlimited_local"));
    ui.add_space(8.0);

    ui.label(RichText::new(t!("options.adv_crc_section")).strong());
    ui.checkbox(&mut t.check_sfv, t!("options.adv_check_sfv"));
    ui.add_enabled(t.check_sfv, egui::Checkbox::new(&mut t.sfv_status_dir, t!("options.adv_sfv_status")));
    ui.checkbox(&mut t.use_xcrc, t!("options.adv_xcrc"));
    ui.checkbox(&mut t.readd_bad_crc, t!("options.adv_readd_bad"));
    ui.checkbox(&mut t.one_sfv_per_folder, t!("options.adv_one_sfv"));
    ui.add_space(8.0);

    ui.label(RichText::new(t!("options.adv_folders_section")).strong());
    ui.checkbox(&mut t.remove_empty_dirs, t!("options.adv_remove_empty"));
    ui.checkbox(&mut t.keep_server_date, t!("options.adv_keep_date"));
    ui.checkbox(&mut t.enter_new_folder, t!("options.adv_enter_new"));
    ui.checkbox(&mut t.refuse_same_name_dir, t!("options.adv_same_name"));
    ui.checkbox(&mut t.fxp_remove_zero_target, t!("options.adv_fxp_zero"));
}

/// What to do once the queues are finished.
fn queue_end_ui(ui: &mut egui::Ui, t: &mut urftp_core::settings::TransferSettings) {
    ui.label(RichText::new(t!("options.queue_end_section")).strong());
    {
        egui::Grid::new("opt-queue-end").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            let l = ui.label(t!("options.queue_end"));
            let combo = egui::ComboBox::from_id_salt("opt-queue-end-combo")
                .selected_text(crate::app::queue_end_label(t.on_queue_end))
                .width(220.0)
                .show_ui(ui, |ui| {
                    for a in QueueEnd::ALL {
                        ui.selectable_value(&mut t.on_queue_end, a, crate::app::queue_end_label(a));
                    }
                });
            combo.response.labelled_by(l.id);
            ui.end_row();
            let enabled = t.on_queue_end.needs_countdown();
            let l = ui.label(t!("options.queue_end_countdown"));
            ui.add_enabled(enabled, egui::DragValue::new(&mut t.queue_end_countdown_secs).range(5..=600)).labelled_by(l.id);
            ui.end_row();
        });
        ui.label(RichText::new(t!("options.queue_end_note")).weak());
    }
}

fn proxy_ui(ui: &mut egui::Ui, g: &mut GlobalProxy, named: &mut Vec<urftp_core::settings::NamedProxy>) {
    proxy_global_ui(ui, g);
    ui.add_space(12.0);
    named_proxies_ui(ui, named);
}

fn proxy_global_ui(ui: &mut egui::Ui, g: &mut GlobalProxy) {
    ui.checkbox(&mut g.enabled, t!("options.proxy_enable"));
    ui.add_enabled_ui(g.enabled, |ui| {
        let p = &mut g.proxy;
        egui::Grid::new("opt-proxy").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
            ui.label(t!("common.type"));
            ui.horizontal(|ui| {
                for (k, l) in [
                    (ProxyType::Socks4, "SOCKS4"),
                    (ProxyType::Socks4a, "SOCKS4A"),
                    (ProxyType::Socks5, "SOCKS5"),
                    (ProxyType::HttpTunnel, "HTTP Tunnel"),
                ] {
                    ui.radio_value(&mut p.kind, k, l);
                }
            });
            ui.end_row();
            let l = ui.label(t!("options.proxy_host_port"));
            ui.horizontal(|ui| {
                let h = ui.add(theme::single_line(&mut p.host).desired_width(200.0));
                theme::name(&h, t!("common.host"));
                ui.add(egui::DragValue::new(&mut p.port).range(1..=65535)).labelled_by(l.id);
            });
            ui.end_row();
            theme::grid_row(ui, t!("common.user"), theme::single_line(&mut p.user).desired_width(160.0));
            theme::grid_row(ui, t!("common.password"), theme::single_line(&mut p.password).password(true).desired_width(160.0));
            theme::grid_row(
                ui,
                t!("options.proxy_exceptions"),
                theme::single_line(&mut g.exceptions).hint_text("220.67.*;*.lan").desired_width(260.0),
            );
        });
        ui.checkbox(&mut p.data_channel, t!("options.proxy_data"));
        ui.checkbox(&mut g.bypass_local, t!("options.proxy_bypass_local"));
    });
}

#[cfg(test)]
mod tests {
    use super::command_key_conflicts;
    use urftp_core::settings::SiteCommand;

    #[test]
    fn command_keys_taken_twice_are_reported() {
        let mut a = SiteCommand::new("Who", "SITE WHO");
        a.hotkey = "F5".into();
        let mut b = SiteCommand::new("Stat", "SITE STAT");
        b.hotkey = "ctrl+alt+s".into();
        let mut c = SiteCommand::new("New", "SITE NEW");
        c.hotkey = "Ctrl+Alt+S".into();
        let free = SiteCommand::new("Help", "SITE HELP");
        let found = command_key_conflicts(&[a, b, c, free], &Default::default());
        let keys: Vec<&str> = found.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["Ctrl+Alt+S", "F5"], "{found:?}");
        assert_eq!(found[0].1, ["Stat", "New"]);
        assert!(found[1].1.len() >= 2 && found[1].1[0] == "Who", "F5 is also Refresh: {found:?}");
    }
}
