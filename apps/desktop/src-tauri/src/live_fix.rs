//! The AI helper while the player plays (v1.30.0): it fixes a cheat or adds
//! one to the loaded pack while the player's other cheats stay on, then the
//! pack is loaded again and those cheats are turned back on.
//!
//! The helper attaches through `jb-mcp` like always (guarded, logged,
//! revertible) and keeps a journal of its own, so its tests and the app's
//! cheats never undo each other. The tools refuse to load or change the
//! cheats that are on, and save the fix into the player's pack.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jb_packs::store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::commands::{
    AppState, CmdResult, CommandError, OnCheat, live_snapshot, load_key, loaded_key, turn_back_on,
};

/// What the player asked, from the Cheats view.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRequest {
    /// The loaded pack (its key from `packs_list`).
    pub pack_key: String,
    /// The cheat to fix, or none for new cheats.
    #[serde(default)]
    pub fix: Option<String>,
    /// What's wrong with it, in the player's words.
    #[serde(default)]
    pub problem: Option<String>,
}

/// A live session in progress.
#[derive(Debug)]
pub struct LivePlan {
    pid: u32,
    /// The player's pack file the fix goes into.
    file: PathBuf,
    /// The key to load it by afterwards (a bundled pack becomes a copy).
    key: String,
    before: String,
    on: Vec<OnCheat>,
    fix: Option<String>,
    /// The cheat being fixed, if it was on.
    fixed_was_on: Option<OnCheat>,
}

/// How it ended, for the conversation.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LiveDone {
    /// The pack changed and was loaded again.
    pub reloaded: bool,
    /// Cheats turned back on.
    pub back_on: Vec<String>,
    /// What didn't work out, in plain words.
    pub problems: Vec<String>,
}

fn bad(kind: &'static str, m: impl Into<String>) -> CommandError {
    CommandError::new(kind, m)
}

/// Checks the request against the session and turns the cheat being fixed
/// off. Returns the plan and what the helper is told.
pub fn prepare(state: &AppState, pid: u32, req: &LiveRequest) -> CmdResult<(LivePlan, Value)> {
    let loaded = loaded_key(state)?
        .ok_or_else(|| bad("noTrainer", "load the cheats first, then ask the AI"))?;
    if loaded != req.pack_key {
        return Err(bad(
            "noTrainer",
            "other cheats are loaded now: load these, then ask again",
        ));
    }
    let found = store::all()
        .into_iter()
        .find(|f| f.key == loaded)
        .ok_or_else(|| bad("notFound", "the cheats' pack file is gone"))?;
    let text = found.text.clone().map_err(|e| bad("io", e))?;
    let (file, key) = match &found.path {
        Some(p) => (p.clone(), loaded.clone()),
        None => own_copy(&text)?,
    };
    let (pack, on, fixed_was_on) = live_snapshot(state, pid, req.fix.as_deref())?;
    let fixing = req.fix.as_deref().and_then(|id| {
        pack.cheats.iter().find(|c| c.id == id).map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "problem": req.problem.clone().unwrap_or_default(),
            })
        })
    });
    let tell = json!({
        "pack": file.display().to_string(),
        "on": on.iter().map(|c| json!({ "id": c.id, "name": c.name })).collect::<Vec<_>>(),
        "fixing": fixing,
    });
    let before = std::fs::read_to_string(&file).unwrap_or(text);
    Ok((
        LivePlan {
            pid,
            file,
            key,
            before,
            on,
            fix: req.fix.clone(),
            fixed_was_on,
        },
        tell,
    ))
}

/// A bundled pack is copied to the player's packs first, like the cheat book.
fn own_copy(text: &str) -> CmdResult<(PathBuf, String)> {
    let (pack, _) = jb_packs::parse(text).map_err(|e| bad("invalidPack", e.to_string()))?;
    let file = store::user_dir()
        .ok_or_else(|| bad("noAppData", "%APPDATA% is not set"))?
        .join(&pack.game.id)
        .join("pack.json");
    if file.exists() {
        return Err(bad(
            "exists",
            "you have your own copy of these cheats in Packs: load that one, then ask again",
        ));
    }
    let dir = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| bad("io", format!("{}: {e}", dir.display())))?;
    std::fs::write(&file, text).map_err(|e| bad("io", format!("{}: {e}", file.display())))?;
    let key = store::all()
        .into_iter()
        .find(|f| f.path.as_deref() == Some(file.as_path()))
        .map_or_else(|| file.display().to_string(), |f| f.key);
    Ok((file, key))
}

/// After the helper ended: load the pack again if it changed and turn the
/// player's cheats back on.
pub fn finish(state: &AppState, plan: LivePlan) -> LiveDone {
    wait_for_helper(plan.pid);
    let now = std::fs::read_to_string(&plan.file).unwrap_or_default();
    let changed = !now.is_empty() && now != plan.before;
    let mut done = LiveDone {
        reloaded: false,
        back_on: Vec::new(),
        problems: Vec::new(),
    };
    let mut again = plan.on.clone();
    if changed {
        match load_key(state, plan.key.clone()) {
            Ok(view) => {
                done.reloaded = true;
                // The fixed cheat goes on to try it, if it's a toggle or was on.
                if let Some(id) = &plan.fix {
                    let toggle = view
                        .pack
                        .cheats
                        .iter()
                        .any(|c| &c.id == id && c.ui == jb_packs::Ui::Toggle);
                    match &plan.fixed_was_on {
                        Some(c) if c.held.is_some() => again.push(c.clone()),
                        _ if toggle => again.push(OnCheat {
                            id: id.clone(),
                            name: view
                                .pack
                                .cheats
                                .iter()
                                .find(|c| &c.id == id)
                                .map_or_else(|| id.clone(), |c| c.name.clone()),
                            held: None,
                        }),
                        _ => {}
                    }
                }
            }
            Err(e) => {
                done.problems.push(format!(
                    "the fixed cheats couldn't be loaded ({}); load them again on the Trainer tab",
                    e.message
                ));
                return done;
            }
        }
    } else if let Some(c) = plan.fixed_was_on {
        // Nothing saved: put it back as it was.
        again.push(c);
    }
    done.problems = turn_back_on(state, &again);
    done.back_on = again
        .iter()
        .filter(|c| {
            !done
                .problems
                .iter()
                .any(|p| p.starts_with(&format!("{}:", c.name)))
        })
        .map(|c| c.name.clone())
        .collect();
    done
}

/// The helper's tool server puts back its own test changes as it closes:
/// wait (a few seconds at most) until its journal for this game is gone.
fn wait_for_helper(pid: u32) {
    let Some(dir) = jb_core::journal::default_dir() else {
        return;
    };
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until && helper_journal(&dir, pid) {
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
}

fn helper_journal(dir: &Path, pid: u32) -> bool {
    let prefix = format!("{pid}-");
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(&prefix) && name.ends_with("-helper.json")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_only_for_the_helpers_journal_of_this_game() {
        let dir = std::env::temp_dir().join(format!("jb-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("42-7.json"), "{}").unwrap();
        std::fs::write(dir.join("420-7-helper.json"), "{}").unwrap();
        assert!(
            !helper_journal(&dir, 42),
            "the app's own journal or another game's"
        );
        std::fs::write(dir.join("42-7-helper.json"), "{}").unwrap();
        assert!(helper_journal(&dir, 42));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_the_request() {
        let r: LiveRequest = serde_json::from_value(
            json!({ "packKey": "k", "fix": "ammo", "problem": "first gun only" }),
        )
        .unwrap();
        assert_eq!((r.pack_key.as_str(), r.fix.as_deref()), ("k", Some("ammo")));
        let r: LiveRequest = serde_json::from_value(json!({ "packKey": "k" })).unwrap();
        assert!(r.fix.is_none() && r.problem.is_none());
    }
}
