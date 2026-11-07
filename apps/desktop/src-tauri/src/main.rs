// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod commands;
mod hotkeys;
mod saves;

use tauri::{Emitter, Manager, RunEvent};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

/// If OpenTrainer itself fails, put the games' original code back before it
/// goes down. Uses the on-disk journal, so it doesn't depend on app state.
fn install_crash_handler(dir: std::path::PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        default(panic);
        for r in ot_core::journal::recover(&dir) {
            if r.restored > 0 || !r.problems.is_empty() {
                eprintln!(
                    "OpenTrainer: restored {} code change(s) in {} (pid {}) after an error",
                    r.restored, r.exe, r.pid
                );
            }
        }
    }));
}

fn main() {
    // `--mcp`: run as the AI helper's guarded tool server on stdio (started
    // by the sidecar). stdout carries the protocol, so this comes first.
    if std::env::args().any(|a| a == "--mcp") {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .init();
        ot_mcp::serve_stdio(ot_mcp::Config::standard());
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let journal_dir = ot_core::journal::default_dir();
    if let Some(dir) = &journal_dir {
        install_crash_handler(dir.clone());
    }

    let app = tauri::Builder::default()
        .manage(commands::AppState::default())
        .manage(saves::SavesState::open(ot_saves::default_root()))
        .manage(ai::AiState::default())
        .setup(move |app| {
            saves::start_watcher(app.handle().clone());
            if let Some(dir) = journal_dir.clone() {
                // Undo code changes a crashed or killed earlier run left behind.
                let undone: Vec<_> = ot_core::journal::recover(&dir)
                    .into_iter()
                    .filter(|r| r.restored > 0 || !r.problems.is_empty())
                    .collect();
                for r in &undone {
                    if r.problems.is_empty() {
                        info!(exe = %r.exe, restored = r.restored, "recovered after a crash");
                    } else {
                        warn!(exe = %r.exe, problems = ?r.problems, "recovery incomplete");
                    }
                }
                let state = app.state::<commands::AppState>();
                *state.recovered.lock().unwrap_or_else(|e| e.into_inner()) = undone;
                let _ = state.journal_dir.set(dir);
            }
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
            commands::pointer_scan,
            commands::pointer_filter,
            commands::pointer_results,
            commands::pointer_clear,
            commands::signature_make,
            commands::recovery_report,
            saves::saves_overview,
            saves::saves_add_process,
            saves::saves_add_exe,
            saves::saves_remove_game,
            saves::saves_relocate,
            saves::saves_add_folder,
            saves::saves_remove_folder,
            saves::saves_backups,
            saves::saves_backup_now,
            saves::saves_delete_backup,
            saves::saves_set_note,
            saves::saves_diff,
            saves::saves_restore,
            saves::saves_open_folder,
            saves::saves_files,
            saves::saves_open_file,
            saves::saves_hex,
            saves::saves_edit,
            saves::saves_quick_codes,
            saves::saves_apply_quick_code,
            ai::ai_status,
            ai::ai_start,
            ai::ai_answer,
            ai::ai_cancel,
            ai::ai_sign_in,
        ])
        .build(tauri::generate_context!())
        .expect("error while building OpenTrainer");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            // Restore patches and stop freezes before the process ends.
            app.state::<ai::AiState>().shutdown();
            app.state::<commands::AppState>().shutdown();
        }
    });
}
