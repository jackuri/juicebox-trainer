//! Tauri commands exposed to the web UI. Keep these thin: real work lives in
//! the `ot-*` crates, which also enforce the anti-cheat guard.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use ot_core::freeze::DEFAULT_FREEZE_INTERVAL;
use ot_core::journal::Recovery;
use ot_core::pointer::{PointerPath, PointerScanOptions};
use ot_core::signature::Signature;
use ot_core::{
    AttachedProcess, FirstScan, Freezer, Module, NextScan, ProcessInfo, Scan, ScanControl,
    ScanOptions, Value, ValueType, Verdict,
};
use ot_packs::schema::PackArch;
use ot_packs::{BuildMatch, CheatState, Issue, LoadError, Pack, Trainer, store};
use serde::{Deserialize, Serialize};
use tauri::State;
use tracing::warn;

use crate::hotkeys::{HotkeyService, Registration};

/// App-wide state: at most one attached process.
#[derive(Debug, Default)]
pub struct AppState {
    live: Mutex<Option<Live>>,
    scan_control: Arc<ScanControl>,
    scanning: AtomicBool,
    /// Set once at startup; absent in unit tests (packs load without hotkeys).
    pub hotkeys: OnceLock<HotkeyService>,
    /// Crash-safe journal folder for code changes. Set at startup; absent in
    /// unit tests.
    pub journal_dir: OnceLock<PathBuf>,
    /// What startup recovery undid, for the UI to report once.
    pub recovered: Mutex<Vec<Recovery>>,
    /// Pointer scan results. Kept across sessions: after restarting the game
    /// they are checked against the value's new address.
    pointers: Mutex<Option<PointerResults>>,
}

#[derive(Debug, Clone)]
struct PointerResults {
    exe: String,
    target: u64,
    rounds: u32,
    paths: Vec<PointerPath>,
}

/// Everything tied to the attached process. Dropping it stops the freezer
/// and detaches.
#[derive(Debug)]
struct Live {
    process: Arc<AttachedProcess>,
    freezer: Freezer,
    scan: Option<Scan>,
    list: Vec<ListEntry>,
    next_id: u64,
    trainer: Option<LoadedTrainer>,
}

/// A pack applied to the attached game. Dropping it unregisters its hotkeys
/// and restores every patch.
#[derive(Debug)]
struct LoadedTrainer {
    key: String,
    warnings: Vec<Issue>,
    hotkey_errors: BTreeMap<String, String>,
    hotkeys: Option<Registration>,
    trainer: Trainer,
}

impl Live {
    fn new(process: AttachedProcess, journal_dir: Option<&PathBuf>) -> Self {
        if let Some(dir) = journal_dir
            && let Err(e) = process.enable_journal(dir)
        {
            // Still restored on detach and exit; only a crash isn't covered.
            warn!("no crash-safe journal for this session: {e}");
        }
        let process = Arc::new(process);
        let freezer = Freezer::start(Arc::clone(&process), DEFAULT_FREEZE_INTERVAL);
        Self {
            process,
            freezer,
            scan: None,
            list: Vec::new(),
            next_id: 1,
            trainer: None,
        }
    }
}

#[derive(Debug, Clone)]
struct ListEntry {
    id: u64,
    label: String,
    address_expr: String,
    address: u64,
    ty: ValueType,
    /// A pointer path, re-resolved every time the list is read.
    pointer: bool,
}

impl AppState {
    /// Detaches (restoring patches, stopping freezes). Called on app exit.
    pub fn shutdown(&self) {
        self.scan_control.cancel();
        self.live().take();
    }

    fn live(&self) -> MutexGuard<'_, Option<Live>> {
        self.live.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Drops the session if the error means it can't be used any more.
    fn settle<T>(&self, result: CmdResult<T>) -> CmdResult<T> {
        if let Err(e) = &result
            && e.is_fatal()
        {
            self.live().take();
        }
        result
    }
}

/// Error shape the UI receives.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub kind: &'static str,
    pub message: String,
    /// Present when the anti-cheat guard blocked.
    pub verdict: Option<Verdict>,
}

impl CommandError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            verdict: None,
        }
    }

    fn is_fatal(&self) -> bool {
        matches!(self.kind, "blocked" | "processExited")
    }
}

impl From<ot_packs::trainer::Error> for CommandError {
    fn from(e: ot_packs::trainer::Error) -> Self {
        match e {
            ot_packs::trainer::Error::Core(core) => core.into(),
            other => Self::new(other.kind(), other.to_string()),
        }
    }
}

impl From<ot_core::Error> for CommandError {
    fn from(e: ot_core::Error) -> Self {
        let verdict = match &e {
            ot_core::Error::Blocked(v) => Some(v.clone()),
            _ => None,
        };
        Self {
            kind: e.kind(),
            message: e.to_string(),
            verdict,
        }
    }
}

fn not_attached() -> CommandError {
    CommandError::new("notAttached", "not attached to a process")
}

type CmdResult<T> = Result<T, CommandError>;

/// Runs `f` with the live session, then drops the session on fatal errors.
fn with_live<T>(state: &AppState, f: impl FnOnce(&mut Live) -> CmdResult<T>) -> CmdResult<T> {
    let result = {
        let mut guard = state.live();
        let live = guard.as_mut().ok_or_else(not_attached)?;
        // The address list polls through here, so a game that exits is noticed
        // even when no read or write is touching its memory.
        if !live.process.is_alive() {
            Err(ot_core::Error::ProcessExited {
                pid: live.process.pid(),
            }
            .into())
        } else if let Some(fatal) = live.freezer.fatal() {
            Err(fatal.into())
        } else {
            f(live)
        }
    };
    state.settle(result)
}

// ----- app / processes / attach ---------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub core_version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        core_version: ot_core::VERSION,
    }
}

#[tauri::command]
pub async fn list_processes() -> CmdResult<Vec<ProcessInfo>> {
    Ok(ot_core::list_processes()?)
}

#[tauri::command]
pub async fn check_guard(pid: u32) -> Verdict {
    ot_core::guard::check(pid)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub process: ProcessInfo,
    pub modules: Vec<Module>,
}

fn session_info(p: &AttachedProcess) -> SessionInfo {
    SessionInfo {
        process: p.info().clone(),
        modules: p.modules(),
    }
}

#[tauri::command]
pub async fn attach(pid: u32, state: State<'_, AppState>) -> CmdResult<SessionInfo> {
    // Detach first so a failed attach never leaves the old session half-alive.
    state.live().take();
    let process = AttachedProcess::attach(pid)?;
    let info = session_info(&process);
    *state.live() = Some(Live::new(process, state.journal_dir.get()));
    Ok(info)
}

#[tauri::command]
pub async fn detach(state: State<'_, AppState>) -> CmdResult<()> {
    state.scan_control.cancel();
    state.live().take();
    Ok(())
}

#[tauri::command]
pub async fn current_session(state: State<'_, AppState>) -> CmdResult<Option<SessionInfo>> {
    let mut guard = state.live();
    if guard.as_ref().is_some_and(|l| !l.process.is_alive()) {
        guard.take();
    }
    Ok(guard.as_ref().map(|l| session_info(&l.process)))
}

#[tauri::command]
pub async fn refresh_modules(state: State<'_, AppState>) -> CmdResult<Vec<Module>> {
    with_live(&state, |l| Ok(l.process.refresh_modules()?))
}

// ----- single value read/write ------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    /// Resolved absolute address.
    pub address: u64,
    /// Value as text (pointers in hex).
    pub value: String,
    /// Raw little-endian bytes, space-separated hex.
    pub bytes: String,
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_at(p: &AttachedProcess, address: u64, ty: ValueType) -> CmdResult<ReadResult> {
    let value = p.read_value(address, ty)?;
    Ok(ReadResult {
        address,
        value: value.to_string(),
        bytes: hex_bytes(&value.to_bytes(p.arch())),
    })
}

#[tauri::command]
pub async fn read_value(
    address: String,
    ty: ValueType,
    state: State<'_, AppState>,
) -> CmdResult<ReadResult> {
    with_live(&state, |l| {
        let a = l.process.resolve(&address)?;
        read_at(&l.process, a, ty)
    })
}

#[tauri::command]
pub async fn write_value(
    address: String,
    ty: ValueType,
    value: String,
    state: State<'_, AppState>,
) -> CmdResult<ReadResult> {
    with_live(&state, |l| {
        let p = &l.process;
        let a = p.resolve(&address)?;
        let v = ty.parse(&value, p.arch())?;
        p.write_value(a, &v)?;
        read_at(p, a, ty)
    })
}

// ----- scanner ----------------------------------------------------------------

/// Scan mode as sent by the UI. Values are text, parsed for the scan's type.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScanMode {
    Exact { value: String },
    Between { min: String, max: String },
    Unknown,
    Changed,
    Unchanged,
    Increased,
    Decreased,
}

fn parse(ty: ValueType, p: &AttachedProcess, s: &str) -> CmdResult<Value> {
    Ok(ty.parse(s, p.arch())?)
}

fn first_mode(mode: ScanMode, ty: ValueType, p: &AttachedProcess) -> CmdResult<FirstScan> {
    Ok(match mode {
        ScanMode::Exact { value } => FirstScan::Exact(parse(ty, p, &value)?),
        ScanMode::Between { min, max } => {
            FirstScan::Between(parse(ty, p, &min)?, parse(ty, p, &max)?)
        }
        ScanMode::Unknown => FirstScan::Unknown,
        _ => {
            return Err(CommandError::new(
                "invalidScan",
                "changed/unchanged/increased/decreased need a previous scan",
            ));
        }
    })
}

fn next_mode(mode: ScanMode, ty: ValueType, p: &AttachedProcess) -> CmdResult<NextScan> {
    Ok(match mode {
        ScanMode::Exact { value } => NextScan::Exact(parse(ty, p, &value)?),
        ScanMode::Between { min, max } => {
            NextScan::Between(parse(ty, p, &min)?, parse(ty, p, &max)?)
        }
        ScanMode::Changed => NextScan::Changed,
        ScanMode::Unchanged => NextScan::Unchanged,
        ScanMode::Increased => NextScan::Increased,
        ScanMode::Decreased => NextScan::Decreased,
        ScanMode::Unknown => {
            return Err(CommandError::new(
                "invalidScan",
                "unknown value is only a first scan",
            ));
        }
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub count: usize,
    pub ty: ValueType,
    pub rounds: u32,
    pub memory_bytes: usize,
}

fn summary(s: &Scan) -> ScanSummary {
    ScanSummary {
        count: s.count(),
        ty: s.value_type(),
        rounds: s.rounds(),
        memory_bytes: s.memory_bytes(),
    }
}

/// Clears the "scan running" flag when dropped.
#[derive(Debug)]
struct ScanSlot<'a>(&'a AtomicBool);

impl Drop for ScanSlot<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn claim_scan(state: &AppState) -> CmdResult<ScanSlot<'_>> {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err(CommandError::new(
            "scanRunning",
            "a scan is already running",
        ));
    }
    state.scan_control.reset();
    Ok(ScanSlot(&state.scanning))
}

/// Puts a finished scan back if the same process is still attached.
fn store_scan(state: &AppState, process: &Arc<AttachedProcess>, scan: Scan) {
    if let Some(live) = state.live().as_mut()
        && Arc::ptr_eq(&live.process, process)
    {
        live.scan = Some(scan);
    }
}

#[tauri::command]
pub async fn scan_first(
    ty: ValueType,
    mode: ScanMode,
    options: ScanOptions,
    state: State<'_, AppState>,
) -> CmdResult<ScanSummary> {
    let _slot = claim_scan(&state)?;
    let process = with_live(&state, |l| {
        l.scan = None;
        Ok(Arc::clone(&l.process))
    })?;
    let mode = first_mode(mode, ty, &process)?;
    // The scan runs without holding the state lock, so the address list and
    // freezer keep working meanwhile.
    let result = Scan::first(&process, ty, mode, options, &state.scan_control);
    let scan = state.settle(result.map_err(CommandError::from))?;
    let out = summary(&scan);
    store_scan(&state, &process, scan);
    Ok(out)
}

#[tauri::command]
pub async fn scan_next(mode: ScanMode, state: State<'_, AppState>) -> CmdResult<ScanSummary> {
    let _slot = claim_scan(&state)?;
    let (process, mut scan) = with_live(&state, |l| {
        let scan = l
            .scan
            .take()
            .ok_or_else(|| CommandError::new("noScan", "run a first scan first"))?;
        Ok((Arc::clone(&l.process), scan))
    })?;
    let ty = scan.value_type();
    let result = next_mode(mode, ty, &process)
        .and_then(|m| Ok(scan.next(&process, m, &state.scan_control)?));
    let out = summary(&scan);
    // The previous results survive errors and cancels, so always put them back.
    store_scan(&state, &process, scan);
    state.settle(result)?;
    Ok(out)
}

#[tauri::command]
pub fn scan_cancel(state: State<'_, AppState>) {
    state.scan_control.cancel();
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub running: bool,
    pub done: u64,
    pub total: u64,
}

#[tauri::command]
pub fn scan_progress(state: State<'_, AppState>) -> ScanProgress {
    let (done, total) = state.scan_control.progress();
    ScanProgress {
        running: state.scanning.load(Ordering::SeqCst),
        done,
        total,
    }
}

#[tauri::command]
pub async fn scan_reset(state: State<'_, AppState>) -> CmdResult<()> {
    with_live(&state, |l| {
        l.scan = None;
        Ok(())
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanHitView {
    pub address: u64,
    /// Value at the last scan.
    pub previous: String,
    /// Value now, if readable.
    pub current: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResults {
    pub count: usize,
    pub hits: Vec<ScanHitView>,
}

#[tauri::command]
pub async fn scan_results(
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> CmdResult<ScanResults> {
    with_live(&state, |l| {
        let Some(scan) = &l.scan else {
            return Ok(ScanResults {
                count: 0,
                hits: vec![],
            });
        };
        let ty = scan.value_type();
        let mut hits = Vec::new();
        for h in scan.hits(offset, limit.min(500)) {
            let current = match l.process.read_value(h.address, ty) {
                Ok(v) => Some(v.to_string()),
                Err(e @ (ot_core::Error::Blocked(_) | ot_core::Error::ProcessExited { .. })) => {
                    return Err(e.into());
                }
                Err(_) => None,
            };
            hits.push(ScanHitView {
                address: h.address,
                previous: h.value.to_string(),
                current,
            });
        }
        Ok(ScanResults {
            count: scan.count(),
            hits,
        })
    })
}

// ----- address list + freeze -------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEntryView {
    pub id: u64,
    pub label: String,
    pub address: u64,
    pub address_expr: String,
    pub ty: ValueType,
    pub value: Option<String>,
    pub error: Option<String>,
    pub frozen: bool,
}

fn entry_view(l: &Live, e: &ListEntry) -> CmdResult<ListEntryView> {
    let (value, mut error) = match l.process.read_value(e.address, e.ty) {
        Ok(v) => (Some(v.to_string()), None),
        Err(err @ (ot_core::Error::Blocked(_) | ot_core::Error::ProcessExited { .. })) => {
            return Err(err.into());
        }
        Err(err) => (None, Some(err.to_string())),
    };
    if let Some(freeze_error) = l.freezer.entry_error(e.id) {
        error = Some(freeze_error);
    }
    Ok(ListEntryView {
        id: e.id,
        label: e.label.clone(),
        address: e.address,
        address_expr: e.address_expr.clone(),
        ty: e.ty,
        value,
        error,
        frozen: l.freezer.is_frozen(e.id),
    })
}

fn find_entry(l: &Live, id: u64) -> CmdResult<ListEntry> {
    l.list
        .iter()
        .find(|e| e.id == id)
        .cloned()
        .ok_or_else(|| CommandError::new("notFound", format!("no address list entry {id}")))
}

/// Follows pointer-path entries to where they lead now. A frozen entry
/// whose target moved is unfrozen (its old value may not fit the new place).
fn follow_pointers(l: &mut Live) -> CmdResult<Vec<(u64, String)>> {
    let mut notes = Vec::new();
    for i in 0..l.list.len() {
        if !l.list[i].pointer {
            continue;
        }
        match l.process.resolve(&l.list[i].address_expr) {
            Ok(a) if a != l.list[i].address => {
                let id = l.list[i].id;
                if l.freezer.is_frozen(id) {
                    l.freezer.remove(id);
                    notes.push((id, "the pointer moved, so the freeze was stopped".into()));
                }
                l.list[i].address = a;
            }
            Ok(_) => {}
            Err(e @ (ot_core::Error::Blocked(_) | ot_core::Error::ProcessExited { .. })) => {
                return Err(e.into());
            }
            Err(e) => notes.push((l.list[i].id, e.to_string())),
        }
    }
    Ok(notes)
}

#[tauri::command]
pub async fn address_list(state: State<'_, AppState>) -> CmdResult<Vec<ListEntryView>> {
    with_live(&state, |l| {
        let notes = follow_pointers(l)?;
        l.list
            .iter()
            .map(|e| {
                let mut v = entry_view(l, e)?;
                if let Some((_, note)) = notes.iter().find(|(id, _)| *id == e.id) {
                    v.error = Some(note.clone());
                }
                Ok(v)
            })
            .collect()
    })
}

#[tauri::command]
pub async fn address_add(
    address: String,
    ty: ValueType,
    label: String,
    state: State<'_, AppState>,
) -> CmdResult<ListEntryView> {
    with_live(&state, |l| {
        let resolved = l.process.resolve(&address)?;
        let pointer = ot_core::address::parse(&address)?.is_pointer();
        let entry = ListEntry {
            id: l.next_id,
            label: label.trim().to_string(),
            address_expr: address.trim().to_string(),
            address: resolved,
            ty,
            pointer,
        };
        l.next_id += 1;
        l.list.push(entry.clone());
        entry_view(l, &entry)
    })
}

#[tauri::command]
pub async fn address_remove(id: u64, state: State<'_, AppState>) -> CmdResult<()> {
    with_live(&state, |l| {
        l.freezer.remove(id);
        l.list.retain(|e| e.id != id);
        Ok(())
    })
}

#[tauri::command]
pub async fn address_rename(id: u64, label: String, state: State<'_, AppState>) -> CmdResult<()> {
    with_live(&state, |l| {
        let entry =
            l.list.iter_mut().find(|e| e.id == id).ok_or_else(|| {
                CommandError::new("notFound", format!("no address list entry {id}"))
            })?;
        entry.label = label.trim().to_string();
        Ok(())
    })
}

/// Freezes an entry at its current value, or unfreezes it.
#[tauri::command]
pub async fn address_set_frozen(
    id: u64,
    frozen: bool,
    state: State<'_, AppState>,
) -> CmdResult<ListEntryView> {
    with_live(&state, |l| {
        let entry = find_entry(l, id)?;
        if frozen {
            let value = l.process.read_value(entry.address, entry.ty)?;
            l.freezer.set(id, entry.address, &value);
        } else {
            l.freezer.remove(id);
        }
        entry_view(l, &entry)
    })
}

/// Writes a new value; a frozen entry stays frozen at the new value.
#[tauri::command]
pub async fn address_write(
    id: u64,
    value: String,
    state: State<'_, AppState>,
) -> CmdResult<ListEntryView> {
    with_live(&state, |l| {
        let entry = find_entry(l, id)?;
        let v = entry.ty.parse(&value, l.process.arch())?;
        l.process.write_value(entry.address, &v)?;
        if l.freezer.is_frozen(id) {
            l.freezer.set(id, entry.address, &v);
        }
        entry_view(l, &entry)
    })
}

// ----- pointer scanner + signature maker ----------------------------------------

/// One pointer path for the UI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerPathView {
    /// Cheat Engine notation; also accepted by the address list.
    pub expression: String,
    pub module: String,
    pub module_offset: u64,
    pub offsets: Vec<u64>,
    /// Where it leads now, if attached and readable.
    pub address: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointerResultsView {
    /// The game the paths were found in.
    pub exe: String,
    /// Address the last scan or check used.
    pub target: u64,
    /// 1 after a scan, +1 per check after a restart.
    pub rounds: u32,
    pub count: usize,
    pub paths: Vec<PointerPathView>,
}

fn pointer_view(r: &PointerResults, live: Option<&Live>, limit: usize) -> PointerResultsView {
    let paths = r
        .paths
        .iter()
        .take(limit.min(500))
        .map(|p| PointerPathView {
            expression: p.to_string(),
            module: p.module.clone(),
            module_offset: p.module_offset,
            offsets: p.offsets.clone(),
            address: live.and_then(|l| p.resolve(&l.process).ok()),
        })
        .collect();
    PointerResultsView {
        exe: r.exe.clone(),
        target: r.target,
        rounds: r.rounds,
        count: r.paths.len(),
        paths,
    }
}

impl AppState {
    fn pointers(&self) -> MutexGuard<'_, Option<PointerResults>> {
        self.pointers.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Finds pointer paths from a module to `target` (an address expression).
#[tauri::command]
pub async fn pointer_scan(
    target: String,
    options: PointerScanOptions,
    state: State<'_, AppState>,
) -> CmdResult<PointerResultsView> {
    let _slot = claim_scan(&state)?;
    let (process, address) = with_live(&state, |l| {
        let a = l.process.resolve(&target)?;
        Ok((Arc::clone(&l.process), a))
    })?;
    // Runs without the state lock, like the value scanner.
    let result = ot_core::pointer::scan(&process, address, options, &state.scan_control);
    let paths = state.settle(result.map_err(CommandError::from))?;
    let results = PointerResults {
        exe: process.info().name.clone(),
        target: address,
        rounds: 1,
        paths,
    };
    let live = state.live();
    let view = pointer_view(&results, live.as_ref(), 200);
    *state.pointers() = Some(results);
    Ok(view)
}

/// Keeps the paths that lead to `target` now (use after restarting the game).
#[tauri::command]
pub async fn pointer_filter(
    target: String,
    state: State<'_, AppState>,
) -> CmdResult<PointerResultsView> {
    let previous = state
        .pointers()
        .clone()
        .ok_or_else(|| CommandError::new("noScan", "run a pointer scan first"))?;
    with_live(&state, |l| {
        let address = l.process.resolve(&target)?;
        let paths = ot_core::pointer::filter(&l.process, &previous.paths, address)?;
        let results = PointerResults {
            exe: previous.exe.clone(),
            target: address,
            rounds: previous.rounds + 1,
            paths,
        };
        let view = pointer_view(&results, Some(l), 200);
        *state.pointers() = Some(results);
        Ok(view)
    })
}

#[tauri::command]
pub async fn pointer_results(state: State<'_, AppState>) -> CmdResult<Option<PointerResultsView>> {
    let results = state.pointers().clone();
    let live = state.live();
    Ok(results.map(|r| pointer_view(&r, live.as_ref(), 200)))
}

#[tauri::command]
pub async fn pointer_clear(state: State<'_, AppState>) -> CmdResult<()> {
    *state.pointers() = None;
    Ok(())
}

/// Makes a unique byte signature for the code at `address`.
#[tauri::command]
pub async fn signature_make(address: String, state: State<'_, AppState>) -> CmdResult<Signature> {
    with_live(&state, |l| {
        let a = l.process.resolve(&address)?;
        Ok(ot_core::signature::make(&l.process, a)?)
    })
}

// ----- crash recovery -------------------------------------------------------------

/// Code changes that startup recovery undid (left by a crash). Reported once.
#[tauri::command]
pub fn recovery_report(state: State<'_, AppState>) -> Vec<Recovery> {
    std::mem::take(&mut *state.recovered.lock().unwrap_or_else(|e| e.into_inner()))
}

// ----- packs + trainer --------------------------------------------------------

/// A pack the app can see, valid or not.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSummary {
    /// Pass to `trainer_load`.
    pub key: String,
    pub source: store::Source,
    pub path: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub exe: Option<String>,
    pub arch: Option<PackArch>,
    pub author: Option<String>,
    pub ai: bool,
    pub pack_version: Option<String>,
    pub cheats: usize,
    /// Why it can't be loaded, if it can't.
    pub error: Option<String>,
    pub issues: Vec<Issue>,
}

fn summarize(found: store::Found) -> PackSummary {
    let mut s = PackSummary {
        key: found.key,
        source: found.source,
        path: found.path.map(|p| p.display().to_string()),
        id: None,
        name: None,
        exe: None,
        arch: None,
        author: None,
        ai: false,
        pack_version: None,
        cheats: 0,
        error: None,
        issues: Vec::new(),
    };
    let text = match found.text {
        Ok(text) => text,
        Err(e) => {
            s.error = Some(e);
            return s;
        }
    };
    match ot_packs::parse(&text) {
        Ok((pack, warnings)) => {
            s.id = Some(pack.game.id);
            s.name = Some(pack.game.name);
            s.exe = Some(pack.game.exe);
            s.arch = pack.game.arch;
            s.ai = pack.meta.is_ai();
            s.author = Some(pack.meta.author);
            s.pack_version = Some(pack.meta.pack_version);
            s.cheats = pack.cheats.len();
            s.issues = warnings;
        }
        Err(e) => {
            s.error = Some(e.to_string());
            if let LoadError::Invalid(issues) = e {
                s.issues = issues;
            }
        }
    }
    s
}

/// Every pack: the built-in examples, then the user's pack folder.
#[tauri::command]
pub async fn packs_list() -> Vec<PackSummary> {
    store::all().into_iter().map(summarize).collect()
}

/// The user's pack folder (created if missing).
#[tauri::command]
pub async fn packs_folder() -> CmdResult<String> {
    let dir =
        store::user_dir().ok_or_else(|| CommandError::new("noAppData", "%APPDATA% is not set"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| CommandError::new("io", format!("{}: {e}", dir.display())))?;
    Ok(dir.display().to_string())
}

#[tauri::command]
pub async fn open_packs_folder() -> CmdResult<()> {
    let dir = packs_folder().await?;
    std::process::Command::new("explorer.exe")
        .arg(&dir)
        .spawn()
        .map_err(|e| CommandError::new("io", format!("could not open {dir}: {e}")))?;
    Ok(())
}

/// Everything the trainer UI shows.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainerView {
    pub key: String,
    pub pack: Pack,
    pub build: BuildMatch,
    pub exe_sha256: Option<String>,
    pub warnings: Vec<Issue>,
    pub cheats: Vec<CheatState>,
    /// Cheat id → why its hotkey could not be registered.
    pub hotkey_errors: BTreeMap<String, String>,
}

fn trainer_view(t: &mut LoadedTrainer) -> CmdResult<TrainerView> {
    t.trainer.refresh()?;
    let cheats = t.trainer.states()?;
    Ok(TrainerView {
        key: t.key.clone(),
        pack: t.trainer.pack().clone(),
        build: t.trainer.build().clone(),
        exe_sha256: t.trainer.exe_sha256().map(str::to_owned),
        warnings: t.warnings.clone(),
        cheats,
        hotkey_errors: t.hotkey_errors.clone(),
    })
}

fn no_trainer() -> CommandError {
    CommandError::new("noTrainer", "no pack is loaded")
}

/// Runs `f` on the loaded trainer and returns the fresh view. A cheat-level
/// error is returned as the error (the cheat also shows it on its row).
fn with_trainer(
    state: &AppState,
    f: impl FnOnce(&mut Trainer) -> Result<(), ot_packs::trainer::Error>,
) -> CmdResult<TrainerView> {
    with_live(state, |l| {
        let t = l.trainer.as_mut().ok_or_else(no_trainer)?;
        f(&mut t.trainer)?;
        trainer_view(t)
    })
}

#[tauri::command]
pub async fn trainer_load(key: String, state: State<'_, AppState>) -> CmdResult<TrainerView> {
    let found = store::all()
        .into_iter()
        .find(|f| f.key == key)
        .ok_or_else(|| CommandError::new("notFound", format!("no pack '{key}'")))?;
    let text = found.text.map_err(|e| CommandError::new("io", e))?;
    let (pack, warnings) =
        ot_packs::parse(&text).map_err(|e| CommandError::new("invalidPack", e.to_string()))?;
    with_live(&state, |l| {
        // Unload the old pack first so its patches are restored.
        l.trainer = None;
        let trainer = Trainer::load(Arc::clone(&l.process), pack)?;
        let (hotkeys, hotkey_errors) = match state.hotkeys.get() {
            Some(service) => {
                let (reg, results) = service.register(trainer.hotkeys());
                let errors = results
                    .into_iter()
                    .filter_map(|(id, e)| e.map(|e| (id, e)))
                    .collect();
                (Some(reg), errors)
            }
            None => (None, BTreeMap::new()),
        };
        let loaded = l.trainer.insert(LoadedTrainer {
            key: key.clone(),
            warnings,
            hotkey_errors,
            hotkeys,
            trainer,
        });
        trainer_view(loaded)
    })
}

#[tauri::command]
pub async fn trainer_state(state: State<'_, AppState>) -> CmdResult<Option<TrainerView>> {
    with_live(&state, |l| match l.trainer.as_mut() {
        Some(t) => trainer_view(t).map(Some),
        None => Ok(None),
    })
}

#[tauri::command]
pub async fn trainer_unload(state: State<'_, AppState>) -> CmdResult<()> {
    with_live(&state, |l| {
        l.trainer = None;
        Ok(())
    })
}

#[tauri::command]
pub async fn trainer_toggle(
    id: String,
    on: bool,
    state: State<'_, AppState>,
) -> CmdResult<TrainerView> {
    with_trainer(&state, |t| t.set_active(&id, on))
}

#[tauri::command]
pub async fn trainer_set(
    id: String,
    value: String,
    state: State<'_, AppState>,
) -> CmdResult<TrainerView> {
    with_trainer(&state, |t| t.set_value(&id, &value))
}

#[tauri::command]
pub async fn trainer_release(id: String, state: State<'_, AppState>) -> CmdResult<TrainerView> {
    with_trainer(&state, |t| t.release(&id))
}

#[tauri::command]
pub async fn trainer_press(id: String, state: State<'_, AppState>) -> CmdResult<TrainerView> {
    with_trainer(&state, |t| t.press(&id))
}

impl AppState {
    /// A registered hotkey was pressed. Ignored unless `generation` is the
    /// loaded trainer's registration.
    pub fn on_hotkey(&self, generation: u64, cheat: &str) {
        let result = with_live(self, |l| {
            let Some(t) = l.trainer.as_mut() else {
                return Ok(());
            };
            if t.hotkeys.as_ref().map(Registration::id) != Some(generation) {
                return Ok(());
            }
            t.trainer.hotkey(cheat).map_err(CommandError::from)
        });
        if let Err(e) = result {
            warn!(%cheat, "hotkey action failed: {}", e.message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_matches_workspace_version() {
        let info = app_info();
        assert_eq!(info.version, info.core_version);
    }

    #[test]
    fn blocked_errors_carry_the_verdict_and_are_fatal() {
        let verdict = Verdict {
            pid: 1,
            target: "game.exe".into(),
            allowed: false,
            findings: vec![],
            signatures_version: 1,
        };
        let e = CommandError::from(ot_core::Error::Blocked(verdict.clone()));
        assert_eq!(e.kind, "blocked");
        assert_eq!(e.verdict, Some(verdict));
        assert!(e.is_fatal());
        assert!(!CommandError::from(ot_core::Error::Cancelled).is_fatal());
    }

    #[test]
    fn hex_bytes_formats_uppercase_pairs() {
        assert_eq!(hex_bytes(&[0x0A, 0xFF, 0x00]), "0A FF 00");
        assert_eq!(hex_bytes(&[]), "");
    }

    #[test]
    fn scan_modes_deserialize_from_ui_json() {
        let m: ScanMode = serde_json::from_str(r#"{"kind":"exact","value":"250"}"#).unwrap();
        assert!(matches!(m, ScanMode::Exact { value } if value == "250"));
        let m: ScanMode =
            serde_json::from_str(r#"{"kind":"between","min":"1","max":"2"}"#).unwrap();
        assert!(matches!(m, ScanMode::Between { .. }));
        let m: ScanMode = serde_json::from_str(r#"{"kind":"decreased"}"#).unwrap();
        assert!(matches!(m, ScanMode::Decreased));
        let o: ScanOptions =
            serde_json::from_str(r#"{"writableOnly":true,"aligned":false,"floatTolerance":null}"#)
                .unwrap();
        assert!(!o.aligned);
    }

    #[test]
    fn bundled_packs_summarize_as_valid() {
        let list: Vec<_> = store::bundled().into_iter().map(summarize).collect();
        assert!(list.len() >= 2);
        for p in &list {
            assert!(p.error.is_none(), "{}: {:?}", p.key, p.error);
            assert_eq!(p.exe.as_deref(), Some("testbed_native.exe"));
            assert!(p.cheats > 0);
        }
        let json = serde_json::to_value(&list[0]).unwrap();
        assert_eq!(json["source"], "bundled");
        assert!(json.get("packVersion").is_some());
    }

    #[test]
    fn broken_pack_summarizes_with_error() {
        let s = summarize(store::Found {
            source: store::Source::User,
            key: "x".into(),
            path: None,
            text: Ok(r#"{"schema": 2}"#.into()),
        });
        assert!(s.error.unwrap().contains("schema 2"));
        assert_eq!(s.issues.len(), 1);
    }

    #[test]
    fn trainer_errors_keep_core_kinds() {
        let e = CommandError::from(ot_packs::trainer::Error::Core(
            ot_core::Error::ProcessExited { pid: 3 },
        ));
        assert!(e.is_fatal());
        let e = CommandError::from(ot_packs::trainer::Error::UnknownCheat("x".into()));
        assert_eq!(e.kind, "unknownCheat");
    }

    #[test]
    fn trainer_commands_need_a_session() {
        let state = AppState::default();
        assert_eq!(
            with_trainer(&state, |_| Ok(())).unwrap_err().kind,
            "notAttached"
        );
        state.on_hotkey(1, "x"); // no session: ignored, no panic
    }

    #[test]
    fn pointer_results_render_for_the_ui() {
        let r = PointerResults {
            exe: "game.exe".into(),
            target: 0x1234,
            rounds: 2,
            paths: vec![PointerPath {
                module: "game.exe".into(),
                module_offset: 0x10,
                offsets: vec![0x18, 0x4],
            }],
        };
        let v = pointer_view(&r, None, 200);
        assert_eq!(v.count, 1);
        assert_eq!(v.paths[0].expression, "[[game.exe+0x10]+0x18]+0x4");
        assert_eq!(v.paths[0].address, None);
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["paths"][0]["moduleOffset"], 0x10);
        let o: PointerScanOptions =
            serde_json::from_str(r#"{"maxDepth":3,"maxOffset":4096,"maxResults":500}"#).unwrap();
        assert_eq!(o.max_depth, 3);
    }

    #[test]
    fn new_state_has_no_journal_or_results() {
        let state = AppState::default();
        assert!(state.journal_dir.get().is_none());
        assert!(state.pointers().is_none());
    }

    #[test]
    fn claim_scan_allows_one_at_a_time() {
        let state = AppState::default();
        let slot = claim_scan(&state).unwrap();
        assert_eq!(claim_scan(&state).unwrap_err().kind, "scanRunning");
        drop(slot);
        assert!(claim_scan(&state).is_ok());
    }
}
