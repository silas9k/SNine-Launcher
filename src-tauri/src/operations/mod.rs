pub mod engine;
pub mod model;
pub mod recovery;

#[cfg(test)]
mod tests {
    use super::model::{FailAt, FailurePoint, OperationState};
    use crate::{
        app::paths::LauncherPaths,
        foundation::CoreServices,
        operations::{
            engine::{profile_derived_paths, staging_derived_paths},
            model::ProfileInstallPlan,
        },
        storage::sqlite::Connection,
    };
    use std::{fs, path::Path};

    const CRASH_POINTS: [FailurePoint; 7] = [
        FailurePoint::AfterPlanned,
        FailurePoint::AfterStaging,
        FailurePoint::AfterVerifying,
        FailurePoint::AfterReadyToCommit,
        FailurePoint::AfterRevisionMoved,
        FailurePoint::AfterDatabaseActivated,
        FailurePoint::DuringValidation,
    ];

    fn active_plan(core: &CoreServices, profile_id: &str) -> ProfileInstallPlan {
        let revision = core
            .storage()
            .active_revision(profile_id)
            .expect("active query")
            .expect("active revision");
        let operation = core
            .storage()
            .operation(&revision.operation_id)
            .expect("operation query")
            .expect("operation");
        serde_json::from_str(&operation.planned_changes_json).expect("plan")
    }

    fn assert_consistent(core: &CoreServices, profile_id: &str) {
        let plan = active_plan(core, profile_id);
        core.operations()
            .validate_active(&plan)
            .expect("active revision must be valid");
    }

    fn revision_path(root: &Path, plan: &ProfileInstallPlan) -> std::path::PathBuf {
        root.join("profiles")
            .join(&plan.profile_id)
            .join("revisions")
            .join(&plan.revision_id)
    }

    #[test]
    fn phase1_transaction_demo() {
        let root = crate::foundation::test_root("transaction-demo");
        let core = CoreServices::open_fixed(&root).expect("open core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Phase 1 Demo")
            .expect("create profile");
        let plan = core
            .operations()
            .plan_simulated_install(&profile_id, "Phase 1 Demo")
            .expect("plan");
        core.operations()
            .execute(&plan.operation_id)
            .expect("execute");
        assert_consistent(&core, &profile_id);
        assert_eq!(
            core.storage()
                .operation(&plan.operation_id)
                .expect("operation")
                .expect("record")
                .state,
            OperationState::Completed
        );
        assert!(!root
            .join("staging/operations")
            .join(&plan.operation_id)
            .exists());
        println!(
            "Phase-1-Demo abgeschlossen: profile={}, revision={}, state=completed",
            profile_id, plan.revision_id
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn controlled_abort_rolls_back_immediately() {
        let root = crate::foundation::test_root("controlled-abort");
        let core = CoreServices::open_fixed(&root).expect("open core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Controlled Abort")
            .expect("create profile");
        let baseline = core
            .operations()
            .plan_simulated_install(&profile_id, "Baseline")
            .expect("baseline plan");
        core.operations()
            .execute(&baseline.operation_id)
            .expect("baseline execute");
        let interrupted = core
            .operations()
            .plan_simulated_install(&profile_id, "Interrupted")
            .expect("interrupted plan");
        assert!(core
            .operations()
            .execute_controlled_with_injector(
                &interrupted.operation_id,
                &FailAt(FailurePoint::AfterRevisionMoved),
            )
            .is_err());
        let profile = core
            .storage()
            .profile(&profile_id)
            .expect("profile query")
            .expect("profile");
        assert_eq!(
            profile.active_revision_id.as_deref(),
            Some(baseline.revision_id.as_str())
        );
        assert_eq!(
            core.storage()
                .operation(&interrupted.operation_id)
                .expect("operation")
                .expect("record")
                .state,
            OperationState::RolledBack
        );
        assert!(!revision_path(&root, &interrupted).exists());
        assert_consistent(&core, &profile_id);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn crash_recovery_never_leaves_a_mixed_revision() {
        for failure_point in CRASH_POINTS {
            let root = crate::foundation::test_root(failure_point.as_str());
            let core = CoreServices::open_fixed(&root).expect("open core");
            let profile_id = core
                .operations()
                .create_minimal_profile("Recovery Demo")
                .expect("create profile");
            let baseline = core
                .operations()
                .plan_simulated_install(&profile_id, "Baseline")
                .expect("baseline plan");
            core.operations()
                .execute(&baseline.operation_id)
                .expect("baseline execute");
            let interrupted = core
                .operations()
                .plan_simulated_install(&profile_id, "Target")
                .expect("target plan");
            let result = core
                .operations()
                .execute_with_injector(&interrupted.operation_id, &FailAt(failure_point));
            assert!(result.is_err(), "failure point did not interrupt");
            drop(core);

            let recovered = CoreServices::open_fixed(&root).expect("restart and recover");
            let record = recovered
                .storage()
                .operation(&interrupted.operation_id)
                .expect("operation query")
                .expect("operation");
            let profile = recovered
                .storage()
                .profile(&profile_id)
                .expect("profile query")
                .expect("profile");
            let commit_happened = matches!(
                failure_point,
                FailurePoint::AfterDatabaseActivated | FailurePoint::DuringValidation
            );
            if commit_happened {
                assert_eq!(record.state, OperationState::Completed);
                assert_eq!(
                    profile.active_revision_id.as_deref(),
                    Some(interrupted.revision_id.as_str())
                );
                assert!(revision_path(&root, &interrupted).exists());
            } else {
                assert_eq!(record.state, OperationState::RolledBack);
                assert_eq!(
                    profile.active_revision_id.as_deref(),
                    Some(baseline.revision_id.as_str())
                );
                assert!(!revision_path(&root, &interrupted).exists());
            }
            assert_consistent(&recovered, &profile_id);
            assert!(!root
                .join("staging/operations")
                .join(&interrupted.operation_id)
                .exists());
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn generated_profile_and_staging_paths_fit_the_documented_budget() {
        let root = crate::foundation::test_root("path-budget");
        let core = CoreServices::open_fixed(&root).expect("open core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Path Budget")
            .expect("create profile");
        let plan = core
            .operations()
            .plan_simulated_install(&profile_id, "Path Budget")
            .expect("plan install within path budget");
        let payload = plan
            .payload_files
            .first()
            .expect("phase 1 plan contains a payload");

        let profile_relative = format!(
            "{}/revisions/{}/{}",
            plan.profile_id, plan.revision_id, payload.relative_path
        );
        let staging_relative = format!("{}/revision/{}", plan.operation_id, payload.relative_path);
        let profile_id_length = plan.profile_id.encode_utf16().count();
        let revision_id_length = plan.revision_id.encode_utf16().count();
        let operation_id_length = plan.operation_id.encode_utf16().count();
        let target_file_length = payload.relative_path.encode_utf16().count();
        let profile_length = profile_relative.encode_utf16().count();
        let staging_length = staging_relative.encode_utf16().count();

        assert_eq!(profile_id_length, 40, "generated profile ID model changed");
        assert_eq!(
            revision_id_length, 36,
            "generated revision ID model changed"
        );
        assert_eq!(
            operation_id_length, 35,
            "generated operation ID model changed"
        );
        assert_eq!(
            target_file_length, 29,
            "generated target file model changed"
        );
        assert_eq!(profile_length, 117, "generated profile path model changed");
        assert_eq!(staging_length, 74, "generated staging path model changed");

        let profile_budget = core
            .registry()
            .length_budget("profiles")
            .expect("profile path budget");
        let staging_budget = core
            .registry()
            .length_budget("staging-operations")
            .expect("staging path budget");
        assert!(profile_length <= profile_budget.available_relative_utf16);
        assert!(staging_length <= staging_budget.available_relative_utf16);
        let profile_path = core
            .registry()
            .resolve("profiles", &profile_relative)
            .expect("generated profile path must resolve");
        let staging_path = core
            .registry()
            .resolve("staging-operations", &staging_relative)
            .expect("generated staging path must resolve");
        let profile_absolute_length = profile_path
            .absolute()
            .to_string_lossy()
            .encode_utf16()
            .count();
        let staging_absolute_length = staging_path
            .absolute()
            .to_string_lossy()
            .encode_utf16()
            .count();
        assert!(profile_absolute_length <= profile_budget.max_absolute_utf16);
        assert!(staging_absolute_length <= staging_budget.max_absolute_utf16);

        #[cfg(windows)]
        {
            assert_eq!(
                profile_budget.max_absolute_utf16,
                crate::security::LEGACY_SAFE_MAX_ABSOLUTE_UTF16
            );
            assert_eq!(
                staging_budget.max_absolute_utf16,
                crate::security::LEGACY_SAFE_MAX_ABSOLUTE_UTF16
            );
        }

        drop(core);
        fs::remove_dir_all(root).expect("remove path budget test root");
    }

    fn utf16_path_length(path: &Path) -> usize {
        path.to_string_lossy().encode_utf16().count()
    }

    fn required_plan_absolute_utf16(root: &Path, plan: &ProfileInstallPlan) -> usize {
        let paths = LauncherPaths::from_root(root.to_path_buf()).expect("resolve launcher paths");
        profile_derived_paths(plan)
            .into_iter()
            .map(|relative| utf16_path_length(&paths.profiles.join(relative)))
            .chain(
                staging_derived_paths(plan)
                    .into_iter()
                    .map(|relative| utf16_path_length(&paths.staging_operations.join(relative))),
            )
            .max()
            .expect("plan contains derived paths")
    }

    fn sqlite_table_count(database: &Path, table: &str) -> i64 {
        assert!(
            matches!(table, "operations" | "operation_journal"),
            "test helper only permits fixed table names"
        );
        let connection = Connection::open(database).expect("open database for count");
        connection
            .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
            .expect("query table count")
            .expect("count row")
            .integer(0)
            .expect("integer count")
    }

    fn assert_no_planning_side_effects(core: &CoreServices) {
        assert_eq!(
            sqlite_table_count(core.storage().database_path(), "operations"),
            0,
            "path preflight must not create an operation row"
        );
        assert_eq!(
            sqlite_table_count(core.storage().database_path(), "operation_journal"),
            0,
            "path preflight must not create a journal row"
        );
        let mut staging_entries = fs::read_dir(&core.paths().staging_operations)
            .expect("read staging operations directory");
        assert!(
            staging_entries.next().is_none(),
            "path preflight must not create an operation staging directory"
        );
    }

    #[test]
    fn operation_plan_preflight_enforces_the_real_root_budget_before_journaling() {
        let fixture_root = crate::foundation::test_root("operation-preflight-budget");
        let allowed_root = fixture_root.join("allowed");
        let rejected_root = fixture_root.join("rejected");
        let synthetic_profile_id = format!("profile-{}", "a".repeat(32));

        let allowed_probe =
            ProfileInstallPlan::new(synthetic_profile_id.clone(), "Allowed Boundary", None)
                .expect("create allowed budget probe");
        let allowed_limit = required_plan_absolute_utf16(&allowed_root, &allowed_probe);
        let allowed_core =
            CoreServices::open_fixed_with_absolute_path_limit(&allowed_root, allowed_limit)
                .expect("open boundary core");
        let allowed_profile = allowed_core
            .operations()
            .create_minimal_profile("Allowed Boundary")
            .expect("create boundary profile");
        allowed_core
            .operations()
            .plan_simulated_install(&allowed_profile, "Allowed Boundary")
            .expect("generated operation paths at the exact budget must be accepted");
        drop(allowed_core);

        let rejected_probe =
            ProfileInstallPlan::new(synthetic_profile_id, "Rejected Boundary", None)
                .expect("create rejected budget probe");
        let rejected_required = required_plan_absolute_utf16(&rejected_root, &rejected_probe);
        let rejected_limit = rejected_required
            .checked_sub(1)
            .expect("derived path requirement must be positive");
        let rejected_core =
            CoreServices::open_fixed_with_absolute_path_limit(&rejected_root, rejected_limit)
                .expect("open over-budget core");
        let rejected_profile = rejected_core
            .operations()
            .create_minimal_profile("Rejected Boundary")
            .expect("create over-budget profile");
        let error = rejected_core
            .operations()
            .plan_simulated_install(&rejected_profile, "Rejected Boundary")
            .expect_err("generated operation paths one UTF-16 unit beyond the budget must fail");
        assert_eq!(error.descriptor().code, "path_too_long");
        assert_no_planning_side_effects(&rejected_core);

        drop(rejected_core);
        fs::remove_dir_all(fixture_root).expect("remove deterministic budget fixture");
    }

    #[test]
    fn corrupt_or_mismatched_plan_is_failed_without_changing_active_revision() {
        use crate::storage::sqlite::{Connection, Value};

        let root = crate::foundation::test_root("invalid-plan-recovery");
        let core = CoreServices::open_fixed(&root).expect("open core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Invalid Plan Recovery")
            .expect("create profile");
        let baseline = core
            .operations()
            .plan_simulated_install(&profile_id, "Baseline")
            .expect("baseline plan");
        core.operations()
            .execute(&baseline.operation_id)
            .expect("baseline execute");
        let target = core
            .operations()
            .plan_simulated_install(&profile_id, "Tampered")
            .expect("target plan");

        let mut tampered = target.clone();
        tampered.operation_id = "different-operation-id".into();
        let connection = Connection::open(core.storage().database_path()).expect("open database");
        connection
            .execute(
                "UPDATE operations SET planned_changes_json = ?2 WHERE id = ?1",
                &[
                    Value::from(target.operation_id.as_str()),
                    Value::from(serde_json::to_string(&tampered).expect("serialize tampered plan")),
                ],
            )
            .expect("tamper test operation");
        drop(connection);
        drop(core);

        let recovered = CoreServices::open_fixed(&root).expect("restart and recover");
        let record = recovered
            .storage()
            .operation(&target.operation_id)
            .expect("operation query")
            .expect("operation");
        let profile = recovered
            .storage()
            .profile(&profile_id)
            .expect("profile query")
            .expect("profile");
        assert_eq!(record.state, OperationState::Failed);
        assert_eq!(
            profile.active_revision_id.as_deref(),
            Some(baseline.revision_id.as_str())
        );
        assert_consistent(&recovered, &profile_id);
        assert!(!root
            .join("staging/operations")
            .join(&target.operation_id)
            .exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn manifest_lock_and_sqlite_revision_are_activated_together() {
        let root = crate::foundation::test_root("authority");
        let core = CoreServices::open_fixed(&root).expect("open core");
        let profile_id = core
            .operations()
            .create_minimal_profile("Authority")
            .expect("create profile");
        let plan = core
            .operations()
            .plan_simulated_install(&profile_id, "Authority")
            .expect("plan");
        core.operations()
            .execute(&plan.operation_id)
            .expect("execute");
        let revision = core
            .storage()
            .active_revision(&profile_id)
            .expect("active query")
            .expect("active revision");
        assert_eq!(revision.manifest_sha256, plan.manifest_sha256);
        assert_eq!(revision.lock_sha256, plan.lock_sha256);
        assert_consistent(&core, &profile_id);
        let _ = fs::remove_dir_all(root);
    }
}
