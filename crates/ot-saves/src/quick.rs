//! Quick codes: named save edits that cheat packs ship
//! (`"saveCodes": [{ "codec": "json", "file": "{localappdatalow}/TestCo/Testbed/save1.json",
//! "set": { "path": "player.gold", "value": 999999 } }]`).
//!
//! The file is given with a placeholder for a Windows folder, so packs work on
//! every PC. It must be inside one of the game's save folders, and applying a
//! code goes through [`editor::save`] (guard, game closed, backup first).

use std::path::{Component, Path};

use crate::editor::{self, EditReport};
use crate::error::{Error, Result};
use crate::folders::Roots;
use crate::library::Game;
use crate::store::Store;
use crate::{codec::Edit, registry};

/// Folder placeholders a save code's `file` may start with. Keep in sync with
/// `ot_packs::schema::SAVE_PLACEHOLDERS` (a desktop test checks).
pub const PLACEHOLDERS: &[&str] = &[
    "{documents}",
    "{mygames}",
    "{savedgames}",
    "{appdata}",
    "{localappdata}",
    "{localappdatalow}",
    "{playerprefs}",
];

/// Codec names a save code may use.
pub const CODECS: &[&str] = &["auto", "json", "xml", "ini", "yaml", "gvas", "playerprefs"];

fn bad(why: impl Into<String>) -> Error {
    Error::Edit(why.into())
}

/// Turns `{localappdatalow}/TestCo/Testbed/save1.json` into a full path, or
/// `{playerprefs}/TestCo/Testbed` into `HKCU\Software\TestCo\Testbed`.
pub fn resolve(template: &str, roots: &Roots) -> Result<String> {
    let t = template.trim();
    let end = t
        .find('}')
        .ok_or_else(|| bad("the file must start with a folder like {documents}"))?;
    let (head, rest) = t.split_at(end + 1);
    let rest = rest.trim_start_matches(['/', '\\']).replace('/', "\\");
    if rest.is_empty()
        || !Path::new(&rest)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(bad(format!("bad save file path {template:?}")));
    }
    let base = match head.to_ascii_lowercase().as_str() {
        "{playerprefs}" => return Ok(format!(r"HKCU\Software\{rest}")),
        "{documents}" => roots.documents.clone(),
        "{mygames}" => roots.documents.as_ref().map(|d| d.join("My Games")),
        "{savedgames}" => roots.saved_games.clone(),
        "{appdata}" => roots.roaming.clone(),
        "{localappdata}" => roots.local.clone(),
        "{localappdatalow}" => roots.local_low.clone(),
        other => return Err(bad(format!("unknown folder {other}"))),
    }
    .ok_or_else(|| bad(format!("this PC has no {head} folder")))?;
    Ok(crate::library::clean(&base.join(rest))
        .to_string_lossy()
        .into_owned())
}

/// The save folder and path inside it for a resolved file.
pub fn place(game: &Game, resolved: &str) -> Result<(String, String)> {
    let lower = resolved.to_lowercase();
    for f in &game.folders {
        let folder = f.path.to_lowercase();
        if registry::is_registry(&f.path) {
            if folder == lower {
                return Ok((f.path.clone(), String::new()));
            }
            continue;
        }
        if let Some(rest) = lower.strip_prefix(&format!("{folder}\\")) {
            let rel = &resolved[resolved.len() - rest.len()..];
            return Ok((f.path.clone(), rel.replace('\\', "/")));
        }
    }
    Err(bad(format!(
        "{resolved} isn't in one of this game's save folders"
    )))
}

/// The edit text for a code's JSON value.
pub fn value_text(v: &serde_json::Value) -> Result<String> {
    match v {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        _ => Err(bad(
            "a save code's value must be a number, text or true/false",
        )),
    }
}

/// Applies one save code to the game's saves.
pub fn apply(
    game: &Game,
    store: &Store,
    roots: &Roots,
    codec: &str,
    file: &str,
    path: &str,
    value: &serde_json::Value,
) -> Result<EditReport> {
    let resolved = resolve(file, roots)?;
    let (folder, rel) = place(game, &resolved)?;
    let view = editor::open(game, &folder, &rel)?;
    let want = codec.trim().to_ascii_lowercase();
    if !CODECS.contains(&want.as_str()) {
        return Err(bad(format!("unknown codec {codec:?}")));
    }
    if want != "auto" && want != view.codec {
        return Err(bad(format!(
            "this code is for a {want} save, but the file is {}",
            view.format
        )));
    }
    let edit = Edit::Set {
        path: path.to_string(),
        value: value_text(value)?,
    };
    editor::save(game, store, &folder, &rel, &[edit])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::locate::{CloudInfo, SaveFolder};
    use std::path::PathBuf;

    fn roots() -> Roots {
        Roots {
            documents: Some(PathBuf::from(r"C:\Users\me\Documents")),
            local_low: Some(PathBuf::from(r"C:\Users\me\AppData\LocalLow")),
            ..Roots::default()
        }
    }

    #[test]
    fn resolves_placeholders() {
        let r = roots();
        assert_eq!(
            resolve("{localappdatalow}/TestCo/Testbed/save1.json", &r).unwrap(),
            r"C:\Users\me\AppData\LocalLow\TestCo\Testbed\save1.json"
        );
        assert_eq!(
            resolve("{mygames}/Game/a.sav", &r).unwrap(),
            r"C:\Users\me\Documents\My Games\Game\a.sav"
        );
        assert_eq!(
            resolve("{playerprefs}/TestCo/Testbed", &r).unwrap(),
            r"HKCU\Software\TestCo\Testbed"
        );
        assert!(resolve("{localappdatalow}/../evil", &r).is_err());
        assert!(resolve("C:/Windows/x", &r).is_err());
        assert!(resolve("{nope}/x", &r).is_err());
        assert!(
            resolve("{savedgames}/x", &r).is_err(),
            "no such folder here"
        );
        assert!(resolve("{documents}", &r).is_err());
    }

    #[test]
    fn places_files_in_save_folders_only() {
        let g = Game {
            id: "g".into(),
            name: "G".into(),
            exe: String::new(),
            steam: None,
            cloud: CloudInfo::default(),
            folders: vec![
                SaveFolder {
                    path: r"C:\Users\me\AppData\LocalLow\TestCo\Testbed".into(),
                    reason: String::new(),
                    user_added: false,
                    steam_cloud: false,
                },
                SaveFolder {
                    path: r"HKCU\Software\TestCo\Testbed".into(),
                    reason: String::new(),
                    user_added: false,
                    steam_cloud: false,
                },
            ],
            added_ms: 0,
        };
        assert_eq!(
            place(
                &g,
                r"C:\Users\me\AppData\LocalLow\TestCo\Testbed\Slots\save1.json"
            )
            .unwrap(),
            (g.folders[0].path.clone(), "Slots/save1.json".into())
        );
        assert_eq!(
            place(&g, r"hkcu\software\testco\testbed").unwrap(),
            (g.folders[1].path.clone(), String::new())
        );
        assert!(place(&g, r"C:\Users\me\AppData\LocalLow\TestCo\Other\x.json").is_err());
        assert!(place(&g, r"C:\Users\me\AppData\LocalLow\TestCo\TestbedX\x.json").is_err());
    }

    #[test]
    fn values() {
        assert_eq!(value_text(&serde_json::json!(999999)).unwrap(), "999999");
        assert_eq!(value_text(&serde_json::json!("Hero")).unwrap(), "Hero");
        assert_eq!(value_text(&serde_json::json!(true)).unwrap(), "true");
        assert!(value_text(&serde_json::json!([1])).is_err());
    }
}
