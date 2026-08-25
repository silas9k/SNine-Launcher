use crate::{
    content::{
        validate_content_resolution_request, validate_resolved_content_lock,
        ContentResolutionRequest, ContentSelection, ContentTargetRuntime, ResolvedContentLockV1,
    },
    error::{AppError, AppResult},
    profiles::model::S9labComponentSelection,
    runtime::{validate_profile_runtime_intent, ProfileRuntimeIntent},
    security::{
        fs as secure_fs,
        paths::{collision_key, normalize_relative_path, validate_existing_chain},
        PathRegistry, SecurePath,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

pub const PROFILE_EXPORT_FORMAT: &str = "site.s9lab.profile-export";
pub const PROFILE_EXPORT_FORMAT_VERSION: u32 = 1;
pub const PROFILE_EXPORT_MANIFEST_ENTRY: &str = "profile.json";

const PROFILE_EXPORT_ARTIFACT_PREFIX: &str = "artifacts/";
const MAX_PROFILE_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PROFILE_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_PROFILE_ARTIFACTS: usize = 12_288;
const MAX_PROFILE_TOTAL_UNCOMPRESSED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_PROFILE_ARCHIVE_BYTES: u64 =
    MAX_PROFILE_TOTAL_UNCOMPRESSED_BYTES + MAX_PROFILE_JSON_BYTES;
const MAX_PROFILE_COMPRESSION_RATIO: u64 = 100;
const IO_BUFFER_BYTES: usize = 64 * 1024;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Portable profile description. Deliberately absent are profile/account IDs,
/// credentials, logs, worlds, absolute paths and provider URLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileExportV1 {
    pub format: String,
    pub format_version: u32,
    pub display_name: String,
    pub runtime: ProfileRuntimeIntent,
    pub s9lab_component: S9labComponentSelection,
    #[serde(default)]
    pub desired_content: Vec<ContentSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_content: Option<ResolvedContentLockV1>,
}

impl ProfileExportV1 {
    pub fn new(
        display_name: impl Into<String>,
        runtime: ProfileRuntimeIntent,
        s9lab_component: S9labComponentSelection,
        desired_content: Vec<ContentSelection>,
        resolved_content: Option<ResolvedContentLockV1>,
    ) -> Self {
        Self {
            format: PROFILE_EXPORT_FORMAT.into(),
            format_version: PROFILE_EXPORT_FORMAT_VERSION,
            display_name: display_name.into(),
            runtime,
            s9lab_component,
            desired_content,
            resolved_content,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProfileExportArtifactSource {
    pub sha256: String,
    pub size_bytes: u64,
    pub source: SecurePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileExportSummary {
    pub artifact_count: usize,
    pub total_artifact_bytes: u64,
    pub archive_size_bytes: u64,
    pub archive_sha256: String,
}

#[derive(Debug, Clone)]
pub struct ImportedProfileArtifact {
    pub sha256: String,
    pub size_bytes: u64,
    pub staged_path: SecurePath,
}

#[derive(Debug, Clone)]
pub struct ImportedProfileExportV1 {
    pub document: ProfileExportV1,
    pub archive_sha256: String,
    pub staged_manifest: SecurePath,
    pub artifacts: Vec<ImportedProfileArtifact>,
}

#[derive(Debug, Clone, Copy)]
struct ProfileArchiveLimits {
    max_entries: usize,
    max_manifest_bytes: u64,
    max_artifact_bytes: u64,
    max_total_uncompressed_bytes: u64,
    max_archive_bytes: u64,
    max_compression_ratio: u64,
}

impl Default for ProfileArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: MAX_PROFILE_ARTIFACTS + 1,
            max_manifest_bytes: MAX_PROFILE_JSON_BYTES,
            max_artifact_bytes: MAX_PROFILE_ARTIFACT_BYTES,
            max_total_uncompressed_bytes: MAX_PROFILE_TOTAL_UNCOMPRESSED_BYTES,
            max_archive_bytes: MAX_PROFILE_ARCHIVE_BYTES,
            max_compression_ratio: MAX_PROFILE_COMPRESSION_RATIO,
        }
    }
}

/// Writes a deterministic V1 archive to a new `SecurePath`. The archive is
/// completed and synced under a sibling temporary name before the final rename.
pub fn export_profile_v1(
    destination: &SecurePath,
    document: &ProfileExportV1,
    artifacts: &[ProfileExportArtifactSource],
) -> AppResult<ProfileExportSummary> {
    export_profile_v1_with_limits(
        destination,
        document,
        artifacts,
        ProfileArchiveLimits::default(),
    )
}

fn export_profile_v1_with_limits(
    destination: &SecurePath,
    document: &ProfileExportV1,
    artifacts: &[ProfileExportArtifactSource],
    limits: ProfileArchiveLimits,
) -> AppResult<ProfileExportSummary> {
    validate_limits(limits)?;
    validate_profile_export(document)?;
    let expected = expected_artifacts(document)?;
    let sources = validate_artifact_sources(&expected, artifacts, limits)?;

    let mut manifest = serde_json::to_vec(document)
        .map_err(|_| AppError::coded("profile_export_manifest_serialize_failed"))?;
    manifest.push(b'\n');
    if manifest.len() as u64 > limits.max_manifest_bytes {
        return Err(AppError::coded("profile_export_manifest_too_large"));
    }

    secure_fs::create_parent_directories(destination)?;
    ensure_path_absent(destination.absolute(), "profile_export_target_exists")?;
    validate_existing_chain(destination.anchor(), destination.absolute())?;

    let temporary = create_temporary_file(destination)?;
    let temporary_path = temporary.path.clone();
    let mut cleanup = FileCleanup::new(temporary_path.clone());
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    let mut writer = ZipWriter::new(temporary.file);
    writer.start_file(PROFILE_EXPORT_MANIFEST_ENTRY, options)?;
    writer.write_all(&manifest)?;

    let mut total_artifact_bytes = 0u64;
    for (sha256, expected_size) in &expected {
        let source = sources
            .get(sha256)
            .ok_or_else(|| AppError::coded("profile_export_artifact_source_missing"))?;
        writer.start_file(format!("{PROFILE_EXPORT_ARTIFACT_PREFIX}{sha256}"), options)?;
        let actual = copy_verified_source(&mut writer, source, *expected_size, sha256)?;
        total_artifact_bytes = total_artifact_bytes
            .checked_add(actual)
            .ok_or_else(|| AppError::coded("profile_export_size_overflow"))?;
    }

    let file = writer.finish()?;
    file.sync_all()?;
    validate_existing_chain(destination.anchor(), &temporary_path)?;
    let archive_size_bytes = file.metadata()?.len();
    if archive_size_bytes == 0 || archive_size_bytes > limits.max_archive_bytes {
        return Err(AppError::coded("profile_export_archive_size_invalid"));
    }
    drop(file);
    let archive_sha256 = hash_file(&temporary_path, archive_size_bytes)?;

    ensure_path_absent(destination.absolute(), "profile_export_target_exists")?;
    fs::rename(&temporary_path, destination.absolute())?;
    cleanup.path = destination.absolute().to_path_buf();
    validate_existing_chain(destination.anchor(), destination.absolute())?;
    sync_parent(destination.absolute())?;
    cleanup.disarm();

    Ok(ProfileExportSummary {
        artifact_count: expected.len(),
        total_artifact_bytes,
        archive_size_bytes,
        archive_sha256,
    })
}

/// Validates and extracts a V1 archive into a previously absent staging
/// directory below a registered root. No profile state is activated here.
pub fn import_profile_v1(
    source: &SecurePath,
    registry: &PathRegistry,
    staging_root_id: &str,
    staging_prefix: impl AsRef<Path>,
) -> AppResult<ImportedProfileExportV1> {
    import_profile_v1_with_limits(
        source,
        registry,
        staging_root_id,
        staging_prefix.as_ref(),
        ProfileArchiveLimits::default(),
    )
}

fn import_profile_v1_with_limits(
    source: &SecurePath,
    registry: &PathRegistry,
    staging_root_id: &str,
    staging_prefix: &Path,
    limits: ProfileArchiveLimits,
) -> AppResult<ImportedProfileExportV1> {
    validate_limits(limits)?;
    validate_existing_chain(source.anchor(), source.absolute())?;
    let mut source_file = File::open(source.absolute())?;
    let source_metadata = source_file.metadata()?;
    if !source_metadata.is_file() {
        return Err(AppError::coded("profile_export_archive_not_regular_file"));
    }
    let archive_size = source_metadata.len();
    if archive_size == 0 || archive_size > limits.max_archive_bytes {
        return Err(AppError::coded("profile_export_archive_size_invalid"));
    }
    let archive_sha256 = hash_reader(&mut source_file, archive_size)?;
    source_file.seek(SeekFrom::Start(0))?;

    let mut archive = ZipArchive::new(source_file)
        .map_err(|_| AppError::coded("profile_export_archive_invalid"))?;
    let inventory = inspect_archive(&mut archive, limits)?;
    let manifest_bytes = read_manifest(&mut archive, &inventory, limits)?;
    let document: ProfileExportV1 = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| AppError::coded("profile_export_manifest_invalid"))?;
    validate_profile_export(&document)?;
    let expected = expected_artifacts(&document)?;
    validate_inventory_binding(&inventory, &expected)?;

    validate_existing_chain(source.anchor(), source.absolute())?;
    if fs::metadata(source.absolute())?.len() != archive_size {
        return Err(AppError::coded(
            "profile_export_archive_changed_during_import",
        ));
    }

    let staging = registry.resolve(staging_root_id, staging_prefix)?;
    ensure_path_absent(staging.absolute(), "profile_export_import_staging_exists")?;
    secure_fs::create_parent_directories(&staging)?;
    fs::create_dir(staging.absolute())?;
    validate_existing_chain(staging.anchor(), staging.absolute())?;
    let mut cleanup = StagingCleanup::new(staging.clone());

    let staged_manifest = registry.resolve(
        staging_root_id,
        staging.relative().join(PROFILE_EXPORT_MANIFEST_ENTRY),
    )?;
    secure_fs::write_new(&staged_manifest, &manifest_bytes)?;

    let mut staged_artifacts = Vec::with_capacity(expected.len());
    for (sha256, expected_size) in &expected {
        let entry_name = format!("{PROFILE_EXPORT_ARTIFACT_PREFIX}{sha256}");
        let mut entry = archive
            .by_name(&entry_name)
            .map_err(|_| AppError::coded("profile_export_artifact_missing"))?;
        let staged_path = registry.resolve(
            staging_root_id,
            staging
                .relative()
                .join(PROFILE_EXPORT_ARTIFACT_PREFIX)
                .join(sha256),
        )?;
        let mut output = secure_fs::open_new_file(&staged_path)?;
        let (actual_size, actual_sha256) = copy_and_hash(&mut entry, &mut output, *expected_size)?;
        output.sync_all()?;
        drop(output);
        if actual_size != *expected_size {
            return Err(AppError::coded("profile_export_artifact_size_mismatch"));
        }
        if actual_sha256 != *sha256 {
            return Err(AppError::coded("profile_export_artifact_hash_mismatch"));
        }
        validate_existing_chain(staged_path.anchor(), staged_path.absolute())?;
        staged_artifacts.push(ImportedProfileArtifact {
            sha256: sha256.clone(),
            size_bytes: *expected_size,
            staged_path,
        });
    }

    validate_existing_chain(source.anchor(), source.absolute())?;
    if fs::metadata(source.absolute())?.len() != archive_size {
        return Err(AppError::coded(
            "profile_export_archive_changed_during_import",
        ));
    }
    cleanup.disarm();
    Ok(ImportedProfileExportV1 {
        document,
        archive_sha256,
        staged_manifest,
        artifacts: staged_artifacts,
    })
}

pub fn validate_profile_export(document: &ProfileExportV1) -> AppResult<()> {
    if document.format != PROFILE_EXPORT_FORMAT
        || document.format_version != PROFILE_EXPORT_FORMAT_VERSION
    {
        return Err(AppError::coded("profile_export_format_unsupported"));
    }
    validate_display_name(&document.display_name)?;
    validate_profile_runtime_intent(&document.runtime)?;
    validate_component_selection(&document.s9lab_component)?;

    let content_runtime = ContentTargetRuntime {
        minecraft_version: document.runtime.minecraft_version.clone(),
        loader: document.runtime.loader.clone(),
    };
    validate_content_resolution_request(&ContentResolutionRequest {
        runtime: content_runtime.clone(),
        requested: document.desired_content.clone(),
        include_optional_dependencies: false,
    })?;

    if let Some(lock) = &document.resolved_content {
        validate_resolved_content_lock(lock)?;
        if lock.runtime != content_runtime {
            return Err(AppError::coded(
                "profile_export_content_runtime_incompatible",
            ));
        }
        let mut desired = document.desired_content.clone();
        desired.sort_by(|left, right| left.content_id.cmp(&right.content_id));
        if desired != lock.requested {
            return Err(AppError::coded(
                "profile_export_desired_content_lock_mismatch",
            ));
        }
    }
    Ok(())
}

fn validate_display_name(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.trim() != value
        || value.chars().count() > 64
        || value.chars().any(char::is_control)
    {
        return Err(AppError::coded("profile_export_display_name_invalid"));
    }
    Ok(())
}

fn validate_component_selection(selection: &S9labComponentSelection) -> AppResult<()> {
    let S9labComponentSelection::Catalog {
        component_id,
        component_version,
    } = selection
    else {
        return Ok(());
    };
    validate_ascii_token(
        component_id,
        128,
        b"._-",
        "profile_export_component_id_invalid",
    )?;
    validate_ascii_token(
        component_version,
        128,
        b"._+-",
        "profile_export_component_version_invalid",
    )
}

fn validate_ascii_token(
    value: &str,
    max_bytes: usize,
    punctuation: &[u8],
    error_code: &'static str,
) -> AppResult<()> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || punctuation.contains(&byte))
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(AppError::coded(error_code));
    }
    Ok(())
}

fn expected_artifacts(document: &ProfileExportV1) -> AppResult<BTreeMap<String, u64>> {
    let mut expected = BTreeMap::new();
    if let Some(lock) = &document.resolved_content {
        for item in &lock.items {
            match expected.insert(item.sha256.clone(), item.size_bytes) {
                Some(size) if size != item.size_bytes => {
                    return Err(AppError::coded("profile_export_artifact_identity_conflict"));
                }
                _ => {}
            }
        }
        for override_file in &lock.overrides {
            match expected.insert(override_file.sha256.clone(), override_file.size_bytes) {
                Some(size) if size != override_file.size_bytes => {
                    return Err(AppError::coded("profile_export_artifact_identity_conflict"));
                }
                _ => {}
            }
        }
    }
    if expected.len() > MAX_PROFILE_ARTIFACTS {
        return Err(AppError::coded("profile_export_artifact_count_invalid"));
    }
    Ok(expected)
}

fn validate_artifact_sources<'a>(
    expected: &BTreeMap<String, u64>,
    artifacts: &'a [ProfileExportArtifactSource],
    limits: ProfileArchiveLimits,
) -> AppResult<BTreeMap<String, &'a ProfileExportArtifactSource>> {
    if artifacts.len() != expected.len() || artifacts.len() > MAX_PROFILE_ARTIFACTS {
        return Err(AppError::coded(
            "profile_export_artifact_source_count_mismatch",
        ));
    }
    let mut sources = BTreeMap::new();
    let mut total = 0u64;
    for artifact in artifacts {
        validate_sha256(&artifact.sha256)?;
        let expected_size = expected
            .get(&artifact.sha256)
            .ok_or_else(|| AppError::coded("profile_export_artifact_source_unexpected"))?;
        if artifact.size_bytes != *expected_size {
            return Err(AppError::coded(
                "profile_export_artifact_source_size_mismatch",
            ));
        }
        if artifact.size_bytes > limits.max_artifact_bytes {
            return Err(AppError::coded("profile_export_artifact_size_invalid"));
        }
        if sources.insert(artifact.sha256.clone(), artifact).is_some() {
            return Err(AppError::coded("profile_export_artifact_source_duplicate"));
        }
        total = total
            .checked_add(artifact.size_bytes)
            .ok_or_else(|| AppError::coded("profile_export_size_overflow"))?;
    }
    if total > limits.max_total_uncompressed_bytes {
        return Err(AppError::coded(
            "profile_export_total_uncompressed_too_large",
        ));
    }
    Ok(sources)
}

fn copy_verified_source<W: Write>(
    output: &mut W,
    source: &ProfileExportArtifactSource,
    expected_size: u64,
    expected_sha256: &str,
) -> AppResult<u64> {
    validate_existing_chain(source.source.anchor(), source.source.absolute())?;
    let mut input = File::open(source.source.absolute())?;
    let initial_metadata = input.metadata()?;
    if !initial_metadata.is_file() {
        return Err(AppError::coded(
            "profile_export_artifact_source_not_regular_file",
        ));
    }
    if initial_metadata.len() != expected_size {
        return Err(AppError::coded(
            "profile_export_artifact_source_size_mismatch",
        ));
    }
    let (actual_size, actual_sha256) = copy_and_hash(&mut input, output, expected_size)?;
    if actual_size != expected_size || input.metadata()?.len() != expected_size {
        return Err(AppError::coded(
            "profile_export_artifact_source_size_mismatch",
        ));
    }
    if actual_sha256 != expected_sha256 {
        return Err(AppError::coded(
            "profile_export_artifact_source_hash_mismatch",
        ));
    }
    validate_existing_chain(source.source.anchor(), source.source.absolute())?;
    Ok(actual_size)
}

fn copy_and_hash<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    maximum_size: u64,
) -> AppResult<(u64, String)> {
    let mut buffer = [0u8; IO_BUFFER_BYTES];
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| AppError::coded("profile_export_size_overflow"))?;
        if total > maximum_size {
            return Err(AppError::coded("profile_export_artifact_too_large"));
        }
        hasher.update(&buffer[..read]);
        output.write_all(&buffer[..read])?;
    }
    Ok((total, hex::encode(hasher.finalize())))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveEntryKind {
    Manifest,
    Artifact,
}

#[derive(Debug, Clone)]
struct ArchiveEntryInventory {
    name: String,
    kind: ArchiveEntryKind,
    size: u64,
}

#[derive(Debug)]
struct ArchiveInventory {
    entries: BTreeMap<String, ArchiveEntryInventory>,
    manifest_name: String,
}

fn inspect_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    limits: ProfileArchiveLimits,
) -> AppResult<ArchiveInventory> {
    if archive.offset() != 0 || !archive.comment().is_empty() || archive.zip64_comment().is_some() {
        return Err(AppError::coded("profile_export_archive_metadata_forbidden"));
    }
    if archive.is_empty() || archive.len() > limits.max_entries {
        return Err(AppError::coded(
            "profile_export_archive_entry_count_invalid",
        ));
    }
    let mut entries = BTreeMap::new();
    let mut seen_collision_keys = BTreeMap::<String, String>::new();
    let mut manifest_name = None;
    let mut total_uncompressed = 0u64;

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::coded("profile_export_archive_invalid"))?;
        if entry.encrypted() {
            return Err(AppError::coded(
                "profile_export_archive_encrypted_entry_forbidden",
            ));
        }
        if !entry.comment().is_empty() || entry.extra_data().is_some_and(|extra| !extra.is_empty())
        {
            return Err(AppError::coded(
                "profile_export_archive_entry_metadata_forbidden",
            ));
        }
        let raw_name = std::str::from_utf8(entry.name_raw())
            .map_err(|_| AppError::coded("profile_export_archive_entry_name_invalid_utf8"))?;
        let normalized = validate_archive_entry_path(raw_name, entry.is_dir())?;
        validate_archive_entry_type(entry.unix_mode(), entry.is_dir())?;
        validate_compression_method(entry.compression())?;

        let collision = collision_key(&normalized)?;
        if let Some(first) = seen_collision_keys.insert(collision.clone(), raw_name.into()) {
            return Err(AppError::coded_with(
                "profile_export_archive_entry_collision",
                [
                    ("firstPath", first),
                    ("secondPath", raw_name.to_string()),
                    ("normalizedPath", collision),
                ],
            ));
        }

        let kind = classify_archive_entry(raw_name)?;
        let maximum = match kind {
            ArchiveEntryKind::Manifest => limits.max_manifest_bytes,
            ArchiveEntryKind::Artifact => limits.max_artifact_bytes,
        };
        validate_archive_entry_size(
            entry.compressed_size(),
            entry.size(),
            maximum,
            limits.max_compression_ratio,
        )?;
        total_uncompressed = total_uncompressed
            .checked_add(entry.size())
            .ok_or_else(|| AppError::coded("profile_export_size_overflow"))?;
        if total_uncompressed > limits.max_total_uncompressed_bytes + limits.max_manifest_bytes {
            return Err(AppError::coded(
                "profile_export_total_uncompressed_too_large",
            ));
        }

        if kind == ArchiveEntryKind::Manifest
            && manifest_name.replace(raw_name.to_string()).is_some()
        {
            return Err(AppError::coded("profile_export_archive_manifest_duplicate"));
        }
        entries.insert(
            raw_name.to_string(),
            ArchiveEntryInventory {
                name: raw_name.to_string(),
                kind,
                size: entry.size(),
            },
        );
    }

    let manifest_name =
        manifest_name.ok_or_else(|| AppError::coded("profile_export_archive_manifest_missing"))?;
    Ok(ArchiveInventory {
        entries,
        manifest_name,
    })
}

fn validate_archive_entry_path(raw_name: &str, is_directory: bool) -> AppResult<PathBuf> {
    if raw_name.contains('\\') {
        return Err(AppError::coded(
            "profile_export_archive_separator_noncanonical",
        ));
    }
    if !raw_name.is_ascii() {
        return Err(AppError::coded(
            "profile_export_archive_entry_name_nonascii",
        ));
    }
    if is_directory || raw_name.ends_with('/') {
        return Err(AppError::coded(
            "profile_export_archive_directory_forbidden",
        ));
    }
    let normalized = normalize_relative_path(Path::new(raw_name))?;
    let canonical = normalized
        .iter()
        .map(|component| component.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if canonical != raw_name {
        return Err(AppError::coded("profile_export_archive_path_noncanonical"));
    }
    Ok(normalized)
}

fn validate_archive_entry_type(mode: Option<u32>, is_directory: bool) -> AppResult<()> {
    if is_directory {
        return Err(AppError::coded(
            "profile_export_archive_directory_forbidden",
        ));
    }
    let Some(mode) = mode else {
        return Ok(());
    };
    match mode & 0o170000 {
        0 | 0o100000 => Ok(()),
        0o120000 => Err(AppError::coded("profile_export_archive_symlink_forbidden")),
        _ => Err(AppError::coded(
            "profile_export_archive_special_entry_forbidden",
        )),
    }
}

fn validate_compression_method(method: CompressionMethod) -> AppResult<()> {
    if matches!(
        method,
        CompressionMethod::Stored | CompressionMethod::Deflated
    ) {
        Ok(())
    } else {
        Err(AppError::coded(
            "profile_export_archive_compression_unsupported",
        ))
    }
}

fn validate_archive_entry_size(
    compressed: u64,
    uncompressed: u64,
    maximum: u64,
    max_compression_ratio: u64,
) -> AppResult<()> {
    if uncompressed > maximum {
        return Err(AppError::coded("profile_export_archive_entry_size_invalid"));
    }
    if uncompressed > 0
        && (compressed == 0 || uncompressed > compressed.saturating_mul(max_compression_ratio))
    {
        return Err(AppError::coded(
            "profile_export_archive_compression_ratio_exceeded",
        ));
    }
    Ok(())
}

fn classify_archive_entry(raw_name: &str) -> AppResult<ArchiveEntryKind> {
    if raw_name == PROFILE_EXPORT_MANIFEST_ENTRY {
        return Ok(ArchiveEntryKind::Manifest);
    }
    let Some(sha256) = raw_name.strip_prefix(PROFILE_EXPORT_ARTIFACT_PREFIX) else {
        return Err(AppError::coded("profile_export_archive_entry_unsupported"));
    };
    validate_sha256(sha256)?;
    Ok(ArchiveEntryKind::Artifact)
}

fn read_manifest<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    inventory: &ArchiveInventory,
    limits: ProfileArchiveLimits,
) -> AppResult<Vec<u8>> {
    let mut entry = archive
        .by_name(&inventory.manifest_name)
        .map_err(|_| AppError::coded("profile_export_archive_manifest_missing"))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .by_ref()
        .take(limits.max_manifest_bytes + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty()
        || bytes.len() as u64 != entry.size()
        || bytes.len() as u64 > limits.max_manifest_bytes
    {
        return Err(AppError::coded("profile_export_manifest_size_invalid"));
    }
    Ok(bytes)
}

fn validate_inventory_binding(
    inventory: &ArchiveInventory,
    expected: &BTreeMap<String, u64>,
) -> AppResult<()> {
    let mut actual = BTreeMap::new();
    for entry in inventory.entries.values() {
        if entry.kind != ArchiveEntryKind::Artifact {
            continue;
        }
        let sha256 = entry
            .name
            .strip_prefix(PROFILE_EXPORT_ARTIFACT_PREFIX)
            .ok_or_else(|| AppError::coded("profile_export_archive_entry_unsupported"))?;
        actual.insert(sha256.to_string(), entry.size);
    }
    if actual.keys().collect::<BTreeSet<_>>() != expected.keys().collect::<BTreeSet<_>>() {
        return Err(AppError::coded(
            "profile_export_archive_artifact_set_mismatch",
        ));
    }
    for (sha256, expected_size) in expected {
        if actual.get(sha256) != Some(expected_size) {
            return Err(AppError::coded(
                "profile_export_archive_artifact_size_mismatch",
            ));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str) -> AppResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::coded("profile_export_sha256_invalid"));
    }
    Ok(())
}

fn validate_limits(limits: ProfileArchiveLimits) -> AppResult<()> {
    if limits.max_entries == 0
        || limits.max_manifest_bytes == 0
        || limits.max_artifact_bytes == 0
        || limits.max_total_uncompressed_bytes == 0
        || limits.max_archive_bytes == 0
        || limits.max_compression_ratio == 0
        || limits.max_artifact_bytes > limits.max_total_uncompressed_bytes
    {
        return Err(AppError::coded("profile_export_limits_invalid"));
    }
    Ok(())
}

fn hash_reader<R: Read>(reader: &mut R, expected_size: u64) -> AppResult<String> {
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; IO_BUFFER_BYTES];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| AppError::coded("profile_export_size_overflow"))?;
        if total > expected_size {
            return Err(AppError::coded(
                "profile_export_archive_changed_during_read",
            ));
        }
        hasher.update(&buffer[..read]);
    }
    if total != expected_size {
        return Err(AppError::coded(
            "profile_export_archive_changed_during_read",
        ));
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_file(path: &Path, expected_size: u64) -> AppResult<String> {
    let mut file = File::open(path)?;
    hash_reader(&mut file, expected_size)
}

fn ensure_path_absent(path: &Path, error_code: &'static str) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(AppError::coded(error_code)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

struct TemporaryFile {
    path: PathBuf,
    file: File,
}

fn create_temporary_file(destination: &SecurePath) -> AppResult<TemporaryFile> {
    let parent = destination
        .absolute()
        .parent()
        .ok_or_else(|| AppError::coded("profile_export_target_parent_missing"))?;
    let destination_name_units = destination
        .absolute()
        .file_name()
        .ok_or_else(|| AppError::coded("profile_export_target_name_missing"))?
        .to_string_lossy()
        .encode_utf16()
        .count();
    if destination_name_units == 0 {
        return Err(AppError::coded("profile_export_target_name_missing"));
    }
    validate_existing_chain(destination.anchor(), parent)?;
    for _ in 0..32 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(temporary_file_name(destination_name_units, counter));
        if !path.starts_with(destination.root()) {
            return Err(AppError::coded("path_outside_registered_root"));
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                validate_existing_chain(destination.anchor(), &path)?;
                return Ok(TemporaryFile { path, file });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::coded("profile_export_temporary_name_exhausted"))
}

fn temporary_file_name(maximum_units: usize, counter: u64) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let width = maximum_units.min(24);
    let mut value = counter;
    let mut bytes = vec![b'x'; width];
    for byte in bytes.iter_mut().rev() {
        *byte = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(bytes).expect("temporary filename alphabet is ASCII")
}

struct FileCleanup {
    path: PathBuf,
    armed: bool,
}

impl FileCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FileCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

struct StagingCleanup {
    path: SecurePath,
    armed: bool,
}

impl StagingCleanup {
    fn new(path: SecurePath) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = secure_fs::remove_tree(&self.path);
        }
    }
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent(_: &Path) -> AppResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::{
            content_lock_sha256, ContentKind, ContentVersionRequirement, ResolvedContentItemV1,
            ResolvedContentOverrideV1, CONTENT_LOCK_FORMAT, CONTENT_LOCK_FORMAT_VERSION,
        },
        profiles::model::S9labComponentSelection,
        runtime::{JavaPolicy, LoaderKind, LoaderSelection},
        security::RegisteredRoot,
    };
    use std::io::Cursor;

    struct Fixture {
        root: PathBuf,
        registry: PathRegistry,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "s9lab-profile-format-{}-{}",
                std::process::id(),
                TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let sources = root.join("sources");
            let exports = root.join("exports");
            let staging = root.join("staging");
            fs::create_dir_all(&sources).expect("source root");
            fs::create_dir_all(&exports).expect("export root");
            fs::create_dir_all(&staging).expect("staging root");
            let registry = PathRegistry::new(
                &root,
                [
                    RegisteredRoot {
                        id: "sources".into(),
                        path: sources,
                    },
                    RegisteredRoot {
                        id: "exports".into(),
                        path: exports,
                    },
                    RegisteredRoot {
                        id: "staging".into(),
                        path: staging,
                    },
                ],
            )
            .expect("path registry");
            Self { root, registry }
        }

        fn write_source(&self, name: &str, bytes: &[u8]) -> SecurePath {
            let path = self.registry.resolve("sources", name).expect("source path");
            secure_fs::write_new(&path, bytes).expect("source bytes");
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("remove fixture");
        }
    }

    fn runtime() -> ProfileRuntimeIntent {
        ProfileRuntimeIntent {
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
            java: JavaPolicy::System { major_version: 21 },
        }
    }

    fn document(artifact: &[u8]) -> ProfileExportV1 {
        let sha256 = hex::encode(Sha256::digest(artifact));
        let selection = ContentSelection {
            content_id: "example-mod".into(),
            version: ContentVersionRequirement::Exact {
                version: "1.0.0".into(),
            },
            enabled: true,
        };
        let mut lock = ResolvedContentLockV1 {
            format: CONTENT_LOCK_FORMAT.into(),
            format_version: CONTENT_LOCK_FORMAT_VERSION,
            runtime: ContentTargetRuntime {
                minecraft_version: "1.21.1".into(),
                loader: runtime().loader,
            },
            include_optional_dependencies: false,
            requested: vec![selection.clone()],
            items: vec![ResolvedContentItemV1 {
                content_id: "example-mod".into(),
                version: "1.0.0".into(),
                kind: ContentKind::Mod,
                enabled: true,
                source: None,
                relative_target: "mods/example-mod.jar".into(),
                sha256,
                size_bytes: artifact.len() as u64,
                dependencies: vec![],
            }],
            pack_members: Vec::new(),
            overrides: Vec::new(),
            resolution_sha256: String::new(),
        };
        lock.resolution_sha256 = content_lock_sha256(&lock).expect("lock hash");
        ProfileExportV1::new(
            "Clean profile",
            runtime(),
            S9labComponentSelection::Catalog {
                component_id: "s9lab-client".into(),
                component_version: "1.0.8".into(),
            },
            vec![selection],
            Some(lock),
        )
    }

    fn document_with_empty_override(pack_artifact: &[u8]) -> ProfileExportV1 {
        let pack_selection = ContentSelection {
            content_id: "example-pack".into(),
            version: ContentVersionRequirement::Exact {
                version: "1.0.0".into(),
            },
            enabled: true,
        };
        let mut lock = ResolvedContentLockV1 {
            format: CONTENT_LOCK_FORMAT.into(),
            format_version: CONTENT_LOCK_FORMAT_VERSION,
            runtime: ContentTargetRuntime {
                minecraft_version: "1.21.1".into(),
                loader: runtime().loader,
            },
            include_optional_dependencies: false,
            requested: vec![pack_selection.clone()],
            items: vec![ResolvedContentItemV1 {
                content_id: "example-pack".into(),
                version: "1.0.0".into(),
                kind: ContentKind::Modpack,
                enabled: true,
                source: None,
                relative_target: "modpacks/example-pack.mrpack".into(),
                sha256: hex::encode(Sha256::digest(pack_artifact)),
                size_bytes: pack_artifact.len() as u64,
                dependencies: vec![],
            }],
            pack_members: Vec::new(),
            overrides: vec![ResolvedContentOverrideV1 {
                pack_content_id: "example-pack".into(),
                relative_target: "config/empty.toml".into(),
                sha256: hex::encode(Sha256::digest([])),
                size_bytes: 0,
            }],
            resolution_sha256: String::new(),
        };
        lock.resolution_sha256 = content_lock_sha256(&lock).expect("lock hash");
        ProfileExportV1::new(
            "Pack with empty override",
            runtime(),
            S9labComponentSelection::Disabled,
            vec![pack_selection],
            Some(lock),
        )
    }

    fn source_descriptor(source: SecurePath, bytes: &[u8]) -> ProfileExportArtifactSource {
        ProfileExportArtifactSource {
            sha256: hex::encode(Sha256::digest(bytes)),
            size_bytes: bytes.len() as u64,
            source,
        }
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected profile format error")
            .descriptor()
            .code
    }

    fn raw_archive(entries: &[(&str, &[u8])], compression: CompressionMethod) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(compression);
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start entry");
            writer.write_all(bytes).expect("write entry");
        }
        writer.finish().expect("finish ZIP").into_inner()
    }

    #[test]
    fn temporary_names_stay_within_the_registered_target_name_budget() {
        for width in 1..=24 {
            let names = (0..32)
                .map(|counter| temporary_file_name(width, counter))
                .collect::<BTreeSet<_>>();
            assert_eq!(names.len(), 32);
            assert!(names
                .iter()
                .all(|name| name.encode_utf16().count() <= width));
        }
    }

    fn import_raw(
        fixture: &Fixture,
        bytes: &[u8],
        staging_name: &str,
    ) -> AppResult<ImportedProfileExportV1> {
        let source = fixture.write_source(&format!("{staging_name}.s9profile"), bytes);
        import_profile_v1(&source, &fixture.registry, "staging", staging_name)
    }

    #[test]
    fn round_trip_is_deduplicated_hash_bound_and_secret_free() {
        let fixture = Fixture::new();
        let artifact = b"verified portable artifact";
        let document = document(artifact);
        let source = fixture.write_source("example.jar", artifact);
        let destination = fixture
            .registry
            .resolve("exports", "profile.s9profile")
            .expect("destination");
        let summary = export_profile_v1(
            &destination,
            &document,
            &[source_descriptor(source, artifact)],
        )
        .expect("export profile");
        assert_eq!(summary.artifact_count, 1);
        assert_eq!(summary.total_artifact_bytes, artifact.len() as u64);

        let imported = import_profile_v1(&destination, &fixture.registry, "staging", "round-trip")
            .expect("import profile");
        assert_eq!(imported.document, document);
        assert_eq!(imported.artifacts.len(), 1);
        assert_eq!(
            fs::read(imported.artifacts[0].staged_path.absolute()).unwrap(),
            artifact
        );
        assert_eq!(imported.archive_sha256, summary.archive_sha256);

        let manifest = fs::read_to_string(imported.staged_manifest.absolute()).unwrap();
        for forbidden in [
            "accountId",
            "accessToken",
            "refreshToken",
            "absolutePath",
            "rawUrl",
            "logs",
            "worlds",
        ] {
            assert!(!manifest.contains(forbidden), "forbidden field {forbidden}");
        }
    }

    #[test]
    fn round_trip_binds_modpack_overrides_including_an_empty_artifact() {
        let fixture = Fixture::new();
        let pack_artifact = b"verified mrpack container";
        let empty_override = b"";
        let document = document_with_empty_override(pack_artifact);
        let pack_source = fixture.write_source("example-pack.mrpack", pack_artifact);
        let empty_source = fixture.write_source("empty.toml", empty_override);
        let destination = fixture
            .registry
            .resolve("exports", "pack-with-override.s9profile")
            .expect("destination");

        assert_eq!(
            error_code(export_profile_v1(
                &destination,
                &document,
                &[source_descriptor(pack_source.clone(), pack_artifact)],
            )),
            "profile_export_artifact_source_count_mismatch"
        );

        let summary = export_profile_v1(
            &destination,
            &document,
            &[
                source_descriptor(pack_source, pack_artifact),
                source_descriptor(empty_source, empty_override),
            ],
        )
        .expect("export profile with empty override");
        assert_eq!(summary.artifact_count, 2);
        assert_eq!(summary.total_artifact_bytes, pack_artifact.len() as u64);

        let imported = import_profile_v1(
            &destination,
            &fixture.registry,
            "staging",
            "pack-round-trip",
        )
        .expect("import profile with empty override");
        assert_eq!(imported.document, document);
        assert_eq!(imported.artifacts.len(), 2);
        let empty_sha256 = hex::encode(Sha256::digest([]));
        let imported_empty = imported
            .artifacts
            .iter()
            .find(|artifact| artifact.sha256 == empty_sha256)
            .expect("empty override artifact");
        assert_eq!(imported_empty.size_bytes, 0);
        assert!(fs::read(imported_empty.staged_path.absolute())
            .expect("read staged empty override")
            .is_empty());
    }

    #[test]
    fn export_rechecks_declared_size_hash_and_hardlink_safety() {
        let fixture = Fixture::new();
        let artifact = b"expected artifact";
        let document = document(artifact);
        let source = fixture.write_source("tampered.jar", b"tampered artifact");
        let destination = fixture
            .registry
            .resolve("exports", "tampered.s9profile")
            .unwrap();
        assert_eq!(
            error_code(export_profile_v1(
                &destination,
                &document,
                &[ProfileExportArtifactSource {
                    sha256: hex::encode(Sha256::digest(artifact)),
                    size_bytes: artifact.len() as u64,
                    source,
                }],
            )),
            "profile_export_artifact_source_hash_mismatch"
        );
        assert!(!destination.absolute().exists());

        let source = fixture.write_source("wrong-size.jar", b"short");
        assert_eq!(
            error_code(export_profile_v1(
                &destination,
                &document,
                &[ProfileExportArtifactSource {
                    sha256: hex::encode(Sha256::digest(artifact)),
                    size_bytes: artifact.len() as u64,
                    source,
                }],
            )),
            "profile_export_artifact_source_size_mismatch"
        );
        assert!(!destination.absolute().exists());

        let source = fixture.write_source("hardlink-source.jar", artifact);
        fs::hard_link(
            source.absolute(),
            fixture.root.join("sources/hardlink-alias.jar"),
        )
        .expect("hardlink fixture");
        assert_eq!(
            error_code(export_profile_v1(
                &destination,
                &document,
                &[source_descriptor(source, artifact)],
            )),
            "path_hardlink_forbidden"
        );
    }

    #[test]
    fn import_rejects_unknown_manifest_fields_without_creating_staging() {
        let fixture = Fixture::new();
        let artifact = b"artifact";
        let document = document(artifact);
        let mut value = serde_json::to_value(&document).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("accountId".into(), serde_json::json!("secret-account"));
        let manifest = serde_json::to_vec(&value).unwrap();
        let sha256 = hex::encode(Sha256::digest(artifact));
        let archive = raw_archive(
            &[
                (PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice()),
                (&format!("artifacts/{sha256}"), artifact),
            ],
            CompressionMethod::Stored,
        );
        assert_eq!(
            error_code(import_raw(&fixture, &archive, "unknown-field")),
            "profile_export_manifest_invalid"
        );
        assert!(!fixture.root.join("staging/unknown-field").exists());
    }

    #[test]
    fn import_rejects_traversal_ads_ambiguous_separators_and_extra_entries() {
        let fixture = Fixture::new();
        let manifest = serde_json::to_vec(&ProfileExportV1::new(
            "Empty",
            runtime(),
            S9labComponentSelection::Disabled,
            vec![],
            None,
        ))
        .unwrap();
        for (index, (name, expected)) in [
            ("../escape", "path_traversal"),
            ("profile.json:ads", "path_alternate_data_stream"),
            (
                "artifacts\\aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "profile_export_archive_separator_noncanonical",
            ),
            ("extra.txt", "profile_export_archive_entry_unsupported"),
        ]
        .into_iter()
        .enumerate()
        {
            let archive = raw_archive(
                &[
                    (PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice()),
                    (name, b"x"),
                ],
                CompressionMethod::Stored,
            );
            assert_eq!(
                error_code(import_raw(&fixture, &archive, &format!("invalid-{index}"))),
                expected
            );
        }
    }

    #[test]
    fn import_rejects_case_collisions_non_ascii_and_special_entry_modes() {
        let fixture = Fixture::new();
        let manifest = serde_json::to_vec(&ProfileExportV1::new(
            "Empty",
            runtime(),
            S9labComponentSelection::Disabled,
            vec![],
            None,
        ))
        .unwrap();
        let collision = raw_archive(
            &[
                (PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice()),
                ("PROFILE.JSON", manifest.as_slice()),
            ],
            CompressionMethod::Stored,
        );
        assert_eq!(
            error_code(import_raw(&fixture, &collision, "case-collision")),
            "profile_export_archive_entry_collision"
        );

        let unicode = raw_archive(
            &[
                (PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice()),
                ("artifacts/\u{e9}", b"x"),
            ],
            CompressionMethod::Stored,
        );
        assert_eq!(
            error_code(import_raw(&fixture, &unicode, "unicode")),
            "profile_export_archive_entry_name_nonascii"
        );
        assert_eq!(
            error_code(validate_archive_entry_type(Some(0o120777), false)),
            "profile_export_archive_symlink_forbidden"
        );
        assert_eq!(
            error_code(validate_archive_entry_type(Some(0o060000), false)),
            "profile_export_archive_special_entry_forbidden"
        );
    }

    #[test]
    fn import_rejects_polyglot_prefixes_and_zip_comments() {
        let fixture = Fixture::new();
        let manifest = serde_json::to_vec(&ProfileExportV1::new(
            "Empty",
            runtime(),
            S9labComponentSelection::Disabled,
            vec![],
            None,
        ))
        .unwrap();
        let archive = raw_archive(
            &[(PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice())],
            CompressionMethod::Stored,
        );
        let mut prefixed = b"untrusted-prefix".to_vec();
        prefixed.extend_from_slice(&archive);
        assert_eq!(
            error_code(import_raw(&fixture, &prefixed, "prefixed")),
            "profile_export_archive_metadata_forbidden"
        );

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer.set_comment("hidden metadata");
        writer
            .start_file(
                PROFILE_EXPORT_MANIFEST_ENTRY,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(&manifest).unwrap();
        let commented = writer.finish().unwrap().into_inner();
        assert_eq!(
            error_code(import_raw(&fixture, &commented, "commented")),
            "profile_export_archive_metadata_forbidden"
        );
    }

    #[test]
    fn import_rejects_zip_bomb_ratio_and_artifact_hash_tampering() {
        let fixture = Fixture::new();
        let repeated = vec![0u8; 2 * 1024 * 1024];
        let bomb_document = document(&repeated);
        let manifest = serde_json::to_vec(&bomb_document).unwrap();
        let sha256 = hex::encode(Sha256::digest(&repeated));
        let bomb = raw_archive(
            &[
                (PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice()),
                (&format!("artifacts/{sha256}"), repeated.as_slice()),
            ],
            CompressionMethod::Deflated,
        );
        assert_eq!(
            error_code(import_raw(&fixture, &bomb, "zip-bomb")),
            "profile_export_archive_compression_ratio_exceeded"
        );

        let expected = b"expected artifact";
        let tampered = b"tampered artifact";
        assert_eq!(expected.len(), tampered.len());
        let document = document(expected);
        let manifest = serde_json::to_vec(&document).unwrap();
        let sha256 = hex::encode(Sha256::digest(expected));
        let archive = raw_archive(
            &[
                (PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice()),
                (&format!("artifacts/{sha256}"), tampered),
            ],
            CompressionMethod::Stored,
        );
        assert_eq!(
            error_code(import_raw(&fixture, &archive, "hash-tampered")),
            "profile_export_artifact_hash_mismatch"
        );
        assert!(!fixture.root.join("staging/hash-tampered").exists());
    }

    #[test]
    fn import_requires_exact_artifact_set_and_absent_staging_target() {
        let fixture = Fixture::new();
        let artifact = b"artifact";
        let document = document(artifact);
        let manifest = serde_json::to_vec(&document).unwrap();
        let missing = raw_archive(
            &[(PROFILE_EXPORT_MANIFEST_ENTRY, manifest.as_slice())],
            CompressionMethod::Stored,
        );
        assert_eq!(
            error_code(import_raw(&fixture, &missing, "missing-artifact")),
            "profile_export_archive_artifact_set_mismatch"
        );

        let source = fixture.write_source("valid-source.jar", artifact);
        let destination = fixture
            .registry
            .resolve("exports", "valid.s9profile")
            .unwrap();
        export_profile_v1(
            &destination,
            &document,
            &[source_descriptor(source, artifact)],
        )
        .unwrap();
        fs::create_dir(fixture.root.join("staging/existing")).unwrap();
        fs::write(fixture.root.join("staging/existing/keep.txt"), b"keep").unwrap();
        assert_eq!(
            error_code(import_profile_v1(
                &destination,
                &fixture.registry,
                "staging",
                "existing",
            )),
            "profile_export_import_staging_exists"
        );
        assert_eq!(
            fs::read(fixture.root.join("staging/existing/keep.txt")).unwrap(),
            b"keep"
        );
    }

    #[test]
    fn rejects_runtime_lock_mismatch_and_noncanonical_component_tokens() {
        let mut invalid_document = ProfileExportV1::new(
            "Portable",
            runtime(),
            S9labComponentSelection::Catalog {
                component_id: concat!("https", "://invalid.example").into(),
                component_version: "1.0.8".into(),
            },
            vec![],
            None,
        );
        assert_eq!(
            error_code(validate_profile_export(&invalid_document)),
            "profile_export_component_id_invalid"
        );
        invalid_document.s9lab_component = S9labComponentSelection::Disabled;
        invalid_document.display_name = " leading".into();
        assert_eq!(
            error_code(validate_profile_export(&invalid_document)),
            "profile_export_display_name_invalid"
        );

        let artifact = b"artifact";
        let mut document = document(artifact);
        document.runtime.minecraft_version = "1.20.1".into();
        assert_eq!(
            error_code(validate_profile_export(&document)),
            "profile_export_content_runtime_incompatible"
        );
    }
}
