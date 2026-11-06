//! Bridge between the interface (egui thread) and a browsing connection.
//!
//! The connection itself (commands, keep alive, reconnection) is
//! [`urftp_proto::browse`]; this module only hands its events to the interface thread
//! and wakes the window up. The interface never blocks on the network.

use std::sync::mpsc;

use eframe::egui;
use tokio::runtime::Handle;
use urftp_proto::browse::{self, BrowseHandle};

pub use urftp_proto::browse::{BrowseCmd as BrowserCmd, BrowseEvent as BrowserEvent, TrustKind};

pub struct BrowserHandle {
    inner: BrowseHandle,
    pub rx: mpsc::Receiver<BrowserEvent>,
}

impl BrowserHandle {
    pub fn send(&self, cmd: BrowserCmd) {
        self.inner.send(cmd);
    }
}

pub fn spawn(rt: &Handle, ctx: egui::Context) -> BrowserHandle {
    let (tx, rx) = mpsc::channel();
    // The sender is shared by the network threads; `mpsc::Sender` is `Send` + `Sync`.
    let inner = browse::spawn(rt, move |e| {
        let _ = tx.send(e);
        ctx.request_repaint();
    });
    BrowserHandle { inner, rx }
}
