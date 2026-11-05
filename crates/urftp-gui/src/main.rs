//! Interface graphique d'UltraRushFTP (egui + egui_dock).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod browser;
mod sites;
mod theme;

use eframe::egui;

fn main() -> eframe::Result {
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
            theme::apply(&cc.egui_ctx, true);
            Ok(Box::new(app::UltraRushApp::new(handle)))
        }),
    )
}
