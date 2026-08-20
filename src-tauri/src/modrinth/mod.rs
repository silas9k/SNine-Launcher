mod digest;
mod model;
mod provider;
mod validation;

pub use digest::{verify_file_bytes, DownloadVerifier};
pub use model::{
    DependencyType, GalleryImage, ModrinthDependency, ModrinthFile, ModrinthLoader,
    ModrinthSearchRequest, ProjectDetail, ProjectFileType, ProjectLicense, ProjectStatus,
    ProjectSupport, ProjectType, ProjectVersion, SearchHit, SearchIndex, SearchPage,
    VerifiedFileDigest, VersionQuery, VersionStatus, VersionType,
};
pub use provider::ModrinthProvider;
pub use validation::{
    validate_modrinth_id, validate_search_request, validate_version_query,
    MAX_MODRINTH_FILE_SIZE_BYTES,
};

pub const MODRINTH_API_ORIGIN: &str = "https://api.modrinth.com";
pub const MODRINTH_CDN_ORIGIN: &str = "https://cdn.modrinth.com";
