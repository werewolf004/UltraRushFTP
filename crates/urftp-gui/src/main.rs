//! Interface graphique d'UltraRushFTP (egui + egui_dock).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod browser;
mod cmdline;
mod options;
mod sites;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
    let startup = cmdline::parse(std::env::args().skip(1));
    // `-f` : dossier des réglages, à fixer avant tout chargement et avant le premier thread.
    if let Some(d) = &startup.data_dir
        && let Some(dir) = cmdline::resolve_data_dir(d)
    {
        // SAFETY : aucun autre thread n'existe encore.
        unsafe { std::env::set_var("URUSHFTP_DATA", dir) };
    }
    // Le réseau tourne sur un runtime tokio à part ; l'interface ne bloque jamais.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("urftp-net")
        .build()
        .expect("démarrage du runtime réseau");
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
    )
}
