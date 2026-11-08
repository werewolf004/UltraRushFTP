//! Master password (site manager, options and the locked window), daily backup and
//! restore of the configuration, and the window shown again by a second start.

use urftp_core::{backup, master};

use super::*;

/// What the password unlocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnlockFor {
    SiteManager,
    Options,
    /// The locked window.
    Window,
}

/// Password dialog.
pub(super) struct Unlock {
    pub(super) target: UnlockFor,
    pub(super) password: String,
    error: bool,
}

/// `YYYY-MM-DD` and `YYYY-MM-DD_HHMMSS` of now (local time).
fn stamps() -> (String, String) {
    let (y, mo, d, h, mi, s) = urftp_sys::win::local_time();
    (format!("{y:04}-{mo:02}-{d:02}"), format!("{y:04}-{mo:02}-{d:02}_{h:02}{mi:02}{s:02}"))
}

/// Backup made at the first start of the day (4 kept); errors are left for the next start.
pub(crate) fn daily_backup() {
    let data = urftp_core::sitestore::data_dir();
    let (today, stamp) = stamps();
    if data.is_dir() && !backup::has_backup_of(&data, &today) && backup::backup(&data, &stamp).is_ok() {
        let _ = backup::prune(&data, 4);
    }
}

impl UltraRushApp {
    fn has_master(&self) -> bool {
        !self.options.settings.master_hash.is_empty()
    }

    /// A second start asked for the window, or the tray icon was clicked.
    pub(super) fn show_main_window(&mut self, ctx: &egui::Context) {
        self.notifier.hidden = false;
        urftp_sys::tray::show_window(self.notifier.main);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// Locks the window (master password needed to use it again); hidden in the
    /// notification area when minimizing goes there.
    pub(super) fn lock(&mut self, ctx: &egui::Context) {
        if !self.has_master() {
            return;
        }
        self.unlocked = false;
        self.unlock = Some(Unlock { target: UnlockFor::Window, password: String::new(), error: false });
        if self.options.settings.notify.minimize_to_tray && self.notifier.main != 0 {
            self.notifier.hidden = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    /// Closes the site manager and the options until the master password is given.
    pub(super) fn guard_windows(&mut self) {
        if !self.has_master() || self.unlocked || self.unlock.is_some() {
            return;
        }
        let target = if self.sites.open {
            self.sites.open = false;
            Some(UnlockFor::SiteManager)
        } else if self.options.open {
            self.options.open = false;
            Some(UnlockFor::Options)
        } else {
            None
        };
        if let Some(target) = target {
            self.unlock = Some(Unlock { target, password: String::new(), error: false });
        }
    }

    pub(super) fn unlock_dialog(&mut self, ctx: &egui::Context) {
        let hash = self.options.settings.master_hash.clone();
        let Some(u) = &mut self.unlock else { return };
        let mut done = None;
        egui::Modal::new(egui::Id::new("unlock")).show(ctx, |ui| {
            ui.set_min_width(360.0);
            let h = ui.heading(if u.target == UnlockFor::Window { t!("lock.locked") } else { t!("lock.title") });
            ui.label(t!("lock.prompt"));
            let r = ui.add(theme::single_line(&mut u.password).password(true).desired_width(f32::INFINITY)).labelled_by(h.id);
            focus_once(ui, &r);
            if u.error {
                ui.colored_label(theme::colors(ui).error, t!("lock.wrong"));
            }
            ui.horizontal(|ui| {
                if theme::primary_button(ui, t!("lock.unlock")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    done = Some(true);
                }
                // A locked window cannot be left without the password.
                if u.target != UnlockFor::Window
                    && (ui.button(t!("common.cancel")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)))
                {
                    done = Some(false);
                }
            });
        });
        match done {
            Some(true) => {
                if master::verify_password(&u.password, &hash) {
                    let target = u.target;
                    self.unlock = None;
                    self.unlocked = true;
                    match target {
                        UnlockFor::SiteManager => self.sites.open = true,
                        UnlockFor::Options => self.options.open = true,
                        UnlockFor::Window => {}
                    }
                } else {
                    u.error = true;
                    u.password.clear();
                }
            }
            Some(false) => self.unlock = None,
            None => {}
        }
    }

    /// File menu entries: backup now, restore one.
    pub(super) fn backup_menu(&mut self, ui: &mut egui::Ui) {
        if ui.button(t!("backup.now")).clicked() {
            ui.close();
            let data = urftp_core::sitestore::data_dir();
            // What the application holds is written first.
            self.options.save();
            self.sites.save();
            let msg = match backup::backup(&data, &stamps().1) {
                Ok(p) => (LogKind::Status, t!("backup.done", path = p.display())),
                Err(e) => (LogKind::Error, t!("backup.failed", error = e)),
            };
            let a = self.active;
            self.tabs[a].push_log(0, msg.0, msg.1);
        }
        let list = backup::list(&urftp_core::sitestore::data_dir());
        ui.menu_button(t!("backup.restore"), |ui| {
            if list.is_empty() {
                ui.label(RichText::new(t!("backup.none")).italics());
            }
            for p in list.iter().take(20) {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if ui.button(name).clicked() {
                    ui.close();
                    self.confirm = Some(confirm::Confirm::Restore(p.clone()));
                }
            }
        });
        if ui.add_enabled(self.has_master(), egui::Button::new(t!("lock.menu"))).on_disabled_hover_text(t!("lock.menu_hint")).clicked() {
            ui.close();
            let ctx = ui.ctx().clone();
            self.lock(&ctx);
        }
    }

    /// Copies a backup back and stops at once: saving on exit would undo it.
    pub(super) fn restore_backup(&mut self, from: &Path) {
        let data = urftp_core::sitestore::data_dir();
        match backup::restore(&data, from) {
            Ok(_) => {
                if !cfg!(test) {
                    rfd::MessageDialog::new().set_title("UltraRushFTP").set_description(t!("backup.restored")).show();
                    std::process::exit(0);
                }
            }
            Err(e) => {
                let a = self.active;
                self.tabs[a].push_log(0, LogKind::Error, t!("backup.restore_failed", error = e));
            }
        }
    }
}
