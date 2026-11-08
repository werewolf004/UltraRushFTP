// Development tool run as an ignored test: a failed unwrap/expect stops the capture.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Documentation screenshots drawn off screen: the real application, started like
//! `main.rs` does, rendered by wgpu into a PNG file. No window is opened, so nothing comes
//! to the foreground. Driven by `scripts/screenshots.ps1`:
//!
//! | Variable | Meaning |
//! |---|---|
//! | `URUSHFTP_SHOT_OUT` | PNG file to write (the test does nothing without it) |
//! | `URUSHFTP_SHOT_ARGS` | command-line arguments of the application, one per line |
//! | `URUSHFTP_SHOT_WAIT` | seconds to let the application run before the capture (default 2) |
//! | `URUSHFTP_SHOT_HOME` | starting folder of the local panels (required: never the real user folder) |
//!
//! The other `URUSHFTP_*` variables (language, theme, windows to open) work as for the
//! application itself.

use std::time::{Duration, Instant};

use egui_kittest::Harness;

use super::UltraRushApp;
use crate::cmdline;

/// Inner size of the main window (as in `main.rs`).
const WINDOW: [f32; 2] = [1320.0, 820.0];

#[test]
#[ignore = "documentation screenshots: run by scripts/screenshots.ps1"]
fn render_screenshot() {
    let Some(out) = std::env::var_os("URUSHFTP_SHOT_OUT") else { return };
    let home = std::env::var_os("URUSHFTP_SHOT_HOME").expect("URUSHFTP_SHOT_HOME: demo folder of the local panels");
    super::ui_tests::HOME.set(std::path::PathBuf::from(home)).expect("starting folder set once");
    let args: Vec<String> = std::env::var("URUSHFTP_SHOT_ARGS")
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let wait: f64 = std::env::var("URUSHFTP_SHOT_WAIT").ok().and_then(|v| v.parse().ok()).unwrap_or(2.0);

    let startup = cmdline::parse(args);
    if let Some(d) = &startup.data_dir
        && let Some(dir) = cmdline::resolve_data_dir(d)
    {
        urftp_core::sitestore::set_data_dir(dir);
    }
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let mut app = UltraRushApp::new(rt.handle().clone(), startup);
    // No mIRC link window: the screenshots must not disturb a running FTPRush.
    app.ipc_enabled = false;
    let mut harness = Harness::builder().with_size(WINDOW).wgpu().build_eframe(move |cc| {
        crate::theme::install_fonts(&cc.egui_ctx);
        crate::theme::apply(&cc.egui_ctx, app.is_dark());
        app
    });
    // Real time: connections, listings and transfers come from the demo server.
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < wait {
        harness.step();
        std::thread::sleep(Duration::from_millis(30));
    }
    harness.run_steps(3);
    let image = harness.render().expect("off-screen rendering (wgpu)");
    image.save(&out).expect("write the screenshot");
}
