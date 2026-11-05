//! Moteur de file d'attente : exécute les éléments d'une [`Queue`] en parallèle.
//!
//! - **Multi-transfert** : plusieurs fichiers vers ou depuis le même site en même temps,
//!   sur des connexions distinctes, dans la limite de [`SiteLimits`] (0 = connexion unique).
//! - **Réservoir de connexions** par site : une connexion libérée sert au transfert suivant.
//! - **Dossiers** : un dossier est développé à son tour (liste, skiplist, priorités) et son
//!   contenu prend sa place dans la file.
//! - **Règles** : skiplist, « fichier existant » (reprendre / ignorer / renommer / écraser /
//!   demander), repli sans reprise si `REST` est refusé, nouvelles tentatives.
//! - **Arrêt** d'un élément : sa tâche est interrompue et ses connexions fermées.
//! - **Débit** limité pour les envois et les téléchargements.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use std::time::Duration;

use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::{AbortHandle, JoinSet};

use urftp_core::TransferKind;
use urftp_core::exists::{ExistsDecision, ExistsRules};
use urftp_core::priority::PriorityList;
use urftp_core::queue::{Endpoint, ItemId, ItemState, Load, Queue, QueueItem, SiteLimits};
use urftp_core::skiplist::{Candidate, SkipList};

use crate::ftp::{EntryKind, FxpOptions, fxp, path as rpath};
use crate::session::{RemoteSession, SessionError, SiteProfile};
use crate::throttle::{RateLimiter, Throttled};

/// Réglages du moteur.
#[derive(Debug, Clone)]
pub struct EngineOptions {
    /// Nouvelles tentatives après un échec (« retry count » de la v2).
    pub retries: u32,
    pub retry_delay: Duration,
    /// Transferts simultanés tous sites confondus (`None` : seulement les limites des sites).
    pub max_total: Option<u32>,
    pub skiplist: SkipList,
    pub priorities: PriorityList,
    pub exists: ExistsRules,
    pub fxp: FxpOptions,
    /// Débit des téléchargements (serveur → local), partagé par tous les transferts.
    pub download_limit: Arc<RateLimiter>,
    /// Débit des envois (local → serveur).
    pub upload_limit: Arc<RateLimiter>,
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
        }
    }
}

/// Événements pour l'interface (file d'attente, barre d'état, journal des transferts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    Started {
        id: ItemId,
    },
    /// `bytes` : position dans le fichier (reprise comprise).
    Progress {
        id: ItemId,
        bytes: u64,
        total: Option<u64>,
    },
    Done {
        id: ItemId,
        bytes: u64,
    },
    /// Dossier développé en `children` éléments.
    Expanded {
        id: ItemId,
        children: usize,
    },
    Skipped {
        id: ItemId,
        reason: String,
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

/// Connexions d'un site.
struct SitePool {
    profile: SiteProfile,
    idle: Mutex<Vec<RemoteSession>>,
}

impl SitePool {
    async fn acquire(&self) -> Result<RemoteSession, SessionError> {
        let idle = self.idle.lock().pop();
        match idle {
            Some(s) => Ok(s),
            None => RemoteSession::open(&self.profile, None).await,
        }
    }

    /// Rend la connexion au réservoir si elle est encore saine.
    fn release<T>(&self, session: RemoteSession, result: &Result<T, SessionError>) {
        if result.as_ref().err().is_none_or(SessionError::keeps_session) {
            self.idle.lock().push(session);
        }
    }
}

enum Outcome {
    Done(u64),
    Expanded(usize),
    Skipped(String),
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
        let sites = sites.into_iter().map(|p| (p.name.clone(), Arc::new(SitePool { profile: p, idle: Mutex::new(Vec::new()) }))).collect();
        Arc::new(Self { queue, sites, opts, events, load: Mutex::default(), running: Mutex::default() })
    }

    pub fn queue(&self) -> Arc<Mutex<Queue>> {
        self.queue.clone()
    }

    /// Connexions inactives gardées pour `site` (pour l'affichage / les tests).
    pub fn idle_connections(&self, site: &str) -> usize {
        self.sites.get(site).map_or(0, |p| p.idle.lock().len())
    }

    /// Arrête un élément en cours : ses connexions sont fermées, il passe à l'état `Stopped`.
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

    /// Traite la file jusqu'à ce qu'il ne reste rien à lancer.
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
                    // La tâche a paniqué : on ne laisse pas l'élément bloqué.
                    q.set_state(id, ItemState::Failed("erreur interne".into()));
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
                if attempt <= self.opts.retries {
                    self.emit(EngineEvent::Retrying { id, attempt, error });
                    tokio::time::sleep(self.opts.retry_delay).await;
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

    fn skip_reason(&self, item: &QueueItem, name: &str, is_dir: bool, size: Option<u64>) -> Option<String> {
        self.opts.skiplist.check(&Candidate { name, full_path: &item.src.path, is_dir, size, kind: item.kind() })
    }

    fn progress(&self, id: ItemId, bytes: u64, total: Option<u64>) {
        self.emit(EngineEvent::Progress { id, bytes, total });
    }

    // ------------------------------------------------------------------ fichiers

    async fn transfer(&self, item: &QueueItem) -> Res<Outcome> {
        match item.kind() {
            TransferKind::Download => self.download(item).await,
            TransferKind::Upload => self.upload(item).await,
            TransferKind::Fxp => self.fxp_file(item).await,
        }
    }

    async fn download(&self, item: &QueueItem) -> Res<Outcome> {
        let pool = self.pool(item.src.site.as_deref().unwrap_or_default())?;
        let mut s = pool.acquire().await?;
        let r = self.download_with(item, &mut s).await;
        pool.release(s, &r);
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
        let mut offset = match self.opts.exists.decide(size, target) {
            ExistsDecision::Transfer { offset } => offset,
            ExistsDecision::Skip => return Ok(Outcome::Skipped("déjà présent".into())),
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
                Err(e) if offset > 0 && e.is_resume_refused() && self.opts.exists.overwrite_if_resume_fails => offset = 0,
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
        let mut s = pool.acquire().await?;
        let r = self.upload_with(item, &mut s).await;
        pool.release(s, &r);
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
        let mut offset = match self.opts.exists.decide(Some(size), target) {
            ExistsDecision::Transfer { offset } => offset,
            ExistsDecision::Skip => return Ok(Outcome::Skipped("déjà présent".into())),
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
                Err(e) if offset > 0 && e.is_resume_refused() && self.opts.exists.overwrite_if_resume_fails => offset = 0,
                Err(e) => return Err(e),
                Ok(n) => return Ok(Outcome::Done(offset + n)),
            }
        }
    }

    async fn fxp_file(&self, item: &QueueItem) -> Res<Outcome> {
        let sp = self.pool(item.src.site.as_deref().unwrap_or_default())?;
        let dp = self.pool(item.dst.site.as_deref().unwrap_or_default())?;
        let mut a = sp.acquire().await?;
        let mut b = match dp.acquire().await {
            Ok(b) => b,
            Err(e) => {
                sp.release(a, &Ok::<(), SessionError>(()));
                return Err(e);
            }
        };
        let r = self.fxp_with(item, &mut a, &mut b).await;
        sp.release(a, &r);
        dp.release(b, &r);
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
        let offset = match self.opts.exists.decide(size, target) {
            ExistsDecision::Transfer { offset } => offset,
            ExistsDecision::Skip => return Ok(Outcome::Skipped("déjà présent".into())),
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
        let opts = FxpOptions { offset, ..self.opts.fxp.clone() };
        fxp(src, &item.src.path, dstc, &dst, &opts).await?;
        self.progress(item.id, size.unwrap_or(0), size);
        Ok(Outcome::Done(size.unwrap_or(0)))
    }

    // ------------------------------------------------------------------ dossiers

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

        // Contenu de la source : (nom, dossier ?, taille).
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
                let mut s = pool.acquire().await?;
                let r = s.list(&item.src.path).await;
                pool.release(s, &r);
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
            if self.opts.skiplist.check(&probe).is_some() {
                continue;
            }
            let prio = if is_dir { None } else { self.opts.priorities.rank(&name) };
            children.push((child_src, join_endpoint(&item.dst, &name), is_dir, size, prio));
        }
        if children.is_empty() && self.opts.skiplist.skip_empty_dirs {
            return Ok(0);
        }

        // Dossier cible.
        match &item.dst.site {
            None => tokio::fs::create_dir_all(&item.dst.path).await?,
            Some(site) => {
                let pool = self.pool(site)?;
                let mut s = pool.acquire().await?;
                let r = s.ensure_dir(&item.dst.path).await;
                pool.release(s, &r);
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

/// Ouvre le fichier local pour écrire à partir de `offset` (troncature à `offset`).
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
    Err(std::io::Error::other(format!("aucun nom libre pour {}", p.display())))
}
