//! Serveur FTP/FTPS minimal en mémoire pour les tests d'intégration (localhost uniquement).
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
type BoxStream = Box<dyn Stream>;
type Conn = BufReader<BoxStream>;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MockTls {
    /// Pas de TLS : `AUTH TLS` est refusé.
    #[default]
    None,
    /// `AUTH TLS` accepté.
    Explicit,
    /// TLS dès la connexion.
    Implicit,
}

#[derive(Clone)]
pub struct MockConfig {
    pub user: String,
    pub pass: String,
    /// Refuse (550) tout argument plus long que cette limite, comme certains serveurs réels.
    pub max_arg_len: Option<usize>,
    pub epsv: bool,
    pub mlsd: bool,
    pub tls: MockTls,
    /// Comme vsftpd `require_ssl_reuse=YES` : refuse un canal de données TLS sans reprise de session.
    pub require_session_reuse: bool,
    /// Refuse les transferts tant que `PROT P` n'a pas été envoyé.
    pub require_prot_p: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            user: "test".into(),
            pass: "secret".into(),
            max_arg_len: None,
            epsv: true,
            mlsd: true,
            tls: MockTls::None,
            require_session_reuse: false,
            require_prot_p: false,
        }
    }
}

pub struct Fs {
    pub dirs: BTreeSet<String>,
    pub files: BTreeMap<String, Vec<u8>>,
}

impl Fs {
    fn new() -> Self {
        Self { dirs: BTreeSet::from(["/".to_string()]), files: BTreeMap::new() }
    }

    pub fn mkdir_p(&mut self, path: &str) {
        let mut cur = String::new();
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            cur.push('/');
            cur.push_str(seg);
            self.dirs.insert(cur.clone());
        }
    }

    fn children(&self, dir: &str) -> (Vec<String>, Vec<(String, usize)>) {
        let dirs = self.dirs.iter().filter(|d| *d != "/" && parent(d) == dir).map(|d| name(d).to_string()).collect();
        let files =
            self.files.iter().filter(|(f, _)| parent(f) == dir).map(|(f, c)| (name(f).to_string(), c.len())).collect();
        (dirs, files)
    }
}

fn parent(p: &str) -> &str {
    match p.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &p[..i],
    }
}

fn name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn resolve(cwd: &str, arg: &str) -> String {
    let full = if arg.starts_with('/') { arg.to_string() } else { format!("{cwd}/{arg}") };
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

/// Statistiques observées par le serveur de test.
#[derive(Default)]
pub struct Stats {
    /// Toutes les commandes reçues, toutes sessions confondues.
    pub commands: Vec<String>,
    /// Canaux de données ouverts : (chiffré, session TLS reprise).
    pub data_channels: Vec<(bool, bool)>,
    /// Connexions de données en mode actif.
    pub active_connections: usize,
}

pub struct MockServer {
    pub addr: SocketAddr,
    pub fs: Arc<Mutex<Fs>>,
    pub stats: Arc<Mutex<Stats>>,
    /// Empreinte SHA-256 du certificat (si TLS).
    pub fingerprint: Option<String>,
}

struct Shared {
    cfg: MockConfig,
    fs: Arc<Mutex<Fs>>,
    stats: Arc<Mutex<Stats>>,
    tls: Option<TlsAcceptor>,
}

fn make_tls() -> (TlsAcceptor, String) {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string(), "127.0.0.1".to_string()]).unwrap();
    let cert: CertificateDer<'static> = ck.cert.der().clone();
    let fp = urftp_proto::ftp::fingerprint_sha256(&cert);
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der()));
    let config = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .unwrap();
    (TlsAcceptor::from(Arc::new(config)), fp)
}

impl MockServer {
    pub async fn start(cfg: MockConfig) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let fs = Arc::new(Mutex::new(Fs::new()));
        let stats = Arc::new(Mutex::new(Stats::default()));
        let (tls, fingerprint) = match cfg.tls {
            MockTls::None => (None, None),
            _ => {
                let (a, fp) = make_tls();
                (Some(a), Some(fp))
            }
        };
        let shared = Arc::new(Shared { cfg, fs: fs.clone(), stats: stats.clone(), tls });
        tokio::spawn(async move {
            while let Ok((s, _)) = listener.accept().await {
                let shared = shared.clone();
                tokio::spawn(async move {
                    let _ = session(s, shared).await;
                });
            }
        });
        Self { addr, fs, stats, fingerprint }
    }

    pub fn received(&self) -> Vec<String> {
        self.stats.lock().unwrap().commands.clone()
    }

    pub fn data_channels(&self) -> Vec<(bool, bool)> {
        self.stats.lock().unwrap().data_channels.clone()
    }
}

enum Pending {
    Pasv(TcpListener),
    Port(SocketAddr),
}

struct State {
    cwd: String,
    user_ok: bool,
    logged: bool,
    rest: u64,
    pending: Option<Pending>,
    rnfr: Option<String>,
    prot_p: bool,
}

async fn reply(c: &mut Conn, s: &str) -> std::io::Result<()> {
    let w = c.get_mut();
    w.write_all(s.as_bytes()).await?;
    w.write_all(b"\r\n").await?;
    w.flush().await
}

/// Ouvre le canal de données (passif ou actif) et négocie TLS si `PROT P`.
async fn open_data(st: &mut State, sh: &Shared) -> Result<BoxStream, &'static str> {
    let tcp = match st.pending.take().ok_or("425 Use PASV or PORT first")? {
        Pending::Pasv(l) => l.accept().await.map_err(|_| "425 Accept failed")?.0,
        Pending::Port(addr) => {
            sh.stats.lock().unwrap().active_connections += 1;
            TcpStream::connect(addr).await.map_err(|_| "425 Cannot connect to client")?
        }
    };
    if !st.prot_p {
        sh.stats.lock().unwrap().data_channels.push((false, false));
        return Ok(Box::new(tcp));
    }
    let acceptor = sh.tls.clone().ok_or("534 TLS not available")?;
    let tls = acceptor.accept(tcp).await.map_err(|_| "522 TLS handshake failed")?;
    let resumed = tls.get_ref().1.handshake_kind() == Some(rustls::HandshakeKind::Resumed);
    sh.stats.lock().unwrap().data_channels.push((true, resumed));
    if sh.cfg.require_session_reuse && !resumed {
        return Err("522 SSL connection failed: session reuse required");
    }
    Ok(Box::new(tls))
}

fn parse_port(arg: &str) -> Option<SocketAddr> {
    let n: Vec<u16> = arg.split(',').map(|s| s.trim().parse().ok()).collect::<Option<_>>()?;
    if n.len() != 6 {
        return None;
    }
    format!("{}.{}.{}.{}:{}", n[0], n[1], n[2], n[3], n[4] * 256 + n[5]).parse().ok()
}

fn parse_eprt(arg: &str) -> Option<SocketAddr> {
    let d = arg.chars().next()?;
    let p: Vec<&str> = arg.split(d).collect();
    let ip: std::net::IpAddr = p.get(2)?.parse().ok()?;
    Some(SocketAddr::new(ip, p.get(3)?.parse().ok()?))
}

async fn session(tcp: TcpStream, sh: Arc<Shared>) -> std::io::Result<()> {
    let stream: BoxStream = if sh.cfg.tls == MockTls::Implicit {
        Box::new(sh.tls.clone().unwrap().accept(tcp).await?)
    } else {
        Box::new(tcp)
    };
    let mut c: Conn = BufReader::new(stream);
    reply(&mut c, "220 Serveur de test UltraRushFTP").await?;
    let mut st = State {
        cwd: "/".into(),
        user_ok: false,
        logged: false,
        rest: 0,
        pending: None,
        rnfr: None,
        prot_p: false,
    };
    let fs = sh.fs.clone();

    loop {
        let mut line = String::new();
        if c.read_line(&mut line).await? == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']).to_string();
        sh.stats.lock().unwrap().commands.push(line.clone());
        let (verb, arg) = match line.split_once(' ') {
            Some((v, a)) => (v.to_ascii_uppercase(), a.to_string()),
            None => (line.to_ascii_uppercase(), String::new()),
        };
        if !st.logged && !matches!(verb.as_str(), "USER" | "PASS" | "QUIT" | "FEAT" | "AUTH") {
            reply(&mut c, "530 Please login").await?;
            continue;
        }
        if sh.cfg.max_arg_len.is_some_and(|m| arg.len() > m) {
            reply(&mut c, "550 Path too long").await?;
            continue;
        }
        let is_transfer = matches!(verb.as_str(), "LIST" | "MLSD" | "RETR" | "STOR" | "APPE");
        if is_transfer && sh.cfg.require_prot_p && !st.prot_p {
            reply(&mut c, "521 Data connections must be encrypted").await?;
            continue;
        }
        let target = resolve(&st.cwd, &arg);
        match verb.as_str() {
            "AUTH" => {
                let upper = arg.to_ascii_uppercase();
                if sh.cfg.tls == MockTls::Explicit && (upper == "TLS" || upper == "SSL") {
                    reply(&mut c, "234 AUTH TLS successful").await?;
                    let plain = c.into_inner();
                    let tls = sh.tls.clone().unwrap().accept(plain).await?;
                    c = BufReader::new(Box::new(tls));
                } else {
                    reply(&mut c, "502 AUTH not supported").await?;
                }
            }
            "PBSZ" => reply(&mut c, "200 PBSZ=0").await?,
            "PROT" => match arg.to_ascii_uppercase().as_str() {
                "P" if sh.tls.is_some() => {
                    st.prot_p = true;
                    reply(&mut c, "200 Protection set to Private").await?;
                }
                "C" => {
                    st.prot_p = false;
                    reply(&mut c, "200 Protection set to Clear").await?;
                }
                _ => reply(&mut c, "536 Protection level not supported").await?,
            },
            "USER" => {
                st.user_ok = arg == sh.cfg.user;
                reply(&mut c, "331 Password required").await?;
            }
            "PASS" => {
                if st.user_ok && arg == sh.cfg.pass {
                    st.logged = true;
                    reply(&mut c, "230 Logged in").await?;
                } else {
                    reply(&mut c, "530 Login incorrect").await?;
                }
            }
            "FEAT" => {
                let mut s = String::from("211-Features:\r\n SIZE\r\n REST STREAM\r\n UTF8\r\n");
                if sh.cfg.epsv {
                    s.push_str(" EPSV\r\n");
                }
                if sh.cfg.mlsd {
                    s.push_str(" MLST type*;size*;modify*;\r\n");
                }
                if sh.tls.is_some() {
                    s.push_str(" AUTH TLS\r\n PBSZ\r\n PROT\r\n");
                }
                s.push_str("211 End");
                reply(&mut c, &s).await?;
            }
            "OPTS" | "NOOP" | "TYPE" | "PRET" => reply(&mut c, "200 OK").await?,
            "SYST" => reply(&mut c, "215 UNIX Type: L8").await?,
            "PWD" => reply(&mut c, &format!("257 \"{}\" is current directory", st.cwd)).await?,
            "CWD" | "CDUP" => {
                let t = if verb == "CDUP" { resolve(&st.cwd, "..") } else { target };
                if fs.lock().unwrap().dirs.contains(&t) {
                    st.cwd = t;
                    reply(&mut c, "250 OK").await?;
                } else {
                    reply(&mut c, "550 No such directory").await?;
                }
            }
            "EPSV" if sh.cfg.epsv => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let port = l.local_addr()?.port();
                st.pending = Some(Pending::Pasv(l));
                reply(&mut c, &format!("229 Entering Extended Passive Mode (|||{port}|)")).await?;
            }
            "PASV" => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let port = l.local_addr()?.port();
                st.pending = Some(Pending::Pasv(l));
                reply(&mut c, &format!("227 Entering Passive Mode (127,0,0,1,{},{})", port >> 8, port & 0xff)).await?;
            }
            "PORT" | "EPRT" => {
                let addr = if verb == "PORT" { parse_port(&arg) } else { parse_eprt(&arg) };
                match addr {
                    Some(a) => {
                        st.pending = Some(Pending::Port(a));
                        reply(&mut c, &format!("200 {verb} command successful")).await?;
                    }
                    None => reply(&mut c, "501 Bad address").await?,
                }
            }
            "LIST" | "MLSD" => {
                let dir = if arg.is_empty() || arg.starts_with('-') { st.cwd.clone() } else { target };
                let (dirs, files) = fs.lock().unwrap().children(&dir);
                let mut out = String::new();
                for d in dirs {
                    if verb == "MLSD" {
                        out += &format!("type=dir;modify=20240101120000; {d}\r\n");
                    } else {
                        out += &format!("drwxr-xr-x 1 owner group 0 Jan 01 12:00 {d}\r\n");
                    }
                }
                for (f, n) in files {
                    if verb == "MLSD" {
                        out += &format!("type=file;size={n};modify=20240101120000; {f}\r\n");
                    } else {
                        out += &format!("-rw-r--r-- 1 owner group {n} Jan 01 12:00 {f}\r\n");
                    }
                }
                reply(&mut c, "150 Opening data connection").await?;
                match open_data(&mut st, &sh).await {
                    Ok(mut d) => {
                        d.write_all(out.as_bytes()).await?;
                        d.shutdown().await?;
                        reply(&mut c, "226 Transfer complete").await?;
                    }
                    Err(e) => reply(&mut c, e).await?,
                }
            }
            "REST" => {
                st.rest = arg.parse().unwrap_or(0);
                reply(&mut c, "350 Restarting").await?;
            }
            "RETR" => {
                let content = fs.lock().unwrap().files.get(&target).cloned();
                let rest = std::mem::take(&mut st.rest) as usize;
                let Some(content) = content else {
                    reply(&mut c, "550 No such file").await?;
                    continue;
                };
                reply(&mut c, "150 Opening data connection").await?;
                match open_data(&mut st, &sh).await {
                    Ok(mut d) => {
                        d.write_all(&content[rest.min(content.len())..]).await?;
                        d.shutdown().await?;
                        reply(&mut c, "226 Transfer complete").await?;
                    }
                    Err(e) => reply(&mut c, e).await?,
                }
            }
            "STOR" | "APPE" => {
                if !fs.lock().unwrap().dirs.contains(parent(&target)) {
                    reply(&mut c, "550 No such directory").await?;
                    continue;
                }
                let rest = std::mem::take(&mut st.rest) as usize;
                reply(&mut c, "150 Opening data connection").await?;
                let mut d = match open_data(&mut st, &sh).await {
                    Ok(d) => d,
                    Err(e) => {
                        reply(&mut c, e).await?;
                        continue;
                    }
                };
                let mut buf = Vec::new();
                match d.read_to_end(&mut buf).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
                    Err(e) => return Err(e),
                }
                {
                    let mut g = fs.lock().unwrap();
                    let file = g.files.entry(target).or_default();
                    if verb != "APPE" {
                        file.truncate(rest);
                    }
                    file.extend_from_slice(&buf);
                }
                reply(&mut c, "226 Transfer complete").await?;
            }
            "SIZE" => {
                let n = fs.lock().unwrap().files.get(&target).map(Vec::len);
                match n {
                    Some(n) => reply(&mut c, &format!("213 {n}")).await?,
                    None => reply(&mut c, "550 No such file").await?,
                }
            }
            "MDTM" => {
                let exists = fs.lock().unwrap().files.contains_key(&target);
                reply(&mut c, if exists { "213 20240102030405" } else { "550 No such file" }).await?;
            }
            "MKD" => {
                let ok = {
                    let mut g = fs.lock().unwrap();
                    let ok = g.dirs.contains(parent(&target)) && !g.dirs.contains(&target);
                    if ok {
                        g.dirs.insert(target.clone());
                    }
                    ok
                };
                if ok {
                    reply(&mut c, &format!("257 \"{target}\" created")).await?;
                } else {
                    reply(&mut c, "550 Cannot create").await?;
                }
            }
            "RMD" => {
                let ok = {
                    let mut g = fs.lock().unwrap();
                    let (d, f) = g.children(&target);
                    let ok = target != "/" && g.dirs.contains(&target) && d.is_empty() && f.is_empty();
                    if ok {
                        g.dirs.remove(&target);
                    }
                    ok
                };
                reply(&mut c, if ok { "250 Removed" } else { "550 Cannot remove" }).await?;
            }
            "DELE" => {
                let ok = fs.lock().unwrap().files.remove(&target).is_some();
                reply(&mut c, if ok { "250 Deleted" } else { "550 No such file" }).await?;
            }
            "RNFR" => {
                let exists = fs.lock().unwrap().files.contains_key(&target);
                if exists {
                    st.rnfr = Some(target);
                    reply(&mut c, "350 Ready for RNTO").await?;
                } else {
                    reply(&mut c, "550 No such file").await?;
                }
            }
            "RNTO" => {
                let moved = st.rnfr.take().and_then(|from| {
                    let mut g = fs.lock().unwrap();
                    let content = g.files.remove(&from)?;
                    g.files.insert(target, content);
                    Some(())
                });
                reply(&mut c, if moved.is_some() { "250 Renamed" } else { "503 Bad sequence" }).await?;
            }
            "QUIT" => {
                reply(&mut c, "221 Bye").await?;
                break;
            }
            _ => reply(&mut c, "502 Not implemented").await?,
        }
    }
    Ok(())
}
