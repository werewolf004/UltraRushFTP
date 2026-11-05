// Test harness (mock servers, fixtures): a failed unwrap/expect is a test failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;

use std::net::{IpAddr, SocketAddr};

use common::proxy::MockProxy;
use common::{MockConfig, MockServer, MockTls};
use urftp_proto::ftp::{FtpClient, FtpConfig, FtpError, Security, TlsOptions, TlsVerify};
use urftp_proto::ident::{IdentConfig, IdentServer};
use urftp_proto::proxy::{ProxyConfig, ProxyError, ProxyKind};

fn proxy_cfg(p: &MockProxy, kind: ProxyKind) -> ProxyConfig {
    // Le serveur de test est en 127.0.0.1 : il faut désactiver le contournement des adresses locales.
    ProxyConfig { bypass_local: false, ..ProxyConfig::new(kind, "127.0.0.1", p.addr.port()) }
}

async fn roundtrip(host: &str, srv: &MockServer, cfg: FtpConfig) -> FtpClient {
    let mut c = FtpClient::connect(host, srv.addr.port(), cfg).await.unwrap();
    c.login("test", "secret").await.unwrap();
    let payload: Vec<u8> = (0..150_000u32).map(|i| (i % 233) as u8).collect();
    c.stor("/p.bin", 0, &mut payload.as_slice(), |_| {}).await.unwrap();
    assert_eq!(c.list(None).await.unwrap().len(), 1);
    let mut back = Vec::new();
    c.retr("/p.bin", 0, &mut back, |_| {}).await.unwrap();
    assert_eq!(back, payload);
    c
}

/// Contrôle + 3 canaux de données passent tous par le proxy, vers le bon hôte.
async fn check_through_proxy(kind: ProxyKind, host: &str, auth: Option<(&str, &str)>) {
    let srv = MockServer::start(MockConfig::default()).await;
    let px = MockProxy::start(kind, auth).await;
    let mut pc = proxy_cfg(&px, kind);
    if let Some((u, p)) = auth {
        pc = pc.with_auth(u, p);
    }
    let c = roundtrip(host, &srv, FtpConfig { proxy: Some(pc), ..FtpConfig::default() }).await;
    assert!(c.uses_proxy());
    let targets = px.targets();
    assert_eq!(targets.len(), 4, "{kind:?} : {targets:?}");
    assert_eq!(targets[0], format!("{host}:{}", srv.addr.port()));
    assert!(targets.iter().all(|t| t.starts_with(host)), "{targets:?}");
}

#[tokio::test]
async fn socks4() {
    check_through_proxy(ProxyKind::Socks4, "127.0.0.1", None).await;
}

#[tokio::test]
async fn socks4a_resolves_names_on_the_proxy() {
    check_through_proxy(ProxyKind::Socks4a, "localhost", None).await;
}

#[tokio::test]
async fn socks5_with_ip_name_and_auth() {
    check_through_proxy(ProxyKind::Socks5, "127.0.0.1", None).await;
    check_through_proxy(ProxyKind::Socks5, "localhost", None).await;
    check_through_proxy(ProxyKind::Socks5, "127.0.0.1", Some(("alice", "s3cr3t"))).await;
}

#[tokio::test]
async fn http_tunnel_with_and_without_auth() {
    check_through_proxy(ProxyKind::HttpTunnel, "127.0.0.1", None).await;
    check_through_proxy(ProxyKind::HttpTunnel, "localhost", Some(("bob", "p@ss:word"))).await;
}

#[tokio::test]
async fn wrong_proxy_credentials() {
    let srv = MockServer::start(MockConfig::default()).await;
    for kind in [ProxyKind::Socks5, ProxyKind::HttpTunnel] {
        let px = MockProxy::start(kind, Some(("alice", "bon"))).await;
        let cfg = FtpConfig { proxy: Some(proxy_cfg(&px, kind).with_auth("alice", "faux")), ..FtpConfig::default() };
        let err = FtpClient::connect("127.0.0.1", srv.addr.port(), cfg).await.err().unwrap();
        assert!(matches!(err, FtpError::Proxy(ProxyError::AuthFailed)), "{kind:?} : {err}");
    }
}

#[tokio::test]
async fn proxy_reports_unreachable_target() {
    let px = MockProxy::start(ProxyKind::Socks5, None).await;
    // Port fermé : on réserve un port puis on le libère.
    let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let cfg = FtpConfig { proxy: Some(proxy_cfg(&px, ProxyKind::Socks5)), ..FtpConfig::default() };
    let err = FtpClient::connect("127.0.0.1", closed, cfg).await.err().unwrap();
    assert!(matches!(err, FtpError::Proxy(ProxyError::Refused { .. })), "{err}");
}

#[tokio::test]
async fn ftps_through_socks5_with_session_reuse() {
    let srv = MockServer::start(MockConfig {
        tls: MockTls::Explicit,
        require_session_reuse: true,
        require_prot_p: true,
        ..MockConfig::default()
    })
    .await;
    let px = MockProxy::start(ProxyKind::Socks5, None).await;
    let cfg = FtpConfig {
        proxy: Some(proxy_cfg(&px, ProxyKind::Socks5)),
        security: Security::Explicit(TlsOptions { verify: TlsVerify::AcceptAny, ..TlsOptions::default() }),
        ..FtpConfig::default()
    };
    let c = roundtrip("127.0.0.1", &srv, cfg).await;
    assert!(c.is_secure() && c.is_data_protected());
    assert_eq!(px.targets().len(), 4);
    assert!(srv.data_channels().iter().all(|&(enc, resumed)| enc && resumed));
}

#[tokio::test]
async fn exceptions_and_local_bypass_skip_the_proxy() {
    let srv = MockServer::start(MockConfig::default()).await;
    let px = MockProxy::start(ProxyKind::Socks5, None).await;

    let mut pc = proxy_cfg(&px, ProxyKind::Socks5);
    pc.exceptions = vec!["127.0.0.*".into()];
    let c = roundtrip("127.0.0.1", &srv, FtpConfig { proxy: Some(pc), ..FtpConfig::default() }).await;
    assert!(!c.uses_proxy());

    // Contournement des adresses locales (réglage par défaut).
    let pc = ProxyConfig::new(ProxyKind::Socks5, "127.0.0.1", px.addr.port());
    roundtrip("127.0.0.1", &srv, FtpConfig { proxy: Some(pc), ..FtpConfig::default() }).await;
    assert!(px.targets().is_empty());
}

#[tokio::test]
async fn data_channels_can_bypass_the_proxy() {
    let srv = MockServer::start(MockConfig::default()).await;
    let px = MockProxy::start(ProxyKind::Socks5, None).await;
    let pc = ProxyConfig { apply_to_data: false, ..proxy_cfg(&px, ProxyKind::Socks5) };
    roundtrip("127.0.0.1", &srv, FtpConfig { proxy: Some(pc), ..FtpConfig::default() }).await;
    assert_eq!(px.targets().len(), 1, "seul le canal de contrôle passe par le proxy");
}

#[tokio::test]
async fn active_mode_switches_to_passive_through_proxy() {
    let srv = MockServer::start(MockConfig::default()).await;
    let px = MockProxy::start(ProxyKind::Socks5, None).await;
    let cfg = FtpConfig { active_mode: true, proxy: Some(proxy_cfg(&px, ProxyKind::Socks5)), ..FtpConfig::default() };
    roundtrip("127.0.0.1", &srv, cfg).await;
    let cmds = srv.received();
    assert!(!cmds.iter().any(|l| l.starts_with("PORT") || l.starts_with("EPRT")));
    assert!(cmds.iter().any(|l| l == "EPSV"));
}

#[tokio::test]
async fn ident_answers_site_name_then_default() {
    let ident = IdentServer::start(IdentConfig { bind: SocketAddr::from(([127, 0, 0, 1], 0)), ..IdentConfig::default() }).await.unwrap();
    let srv = MockServer::start(MockConfig { ident_port: Some(ident.local_addr().port()), ..MockConfig::default() }).await;
    let localhost: IpAddr = [127, 0, 0, 1].into();

    let reg = ident.register(localhost, srv.addr.port(), "SITEUSER");
    let mut c = FtpClient::connect("127.0.0.1", srv.addr.port(), FtpConfig::default()).await.unwrap();
    c.login("test", "secret").await.unwrap();
    drop(reg);
    let mut c2 = FtpClient::connect("127.0.0.1", srv.addr.port(), FtpConfig::default()).await.unwrap();
    c2.login("test", "secret").await.unwrap();

    let answers = srv.stats.lock().unwrap().ident_answers.clone();
    assert_eq!(answers.len(), 2);
    assert!(answers[0].ends_with(": USERID : UNIX : SITEUSER"), "{answers:?}");
    assert!(answers[1].ends_with(": USERID : UNIX : URUSHFTP"), "{answers:?}");
    let first_port: u16 = answers[0].split(',').next().unwrap().trim().parse().unwrap();
    assert_ne!(first_port, srv.addr.port(), "le premier port est celui du client FTP");

    // Serveur Ident arrêté (« seulement pendant la connexion ») : la connexion FTP marche quand même.
    drop(ident);
    let mut c3 = FtpClient::connect("127.0.0.1", srv.addr.port(), FtpConfig::default()).await.unwrap();
    c3.login("test", "secret").await.unwrap();
    assert!(srv.stats.lock().unwrap().ident_answers[2].starts_with("échec"));
}
