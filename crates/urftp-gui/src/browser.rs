//! Pont entre l'interface (thread egui) et une connexion de navigation (tâche tokio).
//!
//! Chaque panneau distant possède son « acteur » : il reçoit des commandes (connexion,
//! changement de dossier…) et renvoie des événements (listing, journal, erreurs). L'interface
//! ne bloque jamais sur le réseau.

use std::sync::Arc;
use std::sync::mpsc;

use eframe::egui;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use urftp_proto::ftp::{FtpError, LogLine, LogSink, RemoteEntry};
use urftp_proto::session::{RemoteSession, SessionError, SiteProfile};
use urftp_proto::sftp::SftpError;

pub enum BrowserCmd {
    Connect(Box<SiteProfile>),
    Cwd(String),
    Refresh,
    Disconnect,
}

/// Type de confiance à accorder après un refus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustKind {
    TlsCertificate,
    SshHostKey,
}

pub enum BrowserEvent {
    Listing { dir: String, entries: Vec<RemoteEntry> },
    Log(LogLine),
    Error(String),
    /// Certificat TLS ou clé SSH inconnus : l'utilisateur peut les approuver.
    Trust { kind: TrustKind, fingerprint: String, reason: String },
    Disconnected,
}

pub struct BrowserHandle {
    tx: UnboundedSender<BrowserCmd>,
    pub rx: mpsc::Receiver<BrowserEvent>,
}

impl BrowserHandle {
    pub fn send(&self, cmd: BrowserCmd) {
        let _ = self.tx.send(cmd);
    }
}

pub fn spawn(rt: &Handle, ctx: egui::Context) -> BrowserHandle {
    let (tx, mut cmd_rx) = unbounded_channel::<BrowserCmd>();
    let (ev_tx, rx) = mpsc::channel::<BrowserEvent>();
    rt.spawn(async move {
        let send = |e: BrowserEvent| {
            let _ = ev_tx.send(e);
            ctx.request_repaint();
        };
        let mut session: Option<RemoteSession> = None;
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                BrowserCmd::Connect(profile) => {
                    session = None;
                    let (log_tx, log_ctx) = (ev_tx.clone(), ctx.clone());
                    let log: LogSink = Arc::new(move |l| {
                        let _ = log_tx.send(BrowserEvent::Log(l));
                        log_ctx.request_repaint();
                    });
                    match RemoteSession::open(&profile, Some(log)).await {
                        Ok(s) => {
                            session = Some(s);
                            listing(session.as_mut().unwrap(), &send).await;
                        }
                        Err(e) => send(error_event(e)),
                    }
                }
                BrowserCmd::Cwd(dir) => match session.as_mut() {
                    Some(s) => match s.cwd(&dir).await {
                        Ok(()) => listing(s, &send).await,
                        Err(e) => send(BrowserEvent::Error(e.to_string())),
                    },
                    None => send(BrowserEvent::Error("non connecté".into())),
                },
                BrowserCmd::Refresh => {
                    if let Some(s) = session.as_mut() {
                        listing(s, &send).await;
                    }
                }
                BrowserCmd::Disconnect => {
                    session = None;
                    send(BrowserEvent::Disconnected);
                }
            }
        }
    });
    BrowserHandle { tx, rx }
}

async fn listing(s: &mut RemoteSession, send: &impl Fn(BrowserEvent)) {
    match s.list_current().await {
        Ok(mut entries) => {
            entries.sort_by(|a, b| b.is_dir().cmp(&a.is_dir()).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
            send(BrowserEvent::Listing { dir: s.current_dir().unwrap_or_else(|| "/".into()), entries });
        }
        Err(e) => send(BrowserEvent::Error(e.to_string())),
    }
}

fn error_event(e: SessionError) -> BrowserEvent {
    match e {
        SessionError::Ftp(FtpError::Certificate { fingerprint, reason }) => {
            BrowserEvent::Trust { kind: TrustKind::TlsCertificate, fingerprint, reason }
        }
        SessionError::Sftp(SftpError::HostKey { fingerprint, reason }) => {
            BrowserEvent::Trust { kind: TrustKind::SshHostKey, fingerprint, reason }
        }
        other => BrowserEvent::Error(other.to_string()),
    }
}
