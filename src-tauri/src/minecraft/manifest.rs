use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2/versions/loader";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadInfo {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndex,
    pub downloads: VersionDownloads,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments", default)]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub logging: Option<LoggingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: DownloadInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetIndex {
    pub id: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaVersion {
    #[serde(default)]
    pub component: Option<String>,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default)]
    pub natives: HashMap<String, String>,
    #[serde(default)]
    pub extract: Option<ExtractRules>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<DownloadInfo>,
    #[serde(default)]
    pub classifiers: HashMap<String, DownloadInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractRules {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<RuleOs>,
    #[serde(default)]
    pub features: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleOs {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub client: ClientLoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientLoggingConfig {
    pub argument: String,
    pub file: LoggingFile,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingFile {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricProfile {
    pub id: String,
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    #[serde(default)]
    pub libraries: Vec<FabricLibrary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricLibrary {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize)]
struct VersionEntry {
    id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderInfo,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderInfo {
    version: String,
    stable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FeatureSet {
    pub values: HashMap<String, bool>,
}

impl FeatureSet {
    pub fn launch_defaults() -> Self {
        let mut values = HashMap::new();
        values.insert("has_custom_resolution".to_string(), true);
        values.insert("is_demo_user".to_string(), false);
        values.insert("has_quick_plays_support".to_string(), false);
        values.insert("is_quick_play_singleplayer".to_string(), false);
        values.insert("is_quick_play_multiplayer".to_string(), false);
        values.insert("is_quick_play_realms".to_string(), false);
        Self { values }
    }
}

pub async fn fetch_version(client: &Client, version_id: &str) -> AppResult<VersionJson> {
    let manifest: VersionManifest = client
        .get(VERSION_MANIFEST_URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|entry| entry.id == version_id)
        .ok_or_else(|| {
            AppError::Message(format!(
                "Minecraft-Version {version_id} wurde im Mojang-Manifest nicht gefunden."
            ))
        })?;
    Ok(client
        .get(entry.url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

pub async fn fetch_fabric_profile(
    client: &Client,
    game_version: &str,
    minimum_loader: &str,
) -> AppResult<(String, FabricProfile)> {
    let list_url = format!("{FABRIC_META_URL}/{game_version}");
    let entries: Vec<FabricLoaderEntry> = client
        .get(list_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let selected = entries
        .into_iter()
        .filter(|entry| version_at_least(&entry.loader.version, minimum_loader))
        .max_by(|a, b| {
            a.loader
                .stable
                .cmp(&b.loader.stable)
                .then_with(|| compare_versions(&a.loader.version, &b.loader.version))
        })
        .ok_or_else(|| {
            AppError::Message(format!(
                "Kein kompatibler Fabric Loader ab Version {minimum_loader} gefunden."
            ))
        })?;
    let loader = selected.loader.version;
    let profile_url = format!("{FABRIC_META_URL}/{game_version}/{loader}/profile/json");
    let profile: FabricProfile = client
        .get(profile_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok((loader, profile))
}

pub fn rules_allow(rules: &[Rule], features: &FeatureSet) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule, features) {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn rule_matches(rule: &Rule, features: &FeatureSet) -> bool {
    if let Some(os) = &rule.os {
        if os
            .name
            .as_deref()
            .is_some_and(|name| name != current_os_name())
        {
            return false;
        }
        if let Some(arch) = os.arch.as_deref() {
            let current = if cfg!(target_pointer_width = "64") {
                "x86_64"
            } else {
                "x86"
            };
            if arch != current && !(arch == "x86" && current == "x86_64") {
                return false;
            }
        }
        // Mojang-OS-Versionsregeln sind selten. Ohne zuverlässige native
        // Versionsabfrage wird eine explizite Versionsregel nicht angewendet.
        if os.version.is_some() {
            return false;
        }
    }
    rule.features
        .iter()
        .all(|(name, expected)| features.values.get(name).copied().unwrap_or(false) == *expected)
}

pub fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

pub fn native_classifier(library: &Library) -> Option<String> {
    library.natives.get(current_os_name()).map(|value| {
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

pub fn flatten_arguments(arguments: &[Argument], features: &FeatureSet) -> Vec<String> {
    let mut output = Vec::new();
    for argument in arguments {
        match argument {
            Argument::Plain(value) => output.push(value.clone()),
            Argument::Conditional { rules, value } if rules_allow(rules, features) => match value {
                ArgumentValue::One(value) => output.push(value.clone()),
                ArgumentValue::Many(values) => output.extend(values.clone()),
            },
            Argument::Conditional { .. } => {}
        }
    }
    output
}

pub fn maven_path(coordinate: &str) -> AppResult<String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 {
        return Err(AppError::Message(format!(
            "Ungültige Maven-Koordinate: {coordinate}"
        )));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    let extension = parts.get(4).copied().unwrap_or("jar");
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

pub fn fabric_download(library: &FabricLibrary) -> AppResult<DownloadInfo> {
    let path = maven_path(&library.name)?;
    let base = library
        .url
        .as_deref()
        .unwrap_or("https://maven.fabricmc.net/")
        .trim_end_matches('/');
    Ok(DownloadInfo {
        sha1: library.sha1.clone(),
        size: library.size,
        url: format!("{base}/{path}"),
        path: Some(path),
    })
}

fn version_at_least(version: &str, minimum: &str) -> bool {
    compare_versions(version, minimum) != std::cmp::Ordering::Less
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split(|character: char| !character.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let mut left = parse(a);
    let mut right = parse(b);
    let max = left.len().max(right.len());
    left.resize(max, 0);
    right.resize(max, 0);
    left.cmp(&right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_loader_versions_numerically() {
        assert!(version_at_least("0.19.3", "0.19.3"));
        assert!(version_at_least("0.20.0", "0.19.3"));
        assert!(!version_at_least("0.16.14", "0.19.3"));
    }

    #[test]
    fn creates_maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.19.3").unwrap(),
            "net/fabricmc/fabric-loader/0.19.3/fabric-loader-0.19.3.jar"
        );
    }
}
