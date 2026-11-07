// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Settings of a site replacing the global options: retries, skip list, priorities,
//! "existing file" rules, resume, separate accounts, `FEAT` / `CLNT`, Ident name.
mod common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use common::{MockConfig, MockServer};
use parking_lot::Mutex;
use urftp_core::exists::{ExistsAction, ExistsRules};
use urftp_core::queue::{Endpoint, ItemId, ItemState, Queue, SiteLimits};
use urftp_core::skiplist::{RuleTarget, SkipList, SkipReason, SkipRule};
use urftp_proto::engine::{Engine, EngineOptions};
use urftp_proto::ftp::FtpConfig;
use urftp_proto::ident::{IdentConfig, IdentServer};
use urftp_proto::session::{Credentials, RemoteSession, SiteIdent, SiteProfile, SiteProtocol, SiteRules};

fn site(name: &str, port: u16, rules: SiteRules) -> SiteProfile {
    SiteProfile {
        name: name.into(),
        host: "127.0.0.1".into(),
        port,
        user: "test".into(),
        password: "secret".into(),
        protocol: SiteProtocol::Ftp(FtpConfig::default()),
        limits: SiteLimits::default(),
        session: Default::default(),
        hosts: Default::default(),
        rules,
        ident: None,
    }
}

fn opts() -> EngineOptions {
    EngineOptions { retries: 3, retry_delay: Duration::from_millis(10), ..EngineOptions::default() }
}

fn temp_dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!("urftp-rules-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn local(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

async fn run(q: Queue, sites: Vec<SiteProfile>, opts: EngineOptions) -> Arc<Engine> {
    let e = Engine::new(Arc::new(Mutex::new(q)), sites, opts, None);
    e.run().await;
    e
}

fn state(e: &Engine, id: ItemId) -> ItemState {
    e.queue().lock().get(id).unwrap().state.clone()
}

#[tokio::test]
async fn the_site_retries_replace_the_global_ones() {
    let srv = MockServer::start(MockConfig { fail_first_retr: 1, ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/a.bin".into(), payload(100));
    let dir = temp_dir();
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/a.bin"), Endpoint::local(local(&dir.join("a.bin"))), false, None, None);
    // Globally 3 retries; this site: none.
    let e = run(q, vec![site("A", srv.addr.port(), SiteRules { retries: Some(0), ..SiteRules::default() })], opts()).await;
    assert!(matches!(state(&e, id), ItemState::Failed(_)), "{:?}", state(&e, id));
}

#[tokio::test]
async fn the_source_site_skip_list_and_priorities_apply() {
    let srv = MockServer::start(MockConfig::default()).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/rel");
        for f in ["a.rar", "b.nfo", "c.jpg"] {
            fs.files.insert(format!("/rel/{f}"), payload(10));
        }
    }
    let dir = temp_dir();
    let rules = SiteRules {
        skiplist: Some(SkipList { rules: vec![SkipRule::skip("*.jpg", RuleTarget::Files)], ..SkipList::default() }),
        priorities: Some(urftp_core::priority::PriorityList::new([urftp_core::pattern::Pattern::wildcard("*.nfo")])),
        ..SiteRules::default()
    };
    let mut q = Queue::new();
    q.push(Endpoint::remote("A", "/rel"), Endpoint::local(local(&dir.join("rel"))), true, None, None);
    let e = run(q, vec![site("A", srv.addr.port(), rules)], opts()).await;
    assert!(dir.join("rel/a.rar").exists() && dir.join("rel/b.nfo").exists());
    assert!(!dir.join("rel/c.jpg").exists(), "skipped by the site's own list");
    let retrs: Vec<String> = srv.received().into_iter().filter(|l| l.starts_with("RETR")).collect();
    assert!(retrs.first().is_some_and(|l| l.ends_with("b.nfo")), "the .nfo goes first: {retrs:?}");
    drop(e);
}

#[tokio::test]
async fn the_target_site_existing_file_rules_apply() {
    let srv = MockServer::start(MockConfig::default()).await;
    srv.fs.lock().unwrap().files.insert("/same.bin".into(), payload(50));
    let dir = temp_dir();
    std::fs::write(dir.join("same.bin"), payload(50)).unwrap();
    // Globally "same size: skip"; this site overwrites.
    let rules = SiteRules { exists: Some(ExistsRules { same: ExistsAction::Overwrite, ..ExistsRules::default() }), ..SiteRules::default() };
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/same.bin"), Endpoint::local(local(&dir.join("same.bin"))), false, None, None);
    let e = run(q, vec![site("A", srv.addr.port(), rules)], opts()).await;
    assert_eq!(state(&e, id), ItemState::Done);

    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/same.bin"), Endpoint::local(local(&dir.join("same.bin"))), false, None, None);
    let e = run(q, vec![site("A", srv.addr.port(), SiteRules::default())], opts()).await;
    assert_eq!(state(&e, id), ItemState::Skipped(SkipReason::AlreadyPresent), "global rule without own rules");
}

#[tokio::test]
async fn without_resume_a_partial_file_starts_again_from_zero() {
    let srv = MockServer::start(MockConfig::default()).await;
    let data = payload(10_000);
    srv.fs.lock().unwrap().files.insert("/big.bin".into(), data.clone());
    let dir = temp_dir();
    // A partial download with wrong bytes: resuming would keep them.
    std::fs::write(dir.join("big.bin"), vec![0xEE; 4_000]).unwrap();
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/big.bin"), Endpoint::local(local(&dir.join("big.bin"))), false, None, None);
    let e = run(q, vec![site("A", srv.addr.port(), SiteRules { resume: false, ..SiteRules::default() })], opts()).await;
    assert_eq!(state(&e, id), ItemState::Done);
    assert_eq!(std::fs::read(dir.join("big.bin")).unwrap(), data);
    assert!(!srv.received().iter().any(|l| l.starts_with("REST")), "no REST sent");
}

#[tokio::test]
async fn uploads_and_downloads_use_their_own_accounts() {
    // The server only knows "transfer"; the main account of the site is refused.
    let srv = MockServer::start(MockConfig { user: "transfer".into(), pass: "pw".into(), ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/down.bin".into(), payload(300));
    let dir = temp_dir();
    std::fs::write(dir.join("up.bin"), payload(200)).unwrap();
    let creds = Credentials { user: "transfer".into(), password: "pw".into() };
    let rules = SiteRules { upload_account: Some(creds.clone()), download_account: Some(creds), ..SiteRules::default() };
    let mut q = Queue::new();
    let down = q.push(Endpoint::remote("A", "/down.bin"), Endpoint::local(local(&dir.join("down.bin"))), false, None, None);
    let up = q.push(Endpoint::local(local(&dir.join("up.bin"))), Endpoint::remote("A", "/up.bin"), false, None, None);
    let e = run(q, vec![site("A", srv.addr.port(), rules)], EngineOptions { retries: 0, ..opts() }).await;
    assert_eq!(state(&e, down), ItemState::Done);
    assert_eq!(state(&e, up), ItemState::Done);
    assert_eq!(srv.fs.lock().unwrap().files["/up.bin"], payload(200));
    assert!(!srv.received().iter().any(|l| l == "USER test"), "the main account is not used for transfers");
    // One pool per login: the two idle connections are kept apart.
    assert_eq!(e.idle_connections("A"), 2);
}

#[tokio::test]
async fn feat_can_be_skipped_and_clnt_sent() {
    let srv = MockServer::start(MockConfig::default()).await;
    let cfg = FtpConfig { send_feat: false, client_name: Some("UltraRushFTP".into()), ..FtpConfig::default() };
    let p = SiteProfile { protocol: SiteProtocol::Ftp(cfg), ..site("A", srv.addr.port(), SiteRules::default()) };
    RemoteSession::open(&p, None).await.unwrap().quit().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    let received = srv.received();
    assert!(!received.iter().any(|l| l == "FEAT"), "{received:?}");
    assert!(received.iter().any(|l| l == "CLNT UltraRushFTP"), "{received:?}");
}

#[tokio::test]
async fn the_site_ident_name_is_answered_while_connecting() {
    let ident = IdentServer::start(IdentConfig { bind: SocketAddr::from(([127, 0, 0, 1], 0)), ..IdentConfig::default() }).await.unwrap();
    let srv = MockServer::start(MockConfig { ident_port: Some(ident.local_addr().port()), ..MockConfig::default() }).await;
    let mut p = site("A", srv.addr.port(), SiteRules::default());
    p.ident = Some(SiteIdent { registrar: ident.registrar(), name: "SITENAME".into() });
    RemoteSession::open(&p, None).await.unwrap().quit().await;
    // Once logged in, the name is no longer registered: another site gets the global one.
    RemoteSession::open(&site("B", srv.addr.port(), SiteRules::default()), None).await.unwrap().quit().await;
    let answers = srv.stats.lock().unwrap().ident_answers.clone();
    assert!(answers[0].ends_with(": SITENAME"), "{answers:?}");
    assert!(answers[1].ends_with(": URUSHFTP"), "{answers:?}");
}
