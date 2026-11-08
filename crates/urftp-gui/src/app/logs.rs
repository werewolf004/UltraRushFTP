//! The three logs of a tab (FTP commands and replies, transfers, system messages), their
//! files on disk, and the progress text of the status bar.

use std::io::Write;

use parking_lot::{Mutex, RwLock};
use urftp_core::browsing::format_size;
use urftp_core::queue::QueueStats;
use urftp_core::settings::InterfaceSettings;

use super::*;

/// Log a line goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LogChannel {
    /// Commands, replies and connection states of the panels.
    Ftp,
    /// Queue and transfers.
    Transfer,
    /// Everything else (files written, settings, scripts, errors of local operations).
    System,
}

impl LogChannel {
    fn file_name(self) -> &'static str {
        match self {
            LogChannel::Ftp => "ftp",
            LogChannel::Transfer => "transfer",
            LogChannel::System => "system",
        }
    }
}

/// What the logs need from the options, set by [`configure`].
struct Config {
    lines: usize,
    /// Folder of the log files when they are written.
    dir: Option<PathBuf>,
}

static CONFIG: RwLock<Config> = parking_lot::const_rwlock(Config { lines: 5000, dir: None });
/// Lines waiting to be written to the files: (log, line).
static PENDING: Mutex<Vec<(LogChannel, String)>> = parking_lot::const_mutex(Vec::new());

/// Applies the log options.
pub(super) fn configure(s: &InterfaceSettings) {
    let dir = s.log_to_disk.then(|| {
        let d = s.log_dir.trim();
        if d.is_empty() { urftp_core::sitestore::data_dir().join("logs") } else { PathBuf::from(d) }
    });
    *CONFIG.write() = Config { lines: usize::try_from(s.log_lines.clamp(100, 1_000_000)).unwrap_or(5000), dir };
}

/// Local date and time: (`YYYY-MM-DD`, `HH:MM:SS`).
fn now() -> (String, String) {
    let (y, mo, d, h, mi, s) = urftp_sys::win::local_time();
    (format!("{y:04}-{mo:02}-{d:02}"), format!("{h:02}:{mi:02}:{s:02}"))
}

/// Writes the waiting lines to `<folder>/<date>-<log>.log`. Errors are returned once and
/// the lines dropped (a full disk must not fill the memory).
pub(super) fn flush() -> std::io::Result<()> {
    let lines = std::mem::take(&mut *PENDING.lock());
    let Some(dir) = CONFIG.read().dir.clone() else { return Ok(()) };
    if lines.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(&dir)?;
    let date = now().0;
    for channel in [LogChannel::Ftp, LogChannel::Transfer, LogChannel::System] {
        let text: String = lines.iter().filter(|(c, _)| *c == channel).map(|(_, l)| format!("{l}\r\n")).collect();
        if text.is_empty() {
            continue;
        }
        let path = dir.join(format!("{date}-{}.log", channel.file_name()));
        std::fs::OpenOptions::new().create(true).append(true).open(&path)?.write_all(text.as_bytes())?;
    }
    Ok(())
}

impl Container {
    /// Adds a line to one of the logs of the tab (and to its file when the logs are
    /// written to disk). `side`: the panel it is about.
    pub(super) fn push_entry(&mut self, side: usize, channel: LogChannel, kind: LogKind, text: impl Into<String>) {
        let text = text.into();
        let site = self.sides.get(side).and_then(Side::profile).map(|p| p.name.clone()).unwrap_or_default();
        let (date, time) = now();
        let cfg = CONFIG.read();
        if cfg.dir.is_some() {
            let who = if site.is_empty() { String::new() } else { format!("[{site}] ") };
            PENDING.lock().push((channel, format!("{date} {time} {who}{}", text.replace('\n', "\r\n    "))));
        }
        let limit = cfg.lines;
        drop(cfg);
        self.log.push(LogEntry { side, kind, text, channel, site, time });
        let in_channel = self.log.iter().filter(|e| e.channel == channel).count();
        if in_channel > limit {
            // Drop a fifth of the oldest lines of that log at once.
            let mut extra = in_channel - limit + limit / 5;
            self.log.retain(|e| {
                if extra > 0 && e.channel == channel {
                    extra -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }

    pub(super) fn push_ftp(&mut self, side: usize, kind: LogKind, text: impl Into<String>) {
        self.push_entry(side, LogChannel::Ftp, kind, text);
    }

    pub(super) fn push_transfer(&mut self, side: usize, kind: LogKind, text: impl Into<String>) {
        self.push_entry(side, LogChannel::Transfer, kind, text);
    }
}

/// A line as shown: time, panel (or site), text.
pub(super) fn line_text(e: &LogEntry, site_names: bool, times: bool) -> String {
    let mut out = String::new();
    if times {
        out.push_str(&e.time);
        out.push(' ');
    }
    if e.channel != LogChannel::System {
        let who = if site_names && !e.site.is_empty() {
            e.site.clone()
        } else if e.side == 0 {
            t!("pane.left")
        } else {
            t!("pane.right")
        };
        out.push_str(&format!("[{who}] "));
    }
    out.push_str(&e.text);
    out
}

/// Progress text of the status bar (see [`InterfaceSettings::status_format`]).
pub(super) fn status_text(format: &str, st: &QueueStats, rate: u64) -> String {
    let percent = st.done_bytes.saturating_mul(100).checked_div(st.total_bytes).unwrap_or(0);
    let eta = st.eta(rate).map_or_else(
        || "—".to_string(),
        |d| {
            let s = d.as_secs();
            if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
        },
    );
    let finished = st.done + st.skipped;
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('p') => out.push_str(&format!("{percent}%")),
            Some('b') => out.push_str(&format!("{} / {}", format_size(st.done_bytes), format_size(st.total_bytes))),
            Some('s') => out.push_str(&format!("{}/s", format_size(rate))),
            Some('e') => out.push_str(&eta),
            Some('c') => out.push_str(&format!("{finished}/{}", st.items())),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_tokens() {
        let st =
            QueueStats { files: 4, done: 1, skipped: 1, total_bytes: 4096, done_bytes: 1024, left_bytes: 3072, ..QueueStats::default() };
        assert_eq!(status_text("%p · %b · %s · %e · %c · 100%%", &st, 1024), "25% · 1.0 KiB / 4.0 KiB · 1.0 KiB/s · 0:03 · 2/4 · 100%");
        assert_eq!(status_text("%e %x", &st, 0), "— %x");
    }

    #[test]
    fn lines_show_the_site_or_the_panel() {
        let e = LogEntry {
            side: 1,
            kind: LogKind::Command,
            text: "PWD".into(),
            channel: LogChannel::Ftp,
            site: "Demo".into(),
            time: "12:00:00".into(),
        };
        assert_eq!(line_text(&e, true, true), "12:00:00 [Demo] PWD");
        assert_eq!(line_text(&e, false, false), format!("[{}] PWD", t!("pane.right")));
        let sys = LogEntry { channel: LogChannel::System, ..e };
        assert_eq!(line_text(&sys, true, false), "PWD");
    }
}
