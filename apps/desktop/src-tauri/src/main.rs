// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod hotkeys;

use tauri::{Emitter, Manager, RunEvent};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = tauri::Builder::default()
        .manage(commands::AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let service = hotkeys::HotkeyService::start(Box::new(move |generation, cheat| {
                handle
                    .state::<commands::AppState>()
                    .on_hotkey(generation, cheat);
                // Tell the UI to refresh right away instead of on its next poll.
                let _ = handle.emit("trainer-changed", cheat);
            }))?;
            let _ = app.state::<commands::AppState>().hotkeys.set(service);
            Ok(())
        })
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
            commands::packs_list,
            commands::packs_folder,
            commands::open_packs_folder,
            commands::trainer_load,
            commands::trainer_state,
            commands::trainer_unload,
            commands::trainer_toggle,
            commands::trainer_set,
            commands::trainer_release,
            commands::trainer_press,
        ])
        .build(tauri::generate_context!())
        .expect("error while building OpenTrainer");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            // Restore patches and stop freezes before the process ends.
            app.state::<commands::AppState>().shutdown();
        }
    });
}
