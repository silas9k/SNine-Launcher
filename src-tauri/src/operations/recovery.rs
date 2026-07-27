use crate::{
    error::{AppError, AppResult},
    operations::{engine::OperationEngine, model::OperationState},
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryResult {
    pub operation_id: String,
    pub original_state: String,
    pub final_state: String,
}

impl OperationEngine {
    pub fn recover_incomplete(&self) -> AppResult<Vec<RecoveryResult>> {
        let mut results = Vec::new();
        for operation in self.storage().incomplete_operations()? {
            let original_state = operation.state;
            let plan_result: AppResult<crate::operations::model::ProfileInstallPlan> =
                serde_json::from_str(&operation.planned_changes_json)
                    .map_err(AppError::from)
                    .and_then(|plan| {
                        self.validate_plan_record(&operation, &plan)?;
                        Ok(plan)
                    });
            let plan = match plan_result {
                Ok(plan) => plan,
                Err(error) => {
                    // Even a corrupt journal plan must not leave an unverified target active.
                    if let (Some(profile_id), Some(target_revision_id)) = (
                        operation.profile_id.as_deref(),
                        operation.target_revision_id.as_deref(),
                    ) {
                        let profile = self.storage().profile(profile_id)?;
                        if profile
                            .as_ref()
                            .and_then(|item| item.active_revision_id.as_deref())
                            == Some(target_revision_id)
                        {
                            self.storage().restore_active_revision(
                                profile_id,
                                target_revision_id,
                                operation.previous_revision_id.as_deref(),
                            )?;
                        }
                        let target = self.registry().resolve(
                            "profiles",
                            format!("{profile_id}/revisions/{target_revision_id}"),
                        )?;
                        crate::security::fs::remove_tree(&target)?;
                    }
                    self.cleanup_staging(&operation.id)?;
                    let failure = AppError::coded_with(
                        "operation_plan_invalid",
                        [("detail", error.to_string())],
                    );
                    let descriptor = failure.descriptor();
                    let params = serde_json::to_string(&descriptor.params)?;
                    self.storage().update_operation_state(
                        &operation.id,
                        OperationState::Failed,
                        Some((&descriptor.code, &params)),
                    )?;
                    results.push(RecoveryResult {
                        operation_id: operation.id,
                        original_state: original_state.as_str().into(),
                        final_state: OperationState::Failed.as_str().into(),
                    });
                    continue;
                }
            };
            let recovery_error = AppError::coded_with(
                "operation_interrupted",
                [("state", original_state.as_str())],
            );

            let final_state = match original_state {
                OperationState::Validating => {
                    if self.validate_active(&plan).is_ok() {
                        self.cleanup_staging(&operation.id)?;
                        self.storage().update_operation_state(
                            &operation.id,
                            OperationState::Completed,
                            None,
                        )?;
                        OperationState::Completed
                    } else {
                        self.rollback(&operation.id, &recovery_error)?;
                        OperationState::RolledBack
                    }
                }
                OperationState::Committing => {
                    let profile = self.storage().profile(&plan.profile_id)?;
                    if profile
                        .as_ref()
                        .and_then(|item| item.active_revision_id.as_deref())
                        == Some(plan.revision_id.as_str())
                        && self.validate_active(&plan).is_ok()
                    {
                        self.cleanup_staging(&operation.id)?;
                        self.storage().update_operation_state(
                            &operation.id,
                            OperationState::Completed,
                            None,
                        )?;
                        OperationState::Completed
                    } else {
                        self.rollback(&operation.id, &recovery_error)?;
                        OperationState::RolledBack
                    }
                }
                OperationState::Planned
                | OperationState::Staging
                | OperationState::Verifying
                | OperationState::ReadyToCommit
                | OperationState::RollingBack => {
                    self.rollback(&operation.id, &recovery_error)?;
                    OperationState::RolledBack
                }
                OperationState::Completed | OperationState::RolledBack | OperationState::Failed => {
                    original_state
                }
            };
            results.push(RecoveryResult {
                operation_id: operation.id,
                original_state: original_state.as_str().into(),
                final_state: final_state.as_str().into(),
            });
        }
        Ok(results)
    }
}
