//! Minimal in-memory FTP/FTPS server for the integration tests (localhost only).
#![allow(dead_code)]

pub mod proxy;
pub mod sftp;

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
type BoxStream = Box<dyn Stream>;
type Conn = BufReader<BoxStream>;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum MockTls {
    /// No TLS: `AUTH TLS` is refused.
    #[default]
    None,
    /// `AUTH TLS` accepted.
    Explicit,
    /// TLS from the start of the connection.
    Implicit,
}

#[derive(Clone)]
pub struct MockConfig {
    pub user: String,
    pub pass: String,
    /// Refuses (550) any argument longer than this limit, like some real servers.
    pub max_arg_len: Option<usize>,
    pub epsv: bool,
    pub mlsd: bool,
    pub tls: MockTls,
    /// Like vsftpd `require_ssl_reuse=YES`: refuses a TLS data channel without session reuse.
    pub require_session_reuse: bool,
    /// Refuses transfers until `PROT P` has been sent.
    pub require_prot_p: bool,
    /// Queries the client's Ident server on this port at every connection (like glftpd).
    pub ident_port: Option<u16>,
    /// Accepts `SSCN ON/OFF` (and announces it in `FEAT`).
    pub sscn: bool,
    /// Accepts `CPSV`.
    pub cpsv: bool,
    /// Announces `PRET` in `FEAT`.
    pub pret: bool,
    /// Refuses every `PORT`/`EPRT` command (FXP forbidden in active mode).
    pub refuse_port: bool,
    /// Delay before every data transfer (to observe parallelism).
    pub transfer_delay: Option<std::time::Duration>,
    /// The first N `RETR` (all sessions together) fail with 451.
    pub fail_first_retr: usize,
    /// Closes the control connection with `421` after this time without a command
    /// (idle kick, like most servers).
    pub idle_kick: Option<std::time::Duration>,
    /// The first N listings (all sessions together) are refused with this reply.
    pub fail_first_list: Option<(usize, &'static str)>,
    /// Logins accepted in all (later ones get `530`).
    pub max_logins: Option<usize>,
    /// Listens on `[::1]` instead of `127.0.0.1` (`PASV` is then refused, like real
    /// servers reached over IPv6).
    pub ipv6: bool,
    /// Refuses `PASV` and `EPSV` (passive mode broken by a firewall).
    pub refuse_pasv: bool,
    /// Names in ISO-8859-1 instead of UTF-8 (commands and listings).
    pub latin1: bool,
    /// Answers `STAT -l` with the listing, one line per entry, each starting with a space.
    pub stat_list: bool,
    /// `LIST` lines start with two spaces (badly formatted listings).
    pub padded_list: bool,
    /// Announces and handles `MODE Z` (zlib data channel).
    pub mode_z: bool,
    /// Announces and handles `XCRC`.
    pub xcrc: bool,
    /// `SITE XDUPE` accepted; then an upload over an existing file is refused with the
    /// dupes of its folder (`553- X-DUPE:` lines).
    pub xdupe: bool,
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
            ident_port: None,
            sscn: false,
            cpsv: false,
            pret: false,
            refuse_port: false,
            transfer_delay: None,
            fail_first_retr: 0,
            idle_kick: None,
            fail_first_list: None,
            max_logins: None,
            ipv6: false,
            refuse_pasv: false,
            latin1: false,
            stat_list: false,
            padded_list: false,
            mode_z: false,
            xcrc: false,
            xdupe: false,
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
        let files = self.files.iter().filter(|(f, _)| parent(f) == dir).map(|(f, c)| (name(f).to_string(), c.len())).collect();
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

/// Statistics observed by the test server.
#[derive(Default)]
pub struct Stats {
    /// Every command received, all sessions together.
    pub commands: Vec<String>,
    /// Data channels opened: (encrypted, TLS session reused).
    pub data_channels: Vec<(bool, bool)>,
    /// Data connections in active mode.
    pub active_connections: usize,
    /// Ident answers obtained when connecting.
    pub ident_answers: Vec<String>,
    /// Data channels where this server acted as the TLS client (SSCN / CPSV).
    pub tls_client_channels: usize,
    /// Authenticated sessions: current, simultaneous maximum, total.
    pub current_sessions: usize,
    pub max_sessions: usize,
    pub logins: usize,
    /// Sessions closed by the idle kick.
    pub kicks: usize,
}

/// Counts an authenticated session when it ends, whatever the exit path.
struct LoginGuard {
    stats: Arc<Mutex<Stats>>,
    active: bool,
}

impl LoginGuard {
    fn login(&mut self) {
        if !self.active {
            self.active = true;
            let mut s = self.stats.lock().unwrap();
            s.current_sessions += 1;
            s.logins += 1;
            s.max_sessions = s.max_sessions.max(s.current_sessions);
        }
    }
}

impl Drop for LoginGuard {
    fn drop(&mut self) {
        if self.active {
            self.stats.lock().unwrap().current_sessions -= 1;
        }
    }
}

pub struct MockServer {
    pub addr: SocketAddr,
    pub fs: Arc<Mutex<Fs>>,
    pub stats: Arc<Mutex<Stats>>,
    /// SHA-256 fingerprint of the certificate (with TLS).
    pub fingerprint: Option<String>,
}

struct Shared {
    cfg: MockConfig,
    fs: Arc<Mutex<Fs>>,
    stats: Arc<Mutex<Stats>>,
    tls: Option<TlsAcceptor>,
    /// TLS client role (encrypted FXP): accepts the certificate of the other test server.
    tls_client: TlsConnector,
    fail_counter: AtomicUsize,
    list_fail_counter: AtomicUsize,
}

#[derive(Debug)]
struct AcceptAll(Arc<rustls::crypto::CryptoProvider>);

impl rustls::client::danger::ServerCertVerifier for AcceptAll {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        m: &[u8],
        c: &CertificateDer<'_>,
        d: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

fn make_tls_client() -> TlsConnector {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAll(provider)))
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
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
        let listener = TcpListener::bind(if cfg.ipv6 { "[::1]:0" } else { "127.0.0.1:0" }).await.unwrap();
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
        let shared = Arc::new(Shared {
            cfg,
            fs: fs.clone(),
            stats: stats.clone(),
            tls,
            tls_client: make_tls_client(),
            fail_counter: AtomicUsize::new(0),
            list_fail_counter: AtomicUsize::new(0),
        });
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
    /// `SSCN ON`: TLS client role on the data channels.
    sscn: bool,
    /// `CPSV`: TLS client role for the next channel only.
    cpsv_once: bool,
    /// `MODE Z` on.
    mode_z: bool,
    /// `SITE XDUPE` asked.
    xdupe: bool,
}

/// `LIST` lines of a folder (the names starting with `.` only when `all`).
fn unix_listing(fs: &Fs, dir: &str, all: bool) -> Vec<String> {
    let (dirs, files) = fs.children(dir);
    let shown = |n: &str| all || !n.starts_with('.');
    let mut out: Vec<String> =
        dirs.into_iter().filter(|d| shown(d)).map(|d| format!("drwxr-xr-x 1 owner group 0 Jan 01 12:00 {d}")).collect();
    out.extend(files.into_iter().filter(|(f, _)| shown(f)).map(|(f, n)| format!("-rw-r--r-- 1 owner group {n} Jan 01 12:00 {f}")));
    out
}

async fn reply(c: &mut Conn, s: &str) -> std::io::Result<()> {
    let w = c.get_mut();
    w.write_all(s.as_bytes()).await?;
    w.write_all(b"\r\n").await?;
    w.flush().await
}

/// Opens the data channel (passive or active) and negotiates TLS when `PROT P`.
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
    if st.sscn || std::mem::take(&mut st.cpsv_once) {
        let name = rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap();
        let tls = sh.tls_client.connect(name, tcp).await.map_err(|_| "522 TLS handshake (client) failed")?;
        let mut g = sh.stats.lock().unwrap();
        g.data_channels.push((true, false));
        g.tls_client_channels += 1;
        return Ok(Box::new(tls));
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

/// End of upload: close_notify / FIN, then wait for the peer to close so as not to cause
/// an RST (TLS 1.3 tickets arriving on a closed socket).
async fn finish_send(d: &mut BoxStream) -> std::io::Result<()> {
    d.shutdown().await?;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut sink = [0u8; 4096];
        while let Ok(n) = d.read(&mut sink).await {
            if n == 0 {
                break;
            }
        }
    })
    .await;
    Ok(())
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

/// Ident request to the client: `<client port> , <server port>`.
async fn ident_lookup(tcp: &TcpStream, ident_port: u16) -> std::io::Result<String> {
    let (client, server) = (tcp.peer_addr()?, tcp.local_addr()?);
    let mut q = TcpStream::connect((client.ip(), ident_port)).await?;
    q.write_all(format!("{} , {}\r\n", client.port(), server.port()).as_bytes()).await?;
    let mut lines = BufReader::new(q).lines();
    Ok(lines.next_line().await?.unwrap_or_default())
}

async fn session(tcp: TcpStream, sh: Arc<Shared>) -> std::io::Result<()> {
    let mut guard = LoginGuard { stats: sh.stats.clone(), active: false };
    if let Some(port) = sh.cfg.ident_port {
        let answer = ident_lookup(&tcp, port).await.unwrap_or_else(|e| format!("failed: {e}"));
        sh.stats.lock().unwrap().ident_answers.push(answer);
    }
    let stream: BoxStream =
        if sh.cfg.tls == MockTls::Implicit { Box::new(sh.tls.clone().unwrap().accept(tcp).await?) } else { Box::new(tcp) };
    let mut c: Conn = BufReader::new(stream);
    reply(&mut c, "220 UltraRushFTP test server").await?;
    let mut st = State {
        cwd: "/".into(),
        user_ok: false,
        logged: false,
        rest: 0,
        pending: None,
        rnfr: None,
        prot_p: false,
        sscn: false,
        cpsv_once: false,
        mode_z: false,
        xdupe: false,
    };
    let fs = sh.fs.clone();

    loop {
        let mut raw = Vec::new();
        let read = match sh.cfg.idle_kick {
            Some(limit) => match tokio::time::timeout(limit, c.read_until(b'\n', &mut raw)).await {
                Ok(r) => r?,
                Err(_) => {
                    sh.stats.lock().unwrap().kicks += 1;
                    reply(&mut c, "421 Idle timeout, closing control connection").await?;
                    break;
                }
            },
            None => c.read_until(b'\n', &mut raw).await?,
        };
        if read == 0 {
            break;
        }
        let line = if sh.cfg.latin1 { raw.iter().map(|&b| char::from(b)).collect() } else { String::from_utf8_lossy(&raw).into_owned() };
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
        if matches!(verb.as_str(), "LIST" | "MLSD")
            && let Some((n, text)) = sh.cfg.fail_first_list
            && sh.list_fail_counter.fetch_add(1, Ordering::SeqCst) < n
        {
            reply(&mut c, text).await?;
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
                let full = sh.cfg.max_logins.is_some_and(|m| sh.stats.lock().unwrap().logins >= m);
                if full {
                    reply(&mut c, "530 No more logins accepted").await?;
                } else if st.user_ok && arg == sh.cfg.pass {
                    st.logged = true;
                    guard.login();
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
                if sh.cfg.sscn {
                    s.push_str(" SSCN\r\n");
                }
                if sh.cfg.pret {
                    s.push_str(" PRET\r\n");
                }
                if sh.cfg.mode_z {
                    s.push_str(" MODE Z\r\n");
                }
                if sh.cfg.xcrc {
                    s.push_str(" XCRC\r\n");
                }
                s.push_str("211 End");
                reply(&mut c, &s).await?;
            }
            "OPTS" | "NOOP" | "TYPE" | "PRET" | "CLNT" => reply(&mut c, "200 OK").await?,
            "MODE" if sh.cfg.mode_z && arg.eq_ignore_ascii_case("Z") => {
                st.mode_z = true;
                reply(&mut c, "200 MODE Z ok").await?;
            }
            "MODE" if arg.eq_ignore_ascii_case("S") => {
                st.mode_z = false;
                reply(&mut c, "200 MODE S ok").await?;
            }
            "SITE" if sh.cfg.xdupe && arg.eq_ignore_ascii_case("XDUPE 3") => {
                st.xdupe = true;
                reply(&mut c, "200 Activated extended dupe mode 3").await?;
            }
            "XCRC" if sh.cfg.xcrc => {
                let crc = fs.lock().unwrap().files.get(&target).map(|b| crc32fast::hash(b));
                match crc {
                    Some(crc) => reply(&mut c, &format!("250 {crc:08X}")).await?,
                    None => reply(&mut c, "550 No such file").await?,
                }
            }
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
            "PASV" | "EPSV" if sh.cfg.refuse_pasv => reply(&mut c, "500 Passive mode disabled").await?,
            "STAT" if sh.cfg.stat_list && arg.starts_with('-') => {
                let mut out = String::from("213-Status of .:\r\n");
                for line in unix_listing(&fs.lock().unwrap(), &st.cwd, true) {
                    out += &format!(" {line}\r\n");
                }
                out += "213 End of status";
                reply(&mut c, &out).await?;
            }
            "EPSV" if sh.cfg.epsv => {
                let l = TcpListener::bind(if sh.cfg.ipv6 { "[::1]:0" } else { "127.0.0.1:0" }).await?;
                let port = l.local_addr()?.port();
                st.pending = Some(Pending::Pasv(l));
                reply(&mut c, &format!("229 Entering Extended Passive Mode (|||{port}|)")).await?;
            }
            "PASV" if sh.cfg.ipv6 => reply(&mut c, "500 PASV is IPv4 only, use EPSV").await?,
            "PASV" => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let port = l.local_addr()?.port();
                st.pending = Some(Pending::Pasv(l));
                reply(&mut c, &format!("227 Entering Passive Mode (127,0,0,1,{},{})", port >> 8, port & 0xff)).await?;
            }
            "SSCN" if sh.cfg.sscn => match arg.to_ascii_uppercase().as_str() {
                "ON" => {
                    st.sscn = true;
                    reply(&mut c, "200 SSCN:CLIENT METHOD").await?;
                }
                "OFF" => {
                    st.sscn = false;
                    reply(&mut c, "200 SSCN:SERVER METHOD").await?;
                }
                _ => reply(&mut c, &format!("200 SSCN:{} METHOD", if st.sscn { "CLIENT" } else { "SERVER" })).await?,
            },
            "CPSV" if sh.cfg.cpsv => {
                let l = TcpListener::bind("127.0.0.1:0").await?;
                let port = l.local_addr()?.port();
                st.pending = Some(Pending::Pasv(l));
                st.cpsv_once = true;
                reply(&mut c, &format!("227 Entering Passive Mode (127,0,0,1,{},{})", port >> 8, port & 0xff)).await?;
            }
            "ABOR" => {
                st.pending = None;
                reply(&mut c, "226 ABOR command successful").await?;
            }
            "PORT" | "EPRT" if sh.cfg.refuse_port => reply(&mut c, "500 Illegal PORT command").await?,
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
                let mut out = String::new();
                if verb == "MLSD" {
                    let (dirs, files) = fs.lock().unwrap().children(&dir);
                    for d in dirs {
                        out += &format!("type=dir;modify=20240101120000; {d}\r\n");
                    }
                    for (f, n) in files {
                        out += &format!("type=file;size={n};modify=20240101120000; {f}\r\n");
                    }
                } else {
                    // Like most Unix servers: the hidden files only with `-a`.
                    let all = arg.starts_with('-') && arg.contains('a');
                    for line in unix_listing(&fs.lock().unwrap(), &dir, all) {
                        if sh.cfg.padded_list {
                            out += "  ";
                        }
                        out += &line;
                        out += "\r\n";
                    }
                }
                reply(&mut c, "150 Opening data connection").await?;
                match open_data(&mut st, &sh).await {
                    Ok(mut d) => {
                        let bytes: Vec<u8> = if sh.cfg.latin1 {
                            out.chars().map(|ch| u8::try_from(u32::from(ch)).unwrap_or(b'?')).collect()
                        } else {
                            out.into_bytes()
                        };
                        d.write_all(&bytes).await?;
                        finish_send(&mut d).await?;
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
                    // Closes the passive listener: an FXP peer already connected sees the connection drop.
                    st.pending = None;
                    reply(&mut c, "550 No such file").await?;
                    continue;
                };
                if sh.fail_counter.fetch_add(1, Ordering::SeqCst) < sh.cfg.fail_first_retr {
                    st.pending = None;
                    reply(&mut c, "451 Temporary failure, try again").await?;
                    continue;
                }
                reply(&mut c, "150 Opening data connection").await?;
                match open_data(&mut st, &sh).await {
                    Ok(mut d) => {
                        if let Some(delay) = sh.cfg.transfer_delay {
                            tokio::time::sleep(delay).await;
                        }
                        let part = &content[rest.min(content.len())..];
                        if st.mode_z {
                            use std::io::Write;
                            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                            z.write_all(part)?;
                            d.write_all(&z.finish()?).await?;
                        } else {
                            d.write_all(part).await?;
                        }
                        finish_send(&mut d).await?;
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
                if st.xdupe && fs.lock().unwrap().files.contains_key(&target) {
                    st.pending = None;
                    let dir = parent(&target).to_string();
                    let dupes: Vec<String> = {
                        let g = fs.lock().unwrap();
                        g.files.keys().filter(|f| parent(f) == dir).map(|f| f.rsplit('/').next().unwrap_or_default().to_string()).collect()
                    };
                    let mut text: String = dupes.iter().map(|n| format!("553- X-DUPE: {n}\r\n")).collect();
                    text.push_str("553 File exists");
                    reply(&mut c, &text).await?;
                    continue;
                }
                reply(&mut c, "150 Opening data connection").await?;
                // Like real servers, the file exists from the start of the upload.
                fs.lock().unwrap().files.entry(target.clone()).or_default();
                let mut d = match open_data(&mut st, &sh).await {
                    Ok(d) => d,
                    Err(e) => {
                        reply(&mut c, e).await?;
                        continue;
                    }
                };
                if let Some(delay) = sh.cfg.transfer_delay {
                    tokio::time::sleep(delay).await;
                }
                let mut buf = Vec::new();
                match d.read_to_end(&mut buf).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {}
                    Err(_) => {
                        reply(&mut c, "426 Connection closed; transfer aborted").await?;
                        continue;
                    }
                }
                if st.mode_z {
                    use std::io::Read;
                    let mut plain = Vec::new();
                    if flate2::read::ZlibDecoder::new(&buf[..]).read_to_end(&mut plain).is_err() {
                        reply(&mut c, "451 Invalid compressed data").await?;
                        continue;
                    }
                    buf = plain;
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
