use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufStream};
use tokio::net::{TcpListener, TcpStream, lookup_host};

use crate::proxy::{self, ProxyConfig};

use super::listing::{self, RemoteEntry};
use super::reply::read_reply;
use super::tls::{Security, TlsContext};
use super::{BoxStream, Features, FtpError, Reply, Result, path};

/// Size of the copy blocks of the transfers.
const CHUNK: usize = 64 * 1024;

/// Settings of an FTP connection.
#[derive(Debug, Clone)]
pub struct FtpConfig {
    /// Timeout for establishing control and data connections.
    pub connect_timeout: Duration,
    /// Maximum wait for a reply or a data block.
    pub io_timeout: Duration,
    /// Try `EPSV` before `PASV` (automatic fallback when refused).
    pub prefer_epsv: bool,
    /// Use `MLSD` when the server announces `MLST`.
    pub use_mlsd: bool,
    /// Listing command when `MLSD` is not used (e.g. `LIST`, `LIST -a`, `STAT -l`...).
    pub list_command: String,
    /// Replace the private address returned by `PASV` with the server's (badly configured NAT).
    pub pasv_fix_private_ip: bool,
    /// Maximum length of a reply line.
    pub max_line_len: usize,
    /// Plain FTP, explicit or implicit FTPS.
    pub security: Security,
    /// Active mode (`PORT`/`EPRT`) instead of passive mode.
    pub active_mode: bool,
    /// Address announced in active mode (public IP behind NAT); the local IP by default.
    pub active_external_ip: Option<IpAddr>,
    /// Range of local ports for active mode (router port forwarding).
    pub active_port_range: Option<(u16, u16)>,
    /// Proxy (SOCKS4/4A/5, HTTP tunnel). Ignored when its bypass rules apply.
    pub proxy: Option<ProxyConfig>,
    /// Ask the server its capabilities (`FEAT`) after the login.
    pub send_feat: bool,
    /// Client name announced with `CLNT` after the login (glftpd, DrFTPD...).
    pub client_name: Option<String>,
    /// Removes the spaces at the start of the `LIST` lines (always done for `STAT`).
    pub trim_list_spaces: bool,
    /// Switches to active mode for the rest of the connection when the passive mode fails.
    pub port_fallback: bool,
    /// Minutes added to the dates of the listings (server in another time zone).
    pub time_offset_mins: i32,
    /// Character set of the commands, replies and listings (`None`: UTF-8).
    pub charset: Option<&'static encoding_rs::Encoding>,
    /// Local address the control and data connections leave from.
    pub bind_ip: Option<IpAddr>,
}

impl Default for FtpConfig {
    fn default() -> Self {
        Self {
            proxy: None,
            security: Security::None,
            active_mode: false,
            active_external_ip: None,
            active_port_range: None,
            connect_timeout: Duration::from_secs(30),
            io_timeout: Duration::from_secs(120),
            prefer_epsv: true,
            use_mlsd: true,
            list_command: "LIST".into(),
            pasv_fix_private_ip: true,
            max_line_len: 64 * 1024,
            send_feat: true,
            client_name: None,
            trim_list_spaces: false,
            port_fallback: false,
            time_offset_mins: 0,
            charset: None,
            bind_ip: None,
        }
    }
}

/// Log line emitted by the client (shown in the FTP log of the tab).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    /// Command sent (password masked).
    Command(String),
    /// Reply received.
    Reply(Reply),
    /// Information from the client itself.
    Status(String),
}

/// Receiver of the log lines.
pub type LogSink = Arc<dyn Fn(LogLine) + Send + Sync>;

/// Representation type (`TYPE A` / `TYPE I`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Ascii,
    Binary,
}

/// Open FTP connection.
///
/// Operations on a path first change to its directory, then act on the name only: a
/// server refusing long absolute paths stays usable, because [`FtpClient::cwd`] falls back
/// to segment-by-segment navigation.
pub struct FtpClient {
    ctrl: BufStream<BoxStream>,
    host: String,
    /// IP of the FTP server when known (unknown when the proxy resolves the name itself).
    server_ip: Option<IpAddr>,
    local: SocketAddr,
    /// Proxy actually used by this connection.
    proxy: Option<ProxyConfig>,
    proxy_active_warned: bool,
    tls: Option<TlsContext>,
    /// Encrypted data channels (`PROT P` accepted).
    prot_p: bool,
    config: FtpConfig,
    features: Features,
    welcome: Reply,
    cwd: Option<String>,
    current_type: Option<TransferType>,
    epsv_refused: bool,
    /// The passive mode failed and the connection switched to active mode.
    passive_failed: bool,
    log: Option<LogSink>,
}

impl FtpClient {
    /// Opens the TCP connection, applies the requested security (implicit TLS or
    /// `AUTH TLS`) and reads the welcome message.
    pub async fn connect(host: &str, port: u16, config: FtpConfig) -> Result<Self> {
        Self::connect_with_log(host, port, config, None).await
    }

    /// Like [`FtpClient::connect`], with a log active from the server welcome.
    pub async fn connect_with_log(host: &str, port: u16, config: FtpConfig, log: Option<LogSink>) -> Result<Self> {
        let proxy = config.proxy.clone().filter(|p| !p.bypasses(host));
        let ct = config.connect_timeout;
        let (tcp, server_ip) = match &proxy {
            Some(p) => {
                if let Some(log) = &log {
                    log(LogLine::Status(format!("Connecting through proxy {:?} {}:{}", p.kind, p.host, p.port)));
                }
                let tcp = timeout_ftp(ct, async { Ok(proxy::connect(p, host, port).await?) }).await?;
                // Without a proxy on the data channels, the real IP of the server is needed.
                let ip = match host.parse::<IpAddr>() {
                    Ok(ip) => Some(ip),
                    Err(_) if !p.apply_to_data => lookup_host((host, port)).await.ok().and_then(|mut a| a.next()).map(|a| a.ip()),
                    Err(_) => None,
                };
                (tcp, ip)
            }
            None => {
                let tcp = timeout(ct, crate::net::connect_from(host, port, config.bind_ip)).await?;
                let ip = tcp.peer_addr()?.ip();
                (tcp, Some(ip))
            }
        };
        tcp.set_nodelay(true)?;
        let local = tcp.local_addr()?;
        Self::start(Box::new(tcp), host, server_ip, local, proxy, config, log).await
    }

    /// Starts on a stream already established by the caller (special tunnel...).
    /// `host` is used for TLS verification and data connections, `server_ip` is the real
    /// IP of the server when known, `local` the address announced in active mode.
    /// The proxy of `config`, if any, is not used.
    pub async fn connect_over(
        stream: BoxStream,
        host: &str,
        server_ip: Option<IpAddr>,
        local: SocketAddr,
        config: FtpConfig,
        log: Option<LogSink>,
    ) -> Result<Self> {
        Self::start(stream, host, server_ip, local, None, config, log).await
    }

    async fn start(
        stream: BoxStream,
        host: &str,
        server_ip: Option<IpAddr>,
        local: SocketAddr,
        proxy: Option<ProxyConfig>,
        config: FtpConfig,
        log: Option<LogSink>,
    ) -> Result<Self> {
        let tls = match &config.security {
            Security::Implicit(opts) => Some(TlsContext::new(host, opts)?),
            _ => None,
        };
        let stream = match &tls {
            Some(ctx) => timeout_ftp(config.connect_timeout, ctx.wrap(stream)).await?,
            None => stream,
        };
        let mut c = Self {
            ctrl: BufStream::new(stream),
            host: host.to_string(),
            server_ip,
            local,
            proxy,
            proxy_active_warned: false,
            tls,
            prot_p: false,
            config,
            features: Features::default(),
            welcome: Reply { code: 0, lines: Vec::new() },
            cwd: None,
            current_type: None,
            epsv_refused: false,
            passive_failed: false,
            log,
        };
        let mut welcome = c.read_reply().await?;
        while welcome.code == 120 {
            welcome = c.read_reply().await?;
        }
        if !welcome.is_completion() {
            return Err(FtpError::Unexpected { command: "(connection)".into(), reply: welcome });
        }
        c.welcome = welcome;
        if matches!(c.config.security, Security::Explicit(_)) {
            c.auth_tls().await?;
        }
        Ok(c)
    }

    /// Switches the control channel to TLS (`AUTH TLS`, `AUTH SSL` fallback when allowed).
    pub async fn auth_tls(&mut self) -> Result<()> {
        if self.tls.is_some() {
            return Ok(());
        }
        let opts = self.config.security.options().cloned().unwrap_or_default();
        let r = self.raw("AUTH TLS").await?;
        if r.code != 234 {
            let retry = opts.auth_ssl_fallback && r.is_permanent_negative();
            let r2 = if retry { Some(self.raw("AUTH SSL").await?) } else { None };
            if !r2.as_ref().is_some_and(|r| r.code == 234 || r.code == 334) {
                return Err(FtpError::Unexpected { command: "AUTH TLS".into(), reply: r2.unwrap_or(r) });
            }
        }
        let ctx = TlsContext::new(&self.host, &opts)?;
        let plain = std::mem::replace(&mut self.ctrl, BufStream::new(Box::new(tokio::io::duplex(1).0)));
        let secured = timeout_ftp(self.config.connect_timeout, ctx.wrap(plain.into_inner())).await?;
        self.ctrl = BufStream::new(secured);
        self.status("Control channel encrypted (TLS)");
        self.tls = Some(ctx);
        Ok(())
    }

    /// Is the control channel encrypted?
    pub fn is_secure(&self) -> bool {
        self.tls.is_some()
    }

    /// Are the data channels encrypted (`PROT P`)?
    pub fn is_data_protected(&self) -> bool {
        self.prot_p
    }

    /// SHA-256 fingerprint of the server certificate (to display or trust it).
    pub fn peer_certificate_fingerprint(&self) -> Option<String> {
        self.tls.as_ref().and_then(TlsContext::peer_fingerprint)
    }

    pub fn set_logger(&mut self, log: Option<LogSink>) {
        self.log = log;
    }
    pub fn welcome(&self) -> &Reply {
        &self.welcome
    }
    pub fn features(&self) -> &Features {
        &self.features
    }
    /// IP of the FTP server, when known.
    pub fn server_ip(&self) -> Option<IpAddr> {
        self.server_ip
    }

    /// Does the connection go through a proxy?
    pub fn uses_proxy(&self) -> bool {
        self.proxy.is_some()
    }

    fn data_via_proxy(&self) -> Option<&ProxyConfig> {
        self.proxy.as_ref().filter(|p| p.apply_to_data)
    }
    /// Current directory known on the client side (`None` when unknown).
    pub fn current_dir(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    // ---------------------------------------------------------------- basic commands

    fn emit(&self, line: LogLine) {
        if let Some(log) = &self.log {
            log(line);
        }
    }

    pub(super) fn status(&self, msg: impl Into<String>) {
        self.emit(LogLine::Status(msg.into()));
    }

    pub(super) async fn read_reply(&mut self) -> Result<Reply> {
        self.read_reply_within(Some(self.config.io_timeout)).await
    }

    /// Reads a reply with a given timeout (`None`: no limit, for the end of an FXP).
    pub(super) async fn read_reply_within(&mut self, limit: Option<Duration>) -> Result<Reply> {
        let max = self.config.max_line_len;
        let enc = self.encoding();
        let reply = match limit {
            Some(d) => match tokio::time::timeout(d, read_reply(&mut self.ctrl, max, enc)).await {
                Ok(r) => r?,
                Err(_) => return Err(FtpError::Timeout),
            },
            None => read_reply(&mut self.ctrl, max, enc).await?,
        };
        self.emit(LogLine::Reply(reply.clone()));
        Ok(reply)
    }

    /// Aborts the running transfer (`ABOR`) and consumes the related replies (`426` of the
    /// transfer then `226`/`225` of the ABOR, depending on the server).
    pub async fn abort(&mut self) -> Result<()> {
        self.send("ABOR").await?;
        for _ in 0..3 {
            let r = self.read_reply().await?;
            if r.is_completion() {
                // A server that had just finished replies 226 (transfer) then 226 (ABOR):
                // absorb the second reply when it arrives quickly.
                let (max, enc) = (self.config.max_line_len, self.encoding());
                if let Ok(Ok(extra)) = tokio::time::timeout(Duration::from_millis(500), read_reply(&mut self.ctrl, max, enc)).await {
                    self.emit(LogLine::Reply(extra));
                }
                return Ok(());
            }
            if r.is_permanent_negative() && r.code != 426 {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Changes the protection of the data channels (`PROT P` / `PROT C`).
    pub async fn set_data_protection(&mut self, protect: bool) -> Result<()> {
        if self.prot_p == protect {
            return Ok(());
        }
        if protect && self.tls.is_none() {
            return Err(FtpError::InvalidArgument("PROT P impossible without TLS on the control channel".into()));
        }
        self.ok(if protect { "PROT P" } else { "PROT C" }).await?;
        self.prot_p = protect;
        Ok(())
    }

    pub(super) async fn send(&mut self, cmd: &str) -> Result<()> {
        if cmd.contains(['\r', '\n']) {
            return Err(FtpError::InvalidArgument("line break not allowed in an FTP command".into()));
        }
        let (bytes, _, unmappable) = self.encoding().encode(cmd);
        if unmappable {
            return Err(FtpError::InvalidArgument(format!(
                "the command contains characters the {} character set cannot write",
                self.encoding().name()
            )));
        }
        self.emit(LogLine::Command(mask(cmd)));
        let io = self.config.io_timeout;
        timeout(io, async {
            self.ctrl.write_all(&bytes).await?;
            self.ctrl.write_all(b"\r\n").await?;
            self.ctrl.flush().await
        })
        .await
    }

    /// Sends a command and returns the reply, whatever its code.
    pub async fn raw(&mut self, cmd: &str) -> Result<Reply> {
        self.send(cmd).await?;
        self.read_reply().await
    }

    /// Sends a command and requires a 2xx reply.
    pub(super) async fn ok(&mut self, cmd: &str) -> Result<Reply> {
        self.expect(cmd, |r| r.is_completion()).await
    }

    pub(super) async fn expect(&mut self, cmd: &str, accept: impl Fn(&Reply) -> bool) -> Result<Reply> {
        let reply = self.raw(cmd).await?;
        if accept(&reply) { Ok(reply) } else { Err(FtpError::Unexpected { command: mask(cmd), reply }) }
    }

    /// Character set of the control channel and of the listings.
    pub(super) fn encoding(&self) -> &'static encoding_rs::Encoding {
        self.config.charset.unwrap_or(encoding_rs::UTF_8)
    }

    fn decode(&self, bytes: &[u8]) -> String {
        self.encoding().decode_without_bom_handling(bytes).0.into_owned()
    }

    // ---------------------------------------------------------------- session

    /// `USER` / `PASS` authentication, then `FEAT`, `OPTS UTF8 ON` and `PWD`.
    pub async fn login(&mut self, user: &str, pass: &str) -> Result<()> {
        let r = self.raw(&format!("USER {user}")).await?;
        let r = match r.code {
            230 => r,
            331 => self.raw(&format!("PASS {pass}")).await?,
            _ => return Err(FtpError::Unexpected { command: format!("USER {user}"), reply: r }),
        };
        if !r.is_completion() {
            return Err(FtpError::Unexpected { command: "PASS ****".into(), reply: r });
        }
        if self.tls.is_some() {
            let protect = self.config.security.options().is_some_and(|o| o.protect_data);
            self.ok("PBSZ 0").await?;
            self.ok(if protect { "PROT P" } else { "PROT C" }).await?;
            self.prot_p = protect;
        }
        if self.config.send_feat {
            self.refresh_features().await?;
        }
        if let Some(name) = self.config.client_name.clone().filter(|n| !n.trim().is_empty()) {
            // Informative only: a refusal does not matter.
            let _ = self.raw(&format!("CLNT {}", name.trim())).await?;
        }
        // A forced character set: the server must not switch to UTF-8.
        if self.features.has("UTF8") && self.config.charset.is_none_or(|c| c == encoding_rs::UTF_8) {
            // Failure does not matter: some servers are already in UTF-8.
            let _ = self.raw("OPTS UTF8 ON").await?;
        }
        self.cwd = self.pwd().await.ok();
        Ok(())
    }

    /// Reads the server capabilities again (needed after `AUTH TLS`).
    pub async fn refresh_features(&mut self) -> Result<&Features> {
        let r = self.raw("FEAT").await?;
        self.features = if r.code == 211 { Features::parse(&r) } else { Features::default() };
        Ok(&self.features)
    }

    pub async fn noop(&mut self) -> Result<()> {
        self.ok("NOOP").await.map(drop)
    }

    /// `QUIT` then close; closing errors are ignored.
    pub async fn quit(mut self) -> Result<()> {
        let r = self.raw("QUIT").await;
        let _ = self.ctrl.shutdown().await;
        r.map(drop)
    }

    // ---------------------------------------------------------------- navigation

    pub async fn pwd(&mut self) -> Result<String> {
        let r = self.expect("PWD", |r| r.code == 257).await?;
        parse_pwd(r.text()).ok_or_else(|| FtpError::Protocol(format!("unreadable PWD reply: {r}")))
    }

    fn resolve(&self, p: &str) -> Option<String> {
        if p.starts_with('/') { Some(path::normalize(p)) } else { self.cwd.as_deref().map(|c| path::join(c, p)) }
    }

    /// Changes directory. When the server refuses the whole path (5xx), the client tries
    /// again segment by segment, which works around the length limits of servers.
    pub async fn cwd(&mut self, dir: &str) -> Result<()> {
        let target = self.resolve(dir);
        if target.is_some() && target == self.cwd {
            return Ok(());
        }
        let cmd = format!("CWD {dir}");
        let r = self.raw(&cmd).await?;
        if r.is_completion() {
            self.cwd = target;
            return Ok(());
        }
        let segments: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
        if !r.is_permanent_negative() || segments.len() < 2 {
            return Err(FtpError::Unexpected { command: cmd, reply: r });
        }
        self.status("Whole path refused: navigating folder by folder");
        self.cwd = None;
        if dir.starts_with('/') {
            self.ok("CWD /").await?;
        }
        for seg in segments {
            self.ok(&format!("CWD {seg}")).await?;
        }
        self.cwd = match target {
            Some(t) => Some(t),
            None => self.pwd().await.ok(),
        };
        Ok(())
    }

    pub async fn cdup(&mut self) -> Result<()> {
        self.ok("CDUP").await?;
        self.cwd = self.cwd.as_deref().map(|c| path::join(c, ".."));
        Ok(())
    }

    /// Changes to the parent directory of `p` and returns the name only.
    pub(super) async fn enter_parent<'p>(&mut self, p: &'p str) -> Result<&'p str> {
        let (dir, name) = path::split(p);
        if name.is_empty() {
            return Err(FtpError::InvalidArgument(format!("path without a name: {p:?}")));
        }
        if let Some(dir) = dir {
            self.cwd(dir).await?;
        }
        Ok(name)
    }

    // ---------------------------------------------------------------- files and folders

    pub async fn mkdir(&mut self, p: &str) -> Result<()> {
        let name = self.enter_parent(p).await?;
        self.expect(&format!("MKD {name}"), |r| r.code == 257 || r.is_completion()).await.map(drop)
    }

    pub async fn rmdir(&mut self, p: &str) -> Result<()> {
        let name = self.enter_parent(p).await?;
        self.ok(&format!("RMD {name}")).await.map(drop)
    }

    pub async fn delete(&mut self, p: &str) -> Result<()> {
        let name = self.enter_parent(p).await?;
        self.ok(&format!("DELE {name}")).await.map(drop)
    }

    /// Renames or moves. `to` may be a plain name (same folder) or a path.
    pub async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let name = self.enter_parent(from).await?;
        self.expect(&format!("RNFR {name}"), |r| r.code == 350).await?;
        self.ok(&format!("RNTO {to}")).await.map(drop)
    }

    /// Size of a file (`SIZE`, in binary mode).
    pub async fn size(&mut self, p: &str) -> Result<u64> {
        let name = self.enter_parent(p).await?;
        self.set_type(TransferType::Binary).await?;
        let r = self.expect(&format!("SIZE {name}"), |r| r.code == 213).await?;
        r.text().trim().parse().map_err(|_| FtpError::Protocol(format!("unreadable SIZE reply: {r}")))
    }

    /// Modification date (`MDTM`).
    pub async fn mdtm(&mut self, p: &str) -> Result<listing::FtpTime> {
        let name = self.enter_parent(p).await?;
        let r = self.expect(&format!("MDTM {name}"), |r| r.code == 213).await?;
        listing::parse_mlsd_time(r.text().trim()).ok_or_else(|| FtpError::Protocol(format!("unreadable MDTM reply: {r}")))
    }

    pub async fn set_type(&mut self, t: TransferType) -> Result<()> {
        if self.current_type == Some(t) {
            return Ok(());
        }
        let cmd = match t {
            TransferType::Ascii => "TYPE A",
            TransferType::Binary => "TYPE I",
        };
        self.ok(cmd).await?;
        self.current_type = Some(t);
        Ok(())
    }

    // ---------------------------------------------------------------- data channel

    /// Prepares the data channel before the transfer command: passive connection, or local
    /// listener announced with `PORT`/`EPRT` in active mode.
    async fn prepare_data(&mut self, transfer_cmd: &str) -> Result<PendingData> {
        let wanted = self.config.active_mode || self.passive_failed;
        let active = wanted && self.data_via_proxy().is_none();
        if wanted && !active && !self.proxy_active_warned {
            self.proxy_active_warned = true;
            self.status("Active mode impossible through the proxy: using passive mode");
        }
        if active {
            return self.open_active().await.map(PendingData::Listening);
        }
        match self.open_passive(transfer_cmd).await {
            Ok(s) => Ok(PendingData::Connected(s)),
            Err(e) if self.config.port_fallback && self.data_via_proxy().is_none() && passive_failure(&e) => {
                self.passive_failed = true;
                self.status(format!("Passive mode failed ({e}): switching to active mode (PORT)"));
                self.open_active().await.map(PendingData::Listening)
            }
            Err(e) => Err(e),
        }
    }

    /// Completes the opening after the 1xx reply: accepts the active connection (only from
    /// the server address), then negotiates TLS when `PROT P`.
    async fn complete_data(&mut self, pending: PendingData) -> Result<BoxStream> {
        let stream: BoxStream = match pending {
            PendingData::Connected(s) => s,
            PendingData::Listening(listener) => {
                let (tcp, from) = timeout(self.config.connect_timeout, listener.accept()).await?;
                if let Some(server) = self.server_ip
                    && canonical(from.ip()) != canonical(server)
                {
                    return Err(FtpError::Protocol(format!(
                        "data connection refused: it comes from {from} and not from the server {server}"
                    )));
                }
                tcp.set_nodelay(true)?;
                Box::new(tcp)
            }
        };
        match (&self.tls, self.prot_p) {
            (Some(ctx), true) => timeout_ftp(self.config.connect_timeout, ctx.wrap(stream)).await,
            _ => Ok(stream),
        }
    }

    /// Active mode: listens on the local IP of the control channel and announces the address.
    async fn open_active(&mut self) -> Result<TcpListener> {
        let ip = self.local.ip();
        let listener = match self.config.active_port_range {
            Some((first, last)) => {
                let mut found = None;
                for port in first..=last {
                    if let Ok(l) = TcpListener::bind((ip, port)).await {
                        found = Some(l);
                        break;
                    }
                }
                found.ok_or_else(|| FtpError::Protocol(format!("no free port between {first} and {last}")))?
            }
            None => TcpListener::bind((ip, 0)).await?,
        };
        let port = listener.local_addr()?.port();
        let advertised = self.config.active_external_ip.unwrap_or(ip);
        let cmd = match advertised {
            IpAddr::V4(v4) => {
                let o = v4.octets();
                format!("PORT {},{},{},{},{},{}", o[0], o[1], o[2], o[3], port >> 8, port & 0xff)
            }
            IpAddr::V6(v6) => format!("EPRT |2|{v6}|{port}|"),
        };
        self.ok(&cmd).await?;
        Ok(listener)
    }

    /// Opens a passive data connection (`PRET` when announced, `EPSV` then `PASV`).
    async fn open_passive(&mut self, transfer_cmd: &str) -> Result<BoxStream> {
        if self.features.has("PRET") {
            self.ok(&format!("PRET {transfer_cmd}")).await?;
        }
        let mut target = None;
        if self.config.prefer_epsv && !self.epsv_refused {
            let r = self.raw("EPSV").await?;
            if r.code == 229 {
                let port = parse_epsv(r.text()).ok_or_else(|| FtpError::Protocol(format!("unreadable EPSV reply: {r}")))?;
                // EPSV only gives a port: same host as the control channel.
                target = Some(self.server_target(port));
            } else if r.is_permanent_negative() {
                self.epsv_refused = true;
                self.status("EPSV refused: falling back to PASV");
            } else {
                return Err(FtpError::Unexpected { command: "EPSV".into(), reply: r });
            }
        }
        let target = match target {
            Some(t) => t,
            None => {
                let r = self.expect("PASV", |r| r.code == 227).await?;
                let (ip, port) = parse_pasv(r.text()).ok_or_else(|| FtpError::Protocol(format!("unreadable PASV reply: {r}")))?;
                let ip = IpAddr::V4(ip);
                if self.config.pasv_fix_private_ip && needs_fix(ip, self.server_ip) {
                    let t = self.server_target(port);
                    self.status(format!("PASV returns {ip}: using {}", t.host(&self.host)));
                    t
                } else {
                    DataTarget::Addr(SocketAddr::new(ip, port))
                }
            }
        };
        let tcp = self.connect_data(target).await?;
        tcp.set_nodelay(true)?;
        Ok(Box::new(tcp))
    }

    /// Target "the server itself" on `port`: its IP when known without proxy, its name otherwise.
    fn server_target(&self, port: u16) -> DataTarget {
        match (self.server_ip, self.data_via_proxy()) {
            (Some(ip), None) => DataTarget::Addr(SocketAddr::new(ip, port)),
            _ => DataTarget::Host(port),
        }
    }

    /// TCP data connection, direct or through the proxy.
    // Does not keep `&self` across `await`: `FtpClient` is not `Sync`, and the future must
    // stay `Send` to run in the engine tasks.
    fn connect_data(&self, target: DataTarget) -> impl Future<Output = Result<TcpStream>> + Send + 'static {
        let ct = self.config.connect_timeout;
        let proxy = self.data_via_proxy().cloned();
        let host = self.host.clone();
        let bind = self.config.bind_ip;
        async move {
            if let Some(p) = proxy {
                let (h, port) = match target {
                    DataTarget::Addr(a) => (a.ip().to_string(), a.port()),
                    DataTarget::Host(port) => (host, port),
                };
                return timeout_ftp(ct, async { Ok(proxy::connect(&p, &h, port).await?) }).await;
            }
            match target {
                DataTarget::Addr(a) => timeout(ct, crate::net::connect_addr(a, bind)).await,
                DataTarget::Host(port) => timeout(ct, crate::net::connect_from(&host, port, bind)).await,
            }
        }
    }

    /// Sends the transfer command and waits for the 1xx reply. `Ok(false)` when the server
    /// replied 2xx directly (nothing to transfer).
    async fn start_transfer(&mut self, cmd: &str) -> Result<bool> {
        let r = self.raw(cmd).await?;
        if r.is_preliminary() {
            Ok(true)
        } else if r.is_completion() {
            Ok(false)
        } else {
            Err(FtpError::Unexpected { command: cmd.into(), reply: r })
        }
    }

    async fn finish_transfer(&mut self, cmd: &str) -> Result<()> {
        let r = self.read_reply().await?;
        if r.is_completion() { Ok(()) } else { Err(FtpError::Unexpected { command: cmd.into(), reply: r }) }
    }

    /// Lists a directory (the current one when `dir` is `None`).
    pub async fn list(&mut self, dir: Option<&str>) -> Result<Vec<RemoteEntry>> {
        let (raw, mlsd) = self.list_raw(dir).await?;
        let mut entries = if mlsd { listing::parse_mlsd(&raw) } else { listing::parse_list(&raw) };
        let offset = self.config.time_offset_mins;
        if offset != 0 {
            for e in &mut entries {
                e.modified = e.modified.map(|t| t.shifted(offset));
            }
        }
        Ok(entries)
    }

    /// `STAT -l` style listing: the lines of the multi-line reply, on the control channel.
    async fn list_on_control(&mut self, cmd: &str) -> Result<String> {
        let r = self.expect(cmd, |r| matches!(r.code, 211..=213)).await?;
        let code = r.code;
        let inner = r.lines.get(1..r.lines.len().saturating_sub(1)).unwrap_or(&[]);
        let text = inner
            .iter()
            .map(|l| {
                // Some servers repeat the code at the start of every line (`213-drwx...`).
                let l = match l.get(..4) {
                    Some(p) if p.ends_with('-') && p[..3].parse() == Ok(code) => &l[4..],
                    _ => l.as_str(),
                };
                l.trim_start()
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }

    /// Raw listing and "MLSD format" flag.
    pub async fn list_raw(&mut self, dir: Option<&str>) -> Result<(String, bool)> {
        if let Some(d) = dir {
            self.cwd(d).await?;
        }
        let mlsd = self.config.use_mlsd && self.features.has("MLST");
        let cmd = if mlsd { "MLSD".to_string() } else { self.config.list_command.trim().to_string() };
        if !mlsd && cmd.get(..4).is_some_and(|v| v.eq_ignore_ascii_case("STAT")) {
            return Ok((self.list_on_control(&cmd).await?, false));
        }
        self.set_type(TransferType::Ascii).await?;
        let pending = self.prepare_data(&cmd).await?;
        if !self.start_transfer(&cmd).await? {
            return Ok((String::new(), mlsd));
        }
        let mut data = self.complete_data(pending).await?;
        let io = self.config.io_timeout;
        let mut buf = Vec::new();
        let mut chunk = vec![0u8; CHUNK];
        loop {
            let n = timeout(io, read_data(&mut data, &mut chunk)).await?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        drop(data);
        self.finish_transfer(&cmd).await?;
        let mut text = self.decode(&buf);
        if self.config.trim_list_spaces && !mlsd {
            text = text.lines().map(str::trim_start).collect::<Vec<_>>().join("\n");
        }
        Ok((text, mlsd))
    }

    /// Downloads `remote` into `out`, from byte `offset` (resume through `REST`).
    /// `progress` receives the number of bytes received during this call. Returns it.
    pub async fn retr<W>(&mut self, remote: &str, offset: u64, out: &mut W, mut progress: impl FnMut(u64)) -> Result<u64>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let name = self.enter_parent(remote).await?;
        let cmd = format!("RETR {name}");
        self.set_type(TransferType::Binary).await?;
        let pending = self.prepare_data(&cmd).await?;
        if offset > 0 {
            self.expect(&format!("REST {offset}"), |r| r.code == 350).await?;
        }
        if !self.start_transfer(&cmd).await? {
            return Ok(0);
        }
        let mut data = self.complete_data(pending).await?;
        let io = self.config.io_timeout;
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            let n = timeout(io, read_data(&mut data, &mut buf)).await?;
            if n == 0 {
                break;
            }
            timeout(io, out.write_all(&buf[..n])).await?;
            total += n as u64;
            progress(total);
        }
        drop(data);
        out.flush().await?;
        self.finish_transfer(&cmd).await?;
        Ok(total)
    }

    /// Uploads `input` to `remote`. With `offset > 0`, resumes the upload (`REST` + `STOR`):
    /// `input` must then already be positioned at `offset`.
    pub async fn stor<R>(&mut self, remote: &str, offset: u64, input: &mut R, mut progress: impl FnMut(u64)) -> Result<u64>
    where
        R: AsyncRead + Unpin + ?Sized,
    {
        let name = self.enter_parent(remote).await?;
        let cmd = format!("STOR {name}");
        self.set_type(TransferType::Binary).await?;
        let pending = self.prepare_data(&cmd).await?;
        if offset > 0 {
            self.expect(&format!("REST {offset}"), |r| r.code == 350).await?;
        }
        if !self.start_transfer(&cmd).await? {
            return Ok(0);
        }
        let mut data = self.complete_data(pending).await?;
        let io = self.config.io_timeout;
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            let n = timeout(io, input.read(&mut buf)).await?;
            if n == 0 {
                break;
            }
            timeout(io, data.write_all(&buf[..n])).await?;
            total += n as u64;
            progress(total);
        }
        // Sends the end of stream (close_notify with TLS) but keeps the socket open until
        // the 226 reply: closing it at once would make the OS answer the last packets of
        // the server with an RST (TLS 1.3 tickets...), and the server could lose the end of
        // the file.
        timeout(io, data.shutdown()).await?;
        let done = self.finish_transfer(&cmd).await;
        drop(data);
        done?;
        Ok(total)
    }
}

// -------------------------------------------------------------------- helpers

/// Destination of a passive data connection.
enum DataTarget {
    Addr(SocketAddr),
    /// The host name of the control channel, on this port.
    Host(u16),
}

impl DataTarget {
    fn host(&self, control_host: &str) -> String {
        match self {
            DataTarget::Addr(a) => a.ip().to_string(),
            DataTarget::Host(_) => control_host.to_string(),
        }
    }
}

/// Data channel being opened.
enum PendingData {
    /// Passive mode: already connected to the server.
    Connected(BoxStream),
    /// Active mode: the server will connect to us.
    Listening(TcpListener),
}

/// Reading on a data channel. Many servers close the TLS connection without
/// `close_notify`: treated as a normal end, the 226 reply confirming the transfer.
async fn read_data(data: &mut BoxStream, buf: &mut [u8]) -> std::io::Result<usize> {
    match data.read(buf).await {
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(0),
        r => r,
    }
}

/// Failure of the passive mode that the active mode can work around: refusal of
/// `PASV`/`EPSV`, unreadable reply, or data connection impossible.
fn passive_failure(e: &FtpError) -> bool {
    match e {
        FtpError::Unexpected { command, reply } => {
            matches!(command.as_str(), "PASV" | "EPSV") && (reply.is_permanent_negative() || reply.is_transient_negative())
        }
        FtpError::Io(_) | FtpError::Timeout => true,
        FtpError::Protocol(m) => m.starts_with("unreadable"),
        _ => false,
    }
}

async fn timeout<T>(d: Duration, f: impl Future<Output = std::io::Result<T>>) -> Result<T> {
    match tokio::time::timeout(d, f).await {
        Ok(r) => Ok(r?),
        Err(_) => Err(FtpError::Timeout),
    }
}

async fn timeout_ftp<T>(d: Duration, f: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(d, f).await.unwrap_or(Err(FtpError::Timeout))
}

/// Masks secrets in the log.
pub(super) fn mask(cmd: &str) -> String {
    let upper = cmd.get(..5).map(str::to_ascii_uppercase);
    match upper.as_deref() {
        Some("PASS ") | Some("ACCT ") => format!("{}****", &cmd[..5]),
        _ => cmd.to_string(),
    }
}

/// `257 "/path ""quoted"""` → `/path "quoted"`.
fn parse_pwd(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let mut out = String::new();
    let mut chars = text[start..].chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' {
            if chars.peek() == Some(&'"') {
                chars.next();
                out.push('"');
            } else {
                return Some(out);
            }
        } else {
            out.push(c);
        }
    }
    None
}

/// `Entering Passive Mode (h1,h2,h3,h4,p1,p2)` (parentheses optional).
pub(super) fn parse_pasv(text: &str) -> Option<(Ipv4Addr, u16)> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let rest = &text[start..];
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == ',')).unwrap_or(rest.len());
    let n: Vec<u8> = rest[..end].split(',').map(str::parse).collect::<Result<_, _>>().ok()?;
    if n.len() != 6 {
        return None;
    }
    Some((Ipv4Addr::new(n[0], n[1], n[2], n[3]), u16::from(n[4]) << 8 | u16::from(n[5])))
}

/// `Entering Extended Passive Mode (|||6446|)`.
pub(super) fn parse_epsv(text: &str) -> Option<u16> {
    let inner = &text[text.find('(')? + 1..];
    let delim = inner.chars().next()?;
    let parts: Vec<&str> = inner.split(delim).collect();
    if parts.len() < 5 {
        return None;
    }
    parts[3].parse().ok()
}

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
    }
}

/// The PASV address is unusable: 0.0.0.0, or not routable while the server is public (or
/// its address is unknown, behind a proxy).
pub(super) fn needs_fix(pasv: IpAddr, server: Option<IpAddr>) -> bool {
    pasv.is_unspecified() || (is_private(pasv) && server.is_none_or(|s| !is_private(s)))
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasv_epsv_pwd() {
        assert_eq!(parse_pasv("Entering Passive Mode (192,168,1,2,19,137)."), Some((Ipv4Addr::new(192, 168, 1, 2), 19 * 256 + 137)));
        assert_eq!(parse_pasv("Entering Passive Mode 10,0,0,1,0,21"), Some((Ipv4Addr::new(10, 0, 0, 1), 21)));
        assert_eq!(parse_pasv("no numbers"), None);
        assert_eq!(parse_epsv("Entering Extended Passive Mode (|||6446|)"), Some(6446));
        assert_eq!(parse_epsv("Entering Extended Passive Mode (!!!21!)"), Some(21));
        assert_eq!(parse_pwd(r#""/pub/a ""b""" is current"#).as_deref(), Some(r#"/pub/a "b""#));
    }

    #[test]
    fn pasv_ip_fix() {
        let public: Option<IpAddr> = "203.0.113.5".parse().ok();
        assert!(needs_fix("192.168.0.10".parse().unwrap(), public));
        assert!(!needs_fix("192.168.0.10".parse().unwrap(), "192.168.0.1".parse().ok()));
        assert!(needs_fix("0.0.0.0".parse().unwrap(), "127.0.0.1".parse().ok()));
        assert!(!needs_fix("198.51.100.7".parse().unwrap(), public));
        // Server with an unknown IP (name resolved by the proxy): a private IP is replaced.
        assert!(needs_fix("10.1.2.3".parse().unwrap(), None));
    }

    #[test]
    fn password_is_masked() {
        assert_eq!(mask("PASS secret"), "PASS ****");
        assert_eq!(mask("pass secret"), "pass ****");
        assert_eq!(mask("USER bob"), "USER bob");
    }
}
