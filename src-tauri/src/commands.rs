use crate::{
    app::{
        config::{self, LauncherSettings},
        state::AppState,
    },
    auth::{model::Account, service::AuthService},
    logging,
    minecraft::{
        installer::{self, ClientStatus},
        launcher::{self, LaunchStatus},
    },
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};
use tauri::{AppHandle, State};

#[derive(Debug, Serialize)]
pub struct LauncherSnapshot {
    pub accounts: Vec<Account>,
    pub active_account_id: Option<String>,
    pub settings: LauncherSettings,
    pub client: ClientStatus,
    pub launch: LaunchStatus,
}

#[derive(Debug, Deserialize)]
struct MojangProfile {
    properties: Vec<MojangProperty>,
}

#[derive(Debug, Deserialize)]
struct MojangProperty {
    name: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct TexturePayload {
    textures: HashMap<String, TextureEntry>,
}

#[derive(Debug, Deserialize)]
struct TextureEntry {
    url: String,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[derive(Debug, Serialize)]
pub struct PendingDesignImport {
    pub path: String,
    pub file_name: String,
    pub content: String,
}

#[tauri::command]
pub fn pending_design_import() -> Result<Option<PendingDesignImport>, String> {
    let candidate = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .find(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("designs9c"))
                .unwrap_or(false)
        });
    let Some(path) = candidate else {
        return Ok(None);
    };
    let metadata = std::fs::metadata(&path).map_err(err)?;
    if !metadata.is_file() || metadata.len() > 65_536 {
        return Err("invalid_design_file".to_string());
    }
    let content = std::fs::read_to_string(&path).map_err(err)?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("S9Lab.designs9c")
        .to_string();
    Ok(Some(PendingDesignImport {
        path: path.to_string_lossy().to_string(),
        file_name,
        content,
    }))
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.len() > 64 && bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
}

#[tauri::command]
pub async fn bootstrap(
    state: State<'_, AppState>,
    auth: State<'_, AuthService>,
) -> Result<LauncherSnapshot, String> {
    let auth_snapshot = auth.snapshot().map_err(err)?;
    Ok(LauncherSnapshot {
        accounts: auth_snapshot.accounts,
        active_account_id: auth_snapshot.active_account_id,
        settings: config::load_settings().map_err(err)?,
        client: installer::get_client_status().map_err(err)?,
        launch: launcher::get_status(state.inner()).await,
    })
}

#[tauri::command]
pub fn save_settings(settings: LauncherSettings) -> Result<LauncherSettings, String> {
    config::save_settings(&settings).map_err(err)
}

#[tauri::command]
pub fn get_client_status() -> Result<ClientStatus, String> {
    installer::get_client_status().map_err(err)
}

#[tauri::command]
pub async fn install_client(app: AppHandle, repair: bool) -> Result<ClientStatus, String> {
    installer::install_client(app, repair).await.map_err(err)
}

#[tauri::command]
pub async fn launch_client(
    app: AppHandle,
    state: State<'_, AppState>,
    auth: State<'_, AuthService>,
    account_id: String,
) -> Result<LaunchStatus, String> {
    launcher::launch_client(app, state.inner().clone(), auth.inner(), &account_id)
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn stop_client(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<LaunchStatus, String> {
    launcher::stop_client(&app, state.inner())
        .await
        .map_err(err)
}

#[tauri::command]
pub async fn get_launch_status(state: State<'_, AppState>) -> Result<LaunchStatus, String> {
    Ok(launcher::get_status(state.inner()).await)
}

#[tauri::command]
pub fn read_launcher_logs(limit: usize) -> Result<Vec<String>, String> {
    logging::read_last(limit).map_err(err)
}

#[tauri::command]
pub fn open_game_directory() -> Result<(), String> {
    let settings = config::load_settings().map_err(err)?;
    opener::open(settings.game_directory).map_err(err)
}

#[derive(Debug, Deserialize)]
struct AuthenticatedMinecraftProfile {
    #[serde(default)]
    skins: Vec<AuthenticatedSkin>,
}

#[derive(Debug, Deserialize)]
struct AuthenticatedSkin {
    url: String,
    #[serde(default)]
    state: Option<String>,
}

#[tauri::command]
pub async fn fetch_player_skin(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
) -> Result<String, String> {
    let compact_uuid: String = account_id
        .chars()
        .filter(|character| *character != '-')
        .collect();
    if compact_uuid.len() != 32
        || !compact_uuid
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("invalid_account_uuid".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(18))
        .user_agent(concat!("S9Lab-Launcher/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(err)?;

    // Primary source: authenticated Minecraft profile of the selected launcher account.
    // This avoids rendering a similarly named player and guarantees that the skin belongs
    // to the Microsoft/Minecraft account currently selected in the launcher.
    if let Ok((account, session)) = auth.ensure_minecraft_session(&account_id).await {
        let Some(access_token) = session.minecraft_access_token.as_deref() else {
            return Err("auth_minecraft_session_missing".to_string());
        };
        let response = client
            .get("https://api.minecraftservices.com/minecraft/profile")
            .bearer_auth(access_token)
            .send()
            .await;

        if let Ok(response) = response {
            if response.status().is_success() {
                if let Ok(profile) = response.json::<AuthenticatedMinecraftProfile>().await {
                    let selected = profile
                        .skins
                        .iter()
                        .find(|skin| skin.state.as_deref() == Some("ACTIVE"))
                        .or_else(|| profile.skins.first());
                    if let Some(skin) = selected {
                        if let Ok(texture_response) = client
                            .get(skin.url.replace("http://", "https://"))
                            .send()
                            .await
                        {
                            if texture_response.status().is_success() {
                                let bytes = texture_response.bytes().await.map_err(err)?;
                                if is_png(&bytes) {
                                    return Ok(format!(
                                        "data:image/png;base64,{}",
                                        STANDARD.encode(&bytes)
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        let _ = account;
    }

    // Public Mojang session server fallback by the exact Minecraft profile UUID.
    let profile_url =
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{compact_uuid}");
    if let Ok(response) = client.get(profile_url).send().await {
        if response.status().is_success() {
            if let Ok(profile) = response.json::<MojangProfile>().await {
                if let Some(property) = profile
                    .properties
                    .into_iter()
                    .find(|property| property.name == "textures")
                {
                    if let Ok(decoded) = STANDARD.decode(property.value) {
                        if let Ok(payload) = serde_json::from_slice::<TexturePayload>(&decoded) {
                            if let Some(texture) = payload.textures.get("SKIN") {
                                if let Ok(texture_response) = client
                                    .get(texture.url.replace("http://", "https://"))
                                    .send()
                                    .await
                                {
                                    if texture_response.status().is_success() {
                                        let bytes = texture_response.bytes().await.map_err(err)?;
                                        if is_png(&bytes) {
                                            return Ok(format!(
                                                "data:image/png;base64,{}",
                                                STANDARD.encode(&bytes)
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Last public fallbacks. UUID is preferred; username is only used last.
    let fallback_urls = [
        format!("https://mc-heads.net/skin/{compact_uuid}"),
        format!("https://crafatar.com/skins/{compact_uuid}"),
        format!("https://mc-heads.net/skin/{}", username.trim()),
    ];

    for url in fallback_urls {
        if let Ok(response) = client.get(url).send().await {
            if response.status().is_success() {
                if let Ok(bytes) = response.bytes().await {
                    if is_png(&bytes) {
                        return Ok(format!("data:image/png;base64,{}", STANDARD.encode(&bytes)));
                    }
                }
            }
        }
    }

    Err("skin_not_found".to_string())
}

#[tauri::command]
pub fn window_minimize(window: tauri::WebviewWindow) -> Result<(), String> {
    window.minimize().map_err(err)
}

#[tauri::command]
pub fn window_toggle_maximize(window: tauri::WebviewWindow) -> Result<(), String> {
    if window.is_maximized().map_err(err)? {
        window.unmaximize().map_err(err)
    } else {
        window.maximize().map_err(err)
    }
}

#[tauri::command]
pub fn window_close(window: tauri::WebviewWindow) -> Result<(), String> {
    window.close().map_err(err)
}

#[tauri::command]
pub fn window_start_dragging(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(err)
}
