//! TLS for FTPS (RFC 4217): explicit (`AUTH TLS`) and implicit.
//!
//! The same [`ClientConfig`] and server name serve the control channel and every data
//! channel: TLS session reuse is therefore attempted automatically, which many servers
//! require (vsftpd `require_ssl_reuse`, glftpd, FileZilla Server...).

use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{
    AlgorithmIdentifier, CertificateDer, InvalidSignature, ServerName, SignatureVerificationAlgorithm, UnixTime, alg_id,
};
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
    /// Also encrypt the data channels of the transfers (`PROT P`); `PROT C` otherwise.
    pub protect_data: bool,
    /// Also encrypt the data channels of the listings.
    pub protect_listings: bool,
    /// Try `AUTH SSL` when `AUTH TLS` is refused (old servers).
    pub auth_ssl_fallback: bool,
}

impl Default for TlsOptions {
    fn default() -> Self {
        Self { verify: TlsVerify::System, protect_data: true, protect_listings: true, auth_ssl_fallback: true }
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

/// Hash of an ECDSA P-521 signature.
#[derive(Debug, Clone, Copy)]
enum P521Hash {
    Sha256,
    Sha384,
    Sha512,
}

/// ECDSA P-521 verification, which *ring* lacks: some servers (glftpd and others built
/// on a recent OpenSSL) present a P-521 certificate and abort the handshake when the
/// client does not offer `ecdsa_secp521r1_sha512`.
#[derive(Debug)]
struct EcdsaP521(P521Hash);

static P521_SHA256: EcdsaP521 = EcdsaP521(P521Hash::Sha256);
static P521_SHA384: EcdsaP521 = EcdsaP521(P521Hash::Sha384);
static P521_SHA512: EcdsaP521 = EcdsaP521(P521Hash::Sha512);
static P521_TLS: [&dyn SignatureVerificationAlgorithm; 1] = [&P521_SHA512];

impl SignatureVerificationAlgorithm for EcdsaP521 {
    fn verify_signature(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), InvalidSignature> {
        use p521::ecdsa::signature::hazmat::PrehashVerifier;
        use sha2::Digest;
        let key = p521::ecdsa::VerifyingKey::from_sec1_bytes(public_key).map_err(|_| InvalidSignature)?;
        let sig = p521::ecdsa::Signature::from_der(signature).map_err(|_| InvalidSignature)?;
        let digest = match self.0 {
            P521Hash::Sha256 => sha2::Sha256::digest(message).to_vec(),
            P521Hash::Sha384 => sha2::Sha384::digest(message).to_vec(),
            P521Hash::Sha512 => sha2::Sha512::digest(message).to_vec(),
        };
        key.verify_prehash(&digest, &sig).map_err(|_| InvalidSignature)
    }

    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::ECDSA_P521
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        match self.0 {
            P521Hash::Sha256 => alg_id::ECDSA_SHA256,
            P521Hash::Sha384 => alg_id::ECDSA_SHA384,
            P521Hash::Sha512 => alg_id::ECDSA_SHA512,
        }
    }
}

/// RSA PKCS#1 verification accepting keys from 1024 bits: *ring*'s default requires 2048
/// bits, and old self-signed certificates of FTP servers often have 1024-bit keys.
#[derive(Debug)]
struct RsaLegacy {
    params: &'static ring::signature::RsaParameters,
    signature: AlgorithmIdentifier,
}

static RSA_LEGACY_SHA256: RsaLegacy =
    RsaLegacy { params: &ring::signature::RSA_PKCS1_1024_8192_SHA256_FOR_LEGACY_USE_ONLY, signature: alg_id::RSA_PKCS1_SHA256 };
static RSA_LEGACY_SHA512: RsaLegacy =
    RsaLegacy { params: &ring::signature::RSA_PKCS1_1024_8192_SHA512_FOR_LEGACY_USE_ONLY, signature: alg_id::RSA_PKCS1_SHA512 };
static RSA_LEGACY_TLS256: [&dyn SignatureVerificationAlgorithm; 1] = [&RSA_LEGACY_SHA256];
static RSA_LEGACY_TLS512: [&dyn SignatureVerificationAlgorithm; 1] = [&RSA_LEGACY_SHA512];

impl SignatureVerificationAlgorithm for RsaLegacy {
    fn verify_signature(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), InvalidSignature> {
        ring::signature::UnparsedPublicKey::new(self.params, public_key).verify(message, signature).map_err(|_| InvalidSignature)
    }

    fn public_key_alg_id(&self) -> AlgorithmIdentifier {
        alg_id::RSA_ENCRYPTION
    }

    fn signature_alg_id(&self) -> AlgorithmIdentifier {
        self.signature
    }
}

/// *ring* provider completed with ECDSA P-521 verification and 1024-bit RSA keys (built
/// once).
fn crypto_provider() -> Arc<CryptoProvider> {
    static PROVIDER: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    PROVIDER
        .get_or_init(|| {
            let mut p = rustls::crypto::ring::default_provider();
            let base = p.signature_verification_algorithms;
            // The legacy RSA checks replace ring's (same identifiers, wider key sizes):
            // webpki stops at the first algorithm that rejects a signature.
            let replaced = |a: &&dyn SignatureVerificationAlgorithm| {
                a.public_key_alg_id() == alg_id::RSA_ENCRYPTION
                    && (a.signature_alg_id() == alg_id::RSA_PKCS1_SHA256 || a.signature_alg_id() == alg_id::RSA_PKCS1_SHA512)
            };
            let mut all: Vec<&'static dyn SignatureVerificationAlgorithm> = base.all.iter().copied().filter(|a| !replaced(a)).collect();
            all.extend([
                &P521_SHA256 as &dyn SignatureVerificationAlgorithm,
                &P521_SHA384,
                &P521_SHA512,
                &RSA_LEGACY_SHA256,
                &RSA_LEGACY_SHA512,
            ]);
            let mut mapping: Vec<_> = base
                .mapping
                .iter()
                .map(|&(scheme, algs)| match scheme {
                    SignatureScheme::RSA_PKCS1_SHA256 => (scheme, &RSA_LEGACY_TLS256[..]),
                    SignatureScheme::RSA_PKCS1_SHA512 => (scheme, &RSA_LEGACY_TLS512[..]),
                    _ => (scheme, algs),
                })
                .collect();
            mapping.push((SignatureScheme::ECDSA_NISTP521_SHA512, &P521_TLS));
            // Leaked once for the whole run: rustls wants 'static tables.
            p.signature_verification_algorithms =
                WebPkiSupportedAlgorithms { all: Box::leak(all.into_boxed_slice()), mapping: Box::leak(mapping.into_boxed_slice()) };
            Arc::new(p)
        })
        .clone()
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
        let provider = crypto_provider();
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

    #[test]
    fn p521_signatures_are_verified_and_offered() {
        use p521::ecdsa::signature::hazmat::PrehashSigner;
        use sha2::Digest;
        let mut scalar = [7u8; 66];
        scalar[0] = 0; // below the order of the curve
        let sk = p521::ecdsa::SigningKey::from_slice(&scalar).expect("valid scalar");
        let pk = sk.verifying_key().to_sec1_bytes();
        let msg = b"handshake transcript";
        for (alg, digest) in [
            (&P521_SHA256, sha2::Sha256::digest(msg).to_vec()),
            (&P521_SHA384, sha2::Sha384::digest(msg).to_vec()),
            (&P521_SHA512, sha2::Sha512::digest(msg).to_vec()),
        ] {
            let sig: p521::ecdsa::Signature = sk.sign_prehash(&digest).expect("signature");
            let der = sig.to_der();
            assert!(alg.verify_signature(&pk, msg, der.as_bytes()).is_ok(), "{alg:?}");
            assert!(alg.verify_signature(&pk, b"other", der.as_bytes()).is_err(), "{alg:?}");
        }
        let p = crypto_provider();
        let ring_count = rustls::crypto::ring::default_provider().signature_verification_algorithms.mapping.len();
        assert_eq!(p.signature_verification_algorithms.mapping.len(), ring_count + 1);
        assert!(p.signature_verification_algorithms.supported_schemes().contains(&SignatureScheme::ECDSA_NISTP521_SHA512));
        assert!(p.signature_verification_algorithms.supported_schemes().contains(&SignatureScheme::ECDSA_NISTP256_SHA256));
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).filter_map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
    }

    #[test]
    fn rsa_1024_signatures_are_verified() {
        // 1024-bit key (PKCS#1) and signatures made with OpenSSL over the message below.
        let pk = unhex(concat!(
            "30818902818100b96a2d9d731925057c24252abee523c40d88e4b792f7d7c920b7dec12e8cb2d153e90f517b97fb2c4dbbde07ffb300f9",
            "02558b2bd85e5cd04720189601244ca6a20d4249e395d5edfdf76b3789bc942be91cf6bc87a737def235141b905e56bc98c319a35f259b82",
            "1621f32253f9f47f9c22301f2cfb31763198108f025433b10203010001"
        ));
        let s256 = unhex(concat!(
            "3d4a6dc0ff80e8be418e3edd8aebc7e03173f14b14a6e67b8505ca95d523227dd89bc4e2467e1ec707217274eaf6677a262eee57c7c37561",
            "0dfa3872b0f1793616afb2a18784fbf5318bd92a6cd3e61cfe3c6885d8e94f482fc75d618e4bf9de5dca6595b550bec0d31d9a4a546a0f76",
            "f8787dcf14ecb69c0449e99dae90531c"
        ));
        let s512 = unhex(concat!(
            "64254fb93ff4e0107e8fca7f1c8c40f4dc83ed434492e41c4e2d590c8f75bb03aeae35c68665da4544614af2f503302822dbcce8312b3819",
            "eb9afe6d8df834cf16a6f4cde8f2dcbe1ce77bfcfd7293d5cc78c8895c56c051e8cc35e45643673a558584de886d70c0119d9f8e859867c8",
            "c6be71fa4e8fe429858816bde0e10672"
        ));
        let msg = b"handshake transcript";
        assert!(RSA_LEGACY_SHA256.verify_signature(&pk, msg, &s256).is_ok());
        assert!(RSA_LEGACY_SHA512.verify_signature(&pk, msg, &s512).is_ok());
        assert!(RSA_LEGACY_SHA256.verify_signature(&pk, b"other", &s256).is_err());
        assert!(RSA_LEGACY_SHA256.verify_signature(&pk, msg, &s512).is_err());
        // ring's default refuses this key: the replacement is what makes it work.
        let ring_default = ring::signature::UnparsedPublicKey::new(&ring::signature::RSA_PKCS1_2048_8192_SHA256, &pk);
        assert!(ring_default.verify(msg, &s256).is_err());
    }
}
