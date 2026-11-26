//! Writes `assets/UltraRushFTP.ico` (executables, installer, notification area) from
//! the drawing of `src/icon/art.rs`:
//! `cargo run -p urftp-gui --example write_icon`.

// A developer tool: its messages go to the console.
#![allow(clippy::print_stdout, clippy::print_stderr)]

#[path = "../src/icon/art.rs"]
#[allow(dead_code)]
mod art;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ico = crate_dir.join("assets").join("UltraRushFTP.ico");
    match std::fs::write(&ico, art::ico(&art::ICO_SIZES)) {
        Ok(()) => {
            println!("{}", ico.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", ico.display());
            ExitCode::FAILURE
        }
    }
}
