use super::{
    model::ModrinthSearchRequest, model::VersionQuery, MODRINTH_API_ORIGIN, MODRINTH_CDN_ORIGIN,
};
use crate::error::{AppError, AppResult};
use reqwest::Url;

pub const MAX_MODRINTH_FILE_SIZE_BYTES: u64 = 1_073_741_824;
pub(crate) const MAX_SEARCH_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const MAX_PROJECT_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_VERSIONS_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const MAX_SEARCH_HITS: usize = 100;
pub(crate) const MAX_PROJECT_VERSIONS: usize = 10_000;
pub(crate) const MAX_VERSION_RESULTS: usize = 2_048;
pub(crate) const MAX_VERSION_FILES: usize = 64;
pub(crate) const MAX_VERSION_DEPENDENCIES: usize = 512;

const MAX_QUERY_BYTES: usize = 256;
const MAX_MINECRAFT_VERSION_BYTES: usize = 64;
const MAX_TEXT_BYTES: usize = 4 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_FILE_NAME_BYTES: usize = 200;
const MAX_URL_BYTES: usize = 2_048;

pub fn validate_search_request(request: &ModrinthSearchRequest) -> AppResult<()> {
    validate_query(&request.query)?;
    if request.limit == 0 || usize::from(request.limit) > MAX_SEARCH_HITS {
        return Err(AppError::coded("modrinth_search_limit_invalid"));
    }
    if request.offset > 10_000 {
        return Err(AppError::coded("modrinth_search_offset_invalid"));
    }
    if let Some(version) = request.minecraft_version.as_deref() {
        validate_minecraft_version(version)?;
    }
    Ok(())
}

pub fn validate_version_query(query: &VersionQuery) -> AppResult<()> {
    if let Some(version) = query.minecraft_version.as_deref() {
        validate_minecraft_version(version)?;
    }
    Ok(())
}

pub fn validate_modrinth_id(value: &str) -> AppResult<()> {
    if !(3..=64).contains(&value.len())
        || !value.is_ascii()
        || !value.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(AppError::coded("modrinth_id_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_minecraft_version(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_MINECRAFT_VERSION_BYTES
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(AppError::coded("modrinth_minecraft_version_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_slug(value: &str) -> AppResult<()> {
    if !(3..=64).contains(&value.len())
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'!'
                        | b'@'
                        | b'$'
                        | b'('
                        | b')'
                        | b'.'
                        | b'+'
                        | b','
                        | b'"'
                        | b'-'
                        | b'\''
                )
        })
    {
        return Err(AppError::coded("modrinth_slug_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_text(value: &str, allow_empty: bool) -> AppResult<()> {
    validate_bounded_text(value, MAX_TEXT_BYTES, allow_empty, "modrinth_text_invalid")
}

pub(crate) fn validate_body(value: &str) -> AppResult<()> {
    validate_bounded_text(value, MAX_BODY_BYTES, true, "modrinth_body_invalid")
}

pub(crate) fn validate_optional_text(value: Option<&str>) -> AppResult<()> {
    if let Some(value) = value {
        validate_text(value, true)?;
    }
    Ok(())
}

pub(crate) fn validate_token(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 96
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'+' | b'(' | b')')
        })
    {
        return Err(AppError::coded("modrinth_token_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_category(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(AppError::coded("modrinth_category_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_file_name(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_FILE_NAME_BYTES
        || !value.is_ascii()
        || value == "."
        || value == ".."
        || value.starts_with([' ', '.'])
        || value.ends_with([' ', '.'])
        || value.bytes().any(|byte| {
            byte.is_ascii_control()
                || matches!(
                    byte,
                    b'/' | b'\\' | b':' | b'"' | b'<' | b'>' | b'|' | b'?' | b'*'
                )
        })
        || is_windows_reserved_name(value)
    {
        return Err(AppError::coded("modrinth_file_name_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_sha512(value: &str) -> AppResult<()> {
    validate_lower_hex(value, 128, "modrinth_sha512_invalid")
}

pub(crate) fn validate_sha1(value: &str) -> AppResult<()> {
    validate_lower_hex(value, 40, "modrinth_sha1_invalid")
}

pub(crate) fn validate_api_url(url: &Url) -> AppResult<()> {
    validate_exact_authority(url, MODRINTH_API_ORIGIN, false)?;
    if !url.path().starts_with("/v2/") {
        return Err(AppError::coded("modrinth_api_path_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_cdn_url(value: &str, expected_project_id: Option<&str>) -> AppResult<Url> {
    if value.len() > MAX_URL_BYTES {
        return Err(AppError::coded("modrinth_cdn_url_invalid"));
    }
    let url = Url::parse(value).map_err(|_| AppError::coded("modrinth_cdn_url_invalid"))?;
    validate_exact_authority(&url, MODRINTH_CDN_ORIGIN, true)?;

    let path = url.path();
    let lowered = path.to_ascii_lowercase();
    if !path.starts_with("/data/")
        || path.contains("//")
        || path.contains('\\')
        || path.contains(':')
        || ["%2f", "%5c", "%00", "%3a"]
            .iter()
            .any(|encoded| lowered.contains(encoded))
    {
        return Err(AppError::coded("modrinth_cdn_path_invalid"));
    }

    if let Some(project_id) = expected_project_id {
        validate_modrinth_id(project_id)?;
        let prefix = format!("/data/{project_id}/");
        if !path.starts_with(&prefix) || path.len() <= prefix.len() {
            return Err(AppError::coded("modrinth_cdn_project_mismatch"));
        }
    }
    Ok(url)
}

pub(crate) fn validate_download_url(value: &str, expected_project_id: &str) -> AppResult<Url> {
    let url = validate_cdn_url(value, Some(expected_project_id))?;
    let prefix = format!("/data/{expected_project_id}/versions/");
    if !url.path().starts_with(&prefix) || url.path().len() <= prefix.len() {
        return Err(AppError::coded("modrinth_cdn_download_path_invalid"));
    }
    Ok(url)
}

fn validate_query(value: &str) -> AppResult<()> {
    if value.len() > MAX_QUERY_BYTES
        || value
            .chars()
            .any(|character| character.is_control() || character == '\u{feff}')
    {
        return Err(AppError::coded("modrinth_search_query_invalid"));
    }
    Ok(())
}

fn validate_bounded_text(
    value: &str,
    maximum: usize,
    allow_empty: bool,
    error_code: &'static str,
) -> AppResult<()> {
    if value.len() > maximum
        || (!allow_empty && value.trim().is_empty())
        || value.chars().any(|character| {
            character == '\u{0}'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
    {
        return Err(AppError::coded(error_code));
    }
    Ok(())
}

fn validate_lower_hex(value: &str, length: usize, error_code: &'static str) -> AppResult<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded(error_code));
    }
    Ok(())
}

fn validate_exact_authority(url: &Url, origin: &str, forbid_query: bool) -> AppResult<()> {
    let expected = Url::parse(origin).map_err(|_| AppError::coded("modrinth_origin_invalid"))?;
    if url.scheme() != "https"
        || url.host_str() != expected.host_str()
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || (forbid_query && url.query().is_some())
    {
        return Err(AppError::coded("modrinth_url_authority_invalid"));
    }
    Ok(())
}

fn is_windows_reserved_name(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && matches!(&stem[..3], "COM" | "LPT")
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modrinth::{ModrinthLoader, ProjectType, SearchIndex};

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result.expect_err("expected error").descriptor().code
    }

    #[test]
    fn typed_search_input_rejects_raw_urls_and_unknown_fields() {
        let value = serde_json::json!({
            "query": "performance",
            "projectType": "mod",
            "loader": "fabric",
            "minecraftVersion": "1.21.1",
            "index": "relevance",
            "offset": 0,
            "limit": 20,
            "rawUrl": concat!("https", "://evil.invalid/search")
        });
        assert!(serde_json::from_value::<ModrinthSearchRequest>(value).is_err());

        validate_search_request(&ModrinthSearchRequest {
            query: "performance".into(),
            project_type: ProjectType::Mod,
            loader: Some(ModrinthLoader::Fabric),
            minecraft_version: Some("1.21.1".into()),
            index: SearchIndex::Relevance,
            offset: 0,
            limit: 20,
        })
        .expect("valid request");
    }

    #[test]
    fn api_and_cdn_authorities_are_exact_https_hosts() {
        let api = Url::parse("https://api.modrinth.com/v2/search?query=x").expect("api URL");
        validate_api_url(&api).expect("approved API host");

        for value in [
            "http://cdn.modrinth.com/data/AABBCCDD/versions/1/a.jar",
            concat!(
                "https",
                "://cdn.modrinth.com.evil.invalid/data/AABBCCDD/versions/1/a.jar"
            ),
            concat!(
                "https",
                "://user@cdn.modrinth.com/data/AABBCCDD/versions/1/a.jar"
            ),
            "https://cdn.modrinth.com:444/data/AABBCCDD/versions/1/a.jar",
            "https://cdn.modrinth.com/data/AABBCCDD/versions/1/a.jar?redirect=x",
            "https://cdn.modrinth.com/data/AABBCCDD/versions/1/a.jar#fragment",
        ] {
            assert!(
                validate_cdn_url(value, Some("AABBCCDD")).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn cdn_urls_are_bound_to_the_response_project() {
        validate_cdn_url(
            "https://cdn.modrinth.com/data/AABBCCDD/versions/1.0/a.jar",
            Some("AABBCCDD"),
        )
        .expect("matching project");
        assert_eq!(
            error_code(validate_cdn_url(
                "https://cdn.modrinth.com/data/EEFFGGHH/versions/1.0/a.jar",
                Some("AABBCCDD"),
            )),
            "modrinth_cdn_project_mismatch"
        );
    }

    #[test]
    fn file_names_reject_paths_ads_and_windows_special_names() {
        for value in [
            "../mod.jar",
            "folder/mod.jar",
            "folder\\mod.jar",
            "mod.jar:stream",
            "CON.jar",
            "LPT1.zip",
            "trailing.jar.",
            "trailing.jar ",
            "mød.jar",
        ] {
            assert!(validate_file_name(value).is_err(), "{value}");
        }
        validate_file_name("sodium-fabric-0.6.0+mc1.21.1.jar").expect("safe name");
    }

    #[test]
    fn hashes_are_lowercase_and_algorithm_specific() {
        validate_sha512(&"a".repeat(128)).expect("sha512");
        validate_sha1(&"b".repeat(40)).expect("sha1");
        assert!(validate_sha512(&"A".repeat(128)).is_err());
        assert!(validate_sha512(&"a".repeat(40)).is_err());
        assert!(validate_sha1(&"a".repeat(128)).is_err());
    }
}
