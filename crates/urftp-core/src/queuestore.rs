//! Unfinished queue kept on disk (`queue.json` next to the site book), so that a queue
//! stopped or interrupted by closing the application can be loaded again at the next start
//! ("Load unfinished queue" of FTPRush v2).
//!
//! Only the items left to do are kept: waiting, running (waiting again on restore),
//! stopped (waiting again), waiting for a decision (waiting again) and failed (still failed,
//! for *Retry failed*). Done and skipped items are dropped.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::queue::{Endpoint, ItemState, Queue, QueueItem};

pub const FILE_NAME: &str = "queue.json";
const VERSION: u32 = 1;

/// State kept for an item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedState {
    Waiting,
    Failed(String),
}

/// One item of the saved queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedItem {
    /// Site name of the source (`None`: local disk).
    pub src_site: Option<String>,
    pub src_path: String,
    pub dst_site: Option<String>,
    pub dst_path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub priority: Option<usize>,
    pub state: SavedState,
}

impl SavedItem {
    /// The item to keep, `None` when it is finished.
    fn of(item: &QueueItem) -> Option<Self> {
        let state = match &item.state {
            ItemState::Waiting | ItemState::Running | ItemState::Stopped | ItemState::NeedsConfirmation => SavedState::Waiting,
            ItemState::Failed(e) => SavedState::Failed(e.clone()),
            ItemState::Done | ItemState::Skipped(_) => return None,
        };
        Some(Self {
            src_site: item.src.site.clone(),
            src_path: item.src.path.clone(),
            dst_site: item.dst.site.clone(),
            dst_path: item.dst.path.clone(),
            is_dir: item.is_dir,
            size: item.size,
            priority: item.priority,
            state,
        })
    }
}

/// Unfinished queue of one tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedTab {
    /// Title the tab had (host of its connection, or "Tab n").
    pub title: String,
    pub items: Vec<SavedItem>,
}

impl SavedTab {
    /// Names of the sites the items use.
    pub fn sites(&self) -> Vec<String> {
        let mut names: Vec<String> = self.items.iter().flat_map(|i| [i.src_site.clone(), i.dst_site.clone()]).flatten().collect();
        names.sort();
        names.dedup();
        names
    }

    /// Items left to transfer (failed ones apart).
    pub fn waiting(&self) -> usize {
        self.items.iter().filter(|i| i.state == SavedState::Waiting).count()
    }

    /// The queue to put back, items in their saved order.
    pub fn into_queue(self) -> Queue {
        let mut q = Queue::new();
        q.append(self.items.into_iter().map(|i| {
            let state = match i.state {
                SavedState::Waiting => ItemState::Waiting,
                SavedState::Failed(e) => ItemState::Failed(e),
            };
            let src = Endpoint { site: i.src_site, path: i.src_path };
            let dst = Endpoint { site: i.dst_site, path: i.dst_path };
            (src, dst, i.is_dir, i.size, i.priority, state)
        }));
        q
    }
}

/// Unfinished queues of the tabs, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SavedQueues {
    pub version: u32,
    pub tabs: Vec<SavedTab>,
}

impl SavedQueues {
    /// Snapshot of the queues of the tabs, with their title (tabs left without unfinished
    /// item are dropped).
    pub fn snapshot<'a>(queues: impl IntoIterator<Item = (String, &'a Queue)>) -> Self {
        let tabs = queues
            .into_iter()
            .map(|(title, q)| SavedTab { title, items: q.items().iter().filter_map(SavedItem::of).collect() })
            .filter(|t| !t.items.is_empty())
            .collect();
        Self { version: VERSION, tabs }
    }

    pub fn item_count(&self) -> usize {
        self.tabs.iter().map(|t| t.items.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.item_count() == 0
    }

    pub fn default_path() -> PathBuf {
        crate::sitestore::data_dir().join(FILE_NAME)
    }

    /// Saved queue; `None` when there is none (missing file or no item).
    pub fn load(path: &Path) -> io::Result<Option<Self>> {
        match std::fs::read_to_string(path) {
            Ok(t) => {
                let q: Self = serde_json::from_str(&t).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok((!q.is_empty()).then_some(q))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Writes the file, or removes it when nothing is left to do.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if self.is_empty() {
            return match std::fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            };
        }
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_items_survive_a_restart() {
        let mut q = Queue::new();
        let a = q.push(Endpoint::remote("Site", "/rel"), Endpoint::local("C:\\dl\\rel"), true, None, None);
        let b = q.push(Endpoint::remote("Site", "/x.bin"), Endpoint::local("C:\\dl\\x.bin"), false, Some(10), Some(0));
        let c = q.push(Endpoint::local("C:\\up.txt"), Endpoint::remote("Other", "/up.txt"), false, Some(3), None);
        let d = q.push(Endpoint::remote("Site", "/done"), Endpoint::local("C:\\done"), false, None, None);
        q.set_state(a, ItemState::Stopped);
        q.set_state(b, ItemState::Running);
        q.set_state(c, ItemState::Failed("550 denied".into()));
        q.set_state(d, ItemState::Done);
        let empty = Queue::new();
        let saved = SavedQueues::snapshot([("site.example.com".to_string(), &q), ("Tab 2".to_string(), &empty)]);
        assert_eq!((saved.tabs.len(), saved.item_count()), (1, 3), "done items and empty tabs dropped");
        assert_eq!(saved.tabs[0].title, "site.example.com");
        assert_eq!(saved.tabs[0].sites(), ["Other", "Site"]);
        assert_eq!(saved.tabs[0].waiting(), 2);

        let dir = std::env::temp_dir().join(format!("urftp-queue-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        saved.save(&path).unwrap();
        let back = SavedQueues::load(&path).unwrap().expect("saved queue");
        assert_eq!(back, saved);
        let restored = back.tabs[0].clone().into_queue();
        let items = restored.items();
        // The order of the queue is kept (the priority item had been put first).
        assert_eq!(items.iter().map(|i| i.src.path.as_str()).collect::<Vec<_>>(), ["/x.bin", "/rel", "C:\\up.txt"]);
        assert_eq!(items[0].state, ItemState::Waiting, "running → waiting");
        assert_eq!(items[1].state, ItemState::Waiting, "stopped → waiting");
        assert_eq!(items[2].state, ItemState::Failed("550 denied".into()));
        assert!(items[1].is_dir && items[0].size == Some(10));

        // Nothing left: the file goes away.
        SavedQueues::snapshot([(String::new(), &empty)]).save(&path).unwrap();
        assert!(!path.exists());
        assert!(SavedQueues::load(&path).unwrap().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
