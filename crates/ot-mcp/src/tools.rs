//! The tools the AI agent gets (PLAN.md §5.2). Every tool goes through the
//! same guarded `ot-core` calls as the app's UI:
//! - attaching runs the anti-cheat guard (no off switch) and backs up the
//!   game's saves before anything else;
//! - memory writes are test writes: the original bytes are kept and put back
//!   by `revert_all`, on detach and when the server exits;
//! - cheats are declarative pack data, loaded and toggled by the trainer
//!   runtime (validated, patches byte-checked). Nothing the agent sends is
//!   ever run as code;
//! - every call is logged (one JSON line per call).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ot_core::pointer::{PointerPath, PointerScanOptions};
use ot_core::{
    AttachedProcess, FirstScan, NextScan, Pattern, Scan, ScanControl, ScanOptions, ValueType,
};
use ot_packs::{Pack, Trainer};
use serde_json::{Value, json};

/// Shown to the model at `initialize`.
pub const INSTRUCTIONS: &str = "OpenTrainer tools for making cheat packs for single-player PC games. \
Attach to the game first (the anti-cheat guard decides; a refusal is final). Find values with \
scan_first/scan_next while the player changes them in game, make them stable with pointer_scan or \
make_aob, then build a pack and try it with pack_try and cheat_set. Memory writes are tests and \
are undone by revert_all. Save the finished pack with pack_write.";

const MAX_HITS_SHOWN: usize = 20;
const MAX_PATHS_SHOWN: usize = 20;

/// Where the server keeps things. `None` turns a feature off (tests).
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Crash-safe journal for code patches (shared with the app).
    pub journal_dir: Option<PathBuf>,
    /// One JSON line per tool call.
    pub log_path: Option<PathBuf>,
    /// Where `pack_write` saves packs (`%APPDATA%\OpenTrainer\packs`).
    pub packs_dir: Option<PathBuf>,
    /// Save backups (`%LOCALAPPDATA%\OpenTrainer\saves`).
    pub saves_root: Option<PathBuf>,
}

impl Config {
    /// The app's usual folders, with a new log file per run.
    pub fn standard() -> Self {
        let local = ot_saves::Roots::current().local;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Self {
            journal_dir: ot_core::journal::default_dir(),
            log_path: local.map(|l| {
                l.join("OpenTrainer")
                    .join("ai-logs")
                    .join(format!("ot-mcp-{stamp}.jsonl"))
            }),
            packs_dir: ot_packs::store::user_dir(),
            saves_root: ot_saves::default_root(),
        }
    }
}

struct Live {
    process: Arc<AttachedProcess>,
    scan: Option<Scan>,
    /// Address → original bytes of every test write.
    writes: BTreeMap<u64, Vec<u8>>,
    trainer: Option<Trainer>,
    pointers: Vec<PointerPath>,
}

impl Live {
    /// Puts back everything this session changed.
    fn revert(&mut self) -> Vec<String> {
        let mut problems = Vec::new();
        if let Some(mut t) = self.trainer.take()
            && let Err(e) = t.restore_all()
        {
            problems.push(e.to_string());
        }
        for (address, bytes) in std::mem::take(&mut self.writes) {
            if let Err(e) = self.process.write_bytes(address, &bytes) {
                problems.push(format!("{address:#X}: {e}"));
            }
        }
        problems
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        for p in self.revert() {
            tracing::warn!("could not undo a change: {p}");
        }
    }
}

/// The tool server's state: at most one attached game.
pub struct Tools {
    cfg: Config,
    live: Option<Live>,
    log: Option<File>,
    control: ScanControl,
}

impl std::fmt::Debug for Tools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tools")
            .field("attached", &self.live.as_ref().map(|l| l.process.pid()))
            .finish()
    }
}

type Res = Result<Value, String>;

fn arg<'a>(args: &'a Value, key: &str) -> Result<&'a Value, String> {
    args.get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| format!("missing argument '{key}'"))
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    arg(args, key)?
        .as_str()
        .ok_or_else(|| format!("'{key}' must be text"))
}

fn arg_text(args: &Value, key: &str) -> Result<String, String> {
    let v = arg(args, key)?;
    Ok(match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return Err(format!("'{key}' must be a number or text")),
    })
}

fn value_type(args: &Value) -> Result<ValueType, String> {
    let ty = arg_str(args, "type")?;
    serde_json::from_value(json!(ty)).map_err(|_| {
        format!("unknown type '{ty}' (use i8, u8, i16, u16, i32, u32, i64, u64, f32, f64 or ptr)")
    })
}

fn hex(a: u64) -> String {
    format!("0x{a:X}")
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl Tools {
    pub fn new(cfg: Config) -> Self {
        let log = cfg.log_path.as_ref().and_then(|p| {
            std::fs::create_dir_all(p.parent()?).ok()?;
            File::options().create(true).append(true).open(p).ok()
        });
        Self {
            cfg,
            live: None,
            log,
            control: ScanControl::new(),
        }
    }

    /// No journal, log, packs folder or save backups.
    pub fn for_tests() -> Self {
        Self::new(Config::default())
    }

    /// Calls a tool by name and logs it.
    pub fn call(&mut self, name: &str, args: &Value) -> Res {
        let result = self.dispatch(name, args);
        if let Some(log) = &mut self.log {
            let line = json!({
                "ms": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as u64),
                "tool": name,
                "args": args,
                "ok": result.is_ok(),
                "error": result.as_ref().err(),
            });
            let _ = writeln!(log, "{line}");
        }
        result
    }

    fn live(&mut self) -> Result<&mut Live, String> {
        let live = self
            .live
            .as_mut()
            .ok_or("not attached to a game; call attach first")?;
        if !live.process.is_alive() {
            self.live = None;
            return Err("the game has closed; attach again after restarting it".into());
        }
        Ok(self.live.as_mut().expect("checked above"))
    }

    fn dispatch(&mut self, name: &str, args: &Value) -> Res {
        match name {
            "list_processes" => self.list_processes(args),
            "attach" => self.attach(args),
            "detach" => {
                let problems = self.live.take().map(|mut l| l.revert()).unwrap_or_default();
                Ok(json!({ "detached": true, "problems": problems }))
            }
            "game_info" => self.game_info(),
            "scan_first" => self.scan_first(args),
            "scan_next" => self.scan_next(args),
            "read" => self.read(args),
            "write_test" => self.write_test(args),
            "pointer_scan" => self.pointer_scan(args),
            "pointer_check" => self.pointer_check(args),
            "make_aob" => self.make_aob(args),
            "verify_aob_unique" => self.verify_aob(args),
            "pack_validate" => pack_validate(args),
            "pack_try" => self.pack_try(args),
            "cheat_set" => self.cheat_set(args),
            "revert_all" => {
                let problems = self.live()?.revert();
                Ok(json!({ "reverted": true, "problems": problems }))
            }
            "pack_write" => self.pack_write(args),
            "save_snapshot" => self.save_snapshot(args),
            "save_diff" => self.save_diff(args),
            "save_read" => self.save_read(args),
            _ => Err(format!("unknown tool {name}")),
        }
    }

    fn list_processes(&mut self, args: &Value) -> Res {
        let q = args
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        let list: Vec<Value> = ot_core::list_processes()
            .map_err(err)?
            .into_iter()
            .filter(|p| {
                p.exe_path
                    .as_deref()
                    .is_some_and(|x| !x.to_lowercase().starts_with("c:\\windows\\"))
            })
            .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
            .take(100)
            .map(|p| json!({ "pid": p.pid, "name": p.name, "arch": p.arch, "exe": p.exe_path }))
            .collect();
        Ok(json!({ "processes": list }))
    }

    fn attach(&mut self, args: &Value) -> Res {
        let pid = arg(args, "pid")?
            .as_u64()
            .and_then(|p| u32::try_from(p).ok())
            .ok_or("'pid' must be a process id")?;
        if let Some(mut old) = self.live.take() {
            old.revert();
        }
        let process = AttachedProcess::attach(pid).map_err(|e| match e {
            ot_core::Error::Blocked(v) => format!(
                "the anti-cheat guard blocked this game, so OpenTrainer will not touch it: {v}. \
                 Stop here and tell the player."
            ),
            other => other.to_string(),
        })?;
        if let Some(dir) = &self.cfg.journal_dir
            && let Err(e) = process.enable_journal(dir)
        {
            tracing::warn!("no crash-safe journal: {e}");
        }
        let backup = self.back_up_saves(process.info().exe_path.as_deref());
        self.live = Some(Live {
            process: Arc::new(process),
            scan: None,
            writes: BTreeMap::new(),
            trainer: None,
            pointers: Vec::new(),
        });
        let mut info = self.game_info()?;
        info["saveBackup"] = json!(backup);
        Ok(info)
    }

    /// Backs up the game's saves before any cheat (PLAN.md §5.5).
    fn back_up_saves(&self, exe: Option<&str>) -> String {
        let (Some(root), Some(exe)) = (&self.cfg.saves_root, exe) else {
            return "not available".into();
        };
        let run = || -> Result<String, String> {
            let mut lib = ot_saves::Library::open(root).map_err(err)?;
            let game = lib
                .add(Path::new(exe), &ot_saves::Roots::current())
                .map_err(err)?;
            if game.folders.is_empty() {
                return Ok("no save folder found".into());
            }
            let store = lib.store(&game.id).map_err(err)?;
            let c = store
                .create(
                    &game.folder_paths(),
                    ot_saves::Reason::Launch,
                    "before AI session",
                    Default::default(),
                )
                .map_err(err)?;
            Ok(format!("saves backed up ({} files)", c.snapshot.files))
        };
        run().unwrap_or_else(|e| format!("save backup failed: {e}"))
    }

    fn game_info(&mut self) -> Res {
        let live = self.live()?;
        let p = &live.process;
        let main = p.main_module().map_err(err)?;
        Ok(json!({
            "pid": p.pid(),
            "name": p.info().name,
            "exe": p.info().exe_path,
            "arch": p.arch(),
            "sha256": p.exe_sha256(),
            "mainModule": { "name": main.name, "base": hex(main.base), "size": main.size },
            "modules": p.modules().len(),
        }))
    }

    fn hits(live: &Live) -> Vec<Value> {
        let Some(scan) = &live.scan else {
            return Vec::new();
        };
        let ty = scan.value_type();
        scan.hits(0, MAX_HITS_SHOWN)
            .into_iter()
            .map(|h| {
                let now = live
                    .process
                    .read_value(h.address, ty)
                    .ok()
                    .map(|v| v.to_string());
                json!({ "address": hex(h.address), "value": h.value.to_string(), "now": now })
            })
            .collect()
    }

    fn scan_first(&mut self, args: &Value) -> Res {
        let ty = value_type(args)?;
        let mode = arg_str(args, "mode")?.to_string();
        self.control.reset();
        let live = self.live()?;
        let p = Arc::clone(&live.process);
        let parse = |k: &str| -> Result<ot_core::Value, String> {
            ty.parse(&arg_text(args, k)?, p.arch()).map_err(err)
        };
        let first = match mode.as_str() {
            "exact" => FirstScan::Exact(parse("value")?),
            "between" => FirstScan::Between(parse("min")?, parse("max")?),
            "unknown" => FirstScan::Unknown,
            other => {
                return Err(format!(
                    "first scan mode '{other}' must be exact, between or unknown"
                ));
            }
        };
        let scan =
            Scan::first(&p, ty, first, ScanOptions::default(), &self.control).map_err(err)?;
        let live = self.live()?;
        live.scan = Some(scan);
        let count = live.scan.as_ref().map_or(0, Scan::count);
        Ok(json!({ "count": count, "hits": Self::hits(live) }))
    }

    fn scan_next(&mut self, args: &Value) -> Res {
        let mode = arg_str(args, "mode")?.to_string();
        self.control.reset();
        let live = self.live()?;
        let p = Arc::clone(&live.process);
        let mut scan = live.scan.take().ok_or("run scan_first first")?;
        let ty = scan.value_type();
        let parse = |k: &str| -> Result<ot_core::Value, String> {
            ty.parse(&arg_text(args, k)?, p.arch()).map_err(err)
        };
        let next = match mode.as_str() {
            "exact" => parse("value").map(NextScan::Exact),
            "between" => parse("min").and_then(|a| Ok(NextScan::Between(a, parse("max")?))),
            "changed" => Ok(NextScan::Changed),
            "unchanged" => Ok(NextScan::Unchanged),
            "increased" => Ok(NextScan::Increased),
            "decreased" => Ok(NextScan::Decreased),
            other => Err(format!(
                "next scan mode '{other}' must be exact, between, changed, unchanged, increased or decreased"
            )),
        };
        let result = next.and_then(|m| scan.next(&p, m, &self.control).map_err(err));
        let live = self.live()?;
        live.scan = Some(scan);
        result?;
        let count = live.scan.as_ref().map_or(0, Scan::count);
        Ok(json!({ "count": count, "hits": Self::hits(live) }))
    }

    fn read(&mut self, args: &Value) -> Res {
        let ty = value_type(args)?;
        let live = self.live()?;
        let a = live
            .process
            .resolve(arg_str(args, "address")?)
            .map_err(err)?;
        let v = live.process.read_value(a, ty).map_err(err)?;
        Ok(json!({ "address": hex(a), "value": v.to_string() }))
    }

    fn write_test(&mut self, args: &Value) -> Res {
        let ty = value_type(args)?;
        let text = arg_text(args, "value")?;
        let live = self.live()?;
        let p = Arc::clone(&live.process);
        let a = p.resolve(arg_str(args, "address")?).map_err(err)?;
        let v = ty.parse(&text, p.arch()).map_err(err)?;
        let size = ty.size(p.arch());
        if let std::collections::btree_map::Entry::Vacant(slot) = live.writes.entry(a) {
            slot.insert(p.read_bytes(a, size).map_err(err)?);
        }
        p.write_value(a, &v).map_err(err)?;
        let now = p.read_value(a, ty).map_err(err)?;
        Ok(
            json!({ "address": hex(a), "value": now.to_string(), "undo": "revert_all puts the old bytes back" }),
        )
    }

    fn pointer_scan(&mut self, args: &Value) -> Res {
        let mut options = PointerScanOptions::default();
        if let Some(d) = args.get("max_depth").and_then(Value::as_u64) {
            options.max_depth = u8::try_from(d).map_err(|_| "max_depth is 1-7")?;
        }
        if let Some(o) = args.get("max_offset").and_then(Value::as_u64) {
            options.max_offset = u32::try_from(o).map_err(|_| "max_offset too large")?;
        }
        self.control.reset();
        let live = self.live()?;
        let p = Arc::clone(&live.process);
        let target = p.resolve(arg_str(args, "address")?).map_err(err)?;
        let paths = ot_core::pointer::scan(&p, target, options, &self.control).map_err(err)?;
        let live = self.live()?;
        live.pointers = paths;
        Ok(json!({
            "count": live.pointers.len(),
            "paths": live.pointers.iter().take(MAX_PATHS_SHOWN).map(ToString::to_string).collect::<Vec<_>>(),
            "next": "restart the game, find the value again, attach, then pointer_check with its new address",
        }))
    }

    fn pointer_check(&mut self, args: &Value) -> Res {
        let live = self.live()?;
        let target = live
            .process
            .resolve(arg_str(args, "address")?)
            .map_err(err)?;
        if let Some(list) = args.get("paths").and_then(Value::as_array) {
            live.pointers = list
                .iter()
                .filter_map(Value::as_str)
                .filter_map(parse_path)
                .collect();
        }
        if live.pointers.is_empty() {
            return Err("no pointer paths to check; run pointer_scan or pass 'paths'".into());
        }
        let kept = ot_core::pointer::filter(&live.process, &live.pointers, target).map_err(err)?;
        live.pointers = kept;
        Ok(json!({
            "count": live.pointers.len(),
            "paths": live.pointers.iter().take(MAX_PATHS_SHOWN).map(ToString::to_string).collect::<Vec<_>>(),
        }))
    }

    fn make_aob(&mut self, args: &Value) -> Res {
        let live = self.live()?;
        let a = live
            .process
            .resolve(arg_str(args, "address")?)
            .map_err(err)?;
        let sig = ot_core::signature::make(&live.process, a).map_err(err)?;
        serde_json::to_value(sig).map_err(err)
    }

    fn verify_aob(&mut self, args: &Value) -> Res {
        let pattern = Pattern::parse(arg_str(args, "aob")?).map_err(err)?;
        let live = self.live()?;
        let module = match args.get("module").and_then(Value::as_str) {
            Some(m) => live.process.module(m).map_err(err)?,
            None => live.process.main_module().map_err(err)?,
        };
        let hits = live.process.aob_scan(&module, &pattern, 10).map_err(err)?;
        Ok(json!({
            "module": module.name,
            "matches": hits.len(),
            "unique": hits.len() == 1,
            "at": hits.iter().map(|h| format!("{}+{:#X}", module.name, h - module.base)).collect::<Vec<_>>(),
        }))
    }

    fn pack_try(&mut self, args: &Value) -> Res {
        let pack = parse_pack(arg(args, "pack")?)?;
        let live = self.live()?;
        if let Some(mut old) = live.trainer.take() {
            let _ = old.restore_all();
        }
        let mut trainer = Trainer::load(Arc::clone(&live.process), pack).map_err(err)?;
        let states = trainer.states().map_err(err)?;
        live.trainer = Some(trainer);
        Ok(json!({ "loaded": true, "cheats": states }))
    }

    fn cheat_set(&mut self, args: &Value) -> Res {
        let id = arg_str(args, "id")?.to_string();
        let live = self.live()?;
        let Live {
            trainer,
            writes,
            process,
            ..
        } = live;
        let trainer = trainer.as_mut().ok_or("load a pack with pack_try first")?;
        // A value cheat changes the game's value: keep the original so
        // revert_all puts it back, like a test write.
        let ty = trainer
            .pack()
            .cheats
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| match &c.implementation {
                ot_packs::Impl::Value(v) => Some(v.ty),
                ot_packs::Impl::Patch(_) => None,
            });
        let address = trainer
            .states()
            .map_err(err)?
            .into_iter()
            .find(|s| s.id == id)
            .and_then(|s| s.address);
        if let (Some(ty), Some(a)) = (ty, address)
            && let std::collections::btree_map::Entry::Vacant(slot) = writes.entry(a)
            && let Ok(bytes) = process.read_bytes(a, ty.size(process.arch()))
        {
            slot.insert(bytes);
        }
        if let Some(on) = args.get("on").and_then(Value::as_bool) {
            trainer.set_active(&id, on).map_err(err)?;
        } else if args.get("value").is_some_and(|v| !v.is_null()) {
            trainer
                .set_value(&id, &arg_text(args, "value")?)
                .map_err(err)?;
        } else if args.get("press").and_then(Value::as_bool) == Some(true) {
            trainer.press(&id).map_err(err)?;
        } else {
            return Err("give 'on' (true/false), 'value' or 'press': true".into());
        }
        let states = trainer.states().map_err(err)?;
        Ok(json!({ "cheats": states }))
    }

    fn pack_write(&mut self, args: &Value) -> Res {
        let mut raw = arg(args, "pack")?.clone();
        // AI packs are always marked as such and tagged with the build they
        // were made on (PLAN.md §5.3 step 7).
        if let Some(meta) = raw.get_mut("meta").and_then(Value::as_object_mut) {
            meta.insert("author".into(), json!("ai"));
        }
        let sha = self.live.as_ref().and_then(|l| l.process.exe_sha256());
        if let (Some(sha), Some(game)) = (sha, raw.get_mut("game").and_then(Value::as_object_mut)) {
            let builds = game.entry("builds").or_insert(json!([]));
            if let Some(list) = builds.as_array_mut()
                && !list.iter().any(|b| b["sha256"] == json!(sha))
            {
                list.push(
                    json!({ "sha256": sha, "label": "made by the AI helper", "verified": false }),
                );
            }
        }
        let pack = parse_pack(&raw)?;
        let dir = self
            .cfg
            .packs_dir
            .clone()
            .ok_or("no packs folder on this PC")?
            .join(&pack.game.id);
        let path = dir.join("pack.json");
        if let Ok(existing) = std::fs::read_to_string(&path) {
            let is_ai = ot_packs::parse(&existing).is_ok_and(|(p, _)| p.meta.is_ai());
            if !is_ai {
                return Err(format!(
                    "{} already has a pack that wasn't made by the AI; pick another game id",
                    path.display()
                ));
            }
        }
        std::fs::create_dir_all(&dir).map_err(err)?;
        let text = serde_json::to_string_pretty(&pack).map_err(err)? + "\n";
        let tmp = dir.join("pack.json.tmp");
        std::fs::write(&tmp, &text).map_err(err)?;
        std::fs::rename(&tmp, &path).map_err(err)?;
        Ok(json!({ "written": path.display().to_string(), "cheats": pack.cheats.len() }))
    }

    /// The attached game in the save library (added if new).
    fn save_game(&mut self) -> Result<(ot_saves::Game, ot_saves::Store), String> {
        let root = self
            .cfg
            .saves_root
            .clone()
            .ok_or("save backups aren't available")?;
        let exe = self
            .live()?
            .process
            .info()
            .exe_path
            .clone()
            .ok_or("can't get the game's file path")?;
        let mut lib = ot_saves::Library::open(root).map_err(err)?;
        let game = lib
            .add(Path::new(&exe), &ot_saves::Roots::current())
            .map_err(err)?;
        let store = lib.store(&game.id).map_err(err)?;
        Ok((game, store))
    }

    fn save_snapshot(&mut self, args: &Value) -> Res {
        let (game, store) = self.save_game()?;
        let note = args
            .get("note")
            .and_then(Value::as_str)
            .unwrap_or("AI helper");
        let c = store
            .create(
                &game.folder_paths(),
                ot_saves::Reason::Manual,
                note,
                Default::default(),
            )
            .map_err(err)?;
        serde_json::to_value(c.snapshot).map_err(err)
    }

    fn save_diff(&mut self, args: &Value) -> Res {
        let (game, store) = self.save_game()?;
        let from = match args.get("from").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => store
                .list()
                .map_err(err)?
                .first()
                .map(|s| s.id.clone())
                .ok_or("no backups yet; call save_snapshot first")?,
        };
        let d = ot_saves::diff(
            &store,
            &game.folder_paths(),
            ot_saves::Side::Snapshot(&from),
            ot_saves::Side::Current,
        )
        .map_err(err)?;
        serde_json::to_value(d).map_err(err)
    }

    fn save_read(&mut self, args: &Value) -> Res {
        let (game, _) = self.save_game()?;
        match (
            args.get("folder").and_then(Value::as_str),
            args.get("path").and_then(Value::as_str),
        ) {
            (Some(folder), Some(path)) => {
                let mut v = ot_saves::editor::open(&game, folder, path).map_err(err)?;
                v.fields.truncate(300);
                serde_json::to_value(v).map_err(err)
            }
            _ => {
                let files = ot_saves::editor::files(&game).map_err(err)?;
                Ok(json!({ "folders": game.folder_paths(), "files": files }))
            }
        }
    }
}

fn parse_pack(raw: &Value) -> Result<Pack, String> {
    let text = match raw {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    ot_packs::parse(&text).map(|(p, _)| p).map_err(err)
}

fn pack_validate(args: &Value) -> Res {
    let raw = arg(args, "pack")?;
    let text = match raw {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    match ot_packs::parse(&text) {
        Ok((pack, warnings)) => {
            Ok(json!({ "valid": true, "cheats": pack.cheats.len(), "warnings": warnings }))
        }
        Err(ot_packs::LoadError::Invalid(issues)) => {
            Ok(json!({ "valid": false, "issues": issues }))
        }
        Err(e) => Ok(json!({ "valid": false, "issues": [e.to_string()] })),
    }
}

/// `[[game.exe+0x10]+0x18]+0x4` → a pointer path.
fn parse_path(s: &str) -> Option<PointerPath> {
    let s = s.trim();
    let depth = s.chars().take_while(|c| *c == '[').count();
    let mut rest = &s[depth..];
    let close = rest.find(']')?;
    let (module, off) = rest[..close].rsplit_once('+')?;
    let module_offset = u64::from_str_radix(
        off.trim().trim_start_matches("0x").trim_start_matches("0X"),
        16,
    )
    .ok()?;
    rest = &rest[close + 1..];
    let mut offsets = Vec::new();
    for part in rest.split(']') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let o = part.strip_prefix('+')?.trim();
        offsets.push(
            u64::from_str_radix(o.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()?,
        );
    }
    (offsets.len() == depth).then(|| PointerPath {
        module: module.trim().to_string(),
        module_offset,
        offsets,
    })
}

fn schema(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

/// Tool definitions for `tools/list`.
pub fn definitions() -> Vec<Value> {
    let ty = json!({ "type": "string", "enum": ["i8","u8","i16","u16","i32","u32","i64","u64","f32","f64","ptr"] });
    let num = json!({ "type": ["string", "number"] });
    let addr = json!({ "type": "string", "description": "0x1F0A10, game.exe+0x3A00, or a pointer path [[game.exe+0x10]+0x18]+0x4" });
    let pack = json!({ "type": "object", "description": "A cheat pack (schema 1): game, meta, cheats, optional saveCodes. Cheats are 'value' (address + type, freeze/set) or 'patch' (aob, offset, original, replace)." });
    let tool = |name: &str, description: &str, input: Value| json!({ "name": name, "description": description, "inputSchema": input });
    vec![
        tool(
            "list_processes",
            "List running programs (not Windows system ones). Optional 'query' filters by name.",
            schema(json!({ "query": { "type": "string" } }), &[]),
        ),
        tool(
            "attach",
            "Attach to a game by process id. The anti-cheat guard checks it first; if it refuses, stop and tell the player. Backs up the game's saves.",
            schema(json!({ "pid": { "type": "integer" } }), &["pid"]),
        ),
        tool(
            "detach",
            "Detach, undoing every change made in this session.",
            schema(json!({}), &[]),
        ),
        tool(
            "game_info",
            "Exe, architecture, SHA-256 and main module of the attached game.",
            schema(json!({}), &[]),
        ),
        tool(
            "scan_first",
            "Start a memory scan for a value. mode: exact (value), between (min, max) or unknown.",
            schema(
                json!({ "type": ty, "mode": { "type": "string", "enum": ["exact","between","unknown"] }, "value": num, "min": num, "max": num }),
                &["type", "mode"],
            ),
        ),
        tool(
            "scan_next",
            "Narrow the scan after the value changed in game. mode: exact, between, changed, unchanged, increased, decreased.",
            schema(
                json!({ "mode": { "type": "string", "enum": ["exact","between","changed","unchanged","increased","decreased"] }, "value": num, "min": num, "max": num }),
                &["mode"],
            ),
        ),
        tool(
            "read",
            "Read a value.",
            schema(json!({ "address": addr, "type": ty }), &["address", "type"]),
        ),
        tool(
            "write_test",
            "Write a value to test a guess (e.g. set gold to 12345 and ask the player to look). Undone by revert_all.",
            schema(
                json!({ "address": addr, "type": ty, "value": num }),
                &["address", "type", "value"],
            ),
        ),
        tool(
            "pointer_scan",
            "Find pointer paths from a module to an address, so the cheat still works after a restart.",
            schema(
                json!({ "address": addr, "max_depth": { "type": "integer" }, "max_offset": { "type": "integer" } }),
                &["address"],
            ),
        ),
        tool(
            "pointer_check",
            "After restarting the game, keep only the pointer paths that lead to the value's new address.",
            schema(
                json!({ "address": addr, "paths": { "type": "array", "items": { "type": "string" } } }),
                &["address"],
            ),
        ),
        tool(
            "make_aob",
            "Make a unique byte signature (AOB) for the code at an address.",
            schema(json!({ "address": addr }), &["address"]),
        ),
        tool(
            "verify_aob_unique",
            "Check that an AOB matches exactly once in a module (main module by default).",
            schema(
                json!({ "aob": { "type": "string" }, "module": { "type": "string" } }),
                &["aob"],
            ),
        ),
        tool(
            "pack_validate",
            "Validate a cheat pack. Returns issues.",
            schema(json!({ "pack": pack }), &["pack"]),
        ),
        tool(
            "pack_try",
            "Load a cheat pack into the attached game (nothing is turned on yet). Returns each cheat's state.",
            schema(json!({ "pack": pack }), &["pack"]),
        ),
        tool(
            "cheat_set",
            "Turn a loaded cheat on/off ('on'), set its value ('value') or run a button cheat ('press': true).",
            schema(
                json!({ "id": { "type": "string" }, "on": { "type": "boolean" }, "value": num, "press": { "type": "boolean" } }),
                &["id"],
            ),
        ),
        tool(
            "revert_all",
            "Undo every test write and cheat in the attached game.",
            schema(json!({}), &[]),
        ),
        tool(
            "pack_write",
            "Save a finished, tested pack for the player (marked as made by AI, tagged with this game build).",
            schema(json!({ "pack": pack }), &["pack"]),
        ),
        tool(
            "save_snapshot",
            "Back up the game's save files now (e.g. before the player buys something).",
            schema(json!({ "note": { "type": "string" } }), &[]),
        ),
        tool(
            "save_diff",
            "Show what changed in the save files since a backup (the newest one by default).",
            schema(json!({ "from": { "type": "string" } }), &[]),
        ),
        tool(
            "save_read",
            "List the game's save files, or (with folder and path) show the values in one.",
            schema(
                json!({ "folder": { "type": "string" }, "path": { "type": "string" } }),
                &[],
            ),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pointer_paths() {
        let p = parse_path("[[game.exe+0x10]+0x18]+0x4").unwrap();
        assert_eq!(p.module, "game.exe");
        assert_eq!(p.module_offset, 0x10);
        assert_eq!(p.offsets, vec![0x18, 0x4]);
        assert_eq!(p.to_string(), "[[game.exe+0x10]+0x18]+0x4");
        assert!(parse_path("[game.exe+0x10]").is_none());
        assert!(parse_path("nonsense").is_none());
    }

    #[test]
    fn needs_a_game_and_reports_bad_args() {
        let mut t = Tools::for_tests();
        assert!(
            t.call("game_info", &json!({}))
                .unwrap_err()
                .contains("attach")
        );
        assert!(t.call("attach", &json!({})).unwrap_err().contains("pid"));
        assert!(
            t.call("nope", &json!({}))
                .unwrap_err()
                .contains("unknown tool")
        );
        let v = t
            .call("pack_validate", &json!({ "pack": { "schema": 1 } }))
            .unwrap();
        assert_eq!(v["valid"], false);
        // No game attached: nothing to detach, still fine.
        assert_eq!(t.call("detach", &json!({})).unwrap()["detached"], true);
    }
}
