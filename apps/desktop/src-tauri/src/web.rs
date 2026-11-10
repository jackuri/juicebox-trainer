//! Web games (v1.18.0): games that run in a browser page. OpenTrainer opens the
//! game in its own window with its helper script (`web_agent.js`) added before
//! the page's scripts, and asks the helper to find, change and lock numbers.
//! The game's page gets no access to OpenTrainer's commands (only the main
//! window and the overlay do, see `capabilities/default.json`).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

use crate::commands::{CmdResult, CommandError};

/// The helper added to every game page.
pub const AGENT_JS: &str = include_str!("web_agent.js");
/// The game window's label.
pub const LABEL: &str = "webgame";
/// How long a call to the helper may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_GAMES: usize = 200;
const MAX_CHEATS: usize = 200;

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

fn app_dir() -> CmdResult<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join("OpenTrainer"))
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
    ot_packs::community::table_id(name, url)
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
    add_game(&app_dir()?, &name, &url, &ot_packs::share::today())
}

/// Removes a web game (and its saved cheats).
#[tauri::command]
pub fn web_game_remove(id: String) -> CmdResult<()> {
    remove_game(&app_dir()?, &id)
}

// ----- saved cheats for a web game ----------------------------------------------------------

/// Where a number lives in a web game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "k", deny_unknown_fields)]
pub enum WebTarget {
    /// In one of the page's memories (WebAssembly or a big byte array).
    #[serde(rename = "m")]
    Memory {
        mem: u32,
        label: String,
        /// The memory's size when it was found (a hint that it's the same one).
        size: u64,
        off: u64,
        t: NumType,
    },
    /// A JavaScript value, by its path from `window`.
    #[serde(rename = "j")]
    Js { path: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NumType {
    U8,
    I16,
    I32,
    F32,
    F64,
}

/// A cheat the player saved for a web game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebCheat {
    pub name: String,
    pub target: WebTarget,
    /// The value it's usually set to.
    #[serde(default)]
    pub value: Option<f64>,
}

fn check_cheat(c: &WebCheat) -> CmdResult<()> {
    check_name(&c.name)?;
    match &c.target {
        WebTarget::Memory { label, off, .. } => {
            if label.chars().count() > 200 || *off > (1 << 40) {
                return Err(err("badRequest", "that cheat's place isn't valid"));
            }
        }
        WebTarget::Js { path } => {
            if path.is_empty()
                || path.len() > 16
                || path.iter().any(|p| p.is_empty() || p.chars().count() > 120)
            {
                return Err(err("badRequest", "that cheat's place isn't valid"));
            }
        }
    }
    if c.value.is_some_and(|v| !v.is_finite()) {
        return Err(err("badRequest", "that value isn't a number"));
    }
    Ok(())
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
    Ok(std::fs::read_to_string(cheats_file(&dir, &id))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default())
}

/// Saves the cheats for a web game (the whole list).
#[tauri::command]
pub fn web_cheats_save(id: String, cheats: Vec<WebCheat>) -> CmdResult<()> {
    let dir = app_dir()?;
    known_game(&dir, &id)?;
    if cheats.len() > MAX_CHEATS {
        return Err(err("badRequest", "too many cheats"));
    }
    for c in &cheats {
        check_cheat(c)?;
    }
    save_json(&cheats_file(&dir, &id), &cheats)
}

// ----- the game window --------------------------------------------------------------------------

/// The web game that's open.
#[derive(Default)]
pub struct WebState {
    open: Mutex<Option<WebGame>>,
}

/// Opens a web game in OpenTrainer's game window (closing one that's open).
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
        .title(format!("{} · OpenTrainer", game.name))
        .inner_size(1280.0, 800.0)
        .initialization_script_for_all_frames(AGENT_JS)
        .build()
        .map_err(|e| err("io", format!("couldn't open the game window: {e}")))?;
    *state.open.lock().unwrap_or_else(|e| e.into_inner()) = Some(game.clone());
    Ok(game)
}

/// The web game that's open, if its window still is.
#[tauri::command]
pub fn web_current(app: AppHandle, state: State<'_, WebState>) -> Option<WebGame> {
    let mut open = state.open.lock().unwrap_or_else(|e| e.into_inner());
    if app.get_webview_window(LABEL).is_none() {
        *open = None;
    }
    open.clone()
}

/// Closes the game window.
#[tauri::command]
pub fn web_close(app: AppHandle, state: State<'_, WebState>) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
    *state.open.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The helper calls OpenTrainer may make.
const CALLS: &[&str] = &[
    "info", "find", "next", "results", "read", "set", "lock", "unlock",
];

/// The script that runs one call in the game page and returns its JSON.
fn call_script(method: &str, args: &Value) -> CmdResult<String> {
    if !CALLS.contains(&method) {
        return Err(err("badRequest", format!("unknown call {method}")));
    }
    let method = serde_json::to_string(method).map_err(|e| err("io", e.to_string()))?;
    let args = serde_json::to_string(args).map_err(|e| err("io", e.to_string()))?;
    Ok(format!(
        "(() => {{ const a = window.__otAgent; return a ? a.callJson({method}, {args}) : '{{\"error\":\"noAgent\"}}'; }})()"
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
                "OpenTrainer's helper isn't in this page yet: wait for the game to load, or open it again",
            )
        } else {
            err("webGame", e.to_string())
        });
    }
    Ok(answer.get("ok").cloned().unwrap_or(Value::Null))
}

/// Asks the helper in the open game page to do something (`info`, `find`,
/// `next`, `results`, `read`, `set`, `lock`, `unlock`).
#[tauri::command]
pub async fn web_call(method: String, args: Option<Value>, app: AppHandle) -> CmdResult<Value> {
    if matches!(method.as_str(), "set" | "lock") {
        crate::agreement::require()?;
    }
    let script = call_script(&method, &args.unwrap_or(Value::Null))?;
    let window = app
        .get_webview_window(LABEL)
        .ok_or_else(|| err("noWebGame", "open a web game first"))?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    window
        .eval_with_callback(script, move |result| {
            let _ = tx.send(result);
        })
        .map_err(|e| err("webGame", format!("couldn't reach the game page: {e}")))?;
    let raw = tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(CALL_TIMEOUT))
        .await
        .map_err(|e| err("webGame", e.to_string()))?
        .map_err(|_| err("webGame", "the game page took too long to answer"))?;
    parse_answer(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

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
