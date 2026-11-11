//! Windows resources of `UltraRushFTP.exe`: icon, version information and manifest.

#[path = "../../build/winres.rs"]
mod winres;

fn main() {
    winres::embed(&winres::Resources {
        file_name: "UltraRushFTP.exe",
        description: "UltraRushFTP",
        icon: Some(std::path::Path::new("assets/UltraRushFTP.ico")),
        manifest: true,
        dll: false,
    });
}
