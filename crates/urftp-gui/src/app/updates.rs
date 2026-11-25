//! New versions: checked by itself a minute after the start when the chosen interval has
//! passed (*Options › General*), or at once from *Help › Check for updates*. A newer
//! version opens a window with its release page; nothing is downloaded or installed.

use std::sync::mpsc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use urftp_core::update;

use super::*;

/// Delay after the start before the automatic check.
const START_DELAY: Duration = Duration::from_secs(60);
/// Longest wait for GitHub.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Result of a check: the tag of the latest release, or why it failed.
type Outcome = Result<String, String>;

/// What the update window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UpdateDialog {
    Checking,
    Available(String),
    UpToDate,
    Failed(String),
}

pub(super) struct UpdateState {
    started: Instant,
    /// The automatic check of this run was looked at (done, or not due).
    auto_done: bool,
    /// Check running; `true`: asked from the menu (every outcome is shown).
    running: Option<(bool, mpsc::Receiver<Outcome>)>,
    pub(super) dialog: Option<UpdateDialog>,
}

impl Default for UpdateState {
    fn default() -> Self {
        // Never from the tests: no request to GitHub.
        Self { started: Instant::now(), auto_done: cfg!(test), running: None, dialog: None }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

impl UltraRushApp {
    /// Starts a check; `manual`: asked from the menu.
    pub(super) fn check_updates(&mut self, manual: bool) {
        if self.updates.running.is_some() {
            if manual {
                self.updates.dialog = Some(UpdateDialog::Checking);
            }
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.rt.spawn(async move {
            let outcome = match urftp_proto::update::latest_release(update::REPOSITORY, TIMEOUT).await {
                Ok(json) => update::latest_tag(&json).ok_or_else(|| t!("update.unreadable")),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(outcome);
        });
        self.updates.running = Some((manual, rx));
        if manual {
            self.updates.dialog = Some(UpdateDialog::Checking);
        }
    }

    /// Automatic check, and the answer of a running one.
    pub(super) fn updates_tick(&mut self, ctx: &egui::Context) {
        if !self.updates.auto_done {
            let s = &self.options.settings.updates;
            if !s.check || !update::is_due(now_secs(), s.last_check, s.frequency) {
                self.updates.auto_done = true;
            } else if let Some(left) = START_DELAY.checked_sub(self.updates.started.elapsed()) {
                ctx.request_repaint_after(left);
            } else {
                self.updates.auto_done = true;
                self.check_updates(false);
            }
        }
        let Some((manual, rx)) = &self.updates.running else { return };
        let manual = *manual;
        let outcome = match rx.try_recv() {
            Ok(o) => o,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(250));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => Err(t!("update.unreadable")),
        };
        self.updates.running = None;
        self.update_outcome(outcome, manual, ctx);
    }

    pub(super) fn update_outcome(&mut self, outcome: Outcome, manual: bool, ctx: &egui::Context) {
        let a = self.active;
        let current = env!("CARGO_PKG_VERSION");
        match outcome {
            Ok(tag) => {
                self.options.settings.updates.last_check = now_secs();
                self.options.save();
                if update::is_newer(&tag, current) {
                    self.tabs[a].push_log(0, LogKind::Status, t!("update.log_available", version = tag.trim_start_matches('v')));
                    if manual || self.options.settings.updates.skipped != tag {
                        self.updates.dialog = Some(UpdateDialog::Available(tag));
                        ctx.request_repaint();
                    }
                } else {
                    self.tabs[a].push_log(0, LogKind::Status, t!("update.log_up_to_date", version = current));
                    if manual {
                        self.updates.dialog = Some(UpdateDialog::UpToDate);
                    }
                }
            }
            Err(e) => {
                self.tabs[a].push_log(0, LogKind::Error, t!("update.log_failed", error = e.clone()));
                if manual {
                    self.updates.dialog = Some(UpdateDialog::Failed(e));
                }
            }
        }
    }

    pub(super) fn updates_window(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.updates.dialog.clone() else { return };
        let current = env!("CARGO_PKG_VERSION");
        let mut open = true;
        let mut close = false;
        egui::Window::new(t!("update.title"))
            .id(egui::Id::new("update-window"))
            .default_pos([380.0, 160.0])
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                match &dialog {
                    UpdateDialog::Checking => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(t!("update.checking"));
                        });
                    }
                    UpdateDialog::UpToDate => {
                        ui.label(t!("update.up_to_date", version = current));
                    }
                    UpdateDialog::Failed(e) => {
                        ui.label(t!("update.failed"));
                        ui.label(RichText::new(e).weak());
                    }
                    UpdateDialog::Available(tag) => {
                        ui.label(RichText::new(t!("update.available", version = tag.trim_start_matches('v'))).strong());
                        ui.label(t!("update.current", version = current));
                        ui.label(RichText::new(t!("update.how")).weak());
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if theme::primary_button(ui, t!("update.open_page")).clicked() {
                                if let Some(url) = update::release_page(tag) {
                                    ctx.open_url(egui::OpenUrl::new_tab(url));
                                }
                                close = true;
                            }
                            if ui.button(t!("update.later")).clicked() {
                                close = true;
                            }
                            if ui.button(t!("update.skip")).clicked() {
                                self.options.settings.updates.skipped = tag.clone();
                                self.options.save();
                                close = true;
                            }
                        });
                        return;
                    }
                }
                ui.add_space(6.0);
                if theme::primary_button(ui, t!("about.close")).clicked() {
                    close = true;
                }
            });
        if !open || close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.updates.dialog = None;
        }
    }
}
