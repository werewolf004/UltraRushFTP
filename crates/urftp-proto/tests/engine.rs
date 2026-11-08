// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use std::path::{Path, PathBuf};
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
use urftp_proto::engine::{Engine, EngineEvent, EngineOptions, TransferLog};
use urftp_proto::ftp::{FtpConfig, LogLine};
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
        session: Default::default(),
        hosts: Default::default(),
        rules: Default::default(),
        ident: None,
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
async fn transfer_connections_write_to_the_log() {
    let srv = MockServer::start(MockConfig::default()).await;
    srv.fs.lock().unwrap().files.insert("/logged.bin".into(), payload(1000, 7));
    let dir = temp_dir();
    let mut q = Queue::new();
    q.push(Endpoint::remote("A", "/logged.bin"), Endpoint::local(local(&dir.join("logged.bin"))), false, None, None);
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    let log = TransferLog(Arc::new(move |l: LogLine| {
        if let LogLine::Command(c) = l {
            sink.lock().push(c);
        }
    }));
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { log: Some(log), ..opts() });
    r.engine.run().await;
    let lines = lines.lock();
    assert!(lines.iter().any(|c| c == "RETR logged.bin" || c == "RETR /logged.bin"), "{lines:?}");
    assert!(lines.iter().any(|c| c == "PASS ****"), "password masked: {lines:?}");
    drop(lines);
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
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { exists: urftp_core::exists::ExistsSet::default(), ..opts() });
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

#[tokio::test]
async fn a_pooled_connection_kicked_while_idle_is_replaced() {
    let srv = MockServer::start(MockConfig { idle_kick: Some(Duration::from_millis(150)), ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/a.bin".into(), payload(1000, 1));
    srv.fs.lock().unwrap().files.insert("/b.bin".into(), payload(1000, 2));
    let dir = temp_dir();
    // Pooled connections are checked once idle for longer than the keep-alive interval.
    let keep = urftp_proto::keepalive::KeepAlive { interval: Duration::from_millis(50), ..Default::default() };
    let profile = SiteProfile {
        session: urftp_proto::keepalive::SessionPolicy { keep_alive: Some(keep), ..Default::default() },
        ..site("A", srv.addr.port(), 1)
    };
    let mut q = Queue::new();
    let a = q.push(Endpoint::remote("A", "/a.bin"), Endpoint::local(local(&dir.join("a.bin"))), false, None, None);
    let r = engine(q, vec![profile], opts());
    r.engine.run().await;
    assert_eq!(state(&r.engine, a), ItemState::Done);
    assert_eq!(r.engine.idle_connections("A"), 1);

    // The server kicks the idle connection; the next item must not fail because of it.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(srv.stats.lock().unwrap().kicks, 1);
    let b = r.engine.queue().lock().push(Endpoint::remote("A", "/b.bin"), Endpoint::local(local(&dir.join("b.bin"))), false, None, None);
    r.engine.run().await;
    assert_eq!(state(&r.engine, b), ItemState::Done);
    assert_eq!(r.engine.queue().lock().get(b).unwrap().attempts, 0, "no failed attempt: the dead connection was dropped first");
    assert_eq!(srv.stats.lock().unwrap().logins, 2);
    assert_eq!(std::fs::read(dir.join("b.bin")).unwrap(), payload(1000, 2));
}

#[tokio::test]
async fn pause_requeues_the_running_item_and_resume_finishes_the_queue() {
    let srv = MockServer::start(MockConfig { transfer_delay: Some(Duration::from_millis(300)), ..MockConfig::default() }).await;
    let dir = temp_dir();
    let mut q = Queue::new();
    let mut ids = Vec::new();
    for i in 0..3u8 {
        srv.fs.lock().unwrap().files.insert(format!("/p{i}.bin"), payload(20_000, i));
        ids.push(q.push(
            Endpoint::remote("A", format!("/p{i}.bin")),
            Endpoint::local(local(&dir.join(format!("p{i}.bin")))),
            false,
            None,
            None,
        ));
    }
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], opts());
    let e = r.engine.clone();
    let run = tokio::spawn(async move { e.run().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    r.engine.pause();
    assert!(r.engine.is_paused());
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!run.is_finished(), "a paused run waits");
    for id in &ids {
        assert_eq!(state(&r.engine, *id), ItemState::Waiting, "nothing runs or fails while paused");
    }
    r.engine.resume();
    tokio::time::timeout(Duration::from_secs(10), run).await.unwrap().unwrap();
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(state(&r.engine, *id), ItemState::Done);
        assert_eq!(std::fs::read(dir.join(format!("p{i}.bin"))).unwrap(), payload(20_000, i as u8));
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn stop_all_ends_the_run_and_leaves_the_rest_waiting() {
    let srv = MockServer::start(MockConfig { transfer_delay: Some(Duration::from_millis(300)), ..MockConfig::default() }).await;
    let dir = temp_dir();
    let mut q = Queue::new();
    let mut ids = Vec::new();
    for i in 0..3u8 {
        srv.fs.lock().unwrap().files.insert(format!("/s{i}.bin"), payload(1000, i));
        ids.push(q.push(
            Endpoint::remote("A", format!("/s{i}.bin")),
            Endpoint::local(local(&dir.join(format!("s{i}.bin")))),
            false,
            None,
            None,
        ));
    }
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], opts());
    let e = r.engine.clone();
    let run = tokio::spawn(async move { e.run().await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    r.engine.stop_all();
    tokio::time::timeout(Duration::from_secs(5), run).await.unwrap().unwrap();
    assert_eq!(state(&r.engine, ids[0]), ItemState::Stopped);
    assert_eq!(state(&r.engine, ids[1]), ItemState::Waiting, "nothing else started");
    assert_eq!(state(&r.engine, ids[2]), ItemState::Waiting);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn retries_per_direction_cycle_retry_and_rounds_after_the_end() {
    // Download retries come from their own setting.
    let srv = MockServer::start(MockConfig { fail_first_retr: 2, ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/a.bin".into(), payload(500, 1));
    srv.fs.lock().unwrap().files.insert("/b.bin".into(), payload(500, 2));
    let dir = temp_dir();
    let mut q = Queue::new();
    let a = q.push(Endpoint::remote("A", "/a.bin"), Endpoint::local(local(&dir.join("a.bin"))), false, None, None);
    let b = q.push(Endpoint::remote("A", "/b.bin"), Endpoint::local(local(&dir.join("b.bin"))), false, None, None);
    let options = EngineOptions { retries: 0, retries_download: Some(1), cycle_retry: true, ..opts() };
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], options);
    r.engine.run().await;
    // First RETR of a fails (retry, sent to the bottom), then b fails once (retry, to the
    // bottom), then a and b succeed: a was moved behind b.
    let events = drain(&mut r.events);
    let done: Vec<u64> = events.iter().filter_map(|e| if let EngineEvent::Done { id, .. } = e { Some(*id) } else { None }).collect();
    assert_eq!(done, [a, b]);
    assert_eq!(events.iter().filter(|e| matches!(e, EngineEvent::Retrying { .. })).count(), 2);
    assert_eq!(state(&r.engine, a), ItemState::Done);

    // No retry at all, but one more round once the queue is finished.
    let srv = MockServer::start(MockConfig { fail_first_retr: 1, ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/c.bin".into(), payload(500, 3));
    let mut q = Queue::new();
    let c = q.push(Endpoint::remote("A", "/c.bin"), Endpoint::local(local(&dir.join("c.bin"))), false, None, None);
    let options = EngineOptions { retries: 0, retry_failed_rounds: 1, retry_delay: Duration::from_millis(10), ..opts() };
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], options);
    r.engine.run().await;
    assert_eq!(state(&r.engine, c), ItemState::Done);
    assert!(drain(&mut r.events).contains(&EngineEvent::RetryRound { round: 1, items: 1 }));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn script_options_filter_limit_depth_and_refresh_a_folder() {
    use urftp_core::itemopts::{ItemOptions, SortKey, SortOrder, TransferFilter};
    let srv = MockServer::start(MockConfig::default()).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/rel/Sample");
        fs.mkdir_p("/rel/CD1/deeper");
        fs.files.insert("/rel/a.rar".into(), payload(300, 1));
        fs.files.insert("/rel/b.nfo".into(), payload(20, 2));
        fs.files.insert("/rel/c.sfv".into(), payload(10, 3));
        fs.files.insert("/rel/Sample/s.mkv".into(), payload(50, 4));
        fs.files.insert("/rel/CD1/y.bin".into(), payload(40, 5));
        fs.files.insert("/rel/CD1/deeper/z.bin".into(), payload(40, 6));
    }
    let dir = temp_dir();
    let mut q = Queue::new();
    let root = q.push(Endpoint::remote("A", "/rel"), Endpoint::local(local(&dir.join("rel"))), true, None, None);
    let options = ItemOptions {
        filter: TransferFilter::parse("", "Sample", "", "*.nfo", false).unwrap(),
        depth: Some(1),
        file_sort: SortOrder { key: SortKey::Size, descending: false },
        no_skiplist: true,
        repeat: 1,
        ..ItemOptions::default()
    };
    q.set_options(root, Some(std::sync::Arc::new(options)), None);
    // The skip list of the options would drop *.sfv: RS_NOSKIP keeps it.
    let skiplist = SkipList { rules: vec![SkipRule::skip("*.sfv", RuleTarget::Files)], ..SkipList::default() };
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { skiplist, retry_delay: Duration::from_millis(400), ..opts() });
    let e = r.engine.clone();
    let run = tokio::spawn(async move { e.run().await });
    // A file arrives after the first listing: the refresh catches it.
    tokio::time::sleep(Duration::from_millis(150)).await;
    srv.fs.lock().unwrap().files.insert("/rel/late.r00".into(), payload(30, 7));
    tokio::time::timeout(Duration::from_secs(10), run).await.unwrap().unwrap();

    let got = |p: &str| dir.join("rel").join(p).exists();
    assert!(got("a.rar") && got("c.sfv") && got("CD1/y.bin") && got("late.r00"));
    assert!(!got("b.nfo"), "file skip mask");
    assert!(!got("Sample"), "folder skip mask");
    assert!(!got("CD1/deeper"), "depth 1: CD1 is entered, not its sub-folders");
    let refreshes = r.engine.queue().lock().items().iter().filter(|i| i.is_dir && i.src.path == "/rel").count();
    assert_eq!(refreshes, 2, "the folder and one refresh");

    // The complete flag is already there: no refresh is queued.
    srv.fs.lock().unwrap().mkdir_p("/done/[ COMPLETE ]");
    let mut q = Queue::new();
    let root = q.push(Endpoint::remote("A", "/done"), Endpoint::local(local(&dir.join("done"))), true, None, None);
    let flag = Some(urftp_core::pattern::Pattern::regex("COMPLETE").unwrap());
    q.set_options(root, Some(std::sync::Arc::new(ItemOptions { repeat: 5, complete_flag: flag, ..ItemOptions::default() })), None);
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], opts());
    r.engine.run().await;
    let folders = r.engine.queue().lock().items().iter().filter(|i| i.src.path == "/done").count();
    assert_eq!(folders, 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn synchronisation_sends_what_differs_and_removes_or_copies_back_the_rest() {
    use urftp_core::itemopts::{ItemOptions, SyncOptions};
    let srv = MockServer::start(MockConfig::default()).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/R");
        fs.files.insert("/R/same.txt".into(), payload(10, 1));
        fs.files.insert("/R/changed.txt".into(), payload(4, 2));
        fs.files.insert("/R/old.txt".into(), payload(3, 3));
    }
    let dir = temp_dir();
    let l = dir.join("L");
    std::fs::create_dir_all(l.join("sub")).unwrap();
    std::fs::write(l.join("same.txt"), payload(10, 1)).unwrap();
    std::fs::write(l.join("changed.txt"), payload(8, 2)).unwrap();
    std::fs::write(l.join("new.txt"), payload(5, 4)).unwrap();
    std::fs::write(l.join("sub").join("c.txt"), payload(6, 5)).unwrap();

    let sync = |delete_missing, both_sides| ItemOptions {
        sync: Some(SyncOptions { compare_size: true, delete_missing, both_sides, ..SyncOptions::default() }),
        ..ItemOptions::default()
    };
    let mut q = Queue::new();
    let root = q.push(Endpoint::local(local(&l)), Endpoint::remote("A", "/R"), true, None, None);
    q.set_options(root, Some(std::sync::Arc::new(sync(true, false))), None);
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], opts());
    r.engine.run().await;
    {
        let fs = srv.fs.lock().unwrap();
        assert_eq!(fs.files.get("/R/changed.txt").map(Vec::len), Some(8), "a different size is sent again");
        assert!(fs.files.contains_key("/R/new.txt") && fs.files.contains_key("/R/sub/c.txt"));
        assert!(!fs.files.contains_key("/R/old.txt"), "missing at the source: removed");
    }
    let sent: Vec<String> = r.engine.queue().lock().items().iter().filter(|i| !i.is_dir).map(|i| i.dst.path.clone()).collect();
    assert!(!sent.contains(&"/R/same.txt".to_string()), "the same file is not queued");
    assert!(drain(&mut r.events).iter().any(|e| matches!(e, EngineEvent::SyncDeleted { path, .. } if path == "/R/old.txt")));

    // Both sides: what only the server has comes back to the local folder.
    srv.fs.lock().unwrap().files.insert("/R/only-there.txt".into(), payload(7, 6));
    let mut q = Queue::new();
    let root = q.push(Endpoint::local(local(&l)), Endpoint::remote("A", "/R"), true, None, None);
    q.set_options(root, Some(std::sync::Arc::new(sync(false, true))), None);
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], opts());
    r.engine.run().await;
    assert_eq!(std::fs::read(l.join("only-there.txt")).unwrap(), payload(7, 6));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn transfer_rules_ascii_case_disk_dupes_compression_and_local_speed() {
    use urftp_core::NameCase;
    use urftp_core::exists::ExistsAction;
    use urftp_core::pattern::Pattern;
    use urftp_proto::engine::TransferRules;
    let srv = MockServer::start(MockConfig { mode_z: true, xdupe: true, ..MockConfig::default() }).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/in");
        fs.files.insert("/README.TXT".into(), b"line\r\n".to_vec());
        fs.files.insert("/big.bin".into(), payload(200_000, 9));
        fs.files.insert("/in/a.bin".into(), payload(3, 1));
        fs.files.insert("/in/c.bin".into(), payload(3, 2));
    }
    let dir = temp_dir();
    // ASCII by mask, lower-case local name, MODE Z, no speed limit for 127.0.0.1.
    let rules = TransferRules {
        ascii_masks: vec![Pattern::wildcard("*.txt")],
        download_case: NameCase::Lower,
        mode_z: Some(vec![Pattern::wildcard("*.zip")]),
        unlimited_local: true,
        ..TransferRules::default()
    };
    let mut q = Queue::new();
    q.push(Endpoint::remote("A", "/README.TXT"), Endpoint::local(local(&dir.join("README.TXT"))), false, None, None);
    q.push(Endpoint::remote("A", "/big.bin"), Endpoint::local(local(&dir.join("big.bin"))), false, None, None);
    let options = EngineOptions { rules, download_limit: RateLimiter::new(20_000), ..opts() };
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], options);
    let start = Instant::now();
    r.engine.run().await;
    assert!(start.elapsed() < Duration::from_secs(3), "the local site ignores the 20 KB/s limit");
    assert!(dir.join("readme.txt").exists(), "lower-case name");
    assert_eq!(std::fs::read(dir.join("big.bin")).unwrap(), payload(200_000, 9));
    let got = srv.received();
    let pos = |c: &str| got.iter().position(|l| l == c);
    assert!(pos("TYPE A").is_some() && pos("MODE Z").is_some());

    // Disk space: keeping more than the disk has makes the download fail.
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/big.bin"), Endpoint::local(local(&dir.join("again.bin"))), false, None, None);
    let rules = TransferRules { keep_free: u64::MAX / 4, ..TransferRules::default() };
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { rules, retries: 0, ..opts() });
    r.engine.run().await;
    assert!(matches!(state(&r.engine, id), ItemState::Failed(e) if e.contains("disk space")));

    // XDUPE: the refusal lists the dupes of the folder; the next dupe is not even tried.
    for n in ["a.bin", "c.bin", "new.bin"] {
        std::fs::write(dir.join(n), payload(10, 3)).unwrap();
    }
    let mut q = Queue::new();
    let ids: Vec<u64> = ["a.bin", "c.bin", "new.bin"]
        .iter()
        .map(|n| q.push(Endpoint::local(local(&dir.join(n))), Endpoint::remote("A", format!("/in/{n}")), false, None, None))
        .collect();
    let mut xsite = site("A", srv.addr.port(), 1);
    xsite.protocol = SiteProtocol::Ftp(FtpConfig { xdupe: true, ..FtpConfig::default() });
    let overwrite = ExistsRules {
        smaller: ExistsAction::Overwrite,
        same: ExistsAction::Overwrite,
        larger: ExistsAction::Overwrite,
        ..ExistsRules::default()
    };
    let r = engine(q, vec![xsite], EngineOptions { exists: urftp_core::exists::ExistsSet::all(overwrite), ..opts() });
    r.engine.run().await;
    assert_eq!(state(&r.engine, ids[0]), ItemState::Skipped(urftp_core::skiplist::SkipReason::Dupe));
    assert_eq!(state(&r.engine, ids[1]), ItemState::Skipped(urftp_core::skiplist::SkipReason::Dupe));
    assert_eq!(state(&r.engine, ids[2]), ItemState::Done);
    assert_eq!(srv.received().iter().filter(|l| l.starts_with("STOR")).count(), 2, "c.bin was never sent");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn crc_checks_sfv_downloads_xcrc_uploads_and_several_sfv() {
    use urftp_core::checksum::write_sfv;
    use urftp_proto::engine::TransferRules;
    let srv = MockServer::start(MockConfig { xcrc: true, ..MockConfig::default() }).await;
    let good = payload(4000, 1);
    let bad = payload(4000, 2);
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/rel");
        fs.files.insert("/rel/a.r00".into(), good.clone());
        fs.files.insert("/rel/b.r01".into(), bad.clone());
        // The SFV expects other contents for b.r01.
        let sfv = write_sfv(&[("a.r00".into(), crc32fast::hash(&good)), ("b.r01".into(), crc32fast::hash(&payload(4000, 3)))]);
        fs.files.insert("/rel/rel.sfv".into(), sfv.into_bytes());
    }
    let dir = temp_dir();
    let rules = TransferRules { check_sfv: true, readd_bad_crc: true, use_xcrc: true, ..TransferRules::default() };
    let mut q = Queue::new();
    q.push(Endpoint::remote("A", "/rel"), Endpoint::local(local(&dir.join("rel"))), true, None, None);
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { rules: rules.clone(), retries: 1, ..opts() });
    r.engine.run().await;
    let events = drain(&mut r.events);
    let checks: Vec<(String, bool)> = events
        .iter()
        .filter_map(|e| {
            if let EngineEvent::CrcChecked { path, ok, .. } = e {
                Some((Path::new(path).file_name().unwrap().to_string_lossy().into_owned(), *ok))
            } else {
                None
            }
        })
        .collect();
    assert!(checks.contains(&("a.r00".into(), true)));
    assert!(checks.contains(&("b.r01".into(), false)));
    let items = r.engine.queue().lock().items().to_vec();
    let b = items.iter().find(|i| i.src.path == "/rel/b.r01").unwrap();
    assert!(matches!(&b.state, ItemState::Failed(e) if e.contains("bad CRC")), "{:?}", b.state);
    assert!(!dir.join("rel").join("b.r01").exists(), "the bad file is removed");
    assert!(dir.join("rel").join("a.r00").exists());

    // XCRC after an upload.
    std::fs::write(dir.join("up.bin"), &good).unwrap();
    let mut q = Queue::new();
    let up = q.push(Endpoint::local(local(&dir.join("up.bin"))), Endpoint::remote("A", "/rel/up.bin"), false, None, None);
    let mut r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { rules: rules.clone(), ..opts() });
    r.engine.run().await;
    assert_eq!(state(&r.engine, up), ItemState::Done);
    assert!(drain(&mut r.events).iter().any(|e| matches!(e, EngineEvent::CrcChecked { ok: true, .. })));

    // Two SFV files in a folder: none of them is taken.
    srv.fs.lock().unwrap().files.insert("/rel/other.sfv".into(), b"; x".to_vec());
    let mut q = Queue::new();
    q.push(Endpoint::remote("A", "/rel"), Endpoint::local(local(&dir.join("rel2"))), true, None, None);
    let rules = TransferRules { one_sfv_per_folder: true, ..TransferRules::default() };
    let r = engine(q, vec![site("A", srv.addr.port(), 1)], EngineOptions { rules, ..opts() });
    r.engine.run().await;
    assert!(!r.engine.queue().lock().items().iter().any(|i| i.src.path.ends_with(".sfv")));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test]
async fn folder_rules_date_same_name_empty_folders_and_fxp_zero_target() {
    use urftp_proto::engine::TransferRules;
    let a = MockServer::start(MockConfig::default()).await;
    let b = MockServer::start(MockConfig::default()).await;
    {
        let mut fs = a.fs.lock().unwrap();
        fs.mkdir_p("/rel/rel");
        fs.mkdir_p("/rel/Sample");
        fs.files.insert("/rel/x.bin".into(), payload(100, 1));
        fs.files.insert("/rel/rel/dupe.bin".into(), payload(100, 2));
        fs.files.insert("/rel/Sample/s.jpg".into(), payload(100, 3));
    }
    b.fs.lock().unwrap().mkdir_p("/in");
    let dir = temp_dir();
    let rules = TransferRules {
        keep_server_date: true,
        refuse_same_name_dir: true,
        remove_empty_dirs: true,
        fxp_remove_zero_target: true,
        ..TransferRules::default()
    };
    // *.jpg skipped: Sample stays empty and is removed.
    let skiplist = SkipList { rules: vec![SkipRule::skip("*.jpg", RuleTarget::Files)], ..SkipList::default() };
    let mut q = Queue::new();
    q.push(Endpoint::remote("A", "/rel"), Endpoint::local(local(&dir.join("rel"))), true, None, None);
    let r = engine(q, vec![site("A", a.addr.port(), 1)], EngineOptions { rules: rules.clone(), skiplist, ..opts() });
    r.engine.run().await;
    let x = dir.join("rel").join("x.bin");
    let modified = std::fs::metadata(&x).unwrap().modified().unwrap();
    let mock_date = std::time::UNIX_EPOCH + Duration::from_secs(1_704_164_645); // 2024-01-02 03:04:05
    assert_eq!(modified, mock_date, "server date kept");
    assert!(!dir.join("rel").join("rel").exists(), "sub-folder named like its parent");
    assert!(!dir.join("rel").join("Sample").exists(), "empty folder removed");

    // FXP of a file the source refuses: the empty target file goes away.
    let mut q = Queue::new();
    let id = q.push(Endpoint::remote("A", "/rel/missing.bin"), Endpoint::remote("B", "/in/missing.bin"), false, Some(10), None);
    let r = engine(q, vec![site("A", a.addr.port(), 1), site("B", b.addr.port(), 1)], EngineOptions { rules, retries: 0, ..opts() });
    r.engine.run().await;
    assert!(matches!(state(&r.engine, id), ItemState::Failed(_)));
    assert!(!b.fs.lock().unwrap().files.contains_key("/in/missing.bin"));
    std::fs::remove_dir_all(&dir).unwrap();
}
