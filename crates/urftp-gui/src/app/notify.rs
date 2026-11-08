//! Notifications: the icon of the notification area (minimize to it, click to come back),
//! the sounds and pop-up messages of the events, the flashing taskbar button, and the
//! folders monitored for new items.

use std::sync::mpsc;
use std::time::Instant;

use urftp_core::settings::{FolderMonitor, NotifyEvent};
use urftp_proto::session::RemoteSession;
use urftp_sys::tray::{Tray, TrayEvent};

use super::*;

/// Pop-up message shown in the window when there is no tray icon.
pub(super) struct Toast {
    title: String,
    text: String,
    until: Instant,
}

/// Run state of a monitored folder.
#[derive(Default)]
pub(super) struct MonitorState {
    /// Names seen at the last listing (`None`: not listed yet, nothing to compare with).
    known: Option<BTreeSet<String>>,
    last: Option<Instant>,
    running: Option<mpsc::Receiver<Result<Vec<String>, String>>>,
}

/// Notification state of the application.
#[derive(Default)]
pub(super) struct Notifier {
    tray: Option<Tray>,
    tray_rx: Option<mpsc::Receiver<TrayEvent>>,
    /// Main window (captured while it has the keyboard).
    pub(super) main: isize,
    /// The window is hidden in the notification area.
    pub(super) hidden: bool,
    toasts: Vec<Toast>,
    monitors: Vec<MonitorState>,
}

/// Names of `now` that `known` did not have, in order.
pub(super) fn new_names(known: &BTreeSet<String>, now: &[String]) -> Vec<String> {
    now.iter().filter(|n| !known.contains(*n)).cloned().collect()
}

fn event_label(e: NotifyEvent) -> String {
    match e {
        NotifyEvent::Connected => t!("notify.ev_connected"),
        NotifyEvent::Disconnected => t!("notify.ev_disconnected"),
        NotifyEvent::QueueFinished => t!("notify.ev_queue_finished"),
        NotifyEvent::TransferFailed => t!("notify.ev_transfer_failed"),
        NotifyEvent::NewItems => t!("notify.ev_new_items"),
    }
}

impl UltraRushApp {
    /// An event happened: its sound, its pop-up (tray or in the window), and the log.
    pub(super) fn notify_event(&mut self, e: NotifyEvent, text: String) {
        let s = self.options.settings.notify.event(e);
        let title = event_label(e);
        // No sound while the tests run.
        if !cfg!(test) {
            match s.sound.trim() {
                "" => {}
                "system" => urftp_sys::tray::play_sound(None),
                file => urftp_sys::tray::play_sound(Some(Path::new(file))),
            }
        }
        if s.popup {
            match &self.notifier.tray {
                Some(t) => t.notify(&title, &text),
                None => self.notifier.toasts.push(Toast {
                    title: title.clone(),
                    text: text.clone(),
                    until: Instant::now() + Duration::from_secs(6),
                }),
            }
        }
        let a = self.active;
        self.tabs[a].push_log(0, LogKind::Status, format!("{title}: {text}"));
    }

    /// Events gathered by the tabs during the frame.
    fn take_tab_events(&mut self) {
        let events: Vec<(NotifyEvent, String)> = self.tabs.iter_mut().flat_map(|t| std::mem::take(&mut t.events)).collect();
        let queue_end = events.iter().any(|(e, _)| *e == NotifyEvent::QueueFinished);
        for (e, text) in events {
            self.notify_event(e, text);
        }
        if queue_end && self.options.settings.notify.flash_on_queue_end && !self.notifier.hidden {
            urftp_sys::tray::flash(self.notifier.main);
        }
    }

    /// Every frame (also while hidden): tray icon, clicks on it, events, monitored folders.
    pub(super) fn notify_tick(&mut self, ctx: &egui::Context) {
        let n = &self.options.settings.notify;
        let want_tray = n.tray_always || (n.minimize_to_tray && self.notifier.hidden);
        if want_tray && self.notifier.tray.is_none() && self.notifier.main != 0 && !cfg!(test) {
            let (tx, rx) = mpsc::channel();
            let ctx2 = ctx.clone();
            match Tray::start("UltraRushFTP", self.notifier.main, move |e| {
                let _ = tx.send(e);
                ctx2.request_repaint();
            }) {
                Ok(t) => {
                    self.notifier.tray = Some(t);
                    self.notifier.tray_rx = Some(rx);
                }
                Err(e) => {
                    let a = self.active;
                    self.tabs[a].push_log(0, LogKind::Error, t!("notify.tray_failed", error = e));
                    // Not tried again on every frame.
                    self.options.settings.notify.tray_always = false;
                    self.options.settings.notify.minimize_to_tray = false;
                }
            }
        } else if !want_tray && self.notifier.tray.is_some() {
            self.notifier.tray = None;
            self.notifier.tray_rx = None;
        }
        let clicked = self.notifier.tray_rx.as_ref().is_some_and(|rx| rx.try_iter().count() > 0);
        if clicked && self.notifier.hidden {
            self.notifier.hidden = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        if self.notifier.hidden {
            // Keeps the logic running while nothing is drawn.
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        if let Some(t) = &self.notifier.tray {
            let running = self.tabs.iter().filter(|t| t.engine.is_some()).count();
            t.set_tip(&if running > 0 { t!("notify.tip_running", n = running) } else { "UltraRushFTP".to_string() });
        }
        self.take_tab_events();
        self.run_monitors(ctx);
    }

    /// In the window (frame drawn): remembers the main window, hides it when minimized
    /// to the notification area, shows the pop-up messages without tray.
    pub(super) fn notify_ui(&mut self, ctx: &egui::Context) {
        if self.notifier.main == 0 && ctx.input(|i| i.focused) && !cfg!(test) {
            self.notifier.main = urftp_sys::tray::active_window();
        }
        let minimized = ctx.input(|i| i.viewport().minimized) == Some(true);
        if minimized && self.options.settings.notify.minimize_to_tray && !self.notifier.hidden && self.notifier.main != 0 {
            self.notifier.hidden = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        let now = Instant::now();
        self.notifier.toasts.retain(|t| t.until > now);
        if self.notifier.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        egui::Area::new(egui::Id::new("toasts")).anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -40.0]).show(ctx, |ui| {
            for t in &self.notifier.toasts {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_max_width(320.0);
                    ui.label(RichText::new(&t.title).strong());
                    ui.label(&t.text);
                });
            }
        });
    }

    /// Lists the monitored folders that are due, on connections of their own.
    fn run_monitors(&mut self, ctx: &egui::Context) {
        let list: Vec<FolderMonitor> = self.options.settings.notify.monitors.clone();
        self.notifier.monitors.resize_with(list.len(), MonitorState::default);
        let mut found = Vec::new();
        for (i, m) in list.iter().enumerate() {
            let st = &mut self.notifier.monitors[i];
            if let Some(rx) = &st.running
                && let Ok(r) = rx.try_recv()
            {
                st.running = None;
                match r {
                    Ok(names) => {
                        if let Some(known) = &st.known {
                            let fresh = new_names(known, &names);
                            if !fresh.is_empty() {
                                found.push((m.clone(), fresh));
                            }
                        }
                        st.known = Some(names.into_iter().collect());
                    }
                    Err(e) => found.push((m.clone(), vec![format!("⚠ {e}")])),
                }
            }
            let due = st.last.is_none_or(|t| t.elapsed() >= Duration::from_secs(u64::from(m.interval_mins.max(1)) * 60));
            if !m.enabled || !due || st.running.is_some() {
                continue;
            }
            let Some(site) = self.sites.book.find_by_name(&m.site) else { continue };
            let profile = self.site_profile(site);
            let st = &mut self.notifier.monitors[i];
            st.last = Some(Instant::now());
            let (tx, rx) = mpsc::channel();
            st.running = Some(rx);
            let (path, ctx2) = (m.path.clone(), ctx.clone());
            self.rt.spawn(async move {
                let r = async {
                    let mut s = RemoteSession::open(&profile, None).await.map_err(|e| e.to_string())?;
                    let r = s.list(&path).await.map_err(|e| e.to_string());
                    s.quit().await;
                    r.map(|v| v.into_iter().map(|e| e.name).collect::<Vec<_>>())
                }
                .await;
                let _ = tx.send(r);
                ctx2.request_repaint();
            });
        }
        for (m, names) in found {
            let shown: Vec<&str> = names.iter().take(5).map(String::as_str).collect();
            let more = names.len().saturating_sub(5);
            let mut text = t!("notify.new_items", site = m.site, path = m.path, names = shown.join(", "));
            if more > 0 {
                text.push_str(&t!("notify.and_more", n = more));
            }
            self.notify_event(NotifyEvent::NewItems, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_new_names_are_reported() {
        let known: BTreeSet<String> = ["a".to_string(), "b".to_string()].into();
        assert_eq!(new_names(&known, &["b".into(), "c".into(), "a".into(), "d".into()]), ["c", "d"]);
    }
}
