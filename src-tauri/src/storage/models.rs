use crate::operations::model::{OperationState, OperationType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRecord {
    pub id: String,
    pub username: String,
    pub account_kind: String,
    pub vault_ref: String,
    pub session_state: String,
    pub ownership_verified_at_unix: i64,
    pub last_online_auth_at_unix: i64,
    pub added_at_unix: i64,
    pub last_used_at_unix: i64,
}

#[derive(Debug, Clone)]
pub struct ProfileRecord {
    pub id: String,
    pub display_name: String,
    pub lifecycle_state: String,
    pub active_revision_id: Option<String>,
    pub favorite: bool,
    pub verification_state: String,
    pub trashed_from_state: Option<String>,
    pub source_profile_id: Option<String>,
    pub account_id: Option<String>,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeQueryProjection {
    pub profile_id: String,
    pub revision_id: String,
    pub minecraft_version: String,
    pub loader_kind: String,
    pub loader_version: Option<String>,
    pub component_id: Option<String>,
    pub component_version: Option<String>,
    pub install_state: String,
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheBlobRecord {
    pub sha256: String,
    pub size_bytes: u64,
    pub relative_path: String,
    pub state: String,
    pub created_at_unix: i64,
    pub last_verified_at_unix: Option<i64>,
    pub quarantine_relative_path: Option<String>,
    pub quarantined_at_unix: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct RevisionRecord {
    pub id: String,
    pub profile_id: String,
    pub operation_id: String,
    pub manifest_sha256: String,
    pub lock_sha256: String,
    pub manifest_relative_path: String,
    pub lock_relative_path: String,
    pub status: String,
    pub created_at_unix: i64,
}

#[derive(Debug, Clone)]
pub struct OperationRecord {
    pub id: String,
    pub operation_type: OperationType,
    pub profile_id: Option<String>,
    pub state: OperationState,
    pub planned_changes_json: String,
    pub staging_relative_path: String,
    pub previous_revision_id: Option<String>,
    pub target_revision_id: Option<String>,
    pub started_at_unix: i64,
    pub completed_at_unix: Option<i64>,
    pub error_code: Option<String>,
    pub error_params_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JournalRecord {
    pub sequence: i64,
    pub step: String,
    pub status: String,
    pub details_json: String,
    pub compensation_json: String,
}
