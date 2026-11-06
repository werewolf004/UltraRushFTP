//! Common remote session (FTP/FTPS or SFTP) and site profile, for the queue engine.

use urftp_core::queue::SiteLimits;

use crate::ftp::{FtpClient, FtpConfig, FtpError, LogSink, RemoteEntry, Reply, path};
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

/// Open, authenticated connection to a site.
pub enum RemoteSession {
    Ftp(Box<FtpClient>),
    Sftp(Box<SftpClient>),
}

impl RemoteSession {
    /// Opens a connection and logs in.
    pub async fn open(site: &SiteProfile, log: Option<LogSink>) -> Result<Self> {
        Ok(match &site.protocol {
            SiteProtocol::Ftp(cfg) => {
                let mut c = FtpClient::connect_with_log(&site.host, site.port, cfg.clone(), log).await?;
                c.login(&site.user, &site.password).await?;
                RemoteSession::Ftp(Box::new(c))
            }
            SiteProtocol::Sftp { config, key } => {
                let auth = key.clone().unwrap_or_else(|| SftpAuth::Password(site.password.clone()));
                let c = SftpClient::connect(&site.host, site.port, &site.user, &auth, config.clone(), log).await?;
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
