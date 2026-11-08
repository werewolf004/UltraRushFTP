//! FTP addresses typed or copied by the user: `ftp://user:pass@host:port/path`, and the
//! incremented lists of "Manual download" (`file[01-12].rar`).

/// Protocol of an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlScheme {
    Ftp,
    /// `ftps://` or `ftpes://`: explicit FTPS.
    FtpsExplicit,
    /// `ftpsi://`: implicit FTPS.
    FtpsImplicit,
    Sftp,
}

impl UrlScheme {
    pub fn default_port(self) -> u16 {
        match self {
            UrlScheme::Ftp | UrlScheme::FtpsExplicit => 21,
            UrlScheme::FtpsImplicit => 990,
            UrlScheme::Sftp => 22,
        }
    }
}

/// A parsed address. `path` is absolute (`/` when missing); it ends with `/` for a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtpUrl {
    pub scheme: UrlScheme,
    pub user: String,
    pub password: String,
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl FtpUrl {
    /// The address names a folder (`.../`).
    pub fn is_folder(&self) -> bool {
        self.path.ends_with('/')
    }

    /// Last name of the path (empty for the root).
    pub fn name(&self) -> &str {
        self.path.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
    }
}

/// `%xx` escapes of an address part.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%').then(|| b.get(i + 1..i + 3)).flatten().and_then(|h| std::str::from_utf8(h).ok());
        if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses `scheme://[user[:password]@]host[:port][/path]` (`[v6]` hosts allowed).
pub fn parse_url(text: &str) -> Option<FtpUrl> {
    let text = text.trim();
    let (scheme, rest) = text.split_once("://")?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "ftp" => UrlScheme::Ftp,
        "ftps" | "ftpes" => UrlScheme::FtpsExplicit,
        "ftpsi" => UrlScheme::FtpsImplicit,
        "sftp" => UrlScheme::Sftp,
        _ => return None,
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (login, hostport) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };
    let (user, password) = match login {
        Some(l) => match l.split_once(':') {
            Some((u, p)) => (unescape(u), unescape(p)),
            None => (unescape(l), String::new()),
        },
        None => (String::new(), String::new()),
    };
    let (host, port) = if let Some(v6) = hostport.strip_prefix('[') {
        let (h, after) = v6.split_once(']')?;
        (h.to_string(), after.strip_prefix(':').map(str::parse).transpose().ok()?)
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p.parse().ok()?)),
            None => (hostport.to_string(), None),
        }
    };
    if host.is_empty() {
        return None;
    }
    Some(FtpUrl { scheme, user, password, port: port.unwrap_or(scheme.default_port()), host, path: unescape(path) })
}

/// Site record of an address: named after its login and host, folder kept.
pub fn site_of_url(u: &FtpUrl) -> crate::site::Site {
    use crate::site::SiteProtocol;
    let user = if u.user.is_empty() { "anonymous".to_string() } else { u.user.clone() };
    let dir = u.path.trim_end_matches('/');
    crate::site::Site {
        name: format!("{user}@{}:{}", u.host, u.port),
        host: u.host.clone(),
        port: u.port,
        protocol: match u.scheme {
            UrlScheme::Ftp => SiteProtocol::Ftp,
            UrlScheme::FtpsExplicit => SiteProtocol::FtpsExplicit,
            UrlScheme::FtpsImplicit => SiteProtocol::FtpsImplicit,
            UrlScheme::Sftp => SiteProtocol::Sftp,
        },
        anonymous: u.user.is_empty() && u.password.is_empty(),
        user,
        password: u.password.clone(),
        remote_dir: dir.to_string(),
        ..crate::site::Site::default()
    }
}

/// Every address of a text (one per line or separated by spaces), for the clipboard.
pub fn find_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|w| {
            let l = w.to_ascii_lowercase();
            ["ftp://", "ftps://", "ftpes://", "ftpsi://", "sftp://"].iter().any(|s| l.starts_with(s))
        })
        .map(|w| w.trim_end_matches([',', ';', ')', '"', '\'']).to_string())
        .collect()
}

/// Expands the ranges of an incremented address: `a[1-3].r` → `a1.r`, `a2.r`, `a3.r`;
/// `[08-10]` keeps the zeros (`08`, `09`, `10`); several ranges multiply. At most
/// `limit` results (a typing mistake must not make millions of items).
pub fn expand_increments(text: &str, limit: usize) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']').map(|c| open + c) else { break };
        let inner = &rest[open + 1..close];
        let range = inner.split_once('-').and_then(|(a, b)| {
            let (x, y) = (a.trim().parse::<u64>().ok()?, b.trim().parse::<u64>().ok()?);
            Some((x, y, a.trim().len()))
        });
        let prefix = &rest[..open];
        match range {
            Some((a, b, width)) if a <= b => {
                let mut next = Vec::new();
                'outer: for base in &out {
                    for n in a..=b {
                        if next.len() >= limit {
                            break 'outer;
                        }
                        next.push(format!("{base}{prefix}{n:0width$}"));
                    }
                }
                out = next;
            }
            _ => {
                for base in &mut out {
                    base.push_str(&rest[..=close]);
                }
            }
        }
        rest = &rest[close + 1..];
    }
    for base in &mut out {
        base.push_str(rest);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let u = parse_url("ftp://bob:p%40ss@host.example:2121/pub/a%20b.zip").unwrap();
        assert_eq!((u.user.as_str(), u.password.as_str(), u.host.as_str(), u.port), ("bob", "p@ss", "host.example", 2121));
        assert_eq!(u.path, "/pub/a b.zip");
        assert_eq!(u.name(), "a b.zip");
        assert!(!u.is_folder());
        let s = parse_url("sftp://[::1]/home/").unwrap();
        assert_eq!((s.host.as_str(), s.port, s.is_folder(), s.name()), ("::1", 22, true, "home"));
        assert_eq!(parse_url("ftpsi://h").map(|u| (u.port, u.path)), Some((990, "/".into())));
        assert_eq!(parse_url("http://h/x"), None);
        assert_eq!(parse_url("ftp://h:x/"), None);
        assert_eq!(parse_url("ftp:///x"), None);
    }

    #[test]
    fn found_in_text() {
        assert_eq!(find_urls("see ftp://a/x.zip, and\nSFTP://b/y\nnot http://c"), ["ftp://a/x.zip", "SFTP://b/y"]);
    }

    #[test]
    fn increments() {
        assert_eq!(expand_increments("f[1-3].r", 100), ["f1.r", "f2.r", "f3.r"]);
        assert_eq!(expand_increments("f[08-10]", 100), ["f08", "f09", "f10"]);
        assert_eq!(expand_increments("[1-2]x[1-2]", 100), ["1x1", "1x2", "2x1", "2x2"]);
        assert_eq!(expand_increments("keep[a]and[3-1]", 100), ["keep[a]and[3-1]"]);
        assert_eq!(expand_increments("n[1-1000000]", 5).len(), 5);
    }
}
