use serde::{Deserialize, Serialize};

use crate::app::config::ShellSettings;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateMode {
    Manual,
    Automatic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdatePolicyV1 {
    pub format_version: u32,
    pub launcher: UpdateMode,
    pub profiles: UpdateMode,
    pub s9lab_component: UpdateMode,
    pub content: UpdateMode,
}

impl Default for UpdatePolicyV1 {
    fn default() -> Self {
        Self {
            format_version: 1,
            launcher: UpdateMode::Manual,
            profiles: UpdateMode::Manual,
            s9lab_component: UpdateMode::Manual,
            content: UpdateMode::Manual,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelStatus {
    pub channel: String,
    pub mode: UpdateMode,
    pub state: String,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RestorePointSummary {
    pub backup_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub source_revision_id: String,
    pub created_at_unix: i64,
    pub file_count: u32,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRevisionSummary {
    pub revision_id: String,
    pub created_at_unix: i64,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileSummary {
    pub profile_id: String,
    pub display_name: String,
    pub active_revision_id: String,
    pub revisions: Vec<ProfileRevisionSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCenterSnapshot {
    pub policy: UpdatePolicyV1,
    pub channels: Vec<UpdateChannelStatus>,
    pub profiles: Vec<UpdateProfileSummary>,
    pub restore_points: Vec<RestorePointSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChangePreview {
    pub channel: String,
    pub item_id: String,
    pub display_name: String,
    pub current_version: String,
    pub target_version: String,
    pub verification: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpdatePreview {
    pub profile_id: String,
    pub base_revision_id: String,
    pub changes: Vec<UpdateChangePreview>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOperationResult {
    pub operation_id: String,
    pub profile_id: String,
    pub revision_id: String,
    pub restore_point_id: String,
    pub applied_changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyProfileUpdatesInput {
    pub profile_id: String,
    pub content_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackProfileInput {
    pub profile_id: String,
    pub revision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupFileV1 {
    pub relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorePointV1 {
    pub format: String,
    pub format_version: u32,
    pub backup_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub source_revision_id: String,
    pub created_at_unix: i64,
    pub shell_settings: ShellSettings,
    pub files: Vec<BackupFileV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreBackupInput {
    pub backup_id: String,
    pub display_name: String,
    pub include_account: bool,
    pub include_settings: bool,
    pub include_files: bool,
}
