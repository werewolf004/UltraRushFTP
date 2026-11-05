//! FlashFXP: `.ftp` site export (XML).
//!
//! `<SITES VERSION="…">` holds one element per site (or per folder): `ADDRESS`, `PORT`,
//! `USERNAME`, `PROTOCOL` (`FTP`, `SFTP`...), `SSL` (`AUTH TLS`, `AUTH SSL`, `IMPLICIT`...),
//! `PASSIVE`, `LOGIN_TYPE`, `LOCALPATH`, `REMOTEPATH`.
//! Passwords are not imported: they must be entered again in UltraRushFTP.

use roxmltree::{Document, Node};

use super::{ImportError, ImportFormat, ImportResult, ImportWarning};
use crate::site::{Site, SiteGroup, SiteProtocol, TransferMode};

pub(super) fn import(text: &str) -> Result<ImportResult, ImportError> {
    let doc = Document::parse(text).map_err(|e| ImportError::Invalid(ImportFormat::FlashFxp, e.to_string()))?;
    let mut warnings = Vec::new();
    let mut group = SiteGroup::new("Import FlashFXP");
    walk(doc.root_element(), &mut group, &mut warnings);
    let n = group.site_count();
    if n == 0 {
        warnings.push(ImportWarning::NoSites);
    } else {
        warnings.push(ImportWarning::PasswordsNotImported(n));
    }
    Ok(ImportResult { format: ImportFormat::FlashFxp, group, warnings })
}

fn text<'a>(n: Node<'a, 'a>, tag: &str) -> &'a str {
    n.children().find(|c| c.has_tag_name(tag)).and_then(|c| c.text()).map(str::trim).unwrap_or_default()
}

fn is_site(n: Node) -> bool {
    n.is_element() && n.children().any(|c| c.has_tag_name("ADDRESS"))
}

fn name_of(n: Node) -> String {
    n.attribute("NAME").map(str::to_string).unwrap_or_else(|| n.tag_name().name().to_string())
}

fn walk(n: Node, parent: &mut SiteGroup, warnings: &mut Vec<ImportWarning>) {
    for c in n.children().filter(Node::is_element) {
        if is_site(c) {
            if let Some(s) = site(c, warnings) {
                parent.sites.push(s);
            }
        } else if c.descendants().any(is_site) {
            let mut g = SiteGroup::new(name_of(c));
            walk(c, &mut g, warnings);
            parent.groups.push(g);
        }
    }
}

fn site(n: Node, warnings: &mut Vec<ImportWarning>) -> Option<Site> {
    let name = name_of(n);
    let ssl = text(n, "SSL").to_ascii_uppercase();
    let protocol = match text(n, "PROTOCOL").to_ascii_uppercase().as_str() {
        "SFTP" | "SSH" => SiteProtocol::Sftp,
        "FTP" | "" => match ssl.as_str() {
            "" | "NONE" => SiteProtocol::Ftp,
            s if s.contains("IMPLICIT") => SiteProtocol::FtpsImplicit,
            _ => SiteProtocol::FtpsExplicit,
        },
        other => {
            warnings.push(ImportWarning::UnsupportedProtocol {
                site: name.to_string(),
                source_format: ImportFormat::FlashFxp,
                protocol: other.to_string(),
            });
            return None;
        }
    };
    let port: u16 = text(n, "PORT").parse().unwrap_or(0);
    let user = text(n, "USERNAME");
    Some(Site {
        name,
        host: text(n, "ADDRESS").into(),
        port: if port == 0 { protocol.default_port() } else { port },
        protocol,
        user: user.into(),
        anonymous: user.is_empty() || text(n, "LOGIN_TYPE") == "0",
        local_dir: text(n, "LOCALPATH").into(),
        remote_dir: text(n, "REMOTEPATH").into(),
        transfer_mode: if text(n, "PASSIVE").eq_ignore_ascii_case("FALSE") { TransferMode::Active } else { TransferMode::Passive },
        ..Site::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Same structure as a FlashFXP export, fictitious values.
    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
    <SITES VERSION="1.3">
      <Alpha NAME="00.ALPHA">
        <ADDRESS>alpha.example.net</ADDRESS><PORT>2121</PORT><USERNAME>bob</USERNAME><PASSWORD>xxx</PASSWORD>
        <PROTOCOL>FTP</PROTOCOL><SSL>AUTH TLS</SSL><PASSIVE>DEFAULT</PASSIVE><LOGIN_TYPE>1</LOGIN_TYPE>
        <LOCALPATH>C:\dl</LOCALPATH><REMOTEPATH>/incoming</REMOTEPATH>
      </Alpha>
      <Dossier NAME="SSH">
        <Seed NAME="Seedbox"><ADDRESS>seed.example.org</ADDRESS><PORT>22</PORT><PROTOCOL>SFTP</PROTOCOL><USERNAME>u</USERNAME></Seed>
      </Dossier>
      <Web NAME="Web"><ADDRESS>w</ADDRESS><PROTOCOL>HTTP</PROTOCOL></Web>
      <Plain NAME="Clair"><ADDRESS>p</ADDRESS><PROTOCOL>FTP</PROTOCOL><PASSIVE>FALSE</PASSIVE><LOGIN_TYPE>0</LOGIN_TYPE></Plain>
    </SITES>"#;

    #[test]
    fn imports_sites_without_passwords() {
        let r = import(SAMPLE).unwrap();
        let a = &r.group.sites[0];
        assert_eq!(
            (a.name.as_str(), a.host.as_str(), a.port, a.protocol),
            ("00.ALPHA", "alpha.example.net", 2121, SiteProtocol::FtpsExplicit)
        );
        assert_eq!(a.user, "bob");
        assert!(a.password.is_empty(), "password not imported");
        assert_eq!((a.local_dir.as_str(), a.remote_dir.as_str()), (r"C:\dl", "/incoming"));
        assert_eq!(r.group.groups[0].sites[0].protocol, SiteProtocol::Sftp);
        let plain = r.group.sites.iter().find(|s| s.name == "Clair").unwrap();
        assert!(plain.anonymous && plain.transfer_mode == TransferMode::Active && plain.protocol == SiteProtocol::Ftp);
        assert_eq!(r.group.site_count(), 3, "HTTP site skipped");
        assert!(r.warnings.iter().any(|w| matches!(w, ImportWarning::UnsupportedProtocol { protocol, .. } if protocol == "HTTP")));
    }
}
