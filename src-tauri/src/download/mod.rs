use crate::{
    error::{AppError, AppResult},
    operations::model::new_identifier,
    security::{fs as secure_fs, PathRegistry, SecurePath},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::BTreeMap,
    io::Write,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Mojang,
    Fabric,
    Neoforge,
    Modrinth,
    S9lab,
}

#[derive(Debug, Clone)]
pub struct ProviderPolicy {
    pub allowed_hosts: Vec<String>,
    pub max_size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ResolvedDownload {
    provider: ProviderId,
    url: reqwest::Url,
    target_relative_path: String,
    expected_size_bytes: u64,
    expected_sha1: Option<String>,
    expected_sha512: Option<String>,
    expected_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub target_relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct DownloadService {
    registry: Arc<PathRegistry>,
    client: reqwest::Client,
    policies: BTreeMap<ProviderId, ProviderPolicy>,
}

impl DownloadService {
    pub fn production(registry: Arc<PathRegistry>) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("S9Lab-Launcher/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            registry,
            client,
            policies: BTreeMap::from([
                (
                    ProviderId::Mojang,
                    ProviderPolicy {
                        allowed_hosts: vec![
                            "piston-meta.mojang.com".into(),
                            "launchermeta.mojang.com".into(),
                            "piston-data.mojang.com".into(),
                            "libraries.minecraft.net".into(),
                            "resources.download.minecraft.net".into(),
                        ],
                        max_size_bytes: 1_073_741_824,
                    },
                ),
                (
                    ProviderId::Fabric,
                    ProviderPolicy {
                        allowed_hosts: vec![
                            "maven.fabricmc.net".into(),
                            "meta.fabricmc.net".into(),
                        ],
                        max_size_bytes: 268_435_456,
                    },
                ),
                (
                    ProviderId::Neoforge,
                    ProviderPolicy {
                        allowed_hosts: vec!["maven.neoforged.net".into()],
                        max_size_bytes: 1_073_741_824,
                    },
                ),
                (
                    ProviderId::Modrinth,
                    ProviderPolicy {
                        allowed_hosts: vec!["cdn.modrinth.com".into()],
                        max_size_bytes: 1_073_741_824,
                    },
                ),
                (
                    ProviderId::S9lab,
                    ProviderPolicy {
                        allowed_hosts: Vec::new(),
                        max_size_bytes: 1_073_741_824,
                    },
                ),
            ]),
        })
    }

    pub fn resolve(
        &self,
        provider: ProviderId,
        url: &str,
        target_relative_path: &str,
        expected_size_bytes: u64,
        expected_sha256: &str,
    ) -> AppResult<ResolvedDownload> {
        let policy = self
            .policies
            .get(&provider)
            .ok_or_else(|| AppError::coded("download_provider_unknown"))?;
        let url = reqwest::Url::parse(url).map_err(|_| AppError::coded("download_url_invalid"))?;
        if url.scheme() != "https" {
            return Err(AppError::coded("download_https_required"));
        }
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return Err(AppError::coded(
                "download_url_credentials_or_fragment_forbidden",
            ));
        }
        if url.port_or_known_default() != Some(443) {
            return Err(AppError::coded("download_port_not_allowed"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| AppError::coded("download_host_missing"))?;
        if !policy.allowed_hosts.iter().any(|allowed| allowed == host) {
            return Err(AppError::coded_with(
                "download_domain_not_allowed",
                [("host", host.to_string())],
            ));
        }
        if expected_size_bytes == 0 || expected_size_bytes > policy.max_size_bytes {
            return Err(AppError::coded_with(
                "download_size_limit",
                [
                    ("expected", expected_size_bytes.to_string()),
                    ("maximum", policy.max_size_bytes.to_string()),
                ],
            ));
        }
        validate_sha256(expected_sha256)?;
        let _ = self
            .registry
            .resolve("staging-operations", target_relative_path)?;
        Ok(ResolvedDownload {
            provider,
            url,
            target_relative_path: target_relative_path.to_string(),
            expected_size_bytes,
            expected_sha1: None,
            expected_sha512: None,
            expected_sha256: Some(expected_sha256.to_string()),
        })
    }

    pub(crate) fn resolve_upstream_sha1(
        &self,
        provider: ProviderId,
        url: &str,
        target_relative_path: &str,
        expected_size_bytes: u64,
        expected_sha1: &str,
    ) -> AppResult<ResolvedDownload> {
        validate_sha1(expected_sha1)?;
        let mut resolved =
            self.resolve_common(provider, url, target_relative_path, expected_size_bytes)?;
        resolved.expected_sha1 = Some(expected_sha1.to_string());
        Ok(resolved)
    }

    pub(crate) fn resolve_upstream_sha512(
        &self,
        provider: ProviderId,
        url: &str,
        target_relative_path: &str,
        expected_size_bytes: u64,
        expected_sha512: &str,
    ) -> AppResult<ResolvedDownload> {
        validate_sha512(expected_sha512)?;
        let mut resolved =
            self.resolve_common(provider, url, target_relative_path, expected_size_bytes)?;
        resolved.expected_sha512 = Some(expected_sha512.to_string());
        Ok(resolved)
    }

    fn resolve_common(
        &self,
        provider: ProviderId,
        url: &str,
        target_relative_path: &str,
        expected_size_bytes: u64,
    ) -> AppResult<ResolvedDownload> {
        let policy = self
            .policies
            .get(&provider)
            .ok_or_else(|| AppError::coded("download_provider_unknown"))?;
        let url = validate_provider_url(policy, url)?;
        if expected_size_bytes == 0 || expected_size_bytes > policy.max_size_bytes {
            return Err(AppError::coded_with(
                "download_size_limit",
                [
                    ("expected", expected_size_bytes.to_string()),
                    ("maximum", policy.max_size_bytes.to_string()),
                ],
            ));
        }
        let _ = self
            .registry
            .resolve("staging-operations", target_relative_path)?;
        Ok(ResolvedDownload {
            provider,
            url,
            target_relative_path: target_relative_path.to_string(),
            expected_size_bytes,
            expected_sha1: None,
            expected_sha512: None,
            expected_sha256: None,
        })
    }

    pub async fn download(
        &self,
        request: &ResolvedDownload,
        cancellation: &CancellationToken,
    ) -> AppResult<DownloadResult> {
        if cancellation.is_cancelled() {
            return Err(AppError::coded("download_cancelled"));
        }
        let policy = self
            .policies
            .get(&request.provider)
            .ok_or_else(|| AppError::coded("download_provider_unknown"))?;
        if request.expected_size_bytes > policy.max_size_bytes {
            return Err(AppError::coded("download_size_limit"));
        }
        let response = self.client.get(request.url.clone()).send().await?;
        if cancellation.is_cancelled() {
            return Err(AppError::coded("download_cancelled"));
        }
        if !response.status().is_success() {
            return Err(AppError::coded_with(
                "download_http_status",
                [("status", response.status().as_u16().to_string())],
            ));
        }
        if let Some(length) = response.content_length() {
            if length != request.expected_size_bytes || length > policy.max_size_bytes {
                return Err(AppError::coded("download_content_length_mismatch"));
            }
        }

        let mut session = self.begin_session(request, cancellation.clone())?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            session.push(&chunk)?;
        }
        session.finish()
    }

    fn begin_session(
        &self,
        request: &ResolvedDownload,
        cancellation: CancellationToken,
    ) -> AppResult<DownloadSession> {
        let target = self
            .registry
            .resolve("staging-operations", &request.target_relative_path)?;
        secure_fs::create_parent_directories(&target)?;
        if target.absolute().exists() {
            return Err(AppError::coded("download_target_exists"));
        }
        let parent = target
            .relative()
            .parent()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_name = target
            .relative()
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::coded("download_target_name_invalid"))?;
        let partial_relative = if parent.is_empty() {
            format!(".{file_name}.{}.partial", new_identifier("download"))
        } else {
            format!(
                "{parent}/.{file_name}.{}.partial",
                new_identifier("download")
            )
        };
        let partial = self
            .registry
            .resolve("staging-operations", partial_relative)?;
        let file = secure_fs::open_new_file(&partial)?;
        Ok(DownloadSession {
            target,
            partial,
            file: Some(file),
            expected_size: request.expected_size_bytes,
            expected_sha1: request.expected_sha1.clone(),
            expected_sha512: request.expected_sha512.clone(),
            expected_sha256: request.expected_sha256.clone(),
            written: 0,
            sha1_hasher: sha1::Sha1::new(),
            sha512_hasher: sha2::Sha512::new(),
            sha256_hasher: sha2::Sha256::new(),
            cancellation,
            committed: false,
        })
    }

    #[cfg(test)]
    fn test_resolved(
        &self,
        target: &str,
        expected: &[u8],
        expected_size_override: Option<u64>,
        hash_override: Option<String>,
    ) -> ResolvedDownload {
        ResolvedDownload {
            provider: ProviderId::Modrinth,
            url: reqwest::Url::parse("https://cdn.modrinth.com/test").expect("url"),
            target_relative_path: target.into(),
            expected_size_bytes: expected_size_override.unwrap_or(expected.len() as u64),
            expected_sha1: None,
            expected_sha512: None,
            expected_sha256: Some(
                hash_override.unwrap_or_else(|| crate::operations::model::sha256_hex(expected)),
            ),
        }
    }
}

struct DownloadSession {
    target: SecurePath,
    partial: SecurePath,
    file: Option<std::fs::File>,
    expected_size: u64,
    expected_sha1: Option<String>,
    expected_sha512: Option<String>,
    expected_sha256: Option<String>,
    written: u64,
    sha1_hasher: sha1::Sha1,
    sha512_hasher: sha2::Sha512,
    sha256_hasher: sha2::Sha256,
    cancellation: CancellationToken,
    committed: bool,
}

impl DownloadSession {
    fn push(&mut self, bytes: &[u8]) -> AppResult<()> {
        if self.cancellation.is_cancelled() {
            return Err(AppError::coded("download_cancelled"));
        }
        let next = self
            .written
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| AppError::coded("download_size_overflow"))?;
        if next > self.expected_size {
            return Err(AppError::coded("download_size_mismatch"));
        }
        self.file
            .as_mut()
            .ok_or_else(|| AppError::coded("download_session_closed"))?
            .write_all(bytes)?;
        self.sha1_hasher.update(bytes);
        self.sha512_hasher.update(bytes);
        self.sha256_hasher.update(bytes);
        self.written = next;
        Ok(())
    }

    fn finish(mut self) -> AppResult<DownloadResult> {
        if self.cancellation.is_cancelled() {
            return Err(AppError::coded("download_cancelled"));
        }
        if self.written != self.expected_size {
            return Err(AppError::coded("download_size_mismatch"));
        }
        let actual_sha1 = hex::encode(self.sha1_hasher.clone().finalize());
        let actual_sha512 = hex::encode(self.sha512_hasher.clone().finalize());
        let actual_sha256 = hex::encode(self.sha256_hasher.clone().finalize());
        if self
            .expected_sha1
            .as_deref()
            .is_some_and(|expected| expected != actual_sha1)
            || self
                .expected_sha512
                .as_deref()
                .is_some_and(|expected| expected != actual_sha512)
            || self
                .expected_sha256
                .as_deref()
                .is_some_and(|expected| expected != actual_sha256)
        {
            return Err(AppError::coded("download_hash_mismatch"));
        }
        let file = self
            .file
            .take()
            .ok_or_else(|| AppError::coded("download_session_closed"))?;
        file.sync_all()?;
        drop(file);
        secure_fs::rename_new(&self.partial, &self.target)?;
        self.committed = true;
        Ok(DownloadResult {
            target_relative_path: self.target.relative().display().to_string(),
            size_bytes: self.written,
            sha256: actual_sha256,
        })
    }
}

impl Drop for DownloadSession {
    fn drop(&mut self) {
        if !self.committed {
            self.file.take();
            let _ = secure_fs::remove_tree(&self.partial);
        }
    }
}

fn validate_sha256(value: &str) -> AppResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded("download_sha256_invalid"));
    }
    Ok(())
}

fn validate_sha1(value: &str) -> AppResult<()> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded("download_sha1_invalid"));
    }
    Ok(())
}

fn validate_sha512(value: &str) -> AppResult<()> {
    if value.len() != 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded("download_sha512_invalid"));
    }
    Ok(())
}

fn validate_provider_url(policy: &ProviderPolicy, value: &str) -> AppResult<reqwest::Url> {
    let url = reqwest::Url::parse(value).map_err(|_| AppError::coded("download_url_invalid"))?;
    if url.scheme() != "https" {
        return Err(AppError::coded("download_https_required"));
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(AppError::coded(
            "download_url_credentials_or_fragment_forbidden",
        ));
    }
    if url.port_or_known_default() != Some(443) {
        return Err(AppError::coded("download_port_not_allowed"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::coded("download_host_missing"))?;
    if !policy.allowed_hosts.iter().any(|allowed| allowed == host) {
        return Err(AppError::coded_with(
            "download_domain_not_allowed",
            [("host", host.to_string())],
        ));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::CoreServices;
    use std::fs;

    fn service(name: &str) -> (DownloadService, std::path::PathBuf) {
        let root = crate::foundation::test_root(name);
        let core = CoreServices::open_fixed(&root).expect("core");
        (core.download().clone(), root)
    }

    #[test]
    fn rejects_raw_http_and_unapproved_domains() {
        let (service, root) = service("download-policy");
        let hash = crate::operations::model::sha256_hex(b"x");
        assert!(service
            .resolve(
                ProviderId::Modrinth,
                "http://cdn.modrinth.com/x",
                "op/x",
                1,
                &hash
            )
            .is_err());
        assert!(service
            .resolve(
                ProviderId::Modrinth,
                "https://example.invalid/x",
                "op/x",
                1,
                &hash
            )
            .is_err());
        assert!(service
            .resolve(
                ProviderId::Modrinth,
                "https://cdn.modrinth.com:444/x",
                "op/x",
                1,
                &hash,
            )
            .is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn hash_error_never_activates_target() {
        let (service, root) = service("download-hash");
        let request = service.test_resolved(
            "op/file.bin",
            b"expected",
            None,
            Some(crate::operations::model::sha256_hex(b"different")),
        );
        let token = CancellationToken::default();
        let mut session = service.begin_session(&request, token).expect("session");
        session.push(b"expected").expect("push");
        assert!(session.finish().is_err());
        assert!(!root.join("staging/operations/op/file.bin").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn size_error_never_activates_target() {
        let (service, root) = service("download-size");
        let request = service.test_resolved("op/file.bin", b"abc", Some(2), None);
        let token = CancellationToken::default();
        let mut session = service.begin_session(&request, token).expect("session");
        assert!(session.push(b"abc").is_err());
        drop(session);
        assert!(!root.join("staging/operations/op/file.bin").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancellation_removes_partial_download() {
        let (service, root) = service("download-cancel");
        let request = service.test_resolved("op/file.bin", b"abcdef", None, None);
        let token = CancellationToken::default();
        let mut session = service
            .begin_session(&request, token.clone())
            .expect("session");
        session.push(b"abc").expect("first chunk");
        token.cancel();
        assert!(session.push(b"def").is_err());
        drop(session);
        let operation_dir = root.join("staging/operations/op");
        let partials = fs::read_dir(operation_dir)
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert_eq!(partials, 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verified_fixture_activates_only_after_finish() {
        let (service, root) = service("download-success");
        let request = service.test_resolved("op/file.bin", b"verified", None, None);
        let token = CancellationToken::default();
        let mut session = service.begin_session(&request, token).expect("session");
        session.push(b"veri").expect("first chunk");
        assert!(!root.join("staging/operations/op/file.bin").exists());
        session.push(b"fied").expect("second chunk");
        let result = session.finish().expect("finish");
        assert_eq!(result.size_bytes, 8);
        assert_eq!(
            fs::read(root.join("staging/operations/op/file.bin")).expect("target"),
            b"verified"
        );
        let _ = fs::remove_dir_all(root);
    }
}
