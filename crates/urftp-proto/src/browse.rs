//! Browsing connection of a remote panel: an actor on the network runtime that receives
//! commands (connect, change folder...) and reports events (listing, log, errors).
//!
//! While idle it sends the anti-idle commands of the site ([`crate::keepalive`]); when
//! the connection is lost, or a server reply matches the reconnection masks, it connects
//! again and goes back to the folder it was in.

use std::sync::Arc;
use std::time::Instant;

use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::ftp::{FtpError, LogLine, LogSink, RemoteEntry};
use crate::keepalive::{KeepAliveState, KeepAliveStep, is_connection_lost};
use crate::session::{RemoteSession, SessionError, SiteProfile};
use crate::sftp::SftpError;

/// Command for the browsing connection.
#[derive(Debug)]
pub enum BrowseCmd {
    Connect(Box<SiteProfile>),
    Cwd(String),
    Refresh,
    Disconnect,
    /// Creates a folder (absolute path), then refreshes.
    Mkdir(String),
    /// Deletes items (absolute path, folder?), then refreshes.
    Delete(Vec<(String, bool)>),
    /// Renames (absolute path, new name), then refreshes.
    Rename(String, String),
    /// Raw FTP command (`RushApp.FTP.RAW`); the reply arrives as [`BrowseEvent::RawReply`].
    Raw(String),
}

/// Kind of trust to grant after a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustKind {
    TlsCertificate,
    SshHostKey,
}

/// Event reported by the browsing connection.
#[derive(Debug, Clone, PartialEq)]
pub enum BrowseEvent {
    Listing {
        dir: String,
        entries: Vec<RemoteEntry>,
    },
    Log(LogLine),
    Error(String),
    /// A command needs a connection and there is none.
    NotConnected,
    /// Unknown TLS certificate or SSH key: the user may trust it.
    Trust {
        kind: TrustKind,
        fingerprint: String,
        reason: String,
    },
    Disconnected,
    /// Reply to a raw command.
    RawReply {
        command: String,
        lines: Vec<String>,
        ok: bool,
    },
    /// Raw commands need an FTP connection.
    RawImpossible(String),
    /// The connection was lost (`reason`); attempt `attempt` of `of` to connect again.
    Reconnecting {
        attempt: u32,
        of: u32,
        reason: String,
    },
    /// Connected again after a loss (the listing follows).
    Reconnected,
    /// Every reconnection attempt failed (`Disconnected` follows).
    ReconnectFailed(String),
    /// Idle for too long: the anti-idle commands stop (`logged_out`: disconnected too).
    KeepAliveStopped {
        logged_out: bool,
    },
}

/// Sender of commands to a browsing connection; dropping it stops the actor.
pub struct BrowseHandle {
    tx: UnboundedSender<BrowseCmd>,
}

impl BrowseHandle {
    pub fn send(&self, cmd: BrowseCmd) {
        let _ = self.tx.send(cmd);
    }
}

/// Starts a browsing connection on `rt`; `on_event` receives its events (from the
/// network threads).
pub fn spawn(rt: &Handle, on_event: impl Fn(BrowseEvent) + Send + Sync + 'static) -> BrowseHandle {
    let (tx, rx) = unbounded_channel();
    let emit: Arc<dyn Fn(BrowseEvent) + Send + Sync> = Arc::new(on_event);
    rt.spawn(Actor { session: None, profile: None, keep: None, last_sent: Instant::now(), dir: None, emit }.run(rx));
    BrowseHandle { tx }
}

struct Actor {
    session: Option<RemoteSession>,
    profile: Option<Box<SiteProfile>>,
    keep: Option<KeepAliveState>,
    /// Last command sent by the keep alive.
    last_sent: Instant,
    /// Folder to go back to after a reconnection.
    dir: Option<String>,
    emit: Arc<dyn Fn(BrowseEvent) + Send + Sync>,
}

impl Actor {
    async fn run(mut self, mut rx: UnboundedReceiver<BrowseCmd>) {
        loop {
            let deadline = if self.session.is_some() { self.keep.as_ref().and_then(|k| k.deadline(self.last_sent)) } else { None };
            let cmd = match deadline {
                Some(at) => tokio::select! {
                    cmd = rx.recv() => cmd,
                    () = tokio::time::sleep_until(tokio::time::Instant::from_std(at)) => {
                        self.keep_alive().await;
                        continue;
                    }
                },
                None => rx.recv().await,
            };
            let Some(cmd) = cmd else { break };
            if let Some(k) = &mut self.keep {
                k.touch(Instant::now());
            }
            self.handle(cmd).await;
        }
    }

    fn emit(&self, e: BrowseEvent) {
        (self.emit)(e);
    }

    fn log_sink(&self) -> LogSink {
        let emit = self.emit.clone();
        Arc::new(move |l| emit(BrowseEvent::Log(l)))
    }

    async fn handle(&mut self, cmd: BrowseCmd) {
        match cmd {
            BrowseCmd::Connect(profile) => {
                self.session = None;
                self.dir = None;
                self.keep = profile.session.keep_alive.clone().map(|k| KeepAliveState::new(k, Instant::now()));
                self.last_sent = Instant::now();
                let log = self.log_sink();
                let opened = RemoteSession::open(&profile, Some(log)).await;
                self.profile = Some(profile);
                match opened {
                    Ok(s) => {
                        self.session = Some(s);
                        self.list().await;
                    }
                    Err(e) => self.emit(error_event(e)),
                }
            }
            BrowseCmd::Disconnect => {
                self.profile = None;
                if let Some(RemoteSession::Ftp(c)) = self.session.take() {
                    let _ = c.quit().await;
                }
                self.emit(BrowseEvent::Disconnected);
            }
            BrowseCmd::Refresh => {
                if self.session.is_some() {
                    self.list().await;
                }
            }
            BrowseCmd::Cwd(dir) => {
                let Some(s) = self.session.as_mut() else { return self.emit(BrowseEvent::NotConnected) };
                match s.cwd(&dir).await {
                    Ok(()) => self.list().await,
                    Err(e) => self.failed(e).await,
                }
            }
            BrowseCmd::Mkdir(p) => {
                let Some(s) = self.session.as_mut() else { return };
                match s.mkdir(&p).await {
                    Ok(()) => self.list().await,
                    Err(e) => self.failed(e).await,
                }
            }
            BrowseCmd::Delete(items) => {
                let Some(s) = self.session.as_mut() else { return };
                let mut lost = None;
                for (p, is_dir) in items {
                    let r = if is_dir { s.remove_tree(&p).await.map(drop) } else { s.delete_file(&p).await };
                    if let Err(e) = r {
                        if is_connection_lost(&e) {
                            lost = Some(e);
                            break;
                        }
                        (self.emit)(BrowseEvent::Error(e.to_string()));
                    }
                }
                match lost {
                    Some(e) => self.failed(e).await,
                    None => self.list().await,
                }
            }
            BrowseCmd::Rename(from, to) => {
                let Some(s) = self.session.as_mut() else { return };
                match s.rename(&from, &to).await {
                    Ok(()) => self.list().await,
                    Err(e) => self.failed(e).await,
                }
            }
            BrowseCmd::Raw(command) => match self.session.as_mut().and_then(RemoteSession::as_ftp) {
                Some(c) => match c.raw(&command).await {
                    Ok(r) => self.emit(BrowseEvent::RawReply { command, ok: r.code < 400, lines: r.lines }),
                    Err(e) => {
                        self.emit(BrowseEvent::RawReply { command, ok: false, lines: vec![e.to_string()] });
                        self.failed(e.into()).await;
                    }
                },
                None => self.emit(BrowseEvent::RawImpossible(command)),
            },
        }
    }

    /// Lists the current folder; a failure goes through [`Actor::failed`].
    async fn list(&mut self) {
        if let Err(e) = self.try_list().await {
            self.failed(e).await;
        }
    }

    async fn try_list(&mut self) -> Result<(), SessionError> {
        let Some(s) = self.session.as_mut() else { return Ok(()) };
        let mut entries = s.list_current().await?;
        entries.sort_by(|a, b| b.is_dir().cmp(&a.is_dir()).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        let dir = s.current_dir().unwrap_or_else(|| "/".into());
        self.dir = Some(dir.clone());
        self.emit(BrowseEvent::Listing { dir, entries });
        Ok(())
    }

    /// An operation failed: reconnect when the policy asks for it, otherwise report it
    /// (and drop a connection that is gone).
    async fn failed(&mut self, e: SessionError) {
        let wanted = self.profile.as_ref().is_some_and(|p| p.session.reconnect.wanted(&e));
        if wanted {
            self.reconnect(e.to_string()).await;
            return;
        }
        let lost = is_connection_lost(&e);
        self.emit(BrowseEvent::Error(e.to_string()));
        if lost {
            self.session = None;
            self.emit(BrowseEvent::Disconnected);
        }
    }

    async fn reconnect(&mut self, reason: String) {
        self.session = None;
        let Some(profile) = self.profile.clone() else { return };
        let policy = &profile.session.reconnect;
        let of = policy.attempts.max(1);
        let mut last = reason.clone();
        for attempt in 1..=of {
            self.emit(BrowseEvent::Reconnecting { attempt, of, reason: reason.clone() });
            tokio::time::sleep(policy.delay).await;
            match RemoteSession::open(&profile, Some(self.log_sink())).await {
                Ok(mut s) => {
                    if let Some(dir) = &self.dir
                        && let Err(e) = s.cwd(dir).await
                    {
                        self.emit(BrowseEvent::Error(e.to_string()));
                    }
                    self.session = Some(s);
                    self.last_sent = Instant::now();
                    self.emit(BrowseEvent::Reconnected);
                    // No new reconnection from here: a failing listing is only reported.
                    if let Err(e) = self.try_list().await {
                        self.emit(BrowseEvent::Error(e.to_string()));
                    }
                    return;
                }
                Err(e) => last = e.to_string(),
            }
        }
        self.emit(BrowseEvent::ReconnectFailed(last));
        self.emit(BrowseEvent::Disconnected);
    }

    async fn keep_alive(&mut self) {
        let now = Instant::now();
        let Some(k) = &mut self.keep else { return };
        self.last_sent = now;
        match k.step(now) {
            KeepAliveStep::Send(cmd) => {
                let Some(s) = self.session.as_mut() else { return };
                if let Err(e) = s.keep_alive(&cmd).await {
                    self.failed(e).await;
                }
            }
            KeepAliveStep::Stop { logout } => {
                self.emit(BrowseEvent::KeepAliveStopped { logged_out: logout });
                if logout {
                    self.profile = None;
                    if let Some(RemoteSession::Ftp(c)) = self.session.take() {
                        let _ = c.quit().await;
                    }
                    self.emit(BrowseEvent::Disconnected);
                }
            }
        }
    }
}

fn error_event(e: SessionError) -> BrowseEvent {
    match e {
        SessionError::Ftp(FtpError::Certificate { fingerprint, reason }) => {
            BrowseEvent::Trust { kind: TrustKind::TlsCertificate, fingerprint, reason }
        }
        SessionError::Sftp(SftpError::HostKey { fingerprint, reason }) => {
            BrowseEvent::Trust { kind: TrustKind::SshHostKey, fingerprint, reason }
        }
        other => BrowseEvent::Error(other.to_string()),
    }
}
