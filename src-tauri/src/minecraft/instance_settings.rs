use crate::{
    error::{AppError, AppResult},
    security::{paths::validate_existing_chain, PathRegistry},
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, sync::Arc};

const SETTINGS_FILE: &str = "instance-settings.json";
const SETTINGS_FORMAT_VERSION: u32 = 1;
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
const MAX_JVM_ARGUMENTS: usize = 32;
const MAX_JVM_ARGUMENT_BYTES: usize = 512;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstanceSettings {
    #[serde(default = "settings_format_version")]
    pub format_version: u32,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default = "default_min_ram")]
    pub min_ram_mb: u32,
    #[serde(default = "default_max_ram")]
    pub max_ram_mb: u32,
    #[serde(default)]
    pub jvm_arguments: Vec<String>,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_height")]
    pub height: u32,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub custom_java_executable: Option<String>,
    #[serde(default)]
    pub last_played_at_unix: Option<i64>,
    #[serde(default)]
    pub share_resourcepacks: bool,
    #[serde(default)]
    pub share_worlds: bool,
    #[serde(default)]
    pub share_shaderpacks: bool,
    #[serde(default)]
    pub share_options: bool,
}

impl Default for InstanceSettings {
    fn default() -> Self {
        Self {
            format_version: SETTINGS_FORMAT_VERSION,
            icon: default_icon(),
            min_ram_mb: default_min_ram(),
            max_ram_mb: default_max_ram(),
            jvm_arguments: Vec::new(),
            width: default_width(),
            height: default_height(),
            fullscreen: false,
            custom_java_executable: None,
            last_played_at_unix: None,
            share_resourcepacks: false,
            share_worlds: false,
            share_shaderpacks: false,
            share_options: false,
        }
    }
}

fn settings_format_version() -> u32 { SETTINGS_FORMAT_VERSION }
fn default_icon() -> String { "grass-block".into() }
fn default_min_ram() -> u32 { 512 }
fn default_max_ram() -> u32 { 4096 }
fn default_width() -> u32 { 1280 }
fn default_height() -> u32 { 720 }

#[derive(Clone)]
pub struct InstanceSettingsStore {
    registry: Arc<PathRegistry>,
}

impl InstanceSettingsStore {
    pub fn new(registry: Arc<PathRegistry>) -> Self { Self { registry } }

    pub fn load(&self, profile_id: &str) -> AppResult<InstanceSettings> {
        validate_profile_id(profile_id)?;
        let path = self.settings_path(profile_id)?;
        if !path.absolute().exists() {
            return Ok(InstanceSettings::default());
        }
        validate_existing_chain(path.anchor(), path.absolute())?;
        let metadata = fs::metadata(path.absolute())?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SETTINGS_BYTES {
            return Err(AppError::coded("instance_settings_file_invalid"));
        }
        let settings: InstanceSettings = serde_json::from_slice(&fs::read(path.absolute())?)?;
        validate_settings(&settings)?;
        Ok(settings)
    }

    pub fn save(&self, profile_id: &str, settings: &InstanceSettings) -> AppResult<InstanceSettings> {
        validate_profile_id(profile_id)?;
        validate_settings(settings)?;
        let path = self.settings_path(profile_id)?;
        let parent = path.absolute().parent().ok_or_else(|| AppError::coded("instance_settings_path_invalid"))?;
        validate_existing_chain(path.anchor(), parent)?;
        let temporary = self.registry.resolve("profiles", format!("{profile_id}/{SETTINGS_FILE}.part"))?;
        let backup = self.registry.resolve("profiles", format!("{profile_id}/{SETTINGS_FILE}.previous"))?;
        if temporary.absolute().exists() {
            fs::remove_file(temporary.absolute())?;
        }
        if backup.absolute().exists() {
            fs::remove_file(backup.absolute())?;
        }
        let bytes = serde_json::to_vec_pretty(settings)?;
        if bytes.len() as u64 > MAX_SETTINGS_BYTES {
            return Err(AppError::coded("instance_settings_file_too_large"));
        }
        fs::write(temporary.absolute(), bytes)?;
        validate_existing_chain(temporary.anchor(), temporary.absolute())?;
        if path.absolute().exists() {
            fs::rename(path.absolute(), backup.absolute())?;
        }
        if let Err(error) = fs::rename(temporary.absolute(), path.absolute()) {
            if backup.absolute().exists() {
                let _ = fs::rename(backup.absolute(), path.absolute());
            }
            return Err(error.into());
        }
        let _ = fs::remove_file(backup.absolute());
        Ok(settings.clone())
    }

    pub fn mark_played(&self, profile_id: &str, timestamp: i64) -> AppResult<()> {
        let mut settings = self.load(profile_id)?;
        settings.last_played_at_unix = Some(timestamp);
        self.save(profile_id, &settings).map(|_| ())
    }

    pub fn instance_directory(&self, profile_id: &str, folder: &str) -> AppResult<std::path::PathBuf> {
        validate_profile_id(profile_id)?;
        let relative = match folder {
            "game" => "instance",
            "mods" => "instance/mods",
            "resourcepacks" => "instance/resourcepacks",
            "worlds" => "instance/saves",
            "shaderpacks" => "instance/shaderpacks",
            "screenshots" => "instance/screenshots",
            "logs" => "instance/logs",
            _ => return Err(AppError::coded("instance_folder_kind_invalid")),
        };
        let secure = self.registry.resolve("profiles", Path::new(profile_id).join(relative))?;
        fs::create_dir_all(secure.absolute())?;
        validate_existing_chain(secure.anchor(), secure.absolute())?;
        Ok(secure.absolute().to_path_buf())
    }

    fn settings_path(&self, profile_id: &str) -> AppResult<crate::security::SecurePath> {
        self.registry.resolve("profiles", format!("{profile_id}/{SETTINGS_FILE}"))
    }
}

pub fn validate_settings(settings: &InstanceSettings) -> AppResult<()> {
    if settings.format_version != SETTINGS_FORMAT_VERSION
        || settings.icon.is_empty()
        || settings.icon.len() > 64
        || !settings.icon.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || !(512..=16_384).contains(&settings.min_ram_mb)
        || !(2_048..=16_384).contains(&settings.max_ram_mb)
        || settings.min_ram_mb > settings.max_ram_mb
        || !(320..=7_680).contains(&settings.width)
        || !(240..=4_320).contains(&settings.height)
        || settings.jvm_arguments.len() > MAX_JVM_ARGUMENTS
    {
        return Err(AppError::coded("instance_settings_invalid"));
    }
    for argument in &settings.jvm_arguments {
        let value = argument.trim();
        if value.is_empty()
            || value.len() > MAX_JVM_ARGUMENT_BYTES
            || value.contains('\0')
            || value.starts_with('@')
            || matches!(value, "-cp" | "-classpath" | "-jar")
            || value.starts_with("-Xms")
            || value.starts_with("-Xmx")
        {
            return Err(AppError::coded("instance_jvm_argument_forbidden"));
        }
    }
    if let Some(executable) = settings.custom_java_executable.as_deref() {
        let path = Path::new(executable);
        if executable.len() > 1_024 || !path.is_absolute() || !path.is_file() {
            return Err(AppError::coded("instance_custom_java_invalid"));
        }
    }
    Ok(())
}

fn validate_profile_id(profile_id: &str) -> AppResult<()> {
    if profile_id.is_empty()
        || profile_id.len() > 128
        || !profile_id.is_ascii()
        || !profile_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::coded("runtime_profile_id_invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ram_bounds_and_jvm_ownership_are_enforced() {
        let mut settings = InstanceSettings::default();
        settings.min_ram_mb = 8_192;
        settings.max_ram_mb = 4_096;
        assert!(validate_settings(&settings).is_err());
        settings.min_ram_mb = 512;
        settings.jvm_arguments = vec!["-Xmx12G".into()];
        assert!(validate_settings(&settings).is_err());
    }
}
