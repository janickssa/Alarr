//! HTTP clients with optional certificate fingerprint pinning.
//!
//! Self-signed certificates (Proxmox, homelab services) are trusted by pinning the SHA-256
//! fingerprint of the server's leaf certificate. TLS verification is never disabled.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};

use crate::crypto::constant_time_eq;

/// Parse a SHA-256 fingerprint written as hex, with or without `:` separators.
pub fn parse_fingerprint(value: &str) -> anyhow::Result<[u8; 32]> {
    let cleaned: String = value
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-'))
        .collect();
    let bytes = hex::decode(&cleaned).context("fingerprint is not hex")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("fingerprint must be a SHA-256 (32 bytes)"))
}

/// Format a fingerprint the way Proxmox and openssl print it (`AB:CD:…`).
pub fn format_fingerprint(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

#[derive(Debug)]
pub struct PinnedVerifier {
    pin: [u8; 32],
    algs: WebPkiSupportedAlgorithms,
}

impl PinnedVerifier {
    pub fn new(pin: [u8; 32]) -> Self {
        Self {
            pin,
            algs: provider().signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for PinnedVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = Sha256::digest(end_entity.as_ref());
        if constant_time_eq(&presented, &self.pin) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "certificate fingerprint mismatch (server presented {})",
                format_fingerprint(&presented)
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algs.supported_schemes()
    }
}

/// Records the presented certificate's fingerprint and then fails the handshake,
/// so no request (and no credential) is ever sent over the connection.
#[derive(Debug)]
struct CaptureVerifier {
    seen: Arc<Mutex<Option<String>>>,
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for CaptureVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = format_fingerprint(&Sha256::digest(end_entity.as_ref()));
        *self.seen.lock().expect("lock") = Some(fp);
        Err(rustls::Error::General("fingerprint captured".into()))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algs.supported_schemes()
    }
}

fn config_with(verifier: Arc<dyn ServerCertVerifier>) -> anyhow::Result<rustls::ClientConfig> {
    Ok(rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth())
}

/// An HTTP client. With a fingerprint, only a server presenting exactly that leaf
/// certificate is accepted; without one, normal WebPKI verification applies.
pub fn http_client(
    fingerprint: Option<&str>,
    timeout: Duration,
) -> anyhow::Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(timeout.min(Duration::from_secs(5)))
        .user_agent(concat!("quorum-server/", env!("CARGO_PKG_VERSION")));
    let builder = match fingerprint {
        Some(fp) => {
            let pin = parse_fingerprint(fp)?;
            builder.use_preconfigured_tls(config_with(Arc::new(PinnedVerifier::new(pin)))?)
        }
        None => builder,
    };
    Ok(builder.build()?)
}

/// Connect to an HTTPS URL only to read the certificate fingerprint. Sends nothing.
pub async fn fetch_fingerprint(url: &str) -> Option<String> {
    let seen = Arc::new(Mutex::new(None));
    let verifier = CaptureVerifier {
        seen: seen.clone(),
        algs: provider().signature_verification_algorithms,
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .use_preconfigured_tls(config_with(Arc::new(verifier)).ok()?)
        .build()
        .ok()?;
    let _ = client.get(url).send().await;
    seen.lock().expect("lock").take()
}

/// True when the error chain looks like a certificate problem.
pub fn is_cert_error(err: &reqwest::Error) -> bool {
    let mut source: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = source {
        let text = e.to_string().to_lowercase();
        if text.contains("certificate") || text.contains("unknownissuer") {
            return true;
        }
        source = e.source();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf() -> CertificateDer<'static> {
        let cert = rcgen::generate_simple_self_signed(vec!["pve.local".into()]).unwrap();
        cert.cert.der().clone()
    }

    fn verify(v: &PinnedVerifier, cert: &CertificateDer<'_>) -> bool {
        v.verify_server_cert(
            cert,
            &[],
            &ServerName::try_from("pve.local").unwrap(),
            &[],
            UnixTime::now(),
        )
        .is_ok()
    }

    #[test]
    fn pinned_verifier_accepts_only_matching_cert() {
        let cert = leaf();
        let fp: [u8; 32] = Sha256::digest(cert.as_ref()).into();
        assert!(verify(&PinnedVerifier::new(fp), &cert));
        assert!(!verify(&PinnedVerifier::new([0u8; 32]), &cert));
        assert!(!verify(&PinnedVerifier::new(fp), &leaf()));
    }

    #[test]
    fn fingerprint_formats_round_trip() {
        let bytes = [0xABu8; 32];
        let text = format_fingerprint(&bytes);
        assert!(text.starts_with("AB:AB:"));
        assert_eq!(parse_fingerprint(&text).unwrap(), bytes);
        assert_eq!(
            parse_fingerprint(&text.replace(':', "").to_lowercase()).unwrap(),
            bytes
        );
        assert!(parse_fingerprint("AB:CD").is_err());
    }
}
