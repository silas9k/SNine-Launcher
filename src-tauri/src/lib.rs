pub mod app;
mod auth;
pub mod cache;
mod capes;
pub mod cloud_sync;
pub mod components;
pub mod content;
pub mod content_projection;
pub mod content_service;
mod discord_rpc;
pub mod download;
pub mod error;
pub mod foundation;
pub mod ipc;
mod logging;
mod launcher_font;
mod snine_bridge;
mod snine_client_delivery;
mod minecraft;
pub mod modrinth;
pub mod operations;
pub mod platform;
pub mod profile_format;
pub mod profiles;
pub mod runtime;
pub mod security;
pub mod storage;
pub mod updates;
mod window_commands;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let core = foundation::CoreServices::open_system()?;
            let java_registry = core.registry().clone();
            tauri::async_runtime::spawn(async move {
                let resolver = minecraft::java_runtime::JavaRuntimeResolver::new(java_registry);
                if let Err(error) = resolver
                    .resolve(&runtime::JavaPolicy::Managed { major_version: 21 })
                    .await
                {
                    eprintln!("[snine-java] automatic Java 21 bootstrap failed: {error}");
                }
            });
            let auth = auth::service::AuthService::system(core.storage().clone(), core.paths())?;
            let profiles = profiles::service::ProfileService::from_core(&core);
            let runtime = minecraft::service::MinecraftRuntimeService::from_core(&core)?;
            let content = content_service::Phase6ContentService::from_core(&core)?;
            let updates = updates::service::UpdateService::from_core(&core)?;
            let cloud_sync = cloud_sync::service::CloudSyncService::from_core(&core)?;
            app.manage(auth);
            app.manage(profiles);
            app.manage(runtime);
            app.manage(content);
            app.manage(updates);
            app.manage(cloud_sync);
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
            discord_rpc::discord_rpc_set_enabled,
            launcher_font::launcher_minecraft_font_data_url,
            snine_bridge::snine_launcher_cosmetics,
            snine_bridge::snine_launcher_live_state,
            snine_bridge::snine_launcher_resolve_cosmetics,
            snine_bridge::snine_launcher_player_skin,
            snine_bridge::snine_launcher_import_skin,
            capes::snine_launcher_custom_capes,
            capes::snine_launcher_custom_cape_upload,
            capes::snine_launcher_custom_cape_favorite,
            capes::snine_launcher_custom_cape_equip,
            capes::snine_launcher_custom_cape_unequip,
            capes::snine_launcher_custom_cape_texture,
            capes::snine_launcher_custom_cape_preview,
            capes::snine_launcher_save_cape_template,
            capes::snine_launcher_vanilla_capes,
            snine_client_delivery::snine_client_update_check,
            snine_client_delivery::snine_client_download_update,
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
            ipc::phase4_rename_profile,
            ipc::phase4_cache_gc_preview,
            ipc::phase4_quarantine_unreferenced_cache,
            ipc::phase5_runtime_catalog,
            ipc::phase5_s9lab_component_catalog,
            ipc::phase5_profile_runtime_status,
            ipc::phase5_install_profile,
            ipc::phase5_repair_profile,
            ipc::phase5_launch_profile,
            ipc::phase5_launch_instance,
            ipc::phase5_profiles_workspace,
            ipc::phase5_instance_settings,
            ipc::phase5_save_instance_settings,
            ipc::phase5_open_instance_folder,
            ipc::phase5_stop_launch,
            ipc::phase5_launch_statuses,
            ipc::phase5_set_s9lab_component,
            ipc::phase6_content_snapshot,
            ipc::phase6_check_content_updates,
            ipc::phase6_modrinth_search,
            ipc::phase6_modrinth_project,
            ipc::phase6_install_modrinth,
            ipc::phase6_set_content_enabled,
            ipc::phase6_remove_content,
            ipc::phase6_update_content,
            ipc::phase6_add_local_file,
            ipc::phase6_import_modrinth_pack,
            ipc::phase6_export_profile,
            ipc::phase6_import_profile,
            ipc::phase7_update_snapshot,
            ipc::phase7_save_update_policy,
            ipc::phase7_preview_profile_updates,
            ipc::phase7_create_restore_point,
            ipc::phase7_apply_profile_updates,
            ipc::phase7_rollback_profile,
            ipc::phase7_restore_backup,
            ipc::phase7_run_automatic_updates,
            ipc::phase8_cloud_sync_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("SNine Launcher konnte nicht gestartet werden");
}
