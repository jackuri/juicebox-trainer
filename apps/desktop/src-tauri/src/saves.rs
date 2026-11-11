//! Save backup commands and the launch/exit watcher. The real work (finding
//! folders, backups, diff, guarded restore) lives in `jb-saves`.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use jb_saves::diff::Side;
use jb_saves::{
    Created, Diff, Edit, EditReport, FileView, Game, Library, Limits, Reason, RestoreOptions,
    RestoreReport, Roots, SaveFile, Store, Summary, WatchKind, Watcher, editor,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};

use crate::commands::CommandError;

type CmdResult<T> = Result<T, CommandError>;

/// How often the watcher looks for games starting or closing.
const WATCH_INTERVAL: Duration = Duration::from_secs(3);

impl From<jb_saves::Error> for CommandError {
    fn from(e: jb_saves::Error) -> Self {
        let kind = e.kind();
        let message = e.to_string();
        let verdict = match e {
            jb_saves::Error::Blocked(v) => Some(v),
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
    /// Opens the library in `root` (`%LOCALAPPDATA%\JuiceBox Trainer\saves`).
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
                let processes: Vec<(u32, Option<String>)> = match jb_core::list_processes() {
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
    let steam_running = jb_core::list_processes()
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
    let exe = jb_core::list_processes()?
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
    Ok(jb_saves::diff(
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
    Ok(jb_saves::restore(
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

// ----- save editor (v0.7.0) -------------------------------------------------

#[tauri::command]
pub async fn saves_files(id: String, state: State<'_, SavesState>) -> CmdResult<Vec<SaveFile>> {
    let (game, _) = state.game_and_store(&id)?;
    Ok(editor::files(&game)?)
}

#[tauri::command]
pub async fn saves_open_file(
    id: String,
    folder: String,
    path: String,
    state: State<'_, SavesState>,
) -> CmdResult<FileView> {
    let (game, _) = state.game_and_store(&id)?;
    Ok(editor::open(&game, &folder, &path)?)
}

#[tauri::command]
pub async fn saves_hex(
    id: String,
    folder: String,
    path: String,
    offset: u64,
    len: u64,
    state: State<'_, SavesState>,
) -> CmdResult<Vec<u8>> {
    let (game, _) = state.game_and_store(&id)?;
    Ok(editor::hex(&game, &folder, &path, offset, len)?)
}

/// Saves edits to one file (game closed, guard check, backup first).
#[tauri::command]
pub async fn saves_edit(
    id: String,
    folder: String,
    path: String,
    edits: Vec<Edit>,
    state: State<'_, SavesState>,
) -> CmdResult<EditReport> {
    let (game, store) = state.game_and_store(&id)?;
    Ok(editor::save(&game, &store, &folder, &path, &edits)?)
}

/// A save code from a cheat pack, as the UI lists it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickCodeView {
    pub pack_key: String,
    pub pack_name: String,
    pub id: String,
    pub name: String,
    pub codec: String,
    pub file: String,
    pub path: String,
    pub value: serde_json::Value,
    /// Why it can't be used right now (e.g. the save file doesn't exist yet).
    pub problem: Option<String>,
}

/// Valid packs for this game's exe (by file name).
fn packs_for(game: &Game) -> Vec<(String, jb_packs::Pack)> {
    let exe = Path::new(&game.exe)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    jb_packs::store::all()
        .into_iter()
        .filter_map(|f| {
            let pack = jb_packs::parse(f.text.as_ref().ok()?).ok()?.0;
            (pack.game.exe.to_lowercase() == exe).then_some((f.key, pack))
        })
        .collect()
}

fn quick_codes(game: &Game, roots: &Roots) -> Vec<QuickCodeView> {
    let mut out: Vec<QuickCodeView> = Vec::new();
    for (key, pack) in packs_for(game) {
        for c in &pack.save_codes {
            // The x64 and x86 packs of one game often share codes.
            let same = out.iter().any(|o| {
                o.file == c.file
                    && o.path == c.set.path
                    && o.value == c.set.value
                    && o.codec == c.codec
            });
            if same {
                continue;
            }
            let problem = jb_saves::quick::resolve(&c.file, roots)
                .and_then(|resolved| {
                    let (folder, rel) = jb_saves::quick::place(game, &resolved)?;
                    editor::open(game, &folder, &rel).map(|_| ())
                })
                .err()
                .map(|e| e.to_string());
            out.push(QuickCodeView {
                pack_key: key.clone(),
                pack_name: pack.game.name.clone(),
                id: c.id.clone(),
                name: c.name.clone(),
                codec: c.codec.clone(),
                file: c.file.clone(),
                path: c.set.path.clone(),
                value: c.set.value.clone(),
                problem,
            });
        }
    }
    out
}

#[tauri::command]
pub async fn saves_quick_codes(
    id: String,
    state: State<'_, SavesState>,
) -> CmdResult<Vec<QuickCodeView>> {
    let (game, _) = state.game_and_store(&id)?;
    Ok(quick_codes(&game, &Roots::current()))
}

#[tauri::command]
pub async fn saves_apply_quick_code(
    id: String,
    pack_key: String,
    code_id: String,
    state: State<'_, SavesState>,
) -> CmdResult<EditReport> {
    let (game, store) = state.game_and_store(&id)?;
    let (_, pack) = packs_for(&game)
        .into_iter()
        .find(|(k, _)| *k == pack_key)
        .ok_or_else(|| CommandError::new("unknownPack", "that cheat pack isn't available"))?;
    let code = pack
        .save_codes
        .iter()
        .find(|c| c.id == code_id)
        .ok_or_else(|| CommandError::new("unknownCode", "that save code isn't in the pack"))?;
    Ok(jb_saves::quick::apply(
        &game,
        &store,
        &Roots::current(),
        &code.codec,
        &code.file,
        &code.set.path,
        &code.set.value,
    )?)
}

fn steam_accounts() -> Vec<jb_saves::accounts::SteamAccount> {
    jb_saves::steam::steam_dir()
        .map(|s| jb_saves::accounts::accounts(&s))
        .unwrap_or_default()
}

/// The game's saves per Steam account, for "Copy to another Steam account".
#[tauri::command]
pub async fn saves_accounts(
    id: String,
    state: State<'_, SavesState>,
) -> CmdResult<jb_saves::accounts::AccountsView> {
    let (game, _) = state.game_and_store(&id)?;
    Ok(jb_saves::accounts::view(&game, &steam_accounts())?)
}

/// What copying one account's saves to another would do. Changes nothing.
#[tauri::command]
pub async fn saves_account_plan(
    id: String,
    from: String,
    to: String,
    state: State<'_, SavesState>,
) -> CmdResult<jb_saves::accounts::CopyPlan> {
    let (game, _) = state.game_and_store(&id)?;
    Ok(jb_saves::accounts::plan(
        &game,
        &steam_accounts(),
        &from,
        &to,
    )?)
}

/// Copies one account's saves to another (guarded, backed up first), and adds
/// the other account's new folders to the game so they are backed up too.
#[tauri::command]
pub async fn saves_account_copy(
    id: String,
    from: String,
    to: String,
    state: State<'_, SavesState>,
) -> CmdResult<jb_saves::accounts::CopyReport> {
    let (game, store) = state.game_and_store(&id)?;
    let known = steam_accounts();
    let report = jb_saves::accounts::copy(&game, &store, &known, &from, &to)?;
    let label = known
        .iter()
        .find(|a| a.id64 == to.trim())
        .map(jb_saves::accounts::SteamAccount::label)
        .unwrap_or_else(|| to.clone());
    for folder in &report.add_folders {
        let added = state.with_lib(|lib| {
            lib.add_folder_as(
                &id,
                Path::new(folder),
                &format!("Copied to the Steam account {label}"),
            )?;
            Ok(())
        });
        if let Err(e) = added {
            warn!(%folder, "could not add the copied folder: {}", e.message);
        }
    }
    Ok(report)
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

        let e = CommandError::from(jb_saves::Error::GameRunning("G".into()));
        assert_eq!(e.kind, "gameRunning");
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn pack_and_saves_agree_on_placeholders_and_codecs() {
        assert_eq!(
            jb_packs::schema::SAVE_PLACEHOLDERS,
            jb_saves::quick::PLACEHOLDERS
        );
        assert_eq!(jb_packs::schema::SAVE_CODECS, jb_saves::quick::CODECS);
    }

    #[test]
    fn lists_quick_codes_from_bundled_packs() {
        let base = tmp("quick");
        let saves = base.join("LocalLow").join("TestCo").join("Testbed");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(
            saves.join("save1.json"),
            "{\"player\": {\"gold\": 1, \"ammo\": 2}}",
        )
        .unwrap();
        let game_dir = base.join("Game");
        std::fs::create_dir_all(&game_dir).unwrap();
        let exe = game_dir.join("testbed_native.exe");
        std::fs::write(&exe, b"").unwrap();
        let roots = Roots {
            local_low: Some(base.join("LocalLow")),
            ..Roots::default()
        };
        let mut lib = Library::open(base.join("store")).unwrap();
        let g = lib.add(&exe, &roots).unwrap();
        let g = lib.add_folder(&g.id, &saves).unwrap();

        let codes = quick_codes(&g, &roots);
        assert_eq!(codes.len(), 2, "{codes:?}");
        assert!(codes.iter().all(|c| c.problem.is_none()), "{codes:?}");
        assert_eq!(codes[0].path, "player.gold");

        std::fs::remove_file(saves.join("save1.json")).unwrap();
        let codes = quick_codes(&g, &roots);
        assert!(codes.iter().all(|c| c.problem.is_some()));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
