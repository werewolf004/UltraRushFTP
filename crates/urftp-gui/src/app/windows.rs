//! Window list (one row per tab, with its connections and what its queue is doing),
//! duplicating a tab with its connections, and showing or hiding the optional panes.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use urftp_i18n::t;
use urftp_proto::session::SiteProfile;

use super::{Action, Container, LocalView, Pane, RemoteState, Side, UltraRushApp};
use crate::theme;

/// One row of the window list.
pub(super) struct WindowRow {
    pub(super) title: String,
    /// What each panel shows: local folder, or site and remote folder.
    pub(super) panels: [String; 2],
    /// Connections: "2 connected", "connecting"…
    pub(super) ftp: String,
    /// What the queue does: transferring, paused, waiting items, idle.
    pub(super) action: String,
}

fn panel_text(side: &Side) -> String {
    match side {
        Side::Local(l) => l.path.display().to_string(),
        Side::Remote(r) => format!("{}:{}", r.profile.name, r.dir),
    }
}

impl Container {
    pub(super) fn window_row(&self, pos: usize) -> WindowRow {
        let remotes: Vec<&RemoteState> = self
            .sides
            .iter()
            .chain(self.parked.iter().flatten())
            .filter_map(|s| if let Side::Remote(r) = s { Some(&r.state) } else { None })
            .collect();
        let connected = remotes.iter().filter(|s| ***s == RemoteState::Connected).count();
        let ftp = if remotes.is_empty() {
            t!("windows.ftp_none")
        } else if connected == remotes.len() {
            t!("windows.ftp_connected", n = connected)
        } else {
            t!("windows.ftp_partly", n = connected, total = remotes.len())
        };
        let st = self.queue.lock().stats();
        let action = if self.engine.is_some() && self.paused() {
            t!("windows.action_paused", left = st.waiting + st.running)
        } else if self.engine.is_some() {
            t!("windows.action_running", running = st.running, left = st.waiting)
        } else if st.waiting > 0 {
            t!("windows.action_waiting", n = st.waiting)
        } else {
            t!("windows.action_idle")
        };
        WindowRow { title: self.title(pos), panels: [panel_text(&self.sides[0]), panel_text(&self.sides[1])], ftp, action }
    }
}

/// Content of the window list (a pane of the tab).
pub(super) fn windows_pane(ui: &mut egui::Ui, rows: &[WindowRow], active: usize, columns: (bool, bool), actions: &mut Vec<Action>) {
    let (show_ftp, show_action) = columns;
    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
        let n = 4 + usize::from(show_ftp) + usize::from(show_action);
        egui::Grid::new("windows-grid").striped(true).num_columns(n).spacing([14.0, 6.0]).show(ui, |ui| {
            ui.strong(t!("windows.col_window"));
            ui.strong(t!("windows.col_left"));
            ui.strong(t!("windows.col_right"));
            if show_ftp {
                ui.strong(t!("windows.col_ftp"));
            }
            if show_action {
                ui.strong(t!("windows.col_action"));
            }
            ui.label("");
            ui.end_row();
            for (i, row) in rows.iter().enumerate() {
                let r = ui.selectable_label(i == active, format!("{}. {}", i + 1, row.title));
                theme::name(&r, t!("windows.show_named", title = &row.title));
                if r.clicked() {
                    actions.push(Action::ActivateTab(i));
                }
                ui.label(RichText::new(&row.panels[0]).weak());
                ui.label(RichText::new(&row.panels[1]).weak());
                if show_ftp {
                    ui.label(&row.ftp);
                }
                if show_action {
                    ui.label(&row.action);
                }
                if rows.len() > 1 {
                    let close = theme::icon_button(ui, "✕", t!("tabs.close"));
                    theme::name(&close, t!("tabs.close_named", title = &row.title));
                    if close.clicked() {
                        actions.push(Action::CloseTab(i));
                    }
                } else {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    });
}

/// What a duplicated panel shows.
enum Copy {
    Local(PathBuf),
    Remote(Box<SiteProfile>, Option<u64>, String),
}

impl UltraRushApp {
    /// "New window of current": a new tab with the layout, the local folders and the
    /// connections (same sites, same folders) of the tab shown. The queue is not copied.
    pub(super) fn duplicate_tab(&mut self, ctx: &egui::Context) {
        let src = &self.tabs[self.active];
        let dock = src.dock.clone();
        let focus = src.focus;
        let copies = [0, 1].map(|side| match &src.sides[side] {
            Side::Local(l) => Copy::Local(l.path.clone()),
            Side::Remote(r) => Copy::Remote(Box::new(r.profile.clone()), r.site_id, r.dir.clone()),
        });
        self.new_tab();
        let t = self.tabs.len() - 1;
        self.tabs[t].dock = dock;
        self.tabs[t].focus = focus;
        for (side, copy) in copies.into_iter().enumerate() {
            match copy {
                Copy::Local(path) => self.tabs[t].sides[side] = Side::Local(LocalView::new(path)),
                Copy::Remote(profile, site_id, dir) => self.open_remote(t, side, *profile, site_id, Some(dir), ctx),
            }
        }
    }

    /// Shows a pane in the tab shown (next to the queue), or hides it.
    pub(super) fn toggle_pane(&mut self, pane: Pane) {
        let dock = &mut self.tabs[self.active].dock;
        if let Some(found) = dock.find_tab(&pane) {
            dock.remove_tab(found);
            return;
        }
        if let Some(queue) = dock.find_tab(&Pane::Queue) {
            dock.set_focused_node_and_surface(egui_dock::NodePath { surface: queue.surface, node: queue.node });
        }
        dock.push_to_focused_leaf(pane);
    }

    /// The pane is in the layout of the tab shown.
    pub(super) fn pane_shown(&self, pane: Pane) -> bool {
        self.tabs[self.active].dock.find_tab(&pane).is_some()
    }
}
