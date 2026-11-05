// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use parking_lot::Mutex;
use std::time::{Duration, Instant};

use common::sftp::{MockSftp, SftpMockConfig};
use common::{MockConfig, MockServer};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use urftp_core::exists::ExistsRules;
use urftp_core::pattern::Pattern;
use urftp_core::priority::PriorityList;
use urftp_core::queue::{Endpoint, ItemId, ItemState, Queue, SiteLimits};
use urftp_core::skiplist::{RuleTarget, SkipList, SkipRule};
use urftp_proto::engine::{Engine, EngineEvent, EngineOptions};
use urftp_proto::ftp::FtpConfig;
use urftp_proto::session::{SiteProfile, SiteProtocol};
use urftp_proto::sftp::{HostKeyPolicy, SftpConfig};
use urftp_proto::throttle::RateLimiter;

fn site(name: &str, port: u16, max_transfers: u32) -> SiteProfile {
    SiteProfile {
        name: name.into(),
        host: "127.0.0.1".into(),
        port,
        user: "test".into(),
        password: "secret".into(),
        protocol: SiteProtocol::Ftp(FtpConfig::default()),
        limits: SiteLimits { max_transfers, ..SiteLimits::default() },
    }
}

fn opts() -> EngineOptions {
    EngineOptions { retry_delay: Duration::from_millis(20), ..EngineOptions::default() }
}

fn temp_dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!("urftp-engine-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn local(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

fn payload(n: usize, seed: u8) -> Vec<u8> {
    (0..n).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
}

struct Run {
    engine: Arc<Engine>,
    events: UnboundedReceiver<EngineEvent>,
}

fn engine(queue: Queue, sites: Vec<SiteProfile>, opts: EngineOptions) -> Run {
    let (tx, rx) = unbounded_channel();
    Run { engine: Engine::new(Arc::new(Mutex::new(queue)), sites, opts, Some(tx)), events: rx }
}

fn drain(rx: &mut UnboundedReceiver<EngineEvent>) -> Vec<EngineEvent> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

fn state(e: &Engine, id: ItemId) -> ItemState {
    e.queue().lock().get(id).unwrap().state.clone()
}

#[tokio::test]
async fn multi_transfer_respects_the_site_limit() {
    let srv = MockServer::start(MockConfig { transfer_delay: Some(Duration::from_millis(200)), ..MockConfig::default() }).await;
    let dir = temp_dir();
    let mut q = Queue::new();
    let mut ids = Vec::new();
    for i in 0..6 {
        srv.fs.lock().unwrap().files.insert(format!("/f{i}.bin"), payload(50_000, i as u8));
        ids.push(q.push(
            Endpoint::remote("A", format!("/f{i}.bin")),
            Endpoint::local(local(&dir.join(format!("f{i}.bin")))),
            false,
            None,
            None,
        ));
    }
    let r = engine(q, vec![site("A", srv.addr.port(), 3)], opts());
    let start = Instant::now();
    r.engine.run().await;
    let took = start.elapsed();

    for (i, id) in ids.iter().enumerate() {
        assert_eq!(state(&r.engine, *id), ItemState::Done);
        assert_eq!(std::fs::read(dir.join(format!("f{i}.bin"))).unwrap(), payload(50_000, i as u8));
    }
    let max = srv.stats.lock().unwrap().max_sessions;
    assert_eq!(max, 3, "at most 3 simultaneous connections");
    assert!(took < Duration::from_millis(6 * 200), "parallel transfers: {took:?}");
    assert_eq!(srv.stats.lock().unwrap().logins, 3, "connections are reused");
    assert_eq!(r.engine.idle_connections("A"), 3);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn single_connection_mode_uses_one_login() {
    let srv = MockServer::start(MockConfig { transfer_delay: Some(Duration::from_millis(30)), ..MockConfig::default() }).await;
    let dir = temp_dir();
    let mut q = Queue::new();
    for i in 0..4 {
        srv.fs.lock().unwrap().files.insert(format!("/s{i}"), vec![i; 1000]);
        q.push(Endpoint::remote("A", format!("/s{i}")), Endpoint::local(local(&dir.join(format!("s{i}")))), false, None, None);
    }
    let r = engine(q, vec![site("A", srv.addr.port(), 0)], opts());
    r.engine.run().await;
    let st = srv.stats.lock().unwrap();
    assert_eq!((st.max_sessions, st.logins), (1, 1));
    drop(st);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn folder_download_with_skiplist_and_priorities() {
    let srv = MockServer::start(MockConfig::default()).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/rel/Sample");
        fs.mkdir_p("/rel/sub");
        for (name, n) in [("a.r00", 3000), ("a.r01", 3000), ("a.nfo", 10), ("a.sfv", 20), ("cover.jpg", 5), ("vide.txt", 0)] {
            fs.files.insert(format!("/rel/{name}"), payload(n, 1));
        }
        fs.files.insert("/rel/Sample/sample.mkv".into(), payload(100, 2));
        fs.files.insert("/rel/sub/inner.bin".into(), payload(100, 3));
    }
    let dir = temp_dir();
    let mut q = Queue::new();
    let root = q.push(Endpoint::remote("A", "/rel"), Endpoint::local(local(&dir.join("rel"))), true, None, None);
    let options = EngineOptions {
        skiplist: SkipList {
            rules: vec![SkipRule::skip("sample", RuleTarget::Dirs), SkipRule::skip("*.jpg", RuleTarget::Files)],
            skip_zero_byte_files: true,
            ..SkipList::default()
        },
        priorities: PriorityList::new([Pattern::wildcard("*.sfv"), Pattern::wildcard("*.nfo")]),
        ..opts()
    };
    // A single connection: the RETR order reflects the queue order.
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], options);
    r.engine.run().await;

    assert_eq!(state(&r.engine, root), ItemState::Done);
    let base = dir.join("rel");
    assert!(base.join("a.r00").exists() && base.join("a.sfv").exists() && base.join("sub/inner.bin").exists());
    assert!(!base.join("Sample").exists(), "folder skipped by the skip list");
    assert!(!base.join("cover.jpg").exists(), "file skipped by the skip list");
    assert!(!base.join("vide.txt").exists(), "0-byte file skipped");
    let retrs: Vec<String> = srv.received().into_iter().filter(|l| l.starts_with("RETR ")).collect();
    assert_eq!(retrs[..2], ["RETR a.sfv".to_string(), "RETR a.nfo".to_string()], "{retrs:?}");
    let events = drain(&mut r.events);
    assert!(events.iter().any(|e| matches!(e, EngineEvent::Expanded { id, children: 5 } if *id == root)), "{events:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn folder_upload_creates_remote_tree() {
    let srv = MockServer::start(MockConfig::default()).await;
    let dir = temp_dir();
    let src = dir.join("monrelease");
    std::fs::create_dir_all(src.join("CD1")).unwrap();
    std::fs::write(src.join("a.rar"), payload(20_000, 4)).unwrap();
    std::fs::write(src.join("CD1/b.rar"), payload(10_000, 5)).unwrap();
    let mut q = Queue::new();
    q.push(Endpoint::local(local(&src)), Endpoint::remote("A", "/incoming/monrelease"), true, None, None);
    srv.fs.lock().unwrap().mkdir_p("/incoming");
    let r = engine(q, vec![site("A", srv.addr.port(), 2)], opts());
    r.engine.run().await;
    let fs = srv.fs.lock().unwrap();
    assert!(fs.dirs.contains("/incoming/monrelease/CD1"));
    assert_eq!(fs.files["/incoming/monrelease/a.rar"], payload(20_000, 4));
    assert_eq!(fs.files["/incoming/monrelease/CD1/b.rar"], payload(10_000, 5));
    drop(fs);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn file_exists_rules_skip_resume_and_rename() {
    let srv = MockServer::start(MockConfig::default()).await;
    let data = payload(30_000, 9);
    for n in ["same", "small", "big"] {
        srv.fs.lock().unwrap().files.insert(format!("/{n}.bin"), data.clone());
    }
    let dir = temp_dir();
    std::fs::write(dir.join("same.bin"), &data).unwrap();
    std::fs::write(dir.join("small.bin"), &data[..10_000]).unwrap();
    std::fs::write(dir.join("big.bin"), payload(50_000, 1)).unwrap();
    let mut q = Queue::new();
    let ids: Vec<ItemId> = ["same", "small", "big"]
        .iter()
        .map(|n| {
            q.push(Endpoint::remote("A", format!("/{n}.bin")), Endpoint::local(local(&dir.join(format!("{n}.bin")))), false, None, None)
        })
        .collect();
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { exists: ExistsRules::default(), ..opts() });
    r.engine.run().await;

    assert!(matches!(state(&r.engine, ids[0]), ItemState::Skipped(_)));
    assert_eq!(state(&r.engine, ids[1]), ItemState::Done);
    assert_eq!(std::fs::read(dir.join("small.bin")).unwrap(), data, "smaller file resumed");
    assert!(srv.received().iter().any(|l| l == "REST 10000"));
    assert_eq!(std::fs::read(dir.join("big (1).bin")).unwrap(), data, "larger file: renamed");
    assert_eq!(std::fs::read(dir.join("big.bin")).unwrap(), payload(50_000, 1), "the original is intact");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn retries_after_temporary_failures() {
    let srv = MockServer::start(MockConfig { fail_first_retr: 2, ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/r.bin".into(), payload(1000, 7));
    let dir = temp_dir();
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/r.bin"), Endpoint::local(local(&dir.join("r.bin"))), false, None, None);
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], opts());
    r.engine.run().await;
    assert_eq!(state(&r.engine, id), ItemState::Done);
    let retries = drain(&mut r.events).into_iter().filter(|e| matches!(e, EngineEvent::Retrying { .. })).count();
    assert_eq!(retries, 2);

    let srv = MockServer::start(MockConfig { fail_first_retr: 10, ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/r.bin".into(), payload(1000, 7));
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/r.bin"), Endpoint::local(local(&dir.join("r2.bin"))), false, None, None);
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { retries: 1, ..opts() });
    r.engine.run().await;
    assert!(matches!(state(&r.engine, id), ItemState::Failed(e) if e.contains("451")));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn stopping_a_running_transfer() {
    let srv = MockServer::start(MockConfig { transfer_delay: Some(Duration::from_secs(5)), ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/slow.bin".into(), payload(1000, 1));
    let dir = temp_dir();
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/slow.bin"), Endpoint::local(local(&dir.join("slow.bin"))), false, None, None);
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], opts());
    let e = r.engine.clone();
    let start = Instant::now();
    let runner = tokio::spawn(async move { e.run().await });
    while !matches!(r.events.recv().await, Some(EngineEvent::Started { .. })) {}
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(r.engine.stop(id));
    runner.await.unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(state(&r.engine, id), ItemState::Stopped);
    assert_eq!(r.engine.idle_connections("A"), 0, "the aborted connection is not reused");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn download_speed_limit() {
    let srv = MockServer::start(MockConfig::default()).await;
    srv.fs.lock().unwrap().files.insert("/big.bin".into(), payload(400_000, 3));
    let dir = temp_dir();
    let mut q = Queue::new();
    q.push(Endpoint::remote("A", "/big.bin"), Endpoint::local(local(&dir.join("big.bin"))), false, None, None);
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { download_limit: RateLimiter::new(200_000), ..opts() });
    let start = Instant::now();
    r.engine.run().await;
    let t = start.elapsed();
    assert_eq!(std::fs::read(dir.join("big.bin")).unwrap(), payload(400_000, 3));
    assert!(t >= Duration::from_millis(1500), "200 KB/s limit not respected: {t:?}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn fxp_folder_between_two_sites() {
    let a = MockServer::start(MockConfig::default()).await;
    let b = MockServer::start(MockConfig::default()).await;
    a.fs.lock().unwrap().mkdir_p("/rel/sub");
    a.fs.lock().unwrap().files.insert("/rel/x.rar".into(), payload(60_000, 1));
    a.fs.lock().unwrap().files.insert("/rel/sub/y.rar".into(), payload(30_000, 2));
    b.fs.lock().unwrap().mkdir_p("/dst");
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/rel"), Endpoint::remote("B", "/dst/rel"), true, None, None);
    let r = engine(q, vec![site("A", a.addr.port(), 2), site("B", b.addr.port(), 2)], opts());
    r.engine.run().await;
    assert_eq!(state(&r.engine, id), ItemState::Done);
    let fs = b.fs.lock().unwrap();
    assert_eq!(fs.files["/dst/rel/x.rar"], payload(60_000, 1));
    assert_eq!(fs.files["/dst/rel/sub/y.rar"], payload(30_000, 2));
}

#[tokio::test]
async fn sftp_site_upload_and_download() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    let dir = temp_dir();
    std::fs::write(dir.join("up.bin"), payload(70_000, 6)).unwrap();
    let profile = SiteProfile {
        protocol: SiteProtocol::Sftp {
            config: SftpConfig { host_key: HostKeyPolicy::Known(vec![srv.fingerprint.clone()]), ..SftpConfig::default() },
            key: None,
        },
        ..site("S", srv.addr.port(), 2)
    };
    let mut q = Queue::new();
    let up = q.push(Endpoint::local(local(&dir.join("up.bin"))), Endpoint::remote("S", "/home/test/up.bin"), false, None, None);
    let r = engine(q, vec![profile.clone()], opts());
    r.engine.run().await;
    assert_eq!(state(&r.engine, up), ItemState::Done);
    assert_eq!(srv.fs.lock().unwrap().files["/home/test/up.bin"], payload(70_000, 6));

    let mut q = Queue::new();
    let down = q.push(Endpoint::remote("S", "/home/test/up.bin"), Endpoint::local(local(&dir.join("down.bin"))), false, None, None);
    let r = engine(q, vec![profile], opts());
    r.engine.run().await;
    assert_eq!(state(&r.engine, down), ItemState::Done);
    assert_eq!(std::fs::read(dir.join("down.bin")).unwrap(), payload(70_000, 6));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn unknown_site_fails_cleanly() {
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("absent", "/x"), Endpoint::local("C:/x"), false, None, None);
    let r = engine(q, vec![], EngineOptions { retries: 0, ..opts() });
    r.engine.run().await;
    assert!(matches!(state(&r.engine, id), ItemState::Failed(e) if e == "unknown site: absent"));
}
