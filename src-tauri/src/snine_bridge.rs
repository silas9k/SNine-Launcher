use base64::{
    engine::general_purpose::{STANDARD as BASE64, STANDARD_NO_PAD as BASE64_NO_PAD},
    Engine as _,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};
use tauri::State;

use crate::{auth::service::AuthService, foundation::CoreServices};

const BACKEND_ENDPOINTS: &str = include_str!("../resources/backend-endpoints.json");
const PINNED_BACKEND_CERTIFICATE: &[u8] = include_bytes!("../resources/backend-cert.crt");
const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherCosmeticAsset {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub texture_data_url: Option<String>,
    pub model: Option<Value>,
    pub definition: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherLiveSync {
    pub account_id: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherCosmeticSnapshot {
    pub ok: bool,
    pub player_name: String,
    pub online: bool,
    #[serde(default)]
    pub badge_icon: String,
    #[serde(default)]
    pub plus_active: bool,
    pub equipped: Vec<LauncherCosmeticAsset>,
    pub source: String,
    pub status_message: String,
    #[serde(default)]
    pub live_sync: Option<LauncherLiveSync>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherLiveState {
    pub ok: bool,
    pub online: bool,
    #[serde(default)]
    pub badge_icon: String,
    #[serde(default)]
    pub plus_active: bool,
    pub equipped_cosmetics: BTreeMap<String, String>,
    pub status_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSkinSnapshot {
    pub ok: bool,
    pub player_name: String,
    pub texture_data_url: Option<String>,
    pub model: String,
    pub source: String,
    pub status_message: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SnineBackendSession {
    pub token: String,
    pub uuid: String,
}

#[derive(Debug, Clone)]
struct CachedSnineBackendSession {
    session: SnineBackendSession,
    valid_until: Instant,
}

static BACKEND_SESSIONS: OnceLock<tokio::sync::Mutex<HashMap<String, CachedSnineBackendSession>>> =
    OnceLock::new();

fn backend_sessions() -> &'static tokio::sync::Mutex<HashMap<String, CachedSnineBackendSession>> {
    BACKEND_SESSIONS.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
}

pub(crate) async fn invalidate_backend_session(account_id: &str) {
    backend_sessions()
        .lock()
        .await
        .remove(&account_id.to_ascii_lowercase());
}

pub(crate) async fn ensure_backend_session(
    auth: &AuthService,
    account_id: &str,
    username: &str,
) -> Result<SnineBackendSession, String> {
    let cache_key = account_id.trim().to_ascii_lowercase();
    {
        let cache = backend_sessions().lock().await;
        if let Some(cached) = cache.get(&cache_key) {
            if cached.valid_until > Instant::now() {
                return Ok(cached.session.clone());
            }
        }
    }

    let requested_uuid = dashed_uuid(account_id)?;
    let (account, minecraft_session) = auth
        .ensure_minecraft_session(account_id)
        .await
        .map_err(|error| format!("minecraft_session_failed:{}", error.descriptor().code))?;
    let access_token = minecraft_session
        .minecraft_access_token
        .ok_or_else(|| "minecraft_access_token_missing".to_string())?;
    let requested_name = if account.username.trim().is_empty() {
        username.trim()
    } else {
        account.username.trim()
    };
    if safe_minecraft_username(requested_name).is_none() {
        return Err("invalid_minecraft_username".into());
    }

    let http = client()?;
    let base = backend_base_url();
    let challenge_response = http
        .post(format!("{base}/handshake/challenge"))
        .json(&json!({
            "uuid": requested_uuid,
            "name": requested_name,
            "clientVersion": format!("SNine Launcher {}", env!("CARGO_PKG_VERSION")),
        }))
        .send()
        .await
        .map_err(|error| format!("snine_handshake_challenge_failed:{error}"))?;
    if !challenge_response.status().is_success() {
        return Err(format!(
            "snine_handshake_challenge_http_{}",
            challenge_response.status().as_u16()
        ));
    }
    let challenge: Value = challenge_response
        .json()
        .await
        .map_err(|error| format!("snine_handshake_challenge_json_failed:{error}"))?;
    let challenge_id = challenge
        .get("challengeId")
        .and_then(Value::as_str)
        .ok_or_else(|| "snine_handshake_challenge_id_missing".to_string())?;
    let server_id = challenge
        .get("serverId")
        .and_then(Value::as_str)
        .ok_or_else(|| "snine_handshake_server_id_missing".to_string())?;

    let join_response = http
        .post("https://sessionserver.mojang.com/session/minecraft/join")
        .json(&json!({
            "accessToken": access_token,
            "selectedProfile": compact_uuid(&requested_uuid)?,
            "serverId": server_id,
        }))
        .send()
        .await
        .map_err(|error| format!("minecraft_session_join_failed:{error}"))?;
    if join_response.status().as_u16() != 204 {
        return Err(format!(
            "minecraft_session_join_http_{}",
            join_response.status().as_u16()
        ));
    }

    let complete_response = http
        .post(format!("{base}/handshake/complete"))
        .json(&json!({
            "challengeId": challenge_id,
            "uuid": requested_uuid,
            "name": requested_name,
            "clientVersion": format!("SNine Launcher {}", env!("CARGO_PKG_VERSION")),
        }))
        .send()
        .await
        .map_err(|error| format!("snine_handshake_complete_failed:{error}"))?;
    if !complete_response.status().is_success() {
        return Err(format!(
            "snine_handshake_complete_http_{}",
            complete_response.status().as_u16()
        ));
    }
    let profile: Value = complete_response
        .json()
        .await
        .map_err(|error| format!("snine_handshake_complete_json_failed:{error}"))?;
    if !profile.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return Err("snine_handshake_rejected".into());
    }
    let token = profile
        .get("sessionToken")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if token.is_empty() {
        return Err("snine_session_token_missing".into());
    }
    let session = SnineBackendSession {
        token,
        uuid: profile
            .get("uuid")
            .and_then(Value::as_str)
            .unwrap_or(&requested_uuid)
            .to_string(),
    };
    backend_sessions().lock().await.insert(
        cache_key,
        CachedSnineBackendSession {
            session: session.clone(),
            valid_until: Instant::now() + Duration::from_secs(30 * 60),
        },
    );
    Ok(session)
}

fn insecure_local_backend_allowed() -> bool {
    std::env::var("SNINE_ALLOW_INSECURE_LOCAL_BACKEND")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
}

fn validate_backend_endpoint(value: &str, websocket: bool) -> Option<String> {
    let value = value.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(value).ok()?;
    let scheme = parsed.scheme();
    let secure = if websocket {
        scheme == "wss"
    } else {
        scheme == "https"
    };
    if secure {
        return Some(value.to_string());
    }
    let local_scheme = if websocket {
        scheme == "ws"
    } else {
        scheme == "http"
    };
    let local_host = parsed
        .host_str()
        .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
    (local_scheme && local_host && insecure_local_backend_allowed()).then(|| value.to_string())
}

fn packaged_backend_endpoint(key: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(BACKEND_ENDPOINTS).ok()?;
    parsed.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn backend_base_url() -> String {
    std::env::var("SNINE_BACKEND_BASE_URL")
        .ok()
        .and_then(|value| validate_backend_endpoint(&value, false))
        .or_else(|| {
            packaged_backend_endpoint("api")
                .and_then(|value| validate_backend_endpoint(&value, false))
        })
        .expect("SNine backend API endpoint is missing or insecure")
}

fn compact_uuid(value: &str) -> Result<String, String> {
    let compact: String = value
        .chars()
        .filter(|character| *character != '-')
        .collect();
    if compact.len() != 32
        || !compact
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err("invalid_minecraft_uuid".into());
    }
    Ok(compact.to_ascii_lowercase())
}

fn dashed_uuid(value: &str) -> Result<String, String> {
    let compact = compact_uuid(value)?;
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &compact[0..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20],
        &compact[20..32]
    ))
}

pub(crate) fn client() -> Result<reqwest::Client, String> {
    let certificate = reqwest::Certificate::from_pem(PINNED_BACKEND_CERTIFICATE)
        .map_err(|error| format!("snine_backend_certificate_invalid:{error}"))?;
    reqwest::Client::builder()
        .add_root_certificate(certificate)
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(12))
        .user_agent(format!("SNine-Launcher/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("http_client_failed:{error}"))
}

fn png_data_url(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
        return None;
    }
    Some(format!("data:image/png;base64,{}", BASE64.encode(bytes)))
}

fn safe_profile_id(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() || value.len() > 128 {
        return None;
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Some(value)
    } else {
        None
    }
}

fn runtime_pack_root(core: &CoreServices, profile_id: &str) -> Option<PathBuf> {
    let profile_id = safe_profile_id(profile_id)?;
    let root = core
        .paths()
        .profiles
        .join(profile_id)
        .join("instance")
        .join("resourcepacks")
        .join("snine-cosmetics-runtime");
    root.is_dir().then_some(root)
}

fn cache_path(core: &CoreServices, account_id: &str) -> Result<PathBuf, String> {
    let compact = compact_uuid(account_id)?;
    Ok(core
        .paths()
        .data
        .join(format!("snine-cosmetics-{compact}.json")))
}

fn read_cached_snapshot(core: &CoreServices, account_id: &str) -> Option<LauncherCosmeticSnapshot> {
    let path = cache_path(core, account_id).ok()?;
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_cached_snapshot(
    core: &CoreServices,
    account_id: &str,
    snapshot: &LauncherCosmeticSnapshot,
) {
    let Ok(path) = cache_path(core, account_id) else {
        return;
    };
    // Backend session tokens are short-lived credentials and must never be persisted.
    let mut cached = snapshot.clone();
    cached.live_sync = None;
    let Ok(bytes) = serde_json::to_vec(&cached) else {
        return;
    };
    let _ = fs::create_dir_all(core.paths().data.as_path());
    let temporary = path.with_extension("tmp");
    if fs::write(&temporary, bytes).is_ok() {
        let _ = fs::rename(&temporary, &path).or_else(|_| {
            let _ = fs::remove_file(&path);
            fs::rename(&temporary, &path)
        });
    }
}

fn skin_cache_path(core: &CoreServices, account_id: &str) -> Result<PathBuf, String> {
    let compact = compact_uuid(account_id)?;
    Ok(core.paths().data.join(format!("snine-skin-{compact}.json")))
}

fn read_cached_skin(core: &CoreServices, account_id: &str) -> Option<LauncherSkinSnapshot> {
    let path = skin_cache_path(core, account_id).ok()?;
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_cached_skin(core: &CoreServices, account_id: &str, snapshot: &LauncherSkinSnapshot) {
    let Ok(path) = skin_cache_path(core, account_id) else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec(snapshot) else {
        return;
    };
    let _ = fs::create_dir_all(core.paths().data.as_path());
    let temporary = path.with_extension("tmp");
    if fs::write(&temporary, bytes).is_ok() {
        let _ = fs::rename(&temporary, &path).or_else(|_| {
            let _ = fs::remove_file(&path);
            fs::rename(&temporary, &path)
        });
    }
}

fn skin_failure(
    core: &CoreServices,
    account_id: &str,
    username: &str,
    status: String,
) -> LauncherSkinSnapshot {
    if let Some(mut cached) = read_cached_skin(core, account_id) {
        if cached.player_name.eq_ignore_ascii_case(username) {
            cached.ok = true;
            cached.source = "launcher-cache".into();
            cached.status_message = format!("{status}_using_cache");
            return cached;
        }
    }
    LauncherSkinSnapshot {
        ok: false,
        player_name: username.to_string(),
        texture_data_url: None,
        model: "classic".into(),
        source: "mojang".into(),
        status_message: status,
    }
}

fn is_minecraft_texture_url(value: &str) -> bool {
    value.starts_with("https://textures.minecraft.net/")
}

fn is_png_bytes(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
}

fn safe_skin_proxy_identity(value: &str) -> Option<String> {
    if let Ok(compact) = compact_uuid(value) {
        return Some(compact);
    }
    safe_minecraft_username(value).map(ToOwned::to_owned)
}

async fn fetch_skin_png(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let response = http
        .get(url)
        .send()
        .await
        .map_err(|error| format!("minecraft_skin_download_failed:{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "minecraft_skin_http_{}",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length == 0 || length > MAX_ASSET_BYTES as u64)
    {
        return Err("minecraft_skin_size_invalid".into());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("minecraft_skin_read_failed:{error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES || !is_png_bytes(&bytes) {
        return Err("minecraft_skin_not_png".into());
    }
    Ok(bytes.to_vec())
}

fn safe_minecraft_username(value: &str) -> Option<&str> {
    let value = value.trim();
    if !(3..=16).contains(&value.len()) {
        return None;
    }
    value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
        .then_some(value)
}

async fn download_skin_snapshot(
    http: &reqwest::Client,
    core: &CoreServices,
    cache_account_id: &str,
    player_name: String,
    skin_url: &str,
    model: String,
    source: &str,
    status_message: &str,
    proxy_identity: Option<&str>,
) -> Result<LauncherSkinSnapshot, String> {
    let normalized_skin_url = skin_url
        .strip_prefix("http://textures.minecraft.net/")
        .map(|path| format!("https://textures.minecraft.net/{path}"))
        .unwrap_or_else(|| skin_url.to_string());
    if !is_minecraft_texture_url(&normalized_skin_url) {
        return Err("minecraft_skin_url_invalid".into());
    }

    let mut resolved_source = source.to_string();
    let mut resolved_status = status_message.to_string();
    let bytes = match fetch_skin_png(http, &normalized_skin_url).await {
        Ok(bytes) => bytes,
        Err(primary_error) => {
            let identity = proxy_identity
                .and_then(safe_skin_proxy_identity)
                .ok_or(primary_error.clone())?;
            let proxy_url = format!("https://mc-heads.net/skin/{identity}");
            match fetch_skin_png(http, &proxy_url).await {
                Ok(bytes) => {
                    resolved_source = format!("{source}+server-side-skin-proxy");
                    resolved_status = format!("{status_message}_proxy_fallback");
                    bytes
                }
                Err(proxy_error) => {
                    return Err(format!("{primary_error}|skin_proxy:{proxy_error}"));
                }
            }
        }
    };

    let texture_data_url =
        png_data_url(&bytes).ok_or_else(|| "minecraft_skin_bytes_invalid".to_string())?;
    let snapshot = LauncherSkinSnapshot {
        ok: true,
        player_name,
        texture_data_url: Some(texture_data_url),
        model: if model.eq_ignore_ascii_case("slim") {
            "slim".into()
        } else {
            "classic".into()
        },
        source: resolved_source,
        status_message: resolved_status,
    };
    write_cached_skin(core, cache_account_id, &snapshot);
    Ok(snapshot)
}

async fn skin_from_server_side_proxy(
    http: &reqwest::Client,
    core: &CoreServices,
    account_id: &str,
    username: &str,
) -> Result<LauncherSkinSnapshot, String> {
    let mut candidates = Vec::new();
    if let Some(identity) = safe_skin_proxy_identity(account_id) {
        candidates.push(identity.to_string());
    }
    if let Some(identity) = safe_skin_proxy_identity(username) {
        if !candidates
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&identity))
        {
            candidates.push(identity.to_string());
        }
    }
    if candidates.is_empty() {
        return Err("skin_proxy_identity_invalid".into());
    }

    let mut failures = Vec::new();
    for identity in candidates {
        let url = format!("https://mc-heads.net/skin/{identity}");
        match fetch_skin_png(http, &url).await {
            Ok(bytes) => {
                let texture_data_url = png_data_url(&bytes)
                    .ok_or_else(|| "minecraft_skin_bytes_invalid".to_string())?;
                let cached_model = read_cached_skin(core, account_id)
                    .filter(|cached| cached.player_name.eq_ignore_ascii_case(username))
                    .map(|cached| cached.model)
                    .unwrap_or_else(|| "classic".into());
                let snapshot = LauncherSkinSnapshot {
                    ok: true,
                    player_name: username.to_string(),
                    texture_data_url: Some(texture_data_url),
                    model: cached_model,
                    source: format!("server-side-skin-proxy:{identity}"),
                    status_message: "official_skin_proxy_fallback".into(),
                };
                write_cached_skin(core, account_id, &snapshot);
                return Ok(snapshot);
            }
            Err(error) => failures.push(format!("{identity}:{error}")),
        }
    }

    Err(format!(
        "skin_proxy_all_sources_failed:{}",
        failures.join("|")
    ))
}

async fn skin_from_authenticated_profile(
    http: &reqwest::Client,
    auth: &AuthService,
    core: &CoreServices,
    account_id: &str,
    username: &str,
) -> Result<LauncherSkinSnapshot, String> {
    let (account, session) = auth
        .ensure_minecraft_session(account_id)
        .await
        .map_err(|error| format!("minecraft_session_failed:{}", error.descriptor().code))?;
    let access_token = session
        .minecraft_access_token
        .ok_or_else(|| "minecraft_access_token_missing".to_string())?;
    let response = http
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access_token)
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
    let player_name = profile
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(if account.username.is_empty() {
            username
        } else {
            &account.username
        })
        .to_string();
    let skins = profile
        .get("skins")
        .and_then(Value::as_array)
        .ok_or_else(|| "minecraft_profile_skins_missing".to_string())?;
    let skin = skins
        .iter()
        .find(|entry| {
            entry
                .get("state")
                .and_then(Value::as_str)
                .is_some_and(|state| state.eq_ignore_ascii_case("active"))
        })
        .or_else(|| skins.first())
        .ok_or_else(|| "minecraft_profile_skin_missing".to_string())?;
    let skin_url = skin
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "minecraft_profile_skin_url_missing".to_string())?;
    let model = skin
        .get("variant")
        .and_then(Value::as_str)
        .unwrap_or("classic")
        .to_ascii_lowercase();
    download_skin_snapshot(
        http,
        core,
        account_id,
        player_name,
        skin_url,
        model,
        "minecraft-services-authenticated",
        "official_authenticated_minecraft_skin",
        Some(account_id),
    )
    .await
}

async fn skin_from_session_server(
    http: &reqwest::Client,
    core: &CoreServices,
    cache_account_id: &str,
    profile_id: &str,
    username: &str,
    source: &str,
) -> Result<LauncherSkinSnapshot, String> {
    let compact = compact_uuid(profile_id)?;
    let profile_url = format!(
        "https://sessionserver.mojang.com/session/minecraft/profile/{compact}?unsigned=false"
    );
    let response = http
        .get(&profile_url)
        .send()
        .await
        .map_err(|error| format!("mojang_profile_request_failed:{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "mojang_profile_http_{}",
            response.status().as_u16()
        ));
    }
    let profile: Value = response
        .json()
        .await
        .map_err(|error| format!("mojang_profile_json_failed:{error}"))?;
    let player_name = profile
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(username)
        .to_string();
    let texture_property = profile
        .get("properties")
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(Value::as_str) == Some("textures"))
        })
        .and_then(|item| item.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| "minecraft_skin_property_missing".to_string())?;
    let decoded = BASE64
        .decode(texture_property)
        .or_else(|_| BASE64_NO_PAD.decode(texture_property))
        .map_err(|error| format!("minecraft_skin_property_decode_failed:{error}"))?;
    let textures: Value = serde_json::from_slice(&decoded)
        .map_err(|error| format!("minecraft_skin_property_json_failed:{error}"))?;
    let skin = textures
        .get("textures")
        .and_then(|value| value.get("SKIN"))
        .ok_or_else(|| "minecraft_skin_missing".to_string())?;
    let skin_url = skin
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| "minecraft_skin_url_missing".to_string())?;
    let model = skin
        .get("metadata")
        .and_then(|value| value.get("model"))
        .and_then(Value::as_str)
        .unwrap_or("classic")
        .to_ascii_lowercase();
    download_skin_snapshot(
        http,
        core,
        cache_account_id,
        player_name,
        skin_url,
        model,
        source,
        "official_mojang_skin",
        Some(profile_id),
    )
    .await
}

async fn resolve_uuid_by_username(
    http: &reqwest::Client,
    username: &str,
) -> Result<String, String> {
    let username = safe_minecraft_username(username)
        .ok_or_else(|| "minecraft_username_invalid".to_string())?;
    let response = http
        .get(format!(
            "https://api.mojang.com/users/profiles/minecraft/{username}"
        ))
        .send()
        .await
        .map_err(|error| format!("minecraft_username_lookup_failed:{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "minecraft_username_lookup_http_{}",
            response.status().as_u16()
        ));
    }
    let profile: Value = response
        .json()
        .await
        .map_err(|error| format!("minecraft_username_lookup_json_failed:{error}"))?;
    let id = profile
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "minecraft_username_lookup_id_missing".to_string())?;
    compact_uuid(id)
}

#[tauri::command]
pub async fn snine_launcher_player_skin(
    core: State<'_, CoreServices>,
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
) -> Result<LauncherSkinSnapshot, String> {
    let http = client()?;
    let mut failures = Vec::new();

    // First use the authenticated Minecraft session stored by this launcher.
    // This is the authoritative active skin for the selected Microsoft/Minecraft account.
    match skin_from_authenticated_profile(&http, auth.inner(), core.inner(), &account_id, &username)
        .await
    {
        Ok(snapshot) => return Ok(snapshot),
        Err(error) => failures.push(format!("authenticated:{error}")),
    }

    // Then resolve the visible Minecraft name. This protects the preview from any
    // stale/mismatched local profile assignment and still resolves the real Mojang UUID.
    match resolve_uuid_by_username(&http, &username).await {
        Ok(resolved_id) => match skin_from_session_server(
            &http,
            core.inner(),
            &account_id,
            &resolved_id,
            &username,
            "mojang-session-username-resolved",
        )
        .await
        {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) => failures.push(format!("username_uuid:{error}")),
        },
        Err(error) => failures.push(format!("username_lookup:{error}")),
    }

    // Finally use the account id recorded by the launcher directly.
    match skin_from_session_server(
        &http,
        core.inner(),
        &account_id,
        &account_id,
        &username,
        "mojang-session-account-id",
    )
    .await
    {
        Ok(snapshot) => return Ok(snapshot),
        Err(error) => failures.push(format!("account_uuid:{error}")),
    }

    // Raw skin proxy remains server-side only; the WebView never performs a cross-origin image request.
    match skin_from_server_side_proxy(&http, core.inner(), &account_id, &username).await {
        Ok(snapshot) => return Ok(snapshot),
        Err(error) => failures.push(format!("skin_proxy:{error}")),
    }

    Ok(skin_failure(
        core.inner(),
        &account_id,
        &username,
        format!("minecraft_skin_all_sources_failed:{}", failures.join("|")),
    ))
}

#[tauri::command]
pub async fn snine_launcher_import_skin(
    core: State<'_, CoreServices>,
    reference: String,
) -> Result<LauncherSkinSnapshot, String> {
    let reference = reference.trim();
    let http = client()?;
    let profile_id = match compact_uuid(reference) {
        Ok(uuid) => uuid,
        Err(_) => resolve_uuid_by_username(&http, reference).await?,
    };

    skin_from_session_server(
        &http,
        core.inner(),
        &profile_id,
        &profile_id,
        reference,
        "mojang-session-library-import",
    )
    .await
}

fn catalog_name(kind: &str) -> Option<&'static str> {
    match kind.to_ascii_lowercase().as_str() {
        "cape" => Some("capes.json"),
        "bandana" => Some("bandanas.json"),
        "wings" => Some("wings.json"),
        "hat" => Some("hats.json"),
        "armor" => Some("armors.json"),
        "chestplate" => Some("chestplates.json"),
        "pants" => Some("pants.json"),
        "shoes" => Some("shoes.json"),
        "accessory" => Some("accessories.json"),
        "halo" => Some("halos.json"),
        "shield" => Some("shields.json"),
        "pets" | "pet" => Some("pets.json"),
        "glint" => Some("glints.json"),
        "emote" => Some("emotes.json"),
        _ => None,
    }
}

fn pack_path(resource: &str) -> Option<String> {
    let trimmed = resource.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("assets/") {
        return Some(trimmed.to_string());
    }
    let without_namespace = trimmed
        .strip_prefix("snineclient:")
        .or_else(|| trimmed.strip_prefix("minecraft:"))
        .unwrap_or(trimmed);
    Some(format!("assets/snineclient/{without_namespace}"))
}

fn entry_string<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn fetch_object(
    http: &reqwest::Client,
    base: &str,
    files: &Map<String, Value>,
    path: &str,
) -> Result<Vec<u8>, String> {
    let metadata = files
        .get(path)
        .ok_or_else(|| format!("missing_runtime_asset:{path}"))?;
    let hash = metadata
        .get("sha256")
        .and_then(Value::as_str)
        .filter(|value| value.len() == 64)
        .ok_or_else(|| format!("invalid_runtime_hash:{path}"))?;
    let declared_size = metadata.get("size").and_then(Value::as_u64).unwrap_or(0) as usize;
    if declared_size == 0 || declared_size > MAX_ASSET_BYTES {
        return Err(format!("invalid_runtime_asset_size:{path}"));
    }
    let response = http
        .get(format!("{base}/cosmetic-content/object/{hash}"))
        .send()
        .await
        .map_err(|error| format!("runtime_asset_request_failed:{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "runtime_asset_http_{}:{path}",
            response.status().as_u16()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("runtime_asset_read_failed:{error}"))?;
    if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES {
        return Err(format!("runtime_asset_size_mismatch:{path}"));
    }
    Ok(bytes.to_vec())
}

fn read_local_pack(root: &Path, path: &str) -> Option<Vec<u8>> {
    if path.is_empty() || path.starts_with('/') || path.contains("..") || path.contains('\\') {
        return None;
    }
    let canonical_root = fs::canonicalize(root).ok()?;
    let target = fs::canonicalize(root.join(path)).ok()?;
    if !target.starts_with(&canonical_root) {
        return None;
    }
    let metadata = fs::metadata(&target).ok()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ASSET_BYTES as u64 {
        return None;
    }
    fs::read(target).ok()
}

fn local_catalog_entry(pack_root: &Path, kind: &str, cosmetic_id: &str) -> Option<Value> {
    let catalog = catalog_name(kind)?;
    let path = pack_root
        .join("assets/snineclient/snine_external/catalogs")
        .join(catalog);
    let root: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let entries = root
        .as_array()
        .cloned()
        .or_else(|| root.get("cosmetics").and_then(Value::as_array).cloned())?;
    entries.into_iter().find(|entry| {
        entry
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.eq_ignore_ascii_case(cosmetic_id))
    })
}

async fn remote_catalog_entry(
    http: &reqwest::Client,
    base: &str,
    files: &Map<String, Value>,
    kind: &str,
    cosmetic_id: &str,
) -> Option<Value> {
    let catalog = catalog_name(kind)?;
    let path = format!("assets/snineclient/snine_external/catalogs/{catalog}");
    let bytes = fetch_object(http, base, files, &path).await.ok()?;
    let root: Value = serde_json::from_slice(&bytes).ok()?;
    let entries = root
        .as_array()
        .cloned()
        .or_else(|| root.get("cosmetics").and_then(Value::as_array).cloned())?;
    entries.into_iter().find(|entry| {
        entry
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.eq_ignore_ascii_case(cosmetic_id))
    })
}

async fn build_assets(
    http: &reqwest::Client,
    base: &str,
    files: Option<&Map<String, Value>>,
    local_pack: Option<&Path>,
    equipped_map: BTreeMap<String, String>,
) -> Vec<LauncherCosmeticAsset> {
    let mut assets = Vec::new();
    for (kind, id) in equipped_map {
        if kind == "emote" {
            continue;
        }

        let custom_cape = if kind == "cape" {
            id.strip_prefix("custom_cape_elytra:")
                .map(|cape_id| (cape_id.trim().to_string(), "CAPE_ELYTRA"))
                .or_else(|| {
                    id.strip_prefix("custom_cape:")
                        .map(|cape_id| (cape_id.trim().to_string(), "CAPE"))
                })
        } else {
            None
        };

        if let Some((cape_id, template)) = custom_cape {
            let texture_bytes = if cape_id.is_empty() {
                None
            } else {
                let url = format!(
                    "{}/custom-capes/{}/texture",
                    base.trim_end_matches('/'),
                    cape_id
                );
                match http.get(url).send().await {
                    Ok(response) if response.status().is_success() => {
                        response.bytes().await.ok().map(|bytes| bytes.to_vec())
                    }
                    _ => None,
                }
            };
            let definition = json!({
                "id": id.clone(),
                "type": kind.clone(),
                "name": "Custom Cape",
                "template": template,
                "source": "custom-cape",
                "capeId": cape_id,
            });
            assets.push(LauncherCosmeticAsset {
                id,
                kind,
                name: "Custom Cape".to_string(),
                texture_data_url: texture_bytes.as_deref().and_then(png_data_url),
                model: None,
                definition,
            });
            continue;
        }

        let definition = if let Some(files) = files {
            remote_catalog_entry(http, base, files, &kind, &id)
                .await
                .or_else(|| local_pack.and_then(|root| local_catalog_entry(root, &kind, &id)))
        } else {
            local_pack.and_then(|root| local_catalog_entry(root, &kind, &id))
        };
        let definition = definition.unwrap_or_else(|| {
            json!({
                "id": id.clone(),
                "type": kind.clone(),
                "name": id.clone(),
            })
        });
        let name = entry_string(&definition, "name").unwrap_or(&id).to_string();
        let texture_path = entry_string(&definition, "texture").and_then(pack_path);
        let model_path = entry_string(&definition, "model").and_then(pack_path);

        let texture_bytes = if let Some(path) = texture_path.as_deref() {
            if let Some(files) = files {
                fetch_object(http, base, files, path)
                    .await
                    .ok()
                    .or_else(|| local_pack.and_then(|root| read_local_pack(root, path)))
            } else {
                local_pack.and_then(|root| read_local_pack(root, path))
            }
        } else {
            None
        };
        let model_bytes = if let Some(path) = model_path.as_deref() {
            if let Some(files) = files {
                fetch_object(http, base, files, path)
                    .await
                    .ok()
                    .or_else(|| local_pack.and_then(|root| read_local_pack(root, path)))
            } else {
                local_pack.and_then(|root| read_local_pack(root, path))
            }
        } else {
            None
        };

        let model = model_bytes
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
        assets.push(LauncherCosmeticAsset {
            id,
            kind,
            name,
            texture_data_url: texture_bytes.as_deref().and_then(png_data_url),
            model,
            definition,
        });
    }
    assets
}

async fn merge_selected_custom_cape(
    http: &reqwest::Client,
    base: &str,
    session_token: &str,
    equipped: &mut BTreeMap<String, String>,
) {
    // The profile's equipped cape is authoritative. Older launcher builds always
    // overlaid the persisted custom-cape selection here, so a custom cape could keep
    // reappearing even after the player equipped a different normal cape in-game.
    if equipped
        .get("cape")
        .is_some_and(|value| !value.trim().is_empty())
    {
        return;
    }
    let response = match http
        .get(format!(
            "{}/custom-capes?scope=mine",
            base.trim_end_matches('/')
        ))
        .header("X-SNine-Session", session_token)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response,
        _ => return,
    };
    let payload: Value = match response.json().await {
        Ok(value) => value,
        Err(_) => return,
    };
    let Some(selected) = payload.get("selected").and_then(Value::as_object) else {
        return;
    };
    let Some(cape_id) = selected
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return;
    };
    let template = selected
        .get("template")
        .and_then(Value::as_str)
        .unwrap_or("CAPE");
    let prefix = if template.eq_ignore_ascii_case("CAPE_ELYTRA") {
        "custom_cape_elytra:"
    } else {
        "custom_cape:"
    };
    equipped.insert("cape".into(), format!("{prefix}{cape_id}"));
}

#[tauri::command]
pub async fn snine_launcher_live_state(
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
) -> Result<LauncherLiveState, String> {
    let session = ensure_backend_session(auth.inner(), &account_id, &username).await?;
    let base = backend_base_url();
    let http = client()?;
    let response = http
        .get(format!("{base}/profile/{}", session.uuid))
        .header("X-SNine-Session", &session.token)
        .send()
        .await
        .map_err(|error| format!("snine_live_request_failed:{error}"))?;

    if response.status().as_u16() == 401 {
        invalidate_backend_session(&account_id).await;
        return Err("snine_session_expired".into());
    }
    if !response.status().is_success() {
        return Err(format!("snine_live_http_{}", response.status().as_u16()));
    }

    let profile: Value = response
        .json()
        .await
        .map_err(|error| format!("snine_live_json_failed:{error}"))?;
    if !profile.get("ok").and_then(Value::as_bool).unwrap_or(true) {
        return Err("snine_live_profile_rejected".into());
    }

    let mut equipped_cosmetics: BTreeMap<String, String> = profile
        .get("equippedCosmetics")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(kind, value)| {
                    let id = value.as_str()?.trim();
                    (!id.is_empty()).then_some((kind.trim().to_ascii_lowercase(), id.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    merge_selected_custom_cape(&http, &base, &session.token, &mut equipped_cosmetics).await;

    Ok(LauncherLiveState {
        ok: true,
        online: profile
            .get("online")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        badge_icon: profile
            .get("badgeIcon")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        plus_active: profile
            .get("plusActive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        equipped_cosmetics,
        status_message: "live_profile_synced".into(),
    })
}

#[tauri::command]
pub async fn snine_launcher_resolve_cosmetics(
    core: State<'_, CoreServices>,
    equipped_cosmetics: BTreeMap<String, String>,
    profile_id: Option<String>,
) -> Result<Vec<LauncherCosmeticAsset>, String> {
    let base = backend_base_url();
    let http = client()?;
    let local_pack = profile_id
        .as_deref()
        .and_then(|id| runtime_pack_root(core.inner(), id));
    let normalized: BTreeMap<String, String> = equipped_cosmetics
        .into_iter()
        .filter_map(|(kind, id)| {
            let kind = kind.trim().to_ascii_lowercase();
            let id = id.trim().to_string();
            (!kind.is_empty() && !id.is_empty()).then_some((kind, id))
        })
        .collect();
    let index: Option<Value> = match http
        .get(format!("{base}/cosmetic-content/index"))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response.json().await.ok(),
        _ => None,
    };
    let files = index
        .as_ref()
        .and_then(|value| value.get("files"))
        .and_then(Value::as_object);
    Ok(build_assets(&http, &base, files, local_pack.as_deref(), normalized).await)
}

#[tauri::command]
pub async fn snine_launcher_cosmetics(
    core: State<'_, CoreServices>,
    auth: State<'_, AuthService>,
    account_id: String,
    username: String,
    profile_id: Option<String>,
    include_assets: Option<bool>,
) -> Result<LauncherCosmeticSnapshot, String> {
    let base = backend_base_url();
    let http = client()?;
    let local_pack = profile_id
        .as_deref()
        .and_then(|id| runtime_pack_root(core.inner(), id));
    let mut session = match ensure_backend_session(auth.inner(), &account_id, &username).await {
        Ok(session) => session,
        Err(error) => {
            if let Some(mut cached) = read_cached_snapshot(core.inner(), &account_id) {
                cached.ok = true;
                cached.online = false;
                cached.source = "launcher-cache".into();
                cached.status_message = format!("{error}_using_cache");
                return Ok(cached);
            }
            return Ok(LauncherCosmeticSnapshot {
                ok: false,
                player_name: username,
                online: false,
                badge_icon: String::new(),
                plus_active: false,
                equipped: Vec::new(),
                source: base,
                status_message: error,
                live_sync: None,
            });
        }
    };
    let mut profile_response = http
        .get(format!("{base}/profile/{}", session.uuid))
        .header("X-SNine-Session", &session.token)
        .send()
        .await
        .map_err(|error| format!("snine_profile_request_failed:{error}"))?;
    if profile_response.status().as_u16() == 401 {
        invalidate_backend_session(&account_id).await;
        session = ensure_backend_session(auth.inner(), &account_id, &username).await?;
        profile_response = http
            .get(format!("{base}/profile/{}", session.uuid))
            .header("X-SNine-Session", &session.token)
            .send()
            .await
            .map_err(|error| format!("snine_profile_retry_failed:{error}"))?;
    }
    if !profile_response.status().is_success() {
        return Ok(LauncherCosmeticSnapshot {
            ok: false,
            player_name: username,
            online: false,
            badge_icon: String::new(),
            plus_active: false,
            equipped: Vec::new(),
            source: base,
            status_message: format!("backend_http_{}", profile_response.status().as_u16()),
            live_sync: None,
        });
    }
    let profile: Value = profile_response
        .json()
        .await
        .map_err(|error| format!("snine_profile_json_failed:{error}"))?;
    let player_name = profile
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&username)
        .to_string();
    let online = profile
        .get("online")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let badge_icon = profile
        .get("badgeIcon")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let plus_active = profile
        .get("plusActive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut equipped_map: BTreeMap<String, String> = profile
        .get("equippedCosmetics")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(kind, value)| {
                    value
                        .as_str()
                        .map(|id| (kind.to_ascii_lowercase(), id.to_string()))
                })
                .filter(|(_, id)| !id.trim().is_empty())
                .collect()
        })
        .unwrap_or_default();
    merge_selected_custom_cape(&http, &base, &session.token, &mut equipped_map).await;

    let include_assets = include_assets.unwrap_or(true);
    let index: Option<Value> = if include_assets {
        match http
            .get(format!("{base}/cosmetic-content/index"))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => response.json().await.ok(),
            _ => None,
        }
    } else {
        None
    };
    let files = index
        .as_ref()
        .and_then(|value| value.get("files"))
        .and_then(Value::as_object);
    let assets = if include_assets {
        build_assets(&http, &base, files, local_pack.as_deref(), equipped_map).await
    } else {
        Vec::new()
    };
    let source = if files.is_some() {
        "snine-backend+runtime-pack"
    } else if local_pack.is_some() {
        "snine-backend+local-runtime-pack"
    } else {
        "snine-backend"
    };
    let live_sync = Some(LauncherLiveSync {
        account_id: account_id.clone(),
        username: username.clone(),
    });
    let snapshot = LauncherCosmeticSnapshot {
        ok: true,
        player_name,
        online,
        badge_icon,
        plus_active,
        equipped: assets,
        source: source.into(),
        status_message: if files.is_some() {
            "live_loadout_synced".into()
        } else if local_pack.is_some() {
            "live_loadout_with_local_assets".into()
        } else {
            "loadout_synced_assets_unavailable".into()
        },
        live_sync,
    };
    write_cached_snapshot(core.inner(), &account_id, &snapshot);
    Ok(snapshot)
}
