//! File d'attente : éléments, ordre (priorités puis ajout), états, choix du prochain
//! élément à lancer selon les limites de chaque site. Aucune entrée-sortie ici.

use std::collections::HashMap;

use crate::TransferKind;

pub type ItemId = u64;

/// Élément à insérer : (source, destination, dossier ?, taille, rang de priorité).
pub type NewItem = (Endpoint, Endpoint, bool, Option<u64>, Option<usize>);

/// Extrémité d'un transfert : un site (par son nom) ou la machine locale.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    /// `None` = disque local.
    pub site: Option<String>,
    pub path: String,
}

impl Endpoint {
    pub fn local(path: impl Into<String>) -> Self {
        Self { site: None, path: path.into() }
    }
    pub fn remote(site: impl Into<String>, path: impl Into<String>) -> Self {
        Self { site: Some(site.into()), path: path.into() }
    }
    pub fn is_local(&self) -> bool {
        self.site.is_none()
    }
}

/// État d'un élément.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemState {
    Waiting,
    Running,
    Done,
    /// Ignoré (skiplist, fichier identique déjà présent…).
    Skipped(String),
    /// La cible existe et la règle dit « demander » : en attente d'une décision.
    NeedsConfirmation,
    /// Arrêté par l'utilisateur.
    Stopped,
    Failed(String),
}

impl ItemState {
    pub fn is_finished(&self) -> bool {
        matches!(self, ItemState::Done | ItemState::Skipped(_) | ItemState::Failed(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: ItemId,
    pub src: Endpoint,
    pub dst: Endpoint,
    pub is_dir: bool,
    /// Taille de la source si connue.
    pub size: Option<u64>,
    /// Rang dans la liste de priorités (0 = le plus prioritaire).
    pub priority: Option<usize>,
    pub state: ItemState,
    /// Tentatives déjà faites.
    pub attempts: u32,
    /// Octets transférés lors de la dernière tentative.
    pub transferred: u64,
}

impl QueueItem {
    pub fn kind(&self) -> TransferKind {
        match (self.src.is_local(), self.dst.is_local()) {
            (true, _) => TransferKind::Upload,
            (false, true) => TransferKind::Download,
            (false, false) => TransferKind::Fxp,
        }
    }

    fn rank(&self) -> usize {
        self.priority.unwrap_or(usize::MAX)
    }
}

/// Limites de connexions d'un site (onglet « Général » des propriétés du site dans la v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SiteLimits {
    /// Transferts simultanés ; 0 = mode connexion unique (un seul transfert à la fois).
    pub max_transfers: u32,
    /// Maximum de transferts où le site est source (téléchargement ou source FXP).
    pub max_downloads: Option<u32>,
    /// Maximum de transferts où le site est destination (envoi ou destination FXP).
    pub max_uploads: Option<u32>,
}

impl Default for SiteLimits {
    fn default() -> Self {
        Self { max_transfers: 2, max_downloads: None, max_uploads: None }
    }
}

impl SiteLimits {
    /// Nombre effectif de connexions de transfert (au moins une).
    pub fn effective_transfers(&self) -> u32 {
        self.max_transfers.max(1)
    }
}

/// Compteurs des transferts en cours pour un site.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SiteLoad {
    pub transfers: u32,
    pub downloads: u32,
    pub uploads: u32,
}

/// Charge en cours de tous les sites, pour décider si un élément peut démarrer.
#[derive(Debug, Clone, Default)]
pub struct Load {
    pub sites: HashMap<String, SiteLoad>,
    pub total: u32,
}

impl Load {
    fn get(&self, site: &str) -> SiteLoad {
        self.sites.get(site).copied().unwrap_or_default()
    }

    /// L'élément peut-il démarrer sans dépasser les limites ?
    pub fn allows(&self, item: &QueueItem, limits: &dyn Fn(&str) -> SiteLimits, max_total: Option<u32>) -> bool {
        if max_total.is_some_and(|m| self.total >= m) {
            return false;
        }
        if let Some(s) = &item.src.site {
            let (l, cur) = (limits(s), self.get(s));
            if cur.transfers >= l.effective_transfers() || l.max_downloads.is_some_and(|m| cur.downloads >= m) {
                return false;
            }
        }
        if let Some(d) = &item.dst.site {
            let (l, cur) = (limits(d), self.get(d));
            // Même site des deux côtés : deux connexions.
            let extra = u32::from(item.src.site.as_deref() == Some(d.as_str()));
            if cur.transfers + extra >= l.effective_transfers() || l.max_uploads.is_some_and(|m| cur.uploads >= m) {
                return false;
            }
        }
        true
    }

    pub fn start(&mut self, item: &QueueItem) {
        self.total += 1;
        if let Some(s) = &item.src.site {
            let e = self.sites.entry(s.clone()).or_default();
            e.transfers += 1;
            e.downloads += 1;
        }
        if let Some(d) = &item.dst.site {
            let e = self.sites.entry(d.clone()).or_default();
            e.transfers += 1;
            e.uploads += 1;
        }
    }

    pub fn finish(&mut self, item: &QueueItem) {
        self.total = self.total.saturating_sub(1);
        if let Some(s) = &item.src.site {
            let e = self.sites.entry(s.clone()).or_default();
            e.transfers = e.transfers.saturating_sub(1);
            e.downloads = e.downloads.saturating_sub(1);
        }
        if let Some(d) = &item.dst.site {
            let e = self.sites.entry(d.clone()).or_default();
            e.transfers = e.transfers.saturating_sub(1);
            e.uploads = e.uploads.saturating_sub(1);
        }
    }
}

/// La file d'attente d'un onglet.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    items: Vec<QueueItem>,
    next_id: ItemId,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[QueueItem] {
        &self.items
    }

    pub fn get(&self, id: ItemId) -> Option<&QueueItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn get_mut(&mut self, id: ItemId) -> Option<&mut QueueItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    fn position(&self, id: ItemId) -> Option<usize> {
        self.items.iter().position(|i| i.id == id)
    }

    fn make(&mut self, src: Endpoint, dst: Endpoint, is_dir: bool, size: Option<u64>, priority: Option<usize>) -> QueueItem {
        self.next_id += 1;
        QueueItem {
            id: self.next_id,
            src,
            dst,
            is_dir,
            size,
            priority,
            state: ItemState::Waiting,
            attempts: 0,
            transferred: 0,
        }
    }

    /// Ajoute un élément : il passe devant les éléments en attente moins prioritaires.
    pub fn push(&mut self, src: Endpoint, dst: Endpoint, is_dir: bool, size: Option<u64>, priority: Option<usize>) -> ItemId {
        let item = self.make(src, dst, is_dir, size, priority);
        let pos = self
            .items
            .iter()
            .position(|i| i.state == ItemState::Waiting && i.rank() > item.rank())
            .unwrap_or(self.items.len());
        let id = item.id;
        self.items.insert(pos, item);
        id
    }

    /// Insère le contenu d'un dossier juste après lui, trié par priorité (ordre stable).
    pub fn insert_children(&mut self, parent: ItemId, children: Vec<NewItem>) -> Vec<ItemId> {
        let mut made: Vec<QueueItem> =
            children.into_iter().map(|(s, d, dir, size, prio)| self.make(s, d, dir, size, prio)).collect();
        made.sort_by_key(QueueItem::rank);
        let ids = made.iter().map(|i| i.id).collect();
        let at = self.position(parent).map_or(self.items.len(), |p| p + 1);
        self.items.splice(at..at, made);
        ids
    }

    /// Premier élément en attente que `can_start` autorise.
    pub fn next_runnable(&self, mut can_start: impl FnMut(&QueueItem) -> bool) -> Option<ItemId> {
        self.items.iter().find(|i| i.state == ItemState::Waiting && can_start(i)).map(|i| i.id)
    }

    pub fn set_state(&mut self, id: ItemId, state: ItemState) {
        if let Some(i) = self.get_mut(id) {
            i.state = state;
        }
    }

    pub fn remove(&mut self, id: ItemId) -> Option<QueueItem> {
        self.position(id).map(|p| self.items.remove(p))
    }

    /// Supprime les éléments correspondant au filtre (sauf ceux en cours) ; renvoie leur nombre.
    pub fn remove_where(&mut self, mut pred: impl FnMut(&QueueItem) -> bool) -> usize {
        let before = self.items.len();
        self.items.retain(|i| i.state == ItemState::Running || !pred(i));
        before - self.items.len()
    }

    /// Remet en attente les éléments échoués ou arrêtés.
    pub fn reset_failed(&mut self) -> usize {
        let mut n = 0;
        for i in &mut self.items {
            if matches!(i.state, ItemState::Failed(_) | ItemState::Stopped) {
                i.state = ItemState::Waiting;
                i.attempts = 0;
                n += 1;
            }
        }
        n
    }

    pub fn move_up(&mut self, id: ItemId) {
        if let Some(p) = self.position(id).filter(|&p| p > 0) {
            self.items.swap(p, p - 1);
        }
    }

    pub fn move_down(&mut self, id: ItemId) {
        if let Some(p) = self.position(id).filter(|&p| p + 1 < self.items.len()) {
            self.items.swap(p, p + 1);
        }
    }

    pub fn move_to_top(&mut self, id: ItemId) {
        if let Some(p) = self.position(id) {
            let it = self.items.remove(p);
            self.items.insert(0, it);
        }
    }

    pub fn move_to_bottom(&mut self, id: ItemId) {
        if let Some(p) = self.position(id) {
            let it = self.items.remove(p);
            self.items.push(it);
        }
    }

    /// Reste-t-il des éléments en attente ou en cours ?
    pub fn has_pending(&self) -> bool {
        self.items.iter().any(|i| matches!(i.state, ItemState::Waiting | ItemState::Running))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up(q: &mut Queue, name: &str, prio: Option<usize>) -> ItemId {
        q.push(Endpoint::local(format!("C:/{name}")), Endpoint::remote("site", format!("/{name}")), false, None, prio)
    }

    fn names(q: &Queue) -> Vec<String> {
        q.items().iter().map(|i| i.dst.path.trim_start_matches('/').to_string()).collect()
    }

    #[test]
    fn priorities_and_children_order() {
        let mut q = Queue::new();
        up(&mut q, "a.r00", None);
        up(&mut q, "b.r01", None);
        up(&mut q, "c.nfo", Some(1));
        up(&mut q, "d.sfv", Some(0));
        assert_eq!(names(&q), ["d.sfv", "c.nfo", "a.r00", "b.r01"]);

        let dir = q.push(Endpoint::remote("s", "/rel"), Endpoint::local("C:/rel"), true, None, None);
        let kids = vec![
            (Endpoint::remote("s", "/rel/x.rar"), Endpoint::local("C:/rel/x.rar"), false, Some(1), None),
            (Endpoint::remote("s", "/rel/x.sfv"), Endpoint::local("C:/rel/x.sfv"), false, Some(1), Some(0)),
        ];
        q.insert_children(dir, kids);
        let tail: Vec<_> = q.items()[4..].iter().map(|i| i.src.path.clone()).collect();
        assert_eq!(tail, ["/rel", "/rel/x.sfv", "/rel/x.rar"]);
        assert_eq!(q.items()[5].kind(), TransferKind::Download);
    }

    #[test]
    fn limits_and_single_connection_mode() {
        let mut q = Queue::new();
        for n in ["1", "2", "3"] {
            up(&mut q, n, None);
        }
        let limits = |_: &str| SiteLimits { max_transfers: 0, ..SiteLimits::default() };
        let mut load = Load::default();
        let first = q.next_runnable(|i| load.allows(i, &limits, None)).unwrap();
        let item = q.get(first).unwrap().clone();
        load.start(&item);
        q.set_state(first, ItemState::Running);
        assert!(q.next_runnable(|i| load.allows(i, &limits, None)).is_none(), "0 = une seule connexion");
        load.finish(&item);
        q.set_state(first, ItemState::Done);
        assert!(q.next_runnable(|i| load.allows(i, &limits, None)).is_some());

        let dl_limited = |_: &str| SiteLimits { max_transfers: 5, max_downloads: Some(0), max_uploads: None };
        let fxp = q.make(Endpoint::remote("a", "/x"), Endpoint::remote("b", "/x"), false, None, None);
        assert!(!Load::default().allows(&fxp, &dl_limited, None));
        assert!(!Load { total: 4, ..Load::default() }.allows(&fxp, &|_| SiteLimits::default(), Some(4)));
    }

    #[test]
    fn moves_removal_and_reset() {
        let mut q = Queue::new();
        let a = up(&mut q, "a", None);
        let b = up(&mut q, "b", None);
        let c = up(&mut q, "c", None);
        q.move_to_top(c);
        q.move_down(a);
        assert_eq!(names(&q), ["c", "b", "a"]);
        q.move_to_bottom(c);
        q.move_up(a);
        assert_eq!(names(&q), ["a", "b", "c"]);
        q.set_state(b, ItemState::Failed("x".into()));
        q.set_state(c, ItemState::Running);
        assert_eq!(q.reset_failed(), 1);
        assert_eq!(q.get(b).unwrap().state, ItemState::Waiting);
        assert_eq!(q.remove_where(|_| true), 2, "l'élément en cours reste");
        assert_eq!(q.items().len(), 1);
    }
}
