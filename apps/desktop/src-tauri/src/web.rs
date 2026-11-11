//! Web games (v1.18.0): games that run in a browser page. JuiceBox Trainer opens the
//! game in its own window with its helper script (`web_agent.js`) added before
//! the page's scripts, and asks the helper to find, change and lock numbers.
//! The game's page gets no access to JuiceBox Trainer's commands (only the main
//! window and the overlay do, see `capabilities/default.json`).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

use crate::commands::{CmdResult, CommandError};

/// The helper added to every game page.
pub const AGENT_JS: &str = include_str!("web_agent.js");
/// The game window's label.
pub const LABEL: &str = "webgame";
/// How long a call to the helper may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_GAMES: usize = 200;
const MAX_CHEATS: usize = jb_packs::web::MAX_CHEATS;

fn err(kind: &'static str, message: impl Into<String>) -> CommandError {
    CommandError::new(kind, message)
}

// ----- the list of web games ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebGame {
    pub id: String,
    pub name: String,
    pub url: String,
    /// `YYYY-MM-DD`.
    pub added: String,
}

pub(crate) fn app_dir() -> CmdResult<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join(jb_core::names::FOLDER))
        .ok_or_else(|| err("noAppData", "%APPDATA% is not set"))
}

/// A web game's address: https anywhere, or http on this PC (for local
/// games); no sign-in details in it.
pub fn check_url(url: &str) -> CmdResult<Url> {
    let bad = || {
        err(
            "badRequest",
            "type the game's web address, starting with https://",
        )
    };
    let url = url.trim();
    if url.len() > 2000 {
        return Err(bad());
    }
    let parsed = Url::parse(url).map_err(|_| bad())?;
    let local = matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    let ok = match parsed.scheme() {
        "https" => true,
        "http" => local,
        _ => false,
    };
    if !ok || parsed.host_str().is_none_or(str::is_empty) {
        return Err(bad());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(err(
            "badRequest",
            "the address can't contain a user name or password",
        ));
    }
    Ok(parsed)
}

fn check_name(name: &str) -> CmdResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(err(
            "badRequest",
            "give the game a name (up to 80 characters)",
        ));
    }
    Ok(name.to_string())
}

fn games_file(dir: &std::path::Path) -> PathBuf {
    dir.join("web-games.json")
}

fn load_games(dir: &std::path::Path) -> Vec<WebGame> {
    std::fs::read_to_string(games_file(dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_json(path: &std::path::Path, value: &impl Serialize) -> CmdResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| err("io", e.to_string()))?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| err("io", e.to_string()))? + "\n";
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(|e| err("io", e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| err("io", e.to_string()))
}

fn game_id(name: &str, url: &str) -> String {
    jb_packs::community::table_id(name, url)
}

fn add_game(dir: &std::path::Path, name: &str, url: &str, today: &str) -> CmdResult<WebGame> {
    let name = check_name(name)?;
    let url = check_url(url)?.to_string();
    let mut games = load_games(dir);
    if let Some(g) = games.iter().find(|g| g.url == url) {
        return Err(err(
            "exists",
            format!("{} is already in your web games", g.name),
        ));
    }
    if games.len() >= MAX_GAMES {
        return Err(err(
            "badRequest",
            "that's a lot of web games: remove some first",
        ));
    }
    let game = WebGame {
        id: game_id(&name, &url),
        name,
        url,
        added: today.to_string(),
    };
    games.push(game.clone());
    save_json(&games_file(dir), &games)?;
    Ok(game)
}

fn remove_game(dir: &std::path::Path, id: &str) -> CmdResult<()> {
    let mut games = load_games(dir);
    let before = games.len();
    games.retain(|g| g.id != id);
    if games.len() == before {
        return Err(err("notFound", "that web game isn't in your list"));
    }
    save_json(&games_file(dir), &games)?;
    let _ = std::fs::remove_file(cheats_file(dir, id));
    Ok(())
}

/// Your web games.
#[tauri::command]
pub fn web_games() -> CmdResult<Vec<WebGame>> {
    Ok(load_games(&app_dir()?))
}

/// Adds a web game by name and address.
#[tauri::command]
pub fn web_game_add(name: String, url: String) -> CmdResult<WebGame> {
    add_game(&app_dir()?, &name, &url, &jb_packs::share::today())
}

/// Removes a web game (and its saved cheats).
#[tauri::command]
pub fn web_game_remove(id: String) -> CmdResult<()> {
    remove_game(&app_dir()?, &id)
}

// ----- saved cheats for a web game ----------------------------------------------------------

pub use jb_packs::web::{WebCheat, WebPack};

fn check_cheat(c: &WebCheat) -> CmdResult<WebCheat> {
    jb_packs::web::check_cheat(c).map_err(|e| err("badRequest", e))
}

fn cheats_file(dir: &std::path::Path, id: &str) -> PathBuf {
    dir.join("web-cheats").join(format!("{id}.json"))
}

fn known_game(dir: &std::path::Path, id: &str) -> CmdResult<WebGame> {
    load_games(dir)
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| err("notFound", "that web game isn't in your list"))
}

/// The cheats you saved for a web game.
#[tauri::command]
pub fn web_cheats(id: String) -> CmdResult<Vec<WebCheat>> {
    let dir = app_dir()?;
    known_game(&dir, &id)?;
    Ok(load_cheats(&dir, &id))
}

fn load_cheats(dir: &std::path::Path, id: &str) -> Vec<WebCheat> {
    std::fs::read_to_string(cheats_file(dir, id))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Saves the cheats for a web game (the whole list).
#[tauri::command]
pub fn web_cheats_save(id: String, cheats: Vec<WebCheat>) -> CmdResult<()> {
    let dir = app_dir()?;
    known_game(&dir, &id)?;
    if cheats.len() > MAX_CHEATS {
        return Err(err("badRequest", "too many cheats"));
    }
    let cheats = cheats
        .iter()
        .map(check_cheat)
        .collect::<CmdResult<Vec<_>>>()?;
    save_json(&cheats_file(&dir, &id), &cheats)
}

// ----- sharing web cheats (v1.19.0) -----------------------------------------------------------

/// A pack of the cheats saved for one of the player's web games.
pub fn pack_for(
    dir: &std::path::Path,
    id: &str,
    by: &str,
    notes: Option<&str>,
    today: &str,
) -> CmdResult<WebPack> {
    let game = known_game(dir, id)?;
    let cheats = load_cheats(dir, id);
    if cheats.is_empty() {
        return Err(err("badRequest", "save some cheats for this game first"));
    }
    let pack = WebPack {
        format: jb_packs::web::FORMAT,
        id: jb_packs::web::pack_id(&game.name, &game.url),
        game: game.name,
        url: game.url,
        by: by.to_string(),
        notes: notes.map(String::from),
        added: Some(today.to_string()),
        cheats,
    };
    jb_packs::web::check_pack(&pack, false).map_err(|e| err("badRequest", e))
}

/// Saves a web game's cheats as an `.otweb` file where the player picks (to
/// give to friends). `None` if they cancelled.
#[tauri::command]
pub async fn web_export(
    id: String,
    by: String,
    notes: Option<String>,
    app: AppHandle,
) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let pack = pack_for(
        &app_dir()?,
        &id,
        &by,
        notes.as_deref(),
        &jb_packs::share::today(),
    )?;
    let text = serde_json::to_string_pretty(&pack).map_err(|e| err("io", e.to_string()))?
        + "
";
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Save the cheats to share")
        .set_file_name(jb_packs::web::file_name(&pack))
        .add_filter("JuiceBox Trainer web cheats", &[jb_packs::web::EXTENSION]);
    if let Ok(docs) = app.path().document_dir() {
        dialog = dialog.set_directory(docs);
    }
    let Some(path) = dialog.blocking_save_file() else {
        return Ok(None);
    };
    let mut path = path.into_path().map_err(|e| err("io", e.to_string()))?;
    if path.extension().is_none() {
        path.set_extension(jb_packs::web::EXTENSION);
    }
    std::fs::write(&path, text).map_err(|e| err("io", format!("{}: {e}", path.display())))?;
    Ok(Some(path.display().to_string()))
}

/// Lets the player pick an `.otweb` file. `None` if they cancelled.
#[tauri::command]
pub async fn web_pick_file(app: AppHandle) -> CmdResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog()
        .file()
        .set_title("Open web cheats")
        .add_filter(
            "JuiceBox Trainer web cheats",
            &[
                jb_packs::web::EXTENSION,
                jb_packs::web::LEGACY_EXTENSION,
                "json",
            ],
        )
        .blocking_pick_file()
        .map(|p| {
            p.into_path()
                .map(|p| p.display().to_string())
                .map_err(|e| err("io", e.to_string()))
        })
        .transpose()
}

fn read_file(path: &std::path::Path) -> CmdResult<WebPack> {
    let size = std::fs::metadata(path)
        .map_err(|e| err("io", format!("couldn't open the file: {e}")))?
        .len();
    if size > jb_packs::web::MAX_BYTES as u64 {
        return Err(err("badRequest", "the file is too big to be web cheats"));
    }
    let bytes =
        std::fs::read(path).map_err(|e| err("io", format!("couldn't open the file: {e}")))?;
    jb_packs::web::parse_pack(&bytes).map_err(|e| err("badRequest", e))
}

/// Reads an `.otweb` file (to show what's in it before adding it).
#[tauri::command]
pub fn web_read_file(path: String) -> CmdResult<WebPack> {
    read_file(std::path::Path::new(&path))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebInstalled {
    pub game: WebGame,
    /// Cheats added to the game's list.
    pub added: usize,
    /// Cheats the list already had (same place).
    pub skipped: usize,
}

fn install(dir: &std::path::Path, pack: &WebPack, today: &str) -> CmdResult<WebInstalled> {
    let pack = jb_packs::web::check_pack(pack, false).map_err(|e| err("badRequest", e))?;
    let url = check_url(&pack.url)?.to_string();
    let game = match load_games(dir).into_iter().find(|g| g.url == url) {
        Some(g) => g,
        None => add_game(dir, &pack.game, &url, today)?,
    };
    let (added, skipped) = add_cheats_in(dir, &game.id, &pack.cheats)?;
    Ok(WebInstalled {
        game,
        added,
        skipped,
    })
}

/// Adds a pack's cheats to the player's list for its game (adding the game
/// if it isn't in the list yet). Cheats already in the list are skipped.
#[tauri::command]
pub fn web_install(pack: WebPack) -> CmdResult<WebInstalled> {
    install(&app_dir()?, &pack, &jb_packs::share::today())
}

// ----- the game window --------------------------------------------------------------------------

/// The web game that's open.
#[derive(Default)]
pub struct WebState {
    open: Mutex<Option<WebGame>>,
    /// The last change made in the page and when (v1.22.0), to say what
    /// happened just before it crashed or froze.
    last_change: Mutex<Option<(std::time::SystemTime, String)>>,
}

/// What a helper call changes in the page, in words ("\"game.player.gold\"
/// was locked at 999"), or `None` for calls that only look.
fn describe_change(method: &str, args: &Value) -> Option<String> {
    let target = args
        .get("target")
        .and_then(|t| t.get("path"))
        .and_then(Value::as_array)
        .map(|p| {
            let parts: Vec<String> = p
                .iter()
                .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_string))
                .collect();
            format!("\"{}\"", parts.join("."))
        })
        .unwrap_or_else(|| "A value".to_string());
    let value = args.get("value").map(|v| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    });
    let value = value.unwrap_or_default();
    match method {
        "set" => Some(format!("{target} was set to {value}")),
        "lock" => Some(format!("{target} was locked at {value}")),
        "unlock" => Some("A value was unlocked".into()),
        "find" | "next" => Some("A value search was running".into()),
        _ => None,
    }
}

/// Keeps the change a helper call makes (see [`WebState::last_change`]).
fn note_change(app: &AppHandle, method: &str, args: &Value) {
    if let Some(what) = describe_change(method, args) {
        *app.state::<WebState>()
            .last_change
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((std::time::SystemTime::now(), what));
    }
}

/// The game page crashed or froze: tell the player why (v1.22.0).
fn page_failed(app: &AppHandle, failure: crate::page_watch::PageFailure) {
    let state = app.state::<WebState>();
    let game = state
        .open
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map_or_else(|| "web game".to_string(), |g| g.name.clone());
    let last = state
        .last_change
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let last = last.map(|(at, what)| {
        let secs = at.elapsed().map_or(0, |d| d.as_secs());
        (what, secs)
    });
    let p = crate::problems::record(crate::problems::web_page(
        &game,
        failure,
        last.as_ref().map(|(w, s)| (w.as_str(), *s)),
    ));
    let _ = app.emit("web-game-problem", p);
}

/// Reloads the game page (after it crashed or froze).
#[tauri::command]
pub fn web_reload(app: AppHandle) -> CmdResult<()> {
    let window = app.get_webview_window(LABEL).ok_or_else(|| {
        err(
            "noWebGame",
            "the game window is closed: open the game again with Play",
        )
    })?;
    window
        .reload()
        .map_err(|e| err("webGame", format!("couldn't reload the game: {e}")))
}

/// Opens a web game in JuiceBox Trainer's game window (closing one that's open).
#[tauri::command]
pub async fn web_open(
    id: String,
    app: AppHandle,
    state: State<'_, WebState>,
) -> CmdResult<WebGame> {
    crate::agreement::require()?;
    let game = known_game(&app_dir()?, &id)?;
    let url = check_url(&game.url)?;
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
    WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(url))
        .title(format!("{} · JuiceBox Trainer", game.name))
        .inner_size(1280.0, 800.0)
        .initialization_script_for_all_frames(AGENT_JS)
        .build()
        .map_err(|e| err("io", format!("couldn't open the game window: {e}")))?;
    let watcher = app.clone();
    if let Some(window) = app.get_webview_window(LABEL) {
        crate::page_watch::watch(&window, move |failure| page_failed(&watcher, failure));
    }
    *state.open.lock().unwrap_or_else(|e| e.into_inner()) = Some(game.clone());
    *state.last_change.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(game)
}

/// The web game that's open, if its window still is.
#[tauri::command]
pub fn web_current(app: AppHandle) -> Option<WebGame> {
    open_game(&app)
}

/// Closes the game window.
#[tauri::command]
pub fn web_close(app: AppHandle, state: State<'_, WebState>) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
    *state.open.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The helper calls JuiceBox Trainer may make.
const CALLS: &[&str] = &[
    "info", "find", "next", "results", "read", "set", "lock", "unlock", "keys", "state", "names",
    "near",
];

/// The script that runs one call in the game page and returns its JSON.
fn call_script(method: &str, args: &Value) -> CmdResult<String> {
    if !CALLS.contains(&method) {
        return Err(err("badRequest", format!("unknown call {method}")));
    }
    let method = serde_json::to_string(method).map_err(|e| err("io", e.to_string()))?;
    let args = serde_json::to_string(args).map_err(|e| err("io", e.to_string()))?;
    Ok(format!(
        "(() => {{ const a = window.__jbAgent; return a ? a.callJson({method}, {args}) : '{{\"error\":\"noAgent\"}}'; }})()"
    ))
}

/// What the helper answered: `{ ok }` or `{ error }`. WebView2 returns the
/// script's result as JSON, here a JSON string holding the answer.
fn parse_answer(raw: &str) -> CmdResult<Value> {
    let inner: String = serde_json::from_str(raw).map_err(|_| {
        err(
            "webGame",
            "the game page didn't answer (it may still be loading)",
        )
    })?;
    let answer: Value =
        serde_json::from_str(&inner).map_err(|_| err("webGame", "the game page answered oddly"))?;
    if let Some(e) = answer.get("error").and_then(Value::as_str) {
        return Err(if e == "noAgent" {
            err(
                "webGame",
                "JuiceBox Trainer's helper isn't in this page yet: wait for the game to load, or open it again",
            )
        } else {
            err("webGame", e.to_string())
        });
    }
    Ok(answer.get("ok").cloned().unwrap_or(Value::Null))
}

/// Asks the helper in the open game page to do something (`info`, `find`,
/// `next`, `results`, `read`, `set`, `lock`, `unlock`, and from v1.19.0
/// `keys`: the cheats' keys, `state`: what's locked).
#[tauri::command]
pub async fn web_call(method: String, args: Option<Value>, app: AppHandle) -> CmdResult<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        call_blocking(&app, &method, &args.unwrap_or(Value::Null))
    })
    .await
    .map_err(|e| err("webGame", e.to_string()))?
}

/// One call to the helper in the game page, waiting for its answer (not on
/// the main thread). Changing calls need the agreement.
pub fn call_blocking(app: &AppHandle, method: &str, args: &Value) -> CmdResult<Value> {
    if matches!(method, "set" | "lock" | "keys") {
        crate::agreement::require()?;
    }
    let script = call_script(method, args)?;
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| err("noWebGame", "open a web game first"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    window
        .eval_with_callback(script, move |result| {
            let _ = tx.send(result);
        })
        .map_err(|e| err("webGame", format!("couldn't reach the game page: {e}")))?;
    note_change(app, method, args);
    let raw = rx.recv_timeout(CALL_TIMEOUT).map_err(|_| {
        err(
            "webGame",
            "the game page took too long to answer. It may be busy or frozen: wait a moment, or reload the game",
        )
    })?;
    parse_answer(&raw)
}

/// The web game whose window is open (`None` once it was closed).
pub fn open_game(app: &AppHandle) -> Option<WebGame> {
    let state = app.state::<WebState>();
    let mut open = state.open.lock().unwrap_or_else(|e| e.into_inner());
    if app.get_webview_window(LABEL).is_none() {
        *open = None;
    }
    open.clone()
}

/// Adds cheats to a web game's saved list (the AI helper's finds, v1.20.0):
/// checked, cheats already there (same place) skipped, clashing names numbered.
pub fn add_cheats(id: &str, new: &[WebCheat]) -> CmdResult<(usize, usize)> {
    add_cheats_in(&app_dir()?, id, new)
}

fn add_cheats_in(dir: &std::path::Path, id: &str, new: &[WebCheat]) -> CmdResult<(usize, usize)> {
    known_game(dir, id)?;
    let new = new.iter().map(check_cheat).collect::<CmdResult<Vec<_>>>()?;
    let mut cheats = load_cheats(dir, id);
    let (mut added, mut skipped) = (0, 0);
    for c in new {
        if cheats.iter().any(|x| x.target == c.target) {
            skipped += 1;
            continue;
        }
        if cheats.len() >= MAX_CHEATS {
            return Err(err(
                "badRequest",
                "that's a lot of cheats for one game: remove some first",
            ));
        }
        cheats.push(WebCheat {
            name: free_name(&cheats, &c.name),
            ..c
        });
        added += 1;
    }
    save_json(&cheats_file(dir, id), &cheats)?;
    Ok((added, skipped))
}

/// `name`, or `name (2)`, `name (3)`… if the list already has it.
fn free_name(cheats: &[WebCheat], name: &str) -> String {
    let mut out = name.to_string();
    let mut n = 2;
    while cheats.iter().any(|x| x.name == out) {
        out = format!("{name} ({n})");
        n += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use jb_packs::web::WebTarget;

    #[test]
    fn web_addresses() {
        assert!(check_url("https://outbreakweb.com/file1/").is_ok());
        assert!(check_url("http://127.0.0.1:8080/game/").is_ok());
        assert!(check_url("http://localhost/").is_ok());
        for bad in [
            "http://example.com/",
            "file:///C:/x.html",
            "javascript:alert(1)",
            "https://user:pw@example.com/",
            "not a url",
            "https://",
        ] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn adds_lists_and_removes_games_with_their_cheats() {
        let dir = std::env::temp_dir().join(format!("ot-web-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let g = add_game(&dir, "Test Game", "https://example.com/play", "2026-11-09").unwrap();
        assert!(g.id.starts_with("test-game-"));
        assert_eq!(load_games(&dir), vec![g.clone()]);
        assert_eq!(
            add_game(&dir, "Again", "https://example.com/play", "2026-11-09")
                .unwrap_err()
                .kind,
            "exists"
        );
        assert!(add_game(&dir, " ", "https://example.com/x", "2026-11-09").is_err());
        std::fs::create_dir_all(dir.join("web-cheats")).unwrap();
        std::fs::write(cheats_file(&dir, &g.id), "[]").unwrap();
        remove_game(&dir, &g.id).unwrap();
        assert!(load_games(&dir).is_empty());
        assert!(!cheats_file(&dir, &g.id).exists());
        assert_eq!(remove_game(&dir, &g.id).unwrap_err().kind, "notFound");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cheats_travel_as_files_and_merge_into_the_list() {
        let base = std::env::temp_dir().join(format!("ot-webpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (mine, friend) = (base.join("mine"), base.join("friend"));
        let g = add_game(&mine, "Outbreak", "https://outbreakweb.com/", "2026-11-10").unwrap();
        assert!(
            pack_for(&mine, &g.id, "Sam", None, "2026-11-10").is_err(),
            "no cheats yet"
        );
        let gold: WebCheat = serde_json::from_str(
            r#"{"name":"Gold","target":{"k":"j","path":["game","gold"]},"value":999,"key":"ctrl+g"}"#,
        )
        .unwrap();
        let hp: WebCheat = serde_json::from_str(
            r#"{"name":"Health","target":{"k":"m","mem":0,"label":"memory","size":65536,"off":4096,"t":"i32"},"value":100}"#,
        )
        .unwrap();
        save_json(&cheats_file(&mine, &g.id), &vec![gold.clone(), hp.clone()]).unwrap();
        assert!(
            pack_for(&mine, &g.id, " ", None, "2026-11-10").is_err(),
            "needs who found them"
        );
        let pack = pack_for(&mine, &g.id, "Sam", Some("  level 1 "), "2026-11-10").unwrap();
        assert_eq!(pack.cheats[0].key.as_deref(), Some("Ctrl+G"));
        assert_eq!(pack.notes.as_deref(), Some("level 1"));
        assert_eq!(pack.url, "https://outbreakweb.com/");

        // The friend opens the file: the game is added with the cheats.
        let file = base.join("outbreak.otweb");
        std::fs::write(&file, serde_json::to_string(&pack).unwrap()).unwrap();
        let read = read_file(&file).unwrap();
        assert_eq!(read, pack);
        let got = install(&friend, &read, "2026-11-11").unwrap();
        assert_eq!((got.added, got.skipped), (2, 0));
        assert_eq!(got.game.url, "https://outbreakweb.com/");
        assert_eq!(load_cheats(&friend, &got.game.id).len(), 2);

        // Again: nothing new. A cheat with the same name in a new place is renamed.
        let again = install(&friend, &read, "2026-11-11").unwrap();
        assert_eq!((again.added, again.skipped), (0, 2));
        let mut moved = read.clone();
        moved.cheats = vec![WebCheat {
            target: WebTarget::Js {
                path: vec!["game".into(), "coins".into()],
            },
            ..gold.clone()
        }];
        install(&friend, &moved, "2026-11-11").unwrap();
        let names: Vec<String> = load_cheats(&friend, &got.game.id)
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, ["Gold", "Health", "Gold (2)"]);
        assert_eq!(load_games(&friend).len(), 1);

        // Bad files are refused.
        std::fs::write(&file, "{\"format\":1}").unwrap();
        assert!(read_file(&file).is_err());
        let mut bad = pack.clone();
        bad.url = "javascript:alert(1)".into();
        assert!(install(&friend, &bad, "2026-11-11").is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn cheats_are_checked() {
        let mem: WebCheat = serde_json::from_str(
            r#"{"name":"Gold","target":{"k":"m","mem":0,"label":"memory","size":65536,"off":4096,"t":"i32"},"value":999}"#,
        )
        .unwrap();
        assert!(check_cheat(&mem).is_ok());
        let js: WebCheat = serde_json::from_str(
            r#"{"name":"HP","target":{"k":"j","path":["game","player","hp"]}}"#,
        )
        .unwrap();
        assert!(check_cheat(&js).is_ok());
        let empty = WebCheat {
            target: WebTarget::Js { path: vec![] },
            ..js.clone()
        };
        assert!(check_cheat(&empty).is_err());
        assert!(
            serde_json::from_str::<WebCheat>(
                r#"{"name":"x","target":{"k":"m","mem":0,"label":"m","size":1,"off":0,"t":"i64"}}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<WebCheat>(
                r#"{"name":"x","target":{"k":"j","path":["a"],"code":"evil()"}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn calls_are_whitelisted_and_quoted() {
        assert!(call_script("eval", &Value::Null).is_err());
        let s = call_script(
            "find",
            &serde_json::json!({ "value": 100, "kind": "'); alert(1); ('" }),
        )
        .unwrap();
        assert!(
            s.contains(r#"a.callJson("find", {"kind":"'); alert(1); ('","value":100})"#),
            "{s}"
        );
    }

    #[test]
    fn answers() {
        let wrap = |s: &str| serde_json::to_string(s).unwrap();
        assert_eq!(
            parse_answer(&wrap(r#"{"ok":{"count":2}}"#)).unwrap()["count"],
            2
        );
        assert!(
            parse_answer(&wrap(r#"{"error":"type a number"}"#))
                .unwrap_err()
                .message
                .contains("type a number")
        );
        assert!(
            parse_answer(&wrap(r#"{"error":"noAgent"}"#))
                .unwrap_err()
                .message
                .contains("helper")
        );
        assert!(parse_answer("null").is_err());
    }
}
