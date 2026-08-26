use crate::{
    components::S9labComponentProvider,
    download::{CancellationToken, ProviderId},
    error::{AppError, AppResult},
    foundation::CoreServices,
    minecraft::{
        instance_settings::{InstanceSettings, InstanceSettingsStore},
        java_runtime::JavaRuntimeResolver,
        neoforge::inspect_verified_installer,
        profile_launch::{
            LaunchSecrets, ProfileLaunchRequest, ProfileLaunchState, ProfileLaunchStatus,
            ProfileProcessManager,
        },
        profile_sharing::ProfileSharingService,
        resolver::{
            LaunchArgument, LaunchArgumentValue, LaunchRule, LoaderCatalogEntry,
            MinecraftCatalogEntry, ResolvedLoader, ResolvedMinecraftVersion, RuntimeArtifactKind,
            RuntimeArtifactSource, RuntimeResolver,
        },
    },
    operations::model::{
        canonical_json, new_identifier, sha256_hex, CacheMaterialization, OperationType,
        ProfileInstallPlan,
    },
    profiles::model::{
        LockedCacheBlob, ProfileLockV2, ProfileManifestV2, ProfileSummary, ResolvedLaunchArgument,
        ResolvedLaunchConfiguration, ResolvedLaunchRule, S9labComponentSelection,
    },
    runtime::{
        validate_profile_runtime_intent, validate_resolved_runtime_lock, CapabilityStatus,
        LoaderKind, LoaderSelection, ProfileRuntimeIntent, ResolvedRuntimeItem,
        ResolvedRuntimeLockV1, RuntimeArtifactKind as LockedArtifactKind, S9labComponentManifestV1,
        RUNTIME_LOCK_FORMAT, RUNTIME_LOCK_FORMAT_VERSION,
    },
    security::{paths::validate_existing_chain, PathRegistry},
    storage::{
        models::{ProfileRecord, RuntimeQueryProjection},
        Storage,
    },
};
use chrono::Utc;
use futures_util::{stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
    sync::Arc,
};

const PROFILE_MANIFEST_FORMAT: &str = "site.s9lab.profile";
const PROFILE_LOCK_FORMAT: &str = "site.s9lab.profile-lock";
const PROFILE_FORMAT_VERSION: u32 = 2;
const MAX_PROFILE_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const DOWNLOAD_CONCURRENCY: usize = 12;
const VERSION_CATALOG_CACHE_SECONDS: i64 = 6 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedVersionCatalog {
    cached_at_unix: i64,
    entries: Vec<MinecraftCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase5RuntimeCatalog {
    pub minecraft_versions: Vec<MinecraftCatalogEntry>,
    pub fabric_versions: Vec<LoaderCatalogEntry>,
    pub selected_minecraft_java_major: Option<u16>,
    pub neoforge_capability: CapabilityStatus,
    pub s9lab_component_capability: CapabilityStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase5ComponentCatalogEntry {
    pub component_id: String,
    pub component_version: String,
    pub minecraft_version: String,
    pub loader: LoaderSelection,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase5ComponentCatalog {
    pub capability: CapabilityStatus,
    pub entries: Vec<Phase5ComponentCatalogEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase5RuntimeStatus {
    pub profile_id: String,
    pub active_revision_id: String,
    pub lifecycle_state: String,
    pub install_state: String,
    pub runtime: Option<ProfileRuntimeIntent>,
    pub component: Option<InstalledComponentSummary>,
    pub launches: Vec<ProfileLaunchStatus>,
    pub s9lab_component_capability: CapabilityStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase5ProfileWorkspaceEntry {
    pub profile: ProfileSummary,
    pub runtime: Phase5RuntimeStatus,
    pub settings: InstanceSettings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase5ProfilesWorkspace {
    pub entries: Vec<Phase5ProfileWorkspaceEntry>,
    pub launches: Vec<ProfileLaunchStatus>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledComponentSummary {
    pub component_id: String,
    pub component_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOperationResult {
    pub operation_id: String,
    pub profile_id: String,
    pub revision_id: String,
    pub install_state: String,
}

#[derive(Clone)]
pub struct MinecraftRuntimeService {
    storage: Storage,
    registry: Arc<PathRegistry>,
    operations: crate::operations::engine::OperationEngine,
    downloads: crate::download::DownloadService,
    cache: crate::cache::CacheStore,
    resolver: RuntimeResolver,
    java: JavaRuntimeResolver,
    components: S9labComponentProvider,
    processes: ProfileProcessManager,
    synced_sharing_launches: Arc<tokio::sync::Mutex<BTreeSet<String>>>,
}

impl MinecraftRuntimeService {
    pub fn from_core(core: &CoreServices) -> AppResult<Self> {
        Ok(Self {
            storage: core.storage().clone(),
            registry: core.registry().clone(),
            operations: core.operations().clone(),
            downloads: core.download().clone(),
            cache: core.cache().clone(),
            resolver: RuntimeResolver::production()?,
            java: JavaRuntimeResolver::new(core.registry().clone()),
            components: S9labComponentProvider::production(),
            processes: ProfileProcessManager::default(),
            synced_sharing_launches: Arc::new(tokio::sync::Mutex::new(BTreeSet::new())),
        })
    }

    pub async fn catalog(
        &self,
        minecraft_version: Option<&str>,
    ) -> AppResult<Phase5RuntimeCatalog> {
        let minecraft_versions = if minecraft_version.is_none() {
            match self.load_version_catalog_cache()? {
                Some(entries) => entries,
                None => {
                    let entries = self.resolver.minecraft_catalog().await?;
                    let _ = self.save_version_catalog_cache(&entries);
                    entries
                }
            }
        } else {
            let entries = self.resolver.minecraft_catalog().await?;
            let _ = self.save_version_catalog_cache(&entries);
            entries
        };
        let (fabric_versions, selected_minecraft_java_major) = match minecraft_version {
            Some(version) => {
                // Resolve only the selected version.  The global Mojang manifest
                // deliberately does not contain Java requirements, and fetching
                // every version document would be slow and unnecessary.
                let resolved = self.resolver.resolve_mojang(version).await?;
                (
                    self.resolver
                        .fabric_catalog(version)
                        .await
                        .unwrap_or_default(),
                    Some(
                        u16::try_from(resolved.java_major)
                            .map_err(|_| AppError::coded("runtime_java_major_unsupported"))?,
                    ),
                )
            }
            None => (Vec::new(), None),
        };
        Ok(Phase5RuntimeCatalog {
            minecraft_versions,
            fabric_versions,
            selected_minecraft_java_major,
            neoforge_capability: CapabilityStatus::unconfigured(
                "runtime.neoforge",
                "runtime_neoforge_pipeline_unavailable",
            ),
            s9lab_component_capability: self.components.capability_status(),
        })
    }

    fn load_version_catalog_cache(&self) -> AppResult<Option<Vec<MinecraftCatalogEntry>>> {
        let path = self
            .registry
            .resolve("cache", "minecraft-version-catalog.json")?;
        if !path.absolute().is_file() {
            return Ok(None);
        }
        validate_existing_chain(path.anchor(), path.absolute())?;
        let metadata = fs::metadata(path.absolute())?;
        if metadata.len() == 0 || metadata.len() > 2 * 1024 * 1024 {
            return Ok(None);
        }
        let cache: PersistedVersionCatalog =
            match serde_json::from_slice(&fs::read(path.absolute())?) {
                Ok(cache) => cache,
                Err(_) => return Ok(None),
            };
        if Utc::now().timestamp().saturating_sub(cache.cached_at_unix)
            > VERSION_CATALOG_CACHE_SECONDS
            || cache.entries.is_empty()
        {
            return Ok(None);
        }
        Ok(Some(cache.entries))
    }

    fn save_version_catalog_cache(&self, entries: &[MinecraftCatalogEntry]) -> AppResult<()> {
        let path = self
            .registry
            .resolve("cache", "minecraft-version-catalog.json")?;
        let temporary = self
            .registry
            .resolve("cache", "minecraft-version-catalog.json.part")?;
        let document = PersistedVersionCatalog {
            cached_at_unix: Utc::now().timestamp(),
            entries: entries.to_vec(),
        };
        fs::write(temporary.absolute(), serde_json::to_vec(&document)?)?;
        if path.absolute().exists() {
            fs::remove_file(path.absolute())?;
        }
        fs::rename(temporary.absolute(), path.absolute())?;
        Ok(())
    }

    pub async fn component_catalog(
        &self,
        runtime: ProfileRuntimeIntent,
    ) -> AppResult<Phase5ComponentCatalog> {
        let capability = self.components.capability_status();
        if !capability.is_available() {
            return Ok(Phase5ComponentCatalog {
                capability,
                entries: Vec::new(),
            });
        }

        validate_profile_runtime_intent(&runtime)?;
        let catalog = self.components.fetch_catalog().await?;
        let mut entries = catalog
            .components()
            .iter()
            .filter(|manifest| component_matches_runtime(manifest, &runtime))
            .map(component_catalog_entry)
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            (
                &left.component_id,
                &left.component_version,
                &left.minecraft_version,
                left.loader.kind.as_str(),
                left.loader.loader_version.as_deref(),
            )
                .cmp(&(
                    &right.component_id,
                    &right.component_version,
                    &right.minecraft_version,
                    right.loader.kind.as_str(),
                    right.loader.loader_version.as_deref(),
                ))
        });
        Ok(Phase5ComponentCatalog {
            capability,
            entries,
        })
    }

    pub async fn status(&self, profile_id: &str) -> AppResult<Phase5RuntimeStatus> {
        let launches = self.launch_statuses().await?;
        self.status_with_launches(profile_id, &launches)
    }

    pub async fn profiles_workspace(
        &self,
        profiles: Vec<ProfileSummary>,
    ) -> AppResult<Phase5ProfilesWorkspace> {
        let launches = self.launch_statuses().await?;
        let running_profile_ids = launches
            .iter()
            .filter(|launch| {
                matches!(
                    launch.state,
                    ProfileLaunchState::Preparing
                        | ProfileLaunchState::CheckingFiles
                        | ProfileLaunchState::Downloading
                        | ProfileLaunchState::Starting
                        | ProfileLaunchState::Running
                        | ProfileLaunchState::Stopping
                )
            })
            .map(|launch| launch.profile_id.clone())
            .collect::<Vec<_>>();
        ProfileSharingService::new(self.registry.clone(), self.storage.clone())
            .sync_servers_to_inactive_profiles(&running_profile_ids)?;
        let settings_store = InstanceSettingsStore::new(self.registry.clone());
        let mut entries = Vec::with_capacity(profiles.len());
        for profile in profiles
            .into_iter()
            .filter(|profile| profile.lifecycle_state == "active")
        {
            let runtime = self.status_with_launches(&profile.id, &launches)?;
            let settings = settings_store.load(&profile.id)?;
            entries.push(Phase5ProfileWorkspaceEntry {
                profile,
                runtime,
                settings,
            });
        }
        Ok(Phase5ProfilesWorkspace { entries, launches })
    }

    fn status_with_launches(
        &self,
        profile_id: &str,
        all_launches: &[ProfileLaunchStatus],
    ) -> AppResult<Phase5RuntimeStatus> {
        let profile = self.profile_for_read(profile_id)?;
        let projection = self.storage.runtime_projection(profile_id)?;
        let launches = all_launches
            .iter()
            .filter(|launch| launch.profile_id == profile_id)
            .cloned()
            .collect::<Vec<_>>();
        let active_revision_id = profile
            .active_revision_id
            .clone()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        let (install_state, runtime, component) = match projection {
            None => ("not-configured".to_string(), None, None),
            Some(projection) => match self.read_active_v2(&profile) {
                Ok((manifest, _)) => {
                    let component = component_summary(&manifest.s9lab_component);
                    let projected_component = projection
                        .component_id
                        .zip(projection.component_version)
                        .map(
                            |(component_id, component_version)| InstalledComponentSummary {
                                component_id,
                                component_version,
                            },
                        );
                    let install_state = if projection.revision_id != active_revision_id
                        || projected_component != component
                    {
                        "repair-required".to_string()
                    } else {
                        projection.install_state
                    };
                    (install_state, Some(manifest.runtime), component)
                }
                Err(_) => ("repair-required".to_string(), None, None),
            },
        };
        Ok(Phase5RuntimeStatus {
            profile_id: profile.id,
            active_revision_id,
            lifecycle_state: profile.lifecycle_state,
            install_state,
            runtime,
            component,
            launches,
            s9lab_component_capability: self.components.capability_status(),
        })
    }

    pub async fn install(
        &self,
        profile_id: &str,
        runtime: ProfileRuntimeIntent,
        component: S9labComponentSelection,
    ) -> AppResult<RuntimeOperationResult> {
        validate_profile_runtime_intent(&runtime)?;
        let profile = self.profile_for_mutation(profile_id)?;
        let prepared = self
            .resolve_and_download(&profile, runtime, component)
            .await?;
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            service.commit_prepared(prepared, OperationType::RuntimeInstall)
        })
        .await
        .map_err(|_| AppError::coded("runtime_worker_failed"))?
    }

    pub fn repair(&self, profile_id: &str) -> AppResult<RuntimeOperationResult> {
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, lock) = self.read_active_v2(&profile)?;
        self.verify_cache_for_lock(&lock)?;
        let prepared = self.prepare_from_locked(&profile, manifest, lock)?;
        self.commit_prepared(prepared, OperationType::RuntimeRepair)
    }

    pub async fn change_component(
        &self,
        profile_id: &str,
        component: S9labComponentSelection,
    ) -> AppResult<RuntimeOperationResult> {
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, _) = self.read_active_v2(&profile)?;
        let prepared = self
            .resolve_and_download(&profile, manifest.runtime, component)
            .await?;
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            service.commit_prepared(prepared, OperationType::ComponentChange)
        })
        .await
        .map_err(|_| AppError::coded("runtime_worker_failed"))?
    }

    pub async fn launch(
        &self,
        auth: &crate::auth::service::AuthService,
        launch_id: &str,
        profile_id: &str,
        memory_mb: u32,
    ) -> AppResult<ProfileLaunchStatus> {
        let launch_fast_total = std::time::Instant::now();
        let profile = self.profile_for_mutation(profile_id)?;
        let (_, lock) = self.read_active_v2(&profile)?;
        let step = std::time::Instant::now();
        self.verify_revision_runtime(&profile, &lock)?;
        eprintln!(
            "[snine-launch-fast] runtime verification: {} ms",
            step.elapsed().as_millis()
        );
        let projection = self
            .storage
            .runtime_projection(profile_id)?
            .ok_or_else(|| AppError::coded("runtime_not_installed"))?;
        if projection.install_state != "installed" || projection.revision_id != lock.revision_id {
            return Err(AppError::coded("runtime_repair_required"));
        }
        let step = std::time::Instant::now();
        crate::content_projection::project_content_for_launch(
            &self.registry,
            profile_id,
            &lock.revision_id,
            lock.content.as_ref(),
        )?;
        eprintln!(
            "[snine-launch-fast] content projection: {} ms",
            step.elapsed().as_millis()
        );
        let account_id = profile
            .account_id
            .as_deref()
            .ok_or_else(|| AppError::coded("runtime_profile_account_required"))?;
        let step = std::time::Instant::now();
        let (account, session) = auth.ensure_minecraft_session(account_id).await?;
        eprintln!(
            "[snine-launch-fast] account/session: {} ms",
            step.elapsed().as_millis()
        );
        let settings_store = InstanceSettingsStore::new(self.registry.clone());
        let mut settings = settings_store.load(profile_id)?;
        settings.max_ram_mb = memory_mb;
        crate::minecraft::instance_settings::validate_settings(&settings)?;
        let step = std::time::Instant::now();
        ProfileSharingService::new(self.registry.clone(), self.storage.clone())
            .prepare_for_launch(profile_id, &settings)?;
        eprintln!(
            "[snine-launch-fast] shared profile data: {} ms",
            step.elapsed().as_millis()
        );
        let step = std::time::Instant::now();
        let java = match settings.custom_java_executable.as_deref() {
            Some(executable) => {
                self.java
                    .resolve_custom(Path::new(executable), lock.launch.java_major_version)
                    .await?
            }
            None => self.java.resolve(&lock.runtime.runtime.java).await?,
        };
        eprintln!(
            "[snine-launch-fast] Java resolve/probe: {} ms",
            step.elapsed().as_millis()
        );
        if java.major_version != lock.launch.java_major_version {
            return Err(AppError::coded("runtime_java_lock_mismatch"));
        }
        let revision_id = profile
            .active_revision_id
            .as_deref()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        let step = std::time::Instant::now();
        let launch = self
            .processes
            .launch(
                &self.registry,
                ProfileLaunchRequest {
                    launch_id,
                    profile_id,
                    revision_id,
                    lock: &lock,
                    java_executable: &java.executable,
                    min_memory_mb: settings.min_ram_mb,
                    max_memory_mb: settings.max_ram_mb,
                    custom_jvm_arguments: &settings.jvm_arguments,
                    width: settings.width,
                    height: settings.height,
                    fullscreen: settings.fullscreen,
                    secrets: LaunchSecrets {
                        account: &account,
                        session: &session,
                    },
                },
            )
            .await?;
        eprintln!(
            "[snine-launch-fast] native prep + process spawn: {} ms",
            step.elapsed().as_millis()
        );
        eprintln!(
            "[snine-launch-fast] total runtime.launch: {} ms",
            launch_fast_total.elapsed().as_millis()
        );
        if let Err(error) = settings_store.mark_played(profile_id, Utc::now().timestamp()) {
            eprintln!("[SNine Launcher] last-played timestamp could not be persisted: {error}");
        }
        Ok(launch)
    }

    pub fn instance_settings(&self, profile_id: &str) -> AppResult<InstanceSettings> {
        self.profile_for_read(profile_id)?;
        InstanceSettingsStore::new(self.registry.clone()).load(profile_id)
    }

    pub fn save_instance_settings(
        &self,
        profile_id: &str,
        settings: &InstanceSettings,
    ) -> AppResult<InstanceSettings> {
        self.profile_for_mutation(profile_id)?;
        let store = InstanceSettingsStore::new(self.registry.clone());
        let previous = store.load(profile_id)?;
        ProfileSharingService::new(self.registry.clone(), self.storage.clone())
            .settings_changed(profile_id, &previous, settings)?;
        store.save(profile_id, settings)
    }

    pub fn open_instance_folder(&self, profile_id: &str, folder: &str) -> AppResult<()> {
        self.profile_for_read(profile_id)?;
        let store = InstanceSettingsStore::new(self.registry.clone());
        let settings = store.load(profile_id)?;
        let sharing = ProfileSharingService::new(self.registry.clone(), self.storage.clone());
        let path = match sharing.directory_for_open(profile_id, folder, &settings)? {
            Some(path) => path,
            None => store.instance_directory(profile_id, folder)?,
        };
        opener::open(path).map_err(|_| AppError::coded("instance_folder_open_failed"))
    }

    pub async fn reserve_launch(
        &self,
        profile_id: &str,
        account_name: &str,
    ) -> AppResult<ProfileLaunchStatus> {
        self.profile_for_mutation(profile_id)?;
        self.processes.reserve(profile_id, account_name).await
    }

    pub async fn transition_launch(
        &self,
        launch_id: &str,
        state: ProfileLaunchState,
    ) -> AppResult<ProfileLaunchStatus> {
        self.processes.transition_pending(launch_id, state).await
    }

    pub async fn fail_launch(
        &self,
        launch_id: &str,
        failure_code: &str,
    ) -> AppResult<ProfileLaunchStatus> {
        self.processes.fail_pending(launch_id, failure_code).await
    }

    pub async fn stop(&self, launch_id: &str) -> AppResult<ProfileLaunchStatus> {
        let status = self.processes.stop(launch_id).await?;
        self.sync_profile_sharing_once(&status).await;
        Ok(status)
    }

    pub async fn launch_statuses(&self) -> AppResult<Vec<ProfileLaunchStatus>> {
        let statuses = self.processes.statuses().await?;
        for status in &statuses {
            if status.state == ProfileLaunchState::Exited {
                self.sync_profile_sharing_once(status).await;
            }
        }
        Ok(statuses)
    }

    async fn sync_profile_sharing_once(&self, status: &ProfileLaunchStatus) {
        let mut synced = self.synced_sharing_launches.lock().await;
        if !synced.insert(status.launch_id.clone()) {
            return;
        }
        if synced.len() > 64 {
            synced.clear();
            synced.insert(status.launch_id.clone());
        }
        drop(synced);
        let registry = self.registry.clone();
        let storage = self.storage.clone();
        let profile_id = status.profile_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(error) =
                ProfileSharingService::new(registry, storage).sync_finished_profile(&profile_id)
            {
                eprintln!("[SNine Launcher] shared profile data sync failed: {error}");
            }
        });
    }

    async fn resolve_and_download(
        &self,
        profile: &ProfileRecord,
        runtime: ProfileRuntimeIntent,
        component: S9labComponentSelection,
    ) -> AppResult<PreparedRevision> {
        let (desired_content, content) = if self.storage.runtime_projection(&profile.id)?.is_some()
        {
            let (current_manifest, current_lock) = self.read_active_v2(profile)?;
            if current_lock.content.is_some() && current_manifest.runtime != runtime {
                return Err(AppError::coded(
                    "content_runtime_change_requires_resolution",
                ));
            }
            if current_manifest.runtime == runtime {
                (current_manifest.desired_content, current_lock.content)
            } else {
                (Vec::new(), None)
            }
        } else {
            (Vec::new(), None)
        };
        let base = self
            .resolver
            .resolve_mojang(&runtime.minecraft_version)
            .await?;
        if u32::from(runtime.java.major_version()) != base.java_major {
            return Err(AppError::coded_with(
                "runtime_java_requirement_mismatch",
                [
                    ("requested", runtime.java.major_version().to_string()),
                    ("required", base.java_major.to_string()),
                ],
            ));
        }
        self.java.resolve(&runtime.java).await?;
        let loader = match runtime.loader.kind {
            LoaderKind::Vanilla => None,
            LoaderKind::Fabric => {
                let version = runtime
                    .loader
                    .loader_version
                    .as_deref()
                    .ok_or_else(|| AppError::coded("runtime_loader_version_required"))?;
                Some(
                    self.resolver
                        .resolve_fabric(&runtime.minecraft_version, version)
                        .await?,
                )
            }
            LoaderKind::Neoforge => {
                let version = runtime
                    .loader
                    .loader_version
                    .as_deref()
                    .ok_or_else(|| AppError::coded("runtime_loader_version_required"))?;
                let installer = self
                    .resolver
                    .resolve_neoforge_installer(&runtime.minecraft_version, version)
                    .await?;
                let inspection_operation = new_identifier("op");
                let cached_sha1 =
                    self.index_cached_sha1_sources(std::slice::from_ref(&installer))?;
                let downloaded = self
                    .download_source(&inspection_operation, 0, installer, &cached_sha1)
                    .await?;
                let cache_relative =
                    crate::cache::CacheStore::blob_relative_path(&downloaded.sha256)?;
                let installer_path = self.registry.resolve("cache-blobs", cache_relative)?;
                let plan = inspect_verified_installer(
                    &installer_path,
                    &downloaded.source,
                    &runtime.minecraft_version,
                    version,
                )?;
                let resolved_loader = plan.resolved_loader();
                validate_source_targets(&resolved_loader.artifacts)?;
                let readiness = plan.execution_readiness();
                return Err(AppError::coded(readiness.blocker_code.unwrap_or_else(
                    || "runtime_neoforge_process_sandbox_unconfigured".into(),
                )));
            }
        };
        let asset_index = base
            .artifacts
            .iter()
            .find(|artifact| artifact.kind == RuntimeArtifactKind::AssetIndex)
            .ok_or_else(|| AppError::coded("runtime_asset_index_missing"))?;
        let mut sources = base.artifacts.clone();
        sources.extend(self.resolver.resolve_assets(asset_index).await?);
        if let Some(loader) = &loader {
            sources.extend(loader.artifacts.clone());
        }
        validate_source_targets(&sources)?;
        self.validate_runtime_projection_paths(&profile.id, &sources)?;

        let operation_id = new_identifier("op");
        let downloaded = self.download_sources(&operation_id, sources).await?;
        let mut locked_items = downloaded
            .iter()
            .map(|download| lock_item(&download.source, &download.sha256))
            .collect::<AppResult<Vec<_>>>()?;

        let mut component_summary = None;
        if let S9labComponentSelection::Catalog {
            component_id,
            component_version,
        } = &component
        {
            let catalog = self.components.fetch_catalog().await?;
            let resolved = self.components.resolve_exact(
                &catalog,
                &runtime,
                component_id,
                component_version,
            )?;
            let destination = self.registry.resolve(
                "staging-operations",
                format!("{operation_id}/downloads/s9lab-component.jar"),
            )?;
            let cancellation = CancellationToken::default();
            self.components
                .download_and_inspect(&resolved, &destination, &cancellation)
                .await?;
            self.cache.activate_verified_copy(
                &format!("{operation_id}/downloads/s9lab-component.jar"),
                &resolved.runtime_item().sha256,
                resolved.runtime_item().size_bytes,
            )?;
            locked_items.push(resolved.runtime_item().clone());
            component_summary = Some(InstalledComponentSummary {
                component_id: resolved.manifest().component_id.clone(),
                component_version: resolved.manifest().component_version.clone(),
            });
        }

        locked_items.sort_by(|left, right| left.relative_target.cmp(&right.relative_target));
        let runtime_lock = ResolvedRuntimeLockV1 {
            format: RUNTIME_LOCK_FORMAT.into(),
            format_version: RUNTIME_LOCK_FORMAT_VERSION,
            runtime: runtime.clone(),
            items: locked_items,
        };
        validate_resolved_runtime_lock(&runtime_lock)?;
        let launch = build_launch_configuration(&base, loader.as_ref())?;
        self.prepare_revision(
            profile,
            RevisionPreparation {
                runtime,
                component,
                component_summary,
                runtime_lock,
                launch,
                operation_id,
                desired_content,
                content,
            },
        )
    }

    fn validate_runtime_projection_paths(
        &self,
        profile_id: &str,
        sources: &[RuntimeArtifactSource],
    ) -> AppResult<()> {
        // Revision identifiers always use the same bounded shape. Validate the
        // final profile paths before downloading any large runtime artifacts.
        const REVISION_PATH_PROBE: &str = "rev-00000000000000000000000000000000";
        for source in sources {
            self.registry.resolve(
                "profiles",
                format!(
                    "{profile_id}/revisions/{REVISION_PATH_PROBE}/runtime/{}",
                    source.target_relative_path
                ),
            )?;
        }
        Ok(())
    }

    async fn download_sources(
        &self,
        operation_id: &str,
        sources: Vec<RuntimeArtifactSource>,
    ) -> AppResult<Vec<DownloadedSource>> {
        let service = self.clone();
        let operation_id = operation_id.to_string();
        let cached_sha1 = Arc::new(self.index_cached_sha1_sources(&sources)?);
        stream::iter(sources.into_iter().enumerate().map(move |(index, source)| {
            let service = service.clone();
            let operation_id = operation_id.clone();
            let cached_sha1 = cached_sha1.clone();
            async move {
                service
                    .download_source(&operation_id, index, source, &cached_sha1)
                    .await
            }
        }))
        .buffer_unordered(DOWNLOAD_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await
    }

    async fn download_source(
        &self,
        operation_id: &str,
        index: usize,
        source: RuntimeArtifactSource,
        cached_sha1: &BTreeMap<(u64, String), String>,
    ) -> AppResult<DownloadedSource> {
        if let Some(expected_sha256) = source.sha256.clone() {
            if let Some(blob) = self.storage.cache_blob(&expected_sha256)? {
                if blob.state == "verified" && blob.size_bytes == source.size_bytes {
                    return Ok(DownloadedSource {
                        source,
                        sha256: expected_sha256,
                    });
                }
            }
        }
        if let Some(expected_sha1) = source.sha1.as_ref() {
            if let Some(sha256) = cached_sha1.get(&(source.size_bytes, expected_sha1.clone())) {
                return Ok(DownloadedSource {
                    source,
                    sha256: sha256.clone(),
                });
            }
        }
        let provider = source_provider_id(&source.provider)?;
        let staging = format!("{operation_id}/downloads/{index:06}.bin");
        let request = match (source.sha256.as_deref(), source.sha1.as_deref()) {
            (Some(sha256), _) => self.downloads.resolve(
                provider,
                &source.url,
                &staging,
                source.size_bytes,
                sha256,
            )?,
            (None, Some(sha1)) => self.downloads.resolve_upstream_sha1(
                provider,
                &source.url,
                &staging,
                source.size_bytes,
                sha1,
            )?,
            (None, None) => return Err(AppError::coded("runtime_artifact_digest_missing")),
        };
        let result = self
            .downloads
            .download(&request, &CancellationToken::default())
            .await?;
        self.cache
            .activate_verified_copy(&staging, &result.sha256, result.size_bytes)?;
        Ok(DownloadedSource {
            source,
            sha256: result.sha256,
        })
    }

    fn index_cached_sha1_sources(
        &self,
        sources: &[RuntimeArtifactSource],
    ) -> AppResult<BTreeMap<(u64, String), String>> {
        let requested = sources
            .iter()
            .filter_map(|source| {
                source
                    .sha1
                    .as_ref()
                    .map(|sha1| (source.size_bytes, sha1.clone()))
            })
            .collect::<BTreeSet<_>>();
        if requested.is_empty() {
            return Ok(BTreeMap::new());
        }

        let mut matches = BTreeMap::new();
        for blob in self.storage.cache_blobs()? {
            if blob.state != "verified"
                || !requested.iter().any(|(size, _)| *size == blob.size_bytes)
            {
                continue;
            }
            let expected_relative = crate::cache::CacheStore::blob_relative_path(&blob.sha256)?;
            if blob.relative_path != expected_relative {
                return Err(AppError::coded("runtime_cache_blob_path_mismatch"));
            }
            let path = self.registry.resolve("cache-blobs", expected_relative)?;
            let metadata = fs::metadata(path.absolute())?;
            if !metadata.is_file() || metadata.len() != blob.size_bytes {
                return Err(AppError::coded("runtime_cache_blob_invalid"));
            }
            let (actual_sha1, actual_sha256) = hash_file_sha1_sha256(path.absolute())?;
            if actual_sha256 != blob.sha256 {
                return Err(AppError::coded("runtime_cache_blob_invalid"));
            }
            let key = (blob.size_bytes, actual_sha1);
            if requested.contains(&key) && matches.insert(key, blob.sha256).is_some() {
                return Err(AppError::coded("runtime_cache_sha1_ambiguous"));
            }
        }
        Ok(matches)
    }

    fn prepare_revision(
        &self,
        profile: &ProfileRecord,
        preparation: RevisionPreparation,
    ) -> AppResult<PreparedRevision> {
        let RevisionPreparation {
            runtime,
            component,
            component_summary,
            runtime_lock,
            launch,
            operation_id,
            desired_content,
            content,
        } = preparation;
        let revision_id = new_identifier("rev");
        let manifest = ProfileManifestV2 {
            format: PROFILE_MANIFEST_FORMAT.into(),
            format_version: PROFILE_FORMAT_VERSION,
            profile_id: profile.id.clone(),
            created_at_unix: Utc::now().timestamp(),
            runtime,
            s9lab_component: component,
            desired_content,
            mutable_directories: crate::profiles::service::MUTABLE_INSTANCE_DIRECTORIES
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            isolation_policy: "verified-copy-no-hardlinks".into(),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let mut cache_blobs = runtime_lock
            .items
            .iter()
            .map(|item| LockedCacheBlob {
                sha256: item.sha256.clone(),
                size_bytes: item.size_bytes,
            })
            .chain(
                content
                    .iter()
                    .flat_map(|content| content.items.iter())
                    .map(|item| LockedCacheBlob {
                        sha256: item.sha256.clone(),
                        size_bytes: item.size_bytes,
                    }),
            )
            .collect::<Vec<_>>();
        cache_blobs.extend(
            content
                .iter()
                .flat_map(|content| content.overrides.iter())
                .map(|override_file| LockedCacheBlob {
                    sha256: override_file.sha256.clone(),
                    size_bytes: override_file.size_bytes,
                }),
        );
        cache_blobs.sort();
        cache_blobs.dedup();
        let lock = ProfileLockV2 {
            format: PROFILE_LOCK_FORMAT.into(),
            format_version: PROFILE_FORMAT_VERSION,
            profile_id: profile.id.clone(),
            revision_id: revision_id.clone(),
            manifest_sha256: manifest_sha256.clone(),
            runtime: runtime_lock,
            launch,
            content,
            cache_blobs,
        };
        let lock_json = canonical_json(&lock)?;
        let lock_sha256 = sha256_hex(lock_json.as_bytes());
        let mut cache_materializations: Vec<CacheMaterialization> = lock
            .runtime
            .items
            .iter()
            .map(|item| CacheMaterialization {
                blob_sha256: item.sha256.clone(),
                size_bytes: item.size_bytes,
                relative_path: format!("runtime/{}", item.relative_target),
            })
            .chain(
                lock.content
                    .iter()
                    .flat_map(|content| content.items.iter())
                    .map(|item| CacheMaterialization {
                        blob_sha256: item.sha256.clone(),
                        size_bytes: item.size_bytes,
                        relative_path: format!("content/{}", item.relative_target),
                    }),
            )
            .collect();
        cache_materializations.extend(
            lock.content
                .iter()
                .flat_map(|content| content.overrides.iter())
                .map(|override_file| CacheMaterialization {
                    blob_sha256: override_file.sha256.clone(),
                    size_bytes: override_file.size_bytes,
                    relative_path: format!("content/{}", override_file.relative_target),
                }),
        );
        let runtime_projection = RuntimeQueryProjection {
            profile_id: profile.id.clone(),
            revision_id: revision_id.clone(),
            minecraft_version: lock.runtime.runtime.minecraft_version.clone(),
            loader_kind: lock.runtime.runtime.loader.kind.as_str().into(),
            loader_version: lock.runtime.runtime.loader.loader_version.clone(),
            component_id: component_summary
                .as_ref()
                .map(|value| value.component_id.clone()),
            component_version: component_summary
                .as_ref()
                .map(|value| value.component_version.clone()),
            install_state: "installed".into(),
            updated_at_unix: Utc::now().timestamp(),
        };
        Ok(PreparedRevision {
            plan: ProfileInstallPlan {
                operation_id,
                profile_id: profile.id.clone(),
                revision_id,
                previous_revision_id: profile.active_revision_id.clone(),
                manifest_json,
                manifest_sha256,
                lock_json,
                lock_sha256,
                payload_files: Vec::new(),
                cache_materializations,
                runtime_projection: Some(runtime_projection),
                previous_runtime_projection: self.storage.runtime_projection(&profile.id)?,
                cleanup_profile_on_rollback: false,
            },
        })
    }

    fn prepare_from_locked(
        &self,
        profile: &ProfileRecord,
        manifest: ProfileManifestV2,
        lock: ProfileLockV2,
    ) -> AppResult<PreparedRevision> {
        let summary = match &manifest.s9lab_component {
            S9labComponentSelection::Disabled => None,
            S9labComponentSelection::Catalog {
                component_id,
                component_version,
            } => Some(InstalledComponentSummary {
                component_id: component_id.clone(),
                component_version: component_version.clone(),
            }),
        };
        self.prepare_revision(
            profile,
            RevisionPreparation {
                runtime: manifest.runtime,
                component: manifest.s9lab_component,
                component_summary: summary,
                runtime_lock: lock.runtime,
                launch: lock.launch,
                operation_id: new_identifier("op"),
                desired_content: manifest.desired_content,
                content: lock.content,
            },
        )
    }

    fn commit_prepared(
        &self,
        prepared: PreparedRevision,
        operation_type: OperationType,
    ) -> AppResult<RuntimeOperationResult> {
        self.operations
            .plan_profile_operation(&prepared.plan, operation_type)?;
        self.operations.execute(&prepared.plan.operation_id)?;
        Ok(RuntimeOperationResult {
            operation_id: prepared.plan.operation_id,
            profile_id: prepared.plan.profile_id,
            revision_id: prepared.plan.revision_id,
            install_state: "installed".into(),
        })
    }

    fn profile_for_read(&self, profile_id: &str) -> AppResult<ProfileRecord> {
        self.storage
            .profile(profile_id)?
            .ok_or_else(|| AppError::coded_with("profile_not_found", [("profileId", profile_id)]))
    }

    fn profile_for_mutation(&self, profile_id: &str) -> AppResult<ProfileRecord> {
        let profile = self.profile_for_read(profile_id)?;
        if profile.lifecycle_state != "active" {
            return Err(AppError::coded("runtime_profile_not_active"));
        }
        Ok(profile)
    }

    fn read_active_v2(
        &self,
        profile: &ProfileRecord,
    ) -> AppResult<(ProfileManifestV2, ProfileLockV2)> {
        let revision_id = profile
            .active_revision_id
            .as_deref()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        let manifest_path = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{revision_id}/manifest.json", profile.id),
        )?;
        let lock_path = self.registry.resolve(
            "profiles",
            format!("{}/revisions/{revision_id}/lock.json", profile.id),
        )?;
        let manifest_bytes = read_bounded_document(manifest_path.absolute())?;
        let lock_bytes = read_bounded_document(lock_path.absolute())?;
        let manifest: ProfileManifestV2 = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| AppError::coded("runtime_manifest_v2_required"))?;
        let lock: ProfileLockV2 = serde_json::from_slice(&lock_bytes)
            .map_err(|_| AppError::coded("runtime_lock_v2_required"))?;
        if manifest.format != PROFILE_MANIFEST_FORMAT
            || manifest.format_version != PROFILE_FORMAT_VERSION
            || manifest.profile_id != profile.id
            || lock.format != PROFILE_LOCK_FORMAT
            || lock.format_version != PROFILE_FORMAT_VERSION
            || lock.profile_id != profile.id
            || lock.revision_id != revision_id
            || lock.manifest_sha256 != sha256_hex(&manifest_bytes)
        {
            return Err(AppError::coded("runtime_profile_documents_invalid"));
        }
        validate_profile_runtime_intent(&manifest.runtime)?;
        validate_resolved_runtime_lock(&lock.runtime)?;
        if manifest.runtime != lock.runtime.runtime {
            return Err(AppError::coded("runtime_profile_documents_incompatible"));
        }
        Ok((manifest, lock))
    }

    fn verify_cache_for_lock(&self, lock: &ProfileLockV2) -> AppResult<()> {
        for item in &lock.runtime.items {
            let blob = self
                .storage
                .cache_blob(&item.sha256)?
                .ok_or_else(|| AppError::coded("runtime_cache_blob_missing"))?;
            if blob.state != "verified" || blob.size_bytes != item.size_bytes {
                return Err(AppError::coded("runtime_cache_blob_invalid"));
            }
        }
        Ok(())
    }

    fn verify_revision_runtime(
        &self,
        profile: &ProfileRecord,
        lock: &ProfileLockV2,
    ) -> AppResult<()> {
        use sha2::Digest as _;

        let revision_id = profile
            .active_revision_id
            .as_deref()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;

        // Runtime revisions are immutable launcher artifacts. Build a fingerprint from
        // the lock itself (already loaded and trusted) and persist it after one full
        // SHA-256 verification. Normal launches then need one tiny marker read instead
        // of hundreds/thousands of Windows metadata calls before Java can spawn.
        let mut lock_hasher = sha2::Sha256::new();
        lock_hasher.update(b"snine-runtime-trusted-v2\0");
        lock_hasher.update(profile.id.as_bytes());
        lock_hasher.update(b"\0");
        lock_hasher.update(revision_id.as_bytes());
        lock_hasher.update(b"\0");
        for item in &lock.runtime.items {
            lock_hasher.update(item.relative_target.as_bytes());
            lock_hasher.update(b"\0");
            lock_hasher.update(item.sha256.as_bytes());
            lock_hasher.update(item.size_bytes.to_le_bytes());
        }
        let lock_fingerprint = hex::encode(lock_hasher.finalize());
        let fast_marker = self.registry.resolve(
            "data",
            format!("runtime-trusted-v2-{}-{revision_id}.sha256", profile.id),
        )?;
        if fs::read_to_string(fast_marker.absolute())
            .ok()
            .is_some_and(|cached| cached.trim() == lock_fingerprint)
        {
            eprintln!(
                "[snine-launch-fast] runtime immutable-revision cache hit for profile {} revision {}",
                profile.id, revision_id
            );
            return Ok(());
        }

        // V1 only wrote its marker after a complete hash pass. Migrate that proof
        // immediately so installing V3 does not force one more slow full scan.
        let legacy_marker = self.registry.resolve(
            "data",
            format!("runtime-verified-{}-{revision_id}.sha256", profile.id),
        )?;
        if fs::read_to_string(legacy_marker.absolute())
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
        {
            let _ = fs::write(fast_marker.absolute(), lock_fingerprint.as_bytes());
            eprintln!(
                "[snine-launch-fast] migrated verified runtime revision {} to instant cache",
                revision_id
            );
            return Ok(());
        }

        if lock.runtime.items.is_empty() {
            return Err(AppError::coded("runtime_repair_required"));
        }

        let mut files = Vec::with_capacity(lock.runtime.items.len());
        for item in &lock.runtime.items {
            let path = self.registry.resolve(
                "profiles",
                format!(
                    "{}/revisions/{revision_id}/runtime/{}",
                    profile.id, item.relative_target
                ),
            )?;
            validate_existing_chain(path.anchor(), path.absolute())?;
            let metadata = fs::metadata(path.absolute())?;
            if !metadata.is_file() || metadata.len() != item.size_bytes {
                return Err(AppError::coded("runtime_repair_required"));
            }
            files.push((path.absolute().to_path_buf(), item.sha256.clone()));
        }

        let workers = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            .clamp(1, 6)
            .min(files.len().max(1));
        let chunk_size = files.len().div_ceil(workers);
        std::thread::scope(|scope| -> AppResult<()> {
            let mut handles = Vec::new();
            for chunk in files.chunks(chunk_size.max(1)) {
                handles.push(scope.spawn(move || -> AppResult<()> {
                    for (path, expected_sha256) in chunk {
                        if hash_file(path)? != expected_sha256.as_str() {
                            return Err(AppError::coded("runtime_repair_required"));
                        }
                    }
                    Ok(())
                }));
            }
            for handle in handles {
                handle
                    .join()
                    .map_err(|_| AppError::coded("runtime_verification_worker_failed"))??;
            }
            Ok(())
        })?;

        let temporary = fast_marker.absolute().with_extension("sha256.tmp");
        if fs::write(&temporary, lock_fingerprint.as_bytes()).is_ok() {
            let _ = fs::remove_file(fast_marker.absolute());
            let _ = fs::rename(&temporary, fast_marker.absolute());
        } else {
            let _ = fs::remove_file(&temporary);
        }
        Ok(())
    }
}

struct PreparedRevision {
    plan: ProfileInstallPlan,
}

struct RevisionPreparation {
    runtime: ProfileRuntimeIntent,
    component: S9labComponentSelection,
    component_summary: Option<InstalledComponentSummary>,
    runtime_lock: ResolvedRuntimeLockV1,
    launch: ResolvedLaunchConfiguration,
    operation_id: String,
    desired_content: Vec<crate::content::ContentSelection>,
    content: Option<crate::content::ResolvedContentLockV1>,
}

struct DownloadedSource {
    source: RuntimeArtifactSource,
    sha256: String,
}

fn component_matches_runtime(
    manifest: &S9labComponentManifestV1,
    runtime: &ProfileRuntimeIntent,
) -> bool {
    manifest.minecraft_version == runtime.minecraft_version
        && manifest.loader.kind == runtime.loader.kind
        && manifest
            .loader
            .loader_version
            .as_ref()
            .is_none_or(|required| runtime.loader.loader_version.as_ref() == Some(required))
}

fn component_catalog_entry(manifest: &S9labComponentManifestV1) -> Phase5ComponentCatalogEntry {
    Phase5ComponentCatalogEntry {
        component_id: manifest.component_id.clone(),
        component_version: manifest.component_version.clone(),
        minecraft_version: manifest.minecraft_version.clone(),
        loader: manifest.loader.clone(),
        size_bytes: manifest.size_bytes,
        sha256: manifest.sha256.clone(),
    }
}

fn component_summary(selection: &S9labComponentSelection) -> Option<InstalledComponentSummary> {
    match selection {
        S9labComponentSelection::Disabled => None,
        S9labComponentSelection::Catalog {
            component_id,
            component_version,
        } => Some(InstalledComponentSummary {
            component_id: component_id.clone(),
            component_version: component_version.clone(),
        }),
    }
}

fn source_provider_id(value: &str) -> AppResult<ProviderId> {
    match value {
        "mojang" => Ok(ProviderId::Mojang),
        "fabric" => Ok(ProviderId::Fabric),
        "neoforge" => Ok(ProviderId::Neoforge),
        _ => Err(AppError::coded("runtime_source_provider_invalid")),
    }
}

fn lock_item(source: &RuntimeArtifactSource, sha256: &str) -> AppResult<ResolvedRuntimeItem> {
    let item = ResolvedRuntimeItem {
        provider_id: source_provider_id(&source.provider)?,
        logical_id: source.logical_id.clone(),
        relative_target: source.target_relative_path.clone(),
        sha256: sha256.to_string(),
        size_bytes: source.size_bytes,
        kind: match source.kind {
            RuntimeArtifactKind::Client => LockedArtifactKind::MinecraftClient,
            RuntimeArtifactKind::Library | RuntimeArtifactKind::Native => {
                LockedArtifactKind::MinecraftLibrary
            }
            RuntimeArtifactKind::AssetIndex => LockedArtifactKind::AssetIndex,
            RuntimeArtifactKind::AssetObject => LockedArtifactKind::AssetObject,
            RuntimeArtifactKind::LoggingConfig => LockedArtifactKind::LoggingConfiguration,
            RuntimeArtifactKind::LoaderLibrary => LockedArtifactKind::LoaderLibrary,
            RuntimeArtifactKind::Installer => {
                return Err(AppError::coded("runtime_installer_not_final_artifact"));
            }
        },
    };
    crate::runtime::validate_resolved_runtime_item(&item)?;
    Ok(item)
}

fn build_launch_configuration(
    base: &ResolvedMinecraftVersion,
    loader: Option<&ResolvedLoader>,
) -> AppResult<ResolvedLaunchConfiguration> {
    let mut classpath = base
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == RuntimeArtifactKind::Library)
        .map(|artifact| artifact.target_relative_path.clone())
        .collect::<Vec<_>>();
    let native_jar_targets = base
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == RuntimeArtifactKind::Native)
        .map(|artifact| artifact.target_relative_path.clone())
        .collect::<Vec<_>>();
    let mut game_arguments = convert_arguments(&base.game_arguments);
    let mut jvm_arguments = convert_arguments(&base.jvm_arguments);
    let main_class = if let Some(loader) = loader {
        classpath.extend(
            loader
                .artifacts
                .iter()
                .map(|artifact| artifact.target_relative_path.clone()),
        );
        game_arguments.extend(convert_arguments(&loader.game_arguments));
        jvm_arguments.extend(convert_arguments(&loader.jvm_arguments));
        loader.main_class.clone()
    } else {
        base.main_class.clone()
    };
    classpath.extend(
        base.artifacts
            .iter()
            .filter(|artifact| artifact.kind == RuntimeArtifactKind::Client)
            .map(|artifact| artifact.target_relative_path.clone()),
    );
    let mut seen_classpath = BTreeSet::new();
    classpath.retain(|target| seen_classpath.insert(target.clone()));
    if classpath.is_empty() {
        return Err(AppError::coded("runtime_classpath_empty"));
    }
    Ok(ResolvedLaunchConfiguration {
        main_class,
        version_type: base.release_type.clone(),
        asset_index_id: base.asset_index_id.clone(),
        java_major_version: u16::try_from(base.java_major)
            .map_err(|_| AppError::coded("runtime_java_major_unsupported"))?,
        game_arguments,
        jvm_arguments,
        classpath_targets: classpath,
        native_jar_targets,
        legacy_game_arguments: base.legacy_game_arguments.clone(),
    })
}

fn convert_arguments(arguments: &[LaunchArgument]) -> Vec<ResolvedLaunchArgument> {
    arguments
        .iter()
        .map(|argument| match argument {
            LaunchArgument::Plain(value) => ResolvedLaunchArgument::Plain {
                value: value.clone(),
            },
            LaunchArgument::Conditional { rules, value } => ResolvedLaunchArgument::Conditional {
                rules: rules.iter().map(convert_rule).collect(),
                values: match value {
                    LaunchArgumentValue::One(value) => vec![value.clone()],
                    LaunchArgumentValue::Many(values) => values.clone(),
                },
            },
        })
        .collect()
}

fn convert_rule(rule: &LaunchRule) -> ResolvedLaunchRule {
    ResolvedLaunchRule {
        action: rule.action.clone(),
        os_name: rule.os.as_ref().and_then(|os| os.name.clone()),
        os_arch: rule.os.as_ref().and_then(|os| os.arch.clone()),
        has_os_version_constraint: rule.os.as_ref().is_some_and(|os| os.version.is_some()),
        features: rule.features.clone(),
    }
}

fn validate_source_targets(sources: &[RuntimeArtifactSource]) -> AppResult<()> {
    let mut targets = BTreeSet::new();
    let mut logical_ids = BTreeSet::new();
    for source in sources {
        let normalized = crate::security::paths::normalize_relative_path(Path::new(
            &source.target_relative_path,
        ))?;
        let canonical = normalized.to_string_lossy().replace('\\', "/");
        if source.target_relative_path.contains('\\') || canonical != source.target_relative_path {
            return Err(AppError::coded("runtime_target_path_invalid"));
        }
        let target_key = crate::security::paths::collision_key(&normalized)?;
        if !targets.insert(target_key) {
            return Err(AppError::coded("runtime_artifact_target_duplicate"));
        }
        if !logical_ids.insert(source.logical_id.to_ascii_lowercase()) {
            return Err(AppError::coded("runtime_artifact_logical_id_duplicate"));
        }
    }
    Ok(())
}

fn read_bounded_document(path: &Path) -> AppResult<Vec<u8>> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_PROFILE_DOCUMENT_BYTES {
        return Err(AppError::coded("runtime_profile_document_size_invalid"));
    }
    fs::read(path).map_err(Into::into)
}

fn hash_file(path: &Path) -> AppResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        use sha2::Digest;
        hasher.update(&buffer[..count]);
    }
    use sha2::Digest;
    Ok(hex::encode(hasher.finalize()))
}

fn hash_file_sha1_sha256(path: &Path) -> AppResult<(String, String)> {
    use sha1::Digest as _;

    let mut file = fs::File::open(path)?;
    let mut sha1 = sha1::Sha1::new();
    let mut sha256 = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha1.update(&buffer[..count]);
        sha256.update(&buffer[..count]);
    }
    Ok((hex::encode(sha1.finalize()), hex::encode(sha256.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{
        JavaPolicy, LoaderSelection, COMPONENT_MANIFEST_FORMAT, COMPONENT_MANIFEST_FORMAT_VERSION,
        COMPONENT_SIGNATURE_DOMAIN,
    };
    use std::collections::BTreeMap;

    #[test]
    fn resolver_arguments_preserve_values_and_fail_closed_version_rules() {
        let arguments = vec![LaunchArgument::Conditional {
            rules: vec![LaunchRule {
                action: "allow".into(),
                os: Some(crate::minecraft::resolver::LaunchOsRule {
                    name: Some("windows".into()),
                    arch: None,
                    version: Some("^10\\.".into()),
                }),
                features: BTreeMap::new(),
            }],
            value: LaunchArgumentValue::Many(vec!["--one".into(), "two words".into()]),
        }];
        let converted = convert_arguments(&arguments);
        match &converted[0] {
            ResolvedLaunchArgument::Conditional { rules, values } => {
                assert!(rules[0].has_os_version_constraint);
                assert_eq!(values, &["--one", "two words"]);
            }
            _ => panic!("conditional argument expected"),
        }
    }

    #[test]
    fn source_target_collisions_are_case_insensitive() {
        let source = |target: &str, logical: &str| RuntimeArtifactSource {
            logical_id: logical.into(),
            provider: "mojang".into(),
            url: "https://libraries.minecraft.net/example.jar".into(),
            target_relative_path: target.into(),
            size_bytes: 1,
            sha1: Some("a".repeat(40)),
            sha256: None,
            kind: RuntimeArtifactKind::Library,
        };
        assert!(validate_source_targets(&[
            source("libraries/Example.jar", "first"),
            source("libraries/example.jar", "second"),
        ])
        .is_err());
    }

    #[test]
    fn relative_target_paths_are_canonicalized_without_rejecting_valid_posix_paths() {
        let source = |target: &str, logical: &str| RuntimeArtifactSource {
            logical_id: logical.into(),
            provider: "mojang".into(),
            url: "https://libraries.minecraft.net/example.jar".into(),
            target_relative_path: target.into(),
            size_bytes: 1,
            sha1: Some("a".repeat(40)),
            sha256: None,
            kind: RuntimeArtifactKind::Library,
        };
        assert!(validate_source_targets(&[source("libraries/example.jar", "library")]).is_ok());
        assert!(validate_source_targets(&[source("../x.jar", "library")]).is_err());
        assert!(validate_source_targets(&[source("libraries\\example.jar", "library")]).is_err());
    }

    #[test]
    fn launch_classpath_preserves_manifest_order_and_keeps_client_last() {
        let artifact = |logical_id: &str, target: &str, kind| RuntimeArtifactSource {
            logical_id: logical_id.into(),
            provider: "mojang".into(),
            url: "https://libraries.minecraft.net/example.jar".into(),
            target_relative_path: target.into(),
            size_bytes: 1,
            sha1: Some("a".repeat(40)),
            sha256: None,
            kind,
        };
        let base = ResolvedMinecraftVersion {
            minecraft_version: "1.21.1".into(),
            release_type: "release".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            java_major: 21,
            asset_index_id: "17".into(),
            artifacts: vec![
                artifact(
                    "client",
                    "versions/1.21.1/1.21.1.jar",
                    RuntimeArtifactKind::Client,
                ),
                artifact(
                    "library-b",
                    "libraries/example/b.jar",
                    RuntimeArtifactKind::Library,
                ),
                artifact(
                    "library-a",
                    "libraries/example/a.jar",
                    RuntimeArtifactKind::Library,
                ),
            ],
            game_arguments: Vec::new(),
            jvm_arguments: Vec::new(),
            legacy_game_arguments: None,
        };
        let loader = ResolvedLoader {
            loader_version: "0.16.10".into(),
            profile_id: "fabric-loader".into(),
            main_class: "net.fabricmc.loader.impl.launch.knot.KnotClient".into(),
            artifacts: vec![artifact(
                "fabric-loader",
                "libraries/net/fabricmc/fabric-loader.jar",
                RuntimeArtifactKind::LoaderLibrary,
            )],
            game_arguments: Vec::new(),
            jvm_arguments: Vec::new(),
        };
        let launch =
            build_launch_configuration(&base, Some(&loader)).expect("ordered launch configuration");
        assert_eq!(
            launch.classpath_targets,
            [
                "libraries/example/b.jar",
                "libraries/example/a.jar",
                "libraries/net/fabricmc/fabric-loader.jar",
                "versions/1.21.1/1.21.1.jar",
            ]
        );
    }

    #[test]
    fn profile_manifest_v2_excludes_account_and_lineage_fields() {
        let manifest = ProfileManifestV2 {
            format: PROFILE_MANIFEST_FORMAT.into(),
            format_version: PROFILE_FORMAT_VERSION,
            profile_id: "profile-1".into(),
            created_at_unix: 0,
            runtime: ProfileRuntimeIntent {
                minecraft_version: "1.21.1".into(),
                loader: LoaderSelection {
                    kind: LoaderKind::Vanilla,
                    loader_version: None,
                },
                java: JavaPolicy::Managed { major_version: 21 },
            },
            s9lab_component: S9labComponentSelection::Disabled,
            desired_content: Vec::new(),
            mutable_directories: Vec::new(),
            isolation_policy: "verified-copy-no-hardlinks".into(),
        };
        let value = serde_json::to_value(manifest).expect("manifest");
        assert!(value.get("accountId").is_none());
        assert!(value.get("sourceProfileId").is_none());
        assert!(value.get("displayName").is_none());
    }

    #[test]
    fn component_catalog_projection_is_compatible_and_secret_free() {
        let manifest = S9labComponentManifestV1 {
            format: COMPONENT_MANIFEST_FORMAT.into(),
            format_version: COMPONENT_MANIFEST_FORMAT_VERSION,
            signature_domain: COMPONENT_SIGNATURE_DOMAIN.into(),
            key_id: "release-key".into(),
            component_id: "s9lab_client".into(),
            component_version: "1.0.8".into(),
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
            size_bytes: 4096,
            sha256: "a".repeat(64),
            relative_target: "instance/mods/s9lab_client.jar".into(),
            signature: "not-exposed".into(),
        };
        let compatible_runtime = ProfileRuntimeIntent {
            minecraft_version: "1.21.1".into(),
            loader: manifest.loader.clone(),
            java: JavaPolicy::Managed { major_version: 21 },
        };
        let incompatible_runtime = ProfileRuntimeIntent {
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.9".into()),
            },
            java: JavaPolicy::Managed { major_version: 21 },
        };

        assert!(component_matches_runtime(&manifest, &compatible_runtime));
        assert!(!component_matches_runtime(&manifest, &incompatible_runtime));

        let value =
            serde_json::to_value(component_catalog_entry(&manifest)).expect("catalog projection");
        assert_eq!(value["componentId"], "s9lab_client");
        assert_eq!(value["componentVersion"], "1.0.8");
        assert_eq!(value["minecraftVersion"], "1.21.1");
        assert_eq!(value["loader"]["kind"], "fabric");
        assert_eq!(value["sizeBytes"], 4096);
        assert_eq!(value["sha256"], "a".repeat(64));
        for forbidden in ["url", "downloadUrl", "signature", "keyId", "relativeTarget"] {
            assert!(value.get(forbidden).is_none(), "{forbidden} leaked");
        }
    }
}
