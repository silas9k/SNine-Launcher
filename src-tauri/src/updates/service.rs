use super::model::{
    BackupFileV1, ProfileRevisionSummary, ProfileUpdatePreview, RestorePointSummary,
    RestorePointV1, UpdateCenterSnapshot, UpdateChangePreview, UpdateChannelStatus, UpdateMode,
    UpdateOperationResult, UpdatePolicyV1, UpdateProfileSummary,
};
use crate::{
    content_projection::immutable_projected_targets_for_duplicate,
    content_service::Phase6ContentService,
    error::{AppError, AppResult},
    foundation::CoreServices,
    operations::model::{canonical_json, new_identifier},
    profiles::service::{ProfileService, RestoreProfileCopyRequest},
    security::{
        fs as secure_fs,
        paths::{collision_key, validate_existing_chain},
        PathRegistry,
    },
    storage::{models::ProfileRecord, Storage},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

const RESTORE_POINT_FORMAT: &str = "site.s9lab.restore-point";
const MAX_BACKUP_FILES: usize = 32_768;
const MAX_BACKUP_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_BACKUP_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_BACKUP_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct UpdateService {
    registry: Arc<PathRegistry>,
    storage: Storage,
    profiles: ProfileService,
    content: Phase6ContentService,
    settings_file: PathBuf,
}

impl UpdateService {
    pub fn from_core(core: &CoreServices) -> AppResult<Self> {
        Ok(Self {
            registry: core.registry().clone(),
            storage: core.storage().clone(),
            profiles: ProfileService::from_core(core),
            content: Phase6ContentService::from_core(core)?,
            settings_file: core.paths().settings_file.clone(),
        })
    }

    pub fn snapshot(&self) -> AppResult<UpdateCenterSnapshot> {
        let policy = self.load_policy()?;
        let profiles = self
            .profiles
            .list_profiles()?
            .into_iter()
            .filter(|profile| profile.lifecycle_state != "trash")
            .map(|profile| {
                let revisions = self
                    .profiles
                    .committed_revisions(&profile.id)?
                    .into_iter()
                    .map(|revision| ProfileRevisionSummary {
                        active: revision.id == profile.active_revision_id,
                        revision_id: revision.id,
                        created_at_unix: revision.created_at_unix,
                    })
                    .collect();
                Ok(UpdateProfileSummary {
                    profile_id: profile.id,
                    display_name: profile.display_name,
                    active_revision_id: profile.active_revision_id,
                    revisions,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(UpdateCenterSnapshot {
            channels: channel_statuses(&policy),
            policy,
            profiles,
            restore_points: self.list_restore_points()?,
        })
    }

    pub fn save_policy(&self, policy: UpdatePolicyV1) -> AppResult<UpdateCenterSnapshot> {
        if policy.format_version != 1 {
            return Err(AppError::coded("update_policy_version_unsupported"));
        }
        if policy.launcher == UpdateMode::Automatic
            || policy.s9lab_component == UpdateMode::Automatic
        {
            return Err(AppError::coded("update_policy_channel_unavailable"));
        }
        let target = self.registry.resolve("data", "update-policy.json")?;
        let temporary = self.registry.resolve(
            "data",
            format!(".update-policy-{}.tmp", new_identifier("write")),
        )?;
        secure_fs::write_new(&temporary, canonical_json(&policy)?.as_bytes())?;
        let result = crate::platform::atomic_replace(temporary.absolute(), target.absolute());
        if result.is_err() {
            let _ = secure_fs::remove_tree(&temporary);
        }
        result?;
        self.snapshot()
    }

    pub async fn preview(&self, profile_id: &str) -> AppResult<ProfileUpdatePreview> {
        let profile = self.profile_for_read(profile_id)?;
        let base_revision_id = profile
            .active_revision_id
            .clone()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        let snapshot = self.content.snapshot(profile_id)?;
        let snapshot = self
            .content
            .populate_snapshot_updates(profile_id, snapshot)
            .await?;
        let changes = snapshot
            .content
            .into_iter()
            .filter_map(|item| {
                item.update.map(|update| UpdateChangePreview {
                    channel: "content".into(),
                    item_id: item.content_id,
                    display_name: item.display_name,
                    current_version: item.version_number,
                    target_version: update.version_number,
                    verification: "modrinth-sha512-and-launcher-sha256".into(),
                })
            })
            .collect();
        Ok(ProfileUpdatePreview {
            profile_id: profile.id,
            base_revision_id,
            changes,
        })
    }

    pub fn create_restore_point(&self, profile_id: &str) -> AppResult<RestorePointSummary> {
        let profile = self.profile_for_read(profile_id)?;
        if profile.lifecycle_state != "active" {
            return Err(AppError::coded("backup_profile_not_active"));
        }
        let source_revision_id = profile
            .active_revision_id
            .clone()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        let backup_id = new_identifier("backup");
        let staging_relative = format!(".{backup_id}.staging");
        let staging = self.registry.resolve("backups", &staging_relative)?;
        let final_path = self.registry.resolve("backups", &backup_id)?;
        secure_fs::create_directories_within(staging.anchor(), staging.root(), staging.absolute())?;
        let result = (|| -> AppResult<RestorePointSummary> {
            let excluded = immutable_projected_targets_for_duplicate(&self.registry, profile_id)?
                .into_iter()
                .map(|target| collision_key(Path::new(&target)))
                .collect::<AppResult<BTreeSet<_>>>()?;
            let mut files = Vec::new();
            let mut total_bytes = 0u64;
            self.copy_instance_tree(
                profile_id,
                &staging_relative,
                Path::new(""),
                &excluded,
                &mut files,
                &mut total_bytes,
            )?;
            files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
            let document = RestorePointV1 {
                format: RESTORE_POINT_FORMAT.into(),
                format_version: 1,
                backup_id: backup_id.clone(),
                profile_id: profile.id.clone(),
                profile_name: profile.display_name.clone(),
                source_revision_id: source_revision_id.clone(),
                created_at_unix: Utc::now().timestamp(),
                shell_settings: crate::app::config::load_settings_from(&self.settings_file)?
                    .shell_settings(),
                files,
            };
            let descriptor = self
                .registry
                .resolve("backups", format!("{staging_relative}/backup.json"))?;
            secure_fs::write_new(&descriptor, canonical_json(&document)?.as_bytes())?;
            secure_fs::rename_new(&staging, &final_path)?;
            self.pin_revision_cache(&document)?;
            Ok(summary(&document))
        })();
        if result.is_err() {
            let _ = secure_fs::remove_tree(&staging);
            let _ = secure_fs::remove_tree(&final_path);
            let _ = self.storage.remove_cache_references("backup", &backup_id);
        }
        result
    }

    pub fn restore_backup(
        &self,
        backup_id: &str,
        display_name: &str,
        include_account: bool,
        include_settings: bool,
        include_files: bool,
    ) -> AppResult<crate::profiles::model::ProfileSummary> {
        let descriptor_path = self
            .registry
            .resolve("backups", format!("{backup_id}/backup.json"))?;
        let document = read_restore_point(&descriptor_path)?;
        if document.backup_id != backup_id {
            return Err(AppError::coded("backup_identity_mismatch"));
        }
        if include_files {
            self.verify_backup_files(&document)?;
        }
        let expected_files = document
            .files
            .iter()
            .map(|file| {
                Ok((
                    collision_key(Path::new(&file.relative_path))?,
                    (file.size_bytes, file.sha256.clone()),
                ))
            })
            .collect::<AppResult<std::collections::BTreeMap<_, _>>>()?;
        let previous_settings = crate::app::config::load_settings_from(&self.settings_file)?;
        if include_settings {
            let next = previous_settings
                .clone()
                .apply_shell_settings(document.shell_settings.clone())?;
            crate::app::config::save_settings_to(&self.settings_file, &next)?;
        }
        let restored = self
            .profiles
            .restore_profile_copy(RestoreProfileCopyRequest {
                source_profile_id: document.profile_id,
                source_revision_id: document.source_revision_id,
                backup_id: backup_id.to_string(),
                display_name: display_name.to_string(),
                include_files,
                include_account,
                expected_files,
            });
        match restored {
            Ok(profile) => Ok(profile),
            Err(primary) if include_settings => {
                match crate::app::config::save_settings_to(&self.settings_file, &previous_settings)
                {
                    Ok(_) => Err(primary),
                    Err(rollback) => Err(AppError::coded_with(
                        "migration_and_settings_rollback_failed",
                        [
                            ("primary", primary.descriptor().code),
                            ("rollback", rollback.descriptor().code),
                        ],
                    )),
                }
            }
            Err(primary) => Err(primary),
        }
    }

    pub async fn apply_updates(
        &self,
        profile_id: &str,
        content_ids: &[String],
    ) -> AppResult<UpdateOperationResult> {
        if content_ids.is_empty() || content_ids.len() > 256 {
            return Err(AppError::coded("update_selection_invalid"));
        }
        let mut requested = content_ids.to_vec();
        requested.sort();
        requested.dedup();
        if requested.len() != content_ids.len() {
            return Err(AppError::coded("update_selection_duplicate"));
        }
        let preview = self.preview(profile_id).await?;
        let available = preview
            .changes
            .iter()
            .map(|change| change.item_id.as_str())
            .collect::<BTreeSet<_>>();
        if requested
            .iter()
            .any(|content_id| !available.contains(content_id.as_str()))
        {
            return Err(AppError::coded("update_selection_stale"));
        }
        let restore_point = self.create_restore_point(profile_id)?;
        let mut last_operation = None;
        for content_id in &requested {
            match self.content.update(profile_id, content_id).await {
                Ok(result) => last_operation = Some(result),
                Err(primary) => {
                    let rollback = self.profile_for_read(profile_id).and_then(|profile| {
                        if profile.active_revision_id.as_deref()
                            == Some(preview.base_revision_id.as_str())
                        {
                            Ok(())
                        } else {
                            self.profiles
                                .rollback_to_revision(profile_id, &preview.base_revision_id)
                                .map(|_| ())
                        }
                    });
                    return match rollback {
                        Ok(_) => Err(primary),
                        Err(rollback) => Err(AppError::coded_with(
                            "update_and_rollback_failed",
                            [
                                ("primary", primary.descriptor().code),
                                ("rollback", rollback.descriptor().code),
                            ],
                        )),
                    };
                }
            }
        }
        let result = last_operation.ok_or_else(|| AppError::coded("update_selection_invalid"))?;
        Ok(UpdateOperationResult {
            operation_id: result.operation_id,
            profile_id: result.profile_id,
            revision_id: result.revision_id,
            restore_point_id: restore_point.backup_id,
            applied_changes: requested,
        })
    }

    pub async fn run_configured_automatic_updates(&self) -> AppResult<Vec<UpdateOperationResult>> {
        let policy = self.load_policy()?;
        if policy.profiles != UpdateMode::Automatic || policy.content != UpdateMode::Automatic {
            return Ok(Vec::new());
        }
        let profile_ids = self
            .profiles
            .list_profiles()?
            .into_iter()
            .filter(|profile| profile.lifecycle_state == "active")
            .map(|profile| profile.id)
            .collect::<Vec<_>>();
        let mut completed = Vec::new();
        for profile_id in profile_ids {
            let preview = self.preview(&profile_id).await?;
            let content_ids = preview
                .changes
                .into_iter()
                .filter(|change| change.channel == "content")
                .map(|change| change.item_id)
                .collect::<Vec<_>>();
            if !content_ids.is_empty() {
                completed.push(self.apply_updates(&profile_id, &content_ids).await?);
            }
        }
        Ok(completed)
    }

    pub fn rollback(
        &self,
        profile_id: &str,
        revision_id: &str,
    ) -> AppResult<UpdateOperationResult> {
        let restore_point = self.create_restore_point(profile_id)?;
        let (operation_id, new_revision_id) = self
            .profiles
            .rollback_to_revision(profile_id, revision_id)?;
        Ok(UpdateOperationResult {
            operation_id,
            profile_id: profile_id.to_string(),
            revision_id: new_revision_id,
            restore_point_id: restore_point.backup_id,
            applied_changes: vec![format!("rollback:{revision_id}")],
        })
    }

    fn load_policy(&self) -> AppResult<UpdatePolicyV1> {
        let target = self.registry.resolve("data", "update-policy.json")?;
        if !target.absolute().exists() {
            return Ok(UpdatePolicyV1::default());
        }
        validate_existing_chain(target.anchor(), target.absolute())?;
        let metadata = fs::symlink_metadata(target.absolute())?;
        if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 64 * 1024 {
            return Err(AppError::coded("update_policy_invalid"));
        }
        let policy: UpdatePolicyV1 = serde_json::from_slice(&fs::read(target.absolute())?)
            .map_err(|_| AppError::coded("update_policy_invalid"))?;
        if policy.format_version != 1 {
            return Err(AppError::coded("update_policy_version_unsupported"));
        }
        Ok(policy)
    }

    fn list_restore_points(&self) -> AppResult<Vec<RestorePointSummary>> {
        let root = self.registry.root("backups")?;
        validate_existing_chain(root, root)?;
        let mut points = Vec::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err(AppError::coded("backup_entry_invalid"));
            }
            let descriptor = self
                .registry
                .resolve("backups", format!("{name}/backup.json"))?;
            let document = read_restore_point(&descriptor)?;
            if document.backup_id != name {
                return Err(AppError::coded("backup_identity_mismatch"));
            }
            points.push(summary(&document));
        }
        points.sort_by(|left, right| {
            right
                .created_at_unix
                .cmp(&left.created_at_unix)
                .then_with(|| left.backup_id.cmp(&right.backup_id))
        });
        Ok(points)
    }

    fn copy_instance_tree(
        &self,
        profile_id: &str,
        staging_relative: &str,
        relative: &Path,
        excluded: &BTreeSet<String>,
        files: &mut Vec<BackupFileV1>,
        total_bytes: &mut u64,
    ) -> AppResult<()> {
        let mut source_relative = PathBuf::from(profile_id).join("instance");
        if !relative.as_os_str().is_empty() {
            source_relative.push(relative);
        }
        let source = self.registry.resolve("profiles", source_relative)?;
        validate_existing_chain(source.anchor(), source.absolute())?;
        let metadata = fs::symlink_metadata(source.absolute())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(AppError::coded("backup_source_tree_invalid"));
        }
        let mut target_relative = PathBuf::from(staging_relative).join("instance");
        if !relative.as_os_str().is_empty() {
            target_relative.push(relative);
        }
        let target = self.registry.resolve("backups", target_relative)?;
        secure_fs::create_directories_within(target.anchor(), target.root(), target.absolute())?;
        for entry in fs::read_dir(source.absolute())? {
            let entry = entry?;
            let child = relative.join(entry.file_name());
            if child.starts_with(".s9lab") || excluded.contains(&collision_key(&child)?) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                return Err(AppError::coded("backup_symlink_forbidden"));
            }
            if metadata.is_dir() {
                self.copy_instance_tree(
                    profile_id,
                    staging_relative,
                    &child,
                    excluded,
                    files,
                    total_bytes,
                )?;
                continue;
            }
            if !metadata.is_file() || metadata.len() > MAX_BACKUP_FILE_BYTES {
                return Err(AppError::coded("backup_file_invalid"));
            }
            if files.len() >= MAX_BACKUP_FILES {
                return Err(AppError::coded("backup_file_count_exceeded"));
            }
            *total_bytes = total_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| AppError::coded("backup_size_overflow"))?;
            if *total_bytes > MAX_BACKUP_TOTAL_BYTES {
                return Err(AppError::coded("backup_total_size_exceeded"));
            }
            let source = self.registry.resolve(
                "profiles",
                PathBuf::from(profile_id).join("instance").join(&child),
            )?;
            let destination = self.registry.resolve(
                "backups",
                PathBuf::from(staging_relative)
                    .join("instance")
                    .join(&child),
            )?;
            let copied = secure_fs::copy_new(&source, &destination)?;
            if copied != metadata.len() {
                return Err(AppError::coded("backup_source_changed"));
            }
            let (size_bytes, sha256) = hash_file(destination.absolute())?;
            if size_bytes != copied {
                return Err(AppError::coded("backup_source_changed"));
            }
            files.push(BackupFileV1 {
                relative_path: child.to_string_lossy().replace('\\', "/"),
                size_bytes,
                sha256,
            });
        }
        Ok(())
    }

    fn pin_revision_cache(&self, document: &RestorePointV1) -> AppResult<()> {
        let cache_blobs = self
            .profiles
            .verified_revision_cache_blobs(&document.profile_id, &document.source_revision_id)?;
        self.storage.replace_cache_references(
            "backup",
            &document.backup_id,
            &cache_blobs
                .into_iter()
                .map(|blob| blob.sha256)
                .collect::<Vec<_>>(),
        )
    }

    fn verify_backup_files(&self, document: &RestorePointV1) -> AppResult<()> {
        let expected = document
            .files
            .iter()
            .map(|file| {
                (
                    file.relative_path.clone(),
                    (file.size_bytes, file.sha256.clone()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let instance = self
            .registry
            .resolve("backups", format!("{}/instance", document.backup_id))?;
        let mut actual = std::collections::BTreeMap::new();
        collect_backup_files(
            &self.registry,
            &document.backup_id,
            instance.absolute(),
            Path::new(""),
            &mut actual,
        )?;
        if actual != expected {
            return Err(AppError::coded("backup_content_mismatch"));
        }
        Ok(())
    }

    fn profile_for_read(&self, profile_id: &str) -> AppResult<ProfileRecord> {
        self.storage
            .profile(profile_id)?
            .ok_or_else(|| AppError::coded_with("profile_not_found", [("profileId", profile_id)]))
    }
}

fn channel_statuses(policy: &UpdatePolicyV1) -> Vec<UpdateChannelStatus> {
    vec![
        UpdateChannelStatus {
            channel: "launcher".into(),
            mode: policy.launcher,
            state: "unconfigured".into(),
            reason_code: Some("launcher_update_trust_not_configured".into()),
        },
        UpdateChannelStatus {
            channel: "profiles".into(),
            mode: policy.profiles,
            state: "available".into(),
            reason_code: None,
        },
        UpdateChannelStatus {
            channel: "s9lab-component".into(),
            mode: policy.s9lab_component,
            state: "unconfigured".into(),
            reason_code: Some("s9lab_component_provider_unconfigured".into()),
        },
        UpdateChannelStatus {
            channel: "content".into(),
            mode: policy.content,
            state: "available".into(),
            reason_code: None,
        },
    ]
}

fn read_restore_point(path: &crate::security::SecurePath) -> AppResult<RestorePointV1> {
    validate_existing_chain(path.anchor(), path.absolute())?;
    let metadata = fs::symlink_metadata(path.absolute())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_BACKUP_DOCUMENT_BYTES {
        return Err(AppError::coded("backup_document_invalid"));
    }
    let document: RestorePointV1 = serde_json::from_slice(&fs::read(path.absolute())?)
        .map_err(|_| AppError::coded("backup_document_invalid"))?;
    if document.format != RESTORE_POINT_FORMAT
        || document.format_version != 1
        || document.files.len() > MAX_BACKUP_FILES
    {
        return Err(AppError::coded("backup_document_invalid"));
    }
    let mut keys = BTreeSet::new();
    let mut total = 0u64;
    for file in &document.files {
        let key = collision_key(Path::new(&file.relative_path))?;
        if !keys.insert(key)
            || file.size_bytes > MAX_BACKUP_FILE_BYTES
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || file.sha256.bytes().any(|byte| byte.is_ascii_uppercase())
        {
            return Err(AppError::coded("backup_document_invalid"));
        }
        total = total
            .checked_add(file.size_bytes)
            .ok_or_else(|| AppError::coded("backup_size_overflow"))?;
    }
    if total > MAX_BACKUP_TOTAL_BYTES {
        return Err(AppError::coded("backup_total_size_exceeded"));
    }
    Ok(document)
}

fn summary(document: &RestorePointV1) -> RestorePointSummary {
    RestorePointSummary {
        backup_id: document.backup_id.clone(),
        profile_id: document.profile_id.clone(),
        profile_name: document.profile_name.clone(),
        source_revision_id: document.source_revision_id.clone(),
        created_at_unix: document.created_at_unix,
        file_count: document.files.len() as u32,
        size_bytes: document.files.iter().map(|file| file.size_bytes).sum(),
    }
}

fn hash_file(path: &Path) -> AppResult<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size = size
            .checked_add(count as u64)
            .ok_or_else(|| AppError::coded("backup_size_overflow"))?;
        hasher.update(&buffer[..count]);
    }
    Ok((size, hex::encode(hasher.finalize())))
}

fn collect_backup_files(
    registry: &PathRegistry,
    backup_id: &str,
    directory: &Path,
    relative: &Path,
    files: &mut std::collections::BTreeMap<String, (u64, String)>,
) -> AppResult<()> {
    validate_existing_chain(registry.root("backups")?, directory)?;
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AppError::coded("backup_content_mismatch"));
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::coded("backup_symlink_forbidden"));
        }
        if metadata.is_dir() {
            collect_backup_files(registry, backup_id, &entry.path(), &child_relative, files)?;
        } else if metadata.is_file() {
            if files.len() >= MAX_BACKUP_FILES {
                return Err(AppError::coded("backup_file_count_exceeded"));
            }
            let source = registry.resolve(
                "backups",
                PathBuf::from(backup_id)
                    .join("instance")
                    .join(&child_relative),
            )?;
            let (size, sha256) = hash_file(source.absolute())?;
            let key = child_relative.to_string_lossy().replace('\\', "/");
            if files.insert(key, (size, sha256)).is_some() {
                return Err(AppError::coded("backup_content_mismatch"));
            }
        } else {
            return Err(AppError::coded("backup_special_file_forbidden"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_point_is_verified_and_restores_an_isolated_profile_copy() {
        let root = crate::foundation::test_root("phase7-backup-restore");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let source = profiles.create_profile("Phase 7 source").expect("source");
        let source_instance = root.join("profiles").join(&source.id).join("instance");
        fs::create_dir_all(source_instance.join("saves/World")).expect("world directory");
        fs::write(
            source_instance.join("saves/World/level.dat"),
            b"backup-world",
        )
        .expect("world");
        fs::write(
            source_instance.join("config/options.txt"),
            b"backup-settings",
        )
        .expect("settings");
        fs::create_dir_all(source_instance.join(".s9lab")).expect("internal directory");

        let updates = UpdateService::from_core(&core).expect("updates");
        let point = updates
            .create_restore_point(&source.id)
            .expect("restore point");
        assert_eq!(point.file_count, 2);
        assert_eq!(point.size_bytes, 27);
        assert!(!root
            .join("backups")
            .join(&point.backup_id)
            .join("instance/.s9lab")
            .exists());

        fs::write(
            source_instance.join("saves/World/level.dat"),
            b"changed-world",
        )
        .expect("change source");
        let restored = updates
            .restore_backup(&point.backup_id, "Recovered copy", false, false, true)
            .expect("restore copy");
        let restored_instance = root.join("profiles").join(restored.id).join("instance");
        assert_eq!(
            fs::read(restored_instance.join("saves/World/level.dat")).expect("restored world"),
            b"backup-world"
        );
        assert_eq!(
            fs::read(source_instance.join("saves/World/level.dat")).expect("source world"),
            b"changed-world"
        );
        assert!(!restored_instance.join(".s9lab").exists());

        let descriptor = core
            .registry()
            .resolve("backups", format!("{}/backup.json", point.backup_id))
            .expect("backup descriptor");
        let document = read_restore_point(&descriptor).expect("restore document");
        let mut wrong_expected = document
            .files
            .iter()
            .map(|file| {
                (
                    collision_key(Path::new(&file.relative_path)).expect("collision key"),
                    (file.size_bytes, file.sha256.clone()),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        wrong_expected.values_mut().next().expect("backup file").1 = "0".repeat(64);
        let profile_count = profiles.list_profiles().expect("profiles before").len();
        let copy_error = profiles
            .restore_profile_copy(RestoreProfileCopyRequest {
                source_profile_id: document.profile_id,
                source_revision_id: document.source_revision_id,
                backup_id: point.backup_id.clone(),
                display_name: "Changed during restore".into(),
                include_files: true,
                include_account: false,
                expected_files: wrong_expected,
            })
            .expect_err("copied payload must remain manifest-bound");
        assert_eq!(copy_error.descriptor().code, "backup_content_mismatch");
        assert_eq!(
            profiles.list_profiles().expect("profiles after").len(),
            profile_count
        );

        fs::write(
            root.join("backups")
                .join(&point.backup_id)
                .join("instance/config/options.txt"),
            b"tampered",
        )
        .expect("tamper backup");
        let error = updates
            .restore_backup(&point.backup_id, "Tampered copy", false, false, true)
            .expect_err("tampered backup must fail");
        assert_eq!(error.descriptor().code, "backup_content_mismatch");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn update_policy_round_trips_atomically_and_rejects_blocked_automation() {
        let root = crate::foundation::test_root("phase7-update-policy");
        let core = CoreServices::open_fixed(&root).expect("core");
        let updates = UpdateService::from_core(&core).expect("updates");
        let policy = UpdatePolicyV1 {
            format_version: 1,
            launcher: crate::updates::model::UpdateMode::Manual,
            profiles: crate::updates::model::UpdateMode::Automatic,
            s9lab_component: crate::updates::model::UpdateMode::Manual,
            content: crate::updates::model::UpdateMode::Automatic,
        };
        let snapshot = updates.save_policy(policy.clone()).expect("save policy");
        assert_eq!(snapshot.policy, policy);
        assert_eq!(updates.snapshot().expect("reload").policy, policy);
        let launcher = snapshot
            .channels
            .iter()
            .find(|channel| channel.channel == "launcher")
            .expect("launcher channel");
        assert_eq!(launcher.state, "unconfigured");
        assert_eq!(launcher.mode, crate::updates::model::UpdateMode::Manual);
        let blocked = UpdatePolicyV1 {
            launcher: crate::updates::model::UpdateMode::Automatic,
            ..policy
        };
        let error = updates
            .save_policy(blocked)
            .expect_err("blocked launcher automation");
        assert_eq!(error.descriptor().code, "update_policy_channel_unavailable");
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[tokio::test]
    async fn automatic_content_updates_require_both_channel_authorizations() {
        let root = crate::foundation::test_root("phase7-update-policy-pair");
        let core = CoreServices::open_fixed(&root).expect("core");
        let updates = UpdateService::from_core(&core).expect("updates");
        for policy in [
            UpdatePolicyV1 {
                profiles: UpdateMode::Automatic,
                ..UpdatePolicyV1::default()
            },
            UpdatePolicyV1 {
                content: UpdateMode::Automatic,
                ..UpdatePolicyV1::default()
            },
        ] {
            updates.save_policy(policy).expect("save partial policy");
            assert!(updates
                .run_configured_automatic_updates()
                .await
                .expect("partial policy is a no-op")
                .is_empty());
        }
        fs::remove_dir_all(root).expect("cleanup");
    }
}
