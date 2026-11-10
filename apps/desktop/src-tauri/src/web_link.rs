//! The AI helper's link to the web game window (v1.20.0).
//!
//! The helper's guarded tool server (`opentrainer-desktop --mcp`, the ot-mcp
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
//! Every request is for the game the session started with: if the player
//! closed its window or opened another game, the request is refused. Nothing
//! the helper sends is run as code (the page calls are whitelisted and their
//! arguments JSON-quoted, see `web::call_script`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter};

use crate::web::{WebCheat, WebGame};

/// The page calls the AI helper may make.
pub const AI_CALLS: &[&str] = &[
    "info", "names", "find", "next", "results", "read", "set", "lock", "unlock",
];
/// Longest request (a list of cheats is the biggest).
const MAX_LINE: u64 = 1 << 20;
/// Locks the AI helper makes start with this.
pub const AI_LOCK: &str = "ai:";

/// The game window, as the link sees it (a fake one in tests).
pub trait Game: Send + Sync + 'static {
    fn open(&self) -> Option<WebGame>;
    fn call(&self, method: &str, args: &Value) -> Result<Value, String>;
    fn add_cheats(&self, id: &str, cheats: &[WebCheat]) -> Result<(usize, usize), String>;
}

#[derive(Debug, Deserialize)]
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

fn ai_id(args: &Value) -> Result<(), String> {
    match args.get("id").and_then(Value::as_str) {
        Some(id) if id.starts_with(AI_LOCK) && id.len() <= 100 => Ok(()),
        _ => Err(format!(
            "lock and unlock need an id starting with \"{AI_LOCK}\" (the player's own locks are theirs)"
        )),
    }
}

/// Answers one request line: `{ "ok": ... }` or `{ "error": "..." }`.
pub fn handle(line: &str, token: &str, game_id: &str, game: &dyn Game) -> Value {
    match answer(line, token, game_id, game) {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "error": e }),
    }
}

fn answer(line: &str, token: &str, game_id: &str, game: &dyn Game) -> Result<Value, String> {
    let req: Request = serde_json::from_str(line).map_err(|_| "bad request".to_string())?;
    if !same(&req.token, token) {
        return Err("wrong token".into());
    }
    let open = game
        .open()
        .filter(|g| g.id == game_id)
        .ok_or("the game window is closed or shows another game: ask the player to open the game again (Web games, Play)")?;
    match req.call.as_str() {
        "game" => Ok(json!({ "id": open.id, "name": open.name, "url": open.url })),
        "saveCheats" => {
            let cheats: Vec<WebCheat> =
                serde_json::from_value(req.args.get("cheats").cloned().unwrap_or(Value::Null))
                    .map_err(|e| format!("cheats: {e}"))?;
            if cheats.is_empty() {
                return Err("no cheats to save".into());
            }
            let (added, skipped) = game.add_cheats(&open.id, &cheats)?;
            Ok(json!({ "added": added, "skipped": skipped }))
        }
        call if AI_CALLS.contains(&call) => {
            if matches!(call, "lock" | "unlock") {
                ai_id(&req.args)?;
            }
            game.call(call, &req.args)
        }
        other => Err(format!("unknown call {other}")),
    }
}

/// A running link. Dropping it stops listening.
#[derive(Debug)]
pub struct Link {
    /// `127.0.0.1:<port>`.
    pub addr: String,
    pub token: String,
    stop: Arc<AtomicBool>,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn serve_one(stream: TcpStream, token: &str, game_id: &str, game: &dyn Game) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
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
    let reply = handle(line.trim(), token, game_id, game);
    let _ = writeln!(writer, "{reply}");
    let _ = writer.flush();
}

/// Starts listening for the helper's requests about `game_id`.
pub fn start(game_id: String, game: Arc<dyn Game>) -> std::io::Result<Link> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?.to_string();
    let token = crate::remote::token();
    let stop = Arc::new(AtomicBool::new(false));
    let (t, s) = (token.clone(), Arc::clone(&stop));
    std::thread::Builder::new()
        .name("web-link".into())
        .spawn(move || {
            // One request at a time (the tool server waits for each answer).
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => serve_one(stream, &t, &game_id, game.as_ref()),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(100)),
                }
            }
        })?;
    Ok(Link { addr, token, stop })
}

/// The real game window.
struct AppGame(AppHandle);

impl Game for AppGame {
    fn open(&self) -> Option<WebGame> {
        crate::web::open_game(&self.0)
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
}

/// Starts the link to the app's game window.
pub fn start_for_app(app: AppHandle, game_id: String) -> std::io::Result<Link> {
    start(game_id, Arc::new(AppGame(app)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        open: Mutex<Option<WebGame>>,
        calls: Mutex<Vec<(String, Value)>>,
        saved: Mutex<Vec<WebCheat>>,
    }

    impl Game for Fake {
        fn open(&self) -> Option<WebGame> {
            self.open.lock().unwrap().clone()
        }
        fn call(&self, method: &str, args: &Value) -> Result<Value, String> {
            self.calls
                .lock()
                .unwrap()
                .push((method.to_string(), args.clone()));
            Ok(json!({ "did": method }))
        }
        fn add_cheats(&self, _id: &str, cheats: &[WebCheat]) -> Result<(usize, usize), String> {
            self.saved.lock().unwrap().extend_from_slice(cheats);
            Ok((cheats.len(), 0))
        }
    }

    fn fake() -> Fake {
        let f = Fake::default();
        *f.open.lock().unwrap() = Some(WebGame {
            id: "zombies-1".into(),
            name: "Zombies".into(),
            url: "https://zombies.example/".into(),
            added: "2026-11-10".into(),
        });
        f
    }

    fn ask(f: &Fake, token: &str, call: &str, args: Value) -> Value {
        let line = json!({ "token": token, "call": call, "args": args }).to_string();
        handle(&line, "secret", "zombies-1", f)
    }

    #[test]
    fn passes_on_only_the_helpers_calls_for_its_game() {
        let f = fake();
        assert_eq!(
            ask(&f, "secret", "find", json!({ "value": 5 }))["ok"]["did"],
            "find"
        );
        assert_eq!(
            ask(&f, "secret", "names", json!({ "query": "hp" }))["ok"]["did"],
            "names"
        );
        assert_eq!(
            ask(&f, "secret", "game", json!({}))["ok"]["name"],
            "Zombies"
        );
        assert!(
            ask(&f, "wrong!", "find", json!({}))["error"]
                .as_str()
                .unwrap()
                .contains("token")
        );
        assert!(handle("not json", "secret", "zombies-1", &f)["error"].is_string());
        // Not for the helper: the player's keys, and anything unknown.
        for call in ["keys", "state", "eval", "callJson"] {
            assert!(
                ask(&f, "secret", call, json!({}))["error"]
                    .as_str()
                    .unwrap()
                    .contains("unknown"),
                "{call}"
            );
        }
        let calls = f.calls.lock().unwrap().clone();
        assert_eq!(
            calls.len(),
            2,
            "only find and names reached the page: {calls:?}"
        );
    }

    #[test]
    fn the_helpers_locks_are_its_own() {
        let f = fake();
        let t = json!({ "k": "j", "path": ["game", "hp"] });
        assert!(
            ask(
                &f,
                "secret",
                "lock",
                json!({ "id": "ai:1", "target": t, "value": 9 })
            )["ok"]
                .is_object()
        );
        assert!(ask(&f, "secret", "unlock", json!({ "id": "ai:1" }))["ok"].is_object());
        // The player's lock, or all locks at once, aren't the helper's to undo.
        assert!(
            ask(&f, "secret", "unlock", json!({ "id": "Health|game.hp" }))["error"].is_string()
        );
        assert!(ask(&f, "secret", "unlock", json!({}))["error"].is_string());
        assert!(
            ask(
                &f,
                "secret",
                "lock",
                json!({ "id": "Gold|x", "target": t, "value": 1 })
            )["error"]
                .is_string()
        );
    }

    #[test]
    fn refuses_when_the_window_shows_another_game_or_is_closed() {
        let f = fake();
        f.open.lock().unwrap().as_mut().unwrap().id = "other-2".into();
        assert!(
            ask(&f, "secret", "find", json!({}))["error"]
                .as_str()
                .unwrap()
                .contains("open the game again")
        );
        *f.open.lock().unwrap() = None;
        assert!(ask(&f, "secret", "info", json!({}))["error"].is_string());
        assert!(f.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn saves_cheats_as_data() {
        let f = fake();
        let r = ask(
            &f,
            "secret",
            "saveCheats",
            json!({ "cheats": [{ "name": "Ammo", "target": { "k": "j", "path": ["game", "ammo"] }, "value": 99 }] }),
        );
        assert_eq!(r["ok"]["added"], 1);
        assert_eq!(f.saved.lock().unwrap()[0].name, "Ammo");
        // Only cheat data: anything else in a cheat is refused.
        let bad = ask(
            &f,
            "secret",
            "saveCheats",
            json!({ "cheats": [{ "name": "x", "target": { "k": "j", "path": ["a"], "code": "evil()" } }] }),
        );
        assert!(bad["error"].is_string());
        assert!(ask(&f, "secret", "saveCheats", json!({ "cheats": [] }))["error"].is_string());
    }

    #[test]
    fn listens_on_this_pc_only_and_stops_when_dropped() {
        let link = start("zombies-1".into(), Arc::new(fake())).unwrap();
        assert!(link.addr.starts_with("127.0.0.1:"));
        let token = link.token.clone();
        let request = |addr: &str| -> std::io::Result<Value> {
            let mut s = TcpStream::connect(addr)?;
            s.set_read_timeout(Some(Duration::from_secs(5)))?;
            let line = json!({ "token": token, "call": "game" });
            writeln!(s, "{line}")?;
            let mut reply = String::new();
            BufReader::new(s).read_line(&mut reply)?;
            Ok(serde_json::from_str(&reply).unwrap_or(Value::Null))
        };
        assert_eq!(request(&link.addr).unwrap()["ok"]["id"], "zombies-1");
        let addr = link.addr.clone();
        drop(link);
        std::thread::sleep(Duration::from_millis(200));
        assert!(request(&addr).is_err(), "nothing answers after the session");
    }
}
