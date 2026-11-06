//! Tauri commands exposed to the web UI. Keep these thin: real work lives in
//! the `ot-*` crates, which also enforce the anti-cheat guard.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use ot_core::freeze::DEFAULT_FREEZE_INTERVAL;
use ot_core::{
    AttachedProcess, FirstScan, Freezer, Module, NextScan, ProcessInfo, Scan, ScanControl,
    ScanOptions, Value, ValueType, Verdict,
};
use serde::{Deserialize, Serialize};
use tauri::State;

/// App-wide state: at most one attached process.
#[derive(Debug, Default)]
pub struct AppState {
    live: Mutex<Option<Live>>,
    scan_control: Arc<ScanControl>,
    scanning: AtomicBool,
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
}

impl Live {
    fn new(process: AttachedProcess) -> Self {
        let process = Arc::new(process);
        let freezer = Freezer::start(Arc::clone(&process), DEFAULT_FREEZE_INTERVAL);
        Self {
            process,
            freezer,
            scan: None,
            list: Vec::new(),
            next_id: 1,
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
}

impl AppState {
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
    *state.live() = Some(Live::new(process));
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

#[tauri::command]
pub async fn address_list(state: State<'_, AppState>) -> CmdResult<Vec<ListEntryView>> {
    with_live(&state, |l| {
        l.list.iter().map(|e| entry_view(l, e)).collect()
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
        let entry = ListEntry {
            id: l.next_id,
            label: label.trim().to_string(),
            address_expr: address.trim().to_string(),
            address: resolved,
            ty,
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
    fn claim_scan_allows_one_at_a_time() {
        let state = AppState::default();
        let slot = claim_scan(&state).unwrap();
        assert_eq!(claim_scan(&state).unwrap_err().kind, "scanRunning");
        drop(slot);
        assert!(claim_scan(&state).is_ok());
    }
}
