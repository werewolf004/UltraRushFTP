//! Queue engine: runs the items of a [`Queue`] in parallel.
//!
//! - **Multi-transfer**: several files to or from the same site at the same time, on
//!   separate connections, within [`SiteLimits`] (0 = single connection).
//! - **Connection pool** per site: a released connection serves the next transfer.
//! - **Folders**: a folder is expanded in turn (listing, skip list, priorities) and its
//!   contents take its place in the queue.
//! - **Rules**: skip list, "existing file" (resume / skip / rename / overwrite / ask),
//!   fallback without resume when `REST` is refused, retries.
//! - **Stopping** an item: its task is aborted and its connections closed. *Stop all*
//!   ends the run; **pause** puts the running items back to waiting (they resume from
//!   where they were) and keeps the idle connections until *resume*.
//! - **Speed** limited for uploads and downloads.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use std::time::{Duration, Instant};

use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::{AbortHandle, JoinSet};

use urftp_core::exists::{ExistsDecision, ExistsRules, ExistsSet};
use urftp_core::itemopts::FolderEntry;
use urftp_core::pattern::Pattern;
use urftp_core::priority::PriorityList;
use urftp_core::queue::{Endpoint, ItemId, ItemState, Load, Queue, QueueItem, SiteLimits};
use urftp_core::site::ResumeMode;
use urftp_core::skiplist::{Candidate, SkipList, SkipReason};
use urftp_core::{NameCase, TransferKind, TransferMode};

use crate::ftp::{DataOptions, EntryKind, FtpError, FtpTime, FxpOptions, LogSink, fxp, path as rpath};
use crate::session::{Account, RemoteSession, SessionError, SiteProfile, SiteRules};
use crate::throttle::{RateLimiter, Throttled};

/// Engine settings.
#[derive(Debug, Clone)]
pub struct EngineOptions {
    /// Retries after a failure ("retry count" in v2).
    pub retries: u32,
    /// Retries per direction, instead of `retries` when set.
    pub retries_download: Option<u32>,
    pub retries_upload: Option<u32>,
    pub retries_fxp: Option<u32>,
    pub retry_delay: Duration,
    /// Cycle retry: an item to retry goes to the bottom of the queue, without delay.
    pub cycle_retry: bool,
    /// Rounds of retry of the failed items once the queue is finished.
    pub retry_failed_rounds: u32,
    /// Simultaneous transfers across all sites (`None`: only the site limits).
    pub max_total: Option<u32>,
    pub skiplist: SkipList,
    pub priorities: PriorityList,
    /// "Existing file" rules per direction.
    pub exists: ExistsSet,
    pub fxp: FxpOptions,
    /// Download speed (server → local), shared by every transfer.
    pub download_limit: Arc<RateLimiter>,
    /// Upload speed (local → server).
    pub upload_limit: Arc<RateLimiter>,
    /// Protocol log of the transfer connections (`None`: not logged).
    pub log: Option<TransferLog>,
    /// Modes, names, disk and speed rules of the transfers.
    pub rules: TransferRules,
}

/// SFV results of a folder: names (lower case) found good, found bad.
type SfvResults = (BTreeSet<String>, BTreeSet<String>);

/// Rules applied to every file transfer.
#[derive(Debug, Clone, Default)]
pub struct TransferRules {
    /// Mode of the items in `Auto` (`Auto`: by `ascii_masks`).
    pub mode: TransferMode,
    pub ascii_masks: Vec<Pattern>,
    /// ASCII files start again from zero instead of resuming.
    pub no_resume_ascii: bool,
    /// Bytes cut from a partial file before resuming a download.
    pub resume_rollback: u64,
    /// Bytes kept free on the local disk.
    pub keep_free: u64,
    /// No speed limit for the sites of the local network.
    pub unlimited_local: bool,
    pub upload_case: NameCase,
    pub download_case: NameCase,
    pub fxp_case: NameCase,
    /// Lists the target folder again when an upload is refused without `XDUPE` names: a
    /// file found there counts as a dupe.
    pub xdupe_refresh: bool,
    /// `MODE Z` on, with the masks never compressed (`None`: off).
    pub mode_z: Option<Vec<Pattern>>,
    /// `MODE Z` left out for the downloads / the uploads.
    pub mode_z_skip_downloads: bool,
    pub mode_z_skip_uploads: bool,
    /// The SFV check keeps a status folder beside the downloaded files.
    pub sfv_status_dir: bool,
    /// Downloads checked against the SFV file of their folder.
    pub check_sfv: bool,
    /// Uploads and FXP checked with `XCRC`.
    pub use_xcrc: bool,
    /// A file with a bad CRC is removed and transferred again.
    pub readd_bad_crc: bool,
    /// Several SFV files in a folder: none is taken.
    pub one_sfv_per_folder: bool,
    /// Target folders left empty are removed at the end of the run.
    pub remove_empty_dirs: bool,
    /// Downloaded files keep the date of the server.
    pub keep_server_date: bool,
    /// A sub-folder named like its parent is not transferred.
    pub refuse_same_name_dir: bool,
    /// The 0-byte target left by a refused FXP source is removed.
    pub fxp_remove_zero_target: bool,
}

/// The local disk would go below the space to keep free.
#[derive(Debug, thiserror::Error)]
#[error("not enough disk space: {needed} bytes needed, {free} free (keep {keep} free)")]
pub struct DiskFull {
    pub needed: u64,
    pub free: u64,
    pub keep: u64,
}

/// Protocol log shared by every connection of the engine.
#[derive(Clone)]
pub struct TransferLog(pub LogSink);

impl std::fmt::Debug for TransferLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TransferLog")
    }
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            retries: 3,
            retries_download: None,
            retries_upload: None,
            retries_fxp: None,
            retry_delay: Duration::from_secs(3),
            cycle_retry: false,
            retry_failed_rounds: 0,
            max_total: None,
            skiplist: SkipList::default(),
            priorities: PriorityList::default(),
            exists: ExistsSet::default(),
            fxp: FxpOptions::default(),
            download_limit: RateLimiter::new(0),
            upload_limit: RateLimiter::new(0),
            log: None,
            rules: TransferRules::default(),
        }
    }
}

/// Events for the interface (queue, status bar, transfer log).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    Started {
        id: ItemId,
    },
    /// `bytes`: position in the file (resume included).
    Progress {
        id: ItemId,
        bytes: u64,
        total: Option<u64>,
    },
    Done {
        id: ItemId,
        bytes: u64,
    },
    /// Folder expanded into `children` items.
    Expanded {
        id: ItemId,
        children: usize,
    },
    Skipped {
        id: ItemId,
        reason: SkipReason,
    },
    NeedsConfirmation {
        id: ItemId,
    },
    Retrying {
        id: ItemId,
        attempt: u32,
        error: String,
    },
    Failed {
        id: ItemId,
        error: String,
    },
    Stopped {
        id: ItemId,
    },
    /// Interrupted by a pause: waiting again.
    Paused {
        id: ItemId,
    },
    /// CRC of a transferred file checked (SFV or `XCRC`); `path` is the file checked.
    CrcChecked {
        id: ItemId,
        path: String,
        ok: bool,
    },
    /// Synchronisation: `path` of the target, missing at the source, was removed.
    SyncDeleted {
        id: ItemId,
        path: String,
    },
    /// The queue ended with failed items: `items` of them wait again (round `round`).
    RetryRound {
        round: u32,
        items: usize,
    },
}

/// Idle time after which a pooled connection is checked before being reused.
const IDLE_CHECK: Duration = Duration::from_secs(30);

/// Connections of a site, per login (main, upload and download accounts).
struct SitePool {
    profile: SiteProfile,
    /// Idle connections and when they were released.
    idle: Mutex<HashMap<Account, Vec<(RemoteSession, Instant)>>>,
    log: Option<LogSink>,
}

impl SitePool {
    /// An idle connection of this login, checked with the site's anti-idle command when it
    /// waited long enough to have been kicked; otherwise a new one.
    async fn acquire(&self, account: Account) -> Result<RemoteSession, SessionError> {
        let account = self.profile.account(account);
        loop {
            let idle = self.idle.lock().get_mut(&account).and_then(Vec::pop);
            let Some((mut s, since)) = idle else { break };
            if since.elapsed() < self.check_after() {
                return Ok(s);
            }
            let cmd = self.profile.session.keep_alive.as_ref().and_then(|k| k.commands.first().cloned());
            if s.keep_alive(cmd.as_deref().unwrap_or("NOOP")).await.is_ok() {
                return Ok(s);
            }
            // Gone (idle kick...): try the next one, or open a new connection.
        }
        RemoteSession::open(&self.profile.with_account(account), self.log.clone()).await
    }

    fn check_after(&self) -> Duration {
        self.profile.session.keep_alive.as_ref().map_or(IDLE_CHECK, |k| k.interval.min(IDLE_CHECK))
    }

    /// Gives the connection back to the pool when it is still healthy.
    fn release<T>(&self, account: Account, session: RemoteSession, result: &Result<T, SessionError>) {
        if result.as_ref().err().is_none_or(SessionError::keeps_session) {
            self.idle.lock().entry(self.profile.account(account)).or_default().push((session, Instant::now()));
        }
    }
}

enum Outcome {
    Done(u64),
    Expanded(usize),
    Skipped(SkipReason),
    Ask,
}

type Res<T> = Result<T, SessionError>;

pub struct Engine {
    queue: Arc<Mutex<Queue>>,
    sites: HashMap<String, Arc<SitePool>>,
    opts: EngineOptions,
    events: Option<UnboundedSender<EngineEvent>>,
    load: Mutex<Load>,
    running: Mutex<HashMap<ItemId, AbortHandle>>,
    /// Paused: nothing starts, the interrupted items wait again.
    paused: AtomicBool,
    /// *Stop all*: nothing starts any more and the run ends.
    halted: AtomicBool,
    /// Wakes a paused run (resume or stop).
    wake: Notify,
    /// Dupes announced by `XDUPE`: (site, folder) → names in lower case.
    dupes: Mutex<HashMap<(String, String), HashSet<String>>>,
    /// SFV results of each local folder: names (lower case) found good, found bad.
    sfv_results: Mutex<HashMap<PathBuf, SfvResults>>,
}

impl Engine {
    pub fn new(
        queue: Arc<Mutex<Queue>>,
        sites: impl IntoIterator<Item = SiteProfile>,
        opts: EngineOptions,
        events: Option<UnboundedSender<EngineEvent>>,
    ) -> Arc<Self> {
        let log = opts.log.as_ref().map(|l| l.0.clone());
        let sites = sites
            .into_iter()
            .map(|p| (p.name.clone(), Arc::new(SitePool { profile: p, idle: Mutex::default(), log: log.clone() })))
            .collect();
        Arc::new(Self {
            queue,
            sites,
            opts,
            events,
            load: Mutex::default(),
            running: Mutex::default(),
            paused: AtomicBool::new(false),
            halted: AtomicBool::new(false),
            wake: Notify::new(),
            dupes: Mutex::default(),
            sfv_results: Mutex::default(),
        })
    }

    pub fn queue(&self) -> Arc<Mutex<Queue>> {
        self.queue.clone()
    }

    /// Idle connections kept for `site` (for display / tests).
    pub fn idle_connections(&self, site: &str) -> usize {
        self.sites.get(site).map_or(0, |p| p.idle.lock().values().map(Vec::len).sum())
    }

    /// Stops a running item: its connections are closed and it goes to `Stopped`.
    pub fn stop(&self, id: ItemId) -> bool {
        self.running.lock().get(&id).map(AbortHandle::abort).is_some()
    }

    /// Stops every running item and ends the run: the waiting items stay waiting.
    pub fn stop_all(&self) {
        self.halted.store(true, Ordering::SeqCst);
        for h in self.running.lock().values() {
            h.abort();
        }
        self.wake.notify_one();
    }

    /// Pauses the queue: the running transfers are interrupted and wait again (they resume
    /// from their partial file), nothing starts until [`Engine::resume`].
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        for h in self.running.lock().values() {
            h.abort();
        }
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.wake.notify_one();
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// May new items start?
    fn starting(&self) -> bool {
        !self.paused.load(Ordering::SeqCst) && !self.halted.load(Ordering::SeqCst)
    }

    fn emit(&self, e: EngineEvent) {
        if let Some(tx) = &self.events {
            let _ = tx.send(e);
        }
    }

    fn limits(&self, site: &str) -> SiteLimits {
        self.sites.get(site).map(|p| p.profile.limits).unwrap_or_default()
    }

    fn pool(&self, site: &str) -> Res<Arc<SitePool>> {
        self.sites.get(site).cloned().ok_or_else(|| SessionError::UnknownSite(site.to_string()))
    }

    /// Rules of the site the files come from (skip list, priorities).
    fn source_rules(&self, item: &QueueItem) -> Option<&SiteRules> {
        let site = item.src.site.as_deref().or(item.dst.site.as_deref())?;
        self.sites.get(site).map(|p| &p.profile.rules)
    }

    /// Rules of the site the files go to (retries, "existing file", resume).
    fn target_rules(&self, item: &QueueItem) -> Option<&SiteRules> {
        let site = item.dst.site.as_deref().or(item.src.site.as_deref())?;
        self.sites.get(site).map(|p| &p.profile.rules)
    }

    fn skiplist(&self, item: &QueueItem) -> &SkipList {
        self.source_rules(item).and_then(|r| r.skiplist.as_ref()).unwrap_or(&self.opts.skiplist)
    }

    fn priorities(&self, item: &QueueItem) -> &PriorityList {
        self.source_rules(item).and_then(|r| r.priorities.as_ref()).unwrap_or(&self.opts.priorities)
    }

    fn exists_rules(&self, item: &QueueItem) -> ExistsRules {
        self.target_rules(item).and_then(|r| r.exists).unwrap_or(self.opts.exists).for_kind(item.kind())
    }

    /// When resuming fails, start again from zero? Not when the site always resumes.
    fn restart_if_resume_fails(&self, item: &QueueItem) -> bool {
        self.exists_rules(item).overwrite_if_resume_fails && self.target_rules(item).is_none_or(|r| r.resume != ResumeMode::Always)
    }

    /// Upload refused without `XDUPE` names: with the refresh asked, a target found anyway
    /// counts as a dupe.
    async fn dupe_after_refresh(&self, item: &QueueItem, s: &mut RemoteSession, dst: &str) -> bool {
        let refresh = self.target_rules(item).and_then(|r| r.xdupe_refresh).unwrap_or(self.opts.rules.xdupe_refresh);
        refresh && s.size_if_exists(dst).await.ok().flatten().is_some()
    }

    /// Retries and delay between them: the site setting, else the setting of the
    /// direction, else the global one.
    fn retries(&self, item: &QueueItem) -> (u32, Duration) {
        let r = self.target_rules(item);
        let by_kind = match item.kind() {
            TransferKind::Download => self.opts.retries_download,
            TransferKind::Upload => self.opts.retries_upload,
            TransferKind::Fxp => self.opts.retries_fxp,
        };
        let script = item.options.as_ref().and_then(|o| o.retries);
        let retries = script.or_else(|| r.and_then(|r| r.retries)).or(by_kind).unwrap_or(self.opts.retries);
        (retries, r.and_then(|r| r.retry_delay).unwrap_or(self.opts.retry_delay))
    }

    /// Decision for an existing target; without resume on the target site, a partial file
    /// is transferred again from zero.
    fn decide(&self, item: &QueueItem, size: Option<u64>, target: Option<u64>) -> ExistsDecision {
        // Synchronisation chose this file because it differs: it replaces the target.
        if item.options.as_ref().is_some_and(|o| o.sync.is_some()) {
            return ExistsDecision::Transfer { offset: 0 };
        }
        match self.exists_rules(item).decide(size, target) {
            ExistsDecision::Transfer { offset } if offset > 0 && self.target_rules(item).is_some_and(|r| r.resume == ResumeMode::Never) => {
                ExistsDecision::Transfer { offset: 0 }
            }
            d => d,
        }
    }

    /// Processes the queue until nothing is left to start.
    pub async fn run(self: &Arc<Self>) {
        let mut tasks: JoinSet<()> = JoinSet::new();
        let mut round = 0;
        let mut by_task: HashMap<tokio::task::Id, ItemId> = HashMap::new();
        loop {
            while self.starting() {
                // Pick and mark the item under one lock: nobody can remove it in between.
                let item = {
                    let mut q = self.queue.lock();
                    let next = {
                        let load = self.load.lock();
                        q.next_runnable(|it| load.allows(it, &|s| self.limits(s), self.opts.max_total))
                    };
                    let Some(id) = next else { break };
                    q.set_state(id, ItemState::Running);
                    match q.get(id).cloned() {
                        Some(item) => item,
                        None => continue,
                    }
                };
                let id = item.id;
                self.load.lock().start(&item);
                let me = self.clone();
                let handle = tasks.spawn(async move { me.process(item).await });
                by_task.insert(handle.id(), id);
                self.running.lock().insert(id, handle);
            }
            if tasks.is_empty() {
                // Paused with nothing running: wait for resume (or stop all).
                if self.paused.load(Ordering::SeqCst) && !self.halted.load(Ordering::SeqCst) {
                    self.wake.notified().await;
                    continue;
                }
                // Finished: the failed items get another round when asked.
                if round < self.opts.retry_failed_rounds && !self.halted.load(Ordering::SeqCst) {
                    let items = self.queue.lock().retry_failed_only();
                    if items > 0 {
                        round += 1;
                        self.emit(EngineEvent::RetryRound { round, items });
                        tokio::time::sleep(self.opts.retry_delay).await;
                        continue;
                    }
                }
                if self.opts.rules.remove_empty_dirs && !self.halted.load(Ordering::SeqCst) {
                    self.remove_empty_targets().await;
                }
                break;
            }
            let Some(joined) = tasks.join_next_with_id().await else { break };
            let (task_id, cancelled) = match joined {
                Ok((tid, ())) => (tid, false),
                Err(e) => (e.id(), e.is_cancelled()),
            };
            let Some(id) = by_task.remove(&task_id) else { continue };
            self.running.lock().remove(&id);
            let mut q = self.queue.lock();
            if let Some(item) = q.get(id).cloned() {
                self.load.lock().finish(&item);
                if cancelled && self.paused.load(Ordering::SeqCst) && !self.halted.load(Ordering::SeqCst) {
                    // Paused: it starts again on resume, from its partial file.
                    q.set_state(id, ItemState::Waiting);
                    self.emit(EngineEvent::Paused { id });
                } else if cancelled {
                    q.set_state(id, ItemState::Stopped);
                    self.emit(EngineEvent::Stopped { id });
                } else if item.state == ItemState::Running {
                    // The task panicked: do not leave the item stuck.
                    q.set_state(id, ItemState::Failed("internal error".into()));
                }
            }
        }
    }

    /// Removes the target folders of the expanded items that stayed empty (deepest first).
    async fn remove_empty_targets(&self) {
        let mut dirs: Vec<Endpoint> = {
            let q = self.queue.lock();
            q.items()
                .iter()
                .filter(|i| i.is_dir && i.state == ItemState::Done)
                .map(|i| Endpoint { site: i.dst.site.clone(), path: self.cased(i, &i.dst.path) })
                .collect()
        };
        dirs.sort_by_key(|d| std::cmp::Reverse(d.path.len()));
        dirs.dedup();
        for d in dirs {
            match &d.site {
                None => {
                    let empty = std::fs::read_dir(&d.path).is_ok_and(|mut rd| rd.next().is_none());
                    if empty {
                        let _ = tokio::fs::remove_dir(&d.path).await;
                    }
                }
                Some(site) => {
                    let Ok(pool) = self.pool(site) else { continue };
                    let Ok(mut s) = pool.acquire(Account::Upload).await else { continue };
                    let r = s.list(&d.path).await;
                    if r.as_ref().is_ok_and(Vec::is_empty) {
                        let _ = s.rmdir(&d.path).await;
                    }
                    pool.release(Account::Upload, s, &r);
                }
            }
        }
    }

    async fn process(self: Arc<Self>, item: QueueItem) {
        let id = item.id;
        self.emit(EngineEvent::Started { id });
        let res = if item.is_dir { self.expand(&item).await.map(Outcome::Expanded) } else { self.transfer(&item).await };
        let (state, event) = match res {
            Ok(Outcome::Done(bytes)) => (ItemState::Done, EngineEvent::Done { id, bytes }),
            Ok(Outcome::Expanded(children)) => (ItemState::Done, EngineEvent::Expanded { id, children }),
            Ok(Outcome::Skipped(reason)) => (ItemState::Skipped(reason.clone()), EngineEvent::Skipped { id, reason }),
            Ok(Outcome::Ask) => (ItemState::NeedsConfirmation, EngineEvent::NeedsConfirmation { id }),
            Err(e) => {
                let attempt = item.attempts + 1;
                let error = e.to_string();
                let (retries, delay) = self.retries(&item);
                if attempt <= retries {
                    self.emit(EngineEvent::Retrying { id, attempt, error });
                    if self.opts.cycle_retry {
                        // The other items run first; this one waits at the bottom.
                        self.queue.lock().move_to_bottom(id);
                    } else {
                        tokio::time::sleep(delay).await;
                    }
                    if let Some(i) = self.queue.lock().get_mut(id) {
                        i.attempts = attempt;
                    }
                    (ItemState::Waiting, EngineEvent::Started { id })
                } else {
                    (ItemState::Failed(error.clone()), EngineEvent::Failed { id, error })
                }
            }
        };
        let retry = state == ItemState::Waiting;
        self.queue.lock().set_state(id, state);
        if !retry {
            self.emit(event);
        }
    }

    /// ASCII or binary, compressed or not, for this file.
    fn data_options(&self, item: &QueueItem, name: &str) -> DataOptions {
        let rules = &self.opts.rules;
        let ascii = match item.mode {
            TransferMode::Ascii => true,
            TransferMode::Binary => false,
            TransferMode::Auto => match rules.mode {
                TransferMode::Ascii => true,
                TransferMode::Binary => false,
                TransferMode::Auto => rules.ascii_masks.iter().any(|m| m.matches(name)),
            },
        };
        let direction = match item.kind() {
            TransferKind::Download => !rules.mode_z_skip_downloads,
            TransferKind::Upload => !rules.mode_z_skip_uploads,
            TransferKind::Fxp => false,
        };
        let compress = direction && rules.mode_z.as_ref().is_some_and(|except| !except.iter().any(|m| m.matches(name)));
        DataOptions { ascii, compress }
    }

    /// Target path with the case of the options on its last name.
    fn cased(&self, item: &QueueItem, path: &str) -> String {
        let own = self.target_rules(item);
        let case = match item.kind() {
            TransferKind::Download => own.and_then(|r| r.download_case).unwrap_or(self.opts.rules.download_case),
            TransferKind::Upload => own.and_then(|r| r.upload_case).unwrap_or(self.opts.rules.upload_case),
            TransferKind::Fxp => own.and_then(|r| r.fxp_case).unwrap_or(self.opts.rules.fxp_case),
        };
        if case == NameCase::Keep {
            return path.to_string();
        }
        match &item.dst.site {
            None => {
                let p = Path::new(path);
                let name = p.file_name().map(|n| case.apply(&n.to_string_lossy())).unwrap_or_default();
                p.with_file_name(name).to_string_lossy().into_owned()
            }
            Some(_) => {
                let (dir, name) = rpath::split(path);
                rpath::join(dir.unwrap_or("/"), &case.apply(name))
            }
        }
    }

    /// Speed limiters of a transfer: none for a site of the local network when asked.
    fn limiters(&self, site: Option<&str>, limit: &Arc<RateLimiter>) -> Vec<Arc<RateLimiter>> {
        let local =
            self.opts.rules.unlimited_local && site.and_then(|s| self.sites.get(s)).is_some_and(|p| is_local_network(&p.profile.host));
        if local { Vec::new() } else { vec![limit.clone()] }
    }

    fn crc_result(&self, item: &QueueItem, path: &str, ok: bool) {
        self.emit(EngineEvent::CrcChecked { id: item.id, path: path.to_string(), ok });
    }

    /// SFV check of a download: the file against the SFV of its folder, or, for an SFV
    /// file, the files of the folder already downloaded. A bad file is removed and queued
    /// again when asked (the item itself fails, so that its retries download it again).
    async fn check_download(&self, item: &QueueItem, local: &Path) -> Res<()> {
        use urftp_core::checksum::{expected_crc, file_crc32, is_sfv, parse_sfv};
        let name = local.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let readd = self.opts.rules.readd_bad_crc;
        if is_sfv(&name) {
            let text = tokio::fs::read(local).await?;
            let dir = local.parent().map(Path::to_path_buf).unwrap_or_default();
            for (listed, crc) in parse_sfv(&String::from_utf8_lossy(&text)) {
                let file = dir.join(&listed);
                // Windows names: the case of the SFV may differ from the file's.
                let Some(file) = find_case_insensitive(&dir, &listed).or_else(|| file.exists().then_some(file)) else { continue };
                let path = file.clone();
                let actual = tokio::task::spawn_blocking(move || file_crc32(&path)).await.map_err(std::io::Error::other)??;
                let ok = actual == crc;
                let shown = file.to_string_lossy().into_owned();
                self.note_sfv(&dir, Some((&listed, ok)));
                let owner = self.queue.lock().items().iter().find(|i| i.dst.path == shown).map(|i| i.id);
                if let Some(id) = owner {
                    self.emit(EngineEvent::CrcChecked { id, path: shown.clone(), ok });
                    if !ok && readd {
                        let _ = tokio::fs::remove_file(&file).await;
                        let mut q = self.queue.lock();
                        if let Some(i) = q.get_mut(id).filter(|i| i.state != ItemState::Running) {
                            i.state = ItemState::Waiting;
                            i.attempts = 0;
                        }
                    }
                }
            }
            // The SFV came first: the status folder shows what is still to come.
            self.note_sfv(&dir, None);
            return Ok(());
        }
        let Some(crc) = expected_crc(local) else { return Ok(()) };
        let path = local.to_path_buf();
        let actual = tokio::task::spawn_blocking(move || file_crc32(&path)).await.map_err(std::io::Error::other)??;
        let shown = local.to_string_lossy().into_owned();
        if let Some(dir) = local.parent() {
            self.note_sfv(dir, Some((&name, actual == crc)));
        }
        self.crc_result(item, &shown, actual == crc);
        if actual != crc && readd {
            let _ = tokio::fs::remove_file(local).await;
            return Err(SessionError::BadCrc(shown));
        }
        Ok(())
    }

    /// Records an SFV result of a file of `dir` and writes the status folder of `dir`
    /// (options).
    fn note_sfv(&self, dir: &Path, result: Option<(&str, bool)>) {
        if !self.opts.rules.sfv_status_dir {
            return;
        }
        let Some(listed) = urftp_core::checksum::sfv_listed(dir) else { return };
        let (ok, bad) = {
            let mut all = self.sfv_results.lock();
            let (good, wrong) = all.entry(dir.to_path_buf()).or_default();
            if let Some((name, passed)) = result {
                let name = name.to_lowercase();
                if passed {
                    wrong.remove(&name);
                    good.insert(name);
                } else {
                    good.remove(&name);
                    wrong.insert(name);
                }
            }
            (listed.intersection(good).count(), listed.intersection(wrong).count())
        };
        if let Err(e) = urftp_core::checksum::write_sfv_status(dir, ok, bad, listed.len())
            && let Some(log) = &self.opts.log
        {
            (log.0)(crate::ftp::LogLine::Status(format!("SFV status folder of {}: {e}", dir.display())));
        }
    }

    /// Is this name a dupe announced by the target folder?
    fn is_dupe(&self, site: &str, path: &str) -> bool {
        let (dir, name) = rpath::split(path);
        let key = (site.to_string(), dir.unwrap_or("/").to_string());
        self.dupes.lock().get(&key).is_some_and(|names| names.contains(&name.to_lowercase()))
    }

    /// Records the dupes listed in an upload refusal (`553- X-DUPE: name`); `true` when the
    /// refusal is one.
    fn record_dupes(&self, site: &str, path: &str, e: &SessionError) -> bool {
        let SessionError::Ftp(FtpError::Unexpected { reply, .. }) = e else { return false };
        let names: Vec<String> = reply
            .lines
            .iter()
            .filter_map(|l| l.split_once("X-DUPE:").map(|(_, n)| n.trim().to_lowercase()))
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            return false;
        }
        let dir = rpath::split(path).0.unwrap_or("/").to_string();
        self.dupes.lock().entry((site.to_string(), dir)).or_default().extend(names);
        true
    }

    fn skip_reason(&self, item: &QueueItem, name: &str, is_dir: bool, size: Option<u64>) -> Option<SkipReason> {
        if item.options.as_ref().is_some_and(|o| o.no_skiplist) {
            return None;
        }
        self.skiplist(item).check_as(&Candidate { name, full_path: &item.src.path, is_dir, size, kind: item.kind() }, item.chosen)
    }

    fn progress(&self, id: ItemId, bytes: u64, total: Option<u64>) {
        self.emit(EngineEvent::Progress { id, bytes, total });
    }

    // ------------------------------------------------------------------ files

    async fn transfer(&self, item: &QueueItem) -> Res<Outcome> {
        match item.kind() {
            TransferKind::Download => self.download(item).await,
            TransferKind::Upload => self.upload(item).await,
            TransferKind::Fxp => self.fxp_file(item).await,
        }
    }

    async fn download(&self, item: &QueueItem) -> Res<Outcome> {
        let pool = self.pool(item.src.site.as_deref().unwrap_or_default())?;
        let mut s = pool.acquire(Account::Download).await?;
        let r = self.download_with(item, &mut s).await;
        pool.release(Account::Download, s, &r);
        r
    }

    async fn download_with(&self, item: &QueueItem, s: &mut RemoteSession) -> Res<Outcome> {
        let src = &item.src.path;
        let size = match item.size {
            Some(n) => Some(n),
            None => s.size_if_exists(src).await?,
        };
        if let Some(r) = self.skip_reason(item, rpath::split(src).1, false, size) {
            return Ok(Outcome::Skipped(r));
        }
        let mut local = PathBuf::from(self.cased(item, &item.dst.path));
        let target = tokio::fs::metadata(&local).await.ok().map(|m| m.len());
        let data = self.data_options(item, rpath::split(src).1);
        let overwrite_if_resume_fails = self.restart_if_resume_fails(item);
        let mut offset = match self.decide(item, size, target) {
            ExistsDecision::Transfer { offset } => offset,
            ExistsDecision::Skip => return Ok(Outcome::Skipped(SkipReason::AlreadyPresent)),
            ExistsDecision::Ask => return Ok(Outcome::Ask),
            ExistsDecision::Rename => {
                local = free_local_name(&local).await?;
                0
            }
        };
        if data.ascii && self.opts.rules.no_resume_ascii {
            offset = 0;
        }
        // Roll back: the end of a partial file may be damaged.
        offset = offset.saturating_sub(self.opts.rules.resume_rollback);
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if self.opts.rules.keep_free > 0
            && let Some(free) = urftp_core::localpath::free_space(&local)
        {
            let needed = size.map_or(0, |n| n.saturating_sub(target.unwrap_or(0).min(offset)));
            if free < needed.saturating_add(self.opts.rules.keep_free) {
                let e = DiskFull { needed, free, keep: self.opts.rules.keep_free };
                return Err(SessionError::Local(std::io::Error::other(e)));
            }
        }
        loop {
            let mut file = open_local_for_write(&local, offset).await?;
            let mut w = Throttled::new(&mut file, self.limiters(item.src.site.as_deref(), &self.opts.download_limit));
            let id = item.id;
            let r = s.retr_with(src, offset, &mut w, |n| self.progress(id, offset + n, size), data).await;
            match r {
                Err(e) if offset > 0 && e.is_resume_refused() && overwrite_if_resume_fails => offset = 0,
                Err(e) => return Err(e),
                Ok(n) => {
                    file.flush().await?;
                    drop(file);
                    if self.opts.rules.keep_server_date
                        && let Some(t) = s.modified(src).await?
                    {
                        set_local_date(&local, t.epoch(FtpTime::now_epoch()));
                    }
                    if self.opts.rules.check_sfv {
                        self.check_download(item, &local).await?;
                    }
                    return Ok(Outcome::Done(offset + n));
                }
            }
        }
    }

    async fn upload(&self, item: &QueueItem) -> Res<Outcome> {
        let pool = self.pool(item.dst.site.as_deref().unwrap_or_default())?;
        let mut s = pool.acquire(Account::Upload).await?;
        let r = self.upload_with(item, &mut s).await;
        pool.release(Account::Upload, s, &r);
        r
    }

    async fn upload_with(&self, item: &QueueItem, s: &mut RemoteSession) -> Res<Outcome> {
        let local = Path::new(&item.src.path);
        let size = tokio::fs::metadata(local).await?.len();
        let name = local.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(r) = self.skip_reason(item, &name, false, Some(size)) {
            return Ok(Outcome::Skipped(r));
        }
        let mut dst = self.cased(item, &item.dst.path);
        let site = item.dst.site.clone().unwrap_or_default();
        if self.is_dupe(&site, &dst) {
            return Ok(Outcome::Skipped(SkipReason::Dupe));
        }
        let target = s.size_if_exists(&dst).await?;
        let overwrite_if_resume_fails = self.restart_if_resume_fails(item);
        let data = self.data_options(item, &name);
        let mut offset = match self.decide(item, Some(size), target) {
            // ASCII: the sizes differ with the line ends, resuming would damage the file.
            ExistsDecision::Transfer { .. } if data.ascii && self.opts.rules.no_resume_ascii => 0,
            ExistsDecision::Transfer { offset } => offset,
            ExistsDecision::Skip => return Ok(Outcome::Skipped(SkipReason::AlreadyPresent)),
            ExistsDecision::Ask => return Ok(Outcome::Ask),
            ExistsDecision::Rename => {
                let (dir, n) = rpath::split(&dst);
                let dir = dir.unwrap_or("/").to_string();
                let free = s.free_name(&dir, n).await?;
                dst = rpath::join(&dir, &free);
                0
            }
        };
        loop {
            let mut file = tokio::fs::File::open(local).await?;
            if offset > 0 {
                file.seek(std::io::SeekFrom::Start(offset)).await?;
            }
            let mut r = Throttled::new(file, self.limiters(item.dst.site.as_deref(), &self.opts.upload_limit));
            let id = item.id;
            match s.stor_with(&dst, offset, &mut r, |n| self.progress(id, offset + n, Some(size)), data).await {
                Err(e) if offset > 0 && e.is_resume_refused() && overwrite_if_resume_fails => offset = 0,
                Err(e) if self.record_dupes(&site, &dst, &e) => return Ok(Outcome::Skipped(SkipReason::Dupe)),
                Err(e) if offset == 0 && self.dupe_after_refresh(item, s, &dst).await => {
                    drop(e);
                    return Ok(Outcome::Skipped(SkipReason::Dupe));
                }
                Err(e) => return Err(e),
                Ok(n) => {
                    if self.opts.rules.use_xcrc
                        && let Some(remote) = s.xcrc(&dst).await?
                    {
                        let path = local.to_path_buf();
                        let mine = tokio::task::spawn_blocking(move || urftp_core::checksum::file_crc32(&path))
                            .await
                            .map_err(std::io::Error::other)??;
                        self.crc_result(item, &dst, mine == remote);
                        if mine != remote {
                            // The bad copy goes away: the retry sends the file again from zero.
                            let _ = s.delete_file(&dst).await;
                            return Err(SessionError::BadCrc(dst));
                        }
                    }
                    return Ok(Outcome::Done(offset + n));
                }
            }
        }
    }

    async fn fxp_file(&self, item: &QueueItem) -> Res<Outcome> {
        let sp = self.pool(item.src.site.as_deref().unwrap_or_default())?;
        let dp = self.pool(item.dst.site.as_deref().unwrap_or_default())?;
        let mut a = sp.acquire(Account::Download).await?;
        let mut b = match dp.acquire(Account::Upload).await {
            Ok(b) => b,
            Err(e) => {
                sp.release(Account::Download, a, &Ok::<(), SessionError>(()));
                return Err(e);
            }
        };
        let r = self.fxp_with(item, &mut a, &mut b).await;
        sp.release(Account::Download, a, &r);
        dp.release(Account::Upload, b, &r);
        r
    }

    async fn fxp_with(&self, item: &QueueItem, a: &mut RemoteSession, b: &mut RemoteSession) -> Res<Outcome> {
        let size = match item.size {
            Some(n) => Some(n),
            None => a.size_if_exists(&item.src.path).await?,
        };
        if let Some(r) = self.skip_reason(item, rpath::split(&item.src.path).1, false, size) {
            return Ok(Outcome::Skipped(r));
        }
        let mut dst = self.cased(item, &item.dst.path);
        let site = item.dst.site.clone().unwrap_or_default();
        if self.is_dupe(&site, &dst) {
            return Ok(Outcome::Skipped(SkipReason::Dupe));
        }
        let target = b.size_if_exists(&dst).await?;
        let offset = match self.decide(item, size, target) {
            ExistsDecision::Transfer { offset } => offset,
            ExistsDecision::Skip => return Ok(Outcome::Skipped(SkipReason::AlreadyPresent)),
            ExistsDecision::Ask => return Ok(Outcome::Ask),
            ExistsDecision::Rename => {
                let (dir, n) = rpath::split(&dst);
                let dir = dir.unwrap_or("/").to_string();
                let free = b.free_name(&dir, n).await?;
                dst = rpath::join(&dir, &free);
                0
            }
        };
        let (Some(src), Some(dstc)) = (a.as_ftp(), b.as_ftp()) else {
            return Err(SessionError::FxpNeedsFtp);
        };
        let mut opts = FxpOptions { offset, ..self.opts.fxp.clone() };
        let rules = |site: &Option<String>| site.as_deref().and_then(|s| self.sites.get(s)).map(|p| &p.profile.rules);
        // Encrypted FXP: a site that refuses it wins, then one that asks for it.
        let tls = [rules(&item.src.site), rules(&item.dst.site)].into_iter().flatten().filter_map(|r| r.fxp_tls).min();
        match tls {
            Some(false) => opts.security = crate::ftp::FxpSecurity::Clear,
            Some(true) if opts.security == crate::ftp::FxpSecurity::Clear => opts.security = crate::ftp::FxpSecurity::Auto,
            _ => {}
        }
        let source_active = rules(&item.src.site).is_some_and(|r| r.fxp_source_active);
        let target_passive = rules(&item.dst.site).is_some_and(|r| r.fxp_target_passive);
        if source_active || target_passive {
            opts.mode = crate::ftp::FxpMode::DestinationPassive;
        }
        if let Err(e) = fxp(src, &item.src.path, dstc, &dst, &opts).await {
            let e = SessionError::from(e);
            if self.record_dupes(&site, &dst, &e) {
                return Ok(Outcome::Skipped(SkipReason::Dupe));
            }
            // The target opened its file before the source refused: an empty file is left.
            if self.opts.rules.fxp_remove_zero_target && offset == 0 && b.size_if_exists(&dst).await.ok().flatten() == Some(0) {
                let _ = b.delete_file(&dst).await;
            }
            return Err(e);
        }
        if self.opts.rules.use_xcrc
            && let (Some(x), Some(y)) = (a.xcrc(&item.src.path).await?, b.xcrc(&dst).await?)
        {
            self.crc_result(item, &dst, x == y);
            if x != y {
                let _ = b.delete_file(&dst).await;
                return Err(SessionError::BadCrc(dst));
            }
        }
        self.progress(item.id, size.unwrap_or(0), size);
        Ok(Outcome::Done(size.unwrap_or(0)))
    }

    // ------------------------------------------------------------------ folders

    async fn expand(&self, item: &QueueItem) -> Res<usize> {
        let kind = item.kind();
        let options = item.options.as_deref().cloned().unwrap_or_default();
        if options.refresh {
            // A refresh lists the folder again a little later, to catch new entries.
            tokio::time::sleep(self.opts.retry_delay).await;
        }
        let src_name = match kind {
            TransferKind::Upload => Path::new(&item.src.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            _ => rpath::split(&item.src.path).1.to_string(),
        };
        if !options.refresh
            && let Some(r) = self.skip_reason(item, &src_name, true, None)
        {
            self.queue.lock().set_state(item.id, ItemState::Skipped(r));
            return Ok(0);
        }

        // Target folder, with the case of the options.
        let dst_dir = Endpoint { site: item.dst.site.clone(), path: self.cased(item, &item.dst.path) };
        // Contents of the source.
        let entries = self.list_endpoint(&item.src, Account::Download).await?;
        let complete = options.is_complete(&entries);
        // Synchronisation: what the target already has (nothing when it does not exist yet).
        let target: Option<Vec<FolderEntry>> = match options.sync {
            Some(_) => Some(self.list_endpoint(&dst_dir, Account::Upload).await.unwrap_or_default()),
            None => None,
        };
        let target_entry = |name: &str| target.as_ref().and_then(|t| t.iter().find(|e| e.name == name));

        let mut children = Vec::new();
        for e in options.arrange(entries.clone()) {
            if e.is_dir && self.opts.rules.refuse_same_name_dir && e.name.eq_ignore_ascii_case(&src_name) {
                continue;
            }
            let child_src = join_endpoint(&item.src, &e.name);
            let probe = Candidate { name: &e.name, full_path: &child_src.path, is_dir: e.is_dir, size: e.size, kind };
            if !options.no_skiplist && self.skiplist(item).check_as(&probe, false).is_some() {
                continue;
            }
            if let Some(sync) = options.sync
                && !sync.transfers(&e, target_entry(&e.name))
            {
                continue;
            }
            let child_dst = join_endpoint(&dst_dir, &e.name);
            // Already queued (a refresh, or the same folder queued twice): not again.
            if self.queue.lock().contains_pending(&child_src, &child_dst) {
                continue;
            }
            let prio = if e.is_dir { None } else { self.priorities(item).rank(&e.name) };
            children.push((child_src, child_dst, e.is_dir, e.size, prio));
        }
        // Synchronisation: what only the target has is removed, or copied back.
        let mut back = Vec::new();
        if let (Some(sync), Some(target)) = (options.sync, &target) {
            let missing: Vec<&FolderEntry> =
                target.iter().filter(|t| options.filter.allows(&t.name, t.is_dir) && !source_has(&item.src, &t.name, &entries)).collect();
            for t in missing {
                if sync.delete_missing {
                    let path = join_endpoint(&dst_dir, &t.name);
                    self.delete_endpoint(&path, t.is_dir).await?;
                    self.emit(EngineEvent::SyncDeleted { id: item.id, path: path.path });
                } else if sync.both_sides {
                    back.push((join_endpoint(&dst_dir, &t.name), join_endpoint(&item.src, &t.name), t.is_dir, t.size, None));
                }
            }
        }
        // Several SFV files in the folder: none is taken (they would disagree).
        if self.opts.rules.one_sfv_per_folder
            && children.iter().filter(|c| !c.2 && urftp_core::checksum::is_sfv(&endpoint_name(&c.0))).count() > 1
        {
            children.retain(|c| c.2 || !urftp_core::checksum::is_sfv(&endpoint_name(&c.0)));
        }
        let refresh = (options.repeat > 0 && !complete).then(|| options.for_refresh());
        if children.is_empty() && refresh.is_none() && self.skiplist(item).skip_empty_dirs && !options.refresh {
            return Ok(0);
        }

        // Target folder (its sub-folders are entered, not created again, when they exist).
        if !options.refresh {
            match &item.dst.site {
                None => tokio::fs::create_dir_all(&dst_dir.path).await?,
                Some(site) => {
                    let pool = self.pool(site)?;
                    let mut s = pool.acquire(Account::Upload).await?;
                    let r = s.ensure_dir(&dst_dir.path).await;
                    pool.release(Account::Upload, s, &r);
                    r?;
                }
            }
        }
        let n = children.len();
        let contents = item.options.as_ref().map(|_| Arc::new(options.for_contents()));
        let mut q = self.queue.lock();
        let mut ids = q.insert_children_with(item.id, children, contents.clone());
        if !back.is_empty() {
            let after = ids.last().copied().unwrap_or(item.id);
            ids.extend(q.insert_children_with(after, back, contents));
        }
        // The refresh of the folder goes after its contents.
        if let Some(refresh) = refresh {
            let after = ids.last().copied().unwrap_or(item.id);
            let refresh_item = (item.src.clone(), dst_dir.clone(), true, None, None);
            q.insert_children_with(after, vec![refresh_item], Some(Arc::new(refresh)));
        }
        Ok(n)
    }
}

impl Engine {
    /// Entries of a folder, local or remote; dates in seconds since 1970.
    async fn list_endpoint(&self, at: &Endpoint, account: Account) -> Res<Vec<FolderEntry>> {
        Ok(match &at.site {
            None => {
                let mut rd = tokio::fs::read_dir(&at.path).await?;
                let mut v = Vec::new();
                while let Some(e) = rd.next_entry().await? {
                    let meta = e.metadata().await?;
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .and_then(|d| i64::try_from(d.as_secs()).ok());
                    let is_dir = meta.is_dir();
                    v.push(FolderEntry { name: e.file_name().to_string_lossy().into_owned(), is_dir, size: Some(meta.len()), modified });
                }
                v
            }
            Some(site) => {
                let pool = self.pool(site)?;
                let mut s = pool.acquire(account).await?;
                let r = s.list(&at.path).await;
                pool.release(account, s, &r);
                let now = FtpTime::now_epoch();
                r?.into_iter()
                    .map(|e| {
                        let is_dir = e.kind == EntryKind::Dir;
                        let modified = e.modified.map(|t| t.epoch(now));
                        FolderEntry { size: (!is_dir).then_some(e.size), modified, name: e.name, is_dir }
                    })
                    .collect()
            }
        })
    }

    /// Removes a file or a folder (with its contents) of the target (synchronisation).
    async fn delete_endpoint(&self, at: &Endpoint, is_dir: bool) -> Res<()> {
        match &at.site {
            None if is_dir => tokio::fs::remove_dir_all(&at.path).await?,
            None => tokio::fs::remove_file(&at.path).await?,
            Some(site) => {
                let pool = self.pool(site)?;
                let mut s = pool.acquire(Account::Upload).await?;
                let r = if is_dir { s.remove_tree(&at.path).await.map(|_| ()) } else { s.delete_file(&at.path).await };
                pool.release(Account::Upload, s, &r);
                r?;
            }
        }
        Ok(())
    }
}

/// Does the source listing have this name? (Names of a local source are compared without
/// case, as Windows does.)
fn source_has(src: &Endpoint, name: &str, entries: &[FolderEntry]) -> bool {
    if src.is_local() { entries.iter().any(|e| e.name.eq_ignore_ascii_case(name)) } else { entries.iter().any(|e| e.name == name) }
}

/// Sets the modification date of a local file (seconds since 1970); errors are ignored.
fn set_local_date(path: &Path, epoch: i64) {
    let Ok(secs) = u64::try_from(epoch) else { return };
    let when = std::time::UNIX_EPOCH + Duration::from_secs(secs);
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(path) {
        let _ = f.set_modified(when);
    }
}

/// Last name of an endpoint.
fn endpoint_name(e: &Endpoint) -> String {
    match e.site {
        None => Path::new(&e.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        Some(_) => rpath::split(&e.path).1.to_string(),
    }
}

/// The file of `dir` named `name` whatever its case.
fn find_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)).map(|e| e.path())
}

/// Is this host on the local network (loopback, private or link-local address, `localhost`)?
fn is_local_network(host: &str) -> bool {
    use std::net::IpAddr;
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(IpAddr::V6(ip)) => {
            let first = ip.segments()[0];
            ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
        Err(_) => false,
    }
}

fn join_endpoint(e: &Endpoint, name: &str) -> Endpoint {
    let path = match e.site {
        None => Path::new(&e.path).join(name).to_string_lossy().into_owned(),
        Some(_) => rpath::join(&e.path, name),
    };
    Endpoint { site: e.site.clone(), path }
}

/// Opens the local file for writing from `offset` (truncated at `offset`).
async fn open_local_for_write(p: &Path, offset: u64) -> std::io::Result<tokio::fs::File> {
    let mut f = tokio::fs::OpenOptions::new().create(true).write(true).truncate(offset == 0).open(p).await?;
    if offset > 0 {
        f.set_len(offset).await?;
        f.seek(std::io::SeekFrom::Start(offset)).await?;
    }
    Ok(f)
}

async fn free_local_name(p: &Path) -> std::io::Result<PathBuf> {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    for n in 1..1000 {
        let candidate = p.with_file_name(urftp_core::exists::renamed(&name, n));
        if tokio::fs::metadata(&candidate).await.is_err() {
            return Ok(candidate);
        }
    }
    Err(std::io::Error::other(format!("no free name left for {}", p.display())))
}
