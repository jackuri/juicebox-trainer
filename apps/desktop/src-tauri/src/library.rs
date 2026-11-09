//! Games library: installed games (Steam, Epic, GOG, Xbox, the player's own)
//! with their pack and anti-cheat status, and "Play with cheats": start the
//! game, wait for it, attach, load its pack and apply the chosen preset.
//!
//! Games the anti-cheat guard blocks are shown as blocked and can't be
//! started from here. Attaching still goes through ot-core's guard.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ot_saves::covers::{self, CoverKind, MAX_COVER_BYTES};
use ot_saves::installs::{self, Install, Launch, OwnGame};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

use crate::commands::{self, AppState, CmdResult, CommandError, SessionInfo, TrainerView};
use crate::saves::SavesState;

#[derive(Debug, Default)]
pub struct LibraryState {
    busy: AtomicBool,
    cancel: AtomicBool,
    /// Where each listed game's cover comes from (filled by `library_list`).
    /// `library_cover` only reads these files, never a path from the UI.
    covers: Mutex<HashMap<String, CoverSource>>,
}

/// Where a game's cover picture comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CoverSource {
    /// A picture file on this PC (Steam's cache, GOG's icon, Xbox's logo).
    File(PathBuf),
    /// The icon of the game's exe.
    ExeIcon(PathBuf),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackRef {
    pub key: String,
    pub name: String,
    pub ai: bool,
    pub exe: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGame {
    #[serde(flatten)]
    pub install: Install,
    pub pack: Option<PackRef>,
    pub presets: Vec<String>,
    pub auto_preset: Option<String>,
    /// Why the anti-cheat guard blocks this game, if it does.
    pub blocked: Option<String>,
    /// Anti-cheat that comes with the game (its files are in the game folder).
    /// Not a block: attaching works only while none of it is running, which
    /// the guard checks when the game runs.
    pub anti_cheat: Option<String>,
    /// A cover picture is available (`library_cover`), and how to draw it.
    pub cover: Option<CoverKind>,
    #[serde(skip)]
    cover_source: Option<CoverSource>,
}

/// `%APPDATA%\OpenTrainer` (the player's own games list lives here).
fn app_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("OpenTrainer"))
}

/// Valid packs by exe name (lowercase). The player's packs win over the
/// built-in examples.
fn packs_by_exe() -> HashMap<String, (PackRef, String)> {
    let mut out = HashMap::new();
    for found in ot_packs::store::all() {
        let Ok(text) = found.text else { continue };
        let Ok((pack, _)) = ot_packs::parse(&text) else {
            continue;
        };
        out.insert(
            pack.game.exe.to_lowercase(),
            (
                PackRef {
                    key: found.key,
                    name: pack.game.name,
                    ai: pack.meta.is_ai(),
                    exe: pack.game.exe,
                },
                pack.game.id,
            ),
        );
    }
    out
}

fn describe(
    install: Install,
    packs: &HashMap<String, (PackRef, String)>,
    settings_dir: Option<&Path>,
    steam: Option<&Path>,
) -> LibraryGame {
    let mut exes: Vec<PathBuf> = installs::exes_in(Path::new(&install.folder))
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    if let Some(e) = &install.exe {
        exes.insert(0, PathBuf::from(e));
    }
    let found = exes.iter().find_map(|e| {
        let name = e.file_name()?.to_string_lossy().to_lowercase();
        packs.get(&name)
    });
    let (presets, auto_preset) = match (found, settings_dir) {
        (Some((_, id)), Some(dir)) => {
            let s = ot_packs::settings::load(dir, id);
            (
                s.presets.into_iter().map(|p| p.name).collect(),
                s.auto_preset,
            )
        }
        _ => (Vec::new(), None),
    };
    let probe = exes
        .first()
        .cloned()
        .unwrap_or_else(|| Path::new(&install.folder).join("game.exe"));
    let verdict = ot_core::guard::check_install(&probe);
    // Only anti-cheat files: the guard decides once the game runs.
    let files_only = !verdict.allowed
        && verdict
            .findings
            .iter()
            .all(|f| f.kind == ot_core::FindingKind::InstallMarker);
    let anti_cheat = files_only.then(|| {
        let mut names: Vec<&str> = verdict
            .findings
            .iter()
            .map(|f| f.product.as_str())
            .collect();
        names.dedup();
        names.join(", ")
    });
    let blocked = (!verdict.allowed && !files_only).then(|| {
        let names: Vec<&str> = verdict
            .findings
            .iter()
            .map(|f| f.product.as_str())
            .collect();
        if names.is_empty() {
            "anti-cheat found".to_string()
        } else {
            format!("anti-cheat found: {}", names.join(", "))
        }
    });
    let cover_source = match covers::find(&install, steam) {
        Some(c) => Some((c.kind, CoverSource::File(c.path))),
        None => exes
            .first()
            .filter(|e| e.is_file())
            .map(|e| (CoverKind::Icon, CoverSource::ExeIcon(e.clone()))),
    };
    LibraryGame {
        install,
        pack: found.map(|(p, _)| p.clone()),
        presets,
        auto_preset,
        blocked,
        anti_cheat,
        cover: cover_source.as_ref().map(|(k, _)| *k),
        cover_source: cover_source.map(|(_, s)| s),
    }
}

fn all_installs() -> Vec<Install> {
    let mut all = installs::detect();
    if let Some(dir) = app_dir() {
        all.extend(installs::load_own(&dir).iter().map(OwnGame::install));
    }
    all
}

fn list(settings_dir: Option<&Path>) -> Vec<LibraryGame> {
    let packs = packs_by_exe();
    let steam = ot_saves::steam::steam_dir();
    let mut games: Vec<LibraryGame> = all_installs()
        .into_iter()
        .map(|i| describe(i, &packs, settings_dir, steam.as_deref()))
        .collect();
    // Games with a pack first, then by name.
    games.sort_by_key(|g| (g.pack.is_none(), g.install.name.to_lowercase()));
    games
}

fn blocking_err(e: impl std::fmt::Display) -> CommandError {
    CommandError::new("io", e.to_string())
}

#[tauri::command]
pub async fn library_list(
    state: State<'_, AppState>,
    lib: State<'_, LibraryState>,
) -> CmdResult<Vec<LibraryGame>> {
    let dir = state.settings_dir.get().cloned();
    let games = tauri::async_runtime::spawn_blocking(move || list(dir.as_deref()))
        .await
        .map_err(blocking_err)?;
    *lib.covers.lock().unwrap_or_else(|e| e.into_inner()) = games
        .iter()
        .filter_map(|g| Some((g.install.id.clone(), g.cover_source.clone()?)))
        .collect();
    Ok(games)
}

/// The cover picture of a listed game (JPEG / PNG / ICO bytes), or nothing.
/// Only files found by `library_list` are read.
#[tauri::command]
pub async fn library_cover(
    id: String,
    lib: State<'_, LibraryState>,
) -> CmdResult<tauri::ipc::Response> {
    let source = lib
        .covers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .cloned();
    let bytes = match source {
        None => None,
        Some(source) => tauri::async_runtime::spawn_blocking(move || cover_bytes(&source))
            .await
            .map_err(blocking_err)?,
    };
    Ok(tauri::ipc::Response::new(bytes.unwrap_or_default()))
}

fn cover_bytes(source: &CoverSource) -> Option<Vec<u8>> {
    match source {
        CoverSource::File(p) => {
            let len = std::fs::metadata(p).ok()?.len();
            (len <= MAX_COVER_BYTES).then(|| std::fs::read(p).ok())?
        }
        CoverSource::ExeIcon(exe) => crate::covers::exe_icon_png(exe),
    }
}

/// Adds a game by its exe path.
#[tauri::command]
pub async fn library_add(exe: String) -> CmdResult<()> {
    let dir = app_dir().ok_or_else(|| CommandError::new("io", "%APPDATA% is not set"))?;
    let game = installs::own_game(Path::new(exe.trim().trim_matches('"')))
        .map_err(|e| CommandError::new("io", e))?;
    let mut own = installs::load_own(&dir);
    own.retain(|g| !g.exe.eq_ignore_ascii_case(&game.exe));
    own.push(game);
    installs::save_own(&dir, &own).map_err(blocking_err)
}

/// Removes a game the player added (store games can't be removed).
#[tauri::command]
pub async fn library_remove(id: String) -> CmdResult<()> {
    let dir = app_dir().ok_or_else(|| CommandError::new("io", "%APPDATA% is not set"))?;
    let mut own = installs::load_own(&dir);
    own.retain(|g| g.install().id != id);
    installs::save_own(&dir, &own).map_err(blocking_err)
}

#[tauri::command]
pub async fn library_open_folder(id: String) -> CmdResult<()> {
    let game = all_installs()
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| CommandError::new("notFound", "that game isn't installed any more"))?;
    std::process::Command::new("explorer.exe")
        .arg(&game.folder)
        .spawn()
        .map_err(blocking_err)?;
    Ok(())
}

#[tauri::command]
pub fn library_cancel(lib: State<'_, LibraryState>) {
    lib.cancel.store(true, Ordering::SeqCst);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayResult {
    pub session: SessionInfo,
    pub trainer: Option<TrainerView>,
    /// The preset that was applied.
    pub preset: Option<String>,
}

fn progress(app: &AppHandle, message: &str) {
    let _ = app.emit("library-progress", message);
}

fn launch(launch: &Launch) -> CmdResult<()> {
    let (target, dir) = match launch {
        Launch::Uri(u) => (u.clone(), None),
        Launch::Exe(e) => (
            e.clone(),
            Path::new(e).parent().map(|d| HSTRING::from(d.as_os_str())),
        ),
        Launch::Manual => return Ok(()),
    };
    let target = HSTRING::from(target);
    let dir_ptr = dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr()));
    // SAFETY: plain ShellExecuteW with NUL-terminated wide strings that
    // outlive the call. The target comes from the store's own listing or the
    // player's own game list.
    let r = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            dir_ptr,
            SW_SHOWNORMAL,
        )
    };
    if r.0 as isize <= 32 {
        return Err(CommandError::new(
            "launch",
            "Windows couldn't start the game",
        ));
    }
    Ok(())
}

/// Running processes whose exe is inside `folder`.
fn running_in(folder: &str) -> Vec<ot_core::ProcessInfo> {
    let prefix = format!(
        "{}\\",
        folder
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    );
    ot_core::list_processes()
        .unwrap_or_default()
        .into_iter()
        .filter(|p| {
            p.exe_path
                .as_ref()
                .is_some_and(|e| e.replace('/', "\\").to_lowercase().starts_with(&prefix))
        })
        .collect()
}

/// A running program that is one of the player's games.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningGame {
    pub pid: u32,
    /// The game's name, from the library or its cheat pack.
    pub name: String,
    /// The library game it belongs to, if any.
    pub library_id: Option<String>,
}

/// Lowercase, backslashes, no trailing backslash.
fn norm_path(p: &str) -> String {
    p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Which running programs are games: the exe lives in an installed game's
/// folder (the deepest one wins), or a cheat pack is made for that exe name.
/// Crash reporters and Windows' own programs are left out.
fn match_running(
    procs: &[ot_core::ProcessInfo],
    installs: &[Install],
    pack_games: &HashMap<String, String>,
) -> Vec<RunningGame> {
    let folders: Vec<(String, &Install)> = installs
        .iter()
        .map(|i| (format!("{}\\", norm_path(&i.folder)), i))
        // A drive root ("c:\") would match every program on that drive.
        .filter(|(f, _)| f.trim_end_matches('\\').contains('\\'))
        .collect();
    let mut out: Vec<RunningGame> = procs
        .iter()
        .filter_map(|p| {
            let exe = norm_path(p.exe_path.as_deref()?);
            let name = p.name.to_lowercase();
            if is_crash_reporter(&name)
                || exe.get(1..).is_some_and(|r| r.starts_with(":\\windows\\"))
            {
                return None;
            }
            let install = folders
                .iter()
                .filter(|(f, _)| exe.starts_with(f.as_str()))
                .max_by_key(|(f, _)| f.len())
                .map(|(_, i)| *i);
            match install {
                Some(i) => Some(RunningGame {
                    pid: p.pid,
                    name: i.name.clone(),
                    library_id: Some(i.id.clone()),
                }),
                None => pack_games.get(&name).map(|g| RunningGame {
                    pid: p.pid,
                    name: g.clone(),
                    library_id: None,
                }),
            }
        })
        .collect();
    out.sort_by_key(|g| (g.name.to_lowercase(), g.pid));
    out
}

/// Crash reporters that ship next to games (not the game, even in its folder).
/// Matched by name, so a game called "Crash ..." still counts.
fn is_crash_reporter(exe_lower: &str) -> bool {
    let stem = exe_lower.strip_suffix(".exe").unwrap_or(exe_lower);
    stem.starts_with("unitycrashhandler")
        || matches!(
            stem,
            "crashreportclient"
                | "crashpad_handler"
                | "crashhandler"
                | "crashreporter"
                | "crashsender"
                | "crashsender1403"
                | "bssndrpt"
                | "bugsplat"
                | "werfault"
        )
}

/// Running programs that are the player's games (installed, or with a pack).
#[tauri::command]
pub async fn running_games() -> CmdResult<Vec<RunningGame>> {
    tauri::async_runtime::spawn_blocking(|| -> CmdResult<Vec<RunningGame>> {
        let procs = ot_core::list_processes()?;
        let pack_games: HashMap<String, String> = packs_by_exe()
            .into_iter()
            .map(|(exe, (pack, _))| (exe, pack.name))
            .collect();
        Ok(match_running(&procs, &all_installs(), &pack_games))
    })
    .await
    .map_err(blocking_err)?
}

/// The game's main process: the pack's exe if it's running, else the one
/// with the biggest exe (a launcher or bootstrap exe is small).
fn pick(procs: &[ot_core::ProcessInfo], pack_exe: Option<&str>) -> Option<u32> {
    if let Some(want) = pack_exe
        && let Some(p) = procs.iter().find(|p| p.name.eq_ignore_ascii_case(want))
    {
        return Some(p.pid);
    }
    procs
        .iter()
        .max_by_key(|p| {
            p.exe_path
                .as_ref()
                .and_then(|e| std::fs::metadata(e).ok())
                .map_or(0, |m| m.len())
        })
        .map(|p| p.pid)
}

struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn play(app: &AppHandle, id: &str) -> CmdResult<PlayResult> {
    let lib = app.state::<LibraryState>();
    if lib.busy.swap(true, Ordering::SeqCst) {
        return Err(CommandError::new("busy", "a game is already being started"));
    }
    let _busy = Busy(&lib.busy);
    lib.cancel.store(false, Ordering::SeqCst);
    let state = app.state::<AppState>();
    let install = all_installs()
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| CommandError::new("notFound", "that game isn't installed any more"))?;
    let game = describe(
        install,
        &packs_by_exe(),
        state.settings_dir.get().map(PathBuf::as_path),
        None, // no cover needed to play
    );
    if let Some(why) = &game.blocked {
        return Err(CommandError::new(
            "blocked",
            format!(
                "OpenTrainer won't start {} with cheats ({why}). It's for single-player games only.",
                game.install.name
            ),
        ));
    }
    let name = game.install.name.clone();
    let folder = game.install.folder.clone();
    let pack_exe = game.pack.as_ref().map(|p| p.exe.clone());

    let already: HashSet<u32> = running_in(&folder).iter().map(|p| p.pid).collect();
    let manual = matches!(game.install.launch, Launch::Manual);
    if already.is_empty() {
        launch(&game.install.launch)?;
        progress(
            app,
            &if manual {
                format!(
                    "Start {name} yourself (e.g. from the Xbox app). OpenTrainer is waiting for it…"
                )
            } else {
                format!("Starting {name}… (waiting for it to open)")
            },
        );
    } else {
        progress(app, &format!("{name} is already running."));
    }
    let limit = Duration::from_secs(if manual { 600 } else { 300 });
    let started = Instant::now();
    let pid = loop {
        if lib.cancel.load(Ordering::SeqCst) {
            return Err(CommandError::new(
                "cancelled",
                "stopped waiting for the game",
            ));
        }
        if started.elapsed() > limit {
            return Err(CommandError::new(
                "timeout",
                format!("{name} didn't start. Start it yourself, then attach from the Cheats tab."),
            ));
        }
        if !running_in(&folder).is_empty() {
            // Give a launcher a moment to start the real game.
            if already.is_empty() {
                std::thread::sleep(Duration::from_secs(4));
            }
            if let Some(pid) = pick(&running_in(&folder), pack_exe.as_deref()) {
                break pid;
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    };

    progress(app, &format!("Attaching to {name}…"));
    let saves = app.state::<SavesState>();
    let mut tries = 0;
    let session = loop {
        match commands::attach_pid(&state, &saves, pid) {
            Ok(s) => break s,
            // A game that's still starting can refuse at first; anti-cheat
            // refusals are final.
            Err(e) if e.kind != "blocked" && tries < 3 => {
                tries += 1;
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => return Err(e),
        }
    };
    let _ = app.emit("trainer-changed", "library");

    let Some(pack) = game.pack else {
        return Ok(PlayResult {
            session,
            trainer: None,
            preset: None,
        });
    };
    progress(app, &format!("Loading {}…", pack.name));
    let mut trainer = Some(commands::load_key(&state, pack.key)?);
    let preset = game.auto_preset;
    if let Some(p) = &preset {
        let (view, waiting) = commands::apply_preset(&state, p)?;
        trainer = Some(view);
        if let Some(w) = waiting {
            commands::retry_in_background(app.clone(), w);
        }
    }
    Ok(PlayResult {
        session,
        trainer,
        preset,
    })
}

/// "Play with cheats": starts the game (or uses it if it's running), waits
/// for it, attaches, loads its pack and applies its start preset.
#[tauri::command]
pub async fn library_play(id: String, app: AppHandle) -> CmdResult<PlayResult> {
    tauri::async_runtime::spawn_blocking(move || play(&app, &id))
        .await
        .map_err(blocking_err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str, exe: Option<&str>) -> ot_core::ProcessInfo {
        ot_core::ProcessInfo {
            pid,
            parent_pid: 0,
            name: name.into(),
            exe_path: exe.map(str::to_string),
            arch: ot_core::Arch::X64,
            threads: 1,
        }
    }

    #[test]
    fn picks_the_packs_exe_then_the_biggest() {
        let dir = std::env::current_exe().unwrap();
        let big = dir.display().to_string();
        let procs = [
            proc(1, "Launcher.exe", Some(r"C:\nope\Launcher.exe")),
            proc(2, "game.exe", Some(&big)),
        ];
        assert_eq!(pick(&procs, Some("launcher.exe")), Some(1));
        assert_eq!(pick(&procs, None), Some(2));
        assert_eq!(pick(&[], None), None);
    }

    fn install(id: &str, name: &str, folder: &str) -> Install {
        Install {
            id: id.into(),
            name: name.into(),
            store: installs::StoreKind::Steam,
            folder: folder.into(),
            exe: None,
            launch: Launch::Manual,
        }
    }

    #[test]
    fn running_games_are_matched_by_folder_or_pack() {
        let installs = [
            install("steam:1", "Big Game", r"D:\Steam\steamapps\common\Big Game"),
            install(
                "steam:2",
                "Big Game DLC",
                r"D:/Steam/steamapps/common/Big Game/DLC/",
            ),
            install("own:x", "Whole drive", r"E:\"),
        ];
        let packs = HashMap::from([(
            "testbed_native.exe".to_string(),
            "OpenTrainer testbed".to_string(),
        )]);
        let procs = [
            proc(
                10,
                "BigGame.exe",
                Some(r"d:\steam\steamapps\common\big game\bin\BigGame.exe"),
            ),
            proc(
                11,
                "dlc.exe",
                Some(r"D:\Steam\steamapps\common\Big Game\DLC\dlc.exe"),
            ),
            proc(
                12,
                "UnityCrashHandler64.exe",
                Some(r"D:\Steam\steamapps\common\Big Game\UnityCrashHandler64.exe"),
            ),
            proc(
                18,
                "CrashReportClient.exe",
                Some(r"D:\Steam\steamapps\common\Big Game\Engine\CrashReportClient.exe"),
            ),
            proc(
                19,
                "CrashBandicoot.exe",
                Some(r"D:\Steam\steamapps\common\Big Game\CrashBandicoot.exe"),
            ),
            proc(
                13,
                "Big Game 2.exe",
                Some(r"D:\Steam\steamapps\common\Big Game 2\Big Game 2.exe"),
            ),
            proc(14, "testbed_native.exe", Some(r"C:\dev\testbed_native.exe")),
            proc(15, "chrome.exe", Some(r"E:\Chrome\chrome.exe")),
            proc(16, "notepad.exe", Some(r"C:\Windows\notepad.exe")),
            proc(17, "System", None),
        ];
        let got = match_running(&procs, &installs, &packs);
        let want = [
            (10, "Big Game", Some("steam:1")),
            (19, "Big Game", Some("steam:1")),
            (11, "Big Game DLC", Some("steam:2")),
            (14, "OpenTrainer testbed", None),
        ];
        let got: Vec<(u32, &str, Option<&str>)> = got
            .iter()
            .map(|g| (g.pid, g.name.as_str(), g.library_id.as_deref()))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn lists_games_with_pack_and_guard_status() {
        // Whatever is installed here: it must not fail, and every blocked
        // game says why.
        for g in list(None) {
            if let Some(why) = &g.blocked {
                assert!(why.starts_with("anti-cheat"), "{why}");
            }
        }
    }
}
