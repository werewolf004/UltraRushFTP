//! Conversion des fiches de site et des options (`urftp-core`) en réglages de connexion et
//! de moteur. Partagé par l'interface et l'outil en ligne de commande `urftp`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use urftp_core::settings::{GlobalProxy, Settings};
use urftp_core::site::{CertPolicy, ProxyChoice, ProxyType, Site, SiteProtocol, SiteProxy, TransferMode};

use crate::engine::EngineOptions;
use crate::ftp::{FtpConfig, Security, TlsOptions, TlsVerify};
use crate::proxy::{ProxyConfig, ProxyKind, parse_exceptions};
use crate::session::{SiteProfile, SiteProtocol as ProfileProtocol};
use crate::sftp::{HostKeyPolicy, SftpAuth, SftpConfig};
use crate::throttle::RateLimiter;

/// Proxy (de site ou global) au format du moteur.
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

/// Proxy global s'il est activé et renseigné.
pub fn global_proxy(g: &GlobalProxy) -> Option<ProxyConfig> {
    (g.enabled && !g.proxy.host.trim().is_empty()).then(|| proxy_config(&g.proxy, g.bypass_local, &g.exceptions))
}

/// Profil de connexion d'une fiche de site, avec le proxy global et les délais des options.
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
    }
}

/// Réglages du moteur de file à partir des options.
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
            name: "Mon site".into(),
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

        // Proxy global : utilisé par les sites réglés sur « Réglage global ».
        let mut settings = Settings::default();
        settings.proxy.enabled = true;
        settings.proxy.proxy.host = "proxy.lan".into();
        let p = site_profile(&Site { host: "h".into(), ..Site::default() }, &settings);
        let ProfileProtocol::Ftp(cfg) = &p.protocol else { panic!() };
        assert_eq!(cfg.proxy.as_ref().map(|x| x.host.as_str()), Some("proxy.lan"));
    }
}
