//! Serveur Ident (RFC 1413).
//!
//! Beaucoup de serveurs FTP (glftpd notamment) interrogent le port 113 du client au moment
//! de la connexion. Le serveur répond avec le nom enregistré pour le site interrogé, ou avec
//! le nom global (`URUSHFTP` par défaut).
//!
//! Requête : `<port local du client FTP> , <port du serveur FTP>`
//! Réponse : `<même paire> : USERID : UNIX : <nom>`.
//!
//! L'option « seulement pendant la connexion » de la v2 revient à démarrer le serveur avant
//! la connexion et à le libérer (drop) une fois le login terminé.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use parking_lot::Mutex;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// Longueur maximale d'une requête (RFC 1413 : 1000 caractères).
const MAX_QUERY: u64 = 1000;
/// Délai maximal d'une connexion Ident.
const CONN_TIMEOUT: Duration = Duration::from_secs(30);

/// Réglages du serveur Ident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentConfig {
    /// Nom répondu quand le site n'a pas de nom propre.
    pub name: String,
    /// Système annoncé (« UNIX » sauf besoin particulier).
    pub system: String,
    /// Adresse d'écoute (port TCP 113 par défaut ; à changer derrière un NAT).
    pub bind: SocketAddr,
}

impl Default for IdentConfig {
    fn default() -> Self {
        Self {
            name: urftp_core::DEFAULT_IDENT_NAME.to_string(),
            system: "UNIX".to_string(),
            bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), urftp_core::DEFAULT_IDENT_PORT),
        }
    }
}

type Registry = Arc<Mutex<HashMap<(IpAddr, u16), String>>>;

/// Serveur Ident en cours d'exécution ; il s'arrête quand on le libère.
pub struct IdentServer {
    local: SocketAddr,
    registry: Registry,
    task: JoinHandle<()>,
}

/// Nom propre à un site, actif tant que cette valeur existe.
pub struct IdentRegistration {
    registry: Registry,
    key: (IpAddr, u16),
}

impl Drop for IdentRegistration {
    fn drop(&mut self) {
        self.registry.lock().remove(&self.key);
    }
}

impl IdentServer {
    pub async fn start(config: IdentConfig) -> std::io::Result<Self> {
        let listener = TcpListener::bind(config.bind).await?;
        let local = listener.local_addr()?;
        let registry: Registry = Arc::default();
        let reg = registry.clone();
        let config = Arc::new(config);
        let task = tokio::spawn(async move {
            while let Ok((stream, peer)) = listener.accept().await {
                let (reg, config) = (reg.clone(), config.clone());
                tokio::spawn(async move {
                    let _ = tokio::time::timeout(CONN_TIMEOUT, serve(stream, peer.ip(), reg, config)).await;
                });
            }
        });
        Ok(Self { local, registry, task })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Répond `name` aux requêtes venant de `server_ip` pour le port FTP `server_port`.
    /// À appeler avant la connexion : le serveur FTP interroge dès l'ouverture TCP.
    pub fn register(&self, server_ip: IpAddr, server_port: u16, name: impl Into<String>) -> IdentRegistration {
        let key = (canonical(server_ip), server_port);
        self.registry.lock().insert(key, sanitize(&name.into()));
        IdentRegistration { registry: self.registry.clone(), key }
    }
}

impl Drop for IdentServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(stream: TcpStream, peer: IpAddr, reg: Registry, config: Arc<IdentConfig>) -> std::io::Result<()> {
    let (rd, mut wr) = stream.into_split();
    let mut rd = BufReader::new(rd);
    loop {
        let mut line = Vec::new();
        let n = (&mut rd).take(MAX_QUERY).read_until(b'\n', &mut line).await?;
        if n == 0 || line.last() != Some(&b'\n') {
            return Ok(());
        }
        let query = String::from_utf8_lossy(&line);
        let resp = answer(query.trim_end_matches(['\r', '\n']), peer, &reg, &config);
        wr.write_all(resp.as_bytes()).await?;
        wr.write_all(b"\r\n").await?;
    }
}

/// Construit la réponse à une requête Ident.
fn answer(query: &str, peer: IpAddr, reg: &Registry, config: &IdentConfig) -> String {
    let ports: Option<(u16, u16)> = query.split_once(',').and_then(|(a, b)| {
        let a: u16 = a.trim().parse().ok()?;
        let b: u16 = b.trim().parse().ok()?;
        (a != 0 && b != 0).then_some((a, b))
    });
    let Some((local, remote)) = ports else {
        return format!("{} : ERROR : INVALID-PORT", query.trim());
    };
    let name = reg.lock().get(&(canonical(peer), remote)).cloned().unwrap_or_else(|| sanitize(&config.name));
    format!("{local} , {remote} : USERID : {} : {name}", sanitize(&config.system))
}

/// Une IPv4 vue en IPv6 (`::ffff:a.b.c.d`) est ramenée à IPv4.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

/// Retire ce qui casserait le format de réponse (`:`, `,`, fins de ligne).
fn sanitize(s: &str) -> String {
    let out: String = s.chars().filter(|c| !matches!(c, ':' | ',' | '\r' | '\n')).collect();
    let out = out.trim();
    if out.is_empty() { urftp_core::DEFAULT_IDENT_NAME.to_string() } else { out.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers() {
        let reg: Registry = Arc::default();
        let cfg = IdentConfig::default();
        let srv: IpAddr = "203.0.113.4".parse().unwrap();
        reg.lock().insert((srv, 21), "monident".into());
        assert_eq!(answer("6193, 21", srv, &reg, &cfg), "6193 , 21 : USERID : UNIX : monident");
        assert_eq!(answer("6193 , 2121", srv, &reg, &cfg), "6193 , 2121 : USERID : UNIX : URUSHFTP");
        let other: IpAddr = "198.51.100.1".parse().unwrap();
        assert_eq!(answer("1,21", other, &reg, &cfg), "1 , 21 : USERID : UNIX : URUSHFTP");
        assert_eq!(answer("abc", srv, &reg, &cfg), "abc : ERROR : INVALID-PORT");
        assert_eq!(answer("0 , 21", srv, &reg, &cfg), "0 , 21 : ERROR : INVALID-PORT");
        assert_eq!(answer("70000 , 21", srv, &reg, &cfg), "70000 , 21 : ERROR : INVALID-PORT");
    }

    #[test]
    fn sanitizing_and_mapped_addresses() {
        assert_eq!(sanitize("a:b,c\r\n"), "abc");
        assert_eq!(sanitize(" : "), "URUSHFTP");
        let mapped: IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert_eq!(canonical(mapped), "10.0.0.1".parse::<IpAddr>().unwrap());
    }
}
