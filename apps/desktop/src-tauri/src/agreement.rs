//! The player's agreement (v1.18.0): before using JuiceBox Trainer, the player reads
//! the warning about fair use and the waiver, and signs by typing their name.
//! It's kept in `%APPDATA%\JuiceBox Trainer\agreement.json` with the date and the
//! agreement's version; a new version is shown again.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::commands::{CmdResult, CommandError};

/// Raise when the agreement's text changes in a way players must see again.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signed {
    pub version: u32,
    /// The name the player typed.
    pub name: String,
    /// `YYYY-MM-DD`.
    pub date: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgreementStatus {
    /// The current version is signed.
    pub accepted: bool,
    pub version: u32,
    /// The last signature, if any (also an older version's).
    pub signed: Option<Signed>,
}

fn file() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| {
        PathBuf::from(a)
            .join(jb_core::names::FOLDER)
            .join("agreement.json")
    })
}

fn read(path: &std::path::Path) -> Option<Signed> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn status_at(path: Option<&std::path::Path>) -> AgreementStatus {
    let signed = path.and_then(read);
    AgreementStatus {
        accepted: signed.as_ref().is_some_and(|s| s.version >= VERSION),
        version: VERSION,
        signed,
    }
}

/// A name a player can sign with: 2 to 80 characters, one line.
fn check_name(name: &str) -> Result<String, CommandError> {
    let name = name.trim();
    if name.chars().count() < 2 || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(CommandError::new(
            "badRequest",
            "type your name (2 to 80 characters) to sign",
        ));
    }
    Ok(name.to_string())
}

fn sign_at(path: &std::path::Path, name: &str, today: &str) -> CmdResult<Signed> {
    let signed = Signed {
        version: VERSION,
        name: check_name(name)?,
        date: today.to_string(),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| CommandError::new("io", format!("couldn't save the agreement: {e}")))?;
    }
    let text = serde_json::to_string_pretty(&signed)
        .map_err(|e| CommandError::new("io", e.to_string()))?;
    std::fs::write(path, text + "\n")
        .map_err(|e| CommandError::new("io", format!("couldn't save the agreement: {e}")))?;
    Ok(signed)
}

/// Whether the player signed the current agreement.
#[tauri::command]
pub fn agreement_status() -> AgreementStatus {
    status_at(file().as_deref())
}

/// Signs the current agreement with the player's typed name.
#[tauri::command]
pub fn agreement_sign(name: String) -> CmdResult<AgreementStatus> {
    let path = file().ok_or_else(|| CommandError::new("noAppData", "%APPDATA% is not set"))?;
    sign_at(&path, &name, &jb_packs::share::today())?;
    Ok(status_at(Some(&path)))
}

/// Refuses until the current agreement is signed (for actions that use cheats).
pub fn require() -> CmdResult<()> {
    if agreement_status().accepted {
        Ok(())
    } else {
        Err(CommandError::new(
            "needsAgreement",
            "read and sign JuiceBox Trainer's agreement first",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_once_per_version() {
        let dir = std::env::temp_dir().join(format!("ot-agreement-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("agreement.json");
        assert!(!status_at(Some(&path)).accepted);
        assert!(sign_at(&path, " ", "2026-11-09").is_err());
        assert!(sign_at(&path, "a\nb", "2026-11-09").is_err());
        let s = sign_at(&path, "  Sam Player ", "2026-11-09").unwrap();
        assert_eq!(s.name, "Sam Player");
        let st = status_at(Some(&path));
        assert!(st.accepted);
        assert_eq!(st.signed.unwrap().date, "2026-11-09");

        // An older version's signature doesn't count.
        std::fs::write(&path, r#"{"version":0,"name":"Old","date":"2026-01-01"}"#).unwrap();
        let old = status_at(Some(&path));
        assert!(!old.accepted);
        assert_eq!(old.signed.unwrap().name, "Old");
        // Broken files don't count either.
        std::fs::write(&path, "{broken").unwrap();
        assert!(!status_at(Some(&path)).accepted);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
