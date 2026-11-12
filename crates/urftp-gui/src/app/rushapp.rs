//! The `RushApp` object of FTPRush scripts, wired to the application: connections, raw
//! commands, queue, sites, statistics. Used by the script console and by `rushmirc.dll`
//! (`RushScript`, `Invoke`).

use std::collections::BTreeMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use urftp_core::itemopts::{ItemOptions, SortKey, SortOrder, TransferFilter};
use urftp_core::pattern::Pattern;
use urftp_i18n::t;
use urftp_script::{Host, HostError, ScriptError, Value, has_flag};

use super::*;

// ====================================================================== statistiques

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Totals {
    /// Date of first use (`YYYY-MM-DD`).
    start_date: String,
    download: u64,
    upload: u64,
    fxp: u64,
    /// Per site of the site manager (by its id).
    sites: BTreeMap<u64, urftp_core::site::SiteStats>,
}

/// Bytes of one frame: direction, count, sites the files come from and go to.
pub(super) type Traffic = (TransferKind, u64, Option<String>, Option<String>);

use crate::sites::StatsView;

/// Bytes transferred: stored totals (`stats.json`), current session, rates.
pub(super) struct Stats {
    totals: Totals,
    session: [u64; 3],
    window: [u64; 2],
    window_start: Instant,
    /// Rates in KiB/s: download, upload.
    bw: [f64; 2],
    dirty: bool,
    last_save: Instant,
}

fn stats_path() -> PathBuf {
    urftp_core::sitestore::data_dir().join("stats.json")
}

fn today() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    let t = urftp_proto::sftp::unix_to_ftptime(secs);
    format!("{:04}-{:02}-{:02}", t.year.unwrap_or(1970), t.month, t.day)
}

/// Days since 1970-01-01 (algorithm by H. Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    Some(days_from_civil(y, m, d))
}

fn mb(bytes: u64) -> Value {
    Value::Str(format!("{:.2}", bytes as f64 / (1024.0 * 1024.0)))
}

impl Stats {
    pub(super) fn load() -> Self {
        let mut totals: Totals = std::fs::read_to_string(stats_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let dirty = totals.start_date.is_empty();
        if dirty {
            totals.start_date = today();
        }
        Self { totals, session: [0; 3], window: [0; 2], window_start: Instant::now(), bw: [0.0; 2], dirty, last_save: Instant::now() }
    }

    /// Counts the traffic of one frame and updates the rates (one-second window); `site_id`
    /// finds the site manager record of a site name.
    pub(super) fn record(&mut self, traffic: &[Traffic], site_id: impl Fn(&str) -> Option<u64>) {
        for (kind, n, src, dst) in traffic {
            let (kind, n) = (*kind, *n);
            if n == 0 {
                continue;
            }
            let (i, total) = match kind {
                TransferKind::Download => (0, &mut self.totals.download),
                TransferKind::Upload => (1, &mut self.totals.upload),
                TransferKind::Fxp => (2, &mut self.totals.fxp),
            };
            *total += n;
            self.session[i] += n;
            if i < 2 {
                self.window[i] += n;
            }
            let mut add = |name: &Option<String>, f: fn(&mut urftp_core::site::SiteStats) -> &mut u64| {
                if let Some(id) = name.as_deref().and_then(&site_id) {
                    let v = f(self.totals.sites.entry(id).or_default());
                    *v = v.saturating_add(n);
                }
            };
            match kind {
                TransferKind::Download => add(src, |s| &mut s.received),
                TransferKind::Upload => add(dst, |s| &mut s.sent),
                TransferKind::Fxp => {
                    add(src, |s| &mut s.fxp_sent);
                    add(dst, |s| &mut s.fxp_received);
                }
            }
            self.dirty = true;
        }
        let elapsed = self.window_start.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            for i in 0..2 {
                self.bw[i] = self.window[i] as f64 / 1024.0 / elapsed;
            }
            self.window = [0; 2];
            self.window_start = Instant::now();
        }
        if self.dirty && self.last_save.elapsed().as_secs() >= 30 {
            self.save();
        }
    }

    pub(super) fn save(&mut self) {
        let path = stats_path();
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.totals)
            && std::fs::write(&path, json).is_ok()
        {
            self.dirty = false;
        }
        self.last_save = Instant::now();
    }

    pub(super) fn bandwidth(&self) -> (f64, f64) {
        (self.bw[0], self.bw[1])
    }

    /// A connection to a site of the site manager succeeded.
    pub(super) fn connected(&mut self, site: u64) {
        let (y, mo, d, h, mi, s) = urftp_sys::win::local_time();
        let st = self.totals.sites.entry(site).or_default();
        st.connections += 1;
        st.last_access = format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}");
        self.dirty = true;
    }

    /// "Default" of the site statistics: back to zero.
    pub(super) fn reset_site(&mut self, site: u64) {
        self.totals.sites.remove(&site);
        self.dirty = true;
    }

    /// "Default" of the statistics of every site: totals back to zero from today.
    pub(super) fn reset_totals(&mut self) {
        let sites = std::mem::take(&mut self.totals.sites);
        self.totals = Totals { start_date: today(), sites, ..Totals::default() };
        self.dirty = true;
    }

    pub(super) fn view(&self) -> StatsView {
        let days = parse_date(&today()).zip(parse_date(&self.totals.start_date)).map_or(0, |(a, b)| a - b);
        StatsView {
            sites: self.totals.sites.clone(),
            since: self.totals.start_date.clone(),
            days: days.max(0),
            total: [self.totals.download, self.totals.upload, self.totals.fxp],
            session: self.session,
        }
    }
}

impl Drop for Stats {
    fn drop(&mut self) {
        if self.dirty {
            self.save();
        }
    }
}

// ====================================================================== execution

impl UltraRushApp {
    /// Runs a script at once, on the interface thread (tests): `InputQuery` answers
    /// nothing and `ShellExecuteCommand` only starts the programs allowed without asking.
    #[cfg(test)]
    pub(super) fn run_script(&mut self, src: &str, ctx: &egui::Context) -> Result<(), ScriptError> {
        let mut host = AppHost { app: self, ctx };
        urftp_script::run(src, &mut host)
    }

    /// Evaluates an expression (`Invoke`) and returns its text.
    pub(super) fn invoke(&mut self, src: &str, ctx: &egui::Context) -> Result<String, ScriptError> {
        let mut host = AppHost { app: self, ctx };
        urftp_script::evaluate(src, &mut host).map(|v| v.to_text())
    }
}

// ====================================================================== liaison mIRC

impl UltraRushApp {
    /// Starts the `TfmRush` window (once), then handles the requests of `rushmirc.dll`.
    pub(super) fn serve_ipc(&mut self, ctx: &egui::Context) {
        if self.ipc_rx.is_none() {
            let class = urftp_sys::window_class();
            if urftp_sys::server::window_exists(&class) {
                // FTPRush (or another UltraRushFTP) already holds the window: leave it alone.
                self.tabs[self.active].push_log(0, LogKind::Error, t!("log.mirc_taken", class = class));
                let (_, rx) = std::sync::mpsc::channel();
                self.ipc_rx = Some(rx);
                return;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let wake = ctx.clone();
            match urftp_sys::server::Server::start(move |req| {
                let _ = tx.send(req);
                wake.request_repaint();
            }) {
                Ok(srv) => {
                    self.ipc = Some(srv);
                    self.tabs[self.active].push_log(0, LogKind::Status, t!("log.mirc_ready"));
                }
                Err(e) => self.tabs[self.active].push_log(0, LogKind::Error, t!("log.mirc_unavailable", error = e)),
            }
            self.ipc_rx = Some(rx);
        }
        let requests: Vec<_> = self.ipc_rx.as_ref().map(|rx| rx.try_iter().collect()).unwrap_or_default();
        for req in requests {
            use urftp_sys::server::Request;
            match req {
                Request::Invoke { expression, reply } => {
                    let answer = self.invoke(&expression, ctx).unwrap_or_else(|e| t!("script.error", error = e));
                    let _ = reply.send(answer);
                }
                Request::Script { source, callback } => {
                    self.start_script(source, scripting::Origin::Mirc { what: "RushScript", callback }, ctx);
                }
                Request::Command { source } => {
                    self.start_script(source, scripting::Origin::Mirc { what: "RushCommand", callback: None }, ctx);
                }
                Request::Show => self.show_main_window(ctx),
            }
        }
    }

    /// Sends mIRC the replies of the `RAW` it asked for; the others are forgotten.
    pub(super) fn forward_raw_replies(&mut self) {
        for t in 0..self.tabs.len() {
            for reply in std::mem::take(&mut self.tabs[t].raw_replies) {
                self.raw.record(&reply);
                let (site, cmd, lines, ok) = reply;
                let pos = self.raw_callbacks.iter().position(|(s, c, _)| s.eq_ignore_ascii_case(&site) && c == &cmd);
                if let Some(i) = pos {
                    let (_, _, cb) = self.raw_callbacks.remove(i);
                    urftp_sys::server::send_raw_callback(cb, ok, &site, &cmd, &lines);
                }
            }
        }
        // Orphan callbacks (connection never established...): not kept forever.
        if self.raw_callbacks.len() > 200 {
            self.raw_callbacks.drain(..100);
        }
    }
}

/// The application as seen by a script.
pub(super) struct AppHost<'a> {
    app: &'a mut UltraRushApp,
    ctx: &'a egui::Context,
}

impl<'a> AppHost<'a> {
    pub(super) fn new(app: &'a mut UltraRushApp, ctx: &'a egui::Context) -> Self {
        Self { app, ctx }
    }
}

/// `RushApp.UI.Bar` and `RushApp.UI.Menu`: the FTPRush help describes no member of them.
fn undocumented(name: &str) -> Option<HostError> {
    let lower = name.to_ascii_lowercase();
    (lower.starts_with("rushapp.ui.bar") || lower.starts_with("rushapp.ui.menu"))
        .then(|| HostError::InvalidArgument { function: name.into(), reason: t!("host.ui_undocumented") })
}

/// `MatchExpr` (regular expression) and `MatchString` (wildcards), case-insensitive like
/// the transfer filters.
fn match_text(name: &str, a: &[Value], regex: bool) -> Result<Value, HostError> {
    want(a, 2, name)?;
    let pattern = if regex {
        Pattern::regex(s(a, 1)).map_err(|e| HostError::InvalidArgument { function: name.into(), reason: e.to_string() })?
    } else {
        Pattern::wildcard(s(a, 1))
    };
    Ok(Value::Bool(pattern.matches(&s(a, 0))))
}

fn s(a: &[Value], i: usize) -> String {
    a.get(i).map(Value::to_text).unwrap_or_default()
}

fn n(a: &[Value], i: usize) -> i64 {
    a.get(i).and_then(Value::as_int).unwrap_or(0)
}

fn b(a: &[Value], i: usize) -> bool {
    a.get(i).and_then(Value::as_bool).unwrap_or(false)
}

fn want(a: &[Value], count: usize, name: &str) -> Result<(), HostError> {
    if a.len() < count { Err(HostError::Arity { name: name.into(), expected: count, got: a.len() }) } else { Ok(()) }
}

impl AppHost<'_> {
    fn log(&mut self, kind: LogKind, msg: impl Into<String>) {
        let t = self.app.active;
        self.app.tabs[t].push_log(0, kind, t!("log.script", message = msg.into()));
    }

    fn names(&self, list: &str) -> Vec<String> {
        let sep = if self.app.splitter.is_empty() { ";" } else { &self.app.splitter };
        list.split(sep).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
    }

    fn site(&self, name: &str) -> Result<Site, HostError> {
        let site = self.app.sites.book.find_by_name(name).cloned().ok_or_else(|| HostError::UnknownSite(name.into()))?;
        if !site.enabled {
            return Err(HostError::DisabledSite(name.into()));
        }
        Ok(site)
    }

    fn bookmark<'s>(site: &'s Site, name: &str) -> Option<&'s urftp_core::site::Bookmark> {
        site.bookmarks.iter().find(|bm| bm.name.eq_ignore_ascii_case(name))
    }

    /// Panels connected to one of the sites (every remote panel when `names` is empty).
    fn panels(&self, names: &[String]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (t, tab) in self.app.tabs.iter().enumerate() {
            for (side, s) in tab.sides.iter().enumerate() {
                if let Side::Remote(r) = s
                    && r.state != RemoteState::Disconnected
                    && (names.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case(&r.profile.name)))
                {
                    out.push((t, side));
                }
            }
        }
        out
    }

    fn send(&self, (t, side): (usize, usize), cmd: BrowserCmd) {
        if let Side::Remote(r) = &self.app.tabs[t].sides[side] {
            r.handle.send(cmd);
        }
    }

    fn panel_dir(&self, (t, side): (usize, usize)) -> String {
        match &self.app.tabs[t].sides[side] {
            Side::Remote(r) => r.dir.clone(),
            Side::Local(_) => "/".into(),
        }
    }

    fn popup(&self, flags: i64) {
        if has_flag(flags, "RS_POPUP") {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    /// Connects a site in a new panel and returns its position.
    fn connect(&mut self, site: &Site, dir: Option<String>, flags: i64) -> (usize, usize) {
        let profile = self.app.site_profile(site).with_login_variant(has_flag(flags, "RS_GHOST"), has_flag(flags, "RS_INVERT"));
        let dir = dir.or_else(|| (!site.remote_dir.is_empty()).then(|| site.remote_dir.clone()));
        self.app.connect_profile(profile, Some(site.id), dir, self.ctx);
        (self.app.active, 1)
    }

    /// Start folder for `Login`/`CWD`: bookmark (`RS_BOOKMARK`) or path.
    fn start_dir(site: &Site, param: &str, flags: i64) -> Result<Option<String>, HostError> {
        if param.is_empty() {
            return Ok(None);
        }
        if has_flag(flags, "RS_BOOKMARK") {
            return match Self::bookmark(site, param) {
                Some(bm) => Ok(Some(bm.remote_at(super::tasks::now_local()))),
                None => Err(HostError::UnknownBookmark { bookmark: param.into(), site: Some(site.name.clone()) }),
            };
        }
        Ok(Some(param.to_string()))
    }

    // ------------------------------------------------------------------ RushApp.FTP.*

    fn login(&mut self, a: &[Value]) -> Result<(), HostError> {
        want(a, 3, "Login")?;
        let (list, param, flags) = (s(a, 0), s(a, 1), n(a, 2));
        let mut sites = Vec::new();
        for name in self.names(&list) {
            let site = match self.site(&name) {
                Ok(site) => site,
                Err(e) => {
                    self.log(LogKind::Error, t!("host.login_error", error = e));
                    continue;
                }
            };
            match Self::start_dir(&site, &param, flags) {
                Ok(dir) => sites.push((site, dir)),
                // RS_CHECKBM: without the requested bookmark, this site is not connected.
                Err(e) if has_flag(flags, "RS_CHECKBM") => self.log(LogKind::Status, t!("host.login_skipped", error = e)),
                Err(e) => {
                    self.log(LogKind::Status, t!("host.login_default_dir", error = e));
                    sites.push((site, None));
                }
            }
        }
        if has_flag(flags, "RS_PAIR") && sites.len() >= 2 {
            // FTP1;FTP2;FTP3 → tabs FTP1|FTP2 and FTP2|FTP3, ready for FXP.
            for w in sites.windows(2) {
                self.app.new_tab();
                let t = self.app.active;
                for (side, (site, dir)) in w.iter().enumerate() {
                    let profile = self.app.site_profile(site).with_login_variant(has_flag(flags, "RS_GHOST"), has_flag(flags, "RS_INVERT"));
                    let dir = dir.clone().or_else(|| (!site.remote_dir.is_empty()).then(|| site.remote_dir.clone()));
                    self.app.open_remote(t, side, profile, Some(site.id), dir, self.ctx);
                }
            }
        } else {
            for (site, dir) in &sites {
                self.connect(site, dir.clone(), flags);
            }
        }
        self.popup(flags);
        Ok(())
    }

    fn logout(&mut self, a: &[Value]) -> Result<(), HostError> {
        want(a, 2, "Logout")?;
        let names = self.names(&s(a, 0));
        for (t, side) in self.panels(&names) {
            self.send((t, side), BrowserCmd::Disconnect);
            self.app.tabs[t].sides[side] = Side::Local(LocalView::new(home_dir()));
        }
        Ok(())
    }

    fn cwd(&mut self, a: &[Value]) -> Result<(), HostError> {
        want(a, 3, "CWD")?;
        let (names, param, flags) = (self.names(&s(a, 0)), s(a, 1), n(a, 2));
        for p in self.panels(&names) {
            let site_name = match &self.app.tabs[p.0].sides[p.1] {
                Side::Remote(r) => r.profile.name.clone(),
                Side::Local(_) => continue,
            };
            let dir = if has_flag(flags, "RS_BOOKMARK") {
                match self.app.sites.book.find_by_name(&site_name).and_then(|site| Self::bookmark(site, &param)) {
                    Some(bm) => bm.remote_at(super::tasks::now_local()),
                    None => {
                        self.log(LogKind::Status, t!("host.cwd_no_bookmark", bookmark = param, site = site_name));
                        continue;
                    }
                }
            } else {
                param.clone()
            };
            self.send(p, BrowserCmd::Cwd(dir));
        }
        self.popup(flags);
        Ok(())
    }

    /// Targeted panels; with `RS_LOGIN`, connects the sites that are not connected yet.
    fn targets(&mut self, names: &[String], flags: i64) -> Vec<(usize, usize)> {
        let mut found = self.panels(names);
        if has_flag(flags, "RS_LOGIN") {
            for name in names {
                if !found
                    .iter()
                    .any(|&(t, sd)| matches!(&self.app.tabs[t].sides[sd], Side::Remote(r) if r.profile.name.eq_ignore_ascii_case(name)))
                {
                    match self.site(name) {
                        Ok(site) => found.push(self.connect(&site, None, flags)),
                        Err(e) => self.log(LogKind::Error, e.to_string()),
                    }
                }
            }
        }
        if has_flag(flags, "RS_ONE") {
            // A single connection per site.
            let mut seen = Vec::<String>::new();
            found.retain(|&(t, sd)| match &self.app.tabs[t].sides[sd] {
                Side::Remote(r) if !seen.iter().any(|x| x.eq_ignore_ascii_case(&r.profile.name)) => {
                    seen.push(r.profile.name.clone());
                    true
                }
                _ => false,
            });
        }
        found
    }

    fn raw(&mut self, a: &[Value]) -> Result<(), HostError> {
        want(a, 3, "RAW")?;
        let (names, cmd, flags) = (self.names(&s(a, 0)), s(a, 1), n(a, 2));
        if cmd.trim().is_empty() {
            return Err(HostError::InvalidArgument { function: "RAW".into(), reason: t!("host.raw_empty") });
        }
        let targets = self.targets(&names, flags);
        if targets.is_empty() {
            self.log(LogKind::Status, t!("host.raw_no_target", command = cmd));
        }
        for p in targets {
            // Script coming from mIRC: the reply will be sent back to it.
            if let (Some(cb), Side::Remote(r)) = (self.app.ipc_callback, &self.app.tabs[p.0].sides[p.1]) {
                self.app.raw_callbacks.push((r.profile.name.clone(), cmd.clone(), cb));
            }
            self.send(p, BrowserCmd::Raw(cmd.clone()));
            if has_flag(flags, "RS_LOGOUT") {
                self.send(p, BrowserCmd::Disconnect);
            }
        }
        self.popup(flags);
        Ok(())
    }

    fn delete(&mut self, a: &[Value]) -> Result<(), HostError> {
        want(a, 4, "Delete")?;
        let (names, path, item, flags) = (self.names(&s(a, 0)), s(a, 1), s(a, 2), n(a, 3));
        if names.is_empty() {
            return Err(HostError::InvalidArgument { function: "Delete".into(), reason: t!("host.delete_empty_list") });
        }
        for p in self.targets(&names, flags) {
            let site_name = match &self.app.tabs[p.0].sides[p.1] {
                Side::Remote(r) => r.profile.name.clone(),
                Side::Local(_) => continue,
            };
            let dir = if has_flag(flags, "RS_BOOKMARK") {
                match self.app.sites.book.find_by_name(&site_name).and_then(|site| Self::bookmark(site, &path)) {
                    Some(bm) => bm.remote_at(super::tasks::now_local()),
                    None => {
                        self.log(LogKind::Error, t!("host.delete_no_bookmark", bookmark = path, site = site_name));
                        continue;
                    }
                }
            } else if path.is_empty() {
                self.panel_dir(p)
            } else {
                path.clone()
            };
            let full = if item.is_empty() { dir } else { rpath::join(&dir, &item) };
            self.send(p, BrowserCmd::Delete(vec![(full, has_flag(flags, "RS_FOLDER"))]));
            if has_flag(flags, "RS_LOGOUT") {
                self.send(p, BrowserCmd::Disconnect);
            }
        }
        self.popup(flags);
        Ok(())
    }

    /// Folder of one end: path (`dir_flag`) or bookmark of the site / local bookmark.
    fn folder(site: Option<&Site>, value: &str, dir_flag: bool, local: bool) -> Result<String, HostError> {
        if dir_flag || value.is_empty() {
            return Ok(value.to_string());
        }
        let bm = site
            .and_then(|st| Self::bookmark(st, value))
            .ok_or_else(|| HostError::UnknownBookmark { bookmark: value.into(), site: site.map(|st| st.name.clone()) })?;
        let now = super::tasks::now_local();
        Ok(if local { bm.local_at(now) } else { bm.remote_at(now) })
    }

    fn transfer(&mut self, a: &[Value]) -> Result<(), HostError> {
        want(a, 8, "Transfer")?;
        let handle = n(a, 0);
        let (src_list, src_folder, src_item) = (s(a, 1), s(a, 2), s(a, 3));
        let (des_list, des_folder, des_item) = (s(a, 4), s(a, 5), s(a, 6));
        let flags = n(a, 7);
        let (up, down, is_file) = (has_flag(flags, "RS_UP"), has_flag(flags, "RS_DOWN"), has_flag(flags, "RS_FILE"));
        let (dir_src, dir_des) = (has_flag(flags, "RS_DIRSRC"), has_flag(flags, "RS_DIRDES"));
        let options = Self::transfer_options(a, flags)?;
        if has_flag(flags, "RS_CHECKLOGIN") {
            self.log(LogKind::Status, t!("host.transfer_unsupported", what = "RS_CHECKLOGIN"));
        }

        let srcs = self.names(&src_list);
        let dsts = self.names(&des_list);
        // (source site, destination site, source folder, source item); None = local.
        let mut pairs: Vec<(Option<Site>, Option<Site>, String, String)> = Vec::new();
        if up {
            for d in &dsts {
                pairs.push((None, Some(self.site(d)?), src_folder.clone(), src_item.clone()));
            }
        } else if down {
            let src = srcs
                .first()
                .ok_or_else(|| HostError::InvalidArgument { function: "Transfer".into(), reason: t!("host.transfer_no_source") })?;
            pairs.push((Some(self.site(src)?), None, src_folder.clone(), src_item.clone()));
        } else if let Some(src) = srcs.first() {
            let src = self.site(src)?;
            for d in &dsts {
                pairs.push((Some(src.clone()), Some(self.site(d)?), src_folder.clone(), src_item.clone()));
            }
        } else {
            // Chain FTP1;FTP2;FTP3: FTP1 → FTP2 and FTP2 → FTP3, same folder and item.
            for w in dsts.windows(2) {
                pairs.push((Some(self.site(&w[0])?), Some(self.site(&w[1])?), des_folder.clone(), des_item.clone()));
            }
        }
        if pairs.is_empty() {
            return Err(HostError::InvalidArgument { function: "Transfer".into(), reason: t!("host.transfer_no_pair") });
        }

        // RS_APPEND (or a window given): every pair in one tab, one after the other;
        // otherwise several pairs run at the same time, each in a new tab.
        let one_tab = has_flag(flags, "RS_APPEND") || handle > 0 || pairs.len() == 1;
        let first_tab = if handle > 0 && (handle as usize) <= self.app.tabs.len() { handle as usize - 1 } else { self.app.active };
        let prio = self.app.options.settings.priority_list();
        let mut added = 0;
        let mut tabs = Vec::new();
        for (src_site, dst_site, sfolder, sitem) in pairs {
            let tab = if one_tab || tabs.is_empty() {
                first_tab
            } else {
                self.app.new_tab();
                self.app.tabs.len() - 1
            };
            if !tabs.contains(&tab) {
                tabs.push(tab);
            }
            let sdir = Self::folder(src_site.as_ref(), &sfolder, dir_src, src_site.is_none())?;
            let ddir = Self::folder(dst_site.as_ref(), &des_folder, dir_des, dst_site.is_none())?;
            let join = |site: &Option<Site>, dir: &str, name: &str| -> String {
                if name.is_empty() {
                    dir.to_string()
                } else if site.is_none() {
                    Path::new(dir).join(name).to_string_lossy().into_owned()
                } else {
                    rpath::join(dir, name)
                }
            };
            let dname = if des_item.is_empty() { sitem.clone() } else { des_item.clone() };
            let src = Endpoint { site: src_site.as_ref().map(|x| x.name.clone()), path: join(&src_site, &sdir, &sitem) };
            let dst = Endpoint { site: dst_site.as_ref().map(|x| x.name.clone()), path: join(&dst_site, &ddir, &dname) };
            let rank = prio.rank(&sitem);
            let mut q = self.app.tabs[tab].queue.lock();
            // RS_NOCIRCLE: each pair waits for the previous one to be all over.
            let sequence = has_flag(flags, "RS_NOCIRCLE").then(|| q.next_sequence());
            let id = q.push(src, dst, !is_file, None, rank);
            q.set_options(id, options.clone(), sequence);
            drop(q);
            added += 1;
        }
        self.log(LogKind::Status, t!("host.transfer_queued", n = added));
        let opts = self.app.engine_options();
        let rt = self.app.rt.clone();
        for tab in tabs {
            let t = &mut self.app.tabs[tab];
            t.logout_when_done |= has_flag(flags, "RS_LOGOUT");
            t.close_when_done |= has_flag(flags, "RS_DELEMPTYTABS");
            let extra = self.app.queue_site_profiles(tab);
            self.app.tabs[tab].start_queue(&rt, self.ctx, opts.clone(), &extra);
        }
        self.popup(flags);
        Ok(())
    }

    /// Options of `Transfer` from its parameters 8 to 19: filters, `CompleteFlag`,
    /// `RepeatCount`, `RetryCount`, `Timeout` (not used, as in FTPRush), `FolderSort`,
    /// `FileSort`, `Depth`, `SubCount`; with `RS_NOSKIP` and `RS_EXPR`.
    fn transfer_options(a: &[Value], flags: i64) -> Result<Option<Arc<ItemOptions>>, HostError> {
        fn invalid(e: impl std::fmt::Display) -> HostError {
            HostError::InvalidArgument { function: "Transfer".into(), reason: e.to_string() }
        }
        let filter = TransferFilter::parse(&s(a, 8), &s(a, 9), &s(a, 10), &s(a, 11), has_flag(flags, "RS_EXPR")).map_err(invalid)?;
        let complete = s(a, 12);
        let complete_flag = if complete.is_empty() { None } else { Some(Pattern::regex(complete).map_err(invalid)?) };
        let sort = |v: i64| SortOrder {
            key: if has_flag(v, "RS_SORTDATE") {
                SortKey::Date
            } else if has_flag(v, "RS_SORTSIZE") {
                SortKey::Size
            } else {
                SortKey::Name
            },
            descending: has_flag(v, "RS_SORTDES"),
        };
        let positive = |i: usize| u32::try_from(n(a, i)).ok().filter(|&v| v > 0);
        let options = ItemOptions {
            filter,
            depth: positive(18),
            sub_count: positive(19).unwrap_or(0),
            folder_sort: sort(n(a, 16)),
            file_sort: sort(n(a, 17)),
            no_skiplist: has_flag(flags, "RS_NOSKIP"),
            retries: positive(14),
            repeat: positive(13).unwrap_or(0),
            complete_flag,
            ..ItemOptions::default()
        };
        Ok((!options.is_default()).then(|| Arc::new(options)))
    }

    fn remove_queue(&mut self, a: &[Value]) -> Result<(), HostError> {
        want(a, 4, "RemoveQueue")?;
        let (source, dest, param, flags) = (s(a, 0), s(a, 1), s(a, 2), n(a, 3));
        let states = ["RS_TRANSFER", "RS_NORMAL", "RS_FAIL", "RS_WAITING"];
        if !states.iter().any(|f| has_flag(flags, f)) {
            return Err(HostError::InvalidArgument { function: "RemoveQueue".into(), reason: t!("host.remove_queue_flags") });
        }
        let pattern = if param.is_empty() {
            None
        } else if has_flag(flags, "RS_EXPR") {
            Some(Pattern::regex(&param).map_err(|e| HostError::InvalidArgument {
                function: "RemoveQueue".into(),
                reason: t!("host.remove_queue_regex", error = e),
            })?)
        } else {
            Some(Pattern::wildcard(&param))
        };
        let endpoint_ok = |want: &str, e: &Endpoint| -> bool {
            want.is_empty()
                || (want.eq_ignore_ascii_case("local") && e.is_local())
                || e.site.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(want))
        };
        let tabs: Vec<usize> = if has_flag(flags, "RS_CURRENT") { vec![self.app.active] } else { (0..self.app.tabs.len()).collect() };
        let (mut removed, mut stopped) = (0, 0);
        for t in tabs {
            let matches = |it: &QueueItem| -> bool {
                let state_ok = match &it.state {
                    ItemState::Running => has_flag(flags, "RS_TRANSFER"),
                    ItemState::Failed(_) => has_flag(flags, "RS_FAIL"),
                    ItemState::Waiting => has_flag(flags, "RS_WAITING") || has_flag(flags, "RS_NORMAL"),
                    _ => has_flag(flags, "RS_NORMAL"),
                };
                let target = if has_flag(flags, "RS_FULLPATH") {
                    it.dst.path.clone()
                } else {
                    it.dst.path.rsplit(['/', '\\']).next().unwrap_or_default().to_string()
                };
                state_ok
                    && endpoint_ok(&source, &it.src)
                    && endpoint_ok(&dest, &it.dst)
                    && pattern.as_ref().is_none_or(|p| p.matches(&target))
            };
            let running: Vec<ItemId> = self.app.tabs[t]
                .queue
                .lock()
                .items()
                .iter()
                .filter(|i| i.state == ItemState::Running && matches(i))
                .map(|i| i.id)
                .collect();
            for id in running {
                self.app.tabs[t].stop_item(id);
                stopped += 1;
            }
            removed += self.app.tabs[t].queue.lock().remove_where(&matches);
        }
        self.log(LogKind::Status, t!("host.remove_queue_done", removed = removed, stopped = stopped));
        self.popup(flags);
        Ok(())
    }

    fn enable_site(&mut self, a: &[Value], enable: bool) -> Result<(), HostError> {
        want(a, 2, if enable { "EnableSite" } else { "DisableSite" })?;
        let (name, this_session) = (s(a, 0), b(a, 1));
        let id = self.app.sites.book.find_by_name(&name).map(|x| x.id).ok_or_else(|| HostError::UnknownSite(name.clone()))?;
        if let Some(site) = self.app.sites.book.find_mut(id) {
            site.enabled = enable;
        }
        if !this_session {
            self.app.sites.save();
        }
        Ok(())
    }

    /// Closes the tabs without connection and with an empty queue (at least one tab stays).
    fn delete_empty_tabs(&mut self) {
        let mut i = 0;
        while i < self.app.tabs.len() && self.app.tabs.len() > 1 {
            let t = &self.app.tabs[i];
            let empty = t.sides.iter().all(|s| matches!(s, Side::Local(_))) && t.queue.lock().items().is_empty();
            if empty {
                self.app.tabs.remove(i);
            } else {
                i += 1;
            }
        }
        self.app.active = self.app.active.min(self.app.tabs.len() - 1);
    }
}

impl Host for AppHost<'_> {
    fn call(&mut self, name: &str, a: &[Value]) -> Result<Value, HostError> {
        match name.to_ascii_lowercase().as_str() {
            "rushapp.ftp.login" => self.login(a)?,
            "rushapp.ftp.logout" => self.logout(a)?,
            "rushapp.ftp.cwd" => self.cwd(a)?,
            "rushapp.ftp.raw" => self.raw(a)?,
            "rushapp.ftp.delete" => self.delete(a)?,
            "rushapp.ftp.transfer" => self.transfer(a)?,
            "rushapp.ftp.removequeue" => self.remove_queue(a)?,
            "rushapp.ftp.enablesite" => self.enable_site(a, true)?,
            "rushapp.ftp.disablesite" => self.enable_site(a, false)?,
            "rushapp.deleteemptytabs" | "rushapp.deleteemptycontainers" => self.delete_empty_tabs(),
            "rushapp.ui.selection.prepare" => self.app.prepare_selection(),
            "matchexpr" => return match_text(name, a, true),
            "matchstring" => return match_text(name, a, false),
            "shellexecutecommand" => {
                want(a, 2, name)?;
                return Ok(self.app.shell_now(&s(a, 0), &s(a, 1), &s(a, 2)));
            }
            "showmessage" => {
                let msg = s(a, 0);
                self.log(LogKind::Status, t!("host.message", message = msg));
            }
            "inputquery" => {
                self.log(LogKind::Status, t!("host.input_query"));
                return Ok(Value::Nil);
            }
            _ => return Err(undocumented(name).unwrap_or_else(|| HostError::UnknownFunction(name.into()))),
        }
        Ok(Value::Nil)
    }

    fn get(&mut self, name: &str) -> Result<Value, HostError> {
        let st = &self.app.stats;
        let remote = || self.app.tabs.iter().flat_map(|t| t.sides.iter());
        Ok(match name.to_ascii_lowercase().as_str() {
            "rushapp.version" => Value::Str(format!("UltraRushFTP {}", env!("CARGO_PKG_VERSION"))),
            "rushapp.language" => Value::Str(urftp_i18n::language().native_name().into()),
            "rushapp.ftp.splitter" => Value::Str(self.app.splitter.clone()),
            "rushapp.totaldownloadsize" => mb(st.totals.download),
            "rushapp.totaluploadsize" => mb(st.totals.upload),
            "rushapp.totalfxpsize" => mb(st.totals.fxp),
            "rushapp.curdownloadsize" => mb(st.session[0]),
            "rushapp.curuploadsize" => mb(st.session[1]),
            "rushapp.curfxpsize" => mb(st.session[2]),
            "rushapp.startdate" => Value::Str(st.totals.start_date.clone()),
            "rushapp.daycount" => {
                let days = parse_date(&today()).zip(parse_date(&st.totals.start_date)).map_or(0, |(a, b)| a - b);
                Value::Int(days.max(0))
            }
            "rushapp.poolinfo" => {
                let running: usize =
                    self.app.tabs.iter().map(|t| t.queue.lock().items().iter().filter(|i| i.state == ItemState::Running).count()).sum();
                let queues = self.app.tabs.iter().filter(|t| t.engine.is_some()).count();
                Value::Str(t!("host.pool_info", running = running, queues = queues))
            }
            "rushapp.containers.containercount" => Value::Int(self.app.tabs.len() as i64),
            "rushapp.containers.connectedsitecount" => {
                Value::Int(remote().filter(|s| matches!(s, Side::Remote(r) if r.state == RemoteState::Connected)).count() as i64)
            }
            "rushapp.containers.transfersitecount" => Value::Int(self.app.tabs.iter().filter(|t| t.engine.is_some()).count() as i64),
            "rushapp.downloadbw" => Value::Int(st.bandwidth().0.round() as i64),
            "rushapp.uploadbw" => Value::Int(st.bandwidth().1.round() as i64),
            "rushapp.ui.selection.count" => Value::Int(self.app.scripts.selection.as_ref().map_or(0, |s| s.items.len() as i64)),
            "rushapp.ui.selection.items" => Value::Array(self.app.scripts.selection.as_ref().map(|s| s.items.clone()).unwrap_or_default()),
            "rushapp.ui.selection.isremoteside" => Value::Bool(self.app.scripts.selection.as_ref().is_some_and(|s| s.remote)),
            "rushapp.ui.selection.isftpconnected" => Value::Bool(self.app.scripts.selection.as_ref().is_some_and(|s| s.connected)),
            "rushapp.ui.selection.ftpname" => {
                Value::Str(self.app.scripts.selection.as_ref().map(|s| s.ftp_name.clone()).unwrap_or_default())
            }
            "rushapp.ui.onswitchlayout" => self.app.scripts.on_switch_layout.clone().map_or(Value::Nil, Value::Event),
            _ => return Err(undocumented(name).unwrap_or_else(|| HostError::UnknownProperty(name.into()))),
        })
    }

    fn set(&mut self, name: &str, value: Value) -> Result<(), HostError> {
        match name.to_ascii_lowercase().as_str() {
            "rushapp.ftp.splitter" => {
                let v = value.to_text();
                if v.is_empty() {
                    return Err(HostError::InvalidArgument { function: "RushApp.FTP.Splitter".into(), reason: t!("host.splitter_empty") });
                }
                self.app.splitter = v;
                Ok(())
            }
            "rushapp.ui.onswitchlayout" => match value {
                Value::Event(h) => {
                    self.app.scripts.on_switch_layout = Some(h);
                    Ok(())
                }
                Value::Nil => {
                    self.app.scripts.on_switch_layout = None;
                    Ok(())
                }
                other => {
                    Err(HostError::InvalidArgument { function: name.into(), reason: t!("host.event_expected", found = other.type_name()) })
                }
            },
            _ => Err(undocumented(name).unwrap_or_else(|| HostError::ReadOnlyProperty(name.into()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(parse_date("2026-11-05").unwrap() - parse_date("2026-11-01").unwrap(), 4);
        assert_eq!(parse_date("2025-01-01").unwrap() - parse_date("2024-01-01").unwrap(), 366);
        assert!(parse_date("not a date").is_none());
        assert_eq!(today().len(), 10);
    }

    #[test]
    fn stats_accumulate_by_direction() {
        let mut st = Stats {
            totals: Totals::default(),
            session: [0; 3],
            window: [0; 2],
            window_start: Instant::now(),
            bw: [0.0; 2],
            dirty: false,
            last_save: Instant::now(),
        };
        let site = |n: &str| Some(n.to_string());
        let ids = |n: &str| match n {
            "A" => Some(1),
            "B" => Some(2),
            _ => None,
        };
        st.record(
            &[
                (TransferKind::Download, 1024 * 1024, site("A"), None),
                (TransferKind::Upload, 10, None, site("A")),
                (TransferKind::Fxp, 5, site("A"), site("B")),
                (TransferKind::Upload, 7, None, site("quick")),
            ],
            ids,
        );
        assert_eq!(st.session, [1024 * 1024, 17, 5]);
        let a = &st.totals.sites[&1];
        assert_eq!((a.received, a.sent, a.fxp_sent, a.fxp_received), (1024 * 1024, 10, 5, 0));
        assert_eq!((a.total_sent(), st.totals.sites[&2].total_received()), (15, 5));
        assert_eq!(st.totals.sites.len(), 2, "a site out of the site manager is not kept");
        assert_eq!(mb(st.totals.download), Value::Str("1.00".into()));
        st.connected(1);
        assert_eq!(st.totals.sites[&1].connections, 1);
        st.reset_site(2);
        assert!(!st.totals.sites.contains_key(&2));
        st.reset_totals();
        assert_eq!((st.totals.upload, st.totals.start_date.len(), st.totals.sites.len()), (0, 10, 1));
        st.dirty = false; // no disk write during the test
    }
}
