//! Application options ("Options" window), stored in `settings.json` next to the site
//! book.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::exists::ExistsRules;
use crate::pattern::Pattern;
use crate::priority::PriorityList;
use crate::site::{Site, SiteProxy};
use crate::skiplist::{Directions, RuleAction, RuleTarget, SkipList, SkipRule};

pub const FILE_NAME: &str = "settings.json";

/// A mask as stored (wildcard or regular expression).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MaskSetting {
    pub mask: String,
    pub regex: bool,
}

impl MaskSetting {
    pub fn wildcard(mask: &str) -> Self {
        Self { mask: mask.into(), regex: false }
    }

    /// Compiled mask; `Err` when the regular expression is invalid.
    pub fn compile(&self) -> Result<Pattern, regex::Error> {
        if self.regex { Pattern::regex(&self.mask) } else { Ok(Pattern::wildcard(&self.mask)) }
    }
}

/// Skip-list rule as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkipRuleSetting {
    pub mask: MaskSetting,
    pub action: RuleAction,
    pub target: RuleTarget,
    pub directions: Directions,
    /// The mask applies to the full path.
    pub full_path: bool,
}

impl Default for SkipRuleSetting {
    fn default() -> Self {
        Self {
            mask: MaskSetting::default(),
            action: RuleAction::Skip,
            target: RuleTarget::Both,
            directions: Directions::ALL,
            full_path: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkipSettings {
    pub enabled: bool,
    pub rules: Vec<SkipRuleSetting>,
    pub skip_zero_byte_files: bool,
    pub skip_empty_dirs: bool,
}

impl Default for SkipSettings {
    fn default() -> Self {
        Self { enabled: true, rules: Vec::new(), skip_zero_byte_files: false, skip_empty_dirs: false }
    }
}

impl SkipSettings {
    /// Usable skip list; rules whose mask is empty or invalid are ignored.
    pub fn to_skiplist(&self) -> SkipList {
        let rules = self
            .rules
            .iter()
            .filter(|r| !r.mask.mask.trim().is_empty())
            .filter_map(|r| {
                // Leading `%W%` (v2 notation) = full path.
                let (full_path, mask) = match r.mask.mask.trim().strip_prefix("%W%") {
                    Some(rest) => (true, MaskSetting { mask: rest.to_string(), regex: r.mask.regex }),
                    None => (r.full_path, r.mask.clone()),
                };
                Some(SkipRule { pattern: mask.compile().ok()?, action: r.action, target: r.target, directions: r.directions, full_path })
            })
            .collect();
        SkipList { enabled: self.enabled, rules, skip_zero_byte_files: self.skip_zero_byte_files, skip_empty_dirs: self.skip_empty_dirs }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransferSettings {
    /// Retries after a failure.
    pub retries: u32,
    pub retry_delay_secs: u32,
    /// Simultaneous transfers across all sites (0 = no global limit).
    pub max_total: u32,
    /// Simultaneous transfers for quick connect.
    pub quick_connect_transfers: u32,
    /// Speeds in KiB/s (0 = unlimited).
    pub download_limit_kib: u64,
    pub upload_limit_kib: u64,
    pub connect_timeout_secs: u32,
    pub io_timeout_secs: u32,
}

impl Default for TransferSettings {
    fn default() -> Self {
        Self {
            retries: 3,
            retry_delay_secs: 3,
            max_total: 0,
            quick_connect_transfers: 2,
            download_limit_kib: 0,
            upload_limit_kib: 0,
            connect_timeout_secs: 30,
            io_timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalProxy {
    pub enabled: bool,
    pub proxy: SiteProxy,
    /// Do not use the proxy for the local network.
    pub bypass_local: bool,
    /// Host masks without proxy (`220.67.*;*.lan`).
    pub exceptions: String,
}

impl Default for GlobalProxy {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy: SiteProxy { port: 1080, data_channel: true, ..SiteProxy::default() },
            bypass_local: true,
            exceptions: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdentSettings {
    pub enabled: bool,
    pub name: String,
    pub system: String,
    pub port: u16,
}

impl Default for IdentSettings {
    fn default() -> Self {
        Self { enabled: false, name: crate::DEFAULT_IDENT_NAME.into(), system: "UNIX".into(), port: crate::DEFAULT_IDENT_PORT }
    }
}

/// Keep alive (anti-idle commands) and automatic reconnection ("Keep Alive" and
/// "Auto Reconnect" of FTPRush v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeepAliveSettings {
    /// Send anti-idle commands while a connection is not used.
    pub enabled: bool,
    /// Commands, separated by `;` (`NOOP` when empty).
    pub commands: String,
    /// Seconds without activity before a command, then between two commands.
    pub interval_secs: u32,
    /// Pick the command at random in the list.
    pub random: bool,
    /// Stop after this many minutes without activity (0 = never).
    pub stop_after_mins: u32,
    /// Disconnect when the commands stop.
    pub logout_after_stop: bool,
    /// Reconnect when the server closes the connection (kick, idle timeout, `421`).
    pub reconnect_when_kicked: bool,
    /// Server replies that also trigger a reconnection: wildcard masks separated by `;`.
    pub reconnect_replies: String,
}

impl Default for KeepAliveSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            commands: "NOOP".into(),
            interval_secs: 60,
            random: false,
            stop_after_mins: 0,
            logout_after_stop: false,
            reconnect_when_kicked: true,
            reconnect_replies: String::new(),
        }
    }
}

/// Listing options (Connection page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ListingSettings {
    /// Use `MLSD` when the server announces it.
    pub use_mlsd: bool,
    /// Show the hidden files (`LIST -a`).
    pub show_hidden: bool,
    /// Listing command replacing `LIST` (empty: `LIST` or `LIST -a`).
    pub custom_command: String,
    /// Switch to active mode (`PORT`) when the passive mode fails.
    pub port_fallback: bool,
}

impl Default for ListingSettings {
    fn default() -> Self {
        Self { use_mlsd: true, show_hidden: false, custom_command: String::new(), port_fallback: false }
    }
}

/// Network options of the active mode and of the outgoing connections (Connection page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NetworkSettings {
    /// Address announced with `PORT` / `EPRT` (public IP behind a router); empty: the local one.
    pub external_ip: String,
    /// First and last local port of the active mode (0: any port).
    pub port_min: u16,
    pub port_max: u16,
    /// Local address the connections leave from (several network cards); empty: the system's choice.
    pub bind_ip: String,
}

impl NetworkSettings {
    /// External address, when it is a valid IP address.
    pub fn external(&self) -> Option<std::net::IpAddr> {
        parse_ip(&self.external_ip)
    }

    /// Local address, when it is a valid IP address.
    pub fn bind(&self) -> Option<std::net::IpAddr> {
        parse_ip(&self.bind_ip)
    }

    /// Port range, when both ends are set and in order.
    pub fn port_range(&self) -> Option<(u16, u16)> {
        (self.port_min > 0 && self.port_max >= self.port_min).then_some((self.port_min, self.port_max))
    }
}

/// `None` for an empty text; `Some(Err)` for an invalid one (shown in red in the options).
pub fn check_ip(s: &str) -> Option<Result<std::net::IpAddr, std::net::AddrParseError>> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.parse())
}

fn parse_ip(s: &str) -> Option<std::net::IpAddr> {
    check_ip(s).and_then(Result::ok)
}

/// Password sent by the anonymous logins (an e-mail address by tradition).
pub const DEFAULT_ANONYMOUS_EMAIL: &str = "urftp@";

/// Interface language chosen in the options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LanguageSetting {
    /// Language of the Windows session (English when unsupported).
    #[default]
    System,
    English,
    French,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub dark_theme: bool,
    /// Interface language.
    pub language: LanguageSetting,
    /// A connection opens a new tab when the right panel is already connected.
    pub new_tab_when_busy: bool,
    pub transfer: TransferSettings,
    pub exists: ExistsRules,
    pub skiplist: SkipSettings,
    pub priorities: Vec<MaskSetting>,
    pub proxy: GlobalProxy,
    pub ident: IdentSettings,
    pub keep_alive: KeepAliveSettings,
    pub listing: ListingSettings,
    pub network: NetworkSettings,
    /// Password of the anonymous logins.
    pub anonymous_email: String,
    /// Folder the local panels start in (empty: the user folder).
    pub download_dir: String,
    /// Model of the new sites (protocol, login, limits, folders, listing...).
    pub new_site: Site,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            dark_theme: true,
            language: LanguageSetting::System,
            new_tab_when_busy: true,
            transfer: TransferSettings::default(),
            exists: ExistsRules::default(),
            skiplist: SkipSettings::default(),
            // As on scene sites: the .sfv then the .nfo go first.
            priorities: vec![MaskSetting::wildcard("*.sfv"), MaskSetting::wildcard("*.nfo")],
            proxy: GlobalProxy::default(),
            ident: IdentSettings::default(),
            keep_alive: KeepAliveSettings::default(),
            listing: ListingSettings::default(),
            network: NetworkSettings::default(),
            anonymous_email: DEFAULT_ANONYMOUS_EMAIL.into(),
            download_dir: String::new(),
            new_site: Site::default(),
        }
    }
}

impl Settings {
    /// New site made from the model: its name, everything else from the model except the
    /// host, the alternate addresses, the bookmarks and the trusted keys.
    pub fn make_site(&self, name: impl Into<String>) -> Site {
        Site {
            id: 0,
            name: name.into(),
            host: String::new(),
            alt_hosts: Vec::new(),
            bookmarks: Vec::new(),
            ssh_host_keys: Vec::new(),
            enabled: true,
            ..self.new_site.clone()
        }
    }

    /// Password of the anonymous logins (the default one when the field is empty).
    pub fn anonymous_password(&self) -> &str {
        let e = self.anonymous_email.trim();
        if e.is_empty() { DEFAULT_ANONYMOUS_EMAIL } else { e }
    }

    pub fn priority_list(&self) -> PriorityList {
        Self::priorities_of(&self.priorities)
    }

    /// Usable priority list; empty or invalid masks are ignored.
    pub fn priorities_of(masks: &[MaskSetting]) -> PriorityList {
        PriorityList::new(masks.iter().filter(|m| !m.mask.trim().is_empty()).filter_map(|m| m.compile().ok()))
    }

    pub fn default_path() -> PathBuf {
        crate::sitestore::data_dir().join(FILE_NAME)
    }

    /// Loads the options; missing file = default values.
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransferKind;
    use crate::skiplist::Candidate;

    #[test]
    fn roundtrip_and_conversion() {
        let mut s = Settings::default();
        s.skiplist.rules.push(SkipRuleSetting { mask: MaskSetting::wildcard("*.jpg"), ..SkipRuleSetting::default() });
        s.skiplist.rules.push(SkipRuleSetting { mask: MaskSetting { mask: "(".into(), regex: true }, ..SkipRuleSetting::default() });
        s.proxy.proxy.password = "secret-proxy".into();
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("secret-proxy"), "proxy password encrypted");
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        let list = back.skiplist.to_skiplist();
        assert_eq!(list.rules.len(), 1, "invalid regex ignored");
        let c = Candidate { name: "a.JPG", full_path: "/a.JPG", is_dir: false, size: Some(1), kind: TransferKind::Download };
        assert!(list.check(&c).is_some());
        assert_eq!(back.priority_list().rank("x.sfv"), Some(0));
        let mut w = SkipSettings::default();
        w.rules.push(SkipRuleSetting { mask: MaskSetting::wildcard("%W%*/proof/*"), ..SkipRuleSetting::default() });
        let wl = w.to_skiplist();
        assert!(wl.rules[0].full_path);
        let proof = Candidate { name: "a.jpg", full_path: "/rel/proof/a.jpg", is_dir: false, size: Some(1), kind: TransferKind::Fxp };
        assert!(wl.check(&proof).is_some());
        // Old or partial file: default values for the rest.
        let partial: Settings = serde_json::from_str(r#"{"dark_theme":false}"#).unwrap();
        assert!(!partial.dark_theme && partial.transfer.retries == 3);
    }

    #[test]
    fn new_sites_follow_the_model() {
        let mut s = Settings::default();
        s.new_site.user = "me".into();
        s.new_site.max_transfers = 5;
        s.new_site.host = "kept.out".into();
        s.new_site.listing.time_offset_mins = 60;
        let site = s.make_site("Fresh");
        assert_eq!((site.name.as_str(), site.user.as_str(), site.max_transfers), ("Fresh", "me", 5));
        assert!(site.host.is_empty(), "the host of the model is not copied");
        assert_eq!(site.listing.time_offset_mins, 60);
        assert_eq!(Settings::default().anonymous_password(), DEFAULT_ANONYMOUS_EMAIL);
        s.anonymous_email = " me@example.com ".into();
        assert_eq!(s.anonymous_password(), "me@example.com");
    }

    #[test]
    fn network_values_are_checked() {
        let n = NetworkSettings { external_ip: "203.0.113.7".into(), port_min: 50000, port_max: 50100, bind_ip: "nope".into() };
        assert_eq!(n.external().map(|i| i.to_string()).as_deref(), Some("203.0.113.7"));
        assert_eq!(n.port_range(), Some((50000, 50100)));
        assert!(n.bind().is_none());
        assert!(matches!(check_ip("nope"), Some(Err(_))));
        assert!(check_ip(" ").is_none());
        assert!(NetworkSettings { port_min: 10, port_max: 5, ..NetworkSettings::default() }.port_range().is_none());
    }

    #[test]
    fn save_and_load() {
        let p = std::env::temp_dir().join(format!("urftp-settings-{}", std::process::id())).join(FILE_NAME);
        assert_eq!(Settings::load(&p).unwrap(), Settings::default());
        let s = Settings { dark_theme: false, ..Settings::default() };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap(), s);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
