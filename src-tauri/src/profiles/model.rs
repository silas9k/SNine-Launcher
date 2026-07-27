use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileManifestV1 {
    pub format: String,
    pub format_version: u32,
    pub profile_id: String,
    pub display_name: String,
    pub created_at_unix: i64,
    pub source_profile_id: Option<String>,
    pub account_id: Option<String>,
    pub mutable_directories: Vec<String>,
    pub isolation_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLockV1 {
    pub format: String,
    pub format_version: u32,
    pub profile_id: String,
    pub revision_id: String,
    pub manifest_sha256: String,
    pub resolution_state: String,
    #[serde(default)]
    pub cache_blobs: Vec<LockedCacheBlob>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct LockedCacheBlob {
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub display_name: String,
    pub lifecycle_state: String,
    pub active_revision_id: String,
    pub account_id: Option<String>,
    pub favorite: bool,
    pub verification_state: String,
    pub source_profile_id: Option<String>,
    pub created_at_unix: i64,
    pub updated_at_unix: i64,
}
