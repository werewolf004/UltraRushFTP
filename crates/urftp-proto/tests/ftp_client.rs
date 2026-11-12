// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use std::sync::{Arc, Mutex};

use common::{MockConfig, MockServer};
use urftp_proto::ftp::{EntryKind, FtpClient, FtpConfig, FtpError, LogLine, LogSink};

async fn connect(srv: &MockServer) -> FtpClient {
    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), FtpConfig::default()).await.unwrap();
    c.login("test", "secret").await.unwrap();
    c
}

#[tokio::test]
async fn full_session_upload_list_download_rename_delete() {
    let srv = MockServer::start(MockConfig::default()).await;
    let mut c = connect(&srv).await;
    assert_eq!(c.current_dir(), Some("/"));
    assert!(c.features().has("MLST"));

    c.mkdir("/pub").await.unwrap();
    c.mkdir("/pub/Releases").await.unwrap();

    let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let mut last = 0;
    let sent = c.stor("/pub/data.bin", 0, &mut payload.as_slice(), |n| last = n).await.unwrap();
    assert_eq!(sent, payload.len() as u64);
    assert_eq!(last, sent);
    assert_eq!(c.size("/pub/data.bin").await.unwrap(), payload.len() as u64);
    assert_eq!(c.mdtm("/pub/data.bin").await.unwrap().year, Some(2024));

    let entries = c.list(Some("/pub")).await.unwrap();
    let dir = entries.iter().find(|e| e.name == "Releases").unwrap();
    assert_eq!(dir.kind, EntryKind::Dir);
    let file = entries.iter().find(|e| e.name == "data.bin").unwrap();
    assert_eq!(file.size, payload.len() as u64);

    let mut back = Vec::new();
    c.retr("/pub/data.bin", 0, &mut back, |_| {}).await.unwrap();
    assert_eq!(back, payload);

    c.rename("/pub/data.bin", "renamed.bin").await.unwrap();
    c.delete("/pub/renamed.bin").await.unwrap();
    c.rmdir("/pub/Releases").await.unwrap();
    assert!(srv.fs.lock().unwrap().files.is_empty());
    c.quit().await.unwrap();
}

#[tokio::test]
async fn resume_download_and_upload() {
    let srv = MockServer::start(MockConfig::default()).await;
    let mut c = connect(&srv).await;
    let full: Vec<u8> = (0..100_000u32).map(|i| (i % 199) as u8).collect();

    // Upload interrupted at 40,000 bytes, then resumed.
    c.stor("/f.bin", 0, &mut &full[..40_000], |_| {}).await.unwrap();
    let remote = c.size("/f.bin").await.unwrap();
    assert_eq!(remote, 40_000);
    c.stor("/f.bin", remote, &mut &full[40_000..], |_| {}).await.unwrap();
    assert_eq!(srv.fs.lock().unwrap().files["/f.bin"], full);

    // Download resumed at 70,000 bytes.
    let mut tail = Vec::new();
    let n = c.retr("/f.bin", 70_000, &mut tail, |_| {}).await.unwrap();
    assert_eq!(n, 30_000);
    assert_eq!(tail, &full[70_000..]);
    assert!(srv.received().iter().any(|l| l == "REST 70000"));
}

#[tokio::test]
async fn long_remote_paths_fall_back_to_step_by_step_cwd() {
    // The server refuses any argument longer than 100 characters.
    let srv = MockServer::start(MockConfig { max_arg_len: Some(100), ..MockConfig::default() }).await;
    let deep: String = (0..10).map(|i| format!("/repertoire_{i:02}_{}", "z".repeat(40))).collect();
    assert!(deep.len() > 500);
    srv.fs.lock().unwrap().mkdir_p(&deep);

    let mut c = connect(&srv).await;
    let file = format!("{deep}/release.nfo");
    c.stor(&file, 0, &mut &b"contenu"[..], |_| {}).await.unwrap();
    assert_eq!(c.current_dir(), Some(deep.as_str()));

    let mut back = Vec::new();
    c.retr(&file, 0, &mut back, |_| {}).await.unwrap();
    assert_eq!(back, b"contenu");
    let names: Vec<String> = c.list(Some(&deep)).await.unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["release.nfo"]);
    // No command sent exceeded the limit, except the first CWD tried as a whole.
    let too_long: Vec<_> = srv.received().into_iter().filter(|l| l.len() > 105).collect();
    assert_eq!(too_long.len(), 1, "{too_long:?}");
}

#[tokio::test]
async fn epsv_refused_falls_back_to_pasv_and_list_without_mlsd() {
    let srv = MockServer::start(MockConfig { epsv: false, mlsd: false, ..MockConfig::default() }).await;
    let mut c = connect(&srv).await;
    c.stor("/a.txt", 0, &mut &b"abc"[..], |_| {}).await.unwrap();
    c.stor("/b.txt", 0, &mut &b"de"[..], |_| {}).await.unwrap();
    let list = c.list(None).await.unwrap();
    assert_eq!(list.len(), 2);
    let cmds = srv.received();
    assert_eq!(cmds.iter().filter(|l| *l == "EPSV").count(), 1, "EPSV must be tried only once");
    assert!(cmds.iter().any(|l| l == "LIST"));
}

#[tokio::test]
async fn bad_password_and_missing_file() {
    let srv = MockServer::start(MockConfig::default()).await;
    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), FtpConfig::default()).await.unwrap();
    let err = c.login("test", "faux").await.unwrap_err();
    assert_eq!(err.reply_code(), Some(530));

    let mut c = connect(&srv).await;
    let err = c.retr("/absent.bin", 0, &mut Vec::new(), |_| {}).await.unwrap_err();
    assert_eq!(err.reply_code(), Some(550));
    // The connection stays usable after an error.
    c.noop().await.unwrap();
}

#[tokio::test]
async fn crlf_injection_is_rejected_and_password_not_logged() {
    let srv = MockServer::start(MockConfig::default()).await;
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink_lines = lines.clone();
    let sink: LogSink = Arc::new(move |l| sink_lines.lock().unwrap().push(l));

    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), FtpConfig::default()).await.unwrap();
    c.set_logger(Some(sink));
    c.login("test", "secret").await.unwrap();

    let err = c.mkdir("/x\r\nDELE /important").await.unwrap_err();
    assert!(matches!(err, FtpError::InvalidArgument(_)));
    assert!(!srv.received().iter().any(|l| l.starts_with("DELE")));

    let logged = lines.lock().unwrap();
    assert!(logged.contains(&LogLine::Command("PASS ****".into())));
    assert!(!logged.iter().any(|l| matches!(l, LogLine::Command(c) if c.contains("secret"))));
}

#[tokio::test]
async fn download_to_long_local_path() {
    let srv = MockServer::start(MockConfig::default()).await;
    srv.fs.lock().unwrap().files.insert("/big.bin".into(), vec![7u8; 10_000]);
    let mut c = connect(&srv).await;

    let root = std::env::temp_dir().join(format!("urftp-dl-{}", std::process::id()));
    let mut dir = root.clone();
    for i in 0..10 {
        dir.push(format!("telechargement_long_{i:02}_{}", "y".repeat(25)));
    }
    std::fs::create_dir_all(&dir).unwrap();
    let local = dir.join("big.bin");
    assert!(local.as_os_str().len() > 400);

    let mut f = tokio::fs::File::create(&local).await.unwrap();
    c.retr("/big.bin", 0, &mut f, |_| {}).await.unwrap();
    drop(f);
    assert_eq!(std::fs::read(&local).unwrap(), vec![7u8; 10_000]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn mode_z_ascii_and_xcrc() {
    use urftp_proto::ftp::DataOptions;
    let srv = MockServer::start(MockConfig { mode_z: true, xcrc: true, ..MockConfig::default() }).await;
    // Larger than the buffers, half compressible, half random.
    let mut data: Vec<u8> = (0..300_000u32).map(|i| (i % 7) as u8).collect();
    data.extend((0..300_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8));
    srv.fs.lock().unwrap().files.insert("/big.bin".into(), data.clone());
    let mut c = connect(&srv).await;
    let z = DataOptions { compress: true, ..DataOptions::default() };

    let mut got = Vec::new();
    let n = c.retr_with("/big.bin", 0, &mut got, |_| {}, z).await.unwrap();
    assert_eq!((n as usize, got == data), (data.len(), true), "decompressed download");
    let mut input = &data[..];
    c.stor_with("/up.bin", 0, &mut input, |_| {}, z).await.unwrap();
    assert!(srv.fs.lock().unwrap().files.get("/up.bin") == Some(&data), "compressed upload");
    assert!(srv.received().iter().filter(|l| l.as_str() == "MODE Z").count() == 1, "MODE Z sent once");

    // Back to stream mode and ASCII.
    let mut text = Vec::new();
    c.retr_with("/big.bin", 0, &mut text, |_| {}, DataOptions { ascii: true, compress: false }).await.unwrap();
    assert!(srv.received().iter().any(|l| l == "MODE S") && srv.received().iter().any(|l| l == "TYPE A"));

    assert_eq!(c.xcrc("/big.bin").await.unwrap(), Some(crc32fast::hash(&data)));
    let plain = MockServer::start(MockConfig::default()).await;
    plain.fs.lock().unwrap().files.insert("/a".into(), b"x".to_vec());
    let mut c = connect(&plain).await;
    assert_eq!(c.xcrc("/a").await.unwrap(), None, "no XCRC: None");
    let mut got = Vec::new();
    c.retr_with("/a", 0, &mut got, |_| {}, z).await.unwrap();
    assert_eq!(got, b"x", "MODE Z not announced: plain transfer");
    assert!(!plain.received().iter().any(|l| l.starts_with("MODE")));
}

#[tokio::test]
async fn mode_z_listings_are_compressed_or_back_in_stream_mode() {
    use urftp_proto::ftp::DataOptions;
    let srv = MockServer::start(MockConfig { mode_z: true, ..MockConfig::default() }).await;
    srv.fs.lock().unwrap().files.insert("/a.bin".into(), vec![1u8; 5000]);
    // Listings not compressed: a compressed download leaves MODE Z, the listing goes back
    // to MODE S and reads plain text.
    let mut c = connect(&srv).await;
    let mut got = Vec::new();
    c.retr_with("/a.bin", 0, &mut got, |_| {}, DataOptions { compress: true, ..DataOptions::default() }).await.unwrap();
    let names: Vec<String> = c.list(None).await.unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["a.bin"]);
    assert!(srv.received().iter().any(|l| l == "MODE S"));
    // Listings compressed.
    let cfg = FtpConfig { compress_listings: true, ..FtpConfig::default() };
    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), cfg).await.unwrap();
    c.login("test", "secret").await.unwrap();
    let before = srv.received().iter().filter(|l| l.as_str() == "MODE Z").count();
    let names: Vec<String> = c.list(None).await.unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["a.bin"], "compressed listing read");
    assert_eq!(srv.received().iter().filter(|l| l.as_str() == "MODE Z").count(), before + 1);
}
