// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Listing and network settings: `LIST -a`, `STAT -l`, trimmed spaces, `PORT` fallback,
//! time zone, character set, local port range and local address.
mod common;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use common::{MockConfig, MockServer};
use urftp_proto::ftp::{FtpClient, FtpConfig, FtpError, FtpTime};

/// Mock without `MLSD`, so that the `LIST` settings apply.
fn plain() -> MockConfig {
    MockConfig { mlsd: false, ..MockConfig::default() }
}

async fn connect(srv: &MockServer, config: FtpConfig) -> FtpClient {
    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), config).await.unwrap();
    c.login("test", "secret").await.unwrap();
    c
}

fn names(entries: &[urftp_proto::ftp::RemoteEntry]) -> Vec<String> {
    let mut v: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    v.sort();
    v
}

#[tokio::test]
async fn hidden_files_need_list_a() {
    let srv = MockServer::start(plain()).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.files.insert("/.hidden".into(), vec![1; 3]);
        fs.files.insert("/shown.txt".into(), vec![1; 3]);
    }
    let mut c = connect(&srv, FtpConfig::default()).await;
    assert_eq!(names(&c.list(None).await.unwrap()), ["shown.txt"]);
    let mut c = connect(&srv, FtpConfig { list_command: "LIST -a".into(), ..FtpConfig::default() }).await;
    assert_eq!(names(&c.list(None).await.unwrap()), [".hidden", "shown.txt"]);
    assert!(srv.received().iter().any(|l| l == "LIST -a"));
}

#[tokio::test]
async fn stat_l_lists_on_the_control_channel() {
    let srv = MockServer::start(MockConfig { stat_list: true, ..plain() }).await;
    {
        let mut fs = srv.fs.lock().unwrap();
        fs.mkdir_p("/pub/dir");
        fs.files.insert("/pub/a b.txt".into(), vec![0; 42]);
    }
    let mut c = connect(&srv, FtpConfig { list_command: "STAT -l".into(), ..FtpConfig::default() }).await;
    let entries = c.list(Some("/pub")).await.unwrap();
    assert_eq!(names(&entries), ["a b.txt", "dir"]);
    assert_eq!(entries.iter().find(|e| e.name == "a b.txt").map(|e| e.size), Some(42));
    let received = srv.received();
    assert!(!received.iter().any(|l| l == "PASV" || l == "EPSV" || l.starts_with("LIST")), "no data connection: {received:?}");
}

#[tokio::test]
async fn trimmed_spaces_keep_a_padded_listing_readable() {
    let srv = MockServer::start(MockConfig { padded_list: true, stat_list: true, ..plain() }).await;
    srv.fs.lock().unwrap().files.insert("/f.bin".into(), vec![0; 5]);
    let mut c = connect(&srv, FtpConfig::default()).await;
    assert!(c.list(None).await.unwrap().is_empty(), "padded lines are not understood as they are");
    let mut c = connect(&srv, FtpConfig { trim_list_spaces: true, ..FtpConfig::default() }).await;
    assert_eq!(names(&c.list(None).await.unwrap()), ["f.bin"]);
    // `STAT` is always trimmed, whatever the case of the command.
    let mut c = connect(&srv, FtpConfig { list_command: "stat -l".into(), ..FtpConfig::default() }).await;
    assert_eq!(names(&c.list(None).await.unwrap()), ["f.bin"]);
}

#[tokio::test]
async fn passive_failure_switches_to_port() {
    let srv = MockServer::start(MockConfig { refuse_pasv: true, ..plain() }).await;
    srv.fs.lock().unwrap().files.insert("/x.bin".into(), vec![7; 1000]);
    let mut c = connect(&srv, FtpConfig::default()).await;
    assert!(matches!(c.list(None).await, Err(FtpError::Unexpected { .. })), "without fallback the listing fails");

    let mut c = connect(&srv, FtpConfig { port_fallback: true, ..FtpConfig::default() }).await;
    assert_eq!(names(&c.list(None).await.unwrap()), ["x.bin"]);
    let mut out = Vec::new();
    c.retr("/x.bin", 0, &mut out, |_| {}).await.unwrap();
    assert_eq!(out, vec![7; 1000]);
    let received = srv.received();
    let pasv = received.iter().filter(|l| *l == "PASV").count();
    assert_eq!(pasv, 2, "passive tried once per connection, then active kept: {received:?}");
    assert_eq!(received.iter().filter(|l| l.starts_with("PORT")).count(), 2);
}

#[tokio::test]
async fn time_offset_moves_the_dates() {
    let srv = MockServer::start(plain()).await;
    srv.fs.lock().unwrap().files.insert("/t.txt".into(), vec![0; 1]);
    let mut c = connect(&srv, FtpConfig { time_offset_mins: -13 * 60, ..FtpConfig::default() }).await;
    let e = c.list(None).await.unwrap();
    // `Jan 01 12:00` minus 13 hours.
    assert_eq!(e[0].modified, Some(FtpTime { year: None, month: 12, day: 31, hour: 23, minute: 0, second: 0 }));
}

#[tokio::test]
async fn latin1_server_names() {
    let srv = MockServer::start(MockConfig { latin1: true, ..plain() }).await;
    srv.fs.lock().unwrap().files.insert("/café.txt".into(), b"ok".to_vec());
    let latin1 = encoding_rs::Encoding::for_label(b"ISO-8859-1");
    let mut c = connect(&srv, FtpConfig { charset: latin1, ..FtpConfig::default() }).await;
    assert_eq!(names(&c.list(None).await.unwrap()), ["café.txt"]);
    let mut out = Vec::new();
    c.retr("/café.txt", 0, &mut out, |_| {}).await.unwrap();
    assert_eq!(out, b"ok");
    assert!(!srv.received().iter().any(|l| l.starts_with("OPTS UTF8")), "the server stays in its character set");
    let e = c.mkdir("/日本").await.expect_err("not writable in Latin-1");
    assert!(matches!(e, FtpError::InvalidArgument(_)), "{e:?}");
}

#[tokio::test]
async fn active_mode_uses_the_port_range() {
    let srv = MockServer::start(plain()).await;
    srv.fs.lock().unwrap().files.insert("/r.bin".into(), vec![0; 10]);
    // A range that is free right now.
    let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let range = (first, first.saturating_add(20));
    let mut c = connect(&srv, FtpConfig { active_mode: true, active_port_range: Some(range), ..FtpConfig::default() }).await;
    assert_eq!(names(&c.list(None).await.unwrap()), ["r.bin"]);
    let port_line = srv.received().into_iter().find(|l| l.starts_with("PORT")).unwrap();
    let n: Vec<u16> = port_line[5..].split(',').map(|x| x.parse().unwrap()).collect();
    let port = n[4] * 256 + n[5];
    assert!((range.0..=range.1).contains(&port), "{port} outside {range:?}");
}

#[tokio::test]
async fn connections_leave_from_the_local_address() {
    let srv = MockServer::start(plain()).await;
    srv.fs.lock().unwrap().files.insert("/b.bin".into(), vec![0; 10]);
    let mut c = connect(&srv, FtpConfig { bind_ip: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)), ..FtpConfig::default() }).await;
    assert_eq!(names(&c.list(None).await.unwrap()), ["b.bin"]);
    let wrong = FtpClient::connect(
        "127.0.0.1",
        srv.addr.port(),
        FtpConfig { bind_ip: Some(IpAddr::V6(Ipv6Addr::LOCALHOST)), ..FtpConfig::default() },
    )
    .await;
    assert!(wrong.is_err(), "an IPv6 local address cannot reach an IPv4 server");
}

#[tokio::test]
async fn noop_keeps_the_control_connection_alive_during_transfers() {
    let srv = MockServer::start(plain()).await;
    let data: Vec<u8> = (0..4_000_000u32).map(|i| (i % 253) as u8).collect();
    srv.fs.lock().unwrap().files.insert("/big.bin".into(), data.clone());
    let cfg = FtpConfig { noop_during_transfer: Some(std::time::Duration::from_millis(1)), ..FtpConfig::default() };
    let mut c = connect(&srv, cfg).await;
    let mut out = Vec::new();
    c.retr("/big.bin", 0, &mut out, |_| {}).await.unwrap();
    assert_eq!(out, data);
    c.stor("/up.bin", 0, &mut data.as_slice(), |_| {}).await.unwrap();
    // The replies of the NOOP were all read: the next commands get their own reply.
    assert_eq!(c.size("/up.bin").await.unwrap(), data.len() as u64);
    assert_eq!(names(&c.list(None).await.unwrap()), ["big.bin", "up.bin"]);
    let noops = srv.received().iter().filter(|l| *l == "NOOP").count();
    assert!(noops >= 2, "NOOP sent during the transfers: {noops}");
}
