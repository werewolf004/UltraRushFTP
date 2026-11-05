//! `urftp` : UltraRushFTP en ligne de commande. Utilise le gestionnaire de sites et les
//! options de l'interface (même dossier de réglages), sans fenêtre.

mod args;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::mpsc::unbounded_channel;
use urftp_core::exists::ExistsRules;
use urftp_core::queue::{Endpoint, ItemState, Queue};
use urftp_core::settings::Settings;
use urftp_core::site::{CertPolicy, SiteBook};
use urftp_core::sitestore;
use urftp_proto::engine::{Engine, EngineEvent};
use urftp_proto::ftp::{LogLine, LogSink, RemoteEntry, path as rpath};
use urftp_proto::profile::{engine_options, site_profile};
use urftp_proto::session::{RemoteSession, SiteProfile};
use urftp_proto::throttle::RateLimiter;

use args::{Command, Options, Target};

type Res<T> = Result<T, String>;

/// Fiche de site et options chargées depuis le dossier des réglages.
struct Ctx {
    book: SiteBook,
    settings: Settings,
    opts: Options,
}

impl Ctx {
    fn load(opts: Options) -> Res<Self> {
        let book = sitestore::load(&sitestore::default_path()).map_err(|e| format!("gestionnaire de sites illisible : {e}"))?;
        let settings = Settings::load(&Settings::default_path()).unwrap_or_default();
        Ok(Self { book, settings, opts })
    }

    /// Profil de connexion et chemin d'une cible (`Site:/chemin` ou URL).
    fn resolve(&self, target: &Target) -> Res<(SiteProfile, String)> {
        let (mut site, path) = match target {
            Target::Named { site, path } => {
                let s = self.book.find_by_name(site).ok_or_else(|| format!("site inconnu : {site} (urftp sites)"))?;
                if !s.enabled {
                    return Err(format!("le site {} est désactivé", s.name));
                }
                let path = if path.is_empty() { s.remote_dir.clone() } else { path.clone() };
                (s.clone(), path)
            }
            Target::Url { site, path } => ((**site).clone(), path.clone()),
        };
        if self.opts.insecure {
            site.cert = CertPolicy::AcceptAny;
        }
        if let Some(fp) = &self.opts.fingerprint {
            site.cert = CertPolicy::Fingerprint(fp.clone());
            site.ssh_host_keys.push(fp.clone());
        }
        if let Some(n) = self.opts.transfers {
            site.max_transfers = n;
        }
        Ok((site_profile(&site, &self.settings), path))
    }

    fn log_sink(&self) -> Option<LogSink> {
        self.opts.verbose.then(|| {
            Arc::new(|l: LogLine| match l {
                LogLine::Command(c) => eprintln!("> {c}"),
                LogLine::Reply(r) => r.lines.iter().for_each(|x| eprintln!("< {x}")),
                LogLine::Status(s) => eprintln!("* {s}"),
            }) as LogSink
        })
    }

    async fn open(&self, profile: &SiteProfile) -> Res<RemoteSession> {
        RemoteSession::open(profile, self.log_sink()).await.map_err(|e| connect_error(profile, &e.to_string()))
    }
}

fn connect_error(p: &SiteProfile, e: &str) -> String {
    let hint = if e.contains("empreinte") { "\n  (vérifiez l'empreinte puis relancez avec --fingerprint <empreinte>)" } else { "" };
    format!("{} : {e}{hint}", p.name)
}

/// Chemin absolu : relatif au dossier courant de la session.
fn absolute(session: &RemoteSession, path: &str) -> String {
    let cwd = session.current_dir().unwrap_or_else(|| "/".into());
    if path.is_empty() { rpath::normalize(&cwd) } else { rpath::join(&cwd, path) }
}

fn human(n: u64) -> String {
    const U: [&str; 5] = ["o", "Kio", "Mio", "Gio", "Tio"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} o") } else { format!("{v:.1} {}", U[i]) }
}

fn print_listing(entries: &mut [RemoteEntry]) {
    entries.sort_by(|a, b| b.is_dir().cmp(&a.is_dir()).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    for e in entries.iter() {
        let date = e
            .modified
            .map(|t| match t.year {
                Some(y) => format!("{y:04}-{:02}-{:02} {:02}:{:02}", t.month, t.day, t.hour, t.minute),
                None => format!("     {:02}-{:02} {:02}:{:02}", t.month, t.day, t.hour, t.minute),
            })
            .unwrap_or_default();
        let size = if e.is_dir() { "<DIR>".to_string() } else { human(e.size) };
        let link = e.link_target.as_deref().map(|t| format!(" -> {t}")).unwrap_or_default();
        println!("{:>12}  {date:<16}  {}{}{link}", size, e.name, if e.is_dir() { "/" } else { "" });
    }
}

/// Entrée distante (dossier ou fichier) d'après le contenu de son parent.
async fn stat(session: &mut RemoteSession, path: &str) -> Res<RemoteEntry> {
    let (parent, name) = rpath::split(path);
    let parent = parent.unwrap_or("/");
    let entries = session.list(parent).await.map_err(|e| format!("{parent} : {e}"))?;
    entries.into_iter().find(|e| e.name == name).ok_or_else(|| format!("introuvable : {path}"))
}

async fn cmd_sites(ctx: &Ctx) -> Res<()> {
    let sites = ctx.book.all_sites();
    if sites.is_empty() {
        println!("Aucun site (gestionnaire : {}).", sitestore::default_path().display());
    }
    for (groups, s) in sites {
        let folder = if groups.is_empty() { String::new() } else { format!("{}/", groups.join("/")) };
        let state = if s.enabled { "" } else { "  (désactivé)" };
        println!("{folder}{:<32} {:<14} {}:{}{state}", s.name, s.protocol.label(), s.host, s.port);
    }
    Ok(())
}

async fn cmd_ls(ctx: &Ctx, target: &str) -> Res<()> {
    let (profile, path) = ctx.resolve(&args::parse_target(target)?)?;
    let mut session = ctx.open(&profile).await?;
    let dir = absolute(&session, &path);
    let mut entries = session.list(&dir).await.map_err(|e| format!("{dir} : {e}"))?;
    println!("{}:{dir}", profile.name);
    print_listing(&mut entries);
    Ok(())
}

async fn cmd_raw(ctx: &Ctx, site: &str, commands: &[String]) -> Res<()> {
    let (profile, _) = ctx.resolve(&args::parse_target(site)?)?;
    let mut session = ctx.open(&profile).await?;
    let ftp = session.as_ftp().ok_or("raw : commandes brutes disponibles en FTP/FTPS seulement")?;
    let mut failed = false;
    for c in commands {
        match ftp.raw(c).await {
            Ok(r) => {
                r.lines.iter().for_each(|l| println!("{l}"));
                failed |= r.code >= 400;
            }
            Err(e) => return Err(format!("{c} : {e}")),
        }
    }
    if failed { Err("le serveur a refusé au moins une commande".into()) } else { Ok(()) }
}

/// Transfert par le moteur de file (multi-transfert, dossiers, skiplist, priorités…).
async fn run_queue(ctx: &Ctx, sites: Vec<SiteProfile>, src: Endpoint, dst: Endpoint, is_dir: bool, size: Option<u64>) -> Res<()> {
    let queue = Arc::new(Mutex::new(Queue::new()));
    queue.lock().unwrap().push(src, dst, is_dir, size, None);
    let s = &ctx.settings;
    let dl = RateLimiter::new(s.transfer.download_limit_kib * 1024);
    let ul = RateLimiter::new(s.transfer.upload_limit_kib * 1024);
    let mut opts = engine_options(s, &dl, &ul);
    if let Some(a) = ctx.opts.exists {
        opts.exists = ExistsRules { smaller: a, same: a, larger: a, ..opts.exists };
    }
    let (tx, mut rx) = unbounded_channel();
    let engine = Engine::new(queue.clone(), sites, opts, Some(tx));
    // Le moteur garde l'émetteur des événements : la boucle s'arrête quand il est libéré.
    let runner = tokio::spawn(async move { engine.run().await });
    let started = Instant::now();
    let mut starts = std::collections::HashMap::new();
    let (mut files, mut bytes_total) = (0u64, 0u64);
    let name = |id| queue.lock().unwrap().get(id).map(|i| i.src.path.clone()).unwrap_or_default();
    while let Some(ev) = rx.recv().await {
        match ev {
            EngineEvent::Started { id } => {
                starts.insert(id, Instant::now());
            }
            EngineEvent::Done { id, bytes } => {
                let secs = starts.remove(&id).map_or(0.0, |t| t.elapsed().as_secs_f64()).max(0.001);
                files += 1;
                bytes_total += bytes;
                println!("ok      {}  ({}, {}/s)", name(id), human(bytes), human((bytes as f64 / secs) as u64));
            }
            EngineEvent::Expanded { id, children } => println!("dossier {}  ({children} élément(s))", name(id)),
            EngineEvent::Skipped { id, reason } => println!("ignoré  {}  ({reason})", name(id)),
            EngineEvent::NeedsConfirmation { id } => println!("existe  {}  (règle « demander » : utilisez --exists)", name(id)),
            EngineEvent::Retrying { id, attempt, error } => eprintln!("reprise {}  (essai {attempt} : {error})", name(id)),
            EngineEvent::Failed { id, error } => eprintln!("échec   {}  : {error}", name(id)),
            EngineEvent::Stopped { id } => eprintln!("arrêté  {}", name(id)),
            EngineEvent::Progress { .. } => {}
        }
    }
    let _ = runner.await;
    let secs = started.elapsed().as_secs_f64().max(0.001);
    println!("{files} fichier(s), {} en {secs:.1} s ({}/s)", human(bytes_total), human((bytes_total as f64 / secs) as u64));
    let q = queue.lock().unwrap();
    let bad = q.items().iter().filter(|i| matches!(i.state, ItemState::Failed(_) | ItemState::NeedsConfirmation | ItemState::Stopped)).count();
    if bad > 0 { Err(format!("{bad} élément(s) non transféré(s)")) } else { Ok(()) }
}

fn local_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

async fn cmd_get(ctx: &Ctx, src: &str, dest: Option<&str>) -> Res<()> {
    let (profile, path) = ctx.resolve(&args::parse_target(src)?)?;
    let mut session = ctx.open(&profile).await?;
    let path = absolute(&session, &path);
    let entry = stat(&mut session, &path).await?;
    drop(session);
    let dir = match dest {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    let local = dir.join(&entry.name);
    let size = (!entry.is_dir()).then_some(entry.size);
    let src = Endpoint::remote(profile.name.clone(), path);
    run_queue(ctx, vec![profile], src, Endpoint::local(local_string(&local)), entry.is_dir(), size).await
}

async fn cmd_put(ctx: &Ctx, src: &str, dest: &str) -> Res<()> {
    let local = std::path::absolute(src).map_err(|e| format!("{src} : {e}"))?;
    let meta = std::fs::metadata(&local).map_err(|e| format!("{} : {e}", local.display()))?;
    let name = local.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| format!("nom invalide : {src}"))?;
    let (profile, path) = ctx.resolve(&args::parse_target(dest)?)?;
    let session = ctx.open(&profile).await?;
    let dir = absolute(&session, &path);
    drop(session);
    let dst = Endpoint::remote(profile.name.clone(), rpath::join(&dir, &name));
    let size = meta.is_file().then_some(meta.len());
    run_queue(ctx, vec![profile], Endpoint::local(local_string(&local)), dst, meta.is_dir(), size).await
}

async fn cmd_fxp(ctx: &Ctx, src: &str, dest: &str) -> Res<()> {
    let (sp, spath) = ctx.resolve(&args::parse_target(src)?)?;
    let (dp, dpath) = ctx.resolve(&args::parse_target(dest)?)?;
    if sp.name == dp.name {
        return Err("FXP : la source et la destination doivent être deux sites différents".into());
    }
    let mut s = ctx.open(&sp).await?;
    let spath = absolute(&s, &spath);
    let entry = stat(&mut s, &spath).await?;
    drop(s);
    let d = ctx.open(&dp).await?;
    let ddir = absolute(&d, &dpath);
    drop(d);
    let size = (!entry.is_dir()).then_some(entry.size);
    let src = Endpoint::remote(sp.name.clone(), spath);
    let dst = Endpoint::remote(dp.name.clone(), rpath::join(&ddir, &entry.name));
    run_queue(ctx, vec![sp, dp], src, dst, entry.is_dir(), size).await
}

async fn run(cmd: Command, opts: Options) -> Res<()> {
    if let Command::Help = cmd {
        println!("{}", args::USAGE);
        return Ok(());
    }
    let ctx = Ctx::load(opts)?;
    match &cmd {
        Command::Sites => cmd_sites(&ctx).await,
        Command::Ls(t) => cmd_ls(&ctx, t).await,
        Command::Get { src, dest } => cmd_get(&ctx, src, dest.as_deref()).await,
        Command::Put { src, dest } => cmd_put(&ctx, src, dest).await,
        Command::Fxp { src, dest } => cmd_fxp(&ctx, src, dest).await,
        Command::Raw { site, commands } => cmd_raw(&ctx, site, commands).await,
        Command::Help => unreachable!(),
    }
}

fn main() -> ExitCode {
    let (cmd, opts) = match args::parse(std::env::args().skip(1)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("urftp : {e}");
            return ExitCode::from(2);
        }
    };
    if let Some(d) = &opts.data_dir {
        // SAFETY : aucun autre thread n'existe encore.
        unsafe { std::env::set_var("URUSHFTP_DATA", d) };
    }
    let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("urftp : {e}");
            return ExitCode::FAILURE;
        }
    };
    match rt.block_on(run(cmd, opts)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("urftp : {e}");
            ExitCode::FAILURE
        }
    }
}
