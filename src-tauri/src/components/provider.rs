use super::{
    jar::{inspect_component_jar, InspectedComponentJar},
    trust::Ed25519ComponentTrust,
};
use crate::{
    download::{CancellationToken, ProviderId},
    error::{AppError, AppResult},
    runtime::{
        validate_and_verify_component_manifest, validate_profile_runtime_intent,
        validate_resolved_runtime_item, CapabilityState, CapabilityStatus, JavaPolicy, LoaderKind,
        ProfileRuntimeIntent, ResolvedRuntimeItem, RuntimeArtifactKind, S9labComponentManifestV1,
        S9LAB_COMPONENT_CAPABILITY_ID,
    },
    security::{fs as secure_fs, SecurePath},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Write, net::IpAddr, time::Duration};

pub const COMPONENT_CATALOG_FORMAT: &str = "s9lab-component-catalog";
pub const COMPONENT_CATALOG_FORMAT_VERSION: u32 = 1;
pub const S9LAB_COMPONENT_ORIGIN_ENV: &str = "S9LAB_COMPONENT_PROVIDER_ORIGIN";
pub const S9LAB_COMPONENT_TRUST_KEYS_ENV: &str = "S9LAB_COMPONENT_TRUST_KEYS_JSON";

const MAX_CATALOG_BYTES: usize = 4 * 1024 * 1024;
const MAX_CATALOG_COMPONENTS: usize = 10_000;
const CATALOG_PATH: &str = "/v1/components/catalog.json";
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

const COMPILED_PROVIDER_ORIGIN: Option<&str> = option_env!("S9LAB_COMPONENT_PROVIDER_ORIGIN");
const COMPILED_TRUST_KEYS: Option<&str> = option_env!("S9LAB_COMPONENT_TRUST_KEYS_JSON");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentCatalogV1 {
    pub format: String,
    pub format_version: u32,
    pub components: Vec<S9labComponentManifestV1>,
}

#[derive(Debug, Clone)]
pub struct VerifiedComponentCatalog {
    components: Vec<S9labComponentManifestV1>,
}

impl VerifiedComponentCatalog {
    pub fn components(&self) -> &[S9labComponentManifestV1] {
        &self.components
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedComponentArtifact {
    manifest: S9labComponentManifestV1,
    runtime_item: ResolvedRuntimeItem,
    download_url: reqwest::Url,
}

impl ResolvedComponentArtifact {
    pub fn manifest(&self) -> &S9labComponentManifestV1 {
        &self.manifest
    }

    pub fn runtime_item(&self) -> &ResolvedRuntimeItem {
        &self.runtime_item
    }
}

#[derive(Debug, Clone)]
pub struct S9labComponentProvider {
    origin: Option<reqwest::Url>,
    trust: Ed25519ComponentTrust,
    status: CapabilityStatus,
}

impl S9labComponentProvider {
    pub fn production() -> Self {
        Self::from_configuration(COMPILED_PROVIDER_ORIGIN, COMPILED_TRUST_KEYS)
    }

    pub fn capability_status(&self) -> CapabilityStatus {
        self.status.clone()
    }

    pub fn parse_and_verify_catalog(&self, bytes: &[u8]) -> AppResult<VerifiedComponentCatalog> {
        self.require_available()?;
        if bytes.is_empty() || bytes.len() > MAX_CATALOG_BYTES {
            return Err(AppError::coded_with(
                "component_catalog_size_invalid",
                [
                    ("sizeBytes", bytes.len().to_string()),
                    ("maxSizeBytes", MAX_CATALOG_BYTES.to_string()),
                ],
            ));
        }
        let catalog: ComponentCatalogV1 = serde_json::from_slice(bytes)
            .map_err(|_| AppError::coded("component_catalog_invalid"))?;
        if catalog.format != COMPONENT_CATALOG_FORMAT
            || catalog.format_version != COMPONENT_CATALOG_FORMAT_VERSION
        {
            return Err(AppError::coded("component_catalog_format_unsupported"));
        }
        if catalog.components.is_empty() || catalog.components.len() > MAX_CATALOG_COMPONENTS {
            return Err(AppError::coded("component_catalog_item_count_invalid"));
        }

        let mut identities = BTreeSet::new();
        for manifest in &catalog.components {
            validate_catalog_manifest(manifest)?;
            let runtime = manifest_runtime(manifest);
            validate_and_verify_component_manifest(manifest, &runtime, &self.trust)?;
            let item = runtime_item_from_manifest(manifest)?;
            validate_resolved_runtime_item(&item)?;

            let identity = format!(
                "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
                manifest.component_id.to_ascii_lowercase(),
                manifest.component_version.to_ascii_lowercase(),
                manifest.minecraft_version.to_ascii_lowercase(),
                manifest.loader.kind.as_str(),
                manifest
                    .loader
                    .loader_version
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
            );
            if !identities.insert(identity) {
                return Err(AppError::coded("component_catalog_identity_duplicate"));
            }
        }

        Ok(VerifiedComponentCatalog {
            components: catalog.components,
        })
    }

    pub async fn fetch_catalog(&self) -> AppResult<VerifiedComponentCatalog> {
        self.require_available()?;
        let response = self
            .controlled_client()?
            .get(self.catalog_url()?)
            .send()
            .await?;
        if response.status().is_redirection() {
            return Err(AppError::coded("component_provider_redirect_forbidden"));
        }
        if !response.status().is_success() {
            return Err(AppError::coded_with(
                "component_provider_http_status",
                [("status", response.status().as_u16().to_string())],
            ));
        }
        let content_length = response
            .content_length()
            .ok_or_else(|| AppError::coded("component_catalog_content_length_missing"))?;
        if content_length == 0 || content_length > MAX_CATALOG_BYTES as u64 {
            return Err(AppError::coded("component_catalog_size_invalid"));
        }

        let mut bytes = Vec::with_capacity(content_length as usize);
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if bytes.len().saturating_add(chunk.len()) > MAX_CATALOG_BYTES {
                return Err(AppError::coded("component_catalog_size_invalid"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.len() as u64 != content_length {
            return Err(AppError::coded("component_catalog_content_length_mismatch"));
        }
        self.parse_and_verify_catalog(&bytes)
    }

    pub fn resolve_exact(
        &self,
        catalog: &VerifiedComponentCatalog,
        runtime: &ProfileRuntimeIntent,
        component_id: &str,
        component_version: &str,
    ) -> AppResult<ResolvedComponentArtifact> {
        self.require_available()?;
        validate_profile_runtime_intent(runtime)?;
        let manifest = catalog
            .components
            .iter()
            .find(|manifest| {
                manifest.component_id == component_id
                    && manifest.component_version == component_version
                    && manifest.minecraft_version == runtime.minecraft_version
                    && manifest.loader.kind == runtime.loader.kind
                    && manifest
                        .loader
                        .loader_version
                        .as_ref()
                        .is_none_or(|required| {
                            runtime.loader.loader_version.as_ref() == Some(required)
                        })
            })
            .ok_or_else(|| AppError::coded("component_release_not_found"))?
            .clone();

        validate_and_verify_component_manifest(&manifest, runtime, &self.trust)?;
        let runtime_item = runtime_item_from_manifest(&manifest)?;
        let download_url = self.artifact_url(&manifest)?;
        Ok(ResolvedComponentArtifact {
            manifest,
            runtime_item,
            download_url,
        })
    }

    pub async fn download_and_inspect(
        &self,
        artifact: &ResolvedComponentArtifact,
        destination: &SecurePath,
        cancellation: &CancellationToken,
    ) -> AppResult<InspectedComponentJar> {
        self.require_available()?;
        validate_resolved_runtime_item(&artifact.runtime_item)?;
        if runtime_item_from_manifest(&artifact.manifest)? != artifact.runtime_item
            || self.artifact_url(&artifact.manifest)? != artifact.download_url
        {
            return Err(AppError::coded(
                "component_resolved_artifact_integrity_invalid",
            ));
        }
        if destination.root_id() != "staging-operations" {
            return Err(AppError::coded("component_download_target_root_invalid"));
        }
        if cancellation.is_cancelled() {
            return Err(AppError::coded("download_cancelled"));
        }

        let result = self
            .download_and_inspect_inner(artifact, destination, cancellation)
            .await;
        if result.is_err() && destination.absolute().exists() {
            let _ = secure_fs::remove_tree(destination);
        }
        result
    }

    async fn download_and_inspect_inner(
        &self,
        artifact: &ResolvedComponentArtifact,
        destination: &SecurePath,
        cancellation: &CancellationToken,
    ) -> AppResult<InspectedComponentJar> {
        let response = self
            .controlled_client()?
            .get(artifact.download_url.clone())
            .send()
            .await?;
        if response.status().is_redirection() {
            return Err(AppError::coded("component_provider_redirect_forbidden"));
        }
        if !response.status().is_success() {
            return Err(AppError::coded_with(
                "component_provider_http_status",
                [("status", response.status().as_u16().to_string())],
            ));
        }
        let expected_size = artifact.manifest.size_bytes;
        if response.content_length() != Some(expected_size) {
            return Err(AppError::coded(
                "component_artifact_content_length_mismatch",
            ));
        }

        let mut output = secure_fs::open_new_file(destination)?;
        let mut actual_size = 0u64;
        let mut hasher = Sha256::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancellation.is_cancelled() {
                return Err(AppError::coded("download_cancelled"));
            }
            let chunk = chunk?;
            actual_size = actual_size
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| AppError::coded("component_artifact_size_overflow"))?;
            if actual_size > expected_size {
                return Err(AppError::coded("component_artifact_size_mismatch"));
            }
            hasher.update(&chunk);
            output.write_all(&chunk)?;
        }
        output.sync_all()?;
        drop(output);

        if actual_size != expected_size {
            return Err(AppError::coded("component_artifact_size_mismatch"));
        }
        if hex::encode(hasher.finalize()) != artifact.manifest.sha256 {
            return Err(AppError::coded("component_artifact_hash_mismatch"));
        }
        inspect_component_jar(destination, &artifact.manifest)
    }

    fn catalog_url(&self) -> AppResult<reqwest::Url> {
        self.require_available()?;
        let mut url = self
            .origin
            .clone()
            .ok_or_else(|| AppError::coded("component_provider_origin_unconfigured"))?;
        url.set_path(CATALOG_PATH);
        Ok(url)
    }

    fn controlled_client(&self) -> AppResult<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .timeout(NETWORK_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("S9Lab-Launcher/", env!("CARGO_PKG_VERSION")))
            .build()?)
    }

    fn artifact_url(&self, manifest: &S9labComponentManifestV1) -> AppResult<reqwest::Url> {
        let mut url = self
            .origin
            .clone()
            .ok_or_else(|| AppError::coded("component_provider_origin_unconfigured"))?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| AppError::coded("component_provider_origin_invalid"))?;
            segments.clear();
            for segment in [
                "v1",
                "components",
                "artifacts",
                &manifest.component_id,
                &manifest.component_version,
                &manifest.minecraft_version,
                manifest.loader.kind.as_str(),
                manifest.loader.loader_version.as_deref().unwrap_or("_"),
                &format!("{}.jar", manifest.sha256),
            ] {
                segments.push(segment);
            }
        }
        Ok(url)
    }

    fn require_available(&self) -> AppResult<()> {
        match self.status.state {
            CapabilityState::Available if self.status.reason_code.is_empty() => Ok(()),
            CapabilityState::Available => {
                Err(AppError::coded("component_capability_status_invalid"))
            }
            CapabilityState::Unconfigured | CapabilityState::Disabled => {
                Err(AppError::coded(self.status.reason_code.clone()))
            }
        }
    }

    fn from_configuration(origin: Option<&str>, encoded_keys: Option<&str>) -> Self {
        let trust = Ed25519ComponentTrust::from_compile_time(encoded_keys);
        let parsed_origin = match origin {
            None | Some("") => Ok(None),
            Some(value) => parse_provider_origin(value).map(Some),
        };

        let (origin, status) = match parsed_origin {
            Err(_) => (
                None,
                CapabilityStatus::disabled(
                    S9LAB_COMPONENT_CAPABILITY_ID,
                    "component_provider_origin_invalid",
                ),
            ),
            Ok(_) if trust.capability_status().state == CapabilityState::Disabled => {
                (None, trust.capability_status())
            }
            Ok(None) => (
                None,
                CapabilityStatus::unconfigured(
                    S9LAB_COMPONENT_CAPABILITY_ID,
                    "component_provider_origin_unconfigured",
                ),
            ),
            Ok(Some(_)) if !trust.capability_status().is_available() => {
                (None, trust.capability_status())
            }
            Ok(Some(origin)) => (
                Some(origin),
                CapabilityStatus::available(S9LAB_COMPONENT_CAPABILITY_ID),
            ),
        };

        Self {
            origin,
            trust,
            status,
        }
    }
}

pub(crate) fn runtime_item_from_manifest(
    manifest: &S9labComponentManifestV1,
) -> AppResult<ResolvedRuntimeItem> {
    let item = ResolvedRuntimeItem {
        provider_id: ProviderId::S9lab,
        logical_id: format!(
            "s9lab-component:{}@{}",
            manifest.component_id, manifest.component_version
        ),
        relative_target: manifest.relative_target.clone(),
        sha256: manifest.sha256.clone(),
        size_bytes: manifest.size_bytes,
        kind: RuntimeArtifactKind::S9labComponent,
    };
    validate_resolved_runtime_item(&item)?;
    Ok(item)
}

pub(crate) fn manifest_runtime(manifest: &S9labComponentManifestV1) -> ProfileRuntimeIntent {
    ProfileRuntimeIntent {
        minecraft_version: manifest.minecraft_version.clone(),
        loader: manifest.loader.clone(),
        java: JavaPolicy::Managed { major_version: 21 },
    }
}

fn validate_catalog_manifest(manifest: &S9labComponentManifestV1) -> AppResult<()> {
    if manifest.loader.kind == LoaderKind::Vanilla {
        return Err(AppError::coded("component_loader_descriptor_unsupported"));
    }
    if manifest.loader.kind == LoaderKind::Neoforge {
        validate_neoforge_mod_id(&manifest.component_id)?;
    }
    Ok(())
}

fn validate_neoforge_mod_id(value: &str) -> AppResult<()> {
    if value.len() < 2
        || value.len() > 64
        || !value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(AppError::coded("component_neoforge_mod_id_invalid"));
    }
    Ok(())
}

fn parse_provider_origin(value: &str) -> AppResult<reqwest::Url> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| AppError::coded("component_provider_origin_invalid"))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::coded("component_provider_origin_invalid"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
        || host.parse::<IpAddr>().is_ok()
        || !is_valid_dns_name(host)
    {
        return Err(AppError::coded("component_provider_origin_invalid"));
    }
    Ok(url)
}

fn is_valid_dns_name(value: &str) -> bool {
    value.len() <= 253
        && value.contains('.')
        && value == value.to_ascii_lowercase()
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        canonical_component_signature_payload, LoaderSelection, COMPONENT_MANIFEST_FORMAT,
        COMPONENT_MANIFEST_FORMAT_VERSION, COMPONENT_SIGNATURE_DOMAIN,
    };
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    use ring::signature::{Ed25519KeyPair, KeyPair};

    const TEST_KEY_ID: &str = "test-release-1";

    fn test_key_pair() -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[0x5au8; 32]).expect("test-only Ed25519 key")
    }

    fn configured_provider() -> S9labComponentProvider {
        let public_key = test_key_pair().public_key().as_ref().to_vec();
        let keys = format!(
            r#"{{"{TEST_KEY_ID}":"{}"}}"#,
            URL_SAFE_NO_PAD.encode(public_key)
        );
        let origin = ["https", "://components.example.test"].concat();
        S9labComponentProvider::from_configuration(Some(&origin), Some(&keys))
    }

    fn runtime() -> ProfileRuntimeIntent {
        ProfileRuntimeIntent {
            minecraft_version: "1.21.1".into(),
            loader: crate::runtime::LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
            java: JavaPolicy::Managed { major_version: 21 },
        }
    }

    fn signed_manifest() -> S9labComponentManifestV1 {
        let mut manifest = S9labComponentManifestV1 {
            format: COMPONENT_MANIFEST_FORMAT.into(),
            format_version: COMPONENT_MANIFEST_FORMAT_VERSION,
            signature_domain: COMPONENT_SIGNATURE_DOMAIN.into(),
            key_id: TEST_KEY_ID.into(),
            component_id: "s9lab_client".into(),
            component_version: "1.0.8".into(),
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
            size_bytes: 4096,
            sha256: "a".repeat(64),
            relative_target: "mods/s9lab/s9lab_client.jar".into(),
            signature: "placeholder-signature-that-is-long-enough".into(),
        };
        manifest.signature = URL_SAFE_NO_PAD.encode(
            test_key_pair()
                .sign(&canonical_component_signature_payload(&manifest))
                .as_ref(),
        );
        manifest
    }

    fn catalog_bytes(manifests: Vec<S9labComponentManifestV1>) -> Vec<u8> {
        serde_json::to_vec(&ComponentCatalogV1 {
            format: COMPONENT_CATALOG_FORMAT.into(),
            format_version: COMPONENT_CATALOG_FORMAT_VERSION,
            components: manifests,
        })
        .expect("catalog JSON")
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected component provider failure")
            .descriptor()
            .code
    }

    #[test]
    fn absent_production_configuration_is_unconfigured() {
        let provider = S9labComponentProvider::from_configuration(None, None);
        assert_eq!(
            provider.capability_status().state,
            CapabilityState::Unconfigured
        );
        assert_eq!(
            error_code(provider.parse_and_verify_catalog(b"{}")),
            "component_provider_origin_unconfigured"
        );
    }

    #[test]
    fn origin_is_https_root_only_and_never_an_ip_or_raw_path() {
        for origin in [
            ["http", "://components.example.test"].concat(),
            ["https", "://127.0.0.1"].concat(),
            ["https", "://user@components.example.test"].concat(),
            ["https", "://components.example.test/raw/catalog"].concat(),
            ["https", "://components.example.test?catalog=raw"].concat(),
            ["https", "://components.example.test:8443"].concat(),
        ] {
            assert_eq!(
                S9labComponentProvider::from_configuration(Some(&origin), None)
                    .capability_status()
                    .state,
                CapabilityState::Disabled,
                "{origin}"
            );
        }
    }

    #[test]
    fn valid_signed_catalog_resolves_only_a_controlled_artifact_route() {
        let provider = configured_provider();
        let catalog = provider
            .parse_and_verify_catalog(&catalog_bytes(vec![signed_manifest()]))
            .expect("signed catalog");
        let artifact = provider
            .resolve_exact(&catalog, &runtime(), "s9lab_client", "1.0.8")
            .expect("resolved component");

        assert_eq!(artifact.runtime_item().provider_id, ProviderId::S9lab);
        assert_eq!(
            artifact.download_url.as_str(),
            [
                "https",
                "://components.example.test/v1/components/artifacts/",
                "s9lab_client/1.0.8/1.21.1/fabric/0.16.10/",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.jar"
            ]
            .concat()
        );
        assert_eq!(
            provider.catalog_url().expect("catalog endpoint").as_str(),
            [
                "https",
                "://components.example.test/v1/components/catalog.json"
            ]
            .concat()
        );
    }

    #[test]
    fn signed_fields_signature_and_key_id_cannot_be_tampered() {
        let provider = configured_provider();
        let trusted = signed_manifest();
        for tampered in [
            {
                let mut value = trusted.clone();
                value.component_version = "1.0.9".into();
                value
            },
            {
                let mut value = trusted.clone();
                value.sha256 = "b".repeat(64);
                value
            },
            {
                let mut value = trusted.clone();
                value.size_bytes += 1;
                value
            },
            {
                let mut value = trusted.clone();
                value.key_id = "unknown-key".into();
                value
            },
            {
                let mut value = trusted.clone();
                value.signature.replace_range(0..1, "A");
                value
            },
        ] {
            let expected = if tampered.key_id == "unknown-key" {
                "component_signature_key_untrusted"
            } else {
                "component_signature_invalid"
            };
            assert_eq!(
                error_code(provider.parse_and_verify_catalog(&catalog_bytes(vec![tampered]))),
                expected
            );
        }
    }

    #[test]
    fn catalog_rejects_duplicate_identity_and_unknown_fields() {
        let provider = configured_provider();
        let manifest = signed_manifest();
        assert_eq!(
            error_code(
                provider.parse_and_verify_catalog(&catalog_bytes(vec![manifest.clone(), manifest]))
            ),
            "component_catalog_identity_duplicate"
        );

        let unknown = serde_json::json!({
            "format": COMPONENT_CATALOG_FORMAT,
            "formatVersion": COMPONENT_CATALOG_FORMAT_VERSION,
            "components": [signed_manifest()],
            "rawUrl": (["https", "://uncontrolled.invalid/component.jar"].concat())
        });
        assert_eq!(
            error_code(provider.parse_and_verify_catalog(
                &serde_json::to_vec(&unknown).expect("unknown-field JSON")
            )),
            "component_catalog_invalid"
        );
    }
}
