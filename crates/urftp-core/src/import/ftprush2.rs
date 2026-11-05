//! FTPRush v1/v2 : `RushSite.xml` (ou export XML).
//!
//! `<SITES>` contient des groupes (`NAME`) imbriqués et des sites (élément avec `UID`) :
//! `HOST`, `PORT`, `USER`, `LPATH`, `RPATH`, `SSL` (code), `SSL@ONDATA`, `MAXTRANSFER@LOGIN`.
//! Les mots de passe ne sont pas importés : ils sont à ressaisir dans UltraRushFTP.

use roxmltree::{Document, Node};

use super::{ImportError, ImportFormat, ImportResult};
use crate::site::{Site, SiteGroup, SiteProtocol};

pub(super) fn import(text: &str) -> Result<ImportResult, ImportError> {
    let doc = Document::parse(text).map_err(|e| ImportError::Invalid(ImportFormat::FtpRush2, e.to_string()))?;
    let root = doc.root_element();
    let mut warnings = Vec::new();
    let mut group = SiteGroup::new("Import FTPRush v2");
    walk(root, &mut group, &mut warnings);
    let n = group.site_count();
    if n == 0 {
        warnings.push("aucun site trouvé".into());
    } else {
        warnings.push(format!("mots de passe non importés : à ressaisir pour les {n} site(s)"));
    }
    Ok(ImportResult { format: ImportFormat::FtpRush2, group, warnings })
}

fn child<'a>(n: Node<'a, 'a>, tag: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(tag))
}

fn text<'a>(n: Node<'a, 'a>, tag: &str) -> &'a str {
    child(n, tag).and_then(|c| c.text()).map(str::trim).unwrap_or_default()
}

fn is_site(n: Node) -> bool {
    n.is_element() && n.attribute("UID").is_some()
}

fn walk(n: Node, parent: &mut SiteGroup, warnings: &mut Vec<String>) {
    for c in n.children().filter(Node::is_element) {
        if is_site(c) {
            parent.sites.push(site(c, warnings));
        } else if c.descendants().any(is_site) {
            let name = c.attribute("NAME").map(str::to_string).unwrap_or_else(|| c.tag_name().name().to_string());
            let mut g = SiteGroup::new(name);
            walk(c, &mut g, warnings);
            parent.groups.push(g);
        }
    }
}

fn site(n: Node, warnings: &mut Vec<String>) -> Site {
    let name = n.attribute("NAME").map(str::to_string).unwrap_or_else(|| n.tag_name().name().to_string());
    let port: u16 = text(n, "PORT").parse().unwrap_or(0);
    let ssl = child(n, "SSL");
    let code: u32 = ssl.and_then(|s| s.text()).and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    // Codes SSL de la v2 : 3 = TLS explicite (le plus courant), 2 = implicite ; port 22 = SSH.
    let protocol = match (code, port) {
        (_, 22) => SiteProtocol::Sftp,
        (3, _) => SiteProtocol::FtpsExplicit,
        (2, _) => SiteProtocol::FtpsImplicit,
        (0 | 1, _) => SiteProtocol::Ftp,
        (other, _) => {
            warnings.push(format!("« {name} » : code SSL {other} non identifié, importé en FTPS explicite — à vérifier"));
            SiteProtocol::FtpsExplicit
        }
    };
    let protect_data = ssl.and_then(|s| s.attribute("ONDATA")).map(|v| !v.eq_ignore_ascii_case("false")).unwrap_or(true);
    let max_transfers = child(n, "MAXTRANSFER").and_then(|m| m.attribute("LOGIN")).and_then(|v| v.parse().ok()).unwrap_or(2);
    let user = text(n, "USER");
    Site {
        name,
        host: text(n, "HOST").into(),
        port: if port == 0 { protocol.default_port() } else { port },
        protocol,
        user: user.into(),
        anonymous: user.is_empty(),
        local_dir: text(n, "LPATH").into(),
        remote_dir: text(n, "RPATH").into(),
        protect_data,
        max_transfers,
        ..Site::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Structure identique à un RushSite.xml, valeurs fictives.
    const SAMPLE: &str = r#"<SITES>
      <Favorites NAME="Favoris">
        <Scene NAME="Scène">
          <Alpha NAME="00.ALPHA" UID="0123456789ABCDEF0123456789ABCDEF">
            <HOST>alpha.example.net</HOST><PORT>2121</PORT><USER>bob</USER><PASS>A1B2C3</PASS>
            <SSL LIST="TRUE" ONDATA="FALSE">3</SSL><MAXTRANSFER LOGIN="4" LOGINUP="2" LOGINDOWN="2"/>
            <LPATH>C:\dl</LPATH><RPATH>/incoming</RPATH>
          </Alpha>
        </Scene>
        <Seed NAME="Seedbox" UID="FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"><HOST>seed.example.org</HOST><PORT>22</PORT><USER>u</USER><SSL>4</SSL></Seed>
        <Odd NAME="Bizarre" UID="EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE"><HOST>h</HOST><PORT>21</PORT><SSL>9</SSL></Odd>
      </Favorites>
      <Empty NAME="Vide"/>
    </SITES>"#;

    #[test]
    fn imports_groups_and_sites_without_passwords() {
        let r = import(SAMPLE).unwrap();
        assert_eq!(r.group.groups.len(), 1, "groupe vide ignoré");
        let fav = &r.group.groups[0];
        assert_eq!(fav.name, "Favoris");
        let a = &fav.groups[0].sites[0];
        assert_eq!(fav.groups[0].name, "Scène");
        assert_eq!((a.name.as_str(), a.host.as_str(), a.port, a.protocol), ("00.ALPHA", "alpha.example.net", 2121, SiteProtocol::FtpsExplicit));
        assert_eq!((a.user.as_str(), a.max_transfers, a.protect_data), ("bob", 4, false));
        assert!(a.password.is_empty(), "mot de passe non importé");
        assert_eq!((a.local_dir.as_str(), a.remote_dir.as_str()), (r"C:\dl", "/incoming"));
        assert_eq!(fav.sites[0].protocol, SiteProtocol::Sftp);
        assert!(fav.sites[1].anonymous);
        assert!(r.warnings.iter().any(|w| w.contains("code SSL 9")));
        assert!(r.warnings.iter().any(|w| w.contains("à ressaisir")));
    }
}
