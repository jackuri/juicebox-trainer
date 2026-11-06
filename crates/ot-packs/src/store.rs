//! Where packs come from: the examples built into the app, and the user's
//! pack folder `%APPDATA%\OpenTrainer\packs\<game-id>\pack.json`.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Packs refuse to load above this size; real packs are a few KiB.
pub const MAX_PACK_BYTES: u64 = 4 << 20;

/// Example packs for the testbed game, compiled into the app.
pub const BUNDLED: &[(&str, &str)] = &[
    (
        "testbed-native",
        include_str!("../../../packs/examples/testbed-native/pack.json"),
    ),
    (
        "testbed-native-x86",
        include_str!("../../../packs/examples/testbed-native-x86/pack.json"),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Built into the app.
    Bundled,
    /// From the user's pack folder.
    User,
}

/// Raw pack text and where it came from.
#[derive(Debug, Clone)]
pub struct Found {
    pub source: Source,
    /// `bundled:<id>` or the file path.
    pub key: String,
    pub path: Option<PathBuf>,
    pub text: Result<String, String>,
}

/// `%APPDATA%\OpenTrainer\packs`.
pub fn user_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("OpenTrainer").join("packs"))
}

pub fn bundled() -> Vec<Found> {
    BUNDLED
        .iter()
        .map(|(id, text)| Found {
            source: Source::Bundled,
            key: format!("bundled:{id}"),
            path: None,
            text: Ok((*text).to_string()),
        })
        .collect()
}

/// Reads a pack file, refusing oversized ones.
pub fn read_file(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_PACK_BYTES {
        return Err(format!(
            "{} is {} bytes; packs are limited to {MAX_PACK_BYTES}",
            path.display(),
            meta.len()
        ));
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

/// Every `<dir>/<id>/pack.json`, sorted by path. A missing folder is empty.
pub fn scan_dir(dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path().join("pack.json"))
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| Found {
            source: Source::User,
            key: p.display().to_string(),
            text: read_file(&p),
            path: Some(p),
        })
        .collect()
}

/// Bundled packs followed by the user's packs.
pub fn all() -> Vec<Found> {
    let mut out = bundled();
    if let Some(dir) = user_dir() {
        out.extend(scan_dir(&dir));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{Severity, parse};

    #[test]
    fn bundled_packs_are_valid() {
        for f in bundled() {
            let (pack, issues) =
                parse(f.text.as_ref().unwrap()).unwrap_or_else(|e| panic!("{}: {e}", f.key));
            assert_eq!(format!("bundled:{}", pack.game.id), f.key);
            // The testbed's exe hash depends on the compiler, so examples
            // list no builds; that warning is the only one allowed.
            let other: Vec<_> = issues
                .iter()
                .filter(|i| i.severity == Severity::Warning && i.path != "game.builds")
                .collect();
            assert!(other.is_empty(), "{}: {other:#?}", f.key);
        }
    }

    #[test]
    fn scans_a_pack_folder() {
        let dir = std::env::temp_dir().join(format!("ot-packs-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("b-game")).unwrap();
        std::fs::create_dir_all(dir.join("a-game")).unwrap();
        std::fs::create_dir_all(dir.join("empty")).unwrap();
        std::fs::write(dir.join("a-game").join("pack.json"), "{}").unwrap();
        std::fs::write(dir.join("b-game").join("pack.json"), "{}").unwrap();
        let found = scan_dir(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found.len(), 2);
        assert!(found[0].key.contains("a-game"));
        assert_eq!(found[0].source, Source::User);
        assert!(scan_dir(Path::new(r"C:\definitely\not\here")).is_empty());
    }
}
