//! Windows resources of `urftp.exe`: icon, version information and manifest.

#[path = "../../build/winres.rs"]
mod winres;

fn main() {
    winres::embed(&winres::Resources {
        file_name: "urftp.exe",
        description: "UltraRushFTP command line",
        icon: Some(std::path::Path::new("../urftp-gui/assets/UltraRushFTP.ico")),
        manifest: true,
        dll: false,
    });
}
