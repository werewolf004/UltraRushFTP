//! FTPRush v3: `site.json` (`RootItem` → `Children` → `Server`).
//!
//! Password: `Base64Password`, plain base64 of the password.
//! FTP encryption: `FTPEnryptMode` (FluentFTP values: 0 none, 1 implicit, 2 explicit,
//! 3 auto). `Protocol` 1 = FTP; the other codes (SFTP, WebDAV, clouds...) are not yet
//! identified with certainty and produce a warning.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

use super::{ImportError, ImportFormat, ImportResult, ImportWarning};
use crate::site::{Bookmark, Site, SiteGroup, SiteProtocol};

pub(super) fn import(text: &str) -> Result<ImportResult, ImportError> {
    let v: Value = serde_json::from_str(text).map_err(|e| ImportError::Invalid(ImportFormat::FtpRush3, e.to_string()))?;
    let root = v.get("RootItem").ok_or_else(|| ImportError::Invalid(ImportFormat::FtpRush3, "missing RootItem".into()))?;
    let mut warnings = Vec::new();
    let mut group = SiteGroup::new("Import FTPRush v3");
    for child in children(root) {
        walk(child, &mut group, &mut warnings);
    }
    if group.site_count() == 0 {
        warnings.push(ImportWarning::NoSites);
    }
    Ok(ImportResult { format: ImportFormat::FtpRush3, group, warnings })
}

fn children(v: &Value) -> impl Iterator<Item = &Value> {
    v.get("Children").and_then(Value::as_array).into_iter().flatten()
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn walk(item: &Value, parent: &mut SiteGroup, warnings: &mut Vec<ImportWarning>) {
    let name = s(item, "Name").to_string();
    match item.get("Server").filter(|sv| !sv.is_null()) {
        Some(server) => parent.sites.push(site(&name, server, warnings)),
        None => {
            let mut g = SiteGroup::new(name);
            for c in children(item) {
                walk(c, &mut g, warnings);
            }
            // The v3 "Quick Connect" folder is often empty: no need to recreate it.
            if g.site_count() > 0 || !g.name.eq_ignore_ascii_case("Quick Connect") {
                parent.groups.push(g);
            }
        }
    }
}

/// `/K:/folder/` (v3 notation) → `K:\folder\`.
fn local_path(p: &str) -> String {
    let b = p.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[2] == b':' { p[1..].replace('/', "\\") } else { p.to_string() }
}

fn site(name: &str, sv: &Value, warnings: &mut Vec<ImportWarning>) -> Site {
    let n = |k: &str| sv.get(k).and_then(Value::as_i64).unwrap_or(0);
    let label = if name.is_empty() { s(sv, "Host").to_string() } else { name.to_string() };
    let protocol = match (n("Protocol"), n("FTPEnryptMode")) {
        (1, 0) => SiteProtocol::Ftp,
        (1, 1) => SiteProtocol::FtpsImplicit,
        (1, _) => SiteProtocol::FtpsExplicit,
        (code, _) => {
            warnings.push(ImportWarning::UnknownCode { site: label.to_string(), code: code.to_string(), assumed: SiteProtocol::Ftp });
            SiteProtocol::Ftp
        }
    };
    let password = match STANDARD.decode(s(sv, "Base64Password")).ok().and_then(|b| String::from_utf8(b).ok()) {
        Some(p) => p,
        None => {
            warnings.push(ImportWarning::PasswordUnreadable(label.to_string()));
            String::new()
        }
    };
    let bookmarks = sv
        .get("BookMarks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|b| Bookmark { name: s(b, "Name").into(), remote_dir: s(b, "Path").into(), local_dir: String::new() })
        .collect();
    let port = n("Port");
    Site {
        name: label,
        host: s(sv, "Host").into(),
        port: if (1..=65535).contains(&port) { port as u16 } else { protocol.default_port() },
        protocol,
        user: s(sv, "Username").into(),
        password,
        anonymous: s(sv, "Username").is_empty(),
        remote_dir: s(sv, "DefaultRemotePath").into(),
        local_dir: local_path(s(sv, "DefaultLocalPath")),
        notes: s(sv, "Notes").into(),
        bookmarks,
        ..Site::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Same structure as the FTPRush v3 site.json, fictitious values.
    const SAMPLE: &str = r#"{
      "RootItem": { "Name": "Root", "Server": null, "Children": [
        { "Name": "Quick Connect", "Server": null, "Children": [] },
        { "Name": "FTPRush Sites", "Server": null, "Children": [
          { "Name": "00.TEST", "Server": {
              "Name": "", "Host": "ftp.example.com", "Port": 36969, "Username": "Bob",
              "Protocol": 1, "LoginType": 1, "Base64Password": "ZmF1eE1kcA==",
              "Notes": "n", "DefaultLocalPath": "/K:/_PRE/TEST/", "DefaultRemotePath": "/_PRE/TEST",
              "FTPEnryptMode": 2, "BookMarks": [ { "Name": "pre", "Path": "/_PRE" } ] },
            "Children": [] },
          { "Name": "Implicite", "Server": { "Host": "h", "Port": 990, "Username": "u", "Protocol": 1,
              "Base64Password": "", "FTPEnryptMode": 1 }, "Children": [] },
          { "Name": "Inconnu", "Server": { "Host": "h2", "Port": 22, "Username": "u", "Protocol": 7,
              "Base64Password": "", "FTPEnryptMode": 0 }, "Children": [] }
        ] }
      ] }
    }"#;

    #[test]
    fn imports_v3_sites() {
        let r = import(SAMPLE).unwrap();
        assert_eq!(r.group.groups.len(), 1, "empty Quick Connect not recreated");
        let g = &r.group.groups[0];
        assert_eq!(g.name, "FTPRush Sites");
        let a = &g.sites[0];
        assert_eq!(
            (a.name.as_str(), a.host.as_str(), a.port, a.protocol),
            ("00.TEST", "ftp.example.com", 36969, SiteProtocol::FtpsExplicit)
        );
        assert_eq!((a.user.as_str(), a.password.as_str()), ("Bob", "fauxMdp"));
        assert_eq!((a.local_dir.as_str(), a.remote_dir.as_str()), (r"K:\_PRE\TEST\", "/_PRE/TEST"));
        assert_eq!(a.bookmarks[0].remote_dir, "/_PRE");
        assert_eq!(g.sites[1].protocol, SiteProtocol::FtpsImplicit);
        assert!(r.warnings.iter().any(|w| matches!(w, ImportWarning::UnknownCode { code, .. } if code == "7")), "{:?}", r.warnings);
    }

    #[test]
    fn real_file_structure_parses_if_present() {
        // The file supplied by the user (not versioned): only check that it can be read.
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../thirdparty_source/FTPRushv3/site.json");
        if let Ok(text) = std::fs::read_to_string(p) {
            let r = import(&text).unwrap();
            assert!(r.group.site_count() >= 1);
        }
    }
}
