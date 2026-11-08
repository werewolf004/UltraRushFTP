//! Modern theme: rounded corners, airy spacing, blue accent, light or dark; semantic
//! colours checked against WCAG 2.2 AA, and helpers for accessible widgets.

use eframe::egui::{self, Color32, CornerRadius, Response, RichText, Shadow, Stroke, Theme, Ui, Visuals, WidgetText};

/// Semantic colours of one theme. Every pair used for text reaches the WCAG 2.2 AA ratio
/// (4.5:1) and every focus or state indicator 3:1; the `tests` module checks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Fill of primary buttons and progress bars.
    pub accent_fill: Color32,
    /// Text on `accent_fill`.
    pub on_accent: Color32,
    /// Accent used as text (titles, numbering).
    pub accent_text: Color32,
    /// Fill of destructive buttons (delete).
    pub danger_fill: Color32,
    /// Text on `danger_fill`.
    pub on_danger: Color32,
    /// Log: command sent.
    pub command: Color32,
    /// Log: positive reply; success messages.
    pub success: Color32,
    /// Log: status lines.
    pub status: Color32,
    /// Errors (log, messages, invalid fields).
    pub error: Color32,
    /// Warnings and pending states (connecting, confirmation needed).
    pub warning: Color32,
}

/// Dark theme colours.
pub const DARK: Palette = Palette {
    accent_fill: Color32::from_rgb(0x2F, 0x6F, 0xDB),
    on_accent: Color32::WHITE,
    accent_text: Color32::from_rgb(0x6F, 0xA8, 0xFF),
    danger_fill: Color32::from_rgb(0xB4, 0x28, 0x28),
    on_danger: Color32::WHITE,
    command: Color32::from_rgb(0x6F, 0xA8, 0xFF),
    success: Color32::from_rgb(0x4C, 0xC3, 0x7A),
    status: Color32::from_rgb(0x9A, 0xA3, 0xB2),
    error: Color32::from_rgb(0xF5, 0x8A, 0x8A),
    warning: Color32::from_rgb(0xE8, 0xB3, 0x3C),
};

/// Light theme colours (darker shades: the same hues on a light background).
pub const LIGHT: Palette = Palette {
    accent_fill: Color32::from_rgb(0x1F, 0x56, 0xC2),
    on_accent: Color32::WHITE,
    accent_text: Color32::from_rgb(0x1F, 0x56, 0xC2),
    danger_fill: Color32::from_rgb(0xB4, 0x28, 0x28),
    on_danger: Color32::WHITE,
    command: Color32::from_rgb(0x1D, 0x4F, 0xA8),
    success: Color32::from_rgb(0x1B, 0x64, 0x34),
    status: Color32::from_rgb(0x4A, 0x52, 0x60),
    error: Color32::from_rgb(0xA3, 0x20, 0x20),
    warning: Color32::from_rgb(0x7A, 0x52, 0x00),
};

/// Palette of a theme.
pub fn palette(dark: bool) -> &'static Palette {
    if dark { &DARK } else { &LIGHT }
}

/// Palette of the theme shown by `ui`.
pub fn colors(ui: &Ui) -> &'static Palette {
    palette(ui.visuals().dark_mode)
}

/// Sets the accessible name of a widget (read by screen readers), for controls whose
/// visible text is a symbol or a placeholder.
pub fn name(response: &Response, label: impl Into<String>) {
    let label = label.into();
    response.ctx.accesskit_node_builder(response.id, |node| node.set_label(label));
}

/// Button showing a symbol: the description is both its tooltip and its accessible name.
pub fn icon_button(ui: &mut Ui, icon: &str, label: impl Into<String>) -> Response {
    let label = label.into();
    let r = ui.button(icon).on_hover_text(&label);
    name(&r, label);
    r
}

/// Main action of a group (Connect, Validate...).
pub fn primary_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = colors(ui);
    ui.add(egui::Button::new(RichText::new(text).color(p.on_accent)).fill(p.accent_fill))
}

/// Destructive action (Delete).
pub fn danger_button(ui: &mut Ui, text: impl Into<String>) -> Response {
    let p = colors(ui);
    ui.add(egui::Button::new(RichText::new(text).color(p.on_danger)).fill(p.danger_fill))
}

/// Inner margin of single-line fields: with the text, a 24 px high target (WCAG 2.5.8).
const FIELD_MARGIN: egui::Margin = egui::Margin::symmetric(6, 5);

/// Single-line text field with the application's target size.
pub fn single_line(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).margin(FIELD_MARGIN)
}

/// Label followed by a control, linked for screen readers.
pub fn labelled(ui: &mut Ui, label: impl Into<WidgetText>, widget: impl egui::Widget) -> Response {
    let l = ui.label(label);
    ui.add(widget).labelled_by(l.id)
}

/// Row of a two-column grid: the label, then the control it names.
pub fn grid_row(ui: &mut Ui, label: impl Into<WidgetText>, widget: impl egui::Widget) -> Response {
    let r = labelled(ui, label, widget);
    ui.end_row();
    r
}

fn tune(mut v: Visuals, dark: bool) -> Visuals {
    let r = CornerRadius::same(7);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    let p = palette(dark);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.hyperlink_color = p.accent_text;
    v.window_shadow = Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 45 }) };
    v.popup_shadow = Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(if dark { 90 } else { 35 }) };
    v.error_fg_color = p.error;
    v.warn_fg_color = p.warning;
    if dark {
        v.panel_fill = Color32::from_rgb(0x1B, 0x1E, 0x24);
        v.window_fill = Color32::from_rgb(0x22, 0x26, 0x2E);
        v.extreme_bg_color = Color32::from_rgb(0x15, 0x17, 0x1C);
        v.faint_bg_color = Color32::from_rgb(0x20, 0x24, 0x2B);
        v.weak_text_color = Some(Color32::from_rgb(0x9A, 0xA3, 0xB2));
        // Opaque selection: the text drawn on it keeps a measurable contrast.
        v.selection.bg_fill = Color32::from_rgb(0x2A, 0x4A, 0x80);
        v.selection.stroke = Stroke::new(1.5, Color32::WHITE);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(0xB4, 0xBA, 0xC4));
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x2A, 0x2F, 0x38);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x33, 0x39, 0x44);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0x33, 0x39, 0x44);
        // Focused and pressed controls: a clear 2 px outline.
        v.widgets.active.bg_stroke = Stroke::new(2.0, Color32::WHITE);
    } else {
        // Light but soft: bluish greys instead of pure white, dark grey text rather than
        // black, to limit glare during long sessions.
        v.panel_fill = Color32::from_rgb(0xD9, 0xDD, 0xE3);
        v.window_fill = Color32::from_rgb(0xE2, 0xE5, 0xEA);
        v.extreme_bg_color = Color32::from_rgb(0xE6, 0xE9, 0xED);
        v.faint_bg_color = Color32::from_rgb(0xDE, 0xE2, 0xE8);
        v.code_bg_color = Color32::from_rgb(0xD0, 0xD5, 0xDC);
        v.weak_text_color = Some(Color32::from_rgb(0x4A, 0x52, 0x60));
        v.selection.bg_fill = Color32::from_rgb(0xA9, 0xC4, 0xF0);
        v.selection.stroke = Stroke::new(1.5, Color32::from_rgb(0x0F, 0x2F, 0x6E));
        v.widgets.noninteractive.bg_fill = v.panel_fill;
        v.widgets.noninteractive.weak_bg_fill = v.panel_fill;
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xC9, 0xCF, 0xD7);
        v.widgets.inactive.bg_fill = Color32::from_rgb(0xC9, 0xCF, 0xD7);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xBD, 0xC4, 0xCE);
        // Hovered rows get lighter (not darker): coloured text keeps its contrast.
        v.widgets.hovered.bg_fill = Color32::from_rgb(0xF0, 0xF2, 0xF5);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(0xB2, 0xBA, 0xC5);
        v.widgets.active.bg_stroke = Stroke::new(2.0, Color32::from_rgb(0x0F, 0x2F, 0x6E));
        let text = Color32::from_rgb(0x2B, 0x30, 0x38);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xC2, 0xC8, 0xD0));
    }
    v
}

/// Visuals of a theme, as installed by [`apply`].
pub fn visuals(dark: bool) -> Visuals {
    if dark { tune(Visuals::dark(), true) } else { tune(Visuals::light(), false) }
}

/// Startup theme: `URUSHFTP_THEME=light` for light, dark otherwise.
pub fn start_dark() -> bool {
    !std::env::var("URUSHFTP_THEME").is_ok_and(|t| t.eq_ignore_ascii_case("light") || t.eq_ignore_ascii_case("clair"))
}

/// Uses Windows' Segoe UI (text) and Segoe UI Symbol (arrows, symbols) when present;
/// egui's embedded fonts stay as a fallback (emojis, icons).
pub fn install_fonts(ctx: &egui::Context) {
    let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into()).join("Fonts");
    let mut fonts = egui::FontDefinitions::default();
    let mut added = Vec::new();
    for (name, file) in [("segoe-ui", "segoeui.ttf"), ("segoe-symbol", "seguisym.ttf")] {
        if let Ok(bytes) = std::fs::read(dir.join(file)) {
            fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            added.push(name.to_string());
        }
    }
    if added.is_empty() {
        return;
    }
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        for (i, name) in added.iter().enumerate() {
            prop.insert(i, name.clone());
        }
    }
    if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace)
        && added.iter().any(|n| n == "segoe-symbol")
    {
        mono.push("segoe-symbol".into());
    }
    ctx.set_fonts(fonts);
}

/// Applies the light and dark themes; `dark` selects the one shown.
pub fn apply(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals_of(Theme::Dark, visuals(true));
    ctx.set_visuals_of(Theme::Light, visuals(false));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 24.0;
    });
    ctx.set_theme(if dark { Theme::Dark } else { Theme::Light });
}

/// Readable size: 1.5 MB, 820 B... (units and decimal separator follow the language).
pub fn human_size(n: u64) -> String {
    use urftp_i18n::text;
    let units = [text("units.b"), text("units.kb"), text("units.mb"), text("units.gb"), text("units.tb")];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} {}", units[0]) } else { format!("{v:.1} {}", units[i]).replace('.', text("units.decimal")) }
}

/// WCAG 2.2 relative luminance of an opaque sRGB colour.
fn luminance(c: Color32) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG 2.2 contrast ratio, from 1 to 21.
pub fn contrast(a: Color32, b: Color32) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Colour written `#RRGGBB` (or `RRGGBB`).
pub fn parse_color(text: &str) -> Option<Color32> {
    let h = text.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Backgrounds a text colour of the lists and logs must stand out from in a theme: the
/// panel and the striped rows (selected rows keep the text colour of the theme's
/// selection check).
pub fn list_backgrounds(dark: bool) -> [Color32; 2] {
    let v = visuals(dark);
    [v.panel_fill, v.faint_bg_color]
}

/// Lowest contrast of a text colour against the list backgrounds of a theme.
pub fn list_contrast(c: Color32, dark: bool) -> f64 {
    list_backgrounds(dark).iter().map(|&b| contrast(c, b)).fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::{Color32, contrast, palette, visuals};

    /// Records a failure when `fg` on `bg` is below `min`.
    fn check(failures: &mut Vec<String>, what: &str, fg: Color32, bg: Color32, min: f64) {
        if fg.a() != 255 || bg.a() != 255 {
            failures.push(format!("{what}: colours must be opaque to be measured ({fg:?} on {bg:?})"));
            return;
        }
        let r = contrast(fg, bg);
        if r < min {
            failures.push(format!("{what}: {r:.2}:1 below {min}:1 ({fg:?} on {bg:?})"));
        }
    }

    #[test]
    fn contrast_formula() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 1e-9);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-9);
        // Reference value: #767676 on white is the classic 4.54:1.
        assert!((contrast(Color32::from_gray(0x76), Color32::WHITE) - 4.54).abs() < 0.01);
    }

    /// WCAG 2.2 AA: 4.5:1 for text (1.4.3), 3:1 for focus and state indicators (1.4.11).
    #[test]
    fn themes_meet_wcag_aa() {
        const TEXT: f64 = 4.5;
        const UI: f64 = 3.0;
        let mut failures = Vec::new();
        for dark in [true, false] {
            let v = visuals(dark);
            let p = palette(dark);
            let theme = if dark { "dark" } else { "light" };
            let backgrounds = [
                ("panel", v.panel_fill),
                ("window", v.window_fill),
                ("field", v.extreme_bg_color),
                ("striped row", v.faint_bg_color),
                ("hovered row", v.widgets.hovered.bg_fill),
            ];
            let texts = [
                ("text", v.text_color()),
                ("weak text", v.weak_text_color()),
                ("accent text", p.accent_text),
                ("log command", p.command),
                ("success", p.success),
                ("status", p.status),
                ("error", p.error),
                ("warning", p.warning),
            ];
            for (bg_name, bg) in backgrounds {
                for (fg_name, fg) in texts {
                    check(&mut failures, &format!("{theme}: {fg_name} on {bg_name}"), fg, bg, TEXT);
                }
            }
            check(&mut failures, &format!("{theme}: code"), v.text_color(), v.code_bg_color, TEXT);
            check(&mut failures, &format!("{theme}: text in a selection"), v.text_color(), v.selection.bg_fill, TEXT);
            check(&mut failures, &format!("{theme}: selected row"), v.selection.stroke.color, v.selection.bg_fill, TEXT);
            for (state, w) in [
                ("inactive", &v.widgets.inactive),
                ("hovered", &v.widgets.hovered),
                ("active", &v.widgets.active),
                ("open", &v.widgets.open),
            ] {
                check(&mut failures, &format!("{theme}: {state} button"), w.fg_stroke.color, w.weak_bg_fill, TEXT);
            }
            check(&mut failures, &format!("{theme}: primary button"), p.on_accent, p.accent_fill, TEXT);
            check(&mut failures, &format!("{theme}: danger button"), p.on_danger, p.danger_fill, TEXT);
            // Indicators: focus outline of fields and buttons, progress bars, selection.
            check(&mut failures, &format!("{theme}: focused field outline"), v.selection.stroke.color, v.extreme_bg_color, UI);
            for (bg_name, bg) in [("panel", v.panel_fill), ("window", v.window_fill)] {
                check(&mut failures, &format!("{theme}: focused button outline on {bg_name}"), v.widgets.active.bg_stroke.color, bg, UI);
            }
            check(&mut failures, &format!("{theme}: progress bar"), p.accent_fill, v.extreme_bg_color, UI);
            for (bg_name, bg) in backgrounds {
                check(&mut failures, &format!("{theme}: list cursor on {bg_name}"), v.selection.stroke.color, bg, UI);
            }
            if v.widgets.active.bg_stroke.width < 2.0 {
                failures.push(format!("{theme}: the focus outline must be at least 2 px wide"));
            }
        }
        assert!(
            failures.is_empty(),
            "WCAG 2.2 AA not met:
{}",
            failures.join(
                "
"
            )
        );
    }

    #[test]
    fn sizes() {
        let _lock = crate::TEST_LOCK.lock();
        urftp_i18n::set_language(urftp_i18n::Lang::Fr);
        assert_eq!(super::human_size(820), "820 o");
        assert_eq!(super::human_size(1536), "1,5 Ko");
        assert_eq!(super::human_size(5 * 1024 * 1024 * 1024), "5,0 Go");
        urftp_i18n::set_language(urftp_i18n::Lang::En);
        assert_eq!(super::human_size(1536), "1.5 KB");
    }
}
