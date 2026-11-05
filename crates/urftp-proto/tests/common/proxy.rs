//! Proxys de test (SOCKS4/4A/5, HTTP CONNECT) qui relaient vers la cible demandée.

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream};
use urftp_proto::proxy::ProxyKind;

pub struct MockProxy {
    pub addr: SocketAddr,
    /// Cibles demandées (`hôte:port`), dans l'ordre.
    pub targets: Arc<Mutex<Vec<String>>>,
}

impl MockProxy {
    /// `auth` : identifiants exigés (SOCKS5 et HTTP).
    pub async fn start(kind: ProxyKind, auth: Option<(&str, &str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let targets = Arc::new(Mutex::new(Vec::new()));
        let t2 = targets.clone();
        let auth = auth.map(|(u, p)| (u.to_string(), p.to_string()));
        tokio::spawn(async move {
            while let Ok((s, _)) = listener.accept().await {
                let (targets, auth) = (t2.clone(), auth.clone());
                tokio::spawn(async move {
                    let _ = handle(kind, s, targets, auth).await;
                });
            }
        });
        Self { addr, targets }
    }

    pub fn targets(&self) -> Vec<String> {
        self.targets.lock().unwrap().clone()
    }
}

async fn read_cstr(s: &mut TcpStream) -> std::io::Result<String> {
    let mut out = Vec::new();
    loop {
        let b = s.read_u8().await?;
        if b == 0 {
            return Ok(String::from_utf8_lossy(&out).into_owned());
        }
        out.push(b);
    }
}

async fn handle(
    kind: ProxyKind,
    mut s: TcpStream,
    targets: Arc<Mutex<Vec<String>>>,
    auth: Option<(String, String)>,
) -> std::io::Result<()> {
    let (host, port) = match kind {
        ProxyKind::Socks4 | ProxyKind::Socks4a => {
            let mut h = [0u8; 8];
            s.read_exact(&mut h).await?;
            let port = u16::from_be_bytes([h[2], h[3]]);
            let _user = read_cstr(&mut s).await?;
            let host = if h[4..7] == [0, 0, 0] && h[7] != 0 {
                read_cstr(&mut s).await?
            } else {
                IpAddr::from([h[4], h[5], h[6], h[7]]).to_string()
            };
            (host, port)
        }
        ProxyKind::Socks5 => {
            let ver = s.read_u8().await?;
            let n = s.read_u8().await?;
            let mut methods = vec![0u8; n as usize];
            s.read_exact(&mut methods).await?;
            assert_eq!(ver, 5);
            if let Some((user, pass)) = &auth {
                if !methods.contains(&2) {
                    s.write_all(&[5, 0xFF]).await?;
                    return Ok(());
                }
                s.write_all(&[5, 2]).await?;
                let _v = s.read_u8().await?;
                let ul = s.read_u8().await? as usize;
                let mut u = vec![0u8; ul];
                s.read_exact(&mut u).await?;
                let pl = s.read_u8().await? as usize;
                let mut p = vec![0u8; pl];
                s.read_exact(&mut p).await?;
                if u != user.as_bytes() || p != pass.as_bytes() {
                    s.write_all(&[1, 1]).await?;
                    return Ok(());
                }
                s.write_all(&[1, 0]).await?;
            } else {
                s.write_all(&[5, 0]).await?;
            }
            let mut h = [0u8; 4];
            s.read_exact(&mut h).await?;
            let host = match h[3] {
                1 => {
                    let mut a = [0u8; 4];
                    s.read_exact(&mut a).await?;
                    IpAddr::from(a).to_string()
                }
                4 => {
                    let mut a = [0u8; 16];
                    s.read_exact(&mut a).await?;
                    IpAddr::from(a).to_string()
                }
                _ => {
                    let l = s.read_u8().await? as usize;
                    let mut d = vec![0u8; l];
                    s.read_exact(&mut d).await?;
                    String::from_utf8_lossy(&d).into_owned()
                }
            };
            (host, s.read_u16().await?)
        }
        ProxyKind::HttpTunnel => {
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(s.read_u8().await?);
            }
            let text = String::from_utf8_lossy(&head).into_owned();
            if let Some((user, pass)) = &auth {
                let expected = format!("Proxy-Authorization: Basic {}", b64(&format!("{user}:{pass}")));
                if !text.lines().any(|l| l == expected) {
                    s.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n").await?;
                    return Ok(());
                }
            }
            let target = text.lines().next().unwrap().split_whitespace().nth(1).unwrap().to_string();
            let (h, p) = target.rsplit_once(':').unwrap();
            (h.trim_matches(['[', ']']).to_string(), p.parse().unwrap())
        }
    };
    targets.lock().unwrap().push(format!("{host}:{port}"));
    // IPv4 d'abord : sous Windows, un refus sur ::1 coûte environ 2 s.
    let mut addrs: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port)).await?.collect();
    addrs.sort_by_key(|a| a.is_ipv6());
    let mut upstream = None;
    for a in addrs {
        if let Ok(u) = TcpStream::connect(a).await {
            upstream = Some(u);
            break;
        }
    }
    let mut upstream = match upstream {
        Some(u) => u,
        None => {
            match kind {
                ProxyKind::Socks4 | ProxyKind::Socks4a => s.write_all(&[0, 0x5B, 0, 0, 0, 0, 0, 0]).await?,
                ProxyKind::Socks5 => s.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?,
                ProxyKind::HttpTunnel => s.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await?,
            }
            return Ok(());
        }
    };
    match kind {
        ProxyKind::Socks4 | ProxyKind::Socks4a => s.write_all(&[0, 0x5A, 0, 0, 0, 0, 0, 0]).await?,
        ProxyKind::Socks5 => s.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await?,
        ProxyKind::HttpTunnel => s.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?,
    }
    let _ = copy_bidirectional(&mut s, &mut upstream).await;
    Ok(())
}

fn b64(s: &str) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let b = s.as_bytes();
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16) | (u32::from(*c.get(1).unwrap_or(&0)) << 8) | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            out.push(if i <= c.len() { T[((n >> (18 - 6 * i)) & 63) as usize] as char } else { '=' });
        }
    }
    out
}
