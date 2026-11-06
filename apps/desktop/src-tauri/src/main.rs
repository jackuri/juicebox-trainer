// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_processes,
            commands::check_guard,
            commands::attach,
            commands::detach,
            commands::current_session,
            commands::refresh_modules,
            commands::read_value,
            commands::write_value,
            commands::read_bytes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenTrainer");
}
