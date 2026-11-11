//! UPnP port mapping on the router (Internet Gateway Device, `WANIPConnection` or
//! `WANPPPConnection` service), for the active mode and the Ident server behind a NAT.
//!
//! Written for the project, without an HTTP or XML library: SSDP discovery (`M-SEARCH`
//! over UDP), the device description fetched over HTTP/1.1, then three SOAP actions —
//! `GetExternalIPAddress`, `AddPortMapping` and `DeletePortMapping`.
//!
//! The router found is remembered by [`Upnp`] until [`Upnp::reset`] ("Restart UPnP
//! service"). A [`Mapping`] is renewed before its lease ends and removed when dropped.

use std::fmt::Write as _;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::task::JoinHandle;

/// SSDP multicast address and port.
const SSDP: SocketAddr = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 1900));
/// Device types searched, newest first.
const SEARCH_TARGETS: [&str; 2] =
    ["urn:schemas-upnp-org:device:InternetGatewayDevice:2", "urn:schemas-upnp-org:device:InternetGatewayDevice:1"];
/// Services able to map ports.
const SERVICES: [&str; 2] = ["WANIPConnection", "WANPPPConnection"];
/// Lease asked for a mapping; renewed at half of it.
const LEASE_SECS: u32 = 3600;
/// Largest HTTP answer read (a device description is a few KiB).
const MAX_HTTP: usize = 256 * 1024;
/// Other external ports tried when the router already maps the wanted one.
const PORT_ATTEMPTS: u16 = 8;

/// `ConflictInMappingEntry`: the external port is already mapped to another host.
const ERR_CONFLICT: u16 = 718;
/// `OnlyPermanentLeasesSupported`: the router refuses a lease duration.
const ERR_PERMANENT_ONLY: u16 = 725;

#[derive(Debug, thiserror::Error)]
pub enum UpnpError {
    #[error("no UPnP router answered on the local network")]
    NoGateway,
    #[error("network error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timed out")]
    Timeout,
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("the router refused {action}: {code} {description}")]
    Refused { action: &'static str, code: u16, description: String },
    #[error("invalid answer from the router: {0}")]
    Invalid(String),
    #[error("UPnP maps IPv4 addresses only")]
    NotIpv4,
}

pub type Result<T> = std::result::Result<T, UpnpError>;

/// A router found on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Gateway {
    /// Address and path of the control URL of the service.
    host: SocketAddr,
    path: String,
    /// Full service type (`urn:schemas-upnp-org:service:WANIPConnection:1`).
    service: String,
}

/// UPnP client shared by the connections (see [`shared`]).
#[derive(Debug)]
pub struct Upnp {
    discovery: SocketAddr,
    timeout: Duration,
    gateway: tokio::sync::Mutex<Option<Gateway>>,
}

/// The process-wide client, used when the option is on.
pub fn shared() -> Arc<Upnp> {
    static SHARED: OnceLock<Arc<Upnp>> = OnceLock::new();
    SHARED.get_or_init(|| Arc::new(Upnp::new())).clone()
}

impl Default for Upnp {
    fn default() -> Self {
        Self::new()
    }
}

impl Upnp {
    pub fn new() -> Self {
        Self::with_discovery(SSDP, Duration::from_secs(3))
    }

    /// Searches routers at `discovery` (the SSDP multicast address normally; a local
    /// address in tests), waiting `timeout` for each step.
    pub fn with_discovery(discovery: SocketAddr, timeout: Duration) -> Self {
        Self { discovery, timeout, gateway: tokio::sync::Mutex::new(None) }
    }

    /// Forgets the router found: the next request searches it again.
    pub async fn reset(&self) {
        *self.gateway.lock().await = None;
    }

    /// The router, searched the first time.
    async fn gateway(&self) -> Result<Gateway> {
        let mut found = self.gateway.lock().await;
        if let Some(g) = found.as_ref() {
            return Ok(g.clone());
        }
        let g = self.discover().await?;
        *found = Some(g.clone());
        Ok(g)
    }

    async fn discover(&self) -> Result<Gateway> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
        for target in SEARCH_TARGETS {
            let msg =
                format!("M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nST: {target}\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\n\r\n");
            socket.send_to(msg.as_bytes(), self.discovery).await?;
        }
        let mut buf = [0u8; 2048];
        let deadline = tokio::time::Instant::now() + self.timeout;
        loop {
            let (n, _) = match tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await {
                Ok(r) => r?,
                Err(_) => return Err(UpnpError::NoGateway),
            };
            let answer = String::from_utf8_lossy(&buf[..n]);
            let Some(location) = header(&answer, "location") else { continue };
            // A device without a mapping service (a printer, a TV...): keep listening.
            if let Ok(g) = self.describe(location).await {
                return Ok(g);
            }
        }
    }

    /// Reads the device description at `location` and finds the mapping service.
    async fn describe(&self, location: &str) -> Result<Gateway> {
        let (host, path) = parse_url(location)?;
        let (status, body) = self.http(host, &format!("GET {path} HTTP/1.1\r\nHOST: {host}\r\nConnection: close\r\n\r\n")).await?;
        if status != 200 {
            return Err(UpnpError::Http(format!("{status} for the description {location}")));
        }
        for block in body.split("<service>").skip(1) {
            let block = block.split("</service>").next().unwrap_or_default();
            let (Some(service), Some(control)) = (tag(block, "serviceType"), tag(block, "controlURL")) else { continue };
            if SERVICES.iter().any(|s| service.contains(s)) {
                let (host, path) = if control.starts_with("http://") {
                    parse_url(&control)?
                } else if control.starts_with('/') {
                    (host, control)
                } else {
                    (host, format!("/{control}"))
                };
                return Ok(Gateway { host, path, service });
            }
        }
        Err(UpnpError::Invalid(format!("no WANIPConnection service in {location}")))
    }

    /// Public address of the router.
    pub async fn external_ip(&self) -> Result<Ipv4Addr> {
        let g = self.gateway().await?;
        let body = self.soap(&g, "GetExternalIPAddress", &[]).await?;
        let ip = tag(&body, "NewExternalIPAddress").unwrap_or_default();
        ip.trim().parse().map_err(|_| UpnpError::Invalid(format!("external address \"{ip}\"")))
    }

    /// Address of this computer on the router's network (the one the router reaches).
    pub async fn local_ip(&self) -> Result<Ipv4Addr> {
        let g = self.gateway().await?;
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await?;
        socket.connect(g.host).await?;
        ipv4(socket.local_addr()?.ip())
    }

    /// Maps a TCP port of the router to `local`: the same port number when free, else one
    /// of the next ones. The mapping lasts as long as the returned value.
    pub async fn map(self: &Arc<Self>, local: SocketAddrV4, description: &str) -> Result<Mapping> {
        self.map_from(local.port(), PORT_ATTEMPTS, local, description).await
    }

    /// Maps exactly the port `external` of the router to `local` (Ident: servers ask on 113).
    pub async fn map_exact(self: &Arc<Self>, external: u16, local: SocketAddrV4, description: &str) -> Result<Mapping> {
        self.map_from(external, 0, local, description).await
    }

    async fn map_from(self: &Arc<Self>, mut external: u16, tries: u16, local: SocketAddrV4, description: &str) -> Result<Mapping> {
        let g = self.gateway().await?;
        let mut attempts = 0;
        loop {
            match self.add(&g, external, local, description).await {
                Ok(lease) => {
                    let renew = (lease > 0).then(|| {
                        let (upnp, g, description) = (self.clone(), g.clone(), description.to_owned());
                        tokio::spawn(async move {
                            loop {
                                tokio::time::sleep(Duration::from_secs(u64::from(lease / 2).max(1))).await;
                                let _ = upnp.add(&g, external, local, &description).await;
                            }
                        })
                    });
                    let runtime = tokio::runtime::Handle::current();
                    return Ok(Mapping { upnp: self.clone(), gateway: g, external, local, renew, runtime });
                }
                Err(UpnpError::Refused { code: ERR_CONFLICT, .. }) if attempts < tries => {
                    attempts += 1;
                    external = external.checked_add(1).unwrap_or(1024);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// `AddPortMapping`; answers the lease obtained (0: permanent).
    async fn add(&self, g: &Gateway, external: u16, local: SocketAddrV4, description: &str) -> Result<u32> {
        let mut lease = LEASE_SECS;
        loop {
            let args = [
                ("NewRemoteHost", String::new()),
                ("NewExternalPort", external.to_string()),
                ("NewProtocol", "TCP".to_owned()),
                ("NewInternalPort", local.port().to_string()),
                ("NewInternalClient", local.ip().to_string()),
                ("NewEnabled", "1".to_owned()),
                ("NewPortMappingDescription", description.to_owned()),
                ("NewLeaseDuration", lease.to_string()),
            ];
            match self.soap(g, "AddPortMapping", &args).await {
                Ok(_) => return Ok(lease),
                Err(UpnpError::Refused { code: ERR_PERMANENT_ONLY, .. }) if lease != 0 => lease = 0,
                Err(e) => return Err(e),
            }
        }
    }

    async fn delete(&self, g: &Gateway, external: u16) -> Result<()> {
        let args = [("NewRemoteHost", String::new()), ("NewExternalPort", external.to_string()), ("NewProtocol", "TCP".to_owned())];
        self.soap(g, "DeletePortMapping", &args).await.map(|_| ())
    }

    /// Sends a SOAP action; answers the body of a successful reply.
    async fn soap(&self, g: &Gateway, action: &'static str, args: &[(&str, String)]) -> Result<String> {
        let mut body = format!(
            "<?xml version=\"1.0\"?>\r\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" \
             s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{}\">",
            g.service
        );
        for (name, value) in args {
            let _ = write!(body, "<{name}>{}</{name}>", escape(value));
        }
        let _ = write!(body, "</u:{action}></s:Body></s:Envelope>\r\n");
        let request = format!(
            "POST {} HTTP/1.1\r\nHOST: {}\r\nContent-Type: text/xml; charset=\"utf-8\"\r\nSOAPAction: \"{}#{action}\"\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            g.path,
            g.host,
            g.service,
            body.len()
        );
        let (status, answer) = self.http(g.host, &request).await?;
        if status == 200 {
            return Ok(answer);
        }
        match tag(&answer, "errorCode").and_then(|c| c.trim().parse().ok()) {
            Some(code) => Err(UpnpError::Refused { action, code, description: tag(&answer, "errorDescription").unwrap_or_default() }),
            None => Err(UpnpError::Http(format!("{status} for {action}"))),
        }
    }

    /// One HTTP/1.1 exchange: answers the status code and the body.
    async fn http(&self, host: SocketAddr, request: &str) -> Result<(u16, String)> {
        let exchange = async {
            let mut stream = TcpStream::connect(host).await?;
            stream.write_all(request.as_bytes()).await?;
            let mut raw = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = stream.read(&mut chunk).await?;
                if n == 0 {
                    break;
                }
                raw.extend_from_slice(&chunk[..n]);
                if raw.len() > MAX_HTTP {
                    return Err(UpnpError::Http("answer too long".into()));
                }
                if complete(&raw) {
                    break;
                }
            }
            parse_response(&raw)
        };
        tokio::time::timeout(self.timeout, exchange).await.map_err(|_| UpnpError::Timeout)?
    }
}

/// A port of the router mapped to this computer; removed when dropped.
#[derive(Debug)]
pub struct Mapping {
    upnp: Arc<Upnp>,
    gateway: Gateway,
    external: u16,
    local: SocketAddrV4,
    renew: Option<JoinHandle<()>>,
    /// Runtime that removes the mapping, also when dropped outside of it (interface thread).
    runtime: tokio::runtime::Handle,
}

impl Mapping {
    /// Port open on the router.
    pub fn external_port(&self) -> u16 {
        self.external
    }

    /// Address of this computer the port leads to.
    pub fn local(&self) -> SocketAddrV4 {
        self.local
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        if let Some(task) = self.renew.take() {
            task.abort();
        }
        // Removed in the background; if the runtime is gone (end of the program) the lease ends it.
        let (upnp, g, external) = (self.upnp.clone(), self.gateway.clone(), self.external);
        self.runtime.spawn(async move {
            let _ = upnp.delete(&g, external).await;
        });
    }
}

/// Local IPv4 address of `addr`, for a mapping.
pub fn ipv4(addr: IpAddr) -> Result<Ipv4Addr> {
    match addr {
        IpAddr::V4(v4) => Ok(v4),
        IpAddr::V6(v6) => v6.to_ipv4_mapped().ok_or(UpnpError::NotIpv4),
    }
}

/// Value of an HTTP header (case-insensitive name).
fn header<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// `http://host:port/path` → (address, path). The host must be an IP address, as routers give.
fn parse_url(url: &str) -> Result<(SocketAddr, String)> {
    let rest = url.strip_prefix("http://").ok_or_else(|| UpnpError::Invalid(format!("URL {url}")))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_owned()),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().map_err(|_| UpnpError::Invalid(format!("URL {url}")))?),
        None => (authority, 80),
    };
    let ip: Ipv4Addr = host.parse().map_err(|_| UpnpError::Invalid(format!("URL {url}")))?;
    Ok((SocketAddr::from((ip, port)), path))
}

/// Text of the first element `name` (with or without a namespace prefix), entities decoded.
fn tag(xml: &str, name: &str) -> Option<String> {
    let close = format!("{name}>");
    let mut from = 0;
    while let Some(i) = xml[from..].find(&close).map(|i| i + from) {
        from = i + close.len();
        let before = xml[..i].rfind('<')?;
        let prefix = &xml[before + 1..i];
        if prefix.starts_with('/') || prefix.contains(['>', ' ']) || !(prefix.is_empty() || prefix.ends_with(':')) {
            continue;
        }
        let rest = &xml[from..];
        let end = rest.find('<').unwrap_or(rest.len());
        return Some(unescape(rest[..end].trim()));
    }
    None
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The whole answer is there (`Content-Length` reached, or last chunk read).
fn complete(raw: &[u8]) -> bool {
    let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else { return false };
    let head = String::from_utf8_lossy(&raw[..end]);
    let body = &raw[end + 4..];
    if let Some(len) = header(&head, "content-length").and_then(|v| v.parse::<usize>().ok()) {
        return body.len() >= len;
    }
    header(&head, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) && body.ends_with(b"0\r\n\r\n")
}

fn parse_response(raw: &[u8]) -> Result<(u16, String)> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| UpnpError::Http("truncated answer".into()))?;
    let head = String::from_utf8_lossy(&raw[..end]);
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| UpnpError::Http(format!("status line \"{}\"", head.lines().next().unwrap_or_default())))?;
    let mut body = raw[end + 4..].to_vec();
    if header(&head, "transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        body = dechunk(&body)?;
    } else if let Some(len) = header(&head, "content-length").and_then(|v| v.parse::<usize>().ok()) {
        body.truncate(len);
    }
    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

fn dechunk(mut data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = data.windows(2).position(|w| w == b"\r\n").ok_or_else(|| UpnpError::Http("bad chunk".into()))?;
        let size_text = String::from_utf8_lossy(&data[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or_default().trim(), 16)
            .map_err(|_| UpnpError::Http("bad chunk size".into()))?;
        data = &data[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        let chunk = data.get(..size).ok_or_else(|| UpnpError::Http("truncated chunk".into()))?;
        out.extend_from_slice(chunk);
        data = data.get(size + 2..).unwrap_or_default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use tokio::net::TcpListener;

    /// A fake router: answers SSDP on a local UDP port, serves its description and the
    /// SOAP actions, and records the actions received.
    struct FakeRouter {
        ssdp: SocketAddr,
        actions: Arc<Mutex<Vec<String>>>,
        /// External ports already mapped to another host (answered 718).
        taken: Arc<Mutex<Vec<u16>>>,
    }

    impl FakeRouter {
        async fn start(chunked: bool) -> Self {
            let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let http_addr = http.local_addr().unwrap();
            let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            let ssdp = udp.local_addr().unwrap();
            let actions = Arc::new(Mutex::new(Vec::new()));
            let taken = Arc::new(Mutex::new(Vec::new()));
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok((_, from)) = udp.recv_from(&mut buf).await {
                    // First a device without mapping service, then the router.
                    let other = format!("HTTP/1.1 200 OK\r\nST: upnp:rootdevice\r\nLOCATION: http://{http_addr}/none.xml\r\n\r\n");
                    let _ = udp.send_to(other.as_bytes(), from).await;
                    let answer =
                        format!("HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=120\r\nLocation: http://{http_addr}/rootDesc.xml\r\n\r\n");
                    let _ = udp.send_to(answer.as_bytes(), from).await;
                }
            });
            let (log, ports) = (actions.clone(), taken.clone());
            tokio::spawn(async move {
                while let Ok((mut s, _)) = http.accept().await {
                    let (log, ports) = (log.clone(), ports.clone());
                    tokio::spawn(async move {
                        let mut raw = Vec::new();
                        let mut buf = [0u8; 4096];
                        while !complete_request(&raw) {
                            let n = s.read(&mut buf).await.unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            raw.extend_from_slice(&buf[..n]);
                        }
                        let req = String::from_utf8_lossy(&raw).into_owned();
                        let (status, body) = answer(&req, &log, &ports);
                        let reply = if chunked {
                            let (a, b) = body.split_at(body.len() / 2);
                            format!(
                                "HTTP/1.1 {status}\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{a}\r\n{:x}\r\n{b}\r\n0\r\n\r\n",
                                a.len(),
                                b.len()
                            )
                        } else {
                            format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}", body.len())
                        };
                        let _ = s.write_all(reply.as_bytes()).await;
                    });
                }
            });
            Self { ssdp, actions, taken }
        }

        fn client(&self) -> Arc<Upnp> {
            Arc::new(Upnp::with_discovery(self.ssdp, Duration::from_secs(2)))
        }
    }

    fn complete_request(raw: &[u8]) -> bool {
        let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else { return false };
        let head = String::from_utf8_lossy(&raw[..end]);
        let len = header(&head, "content-length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        raw.len() >= end + 4 + len
    }

    const SERVICE: &str = "urn:schemas-upnp-org:service:WANIPConnection:1";

    fn answer(req: &str, log: &Mutex<Vec<String>>, taken: &Mutex<Vec<u16>>) -> (&'static str, String) {
        if req.starts_with("GET /none.xml") {
            return ("404 Not Found", String::new());
        }
        if req.starts_with("GET /rootDesc.xml") {
            return (
                "200 OK",
                format!(
                    "<?xml version=\"1.0\"?><root><device><serviceList>\
                     <service><serviceType>urn:schemas-upnp-org:service:Layer3Forwarding:1</serviceType><controlURL>/l3f</controlURL></service>\
                     <service><serviceType>{SERVICE}</serviceType><controlURL>/ctl/IPConn</controlURL></service>\
                     </serviceList></device></root>"
                ),
            );
        }
        let action = header(req, "soapaction").unwrap_or_default().trim_matches('"').to_owned();
        assert!(req.starts_with("POST /ctl/IPConn "), "{req}");
        assert!(action.starts_with(SERVICE), "{action}");
        let name = action.rsplit('#').next().unwrap_or_default().to_owned();
        let port = tag(req, "NewExternalPort").unwrap_or_default();
        log.lock().push(format!("{name} {port} {}", tag(req, "NewInternalClient").unwrap_or_default()).trim().to_owned());
        let ok = |inner: &str| {
            format!("<s:Envelope><s:Body><u:{name}Response xmlns:u=\"{SERVICE}\">{inner}</u:{name}Response></s:Body></s:Envelope>")
        };
        match name.as_str() {
            "GetExternalIPAddress" => ("200 OK", ok("<NewExternalIPAddress>203.0.113.7</NewExternalIPAddress>")),
            "AddPortMapping" if taken.lock().iter().any(|p| p.to_string() == port) => (
                "500 Internal Server Error",
                "<s:Envelope><s:Body><s:Fault><detail><UPnPError><errorCode>718</errorCode>\
                 <errorDescription>ConflictInMappingEntry</errorDescription></UPnPError></detail></s:Fault></s:Body></s:Envelope>"
                    .into(),
            ),
            _ => ("200 OK", ok("")),
        }
    }

    #[tokio::test]
    async fn finds_the_router_and_its_public_address() {
        for chunked in [false, true] {
            let router = FakeRouter::start(chunked).await;
            let upnp = router.client();
            assert_eq!(upnp.external_ip().await.unwrap(), Ipv4Addr::new(203, 0, 113, 7));
            assert_eq!(router.actions.lock().as_slice(), ["GetExternalIPAddress"]);
        }
    }

    #[tokio::test]
    async fn maps_a_port_and_removes_it_when_dropped() {
        let router = FakeRouter::start(false).await;
        let upnp = router.client();
        let local = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 20), 50_000);
        let mapping = upnp.map(local, "UltraRushFTP").await.unwrap();
        assert_eq!((mapping.external_port(), mapping.local()), (50_000, local));
        drop(mapping);
        for _ in 0..50 {
            if router.actions.lock().len() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(router.actions.lock().as_slice(), ["AddPortMapping 50000 192.168.1.20", "DeletePortMapping 50000"]);
    }

    #[tokio::test]
    async fn a_port_taken_on_the_router_moves_to_the_next_one() {
        let router = FakeRouter::start(false).await;
        router.taken.lock().extend([113, 114]);
        let upnp = router.client();
        let mapping = upnp.map(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 5), 113), "Ident").await.unwrap();
        assert_eq!(mapping.external_port(), 115);
        assert_eq!(router.actions.lock().len(), 3);
    }

    #[tokio::test]
    async fn an_exact_port_is_not_moved() {
        let router = FakeRouter::start(false).await;
        router.taken.lock().push(113);
        let upnp = router.client();
        let local = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1113);
        assert!(matches!(upnp.map_exact(113, local, "Ident").await, Err(UpnpError::Refused { code: 718, .. })));
        router.taken.lock().clear();
        let mapping = upnp.map_exact(113, local, "Ident").await.unwrap();
        assert_eq!((mapping.external_port(), mapping.local().port()), (113, 1113));
        assert_eq!(upnp.local_ip().await.unwrap(), Ipv4Addr::LOCALHOST);
    }

    #[tokio::test]
    async fn the_router_is_searched_once_until_reset() {
        let router = FakeRouter::start(false).await;
        let upnp = router.client();
        upnp.external_ip().await.unwrap();
        let first = upnp.gateway.lock().await.clone();
        assert!(first.is_some());
        upnp.reset().await;
        assert!(upnp.gateway.lock().await.is_none());
        upnp.external_ip().await.unwrap();
        assert_eq!(*upnp.gateway.lock().await, first);
    }

    #[tokio::test]
    async fn no_router_answers() {
        let silent = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let upnp = Upnp::with_discovery(silent.local_addr().unwrap(), Duration::from_millis(200));
        assert!(matches!(upnp.external_ip().await, Err(UpnpError::NoGateway)));
    }

    #[test]
    fn xml_and_urls() {
        assert_eq!(tag("<a><u:Value>1 &amp; 2</u:Value></a>", "Value").as_deref(), Some("1 & 2"));
        assert_eq!(tag("<NewX>a</NewX><X>b</X>", "X").as_deref(), Some("b"));
        assert_eq!(tag("<X attr=\"1\">c</X>", "X"), None, "attributes are not expected in these answers");
        assert_eq!(
            parse_url("http://192.168.1.1:5000/rootDesc.xml").unwrap(),
            ("192.168.1.1:5000".parse().unwrap(), "/rootDesc.xml".into())
        );
        assert_eq!(parse_url("http://10.0.0.1").unwrap(), ("10.0.0.1:80".parse().unwrap(), "/".into()));
        assert!(parse_url("https://10.0.0.1/").is_err());
        assert_eq!(ipv4("::ffff:192.168.0.2".parse().unwrap()).unwrap(), Ipv4Addr::new(192, 168, 0, 2));
        assert!(ipv4("2001:db8::1".parse().unwrap()).is_err());
    }
}
