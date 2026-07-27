use crate::{
    app::config::{self, ShellSettings},
    auth::{
        model::Account,
        service::{AuthService, AuthSnapshot, DeviceLoginPrompt},
    },
    error::{AppError, AppResult},
    foundation::CoreStatus,
    profiles::{model::ProfileSummary, service::ProfileService},
};
use serde::Serialize;

pub const IPC_CONTRACT_VERSION: u32 = 4;
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
