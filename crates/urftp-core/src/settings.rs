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
    /// Retries per direction (`None`: [`TransferSettings::retries`]).
    pub retries_download: Option<u32>,
    pub retries_upload: Option<u32>,
    pub retries_fxp: Option<u32>,
    /// Cycle retry: an item to retry goes to the bottom of the queue, the others run first.
    pub cycle_retry: bool,
    /// When the queue ends with failed items, they are retried this many times (0 = never).
    pub retry_failed_rounds: u32,
    /// Failed items wait again when a queue is started automatically (`-r`, scripts).
    pub reset_failed_on_auto_start: bool,
    /// Source and target of each item in the log.
    pub log_paths: bool,
    /// Logging out a site removes its waiting items from the queue of the tab.
    pub clear_queue_on_logout: bool,
    /// What to do once the queue of a tab is finished.
    pub on_queue_end: QueueEnd,
    /// Seconds to cancel the end-of-queue action (sleep, shut down...).
    pub queue_end_countdown_secs: u32,
    /// Transfer mode of the files (Auto: by the ASCII list).
    pub mode: crate::TransferMode,
    /// Files sent in ASCII in Auto mode (`;`-separated masks).
    pub ascii_masks: String,
    /// ASCII files are never resumed (their size changes with the line ends).
    pub no_resume_ascii: bool,
    /// Data connections in active mode (`PORT`) for every site.
    pub force_active: bool,
    /// Buffer of the data connections in KiB (0: the system's).
    pub tcp_buffer_kib: u32,
    /// KiB cut from the end of a partial file before resuming a download.
    pub resume_rollback_kib: u32,
    /// MiB kept free on the local disk: a download that would go below fails.
    pub keep_free_mib: u64,
    /// Sites on the local network (private addresses) ignore the speed limits.
    pub unlimited_local: bool,
    /// Case of the uploaded / downloaded names.
    pub upload_case: crate::NameCase,
    pub download_case: crate::NameCase,
    /// `SITE XDUPE 3`: dupes listed by the server are skipped.
    pub xdupe: bool,
    /// `MODE Z` (compressed data) when the server has it.
    pub mode_z: bool,
    /// Files never compressed (`;`-separated masks).
    pub mode_z_exceptions: String,
    /// Downloaded files are checked against the SFV file of their folder.
    pub check_sfv: bool,
    /// Uploads and FXP are checked with `XCRC` when the server has it.
    pub use_xcrc: bool,
    /// A file with a bad CRC is removed and transferred again.
    pub readd_bad_crc: bool,
    /// A folder with several SFV files: none of them is taken.
    pub one_sfv_per_folder: bool,
    /// Target folders left empty by a transfer (skipped contents) are removed.
    pub remove_empty_dirs: bool,
    /// Downloaded files keep the date they have on the server.
    pub keep_server_date: bool,
    /// The panel enters a folder it has just created.
    pub enter_new_folder: bool,
    /// A sub-folder named like its parent (`rel/rel`, a common mistake) is not transferred.
    pub refuse_same_name_dir: bool,
    /// When the source of an FXP refuses, the 0-byte file left on the target is removed.
    pub fxp_remove_zero_target: bool,
}

/// Masks of the ASCII list by default.
pub const DEFAULT_ASCII_MASKS: &str = "*.txt;*.nfo;*.diz;*.sfv;*.md5;*.htm;*.html;*.ini;*.log;*.m3u;*.cue;*.xml;*.csv";
/// Files not worth compressing with `MODE Z`, by default.
pub const DEFAULT_MODE_Z_EXCEPTIONS: &str = "*.rar;*.r??;*.zip;*.7z;*.gz;*.bz2;*.xz;*.mp3;*.mp4;*.mkv;*.avi;*.jpg;*.png;*.flac";

/// `;`-separated wildcard masks.
pub fn mask_list(masks: &str) -> Vec<crate::pattern::Pattern> {
    masks.split(';').map(str::trim).filter(|m| !m.is_empty()).map(crate::pattern::Pattern::wildcard).collect()
}

/// Action when the queue of a tab is finished ("On transfer completed").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueEnd {
    #[default]
    Nothing,
    /// Logs out the sites of the tab.
    Disconnect,
    /// Quits UltraRushFTP.
    Exit,
    /// Puts the computer to sleep.
    Sleep,
    Hibernate,
    ShutDown,
}

impl QueueEnd {
    pub const ALL: [QueueEnd; 6] =
        [QueueEnd::Nothing, QueueEnd::Disconnect, QueueEnd::Exit, QueueEnd::Sleep, QueueEnd::Hibernate, QueueEnd::ShutDown];

    /// Asks for a confirmation with a countdown (everything but logging out).
    pub fn needs_countdown(self) -> bool {
        !matches!(self, QueueEnd::Nothing | QueueEnd::Disconnect)
    }
}

impl TransferSettings {
    /// Retries of a transfer direction.
    pub fn retries_for(&self, kind: crate::TransferKind) -> u32 {
        match kind {
            crate::TransferKind::Download => self.retries_download,
            crate::TransferKind::Upload => self.retries_upload,
            crate::TransferKind::Fxp => self.retries_fxp,
        }
        .unwrap_or(self.retries)
    }
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
            retries_download: None,
            retries_upload: None,
            retries_fxp: None,
            cycle_retry: false,
            retry_failed_rounds: 0,
            reset_failed_on_auto_start: false,
            log_paths: false,
            clear_queue_on_logout: false,
            on_queue_end: QueueEnd::Nothing,
            queue_end_countdown_secs: 30,
            mode: crate::TransferMode::Auto,
            ascii_masks: DEFAULT_ASCII_MASKS.into(),
            no_resume_ascii: true,
            force_active: false,
            tcp_buffer_kib: 0,
            resume_rollback_kib: 0,
            keep_free_mib: 0,
            unlimited_local: false,
            upload_case: crate::NameCase::Keep,
            download_case: crate::NameCase::Keep,
            xdupe: false,
            mode_z: false,
            mode_z_exceptions: DEFAULT_MODE_Z_EXCEPTIONS.into(),
            check_sfv: false,
            use_xcrc: false,
            readd_bad_crc: true,
            one_sfv_per_folder: false,
            remove_empty_dirs: false,
            keep_server_date: false,
            enter_new_folder: false,
            refuse_same_name_dir: false,
            fxp_remove_zero_target: true,
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

/// Proxy of the list in the options, chosen by name in the site records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NamedProxy {
    pub name: String,
    pub proxy: SiteProxy,
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
    /// Answer only the requests of a connection being opened (others: `NO-USER`).
    pub only_while_connecting: bool,
}

impl Default for IdentSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            name: crate::DEFAULT_IDENT_NAME.into(),
            system: "UNIX".into(),
            port: crate::DEFAULT_IDENT_PORT,
            only_while_connecting: false,
        }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    /// Address announced with `PORT` / `EPRT` (public IP behind a router); empty: the local one.
    pub external_ip: String,
    /// First and last local port of the active mode (0: any port).
    pub port_min: u16,
    pub port_max: u16,
    /// Local address the connections leave from (several network cards); empty: the system's choice.
    pub bind_ip: String,
    /// Send `NOOP` on the control connection during the transfers (proxies and routers
    /// that drop a connection idle for too long).
    pub noop_during_transfers: bool,
    pub noop_interval_secs: u32,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            external_ip: String::new(),
            port_min: 0,
            port_max: 0,
            bind_ip: String::new(),
            noop_during_transfers: false,
            noop_interval_secs: 60,
        }
    }
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

/// What to do at startup with a queue left unfinished ("Load unfinished queue" of v2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueAtStart {
    /// Ask whether to load it.
    #[default]
    Ask,
    Load,
    Discard,
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
    /// Proxies chosen by name in the site records.
    pub proxies: Vec<NamedProxy>,
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
    /// Queue left unfinished when the application was closed.
    pub queue_at_start: QueueAtStart,
    /// The mIRC link window takes the FTPRush class (`TfmRush`), for the original DLL of
    /// FTPRush; FTPRush v2 then cannot start while UltraRushFTP runs.
    pub ftprush_dll_window: bool,
    /// Schemes saved in the conditional transfer dialog.
    pub condition_schemes: Vec<ConditionScheme>,
    pub browse: BrowseSettings,
    pub interface: InterfaceSettings,
    /// Predefined commands of the FTP menu ("Command designer").
    pub site_commands: Vec<SiteCommand>,
    /// Sites of the site manager last connected, most recent first.
    pub recent_sites: Vec<u64>,
}

/// A predefined command: one FTP command per line; `%1` is a parameter typed before
/// sending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SiteCommand {
    pub name: String,
    pub lines: String,
}

impl SiteCommand {
    fn new(name: &str, lines: &str) -> Self {
        Self { name: name.into(), lines: lines.into() }
    }
}

/// Commands offered to a new installation (glftpd and most servers).
pub fn default_site_commands() -> Vec<SiteCommand> {
    vec![
        SiteCommand::new("SITE HELP", "SITE HELP"),
        SiteCommand::new("SITE WHO", "SITE WHO"),
        SiteCommand::new("SITE NEW", "SITE NEW"),
        SiteCommand::new("SITE SEARCH", "SITE SEARCH %1"),
        SiteCommand::new("SITE STAT", "SITE STAT"),
        SiteCommand::new("SYST + FEAT", "SYST\nFEAT"),
    ]
}

/// Logs, status bar and other display settings of the main window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InterfaceSettings {
    /// The FTP and transfer logs name the site instead of "Left" / "Right".
    pub log_site_names: bool,
    /// Time of each line in the logs.
    pub log_times: bool,
    /// Lines kept by each log of a tab.
    pub log_lines: u32,
    /// The logs are also written to files, one per day and per log.
    pub log_to_disk: bool,
    /// Folder of the log files (empty: `logs` in the data folder).
    pub log_dir: String,
    /// Text of the progress bar of the status bar: `%p` percent, `%b` bytes, `%s` speed,
    /// `%e` time left, `%c` items.
    pub status_format: String,
    /// FTP addresses copied to the clipboard open "Manual download".
    pub watch_clipboard: bool,
    /// ... and are queued at once, into the local panel of the tab.
    pub clipboard_auto: bool,
    /// Keys chosen in the shortcut manager: command → `Ctrl+Shift+F, F9` (missing: the
    /// default keys).
    pub shortcuts: std::collections::BTreeMap<String, String>,
    /// Double-click on a file of a panel.
    pub double_click_file: FileDoubleClick,
    /// Ask before quitting while transfers run or wait.
    pub confirm_exit: bool,
    /// Ask before closing a tab with connections or a queue.
    pub confirm_close_tab: bool,
    /// Ask before removing items from the queue.
    pub confirm_queue_remove: bool,
}

/// What a double-click on a file does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileDoubleClick {
    /// Transfer it to the other panel (FTPRush).
    #[default]
    Transfer,
    /// Open it with the viewer (remote files).
    View,
    Nothing,
}

impl Default for InterfaceSettings {
    fn default() -> Self {
        Self {
            log_site_names: true,
            log_times: true,
            log_lines: 5000,
            log_to_disk: false,
            log_dir: String::new(),
            status_format: "%p · %b · %s · %e".into(),
            watch_clipboard: false,
            clipboard_auto: false,
            shortcuts: std::collections::BTreeMap::new(),
            double_click_file: FileDoubleClick::Transfer,
            confirm_exit: true,
            confirm_close_tab: true,
            confirm_queue_remove: false,
        }
    }
}

/// Browsing of the panels ("Cache" and "File associations" of FTPRush v2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowseSettings {
    /// Folders already visited are shown from memory ("Cache FTP directories"); `F5`
    /// always lists from the server.
    pub dir_cache: bool,
    /// The cached listing of a folder a transfer wrote into is forgotten
    /// ("Update cache of transferring directory").
    pub update_cache_on_transfer: bool,
    /// Programs that show a remote file ("View file").
    pub view: Vec<FileAssociation>,
    /// Programs that edit a remote file ("Edit file"); the file is sent back when saved.
    pub edit: Vec<FileAssociation>,
}

impl Default for BrowseSettings {
    fn default() -> Self {
        Self { dir_cache: false, update_cache_on_transfer: true, view: Vec::new(), edit: Vec::new() }
    }
}

/// Program used for the files matching some masks (`*.nfo;*.txt`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FileAssociation {
    pub masks: String,
    pub program: String,
}

/// Program of the first association whose masks match `name` (`None`: the program
/// Windows opens the file with).
pub fn program_for<'a>(list: &'a [FileAssociation], name: &str) -> Option<&'a str> {
    list.iter().find(|a| !a.program.trim().is_empty() && crate::browsing::mask_matches(&a.masks, name)).map(|a| a.program.trim())
}

/// Settings of the conditional transfer dialog, kept as a named scheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConditionScheme {
    pub name: String,
    pub folder_include: String,
    pub folder_exclude: String,
    pub file_include: String,
    pub file_exclude: String,
    /// The masks are regular expressions.
    pub regex: bool,
    pub date_between: bool,
    /// `YYYY-MM-DD`.
    pub date_from: String,
    pub date_to: String,
    pub not_older: bool,
    pub not_older_days: u32,
    pub dates_on_folders: bool,
    pub size_range: bool,
    pub min_kib: u64,
    pub max_kib: u64,
    pub no_skiplist: bool,
    pub sub_dirs: bool,
    pub disconnect_after: bool,
    pub sync: bool,
    pub delete_missing: bool,
    pub compare_date: bool,
    pub compare_size: bool,
    pub existing_only: bool,
    pub both_sides: bool,
}

impl Default for ConditionScheme {
    fn default() -> Self {
        Self {
            name: String::new(),
            folder_include: String::new(),
            folder_exclude: String::new(),
            file_include: String::new(),
            file_exclude: String::new(),
            regex: false,
            date_between: false,
            date_from: String::new(),
            date_to: String::new(),
            not_older: false,
            not_older_days: 7,
            dates_on_folders: false,
            size_range: false,
            min_kib: 0,
            max_kib: 0,
            no_skiplist: false,
            sub_dirs: true,
            disconnect_after: false,
            sync: false,
            delete_missing: false,
            compare_date: true,
            compare_size: true,
            existing_only: false,
            both_sides: false,
        }
    }
}

/// A condition that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConditionError {
    #[error("invalid regular expression: {0}")]
    Regex(String),
    #[error("invalid date (YYYY-MM-DD expected): {0}")]
    Date(String),
}

impl ConditionScheme {
    /// Options of the queued items; `now` (seconds since 1970) places "not older than".
    pub fn to_options(&self, now: i64) -> Result<crate::itemopts::ItemOptions, ConditionError> {
        use crate::itemopts::{Conditions, ItemOptions, SyncOptions, TransferFilter, parse_date};
        let filter = TransferFilter::parse(&self.folder_include, &self.folder_exclude, &self.file_include, &self.file_exclude, self.regex)
            .map_err(|e| ConditionError::Regex(e.to_string()))?;
        let date = |text: &str| -> Result<Option<i64>, ConditionError> {
            if text.trim().is_empty() { Ok(None) } else { parse_date(text).map(Some).ok_or_else(|| ConditionError::Date(text.to_string())) }
        };
        let (mut newer_than, mut older_than) = (None, None);
        if self.date_between {
            newer_than = date(&self.date_from)?;
            // The end date includes its whole day.
            older_than = date(&self.date_to)?.map(|d| d + 86_399);
        }
        if self.not_older {
            let limit = now - i64::from(self.not_older_days) * 86_400;
            newer_than = Some(newer_than.map_or(limit, |d| d.max(limit)));
        }
        let (min_size, max_size) =
            if self.size_range { (Some(self.min_kib * 1024), (self.max_kib > 0).then_some(self.max_kib * 1024)) } else { (None, None) };
        Ok(ItemOptions {
            filter,
            depth: (!self.sub_dirs).then_some(0),
            no_skiplist: self.no_skiplist,
            conditions: Conditions { newer_than, older_than, dates_on_folders: self.dates_on_folders, min_size, max_size },
            sync: self.sync.then_some(SyncOptions {
                delete_missing: self.delete_missing,
                compare_date: self.compare_date,
                compare_size: self.compare_size,
                existing_only: self.existing_only,
                both_sides: self.both_sides,
            }),
            ..ItemOptions::default()
        })
    }
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
            proxies: Vec::new(),
            ident: IdentSettings::default(),
            keep_alive: KeepAliveSettings::default(),
            listing: ListingSettings::default(),
            network: NetworkSettings::default(),
            anonymous_email: DEFAULT_ANONYMOUS_EMAIL.into(),
            download_dir: String::new(),
            new_site: Site::default(),
            queue_at_start: QueueAtStart::Ask,
            ftprush_dll_window: false,
            condition_schemes: Vec::new(),
            browse: BrowseSettings::default(),
            interface: InterfaceSettings::default(),
            site_commands: default_site_commands(),
            recent_sites: Vec::new(),
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

    /// Proxy of the list with this name.
    pub fn named_proxy(&self, name: &str) -> Option<&NamedProxy> {
        self.proxies.iter().find(|p| p.name == name)
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

    #[test]
    fn condition_scheme_to_options() {
        let day = 86_400;
        let now = crate::itemopts::epoch_seconds(2026, 11, 8, 12, 0, 0);
        let scheme = ConditionScheme {
            file_exclude: "*.nfo".into(),
            date_between: true,
            date_from: "2026-11-01".into(),
            date_to: "2026-11-07".into(),
            not_older: true,
            not_older_days: 3,
            size_range: true,
            min_kib: 1,
            sub_dirs: false,
            sync: true,
            ..ConditionScheme::default()
        };
        let o = scheme.to_options(now).unwrap();
        assert_eq!(o.conditions.newer_than, Some(now - 3 * day), "the later of the two limits");
        assert_eq!(o.conditions.older_than, Some(crate::itemopts::epoch_seconds(2026, 11, 7, 23, 59, 59)));
        assert_eq!((o.conditions.min_size, o.conditions.max_size, o.depth), (Some(1024), None, Some(0)));
        assert!(o.sync.is_some_and(|s| s.compare_size && s.compare_date));
        assert!(!o.filter.allows("a.nfo", false));
        let bad = ConditionScheme { date_between: true, date_from: "07/11/2026".into(), ..ConditionScheme::default() };
        assert!(matches!(bad.to_options(now), Err(ConditionError::Date(_))));
        let bad = ConditionScheme { regex: true, file_include: "(".into(), ..ConditionScheme::default() };
        assert!(matches!(bad.to_options(now), Err(ConditionError::Regex(_))));
        assert!(ConditionScheme::default().to_options(now).unwrap().is_default(), "nothing asked: no options");
    }
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
        let n = NetworkSettings {
            external_ip: "203.0.113.7".into(),
            port_min: 50000,
            port_max: 50100,
            bind_ip: "nope".into(),
            ..NetworkSettings::default()
        };
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
