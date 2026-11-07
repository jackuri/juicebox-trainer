//! The player's own settings for a game's pack: presets ("Story mode"),
//! favourites, notes, and their own keyboard and controller keys.
//!
//! Kept in `%APPDATA%\OpenTrainer\settings\<game-id>.json`, apart from the
//! pack, so a pack update or repair keeps them. Plain data; nothing in it is
//! run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::pad::PadChord;
use crate::schema::Ui;
use crate::{CheatState, Hotkey, Pack, Trainer};

/// Longest preset name.
pub const MAX_PRESET_NAME: usize = 40;
/// Most presets per game.
pub const MAX_PRESETS: usize = 20;
/// Longest note per cheat.
pub const MAX_NOTE: usize = 500;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSettings {
    #[serde(default)]
    pub presets: Vec<Preset>,
    /// Applied when the game is started from the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_preset: Option<String>,
    #[serde(default)]
    pub favourites: Vec<String>,
    #[serde(default)]
    pub notes: BTreeMap<String, String>,
    /// Cheat id → the player's key (canonical text). An empty string means
    /// "no key" (the pack's key is turned off).
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    /// Cheat id → controller chord.
    #[serde(default)]
    pub pads: BTreeMap<String, PadChord>,
}

/// A saved set of cheats: these on, these values held, everything else off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub cheats: Vec<PresetCheat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetCheat {
    pub id: String,
    /// A slider / number value to set (and hold, if the cheat holds).
    /// Without it, the cheat is a toggle turned on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// `%APPDATA%\OpenTrainer\settings`.
pub fn default_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("OpenTrainer").join("settings"))
}

fn file(dir: &Path, game_id: &str) -> PathBuf {
    // Game ids are validated slugs; keep only safe characters anyway.
    let safe: String = game_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .collect();
    dir.join(format!("{safe}.json"))
}

/// Loads a game's settings. Missing or unreadable files give empty settings
/// (a broken file is kept as `.json.bad` so nothing is lost).
pub fn load(dir: &Path, game_id: &str) -> GameSettings {
    let path = file(dir, game_id);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return GameSettings::default();
    };
    match serde_json::from_str(&text) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(path = %path.display(), "settings file unreadable, starting fresh: {e}");
            let _ = std::fs::rename(&path, path.with_extension("json.bad"));
            GameSettings::default()
        }
    }
}

/// Saves a game's settings (written to a temp file, then swapped in).
pub fn save(dir: &Path, game_id: &str, settings: &GameSettings) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = file(dir, game_id);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, text + "\n")?;
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn has_key(pack: &Pack, id: &str) -> bool {
    pack.cheats
        .iter()
        .any(|c| c.id == id && matches!(c.ui, Ui::Toggle | Ui::Button))
}

impl GameSettings {
    /// Keyboard keys to register: the player's, else the pack's. Toggles and
    /// buttons only.
    pub fn hotkeys(&self, pack: &Pack) -> Vec<(String, Hotkey)> {
        pack.cheats
            .iter()
            .filter(|c| matches!(c.ui, Ui::Toggle | Ui::Button))
            .filter_map(|c| {
                let text = match self.keys.get(&c.id) {
                    Some(own) => own.as_str(),
                    None => c.hotkey.as_deref()?,
                };
                Some((c.id.clone(), Hotkey::parse(text).ok()?))
            })
            .collect()
    }

    /// Controller chords to watch.
    pub fn pad_chords(&self, pack: &Pack) -> Vec<(String, PadChord)> {
        self.pads
            .iter()
            .filter(|(id, _)| has_key(pack, id))
            .map(|(id, c)| (id.clone(), *c))
            .collect()
    }

    /// Sets the player's key for a cheat. `""` turns its key off; `None`
    /// goes back to the pack's key.
    pub fn set_key(&mut self, pack: &Pack, id: &str, key: Option<&str>) -> Result<(), String> {
        if !has_key(pack, id) {
            return Err(format!(
                "'{id}' can't have a key (only toggles and buttons can)"
            ));
        }
        match key.map(str::trim) {
            None => {
                self.keys.remove(id);
            }
            Some("") => {
                self.keys.insert(id.to_string(), String::new());
            }
            Some(text) => {
                let hk = Hotkey::parse(text)?;
                if let Some((other, _)) = self
                    .hotkeys(pack)
                    .into_iter()
                    .find(|(other, k)| other != id && *k == hk)
                {
                    let name = pack
                        .cheats
                        .iter()
                        .find(|c| c.id == other)
                        .map_or(other.as_str(), |c| c.name.as_str());
                    return Err(format!("{hk} is already the key for {name}"));
                }
                self.keys.insert(id.to_string(), hk.to_string());
            }
        }
        Ok(())
    }

    /// Sets (or with `None` / `""` removes) a cheat's controller chord.
    pub fn set_pad(&mut self, pack: &Pack, id: &str, chord: Option<&str>) -> Result<(), String> {
        if !has_key(pack, id) {
            return Err(format!(
                "'{id}' can't have a controller combo (only toggles and buttons can)"
            ));
        }
        match chord.map(str::trim).filter(|c| !c.is_empty()) {
            None => {
                self.pads.remove(id);
            }
            Some(text) => {
                let c = PadChord::parse(text)?;
                if let Some((other, _)) = self.pads.iter().find(|(o, p)| *o != id && **p == c) {
                    return Err(format!("{c} is already the combo for {other}"));
                }
                self.pads.insert(id.to_string(), c);
            }
        }
        Ok(())
    }

    pub fn set_note(&mut self, id: &str, note: &str) {
        let note: String = note.trim().chars().take(MAX_NOTE).collect();
        if note.is_empty() {
            self.notes.remove(id);
        } else {
            self.notes.insert(id.to_string(), note);
        }
    }

    pub fn set_favourite(&mut self, id: &str, on: bool) {
        self.favourites.retain(|f| f != id);
        if on {
            self.favourites.push(id.to_string());
        }
    }

    pub fn preset(&self, name: &str) -> Option<&Preset> {
        self.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Saves `cheats` as preset `name`, replacing one with the same name.
    pub fn save_preset(&mut self, name: &str, cheats: Vec<PresetCheat>) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("give the preset a name".into());
        }
        if name.chars().count() > MAX_PRESET_NAME {
            return Err(format!(
                "preset names can be up to {MAX_PRESET_NAME} characters"
            ));
        }
        let preset = Preset {
            name: name.to_string(),
            cheats,
        };
        if let Some(p) = self
            .presets
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(name))
        {
            *p = preset;
        } else {
            if self.presets.len() >= MAX_PRESETS {
                return Err(format!("a game can have up to {MAX_PRESETS} presets"));
            }
            self.presets.push(preset);
        }
        Ok(())
    }

    pub fn delete_preset(&mut self, name: &str) {
        self.presets.retain(|p| !p.name.eq_ignore_ascii_case(name));
        if self
            .auto_preset
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(name))
        {
            self.auto_preset = None;
        }
    }

    pub fn set_auto_preset(&mut self, name: Option<&str>) -> Result<(), String> {
        match name {
            None => self.auto_preset = None,
            Some(n) => {
                let p = self
                    .preset(n)
                    .ok_or_else(|| format!("no preset named {n}"))?;
                self.auto_preset = Some(p.name.clone());
            }
        }
        Ok(())
    }
}

/// What's on right now, as preset entries: toggles that are on and held
/// slider / number values. Buttons are one-shot and aren't kept.
pub fn capture(pack: &Pack, states: &[CheatState]) -> Vec<PresetCheat> {
    pack.cheats
        .iter()
        .filter_map(|c| {
            let s = states.iter().find(|s| s.id == c.id)?;
            if !s.active {
                return None;
            }
            match c.ui {
                Ui::Toggle => Some(PresetCheat {
                    id: c.id.clone(),
                    value: None,
                }),
                Ui::Slider | Ui::Number => Some(PresetCheat {
                    id: c.id.clone(),
                    value: Some(s.value.clone()?),
                }),
                Ui::Button => None,
            }
        })
        .collect()
}

/// Applies a preset: its cheats on, every other toggle off and other held
/// values released. Returns the entries that couldn't be applied yet (e.g.
/// the player isn't in a level, so a value can't be found), with why.
pub fn apply(
    trainer: &mut Trainer,
    entries: &[PresetCheat],
    others_off: bool,
) -> Vec<(PresetCheat, String)> {
    let pack = trainer.pack().clone();
    if others_off {
        let states = trainer.states().unwrap_or_default();
        for c in &pack.cheats {
            if entries.iter().any(|e| e.id == c.id) {
                continue;
            }
            let active = states.iter().any(|s| s.id == c.id && s.active);
            if !active {
                continue;
            }
            let _ = match c.ui {
                Ui::Toggle => trainer.set_active(&c.id, false),
                Ui::Slider | Ui::Number => trainer.release(&c.id),
                Ui::Button => Ok(()),
            };
        }
    }
    let mut pending = Vec::new();
    for e in entries {
        let Some(c) = pack.cheats.iter().find(|c| c.id == e.id) else {
            // The pack changed and this cheat is gone; skip it quietly.
            continue;
        };
        let r = match (c.ui, &e.value) {
            (Ui::Toggle, _) => trainer.set_active(&e.id, true),
            (Ui::Slider | Ui::Number, Some(v)) => trainer.set_value(&e.id, v),
            _ => continue,
        };
        if let Err(err) = r {
            pending.push((e.clone(), err.to_string()));
        }
    }
    pending
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack() -> Pack {
        let text = include_str!("../../../packs/examples/testbed-native/pack.json");
        crate::parse(text).unwrap().0
    }

    fn dir(name: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/tmp")
            .join(name);
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn own_keys_replace_the_packs_and_conflicts_are_refused() {
        let p = pack();
        let mut s = GameSettings::default();
        let keys = |s: &GameSettings| -> Vec<(String, String)> {
            s.hotkeys(&p)
                .into_iter()
                .map(|(i, k)| (i, k.to_string()))
                .collect()
        };
        assert!(keys(&s).contains(&("inf_health".into(), "F1".into())));
        s.set_key(&p, "inf_health", Some("ctrl+num1")).unwrap();
        assert!(keys(&s).contains(&("inf_health".into(), "Ctrl+Num1".into())));
        assert!(
            s.set_key(&p, "inf_ammo", Some("Ctrl+Num1"))
                .unwrap_err()
                .contains("Infinite")
        );
        // F1 is free again now.
        s.set_key(&p, "inf_ammo", Some("F1")).unwrap();
        // Off, then back to the pack's.
        s.set_key(&p, "add_gold", Some("")).unwrap();
        assert!(!keys(&s).iter().any(|(i, _)| i == "add_gold"));
        s.set_key(&p, "add_gold", None).unwrap();
        assert!(keys(&s).contains(&("add_gold".into(), "F4".into())));
        assert!(
            s.set_key(&p, "speed", Some("F9")).is_err(),
            "sliders have no key"
        );
        assert!(
            s.set_key(&p, "inf_health", Some("Q")).is_err(),
            "bare letters are refused"
        );
    }

    #[test]
    fn controller_combos() {
        let p = pack();
        let mut s = GameSettings::default();
        s.set_pad(&p, "inf_health", Some("LB+RB+DPadUp")).unwrap();
        assert!(
            s.set_pad(&p, "inf_ammo", Some("rb+lb+up"))
                .unwrap_err()
                .contains("already")
        );
        assert!(s.set_pad(&p, "inf_ammo", Some("A")).is_err());
        assert_eq!(s.pad_chords(&p).len(), 1);
        s.set_pad(&p, "inf_health", Some("")).unwrap();
        assert!(s.pad_chords(&p).is_empty());
    }

    #[test]
    fn presets_save_replace_delete_and_auto() {
        let mut s = GameSettings::default();
        let on = |id: &str| PresetCheat {
            id: id.into(),
            value: None,
        };
        s.save_preset("Story mode", vec![on("inf_health")]).unwrap();
        s.save_preset("story MODE", vec![on("inf_ammo")]).unwrap();
        assert_eq!(s.presets.len(), 1);
        assert_eq!(s.presets[0].cheats[0].id, "inf_ammo");
        assert!(s.save_preset("  ", vec![]).is_err());
        s.set_auto_preset(Some("story mode")).unwrap();
        assert_eq!(s.auto_preset.as_deref(), Some("story MODE"));
        assert!(s.set_auto_preset(Some("nope")).is_err());
        s.delete_preset("Story Mode");
        assert!(s.presets.is_empty() && s.auto_preset.is_none());
    }

    #[test]
    fn notes_favourites_and_files() {
        let d = dir("settings-files");
        let mut s = GameSettings::default();
        s.set_note("inf_health", "  turn off before the boss  ");
        s.set_favourite("inf_health", true);
        s.set_favourite("inf_health", true);
        assert_eq!(s.favourites, ["inf_health"]);
        save(&d, "testbed-native", &s).unwrap();
        let back = load(&d, "testbed-native");
        assert_eq!(back, s);
        assert_eq!(back.notes["inf_health"], "turn off before the boss");
        // A broken file is set aside, not lost.
        std::fs::write(d.join("broken.json"), "{ nope").unwrap();
        assert_eq!(load(&d, "broken"), GameSettings::default());
        assert!(d.join("broken.json.bad").is_file());
        // Unsafe ids can't leave the folder.
        assert_eq!(file(&d, "../x"), d.join("x.json"));
    }
}
