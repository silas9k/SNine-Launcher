pub mod app;
mod auth;
pub mod cache;
mod commands;
mod discord_rpc;
pub mod download;
pub mod error;
pub mod foundation;
pub mod ipc;
mod logging;
mod minecraft;
pub mod operations;
pub mod platform;
pub mod profiles;
pub mod security;
pub mod storage;
use app::state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let core = foundation::CoreServices::open_system()?;
            let auth = auth::service::AuthService::system(core.storage().clone(), core.paths())?;
            let profiles = profiles::service::ProfileService::from_core(&core);
            app.manage(auth);
            app.manage(profiles);
            app.manage(core);
            let _ = app::config::load_settings()?;
            discord_rpc::start();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::save_settings,
            commands::get_client_status,
            commands::install_client,
            commands::launch_client,
            commands::stop_client,
            commands::get_launch_status,
            commands::read_launcher_logs,
            commands::open_game_directory,
            commands::pending_design_import,
            commands::fetch_player_skin,
            commands::window_minimize,
            commands::window_toggle_maximize,
            commands::window_close,
            commands::window_start_dragging,
            ipc::phase1_core_status,
            ipc::phase2_shell_bootstrap,
            ipc::phase2_save_shell_settings,
            ipc::phase3_auth_snapshot,
            ipc::phase3_start_device_login,
            ipc::phase3_complete_device_login,
            ipc::phase3_cancel_device_login,
            ipc::phase3_refresh_account,
            ipc::phase3_select_account,
            ipc::phase3_remove_account,
            ipc::phase3_assign_profile_account,
            ipc::phase4_list_profiles,
            ipc::phase4_create_profile,
            ipc::phase4_duplicate_profile,
            ipc::phase4_archive_profile,
            ipc::phase4_trash_profile,
            ipc::phase4_restore_profile,
            ipc::phase4_set_profile_favorite,
            ipc::phase4_cache_gc_preview,
            ipc::phase4_quarantine_unreferenced_cache,
        ])
        .run(tauri::generate_context!())
        .expect("S9Lab Launcher konnte nicht gestartet werden");
}
