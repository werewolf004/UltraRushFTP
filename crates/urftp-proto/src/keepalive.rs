//! Keep alive (anti-idle commands) and automatic reconnection of a connection.
//!
//! The policy is plain data (built from the options by [`crate::profile`]); the state
//! ([`KeepAliveState`]) decides which command to send next and when to stop. The
//! connection itself is driven by [`crate::browse`] (browsing panels) and the queue engine.

use std::time::{Duration, Instant};

use urftp_core::pattern::wildcard_match;

use crate::ftp::FtpError;
use crate::session::SessionError;
use crate::sftp::SftpError;

/// Anti-idle commands sent while a connection is not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepAlive {
    /// Time without activity before a command is sent, then between two commands.
    pub interval: Duration,
    /// Commands sent in turn (`NOOP` when the list is empty).
    pub commands: Vec<String>,
    /// Pick the next command at random instead of in turn (some servers spot a command
    /// sent again and again).
    pub random: bool,
    /// Stop sending after this idle time (`None`: never stop).
    pub stop_after: Option<Duration>,
    /// Disconnect when sending stops.
    pub logout_after_stop: bool,
}

impl Default for KeepAlive {
    fn default() -> Self {
        Self { interval: Duration::from_secs(60), commands: vec!["NOOP".into()], random: false, stop_after: None, logout_after_stop: false }
    }
}

/// When and how to reconnect a connection that was lost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reconnect {
    /// Reconnect when the server closes the connection, times out or sends `421`.
    pub when_kicked: bool,
    /// Server replies that also trigger a reconnection: wildcard masks, case-insensitive,
    /// matched against the whole reply line (`421*`, `*idle*`...).
    pub on_replies: Vec<String>,
    /// Connection attempts before giving up.
    pub attempts: u32,
    /// Wait before every attempt.
    pub delay: Duration,
}

impl Default for Reconnect {
    fn default() -> Self {
        Self { when_kicked: false, on_replies: Vec::new(), attempts: 3, delay: Duration::from_secs(3) }
    }
}

impl Reconnect {
    /// Does this error call for a reconnection?
    pub fn wanted(&self, e: &SessionError) -> bool {
        (self.when_kicked && is_connection_lost(e)) || reply_text(e).is_some_and(|line| self.matches_reply(&line))
    }

    /// Does a reply line match one of the masks?
    pub fn matches_reply(&self, line: &str) -> bool {
        let line = line.trim();
        self.on_replies.iter().map(|m| m.trim()).filter(|m| !m.is_empty()).any(|m| wildcard_match(m, line))
    }
}

/// Keep-alive and reconnection settings of a site.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionPolicy {
    /// `None`: no anti-idle command.
    pub keep_alive: Option<KeepAlive>,
    pub reconnect: Reconnect,
}

/// Is the connection gone (closed, network error, timeout, `421`)?
pub fn is_connection_lost(e: &SessionError) -> bool {
    match e {
        SessionError::Ftp(f) => matches!(f, FtpError::Io(_) | FtpError::Timeout | FtpError::Closed) || f.reply_code() == Some(421),
        SessionError::Sftp(s) => matches!(s, SftpError::Io(_) | SftpError::Timeout | SftpError::Ssh(_)),
        _ => false,
    }
}

/// First line of the server reply carried by an error, as "code text".
fn reply_text(e: &SessionError) -> Option<String> {
    match e {
        SessionError::Ftp(FtpError::Unexpected { reply, .. }) => {
            Some(format!("{} {}", reply.code, reply.lines.first().map(|l| l.get(4..).unwrap_or(l).trim()).unwrap_or_default()))
        }
        _ => None,
    }
}

/// What the keep-alive loop should do now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeepAliveStep {
    /// Send this command.
    Send(String),
    /// Idle for too long: stop sending (and disconnect when asked).
    Stop { logout: bool },
}

/// Progress of the keep alive of one connection.
#[derive(Debug, Clone)]
pub struct KeepAliveState {
    policy: KeepAlive,
    /// Last activity of the user (commands sent by the keep alive do not count).
    idle_since: Instant,
    next: usize,
    rng: u64,
    stopped: bool,
}

impl KeepAliveState {
    pub fn new(policy: KeepAlive, now: Instant) -> Self {
        // Seed of the random choice: varies between connections, no cryptographic need.
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        Self { policy, idle_since: now, next: 0, rng: seed | 1, stopped: false }
    }

    /// The user did something: the idle time starts again, sending resumes.
    pub fn touch(&mut self, now: Instant) {
        self.idle_since = now;
        self.stopped = false;
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// When the next step is due (`None` once stopped).
    pub fn deadline(&self, last_sent: Instant) -> Option<Instant> {
        (!self.stopped).then(|| last_sent.max(self.idle_since) + self.policy.interval)
    }

    /// Step to take at `now` (called when the deadline is reached).
    pub fn step(&mut self, now: Instant) -> KeepAliveStep {
        if let Some(limit) = self.policy.stop_after
            && now.saturating_duration_since(self.idle_since) >= limit
        {
            self.stopped = true;
            return KeepAliveStep::Stop { logout: self.policy.logout_after_stop };
        }
        KeepAliveStep::Send(self.next_command())
    }

    fn next_command(&mut self) -> String {
        let cmds: Vec<&str> = self.policy.commands.iter().map(|c| c.trim()).filter(|c| !c.is_empty()).collect();
        if cmds.is_empty() {
            return "NOOP".into();
        }
        let i = if self.policy.random {
            // xorshift64: enough to vary the commands.
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            (self.rng % cmds.len() as u64) as usize
        } else {
            let i = self.next % cmds.len();
            self.next = self.next.wrapping_add(1);
            i
        };
        cmds[i].to_string()
    }
}

/// Splits a `;`-separated list of the options (commands, masks), ignoring blanks.
pub fn split_list(s: &str) -> Vec<String> {
    s.split(';').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ftp::Reply;

    fn unexpected(code: u16, text: &str) -> SessionError {
        SessionError::Ftp(FtpError::Unexpected { command: "LIST".into(), reply: Reply { code, lines: vec![format!("{code} {text}")] } })
    }

    #[test]
    fn commands_in_turn_and_default() {
        let t0 = Instant::now();
        let mut s = KeepAliveState::new(KeepAlive { commands: vec!["NOOP".into(), " PWD ".into(), "".into()], ..KeepAlive::default() }, t0);
        let sent: Vec<_> = (0..3).map(|_| s.step(t0)).collect();
        assert_eq!(sent, [KeepAliveStep::Send("NOOP".into()), KeepAliveStep::Send("PWD".into()), KeepAliveStep::Send("NOOP".into())]);
        let mut empty = KeepAliveState::new(KeepAlive { commands: Vec::new(), ..KeepAlive::default() }, t0);
        assert_eq!(empty.step(t0), KeepAliveStep::Send("NOOP".into()));
    }

    #[test]
    fn random_choice_stays_in_the_list() {
        let t0 = Instant::now();
        let list = vec!["NOOP".to_string(), "PWD".to_string(), "STAT".to_string()];
        let mut s = KeepAliveState::new(KeepAlive { commands: list.clone(), random: true, ..KeepAlive::default() }, t0);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let KeepAliveStep::Send(c) = s.step(t0) else { panic!("no stop limit") };
            assert!(list.contains(&c));
            seen.insert(c);
        }
        assert!(seen.len() > 1, "the commands vary");
    }

    #[test]
    fn stops_after_the_idle_limit_and_resumes_on_activity() {
        let t0 = Instant::now();
        let policy = KeepAlive {
            interval: Duration::from_secs(60),
            stop_after: Some(Duration::from_secs(600)),
            logout_after_stop: true,
            ..KeepAlive::default()
        };
        let mut s = KeepAliveState::new(policy, t0);
        assert_eq!(s.deadline(t0), Some(t0 + Duration::from_secs(60)));
        assert!(matches!(s.step(t0 + Duration::from_secs(300)), KeepAliveStep::Send(_)));
        assert_eq!(s.step(t0 + Duration::from_secs(600)), KeepAliveStep::Stop { logout: true });
        assert!(s.is_stopped());
        assert_eq!(s.deadline(t0), None);
        let t1 = t0 + Duration::from_secs(700);
        s.touch(t1);
        assert_eq!(s.deadline(t0), Some(t1 + Duration::from_secs(60)), "the user's activity restarts the clock");
    }

    #[test]
    fn deadline_follows_the_last_command() {
        let t0 = Instant::now();
        let s = KeepAliveState::new(KeepAlive::default(), t0);
        let sent = t0 + Duration::from_secs(60);
        assert_eq!(s.deadline(sent), Some(sent + Duration::from_secs(60)));
    }

    #[test]
    fn reconnection_triggers() {
        let kicked = Reconnect { when_kicked: true, ..Reconnect::default() };
        assert!(kicked.wanted(&SessionError::Ftp(FtpError::Closed)));
        assert!(kicked.wanted(&SessionError::Ftp(FtpError::Timeout)));
        assert!(kicked.wanted(&unexpected(421, "Timeout, closing control connection")));
        assert!(!kicked.wanted(&unexpected(550, "No such file")));
        assert!(!kicked.wanted(&SessionError::UnknownSite("x".into())));
        assert!(!Reconnect::default().wanted(&SessionError::Ftp(FtpError::Closed)), "off by default");

        let replies = Reconnect { on_replies: vec!["530*too many*".into(), " ".into()], ..Reconnect::default() };
        assert!(replies.wanted(&unexpected(530, "Sorry, Too many users logged in")));
        assert!(!replies.wanted(&unexpected(530, "Login incorrect")));
        assert!(!replies.wanted(&SessionError::Ftp(FtpError::Closed)), "masks only look at replies");
    }

    #[test]
    fn lists_are_split_on_semicolons() {
        assert_eq!(split_list(" NOOP ; PWD;;STAT -l "), ["NOOP", "PWD", "STAT -l"]);
        assert!(split_list(" ; ").is_empty());
    }
}
