use crate::{
    app::paths::LauncherPaths,
    cache::CacheStore,
    error::{AppError, AppResult},
    operations::model::{
        FailureInjector, FailurePoint, NoFailure, OperationState, OperationType, ProfileInstallPlan,
    },
    security::{fs as secure_fs, PathRegistry},
    storage::{
        models::{OperationRecord, RevisionRecord},
        Storage,
    },
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;
use std::{collections::BTreeSet, fs, io::Read, path::Path, sync::Arc};

pub(crate) const REVISION_CACHE_OWNER_TYPE: &str = "profile-revision";

#[derive(Clone)]
pub struct OperationEngine {
    paths: LauncherPaths,
    registry: Arc<PathRegistry>,
    storage: Storage,
}

impl OperationEngine {
    pub fn new(paths: LauncherPaths, registry: Arc<PathRegistry>, storage: Storage) -> Self {
        Self {
            paths,
            registry,
            storage,
        }
    }

    pub fn create_minimal_profile(&self, display_name: &str) -> AppResult<String> {
        let id = crate::operations::model::new_identifier("profile");
        let profile_dir = self.registry.resolve("profiles", &id)?;
        secure_fs::create_directories_within(
            profile_dir.anchor(),
            profile_dir.root(),
            profile_dir.absolute(),
        )?;
        if let Err(error) = self
            .storage
            .create_profile_with_metadata(&id, display_name, None)
        {
            let _ = secure_fs::remove_tree(&profile_dir);
            return Err(error);
        }
        Ok(id)
    }

    pub fn plan_simulated_install(
        &self,
        profile_id: &str,
        display_name: &str,
    ) -> AppResult<ProfileInstallPlan> {
        let profile = self.storage.profile(profile_id)?.ok_or_else(|| {
            AppError::coded_with("profile_not_found", [("profileId", profile_id)])
        })?;
        let plan =
            ProfileInstallPlan::new(profile_id, display_name, profile.active_revision_id.clone())?;
        self.register_plan(&plan, OperationType::SimulatedProfileInstall)?;
        Ok(plan)
    }

    pub fn plan_profile_revision(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        self.plan_profile_operation(plan, OperationType::ProfileRevision)
    }

    pub fn plan_profile_operation(
        &self,
        plan: &ProfileInstallPlan,
        operation_type: OperationType,
    ) -> AppResult<()> {
        if operation_type == OperationType::SimulatedProfileInstall {
            return Err(AppError::coded("operation_type_reserved"));
        }
        let profile = self.storage.profile(&plan.profile_id)?.ok_or_else(|| {
            AppError::coded_with(
                "profile_not_found",
                [("profileId", plan.profile_id.clone())],
            )
        })?;
        if profile.active_revision_id != plan.previous_revision_id {
            return Err(AppError::coded("profile_revision_conflict"));
        }
        if self.storage.runtime_projection(&plan.profile_id)? != plan.previous_runtime_projection {
            return Err(AppError::coded("runtime_projection_conflict"));
        }
        if plan.cleanup_profile_on_rollback && plan.previous_revision_id.is_some() {
            return Err(AppError::coded("profile_cleanup_flag_invalid"));
        }
        self.register_plan(plan, operation_type)
    }

    fn register_plan(
        &self,
        plan: &ProfileInstallPlan,
        operation_type: OperationType,
    ) -> AppResult<()> {
        self.validate_plan_documents(plan)?;
        self.validate_plan_paths(plan)?;
        self.validate_cache_materializations(plan)?;
        let staging_relative_path = format!("{}/revision", plan.operation_id);
        let operation = OperationRecord {
            id: plan.operation_id.clone(),
            operation_type,
            profile_id: Some(plan.profile_id.clone()),
            state: OperationState::Planned,
            planned_changes_json: serde_json::to_string(&plan)?,
            staging_relative_path,
            previous_revision_id: plan.previous_revision_id.clone(),
            target_revision_id: Some(plan.revision_id.clone()),
            started_at_unix: Utc::now().timestamp(),
            completed_at_unix: None,
            error_code: None,
            error_params_json: None,
        };
        self.storage.insert_operation(&operation)?;
        self.storage.append_journal(
            &plan.operation_id,
            "operation-planned",
            "completed",
            &json!({"profileId": &plan.profile_id, "revisionId": &plan.revision_id}).to_string(),
            &json!({"action": "mark-rolled-back"}).to_string(),
        )?;
        Ok(())
    }

    pub fn execute(&self, operation_id: &str) -> AppResult<()> {
        self.execute_controlled(operation_id, &NoFailure)
    }

    fn execute_controlled(
        &self,
        operation_id: &str,
        injector: &dyn FailureInjector,
    ) -> AppResult<()> {
        match self.run(operation_id, injector) {
            Ok(()) => Ok(()),
            Err(error) => {
                let should_rollback = self
                    .storage
                    .operation(operation_id)?
                    .is_some_and(|operation| !operation.state.is_terminal());
                if should_rollback {
                    if let Err(rollback_error) = self.rollback(operation_id, &error) {
                        return Err(AppError::coded_with(
                            "operation_rollback_failed",
                            [
                                ("operationId", operation_id.to_string()),
                                ("originalError", error.descriptor().code),
                                ("rollbackError", rollback_error.descriptor().code),
                            ],
                        ));
                    }
                }
                Err(error)
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn execute_controlled_with_injector(
        &self,
        operation_id: &str,
        injector: &dyn FailureInjector,
    ) -> AppResult<()> {
        self.execute_controlled(operation_id, injector)
    }

    #[cfg(test)]
    pub(crate) fn execute_with_injector(
        &self,
        operation_id: &str,
        injector: &dyn FailureInjector,
    ) -> AppResult<()> {
        self.run(operation_id, injector)
    }

    fn run(&self, operation_id: &str, injector: &dyn FailureInjector) -> AppResult<()> {
        let operation = self.storage.operation(operation_id)?.ok_or_else(|| {
            AppError::coded_with("operation_not_found", [("operationId", operation_id)])
        })?;
        if operation.state != OperationState::Planned {
            return Err(AppError::coded_with(
                "operation_invalid_start_state",
                [("state", operation.state.as_str())],
            ));
        }
        let plan: ProfileInstallPlan = serde_json::from_str(&operation.planned_changes_json)?;
        self.validate_plan_record(&operation, &plan)?;
        self.validate_plan_documents(&plan)?;
        self.validate_plan_paths(&plan)?;
        self.validate_cache_materializations(&plan)?;
        injector.checkpoint(FailurePoint::AfterPlanned)?;

        self.transition(operation_id, OperationState::Staging)?;
        self.stage(&plan)?;
        injector.checkpoint(FailurePoint::AfterStaging)?;

        self.transition(operation_id, OperationState::Verifying)?;
        self.verify_staged(&plan)?;
        injector.checkpoint(FailurePoint::AfterVerifying)?;

        self.transition(operation_id, OperationState::ReadyToCommit)?;
        self.storage.append_journal(
            operation_id,
            "staging-verified",
            "completed",
            &json!({
                "manifestSha256": &plan.manifest_sha256,
                "lockSha256": &plan.lock_sha256
            })
            .to_string(),
            &json!({"action": "remove-staging"}).to_string(),
        )?;
        injector.checkpoint(FailurePoint::AfterReadyToCommit)?;

        self.transition(operation_id, OperationState::Committing)?;
        let destination = self.commit_revision_directory(&plan)?;
        self.storage.append_journal(
            operation_id,
            "revision-moved",
            "completed",
            &json!({"destination": destination.relative().display().to_string()}).to_string(),
            &json!({
                "action": "remove-revision",
                "relativePath": destination.relative().display().to_string()
            })
            .to_string(),
        )?;
        injector.checkpoint(FailurePoint::AfterRevisionMoved)?;

        self.storage.activate_revision_with_runtime_projection(
            &RevisionRecord {
                id: plan.revision_id.clone(),
                profile_id: plan.profile_id.clone(),
                operation_id: plan.operation_id.clone(),
                manifest_sha256: plan.manifest_sha256.clone(),
                lock_sha256: plan.lock_sha256.clone(),
                manifest_relative_path: format!(
                    "{}/revisions/{}/manifest.json",
                    plan.profile_id, plan.revision_id
                ),
                lock_relative_path: format!(
                    "{}/revisions/{}/lock.json",
                    plan.profile_id, plan.revision_id
                ),
                status: "committed".into(),
                created_at_unix: Utc::now().timestamp(),
            },
            plan.previous_revision_id.as_deref(),
            plan.runtime_projection.as_ref(),
        )?;
        self.storage.append_journal(
            operation_id,
            "database-activated",
            "completed",
            &json!({"revisionId": &plan.revision_id}).to_string(),
            &json!({
                "action": "restore-active-revision",
                "previousRevisionId": &plan.previous_revision_id
            })
            .to_string(),
        )?;
        injector.checkpoint(FailurePoint::AfterDatabaseActivated)?;

        self.transition(operation_id, OperationState::Validating)?;
        injector.checkpoint(FailurePoint::DuringValidation)?;
        self.validate_active(&plan)?;
        self.replace_revision_cache_references(&plan)?;
        self.storage.append_journal(
            operation_id,
            "cache-references-activated",
            "completed",
            &json!({
                "ownerType": REVISION_CACHE_OWNER_TYPE,
                "ownerId": &plan.revision_id,
                "blobs": plan.cache_materializations.len()
            })
            .to_string(),
            &json!({
                "action": "remove-cache-references",
                "ownerType": REVISION_CACHE_OWNER_TYPE,
                "ownerId": &plan.revision_id
            })
            .to_string(),
        )?;
        injector.checkpoint(FailurePoint::AfterCacheReferences)?;
        self.cleanup_staging(&plan.operation_id)?;
        self.transition(operation_id, OperationState::Completed)?;
        Ok(())
    }

    pub(crate) fn validate_plan_record(
        &self,
        operation: &OperationRecord,
        plan: &ProfileInstallPlan,
    ) -> AppResult<()> {
        let expected_staging_path = format!("{}/revision", operation.id);
        let matches_record = plan.operation_id == operation.id
            && operation.profile_id.as_deref() == Some(plan.profile_id.as_str())
            && operation.target_revision_id.as_deref() == Some(plan.revision_id.as_str())
            && operation.previous_revision_id == plan.previous_revision_id
            && operation.staging_relative_path == expected_staging_path;
        if !matches_record {
            return Err(AppError::coded_with(
                "operation_plan_record_mismatch",
                [("operationId", operation.id.clone())],
            ));
        }
        Ok(())
    }

    pub fn validate_active(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        let profile = self
            .storage
            .profile(&plan.profile_id)?
            .ok_or_else(|| AppError::coded("profile_not_found"))?;
        if profile.active_revision_id.as_deref() != Some(plan.revision_id.as_str()) {
            return Err(AppError::coded("profile_active_revision_mismatch"));
        }
        let revision = self
            .storage
            .revision(&plan.revision_id)?
            .ok_or_else(|| AppError::coded("profile_revision_missing"))?;
        if revision.manifest_sha256 != plan.manifest_sha256
            || revision.lock_sha256 != plan.lock_sha256
            || revision.status != "committed"
        {
            return Err(AppError::coded("profile_revision_metadata_mismatch"));
        }
        let manifest = self.registry.resolve(
            "profiles",
            format!(
                "{}/revisions/{}/manifest.json",
                plan.profile_id, plan.revision_id
            ),
        )?;
        let lock = self.registry.resolve(
            "profiles",
            format!(
                "{}/revisions/{}/lock.json",
                plan.profile_id, plan.revision_id
            ),
        )?;
        if crate::operations::model::sha256_hex(&fs::read(manifest.absolute())?)
            != plan.manifest_sha256
            || crate::operations::model::sha256_hex(&fs::read(lock.absolute())?) != plan.lock_sha256
        {
            return Err(AppError::coded("profile_revision_file_hash_mismatch"));
        }
        let manifest_bytes = fs::read(manifest.absolute())?;
        let lock_bytes = fs::read(lock.absolute())?;
        let parsed_manifest: ManifestIdentity = serde_json::from_slice(&manifest_bytes)?;
        let parsed_lock: LockIdentity = serde_json::from_slice(&lock_bytes)?;
        if parsed_manifest.profile_id != plan.profile_id
            || parsed_lock.profile_id != plan.profile_id
            || parsed_lock.revision_id != plan.revision_id
            || parsed_lock.manifest_sha256 != plan.manifest_sha256
        {
            return Err(AppError::coded("profile_lock_manifest_metadata_mismatch"));
        }
        for file in &plan.payload_files {
            let payload = self.registry.resolve(
                "profiles",
                format!(
                    "{}/revisions/{}/{}",
                    plan.profile_id, plan.revision_id, file.relative_path
                ),
            )?;
            if crate::operations::model::sha256_hex(&fs::read(payload.absolute())?) != file.sha256 {
                return Err(AppError::coded_with(
                    "profile_revision_payload_hash_mismatch",
                    [("path", file.relative_path.clone())],
                ));
            }
        }
        for materialization in &plan.cache_materializations {
            let materialized = self.registry.resolve(
                "profiles",
                format!(
                    "{}/revisions/{}/{}",
                    plan.profile_id, plan.revision_id, materialization.relative_path
                ),
            )?;
            verify_file(
                materialized.absolute(),
                materialization.size_bytes,
                &materialization.blob_sha256,
                "profile_revision_cache_materialization_mismatch",
            )?;
        }
        Ok(())
    }

    pub(crate) fn rollback(&self, operation_id: &str, reason: &AppError) -> AppResult<()> {
        let operation = self
            .storage
            .operation(operation_id)?
            .ok_or_else(|| AppError::coded("operation_not_found"))?;
        let plan: ProfileInstallPlan = serde_json::from_str(&operation.planned_changes_json)?;
        let descriptor = reason.descriptor();
        let error_params_json = serde_json::to_string(&descriptor.params)?;
        self.storage.update_operation_state(
            operation_id,
            OperationState::RollingBack,
            Some((&descriptor.code, &error_params_json)),
        )?;

        let profile = self.storage.profile(&plan.profile_id)?;
        let new_is_active = profile
            .as_ref()
            .and_then(|item| item.active_revision_id.as_deref())
            == Some(plan.revision_id.as_str());
        if new_is_active {
            self.storage
                .restore_active_revision_with_runtime_projection(
                    &plan.profile_id,
                    &plan.revision_id,
                    plan.previous_revision_id.as_deref(),
                    plan.previous_runtime_projection.as_ref(),
                )?;
            self.storage.append_journal(
                operation_id,
                "database-activation-rolled-back",
                "compensated",
                &json!({"revisionId": &plan.revision_id}).to_string(),
                "{}",
            )?;
        }

        let removed_references = self
            .storage
            .remove_cache_references(REVISION_CACHE_OWNER_TYPE, &plan.revision_id)?;
        if removed_references > 0 {
            self.storage.append_journal(
                operation_id,
                "cache-references-rolled-back",
                "compensated",
                &json!({
                    "ownerType": REVISION_CACHE_OWNER_TYPE,
                    "ownerId": &plan.revision_id,
                    "removed": removed_references
                })
                .to_string(),
                "{}",
            )?;
        }

        let destination = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{}", plan.profile_id, plan.revision_id),
        )?;
        if destination.absolute().exists() {
            secure_fs::remove_tree(&destination)?;
            self.storage.append_journal(
                operation_id,
                "revision-move-rolled-back",
                "compensated",
                &json!({"relativePath": destination.relative().display().to_string()}).to_string(),
                "{}",
            )?;
        }
        self.cleanup_staging(operation_id)?;
        self.storage.update_operation_state(
            operation_id,
            OperationState::RolledBack,
            Some((&descriptor.code, &error_params_json)),
        )?;
        if plan.cleanup_profile_on_rollback {
            self.storage
                .detach_and_delete_incomplete_profile(&plan.profile_id, operation_id)?;
            let profile_root = self.registry.resolve("profiles", &plan.profile_id)?;
            secure_fs::remove_tree(&profile_root)?;
        }
        Ok(())
    }

    fn validate_plan_documents(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        if crate::operations::model::sha256_hex(plan.manifest_json.as_bytes())
            != plan.manifest_sha256
            || crate::operations::model::sha256_hex(plan.lock_json.as_bytes()) != plan.lock_sha256
        {
            return Err(AppError::coded("operation_plan_document_hash_mismatch"));
        }
        let manifest: ManifestIdentity = serde_json::from_str(&plan.manifest_json)?;
        let lock: LockIdentity = serde_json::from_str(&plan.lock_json)?;
        if manifest.profile_id != plan.profile_id
            || lock.profile_id != plan.profile_id
            || lock.revision_id != plan.revision_id
            || lock.manifest_sha256 != plan.manifest_sha256
        {
            return Err(AppError::coded("operation_plan_document_metadata_mismatch"));
        }
        if plan.runtime_projection.as_ref().is_some_and(|projection| {
            projection.profile_id != plan.profile_id || projection.revision_id != plan.revision_id
        }) || plan
            .previous_runtime_projection
            .as_ref()
            .is_some_and(|projection| {
                projection.profile_id != plan.profile_id
                    || Some(projection.revision_id.as_str()) != plan.previous_revision_id.as_deref()
            })
        {
            return Err(AppError::coded("runtime_projection_revision_mismatch"));
        }
        Ok(())
    }

    fn validate_plan_paths(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        self.registry
            .validate_unique("staging-operations", staging_derived_paths(plan))?;
        self.registry
            .validate_unique("profiles", profile_derived_paths(plan))?;
        Ok(())
    }

    fn validate_cache_materializations(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        for materialization in &plan.cache_materializations {
            let source = self.verified_cache_source(materialization)?;
            verify_file(
                source.absolute(),
                materialization.size_bytes,
                &materialization.blob_sha256,
                "operation_cache_blob_integrity_failed",
            )?;
        }
        Ok(())
    }

    fn verified_cache_source(
        &self,
        materialization: &crate::operations::model::CacheMaterialization,
    ) -> AppResult<crate::security::SecurePath> {
        let expected_relative = CacheStore::blob_relative_path(&materialization.blob_sha256)?;
        let blob = self
            .storage
            .cache_blob(&materialization.blob_sha256)?
            .ok_or_else(|| AppError::coded("operation_cache_blob_missing"))?;
        if blob.state != "verified" {
            return Err(AppError::coded("operation_cache_blob_not_verified"));
        }
        if blob.sha256 != materialization.blob_sha256
            || blob.size_bytes != materialization.size_bytes
        {
            return Err(AppError::coded("operation_cache_blob_metadata_mismatch"));
        }
        if blob.relative_path != expected_relative {
            return Err(AppError::coded("operation_cache_blob_path_mismatch"));
        }
        self.registry.resolve("cache-blobs", expected_relative)
    }

    pub(crate) fn replace_revision_cache_references(
        &self,
        plan: &ProfileInstallPlan,
    ) -> AppResult<()> {
        self.validate_cache_materializations(plan)?;
        let hashes: Vec<String> = plan
            .cache_materializations
            .iter()
            .map(|materialization| materialization.blob_sha256.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.storage
            .replace_cache_references(REVISION_CACHE_OWNER_TYPE, &plan.revision_id, &hashes)
    }

    fn stage(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        let paths = self
            .registry
            .validate_unique("staging-operations", staging_file_paths(plan))?;
        let manifest = &paths[0];
        let lock = &paths[1];
        secure_fs::write_new(manifest, plan.manifest_json.as_bytes())?;
        secure_fs::write_new(lock, plan.lock_json.as_bytes())?;
        for (secure, file) in paths.iter().skip(2).zip(&plan.payload_files) {
            secure_fs::write_new(secure, file.content_utf8.as_bytes())?;
        }
        let materialization_offset = 2 + plan.payload_files.len();
        for (secure, materialization) in paths
            .iter()
            .skip(materialization_offset)
            .zip(&plan.cache_materializations)
        {
            let source = self.verified_cache_source(materialization)?;
            verify_file(
                source.absolute(),
                materialization.size_bytes,
                &materialization.blob_sha256,
                "operation_cache_blob_integrity_failed",
            )?;
            let copied = secure_fs::copy_new(&source, secure)?;
            let verified = copied == materialization.size_bytes
                && verify_file(
                    secure.absolute(),
                    materialization.size_bytes,
                    &materialization.blob_sha256,
                    "operation_cache_copy_verification_failed",
                )
                .is_ok();
            if !verified {
                let _ = secure_fs::remove_tree(secure);
                return Err(AppError::coded("operation_cache_copy_verification_failed"));
            }
        }
        self.storage.append_journal(
            &plan.operation_id,
            "staging-written",
            "completed",
            &json!({"files": paths.len()}).to_string(),
            &json!({"action": "remove-staging"}).to_string(),
        )?;
        Ok(())
    }

    fn verify_staged(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        let base = format!("{}/revision", plan.operation_id);
        let manifest = self
            .registry
            .resolve("staging-operations", format!("{base}/manifest.json"))?;
        let lock = self
            .registry
            .resolve("staging-operations", format!("{base}/lock.json"))?;
        let manifest_bytes = fs::read(manifest.absolute())?;
        let lock_bytes = fs::read(lock.absolute())?;
        if crate::operations::model::sha256_hex(&manifest_bytes) != plan.manifest_sha256 {
            return Err(AppError::coded("staging_manifest_hash_mismatch"));
        }
        if crate::operations::model::sha256_hex(&lock_bytes) != plan.lock_sha256 {
            return Err(AppError::coded("staging_lock_hash_mismatch"));
        }
        let parsed_lock: LockIdentity = serde_json::from_slice(&lock_bytes)?;
        if parsed_lock.manifest_sha256 != plan.manifest_sha256
            || parsed_lock.profile_id != plan.profile_id
            || parsed_lock.revision_id != plan.revision_id
        {
            return Err(AppError::coded("staging_lock_metadata_mismatch"));
        }
        for file in &plan.payload_files {
            let secure = self.registry.resolve(
                "staging-operations",
                format!("{base}/{}", file.relative_path),
            )?;
            if crate::operations::model::sha256_hex(&fs::read(secure.absolute())?) != file.sha256 {
                return Err(AppError::coded_with(
                    "staging_payload_hash_mismatch",
                    [("path", file.relative_path.clone())],
                ));
            }
        }
        for materialization in &plan.cache_materializations {
            let secure = self.registry.resolve(
                "staging-operations",
                format!("{base}/{}", materialization.relative_path),
            )?;
            verify_file(
                secure.absolute(),
                materialization.size_bytes,
                &materialization.blob_sha256,
                "staging_cache_materialization_mismatch",
            )?;
        }
        Ok(())
    }

    fn commit_revision_directory(
        &self,
        plan: &ProfileInstallPlan,
    ) -> AppResult<crate::security::SecurePath> {
        let source = self.registry.resolve(
            "staging-operations",
            format!("{}/revision", plan.operation_id),
        )?;
        let destination = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{}", plan.profile_id, plan.revision_id),
        )?;
        // Staging and profiles are separate registered roots below one launcher root.
        // The checked cross-root rename is atomic because both paths stay on that volume.
        if destination.absolute().exists() {
            return Err(AppError::coded("profile_revision_already_exists"));
        }
        secure_fs::rename_new_within_parent(&source, &destination, &self.paths.root)?;
        Ok(destination)
    }

    pub(crate) fn cleanup_staging(&self, operation_id: &str) -> AppResult<()> {
        let staging = self.registry.resolve("staging-operations", operation_id)?;
        secure_fs::remove_tree(&staging)
    }

    fn transition(&self, operation_id: &str, state: OperationState) -> AppResult<()> {
        self.storage
            .update_operation_state(operation_id, state, None)?;
        self.storage.append_journal(
            operation_id,
            "state-transition",
            "completed",
            &json!({"state": state.as_str()}).to_string(),
            "{}",
        )?;
        Ok(())
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub(crate) fn registry(&self) -> &Arc<PathRegistry> {
        &self.registry
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestIdentity {
    profile_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockIdentity {
    profile_id: String,
    revision_id: String,
    manifest_sha256: String,
}

fn staging_file_paths(plan: &ProfileInstallPlan) -> Vec<String> {
    let base = format!("{}/revision", plan.operation_id);
    std::iter::once(format!("{base}/manifest.json"))
        .chain(std::iter::once(format!("{base}/lock.json")))
        .chain(
            plan.payload_files
                .iter()
                .map(|file| format!("{base}/{}", file.relative_path)),
        )
        .chain(
            plan.cache_materializations
                .iter()
                .map(|materialization| format!("{base}/{}", materialization.relative_path)),
        )
        .collect()
}

pub(crate) fn staging_derived_paths(plan: &ProfileInstallPlan) -> Vec<String> {
    let revision = format!("{}/revision", plan.operation_id);
    std::iter::once(plan.operation_id.clone())
        .chain(std::iter::once(revision))
        .chain(staging_file_paths(plan))
        .collect()
}

fn profile_file_paths(plan: &ProfileInstallPlan) -> Vec<String> {
    let base = format!("{}/revisions/{}", plan.profile_id, plan.revision_id);
    std::iter::once(format!("{base}/manifest.json"))
        .chain(std::iter::once(format!("{base}/lock.json")))
        .chain(
            plan.payload_files
                .iter()
                .map(|file| format!("{base}/{}", file.relative_path)),
        )
        .chain(
            plan.cache_materializations
                .iter()
                .map(|materialization| format!("{base}/{}", materialization.relative_path)),
        )
        .collect()
}

pub(crate) fn profile_derived_paths(plan: &ProfileInstallPlan) -> Vec<String> {
    let revisions = format!("{}/revisions", plan.profile_id);
    let revision = format!("{revisions}/{}", plan.revision_id);
    std::iter::once(plan.profile_id.clone())
        .chain(std::iter::once(revisions))
        .chain(std::iter::once(revision))
        .chain(profile_file_paths(plan))
        .collect()
}

fn verify_file(
    path: &Path,
    expected_size: u64,
    expected_sha256: &str,
    code: &str,
) -> AppResult<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() != expected_size || hash_file(path)? != expected_sha256
    {
        return Err(AppError::coded(code));
    }
    Ok(())
}

fn hash_file(path: &Path) -> AppResult<String> {
    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        foundation::CoreServices,
        operations::model::{CacheMaterialization, FailAt},
        security::fs as secure_fs,
        storage::models::RuntimeQueryProjection,
    };

    fn activate_cache_fixture(core: &CoreServices, label: &str, bytes: &[u8]) -> String {
        let staging_relative = format!("cache-operation-{label}/source.bin");
        let source = core
            .registry()
            .resolve("staging-operations", &staging_relative)
            .expect("resolve cache fixture source");
        secure_fs::write_new(&source, bytes).expect("write cache fixture");
        let hash = crate::operations::model::sha256_hex(bytes);
        core.cache()
            .activate_verified_copy(&staging_relative, &hash, bytes.len() as u64)
            .expect("activate cache fixture");
        hash
    }

    fn materialization(
        blob_sha256: &str,
        bytes: &[u8],
        relative_path: &str,
    ) -> CacheMaterialization {
        CacheMaterialization {
            blob_sha256: blob_sha256.to_string(),
            size_bytes: bytes.len() as u64,
            relative_path: relative_path.to_string(),
        }
    }

    fn runtime_plan(
        core: &CoreServices,
        profile_id: &str,
        blob_sha256: &str,
        bytes: &[u8],
        relative_path: &str,
    ) -> ProfileInstallPlan {
        let previous_revision_id = core
            .storage()
            .profile(profile_id)
            .expect("profile query")
            .expect("profile")
            .active_revision_id;
        let mut plan =
            ProfileInstallPlan::new(profile_id, "Runtime", previous_revision_id).expect("plan");
        plan.payload_files.clear();
        plan.cache_materializations = vec![materialization(blob_sha256, bytes, relative_path)];
        plan
    }

    fn runtime_projection(
        profile_id: &str,
        revision_id: &str,
        minecraft_version: &str,
    ) -> RuntimeQueryProjection {
        RuntimeQueryProjection {
            profile_id: profile_id.to_string(),
            revision_id: revision_id.to_string(),
            minecraft_version: minecraft_version.to_string(),
            loader_kind: "vanilla".into(),
            loader_version: None,
            component_id: None,
            component_version: None,
            install_state: "installed".into(),
            updated_at_unix: 1,
        }
    }

    // `set_readonly(false)` is the platform-appropriate way to clear the
    // Windows read-only attribute; the Unix branch sets only the owner bit.
    #[cfg_attr(windows, allow(clippy::permissions_set_readonly_false))]
    fn make_test_file_writable(path: &Path) {
        let mut permissions = fs::metadata(path).expect("cache permissions").permissions();
        #[cfg(windows)]
        permissions.set_readonly(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(permissions.mode() | 0o200);
        }
        fs::set_permissions(path, permissions).expect("make cache fixture writable");
    }

    #[test]
    fn cache_materialization_commits_an_independent_copy_and_typed_reference() {
        let root = crate::foundation::test_root("operation-cache-copy");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Runtime copy")
            .expect("profile");
        let bytes = b"verified runtime artifact";
        let hash = activate_cache_fixture(&core, "copy", bytes);
        let plan = runtime_plan(
            &core,
            &profile_id,
            &hash,
            bytes,
            "instance/mods/runtime.jar",
        );

        core.operations()
            .plan_profile_operation(&plan, OperationType::RuntimeInstall)
            .expect("register runtime operation");
        core.operations()
            .execute(&plan.operation_id)
            .expect("execute runtime operation");

        let materialized = root
            .join("profiles")
            .join(&profile_id)
            .join("revisions")
            .join(&plan.revision_id)
            .join("instance/mods/runtime.jar");
        let cache_blob = root
            .join("cache/blobs/sha256")
            .join(CacheStore::blob_relative_path(&hash).expect("cache relative path"));
        assert_eq!(fs::read(&materialized).expect("materialized"), bytes);
        assert_eq!(
            core.storage()
                .cache_references_for_owner(REVISION_CACHE_OWNER_TYPE, &plan.revision_id)
                .expect("cache references"),
            vec![hash.clone()]
        );

        fs::write(&materialized, b"locally changed runtime").expect("mutate materialized copy");
        assert_eq!(fs::read(&cache_blob).expect("cache remains intact"), bytes);
        assert!(!fs::metadata(&materialized)
            .expect("materialized metadata")
            .permissions()
            .readonly());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_ne!(
                fs::metadata(&cache_blob).expect("cache metadata").ino(),
                fs::metadata(&materialized)
                    .expect("materialized metadata")
                    .ino()
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manipulated_cache_blob_is_rejected_before_revision_commit() {
        let root = crate::foundation::test_root("operation-cache-tamper");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Runtime tamper")
            .expect("profile");
        let bytes = b"trusted runtime bytes";
        let hash = activate_cache_fixture(&core, "tamper", bytes);
        let plan = runtime_plan(
            &core,
            &profile_id,
            &hash,
            bytes,
            "instance/libraries/runtime.jar",
        );
        core.operations()
            .plan_profile_operation(&plan, OperationType::RuntimeInstall)
            .expect("register runtime operation");

        let cache_blob = root
            .join("cache/blobs/sha256")
            .join(CacheStore::blob_relative_path(&hash).expect("cache relative path"));
        make_test_file_writable(&cache_blob);
        fs::write(&cache_blob, b"forged! runtime bytes").expect("tamper cache fixture");
        let error = core
            .operations()
            .execute(&plan.operation_id)
            .expect_err("tampered cache must fail");
        assert_eq!(
            error.descriptor().code,
            "operation_cache_blob_integrity_failed"
        );
        assert!(!root
            .join("profiles")
            .join(&profile_id)
            .join("revisions")
            .join(&plan.revision_id)
            .exists());
        assert!(core
            .storage()
            .cache_references_for_owner(REVISION_CACHE_OWNER_TYPE, &plan.revision_id)
            .expect("cache references")
            .is_empty());
        assert!(!root
            .join("profiles")
            .join(&profile_id)
            .join("instance")
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn materialization_paths_reject_traversal_ambiguity_and_ads_before_journaling() {
        let root = crate::foundation::test_root("operation-cache-paths");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Runtime paths")
            .expect("profile");
        let bytes = b"x";
        let hash = crate::operations::model::sha256_hex(bytes);

        for (relative_path, expected_code) in [
            ("../escape.jar", "path_traversal"),
            ("instance/mods//ambiguous.jar", "path_ambiguous_separator"),
            (
                "instance/mods/runtime.jar:stream",
                "path_alternate_data_stream",
            ),
        ] {
            let plan = runtime_plan(&core, &profile_id, &hash, bytes, relative_path);
            let error = core
                .operations()
                .plan_profile_operation(&plan, OperationType::RuntimeInstall)
                .expect_err("unsafe materialization path must fail");
            assert_eq!(error.descriptor().code, expected_code, "{relative_path}");
            assert!(core
                .storage()
                .operation(&plan.operation_id)
                .expect("operation query")
                .is_none());
        }
        assert!(!root
            .join("profiles")
            .join(&profile_id)
            .join("instance")
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn precommit_failure_never_mutates_instance_and_recovery_removes_staging() {
        let root = crate::foundation::test_root("operation-cache-precommit");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Runtime precommit")
            .expect("profile");
        let bytes = b"runtime before commit";
        let hash = activate_cache_fixture(&core, "precommit", bytes);
        let plan = runtime_plan(
            &core,
            &profile_id,
            &hash,
            bytes,
            "instance/runtime/runtime.jar",
        );
        core.operations()
            .plan_profile_operation(&plan, OperationType::RuntimeInstall)
            .expect("register runtime operation");

        core.operations()
            .execute_with_injector(&plan.operation_id, &FailAt(FailurePoint::AfterStaging))
            .expect_err("inject precommit interruption");
        assert!(!root
            .join("profiles")
            .join(&profile_id)
            .join("instance")
            .exists());
        assert!(!root
            .join("profiles")
            .join(&profile_id)
            .join("revisions")
            .join(&plan.revision_id)
            .exists());
        assert!(root
            .join("staging/operations")
            .join(&plan.operation_id)
            .join("revision/instance/runtime/runtime.jar")
            .is_file());
        drop(core);

        let recovered = CoreServices::open_fixed(&root).expect("recover");
        assert_eq!(
            recovered
                .storage()
                .operation(&plan.operation_id)
                .expect("operation")
                .expect("operation record")
                .state,
            OperationState::RolledBack
        );
        assert!(!root
            .join("staging/operations")
            .join(&plan.operation_id)
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn runtime_projection_activation_and_rollback_are_atomic_with_revision() {
        let root = crate::foundation::test_root("operation-runtime-projection-atomic");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Runtime projection")
            .expect("profile");
        let bytes = b"runtime projection artifact";
        let hash = activate_cache_fixture(&core, "runtime-projection", bytes);

        let mut first = runtime_plan(
            &core,
            &profile_id,
            &hash,
            bytes,
            "instance/runtime/first.jar",
        );
        let first_projection = runtime_projection(&profile_id, &first.revision_id, "1.21.1");
        first.runtime_projection = Some(first_projection.clone());
        core.operations()
            .plan_profile_operation(&first, OperationType::RuntimeInstall)
            .expect("register first runtime");
        core.operations()
            .execute(&first.operation_id)
            .expect("activate first runtime");
        assert_eq!(
            core.storage()
                .runtime_projection(&profile_id)
                .expect("first projection"),
            Some(first_projection.clone())
        );

        let mut second = runtime_plan(
            &core,
            &profile_id,
            &hash,
            bytes,
            "instance/runtime/second.jar",
        );
        second.previous_runtime_projection = Some(first_projection.clone());
        second.runtime_projection = Some(runtime_projection(
            &profile_id,
            &second.revision_id,
            "1.21.4",
        ));
        core.operations()
            .plan_profile_operation(&second, OperationType::RuntimeRepair)
            .expect("register second runtime");
        core.operations()
            .execute_controlled_with_injector(
                &second.operation_id,
                &FailAt(FailurePoint::AfterDatabaseActivated),
            )
            .expect_err("inject post-activation failure");

        assert_eq!(
            core.storage()
                .profile(&profile_id)
                .expect("profile")
                .expect("profile record")
                .active_revision_id
                .as_deref(),
            Some(first.revision_id.as_str())
        );
        assert_eq!(
            core.storage()
                .runtime_projection(&profile_id)
                .expect("restored projection"),
            Some(first_projection)
        );
        assert_eq!(
            core.storage()
                .revision(&second.revision_id)
                .expect("second revision query")
                .expect("rollback audit record")
                .status,
            "invalidated"
        );
        assert!(!root
            .join("profiles")
            .join(&profile_id)
            .join("revisions")
            .join(&second.revision_id)
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reference_checkpoint_is_compensated_or_completed_during_recovery() {
        let controlled_root = crate::foundation::test_root("operation-cache-compensation");
        let controlled = CoreServices::open_fixed(&controlled_root).expect("core");
        let controlled_profile = controlled
            .operations()
            .create_minimal_profile("Runtime compensation")
            .expect("profile");
        let bytes = b"runtime reference checkpoint";
        let hash = activate_cache_fixture(&controlled, "compensation", bytes);
        let plan = runtime_plan(
            &controlled,
            &controlled_profile,
            &hash,
            bytes,
            "instance/runtime/reference.jar",
        );
        controlled
            .operations()
            .plan_profile_operation(&plan, OperationType::RuntimeRepair)
            .expect("register repair");
        controlled
            .operations()
            .execute_controlled_with_injector(
                &plan.operation_id,
                &FailAt(FailurePoint::AfterCacheReferences),
            )
            .expect_err("inject controlled reference failure");
        assert!(controlled
            .storage()
            .cache_references_for_owner(REVISION_CACHE_OWNER_TYPE, &plan.revision_id)
            .expect("compensated references")
            .is_empty());
        assert!(!controlled_root
            .join("profiles")
            .join(&controlled_profile)
            .join("revisions")
            .join(&plan.revision_id)
            .exists());
        drop(controlled);
        let _ = fs::remove_dir_all(controlled_root);

        let crash_root = crate::foundation::test_root("operation-cache-reference-recovery");
        let crashed = CoreServices::open_fixed(&crash_root).expect("core");
        let crash_profile = crashed
            .operations()
            .create_minimal_profile("Runtime recovery")
            .expect("profile");
        let crash_hash = activate_cache_fixture(&crashed, "reference-recovery", bytes);
        let crash_plan = runtime_plan(
            &crashed,
            &crash_profile,
            &crash_hash,
            bytes,
            "instance/runtime/reference.jar",
        );
        crashed
            .operations()
            .plan_profile_operation(&crash_plan, OperationType::ComponentChange)
            .expect("register component change");
        crashed
            .operations()
            .execute_with_injector(
                &crash_plan.operation_id,
                &FailAt(FailurePoint::AfterCacheReferences),
            )
            .expect_err("inject crash after references");
        drop(crashed);

        let recovered = CoreServices::open_fixed(&crash_root).expect("recover");
        assert_eq!(
            recovered
                .storage()
                .operation(&crash_plan.operation_id)
                .expect("operation")
                .expect("record")
                .state,
            OperationState::Completed
        );
        assert_eq!(
            recovered
                .storage()
                .cache_references_for_owner(REVISION_CACHE_OWNER_TYPE, &crash_plan.revision_id,)
                .expect("recovered references"),
            vec![crash_hash]
        );
        let _ = fs::remove_dir_all(crash_root);
    }
}
