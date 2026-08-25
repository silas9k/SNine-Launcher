use super::{
    model::{CapabilityState, CapabilityStatus, LoaderKind, LoaderSelection, ProfileRuntimeIntent},
    validation::{
        validate_canonical_relative_path, validate_logical_id, validate_profile_runtime_intent,
        validate_sha256, validate_version_token, MAX_RUNTIME_ARTIFACT_SIZE_BYTES,
    },
};
use crate::{
    error::{AppError, AppResult},
    security::paths::collision_key,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

pub const COMPONENT_MANIFEST_FORMAT: &str = "s9lab-component-manifest";
pub const COMPONENT_MANIFEST_FORMAT_VERSION: u32 = 1;
pub const COMPONENT_SIGNATURE_DOMAIN: &str = "S9LAB-COMPONENT-MANIFEST-V1";
pub const S9LAB_COMPONENT_CAPABILITY_ID: &str = "s9lab.components";

const MAX_COMPONENT_ID_BYTES: usize = 128;
const MAX_COMPONENT_VERSION_BYTES: usize = 128;
const MAX_KEY_ID_BYTES: usize = 128;
const MIN_ENCODED_SIGNATURE_BYTES: usize = 32;
const MAX_ENCODED_SIGNATURE_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct S9labComponentManifestV1 {
    pub format: String,
    pub format_version: u32,
    pub signature_domain: String,
    pub key_id: String,
    pub component_id: String,
    pub component_version: String,
    pub minecraft_version: String,
    pub loader: LoaderSelection,
    pub size_bytes: u64,
    pub sha256: String,
    pub relative_target: String,
    pub signature: String,
}

pub trait SignatureVerifier: Send + Sync {
    fn capability_status(&self) -> CapabilityStatus;

    fn verify(
        &self,
        key_id: &str,
        domain: &str,
        payload: &[u8],
        encoded_signature: &str,
    ) -> AppResult<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoProductionTrust;

impl SignatureVerifier for NoProductionTrust {
    fn capability_status(&self) -> CapabilityStatus {
        production_component_capability()
    }

    fn verify(
        &self,
        _key_id: &str,
        _domain: &str,
        _payload: &[u8],
        _encoded_signature: &str,
    ) -> AppResult<()> {
        Err(AppError::coded("component_trust_unconfigured"))
    }
}

pub fn production_component_capability() -> CapabilityStatus {
    CapabilityStatus::unconfigured(
        S9LAB_COMPONENT_CAPABILITY_ID,
        "component_trust_unconfigured",
    )
}

pub fn validate_component_manifest(
    manifest: &S9labComponentManifestV1,
    runtime: &ProfileRuntimeIntent,
) -> AppResult<()> {
    validate_profile_runtime_intent(runtime)?;
    if manifest.format != COMPONENT_MANIFEST_FORMAT
        || manifest.format_version != COMPONENT_MANIFEST_FORMAT_VERSION
    {
        return Err(AppError::coded("component_manifest_format_unsupported"));
    }
    if manifest.signature_domain != COMPONENT_SIGNATURE_DOMAIN {
        return Err(AppError::coded("component_signature_domain_invalid"));
    }
    validate_key_id(&manifest.key_id)?;
    validate_component_id(&manifest.component_id)?;
    validate_version_token(
        &manifest.component_version,
        MAX_COMPONENT_VERSION_BYTES,
        "component_version_invalid",
    )?;
    validate_version_token(
        &manifest.minecraft_version,
        64,
        "component_minecraft_version_invalid",
    )?;
    validate_component_loader(&manifest.loader)?;

    if manifest.size_bytes == 0 || manifest.size_bytes > MAX_RUNTIME_ARTIFACT_SIZE_BYTES {
        return Err(AppError::coded_with(
            "component_size_invalid",
            [
                ("sizeBytes", manifest.size_bytes.to_string()),
                ("maxSizeBytes", MAX_RUNTIME_ARTIFACT_SIZE_BYTES.to_string()),
            ],
        ));
    }
    validate_sha256(&manifest.sha256).map_err(|_| AppError::coded("component_sha256_invalid"))?;
    validate_component_target(&manifest.component_id, &manifest.relative_target)?;
    validate_encoded_signature(&manifest.signature)?;
    validate_component_compatibility(manifest, runtime)
}

pub fn validate_and_verify_component_manifest(
    manifest: &S9labComponentManifestV1,
    runtime: &ProfileRuntimeIntent,
    verifier: &dyn SignatureVerifier,
) -> AppResult<()> {
    validate_component_manifest(manifest, runtime)?;
    let capability = verifier.capability_status();
    require_component_capability(&capability)?;
    verifier.verify(
        &manifest.key_id,
        &manifest.signature_domain,
        &canonical_component_signature_payload(manifest),
        &manifest.signature,
    )
}

pub fn validate_component_target(component_id: &str, target: &str) -> AppResult<PathBuf> {
    validate_component_id(component_id)?;
    let normalized = validate_canonical_relative_path(target)?;
    let expected = format!("mods/s9lab/{component_id}.jar");
    if target != expected {
        return Err(AppError::coded_with(
            "component_target_invalid",
            [
                ("componentId", component_id.to_string()),
                ("target", target.to_string()),
                ("expected", expected),
            ],
        ));
    }
    Ok(normalized)
}

/// Produces the exact bytes covered by a component signature.
///
/// Text values are UTF-8 and length-prefixed with an unsigned big-endian
/// 64-bit length. Numeric values are unsigned big-endian integers. This avoids
/// delimiter, escaping, map-ordering, and JSON-number ambiguities.
pub fn canonical_component_signature_payload(manifest: &S9labComponentManifestV1) -> Vec<u8> {
    let mut payload = Vec::new();
    append_text(&mut payload, COMPONENT_SIGNATURE_DOMAIN);
    append_text(&mut payload, &manifest.signature_domain);
    append_text(&mut payload, &manifest.format);
    payload.extend_from_slice(&manifest.format_version.to_be_bytes());
    append_text(&mut payload, &manifest.key_id);
    append_text(&mut payload, &manifest.component_id);
    append_text(&mut payload, &manifest.component_version);
    append_text(&mut payload, &manifest.minecraft_version);
    append_text(&mut payload, manifest.loader.kind.as_str());
    append_optional_text(&mut payload, manifest.loader.loader_version.as_deref());
    payload.extend_from_slice(&manifest.size_bytes.to_be_bytes());
    append_text(&mut payload, &manifest.sha256);
    append_text(&mut payload, &manifest.relative_target);
    payload
}

fn append_text(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value.as_bytes());
}

fn append_optional_text(output: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            output.push(1);
            append_text(output, value);
        }
        None => output.push(0),
    }
}

fn require_component_capability(status: &CapabilityStatus) -> AppResult<()> {
    if status.capability_id != S9LAB_COMPONENT_CAPABILITY_ID {
        return Err(AppError::coded("component_capability_identity_invalid"));
    }
    match status.state {
        CapabilityState::Available if status.reason_code.is_empty() => Ok(()),
        CapabilityState::Available => Err(AppError::coded("component_capability_status_invalid")),
        CapabilityState::Unconfigured | CapabilityState::Disabled
            if !status.reason_code.is_empty() =>
        {
            Err(AppError::coded_with(
                status.reason_code.clone(),
                [
                    ("capabilityId", status.capability_id.clone()),
                    ("capabilityState", format!("{:?}", status.state)),
                ],
            ))
        }
        CapabilityState::Unconfigured | CapabilityState::Disabled => {
            Err(AppError::coded("component_capability_status_invalid"))
        }
    }
}

fn validate_component_compatibility(
    manifest: &S9labComponentManifestV1,
    runtime: &ProfileRuntimeIntent,
) -> AppResult<()> {
    if manifest.minecraft_version != runtime.minecraft_version {
        return Err(AppError::coded("component_minecraft_incompatible"));
    }
    if manifest.loader.kind != runtime.loader.kind {
        return Err(AppError::coded("component_loader_incompatible"));
    }
    if let Some(required) = manifest.loader.loader_version.as_deref() {
        match runtime.loader.loader_version.as_deref() {
            Some(actual) if actual == required => {}
            Some(_) => {
                return Err(AppError::coded("component_loader_version_incompatible"));
            }
            None => {
                return Err(AppError::coded("component_loader_version_unresolved"));
            }
        }
    }
    Ok(())
}

fn validate_component_loader(loader: &LoaderSelection) -> AppResult<()> {
    match (loader.kind, loader.loader_version.as_deref()) {
        (LoaderKind::Vanilla, None) => Ok(()),
        (LoaderKind::Vanilla, Some(_)) => Err(AppError::coded(
            "component_vanilla_loader_version_forbidden",
        )),
        (_, None) => Ok(()),
        (_, Some(version)) => {
            validate_version_token(version, 128, "component_loader_version_invalid")
        }
    }
}

fn validate_key_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_KEY_ID_BYTES
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        || !value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(AppError::coded("component_key_id_invalid"));
    }
    Ok(())
}

fn validate_component_id(value: &str) -> AppResult<()> {
    validate_logical_id(value).map_err(|_| AppError::coded("component_id_invalid"))?;
    if value.len() > MAX_COMPONENT_ID_BYTES
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return Err(AppError::coded("component_id_invalid"));
    }
    Ok(())
}

fn validate_encoded_signature(value: &str) -> AppResult<()> {
    if !(MIN_ENCODED_SIGNATURE_BYTES..=MAX_ENCODED_SIGNATURE_BYTES).contains(&value.len())
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'='))
    {
        return Err(AppError::coded("component_signature_encoding_invalid"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JarEntryDescriptor {
    pub relative_path: String,
    pub is_directory: bool,
    pub compressed_size_bytes: u64,
    pub uncompressed_size_bytes: u64,
    #[serde(default)]
    pub encrypted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix_mode: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JarValidationLimits {
    pub max_entries: usize,
    pub max_total_compressed_bytes: u64,
    pub max_entry_uncompressed_bytes: u64,
    pub max_total_uncompressed_bytes: u64,
    pub max_compression_ratio: u64,
}

impl Default for JarValidationLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_total_compressed_bytes: MAX_RUNTIME_ARTIFACT_SIZE_BYTES,
            max_entry_uncompressed_bytes: 536_870_912,
            max_total_uncompressed_bytes: 2_147_483_648,
            max_compression_ratio: 200,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JarValidationSummary {
    pub entry_count: usize,
    pub file_count: usize,
    pub total_compressed_bytes: u64,
    pub total_uncompressed_bytes: u64,
}

pub fn validate_jar_entries(
    entries: &[JarEntryDescriptor],
    limits: JarValidationLimits,
) -> AppResult<JarValidationSummary> {
    validate_jar_limits(limits)?;
    if entries.is_empty() || entries.len() > limits.max_entries {
        return Err(AppError::coded_with(
            "component_jar_entry_count_invalid",
            [
                ("entryCount", entries.len().to_string()),
                ("maxEntryCount", limits.max_entries.to_string()),
            ],
        ));
    }

    let mut paths = BTreeMap::<String, bool>::new();
    let mut file_count = 0usize;
    let mut total_compressed_bytes = 0u64;
    let mut total_uncompressed_bytes = 0u64;

    for entry in entries {
        let normalized = validate_jar_entry_path(entry)?;
        validate_jar_entry_type(entry)?;
        validate_jar_entry_sizes(entry, limits)?;

        let collision = collision_key(&normalized)?;
        if paths
            .insert(collision.clone(), entry.is_directory)
            .is_some()
        {
            return Err(AppError::coded_with(
                "component_jar_entry_collision",
                [("normalizedPath", collision)],
            ));
        }

        if !entry.is_directory {
            file_count += 1;
        }
        total_compressed_bytes = total_compressed_bytes
            .checked_add(entry.compressed_size_bytes)
            .ok_or_else(|| AppError::coded("component_jar_size_overflow"))?;
        if total_compressed_bytes > limits.max_total_compressed_bytes {
            return Err(AppError::coded("component_jar_total_compressed_too_large"));
        }
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(entry.uncompressed_size_bytes)
            .ok_or_else(|| AppError::coded("component_jar_size_overflow"))?;
        if total_uncompressed_bytes > limits.max_total_uncompressed_bytes {
            return Err(AppError::coded(
                "component_jar_total_uncompressed_too_large",
            ));
        }
    }

    for (path, is_directory) in &paths {
        for (separator, _) in path.match_indices('/') {
            if matches!(paths.get(&path[..separator]), Some(false)) {
                return Err(AppError::coded_with(
                    "component_jar_path_conflict",
                    [("normalizedPath", path.clone())],
                ));
            }
        }
        if !is_directory
            && paths
                .range(format!("{path}/")..)
                .next()
                .is_some_and(|(candidate, _)| candidate.starts_with(&format!("{path}/")))
        {
            return Err(AppError::coded_with(
                "component_jar_path_conflict",
                [("normalizedPath", path.clone())],
            ));
        }
    }

    Ok(JarValidationSummary {
        entry_count: entries.len(),
        file_count,
        total_compressed_bytes,
        total_uncompressed_bytes,
    })
}

fn validate_jar_limits(limits: JarValidationLimits) -> AppResult<()> {
    if limits.max_entries == 0
        || limits.max_total_compressed_bytes == 0
        || limits.max_entry_uncompressed_bytes == 0
        || limits.max_total_uncompressed_bytes == 0
        || limits.max_entry_uncompressed_bytes > limits.max_total_uncompressed_bytes
        || limits.max_compression_ratio == 0
    {
        return Err(AppError::coded("component_jar_limits_invalid"));
    }
    Ok(())
}

fn validate_jar_entry_path(entry: &JarEntryDescriptor) -> AppResult<PathBuf> {
    if entry.relative_path.contains('\\') {
        return Err(AppError::coded("component_jar_entry_separator_invalid"));
    }

    let path = if entry.is_directory {
        if !entry.relative_path.ends_with('/')
            || entry.relative_path.ends_with("//")
            || entry.relative_path.len() == 1
        {
            return Err(AppError::coded("component_jar_directory_marker_invalid"));
        }
        &entry.relative_path[..entry.relative_path.len() - 1]
    } else {
        if entry.relative_path.ends_with('/') {
            return Err(AppError::coded("component_jar_entry_type_mismatch"));
        }
        entry.relative_path.as_str()
    };
    validate_canonical_relative_path(path)
}

fn validate_jar_entry_type(entry: &JarEntryDescriptor) -> AppResult<()> {
    if entry.encrypted {
        return Err(AppError::coded("component_jar_encrypted_entry_forbidden"));
    }
    let Some(mode) = entry.unix_mode else {
        return Ok(());
    };
    match mode & 0o170000 {
        0 => Ok(()),
        0o040000 if entry.is_directory => Ok(()),
        0o100000 if !entry.is_directory => Ok(()),
        0o120000 => Err(AppError::coded("component_jar_symlink_forbidden")),
        0o040000 | 0o100000 => Err(AppError::coded("component_jar_entry_type_mismatch")),
        _ => Err(AppError::coded("component_jar_special_entry_forbidden")),
    }
}

fn validate_jar_entry_sizes(
    entry: &JarEntryDescriptor,
    limits: JarValidationLimits,
) -> AppResult<()> {
    if entry.is_directory {
        if entry.compressed_size_bytes != 0 || entry.uncompressed_size_bytes != 0 {
            return Err(AppError::coded("component_jar_directory_size_invalid"));
        }
        return Ok(());
    }
    if entry.uncompressed_size_bytes > limits.max_entry_uncompressed_bytes {
        return Err(AppError::coded(
            "component_jar_entry_uncompressed_too_large",
        ));
    }
    if entry.uncompressed_size_bytes > 0 && entry.compressed_size_bytes == 0 {
        return Err(AppError::coded("component_jar_compression_ratio_exceeded"));
    }
    if entry.uncompressed_size_bytes
        > entry
            .compressed_size_bytes
            .saturating_mul(limits.max_compression_ratio)
    {
        return Err(AppError::coded("component_jar_compression_ratio_exceeded"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::model::JavaPolicy;
    use sha2::{Digest, Sha256};

    #[derive(Debug, Clone, Copy)]
    struct DeterministicTestVerifier;

    impl DeterministicTestVerifier {
        fn sign(manifest: &S9labComponentManifestV1) -> String {
            let mut hasher = Sha256::new();
            hasher.update(COMPONENT_SIGNATURE_DOMAIN.as_bytes());
            hasher.update(canonical_component_signature_payload(manifest));
            format!("test-{}", hex::encode(hasher.finalize()))
        }
    }

    impl SignatureVerifier for DeterministicTestVerifier {
        fn capability_status(&self) -> CapabilityStatus {
            CapabilityStatus::available(S9LAB_COMPONENT_CAPABILITY_ID)
        }

        fn verify(
            &self,
            _key_id: &str,
            domain: &str,
            payload: &[u8],
            encoded_signature: &str,
        ) -> AppResult<()> {
            let mut hasher = Sha256::new();
            hasher.update(domain.as_bytes());
            hasher.update(payload);
            let expected = format!("test-{}", hex::encode(hasher.finalize()));
            if encoded_signature == expected {
                Ok(())
            } else {
                Err(AppError::coded("component_signature_invalid"))
            }
        }
    }

    fn runtime() -> ProfileRuntimeIntent {
        ProfileRuntimeIntent {
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
            java: JavaPolicy::Managed { major_version: 21 },
        }
    }

    fn manifest() -> S9labComponentManifestV1 {
        let mut manifest = S9labComponentManifestV1 {
            format: COMPONENT_MANIFEST_FORMAT.into(),
            format_version: COMPONENT_MANIFEST_FORMAT_VERSION,
            signature_domain: COMPONENT_SIGNATURE_DOMAIN.into(),
            key_id: "test-key-1".into(),
            component_id: "s9lab-client".into(),
            component_version: "1.0.8".into(),
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
            size_bytes: 4096,
            sha256: "a".repeat(64),
            relative_target: "mods/s9lab/s9lab-client.jar".into(),
            signature: "placeholder-signature-that-is-long-enough".into(),
        };
        manifest.signature = DeterministicTestVerifier::sign(&manifest);
        manifest
    }

    fn file(path: &str, compressed: u64, uncompressed: u64) -> JarEntryDescriptor {
        JarEntryDescriptor {
            relative_path: path.into(),
            is_directory: false,
            compressed_size_bytes: compressed,
            uncompressed_size_bytes: uncompressed,
            encrypted: false,
            unix_mode: Some(0o100644),
        }
    }

    fn directory(path: &str) -> JarEntryDescriptor {
        JarEntryDescriptor {
            relative_path: path.into(),
            is_directory: true,
            compressed_size_bytes: 0,
            uncompressed_size_bytes: 0,
            encrypted: false,
            unix_mode: Some(0o040755),
        }
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected validation failure")
            .descriptor()
            .code
    }

    #[test]
    fn production_trust_is_explicitly_unconfigured_and_fails_closed() {
        let capability = production_component_capability();
        assert_eq!(capability.state, CapabilityState::Unconfigured);
        assert!(!capability.is_available());
        assert_eq!(
            error_code(validate_and_verify_component_manifest(
                &manifest(),
                &runtime(),
                &NoProductionTrust
            )),
            "component_trust_unconfigured"
        );
    }

    #[test]
    fn deterministic_test_verifier_accepts_exact_payload_and_rejects_tampering() {
        let trusted = manifest();
        validate_and_verify_component_manifest(&trusted, &runtime(), &DeterministicTestVerifier)
            .expect("test signature");

        let mut tampered = trusted;
        tampered.component_version = "1.0.9".into();
        assert_eq!(
            error_code(validate_and_verify_component_manifest(
                &tampered,
                &runtime(),
                &DeterministicTestVerifier
            )),
            "component_signature_invalid"
        );
    }

    #[test]
    fn canonical_payload_has_domain_separation_and_covers_loader_version() {
        let first = manifest();
        let first_payload = canonical_component_signature_payload(&first);
        assert_eq!(
            canonical_component_signature_payload(&first),
            first_payload,
            "same manifest must always produce the same bytes"
        );
        assert!(first_payload
            .windows(COMPONENT_SIGNATURE_DOMAIN.len())
            .any(|window| window == COMPONENT_SIGNATURE_DOMAIN.as_bytes()));

        let mut other = first;
        other.loader.loader_version = Some("0.16.11".into());
        assert_ne!(
            canonical_component_signature_payload(&other),
            first_payload,
            "loader compatibility is signed"
        );
    }

    #[test]
    fn manifest_compatibility_is_exact_or_explicitly_loader_wide() {
        let mut value = manifest();
        value.minecraft_version = "1.20.1".into();
        assert_eq!(
            error_code(validate_component_manifest(&value, &runtime())),
            "component_minecraft_incompatible"
        );

        let mut value = manifest();
        value.loader.kind = LoaderKind::Neoforge;
        value.loader.loader_version = None;
        assert_eq!(
            error_code(validate_component_manifest(&value, &runtime())),
            "component_loader_incompatible"
        );

        let mut value = manifest();
        value.loader.loader_version = None;
        value.signature = DeterministicTestVerifier::sign(&value);
        validate_component_manifest(&value, &runtime())
            .expect("signed manifest may explicitly support the loader family");
    }

    #[test]
    fn component_target_is_confined_to_its_owned_jar_name() {
        for (target, expected_code) in [
            ("../s9lab-client.jar", "path_traversal"),
            (
                "mods/s9lab/s9lab-client.jar:ads",
                "path_alternate_data_stream",
            ),
            ("mods/s9lab/CON.jar", "path_windows_reserved_name"),
            ("mods//s9lab/s9lab-client.jar", "path_ambiguous_separator"),
            ("mods/s9lab/other.jar", "component_target_invalid"),
        ] {
            assert_eq!(
                error_code(validate_component_target("s9lab-client", target)),
                expected_code,
                "{target}"
            );
        }
    }

    #[test]
    fn safe_jar_entry_set_is_accepted_and_summarized() {
        let entries = vec![
            directory("META-INF/"),
            file("META-INF/MANIFEST.MF", 50, 100),
            file("com/s9lab/Client.class", 500, 1000),
        ];
        let summary =
            validate_jar_entries(&entries, JarValidationLimits::default()).expect("safe jar");
        assert_eq!(summary.entry_count, 3);
        assert_eq!(summary.file_count, 2);
        assert_eq!(summary.total_compressed_bytes, 550);
        assert_eq!(summary.total_uncompressed_bytes, 1100);
    }

    #[test]
    fn jar_paths_reject_traversal_ads_reserved_names_and_ambiguous_separators() {
        for (path, expected_code) in [
            ("../escape.class", "path_traversal"),
            ("file.class:stream", "path_alternate_data_stream"),
            ("CON.class", "path_windows_reserved_name"),
            ("com//s9lab/Client.class", "path_ambiguous_separator"),
        ] {
            assert_eq!(
                error_code(validate_jar_entries(
                    &[file(path, 10, 10)],
                    JarValidationLimits::default()
                )),
                expected_code,
                "{path}"
            );
        }
    }

    #[test]
    fn jar_rejects_case_collisions_symlinks_and_file_ancestor_conflicts() {
        assert_eq!(
            error_code(validate_jar_entries(
                &[
                    file("com/s9lab/Client.class", 10, 10),
                    file("COM/S9LAB/client.class", 10, 10),
                ],
                JarValidationLimits::default()
            )),
            "component_jar_entry_collision"
        );

        let mut symlink = file("com/s9lab/link", 10, 10);
        symlink.unix_mode = Some(0o120777);
        assert_eq!(
            error_code(validate_jar_entries(
                &[symlink],
                JarValidationLimits::default()
            )),
            "component_jar_symlink_forbidden"
        );

        assert_eq!(
            error_code(validate_jar_entries(
                &[
                    file("com/s9lab", 10, 10),
                    file("com/s9lab/Client.class", 10, 10),
                ],
                JarValidationLimits::default()
            )),
            "component_jar_path_conflict"
        );
    }

    #[test]
    fn jar_rejects_zip_bomb_ratios_entry_limits_and_total_limits() {
        let limits = JarValidationLimits {
            max_entries: 2,
            max_total_compressed_bytes: 1200,
            max_entry_uncompressed_bytes: 1000,
            max_total_uncompressed_bytes: 1200,
            max_compression_ratio: 10,
        };
        assert_eq!(
            error_code(validate_jar_entries(&[file("bomb.class", 1, 11)], limits)),
            "component_jar_compression_ratio_exceeded"
        );
        assert_eq!(
            error_code(validate_jar_entries(
                &[file("one.class", 100, 600), file("two.class", 100, 601),],
                limits
            )),
            "component_jar_total_uncompressed_too_large"
        );
        assert_eq!(
            error_code(validate_jar_entries(
                &[
                    file("one.class", 10, 10),
                    file("two.class", 10, 10),
                    file("three.class", 10, 10),
                ],
                limits
            )),
            "component_jar_entry_count_invalid"
        );
    }
}
