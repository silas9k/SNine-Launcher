use crate::{
    error::{AppError, AppResult},
    minecraft::instance_settings::{InstanceSettings, InstanceSettingsStore},
    security::{paths::validate_existing_chain, PathRegistry},
    storage::Storage,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const SHARED_ROOT: &str = "shared-minecraft";
const MAX_SHARED_FILES: usize = 200_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct FileStamp {
    size: u64,
    modified_millis: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DirectoryBaseline {
    files: BTreeMap<String, FileStamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FileBaseline {
    stamp: Option<FileStamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LastLaunchSharing {
    profile_id: String,
    share_resourcepacks: bool,
    share_worlds: bool,
    share_shaderpacks: bool,
    share_options: bool,
}

#[derive(Clone)]
pub struct ProfileSharingService {
    registry: Arc<PathRegistry>,
    storage: Storage,
}

impl ProfileSharingService {
    pub fn new(registry: Arc<PathRegistry>, storage: Storage) -> Self {
        Self { registry, storage }
    }

    pub fn prepare_for_launch(
        &self,
        profile_id: &str,
        settings: &InstanceSettings,
    ) -> AppResult<()> {
        self.ensure_layout()?;
        self.ingest_last_launch()?;
        self.ensure_global_servers_seeded()?;
        self.materialize_shared_file(profile_id, "servers", "servers.dat", "servers.dat")?;

        if settings.share_resourcepacks {
            self.ensure_shared_directory_seeded("resourcepacks", "resourcepacks")?;
            self.materialize_shared_directory(profile_id, "resourcepacks", "resourcepacks")?;
        }
        if settings.share_worlds {
            self.ensure_shared_directory_seeded("worlds", "saves")?;
            self.materialize_shared_directory(profile_id, "worlds", "saves")?;
        }
        if settings.share_shaderpacks {
            self.ensure_shared_directory_seeded("shaderpacks", "shaderpacks")?;
            self.materialize_shared_directory(profile_id, "shaderpacks", "shaderpacks")?;
        }
        if settings.share_options {
            self.ensure_shared_file_seeded("options", "options.txt", "options.txt")?;
            self.materialize_shared_file(profile_id, "options", "options.txt", "options.txt")?;
        }

        self.save_last_launch(&LastLaunchSharing {
            profile_id: profile_id.to_string(),
            share_resourcepacks: settings.share_resourcepacks,
            share_worlds: settings.share_worlds,
            share_shaderpacks: settings.share_shaderpacks,
            share_options: settings.share_options,
        })?;
        Ok(())
    }

    pub fn settings_changed(
        &self,
        profile_id: &str,
        previous: &InstanceSettings,
        next: &InstanceSettings,
    ) -> AppResult<()> {
        self.ensure_layout()?;
        self.ensure_global_servers_seeded()?;
        self.materialize_shared_file(profile_id, "servers", "servers.dat", "servers.dat")?;

        if next.share_resourcepacks && !previous.share_resourcepacks {
            self.ensure_shared_directory_seeded("resourcepacks", "resourcepacks")?;
            self.materialize_shared_directory(profile_id, "resourcepacks", "resourcepacks")?;
        }
        if next.share_worlds && !previous.share_worlds {
            self.ensure_shared_directory_seeded("worlds", "saves")?;
            self.materialize_shared_directory(profile_id, "worlds", "saves")?;
        }
        if next.share_shaderpacks && !previous.share_shaderpacks {
            self.ensure_shared_directory_seeded("shaderpacks", "shaderpacks")?;
            self.materialize_shared_directory(profile_id, "shaderpacks", "shaderpacks")?;
        }
        if next.share_options && !previous.share_options {
            self.ensure_shared_file_seeded("options", "options.txt", "options.txt")?;
            self.materialize_shared_file(profile_id, "options", "options.txt", "options.txt")?;
        }
        Ok(())
    }

    pub fn sync_finished_profile(&self, profile_id: &str) -> AppResult<()> {
        self.ensure_layout()?;
        let settings = InstanceSettingsStore::new(self.registry.clone()).load(profile_id)?;
        self.ensure_global_servers_seeded()?;
        self.ingest_shared_file(profile_id, "servers", "servers.dat", "servers.dat")?;

        if settings.share_resourcepacks {
            self.ingest_shared_directory(profile_id, "resourcepacks", "resourcepacks")?;
        }
        if settings.share_worlds {
            self.ingest_shared_directory(profile_id, "worlds", "saves")?;
        }
        if settings.share_shaderpacks {
            self.ingest_shared_directory(profile_id, "shaderpacks", "shaderpacks")?;
        }
        if settings.share_options {
            self.ingest_shared_file(profile_id, "options", "options.txt", "options.txt")?;
        }
        if self
            .load_last_launch()?
            .is_some_and(|last| last.profile_id == profile_id)
        {
            self.clear_last_launch()?;
        }
        Ok(())
    }

    pub fn inherit_servers_for_new_profile(&self, profile_id: &str) -> AppResult<()> {
        self.ensure_layout()?;
        self.ensure_global_servers_seeded()?;
        self.materialize_shared_file(profile_id, "servers", "servers.dat", "servers.dat")
    }

    pub fn sync_servers_to_inactive_profiles(
        &self,
        running_profile_ids: &[String],
    ) -> AppResult<()> {
        self.ensure_layout()?;

        // If the launcher was closed before the previous process-exit callback ran,
        // ingest that profile before writing the canonical server list back out.
        self.ingest_last_launch_if_inactive(running_profile_ids)?;
        self.ensure_global_servers_seeded()?;

        // The oldest active profile is the stable "standard profile". When it is
        // not running, its servers.dat is authoritative. This also picks up manual
        // edits made outside Minecraft without requiring a launch first.
        if let Some(standard_profile_id) = self.standard_profile_id()? {
            if !running_profile_ids
                .iter()
                .any(|id| id == &standard_profile_id)
            {
                self.ingest_shared_file(
                    &standard_profile_id,
                    "servers",
                    "servers.dat",
                    "servers.dat",
                )?;
            }
        }

        if !self.shared_file("servers.dat")?.is_file() {
            return Ok(());
        }
        for profile in self.storage.profiles()? {
            if profile.lifecycle_state != "active"
                || running_profile_ids.iter().any(|id| id == &profile.id)
            {
                continue;
            }
            self.materialize_shared_file(&profile.id, "servers", "servers.dat", "servers.dat")?;
        }
        Ok(())
    }

    pub fn directory_for_open(
        &self,
        profile_id: &str,
        folder: &str,
        settings: &InstanceSettings,
    ) -> AppResult<Option<PathBuf>> {
        self.ensure_layout()?;
        let shared_kind = match folder {
            "resourcepacks" if settings.share_resourcepacks => Some("resourcepacks"),
            "worlds" if settings.share_worlds => Some("worlds"),
            "shaderpacks" if settings.share_shaderpacks => Some("shaderpacks"),
            _ => None,
        };
        if let Some(kind) = shared_kind {
            let profile_name = match kind {
                "resourcepacks" => "resourcepacks",
                "worlds" => "saves",
                "shaderpacks" => "shaderpacks",
                _ => return Err(AppError::coded("instance_folder_kind_invalid")),
            };
            self.ensure_shared_directory_seeded(kind, profile_name)?;
            let path = self.shared_directory(kind)?;
            fs::create_dir_all(&path)?;
            // The user can mutate this directory outside the launcher after it opens.
            // Mark it dirty so the next launch refreshes the canonical manifest once.
            fs::write(self.shared_directory_dirty_path(kind)?, b"1")?;
            return Ok(Some(path));
        }
        let _ = profile_id;
        Ok(None)
    }

    fn ingest_last_launch(&self) -> AppResult<()> {
        self.ingest_last_launch_if_inactive(&[])
    }

    fn ingest_last_launch_if_inactive(&self, running_profile_ids: &[String]) -> AppResult<()> {
        let Some(last) = self.load_last_launch()? else {
            return Ok(());
        };
        if running_profile_ids.iter().any(|id| id == &last.profile_id) {
            return Ok(());
        }
        if self.storage.profile(&last.profile_id)?.is_none() {
            self.clear_last_launch()?;
            return Ok(());
        }
        self.ensure_global_servers_seeded()?;
        self.ingest_shared_file(&last.profile_id, "servers", "servers.dat", "servers.dat")?;
        if last.share_resourcepacks {
            self.ingest_shared_directory(&last.profile_id, "resourcepacks", "resourcepacks")?;
        }
        if last.share_worlds {
            self.ingest_shared_directory(&last.profile_id, "worlds", "saves")?;
        }
        if last.share_shaderpacks {
            self.ingest_shared_directory(&last.profile_id, "shaderpacks", "shaderpacks")?;
        }
        if last.share_options {
            self.ingest_shared_file(&last.profile_id, "options", "options.txt", "options.txt")?;
        }
        self.clear_last_launch()?;
        Ok(())
    }

    fn ensure_layout(&self) -> AppResult<()> {
        for relative in [
            SHARED_ROOT.to_string(),
            format!("{SHARED_ROOT}/resourcepacks"),
            format!("{SHARED_ROOT}/worlds"),
            format!("{SHARED_ROOT}/shaderpacks"),
            format!("{SHARED_ROOT}/manifests"),
        ] {
            let path = self.registry.resolve("data", relative)?;
            fs::create_dir_all(path.absolute())?;
            validate_existing_chain(path.anchor(), path.absolute())?;
        }
        Ok(())
    }

    fn standard_profile_id(&self) -> AppResult<Option<String>> {
        let mut profiles = self
            .storage
            .profiles()?
            .into_iter()
            .filter(|profile| profile.lifecycle_state == "active")
            .collect::<Vec<_>>();
        profiles.sort_by_key(|profile| (profile.created_at_unix, profile.id.clone()));
        Ok(profiles.first().map(|profile| profile.id.clone()))
    }

    fn ensure_global_servers_seeded(&self) -> AppResult<()> {
        let target = self.shared_file("servers.dat")?;
        if target.is_file() {
            return Ok(());
        }
        if let Some(profile_id) = self.standard_profile_id()? {
            let source = self.profile_path(&profile_id, "servers.dat")?;
            if source.is_file() {
                copy_file_creating_parent(&source, &target)?;
            }
        }
        Ok(())
    }

    fn ensure_shared_file_seeded(
        &self,
        _kind: &str,
        shared_name: &str,
        profile_name: &str,
    ) -> AppResult<()> {
        let target = self.shared_file(shared_name)?;
        if target.is_file() {
            return Ok(());
        }
        if let Some(profile_id) = self.standard_profile_id()? {
            let source = self.profile_path(&profile_id, profile_name)?;
            if source.is_file() {
                copy_file_creating_parent(&source, &target)?;
            }
        }
        Ok(())
    }

    fn ensure_shared_directory_seeded(&self, kind: &str, profile_name: &str) -> AppResult<()> {
        let target = self.shared_directory(kind)?;
        fs::create_dir_all(&target)?;
        if self.load_shared_directory_manifest(kind)?.is_some() {
            return Ok(());
        }
        if let Some(profile_id) = self.standard_profile_id()? {
            let source = self.profile_path(&profile_id, profile_name)?;
            if source.is_dir() {
                copy_tree_full(&source, &target)?;
            }
        }
        let files = scan_tree(&target)?;
        self.save_shared_directory_manifest(kind, &DirectoryBaseline { files })
    }

    fn materialize_shared_file(
        &self,
        profile_id: &str,
        kind: &str,
        shared_name: &str,
        profile_name: &str,
    ) -> AppResult<()> {
        let source = self.shared_file(shared_name)?;
        let target = self.profile_path(profile_id, profile_name)?;
        let canonical_stamp = stamp_for_file(&source)?;
        let baseline = self.load_file_baseline(profile_id, kind)?;
        if baseline != canonical_stamp {
            match canonical_stamp.as_ref() {
                Some(_) => copy_file_creating_parent(&source, &target)?,
                None => {
                    if target.is_file() {
                        fs::remove_file(&target)?;
                    }
                }
            }
        }
        self.save_file_baseline(profile_id, kind, canonical_stamp)
    }

    fn ingest_shared_file(
        &self,
        profile_id: &str,
        kind: &str,
        shared_name: &str,
        profile_name: &str,
    ) -> AppResult<()> {
        let source = self.profile_path(profile_id, profile_name)?;
        let target = self.shared_file(shared_name)?;
        let current = stamp_for_file(&source)?;
        let baseline = self.load_file_baseline(profile_id, kind)?;
        if current != baseline {
            match current.as_ref() {
                Some(_) => copy_file_creating_parent(&source, &target)?,
                None => {
                    if target.is_file() {
                        fs::remove_file(&target)?;
                    }
                }
            }
        }
        self.save_file_baseline(profile_id, kind, stamp_for_file(&target)?)
    }

    fn materialize_shared_directory(
        &self,
        profile_id: &str,
        kind: &str,
        profile_name: &str,
    ) -> AppResult<()> {
        let source = self.shared_directory(kind)?;
        let target = self.profile_path(profile_id, profile_name)?;
        fs::create_dir_all(&source)?;
        fs::create_dir_all(&target)?;
        let canonical = self.shared_directory_snapshot(kind)?;
        let baseline = self.load_directory_baseline(profile_id, kind)?;

        for relative in baseline.files.keys() {
            if !canonical.contains_key(relative) {
                let stale = target.join(relative);
                if stale.is_file() {
                    fs::remove_file(stale)?;
                }
            }
        }
        for (relative, stamp) in &canonical {
            if baseline.files.get(relative) == Some(stamp) {
                continue;
            }
            copy_file_creating_parent(&source.join(relative), &target.join(relative))?;
        }
        self.save_directory_baseline(profile_id, kind, &DirectoryBaseline { files: canonical })
    }

    fn ingest_shared_directory(
        &self,
        profile_id: &str,
        kind: &str,
        profile_name: &str,
    ) -> AppResult<()> {
        let source = self.profile_path(profile_id, profile_name)?;
        let target = self.shared_directory(kind)?;
        fs::create_dir_all(&source)?;
        fs::create_dir_all(&target)?;
        let current = scan_tree(&source)?;
        let baseline = self.load_directory_baseline(profile_id, kind)?;
        let mut canonical = self.shared_directory_snapshot(kind)?;

        for relative in baseline.files.keys() {
            if !current.contains_key(relative) {
                let stale = target.join(relative);
                if stale.is_file() {
                    fs::remove_file(stale)?;
                }
                canonical.remove(relative);
            }
        }
        for (relative, stamp) in &current {
            if baseline.files.get(relative) == Some(stamp) {
                continue;
            }
            copy_file_creating_parent(&source.join(relative), &target.join(relative))?;
            canonical.insert(relative.clone(), stamp.clone());
        }
        let canonical_baseline = DirectoryBaseline { files: canonical };
        self.save_shared_directory_manifest(kind, &canonical_baseline)?;
        self.save_directory_baseline(profile_id, kind, &canonical_baseline)
    }

    fn shared_file(&self, name: &str) -> AppResult<PathBuf> {
        Ok(self
            .registry
            .resolve("data", format!("{SHARED_ROOT}/{name}"))?
            .absolute()
            .to_path_buf())
    }

    fn shared_directory(&self, kind: &str) -> AppResult<PathBuf> {
        Ok(self
            .registry
            .resolve("data", format!("{SHARED_ROOT}/{kind}"))?
            .absolute()
            .to_path_buf())
    }

    fn shared_directory_manifest_path(&self, kind: &str) -> AppResult<PathBuf> {
        Ok(self
            .registry
            .resolve(
                "data",
                format!("{SHARED_ROOT}/manifests/_shared-{kind}.json"),
            )?
            .absolute()
            .to_path_buf())
    }

    fn shared_directory_dirty_path(&self, kind: &str) -> AppResult<PathBuf> {
        Ok(self
            .registry
            .resolve(
                "data",
                format!("{SHARED_ROOT}/manifests/_shared-{kind}.dirty"),
            )?
            .absolute()
            .to_path_buf())
    }

    fn load_shared_directory_manifest(&self, kind: &str) -> AppResult<Option<DirectoryBaseline>> {
        let path = self.shared_directory_manifest_path(kind)?;
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(path)?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::coded("shared_profile_manifest_invalid"))?;
        Ok(Some(value))
    }

    fn save_shared_directory_manifest(
        &self,
        kind: &str,
        value: &DirectoryBaseline,
    ) -> AppResult<()> {
        atomic_json_write(&self.shared_directory_manifest_path(kind)?, value)
    }

    fn shared_directory_snapshot(&self, kind: &str) -> AppResult<BTreeMap<String, FileStamp>> {
        let dirty = self.shared_directory_dirty_path(kind)?;
        if !dirty.is_file() {
            if let Some(manifest) = self.load_shared_directory_manifest(kind)? {
                return Ok(manifest.files);
            }
        }
        let files = scan_tree(&self.shared_directory(kind)?)?;
        self.save_shared_directory_manifest(
            kind,
            &DirectoryBaseline {
                files: files.clone(),
            },
        )?;
        if dirty.is_file() {
            fs::remove_file(dirty)?;
        }
        Ok(files)
    }

    fn profile_path(&self, profile_id: &str, relative: &str) -> AppResult<PathBuf> {
        validate_profile_id(profile_id)?;
        Ok(self
            .registry
            .resolve(
                "profiles",
                Path::new(profile_id).join("instance").join(relative),
            )?
            .absolute()
            .to_path_buf())
    }

    fn baseline_path(&self, profile_id: &str, kind: &str) -> AppResult<PathBuf> {
        validate_profile_id(profile_id)?;
        Ok(self
            .registry
            .resolve(
                "data",
                format!("{SHARED_ROOT}/manifests/{profile_id}-{kind}.json"),
            )?
            .absolute()
            .to_path_buf())
    }

    fn load_directory_baseline(
        &self,
        profile_id: &str,
        kind: &str,
    ) -> AppResult<DirectoryBaseline> {
        let path = self.baseline_path(profile_id, kind)?;
        if !path.is_file() {
            return Ok(DirectoryBaseline::default());
        }
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes)
            .map_err(|_| AppError::coded("shared_profile_manifest_invalid"))
    }

    fn save_directory_baseline(
        &self,
        profile_id: &str,
        kind: &str,
        value: &DirectoryBaseline,
    ) -> AppResult<()> {
        atomic_json_write(&self.baseline_path(profile_id, kind)?, value)
    }

    fn load_file_baseline(&self, profile_id: &str, kind: &str) -> AppResult<Option<FileStamp>> {
        let path = self.baseline_path(profile_id, kind)?;
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(path)?;
        let value: FileBaseline = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::coded("shared_profile_manifest_invalid"))?;
        Ok(value.stamp)
    }

    fn save_file_baseline(
        &self,
        profile_id: &str,
        kind: &str,
        stamp: Option<FileStamp>,
    ) -> AppResult<()> {
        atomic_json_write(
            &self.baseline_path(profile_id, kind)?,
            &FileBaseline { stamp },
        )
    }

    fn last_launch_path(&self) -> AppResult<PathBuf> {
        Ok(self
            .registry
            .resolve("data", format!("{SHARED_ROOT}/last-launch.json"))?
            .absolute()
            .to_path_buf())
    }

    fn load_last_launch(&self) -> AppResult<Option<LastLaunchSharing>> {
        let path = self.last_launch_path()?;
        if !path.is_file() {
            return Ok(None);
        }
        let bytes = fs::read(path)?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|_| AppError::coded("shared_profile_last_launch_invalid"))?;
        Ok(Some(value))
    }

    fn save_last_launch(&self, value: &LastLaunchSharing) -> AppResult<()> {
        atomic_json_write(&self.last_launch_path()?, value)
    }

    fn clear_last_launch(&self) -> AppResult<()> {
        let path = self.last_launch_path()?;
        if path.is_file() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}

fn validate_profile_id(profile_id: &str) -> AppResult<()> {
    if profile_id.is_empty()
        || profile_id.len() > 128
        || !profile_id.is_ascii()
        || !profile_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::coded("runtime_profile_id_invalid"));
    }
    Ok(())
}

fn atomic_json_write<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::coded("shared_profile_path_invalid"))?;
    fs::create_dir_all(parent)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let temporary = path.with_extension(format!("json.part.{}.{}", std::process::id(), nonce));
    fs::write(&temporary, serde_json::to_vec(value)?)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

fn stamp_for_file(path: &Path) -> AppResult<Option<FileStamp>> {
    if !path.is_file() {
        return Ok(None);
    }
    let metadata = fs::metadata(path)?;
    let modified_millis = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0);
    Ok(Some(FileStamp {
        size: metadata.len(),
        modified_millis,
    }))
}

fn scan_tree(root: &Path) -> AppResult<BTreeMap<String, FileStamp>> {
    let mut files = BTreeMap::new();
    if !root.is_dir() {
        return Ok(files);
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            if files.len() >= MAX_SHARED_FILES {
                return Err(AppError::coded("shared_profile_file_limit_exceeded"));
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| AppError::coded("shared_profile_path_invalid"))?
                .to_string_lossy()
                .replace('\\', "/");
            let modified_millis = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_millis().min(u64::MAX as u128) as u64)
                .unwrap_or(0);
            files.insert(
                relative,
                FileStamp {
                    size: metadata.len(),
                    modified_millis,
                },
            );
        }
    }
    Ok(files)
}

fn copy_file_creating_parent(source: &Path, target: &Path) -> AppResult<()> {
    let parent = target
        .parent()
        .ok_or_else(|| AppError::coded("shared_profile_path_invalid"))?;
    fs::create_dir_all(parent)?;
    if target.exists() && target.is_dir() {
        return Err(AppError::coded("shared_profile_target_invalid"));
    }
    fs::copy(source, target)?;
    Ok(())
}

fn copy_tree_full(source: &Path, target: &Path) -> AppResult<()> {
    let files = scan_tree(source)?;
    for relative in files.keys() {
        copy_file_creating_parent(&source.join(relative), &target.join(relative))?;
    }
    Ok(())
}
