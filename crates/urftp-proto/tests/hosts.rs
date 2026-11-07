// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Several addresses per site (rotation), availability test, ghost login and `-password`.
mod common;

use std::sync::{Arc, Mutex};

use common::{MockConfig, MockServer};
use urftp_core::queue::SiteLimits;
use urftp_proto::ftp::{FtpConfig, LogLine};
use urftp_proto::session::{HostList, RemoteSession, SiteProfile, SiteProtocol};

fn profile(port: u16, alternates: Vec<(String, u16)>) -> SiteProfile {
    SiteProfile {
        name: "A".into(),
        host: "127.0.0.1".into(),
        port,
        user: "test".into(),
        password: "secret".into(),
        protocol: SiteProtocol::Ftp(FtpConfig::default()),
        limits: SiteLimits::default(),
        session: Default::default(),
        hosts: HostList::new(alternates),
    }
}

/// A local port nobody listens on.
async fn closed_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    l.local_addr().unwrap().port()
}

fn status_log() -> (urftp_proto::ftp::LogSink, Arc<Mutex<Vec<String>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    let log: urftp_proto::ftp::LogSink = Arc::new(move |l: LogLine| {
        if let LogLine::Status(s) = l {
            sink.lock().unwrap().push(s);
        }
    });
    (log, lines)
}

#[tokio::test]
async fn an_unreachable_address_moves_to_the_next_and_is_remembered() {
    let srv = MockServer::start(MockConfig::default()).await;
    let dead = closed_port().await;
    let site = profile(dead, vec![("127.0.0.1".into(), srv.addr.port())]);

    let (log, lines) = status_log();
    let s = RemoteSession::open(&site, Some(log)).await.expect("second address");
    s.quit().await;
    assert!(lines.lock().unwrap().iter().any(|l| l.contains(&format!("127.0.0.1:{dead} failed"))));
    assert_eq!(site.current_address(), ("127.0.0.1".to_string(), srv.addr.port()));

    // The next connection (any copy of the profile) starts with the address that worked.
    let (log, lines) = status_log();
    let copy = site.clone();
    RemoteSession::open(&copy, Some(log)).await.expect("remembered address").quit().await;
    assert!(lines.lock().unwrap().is_empty(), "no failed attempt: {:?}", lines.lock().unwrap());
}

#[tokio::test]
async fn a_refused_login_moves_to_the_next_address() {
    let full = MockServer::start(MockConfig { max_logins: Some(0), ..MockConfig::default() }).await;
    let ok = MockServer::start(MockConfig::default()).await;
    let site = profile(full.addr.port(), vec![("127.0.0.1".into(), ok.addr.port())]);
    RemoteSession::open(&site, None).await.expect("second server").quit().await;
    assert_eq!(site.current_address().1, ok.addr.port());
    assert_eq!(ok.stats.lock().unwrap().logins, 1);
}

#[tokio::test]
async fn every_address_failing_gives_the_last_error() {
    let site = profile(closed_port().await, vec![("127.0.0.1".into(), closed_port().await)]);
    let e = RemoteSession::open(&site, None).await.err().expect("nothing listens");
    assert!(urftp_proto::keepalive::is_connection_lost(&e), "{e}");
}

#[tokio::test]
async fn availability_test_reports_the_address_that_answered() {
    let srv = MockServer::start(MockConfig::default()).await;
    let site = profile(closed_port().await, vec![("127.0.0.1".into(), srv.addr.port())]);
    let ((host, port), took) = RemoteSession::probe(&site).await.expect("reachable");
    assert_eq!((host.as_str(), port), ("127.0.0.1", srv.addr.port()));
    assert!(took.as_secs() < 30);
    // Logged out politely.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(srv.received().iter().any(|l| l == "QUIT"));

    let down = profile(closed_port().await, Vec::new());
    assert!(RemoteSession::probe(&down).await.is_err());
}

#[tokio::test]
async fn ghost_login_and_quiet_password() {
    let srv = MockServer::start(MockConfig { user: "!test".into(), pass: "-secret".into(), ..MockConfig::default() }).await;
    let site = profile(srv.addr.port(), Vec::new()).with_login_variant(true, true);
    RemoteSession::open(&site, None).await.expect("ghost login").quit().await;
    let received = srv.received();
    assert!(received.iter().any(|l| l == "USER !test"), "{received:?}");
}
