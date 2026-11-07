//! Common remote session (FTP/FTPS or SFTP) and site profile, for the queue engine.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use urftp_core::exists::ExistsRules;
use urftp_core::priority::PriorityList;
use urftp_core::queue::SiteLimits;
use urftp_core::skiplist::SkipList;

use crate::ftp::{FtpClient, FtpConfig, FtpError, LogLine, LogSink, RemoteEntry, Reply, path};
use crate::ident::IdentRegistrar;
use crate::keepalive::SessionPolicy;
use crate::sftp::{SftpAuth, SftpClient, SftpConfig, SftpError};

/// Protocol and settings of a site.
#[derive(Debug, Clone)]
pub enum SiteProtocol {
    /// FTP or FTPS (see `FtpConfig::security`).
    Ftp(FtpConfig),
    /// SFTP; `key` replaces the password when present.
    Sftp { config: SftpConfig, key: Option<SftpAuth> },
}

/// Everything needed to open connections to a site.
#[derive(Debug, Clone)]
pub struct SiteProfile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub protocol: SiteProtocol,
    pub limits: SiteLimits,
    /// Keep alive and reconnection.
    pub session: SessionPolicy,
    /// Other addresses of the site, tried in turn when one fails.
    pub hosts: HostList,
    /// Queue rules and accounts specific to the site.
    pub rules: SiteRules,
    /// Ident name answered while this site connects (`None`: the global name).
    pub ident: Option<SiteIdent>,
}

/// Login and password (the password never appears in `Debug`).
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Credentials({}, ****)", self.user)
    }
}

/// Queue settings of a site replacing the engine options (`None`: the engine's).
///
/// For a transfer, the skip list and the priorities are those of the site the files come
/// from; the retries, the "existing file" rules and the resume setting those of the site
/// they go to (of the remote site for a download or an upload).
#[derive(Debug, Clone)]
pub struct SiteRules {
    pub retries: Option<u32>,
    pub retry_delay: Option<Duration>,
    pub skiplist: Option<SkipList>,
    pub priorities: Option<PriorityList>,
    pub exists: Option<ExistsRules>,
    /// Resume interrupted transfers; off: always start again from zero.
    pub resume: bool,
    /// Login for the uploads (and the folders created for them).
    pub upload_account: Option<Credentials>,
    /// Login for the downloads (and the listings of the folders downloaded).
    pub download_account: Option<Credentials>,
}

impl Default for SiteRules {
    fn default() -> Self {
        Self {
            retries: None,
            retry_delay: None,
            skiplist: None,
            priorities: None,
            exists: None,
            resume: true,
            upload_account: None,
            download_account: None,
        }
    }
}

/// Ident name of a site, registered on the running Ident server while connecting.
#[derive(Debug, Clone)]
pub struct SiteIdent {
    pub registrar: IdentRegistrar,
    pub name: String,
}

/// Which login a connection uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Account {
    Main,
    Upload,
    Download,
}

/// Other `host:port` addresses of a site ("Site IP List" of FTPRush v2).
///
/// A connection starts with the address that worked last, then moves on to the next one
/// when the server cannot be reached or refuses the login. The address that worked is
/// remembered by every copy of the profile (engine pool, browsing panel).
#[derive(Debug, Clone, Default)]
pub struct HostList {
    pub alternates: Vec<(String, u16)>,
    preferred: Arc<AtomicUsize>,
}

impl HostList {
    pub fn new(alternates: Vec<(String, u16)>) -> Self {
        Self { alternates, preferred: Arc::default() }
    }
}

impl SiteProfile {
    /// The login actually used for `account` (`Main` when the site has no such account).
    pub fn account(&self, account: Account) -> Account {
        match account {
            Account::Upload if self.rules.upload_account.is_some() => Account::Upload,
            Account::Download if self.rules.download_account.is_some() => Account::Download,
            _ => Account::Main,
        }
    }

    /// Copy of the profile logging in with `account`.
    pub fn with_account(&self, account: Account) -> SiteProfile {
        let creds = match self.account(account) {
            Account::Main => None,
            Account::Upload => self.rules.upload_account.as_ref(),
            Account::Download => self.rules.download_account.as_ref(),
        };
        let mut p = self.clone();
        if let Some(c) = creds {
            p.user = c.user.clone();
            p.password = c.password.clone();
        }
        p
    }

    /// Every address of the site, the main one first.
    pub fn addresses(&self) -> Vec<(String, u16)> {
        std::iter::once((self.host.clone(), self.port)).chain(self.hosts.alternates.iter().cloned()).collect()
    }

    /// Address that worked last (the main one at first).
    pub fn current_address(&self) -> (String, u16) {
        let all = self.addresses();
        let i = self.hosts.preferred.load(Ordering::Relaxed) % all.len();
        all[i].clone()
    }

    /// Ghost login (`!user`: the server closes the other sessions of the account) and
    /// `-password` (glftpd and others: no welcome messages). FTP only.
    pub fn with_login_variant(mut self, ghost: bool, quiet: bool) -> Self {
        if matches!(self.protocol, SiteProtocol::Ftp(_)) {
            if ghost && !self.user.starts_with('!') {
                self.user.insert(0, '!');
            }
            if quiet && !self.password.starts_with('-') {
                self.password.insert(0, '-');
            }
        }
        self
    }
}

/// Error of a session or transfer operation.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Ftp(#[from] FtpError),
    #[error(transparent)]
    Sftp(#[from] SftpError),
    #[error("local file: {0}")]
    Local(#[from] std::io::Error),
    /// The queue refers to a site the engine was not given.
    #[error("unknown site: {0}")]
    UnknownSite(String),
    /// FXP needs two FTP/FTPS servers.
    #[error("FXP is only possible between two FTP/FTPS servers")]
    FxpNeedsFtp,
    /// Every renamed variant of the target already exists.
    #[error("no free name left for {0}")]
    NoFreeName(String),
}

impl SessionError {
    /// Is the connection still usable after this error? (server refusal: yes; network
    /// error, timeout, protocol error: no.)
    pub fn keeps_session(&self) -> bool {
        matches!(
            self,
            SessionError::Ftp(FtpError::Unexpected { .. })
                | SessionError::Sftp(SftpError::Status { .. })
                | SessionError::Local(_)
                | SessionError::UnknownSite(_)
                | SessionError::FxpNeedsFtp
                | SessionError::NoFreeName(_)
        )
    }

    /// Did the server refuse to resume (`REST`)?
    pub fn is_resume_refused(&self) -> bool {
        matches!(self, SessionError::Ftp(FtpError::Unexpected { command, .. }) if command.starts_with("REST"))
    }
}

pub type Result<T, E = SessionError> = std::result::Result<T, E>;

/// Is it worth trying the next address of the site? Yes when the server cannot be reached
/// or refuses the connection or the login (full, banned...); no when the certificate or
/// the host key must be confirmed first.
fn rotates(e: &SessionError) -> bool {
    match e {
        SessionError::Ftp(FtpError::Certificate { .. }) | SessionError::Sftp(SftpError::HostKey { .. }) => false,
        SessionError::Ftp(FtpError::Unexpected { .. }) | SessionError::Sftp(SftpError::AuthFailed) => true,
        e => crate::keepalive::is_connection_lost(e) || matches!(e, SessionError::Ftp(FtpError::Proxy(_) | FtpError::Tls(_))),
    }
}

/// Open, authenticated connection to a site.
pub enum RemoteSession {
    Ftp(Box<FtpClient>),
    Sftp(Box<SftpClient>),
}

impl RemoteSession {
    /// Opens a connection and logs in, trying the other addresses of the site in turn
    /// when one cannot be reached or refuses the login.
    pub async fn open(site: &SiteProfile, log: Option<LogSink>) -> Result<Self> {
        let all = site.addresses();
        let first = site.hosts.preferred.load(Ordering::Relaxed) % all.len();
        let mut k = 0;
        loop {
            let i = (first + k) % all.len();
            let (host, port) = &all[i];
            match Self::open_at(site, host, *port, log.clone()).await {
                Ok(s) => {
                    site.hosts.preferred.store(i, Ordering::Relaxed);
                    return Ok(s);
                }
                Err(e) if k + 1 < all.len() && rotates(&e) => {
                    let (next, next_port) = &all[(i + 1) % all.len()];
                    if let Some(l) = &log {
                        l(LogLine::Status(format!("{host}:{port} failed ({e}): trying {next}:{next_port}")));
                    }
                    k += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Connects to the site (every address in turn), logs in and out: availability test.
    /// Returns the address that answered and the time taken.
    pub async fn probe(site: &SiteProfile) -> Result<((String, u16), Duration)> {
        let t0 = Instant::now();
        let session = Self::open(site, None).await?;
        let took = t0.elapsed();
        session.quit().await;
        Ok((site.current_address(), took))
    }

    /// Logs out politely (`QUIT`, SSH disconnect); errors are ignored.
    pub async fn quit(self) {
        match self {
            RemoteSession::Ftp(c) => {
                let _ = (*c).quit().await;
            }
            RemoteSession::Sftp(c) => {
                let _ = (*c).quit().await;
            }
        }
    }

    async fn open_at(site: &SiteProfile, host: &str, port: u16, log: Option<LogSink>) -> Result<Self> {
        // The FTP server queries the Ident server as soon as TCP is open: the name of the
        // site is registered for its addresses before connecting, until the login is done.
        let mut _ident = Vec::new();
        if let (Some(id), SiteProtocol::Ftp(_)) = (&site.ident, &site.protocol) {
            let addrs: Vec<std::net::SocketAddr> = match host.trim_matches(['[', ']']).parse::<std::net::IpAddr>() {
                Ok(ip) => vec![std::net::SocketAddr::new(ip, port)],
                Err(_) => tokio::net::lookup_host((host, port)).await.map(Iterator::collect).unwrap_or_default(),
            };
            _ident = addrs.iter().map(|a| id.registrar.register(a.ip(), port, &id.name)).collect();
        }
        Ok(match &site.protocol {
            SiteProtocol::Ftp(cfg) => {
                let mut c = FtpClient::connect_with_log(host, port, cfg.clone(), log).await?;
                c.login(&site.user, &site.password).await?;
                RemoteSession::Ftp(Box::new(c))
            }
            SiteProtocol::Sftp { config, key } => {
                let auth = key.clone().unwrap_or_else(|| SftpAuth::Password(site.password.clone()));
                let c = SftpClient::connect(host, port, &site.user, &auth, config.clone(), log).await?;
                RemoteSession::Sftp(Box::new(c))
            }
        })
    }

    /// Anti-idle command: an FTP command (`NOOP`...), or a check of the current folder
    /// in SFTP (SSH has its own keep alive). A `421` reply means the server is closing
    /// the connection; any other reply proves it is alive.
    pub async fn keep_alive(&mut self, command: &str) -> Result<()> {
        match self {
            RemoteSession::Ftp(c) => {
                let reply: Reply = c.raw(command).await?;
                if reply.code == 421 {
                    return Err(FtpError::Unexpected { command: command.to_string(), reply }.into());
                }
            }
            RemoteSession::Sftp(c) => {
                let dir = c.current_dir().to_string();
                c.stat(&dir).await?;
            }
        }
        Ok(())
    }

    pub fn as_ftp(&mut self) -> Option<&mut FtpClient> {
        match self {
            RemoteSession::Ftp(c) => Some(c),
            RemoteSession::Sftp(_) => None,
        }
    }

    /// Size of a file, `None` when it does not exist.
    pub async fn size_if_exists(&mut self, p: &str) -> Result<Option<u64>> {
        match self {
            RemoteSession::Ftp(c) => match c.size(p).await {
                Ok(n) => Ok(Some(n)),
                Err(FtpError::Unexpected { reply, .. }) if reply.code == 550 || reply.code == 450 => Ok(None),
                // Missing parent folder: CWD fails, so the target does not exist.
                Err(FtpError::Unexpected { command, .. }) if command.starts_with("CWD") => Ok(None),
                Err(e) => Err(e.into()),
            },
            RemoteSession::Sftp(c) => match c.size(p).await {
                Ok(n) => Ok(Some(n)),
                Err(SftpError::Status { code: 2, .. }) => Ok(None),
                Err(e) => Err(e.into()),
            },
        }
    }

    pub async fn mkdir(&mut self, p: &str) -> Result<()> {
        match self {
            RemoteSession::Ftp(c) => c.mkdir(p).await?,
            RemoteSession::Sftp(c) => c.mkdir(p).await?,
        }
        Ok(())
    }

    pub async fn delete_file(&mut self, p: &str) -> Result<()> {
        match self {
            RemoteSession::Ftp(c) => c.delete(p).await?,
            RemoteSession::Sftp(c) => c.delete(p).await?,
        }
        Ok(())
    }

    /// Removes an empty folder.
    pub async fn rmdir(&mut self, p: &str) -> Result<()> {
        match self {
            RemoteSession::Ftp(c) => c.rmdir(p).await?,
            RemoteSession::Sftp(c) => c.rmdir(p).await?,
        }
        Ok(())
    }

    /// Removes a folder and all its contents.
    pub async fn remove_tree(&mut self, dir: &str) -> Result<usize> {
        let mut removed = 0;
        let mut stack = vec![(dir.to_string(), false)];
        // Depth-first walk: a folder is removed only after its contents.
        while let Some((d, listed)) = stack.pop() {
            if listed {
                self.rmdir(&d).await?;
                removed += 1;
                continue;
            }
            stack.push((d.clone(), true));
            for e in self.list(&d).await? {
                let child = path::join(&d, &e.name);
                if e.is_dir() {
                    stack.push((child, false));
                } else {
                    self.delete_file(&child).await?;
                    removed += 1;
                }
            }
        }
        Ok(removed)
    }

    /// Renames `from` to `to` (plain name in the same folder, or a path).
    pub async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        match self {
            RemoteSession::Ftp(c) => c.rename(from, to).await?,
            RemoteSession::Sftp(c) => c.rename(from, to).await?,
        }
        Ok(())
    }

    /// Changes directory (absolute or relative path, `..` allowed).
    pub async fn cwd(&mut self, dir: &str) -> Result<()> {
        match self {
            RemoteSession::Ftp(c) => c.cwd(dir).await?,
            RemoteSession::Sftp(c) => c.cwd(dir).await?,
        }
        Ok(())
    }

    /// Known current directory.
    pub fn current_dir(&self) -> Option<String> {
        match self {
            RemoteSession::Ftp(c) => c.current_dir().map(String::from),
            RemoteSession::Sftp(c) => Some(c.current_dir().to_string()),
        }
    }

    /// Lists the current directory.
    pub async fn list_current(&mut self) -> Result<Vec<RemoteEntry>> {
        Ok(match self {
            RemoteSession::Ftp(c) => c.list(None).await?,
            RemoteSession::Sftp(c) => c.list(None).await?,
        })
    }

    pub async fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>> {
        Ok(match self {
            RemoteSession::Ftp(c) => c.list(Some(dir)).await?,
            RemoteSession::Sftp(c) => c.list(Some(dir)).await?,
        })
    }

    /// Creates the folder when missing (the parents must exist).
    pub async fn ensure_dir(&mut self, dir: &str) -> Result<()> {
        match self {
            RemoteSession::Ftp(c) => {
                if c.cwd(dir).await.is_ok() {
                    return Ok(());
                }
                c.mkdir(dir).await?;
            }
            RemoteSession::Sftp(c) => {
                if c.stat(dir).await.is_ok_and(|e| e.is_dir()) {
                    return Ok(());
                }
                c.mkdir(dir).await?;
            }
        }
        Ok(())
    }

    pub async fn retr<W>(&mut self, p: &str, offset: u64, out: &mut W, progress: impl FnMut(u64)) -> Result<u64>
    where
        W: tokio::io::AsyncWrite + Unpin + ?Sized,
    {
        Ok(match self {
            RemoteSession::Ftp(c) => c.retr(p, offset, out, progress).await?,
            RemoteSession::Sftp(c) => c.retr(p, offset, out, progress).await?,
        })
    }

    pub async fn stor<R>(&mut self, p: &str, offset: u64, input: &mut R, progress: impl FnMut(u64)) -> Result<u64>
    where
        R: tokio::io::AsyncRead + Unpin + ?Sized,
    {
        Ok(match self {
            RemoteSession::Ftp(c) => c.stor(p, offset, input, progress).await?,
            RemoteSession::Sftp(c) => c.stor(p, offset, input, progress).await?,
        })
    }

    /// Free name in `dir` based on `name` ("name (1).ext"...).
    pub async fn free_name(&mut self, dir: &str, name: &str) -> Result<String> {
        for n in 1..1000 {
            let candidate = urftp_core::exists::renamed(name, n);
            if self.size_if_exists(&path::join(dir, &candidate)).await?.is_none() {
                return Ok(candidate);
            }
        }
        Err(SessionError::NoFreeName(name.to_string()))
    }
}
