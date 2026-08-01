use crate::{
    app::paths::LauncherPaths,
    error::{AppError, AppResult},
    foundation::CoreServices,
    operations::{
        engine::OperationEngine,
        model::{canonical_json, new_identifier, sha256_hex, ProfileInstallPlan},
    },
    profiles::model::{LockedCacheBlob, ProfileLockV1, ProfileManifestV1, ProfileSummary},
    security::{fs as secure_fs, paths::validate_existing_chain, PathRegistry},
    storage::{models::ProfileRecord, Storage},
};
use chrono::Utc;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

pub const MUTABLE_INSTANCE_DIRECTORIES: &[&str] = &[
    "instance",
    "instance/mods",
    "instance/config",
    "instance/saves",
    "instance/resourcepacks",
    "instance/shaderpacks",
    "instance/datapacks",
    "instance/screenshots",
    "instance/logs",
    "instance/crash-reports",
];

#[derive(Clone)]
pub struct ProfileService {
    paths: LauncherPaths,
    registry: Arc<PathRegistry>,
    storage: Storage,
    operations: OperationEngine,
}

impl ProfileService {
    pub fn from_core(core: &CoreServices) -> Self {
        Self {
            paths: core.paths().clone(),
            registry: core.registry().clone(),
            storage: core.storage().clone(),
            operations: core.operations().clone(),
        }
    }

    pub fn list_profiles(&self) -> AppResult<Vec<ProfileSummary>> {
        self.storage
            .profiles()?
            .into_iter()
            .map(profile_summary)
            .collect()
    }

    pub fn create_profile(&self, display_name: &str) -> AppResult<ProfileSummary> {
        let account_id = self.storage.selected_account_id()?;
        self.create_profile_internal(display_name, None, account_id, Vec::new())
    }

    pub fn duplicate_profile(
        &self,
        source_profile_id: &str,
        display_name: &str,
    ) -> AppResult<ProfileSummary> {
        let source = self
            .storage
            .profile(source_profile_id)?
            .ok_or_else(|| profile_not_found(source_profile_id))?;
        if source.lifecycle_state == "trash" {
            return Err(AppError::coded("profile_duplicate_from_trash_forbidden"));
        }
        let cache_blobs = self.read_active_cache_projection(&source)?;
        self.create_profile_internal(
            display_name,
            Some(source_profile_id),
            source.account_id.clone(),
            cache_blobs,
        )
    }

    pub fn archive_profile(&self, profile_id: &str) -> AppResult<ProfileSummary> {
        profile_summary(self.storage.archive_profile(profile_id)?)
    }

    pub fn trash_profile(&self, profile_id: &str) -> AppResult<ProfileSummary> {
        profile_summary(self.storage.trash_profile(profile_id)?)
    }

    pub fn restore_profile(&self, profile_id: &str) -> AppResult<ProfileSummary> {
        profile_summary(self.storage.restore_profile(profile_id)?)
    }

    pub fn set_favorite(&self, profile_id: &str, favorite: bool) -> AppResult<ProfileSummary> {
        self.storage.set_profile_favorite(profile_id, favorite)?;
        profile_summary(
            self.storage
                .profile(profile_id)?
                .ok_or_else(|| profile_not_found(profile_id))?,
        )
    }

    fn create_profile_internal(
        &self,
        display_name: &str,
        source_profile_id: Option<&str>,
        account_id: Option<String>,
        cache_blobs: Vec<LockedCacheBlob>,
    ) -> AppResult<ProfileSummary> {
        let display_name = validate_display_name(display_name)?;
        let profile_id = new_identifier("profile");
        let profile_root = self.registry.resolve("profiles", &profile_id)?;
        secure_fs::create_directories_within(
            profile_root.anchor(),
            profile_root.root(),
            profile_root.absolute(),
        )?;
        if let Err(error) =
            self.storage
                .create_profile_with_metadata(&profile_id, &display_name, source_profile_id)
        {
            let _ = secure_fs::remove_tree(&profile_root);
            return Err(error);
        }

        let preparation = (|| -> AppResult<()> {
            if let Some(account_id) = account_id.as_deref() {
                self.storage
                    .assign_profile_account(&profile_id, Some(account_id))?;
            }
            self.create_mutable_layout(&profile_id)?;
            if let Some(source_profile_id) = source_profile_id {
                self.copy_mutable_instance(source_profile_id, &profile_id)?;
                self.create_mutable_layout(&profile_id)?;
            }
            Ok(())
        })();
        if let Err(error) = preparation {
            return Err(self.cleanup_unactivated_profile(&profile_id, error));
        }

        let plan = match build_profile_plan(
            &profile_id,
            &display_name,
            source_profile_id,
            account_id.as_deref(),
            cache_blobs,
        ) {
            Ok(plan) => plan,
            Err(error) => return Err(self.cleanup_unactivated_profile(&profile_id, error)),
        };
        if let Err(error) = self.operations.plan_profile_revision(&plan) {
            return Err(self.cleanup_unactivated_profile(&profile_id, error));
        }
        self.operations.execute(&plan.operation_id)?;
        let record = self
            .storage
            .profile(&profile_id)?
            .ok_or_else(|| profile_not_found(&profile_id))?;
        profile_summary(record)
    }

    fn cleanup_unactivated_profile(&self, profile_id: &str, primary: AppError) -> AppError {
        let database_cleanup = self.storage.delete_unactivated_profile(profile_id);
        let filesystem_cleanup = self
            .registry
            .resolve("profiles", profile_id)
            .and_then(|path| secure_fs::remove_tree(&path));
        match (database_cleanup, filesystem_cleanup) {
            (Ok(()), Ok(())) => primary,
            (database_cleanup, filesystem_cleanup) => AppError::coded_with(
                "profile_creation_and_cleanup_failed",
                [
                    ("primary", primary.descriptor().code),
                    (
                        "databaseCleanup",
                        database_cleanup
                            .err()
                            .map(|error| error.descriptor().code)
                            .unwrap_or_else(|| "ok".to_string()),
                    ),
                    (
                        "filesystemCleanup",
                        filesystem_cleanup
                            .err()
                            .map(|error| error.descriptor().code)
                            .unwrap_or_else(|| "ok".to_string()),
                    ),
                ],
            ),
        }
    }

    fn create_mutable_layout(&self, profile_id: &str) -> AppResult<()> {
        for relative in MUTABLE_INSTANCE_DIRECTORIES {
            let directory = self
                .registry
                .resolve("profiles", format!("{profile_id}/{relative}"))?;
            secure_fs::create_directories_within(
                directory.anchor(),
                directory.root(),
                directory.absolute(),
            )?;
        }
        Ok(())
    }

    fn copy_mutable_instance(
        &self,
        source_profile_id: &str,
        target_profile_id: &str,
    ) -> AppResult<()> {
        let source = self
            .registry
            .resolve("profiles", format!("{source_profile_id}/instance"))?;
        let target = self
            .registry
            .resolve("profiles", format!("{target_profile_id}/instance"))?;
        copy_directory_tree(
            &self.registry,
            source.anchor(),
            source.absolute(),
            target.absolute(),
            Path::new(""),
            source_profile_id,
            target_profile_id,
        )
    }

    fn read_active_cache_projection(
        &self,
        source: &ProfileRecord,
    ) -> AppResult<Vec<LockedCacheBlob>> {
        let revision_id = source
            .active_revision_id
            .as_deref()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        let lock = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{revision_id}/lock.json", source.id),
        )?;
        validate_existing_chain(lock.anchor(), lock.absolute())?;
        let projection: CacheProjection = serde_json::from_slice(&fs::read(lock.absolute())?)?;
        Ok(projection.cache_blobs)
    }

    pub fn root(&self) -> &Path {
        &self.paths.root
    }
}

fn build_profile_plan(
    profile_id: &str,
    display_name: &str,
    source_profile_id: Option<&str>,
    account_id: Option<&str>,
    mut cache_blobs: Vec<LockedCacheBlob>,
) -> AppResult<ProfileInstallPlan> {
    cache_blobs.sort();
    cache_blobs.dedup();
    let operation_id = new_identifier("op");
    let revision_id = new_identifier("rev");
    let manifest = ProfileManifestV1 {
        format: "site.s9lab.profile".into(),
        format_version: 1,
        profile_id: profile_id.to_string(),
        display_name: display_name.to_string(),
        created_at_unix: Utc::now().timestamp(),
        source_profile_id: source_profile_id.map(str::to_string),
        account_id: account_id.map(str::to_string),
        mutable_directories: MUTABLE_INSTANCE_DIRECTORIES
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        isolation_policy: "verified-copy-no-hardlinks".into(),
    };
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
    let lock = ProfileLockV1 {
        format: "site.s9lab.profile-lock".into(),
        format_version: 1,
        profile_id: profile_id.to_string(),
        revision_id: revision_id.clone(),
        manifest_sha256: manifest_sha256.clone(),
        resolution_state: "phase4-foundation".into(),
        cache_blobs,
    };
    let lock_json = canonical_json(&lock)?;
    let lock_sha256 = sha256_hex(lock_json.as_bytes());
    Ok(ProfileInstallPlan {
        operation_id,
        profile_id: profile_id.to_string(),
        revision_id,
        previous_revision_id: None,
        manifest_json,
        manifest_sha256,
        lock_json,
        lock_sha256,
        payload_files: Vec::new(),
        cache_materializations: Vec::new(),
        runtime_projection: None,
        previous_runtime_projection: None,
        cleanup_profile_on_rollback: true,
    })
}

fn copy_directory_tree(
    registry: &PathRegistry,
    anchor: &Path,
    source_root: &Path,
    target_root: &Path,
    relative: &Path,
    source_profile_id: &str,
    target_profile_id: &str,
) -> AppResult<()> {
    let source_directory = source_root.join(relative);
    validate_existing_chain(anchor, &source_directory)?;
    let metadata = fs::symlink_metadata(&source_directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::coded("profile_copy_source_tree_invalid"));
    }
    let target_directory = target_root.join(relative);
    let mut target_relative = PathBuf::from(target_profile_id).join("instance");
    if !relative.as_os_str().is_empty() {
        target_relative.push(relative);
    }
    let target = registry.resolve("profiles", target_relative)?;
    secure_fs::create_directories_within(target.anchor(), target.root(), &target_directory)?;
    for entry in fs::read_dir(source_directory)? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::coded("profile_copy_symlink_forbidden"));
        }
        if metadata.is_dir() {
            copy_directory_tree(
                registry,
                anchor,
                source_root,
                target_root,
                &child_relative,
                source_profile_id,
                target_profile_id,
            )?;
        } else if metadata.is_file() {
            let source = registry.resolve(
                "profiles",
                PathBuf::from(source_profile_id)
                    .join("instance")
                    .join(&child_relative),
            )?;
            let destination = registry.resolve(
                "profiles",
                PathBuf::from(target_profile_id)
                    .join("instance")
                    .join(&child_relative),
            )?;
            secure_fs::copy_new(&source, &destination)?;
        } else {
            return Err(AppError::coded("profile_copy_special_file_forbidden"));
        }
    }
    Ok(())
}

fn validate_display_name(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 64 || value.chars().any(char::is_control) {
        return Err(AppError::coded("profile_display_name_invalid"));
    }
    Ok(value.to_string())
}

fn profile_summary(record: ProfileRecord) -> AppResult<ProfileSummary> {
    let active_revision_id = record
        .active_revision_id
        .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
    Ok(ProfileSummary {
        id: record.id,
        display_name: record.display_name,
        lifecycle_state: record.lifecycle_state,
        active_revision_id,
        account_id: record.account_id,
        favorite: record.favorite,
        verification_state: record.verification_state,
        source_profile_id: record.source_profile_id,
        created_at_unix: record.created_at_unix,
        updated_at_unix: record.updated_at_unix,
    })
}

fn profile_not_found(profile_id: &str) -> AppError {
    AppError::coded_with("profile_not_found", [("profileId", profile_id.to_string())])
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheProjection {
    #[serde(default)]
    cache_blobs: Vec<LockedCacheBlob>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn profile_lifecycle_and_duplicate_instance_are_isolated() {
        let root = crate::foundation::test_root("phase4-profile-isolation");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let source = profiles.create_profile("Source").expect("source");
        let source_config = root
            .join("profiles")
            .join(&source.id)
            .join("instance/config/options.txt");
        let source_world = root
            .join("profiles")
            .join(&source.id)
            .join("instance/saves/World/level.dat");
        fs::create_dir_all(source_world.parent().expect("world parent")).expect("world directory");
        fs::write(&source_config, b"source-value").expect("source config");
        fs::write(&source_world, b"source-world").expect("source world");
        let duplicate = profiles
            .duplicate_profile(&source.id, "Duplicate")
            .expect("duplicate");
        let duplicate_config = root
            .join("profiles")
            .join(&duplicate.id)
            .join("instance/config/options.txt");
        let duplicate_world = root
            .join("profiles")
            .join(&duplicate.id)
            .join("instance/saves/World/level.dat");
        assert_eq!(
            fs::read(&duplicate_config).expect("duplicate data"),
            b"source-value"
        );
        assert_eq!(
            fs::read(&duplicate_world).expect("duplicate world"),
            b"source-world"
        );
        fs::write(&duplicate_config, b"duplicate-value").expect("mutate duplicate");
        fs::write(&duplicate_world, b"duplicate-world").expect("mutate duplicate world");
        assert_eq!(
            fs::read(&source_config).expect("source unchanged"),
            b"source-value"
        );
        assert_eq!(
            fs::read(&source_world).expect("source world unchanged"),
            b"source-world"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_ne!(
                fs::metadata(&source_config).expect("source metadata").ino(),
                fs::metadata(&duplicate_config)
                    .expect("duplicate metadata")
                    .ino()
            );
        }

        let archived = profiles.archive_profile(&source.id).expect("archive");
        assert_eq!(archived.lifecycle_state, "archived");
        let trashed = profiles.trash_profile(&source.id).expect("trash");
        assert_eq!(trashed.lifecycle_state, "trash");
        let restored = profiles.restore_profile(&source.id).expect("restore");
        assert_eq!(restored.lifecycle_state, "archived");
        let active = profiles.restore_profile(&source.id).expect("unarchive");
        assert_eq!(active.lifecycle_state, "active");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profile_duplicate_accepts_empty_root_relative_path_without_relaxing_separator_checks() {
        let root = crate::foundation::test_root("phase4-profile-duplicate-root");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let source = profiles.create_profile("Source").expect("source");
        let source_marker = root
            .join("profiles")
            .join(&source.id)
            .join("instance/root-marker.txt");
        fs::write(&source_marker, b"root-marker").expect("source root marker");

        let duplicate = profiles
            .duplicate_profile(&source.id, "Duplicate")
            .expect("duplicate");
        let duplicate_marker = root
            .join("profiles")
            .join(&duplicate.id)
            .join("instance/root-marker.txt");
        assert_eq!(
            fs::read(duplicate_marker).expect("duplicate root marker"),
            b"root-marker"
        );

        let ambiguous = core
            .registry()
            .resolve("profiles", format!("{}/instance//config", duplicate.id))
            .expect_err("ambiguous separator");
        assert_eq!(ambiguous.descriptor().code, "path_ambiguous_separator");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_new_profile_revision_removes_projection_and_mutable_files() {
        use crate::operations::model::{FailAt, FailurePoint, OperationState};

        let root = crate::foundation::test_root("phase4-profile-rollback-cleanup");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let profile_id = new_identifier("profile");
        let profile_root = core
            .registry()
            .resolve("profiles", &profile_id)
            .expect("profile root");
        secure_fs::create_directories_within(
            profile_root.anchor(),
            profile_root.root(),
            profile_root.absolute(),
        )
        .expect("profile directory");
        core.storage()
            .create_profile_with_metadata(&profile_id, "Rollback profile", None)
            .expect("profile projection");
        profiles
            .create_mutable_layout(&profile_id)
            .expect("mutable layout");
        let marker = root
            .join("profiles")
            .join(&profile_id)
            .join("instance/config/marker.txt");
        fs::write(&marker, b"incomplete").expect("marker");

        let plan = build_profile_plan(&profile_id, "Rollback profile", None, None, Vec::new())
            .expect("plan");
        core.operations()
            .plan_profile_revision(&plan)
            .expect("register plan");
        let error = core
            .operations()
            .execute_controlled_with_injector(
                &plan.operation_id,
                &FailAt(FailurePoint::AfterRevisionMoved),
            )
            .expect_err("injected failure");
        assert_eq!(error.descriptor().code, "operation_injected_failure");
        assert!(core
            .storage()
            .profile(&profile_id)
            .expect("profile query")
            .is_none());
        assert!(!root.join("profiles").join(&profile_id).exists());
        let operation = core
            .storage()
            .operation(&plan.operation_id)
            .expect("operation query")
            .expect("operation");
        assert_eq!(operation.state, OperationState::RolledBack);
        assert!(operation.profile_id.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn one_thousand_profile_projection_meets_the_phase4_budget() {
        let root = crate::foundation::test_root("phase4-thousand-profiles");
        let core = CoreServices::open_fixed(&root).expect("core");
        for index in 0..1_000 {
            let id = format!("profile-performance-{index:04}");
            core.storage()
                .create_profile_with_metadata(&id, &format!("Profile {index:04}"), None)
                .expect("profile projection");
        }
        let started = Instant::now();
        let records = core.storage().profiles().expect("list profiles");
        let elapsed = started.elapsed();
        assert_eq!(records.len(), 1_000);
        assert!(
            elapsed < Duration::from_millis(500),
            "listing took {elapsed:?}"
        );
        let _ = fs::remove_dir_all(root);
    }
}
