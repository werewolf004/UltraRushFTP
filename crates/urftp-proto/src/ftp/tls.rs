//! TLS pour FTPS (RFC 4217) : explicite (`AUTH TLS`) et implicite.
//!
//! Le même [`ClientConfig`] et le même nom de serveur servent au canal de contrôle et à
//! tous les canaux de données : la reprise de session TLS est donc tentée automatiquement,
//! ce qu'exigent beaucoup de serveurs (vsftpd `require_ssl_reuse`, glftpd, FileZilla Server…).

use std::sync::{Arc, Mutex};

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio_rustls::TlsConnector;

use super::{BoxStream, FtpError, Result};

/// Politique de vérification du certificat du serveur.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TlsVerify {
    /// Chaîne vérifiée avec les autorités du magasin Windows, et nom du serveur contrôlé.
    #[default]
    System,
    /// Accepte n'importe quel certificat (auto-signé, expiré…). Le chiffrement reste actif,
    /// mais l'identité du serveur n'est pas garantie.
    AcceptAny,
    /// Accepte uniquement le certificat dont l'empreinte SHA-256 correspond
    /// (hexadécimal, séparateurs `:` et casse ignorés). Permet d'approuver un certificat
    /// auto-signé une fois pour toutes.
    Fingerprint(String),
}

/// Réglages TLS d'un site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsOptions {
    pub verify: TlsVerify,
    /// Chiffrer aussi les canaux de données (`PROT P`) ; sinon `PROT C`.
    pub protect_data: bool,
    /// Essayer `AUTH SSL` si `AUTH TLS` est refusé (anciens serveurs).
    pub auth_ssl_fallback: bool,
}

impl Default for TlsOptions {
    fn default() -> Self {
        Self { verify: TlsVerify::System, protect_data: true, auth_ssl_fallback: true }
    }
}

/// Mode de sécurité d'une connexion FTP.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Security {
    /// FTP en clair.
    #[default]
    None,
    /// FTPS explicite : connexion en clair puis `AUTH TLS`.
    Explicit(TlsOptions),
    /// FTPS implicite : TLS dès l'ouverture de la connexion (port 990 en général).
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

/// Empreinte SHA-256 d'un certificat au format `AB:CD:…`.
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
    /// Empreinte du dernier certificat présenté (pour proposer de l'approuver en cas d'échec).
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
        *self.seen.lock().unwrap() = Some(fp.clone());
        if let Some(pinned) = &self.pinned {
            return if normalize_fingerprint(&fp) == *pinned {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General("empreinte du certificat différente de celle approuvée".into()))
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

/// Contexte TLS d'une connexion : configuration partagée, nom du serveur, empreinte vue.
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
                    .map_err(|e| FtpError::Tls(format!("magasin de certificats inutilisable : {e}")))?;
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
            .map_err(|_| FtpError::InvalidArgument(format!("nom de serveur invalide pour TLS : {host:?}")))?;
        Ok(Self { connector: TlsConnector::from(Arc::new(config)), server_name, seen })
    }

    /// Négociation TLS côté client sur un flux existant.
    pub(crate) async fn wrap(&self, stream: BoxStream) -> Result<BoxStream> {
        match self.connector.connect(self.server_name.clone(), stream).await {
            Ok(tls) => Ok(Box::new(tls)),
            Err(e) => Err(match self.seen.lock().unwrap().clone() {
                Some(fingerprint) if is_cert_error(&e) => FtpError::Certificate { fingerprint, reason: e.to_string() },
                _ => FtpError::Tls(e.to_string()),
            }),
        }
    }

    /// Empreinte du certificat présenté par le serveur lors de la dernière négociation.
    pub(crate) fn peer_fingerprint(&self) -> Option<String> {
        self.seen.lock().unwrap().clone()
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
