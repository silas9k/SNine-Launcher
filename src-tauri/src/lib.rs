pub mod app;
mod auth;
pub mod cache;
pub mod components;
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
pub mod runtime;
pub mod security;
pub mod storage;
mod window_commands;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let core = foundation::CoreServices::open_system()?;
            let auth = auth::service::AuthService::system(core.storage().clone(), core.paths())?;
            let profiles = profiles::service::ProfileService::from_core(&core);
            let runtime = minecraft::service::MinecraftRuntimeService::from_core(&core)?;
            app.manage(auth);
            app.manage(profiles);
            app.manage(runtime);
            app.manage(core);
            let _ = app::config::load_settings()?;
            discord_rpc::start();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            window_commands::window_minimize,
            window_commands::window_toggle_maximize,
            window_commands::window_close,
            window_commands::window_start_dragging,
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
            ipc::phase5_runtime_catalog,
            ipc::phase5_s9lab_component_catalog,
            ipc::phase5_profile_runtime_status,
            ipc::phase5_install_profile,
            ipc::phase5_repair_profile,
            ipc::phase5_launch_profile,
            ipc::phase5_stop_launch,
            ipc::phase5_launch_statuses,
            ipc::phase5_set_s9lab_component,
        ])
        .run(tauri::generate_context!())
        .expect("S9Lab Launcher konnte nicht gestartet werden");
}
