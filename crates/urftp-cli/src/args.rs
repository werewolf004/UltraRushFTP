//! Analyse des arguments de `urftp`.

use urftp_core::exists::ExistsAction;
use urftp_core::site::{Site, SiteProtocol};

pub const USAGE: &str = "\
urftp — UltraRushFTP en ligne de commande (sites du gestionnaire ou URL)

Utilisation :
  urftp sites                              liste des sites du gestionnaire
  urftp ls   <cible>                       contenu d'un dossier distant
  urftp get  <cible> [dossier local]       télécharger un fichier ou un dossier
  urftp put  <fichier|dossier> <cible>     envoyer dans le dossier distant
  urftp fxp  <cible source> <cible dest.>  transfert site à site (FXP)
  urftp raw  <site> <commande>...          commandes brutes (FTP), réponses affichées

Cible : « NomDuSite:/chemin » (gestionnaire de sites, nom insensible à la casse)
        ou URL « ftp://user:pass@hôte:port/chemin » (ftps://, ftpsi://, sftp://).

Options :
  -v, --verbose          journal du protocole (mot de passe masqué)
  -n, --transfers <n>    transferts simultanés par site (défaut : réglage du site)
  --exists <action>      fichier existant : overwrite, resume, skip, rename
  --insecure             accepter tout certificat TLS (FTPS)
  --fingerprint <emp.>   empreinte SHA-256 attendue (certificat FTPS ou clé d'hôte SFTP)
  --data <dossier>       dossier des réglages (comme -f de l'interface)
  -h, --help             cette aide";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Sites,
    Ls(String),
    Get { src: String, dest: Option<String> },
    Put { src: String, dest: String },
    Fxp { src: String, dest: String },
    Raw { site: String, commands: Vec<String> },
    Help,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    pub verbose: bool,
    pub transfers: Option<u32>,
    pub exists: Option<ExistsAction>,
    pub insecure: bool,
    pub fingerprint: Option<String>,
    pub data_dir: Option<String>,
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<(Command, Options), String> {
    let mut opts = Options::default();
    let mut pos: Vec<String> = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} : valeur manquante"));
        match a.as_str() {
            "-h" | "--help" | "/?" => return Ok((Command::Help, opts)),
            "-v" | "--verbose" => opts.verbose = true,
            "-n" | "--transfers" => {
                let v = value(&a)?;
                opts.transfers = Some(v.parse().map_err(|_| format!("{a} : nombre attendu, pas « {v} »"))?);
            }
            "--exists" => {
                let v = value(&a)?;
                opts.exists = Some(match v.to_ascii_lowercase().as_str() {
                    "overwrite" | "ecraser" | "écraser" => ExistsAction::Overwrite,
                    "resume" | "reprendre" => ExistsAction::Resume,
                    "skip" | "ignorer" => ExistsAction::Skip,
                    "rename" | "renommer" => ExistsAction::Rename,
                    _ => return Err(format!("--exists : action inconnue « {v} »")),
                });
            }
            "--insecure" => opts.insecure = true,
            "--fingerprint" => opts.fingerprint = Some(value(&a)?),
            "--data" => opts.data_dir = Some(value(&a)?),
            s if s.starts_with("--") => return Err(format!("option inconnue : {s}")),
            _ => pos.push(a),
        }
    }
    let mut pos = pos.into_iter();
    let Some(verb) = pos.next() else { return Ok((Command::Help, opts)) };
    let rest: Vec<String> = pos.collect();
    let n = rest.len();
    let need = |ok: bool, form: &str| if ok { Ok(()) } else { Err(format!("utilisation : urftp {form}")) };
    let cmd = match verb.to_ascii_lowercase().as_str() {
        "sites" | "list" => Command::Sites,
        "ls" | "dir" => {
            need(n == 1, "ls <cible>")?;
            Command::Ls(rest[0].clone())
        }
        "get" | "download" => {
            need(n == 1 || n == 2, "get <cible> [dossier local]")?;
            Command::Get { src: rest[0].clone(), dest: rest.get(1).cloned() }
        }
        "put" | "upload" => {
            need(n == 2, "put <fichier|dossier> <cible>")?;
            Command::Put { src: rest[0].clone(), dest: rest[1].clone() }
        }
        "fxp" => {
            need(n == 2, "fxp <cible source> <cible destination>")?;
            Command::Fxp { src: rest[0].clone(), dest: rest[1].clone() }
        }
        "raw" | "quote" => {
            need(n >= 2, "raw <site> <commande>...")?;
            Command::Raw { site: rest[0].clone(), commands: rest[1..].to_vec() }
        }
        "help" => Command::Help,
        v => return Err(format!("commande inconnue : {v} (urftp --help)")),
    };
    Ok((cmd, opts))
}

/// Cible distante : site (fiche du gestionnaire ou construite depuis l'URL) et chemin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Nom d'un site du gestionnaire.
    Named { site: String, path: String },
    /// URL : fiche temporaire.
    Url { site: Box<Site>, path: String },
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(hex) = b.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(std::str::from_utf8(hex).unwrap_or("-"), 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `NomDuSite:/chemin`, `NomDuSite` (dossier de départ) ou `ftp://user:pass@hôte:port/chemin`.
pub fn parse_target(s: &str) -> Result<Target, String> {
    if let Some((scheme, rest)) = s.split_once("://") {
        let protocol = match scheme.to_ascii_lowercase().as_str() {
            "ftp" => SiteProtocol::Ftp,
            "ftps" | "ftpes" => SiteProtocol::FtpsExplicit,
            "ftpsi" => SiteProtocol::FtpsImplicit,
            "sftp" => SiteProtocol::Sftp,
            _ => return Err(format!("protocole inconnu : {scheme}")),
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], percent_decode(&rest[i..])),
            None => (rest, String::new()),
        };
        let (creds, hostport) = authority.rsplit_once('@').unwrap_or(("", authority));
        let (user, pass) = creds.split_once(':').unwrap_or((creds, ""));
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().map_err(|_| format!("port invalide : {p}"))?),
            None => (hostport, protocol.default_port()),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return Err(format!("hôte manquant : {s}"));
        }
        let user = percent_decode(user);
        let site = Site {
            name: format!("{host}:{port}"),
            host: host.to_string(),
            port,
            protocol,
            anonymous: user.is_empty(),
            user,
            password: percent_decode(pass),
            ..Site::default()
        };
        return Ok(Target::Url { site: Box::new(site), path });
    }
    let (site, path) = s.split_once(':').unwrap_or((s, ""));
    if site.trim().is_empty() {
        return Err(format!("site manquant : {s}"));
    }
    Ok(Target::Named { site: site.trim().to_string(), path: path.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<(Command, Options), String> {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn commands() {
        assert_eq!(p(&[]).unwrap().0, Command::Help);
        assert_eq!(p(&["sites"]).unwrap().0, Command::Sites);
        assert_eq!(p(&["ls", "Dev:/pub"]).unwrap().0, Command::Ls("Dev:/pub".into()));
        assert_eq!(p(&["get", "Dev:/a.bin"]).unwrap().0, Command::Get { src: "Dev:/a.bin".into(), dest: None });
        assert_eq!(
            p(&["raw", "Dev", "SITE WHO", "STAT"]).unwrap().0,
            Command::Raw { site: "Dev".into(), commands: vec!["SITE WHO".into(), "STAT".into()] }
        );
        assert!(p(&["put", "x"]).is_err());
        assert!(p(&["frobnicate"]).is_err());
    }

    #[test]
    fn options() {
        let (c, o) = p(&["-v", "get", "--exists", "overwrite", "Dev:/a", "C:\\dl", "-n", "4", "--insecure"]).unwrap();
        assert_eq!(c, Command::Get { src: "Dev:/a".into(), dest: Some("C:\\dl".into()) });
        assert_eq!((o.verbose, o.transfers, o.exists, o.insecure), (true, Some(4), Some(ExistsAction::Overwrite), true));
        assert!(p(&["-n", "x", "sites"]).is_err());
        assert!(p(&["--exists", "peut-être", "sites"]).is_err());
        assert!(p(&["--bogus"]).is_err());
    }

    #[test]
    fn targets() {
        assert_eq!(parse_target("Mon Site:/pub/a b").unwrap(), Target::Named { site: "Mon Site".into(), path: "/pub/a b".into() });
        assert_eq!(parse_target("Dev").unwrap(), Target::Named { site: "Dev".into(), path: String::new() });
        let Target::Url { site, path } = parse_target("ftps://bob:p%40ss@127.0.0.1:2121/pub/f%20x.bin").unwrap() else { panic!() };
        assert_eq!((site.host.as_str(), site.port, site.user.as_str(), site.password.as_str()), ("127.0.0.1", 2121, "bob", "p@ss"));
        assert_eq!((site.protocol, path.as_str(), site.anonymous), (SiteProtocol::FtpsExplicit, "/pub/f x.bin", false));
        let Target::Url { site, .. } = parse_target("sftp://h").unwrap() else { panic!() };
        assert_eq!((site.port, site.anonymous), (22, true));
        assert!(parse_target("gopher://h").is_err());
        assert!(parse_target("ftp://h:99999").is_err());
        assert!(parse_target(":/x").is_err());
    }
}
