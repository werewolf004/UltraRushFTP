//! SFTP (SSH File Transfer Protocol) client on top of `russh` / `russh-sftp`.
//!
//! - Authentication by password or private key (OpenSSH, PEM, PuTTY `.ppk`, encrypted
//!   or not).
//! - Host key verified by SHA-256 fingerprint in the OpenSSH format (`SHA256:…`): an
//!   unknown key returns [`SftpError::HostKey`] with its fingerprint, to offer trusting it.
//! - Same interface as the FTP client for the common operations (listing as
//!   [`RemoteEntry`], upload/download with resume and progress).
//! - SOCKS/HTTP proxy through [`crate::proxy`].

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use std::time::Duration;

use russh::client::{self, AuthResult, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpLibError;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

use crate::ftp::listing::{EntryKind, FtpTime, RemoteEntry};
use crate::ftp::{LogLine, LogSink, path};
use crate::proxy::{self, ProxyConfig, ProxyError};

const CHUNK: usize = 64 * 1024;

/// Authentication method.
#[derive(Clone, PartialEq, Eq)]
pub enum SftpAuth {
    Password(String),
    /// Private key file (OpenSSH, PEM or PuTTY `.ppk`).
    KeyFile {
        path: PathBuf,
        passphrase: Option<String>,
    },
    /// Private key given as text.
    KeyText {
        text: String,
        passphrase: Option<String>,
    },
}

impl std::fmt::Debug for SftpAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SftpAuth::Password(_) => f.write_str("Password(****)"),
            SftpAuth::KeyFile { path, .. } => write!(f, "KeyFile({})", path.display()),
            SftpAuth::KeyText { .. } => f.write_str("KeyText(****)"),
        }
    }
}

/// Acceptance policy of the server key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Accepts only the listed fingerprints (`SHA256:…`). Empty list: every key is
    /// refused and the error gives its fingerprint (first connection).
    Known(Vec<String>),
    /// Accepts any key (not recommended: no protection against interception).
    AcceptAny,
}

impl Default for HostKeyPolicy {
    fn default() -> Self {
        HostKeyPolicy::Known(Vec::new())
    }
}

/// Settings of an SFTP connection.
#[derive(Debug, Clone)]
pub struct SftpConfig {
    pub connect_timeout: Duration,
    /// Maximum duration of an SFTP request.
    pub io_timeout: Duration,
    /// SSH keep-alive interval (`None`: disabled).
    pub keepalive: Option<Duration>,
    pub host_key: HostKeyPolicy,
    pub proxy: Option<ProxyConfig>,
    /// Read/write requests sent without waiting for the reply (pipelining), to keep a
    /// good throughput on high-latency links.
    pub max_in_flight: usize,
    /// Local address the connection leaves from.
    pub bind_ip: Option<std::net::IpAddr>,
}

impl Default for SftpConfig {
    fn default() -> Self {
        Self {
            max_in_flight: 16,
            connect_timeout: Duration::from_secs(30),
            io_timeout: Duration::from_secs(120),
            keepalive: Some(Duration::from_secs(30)),
            host_key: HostKeyPolicy::default(),
            proxy: None,
            bind_ip: None,
        }
    }
}

/// SFTP errors.
#[derive(Debug, thiserror::Error)]
pub enum SftpError {
    #[error("network error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timed out")]
    Timeout,
    #[error("proxy: {0}")]
    Proxy(#[from] ProxyError),
    #[error("server key not trusted ({reason}) — fingerprint: {fingerprint}")]
    HostKey { fingerprint: String, reason: String },
    #[error("authentication refused by the server")]
    AuthFailed,
    #[error("private key unreadable: {0}")]
    Key(String),
    #[error("SSH error: {0}")]
    Ssh(String),
    /// Error returned by the SFTP server (`SSH_FX_*` code).
    #[error("{operation}: {message} (code {code})")]
    Status { operation: String, code: u32, message: String },
    #[error("SFTP error: {0}")]
    Sftp(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}

impl SftpError {
    /// `SSH_FX_*` code (2 = no such file, 3 = permission denied...), when given by the server.
    pub fn status_code(&self) -> Option<u32> {
        match self {
            SftpError::Status { code, .. } => Some(*code),
            _ => None,
        }
    }
}

pub type Result<T, E = SftpError> = std::result::Result<T, E>;

fn op_err(operation: &str, p: &str) -> impl FnOnce(SftpLibError) -> SftpError {
    let operation = format!("{operation} {p}");
    move |e| match e {
        SftpLibError::Status(s) => SftpError::Status {
            operation,
            code: s.status_code as u32,
            message: if s.error_message.is_empty() { s.status_code.to_string() } else { s.error_message },
        },
        SftpLibError::Timeout => SftpError::Timeout,
        other => SftpError::Sftp(format!("{operation} : {other}")),
    }
}

/// SHA-256 fingerprint in the OpenSSH format (`SHA256:base64`).
pub fn host_key_fingerprint(key: &PublicKeyOrCertificate) -> String {
    match key {
        PublicKeyOrCertificate::PublicKey { key, .. } => key.fingerprint(HashAlg::Sha256).to_string(),
        PublicKeyOrCertificate::Certificate(cert) => cert.public_key().fingerprint(HashAlg::Sha256).to_string(),
    }
}

struct Handler {
    policy: HostKeyPolicy,
    seen: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let fp = host_key_fingerprint(key);
        *self.seen.lock() = Some(fp.clone());
        Ok(match &self.policy {
            HostKeyPolicy::AcceptAny => true,
            HostKeyPolicy::Known(list) => list.iter().any(|k| k.trim() == fp),
        })
    }
}

/// Open SFTP session.
pub struct SftpClient {
    ssh: Handle<Handler>,
    sftp: SftpSession,
    cwd: String,
    fingerprint: String,
    log: Option<LogSink>,
}

impl SftpClient {
    /// SSH connection, host key check, authentication, opening of the SFTP subsystem.
    pub async fn connect(host: &str, port: u16, user: &str, auth: &SftpAuth, config: SftpConfig, log: Option<LogSink>) -> Result<Self> {
        let status = |m: String| {
            if let Some(l) = &log {
                l(LogLine::Status(m));
            }
        };
        let ct = config.connect_timeout;
        let proxy = config.proxy.clone().filter(|p| !p.bypasses(host));
        let tcp = match &proxy {
            Some(p) => {
                status(format!("Connecting through proxy {:?} {}:{}", p.kind, p.host, p.port));
                limit(ct, async { Ok(proxy::connect(p, host, port).await?) }).await?
            }
            None => limit(ct, async { Ok(crate::net::connect_from(host, port, config.bind_ip).await?) }).await?,
        };
        tcp.set_nodelay(true)?;

        let seen = Arc::new(Mutex::new(None));
        let handler = Handler { policy: config.host_key.clone(), seen: seen.clone() };
        let ssh_config = client::Config { keepalive_interval: config.keepalive, inactivity_timeout: None, ..client::Config::default() };
        let connected = tokio::time::timeout(ct, client::connect_stream(Arc::new(ssh_config), tcp, handler)).await;
        let mut ssh = match connected {
            Err(_) => return Err(SftpError::Timeout),
            Ok(Ok(h)) => h,
            Ok(Err(e)) => {
                let fingerprint = seen.lock().clone();
                return Err(match (e, fingerprint) {
                    (russh::Error::UnknownKey, Some(fingerprint)) => {
                        SftpError::HostKey { fingerprint, reason: "unknown key or different from the trusted one".into() }
                    }
                    (e, _) => SftpError::Ssh(e.to_string()),
                });
            }
        };
        let fingerprint = seen.lock().clone().unwrap_or_default();
        status(format!("Server key: {fingerprint}"));

        let result = match auth {
            SftpAuth::Password(pass) => ssh.authenticate_password(user, pass).await,
            SftpAuth::KeyFile { .. } | SftpAuth::KeyText { .. } => {
                let key = load_key(auth)?;
                let hash = ssh.best_supported_rsa_hash().await.map_err(ssh_err)?.flatten();
                ssh.authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash)).await
            }
        }
        .map_err(ssh_err)?;
        if !matches!(result, AuthResult::Success) {
            return Err(SftpError::AuthFailed);
        }

        let channel = ssh.channel_open_session().await.map_err(ssh_err)?;
        channel.request_subsystem(true, "sftp").await.map_err(ssh_err)?;
        let sftp_config = russh_sftp::client::Config {
            max_concurrent_reads: config.max_in_flight.max(1),
            max_concurrent_writes: config.max_in_flight.max(1),
            request_timeout_secs: config.io_timeout.as_secs().max(1),
            ..russh_sftp::client::Config::default()
        };
        let sftp = SftpSession::new_with_config(channel.into_stream(), sftp_config)
            .await
            .map_err(|e| SftpError::Sftp(format!("opening the SFTP subsystem: {e}")))?;
        sftp.set_timeout(config.io_timeout.as_secs().max(1));
        let cwd = sftp.canonicalize(".").await.map_err(op_err("REALPATH", "."))?;
        status(format!("SFTP ready, directory: {cwd}"));
        Ok(Self { ssh, sftp, cwd, fingerprint, log })
    }

    pub fn set_logger(&mut self, log: Option<LogSink>) {
        self.log = log;
    }

    /// Fingerprint of the server key (`SHA256:…`).
    pub fn host_key_fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn current_dir(&self) -> &str {
        &self.cwd
    }

    fn status(&self, msg: impl Into<String>) {
        if let Some(l) = &self.log {
            l(LogLine::Status(msg.into()));
        }
    }

    /// Absolute path (relative paths start from the current directory).
    fn abs(&self, p: &str) -> String {
        path::join(&self.cwd, p)
    }

    pub async fn cwd(&mut self, dir: &str) -> Result<()> {
        let target = self.abs(dir);
        let meta = self.sftp.metadata(target.clone()).await.map_err(op_err("CWD", &target))?;
        if !meta.is_dir() {
            return Err(SftpError::InvalidArgument(format!("{target} is not a folder")));
        }
        self.cwd = target;
        Ok(())
    }

    pub async fn list(&mut self, dir: Option<&str>) -> Result<Vec<RemoteEntry>> {
        let target = dir.map_or_else(|| self.cwd.clone(), |d| self.abs(d));
        let entries = self.sftp.read_dir(target.clone()).await.map_err(op_err("LIST", &target))?;
        Ok(entries.filter(|e| e.file_name() != "." && e.file_name() != "..").map(|e| to_entry(e.file_name(), &e.metadata())).collect())
    }

    pub async fn mkdir(&mut self, p: &str) -> Result<()> {
        let t = self.abs(p);
        self.sftp.create_dir(t.clone()).await.map_err(op_err("MKDIR", &t))
    }

    pub async fn rmdir(&mut self, p: &str) -> Result<()> {
        let t = self.abs(p);
        self.sftp.remove_dir(t.clone()).await.map_err(op_err("RMDIR", &t))
    }

    pub async fn delete(&mut self, p: &str) -> Result<()> {
        let t = self.abs(p);
        self.sftp.remove_file(t.clone()).await.map_err(op_err("DELETE", &t))
    }

    /// Renames or moves. `to` may be a plain name (same folder) or a path.
    pub async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        let f = self.abs(from);
        let t = if to.contains('/') { self.abs(to) } else { path::join(path::split(&f).0.unwrap_or("/"), to) };
        self.sftp.rename(f.clone(), t).await.map_err(op_err("RENAME", &f))
    }

    pub async fn stat(&mut self, p: &str) -> Result<RemoteEntry> {
        let t = self.abs(p);
        let meta = self.sftp.metadata(t.clone()).await.map_err(op_err("STAT", &t))?;
        Ok(to_entry(path::split(&t).1.to_string(), &meta))
    }

    pub async fn size(&mut self, p: &str) -> Result<u64> {
        Ok(self.stat(p).await?.size)
    }

    /// Downloads `remote` into `out` from `offset`. Returns the bytes received.
    pub async fn retr<W>(&mut self, remote: &str, offset: u64, out: &mut W, mut progress: impl FnMut(u64)) -> Result<u64>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let t = self.abs(remote);
        let mut file = self.sftp.open(t.clone()).await.map_err(op_err("OPEN", &t))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset)).await?;
        }
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).await?;
            total += n as u64;
            progress(total);
        }
        out.flush().await?;
        file.shutdown().await?;
        Ok(total)
    }

    /// Uploads `input` to `remote`. With `offset > 0`, writes from that byte without
    /// truncating (resume); `input` must already be positioned at `offset`.
    pub async fn stor<R>(&mut self, remote: &str, offset: u64, input: &mut R, mut progress: impl FnMut(u64)) -> Result<u64>
    where
        R: AsyncRead + Unpin + ?Sized,
    {
        let t = self.abs(remote);
        let flags =
            if offset > 0 { OpenFlags::WRITE | OpenFlags::CREATE } else { OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE };
        let mut file = self.sftp.open_with_flags(t.clone(), flags).await.map_err(op_err("OPEN", &t))?;
        if offset > 0 {
            file.seek(std::io::SeekFrom::Start(offset)).await?;
        }
        let mut buf = vec![0u8; CHUNK];
        let mut total = 0u64;
        loop {
            let n = input.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).await?;
            total += n as u64;
            progress(total);
        }
        file.shutdown().await?;
        Ok(total)
    }

    /// Closes the SFTP session cleanly, then the SSH connection.
    pub async fn quit(self) -> Result<()> {
        self.status("Disconnecting");
        let _ = self.sftp.close().await;
        self.ssh.disconnect(russh::Disconnect::ByApplication, "", "en").await.map_err(ssh_err)
    }
}

fn ssh_err(e: russh::Error) -> SftpError {
    SftpError::Ssh(e.to_string())
}

async fn limit<T>(d: Duration, f: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(d, f).await.unwrap_or(Err(SftpError::Timeout))
}

fn load_key(auth: &SftpAuth) -> Result<russh::keys::PrivateKey> {
    let (text, pass) = match auth {
        SftpAuth::KeyFile { path, passphrase } => {
            (std::fs::read_to_string(path).map_err(|e| SftpError::Key(format!("{}: {e}", path.display())))?, passphrase.as_deref())
        }
        SftpAuth::KeyText { text, passphrase } => (text.clone(), passphrase.as_deref()),
        SftpAuth::Password(_) => return Err(SftpError::Key("password authentication has no key file".into())),
    };
    russh::keys::decode_secret_key(&text, pass).map_err(|e| SftpError::Key(e.to_string()))
}

fn to_entry(name: String, meta: &FileAttributes) -> RemoteEntry {
    let kind = if meta.is_dir() {
        EntryKind::Dir
    } else if meta.is_symlink() {
        EntryKind::Link
    } else {
        EntryKind::File
    };
    RemoteEntry {
        name,
        kind,
        size: meta.size.unwrap_or(0),
        modified: meta.mtime.map(|t| unix_to_ftptime(i64::from(t))),
        permissions: meta.permissions.map(mode_string),
        owner: meta.user.clone().or_else(|| meta.uid.map(|u| u.to_string())),
        group: meta.group.clone().or_else(|| meta.gid.map(|g| g.to_string())),
        link_target: None,
    }
}

/// Unix date (UTC) → [`FtpTime`] ("civil from days" algorithm by H. Hinnant).
pub fn unix_to_ftptime(secs: i64) -> FtpTime {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    FtpTime { year: Some(year), month, day, hour: (rem / 3600) as u8, minute: (rem % 3600 / 60) as u8, second: (rem % 60) as u8 }
}

/// Unix mode → `drwxr-xr-x`.
pub fn mode_string(mode: u32) -> String {
    let kind = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        0o020000 => 'c',
        0o060000 => 'b',
        0o010000 => 'p',
        0o140000 => 's',
        _ => '-',
    };
    let mut s = String::with_capacity(10);
    s.push(kind);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_time_conversion() {
        let t = unix_to_ftptime(0);
        assert_eq!((t.year, t.month, t.day, t.hour), (Some(1970), 1, 1, 0));
        let t = unix_to_ftptime(1_704_164_645); // 2024-01-02 03:04:05 UTC
        assert_eq!((t.year, t.month, t.day, t.hour, t.minute, t.second), (Some(2024), 1, 2, 3, 4, 5));
        let t = unix_to_ftptime(951_782_400); // 2000-02-29
        assert_eq!((t.year, t.month, t.day), (Some(2000), 2, 29));
    }

    #[test]
    fn modes() {
        assert_eq!(mode_string(0o040755), "drwxr-xr-x");
        assert_eq!(mode_string(0o100644), "-rw-r--r--");
        assert_eq!(mode_string(0o120777), "lrwxrwxrwx");
    }

    #[test]
    fn auth_debug_hides_secrets() {
        let s = format!("{:?}", SftpAuth::Password("secret".into()));
        assert!(!s.contains("secret"));
    }
}
