//! Queue engine: runs the items of a [`Queue`] in parallel.
//!
//! - **Multi-transfer**: several files to or from the same site at the same time, on
//!   separate connections, within [`SiteLimits`] (0 = single connection).
//! - **Connection pool** per site: a released connection serves the next transfer.
//! - **Folders**: a folder is expanded in turn (listing, skip list, priorities) and its
//!   contents take its place in the queue.
//! - **Rules**: skip list, "existing file" (resume / skip / rename / overwrite / ask),
//!   fallback without resume when `REST` is refused, retries.
//! - **Stopping** an item: its task is aborted and its connections closed.
//! - **Speed** limited for uploads and downloads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use std::time::{Duration, Instant};

use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::{AbortHandle, JoinSet};

use urftp_core::TransferKind;
use urftp_core::exists::{ExistsDecision, ExistsRules};
use urftp_core::priority::PriorityList;
use urftp_core::queue::{Endpoint, ItemId, ItemState, Load, Queue, QueueItem, SiteLimits};
use urftp_core::skiplist::{Candidate, SkipList, SkipReason};

use crate::ftp::{EntryKind, FxpOptions, LogSink, fxp, path as rpath};
use crate::session::{Account, RemoteSession, SessionError, SiteProfile, SiteRules};
use crate::throttle::{RateLimiter, Throttled};

/// Engine settings.
#[derive(Debug, Clone)]
pub struct EngineOptions {
    /// Retries after a failure ("retry count" in v2).
    pub retries: u32,
    pub retry_delay: Duration,
    /// Simultaneous transfers across all sites (`None`: only the site limits).
    pub max_total: Option<u32>,
    pub skiplist: SkipList,
    pub priorities: PriorityList,
    pub exists: ExistsRules,
    pub fxp: FxpOptions,
    /// Download speed (server → local), shared by every transfer.
    pub download_limit: Arc<RateLimiter>,
    /// Upload speed (local → server).
    pub upload_limit: Arc<RateLimiter>,
    /// Protocol log of the transfer connections (`None`: not logged).
    pub log: Option<TransferLog>,
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
            retry_delay: Duration::from_secs(3),
            max_total: None,
            skiplist: SkipList::default(),
            priorities: PriorityList::default(),
            exists: ExistsRules::default(),
            fxp: FxpOptions::default(),
            download_limit: RateLimiter::new(0),
            upload_limit: RateLimiter::new(0),
            log: None,
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
        Arc::new(Self { queue, sites, opts, events, load: Mutex::default(), running: Mutex::default() })
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

    pub fn stop_all(&self) {
        for h in self.running.lock().values() {
            h.abort();
        }
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
        self.target_rules(item).and_then(|r| r.exists).unwrap_or(self.opts.exists)
    }

    /// Retries and delay between them.
    fn retries(&self, item: &QueueItem) -> (u32, Duration) {
        let r = self.target_rules(item);
        (r.and_then(|r| r.retries).unwrap_or(self.opts.retries), r.and_then(|r| r.retry_delay).unwrap_or(self.opts.retry_delay))
    }

    /// Decision for an existing target; without resume on the target site, a partial file
    /// is transferred again from zero.
    fn decide(&self, item: &QueueItem, size: Option<u64>, target: Option<u64>) -> ExistsDecision {
        match self.exists_rules(item).decide(size, target) {
            ExistsDecision::Transfer { offset } if offset > 0 && !self.target_rules(item).is_none_or(|r| r.resume) => {
                ExistsDecision::Transfer { offset: 0 }
            }
            d => d,
        }
    }

    /// Processes the queue until nothing is left to start.
    pub async fn run(self: &Arc<Self>) {
        let mut tasks: JoinSet<()> = JoinSet::new();
        let mut by_task: HashMap<tokio::task::Id, ItemId> = HashMap::new();
        loop {
            loop {
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
                if cancelled {
                    q.set_state(id, ItemState::Stopped);
                    self.emit(EngineEvent::Stopped { id });
                } else if item.state == ItemState::Running {
                    // The task panicked: do not leave the item stuck.
                    q.set_state(id, ItemState::Failed("internal error".into()));
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
                    tokio::time::sleep(delay).await;
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

    fn skip_reason(&self, item: &QueueItem, name: &str, is_dir: bool, size: Option<u64>) -> Option<SkipReason> {
        self.skiplist(item).check(&Candidate { name, full_path: &item.src.path, is_dir, size, kind: item.kind() })
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
        let mut local = PathBuf::from(&item.dst.path);
        let target = tokio::fs::metadata(&local).await.ok().map(|m| m.len());
        let overwrite_if_resume_fails = self.exists_rules(item).overwrite_if_resume_fails;
        let mut offset = match self.decide(item, size, target) {
            ExistsDecision::Transfer { offset } => offset,
            ExistsDecision::Skip => return Ok(Outcome::Skipped(SkipReason::AlreadyPresent)),
            ExistsDecision::Ask => return Ok(Outcome::Ask),
            ExistsDecision::Rename => {
                local = free_local_name(&local).await?;
                0
            }
        };
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        loop {
            let mut file = open_local_for_write(&local, offset).await?;
            let mut w = Throttled::new(&mut file, vec![self.opts.download_limit.clone()]);
            let id = item.id;
            let r = s.retr(src, offset, &mut w, |n| self.progress(id, offset + n, size)).await;
            match r {
                Err(e) if offset > 0 && e.is_resume_refused() && overwrite_if_resume_fails => offset = 0,
                Err(e) => return Err(e),
                Ok(n) => {
                    file.flush().await?;
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
        let mut dst = item.dst.path.clone();
        let target = s.size_if_exists(&dst).await?;
        let overwrite_if_resume_fails = self.exists_rules(item).overwrite_if_resume_fails;
        let mut offset = match self.decide(item, Some(size), target) {
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
            let mut r = Throttled::new(file, vec![self.opts.upload_limit.clone()]);
            let id = item.id;
            match s.stor(&dst, offset, &mut r, |n| self.progress(id, offset + n, Some(size))).await {
                Err(e) if offset > 0 && e.is_resume_refused() && overwrite_if_resume_fails => offset = 0,
                Err(e) => return Err(e),
                Ok(n) => return Ok(Outcome::Done(offset + n)),
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
        let mut dst = item.dst.path.clone();
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
        let source_active = rules(&item.src.site).is_some_and(|r| r.fxp_source_active);
        let target_passive = rules(&item.dst.site).is_some_and(|r| r.fxp_target_passive);
        if source_active || target_passive {
            opts.mode = crate::ftp::FxpMode::DestinationPassive;
        }
        fxp(src, &item.src.path, dstc, &dst, &opts).await?;
        self.progress(item.id, size.unwrap_or(0), size);
        Ok(Outcome::Done(size.unwrap_or(0)))
    }

    // ------------------------------------------------------------------ folders

    async fn expand(&self, item: &QueueItem) -> Res<usize> {
        let kind = item.kind();
        let src_name = match kind {
            TransferKind::Upload => Path::new(&item.src.path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            _ => rpath::split(&item.src.path).1.to_string(),
        };
        if let Some(r) = self.skip_reason(item, &src_name, true, None) {
            self.queue.lock().set_state(item.id, ItemState::Skipped(r));
            return Ok(0);
        }

        // Contents of the source: (name, folder?, size).
        let entries: Vec<(String, bool, Option<u64>)> = match &item.src.site {
            None => {
                let mut rd = tokio::fs::read_dir(&item.src.path).await?;
                let mut v = Vec::new();
                while let Some(e) = rd.next_entry().await? {
                    let meta = e.metadata().await?;
                    v.push((e.file_name().to_string_lossy().into_owned(), meta.is_dir(), Some(meta.len())));
                }
                v
            }
            Some(site) => {
                let pool = self.pool(site)?;
                let mut s = pool.acquire(Account::Download).await?;
                let r = s.list(&item.src.path).await;
                pool.release(Account::Download, s, &r);
                r?.into_iter()
                    .map(|e| {
                        let dir = e.kind == EntryKind::Dir;
                        (e.name, dir, (!dir).then_some(e.size))
                    })
                    .collect()
            }
        };

        let mut children = Vec::new();
        for (name, is_dir, size) in entries {
            let child_src = join_endpoint(&item.src, &name);
            let probe = Candidate { name: &name, full_path: &child_src.path, is_dir, size, kind };
            if self.skiplist(item).check(&probe).is_some() {
                continue;
            }
            let prio = if is_dir { None } else { self.priorities(item).rank(&name) };
            children.push((child_src, join_endpoint(&item.dst, &name), is_dir, size, prio));
        }
        if children.is_empty() && self.skiplist(item).skip_empty_dirs {
            return Ok(0);
        }

        // Target folder.
        match &item.dst.site {
            None => tokio::fs::create_dir_all(&item.dst.path).await?,
            Some(site) => {
                let pool = self.pool(site)?;
                let mut s = pool.acquire(Account::Upload).await?;
                let r = s.ensure_dir(&item.dst.path).await;
                pool.release(Account::Upload, s, &r);
                r?;
            }
        }
        let n = children.len();
        self.queue.lock().insert_children(item.id, children);
        Ok(n)
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
