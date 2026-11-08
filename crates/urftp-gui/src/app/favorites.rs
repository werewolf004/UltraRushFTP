//! Favourites of the toolbar (recent sites and the whole site manager), quick connection
//! saved as a site, "Manual download" of addresses (with incremented lists) and the
//! clipboard watch.

use urftp_core::urls::{self, FtpUrl, UrlScheme};

use super::*;

/// Sites remembered in the "Recent" list.
const RECENT: usize = 10;
/// Most items an incremented list may give.
const MAX_EXPANDED: usize = 1000;

/// "Manual download" window.
#[derive(Default)]
pub(super) struct ManualDownload {
    pub(super) open: bool,
    pub(super) text: String,
    pub(super) target: String,
}

/// Addresses of a text, ranges expanded, and the lines that are not addresses.
fn addresses(text: &str) -> (Vec<FtpUrl>, Vec<String>) {
    let (mut ok, mut bad) = (Vec::new(), Vec::new());
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        for u in urls::expand_increments(line, MAX_EXPANDED) {
            match urls::parse_url(&u) {
                Some(p) if p.path != "/" => ok.push(p),
                _ => bad.push(u),
            }
        }
    }
    (ok, bad)
}

/// Site record of an address (not added to the site manager).
fn site_of(u: &FtpUrl, transfers: u32) -> Site {
    let user = if u.user.is_empty() { "anonymous".to_string() } else { u.user.clone() };
    Site {
        name: format!("{user}@{}:{}", u.host, u.port),
        host: u.host.clone(),
        port: u.port,
        protocol: match u.scheme {
            UrlScheme::Ftp => urftp_core::site::SiteProtocol::Ftp,
            UrlScheme::FtpsExplicit => urftp_core::site::SiteProtocol::FtpsExplicit,
            UrlScheme::FtpsImplicit => urftp_core::site::SiteProtocol::FtpsImplicit,
            UrlScheme::Sftp => urftp_core::site::SiteProtocol::Sftp,
        },
        anonymous: u.user.is_empty() && u.password.is_empty(),
        user,
        password: u.password.clone(),
        max_transfers: transfers,
        ..Site::default()
    }
}

impl UltraRushApp {
    /// Puts a site of the site manager first in the recent list.
    pub(super) fn note_recent(&mut self, id: u64, ctx: &egui::Context) {
        let r = &mut self.options.settings.recent_sites;
        r.retain(|&x| x != id);
        r.insert(0, id);
        r.truncate(RECENT);
        self.options.touch(ctx);
    }

    /// The quick connection fields as a site record.
    pub(super) fn quick_site(&self) -> Option<Site> {
        let q = &self.quick;
        let host = q.host.trim();
        if host.is_empty() {
            return None;
        }
        let proto = match q.proto {
            QuickProto::Ftp => UrlScheme::Ftp,
            QuickProto::FtpsExplicit => UrlScheme::FtpsExplicit,
            QuickProto::FtpsImplicit => UrlScheme::FtpsImplicit,
            QuickProto::Sftp => UrlScheme::Sftp,
        };
        let port = q.port.trim().parse().unwrap_or(q.proto.default_port());
        let url = FtpUrl {
            scheme: proto,
            user: q.user.trim().to_string(),
            password: q.pass.clone(),
            host: host.to_string(),
            port,
            path: "/".into(),
        };
        Some(site_of(&url, self.options.settings.transfer.quick_connect_transfers))
    }

    /// "Add to the site manager": the quick connection becomes a site record.
    fn add_quick_site(&mut self) {
        let Some(site) = self.quick_site() else { return };
        let name = site.name.clone();
        self.sites.book.add_site(&[], site);
        self.sites.save();
        let a = self.active;
        self.tabs[a].push_log(0, LogKind::Status, t!("fav.site_added", site = name));
    }

    /// Favourites button of the toolbar: recent sites, then every site.
    pub(super) fn favorites_menu(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let side = self.tabs[self.active].focus;
        let recent: Vec<(u64, String)> =
            self.options.settings.recent_sites.iter().filter_map(|id| self.sites.book.find(*id).map(|s| (*id, s.name.clone()))).collect();
        let entries = self.site_entries();
        let r = ui.menu_button(t!("fav.button"), |ui| {
            ui.label(RichText::new(t!("fav.recent")).strong());
            if recent.is_empty() {
                ui.label(RichText::new(t!("fav.no_recent")).italics());
            }
            for (id, name) in &recent {
                if ui.button(format!("🕘 {name}")).clicked() {
                    let variant = sites::LoginVariant::from_modifiers(ui.input(|i| i.modifiers));
                    actions.push(Action::ConnectSite(side, *id, variant));
                    ui.close();
                }
            }
            ui.separator();
            connection_menu(ui, &entries, side, actions);
            ui.separator();
            if ui.button(t!("fav.manual")).clicked() {
                actions.push(Action::ManualDownload);
                ui.close();
            }
        });
        r.response.on_hover_text(t!("fav.hint"));
    }

    /// "➕" next to Connect of the quick bar.
    pub(super) fn add_quick_button(&mut self, ui: &mut egui::Ui) {
        let enabled = !self.quick.host.trim().is_empty();
        let r = ui.add_enabled(enabled, egui::Button::new("➕"));
        theme::name(&r, t!("fav.add_quick"));
        if r.on_hover_text(t!("fav.add_quick")).on_disabled_hover_text(t!("fav.add_quick")).clicked() {
            self.add_quick_site();
        }
    }

    /// Opens "Manual download" with addresses (clipboard) or empty.
    pub(super) fn open_manual(&mut self, text: Option<String>) {
        let a = self.active;
        let local = self.tabs[a]
            .sides
            .iter()
            .find_map(|s| if let Side::Local(l) = s { Some(l.path.clone()) } else { None })
            .unwrap_or_else(home_dir);
        let m = &mut self.manual;
        if !m.open || m.target.trim().is_empty() {
            m.target = local.to_string_lossy().into_owned();
        }
        if let Some(t) = text {
            if !m.text.trim().is_empty() && !m.text.ends_with('\n') {
                m.text.push('\n');
            }
            m.text.push_str(&t);
        }
        m.open = true;
    }

    /// Queues the addresses of `text` into `target` and starts the queue. Returns how
    /// many items.
    pub(super) fn queue_addresses(&mut self, text: &str, target: &Path, ctx: &egui::Context) -> usize {
        let (found, _) = addresses(text);
        if found.is_empty() {
            return 0;
        }
        let transfers = self.options.settings.transfer.quick_connect_transfers;
        let a = self.active;
        for u in &found {
            let site = site_of(u, transfers);
            let profile = self.site_profile(&site);
            let tab = &mut self.tabs[a];
            if !tab.adhoc.iter().any(|p| p.name == profile.name) {
                tab.adhoc.push(profile);
            }
            let dst = target.join(u.name());
            tab.queue.lock().push(
                Endpoint::remote(site.name.clone(), u.path.trim_end_matches('/')),
                Endpoint::local(dst.to_string_lossy()),
                u.is_folder(),
                None,
                None,
            );
        }
        let n = found.len();
        self.tabs[a].push_transfer(0, LogKind::Status, t!("log.items_queued", n = n));
        let (rt, opts, extra) = (self.rt.clone(), self.engine_options(), self.queue_site_profiles(a));
        self.tabs[a].start_queue(&rt, ctx, opts, &extra);
        n
    }

    pub(super) fn manual_window(&mut self, ctx: &egui::Context) {
        if !self.manual.open {
            return;
        }
        let mut open = true;
        let mut queue = false;
        let m = &mut self.manual;
        egui::Window::new(t!("fav.manual_title")).id(egui::Id::new("manual-download")).default_size([600.0, 360.0]).open(&mut open).show(
            ctx,
            |ui| {
                let h = ui.label(t!("fav.manual_urls"));
                let r = ui.add(
                    egui::TextEdit::multiline(&mut m.text).font(egui::TextStyle::Monospace).desired_rows(6).desired_width(f32::INFINITY),
                );
                r.labelled_by(h.id);
                ui.label(RichText::new(t!("fav.manual_hint")).weak());
                ui.horizontal(|ui| {
                    let l = ui.label(t!("fav.manual_target"));
                    ui.add(theme::single_line(&mut m.target).desired_width(360.0)).labelled_by(l.id);
                    if ui.button(t!("common.browse")).clicked()
                        && let Some(d) = rfd::FileDialog::new().pick_folder()
                    {
                        m.target = d.to_string_lossy().into_owned();
                    }
                });
                let (found, bad) = addresses(&m.text);
                ui.label(t!("fav.manual_count", n = found.len()));
                for b in bad.iter().take(5) {
                    ui.colored_label(theme::colors(ui).error, t!("fav.manual_bad", line = b));
                }
                let ok = !found.is_empty() && !m.target.trim().is_empty();
                if ui.add_enabled(ok, egui::Button::new(t!("fav.manual_queue"))).clicked() {
                    queue = true;
                }
            },
        );
        if !open {
            self.manual.open = false;
        }
        if queue {
            let (text, target) = (self.manual.text.clone(), PathBuf::from(self.manual.target.trim()));
            if self.queue_addresses(&text, &target, ctx) > 0 {
                self.manual.text.clear();
                self.manual.open = false;
            }
        }
    }

    /// Clipboard watch: new FTP addresses open "Manual download", or are queued at once.
    pub(super) fn watch_clipboard(&mut self, ctx: &egui::Context) {
        let ui = &self.options.settings.interface;
        if !ui.watch_clipboard {
            return;
        }
        let auto = ui.clipboard_auto;
        ctx.request_repaint_after(Duration::from_secs(1));
        let seq = urftp_sys::win::clipboard_sequence();
        if self.clipboard_seq == Some(seq) {
            return;
        }
        // The first look only notes the current content.
        let first = self.clipboard_seq.is_none();
        self.clipboard_seq = Some(seq);
        if first {
            return;
        }
        let Some(text) = urftp_sys::win::clipboard_text() else { return };
        let found = urls::find_urls(&text);
        if found.is_empty() {
            return;
        }
        let list = found.join("\n");
        if auto {
            let target = self.tabs[self.active]
                .sides
                .iter()
                .find_map(|s| if let Side::Local(l) = s { Some(l.path.clone()) } else { None })
                .unwrap_or_else(home_dir);
            self.queue_addresses(&list, &target, ctx);
        } else {
            self.open_manual(Some(list));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_expanded_and_checked() {
        let (ok, bad) = addresses("ftp://h/pub/a[1-3].rar\nnot an address\nftp://h/\n");
        assert_eq!(ok.iter().map(|u| u.path.as_str()).collect::<Vec<_>>(), ["/pub/a1.rar", "/pub/a2.rar", "/pub/a3.rar"]);
        assert_eq!(bad, ["not an address", "ftp://h/"], "the root alone is not something to download");
        let s = site_of(&ok[0], 2);
        assert_eq!((s.name.as_str(), s.anonymous, s.max_transfers), ("anonymous@h:21", true, 2));
    }
}
