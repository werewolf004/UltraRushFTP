//! Modern theme: rounded corners, airy spacing, blue accent, light or dark.

use eframe::egui::{self, Color32, CornerRadius, Shadow, Stroke, Theme, Visuals};

/// Accent colour (selection, active buttons, progress).
pub const ACCENT: Color32 = Color32::from_rgb(0x4C, 0x8D, 0xFF);
/// Log colours (as in v2/v3: blue command, green reply, red error).
pub const LOG_COMMAND: Color32 = Color32::from_rgb(0x6F, 0xA8, 0xFF);
pub const LOG_REPLY: Color32 = Color32::from_rgb(0x4C, 0xC3, 0x7A);
pub const LOG_ERROR: Color32 = Color32::from_rgb(0xF0, 0x5D, 0x5D);
pub const LOG_STATUS: Color32 = Color32::from_rgb(0x9A, 0xA3, 0xB2);

fn tune(mut v: Visuals, dark: bool) -> Visuals {
    let r = CornerRadius::same(7);
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = r;
    }
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.selection.bg_fill = if dark { ACCENT.linear_multiply(0.55) } else { ACCENT.linear_multiply(0.35) };
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.window_shadow = Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 45 }) };
    v.popup_shadow = Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(if dark { 90 } else { 35 }) };
    if dark {
        v.panel_fill = Color32::from_rgb(0x1B, 0x1E, 0x24);
        v.window_fill = Color32::from_rgb(0x22, 0x26, 0x2E);
        v.extreme_bg_color = Color32::from_rgb(0x15, 0x17, 0x1C);
        v.faint_bg_color = Color32::from_rgb(0x20, 0x24, 0x2B);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0x2A, 0x2F, 0x38);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x33, 0x39, 0x44);
    } else {
        // Light but soft: bluish greys instead of pure white, dark grey text rather than
        // black, to limit glare during long sessions.
        v.panel_fill = Color32::from_rgb(0xD9, 0xDD, 0xE3);
        v.window_fill = Color32::from_rgb(0xE2, 0xE5, 0xEA);
        v.extreme_bg_color = Color32::from_rgb(0xE6, 0xE9, 0xED);
        v.faint_bg_color = Color32::from_rgb(0xDE, 0xE2, 0xE8);
        v.code_bg_color = Color32::from_rgb(0xD0, 0xD5, 0xDC);
        v.widgets.noninteractive.bg_fill = v.panel_fill;
        v.widgets.noninteractive.weak_bg_fill = v.panel_fill;
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xC9, 0xCF, 0xD7);
        v.widgets.inactive.bg_fill = Color32::from_rgb(0xC9, 0xCF, 0xD7);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xBD, 0xC4, 0xCE);
        v.widgets.hovered.bg_fill = Color32::from_rgb(0xBD, 0xC4, 0xCE);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(0xB2, 0xBA, 0xC5);
        let text = Color32::from_rgb(0x2B, 0x30, 0x38);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xC2, 0xC8, 0xD0));
    }
    v
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
    ctx.set_visuals_of(Theme::Dark, tune(Visuals::dark(), true));
    ctx.set_visuals_of(Theme::Light, tune(Visuals::light(), false));
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

#[cfg(test)]
mod tests {
    #[test]
    fn sizes() {
        urftp_i18n::set_language(urftp_i18n::Lang::Fr);
        assert_eq!(super::human_size(820), "820 o");
        assert_eq!(super::human_size(1536), "1,5 Ko");
        assert_eq!(super::human_size(5 * 1024 * 1024 * 1024), "5,0 Go");
        urftp_i18n::set_language(urftp_i18n::Lang::En);
        assert_eq!(super::human_size(1536), "1.5 KB");
    }
}
