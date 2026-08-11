use crate::{
    app::config::{self, ShellSettings},
    auth::{
        model::Account,
        service::{AuthService, AuthSnapshot, DeviceLoginPrompt},
    },
    cloud_sync::{model::CloudSyncSnapshot, service::CloudSyncService},
    content::ContentKind,
    content_service::{
        Phase6ContentService, Phase6ContentSnapshot, Phase6OperationResult,
        Phase6ProfileTransferResult, Phase6ProjectDetail, Phase6SearchResult,
    },
    error::{AppError, AppResult},
    foundation::CoreStatus,
    profiles::{model::ProfileSummary, service::ProfileService},
    updates::{
        model::{
            ProfileUpdatePreview, RestorePointSummary, UpdateCenterSnapshot, UpdateOperationResult,
            UpdatePolicyV1,
        },
        service::UpdateService,
    },
};
use serde::Serialize;

pub const IPC_CONTRACT_VERSION: u32 = 8;
pub const PHASE1_CORE_STATUS_COMMAND: &str = "phase1_core_status";
pub const PHASE2_SHELL_BOOTSTRAP_COMMAND: &str = "phase2_shell_bootstrap";
pub const PHASE2_SAVE_SHELL_SETTINGS_COMMAND: &str = "phase2_save_shell_settings";
pub const PHASE3_AUTH_SNAPSHOT_COMMAND: &str = "phase3_auth_snapshot";
pub const PHASE3_START_DEVICE_LOGIN_COMMAND: &str = "phase3_start_device_login";
pub const PHASE3_COMPLETE_DEVICE_LOGIN_COMMAND: &str = "phase3_complete_device_login";
pub const PHASE3_CANCEL_DEVICE_LOGIN_COMMAND: &str = "phase3_cancel_device_login";
pub const PHASE3_REFRESH_ACCOUNT_COMMAND: &str = "phase3_refresh_account";
pub const PHASE3_SELECT_ACCOUNT_COMMAND: &str = "phase3_select_account";
pub const PHASE3_REMOVE_ACCOUNT_COMMAND: &str = "phase3_remove_account";
pub const PHASE3_ASSIGN_PROFILE_ACCOUNT_COMMAND: &str = "phase3_assign_profile_account";
pub const PHASE4_LIST_PROFILES_COMMAND: &str = "phase4_list_profiles";
pub const PHASE4_CREATE_PROFILE_COMMAND: &str = "phase4_create_profile";
pub const PHASE4_DUPLICATE_PROFILE_COMMAND: &str = "phase4_duplicate_profile";
pub const PHASE4_ARCHIVE_PROFILE_COMMAND: &str = "phase4_archive_profile";
pub const PHASE4_TRASH_PROFILE_COMMAND: &str = "phase4_trash_profile";
pub const PHASE4_RESTORE_PROFILE_COMMAND: &str = "phase4_restore_profile";
pub const PHASE4_SET_PROFILE_FAVORITE_COMMAND: &str = "phase4_set_profile_favorite";
pub const PHASE4_CACHE_GC_PREVIEW_COMMAND: &str = "phase4_cache_gc_preview";
pub const PHASE4_QUARANTINE_CACHE_COMMAND: &str = "phase4_quarantine_unreferenced_cache";
pub const PHASE5_RUNTIME_CATALOG_COMMAND: &str = "phase5_runtime_catalog";
pub const PHASE5_S9LAB_COMPONENT_CATALOG_COMMAND: &str = "phase5_s9lab_component_catalog";
pub const PHASE5_PROFILE_RUNTIME_STATUS_COMMAND: &str = "phase5_profile_runtime_status";
pub const PHASE5_INSTALL_PROFILE_COMMAND: &str = "phase5_install_profile";
pub const PHASE5_REPAIR_PROFILE_COMMAND: &str = "phase5_repair_profile";
pub const PHASE5_LAUNCH_PROFILE_COMMAND: &str = "phase5_launch_profile";
pub const PHASE5_STOP_LAUNCH_COMMAND: &str = "phase5_stop_launch";
pub const PHASE5_LAUNCH_STATUSES_COMMAND: &str = "phase5_launch_statuses";
pub const PHASE5_SET_S9LAB_COMPONENT_COMMAND: &str = "phase5_set_s9lab_component";
pub const PHASE6_CONTENT_SNAPSHOT_COMMAND: &str = "phase6_content_snapshot";
pub const PHASE6_CHECK_CONTENT_UPDATES_COMMAND: &str = "phase6_check_content_updates";
pub const PHASE6_MODRINTH_SEARCH_COMMAND: &str = "phase6_modrinth_search";
pub const PHASE6_MODRINTH_PROJECT_COMMAND: &str = "phase6_modrinth_project";
pub const PHASE6_INSTALL_MODRINTH_COMMAND: &str = "phase6_install_modrinth";
pub const PHASE6_SET_CONTENT_ENABLED_COMMAND: &str = "phase6_set_content_enabled";
pub const PHASE6_REMOVE_CONTENT_COMMAND: &str = "phase6_remove_content";
pub const PHASE6_UPDATE_CONTENT_COMMAND: &str = "phase6_update_content";
pub const PHASE6_ADD_LOCAL_FILE_COMMAND: &str = "phase6_add_local_file";
pub const PHASE6_IMPORT_MODRINTH_PACK_COMMAND: &str = "phase6_import_modrinth_pack";
pub const PHASE6_EXPORT_PROFILE_COMMAND: &str = "phase6_export_profile";
pub const PHASE6_IMPORT_PROFILE_COMMAND: &str = "phase6_import_profile";
pub const PHASE7_UPDATE_SNAPSHOT_COMMAND: &str = "phase7_update_snapshot";
pub const PHASE7_SAVE_UPDATE_POLICY_COMMAND: &str = "phase7_save_update_policy";
pub const PHASE7_PREVIEW_PROFILE_UPDATES_COMMAND: &str = "phase7_preview_profile_updates";
pub const PHASE7_CREATE_RESTORE_POINT_COMMAND: &str = "phase7_create_restore_point";
pub const PHASE7_APPLY_PROFILE_UPDATES_COMMAND: &str = "phase7_apply_profile_updates";
pub const PHASE7_ROLLBACK_PROFILE_COMMAND: &str = "phase7_rollback_profile";
pub const PHASE7_RESTORE_BACKUP_COMMAND: &str = "phase7_restore_backup";
pub const PHASE7_RUN_AUTOMATIC_UPDATES_COMMAND: &str = "phase7_run_automatic_updates";
pub const PHASE8_CLOUD_SYNC_SNAPSHOT_COMMAND: &str = "phase8_cloud_sync_snapshot";
pub const TYPED_IPC_ERROR_FIELDS: &[&str] = &["code", "messageKey", "params"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    pub code: String,
    pub message_key: String,
    pub params: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase2ShellBootstrap {
    pub settings: ShellSettings,
}

impl From<AppError> for IpcError {
    fn from(value: AppError) -> Self {
        let descriptor = value.descriptor();
        Self {
            code: descriptor.code,
            message_key: descriptor.message_key,
            params: descriptor.params,
        }
    }
}

fn authorize_main_window(window: &tauri::Window) -> AppResult<()> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(AppError::coded_with(
            "ipc_window_not_allowed",
            [("windowLabel", window.label().to_string())],
        ))
    }
}

#[tauri::command]
pub fn phase1_core_status(
    window: tauri::Window,
    core: tauri::State<'_, crate::foundation::CoreServices>,
) -> Result<CoreStatus, IpcError> {
    authorize_main_window(&window)?;
    core.status().map_err(Into::into)
}

#[tauri::command]
pub fn phase2_shell_bootstrap(
    window: tauri::Window,
    core: tauri::State<'_, crate::foundation::CoreServices>,
) -> Result<Phase2ShellBootstrap, IpcError> {
    authorize_main_window(&window)?;
    let settings = config::load_settings_from(&core.paths().settings_file)?;
    Ok(Phase2ShellBootstrap {
        settings: settings.shell_settings(),
    })
}

#[tauri::command]
pub fn phase2_save_shell_settings(
    window: tauri::Window,
    core: tauri::State<'_, crate::foundation::CoreServices>,
    settings: ShellSettings,
) -> Result<Phase2ShellBootstrap, IpcError> {
    authorize_main_window(&window)?;
    let current = config::load_settings_from(&core.paths().settings_file)?;
    let next = current.apply_shell_settings(settings)?;
    let saved = config::save_settings_to(&core.paths().settings_file, &next)?;
    Ok(Phase2ShellBootstrap {
        settings: saved.shell_settings(),
    })
}

#[tauri::command]
pub fn phase3_auth_snapshot(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
) -> Result<AuthSnapshot, IpcError> {
    authorize_main_window(&window)?;
    auth.snapshot().map_err(Into::into)
}

#[tauri::command]
pub async fn phase3_start_device_login(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
    locale: String,
) -> Result<DeviceLoginPrompt, IpcError> {
    authorize_main_window(&window)?;
    auth.start_device_login(&locale).await.map_err(Into::into)
}

#[tauri::command]
pub async fn phase3_complete_device_login(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
    login_id: String,
) -> Result<Account, IpcError> {
    authorize_main_window(&window)?;
    auth.complete_device_login(&login_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn phase3_cancel_device_login(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
    login_id: String,
) -> Result<(), IpcError> {
    authorize_main_window(&window)?;
    auth.cancel_device_login(&login_id).map_err(Into::into)
}

#[tauri::command]
pub async fn phase3_refresh_account(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
    account_id: String,
) -> Result<Account, IpcError> {
    authorize_main_window(&window)?;
    auth.refresh_account(&account_id).await.map_err(Into::into)
}

#[tauri::command]
pub fn phase3_select_account(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
    account_id: String,
) -> Result<Account, IpcError> {
    authorize_main_window(&window)?;
    auth.select_account(&account_id).map_err(Into::into)
}

#[tauri::command]
pub fn phase3_remove_account(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
    account_id: String,
) -> Result<(), IpcError> {
    authorize_main_window(&window)?;
    auth.remove_account(&account_id).map_err(Into::into)
}

#[tauri::command]
pub fn phase3_assign_profile_account(
    window: tauri::Window,
    auth: tauri::State<'_, AuthService>,
    profile_id: String,
    account_id: Option<String>,
) -> Result<(), IpcError> {
    authorize_main_window(&window)?;
    auth.assign_profile_account(&profile_id, account_id.as_deref())
        .map_err(Into::into)
}

#[tauri::command]
pub fn phase4_list_profiles(
    window: tauri::Window,
    profiles: tauri::State<'_, ProfileService>,
) -> Result<Vec<ProfileSummary>, IpcError> {
    authorize_main_window(&window)?;
    profiles.list_profiles().map_err(Into::into)
}

#[tauri::command]
pub async fn phase4_create_profile(
    window: tauri::Window,
    profiles: tauri::State<'_, ProfileService>,
    display_name: String,
) -> Result<ProfileSummary, IpcError> {
    authorize_main_window(&window)?;
    let profiles = profiles.inner().clone();
    tauri::async_runtime::spawn_blocking(move || profiles.create_profile(&display_name))
        .await
        .map_err(|_| IpcError::from(AppError::coded("profile_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase4_duplicate_profile(
    window: tauri::Window,
    profiles: tauri::State<'_, ProfileService>,
    profile_id: String,
    display_name: String,
) -> Result<ProfileSummary, IpcError> {
    authorize_main_window(&window)?;
    let profiles = profiles.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        profiles.duplicate_profile(&profile_id, &display_name)
    })
    .await
    .map_err(|_| IpcError::from(AppError::coded("profile_worker_failed")))?
    .map_err(Into::into)
}

#[tauri::command]
pub fn phase4_archive_profile(
    window: tauri::Window,
    profiles: tauri::State<'_, ProfileService>,
    profile_id: String,
) -> Result<ProfileSummary, IpcError> {
    authorize_main_window(&window)?;
    profiles.archive_profile(&profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn phase4_trash_profile(
    window: tauri::Window,
    profiles: tauri::State<'_, ProfileService>,
    profile_id: String,
) -> Result<ProfileSummary, IpcError> {
    authorize_main_window(&window)?;
    profiles.trash_profile(&profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn phase4_restore_profile(
    window: tauri::Window,
    profiles: tauri::State<'_, ProfileService>,
    profile_id: String,
) -> Result<ProfileSummary, IpcError> {
    authorize_main_window(&window)?;
    profiles.restore_profile(&profile_id).map_err(Into::into)
}

#[tauri::command]
pub fn phase4_set_profile_favorite(
    window: tauri::Window,
    profiles: tauri::State<'_, ProfileService>,
    profile_id: String,
    favorite: bool,
) -> Result<ProfileSummary, IpcError> {
    authorize_main_window(&window)?;
    profiles
        .set_favorite(&profile_id, favorite)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase4_cache_gc_preview(
    window: tauri::Window,
    core: tauri::State<'_, crate::foundation::CoreServices>,
) -> Result<crate::cache::CacheGcReport, IpcError> {
    authorize_main_window(&window)?;
    let cache = core.cache().clone();
    tauri::async_runtime::spawn_blocking(move || cache.gc_preview())
        .await
        .map_err(|_| IpcError::from(AppError::coded("cache_gc_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase4_quarantine_unreferenced_cache(
    window: tauri::Window,
    core: tauri::State<'_, crate::foundation::CoreServices>,
) -> Result<crate::cache::CacheGcReport, IpcError> {
    authorize_main_window(&window)?;
    let cache = core.cache().clone();
    tauri::async_runtime::spawn_blocking(move || cache.quarantine_unreferenced())
        .await
        .map_err(|_| IpcError::from(AppError::coded("cache_gc_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_runtime_catalog(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    minecraft_version: Option<String>,
) -> Result<crate::minecraft::service::Phase5RuntimeCatalog, IpcError> {
    authorize_main_window(&window)?;
    runtime
        .catalog(minecraft_version.as_deref())
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_s9lab_component_catalog(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    intent: crate::runtime::ProfileRuntimeIntent,
) -> Result<crate::minecraft::service::Phase5ComponentCatalog, IpcError> {
    authorize_main_window(&window)?;
    runtime.component_catalog(intent).await.map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_profile_runtime_status(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    profile_id: String,
) -> Result<crate::minecraft::service::Phase5RuntimeStatus, IpcError> {
    authorize_main_window(&window)?;
    runtime.status(&profile_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_install_profile(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    profile_id: String,
    intent: crate::runtime::ProfileRuntimeIntent,
    component: crate::profiles::model::S9labComponentSelection,
) -> Result<crate::minecraft::service::RuntimeOperationResult, IpcError> {
    authorize_main_window(&window)?;
    runtime
        .install(&profile_id, intent, component)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_repair_profile(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    profile_id: String,
) -> Result<crate::minecraft::service::RuntimeOperationResult, IpcError> {
    authorize_main_window(&window)?;
    let runtime = runtime.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runtime.repair(&profile_id))
        .await
        .map_err(|_| IpcError::from(AppError::coded("runtime_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_launch_profile(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    auth: tauri::State<'_, AuthService>,
    profile_id: String,
    memory_mb: u32,
) -> Result<crate::minecraft::profile_launch::ProfileLaunchStatus, IpcError> {
    authorize_main_window(&window)?;
    runtime
        .launch(auth.inner(), &profile_id, memory_mb)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_stop_launch(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    launch_id: String,
) -> Result<crate::minecraft::profile_launch::ProfileLaunchStatus, IpcError> {
    authorize_main_window(&window)?;
    runtime.stop(&launch_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_launch_statuses(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
) -> Result<Vec<crate::minecraft::profile_launch::ProfileLaunchStatus>, IpcError> {
    authorize_main_window(&window)?;
    runtime.launch_statuses().await.map_err(Into::into)
}

#[tauri::command]
pub async fn phase5_set_s9lab_component(
    window: tauri::Window,
    runtime: tauri::State<'_, crate::minecraft::service::MinecraftRuntimeService>,
    profile_id: String,
    selection: crate::profiles::model::S9labComponentSelection,
) -> Result<crate::minecraft::service::RuntimeOperationResult, IpcError> {
    authorize_main_window(&window)?;
    runtime
        .change_component(&profile_id, selection)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_content_snapshot(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
) -> Result<Phase6ContentSnapshot, IpcError> {
    authorize_main_window(&window)?;
    let content = content.inner().clone();
    tauri::async_runtime::spawn_blocking(move || content.snapshot(&profile_id))
        .await
        .map_err(|_| IpcError::from(AppError::coded("content_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_check_content_updates(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
) -> Result<Phase6ContentSnapshot, IpcError> {
    authorize_main_window(&window)?;
    let content = content.inner().clone();
    let worker = content.clone();
    let worker_profile_id = profile_id.clone();
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || worker.snapshot(&worker_profile_id))
            .await
            .map_err(|_| IpcError::from(AppError::coded("content_worker_failed")))?
            .map_err(IpcError::from)?;
    content
        .populate_snapshot_updates(&profile_id, snapshot)
        .await
        .map_err(Into::into)
}

#[tauri::command]
// Tauri exposes command inputs as individual, generated camelCase fields. Keeping
// the six typed search filters flat preserves the shared IPC contract.
#[allow(clippy::too_many_arguments)]
pub async fn phase6_modrinth_search(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    query: String,
    content_type: ContentKind,
    minecraft_version: String,
    loader: crate::runtime::LoaderKind,
    offset: u32,
    limit: u8,
) -> Result<Phase6SearchResult, IpcError> {
    authorize_main_window(&window)?;
    content
        .search(
            query,
            content_type,
            minecraft_version,
            loader,
            offset,
            limit,
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_modrinth_project(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    project_id: String,
) -> Result<Phase6ProjectDetail, IpcError> {
    authorize_main_window(&window)?;
    content.project(&project_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_install_modrinth(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
    project_id: String,
    version_id: Option<String>,
) -> Result<Phase6OperationResult, IpcError> {
    authorize_main_window(&window)?;
    content
        .install_modrinth(&profile_id, &project_id, version_id.as_deref())
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_set_content_enabled(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
    content_id: String,
    enabled: bool,
) -> Result<Phase6OperationResult, IpcError> {
    authorize_main_window(&window)?;
    let content = content.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        content.set_enabled(&profile_id, &content_id, enabled)
    })
    .await
    .map_err(|_| IpcError::from(AppError::coded("content_worker_failed")))?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_remove_content(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
    content_id: String,
) -> Result<Phase6OperationResult, IpcError> {
    authorize_main_window(&window)?;
    let content = content.inner().clone();
    tauri::async_runtime::spawn_blocking(move || content.remove(&profile_id, &content_id))
        .await
        .map_err(|_| IpcError::from(AppError::coded("content_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_update_content(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
    content_id: String,
) -> Result<Phase6OperationResult, IpcError> {
    authorize_main_window(&window)?;
    content
        .update(&profile_id, &content_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_add_local_file(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
    source_path: String,
    content_type: ContentKind,
) -> Result<Phase6OperationResult, IpcError> {
    authorize_main_window(&window)?;
    let content = content.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        content.add_local_file(&profile_id, &source_path, content_type)
    })
    .await
    .map_err(|_| IpcError::from(AppError::coded("content_worker_failed")))?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_import_modrinth_pack(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
    source_path: String,
) -> Result<Phase6OperationResult, IpcError> {
    authorize_main_window(&window)?;
    content
        .import_modrinth_pack(&profile_id, &source_path)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_export_profile(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    profile_id: String,
) -> Result<Phase6ProfileTransferResult, IpcError> {
    authorize_main_window(&window)?;
    let content = content.inner().clone();
    tauri::async_runtime::spawn_blocking(move || content.export_profile(&profile_id))
        .await
        .map_err(|_| IpcError::from(AppError::coded("content_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase6_import_profile(
    window: tauri::Window,
    content: tauri::State<'_, Phase6ContentService>,
    source_path: String,
) -> Result<Phase6ProfileTransferResult, IpcError> {
    authorize_main_window(&window)?;
    content
        .import_profile(&source_path)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_update_snapshot(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
) -> Result<UpdateCenterSnapshot, IpcError> {
    authorize_main_window(&window)?;
    let updates = updates.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updates.snapshot())
        .await
        .map_err(|_| IpcError::from(AppError::coded("update_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_save_update_policy(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
    policy: UpdatePolicyV1,
) -> Result<UpdateCenterSnapshot, IpcError> {
    authorize_main_window(&window)?;
    let updates = updates.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updates.save_policy(policy))
        .await
        .map_err(|_| IpcError::from(AppError::coded("update_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_preview_profile_updates(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
    profile_id: String,
) -> Result<ProfileUpdatePreview, IpcError> {
    authorize_main_window(&window)?;
    updates.preview(&profile_id).await.map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_create_restore_point(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
    profile_id: String,
) -> Result<RestorePointSummary, IpcError> {
    authorize_main_window(&window)?;
    let updates = updates.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updates.create_restore_point(&profile_id))
        .await
        .map_err(|_| IpcError::from(AppError::coded("update_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_apply_profile_updates(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
    profile_id: String,
    content_ids: Vec<String>,
) -> Result<UpdateOperationResult, IpcError> {
    authorize_main_window(&window)?;
    updates
        .apply_updates(&profile_id, &content_ids)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_rollback_profile(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
    profile_id: String,
    revision_id: String,
) -> Result<UpdateOperationResult, IpcError> {
    authorize_main_window(&window)?;
    let updates = updates.inner().clone();
    tauri::async_runtime::spawn_blocking(move || updates.rollback(&profile_id, &revision_id))
        .await
        .map_err(|_| IpcError::from(AppError::coded("update_worker_failed")))?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_restore_backup(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
    backup_id: String,
    display_name: String,
    include_account: bool,
    include_settings: bool,
    include_files: bool,
) -> Result<ProfileSummary, IpcError> {
    authorize_main_window(&window)?;
    let updates = updates.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        updates.restore_backup(
            &backup_id,
            &display_name,
            include_account,
            include_settings,
            include_files,
        )
    })
    .await
    .map_err(|_| IpcError::from(AppError::coded("update_worker_failed")))?
    .map_err(Into::into)
}

#[tauri::command]
pub async fn phase7_run_automatic_updates(
    window: tauri::Window,
    updates: tauri::State<'_, UpdateService>,
) -> Result<Vec<UpdateOperationResult>, IpcError> {
    authorize_main_window(&window)?;
    updates
        .run_configured_automatic_updates()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn phase8_cloud_sync_snapshot(
    window: tauri::Window,
    cloud_sync: tauri::State<'_, CloudSyncService>,
) -> Result<CloudSyncSnapshot, IpcError> {
    authorize_main_window(&window)?;
    let cloud_sync = cloud_sync.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cloud_sync.snapshot())
        .await
        .map_err(|_| IpcError::from(AppError::coded("cloud_sync_worker_failed")))?
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ContractFile {
        version: u32,
        commands: Vec<ContractCommand>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ContractCommand {
        name: String,
        error_type: String,
    }

    #[test]
    fn rust_commands_match_shared_contract_file() {
        let contract: ContractFile =
            serde_json::from_str(include_str!("../../../contracts/ipc-contracts.json"))
                .expect("contract json");
        assert_eq!(contract.version, IPC_CONTRACT_VERSION);
        let expected = [
            PHASE1_CORE_STATUS_COMMAND,
            PHASE2_SHELL_BOOTSTRAP_COMMAND,
            PHASE2_SAVE_SHELL_SETTINGS_COMMAND,
            PHASE3_AUTH_SNAPSHOT_COMMAND,
            PHASE3_START_DEVICE_LOGIN_COMMAND,
            PHASE3_COMPLETE_DEVICE_LOGIN_COMMAND,
            PHASE3_CANCEL_DEVICE_LOGIN_COMMAND,
            PHASE3_REFRESH_ACCOUNT_COMMAND,
            PHASE3_SELECT_ACCOUNT_COMMAND,
            PHASE3_REMOVE_ACCOUNT_COMMAND,
            PHASE3_ASSIGN_PROFILE_ACCOUNT_COMMAND,
            PHASE4_LIST_PROFILES_COMMAND,
            PHASE4_CREATE_PROFILE_COMMAND,
            PHASE4_DUPLICATE_PROFILE_COMMAND,
            PHASE4_ARCHIVE_PROFILE_COMMAND,
            PHASE4_TRASH_PROFILE_COMMAND,
            PHASE4_RESTORE_PROFILE_COMMAND,
            PHASE4_SET_PROFILE_FAVORITE_COMMAND,
            PHASE4_CACHE_GC_PREVIEW_COMMAND,
            PHASE4_QUARANTINE_CACHE_COMMAND,
            PHASE5_RUNTIME_CATALOG_COMMAND,
            PHASE5_S9LAB_COMPONENT_CATALOG_COMMAND,
            PHASE5_PROFILE_RUNTIME_STATUS_COMMAND,
            PHASE5_INSTALL_PROFILE_COMMAND,
            PHASE5_REPAIR_PROFILE_COMMAND,
            PHASE5_LAUNCH_PROFILE_COMMAND,
            PHASE5_STOP_LAUNCH_COMMAND,
            PHASE5_LAUNCH_STATUSES_COMMAND,
            PHASE5_SET_S9LAB_COMPONENT_COMMAND,
            PHASE6_CONTENT_SNAPSHOT_COMMAND,
            PHASE6_CHECK_CONTENT_UPDATES_COMMAND,
            PHASE6_MODRINTH_SEARCH_COMMAND,
            PHASE6_MODRINTH_PROJECT_COMMAND,
            PHASE6_INSTALL_MODRINTH_COMMAND,
            PHASE6_SET_CONTENT_ENABLED_COMMAND,
            PHASE6_REMOVE_CONTENT_COMMAND,
            PHASE6_UPDATE_CONTENT_COMMAND,
            PHASE6_ADD_LOCAL_FILE_COMMAND,
            PHASE6_IMPORT_MODRINTH_PACK_COMMAND,
            PHASE6_EXPORT_PROFILE_COMMAND,
            PHASE6_IMPORT_PROFILE_COMMAND,
            PHASE7_UPDATE_SNAPSHOT_COMMAND,
            PHASE7_SAVE_UPDATE_POLICY_COMMAND,
            PHASE7_PREVIEW_PROFILE_UPDATES_COMMAND,
            PHASE7_CREATE_RESTORE_POINT_COMMAND,
            PHASE7_APPLY_PROFILE_UPDATES_COMMAND,
            PHASE7_ROLLBACK_PROFILE_COMMAND,
            PHASE7_RESTORE_BACKUP_COMMAND,
            PHASE7_RUN_AUTOMATIC_UPDATES_COMMAND,
            PHASE8_CLOUD_SYNC_SNAPSHOT_COMMAND,
        ];
        for command_name in expected {
            let command = contract
                .commands
                .iter()
                .find(|command| command.name == command_name)
                .unwrap_or_else(|| panic!("missing contract for {command_name}"));
            assert_eq!(command.error_type, "TypedIpcError");
        }
        assert_eq!(contract.commands.len(), expected.len());
        assert_eq!(TYPED_IPC_ERROR_FIELDS, ["code", "messageKey", "params"]);
    }

    #[test]
    fn shell_settings_round_trip_through_json_uses_camel_case() {
        let settings = ShellSettings {
            appearance: "dark".into(),
            locale: "en".into(),
            accent_color: "#336699".into(),
            density: "compact".into(),
            navigation_mode: "expanded".into(),
            background_variant: "grid".into(),
            reduced_motion: true,
        };
        let value = serde_json::to_value(&settings).expect("serialize");
        assert_eq!(value["accentColor"], "#336699");
        assert_eq!(value["navigationMode"], "expanded");
        assert_eq!(value["reducedMotion"], true);
    }
}
