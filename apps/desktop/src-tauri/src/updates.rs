//! Automatic updates (Tauri's updater: the download is checked against our
//! own signing key before it is installed).
//!
//! Updates are read from the GitHub releases of the project, so they only
//! work once the project is public. Release builds made by the release
//! workflow on a public repository set `OT_UPDATES=1`; every other build has
//! updates switched off and says so. For testing, a debug build also turns
//! them on when `OT_UPDATE_URL` names a local `latest.json`.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Url};
use tauri_plugin_updater::UpdaterExt;

use crate::commands::{CmdResult, CommandError};

/// Where releases are listed.
pub const RELEASES_URL: &str = "https://github.com/jackuri/opentrainer/releases";
const LATEST_JSON: &str =
    "https://github.com/jackuri/opentrainer/releases/latest/download/latest.json";

/// Set by the release workflow when the repository is public.
const BUILT_WITH_UPDATES: bool =
    matches!(option_env!("OT_UPDATES"), Some(v) if v.len() == 1 && v.as_bytes()[0] == b'1');

#[derive(Default)]
pub struct UpdateState {
    found: Mutex<Option<tauri_plugin_updater::Update>>,
}

impl std::fmt::Debug for UpdateState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateState").finish_non_exhaustive()
    }
}

fn test_url() -> Option<Url> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let url = std::env::var("OT_UPDATE_URL").ok()?;
    Url::parse(&url).ok()
}

fn endpoint() -> Option<Url> {
    test_url().or_else(|| {
        BUILT_WITH_UPDATES
            .then(|| Url::parse(LATEST_JSON).ok())
            .flatten()
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub version: &'static str,
    pub enabled: bool,
    /// Why updates are off, in plain words.
    pub off_reason: Option<&'static str>,
    pub releases_url: &'static str,
}

#[tauri::command]
pub fn update_status() -> UpdateStatus {
    let enabled = endpoint().is_some();
    UpdateStatus {
        version: env!("CARGO_PKG_VERSION"),
        enabled,
        off_reason: (!enabled).then_some(
            "Automatic updates are off in this copy. They switch on once OpenTrainer is published \
             publicly; until then, new versions are on the releases page.",
        ),
        releases_url: RELEASES_URL,
    }
}

/// Opens one of OpenTrainer's own web pages in the browser. Only these
/// fixed addresses can be opened.
#[tauri::command]
pub async fn open_link(which: String) -> CmdResult<()> {
    let url = match which.as_str() {
        "releases" => RELEASES_URL,
        "guide" => "https://github.com/jackuri/opentrainer/blob/main/docs/USER_GUIDE.md",
        "contribute" => ot_packs::community::CONTRIBUTE_URL,
        "node" => "https://nodejs.org/",
        _ => {
            return Err(CommandError::new(
                "badRequest",
                format!("unknown link {which}"),
            ));
        }
    };
    std::process::Command::new("explorer.exe")
        .arg(url)
        .spawn()
        .map_err(|e| CommandError::new("io", format!("couldn't open the browser: {e}")))?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

fn err(e: impl std::fmt::Display) -> CommandError {
    CommandError::new("update", e.to_string())
}

/// Looks for a newer version. `None` when this is the newest.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, UpdateState>,
) -> CmdResult<Option<UpdateInfo>> {
    let url = endpoint()
        .ok_or_else(|| CommandError::new("updatesOff", "updates are off in this copy"))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(err)?
        .build()
        .map_err(err)?;
    let found = updater.check().await.map_err(|e| {
        CommandError::new(
            "update",
            format!("couldn't check for updates ({e}); check your internet connection"),
        )
    })?;
    let info = found.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.date().to_string()),
    });
    *state.found.lock().unwrap_or_else(|e| e.into_inner()) = found;
    Ok(info)
}

/// Puts the game's original code back and stops everything, as on exit
/// (the installer closes OpenTrainer).
fn before_install(app: &AppHandle) {
    app.state::<crate::commands::AppState>().shutdown();
    app.state::<crate::ai::AiState>().shutdown();
    app.state::<crate::remote::RemoteState>().stop();
}

/// Downloads, checks and installs the update found by `update_check`.
/// OpenTrainer closes and the new version starts.
#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, UpdateState>) -> CmdResult<()> {
    let update = state
        .found
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .ok_or_else(|| CommandError::new("update", "check for updates first"))?;
    let progress = app.clone();
    let mut got = 0usize;
    let bytes = update
        .download(
            move |chunk, total| {
                got += chunk;
                let _ = progress.emit("update-progress", (got, total));
            },
            || {},
        )
        .await
        .map_err(|e| {
            CommandError::new(
                "update",
                format!("the download failed or wasn't signed by us: {e}"),
            )
        })?;
    before_install(&app);
    update.install(bytes).map_err(err)?;
    app.restart();
}

/// Checks once, shortly after start, and tells the UI if there's an update.
pub fn check_in_background(app: AppHandle) {
    if endpoint().is_none() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio_sleep(std::time::Duration::from_secs(15)).await;
        let state = app.state::<UpdateState>();
        if let Ok(Some(info)) = update_check(app.clone(), state).await {
            let _ = app.emit("update-available", info);
        }
    });
}

async fn tokio_sleep(d: std::time::Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_unless_built_for_a_public_release() {
        // Plain test builds aren't release builds and have no test URL.
        if std::env::var_os("OT_UPDATE_URL").is_none() && !BUILT_WITH_UPDATES {
            let s = update_status();
            assert!(!s.enabled);
            assert!(s.off_reason.unwrap().contains("releases page"));
        }
        assert!(LATEST_JSON.starts_with(RELEASES_URL));
    }
}
