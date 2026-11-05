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

    // Envoi interrompu à 40 000 octets, puis reprise.
    c.stor("/f.bin", 0, &mut &full[..40_000], |_| {}).await.unwrap();
    let remote = c.size("/f.bin").await.unwrap();
    assert_eq!(remote, 40_000);
    c.stor("/f.bin", remote, &mut &full[40_000..], |_| {}).await.unwrap();
    assert_eq!(srv.fs.lock().unwrap().files["/f.bin"], full);

    // Téléchargement repris à 70 000 octets.
    let mut tail = Vec::new();
    let n = c.retr("/f.bin", 70_000, &mut tail, |_| {}).await.unwrap();
    assert_eq!(n, 30_000);
    assert_eq!(tail, &full[70_000..]);
    assert!(srv.received().iter().any(|l| l == "REST 70000"));
}

#[tokio::test]
async fn long_remote_paths_fall_back_to_step_by_step_cwd() {
    // Le serveur refuse tout argument de plus de 100 caractères.
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
    // Aucune commande envoyée n'a dépassé la limite, hormis le premier CWD tenté en entier.
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
    assert_eq!(cmds.iter().filter(|l| *l == "EPSV").count(), 1, "EPSV ne doit être tenté qu'une fois");
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
    // La connexion reste utilisable après une erreur.
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
