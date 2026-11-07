//! Site book: nested groups (folders) and sites with their settings.

use serde::{Deserialize, Serialize};

use crate::exists::ExistsRules;
use crate::queue::SiteLimits;
use crate::settings::{KeepAliveSettings, MaskSetting, SkipSettings};

/// Protocol of a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SiteProtocol {
    #[default]
    Ftp,
    FtpsExplicit,
    FtpsImplicit,
    Sftp,
}

impl std::fmt::Display for SiteProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl SiteProtocol {
    pub const ALL: [SiteProtocol; 4] = [SiteProtocol::Ftp, SiteProtocol::FtpsExplicit, SiteProtocol::FtpsImplicit, SiteProtocol::Sftp];

    pub fn label(self) -> &'static str {
        match self {
            SiteProtocol::Ftp => "FTP",
            SiteProtocol::FtpsExplicit => "FTPS (explicit)",
            SiteProtocol::FtpsImplicit => "FTPS (implicit)",
            SiteProtocol::Sftp => "SFTP",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            SiteProtocol::Ftp | SiteProtocol::FtpsExplicit => 21,
            SiteProtocol::FtpsImplicit => 990,
            SiteProtocol::Sftp => 22,
        }
    }

    pub fn is_tls(self) -> bool {
        matches!(self, SiteProtocol::FtpsExplicit | SiteProtocol::FtpsImplicit)
    }
}

/// TLS certificate verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CertPolicy {
    #[default]
    System,
    AcceptAny,
    /// Trusted SHA-256 fingerprint.
    Fingerprint(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransferMode {
    #[default]
    Passive,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyType {
    Socks4,
    Socks4a,
    #[default]
    Socks5,
    HttpTunnel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SiteProxy {
    pub kind: ProxyType,
    pub host: String,
    pub port: u16,
    pub user: String,
    #[serde(with = "crate::secret::serde_secret")]
    pub password: String,
    /// Also apply to the data channels.
    pub data_channel: bool,
}

/// Proxy of a site ("Proxy of FTP Server" in v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyChoice {
    /// Global application setting.
    #[default]
    Global,
    /// Never use a proxy for this site.
    Direct,
    Custom(SiteProxy),
}

/// Ident name answered for this site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdentChoice {
    #[default]
    Global,
    Custom(String),
}

/// Login used instead of the main one for one direction of the transfers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Account {
    pub user: String,
    #[serde(with = "crate::secret::serde_secret")]
    pub password: String,
}

/// Settings of a site that replace the global options ("Site properties" of FTPRush v2).
/// `None` everywhere = the global setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteSettings {
    pub retries: Option<u32>,
    pub retry_delay_secs: Option<u32>,
    pub connect_timeout_secs: Option<u32>,
    pub io_timeout_secs: Option<u32>,
    pub keep_alive: Option<KeepAliveSettings>,
    /// Resume interrupted transfers (`REST`); off: always start again from zero.
    pub resume: bool,
    /// Send `FEAT` after the login (some old servers dislike it).
    pub send_feat: bool,
    /// Client name sent with `CLNT` after the login (empty: not sent).
    pub client_name: String,
    pub skiplist: Option<SkipSettings>,
    pub priorities: Option<Vec<MaskSetting>>,
    pub exists: Option<ExistsRules>,
    /// Login for the uploads (`None`: the main one).
    pub upload_account: Option<Account>,
    /// Login for the downloads (`None`: the main one).
    pub download_account: Option<Account>,
}

impl Default for SiteSettings {
    fn default() -> Self {
        Self {
            retries: None,
            retry_delay_secs: None,
            connect_timeout_secs: None,
            io_timeout_secs: None,
            keep_alive: None,
            resume: true,
            send_feat: true,
            client_name: String::new(),
            skiplist: None,
            priorities: None,
            exists: None,
            upload_account: None,
            download_account: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Bookmark {
    pub name: String,
    pub remote_dir: String,
    pub local_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Site {
    pub id: u64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub protocol: SiteProtocol,
    pub user: String,
    #[serde(with = "crate::secret::serde_secret")]
    pub password: String,
    pub anonymous: bool,
    /// Fallback addresses, tried when the first one fails ("Site IP List" in v2).
    pub alt_hosts: Vec<String>,
    pub remote_dir: String,
    pub local_dir: String,
    pub notes: String,
    /// Disabled site (`RushApp.FTP.DisableSite`).
    pub enabled: bool,
    pub cert: CertPolicy,
    /// Encrypt the data channels (`PROT P`).
    pub protect_data: bool,
    pub transfer_mode: TransferMode,
    pub max_transfers: u32,
    pub max_downloads: Option<u32>,
    pub max_uploads: Option<u32>,
    pub proxy: ProxyChoice,
    pub ident: IdentChoice,
    /// SSH private key (the password is then its passphrase).
    pub ssh_key_file: Option<String>,
    /// Trusted SSH host key fingerprints (`SHA256:…`).
    pub ssh_host_keys: Vec<String>,
    pub bookmarks: Vec<Bookmark>,
    /// Settings replacing the global options for this site.
    pub settings: SiteSettings,
}

impl Default for Site {
    fn default() -> Self {
        Self {
            id: 0,
            name: "New site".into(),
            host: String::new(),
            port: 21,
            protocol: SiteProtocol::Ftp,
            user: String::new(),
            password: String::new(),
            anonymous: false,
            alt_hosts: Vec::new(),
            remote_dir: String::new(),
            local_dir: String::new(),
            notes: String::new(),
            enabled: true,
            cert: CertPolicy::System,
            protect_data: true,
            transfer_mode: TransferMode::Passive,
            max_transfers: 2,
            max_downloads: None,
            max_uploads: None,
            proxy: ProxyChoice::Global,
            ident: IdentChoice::Global,
            ssh_key_file: None,
            ssh_host_keys: Vec::new(),
            bookmarks: Vec::new(),
            settings: SiteSettings::default(),
        }
    }
}

impl Site {
    pub fn limits(&self) -> SiteLimits {
        SiteLimits { max_transfers: self.max_transfers, max_downloads: self.max_downloads, max_uploads: self.max_uploads }
    }

    /// Effective login (anonymous: `anonymous`).
    pub fn login_user(&self) -> &str {
        if self.anonymous || self.user.is_empty() { "anonymous" } else { &self.user }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SiteGroup {
    pub name: String,
    pub groups: Vec<SiteGroup>,
    pub sites: Vec<Site>,
}

impl SiteGroup {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    pub fn site_count(&self) -> usize {
        self.sites.len() + self.groups.iter().map(SiteGroup::site_count).sum::<usize>()
    }

    /// Sites of this folder and of its subfolders.
    pub fn sites_recursive(&self) -> Vec<&Site> {
        let mut out = Vec::new();
        self.visit(&mut Vec::new(), &mut out);
        out.into_iter().map(|(_, s)| s).collect()
    }

    fn visit<'a>(&'a self, path: &mut Vec<String>, out: &mut Vec<(Vec<String>, &'a Site)>) {
        for s in &self.sites {
            out.push((path.clone(), s));
        }
        for g in &self.groups {
            path.push(g.name.clone());
            g.visit(path, out);
            path.pop();
        }
    }

    fn find_mut(&mut self, id: u64) -> Option<&mut Site> {
        if let Some(s) = self.sites.iter_mut().find(|s| s.id == id) {
            return Some(s);
        }
        self.groups.iter_mut().find_map(|g| g.find_mut(id))
    }

    fn remove(&mut self, id: u64) -> Option<Site> {
        if let Some(p) = self.sites.iter().position(|s| s.id == id) {
            return Some(self.sites.remove(p));
        }
        self.groups.iter_mut().find_map(|g| g.remove(id))
    }

    fn max_id(&self) -> u64 {
        let own = self.sites.iter().map(|s| s.id).max().unwrap_or(0);
        self.groups.iter().map(SiteGroup::max_id).max().unwrap_or(0).max(own)
    }

    /// Gives every site a fresh identifier (after an import).
    fn renumber(&mut self, next: &mut u64) {
        for s in &mut self.sites {
            *next += 1;
            s.id = *next;
        }
        for g in &mut self.groups {
            g.renumber(next);
        }
    }

    /// Group designated by a sequence of indices (empty = this group).
    pub fn group(&self, path: &[usize]) -> Option<&SiteGroup> {
        match path.split_first() {
            None => Some(self),
            Some((&i, rest)) => self.groups.get(i)?.group(rest),
        }
    }

    pub fn group_mut(&mut self, path: &[usize]) -> Option<&mut SiteGroup> {
        match path.split_first() {
            None => Some(self),
            Some((&i, rest)) => self.groups.get_mut(i)?.group_mut(rest),
        }
    }

    fn locate(&self, id: u64, path: &mut Vec<usize>) -> bool {
        if self.sites.iter().any(|s| s.id == id) {
            return true;
        }
        for (i, g) in self.groups.iter().enumerate() {
            path.push(i);
            if g.locate(id, path) {
                return true;
            }
            path.pop();
        }
        false
    }
}

/// The complete site book, stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteBook {
    pub version: u32,
    pub root: SiteGroup,
}

impl Default for SiteBook {
    fn default() -> Self {
        Self { version: 1, root: SiteGroup::new("Sites") }
    }
}

impl SiteBook {
    pub fn next_id(&self) -> u64 {
        self.root.max_id() + 1
    }

    /// Every site with the path of its folders.
    pub fn all_sites(&self) -> Vec<(Vec<String>, &Site)> {
        let mut out = Vec::new();
        self.root.visit(&mut Vec::new(), &mut out);
        out
    }

    pub fn find(&self, id: u64) -> Option<&Site> {
        self.all_sites().into_iter().map(|(_, s)| s).find(|s| s.id == id)
    }

    pub fn find_mut(&mut self, id: u64) -> Option<&mut Site> {
        self.root.find_mut(id)
    }

    /// Case-insensitive search by name, like `RushApp.FTP.Login('FTP1')`.
    pub fn find_by_name(&self, name: &str) -> Option<&Site> {
        self.all_sites().into_iter().map(|(_, s)| s).find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn remove(&mut self, id: u64) -> Option<Site> {
        self.root.remove(id)
    }

    /// Indices of the folder containing the site.
    pub fn group_path_of(&self, id: u64) -> Option<Vec<usize>> {
        let mut p = Vec::new();
        self.root.locate(id, &mut p).then_some(p)
    }

    /// Adds a site to the designated folder and returns its identifier.
    pub fn add_site(&mut self, group: &[usize], mut site: Site) -> u64 {
        site.id = self.next_id();
        let id = site.id;
        // Unknown folder: the site goes to the root.
        match self.root.group_mut(group) {
            Some(g) => g.sites.push(site),
            None => self.root.sites.push(site),
        }
        id
    }

    /// Copy of a site, placed right next to it.
    pub fn duplicate(&mut self, id: u64) -> Option<u64> {
        let mut copy = self.find(id)?.clone();
        copy.name = format!("{} (copie)", copy.name);
        let path = self.group_path_of(id)?;
        Some(self.add_site(&path, copy))
    }

    /// Grafts an imported group at the root, with new identifiers.
    pub fn merge_import(&mut self, mut group: SiteGroup) {
        let mut next = self.next_id() - 1;
        group.renumber(&mut next);
        self.root.groups.push(group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str) -> Site {
        Site { name: name.into(), host: format!("{name}.example.com"), ..Site::default() }
    }

    #[test]
    fn book_operations() {
        let mut book = SiteBook::default();
        book.root.groups.push(SiteGroup::new("Scène"));
        let a = book.add_site(&[], site("Alpha"));
        let b = book.add_site(&[0], site("Beta"));
        assert_ne!(a, b);
        assert_eq!(book.group_path_of(b), Some(vec![0]));
        assert_eq!(book.find_by_name("beta").unwrap().id, b);
        let c = book.duplicate(b).unwrap();
        assert_eq!(book.find(c).unwrap().name, "Beta (copie)");
        assert_eq!(book.root.site_count(), 3);
        book.remove(a);
        assert!(book.find(a).is_none());
        let paths: Vec<_> = book.all_sites().into_iter().map(|(p, s)| (p, s.name.clone())).collect();
        assert_eq!(paths[0], (vec!["Scène".to_string()], "Beta".to_string()));

        let mut imported = SiteGroup::new("Import");
        imported.sites.push(site("Gamma"));
        imported.sites.push(site("Delta"));
        book.merge_import(imported);
        let ids: Vec<u64> = book.all_sites().iter().map(|(_, s)| s.id).collect();
        let mut dedup = ids.clone();
        dedup.sort_unstable();
        dedup.dedup();
        assert_eq!(ids.len(), dedup.len(), "unique identifiers after import");
    }

    #[test]
    fn json_roundtrip_hides_passwords() {
        let mut book = SiteBook::default();
        let mut s = site("Alpha");
        s.password = "TrèsSecret!".into();
        s.proxy = ProxyChoice::Custom(SiteProxy { host: "p".into(), port: 1080, password: "proxypass".into(), ..SiteProxy::default() });
        s.cert = CertPolicy::Fingerprint("AB:CD".into());
        book.add_site(&[], s);
        let json = serde_json::to_string_pretty(&book).unwrap();
        assert!(!json.contains("TrèsSecret!") && !json.contains("proxypass"), "{json}");
        let back: SiteBook = serde_json::from_str(&json).unwrap();
        assert_eq!(back, book);
        // Missing fields: default values (compatibility with older files).
        let partial: SiteBook = serde_json::from_str(r#"{"root":{"name":"Sites","sites":[{"id":1,"name":"x"}]}}"#).unwrap();
        assert!(partial.root.sites[0].enabled && partial.root.sites[0].protect_data);
    }
}
