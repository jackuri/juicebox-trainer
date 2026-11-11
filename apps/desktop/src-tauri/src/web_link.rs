//! The AI helper's link to the web game window (v1.20.0).
//!
//! The helper's guarded tool server (`juicebox-trainer --mcp`, the jb-mcp
//! tools) runs as its own process, but the game window lives in this one. So
//! while a helper session for a web game runs, the app listens on a local port
//! (127.0.0.1 only, a new port each time) for one-line JSON requests that
//! carry a random token only the tool server was given, and passes on only
//! these:
//! - the page helper's search calls (`info`, `names`, `find`, `next`,
//!   `results`, `read`) and its changing ones (`set`, `lock`, `unlock`, which
//!   need the agreement, as in the UI). The helper's locks are its own (`ai:`
//!   ids): it can't unlock the player's;
//! - `game`: which web game the session is for;
//! - `saveCheats`: adds checked cheat data to that game's list.
//!
//! - `reverted`: the tool server undid all its changes itself.
//!
//! Every request is for the game the session started with, on its own site:
//! if the player closed its window, opened another game or the page went to
//! another website, the request is refused. Nothing the helper sends is run as
//! code (the page calls are whitelisted and their arguments JSON-quoted, see
//! `web::call_script`).
//!
//! The link also keeps each place's value from before the helper first set or
//! locked it. When the session ends (`Link::finish`), the app lifts the
//! helper's locks and, unless the tool server said it undid everything, puts
//! those values back: a helper that was stopped leaves nothing behind.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use crate::web::{WebCheat, WebGame};

/// The page calls the AI helper may make.
pub const AI_CALLS: &[&str] = &[
    "info", "names", "find", "next", "results", "read", "set", "lock", "unlock", "near",
];
/// Longest request (a list of cheats is the biggest).
const MAX_LINE: u64 = 1 << 20;

/// The game window, as the link sees it (a fake one in tests).
pub trait Game: Send + Sync + 'static {
    fn open(&self) -> Option<WebGame>;
    /// The address the game window shows now.
    fn page_url(&self) -> Option<String>;
    fn call(&self, method: &str, args: &Value) -> Result<Value, String>;
    fn add_cheats(&self, id: &str, cheats: &[WebCheat]) -> Result<(usize, usize), String>;
    /// A picture of the game window: `{ png (base64), width, height }`.
    fn screenshot(&self) -> Result<Value, String>;
}

#[derive(Deserialize)]
struct Request {
    token: String,
    call: String,
    #[serde(default)]
    args: Value,
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// The AI helper's lock ids: `ai:` and a number (the player's are `name|place`).
pub fn is_ai_lock(id: &str) -> bool {
    id.strip_prefix("ai:")
        .is_some_and(|n| !n.is_empty() && n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit()))
}

fn ai_id(args: &Value) -> Result<(), String> {
    match args.get("id").and_then(Value::as_str) {
        Some(id) if is_ai_lock(id) => Ok(()),
        _ => Err("lock and unlock need one of the helper's own ids (ai:1, ai:2…); the player's locks are theirs".into()),
    }
}

/// The site a web address is on: scheme and host, without a leading `www.`.
fn site(url: &str) -> Option<(String, String)> {
    let u = tauri::Url::parse(url).ok()?;
    let host = u.host_str()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    Some((
        u.scheme().to_string(),
        format!("{host}:{}", u.port_or_known_default().unwrap_or(0)),
    ))
}

/// One helper session's link: who may ask, about which game, and what to undo.
pub struct Session {
    token: String,
    game_id: String,
    game: Arc<dyn Game>,
    /// Each place the helper set or locked since its last full undo, with the
    /// value it had before (oldest first).
    before: Mutex<Vec<(Value, f64)>>,
}

impl Session {
    pub fn new(token: String, game_id: String, game: Arc<dyn Game>) -> Self {
        Self {
            token,
            game_id,
            game,
            before: Mutex::new(Vec::new()),
        }
    }

    fn journal(&self) -> std::sync::MutexGuard<'_, Vec<(Value, f64)>> {
        self.before.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Answers one request line: `{ "ok": ... }` or `{ "error": "..." }`.
    pub fn handle(&self, line: &str) -> Value {
        match self.answer(line) {
            Ok(v) => json!({ "ok": v }),
            Err(e) => json!({ "error": e }),
        }
    }

    /// The game window still shows the session's game, on its own site.
    fn check_window(&self) -> Result<WebGame, String> {
        let open = self
            .game
            .open()
            .filter(|g| g.id == self.game_id)
            .ok_or("the game window is closed or shows another game: ask the player to open the game again (Web games, Play)")?;
        let here = self.game.page_url().as_deref().and_then(site);
        if here.is_none() || here != site(&open.url) {
            return Err("the game window has gone to another website: ask the player to open the game again (Web games, Play)".into());
        }
        Ok(open)
    }

    fn answer(&self, line: &str) -> Result<Value, String> {
        let req: Request = serde_json::from_str(line).map_err(|_| "bad request".to_string())?;
        if !same(&req.token, &self.token) {
            return Err("wrong token".into());
        }
        let open = self.check_window()?;
        match req.call.as_str() {
            "game" => Ok(json!({ "id": open.id, "name": open.name, "url": open.url })),
            // A picture of the game window only (v1.21.0).
            "screenshot" => self.game.screenshot(),
            // The tool server undid everything itself.
            "reverted" => {
                self.journal().clear();
                Ok(json!({}))
            }
            "saveCheats" => {
                let cheats: Vec<WebCheat> =
                    serde_json::from_value(req.args.get("cheats").cloned().unwrap_or(Value::Null))
                        .map_err(|e| format!("cheats: {e}"))?;
                if cheats.is_empty() {
                    return Err("no cheats to save".into());
                }
                let (added, skipped) = self.game.add_cheats(&open.id, &cheats)?;
                Ok(json!({ "added": added, "skipped": skipped }))
            }
            call if AI_CALLS.contains(&call) => {
                if matches!(call, "lock" | "unlock") {
                    ai_id(&req.args)?;
                }
                if matches!(call, "set" | "lock") {
                    self.remember(req.args.get("target"));
                }
                self.game.call(call, &req.args)
            }
            other => Err(format!("unknown call {other}")),
        }
    }

    /// Keeps a place's value before the helper first changes it.
    fn remember(&self, target: Option<&Value>) {
        let Some(t) = target else { return };
        if self.journal().iter().any(|(x, _)| x == t) {
            return;
        }
        let read = self.game.call("read", &json!({ "targets": [t] }));
        if let Some(v) = read.ok().and_then(|r| r.get(0)?.get("value")?.as_f64()) {
            self.journal().push((t.clone(), v));
        }
    }

    /// After the session: lifts the helper's locks and puts back what it
    /// changed and didn't undo itself (for example when it was stopped).
    pub fn undo(&self) {
        if self.check_window().is_err() {
            return; // The page is gone, and its changes with it.
        }
        if let Ok(state) = self.game.call("state", &json!({})) {
            for id in state["locks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if is_ai_lock(id) {
                    let _ = self.game.call("unlock", &json!({ "id": id }));
                }
            }
        }
        let changed = std::mem::take(&mut *self.journal());
        for (target, value) in changed.into_iter().rev() {
            let _ = self
                .game
                .call("set", &json!({ "target": target, "value": value }));
        }
    }
}

/// A running link. Dropping it stops listening; `finish` also undoes what
/// the helper left in the page.
pub struct Link {
    /// `127.0.0.1:<port>`.
    pub addr: String,
    pub token: String,
    session: Arc<Session>,
    stop: Arc<AtomicBool>,
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Link")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl Link {
    /// Stops listening, then lifts the helper's locks and puts back the values
    /// it changed and didn't undo.
    pub fn finish(self) {
        self.stop.store(true, Ordering::SeqCst);
        self.session.undo();
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn serve_one(stream: TcpStream, session: &Session) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut line = String::new();
    if BufReader::new(stream.take(MAX_LINE))
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let reply = session.handle(line.trim());
    let _ = writeln!(writer, "{reply}");
    let _ = writer.flush();
}

/// Starts listening for the helper's requests about `game_id`.
pub fn start(game_id: String, game: Arc<dyn Game>) -> std::io::Result<Link> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?.to_string();
    let token = crate::remote::token();
    let session = Arc::new(Session::new(token.clone(), game_id, game));
    let stop = Arc::new(AtomicBool::new(false));
    let (s, stopped) = (Arc::clone(&session), Arc::clone(&stop));
    std::thread::Builder::new()
        .name("web-link".into())
        .spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    // Its own thread, so a connection that sends nothing holds up nobody.
                    Ok((stream, _)) => {
                        let s = Arc::clone(&s);
                        let _ = std::thread::Builder::new()
                            .name("web-link-call".into())
                            .spawn(move || serve_one(stream, &s));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        })?;
    Ok(Link {
        addr,
        token,
        session,
        stop,
    })
}

/// The real game window.
struct AppGame(AppHandle);

impl Game for AppGame {
    fn open(&self) -> Option<WebGame> {
        crate::web::open_game(&self.0)
    }
    fn page_url(&self) -> Option<String> {
        let w = self.0.get_webview_window(crate::web::LABEL)?;
        w.url().ok().map(|u| u.to_string())
    }
    fn call(&self, method: &str, args: &Value) -> Result<Value, String> {
        crate::web::call_blocking(&self.0, method, args).map_err(|e| e.message)
    }
    fn add_cheats(&self, id: &str, cheats: &[WebCheat]) -> Result<(usize, usize), String> {
        let r = crate::web::add_cheats(id, cheats).map_err(|e| e.message)?;
        // The cheats panel shows them right away.
        let _ = self.0.emit("web-cheats-changed", id);
        Ok(r)
    }
    fn screenshot(&self) -> Result<Value, String> {
        use base64::Engine as _;
        let w = self
            .0
            .get_webview_window(crate::web::LABEL)
            .ok_or("the game window is closed")?;
        let hwnd = w.hwnd().map_err(|e| e.to_string())?;
        let shot = jb_core::screen::capture_window(hwnd.0 as isize, jb_core::screen::MAX_SIDE)?;
        Ok(json!({
            "png": base64::engine::general_purpose::STANDARD.encode(&shot.png),
            "width": shot.width,
            "height": shot.height,
        }))
    }
}

/// Starts the link to the app's game window.
pub fn start_for_app(app: AppHandle, game_id: String) -> std::io::Result<Link> {
    start(game_id, Arc::new(AppGame(app)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny page: places by path text, plus locks.
    struct Fake {
        open: Mutex<Option<WebGame>>,
        url: Mutex<String>,
        calls: Mutex<Vec<(String, Value)>>,
        saved: Mutex<Vec<WebCheat>>,
        page: Mutex<Value>,
        locks: Mutex<Vec<String>>,
    }

    fn key(t: &Value) -> String {
        t["path"]
            .as_array()
            .map(|p| {
                p.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .unwrap_or_default()
    }

    impl Game for Fake {
        fn open(&self) -> Option<WebGame> {
            self.open.lock().unwrap().clone()
        }
        fn page_url(&self) -> Option<String> {
            Some(self.url.lock().unwrap().clone())
        }
        fn call(&self, method: &str, args: &Value) -> Result<Value, String> {
            self.calls
                .lock()
                .unwrap()
                .push((method.to_string(), args.clone()));
            let mut page = self.page.lock().unwrap();
            Ok(match method {
                "read" => json!([{ "value": page[key(&args["targets"][0])] }]),
                "set" | "lock" => {
                    page[key(&args["target"])] = args["value"].clone();
                    if method == "lock" {
                        self.locks
                            .lock()
                            .unwrap()
                            .push(args["id"].as_str().unwrap().into());
                    }
                    json!({ "value": args["value"] })
                }
                "unlock" => {
                    let id = args["id"].as_str().unwrap_or_default().to_string();
                    self.locks.lock().unwrap().retain(|l| *l != id);
                    json!({})
                }
                "state" => json!({ "locks": *self.locks.lock().unwrap() }),
                _ => json!({ "did": method }),
            })
        }
        fn add_cheats(&self, _id: &str, cheats: &[WebCheat]) -> Result<(usize, usize), String> {
            self.saved.lock().unwrap().extend_from_slice(cheats);
            Ok((cheats.len(), 0))
        }
        fn screenshot(&self) -> Result<Value, String> {
            Ok(json!({ "png": "iVBORw0KGgo=", "width": 2, "height": 1 }))
        }
    }

    fn fake() -> Arc<Fake> {
        Arc::new(Fake {
            open: Mutex::new(Some(WebGame {
                id: "zombies-1".into(),
                name: "Zombies".into(),
                url: "https://zombies.example/play".into(),
                added: "2026-11-10".into(),
            })),
            url: Mutex::new("https://www.zombies.example/play/level2".into()),
            calls: Mutex::default(),
            saved: Mutex::default(),
            page: Mutex::new(json!({ "game.hp": 50.0, "game.gold": 10.0 })),
            locks: Mutex::new(vec!["Health|game.hp".into()]),
        })
    }

    fn session(f: &Arc<Fake>) -> Session {
        Session::new(
            "secret".into(),
            "zombies-1".into(),
            Arc::clone(f) as Arc<dyn Game>,
        )
    }

    fn ask(s: &Session, token: &str, call: &str, args: Value) -> Value {
        s.handle(&json!({ "token": token, "call": call, "args": args }).to_string())
    }

    fn page_calls(f: &Fake) -> Vec<String> {
        f.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(c, _)| c.clone())
            .collect()
    }

    #[test]
    fn passes_on_only_the_helpers_calls_for_its_game() {
        let f = fake();
        let s = session(&f);
        assert_eq!(
            ask(&s, "secret", "find", json!({ "value": 5 }))["ok"]["did"],
            "find"
        );
        assert_eq!(
            ask(&s, "secret", "names", json!({ "query": "hp" }))["ok"]["did"],
            "names"
        );
        assert_eq!(
            ask(&s, "secret", "game", json!({}))["ok"]["name"],
            "Zombies"
        );
        assert!(
            ask(&s, "wrong!", "find", json!({}))["error"]
                .as_str()
                .unwrap()
                .contains("token")
        );
        assert!(s.handle("not json")["error"].is_string());
        assert!(
            s.handle("POST / HTTP/1.1")["error"].is_string(),
            "a browser's request"
        );
        // Not for the helper: the player's keys and state, and anything unknown.
        for call in ["keys", "state", "eval", "callJson"] {
            assert!(
                ask(&s, "secret", call, json!({}))["error"]
                    .as_str()
                    .unwrap()
                    .contains("unknown"),
                "{call}"
            );
        }
        assert_eq!(page_calls(&f), ["find", "names"]);
    }

    #[test]
    fn the_helpers_locks_are_its_own() {
        let f = fake();
        let s = session(&f);
        let t = json!({ "k": "j", "path": ["game", "hp"] });
        assert!(
            ask(
                &s,
                "secret",
                "lock",
                json!({ "id": "ai:1", "target": t, "value": 9 })
            )["ok"]
                .is_object()
        );
        assert!(ask(&s, "secret", "unlock", json!({ "id": "ai:1" }))["ok"].is_object());
        // The player's locks (even one named "ai:…"), or all locks at once, aren't the helper's.
        for id in [
            json!("Health|game.hp"),
            json!("ai:x|game.hp"),
            json!("ai:"),
            json!(null),
        ] {
            assert!(
                ask(&s, "secret", "unlock", json!({ "id": id }))["error"].is_string(),
                "{id}"
            );
        }
        assert!(
            ask(
                &s,
                "secret",
                "lock",
                json!({ "id": "Gold|x", "target": t, "value": 1 })
            )["error"]
                .is_string()
        );
        assert!(is_ai_lock("ai:12") && !is_ai_lock("ai:1|x") && !is_ai_lock("x"));
    }

    #[test]
    fn refuses_when_the_window_shows_another_game_or_site_or_is_closed() {
        let f = fake();
        let s = session(&f);
        // The same site (with or without www) is fine.
        assert!(ask(&s, "secret", "info", json!({}))["ok"].is_object());
        *f.url.lock().unwrap() = "https://evil.example/zombies.example".into();
        assert!(
            ask(&s, "secret", "find", json!({}))["error"]
                .as_str()
                .unwrap()
                .contains("another website")
        );
        *f.url.lock().unwrap() = "http://zombies.example/play".into();
        assert!(
            ask(&s, "secret", "find", json!({}))["error"].is_string(),
            "not https any more"
        );
        *f.url.lock().unwrap() = "https://zombies.example/".into();
        f.open.lock().unwrap().as_mut().unwrap().id = "other-2".into();
        assert!(
            ask(&s, "secret", "find", json!({}))["error"]
                .as_str()
                .unwrap()
                .contains("open the game again")
        );
        *f.open.lock().unwrap() = None;
        assert!(ask(&s, "secret", "info", json!({}))["error"].is_string());
        assert_eq!(page_calls(&f), ["info"]);
    }

    #[test]
    fn saves_cheats_as_data() {
        let f = fake();
        let s = session(&f);
        let r = ask(
            &s,
            "secret",
            "saveCheats",
            json!({ "cheats": [{ "name": "Ammo", "target": { "k": "j", "path": ["game", "ammo"] }, "value": 99 }] }),
        );
        assert_eq!(r["ok"]["added"], 1);
        assert_eq!(f.saved.lock().unwrap()[0].name, "Ammo");
        // Only cheat data: anything else in a cheat is refused.
        let bad = ask(
            &s,
            "secret",
            "saveCheats",
            json!({ "cheats": [{ "name": "x", "target": { "k": "j", "path": ["a"], "code": "evil()" } }] }),
        );
        assert!(bad["error"].is_string());
        assert!(ask(&s, "secret", "saveCheats", json!({ "cheats": [] }))["error"].is_string());
    }

    #[test]
    fn undoes_what_a_stopped_helper_left_in_the_page() {
        let f = fake();
        let s = session(&f);
        let hp = json!({ "k": "j", "path": ["game", "hp"] });
        let gold = json!({ "k": "j", "path": ["game", "gold"] });
        ask(&s, "secret", "set", json!({ "target": hp, "value": 999 }));
        ask(&s, "secret", "set", json!({ "target": hp, "value": 5000 }));
        ask(
            &s,
            "secret",
            "lock",
            json!({ "id": "ai:1", "target": gold, "value": 77 }),
        );
        // The helper was stopped before it undid anything: the app does it.
        s.undo();
        let page = f.page.lock().unwrap().clone();
        assert_eq!(
            page["game.hp"], 50.0,
            "the value from before the first change"
        );
        assert_eq!(page["game.gold"], 10.0);
        assert_eq!(
            *f.locks.lock().unwrap(),
            ["Health|game.hp"],
            "the player's lock stays"
        );

        // When the tool server undid everything itself, nothing is put back twice
        // (the game may have changed the value since).
        let f = fake();
        let s = session(&f);
        ask(&s, "secret", "set", json!({ "target": hp, "value": 999 }));
        ask(&s, "secret", "set", json!({ "target": hp, "value": 50 }));
        ask(&s, "secret", "reverted", json!({}));
        f.page.lock().unwrap()["game.hp"] = json!(42.0); // the player took a hit
        s.undo();
        assert_eq!(f.page.lock().unwrap()["game.hp"], 42.0);
    }

    #[test]
    fn listens_on_this_pc_only_and_stops_when_finished() {
        let f = fake();
        let link = start("zombies-1".into(), Arc::clone(&f) as Arc<dyn Game>).unwrap();
        assert!(link.addr.starts_with("127.0.0.1:"));
        assert!(
            !format!("{link:?}").contains(&link.token),
            "the token isn't printed"
        );
        let token = link.token.clone();
        let request = |addr: &str, line: &str| -> std::io::Result<Value> {
            let mut s = TcpStream::connect(addr)?;
            s.set_read_timeout(Some(Duration::from_secs(5)))?;
            writeln!(s, "{line}")?;
            let mut reply = String::new();
            BufReader::new(s).read_line(&mut reply)?;
            Ok(serde_json::from_str(&reply).unwrap_or(Value::Null))
        };
        // A connection that sends nothing doesn't hold up the helper.
        let _idle = TcpStream::connect(&link.addr).unwrap();
        let game = json!({ "token": token, "call": "game" }).to_string();
        assert_eq!(request(&link.addr, &game).unwrap()["ok"]["id"], "zombies-1");
        let addr = link.addr.clone();
        link.finish();
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            request(&addr, &game).is_err(),
            "nothing answers after the session"
        );
    }
}
