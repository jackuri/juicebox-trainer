//! Instant engine cheats (v1.26.0): for an attached Unreal Engine 4.23+ / 5
//! game, read the engine's own names and save the cheats found as the
//! game's "instant cheats" pack (`%APPDATA%\JuiceBox Trainer\packs\engine-<exe>`),
//! then load it. Reading only; the pack is ordinary declarative data.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use jb_core::engine::{Engine, detect};
use jb_core::{Arch, engine_cheats};
use jb_packs::engine::{EngineGame, engine_pack, pack_id};
use jb_packs::{Severity, store};

use crate::commands::{AppState, CmdResult, CommandError, live_process, load_key, loaded_key};

/// Whether the attached game can get instant cheats.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineSupport {
    /// `Unreal Engine 5.7`, `Unity 2022.3.5f1`, …
    pub engine: Option<String>,
    pub supported: bool,
    /// Why not, in plain words (None when supported or no engine is known).
    pub why: Option<String>,
    /// The game's instant cheats pack, if made before.
    pub pack_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineCheatsMade {
    pub key: String,
    pub pack_name: String,
    /// The cheats made.
    pub cheats: Vec<String>,
    /// The usual cheats that weren't found in this game.
    pub not_found: Vec<String>,
    /// The pack is now loaded in the Cheats tab.
    pub loaded: bool,
}

fn fail(m: impl Into<String>) -> CommandError {
    CommandError::new("engineCheats", m)
}

/// `5.7.1` / `4.27` → can it be read (4.23 and newer).
fn unreal_version_ok(version: Option<&str>) -> bool {
    let Some(v) = version else { return true };
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
    match (parts.next().flatten(), parts.next().flatten()) {
        (Some(4), Some(minor)) => minor >= 23,
        (Some(major), _) => major >= 5,
        _ => true,
    }
}

fn engine_label(name: &str, version: Option<&str>) -> String {
    match version {
        Some(v) => format!("{name} {v}"),
        None => name.to_string(),
    }
}

fn support_for(exe: &str, arch: Arch) -> EngineSupport {
    let info = detect(Path::new(exe));
    let engine =
        (info.engine != Engine::Native).then(|| engine_label(info.name, info.version.as_deref()));
    let (supported, why) = match info.engine {
        Engine::Unreal if arch != Arch::X64 => (
            false,
            Some("Instant cheats need the 64-bit version of the game.".to_string()),
        ),
        Engine::Unreal if !unreal_version_ok(info.version.as_deref()) => (
            false,
            Some(format!(
                "This game uses {}. Instant cheats work with Unreal Engine 4.23 and newer.",
                engine.clone().unwrap_or_default()
            )),
        ),
        Engine::Unreal => (true, None),
        Engine::UnityMono | Engine::UnityIl2cpp => (
            false,
            Some("Instant cheats for Unity games are coming in a later version.".to_string()),
        ),
        _ => (false, None),
    };
    let exe_name = Path::new(exe)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    EngineSupport {
        engine,
        supported,
        why,
        pack_key: pack_path(&exe_name).and_then(|p| key_of(&p)),
    }
}

fn pack_path(exe_name: &str) -> Option<std::path::PathBuf> {
    Some(store::user_dir()?.join(pack_id(exe_name)).join("pack.json"))
}

fn key_of(path: &Path) -> Option<String> {
    path.exists().then(|| {
        store::all()
            .into_iter()
            .find(|f| f.path.as_deref() == Some(path))
            .map_or_else(|| path.display().to_string(), |f| f.key)
    })
}

/// A game name for the pack: one line, up to 80 characters.
fn clean_name(name: &str, exe_name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
    let clean = clean.trim();
    if clean.is_empty() {
        exe_name
            .trim_end_matches(".exe")
            .trim_end_matches("-Win64-Shipping")
            .to_string()
    } else {
        clean.to_string()
    }
}

#[tauri::command]
pub async fn engine_cheats_support(state: State<'_, AppState>) -> CmdResult<EngineSupport> {
    let p = live_process(&state)?;
    let Some(exe) = p.info().exe_path.clone() else {
        return Ok(EngineSupport {
            engine: None,
            supported: false,
            why: None,
            pack_key: None,
        });
    };
    Ok(support_for(&exe, p.arch()))
}

/// Reads the attached game's engine and saves its instant cheats. Can take
/// a few minutes the first time on a big game (nothing in it is changed).
#[tauri::command]
pub async fn engine_cheats_make(
    game_name: String,
    state: State<'_, AppState>,
) -> CmdResult<EngineCheatsMade> {
    let p = live_process(&state)?;
    let exe = p
        .info()
        .exe_path
        .clone()
        .ok_or_else(|| fail("can't see where the game is installed"))?;
    let support = support_for(&exe, p.arch());
    if !support.supported {
        return Err(fail(support.why.unwrap_or_else(|| {
            "instant cheats work with Unreal Engine games for now".into()
        })));
    }
    let main = p.main_module()?;
    let reader_process = Arc::clone(&p);
    let found = tauri::async_runtime::spawn_blocking(move || {
        let reader = jb_core::unreal::Reader::connect(reader_process)?;
        engine_cheats::find_unreal(&reader)
    })
    .await
    .map_err(|e| fail(format!("the search stopped: {e}")))??;
    if found.global < main.base || found.global >= main.end() {
        return Err(fail("the engine's start point isn't inside the game's exe"));
    }
    if found.cheats.is_empty() {
        return Err(fail("no instant cheats were found in this game"));
    }

    let exe_name = p.info().name.clone();
    let info = detect(Path::new(&exe));
    let game = EngineGame {
        name: clean_name(&game_name, &exe_name),
        exe: exe_name.clone(),
        sha256: p.exe_sha256(),
        engine: engine_label(info.name, info.version.as_deref()),
        created: jb_packs::share::today(),
    };
    let pack = engine_pack(&game, found.global - main.base, &found);
    if let Some(issue) = jb_packs::validate(&pack)
        .into_iter()
        .find(|i| i.severity == Severity::Error)
    {
        return Err(fail(format!(
            "the cheats didn't pass the pack check ({issue})"
        )));
    }
    let text = serde_json::to_string_pretty(&pack).map_err(|e| fail(e.to_string()))?;

    let path = pack_path(&exe_name)
        .ok_or_else(|| CommandError::new("noAppData", "%APPDATA% is not set"))?;
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let io = |e: std::io::Error| CommandError::new("io", format!("{}: {e}", dir.display()));
    std::fs::create_dir_all(&dir).map_err(io)?;
    if path.exists() {
        let _ = std::fs::copy(&path, dir.join("pack.json.bak"));
    }
    let tmp = dir.join("pack.json.tmp");
    std::fs::write(&tmp, &text).map_err(io)?;
    std::fs::rename(&tmp, &path).map_err(io)?;

    let key = key_of(&path).unwrap_or_else(|| path.display().to_string());
    let loaded = match loaded_key(&state)? {
        None => load_key(&state, key.clone()).is_ok(),
        Some(k) if k == key => load_key(&state, key.clone()).is_ok(),
        Some(_) => false,
    };
    Ok(EngineCheatsMade {
        key,
        pack_name: pack.game.name.clone(),
        cheats: pack.cheats.iter().map(|c| c.name.clone()).collect(),
        not_found: found.not_found,
        loaded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unreal_versions() {
        assert!(unreal_version_ok(Some("5.7.1")));
        assert!(unreal_version_ok(Some("4.27")));
        assert!(unreal_version_ok(Some("4.23.1")));
        assert!(!unreal_version_ok(Some("4.19")));
        assert!(unreal_version_ok(None));
    }

    #[test]
    fn game_names() {
        assert_eq!(
            clean_name("  Species: Unknown ", "x.exe"),
            "Species: Unknown"
        );
        assert_eq!(clean_name("a\u{7}b", "x.exe"), "ab");
        assert_eq!(clean_name("", "Hero-Win64-Shipping.exe"), "Hero");
    }

    #[test]
    fn unknown_games_are_not_supported() {
        let s = support_for("C:\\nowhere\\game.exe", Arch::X64);
        assert!(!s.supported);
        assert_eq!(s.engine, None);
    }
}
