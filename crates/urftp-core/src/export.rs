//! Export of the site book: plain text (one address per line), CSV, HTML and FTPRush v2
//! XML. Passwords are never exported.

use crate::site::{Site, SiteGroup, SiteProtocol};

/// Format of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// `ftp://user@host:port/folder`, one per line.
    Text,
    Csv,
    Html,
    /// `RushSite.xml` structure, read back by the FTPRush v1/v2 import.
    FtpRushXml,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 4] = [ExportFormat::Text, ExportFormat::Csv, ExportFormat::Html, ExportFormat::FtpRushXml];

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Text => "txt",
            ExportFormat::Csv => "csv",
            ExportFormat::Html => "html",
            ExportFormat::FtpRushXml => "xml",
        }
    }
}

fn scheme(p: SiteProtocol) -> &'static str {
    match p {
        SiteProtocol::Ftp => "ftp",
        SiteProtocol::FtpsExplicit => "ftps",
        SiteProtocol::FtpsImplicit => "ftpsi",
        SiteProtocol::Sftp => "sftp",
    }
}

/// Address of a site, without password.
pub fn site_url(s: &Site) -> String {
    let user = if s.anonymous || s.user.is_empty() { String::new() } else { format!("{}@", escape_url(&s.user)) };
    let host = if s.host.contains(':') { format!("[{}]", s.host) } else { s.host.clone() };
    let dir = if s.remote_dir.trim().is_empty() { "/".to_string() } else { format!("/{}", s.remote_dir.trim().trim_start_matches('/')) };
    format!("{}://{user}{host}:{}{dir}", scheme(s.protocol), s.port)
}

fn escape_url(s: &str) -> String {
    s.chars().map(|c| if matches!(c, '@' | ':' | '/' | '%' | ' ') { format!("%{:02X}", c as u32) } else { c.to_string() }).collect()
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r', ';']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Sites with the path of their folder.
fn sites(g: &SiteGroup) -> Vec<(String, &Site)> {
    fn go<'a>(g: &'a SiteGroup, path: &str, out: &mut Vec<(String, &'a Site)>) {
        for s in &g.sites {
            out.push((path.to_string(), s));
        }
        for sub in &g.groups {
            let p = if path.is_empty() { sub.name.clone() } else { format!("{path}/{}", sub.name) };
            go(sub, &p, out);
        }
    }
    let mut out = Vec::new();
    go(g, "", &mut out);
    out
}

/// Stable 32-hexadecimal identifier of a site (FTPRush `UID`).
fn uid(s: &Site) -> String {
    let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for b in format!("{}\u{0}{}\u{0}{}", s.id, s.name, s.host).bytes() {
        h ^= u128::from(b);
        h = h.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013B);
    }
    format!("{h:032X}")
}

fn ftprush_group(g: &SiteGroup, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    for s in &g.sites {
        let ssl = match s.protocol {
            SiteProtocol::FtpsExplicit => 3,
            SiteProtocol::FtpsImplicit => 2,
            SiteProtocol::Ftp | SiteProtocol::Sftp => 0,
        };
        out.push_str(&format!(
            "{pad}<SITE NAME=\"{}\" UID=\"{}\">\n{pad}  <HOST>{}</HOST><PORT>{}</PORT><USER>{}</USER>\n{pad}  <SSL ONDATA=\"{}\">{ssl}</SSL><MAXTRANSFER LOGIN=\"{}\"/>\n{pad}  <LPATH>{}</LPATH><RPATH>{}</RPATH>\n{pad}</SITE>\n",
            xml(&s.name),
            uid(s),
            xml(&s.host),
            s.port,
            xml(if s.anonymous { "" } else { &s.user }),
            if s.protect_data { "TRUE" } else { "FALSE" },
            s.max_transfers,
            xml(&s.local_dir),
            xml(&s.remote_dir),
        ));
    }
    for sub in &g.groups {
        out.push_str(&format!("{pad}<GROUP NAME=\"{}\">\n", xml(&sub.name)));
        ftprush_group(sub, indent + 1, out);
        out.push_str(&format!("{pad}</GROUP>\n"));
    }
}

/// The sites of `g` (and of its folders) in `format`.
pub fn export(g: &SiteGroup, format: ExportFormat) -> String {
    match format {
        ExportFormat::Text => sites(g).iter().map(|(_, s)| site_url(s) + "\r\n").collect(),
        ExportFormat::Csv => {
            let mut out = String::from("folder,name,protocol,host,port,user,remote folder,local folder\r\n");
            for (path, s) in sites(g) {
                let user = if s.anonymous { "anonymous" } else { &s.user };
                let fields = [path.as_str(), &s.name, scheme(s.protocol), &s.host, &s.port.to_string(), user, &s.remote_dir, &s.local_dir];
                out.push_str(&fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
                out.push_str("\r\n");
            }
            out
        }
        ExportFormat::Html => {
            let mut out = String::from(
                "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><title>Sites</title></head><body>\n<table border=\"1\" cellpadding=\"4\">\n<tr><th>Folder</th><th>Name</th><th>Address</th><th>Local folder</th></tr>\n",
            );
            for (path, s) in sites(g) {
                out.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                    xml(&path),
                    xml(&s.name),
                    xml(&site_url(s)),
                    xml(&s.local_dir)
                ));
            }
            out.push_str("</table>\n</body></html>\n");
            out
        }
        ExportFormat::FtpRushXml => {
            let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<SITES>\n");
            ftprush_group(g, 1, &mut out);
            out.push_str("</SITES>\n");
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> SiteGroup {
        let mut root = SiteGroup::new("root");
        let mut g = SiteGroup::new("Scène & co");
        g.sites.push(Site {
            id: 7,
            name: "Alpha \"A\"".into(),
            host: "alpha.example.net".into(),
            port: 2121,
            protocol: SiteProtocol::FtpsExplicit,
            user: "bob@corp".into(),
            password: "secret".into(),
            remote_dir: "/incoming".into(),
            local_dir: r"C:\dl, here".into(),
            max_transfers: 4,
            ..Site::default()
        });
        root.groups.push(g);
        root.sites.push(Site { name: "Anon".into(), host: "::1".into(), port: 21, anonymous: true, ..Site::default() });
        root
    }

    #[test]
    fn no_password_anywhere() {
        for f in ExportFormat::ALL {
            assert!(!export(&book(), f).contains("secret"), "{f:?}");
        }
    }

    #[test]
    fn text_csv_and_html() {
        let t = export(&book(), ExportFormat::Text);
        assert_eq!(t, "ftp://[::1]:21/\r\nftps://bob%40corp@alpha.example.net:2121/incoming\r\n");
        let back = crate::urls::parse_url(t.lines().nth(1).unwrap()).unwrap();
        assert_eq!((back.user.as_str(), back.port, back.path.as_str()), ("bob@corp", 2121, "/incoming"));
        let csv = export(&book(), ExportFormat::Csv);
        assert!(csv.contains(r#"Scène & co,"Alpha ""A""",ftps,alpha.example.net,2121,bob@corp,/incoming,"C:\dl, here""#), "{csv}");
        let html = export(&book(), ExportFormat::Html);
        assert!(html.contains("Alpha &quot;A&quot;") && html.contains("Scène &amp; co"));
    }

    #[test]
    fn ftprush_xml_reads_back() {
        let x = export(&book(), ExportFormat::FtpRushXml);
        let r = crate::import::import_text("RushSite.xml", &x).unwrap();
        let g = &r.group.groups[0];
        assert_eq!(g.name, "Scène & co");
        let a = &g.sites[0];
        assert_eq!(
            (a.name.as_str(), a.host.as_str(), a.port, a.protocol, a.user.as_str()),
            ("Alpha \"A\"", "alpha.example.net", 2121, SiteProtocol::FtpsExplicit, "bob@corp")
        );
        assert_eq!((a.max_transfers, a.remote_dir.as_str(), a.local_dir.as_str()), (4, "/incoming", r"C:\dl, here"));
        assert!(r.group.sites[0].anonymous);
    }
}
