//! Application options ("Options" window), stored in `settings.json` next to the site
//! book.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::exists::ExistsRules;
use crate::pattern::Pattern;
use crate::priority::PriorityList;
use crate::site::SiteProxy;
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
        }
    }
}

impl Settings {
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
    fn save_and_load() {
        let p = std::env::temp_dir().join(format!("urftp-settings-{}", std::process::id())).join(FILE_NAME);
        assert_eq!(Settings::load(&p).unwrap(), Settings::default());
        let s = Settings { dark_theme: false, ..Settings::default() };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap(), s);
        std::fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
