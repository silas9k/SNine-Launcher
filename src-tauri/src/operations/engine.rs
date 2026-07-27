use crate::{
    app::paths::LauncherPaths,
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
use std::{fs, sync::Arc};

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
        let profile = self.storage.profile(&plan.profile_id)?.ok_or_else(|| {
            AppError::coded_with(
                "profile_not_found",
                [("profileId", plan.profile_id.clone())],
            )
        })?;
        if profile.active_revision_id != plan.previous_revision_id {
            return Err(AppError::coded("profile_revision_conflict"));
        }
        if plan.cleanup_profile_on_rollback && plan.previous_revision_id.is_some() {
            return Err(AppError::coded("profile_cleanup_flag_invalid"));
        }
        self.register_plan(plan, OperationType::ProfileRevision)
    }

    fn register_plan(
        &self,
        plan: &ProfileInstallPlan,
        operation_type: OperationType,
    ) -> AppResult<()> {
        self.validate_plan_documents(plan)?;
        self.validate_plan_paths(plan)?;
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

        self.storage.activate_revision(
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
            self.storage.restore_active_revision(
                &plan.profile_id,
                &plan.revision_id,
                plan.previous_revision_id.as_deref(),
            )?;
            self.storage.append_journal(
                operation_id,
                "database-activation-rolled-back",
                "compensated",
                &json!({"revisionId": &plan.revision_id}).to_string(),
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
            || crate::operations::model::sha256_hex(plan.lock_json.as_bytes())
                != plan.lock_sha256
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
        Ok(())
    }

    fn validate_plan_paths(&self, plan: &ProfileInstallPlan) -> AppResult<()> {
        self.registry
            .validate_unique("staging-operations", staging_derived_paths(plan))?;
        self.registry
            .validate_unique("profiles", profile_derived_paths(plan))?;
        Ok(())
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
