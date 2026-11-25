//! In-memory SSH/SFTP test server (localhost only).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::keys::{Algorithm, HashAlg, PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Msg, Session, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, SftpHandle, Status, StatusCode, Version};
use tokio::net::TcpListener;

#[derive(Default)]
pub struct SftpFs {
    pub dirs: BTreeSet<String>,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl SftpFs {
    pub fn mkdir_p(&mut self, path: &str) {
        let mut cur = String::new();
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            cur.push('/');
            cur.push_str(seg);
            self.dirs.insert(cur.clone());
        }
    }
}

pub struct MockSftp {
    pub addr: SocketAddr,
    pub fs: Arc<Mutex<SftpFs>>,
    /// Fingerprint of the host key (`SHA256:…`).
    pub fingerprint: String,
    /// Accepted authentication methods ("password", "publickey").
    pub auth_log: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone)]
pub struct SftpMockConfig {
    pub user: String,
    pub pass: String,
    /// Public key authorised for key authentication.
    pub authorized_key: Option<PublicKey>,
}

impl Default for SftpMockConfig {
    fn default() -> Self {
        Self { user: "test".into(), pass: "secret".into(), authorized_key: None }
    }
}

impl MockSftp {
    pub async fn start(cfg: SftpMockConfig) -> Self {
        let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let fingerprint = host_key.public_key().fingerprint(HashAlg::Sha256).to_string();
        let config = Arc::new(russh::server::Config {
            keys: vec![host_key],
            auth_rejection_time: Duration::from_millis(10),
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let fs = Arc::new(Mutex::new(SftpFs::default()));
        fs.lock().unwrap().dirs.insert("/".into());
        fs.lock().unwrap().mkdir_p("/home/test");
        let auth_log = Arc::new(Mutex::new(Vec::new()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (fs2, log2) = (fs.clone(), auth_log.clone());
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let handler = SshHandler { cfg: cfg.clone(), fs: fs2.clone(), auth_log: log2.clone(), channels: HashMap::new() };
                let config = config.clone();
                tokio::spawn(async move {
                    if let Ok(running) = russh::server::run_stream(config, tcp, handler).await {
                        let _ = running.await;
                    }
                });
            }
        });
        Self { addr, fs, fingerprint, auth_log }
    }
}

struct SshHandler {
    cfg: SftpMockConfig,
    fs: Arc<Mutex<SftpFs>>,
    auth_log: Arc<Mutex<Vec<String>>>,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl russh::server::Handler for SshHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == self.cfg.user && password == self.cfg.pass {
            self.auth_log.lock().unwrap().push("password".into());
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(&mut self, user: &str, key: VerifiedPublicKey<'_>) -> Result<Auth, Self::Error> {
        let ok = user == self.cfg.user && self.cfg.authorized_key.as_ref().is_some_and(|k| k.key_data() == key.public_key.key_data());
        if ok {
            self.auth_log.lock().unwrap().push("publickey".into());
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn channel_eof(&mut self, channel: ChannelId, session: &mut Session) -> Result<(), Self::Error> {
        session.close(channel)?;
        Ok(())
    }

    async fn subsystem_request(&mut self, id: ChannelId, name: &str, session: &mut Session) -> Result<(), Self::Error> {
        match (name, self.channels.remove(&id)) {
            ("sftp", Some(channel)) => {
                session.channel_success(id)?;
                let fs = self.fs.clone();
                tokio::spawn(russh_sftp::server::run(channel.into_stream(), SftpHandler::new(fs)));
            }
            _ => session.channel_failure(id)?,
        }
        Ok(())
    }
}

enum OpenItem {
    Dir { path: String, done: bool },
    File { path: String },
}

struct SftpHandler {
    fs: Arc<Mutex<SftpFs>>,
    handles: HashMap<String, OpenItem>,
    next: u64,
}

impl SftpHandler {
    fn new(fs: Arc<Mutex<SftpFs>>) -> Self {
        Self { fs, handles: HashMap::new(), next: 0 }
    }

    fn handle(&mut self, item: OpenItem) -> String {
        self.next += 1;
        let h = format!("h{}", self.next);
        self.handles.insert(h.clone(), item);
        h
    }
}

fn ok(id: u32) -> Status {
    Status { id, status_code: StatusCode::Ok, error_message: "Ok".into(), language_tag: "en-US".into() }
}

fn norm(p: &str) -> String {
    let base = if p.starts_with('/') { String::new() } else { "/home/test/".to_string() };
    let full = format!("{base}{p}");
    let mut parts: Vec<&str> = Vec::new();
    for seg in full.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    format!("/{}", parts.join("/"))
}

fn parent(p: &str) -> &str {
    match p.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &p[..i],
    }
}

fn dir_attrs() -> FileAttributes {
    let mut a = FileAttributes::empty();
    a.permissions = Some(0o040755);
    a.size = Some(0);
    a.mtime = Some(1_704_164_645);
    a
}

fn file_attrs(len: usize) -> FileAttributes {
    let mut a = FileAttributes::empty();
    a.permissions = Some(0o100644);
    a.size = Some(len as u64);
    a.mtime = Some(1_704_164_645);
    a.user = Some("test".into());
    a
}

impl russh_sftp::server::Handler for SftpHandler {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(&mut self, _v: u32, _ext: HashMap<String, String>) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        Ok(Name { id, files: vec![File::dummy(norm(&path))] })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let p = norm(&path);
        let fs = self.fs.lock().unwrap();
        if fs.dirs.contains(&p) {
            Ok(Attrs { id, attrs: dir_attrs() })
        } else if let Some(f) = fs.files.get(&p) {
            Ok(Attrs { id, attrs: file_attrs(f.len()) })
        } else {
            Err(StatusCode::NoSuchFile)
        }
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        self.stat(id, path).await
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        let p = norm(&path);
        if !self.fs.lock().unwrap().dirs.contains(&p) {
            return Err(StatusCode::NoSuchFile);
        }
        Ok(Handle { id, handle: self.handle(OpenItem::Dir { path: p, done: false }).into() })
    }

    async fn readdir(&mut self, id: u32, handle: SftpHandle) -> Result<Name, Self::Error> {
        let handle = key(&handle);
        let Some(OpenItem::Dir { path, done }) = self.handles.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        if *done {
            return Err(StatusCode::Eof);
        }
        *done = true;
        let fs = self.fs.lock().unwrap();
        let mut files = vec![File::new(".", dir_attrs()), File::new("..", dir_attrs())];
        for d in fs.dirs.iter().filter(|d| *d != "/" && parent(d) == path.as_str()) {
            files.push(File::new(d.rsplit('/').next().unwrap(), dir_attrs()));
        }
        for (f, c) in fs.files.iter().filter(|(f, _)| parent(f) == path.as_str()) {
            files.push(File::new(f.rsplit('/').next().unwrap(), file_attrs(c.len())));
        }
        Ok(Name { id, files })
    }

    async fn open(&mut self, id: u32, filename: String, flags: OpenFlags, _a: FileAttributes) -> Result<Handle, Self::Error> {
        let p = norm(&filename);
        {
            let mut fs = self.fs.lock().unwrap();
            if !fs.dirs.contains(parent(&p)) {
                return Err(StatusCode::NoSuchFile);
            }
            let exists = fs.files.contains_key(&p);
            if !exists && !flags.contains(OpenFlags::CREATE) {
                return Err(StatusCode::NoSuchFile);
            }
            let f = fs.files.entry(p.clone()).or_default();
            if flags.contains(OpenFlags::TRUNCATE) {
                f.clear();
            }
        }
        Ok(Handle { id, handle: self.handle(OpenItem::File { path: p }).into() })
    }

    async fn read(&mut self, id: u32, handle: SftpHandle, offset: u64, len: u32) -> Result<Data, Self::Error> {
        let handle = key(&handle);
        let Some(OpenItem::File { path }) = self.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        let fs = self.fs.lock().unwrap();
        let content = fs.files.get(path).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        if start >= content.len() {
            return Err(StatusCode::Eof);
        }
        let end = (start + len as usize).min(content.len());
        Ok(Data { id, data: content[start..end].to_vec() })
    }

    async fn write(&mut self, id: u32, handle: SftpHandle, offset: u64, data: Vec<u8>) -> Result<Status, Self::Error> {
        let handle = key(&handle);
        let Some(OpenItem::File { path }) = self.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        let mut fs = self.fs.lock().unwrap();
        let f = fs.files.get_mut(path).ok_or(StatusCode::NoSuchFile)?;
        let start = offset as usize;
        if f.len() < start + data.len() {
            f.resize(start + data.len(), 0);
        }
        f[start..start + data.len()].copy_from_slice(&data);
        Ok(ok(id))
    }

    async fn fstat(&mut self, id: u32, handle: SftpHandle) -> Result<Attrs, Self::Error> {
        let handle = key(&handle);
        match self.handles.get(&handle) {
            Some(OpenItem::File { path }) => {
                let n = self.fs.lock().unwrap().files.get(path).map_or(0, Vec::len);
                Ok(Attrs { id, attrs: file_attrs(n) })
            }
            Some(OpenItem::Dir { .. }) => Ok(Attrs { id, attrs: dir_attrs() }),
            None => Err(StatusCode::Failure),
        }
    }

    async fn close(&mut self, id: u32, handle: SftpHandle) -> Result<Status, Self::Error> {
        let handle = key(&handle);
        self.handles.remove(&handle);
        Ok(ok(id))
    }

    async fn mkdir(&mut self, id: u32, path: String, _a: FileAttributes) -> Result<Status, Self::Error> {
        let p = norm(&path);
        let mut fs = self.fs.lock().unwrap();
        if !fs.dirs.contains(parent(&p)) || fs.dirs.contains(&p) {
            return Err(StatusCode::Failure);
        }
        fs.dirs.insert(p);
        Ok(ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        let p = norm(&path);
        let mut fs = self.fs.lock().unwrap();
        let busy = fs.dirs.iter().any(|d| parent(d) == p && d != "/") || fs.files.keys().any(|f| parent(f) == p);
        if !fs.dirs.contains(&p) || busy {
            return Err(StatusCode::Failure);
        }
        fs.dirs.remove(&p);
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let p = norm(&filename);
        match self.fs.lock().unwrap().files.remove(&p) {
            Some(_) => Ok(ok(id)),
            None => Err(StatusCode::NoSuchFile),
        }
    }

    async fn rename(&mut self, id: u32, oldpath: String, newpath: String) -> Result<Status, Self::Error> {
        let (from, to) = (norm(&oldpath), norm(&newpath));
        let mut fs = self.fs.lock().unwrap();
        let content = fs.files.remove(&from).ok_or(StatusCode::NoSuchFile)?;
        fs.files.insert(to, content);
        Ok(ok(id))
    }
}

/// Key of an open handle in the table (the handles made by `handle` are text).
fn key(handle: &SftpHandle) -> String {
    String::from_utf8_lossy(handle.as_bytes()).into_owned()
}
