use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufStream};
use tokio::net::TcpStream;

use super::listing::{self, RemoteEntry};
use super::reply::read_reply;
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
}

impl Default for FtpConfig {
    fn default() -> Self {
        Self {
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
    peer: SocketAddr,
    config: FtpConfig,
    features: Features,
    welcome: Reply,
    cwd: Option<String>,
    current_type: Option<TransferType>,
    epsv_refused: bool,
    log: Option<LogSink>,
}

impl FtpClient {
    /// Ouvre une connexion TCP et lit le message d'accueil.
    pub async fn connect(host: &str, port: u16, config: FtpConfig) -> Result<Self> {
        let tcp = timeout(config.connect_timeout, TcpStream::connect((host, port))).await?;
        tcp.set_nodelay(true)?;
        let peer = tcp.peer_addr()?;
        Self::from_stream(Box::new(tcp), peer, config, None).await
    }

    /// Démarre sur un flux déjà établi (TLS implicite, tunnel proxy…).
    pub async fn from_stream(
        stream: BoxStream,
        peer: SocketAddr,
        config: FtpConfig,
        log: Option<LogSink>,
    ) -> Result<Self> {
        let mut c = Self {
            ctrl: BufStream::new(stream),
            peer,
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
        Ok(c)
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
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer
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

    fn status(&self, msg: impl Into<String>) {
        self.emit(LogLine::Status(msg.into()));
    }

    async fn read_reply(&mut self) -> Result<Reply> {
        let max = self.config.max_line_len;
        let reply = match tokio::time::timeout(self.config.io_timeout, read_reply(&mut self.ctrl, max)).await {
            Ok(r) => r?,
            Err(_) => return Err(FtpError::Timeout),
        };
        self.emit(LogLine::Reply(reply.clone()));
        Ok(reply)
    }

    async fn send(&mut self, cmd: &str) -> Result<()> {
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
    async fn ok(&mut self, cmd: &str) -> Result<Reply> {
        self.expect(cmd, |r| r.is_completion()).await
    }

    async fn expect(&mut self, cmd: &str, accept: impl Fn(&Reply) -> bool) -> Result<Reply> {
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
    async fn enter_parent<'p>(&mut self, p: &'p str) -> Result<&'p str> {
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

    /// Ouvre une connexion de données passive (`PRET` si annoncé, `EPSV` puis `PASV`).
    async fn open_passive(&mut self, transfer_cmd: &str) -> Result<BoxStream> {
        if self.features.has("PRET") {
            self.ok(&format!("PRET {transfer_cmd}")).await?;
        }
        let mut addr = None;
        if self.config.prefer_epsv && !self.epsv_refused {
            let r = self.raw("EPSV").await?;
            if r.code == 229 {
                let port = parse_epsv(r.text())
                    .ok_or_else(|| FtpError::Protocol(format!("réponse EPSV illisible : {r}")))?;
                addr = Some(SocketAddr::new(self.peer.ip(), port));
            } else if r.is_permanent_negative() {
                self.epsv_refused = true;
                self.status("EPSV refusé : repli sur PASV");
            } else {
                return Err(FtpError::Unexpected { command: "EPSV".into(), reply: r });
            }
        }
        let addr = match addr {
            Some(a) => a,
            None => {
                let r = self.expect("PASV", |r| r.code == 227).await?;
                let (ip, port) = parse_pasv(r.text())
                    .ok_or_else(|| FtpError::Protocol(format!("réponse PASV illisible : {r}")))?;
                let ip = IpAddr::V4(ip);
                if self.config.pasv_fix_private_ip && needs_fix(ip, self.peer.ip()) {
                    self.status(format!("PASV renvoie {ip} : utilisation de {}", self.peer.ip()));
                    SocketAddr::new(self.peer.ip(), port)
                } else {
                    SocketAddr::new(ip, port)
                }
            }
        };
        let tcp = timeout(self.config.connect_timeout, TcpStream::connect(addr)).await?;
        tcp.set_nodelay(true)?;
        Ok(Box::new(tcp))
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
        let mut data = self.open_passive(&cmd).await?;
        if !self.start_transfer(&cmd).await? {
            return Ok((String::new(), mlsd));
        }
        let mut buf = Vec::new();
        timeout(self.config.io_timeout, data.read_to_end(&mut buf)).await?;
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
        let mut data = self.open_passive(&cmd).await?;
        if offset > 0 {
            self.expect(&format!("REST {offset}"), |r| r.code == 350).await?;
        }
        if !self.start_transfer(&cmd).await? {
            return Ok(0);
        }
        let io = self.config.io_timeout;
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            let n = timeout(io, data.read(&mut buf)).await?;
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
        let mut data = self.open_passive(&cmd).await?;
        if offset > 0 {
            self.expect(&format!("REST {offset}"), |r| r.code == 350).await?;
        }
        if !self.start_transfer(&cmd).await? {
            return Ok(0);
        }
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
        timeout(io, data.shutdown()).await?;
        drop(data);
        self.finish_transfer(&cmd).await?;
        Ok(total)
    }
}

// -------------------------------------------------------------------- utilitaires

async fn timeout<T>(d: Duration, f: impl Future<Output = std::io::Result<T>>) -> Result<T> {
    match tokio::time::timeout(d, f).await {
        Ok(r) => Ok(r?),
        Err(_) => Err(FtpError::Timeout),
    }
}

/// Masque les secrets dans le journal.
fn mask(cmd: &str) -> String {
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
fn parse_pasv(text: &str) -> Option<(Ipv4Addr, u16)> {
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

/// L'adresse PASV est inutilisable (non routable alors que le serveur est public, ou 0.0.0.0).
fn needs_fix(pasv: IpAddr, peer: IpAddr) -> bool {
    pasv.is_unspecified() || (is_private(pasv) && !is_private(peer))
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
        let public: IpAddr = "203.0.113.5".parse().unwrap();
        assert!(needs_fix("192.168.0.10".parse().unwrap(), public));
        assert!(!needs_fix("192.168.0.10".parse().unwrap(), "192.168.0.1".parse().unwrap()));
        assert!(needs_fix("0.0.0.0".parse().unwrap(), "127.0.0.1".parse().unwrap()));
        assert!(!needs_fix("198.51.100.7".parse().unwrap(), public));
    }

    #[test]
    fn password_is_masked() {
        assert_eq!(mask("PASS secret"), "PASS ****");
        assert_eq!(mask("pass secret"), "pass ****");
        assert_eq!(mask("USER bob"), "USER bob");
    }
}
