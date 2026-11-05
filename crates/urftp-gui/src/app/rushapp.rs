//! L'objet `RushApp` des scripts FTPRush, branché sur l'application : connexions, commandes
//! brutes, file d'attente, sites, statistiques. Utilisé par la console de script et, plus
//! tard, par `rushmirc.dll` (`RushScript`, `Invoke`).

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use urftp_core::pattern::Pattern;
use urftp_script::{Host, Value, has_flag};

use super::*;

// ====================================================================== statistiques

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Totals {
    /// Date de la première utilisation (`AAAA-MM-JJ`).
    start_date: String,
    download: u64,
    upload: u64,
    fxp: u64,
}

/// Octets transférés : totaux enregistrés (`stats.json`), session en cours, débits.
pub(super) struct Stats {
    totals: Totals,
    session: [u64; 3],
    window: [u64; 2],
    window_start: Instant,
    /// Débits en Kio/s : téléchargement, envoi.
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

/// Jours depuis le 1970-01-01 (algorithme de H. Hinnant).
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

    /// Compte le trafic d'une image et met à jour les débits (fenêtre d'une seconde).
    pub(super) fn record(&mut self, traffic: &[(TransferKind, u64)]) {
        for &(kind, n) in traffic {
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

    fn save(&mut self) {
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
}

impl Drop for Stats {
    fn drop(&mut self) {
        if self.dirty {
            self.save();
        }
    }
}

// ====================================================================== exécution

impl UltraRushApp {
    /// Exécute un script (`RushScript`) ; les erreurs portent leur numéro de ligne.
    pub(super) fn run_script(&mut self, src: &str, ctx: &egui::Context) -> Result<(), String> {
        let mut host = AppHost { app: self, ctx };
        urftp_script::run(src, &mut host).map_err(|e| e.to_string())
    }

    /// Évalue une expression (`Invoke`) et renvoie son texte.
    pub(super) fn invoke(&mut self, src: &str, ctx: &egui::Context) -> Result<String, String> {
        let mut host = AppHost { app: self, ctx };
        urftp_script::evaluate(src, &mut host).map(|v| v.to_text()).map_err(|e| e.to_string())
    }
}

struct AppHost<'a> {
    app: &'a mut UltraRushApp,
    ctx: &'a egui::Context,
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

fn want(a: &[Value], count: usize, name: &str) -> Result<(), String> {
    if a.len() < count { Err(format!("{name} attend {count} argument(s), {} reçu(s)", a.len())) } else { Ok(()) }
}

impl AppHost<'_> {
    fn log(&mut self, kind: LogKind, msg: impl Into<String>) {
        let t = self.app.active;
        self.app.tabs[t].push_log(0, kind, format!("[script] {}", msg.into()));
    }

    fn names(&self, list: &str) -> Vec<String> {
        let sep = if self.app.splitter.is_empty() { ";" } else { &self.app.splitter };
        list.split(sep).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
    }

    fn site(&self, name: &str) -> Result<Site, String> {
        let site = self.app.sites.book.find_by_name(name).cloned().ok_or_else(|| format!("site inconnu : {name}"))?;
        if !site.enabled {
            return Err(format!("site désactivé : {name}"));
        }
        Ok(site)
    }

    fn bookmark<'s>(site: &'s Site, name: &str) -> Option<&'s urftp_core::site::Bookmark> {
        site.bookmarks.iter().find(|bm| bm.name.eq_ignore_ascii_case(name))
    }

    /// Panneaux connectés à l'un des sites (tous les panneaux distants si `names` est vide).
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

    /// Connecte un site dans un nouveau panneau et renvoie sa position.
    fn connect(&mut self, site: &Site, dir: Option<String>, flags: i64) -> (usize, usize) {
        let mut profile = sites::to_profile(site, &self.app.options.settings);
        if has_flag(flags, "RS_GHOST") && !profile.user.starts_with('!') {
            profile.user = format!("!{}", profile.user);
        }
        let dir = dir.or_else(|| (!site.remote_dir.is_empty()).then(|| site.remote_dir.clone()));
        self.app.connect_profile(profile, Some(site.id), dir, self.ctx);
        (self.app.active, 1)
    }

    /// Dossier de départ pour `Login`/`CWD` : signet (`RS_BOOKMARK`) ou chemin.
    fn start_dir(site: &Site, param: &str, flags: i64) -> Result<Option<String>, String> {
        if param.is_empty() {
            return Ok(None);
        }
        if has_flag(flags, "RS_BOOKMARK") {
            return match Self::bookmark(site, param) {
                Some(bm) => Ok(Some(bm.remote_dir.clone())),
                None => Err(format!("signet « {param} » absent du site {}", site.name)),
            };
        }
        Ok(Some(param.to_string()))
    }

    // ------------------------------------------------------------------ RushApp.FTP.*

    fn login(&mut self, a: &[Value]) -> Result<(), String> {
        want(a, 3, "Login")?;
        let (list, param, flags) = (s(a, 0), s(a, 1), n(a, 2));
        let mut sites = Vec::new();
        for name in self.names(&list) {
            let site = match self.site(&name) {
                Ok(site) => site,
                Err(e) => {
                    self.log(LogKind::Error, format!("Login : {e}"));
                    continue;
                }
            };
            match Self::start_dir(&site, &param, flags) {
                Ok(dir) => sites.push((site, dir)),
                // RS_CHECKBM : sans le signet demandé, ce site n'est pas connecté.
                Err(e) if has_flag(flags, "RS_CHECKBM") => self.log(LogKind::Status, format!("Login : {e}, site ignoré")),
                Err(e) => {
                    self.log(LogKind::Status, format!("Login : {e}, dossier par défaut"));
                    sites.push((site, None));
                }
            }
        }
        if has_flag(flags, "RS_PAIR") && sites.len() >= 2 {
            // FTP1;FTP2;FTP3 → onglets FTP1|FTP2 et FTP2|FTP3, prêts pour le FXP.
            for w in sites.windows(2) {
                self.app.new_tab();
                let t = self.app.active;
                for (side, (site, dir)) in w.iter().enumerate() {
                    let profile = sites::to_profile(site, &self.app.options.settings);
                    let dir = dir.clone().or_else(|| (!site.remote_dir.is_empty()).then(|| site.remote_dir.clone()));
                    self.app.open_remote(t, side, profile, Some(site.id), dir, self.ctx);
                }
            }
        } else {
            for (site, dir) in &sites {
                self.connect(site, dir.clone(), flags);
            }
        }
        if has_flag(flags, "RS_INVERT") {
            self.log(LogKind::Status, "Login : RS_INVERT n'est pas pris en charge, ignoré");
        }
        self.popup(flags);
        Ok(())
    }

    fn logout(&mut self, a: &[Value]) -> Result<(), String> {
        want(a, 2, "Logout")?;
        let names = self.names(&s(a, 0));
        for (t, side) in self.panels(&names) {
            self.send((t, side), BrowserCmd::Disconnect);
            self.app.tabs[t].sides[side] = Side::Local(LocalView::new(home_dir()));
        }
        Ok(())
    }

    fn cwd(&mut self, a: &[Value]) -> Result<(), String> {
        want(a, 3, "CWD")?;
        let (names, param, flags) = (self.names(&s(a, 0)), s(a, 1), n(a, 2));
        for p in self.panels(&names) {
            let site_name = match &self.app.tabs[p.0].sides[p.1] {
                Side::Remote(r) => r.profile.name.clone(),
                Side::Local(_) => continue,
            };
            let dir = if has_flag(flags, "RS_BOOKMARK") {
                match self.app.sites.book.find_by_name(&site_name).and_then(|site| Self::bookmark(site, &param)) {
                    Some(bm) => bm.remote_dir.clone(),
                    None => {
                        self.log(LogKind::Status, format!("CWD : signet « {param} » absent de {site_name}"));
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

    /// Panneaux visés ; avec `RS_LOGIN`, connecte les sites qui ne le sont pas encore.
    fn targets(&mut self, names: &[String], flags: i64) -> Vec<(usize, usize)> {
        let mut found = self.panels(names);
        if has_flag(flags, "RS_LOGIN") {
            for name in names {
                if !found.iter().any(|&(t, sd)| matches!(&self.app.tabs[t].sides[sd], Side::Remote(r) if r.profile.name.eq_ignore_ascii_case(name))) {
                    match self.site(name) {
                        Ok(site) => found.push(self.connect(&site, None, flags)),
                        Err(e) => self.log(LogKind::Error, e),
                    }
                }
            }
        }
        if has_flag(flags, "RS_ONE") {
            // Une seule connexion par site.
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

    fn raw(&mut self, a: &[Value]) -> Result<(), String> {
        want(a, 3, "RAW")?;
        let (names, cmd, flags) = (self.names(&s(a, 0)), s(a, 1), n(a, 2));
        if cmd.trim().is_empty() {
            return Err("RAW : commande vide".into());
        }
        let targets = self.targets(&names, flags);
        if targets.is_empty() {
            self.log(LogKind::Status, format!("RAW « {cmd} » : aucune connexion visée"));
        }
        for p in targets {
            self.send(p, BrowserCmd::Raw(cmd.clone()));
            if has_flag(flags, "RS_LOGOUT") {
                self.send(p, BrowserCmd::Disconnect);
            }
        }
        self.popup(flags);
        Ok(())
    }

    fn delete(&mut self, a: &[Value]) -> Result<(), String> {
        want(a, 4, "Delete")?;
        let (names, path, item, flags) = (self.names(&s(a, 0)), s(a, 1), s(a, 2), n(a, 3));
        if names.is_empty() {
            return Err("Delete : la liste de sites ne peut pas être vide".into());
        }
        for p in self.targets(&names, flags) {
            let site_name = match &self.app.tabs[p.0].sides[p.1] {
                Side::Remote(r) => r.profile.name.clone(),
                Side::Local(_) => continue,
            };
            let dir = if has_flag(flags, "RS_BOOKMARK") {
                match self.app.sites.book.find_by_name(&site_name).and_then(|site| Self::bookmark(site, &path)) {
                    Some(bm) => bm.remote_dir.clone(),
                    None => {
                        self.log(LogKind::Error, format!("Delete : signet « {path} » absent de {site_name}"));
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

    /// Dossier d'une extrémité : chemin (`dir_flag`) ou signet du site / signet local.
    fn folder(site: Option<&Site>, value: &str, dir_flag: bool, local: bool) -> Result<String, String> {
        if dir_flag || value.is_empty() {
            return Ok(value.to_string());
        }
        let bm = site.and_then(|st| Self::bookmark(st, value)).ok_or_else(|| format!("signet « {value} » introuvable"))?;
        Ok(if local { bm.local_dir.clone() } else { bm.remote_dir.clone() })
    }

    fn transfer(&mut self, a: &[Value]) -> Result<(), String> {
        want(a, 8, "Transfer")?;
        let handle = n(a, 0);
        let (src_list, src_folder, src_item) = (s(a, 1), s(a, 2), s(a, 3));
        let (des_list, des_folder, des_item) = (s(a, 4), s(a, 5), s(a, 6));
        let flags = n(a, 7);
        let (up, down, is_file) = (has_flag(flags, "RS_UP"), has_flag(flags, "RS_DOWN"), has_flag(flags, "RS_FILE"));
        let (dir_src, dir_des) = (has_flag(flags, "RS_DIRSRC"), has_flag(flags, "RS_DIRDES"));
        for (i, label) in [(8, "FolderAllow"), (9, "FolderSkip"), (10, "FileAllow"), (11, "FileSkip"), (12, "CompleteFlag")] {
            if !s(a, i).is_empty() {
                self.log(LogKind::Status, format!("Transfer : {label} n'est pas encore pris en charge, ignoré"));
            }
        }
        if n(a, 13) > 0 {
            self.log(LogKind::Status, "Transfer : RepeatCount n'est pas encore pris en charge, ignoré");
        }

        let srcs = self.names(&src_list);
        let dsts = self.names(&des_list);
        // (site source, site destination, dossier source, élément source) ; None = local.
        let mut pairs: Vec<(Option<Site>, Option<Site>, String, String)> = Vec::new();
        if up {
            for d in &dsts {
                pairs.push((None, Some(self.site(d)?), src_folder.clone(), src_item.clone()));
            }
        } else if down {
            let src = srcs.first().ok_or("Transfer : site source manquant")?;
            pairs.push((Some(self.site(src)?), None, src_folder.clone(), src_item.clone()));
        } else if let Some(src) = srcs.first() {
            let src = self.site(src)?;
            for d in &dsts {
                pairs.push((Some(src.clone()), Some(self.site(d)?), src_folder.clone(), src_item.clone()));
            }
        } else {
            // Chaîne FTP1;FTP2;FTP3 : FTP1 → FTP2 et FTP2 → FTP3, mêmes dossier et élément.
            for w in dsts.windows(2) {
                pairs.push((Some(self.site(&w[0])?), Some(self.site(&w[1])?), des_folder.clone(), des_item.clone()));
            }
        }
        if pairs.is_empty() {
            return Err("Transfer : aucune paire source/destination".into());
        }

        let tab = if handle > 0 && (handle as usize) <= self.app.tabs.len() { handle as usize - 1 } else { self.app.active };
        let prio = self.app.options.settings.priority_list();
        let mut added = 0;
        for (src_site, dst_site, sfolder, sitem) in pairs {
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
            self.app.tabs[tab].queue.lock().unwrap().push(src, dst, !is_file, None, rank);
            added += 1;
        }
        self.log(LogKind::Status, format!("Transfer : {added} élément(s) ajouté(s) à la file"));
        let opts = self.app.engine_options();
        let extra = self.app.queue_site_profiles(tab);
        let rt = self.app.rt.clone();
        self.app.tabs[tab].start_queue(&rt, self.ctx, opts, &extra);
        self.popup(flags);
        Ok(())
    }

    fn remove_queue(&mut self, a: &[Value]) -> Result<(), String> {
        want(a, 4, "RemoveQueue")?;
        let (source, dest, param, flags) = (s(a, 0), s(a, 1), s(a, 2), n(a, 3));
        let states = ["RS_TRANSFER", "RS_NORMAL", "RS_FAIL", "RS_WAITING"];
        if !states.iter().any(|f| has_flag(flags, f)) {
            return Err("RemoveQueue : au moins un drapeau d'état est nécessaire (RS_TRANSFER, RS_NORMAL, RS_FAIL, RS_WAITING)".into());
        }
        let pattern = if param.is_empty() {
            None
        } else if has_flag(flags, "RS_EXPR") {
            Some(Pattern::regex(&param).map_err(|e| format!("RemoveQueue : expression invalide : {e}"))?)
        } else {
            Some(Pattern::wildcard(&param))
        };
        let endpoint_ok = |want: &str, e: &Endpoint| -> bool {
            want.is_empty() || (want.eq_ignore_ascii_case("local") && e.is_local()) || e.site.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(want))
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
                state_ok && endpoint_ok(&source, &it.src) && endpoint_ok(&dest, &it.dst) && pattern.as_ref().is_none_or(|p| p.matches(&target))
            };
            let running: Vec<ItemId> =
                self.app.tabs[t].queue.lock().unwrap().items().iter().filter(|i| i.state == ItemState::Running && matches(i)).map(|i| i.id).collect();
            for id in running {
                self.app.tabs[t].stop_item(id);
                stopped += 1;
            }
            removed += self.app.tabs[t].queue.lock().unwrap().remove_where(&matches);
        }
        self.log(LogKind::Status, format!("RemoveQueue : {removed} retiré(s), {stopped} arrêté(s)"));
        self.popup(flags);
        Ok(())
    }

    fn enable_site(&mut self, a: &[Value], enable: bool) -> Result<(), String> {
        want(a, 2, if enable { "EnableSite" } else { "DisableSite" })?;
        let (name, this_session) = (s(a, 0), b(a, 1));
        let id = self.app.sites.book.find_by_name(&name).map(|x| x.id).ok_or_else(|| format!("site inconnu : {name}"))?;
        if let Some(site) = self.app.sites.book.find_mut(id) {
            site.enabled = enable;
        }
        if !this_session {
            self.app.sites.save();
        }
        Ok(())
    }

    /// Ferme les onglets sans connexion et à la file vide (au moins un onglet reste).
    fn delete_empty_tabs(&mut self) {
        let mut i = 0;
        while i < self.app.tabs.len() && self.app.tabs.len() > 1 {
            let t = &self.app.tabs[i];
            let empty = t.sides.iter().all(|s| matches!(s, Side::Local(_))) && t.queue.lock().unwrap().items().is_empty();
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
    fn call(&mut self, name: &str, a: &[Value]) -> Result<Value, String> {
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
            "showmessage" => {
                let msg = s(a, 0);
                self.log(LogKind::Status, format!("Message : {msg}"));
            }
            "inputquery" => {
                self.log(LogKind::Status, "InputQuery : la saisie interactive n'est pas encore disponible, considérée comme annulée");
                return Ok(Value::Nil);
            }
            _ => return Err(format!("fonction inconnue : {name}")),
        }
        Ok(Value::Nil)
    }

    fn get(&mut self, name: &str) -> Result<Value, String> {
        let st = &self.app.stats;
        let remote = || self.app.tabs.iter().flat_map(|t| t.sides.iter());
        Ok(match name.to_ascii_lowercase().as_str() {
            "rushapp.version" => Value::Str(format!("UltraRushFTP {}", env!("CARGO_PKG_VERSION"))),
            "rushapp.language" => Value::Str("Français".into()),
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
                let running: usize = self.app.tabs.iter().map(|t| t.queue.lock().unwrap().items().iter().filter(|i| i.state == ItemState::Running).count()).sum();
                Value::Str(format!("{running} transfert(s) en cours, {} file(s) active(s)", self.app.tabs.iter().filter(|t| t.engine.is_some()).count()))
            }
            "rushapp.containers.containercount" => Value::Int(self.app.tabs.len() as i64),
            "rushapp.containers.connectedsitecount" => {
                Value::Int(remote().filter(|s| matches!(s, Side::Remote(r) if r.state == RemoteState::Connected)).count() as i64)
            }
            "rushapp.containers.transfersitecount" => Value::Int(self.app.tabs.iter().filter(|t| t.engine.is_some()).count() as i64),
            "rushapp.downloadbw" => Value::Int(st.bandwidth().0.round() as i64),
            "rushapp.uploadbw" => Value::Int(st.bandwidth().1.round() as i64),
            _ => return Err(format!("propriété inconnue : {name}")),
        })
    }

    fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        match name.to_ascii_lowercase().as_str() {
            "rushapp.ftp.splitter" => {
                let v = value.to_text();
                if v.is_empty() {
                    return Err("le séparateur ne peut pas être vide".into());
                }
                self.app.splitter = v;
                Ok(())
            }
            _ => Err(format!("propriété non modifiable : {name}")),
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
        assert!(parse_date("pas une date").is_none());
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
        st.record(&[(TransferKind::Download, 1024 * 1024), (TransferKind::Upload, 10), (TransferKind::Fxp, 5)]);
        assert_eq!(st.session, [1024 * 1024, 10, 5]);
        assert_eq!(mb(st.totals.download), Value::Str("1.00".into()));
        st.dirty = false; // pas d'écriture disque pendant le test
    }
}
