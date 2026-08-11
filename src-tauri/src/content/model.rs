use crate::runtime::{LoaderKind, LoaderSelection};
use serde::{Deserialize, Serialize};

pub const CONTENT_LOCK_FORMAT: &str = "s9lab-content-lock";
pub const CONTENT_LOCK_FORMAT_VERSION: u32 = 1;
pub const CONTENT_RELEASE_FORMAT: &str = "s9lab-content-release";
pub const CONTENT_RELEASE_FORMAT_VERSION: u32 = 1;

pub(crate) const fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    Mod,
    Modpack,
    ShaderPack,
    ResourcePack,
}

impl ContentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::ShaderPack => "shaderPack",
            Self::ResourcePack => "resourcePack",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ContentVersionRequirement {
    #[default]
    Any,
    Exact {
        version: String,
    },
    OneOf {
        versions: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentSelection {
    pub content_id: String,
    #[serde(default)]
    pub version: ContentVersionRequirement,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentTargetRuntime {
    pub minecraft_version: String,
    pub loader: LoaderSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentResolutionRequest {
    pub runtime: ContentTargetRuntime,
    #[serde(default)]
    pub requested: Vec<ContentSelection>,
    /// When enabled, every declared optional dependency becomes a strict
    /// resolution edge. Missing or incompatible optional releases therefore
    /// fail closed instead of silently changing the selected graph.
    #[serde(default)]
    pub include_optional_dependencies: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentLoaderCompatibility {
    pub kind: LoaderKind,
    /// Empty means that every version of the selected loader is accepted.
    #[serde(default)]
    pub loader_versions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentCompatibility {
    pub minecraft_versions: Vec<String>,
    /// Empty is only valid for loader-independent shader/resource packs.
    #[serde(default)]
    pub loaders: Vec<ContentLoaderCompatibility>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentDependencyKind {
    Required,
    Optional,
    Incompatible,
}

impl ContentDependencyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Incompatible => "incompatible",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentDependency {
    pub content_id: String,
    pub kind: ContentDependencyKind,
    #[serde(default)]
    pub version: ContentVersionRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "provider",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ContentSourceV1 {
    Modrinth {
        project_id: String,
        version_id: String,
        file_name: String,
    },
    Local {
        file_name: String,
    },
}

impl ContentSourceV1 {
    pub fn provider_name(&self) -> &'static str {
        match self {
            Self::Modrinth { .. } => "modrinth",
            Self::Local { .. } => "local",
        }
    }

    pub fn file_name(&self) -> &str {
        match self {
            Self::Modrinth { file_name, .. } | Self::Local { file_name } => file_name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentArtifactV1 {
    pub relative_target: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentReleaseV1 {
    pub format: String,
    pub format_version: u32,
    pub content_id: String,
    pub version: String,
    pub kind: ContentKind,
    pub compatibility: ContentCompatibility,
    #[serde(default)]
    pub dependencies: Vec<ContentDependency>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ContentSourceV1>,
    pub artifact: ContentArtifactV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedDependencyV1 {
    pub content_id: String,
    pub kind: ContentDependencyKind,
    pub version_requirement: ContentVersionRequirement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedContentItemV1 {
    pub content_id: String,
    pub version: String,
    pub kind: ContentKind,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ContentSourceV1>,
    pub relative_target: String,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub dependencies: Vec<ResolvedDependencyV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedContentOverrideV1 {
    pub pack_content_id: String,
    pub relative_target: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedContentPackMemberV1 {
    pub pack_content_id: String,
    pub content_id: String,
    pub version: String,
    #[serde(default = "default_enabled")]
    pub enabled_by_default: bool,
    #[serde(default)]
    pub owns_selection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedContentLockV1 {
    pub format: String,
    pub format_version: u32,
    pub runtime: ContentTargetRuntime,
    pub include_optional_dependencies: bool,
    pub requested: Vec<ContentSelection>,
    pub items: Vec<ResolvedContentItemV1>,
    #[serde(default)]
    pub pack_members: Vec<ResolvedContentPackMemberV1>,
    #[serde(default)]
    pub overrides: Vec<ResolvedContentOverrideV1>,
    pub resolution_sha256: String,
}
