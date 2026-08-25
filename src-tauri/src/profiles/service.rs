use crate::{
    app::paths::LauncherPaths,
    content::ContentKind,
    content_projection::immutable_projected_targets_for_duplicate,
    error::{AppError, AppResult},
    foundation::CoreServices,
    operations::{
        engine::OperationEngine,
        model::{
            canonical_json, new_identifier, sha256_hex, CacheMaterialization, OperationType,
            ProfileInstallPlan,
        },
    },
    profiles::model::{
        LockedCacheBlob, ProfileLockV1, ProfileLockV2, ProfileManifestV1, ProfileManifestV2,
        ProfileSummary,
    },
    security::{
        fs as secure_fs,
        paths::{collision_key, validate_existing_chain},
        PathRegistry, SecurePath,
    },
    storage::{models::ProfileRecord, models::RuntimeQueryProjection, Storage},
};
use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
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
const MAX_DUPLICATE_PROFILE_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
pub struct ProfileService {
    paths: LauncherPaths,
    registry: Arc<PathRegistry>,
    storage: Storage,
    operations: OperationEngine,
}

pub(crate) struct RestoreProfileCopyRequest {
    pub source_profile_id: String,
    pub source_revision_id: String,
    pub backup_id: String,
    pub display_name: String,
    pub include_files: bool,
    pub include_account: bool,
    pub expected_files: BTreeMap<String, (u64, String)>,
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
        self.create_profile_internal(display_name, None, account_id, Vec::new(), None, None)
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
        let runtime_revision = self.read_duplicate_runtime_revision(&source)?;
        let cache_blobs = runtime_revision
            .as_ref()
            .map(|revision| revision.lock.cache_blobs.clone())
            .map(Ok)
            .unwrap_or_else(|| self.read_active_cache_projection(&source))?;
        self.create_profile_internal(
            display_name,
            Some(source_profile_id),
            source.account_id.clone(),
            cache_blobs,
            runtime_revision,
            Some(InstanceCopySource::Profile(source_profile_id.to_string())),
        )
    }

    pub(crate) fn restore_profile_copy(
        &self,
        request: RestoreProfileCopyRequest,
    ) -> AppResult<ProfileSummary> {
        let source = self
            .storage
            .profile(&request.source_profile_id)?
            .ok_or_else(|| profile_not_found(&request.source_profile_id))?;
        let (runtime_revision, cache_blobs) = match self
            .profile_revision_format_version(&source.id, &request.source_revision_id)?
        {
            1 => (
                None,
                self.read_v1_revision_cache(&source, &request.source_revision_id)?,
            ),
            2 => {
                let revision = self.read_runtime_revision(&source, &request.source_revision_id)?;
                let cache_blobs = revision.lock.cache_blobs.clone();
                (Some(revision), cache_blobs)
            }
            _ => return Err(AppError::coded("profile_restore_revision_unsupported")),
        };
        self.create_profile_internal(
            &request.display_name,
            Some(&request.source_profile_id),
            request
                .include_account
                .then(|| source.account_id.clone())
                .flatten(),
            cache_blobs,
            runtime_revision,
            request.include_files.then_some(InstanceCopySource::Backup {
                backup_id: request.backup_id,
                expected_files: request.expected_files,
            }),
        )
    }

    pub fn committed_revisions(
        &self,
        profile_id: &str,
    ) -> AppResult<Vec<crate::storage::models::RevisionRecord>> {
        let profile = self
            .storage
            .profile(profile_id)?
            .ok_or_else(|| profile_not_found(profile_id))?;
        if profile.lifecycle_state == "trash" {
            return Err(AppError::coded("profile_revision_from_trash_forbidden"));
        }
        self.storage.profile_revisions(profile_id)
    }

    pub(crate) fn verified_revision_cache_blobs(
        &self,
        profile_id: &str,
        revision_id: &str,
    ) -> AppResult<Vec<LockedCacheBlob>> {
        let profile = self
            .storage
            .profile(profile_id)?
            .ok_or_else(|| profile_not_found(profile_id))?;
        match self.profile_revision_format_version(profile_id, revision_id)? {
            1 => self.read_v1_revision_cache(&profile, revision_id),
            2 => self
                .read_runtime_revision(&profile, revision_id)
                .map(|revision| revision.lock.cache_blobs),
            _ => Err(AppError::coded("profile_revision_unsupported")),
        }
    }

    pub fn rollback_to_revision(
        &self,
        profile_id: &str,
        source_revision_id: &str,
    ) -> AppResult<(String, String)> {
        let profile = self
            .storage
            .profile(profile_id)?
            .ok_or_else(|| profile_not_found(profile_id))?;
        if profile.lifecycle_state != "active" {
            return Err(AppError::coded("profile_rollback_requires_active_profile"));
        }
        if profile.active_revision_id.as_deref() == Some(source_revision_id) {
            return Err(AppError::coded("profile_rollback_target_is_active"));
        }
        let plan = match self.profile_revision_format_version(profile_id, source_revision_id)? {
            1 => {
                let cache_blobs = self.read_v1_revision_cache(&profile, source_revision_id)?;
                let mut plan = build_profile_plan(
                    profile_id,
                    &profile.display_name,
                    profile.source_profile_id.as_deref(),
                    profile.account_id.as_deref(),
                    cache_blobs,
                )?;
                plan.previous_revision_id = profile.active_revision_id.clone();
                plan.cleanup_profile_on_rollback = false;
                plan
            }
            2 => build_cloned_runtime_plan(
                profile_id,
                self.read_runtime_revision(&profile, source_revision_id)?,
                profile.active_revision_id.clone(),
                self.storage.runtime_projection(profile_id)?,
                false,
            )?,
            _ => return Err(AppError::coded("profile_rollback_revision_unsupported")),
        };
        self.operations
            .plan_profile_operation(&plan, OperationType::ProfileRollback)?;
        self.operations.execute(&plan.operation_id)?;
        Ok((plan.operation_id, plan.revision_id))
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

    pub fn rename_profile(&self, profile_id: &str, display_name: &str) -> AppResult<ProfileSummary> {
        let display_name = validate_display_name(display_name)?;
        let profile = self
            .storage
            .profile(profile_id)?
            .ok_or_else(|| profile_not_found(profile_id))?;
        if profile.lifecycle_state != "active" {
            return Err(AppError::coded("profile_rename_requires_active_profile"));
        }
        self.storage.rename_profile(profile_id, &display_name)?;
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
        runtime_revision: Option<DuplicateRuntimeRevision>,
        instance_source: Option<InstanceCopySource>,
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
            if let Some(instance_source) = instance_source.as_ref() {
                let empty_excluded_targets = BTreeSet::new();
                let excluded_targets = runtime_revision
                    .as_ref()
                    .map(|revision| &revision.excluded_instance_targets)
                    .unwrap_or(&empty_excluded_targets);
                match instance_source {
                    InstanceCopySource::Profile(source_profile_id) => self.copy_mutable_instance(
                        source_profile_id,
                        &profile_id,
                        excluded_targets,
                    )?,
                    InstanceCopySource::Backup {
                        backup_id,
                        expected_files,
                    } => self.copy_instance_from_registered(
                        "backups",
                        &PathBuf::from(backup_id).join("instance"),
                        &profile_id,
                        excluded_targets,
                        Some(expected_files),
                    )?,
                }
                self.create_mutable_layout(&profile_id)?;
            }
            Ok(())
        })();
        if let Err(error) = preparation {
            return Err(self.cleanup_unactivated_profile(&profile_id, error));
        }

        let plan_result = match runtime_revision {
            Some(revision) => build_cloned_runtime_plan(&profile_id, revision, None, None, true),
            None => build_profile_plan(
                &profile_id,
                &display_name,
                source_profile_id,
                account_id.as_deref(),
                cache_blobs,
            ),
        };
        let plan = match plan_result {
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
        excluded_targets: &BTreeSet<String>,
    ) -> AppResult<()> {
        self.copy_instance_from_registered(
            "profiles",
            &PathBuf::from(source_profile_id).join("instance"),
            target_profile_id,
            excluded_targets,
            None,
        )
    }

    fn copy_instance_from_registered(
        &self,
        source_root_id: &str,
        source_prefix: &Path,
        target_profile_id: &str,
        excluded_targets: &BTreeSet<String>,
        expected_files: Option<&BTreeMap<String, (u64, String)>>,
    ) -> AppResult<()> {
        let source = self.registry.resolve(source_root_id, source_prefix)?;
        let target = self
            .registry
            .resolve("profiles", format!("{target_profile_id}/instance"))?;
        let context = DirectoryCopyContext {
            registry: &self.registry,
            anchor: source.anchor(),
            source_root: source.absolute(),
            target_root: target.absolute(),
            source_root_id,
            source_prefix,
            target_profile_id,
            excluded_targets,
            expected_files,
        };
        let mut copied_files = BTreeSet::new();
        copy_directory_tree(&context, Path::new(""), &mut copied_files)?;
        if expected_files.is_some_and(|expected| expected.len() != copied_files.len()) {
            return Err(AppError::coded("backup_content_mismatch"));
        }
        Ok(())
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

    fn profile_revision_format_version(
        &self,
        profile_id: &str,
        revision_id: &str,
    ) -> AppResult<u64> {
        let manifest_path = self.registry.resolve(
            "profiles",
            format!("{profile_id}/revisions/{revision_id}/manifest.json"),
        )?;
        let document: serde_json::Value =
            serde_json::from_slice(&read_profile_document(&manifest_path)?)
                .map_err(|_| AppError::coded("profile_restore_manifest_invalid"))?;
        document
            .get("formatVersion")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| AppError::coded("profile_restore_manifest_invalid"))
    }

    fn read_v1_revision_cache(
        &self,
        source: &ProfileRecord,
        revision_id: &str,
    ) -> AppResult<Vec<LockedCacheBlob>> {
        let revision = self
            .storage
            .revision(revision_id)?
            .filter(|revision| revision.profile_id == source.id && revision.status == "committed")
            .ok_or_else(|| AppError::coded("profile_restore_revision_invalid"))?;
        let manifest_path = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{revision_id}/manifest.json", source.id),
        )?;
        let lock_path = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{revision_id}/lock.json", source.id),
        )?;
        let manifest_bytes = read_profile_document(&manifest_path)?;
        let lock_bytes = read_profile_document(&lock_path)?;
        let manifest: ProfileManifestV1 = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| AppError::coded("profile_restore_manifest_invalid"))?;
        let lock: ProfileLockV1 = serde_json::from_slice(&lock_bytes)
            .map_err(|_| AppError::coded("profile_restore_lock_invalid"))?;
        let manifest_sha256 = sha256_hex(canonical_json(&manifest)?.as_bytes());
        if manifest.format != "site.s9lab.profile"
            || manifest.format_version != 1
            || manifest.profile_id != source.id
            || lock.format != "site.s9lab.profile-lock"
            || lock.format_version != 1
            || lock.profile_id != source.id
            || lock.revision_id != revision_id
            || lock.manifest_sha256 != manifest_sha256
            || revision.manifest_sha256 != manifest_sha256
            || revision.lock_sha256 != sha256_hex(&lock_bytes)
        {
            return Err(AppError::coded("profile_restore_revision_invalid"));
        }
        Ok(lock.cache_blobs)
    }

    fn read_duplicate_runtime_revision(
        &self,
        source: &ProfileRecord,
    ) -> AppResult<Option<DuplicateRuntimeRevision>> {
        let Some(_) = self.storage.runtime_projection(&source.id)? else {
            return Ok(None);
        };
        let revision_id = source
            .active_revision_id
            .as_deref()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        self.read_runtime_revision(source, revision_id).map(Some)
    }

    fn read_runtime_revision(
        &self,
        source: &ProfileRecord,
        revision_id: &str,
    ) -> AppResult<DuplicateRuntimeRevision> {
        let revision = self
            .storage
            .revision(revision_id)?
            .filter(|revision| revision.profile_id == source.id && revision.status == "committed")
            .ok_or_else(|| AppError::coded("profile_rollback_revision_invalid"))?;
        let manifest_path = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{revision_id}/manifest.json", source.id),
        )?;
        let lock_path = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{revision_id}/lock.json", source.id),
        )?;
        let manifest_bytes = read_profile_document(&manifest_path)?;
        let lock_bytes = read_profile_document(&lock_path)?;
        let manifest: ProfileManifestV2 = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| AppError::coded("profile_duplicate_manifest_invalid"))?;
        let lock: ProfileLockV2 = serde_json::from_slice(&lock_bytes)
            .map_err(|_| AppError::coded("profile_duplicate_lock_invalid"))?;
        let manifest_sha256 = sha256_hex(canonical_json(&manifest)?.as_bytes());
        if manifest.format != "site.s9lab.profile"
            || manifest.format_version != 2
            || manifest.profile_id != source.id
            || lock.format != "site.s9lab.profile-lock"
            || lock.format_version != 2
            || lock.profile_id != source.id
            || lock.revision_id != revision_id
            || lock.manifest_sha256 != manifest_sha256
            || revision.manifest_sha256 != manifest_sha256
            || revision.lock_sha256 != sha256_hex(&lock_bytes)
        {
            return Err(AppError::coded("profile_duplicate_revision_invalid"));
        }

        let (component_id, component_version) = match &manifest.s9lab_component {
            crate::profiles::model::S9labComponentSelection::Disabled => (None, None),
            crate::profiles::model::S9labComponentSelection::Catalog {
                component_id,
                component_version,
            } => (Some(component_id.clone()), Some(component_version.clone())),
        };
        let runtime_projection = RuntimeQueryProjection {
            profile_id: source.id.clone(),
            revision_id: revision_id.to_string(),
            minecraft_version: manifest.runtime.minecraft_version.clone(),
            loader_kind: manifest.runtime.loader.kind.as_str().into(),
            loader_version: manifest.runtime.loader.loader_version.clone(),
            component_id,
            component_version,
            install_state: "installed".into(),
            updated_at_unix: Utc::now().timestamp(),
        };

        let mut excluded_instance_targets =
            immutable_projected_targets_for_duplicate(&self.registry, &source.id)?
                .into_iter()
                .map(|target| collision_key(Path::new(&target)))
                .collect::<AppResult<BTreeSet<_>>>()?;
        excluded_instance_targets.extend(
            lock.content
                .iter()
                .flat_map(|content| content.items.iter())
                .filter(|item| item.enabled && item.kind != ContentKind::Modpack)
                .map(|item| collision_key(Path::new(&item.relative_target)))
                .collect::<AppResult<BTreeSet<_>>>()?,
        );
        Ok(DuplicateRuntimeRevision {
            manifest,
            lock,
            runtime_projection,
            excluded_instance_targets,
        })
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

fn build_cloned_runtime_plan(
    profile_id: &str,
    revision: DuplicateRuntimeRevision,
    previous_revision_id: Option<String>,
    previous_runtime_projection: Option<RuntimeQueryProjection>,
    cleanup_profile_on_rollback: bool,
) -> AppResult<ProfileInstallPlan> {
    let operation_id = new_identifier("op");
    let revision_id = new_identifier("rev");
    let mut manifest = revision.manifest;
    manifest.profile_id = profile_id.to_string();
    manifest.created_at_unix = Utc::now().timestamp();
    let manifest_json = canonical_json(&manifest)?;
    let manifest_sha256 = sha256_hex(manifest_json.as_bytes());

    let mut lock = revision.lock;
    lock.profile_id = profile_id.to_string();
    lock.revision_id = revision_id.clone();
    lock.manifest_sha256 = manifest_sha256.clone();
    lock.cache_blobs.sort();
    lock.cache_blobs.dedup();
    let lock_json = canonical_json(&lock)?;
    let lock_sha256 = sha256_hex(lock_json.as_bytes());

    let mut cache_materializations = lock
        .runtime
        .items
        .iter()
        .map(|item| CacheMaterialization {
            blob_sha256: item.sha256.clone(),
            size_bytes: item.size_bytes,
            relative_path: format!("runtime/{}", item.relative_target),
        })
        .chain(
            lock.content
                .iter()
                .flat_map(|content| content.items.iter())
                .map(|item| CacheMaterialization {
                    blob_sha256: item.sha256.clone(),
                    size_bytes: item.size_bytes,
                    relative_path: format!("content/{}", item.relative_target),
                }),
        )
        .collect::<Vec<_>>();
    cache_materializations.extend(
        lock.content
            .iter()
            .flat_map(|content| content.overrides.iter())
            .map(|override_file| CacheMaterialization {
                blob_sha256: override_file.sha256.clone(),
                size_bytes: override_file.size_bytes,
                relative_path: format!("content/{}", override_file.relative_target),
            }),
    );

    let mut runtime_projection = revision.runtime_projection;
    runtime_projection.profile_id = profile_id.to_string();
    runtime_projection.revision_id = revision_id.clone();
    runtime_projection.updated_at_unix = Utc::now().timestamp();
    Ok(ProfileInstallPlan {
        operation_id,
        profile_id: profile_id.to_string(),
        revision_id,
        previous_revision_id,
        manifest_json,
        manifest_sha256,
        lock_json,
        lock_sha256,
        payload_files: Vec::new(),
        cache_materializations,
        runtime_projection: Some(runtime_projection),
        previous_runtime_projection,
        cleanup_profile_on_rollback,
    })
}

struct DirectoryCopyContext<'a> {
    registry: &'a PathRegistry,
    anchor: &'a Path,
    source_root: &'a Path,
    target_root: &'a Path,
    source_root_id: &'a str,
    source_prefix: &'a Path,
    target_profile_id: &'a str,
    excluded_targets: &'a BTreeSet<String>,
    expected_files: Option<&'a BTreeMap<String, (u64, String)>>,
}

fn copy_directory_tree(
    context: &DirectoryCopyContext<'_>,
    relative: &Path,
    copied_files: &mut BTreeSet<String>,
) -> AppResult<()> {
    let source_directory = context.source_root.join(relative);
    validate_existing_chain(context.anchor, &source_directory)?;
    let metadata = fs::symlink_metadata(&source_directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::coded("profile_copy_source_tree_invalid"));
    }
    let target_directory = context.target_root.join(relative);
    let mut target_relative = PathBuf::from(context.target_profile_id).join("instance");
    if !relative.as_os_str().is_empty() {
        target_relative.push(relative);
    }
    let target = context.registry.resolve("profiles", target_relative)?;
    secure_fs::create_directories_within(target.anchor(), target.root(), &target_directory)?;
    for entry in fs::read_dir(source_directory)? {
        let entry = entry?;
        let child_relative = relative.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::coded("profile_copy_symlink_forbidden"));
        }
        if child_relative.starts_with(Path::new(".s9lab")) {
            continue;
        }
        let child_key = collision_key(&child_relative)?;
        if context.excluded_targets.contains(&child_key) {
            if metadata.is_file() {
                continue;
            }
            return Err(AppError::coded("profile_copy_managed_target_invalid"));
        }
        if metadata.is_dir() {
            copy_directory_tree(context, &child_relative, copied_files)?;
        } else if metadata.is_file() {
            let source = context.registry.resolve(
                context.source_root_id,
                context.source_prefix.join(&child_relative),
            )?;
            let destination = context.registry.resolve(
                "profiles",
                PathBuf::from(context.target_profile_id)
                    .join("instance")
                    .join(&child_relative),
            )?;
            let copied = secure_fs::copy_new(&source, &destination)?;
            if let Some(expected_files) = context.expected_files {
                let (expected_size, expected_sha256) = expected_files
                    .get(&child_key)
                    .ok_or_else(|| AppError::coded("backup_content_mismatch"))?;
                let (actual_size, actual_sha256) = hash_copied_file(destination.absolute())?;
                if copied != *expected_size
                    || actual_size != *expected_size
                    || actual_sha256 != *expected_sha256
                    || !copied_files.insert(child_key)
                {
                    return Err(AppError::coded("backup_content_mismatch"));
                }
            }
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

struct DuplicateRuntimeRevision {
    manifest: ProfileManifestV2,
    lock: ProfileLockV2,
    runtime_projection: RuntimeQueryProjection,
    excluded_instance_targets: BTreeSet<String>,
}

enum InstanceCopySource {
    Profile(String),
    Backup {
        backup_id: String,
        expected_files: BTreeMap<String, (u64, String)>,
    },
}

fn hash_copied_file(path: &Path) -> AppResult<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| AppError::coded("backup_size_overflow"))?;
        hasher.update(&buffer[..read]);
    }
    Ok((size, hex::encode(hasher.finalize())))
}

fn read_profile_document(path: &SecurePath) -> AppResult<Vec<u8>> {
    validate_existing_chain(path.anchor(), path.absolute())?;
    let metadata = fs::symlink_metadata(path.absolute())?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_DUPLICATE_PROFILE_DOCUMENT_BYTES
    {
        return Err(AppError::coded("profile_duplicate_document_invalid"));
    }
    let bytes = fs::read(path.absolute())?;
    if bytes.len() as u64 != metadata.len() {
        return Err(AppError::coded("profile_duplicate_document_changed"));
    }
    Ok(bytes)
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
    fn duplicate_copy_excludes_projection_state_and_immutable_managed_files() {
        let root = crate::foundation::test_root("phase6-profile-duplicate-projection");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let source = profiles.create_profile("Source").expect("source");
        let target = profiles.create_profile("Target").expect("target");
        let source_instance = root.join("profiles").join(&source.id).join("instance");
        fs::create_dir_all(source_instance.join(".s9lab")).expect("internal state");
        fs::write(
            source_instance.join(".s9lab/content-projection.json"),
            b"source-only marker",
        )
        .expect("marker");
        fs::write(source_instance.join("mods/managed.jar"), b"managed").expect("managed file");
        fs::write(source_instance.join("config/user.toml"), b"user override")
            .expect("mutable override");
        let excluded =
            BTreeSet::from([collision_key(Path::new("mods/managed.jar")).expect("collision key")]);

        profiles
            .copy_mutable_instance(&source.id, &target.id, &excluded)
            .expect("copy mutable state");

        let target_instance = root.join("profiles").join(&target.id).join("instance");
        assert!(!target_instance.join(".s9lab").exists());
        assert!(!target_instance.join("mods/managed.jar").exists());
        assert_eq!(
            fs::read(target_instance.join("config/user.toml")).expect("copied override"),
            b"user override"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicate_of_runtime_profile_clones_v2_revision_with_new_identity() {
        use crate::{
            profiles::model::{ResolvedLaunchConfiguration, S9labComponentSelection},
            runtime::{
                JavaPolicy, LoaderKind, LoaderSelection, ProfileRuntimeIntent,
                ResolvedRuntimeLockV1, RUNTIME_LOCK_FORMAT, RUNTIME_LOCK_FORMAT_VERSION,
            },
        };

        let root = crate::foundation::test_root("phase6-profile-duplicate-v2");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let source_id = new_identifier("profile");
        let source_root = core
            .registry()
            .resolve("profiles", &source_id)
            .expect("source root");
        secure_fs::create_directories_within(
            source_root.anchor(),
            source_root.root(),
            source_root.absolute(),
        )
        .expect("source directory");
        core.storage()
            .create_profile_with_metadata(&source_id, "Runtime source", None)
            .expect("source projection");
        profiles
            .create_mutable_layout(&source_id)
            .expect("source mutable layout");
        let runtime = ProfileRuntimeIntent {
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Vanilla,
                loader_version: None,
            },
            java: JavaPolicy::Managed { major_version: 21 },
        };
        let revision = DuplicateRuntimeRevision {
            manifest: ProfileManifestV2 {
                format: "site.s9lab.profile".into(),
                format_version: 2,
                profile_id: "template".into(),
                created_at_unix: 0,
                runtime: runtime.clone(),
                s9lab_component: S9labComponentSelection::Disabled,
                desired_content: Vec::new(),
                mutable_directories: MUTABLE_INSTANCE_DIRECTORIES
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect(),
                isolation_policy: "verified-copy-no-hardlinks".into(),
            },
            lock: ProfileLockV2 {
                format: "site.s9lab.profile-lock".into(),
                format_version: 2,
                profile_id: "template".into(),
                revision_id: "template-revision".into(),
                manifest_sha256: "0".repeat(64),
                runtime: ResolvedRuntimeLockV1 {
                    format: RUNTIME_LOCK_FORMAT.into(),
                    format_version: RUNTIME_LOCK_FORMAT_VERSION,
                    runtime,
                    items: Vec::new(),
                },
                launch: ResolvedLaunchConfiguration {
                    main_class: "net.minecraft.client.main.Main".into(),
                    version_type: "release".into(),
                    asset_index_id: "1.21.1".into(),
                    java_major_version: 21,
                    game_arguments: Vec::new(),
                    jvm_arguments: Vec::new(),
                    classpath_targets: Vec::new(),
                    native_jar_targets: Vec::new(),
                    legacy_game_arguments: None,
                },
                content: None,
                cache_blobs: Vec::new(),
            },
            runtime_projection: RuntimeQueryProjection {
                profile_id: "template".into(),
                revision_id: "template-revision".into(),
                minecraft_version: "1.21.1".into(),
                loader_kind: "vanilla".into(),
                loader_version: None,
                component_id: None,
                component_version: None,
                install_state: "installed".into(),
                updated_at_unix: 0,
            },
            excluded_instance_targets: BTreeSet::new(),
        };
        let source_plan =
            build_cloned_runtime_plan(&source_id, revision, None, None, true).expect("source plan");
        core.operations()
            .plan_profile_revision(&source_plan)
            .expect("plan source");
        core.operations()
            .execute(&source_plan.operation_id)
            .expect("activate source");

        let duplicate = profiles
            .duplicate_profile(&source_id, "Runtime duplicate")
            .expect("duplicate runtime profile");
        let projection = core
            .storage()
            .runtime_projection(&duplicate.id)
            .expect("projection query")
            .expect("runtime projection");
        assert_eq!(projection.profile_id, duplicate.id);
        assert_eq!(projection.revision_id, duplicate.active_revision_id);
        let lock_path = core
            .registry()
            .resolve(
                "profiles",
                format!(
                    "{}/revisions/{}/lock.json",
                    duplicate.id, duplicate.active_revision_id
                ),
            )
            .expect("duplicate lock");
        let lock: ProfileLockV2 = serde_json::from_slice(
            &read_profile_document(&lock_path).expect("duplicate lock document"),
        )
        .expect("duplicate v2 lock");
        assert_eq!(lock.profile_id, duplicate.id);
        assert_eq!(lock.revision_id, duplicate.active_revision_id);
        assert_eq!(lock.format_version, 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restore_can_recover_a_historical_v1_revision() {
        let root = crate::foundation::test_root("phase7-profile-restore-historical-v1");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let source = profiles.create_profile("V1 source").expect("source");
        let historical_revision = source.active_revision_id.clone();
        let backup_id = new_identifier("backup");
        let backup_file = root
            .join("backups")
            .join(&backup_id)
            .join("instance/config/historical.txt");
        fs::create_dir_all(backup_file.parent().expect("backup parent")).expect("backup directory");
        fs::write(&backup_file, b"historical-v1").expect("backup file");

        let mut next_plan =
            build_profile_plan(&source.id, "V1 source", None, None, Vec::new()).expect("next plan");
        next_plan.previous_revision_id = Some(historical_revision.clone());
        next_plan.cleanup_profile_on_rollback = false;
        core.operations()
            .plan_profile_revision(&next_plan)
            .expect("plan next revision");
        core.operations()
            .execute(&next_plan.operation_id)
            .expect("activate next revision");
        let (_, rolled_back_revision) = profiles
            .rollback_to_revision(&source.id, &historical_revision)
            .expect("rollback historical V1");
        assert_ne!(rolled_back_revision, historical_revision);
        let rolled_back_manifest = root
            .join("profiles")
            .join(&source.id)
            .join("revisions")
            .join(&rolled_back_revision)
            .join("manifest.json");
        let manifest: ProfileManifestV1 =
            serde_json::from_slice(&fs::read(rolled_back_manifest).expect("rolled back manifest"))
                .expect("rolled back V1 manifest");
        assert_eq!(manifest.format_version, 1);

        let expected_files = BTreeMap::from([(
            collision_key(Path::new("config/historical.txt")).expect("collision key"),
            hash_copied_file(&backup_file).expect("backup hash"),
        )]);
        let restored = profiles
            .restore_profile_copy(RestoreProfileCopyRequest {
                source_profile_id: source.id,
                source_revision_id: historical_revision,
                backup_id,
                display_name: "Recovered V1".into(),
                include_files: true,
                include_account: false,
                expected_files,
            })
            .expect("restore historical V1");
        assert_eq!(
            fs::read(
                root.join("profiles")
                    .join(restored.id)
                    .join("instance/config/historical.txt")
            )
            .expect("restored file"),
            b"historical-v1"
        );
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
