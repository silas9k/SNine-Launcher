use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use reqwest::{header, Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::time::Duration;

const METADATA_LIMIT_BYTES: u64 = 16 * 1024 * 1024;
const ARTIFACT_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlledProvider {
    MojangMetadata,
    MojangContent,
    FabricMetadata,
    FabricMaven,
    NeoforgeMaven,
}

impl ControlledProvider {
    fn allowed_hosts(self) -> &'static [&'static str] {
        match self {
            Self::MojangMetadata => &[
                "piston-meta.mojang.com",
                "launchermeta.mojang.com",
                "piston-data.mojang.com",
            ],
            Self::MojangContent => &[
                "piston-meta.mojang.com",
                "launchermeta.mojang.com",
                "piston-data.mojang.com",
                "libraries.minecraft.net",
                "resources.download.minecraft.net",
            ],
            Self::FabricMetadata => &["meta.fabricmc.net"],
            Self::FabricMaven => &["maven.fabricmc.net"],
            Self::NeoforgeMaven => &["maven.neoforged.net"],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DigestExpectation {
    pub sha1: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct VerifiedBytes {
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct ControlledHttpClient {
    client: Client,
}

impl ControlledHttpClient {
    pub fn production() -> AppResult<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(180))
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("SNine-Launcher/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        provider: ControlledProvider,
        url: &str,
    ) -> AppResult<T> {
        let verified = self
            .get_verified(
                provider,
                url,
                None,
                METADATA_LIMIT_BYTES,
                &DigestExpectation::default(),
            )
            .await?;
        serde_json::from_slice(&verified.bytes).map_err(Into::into)
    }

    pub async fn get_verified(
        &self,
        provider: ControlledProvider,
        url: &str,
        expected_size: Option<u64>,
        maximum_size: u64,
        digests: &DigestExpectation,
    ) -> AppResult<VerifiedBytes> {
        if maximum_size == 0 || maximum_size > ARTIFACT_LIMIT_BYTES {
            return Err(AppError::coded("runtime_download_limit_invalid"));
        }
        if expected_size.is_some_and(|size| size == 0 || size > maximum_size) {
            return Err(AppError::coded("runtime_download_size_invalid"));
        }
        validate_digest(digests.sha1.as_deref(), 40, "runtime_sha1_invalid")?;
        validate_digest(digests.sha256.as_deref(), 64, "runtime_sha256_invalid")?;
        let url = validate_url(provider, url)?;
        let response = self.client.get(url).send().await?;
        if response.status().is_redirection() {
            return Err(AppError::coded("runtime_redirect_forbidden"));
        }
        if !response.status().is_success() {
            return Err(AppError::coded_with(
                "runtime_http_status",
                [("status", response.status().as_u16().to_string())],
            ));
        }
        validate_response_length(response.content_length(), expected_size, maximum_size)?;

        let mut bytes = Vec::new();
        let mut size = 0_u64;
        let mut sha1 = Sha1::new();
        let mut sha256 = Sha256::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            size = size
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| AppError::coded("runtime_download_size_overflow"))?;
            if size > maximum_size || expected_size.is_some_and(|expected| size > expected) {
                return Err(AppError::coded("runtime_download_size_mismatch"));
            }
            sha1.update(&chunk);
            sha256.update(&chunk);
            bytes.extend_from_slice(&chunk);
        }
        if size == 0 || expected_size.is_some_and(|expected| expected != size) {
            return Err(AppError::coded("runtime_download_size_mismatch"));
        }
        let actual_sha1 = hex::encode(sha1.finalize());
        let actual_sha256 = hex::encode(sha256.finalize());
        if digests
            .sha1
            .as_deref()
            .is_some_and(|expected| expected != actual_sha1)
            || digests
                .sha256
                .as_deref()
                .is_some_and(|expected| expected != actual_sha256)
        {
            return Err(AppError::coded("runtime_download_hash_mismatch"));
        }
        Ok(VerifiedBytes { bytes })
    }

    pub async fn head_size(
        &self,
        provider: ControlledProvider,
        url: &str,
        maximum_size: u64,
    ) -> AppResult<u64> {
        if maximum_size == 0 || maximum_size > ARTIFACT_LIMIT_BYTES {
            return Err(AppError::coded("runtime_download_limit_invalid"));
        }
        let url = validate_url(provider, url)?;
        let response = self.client.head(url.clone()).send().await?;
        if response.status().is_redirection() {
            return Err(AppError::coded("runtime_redirect_forbidden"));
        }
        if !response.status().is_success() {
            return Err(AppError::coded_with(
                "runtime_http_status",
                [("status", response.status().as_u16().to_string())],
            ));
        }
        if let Some(size) = response.content_length().filter(|size| *size > 0) {
            if size > maximum_size {
                return Err(AppError::coded("runtime_content_length_mismatch"));
            }
            return Ok(size);
        }

        // Some Maven endpoints incorrectly report zero bytes to a HEAD request.
        // A one-byte range request gives us the authoritative total without
        // downloading an unbounded artifact before its digest is verified.
        let range_response = self
            .client
            .get(url)
            .header(header::RANGE, "bytes=0-0")
            .send()
            .await?;
        if range_response.status().is_redirection() {
            return Err(AppError::coded("runtime_redirect_forbidden"));
        }
        if range_response.status() != StatusCode::PARTIAL_CONTENT {
            return Err(AppError::coded("runtime_content_length_missing"));
        }
        let range = range_response
            .headers()
            .get(header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::coded("runtime_content_length_missing"))?;
        let size = content_range_total(range)?;
        if size > maximum_size {
            return Err(AppError::coded("runtime_content_length_mismatch"));
        }
        Ok(size)
    }
}

fn validate_response_length(
    length: Option<u64>,
    expected_size: Option<u64>,
    maximum_size: u64,
) -> AppResult<()> {
    let Some(length) = length else {
        return Ok(());
    };
    if length == 0 {
        return Ok(());
    }
    if length > maximum_size || expected_size.is_some_and(|expected| expected != length) {
        return Err(AppError::coded("runtime_content_length_mismatch"));
    }
    Ok(())
}

fn content_range_total(value: &str) -> AppResult<u64> {
    let (range, total) = value
        .strip_prefix("bytes ")
        .and_then(|value| value.rsplit_once('/'))
        .ok_or_else(|| AppError::coded("runtime_content_length_missing"))?;
    if range != "0-0"
        || total.is_empty()
        || total.len() > 20
        || !total.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(AppError::coded("runtime_content_length_missing"));
    }
    let size = total
        .parse::<u64>()
        .map_err(|_| AppError::coded("runtime_content_length_missing"))?;
    if size == 0 {
        return Err(AppError::coded("runtime_content_length_mismatch"));
    }
    Ok(size)
}

pub fn validate_url(provider: ControlledProvider, value: &str) -> AppResult<Url> {
    let url = Url::parse(value).map_err(|_| AppError::coded("runtime_url_invalid"))?;
    if url.scheme() != "https" {
        return Err(AppError::coded("runtime_https_required"));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(AppError::coded("runtime_url_authority_invalid"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::coded("runtime_host_missing"))?;
    if !provider.allowed_hosts().contains(&host) {
        return Err(AppError::coded_with(
            "runtime_domain_not_allowed",
            [("host", host.to_string())],
        ));
    }
    Ok(url)
}

pub fn maven_artifact_path(coordinate: &str) -> AppResult<String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if !(3..=5).contains(&parts.len()) {
        return Err(AppError::coded("runtime_maven_coordinate_invalid"));
    }
    for part in &parts {
        validate_maven_segment(part)?;
    }
    let group = parts[0]
        .split('.')
        .map(|segment| {
            validate_maven_segment(segment)?;
            Ok(segment)
        })
        .collect::<AppResult<Vec<_>>>()?
        .join("/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    let extension = parts.get(4).copied().unwrap_or("jar");
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

pub fn parse_sha256_sidecar(value: &[u8]) -> AppResult<String> {
    parse_checksum_sidecar(value, 64, "runtime_sha256_invalid")
}

pub fn parse_sha1_sidecar(value: &[u8]) -> AppResult<String> {
    parse_checksum_sidecar(value, 40, "runtime_sha1_invalid")
}

fn parse_checksum_sidecar(value: &[u8], length: usize, code: &str) -> AppResult<String> {
    if value.len() > 512 {
        return Err(AppError::coded("runtime_checksum_response_invalid"));
    }
    let text = std::str::from_utf8(value)
        .map_err(|_| AppError::coded("runtime_checksum_response_invalid"))?;
    let hash = text
        .split_ascii_whitespace()
        .next()
        .ok_or_else(|| AppError::coded("runtime_checksum_response_invalid"))?;
    validate_digest(Some(hash), length, code)?;
    Ok(hash.to_string())
}

pub fn validate_version_identifier(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 96
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(AppError::coded("runtime_version_invalid"));
    }
    Ok(())
}

fn validate_maven_segment(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(AppError::coded("runtime_maven_coordinate_invalid"));
    }
    Ok(())
}

fn validate_digest(value: Option<&str>, length: usize, code: &str) -> AppResult<()> {
    if value.is_some_and(|hash| {
        hash.len() != length
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) {
        return Err(AppError::coded(code));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_urls_are_exact_https_authorities() {
        assert!(validate_url(
            ControlledProvider::MojangMetadata,
            "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
        )
        .is_ok());
        for value in [
            ["http", "://piston-meta.mojang.com/x"].concat(),
            ["https", "://user@piston-meta.mojang.com/x"].concat(),
            ["https", "://piston-meta.mojang.com:444/x"].concat(),
            ["https", "://piston-meta.mojang.com.evil.invalid/x"].concat(),
            ["https", "://piston-meta.mojang.com/x#fragment"].concat(),
        ] {
            assert!(validate_url(ControlledProvider::MojangMetadata, &value).is_err());
        }
    }

    #[test]
    fn provider_host_scopes_do_not_overlap_accidentally() {
        assert!(validate_url(
            ControlledProvider::FabricMaven,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/1.jar"
        )
        .is_ok());
        assert!(validate_url(
            ControlledProvider::FabricMetadata,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/1.jar"
        )
        .is_err());
        assert!(validate_url(
            ControlledProvider::NeoforgeMaven,
            "https://maven.fabricmc.net/net/fabricmc/fabric-loader/1.jar"
        )
        .is_err());
    }

    #[test]
    fn maven_paths_reject_traversal_and_unsafe_segments() {
        assert_eq!(
            maven_artifact_path("net.fabricmc:fabric-loader:0.16.14").expect("path"),
            "net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar"
        );
        for value in [
            "net.fabricmc:../loader:1",
            "net.fabricmc:loader:..",
            "net.fabricmc:loader:1:../../x",
            "net.fabricmc:loader:1:client:jar/evil",
            "net.fabricmc:loader",
        ] {
            assert!(maven_artifact_path(value).is_err(), "{value}");
        }
    }

    #[test]
    fn checksum_sidecar_is_strict_and_lowercase() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_sha256_sidecar(format!("{hash}  artifact.jar\n").as_bytes()).expect("hash"),
            hash
        );
        assert!(parse_sha256_sidecar(hash.to_ascii_uppercase().as_bytes()).is_err());
        assert!(parse_sha256_sidecar(b"abc").is_err());
        let sha1 = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(
            parse_sha1_sidecar(format!("{sha1}\n").as_bytes()).expect("sha1"),
            sha1
        );
    }

    #[test]
    fn version_identifiers_are_bounded_ascii_tokens() {
        for value in ["1.21.4", "0.16.14", "21.1.200-beta+1"] {
            validate_version_identifier(value).expect(value);
        }
        for value in ["", ".", "..", "../1.21", "1.21/evil", "1.21:stream"] {
            assert!(validate_version_identifier(value).is_err(), "{value}");
        }
    }

    #[test]
    fn zero_length_headers_are_ignored_until_the_actual_download_verifies_size() {
        assert!(validate_response_length(Some(0), Some(123), 1024).is_ok());
        assert!(validate_response_length(Some(0), None, 1024).is_ok());
        assert!(validate_response_length(Some(123), Some(123), 1024).is_ok());
        assert!(validate_response_length(Some(124), Some(123), 1024).is_err());
        assert!(validate_response_length(Some(2048), Some(123), 1024).is_err());
    }

    #[tokio::test]
    #[ignore = "manual production metadata connectivity probe"]
    async fn production_mojang_metadata_is_retrievable() {
        let client = ControlledHttpClient::production().expect("controlled client");
        let response = client
            .get_verified(
                ControlledProvider::MojangMetadata,
                "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
                None,
                METADATA_LIMIT_BYTES,
                &DigestExpectation::default(),
            )
            .await
            .expect("Mojang metadata response");
        assert!(!response.bytes.is_empty());
    }
}
