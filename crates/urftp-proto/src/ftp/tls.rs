//! TLS for FTPS (RFC 4217): explicit (`AUTH TLS`) and implicit.
//!
//! The same [`ClientConfig`] and server name serve the control channel and every data
//! channel: TLS session reuse is therefore attempted automatically, which many servers
//! require (vsftpd `require_ssl_reuse`, glftpd, FileZilla Server...).

use std::sync::Arc;

use parking_lot::Mutex;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio_rustls::TlsConnector;

use super::{BoxStream, FtpError, Result};

/// Verification policy of the server certificate.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TlsVerify {
    /// Chain verified with the authorities of the Windows store, and server name checked.
    #[default]
    System,
    /// Accepts any certificate (self-signed, expired...). Encryption stays active, but the
    /// identity of the server is not guaranteed.
    AcceptAny,
    /// Accepts only the certificate whose SHA-256 fingerprint matches (hexadecimal, `:`
    /// separators and case ignored). Lets the user trust a self-signed certificate once
    /// and for all.
    Fingerprint(String),
}

/// TLS settings of a site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsOptions {
    pub verify: TlsVerify,
    /// Also encrypt the data channels (`PROT P`); `PROT C` otherwise.
    pub protect_data: bool,
    /// Try `AUTH SSL` when `AUTH TLS` is refused (old servers).
    pub auth_ssl_fallback: bool,
}

impl Default for TlsOptions {
    fn default() -> Self {
        Self { verify: TlsVerify::System, protect_data: true, auth_ssl_fallback: true }
    }
}

/// Security mode of an FTP connection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Security {
    /// Plain FTP.
    #[default]
    None,
    /// Explicit FTPS: plain connection then `AUTH TLS`.
    Explicit(TlsOptions),
    /// Implicit FTPS: TLS from the start of the connection (usually port 990).
    Implicit(TlsOptions),
}

impl Security {
    pub fn options(&self) -> Option<&TlsOptions> {
        match self {
            Security::None => None,
            Security::Explicit(o) | Security::Implicit(o) => Some(o),
        }
    }
}

/// SHA-256 fingerprint of a certificate in the `AB:CD:…` format.
pub fn fingerprint_sha256(der: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, der);
    d.as_ref().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

fn normalize_fingerprint(s: &str) -> String {
    s.chars().filter(char::is_ascii_hexdigit).collect::<String>().to_ascii_uppercase()
}

#[derive(Debug)]
struct Verifier {
    webpki: Option<Arc<WebPkiServerVerifier>>,
    pinned: Option<String>,
    provider: Arc<CryptoProvider>,
    /// Fingerprint of the last certificate presented (to offer trusting it after a failure).
    seen: Arc<Mutex<Option<String>>>,
}

impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint_sha256(end_entity);
        *self.seen.lock() = Some(fp.clone());
        if let Some(pinned) = &self.pinned {
            return if normalize_fingerprint(&fp) == *pinned {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General("certificate fingerprint differs from the trusted one".into()))
            };
        }
        match &self.webpki {
            Some(v) => v.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now),
            None => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

/// TLS context of a connection: shared configuration, server name, fingerprint seen.
pub(crate) struct TlsContext {
    connector: TlsConnector,
    server_name: ServerName<'static>,
    seen: Arc<Mutex<Option<String>>>,
}

impl TlsContext {
    pub(crate) fn new(host: &str, opts: &TlsOptions) -> Result<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let seen = Arc::new(Mutex::new(None));
        let (webpki, pinned) = match &opts.verify {
            TlsVerify::System => {
                let mut roots = RootCertStore::empty();
                let native = rustls_native_certs::load_native_certs();
                roots.add_parsable_certificates(native.certs);
                let v = WebPkiServerVerifier::builder_with_provider(Arc::new(roots), provider.clone())
                    .build()
                    .map_err(|e| FtpError::Tls(format!("certificate store unusable: {e}")))?;
                (Some(v), None)
            }
            TlsVerify::AcceptAny => (None, None),
            TlsVerify::Fingerprint(fp) => (None, Some(normalize_fingerprint(fp))),
        };
        let verifier = Verifier { webpki, pinned, provider: provider.clone(), seen: seen.clone() };
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| FtpError::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|_| FtpError::InvalidArgument(format!("invalid server name for TLS: {host:?}")))?;
        Ok(Self { connector: TlsConnector::from(Arc::new(config)), server_name, seen })
    }

    /// Client-side TLS handshake on an existing stream.
    pub(crate) async fn wrap(&self, stream: BoxStream) -> Result<BoxStream> {
        match self.connector.connect(self.server_name.clone(), stream).await {
            Ok(tls) => Ok(Box::new(tls)),
            Err(e) => Err(match self.seen.lock().clone() {
                Some(fingerprint) if is_cert_error(&e) => FtpError::Certificate { fingerprint, reason: e.to_string() },
                _ => FtpError::Tls(e.to_string()),
            }),
        }
    }

    /// Fingerprint of the certificate presented by the server during the last handshake.
    pub(crate) fn peer_fingerprint(&self) -> Option<String> {
        self.seen.lock().clone()
    }
}

fn is_cert_error(e: &std::io::Error) -> bool {
    e.get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|re| matches!(re, rustls::Error::InvalidCertificate(_) | rustls::Error::General(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_format_and_normalization() {
        let fp = fingerprint_sha256(b"abc");
        assert_eq!(fp.len(), 32 * 3 - 1);
        assert!(fp.starts_with("BA:78:16:BF"));
        assert_eq!(normalize_fingerprint("ba:78:16"), "BA7816");
    }
}
