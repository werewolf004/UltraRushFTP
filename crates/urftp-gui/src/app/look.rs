//! Look of the main window: highlighted names, colours of the logs, sort options, size of
//! the interface and of the logs, saved layouts of the panels.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32};

use serde::{Deserialize, Serialize};
use urftp_core::settings::InterfaceSettings;

use super::*;

static DIRS_FIRST: AtomicBool = AtomicBool::new(true);
static DESC_FIRST: AtomicBool = AtomicBool::new(false);
static SHOW_HIDDEN: AtomicBool = AtomicBool::new(true);
/// 0: bytes, 1: KiB, 2: best unit.
static SIZE_MODE: AtomicU8 = AtomicU8::new(2);
/// Bit 0: local panels, bit 1: remote panels, bit 2: queue.
static AUTOSIZE: AtomicU8 = AtomicU8::new(0);
static LINKS_APART: AtomicBool = AtomicBool::new(false);
static LARGE_ICONS: AtomicBool = AtomicBool::new(false);
/// Double-click on a log line, on the title of a file panel, on a queue item (see
/// [`configure`]).
static LOG_CLICK: AtomicU8 = AtomicU8::new(0);
static SIDE_CLICK: AtomicU8 = AtomicU8::new(0);
static QUEUE_CLICK: AtomicU8 = AtomicU8::new(0);
/// Colour of the progress bars in the dark and light themes (RGBA, 0: the theme's).
static PROGRESS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];

/// Applies the sort and display options; answers whether the local panels must be read
/// again (hidden files shown or not).
pub(super) fn configure(s: &InterfaceSettings) -> bool {
    use urftp_core::settings::SizeDisplay;
    DIRS_FIRST.store(s.dirs_first, Ordering::Relaxed);
    DESC_FIRST.store(s.sort_desc_first, Ordering::Relaxed);
    SIZE_MODE.store(
        match s.size_display {
            SizeDisplay::Bytes => 0,
            SizeDisplay::KBytes => 1,
            SizeDisplay::Dynamic => 2,
        },
        Ordering::Relaxed,
    );
    AUTOSIZE.store(u8::from(s.autosize_local) | u8::from(s.autosize_remote) << 1 | u8::from(s.autosize_queue) << 2, Ordering::Relaxed);
    LINKS_APART.store(s.links_apart, Ordering::Relaxed);
    LARGE_ICONS.store(s.large_icons, Ordering::Relaxed);
    {
        use urftp_core::settings::{LogDoubleClick as L, QueueDoubleClick as Q, SideDoubleClick as D};
        LOG_CLICK.store(u8::from(s.double_click_log == L::Nothing), Ordering::Relaxed);
        let side = match s.double_click_side {
            D::Nothing => 0,
            D::Refresh => 1,
            D::Parent => 2,
            D::Switch => 3,
        };
        SIDE_CLICK.store(side, Ordering::Relaxed);
        let queue = match s.double_click_queue {
            Q::Nothing => 0,
            Q::Edit => 1,
            Q::Retry => 2,
            Q::Remove => 3,
        };
        QUEUE_CLICK.store(queue, Ordering::Relaxed);
    }
    for (i, text) in [&s.progress_dark, &s.progress_light].into_iter().enumerate() {
        let c = theme::parse_color(text).filter(|_| s.custom_progress).map_or(0, |c| u32::from_be_bytes(c.to_array()));
        PROGRESS[i].store(c, Ordering::Relaxed);
    }
    SHOW_HIDDEN.swap(s.local_show_hidden, Ordering::Relaxed) != s.local_show_hidden
}

/// Links are listed apart.
pub(super) fn links_apart() -> bool {
    LINKS_APART.load(Ordering::Relaxed)
}

/// A double-click on a log line copies it.
pub(super) fn log_copy() -> bool {
    LOG_CLICK.load(Ordering::Relaxed) == 0
}

/// What a double-click on the title of a file panel does.
pub(super) fn side_double_click() -> urftp_core::settings::SideDoubleClick {
    use urftp_core::settings::SideDoubleClick as D;
    match SIDE_CLICK.load(Ordering::Relaxed) {
        1 => D::Refresh,
        2 => D::Parent,
        3 => D::Switch,
        _ => D::Nothing,
    }
}

/// What a double-click on a queue item does.
pub(super) fn queue_double_click() -> urftp_core::settings::QueueDoubleClick {
    use urftp_core::settings::QueueDoubleClick as Q;
    match QUEUE_CLICK.load(Ordering::Relaxed) {
        1 => Q::Edit,
        2 => Q::Retry,
        3 => Q::Remove,
        _ => Q::Nothing,
    }
}

/// Colour of the progress bars chosen for the theme (`None`: the theme's).
pub(super) fn progress_fill(dark: bool) -> Option<egui::Color32> {
    let c = PROGRESS[usize::from(!dark)].load(Ordering::Relaxed);
    (c != 0).then(|| {
        let [r, g, b, a] = c.to_be_bytes();
        egui::Color32::from_rgba_premultiplied(r, g, b, a)
    })
}

/// Larger buttons in a toolbar, when chosen in the options.
pub(super) fn toolbar_style(ui: &mut egui::Ui) {
    if !LARGE_ICONS.load(Ordering::Relaxed) {
        return;
    }
    let s = ui.style_mut();
    if let Some(f) = s.text_styles.get_mut(&egui::TextStyle::Button) {
        f.size *= 1.4;
    }
    s.spacing.button_padding *= 1.4;
    s.spacing.interact_size.y *= 1.4;
}

/// The local panels list hidden files.
pub(super) fn show_hidden() -> bool {
    SHOW_HIDDEN.load(Ordering::Relaxed)
}

/// Columns sized to their content: 0 local panels, 1 remote panels, 2 queue.
pub(super) fn autosize(which: u8) -> bool {
    AUTOSIZE.load(Ordering::Relaxed) & (1 << which) != 0
}

/// Size of a file in the lists, in the unit chosen in the options.
pub(super) fn size_text(n: u64) -> String {
    match SIZE_MODE.load(Ordering::Relaxed) {
        0 => format!("{} {}", group_digits(n), urftp_i18n::text("units.b")),
        1 => format!("{} {}", group_digits(n.div_ceil(1024)), urftp_i18n::text("units.kb")),
        _ => human_size(n),
    }
}

/// `1234567` → `1,234,567` (English) or `1 234 567` (French, narrow no-break spaces).
fn group_digits(n: u64) -> String {
    let sep = if urftp_i18n::language() == Lang::Fr { '\u{202F}' } else { ',' };
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Colour of the items the skip list would skip, for the theme (`None`: not shown).
pub(super) fn skipped_color(s: &urftp_core::settings::SkipSettings, dark: bool) -> Option<egui::Color32> {
    (s.enabled && s.show_skipped).then(|| theme::parse_color(if dark { &s.skipped_dark } else { &s.skipped_light })).flatten()
}

/// Folders are listed before the files.
pub(super) fn dirs_first() -> bool {
    DIRS_FIRST.load(Ordering::Relaxed)
}

/// A first click on the size or date column sorts the largest / newest first.
pub(super) fn desc_first() -> bool {
    DESC_FIRST.load(Ordering::Relaxed)
}

/// Highlight colours of the theme: (masks, colour); unreadable entries are left out.
pub(super) fn highlights(s: &InterfaceSettings, dark: bool) -> Vec<(String, egui::Color32)> {
    s.highlights.iter().filter_map(|h| theme::parse_color(if dark { &h.dark } else { &h.light }).map(|c| (h.masks.clone(), c))).collect()
}

/// Colours of the logs (command, reply, status, error) chosen for the theme.
pub(super) fn log_colors(s: &InterfaceSettings, dark: bool) -> Option<[egui::Color32; 4]> {
    if !s.custom_log_colors {
        return None;
    }
    let set = if dark { &s.log_colors_dark } else { &s.log_colors_light };
    let p = theme::palette(dark);
    let defaults = [p.command, p.success, p.status, p.error];
    Some(std::array::from_fn(|i| set.get(i).and_then(|t| theme::parse_color(t)).unwrap_or(defaults[i])))
}

/// A layout of the panels kept by name.
#[derive(Serialize, Deserialize)]
pub(super) struct SavedLayout {
    pub(super) name: String,
    pub(super) dock: DockState<Pane>,
}

fn layouts_path() -> PathBuf {
    urftp_core::sitestore::data_dir().join("layouts.json")
}

/// Saved layouts (an unreadable file gives none).
pub(super) fn load_layouts() -> Vec<SavedLayout> {
    std::fs::read_to_string(layouts_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_layouts(v: &[SavedLayout]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    let path = layouts_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

impl UltraRushApp {
    /// Keeps the layout of the active tab under `name` (replacing one of that name).
    pub(super) fn save_layout(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let dock = self.tabs[self.active].dock.clone();
        self.layouts.retain(|l| l.name != name);
        self.layouts.push(SavedLayout { name: name.to_string(), dock });
        self.write_layouts();
    }

    fn write_layouts(&mut self) {
        if let Err(e) = save_layouts(&self.layouts) {
            let a = self.active;
            self.tabs[a].push_log(0, LogKind::Error, t!("look.layout_failed", error = e));
        }
    }

    /// "Layouts" sub-menu of the View menu.
    pub(super) fn layouts_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("look.layouts"), |ui| {
            let mut apply = None;
            let mut delete = None;
            for (i, l) in self.layouts.iter().enumerate() {
                ui.horizontal(|ui| {
                    if ui.button(&l.name).clicked() {
                        apply = Some(i);
                        ui.close();
                    }
                    if theme::icon_button(ui, "✕", t!("look.layout_delete", name = l.name)).clicked() {
                        delete = Some(i);
                    }
                });
            }
            if self.layouts.is_empty() {
                ui.label(RichText::new(t!("look.no_layouts")).italics());
            }
            ui.separator();
            if ui.button(t!("look.layout_save")).clicked() {
                self.layout_name = Some(String::new());
                ui.close();
            }
            if ui.button(t!("look.layout_reset")).clicked() {
                self.tabs[self.active].dock = default_dock();
                ui.close();
            }
            if ui.button(t!("look.layout_tabbed")).on_hover_text(t!("look.layout_tabbed_hint")).clicked() {
                self.tabs[self.active].dock = tabbed_dock();
                ui.close();
            }
            if let Some(i) = apply {
                self.tabs[self.active].dock = self.layouts[i].dock.clone();
                self.fire_switch_layout(ui.ctx());
            }
            if let Some(i) = delete {
                self.layouts.remove(i);
                self.write_layouts();
            }
        });
    }

    /// Name of a layout to save.
    pub(super) fn layout_dialog(&mut self, ctx: &egui::Context) {
        let Some(name) = &mut self.layout_name else { return };
        let mut done = None;
        egui::Modal::new(egui::Id::unique("layout-name")).show(ctx, |ui| {
            ui.set_min_width(320.0);
            let h = ui.heading(t!("look.layout_save_title"));
            let r = ui.add(theme::single_line(name).desired_width(f32::INFINITY)).labelled_by(h.id);
            focus_once(ui, &r);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, t!("common.validate")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = Some(true);
                }
                if ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    done = Some(false);
                }
            });
        });
        if let Some(ok) = done
            && let Some(name) = self.layout_name.take()
            && ok
        {
            self.save_layout(&name);
        }
    }

    /// Layout given to a new tab (the one named in the options, when it exists).
    pub(super) fn new_tab_layout(&self) -> Option<DockState<Pane>> {
        let name = self.options.settings.interface.default_layout.trim();
        self.layouts.iter().find(|l| !name.is_empty() && l.name == name).map(|l| l.dock.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_colours_are_readable_in_their_theme() {
        let s = InterfaceSettings {
            custom_log_colors: true,
            highlights: vec![urftp_core::settings::Highlight::default()],
            ..InterfaceSettings::default()
        };
        for dark in [true, false] {
            let colors = log_colors(&s, dark).unwrap();
            for c in colors.into_iter().chain(highlights(&s, dark).into_iter().map(|(_, c)| c)) {
                let r = theme::list_contrast(c, dark);
                assert!(r >= 4.5, "{c:?} in the {} theme: {r:.2}:1", if dark { "dark" } else { "light" });
            }
        }
        assert_eq!(theme::parse_color("#1d4fa8"), Some(egui::Color32::from_rgb(0x1D, 0x4F, 0xA8)));
        assert_eq!(theme::parse_color("12345"), None);
    }
}
