//! Conversion of site records and options (`urftp-core`) into connection and engine
//! settings. Shared by the interface and the `urftp` command-line tool.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use urftp_core::settings::{GlobalProxy, Settings};
use urftp_core::site::{CertPolicy, ProxyChoice, ProxyType, Site, SiteProtocol, SiteProxy, TransferMode};

use crate::engine::EngineOptions;
use crate::ftp::{FtpConfig, Security, TlsOptions, TlsVerify};
use crate::keepalive::{KeepAlive, Reconnect, SessionPolicy, split_list};
use crate::proxy::{ProxyConfig, ProxyKind, parse_exceptions};
use crate::session::{HostList, SiteProfile, SiteProtocol as ProfileProtocol};
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
    let k = &s.keep_alive;
    SessionPolicy {
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
            attempts: s.transfer.retries.max(1),
            delay: Duration::from_secs(u64::from(s.transfer.retry_delay_secs)),
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

/// Connection profile of a site record, with the global proxy and the option timeouts.
pub fn site_profile(site: &Site, settings: &Settings) -> SiteProfile {
    let proxy = match &site.proxy {
        ProxyChoice::Custom(p) => Some(proxy_config(p, false, "")),
        ProxyChoice::Global => global_proxy(&settings.proxy),
        ProxyChoice::Direct => None,
    };
    let connect_timeout = Duration::from_secs(u64::from(settings.transfer.connect_timeout_secs));
    let io_timeout = Duration::from_secs(u64::from(settings.transfer.io_timeout_secs));
    let tls = TlsOptions {
        verify: match &site.cert {
            CertPolicy::System => TlsVerify::System,
            CertPolicy::AcceptAny => TlsVerify::AcceptAny,
            CertPolicy::Fingerprint(f) => TlsVerify::Fingerprint(f.clone()),
        },
        protect_data: site.protect_data,
        auth_ssl_fallback: true,
    };
    let ftp = |security| {
        ProfileProtocol::Ftp(FtpConfig {
            security,
            active_mode: site.transfer_mode == TransferMode::Active,
            proxy: proxy.clone(),
            connect_timeout,
            io_timeout,
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
                ..SftpConfig::default()
            },
            key: site.ssh_key_file.as_ref().map(|p| SftpAuth::KeyFile {
                path: PathBuf::from(p),
                passphrase: (!site.password.is_empty()).then(|| site.password.clone()),
            }),
        },
    };
    SiteProfile {
        name: site.name.clone(),
        host: site.host.trim().to_string(),
        port: site.port,
        user: site.login_user().to_string(),
        password: if site.anonymous { "anonymous@".into() } else { site.password.clone() },
        protocol,
        limits: site.limits(),
        session: session_policy(settings),
        hosts: HostList::new(site.alt_hosts.iter().filter_map(|a| parse_address(a, site.port)).collect()),
    }
}

/// Queue engine settings from the options.
pub fn engine_options(s: &Settings, download: &Arc<RateLimiter>, upload: &Arc<RateLimiter>) -> EngineOptions {
    EngineOptions {
        retries: s.transfer.retries,
        retry_delay: Duration::from_secs(u64::from(s.transfer.retry_delay_secs)),
        max_total: (s.transfer.max_total > 0).then_some(s.transfer.max_total),
        skiplist: s.skiplist.to_skiplist(),
        priorities: s.priority_list(),
        exists: s.exists,
        download_limit: download.clone(),
        upload_limit: upload.clone(),
        ..EngineOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
