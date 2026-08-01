use crate::{
    auth::model::{Account, AccountSession},
    error::{AppError, AppResult},
    operations::model::new_identifier,
    profiles::model::{
        ProfileLockV2, ResolvedLaunchArgument, ResolvedLaunchConfiguration, ResolvedLaunchRule,
    },
    runtime::{
        validate_jar_entries, JarEntryDescriptor, JarValidationLimits, LoaderKind,
        ResolvedRuntimeItem, RuntimeArtifactKind,
    },
    security::{paths::validate_existing_chain, PathRegistry, SecurePath},
};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};
use tokio::{process::Child, sync::Mutex};

const MAX_NATIVE_ARCHIVES: usize = 256;
const MAX_NATIVE_TOTAL_ENTRIES: usize = 16_384;
const MAX_NATIVE_TOTAL_COMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_NATIVE_TOTAL_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
const NATIVE_JAR_LIMITS: JarValidationLimits = JarValidationLimits {
    max_entries: 4_096,
    max_total_compressed_bytes: 256 * 1024 * 1024,
    max_entry_uncompressed_bytes: 128 * 1024 * 1024,
    max_total_uncompressed_bytes: 512 * 1024 * 1024,
    max_compression_ratio: 100,
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileLaunchState {
    Starting,
    Running,
    Stopping,
    Exited,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileLaunchStatus {
    pub launch_id: String,
    pub profile_id: String,
    pub state: ProfileLaunchState,
    pub process_id: Option<u32>,
    pub account_name: String,
    pub started_at_unix: i64,
    pub exit_code: Option<i32>,
}

struct RunningProfile {
    child: Child,
    process_tree: ManagedProcessTree,
    status: ProfileLaunchStatus,
}

#[cfg(target_os = "windows")]
struct ManagedProcessTree {
    job: std::os::windows::io::OwnedHandle,
}

#[cfg(not(target_os = "windows"))]
struct ManagedProcessTree;

#[derive(Clone, Default)]
pub struct ProfileProcessManager {
    running: Arc<Mutex<BTreeMap<String, RunningProfile>>>,
}

pub(crate) struct LaunchSecrets<'a> {
    pub account: &'a Account,
    pub session: &'a AccountSession,
}

pub struct ProfileLaunchRequest<'a> {
    pub profile_id: &'a str,
    pub revision_id: &'a str,
    pub lock: &'a ProfileLockV2,
    pub java_executable: &'a Path,
    pub memory_mb: u32,
    pub secrets: LaunchSecrets<'a>,
}

impl ProfileProcessManager {
    pub async fn launch(
        &self,
        registry: &PathRegistry,
        request: ProfileLaunchRequest<'_>,
    ) -> AppResult<ProfileLaunchStatus> {
        validate_launch_request(&request)?;
        {
            let mut running = self.running.lock().await;
            cleanup_finished_entries(&mut running)?;
            ensure_profile_is_not_running(&running, request.profile_id)?;
        }

        let launch_id = new_identifier("launch");
        let paths = resolve_launch_paths(
            registry,
            request.profile_id,
            request.revision_id,
            &launch_id,
        )?;
        let native_archives = validate_native_archives(request.lock, &paths)?;
        let args = build_launch_arguments(&request, &paths)?;
        extract_native_archives(registry, &paths, native_archives)?;

        // Re-check while holding the process table through spawn and insertion.
        // Two concurrent launch preparations may proceed independently, but only
        // one process for a profile can cross this final boundary.
        let mut running = self.running.lock().await;
        cleanup_finished_entries(&mut running)?;
        ensure_profile_is_not_running(&running, request.profile_id)?;
        let mut command = tokio::process::Command::new(request.java_executable);
        command
            .args(args)
            .current_dir(&paths.instance)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(false)
            .env("MINECRAFT_LAUNCHER_BRAND", "S9LabLauncher")
            .env("MINECRAFT_LAUNCHER_VERSION", env!("CARGO_PKG_VERSION"));
        configure_windows_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|_| AppError::coded("runtime_process_spawn_failed"))?;
        let process_tree = match ManagedProcessTree::attach_and_resume(&child) {
            Ok(process_tree) => process_tree,
            Err(error) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(error);
            }
        };
        let status = ProfileLaunchStatus {
            launch_id: launch_id.clone(),
            profile_id: request.profile_id.to_string(),
            state: ProfileLaunchState::Running,
            process_id: child.id(),
            account_name: request.secrets.account.username.clone(),
            started_at_unix: Utc::now().timestamp(),
            exit_code: None,
        };
        running.insert(
            launch_id,
            RunningProfile {
                child,
                process_tree,
                status: status.clone(),
            },
        );
        Ok(status)
    }

    pub async fn statuses(&self) -> AppResult<Vec<ProfileLaunchStatus>> {
        self.cleanup_finished().await?;
        let mut statuses = self
            .running
            .lock()
            .await
            .values()
            .map(|running| running.status.clone())
            .collect::<Vec<_>>();
        statuses.sort_by(|left, right| left.launch_id.cmp(&right.launch_id));
        Ok(statuses)
    }

    pub async fn stop(&self, launch_id: &str) -> AppResult<ProfileLaunchStatus> {
        validate_identifier(launch_id, "runtime_launch_id_invalid")?;
        let mut running = self.running.lock().await;
        let instance = running
            .get_mut(launch_id)
            .ok_or_else(|| AppError::coded("runtime_launch_not_found"))?;
        instance.status.state = ProfileLaunchState::Stopping;
        #[cfg(target_os = "windows")]
        instance.process_tree.terminate()?;
        #[cfg(not(target_os = "windows"))]
        instance
            .child
            .kill()
            .await
            .map_err(|_| AppError::coded("runtime_process_stop_failed"))?;
        let exit = instance
            .child
            .wait()
            .await
            .map_err(|_| AppError::coded("runtime_process_wait_failed"))?;
        let mut status = instance.status.clone();
        status.state = ProfileLaunchState::Exited;
        status.exit_code = exit.code();
        running.remove(launch_id);
        Ok(status)
    }

    async fn cleanup_finished(&self) -> AppResult<()> {
        let mut running = self.running.lock().await;
        cleanup_finished_entries(&mut running)
    }
}

fn ensure_profile_is_not_running(
    running: &BTreeMap<String, RunningProfile>,
    profile_id: &str,
) -> AppResult<()> {
    if running
        .values()
        .any(|instance| instance.status.profile_id == profile_id)
    {
        return Err(AppError::coded("runtime_profile_already_running"));
    }
    Ok(())
}

fn cleanup_finished_entries(running: &mut BTreeMap<String, RunningProfile>) -> AppResult<()> {
    let mut finished = Vec::new();
    for (launch_id, instance) in running.iter_mut() {
        if instance
            .child
            .try_wait()
            .map_err(|_| AppError::coded("runtime_process_status_failed"))?
            .is_some()
        {
            finished.push(launch_id.clone());
        }
    }
    for launch_id in finished {
        running.remove(&launch_id);
    }
    Ok(())
}

struct LaunchPaths {
    instance: PathBuf,
    runtime: PathBuf,
    assets: PathBuf,
    libraries: PathBuf,
    native_parent: SecurePath,
    natives: SecurePath,
}

fn resolve_launch_paths(
    registry: &PathRegistry,
    profile_id: &str,
    revision_id: &str,
    launch_id: &str,
) -> AppResult<LaunchPaths> {
    validate_identifier(profile_id, "runtime_profile_id_invalid")?;
    validate_identifier(revision_id, "runtime_revision_id_invalid")?;
    validate_identifier(launch_id, "runtime_launch_id_invalid")?;
    let instance = registry.resolve("profiles", format!("{profile_id}/instance"))?;
    let runtime = registry.resolve(
        "profiles",
        format!("{profile_id}/revisions/{revision_id}/runtime"),
    )?;
    let assets = registry.resolve(
        "profiles",
        format!("{profile_id}/revisions/{revision_id}/runtime/assets"),
    )?;
    let libraries = registry.resolve(
        "profiles",
        format!("{profile_id}/revisions/{revision_id}/runtime/libraries"),
    )?;
    let native_parent =
        registry.resolve("profiles", format!("{profile_id}/instance/.s9lab/natives"))?;
    let natives = registry.resolve(
        "profiles",
        format!("{profile_id}/instance/.s9lab/natives/{launch_id}"),
    )?;
    for path in [&instance, &runtime, &assets, &libraries] {
        validate_existing_chain(path.anchor(), path.absolute())?;
        if !path.absolute().is_dir() {
            return Err(AppError::coded("runtime_projection_missing"));
        }
    }
    Ok(LaunchPaths {
        instance: instance.absolute().to_path_buf(),
        runtime: runtime.absolute().to_path_buf(),
        assets: assets.absolute().to_path_buf(),
        libraries: libraries.absolute().to_path_buf(),
        native_parent,
        natives,
    })
}

struct ValidatedNativeEntry {
    archive_index: usize,
    descriptor: JarEntryDescriptor,
    relative_path: PathBuf,
}

struct ValidatedNativeArchive {
    archive: zip::ZipArchive<File>,
    entries: Vec<ValidatedNativeEntry>,
}

fn validate_native_archives(
    lock: &ProfileLockV2,
    paths: &LaunchPaths,
) -> AppResult<Vec<ValidatedNativeArchive>> {
    let targets = &lock.launch.native_jar_targets;
    if targets.len() > MAX_NATIVE_ARCHIVES {
        return Err(AppError::coded_with(
            "runtime_native_archive_count_invalid",
            [
                ("archiveCount", targets.len().to_string()),
                ("maxArchiveCount", MAX_NATIVE_ARCHIVES.to_string()),
            ],
        ));
    }
    let locked_items = targets
        .iter()
        .map(|target| locked_native_item(lock, target))
        .collect::<AppResult<Vec<_>>>()?;
    let mut total_archive_bytes = 0u64;
    for item in &locked_items {
        if item.size_bytes > NATIVE_JAR_LIMITS.max_total_compressed_bytes {
            return Err(AppError::coded("runtime_native_jar_file_too_large"));
        }
        total_archive_bytes = total_archive_bytes
            .checked_add(item.size_bytes)
            .ok_or_else(|| AppError::coded("runtime_native_budget_overflow"))?;
        if total_archive_bytes > MAX_NATIVE_TOTAL_COMPRESSED_BYTES {
            return Err(AppError::coded(
                "runtime_native_total_archive_limit_exceeded",
            ));
        }
    }

    let mut global_paths = BTreeMap::<String, bool>::new();
    let mut total_entries = 0usize;
    let mut total_compressed_bytes = 0u64;
    let mut total_uncompressed_bytes = 0u64;
    let mut archives = Vec::with_capacity(targets.len());

    for (target, locked_item) in targets.iter().zip(locked_items) {
        let path = resolve_runtime_target(&paths.runtime, target)?;
        let mut archive = open_verified_native_archive(&paths.runtime, &path, locked_item)?;
        let mut descriptors = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|_| AppError::coded("runtime_native_jar_invalid"))?;
            let relative_path = std::str::from_utf8(entry.name_raw())
                .map_err(|_| AppError::coded("runtime_native_entry_name_invalid_utf8"))?
                .to_string();
            descriptors.push(JarEntryDescriptor {
                relative_path,
                is_directory: entry.is_dir(),
                compressed_size_bytes: entry.compressed_size(),
                uncompressed_size_bytes: entry.size(),
                encrypted: entry.encrypted(),
                unix_mode: entry.unix_mode(),
            });
        }

        let summary = validate_jar_entries(&descriptors, NATIVE_JAR_LIMITS)?;
        total_entries = total_entries
            .checked_add(summary.entry_count)
            .ok_or_else(|| AppError::coded("runtime_native_budget_overflow"))?;
        total_compressed_bytes = total_compressed_bytes
            .checked_add(summary.total_compressed_bytes)
            .ok_or_else(|| AppError::coded("runtime_native_budget_overflow"))?;
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(summary.total_uncompressed_bytes)
            .ok_or_else(|| AppError::coded("runtime_native_budget_overflow"))?;
        validate_native_totals(
            total_entries,
            total_compressed_bytes,
            total_uncompressed_bytes,
        )?;

        let mut entries = Vec::with_capacity(summary.file_count);
        for (archive_index, descriptor) in descriptors.into_iter().enumerate() {
            let relative_path = normalized_jar_entry_path(&descriptor)?;
            let collision = crate::security::paths::collision_key(&relative_path)?;
            if is_native_metadata_path(&collision) {
                continue;
            }
            register_native_output_path(&mut global_paths, &collision, descriptor.is_directory)?;
            if !descriptor.is_directory {
                entries.push(ValidatedNativeEntry {
                    archive_index,
                    descriptor,
                    relative_path,
                });
            }
        }
        archives.push(ValidatedNativeArchive { archive, entries });
    }
    Ok(archives)
}

fn locked_native_item<'a>(
    lock: &'a ProfileLockV2,
    target: &str,
) -> AppResult<&'a ResolvedRuntimeItem> {
    let item = lock
        .runtime
        .items
        .iter()
        .find(|item| item.relative_target == target)
        .ok_or_else(|| AppError::coded("runtime_native_jar_lock_missing"))?;
    if item.kind != RuntimeArtifactKind::MinecraftLibrary {
        return Err(AppError::coded("runtime_native_jar_kind_invalid"));
    }
    Ok(item)
}

fn open_verified_native_archive(
    runtime_root: &Path,
    path: &Path,
    locked_item: &ResolvedRuntimeItem,
) -> AppResult<zip::ZipArchive<File>> {
    let file = open_verified_runtime_file(
        runtime_root,
        path,
        locked_item,
        "runtime_native_jar_size_mismatch",
        "runtime_native_jar_hash_mismatch",
    )?;
    zip::ZipArchive::new(file).map_err(|_| AppError::coded("runtime_native_jar_invalid"))
}

fn open_verified_runtime_file(
    runtime_root: &Path,
    path: &Path,
    locked_item: &ResolvedRuntimeItem,
    size_error_code: &'static str,
    hash_error_code: &'static str,
) -> AppResult<File> {
    validate_existing_chain(runtime_root, path)?;
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != locked_item.size_bytes {
        return Err(AppError::coded(size_error_code));
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if hex::encode(hasher.finalize()) != locked_item.sha256 {
        return Err(AppError::coded(hash_error_code));
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}

fn normalized_jar_entry_path(descriptor: &JarEntryDescriptor) -> AppResult<PathBuf> {
    let value = if descriptor.is_directory {
        descriptor
            .relative_path
            .strip_suffix('/')
            .ok_or_else(|| AppError::coded("component_jar_directory_marker_invalid"))?
    } else {
        descriptor.relative_path.as_str()
    };
    crate::security::paths::normalize_relative_path(Path::new(value))
}

fn is_native_metadata_path(collision_key: &str) -> bool {
    collision_key == "meta-inf" || collision_key.starts_with("meta-inf/")
}

fn register_native_output_path(
    paths: &mut BTreeMap<String, bool>,
    collision: &str,
    is_directory: bool,
) -> AppResult<()> {
    if let Some(existing_is_directory) = paths.get(collision) {
        if *existing_is_directory && is_directory {
            return Ok(());
        }
        let code = if !*existing_is_directory && !is_directory {
            "runtime_native_entry_duplicate"
        } else {
            "runtime_native_entry_path_conflict"
        };
        return Err(AppError::coded_with(
            code,
            [("normalizedPath", collision.to_string())],
        ));
    }

    for (separator, _) in collision.match_indices('/') {
        if matches!(paths.get(&collision[..separator]), Some(false)) {
            return Err(AppError::coded_with(
                "runtime_native_entry_path_conflict",
                [("normalizedPath", collision.to_string())],
            ));
        }
    }
    if !is_directory {
        let descendant_prefix = format!("{collision}/");
        if paths
            .range(descendant_prefix.clone()..)
            .next()
            .is_some_and(|(candidate, _)| candidate.starts_with(&descendant_prefix))
        {
            return Err(AppError::coded_with(
                "runtime_native_entry_path_conflict",
                [("normalizedPath", collision.to_string())],
            ));
        }
    }
    paths.insert(collision.to_string(), is_directory);
    Ok(())
}

fn validate_native_totals(
    entries: usize,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
) -> AppResult<()> {
    if entries > MAX_NATIVE_TOTAL_ENTRIES {
        return Err(AppError::coded("runtime_native_total_entry_limit_exceeded"));
    }
    if compressed_bytes > MAX_NATIVE_TOTAL_COMPRESSED_BYTES {
        return Err(AppError::coded(
            "runtime_native_total_compressed_limit_exceeded",
        ));
    }
    if uncompressed_bytes > MAX_NATIVE_TOTAL_UNCOMPRESSED_BYTES {
        return Err(AppError::coded(
            "runtime_native_total_uncompressed_limit_exceeded",
        ));
    }
    Ok(())
}

fn extract_native_archives(
    registry: &PathRegistry,
    paths: &LaunchPaths,
    mut archives: Vec<ValidatedNativeArchive>,
) -> AppResult<()> {
    create_native_launch_directory(paths)?;
    for validated in &mut archives {
        for planned in &validated.entries {
            let mut entry = validated
                .archive
                .by_index(planned.archive_index)
                .map_err(|_| AppError::coded("runtime_native_jar_invalid"))?;
            if entry.name_raw() != planned.descriptor.relative_path.as_bytes()
                || entry.is_dir()
                || entry.size() != planned.descriptor.uncompressed_size_bytes
                || entry.compressed_size() != planned.descriptor.compressed_size_bytes
                || entry.encrypted() != planned.descriptor.encrypted
                || entry.unix_mode() != planned.descriptor.unix_mode
            {
                return Err(AppError::coded("runtime_native_entry_changed"));
            }

            let relative_target = paths.natives.relative().join(&planned.relative_path);
            let target = registry.resolve("profiles", relative_target)?;
            if !target.absolute().starts_with(paths.natives.absolute()) {
                return Err(AppError::coded("runtime_native_target_outside_launch"));
            }
            let mut output = crate::security::fs::open_new_file(&target)?;
            let read_limit = planned
                .descriptor
                .uncompressed_size_bytes
                .checked_add(1)
                .ok_or_else(|| AppError::coded("runtime_native_budget_overflow"))?;
            let written = std::io::copy(&mut entry.by_ref().take(read_limit), &mut output)
                .map_err(|_| AppError::coded("runtime_native_entry_extract_failed"))?;
            if written != planned.descriptor.uncompressed_size_bytes {
                return Err(AppError::coded("runtime_native_entry_size_mismatch"));
            }
            output.sync_all()?;
            drop(output);
            validate_existing_chain(target.anchor(), target.absolute())?;
            let metadata = fs::symlink_metadata(target.absolute())?;
            if !metadata.is_file() || metadata.len() != written {
                return Err(AppError::coded("runtime_native_output_invalid"));
            }
        }
    }
    Ok(())
}

fn create_native_launch_directory(paths: &LaunchPaths) -> AppResult<()> {
    crate::security::fs::create_directories_within(
        paths.native_parent.anchor(),
        paths.native_parent.root(),
        paths.native_parent.absolute(),
    )?;
    match fs::create_dir(paths.natives.absolute()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(AppError::coded("runtime_native_target_exists"));
        }
        Err(error) => return Err(error.into()),
    }
    validate_existing_chain(paths.natives.anchor(), paths.natives.absolute())
}

fn resolve_verified_runtime_item_path(
    runtime_root: &Path,
    item: &ResolvedRuntimeItem,
) -> AppResult<PathBuf> {
    crate::runtime::validate_resolved_runtime_item(item)?;
    let path = resolve_runtime_target(runtime_root, &item.relative_target)?;
    drop(open_verified_runtime_file(
        runtime_root,
        &path,
        item,
        "runtime_revision_artifact_size_mismatch",
        "runtime_revision_artifact_hash_mismatch",
    )?);
    Ok(path)
}

fn resolve_logging_configuration(
    lock: &ProfileLockV2,
    paths: &LaunchPaths,
) -> AppResult<Option<PathBuf>> {
    let mut items = lock
        .runtime
        .items
        .iter()
        .filter(|item| item.kind == RuntimeArtifactKind::LoggingConfiguration);
    let Some(item) = items.next() else {
        return Ok(None);
    };
    if items.next().is_some() {
        return Err(AppError::coded("runtime_logging_configuration_ambiguous"));
    }
    resolve_verified_runtime_item_path(&paths.runtime, item).map(Some)
}

fn resolve_fabric_component_argument(
    lock: &ProfileLockV2,
    paths: &LaunchPaths,
    path_separator: &str,
) -> AppResult<Option<String>> {
    let components = lock
        .runtime
        .items
        .iter()
        .filter(|item| item.kind == RuntimeArtifactKind::S9labComponent)
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Ok(None);
    }
    match lock.runtime.runtime.loader.kind {
        LoaderKind::Vanilla => {
            return Err(AppError::coded("runtime_component_vanilla_forbidden"));
        }
        LoaderKind::Neoforge => {
            return Err(AppError::coded(
                "runtime_component_neoforge_launch_unsupported",
            ));
        }
        LoaderKind::Fabric => {}
    }

    let mut targets = Vec::with_capacity(components.len());
    let mut collisions = BTreeSet::new();
    for item in components {
        let normalized =
            crate::security::paths::normalize_relative_path(Path::new(&item.relative_target))?;
        let collision = crate::security::paths::collision_key(&normalized)?;
        if !collisions.insert(collision.clone()) {
            return Err(AppError::coded_with(
                "runtime_component_target_collision",
                [("normalizedPath", collision)],
            ));
        }
        let path = resolve_verified_runtime_item_path(&paths.runtime, item)?;
        targets.push((item.relative_target.clone(), path));
    }
    targets.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(Some(
        targets
            .into_iter()
            .map(|(_, path)| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(path_separator),
    ))
}

fn arguments_reference_placeholder(arguments: &[ResolvedLaunchArgument], name: &str) -> bool {
    let placeholder = format!("${{{name}}}");
    arguments.iter().any(|argument| match argument {
        ResolvedLaunchArgument::Plain { value } => value.contains(&placeholder),
        ResolvedLaunchArgument::Conditional { values, .. } => {
            values.iter().any(|value| value.contains(&placeholder))
        }
    })
}

fn build_launch_arguments(
    request: &ProfileLaunchRequest<'_>,
    paths: &LaunchPaths,
) -> AppResult<Vec<String>> {
    let launch = &request.lock.launch;
    let separator = if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    };
    let classpath = launch
        .classpath_targets
        .iter()
        .map(|target| resolve_runtime_target(&paths.runtime, target))
        .collect::<AppResult<Vec<_>>>()?
        .into_iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(separator);
    let token = request
        .secrets
        .session
        .minecraft_access_token
        .as_deref()
        .ok_or_else(|| AppError::coded("auth_minecraft_session_missing"))?;
    let mut replacements = BTreeMap::from([
        ("auth_player_name", request.secrets.account.username.clone()),
        (
            "version_name",
            request.lock.runtime.runtime.minecraft_version.clone(),
        ),
        (
            "game_directory",
            paths.instance.to_string_lossy().into_owned(),
        ),
        ("assets_root", paths.assets.to_string_lossy().into_owned()),
        ("assets_index_name", launch.asset_index_id.clone()),
        ("auth_uuid", request.secrets.account.id.clone()),
        ("auth_access_token", token.to_string()),
        ("clientid", String::new()),
        (
            "auth_xuid",
            request
                .secrets
                .session
                .xuid
                .as_deref()
                .unwrap_or("")
                .to_string(),
        ),
        ("user_type", "msa".into()),
        ("version_type", "release".into()),
        (
            "natives_directory",
            paths.natives.absolute().to_string_lossy().into_owned(),
        ),
        ("launcher_name", "S9LabLauncher".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("classpath", classpath.clone()),
        ("classpath_separator", separator.into()),
        (
            "library_directory",
            paths.libraries.to_string_lossy().into_owned(),
        ),
        ("resolution_width", "1280".into()),
        ("resolution_height", "720".into()),
    ]);
    if arguments_reference_placeholder(&launch.jvm_arguments, "logging_config") {
        if let Some(logging_config) = resolve_logging_configuration(request.lock, paths)? {
            replacements.insert(
                "logging_config",
                logging_config.to_string_lossy().into_owned(),
            );
        }
    }
    let features = BTreeMap::from([
        ("has_custom_resolution".to_string(), true),
        ("is_demo_user".to_string(), false),
        ("has_quick_plays_support".to_string(), false),
        ("is_quick_play_singleplayer".to_string(), false),
        ("is_quick_play_multiplayer".to_string(), false),
        ("is_quick_play_realms".to_string(), false),
    ]);

    let mut output = Vec::new();
    output.push(format!("-Xms{}M", 512_u32.min(request.memory_mb)));
    output.push(format!("-Xmx{}M", request.memory_mb));
    let mut resolved_jvm_arguments =
        resolve_arguments(&launch.jvm_arguments, &replacements, &features)?;
    if resolved_jvm_arguments
        .iter()
        .any(|value| value == "-Dfabric.addMods" || value.starts_with("-Dfabric.addMods="))
    {
        return Err(AppError::coded("runtime_component_argument_conflict"));
    }
    if let Some(component_paths) =
        resolve_fabric_component_argument(request.lock, paths, separator)?
    {
        resolved_jvm_arguments.push(format!("-Dfabric.addMods={component_paths}"));
    }
    output.extend(resolved_jvm_arguments);
    if !output
        .iter()
        .any(|value| value == "-cp" || value == "-classpath")
    {
        output.push("-cp".into());
        output.push(classpath);
    }
    output.push(launch.main_class.clone());
    if let Some(legacy) = launch.legacy_game_arguments.as_deref() {
        output.extend(parse_legacy_arguments(legacy)?);
    } else {
        output.extend(resolve_arguments(
            &launch.game_arguments,
            &replacements,
            &features,
        )?);
    }
    Ok(output)
}

fn resolve_arguments(
    arguments: &[ResolvedLaunchArgument],
    replacements: &BTreeMap<&str, String>,
    features: &BTreeMap<String, bool>,
) -> AppResult<Vec<String>> {
    let mut output = Vec::new();
    for argument in arguments {
        match argument {
            ResolvedLaunchArgument::Plain { value } => {
                output.push(replace_placeholders(value, replacements)?);
            }
            ResolvedLaunchArgument::Conditional { rules, values }
                if rules_allow(rules, features) =>
            {
                for value in values {
                    output.push(replace_placeholders(value, replacements)?);
                }
            }
            ResolvedLaunchArgument::Conditional { .. } => {}
        }
    }
    Ok(output)
}

fn rules_allow(rules: &[ResolvedLaunchRule], features: &BTreeMap<String, bool>) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_matches = !rule.has_os_version_constraint
            && rule
                .os_name
                .as_deref()
                .is_none_or(|name| name == current_os_name())
            && rule.os_arch.as_deref().is_none_or(|arch| {
                arch == current_arch() || (arch == "x86" && current_arch() == "x86_64")
            });
        let features_match = rule
            .features
            .iter()
            .all(|(key, expected)| features.get(key).copied().unwrap_or(false) == *expected);
        if os_matches && features_match {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn replace_placeholders(value: &str, replacements: &BTreeMap<&str, String>) -> AppResult<String> {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(start) = remaining.find("${") {
        output.push_str(&remaining[..start]);
        let after = &remaining[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| AppError::coded("runtime_argument_placeholder_invalid"))?;
        let name = &after[..end];
        let replacement = replacements.get(name).ok_or_else(|| {
            AppError::coded_with(
                "runtime_argument_placeholder_unknown",
                [("placeholder", name.to_string())],
            )
        })?;
        output.push_str(replacement);
        remaining = &after[end + 1..];
    }
    output.push_str(remaining);
    if output.contains('\0')
        || output
            .chars()
            .any(|character| character.is_control() && character != '\t')
    {
        return Err(AppError::coded("runtime_argument_invalid"));
    }
    Ok(output)
}

fn parse_legacy_arguments(value: &str) -> AppResult<Vec<String>> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            character if character.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    arguments.push(std::mem::take(&mut current));
                }
            }
            '\0' => return Err(AppError::coded("runtime_argument_invalid")),
            character => current.push(character),
        }
    }
    if quoted || escaped {
        return Err(AppError::coded("runtime_legacy_arguments_invalid"));
    }
    if !current.is_empty() {
        arguments.push(current);
    }
    Ok(arguments)
}

fn resolve_runtime_target(root: &Path, target: &str) -> AppResult<PathBuf> {
    let normalized = crate::security::paths::normalize_relative_path(Path::new(target))?;
    let path = root.join(normalized);
    validate_existing_chain(root, &path)?;
    if !path.is_file() {
        return Err(AppError::coded("runtime_projection_file_missing"));
    }
    Ok(path)
}

fn validate_launch_request(request: &ProfileLaunchRequest<'_>) -> AppResult<()> {
    validate_identifier(request.profile_id, "runtime_profile_id_invalid")?;
    validate_identifier(request.revision_id, "runtime_revision_id_invalid")?;
    if request.lock.profile_id != request.profile_id
        || request.lock.revision_id != request.revision_id
    {
        return Err(AppError::coded("runtime_lock_profile_mismatch"));
    }
    if !(2048..=16384).contains(&request.memory_mb) {
        return Err(AppError::coded("runtime_memory_invalid"));
    }
    if !request.java_executable.is_absolute() {
        return Err(AppError::coded("runtime_java_path_uncontrolled"));
    }
    crate::runtime::validate_resolved_runtime_lock(&request.lock.runtime)?;
    validate_launch_configuration(&request.lock.launch)
}

fn validate_launch_configuration(launch: &ResolvedLaunchConfiguration) -> AppResult<()> {
    if launch.main_class.is_empty()
        || launch.main_class.len() > 256
        || !launch
            .main_class
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'$'))
        || launch.asset_index_id.is_empty()
        || launch.asset_index_id.len() > 128
        || launch.classpath_targets.is_empty()
    {
        return Err(AppError::coded("runtime_launch_configuration_invalid"));
    }
    let mut targets = BTreeSet::new();
    for target in launch
        .classpath_targets
        .iter()
        .chain(launch.native_jar_targets.iter())
    {
        let normalized = crate::security::paths::normalize_relative_path(Path::new(target))?;
        let key = crate::security::paths::collision_key(&normalized)?;
        if !targets.insert(key) {
            return Err(AppError::coded("runtime_launch_target_duplicate"));
        }
    }
    Ok(())
}

fn validate_identifier(value: &str, code: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 160
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::coded(code));
    }
    Ok(())
}

fn current_os_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn current_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86"
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_process_group(command: &mut tokio::process::Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    // Suspension closes the assignment race: no Java or descendant code can run
    // before the process is inside its launch-specific kill-on-close Job Object.
    command
        .as_std_mut()
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED);
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_process_group(_: &mut tokio::process::Command) {}

#[cfg(target_os = "windows")]
impl ManagedProcessTree {
    fn attach_and_resume(child: &Child) -> AppResult<Self> {
        use std::{
            mem::size_of,
            os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
            ptr,
        };
        use windows_sys::Win32::{
            Foundation::HANDLE,
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
                JobObjectExtendedLimitInformation, SetInformationJobObject,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
        };

        let process = child
            .raw_handle()
            .ok_or_else(|| AppError::coded("runtime_windows_process_handle_missing"))?
            as HANDLE;
        let raw_job = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if raw_job.is_null() {
            return Err(AppError::coded("runtime_windows_job_create_failed"));
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw_job.cast()) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let limits_size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .map_err(|_| AppError::coded("runtime_windows_job_configure_failed"))?;
        let configured = unsafe {
            SetInformationJobObject(
                job.as_raw_handle() as HANDLE,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                limits_size,
            )
        };
        if configured == 0 {
            return Err(AppError::coded("runtime_windows_job_configure_failed"));
        }
        if unsafe { AssignProcessToJobObject(job.as_raw_handle() as HANDLE, process) } == 0 {
            return Err(AppError::coded("runtime_windows_job_assign_failed"));
        }
        let mut assigned = 0;
        if unsafe { IsProcessInJob(process, job.as_raw_handle() as HANDLE, &raw mut assigned) } == 0
            || assigned == 0
        {
            return Err(AppError::coded("runtime_windows_job_verify_failed"));
        }
        resume_suspended_primary_thread(
            child
                .id()
                .ok_or_else(|| AppError::coded("runtime_windows_process_id_missing"))?,
        )?;
        Ok(Self { job })
    }

    fn terminate(&self) -> AppResult<()> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::HANDLE, System::JobObjects::TerminateJobObject};
        if unsafe { TerminateJobObject(self.job.as_raw_handle() as HANDLE, 1) } == 0 {
            return Err(AppError::coded("runtime_process_stop_failed"));
        }
        Ok(())
    }

    #[cfg(test)]
    fn active_process_count(&self) -> AppResult<u32> {
        use std::{mem::size_of, os::windows::io::AsRawHandle};
        use windows_sys::Win32::{
            Foundation::HANDLE,
            System::JobObjects::{
                JobObjectBasicAccountingInformation, QueryInformationJobObject,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            },
        };
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let accounting_size = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
            .map_err(|_| AppError::coded("runtime_windows_job_query_failed"))?;
        if unsafe {
            QueryInformationJobObject(
                self.job.as_raw_handle() as HANDLE,
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast(),
                accounting_size,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(AppError::coded("runtime_windows_job_query_failed"));
        }
        Ok(accounting.ActiveProcesses)
    }
}

#[cfg(target_os = "windows")]
fn resume_suspended_primary_thread(process_id: u32) -> AppResult<()> {
    use std::{
        mem::size_of,
        os::windows::io::{FromRawHandle, OwnedHandle},
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };

    let raw_snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw_snapshot == INVALID_HANDLE_VALUE {
        return Err(AppError::coded("runtime_windows_thread_snapshot_failed"));
    }
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw_snapshot.cast()) };
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>())
            .map_err(|_| AppError::coded("runtime_windows_primary_thread_missing"))?,
        ..THREADENTRY32::default()
    };
    let mut has_entry = unsafe { Thread32First(raw_snapshot, &raw mut entry) };
    if has_entry == 0 {
        return Err(AppError::coded("runtime_windows_primary_thread_missing"));
    }
    let mut primary_thread_id = None;
    loop {
        if entry.th32OwnerProcessID == process_id
            && primary_thread_id.replace(entry.th32ThreadID).is_some()
        {
            return Err(AppError::coded("runtime_windows_primary_thread_ambiguous"));
        }
        has_entry = unsafe { Thread32Next(raw_snapshot, &raw mut entry) };
        if has_entry == 0 {
            if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
                return Err(AppError::coded("runtime_windows_thread_snapshot_failed"));
            }
            break;
        }
    }
    drop(snapshot);
    let thread_id = primary_thread_id
        .ok_or_else(|| AppError::coded("runtime_windows_primary_thread_missing"))?;
    let raw_thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
    if raw_thread.is_null() {
        return Err(AppError::coded(
            "runtime_windows_primary_thread_open_failed",
        ));
    }
    let thread = unsafe { OwnedHandle::from_raw_handle(raw_thread.cast()) };
    let previous_suspend_count = unsafe { ResumeThread(raw_thread) };
    drop(thread);
    if previous_suspend_count != 1 {
        return Err(AppError::coded(
            "runtime_windows_primary_thread_resume_failed",
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
impl ManagedProcessTree {
    fn attach_and_resume(_: &Child) -> AppResult<Self> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::model::{AccountKind, AccountSessionState},
        download::ProviderId,
        operations::model::sha256_hex,
        runtime::{
            JavaPolicy, LoaderKind, LoaderSelection, ProfileRuntimeIntent, ResolvedRuntimeLockV1,
            RUNTIME_LOCK_FORMAT, RUNTIME_LOCK_FORMAT_VERSION,
        },
        security::RegisteredRoot,
    };
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    struct NativeFixture {
        root: PathBuf,
        registry: PathRegistry,
        profile_id: &'static str,
        revision_id: &'static str,
        runtime: PathBuf,
    }

    impl NativeFixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "s9lab-native-launch-{}-{}",
                std::process::id(),
                new_identifier("fixture")
            ));
            let profiles = root.join("profiles");
            let profile_id = "profile-native";
            let revision_id = "revision-native";
            let instance = profiles.join(profile_id).join("instance");
            let runtime = profiles
                .join(profile_id)
                .join("revisions")
                .join(revision_id)
                .join("runtime");
            fs::create_dir_all(&instance).expect("instance");
            fs::create_dir_all(runtime.join("assets")).expect("assets");
            fs::create_dir_all(runtime.join("libraries")).expect("libraries");
            fs::create_dir_all(runtime.join("versions")).expect("versions");
            fs::write(runtime.join("versions/1.21.1.jar"), b"client")
                .expect("client classpath fixture");
            let registry = PathRegistry::new(
                &root,
                [RegisteredRoot {
                    id: "profiles".into(),
                    path: profiles,
                }],
            )
            .expect("registry");
            Self {
                root,
                registry,
                profile_id,
                revision_id,
                runtime,
            }
        }

        fn write_jar(&self, name: &str, bytes: &[u8]) -> String {
            let target = format!("libraries/{name}");
            self.write_artifact(&target, bytes);
            target
        }

        fn write_artifact(&self, target: &str, bytes: &[u8]) {
            let path = self.runtime.join(target);
            fs::create_dir_all(path.parent().expect("artifact parent"))
                .expect("create artifact parent");
            fs::write(path, bytes).expect("write runtime artifact");
        }

        fn paths(&self, launch_id: &str) -> LaunchPaths {
            resolve_launch_paths(&self.registry, self.profile_id, self.revision_id, launch_id)
                .expect("launch paths")
        }

        fn lock(&self, jars: &[(String, Vec<u8>)]) -> ProfileLockV2 {
            let items = jars
                .iter()
                .enumerate()
                .map(|(index, (target, bytes))| ResolvedRuntimeItem {
                    provider_id: ProviderId::Mojang,
                    logical_id: format!("native:{index}"),
                    relative_target: target.clone(),
                    sha256: sha256_hex(bytes),
                    size_bytes: bytes.len() as u64,
                    kind: RuntimeArtifactKind::MinecraftLibrary,
                })
                .collect();
            ProfileLockV2 {
                format: "site.s9lab.profile-lock".into(),
                format_version: 2,
                profile_id: self.profile_id.into(),
                revision_id: self.revision_id.into(),
                manifest_sha256: "a".repeat(64),
                runtime: ResolvedRuntimeLockV1 {
                    format: RUNTIME_LOCK_FORMAT.into(),
                    format_version: RUNTIME_LOCK_FORMAT_VERSION,
                    runtime: ProfileRuntimeIntent {
                        minecraft_version: "1.21.1".into(),
                        loader: LoaderSelection {
                            kind: LoaderKind::Vanilla,
                            loader_version: None,
                        },
                        java: JavaPolicy::Managed { major_version: 21 },
                    },
                    items,
                },
                launch: ResolvedLaunchConfiguration {
                    main_class: "net.minecraft.client.main.Main".into(),
                    asset_index_id: "17".into(),
                    java_major_version: 21,
                    game_arguments: Vec::new(),
                    jvm_arguments: Vec::new(),
                    classpath_targets: vec!["versions/1.21.1.jar".into()],
                    native_jar_targets: jars.iter().map(|(target, _)| target.clone()).collect(),
                    legacy_game_arguments: None,
                },
                cache_blobs: Vec::new(),
            }
        }
    }

    impl Drop for NativeFixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).expect("remove native fixture");
        }
    }

    fn build_jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start JAR entry");
            writer.write_all(bytes).expect("write JAR entry");
        }
        writer.finish().expect("finish JAR").into_inner()
    }

    fn validation_error_code<T>(result: AppResult<T>) -> String {
        match result {
            Ok(_) => panic!("expected native validation failure"),
            Err(error) => error.descriptor().code,
        }
    }

    fn validate_single_jar(entries: &[(&str, &[u8])]) -> String {
        let fixture = NativeFixture::new();
        let bytes = build_jar(entries);
        let target = fixture.write_jar("native.jar", &bytes);
        let lock = fixture.lock(&[(target, bytes)]);
        validation_error_code(validate_native_archives(
            &lock,
            &fixture.paths("launch-validation"),
        ))
    }

    fn test_account() -> Account {
        Account {
            id: "00000000000000000000000000000000".into(),
            username: "Player".into(),
            kind: AccountKind::Microsoft,
            session_state: AccountSessionState::Active,
            ownership_verified_at_unix: 1,
            last_online_auth_at_unix: 1,
            added_at_unix: 1,
            last_used_at_unix: 1,
        }
    }

    fn test_session() -> AccountSession {
        AccountSession {
            microsoft_refresh_token: "must-never-leave-rust".into(),
            minecraft_access_token: Some("minecraft-token".into()),
            minecraft_expires_at_unix: i64::MAX,
            xuid: Some("xuid".into()),
        }
    }

    #[test]
    fn placeholder_replacement_is_strict_and_preserves_argument_boundaries() {
        let replacements =
            BTreeMap::from([("name", "Player One".into()), ("token", "private".into())]);
        assert_eq!(
            replace_placeholders("--username=${name}", &replacements).expect("replace"),
            "--username=Player One"
        );
        let error =
            replace_placeholders("${unknown}", &replacements).expect_err("unknown placeholder");
        assert_eq!(
            error.descriptor().code,
            "runtime_argument_placeholder_unknown"
        );
        assert!(replace_placeholders("${name", &replacements).is_err());
    }

    #[test]
    fn legacy_argument_parser_does_not_split_quoted_values() {
        assert_eq!(
            parse_legacy_arguments(r#"--username "Player One" --demo"#).expect("arguments"),
            ["--username", "Player One", "--demo"]
        );
        assert!(parse_legacy_arguments(r#""unterminated"#).is_err());
    }

    #[test]
    fn platform_rules_use_last_matching_rule() {
        let rules = vec![
            ResolvedLaunchRule {
                action: "allow".into(),
                os_name: None,
                os_arch: None,
                has_os_version_constraint: false,
                features: BTreeMap::new(),
            },
            ResolvedLaunchRule {
                action: "disallow".into(),
                os_name: Some(current_os_name().into()),
                os_arch: None,
                has_os_version_constraint: false,
                features: BTreeMap::new(),
            },
        ];
        assert!(!rules_allow(&rules, &BTreeMap::new()));
    }

    #[test]
    fn launch_configuration_rejects_duplicate_case_targets() {
        let launch = ResolvedLaunchConfiguration {
            main_class: "net.minecraft.client.main.Main".into(),
            asset_index_id: "17".into(),
            java_major_version: 21,
            game_arguments: Vec::new(),
            jvm_arguments: Vec::new(),
            classpath_targets: vec!["libraries/A.jar".into(), "libraries/a.jar".into()],
            native_jar_targets: Vec::new(),
            legacy_game_arguments: None,
        };
        assert_eq!(
            validate_launch_configuration(&launch)
                .expect_err("collision")
                .descriptor()
                .code,
            "runtime_launch_target_duplicate"
        );
    }

    #[test]
    fn natives_are_fully_validated_before_create_new_extraction() {
        let fixture = NativeFixture::new();
        let first = build_jar(&[
            ("META-INF/MANIFEST.MF", b"manifest"),
            ("lwjgl.dll", b"native-one"),
        ]);
        let second = build_jar(&[
            ("META-INF/MANIFEST.MF", b"other manifest"),
            ("nested/glfw.dll", b"native-two"),
        ]);
        let first_target = fixture.write_jar("first.jar", &first);
        let second_target = fixture.write_jar("second.jar", &second);
        let lock = fixture.lock(&[
            (first_target, first.clone()),
            (second_target, second.clone()),
        ]);
        let paths = fixture.paths("launch-valid");

        let archives =
            validate_native_archives(&lock, &paths).expect("validate all native archives");
        assert!(
            !paths.natives.absolute().exists(),
            "validation must not create extraction state"
        );
        extract_native_archives(&fixture.registry, &paths, archives).expect("extract natives");
        assert_eq!(
            fs::read(paths.natives.absolute().join("lwjgl.dll")).expect("first native"),
            b"native-one"
        );
        assert_eq!(
            fs::read(paths.natives.absolute().join("nested/glfw.dll")).expect("second native"),
            b"native-two"
        );
        assert!(!paths.natives.absolute().join("META-INF").exists());

        let second_attempt =
            validate_native_archives(&lock, &paths).expect("revalidate immutable archives");
        assert_eq!(
            validation_error_code(extract_native_archives(
                &fixture.registry,
                &paths,
                second_attempt,
            )),
            "runtime_native_target_exists"
        );
    }

    #[test]
    fn archive_entries_reject_zip_slip_ads_case_collisions_and_zip_bombs() {
        assert_eq!(
            validate_single_jar(&[("../escape.dll", b"escape")]),
            "path_traversal"
        );
        assert_eq!(
            validate_single_jar(&[("native.dll:payload", b"ads")]),
            "path_alternate_data_stream"
        );
        assert_eq!(
            validate_single_jar(&[("Native.dll", b"one"), ("native.DLL", b"two")]),
            "component_jar_entry_collision"
        );

        let oversized = vec![0u8; 2 * 1024 * 1024];
        assert_eq!(
            validate_single_jar(&[("compressed.dll", &oversized)]),
            "component_jar_compression_ratio_exceeded"
        );
    }

    #[cfg(windows)]
    #[test]
    fn archive_entries_reject_unicode_normalization_collisions_on_windows() {
        assert_eq!(
            validate_single_jar(&[("ä.dll", b"one"), ("a\u{0308}.dll", b"two")]),
            "component_jar_entry_collision"
        );
    }

    #[test]
    fn archive_entries_reject_unix_symlink_modes() {
        let descriptors = [JarEntryDescriptor {
            relative_path: "native-link".into(),
            is_directory: false,
            compressed_size_bytes: 4,
            uncompressed_size_bytes: 4,
            encrypted: false,
            unix_mode: Some(0o120777),
        }];
        assert_eq!(
            validation_error_code(validate_jar_entries(&descriptors, NATIVE_JAR_LIMITS)),
            "component_jar_symlink_forbidden"
        );
    }

    #[test]
    fn duplicate_or_conflicting_outputs_across_native_jars_are_rejected() {
        let fixture = NativeFixture::new();
        let first = build_jar(&[("shared.dll", b"one")]);
        let second = build_jar(&[("SHARED.DLL", b"two")]);
        let first_target = fixture.write_jar("first.jar", &first);
        let second_target = fixture.write_jar("second.jar", &second);
        let lock = fixture.lock(&[(first_target, first), (second_target, second)]);
        assert_eq!(
            validation_error_code(validate_native_archives(
                &lock,
                &fixture.paths("launch-duplicate"),
            )),
            "runtime_native_entry_duplicate"
        );

        let fixture = NativeFixture::new();
        let first = build_jar(&[("conflict", b"file")]);
        let second = build_jar(&[("conflict/child.dll", b"child")]);
        let first_target = fixture.write_jar("first.jar", &first);
        let second_target = fixture.write_jar("second.jar", &second);
        let lock = fixture.lock(&[(first_target, first), (second_target, second)]);
        assert_eq!(
            validation_error_code(validate_native_archives(
                &lock,
                &fixture.paths("launch-conflict"),
            )),
            "runtime_native_entry_path_conflict"
        );
    }

    #[test]
    fn native_jar_must_still_match_the_active_revision_lock() {
        let fixture = NativeFixture::new();
        let bytes = build_jar(&[("native.dll", b"native")]);
        let target = fixture.write_jar("native.jar", &bytes);
        let mut lock = fixture.lock(&[(target, bytes)]);
        lock.runtime.items[0].sha256 = "b".repeat(64);
        assert_eq!(
            validation_error_code(validate_native_archives(
                &lock,
                &fixture.paths("launch-hash-mismatch"),
            )),
            "runtime_native_jar_hash_mismatch"
        );
        assert!(!fixture
            .paths("launch-hash-mismatch")
            .natives
            .absolute()
            .exists());

        lock.runtime.items[0].sha256 = sha256_hex(b"not-used-after-size-preflight");
        lock.runtime.items[0].size_bytes = NATIVE_JAR_LIMITS.max_total_compressed_bytes + 1;
        assert_eq!(
            validation_error_code(validate_native_archives(
                &lock,
                &fixture.paths("launch-oversized-archive"),
            )),
            "runtime_native_jar_file_too_large"
        );
    }

    #[test]
    fn fabric_component_is_added_from_the_verified_revision_before_main_class() {
        let fixture = NativeFixture::new();
        let component = b"verified component".to_vec();
        let target = "mods/s9lab/s9lab-client.jar".to_string();
        fixture.write_artifact(&target, &component);
        let mut lock = fixture.lock(&[(target.clone(), component)]);
        lock.runtime.runtime.loader = LoaderSelection {
            kind: LoaderKind::Fabric,
            loader_version: Some("0.16.10".into()),
        };
        lock.runtime.items[0].provider_id = ProviderId::S9lab;
        lock.runtime.items[0].logical_id = "s9lab-client".into();
        lock.runtime.items[0].kind = RuntimeArtifactKind::S9labComponent;
        lock.launch.native_jar_targets.clear();
        let paths = fixture.paths("launch-component");
        let account = test_account();
        let session = test_session();
        let request = ProfileLaunchRequest {
            profile_id: fixture.profile_id,
            revision_id: fixture.revision_id,
            lock: &lock,
            java_executable: Path::new(if cfg!(windows) {
                r"C:\Java\bin\java.exe"
            } else {
                "/java/bin/java"
            }),
            memory_mb: 4096,
            secrets: LaunchSecrets {
                account: &account,
                session: &session,
            },
        };

        let arguments = build_launch_arguments(&request, &paths).expect("Fabric arguments");
        let component_path =
            resolve_runtime_target(&fixture.runtime, &target).expect("component path");
        let expected = format!("-Dfabric.addMods={}", component_path.to_string_lossy());
        let component_index = arguments
            .iter()
            .position(|value| value == &expected)
            .unwrap_or_else(|| {
                panic!(
                    "controlled Fabric component argument missing: expected={expected:?}, actual={arguments:?}"
                )
            });
        let main_index = arguments
            .iter()
            .position(|value| value == &lock.launch.main_class)
            .expect("main class");
        assert!(component_index < main_index);
        assert!(
            !paths.instance.join("mods").exists(),
            "the immutable component must not be copied into mutable instance mods"
        );

        let mut conflicting = lock.clone();
        conflicting.launch.jvm_arguments = vec![ResolvedLaunchArgument::Plain {
            value: "-Dfabric.addMods=C:\\uncontrolled".into(),
        }];
        let conflicting_request = ProfileLaunchRequest {
            lock: &conflicting,
            ..request
        };
        assert_eq!(
            validation_error_code(build_launch_arguments(&conflicting_request, &paths)),
            "runtime_component_argument_conflict"
        );
    }

    #[test]
    fn component_launch_fails_closed_for_vanilla_neoforge_and_hash_drift() {
        let fixture = NativeFixture::new();
        let component = b"verified component".to_vec();
        let target = "mods/s9lab/s9lab-client.jar".to_string();
        fixture.write_artifact(&target, &component);
        let mut lock = fixture.lock(&[(target, component)]);
        lock.runtime.items[0].provider_id = ProviderId::S9lab;
        lock.runtime.items[0].logical_id = "s9lab-client".into();
        lock.runtime.items[0].kind = RuntimeArtifactKind::S9labComponent;
        lock.launch.native_jar_targets.clear();
        let paths = fixture.paths("launch-component-closed");

        assert_eq!(
            validation_error_code(resolve_fabric_component_argument(&lock, &paths, ";")),
            "runtime_component_vanilla_forbidden"
        );
        lock.runtime.runtime.loader = LoaderSelection {
            kind: LoaderKind::Neoforge,
            loader_version: Some("21.1.200".into()),
        };
        assert_eq!(
            validation_error_code(resolve_fabric_component_argument(&lock, &paths, ";")),
            "runtime_component_neoforge_launch_unsupported"
        );
        lock.runtime.runtime.loader = LoaderSelection {
            kind: LoaderKind::Fabric,
            loader_version: Some("0.16.10".into()),
        };
        lock.runtime.items[0].sha256 = "b".repeat(64);
        assert_eq!(
            validation_error_code(resolve_fabric_component_argument(&lock, &paths, ";")),
            "runtime_revision_artifact_hash_mismatch"
        );
    }

    #[test]
    fn logging_placeholder_uses_exactly_one_verified_locked_configuration() {
        let fixture = NativeFixture::new();
        let logging = b"<Configuration/>".to_vec();
        let target = "assets/log_configs/client.xml".to_string();
        fixture.write_artifact(&target, &logging);
        let mut lock = fixture.lock(&[(target.clone(), logging)]);
        lock.runtime.items[0].logical_id = "logging:client".into();
        lock.runtime.items[0].kind = RuntimeArtifactKind::LoggingConfiguration;
        lock.launch.native_jar_targets.clear();
        lock.launch.jvm_arguments = vec![ResolvedLaunchArgument::Plain {
            value: "-Dlog4j.configurationFile=${logging_config}".into(),
        }];
        let paths = fixture.paths("launch-logging");
        let account = test_account();
        let session = test_session();
        let request = ProfileLaunchRequest {
            profile_id: fixture.profile_id,
            revision_id: fixture.revision_id,
            lock: &lock,
            java_executable: Path::new(if cfg!(windows) {
                r"C:\Java\bin\java.exe"
            } else {
                "/java/bin/java"
            }),
            memory_mb: 4096,
            secrets: LaunchSecrets {
                account: &account,
                session: &session,
            },
        };
        let arguments = build_launch_arguments(&request, &paths).expect("logging arguments");
        let logging_path = resolve_runtime_target(&fixture.runtime, &target).expect("logging path");
        assert!(
            arguments.iter().any(|value| {
                value
                    == &format!(
                        "-Dlog4j.configurationFile={}",
                        logging_path.to_string_lossy()
                    )
            }),
            "verified logging argument missing: {arguments:?}"
        );

        let mut missing = lock.clone();
        missing.runtime.items[0].kind = RuntimeArtifactKind::MinecraftLibrary;
        let missing_request = ProfileLaunchRequest {
            lock: &missing,
            ..request
        };
        assert_eq!(
            validation_error_code(build_launch_arguments(&missing_request, &paths)),
            "runtime_argument_placeholder_unknown"
        );
    }

    #[test]
    fn logging_placeholder_rejects_multiple_locked_configurations() {
        let fixture = NativeFixture::new();
        let first = b"<First/>".to_vec();
        let second = b"<Second/>".to_vec();
        let first_target = "assets/log_configs/first.xml".to_string();
        let second_target = "assets/log_configs/second.xml".to_string();
        fixture.write_artifact(&first_target, &first);
        fixture.write_artifact(&second_target, &second);
        let mut lock = fixture.lock(&[(first_target, first), (second_target, second)]);
        for (index, item) in lock.runtime.items.iter_mut().enumerate() {
            item.logical_id = format!("logging:{index}");
            item.kind = RuntimeArtifactKind::LoggingConfiguration;
        }
        lock.launch.native_jar_targets.clear();
        lock.launch.jvm_arguments = vec![ResolvedLaunchArgument::Plain {
            value: "-Dlog4j.configurationFile=${logging_config}".into(),
        }];
        let paths = fixture.paths("launch-logging-ambiguous");
        let account = test_account();
        let session = test_session();
        let request = ProfileLaunchRequest {
            profile_id: fixture.profile_id,
            revision_id: fixture.revision_id,
            lock: &lock,
            java_executable: Path::new(if cfg!(windows) {
                r"C:\Java\bin\java.exe"
            } else {
                "/java/bin/java"
            }),
            memory_mb: 4096,
            secrets: LaunchSecrets {
                account: &account,
                session: &session,
            },
        };
        assert_eq!(
            validation_error_code(build_launch_arguments(&request, &paths)),
            "runtime_logging_configuration_ambiguous"
        );
    }

    #[tokio::test]
    async fn native_validation_failure_never_crosses_the_process_spawn_boundary() {
        let fixture = NativeFixture::new();
        let bytes = build_jar(&[("native.dll", b"native")]);
        let target = fixture.write_jar("native.jar", &bytes);
        let mut lock = fixture.lock(&[(target, bytes)]);
        lock.runtime.items[0].sha256 = "b".repeat(64);
        let account = test_account();
        let session = test_session();
        let java = if cfg!(windows) {
            PathBuf::from(std::env::var_os("COMSPEC").expect("COMSPEC"))
        } else {
            PathBuf::from("/bin/sh")
        };
        let manager = ProfileProcessManager::default();

        let error = manager
            .launch(
                &fixture.registry,
                ProfileLaunchRequest {
                    profile_id: fixture.profile_id,
                    revision_id: fixture.revision_id,
                    lock: &lock,
                    java_executable: &java,
                    memory_mb: 4096,
                    secrets: LaunchSecrets {
                        account: &account,
                        session: &session,
                    },
                },
            )
            .await
            .expect_err("invalid native must fail before process spawn");
        assert_eq!(error.descriptor().code, "runtime_native_jar_hash_mismatch");
        assert!(manager.running.lock().await.is_empty());
        assert!(!fixture
            .root
            .join("profiles/profile-native/instance/.s9lab/natives")
            .exists());
    }

    #[tokio::test]
    async fn stop_targets_only_the_requested_launch_id() {
        fn sleeper() -> (Child, ManagedProcessTree) {
            let mut command = if cfg!(windows) {
                let mut command = tokio::process::Command::new("cmd.exe");
                command.args(["/D", "/C", "ping -n 30 127.0.0.1 >nul"]);
                command
            } else {
                let mut command = tokio::process::Command::new("sh");
                command.args(["-c", "sleep 30"]);
                command
            };
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            configure_windows_process_group(&mut command);
            let child = command.spawn().expect("spawn sleeper");
            let process_tree =
                ManagedProcessTree::attach_and_resume(&child).expect("contain sleeper");
            (child, process_tree)
        }

        fn status(launch_id: &str, profile_id: &str) -> ProfileLaunchStatus {
            ProfileLaunchStatus {
                launch_id: launch_id.into(),
                profile_id: profile_id.into(),
                state: ProfileLaunchState::Running,
                process_id: None,
                account_name: "Player".into(),
                started_at_unix: 1,
                exit_code: None,
            }
        }

        let manager = ProfileProcessManager::default();
        let (first_child, first_process_tree) = sleeper();
        manager.running.lock().await.insert(
            "launch-first".into(),
            RunningProfile {
                child: first_child,
                process_tree: first_process_tree,
                status: status("launch-first", "profile-first"),
            },
        );
        let (second_child, second_process_tree) = sleeper();
        manager.running.lock().await.insert(
            "launch-second".into(),
            RunningProfile {
                child: second_child,
                process_tree: second_process_tree,
                status: status("launch-second", "profile-second"),
            },
        );

        let stopped = manager.stop("launch-first").await.expect("stop first");
        assert_eq!(stopped.launch_id, "launch-first");
        let statuses = manager.statuses().await.expect("remaining statuses");
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].launch_id, "launch-second");
        assert_eq!(
            manager
                .stop("launch-first/launch-second")
                .await
                .expect_err("combined identifier is invalid")
                .descriptor()
                .code,
            "runtime_launch_id_invalid"
        );
        manager.stop("launch-second").await.expect("stop second");
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_job_object_contains_and_terminates_the_descendant_tree() {
        use std::time::Duration;

        let mut command = tokio::process::Command::new("cmd.exe");
        command
            .args(["/D", "/C", "ping -n 30 127.0.0.1 >nul"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_windows_process_group(&mut command);
        let mut child = command.spawn().expect("spawn suspended process tree");
        let process_tree =
            ManagedProcessTree::attach_and_resume(&child).expect("assign and resume job");

        let mut descendant_observed = false;
        for _ in 0..40 {
            if process_tree
                .active_process_count()
                .expect("query active job processes")
                >= 2
            {
                descendant_observed = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            descendant_observed,
            "the cmd-owned ping descendant must be contained in the same job"
        );

        process_tree.terminate().expect("terminate job tree");
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("job termination timeout")
            .expect("wait for root process");
        for _ in 0..40 {
            if process_tree
                .active_process_count()
                .expect("query terminated job")
                == 0
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("job termination left a descendant process alive");
    }
}
