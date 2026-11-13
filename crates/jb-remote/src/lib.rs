//! The link between JuiceBox Remote (the phone app) and JuiceBox Trainer on a PC.
//!
//! - **Pinned TLS.** The PC makes its own certificate once ([`tls::Identity`]) and keeps it.
//!   The QR code the phone scans holds the certificate's SHA-256 [`fingerprint`]; the phone
//!   accepts that certificate and no other, so nobody on the same Wi-Fi can read or change
//!   what goes between them, or pretend to be the PC.
//! - **Pairing.** The QR code also holds a one-time code. The phone trades it for its own random
//!   token (`POST /app/pair`), which it sends with every request after that. The PC keeps only
//!   the token's SHA-256, so a copied settings file can't be used to control it.
//! - **Finding the PC again.** If the PC's address changes, the phone asks the local network
//!   who has that certificate ([`discover`]).
//!
//! The phone can only do what the trainer's own buttons do ([`Action`]).

pub mod client;
pub mod discover;
pub mod tls;

use serde::{Deserialize, Serialize};

/// The port the PC listens on (TCP for the remote, UDP for being found), unless taken.
pub const DEFAULT_PORT: u16 = 47110;

/// What a phone asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    /// `toggle`, `press`, `set` or `release`.
    pub op: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// `POST /app/pair`: the one-time code from the QR code, and what to call this phone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairRequest {
    pub code: String,
    pub name: String,
}

/// The PC's answer to a pairing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairReply {
    /// The phone's own token (keep it secret).
    pub token: String,
    /// The PC's name, to show on the phone.
    pub pc: String,
}

/// `POST /app/preset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetRequest {
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("can't reach the PC: {0}")]
    Io(#[from] std::io::Error),
    #[error("secure connection failed: {0}")]
    Tls(#[from] rustls::Error),
    #[error("the PC answered something that isn't a remote reply")]
    BadReply,
    #[error("{0}")]
    Link(String),
}

/// 128 random bits as hex, from the system's secure generator.
pub fn token() -> String {
    use ring::rand::SecureRandom;
    let mut buf = [0u8; 16];
    ring::rand::SystemRandom::new()
        .fill(&mut buf)
        .expect("the system random generator failed");
    hex(&buf)
}

/// SHA-256 as lowercase hex.
pub fn sha256_hex(data: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, data).as_ref())
}

/// A certificate's fingerprint (SHA-256 of its DER bytes, lowercase hex).
pub fn fingerprint(cert_der: &[u8]) -> String {
    sha256_hex(cert_der)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Equal strings, in time that doesn't depend on where they differ (equal lengths).
pub fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// What the QR code says: where the PC is, its certificate and the one-time code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairLink {
    pub addr: std::net::SocketAddr,
    pub code: String,
    pub fingerprint: String,
}

impl PairLink {
    /// The QR code's text: `http://<ip>:<port>/pair?code=<code>&fp=<fingerprint>`. A phone
    /// camera opens it as the remote web page (which ignores `fp`); the app reads all of it.
    pub fn to_url(&self) -> String {
        format!(
            "http://{}/pair?code={}&fp={}",
            self.addr, self.code, self.fingerprint
        )
    }

    /// Reads a pairing link (from the QR code, or pasted).
    pub fn parse(text: &str) -> Result<Self, Error> {
        let bad = |why: &str| Error::Link(why.to_string());
        let rest = text
            .trim()
            .strip_prefix("http://")
            .ok_or_else(|| bad("that isn't a JuiceBox Trainer pairing link"))?;
        let (host, rest) = rest
            .split_once('/')
            .ok_or_else(|| bad("that isn't a JuiceBox Trainer pairing link"))?;
        let addr: std::net::SocketAddr = host
            .parse()
            .map_err(|_| bad("the link doesn't hold the PC's address"))?;
        let query = rest
            .strip_prefix("pair?")
            .ok_or_else(|| bad("that isn't a JuiceBox Trainer pairing link"))?;
        let param = |name: &str| {
            query.split('&').find_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                (k == name).then(|| v.to_string())
            })
        };
        let code = param("code")
            .filter(|c| c.len() == 32 && c.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| bad("the link has no pairing code"))?;
        let fingerprint = param("fp")
            .filter(|f| f.len() == 64 && f.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(|f| f.to_ascii_lowercase())
            .ok_or_else(|| {
                bad("this link is from an older JuiceBox Trainer: update it on your PC (About → Check for updates)")
            })?;
        Ok(Self {
            addr,
            code,
            fingerprint,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_hex() {
        let (a, b) = (token(), token());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sha256_matches_a_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn pair_links_round_trip_and_reject_others() {
        let link = PairLink {
            addr: "192.168.1.20:47110".parse().unwrap(),
            code: "0123456789abcdef0123456789abcdef".into(),
            fingerprint: "ab".repeat(32),
        };
        assert_eq!(PairLink::parse(&link.to_url()).unwrap(), link);
        assert_eq!(
            PairLink::parse(&format!("  {}\n", link.to_url())).unwrap(),
            link
        );
        for bad in [
            "",
            "https://example.com/pair?code=x",
            "http://example.com/pair?code=0123456789abcdef0123456789abcdef&fp=aa",
            "http://192.168.1.20:47110/pair?code=short",
            "http://192.168.1.20:47110/other?code=0123456789abcdef0123456789abcdef",
        ] {
            assert!(PairLink::parse(bad).is_err(), "{bad}");
        }
        // A link from before the app existed (no fingerprint) says to update the PC.
        let old = "http://192.168.1.20:47110/pair?code=0123456789abcdef0123456789abcdef";
        assert!(
            PairLink::parse(old)
                .unwrap_err()
                .to_string()
                .contains("older")
        );
    }

    #[test]
    fn same_compares_whole_strings() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "ab"));
    }
}
