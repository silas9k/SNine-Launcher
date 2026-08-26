use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::State;

use crate::{
    auth::service::AuthService,
    snine_bridge::{backend_base_url, client, ensure_backend_session, invalidate_backend_session},
};

const MAX_CAPE_BYTES: usize = 1024 * 1024;
const CAPE_TEMPLATE_BYTES: &[u8] = include_bytes!("../resources/custom-cape-template.png");

fn is_cape_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

async fn session(
    auth: &AuthService,
    account_id: &str,
    username: &str,
) -> Result<crate::snine_bridge::SnineBackendSession, String> {
    ensure_backend_session(auth, account_id, username).await
}

fn data_url(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() || bytes.len() > MAX_CAPE_BYTES {
        return Err("custom_cape_texture_invalid".into());
    }
    Ok(format!("data:image/png;base64,{}", BASE64.encode(bytes)))
}

async fn authed_get_json(
    auth: &AuthService,
    account_id: &str,
    username: &str,
    route: &str,
) -> Result<Value, String> {
    let http = client()?;
    let base = backend_base_url();
    let mut current = session(auth, account_id, username).await?;
    for attempt in 0..2 {
        let response = http
            .get(format!("{base}{route}"))
            .header("X-SNine-Session", &current.token)
            .send()
            .await
            .map_err(|error| format!("custom_cape_request_failed:{error}"))?;
        if response.status().as_u16() == 401 && attempt == 0 {
            invalidate_backend_session(account_id).await;
            current = session(auth, account_id, username).await?;
            continue;
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("custom_cape_http_{status}:{body}"));
        }
        return response
            .json()
            .await
            .map_err(|error| format!("custom_cape_json_failed:{error}"));
    }
    Err("custom_cape_auth_failed".into())
}

async fn authed_get_bytes(
    auth: &AuthService,
    account_id: &str,
    username: &str,
    route: &str,
) -> Result<Vec<u8>, String> {
    let http = client()?;
    let base = backend_base_url();
    let mut current = session(auth, account_id, username).await?;
    for attempt in 0..2 {
        let response = http
            .get(format!("{base}{route}"))
            .header("X-SNine-Session", &current.token)
            .send()
            .await
            .map_err(|error| format!("custom_cape_request_failed:{error}"))?;
        if response.status().as_u16() == 401 && attempt == 0 {
            invalidate_backend_session(account_id).await;
            current = session(auth, account_id, username).await?;
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("custom_cape_http_{}", response.status().as_u16()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("custom_cape_read_failed:{error}"))?;
        if bytes.is_empty() || bytes.len() > MAX_CAPE_BYTES {
            return Err("custom_cape_texture_invalid".into());
        }
        return Ok(bytes.to_vec());
    }
    Err("custom_cape_auth_failed".into())
}

async fn authed_post_json(
    auth: &AuthService,
    account_id: &str,
    username: &str,
    route: &str,
    body: Value,
) -> Result<Value, String> {
    let http = client()?;
    let base = backend_base_url();
    let mut current = session(auth, account_id, username).await?;
    for attempt in 0..2 {
        let response = http
            .post(format!("{base}{route}"))
            .header("X-SNine-Session", &current.token)
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("custom_cape_request_failed:{error}"))?;
        if response.status().as_u16() == 401 && attempt == 0 {
            invalidate_backend_session(account_id).await;
            current = session(auth, account_id, username).await?;
            continue;
        }
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let payload: Value = response.json().await.unwrap_or_else(|_| json!({}));
            let code = payload
                .get("error")
                .and_then(Value::as_str)
                .or_else(|| payload.get("message").and_then(Value::as_str))
                .unwrap_or("request_failed");
            return Err(format!("custom_cape_http_{status}:{code}"));
        }
        return response
            .json()
            .await
            .map_err(|error| format!("custom_cape_json_failed:{error}"));
    }
    Err("custom_cape_auth_failed".into())
}

#[tauri::command]
pub async fn snine_launcher_custom_capes(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
    scope: String,
    search: String,
) -> Result<Value, String> {
    let mut url = reqwest::Url::parse(&format!("{}/custom-capes", backend_base_url()))
        .map_err(|error| format!("custom_cape_url_invalid:{error}"))?;
    url.query_pairs_mut()
        .append_pair("scope", scope.trim())
        .append_pair("search", search.trim());
    let query = url.query().unwrap_or_default();
    authed_get_json(
        auth.inner(),
        &account_id,
        &username,
        &format!("/custom-capes?{query}"),
    )
    .await
}

#[tauri::command]
pub async fn snine_launcher_custom_cape_upload(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
    cape_name: String,
    template: String,
    image_base64: String,
) -> Result<Value, String> {
    if image_base64.len() > MAX_CAPE_BYTES * 2 + 128 {
        return Err("custom_cape_too_large".into());
    }
    authed_post_json(
        auth.inner(),
        &account_id,
        &username,
        "/custom-capes/upload",
        json!({ "capeName": cape_name, "template": template, "imageBase64": image_base64 }),
    )
    .await
}

#[tauri::command]
pub async fn snine_launcher_custom_cape_favorite(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
    cape_id: String,
    favorite: bool,
) -> Result<Value, String> {
    authed_post_json(
        auth.inner(),
        &account_id,
        &username,
        "/custom-capes/favorite",
        json!({ "capeId": cape_id, "favorite": favorite }),
    )
    .await
}

#[tauri::command]
pub async fn snine_launcher_custom_cape_equip(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
    cape_id: String,
) -> Result<Value, String> {
    authed_post_json(
        auth.inner(),
        &account_id,
        &username,
        "/custom-capes/equip",
        json!({ "capeId": cape_id }),
    )
    .await
}

#[tauri::command]
pub async fn snine_launcher_custom_cape_unequip(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
) -> Result<Value, String> {
    authed_post_json(
        auth.inner(),
        &account_id,
        &username,
        "/custom-capes/unequip",
        json!({}),
    )
    .await
}

#[tauri::command]
pub async fn snine_launcher_custom_cape_preview(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
    cape_id: String,
) -> Result<String, String> {
    let id = cape_id.trim();
    if !is_cape_id(id) {
        return Err("custom_cape_id_invalid".into());
    }
    let bytes = authed_get_bytes(
        auth.inner(),
        &account_id,
        &username,
        &format!("/custom-capes/{id}/preview"),
    )
    .await?;
    data_url(&bytes)
}

#[tauri::command]
pub async fn snine_launcher_custom_cape_texture(cape_id: String) -> Result<String, String> {
    let id = cape_id.trim();
    if !is_cape_id(id) {
        return Err("custom_cape_id_invalid".into());
    }
    let http = client()?;
    let response = http
        .get(format!("{}/custom-capes/{id}/texture", backend_base_url()))
        .send()
        .await
        .map_err(|error| format!("custom_cape_texture_failed:{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "custom_cape_texture_http_{}",
            response.status().as_u16()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("custom_cape_texture_read_failed:{error}"))?;
    data_url(&bytes)
}

fn next_template_path() -> Result<PathBuf, String> {
    let directory = dirs::download_dir()
        .ok_or_else(|| "cape_template_download_directory_unavailable".to_string())?;
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("cape_template_download_directory_failed:{error}"))?;
    let preferred = directory.join("SNine-Cape-Template-512x256.png");
    if !preferred.exists() {
        return Ok(preferred);
    }
    for index in 2..=99 {
        let candidate = directory.join(format!("SNine-Cape-Template-512x256 ({index}).png"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("cape_template_download_name_exhausted".into())
}

#[tauri::command]
pub async fn snine_launcher_save_cape_template() -> Result<String, String> {
    let target = next_template_path()?;
    tokio::fs::write(&target, CAPE_TEMPLATE_BYTES)
        .await
        .map_err(|error| format!("cape_template_download_failed:{error}"))?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn snine_launcher_vanilla_capes(
    auth: State<'_, AuthService>,
    account_id: String,
) -> Result<Value, String> {
    let (account, session) = auth
        .ensure_minecraft_session(&account_id)
        .await
        .map_err(|error| format!("minecraft_session_failed:{}", error.descriptor().code))?;
    let token = session
        .minecraft_access_token
        .ok_or_else(|| "minecraft_access_token_missing".to_string())?;
    let http = client()?;
    let response = http
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| format!("minecraft_profile_request_failed:{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "minecraft_profile_http_{}",
            response.status().as_u16()
        ));
    }
    let profile: Value = response
        .json()
        .await
        .map_err(|error| format!("minecraft_profile_json_failed:{error}"))?;
    let mut result = Vec::new();
    if let Some(capes) = profile.get("capes").and_then(Value::as_array) {
        for cape in capes {
            let id = cape
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let url = cape
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if id.is_empty() || url.is_empty() {
                continue;
            }
            let state = cape
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("INACTIVE")
                .to_string();
            let alias = cape
                .get("alias")
                .and_then(Value::as_str)
                .unwrap_or("Vanilla Cape")
                .to_string();
            let texture_data_url = match http.get(&url).send().await {
                Ok(image_response) if image_response.status().is_success() => {
                    match image_response.bytes().await {
                        Ok(bytes) if bytes.len() <= MAX_CAPE_BYTES => data_url(&bytes).ok(),
                        _ => None,
                    }
                }
                _ => None,
            };
            result.push(json!({ "id": id, "name": alias, "state": state, "textureDataUrl": texture_data_url }));
        }
    }
    Ok(json!({ "ok": true, "playerName": account.username, "capes": result }))
}
