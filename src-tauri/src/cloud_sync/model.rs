use crate::app::config::ShellSettings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SYNC_PAYLOAD_FORMAT: &str = "site.s9lab.cloud-payload";
pub const SYNC_PAYLOAD_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncProfileMetadataV1 {
    pub profile_id: String,
    pub display_name: String,
    pub lifecycle_state: String,
    pub favorite: bool,
    pub active_revision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncContentEntryV1 {
    pub profile_id: String,
    pub content_id: String,
    pub content_type: String,
    pub version: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncPayloadV1 {
    pub format: String,
    pub format_version: u32,
    pub profiles: Vec<SyncProfileMetadataV1>,
    pub content: Vec<SyncContentEntryV1>,
    pub settings: ShellSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRevisionV1 {
    pub revision_id: String,
    pub parent_revision_id: Option<String>,
    pub device_id: String,
    pub created_at_unix: i64,
    pub payload_sha256: String,
    pub payload: SyncPayloadV1,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub field: String,
    pub base_value: Option<String>,
    pub local_value: Option<String>,
    pub remote_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreeWayMerge {
    pub merged_fields: BTreeMap<String, String>,
    pub conflicts: Vec<SyncConflict>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalSyncRevisionSummary {
    pub revision_id: String,
    pub payload_sha256: String,
    pub profile_count: u32,
    pub content_count: u32,
    pub settings_included: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudSyncSnapshot {
    pub provider_state: String,
    pub reason_code: String,
    pub microsoft_base_account: Option<String>,
    pub linked_s9lab_account: Option<String>,
    pub session_state: String,
    pub online: bool,
    pub device_limit: u8,
    pub enrolled_devices: u8,
    pub scopes: Vec<String>,
    pub local_revision: LocalSyncRevisionSummary,
    pub pending_conflicts: u32,
}
