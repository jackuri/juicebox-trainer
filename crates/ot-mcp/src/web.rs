//! Web game tools (v1.20.0). The game runs in OpenTrainer's game window, in the
//! app's own process, so these tools reach it through the app's local link
//! (`OT_WEB_LINK` / `OT_WEB_TOKEN`, given only to a helper session started for
//! a web game; see the app's `web_link.rs`). The app passes on only the page
//! helper's search, read, set and lock calls, and cheat data to save.
//!
//! Like memory writes in a PC game, every change here is a test: `web_set`
//! remembers the value it replaced and `web_lock` its lock, and `revert_all`
//! (or the server exiting) puts the values back and lifts the locks.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::{Value, json};

/// How long one call may take (a first search in a big game can be slow).
const TIMEOUT: Duration = Duration::from_secs(60);
/// Results shown to the model per search.
const SHOWN: usize = 20;

/// Where the app's link listens, and its token.
#[derive(Clone, PartialEq, Eq)]
pub struct LinkConfig {
    pub addr: String,
    pub token: String,
}

impl std::fmt::Debug for LinkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the token.
        f.debug_struct("LinkConfig")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

/// Longest text passed on from the page (names, titles): anything longer is
/// cut, so a page can't fill the helper's context with its own words.
const MAX_TEXT: usize = 300;

/// Cuts every string in `v` to `MAX_TEXT` characters.
fn clip(v: Value) -> Value {
    match v {
        Value::String(s) if s.chars().count() > MAX_TEXT => {
            Value::String(s.chars().take(MAX_TEXT).collect::<String>() + "…")
        }
        Value::Array(a) => Value::Array(a.into_iter().map(clip).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, clip(v))).collect()),
        other => other,
    }
}

impl LinkConfig {
    /// From the environment the app starts the helper with.
    pub fn from_env() -> Option<Self> {
        let addr = std::env::var("OT_WEB_LINK").ok()?;
        let token = std::env::var("OT_WEB_TOKEN").ok()?;
        (!addr.is_empty() && !token.is_empty()).then_some(Self { addr, token })
    }
}

/// The web game session: the link and what to undo.
#[derive(Debug)]
pub struct WebSession {
    link: LinkConfig,
    /// Test writes, oldest first: (place, the value it had).
    sets: Vec<(Value, f64)>,
    /// Lock ids this session made.
    locks: Vec<String>,
    next_lock: u32,
}

type Res = Result<Value, String>;

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a Value, String> {
    args.get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| format!("missing argument '{key}'"))
}

fn number(args: &Value, key: &str) -> Result<f64, String> {
    let v = arg(args, key)?;
    let n = v
        .as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
        .ok_or_else(|| format!("'{key}' must be a number"))?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(format!("'{key}' must be a finite number"))
    }
}

/// A place in the page, checked like a saved cheat's (`k` "m" or "j").
fn target(args: &Value) -> Result<Value, String> {
    let t = arg(args, "target")?.clone();
    let probe = json!({ "name": "test", "target": t });
    let cheat: ot_packs::web::WebCheat =
        serde_json::from_value(probe).map_err(|e| format!("target: {e}"))?;
    ot_packs::web::check_cheat(&cheat).map_err(|e| format!("target: {e}"))?;
    serde_json::to_value(&cheat.target).map_err(|e| e.to_string())
}

/// Keeps a search answer short: the count and the first results.
fn short(mut v: Value) -> Value {
    if let Some(list) = v.get_mut("results").and_then(Value::as_array_mut) {
        list.truncate(SHOWN);
    }
    v
}

impl WebSession {
    pub fn new(link: LinkConfig) -> Self {
        Self {
            link,
            sets: Vec::new(),
            locks: Vec::new(),
            next_lock: 1,
        }
    }

    /// One request to the app.
    fn ask(&self, call: &str, args: Value) -> Res {
        let fail = |e: std::io::Error| format!("couldn't reach OpenTrainer's game window: {e}");
        let mut s = TcpStream::connect(&self.link.addr).map_err(fail)?;
        s.set_read_timeout(Some(TIMEOUT)).map_err(fail)?;
        let line = json!({ "token": self.link.token, "call": call, "args": args });
        writeln!(s, "{line}").map_err(fail)?;
        s.flush().map_err(fail)?;
        let mut reply = String::new();
        BufReader::new(s).read_line(&mut reply).map_err(fail)?;
        let v: Value = serde_json::from_str(&reply)
            .map_err(|_| "OpenTrainer's game window answered oddly".to_string())?;
        if let Some(e) = v.get("error").and_then(Value::as_str) {
            return Err(e.to_string());
        }
        Ok(clip(v.get("ok").cloned().unwrap_or(Value::Null)))
    }

    fn read_one(&self, target: &Value) -> Result<f64, String> {
        let r = self.ask("read", json!({ "targets": [target] }))?;
        let first = r.get(0).cloned().unwrap_or(Value::Null);
        if let Some(e) = first.get("error").and_then(Value::as_str) {
            return Err(e.to_string());
        }
        first
            .get("value")
            .and_then(Value::as_f64)
            .ok_or_else(|| "that value can't be read".to_string())
    }

    pub fn call(&mut self, name: &str, args: &Value) -> Res {
        match name {
            "web_game" => {
                let game = self.ask("game", json!({}))?;
                let info = self.ask("info", json!({}))?;
                Ok(json!({ "game": game, "page": info }))
            }
            "web_names" => {
                let query = arg(args, "query")?
                    .as_str()
                    .ok_or("'query' must be text")?
                    .to_string();
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(30)
                    .min(100);
                self.ask("names", json!({ "query": query, "limit": limit }))
            }
            "web_find" => {
                let value = number(args, "value")?;
                let kind = match args.get("kind").and_then(Value::as_str) {
                    None | Some("any") => "any",
                    Some("whole") => "whole",
                    Some("decimal") => "decimal",
                    Some(k) => return Err(format!("kind must be any, whole or decimal, not {k}")),
                };
                self.ask("find", json!({ "value": value, "kind": kind }))
                    .map(short)
            }
            "web_next" => {
                let how = arg(args, "how")?.as_str().ok_or("'how' must be text")?;
                if !["equals", "changed", "unchanged", "increased", "decreased"].contains(&how) {
                    return Err(
                        "how must be equals, changed, unchanged, increased or decreased".into(),
                    );
                }
                let value = if how == "equals" {
                    Some(number(args, "value")?)
                } else {
                    None
                };
                self.ask("next", json!({ "how": how, "value": value }))
                    .map(short)
            }
            "web_results" => self.ask("results", json!({})).map(short),
            "web_read" => {
                let list = arg(args, "targets")?
                    .as_array()
                    .ok_or("'targets' must be a list")?;
                if list.is_empty() || list.len() > 50 {
                    return Err("read 1 to 50 places at a time".into());
                }
                let targets = list
                    .iter()
                    .map(|t| target(&json!({ "target": t })))
                    .collect::<Result<Vec<_>, _>>()?;
                self.ask("read", json!({ "targets": targets }))
            }
            "web_set" => {
                let t = target(args)?;
                let value = number(args, "value")?;
                let before = self.read_one(&t)?;
                let r = self.ask("set", json!({ "target": t, "value": value }))?;
                self.sets.push((t, before));
                Ok(json!({ "before": before, "now": r.get("value") }))
            }
            "web_lock" => {
                let t = target(args)?;
                let value = match args.get("value").filter(|v| !v.is_null()) {
                    Some(_) => number(args, "value")?,
                    None => self.read_one(&t)?,
                };
                let before = self.read_one(&t)?;
                let id = format!("ai:{}", self.next_lock);
                self.next_lock += 1;
                self.ask("lock", json!({ "id": id, "target": t, "value": value }))?;
                self.sets.push((t, before));
                self.locks.push(id.clone());
                Ok(json!({ "lock": id, "value": value, "before": before }))
            }
            "web_unlock" => {
                let id = arg(args, "lock")?.as_str().ok_or("'lock' must be text")?;
                if !self.locks.iter().any(|l| l == id) {
                    return Err(format!("{id} isn't one of your locks"));
                }
                self.ask("unlock", json!({ "id": id }))?;
                self.locks.retain(|l| l != id);
                Ok(json!({ "unlocked": id }))
            }
            "web_cheats_save" => {
                let list = arg(args, "cheats")?
                    .as_array()
                    .ok_or("'cheats' must be a list")?;
                if list.is_empty() || list.len() > ot_packs::web::MAX_CHEATS {
                    return Err("save 1 to 200 cheats".into());
                }
                let mut cheats = Vec::new();
                for (i, c) in list.iter().enumerate() {
                    let cheat: ot_packs::web::WebCheat = serde_json::from_value(c.clone())
                        .map_err(|e| format!("cheat {}: {e}", i + 1))?;
                    let cheat = ot_packs::web::check_cheat(&cheat)
                        .map_err(|e| format!("cheat {}: {e}", i + 1))?;
                    cheats.push(cheat);
                }
                self.ask("saveCheats", json!({ "cheats": cheats }))
            }
            _ => Err(format!("unknown tool {name}")),
        }
    }

    /// Lifts this session's locks and puts back the values it changed
    /// (newest first). Returns what couldn't be undone.
    pub fn revert(&mut self) -> Vec<String> {
        let mut problems = Vec::new();
        for id in std::mem::take(&mut self.locks) {
            if let Err(e) = self.ask("unlock", json!({ "id": id })) {
                problems.push(format!("{id}: {e}"));
            }
        }
        for (t, before) in std::mem::take(&mut self.sets).into_iter().rev() {
            if let Err(e) = self.ask("set", json!({ "target": t, "value": before })) {
                problems.push(e);
            }
        }
        // All undone: the app needn't put anything back after the session.
        if problems.is_empty() {
            let _ = self.ask("reverted", json!({}));
        }
        problems
    }

    /// Something to undo.
    pub fn changed(&self) -> bool {
        !self.sets.is_empty() || !self.locks.is_empty()
    }
}

impl Drop for WebSession {
    fn drop(&mut self) {
        if self.changed() {
            for p in self.revert() {
                tracing::warn!("could not undo a web game change: {p}");
            }
        }
    }
}

/// Tool definitions for the web tools.
pub fn definitions() -> Vec<Value> {
    let place = json!({
        "type": "object",
        "description": "A place in the page, as the search results give it: {\"k\":\"j\",\"path\":[\"game\",\"player\",\"hp\"]} (a JavaScript value) or {\"k\":\"m\",\"mem\":0,\"label\":\"...\",\"size\":16777216,\"off\":4096,\"t\":\"i32\"} (a number in a memory; t is u8, i16, i32, f32 or f64)."
    });
    let tool = |name: &str, description: &str, props: Value, required: &[&str]| {
        json!({
            "name": name,
            "description": description,
            "inputSchema": { "type": "object", "properties": props, "required": required, "additionalProperties": false },
        })
    };
    let cheat = json!({
        "type": "object",
        "description": "{\"name\":\"Infinite ammo\",\"target\":<place>,\"value\":99}. Optional \"key\" (e.g. \"F1\") and \"keyDoes\" (\"lock\", the default, or \"set\").",
    });
    vec![
        tool(
            "web_game",
            "The web game open in OpenTrainer's game window: its name and address, and the page's memories (WebAssembly or big arrays) with their sizes.",
            json!({}),
            &[],
        ),
        tool(
            "web_names",
            "Find numbers in the page's JavaScript values by name (e.g. \"hp|health|life\" or \"gold,coins\"): each with its place, path and current value. Try this first: many web games keep their numbers in named JavaScript values.",
            json!({ "query": { "type": "string" }, "limit": { "type": "integer" } }),
            &["query"],
        ),
        tool(
            "web_find",
            "First search for a number the player sees, in the page's memories and JavaScript values. kind: any (default), whole or decimal. Returns the count and the first results.",
            json!({ "value": { "type": ["number", "string"] }, "kind": { "type": "string", "enum": ["any", "whole", "decimal"] } }),
            &["value"],
        ),
        tool(
            "web_next",
            "Narrow the last search after the value changed in the game. how: equals (with value), changed, unchanged, increased, decreased.",
            json!({ "how": { "type": "string", "enum": ["equals", "changed", "unchanged", "increased", "decreased"] }, "value": { "type": ["number", "string"] } }),
            &["how"],
        ),
        tool(
            "web_results",
            "The current search results again (with their values now).",
            json!({}),
            &[],
        ),
        tool(
            "web_read",
            "Read the current value of up to 50 places.",
            json!({ "targets": { "type": "array", "items": place } }),
            &["targets"],
        ),
        tool(
            "web_set",
            "Test: set a place to a number (undone by revert_all).",
            json!({ "target": place, "value": { "type": ["number", "string"] } }),
            &["target", "value"],
        ),
        tool(
            "web_lock",
            "Test: keep a place at a number (its value now if no value is given) until web_unlock or revert_all.",
            json!({ "target": place, "value": { "type": ["number", "string"] } }),
            &["target"],
        ),
        tool(
            "web_unlock",
            "Stop one of your test locks (the id web_lock gave).",
            json!({ "lock": { "type": "string" } }),
            &["lock"],
        ),
        tool(
            "web_cheats_save",
            "Save tested cheats to the player's list for this web game (they show in the game's cheats right away). Cheats already in the list are skipped.",
            json!({ "cheats": { "type": "array", "items": cheat } }),
            &["cheats"],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A fake app link with a tiny page: places by path text, plus locks.
    /// The calls the fake link got: (call, args).
    type Calls = Arc<Mutex<Vec<(String, Value)>>>;

    fn fake_link() -> (LinkConfig, Calls, Arc<Mutex<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let page = Arc::new(Mutex::new(json!({ "game.hp": 50.0, "game.gold": 10.0 })));
        let (c, p) = (Arc::clone(&calls), Arc::clone(&page));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                let req: Value = serde_json::from_str(&line).unwrap();
                let call = req["call"].as_str().unwrap().to_string();
                let args = req["args"].clone();
                c.lock().unwrap().push((call.clone(), args.clone()));
                let key = |t: &Value| {
                    t["path"]
                        .as_array()
                        .map(|p| {
                            p.iter()
                                .map(|s| s.as_str().unwrap())
                                .collect::<Vec<_>>()
                                .join(".")
                        })
                        .unwrap_or_default()
                };
                let reply = if req["token"] != "t0k" {
                    json!({ "error": "wrong token" })
                } else {
                    match call.as_str() {
                        "read" => {
                            let page = p.lock().unwrap();
                            let out: Vec<Value> = args["targets"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|t| json!({ "value": page[key(t)] }))
                                .collect();
                            json!({ "ok": out })
                        }
                        "set" | "lock" => {
                            p.lock().unwrap()[key(&args["target"])] = args["value"].clone();
                            json!({ "ok": { "value": args["value"] } })
                        }
                        "find" => {
                            json!({ "ok": { "count": 30, "results": (0..30).map(|i| json!({ "id": i })).collect::<Vec<_>>() } })
                        }
                        _ => json!({ "ok": { "did": call } }),
                    }
                };
                let mut w = stream;
                writeln!(w, "{reply}").unwrap();
            }
        });
        (
            LinkConfig {
                addr,
                token: "t0k".into(),
            },
            calls,
            page,
        )
    }

    fn hp() -> Value {
        json!({ "k": "j", "path": ["game", "hp"] })
    }

    #[test]
    fn tests_are_undone_and_locks_are_its_own() {
        let (link, calls, page) = fake_link();
        let mut s = WebSession::new(link);
        let r = s
            .call("web_set", &json!({ "target": hp(), "value": 999 }))
            .unwrap();
        assert_eq!(r["before"], 50.0);
        assert_eq!(page.lock().unwrap()["game.hp"], 999.0);
        let lock = s
            .call(
                "web_lock",
                &json!({ "target": { "k": "j", "path": ["game", "gold"] }, "value": 77 }),
            )
            .unwrap();
        assert_eq!(lock["lock"], "ai:1");
        assert!(
            s.call("web_unlock", &json!({ "lock": "Health|game.hp" }))
                .is_err(),
            "not its lock"
        );
        assert!(s.changed());
        assert!(s.revert().is_empty());
        assert_eq!(page.lock().unwrap()["game.hp"], 50.0);
        assert_eq!(page.lock().unwrap()["game.gold"], 10.0);
        assert!(!s.changed());
        let calls = calls.lock().unwrap();
        assert!(
            calls
                .iter()
                .any(|(c, a)| c == "unlock" && a["id"] == "ai:1")
        );
        assert_eq!(
            calls.last().unwrap().0,
            "reverted",
            "tells the app it's all undone"
        );
    }

    #[test]
    fn checks_arguments_before_asking_the_page() {
        let (link, calls, _) = fake_link();
        let mut s = WebSession::new(link);
        assert!(
            s.call(
                "web_set",
                &json!({ "target": { "k": "j", "path": [] }, "value": 1 })
            )
            .is_err()
        );
        assert!(
            s.call(
                "web_set",
                &json!({ "target": { "k": "j", "path": ["a"], "code": "x()" }, "value": 1 })
            )
            .is_err()
        );
        assert!(
            s.call("web_set", &json!({ "target": hp(), "value": "lots" }))
                .is_err()
        );
        assert!(
            s.call("web_find", &json!({ "value": 5, "kind": "huge" }))
                .is_err()
        );
        assert!(
            s.call("web_next", &json!({ "how": "equals" })).is_err(),
            "equals needs a value"
        );
        assert!(
            s.call(
                "web_cheats_save",
                &json!({ "cheats": [{ "name": "", "target": hp() }] })
            )
            .is_err()
        );
        assert!(calls.lock().unwrap().is_empty(), "nothing reached the page");
        // A search answer is kept short for the model.
        let found = s.call("web_find", &json!({ "value": "5" })).unwrap();
        assert_eq!(found["count"], 30);
        assert_eq!(found["results"].as_array().unwrap().len(), SHOWN);
        assert_eq!(
            s.call(
                "web_cheats_save",
                &json!({ "cheats": [{ "name": "HP", "target": hp(), "value": 100, "key": "f1" }] })
            )
            .unwrap()["did"],
            "saveCheats"
        );
        let saved = calls.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(
            saved["cheats"][0]["key"], "F1",
            "tidied like the app's cheats"
        );
    }

    #[test]
    fn long_page_text_is_cut_and_the_token_never_printed() {
        let long = "x".repeat(5000);
        let v = clip(json!({ "where": long, "list": [long], "n": 5 }));
        assert_eq!(v["where"].as_str().unwrap().chars().count(), MAX_TEXT + 1);
        assert_eq!(v["list"][0].as_str().unwrap().chars().count(), MAX_TEXT + 1);
        assert_eq!(v["n"], 5);
        let link = LinkConfig {
            addr: "127.0.0.1:1".into(),
            token: "s3cret".into(),
        };
        assert!(!format!("{link:?}").contains("s3cret"));
    }

    #[test]
    fn a_wrong_token_is_refused_by_the_link() {
        let (mut link, _, _) = fake_link();
        link.token = "nope".into();
        let mut s = WebSession::new(link);
        assert!(
            s.call("web_game", &json!({}))
                .unwrap_err()
                .contains("token")
        );
    }
}
