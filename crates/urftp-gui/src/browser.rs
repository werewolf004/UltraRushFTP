//! Bridge between the interface (egui thread) and a browsing connection (tokio task).
//!
//! Every remote panel has its "actor": it receives commands (connect, change folder...)
//! and sends back events (listing, log, errors). The interface never blocks on the
//! network.

use std::sync::Arc;
use std::sync::mpsc;
use urftp_i18n::t;

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
    /// Creates a folder (absolute path).
    Mkdir(String),
    /// Deletes items (absolute path, folder?), then refreshes.
    Delete(Vec<(String, bool)>),
    /// Renames (absolute path, new name), then refreshes.
    Rename(String, String),
    /// Raw FTP command (`RushApp.FTP.RAW`); the reply arrives as `RawReply`.
    Raw(String),
}

/// Kind of trust to grant after a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustKind {
    TlsCertificate,
    SshHostKey,
}

pub enum BrowserEvent {
    Listing {
        dir: String,
        entries: Vec<RemoteEntry>,
    },
    Log(LogLine),
    Error(String),
    /// Unknown TLS certificate or SSH key: the user may trust it.
    Trust {
        kind: TrustKind,
        fingerprint: String,
        reason: String,
    },
    Disconnected,
    /// Reply to a raw command: (command, reply lines).
    RawReply {
        command: String,
        lines: Vec<String>,
        ok: bool,
    },
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
                        Ok(s) => listing(session.insert(s), &send).await,
                        Err(e) => send(error_event(e)),
                    }
                }
                BrowserCmd::Cwd(dir) => match session.as_mut() {
                    Some(s) => match s.cwd(&dir).await {
                        Ok(()) => listing(s, &send).await,
                        Err(e) => send(BrowserEvent::Error(e.to_string())),
                    },
                    None => send(BrowserEvent::Error(t!("browser.not_connected"))),
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
                BrowserCmd::Mkdir(p) => {
                    if let Some(s) = session.as_mut() {
                        if let Err(e) = s.mkdir(&p).await {
                            send(BrowserEvent::Error(e.to_string()));
                        }
                        listing(s, &send).await;
                    }
                }
                BrowserCmd::Delete(items) => {
                    if let Some(s) = session.as_mut() {
                        for (p, is_dir) in items {
                            let r = if is_dir { s.remove_tree(&p).await.map(drop) } else { s.delete_file(&p).await };
                            if let Err(e) = r {
                                send(BrowserEvent::Error(e.to_string()));
                            }
                        }
                        listing(s, &send).await;
                    }
                }
                BrowserCmd::Raw(command) => match session.as_mut().and_then(RemoteSession::as_ftp) {
                    Some(c) => match c.raw(&command).await {
                        Ok(r) => send(BrowserEvent::RawReply { command, ok: r.code < 400, lines: r.lines }),
                        Err(e) => send(BrowserEvent::RawReply { command, ok: false, lines: vec![e.to_string()] }),
                    },
                    None => send(BrowserEvent::Error(t!("browser.raw_impossible", command = command))),
                },
                BrowserCmd::Rename(from, to) => {
                    if let Some(s) = session.as_mut() {
                        if let Err(e) = s.rename(&from, &to).await {
                            send(BrowserEvent::Error(e.to_string()));
                        }
                        listing(s, &send).await;
                    }
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
