//! Conversion of site records and options (`urftp-core`) into connection and engine
//! settings. Shared by the interface and the `urftp` command-line tool.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use urftp_core::settings::{GlobalProxy, KeepAliveSettings, Settings, mask_list};
use urftp_core::site::{CertPolicy, ListCommand, ListParser, ProxyChoice, ProxyType, Site, SiteProtocol, SiteProxy, TransferMode};

use crate::engine::{EngineOptions, TransferRules};
use crate::ftp::{FtpConfig, ListFormat, Security, TlsOptions, TlsVerify};
use crate::keepalive::{KeepAlive, Reconnect, SessionPolicy, split_list};
use crate::proxy::{ProxyConfig, ProxyKind, parse_exceptions};
use crate::session::{Credentials, HostList, SiteProfile, SiteProtocol as ProfileProtocol, SiteRules};
use crate::sftp::{HostKeyPolicy, SftpAuth, SftpConfig};
use crate::throttle::RateLimiter;

/// Proxy (site or global) in the engine format.
pub fn proxy_config(p: &SiteProxy, bypass_local: bool, exceptions: &str) -> ProxyConfig {
    ProxyConfig {
        kind: match p.kind {
            ProxyType::Socks4 => ProxyKind::Socks4,
            ProxyType::Socks4a => ProxyKind::Socks4a,
            ProxyType::Socks5 => ProxyKind::Socks5,
            ProxyType::HttpTunnel => ProxyKind::HttpTunnel,
        },
        host: p.host.trim().to_string(),
        port: p.port,
        username: (!p.user.is_empty()).then(|| p.user.clone()),
        password: (!p.user.is_empty()).then(|| p.password.clone()),
        apply_to_data: p.data_channel,
        bypass_local,
        exceptions: parse_exceptions(exceptions),
    }
}

/// Global proxy when enabled and filled in.
pub fn global_proxy(g: &GlobalProxy) -> Option<ProxyConfig> {
    (g.enabled && !g.proxy.host.trim().is_empty()).then(|| proxy_config(&g.proxy, g.bypass_local, &g.exceptions))
}

/// Keep alive and reconnection from the options (retries and delay of the transfers are
/// reused for the reconnection attempts).
pub fn session_policy(s: &Settings) -> SessionPolicy {
    policy(&s.keep_alive, s.transfer.retries, s.transfer.retry_delay_secs, s.browse.dir_cache)
}

fn policy(k: &KeepAliveSettings, retries: u32, retry_delay_secs: u32, dir_cache: bool) -> SessionPolicy {
    SessionPolicy {
        dir_cache,
        keep_alive: k.enabled.then(|| KeepAlive {
            interval: Duration::from_secs(u64::from(k.interval_secs.max(5))),
            commands: split_list(&k.commands),
            random: k.random,
            stop_after: (k.stop_after_mins > 0).then(|| Duration::from_secs(u64::from(k.stop_after_mins) * 60)),
            logout_after_stop: k.logout_after_stop,
        }),
        reconnect: Reconnect {
            when_kicked: k.reconnect_when_kicked,
            on_replies: split_list(&k.reconnect_replies),
            attempts: retries.max(1),
            delay: Duration::from_secs(u64::from(retry_delay_secs)),
        },
    }
}

/// `host`, `host:port`, `[v6]:port` or a bare IPv6 address; `None` when empty or when the
/// port is not a number.
pub fn parse_address(s: &str, default_port: u16) -> Option<(String, u16)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.trim().parse().ok()?,
            None if after.trim().is_empty() => default_port,
            None => return None,
        };
        return Some((host.to_string(), port));
    }
    match s.split_once(':') {
        // Several ':' without brackets: a bare IPv6 address.
        Some((_, rest)) if rest.contains(':') => Some((s.to_string(), default_port)),
        Some((host, port)) => Some((host.trim().to_string(), port.trim().parse().ok()?)),
        None => Some((s.to_string(), default_port)),
    }
}

/// Listing command of a site and whether `MLSD` may replace it.
pub fn list_command(site: &Site, settings: &Settings) -> (String, bool) {
    let g = &settings.listing;
    match &site.listing.command {
        ListCommand::Stat => ("STAT -l".into(), false),
        ListCommand::Custom(c) if !c.trim().is_empty() => (c.trim().to_string(), false),
        _ if !g.custom_command.trim().is_empty() => (g.custom_command.trim().to_string(), false),
        _ => ((if g.show_hidden { "LIST -a" } else { "LIST" }).into(), g.use_mlsd),
    }
}

/// Character set of a site (`None`: UTF-8, or a name `encoding_rs` does not know).
pub fn charset(site: &Site) -> Option<&'static encoding_rs::Encoding> {
    let label = site.listing.charset.trim();
    (!label.is_empty()).then(|| encoding_rs::Encoding::for_label(label.as_bytes())).flatten()
}

/// Connection profile of a site record, with the global proxy and the option timeouts.
pub fn site_profile(site: &Site, settings: &Settings) -> SiteProfile {
    let proxy = match &site.proxy {
        ProxyChoice::Custom(p) => Some(proxy_config(p, false, "")),
        ProxyChoice::Global => global_proxy(&settings.proxy),
        ProxyChoice::Direct => None,
        // A proxy removed from the list: the global setting applies.
        ProxyChoice::Named(name) => match settings.named_proxy(name) {
            Some(n) => Some(proxy_config(&n.proxy, false, "")),
            None => global_proxy(&settings.proxy),
        },
    };
    let own = &site.settings;
    let t = &settings.transfer;
    let connect_timeout = Duration::from_secs(u64::from(own.connect_timeout_secs.unwrap_or(t.connect_timeout_secs)));
    let io_timeout = Duration::from_secs(u64::from(own.io_timeout_secs.unwrap_or(t.io_timeout_secs)));
    let retries = own.retries.unwrap_or(t.retries);
    let retry_delay_secs = own.retry_delay_secs.unwrap_or(t.retry_delay_secs);
    let tls = TlsOptions {
        verify: match &site.cert {
            CertPolicy::System => TlsVerify::System,
            CertPolicy::AcceptAny => TlsVerify::AcceptAny,
            CertPolicy::Fingerprint(f) => TlsVerify::Fingerprint(f.clone()),
        },
        protect_data: site.protect_data,
        protect_listings: site.listings_protected(),
        auth_ssl_fallback: true,
    };
    let net = &settings.network;
    let (list_command, use_mlsd) = list_command(site, settings);
    let ftp = |security| {
        ProfileProtocol::Ftp(FtpConfig {
            security,
            // "Force active mode for transfers" of the options applies to every site.
            active_mode: site.transfer_mode == TransferMode::Active || settings.transfer.force_active,
            xdupe: own.xdupe.unwrap_or(settings.transfer.xdupe),
            send_buffer: buffer(own.send_buffer_kib, settings.transfer.send_buffer_kib),
            receive_buffer: buffer(own.receive_buffer_kib, settings.transfer.receive_buffer_kib),
            disabled_features: own.features.map(|f| f.disabled().into_iter().map(String::from).collect()).unwrap_or_default(),
            utf8_login: site.utf8_username,
            force_utf8: site.force_utf8,
            pasv_use_server_ip: site.behind_nat,
            send_pbsz: site.send_pbsz,
            active_external_ip: net.external(),
            active_port_range: net.port_range(),
            upnp: settings.network.upnp.then(crate::upnp::shared),
            bind_ip: net.bind(),
            list_command: list_command.clone(),
            compress_listings: settings.transfer.mode_z && settings.transfer.mode_z_listings,
            use_mlsd,
            trim_list_spaces: site.listing.trim_spaces,
            port_fallback: settings.listing.port_fallback,
            time_offset_mins: site.listing.time_offset_mins,
            charset: if site.force_utf8 { None } else { charset(site) },
            list_format: match site.listing.parser {
                ListParser::Auto => ListFormat::Auto,
                ListParser::Unix => ListFormat::Unix,
                ListParser::Dos => ListFormat::Dos,
            },
            noop_during_transfer: own
                .noop_during_transfer
                .unwrap_or(net.noop_during_transfers)
                .then(|| Duration::from_secs(u64::from(net.noop_interval_secs.max(5)))),
            proxy: proxy.clone(),
            connect_timeout,
            io_timeout,
            send_feat: own.send_feat,
            client_name: (!own.client_name.trim().is_empty()).then(|| own.client_name.trim().to_string()),
            ..FtpConfig::default()
        })
    };
    let protocol = match site.protocol {
        SiteProtocol::Ftp => ftp(Security::None),
        SiteProtocol::FtpsExplicit => ftp(Security::Explicit(tls)),
        SiteProtocol::FtpsImplicit => ftp(Security::Implicit(tls)),
        SiteProtocol::Sftp => ProfileProtocol::Sftp {
            config: SftpConfig {
                host_key: HostKeyPolicy::Known(site.ssh_host_keys.clone()),
                proxy,
                connect_timeout,
                io_timeout,
                bind_ip: net.bind(),
                ..SftpConfig::default()
            },
            key: sftp_keys(site),
        },
    };
    SiteProfile {
        name: site.name.clone(),
        host: site.host.trim().to_string(),
        port: site.port,
        user: site.login_user().to_string(),
        password: if site.anonymous { settings.anonymous_password().to_string() } else { site.password.clone() },
        protocol,
        limits: site.limits(),
        session: policy(
            own.keep_alive.as_ref().unwrap_or(&settings.keep_alive),
            retries,
            retry_delay_secs,
            own.dir_cache.unwrap_or(settings.browse.dir_cache),
        ),
        hosts: HostList::new(site.alt_hosts.iter().filter_map(|a| parse_address(a, site.port)).collect()),
        rules: SiteRules {
            retries: own.retries,
            retry_delay: own.retry_delay_secs.map(|s| Duration::from_secs(u64::from(s))),
            skiplist: own.skiplist.as_ref().map(|s| s.to_skiplist()),
            priorities: own.priorities.as_ref().map(|p| Settings::priorities_of(p)),
            exists: own.exists,
            resume: own.resume,
            upload_case: own.upload_case,
            download_case: own.download_case,
            fxp_case: own.fxp_case,
            xdupe_refresh: own.xdupe_refresh,
            fxp_tls: site.fxp_tls,
            upload_account: account(own.upload_account.as_ref(), site.port),
            download_account: account(own.download_account.as_ref(), site.port),
            fxp_source_active: site.fxp_source_active,
            fxp_target_passive: site.fxp_target_passive,
        },
        ident: None,
    }
}

/// SSH keys of a site, tried in order (the password is their passphrase).
fn sftp_keys(site: &Site) -> Option<SftpAuth> {
    let passphrase = (!site.password.is_empty()).then(|| site.password.clone());
    let mut keys: Vec<SftpAuth> = site
        .ssh_key_file
        .iter()
        .chain(&site.ssh_more_keys)
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| SftpAuth::KeyFile { path: PathBuf::from(p), passphrase: passphrase.clone() })
        .collect();
    match keys.len() {
        0 => None,
        1 => keys.pop(),
        _ => Some(SftpAuth::Any(keys)),
    }
}

/// A separate account, when its login is filled in, with its own address when given.
fn account(a: Option<&urftp_core::site::Account>, site_port: u16) -> Option<Credentials> {
    a.filter(|a| !a.user.trim().is_empty()).map(|a| Credentials {
        user: a.user.trim().to_string(),
        password: a.password.clone(),
        address: (!a.host.trim().is_empty()).then(|| (a.host.trim().to_string(), if a.port == 0 { site_port } else { a.port })),
    })
}

/// Buffer size in bytes: the site's, else the global one (0: the system's).
fn buffer(own_kib: Option<u32>, global_kib: u32) -> Option<usize> {
    let kib = own_kib.unwrap_or(global_kib);
    (kib > 0).then(|| kib as usize * 1024)
}

/// Transfer rules from the options.
pub fn transfer_rules(s: &Settings) -> TransferRules {
    let t = &s.transfer;
    TransferRules {
        mode: t.mode,
        ascii_masks: mask_list(&t.ascii_masks),
        no_resume_ascii: t.no_resume_ascii,
        resume_rollback: u64::from(t.resume_rollback_kib) * 1024,
        keep_free: t.keep_free_mib * 1024 * 1024,
        unlimited_local: t.unlimited_local,
        upload_case: t.upload_case,
        download_case: t.download_case,
        fxp_case: t.fxp_case,
        xdupe_refresh: t.xdupe_refresh,
        mode_z: t.mode_z.then(|| mask_list(&t.mode_z_exceptions)),
        mode_z_skip_downloads: !t.mode_z_downloads,
        mode_z_skip_uploads: !t.mode_z_uploads,
        sfv_status_dir: t.sfv_status_dir,
        check_sfv: t.check_sfv,
        use_xcrc: t.use_xcrc,
        readd_bad_crc: t.readd_bad_crc,
        one_sfv_per_folder: t.one_sfv_per_folder,
        remove_empty_dirs: t.remove_empty_dirs,
        keep_server_date: t.keep_server_date,
        refuse_same_name_dir: t.refuse_same_name_dir,
        fxp_remove_zero_target: t.fxp_remove_zero_target,
    }
}

/// Queue engine settings from the options.
pub fn engine_options(s: &Settings, download: &Arc<RateLimiter>, upload: &Arc<RateLimiter>) -> EngineOptions {
    EngineOptions {
        retries: s.transfer.retries,
        retries_download: s.transfer.retries_download,
        retries_upload: s.transfer.retries_upload,
        retries_fxp: s.transfer.retries_fxp,
        retry_delay: Duration::from_secs(u64::from(s.transfer.retry_delay_secs)),
        cycle_retry: s.transfer.cycle_retry,
        retry_failed_rounds: s.transfer.retry_failed_rounds,
        max_total: (s.transfer.max_total > 0).then_some(s.transfer.max_total),
        skiplist: s.skiplist.to_skiplist(),
        priorities: s.priority_list(),
        exists: s.exists,
        download_limit: download.clone(),
        upload_limit: upload.clone(),
        rules: transfer_rules(s),
        ..EngineOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_proxies_keys_and_protection() {
        use urftp_core::settings::NamedProxy;
        let mut settings = Settings::default();
        settings
            .proxies
            .push(NamedProxy { name: "Office".into(), proxy: SiteProxy { host: "office.lan".into(), port: 3128, ..SiteProxy::default() } });
        let site = Site {
            proxy: ProxyChoice::Named("Office".into()),
            protocol: SiteProtocol::FtpsExplicit,
            protect_listings: Some(false),
            ..Site::default()
        };
        let ProfileProtocol::Ftp(cfg) = site_profile(&site, &settings).protocol else { panic!() };
        assert_eq!(cfg.proxy.as_ref().map(|p| p.host.as_str()), Some("office.lan"));
        assert!(matches!(&cfg.security, Security::Explicit(o) if o.protect_data && !o.protect_listings));
        let gone = Site { proxy: ProxyChoice::Named("Removed".into()), ..Site::default() };
        let ProfileProtocol::Ftp(cfg) = site_profile(&gone, &settings).protocol else { panic!() };
        assert!(cfg.proxy.is_none(), "unknown name: the global setting (disabled here)");

        let keys = Site {
            protocol: SiteProtocol::Sftp,
            ssh_key_file: Some("a.ppk".into()),
            ssh_more_keys: vec!["b.ppk".into(), " ".into()],
            ..Site::default()
        };
        let ProfileProtocol::Sftp { key: Some(SftpAuth::Any(list)), .. } = site_profile(&keys, &settings).protocol else { panic!() };
        assert_eq!(list.len(), 2, "blank entries ignored");

        settings.network.noop_during_transfers = true;
        let ProfileProtocol::Ftp(cfg) = site_profile(&Site::default(), &settings).protocol else { panic!() };
        assert_eq!(cfg.noop_during_transfer, Some(Duration::from_secs(60)));
    }

    #[test]
    fn site_properties_reach_the_connection() {
        use urftp_core::site::{Account, FeatureSwitches, SiteListing};
        let mut settings = Settings::default();
        settings.transfer.send_buffer_kib = 64;
        settings.transfer.receive_buffer_kib = 128;
        settings.transfer.xdupe = true;
        let mut site = Site {
            port: 2121,
            utf8_username: true,
            force_utf8: true,
            behind_nat: true,
            send_pbsz: false,
            fxp_tls: Some(false),
            listing: SiteListing { charset: "windows-1252".into(), ..SiteListing::default() },
            ..Site::default()
        };
        site.settings.features = Some(FeatureSwitches { pret: false, cpsv: false, ..FeatureSwitches::default() });
        site.settings.receive_buffer_kib = Some(0);
        site.settings.xdupe = Some(false);
        site.settings.noop_during_transfer = Some(true);
        site.settings.upload_account = Some(Account { host: "up.example".into(), port: 0, user: "u".into(), password: "p".into() });
        let p = site_profile(&site, &settings);
        let ProfileProtocol::Ftp(cfg) = &p.protocol else { panic!() };
        assert!(cfg.utf8_login && cfg.force_utf8 && cfg.pasv_use_server_ip && !cfg.send_pbsz && !cfg.xdupe);
        assert!(cfg.charset.is_none(), "forced UTF-8 replaces the character set");
        assert_eq!((cfg.send_buffer, cfg.receive_buffer), (Some(64 * 1024), None), "the site sets the receive buffer to the system's");
        let ProfileProtocol::Ftp(plain) = site_profile(&Site::default(), &settings).protocol else { panic!() };
        assert_eq!((plain.send_buffer, plain.receive_buffer), (Some(64 * 1024), Some(128 * 1024)));
        assert_eq!(cfg.disabled_features, ["PRET", "CPSV"]);
        assert!(cfg.noop_during_transfer.is_some());
        assert_eq!(p.rules.fxp_tls, Some(false));
        let up = p.with_account(crate::session::Account::Upload);
        assert_eq!((up.host.as_str(), up.port, up.user.as_str()), ("up.example", 2121, "u"));
    }

    #[test]
    fn listing_and_network_settings() {
        use urftp_core::site::SiteListing;
        let mut settings = Settings::default();
        let site = Site::default();
        assert_eq!(list_command(&site, &settings), ("LIST".into(), true));
        settings.listing.show_hidden = true;
        assert_eq!(list_command(&site, &settings), ("LIST -a".into(), true));
        settings.listing.custom_command = "LIST -al".into();
        assert_eq!(list_command(&site, &settings), ("LIST -al".into(), false), "a custom command replaces MLSD");
        let stat = Site { listing: SiteListing { command: ListCommand::Stat, ..SiteListing::default() }, ..Site::default() };
        assert_eq!(list_command(&stat, &settings), ("STAT -l".into(), false));
        let blank = Site { listing: SiteListing { command: ListCommand::Custom(" ".into()), ..SiteListing::default() }, ..Site::default() };
        assert_eq!(list_command(&blank, &settings).0, "LIST -al", "an empty site command follows the global one");

        settings.network.external_ip = "203.0.113.9".into();
        settings.network.port_min = 50000;
        settings.network.port_max = 50010;
        settings.anonymous_email = "me@example.org".into();
        let site = Site {
            anonymous: true,
            listing: SiteListing { charset: "windows-1251".into(), time_offset_mins: -120, trim_spaces: true, ..SiteListing::default() },
            ..Site::default()
        };
        let p = site_profile(&site, &settings);
        assert_eq!(p.password, "me@example.org");
        let ProfileProtocol::Ftp(cfg) = &p.protocol else { panic!() };
        assert_eq!(cfg.active_external_ip.map(|i| i.to_string()).as_deref(), Some("203.0.113.9"));
        assert_eq!(cfg.active_port_range, Some((50000, 50010)));
        assert_eq!(cfg.charset, Some(encoding_rs::WINDOWS_1251));
        assert_eq!((cfg.time_offset_mins, cfg.trim_list_spaces), (-120, true));
        assert!(charset(&Site { listing: SiteListing { charset: "nope".into(), ..SiteListing::default() }, ..Site::default() }).is_none());
    }

    #[test]
    fn site_to_profile() {
        let s = Site {
            name: "My site".into(),
            host: " ftp.example.com ".into(),
            port: 2121,
            protocol: SiteProtocol::FtpsExplicit,
            user: "bob".into(),
            password: "pw".into(),
            cert: CertPolicy::Fingerprint("AA:BB".into()),
            transfer_mode: TransferMode::Active,
            max_transfers: 4,
            proxy: ProxyChoice::Custom(SiteProxy { kind: ProxyType::Socks5, host: "p".into(), port: 1080, ..SiteProxy::default() }),
            ..Site::default()
        };
        let p = site_profile(&s, &Settings::default());
        assert_eq!((p.host.as_str(), p.port, p.user.as_str(), p.limits.max_transfers), ("ftp.example.com", 2121, "bob", 4));
        let ProfileProtocol::Ftp(cfg) = &p.protocol else { panic!("FTP attendu") };
        assert!(cfg.active_mode && cfg.proxy.is_some());
        assert!(matches!(&cfg.security, Security::Explicit(o) if o.verify == TlsVerify::Fingerprint("AA:BB".into())));

        let anon = site_profile(&Site { anonymous: true, user: "x".into(), ..Site::default() }, &Settings::default());
        assert_eq!(anon.user, "anonymous");
        let sftp = site_profile(
            &Site { protocol: SiteProtocol::Sftp, ssh_key_file: Some("k.ppk".into()), password: "phrase".into(), ..Site::default() },
            &Settings::default(),
        );
        let ProfileProtocol::Sftp { key: Some(SftpAuth::KeyFile { passphrase, .. }), .. } = &sftp.protocol else { panic!() };
        assert_eq!(passphrase.as_deref(), Some("phrase"));

        // Global proxy: used by the sites set to "Global setting".
        let mut settings = Settings::default();
        settings.proxy.enabled = true;
        settings.proxy.proxy.host = "proxy.lan".into();
        let p = site_profile(&Site { host: "h".into(), ..Site::default() }, &settings);
        let ProfileProtocol::Ftp(cfg) = &p.protocol else { panic!() };
        assert_eq!(cfg.proxy.as_ref().map(|x| x.host.as_str()), Some("proxy.lan"));
    }

    #[test]
    fn addresses() {
        assert_eq!(parse_address(" ftp.example.com ", 21), Some(("ftp.example.com".into(), 21)));
        assert_eq!(parse_address("ftp.example.com:2121", 21), Some(("ftp.example.com".into(), 2121)));
        assert_eq!(parse_address("[2001:db8::1]:990", 21), Some(("2001:db8::1".into(), 990)));
        assert_eq!(parse_address("[2001:db8::1]", 21), Some(("2001:db8::1".into(), 21)));
        assert_eq!(parse_address("2001:db8::1", 21), Some(("2001:db8::1".into(), 21)));
        assert_eq!(parse_address("host:abc", 21), None);
        assert_eq!(parse_address("  ", 21), None);

        let site = Site { host: "a".into(), port: 21, alt_hosts: vec!["b:2121".into(), "".into(), "c".into()], ..Site::default() };
        let p = site_profile(&site, &Settings::default());
        assert_eq!(p.addresses(), [("a".to_string(), 21), ("b".into(), 2121), ("c".into(), 21)]);
        assert_eq!(p.current_address(), ("a".to_string(), 21));
    }

    #[test]
    fn site_settings_replace_the_options() {
        use urftp_core::site::{Account, SiteSettings};
        let g = Settings::default();
        let p = site_profile(&Site::default(), &g);
        assert!(
            p.rules.retries.is_none()
                && p.rules.skiplist.is_none()
                && p.rules.exists.is_none()
                && p.rules.resume == urftp_core::site::ResumeMode::Auto
        );
        assert!(p.rules.upload_account.is_none());

        let own = SiteSettings {
            retries: Some(7),
            retry_delay_secs: Some(1),
            connect_timeout_secs: Some(11),
            io_timeout_secs: Some(22),
            keep_alive: Some(KeepAliveSettings { enabled: false, ..KeepAliveSettings::default() }),
            resume: urftp_core::site::ResumeMode::Never,
            send_feat: false,
            client_name: " UltraRushFTP ".into(),
            priorities: Some(Vec::new()),
            upload_account: Some(Account { user: "up".into(), password: "x".into(), ..Account::default() }),
            download_account: Some(Account { user: " ".into(), password: "x".into(), ..Account::default() }),
            ..SiteSettings::default()
        };
        let p = site_profile(&Site { settings: own, ..Site::default() }, &g);
        assert_eq!((p.rules.retries, p.rules.retry_delay), (Some(7), Some(Duration::from_secs(1))));
        assert!(p.session.keep_alive.is_none(), "own keep alive: off");
        assert_eq!((p.session.reconnect.attempts, p.session.reconnect.delay), (7, Duration::from_secs(1)));
        assert!(p.rules.resume == urftp_core::site::ResumeMode::Never && p.rules.priorities.is_some());
        assert_eq!(p.rules.upload_account.as_ref().map(|c| c.user.as_str()), Some("up"));
        assert!(p.rules.download_account.is_none(), "an account without login is ignored");
        let ProfileProtocol::Ftp(cfg) = &p.protocol else { panic!() };
        assert_eq!((cfg.connect_timeout, cfg.io_timeout), (Duration::from_secs(11), Duration::from_secs(22)));
        assert!(!cfg.send_feat);
        assert_eq!(cfg.client_name.as_deref(), Some("UltraRushFTP"));

        let up = p.with_account(crate::session::Account::Upload);
        assert_eq!(up.user, "up");
        assert_eq!(p.with_account(crate::session::Account::Download).user, p.user, "no download account: main login");
    }

    #[test]
    fn login_variants() {
        let p = site_profile(&Site { user: "bob".into(), password: "pw".into(), ..Site::default() }, &Settings::default());
        let v = p.clone().with_login_variant(true, true);
        assert_eq!((v.user.as_str(), v.password.as_str()), ("!bob", "-pw"));
        let again = v.with_login_variant(true, true);
        assert_eq!((again.user.as_str(), again.password.as_str()), ("!bob", "-pw"), "never doubled");
        let sftp = site_profile(&Site { protocol: SiteProtocol::Sftp, user: "bob".into(), ..Site::default() }, &Settings::default());
        assert_eq!(sftp.with_login_variant(true, true).user, "bob", "FTP only");
    }

    #[test]
    fn keep_alive_from_the_options() {
        let mut s = Settings::default();
        let p = session_policy(&s);
        let k = p.keep_alive.expect("on by default");
        assert_eq!((k.interval, k.commands.as_slice(), k.stop_after), (Duration::from_secs(60), ["NOOP".to_string()].as_slice(), None));
        assert!(p.reconnect.when_kicked && p.reconnect.on_replies.is_empty());
        assert_eq!((p.reconnect.attempts, p.reconnect.delay), (3, Duration::from_secs(3)));

        s.keep_alive.commands = "NOOP; PWD ;".into();
        s.keep_alive.interval_secs = 1;
        s.keep_alive.stop_after_mins = 15;
        s.keep_alive.reconnect_replies = "421*;530*too many*".into();
        s.transfer.retries = 0;
        let p = session_policy(&s);
        let k = p.keep_alive.expect("still on");
        assert_eq!(k.commands, ["NOOP", "PWD"]);
        assert_eq!(k.interval, Duration::from_secs(5), "never more often than every 5 s");
        assert_eq!(k.stop_after, Some(Duration::from_secs(900)));
        assert_eq!(p.reconnect.on_replies, ["421*", "530*too many*"]);
        assert_eq!(p.reconnect.attempts, 1, "at least one attempt");

        s.keep_alive.enabled = false;
        assert!(session_policy(&s).keep_alive.is_none());
        assert_eq!(site_profile(&Site::default(), &s).session, session_policy(&s));
    }
}
