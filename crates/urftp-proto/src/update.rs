//! Latest release published on GitHub: one HTTPS request to the public API, checked against
//! the certificates of Windows. The answer (JSON) is read by `urftp_core::update`.
//! Nothing else is sent than the request itself (program name and version).

use std::sync::Arc;
use std::time::Duration;

use rustls::client::WebPkiServerVerifier;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;

const HOST: &str = "api.github.com";
/// Largest answer read (the latest release is a few kilobytes).
const MAX_RESPONSE: usize = 1 << 20;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("no answer from {HOST} within {0} s")]
    Timeout(u64),
    #[error("connection to {HOST}: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS: {0}")]
    Tls(String),
    #[error("unexpected answer from {HOST}: {0}")]
    Http(String),
}

/// Body of `GET /repos/{repository}/releases/latest` (JSON).
pub async fn latest_release(repository: &str, timeout: Duration) -> Result<String, UpdateError> {
    tokio::time::timeout(timeout, request(repository)).await.map_err(|_| UpdateError::Timeout(timeout.as_secs()))?
}

async fn request(repository: &str) -> Result<String, UpdateError> {
    let provider = crate::ftp::crypto_provider();
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    let verifier = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .map_err(|e| UpdateError::Tls(format!("certificate store unusable: {e}")))?;
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| UpdateError::Tls(e.to_string()))?
        .with_webpki_verifier(verifier)
        .with_no_client_auth();
    let name = ServerName::try_from(HOST).map_err(|e| UpdateError::Tls(e.to_string()))?;
    let tcp = crate::net::connect(HOST, 443).await?;
    let mut tls = TlsConnector::from(Arc::new(config)).connect(name, tcp).await.map_err(|e| UpdateError::Tls(e.to_string()))?;
    let request = format!(
        "GET /repos/{repository}/releases/latest HTTP/1.1\r\nHost: {HOST}\r\nUser-Agent: UltraRushFTP/{}\r\n\
         Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\nConnection: close\r\n\r\n",
        env!("CARGO_PKG_VERSION")
    );
    tls.write_all(request.as_bytes()).await?;
    let mut raw = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        match tls.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.len() > MAX_RESPONSE {
                    return Err(UpdateError::Http("answer too large".into()));
                }
                if body_complete(&raw) {
                    break;
                }
            }
            // Servers often close without the TLS close message: the answer is judged on
            // what was received.
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !raw.is_empty() => break,
            Err(e) => return Err(e.into()),
        }
    }
    parse_response(&raw)
}

/// Header lines of an answer: lowercase name, value.
type Headers = Vec<(String, String)>;

/// Splits an HTTP answer: status code, headers, body.
fn split(raw: &[u8]) -> Option<(u16, Headers, &[u8])> {
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..end]).ok()?;
    let mut lines = head.split("\r\n");
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string())).collect();
    Some((status, headers, &raw[end + 4..]))
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn is_chunked(headers: &[(String, String)]) -> bool {
    header(headers, "transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked"))
}

/// The whole body arrived (by its length, or the last chunk).
fn body_complete(raw: &[u8]) -> bool {
    let Some((_, headers, body)) = split(raw) else { return false };
    if is_chunked(&headers) {
        return dechunk(body).is_some();
    }
    match header(&headers, "content-length").and_then(|v| v.parse::<usize>().ok()) {
        Some(len) => body.len() >= len,
        None => false,
    }
}

/// Body of a `Transfer-Encoding: chunked` answer; `None` while incomplete or malformed.
fn dechunk(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = body.windows(2).position(|w| w == b"\r\n")?;
        let size_text = std::str::from_utf8(&body[..line_end]).ok()?;
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Some(out);
        }
        if body.len() < size + 2 {
            return None;
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

fn parse_response(raw: &[u8]) -> Result<String, UpdateError> {
    let (status, headers, body) = split(raw).ok_or_else(|| UpdateError::Http("incomplete answer".into()))?;
    let body = if is_chunked(&headers) {
        dechunk(body).ok_or_else(|| UpdateError::Http("incomplete answer".into()))?
    } else {
        match header(&headers, "content-length").and_then(|v| v.parse::<usize>().ok()) {
            Some(len) if body.len() >= len => body[..len].to_vec(),
            Some(_) => return Err(UpdateError::Http("incomplete answer".into())),
            None => body.to_vec(),
        }
    };
    if status != 200 {
        return Err(UpdateError::Http(format!("HTTP {status}")));
    }
    String::from_utf8(body).map_err(|_| UpdateError::Http("answer is not UTF-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_with_a_length_or_in_chunks_are_read() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\n\r\n{\"tag\":\"v1.0\"}\n";
        assert!(body_complete(plain));
        assert_eq!(parse_response(plain).unwrap(), "{\"tag\":\"v1.0\"}\n");
        assert!(!body_complete(&plain[..plain.len() - 3]));

        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"a\":\r\n4;x=y\r\n1}\r\n\r\n0\r\n\r\n";
        assert!(body_complete(chunked));
        assert_eq!(parse_response(chunked).unwrap(), "{\"a\":1}\r\n");
        assert!(!body_complete(&chunked[..chunked.len() - 7]), "last chunk missing");
    }

    /// Real request to GitHub: `cargo test -p urftp-proto -- --ignored real_github`.
    #[tokio::test]
    #[ignore = "network: asks api.github.com"]
    async fn real_github_answer_has_a_tag() {
        let json = latest_release("werewolf004/UltraRushFTP", Duration::from_secs(20)).await.unwrap();
        assert!(json.contains("\"tag_name\""), "{}", &json[..json.len().min(300)]);
    }

    #[test]
    fn errors_and_truncated_answers_are_refused() {
        let missing = b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\n\r\n{}";
        assert!(matches!(parse_response(missing), Err(UpdateError::Http(m)) if m == "HTTP 404"));
        let short = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n{}";
        assert!(parse_response(short).is_err());
        assert!(parse_response(b"garbage").is_err());
    }
}
