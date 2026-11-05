//! Options de l'application (fenêtre « Options »), enregistrées dans `settings.json` à côté
//! du carnet de sites.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::exists::ExistsRules;
use crate::pattern::Pattern;
use crate::priority::PriorityList;
use crate::site::SiteProxy;
use crate::skiplist::{Directions, RuleAction, RuleTarget, SkipList, SkipRule};

pub const FILE_NAME: &str = "settings.json";

/// Un masque tel qu'enregistré (joker ou expression régulière).
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

    /// Masque compilé ; `Err` si l'expression régulière est invalide.
    pub fn compile(&self) -> Result<Pattern, String> {
        if self.regex { Pattern::regex(&self.mask).map_err(|e| e.to_string()) } else { Ok(Pattern::wildcard(&self.mask)) }
    }
}

/// Règle de skiplist telle qu'enregistrée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkipRuleSetting {
    pub mask: MaskSetting,
    pub action: RuleAction,
    pub target: RuleTarget,
    pub directions: Directions,
    /// Le masque porte sur le chemin complet.
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
    /// Skiplist utilisable ; les règles dont le masque est vide ou invalide sont ignorées.
    pub fn to_skiplist(&self) -> SkipList {
        let rules = self
            .rules
            .iter()
            .filter(|r| !r.mask.mask.trim().is_empty())
            .filter_map(|r| {
                // `%W%` en tête (notation de la v2) = chemin complet.
                let (full_path, mask) = match r.mask.mask.trim().strip_prefix("%W%") {
                    Some(rest) => (true, MaskSetting { mask: rest.to_string(), regex: r.mask.regex }),
                    None => (r.full_path, r.mask.clone()),
                };
                Some(SkipRule {
                    pattern: mask.compile().ok()?,
                    action: r.action,
                    target: r.target,
                    directions: r.directions,
                    full_path,
                })
            })
            .collect();
        SkipList { enabled: self.enabled, rules, skip_zero_byte_files: self.skip_zero_byte_files, skip_empty_dirs: self.skip_empty_dirs }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransferSettings {
    /// Nouvelles tentatives après un échec.
    pub retries: u32,
    pub retry_delay_secs: u32,
    /// Transferts simultanés tous sites confondus (0 = pas de limite globale).
    pub max_total: u32,
    /// Transferts simultanés pour la connexion rapide.
    pub quick_connect_transfers: u32,
    /// Débits en Kio/s (0 = illimité).
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
    /// Ne pas passer par le proxy pour le réseau local.
    pub bypass_local: bool,
    /// Masques d'hôtes sans proxy (`220.67.*;*.lan`).
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub dark_theme: bool,
    /// Une connexion ouvre un nouvel onglet si le panneau droit est déjà connecté.
    pub new_tab_when_busy: bool,
    pub transfer: TransferSettings,
    pub exists: ExistsRules,
    pub skiplist: SkipSettings,
    pub priorities: Vec<MaskSetting>,
    pub proxy: GlobalProxy,
    pub ident: IdentSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            dark_theme: true,
            new_tab_when_busy: true,
            transfer: TransferSettings::default(),
            exists: ExistsRules::default(),
            skiplist: SkipSettings::default(),
            // Comme sur les sites de la scène : le .sfv puis le .nfo partent en premier.
            priorities: vec![MaskSetting::wildcard("*.sfv"), MaskSetting::wildcard("*.nfo")],
            proxy: GlobalProxy::default(),
            ident: IdentSettings::default(),
        }
    }
}

impl Settings {
    pub fn priority_list(&self) -> PriorityList {
        PriorityList::new(self.priorities.iter().filter(|m| !m.mask.trim().is_empty()).filter_map(|m| m.compile().ok()))
    }

    pub fn default_path() -> PathBuf {
        crate::sitestore::data_dir().join(FILE_NAME)
    }

    /// Charge les options ; fichier absent = valeurs par défaut.
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
        assert!(!json.contains("secret-proxy"), "mot de passe du proxy chiffré");
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
        let list = back.skiplist.to_skiplist();
        assert_eq!(list.rules.len(), 1, "regex invalide ignorée");
        let c = Candidate { name: "a.JPG", full_path: "/a.JPG", is_dir: false, size: Some(1), kind: TransferKind::Download };
        assert!(list.check(&c).is_some());
        assert_eq!(back.priority_list().rank("x.sfv"), Some(0));
        let mut w = SkipSettings::default();
        w.rules.push(SkipRuleSetting { mask: MaskSetting::wildcard("%W%*/proof/*"), ..SkipRuleSetting::default() });
        let wl = w.to_skiplist();
        assert!(wl.rules[0].full_path);
        let proof = Candidate { name: "a.jpg", full_path: "/rel/proof/a.jpg", is_dir: false, size: Some(1), kind: TransferKind::Fxp };
        assert!(wl.check(&proof).is_some());
        // Fichier ancien ou partiel : valeurs par défaut pour le reste.
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
