// Windows resources shared by the build scripts of the binaries: version information,
// and for the programs an icon and an application manifest. Included with
// `#[path = "../../build/winres.rs"] mod winres;` from each `build.rs`.
//
// The `.rc` file is written to OUT_DIR with the version of the workspace, then compiled
// and linked by `embed-resource` (rc.exe of the Windows SDK). Nothing is done when the
// target is not Windows.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// What one binary receives.
pub struct Resources<'a> {
    /// File name shown in the properties of the file (`OriginalFilename`).
    pub file_name: &'a str,
    /// `FileDescription`.
    pub description: &'a str,
    /// Icon of the program (a DLL has none).
    pub icon: Option<&'a Path>,
    /// Application manifest (programs only).
    pub manifest: bool,
    /// The binary is a DLL (`VFT_DLL`, resources linked into the library).
    pub dll: bool,
}

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
  <assemblyIdentity type="win32" name="UltraRushFTP" version="{VERSION}" processorArchitecture="*"/>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <asmv3:application>
    <asmv3:windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </asmv3:windowsSettings>
  </asmv3:application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

/// Writes the resources of the binary being built and links them.
pub fn embed(res: &Resources<'_>) {
    println!("cargo:rerun-if-changed=../../build/winres.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut parts = version.split(['.', '-', '+']).map(|p| p.parse::<u16>().unwrap_or(0));
    let (major, minor, patch) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    let numeric = format!("{major},{minor},{patch},0");
    let dotted = format!("{major}.{minor}.{patch}.0");

    let mut rc = String::from("#pragma code_page(65001)\n");
    if let Some(icon) = res.icon {
        let icon = &PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default()).join(icon);
        println!("cargo:rerun-if-changed={}", icon.display());
        let _ = writeln!(rc, "1 ICON \"{}\"", escape(icon));
    }
    if res.manifest {
        let file = out.join("app.manifest");
        if let Err(e) = std::fs::write(&file, MANIFEST.replace("{VERSION}", &dotted)) {
            println!("cargo:warning=manifest not written: {e}");
            return;
        }
        let _ = writeln!(rc, "1 24 \"{}\"", escape(&file));
    }
    let file_type = if res.dll { "0x2L" } else { "0x1L" };
    let _ = write!(
        rc,
        r#"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE {file_type}
FILESUBTYPE 0x0L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "CompanyName", "werewolf004"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{file_name}"
      VALUE "LegalCopyright", "Copyright (c) werewolf004. All rights reserved."
      VALUE "OriginalFilename", "{file_name}"
      VALUE "ProductName", "UltraRushFTP"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        description = res.description,
        file_name = res.file_name,
    );
    let file = out.join("resources.rc");
    if let Err(e) = std::fs::write(&file, rc) {
        println!("cargo:warning=resources not written: {e}");
        return;
    }
    let compiled = if res.dll {
        embed_resource::compile_for_cdylib(&file, embed_resource::NONE)
    } else {
        embed_resource::compile(&file, embed_resource::NONE)
    };
    if let Err(e) = compiled.manifest_required() {
        // No rc.exe: the binary still builds, without its resources.
        println!("cargo:warning=Windows resources not linked: {e}");
    }
}

/// A path in an `.rc` string: backslashes doubled.
fn escape(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}
