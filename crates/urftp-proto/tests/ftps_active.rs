// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use common::{MockConfig, MockServer, MockTls};
use urftp_proto::ftp::{FtpClient, FtpConfig, FtpError, Security, TlsOptions, TlsVerify};

fn tls(verify: TlsVerify) -> TlsOptions {
    TlsOptions { verify, ..TlsOptions::default() }
}

fn config(security: Security) -> FtpConfig {
    FtpConfig { security, ..FtpConfig::default() }
}

/// Full upload, listing and download; returns the client for further checks.
async fn roundtrip(srv: &MockServer, cfg: FtpConfig) -> FtpClient {
    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), cfg).await.unwrap();
    c.login("test", "secret").await.unwrap();
    let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 241) as u8).collect();
    c.stor("/x.bin", 0, &mut payload.as_slice(), |_| {}).await.unwrap();
    let list = c.list(None).await.unwrap();
    assert_eq!(list.iter().find(|e| e.name == "x.bin").unwrap().size, payload.len() as u64);
    let mut back = Vec::new();
    c.retr("/x.bin", 0, &mut back, |_| {}).await.unwrap();
    assert_eq!(back, payload);
    c
}

#[tokio::test]
async fn explicit_tls_encrypts_control_and_data() {
    let srv = MockServer::start(MockConfig { tls: MockTls::Explicit, require_prot_p: true, ..MockConfig::default() }).await;
    let c = roundtrip(&srv, config(Security::Explicit(tls(TlsVerify::AcceptAny)))).await;
    assert!(c.is_secure() && c.is_data_protected());
    assert_eq!(c.peer_certificate_fingerprint(), srv.fingerprint);

    let cmds = srv.received();
    let pos = |s: &str| cmds.iter().position(|l| l == s).unwrap_or_else(|| panic!("{s} absent : {cmds:?}"));
    assert!(pos("AUTH TLS") < pos("USER test"));
    assert!(pos("PASS secret") < pos("PBSZ 0") && pos("PBSZ 0") < pos("PROT P"));
    let channels = srv.data_channels();
    assert_eq!(channels.len(), 3);
    assert!(channels.iter().all(|(encrypted, _)| *encrypted));
}

#[tokio::test]
async fn implicit_tls() {
    let srv = MockServer::start(MockConfig { tls: MockTls::Implicit, ..MockConfig::default() }).await;
    let c = roundtrip(&srv, config(Security::Implicit(tls(TlsVerify::AcceptAny)))).await;
    assert!(c.is_secure() && c.is_data_protected());
    assert!(!srv.received().iter().any(|l| l.starts_with("AUTH")));
}

#[tokio::test]
async fn data_channels_reuse_the_tls_session() {
    // Like vsftpd with require_ssl_reuse=YES.
    let srv = MockServer::start(MockConfig { tls: MockTls::Explicit, require_session_reuse: true, ..MockConfig::default() }).await;
    roundtrip(&srv, config(Security::Explicit(tls(TlsVerify::AcceptAny)))).await;
    let channels = srv.data_channels();
    assert!(channels.iter().all(|&(enc, resumed)| enc && resumed), "{channels:?}");
}

#[tokio::test]
async fn clear_data_channel_when_protection_disabled() {
    let srv = MockServer::start(MockConfig { tls: MockTls::Explicit, ..MockConfig::default() }).await;
    let opts = TlsOptions { protect_data: false, protect_listings: false, ..tls(TlsVerify::AcceptAny) };
    let c = roundtrip(&srv, config(Security::Explicit(opts))).await;
    assert!(c.is_secure() && !c.is_data_protected());
    assert!(srv.received().iter().any(|l| l == "PROT C"));
    assert!(srv.data_channels().iter().all(|(enc, _)| !enc));
}

#[tokio::test]
async fn pinned_fingerprint_accepts_only_the_right_certificate() {
    let srv = MockServer::start(MockConfig { tls: MockTls::Explicit, ..MockConfig::default() }).await;
    let good = srv.fingerprint.clone().unwrap().to_lowercase().replace(':', "");
    roundtrip(&srv, config(Security::Explicit(tls(TlsVerify::Fingerprint(good))))).await;

    let wrong = "00".repeat(32);
    let err = FtpClient::connect("127.0.0.1", srv.addr.port(), config(Security::Explicit(tls(TlsVerify::Fingerprint(wrong)))))
        .await
        .err()
        .expect("the wrong certificate must be refused");
    match err {
        FtpError::Certificate { fingerprint, .. } => assert_eq!(Some(fingerprint), srv.fingerprint),
        other => panic!("erreur inattendue : {other}"),
    }
}

#[tokio::test]
async fn system_verification_rejects_self_signed_and_reports_fingerprint() {
    let srv = MockServer::start(MockConfig { tls: MockTls::Implicit, ..MockConfig::default() }).await;
    let err = FtpClient::connect("127.0.0.1", srv.addr.port(), config(Security::Implicit(tls(TlsVerify::System))))
        .await
        .err()
        .expect("a self-signed certificate must not pass the system verification");
    match err {
        FtpError::Certificate { fingerprint, reason } => {
            assert_eq!(Some(fingerprint), srv.fingerprint);
            assert!(!reason.is_empty());
        }
        other => panic!("erreur inattendue : {other}"),
    }
}

#[tokio::test]
async fn auth_tls_refused_by_plain_server() {
    let srv = MockServer::start(MockConfig::default()).await;
    let err = FtpClient::connect("127.0.0.1", srv.addr.port(), config(Security::Explicit(tls(TlsVerify::AcceptAny))))
        .await
        .err()
        .expect("AUTH TLS must fail");
    assert_eq!(err.reply_code(), Some(502));
    let cmds = srv.received();
    assert_eq!(cmds, ["AUTH TLS", "AUTH SSL"]);
}

#[tokio::test]
async fn active_mode_plain_and_tls() {
    let srv = MockServer::start(MockConfig::default()).await;
    roundtrip(&srv, FtpConfig { active_mode: true, ..FtpConfig::default() }).await;
    assert!(srv.received().iter().any(|l| l.starts_with("PORT 127,0,0,1,")));
    assert!(!srv.received().iter().any(|l| l == "EPSV" || l == "PASV"));
    assert_eq!(srv.stats.lock().unwrap().active_connections, 3);

    let srv = MockServer::start(MockConfig { tls: MockTls::Explicit, require_prot_p: true, ..MockConfig::default() }).await;
    let cfg = FtpConfig { active_mode: true, ..config(Security::Explicit(tls(TlsVerify::AcceptAny))) };
    roundtrip(&srv, cfg).await;
    assert!(srv.data_channels().iter().all(|(enc, _)| *enc));
}

#[tokio::test]
async fn active_mode_port_range_and_external_ip() {
    let srv = MockServer::start(MockConfig::default()).await;
    let cfg = FtpConfig { active_mode: true, active_port_range: Some((50_100, 50_200)), ..FtpConfig::default() };
    roundtrip(&srv, cfg).await;
    let port_cmd = srv.received().into_iter().find(|l| l.starts_with("PORT ")).unwrap();
    let n: Vec<u16> = port_cmd[5..].split(',').map(|s| s.parse().unwrap()).collect();
    assert!((50_100..=50_200).contains(&(n[4] * 256 + n[5])), "{port_cmd}");

    // The announced external IP is sent as is (deliberately unreachable here: only the
    // text of the command is checked).
    let mut c = FtpClient::connect(
        "127.0.0.1",
        srv.addr.port(),
        FtpConfig {
            active_mode: true,
            active_external_ip: Some("203.0.113.9".parse().unwrap()),
            connect_timeout: std::time::Duration::from_millis(300),
            ..FtpConfig::default()
        },
    )
    .await
    .unwrap();
    c.login("test", "secret").await.unwrap();
    let _ = c.list(None).await;
    assert!(srv.received().iter().any(|l| l.starts_with("PORT 203,0,113,9,")));
}

#[tokio::test]
async fn listings_and_transfers_are_protected_separately() {
    let srv = MockServer::start(MockConfig { tls: MockTls::Explicit, ..MockConfig::default() }).await;
    let opts = TlsOptions { verify: TlsVerify::AcceptAny, protect_data: true, protect_listings: false, ..TlsOptions::default() };
    roundtrip(&srv, config(Security::Explicit(opts))).await;
    // Upload, listing, download.
    let channels: Vec<bool> = srv.data_channels().into_iter().map(|(encrypted, _)| encrypted).collect();
    assert_eq!(channels, [true, false, true]);
    let cmds = srv.received();
    let list = cmds.iter().position(|l| l.starts_with("MLSD") || l.starts_with("LIST")).unwrap();
    assert_eq!(cmds[..list].iter().rev().find(|l| l.starts_with("PROT")).map(String::as_str), Some("PROT C"), "{cmds:?}");
}
