use crate::{
    content::{
        validate_content_override_target, validate_resolved_content_lock, ContentKind,
        ResolvedContentLockV1, MAX_PROJECTED_CONTENT_ITEMS,
    },
    error::{AppError, AppResult},
    operations::model::new_identifier,
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
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

const PROJECTION_FORMAT: &str = "s9lab-content-projection";
const PROJECTION_FORMAT_VERSION: u32 = 2;
const MAX_MARKER_BYTES: u64 = 64 * 1024 * 1024;

static CONTENT_PROJECTION_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContentProjectionMarkerV1 {
    format: String,
    format_version: u32,
    profile_id: String,
    revision_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_lock_sha256: Option<String>,
    items: Vec<ProjectedContentItemV1>,
    state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectedContentItemV1 {
    content_id: String,
    kind: ContentKind,
    #[serde(default)]
    is_override: bool,
    relative_target: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct DesiredProjection {
    marker: ContentProjectionMarkerV1,
    sources: BTreeMap<String, SecurePath>,
    targets: BTreeMap<String, SecurePath>,
}

#[derive(Debug, Clone)]
struct ExistingProjectionEntry {
    relative_target: String,
    is_file: bool,
}

#[derive(Debug, Clone)]
struct MoveRecord {
    source: SecurePath,
    destination: SecurePath,
}

trait ProjectionCheckpoint {
    fn after_target_activated(&self, _activated_targets: usize) -> AppResult<()> {
        Ok(())
    }
}

struct NoProjectionFailure;

impl ProjectionCheckpoint for NoProjectionFailure {}

/// Reconciles the launch-time content directories with an immutable profile
/// revision. Only files recorded in the validated, revision-bound projection
/// marker are ever replaced or removed; every other file remains user-owned.
pub fn project_content_for_launch(
    registry: &PathRegistry,
    profile_id: &str,
    revision_id: &str,
    lock: Option<&ResolvedContentLockV1>,
) -> AppResult<()> {
    let _guard = acquire_projection_lock()?;
    project_content_for_launch_locked(
        registry,
        profile_id,
        revision_id,
        lock,
        &NoProjectionFailure,
    )
}

/// Returns only immutable files owned by the validated projection marker.
/// Profile duplication uses this list to copy mutable user state without
/// carrying source-profile markers or revision-bound content into the clone.
pub(crate) fn immutable_projected_targets_for_duplicate(
    registry: &PathRegistry,
    profile_id: &str,
) -> AppResult<BTreeSet<String>> {
    validate_identifier(profile_id)?;
    let marker_path = registry.resolve("profiles", marker_relative(profile_id))?;
    Ok(read_previous_marker(registry, profile_id, &marker_path)?
        .into_iter()
        .flat_map(|marker| marker.items)
        .filter(|item| !item.is_override)
        .map(|item| item.relative_target)
        .collect())
}

fn project_content_for_launch_locked(
    registry: &PathRegistry,
    profile_id: &str,
    revision_id: &str,
    lock: Option<&ResolvedContentLockV1>,
    checkpoint: &dyn ProjectionCheckpoint,
) -> AppResult<()> {
    validate_identifier(profile_id)?;
    validate_identifier(revision_id)?;

    let marker_path = registry.resolve("profiles", marker_relative(profile_id))?;
    let previous_marker = read_previous_marker(registry, profile_id, &marker_path)?;
    let desired = build_desired_projection(registry, profile_id, revision_id, lock)?;

    // The SNine one-button profile normally has no launcher-managed content lock;
    // its client/support mods are managed by snine_client_delivery instead. The
    // old code still created, backed up and rewrote an empty projection marker on
    // every Play click. If the immutable desired marker is already committed and
    // contains no managed items, there is literally nothing to reconcile.
    if desired.marker.items.is_empty()
        && previous_marker
            .as_ref()
            .is_some_and(|previous| previous == &desired.marker)
    {
        eprintln!("[snine-launch-fast] content projection no-op cache hit");
        return Ok(());
    }

    let existing_entries = scan_projection_directories(
        registry,
        profile_id,
        previous_marker.as_ref(),
        &desired.marker,
    )?;
    validate_previous_projection(
        registry,
        profile_id,
        previous_marker.as_ref(),
        &existing_entries,
    )?;

    validate_foreign_conflicts(previous_marker.as_ref(), &desired.marker, &existing_entries)?;
    verify_desired_sources(&desired)?;

    apply_projection_transaction(
        registry,
        profile_id,
        &marker_path,
        previous_marker.as_ref(),
        &desired,
        checkpoint,
    )
}

fn acquire_projection_lock() -> AppResult<MutexGuard<'static, ()>> {
    CONTENT_PROJECTION_LOCK
        .lock()
        .map_err(|_| AppError::coded("content_projection_lock_poisoned"))
}

fn build_desired_projection(
    registry: &PathRegistry,
    profile_id: &str,
    revision_id: &str,
    lock: Option<&ResolvedContentLockV1>,
) -> AppResult<DesiredProjection> {
    if let Some(lock) = lock {
        validate_resolved_content_lock(lock)?;
    }

    let mut items = lock
        .into_iter()
        .flat_map(|lock| lock.items.iter())
        .filter(|item| item.enabled && item.kind != ContentKind::Modpack)
        .map(|item| ProjectedContentItemV1 {
            content_id: item.content_id.clone(),
            kind: item.kind,
            is_override: false,
            relative_target: item.relative_target.clone(),
            sha256: item.sha256.clone(),
            size_bytes: item.size_bytes,
        })
        .collect::<Vec<_>>();
    if let Some(lock) = lock {
        let enabled_packs = lock
            .items
            .iter()
            .filter(|item| item.kind == ContentKind::Modpack && item.enabled)
            .map(|item| item.content_id.as_str())
            .collect::<BTreeSet<_>>();
        items.extend(
            lock.overrides
                .iter()
                .filter(|override_file| {
                    enabled_packs.contains(override_file.pack_content_id.as_str())
                })
                .map(|override_file| ProjectedContentItemV1 {
                    content_id: override_file.pack_content_id.clone(),
                    kind: ContentKind::Modpack,
                    is_override: true,
                    relative_target: override_file.relative_target.clone(),
                    sha256: override_file.sha256.clone(),
                    size_bytes: override_file.size_bytes,
                }),
        );
    }
    items.sort_by(|left, right| left.relative_target.cmp(&right.relative_target));

    let mut marker = ContentProjectionMarkerV1 {
        format: PROJECTION_FORMAT.into(),
        format_version: PROJECTION_FORMAT_VERSION,
        profile_id: profile_id.to_string(),
        revision_id: revision_id.to_string(),
        content_lock_sha256: lock.map(|lock| lock.resolution_sha256.clone()),
        items,
        state_sha256: String::new(),
    };
    marker.state_sha256 = marker_state_sha256(&marker);
    validate_marker(&marker, profile_id)?;

    let source_relatives = marker
        .items
        .iter()
        .map(|item| revision_content_relative(profile_id, revision_id, &item.relative_target))
        .collect::<Vec<_>>();
    let target_relatives = marker
        .items
        .iter()
        .map(|item| instance_content_relative(profile_id, &item.relative_target))
        .collect::<Vec<_>>();
    let source_paths = registry.validate_unique("profiles", &source_relatives)?;
    let target_paths = registry.validate_unique("profiles", &target_relatives)?;

    let mut sources = BTreeMap::new();
    let mut targets = BTreeMap::new();
    for ((item, source), target) in marker.items.iter().zip(source_paths).zip(target_paths) {
        let key = collision_key(Path::new(&item.relative_target))?;
        sources.insert(key.clone(), source);
        targets.insert(key, target);
    }

    Ok(DesiredProjection {
        marker,
        sources,
        targets,
    })
}

fn read_previous_marker(
    registry: &PathRegistry,
    profile_id: &str,
    marker_path: &SecurePath,
) -> AppResult<Option<ContentProjectionMarkerV1>> {
    if !marker_path.absolute().exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(marker_path.absolute())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_MARKER_BYTES {
        return Err(AppError::coded("content_projection_marker_invalid"));
    }
    validate_existing_chain(marker_path.anchor(), marker_path.absolute())?;
    let bytes = fs::read(marker_path.absolute())?;
    if bytes.len() as u64 != metadata.len() {
        return Err(AppError::coded("content_projection_marker_changed"));
    }
    let marker: ContentProjectionMarkerV1 = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::coded("content_projection_marker_invalid"))?;
    validate_marker(&marker, profile_id)?;

    // Resolving every marker target before a transaction also binds it to the
    // currently registered profile root and re-checks the complete chain.
    for item in &marker.items {
        registry.resolve(
            "profiles",
            instance_content_relative(profile_id, &item.relative_target),
        )?;
    }
    Ok(Some(marker))
}

fn validate_marker(marker: &ContentProjectionMarkerV1, expected_profile_id: &str) -> AppResult<()> {
    if marker.format != PROJECTION_FORMAT
        || marker.format_version != PROJECTION_FORMAT_VERSION
        || marker.profile_id != expected_profile_id
        || marker.items.len() > MAX_PROJECTED_CONTENT_ITEMS
    {
        return Err(AppError::coded("content_projection_marker_invalid"));
    }
    validate_identifier(&marker.profile_id)?;
    validate_identifier(&marker.revision_id)?;
    if let Some(hash) = marker.content_lock_sha256.as_deref() {
        validate_sha256(hash, "content_projection_marker_invalid")?;
    }
    validate_sha256(&marker.state_sha256, "content_projection_marker_invalid")?;

    let mut previous_target = None;
    let mut collision_keys = BTreeSet::new();
    for item in &marker.items {
        validate_content_id(&item.content_id)?;
        let target_valid = if item.is_override {
            item.kind == ContentKind::Modpack
                && validate_content_override_target(&item.relative_target).is_ok()
        } else {
            item.kind != ContentKind::Modpack
                && target_matches_kind(item.kind, &item.relative_target)?
        };
        if (!item.is_override && item.size_bytes == 0) || !target_valid {
            return Err(AppError::coded("content_projection_marker_invalid"));
        }
        validate_sha256(&item.sha256, "content_projection_marker_invalid")?;
        if previous_target
            .as_ref()
            .is_some_and(|previous: &String| previous >= &item.relative_target)
        {
            return Err(AppError::coded("content_projection_marker_invalid"));
        }
        previous_target = Some(item.relative_target.clone());
        let key = collision_key(Path::new(&item.relative_target))?;
        if !collision_keys.insert(key) {
            return Err(AppError::coded("content_projection_marker_invalid"));
        }
    }
    if marker_state_sha256(marker) != marker.state_sha256 {
        return Err(AppError::coded("content_projection_marker_hash_mismatch"));
    }
    Ok(())
}

fn validate_previous_projection(
    registry: &PathRegistry,
    profile_id: &str,
    marker: Option<&ContentProjectionMarkerV1>,
    existing_entries: &BTreeMap<String, ExistingProjectionEntry>,
) -> AppResult<()> {
    let Some(marker) = marker else {
        return Ok(());
    };
    for item in &marker.items {
        let key = collision_key(Path::new(&item.relative_target))?;
        let existing = existing_entries.get(&key);
        if item.is_override && existing.is_none() {
            // MRPACK overrides are mutable seeds. A user or Minecraft may
            // deliberately remove one after its initial projection.
            continue;
        }
        let existing =
            existing.ok_or_else(|| AppError::coded("content_projection_managed_file_missing"))?;
        if existing.relative_target != item.relative_target {
            return Err(AppError::coded("content_projection_managed_path_ambiguous"));
        }
        let path = registry.resolve(
            "profiles",
            instance_content_relative(profile_id, &item.relative_target),
        )?;
        if item.is_override {
            let metadata = fs::symlink_metadata(path.absolute())?;
            if !metadata.is_file() {
                return Err(AppError::coded(
                    "content_projection_override_target_invalid",
                ));
            }
            validate_existing_chain(path.anchor(), path.absolute())?;
        } else {
            verify_file(
                &path,
                item.size_bytes,
                &item.sha256,
                "content_projection_managed_file_mismatch",
            )?;
        }
    }
    Ok(())
}

fn validate_foreign_conflicts(
    previous: Option<&ContentProjectionMarkerV1>,
    desired: &ContentProjectionMarkerV1,
    existing_entries: &BTreeMap<String, ExistingProjectionEntry>,
) -> AppResult<()> {
    let previous_by_key = previous
        .into_iter()
        .flat_map(|marker| marker.items.iter())
        .map(|item| Ok((collision_key(Path::new(&item.relative_target))?, item)))
        .collect::<AppResult<BTreeMap<_, _>>>()?;

    for item in &desired.items {
        let key = collision_key(Path::new(&item.relative_target))?;
        if let Some(existing) = existing_entries.get(&key) {
            if item.is_override {
                if existing.relative_target != item.relative_target || !existing.is_file {
                    return Err(AppError::coded(
                        "content_projection_override_target_invalid",
                    ));
                }
                continue;
            }
            let managed = previous_by_key.get(&key).is_some_and(|previous| {
                !previous.is_override
                    && previous.relative_target == item.relative_target
                    && existing.relative_target == item.relative_target
            });
            if !managed {
                return Err(AppError::coded_with(
                    "content_projection_foreign_target_conflict",
                    [("relativeTarget", item.relative_target.clone())],
                ));
            }
        }
    }
    Ok(())
}

fn verify_desired_sources(desired: &DesiredProjection) -> AppResult<()> {
    for item in &desired.marker.items {
        let key = collision_key(Path::new(&item.relative_target))?;
        let source = desired
            .sources
            .get(&key)
            .ok_or_else(|| AppError::coded("content_projection_source_missing"))?;
        verify_file(
            source,
            item.size_bytes,
            &item.sha256,
            "content_projection_source_mismatch",
        )?;
    }
    Ok(())
}

fn apply_projection_transaction(
    registry: &PathRegistry,
    profile_id: &str,
    marker_path: &SecurePath,
    previous: Option<&ContentProjectionMarkerV1>,
    desired: &DesiredProjection,
    checkpoint: &dyn ProjectionCheckpoint,
) -> AppResult<()> {
    let previous_by_key = previous
        .into_iter()
        .flat_map(|marker| marker.items.iter())
        .map(|item| Ok((collision_key(Path::new(&item.relative_target))?, item)))
        .collect::<AppResult<BTreeMap<_, _>>>()?;
    let desired_by_key = desired
        .marker
        .items
        .iter()
        .map(|item| Ok((collision_key(Path::new(&item.relative_target))?, item)))
        .collect::<AppResult<BTreeMap<_, _>>>()?;

    let transaction_id = new_identifier("content-projection");
    let transaction_relative = PathBuf::from(profile_id)
        .join("instance")
        .join(".s9lab")
        .join(transaction_id);
    let transaction_root = registry.resolve("profiles", &transaction_relative)?;
    if transaction_root.absolute().exists() {
        return Err(AppError::coded("content_projection_transaction_collision"));
    }
    secure_fs::create_directories_within(
        transaction_root.anchor(),
        transaction_root.root(),
        transaction_root.absolute(),
    )?;

    let staged = match stage_desired_projection(
        registry,
        &transaction_relative,
        desired,
        &previous_by_key,
    ) {
        Ok(staged) => staged,
        Err(primary) => {
            let cleanup = secure_fs::remove_tree(&transaction_root);
            return Err(combine_cleanup_error(primary, cleanup));
        }
    };
    let staged_marker = match stage_marker(registry, &transaction_relative, &desired.marker) {
        Ok(marker) => marker,
        Err(primary) => {
            let cleanup = secure_fs::remove_tree(&transaction_root);
            return Err(combine_cleanup_error(primary, cleanup));
        }
    };

    let mut backups = Vec::<MoveRecord>::new();
    let mut activations = Vec::<MoveRecord>::new();
    let activation_result = (|| -> AppResult<()> {
        for (index, (key, old_item)) in previous_by_key.iter().enumerate() {
            if old_item.is_override {
                // Seeded overrides become instance-owned and are never
                // silently deleted or replaced by projection reconciliation.
                continue;
            }
            let unchanged = desired_by_key
                .get(key)
                .is_some_and(|new_item| same_projected_artifact(old_item, new_item));
            if unchanged {
                continue;
            }
            let source = registry.resolve(
                "profiles",
                instance_content_relative(profile_id, &old_item.relative_target),
            )?;
            let destination = registry.resolve(
                "profiles",
                transaction_relative
                    .join("backup")
                    .join(format!("{index:08}.bin")),
            )?;
            secure_fs::rename_new(&source, &destination)?;
            backups.push(MoveRecord {
                source,
                destination,
            });
        }

        if marker_path.absolute().exists() {
            let backup_marker = registry.resolve(
                "profiles",
                transaction_relative.join("backup").join("marker.json"),
            )?;
            secure_fs::rename_new(marker_path, &backup_marker)?;
            backups.push(MoveRecord {
                source: marker_path.clone(),
                destination: backup_marker,
            });
        }

        let mut activated_targets = 0usize;
        for (key, item) in &desired_by_key {
            let destination = desired
                .targets
                .get(key)
                .ok_or_else(|| AppError::coded("content_projection_target_missing"))?;
            if previous_by_key
                .get(key)
                .is_some_and(|old_item| same_projected_artifact(old_item, item))
                || preserve_override_seed(previous_by_key.get(key).copied(), item, destination)
            {
                continue;
            }
            let source = staged
                .get(key)
                .ok_or_else(|| AppError::coded("content_projection_stage_missing"))?;
            secure_fs::rename_new(source, destination)?;
            activations.push(MoveRecord {
                source: source.clone(),
                destination: destination.clone(),
            });
            activated_targets += 1;
            checkpoint.after_target_activated(activated_targets)?;
        }

        secure_fs::rename_new(&staged_marker, marker_path)?;
        activations.push(MoveRecord {
            source: staged_marker.clone(),
            destination: marker_path.clone(),
        });
        Ok(())
    })();

    if let Err(primary) = activation_result {
        let rollback = rollback_moves(&activations, &backups);
        let cleanup = if rollback.is_ok() {
            secure_fs::remove_tree(&transaction_root)
        } else {
            // Preserve the backup tree when rollback itself fails so no
            // remaining recovery material is destroyed.
            Ok(())
        };
        return Err(combine_rollback_error(primary, rollback, cleanup));
    }

    // Projection is committed by the marker rename. Cleanup is deliberately
    // best-effort: surfacing a post-commit cleanup failure would falsely imply
    // that the previous projection had been restored.
    let _ = secure_fs::remove_tree(&transaction_root);
    Ok(())
}

fn stage_desired_projection(
    registry: &PathRegistry,
    transaction_relative: &Path,
    desired: &DesiredProjection,
    previous_by_key: &BTreeMap<String, &ProjectedContentItemV1>,
) -> AppResult<BTreeMap<String, SecurePath>> {
    let mut staged = BTreeMap::new();
    for (index, item) in desired.marker.items.iter().enumerate() {
        let key = collision_key(Path::new(&item.relative_target))?;
        let target = desired
            .targets
            .get(&key)
            .ok_or_else(|| AppError::coded("content_projection_target_missing"))?;
        if previous_by_key
            .get(&key)
            .is_some_and(|old_item| same_projected_artifact(old_item, item))
            || preserve_override_seed(previous_by_key.get(&key).copied(), item, target)
        {
            continue;
        }
        let source = desired
            .sources
            .get(&key)
            .ok_or_else(|| AppError::coded("content_projection_source_missing"))?;
        let destination = registry.resolve(
            "profiles",
            transaction_relative
                .join("staged")
                .join(format!("{index:08}.bin")),
        )?;
        let copied = secure_fs::copy_new(source, &destination)?;
        if copied != item.size_bytes {
            return Err(AppError::coded("content_projection_stage_size_mismatch"));
        }
        verify_file(
            &destination,
            item.size_bytes,
            &item.sha256,
            "content_projection_stage_hash_mismatch",
        )?;
        staged.insert(key, destination);
    }
    Ok(staged)
}

fn same_projected_artifact(
    previous: &ProjectedContentItemV1,
    desired: &ProjectedContentItemV1,
) -> bool {
    previous.is_override == desired.is_override
        && previous.relative_target == desired.relative_target
        && previous.sha256 == desired.sha256
        && previous.size_bytes == desired.size_bytes
}

fn preserve_override_seed(
    previous: Option<&ProjectedContentItemV1>,
    desired: &ProjectedContentItemV1,
    target: &SecurePath,
) -> bool {
    desired.is_override
        && ((target.absolute().exists() && previous.is_none_or(|previous| previous.is_override))
            || previous.is_some_and(|previous| {
                previous.is_override && previous.relative_target == desired.relative_target
            }))
}

fn stage_marker(
    registry: &PathRegistry,
    transaction_relative: &Path,
    marker: &ContentProjectionMarkerV1,
) -> AppResult<SecurePath> {
    let marker_path = registry.resolve("profiles", transaction_relative.join("marker.json"))?;
    let mut bytes = serde_json::to_vec_pretty(marker)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_MARKER_BYTES {
        return Err(AppError::coded("content_projection_marker_too_large"));
    }
    secure_fs::write_new(&marker_path, &bytes)?;
    Ok(marker_path)
}

fn rollback_moves(activations: &[MoveRecord], backups: &[MoveRecord]) -> AppResult<()> {
    let mut first_error = None;
    for movement in activations.iter().rev() {
        if movement.destination.absolute().exists() {
            if let Err(error) = secure_fs::rename_new(&movement.destination, &movement.source) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    for movement in backups.iter().rev() {
        if movement.destination.absolute().exists() {
            if let Err(error) = secure_fs::rename_new(&movement.destination, &movement.source) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn scan_projection_directories(
    registry: &PathRegistry,
    profile_id: &str,
    previous: Option<&ContentProjectionMarkerV1>,
    desired: &ContentProjectionMarkerV1,
) -> AppResult<BTreeMap<String, ExistingProjectionEntry>> {
    let mut existing = BTreeMap::new();
    let directories = previous
        .into_iter()
        .flat_map(|marker| marker.items.iter())
        .chain(desired.items.iter())
        .filter_map(|item| Path::new(&item.relative_target).parent())
        .map(Path::to_path_buf)
        .collect::<BTreeSet<_>>();
    if directories.is_empty() {
        return Ok(existing);
    }
    for directory in directories {
        let relative_directory = PathBuf::from(profile_id).join("instance").join(&directory);
        let secure_directory = registry.resolve("profiles", &relative_directory)?;
        if !secure_directory.absolute().exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(secure_directory.absolute())?;
        if !metadata.is_dir() {
            return Err(AppError::coded("content_projection_directory_invalid"));
        }
        for entry in fs::read_dir(secure_directory.absolute())? {
            let entry = entry?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|_| AppError::coded("content_projection_path_encoding_invalid"))?;
            let relative_target = if directory.as_os_str().is_empty() {
                file_name
            } else {
                format!(
                    "{}/{}",
                    directory
                        .iter()
                        .map(|part| part.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                    file_name
                )
            };
            let resolved = registry.resolve(
                "profiles",
                instance_content_relative(profile_id, &relative_target),
            )?;
            validate_existing_chain(resolved.anchor(), resolved.absolute())?;
            let metadata = fs::symlink_metadata(resolved.absolute())?;
            if !metadata.is_file() && !metadata.is_dir() {
                return Err(AppError::coded("content_projection_special_file_forbidden"));
            }
            let key = collision_key(Path::new(&relative_target))?;
            if existing
                .insert(
                    key,
                    ExistingProjectionEntry {
                        relative_target,
                        is_file: metadata.is_file(),
                    },
                )
                .is_some()
            {
                return Err(AppError::coded(
                    "content_projection_existing_path_collision",
                ));
            }
        }
    }
    Ok(existing)
}

fn verify_file(
    path: &SecurePath,
    expected_size: u64,
    expected_sha256: &str,
    mismatch_code: &'static str,
) -> AppResult<()> {
    validate_existing_chain(path.anchor(), path.absolute())?;
    let before = fs::symlink_metadata(path.absolute())?;
    if !before.is_file() || before.len() != expected_size {
        return Err(AppError::coded_with(
            mismatch_code,
            [("relativePath", path.relative().display().to_string())],
        ));
    }
    let mut file = File::open(path.absolute())?;
    let mut hasher = Sha256::new();
    let mut read_size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        read_size = read_size
            .checked_add(read as u64)
            .ok_or_else(|| AppError::coded("content_projection_size_overflow"))?;
        hasher.update(&buffer[..read]);
    }
    validate_existing_chain(path.anchor(), path.absolute())?;
    let after = fs::symlink_metadata(path.absolute())?;
    let actual_sha256 = hex::encode(hasher.finalize());
    if read_size != expected_size
        || after.len() != expected_size
        || actual_sha256 != expected_sha256
    {
        return Err(AppError::coded_with(
            mismatch_code,
            [("relativePath", path.relative().display().to_string())],
        ));
    }
    Ok(())
}

fn marker_state_sha256(marker: &ContentProjectionMarkerV1) -> String {
    let mut hasher = Sha256::new();
    append_text(&mut hasher, PROJECTION_FORMAT);
    hasher.update(marker.format_version.to_be_bytes());
    append_text(&mut hasher, &marker.profile_id);
    append_text(&mut hasher, &marker.revision_id);
    match marker.content_lock_sha256.as_deref() {
        Some(value) => {
            hasher.update([1]);
            append_text(&mut hasher, value);
        }
        None => hasher.update([0]),
    }
    hasher.update((marker.items.len() as u64).to_be_bytes());
    for item in &marker.items {
        append_text(&mut hasher, &item.content_id);
        append_text(&mut hasher, item.kind.as_str());
        hasher.update([u8::from(item.is_override)]);
        append_text(&mut hasher, &item.relative_target);
        append_text(&mut hasher, &item.sha256);
        hasher.update(item.size_bytes.to_be_bytes());
    }
    hex::encode(hasher.finalize())
}

fn append_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn marker_relative(profile_id: &str) -> PathBuf {
    PathBuf::from(profile_id)
        .join("instance")
        .join(".s9lab")
        .join("content-projection.json")
}

fn instance_content_relative(profile_id: &str, relative_target: &str) -> PathBuf {
    PathBuf::from(profile_id)
        .join("instance")
        .join(relative_target)
}

fn revision_content_relative(
    profile_id: &str,
    revision_id: &str,
    relative_target: &str,
) -> PathBuf {
    PathBuf::from(profile_id)
        .join("revisions")
        .join(revision_id)
        .join("content")
        .join(relative_target)
}

fn validate_identifier(value: &str) -> AppResult<()> {
    let normalized = normalize_relative_path(Path::new(value))?;
    if value.contains(['/', '\\'])
        || normalized.components().count() != 1
        || normalized.as_os_str().to_string_lossy() != value
    {
        return Err(AppError::coded("content_projection_identifier_invalid"));
    }
    Ok(())
}

fn validate_content_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(AppError::coded("content_projection_marker_invalid"));
    }
    Ok(())
}

fn validate_sha256(value: &str, code: &'static str) -> AppResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded(code));
    }
    Ok(())
}

fn target_matches_kind(kind: ContentKind, target: &str) -> AppResult<bool> {
    let normalized = normalize_relative_path(Path::new(target))?;
    if target.contains('\\')
        || normalized
            .iter()
            .map(|component| component.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
            != target
    {
        return Ok(false);
    }
    let (prefix, suffix) = match kind {
        ContentKind::Mod => ("mods/", ".jar"),
        ContentKind::ResourcePack => ("resourcepacks/", ".zip"),
        ContentKind::ShaderPack => ("shaderpacks/", ".zip"),
        ContentKind::Modpack => return Ok(false),
    };
    Ok(target
        .strip_prefix(prefix)
        .is_some_and(|name| !name.is_empty() && !name.contains('/') && name.ends_with(suffix)))
}

fn combine_cleanup_error(primary: AppError, cleanup: AppResult<()>) -> AppError {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) => AppError::coded_with(
            "content_projection_cleanup_failed",
            [
                ("primary", primary.descriptor().code),
                ("cleanup", cleanup.descriptor().code),
            ],
        ),
    }
}

fn combine_rollback_error(
    primary: AppError,
    rollback: AppResult<()>,
    cleanup: AppResult<()>,
) -> AppError {
    match (rollback, cleanup) {
        (Ok(()), Ok(())) => primary,
        (rollback, cleanup) => AppError::coded_with(
            "content_projection_rollback_failed",
            [
                ("primary", primary.descriptor().code),
                (
                    "rollback",
                    rollback
                        .err()
                        .map_or_else(|| "ok".into(), |error| error.descriptor().code),
                ),
                (
                    "cleanup",
                    cleanup
                        .err()
                        .map_or_else(|| "ok".into(), |error| error.descriptor().code),
                ),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::{
            content_lock_sha256, ContentSelection, ContentTargetRuntime, ContentVersionRequirement,
            ResolvedContentItemV1, ResolvedContentOverrideV1, CONTENT_LOCK_FORMAT,
            CONTENT_LOCK_FORMAT_VERSION,
        },
        operations::model::sha256_hex,
        runtime::{LoaderKind, LoaderSelection},
        security::RegisteredRoot,
    };

    struct Fixture {
        root: PathBuf,
        profiles: PathBuf,
        registry: PathRegistry,
        profile_id: String,
        revision_id: String,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = crate::foundation::test_root(name);
            let profiles = root.join("profiles");
            fs::create_dir(&profiles).expect("create profiles root");
            let registry = PathRegistry::new(
                &root,
                [RegisteredRoot {
                    id: "profiles".into(),
                    path: profiles.clone(),
                }],
            )
            .expect("create registry");
            let fixture = Self {
                root,
                profiles,
                registry,
                profile_id: "profile-a".into(),
                revision_id: "revision-a".into(),
            };
            fs::create_dir_all(fixture.revision_content_root())
                .expect("create revision content root");
            fs::create_dir_all(fixture.instance_root()).expect("create instance root");
            fixture
        }

        fn revision_content_root(&self) -> PathBuf {
            self.profiles
                .join(&self.profile_id)
                .join("revisions")
                .join(&self.revision_id)
                .join("content")
        }

        fn instance_root(&self) -> PathBuf {
            self.profiles.join(&self.profile_id).join("instance")
        }

        fn source(&self, relative_target: &str, bytes: &[u8]) {
            let path = self.revision_content_root().join(relative_target);
            fs::create_dir_all(path.parent().expect("source parent"))
                .expect("create source parent");
            fs::write(path, bytes).expect("write revision content");
        }

        fn target(&self, relative_target: &str) -> PathBuf {
            self.instance_root().join(relative_target)
        }

        fn marker(&self) -> PathBuf {
            self.instance_root()
                .join(".s9lab")
                .join("content-projection.json")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn item(
        content_id: &str,
        kind: ContentKind,
        relative_target: &str,
        bytes: &[u8],
        enabled: bool,
    ) -> ResolvedContentItemV1 {
        ResolvedContentItemV1 {
            content_id: content_id.into(),
            version: "1.0.0".into(),
            kind,
            enabled,
            source: None,
            relative_target: relative_target.into(),
            sha256: sha256_hex(bytes),
            size_bytes: bytes.len() as u64,
            dependencies: Vec::new(),
        }
    }

    fn lock(mut items: Vec<ResolvedContentItemV1>) -> ResolvedContentLockV1 {
        items.sort_by(|left, right| left.content_id.cmp(&right.content_id));
        let requested = items
            .iter()
            .map(|item| ContentSelection {
                content_id: item.content_id.clone(),
                version: ContentVersionRequirement::Exact {
                    version: item.version.clone(),
                },
                enabled: item.enabled,
            })
            .collect();
        let mut lock = ResolvedContentLockV1 {
            format: CONTENT_LOCK_FORMAT.into(),
            format_version: CONTENT_LOCK_FORMAT_VERSION,
            runtime: ContentTargetRuntime {
                minecraft_version: "1.21.1".into(),
                loader: LoaderSelection {
                    kind: LoaderKind::Fabric,
                    loader_version: Some("0.16.0".into()),
                },
            },
            include_optional_dependencies: false,
            requested,
            items,
            pack_members: Vec::new(),
            overrides: Vec::new(),
            resolution_sha256: String::new(),
        };
        lock.resolution_sha256 = content_lock_sha256(&lock).expect("hash content lock");
        lock
    }

    fn project(fixture: &Fixture, lock: Option<&ResolvedContentLockV1>) -> AppResult<()> {
        project_content_for_launch(
            &fixture.registry,
            &fixture.profile_id,
            &fixture.revision_id,
            lock,
        )
    }

    #[test]
    fn projects_only_enabled_launch_content_and_binds_the_marker() {
        let fixture = Fixture::new("content-projection-enabled");
        fixture.source("mods/enabled.jar", b"enabled");
        let lock = lock(vec![
            item(
                "enabled",
                ContentKind::Mod,
                "mods/enabled.jar",
                b"enabled",
                true,
            ),
            item(
                "disabled",
                ContentKind::Mod,
                "mods/disabled.jar",
                b"disabled",
                false,
            ),
        ]);

        project(&fixture, Some(&lock)).expect("project enabled content");

        assert_eq!(
            fs::read(fixture.target("mods/enabled.jar")).unwrap(),
            b"enabled"
        );
        assert!(!fixture.target("mods/disabled.jar").exists());
        let marker: ContentProjectionMarkerV1 =
            serde_json::from_slice(&fs::read(fixture.marker()).unwrap()).unwrap();
        assert_eq!(marker.profile_id, fixture.profile_id);
        assert_eq!(marker.revision_id, fixture.revision_id);
        assert_eq!(marker.content_lock_sha256, Some(lock.resolution_sha256));
        assert_eq!(marker.items.len(), 1);
        assert_eq!(marker.state_sha256, marker_state_sha256(&marker));
    }

    #[test]
    fn clearing_projection_removes_only_managed_files_and_preserves_foreign_files() {
        let fixture = Fixture::new("content-projection-foreign-preserved");
        fixture.source("mods/managed.jar", b"managed");
        let initial = lock(vec![item(
            "managed",
            ContentKind::Mod,
            "mods/managed.jar",
            b"managed",
            true,
        )]);
        project(&fixture, Some(&initial)).expect("initial projection");
        fs::write(fixture.target("mods/foreign.jar"), b"foreign").expect("write foreign mod");

        project(&fixture, None).expect("clear managed projection");

        assert!(!fixture.target("mods/managed.jar").exists());
        assert_eq!(
            fs::read(fixture.target("mods/foreign.jar")).unwrap(),
            b"foreign"
        );
    }

    #[test]
    fn existing_foreign_target_fails_closed_without_overwrite() {
        let fixture = Fixture::new("content-projection-conflict");
        fixture.source("mods/example.jar", b"managed");
        fs::create_dir_all(fixture.target("mods")).unwrap();
        fs::write(fixture.target("mods/example.jar"), b"foreign").unwrap();
        let desired = lock(vec![item(
            "example",
            ContentKind::Mod,
            "mods/example.jar",
            b"managed",
            true,
        )]);

        let error = project(&fixture, Some(&desired)).expect_err("foreign conflict must fail");

        assert_eq!(
            error.descriptor().code,
            "content_projection_foreign_target_conflict"
        );
        assert_eq!(
            fs::read(fixture.target("mods/example.jar")).unwrap(),
            b"foreign"
        );
        assert!(!fixture.marker().exists());
    }

    #[test]
    fn case_alias_of_foreign_target_is_rejected_portably() {
        let fixture = Fixture::new("content-projection-case-conflict");
        fixture.source("mods/example.jar", b"managed");
        fs::create_dir_all(fixture.target("mods")).unwrap();
        fs::write(fixture.target("mods/EXAMPLE.jar"), b"foreign").unwrap();
        let desired = lock(vec![item(
            "example",
            ContentKind::Mod,
            "mods/example.jar",
            b"managed",
            true,
        )]);

        assert!(project(&fixture, Some(&desired)).is_err());
        assert_eq!(
            fs::read(fixture.target("mods/EXAMPLE.jar")).unwrap(),
            b"foreign"
        );
    }

    #[test]
    fn source_hash_mismatch_never_changes_the_instance() {
        let fixture = Fixture::new("content-projection-hash");
        fixture.source("mods/example.jar", b"tampered");
        let desired = lock(vec![item(
            "example",
            ContentKind::Mod,
            "mods/example.jar",
            b"expected",
            true,
        )]);

        let error = project(&fixture, Some(&desired)).expect_err("hash mismatch must fail");

        assert_eq!(
            error.descriptor().code,
            "content_projection_source_mismatch"
        );
        assert!(!fixture.target("mods/example.jar").exists());
        assert!(!fixture.marker().exists());
    }

    #[test]
    fn disabled_content_does_not_require_or_project_an_artifact() {
        let fixture = Fixture::new("content-projection-disabled");
        let desired = lock(vec![item(
            "disabled",
            ContentKind::Mod,
            "mods/disabled.jar",
            b"not-present",
            false,
        )]);

        project(&fixture, Some(&desired)).expect("disabled content is metadata only");

        assert!(!fixture.target("mods/disabled.jar").exists());
        assert!(fixture.marker().is_file());
    }

    #[test]
    fn modpack_container_is_never_projected_into_the_instance() {
        let fixture = Fixture::new("content-projection-modpack-container");
        let desired = lock(vec![item(
            "container",
            ContentKind::Modpack,
            "modpacks/container.zip",
            b"container",
            true,
        )]);

        project(&fixture, Some(&desired)).expect("container is intentionally not projected");

        assert!(!fixture.target("modpacks/container.zip").exists());
    }

    #[test]
    fn seeds_modpack_overrides_once_and_preserves_local_changes_when_disabled() {
        let fixture = Fixture::new("content-projection-modpack-override");
        fixture.source("config/example.toml", b"managed override");
        let mut desired = lock(vec![item(
            "container",
            ContentKind::Modpack,
            "modpacks/container.mrpack",
            b"container",
            true,
        )]);
        desired.overrides.push(ResolvedContentOverrideV1 {
            pack_content_id: "container".into(),
            relative_target: "config/example.toml".into(),
            sha256: sha256_hex(b"managed override"),
            size_bytes: b"managed override".len() as u64,
        });
        desired.resolution_sha256 = content_lock_sha256(&desired).unwrap();

        project(&fixture, Some(&desired)).expect("project override");
        assert_eq!(
            fs::read(fixture.target("config/example.toml")).unwrap(),
            b"managed override"
        );

        fs::write(
            fixture.target("config/example.toml"),
            b"user modified override",
        )
        .unwrap();
        project(&fixture, Some(&desired)).expect("preserve changed override");
        assert_eq!(
            fs::read(fixture.target("config/example.toml")).unwrap(),
            b"user modified override"
        );

        desired.items[0].enabled = false;
        desired.requested[0].enabled = false;
        desired.resolution_sha256 = content_lock_sha256(&desired).unwrap();
        project(&fixture, Some(&desired)).expect("disable pack override");
        assert_eq!(
            fs::read(fixture.target("config/example.toml")).unwrap(),
            b"user modified override"
        );

        desired.items[0].enabled = true;
        desired.requested[0].enabled = true;
        desired.resolution_sha256 = content_lock_sha256(&desired).unwrap();
        project(&fixture, Some(&desired)).expect("re-enable changed override seed");
        assert_eq!(
            fs::read(fixture.target("config/example.toml")).unwrap(),
            b"user modified override"
        );
    }

    struct FailAfterFirstTarget;

    impl ProjectionCheckpoint for FailAfterFirstTarget {
        fn after_target_activated(&self, activated_targets: usize) -> AppResult<()> {
            if activated_targets == 1 {
                Err(AppError::coded("content_projection_injected_failure"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn activation_failure_restores_previous_files_and_marker() {
        let fixture = Fixture::new("content-projection-rollback");
        fixture.source("mods/example.jar", b"old");
        let initial = lock(vec![item(
            "example",
            ContentKind::Mod,
            "mods/example.jar",
            b"old",
            true,
        )]);
        project(&fixture, Some(&initial)).expect("initial projection");
        let old_marker = fs::read(fixture.marker()).unwrap();

        fixture.source("mods/second.jar", b"second");
        fs::write(
            fixture.revision_content_root().join("mods/example.jar"),
            b"new",
        )
        .unwrap();
        let replacement = lock(vec![
            item(
                "example",
                ContentKind::Mod,
                "mods/example.jar",
                b"new",
                true,
            ),
            item(
                "second",
                ContentKind::Mod,
                "mods/second.jar",
                b"second",
                true,
            ),
        ]);

        let _guard = acquire_projection_lock().unwrap();
        let error = project_content_for_launch_locked(
            &fixture.registry,
            &fixture.profile_id,
            &fixture.revision_id,
            Some(&replacement),
            &FailAfterFirstTarget,
        )
        .expect_err("injected activation failure");

        assert_eq!(
            error.descriptor().code,
            "content_projection_injected_failure"
        );
        assert_eq!(
            fs::read(fixture.target("mods/example.jar")).unwrap(),
            b"old"
        );
        assert!(!fixture.target("mods/second.jar").exists());
        assert_eq!(fs::read(fixture.marker()).unwrap(), old_marker);
    }

    #[test]
    fn tampered_marker_fails_closed_without_removing_managed_content() {
        let fixture = Fixture::new("content-projection-marker-tamper");
        fixture.source("mods/example.jar", b"managed");
        let initial = lock(vec![item(
            "example",
            ContentKind::Mod,
            "mods/example.jar",
            b"managed",
            true,
        )]);
        project(&fixture, Some(&initial)).expect("initial projection");
        let mut marker: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.marker()).unwrap()).unwrap();
        marker["revisionId"] = serde_json::Value::String("revision-tampered".into());
        fs::write(
            fixture.marker(),
            serde_json::to_vec_pretty(&marker).unwrap(),
        )
        .unwrap();

        let error = project(&fixture, None).expect_err("tampered marker must fail");

        assert_eq!(
            error.descriptor().code,
            "content_projection_marker_hash_mismatch"
        );
        assert_eq!(
            fs::read(fixture.target("mods/example.jar")).unwrap(),
            b"managed"
        );
    }
}
