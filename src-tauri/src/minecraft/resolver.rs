use crate::{
    error::{AppError, AppResult},
    minecraft::provider::{
        maven_artifact_path, parse_sha1_sidecar, parse_sha256_sidecar, validate_url,
        validate_version_identifier, ControlledHttpClient, ControlledProvider, DigestExpectation,
    },
    security::paths::normalize_relative_path,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const MAX_VERSION_JSON_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ASSET_INDEX_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LIBRARY_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
const METADATA_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeArtifactKind {
    Client,
    Library,
    Native,
    AssetIndex,
    AssetObject,
    LoggingConfig,
    LoaderLibrary,
    Installer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeArtifactSource {
    pub logical_id: String,
    pub provider: String,
    pub url: String,
    pub target_relative_path: String,
    pub size_bytes: u64,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub kind: RuntimeArtifactKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMinecraftVersion {
    pub minecraft_version: String,
    pub release_type: String,
    pub main_class: String,
    pub java_major: u32,
    pub asset_index_id: String,
    pub artifacts: Vec<RuntimeArtifactSource>,
    pub game_arguments: Vec<LaunchArgument>,
    pub jvm_arguments: Vec<LaunchArgument>,
    pub legacy_game_arguments: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LaunchArgument {
    Plain(String),
    Conditional {
        rules: Vec<LaunchRule>,
        value: LaunchArgumentValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LaunchArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchRule {
    pub action: String,
    #[serde(default)]
    pub os: Option<LaunchOsRule>,
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchOsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionManifest {
    versions: Vec<VersionManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct VersionManifestEntry {
    id: String,
    url: String,
    sha1: String,
    #[serde(rename = "type")]
    release_type: String,
}

#[derive(Debug, Clone, Deserialize)]
struct FabricLoaderCatalogEntry {
    loader: FabricLoaderCatalogVersion,
}

#[derive(Debug, Clone, Deserialize)]
struct FabricLoaderCatalogVersion {
    version: String,
    stable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftCatalogEntry {
    pub version: String,
    pub release_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderCatalogEntry {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MojangVersionDocument {
    id: String,
    #[serde(rename = "type", default = "default_release_type")]
    release_type: String,
    main_class: String,
    asset_index: MojangAssetIndex,
    downloads: MojangVersionDownloads,
    #[serde(default)]
    libraries: Vec<MojangLibrary>,
    #[serde(default)]
    arguments: Option<MojangArguments>,
    #[serde(default)]
    minecraft_arguments: Option<String>,
    #[serde(default)]
    java_version: Option<MojangJavaVersion>,
    #[serde(default)]
    logging: Option<MojangLogging>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MojangAssetIndex {
    id: String,
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangVersionDownloads {
    client: MojangDownload,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangDownload {
    sha1: String,
    size: u64,
    url: String,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangLibrary {
    name: String,
    #[serde(default)]
    downloads: Option<MojangLibraryDownloads>,
    #[serde(default)]
    natives: BTreeMap<String, String>,
    #[serde(default)]
    rules: Vec<LaunchRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangLibraryDownloads {
    #[serde(default)]
    artifact: Option<MojangDownload>,
    #[serde(default)]
    classifiers: BTreeMap<String, MojangDownload>,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangArguments {
    #[serde(default)]
    game: Vec<LaunchArgument>,
    #[serde(default)]
    jvm: Vec<LaunchArgument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MojangJavaVersion {
    major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangLogging {
    client: MojangLoggingClient,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangLoggingClient {
    argument: String,
    file: MojangLoggingFile,
}

#[derive(Debug, Clone, Deserialize)]
struct MojangLoggingFile {
    id: String,
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetIndexDocument {
    objects: BTreeMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FabricProfileDocument {
    id: String,
    main_class: String,
    #[serde(default)]
    arguments: Option<MojangArguments>,
    #[serde(default)]
    libraries: Vec<FabricLibrary>,
}

#[derive(Debug, Clone, Deserialize)]
struct FabricLibrary {
    name: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    sha1: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedLoader {
    pub loader_version: String,
    pub profile_id: String,
    pub main_class: String,
    pub artifacts: Vec<RuntimeArtifactSource>,
    pub game_arguments: Vec<LaunchArgument>,
    pub jvm_arguments: Vec<LaunchArgument>,
}

#[derive(Clone)]
pub struct RuntimeResolver {
    http: ControlledHttpClient,
    version_manifest_cache: Arc<Mutex<Option<(Instant, VersionManifest)>>>,
    fabric_catalog_cache: Arc<Mutex<BTreeMap<String, (Instant, Vec<LoaderCatalogEntry>)>>>,
}

impl RuntimeResolver {
    pub fn production() -> AppResult<Self> {
        Ok(Self {
            http: ControlledHttpClient::production()?,
            version_manifest_cache: Arc::new(Mutex::new(None)),
            fabric_catalog_cache: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    async fn version_manifest(&self) -> AppResult<VersionManifest> {
        if let Some((cached_at, manifest)) = self
            .version_manifest_cache
            .lock()
            .map_err(|_| AppError::coded("runtime_metadata_cache_poisoned"))?
            .as_ref()
            .filter(|(cached_at, _)| cached_at.elapsed() <= METADATA_CACHE_TTL)
        {
            let _ = cached_at;
            return Ok(manifest.clone());
        }
        let manifest: VersionManifest = self
            .http
            .get_json(ControlledProvider::MojangMetadata, VERSION_MANIFEST_URL)
            .await?;
        *self
            .version_manifest_cache
            .lock()
            .map_err(|_| AppError::coded("runtime_metadata_cache_poisoned"))? =
            Some((Instant::now(), manifest.clone()));
        Ok(manifest)
    }

    pub async fn resolve_mojang(
        &self,
        minecraft_version: &str,
    ) -> AppResult<ResolvedMinecraftVersion> {
        validate_version_identifier(minecraft_version)?;
        let manifest = self.version_manifest().await?;
        let entry = manifest
            .versions
            .into_iter()
            .find(|entry| entry.id == minecraft_version)
            .ok_or_else(|| AppError::coded("runtime_minecraft_version_not_found"))?;
        validate_url(ControlledProvider::MojangMetadata, &entry.url)?;
        let document = self
            .http
            .get_verified(
                ControlledProvider::MojangMetadata,
                &entry.url,
                None,
                MAX_VERSION_JSON_BYTES,
                &DigestExpectation {
                    sha1: Some(entry.sha1),
                    sha256: None,
                },
            )
            .await?;
        let version: MojangVersionDocument = serde_json::from_slice(&document.bytes)?;
        resolve_mojang_document(minecraft_version, version)
    }

    pub async fn minecraft_catalog(&self) -> AppResult<Vec<MinecraftCatalogEntry>> {
        let manifest = self.version_manifest().await?;
        let mut result = Vec::new();
        for entry in manifest.versions {
            if !matches!(entry.release_type.as_str(), "release" | "snapshot") {
                continue;
            }
            // Mojang retains a handful of historical display labels containing
            // spaces. They are never safe runtime identifiers, so they must not
            // invalidate the current catalog or enter a later filesystem path.
            if validate_version_identifier(&entry.id).is_err() {
                continue;
            }
            result.push(MinecraftCatalogEntry {
                version: entry.id,
                release_type: entry.release_type,
            });
        }
        if result.is_empty() {
            return Err(AppError::coded("runtime_minecraft_catalog_empty"));
        }
        Ok(result)
    }

    pub async fn fabric_catalog(
        &self,
        minecraft_version: &str,
    ) -> AppResult<Vec<LoaderCatalogEntry>> {
        validate_version_identifier(minecraft_version)?;
        if let Some((_, entries)) = self
            .fabric_catalog_cache
            .lock()
            .map_err(|_| AppError::coded("runtime_metadata_cache_poisoned"))?
            .get(minecraft_version)
            .filter(|(cached_at, _)| cached_at.elapsed() <= METADATA_CACHE_TTL)
        {
            return Ok(entries.clone());
        }
        let url = format!("https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}");
        let entries: Vec<FabricLoaderCatalogEntry> = self
            .http
            .get_json(ControlledProvider::FabricMetadata, &url)
            .await?;
        let mut result = entries
            .into_iter()
            .map(|entry| {
                validate_version_identifier(&entry.loader.version)?;
                Ok(LoaderCatalogEntry {
                    version: entry.loader.version,
                    stable: entry.loader.stable,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        result.sort_by(|left, right| right.version.cmp(&left.version));
        result.dedup_by(|left, right| left.version == right.version);
        if result.is_empty() {
            return Err(AppError::coded("runtime_fabric_catalog_empty"));
        }
        self.fabric_catalog_cache
            .lock()
            .map_err(|_| AppError::coded("runtime_metadata_cache_poisoned"))?
            .insert(
                minecraft_version.to_string(),
                (Instant::now(), result.clone()),
            );
        Ok(result)
    }

    pub async fn resolve_assets(
        &self,
        index: &RuntimeArtifactSource,
    ) -> AppResult<Vec<RuntimeArtifactSource>> {
        if index.kind != RuntimeArtifactKind::AssetIndex || index.provider != "mojang" {
            return Err(AppError::coded("runtime_asset_index_invalid"));
        }
        let document = self
            .http
            .get_verified(
                ControlledProvider::MojangContent,
                &index.url,
                Some(index.size_bytes),
                MAX_ASSET_INDEX_BYTES,
                &DigestExpectation {
                    sha1: index.sha1.clone(),
                    sha256: index.sha256.clone(),
                },
            )
            .await?;
        let index: AssetIndexDocument = serde_json::from_slice(&document.bytes)?;
        resolve_asset_objects(index)
    }

    pub async fn resolve_fabric(
        &self,
        minecraft_version: &str,
        loader_version: &str,
    ) -> AppResult<ResolvedLoader> {
        validate_version_identifier(minecraft_version)?;
        validate_version_identifier(loader_version)?;
        let url = format!(
            "https://meta.fabricmc.net/v2/versions/loader/{minecraft_version}/{loader_version}/profile/json"
        );
        let profile: FabricProfileDocument = self
            .http
            .get_json(ControlledProvider::FabricMetadata, &url)
            .await?;
        let mut artifacts = Vec::with_capacity(profile.libraries.len());
        for library in profile.libraries {
            let path = maven_artifact_path(&library.name)?;
            let base = library
                .url
                .as_deref()
                .unwrap_or("https://maven.fabricmc.net/")
                .trim_end_matches('/');
            let artifact_url = format!("{base}/{path}");
            validate_url(ControlledProvider::FabricMaven, &artifact_url)?;
            let checksum_url = format!("{artifact_url}.sha256");
            let sha256 = match self
                .http
                .get_verified(
                    ControlledProvider::FabricMaven,
                    &checksum_url,
                    None,
                    512,
                    &DigestExpectation::default(),
                )
                .await
            {
                Ok(checksum) => Some(parse_sha256_sidecar(&checksum.bytes)?),
                Err(_) => None,
            };
            let sha1 = match library.sha1 {
                Some(sha1) => {
                    validate_sha1(&sha1)?;
                    Some(sha1)
                }
                None => {
                    let checksum = self
                        .http
                        .get_verified(
                            ControlledProvider::FabricMaven,
                            &format!("{artifact_url}.sha1"),
                            None,
                            512,
                            &DigestExpectation::default(),
                        )
                        .await?;
                    Some(parse_sha1_sidecar(&checksum.bytes)?)
                }
            };
            let size = match library.size {
                Some(size) => size,
                None => {
                    self.http
                        .head_size(
                            ControlledProvider::FabricMaven,
                            &artifact_url,
                            MAX_LIBRARY_BYTES,
                        )
                        .await?
                }
            };
            let target = compact_library_target(
                sha1.as_deref()
                    .ok_or_else(|| AppError::coded("runtime_sha1_invalid"))?,
            )?;
            artifacts.push(RuntimeArtifactSource {
                logical_id: library.name,
                provider: "fabric".into(),
                url: artifact_url,
                target_relative_path: target,
                size_bytes: size,
                sha1,
                sha256,
                kind: RuntimeArtifactKind::LoaderLibrary,
            });
        }
        Ok(ResolvedLoader {
            loader_version: loader_version.to_string(),
            profile_id: profile.id,
            main_class: profile.main_class,
            artifacts,
            game_arguments: profile
                .arguments
                .as_ref()
                .map(|arguments| arguments.game.clone())
                .unwrap_or_default(),
            jvm_arguments: profile
                .arguments
                .map(|arguments| arguments.jvm)
                .unwrap_or_default(),
        })
    }

    pub async fn resolve_neoforge_installer(
        &self,
        minecraft_version: &str,
        loader_version: &str,
    ) -> AppResult<RuntimeArtifactSource> {
        validate_version_identifier(minecraft_version)?;
        validate_version_identifier(loader_version)?;
        if !neoforge_version_matches_minecraft(minecraft_version, loader_version) {
            return Err(AppError::coded("runtime_loader_version_incompatible"));
        }
        let path = maven_artifact_path(&format!(
            "net.neoforged:neoforge:{loader_version}:installer:jar"
        ))?;
        let url = format!("https://maven.neoforged.net/releases/{path}");
        validate_url(ControlledProvider::NeoforgeMaven, &url)?;
        let checksum = self
            .http
            .get_verified(
                ControlledProvider::NeoforgeMaven,
                &format!("{url}.sha256"),
                None,
                512,
                &DigestExpectation::default(),
            )
            .await?;
        let sha256 = parse_sha256_sidecar(&checksum.bytes)?;
        let size = self
            .http
            .head_size(ControlledProvider::NeoforgeMaven, &url, MAX_LIBRARY_BYTES)
            .await?;
        Ok(RuntimeArtifactSource {
            logical_id: format!("net.neoforged:neoforge:{loader_version}:installer"),
            provider: "neoforge".into(),
            url,
            target_relative_path: format!("installers/neoforge/{loader_version}.jar"),
            size_bytes: size,
            sha1: None,
            sha256: Some(sha256),
            kind: RuntimeArtifactKind::Installer,
        })
    }
}

fn resolve_mojang_document(
    requested_version: &str,
    version: MojangVersionDocument,
) -> AppResult<ResolvedMinecraftVersion> {
    if !matches!(version.release_type.as_str(), "release" | "snapshot") {
        return Err(AppError::coded(
            "runtime_minecraft_release_type_unsupported",
        ));
    }
    if version.id != requested_version {
        return Err(AppError::coded("runtime_version_identity_mismatch"));
    }
    validate_version_identifier(&version.id)?;
    validate_identifier(&version.main_class, "runtime_main_class_invalid")?;
    validate_identifier(&version.asset_index.id, "runtime_asset_index_id_invalid")?;
    let mut artifacts = Vec::new();
    artifacts.push(resolve_mojang_download(
        format!("minecraft-client:{}", version.id),
        format!("versions/{0}/{0}.jar", version.id),
        version.downloads.client,
        RuntimeArtifactKind::Client,
    )?);
    artifacts.push(resolve_mojang_download(
        format!("asset-index:{}", version.asset_index.id),
        format!("assets/indexes/{}.json", version.asset_index.id),
        MojangDownload {
            sha1: version.asset_index.sha1,
            size: version.asset_index.size,
            url: version.asset_index.url,
            path: None,
        },
        RuntimeArtifactKind::AssetIndex,
    )?);
    for library in version.libraries {
        if !rules_allow_current_platform(&library.rules) {
            continue;
        }
        if let Some(downloads) = library.downloads {
            if let Some(artifact) = downloads.artifact {
                let path = artifact
                    .path
                    .clone()
                    .ok_or_else(|| AppError::coded("runtime_library_path_missing"))?;
                validate_relative_target(&format!("libraries/{path}"))?;
                let target = compact_library_target(&artifact.sha1)?;
                artifacts.push(resolve_mojang_download(
                    library.name.clone(),
                    target,
                    artifact,
                    RuntimeArtifactKind::Library,
                )?);
            }
            if let Some(classifier) = native_classifier(&library.natives) {
                if let Some(native) = downloads.classifiers.get(&classifier) {
                    let path = native
                        .path
                        .clone()
                        .ok_or_else(|| AppError::coded("runtime_native_path_missing"))?;
                    validate_relative_target(&format!("libraries/{path}"))?;
                    let target = compact_library_target(&native.sha1)?;
                    artifacts.push(resolve_mojang_download(
                        format!("{}:{classifier}", library.name),
                        target,
                        native.clone(),
                        RuntimeArtifactKind::Native,
                    )?);
                }
            }
        }
    }
    let logging_argument = if let Some(logging) = version.logging {
        let argument = normalize_logging_argument(&logging.client.argument)?;
        artifacts.push(resolve_mojang_download(
            format!("logging:{}", logging.client.file.id),
            format!("assets/log_configs/{}", logging.client.file.id),
            MojangDownload {
                sha1: logging.client.file.sha1,
                size: logging.client.file.size,
                url: logging.client.file.url,
                path: None,
            },
            RuntimeArtifactKind::LoggingConfig,
        )?);
        Some(argument)
    } else {
        None
    };
    let mut arguments = version.arguments.unwrap_or(MojangArguments {
        game: Vec::new(),
        jvm: Vec::new(),
    });
    if let Some(argument) = logging_argument {
        arguments.jvm.push(LaunchArgument::Plain(argument));
    }
    Ok(ResolvedMinecraftVersion {
        minecraft_version: version.id,
        release_type: version.release_type,
        main_class: version.main_class,
        java_major: version
            .java_version
            .map(|java| java.major_version)
            .unwrap_or(8),
        asset_index_id: version.asset_index.id,
        artifacts,
        game_arguments: arguments.game,
        jvm_arguments: arguments.jvm,
        legacy_game_arguments: version.minecraft_arguments,
    })
}

fn default_release_type() -> String {
    "release".into()
}

fn normalize_logging_argument(value: &str) -> AppResult<String> {
    if value.is_empty()
        || value.len() > 1024
        || !value.is_ascii()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b'\0')
        || value.matches("${path}").count() != 1
    {
        return Err(AppError::coded("runtime_logging_argument_invalid"));
    }
    let normalized = value.replace("${path}", "${logging_config}");
    if normalized.contains("${path}") {
        return Err(AppError::coded("runtime_logging_argument_invalid"));
    }
    Ok(normalized)
}

fn resolve_mojang_download(
    logical_id: String,
    target_relative_path: String,
    download: MojangDownload,
    kind: RuntimeArtifactKind,
) -> AppResult<RuntimeArtifactSource> {
    validate_url(ControlledProvider::MojangContent, &download.url)?;
    validate_sha1(&download.sha1)?;
    validate_relative_target(&target_relative_path)?;
    if download.size == 0 || download.size > MAX_LIBRARY_BYTES {
        return Err(AppError::coded("runtime_artifact_size_invalid"));
    }
    Ok(RuntimeArtifactSource {
        logical_id,
        provider: "mojang".into(),
        url: download.url,
        target_relative_path,
        size_bytes: download.size,
        sha1: Some(download.sha1),
        sha256: None,
        kind,
    })
}

fn compact_library_target(sha1: &str) -> AppResult<String> {
    validate_sha1(sha1)?;
    Ok(format!("libraries/{}/{}.jar", &sha1[..2], sha1))
}

fn resolve_asset_objects(index: AssetIndexDocument) -> AppResult<Vec<RuntimeArtifactSource>> {
    let mut unique = BTreeMap::<String, u64>::new();
    for object in index.objects.into_values() {
        validate_sha1(&object.hash)?;
        if object.size == 0 || object.size > MAX_ASSET_BYTES {
            return Err(AppError::coded("runtime_asset_size_invalid"));
        }
        if let Some(previous) = unique.insert(object.hash.clone(), object.size) {
            if previous != object.size {
                return Err(AppError::coded("runtime_asset_metadata_conflict"));
            }
        }
    }
    unique
        .into_iter()
        .map(|(hash, size)| {
            let prefix = &hash[..2];
            let url = format!("https://resources.download.minecraft.net/{prefix}/{hash}");
            validate_url(ControlledProvider::MojangContent, &url)?;
            Ok(RuntimeArtifactSource {
                logical_id: format!("asset:{hash}"),
                provider: "mojang".into(),
                url,
                target_relative_path: format!("assets/objects/{prefix}/{hash}"),
                size_bytes: size,
                sha1: Some(hash),
                sha256: None,
                kind: RuntimeArtifactKind::AssetObject,
            })
        })
        .collect()
}

pub fn rules_allow_current_platform(rules: &[LaunchRule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches_current_platform(rule) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn rule_matches_current_platform(rule: &LaunchRule) -> bool {
    if !rule.features.is_empty() {
        return false;
    }
    let Some(os) = &rule.os else {
        return true;
    };
    let current_name = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };
    if os.name.as_deref().is_some_and(|name| name != current_name) {
        return false;
    }
    let current_arch = if cfg!(target_pointer_width = "64") {
        "x86_64"
    } else {
        "x86"
    };
    if os
        .arch
        .as_deref()
        .is_some_and(|arch| arch != current_arch && !(arch == "x86" && current_arch == "x86_64"))
    {
        return false;
    }
    os.version.is_none()
}

fn native_classifier(natives: &BTreeMap<String, String>) -> Option<String> {
    let current_name = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    };
    natives.get(current_name).map(|value| {
        value.replace(
            "${arch}",
            if cfg!(target_pointer_width = "64") {
                "64"
            } else {
                "32"
            },
        )
    })
}

fn neoforge_version_matches_minecraft(minecraft: &str, loader: &str) -> bool {
    let mut minecraft_parts = minecraft.split('.');
    let major = minecraft_parts.next();
    let minor = minecraft_parts.next();
    let mut loader_parts = loader.split('.');
    match (major, minor, loader_parts.next(), loader_parts.next()) {
        (Some("1"), Some(mc_minor), Some(loader_minor), Some(_)) => mc_minor == loader_minor,
        _ => false,
    }
}

fn validate_relative_target(value: &str) -> AppResult<()> {
    let normalized = normalize_relative_path(Path::new(value))
        .map_err(|_| AppError::coded("runtime_target_path_invalid"))?;
    let canonical = normalized.to_string_lossy().replace('\\', "/");
    if value.contains('\\') || canonical != value {
        return Err(AppError::coded("runtime_target_path_invalid"));
    }
    Ok(())
}

fn validate_identifier(value: &str, code: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'$'))
    {
        return Err(AppError::coded(code));
    }
    Ok(())
}

fn validate_sha1(value: &str) -> AppResult<()> {
    validate_sha1_optional(Some(value))
}

fn validate_sha1_optional(value: Option<&str>) -> AppResult<()> {
    if value.is_some_and(|hash| {
        hash.len() != 40
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) {
        return Err(AppError::coded("runtime_sha1_invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_a_minimal_mojang_document_without_trusting_paths() {
        let document: MojangVersionDocument = serde_json::from_value(serde_json::json!({
            "id": "1.21.4",
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {
                "id": "17",
                "sha1": "0123456789abcdef0123456789abcdef01234567",
                "size": 42,
                "url": "https://piston-data.mojang.com/v1/packages/asset-index.json"
            },
            "downloads": {
                "client": {
                    "sha1": "1123456789abcdef0123456789abcdef01234567",
                    "size": 123,
                    "url": "https://piston-data.mojang.com/v1/objects/client.jar"
                }
            },
            "libraries": [{
                "name": "org.example:library:1.0",
                "downloads": {
                    "artifact": {
                        "sha1": "2123456789abcdef0123456789abcdef01234567",
                        "size": 12,
                        "url": "https://libraries.minecraft.net/org/example/library/1.0/library-1.0.jar",
                        "path": "org/example/library/1.0/library-1.0.jar"
                    }
                }
            }],
            "arguments": {"game": ["--demo"], "jvm": ["-Ddemo=true"]},
            "javaVersion": {"majorVersion": 21}
        }))
        .expect("fixture");
        let resolved = resolve_mojang_document("1.21.4", document).expect("resolve");
        assert_eq!(resolved.java_major, 21);
        assert_eq!(resolved.artifacts.len(), 3);
        assert_eq!(resolved.artifacts[2].kind, RuntimeArtifactKind::Library);
        assert_eq!(
            resolved.artifacts[2].target_relative_path,
            "libraries/21/2123456789abcdef0123456789abcdef01234567.jar"
        );
    }

    #[test]
    fn compact_library_targets_are_hash_bound_and_path_safe() {
        assert_eq!(
            compact_library_target("0123456789abcdef0123456789abcdef01234567")
                .expect("compact target"),
            "libraries/01/0123456789abcdef0123456789abcdef01234567.jar"
        );
        assert!(compact_library_target("../library.jar").is_err());
    }

    #[test]
    fn rejects_mojang_library_traversal() {
        let document: MojangVersionDocument = serde_json::from_value(serde_json::json!({
            "id": "1.21.4",
            "mainClass": "net.minecraft.client.main.Main",
            "assetIndex": {
                "id": "17",
                "sha1": "0123456789abcdef0123456789abcdef01234567",
                "size": 42,
                "url": "https://piston-data.mojang.com/v1/packages/asset-index.json"
            },
            "downloads": {
                "client": {
                    "sha1": "1123456789abcdef0123456789abcdef01234567",
                    "size": 123,
                    "url": "https://piston-data.mojang.com/v1/objects/client.jar"
                }
            },
            "libraries": [{
                "name": "org.example:library:1.0",
                "downloads": {
                    "artifact": {
                        "sha1": "2123456789abcdef0123456789abcdef01234567",
                        "size": 12,
                        "url": "https://libraries.minecraft.net/evil.jar",
                        "path": "../../evil.jar"
                    }
                }
            }]
        }))
        .expect("fixture");
        assert!(resolve_mojang_document("1.21.4", document).is_err());
    }

    #[test]
    fn asset_index_deduplicates_identical_hashes_and_rejects_conflicts() {
        let hash = "0123456789abcdef0123456789abcdef01234567";
        let okay: AssetIndexDocument = serde_json::from_value(serde_json::json!({
            "objects": {
                "one": {"hash": hash, "size": 12},
                "two": {"hash": hash, "size": 12}
            }
        }))
        .expect("fixture");
        assert_eq!(resolve_asset_objects(okay).expect("assets").len(), 1);
        let conflict: AssetIndexDocument = serde_json::from_value(serde_json::json!({
            "objects": {
                "one": {"hash": hash, "size": 12},
                "two": {"hash": hash, "size": 13}
            }
        }))
        .expect("fixture");
        assert!(resolve_asset_objects(conflict).is_err());
    }

    #[test]
    fn neoforge_versions_must_match_the_minecraft_minor() {
        assert!(neoforge_version_matches_minecraft("1.21.1", "21.1.200"));
        assert!(!neoforge_version_matches_minecraft("1.20.1", "21.1.200"));
        assert!(!neoforge_version_matches_minecraft("1.21.1", "../21.1"));
    }

    #[test]
    fn logging_argument_is_rewritten_to_a_controlled_runtime_placeholder() {
        assert_eq!(
            normalize_logging_argument("-Dlog4j.configurationFile=${path}")
                .expect("valid Mojang logging argument"),
            "-Dlog4j.configurationFile=${logging_config}"
        );
        for invalid in [
            "-Dlog4j.configurationFile=relative.xml",
            "${path}${path}",
            "${other}",
            "line\n${path}",
        ] {
            assert!(
                normalize_logging_argument(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn current_mojang_logging_shape_is_deserialized() {
        let logging: MojangLogging = serde_json::from_value(serde_json::json!({
            "client": {
                "argument": "-Dlog4j.configurationFile=${path}",
                "file": {
                    "id": "client-1.21.2.xml",
                    "sha1": "39384bd14c0606d812afec88d8aff595b2587dd9",
                    "size": 1073,
                    "url": "https://piston-data.mojang.com/v1/objects/39384bd14c0606d812afec88d8aff595b2587dd9/client-1.21.2.xml"
                },
                "type": "log4j2-xml"
            }
        }))
        .expect("current Mojang logging format");

        assert_eq!(logging.client.argument, "-Dlog4j.configurationFile=${path}");
    }

    #[test]
    fn feature_bound_rules_fail_closed() {
        let rule = LaunchRule {
            action: "allow".into(),
            os: None,
            features: BTreeMap::from([("is_demo_user".into(), false)]),
        };
        assert!(!rules_allow_current_platform(&[rule]));
    }

    #[tokio::test]
    #[ignore = "manual production runtime catalog probe"]
    async fn production_minecraft_catalog_is_retrievable() {
        let resolver = RuntimeResolver::production().expect("runtime resolver");
        let catalog = resolver
            .minecraft_catalog()
            .await
            .expect("Minecraft catalog");
        assert!(!catalog.is_empty());
    }

    #[tokio::test]
    #[ignore = "manual production Minecraft metadata probe"]
    async fn production_minecraft_1_21_11_metadata_is_resolvable() {
        let resolver = RuntimeResolver::production().expect("runtime resolver");
        let resolved = resolver
            .resolve_mojang("1.21.11")
            .await
            .expect("Minecraft 1.21.11 metadata");
        assert_eq!(resolved.minecraft_version, "1.21.11");
        assert!(!resolved.artifacts.is_empty());
    }

    #[tokio::test]
    #[ignore = "manual production Fabric metadata probe"]
    async fn production_fabric_1_21_11_profile_is_resolvable() {
        let resolver = RuntimeResolver::production().expect("runtime resolver");
        let loader = resolver
            .fabric_catalog("1.21.11")
            .await
            .expect("Fabric catalog")
            .into_iter()
            .find(|entry| entry.stable)
            .expect("stable Fabric loader");
        let resolved = resolver
            .resolve_fabric("1.21.11", &loader.version)
            .await
            .expect("Fabric profile");
        assert!(!resolved.artifacts.is_empty());
    }
}
