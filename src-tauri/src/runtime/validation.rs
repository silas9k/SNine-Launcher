use super::model::{
    JavaPolicy, LoaderKind, LoaderSelection, ProfileRuntimeIntent, ResolvedRuntimeItem,
    ResolvedRuntimeLockV1, RuntimeArtifactKind, RUNTIME_LOCK_FORMAT, RUNTIME_LOCK_FORMAT_VERSION,
};
use crate::{
    download::ProviderId,
    error::{AppError, AppResult},
    security::{
        paths::{collision_key, normalize_relative_path},
        PathRegistry, SecurePath,
    },
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub const MAX_RUNTIME_ITEMS: usize = 100_000;
pub const MAX_RUNTIME_ARTIFACT_SIZE_BYTES: u64 = 1_073_741_824;

const MAX_MINECRAFT_VERSION_BYTES: usize = 64;
const MAX_LOADER_VERSION_BYTES: usize = 128;
const MAX_LOGICAL_ID_BYTES: usize = 256;
const SUPPORTED_JAVA_MAJORS: [u16; 2] = [17, 21];

pub fn validate_profile_runtime_intent(intent: &ProfileRuntimeIntent) -> AppResult<()> {
    validate_version_token(
        &intent.minecraft_version,
        MAX_MINECRAFT_VERSION_BYTES,
        "runtime_minecraft_version_invalid",
    )?;
    validate_loader_selection(&intent.loader, false)?;
    validate_java_policy(&intent.java)
}

pub fn validate_resolved_runtime_item(item: &ResolvedRuntimeItem) -> AppResult<()> {
    validate_logical_id(&item.logical_id)?;
    validate_sha256(&item.sha256)?;
    if item.size_bytes == 0 || item.size_bytes > MAX_RUNTIME_ARTIFACT_SIZE_BYTES {
        return Err(AppError::coded_with(
            "runtime_artifact_size_invalid",
            [
                ("logicalId", item.logical_id.clone()),
                ("sizeBytes", item.size_bytes.to_string()),
                ("maxSizeBytes", MAX_RUNTIME_ARTIFACT_SIZE_BYTES.to_string()),
            ],
        ));
    }

    validate_canonical_relative_path(&item.relative_target)?;
    validate_artifact_target(item.kind, &item.relative_target)?;
    validate_artifact_provider(item.kind, item.provider_id)
}

pub fn validate_resolved_runtime_lock(lock: &ResolvedRuntimeLockV1) -> AppResult<()> {
    if lock.format != RUNTIME_LOCK_FORMAT || lock.format_version != RUNTIME_LOCK_FORMAT_VERSION {
        return Err(AppError::coded("runtime_lock_format_unsupported"));
    }
    validate_profile_runtime_intent(&lock.runtime)?;
    validate_loader_selection(&lock.runtime.loader, true)?;

    if lock.items.is_empty() || lock.items.len() > MAX_RUNTIME_ITEMS {
        return Err(AppError::coded_with(
            "runtime_lock_item_count_invalid",
            [
                ("itemCount", lock.items.len().to_string()),
                ("maxItemCount", MAX_RUNTIME_ITEMS.to_string()),
            ],
        ));
    }

    let mut logical_ids = BTreeSet::new();
    let mut target_keys = BTreeSet::new();
    for item in &lock.items {
        validate_resolved_runtime_item(item)?;
        validate_loader_item_provider(&lock.runtime.loader, item)?;

        let logical_key = item.logical_id.to_ascii_lowercase();
        if !logical_ids.insert(logical_key) {
            return Err(AppError::coded_with(
                "runtime_lock_logical_id_duplicate",
                [("logicalId", item.logical_id.clone())],
            ));
        }

        let target_key = collision_key(Path::new(&item.relative_target))?;
        if !target_keys.insert(target_key.clone()) {
            return Err(AppError::coded_with(
                "runtime_lock_target_collision",
                [("normalizedPath", target_key)],
            ));
        }
    }
    Ok(())
}

pub fn validate_runtime_lock_compatibility(
    intent: &ProfileRuntimeIntent,
    lock: &ResolvedRuntimeLockV1,
) -> AppResult<()> {
    validate_profile_runtime_intent(intent)?;
    validate_resolved_runtime_lock(lock)?;

    if intent.minecraft_version != lock.runtime.minecraft_version {
        return Err(AppError::coded("runtime_lock_minecraft_incompatible"));
    }
    if intent.loader.kind != lock.runtime.loader.kind {
        return Err(AppError::coded("runtime_lock_loader_incompatible"));
    }
    if let Some(requested) = intent.loader.loader_version.as_deref() {
        if lock.runtime.loader.loader_version.as_deref() != Some(requested) {
            return Err(AppError::coded("runtime_lock_loader_version_incompatible"));
        }
    }
    if intent.java != lock.runtime.java {
        return Err(AppError::coded("runtime_lock_java_policy_incompatible"));
    }
    Ok(())
}

pub fn validate_registered_runtime_target(
    registry: &PathRegistry,
    root_id: &str,
    item: &ResolvedRuntimeItem,
) -> AppResult<SecurePath> {
    validate_resolved_runtime_item(item)?;
    registry.resolve(root_id, &item.relative_target)
}

pub(crate) fn validate_canonical_relative_path(value: &str) -> AppResult<PathBuf> {
    let normalized = normalize_relative_path(Path::new(value))?;
    if value.contains('\\') {
        return Err(AppError::coded("runtime_target_separator_noncanonical"));
    }
    Ok(normalized)
}

pub(crate) fn validate_version_token(
    value: &str,
    max_bytes: usize,
    error_code: &'static str,
) -> AppResult<()> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        return Err(AppError::coded(error_code));
    }
    Ok(())
}

pub(crate) fn validate_logical_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_LOGICAL_ID_BYTES
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/@+-".contains(&byte))
        || !value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(AppError::coded("runtime_logical_id_invalid"));
    }
    Ok(())
}

pub(crate) fn validate_sha256(value: &str) -> AppResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded("runtime_sha256_invalid"));
    }
    Ok(())
}

fn validate_loader_selection(selection: &LoaderSelection, require_resolved: bool) -> AppResult<()> {
    match (selection.kind, selection.loader_version.as_deref()) {
        (LoaderKind::Vanilla, None) => Ok(()),
        (LoaderKind::Vanilla, Some(_)) => {
            Err(AppError::coded("runtime_vanilla_loader_version_forbidden"))
        }
        (_, None) if require_resolved => {
            Err(AppError::coded("runtime_lock_loader_version_required"))
        }
        (_, None) => Ok(()),
        (_, Some(version)) => validate_version_token(
            version,
            MAX_LOADER_VERSION_BYTES,
            "runtime_loader_version_invalid",
        ),
    }
}

fn validate_java_policy(policy: &JavaPolicy) -> AppResult<()> {
    if !SUPPORTED_JAVA_MAJORS.contains(&policy.major_version()) {
        return Err(AppError::coded_with(
            "runtime_java_major_unsupported",
            [("majorVersion", policy.major_version().to_string())],
        ));
    }
    Ok(())
}

fn validate_artifact_target(kind: RuntimeArtifactKind, target: &str) -> AppResult<()> {
    let (prefix, suffix) = match kind {
        RuntimeArtifactKind::MinecraftClient => ("versions", Some(".jar")),
        RuntimeArtifactKind::MinecraftVersionMetadata => ("versions", Some(".json")),
        RuntimeArtifactKind::MinecraftLibrary | RuntimeArtifactKind::LoaderLibrary => {
            ("libraries", Some(".jar"))
        }
        RuntimeArtifactKind::AssetIndex => ("assets/indexes", Some(".json")),
        RuntimeArtifactKind::AssetObject => ("assets/objects", None),
        RuntimeArtifactKind::LoggingConfiguration => ("assets/log_configs", Some(".xml")),
        RuntimeArtifactKind::LoaderMetadata => ("versions", Some(".json")),
        RuntimeArtifactKind::S9labComponent => ("mods/s9lab", Some(".jar")),
    };

    if !has_path_prefix(target, prefix)
        || suffix.is_some_and(|expected| !target.ends_with(expected))
    {
        return Err(AppError::coded_with(
            "runtime_artifact_target_invalid",
            [
                ("kind", format!("{kind:?}")),
                ("target", target.to_string()),
            ],
        ));
    }
    Ok(())
}

fn has_path_prefix(target: &str, prefix: &str) -> bool {
    target
        .strip_prefix(prefix)
        .is_some_and(|remainder| remainder.starts_with('/') && remainder.len() > 1)
}

fn validate_artifact_provider(kind: RuntimeArtifactKind, provider: ProviderId) -> AppResult<()> {
    let valid = match kind {
        RuntimeArtifactKind::MinecraftClient
        | RuntimeArtifactKind::MinecraftVersionMetadata
        | RuntimeArtifactKind::MinecraftLibrary
        | RuntimeArtifactKind::AssetIndex
        | RuntimeArtifactKind::AssetObject
        | RuntimeArtifactKind::LoggingConfiguration => provider == ProviderId::Mojang,
        RuntimeArtifactKind::LoaderMetadata | RuntimeArtifactKind::LoaderLibrary => {
            matches!(provider, ProviderId::Fabric | ProviderId::Neoforge)
        }
        RuntimeArtifactKind::S9labComponent => provider == ProviderId::S9lab,
    };
    if !valid {
        return Err(AppError::coded_with(
            "runtime_artifact_provider_invalid",
            [
                ("kind", format!("{kind:?}")),
                ("providerId", format!("{provider:?}")),
            ],
        ));
    }
    Ok(())
}

fn validate_loader_item_provider(
    selection: &LoaderSelection,
    item: &ResolvedRuntimeItem,
) -> AppResult<()> {
    if !matches!(
        item.kind,
        RuntimeArtifactKind::LoaderMetadata | RuntimeArtifactKind::LoaderLibrary
    ) {
        return Ok(());
    }
    let expected = match selection.kind {
        LoaderKind::Vanilla => {
            return Err(AppError::coded(
                "runtime_lock_vanilla_loader_artifact_forbidden",
            ));
        }
        LoaderKind::Fabric => ProviderId::Fabric,
        LoaderKind::Neoforge => ProviderId::Neoforge,
    };
    if item.provider_id != expected {
        return Err(AppError::coded("runtime_lock_loader_provider_incompatible"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::RegisteredRoot;
    use std::{fs, time::SystemTime};

    fn fabric_runtime() -> ProfileRuntimeIntent {
        ProfileRuntimeIntent {
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
            java: JavaPolicy::Managed { major_version: 21 },
        }
    }

    fn component_item(target: &str) -> ResolvedRuntimeItem {
        ResolvedRuntimeItem {
            provider_id: ProviderId::S9lab,
            logical_id: "s9lab-client".into(),
            relative_target: target.into(),
            sha256: "a".repeat(64),
            size_bytes: 4096,
            kind: RuntimeArtifactKind::S9labComponent,
        }
    }

    fn lock(items: Vec<ResolvedRuntimeItem>) -> ResolvedRuntimeLockV1 {
        ResolvedRuntimeLockV1 {
            format: RUNTIME_LOCK_FORMAT.into(),
            format_version: RUNTIME_LOCK_FORMAT_VERSION,
            runtime: fabric_runtime(),
            items,
        }
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected validation failure")
            .descriptor()
            .code
    }

    #[test]
    fn runtime_intent_serializes_with_explicit_camel_case_fields() {
        let json = serde_json::to_value(fabric_runtime()).expect("serialize intent");
        assert_eq!(json["minecraftVersion"], "1.21.1");
        assert_eq!(json["loader"]["kind"], "fabric");
        assert_eq!(json["loader"]["loaderVersion"], "0.16.10");
        assert_eq!(json["java"]["mode"], "managed");
        assert_eq!(json["java"]["majorVersion"], 21);
    }

    #[test]
    fn intent_allows_unresolved_mod_loader_but_resolved_lock_does_not() {
        let mut intent = fabric_runtime();
        intent.loader.loader_version = None;
        validate_profile_runtime_intent(&intent).expect("unresolved intent remains valid");

        let mut resolved = lock(vec![component_item("mods/s9lab/s9lab-client.jar")]);
        resolved.runtime.loader.loader_version = None;
        assert_eq!(
            error_code(validate_resolved_runtime_lock(&resolved)),
            "runtime_lock_loader_version_required"
        );
    }

    #[test]
    fn vanilla_rejects_loader_version_and_java_is_controlled() {
        let mut intent = fabric_runtime();
        intent.loader = LoaderSelection {
            kind: LoaderKind::Vanilla,
            loader_version: Some("not-applicable".into()),
        };
        assert_eq!(
            error_code(validate_profile_runtime_intent(&intent)),
            "runtime_vanilla_loader_version_forbidden"
        );

        intent.loader.loader_version = None;
        intent.java = JavaPolicy::System { major_version: 8 };
        assert_eq!(
            error_code(validate_profile_runtime_intent(&intent)),
            "runtime_java_major_unsupported"
        );
    }

    #[test]
    fn runtime_targets_preserve_path_registry_rejections() {
        for (target, expected_code) in [
            ("../escape.jar", "path_traversal"),
            ("mods/s9lab/client.jar:stream", "path_alternate_data_stream"),
            ("mods/s9lab/CON.jar", "path_windows_reserved_name"),
            ("mods//s9lab/client.jar", "path_ambiguous_separator"),
            ("mods/s9lab/client.jar/", "path_ambiguous_separator"),
        ] {
            assert_eq!(
                error_code(validate_resolved_runtime_item(&component_item(target))),
                expected_code,
                "{target}"
            );
        }
    }

    #[test]
    fn serialized_targets_require_forward_slashes() {
        assert_eq!(
            error_code(validate_resolved_runtime_item(&component_item(
                r"mods\s9lab\s9lab-client.jar"
            ))),
            "runtime_target_separator_noncanonical"
        );
    }

    #[test]
    fn runtime_items_reject_provider_namespace_hash_and_size_mismatches() {
        let mut item = component_item("mods/s9lab/s9lab-client.jar");
        item.provider_id = ProviderId::Modrinth;
        assert_eq!(
            error_code(validate_resolved_runtime_item(&item)),
            "runtime_artifact_provider_invalid"
        );

        item.provider_id = ProviderId::S9lab;
        item.sha256 = "A".repeat(64);
        assert_eq!(
            error_code(validate_resolved_runtime_item(&item)),
            "runtime_sha256_invalid"
        );

        item.sha256 = "a".repeat(64);
        item.size_bytes = MAX_RUNTIME_ARTIFACT_SIZE_BYTES + 1;
        assert_eq!(
            error_code(validate_resolved_runtime_item(&item)),
            "runtime_artifact_size_invalid"
        );

        item.size_bytes = 1;
        item.relative_target = "mods/other/s9lab-client.jar".into();
        assert_eq!(
            error_code(validate_resolved_runtime_item(&item)),
            "runtime_artifact_target_invalid"
        );
    }

    #[test]
    fn lock_rejects_case_collisions_and_loader_provider_mismatch() {
        let first = component_item("mods/s9lab/Client.jar");
        let mut second = component_item("mods/s9lab/client.jar");
        second.logical_id = "s9lab-client-secondary".into();
        assert_eq!(
            error_code(validate_resolved_runtime_lock(&lock(vec![first, second]))),
            "runtime_lock_target_collision"
        );

        let loader_item = ResolvedRuntimeItem {
            provider_id: ProviderId::Neoforge,
            logical_id: "fabric-loader".into(),
            relative_target: "libraries/fabric-loader.jar".into(),
            sha256: "b".repeat(64),
            size_bytes: 2048,
            kind: RuntimeArtifactKind::LoaderLibrary,
        };
        assert_eq!(
            error_code(validate_resolved_runtime_lock(&lock(vec![loader_item]))),
            "runtime_lock_loader_provider_incompatible"
        );
    }

    #[test]
    fn lock_compatibility_accepts_resolved_optional_version_and_rejects_drift() {
        let resolved = lock(vec![component_item("mods/s9lab/s9lab-client.jar")]);
        let mut requested = fabric_runtime();
        requested.loader.loader_version = None;
        validate_runtime_lock_compatibility(&requested, &resolved)
            .expect("resolver may select a version when intent leaves it open");

        requested.loader.loader_version = Some("0.15.0".into());
        assert_eq!(
            error_code(validate_runtime_lock_compatibility(&requested, &resolved)),
            "runtime_lock_loader_version_incompatible"
        );
    }

    #[test]
    fn registered_target_resolution_keeps_registry_length_and_chain_checks() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let anchor = std::env::temp_dir().join(format!(
            "s9lab-runtime-target-{}-{unique}",
            std::process::id()
        ));
        let profiles = anchor.join("profiles");
        fs::create_dir_all(&profiles).expect("create registry root");
        let registry = PathRegistry::new(
            &anchor,
            [RegisteredRoot {
                id: "profile-instance".into(),
                path: profiles.clone(),
            }],
        )
        .expect("registry");

        let resolved = validate_registered_runtime_target(
            &registry,
            "profile-instance",
            &component_item("mods/s9lab/s9lab-client.jar"),
        )
        .expect("secure target");
        assert!(resolved.absolute().starts_with(&profiles));

        fs::remove_dir_all(anchor).expect("remove registry fixture");
    }
}
