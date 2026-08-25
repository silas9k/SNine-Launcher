use super::{
    provider::{validate_url, validate_version_identifier, ControlledProvider},
    resolver::{
        LaunchArgument, LaunchArgumentValue, ResolvedLoader, RuntimeArtifactKind,
        RuntimeArtifactSource,
    },
};
#[cfg(test)]
use crate::security::{fs as secure_fs, PathRegistry};
use crate::{
    error::{AppError, AppResult},
    runtime::{validate_jar_entries, JarEntryDescriptor, JarValidationLimits},
    security::{
        paths::{normalize_relative_path, validate_existing_chain},
        SecurePath,
    },
};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
#[cfg(test)]
use std::{ffi::OsString, fs, io::Write, time::Duration};

const INSTALL_PROFILE_ENTRY: &str = "install_profile.json";
const VERSION_PROFILE_ENTRY: &str = "version.json";
const MAX_INSTALLER_BYTES: u64 = 1_073_741_824;
const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;
const MAX_INSTALLER_ENTRIES: usize = 25_000;
const MAX_INSTALLER_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_INSTALLER_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PROCESSORS: usize = 256;
const MAX_PROCESSOR_CLASSPATH: usize = 512;
const MAX_PROCESSOR_ARGUMENTS: usize = 1_024;
const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
#[cfg(test)]
const MAX_PROCESS_OUTPUT_BYTES: usize = 64 * 1024;
#[cfg(test)]
const PROCESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoForgeInstallPlan {
    pub minecraft_version: String,
    pub loader_version: String,
    pub profile_id: String,
    pub main_class: String,
    pub game_arguments: Vec<LaunchArgument>,
    pub jvm_arguments: Vec<LaunchArgument>,
    pub installer_source: RuntimeArtifactSource,
    pub external_artifacts: Vec<RuntimeArtifactSource>,
    pub embedded_artifacts: Vec<NeoForgeEmbeddedArtifact>,
    pub runtime_library_targets: Vec<String>,
    pub data: BTreeMap<String, NeoForgeDataReference>,
    pub processors: Vec<NeoForgeProcessorPlan>,
    pub installer_sha256: String,
    pub installer_size_bytes: u64,
    pub plan_sha256: String,
}

impl NeoForgeInstallPlan {
    pub fn resolved_loader(&self) -> ResolvedLoader {
        let runtime_targets = self
            .runtime_library_targets
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        ResolvedLoader {
            loader_version: self.loader_version.clone(),
            profile_id: self.profile_id.clone(),
            main_class: self.main_class.clone(),
            artifacts: self
                .external_artifacts
                .iter()
                .filter(|artifact| runtime_targets.contains(&artifact.target_relative_path))
                .cloned()
                .collect(),
            game_arguments: self.game_arguments.clone(),
            jvm_arguments: self.jvm_arguments.clone(),
        }
    }

    pub fn execution_readiness(&self) -> NeoForgeExecutionReadiness {
        match validate_execution_readiness(self) {
            Ok(()) => NeoForgeExecutionReadiness {
                ready: true,
                blocker_code: None,
            },
            Err(error) => NeoForgeExecutionReadiness {
                ready: false,
                blocker_code: Some(error.descriptor().code),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoForgeExecutionReadiness {
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoForgeEmbeddedArtifact {
    pub coordinate: String,
    pub installer_entry: String,
    pub target_relative_path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NeoForgeArtifactAvailability {
    External,
    Embedded,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NeoForgeDataReference {
    Maven {
        coordinate: String,
        target_relative_path: String,
        availability: NeoForgeArtifactAvailability,
    },
    InstallerEntry {
        installer_entry: String,
        materialized_relative_path: String,
        size_bytes: u64,
        sha256: String,
    },
    Literal {
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoForgeProcessorArtifact {
    pub coordinate: String,
    pub target_relative_path: String,
    pub availability: NeoForgeArtifactAvailability,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoForgeProcessorPlan {
    pub index: usize,
    pub executable_jar: NeoForgeProcessorArtifact,
    pub classpath: Vec<NeoForgeProcessorArtifact>,
    pub arguments: Vec<NeoForgeProcessorValue>,
    pub outputs: Vec<NeoForgeProcessorOutput>,
    pub plan_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NeoForgeProcessorValue {
    Literal {
        value: String,
    },
    Data {
        reference: NeoForgeDataReference,
    },
    Installer,
    MinecraftClient {
        target_relative_path: String,
    },
    ProfileRoot,
    ProfileRelative {
        relative_path: String,
        directory: bool,
    },
    Side {
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NeoForgeDigestAlgorithm {
    Sha1,
    Sha256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoForgeProcessorOutput {
    pub target: NeoForgeProcessorValue,
    pub algorithm: NeoForgeDigestAlgorithm,
    pub digest: String,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct NeoForgeVerifiedFile {
    pub path: SecurePath,
    pub size_bytes: u64,
    pub sha256: String,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct NeoForgeExecutionContext {
    pub java: SecurePath,
    pub staging: SecurePath,
    pub installer: NeoForgeVerifiedFile,
    pub minecraft_client: NeoForgeVerifiedFile,
    pub artifacts: BTreeMap<String, NeoForgeVerifiedFile>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeoForgeSandboxCapabilities {
    pub no_network: bool,
    pub no_shell: bool,
    pub clears_environment: bool,
    pub bounded_output: bool,
    pub process_tree_timeout: bool,
    pub exact_write_allowlist: bool,
}

#[cfg(test)]
impl NeoForgeSandboxCapabilities {
    fn is_strict(self) -> bool {
        self.no_network
            && self.no_shell
            && self.clears_environment
            && self.bounded_output
            && self.process_tree_timeout
            && self.exact_write_allowlist
    }
}

#[cfg(test)]
#[derive(Debug)]
pub struct NeoForgeSandboxInvocation {
    executable: PathBuf,
    current_directory: PathBuf,
    arguments: Vec<OsString>,
    writable_outputs: Vec<PathBuf>,
    timeout: Duration,
    maximum_output_bytes: usize,
}

#[cfg(test)]
impl NeoForgeSandboxInvocation {
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn current_directory(&self) -> &Path {
        &self.current_directory
    }

    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    pub fn writable_outputs(&self) -> &[PathBuf] {
        &self.writable_outputs
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn maximum_output_bytes(&self) -> usize {
        self.maximum_output_bytes
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeoForgeSandboxResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[cfg(test)]
pub trait NeoForgeProcessSandbox: Send + Sync {
    fn capabilities(&self) -> NeoForgeSandboxCapabilities;

    fn execute(&self, invocation: &NeoForgeSandboxInvocation) -> AppResult<NeoForgeSandboxResult>;
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NoNeoForgeProcessSandbox;

#[cfg(test)]
impl NeoForgeProcessSandbox for NoNeoForgeProcessSandbox {
    fn capabilities(&self) -> NeoForgeSandboxCapabilities {
        NeoForgeSandboxCapabilities {
            no_network: false,
            no_shell: true,
            clears_environment: true,
            bounded_output: true,
            process_tree_timeout: false,
            exact_write_allowlist: false,
        }
    }

    fn execute(&self, _invocation: &NeoForgeSandboxInvocation) -> AppResult<NeoForgeSandboxResult> {
        Err(AppError::coded(
            "runtime_neoforge_process_sandbox_unconfigured",
        ))
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeoForgeProcessorExecution {
    pub processor_index: usize,
    pub exit_code: i32,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub verified_outputs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstallProfileDocument {
    spec: u32,
    profile: String,
    version: String,
    minecraft: String,
    json: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    data: BTreeMap<String, SideDataValue>,
    #[serde(default)]
    processors: Vec<ProcessorDocument>,
    #[serde(default)]
    libraries: Vec<LibraryDocument>,
    #[serde(default, rename = "_comment_")]
    _comment: Option<serde_json::Value>,
    #[serde(default, rename = "icon")]
    _icon: Option<String>,
    #[serde(default, rename = "logo")]
    _logo: Option<String>,
    #[serde(default, rename = "welcome")]
    _welcome: Option<String>,
    #[serde(default, rename = "mirrorList")]
    mirror_list: Option<String>,
    #[serde(default, rename = "hideExtract")]
    _hide_extract: Option<bool>,
    #[serde(default, rename = "serverJarPath")]
    server_jar_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VersionProfileDocument {
    id: String,
    inherits_from: String,
    main_class: String,
    #[serde(rename = "type")]
    release_type: String,
    #[serde(default)]
    arguments: VersionArguments,
    #[serde(default)]
    libraries: Vec<LibraryDocument>,
    #[serde(default, rename = "time")]
    _time: Option<String>,
    #[serde(default, rename = "releaseTime")]
    _release_time: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct VersionArguments {
    #[serde(default)]
    game: Vec<LaunchArgument>,
    #[serde(default)]
    jvm: Vec<LaunchArgument>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SideDataValue {
    client: String,
    #[serde(default)]
    server: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessorDocument {
    #[serde(default)]
    sides: Vec<String>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryDocument {
    name: String,
    #[serde(default)]
    downloads: Option<LibraryDownloads>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryDownloads {
    artifact: LibraryDownloadArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryDownloadArtifact {
    path: String,
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MavenCoordinate {
    canonical: String,
    path: String,
}

#[derive(Debug, Clone)]
struct LibrarySpec {
    coordinate: MavenCoordinate,
    download: Option<LibraryDownloadArtifact>,
}

#[derive(Debug, Clone)]
struct ArtifactBinding {
    processor: NeoForgeProcessorArtifact,
}

#[derive(Debug, Default)]
struct ArtifactCatalog {
    bindings: BTreeMap<String, ArtifactBinding>,
    external: Vec<RuntimeArtifactSource>,
    embedded: Vec<NeoForgeEmbeddedArtifact>,
}

#[derive(Debug, Clone)]
struct ArchiveEntryMetadata {
    index: usize,
    is_directory: bool,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct EntryDigests {
    size_bytes: u64,
    sha1: String,
    sha256: String,
}

/// Parses an already-downloaded and SHA-256-pinned NeoForge installer without
/// extracting files, performing network requests, launching Java, or mutating
/// a profile.
pub fn inspect_verified_installer(
    installer: &SecurePath,
    pinned_source: &RuntimeArtifactSource,
    requested_minecraft_version: &str,
    requested_loader_version: &str,
) -> AppResult<NeoForgeInstallPlan> {
    validate_installer_source(
        pinned_source,
        requested_minecraft_version,
        requested_loader_version,
    )?;
    validate_existing_chain(installer.anchor(), installer.absolute())?;
    let mut file = File::open(installer.absolute())?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(AppError::coded("runtime_neoforge_installer_not_file"));
    }
    if metadata.len() != pinned_source.size_bytes {
        return Err(AppError::coded("runtime_neoforge_installer_size_mismatch"));
    }
    let installer_digests = hash_reader(&mut file)?;
    let expected_sha256 = pinned_source
        .sha256
        .as_deref()
        .ok_or_else(|| AppError::coded("runtime_neoforge_installer_sha256_required"))?;
    if installer_digests.sha256 != expected_sha256 {
        return Err(AppError::coded("runtime_neoforge_installer_hash_mismatch"));
    }
    file.seek(SeekFrom::Start(0))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| AppError::coded("runtime_neoforge_installer_invalid"))?;
    let entries = validate_installer_archive(&mut archive)?;
    let install_profile: InstallProfileDocument =
        read_json_entry(&mut archive, &entries, INSTALL_PROFILE_ENTRY)?;
    let version_profile: VersionProfileDocument =
        read_json_entry(&mut archive, &entries, VERSION_PROFILE_ENTRY)?;
    validate_profile_identity(
        &install_profile,
        &version_profile,
        requested_minecraft_version,
        requested_loader_version,
    )?;
    validate_launch_arguments(&version_profile.arguments.game)?;
    validate_launch_arguments(&version_profile.arguments.jvm)?;

    let (library_specs, runtime_coordinates) =
        merge_library_documents(&install_profile.libraries, &version_profile.libraries)?;
    let catalog = resolve_artifact_catalog(&mut archive, &entries, library_specs)?;
    let mut entry_digests = BTreeMap::new();
    let data = resolve_data_map(
        &install_profile.data,
        &catalog,
        &mut archive,
        &entries,
        &mut entry_digests,
    )?;
    let processors = resolve_processors(
        &install_profile.processors,
        &data,
        &catalog,
        &mut archive,
        &entries,
        &mut entry_digests,
        requested_minecraft_version,
    )?;
    let runtime_library_targets = runtime_coordinates
        .iter()
        .map(|coordinate| {
            catalog
                .bindings
                .get(coordinate)
                .map(|binding| binding.processor.target_relative_path.clone())
                .ok_or_else(|| AppError::coded("runtime_neoforge_runtime_library_missing"))
        })
        .collect::<AppResult<Vec<_>>>()?;

    let mut plan = NeoForgeInstallPlan {
        minecraft_version: requested_minecraft_version.to_string(),
        loader_version: requested_loader_version.to_string(),
        profile_id: version_profile.id,
        main_class: version_profile.main_class,
        game_arguments: version_profile.arguments.game,
        jvm_arguments: version_profile.arguments.jvm,
        installer_source: pinned_source.clone(),
        external_artifacts: catalog.external,
        embedded_artifacts: catalog.embedded,
        runtime_library_targets,
        data,
        processors,
        installer_sha256: installer_digests.sha256,
        installer_size_bytes: installer_digests.size_bytes,
        plan_sha256: String::new(),
    };
    plan.plan_sha256 = compute_plan_hash(&plan)?;
    Ok(plan)
}

/// Materializes only installer-owned entries into a registered operation
/// staging directory. Every target is create-new and content-address verified.
#[cfg(test)]
pub fn materialize_installer_entries(
    registry: &PathRegistry,
    plan: &NeoForgeInstallPlan,
    installer: &NeoForgeVerifiedFile,
    staging: &SecurePath,
) -> AppResult<BTreeMap<String, NeoForgeVerifiedFile>> {
    validate_staging_directory(staging)?;
    validate_verified_file(installer)?;
    if installer.size_bytes != plan.installer_size_bytes
        || installer.sha256 != plan.installer_sha256
    {
        return Err(AppError::coded(
            "runtime_neoforge_installer_context_mismatch",
        ));
    }

    let file = File::open(installer.path.absolute())?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| AppError::coded("runtime_neoforge_installer_invalid"))?;
    let entries = validate_installer_archive(&mut archive)?;
    let mut requests = BTreeMap::<String, (String, u64, String)>::new();
    for embedded in &plan.embedded_artifacts {
        insert_materialization_request(
            &mut requests,
            &embedded.target_relative_path,
            &embedded.installer_entry,
            embedded.size_bytes,
            &embedded.sha256,
        )?;
    }
    for reference in plan.data.values() {
        insert_data_materialization_request(&mut requests, reference)?;
    }
    for processor in &plan.processors {
        for argument in &processor.arguments {
            if let NeoForgeProcessorValue::Data { reference } = argument {
                insert_data_materialization_request(&mut requests, reference)?;
            }
        }
        for output in &processor.outputs {
            if let NeoForgeProcessorValue::Data { reference } = &output.target {
                insert_data_materialization_request(&mut requests, reference)?;
            }
        }
    }

    let mut materialized = BTreeMap::new();
    let mut created = Vec::new();
    let result = (|| {
        for (target, (entry_name, expected_size, expected_sha256)) in requests {
            let relative = staging.relative().join(&target);
            let destination = registry.resolve(staging.root_id(), relative)?;
            if destination.absolute().exists() {
                return Err(AppError::coded(
                    "runtime_neoforge_materialization_target_exists",
                ));
            }
            let metadata = entries
                .get(&entry_name)
                .filter(|metadata| !metadata.is_directory)
                .ok_or_else(|| AppError::coded("runtime_neoforge_installer_entry_missing"))?;
            if metadata.size_bytes != expected_size {
                return Err(AppError::coded("runtime_neoforge_embedded_size_mismatch"));
            }
            let mut input = archive
                .by_index(metadata.index)
                .map_err(|_| AppError::coded("runtime_neoforge_installer_invalid"))?;
            let mut output = secure_fs::open_new_file(&destination)?;
            created.push(destination.clone());
            let mut sha256 = Sha256::new();
            let mut size = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = input
                    .read(&mut buffer)
                    .map_err(|_| AppError::coded("runtime_neoforge_installer_entry_read_failed"))?;
                if read == 0 {
                    break;
                }
                size = size
                    .checked_add(read as u64)
                    .ok_or_else(|| AppError::coded("runtime_neoforge_entry_size_overflow"))?;
                if size > expected_size {
                    return Err(AppError::coded("runtime_neoforge_embedded_size_mismatch"));
                }
                sha256.update(&buffer[..read]);
                output.write_all(&buffer[..read])?;
            }
            output.sync_all()?;
            if size != expected_size || hex::encode(sha256.finalize()) != expected_sha256 {
                return Err(AppError::coded("runtime_neoforge_embedded_hash_mismatch"));
            }
            validate_existing_chain(destination.anchor(), destination.absolute())?;
            materialized.insert(
                target,
                NeoForgeVerifiedFile {
                    path: destination,
                    size_bytes: expected_size,
                    sha256: expected_sha256,
                },
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        for path in created.into_iter().rev() {
            let _ = secure_fs::remove_tree(&path);
        }
        return Err(error);
    }
    Ok(materialized)
}

/// Executes plans only through an explicitly supplied process sandbox. The
/// default implementation is unavailable; callers cannot silently fall back
/// to an unrestricted `java` process.
#[cfg(test)]
pub fn execute_client_processors(
    registry: &PathRegistry,
    plan: &NeoForgeInstallPlan,
    context: &NeoForgeExecutionContext,
    sandbox: &dyn NeoForgeProcessSandbox,
) -> AppResult<Vec<NeoForgeProcessorExecution>> {
    validate_execution_readiness(plan)?;
    if !sandbox.capabilities().is_strict() {
        return Err(AppError::coded(
            "runtime_neoforge_process_sandbox_inadequate",
        ));
    }
    validate_execution_context(plan, context)?;

    let mut executions = Vec::with_capacity(plan.processors.len());
    for processor in &plan.processors {
        let output_paths = resolve_output_paths(registry, context, processor)?;
        for (_, path, _, _) in &output_paths {
            if path.absolute().exists() {
                return Err(AppError::coded("runtime_neoforge_output_exists"));
            }
            secure_fs::create_parent_directories(path)?;
        }

        let executable_jar = resolve_processor_artifact(context, &processor.executable_jar)?;
        let main_class = processor_main_class(executable_jar.path.absolute())?;
        let classpath = processor
            .classpath
            .iter()
            .map(|artifact| {
                resolve_processor_artifact(context, artifact)
                    .map(|file| file.path.absolute().to_path_buf())
            })
            .collect::<AppResult<Vec<_>>>()?;
        let classpath = std::env::join_paths(classpath)
            .map_err(|_| AppError::coded("runtime_neoforge_classpath_invalid"))?;
        let current_outputs = output_paths
            .iter()
            .map(|(target, path, _, _)| (target.clone(), path.absolute().to_path_buf()))
            .collect::<BTreeMap<_, _>>();
        let mut arguments = vec![OsString::from("-cp"), classpath, OsString::from(main_class)];
        for value in &processor.arguments {
            arguments.push(resolve_processor_value(
                registry,
                context,
                value,
                &current_outputs,
            )?);
        }
        let invocation = NeoForgeSandboxInvocation {
            executable: context.java.absolute().to_path_buf(),
            current_directory: context.staging.absolute().to_path_buf(),
            arguments,
            writable_outputs: output_paths
                .iter()
                .map(|(_, path, _, _)| path.absolute().to_path_buf())
                .collect(),
            timeout: PROCESS_TIMEOUT,
            maximum_output_bytes: MAX_PROCESS_OUTPUT_BYTES,
        };
        let result = sandbox.execute(&invocation)?;
        if result.stdout.len() > MAX_PROCESS_OUTPUT_BYTES
            || result.stderr.len() > MAX_PROCESS_OUTPUT_BYTES
        {
            return Err(AppError::coded(
                "runtime_neoforge_processor_output_limit_exceeded",
            ));
        }
        if result.exit_code != 0 {
            return Err(AppError::coded_with(
                "runtime_neoforge_processor_failed",
                [
                    ("processorIndex", processor.index.to_string()),
                    ("exitCode", result.exit_code.to_string()),
                ],
            ));
        }

        let mut verified_outputs = Vec::new();
        for (target, path, algorithm, expected) in output_paths {
            validate_existing_chain(path.anchor(), path.absolute())?;
            let mut file = File::open(path.absolute())?;
            let digests = hash_reader(&mut file)?;
            let actual = match algorithm {
                NeoForgeDigestAlgorithm::Sha1 => digests.sha1,
                NeoForgeDigestAlgorithm::Sha256 => digests.sha256,
            };
            if actual != expected {
                return Err(AppError::coded_with(
                    "runtime_neoforge_processor_output_hash_mismatch",
                    [
                        ("processorIndex", processor.index.to_string()),
                        ("target", target.clone()),
                    ],
                ));
            }
            verified_outputs.push(target);
        }
        executions.push(NeoForgeProcessorExecution {
            processor_index: processor.index,
            exit_code: result.exit_code,
            stdout_bytes: result.stdout.len(),
            stderr_bytes: result.stderr.len(),
            verified_outputs,
        });
    }
    Ok(executions)
}

fn validate_installer_source(
    source: &RuntimeArtifactSource,
    minecraft_version: &str,
    loader_version: &str,
) -> AppResult<()> {
    validate_version_identifier(minecraft_version)?;
    validate_version_identifier(loader_version)?;
    if !loader_matches_minecraft(minecraft_version, loader_version) {
        return Err(AppError::coded("runtime_loader_version_incompatible"));
    }
    if source.kind != RuntimeArtifactKind::Installer || source.provider != "neoforge" {
        return Err(AppError::coded("runtime_neoforge_installer_source_invalid"));
    }
    if source.size_bytes == 0 || source.size_bytes > MAX_INSTALLER_BYTES {
        return Err(AppError::coded("runtime_neoforge_installer_size_invalid"));
    }
    validate_sha256(
        source
            .sha256
            .as_deref()
            .ok_or_else(|| AppError::coded("runtime_neoforge_installer_sha256_required"))?,
    )?;
    let coordinate = parse_maven_coordinate(&format!(
        "net.neoforged:neoforge:{loader_version}:installer"
    ))?;
    let expected_id = format!("net.neoforged:neoforge:{loader_version}:installer");
    let expected_target = format!("installers/neoforge/{loader_version}.jar");
    let expected_url = format!("https://maven.neoforged.net/releases/{}", coordinate.path);
    if source.logical_id != expected_id
        || source.target_relative_path != expected_target
        || source.url != expected_url
    {
        return Err(AppError::coded(
            "runtime_neoforge_installer_source_identity_mismatch",
        ));
    }
    let url = validate_url(ControlledProvider::NeoforgeMaven, &source.url)?;
    if url.query().is_some() {
        return Err(AppError::coded("runtime_neoforge_url_query_forbidden"));
    }
    validate_canonical_relative(&source.target_relative_path)?;
    Ok(())
}

fn validate_installer_archive<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> AppResult<BTreeMap<String, ArchiveEntryMetadata>> {
    let mut descriptors = Vec::with_capacity(archive.len());
    let mut entries = BTreeMap::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::coded("runtime_neoforge_installer_invalid"))?;
        let name = std::str::from_utf8(entry.name_raw())
            .map_err(|_| AppError::coded("runtime_neoforge_entry_name_invalid_utf8"))?
            .to_string();
        let is_directory = entry.is_dir();
        if is_directory && (entry.size() != 0 || entry.compressed_size() > 64) {
            return Err(AppError::coded("runtime_neoforge_directory_entry_invalid"));
        }
        descriptors.push(JarEntryDescriptor {
            relative_path: name.clone(),
            is_directory,
            compressed_size_bytes: if is_directory {
                0
            } else {
                entry.compressed_size()
            },
            uncompressed_size_bytes: entry.size(),
            encrypted: entry.encrypted(),
            unix_mode: entry.unix_mode(),
        });
        if entries
            .insert(
                name,
                ArchiveEntryMetadata {
                    index,
                    is_directory,
                    size_bytes: entry.size(),
                },
            )
            .is_some()
        {
            return Err(AppError::coded("runtime_neoforge_entry_duplicate"));
        }
    }
    validate_jar_entries(
        &descriptors,
        JarValidationLimits {
            max_entries: MAX_INSTALLER_ENTRIES,
            max_total_compressed_bytes: MAX_INSTALLER_BYTES,
            max_entry_uncompressed_bytes: MAX_INSTALLER_ENTRY_BYTES,
            max_total_uncompressed_bytes: MAX_INSTALLER_EXPANDED_BYTES,
            max_compression_ratio: 200,
        },
    )?;
    Ok(entries)
}

fn read_json_entry<T: for<'de> Deserialize<'de>, R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    entries: &BTreeMap<String, ArchiveEntryMetadata>,
    name: &str,
) -> AppResult<T> {
    let metadata = entries
        .get(name)
        .filter(|metadata| !metadata.is_directory)
        .ok_or_else(|| AppError::coded("runtime_neoforge_metadata_missing"))?;
    if metadata.size_bytes == 0 || metadata.size_bytes > MAX_METADATA_BYTES {
        return Err(AppError::coded("runtime_neoforge_metadata_size_invalid"));
    }
    let mut entry = archive
        .by_index(metadata.index)
        .map_err(|_| AppError::coded("runtime_neoforge_installer_invalid"))?;
    let mut bytes = Vec::with_capacity(metadata.size_bytes as usize);
    entry
        .by_ref()
        .take(MAX_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::coded("runtime_neoforge_metadata_read_failed"))?;
    if bytes.len() as u64 != metadata.size_bytes || bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err(AppError::coded("runtime_neoforge_metadata_size_invalid"));
    }
    serde_json::from_slice(&bytes).map_err(|_| AppError::coded("runtime_neoforge_metadata_invalid"))
}

fn validate_profile_identity(
    install: &InstallProfileDocument,
    version: &VersionProfileDocument,
    minecraft_version: &str,
    loader_version: &str,
) -> AppResult<()> {
    if install.spec != 1 {
        return Err(AppError::coded(
            "runtime_neoforge_install_profile_spec_unsupported",
        ));
    }
    if install.profile != "NeoForge"
        || install.version != format!("neoforge-{loader_version}")
        || version.id != install.version
    {
        return Err(AppError::coded(
            "runtime_neoforge_version_identity_mismatch",
        ));
    }
    if install.minecraft != minecraft_version || version.inherits_from != minecraft_version {
        return Err(AppError::coded(
            "runtime_neoforge_minecraft_identity_mismatch",
        ));
    }
    if install.json != "/version.json" || version.release_type != "release" {
        return Err(AppError::coded("runtime_neoforge_profile_metadata_invalid"));
    }
    if install
        .mirror_list
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        return Err(AppError::coded("runtime_neoforge_mirror_list_forbidden"));
    }
    if let Some(path) = install.path.as_deref() {
        let coordinate = parse_maven_coordinate(path)?;
        if coordinate.canonical != format!("net.neoforged:neoforge:{loader_version}") {
            return Err(AppError::coded("runtime_neoforge_profile_path_invalid"));
        }
    }
    if let Some(path) = install.server_jar_path.as_deref() {
        let trimmed = path.trim_end_matches('/');
        validate_canonical_relative(trimmed)?;
    }
    validate_java_class(&version.main_class)
}

fn merge_library_documents(
    install: &[LibraryDocument],
    runtime: &[LibraryDocument],
) -> AppResult<(BTreeMap<String, LibrarySpec>, Vec<String>)> {
    let mut specs = BTreeMap::new();
    let mut install_seen = BTreeSet::new();
    for library in install {
        let spec = parse_library(library)?;
        if !install_seen.insert(spec.coordinate.canonical.clone()) {
            return Err(AppError::coded("runtime_neoforge_library_duplicate"));
        }
        merge_library_spec(&mut specs, spec)?;
    }
    let mut runtime_seen = BTreeSet::new();
    let mut runtime_coordinates = Vec::with_capacity(runtime.len());
    for library in runtime {
        let spec = parse_library(library)?;
        if !runtime_seen.insert(spec.coordinate.canonical.clone()) {
            return Err(AppError::coded(
                "runtime_neoforge_runtime_library_duplicate",
            ));
        }
        runtime_coordinates.push(spec.coordinate.canonical.clone());
        merge_library_spec(&mut specs, spec)?;
    }
    if specs.is_empty() || runtime_coordinates.is_empty() {
        return Err(AppError::coded("runtime_neoforge_libraries_missing"));
    }
    Ok((specs, runtime_coordinates))
}

fn parse_library(library: &LibraryDocument) -> AppResult<LibrarySpec> {
    let coordinate = parse_maven_coordinate(&library.name)?;
    let download = library
        .downloads
        .as_ref()
        .map(|downloads| downloads.artifact.clone());
    if let Some(download) = &download {
        validate_library_download(&coordinate, download)?;
    }
    Ok(LibrarySpec {
        coordinate,
        download,
    })
}

fn merge_library_spec(
    specs: &mut BTreeMap<String, LibrarySpec>,
    mut candidate: LibrarySpec,
) -> AppResult<()> {
    if let Some(existing) = specs.get_mut(&candidate.coordinate.canonical) {
        if existing.coordinate.path != candidate.coordinate.path {
            return Err(AppError::coded("runtime_neoforge_library_path_conflict"));
        }
        match (&existing.download, candidate.download.take()) {
            (Some(left), Some(right)) if left != &right => {
                return Err(AppError::coded(
                    "runtime_neoforge_library_metadata_conflict",
                ));
            }
            (None, Some(download)) => existing.download = Some(download),
            _ => {}
        }
    } else {
        specs.insert(candidate.coordinate.canonical.clone(), candidate);
    }
    Ok(())
}

fn validate_library_download(
    coordinate: &MavenCoordinate,
    download: &LibraryDownloadArtifact,
) -> AppResult<()> {
    if download.path != coordinate.path {
        return Err(AppError::coded("runtime_neoforge_library_path_mismatch"));
    }
    validate_canonical_relative(&download.path)?;
    validate_sha1(&download.sha1)?;
    if download.size == 0 || download.size > MAX_INSTALLER_BYTES {
        return Err(AppError::coded("runtime_neoforge_library_size_invalid"));
    }
    let parsed =
        reqwest::Url::parse(&download.url).map_err(|_| AppError::coded("runtime_url_invalid"))?;
    let (provider, expected_path) = match parsed.host_str() {
        Some("maven.neoforged.net") => (
            ControlledProvider::NeoforgeMaven,
            format!("/releases/{}", coordinate.path),
        ),
        Some("libraries.minecraft.net") => (
            ControlledProvider::MojangContent,
            format!("/{}", coordinate.path),
        ),
        Some(host) => {
            return Err(AppError::coded_with(
                "runtime_domain_not_allowed",
                [("host", host.to_string())],
            ));
        }
        None => return Err(AppError::coded("runtime_host_missing")),
    };
    let url = validate_url(provider, &download.url)?;
    if url.query().is_some() || url.path() != expected_path {
        return Err(AppError::coded("runtime_neoforge_library_url_mismatch"));
    }
    Ok(())
}

fn resolve_artifact_catalog<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    entries: &BTreeMap<String, ArchiveEntryMetadata>,
    specs: BTreeMap<String, LibrarySpec>,
) -> AppResult<ArtifactCatalog> {
    let mut path_to_coordinate = BTreeMap::new();
    for spec in specs.values() {
        if path_to_coordinate
            .insert(
                spec.coordinate.path.clone(),
                spec.coordinate.canonical.clone(),
            )
            .is_some()
        {
            return Err(AppError::coded("runtime_neoforge_library_target_collision"));
        }
    }
    for (entry_name, metadata) in entries {
        if metadata.is_directory || !entry_name.starts_with("maven/") {
            continue;
        }
        let path = entry_name
            .strip_prefix("maven/")
            .ok_or_else(|| AppError::coded("runtime_neoforge_embedded_path_invalid"))?;
        validate_canonical_relative(path)?;
        if !path_to_coordinate.contains_key(path) {
            return Err(AppError::coded(
                "runtime_neoforge_embedded_artifact_undeclared",
            ));
        }
    }

    let mut catalog = ArtifactCatalog::default();
    for spec in specs.into_values() {
        let installer_entry = format!("maven/{}", spec.coordinate.path);
        let target = format!("libraries/{}", spec.coordinate.path);
        validate_canonical_relative(&target)?;
        let (processor, external, embedded) =
            if let Some(entry_metadata) = entries.get(&installer_entry) {
                if entry_metadata.is_directory {
                    return Err(AppError::coded(
                        "runtime_neoforge_embedded_artifact_invalid",
                    ));
                }
                let digests = hash_archive_entry(archive, entry_metadata)?;
                if let Some(download) = &spec.download {
                    if digests.size_bytes != download.size || digests.sha1 != download.sha1 {
                        return Err(AppError::coded(
                            "runtime_neoforge_embedded_artifact_mismatch",
                        ));
                    }
                }
                let embedded = NeoForgeEmbeddedArtifact {
                    coordinate: spec.coordinate.canonical.clone(),
                    installer_entry,
                    target_relative_path: target.clone(),
                    size_bytes: digests.size_bytes,
                    sha256: digests.sha256.clone(),
                };
                (
                    NeoForgeProcessorArtifact {
                        coordinate: spec.coordinate.canonical.clone(),
                        target_relative_path: target,
                        availability: NeoForgeArtifactAvailability::Embedded,
                        size_bytes: digests.size_bytes,
                        sha1: Some(digests.sha1),
                        sha256: Some(digests.sha256),
                    },
                    None,
                    Some(embedded),
                )
            } else {
                let download = spec
                    .download
                    .ok_or_else(|| AppError::coded("runtime_neoforge_library_source_missing"))?;
                let provider = if download.url.starts_with("https://maven.neoforged.net/") {
                    "neoforge"
                } else {
                    "mojang"
                };
                let external = RuntimeArtifactSource {
                    logical_id: spec.coordinate.canonical.clone(),
                    provider: provider.into(),
                    url: download.url,
                    target_relative_path: target.clone(),
                    size_bytes: download.size,
                    sha1: Some(download.sha1.clone()),
                    sha256: None,
                    kind: RuntimeArtifactKind::LoaderLibrary,
                };
                (
                    NeoForgeProcessorArtifact {
                        coordinate: spec.coordinate.canonical.clone(),
                        target_relative_path: target,
                        availability: NeoForgeArtifactAvailability::External,
                        size_bytes: download.size,
                        sha1: Some(download.sha1),
                        sha256: None,
                    },
                    Some(external),
                    None,
                )
            };
        if let Some(source) = external.clone() {
            catalog.external.push(source);
        }
        if let Some(artifact) = embedded.clone() {
            catalog.embedded.push(artifact);
        }
        catalog
            .bindings
            .insert(spec.coordinate.canonical, ArtifactBinding { processor });
    }
    catalog
        .external
        .sort_by(|left, right| left.target_relative_path.cmp(&right.target_relative_path));
    catalog
        .embedded
        .sort_by(|left, right| left.target_relative_path.cmp(&right.target_relative_path));
    Ok(catalog)
}

fn resolve_data_map<R: Read + Seek>(
    source: &BTreeMap<String, SideDataValue>,
    catalog: &ArtifactCatalog,
    archive: &mut zip::ZipArchive<R>,
    entries: &BTreeMap<String, ArchiveEntryMetadata>,
    entry_digests: &mut BTreeMap<String, EntryDigests>,
) -> AppResult<BTreeMap<String, NeoForgeDataReference>> {
    let mut result = BTreeMap::new();
    for (key, value) in source {
        validate_data_key(key)?;
        let client = resolve_data_value(&value.client, catalog, archive, entries, entry_digests)?;
        if let Some(server) = value.server.as_deref() {
            let _ = resolve_data_value(server, catalog, archive, entries, entry_digests)?;
        }
        result.insert(key.clone(), client);
    }
    Ok(result)
}

fn resolve_data_value<R: Read + Seek>(
    raw: &str,
    catalog: &ArtifactCatalog,
    archive: &mut zip::ZipArchive<R>,
    entries: &BTreeMap<String, ArchiveEntryMetadata>,
    entry_digests: &mut BTreeMap<String, EntryDigests>,
) -> AppResult<NeoForgeDataReference> {
    if let Some(coordinate) = bracket_coordinate(raw)? {
        let coordinate = parse_maven_coordinate(coordinate)?;
        let (target, availability) = catalog
            .bindings
            .get(&coordinate.canonical)
            .map(|binding| {
                (
                    binding.processor.target_relative_path.clone(),
                    binding.processor.availability,
                )
            })
            .unwrap_or_else(|| {
                (
                    format!("libraries/{}", coordinate.path),
                    NeoForgeArtifactAvailability::Generated,
                )
            });
        validate_canonical_relative(&target)?;
        return Ok(NeoForgeDataReference::Maven {
            coordinate: coordinate.canonical,
            target_relative_path: target,
            availability,
        });
    }
    if let Some(entry_name) = raw.strip_prefix('/') {
        validate_canonical_relative(entry_name)?;
        let metadata = entries
            .get(entry_name)
            .filter(|metadata| !metadata.is_directory)
            .ok_or_else(|| AppError::coded("runtime_neoforge_installer_entry_missing"))?;
        let digests = if let Some(digests) = entry_digests.get(entry_name) {
            digests.clone()
        } else {
            let digests = hash_archive_entry(archive, metadata)?;
            entry_digests.insert(entry_name.to_string(), digests.clone());
            digests
        };
        return Ok(NeoForgeDataReference::InstallerEntry {
            installer_entry: entry_name.to_string(),
            materialized_relative_path: format!("installer-data/{}.bin", digests.sha256),
            size_bytes: digests.size_bytes,
            sha256: digests.sha256,
        });
    }
    if raw.starts_with('\'') && raw.ends_with('\'') && raw.len() >= 2 {
        let value = &raw[1..raw.len() - 1];
        validate_literal(value)?;
        return Ok(NeoForgeDataReference::Literal {
            value: value.to_string(),
        });
    }
    Err(AppError::coded("runtime_neoforge_data_value_unsupported"))
}

#[allow(clippy::too_many_arguments)]
fn resolve_processors<R: Read + Seek>(
    source: &[ProcessorDocument],
    data: &BTreeMap<String, NeoForgeDataReference>,
    catalog: &ArtifactCatalog,
    archive: &mut zip::ZipArchive<R>,
    entries: &BTreeMap<String, ArchiveEntryMetadata>,
    entry_digests: &mut BTreeMap<String, EntryDigests>,
    minecraft_version: &str,
) -> AppResult<Vec<NeoForgeProcessorPlan>> {
    if source.is_empty() || source.len() > MAX_PROCESSORS {
        return Err(AppError::coded("runtime_neoforge_processor_count_invalid"));
    }
    let mut plans = Vec::new();
    for (index, processor) in source.iter().enumerate() {
        let applies_to_client = validate_processor_sides(&processor.sides)?;
        let executable_jar = resolve_processor_artifact_binding(&processor.jar, catalog)?;
        if processor.classpath.is_empty()
            || processor.classpath.len() > MAX_PROCESSOR_CLASSPATH
            || processor.args.len() > MAX_PROCESSOR_ARGUMENTS
        {
            return Err(AppError::coded("runtime_neoforge_processor_shape_invalid"));
        }
        let classpath = processor
            .classpath
            .iter()
            .map(|coordinate| resolve_processor_artifact_binding(coordinate, catalog))
            .collect::<AppResult<Vec<_>>>()?;
        if !classpath
            .iter()
            .any(|artifact| artifact.coordinate == executable_jar.coordinate)
        {
            return Err(AppError::coded(
                "runtime_neoforge_processor_jar_not_in_classpath",
            ));
        }
        let arguments = processor
            .args
            .iter()
            .map(|argument| {
                resolve_processor_argument(
                    argument,
                    data,
                    catalog,
                    archive,
                    entries,
                    entry_digests,
                    minecraft_version,
                )
            })
            .collect::<AppResult<Vec<_>>>()?;
        let outputs = processor
            .outputs
            .iter()
            .map(|(target, hash)| {
                let target = resolve_processor_argument(
                    target,
                    data,
                    catalog,
                    archive,
                    entries,
                    entry_digests,
                    minecraft_version,
                )?;
                validate_processor_output_target(&target)?;
                let (algorithm, digest) = parse_output_digest(hash)?;
                Ok(NeoForgeProcessorOutput {
                    target,
                    algorithm,
                    digest,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        let plan_sha256 =
            hash_serializable(&(index, &executable_jar, &classpath, &arguments, &outputs))?;
        if applies_to_client {
            plans.push(NeoForgeProcessorPlan {
                index,
                executable_jar,
                classpath,
                arguments,
                outputs,
                plan_sha256,
            });
        }
    }
    if plans.is_empty() {
        return Err(AppError::coded(
            "runtime_neoforge_client_processors_missing",
        ));
    }
    Ok(plans)
}

fn validate_processor_sides(sides: &[String]) -> AppResult<bool> {
    let mut seen = BTreeSet::new();
    for side in sides {
        if !matches!(side.as_str(), "client" | "server") || !seen.insert(side) {
            return Err(AppError::coded("runtime_neoforge_processor_side_invalid"));
        }
    }
    Ok(sides.is_empty() || sides.iter().any(|side| side == "client"))
}

fn resolve_processor_artifact_binding(
    raw: &str,
    catalog: &ArtifactCatalog,
) -> AppResult<NeoForgeProcessorArtifact> {
    let coordinate = parse_maven_coordinate(raw)?;
    catalog
        .bindings
        .get(&coordinate.canonical)
        .map(|binding| binding.processor.clone())
        .ok_or_else(|| AppError::coded("runtime_neoforge_processor_dependency_missing"))
}

#[allow(clippy::too_many_arguments)]
fn resolve_processor_argument<R: Read + Seek>(
    raw: &str,
    data: &BTreeMap<String, NeoForgeDataReference>,
    catalog: &ArtifactCatalog,
    archive: &mut zip::ZipArchive<R>,
    entries: &BTreeMap<String, ArchiveEntryMetadata>,
    entry_digests: &mut BTreeMap<String, EntryDigests>,
    minecraft_version: &str,
) -> AppResult<NeoForgeProcessorValue> {
    if raw.is_empty() || raw.len() > MAX_ARGUMENT_BYTES || contains_control(raw) {
        return Err(AppError::coded(
            "runtime_neoforge_processor_argument_invalid",
        ));
    }
    if raw.contains("://") || raw.contains('\\') {
        return Err(AppError::coded(
            "runtime_neoforge_processor_argument_path_forbidden",
        ));
    }
    if let Some(suffix) = raw.strip_prefix("{ROOT}") {
        if suffix.is_empty() {
            return Ok(NeoForgeProcessorValue::ProfileRoot);
        }
        let suffix = suffix
            .strip_prefix('/')
            .ok_or_else(|| AppError::coded("runtime_neoforge_processor_argument_path_forbidden"))?;
        let directory = suffix.ends_with('/');
        let relative = suffix.trim_end_matches('/');
        validate_canonical_relative(relative)?;
        return Ok(NeoForgeProcessorValue::ProfileRelative {
            relative_path: relative.to_string(),
            directory,
        });
    }
    if raw.starts_with('{') || raw.ends_with('}') {
        if !(raw.starts_with('{') && raw.ends_with('}') && raw.len() >= 3) {
            return Err(AppError::coded("runtime_neoforge_placeholder_invalid"));
        }
        let key = &raw[1..raw.len() - 1];
        return match key {
            "INSTALLER" => Ok(NeoForgeProcessorValue::Installer),
            "MINECRAFT_JAR" => Ok(NeoForgeProcessorValue::MinecraftClient {
                target_relative_path: format!(
                    "versions/{minecraft_version}/{minecraft_version}.jar"
                ),
            }),
            "SIDE" => Ok(NeoForgeProcessorValue::Side {
                value: "client".into(),
            }),
            "LIBRARY_DIR" => Ok(NeoForgeProcessorValue::ProfileRelative {
                relative_path: "libraries".into(),
                directory: true,
            }),
            _ => data
                .get(key)
                .cloned()
                .map(|reference| NeoForgeProcessorValue::Data { reference })
                .ok_or_else(|| AppError::coded("runtime_neoforge_placeholder_unknown")),
        };
    }
    if raw.contains(['{', '}']) {
        return Err(AppError::coded("runtime_neoforge_placeholder_invalid"));
    }
    if bracket_coordinate(raw)?.is_some() {
        return Ok(NeoForgeProcessorValue::Data {
            reference: resolve_data_value(raw, catalog, archive, entries, entry_digests)?,
        });
    }
    if raw.contains('/') {
        validate_canonical_relative(raw)?;
        let metadata = entries
            .get(raw)
            .filter(|metadata| !metadata.is_directory)
            .ok_or_else(|| AppError::coded("runtime_neoforge_processor_installer_entry_missing"))?;
        let digests = if let Some(digests) = entry_digests.get(raw) {
            digests.clone()
        } else {
            let digests = hash_archive_entry(archive, metadata)?;
            entry_digests.insert(raw.to_string(), digests.clone());
            digests
        };
        return Ok(NeoForgeProcessorValue::Data {
            reference: NeoForgeDataReference::InstallerEntry {
                installer_entry: raw.to_string(),
                materialized_relative_path: format!("installer-data/{}.bin", digests.sha256),
                size_bytes: digests.size_bytes,
                sha256: digests.sha256,
            },
        });
    }
    validate_literal(raw)?;
    Ok(NeoForgeProcessorValue::Literal {
        value: raw.to_string(),
    })
}

fn validate_processor_output_target(value: &NeoForgeProcessorValue) -> AppResult<()> {
    match value {
        NeoForgeProcessorValue::Data {
            reference:
                NeoForgeDataReference::Maven {
                    availability: NeoForgeArtifactAvailability::Generated,
                    ..
                },
        }
        | NeoForgeProcessorValue::ProfileRelative { .. } => Ok(()),
        _ => Err(AppError::coded(
            "runtime_neoforge_processor_output_target_invalid",
        )),
    }
}

fn parse_output_digest(value: &str) -> AppResult<(NeoForgeDigestAlgorithm, String)> {
    match value.len() {
        40 => {
            validate_sha1(value)?;
            Ok((NeoForgeDigestAlgorithm::Sha1, value.to_string()))
        }
        64 => {
            validate_sha256(value)?;
            Ok((NeoForgeDigestAlgorithm::Sha256, value.to_string()))
        }
        _ => Err(AppError::coded(
            "runtime_neoforge_processor_output_digest_invalid",
        )),
    }
}

fn validate_execution_readiness(plan: &NeoForgeInstallPlan) -> AppResult<()> {
    if plan.plan_sha256 != compute_plan_hash(plan)? {
        return Err(AppError::coded(
            "runtime_neoforge_install_plan_hash_mismatch",
        ));
    }
    if plan.processors.is_empty() {
        return Err(AppError::coded(
            "runtime_neoforge_client_processors_missing",
        ));
    }
    let mut produced = BTreeSet::new();
    for processor in &plan.processors {
        let expected_hash = hash_serializable(&(
            processor.index,
            &processor.executable_jar,
            &processor.classpath,
            &processor.arguments,
            &processor.outputs,
        ))?;
        if processor.plan_sha256 != expected_hash {
            return Err(AppError::coded(
                "runtime_neoforge_processor_plan_hash_mismatch",
            ));
        }
        if processor.outputs.is_empty() {
            return Err(AppError::coded(
                "runtime_neoforge_processor_output_hashes_required",
            ));
        }
        if processor_requests_network(processor) {
            return Err(AppError::coded(
                "runtime_neoforge_processor_network_required",
            ));
        }
        let output_targets = processor
            .outputs
            .iter()
            .map(|output| processor_value_target(&output.target))
            .collect::<AppResult<BTreeSet<_>>>()?;
        if output_targets.len() != processor.outputs.len() {
            return Err(AppError::coded(
                "runtime_neoforge_processor_output_duplicate",
            ));
        }
        for argument in &processor.arguments {
            if let Some(target) = generated_target(argument) {
                if !output_targets.contains(target) && !produced.contains(target) {
                    return Err(AppError::coded(
                        "runtime_neoforge_generated_input_unavailable",
                    ));
                }
            }
        }
        for target in output_targets {
            if !produced.insert(target) {
                return Err(AppError::coded(
                    "runtime_neoforge_processor_output_overwrite",
                ));
            }
        }
    }
    Ok(())
}

fn processor_requests_network(processor: &NeoForgeProcessorPlan) -> bool {
    processor.arguments.windows(2).any(|pair| {
        matches!(
            (&pair[0], &pair[1]),
            (
                NeoForgeProcessorValue::Literal { value: flag },
                NeoForgeProcessorValue::Literal { value: task }
            ) if flag == "--task" && task == "DOWNLOAD_MOJMAPS"
        )
    })
}

fn generated_target(value: &NeoForgeProcessorValue) -> Option<&String> {
    match value {
        NeoForgeProcessorValue::Data {
            reference:
                NeoForgeDataReference::Maven {
                    target_relative_path,
                    availability: NeoForgeArtifactAvailability::Generated,
                    ..
                },
        } => Some(target_relative_path),
        _ => None,
    }
}

fn processor_value_target(value: &NeoForgeProcessorValue) -> AppResult<String> {
    match value {
        NeoForgeProcessorValue::Data {
            reference:
                NeoForgeDataReference::Maven {
                    target_relative_path,
                    availability: NeoForgeArtifactAvailability::Generated,
                    ..
                },
        } => Ok(target_relative_path.clone()),
        NeoForgeProcessorValue::ProfileRelative { relative_path, .. } => Ok(relative_path.clone()),
        _ => Err(AppError::coded(
            "runtime_neoforge_processor_output_target_invalid",
        )),
    }
}

#[cfg(test)]
fn validate_execution_context(
    plan: &NeoForgeInstallPlan,
    context: &NeoForgeExecutionContext,
) -> AppResult<()> {
    if context.java.root_id() != "runtimes"
        || context
            .java
            .absolute()
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| !matches!(name, "java" | "java.exe"))
    {
        return Err(AppError::coded("runtime_java_path_uncontrolled"));
    }
    validate_existing_regular_file(&context.java)?;
    validate_staging_directory(&context.staging)?;
    validate_verified_file(&context.installer)?;
    validate_verified_file(&context.minecraft_client)?;
    if context.installer.size_bytes != plan.installer_size_bytes
        || context.installer.sha256 != plan.installer_sha256
    {
        return Err(AppError::coded(
            "runtime_neoforge_installer_context_mismatch",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn validate_staging_directory(staging: &SecurePath) -> AppResult<()> {
    if staging.root_id() != "staging-operations" {
        return Err(AppError::coded("runtime_neoforge_staging_root_invalid"));
    }
    validate_existing_chain(staging.anchor(), staging.absolute())?;
    let metadata = fs::symlink_metadata(staging.absolute())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::coded(
            "runtime_neoforge_staging_directory_invalid",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn validate_existing_regular_file(path: &SecurePath) -> AppResult<()> {
    validate_existing_chain(path.anchor(), path.absolute())?;
    let metadata = fs::symlink_metadata(path.absolute())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() == 0 {
        return Err(AppError::coded("runtime_neoforge_execution_input_invalid"));
    }
    Ok(())
}

#[cfg(test)]
fn validate_verified_file(file: &NeoForgeVerifiedFile) -> AppResult<()> {
    validate_sha256(&file.sha256)?;
    validate_existing_regular_file(&file.path)?;
    let mut input = File::open(file.path.absolute())?;
    let actual = hash_reader(&mut input)?;
    if actual.size_bytes != file.size_bytes || actual.sha256 != file.sha256 {
        return Err(AppError::coded("runtime_neoforge_execution_input_mismatch"));
    }
    Ok(())
}

#[cfg(test)]
fn resolve_processor_artifact<'a>(
    context: &'a NeoForgeExecutionContext,
    artifact: &NeoForgeProcessorArtifact,
) -> AppResult<&'a NeoForgeVerifiedFile> {
    let file = context
        .artifacts
        .get(&artifact.target_relative_path)
        .ok_or_else(|| AppError::coded("runtime_neoforge_processor_artifact_missing"))?;
    validate_verified_file(file)?;
    if file.size_bytes != artifact.size_bytes {
        return Err(AppError::coded(
            "runtime_neoforge_processor_artifact_size_mismatch",
        ));
    }
    let mut input = File::open(file.path.absolute())?;
    let digests = hash_reader(&mut input)?;
    if artifact
        .sha1
        .as_deref()
        .is_some_and(|expected| expected != digests.sha1)
        || artifact
            .sha256
            .as_deref()
            .is_some_and(|expected| expected != digests.sha256)
    {
        return Err(AppError::coded(
            "runtime_neoforge_processor_artifact_hash_mismatch",
        ));
    }
    Ok(file)
}

#[cfg(test)]
type ResolvedOutput = (String, SecurePath, NeoForgeDigestAlgorithm, String);

#[cfg(test)]
fn resolve_output_paths(
    registry: &PathRegistry,
    context: &NeoForgeExecutionContext,
    processor: &NeoForgeProcessorPlan,
) -> AppResult<Vec<ResolvedOutput>> {
    processor
        .outputs
        .iter()
        .map(|output| {
            let target = processor_value_target(&output.target)?;
            let path = registry.resolve(
                context.staging.root_id(),
                context.staging.relative().join(&target),
            )?;
            Ok((target, path, output.algorithm, output.digest.clone()))
        })
        .collect()
}

#[cfg(test)]
fn resolve_processor_value(
    registry: &PathRegistry,
    context: &NeoForgeExecutionContext,
    value: &NeoForgeProcessorValue,
    current_outputs: &BTreeMap<String, PathBuf>,
) -> AppResult<OsString> {
    match value {
        NeoForgeProcessorValue::Literal { value } => Ok(OsString::from(value)),
        NeoForgeProcessorValue::Installer => {
            Ok(context.installer.path.absolute().as_os_str().to_owned())
        }
        NeoForgeProcessorValue::MinecraftClient { .. } => Ok(context
            .minecraft_client
            .path
            .absolute()
            .as_os_str()
            .to_owned()),
        NeoForgeProcessorValue::ProfileRoot => {
            Ok(context.staging.absolute().as_os_str().to_owned())
        }
        NeoForgeProcessorValue::ProfileRelative { relative_path, .. } => {
            if let Some(path) = current_outputs.get(relative_path) {
                return Ok(path.as_os_str().to_owned());
            }
            let path = registry.resolve(
                context.staging.root_id(),
                context.staging.relative().join(relative_path),
            )?;
            validate_existing_chain(path.anchor(), path.absolute())?;
            if !path.absolute().exists() {
                return Err(AppError::coded("runtime_neoforge_profile_input_missing"));
            }
            Ok(path.absolute().as_os_str().to_owned())
        }
        NeoForgeProcessorValue::Side { value } => Ok(OsString::from(value)),
        NeoForgeProcessorValue::Data { reference } => match reference {
            NeoForgeDataReference::Literal { value } => Ok(OsString::from(value)),
            NeoForgeDataReference::InstallerEntry {
                materialized_relative_path,
                ..
            } => {
                let file = context
                    .artifacts
                    .get(materialized_relative_path)
                    .ok_or_else(|| {
                        AppError::coded("runtime_neoforge_installer_entry_not_materialized")
                    })?;
                validate_verified_file(file)?;
                Ok(file.path.absolute().as_os_str().to_owned())
            }
            NeoForgeDataReference::Maven {
                target_relative_path,
                availability: NeoForgeArtifactAvailability::Generated,
                ..
            } => {
                if let Some(path) = current_outputs.get(target_relative_path) {
                    return Ok(path.as_os_str().to_owned());
                }
                let path = registry.resolve(
                    context.staging.root_id(),
                    context.staging.relative().join(target_relative_path),
                )?;
                validate_existing_regular_file(&path)?;
                Ok(path.absolute().as_os_str().to_owned())
            }
            NeoForgeDataReference::Maven {
                target_relative_path,
                ..
            } => {
                let file = context.artifacts.get(target_relative_path).ok_or_else(|| {
                    AppError::coded("runtime_neoforge_processor_artifact_missing")
                })?;
                validate_verified_file(file)?;
                Ok(file.path.absolute().as_os_str().to_owned())
            }
        },
    }
}

#[cfg(test)]
fn processor_main_class(path: &Path) -> AppResult<String> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| AppError::coded("runtime_neoforge_processor_jar_invalid"))?;
    let entries = validate_installer_archive(&mut archive)?;
    let metadata = entries
        .get("META-INF/MANIFEST.MF")
        .filter(|metadata| !metadata.is_directory)
        .ok_or_else(|| AppError::coded("runtime_neoforge_processor_main_class_missing"))?;
    if metadata.size_bytes == 0 || metadata.size_bytes > 64 * 1024 {
        return Err(AppError::coded(
            "runtime_neoforge_processor_manifest_invalid",
        ));
    }
    let mut entry = archive
        .by_index(metadata.index)
        .map_err(|_| AppError::coded("runtime_neoforge_processor_jar_invalid"))?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|_| AppError::coded("runtime_neoforge_processor_manifest_invalid"))?;
    let mut main_class = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Main-Class: ") {
            if main_class.replace(value.trim().to_string()).is_some() {
                return Err(AppError::coded(
                    "runtime_neoforge_processor_main_class_duplicate",
                ));
            }
        }
    }
    let main_class = main_class
        .ok_or_else(|| AppError::coded("runtime_neoforge_processor_main_class_missing"))?;
    validate_java_class(&main_class)?;
    Ok(main_class)
}

#[cfg(test)]
fn insert_materialization_request(
    requests: &mut BTreeMap<String, (String, u64, String)>,
    target: &str,
    entry: &str,
    size: u64,
    sha256: &str,
) -> AppResult<()> {
    validate_canonical_relative(target)?;
    validate_canonical_relative(entry)?;
    validate_sha256(sha256)?;
    let candidate = (entry.to_string(), size, sha256.to_string());
    if requests
        .insert(target.to_string(), candidate.clone())
        .is_some_and(|previous| previous != candidate)
    {
        return Err(AppError::coded("runtime_neoforge_materialization_conflict"));
    }
    Ok(())
}

#[cfg(test)]
fn insert_data_materialization_request(
    requests: &mut BTreeMap<String, (String, u64, String)>,
    reference: &NeoForgeDataReference,
) -> AppResult<()> {
    if let NeoForgeDataReference::InstallerEntry {
        installer_entry,
        materialized_relative_path,
        size_bytes,
        sha256,
    } = reference
    {
        insert_materialization_request(
            requests,
            materialized_relative_path,
            installer_entry,
            *size_bytes,
            sha256,
        )?;
    }
    Ok(())
}

fn parse_maven_coordinate(value: &str) -> AppResult<MavenCoordinate> {
    if value.is_empty() || value.len() > 512 || !value.is_ascii() {
        return Err(AppError::coded("runtime_maven_coordinate_invalid"));
    }
    let mut at = value.split('@');
    let base = at.next().unwrap_or_default();
    let extension = at.next().unwrap_or("jar");
    if at.next().is_some() {
        return Err(AppError::coded("runtime_maven_coordinate_invalid"));
    }
    validate_maven_segment(extension)?;
    let parts = base.split(':').collect::<Vec<_>>();
    if !(3..=4).contains(&parts.len()) {
        return Err(AppError::coded("runtime_maven_coordinate_invalid"));
    }
    for part in &parts {
        validate_maven_segment(part)?;
    }
    let group = parts[0]
        .split('.')
        .map(|part| {
            validate_maven_segment(part)?;
            Ok(part)
        })
        .collect::<AppResult<Vec<_>>>()?
        .join("/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map(|classifier| format!("-{classifier}"))
        .unwrap_or_default();
    let canonical = format!(
        "{}{}",
        parts.join(":"),
        if extension == "jar" {
            String::new()
        } else {
            format!("@{extension}")
        }
    );
    Ok(MavenCoordinate {
        canonical,
        path: format!("{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"),
    })
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

fn bracket_coordinate(value: &str) -> AppResult<Option<&str>> {
    if value.starts_with('[') || value.ends_with(']') {
        if value.starts_with('[')
            && value.ends_with(']')
            && value.len() >= 3
            && !value[1..value.len() - 1].contains(['[', ']'])
        {
            Ok(Some(&value[1..value.len() - 1]))
        } else {
            Err(AppError::coded("runtime_neoforge_maven_reference_invalid"))
        }
    } else {
        Ok(None)
    }
}

fn validate_data_key(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(AppError::coded("runtime_neoforge_data_key_invalid"));
    }
    Ok(())
}

fn validate_literal(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_ARGUMENT_BYTES
        || !value.is_ascii()
        || contains_control(value)
        || value.contains("://")
        || value.contains(['\\', '{', '}', '[', ']', ':'])
        || value == "."
        || value == ".."
    {
        return Err(AppError::coded(
            "runtime_neoforge_processor_literal_invalid",
        ));
    }
    Ok(())
}

fn validate_launch_arguments(arguments: &[LaunchArgument]) -> AppResult<()> {
    if arguments.len() > MAX_PROCESSOR_ARGUMENTS {
        return Err(AppError::coded("runtime_neoforge_launch_arguments_invalid"));
    }
    for argument in arguments {
        match argument {
            LaunchArgument::Plain(value) => validate_launch_argument_value(value)?,
            LaunchArgument::Conditional { rules, value } => {
                if rules.is_empty() {
                    return Err(AppError::coded(
                        "runtime_neoforge_launch_argument_rule_invalid",
                    ));
                }
                match value {
                    LaunchArgumentValue::One(value) => validate_launch_argument_value(value)?,
                    LaunchArgumentValue::Many(values) => {
                        for value in values {
                            validate_launch_argument_value(value)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_launch_argument_value(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_ARGUMENT_BYTES
        || contains_control(value)
        || value.contains("://")
        || value.contains('\\')
        || value.split('/').any(|component| component == "..")
    {
        return Err(AppError::coded("runtime_neoforge_launch_argument_invalid"));
    }
    let mut remainder = value;
    let mut scrubbed = String::with_capacity(value.len());
    while let Some(start) = remainder.find("${") {
        scrubbed.push_str(&remainder[..start]);
        let after = &remainder[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| AppError::coded("runtime_neoforge_launch_placeholder_invalid"))?;
        let placeholder = &after[..end];
        if !matches!(
            placeholder,
            "version_name" | "library_directory" | "classpath_separator"
        ) {
            return Err(AppError::coded(
                "runtime_neoforge_launch_placeholder_unknown",
            ));
        }
        remainder = &after[end + 1..];
    }
    scrubbed.push_str(remainder);
    if scrubbed.contains(['{', '}']) {
        return Err(AppError::coded(
            "runtime_neoforge_launch_placeholder_invalid",
        ));
    }
    Ok(())
}

fn validate_java_class(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 256
        || !value.is_ascii()
        || value.split('.').any(|part| {
            part.is_empty()
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
                || !part
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
        })
    {
        return Err(AppError::coded("runtime_main_class_invalid"));
    }
    Ok(())
}

fn validate_canonical_relative(value: &str) -> AppResult<PathBuf> {
    if value.contains('\\') {
        return Err(AppError::coded(
            "runtime_neoforge_path_separator_noncanonical",
        ));
    }
    let normalized = normalize_relative_path(Path::new(value))?;
    let canonical = normalized
        .iter()
        .map(|component| component.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if canonical != value {
        return Err(AppError::coded("runtime_neoforge_path_noncanonical"));
    }
    Ok(normalized)
}

fn loader_matches_minecraft(minecraft: &str, loader: &str) -> bool {
    let mut minecraft_parts = minecraft.split('.');
    let major = minecraft_parts.next();
    let minor = minecraft_parts.next();
    let mut loader_parts = loader.split('.');
    matches!(
        (major, minor, loader_parts.next(), loader_parts.next()),
        (Some("1"), Some(mc_minor), Some(loader_minor), Some(_)) if mc_minor == loader_minor
    )
}

fn validate_sha1(value: &str) -> AppResult<()> {
    validate_digest(value, 40, "runtime_sha1_invalid")
}

fn validate_sha256(value: &str) -> AppResult<()> {
    validate_digest(value, 64, "runtime_sha256_invalid")
}

fn validate_digest(value: &str, length: usize, code: &str) -> AppResult<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded(code));
    }
    Ok(())
}

fn hash_archive_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    metadata: &ArchiveEntryMetadata,
) -> AppResult<EntryDigests> {
    let entry = archive
        .by_index(metadata.index)
        .map_err(|_| AppError::coded("runtime_neoforge_installer_invalid"))?;
    hash_bounded_reader(entry, metadata.size_bytes)
}

fn hash_bounded_reader(mut reader: impl Read, expected_size: u64) -> AppResult<EntryDigests> {
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| AppError::coded("runtime_neoforge_installer_entry_read_failed"))?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| AppError::coded("runtime_neoforge_entry_size_overflow"))?;
        if size > expected_size {
            return Err(AppError::coded(
                "runtime_neoforge_installer_entry_size_mismatch",
            ));
        }
        sha1.update(&buffer[..read]);
        sha256.update(&buffer[..read]);
    }
    if size != expected_size {
        return Err(AppError::coded(
            "runtime_neoforge_installer_entry_size_mismatch",
        ));
    }
    Ok(EntryDigests {
        size_bytes: size,
        sha1: hex::encode(sha1.finalize()),
        sha256: hex::encode(sha256.finalize()),
    })
}

fn hash_reader(reader: &mut impl Read) -> AppResult<EntryDigests> {
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut size = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| AppError::coded("runtime_neoforge_file_size_overflow"))?;
        sha1.update(&buffer[..read]);
        sha256.update(&buffer[..read]);
    }
    Ok(EntryDigests {
        size_bytes: size,
        sha1: hex::encode(sha1.finalize()),
        sha256: hex::encode(sha256.finalize()),
    })
}

fn hash_serializable(value: &impl Serialize) -> AppResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn compute_plan_hash(plan: &NeoForgeInstallPlan) -> AppResult<String> {
    hash_serializable(&(
        &plan.minecraft_version,
        &plan.loader_version,
        &plan.profile_id,
        &plan.main_class,
        &plan.game_arguments,
        &plan.jvm_arguments,
        &plan.installer_source,
        &plan.external_artifacts,
        &plan.embedded_artifacts,
        &plan.runtime_library_targets,
        &plan.data,
        &plan.processors,
        &plan.installer_sha256,
        plan.installer_size_bytes,
    ))
}

fn contains_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        operations::model::{new_identifier, sha256_hex},
        security::RegisteredRoot,
    };
    use std::{
        io::{Cursor, Write},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    use zip::write::SimpleFileOptions;

    const PROCESSOR_COORDINATE: &str = "net.neoforged.tools:processor:1.0";
    const PROCESSOR_PATH: &str = "net/neoforged/tools/processor/1.0/processor-1.0.jar";
    const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";
    const OUTPUT_SHA256: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    struct Fixture {
        root: PathBuf,
        installer: SecurePath,
        source: RuntimeArtifactSource,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn base_documents() -> (serde_json::Value, serde_json::Value) {
        let library = serde_json::json!({
            "name": PROCESSOR_COORDINATE,
            "downloads": {
                "artifact": {
                    "path": PROCESSOR_PATH,
                    "sha1": SHA1,
                    "size": 1234,
                    "url": format!("https://maven.neoforged.net/releases/{PROCESSOR_PATH}")
                }
            }
        });
        let install = serde_json::json!({
            "spec": 1,
            "profile": "NeoForge",
            "version": "neoforge-21.1.200",
            "minecraft": "1.21.1",
            "json": "/version.json",
            "mirrorList": "",
            "data": {
                "BINPATCH": {
                    "client": "/data/client.lzma",
                    "server": "/data/server.lzma"
                },
                "PATCHED": {
                    "client": "[net.neoforged:neoforge:21.1.200:client]",
                    "server": "[net.neoforged:neoforge:21.1.200:server]"
                }
            },
            "processors": [{
                "sides": ["client"],
                "jar": PROCESSOR_COORDINATE,
                "classpath": [PROCESSOR_COORDINATE],
                "args": [
                    "--clean",
                    "{MINECRAFT_JAR}",
                    "--output",
                    "{PATCHED}",
                    "--apply",
                    "{BINPATCH}"
                ],
                "outputs": {
                    "{PATCHED}": OUTPUT_SHA256
                }
            }],
            "libraries": [library.clone()]
        });
        let version = serde_json::json!({
            "id": "neoforge-21.1.200",
            "inheritsFrom": "1.21.1",
            "type": "release",
            "mainClass": "cpw.mods.bootstraplauncher.BootstrapLauncher",
            "arguments": {
                "game": ["--fml.neoForgeVersion", "21.1.200"],
                "jvm": ["-DlibraryDirectory=${library_directory}"]
            },
            "libraries": [library]
        });
        (install, version)
    }

    fn build_installer(
        install: &serde_json::Value,
        version: &serde_json::Value,
        extra_entries: &[(&str, Vec<u8>)],
    ) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer
            .start_file(INSTALL_PROFILE_ENTRY, options)
            .expect("install profile entry");
        writer
            .write_all(&serde_json::to_vec(install).expect("install JSON"))
            .expect("install profile");
        writer
            .start_file(VERSION_PROFILE_ENTRY, options)
            .expect("version profile entry");
        writer
            .write_all(&serde_json::to_vec(version).expect("version JSON"))
            .expect("version profile");
        writer
            .start_file("data/client.lzma", options)
            .expect("client patch entry");
        writer.write_all(b"client-patch").expect("client patch");
        writer
            .start_file("data/server.lzma", options)
            .expect("server patch entry");
        writer.write_all(b"server-patch").expect("server patch");
        for (name, bytes) in extra_entries {
            writer.start_file(*name, options).expect("extra entry");
            writer.write_all(bytes).expect("extra bytes");
        }
        writer.finish().expect("finish installer").into_inner()
    }

    fn make_fixture(bytes: Vec<u8>) -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "s9lab-neoforge-test-{}-{}",
            std::process::id(),
            new_identifier("installer")
        ));
        fs::create_dir_all(&root).expect("test root");
        let registry = PathRegistry::new(
            &root,
            [RegisteredRoot {
                id: "test".into(),
                path: root.clone(),
            }],
        )
        .expect("registry");
        let installer = registry
            .resolve("test", "installer.jar")
            .expect("installer path");
        fs::write(installer.absolute(), &bytes).expect("installer fixture");
        let loader = "21.1.200";
        Fixture {
            root,
            installer,
            source: RuntimeArtifactSource {
                logical_id: format!("net.neoforged:neoforge:{loader}:installer"),
                provider: "neoforge".into(),
                url: format!(
                    "https://maven.neoforged.net/releases/net/neoforged/neoforge/{loader}/neoforge-{loader}-installer.jar"
                ),
                target_relative_path: format!("installers/neoforge/{loader}.jar"),
                size_bytes: bytes.len() as u64,
                sha1: None,
                sha256: Some(sha256_hex(&bytes)),
                kind: RuntimeArtifactKind::Installer,
            },
        }
    }

    fn inspect(fixture: &Fixture) -> AppResult<NeoForgeInstallPlan> {
        inspect_verified_installer(&fixture.installer, &fixture.source, "1.21.1", "21.1.200")
    }

    fn verified_installer(fixture: &Fixture) -> NeoForgeVerifiedFile {
        NeoForgeVerifiedFile {
            path: fixture.installer.clone(),
            size_bytes: fixture.source.size_bytes,
            sha256: fixture.source.sha256.clone().expect("fixture sha256"),
        }
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected NeoForge validation failure")
            .descriptor()
            .code
    }

    #[test]
    fn valid_installer_produces_a_typed_offline_plan() {
        let (install, version) = base_documents();
        let fixture = make_fixture(build_installer(&install, &version, &[]));
        let plan = inspect(&fixture).expect("valid plan");
        assert_eq!(plan.minecraft_version, "1.21.1");
        assert_eq!(plan.loader_version, "21.1.200");
        assert_eq!(plan.external_artifacts.len(), 1);
        assert_eq!(plan.processors.len(), 1);
        assert!(plan.execution_readiness().ready);
        assert_eq!(
            plan.resolved_loader().main_class,
            "cpw.mods.bootstraplauncher.BootstrapLauncher"
        );
    }

    #[test]
    fn traversal_ads_case_collision_and_zip_bomb_are_rejected() {
        let (install, version) = base_documents();
        for (entries, expected) in [
            (vec![("../escape.bin", b"x".to_vec())], "path_traversal"),
            (
                vec![("data//ambiguous.bin", b"x".to_vec())],
                "path_ambiguous_separator",
            ),
            (
                vec![("data/file.bin:stream", b"x".to_vec())],
                "path_alternate_data_stream",
            ),
            (
                vec![
                    ("Case/Entry.bin", b"x".to_vec()),
                    ("case/entry.BIN", b"y".to_vec()),
                ],
                "component_jar_entry_collision",
            ),
            (
                vec![("data/bomb.bin", vec![0; 2 * 1024 * 1024])],
                "component_jar_compression_ratio_exceeded",
            ),
        ] {
            let fixture = make_fixture(build_installer(&install, &version, &entries));
            assert_eq!(error_code(inspect(&fixture)), expected);
        }
    }

    #[test]
    fn declared_embedded_maven_artifact_is_verified_and_not_downloaded() {
        let (mut install, mut version) = base_documents();
        let processor_bytes = b"embedded-processor-jar".to_vec();
        let processor_sha1 = hex::encode(Sha1::digest(&processor_bytes));
        let processor_size = processor_bytes.len() as u64;
        for document in [&mut install, &mut version] {
            document["libraries"][0]["downloads"]["artifact"]["sha1"] =
                serde_json::json!(processor_sha1.clone());
            document["libraries"][0]["downloads"]["artifact"]["size"] =
                serde_json::json!(processor_size);
        }
        let entry_name = format!("maven/{PROCESSOR_PATH}");
        let fixture = make_fixture(build_installer(
            &install,
            &version,
            &[(entry_name.as_str(), processor_bytes.clone())],
        ));

        let plan = inspect(&fixture).expect("embedded Maven artifact");
        assert!(plan.external_artifacts.is_empty());
        assert_eq!(plan.embedded_artifacts.len(), 1);
        assert_eq!(
            plan.embedded_artifacts[0].sha256,
            hex::encode(Sha256::digest(&processor_bytes))
        );
        assert_eq!(
            plan.processors[0].executable_jar.availability,
            NeoForgeArtifactAvailability::Embedded
        );
    }

    #[test]
    fn installer_entries_materialize_create_new_into_registered_staging() {
        let (install, version) = base_documents();
        let fixture = make_fixture(build_installer(&install, &version, &[]));
        let plan = inspect(&fixture).expect("plan");
        let staging_root = fixture.root.join("staging");
        let operation_directory = staging_root.join("operation");
        fs::create_dir_all(&operation_directory).expect("operation staging");
        let registry = PathRegistry::new(
            &fixture.root,
            [RegisteredRoot {
                id: "staging-operations".into(),
                path: staging_root,
            }],
        )
        .expect("staging registry");
        let staging = registry
            .resolve("staging-operations", "operation")
            .expect("staging path");

        let files = materialize_installer_entries(
            &registry,
            &plan,
            &verified_installer(&fixture),
            &staging,
        )
        .expect("materialize installer entry");
        let target = match plan.data.get("BINPATCH").expect("BINPATCH") {
            NeoForgeDataReference::InstallerEntry {
                materialized_relative_path,
                ..
            } => materialized_relative_path,
            _ => panic!("BINPATCH must be an installer entry"),
        };
        let materialized = files.get(target).expect("materialized BINPATCH");
        assert_eq!(
            fs::read(materialized.path.absolute()).expect("materialized bytes"),
            b"client-patch"
        );
        assert_eq!(
            error_code(materialize_installer_entries(
                &registry,
                &plan,
                &verified_installer(&fixture),
                &staging,
            )),
            "runtime_neoforge_materialization_target_exists"
        );
    }

    #[test]
    fn symlink_entry_metadata_is_rejected() {
        let descriptor = JarEntryDescriptor {
            relative_path: "data/link".into(),
            is_directory: false,
            compressed_size_bytes: 4,
            uncompressed_size_bytes: 4,
            encrypted: false,
            unix_mode: Some(0o120777),
        };
        assert_eq!(
            error_code(validate_jar_entries(
                &[descriptor],
                JarValidationLimits {
                    max_entries: MAX_INSTALLER_ENTRIES,
                    max_total_compressed_bytes: MAX_INSTALLER_BYTES,
                    max_entry_uncompressed_bytes: MAX_INSTALLER_ENTRY_BYTES,
                    max_total_uncompressed_bytes: MAX_INSTALLER_EXPANDED_BYTES,
                    max_compression_ratio: 200,
                }
            )),
            "component_jar_symlink_forbidden"
        );
    }

    #[test]
    fn unknown_placeholder_uncontrolled_host_and_version_mismatch_fail_closed() {
        let (mut unknown, version) = base_documents();
        unknown["processors"][0]["args"][1] = serde_json::json!("{UNKNOWN}");
        let fixture = make_fixture(build_installer(&unknown, &version, &[]));
        assert_eq!(
            error_code(inspect(&fixture)),
            "runtime_neoforge_placeholder_unknown"
        );

        let (mut host, version) = base_documents();
        host["libraries"][0]["downloads"]["artifact"]["url"] =
            serde_json::json!(["https", "://evil.invalid/processor.jar"].concat());
        let fixture = make_fixture(build_installer(&host, &version, &[]));
        assert_eq!(error_code(inspect(&fixture)), "runtime_domain_not_allowed");

        let (mut mismatch, version) = base_documents();
        mismatch["minecraft"] = serde_json::json!("1.21.4");
        let fixture = make_fixture(build_installer(&mismatch, &version, &[]));
        assert_eq!(
            error_code(inspect(&fixture)),
            "runtime_neoforge_minecraft_identity_mismatch"
        );
    }

    #[test]
    fn missing_output_hashes_and_network_processors_are_not_executable() {
        let (mut install, version) = base_documents();
        install["processors"][0]["outputs"] = serde_json::json!({});
        let fixture = make_fixture(build_installer(&install, &version, &[]));
        let plan = inspect(&fixture).expect("parse plan with explicit blocker");
        assert_eq!(
            plan.execution_readiness().blocker_code.as_deref(),
            Some("runtime_neoforge_processor_output_hashes_required")
        );

        let (mut install, version) = base_documents();
        install["processors"][0]["args"] =
            serde_json::json!(["--task", "DOWNLOAD_MOJMAPS", "--output", "{PATCHED}"]);
        let fixture = make_fixture(build_installer(&install, &version, &[]));
        let plan = inspect(&fixture).expect("parse network-requiring plan");
        assert_eq!(
            plan.execution_readiness().blocker_code.as_deref(),
            Some("runtime_neoforge_processor_network_required")
        );
    }

    #[derive(Clone)]
    struct CountingSandbox {
        calls: Arc<AtomicUsize>,
    }

    impl NeoForgeProcessSandbox for CountingSandbox {
        fn capabilities(&self) -> NeoForgeSandboxCapabilities {
            NeoForgeSandboxCapabilities {
                no_network: false,
                no_shell: true,
                clears_environment: true,
                bounded_output: true,
                process_tree_timeout: true,
                exact_write_allowlist: true,
            }
        }

        fn execute(
            &self,
            invocation: &NeoForgeSandboxInvocation,
        ) -> AppResult<NeoForgeSandboxResult> {
            let _ = (
                invocation.executable(),
                invocation.current_directory(),
                invocation.arguments(),
                invocation.writable_outputs(),
                invocation.timeout(),
                invocation.maximum_output_bytes(),
            );
            self.calls.fetch_add(1, Ordering::SeqCst);
            unreachable!("inadequate sandbox must never execute")
        }
    }

    #[test]
    fn inadequate_sandbox_is_rejected_before_process_execution() {
        let (install, version) = base_documents();
        let fixture = make_fixture(build_installer(&install, &version, &[]));
        let plan = inspect(&fixture).expect("plan");
        let calls = Arc::new(AtomicUsize::new(0));
        let sandbox = CountingSandbox {
            calls: Arc::clone(&calls),
        };
        let default_sandbox = NoNeoForgeProcessSandbox;
        assert!(!default_sandbox.capabilities().is_strict());

        let result = execute_client_processors(
            &PathRegistry::new(
                &fixture.root,
                [RegisteredRoot {
                    id: "test".into(),
                    path: fixture.root.clone(),
                }],
            )
            .expect("registry"),
            &plan,
            &NeoForgeExecutionContext {
                java: fixture.installer.clone(),
                staging: fixture.installer.clone(),
                installer: NeoForgeVerifiedFile {
                    path: fixture.installer.clone(),
                    size_bytes: fixture.source.size_bytes,
                    sha256: fixture.source.sha256.clone().expect("sha256"),
                },
                minecraft_client: NeoForgeVerifiedFile {
                    path: fixture.installer.clone(),
                    size_bytes: fixture.source.size_bytes,
                    sha256: fixture.source.sha256.clone().expect("sha256"),
                },
                artifacts: BTreeMap::new(),
            },
            &sandbox,
        );
        assert_eq!(
            error_code(result),
            "runtime_neoforge_process_sandbox_inadequate"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
