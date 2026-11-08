//! Confirmations of the options: quit while transfers run, close a busy tab, remove
//! queue items.

use super::*;

/// Question waiting for an answer.
pub(super) enum Confirm {
    Exit,
    CloseTab(usize),
    /// Queue removals held back (`QueueRemove`, `QueueClearAll`).
    Queue(Vec<Action>),
}

impl Container {
    /// Connections open (shown or behind the local folder) or a queue left.
    fn busy(&self) -> bool {
        self.engine.is_some()
            || self.queue.lock().has_pending()
            || self.sides.iter().chain(self.parked.iter().flatten()).any(Side::is_remote)
    }
}

impl UltraRushApp {
    /// Items still to transfer in every tab.
    fn pending_items(&self) -> usize {
        self.tabs.iter().map(|t| t.queue.lock().stats()).map(|s| (s.waiting + s.running) as usize).sum()
    }

    /// Closing a tab from the interface: asked first when it is busy.
    pub(super) fn request_close_tab(&mut self, i: usize) {
        let ask = self.options.settings.interface.confirm_close_tab && self.tabs.get(i).is_some_and(Container::busy) && self.tabs.len() > 1;
        if ask {
            self.confirm = Some(Confirm::CloseTab(i));
        } else {
            self.close_tab(i);
        }
    }

    /// The window is being closed: held back while transfers run, until confirmed.
    pub(super) fn check_exit(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.exit_confirmed {
            return;
        }
        if self.options.settings.interface.confirm_exit && self.pending_items() > 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(Confirm::Exit);
        }
    }

    /// Keeps back the queue removals to confirm; returns the other actions.
    pub(super) fn hold_queue_removals(&mut self, actions: Vec<Action>) -> Vec<Action> {
        if !self.options.settings.interface.confirm_queue_remove || std::mem::take(&mut self.queue_remove_confirmed) {
            return actions;
        }
        let (held, rest): (Vec<Action>, Vec<Action>) =
            actions.into_iter().partition(|a| matches!(a, Action::QueueRemove(_) | Action::QueueClearAll));
        if !held.is_empty() {
            self.confirm = Some(Confirm::Queue(held));
        }
        rest
    }

    pub(super) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.confirm else { return };
        let (title, text, yes) = match c {
            Confirm::Exit => (t!("confirm.exit_title"), t!("confirm.exit_text", n = self.pending_items()), t!("confirm.quit")),
            Confirm::CloseTab(i) => {
                let Some(tab) = self.tabs.get(*i) else {
                    self.confirm = None;
                    return;
                };
                let n = tab.queue.lock().items().iter().filter(|it| !it.state.is_finished()).count();
                (t!("confirm.close_tab_title", title = tab.title(i + 1)), t!("confirm.close_tab_text", n = n), t!("confirm.close"))
            }
            Confirm::Queue(actions) => {
                let a = self.active;
                let n = actions
                    .iter()
                    .map(|x| match x {
                        Action::QueueClearAll => self.tabs[a].queue.lock().items().len(),
                        _ => 1,
                    })
                    .sum::<usize>();
                (t!("confirm.queue_title"), t!("confirm.queue_text", n = n), t!("confirm.remove"))
            }
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.heading(title);
            ui.add_space(6.0);
            ui.label(text);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if theme::danger_button(ui, yes).clicked() {
                    answer = Some(true);
                }
                let cancel = ui.button(t!("common.cancel"));
                // The keyboard starts on Cancel.
                focus_once(ui, &cancel);
                if cancel.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(false);
                }
            });
        });
        let Some(yes) = answer else { return };
        let Some(c) = self.confirm.take() else { return };
        if !yes {
            return;
        }
        match c {
            Confirm::Exit => {
                self.exit_confirmed = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Confirm::CloseTab(i) => self.close_tab(i),
            Confirm::Queue(actions) => {
                self.queue_remove_confirmed = true;
                self.apply(actions, ctx);
            }
        }
    }
}
