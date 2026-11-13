//! JuiceBox Remote: the phone app. Step A of PLAN §10.2: a remote for JuiceBox Trainer's
//! cheats on a PC. It pairs once by QR code and talks to the PC only through `jb-remote`
//! (pinned TLS); it can only do what the trainer's own buttons do.

mod pcs;

use jb_remote::Action;
use pcs::{PcInfo, Pcs, Problem};
use tauri::Manager;

type Result<T> = std::result::Result<T, Problem>;

fn store(app: &tauri::AppHandle) -> Result<Pcs> {
    let dir = app.path().app_data_dir().map_err(|e| Problem {
        kind: "storage",
        message: format!("no folder for the app's data: {e}"),
    })?;
    Ok(Pcs::new(&dir))
}

/// Runs blocking network work off the screen's thread.
async fn blocking<T: Send + 'static>(
    app: tauri::AppHandle,
    f: impl FnOnce(Pcs) -> Result<T> + Send + 'static,
) -> Result<T> {
    let pcs = store(&app)?;
    tauri::async_runtime::spawn_blocking(move || f(pcs))
        .await
        .map_err(|e| Problem {
            kind: "storage",
            message: e.to_string(),
        })?
}

#[tauri::command]
async fn pcs_list(app: tauri::AppHandle) -> Result<Vec<PcInfo>> {
    blocking(app, |p| Ok(p.list())).await
}

/// Pairs with the PC in a QR code's link.
#[tauri::command]
async fn pc_pair(app: tauri::AppHandle, link: String, phone: String) -> Result<PcInfo> {
    blocking(app, move |p| p.pair(&link, &phone)).await
}

#[tauri::command]
async fn pc_forget(app: tauri::AppHandle, id: String) -> Result<()> {
    blocking(app, move |p| p.forget(&id)).await
}

/// What the PC shows: the loaded game's cheats, presets, or that the game closed.
#[tauri::command]
async fn pc_state(app: tauri::AppHandle, id: String) -> Result<serde_json::Value> {
    blocking(app, move |p| p.state(&id)).await
}

#[tauri::command]
async fn pc_action(app: tauri::AppHandle, id: String, action: Action) -> Result<serde_json::Value> {
    blocking(app, move |p| p.act(&id, &action)).await
}

#[tauri::command]
async fn pc_preset(app: tauri::AppHandle, id: String, name: String) -> Result<serde_json::Value> {
    blocking(app, move |p| p.preset(&id, &name)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    builder
        .invoke_handler(tauri::generate_handler![
            pcs_list, pc_pair, pc_forget, pc_state, pc_action, pc_preset
        ])
        .run(tauri::generate_context!())
        .expect("error while running JuiceBox Remote");
}
