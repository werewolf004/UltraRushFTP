//! "Send RAW command" dialog (`Ctrl+R`: several lines, to the active panel, both panels
//! or every connected site) and the commands of the command designer (FTP menu, menu
//! of the remote panels, their own keys).

use urftp_core::commands::{self, CommandError, CommandSpec, CommandValues};
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
    /// Questions of a command (`%d[...]`, `%1`) before it runs.
    pub(super) ask: Option<CommandAsk>,
}

/// Questions of a command, answered in a dialog before it is sent.
pub(super) struct CommandAsk {
    pub(super) index: usize,
    pub(super) name: String,
    pub(super) captions: Vec<String>,
    pub(super) answers: Vec<String>,
    focus: bool,
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

/// A command asks questions before it is sent.
fn needs_parameter(c: &SiteCommand) -> bool {
    !commands::parse(&c.lines).prompts.is_empty()
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

    /// A command of the designer: its questions first, if any, then sent.
    pub(super) fn site_command(&mut self, index: usize) {
        let Some(c) = self.options.settings.site_commands.get(index).cloned() else { return };
        let spec = commands::parse(&c.lines);
        if spec.prompts.is_empty() {
            self.run_command(&spec, &[]);
        } else {
            let n = spec.prompts.len();
            self.raw.ask = Some(CommandAsk { index, name: c.name, captions: spec.prompts, answers: vec![String::new(); n], focus: true });
        }
    }

    /// Fills the parameters from the active remote panel and sends the command.
    pub(super) fn run_command(&mut self, spec: &CommandSpec, answers: &[String]) {
        let source = self.raw_targets(RawTarget::Active).first().copied();
        let (files, path): (Vec<String>, String) = source
            .map(|(t, s)| {
                let side = &self.tabs[t].sides[s];
                let dir = match side {
                    Side::Remote(r) => r.dir.clone(),
                    Side::Local(_) => String::new(),
                };
                (side.picked().into_iter().map(|(n, _)| n).collect(), dir)
            })
            .unwrap_or_default();
        let uses_clipboard = spec.lines.iter().any(|l| l.to_ascii_lowercase().contains("%c"));
        let clipboard = if uses_clipboard { urftp_sys::win::clipboard_text().unwrap_or_default() } else { String::new() };
        let values = CommandValues { files: &files, path: &path, clipboard: &clipboard, answers };
        let lines = match commands::expand(spec, &values) {
            Ok(lines) => lines,
            Err(e) => {
                let a = self.active;
                let f = self.tabs[a].focus;
                let text = match e {
                    CommandError::NoSelection => t!("raw.no_selection"),
                    CommandError::Empty => t!("raw.empty_command"),
                };
                self.tabs[a].push_ftp(f, LogKind::Error, text);
                return;
            }
        };
        let target = if spec.all { RawTarget::All } else { RawTarget::Active };
        if self.send_raw(&lines.join("\n"), target) == 0 {
            return;
        }
        if spec.clear_cache {
            for (t, s) in self.raw_targets(target) {
                if let Side::Remote(r) = &self.tabs[t].sides[s] {
                    r.handle.send(BrowserCmd::ClearCache);
                    r.handle.send(BrowserCmd::Refresh);
                }
            }
        }
        if spec.window {
            self.raw.open_with(None);
        }
    }

    /// Dialog of the questions of a command.
    pub(super) fn command_ask_window(&mut self, ctx: &egui::Context) {
        let Some(ask) = &mut self.raw.ask else { return };
        let mut done = None;
        egui::Modal::new(egui::Id::new("command-ask")).show(ctx, |ui| {
            ui.set_min_width(380.0);
            ui.heading(&ask.name);
            for (i, (caption, answer)) in ask.captions.iter().zip(ask.answers.iter_mut()).enumerate() {
                let label = if caption.is_empty() { t!("raw.parameter", n = i + 1) } else { caption.clone() };
                let l = ui.label(label);
                let r = ui.add(theme::single_line(answer).desired_width(f32::INFINITY)).labelled_by(l.id);
                if i == 0 && std::mem::take(&mut ask.focus) {
                    r.request_focus();
                }
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            ui.horizontal(|ui| {
                if theme::primary_button(ui, t!("raw.send")).clicked() || enter {
                    done = Some(true);
                }
                if ui.button(t!("common.cancel")).clicked() || escape {
                    done = Some(false);
                }
            });
        });
        if let Some(send) = done
            && let Some(ask) = self.raw.ask.take()
            && send
            && let Some(c) = self.options.settings.site_commands.get(ask.index)
        {
            let spec = commands::parse(&c.lines);
            self.run_command(&spec, &ask.answers);
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
    ui.menu_button(t!("raw.menu_commands"), |ui| commands_tree(ui, commands, actions));
}

/// The commands, their folders as sub-menus.
pub(super) fn commands_tree(ui: &mut egui::Ui, commands: &[SiteCommand], actions: &mut Vec<Action>) {
    if commands.is_empty() {
        ui.label(RichText::new(t!("raw.no_commands")).italics());
    }
    commands_level(ui, commands, &[], actions);
}

fn commands_level(ui: &mut egui::Ui, commands: &[SiteCommand], prefix: &[&str], actions: &mut Vec<Action>) {
    let mut subs: Vec<&str> = Vec::new();
    for c in commands {
        let f = c.folders();
        if f.len() > prefix.len() && f[..prefix.len()] == *prefix && !subs.contains(&f[prefix.len()]) {
            subs.push(f[prefix.len()]);
        }
    }
    for sub in subs {
        let mut deeper = prefix.to_vec();
        deeper.push(sub);
        ui.menu_button(format!("📁 {sub}"), |ui| commands_level(ui, commands, &deeper, actions));
    }
    for (i, c) in commands.iter().enumerate().filter(|(_, c)| c.folders() == prefix) {
        let label = if needs_parameter(c) { format!("{}…", c.name) } else { c.name.clone() };
        let b = egui::Button::new(label).shortcut_text(c.hotkey.trim());
        if ui.add(b).on_hover_text(c.lines.as_str()).clicked() {
            actions.push(Action::SiteCommand(i));
            ui.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines() {
        assert_eq!(lines_of(" SITE WHO \n\n STAT\r\n"), ["SITE WHO", "STAT"]);
        assert!(needs_parameter(&SiteCommand::new("x", "SITE SEARCH %1")));
        assert!(needs_parameter(&SiteCommand::new("x", "SITE ADDUSER %d[User]")));
        assert!(!needs_parameter(&SiteCommand::new("x", "SITE WHO\n%all%")));
    }
}
