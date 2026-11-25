//! Help window (`F1`, Help menu): the keyboard shortcuts and the reference documentation
//! (`docs/OPTIONS.md` / `docs/OPTIONS.fr.md`, built into the program), with an index of
//! its sections and a search.

use std::sync::OnceLock;

use super::*;

const DOC_EN: &str = include_str!("../../../../docs/OPTIONS.md");
const DOC_FR: &str = include_str!("../../../../docs/OPTIONS.fr.md");

/// A section of the documentation.
pub(super) struct Section {
    pub(super) title: String,
    /// 2 for `##`, 3 for `###`.
    pub(super) level: u8,
    /// Markdown text of the section.
    pub(super) body: String,
}

/// Sections of a Markdown document (`##` and `###` titles; the text before the first one
/// is left out).
pub(super) fn sections(doc: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut fence = false;
    for line in doc.lines() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
        let heading = if fence {
            None
        } else if let Some(t) = line.strip_prefix("### ") {
            Some((3, t))
        } else {
            line.strip_prefix("## ").map(|t| (2, t))
        };
        match heading {
            Some((level, t)) => out.push(Section { title: plain(t), level, body: String::new() }),
            None => {
                if let Some(s) = out.last_mut() {
                    s.body.push_str(line);
                    s.body.push('\n');
                }
            }
        }
    }
    out
}

/// Sections of the documentation in the language of the interface (read once).
fn doc_sections() -> &'static [Section] {
    static EN: OnceLock<Vec<Section>> = OnceLock::new();
    static FR: OnceLock<Vec<Section>> = OnceLock::new();
    if language() == Lang::Fr { FR.get_or_init(|| sections(DOC_FR)) } else { EN.get_or_init(|| sections(DOC_EN)) }
}

/// Text without the Markdown marks: `**bold**`, `*italic*`, `[text](link)`; code spans
/// (`` `x` ``) are kept as written, without their quotes.
pub(super) fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        match c {
            '`' => {
                let inner = &rest[1..];
                match inner.find('`') {
                    Some(end) => {
                        out.push_str(&inner[..end]);
                        rest = &inner[end + 1..];
                    }
                    None => {
                        out.push_str(inner);
                        rest = "";
                    }
                }
            }
            '*' => rest = &rest[1..],
            '[' => match (rest.find("]("), rest.find(')')) {
                (Some(close), Some(end)) if end > close => {
                    out.push_str(&plain(&rest[1..close]));
                    rest = &rest[end + 1..];
                }
                _ => {
                    out.push('[');
                    rest = &rest[1..];
                }
            },
            '\\' if rest.len() > 1 => {
                let next = rest[1..].chars().next().unwrap_or('\\');
                out.push(next);
                rest = &rest[1 + next.len_utf8()..];
            }
            _ => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    out
}

/// Cells of a table row `| a | b |` (pipes inside code spans or escaped are kept).
pub(super) fn cells(row: &str) -> Vec<String> {
    let row = row.trim().trim_start_matches('|').trim_end_matches('|');
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut code = false;
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                code = !code;
                cell.push(c);
            }
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' if !code => out.push(std::mem::take(&mut cell).trim().to_string()),
            _ => cell.push(c),
        }
    }
    out.push(cell.trim().to_string());
    out
}

/// Shows the Markdown text of a section: paragraphs, lists, tables, code; pictures are
/// left out (they are in `docs/`).
fn render(ui: &mut egui::Ui, body: &str, salt: usize) {
    let lines: Vec<&str> = body.lines().collect();
    let mut para = String::new();
    let mut tables = 0;
    let flush = |ui: &mut egui::Ui, para: &mut String| {
        if !para.trim().is_empty() {
            ui.label(plain(para.trim()));
        }
        para.clear();
    };
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("```") {
            flush(ui, &mut para);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            egui::Frame::new().fill(ui.visuals().faint_bg_color).inner_margin(6.0).corner_radius(4.0).show(ui, |ui| {
                ui.label(RichText::new(code.join("\n")).monospace());
            });
        } else if t.is_empty() || t.starts_with("![") {
            flush(ui, &mut para);
            i += 1;
        } else if t.starts_with('|') {
            flush(ui, &mut para);
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let c = cells(lines[i]);
                // The line of dashes under the header.
                if !c.iter().all(|x| !x.is_empty() && x.chars().all(|ch| matches!(ch, '-' | ':'))) {
                    rows.push(c);
                }
                i += 1;
            }
            tables += 1;
            let columns = rows.first().map_or(1, Vec::len);
            egui::Grid::new(("help-table", salt, tables))
                .num_columns(columns)
                .striped(true)
                .spacing([12.0, 6.0])
                .max_col_width(360.0)
                .show(ui, |ui| {
                    for (r, row) in rows.iter().enumerate() {
                        for cell in row {
                            let text = RichText::new(plain(cell));
                            ui.label(if r == 0 { text.strong() } else { text });
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(4.0);
        } else if let Some(item) = t.strip_prefix("- ") {
            flush(ui, &mut para);
            let mut text = item.to_string();
            i += 1;
            // Continuation lines of the item.
            while i < lines.len() && lines[i].starts_with("  ") && !lines[i].trim().starts_with("- ") && !lines[i].trim().is_empty() {
                text.push(' ');
                text.push_str(lines[i].trim());
                i += 1;
            }
            ui.horizontal_wrapped(|ui| {
                ui.label("•");
                ui.label(plain(&text));
            });
        } else if let Some(h) = t.strip_prefix("#### ") {
            flush(ui, &mut para);
            ui.label(RichText::new(plain(h)).strong());
            i += 1;
        } else {
            para.push_str(t);
            para.push(' ');
            i += 1;
        }
    }
    flush(ui, &mut para);
}

impl UltraRushApp {
    /// Opens the help on a section (0: the keyboard shortcuts, then the sections of the
    /// documentation from 1).
    pub(super) fn open_help(&mut self, section: usize) {
        self.help_section = section;
        self.shortcuts_open = true;
    }

    /// Help window: index (with a search) on the left, the section on the right.
    pub(super) fn help_window(&mut self, ctx: &egui::Context) {
        if !self.shortcuts_open {
            return;
        }
        let chosen = self.options.settings.interface.shortcuts.clone();
        let docs = doc_sections();
        let mut open = true;
        let query = self.help_query.trim().to_lowercase();
        let shown: Vec<usize> = (0..docs.len())
            .filter(|&i| query.is_empty() || docs[i].title.to_lowercase().contains(&query) || docs[i].body.to_lowercase().contains(&query))
            .collect();
        egui::Window::new(t!("help.title"))
            .id(egui::Id::unique("shortcuts-window"))
            .default_pos([200.0, 90.0])
            .default_size([960.0, 620.0])
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(250.0);
                        let r = ui.add(theme::single_line(&mut self.help_query).hint_text(t!("help.search")).desired_width(240.0));
                        theme::name(&r, t!("help.search"));
                        egui::ScrollArea::vertical().id_salt("help-index").max_height(ui.available_height()).show(ui, |ui| {
                            if ui.selectable_label(self.help_section == 0, format!("⌨ {}", t!("shortcuts.title"))).clicked() {
                                self.help_section = 0;
                            }
                            for &i in &shown {
                                let s = &docs[i];
                                let label = if s.level == 3 { format!("    {}", s.title) } else { s.title.clone() };
                                let text = if s.level == 2 { RichText::new(label).strong() } else { RichText::new(label) };
                                if ui.selectable_label(self.help_section == i + 1, text).clicked() {
                                    self.help_section = i + 1;
                                }
                            }
                            if shown.is_empty() {
                                ui.label(RichText::new(t!("help.no_match")).italics());
                            }
                        });
                    });
                    ui.separator();
                    // A column: the page is read from top to bottom.
                    let page = egui::ScrollArea::vertical().id_salt(("help-page", self.help_section));
                    page.show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            match self.help_section.checked_sub(1).and_then(|i| docs.get(i).map(|s| (i, s))) {
                                Some((i, s)) => {
                                    ui.heading(&s.title);
                                    ui.add_space(4.0);
                                    render(ui, &s.body, i);
                                }
                                None => {
                                    ui.heading(t!("shortcuts.title"));
                                    keyboard_ui(ui, &chosen);
                                }
                            }
                        });
                    });
                });
            });
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.shortcuts_open = false;
        }
    }
}

/// The keys of every command.
fn keyboard_ui(ui: &mut egui::Ui, chosen: &std::collections::BTreeMap<String, String>) {
    egui::Grid::new("shortcuts").num_columns(2).spacing([24.0, 6.0]).striped(true).show(ui, |ui| {
        for (keys, what) in FIXED_SHORTCUTS.iter().filter(|(k, _)| *k != "shortcuts.k_f3_f4") {
            ui.label(RichText::new(urftp_i18n::text(keys)).monospace().strong());
            ui.label(urftp_i18n::text(what));
            ui.end_row();
        }
        for c in input::Command::ALL {
            let keys = input::keys_of(c, chosen).iter().map(input::format).collect::<Vec<_>>().join(", ");
            ui.label(RichText::new(keys).monospace().strong());
            ui.label(urftp_i18n::text(&c.label_key()));
            ui.end_row();
        }
    });
    ui.label(RichText::new(t!("shortcuts.change_note")).weak());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documentation_sections_and_marks() {
        for doc in [DOC_EN, DOC_FR] {
            let s = sections(doc);
            assert!(s.len() > 20, "{}", s.len());
            assert!(s.iter().all(|x| !x.title.is_empty() && !x.title.contains('`')));
        }
        assert!(sections(DOC_EN).iter().any(|s| s.title == "Advanced transfer" && s.body.contains("MODE Z")));
        assert_eq!(plain("**Bold** and *it* `*.zip` [link](docs/X.md)"), "Bold and it *.zip link");
        assert_eq!(plain(r"a \| b"), "a | b");
        assert_eq!(cells("| `a|b` | x \\| y | z |"), ["`a|b`", "x | y", "z"]);
    }
}
