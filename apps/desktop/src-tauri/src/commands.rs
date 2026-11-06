//! Tauri commands exposed to the web UI. Keep these thin: real work lives in
//! the `ot-*` crates, which also enforce the anti-cheat guard.

use std::sync::Mutex;

use ot_core::{AttachedProcess, Module, ProcessInfo, ValueType, Verdict};
use serde::Serialize;
use tauri::State;

/// App-wide state: at most one attached process.
#[derive(Debug, Default)]
pub struct AppState {
    session: Mutex<Option<AttachedProcess>>,
}

impl AppState {
    fn session(&self) -> std::sync::MutexGuard<'_, Option<AttachedProcess>> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
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
    CommandError {
        kind: "notAttached",
        message: "not attached to a process".into(),
        verdict: None,
    }
}

type CmdResult<T> = Result<T, CommandError>;

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

fn session_info(s: &AttachedProcess) -> SessionInfo {
    SessionInfo {
        process: s.info().clone(),
        modules: s.modules(),
    }
}

#[tauri::command]
pub async fn attach(pid: u32, state: State<'_, AppState>) -> CmdResult<SessionInfo> {
    // Detach first so a failed attach never leaves the old session half-alive.
    state.session().take();
    let session = AttachedProcess::attach(pid)?;
    let info = session_info(&session);
    *state.session() = Some(session);
    Ok(info)
}

#[tauri::command]
pub async fn detach(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some(s) = state.session().take() {
        s.detach();
    }
    Ok(())
}

#[tauri::command]
pub async fn current_session(state: State<'_, AppState>) -> CmdResult<Option<SessionInfo>> {
    let mut guard = state.session();
    if guard.as_ref().is_some_and(|s| !s.is_alive()) {
        guard.take();
    }
    Ok(guard.as_ref().map(session_info))
}

#[tauri::command]
pub async fn refresh_modules(state: State<'_, AppState>) -> CmdResult<Vec<Module>> {
    let guard = state.session();
    let s = guard.as_ref().ok_or_else(not_attached)?;
    Ok(s.refresh_modules()?)
}

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

fn read_at(s: &AttachedProcess, address: u64, ty: ValueType) -> CmdResult<ReadResult> {
    let value = s.read_value(address, ty)?;
    Ok(ReadResult {
        address,
        value: value.to_string(),
        bytes: hex_bytes(&value.to_bytes(s.arch())),
    })
}

/// Drops the session if the error means it can't be used any more.
fn settle<T>(state: &AppState, result: CmdResult<T>) -> CmdResult<T> {
    if let Err(e) = &result
        && matches!(e.kind, "blocked" | "processExited")
    {
        state.session().take();
    }
    result
}

#[tauri::command]
pub async fn read_value(
    address: String,
    ty: ValueType,
    state: State<'_, AppState>,
) -> CmdResult<ReadResult> {
    let result = {
        let guard = state.session();
        let s = guard.as_ref().ok_or_else(not_attached)?;
        s.resolve(&address)
            .map_err(CommandError::from)
            .and_then(|a| read_at(s, a, ty))
    };
    settle(&state, result)
}

#[tauri::command]
pub async fn write_value(
    address: String,
    ty: ValueType,
    value: String,
    state: State<'_, AppState>,
) -> CmdResult<ReadResult> {
    let result = {
        let guard = state.session();
        let s = guard.as_ref().ok_or_else(not_attached)?;
        (|| {
            let a = s.resolve(&address)?;
            let v = ty.parse(&value, s.arch())?;
            s.write_value(a, &v)?;
            Ok::<_, ot_core::Error>(a)
        })()
        .map_err(CommandError::from)
        .and_then(|a| read_at(s, a, ty))
    };
    settle(&state, result)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BytesResult {
    pub address: u64,
    pub bytes: String,
}

#[tauri::command]
pub async fn read_bytes(
    address: String,
    len: usize,
    state: State<'_, AppState>,
) -> CmdResult<BytesResult> {
    let len = len.clamp(1, 4096);
    let result = {
        let guard = state.session();
        let s = guard.as_ref().ok_or_else(not_attached)?;
        (|| {
            let a = s.resolve(&address)?;
            let bytes = s.read_bytes(a, len)?;
            Ok::<_, ot_core::Error>(BytesResult {
                address: a,
                bytes: hex_bytes(&bytes),
            })
        })()
        .map_err(CommandError::from)
    };
    settle(&state, result)
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
    fn blocked_errors_carry_the_verdict() {
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
    }

    #[test]
    fn hex_bytes_formats_uppercase_pairs() {
        assert_eq!(hex_bytes(&[0x0A, 0xFF, 0x00]), "0A FF 00");
        assert_eq!(hex_bytes(&[]), "");
    }
}
