//! Version information of `rushmirc.dll`.

#[path = "../../build/winres.rs"]
mod winres;

fn main() {
    winres::embed(&winres::Resources {
        file_name: "rushmirc.dll",
        description: "UltraRushFTP link for mIRC",
        icon: None,
        manifest: false,
        dll: true,
    });
}
