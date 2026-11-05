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
use super::{path, BoxStream, Features, FtpError, Reply, Result};

/// Taille des blocs de copie des transferts.
const CHUNK: usize = 64 * 1024;

/// Réglages d'une connexion FTP.
#[derive(Debug, Clone)]
pub struct FtpConfig {
    /// Délai d'établissement des connexions de contrôle et de données.
    pub connect_timeout: Duration,
    /// Délai maximal d'attente d'une réponse ou d'un bloc de données.
    pub io_timeout: Duration,
    /// Essayer `EPSV` avant `PASV` (repli automatique si refusé).
    pub prefer_epsv: bool,
    /// Utiliser `MLSD` si le serveur annonce `MLST`.
    pub use_mlsd: bool,
    /// Commande de listing quand `MLSD` n'est pas utilisé (ex. `LIST`, `LIST -a`, `STAT -l`…).
    pub list_command: String,
    /// Remplacer l'adresse privée renvoyée par `PASV` par celle du serveur (NAT mal configuré).
    pub pasv_fix_private_ip: bool,
    /// Longueur maximale d'une ligne de réponse.
    pub max_line_len: usize,
    /// FTP en clair, FTPS explicite ou implicite.
    pub security: Security,
    /// Mode actif (`PORT`/`EPRT`) au lieu du mode passif.
    pub active_mode: bool,
    /// Adresse annoncée en mode actif (IP publique derrière un NAT) ; par défaut l'IP locale.
    pub active_external_ip: Option<IpAddr>,
    /// Plage de ports locaux pour le mode actif (redirections de ports du routeur).
    pub active_port_range: Option<(u16, u16)>,
    /// Proxy (SOCKS4/4A/5, HTTP Tunnel). Ignoré si ses règles de contournement s'appliquent.
    pub proxy: Option<ProxyConfig>,
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
        }
    }
}

/// Ligne de journal émise par le client (affichée dans le log FTP de l'onglet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogLine {
    /// Commande envoyée (mot de passe masqué).
    Command(String),
    /// Réponse reçue.
    Reply(Reply),
    /// Information du client lui-même.
    Status(String),
}

/// Destinataire des lignes de journal.
pub type LogSink = Arc<dyn Fn(LogLine) + Send + Sync>;

/// Type de représentation (`TYPE A` / `TYPE I`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Ascii,
    Binary,
}

/// Connexion FTP ouverte.
///
/// Les opérations sur un chemin se placent d'abord dans son répertoire puis agissent sur le
/// nom seul : un serveur qui refuse les chemins absolus trop longs reste utilisable, car
/// [`FtpClient::cwd`] se replie sur une navigation segment par segment.
pub struct FtpClient {
    ctrl: BufStream<BoxStream>,
    host: String,
    /// IP du serveur FTP si connue (inconnue quand le proxy résout le nom lui-même).
    server_ip: Option<IpAddr>,
    local: SocketAddr,
    /// Proxy effectivement utilisé par cette connexion.
    proxy: Option<ProxyConfig>,
    proxy_active_warned: bool,
    tls: Option<TlsContext>,
    /// Canaux de données chiffrés (`PROT P` accepté).
    prot_p: bool,
    config: FtpConfig,
    features: Features,
    welcome: Reply,
    cwd: Option<String>,
    current_type: Option<TransferType>,
    epsv_refused: bool,
    log: Option<LogSink>,
}

impl FtpClient {
    /// Ouvre la connexion TCP, applique la sécurité demandée (TLS implicite ou `AUTH TLS`)
    /// et lit le message d'accueil.
    pub async fn connect(host: &str, port: u16, config: FtpConfig) -> Result<Self> {
        Self::connect_with_log(host, port, config, None).await
    }

    /// Comme [`FtpClient::connect`], avec un journal actif dès l'accueil du serveur.
    pub async fn connect_with_log(host: &str, port: u16, config: FtpConfig, log: Option<LogSink>) -> Result<Self> {
        let proxy = config.proxy.clone().filter(|p| !p.bypasses(host));
        let ct = config.connect_timeout;
        let (tcp, server_ip) = match &proxy {
            Some(p) => {
                if let Some(log) = &log {
                    log(LogLine::Status(format!("Connexion via proxy {:?} {}:{}", p.kind, p.host, p.port)));
                }
                let tcp = timeout_ftp(ct, async { Ok(proxy::connect(p, host, port).await?) }).await?;
                // Sans proxy sur les données, il faut l'IP réelle du serveur.
                let ip = match host.parse::<IpAddr>() {
                    Ok(ip) => Some(ip),
                    Err(_) if !p.apply_to_data => lookup_host((host, port)).await.ok().and_then(|mut a| a.next()).map(|a| a.ip()),
                    Err(_) => None,
                };
                (tcp, ip)
            }
            None => {
                let tcp = timeout(ct, TcpStream::connect((host, port))).await?;
                let ip = tcp.peer_addr()?.ip();
                (tcp, Some(ip))
            }
        };
        tcp.set_nodelay(true)?;
        let local = tcp.local_addr()?;
        Self::start(Box::new(tcp), host, server_ip, local, proxy, config, log).await
    }

    /// Démarre sur un flux déjà établi par l'appelant (tunnel particulier…).
    /// `host` sert à la vérification TLS et aux connexions de données, `server_ip` est l'IP
    /// réelle du serveur si connue, `local` l'adresse annoncée en mode actif.
    /// Le proxy éventuel de `config` n'est pas utilisé.
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
            log,
        };
        let mut welcome = c.read_reply().await?;
        while welcome.code == 120 {
            welcome = c.read_reply().await?;
        }
        if !welcome.is_completion() {
            return Err(FtpError::Unexpected { command: "(connexion)".into(), reply: welcome });
        }
        c.welcome = welcome;
        if matches!(c.config.security, Security::Explicit(_)) {
            c.auth_tls().await?;
        }
        Ok(c)
    }

    /// Passe le canal de contrôle en TLS (`AUTH TLS`, repli `AUTH SSL` si autorisé).
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
        self.status("Canal de contrôle chiffré (TLS)");
        self.tls = Some(ctx);
        Ok(())
    }

    /// Le canal de contrôle est-il chiffré ?
    pub fn is_secure(&self) -> bool {
        self.tls.is_some()
    }

    /// Les canaux de données sont-ils chiffrés (`PROT P`) ?
    pub fn is_data_protected(&self) -> bool {
        self.prot_p
    }

    /// Empreinte SHA-256 du certificat du serveur (pour l'afficher ou l'approuver).
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
    /// IP du serveur FTP, si connue.
    pub fn server_ip(&self) -> Option<IpAddr> {
        self.server_ip
    }

    /// La connexion passe-t-elle par un proxy ?
    pub fn uses_proxy(&self) -> bool {
        self.proxy.is_some()
    }

    fn data_via_proxy(&self) -> Option<&ProxyConfig> {
        self.proxy.as_ref().filter(|p| p.apply_to_data)
    }
    /// Répertoire courant connu côté client (`None` si inconnu).
    pub fn current_dir(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    // ---------------------------------------------------------------- commandes de base

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

    /// Lit une réponse avec un délai donné (`None` : sans limite, pour la fin d'un FXP).
    pub(super) async fn read_reply_within(&mut self, limit: Option<Duration>) -> Result<Reply> {
        let max = self.config.max_line_len;
        let reply = match limit {
            Some(d) => match tokio::time::timeout(d, read_reply(&mut self.ctrl, max)).await {
                Ok(r) => r?,
                Err(_) => return Err(FtpError::Timeout),
            },
            None => read_reply(&mut self.ctrl, max).await?,
        };
        self.emit(LogLine::Reply(reply.clone()));
        Ok(reply)
    }

    /// Interrompt le transfert en cours (`ABOR`) et consomme les réponses associées
    /// (`426` du transfert puis `226`/`225` de l'ABOR, selon les serveurs).
    pub async fn abort(&mut self) -> Result<()> {
        self.send("ABOR").await?;
        for _ in 0..3 {
            let r = self.read_reply().await?;
            if r.is_completion() {
                // Un serveur qui venait de finir répond 226 (transfert) puis 226 (ABOR) :
                // on absorbe la seconde réponse si elle arrive vite.
                if let Ok(Ok(extra)) =
                    tokio::time::timeout(Duration::from_millis(500), read_reply(&mut self.ctrl, self.config.max_line_len)).await
                {
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

    /// Change la protection des canaux de données (`PROT P` / `PROT C`).
    pub async fn set_data_protection(&mut self, protect: bool) -> Result<()> {
        if self.prot_p == protect {
            return Ok(());
        }
        if protect && self.tls.is_none() {
            return Err(FtpError::InvalidArgument("PROT P impossible sans TLS sur le contrôle".into()));
        }
        self.ok(if protect { "PROT P" } else { "PROT C" }).await?;
        self.prot_p = protect;
        Ok(())
    }

    pub(super) async fn send(&mut self, cmd: &str) -> Result<()> {
        if cmd.contains(['\r', '\n']) {
            return Err(FtpError::InvalidArgument(
                "retour à la ligne interdit dans une commande FTP".into(),
            ));
        }
        self.emit(LogLine::Command(mask(cmd)));
        let io = self.config.io_timeout;
        timeout(io, async {
            self.ctrl.write_all(cmd.as_bytes()).await?;
            self.ctrl.write_all(b"\r\n").await?;
            self.ctrl.flush().await
        })
        .await
    }

    /// Envoie une commande et renvoie la réponse, quel que soit son code.
    pub async fn raw(&mut self, cmd: &str) -> Result<Reply> {
        self.send(cmd).await?;
        self.read_reply().await
    }

    /// Envoie une commande et exige une réponse 2xx.
    pub(super) async fn ok(&mut self, cmd: &str) -> Result<Reply> {
        self.expect(cmd, |r| r.is_completion()).await
    }

    pub(super) async fn expect(&mut self, cmd: &str, accept: impl Fn(&Reply) -> bool) -> Result<Reply> {
        let reply = self.raw(cmd).await?;
        if accept(&reply) {
            Ok(reply)
        } else {
            Err(FtpError::Unexpected { command: mask(cmd), reply })
        }
    }

    // ---------------------------------------------------------------- session

    /// Authentification `USER` / `PASS`, puis `FEAT`, `OPTS UTF8 ON` et `PWD`.
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
        self.refresh_features().await?;
        if self.features.has("UTF8") {
            // Échec sans importance : certains serveurs sont déjà en UTF-8.
            let _ = self.raw("OPTS UTF8 ON").await?;
        }
        self.cwd = self.pwd().await.ok();
        Ok(())
    }

    /// Relit les capacités du serveur (à refaire après `AUTH TLS`).
    pub async fn refresh_features(&mut self) -> Result<&Features> {
        let r = self.raw("FEAT").await?;
        self.features = if r.code == 211 { Features::parse(&r) } else { Features::default() };
        Ok(&self.features)
    }

    pub async fn noop(&mut self) -> Result<()> {
        self.ok("NOOP").await.map(drop)
    }

    /// `QUIT` puis fermeture ; les erreurs de fermeture sont ignorées.
    pub async fn quit(mut self) -> Result<()> {
        let r = self.raw("QUIT").await;
        let _ = self.ctrl.shutdown().await;
        r.map(drop)
    }

    // ---------------------------------------------------------------- navigation

    pub async fn pwd(&mut self) -> Result<String> {
        let r = self.expect("PWD", |r| r.code == 257).await?;
        parse_pwd(r.text()).ok_or_else(|| FtpError::Protocol(format!("réponse PWD illisible : {r}")))
    }

    fn resolve(&self, p: &str) -> Option<String> {
        if p.starts_with('/') {
            Some(path::normalize(p))
        } else {
            self.cwd.as_deref().map(|c| path::join(c, p))
        }
    }

    /// Change de répertoire. Si le serveur refuse le chemin entier (5xx), le client
    /// réessaie segment par segment, ce qui contourne les limites de longueur des serveurs.
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
        self.status("Chemin refusé en entier : navigation dossier par dossier");
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

    /// Se place dans le répertoire parent de `p` et renvoie le nom seul.
    pub(super) async fn enter_parent<'p>(&mut self, p: &'p str) -> Result<&'p str> {
        let (dir, name) = path::split(p);
        if name.is_empty() {
            return Err(FtpError::InvalidArgument(format!("chemin sans nom : {p:?}")));
        }
        if let Some(dir) = dir {
            self.cwd(dir).await?;
        }
        Ok(name)
    }

    // ---------------------------------------------------------------- fichiers et dossiers

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

    /// Renomme ou déplace. `to` peut être un nom simple (même dossier) ou un chemin.
    pub async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let name = self.enter_parent(from).await?;
        self.expect(&format!("RNFR {name}"), |r| r.code == 350).await?;
        self.ok(&format!("RNTO {to}")).await.map(drop)
    }

    /// Taille d'un fichier (`SIZE`, en binaire).
    pub async fn size(&mut self, p: &str) -> Result<u64> {
        let name = self.enter_parent(p).await?;
        self.set_type(TransferType::Binary).await?;
        let r = self.expect(&format!("SIZE {name}"), |r| r.code == 213).await?;
        r.text().trim().parse().map_err(|_| FtpError::Protocol(format!("réponse SIZE illisible : {r}")))
    }

    /// Date de modification (`MDTM`).
    pub async fn mdtm(&mut self, p: &str) -> Result<listing::FtpTime> {
        let name = self.enter_parent(p).await?;
        let r = self.expect(&format!("MDTM {name}"), |r| r.code == 213).await?;
        listing::parse_mlsd_time(r.text().trim())
            .ok_or_else(|| FtpError::Protocol(format!("réponse MDTM illisible : {r}")))
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

    // ---------------------------------------------------------------- canal de données

    /// Prépare le canal de données avant la commande de transfert : connexion passive,
    /// ou écoute locale annoncée par `PORT`/`EPRT` en mode actif.
    async fn prepare_data(&mut self, transfer_cmd: &str) -> Result<PendingData> {
        let active = self.config.active_mode && self.data_via_proxy().is_none();
        if self.config.active_mode && !active && !self.proxy_active_warned {
            self.proxy_active_warned = true;
            self.status("Mode actif impossible à travers le proxy : utilisation du mode passif");
        }
        if active {
            self.open_active().await.map(PendingData::Listening)
        } else {
            self.open_passive(transfer_cmd).await.map(PendingData::Connected)
        }
    }

    /// Termine l'ouverture après la réponse 1xx : accepte la connexion active (uniquement
    /// depuis l'adresse du serveur), puis négocie TLS si `PROT P`.
    async fn complete_data(&mut self, pending: PendingData) -> Result<BoxStream> {
        let stream: BoxStream = match pending {
            PendingData::Connected(s) => s,
            PendingData::Listening(listener) => {
                let (tcp, from) = timeout(self.config.connect_timeout, listener.accept()).await?;
                if let Some(server) = self.server_ip
                    && canonical(from.ip()) != canonical(server)
                {
                    return Err(FtpError::Protocol(format!(
                        "connexion de données refusée : elle vient de {from} et non du serveur {server}"
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

    /// Mode actif : écoute sur l'IP locale du canal de contrôle et annonce l'adresse.
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
                found.ok_or_else(|| FtpError::Protocol(format!("aucun port libre entre {first} et {last}")))?
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

    /// Ouvre une connexion de données passive (`PRET` si annoncé, `EPSV` puis `PASV`).
    async fn open_passive(&mut self, transfer_cmd: &str) -> Result<BoxStream> {
        if self.features.has("PRET") {
            self.ok(&format!("PRET {transfer_cmd}")).await?;
        }
        let mut target = None;
        if self.config.prefer_epsv && !self.epsv_refused {
            let r = self.raw("EPSV").await?;
            if r.code == 229 {
                let port = parse_epsv(r.text())
                    .ok_or_else(|| FtpError::Protocol(format!("réponse EPSV illisible : {r}")))?;
                // EPSV ne donne qu'un port : même hôte que le canal de contrôle.
                target = Some(self.server_target(port));
            } else if r.is_permanent_negative() {
                self.epsv_refused = true;
                self.status("EPSV refusé : repli sur PASV");
            } else {
                return Err(FtpError::Unexpected { command: "EPSV".into(), reply: r });
            }
        }
        let target = match target {
            Some(t) => t,
            None => {
                let r = self.expect("PASV", |r| r.code == 227).await?;
                let (ip, port) = parse_pasv(r.text())
                    .ok_or_else(|| FtpError::Protocol(format!("réponse PASV illisible : {r}")))?;
                let ip = IpAddr::V4(ip);
                if self.config.pasv_fix_private_ip && needs_fix(ip, self.server_ip) {
                    let t = self.server_target(port);
                    self.status(format!("PASV renvoie {ip} : utilisation de {}", t.host(&self.host)));
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

    /// Cible « le serveur lui-même » sur `port` : son IP si connue hors proxy, sinon son nom.
    fn server_target(&self, port: u16) -> DataTarget {
        match (self.server_ip, self.data_via_proxy()) {
            (Some(ip), None) => DataTarget::Addr(SocketAddr::new(ip, port)),
            _ => DataTarget::Host(port),
        }
    }

    /// Connexion TCP de données, directe ou à travers le proxy.
    async fn connect_data(&self, target: DataTarget) -> Result<TcpStream> {
        let ct = self.config.connect_timeout;
        if let Some(p) = self.data_via_proxy() {
            let (host, port) = match &target {
                DataTarget::Addr(a) => (a.ip().to_string(), a.port()),
                DataTarget::Host(port) => (self.host.clone(), *port),
            };
            return timeout_ftp(ct, async { Ok(proxy::connect(p, &host, port).await?) }).await;
        }
        match target {
            DataTarget::Addr(a) => timeout(ct, TcpStream::connect(a)).await,
            DataTarget::Host(port) => timeout(ct, TcpStream::connect((self.host.as_str(), port))).await,
        }
    }

    /// Envoie la commande de transfert et attend la réponse 1xx. `Ok(false)` si le serveur
    /// a répondu 2xx directement (rien à transférer).
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
        if r.is_completion() {
            Ok(())
        } else {
            Err(FtpError::Unexpected { command: cmd.into(), reply: r })
        }
    }

    /// Liste un répertoire (le répertoire courant si `dir` vaut `None`).
    pub async fn list(&mut self, dir: Option<&str>) -> Result<Vec<RemoteEntry>> {
        let (raw, mlsd) = self.list_raw(dir).await?;
        Ok(if mlsd { listing::parse_mlsd(&raw) } else { listing::parse_list(&raw) })
    }

    /// Listing brut et indicateur « format MLSD ».
    pub async fn list_raw(&mut self, dir: Option<&str>) -> Result<(String, bool)> {
        if let Some(d) = dir {
            self.cwd(d).await?;
        }
        let mlsd = self.config.use_mlsd && self.features.has("MLST");
        let cmd = if mlsd { "MLSD".to_string() } else { self.config.list_command.clone() };
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
        Ok((String::from_utf8_lossy(&buf).into_owned(), mlsd))
    }

    /// Télécharge `remote` vers `out`, à partir de l'octet `offset` (reprise via `REST`).
    /// `progress` reçoit le nombre d'octets reçus pendant cet appel. Renvoie ce nombre.
    pub async fn retr<W>(
        &mut self,
        remote: &str,
        offset: u64,
        out: &mut W,
        mut progress: impl FnMut(u64),
    ) -> Result<u64>
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

    /// Envoie `input` vers `remote`. Avec `offset > 0`, reprend l'envoi (`REST` + `STOR`) :
    /// `input` doit alors déjà être positionné à `offset`.
    pub async fn stor<R>(
        &mut self,
        remote: &str,
        offset: u64,
        input: &mut R,
        mut progress: impl FnMut(u64),
    ) -> Result<u64>
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
        // Envoie la fin de flux (close_notify en TLS) mais garde la socket ouverte jusqu'à
        // la réponse 226 : la fermer tout de suite ferait répondre l'OS par un RST aux
        // derniers paquets du serveur (tickets TLS 1.3…), et celui-ci pourrait perdre la fin
        // du fichier.
        timeout(io, data.shutdown()).await?;
        let done = self.finish_transfer(&cmd).await;
        drop(data);
        done?;
        Ok(total)
    }
}

// -------------------------------------------------------------------- utilitaires

/// Destination d'une connexion de données passive.
enum DataTarget {
    Addr(SocketAddr),
    /// Le nom d'hôte du canal de contrôle, sur ce port.
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

/// Canal de données en cours d'ouverture.
enum PendingData {
    /// Mode passif : déjà connecté au serveur.
    Connected(BoxStream),
    /// Mode actif : le serveur va se connecter à nous.
    Listening(TcpListener),
}

/// Lecture sur un canal de données. Beaucoup de serveurs ferment la connexion TLS sans
/// `close_notify` : on la traite comme une fin normale, la réponse 226 confirmant le transfert.
async fn read_data(data: &mut BoxStream, buf: &mut [u8]) -> std::io::Result<usize> {
    match data.read(buf).await {
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(0),
        r => r,
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

/// Masque les secrets dans le journal.
pub(super) fn mask(cmd: &str) -> String {
    let upper = cmd.get(..5).map(str::to_ascii_uppercase);
    match upper.as_deref() {
        Some("PASS ") | Some("ACCT ") => format!("{}****", &cmd[..5]),
        _ => cmd.to_string(),
    }
}

/// `257 "/chemin ""cité"""` → `/chemin "cité"`.
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

/// `Entering Passive Mode (h1,h2,h3,h4,p1,p2)` (parenthèses facultatives).
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
fn parse_epsv(text: &str) -> Option<u16> {
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

/// L'adresse PASV est inutilisable : 0.0.0.0, ou non routable alors que le serveur est public
/// (ou d'adresse inconnue, derrière un proxy).
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
        assert_eq!(
            parse_pasv("Entering Passive Mode (192,168,1,2,19,137)."),
            Some((Ipv4Addr::new(192, 168, 1, 2), 19 * 256 + 137))
        );
        assert_eq!(parse_pasv("Entering Passive Mode 10,0,0,1,0,21"), Some((Ipv4Addr::new(10, 0, 0, 1), 21)));
        assert_eq!(parse_pasv("pas de nombres"), None);
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
        // Serveur d'IP inconnue (nom résolu par le proxy) : une IP privée est remplacée.
        assert!(needs_fix("10.1.2.3".parse().unwrap(), None));
    }

    #[test]
    fn password_is_masked() {
        assert_eq!(mask("PASS secret"), "PASS ****");
        assert_eq!(mask("pass secret"), "pass ****");
        assert_eq!(mask("USER bob"), "USER bob");
    }
}
