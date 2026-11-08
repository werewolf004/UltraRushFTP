//! "Options" window: global settings, saved automatically to `settings.json`.

use std::time::Duration;
use urftp_i18n::{Lang, t};

use eframe::egui::{self, RichText};
use urftp_core::exists::{ExistsAction, ExistsRules};
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
}

impl Page {
    pub(crate) const ALL: [Page; 14] = [
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
    error: Option<String>,
}

impl OptionsWindow {
    pub fn load() -> Self {
        let (settings, error) = match Settings::load(&Settings::default_path()) {
            Ok(s) => (s, None),
            Err(e) => (Settings::default(), Some(t!("options.unreadable", error = e))),
        };
        // Development: `URUSHFTP_OPEN_OPTIONS=<page>` (0 to 10) opens the window on that page.
        let open_on = std::env::var("URUSHFTP_OPEN_OPTIONS").ok().and_then(|v| v.parse::<usize>().ok());
        let page = open_on.and_then(|i| Page::ALL.get(i).copied()).unwrap_or(Page::General);
        Self { open: open_on.is_some(), settings, page, dirty: false, last_change: 0.0, ident_status: None, error }
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
                });
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
                ui.label(RichText::new(t!("options.speed_note")).weak());
            }
            Page::Exists => exists_ui(ui, &mut s.exists),
            Page::Skiplist => skiplist_ui(ui, &mut s.skiplist),
            Page::Priorities => priorities_ui(ui, &mut s.priorities),
            Page::Proxy => proxy_ui(ui, &mut s.proxy, &mut s.proxies),
            Page::KeepAlive => keep_alive_ui(ui, &mut s.keep_alive),
            Page::Connection => connection_ui(ui, s),
            Page::NewSite => {
                ui.label(RichText::new(t!("options.new_site_note")).weak());
                ui.add_space(4.0);
                ui.push_id("opt-new-site", |ui| crate::sites::new_site_ui(ui, &mut s.new_site));
            }
            Page::Advanced => advanced_ui(ui, &mut s.transfer),
            Page::Browse => browse_ui(ui, &mut s.browse),
            Page::Interface => interface_ui(ui, &mut s.interface),
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
fn connection_ui(ui: &mut egui::Ui, s: &mut Settings) {
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
pub(crate) fn exists_ui(ui: &mut egui::Ui, e: &mut ExistsRules) {
    egui::Grid::new("opt-exists").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        for (label, action) in [
            (t!("options.exists_smaller"), &mut e.smaller),
            (t!("options.exists_same"), &mut e.same),
            (t!("options.exists_larger"), &mut e.larger),
        ] {
            let l = ui.label(&label);
            exists_combo(ui, &label, action).labelled_by(l.id);
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    ui.checkbox(&mut e.overwrite_if_resume_fails, t!("options.exists_restart"));
    if ui.button(t!("common.defaults")).clicked() {
        *e = ExistsRules::default();
    }
}

/// Skip list (options page, and site record when the site has its own).
pub(crate) fn skiplist_ui(ui: &mut egui::Ui, k: &mut SkipSettings) {
    ui.checkbox(&mut k.enabled, t!("options.skip_enable"));
    ui.checkbox(&mut k.skip_zero_byte_files, t!("options.skip_zero"));
    ui.checkbox(&mut k.skip_empty_dirs, t!("options.skip_empty_dirs"));
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
        for (label, value, salt) in
            [(t!("options.adv_upload_case"), &mut t.upload_case, "up"), (t!("options.adv_download_case"), &mut t.download_case, "down")]
        {
            let l = ui.label(&label);
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
    ui.checkbox(&mut t.mode_z, t!("options.adv_mode_z"));
    egui::Grid::new("opt-adv-data").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
        let l = ui.label(t!("options.adv_mode_z_except"));
        ui.add_enabled(t.mode_z, theme::single_line(&mut t.mode_z_exceptions).desired_width(360.0)).labelled_by(l.id);
        ui.end_row();
        theme::grid_row(ui, t!("options.adv_tcp_buffer"), egui::DragValue::new(&mut t.tcp_buffer_kib).range(0..=65_536).speed(8));
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
