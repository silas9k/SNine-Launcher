use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectType {
    Mod,
    Modpack,
    Resourcepack,
    Shader,
}

impl ProjectType {
    pub(crate) fn as_api_value(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Modpack => "modpack",
            Self::Resourcepack => "resourcepack",
            Self::Shader => "shader",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModrinthLoader {
    Vanilla,
    Fabric,
    Neoforge,
}

impl ModrinthLoader {
    pub(crate) fn as_api_value(self) -> &'static str {
        match self {
            Self::Vanilla => "minecraft",
            Self::Fabric => "fabric",
            Self::Neoforge => "neoforge",
        }
    }

    pub(crate) fn from_api_value(value: &str) -> Option<Self> {
        match value {
            "minecraft" => Some(Self::Vanilla),
            "fabric" => Some(Self::Fabric),
            "neoforge" => Some(Self::Neoforge),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchIndex {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl SearchIndex {
    pub(crate) fn as_api_value(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModrinthSearchRequest {
    pub query: String,
    pub project_type: ProjectType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<ModrinthLoader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_version: Option<String>,
    #[serde(default)]
    pub index: SearchIndex,
    pub offset: u32,
    pub limit: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader: Option<ModrinthLoader>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub featured: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub offset: u32,
    pub limit: u8,
    pub total_hits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub project_type: ProjectType,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub minecraft_versions: Vec<String>,
    pub loaders: Vec<ModrinthLoader>,
    pub categories: Vec<String>,
    pub license: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectSupport {
    Required,
    Optional,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Approved,
    Archived,
    Rejected,
    Draft,
    Unlisted,
    Processing,
    Withheld,
    Scheduled,
    Private,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLicense {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryImage {
    pub url: String,
    pub featured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ordering: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub project_type: ProjectType,
    pub status: ProjectStatus,
    pub client_side: ProjectSupport,
    pub server_side: ProjectSupport,
    pub downloads: u64,
    pub followers: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub license: ProjectLicense,
    pub minecraft_versions: Vec<String>,
    pub loaders: Vec<ModrinthLoader>,
    pub categories: Vec<String>,
    pub version_ids: Vec<String>,
    pub gallery: Vec<GalleryImage>,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionType {
    Release,
    Beta,
    Alpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Listed,
    Archived,
    Draft,
    Unlisted,
    Scheduled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthDependency {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectFileType {
    RequiredResourcePack,
    OptionalResourcePack,
    SourcesJar,
    DevJar,
    JavadocJar,
    Unknown,
    Signature,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthFile {
    pub file_name: String,
    pub size_bytes: u64,
    pub primary: bool,
    pub upstream_sha512: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_sha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<ProjectFileType>,
    #[serde(skip)]
    pub(crate) download_url: Url,
}

impl ModrinthFile {
    pub(crate) fn new(
        file_name: String,
        size_bytes: u64,
        primary: bool,
        upstream_sha512: String,
        upstream_sha1: Option<String>,
        file_type: Option<ProjectFileType>,
        download_url: Url,
    ) -> Self {
        Self {
            file_name,
            size_bytes,
            primary,
            upstream_sha512,
            upstream_sha1,
            file_type,
            download_url,
        }
    }

    /// This URL originates in a validated Modrinth response and is restricted to the
    /// compiled-in CDN authority. It is deliberately omitted from serialization/IPC.
    pub fn validated_download_url(&self) -> &Url {
        &self.download_url
    }
}

impl fmt::Debug for ModrinthFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModrinthFile")
            .field("file_name", &self.file_name)
            .field("size_bytes", &self.size_bytes)
            .field("primary", &self.primary)
            .field("upstream_sha512", &self.upstream_sha512)
            .field("upstream_sha1", &self.upstream_sha1)
            .field("file_type", &self.file_type)
            .field("download_url", &"<validated-modrinth-cdn-url>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectVersion {
    pub version_id: String,
    pub project_id: String,
    pub author_id: String,
    pub name: String,
    pub version_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<ModrinthLoader>,
    pub version_type: VersionType,
    pub featured: bool,
    pub status: VersionStatus,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub downloads: u64,
    pub dependencies: Vec<ModrinthDependency>,
    pub files: Vec<ModrinthFile>,
}

impl ProjectVersion {
    pub fn primary_file(&self) -> Option<&ModrinthFile> {
        self.files.iter().find(|file| file.primary)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedFileDigest {
    pub size_bytes: u64,
    /// Launcher-owned digest computed after the upstream SHA-512 (and optional SHA-1)
    /// verification. This is the digest persisted in S9Lab locks and caches.
    pub sha256: String,
}
