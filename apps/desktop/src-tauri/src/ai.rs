//! The "Make a trainer" AI helper (PLAN.md §5). Starts the Node sidecar
//! (`sidecars/ai-agent`), which runs the Claude Agent SDK with this app as its
//! only tool server (`opentrainer-desktop --mcp`, the guarded ot-mcp tools),
//! and relays its messages to the UI as `ai-event`s.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};
use tracing::{info, warn};

use crate::commands::{AppState, CommandError};

type CmdResult<T> = Result<T, CommandError>;

/// No console window for the sidecar.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug)]
struct Running {
    child: Child,
    stdin: ChildStdin,
}

/// The helper process (at most one) and what it has said so far.
#[derive(Debug, Default)]
pub struct AiState {
    running: Arc<Mutex<Option<Running>>>,
    transcript: Arc<Mutex<Vec<Value>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AiState {
    fn send(&self, msg: &Value) -> CmdResult<()> {
        let mut guard = lock(&self.running);
        let r = guard
            .as_mut()
            .ok_or_else(|| CommandError::new("aiNotRunning", "the AI helper isn't running"))?;
        writeln!(r.stdin, "{msg}")
            .and_then(|()| r.stdin.flush())
            .map_err(|e| CommandError::new("io", format!("the AI helper stopped: {e}")))
    }

    /// Stops the helper (on app exit).
    pub fn shutdown(&self) {
        if let Some(mut r) = lock(&self.running).take() {
            let _ = writeln!(r.stdin, "{}", json!({ "type": "cancel" }));
            let _ = r.stdin.flush();
            std::thread::sleep(Duration::from_millis(500));
            let _ = r.child.kill();
        }
    }
}

/// The sidecar script: `OT_AI_AGENT`, next to the exe, or the repo's build.
fn sidecar_script() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OT_AI_AGENT") {
        return Some(PathBuf::from(p));
    }
    let beside = std::env::current_exe()
        .ok()?
        .parent()?
        .join("ai-agent")
        .join("index.js");
    if beside.is_file() {
        return Some(beside);
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("sidecars")
        .join("ai-agent")
        .join("dist")
        .join("index.js");
    repo.is_file().then_some(repo)
}

/// The Claude engine the Agent SDK runs (`claude.exe` from its Windows
/// package), found in a `node_modules` folder above the sidecar script.
fn engine_path() -> Option<PathBuf> {
    let script = sidecar_script()?;
    script.ancestors().skip(1).find_map(|dir| {
        let exe = dir
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-agent-sdk-win32-x64")
            .join("claude.exe");
        exe.is_file().then_some(exe)
    })
}

/// Claude settings inherited from another Claude program (e.g. when
/// OpenTrainer was started from one) that would confuse the engine.
fn is_inherited_claude_var(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    k == "CLAUDECODE"
        || k.starts_with("CLAUDE_CODE_")
        || k.starts_with("CLAUDE_AGENT_SDK_")
        || k == "ANTHROPIC_BASE_URL"
}

fn engine_command(engine: &std::path::Path) -> Command {
    let mut c = Command::new(engine);
    for (k, _) in std::env::vars_os() {
        if k.to_str().is_some_and(is_inherited_claude_var) {
            c.env_remove(k);
        }
    }
    c
}

/// Is the engine signed in to a Claude account? `None` if it can't tell.
fn signed_in() -> Option<bool> {
    use std::os::windows::process::CommandExt;
    let engine = engine_path()?;
    let out = engine_command(&engine)
        .args(["auth", "status", "--json"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let v: Value = serde_json::from_slice(&out.stdout).ok()?;
    v.get("loggedIn").and_then(Value::as_bool)
}

fn node_works() -> bool {
    use std::os::windows::process::CommandExt;
    Command::new("node")
        .arg("--version")
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub available: bool,
    /// Why it isn't available, in plain words.
    pub problem: Option<String>,
    pub running: bool,
    pub transcript: Vec<Value>,
    /// Signed in to a Claude account (for the subscription option).
    pub signed_in: Option<bool>,
}

fn problem() -> Option<String> {
    if sidecar_script().is_none() {
        return Some(
            "The AI helper isn't built yet. Run `npm run build:agent` (Try OpenTrainer.cmd does this).".into(),
        );
    }
    if !node_works() {
        return Some("The AI helper needs Node.js. Install it from https://nodejs.org.".into());
    }
    None
}

#[tauri::command]
pub async fn ai_status(state: State<'_, AiState>) -> CmdResult<AiStatus> {
    let problem = problem();
    let signed_in = tauri::async_runtime::spawn_blocking(signed_in)
        .await
        .ok()
        .flatten();
    let running = lock(&state.running).is_some();
    let transcript = lock(&state.transcript).clone();
    Ok(AiStatus {
        available: problem.is_none(),
        problem,
        running,
        transcript,
        signed_in,
    })
}

/// Opens a window where the player signs in to Claude (in their browser).
#[tauri::command]
pub async fn ai_sign_in() -> CmdResult<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    let engine = engine_path()
        .ok_or_else(|| CommandError::new("aiUnavailable", "the AI helper isn't installed"))?;
    engine_command(&engine)
        .args(["auth", "login"])
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map_err(|e| CommandError::new("io", format!("could not open the sign-in window: {e}")))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStartRequest {
    /// The running game (trainer task).
    #[serde(default)]
    pub pid: u32,
    /// `trainer` (default) or `explainSave`.
    #[serde(default)]
    pub task: Option<String>,
    /// For `explainSave`: the game's exe and the backup to compare with.
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub backup: Option<String>,
    pub wishes: Vec<String>,
    pub notes: String,
    /// `subscription` (Claude login), `apiKey` (ANTHROPIC_API_KEY) or
    /// `local` (Ollama / LM Studio).
    pub backend: String,
    pub max_budget_usd: f64,
    /// For `local`: the server address and the model's name there.
    #[serde(default)]
    pub local_url: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Optional static-analysis tool server (e.g. REA): its command and
    /// arguments, chosen by the player.
    #[serde(default)]
    pub analysis_command: Option<String>,
    #[serde(default)]
    pub analysis_args: Vec<String>,
}

fn push(transcript: &Mutex<Vec<Value>>, app: &AppHandle, event: Value) {
    lock(transcript).push(event.clone());
    let _ = app.emit("ai-event", event);
}

#[tauri::command]
pub async fn ai_start(
    request: AiStartRequest,
    app: AppHandle,
    state: State<'_, AiState>,
    session: State<'_, AppState>,
) -> CmdResult<()> {
    use std::os::windows::process::CommandExt;
    if lock(&state.running).is_some() {
        return Err(CommandError::new(
            "aiRunning",
            "the AI helper is already running",
        ));
    }
    if let Some(p) = problem() {
        return Err(CommandError::new("aiUnavailable", p));
    }
    let explain = request.task.as_deref() == Some("explainSave");
    let game = if explain {
        // Only save files are read; no game is attached.
        let exe = request
            .exe
            .clone()
            .ok_or_else(|| CommandError::new("badRequest", "which game's saves?"))?;
        let name = std::path::Path::new(&exe)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        json!({ "pid": 0, "name": name, "exe": exe })
    } else {
        let process = ot_core::list_processes()?
            .into_iter()
            .find(|p| p.pid == request.pid)
            .ok_or_else(|| CommandError::new("notFound", "that game isn't running any more"))?;
        // Refuse up front (and for free) if the guard would.
        let verdict = ot_core::guard::check(request.pid);
        if !verdict.allowed {
            let mut e = CommandError::new("blocked", verdict.to_string());
            e.verdict = Some(verdict);
            return Err(e);
        }
        // The helper attaches on its own; the app lets go of the game first.
        session.shutdown();
        json!({ "pid": process.pid, "name": process.name, "exe": process.exe_path })
    };

    let script =
        sidecar_script().ok_or_else(|| CommandError::new("aiUnavailable", "no sidecar"))?;
    let exe = std::env::current_exe()
        .map_err(|e| CommandError::new("io", format!("can't find OpenTrainer's exe: {e}")))?;
    let mut command = Command::new("node");
    for (k, _) in std::env::vars_os() {
        if k.to_str().is_some_and(is_inherited_claude_var) {
            command.env_remove(k);
        }
    }
    let mut child = command
        .arg(&script)
        .env("OT_MCP_COMMAND", &exe)
        .env("OT_MCP_ARGS", r#"["--mcp"]"#)
        .env("OT_APP_VERSION", env!("CARGO_PKG_VERSION"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| CommandError::new("io", format!("could not start the AI helper: {e}")))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| CommandError::new("io", "no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CommandError::new("io", "no stdout"))?;
    let stderr = child.stderr.take();

    lock(&state.transcript).clear();
    *lock(&state.running) = Some(Running { child, stdin });

    let running = Arc::clone(&state.running);
    let transcript = Arc::clone(&state.transcript);
    std::thread::Builder::new()
        .name("ai-helper".into())
        .spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str::<Value>(&line) {
                    Ok(event) => push(&transcript, &app, event),
                    Err(_) => info!("ai helper: {line}"),
                }
            }
            if let Some(mut r) = lock(&running).take() {
                let _ = r.child.wait();
            }
            push(&transcript, &app, json!({ "type": "exit" }));
        })
        .map_err(|e| CommandError::new("io", e.to_string()))?;
    if let Some(stderr) = stderr {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                warn!("ai helper: {line}");
            }
        });
    }

    let start = json!({
        "type": "start",
        "game": game,
        "task": if explain { "explainSave" } else { "trainer" },
        "backup": request.backup,
        "wishes": request.wishes,
        "notes": request.notes,
        "backend": match request.backend.as_str() {
            "apiKey" => "apiKey",
            "local" => "local",
            _ => "subscription",
        },
        "localUrl": request.local_url,
        "model": request.model.filter(|m| !m.trim().is_empty()),
        "analysis": request
            .analysis_command
            .filter(|c| !c.trim().is_empty())
            .map(|command| json!({ "command": command, "args": request.analysis_args })),
        "maxBudgetUsd": request.max_budget_usd.clamp(0.5, 50.0),
    });
    state.send(&start)
}

#[tauri::command]
pub async fn ai_answer(id: String, text: String, state: State<'_, AiState>) -> CmdResult<()> {
    let event = json!({ "type": "answer", "id": id, "text": text });
    state.send(&event)?;
    lock(&state.transcript).push(json!({ "type": "answered", "id": id, "text": text }));
    Ok(())
}

#[tauri::command]
pub async fn ai_cancel(state: State<'_, AiState>) -> CmdResult<()> {
    state.send(&json!({ "type": "cancel" }))?;
    // If it doesn't stop by itself, stop it.
    let running = Arc::clone(&state.running);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(15));
        if let Some(r) = lock(&running).as_mut() {
            let _ = r.child.kill();
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_running_until_started() {
        let s = AiState::default();
        assert_eq!(s.send(&json!({})).unwrap_err().kind, "aiNotRunning");
        s.shutdown();
    }

    #[test]
    fn explain_requests_need_no_pid() {
        let r: AiStartRequest = serde_json::from_str(
            r#"{"task":"explainSave","exe":"C:\\g\\g.exe","backup":"17","wishes":[],"notes":"","backend":"subscription","maxBudgetUsd":1}"#,
        )
        .unwrap();
        assert_eq!(r.pid, 0);
        assert_eq!(r.task.as_deref(), Some("explainSave"));
        assert_eq!(r.backup.as_deref(), Some("17"));
    }

    #[test]
    fn start_requests_deserialize_from_ui_json() {
        let r: AiStartRequest = serde_json::from_str(
            r#"{"pid":5,"wishes":["Gold"],"notes":"","backend":"subscription","maxBudgetUsd":5}"#,
        )
        .unwrap();
        assert_eq!(r.pid, 5);
        assert_eq!(r.wishes, vec!["Gold".to_string()]);
    }

    #[test]
    fn strips_settings_inherited_from_other_claude_programs() {
        assert!(is_inherited_claude_var("CLAUDECODE"));
        assert!(is_inherited_claude_var("CLAUDE_CODE_ENTRYPOINT"));
        assert!(is_inherited_claude_var("ANTHROPIC_BASE_URL"));
        assert!(!is_inherited_claude_var("ANTHROPIC_API_KEY"));
        assert!(!is_inherited_claude_var("PATH"));
    }

    #[test]
    fn finds_the_built_sidecar_in_the_repo() {
        // Built by `npm run build:agent` (CI and the check script build it first).
        let p = sidecar_script();
        assert!(p.is_none_or(|p| p.ends_with("index.js")));
    }
}
