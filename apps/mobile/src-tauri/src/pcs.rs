//! The PCs this phone is paired with, and talking to them.
//!
//! Kept in the app's own folder (`pcs.json`), which other apps can't read. The tokens never
//! leave this file except to go to their PC, over pinned TLS; the screens only see [`PcInfo`].

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jb_remote::{Action, PairLink, PairReply, PairRequest, PresetRequest};
use serde::{Deserialize, Serialize};

/// How long to wait for the PC.
const TIMEOUT: Duration = Duration::from_secs(4);
/// How long to look for a PC whose address changed.
const FIND_WAIT: Duration = Duration::from_millis(1500);
const MAX_PCS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedPc {
    pub id: String,
    pub name: String,
    pub addr: SocketAddr,
    pub fingerprint: String,
    pub token: String,
}

/// A paired PC, for the screens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PcInfo {
    pub id: String,
    pub name: String,
    pub address: String,
}

impl From<&SavedPc> for PcInfo {
    fn from(p: &SavedPc) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            address: p.addr.to_string(),
        }
    }
}

/// What went wrong, for the screens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    /// `unreachable` (PC off, asleep or not on this Wi-Fi), `unpaired`, `refused` (the PC said
    /// no), `badLink`, `notFound` or `storage`.
    pub kind: &'static str,
    pub message: String,
}

impl Problem {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PcsFile {
    #[serde(default)]
    pcs: Vec<SavedPc>,
}

/// The saved PCs (in `dir`).
#[derive(Debug, Clone)]
pub struct Pcs {
    file: PathBuf,
}

impl Pcs {
    pub fn new(dir: &Path) -> Self {
        Self {
            file: dir.join("pcs.json"),
        }
    }

    pub fn load(&self) -> Vec<SavedPc> {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|t| serde_json::from_str::<PcsFile>(&t).ok())
            .map(|f| f.pcs)
            .unwrap_or_default()
    }

    fn save(&self, pcs: &[SavedPc]) -> Result<(), Problem> {
        let text = serde_json::to_string_pretty(&PcsFile { pcs: pcs.to_vec() })
            .map_err(|e| Problem::new("storage", e.to_string()))?;
        let tmp = self.file.with_extension("json.tmp");
        self.file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&tmp, text))
            .and_then(|()| std::fs::rename(&tmp, &self.file))
            .map_err(|e| Problem::new("storage", format!("couldn't save the paired PCs: {e}")))
    }

    pub fn list(&self) -> Vec<PcInfo> {
        self.load().iter().map(PcInfo::from).collect()
    }

    fn get(&self, id: &str) -> Result<SavedPc, Problem> {
        self.load()
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| Problem::new("notFound", "this PC isn't paired any more"))
    }

    /// Pairs with the PC in a QR code's `link`. Pairing again with the same PC replaces it.
    pub fn pair(&self, link: &str, phone: &str) -> Result<PcInfo, Problem> {
        let link = PairLink::parse(link).map_err(|e| Problem::new("badLink", e.to_string()))?;
        let body = serde_json::to_value(PairRequest {
            code: link.code.clone(),
            name: phone.to_string(),
        })
        .map_err(|e| Problem::new("storage", e.to_string()))?;
        let reply = jb_remote::client::request(
            link.addr,
            &link.fingerprint,
            "POST",
            "/app/pair",
            None,
            Some(&body),
            TIMEOUT,
        )
        .map_err(unreachable)?;
        let value = answer(&reply)?;
        let paired: PairReply = serde_json::from_value(value)
            .map_err(|_| Problem::new("refused", "the PC's answer didn't make sense"))?;
        let mut pcs = self.load();
        pcs.retain(|p| p.fingerprint != link.fingerprint);
        if pcs.len() >= MAX_PCS {
            pcs.remove(0);
        }
        let pc = SavedPc {
            id: jb_remote::token()[..8].to_string(),
            name: paired.pc,
            addr: link.addr,
            fingerprint: link.fingerprint,
            token: paired.token,
        };
        let info = PcInfo::from(&pc);
        pcs.push(pc);
        self.save(&pcs)?;
        Ok(info)
    }

    /// Forgets a PC here, and tells it (if it can be reached) to forget this phone.
    pub fn forget(&self, id: &str) -> Result<(), Problem> {
        let mut pcs = self.load();
        if let Some(pc) = pcs.iter().find(|p| p.id == id) {
            let _ = jb_remote::client::request(
                pc.addr,
                &pc.fingerprint,
                "POST",
                "/app/unpair",
                Some(&pc.token),
                None,
                Duration::from_secs(2),
            );
        }
        pcs.retain(|p| p.id != id);
        self.save(&pcs)
    }

    /// Sends a request to a paired PC. If it can't be reached, looks for it on the network
    /// (its address may have changed) and tries once more.
    fn call(
        &self,
        id: &str,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Problem> {
        let pc = self.get(id)?;
        let send = |addr: SocketAddr| {
            jb_remote::client::request(
                addr,
                &pc.fingerprint,
                method,
                path,
                Some(&pc.token),
                body.as_ref(),
                TIMEOUT,
            )
        };
        let reply = match send(pc.addr) {
            Ok(r) => r,
            Err(jb_remote::Error::Io(e)) => {
                let Some(found) =
                    jb_remote::discover::find(&pc.fingerprint, pc.addr.port(), None, FIND_WAIT)
                        .filter(|a| *a != pc.addr)
                else {
                    return Err(unreachable(jb_remote::Error::Io(e)));
                };
                let r = send(found).map_err(unreachable)?;
                self.moved(id, found);
                r
            }
            Err(e) => return Err(unreachable(e)),
        };
        answer(&reply).map_err(|p| {
            if p.kind == "unpaired" {
                Problem::new(
                    "unpaired",
                    format!(
                        "{} doesn't know this phone any more. Scan its QR code again.",
                        pc.name
                    ),
                )
            } else {
                p
            }
        })
    }

    /// Remembers a PC's new address.
    fn moved(&self, id: &str, addr: SocketAddr) {
        let mut pcs = self.load();
        if let Some(p) = pcs.iter_mut().find(|p| p.id == id) {
            p.addr = addr;
            let _ = self.save(&pcs);
        }
    }

    pub fn state(&self, id: &str) -> Result<serde_json::Value, Problem> {
        self.call(id, "GET", "/app/state", None)
    }

    pub fn act(&self, id: &str, action: &Action) -> Result<serde_json::Value, Problem> {
        let body =
            serde_json::to_value(action).map_err(|e| Problem::new("storage", e.to_string()))?;
        self.call(id, "POST", "/app/action", Some(body))
    }

    pub fn preset(&self, id: &str, name: &str) -> Result<serde_json::Value, Problem> {
        let body = serde_json::to_value(PresetRequest {
            name: name.to_string(),
        })
        .map_err(|e| Problem::new("storage", e.to_string()))?;
        self.call(id, "POST", "/app/preset", Some(body))
    }
}

fn unreachable(e: jb_remote::Error) -> Problem {
    match e {
        jb_remote::Error::Tls(_) => Problem::new(
            "refused",
            "the PC at that address isn't the one you paired with",
        ),
        _ => Problem::new(
            "unreachable",
            "can't reach the PC. Is JuiceBox Trainer open there with the phone remote on, and is this phone on the same Wi-Fi?",
        ),
    }
}

/// The reply's JSON, or the PC's reason for saying no.
fn answer(reply: &jb_remote::client::Reply) -> Result<serde_json::Value, Problem> {
    let value = reply.json().unwrap_or(serde_json::Value::Null);
    let why = value
        .get("error")
        .and_then(|e| e.as_str())
        .unwrap_or("the PC said no")
        .to_string();
    match reply.status {
        200 => Ok(value),
        401 => Err(Problem::new("unpaired", why)),
        _ => Err(Problem::new("refused", why)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> (Pcs, PathBuf) {
        let dir = std::env::temp_dir().join(format!("jb-phone-test-{}", jb_remote::token()));
        (Pcs::new(&dir), dir)
    }

    #[test]
    fn a_bad_link_is_explained() {
        let (pcs, dir) = temp();
        let e = pcs.pair("https://example.com", "Phone").unwrap_err();
        assert_eq!(e.kind, "badLink");
        assert!(pcs.list().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saved_pcs_round_trip_without_showing_tokens() {
        let (pcs, dir) = temp();
        let pc = SavedPc {
            id: "abcd1234".into(),
            name: "DESKTOP".into(),
            addr: "192.168.1.20:47110".parse().unwrap(),
            fingerprint: "ab".repeat(32),
            token: "secret".into(),
        };
        pcs.save(std::slice::from_ref(&pc)).unwrap();
        assert_eq!(pcs.load(), vec![pc.clone()]);
        let shown = serde_json::to_string(&pcs.list()).unwrap();
        assert!(shown.contains("DESKTOP") && !shown.contains("secret"));
        pcs.moved("abcd1234", "192.168.1.30:47110".parse().unwrap());
        assert_eq!(pcs.list()[0].address, "192.168.1.30:47110");
        // Nobody listens on this port: unreachable, and nothing changes.
        let gone = SavedPc {
            addr: "127.0.0.1:9".parse().unwrap(),
            ..pc
        };
        pcs.save(std::slice::from_ref(&gone)).unwrap();
        assert_eq!(pcs.state("abcd1234").unwrap_err().kind, "unreachable");
        assert_eq!(pcs.state("nope").unwrap_err().kind, "notFound");
        pcs.forget("abcd1234").unwrap();
        assert!(pcs.list().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn answers_map_to_problems() {
        let r = |status, body: &str| jb_remote::client::Reply {
            status,
            body: body.as_bytes().to_vec(),
        };
        assert_eq!(
            answer(&r(200, r#"{"pack":null}"#)).unwrap()["pack"],
            serde_json::Value::Null
        );
        assert_eq!(
            answer(&r(401, r#"{"error":"x"}"#)).unwrap_err().kind,
            "unpaired"
        );
        let e = answer(&r(409, r#"{"error":"no cheat 'bad'"}"#)).unwrap_err();
        assert_eq!((e.kind, e.message.as_str()), ("refused", "no cheat 'bad'"));
    }
}
