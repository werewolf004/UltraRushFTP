//! Scheduled tasks (Task window of FTPRush): a saved queue or a script run at a time,
//! every N minutes, or every day at a time on chosen days of the week.
//!
//! Times are local wall-clock times kept as seconds since 1970 read as if they were UTC
//! (see [`crate::browsing::parse_datetime`]): "every day at 08:00" stays at 08:00 across
//! daylight saving changes. The book is stored in `tasks.json` next to the site book.
//!
//! A task whose time passed while the application was closed is not run late: it is
//! marked [`LastResult::Missed`] and planned for its next time (a one-time task is turned
//! off).

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::browsing::{format_datetime, parse_datetime};
use crate::queuestore::SavedTab;

pub const FILE_NAME: &str = "tasks.json";
const VERSION: u32 = 1;
const DAY: i64 = 86_400;
/// A time missed by less than this is still run (the check runs about once a second).
const GRACE_SECS: i64 = 120;

/// When a task runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Schedule {
    /// Once, at this local time.
    Once { at: i64 },
    /// Every `minutes`, the first time at `from`.
    Every { minutes: u32, from: i64 },
    /// Every chosen day of the week (bit 0 = Monday … bit 6 = Sunday) at this minute of
    /// the day.
    Daily { minute: u16, days: u8 },
}

/// Every day of the week.
pub const ALL_DAYS: u8 = 0b111_1111;

impl Schedule {
    /// First time strictly after `now`; `None` when there is none left.
    pub fn next_after(&self, now: i64) -> Option<i64> {
        match *self {
            Schedule::Once { at } => (at > now).then_some(at),
            Schedule::Every { minutes, from } => {
                let step = i64::from(minutes.max(1)) * 60;
                Some(if now < from { from } else { from + ((now - from) / step + 1) * step })
            }
            Schedule::Daily { minute, days } => {
                let minute = i64::from(minute.min(24 * 60 - 1));
                (0..8).map(|d| now.div_euclid(DAY) + d).find_map(|day| {
                    let at = day * DAY + minute * 60;
                    (at > now && days & (1 << weekday(day)) != 0).then_some(at)
                })
            }
        }
    }

    /// The task runs once only.
    pub fn is_once(&self) -> bool {
        matches!(self, Schedule::Once { .. })
    }
}

/// Day of the week of a day number since 1970 (0 = Monday … 6 = Sunday; 1970-01-01 was a
/// Thursday).
pub fn weekday(day: i64) -> u32 {
    u32::try_from((day + 3).rem_euclid(7)).unwrap_or(0)
}

/// What a task does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TaskAction {
    /// Loads these items in a new tab and starts its queue.
    Queue {
        queue: SavedTab,
        /// The tab is closed once its queue is finished and nothing is connected.
        #[serde(default)]
        close_tab: bool,
    },
    /// Runs this Pascal Script.
    Script { source: String },
}

/// How the last time went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastResult {
    Started,
    /// The time passed while the application was closed.
    Missed,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub schedule: Schedule,
    pub action: TaskAction,
    /// Next local time it runs (`None`: no time left, or turned off).
    #[serde(default)]
    pub next_run: Option<i64>,
    #[serde(default)]
    pub last_run: Option<i64>,
    #[serde(default)]
    pub last_result: Option<LastResult>,
}

impl Task {
    pub fn new(name: impl Into<String>, schedule: Schedule, action: TaskAction) -> Self {
        Self { id: 0, name: name.into(), enabled: true, schedule, action, next_run: None, last_run: None, last_result: None }
    }

    /// Plans the next time from `now` (after a change of its settings).
    pub fn plan(&mut self, now: i64) {
        self.next_run = if self.enabled { self.schedule.next_after(now) } else { None };
    }
}

/// The tasks, in the order of the Task window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskBook {
    pub version: u32,
    pub next_id: u64,
    pub tasks: Vec<Task>,
}

impl Default for TaskBook {
    fn default() -> Self {
        Self { version: VERSION, next_id: 1, tasks: Vec::new() }
    }
}

impl TaskBook {
    pub fn default_path() -> PathBuf {
        crate::sitestore::data_dir().join(FILE_NAME)
    }

    /// The book; empty when the file does not exist.
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Writes the file (temporary file, then rename).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }

    /// Adds a task, planned from `now`; answers its id.
    pub fn add(&mut self, mut task: Task, now: i64) -> u64 {
        task.id = self.next_id;
        self.next_id += 1;
        task.plan(now);
        let id = task.id;
        self.tasks.push(task);
        id
    }

    pub fn get(&self, id: u64) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        self.tasks.len() != before
    }

    /// At startup: the times passed while the application was closed are marked missed
    /// and the tasks planned again from `now`. Answers whether something changed.
    pub fn skip_missed(&mut self, now: i64) -> bool {
        let mut changed = false;
        for t in self.tasks.iter_mut().filter(|t| t.enabled) {
            match t.next_run {
                Some(at) if at + GRACE_SECS < now => {
                    t.last_result = Some(LastResult::Missed);
                    if t.schedule.is_once() {
                        t.enabled = false;
                    }
                    t.plan(now);
                    changed = true;
                }
                None => {
                    t.plan(now);
                    changed |= t.next_run.is_some();
                }
                Some(_) => {}
            }
        }
        changed
    }

    /// Tasks whose time has come: each is planned for its next time (a one-time task is
    /// turned off) and returned to be run.
    pub fn take_due(&mut self, now: i64) -> Vec<Task> {
        let mut due = Vec::new();
        for t in self.tasks.iter_mut().filter(|t| t.enabled && t.next_run.is_some_and(|at| at <= now)) {
            t.last_run = Some(now);
            t.last_result = Some(LastResult::Started);
            if t.schedule.is_once() {
                t.enabled = false;
            }
            t.plan(now);
            due.push(t.clone());
        }
        due
    }

    /// Earliest next time of the tasks turned on.
    pub fn next_due(&self) -> Option<i64> {
        self.tasks.iter().filter(|t| t.enabled).filter_map(|t| t.next_run).min()
    }
}

/// `YYYY-MM-DD HH:MM` of a local time.
pub fn format_local(at: i64) -> String {
    let s = format_datetime(at);
    s.get(..16).unwrap_or(&s).to_owned()
}

/// Local time of `YYYY-MM-DD HH:MM` (seconds optional).
pub fn parse_local(text: &str) -> Option<i64> {
    parse_datetime(text)
}

/// `HH:MM` → minute of the day.
pub fn parse_minute(text: &str) -> Option<u16> {
    let (h, m) = text.trim().split_once(':')?;
    let (h, m): (u16, u16) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Minute of the day → `HH:MM`.
pub fn format_minute(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// Local time of the computer from the parts of `GetLocalTime`.
pub fn local_now(parts: (u16, u16, u16, u16, u16, u16)) -> i64 {
    let (y, mo, d, h, mi, s) = parts;
    parse_datetime(&format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")).unwrap_or(0)
}

/// Day of the week (0 = Monday) of a local time.
pub fn weekday_of(at: i64) -> u32 {
    weekday(at.div_euclid(DAY))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> i64 {
        parse_local(text).unwrap()
    }

    #[test]
    fn once() {
        let s = Schedule::Once { at: at("2026-11-12 10:00") };
        assert_eq!(s.next_after(at("2026-11-12 09:59")), Some(at("2026-11-12 10:00")));
        assert_eq!(s.next_after(at("2026-11-12 10:00")), None);
    }

    #[test]
    fn every_n_minutes_keeps_its_rhythm() {
        let s = Schedule::Every { minutes: 30, from: at("2026-11-12 10:00") };
        assert_eq!(s.next_after(at("2026-11-12 08:00")), Some(at("2026-11-12 10:00")));
        assert_eq!(s.next_after(at("2026-11-12 10:00")), Some(at("2026-11-12 10:30")));
        assert_eq!(s.next_after(at("2026-11-12 11:47")), Some(at("2026-11-12 12:00")));
    }

    #[test]
    fn daily_on_chosen_days() {
        // 2026-11-12 is a Thursday.
        assert_eq!(weekday_of(at("2026-11-12 12:00")), 3);
        assert_eq!(weekday_of(at("1970-01-01 00:00")), 3);
        let every_day = Schedule::Daily { minute: 8 * 60, days: ALL_DAYS };
        assert_eq!(every_day.next_after(at("2026-11-12 07:00")), Some(at("2026-11-12 08:00")));
        assert_eq!(every_day.next_after(at("2026-11-12 08:00")), Some(at("2026-11-13 08:00")));
        // Monday and Saturday only.
        let some = Schedule::Daily { minute: 22 * 60 + 30, days: 0b010_0001 };
        assert_eq!(some.next_after(at("2026-11-12 23:00")), Some(at("2026-11-14 22:30")));
        assert_eq!(some.next_after(at("2026-11-14 22:30")), Some(at("2026-11-16 22:30")));
        assert_eq!(Schedule::Daily { minute: 0, days: 0 }.next_after(0), None);
        // Across the end of a year and a leap day.
        let daily = Schedule::Daily { minute: 0, days: ALL_DAYS };
        assert_eq!(daily.next_after(at("2027-12-31 12:00")), Some(at("2028-01-01 00:00")));
        assert_eq!(daily.next_after(at("2028-02-28 01:00")), Some(at("2028-02-29 00:00")));
    }

    fn script(name: &str, schedule: Schedule) -> Task {
        Task::new(name, schedule, TaskAction::Script { source: "x := 1;".into() })
    }

    #[test]
    fn due_tasks_are_planned_again_and_once_turns_off() {
        let now = at("2026-11-12 10:00");
        let mut book = TaskBook::default();
        let once = book.add(script("once", Schedule::Once { at: now + 60 }), now);
        let every = book.add(script("every", Schedule::Every { minutes: 5, from: now }), now);
        assert_eq!(book.next_due(), Some(now + 60));
        assert!(book.take_due(now).is_empty());

        let due = book.take_due(now + 300);
        assert_eq!(due.iter().map(|t| t.id).collect::<Vec<_>>(), [once, every]);
        let once = book.get(once).unwrap();
        assert!(!once.enabled && once.next_run.is_none() && once.last_result == Some(LastResult::Started));
        assert_eq!(book.get(every).unwrap().next_run, Some(now + 600));
        assert_eq!(book.next_due(), Some(now + 600));
    }

    #[test]
    fn times_passed_while_closed_are_missed_not_run() {
        let then = at("2026-11-12 10:00");
        let mut book = TaskBook::default();
        let once = book.add(script("once", Schedule::Once { at: then + 60 }), then);
        let daily = book.add(script("daily", Schedule::Daily { minute: 10 * 60 + 30, days: ALL_DAYS }), then);
        let later = at("2026-11-13 09:00");
        assert!(book.skip_missed(later));
        assert!(book.take_due(later).is_empty());
        let once = book.get(once).unwrap();
        assert_eq!((once.enabled, once.last_result.clone()), (false, Some(LastResult::Missed)));
        assert_eq!(book.get(daily).unwrap().next_run, Some(at("2026-11-13 10:30")));
        assert!(!book.skip_missed(later), "nothing more to do");
    }

    #[test]
    fn saved_and_loaded() {
        let dir = std::env::temp_dir().join(format!("urftp-tasks-{}", std::process::id()));
        let path = dir.join(FILE_NAME);
        let mut book = TaskBook::default();
        let queue = SavedTab { title: "Tab 1".into(), items: Vec::new() };
        book.add(Task::new("q", Schedule::Daily { minute: 60, days: ALL_DAYS }, TaskAction::Queue { queue, close_tab: true }), 0);
        book.save(&path).unwrap();
        assert_eq!(TaskBook::load(&path).unwrap(), book);
        assert_eq!(TaskBook::load(&dir.join("missing.json")).unwrap(), TaskBook::default());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn text_fields() {
        assert_eq!(parse_minute("8:05"), Some(485));
        assert_eq!(parse_minute("24:00"), None);
        assert_eq!(format_minute(485), "08:05");
        assert_eq!(format_local(at("2026-11-12 10:07:42")), "2026-11-12 10:07");
        assert_eq!(local_now((2026, 11, 12, 10, 7, 0)), at("2026-11-12 10:07"));
    }
}
