//! Save backup commands and the launch/exit watcher. The real work (finding
//! folders, backups, diff, guarded restore) lives in `ot-saves`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use ot_saves::diff::Side;
use ot_saves::{
    Created, Diff, Game, Library, Limits, Reason, RestoreOptions, RestoreReport, Roots, Store,
    Summary, WatchKind, Watcher,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};

use crate::commands::CommandError;

type CmdResult<T> = Result<T, CommandError>;

/// How often the watcher looks for games starting or closing.
const WATCH_INTERVAL: Duration = Duration::from_secs(3);

impl From<ot_saves::Error> for CommandError {
    fn from(e: ot_saves::Error) -> Self {
        let kind = e.kind();
        let message = e.to_string();
        let verdict = match e {
            ot_saves::Error::Blocked(v) => Some(*v),
            _ => None,
        };
        let mut err = CommandError::new(kind, message);
        err.verdict = verdict;
        err
    }
}

/// The save library and which of its games are running.
#[derive(Debug, Default)]
pub struct SavesState {
    library: Mutex<Option<Library>>,
    open_error: Mutex<Option<String>>,
    watcher: Mutex<Watcher>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl SavesState {
    /// Opens the library in `root` (`%LOCALAPPDATA%\OpenTrainer\saves`).
    pub fn open(root: Option<PathBuf>) -> Self {
        let state = Self::default();
        match root.map(Library::open) {
            Some(Ok(lib)) => *lock(&state.library) = Some(lib),
            Some(Err(e)) => *lock(&state.open_error) = Some(e.to_string()),
            None => *lock(&state.open_error) = Some("can't find %LOCALAPPDATA%".into()),
        }
        state
    }

    fn with_lib<T>(&self, f: impl FnOnce(&mut Library) -> CmdResult<T>) -> CmdResult<T> {
        let mut guard = lock(&self.library);
        match guard.as_mut() {
            Some(lib) => f(lib),
            None => Err(CommandError::new(
                "savesUnavailable",
                format!(
                    "save backups are unavailable: {}",
                    lock(&self.open_error).clone().unwrap_or_default()
                ),
            )),
        }
    }

    /// The game and its backup store, so slow file work runs without the lock.
    fn game_and_store(&self, id: &str) -> CmdResult<(Game, Store)> {
        self.with_lib(|lib| Ok((lib.get(id)?.clone(), lib.store(id)?)))
    }

    /// Adds the attached game to the library (if new) and backs up its saves
    /// before any cheat is used. Never fails the attach; problems are logged.
    pub fn on_attach(&self, exe: &str) {
        let game = self.with_lib(|lib| {
            let known = lib.find_exe(exe).is_some();
            let g = lib.add(Path::new(exe), &Roots::current())?;
            Ok((g.clone(), lib.store(&g.id)?, known))
        });
        match game {
            Ok((g, store, known)) => {
                if !known {
                    info!(game = %g.name, folders = g.folders.len(), "added to save backups");
                }
                if !g.folders.is_empty()
                    && let Err(e) =
                        store.create(&g.folder_paths(), Reason::Launch, "", Limits::default())
                {
                    warn!(game = %g.name, "backup on attach failed: {e}");
                }
            }
            Err(e) => warn!(%exe, "could not add to save backups: {}", e.message),
        }
    }

    /// One watcher round: backs up games that started or closed. Returns
    /// whether anything changed.
    pub fn watch_once(&self, processes: &[(u32, Option<String>)]) -> bool {
        let games: Vec<(String, String)> = self
            .with_lib(|lib| {
                Ok(lib
                    .games()
                    .iter()
                    .map(|g| (g.id.clone(), g.exe.clone()))
                    .collect())
            })
            .unwrap_or_default();
        let events = lock(&self.watcher).tick(&games, processes);
        for e in &events {
            let Ok((game, store)) = self.game_and_store(&e.game_id) else {
                continue;
            };
            if game.folders.is_empty() {
                continue;
            }
            let reason = match e.kind {
                WatchKind::Started => Reason::Launch,
                WatchKind::Stopped => Reason::Exit,
            };
            match store.create(&game.folder_paths(), reason, "", Limits::default()) {
                Ok(c) if c.new => info!(game = %game.name, ?reason, "saves backed up"),
                Ok(_) => {}
                Err(e) => warn!(game = %game.name, ?reason, "backup failed: {e}"),
            }
        }
        !events.is_empty()
    }
}

/// Runs the watcher on its own thread for the life of the app.
pub fn start_watcher(app: AppHandle) {
    std::thread::Builder::new()
        .name("save-watcher".into())
        .spawn(move || {
            loop {
                std::thread::sleep(WATCH_INTERVAL);
                let processes: Vec<(u32, Option<String>)> = match ot_core::list_processes() {
                    Ok(list) => list.into_iter().map(|p| (p.pid, p.exe_path)).collect(),
                    Err(e) => {
                        warn!("save watcher: {e}");
                        continue;
                    }
                };
                if app.state::<SavesState>().watch_once(&processes) {
                    let _ = app.emit("saves-changed", ());
                }
            }
        })
        .expect("start the save watcher");
}

// ----- commands -----------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    #[serde(flatten)]
    pub game: Game,
    pub running: bool,
    pub cloud_synced: bool,
    pub backups: usize,
    pub latest_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavesOverview {
    pub games: Vec<GameView>,
    pub steam_running: bool,
    /// Where backups are kept.
    pub root: Option<String>,
}

fn view(state: &SavesState, game: Game, store: &Store) -> GameView {
    let list = store.list().unwrap_or_default();
    GameView {
        running: lock(&state.watcher).is_running(&game.id),
        cloud_synced: game.cloud_synced(),
        backups: list.len(),
        latest_ms: list.first().map(|s| s.created_ms),
        game,
    }
}

fn view_of(state: &SavesState, id: &str) -> CmdResult<GameView> {
    let (game, store) = state.game_and_store(id)?;
    Ok(view(state, game, &store))
}

#[tauri::command]
pub async fn saves_overview(state: State<'_, SavesState>) -> CmdResult<SavesOverview> {
    let (games, root) = state.with_lib(|lib| {
        let games: Vec<(Game, Store)> = lib
            .games()
            .iter()
            .map(|g| Ok((g.clone(), lib.store(&g.id)?)))
            .collect::<CmdResult<_>>()?;
        Ok((games, lib.root().to_string_lossy().into_owned()))
    })?;
    let steam_running = ot_core::list_processes()
        .map(|l| l.iter().any(|p| p.name.eq_ignore_ascii_case("steam.exe")))
        .unwrap_or(false);
    Ok(SavesOverview {
        games: games
            .into_iter()
            .map(|(g, s)| view(&state, g, &s))
            .collect(),
        steam_running,
        root: Some(root),
    })
}

fn add_exe(state: &SavesState, exe: &str) -> CmdResult<GameView> {
    let id = state.with_lib(|lib| Ok(lib.add(Path::new(exe), &Roots::current())?.id))?;
    view_of(state, &id)
}

/// Adds a running program by its process id.
#[tauri::command]
pub async fn saves_add_process(pid: u32, state: State<'_, SavesState>) -> CmdResult<GameView> {
    let exe = ot_core::list_processes()?
        .into_iter()
        .find(|p| p.pid == pid)
        .and_then(|p| p.exe_path)
        .ok_or_else(|| CommandError::new("noExe", "can't get that program's file path"))?;
    add_exe(&state, &exe)
}

/// Adds a game by the full path of its .exe.
#[tauri::command]
pub async fn saves_add_exe(path: String, state: State<'_, SavesState>) -> CmdResult<GameView> {
    let path = path.trim().trim_matches('"').to_string();
    if !path.to_ascii_lowercase().ends_with(".exe") {
        return Err(CommandError::new(
            "badExe",
            "enter the full path of the game's .exe file",
        ));
    }
    add_exe(&state, &path)
}

#[tauri::command]
pub async fn saves_remove_game(
    id: String,
    delete_backups: bool,
    state: State<'_, SavesState>,
) -> CmdResult<()> {
    state.with_lib(|lib| Ok(lib.remove(&id, delete_backups)?))
}

#[tauri::command]
pub async fn saves_relocate(id: String, state: State<'_, SavesState>) -> CmdResult<GameView> {
    state.with_lib(|lib| {
        lib.relocate(&id, &Roots::current())?;
        Ok(())
    })?;
    view_of(&state, &id)
}

#[tauri::command]
pub async fn saves_add_folder(
    id: String,
    path: String,
    state: State<'_, SavesState>,
) -> CmdResult<GameView> {
    let path = path.trim().trim_matches('"').to_string();
    state.with_lib(|lib| {
        lib.add_folder(&id, Path::new(&path))?;
        Ok(())
    })?;
    view_of(&state, &id)
}

#[tauri::command]
pub async fn saves_remove_folder(
    id: String,
    path: String,
    state: State<'_, SavesState>,
) -> CmdResult<GameView> {
    state.with_lib(|lib| {
        lib.remove_folder(&id, &path)?;
        Ok(())
    })?;
    view_of(&state, &id)
}

#[tauri::command]
pub async fn saves_backups(id: String, state: State<'_, SavesState>) -> CmdResult<Vec<Summary>> {
    let (_, store) = state.game_and_store(&id)?;
    Ok(store.list()?)
}

#[tauri::command]
pub async fn saves_backup_now(
    id: String,
    note: String,
    state: State<'_, SavesState>,
) -> CmdResult<Created> {
    let (game, store) = state.game_and_store(&id)?;
    Ok(store.create(
        &game.folder_paths(),
        Reason::Manual,
        &note,
        Limits::default(),
    )?)
}

#[tauri::command]
pub async fn saves_delete_backup(
    id: String,
    backup: String,
    state: State<'_, SavesState>,
) -> CmdResult<()> {
    let (_, store) = state.game_and_store(&id)?;
    Ok(store.delete(&backup)?)
}

#[tauri::command]
pub async fn saves_set_note(
    id: String,
    backup: String,
    note: String,
    state: State<'_, SavesState>,
) -> CmdResult<Summary> {
    let (_, store) = state.game_and_store(&id)?;
    Ok(store.set_note(&backup, &note)?)
}

/// Compares two backups; `None` means the saves as they are now.
#[tauri::command]
pub async fn saves_diff(
    id: String,
    from: Option<String>,
    to: Option<String>,
    state: State<'_, SavesState>,
) -> CmdResult<Diff> {
    let (game, store) = state.game_and_store(&id)?;
    fn side(s: &Option<String>) -> Side<'_> {
        s.as_deref().map_or(Side::Current, Side::Snapshot)
    }
    Ok(ot_saves::diff(
        &store,
        &game.folder_paths(),
        side(&from),
        side(&to),
    )?)
}

#[tauri::command]
pub async fn saves_restore(
    id: String,
    backup: String,
    remove_extra: bool,
    state: State<'_, SavesState>,
) -> CmdResult<RestoreReport> {
    let (game, store) = state.game_and_store(&id)?;
    Ok(ot_saves::restore(
        &game,
        &store,
        &backup,
        RestoreOptions { remove_extra },
    )?)
}

/// Opens one of the game's save folders in Explorer.
#[tauri::command]
pub async fn saves_open_folder(
    id: String,
    path: String,
    state: State<'_, SavesState>,
) -> CmdResult<()> {
    let (game, _) = state.game_and_store(&id)?;
    if !game
        .folders
        .iter()
        .any(|f| f.path.to_lowercase() == path.to_lowercase())
    {
        return Err(CommandError::new(
            "badFolder",
            "that isn't one of this game's save folders",
        ));
    }
    std::process::Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| CommandError::new("io", format!("could not open {path}: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .join("target")
            .join("tmp")
            .join(format!("ot-desktop-saves-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn unavailable_library_gives_a_clear_error() {
        let state = SavesState::open(None);
        let e = state.game_and_store("x").unwrap_err();
        assert_eq!(e.kind, "savesUnavailable");
        assert!(!state.watch_once(&[]));
    }

    #[test]
    fn watcher_backs_up_on_start_and_exit() {
        let base = tmp("watch");
        let game_dir = base.join("Game");
        let saves = base.join("Saves");
        std::fs::create_dir_all(&game_dir).unwrap();
        std::fs::create_dir_all(&saves).unwrap();
        let exe = game_dir.join("mygame.exe");
        std::fs::write(&exe, b"").unwrap();
        std::fs::write(saves.join("slot1.sav"), b"one").unwrap();

        let state = SavesState::open(Some(base.join("store")));
        let id = state
            .with_lib(|lib| {
                let g = lib.add(&exe, &Roots::default())?;
                lib.add_folder(&g.id, &saves)?;
                Ok(g.id)
            })
            .unwrap();
        let exe_s = exe.to_string_lossy().into_owned();
        let running = vec![(42, Some(exe_s.clone()))];

        assert!(state.watch_once(&running));
        let (_, store) = state.game_and_store(&id).unwrap();
        assert_eq!(store.list().unwrap()[0].reason, Reason::Launch);
        assert!(!state.watch_once(&running), "no change while it runs");
        std::fs::write(saves.join("slot1.sav"), b"two").unwrap();
        assert!(state.watch_once(&[]));
        let list = store.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].reason, Reason::Exit);

        // Attaching to it again backs up (unchanged saves aren't duplicated).
        state.on_attach(&exe_s);
        assert_eq!(store.list().unwrap().len(), 2);

        let e = CommandError::from(ot_saves::Error::GameRunning("G".into()));
        assert_eq!(e.kind, "gameRunning");
        std::fs::remove_dir_all(&base).unwrap();
    }
}
