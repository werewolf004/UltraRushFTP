//! Graphical interface of UltraRushFTP (egui + egui_dock).
// Released builds have no console; the test binary keeps one (its output is read).
#![cfg_attr(all(not(debug_assertions), not(test)), windows_subsystem = "windows")]

mod app;
mod browser;
mod cmdline;
mod options;
mod sites;
mod theme;

use eframe::egui;

/// Serialises the tests that change the interface language (a process-wide setting).
#[cfg(test)]
static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Fatal startup failure.
#[derive(Debug, thiserror::Error)]
enum StartupError {
    #[error("cannot start the network runtime: {0}")]
    Runtime(#[source] std::io::Error),
    #[error("cannot open the window: {0}")]
    Ui(#[from] eframe::Error),
}

fn main() -> Result<(), StartupError> {
    let result = run();
    if let Err(e) = &result {
        // Released builds have no console: tell the user in a dialog.
        rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("UltraRushFTP").set_description(e.to_string()).show();
    }
    result
}

fn run() -> Result<(), StartupError> {
    let startup = cmdline::parse(std::env::args().skip(1));
    // `-f`: settings folder, to set before anything is loaded.
    if let Some(d) = &startup.data_dir
        && let Some(dir) = cmdline::resolve_data_dir(d)
    {
        urftp_core::sitestore::set_data_dir(dir);
    }
    // The network runs on a separate tokio runtime; the interface never blocks.
    let runtime =
        tokio::runtime::Builder::new_multi_thread().enable_all().thread_name("urftp-net").build().map_err(StartupError::Runtime)?;
    let handle = runtime.handle().clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("UltraRushFTP")
            .with_inner_size([1320.0, 820.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "UltraRushFTP",
        options,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx);
            let app = app::UltraRushApp::new(handle, startup);
            theme::apply(&cc.egui_ctx, app.is_dark());
            Ok(Box::new(app))
        }),
    )?;
    Ok(())
}
