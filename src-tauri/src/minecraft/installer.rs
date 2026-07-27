use crate::{
    app::{
        config,
        paths::{self, GamePaths},
    },
    error::{AppError, AppResult},
    logging,
    minecraft::{
        java,
        manifest::{
            self, DownloadInfo, FabricLibrary, FabricProfile, FeatureSet, Library, VersionJson,
        },
    },
};
use futures_util::{stream, StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

const MIN_FABRIC_LOADER: &str = "0.19.3";
const MANAGED_MODS_FILE: &str = ".s9lab-managed-mods.json";
const ASSET_CONCURRENCY: usize = 16;
const WAVEY_CAPES_FILE: &str = "wavey-capes-fabric-1.21.11.jar";
const WAVEY_CAPES_VERSIONS_API: &str = "https://api.modrinth.com/v2/project/wavey-capes/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%221.21.11%22%5D&include_changelog=false";

const FABRIC_API: &[u8] =
    include_bytes!("../../resources/default-profile-mods/fabric-api-0.141.4+1.21.11.jar");
const GECKOLIB: &[u8] =
    include_bytes!("../../resources/default-profile-mods/geckolib-fabric-1.21.11-5.4.5.jar");
const S9LAB_CLIENT: &[u8] =
    include_bytes!("../../resources/default-profile-mods/s9lab-client-bundled.jar");
const BUNDLED_MODS: &[(&str, &[u8])] = &[
    ("fabric-api-0.141.4+1.21.11.jar", FABRIC_API),
    ("geckolib-fabric-1.21.11-5.4.5.jar", GECKOLIB),
    ("s9lab-client-bundled.jar", S9LAB_CLIENT),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRecord {
    pub game_version: String,
    pub fabric_loader: String,
    pub fabric_profile_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientStatus {
    pub installed: bool,
    pub game_version: String,
    pub fabric_loader: Option<String>,
    pub java_found: bool,
    pub java_path: Option<String>,
    pub bundled_mods: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallProgress {
    pub stage: String,
    pub detail: String,
    pub current: u64,
    pub total: u64,
    pub percent: f64,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    url: String,
    #[serde(default)]
    primary: bool,
    hashes: ModrinthHashes,
}

#[derive(Debug, Deserialize)]
struct ModrinthHashes {
    sha1: String,
}

#[derive(Debug, Deserialize)]
struct AssetObjects {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    #[serde(default)]
    size: Option<u64>,
}

pub fn bundled_mod_names() -> Vec<String> {
    let mut names: Vec<String> = BUNDLED_MODS
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    names.push(WAVEY_CAPES_FILE.to_string());
    names
}

pub fn get_client_status() -> AppResult<ClientStatus> {
    let settings = config::load_settings()?;
    let game = paths::game_paths(&settings.game_directory)?;
    let record = load_install_record().ok();
    let installed = record.as_ref().is_some_and(|record| {
        record.game_version == settings.game_version
            && validate_installation(&game, record).unwrap_or(false)
    });
    let java = java::resolve_java_optional(settings.java_path.as_deref());
    Ok(ClientStatus {
        installed,
        game_version: settings.game_version,
        fabric_loader: record.map(|record| record.fabric_loader),
        java_found: java.is_some(),
        java_path: java.map(|runtime| runtime.path),
        bundled_mods: bundled_mod_names(),
    })
}

pub async fn install_client(app: AppHandle, repair: bool) -> AppResult<ClientStatus> {
    let settings = config::load_settings()?;
    let game = paths::game_paths(&settings.game_directory)?;
    let client = http_client()?;
    emit_progress(&app, "Java", "Java-21-Laufzeit wird geprüft", 0, 9, 1.0);
    let runtime = java::ensure_java(settings.java_path.as_deref()).await?;
    logging::append(&format!(
        "Java-Runtime bereit: Java {} unter {}",
        runtime.major_version, runtime.path
    ))?;
    logging::append(&format!(
        "Installation gestartet: Minecraft {}, repair={repair}",
        settings.game_version
    ))?;

    emit_progress(
        &app,
        "Vorbereitung",
        "Spielordner und Client-Mods",
        0,
        8,
        1.0,
    );
    sync_bundled_mods(&game)?;
    sync_wavey_capes(&client, &game).await?;
    if repair {
        let native_dir = game.natives.join(&settings.game_version);
        if native_dir.exists() {
            fs::remove_dir_all(native_dir)?;
        }
    }

    emit_progress(&app, "Minecraft", "Versionsdaten werden geladen", 1, 8, 7.0);
    let version = manifest::fetch_version(&client, &settings.game_version).await?;
    save_version_json(&game, &version)?;

    emit_progress(&app, "Minecraft", "Client-Datei", 2, 8, 14.0);
    download_to(
        &client,
        &version.downloads.client,
        &version_jar_path(&game, &version.id),
    )
    .await?;

    emit_progress(
        &app,
        "Bibliotheken",
        "Minecraft-Bibliotheken und Natives",
        3,
        8,
        22.0,
    );
    install_minecraft_libraries(&app, &client, &game, &version).await?;

    emit_progress(&app, "Assets", "Asset-Index und Spieldateien", 4, 8, 48.0);
    install_assets(&app, &client, &game, &version).await?;

    emit_progress(
        &app,
        "Logging",
        "Minecraft-Logging-Konfiguration",
        5,
        8,
        81.0,
    );
    install_logging_config(&client, &game, &version).await?;

    emit_progress(&app, "Fabric", "Kompatibler Fabric Loader", 6, 8, 86.0);
    let (loader, profile) =
        manifest::fetch_fabric_profile(&client, &settings.game_version, MIN_FABRIC_LOADER).await?;
    install_fabric_libraries(&app, &client, &game, &profile).await?;
    save_fabric_profile(&game, &profile)?;

    let record = InstallRecord {
        game_version: settings.game_version,
        fabric_loader: loader,
        fabric_profile_id: profile.id,
    };
    save_install_record(&record)?;
    sync_bundled_mods(&game)?;
    sync_wavey_capes(&client, &game).await?;
    emit_progress(&app, "Fertig", "S9Lab Client ist startbereit", 8, 8, 100.0);
    logging::append(&format!(
        "Installation abgeschlossen: {} / Fabric {}",
        record.game_version, record.fabric_loader
    ))?;
    get_client_status()
}

pub fn sync_bundled_mods(game: &GamePaths) -> AppResult<Vec<String>> {
    fs::create_dir_all(&game.mods)?;
    let managed_file = game.mods.join(MANAGED_MODS_FILE);
    let current = bundled_mod_names();
    let previous: Vec<String> = fs::read_to_string(&managed_file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    for name in previous {
        if !current.iter().any(|current_name| current_name == &name) {
            let stale = game.mods.join(name);
            if stale.exists() {
                fs::remove_file(stale)?;
            }
        }
    }
    for &(name, bytes) in BUNDLED_MODS {
        if name.starts_with("s9labclient-") && game.mods.join(name).exists() {
            continue;
        }
        let target = game.mods.join(name);
        if target.exists() && file_sha1(&target)? == bytes_sha1(bytes) {
            continue;
        }
        write_atomic(&target, bytes)?;
    }
    write_atomic(
        &managed_file,
        serde_json::to_string_pretty(&current)?.as_bytes(),
    )?;
    Ok(current)
}

pub async fn sync_wavey_capes(client: &Client, game: &GamePaths) -> AppResult<()> {
    fs::create_dir_all(&game.mods)?;
    let versions: Vec<ModrinthVersion> = client
        .get(WAVEY_CAPES_VERSIONS_API)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let version = versions.first().ok_or_else(|| {
        AppError::Message(
            "Keine kompatible Wavey-Capes-Version für Fabric 1.21.11 gefunden.".into(),
        )
    })?;
    let file = version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| {
            AppError::Message("Die Wavey-Capes-Version enthält keine herunterladbare Datei.".into())
        })?;

    let target = game.mods.join(WAVEY_CAPES_FILE);
    if target.exists() && file_sha1(&target)?.eq_ignore_ascii_case(&file.hashes.sha1) {
        return Ok(());
    }

    let bytes = client
        .get(&file.url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let downloaded_sha1 = bytes_sha1(&bytes);
    if !downloaded_sha1.eq_ignore_ascii_case(&file.hashes.sha1) {
        return Err(AppError::Message(
            "Wavey Capes konnte nicht sicher verifiziert werden (SHA-1 stimmt nicht überein)."
                .into(),
        ));
    }

    write_atomic(&target, &bytes)?;
    logging::append("Wavey Capes wurde über Modrinth installiert oder aktualisiert.")?;
    Ok(())
}

pub fn load_install_record() -> AppResult<InstallRecord> {
    let path = paths::launcher_paths()?.installation_file;
    if !path.exists() {
        return Err(AppError::ClientNotInstalled);
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub fn load_version_json(game: &GamePaths, version_id: &str) -> AppResult<VersionJson> {
    Ok(serde_json::from_str(&fs::read_to_string(
        version_json_path(game, version_id),
    )?)?)
}

pub fn load_fabric_profile(game: &GamePaths, profile_id: &str) -> AppResult<FabricProfile> {
    Ok(serde_json::from_str(&fs::read_to_string(
        fabric_profile_path(game, profile_id),
    )?)?)
}

pub fn version_jar_path(game: &GamePaths, version_id: &str) -> PathBuf {
    game.versions
        .join(version_id)
        .join(format!("{version_id}.jar"))
}

pub fn version_json_path(game: &GamePaths, version_id: &str) -> PathBuf {
    game.versions
        .join(version_id)
        .join(format!("{version_id}.json"))
}

pub fn fabric_profile_path(game: &GamePaths, profile_id: &str) -> PathBuf {
    game.versions
        .join(profile_id)
        .join(format!("{profile_id}.json"))
}

pub fn library_artifact_path(game: &GamePaths, library: &Library) -> AppResult<Option<PathBuf>> {
    let Some(download) = library
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.artifact.as_ref())
    else {
        return Ok(None);
    };
    let relative = download
        .path
        .clone()
        .unwrap_or(manifest::maven_path(&library.name)?);
    Ok(Some(game.libraries.join(relative)))
}

pub fn fabric_library_path(game: &GamePaths, library: &FabricLibrary) -> AppResult<PathBuf> {
    Ok(game.libraries.join(manifest::maven_path(&library.name)?))
}

pub fn logging_config_path(game: &GamePaths, version: &VersionJson) -> Option<PathBuf> {
    version.logging.as_ref().map(|logging| {
        let id = logging
            .client
            .file
            .id
            .clone()
            .or_else(|| {
                logging
                    .client
                    .file
                    .url
                    .rsplit('/')
                    .next()
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "client-log.xml".to_string());
        game.assets.join("log_configs").join(id)
    })
}

fn validate_installation(game: &GamePaths, record: &InstallRecord) -> AppResult<bool> {
    if !version_jar_path(game, &record.game_version).exists()
        || !version_json_path(game, &record.game_version).exists()
        || !fabric_profile_path(game, &record.fabric_profile_id).exists()
        || !game.natives.join(&record.game_version).exists()
    {
        return Ok(false);
    }
    for &(name, bytes) in BUNDLED_MODS {
        if name.starts_with("s9labclient-") && game.mods.join(name).exists() {
            continue;
        }
        let path = game.mods.join(name);
        if !path.exists() || file_sha1(&path)? != bytes_sha1(bytes) {
            return Ok(false);
        }
    }
    if !game.mods.join(WAVEY_CAPES_FILE).exists() {
        return Ok(false);
    }
    Ok(true)
}

fn save_install_record(record: &InstallRecord) -> AppResult<()> {
    write_atomic(
        &paths::launcher_paths()?.installation_file,
        serde_json::to_string_pretty(record)?.as_bytes(),
    )
}

fn save_version_json(game: &GamePaths, version: &VersionJson) -> AppResult<()> {
    let dir = game.versions.join(&version.id);
    fs::create_dir_all(&dir)?;
    write_atomic(
        &dir.join(format!("{}.json", version.id)),
        serde_json::to_string_pretty(version)?.as_bytes(),
    )
}

fn save_fabric_profile(game: &GamePaths, profile: &FabricProfile) -> AppResult<()> {
    let dir = game.versions.join(&profile.id);
    fs::create_dir_all(&dir)?;
    write_atomic(
        &dir.join(format!("{}.json", profile.id)),
        serde_json::to_string_pretty(profile)?.as_bytes(),
    )
}

async fn install_minecraft_libraries(
    app: &AppHandle,
    client: &Client,
    game: &GamePaths,
    version: &VersionJson,
) -> AppResult<()> {
    let native_dir = game.natives.join(&version.id);
    if native_dir.exists() {
        fs::remove_dir_all(&native_dir)?;
    }
    fs::create_dir_all(&native_dir)?;
    let features = FeatureSet::launch_defaults();
    let libraries: Vec<&Library> = version
        .libraries
        .iter()
        .filter(|library| manifest::rules_allow(&library.rules, &features))
        .collect();
    let total = libraries.len().max(1) as u64;
    for (index, library) in libraries.into_iter().enumerate() {
        if let Some(artifact) = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
        {
            let relative = artifact
                .path
                .clone()
                .unwrap_or(manifest::maven_path(&library.name)?);
            download_to(client, artifact, &game.libraries.join(relative)).await?;
        }
        if let Some(classifier) = manifest::native_classifier(library) {
            if let Some(native) = library
                .downloads
                .as_ref()
                .and_then(|downloads| downloads.classifiers.get(&classifier))
            {
                let relative = native.path.clone().unwrap_or_else(|| {
                    format!("natives/{}/{}.jar", version.id, sanitize(&library.name))
                });
                let jar = game.libraries.join(relative);
                download_to(client, native, &jar).await?;
                let excludes = library
                    .extract
                    .as_ref()
                    .map(|extract| extract.exclude.as_slice())
                    .unwrap_or(&[]);
                extract_natives(&jar, &native_dir, excludes)?;
            }
        }
        emit_progress(
            app,
            "Bibliotheken",
            &library.name,
            (index + 1) as u64,
            total,
            22.0 + ((index + 1) as f64 / total as f64) * 25.0,
        );
    }
    Ok(())
}

async fn install_assets(
    app: &AppHandle,
    client: &Client,
    game: &GamePaths,
    version: &VersionJson,
) -> AppResult<()> {
    let index_info = DownloadInfo {
        sha1: version.asset_index.sha1.clone(),
        size: version.asset_index.size,
        url: version.asset_index.url.clone(),
        path: None,
    };
    let index_path = game
        .assets
        .join("indexes")
        .join(format!("{}.json", version.asset_index.id));
    download_to(client, &index_info, &index_path).await?;
    let index: AssetObjects = serde_json::from_str(&fs::read_to_string(index_path)?)?;
    let mut unique = HashSet::new();
    let objects: Vec<AssetObject> = index
        .objects
        .into_values()
        .filter(|object| unique.insert(object.hash.clone()))
        .collect();
    let total = objects.len().max(1) as u64;
    let completed = Arc::new(AtomicU64::new(0));
    let game_assets = game.assets.clone();
    let app_handle = app.clone();
    let client = client.clone();
    stream::iter(objects.into_iter().map(|object| {
        let client = client.clone();
        let assets = game_assets.clone();
        let completed = completed.clone();
        let app = app_handle.clone();
        async move {
            if object.hash.len() < 2 {
                return Err(AppError::Message(
                    "Ungültiger Asset-Hash im Mojang-Index.".into(),
                ));
            }
            let prefix = &object.hash[0..2];
            let info = DownloadInfo {
                sha1: Some(object.hash.clone()),
                size: object.size,
                url: format!(
                    "https://resources.download.minecraft.net/{prefix}/{}",
                    object.hash
                ),
                path: None,
            };
            download_to(
                &client,
                &info,
                &assets.join("objects").join(prefix).join(&object.hash),
            )
            .await?;
            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if current.is_multiple_of(25) || current == total {
                emit_progress(
                    &app,
                    "Assets",
                    &format!("{current} von {total} Dateien"),
                    current,
                    total,
                    48.0 + (current as f64 / total as f64) * 32.0,
                );
            }
            Ok::<(), AppError>(())
        }
    }))
    .buffer_unordered(ASSET_CONCURRENCY)
    .try_collect::<Vec<_>>()
    .await?;
    Ok(())
}

async fn install_fabric_libraries(
    app: &AppHandle,
    client: &Client,
    game: &GamePaths,
    profile: &FabricProfile,
) -> AppResult<()> {
    let total = profile.libraries.len().max(1) as u64;
    for (index, library) in profile.libraries.iter().enumerate() {
        let info = manifest::fabric_download(library)?;
        let target = game
            .libraries
            .join(info.path.as_deref().unwrap_or_default());
        download_to(client, &info, &target).await?;
        emit_progress(
            app,
            "Fabric",
            &library.name,
            (index + 1) as u64,
            total,
            87.0 + ((index + 1) as f64 / total as f64) * 11.0,
        );
    }
    Ok(())
}

async fn install_logging_config(
    client: &Client,
    game: &GamePaths,
    version: &VersionJson,
) -> AppResult<()> {
    let Some(logging) = &version.logging else {
        return Ok(());
    };
    let Some(target) = logging_config_path(game, version) else {
        return Ok(());
    };
    let info = DownloadInfo {
        sha1: logging.client.file.sha1.clone(),
        size: logging.client.file.size,
        url: logging.client.file.url.clone(),
        path: None,
    };
    download_to(client, &info, &target).await
}

async fn download_to(client: &Client, info: &DownloadInfo, target: &Path) -> AppResult<()> {
    if valid_existing(target, info)? {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = target.with_extension(format!(
        "{}part",
        target
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!("{value}."))
            .unwrap_or_default()
    ));
    let mut last_error: Option<AppError> = None;
    for _ in 0..3 {
        let result = async {
            let response = client.get(&info.url).send().await?.error_for_status()?;
            let mut file = tokio::fs::File::create(&part).await?;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                file.write_all(&chunk?).await?;
            }
            file.flush().await?;
            drop(file);
            if !valid_existing(&part, info)? {
                return Err(AppError::HashMismatch { path: part.clone() });
            }
            if target.exists() {
                fs::remove_file(target)?;
            }
            fs::rename(&part, target)?;
            Ok(())
        }
        .await;
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                let _ = fs::remove_file(&part);
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| AppError::Message(format!("Download fehlgeschlagen: {}", info.url))))
}

fn valid_existing(path: &Path, info: &DownloadInfo) -> AppResult<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if let Some(size) = info.size {
        if fs::metadata(path)?.len() != size {
            return Ok(false);
        }
    }
    if let Some(expected) = &info.sha1 {
        return Ok(file_sha1(path)?.eq_ignore_ascii_case(expected));
    }
    Ok(true)
}

fn extract_natives(jar: &Path, target: &Path, excludes: &[String]) -> AppResult<()> {
    let file = fs::File::open(jar)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut item = archive.by_index(index)?;
        if item.is_dir() {
            continue;
        }
        let name = item.name().replace('\\', "/");
        if name.starts_with("META-INF/") || excludes.iter().any(|exclude| name.starts_with(exclude))
        {
            continue;
        }
        let Some(enclosed) = item.enclosed_name() else {
            continue;
        };
        let output = target.join(enclosed);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(output)?;
        std::io::copy(&mut item, &mut out)?;
    }
    Ok(())
}

fn http_client() -> AppResult<Client> {
    Ok(Client::builder()
        .user_agent(concat!("S9Lab-Launcher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(180))
        .build()?)
}

fn emit_progress(
    app: &AppHandle,
    stage: &str,
    detail: &str,
    current: u64,
    total: u64,
    percent: f64,
) {
    let _ = app.emit(
        "install-progress",
        InstallProgress {
            stage: stage.to_string(),
            detail: detail.to_string(),
            current,
            total,
            percent: percent.clamp(0.0, 100.0),
        },
    );
}

fn write_atomic(path: &Path, bytes: &[u8]) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp)?;
    file.write_all(bytes)?;
    file.flush()?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp, path)?;
    Ok(())
}

fn file_sha1(path: &Path) -> AppResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn bytes_sha1(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}
