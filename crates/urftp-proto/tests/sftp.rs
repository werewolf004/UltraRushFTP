mod common;

use common::proxy::MockProxy;
use common::sftp::{MockSftp, SftpMockConfig};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey};
use urftp_proto::ftp::EntryKind;
use urftp_proto::proxy::{ProxyConfig, ProxyKind};
use urftp_proto::sftp::{HostKeyPolicy, SftpAuth, SftpClient, SftpConfig, SftpError};

fn trusting(srv: &MockSftp) -> SftpConfig {
    SftpConfig { host_key: HostKeyPolicy::Known(vec![srv.fingerprint.clone()]), ..SftpConfig::default() }
}

async fn connect(srv: &MockSftp, auth: &SftpAuth, cfg: SftpConfig) -> Result<SftpClient, SftpError> {
    SftpClient::connect("127.0.0.1", srv.addr.port(), "test", auth, cfg, None).await
}

fn password() -> SftpAuth {
    SftpAuth::Password("secret".into())
}

#[tokio::test]
async fn password_session_full_cycle() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    let mut c = connect(&srv, &password(), trusting(&srv)).await.unwrap();
    assert_eq!(c.current_dir(), "/home/test");
    assert_eq!(c.host_key_fingerprint(), srv.fingerprint);

    c.mkdir("upload").await.unwrap();
    let data: Vec<u8> = (0..400_000u32).map(|i| (i % 239) as u8).collect();
    let mut last = 0;
    let n = c.stor("upload/big.bin", 0, &mut data.as_slice(), |p| last = p).await.unwrap();
    assert_eq!((n, last), (data.len() as u64, data.len() as u64));

    let list = c.list(Some("upload")).await.unwrap();
    assert_eq!(list.len(), 1);
    let e = &list[0];
    assert_eq!((e.name.as_str(), e.kind, e.size), ("big.bin", EntryKind::File, data.len() as u64));
    assert_eq!(e.permissions.as_deref(), Some("-rw-r--r--"));
    assert_eq!(e.modified.unwrap().year, Some(2024));
    let root = c.list(None).await.unwrap();
    assert!(root.iter().any(|e| e.name == "upload" && e.is_dir()));
    assert!(!root.iter().any(|e| e.name == "." || e.name == ".."));

    let mut back = Vec::new();
    c.retr("/home/test/upload/big.bin", 0, &mut back, |_| {}).await.unwrap();
    assert_eq!(back, data);

    c.cwd("upload").await.unwrap();
    assert_eq!(c.current_dir(), "/home/test/upload");
    c.rename("big.bin", "renamed.bin").await.unwrap();
    assert_eq!(c.size("renamed.bin").await.unwrap(), data.len() as u64);
    c.delete("renamed.bin").await.unwrap();
    c.cwd("..").await.unwrap();
    c.rmdir("upload").await.unwrap();
    assert!(srv.fs.lock().unwrap().files.is_empty());
    assert_eq!(srv.auth_log.lock().unwrap().as_slice(), ["password"]);
    c.quit().await.unwrap();
}

#[tokio::test]
async fn resume_upload_and_download() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    let mut c = connect(&srv, &password(), trusting(&srv)).await.unwrap();
    let full: Vec<u8> = (0..120_000u32).map(|i| (i % 211) as u8).collect();
    c.stor("f.bin", 0, &mut &full[..50_000], |_| {}).await.unwrap();
    let have = c.size("f.bin").await.unwrap();
    c.stor("f.bin", have, &mut &full[have as usize..], |_| {}).await.unwrap();
    assert_eq!(srv.fs.lock().unwrap().files["/home/test/f.bin"], full);

    let mut tail = Vec::new();
    let n = c.retr("f.bin", 100_000, &mut tail, |_| {}).await.unwrap();
    assert_eq!((n, tail.as_slice()), (20_000, &full[100_000..]));
}

#[tokio::test]
async fn unknown_host_key_is_reported_with_fingerprint() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    let err = connect(&srv, &password(), SftpConfig::default()).await.err().expect("clé inconnue refusée");
    match err {
        SftpError::HostKey { fingerprint, .. } => {
            assert_eq!(fingerprint, srv.fingerprint);
            assert!(fingerprint.starts_with("SHA256:"));
        }
        other => panic!("erreur inattendue : {other}"),
    }
    let wrong = SftpConfig {
        host_key: HostKeyPolicy::Known(vec!["SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into()]),
        ..SftpConfig::default()
    };
    assert!(matches!(connect(&srv, &password(), wrong).await, Err(SftpError::HostKey { .. })));
    let any = SftpConfig { host_key: HostKeyPolicy::AcceptAny, ..SftpConfig::default() };
    connect(&srv, &password(), any).await.unwrap();
}

#[tokio::test]
async fn wrong_password_is_refused() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    let err = connect(&srv, &SftpAuth::Password("faux".into()), trusting(&srv)).await.err().unwrap();
    assert!(matches!(err, SftpError::AuthFailed), "{err}");
}

#[tokio::test]
async fn public_key_auth_from_file_and_encrypted_text() {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let srv = MockSftp::start(SftpMockConfig { authorized_key: Some(key.public_key().clone()), ..SftpMockConfig::default() }).await;

    // Clé OpenSSH en clair dans un fichier.
    let dir = std::env::temp_dir().join(format!("urftp-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("id_ed25519");
    std::fs::write(&file, key.to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
    connect(&srv, &SftpAuth::KeyFile { path: file, passphrase: None }, trusting(&srv)).await.unwrap();
    std::fs::remove_dir_all(&dir).unwrap();

    // Clé chiffrée par phrase de passe, fournie en texte.
    let encrypted = key.encrypt(&mut rand::rng(), "phrase secrète").unwrap();
    let text = encrypted.to_openssh(LineEnding::LF).unwrap().to_string();
    let auth = SftpAuth::KeyText { text: text.clone(), passphrase: Some("phrase secrète".into()) };
    connect(&srv, &auth, trusting(&srv)).await.unwrap();
    assert_eq!(srv.auth_log.lock().unwrap().as_slice(), ["publickey", "publickey"]);

    // Mauvaise phrase de passe : erreur de clé, pas de connexion.
    let bad = SftpAuth::KeyText { text, passphrase: Some("faux".into()) };
    assert!(matches!(connect(&srv, &bad, trusting(&srv)).await, Err(SftpError::Key(_))));

    // Clé non autorisée sur le serveur.
    let other = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let auth = SftpAuth::KeyText { text: other.to_openssh(LineEnding::LF).unwrap().to_string(), passphrase: None };
    assert!(matches!(connect(&srv, &auth, trusting(&srv)).await, Err(SftpError::AuthFailed)));
}

#[tokio::test]
async fn errors_carry_sftp_status_codes() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    let mut c = connect(&srv, &password(), trusting(&srv)).await.unwrap();
    let err = c.retr("absent.bin", 0, &mut Vec::new(), |_| {}).await.unwrap_err();
    assert_eq!(err.status_code(), Some(2), "{err}");
    assert!(c.cwd("/nulle/part").await.is_err());
    // La session reste utilisable.
    c.list(None).await.unwrap();
}

#[tokio::test]
async fn long_remote_paths() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    let deep: String = (0..15).map(|i| format!("/dossier_sftp_{i:02}_{}", "q".repeat(40))).collect();
    assert!(deep.len() > 800);
    srv.fs.lock().unwrap().mkdir_p(&deep);
    let mut c = connect(&srv, &password(), trusting(&srv)).await.unwrap();
    let file = format!("{deep}/release.nfo");
    c.stor(&file, 0, &mut &b"profond"[..], |_| {}).await.unwrap();
    c.cwd(&deep).await.unwrap();
    assert_eq!(c.list(None).await.unwrap()[0].name, "release.nfo");
    let mut back = Vec::new();
    c.retr("release.nfo", 0, &mut back, |_| {}).await.unwrap();
    assert_eq!(back, b"profond");
}

#[tokio::test]
async fn sftp_through_socks5_and_http_proxies() {
    let srv = MockSftp::start(SftpMockConfig::default()).await;
    for kind in [ProxyKind::Socks5, ProxyKind::HttpTunnel, ProxyKind::Socks4] {
        let px = MockProxy::start(kind, None).await;
        let proxy = ProxyConfig { bypass_local: false, ..ProxyConfig::new(kind, "127.0.0.1", px.addr.port()) };
        let cfg = SftpConfig { proxy: Some(proxy), ..trusting(&srv) };
        let mut c = connect(&srv, &password(), cfg).await.unwrap();
        c.stor("p.txt", 0, &mut &b"via proxy"[..], |_| {}).await.unwrap();
        assert_eq!(px.targets(), [format!("127.0.0.1:{}", srv.addr.port())], "{kind:?}");
    }
}
