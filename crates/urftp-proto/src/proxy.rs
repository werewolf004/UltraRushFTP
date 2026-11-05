//! Connection through a proxy: SOCKS4, SOCKS4A, SOCKS5 (RFC 1928/1929) and HTTP tunnel
//! (`CONNECT`), as in FTPRush v2. Used for the control channel and, when requested, for
//! the data channels.

use std::net::{IpAddr, SocketAddr};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, lookup_host};

/// Proxy type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProxyKind {
    /// SOCKS4: IPv4 address resolved locally.
    Socks4,
    /// SOCKS4A: the proxy resolves the server name.
    Socks4a,
    /// SOCKS5: IPv4, IPv6 or name, with optional authentication.
    Socks5,
    /// HTTP tunnel (`CONNECT host:port`), optional Basic authentication.
    HttpTunnel,
}

/// Proxy used by a site (or the global proxy).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Also route the data channels through the proxy (v2 default).
    pub apply_to_data: bool,
    /// Do not use the proxy for local addresses (LAN, `localhost`, name without a dot).
    pub bypass_local: bool,
    /// Host masks bypassing the proxy (`*` and `?`), e.g. `220.67.*`.
    pub exceptions: Vec<String>,
}

impl ProxyConfig {
    pub fn new(kind: ProxyKind, host: impl Into<String>, port: u16) -> Self {
        Self {
            kind,
            host: host.into(),
            port,
            username: None,
            password: None,
            apply_to_data: true,
            bypass_local: true,
            exceptions: Vec::new(),
        }
    }

    pub fn with_auth(mut self, user: impl Into<String>, pass: impl Into<String>) -> Self {
        self.username = Some(user.into());
        self.password = Some(pass.into());
        self
    }

    /// Must the server `target` be reached without the proxy?
    pub fn bypasses(&self, target: &str) -> bool {
        (self.bypass_local && is_local_host(target))
            || self.exceptions.iter().any(|m| !m.trim().is_empty() && wildcard_match(m.trim(), target))
    }
}

/// Splits an exception list in the v2 format: `220.67.*;12.1.3.*;211.*`.
pub fn parse_exceptions(list: &str) -> Vec<String> {
    list.split([';', ',']).map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Proxy negotiation errors.
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("cannot connect to the proxy: {0}")]
    Io(#[from] std::io::Error),
    #[error("the proxy refused the authentication")]
    AuthFailed,
    #[error("the proxy refused the connection to {target}: {reason}")]
    Refused { target: String, reason: String },
    #[error("invalid proxy reply: {0}")]
    Protocol(String),
}

/// Opens a TCP connection to `target_host:target_port` through the proxy.
pub async fn connect(proxy: &ProxyConfig, target_host: &str, target_port: u16) -> Result<TcpStream, ProxyError> {
    let mut s = TcpStream::connect((proxy.host.as_str(), proxy.port)).await?;
    s.set_nodelay(true)?;
    match proxy.kind {
        ProxyKind::Socks4 | ProxyKind::Socks4a => socks4(&mut s, proxy, target_host, target_port).await?,
        ProxyKind::Socks5 => socks5(&mut s, proxy, target_host, target_port).await?,
        ProxyKind::HttpTunnel => http_connect(&mut s, proxy, target_host, target_port).await?,
    }
    Ok(s)
}

fn target_str(host: &str, port: u16) -> String {
    if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") }
}

async fn socks4(s: &mut TcpStream, p: &ProxyConfig, host: &str, port: u16) -> Result<(), ProxyError> {
    let mut req = vec![4u8, 1];
    req.extend_from_slice(&port.to_be_bytes());
    let ip = match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => Some(v4),
        Ok(IpAddr::V6(_)) => return Err(ProxyError::Protocol("SOCKS4 does not support IPv6".into())),
        Err(_) if p.kind == ProxyKind::Socks4a => None,
        Err(_) => {
            let v4 = lookup_host((host, port)).await?.find_map(|a| match a {
                SocketAddr::V4(v4) => Some(*v4.ip()),
                SocketAddr::V6(_) => None,
            });
            Some(v4.ok_or_else(|| ProxyError::Protocol(format!("no IPv4 address for {host}")))?)
        }
    };
    // SOCKS4A: address 0.0.0.1, then the host name after the user id.
    req.extend_from_slice(&ip.map_or([0, 0, 0, 1], |v| v.octets()));
    req.extend_from_slice(p.username.as_deref().unwrap_or("").as_bytes());
    req.push(0);
    if ip.is_none() {
        req.extend_from_slice(host.as_bytes());
        req.push(0);
    }
    s.write_all(&req).await?;
    let mut rep = [0u8; 8];
    s.read_exact(&mut rep).await?;
    match rep[1] {
        0x5A => Ok(()),
        code => Err(ProxyError::Refused {
            target: target_str(host, port),
            reason: match code {
                0x5B => "request rejected".into(),
                0x5C | 0x5D => "identification (identd) refused".into(),
                c => format!("code {c:#04x}"),
            },
        }),
    }
}

async fn socks5(s: &mut TcpStream, p: &ProxyConfig, host: &str, port: u16) -> Result<(), ProxyError> {
    let auth = p.username.is_some();
    s.write_all(if auth { &[5, 2, 0, 2] } else { &[5, 1, 0] }).await?;
    let mut rep = [0u8; 2];
    s.read_exact(&mut rep).await?;
    if rep[0] != 5 {
        return Err(ProxyError::Protocol(format!("version SOCKS {} inattendue", rep[0])));
    }
    match rep[1] {
        0 => {}
        2 if auth => {
            let user = p.username.as_deref().unwrap_or("").as_bytes();
            let pass = p.password.as_deref().unwrap_or("").as_bytes();
            if user.len() > 255 || pass.len() > 255 {
                return Err(ProxyError::Protocol("user name or password too long (255 max)".into()));
            }
            let mut req = vec![1u8, user.len() as u8];
            req.extend_from_slice(user);
            req.push(pass.len() as u8);
            req.extend_from_slice(pass);
            s.write_all(&req).await?;
            s.read_exact(&mut rep).await?;
            if rep[1] != 0 {
                return Err(ProxyError::AuthFailed);
            }
        }
        0xFF => return Err(ProxyError::AuthFailed),
        m => return Err(ProxyError::Protocol(format!("authentication method {m} not supported"))),
    }

    let mut req = vec![5u8, 1, 0];
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => {
            req.push(1);
            req.extend_from_slice(&v4.octets());
        }
        Ok(IpAddr::V6(v6)) => {
            req.push(4);
            req.extend_from_slice(&v6.octets());
        }
        Err(_) => {
            if host.len() > 255 {
                return Err(ProxyError::Protocol("host name too long for SOCKS5".into()));
            }
            req.push(3);
            req.push(host.len() as u8);
            req.extend_from_slice(host.as_bytes());
        }
    }
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req).await?;

    let mut head = [0u8; 4];
    s.read_exact(&mut head).await?;
    if head[1] != 0 {
        let reason = match head[1] {
            1 => "general failure",
            2 => "not allowed by the proxy rules",
            3 => "network unreachable",
            4 => "host unreachable",
            5 => "connection refused",
            6 => "TTL expired",
            7 => "command not supported",
            8 => "address type not supported",
            _ => "unknown error",
        };
        return Err(ProxyError::Refused { target: target_str(host, port), reason: reason.into() });
    }
    // Address bound by the proxy: read then ignored.
    let len = match head[3] {
        1 => 4,
        4 => 16,
        3 => {
            let mut l = [0u8; 1];
            s.read_exact(&mut l).await?;
            l[0] as usize
        }
        t => return Err(ProxyError::Protocol(format!("unknown address type {t}"))),
    };
    let mut rest = vec![0u8; len + 2];
    s.read_exact(&mut rest).await?;
    Ok(())
}

async fn http_connect(s: &mut TcpStream, p: &ProxyConfig, host: &str, port: u16) -> Result<(), ProxyError> {
    let target = target_str(host, port);
    let mut req = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(user) = &p.username {
        let creds = format!("{user}:{}", p.password.as_deref().unwrap_or(""));
        req += &format!("Proxy-Authorization: Basic {}\r\n", base64(creds.as_bytes()));
    }
    req += "\r\n";
    s.write_all(req.as_bytes()).await?;

    // Byte by byte: nothing may be consumed after the header (it is the FTP stream).
    let mut head = Vec::new();
    let mut b = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() > 16 * 1024 {
            return Err(ProxyError::Protocol("HTTP header too long".into()));
        }
        if s.read(&mut b).await? == 0 {
            return Err(ProxyError::Protocol("connection closed during the HTTP reply".into()));
        }
        head.push(b[0]);
    }
    let text = String::from_utf8_lossy(&head);
    let status_line = text.lines().next().unwrap_or("");
    let code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| ProxyError::Protocol(format!("statut HTTP illisible : {status_line:?}")))?;
    match code {
        200..=299 => Ok(()),
        407 => Err(ProxyError::AuthFailed),
        _ => Err(ProxyError::Refused { target, reason: status_line.to_string() }),
    }
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = match chunk.len() {
            3 => u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8 | u32::from(chunk[2]),
            2 => u32::from(chunk[0]) << 16 | u32::from(chunk[1]) << 8,
            _ => u32::from(chunk[0]) << 16,
        };
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Local address: `localhost`, private / loopback / link-local IP, or name without a dot (intranet).
fn is_local_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(IpAddr::V4(v)) => v.is_private() || v.is_loopback() || v.is_link_local(),
        Ok(IpAddr::V6(v)) => {
            let s0 = v.segments()[0];
            v.is_loopback() || (s0 & 0xfe00) == 0xfc00 || (s0 & 0xffc0) == 0xfe80
        }
        Err(_) => !host.contains('.'),
    }
}

use urftp_core::pattern::wildcard_match;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"user:p@ss"), "dXNlcjpwQHNz");
    }

    #[test]
    fn bypass_rules() {
        let mut p = ProxyConfig::new(ProxyKind::Socks5, "proxy", 1080);
        assert!(p.bypasses("192.168.1.10") && p.bypasses("localhost") && p.bypasses("nas"));
        assert!(p.bypasses("10.0.0.1") && p.bypasses("::1") && p.bypasses("fd00::1"));
        assert!(!p.bypasses("ftp.example.com") && !p.bypasses("8.8.8.8"));
        p.bypass_local = false;
        assert!(!p.bypasses("192.168.1.10"));
        p.exceptions = parse_exceptions("220.67.*; *.lan ;");
        assert_eq!(p.exceptions, ["220.67.*", "*.lan"]);
        assert!(p.bypasses("220.67.3.4") && p.bypasses("box.lan") && !p.bypasses("1.2.3.4"));
    }
}
