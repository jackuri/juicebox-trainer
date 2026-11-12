//! The cheat book (v1.28.0): see and edit a pack's cheats without the game
//! running. Only what a player sees and uses can change (names, groups,
//! descriptions, the values a switch keeps or a slider allows, notes,
//! favourites, keys) and cheats can be removed. Where a cheat looks in the
//! game (its address or code patch) is never changed here, so an edit can't
//! break a cheat or touch the game. Packs in the packs folder are edited in
//! place with the old one kept as `pack.json.bak`; a built-in pack is saved
//! as the player's own copy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use jb_packs::schema::Cheat;
use jb_packs::store::{self, Source};
use jb_packs::{GameSettings, Pack, Severity, settings};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::{AppState, CmdResult, CommandError, load_key, loaded_key};

fn bad(m: impl Into<String>) -> CommandError {
    CommandError::new("badRequest", m)
}

/// A pack opened in the cheat book.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookView {
    pub key: String,
    pub source: Source,
    pub pack: Pack,
    pub notes: BTreeMap<String, String>,
    pub favourites: Vec<String>,
    /// The player's own keys: text, or `""` for "no key".
    pub keys: BTreeMap<String, String>,
    /// It's the pack in use in Cheats right now.
    pub loaded: bool,
}

/// The player's changes: the cheats as they want them (any order, some
/// removed) and their notes, favourites and keys.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookEdit {
    pub cheats: Vec<Cheat>,
    #[serde(default)]
    pub notes: BTreeMap<String, String>,
    #[serde(default)]
    pub favourites: Vec<String>,
    /// Cheat id → key text, `""` for no key; missing means the pack's key.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookSaved {
    pub key: String,
    /// A built-in pack was saved as the player's own copy.
    pub copied: bool,
    /// The pack was in use: it was loaded again (its cheats were turned off).
    pub reloaded: bool,
}

fn find(key: &str) -> CmdResult<(store::Found, Pack)> {
    let found = store::all()
        .into_iter()
        .find(|f| f.key == key)
        .ok_or_else(|| CommandError::new("notFound", format!("no pack '{key}'")))?;
    let text = found.text.clone().map_err(|e| CommandError::new("io", e))?;
    let (pack, _) = jb_packs::parse(&text)
        .map_err(|e| CommandError::new("invalidPack", format!("the pack isn't valid: {e}")))?;
    Ok((found, pack))
}

fn settings_dir(state: &AppState) -> Option<PathBuf> {
    state.settings_dir.get().cloned()
}

fn is_loaded(state: &AppState, key: &str) -> bool {
    loaded_key(state).ok().flatten().as_deref() == Some(key)
}

/// Opens a pack's cheats (no game needed).
#[tauri::command]
pub async fn book_open(key: String, state: State<'_, AppState>) -> CmdResult<BookView> {
    let (found, pack) = find(&key)?;
    let s = settings_dir(&state)
        .map(|d| settings::load(&d, &pack.game.id))
        .unwrap_or_default();
    Ok(BookView {
        loaded: is_loaded(&state, &key),
        key,
        source: found.source,
        notes: s.notes,
        favourites: s.favourites,
        keys: s.keys,
        pack,
    })
}

/// The pack with the player's edits: for each cheat they kept, the original
/// with only the parts a player may change taken from the edit. A cheat that
/// isn't in the pack can't be added here, and `impl` and `ui` never change.
pub fn apply_edit(original: &Pack, cheats: &[Cheat]) -> Result<Pack, String> {
    let mut seen = Vec::new();
    let mut out = Vec::with_capacity(cheats.len());
    for c in cheats {
        let Some(orig) = original.cheats.iter().find(|o| o.id == c.id) else {
            return Err(format!("“{}” isn't in this pack", c.name));
        };
        if seen.contains(&c.id) {
            return Err(format!("“{}” is in the list twice", c.name));
        }
        seen.push(c.id.clone());
        let mut kept = orig.clone();
        kept.name = c.name.trim().to_string();
        kept.category = c
            .category
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        kept.description = c
            .description
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        // The settings that go with its kind of control (the kind stays).
        if orig.toggle.is_some() || c.toggle.is_some() {
            kept.toggle = c.toggle.clone().or_else(|| orig.toggle.clone());
        }
        if orig.slider.is_some() {
            kept.slider = c.slider.clone().or_else(|| orig.slider.clone());
        }
        if orig.number.is_some() {
            kept.number = c.number.clone().or_else(|| orig.number.clone());
        }
        if orig.button.is_some() {
            kept.button = c.button.clone().or_else(|| orig.button.clone());
        }
        // A switch of a code patch has no value to keep.
        if matches!(orig.implementation, jb_packs::Impl::Patch(_)) {
            kept.toggle = orig.toggle.clone();
        }
        out.push(kept);
    }
    let mut pack = original.clone();
    pack.cheats = out;
    if let Some(issue) = jb_packs::validate(&pack)
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(friendly_issue(&pack, &issue));
    }
    Ok(pack)
}

/// "cheats[2].slider: min must be below max" → "Infinite gold: min must be below max".
fn friendly_issue(pack: &Pack, issue: &jb_packs::Issue) -> String {
    let name = issue
        .path
        .strip_prefix("cheats[")
        .and_then(|r| r.split(']').next())
        .and_then(|i| i.parse::<usize>().ok())
        .and_then(|i| pack.cheats.get(i))
        .map(|c| c.name.clone());
    match name {
        Some(n) => format!("{n}: {}", issue.message),
        None => issue.to_string(),
    }
}

/// The player's notes, favourites and keys for the edited pack.
pub fn edited_settings(
    mut s: GameSettings,
    pack: &Pack,
    edit: &BookEdit,
) -> Result<GameSettings, String> {
    let ids: Vec<&str> = pack.cheats.iter().map(|c| c.id.as_str()).collect();
    s.notes.clear();
    for (id, note) in &edit.notes {
        if ids.contains(&id.as_str()) {
            s.set_note(id, note);
        }
    }
    s.favourites.clear();
    for id in &edit.favourites {
        if ids.contains(&id.as_str()) {
            s.set_favourite(id, true);
        }
    }
    s.keys.clear();
    s.pads.retain(|id, _| ids.contains(&id.as_str()));
    for (id, key) in &edit.keys {
        if ids.contains(&id.as_str()) {
            s.set_key(pack, id, Some(key))?;
        }
    }
    for p in &mut s.presets {
        p.cheats.retain(|c| ids.contains(&c.id.as_str()));
    }
    Ok(s)
}

fn write_pack(path: &Path, pack: &Pack) -> CmdResult<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let io = |e: std::io::Error| CommandError::new("io", format!("{}: {e}", dir.display()));
    std::fs::create_dir_all(dir).map_err(io)?;
    if path.is_file() {
        let _ = std::fs::copy(path, dir.join("pack.json.bak"));
    }
    let text = serde_json::to_string_pretty(pack)
        .map_err(|e| CommandError::new("io", e.to_string()))?
        + "\n";
    let tmp = dir.join("pack.json.tmp");
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

/// Saves the player's edits to a pack's cheats.
#[tauri::command]
pub async fn book_save(
    key: String,
    edit: BookEdit,
    state: State<'_, AppState>,
) -> CmdResult<BookSaved> {
    let (found, original) = find(&key)?;
    let pack = apply_edit(&original, &edit.cheats).map_err(bad)?;
    let dir = settings_dir(&state);
    let s = dir
        .as_deref()
        .map(|d| settings::load(d, &pack.game.id))
        .unwrap_or_default();
    let s = edited_settings(s, &pack, &edit).map_err(bad)?;

    let (path, copied) = match &found.path {
        Some(p) => (p.clone(), false),
        None => {
            let p = store::user_dir()
                .ok_or_else(|| CommandError::new("noAppData", "%APPDATA% is not set"))?
                .join(&pack.game.id)
                .join("pack.json");
            if p.exists() {
                return Err(bad(
                    "you already have your own copy of this pack (in Packs): edit that one",
                ));
            }
            (p, true)
        }
    };
    write_pack(&path, &pack)?;
    if let Some(d) = &dir {
        settings::save(d, &pack.game.id, &s)
            .map_err(|e| CommandError::new("io", format!("couldn't save your settings: {e}")))?;
    }
    let new_key = store::all()
        .into_iter()
        .find(|f| f.path.as_deref() == Some(path.as_path()))
        .map_or_else(|| path.display().to_string(), |f| f.key);
    // In use in Cheats: load it again so the changes apply (this turns its cheats off).
    let reloaded = is_loaded(&state, &key) && load_key(&state, new_key.clone()).is_ok();
    Ok(BookSaved {
        key: new_key,
        copied,
        reloaded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> Pack {
        let (p, _) = jb_packs::parse(store::BUNDLED[0].1).unwrap();
        p
    }

    #[test]
    fn edits_only_what_a_player_may_change() {
        let original = pack();
        let mut cheats = original.cheats.clone();
        let first = cheats[0].clone();
        cheats[0].name = "  Never die  ".into();
        cheats[0].category = Some("  ".into());
        cheats[0].description = Some("Mine now".into());
        // Trying to change where it looks in the game, or what kind of control it is, does nothing.
        cheats[0].ui = jb_packs::Ui::Number;
        if let Some(other) = original
            .cheats
            .iter()
            .find(|c| c.implementation != first.implementation)
        {
            cheats[0].implementation = other.implementation.clone();
        }
        cheats.remove(1);
        let edited = apply_edit(&original, &cheats).unwrap();
        assert_eq!(edited.cheats.len(), original.cheats.len() - 1);
        let c = &edited.cheats[0];
        assert_eq!(c.name, "Never die");
        assert_eq!(c.category, None);
        assert_eq!(c.description.as_deref(), Some("Mine now"));
        assert_eq!(c.ui, first.ui);
        assert_eq!(c.implementation, first.implementation);
        assert_eq!(edited.game, original.game);
        assert_eq!(edited.meta, original.meta);
    }

    #[test]
    fn refuses_new_cheats_doubles_and_broken_values() {
        let original = pack();
        let mut extra = original.cheats.clone();
        extra[0].id = "made_up".into();
        assert!(
            apply_edit(&original, &extra)
                .unwrap_err()
                .contains("isn't in this pack")
        );

        let mut twice = original.cheats.clone();
        twice.push(twice[0].clone());
        assert!(apply_edit(&original, &twice).unwrap_err().contains("twice"));

        let mut empty = original.cheats.clone();
        empty[0].name = "   ".into();
        let e = apply_edit(&original, &empty).unwrap_err();
        assert!(!e.is_empty(), "a cheat needs a name");

        if let Some(i) = original.cheats.iter().position(|c| c.slider.is_some()) {
            let mut wrong = original.cheats.clone();
            let s = wrong[i].slider.as_mut().unwrap();
            std::mem::swap(&mut s.min, &mut s.max);
            let e = apply_edit(&original, &wrong).unwrap_err();
            assert!(e.starts_with(&original.cheats[i].name), "{e}");
        }
    }

    #[test]
    fn keeps_notes_favourites_and_keys_of_kept_cheats() {
        let p = pack();
        let toggle = p
            .cheats
            .iter()
            .find(|c| c.ui == jb_packs::Ui::Toggle)
            .unwrap();
        let mut s = GameSettings::default();
        s.set_note("gone", "old");
        let edit = BookEdit {
            cheats: p.cheats.clone(),
            notes: [
                (toggle.id.clone(), "works after level 2".to_string()),
                ("gone".into(), "x".into()),
            ]
            .into(),
            favourites: vec![toggle.id.clone(), "gone".into()],
            keys: [(toggle.id.clone(), "Ctrl+F9".to_string())].into(),
        };
        let s = edited_settings(s, &p, &edit).unwrap();
        assert_eq!(
            s.notes.get(&toggle.id).map(String::as_str),
            Some("works after level 2")
        );
        assert!(!s.notes.contains_key("gone"));
        assert_eq!(s.favourites, vec![toggle.id.clone()]);
        assert_eq!(s.keys.get(&toggle.id).map(String::as_str), Some("Ctrl+F9"));

        let bad_key = BookEdit {
            cheats: p.cheats.clone(),
            notes: BTreeMap::new(),
            favourites: vec![],
            keys: [(toggle.id.clone(), "NotAKey!".to_string())].into(),
        };
        assert!(edited_settings(GameSettings::default(), &p, &bad_key).is_err());
    }
}
