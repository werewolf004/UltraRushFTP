//! "Options" window: global settings, saved automatically to `settings.json`.

use std::time::Duration;
use urftp_i18n::{Lang, t};

use eframe::egui::{self, RichText};
use urftp_core::exists::{ExistsAction, ExistsRules};
use urftp_core::settings::{GlobalProxy, LanguageSetting, MaskSetting, Settings, SkipRuleSetting};
use urftp_core::site::ProxyType;
use urftp_core::skiplist::{RuleAction, RuleTarget};

/// Conversions shared with the `urftp` tool.
pub use urftp_proto::profile::{engine_options, global_proxy};

use crate::theme::{ACCENT, LOG_ERROR, LOG_REPLY};

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

    fn label(self) -> String {
        match self {
            Page::General => t!("options.page_general"),
            Page::Transfers => t!("options.page_transfers"),
            Page::Speed => t!("options.page_speed"),
            Page::Exists => t!("options.page_exists"),
            Page::Skiplist => t!("options.page_skiplist"),
            Page::Priorities => t!("options.page_priorities"),
            Page::Proxy => t!("options.page_proxy"),
            Page::Ident => t!("options.page_ident"),
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
        // Development: `URUSHFTP_OPEN_OPTIONS=<page>` (0 to 7) opens the window on that page.
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
                .collapsible(false)
                .show(ctx, |ui| {
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
                ui.checkbox(&mut s.dark_theme, t!("options.dark_theme"));
                language_combo(ui, &mut s.language);
                ui.checkbox(&mut s.new_tab_when_busy, t!("options.new_tab_when_busy"));
                ui.add_space(8.0);
                ui.label(RichText::new(t!("options.data_dir", path = urftp_core::sitestore::data_dir().display())).weak());
            }
            Page::Transfers => {
                egui::Grid::new("opt-xfer").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    let t = &mut s.transfer;
                    ui.label(t!("options.retries"));
                    ui.add(egui::DragValue::new(&mut t.retries).range(0..=50));
                    ui.end_row();
                    ui.label(t!("options.retry_delay"));
                    ui.add(egui::DragValue::new(&mut t.retry_delay_secs).range(0..=600));
                    ui.end_row();
                    ui.label(t!("options.max_total"));
                    ui.add(egui::DragValue::new(&mut t.max_total).range(0..=100));
                    ui.end_row();
                    ui.label(t!("options.quick_transfers"));
                    ui.add(egui::DragValue::new(&mut t.quick_connect_transfers).range(0..=50));
                    ui.end_row();
                    ui.label(t!("options.connect_timeout"));
                    ui.add(egui::DragValue::new(&mut t.connect_timeout_secs).range(5..=600));
                    ui.end_row();
                    ui.label(t!("options.io_timeout"));
                    ui.add(egui::DragValue::new(&mut t.io_timeout_secs).range(10..=3600));
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(RichText::new(t!("options.timeouts_note")).weak());
            }
            Page::Speed => {
                let t = &mut s.transfer;
                egui::Grid::new("opt-speed").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label(t!("options.download_limit"));
                    ui.add(egui::DragValue::new(&mut t.download_limit_kib).range(0..=10_000_000).speed(16));
                    ui.end_row();
                    ui.label(t!("options.upload_limit"));
                    ui.add(egui::DragValue::new(&mut t.upload_limit_kib).range(0..=10_000_000).speed(16));
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.label(RichText::new(t!("options.speed_note")).weak());
            }
            Page::Exists => {
                let e = &mut s.exists;
                egui::Grid::new("opt-exists").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    for (label, action) in [
                        (t!("options.exists_smaller"), &mut e.smaller),
                        (t!("options.exists_same"), &mut e.same),
                        (t!("options.exists_larger"), &mut e.larger),
                    ] {
                        ui.label(&label);
                        exists_combo(ui, &label, action);
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.checkbox(&mut e.overwrite_if_resume_fails, t!("options.exists_restart"));
                if ui.small_button(t!("common.defaults")).clicked() {
                    *e = ExistsRules::default();
                }
            }
            Page::Skiplist => skiplist_ui(ui, s),
            Page::Priorities => priorities_ui(ui, &mut s.priorities),
            Page::Proxy => proxy_ui(ui, &mut s.proxy),
            Page::Ident => {
                let id = &mut s.ident;
                ui.checkbox(&mut id.enabled, t!("options.ident_enable"));
                egui::Grid::new("opt-ident").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label(t!("options.ident_name"));
                    ui.add(egui::TextEdit::singleline(&mut id.name).hint_text(urftp_core::DEFAULT_IDENT_NAME).desired_width(180.0));
                    ui.end_row();
                    ui.label(t!("options.ident_system"));
                    ui.add(egui::TextEdit::singleline(&mut id.system).desired_width(100.0));
                    ui.end_row();
                    ui.label(t!("options.ident_port"));
                    ui.add(egui::DragValue::new(&mut id.port).range(1..=65535));
                    ui.end_row();
                });
                ui.add_space(6.0);
                if let Some((msg, ok)) = &self.ident_status {
                    ui.colored_label(if *ok { LOG_REPLY } else { LOG_ERROR }, msg);
                }
                ui.label(RichText::new(t!("options.ident_note")).weak());
            }
        }
    }
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
        ui.label(t!("options.language"));
        egui::ComboBox::from_id_salt("opt-language").selected_text(label(*value)).show_ui(ui, |ui| {
            for v in [LanguageSetting::System, LanguageSetting::English, LanguageSetting::French] {
                ui.selectable_value(value, v, label(v));
            }
        });
    });
}

fn exists_combo(ui: &mut egui::Ui, id: &str, a: &mut ExistsAction) {
    let label = |a: ExistsAction| match a {
        ExistsAction::Resume => t!("exists.resume"),
        ExistsAction::Skip => t!("exists.skip"),
        ExistsAction::Rename => t!("exists.rename"),
        ExistsAction::Overwrite => t!("exists.overwrite"),
        ExistsAction::Ask => t!("exists.ask"),
    };
    egui::ComboBox::from_id_salt(id).selected_text(label(*a)).width(220.0).show_ui(ui, |ui| {
        for v in [ExistsAction::Resume, ExistsAction::Skip, ExistsAction::Rename, ExistsAction::Overwrite, ExistsAction::Ask] {
            ui.selectable_value(a, v, label(v));
        }
    });
}

/// Mask field with a "regex" box and a warning for an invalid expression.
fn mask_field(ui: &mut egui::Ui, m: &mut MaskSetting, width: f32) {
    let invalid = m.regex && m.compile().is_err();
    let mut edit = egui::TextEdit::singleline(&mut m.mask).desired_width(width).hint_text("*.nfo");
    if invalid {
        edit = edit.text_color(LOG_ERROR);
    }
    ui.add(edit).on_hover_text(if invalid { t!("options.mask_invalid") } else { t!("options.mask_wildcard") });
    ui.checkbox(&mut m.regex, "regex");
}

fn skiplist_ui(ui: &mut egui::Ui, s: &mut Settings) {
    let k = &mut s.skiplist;
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
            ui.horizontal(|ui| mask_field(ui, &mut r.mask, 150.0));
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
            ui.checkbox(&mut r.directions.upload, "⬆").on_hover_text(t!("options.uploads"));
            ui.checkbox(&mut r.directions.download, "⬇").on_hover_text(t!("options.downloads"));
            ui.checkbox(&mut r.directions.fxp, "↔").on_hover_text("FXP");
            ui.checkbox(&mut r.full_path, t!("options.path")).on_hover_text(t!("options.path_hint"));
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
    if ui.button(t!("options.skip_add")).clicked() {
        k.rules.push(SkipRuleSetting::default());
    }
}

fn priorities_ui(ui: &mut egui::Ui, list: &mut Vec<MaskSetting>) {
    ui.label(RichText::new(t!("options.prio_note")).weak());
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
    if ui.button(t!("options.prio_add")).clicked() {
        list.push(MaskSetting::default());
    }
}

fn proxy_ui(ui: &mut egui::Ui, g: &mut GlobalProxy) {
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
            ui.label(t!("options.proxy_host_port"));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut p.host).desired_width(200.0));
                ui.add(egui::DragValue::new(&mut p.port).range(1..=65535));
            });
            ui.end_row();
            ui.label(t!("common.user"));
            ui.add(egui::TextEdit::singleline(&mut p.user).desired_width(160.0));
            ui.end_row();
            ui.label(t!("common.password"));
            ui.add(egui::TextEdit::singleline(&mut p.password).password(true).desired_width(160.0));
            ui.end_row();
            ui.label(t!("options.proxy_exceptions"));
            ui.add(egui::TextEdit::singleline(&mut g.exceptions).hint_text("220.67.*;*.lan").desired_width(260.0));
            ui.end_row();
        });
        ui.checkbox(&mut p.data_channel, t!("options.proxy_data"));
        ui.checkbox(&mut g.bypass_local, t!("options.proxy_bypass_local"));
    });
}
