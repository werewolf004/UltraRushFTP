//! Session distante commune (FTP/FTPS ou SFTP) et profil de site, pour le moteur de file.

use urftp_core::queue::SiteLimits;

use crate::ftp::{FtpClient, FtpConfig, FtpError, LogSink, RemoteEntry, path};
use crate::sftp::{SftpAuth, SftpClient, SftpConfig, SftpError};

/// Protocole et réglages d'un site.
#[derive(Debug, Clone)]
pub enum SiteProtocol {
    /// FTP ou FTPS (voir `FtpConfig::security`).
    Ftp(FtpConfig),
    /// SFTP ; `key` remplace le mot de passe si présente.
    Sftp { config: SftpConfig, key: Option<SftpAuth> },
}

/// Tout ce qu'il faut pour ouvrir des connexions vers un site.
#[derive(Debug, Clone)]
pub struct SiteProfile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub protocol: SiteProtocol,
    pub limits: SiteLimits,
}

/// Erreur d'une opération de session ou de transfert.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Ftp(#[from] FtpError),
    #[error(transparent)]
    Sftp(#[from] SftpError),
    #[error("fichier local : {0}")]
    Local(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl SessionError {
    /// La connexion reste-t-elle utilisable après cette erreur ? (refus du serveur : oui ;
    /// erreur réseau, délai, protocole : non.)
    pub fn keeps_session(&self) -> bool {
        matches!(
            self,
            SessionError::Ftp(FtpError::Unexpected { .. })
                | SessionError::Sftp(SftpError::Status { .. })
                | SessionError::Local(_)
                | SessionError::Other(_)
        )
    }

    /// Le serveur a-t-il refusé la reprise (`REST`) ?
    pub fn is_resume_refused(&self) -> bool {
        matches!(self, SessionError::Ftp(FtpError::Unexpected { command, .. }) if command.starts_with("REST"))
    }
}

pub type Result<T, E = SessionError> = std::result::Result<T, E>;

/// Connexion ouverte et authentifiée vers un site.
pub enum RemoteSession {
    Ftp(Box<FtpClient>),
    Sftp(Box<SftpClient>),
}

impl RemoteSession {
    /// Ouvre une connexion et s'authentifie.
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

    pub fn as_ftp(&mut self) -> Option<&mut FtpClient> {
        match self {
            RemoteSession::Ftp(c) => Some(c),
            RemoteSession::Sftp(_) => None,
        }
    }

    /// Taille d'un fichier, `None` s'il n'existe pas.
    pub async fn size_if_exists(&mut self, p: &str) -> Result<Option<u64>> {
        match self {
            RemoteSession::Ftp(c) => match c.size(p).await {
                Ok(n) => Ok(Some(n)),
                Err(FtpError::Unexpected { reply, .. }) if reply.code == 550 || reply.code == 450 => Ok(None),
                // Répertoire parent absent : le CWD échoue, la cible n'existe donc pas.
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

    pub async fn list(&mut self, dir: &str) -> Result<Vec<RemoteEntry>> {
        Ok(match self {
            RemoteSession::Ftp(c) => c.list(Some(dir)).await?,
            RemoteSession::Sftp(c) => c.list(Some(dir)).await?,
        })
    }

    /// Crée le dossier s'il n'existe pas (les parents doivent exister).
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

    /// Nom libre dans `dir` à partir de `name` (« nom (1).ext »…).
    pub async fn free_name(&mut self, dir: &str, name: &str) -> Result<String> {
        for n in 1..1000 {
            let candidate = urftp_core::exists::renamed(name, n);
            if self.size_if_exists(&path::join(dir, &candidate)).await?.is_none() {
                return Ok(candidate);
            }
        }
        Err(SessionError::Other(format!("aucun nom libre pour {name}")))
    }
}
