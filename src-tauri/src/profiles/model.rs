use serde::{Deserialize, Serialize};

use crate::{
    content::{ContentSelection, ResolvedContentLockV1},
    runtime::{ProfileRuntimeIntent, ResolvedRuntimeLockV1},
};

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileManifestV2 {
    pub format: String,
    pub format_version: u32,
    pub profile_id: String,
    pub created_at_unix: i64,
    pub runtime: ProfileRuntimeIntent,
    pub s9lab_component: S9labComponentSelection,
    #[serde(default)]
    pub desired_content: Vec<ContentSelection>,
    pub mutable_directories: Vec<String>,
    pub isolation_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "mode",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum S9labComponentSelection {
    Disabled,
    Catalog {
        component_id: String,
        component_version: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileLockV2 {
    pub format: String,
    pub format_version: u32,
    pub profile_id: String,
    pub revision_id: String,
    pub manifest_sha256: String,
    pub runtime: ResolvedRuntimeLockV1,
    pub launch: ResolvedLaunchConfiguration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<ResolvedContentLockV1>,
    #[serde(default)]
    pub cache_blobs: Vec<LockedCacheBlob>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedLaunchConfiguration {
    pub main_class: String,
    #[serde(default = "default_version_type")]
    pub version_type: String,
    pub asset_index_id: String,
    pub java_major_version: u16,
    pub game_arguments: Vec<ResolvedLaunchArgument>,
    pub jvm_arguments: Vec<ResolvedLaunchArgument>,
    pub classpath_targets: Vec<String>,
    pub native_jar_targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_game_arguments: Option<String>,
}

fn default_version_type() -> String { "release".into() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ResolvedLaunchArgument {
    Plain {
        value: String,
    },
    Conditional {
        rules: Vec<ResolvedLaunchRule>,
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedLaunchRule {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_arch: Option<String>,
    #[serde(default)]
    pub has_os_version_constraint: bool,
    #[serde(default)]
    pub features: std::collections::BTreeMap<String, bool>,
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
