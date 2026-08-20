pub mod migrations;
pub mod models;
pub mod sqlite;

use crate::{
    error::{AppError, AppResult},
    operations::model::{OperationState, OperationType},
    security::{fs as secure_fs, SecurePath},
    storage::{
        migrations::LATEST_SCHEMA_VERSION,
        models::{
            AccountRecord, CacheBlobRecord, JournalRecord, OperationRecord, ProfileRecord,
            RevisionRecord, RuntimeQueryProjection,
        },
        sqlite::{Connection, Value},
    },
};
use chrono::Utc;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Storage {
    database_path: PathBuf,
}

impl Storage {
    pub(crate) fn initialize(database_path: &SecurePath) -> AppResult<Self> {
        secure_fs::create_parent_directories(database_path)?;
        Self::initialize_path(database_path.absolute().to_path_buf())
    }

    #[cfg(test)]
    pub(crate) fn initialize_for_test(database_path: impl Into<PathBuf>) -> AppResult<Self> {
        let database_path = database_path.into();
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::initialize_path(database_path)
    }

    fn initialize_path(database_path: PathBuf) -> AppResult<Self> {
        let storage = Self { database_path };
        let connection = storage.open()?;
        let version = migrations::apply_all(&connection)?;
        if version != LATEST_SCHEMA_VERSION {
            return Err(AppError::coded_with(
                "storage_schema_incomplete",
                [("version", version.to_string())],
            ));
        }
        Ok(storage)
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn schema_version(&self) -> AppResult<i64> {
        migrations::current_version(&self.open()?)
    }

    pub fn accounts(&self) -> AppResult<Vec<AccountRecord>> {
        self.open()?
            .query(
                "SELECT id, username, account_kind, vault_ref, session_state,\
                        ownership_verified_at_unix, last_online_auth_at_unix,\
                        added_at_unix, last_used_at_unix \
                 FROM accounts ORDER BY last_used_at_unix DESC, id",
                &[],
            )?
            .into_iter()
            .map(parse_account_row)
            .collect()
    }

    pub fn account(&self, id: &str) -> AppResult<Option<AccountRecord>> {
        self.open()?
            .query_one(
                "SELECT id, username, account_kind, vault_ref, session_state,\
                        ownership_verified_at_unix, last_online_auth_at_unix,\
                        added_at_unix, last_used_at_unix \
                 FROM accounts WHERE id = ?1",
                &[Value::from(id)],
            )?
            .map(parse_account_row)
            .transpose()
    }

    pub fn upsert_account(&self, record: &AccountRecord) -> AppResult<Option<String>> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let previous_vault = transaction
                .query_one(
                    "SELECT vault_ref FROM accounts WHERE id = ?1",
                    &[Value::from(record.id.as_str())],
                )?
                .map(|row| row.text(0))
                .transpose()?;
            if previous_vault.is_some() {
                transaction.execute(
                    "UPDATE accounts SET username = ?2, account_kind = ?3, vault_ref = ?4,\
                            session_state = ?5, ownership_verified_at_unix = ?6,\
                            last_online_auth_at_unix = ?7, last_used_at_unix = ?8 \
                     WHERE id = ?1",
                    &[
                        Value::from(record.id.as_str()),
                        Value::from(record.username.as_str()),
                        Value::from(record.account_kind.as_str()),
                        Value::from(record.vault_ref.as_str()),
                        Value::from(record.session_state.as_str()),
                        Value::Integer(record.ownership_verified_at_unix),
                        Value::Integer(record.last_online_auth_at_unix),
                        Value::Integer(record.last_used_at_unix),
                    ],
                )?;
            } else {
                transaction.execute(
                    "INSERT INTO accounts(\
                        id, username, account_kind, vault_ref, session_state,\
                        ownership_verified_at_unix, last_online_auth_at_unix,\
                        added_at_unix, last_used_at_unix\
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    &[
                        Value::from(record.id.as_str()),
                        Value::from(record.username.as_str()),
                        Value::from(record.account_kind.as_str()),
                        Value::from(record.vault_ref.as_str()),
                        Value::from(record.session_state.as_str()),
                        Value::Integer(record.ownership_verified_at_unix),
                        Value::Integer(record.last_online_auth_at_unix),
                        Value::Integer(record.added_at_unix),
                        Value::Integer(record.last_used_at_unix),
                    ],
                )?;
            }
            transaction.execute(
                "UPDATE launcher_account_selection \
                 SET active_account_id = ?1, updated_at_unix = ?2 WHERE singleton = 1",
                &[
                    Value::from(record.id.as_str()),
                    Value::Integer(Utc::now().timestamp()),
                ],
            )?;
            Ok(previous_vault)
        })
    }

    pub fn selected_account_id(&self) -> AppResult<Option<String>> {
        self.open()?
            .query_one(
                "SELECT active_account_id FROM launcher_account_selection WHERE singleton = 1",
                &[],
            )?
            .ok_or_else(|| AppError::coded("account_selection_missing"))?
            .optional_text(0)
    }

    pub fn select_account(&self, account_id: &str) -> AppResult<AccountRecord> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE accounts SET last_used_at_unix = ?2 WHERE id = ?1",
                &[
                    Value::from(account_id),
                    Value::Integer(Utc::now().timestamp()),
                ],
            )?;
            if changed != 1 {
                return Err(AppError::AccountNotFound(account_id.to_string()));
            }
            transaction.execute(
                "UPDATE launcher_account_selection \
                 SET active_account_id = ?1, updated_at_unix = ?2 WHERE singleton = 1",
                &[
                    Value::from(account_id),
                    Value::Integer(Utc::now().timestamp()),
                ],
            )?;
            transaction
                .query_one(
                    "SELECT id, username, account_kind, vault_ref, session_state,\
                            ownership_verified_at_unix, last_online_auth_at_unix,\
                            added_at_unix, last_used_at_unix \
                     FROM accounts WHERE id = ?1",
                    &[Value::from(account_id)],
                )?
                .ok_or_else(|| AppError::AccountNotFound(account_id.to_string()))
                .and_then(parse_account_row)
        })
    }

    pub fn mark_account_relogin_required(&self, account_id: &str) -> AppResult<()> {
        let changed = self.open()?.execute(
            "UPDATE accounts SET session_state = 'relogin-required' WHERE id = ?1",
            &[Value::from(account_id)],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(AppError::AccountNotFound(account_id.to_string()))
        }
    }

    pub fn delete_account(&self, account_id: &str) -> AppResult<AccountRecord> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let record = transaction
                .query_one(
                    "SELECT id, username, account_kind, vault_ref, session_state,\
                            ownership_verified_at_unix, last_online_auth_at_unix,\
                            added_at_unix, last_used_at_unix \
                     FROM accounts WHERE id = ?1",
                    &[Value::from(account_id)],
                )?
                .ok_or_else(|| AppError::AccountNotFound(account_id.to_string()))
                .and_then(parse_account_row)?;
            transaction.execute(
                "DELETE FROM accounts WHERE id = ?1",
                &[Value::from(account_id)],
            )?;
            Ok(record)
        })
    }

    pub fn assign_profile_account(
        &self,
        profile_id: &str,
        account_id: Option<&str>,
    ) -> AppResult<()> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let profile_exists = transaction
                .query_one(
                    "SELECT 1 FROM profiles WHERE id = ?1",
                    &[Value::from(profile_id)],
                )?
                .is_some();
            if !profile_exists {
                return Err(AppError::coded_with(
                    "profile_not_found",
                    [("profileId", profile_id.to_string())],
                ));
            }
            transaction.execute(
                "DELETE FROM profile_account_assignments WHERE profile_id = ?1",
                &[Value::from(profile_id)],
            )?;
            if let Some(account_id) = account_id {
                let account_exists = transaction
                    .query_one(
                        "SELECT 1 FROM accounts WHERE id = ?1",
                        &[Value::from(account_id)],
                    )?
                    .is_some();
                if !account_exists {
                    return Err(AppError::AccountNotFound(account_id.to_string()));
                }
                transaction.execute(
                    "INSERT INTO profile_account_assignments(profile_id, account_id, assigned_at_unix)\
                     VALUES (?1, ?2, ?3)",
                    &[
                        Value::from(profile_id),
                        Value::from(account_id),
                        Value::Integer(Utc::now().timestamp()),
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn profile_account_id(&self, profile_id: &str) -> AppResult<Option<String>> {
        self.open()?
            .query_one(
                "SELECT account_id FROM profile_account_assignments WHERE profile_id = ?1",
                &[Value::from(profile_id)],
            )?
            .map(|row| row.text(0))
            .transpose()
    }

    pub fn create_profile(&self, id: &str) -> AppResult<ProfileRecord> {
        let suffix: String = id.chars().take(8).collect();
        self.create_profile_with_metadata(id, &format!("Profile {suffix}"), None)
    }

    pub fn create_profile_with_metadata(
        &self,
        id: &str,
        display_name: &str,
        source_profile_id: Option<&str>,
    ) -> AppResult<ProfileRecord> {
        let now = Utc::now().timestamp();
        let connection = self.open()?;
        connection.transaction(|transaction| {
            transaction.execute(
                "INSERT INTO profiles(id, lifecycle_state, active_revision_id, created_at_unix, updated_at_unix) \
                 VALUES (?1, 'active', NULL, ?2, ?2)",
                &[Value::from(id), Value::Integer(now)],
            )?;
            transaction.execute(
                "INSERT INTO profile_metadata(\
                    profile_id, display_name, favorite, verification_state, trashed_from_state\
                 ) VALUES (?1, ?2, 0, 'verified', NULL)",
                &[Value::from(id), Value::from(display_name)],
            )?;
            transaction.execute(
                "INSERT INTO profile_lineage(profile_id, source_profile_id, duplicated_at_unix) \
                 VALUES (?1, ?2, ?3)",
                &[
                    Value::from(id),
                    source_profile_id.map(Value::from).unwrap_or(Value::Null),
                    Value::Integer(now),
                ],
            )?;
            Ok(())
        })?;
        self.profile(id)?
            .ok_or_else(|| AppError::coded("profile_create_failed"))
    }

    pub fn profile(&self, id: &str) -> AppResult<Option<ProfileRecord>> {
        let row = self.open()?.query_one(
            "SELECT p.id, m.display_name, p.lifecycle_state, p.active_revision_id,\
                    m.favorite, m.verification_state, m.trashed_from_state,\
                    l.source_profile_id, a.account_id, p.created_at_unix, p.updated_at_unix \
             FROM profiles p \
             JOIN profile_metadata m ON m.profile_id = p.id \
             JOIN profile_lineage l ON l.profile_id = p.id \
             LEFT JOIN profile_account_assignments a ON a.profile_id = p.id \
             WHERE p.id = ?1",
            &[Value::from(id)],
        )?;
        row.map(parse_profile_row).transpose()
    }

    pub fn profiles(&self) -> AppResult<Vec<ProfileRecord>> {
        self.open()?
            .query(
                "SELECT p.id, m.display_name, p.lifecycle_state, p.active_revision_id,\
                        m.favorite, m.verification_state, m.trashed_from_state,\
                        l.source_profile_id, a.account_id, p.created_at_unix, p.updated_at_unix \
                 FROM profiles p \
                 JOIN profile_metadata m ON m.profile_id = p.id \
                 JOIN profile_lineage l ON l.profile_id = p.id \
                 LEFT JOIN profile_account_assignments a ON a.profile_id = p.id \
                 ORDER BY m.favorite DESC, m.display_name COLLATE NOCASE, p.id",
                &[],
            )?
            .into_iter()
            .map(parse_profile_row)
            .collect()
    }

    pub fn runtime_projection(
        &self,
        profile_id: &str,
    ) -> AppResult<Option<RuntimeQueryProjection>> {
        validate_runtime_text("profileId", profile_id, 128)?;
        self.open()?
            .query_one(
                "SELECT profile_id, revision_id, minecraft_version, loader_kind,\
                        loader_version, component_id, component_version, install_state,\
                        updated_at_unix \
                 FROM profile_runtime_projection WHERE profile_id = ?1",
                &[Value::from(profile_id)],
            )?
            .map(parse_runtime_projection_row)
            .transpose()
    }

    pub fn runtime_projections(&self) -> AppResult<Vec<RuntimeQueryProjection>> {
        self.open()?
            .query(
                "SELECT profile_id, revision_id, minecraft_version, loader_kind,\
                        loader_version, component_id, component_version, install_state,\
                        updated_at_unix \
                 FROM profile_runtime_projection ORDER BY profile_id",
                &[],
            )?
            .into_iter()
            .map(parse_runtime_projection_row)
            .collect()
    }

    pub fn upsert_runtime_projection(&self, record: &RuntimeQueryProjection) -> AppResult<()> {
        validate_runtime_projection(record)?;
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let revision = transaction.query_one(
                "SELECT profile_id, status FROM profile_revisions WHERE id = ?1",
                &[Value::from(record.revision_id.as_str())],
            )?;
            let revision = revision.ok_or_else(|| {
                AppError::coded_with(
                    "runtime_revision_not_found",
                    [("revisionId", record.revision_id.clone())],
                )
            })?;
            let revision_profile_id = revision.text(0)?;
            if revision_profile_id != record.profile_id {
                return Err(AppError::coded_with(
                    "runtime_revision_profile_mismatch",
                    [
                        ("profileId", record.profile_id.clone()),
                        ("revisionId", record.revision_id.clone()),
                    ],
                ));
            }
            if revision.text(1)? != "committed" {
                return Err(AppError::coded_with(
                    "runtime_revision_not_committed",
                    [("revisionId", record.revision_id.clone())],
                ));
            }

            transaction.execute(
                "INSERT INTO profile_runtime_projection(\
                    profile_id, revision_id, minecraft_version, loader_kind, loader_version,\
                    component_id, component_version, install_state, updated_at_unix\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                 ON CONFLICT(profile_id) DO UPDATE SET \
                    revision_id = excluded.revision_id,\
                    minecraft_version = excluded.minecraft_version,\
                    loader_kind = excluded.loader_kind,\
                    loader_version = excluded.loader_version,\
                    component_id = excluded.component_id,\
                    component_version = excluded.component_version,\
                    install_state = excluded.install_state,\
                    updated_at_unix = excluded.updated_at_unix",
                &[
                    Value::from(record.profile_id.as_str()),
                    Value::from(record.revision_id.as_str()),
                    Value::from(record.minecraft_version.as_str()),
                    Value::from(record.loader_kind.as_str()),
                    record
                        .loader_version
                        .as_deref()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    record
                        .component_id
                        .as_deref()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    record
                        .component_version
                        .as_deref()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    Value::from(record.install_state.as_str()),
                    Value::Integer(record.updated_at_unix),
                ],
            )?;
            Ok(())
        })
    }

    pub fn delete_runtime_projection(&self, profile_id: &str) -> AppResult<bool> {
        validate_runtime_text("profileId", profile_id, 128)?;
        let connection = self.open()?;
        connection.transaction(|transaction| {
            transaction
                .execute(
                    "DELETE FROM profile_runtime_projection WHERE profile_id = ?1",
                    &[Value::from(profile_id)],
                )
                .map(|changed| changed == 1)
        })
    }

    pub fn set_profile_favorite(&self, profile_id: &str, favorite: bool) -> AppResult<()> {
        let changed = self.open()?.execute(
            "UPDATE profile_metadata SET favorite = ?2 WHERE profile_id = ?1",
            &[
                Value::from(profile_id),
                Value::Integer(if favorite { 1 } else { 0 }),
            ],
        )?;
        if changed == 1 {
            Ok(())
        } else {
            Err(AppError::coded_with(
                "profile_not_found",
                [("profileId", profile_id.to_string())],
            ))
        }
    }

    pub fn archive_profile(&self, profile_id: &str) -> AppResult<ProfileRecord> {
        self.update_profile_lifecycle(profile_id, "archived")
    }

    pub fn trash_profile(&self, profile_id: &str) -> AppResult<ProfileRecord> {
        self.update_profile_lifecycle(profile_id, "trash")
    }

    pub fn restore_profile(&self, profile_id: &str) -> AppResult<ProfileRecord> {
        let record = self.profile(profile_id)?.ok_or_else(|| {
            AppError::coded_with("profile_not_found", [("profileId", profile_id)])
        })?;
        let target = match record.lifecycle_state.as_str() {
            "archived" => "active",
            "trash" => record.trashed_from_state.as_deref().unwrap_or("active"),
            _ => {
                return Err(AppError::coded_with(
                    "profile_lifecycle_transition_invalid",
                    [
                        ("from", record.lifecycle_state),
                        ("to", "active".to_string()),
                    ],
                ))
            }
        };
        self.update_profile_lifecycle(profile_id, target)
    }

    fn update_profile_lifecycle(&self, profile_id: &str, target: &str) -> AppResult<ProfileRecord> {
        let now = Utc::now().timestamp();
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let current = transaction
                .query_one(
                    "SELECT lifecycle_state FROM profiles WHERE id = ?1",
                    &[Value::from(profile_id)],
                )?
                .ok_or_else(|| {
                    AppError::coded_with("profile_not_found", [("profileId", profile_id)])
                })?
                .text(0)?;
            let allowed = matches!(
                (current.as_str(), target),
                ("active", "archived")
                    | ("active", "trash")
                    | ("archived", "active")
                    | ("archived", "trash")
                    | ("trash", "active")
                    | ("trash", "archived")
            );
            if !allowed {
                return Err(AppError::coded_with(
                    "profile_lifecycle_transition_invalid",
                    [("from", current), ("to", target.to_string())],
                ));
            }
            let archived_at = if target == "archived" {
                Value::Integer(now)
            } else {
                Value::Null
            };
            let deleted_at = if target == "trash" {
                Value::Integer(now)
            } else {
                Value::Null
            };
            transaction.execute(
                "UPDATE profiles SET lifecycle_state = ?2, updated_at_unix = ?3,\
                        archived_at_unix = ?4, deleted_at_unix = ?5 WHERE id = ?1",
                &[
                    Value::from(profile_id),
                    Value::from(target),
                    Value::Integer(now),
                    archived_at,
                    deleted_at,
                ],
            )?;
            transaction.execute(
                "UPDATE profile_metadata SET trashed_from_state = ?2 WHERE profile_id = ?1",
                &[
                    Value::from(profile_id),
                    if target == "trash" {
                        Value::from(current)
                    } else {
                        Value::Null
                    },
                ],
            )?;
            Ok(())
        })?;
        self.profile(profile_id)?
            .ok_or_else(|| AppError::coded("profile_not_found"))
    }

    pub fn detach_and_delete_incomplete_profile(
        &self,
        profile_id: &str,
        operation_id: &str,
    ) -> AppResult<()> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let active = transaction
                .query_one(
                    "SELECT active_revision_id FROM profiles WHERE id = ?1",
                    &[Value::from(profile_id)],
                )?
                .ok_or_else(|| AppError::coded("profile_not_found"))?
                .optional_text(0)?;
            if active.is_some() {
                return Err(AppError::coded(
                    "profile_incomplete_cleanup_has_active_revision",
                ));
            }
            let state = transaction
                .query_one(
                    "SELECT state FROM operations WHERE id = ?1 AND profile_id = ?2",
                    &[Value::from(operation_id), Value::from(profile_id)],
                )?
                .ok_or_else(|| AppError::coded("operation_not_found"))?
                .text(0)?;
            if !matches!(state.as_str(), "rolled-back" | "failed") {
                return Err(AppError::coded(
                    "profile_incomplete_cleanup_operation_not_terminal",
                ));
            }
            transaction.execute(
                "UPDATE operations SET profile_id = NULL WHERE id = ?1 AND profile_id = ?2",
                &[Value::from(operation_id), Value::from(profile_id)],
            )?;
            transaction.execute(
                "DELETE FROM profiles WHERE id = ?1",
                &[Value::from(profile_id)],
            )?;
            Ok(())
        })
    }

    pub fn delete_unactivated_profile(&self, profile_id: &str) -> AppResult<()> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let active = transaction
                .query_one(
                    "SELECT active_revision_id FROM profiles WHERE id = ?1",
                    &[Value::from(profile_id)],
                )?
                .ok_or_else(|| AppError::coded("profile_not_found"))?
                .optional_text(0)?;
            if active.is_some() {
                return Err(AppError::coded("profile_unactivated_cleanup_has_revision"));
            }
            let operations = transaction
                .query_one(
                    "SELECT COUNT(*) FROM operations WHERE profile_id = ?1",
                    &[Value::from(profile_id)],
                )?
                .ok_or_else(|| AppError::coded("profile_operation_count_missing"))?
                .integer(0)?;
            if operations != 0 {
                return Err(AppError::coded("profile_unactivated_cleanup_has_operation"));
            }
            transaction.execute(
                "DELETE FROM profiles WHERE id = ?1",
                &[Value::from(profile_id)],
            )?;
            Ok(())
        })
    }

    pub fn insert_operation(&self, operation: &OperationRecord) -> AppResult<()> {
        self.open()?.execute(
            "INSERT INTO operations(\
                id, operation_type, profile_id, state, planned_changes_json, staging_relative_path,\
                previous_revision_id, target_revision_id, started_at_unix, completed_at_unix,\
                error_code, error_params_json\
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            &[
                Value::from(operation.id.clone()),
                Value::from(operation.operation_type.as_str()),
                optional_text(&operation.profile_id),
                Value::from(operation.state.as_str()),
                Value::from(operation.planned_changes_json.clone()),
                Value::from(operation.staging_relative_path.clone()),
                optional_text(&operation.previous_revision_id),
                optional_text(&operation.target_revision_id),
                Value::Integer(operation.started_at_unix),
                optional_integer(operation.completed_at_unix),
                optional_text(&operation.error_code),
                optional_text(&operation.error_params_json),
            ],
        )?;
        Ok(())
    }

    pub fn operation(&self, id: &str) -> AppResult<Option<OperationRecord>> {
        let row = self.open()?.query_one(
            "SELECT id, operation_type, profile_id, state, planned_changes_json, staging_relative_path,\
                    previous_revision_id, target_revision_id, started_at_unix, completed_at_unix,\
                    error_code, error_params_json \
             FROM operations WHERE id = ?1",
            &[Value::from(id)],
        )?;
        row.map(parse_operation_row).transpose()
    }

    pub fn incomplete_operations(&self) -> AppResult<Vec<OperationRecord>> {
        self.open()?
            .query(
                "SELECT id, operation_type, profile_id, state, planned_changes_json, staging_relative_path,\
                        previous_revision_id, target_revision_id, started_at_unix, completed_at_unix,\
                        error_code, error_params_json \
                 FROM operations \
                 WHERE state NOT IN ('completed', 'rolled-back', 'failed')\
                 ORDER BY started_at_unix, id",
                &[],
            )?
            .into_iter()
            .map(parse_operation_row)
            .collect()
    }

    pub fn update_operation_state(
        &self,
        id: &str,
        state: OperationState,
        error: Option<(&str, &str)>,
    ) -> AppResult<()> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let current = transaction
                .query_one("SELECT state FROM operations WHERE id = ?1", &[Value::from(id)])?
                .ok_or_else(|| {
                    AppError::coded_with(
                        "operation_not_found",
                        [("operationId", id.to_string())],
                    )
                })?;
            let current = OperationState::parse(&current.text(0)?)?;
            if !current.can_transition_to(state) {
                return Err(AppError::coded_with(
                    "operation_invalid_state_transition",
                    [
                        ("from", current.as_str().to_string()),
                        ("to", state.as_str().to_string()),
                    ],
                ));
            }

            let completed = state.is_terminal().then(|| Utc::now().timestamp());
            let (error_code, error_params) = error
                .map(|(code, params)| (Value::from(code), Value::from(params)))
                .unwrap_or((Value::Null, Value::Null));
            let changed = transaction.execute(
                "UPDATE operations                 SET state = ?2, completed_at_unix = ?3, error_code = ?4, error_params_json = ?5                 WHERE id = ?1",
                &[
                    Value::from(id),
                    Value::from(state.as_str()),
                    optional_integer(completed),
                    error_code,
                    error_params,
                ],
            )?;
            if changed != 1 {
                return Err(AppError::coded_with(
                    "operation_not_found",
                    [("operationId", id.to_string())],
                ));
            }
            Ok(())
        })
    }

    pub fn append_journal(
        &self,
        operation_id: &str,
        step: &str,
        status: &str,
        details_json: &str,
        compensation_json: &str,
    ) -> AppResult<i64> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let sequence = transaction
                .query_one(
                    "SELECT COALESCE(MAX(sequence), 0) + 1 FROM operation_journal WHERE operation_id = ?1",
                    &[Value::from(operation_id)],
                )?
                .ok_or_else(|| AppError::coded("operation_journal_sequence_missing"))?
                .integer(0)?;
            transaction.execute(
                "INSERT INTO operation_journal(\
                    operation_id, sequence, step, status, details_json, compensation_json, created_at_unix\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                &[
                    Value::from(operation_id),
                    Value::Integer(sequence),
                    Value::from(step),
                    Value::from(status),
                    Value::from(details_json),
                    Value::from(compensation_json),
                    Value::Integer(Utc::now().timestamp()),
                ],
            )?;
            Ok(sequence)
        })
    }

    pub fn journal(&self, operation_id: &str) -> AppResult<Vec<JournalRecord>> {
        self.open()?
            .query(
                "SELECT sequence, step, status, details_json, compensation_json \
                 FROM operation_journal WHERE operation_id = ?1 ORDER BY sequence",
                &[Value::from(operation_id)],
            )?
            .into_iter()
            .map(|row| {
                Ok(JournalRecord {
                    sequence: row.integer(0)?,
                    step: row.text(1)?,
                    status: row.text(2)?,
                    details_json: row.text(3)?,
                    compensation_json: row.text(4)?,
                })
            })
            .collect()
    }

    pub fn activate_revision(
        &self,
        revision: &RevisionRecord,
        expected_previous_revision: Option<&str>,
    ) -> AppResult<()> {
        self.activate_revision_with_runtime_projection(revision, expected_previous_revision, None)
    }

    pub fn activate_revision_with_runtime_projection(
        &self,
        revision: &RevisionRecord,
        expected_previous_revision: Option<&str>,
        runtime_projection: Option<&RuntimeQueryProjection>,
    ) -> AppResult<()> {
        if let Some(projection) = runtime_projection {
            validate_runtime_projection(projection)?;
            if projection.profile_id != revision.profile_id || projection.revision_id != revision.id
            {
                return Err(AppError::coded("runtime_projection_revision_mismatch"));
            }
        }
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let active = transaction
                .query_one(
                    "SELECT active_revision_id FROM profiles WHERE id = ?1",
                    &[Value::from(revision.profile_id.as_str())],
                )?
                .ok_or_else(|| AppError::coded("profile_not_found"))?
                .optional_text(0)?;
            if active.as_deref() != expected_previous_revision {
                return Err(AppError::coded_with(
                    "profile_revision_conflict",
                    [
                        ("profileId", revision.profile_id.clone()),
                        (
                            "expected",
                            expected_previous_revision.unwrap_or("").to_string(),
                        ),
                        ("actual", active.unwrap_or_default()),
                    ],
                ));
            }
            transaction.execute(
                "INSERT INTO profile_revisions(\
                    id, profile_id, operation_id, manifest_sha256, lock_sha256,\
                    manifest_relative_path, lock_relative_path, status, created_at_unix\
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                &[
                    Value::from(revision.id.as_str()),
                    Value::from(revision.profile_id.as_str()),
                    Value::from(revision.operation_id.as_str()),
                    Value::from(revision.manifest_sha256.as_str()),
                    Value::from(revision.lock_sha256.as_str()),
                    Value::from(revision.manifest_relative_path.as_str()),
                    Value::from(revision.lock_relative_path.as_str()),
                    Value::from(revision.status.as_str()),
                    Value::Integer(revision.created_at_unix),
                ],
            )?;
            transaction.execute(
                "UPDATE profiles SET active_revision_id = ?2, updated_at_unix = ?3 WHERE id = ?1",
                &[
                    Value::from(revision.profile_id.as_str()),
                    Value::from(revision.id.as_str()),
                    Value::Integer(Utc::now().timestamp()),
                ],
            )?;
            if let Some(projection) = runtime_projection {
                upsert_runtime_projection_row(transaction, projection)?;
            }
            Ok(())
        })
    }

    pub fn restore_active_revision(
        &self,
        profile_id: &str,
        current_revision: &str,
        previous_revision: Option<&str>,
    ) -> AppResult<()> {
        self.restore_active_revision_with_runtime_projection(
            profile_id,
            current_revision,
            previous_revision,
            None,
        )
    }

    pub fn restore_active_revision_with_runtime_projection(
        &self,
        profile_id: &str,
        current_revision: &str,
        previous_revision: Option<&str>,
        runtime_projection: Option<&RuntimeQueryProjection>,
    ) -> AppResult<()> {
        if let Some(projection) = runtime_projection {
            validate_runtime_projection(projection)?;
            if projection.profile_id != profile_id
                || Some(projection.revision_id.as_str()) != previous_revision
            {
                return Err(AppError::coded("runtime_projection_revision_mismatch"));
            }
        }
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let current = transaction
                .query_one(
                    "SELECT active_revision_id FROM profiles WHERE id = ?1",
                    &[Value::from(profile_id)],
                )?
                .ok_or_else(|| AppError::coded("profile_not_found"))?
                .optional_text(0)?;
            if current.as_deref() != Some(current_revision) {
                return Err(AppError::coded("profile_revision_conflict"));
            }
            transaction.execute(
                "UPDATE profiles SET active_revision_id = ?2, updated_at_unix = ?3 WHERE id = ?1",
                &[
                    Value::from(profile_id),
                    previous_revision
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    Value::Integer(Utc::now().timestamp()),
                ],
            )?;
            transaction.execute(
                "UPDATE profile_revisions SET status = 'invalidated' WHERE id = ?1 AND profile_id = ?2",
                &[Value::from(current_revision), Value::from(profile_id)],
            )?;
            if let Some(projection) = runtime_projection {
                upsert_runtime_projection_row(transaction, projection)?;
            }
            Ok(())
        })
    }

    pub fn revision(&self, id: &str) -> AppResult<Option<RevisionRecord>> {
        let row = self.open()?.query_one(
            "SELECT id, profile_id, operation_id, manifest_sha256, lock_sha256,\
                    manifest_relative_path, lock_relative_path, status, created_at_unix \
             FROM profile_revisions WHERE id = ?1",
            &[Value::from(id)],
        )?;
        row.map(|row| {
            Ok(RevisionRecord {
                id: row.text(0)?,
                profile_id: row.text(1)?,
                operation_id: row.text(2)?,
                manifest_sha256: row.text(3)?,
                lock_sha256: row.text(4)?,
                manifest_relative_path: row.text(5)?,
                lock_relative_path: row.text(6)?,
                status: row.text(7)?,
                created_at_unix: row.integer(8)?,
            })
        })
        .transpose()
    }

    pub fn profile_revisions(&self, profile_id: &str) -> AppResult<Vec<RevisionRecord>> {
        self.open()?
            .query(
                "SELECT id, profile_id, operation_id, manifest_sha256, lock_sha256,\
                        manifest_relative_path, lock_relative_path, status, created_at_unix \
                 FROM profile_revisions WHERE profile_id = ?1 AND status = 'committed' \
                 ORDER BY created_at_unix DESC, id DESC",
                &[Value::from(profile_id)],
            )?
            .into_iter()
            .map(|row| {
                Ok(RevisionRecord {
                    id: row.text(0)?,
                    profile_id: row.text(1)?,
                    operation_id: row.text(2)?,
                    manifest_sha256: row.text(3)?,
                    lock_sha256: row.text(4)?,
                    manifest_relative_path: row.text(5)?,
                    lock_relative_path: row.text(6)?,
                    status: row.text(7)?,
                    created_at_unix: row.integer(8)?,
                })
            })
            .collect()
    }

    pub fn active_revision(&self, profile_id: &str) -> AppResult<Option<RevisionRecord>> {
        let profile = self
            .profile(profile_id)?
            .ok_or_else(|| AppError::coded("profile_not_found"))?;
        match profile.active_revision_id {
            Some(id) => self.revision(&id),
            None => Ok(None),
        }
    }

    pub fn insert_cache_blob(
        &self,
        sha256: &str,
        size_bytes: u64,
        relative_path: &str,
        state: &str,
    ) -> AppResult<()> {
        let size_bytes =
            i64::try_from(size_bytes).map_err(|_| AppError::coded("size_too_large"))?;
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let existing = transaction.query_one(
                "SELECT size_bytes, relative_path, state FROM cache_blobs WHERE sha256 = ?1",
                &[Value::from(sha256)],
            )?;
            if let Some(existing) = existing {
                if existing.integer(0)? != size_bytes || existing.text(1)? != relative_path {
                    return Err(AppError::coded("cache_blob_metadata_conflict"));
                }
                if existing.text(2)? == "quarantined" && state != "quarantined" {
                    return Err(AppError::coded("cache_blob_requires_reactivation"));
                }
                transaction.execute(
                    "UPDATE cache_blobs SET state = ?2, last_verified_at_unix = ?3 WHERE sha256 = ?1",
                    &[
                        Value::from(sha256),
                        Value::from(state),
                        Value::Integer(Utc::now().timestamp()),
                    ],
                )?;
            } else {
                transaction.execute(
                    "INSERT INTO cache_blobs(sha256, size_bytes, relative_path, state, created_at_unix, last_verified_at_unix)                     VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    &[
                        Value::from(sha256),
                        Value::Integer(size_bytes),
                        Value::from(relative_path),
                        Value::from(state),
                        Value::Integer(Utc::now().timestamp()),
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn cache_blob(&self, sha256: &str) -> AppResult<Option<CacheBlobRecord>> {
        self.open()?
            .query_one(
                "SELECT b.sha256, b.size_bytes, b.relative_path, b.state,\
                        b.created_at_unix, b.last_verified_at_unix,\
                        q.quarantine_relative_path, q.quarantined_at_unix \
                 FROM cache_blobs b \
                 LEFT JOIN cache_quarantine q ON q.blob_sha256 = b.sha256 \
                 WHERE b.sha256 = ?1",
                &[Value::from(sha256)],
            )?
            .map(parse_cache_blob_row)
            .transpose()
    }

    pub fn cache_blobs(&self) -> AppResult<Vec<CacheBlobRecord>> {
        self.open()?
            .query(
                "SELECT b.sha256, b.size_bytes, b.relative_path, b.state,\
                        b.created_at_unix, b.last_verified_at_unix,\
                        q.quarantine_relative_path, q.quarantined_at_unix \
                 FROM cache_blobs b \
                 LEFT JOIN cache_quarantine q ON q.blob_sha256 = b.sha256 \
                 ORDER BY b.sha256",
                &[],
            )?
            .into_iter()
            .map(parse_cache_blob_row)
            .collect()
    }

    pub fn add_cache_reference(
        &self,
        sha256: &str,
        owner_type: &str,
        owner_id: &str,
    ) -> AppResult<()> {
        self.open()?.execute(
            "INSERT OR IGNORE INTO cache_references(\
                blob_sha256, owner_type, owner_id, created_at_unix\
             ) VALUES (?1, ?2, ?3, ?4)",
            &[
                Value::from(sha256),
                Value::from(owner_type),
                Value::from(owner_id),
                Value::Integer(Utc::now().timestamp()),
            ],
        )?;
        Ok(())
    }

    pub fn replace_cache_references(
        &self,
        owner_type: &str,
        owner_id: &str,
        hashes: &[String],
    ) -> AppResult<()> {
        validate_cache_reference_owner(owner_type, owner_id)?;
        let mut hashes = hashes.to_vec();
        hashes.sort();
        hashes.dedup();
        for hash in &hashes {
            validate_cache_hash(hash)?;
        }
        let connection = self.open()?;
        connection.transaction(|transaction| {
            transaction.execute(
                "DELETE FROM cache_references WHERE owner_type = ?1 AND owner_id = ?2",
                &[Value::from(owner_type), Value::from(owner_id)],
            )?;
            for hash in &hashes {
                let blob = transaction.query_one(
                    "SELECT state FROM cache_blobs WHERE sha256 = ?1",
                    &[Value::from(hash.as_str())],
                )?;
                if blob.as_ref().map(|row| row.text(0)).transpose()?.as_deref() != Some("verified")
                {
                    return Err(AppError::coded_with(
                        "cache_reference_blob_not_verified",
                        [("sha256", hash.clone())],
                    ));
                }
                transaction.execute(
                    "INSERT INTO cache_references(\
                        blob_sha256, owner_type, owner_id, created_at_unix\
                     ) VALUES (?1, ?2, ?3, ?4)",
                    &[
                        Value::from(hash.as_str()),
                        Value::from(owner_type),
                        Value::from(owner_id),
                        Value::Integer(Utc::now().timestamp()),
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn remove_cache_references(&self, owner_type: &str, owner_id: &str) -> AppResult<usize> {
        validate_cache_reference_owner(owner_type, owner_id)?;
        self.open()?.execute(
            "DELETE FROM cache_references WHERE owner_type = ?1 AND owner_id = ?2",
            &[Value::from(owner_type), Value::from(owner_id)],
        )
    }

    pub fn cache_references_for_owner(
        &self,
        owner_type: &str,
        owner_id: &str,
    ) -> AppResult<Vec<String>> {
        validate_cache_reference_owner(owner_type, owner_id)?;
        self.open()?
            .query(
                "SELECT blob_sha256 FROM cache_references \
                 WHERE owner_type = ?1 AND owner_id = ?2 ORDER BY blob_sha256",
                &[Value::from(owner_type), Value::from(owner_id)],
            )?
            .into_iter()
            .map(|row| row.text(0))
            .collect()
    }

    pub fn cache_reference_hashes(&self) -> AppResult<Vec<String>> {
        self.open()?
            .query(
                "SELECT DISTINCT blob_sha256 FROM cache_references ORDER BY blob_sha256",
                &[],
            )?
            .into_iter()
            .map(|row| row.text(0))
            .collect()
    }

    pub fn mark_cache_quarantined(
        &self,
        sha256: &str,
        quarantine_relative_path: &str,
    ) -> AppResult<()> {
        let now = Utc::now().timestamp();
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let changed = transaction.execute(
                "UPDATE cache_blobs SET state = 'quarantined' \
                 WHERE sha256 = ?1 AND state = 'verified'",
                &[Value::from(sha256)],
            )?;
            if changed != 1 {
                return Err(AppError::coded("cache_blob_not_verified"));
            }
            transaction.execute(
                "INSERT INTO cache_quarantine(\
                    blob_sha256, quarantine_relative_path, quarantined_at_unix, deletion_policy\
                 ) VALUES (?1, ?2, ?3, 'unconfigured')",
                &[
                    Value::from(sha256),
                    Value::from(quarantine_relative_path),
                    Value::Integer(now),
                ],
            )?;
            Ok(())
        })
    }

    pub fn mark_cache_reactivated(&self, sha256: &str) -> AppResult<()> {
        let connection = self.open()?;
        connection.transaction(|transaction| {
            let removed = transaction.execute(
                "DELETE FROM cache_quarantine WHERE blob_sha256 = ?1",
                &[Value::from(sha256)],
            )?;
            let changed = transaction.execute(
                "UPDATE cache_blobs SET state = 'verified', last_verified_at_unix = ?2 \
                 WHERE sha256 = ?1 AND state = 'quarantined'",
                &[Value::from(sha256), Value::Integer(Utc::now().timestamp())],
            )?;
            if removed != 1 || changed != 1 {
                return Err(AppError::coded("cache_quarantine_metadata_missing"));
            }
            Ok(())
        })
    }

    pub fn schema_sql(&self) -> AppResult<String> {
        let rows = self.open()?.query(
            "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name",
            &[],
        )?;
        let mut result = String::new();
        for row in rows {
            result.push_str(&row.text(0)?);
            result.push('\n');
        }
        Ok(result)
    }

    fn open(&self) -> AppResult<Connection> {
        Connection::open(&self.database_path)
    }
}

fn optional_text(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map(|item| Value::from(item.as_str()))
        .unwrap_or(Value::Null)
}

fn optional_integer(value: Option<i64>) -> Value {
    value.map(Value::Integer).unwrap_or(Value::Null)
}

fn validate_cache_reference_owner(owner_type: &str, owner_id: &str) -> AppResult<()> {
    const ALLOWED_OWNER_TYPES: &[&str] = &[
        "profile-revision",
        "runtime-launch",
        "backup",
        "recovery",
        "operation",
    ];
    if !ALLOWED_OWNER_TYPES.contains(&owner_type)
        || owner_id.is_empty()
        || owner_id.len() > 160
        || !owner_id.is_ascii()
        || !owner_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::coded("cache_reference_owner_invalid"));
    }
    Ok(())
}

fn validate_cache_hash(hash: &str) -> AppResult<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded("cache_reference_hash_invalid"));
    }
    Ok(())
}

fn parse_operation_row(row: sqlite::Row) -> AppResult<OperationRecord> {
    Ok(OperationRecord {
        id: row.text(0)?,
        operation_type: OperationType::parse(&row.text(1)?)?,
        profile_id: row.optional_text(2)?,
        state: OperationState::parse(&row.text(3)?)?,
        planned_changes_json: row.text(4)?,
        staging_relative_path: row.text(5)?,
        previous_revision_id: row.optional_text(6)?,
        target_revision_id: row.optional_text(7)?,
        started_at_unix: row.integer(8)?,
        completed_at_unix: row.optional_integer(9)?,
        error_code: row.optional_text(10)?,
        error_params_json: row.optional_text(11)?,
    })
}

fn parse_account_row(row: sqlite::Row) -> AppResult<AccountRecord> {
    Ok(AccountRecord {
        id: row.text(0)?,
        username: row.text(1)?,
        account_kind: row.text(2)?,
        vault_ref: row.text(3)?,
        session_state: row.text(4)?,
        ownership_verified_at_unix: row.integer(5)?,
        last_online_auth_at_unix: row.integer(6)?,
        added_at_unix: row.integer(7)?,
        last_used_at_unix: row.integer(8)?,
    })
}

fn parse_profile_row(row: sqlite::Row) -> AppResult<ProfileRecord> {
    Ok(ProfileRecord {
        id: row.text(0)?,
        display_name: row.text(1)?,
        lifecycle_state: row.text(2)?,
        active_revision_id: row.optional_text(3)?,
        favorite: row.integer(4)? == 1,
        verification_state: row.text(5)?,
        trashed_from_state: row.optional_text(6)?,
        source_profile_id: row.optional_text(7)?,
        account_id: row.optional_text(8)?,
        created_at_unix: row.integer(9)?,
        updated_at_unix: row.integer(10)?,
    })
}

fn upsert_runtime_projection_row(
    connection: &Connection,
    record: &RuntimeQueryProjection,
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO profile_runtime_projection(\
            profile_id, revision_id, minecraft_version, loader_kind, loader_version,\
            component_id, component_version, install_state, updated_at_unix\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
         ON CONFLICT(profile_id) DO UPDATE SET \
            revision_id = excluded.revision_id,\
            minecraft_version = excluded.minecraft_version,\
            loader_kind = excluded.loader_kind,\
            loader_version = excluded.loader_version,\
            component_id = excluded.component_id,\
            component_version = excluded.component_version,\
            install_state = excluded.install_state,\
            updated_at_unix = excluded.updated_at_unix",
        &[
            Value::from(record.profile_id.as_str()),
            Value::from(record.revision_id.as_str()),
            Value::from(record.minecraft_version.as_str()),
            Value::from(record.loader_kind.as_str()),
            record
                .loader_version
                .as_deref()
                .map(Value::from)
                .unwrap_or(Value::Null),
            record
                .component_id
                .as_deref()
                .map(Value::from)
                .unwrap_or(Value::Null),
            record
                .component_version
                .as_deref()
                .map(Value::from)
                .unwrap_or(Value::Null),
            Value::from(record.install_state.as_str()),
            Value::Integer(record.updated_at_unix),
        ],
    )?;
    Ok(())
}

fn parse_runtime_projection_row(row: sqlite::Row) -> AppResult<RuntimeQueryProjection> {
    Ok(RuntimeQueryProjection {
        profile_id: row.text(0)?,
        revision_id: row.text(1)?,
        minecraft_version: row.text(2)?,
        loader_kind: row.text(3)?,
        loader_version: row.optional_text(4)?,
        component_id: row.optional_text(5)?,
        component_version: row.optional_text(6)?,
        install_state: row.text(7)?,
        updated_at_unix: row.integer(8)?,
    })
}

fn parse_cache_blob_row(row: sqlite::Row) -> AppResult<CacheBlobRecord> {
    let size = row.integer(1)?;
    let size_bytes = u64::try_from(size).map_err(|_| AppError::coded("cache_size_invalid"))?;
    Ok(CacheBlobRecord {
        sha256: row.text(0)?,
        size_bytes,
        relative_path: row.text(2)?,
        state: row.text(3)?,
        created_at_unix: row.integer(4)?,
        last_verified_at_unix: row.optional_integer(5)?,
        quarantine_relative_path: row.optional_text(6)?,
        quarantined_at_unix: row.optional_integer(7)?,
    })
}

fn validate_runtime_projection(record: &RuntimeQueryProjection) -> AppResult<()> {
    validate_runtime_text("profileId", &record.profile_id, 128)?;
    validate_runtime_text("revisionId", &record.revision_id, 128)?;
    validate_runtime_text("minecraftVersion", &record.minecraft_version, 64)?;

    match (
        record.loader_kind.as_str(),
        record.loader_version.as_deref(),
    ) {
        ("vanilla", None) => {}
        ("fabric" | "neoforge", Some(version)) => {
            validate_runtime_text("loaderVersion", version, 128)?;
        }
        ("vanilla", Some(_)) => {
            return Err(runtime_projection_invalid(
                "loaderVersion",
                "must_be_null_for_vanilla",
            ));
        }
        ("fabric" | "neoforge", None) => {
            return Err(runtime_projection_invalid(
                "loaderVersion",
                "required_for_modded_loader",
            ));
        }
        _ => {
            return Err(runtime_projection_invalid(
                "loaderKind",
                "unsupported_loader",
            ));
        }
    }

    match (
        record.component_id.as_deref(),
        record.component_version.as_deref(),
    ) {
        (None, None) => {}
        (Some(component_id), Some(component_version)) => {
            validate_runtime_text("componentId", component_id, 128)?;
            validate_runtime_text("componentVersion", component_version, 128)?;
        }
        _ => {
            return Err(runtime_projection_invalid(
                "component",
                "id_and_version_must_be_paired",
            ));
        }
    }

    if !matches!(
        record.install_state.as_str(),
        "configured" | "installed" | "repair-required"
    ) {
        return Err(runtime_projection_invalid(
            "installState",
            "unsupported_state",
        ));
    }
    if record.updated_at_unix < 0 {
        return Err(runtime_projection_invalid(
            "updatedAtUnix",
            "must_be_non_negative",
        ));
    }
    Ok(())
}

fn validate_runtime_text(field: &str, value: &str, max_chars: usize) -> AppResult<()> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        return Err(runtime_projection_invalid(field, "invalid_text"));
    }
    Ok(())
}

fn runtime_projection_invalid(field: &str, reason: &str) -> AppError {
    AppError::coded_with(
        "runtime_projection_invalid",
        [("field", field), ("reason", reason)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_path() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "s9lab-storage-test-{}-{}",
            std::process::id(),
            crate::operations::model::new_identifier("storage")
        ));
        fs::create_dir_all(&root).expect("root");
        root.join("launcher.db")
    }

    fn activate_test_revision(
        storage: &Storage,
        profile_id: &str,
        revision_id: &str,
    ) -> RevisionRecord {
        storage.create_profile(profile_id).expect("profile");
        let operation_id = format!("operation-{revision_id}");
        storage
            .insert_operation(&OperationRecord {
                id: operation_id.clone(),
                operation_type: OperationType::ProfileRevision,
                profile_id: Some(profile_id.to_string()),
                state: OperationState::Planned,
                planned_changes_json: "{}".into(),
                staging_relative_path: format!("{operation_id}/revision"),
                previous_revision_id: None,
                target_revision_id: Some(revision_id.to_string()),
                started_at_unix: 1,
                completed_at_unix: None,
                error_code: None,
                error_params_json: None,
            })
            .expect("operation");
        let revision = RevisionRecord {
            id: revision_id.to_string(),
            profile_id: profile_id.to_string(),
            operation_id,
            manifest_sha256: "a".repeat(64),
            lock_sha256: "b".repeat(64),
            manifest_relative_path: format!("{profile_id}/revisions/{revision_id}/manifest.json"),
            lock_relative_path: format!("{profile_id}/revisions/{revision_id}/lock.json"),
            status: "committed".into(),
            created_at_unix: 1,
        };
        storage
            .activate_revision(&revision, None)
            .expect("activate revision");
        revision
    }

    #[test]
    fn creates_and_reopens_current_database() {
        let path = test_path();
        let storage = Storage::initialize_for_test(&path).expect("initialize");
        assert_eq!(
            storage.schema_version().expect("version"),
            LATEST_SCHEMA_VERSION
        );
        drop(storage);
        let reopened = Storage::initialize_for_test(&path).expect("reopen");
        assert_eq!(
            reopened.schema_version().expect("version"),
            LATEST_SCHEMA_VERSION
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn migrates_forward_from_version_one() {
        let path = test_path();
        let connection = Connection::open(&path).expect("open");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL, applied_at_unix INTEGER NOT NULL);",
            )
            .expect("migration table");
        connection
            .transaction(|transaction| {
                transaction.execute_batch(migrations::MIGRATIONS[0].sql)?;
                transaction.execute(
                    "INSERT INTO schema_migrations(version, name, applied_at_unix) VALUES (1, 'phase1_core_schema', 0)",
                    &[],
                )?;
                Ok(())
            })
            .expect("v1");
        drop(connection);
        let storage = Storage::initialize_for_test(&path).expect("forward migration");
        assert_eq!(
            storage.schema_version().expect("version"),
            LATEST_SCHEMA_VERSION
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn schema_contains_no_secret_columns() {
        let path = test_path();
        let storage = Storage::initialize_for_test(&path).expect("initialize");
        let schema = storage.schema_sql().expect("schema").to_ascii_lowercase();
        for forbidden in ["token", "password", "secret", "credential"] {
            assert!(!schema.contains(forbidden), "schema contains {forbidden}");
        }
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn rejects_illegal_operation_state_transition() {
        let path = test_path();
        let storage = Storage::initialize_for_test(&path).expect("initialize");
        let profile = storage.create_profile("profile-state").expect("profile");
        let operation = OperationRecord {
            id: "operation-state".into(),
            operation_type: OperationType::SimulatedProfileInstall,
            profile_id: Some(profile.id),
            state: OperationState::Planned,
            planned_changes_json: "{}".into(),
            staging_relative_path: "operation-state/revision".into(),
            previous_revision_id: None,
            target_revision_id: None,
            started_at_unix: 0,
            completed_at_unix: None,
            error_code: None,
            error_params_json: None,
        };
        storage.insert_operation(&operation).expect("operation");
        assert!(storage
            .update_operation_state("operation-state", OperationState::Completed, None)
            .is_err());
        assert_eq!(
            storage
                .operation("operation-state")
                .expect("query")
                .expect("record")
                .state,
            OperationState::Planned
        );
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn migrates_forward_from_every_previous_schema_version() {
        for prior_version in 1..LATEST_SCHEMA_VERSION {
            let path = test_path();
            let connection = Connection::open(&path).expect("open");
            connection
                .execute_batch(
                    "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL, applied_at_unix INTEGER NOT NULL);",
                )
                .expect("migration table");
            for migration in migrations::MIGRATIONS
                .iter()
                .filter(|migration| migration.version <= prior_version)
            {
                connection
                    .transaction(|transaction| {
                        transaction.execute_batch(migration.sql)?;
                        transaction.execute(
                            "INSERT INTO schema_migrations(version, name, applied_at_unix) VALUES (?1, ?2, 0)",
                            &[
                                Value::Integer(migration.version),
                                Value::from(migration.name),
                            ],
                        )?;
                        Ok(())
                    })
                    .expect("apply prior migration");
            }
            drop(connection);
            let storage = Storage::initialize_for_test(&path).expect("forward migration");
            assert_eq!(
                storage.schema_version().expect("version"),
                LATEST_SCHEMA_VERSION
            );
            let _ = fs::remove_dir_all(path.parent().expect("parent"));
        }
    }

    #[test]
    fn phase5_migration_from_v5_does_not_invent_runtime_rows() {
        let path = test_path();
        let connection = Connection::open(&path).expect("open");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL, applied_at_unix INTEGER NOT NULL);",
            )
            .expect("migration table");
        for migration in migrations::MIGRATIONS
            .iter()
            .filter(|migration| migration.version <= 5)
        {
            connection
                .transaction(|transaction| {
                    transaction.execute_batch(migration.sql)?;
                    transaction.execute(
                        "INSERT INTO schema_migrations(version, name, applied_at_unix) VALUES (?1, ?2, 0)",
                        &[
                            Value::Integer(migration.version),
                            Value::from(migration.name),
                        ],
                    )?;
                    Ok(())
                })
                .expect("apply v5 migration");
        }
        connection
            .execute_batch(
                "INSERT INTO profiles(
                    id, lifecycle_state, active_revision_id, created_at_unix, updated_at_unix,
                    archived_at_unix, deleted_at_unix
                 ) VALUES ('legacy-runtime', 'active', NULL, 17, 17, NULL, NULL);
                 INSERT INTO profile_metadata(
                    profile_id, display_name, favorite, verification_state, trashed_from_state
                 ) VALUES ('legacy-runtime', 'Legacy runtime', 0, 'verified', NULL);
                 INSERT INTO profile_lineage(profile_id, source_profile_id, duplicated_at_unix)
                 VALUES ('legacy-runtime', NULL, 17);
                 INSERT INTO operations(
                    id, operation_type, profile_id, state, planned_changes_json,
                    staging_relative_path, previous_revision_id, target_revision_id,
                    started_at_unix, completed_at_unix, error_code, error_params_json
                 ) VALUES (
                    'legacy-operation', 'profile-revision', 'legacy-runtime', 'completed', '{}',
                    'legacy-operation/revision', NULL, 'legacy-revision', 17, 18, NULL, NULL
                 );
                 INSERT INTO profile_revisions(
                    id, profile_id, operation_id, manifest_sha256, lock_sha256,
                    manifest_relative_path, lock_relative_path, status, created_at_unix
                 ) VALUES (
                    'legacy-revision', 'legacy-runtime', 'legacy-operation',
                    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                    'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                    'legacy-runtime/revisions/legacy-revision/manifest.json',
                    'legacy-runtime/revisions/legacy-revision/lock.json', 'committed', 18
                 );
                 UPDATE profiles
                 SET active_revision_id = 'legacy-revision'
                 WHERE id = 'legacy-runtime';",
            )
            .expect("legacy profile and revision");
        drop(connection);

        let storage = Storage::initialize_for_test(&path).expect("phase5 migration");
        assert_eq!(
            storage.schema_version().expect("version"),
            LATEST_SCHEMA_VERSION
        );
        assert!(storage
            .runtime_projections()
            .expect("runtime projections")
            .is_empty());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn runtime_projection_crud_and_validation_are_atomic() {
        let path = test_path();
        let storage = Storage::initialize_for_test(&path).expect("initialize");
        let first_revision =
            activate_test_revision(&storage, "runtime-profile", "runtime-revision");
        let other_revision = activate_test_revision(&storage, "other-profile", "other-revision");
        let projection = RuntimeQueryProjection {
            profile_id: first_revision.profile_id.clone(),
            revision_id: first_revision.id.clone(),
            minecraft_version: "1.21.1".into(),
            loader_kind: "fabric".into(),
            loader_version: Some("0.16.10".into()),
            component_id: Some("s9lab-client".into()),
            component_version: Some("1.0.8".into()),
            install_state: "configured".into(),
            updated_at_unix: 23,
        };

        storage
            .upsert_runtime_projection(&projection)
            .expect("insert projection");
        assert_eq!(
            storage
                .runtime_projection("runtime-profile")
                .expect("query")
                .expect("projection"),
            projection
        );
        assert_eq!(
            storage.runtime_projections().expect("list"),
            vec![projection.clone()]
        );

        let mut mismatched = projection.clone();
        mismatched.revision_id = other_revision.id;
        mismatched.minecraft_version = "1.21.2".into();
        assert!(storage.upsert_runtime_projection(&mismatched).is_err());
        assert_eq!(
            storage
                .runtime_projection("runtime-profile")
                .expect("query after rejected update")
                .expect("unchanged projection"),
            projection
        );

        let mut invalid = projection.clone();
        invalid.loader_kind = "vanilla".into();
        assert!(storage.upsert_runtime_projection(&invalid).is_err());
        invalid.loader_version = None;
        invalid.component_version = None;
        assert!(storage.upsert_runtime_projection(&invalid).is_err());
        invalid.component_id = None;
        invalid.install_state = "unknown".into();
        assert!(storage.upsert_runtime_projection(&invalid).is_err());

        assert!(storage
            .delete_runtime_projection("runtime-profile")
            .expect("delete"));
        assert!(!storage
            .delete_runtime_projection("runtime-profile")
            .expect("idempotent delete"));
        assert!(storage
            .runtime_projection("runtime-profile")
            .expect("query deleted")
            .is_none());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn runtime_projection_enforces_committed_revision_ownership_and_invalidation() {
        let path = test_path();
        let storage = Storage::initialize_for_test(&path).expect("initialize");
        let revision = activate_test_revision(&storage, "owner-profile", "owner-revision");
        activate_test_revision(&storage, "foreign-profile", "foreign-revision");
        let connection = storage.open().expect("connection");

        let cross_profile = connection.execute(
            "INSERT INTO profile_runtime_projection(
                profile_id, revision_id, minecraft_version, loader_kind, loader_version,
                component_id, component_version, install_state, updated_at_unix
             ) VALUES (?1, ?2, '1.21.1', 'vanilla', NULL, NULL, NULL, 'configured', 1)",
            &[
                Value::from("owner-profile"),
                Value::from("foreign-revision"),
            ],
        );
        assert!(cross_profile.is_err());

        storage
            .upsert_runtime_projection(&RuntimeQueryProjection {
                profile_id: revision.profile_id.clone(),
                revision_id: revision.id.clone(),
                minecraft_version: "1.21.1".into(),
                loader_kind: "vanilla".into(),
                loader_version: None,
                component_id: None,
                component_version: None,
                install_state: "installed".into(),
                updated_at_unix: 2,
            })
            .expect("valid projection");
        storage
            .restore_active_revision(&revision.profile_id, &revision.id, None)
            .expect("invalidate active revision");
        assert!(storage
            .runtime_projection(&revision.profile_id)
            .expect("query invalidated projection")
            .is_none());

        let invalidated = connection.execute(
            "INSERT INTO profile_runtime_projection(
                profile_id, revision_id, minecraft_version, loader_kind, loader_version,
                component_id, component_version, install_state, updated_at_unix
             ) VALUES (?1, ?2, '1.21.1', 'vanilla', NULL, NULL, NULL, 'configured', 3)",
            &[
                Value::from(revision.profile_id.as_str()),
                Value::from(revision.id.as_str()),
            ],
        );
        assert!(invalidated.is_err());
        drop(connection);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn phase4_migration_backfills_existing_profile_metadata_and_lineage() {
        let path = test_path();
        let connection = Connection::open(&path).expect("open");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY NOT NULL, name TEXT NOT NULL, applied_at_unix INTEGER NOT NULL);",
            )
            .expect("migration table");
        for migration in migrations::MIGRATIONS
            .iter()
            .filter(|migration| migration.version <= 4)
        {
            connection
                .transaction(|transaction| {
                    transaction.execute_batch(migration.sql)?;
                    transaction.execute(
                        "INSERT INTO schema_migrations(version, name, applied_at_unix) VALUES (?1, ?2, 0)",
                        &[
                            Value::Integer(migration.version),
                            Value::from(migration.name),
                        ],
                    )?;
                    Ok(())
                })
                .expect("apply prior migration");
        }
        connection
            .execute(
                "INSERT INTO profiles(\
                    id, lifecycle_state, active_revision_id, created_at_unix, updated_at_unix,\
                    archived_at_unix, deleted_at_unix\
                 ) VALUES ('legacy-profile', 'active', NULL, 17, 17, NULL, NULL)",
                &[],
            )
            .expect("legacy profile");
        drop(connection);

        let storage = Storage::initialize_for_test(&path).expect("phase4 migration");
        let profile = storage
            .profile("legacy-profile")
            .expect("profile query")
            .expect("profile");
        assert_eq!(profile.display_name, "Profile legacy-p");
        assert_eq!(profile.created_at_unix, 17);
        assert!(profile.source_profile_id.is_none());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }
}
