//! "Send RAW command" dialog (`Ctrl+R`: several lines, to the active panel, both panels
//! or every connected site) and the predefined site commands of the FTP menu.

use urftp_core::settings::SiteCommand;

use super::*;

/// Where the RAW commands go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum RawTarget {
    /// The remote panel that has the keyboard (or else the other one).
    #[default]
    Active,
    /// Both panels of the tab.
    Both,
    /// Every connected panel of every tab.
    All,
}

/// State of the dialog; the replies stay listed while it is closed.
#[derive(Default)]
pub(super) struct RawDialog {
    pub(super) open: bool,
    pub(super) text: String,
    pub(super) target: RawTarget,
    focus: bool,
    /// Replies received: (site, command, lines, success).
    pub(super) history: Vec<(String, String, Vec<String>, bool)>,
}

impl RawDialog {
    /// Opens the dialog with `text` (a predefined command whose parameter is to fill in).
    pub(super) fn open_with(&mut self, text: Option<String>) {
        self.open = true;
        self.focus = true;
        if let Some(t) = text {
            self.text = t;
        }
    }

    /// Keeps the replies of the RAW commands for the dialog.
    pub(super) fn record(&mut self, reply: &(String, String, Vec<String>, bool)) {
        self.history.push(reply.clone());
        if self.history.len() > 200 {
            self.history.drain(..50);
        }
    }
}

/// A predefined command asks for a parameter (`%1`) before it is sent.
fn needs_parameter(c: &SiteCommand) -> bool {
    c.lines.contains("%1")
}

/// Lines of a command: one command per non-empty line.
fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

impl UltraRushApp {
    /// Panels a RAW command goes to: (tab, side).
    fn raw_targets(&self, target: RawTarget) -> Vec<(usize, usize)> {
        let connected = |t: usize, s: usize| matches!(&self.tabs[t].sides[s], Side::Remote(r) if r.state == RemoteState::Connected);
        let a = self.active;
        match target {
            RawTarget::Active => {
                let f = self.tabs[a].focus;
                [f, 1 - f].into_iter().find(|&s| connected(a, s)).map(|s| vec![(a, s)]).unwrap_or_default()
            }
            RawTarget::Both => (0..2).filter(|&s| connected(a, s)).map(|s| (a, s)).collect(),
            RawTarget::All => (0..self.tabs.len()).flat_map(|t| (0..2).map(move |s| (t, s))).filter(|&(t, s)| connected(t, s)).collect(),
        }
    }

    /// Sends every line of `text` to the panels of `target`. Returns how many panels.
    pub(super) fn send_raw(&mut self, text: &str, target: RawTarget) -> usize {
        let lines = lines_of(text);
        let panels = self.raw_targets(target);
        if panels.is_empty() {
            let a = self.active;
            let f = self.tabs[a].focus;
            self.tabs[a].push_ftp(f, LogKind::Error, t!("raw.no_connection"));
            return 0;
        }
        for &(t, s) in &panels {
            if let Side::Remote(r) = &self.tabs[t].sides[s] {
                for l in &lines {
                    r.handle.send(BrowserCmd::Raw(l.clone()));
                }
            }
        }
        panels.len()
    }

    /// A predefined command of the FTP menu: sent to the active panel, or put in the
    /// RAW dialog when it has a parameter to fill in.
    pub(super) fn site_command(&mut self, index: usize) {
        let Some(c) = self.options.settings.site_commands.get(index).cloned() else { return };
        if needs_parameter(&c) {
            self.raw.open_with(Some(c.lines));
        } else {
            self.send_raw(&c.lines, RawTarget::Active);
        }
    }

    pub(super) fn raw_dialog_window(&mut self, ctx: &egui::Context) {
        if !self.raw.open {
            return;
        }
        let mut open = true;
        let mut send = false;
        let d = &mut self.raw;
        egui::Window::new(t!("raw.title")).id(egui::Id::new("raw-dialog")).default_size([560.0, 420.0]).open(&mut open).show(ctx, |ui| {
            let h = ui.label(t!("raw.commands"));
            let r = ui.add(
                egui::TextEdit::multiline(&mut d.text)
                    .font(egui::TextStyle::Monospace)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .hint_text("SITE WHO"),
            );
            let r = r.labelled_by(h.id);
            if std::mem::take(&mut d.focus) {
                r.request_focus();
            }
            // Ctrl+Enter sends (Enter adds a line).
            if r.has_focus() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
                send = true;
            }
            ui.label(RichText::new(t!("raw.hint")).weak());
            ui.horizontal(|ui| {
                ui.radio_value(&mut d.target, RawTarget::Active, t!("raw.to_active"));
                ui.radio_value(&mut d.target, RawTarget::Both, t!("raw.to_both"));
                ui.radio_value(&mut d.target, RawTarget::All, t!("raw.to_all"));
            });
            ui.horizontal(|ui| {
                if theme::primary_button(ui, t!("raw.send")).clicked() {
                    send = true;
                }
                if ui.button(t!("raw.clear")).clicked() {
                    d.history.clear();
                }
            });
            ui.separator();
            let p = theme::colors(ui);
            egui::ScrollArea::vertical().auto_shrink(false).stick_to_bottom(true).show(ui, |ui| {
                for (site, cmd, lines, ok) in &d.history {
                    ui.label(RichText::new(format!("[{site}] {cmd}")).monospace().color(p.command));
                    for l in lines {
                        ui.label(RichText::new(l).monospace().color(if *ok { p.success } else { p.error }));
                    }
                }
            });
        });
        if !open {
            self.raw.open = false;
        }
        if send {
            let (text, target) = (self.raw.text.clone(), self.raw.target);
            self.send_raw(&text, target);
        }
    }
}

/// "Site commands" sub-menu of the FTP menu.
pub(super) fn commands_menu(ui: &mut egui::Ui, commands: &[SiteCommand], actions: &mut Vec<Action>) {
    if ui.button(t!("raw.menu_send")).clicked() {
        actions.push(Action::RawDialog);
        ui.close();
    }
    ui.menu_button(t!("raw.menu_commands"), |ui| {
        if commands.is_empty() {
            ui.label(RichText::new(t!("raw.no_commands")).italics());
        }
        for (i, c) in commands.iter().enumerate() {
            let label = if needs_parameter(c) { format!("{}…", c.name) } else { c.name.clone() };
            if ui.button(label).on_hover_text(c.lines.as_str()).clicked() {
                actions.push(Action::SiteCommand(i));
                ui.close();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        assert_eq!(lines_of(" SITE WHO \n\n STAT\r\n"), ["SITE WHO", "STAT"]);
        assert!(needs_parameter(&SiteCommand { name: "x".into(), lines: "SITE SEARCH %1".into() }));
    }
}
