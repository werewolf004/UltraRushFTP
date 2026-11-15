//! Site book: nested groups (folders) and sites with their settings.

use serde::{Deserialize, Serialize};

use crate::exists::ExistsSet;
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
    /// A proxy of the list in the options, by name.
    Named(String),
}

/// Ident name answered for this site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdentChoice {
    #[default]
    Global,
    Custom(String),
}

/// Login used instead of the main one for one direction of the transfers ("Account" of
/// FTPRush v2), possibly on another address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Account {
    /// Other address (empty: the one of the site).
    pub host: String,
    /// Port of that address (0: the one of the site).
    pub port: u16,
    pub user: String,
    #[serde(with = "crate::secret::serde_secret")]
    pub password: String,
}

/// Resuming interrupted transfers ("Resume support" of FTPRush v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResumeMode {
    /// Resume; when the server refuses `REST`, follow the "existing file" rules (start again
    /// from zero, by default).
    #[default]
    Auto,
    /// Always resume: a refused `REST` fails the transfer.
    Always,
    /// Never resume: always start again from zero.
    Never,
}

/// Reads the mode, or the `true` / `false` of the files written before 0.8.0.
impl<'de> Deserialize<'de> for ResumeMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Named {
            Auto,
            Always,
            Never,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Old(bool),
            Named(Named),
        }
        Ok(match Stored::deserialize(d)? {
            Stored::Old(true) | Stored::Named(Named::Auto) => ResumeMode::Auto,
            Stored::Old(false) | Stored::Named(Named::Never) => ResumeMode::Never,
            Stored::Named(Named::Always) => ResumeMode::Always,
        })
    }
}

/// Capabilities a site may use when its server announces them (`FEAT`); one switched off
/// is treated as not announced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeatureSwitches {
    pub pret: bool,
    /// `OPTS UTF8 ON` when the server announces `UTF8`.
    pub utf8: bool,
    pub mode_z: bool,
    pub xcrc: bool,
    pub mlsd: bool,
    pub sscn: bool,
    pub cpsv: bool,
}

impl Default for FeatureSwitches {
    fn default() -> Self {
        Self { pret: true, utf8: true, mode_z: true, xcrc: true, mlsd: true, sscn: true, cpsv: true }
    }
}

impl FeatureSwitches {
    /// `FEAT` names switched off.
    pub fn disabled(&self) -> Vec<&'static str> {
        [
            (self.pret, "PRET"),
            (self.utf8, "UTF8"),
            (self.mode_z, "MODE Z"),
            (self.xcrc, "XCRC"),
            (self.mlsd, "MLST"),
            (self.sscn, "SSCN"),
            (self.cpsv, "CPSV"),
        ]
        .into_iter()
        .filter(|(on, _)| !on)
        .map(|(_, n)| n)
        .collect()
    }
}

/// Statistics of one site ("Statistics (this FTP server)" of FTPRush v2), in bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SiteStats {
    /// Last connection (`YYYY-MM-DD HH:MM:SS`, local time).
    pub last_access: String,
    pub connections: u64,
    /// Uploaded to the site.
    pub sent: u64,
    /// Downloaded from the site.
    pub received: u64,
    /// FXP from the site to another one.
    pub fxp_sent: u64,
    /// FXP from another site to this one.
    pub fxp_received: u64,
}

impl SiteStats {
    pub fn total_sent(&self) -> u64 {
        self.sent.saturating_add(self.fxp_sent)
    }

    pub fn total_received(&self) -> u64 {
        self.received.saturating_add(self.fxp_received)
    }
}

/// Folder of the site checked for new entries ("Notification" of the site properties).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteMonitor {
    pub path: String,
    pub interval_mins: u32,
}

impl Default for SiteMonitor {
    fn default() -> Self {
        Self { path: "/".into(), interval_mins: 10 }
    }
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
    /// Resuming interrupted transfers (`REST`).
    pub resume: ResumeMode,
    /// Send `FEAT` after the login (some old servers dislike it).
    pub send_feat: bool,
    /// Capabilities used when announced (`None`: all of them).
    pub features: Option<FeatureSwitches>,
    /// Client name sent with `CLNT` after the login (empty: not sent).
    pub client_name: String,
    pub skiplist: Option<SkipSettings>,
    pub priorities: Option<Vec<MaskSetting>>,
    /// "Existing file" rules per direction (`None`: the global ones).
    pub exists: Option<ExistsSet>,
    /// Case of the names uploaded, downloaded, copied by FXP (`None`: the global setting).
    pub upload_case: Option<crate::NameCase>,
    pub download_case: Option<crate::NameCase>,
    pub fxp_case: Option<crate::NameCase>,
    /// `SITE XDUPE` (`None`: the global setting).
    pub xdupe: Option<bool>,
    /// Lists the target folder again when an upload is refused without `XDUPE` names.
    pub xdupe_refresh: Option<bool>,
    /// `NOOP` on the control channel during the transfers (`None`: the global setting).
    pub noop_during_transfer: Option<bool>,
    /// Buffers of the data connections in KiB (`None`: the global setting).
    pub send_buffer_kib: Option<u32>,
    pub receive_buffer_kib: Option<u32>,
    /// Login for the uploads (`None`: the main one).
    pub upload_account: Option<Account>,
    /// Login for the downloads (`None`: the main one).
    pub download_account: Option<Account>,
    /// Directory cache (`None`: the global setting).
    pub dir_cache: Option<bool>,
}

impl Default for SiteSettings {
    fn default() -> Self {
        Self {
            retries: None,
            retry_delay_secs: None,
            connect_timeout_secs: None,
            io_timeout_secs: None,
            keep_alive: None,
            resume: ResumeMode::Auto,
            send_feat: true,
            features: None,
            client_name: String::new(),
            skiplist: None,
            priorities: None,
            exists: None,
            upload_case: None,
            download_case: None,
            fxp_case: None,
            xdupe: None,
            xdupe_refresh: None,
            noop_during_transfer: None,
            send_buffer_kib: None,
            receive_buffer_kib: None,
            upload_account: None,
            download_account: None,
            dir_cache: None,
        }
    }
}

/// Listing command of a site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ListCommand {
    /// The global setting (`MLSD` when announced, otherwise `LIST`, `LIST -a` or the global
    /// custom command).
    #[default]
    Global,
    /// `STAT -l`: listing on the control channel (no data connection).
    Stat,
    /// Any command (`LIST -al`, `NLST`...); a command starting with `STAT` is read on the
    /// control channel.
    Custom(String),
}

/// Character sets offered for the servers that do not use UTF-8 (WHATWG labels).
pub const CHARSETS: [&str; 14] = [
    "windows-1252",
    "ISO-8859-1",
    "ISO-8859-2",
    "ISO-8859-15",
    "windows-1250",
    "windows-1251",
    "KOI8-R",
    "Shift_JIS",
    "EUC-JP",
    "GBK",
    "gb18030",
    "Big5",
    "EUC-KR",
    "IBM866",
];

/// Format of the `LIST` lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ListParser {
    /// Unix or DOS/IIS, recognised line by line.
    #[default]
    Auto,
    Unix,
    Dos,
}

/// Web address of the files of a site ("HTTP URL mapping" in v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HttpMapping {
    /// Address of the remote folder below (`https://www.example.com/files`).
    pub base_url: String,
    /// Remote folder served at that address (empty: `/`).
    pub ftp_root: String,
}

impl HttpMapping {
    pub fn is_set(&self) -> bool {
        !self.base_url.trim().is_empty()
    }

    /// Web address of a remote path; `None` when no address is set or when the path is
    /// outside the mapped folder. The names are percent-encoded.
    pub fn url_of(&self, remote_path: &str) -> Option<String> {
        let base = self.base_url.trim().trim_end_matches('/');
        if base.is_empty() {
            return None;
        }
        let root = self.ftp_root.trim().trim_end_matches('/');
        let rest = if root.is_empty() {
            remote_path
        } else {
            let r = remote_path.strip_prefix(root)?;
            if !(r.is_empty() || r.starts_with('/')) {
                return None;
            }
            r
        };
        let encoded: Vec<String> = rest.split('/').filter(|s| !s.is_empty()).map(percent_encode).collect();
        Some(if encoded.is_empty() { format!("{base}/") } else { format!("{base}/{}", encoded.join("/")) })
    }
}

/// Percent-encoding of a path segment (RFC 3986: unreserved characters kept).
fn percent_encode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for b in segment.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Listing settings of a site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SiteListing {
    pub command: ListCommand,
    /// Removes the spaces at the start of the listing lines (some `STAT -l` replies).
    pub trim_spaces: bool,
    /// Minutes added to the dates of the listings (server in another time zone).
    pub time_offset_mins: i32,
    /// Character set of the names and paths (empty: UTF-8).
    pub charset: String,
    /// Format of the `LIST` lines.
    pub parser: ListParser,
}

#[cfg(test)]
mod http_tests {
    use super::HttpMapping;

    #[test]
    fn remote_paths_become_web_addresses() {
        let m = HttpMapping { base_url: "https://www.example.com/files/".into(), ftp_root: "/pub/".into() };
        assert_eq!(m.url_of("/pub/a b/été.txt").as_deref(), Some("https://www.example.com/files/a%20b/%C3%A9t%C3%A9.txt"));
        assert_eq!(m.url_of("/pub").as_deref(), Some("https://www.example.com/files/"));
        assert_eq!(m.url_of("/public/x"), None, "outside the mapped folder");
        assert_eq!(m.url_of("/other"), None);
        let whole = HttpMapping { base_url: "http://h.example.org".into(), ftp_root: String::new() };
        assert_eq!(whole.url_of("/x/y.zip").as_deref(), Some("http://h.example.org/x/y.zip"));
        assert_eq!(HttpMapping::default().url_of("/x"), None);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Bookmark {
    pub name: String,
    pub remote_dir: String,
    pub local_dir: String,
}

impl Bookmark {
    /// Remote folder with its date tokens (`%yyyy%`, `%mm%`…) replaced by the local date `at`.
    pub fn remote_at(&self, at: i64) -> String {
        crate::urls::expand_date_tokens(self.remote_dir.trim(), at)
    }

    /// Local folder with its date tokens replaced by the local date `at`.
    pub fn local_at(&self, at: i64) -> String {
        crate::urls::expand_date_tokens(self.local_dir.trim(), at)
    }
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
    /// Short text shown next to the name (tree, menus).
    pub description: String,
    /// Disabled site (`RushApp.FTP.DisableSite`).
    pub enabled: bool,
    pub cert: CertPolicy,
    /// Encrypt the data channels of the transfers (`PROT P`).
    pub protect_data: bool,
    /// Encrypt the data channels of the listings (`None`: like the transfers).
    pub protect_listings: Option<bool>,
    pub transfer_mode: TransferMode,
    pub max_transfers: u32,
    pub max_downloads: Option<u32>,
    pub max_uploads: Option<u32>,
    pub proxy: ProxyChoice,
    pub ident: IdentChoice,
    /// SSH private key (the password is then its passphrase).
    pub ssh_key_file: Option<String>,
    /// Other keys, tried in turn when the server refuses the first one.
    pub ssh_more_keys: Vec<String>,
    /// Trusted SSH host key fingerprints (`SHA256:…`).
    pub ssh_host_keys: Vec<String>,
    pub bookmarks: Vec<Bookmark>,
    /// Settings replacing the global options for this site.
    pub settings: SiteSettings,
    /// Listing command, time zone and character set (FTP).
    pub listing: SiteListing,
    /// FXP from this site: always `PORT` (the other site listens).
    pub fxp_source_active: bool,
    /// FXP to this site: always `PASV` (this site listens).
    pub fxp_target_passive: bool,
    pub http: HttpMapping,
    /// The login is sent in UTF-8 even when the site uses another character set.
    pub utf8_username: bool,
    /// UTF-8 is used even when the server does not announce it (`OPTS UTF8 ON` sent).
    pub force_utf8: bool,
    /// The server is behind NAT: the data connections always go to the address of the
    /// control connection, whatever `PASV` answers.
    pub behind_nat: bool,
    /// `PBSZ 0` after the TLS login (some servers refuse it).
    pub send_pbsz: bool,
    /// Explicit FTPS with `AUTH SSL` instead of `AUTH TLS` (servers that hang up on
    /// `AUTH TLS`).
    pub auth_ssl: bool,
    /// Encrypted FXP from or to this site (`None`: automatic, `SSCN` or `CPSV`).
    pub fxp_tls: Option<bool>,
    /// The logs show the name of the site instead of "Left" / "Right", whatever the
    /// global option.
    pub log_site_name: bool,
    /// New entries in the folders below are notified.
    pub notify_new: bool,
    pub monitors: Vec<SiteMonitor>,
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
            description: String::new(),
            enabled: true,
            cert: CertPolicy::System,
            protect_data: true,
            protect_listings: None,
            transfer_mode: TransferMode::Passive,
            max_transfers: 2,
            max_downloads: None,
            max_uploads: None,
            proxy: ProxyChoice::Global,
            ident: IdentChoice::Global,
            ssh_key_file: None,
            ssh_more_keys: Vec::new(),
            ssh_host_keys: Vec::new(),
            bookmarks: Vec::new(),
            settings: SiteSettings::default(),
            listing: SiteListing::default(),
            fxp_source_active: false,
            fxp_target_passive: false,
            http: HttpMapping::default(),
            utf8_username: false,
            force_utf8: false,
            behind_nat: false,
            send_pbsz: true,
            auth_ssl: false,
            fxp_tls: None,
            log_site_name: false,
            notify_new: false,
            monitors: Vec::new(),
        }
    }
}

impl Site {
    /// Encryption of the listings: its own setting, or the one of the transfers.
    pub fn listings_protected(&self) -> bool {
        self.protect_listings.unwrap_or(self.protect_data)
    }

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

    /// Moves a site to a folder: before the site `before` when it is in that folder, at the
    /// end otherwise. Returns `false` (and changes nothing) for an unknown site or folder.
    pub fn move_site(&mut self, id: u64, group: &[usize], before: Option<u64>) -> bool {
        if before == Some(id) || self.root.group(group).is_none() {
            return false;
        }
        let Some(site) = self.root.remove(id) else { return false };
        let Some(g) = self.root.group_mut(group) else { return false };
        let at = before.and_then(|b| g.sites.iter().position(|s| s.id == b)).unwrap_or(g.sites.len());
        g.sites.insert(at, site);
        true
    }

    /// Moves a folder (with its content) at the end of another folder and returns its new
    /// path. Refused for the root, an unknown folder, or a folder into itself or one of its
    /// subfolders.
    pub fn move_group(&mut self, from: &[usize], to: &[usize]) -> Option<Vec<usize>> {
        let (&last, parent) = from.split_last()?;
        if to.starts_with(from) || self.root.group(to).is_none() || parent == to {
            return None;
        }
        let group = self.root.group_mut(parent).filter(|p| last < p.groups.len())?.groups.remove(last);
        // The folders after the one taken out of `parent` moved up by one.
        let mut to = to.to_vec();
        if to.len() > parent.len() && to.starts_with(parent) && to[parent.len()] > last {
            to[parent.len()] -= 1;
        }
        let target = self.root.group_mut(&to)?;
        target.groups.push(group);
        to.push(target.groups.len() - 1);
        Some(to)
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
    fn sites_and_folders_move() {
        let mut book = SiteBook::default();
        book.root.groups.push(SiteGroup::new("A"));
        book.root.groups.push(SiteGroup::new("B"));
        book.root.groups[1].groups.push(SiteGroup::new("B1"));
        let x = book.add_site(&[], site("X"));
        let y = book.add_site(&[0], site("Y"));
        let z = book.add_site(&[0], site("Z"));
        // Into another folder, before a site, at the end.
        assert!(book.move_site(x, &[0], Some(z)));
        let names = |g: &SiteGroup| g.sites.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&book.root.groups[0]), ["Y", "X", "Z"]);
        assert!(book.move_site(y, &[1, 0], None));
        assert_eq!(book.group_path_of(y), Some(vec![1, 0]));
        assert!(!book.move_site(y, &[7], None), "unknown folder");
        assert!(!book.move_site(99, &[], None), "unknown site");
        assert_eq!(book.group_path_of(y), Some(vec![1, 0]));
        // A folder into a later sibling: the target index shifts.
        assert_eq!(book.move_group(&[0], &[1]), Some(vec![0, 1]));
        assert_eq!(book.root.groups.len(), 1);
        assert_eq!(book.root.groups[0].groups[1].name, "A");
        assert_eq!(book.group_path_of(z), Some(vec![0, 1]));
        // Never into itself or below itself, never the root.
        assert_eq!(book.move_group(&[0], &[0, 0]), None);
        assert_eq!(book.move_group(&[0], &[0]), None);
        assert_eq!(book.move_group(&[], &[0]), None);
        // Back to the root.
        assert_eq!(book.move_group(&[0, 1], &[]), Some(vec![1]));
        assert_eq!(book.root.groups[1].name, "A");
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
