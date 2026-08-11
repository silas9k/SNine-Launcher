use super::{
    model::{
        DependencyType, GalleryImage, ModrinthDependency, ModrinthFile, ModrinthLoader,
        ModrinthSearchRequest, ProjectDetail, ProjectFileType, ProjectLicense, ProjectStatus,
        ProjectSupport, ProjectType, ProjectVersion, SearchHit, SearchPage, VersionQuery,
        VersionStatus, VersionType,
    },
    validation,
};
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use reqwest::{header::CONTENT_TYPE, Client, Url};
use serde::{de::DeserializeOwned, Deserialize};
use std::{collections::BTreeSet, time::Duration};

const API_BASE: &str = "https://api.modrinth.com/v2";
const NETWORK_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct ModrinthProvider {
    client: Client,
}

impl ModrinthProvider {
    pub fn production() -> AppResult<Self> {
        let client = Client::builder()
            .timeout(NETWORK_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!(
                "S9Lab/S9Lab-Launcher/",
                env!("CARGO_PKG_VERSION"),
                " (Modrinth integration)"
            ))
            .build()?;
        Ok(Self { client })
    }

    pub async fn search_projects(&self, request: &ModrinthSearchRequest) -> AppResult<SearchPage> {
        validation::validate_search_request(request)?;
        let url = build_search_url(request)?;
        let bytes = self
            .get_json_bytes(url, validation::MAX_SEARCH_RESPONSE_BYTES)
            .await?;
        parse_search_response(&bytes, request)
    }

    pub async fn project_detail(&self, project_id: &str) -> AppResult<ProjectDetail> {
        validation::validate_modrinth_id(project_id)?;
        let url = build_project_detail_url(project_id)?;
        let bytes = self
            .get_json_bytes(url, validation::MAX_PROJECT_RESPONSE_BYTES)
            .await?;
        parse_project_detail(&bytes, project_id)
    }

    pub async fn project_versions(
        &self,
        project_id: &str,
        query: &VersionQuery,
    ) -> AppResult<Vec<ProjectVersion>> {
        validation::validate_modrinth_id(project_id)?;
        validation::validate_version_query(query)?;
        let url = build_project_versions_url(project_id, query)?;
        let bytes = self
            .get_json_bytes(url, validation::MAX_VERSIONS_RESPONSE_BYTES)
            .await?;
        parse_project_versions(&bytes, project_id, query)
    }

    pub async fn version_detail(&self, version_id: &str) -> AppResult<ProjectVersion> {
        validation::validate_modrinth_id(version_id)?;
        let url = build_version_detail_url(version_id)?;
        let bytes = self
            .get_json_bytes(url, validation::MAX_PROJECT_RESPONSE_BYTES)
            .await?;
        let wire: VersionWire = parse_json(&bytes, validation::MAX_PROJECT_RESPONSE_BYTES)?;
        if wire.id != version_id {
            return Err(AppError::coded("modrinth_version_identity_mismatch"));
        }
        convert_version(wire, None, None)
    }

    async fn get_json_bytes(&self, url: Url, maximum_size: u64) -> AppResult<Vec<u8>> {
        validation::validate_api_url(&url)?;
        let response = self.client.get(url).send().await?;
        if response.status().is_redirection() {
            return Err(AppError::coded("modrinth_redirect_forbidden"));
        }
        if !response.status().is_success() {
            return Err(AppError::coded_with(
                "modrinth_http_status",
                [("status", response.status().as_u16().to_string())],
            ));
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim);
        if content_type != Some("application/json") {
            return Err(AppError::coded("modrinth_content_type_invalid"));
        }
        if response
            .content_length()
            .is_some_and(|length| length == 0 || length > maximum_size)
        {
            return Err(AppError::coded("modrinth_response_size_invalid"));
        }

        let mut bytes = Vec::new();
        let mut received = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            received = received
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| AppError::coded("modrinth_response_size_overflow"))?;
            if received > maximum_size {
                return Err(AppError::coded("modrinth_response_size_invalid"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Err(AppError::coded("modrinth_response_empty"));
        }
        Ok(bytes)
    }
}

fn build_search_url(request: &ModrinthSearchRequest) -> AppResult<Url> {
    validation::validate_search_request(request)?;
    let mut facets = vec![vec![format!(
        "project_type:{}",
        request.project_type.as_api_value()
    )]];
    if let Some(loader) = request.loader {
        facets.push(vec![format!("categories:{}", loader.as_api_value())]);
    }
    if let Some(version) = request.minecraft_version.as_deref() {
        facets.push(vec![format!("versions:{version}")]);
    }
    let facets = serde_json::to_string(&facets)
        .map_err(|_| AppError::coded("modrinth_search_facets_invalid"))?;
    let mut url = endpoint("search")?;
    url.query_pairs_mut()
        .append_pair("query", &request.query)
        .append_pair("facets", &facets)
        .append_pair("index", request.index.as_api_value())
        .append_pair("offset", &request.offset.to_string())
        .append_pair("limit", &request.limit.to_string());
    validation::validate_api_url(&url)?;
    Ok(url)
}

fn build_project_detail_url(project_id: &str) -> AppResult<Url> {
    validation::validate_modrinth_id(project_id)?;
    endpoint(&format!("project/{project_id}"))
}

fn build_project_versions_url(project_id: &str, query: &VersionQuery) -> AppResult<Url> {
    validation::validate_modrinth_id(project_id)?;
    validation::validate_version_query(query)?;
    let mut url = endpoint(&format!("project/{project_id}/version"))?;
    {
        let mut pairs = url.query_pairs_mut();
        if let Some(loader) = query.loader {
            let loaders = serde_json::to_string(&[loader.as_api_value()])
                .map_err(|_| AppError::coded("modrinth_version_filter_invalid"))?;
            pairs.append_pair("loaders", &loaders);
        }
        if let Some(version) = query.minecraft_version.as_deref() {
            let versions = serde_json::to_string(&[version])
                .map_err(|_| AppError::coded("modrinth_version_filter_invalid"))?;
            pairs.append_pair("game_versions", &versions);
        }
        if let Some(featured) = query.featured {
            pairs.append_pair("featured", if featured { "true" } else { "false" });
        }
        pairs.append_pair("include_changelog", "false");
    }
    validation::validate_api_url(&url)?;
    Ok(url)
}

fn build_version_detail_url(version_id: &str) -> AppResult<Url> {
    validation::validate_modrinth_id(version_id)?;
    endpoint(&format!("version/{version_id}"))
}

fn endpoint(relative: &str) -> AppResult<Url> {
    let url = Url::parse(&format!("{API_BASE}/{relative}"))
        .map_err(|_| AppError::coded("modrinth_api_url_invalid"))?;
    validation::validate_api_url(&url)?;
    Ok(url)
}

#[derive(Deserialize)]
struct SearchResponseWire {
    hits: Vec<SearchHitWire>,
    offset: u32,
    limit: u8,
    total_hits: u64,
}

#[derive(Deserialize)]
struct SearchHitWire {
    slug: String,
    title: String,
    description: String,
    categories: Vec<String>,
    project_type: ProjectType,
    downloads: u64,
    icon_url: Option<String>,
    project_id: String,
    author: String,
    versions: Vec<String>,
    follows: u64,
    date_created: DateTime<Utc>,
    date_modified: DateTime<Utc>,
    license: String,
}

#[derive(Deserialize)]
struct ProjectDetailWire {
    slug: String,
    title: String,
    description: String,
    categories: Vec<String>,
    client_side: ProjectSupport,
    server_side: ProjectSupport,
    body: String,
    status: ProjectStatus,
    additional_categories: Vec<String>,
    project_type: ProjectType,
    downloads: u64,
    icon_url: Option<String>,
    id: String,
    published: DateTime<Utc>,
    updated: DateTime<Utc>,
    followers: u64,
    license: LicenseWire,
    versions: Vec<String>,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    gallery: Vec<GalleryWire>,
}

#[derive(Deserialize)]
struct LicenseWire {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct GalleryWire {
    url: String,
    featured: bool,
    title: Option<String>,
    description: Option<String>,
    created: DateTime<Utc>,
    ordering: i64,
}

#[derive(Deserialize)]
struct VersionWire {
    name: String,
    version_number: String,
    changelog: Option<String>,
    dependencies: Vec<DependencyWire>,
    game_versions: Vec<String>,
    version_type: VersionType,
    loaders: Vec<String>,
    featured: bool,
    status: VersionStatus,
    id: String,
    project_id: String,
    author_id: String,
    date_published: DateTime<Utc>,
    downloads: u64,
    files: Vec<FileWire>,
}

#[derive(Deserialize)]
struct DependencyWire {
    version_id: Option<String>,
    project_id: Option<String>,
    file_name: Option<String>,
    dependency_type: DependencyType,
}

#[derive(Deserialize)]
struct FileWire {
    hashes: HashesWire,
    url: String,
    filename: String,
    primary: bool,
    size: u64,
    file_type: Option<ProjectFileType>,
}

#[derive(Deserialize)]
struct HashesWire {
    sha512: Option<String>,
    sha1: Option<String>,
}

fn parse_search_response(bytes: &[u8], request: &ModrinthSearchRequest) -> AppResult<SearchPage> {
    validation::validate_search_request(request)?;
    let wire: SearchResponseWire = parse_json(bytes, validation::MAX_SEARCH_RESPONSE_BYTES)?;
    if wire.offset != request.offset
        || wire.limit != request.limit
        || wire.hits.len() > usize::from(wire.limit)
        || wire.hits.len() > validation::MAX_SEARCH_HITS
        || wire.total_hits < wire.hits.len() as u64
    {
        return Err(AppError::coded("modrinth_search_page_invalid"));
    }

    let mut project_ids = BTreeSet::new();
    let hits = wire
        .hits
        .into_iter()
        .map(|hit| {
            let converted = convert_search_hit(hit, request)?;
            if !project_ids.insert(converted.project_id.clone()) {
                return Err(AppError::coded("modrinth_search_project_duplicate"));
            }
            Ok(converted)
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(SearchPage {
        hits,
        offset: wire.offset,
        limit: wire.limit,
        total_hits: wire.total_hits,
    })
}

fn convert_search_hit(
    wire: SearchHitWire,
    request: &ModrinthSearchRequest,
) -> AppResult<SearchHit> {
    validation::validate_modrinth_id(&wire.project_id)?;
    validation::validate_slug(&wire.slug)?;
    validation::validate_text(&wire.title, false)?;
    validation::validate_text(&wire.description, true)?;
    validation::validate_text(&wire.author, false)?;
    validation::validate_token(&wire.license)?;
    if wire.project_type != request.project_type {
        return Err(AppError::coded("modrinth_search_project_type_mismatch"));
    }
    let categories = validate_string_set(
        wire.categories,
        128,
        validation::validate_category,
        "modrinth_categories_invalid",
    )?;
    let minecraft_versions = validate_string_set(
        wire.versions,
        512,
        validation::validate_minecraft_version,
        "modrinth_game_versions_invalid",
    )?;
    if request.minecraft_version.as_ref().is_some_and(|version| {
        !minecraft_versions
            .iter()
            .any(|candidate| candidate == version)
    }) {
        return Err(AppError::coded("modrinth_search_version_mismatch"));
    }
    let loaders = supported_loaders(&categories);
    if request
        .loader
        .is_some_and(|loader| !loaders.contains(&loader))
    {
        return Err(AppError::coded("modrinth_search_loader_mismatch"));
    }
    let icon_url = wire
        .icon_url
        .map(|value| {
            validation::validate_cdn_url(&value, Some(&wire.project_id)).map(|url| url.to_string())
        })
        .transpose()?;
    Ok(SearchHit {
        project_id: wire.project_id,
        slug: wire.slug,
        title: wire.title,
        description: wire.description,
        project_type: wire.project_type,
        author: wire.author,
        downloads: wire.downloads,
        follows: wire.follows,
        icon_url,
        minecraft_versions,
        loaders,
        categories,
        license: wire.license,
        created_at: wire.date_created,
        updated_at: wire.date_modified,
    })
}

fn parse_project_detail(bytes: &[u8], expected_project_id: &str) -> AppResult<ProjectDetail> {
    validation::validate_modrinth_id(expected_project_id)?;
    let wire: ProjectDetailWire = parse_json(bytes, validation::MAX_PROJECT_RESPONSE_BYTES)?;
    if wire.id != expected_project_id {
        return Err(AppError::coded("modrinth_project_identity_mismatch"));
    }
    convert_project_detail(wire)
}

fn convert_project_detail(wire: ProjectDetailWire) -> AppResult<ProjectDetail> {
    validation::validate_modrinth_id(&wire.id)?;
    validation::validate_slug(&wire.slug)?;
    validation::validate_text(&wire.title, false)?;
    validation::validate_text(&wire.description, true)?;
    validation::validate_body(&wire.body)?;
    validation::validate_token(&wire.license.id)?;
    validation::validate_text(&wire.license.name, false)?;

    let mut raw_categories = wire.categories;
    raw_categories.extend(wire.additional_categories);
    let categories = validate_string_set(
        raw_categories,
        256,
        validation::validate_category,
        "modrinth_categories_invalid",
    )?;
    let raw_loaders = validate_string_set(
        wire.loaders,
        64,
        validation::validate_category,
        "modrinth_loaders_invalid",
    )?;
    let loaders = supported_loaders(&raw_loaders);
    let minecraft_versions = validate_string_set(
        wire.game_versions,
        512,
        validation::validate_minecraft_version,
        "modrinth_game_versions_invalid",
    )?;
    let version_ids = validate_string_set(
        wire.versions,
        validation::MAX_PROJECT_VERSIONS,
        validation::validate_modrinth_id,
        "modrinth_version_ids_invalid",
    )?;
    let icon_url = wire
        .icon_url
        .map(|value| {
            validation::validate_cdn_url(&value, Some(&wire.id)).map(|url| url.to_string())
        })
        .transpose()?;
    if wire.gallery.len() > 128 {
        return Err(AppError::coded("modrinth_gallery_invalid"));
    }
    let gallery = wire
        .gallery
        .into_iter()
        .map(|image| {
            validation::validate_optional_text(image.title.as_deref())?;
            validation::validate_optional_text(image.description.as_deref())?;
            let url = validation::validate_cdn_url(&image.url, Some(&wire.id))?;
            Ok(GalleryImage {
                url: url.to_string(),
                featured: image.featured,
                title: image.title,
                description: image.description,
                created_at: image.created,
                ordering: image.ordering,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(ProjectDetail {
        project_id: wire.id,
        slug: wire.slug,
        title: wire.title,
        description: wire.description,
        body: wire.body,
        project_type: wire.project_type,
        status: wire.status,
        client_side: wire.client_side,
        server_side: wire.server_side,
        downloads: wire.downloads,
        followers: wire.followers,
        icon_url,
        license: ProjectLicense {
            id: wire.license.id,
            name: wire.license.name,
        },
        minecraft_versions,
        loaders,
        categories,
        version_ids,
        gallery,
        published_at: wire.published,
        updated_at: wire.updated,
    })
}

fn parse_project_versions(
    bytes: &[u8],
    expected_project_id: &str,
    query: &VersionQuery,
) -> AppResult<Vec<ProjectVersion>> {
    validation::validate_modrinth_id(expected_project_id)?;
    validation::validate_version_query(query)?;
    let wire: Vec<VersionWire> = parse_json(bytes, validation::MAX_VERSIONS_RESPONSE_BYTES)?;
    if wire.len() > validation::MAX_VERSION_RESULTS {
        return Err(AppError::coded("modrinth_version_result_count_invalid"));
    }
    let mut version_ids = BTreeSet::new();
    wire.into_iter()
        .map(|version| {
            let converted = convert_version(version, Some(expected_project_id), Some(query))?;
            if !version_ids.insert(converted.version_id.clone()) {
                return Err(AppError::coded("modrinth_version_duplicate"));
            }
            Ok(converted)
        })
        .collect()
}

fn convert_version(
    wire: VersionWire,
    expected_project_id: Option<&str>,
    query: Option<&VersionQuery>,
) -> AppResult<ProjectVersion> {
    validation::validate_modrinth_id(&wire.id)?;
    validation::validate_modrinth_id(&wire.project_id)?;
    validation::validate_modrinth_id(&wire.author_id)?;
    if expected_project_id.is_some_and(|expected| expected != wire.project_id) {
        return Err(AppError::coded("modrinth_version_project_mismatch"));
    }
    validation::validate_text(&wire.name, false)?;
    validation::validate_text(&wire.version_number, false)?;
    if let Some(changelog) = wire.changelog.as_deref() {
        validation::validate_body(changelog)?;
    }
    let game_versions = validate_string_set(
        wire.game_versions,
        256,
        validation::validate_minecraft_version,
        "modrinth_game_versions_invalid",
    )?;
    let raw_loaders = validate_string_set(
        wire.loaders,
        64,
        validation::validate_category,
        "modrinth_loaders_invalid",
    )?;
    if let Some(query) = query {
        if query
            .minecraft_version
            .as_ref()
            .is_some_and(|expected| !game_versions.iter().any(|candidate| candidate == expected))
        {
            return Err(AppError::coded("modrinth_version_game_mismatch"));
        }
        if query.loader.is_some_and(|expected| {
            !raw_loaders
                .iter()
                .any(|candidate| candidate == expected.as_api_value())
        }) {
            return Err(AppError::coded("modrinth_version_loader_mismatch"));
        }
        if query
            .featured
            .is_some_and(|expected| expected != wire.featured)
        {
            return Err(AppError::coded("modrinth_version_featured_mismatch"));
        }
    }
    let loaders = supported_loaders(&raw_loaders);
    let dependencies = convert_dependencies(wire.dependencies)?;
    let files = convert_files(wire.files, &wire.project_id)?;
    Ok(ProjectVersion {
        version_id: wire.id,
        project_id: wire.project_id,
        author_id: wire.author_id,
        name: wire.name,
        version_number: wire.version_number,
        changelog: wire.changelog,
        game_versions,
        loaders,
        version_type: wire.version_type,
        featured: wire.featured,
        status: wire.status,
        published_at: wire.date_published,
        downloads: wire.downloads,
        dependencies,
        files,
    })
}

fn convert_dependencies(wire: Vec<DependencyWire>) -> AppResult<Vec<ModrinthDependency>> {
    if wire.len() > validation::MAX_VERSION_DEPENDENCIES {
        return Err(AppError::coded("modrinth_dependency_count_invalid"));
    }
    let mut identities = BTreeSet::new();
    wire.into_iter()
        .map(|dependency| {
            if let Some(version_id) = dependency.version_id.as_deref() {
                validation::validate_modrinth_id(version_id)?;
            }
            if let Some(project_id) = dependency.project_id.as_deref() {
                validation::validate_modrinth_id(project_id)?;
            }
            if let Some(file_name) = dependency.file_name.as_deref() {
                validation::validate_file_name(file_name)?;
            }
            let identity = dependency
                .project_id
                .as_ref()
                .map(|value| format!("project:{value}"))
                .or_else(|| {
                    dependency
                        .version_id
                        .as_ref()
                        .map(|value| format!("version:{value}"))
                })
                .or_else(|| {
                    dependency
                        .file_name
                        .as_ref()
                        .map(|value| format!("file:{}", value.to_ascii_lowercase()))
                })
                .ok_or_else(|| AppError::coded("modrinth_dependency_identity_missing"))?;
            if !identities.insert(identity) {
                return Err(AppError::coded("modrinth_dependency_duplicate"));
            }
            Ok(ModrinthDependency {
                version_id: dependency.version_id,
                project_id: dependency.project_id,
                file_name: dependency.file_name,
                dependency_type: dependency.dependency_type,
            })
        })
        .collect()
}

fn convert_files(wire: Vec<FileWire>, project_id: &str) -> AppResult<Vec<ModrinthFile>> {
    if wire.is_empty() || wire.len() > validation::MAX_VERSION_FILES {
        return Err(AppError::coded("modrinth_file_count_invalid"));
    }
    let primary_count = wire.iter().filter(|file| file.primary).count();
    if primary_count > 1 {
        return Err(AppError::coded("modrinth_primary_file_ambiguous"));
    }
    let infer_primary = primary_count == 0;
    let mut names = BTreeSet::new();
    wire.into_iter()
        .enumerate()
        .map(|(index, file)| {
            validation::validate_file_name(&file.filename)?;
            if !names.insert(file.filename.to_ascii_lowercase()) {
                return Err(AppError::coded("modrinth_file_name_collision"));
            }
            if file.size == 0 || file.size > validation::MAX_MODRINTH_FILE_SIZE_BYTES {
                return Err(AppError::coded("modrinth_file_size_invalid"));
            }
            let sha512 = file
                .hashes
                .sha512
                .ok_or_else(|| AppError::coded("modrinth_sha512_required"))?;
            validation::validate_sha512(&sha512)?;
            if let Some(sha1) = file.hashes.sha1.as_deref() {
                validation::validate_sha1(sha1)?;
            }
            let url = validation::validate_download_url(&file.url, project_id)?;
            Ok(ModrinthFile::new(
                file.filename,
                file.size,
                file.primary || (infer_primary && index == 0),
                sha512,
                file.hashes.sha1,
                file.file_type,
                url,
            ))
        })
        .collect()
}

fn supported_loaders(values: &[String]) -> Vec<ModrinthLoader> {
    let mut loaders = Vec::new();
    for value in values {
        if let Some(loader) = ModrinthLoader::from_api_value(value) {
            if !loaders.contains(&loader) {
                loaders.push(loader);
            }
        }
    }
    loaders
}

fn validate_string_set(
    values: Vec<String>,
    maximum: usize,
    validate: fn(&str) -> AppResult<()>,
    error_code: &'static str,
) -> AppResult<Vec<String>> {
    if values.len() > maximum {
        return Err(AppError::coded(error_code));
    }
    let mut unique = BTreeSet::new();
    for value in &values {
        validate(value)?;
        if !unique.insert(value.clone()) {
            return Err(AppError::coded(error_code));
        }
    }
    Ok(values)
}

fn parse_json<T: DeserializeOwned>(bytes: &[u8], maximum_size: u64) -> AppResult<T> {
    if bytes.is_empty() || bytes.len() as u64 > maximum_size {
        return Err(AppError::coded("modrinth_response_size_invalid"));
    }
    serde_json::from_slice(bytes).map_err(|_| AppError::coded("modrinth_response_invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHA512: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

    fn request() -> ModrinthSearchRequest {
        ModrinthSearchRequest {
            query: "performance & rendering".into(),
            project_type: ProjectType::Mod,
            loader: Some(ModrinthLoader::Fabric),
            minecraft_version: Some("1.21.1".into()),
            index: super::super::SearchIndex::Updated,
            offset: 0,
            limit: 20,
        }
    }

    fn version_query() -> VersionQuery {
        VersionQuery {
            loader: Some(ModrinthLoader::Fabric),
            minecraft_version: Some("1.21.1".into()),
            featured: Some(true),
        }
    }

    fn version_fixture() -> serde_json::Value {
        json!({
            "name": "Sodium 0.6",
            "version_number": "0.6.0+mc1.21.1",
            "changelog": null,
            "dependencies": [{
                "version_id": null,
                "project_id": "EEFFGGHH",
                "file_name": null,
                "dependency_type": "required"
            }],
            "game_versions": ["1.21.1"],
            "version_type": "release",
            "loaders": ["fabric"],
            "featured": true,
            "status": "listed",
            "id": "IIJJKKLL",
            "project_id": "AABBCCDD",
            "author_id": "MMNNOOPP",
            "date_published": "2026-07-01T10:00:00Z",
            "downloads": 42,
            "files": [{
                "hashes": { "sha512": SHA512, "sha1": SHA1 },
                "url": "https://cdn.modrinth.com/data/AABBCCDD/versions/0.6/sodium.jar",
                "filename": "sodium.jar",
                "primary": false,
                "size": 4096,
                "file_type": null
            }],
            "unknown_future_field": "ignored"
        })
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result.expect_err("expected error").descriptor().code
    }

    #[test]
    fn search_url_is_built_from_typed_filters_only() {
        let url = build_search_url(&request()).expect("search URL");
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("api.modrinth.com"));
        assert_eq!(url.path(), "/v2/search");
        let pairs = url
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            pairs.get("query").map(|value| value.as_ref()),
            Some("performance & rendering")
        );
        assert_eq!(
            pairs.get("index").map(|value| value.as_ref()),
            Some("updated")
        );
        let facets = pairs.get("facets").expect("facets");
        assert!(facets.contains("project_type:mod"));
        assert!(facets.contains("categories:fabric"));
        assert!(facets.contains("versions:1.21.1"));
    }

    #[test]
    fn search_response_is_typed_bounded_and_filter_consistent() {
        let fixture = json!({
            "hits": [{
                "slug": "sodium",
                "title": "Sodium",
                "description": "Rendering optimization",
                "categories": ["optimization", "fabric"],
                "project_type": "mod",
                "downloads": 100,
                "icon_url": "https://cdn.modrinth.com/data/AABBCCDD/icon.png",
                "project_id": "AABBCCDD",
                "author": "jellysquid3",
                "versions": ["1.21.1"],
                "follows": 20,
                "date_created": "2020-01-01T00:00:00Z",
                "date_modified": "2026-01-01T00:00:00Z",
                "license": "LGPL-3.0-only",
                "future_field": true
            }],
            "offset": 0,
            "limit": 20,
            "total_hits": 1
        });
        let page =
            parse_search_response(&serde_json::to_vec(&fixture).expect("fixture"), &request())
                .expect("search response");
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].loaders, vec![ModrinthLoader::Fabric]);

        let mut hostile = fixture;
        hostile["hits"][0]["icon_url"] = json!(concat!("https", "://evil.invalid/icon.png"));
        assert!(
            parse_search_response(&serde_json::to_vec(&hostile).expect("fixture"), &request(),)
                .is_err()
        );
    }

    #[test]
    fn project_detail_validates_identity_assets_and_supported_loaders() {
        let fixture = json!({
            "slug": "sodium",
            "title": "Sodium",
            "description": "Rendering optimization",
            "categories": ["optimization"],
            "client_side": "required",
            "server_side": "unsupported",
            "body": "Long description",
            "status": "approved",
            "additional_categories": ["fabric"],
            "project_type": "mod",
            "downloads": 100,
            "icon_url": "https://cdn.modrinth.com/data/AABBCCDD/icon.png",
            "id": "AABBCCDD",
            "published": "2020-01-01T00:00:00Z",
            "updated": "2026-01-01T00:00:00Z",
            "followers": 20,
            "license": { "id": "LGPL-3.0-only", "name": "LGPL v3" },
            "versions": ["IIJJKKLL"],
            "game_versions": ["1.21.1"],
            "loaders": ["fabric", "forge"],
            "gallery": [{
                "url": "https://cdn.modrinth.com/data/AABBCCDD/images/demo.png",
                "featured": true,
                "title": "Demo",
                "description": null,
                "created": "2026-01-01T00:00:00Z",
                "ordering": 0
            }]
        });
        let detail =
            parse_project_detail(&serde_json::to_vec(&fixture).expect("fixture"), "AABBCCDD")
                .expect("detail");
        assert_eq!(detail.loaders, vec![ModrinthLoader::Fabric]);
        assert_eq!(detail.gallery.len(), 1);
        assert_eq!(
            error_code(parse_project_detail(
                &serde_json::to_vec(&fixture).expect("fixture"),
                "EEFFGGHH",
            )),
            "modrinth_project_identity_mismatch"
        );
    }

    #[test]
    fn versions_require_sha512_and_keep_download_url_out_of_serialization() {
        let bytes = serde_json::to_vec(&json!([version_fixture()])).expect("fixture");
        let versions =
            parse_project_versions(&bytes, "AABBCCDD", &version_query()).expect("versions");
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].dependencies.len(), 1);
        let primary = versions[0].primary_file().expect("inferred primary");
        assert_eq!(primary.file_name, "sodium.jar");
        assert_eq!(primary.upstream_sha512, SHA512);
        assert_eq!(
            primary.validated_download_url().host_str(),
            Some("cdn.modrinth.com")
        );
        let serialized = serde_json::to_string(&versions[0]).expect("serialize version");
        assert!(!serialized.contains("cdn.modrinth.com"));
        assert!(!serialized.contains("downloadUrl"));

        let mut missing_hash = version_fixture();
        missing_hash["files"][0]["hashes"]["sha512"] = serde_json::Value::Null;
        assert_eq!(
            error_code(parse_project_versions(
                &serde_json::to_vec(&json!([missing_hash])).expect("fixture"),
                "AABBCCDD",
                &version_query(),
            )),
            "modrinth_sha512_required"
        );
    }

    #[test]
    fn versions_reject_filter_drift_domains_and_case_collisions() {
        let mut wrong_loader = version_fixture();
        wrong_loader["loaders"] = json!(["neoforge"]);
        assert_eq!(
            error_code(parse_project_versions(
                &serde_json::to_vec(&json!([wrong_loader])).expect("fixture"),
                "AABBCCDD",
                &version_query(),
            )),
            "modrinth_version_loader_mismatch"
        );

        let mut wrong_domain = version_fixture();
        wrong_domain["files"][0]["url"] = json!(concat!("https", "://example.invalid/sodium.jar"));
        assert!(parse_project_versions(
            &serde_json::to_vec(&json!([wrong_domain])).expect("fixture"),
            "AABBCCDD",
            &version_query(),
        )
        .is_err());

        let mut collision = version_fixture();
        let mut second = collision["files"][0].clone();
        second["filename"] = json!("SODIUM.JAR");
        second["primary"] = json!(false);
        collision["files"]
            .as_array_mut()
            .expect("files")
            .push(second);
        assert_eq!(
            error_code(parse_project_versions(
                &serde_json::to_vec(&json!([collision])).expect("fixture"),
                "AABBCCDD",
                &version_query(),
            )),
            "modrinth_file_name_collision"
        );
    }

    #[test]
    fn dependency_metadata_is_unambiguous_and_bounded() {
        let dependency = DependencyWire {
            version_id: None,
            project_id: None,
            file_name: None,
            dependency_type: DependencyType::Required,
        };
        assert_eq!(
            error_code(convert_dependencies(vec![dependency])),
            "modrinth_dependency_identity_missing"
        );
        let duplicate = || DependencyWire {
            version_id: None,
            project_id: Some("EEFFGGHH".into()),
            file_name: None,
            dependency_type: DependencyType::Required,
        };
        assert_eq!(
            error_code(convert_dependencies(vec![duplicate(), duplicate()])),
            "modrinth_dependency_duplicate"
        );
    }

    #[test]
    fn response_size_is_checked_before_json_parsing() {
        assert_eq!(
            error_code(parse_json::<serde_json::Value>(
                &vec![b' '; validation::MAX_SEARCH_RESPONSE_BYTES as usize + 1],
                validation::MAX_SEARCH_RESPONSE_BYTES,
            )),
            "modrinth_response_size_invalid"
        );
    }
}
