use crate::foundation::CoreServices;
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, ETAG, LAST_MODIFIED, RANGE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, State};
use tokio::io::AsyncWriteExt;

const SNINE_CLIENT_URL: &str = "https://s9lab.site/updates/snineclient.jar";
const SNINE_CLIENT_FALLBACK_URL: &str = "http://s9lab.site/updates/snineclient.jar";
const DOWNLOAD_EVENT: &str = "snine-client-download-progress";
const UPDATER_REVISION: &str = "v18-no-remote-size-gate";
const MAX_CLIENT_JAR_BYTES: u64 = 256 * 1024 * 1024;

const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";
const FABRIC_API_PROJECT_SLUG: &str = "fabric-api";
const GECKOLIB_PROJECT_SLUG: &str = "geckolib";
const OWO_LIB_PROJECT_SLUG: &str = "owo-lib";
const FABRIC_API_MOD_ID: &str = "fabric-api";
const GECKOLIB_MOD_ID: &str = "geckolib";
const OWO_LIB_MOD_ID: &str = "owo";
const COMPANION_MOD_MAX_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn remove_legacy_s9lab_client(mods_dir: &Path) -> Result<(), String> {
    let legacy = mods_dir.join("s9labclient.jar");
    if legacy.exists() {
        fs::remove_file(&legacy)
            .map_err(|error| format!("snine_remove_legacy_s9labclient_failed:{error}"))?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnineClientUpdateCheck {
    reachable: bool,
    update_available: bool,
    external_client_installed: bool,
    installed_version: Option<String>,
    remote_version: Option<String>,
    remote_size_bytes: Option<u64>,
    status_message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnineClientDownloadProgress {
    profile_id: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    percent: f64,
    stage: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnineClientDownloadResult {
    installed_version: Option<String>,
    sha256: String,
    size_bytes: u64,
    target_file: String,
}

#[derive(Debug, Clone, Default)]
struct RemoteMetadata {
    etag: Option<String>,
    last_modified: Option<String>,
    version: Option<String>,
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedClientMetadata {
    url: String,
    etag: Option<String>,
    last_modified: Option<String>,
    remote_version: Option<String>,
    size_bytes: Option<u64>,
    sha256: String,
    installed_version: Option<String>,
    downloaded_at_unix: i64,
}


#[derive(Debug, Clone, Copy)]
struct CompanionModSpec {
    project_slug: &'static str,
    mod_id: &'static str,
    display_name: &'static str,
}

const COMPANION_MODS: [CompanionModSpec; 3] = [
    CompanionModSpec {
        project_slug: FABRIC_API_PROJECT_SLUG,
        mod_id: FABRIC_API_MOD_ID,
        display_name: "Fabric API",
    },
    CompanionModSpec {
        project_slug: GECKOLIB_PROJECT_SLUG,
        mod_id: GECKOLIB_MOD_ID,
        display_name: "GeckoLib",
    },
    CompanionModSpec {
        project_slug: OWO_LIB_PROJECT_SLUG,
        mod_id: OWO_LIB_MOD_ID,
        display_name: "owo-lib",
    },
];

#[derive(Debug, Deserialize, Clone)]
struct ModrinthVersionFileWire {
    url: String,
    filename: String,
    primary: Option<bool>,
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersionWire {
    version_number: String,
    game_versions: Vec<String>,
    files: Vec<ModrinthVersionFileWire>,
}

#[derive(Debug, Clone)]
struct ResolvedCompanionDownload {
    download_url: String,
    file_name: String,
    version: String,
    size_hint: Option<u64>,
}

fn validate_profile_id(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("snine_update_profile_id_invalid".into());
    }
    Ok(value)
}

fn update_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::limited(8))
        .http1_only()
        .user_agent(format!("SNine-Launcher/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| format!("snine_update_http_client_failed:{error}"))
}

pub(crate) fn target_paths(core: &CoreServices, profile_id: &str) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let profile_id = validate_profile_id(profile_id)?;
    let mods = core
        .registry()
        .resolve("profiles", format!("{profile_id}/instance/mods"))
        .map_err(|error| format!("snine_update_mods_path_failed:{}", error.descriptor().code))?;
    let target = core
        .registry()
        .resolve("profiles", format!("{profile_id}/instance/mods/snineclient.jar"))
        .map_err(|error| format!("snine_update_target_path_failed:{}", error.descriptor().code))?;
    let metadata = core
        .registry()
        .resolve("data", format!("snine-client-update-{profile_id}.json"))
        .map_err(|error| format!("snine_update_metadata_path_failed:{}", error.descriptor().code))?;
    Ok((
        mods.absolute().to_path_buf(),
        target.absolute().to_path_buf(),
        metadata.absolute().to_path_buf(),
    ))
}

fn header_string(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn custom_version(headers: &reqwest::header::HeaderMap) -> Option<String> {
    ["x-snine-version", "x-client-version", "x-version"]
        .into_iter()
        .find_map(|name| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

fn normalized_remote_size(size: Option<u64>) -> Option<u64> {
    size.filter(|value| *value > 0 && *value <= MAX_CLIENT_JAR_BYTES)
}

fn content_range_total(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    let raw = header_string(headers, CONTENT_RANGE)?;
    let total = raw.rsplit('/').next()?.trim();
    if total == "*" {
        return None;
    }
    normalized_remote_size(total.parse::<u64>().ok())
}

fn remote_metadata(response: &reqwest::Response) -> RemoteMetadata {
    let headers = response.headers();
    // Content-Length is only a hint. Range responses can legally report the
    // length of the returned slice (often 1 byte), while Content-Range carries
    // the actual object size. Invalid/absurd values are ignored instead of
    // blocking an otherwise valid SNine jar download.
    let size_bytes = content_range_total(headers).or_else(|| {
        normalized_remote_size(response.content_length().or_else(|| {
            header_string(headers, CONTENT_LENGTH).and_then(|value| value.parse::<u64>().ok())
        }))
    });
    RemoteMetadata {
        etag: header_string(headers, ETAG),
        last_modified: header_string(headers, LAST_MODIFIED),
        version: custom_version(headers),
        size_bytes,
    }
}

fn update_urls() -> [&'static str; 2] {
    // Prefer the exact distribution URL requested by SNine. HTTPS stays as a
    // fallback for hosts/proxies that force TLS.
    [SNINE_CLIENT_FALLBACK_URL, SNINE_CLIENT_URL]
}

async fn probe_remote(client: &reqwest::Client) -> Result<RemoteMetadata, String> {
    let mut last_error = String::from("snine_update_remote_unreachable");

    for url in update_urls() {
        match client.head(url).send().await {
            Ok(head) if head.status().is_success() => return Ok(remote_metadata(&head)),
            Ok(head) => last_error = format!("snine_update_probe_http_{}", head.status().as_u16()),
            Err(error) => last_error = format!("snine_update_probe_failed:{error}"),
        }

        // Some static hosts/CDNs reject HEAD. A one-byte range request gives us the
        // same metadata without downloading the whole client.
        match client.get(url).header(RANGE, "bytes=0-0").send().await {
            Ok(response) if response.status().is_success() || response.status().as_u16() == 206 => {
                return Ok(remote_metadata(&response));
            }
            Ok(response) => {
                last_error = format!("snine_update_probe_get_http_{}", response.status().as_u16())
            }
            Err(error) => last_error = format!("snine_update_probe_get_failed:{error}"),
        }
    }

    Err(last_error)
}

async fn open_download_response(client: &reqwest::Client) -> Result<reqwest::Response, String> {
    let mut last_error = String::from("snine_update_download_unreachable");
    for url in update_urls() {
        for attempt in 0..2u64 {
            match client
                .get(url)
                .header(ACCEPT, "application/java-archive, application/octet-stream, */*")
                .header(ACCEPT_ENCODING, "identity")
                .header(CACHE_CONTROL, "no-cache")
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    last_error = format!("snine_update_download_http_{}", response.status().as_u16())
                }
                Err(error) => last_error = format!("snine_update_download_failed:{error}"),
            }
            tokio::time::sleep(Duration::from_millis(250 * (attempt + 1))).await;
        }
    }
    Err(last_error)
}

#[cfg(target_os = "windows")]
async fn download_with_windows_curl(
    app: &AppHandle,
    profile_id: &str,
    temporary: &Path,
    total_bytes: Option<u64>,
) -> Result<(u64, String), String> {
    let mut last_error = String::from("snine_update_curl_failed");

    for url in update_urls() {
        let _ = tokio::fs::remove_file(temporary).await;
        let mut child = tokio::process::Command::new("curl.exe")
            .arg("--location")
            .arg("--fail")
            .arg("--retry")
            .arg("3")
            .arg("--retry-delay")
            .arg("1")
            .arg("--retry-connrefused")
            .arg("--http1.1")
            .arg("--header")
            .arg("Accept: application/java-archive, application/octet-stream, */*")
            .arg("--header")
            .arg("Cache-Control: no-cache")
            .arg("--silent")
            .arg("--show-error")
            .arg("--connect-timeout")
            .arg("15")
            .arg("--max-time")
            .arg("240")
            .arg("--output")
            .arg(temporary)
            .arg(url)
            .spawn()
            .map_err(|error| format!("snine_update_curl_spawn_failed:{error}"))?;

        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if status.success() {
                        let (sha256, downloaded) = hash_file(temporary)?;
                        emit_progress(app, profile_id, downloaded, total_bytes.or(Some(downloaded)), "downloading");
                        return Ok((downloaded, sha256));
                    }
                    last_error = format!("snine_update_curl_exit_{}", status.code().unwrap_or(-1));
                    break;
                }
                Ok(None) => {
                    if let Ok(metadata) = tokio::fs::metadata(temporary).await {
                        emit_progress(app, profile_id, metadata.len(), total_bytes, "downloading");
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                Err(error) => {
                    last_error = format!("snine_update_curl_wait_failed:{error}");
                    let _ = child.kill().await;
                    break;
                }
            }
        }
    }

    Err(last_error)
}

#[cfg(target_os = "windows")]
async fn download_with_windows_powershell(
    app: &AppHandle,
    profile_id: &str,
    temporary: &Path,
    total_bytes: Option<u64>,
) -> Result<(u64, String), String> {
    let mut last_error = String::from("snine_update_powershell_failed");
    let path = temporary.to_string_lossy().replace('\'', "''");

    for url in update_urls() {
        let _ = tokio::fs::remove_file(temporary).await;
        let url = url.replace('\'', "''");
        let script = format!(
            concat!(
                "$ErrorActionPreference='Stop'; ",
                "[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; ",
                "$wc=New-Object System.Net.WebClient; ",
                "$wc.Headers['User-Agent']='SNine-Launcher/{}'; ",
                "$wc.Headers['Accept']='application/java-archive, application/octet-stream, */*'; ",
                "$wc.DownloadFile('{}','{}')"
            ),
            env!("CARGO_PKG_VERSION"),
            url,
            path
        );

        let status = tokio::process::Command::new("powershell.exe")
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-Command")
            .arg(script)
            .status()
            .await
            .map_err(|error| format!("snine_update_powershell_spawn_failed:{error}"))?;

        if status.success() && temporary.is_file() {
            let (sha256, downloaded) = hash_file(temporary)?;
            emit_progress(
                app,
                profile_id,
                downloaded,
                total_bytes.or(Some(downloaded)),
                "downloading",
            );
            return Ok((downloaded, sha256));
        }
        last_error = format!(
            "snine_update_powershell_exit_{}",
            status.code().unwrap_or(-1)
        );
    }

    Err(last_error)
}

#[cfg(not(target_os = "windows"))]
async fn download_with_windows_powershell(
    _app: &AppHandle,
    _profile_id: &str,
    _temporary: &Path,
    _total_bytes: Option<u64>,
) -> Result<(u64, String), String> {
    Err("snine_update_powershell_unavailable".into())
}

#[cfg(not(target_os = "windows"))]
async fn download_with_windows_curl(
    _app: &AppHandle,
    _profile_id: &str,
    _temporary: &Path,
    _total_bytes: Option<u64>,
) -> Result<(u64, String), String> {
    Err("snine_update_curl_unavailable".into())
}

fn read_metadata(path: &Path) -> Option<PersistedClientMetadata> {
    serde_json::from_slice::<PersistedClientMetadata>(&fs::read(path).ok()?).ok()
}

fn write_metadata(path: &Path, metadata: &PersistedClientMetadata) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("snine_update_metadata_dir_failed:{error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|error| format!("snine_update_metadata_json_failed:{error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)
        .map_err(|error| format!("snine_update_metadata_write_failed:{error}"))?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&temporary, path)
        .map_err(|error| format!("snine_update_metadata_commit_failed:{error}"))
}

fn hash_file(path: &Path) -> Result<(String, u64), String> {
    let mut file = File::open(path)
        .map_err(|error| format!("snine_update_local_open_failed:{error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("snine_update_local_read_failed:{error}"))?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    Ok((hex::encode(hasher.finalize()), total))
}

fn inspect_snine_jar(path: &Path) -> Result<Option<String>, String> {
    let file = File::open(path)
        .map_err(|error| format!("snine_client_jar_open_failed:{error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("snine_client_jar_invalid:{error}"))?;
    let mut entry = archive
        .by_name("fabric.mod.json")
        .map_err(|_| "snine_client_fabric_metadata_missing".to_string())?;
    let mut json_bytes = Vec::new();
    entry
        .read_to_end(&mut json_bytes)
        .map_err(|error| format!("snine_client_fabric_metadata_read_failed:{error}"))?;
    let json: Value = serde_json::from_slice(&json_bytes)
        .map_err(|error| format!("snine_client_fabric_metadata_json_failed:{error}"))?;
    let id = json.get("id").and_then(Value::as_str).unwrap_or_default();
    if id != "snineclient" {
        return Err(format!("snine_client_mod_id_invalid:{id}"));
    }

    // The launcher must never silently start a different Minecraft runtime with
    // a SNine jar that Fabric will ignore/reject. The distributed client is the
    // Minecraft 1.21.11 build, so fail closed if the jar says otherwise.
    if let Some(minecraft) = json
        .get("depends")
        .and_then(|value| value.get("minecraft"))
        .and_then(Value::as_str)
    {
        if !minecraft.contains("1.21.11") {
            return Err(format!("snine_client_minecraft_version_invalid:{minecraft}"));
        }
    }

    Ok(json
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned))
}

fn is_snine_client_jar(path: &Path) -> bool {
    inspect_snine_jar(path).is_ok()
}

fn remove_duplicate_snine_jars(mods_dir: &Path, target: &Path) {
    let Ok(entries) = fs::read_dir(mods_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == target || !path.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("jar"))
            && is_snine_client_jar(&path)
        {
            let _ = fs::remove_file(path);
        }
    }
}

fn metadata_changed(remote: &RemoteMetadata, local: &PersistedClientMetadata) -> bool {
    if local.url != SNINE_CLIENT_URL {
        return true;
    }
    // Content-Length alone is not a safe version identifier. If the host does not
    // expose ETag, Last-Modified or an explicit version, redownload on launch so
    // a changed jar can never be mistaken for the currently installed build.
    if remote.version.is_none() && remote.etag.is_none() && remote.last_modified.is_none() {
        // Never use Content-Length as an update gate. Some proxies/CDNs report
        // range/chunk sizes here. Without a strong validator we prefer a fresh,
        // verified JAR download over trusting remote size metadata.
        return true;
    }
    if let Some(value) = remote.version.as_deref() {
        if local.remote_version.as_deref() != Some(value) {
            return true;
        }
    }
    if let Some(value) = remote.etag.as_deref() {
        if local.etag.as_deref() != Some(value) {
            return true;
        }
    }
    if let Some(value) = remote.last_modified.as_deref() {
        if local.last_modified.as_deref() != Some(value) {
            return true;
        }
    }
    if let Some(value) = remote.size_bytes {
        if local.size_bytes != Some(value) {
            return true;
        }
    }
    false
}


fn inspect_generic_fabric_jar(path: &Path) -> Result<(String, Option<String>), String> {
    let file = File::open(path)
        .map_err(|error| format!("fabric_mod_open_failed:{error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("fabric_mod_archive_invalid:{error}"))?;
    let mut entry = archive
        .by_name("fabric.mod.json")
        .map_err(|_| "fabric_mod_metadata_missing".to_string())?;
    let mut json_bytes = Vec::new();
    entry
        .read_to_end(&mut json_bytes)
        .map_err(|error| format!("fabric_mod_metadata_read_failed:{error}"))?;
    let json: Value = serde_json::from_slice(&json_bytes)
        .map_err(|error| format!("fabric_mod_metadata_json_failed:{error}"))?;
    let id = json
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "fabric_mod_id_missing".to_string())?
        .to_string();
    let version = json
        .get("version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Ok((id, version))
}

fn installed_mod_version(mods_dir: &Path, mod_id: &str) -> Option<String> {
    let entries = fs::read_dir(mods_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        if let Ok((id, version)) = inspect_generic_fabric_jar(&path) {
            if id.eq_ignore_ascii_case(mod_id) {
                return version;
            }
        }
    }
    None
}

fn remove_duplicate_mod_jars(mods_dir: &Path, target: &Path, mod_id: &str) {
    let Ok(entries) = fs::read_dir(mods_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == target || !path.is_file() {
            continue;
        }
        if !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("jar"))
        {
            continue;
        }
        if inspect_generic_fabric_jar(&path)
            .ok()
            .is_some_and(|(id, _)| id.eq_ignore_ascii_case(mod_id))
        {
            let _ = fs::remove_file(path);
        }
    }
}

async fn resolve_companion_download(
    client: &reqwest::Client,
    spec: CompanionModSpec,
) -> Result<ResolvedCompanionDownload, String> {
    let url = format!(
        "{}/project/{}/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%221.21.11%22%5D",
        MODRINTH_API_BASE, spec.project_slug
    );
    let response = client
        .get(&url)
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| format!("snine_support_{}_version_lookup_failed:{error}", spec.mod_id))?;
    if !response.status().is_success() {
        return Err(format!(
            "snine_support_{}_version_http_{}",
            spec.mod_id,
            response.status().as_u16()
        ));
    }
    let versions = response
        .json::<Vec<ModrinthVersionWire>>()
        .await
        .map_err(|error| format!("snine_support_{}_version_json_failed:{error}", spec.mod_id))?;
    let version = versions
        .into_iter()
        .find(|version| version.game_versions.iter().any(|game| game == "1.21.11"))
        .ok_or_else(|| format!("snine_support_{}_version_missing", spec.mod_id))?;
    let files = version.files;
    let file = files
        .iter()
        .find(|file| file.primary.unwrap_or(false))
        .cloned()
        .or_else(|| files.into_iter().next())
        .ok_or_else(|| format!("snine_support_{}_file_missing", spec.mod_id))?;
    Ok(ResolvedCompanionDownload {
        download_url: file.url,
        file_name: file.filename,
        version: version.version_number,
        size_hint: file.size,
    })
}

async fn download_companion_mod(
    client: &reqwest::Client,
    app: &AppHandle,
    profile_id: &str,
    mods_dir: &Path,
    spec: CompanionModSpec,
) -> Result<(), String> {
    let resolved = resolve_companion_download(client, spec).await?;
    if installed_mod_version(mods_dir, spec.mod_id)
        .as_deref()
        .is_some_and(|version| version == resolved.version)
    {
        return Ok(());
    }
    let target = mods_dir.join(&resolved.file_name);
    let temporary = target.with_extension("jar.part");
    let _ = fs::remove_file(&temporary);

    emit_progress(app, profile_id, 0, resolved.size_hint, "dependencies");
    let response = client
        .get(&resolved.download_url)
        .header(ACCEPT, "application/java-archive, application/octet-stream, */*")
        .send()
        .await
        .map_err(|error| format!("snine_support_{}_download_failed:{error}", spec.mod_id))?;
    if !response.status().is_success() {
        return Err(format!(
            "snine_support_{}_download_http_{}",
            spec.mod_id,
            response.status().as_u16()
        ));
    }

    let mut file = tokio::fs::File::create(&temporary)
        .await
        .map_err(|error| format!("snine_support_{}_temp_create_failed:{error}", spec.mod_id))?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("snine_support_{}_stream_failed:{error}", spec.mod_id))?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if downloaded > COMPANION_MOD_MAX_BYTES {
            return Err(format!("snine_support_{}_download_too_large", spec.mod_id));
        }
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("snine_support_{}_temp_write_failed:{error}", spec.mod_id))?;
        emit_progress(app, profile_id, downloaded, resolved.size_hint, "dependencies");
    }
    file.flush()
        .await
        .map_err(|error| format!("snine_support_{}_temp_flush_failed:{error}", spec.mod_id))?;
    drop(file);

    let (mod_id, _version) = inspect_generic_fabric_jar(&temporary)
        .map_err(|error| format!("snine_support_{}_invalid_jar:{error}", spec.mod_id))?;
    if !mod_id.eq_ignore_ascii_case(spec.mod_id) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("snine_support_{}_mod_id_invalid:{mod_id}", spec.mod_id));
    }
    remove_duplicate_mod_jars(mods_dir, &target, spec.mod_id);
    fs::rename(&temporary, &target)
        .map_err(|error| format!("snine_support_{}_commit_failed:{error}", spec.mod_id))?;
    eprintln!(
        "[snine-client-updater] installed companion mod {} {} for profile {}",
        spec.display_name,
        resolved.version,
        profile_id
    );
    Ok(())
}

pub async fn ensure_required_support_mods(
    app: &AppHandle,
    core: &CoreServices,
    profile_id: &str,
) -> Result<(), String> {
    let (mods_dir, _, _) = target_paths(core, profile_id)?;
    fs::create_dir_all(&mods_dir)
        .map_err(|error| format!("snine_support_mods_dir_failed:{error}"))?;
    let client = update_client()?;
    for spec in COMPANION_MODS {
        download_companion_mod(&client, app, profile_id, &mods_dir, spec).await?;
    }
    Ok(())
}

async fn update_check_impl(
    core: &CoreServices,
    profile_id: &str,
) -> Result<SnineClientUpdateCheck, String> {
    let (_, target, metadata_path) = target_paths(core, profile_id)?;
    let external_client_installed = target.is_file() && inspect_snine_jar(&target).is_ok();
    let installed_version = inspect_snine_jar(&target).ok().flatten();
    let local_metadata = read_metadata(&metadata_path);

    let client = update_client()?;
    let remote = match probe_remote(&client).await {
        Ok(value) => value,
        Err(error) => {
            return Ok(SnineClientUpdateCheck {
                reachable: false,
                update_available: false,
                external_client_installed,
                installed_version,
                remote_version: None,
                remote_size_bytes: None,
                status_message: error,
            });
        }
    };

    let mut update_available = !external_client_installed || local_metadata.is_none();
    if let Some(local) = local_metadata.as_ref() {
        if external_client_installed {
            if let Ok((current_sha, current_size)) = hash_file(&target) {
                if current_sha != local.sha256 || Some(current_size) != local.size_bytes {
                    update_available = true;
                }
            } else {
                update_available = true;
            }
        }
        if metadata_changed(&remote, local) {
            update_available = true;
        }
    }

    Ok(SnineClientUpdateCheck {
        reachable: true,
        update_available,
        external_client_installed,
        installed_version,
        remote_version: remote.version,
        remote_size_bytes: remote.size_bytes,
        status_message: if update_available {
            "update_available"
        } else {
            "client_current"
        }
        .into(),
    })
}

fn emit_progress(
    app: &AppHandle,
    profile_id: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    stage: &str,
) {
    let percent = total_bytes
        .filter(|total| *total > 0)
        .map(|total| ((downloaded_bytes as f64 / total as f64) * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0);
    let _ = app.emit(
        DOWNLOAD_EVENT,
        SnineClientDownloadProgress {
            profile_id: profile_id.to_string(),
            downloaded_bytes,
            total_bytes,
            percent,
            stage: stage.to_string(),
        },
    );
}

async fn download_update_impl(
    app: &AppHandle,
    core: &CoreServices,
    profile_id: &str,
) -> Result<SnineClientDownloadResult, String> {
    let profile_id = validate_profile_id(profile_id)?.to_string();
    let (mods_dir, target, metadata_path) = target_paths(core, &profile_id)?;
    fs::create_dir_all(&mods_dir)
        .map_err(|error| format!("snine_update_mods_dir_failed:{error}"))?;

    let client = update_client()?;
    let remote_hint = probe_remote(&client).await.ok();
    let temporary = target.with_extension("jar.part");
    let backup = target.with_extension("jar.previous");
    let _ = fs::remove_file(&temporary);
    let _ = fs::remove_file(&backup);

    let mut remote = remote_hint.unwrap_or_default();
    remote.size_bytes = normalized_remote_size(remote.size_bytes);

    emit_progress(app, &profile_id, 0, remote.size_bytes, "downloading");

    let reqwest_result: Result<(u64, String), String> = async {
        let response = open_download_response(&client).await?;
        let response_metadata = remote_metadata(&response);
        if response_metadata.etag.is_some() { remote.etag = response_metadata.etag.clone(); }
        if response_metadata.last_modified.is_some() { remote.last_modified = response_metadata.last_modified.clone(); }
        if response_metadata.version.is_some() { remote.version = response_metadata.version.clone(); }
        if response_metadata.size_bytes.is_some() { remote.size_bytes = response_metadata.size_bytes; }

        remote.size_bytes = normalized_remote_size(remote.size_bytes);

        let mut file = tokio::fs::File::create(&temporary)
            .await
            .map_err(|error| format!("snine_update_temp_create_failed:{error}"))?;
        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| format!("snine_update_stream_failed:{error}"))?;
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > MAX_CLIENT_JAR_BYTES {
                return Err("snine_update_download_too_large".into());
            }
            file.write_all(&chunk)
                .await
                .map_err(|error| format!("snine_update_temp_write_failed:{error}"))?;
            hasher.update(&chunk);
            emit_progress(app, &profile_id, downloaded, remote.size_bytes, "downloading");
        }
        file.flush()
            .await
            .map_err(|error| format!("snine_update_temp_flush_failed:{error}"))?;
        drop(file);

        if downloaded == 0 {
            return Err("snine_update_download_empty".into());
        }
        if let Some(expected) = remote.size_bytes {
            if expected != downloaded {
                eprintln!(
                    "[snine-client-updater] server size hint differs from downloaded bytes: expected={expected} actual={downloaded}; validating jar instead"
                );
                remote.size_bytes = Some(downloaded);
            }
        }
        Ok((downloaded, hex::encode(hasher.finalize())))
    }.await;

    let mut failures = Vec::<String>::new();
    let mut accepted: Option<(u64, String, Option<String>)> = None;

    let mut accept_candidate = |source: &str, candidate: Result<(u64, String), String>| {
        match candidate {
            Ok((downloaded, sha256)) => {
                if downloaded == 0 || downloaded > MAX_CLIENT_JAR_BYTES {
                    failures.push(format!("{source}:snine_update_download_size_invalid:{downloaded}"));
                    return false;
                }
                // A proxy/CDN can return an HTML body with HTTP 200. Validate the
                // downloaded bytes as the actual SNine Fabric jar *before* we
                // accept this transport and before we stop trying fallbacks.
                match inspect_snine_jar(&temporary) {
                    Ok(version) => {
                        accepted = Some((downloaded, sha256, version));
                        true
                    }
                    Err(error) => {
                        failures.push(format!("{source}:{error}"));
                        let _ = fs::remove_file(&temporary);
                        false
                    }
                }
            }
            Err(error) => {
                failures.push(format!("{source}:{error}"));
                let _ = fs::remove_file(&temporary);
                false
            }
        }
    };

    if !accept_candidate("reqwest", reqwest_result) {
        eprintln!("[snine-client-updater] reqwest candidate rejected; trying curl.exe");
        let curl = download_with_windows_curl(app, &profile_id, &temporary, remote.size_bytes).await;
        if !accept_candidate("curl", curl) {
            eprintln!("[snine-client-updater] curl candidate rejected; trying Windows PowerShell WebClient");
            let powershell = download_with_windows_powershell(app, &profile_id, &temporary, remote.size_bytes).await;
            let _ = accept_candidate("powershell", powershell);
        }
    }

    let Some((downloaded, sha256, installed_version)) = accepted else {
        let detail = failures.join(" | ");
        eprintln!("[snine-client-updater] every download transport failed: {detail}");
        return Err(format!("snine_update_all_transports_failed:{detail}"));
    };

    // Content-Length is metadata, not a trust boundary. A transparent proxy can
    // change transfer encoding/length while the actual jar remains valid. The jar
    // structure + Fabric metadata validation above is authoritative.
    if let Some(expected) = remote.size_bytes {
        if expected != downloaded {
            eprintln!("[snine-client-updater] remote size differs after validated download: expected={expected} actual={downloaded}");
            remote.size_bytes = Some(downloaded);
        }
    }

    emit_progress(app, &profile_id, downloaded, Some(downloaded), "verifying");

    if target.exists() {
        fs::rename(&target, &backup)
            .map_err(|error| format!("snine_update_backup_failed:{error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!("snine_update_commit_failed:{error}"));
    }
    let _ = fs::remove_file(&backup);

    remove_duplicate_snine_jars(&mods_dir, &target);

    let metadata = PersistedClientMetadata {
        url: SNINE_CLIENT_URL.into(),
        etag: remote.etag,
        last_modified: remote.last_modified,
        remote_version: remote.version,
        size_bytes: Some(downloaded),
        sha256: sha256.clone(),
        installed_version: installed_version.clone(),
        downloaded_at_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs() as i64)
            .unwrap_or_default(),
    };
    write_metadata(&metadata_path, &metadata)?;
    emit_progress(app, &profile_id, downloaded, Some(downloaded), "complete");

    Ok(SnineClientDownloadResult {
        installed_version,
        sha256,
        size_bytes: downloaded,
        target_file: target.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn snine_client_update_check(
    core: State<'_, CoreServices>,
    profile_id: String,
) -> Result<SnineClientUpdateCheck, String> {
    update_check_impl(core.inner(), &profile_id).await
}

#[tauri::command]
pub async fn snine_client_download_update(
    app: AppHandle,
    core: State<'_, CoreServices>,
    profile_id: String,
) -> Result<SnineClientDownloadResult, String> {
    download_update_impl(&app, core.inner(), &profile_id).await
}


pub fn verify_client_ready(core: &CoreServices, profile_id: &str) -> Result<Option<String>, String> {
    let (_, target, _) = target_paths(core, profile_id)?;
    if !target.is_file() {
        return Err("snine_client_missing_after_update".into());
    }
    inspect_snine_jar(&target)
}

/// Launch-time updater used by the already-established `phase5_launch_profile` IPC.
/// This deliberately keeps update delivery behind an IPC command that has existed
/// since the original launcher, so a stale frontend/backend command table can no
/// longer block launching with `Command snine_client_update_check not found`.
pub async fn ensure_client_for_launch(
    app: &AppHandle,
    core: &CoreServices,
    profile_id: &str,
) -> Result<bool, String> {
    eprintln!("[snine-client-updater] revision={UPDATER_REVISION}; remote size metadata is advisory only");
    let check = match update_check_impl(core, profile_id).await {
        Ok(check) => check,
        Err(error) => {
            eprintln!("[snine-client-updater] metadata check failed ({error}); attempting direct verified download");
            download_update_impl(app, core, profile_id).await?;
            return Ok(true);
        }
    };

    if check.update_available {
        if !check.reachable {
            if check.external_client_installed {
                return Ok(true);
            }
            return Err(check.status_message);
        }
        if let Err(error) = download_update_impl(app, core, profile_id).await {
            if check.external_client_installed && verify_client_ready(core, profile_id).is_ok() {
                eprintln!("[snine-client-updater] update failed; launching verified installed SNine Client: {error}");
                return Ok(true);
            }
            return Err(error);
        }
        return Ok(true);
    }

    if check.external_client_installed {
        Ok(true)
    } else if check.reachable {
        // No local validated jar exists, so a reachable source must be downloaded
        // even if a metadata edge-case reported no change.
        download_update_impl(app, core, profile_id).await?;
        Ok(true)
    } else {
        Err(check.status_message)
    }
}
