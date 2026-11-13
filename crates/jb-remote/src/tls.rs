//! TLS with a pinned certificate: the PC's own certificate, trusted by its fingerprint alone.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, ServerConfig, SignatureScheme};

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The PC's certificate and its private key (PKCS#8), both DER.
#[derive(Clone, PartialEq, Eq)]
pub struct Identity {
    pub cert: Vec<u8>,
    pub key: Vec<u8>,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never prints the key.
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint())
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// A new self-signed certificate (ECDSA P-256).
    #[cfg(feature = "server")]
    pub fn generate() -> Result<Self, crate::Error> {
        let made =
            rcgen::generate_simple_self_signed(vec!["juicebox-trainer.local".to_string()])
                .map_err(|e| crate::Error::Link(format!("couldn't make the certificate: {e}")))?;
        Ok(Self {
            cert: made.cert.der().to_vec(),
            key: made.signing_key.serialize_der(),
        })
    }

    pub fn fingerprint(&self) -> String {
        crate::fingerprint(&self.cert)
    }

    /// The server side: TLS 1.3 only, no client certificates (phones send their token).
    pub fn server_config(&self) -> Result<Arc<ServerConfig>, crate::Error> {
        let config = ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.cert.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.clone())),
            )?;
        Ok(Arc::new(config))
    }
}

/// Accepts exactly one certificate: the one with this fingerprint.
#[derive(Debug)]
struct Pinned {
    fingerprint: String,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if crate::same(&crate::fingerprint(end_entity), &self.fingerprint) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "this isn't the PC you paired with".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // Proves the PC holds the key of the pinned certificate.
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// The phone side: trusts only the certificate with `fingerprint`.
pub fn client_config(fingerprint: &str) -> Result<Arc<ClientConfig>, crate::Error> {
    let provider = provider();
    let config = ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned {
            fingerprint: fingerprint.to_ascii_lowercase(),
            provider,
        }))
        .with_no_client_auth();
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    /// A PC that answers one request with `{"ok":true}`; returns its address.
    fn one_shot_server(id: &Identity) -> std::net::SocketAddr {
        let config = id.server_config().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let Ok((sock, _)) = listener.accept() else {
                return;
            };
            let conn = rustls::ServerConnection::new(config).unwrap();
            let mut tls = rustls::StreamOwned::new(conn, sock);
            let mut buf = [0u8; 4096];
            let mut raw = Vec::new();
            while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
                match tls.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => raw.extend_from_slice(&buf[..n]),
                }
            }
            let body = br#"{"ok":true}"#;
            let _ = write!(
                tls,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                body.len()
            );
            let _ = tls.write_all(body);
            tls.conn.send_close_notify();
            let _ = tls.flush();
        });
        addr
    }

    #[test]
    fn talks_to_the_pinned_pc_only() {
        let pc = Identity::generate().unwrap();
        let other = Identity::generate().unwrap();
        assert_ne!(pc.fingerprint(), other.fingerprint());
        assert_eq!(pc.fingerprint().len(), 64);
        assert!(!format!("{pc:?}").contains("key:"));

        let addr = one_shot_server(&pc);
        let reply = crate::client::request(
            addr,
            &pc.fingerprint(),
            "GET",
            "/app/state",
            Some("t"),
            None,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(reply.status, 200);
        assert_eq!(reply.json().unwrap()["ok"], true);

        // The same server, but the phone paired with another PC: refused before anything is sent.
        let addr = one_shot_server(&pc);
        let err = crate::client::request(
            addr,
            &other.fingerprint(),
            "GET",
            "/app/state",
            Some("t"),
            None,
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("isn't the PC you paired with"),
            "{err}"
        );
    }
}
