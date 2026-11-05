// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use common::{MockConfig, MockServer, MockTls};
use urftp_proto::ftp::{FtpClient, FtpConfig, FxpMode, FxpOptions, FxpSecurity, Security, TlsOptions, TlsVerify, fxp};

fn payload(n: u32) -> Vec<u8> {
    (0..n).map(|i| (i % 227) as u8).collect()
}

fn tls_config() -> FtpConfig {
    FtpConfig { security: Security::Explicit(TlsOptions { verify: TlsVerify::AcceptAny, ..TlsOptions::default() }), ..FtpConfig::default() }
}

async fn login(srv: &MockServer, cfg: FtpConfig) -> FtpClient {
    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), cfg).await.unwrap();
    c.login("test", "secret").await.unwrap();
    c
}

/// Two servers, one file on the source.
async fn pair(src_cfg: MockConfig, dst_cfg: MockConfig, data: &[u8]) -> (MockServer, MockServer) {
    let src = MockServer::start(src_cfg).await;
    let dst = MockServer::start(dst_cfg).await;
    src.fs.lock().unwrap().mkdir_p("/pub");
    src.fs.lock().unwrap().files.insert("/pub/release.rar".into(), data.to_vec());
    dst.fs.lock().unwrap().mkdir_p("/incoming");
    (src, dst)
}

fn dst_file(dst: &MockServer) -> Vec<u8> {
    dst.fs.lock().unwrap().files.get("/incoming/release.rar").cloned().unwrap_or_default()
}

#[tokio::test]
async fn plain_fxp_source_passive() {
    let data = payload(500_000);
    let (s, d) = pair(MockConfig::default(), MockConfig::default(), &data).await;
    let (mut a, mut b) = (login(&s, FtpConfig::default()).await, login(&d, FtpConfig::default()).await);

    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap();
    assert_eq!((out.mode, out.reversed, out.encrypted), (FxpMode::SourcePassive, false, false));
    assert_eq!(dst_file(&d), data);
    assert!(s.received().iter().any(|l| l == "PASV"));
    assert!(d.received().iter().any(|l| l.starts_with("PORT 127,0,0,1,")));
    // Les deux connexions restent utilisables.
    a.noop().await.unwrap();
    b.noop().await.unwrap();
}

#[tokio::test]
async fn destination_passive_mode() {
    let data = payload(10_000);
    let (s, d) = pair(MockConfig::default(), MockConfig::default(), &data).await;
    let (mut a, mut b) = (login(&s, FtpConfig::default()).await, login(&d, FtpConfig::default()).await);
    let opts = FxpOptions { mode: FxpMode::DestinationPassive, ..FxpOptions::default() };
    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &opts).await.unwrap();
    assert_eq!(out.mode, FxpMode::DestinationPassive);
    assert_eq!(dst_file(&d), data);
    assert!(d.received().iter().any(|l| l == "PASV"));
    assert!(s.received().iter().any(|l| l.starts_with("PORT ")));
}

#[tokio::test]
async fn auto_reverse_when_port_is_refused() {
    let data = payload(20_000);
    let refuse = MockConfig { refuse_port: true, ..MockConfig::default() };
    let (s, d) = pair(MockConfig::default(), refuse, &data).await;
    let (mut a, mut b) = (login(&s, FtpConfig::default()).await, login(&d, FtpConfig::default()).await);

    let no_reverse = FxpOptions { auto_reverse: false, ..FxpOptions::default() };
    let err = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &no_reverse).await.unwrap_err();
    assert_eq!(err.reply_code(), Some(500));

    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap();
    assert_eq!((out.mode, out.reversed), (FxpMode::DestinationPassive, true));
    assert_eq!(dst_file(&d), data);
}

#[tokio::test]
async fn secure_fxp_with_sscn() {
    let data = payload(300_000);
    let tls = MockConfig { tls: MockTls::Explicit, sscn: true, require_prot_p: true, ..MockConfig::default() };
    let (s, d) = pair(tls.clone(), tls, &data).await;
    let (mut a, mut b) = (login(&s, tls_config()).await, login(&d, tls_config()).await);

    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap();
    assert!(out.encrypted);
    assert_eq!(dst_file(&d), data);
    let cmds = s.received();
    let on = cmds.iter().position(|l| l == "SSCN ON").expect("SSCN ON on the passive source");
    let off = cmds.iter().position(|l| l == "SSCN OFF").expect("SSCN OFF after the transfer");
    assert!(on < off);
    assert_eq!(s.stats.lock().unwrap().tls_client_channels, 1);
    assert!(d.data_channels().iter().all(|(enc, _)| *enc));

    // After the FXP, a normal download from the source works (SSCN disabled).
    let mut back = Vec::new();
    a.retr("/pub/release.rar", 0, &mut back, |_| {}).await.unwrap();
    assert_eq!(back, data);
}

#[tokio::test]
async fn secure_fxp_with_cpsv() {
    let data = payload(50_000);
    let tls = MockConfig { tls: MockTls::Explicit, cpsv: true, require_prot_p: true, ..MockConfig::default() };
    let (s, d) = pair(tls.clone(), tls, &data).await;
    let (mut a, mut b) = (login(&s, tls_config()).await, login(&d, tls_config()).await);
    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap();
    assert!(out.encrypted);
    assert_eq!(dst_file(&d), data);
    assert!(s.received().iter().any(|l| l == "CPSV"));
    assert!(!s.received().iter().any(|l| l.starts_with("SSCN ON")));
}

#[tokio::test]
async fn no_secure_method_fails_or_falls_back_to_clear() {
    let data = payload(30_000);
    let tls = MockConfig { tls: MockTls::Explicit, ..MockConfig::default() };
    let (s, d) = pair(tls.clone(), tls, &data).await;
    let (mut a, mut b) = (login(&s, tls_config()).await, login(&d, tls_config()).await);

    let err = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap_err();
    assert_eq!(err.reply_code(), Some(502), "{err}");
    assert!(a.is_data_protected() && b.is_data_protected(), "PROT P restored after the failure");

    let opts = FxpOptions { allow_clear_fallback: true, ..FxpOptions::default() };
    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &opts).await.unwrap();
    assert!(!out.encrypted);
    assert_eq!(dst_file(&d), data);
    assert!(a.is_data_protected() && b.is_data_protected(), "PROT P restored after the transfer");
    assert!(s.received().iter().any(|l| l == "PROT C"));
}

#[tokio::test]
async fn mixed_tls_and_plain_servers_transfer_in_clear() {
    let data = payload(12_345);
    let (s, d) = pair(MockConfig { tls: MockTls::Explicit, ..MockConfig::default() }, MockConfig::default(), &data).await;
    let (mut a, mut b) = (login(&s, tls_config()).await, login(&d, FtpConfig::default()).await);
    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap();
    assert!(!out.encrypted);
    assert_eq!(dst_file(&d), data);
    assert!(a.is_data_protected());
}

#[tokio::test]
async fn pret_is_sent_to_the_passive_side() {
    let data = payload(1_000);
    let (s, d) = pair(MockConfig { pret: true, ..MockConfig::default() }, MockConfig::default(), &data).await;
    let (mut a, mut b) = (login(&s, FtpConfig::default()).await, login(&d, FtpConfig::default()).await);
    fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap();
    let cmds = s.received();
    let pret = cmds.iter().position(|l| l == "PRET RETR release.rar").unwrap();
    let pasv = cmds.iter().rposition(|l| l == "PASV").unwrap();
    assert!(pret < pasv);
}

#[tokio::test]
async fn resume_fxp_from_offset() {
    let data = payload(80_000);
    let (s, d) = pair(MockConfig::default(), MockConfig::default(), &data).await;
    d.fs.lock().unwrap().files.insert("/incoming/release.rar".into(), data[..30_000].to_vec());
    let (mut a, mut b) = (login(&s, FtpConfig::default()).await, login(&d, FtpConfig::default()).await);
    let offset = b.size("/incoming/release.rar").await.unwrap();
    let opts = FxpOptions { offset, ..FxpOptions::default() };
    fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &opts).await.unwrap();
    assert_eq!(dst_file(&d), data);
    assert!(s.received().iter().any(|l| l == "REST 30000"));
    assert!(d.received().iter().any(|l| l == "REST 30000"));
}

#[tokio::test]
async fn missing_source_file_aborts_destination() {
    let (s, d) = pair(MockConfig::default(), MockConfig::default(), b"x").await;
    let (mut a, mut b) = (login(&s, FtpConfig::default()).await, login(&d, FtpConfig::default()).await);
    let err = fxp(&mut a, "/pub/absent.rar", &mut b, "/incoming/absent.rar", &FxpOptions::default()).await.unwrap_err();
    assert_eq!(err.reply_code(), Some(550));
    assert!(d.received().iter().any(|l| l == "ABOR"));
    a.noop().await.unwrap();
    b.noop().await.unwrap();
    // The connections stay in sync: a second FXP succeeds.
    fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &FxpOptions::default()).await.unwrap();
    assert_eq!(dst_file(&d), b"x");
}

#[tokio::test]
async fn fxp_with_long_paths_on_limited_servers() {
    let data = payload(4_000);
    let limited = MockConfig { max_arg_len: Some(80), ..MockConfig::default() };
    let src = MockServer::start(limited.clone()).await;
    let dst = MockServer::start(limited).await;
    let deep_s: String = (0..8).map(|i| format!("/source_{i}_{}", "s".repeat(40))).collect();
    let deep_d: String = (0..8).map(|i| format!("/cible_{i}_{}", "d".repeat(40))).collect();
    src.fs.lock().unwrap().mkdir_p(&deep_s);
    src.fs.lock().unwrap().files.insert(format!("{deep_s}/f.bin"), data.clone());
    dst.fs.lock().unwrap().mkdir_p(&deep_d);
    let (mut a, mut b) = (login(&src, FtpConfig::default()).await, login(&dst, FtpConfig::default()).await);
    fxp(&mut a, &format!("{deep_s}/f.bin"), &mut b, &format!("{deep_d}/f.bin"), &FxpOptions::default()).await.unwrap();
    assert_eq!(dst.fs.lock().unwrap().files[&format!("{deep_d}/f.bin")], data);
}

#[tokio::test]
async fn clear_security_forces_plain_data_between_tls_servers() {
    let data = payload(7_000);
    let tls = MockConfig { tls: MockTls::Explicit, sscn: true, ..MockConfig::default() };
    let (s, d) = pair(tls.clone(), tls, &data).await;
    let (mut a, mut b) = (login(&s, tls_config()).await, login(&d, tls_config()).await);
    let opts = FxpOptions { security: FxpSecurity::Clear, ..FxpOptions::default() };
    let out = fxp(&mut a, "/pub/release.rar", &mut b, "/incoming/release.rar", &opts).await.unwrap();
    assert!(!out.encrypted);
    assert_eq!(dst_file(&d), data);
    assert!(!s.received().iter().any(|l| l == "SSCN ON"));
    assert!(a.is_data_protected() && b.is_data_protected());
}
