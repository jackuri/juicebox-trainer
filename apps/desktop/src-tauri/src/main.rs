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
            commands::scan_first,
            commands::scan_next,
            commands::scan_cancel,
            commands::scan_progress,
            commands::scan_reset,
            commands::scan_results,
            commands::address_list,
            commands::address_add,
            commands::address_remove,
            commands::address_rename,
            commands::address_set_frozen,
            commands::address_write,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenTrainer");
}
