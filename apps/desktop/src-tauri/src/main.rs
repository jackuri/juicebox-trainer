// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agreement;
mod ai;
mod browser;
mod cheat_book;
mod commands;
mod covers;
mod dialogs;
mod engine_cheats;
mod gamepad;
mod github;
mod hotkeys;
mod library;
mod live_fix;
mod mascot;
mod my_cheats;
mod overlay;
mod page_watch;
mod problems;
mod remote;
mod saves;
mod sharing;
mod tables;
mod updates;
mod web;
mod web_link;

use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

/// If JuiceBox Trainer itself fails, put the games' original code back before it
/// goes down. Uses the on-disk journal, so it doesn't depend on app state.
fn install_crash_handler(dir: std::path::PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        default(panic);
        // Kept for the Help page (v1.22.0).
        problems::app_crash(&panic.to_string());
        for r in jb_core::journal::recover(&dir) {
            if r.restored > 0 || !r.problems.is_empty() {
                eprintln!(
                    "JuiceBox Trainer: restored {} code change(s) in {} (pid {}) after an error",
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
        jb_mcp::serve_stdio(jb_mcp::Config::standard());
        return;
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // OpenTrainer became JuiceBox Trainer (v1.23.0): bring its folders and
    // GitHub sign-in along before anything reads the new ones.
    let moved_from_open_trainer = jb_core::names::move_legacy_folders() > 0;
    github::move_legacy_sign_in();
    commands::set_moved_from_open_trainer(moved_from_open_trainer);

    let journal_dir = jb_core::journal::default_dir();
    if let Some(dir) = &journal_dir {
        install_crash_handler(dir.clone());
    }

    let app = tauri::Builder::default()
        // A second start (e.g. double-clicking an .otpack) hands its file to
        // the running JuiceBox Trainer instead of opening another window.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
            if let Some(path) = sharing::otpack_arg(argv) {
                sharing::open_from_shell(app, path);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(commands::AppState::default())
        .manage(saves::SavesState::open(jb_saves::default_root()))
        .manage(ai::AiState::default())
        .manage(library::LibraryState::default())
        .manage(remote::RemoteState::default())
        .manage(sharing::SharingState::default())
        .manage(tables::TablesState::default())
        .manage(web::WebState::default())
        .manage(github::GithubState::default())
        .manage(updates::UpdateState::default())
        .on_window_event(|window, event| {
            // Closing the main window closes JuiceBox Trainer (and the overlay).
            if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .setup(move |app| {
            problems::init(app.handle());
            // JuiceBox Trainer's own screen: if it crashes or freezes, say so and
            // restart it instead of leaving a blank window (v1.22.0).
            if let Some(main) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                page_watch::watch(&main, move |failure| {
                    problems::record(problems::app_screen(failure));
                    if failure != page_watch::PageFailure::EngineGone
                        && let Some(w) = handle.get_webview_window("main")
                    {
                        let _ = w.reload();
                    }
                });
            }
            // Notice a game closing whichever screen is showing (v1.22.0).
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    handle.state::<commands::AppState>().check_game_closed();
                }
            });
            if let Some(path) = sharing::otpack_arg(std::env::args()) {
                sharing::open_from_shell(app.handle(), path);
            }
            updates::check_in_background(app.handle().clone());
            // The phone remote, if the player left it on (v1.36.0).
            app.state::<remote::RemoteState>().start_if_on(app.handle());
            saves::start_watcher(app.handle().clone());
            if let Some(dir) = journal_dir.clone() {
                // Undo code changes a crashed or killed earlier run left behind.
                let undone: Vec<_> = jb_core::journal::recover(&dir)
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
            let handle = app.handle().clone();
            let pads = gamepad::PadService::start(Box::new(move |generation, cheat| {
                handle
                    .state::<commands::AppState>()
                    .on_pad(generation, cheat);
                let _ = handle.emit("trainer-changed", cheat);
            }))?;
            let state = app.state::<commands::AppState>();
            let _ = state.pads.set(pads);
            if let Some(dir) = jb_packs::settings::default_dir() {
                let _ = state.settings_dir.set(dir);
            }
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
            commands::memory_view,
            commands::memory_edit,
            commands::scan_first,
            commands::scan_next,
            commands::scan_cancel,
            commands::scan_progress,
            commands::scan_reset,
            commands::scan_results,
            commands::finder_start,
            commands::finder_next,
            commands::finder_results,
            commands::finder_reset,
            commands::my_cheat_save,
            engine_cheats::engine_cheats_support,
            engine_cheats::engine_cheats_make,
            cheat_book::book_open,
            cheat_book::book_save,
            commands::address_list,
            commands::address_add,
            commands::address_remove,
            commands::address_rename,
            commands::address_set_frozen,
            commands::address_write,
            commands::packs_list,
            commands::packs_folder,
            commands::open_packs_folder,
            sharing::packs_pending_open,
            sharing::packs_export,
            sharing::packs_show_file,
            sharing::packs_pick_otpack,
            dialogs::pick_folder,
            dialogs::pick_exe,
            dialogs::pick_table,
            sharing::packs_preview,
            sharing::packs_import,
            tables::tables_search,
            problems::problems_list,
            sharing::community_wanted,
            sharing::community_ask,
            sharing::community_open_request,
            mascot::app_icon_set,
            problems::problems_seen,
            problems::problems_clear,
            web::web_reload,
            agreement::agreement_status,
            agreement::agreement_sign,
            web::web_games,
            web::web_game_add,
            web::web_game_remove,
            web::web_cheats,
            web::web_cheats_save,
            web::web_open,
            web::web_current,
            web::web_close,
            web::web_call,
            web::web_export,
            web::web_pick_file,
            web::web_read_file,
            web::web_install,
            tables::tables_recent,
            github::community_share_web,
            github::github_status,
            github::github_sign_in,
            github::github_sign_in_wait,
            github::github_sign_in_cancel,
            github::github_sign_out,
            github::github_open_device_page,
            github::github_open_pr,
            github::community_share,
            github::community_share_table,
            sharing::community_open_table,
            sharing::community_get_table,
            sharing::community_ratings,
            sharing::community_xp,
            github::community_rate,
            github::community_check_in,
            sharing::community_open_credit,
            sharing::packs_remove,
            sharing::community_list,
            sharing::community_web_status,
            sharing::community_pack_status,
            sharing::community_preview,
            sharing::community_install,
            updates::update_status,
            updates::update_check,
            updates::update_install,
            updates::open_link,
            commands::trainer_load,
            commands::trainer_state,
            commands::trainer_unload,
            commands::trainer_toggle,
            commands::trainer_set,
            commands::trainer_release,
            commands::trainer_press,
            commands::trainer_repair,
            commands::preset_apply,
            commands::preset_save,
            commands::preset_delete,
            commands::preset_auto,
            commands::cheat_note,
            commands::cheat_favourite,
            commands::cheat_key,
            commands::cheat_pad,
            library::library_list,
            library::library_cover,
            library::library_add,
            library::library_remove,
            library::library_set_hidden,
            library::library_open_folder,
            library::library_cancel,
            library::library_play,
            library::running_games,
            overlay::overlay_set,
            overlay::overlay_visible,
            remote::remote_start,
            remote::remote_stop,
            remote::remote_status,
            remote::remote_forget,
            remote::remote_unpair,
            commands::pointer_scan,
            commands::pointer_filter,
            commands::pointer_results,
            commands::pointer_clear,
            commands::signature_make,
            commands::recovery_report,
            saves::saves_overview,
            saves::saves_accounts,
            saves::saves_account_plan,
            saves::saves_account_copy,
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
            ai::ai_note,
            ai::ai_cancel,
            ai::ai_sign_in,
            ai::ai_install,
            ai::ai_uninstall,
        ])
        .build(tauri::generate_context!())
        .expect("error while building JuiceBox Trainer");

    app.run(|app, event| {
        if let RunEvent::Exit = event {
            // Restore patches and stop freezes before the process ends.
            app.state::<ai::AiState>().shutdown();
            app.state::<remote::RemoteState>().stop();
            app.state::<commands::AppState>().shutdown();
        }
    });
}
