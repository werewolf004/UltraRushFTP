//! FileZilla 3 : `sitemanager.xml` (ou export « Exporter… » du gestionnaire de sites).
//!
//! `<FileZilla3><Servers>` contient des `<Folder>` (nom en texte) et des `<Server>`.
//! Protocoles : 0 = FTP avec TLS explicite si disponible, 1 = SFTP, 3 = FTPS implicite,
//! 4 = FTPS explicite, 6 = FTP en clair ; les autres (S3, WebDAV…) sont ignorés.
//! Mots de passe : `<Pass encoding="base64">` ; `encoding="crypt"` (mot de passe principal
//! FileZilla) ne peut pas être déchiffré.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use roxmltree::{Document, Node};

use super::{ImportError, ImportFormat, ImportResult};
use crate::site::{Site, SiteGroup, SiteProtocol, TransferMode};

pub(super) fn import(text: &str) -> Result<ImportResult, ImportError> {
    let doc = Document::parse(text).map_err(|e| ImportError::Invalid(ImportFormat::FileZilla, e.to_string()))?;
    let servers = doc
        .descendants()
        .find(|n| n.has_tag_name("Servers"))
        .ok_or_else(|| ImportError::Invalid(ImportFormat::FileZilla, "élément <Servers> absent".into()))?;
    let mut warnings = Vec::new();
    let mut group = folder(servers, "Import FileZilla".into(), &mut warnings);
    if group.site_count() == 0 {
        warnings.push("aucun site trouvé".into());
    }
    if warnings.iter().any(|w| w.contains("TLS si disponible")) {
        warnings.retain(|w| !w.contains("TLS si disponible"));
        warnings.push(
            "les sites « FTP avec TLS si disponible » sont importés en FTPS explicite ; repassez-les en FTP si le serveur ne gère pas TLS"
                .into(),
        );
    }
    group.name = "Import FileZilla".into();
    Ok(ImportResult { format: ImportFormat::FileZilla, group, warnings })
}

fn own_text(n: Node) -> String {
    n.children().filter(|c| c.is_text()).filter_map(|c| c.text()).map(str::trim).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")
}

fn child_text<'a>(n: Node<'a, 'a>, tag: &str) -> Option<&'a str> {
    n.children().find(|c| c.has_tag_name(tag)).and_then(|c| c.text()).map(str::trim)
}

fn folder(n: Node, name: String, warnings: &mut Vec<String>) -> SiteGroup {
    let mut g = SiteGroup::new(name);
    for c in n.children().filter(Node::is_element) {
        match c.tag_name().name() {
            "Folder" => {
                let name = own_text(c);
                g.groups.push(folder(c, if name.is_empty() { "Dossier".into() } else { name }, warnings));
            }
            "Server" => {
                if let Some(s) = server(c, warnings) {
                    g.sites.push(s);
                }
            }
            _ => {}
        }
    }
    g
}

fn server(n: Node, warnings: &mut Vec<String>) -> Option<Site> {
    let name = child_text(n, "Name").map(String::from).filter(|s| !s.is_empty()).unwrap_or_else(|| own_text(n));
    let host = child_text(n, "Host").unwrap_or_default().to_string();
    let label = if name.is_empty() { host.clone() } else { name.clone() };
    let protocol = match child_text(n, "Protocol").and_then(|p| p.parse::<u32>().ok()).unwrap_or(0) {
        0 => {
            warnings.push("TLS si disponible".into());
            SiteProtocol::FtpsExplicit
        }
        1 => SiteProtocol::Sftp,
        3 => SiteProtocol::FtpsImplicit,
        4 => SiteProtocol::FtpsExplicit,
        6 => SiteProtocol::Ftp,
        other => {
            warnings.push(format!("« {label} » ignoré : protocole FileZilla {other} non géré (S3, WebDAV…)"));
            return None;
        }
    };
    let mut site = Site {
        name: label.clone(),
        host,
        port: child_text(n, "Port").and_then(|p| p.parse().ok()).unwrap_or(protocol.default_port()),
        protocol,
        user: child_text(n, "User").unwrap_or_default().to_string(),
        notes: child_text(n, "Comments").unwrap_or_default().to_string(),
        remote_dir: child_text(n, "RemoteDir").map(parse_remote_dir).unwrap_or_default(),
        local_dir: child_text(n, "LocalDir").unwrap_or_default().to_string(),
        ..Site::default()
    };
    match child_text(n, "Logontype").and_then(|l| l.parse::<u32>().ok()).unwrap_or(1) {
        0 => site.anonymous = true,
        5 => site.ssh_key_file = child_text(n, "Keyfile").map(String::from),
        _ => {}
    }
    if let Some(p) = n.children().find(|c| c.has_tag_name("Pass")) {
        let raw = p.text().unwrap_or_default().trim();
        match p.attribute("encoding") {
            Some("base64") => match STANDARD.decode(raw).ok().and_then(|b| String::from_utf8(b).ok()) {
                Some(pw) => site.password = pw,
                None => warnings.push(format!("« {label} » : mot de passe illisible")),
            },
            Some("crypt") => warnings.push(format!(
                "« {label} » : mot de passe protégé par le mot de passe principal de FileZilla, non importé"
            )),
            _ => site.password = raw.to_string(),
        }
    }
    if child_text(n, "PasvMode") == Some("MODE_ACTIVE") {
        site.transfer_mode = TransferMode::Active;
    }
    Some(site)
}

/// Chemin distant FileZilla : `1 0 4 home 3 bob` → `/home/bob`
/// (type de serveur, longueur du préfixe, puis segments préfixés par leur longueur).
fn parse_remote_dir(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let number = |i: &mut usize| -> Option<usize> {
        let start = *i;
        while *i < chars.len() && chars[*i].is_ascii_digit() {
            *i += 1;
        }
        let n = chars[start..*i].iter().collect::<String>().parse().ok();
        if *i < chars.len() && chars[*i] == ' ' {
            *i += 1;
        }
        n
    };
    let take = |i: &mut usize, len: usize| -> Option<String> {
        let seg: String = chars.get(*i..*i + len)?.iter().collect();
        *i += len;
        if *i < chars.len() && chars[*i] == ' ' {
            *i += 1;
        }
        Some(seg)
    };
    let (Some(_kind), Some(prefix_len)) = (number(&mut i), number(&mut i)) else { return String::new() };
    let prefix = if prefix_len > 0 { take(&mut i, prefix_len).unwrap_or_default() } else { String::new() };
    let mut segs = Vec::new();
    while i < chars.len() {
        let Some(len) = number(&mut i) else { break };
        match take(&mut i, len) {
            Some(seg) => segs.push(seg),
            None => break,
        }
    }
    format!("{prefix}/{}", segs.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FileZilla3 version="3.66.0" platform="windows">
  <Servers>
    <Server>
      <Host>ftp.example.com</Host><Port>21</Port><Protocol>4</Protocol><Type>0</Type>
      <User>alice</User><Pass encoding="base64">c2VjcmV0w6k=</Pass><Logontype>1</Logontype>
      <PasvMode>MODE_ACTIVE</PasvMode><Comments>note</Comments>
      <RemoteDir>1 0 4 home 9 mon dossi</RemoteDir>
      <Name>Mon FTPS</Name>
    </Server>
    <Folder expanded="1">Scène
      <Server>
        <Host>sftp.example.org</Host><Port>2222</Port><Protocol>1</Protocol>
        <User>bob</User><Logontype>5</Logontype><Keyfile>C:\keys\bob.ppk</Keyfile>
        Ancien nom
      </Server>
      <Server><Host>s3.amazonaws.com</Host><Protocol>8</Protocol><Name>S3</Name></Server>
      <Folder>Vide</Folder>
    </Folder>
    <Server><Host>anon.example.net</Host><Protocol>6</Protocol><Logontype>0</Logontype><Name>Anonyme</Name></Server>
    <Server><Host>x</Host><Protocol>0</Protocol><Pass encoding="crypt" pubkey="abc">zzz</Pass><Name>Chiffré</Name></Server>
  </Servers>
</FileZilla3>"#;

    #[test]
    fn imports_sites_folders_and_passwords() {
        let r = import(SAMPLE).unwrap();
        let g = &r.group;
        assert_eq!(g.sites.len(), 3);
        let a = &g.sites[0];
        assert_eq!((a.name.as_str(), a.host.as_str(), a.port, a.protocol), ("Mon FTPS", "ftp.example.com", 21, SiteProtocol::FtpsExplicit));
        assert_eq!((a.user.as_str(), a.password.as_str()), ("alice", "secreté"));
        assert_eq!(a.transfer_mode, TransferMode::Active);
        assert_eq!(a.remote_dir, "/home/mon dossi");
        assert_eq!(g.groups[0].name, "Scène");
        let b = &g.groups[0].sites[0];
        assert_eq!((b.name.as_str(), b.protocol, b.port), ("Ancien nom", SiteProtocol::Sftp, 2222));
        assert_eq!(b.ssh_key_file.as_deref(), Some(r"C:\keys\bob.ppk"));
        assert_eq!(g.groups[0].sites.len(), 1, "S3 ignoré");
        assert!(g.sites[1].anonymous && g.sites[1].protocol == SiteProtocol::Ftp);
        assert!(g.sites[2].password.is_empty());
        assert!(r.warnings.iter().any(|w| w.contains("S3")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("mot de passe principal")));
        assert!(r.warnings.iter().any(|w| w.contains("FTPS explicite")));
    }

    #[test]
    fn remote_dir_format() {
        assert_eq!(parse_remote_dir("1 0 4 home 3 bob"), "/home/bob");
        assert_eq!(parse_remote_dir("1 0"), "/");
        assert_eq!(parse_remote_dir(""), "");
    }
}
