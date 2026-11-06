// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Browsing connection against the test server: keep alive, idle kick, reconnection.
mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{MockConfig, MockServer};
use parking_lot::Mutex;
use urftp_core::queue::SiteLimits;
use urftp_proto::browse::{self, BrowseCmd, BrowseEvent, BrowseHandle};
use urftp_proto::ftp::{FtpConfig, LogLine};
use urftp_proto::keepalive::{KeepAlive, Reconnect, SessionPolicy};
use urftp_proto::session::{SiteProfile, SiteProtocol};

fn profile(srv: &MockServer, session: SessionPolicy) -> SiteProfile {
    SiteProfile {
        name: "A".into(),
        host: "127.0.0.1".into(),
        port: srv.addr.port(),
        user: "test".into(),
        password: "secret".into(),
        protocol: SiteProtocol::Ftp(FtpConfig::default()),
        limits: SiteLimits::default(),
        session,
    }
}

fn keep(interval_ms: u64) -> KeepAlive {
    KeepAlive { interval: Duration::from_millis(interval_ms), ..KeepAlive::default() }
}

fn reconnect_when_kicked() -> Reconnect {
    Reconnect { when_kicked: true, delay: Duration::from_millis(10), ..Reconnect::default() }
}

/// Browsing connection whose events are recorded (log lines left out).
struct Panel {
    handle: BrowseHandle,
    events: Arc<Mutex<Vec<BrowseEvent>>>,
}

impl Panel {
    fn start(p: SiteProfile) -> Self {
        let events: Arc<Mutex<Vec<BrowseEvent>>> = Arc::default();
        let sink = events.clone();
        let handle = browse::spawn(&tokio::runtime::Handle::current(), move |e| {
            if !matches!(e, BrowseEvent::Log(LogLine::Reply(_) | LogLine::Status(_))) {
                sink.lock().push(e);
            }
        });
        handle.send(BrowseCmd::Connect(Box::new(p)));
        Self { handle, events }
    }

    fn events(&self) -> Vec<BrowseEvent> {
        self.events.lock().clone()
    }

    /// Waits until an event satisfies `f` (2 s at most).
    async fn wait(&self, what: &str, f: impl Fn(&BrowseEvent) -> bool) {
        self.wait_all(what, |ev| ev.iter().any(&f)).await;
    }

    /// Waits until the recorded events satisfy `f` (2 s at most). `f` sees a copy: it
    /// may not lock the events itself.
    async fn wait_all(&self, what: &str, f: impl Fn(&[BrowseEvent]) -> bool) {
        for _ in 0..200 {
            if f(&self.events()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{what} never came; events: {:#?}", self.events());
    }

    async fn wait_listing(&self, dir: &str, nth: usize) {
        self.wait_all(&format!("listing #{nth} of {dir}"), |ev| {
            ev.iter().filter(|e| matches!(e, BrowseEvent::Listing { dir: d, .. } if d == dir)).count() >= nth
        })
        .await;
    }

    fn clear(&self) {
        self.events.lock().clear();
    }
}

fn commands_sent(events: &[BrowseEvent], verb: &str) -> usize {
    events.iter().filter(|e| matches!(e, BrowseEvent::Log(LogLine::Command(c)) if c == verb)).count()
}

#[tokio::test]
async fn keep_alive_prevents_the_idle_kick() {
    let srv = MockServer::start(MockConfig { idle_kick: Some(Duration::from_millis(300)), ..MockConfig::default() }).await;
    let panel = Panel::start(profile(&srv, SessionPolicy { keep_alive: Some(keep(80)), ..SessionPolicy::default() }));
    panel.wait_listing("/", 1).await;
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(srv.stats.lock().unwrap().kicks, 0, "the anti-idle commands kept the connection");
    assert!(commands_sent(&panel.events(), "NOOP") >= 5, "NOOP every 80 ms: {:?}", panel.events());
    panel.handle.send(BrowseCmd::Refresh);
    panel.wait_listing("/", 2).await;
    assert_eq!(srv.stats.lock().unwrap().logins, 1, "still the first connection");
}

#[tokio::test]
async fn kicked_connection_reconnects_and_returns_to_its_folder() {
    let srv = MockServer::start(MockConfig { idle_kick: Some(Duration::from_millis(150)), ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().mkdir_p("/pub/Releases");
    let panel = Panel::start(profile(&srv, SessionPolicy { keep_alive: None, reconnect: reconnect_when_kicked() }));
    panel.wait_listing("/", 1).await;
    panel.handle.send(BrowseCmd::Cwd("/pub".into()));
    panel.wait_listing("/pub", 1).await;
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert_eq!(srv.stats.lock().unwrap().kicks, 1, "no keep alive: the server kicks");

    panel.clear();
    panel.handle.send(BrowseCmd::Refresh);
    panel.wait_listing("/pub", 1).await;
    let ev = panel.events();
    assert!(ev.iter().any(|e| matches!(e, BrowseEvent::Reconnecting { attempt: 1, .. })), "{ev:#?}");
    assert!(ev.contains(&BrowseEvent::Reconnected));
    assert!(!ev.iter().any(|e| matches!(e, BrowseEvent::Error(_) | BrowseEvent::Disconnected)), "{ev:#?}");
    assert_eq!(srv.stats.lock().unwrap().logins, 2);
}

#[tokio::test]
async fn without_reconnection_the_loss_is_reported() {
    let srv = MockServer::start(MockConfig { idle_kick: Some(Duration::from_millis(100)), ..MockConfig::default() }).await;
    let panel = Panel::start(profile(&srv, SessionPolicy::default()));
    panel.wait_listing("/", 1).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    panel.handle.send(BrowseCmd::Refresh);
    panel.wait("the disconnection", |e| *e == BrowseEvent::Disconnected).await;
    let ev = panel.events();
    // Either the `421` of the server or a network error, depending on what the system
    // reports first once the server has closed the connection.
    assert!(ev.iter().any(|e| matches!(e, BrowseEvent::Error(_))), "{ev:#?}");
    assert!(!ev.iter().any(|e| matches!(e, BrowseEvent::Reconnecting { .. })));
    // The panel knows it has no connection any more.
    panel.handle.send(BrowseCmd::Cwd("/".into()));
    panel.wait("not connected", |e| *e == BrowseEvent::NotConnected).await;
}

#[tokio::test]
async fn a_matching_reply_triggers_a_reconnection() {
    let srv =
        MockServer::start(MockConfig { fail_first_list: Some((1, "530 Sorry, too many users connected")), ..MockConfig::default() }).await;
    let reconnect = Reconnect { on_replies: vec!["530*too many*".into()], delay: Duration::from_millis(10), ..Reconnect::default() };
    let panel = Panel::start(profile(&srv, SessionPolicy { keep_alive: None, reconnect }));
    panel.wait_listing("/", 1).await;
    let ev = panel.events();
    assert!(ev.iter().any(|e| matches!(e, BrowseEvent::Reconnecting { reason, .. } if reason.contains("too many"))), "{ev:#?}");
    assert_eq!(srv.stats.lock().unwrap().logins, 2);
}

#[tokio::test]
async fn reconnection_gives_up_after_its_attempts() {
    let srv =
        MockServer::start(MockConfig { idle_kick: Some(Duration::from_millis(100)), max_logins: Some(1), ..MockConfig::default() }).await;
    let reconnect = Reconnect { attempts: 2, ..reconnect_when_kicked() };
    let panel = Panel::start(profile(&srv, SessionPolicy { keep_alive: None, reconnect }));
    panel.wait_listing("/", 1).await;
    tokio::time::sleep(Duration::from_millis(250)).await;
    panel.handle.send(BrowseCmd::Refresh);
    panel.wait("the disconnection", |e| *e == BrowseEvent::Disconnected).await;
    let ev = panel.events();
    let attempts: Vec<u32> = ev
        .iter()
        .filter_map(|e| match e {
            BrowseEvent::Reconnecting { attempt, of: 2, .. } => Some(*attempt),
            _ => None,
        })
        .collect();
    assert_eq!(attempts, [1, 2], "{ev:#?}");
    assert!(ev.iter().any(|e| matches!(e, BrowseEvent::ReconnectFailed(m) if m.contains("No more logins"))), "{ev:#?}");
}

#[tokio::test]
async fn keep_alive_stops_after_the_idle_limit_and_logs_out() {
    let srv = MockServer::start(MockConfig::default()).await;
    let policy = KeepAlive { stop_after: Some(Duration::from_millis(250)), logout_after_stop: true, ..keep(60) };
    let panel = Panel::start(profile(&srv, SessionPolicy { keep_alive: Some(policy), ..SessionPolicy::default() }));
    panel.wait_listing("/", 1).await;
    panel.wait("the stop", |e| *e == BrowseEvent::KeepAliveStopped { logged_out: true }).await;
    panel.wait("the disconnection", |e| *e == BrowseEvent::Disconnected).await;
    assert!(commands_sent(&panel.events(), "NOOP") >= 2);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(srv.stats.lock().unwrap().current_sessions, 0, "QUIT sent");
    assert!(srv.received().iter().any(|c| c == "QUIT"));
}

#[tokio::test]
async fn several_commands_are_sent_in_turn() {
    let srv = MockServer::start(MockConfig::default()).await;
    let policy = KeepAlive { commands: vec!["NOOP".into(), "PWD".into()], ..keep(60) };
    let panel = Panel::start(profile(&srv, SessionPolicy { keep_alive: Some(policy), ..SessionPolicy::default() }));
    panel.wait_listing("/", 1).await;
    panel.wait_all("three PWD", |ev| commands_sent(ev, "PWD") >= 3).await;
    let ev = panel.events();
    // After the login commands, keep-alive commands alternate.
    let ka: Vec<&str> = ev
        .iter()
        .filter_map(|e| match e {
            BrowseEvent::Log(LogLine::Command(c)) if c == "NOOP" || c == "PWD" => Some(c.as_str()),
            _ => None,
        })
        .skip_while(|c| *c == "PWD")
        .take(4)
        .collect();
    assert_eq!(ka, ["NOOP", "PWD", "NOOP", "PWD"]);
}
