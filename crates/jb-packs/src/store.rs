//! Where packs come from: the examples built into the app, and the user's
//! pack folder `%APPDATA%\JuiceBox Trainer\packs\<game-id>\pack.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::schema::Pack;

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
    /// From the user's pack folder (written by hand or by the AI helper).
    User,
    /// Imported from a `.juice` file.
    Imported,
    /// Downloaded from the community pack list.
    Community,
}

/// `origin.json` next to an installed pack: where it came from.
pub const ORIGIN_FILE: &str = "origin.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OriginKind {
    File,
    Community,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub kind: OriginKind,
    /// The file name, or the community list's address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// SHA-256 of the installed `pack.json` text.
    pub sha256: String,
}

impl Origin {
    pub fn source(&self) -> Source {
        match self.kind {
            OriginKind::File => Source::Imported,
            OriginKind::Community => Source::Community,
        }
    }
}

/// The origin recorded next to a pack file, if any.
pub fn origin_of(pack_file: &Path) -> Option<Origin> {
    let text = std::fs::read_to_string(pack_file.with_file_name(ORIGIN_FILE)).ok()?;
    serde_json::from_str(&text).ok()
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

/// `%APPDATA%\JuiceBox Trainer\packs`.
pub fn user_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(jb_core::names::FOLDER).join("packs"))
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
            source: origin_of(&p).map_or(Source::User, |o| o.source()),
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

/// What [`install`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Installed {
    /// No pack for this game before.
    New,
    /// The same pack was already there.
    Same,
    /// Another pack for this game was replaced; the old one is kept as
    /// `pack.json.bak`.
    Replaced,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// A different pack for this game is installed; ask before replacing.
    #[error("you already have a pack for {game}: {name} (version {version}{ai})")]
    Exists {
        game: String,
        name: String,
        version: String,
        ai: &'static str,
    },
    #[error("{0}")]
    Io(String),
}

/// Installs a validated pack as `<dir>/<game.id>/pack.json` and records
/// where it came from. A different pack already there is only replaced when
/// `replace` is set, and is kept as `pack.json.bak`.
pub fn install(
    dir: &Path,
    pack: &Pack,
    text: &str,
    from: OriginKind,
    from_name: Option<String>,
    replace: bool,
) -> Result<(PathBuf, Installed), InstallError> {
    let io = |e: std::io::Error, p: &Path| InstallError::Io(format!("{}: {e}", p.display()));
    let folder = dir.join(&pack.game.id);
    let path = folder.join("pack.json");
    let mut result = Installed::New;
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing == text {
            result = Installed::Same;
        } else if !replace {
            let old = crate::validate::parse(&existing).ok().map(|(p, _)| p);
            return Err(InstallError::Exists {
                game: pack.game.name.clone(),
                name: old.as_ref().map_or_else(
                    || "a pack that doesn't load".into(),
                    |p| p.game.name.clone(),
                ),
                version: old
                    .as_ref()
                    .map_or_else(|| "?".into(), |p| p.meta.pack_version.clone()),
                ai: if old.as_ref().is_some_and(|p| p.meta.is_ai()) {
                    ", made by the AI helper"
                } else {
                    ""
                },
            });
        } else {
            let bak = folder.join("pack.json.bak");
            std::fs::copy(&path, &bak).map_err(|e| io(e, &bak))?;
            result = Installed::Replaced;
        }
    }
    std::fs::create_dir_all(&folder).map_err(|e| io(e, &folder))?;
    if result != Installed::Same {
        let tmp = folder.join("pack.json.tmp");
        std::fs::write(&tmp, text).map_err(|e| io(e, &tmp))?;
        std::fs::rename(&tmp, &path).map_err(|e| io(e, &path))?;
    }
    let origin = Origin {
        kind: from,
        from: from_name,
        sha256: jb_core::hash::sha256_bytes(text.as_bytes()),
    };
    let origin_path = folder.join(ORIGIN_FILE);
    let json =
        serde_json::to_string_pretty(&origin).map_err(|e| InstallError::Io(e.to_string()))?;
    std::fs::write(&origin_path, json + "\n").map_err(|e| io(e, &origin_path))?;
    Ok((path, result))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .to_lowercase()
            .trim_end_matches('\\')
            .to_string()
    };
    norm(a) == norm(b)
}

/// Moves a user pack's folder to `<dir>\..\packs-removed\<id>-<stamp>` (kept,
/// not deleted). Only folders directly inside `dir` are moved.
pub fn remove(dir: &Path, pack_file: &Path, stamp: &str) -> Result<PathBuf, String> {
    let folder = pack_file.parent().ok_or("not a pack file")?;
    if !folder.parent().is_some_and(|p| same_dir(p, dir))
        || pack_file.file_name().is_none_or(|n| n != "pack.json")
    {
        return Err(format!("{} isn't in your pack folder", pack_file.display()));
    }
    let name = folder
        .file_name()
        .ok_or("not a pack folder")?
        .to_string_lossy()
        .into_owned();
    let removed = dir
        .parent()
        .ok_or("no folder above the pack folder")?
        .join("packs-removed");
    std::fs::create_dir_all(&removed).map_err(|e| format!("{}: {e}", removed.display()))?;
    let target = removed.join(format!("{name}-{stamp}"));
    std::fs::rename(folder, &target).map_err(|e| format!("{}: {e}", folder.display()))?;
    Ok(target)
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
        let dir = std::env::temp_dir().join(format!("jb-packs-scan-{}", std::process::id()));
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

    #[test]
    fn installs_asks_before_replacing_and_removes() {
        let root = std::env::temp_dir().join(format!("jb-packs-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("packs");
        let text = BUNDLED[0].1;
        let (pack, _) = parse(text).unwrap();

        let (path, r) = install(
            &dir,
            &pack,
            text,
            OriginKind::File,
            Some("a.otpack".into()),
            false,
        )
        .unwrap();
        assert_eq!(r, Installed::New);
        assert_eq!(scan_dir(&dir)[0].source, Source::Imported);
        let origin = origin_of(&path).unwrap();
        assert_eq!(origin.from.as_deref(), Some("a.otpack"));
        assert_eq!(origin.sha256, jb_core::hash::sha256_bytes(text.as_bytes()));

        let (_, r) = install(&dir, &pack, text, OriginKind::Community, None, false).unwrap();
        assert_eq!(r, Installed::Same);
        assert_eq!(scan_dir(&dir)[0].source, Source::Community);

        let changed = format!("{text}\n");
        let e = install(&dir, &pack, &changed, OriginKind::File, None, false).unwrap_err();
        assert!(matches!(e, InstallError::Exists { .. }), "{e}");
        assert!(e.to_string().contains("you already have a pack for"), "{e}");
        let (_, r) = install(&dir, &pack, &changed, OriginKind::File, None, true).unwrap();
        assert_eq!(r, Installed::Replaced);
        assert_eq!(
            std::fs::read_to_string(path.with_file_name("pack.json.bak")).unwrap(),
            text
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), changed);

        assert!(remove(&dir, Path::new(r"C:\Windows\x\pack.json"), "1").is_err());
        assert!(remove(&dir, &dir.join("testbed-native").join("origin.json"), "1").is_err());
        let moved = remove(&dir, &path, "20261107").unwrap();
        assert!(moved.join("pack.json").is_file());
        assert!(moved.ends_with("testbed-native-20261107"));
        assert!(scan_dir(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
