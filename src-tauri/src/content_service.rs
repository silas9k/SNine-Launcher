use crate::{
    cache::CacheStore,
    content::{
        content_lock_sha256, resolve_content, validate_content_override_target,
        validate_local_content, validate_resolved_content_lock, ContentArtifactV1,
        ContentCompatibility, ContentDependency, ContentDependencyKind, ContentKind,
        ContentLoaderCompatibility, ContentReleaseV1, ContentResolutionRequest, ContentSelection,
        ContentSourceV1, ContentTargetRuntime, ContentVersionRequirement, ResolvedContentItemV1,
        ResolvedContentLockV1, ResolvedContentOverrideV1, ResolvedContentPackMemberV1,
        CONTENT_RELEASE_FORMAT, CONTENT_RELEASE_FORMAT_VERSION, MAX_RESOLVED_CONTENT_ITEMS,
        MAX_RESOLVED_CONTENT_OVERRIDES,
    },
    download::{DownloadService, ProviderId},
    error::{AppError, AppResult},
    foundation::CoreServices,
    minecraft::service::MinecraftRuntimeService,
    modrinth::{
        DependencyType, ModrinthDependency, ModrinthLoader, ModrinthProvider,
        ModrinthSearchRequest, ProjectDetail, ProjectType, ProjectVersion, SearchIndex,
        VersionQuery, VersionStatus,
    },
    operations::{
        engine::OperationEngine,
        model::{
            canonical_json, new_identifier, sha256_hex, CacheMaterialization, OperationType,
            ProfileInstallPlan,
        },
    },
    profile_format::{
        export_profile_v1, import_profile_v1, ProfileExportArtifactSource, ProfileExportV1,
    },
    profiles::model::{LockedCacheBlob, ProfileLockV2, ProfileManifestV2},
    profiles::service::ProfileService,
    runtime::{CapabilityStatus, LoaderKind},
    security::{
        fs as secure_fs, paths::validate_existing_chain, PathRegistry, RegisteredRoot, SecurePath,
    },
    storage::{models::ProfileRecord, Storage},
};
use chrono::Utc;
use futures_util::{stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

const PROFILE_MANIFEST_FORMAT: &str = "site.s9lab.profile";
const PROFILE_LOCK_FORMAT: &str = "site.s9lab.profile-lock";
const PROFILE_FORMAT_VERSION: u32 = 2;
const MAX_PROFILE_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_AUTOMATIC_UPDATE_CHECKS: usize = 256;
const UPDATE_CHECK_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6Dependency {
    pub project_id: String,
    pub display_name: String,
    pub relation: String,
    pub satisfied: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6Conflict {
    pub content_id: String,
    pub display_name: String,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6InstalledContentUpdate {
    pub version_id: String,
    pub version_number: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6InstalledContent {
    pub content_id: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub display_name: String,
    pub version_number: String,
    pub content_type: ContentKind,
    pub source: String,
    pub enabled: bool,
    pub managed_by_pack: bool,
    pub size_bytes: u64,
    pub sha256: String,
    pub dependencies: Vec<Phase6Dependency>,
    pub conflicts: Vec<Phase6Conflict>,
    pub update: Option<Phase6InstalledContentUpdate>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6ContentSnapshot {
    pub profile_id: String,
    pub minecraft_version: Option<String>,
    pub loader: Option<LoaderKind>,
    pub lock_sha256: Option<String>,
    pub content: Vec<Phase6InstalledContent>,
    pub local_file_capability: CapabilityStatus,
    pub profile_format_capability: CapabilityStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6SearchHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub content_type: ContentKind,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    pub icon_url: Option<String>,
    pub updated_at_unix: i64,
    pub latest_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6SearchResult {
    pub capability: CapabilityStatus,
    pub total: u64,
    pub offset: u32,
    pub hits: Vec<Phase6SearchHit>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6ProjectVersion {
    pub version_id: String,
    pub version_number: String,
    pub name: String,
    pub published_at_unix: i64,
    pub compatible: bool,
    pub incompatibility_reason: Option<String>,
    pub dependencies: Vec<Phase6Dependency>,
    pub conflicts: Vec<Phase6Conflict>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6ProjectDetail {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub content_type: ContentKind,
    pub author: String,
    pub license: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub followers: u64,
    pub updated_at_unix: i64,
    pub categories: Vec<String>,
    pub versions: Vec<Phase6ProjectVersion>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6OperationResult {
    pub operation_id: String,
    pub profile_id: String,
    pub revision_id: String,
    pub changed_content_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase6ProfileTransferResult {
    pub operation_id: String,
    pub profile_id: String,
    pub display_name: String,
    pub file_name: Option<String>,
}

#[derive(Clone)]
pub struct Phase6ContentService {
    storage: Storage,
    registry: Arc<PathRegistry>,
    operations: OperationEngine,
    downloads: DownloadService,
    cache: CacheStore,
    modrinth: ModrinthProvider,
    profiles: ProfileService,
    runtime: MinecraftRuntimeService,
}

impl Phase6ContentService {
    pub fn from_core(core: &CoreServices) -> AppResult<Self> {
        Ok(Self {
            storage: core.storage().clone(),
            registry: core.registry().clone(),
            operations: core.operations().clone(),
            downloads: core.download().clone(),
            cache: core.cache().clone(),
            modrinth: ModrinthProvider::production()?,
            profiles: ProfileService::from_core(core),
            runtime: MinecraftRuntimeService::from_core(core)?,
        })
    }

    pub fn snapshot(&self, profile_id: &str) -> AppResult<Phase6ContentSnapshot> {
        let profile = self.profile_for_read(profile_id)?;
        let documents = if self.storage.runtime_projection(&profile.id)?.is_some() {
            Some(self.read_active_documents(&profile)?)
        } else {
            None
        };
        let (minecraft_version, loader, content_lock) = documents
            .as_ref()
            .map(|(manifest, lock)| {
                (
                    Some(manifest.runtime.minecraft_version.clone()),
                    Some(manifest.runtime.loader.kind),
                    lock.content.as_ref(),
                )
            })
            .unwrap_or((None, None, None));
        let content = content_lock
            .map(|lock| {
                let pack_member_ids = lock
                    .pack_members
                    .iter()
                    .map(|member| member.content_id.as_str())
                    .collect::<BTreeSet<_>>();
                lock.items
                    .iter()
                    .map(|item| {
                        installed_content(item, pack_member_ids.contains(item.content_id.as_str()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Phase6ContentSnapshot {
            profile_id: profile.id,
            minecraft_version,
            loader,
            lock_sha256: content_lock.map(|lock| lock.resolution_sha256.clone()),
            content,
            local_file_capability: CapabilityStatus::available("content.local-file"),
            profile_format_capability: CapabilityStatus::available("content.profile-format"),
        })
    }

    pub async fn populate_snapshot_updates(
        &self,
        profile_id: &str,
        mut snapshot: Phase6ContentSnapshot,
    ) -> AppResult<Phase6ContentSnapshot> {
        let (Some(minecraft_version), Some(loader)) =
            (snapshot.minecraft_version.clone(), snapshot.loader)
        else {
            return Ok(snapshot);
        };
        let profile = self.profile_for_read(profile_id)?;
        let (_, lock) = self.read_active_documents(&profile)?;
        let Some(content_lock) = lock.content.as_ref() else {
            return Ok(snapshot);
        };
        if snapshot.lock_sha256.as_deref() != Some(content_lock.resolution_sha256.as_str()) {
            // A concurrent revision won the race. Returning the already valid
            // base snapshot is safer than attaching update metadata to it.
            return Ok(snapshot);
        }
        let pack_member_ids = content_lock
            .pack_members
            .iter()
            .map(|member| member.content_id.as_str())
            .collect::<BTreeSet<_>>();
        let candidates = content_lock
            .items
            .iter()
            .filter(|item| !pack_member_ids.contains(item.content_id.as_str()))
            .filter_map(|item| match item.source.as_ref() {
                Some(ContentSourceV1::Modrinth {
                    project_id,
                    version_id,
                    ..
                }) => Some((
                    item.content_id.clone(),
                    project_id.clone(),
                    version_id.clone(),
                    item.kind,
                )),
                _ => None,
            })
            .take(MAX_AUTOMATIC_UPDATE_CHECKS)
            .collect::<Vec<_>>();
        let checks = stream::iter(candidates)
            .map(|(content_id, project_id, version_id, kind)| {
                let minecraft_version = minecraft_version.clone();
                async move {
                    let update = self
                        .latest_compatible_update(
                            &project_id,
                            &version_id,
                            kind,
                            &minecraft_version,
                            loader,
                        )
                        .await
                        .ok()
                        .flatten();
                    (content_id, update)
                }
            })
            .buffer_unordered(UPDATE_CHECK_CONCURRENCY)
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        for item in &mut snapshot.content {
            if let Some(update) = checks.get(&item.content_id) {
                item.update.clone_from(update);
            }
        }
        Ok(snapshot)
    }

    pub async fn search(
        &self,
        query: String,
        content_type: ContentKind,
        minecraft_version: String,
        loader: LoaderKind,
        offset: u32,
        limit: u8,
    ) -> AppResult<Phase6SearchResult> {
        let request = ModrinthSearchRequest {
            query,
            project_type: project_type(content_type),
            loader: Some(modrinth_loader(loader)),
            minecraft_version: Some(minecraft_version),
            index: SearchIndex::Relevance,
            offset,
            limit,
        };
        let page = self.modrinth.search_projects(&request).await?;
        Ok(Phase6SearchResult {
            capability: CapabilityStatus::available("content.modrinth"),
            total: page.total_hits,
            offset: page.offset,
            hits: page
                .hits
                .into_iter()
                .map(|hit| Phase6SearchHit {
                    project_id: hit.project_id,
                    slug: hit.slug,
                    title: hit.title,
                    description: hit.description,
                    content_type: content_kind(hit.project_type),
                    author: hit.author,
                    downloads: hit.downloads,
                    follows: hit.follows,
                    icon_url: hit.icon_url,
                    updated_at_unix: hit.updated_at.timestamp(),
                    latest_version: None,
                })
                .collect(),
        })
    }

    pub async fn project(
        &self,
        profile_id: &str,
        project_id: &str,
    ) -> AppResult<Phase6ProjectDetail> {
        let profile = self.profile_for_read(profile_id)?;
        let (manifest, _) = self.read_active_documents(&profile)?;
        let detail = self.modrinth.project_detail(project_id).await?;
        let kind = content_kind(detail.project_type);
        let versions = self
            .compatible_versions(project_id, kind, &manifest)
            .await?;
        Ok(project_detail_dto(detail, versions))
    }

    pub async fn install_modrinth(
        &self,
        profile_id: &str,
        project_id: &str,
        version_id: Option<&str>,
    ) -> AppResult<Phase6OperationResult> {
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, lock) = self.read_active_documents(&profile)?;
        let detail = self.modrinth.project_detail(project_id).await?;
        let kind = content_kind(detail.project_type);
        let version = match version_id {
            Some(version_id) => {
                let version = self.modrinth.version_detail(version_id).await?;
                if version.project_id != project_id {
                    return Err(AppError::coded("modrinth_version_project_mismatch"));
                }
                version
            }
            None => self
                .compatible_versions(project_id, kind, &manifest)
                .await?
                .into_iter()
                .find(|version| version.status == VersionStatus::Listed)
                .ok_or_else(|| AppError::coded("content_compatible_version_missing"))?,
        };
        ensure_version_compatible(&version, kind, &manifest)?;
        if kind == ContentKind::Modpack {
            return self
                .install_modrinth_pack_version(&profile, manifest, lock, version)
                .await;
        }
        let current = lock.content.as_ref();
        if current.is_some_and(|content| {
            content.items.iter().any(|item| {
                matches!(
                    item.source.as_ref(),
                    Some(ContentSourceV1::Modrinth { version_id, .. })
                        if version_id == &version.version_id
                )
            })
        }) {
            return Err(AppError::coded("content_version_already_installed"));
        }
        if current.is_some_and(|content| {
            content.pack_members.iter().any(|member| {
                member.content_id == project_id && member.version != version.version_id
            })
        }) {
            return Err(AppError::coded_with(
                "content_pack_member_version_conflict",
                [("contentId", project_id)],
            ));
        }
        let stage_id = new_identifier("content-download");
        let graph = match self
            .download_modrinth_graph(&stage_id, version.clone(), kind, &manifest)
            .await
        {
            Ok(graph) => graph,
            Err(error) => {
                let _ = self.operations.cleanup_staging(&stage_id);
                return Err(error);
            }
        };
        let prepared = (|| -> AppResult<ResolvedContentLockV1> {
            let mut requested = current
                .map(|content| content.requested.clone())
                .unwrap_or_default();
            requested.retain(|selection| selection.content_id != project_id);
            requested.push(ContentSelection {
                content_id: project_id.to_string(),
                version: ContentVersionRequirement::Exact {
                    version: version.version_id.clone(),
                },
                enabled: true,
            });
            let next = resolve_next_content(&manifest, current, requested, graph.releases, true)?;
            for activation in graph.activations {
                self.cache.activate_verified_copy(
                    &activation.staging_relative,
                    &activation.sha256,
                    activation.size_bytes,
                )?;
            }
            Ok(next)
        })();
        let cleanup = self.operations.cleanup_staging(&stage_id);
        let next = match (prepared, cleanup) {
            (Ok(next), _) => next,
            (Err(primary), Ok(())) => return Err(primary),
            (Err(primary), Err(cleanup)) => {
                return Err(AppError::coded_with(
                    "content_download_cleanup_failed",
                    [
                        ("primary", primary.descriptor().code),
                        ("cleanup", cleanup.descriptor().code),
                    ],
                ));
            }
        };
        let changed_content_ids = changed_content_ids(current, &next);
        self.commit_content_revision(
            &profile,
            manifest,
            lock,
            Some(next),
            OperationType::ContentInstall,
            changed_content_ids,
        )
    }

    async fn install_modrinth_pack_version(
        &self,
        profile: &ProfileRecord,
        manifest: ProfileManifestV2,
        lock: ProfileLockV2,
        version: ProjectVersion,
    ) -> AppResult<Phase6OperationResult> {
        if lock.content.as_ref().is_some_and(|content| {
            content.items.iter().any(|item| {
                matches!(
                    item.source.as_ref(),
                    Some(ContentSourceV1::Modrinth { version_id, .. })
                        if version_id == &version.version_id
                )
            })
        }) {
            return Err(AppError::coded("content_version_already_installed"));
        }

        let file_name = version
            .primary_file()
            .or_else(|| (version.files.len() == 1).then(|| &version.files[0]))
            .ok_or_else(|| AppError::coded("modrinth_primary_file_missing"))?
            .file_name
            .clone();
        if !file_name.to_ascii_lowercase().ends_with(".mrpack") {
            return Err(AppError::coded("content_modpack_extension_invalid"));
        }

        let stage_id = new_identifier("mrpack-download");
        let result = async {
            let (pack_release, activation) = self
                .download_modrinth_release(
                    &stage_id,
                    version,
                    ContentKind::Modpack,
                    &manifest,
                    Vec::new(),
                )
                .await?;
            let staging_relative = activation.staging_relative;
            let staged_source = self
                .registry
                .resolve("staging-operations", &staging_relative)?;
            self.import_staged_modrinth_pack(
                &stage_id,
                profile,
                manifest,
                lock,
                &staging_relative,
                &staged_source,
                pack_release,
            )
            .await
        }
        .await;
        self.finish_modpack_staging(&stage_id, result)
    }

    pub async fn update(
        &self,
        profile_id: &str,
        content_id: &str,
    ) -> AppResult<Phase6OperationResult> {
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, lock) = self.read_active_documents(&profile)?;
        let content = lock
            .content
            .as_ref()
            .ok_or_else(|| AppError::coded("content_not_installed"))?;
        if content
            .pack_members
            .iter()
            .any(|member| member.content_id == content_id)
        {
            return Err(AppError::coded("content_pack_member_update_unavailable"));
        }
        let item = content
            .items
            .iter()
            .find(|item| item.content_id == content_id)
            .ok_or_else(|| AppError::coded("content_not_installed"))?;
        let (project_id, current_version_id) = match item.source.as_ref() {
            Some(ContentSourceV1::Modrinth {
                project_id,
                version_id,
                ..
            }) => (project_id.clone(), version_id.clone()),
            _ => return Err(AppError::coded("content_local_update_unavailable")),
        };
        let update = self
            .latest_compatible_update(
                &project_id,
                &current_version_id,
                item.kind,
                &manifest.runtime.minecraft_version,
                manifest.runtime.loader.kind,
            )
            .await?
            .ok_or_else(|| AppError::coded("content_update_unavailable"))?;
        self.install_modrinth(profile_id, &project_id, Some(&update.version_id))
            .await
    }

    async fn latest_compatible_update(
        &self,
        project_id: &str,
        current_version_id: &str,
        kind: ContentKind,
        minecraft_version: &str,
        loader: LoaderKind,
    ) -> AppResult<Option<Phase6InstalledContentUpdate>> {
        let current = self.modrinth.version_detail(current_version_id).await?;
        if current.project_id != project_id {
            return Err(AppError::coded("modrinth_version_project_mismatch"));
        }
        let versions = self
            .modrinth
            .project_versions(
                project_id,
                &VersionQuery {
                    loader: matches!(kind, ContentKind::Mod | ContentKind::Modpack)
                        .then_some(modrinth_loader(loader)),
                    minecraft_version: Some(minecraft_version.to_string()),
                    featured: None,
                },
            )
            .await?;
        Ok(versions
            .into_iter()
            .filter(|candidate| {
                candidate.status == VersionStatus::Listed
                    && candidate.version_id != current.version_id
                    && candidate.published_at > current.published_at
            })
            .max_by(|left, right| {
                left.published_at
                    .cmp(&right.published_at)
                    .then_with(|| left.version_id.cmp(&right.version_id))
            })
            .map(|version| Phase6InstalledContentUpdate {
                version_id: version.version_id,
                version_number: version.version_number,
            }))
    }

    async fn compatible_versions(
        &self,
        project_id: &str,
        kind: ContentKind,
        manifest: &ProfileManifestV2,
    ) -> AppResult<Vec<ProjectVersion>> {
        self.modrinth
            .project_versions(
                project_id,
                &VersionQuery {
                    loader: matches!(kind, ContentKind::Mod | ContentKind::Modpack)
                        .then_some(modrinth_loader(manifest.runtime.loader.kind)),
                    minecraft_version: Some(manifest.runtime.minecraft_version.clone()),
                    featured: None,
                },
            )
            .await
    }

    async fn download_modrinth_graph(
        &self,
        stage_id: &str,
        root: ProjectVersion,
        root_kind: ContentKind,
        manifest: &ProfileManifestV2,
    ) -> AppResult<DownloadedContentGraph> {
        let mut queue = VecDeque::from([(root, root_kind)]);
        let mut scheduled = BTreeSet::<(String, String)>::new();
        let mut releases = Vec::new();
        let mut activations = Vec::new();
        while let Some((version, kind)) = queue.pop_front() {
            let identity = (version.project_id.clone(), version.version_id.clone());
            if !scheduled.insert(identity) {
                continue;
            }
            if scheduled.len() > 512 {
                return Err(AppError::coded("content_dependency_graph_too_large"));
            }
            ensure_version_compatible(&version, kind, manifest)?;
            let mut dependencies = Vec::new();
            for dependency in version.dependencies.clone() {
                if dependency.dependency_type == DependencyType::Embedded {
                    continue;
                }
                let resolved = self
                    .resolve_modrinth_dependency(&dependency, manifest)
                    .await?;
                let Some((project_id, dependency_version, dependency_kind)) = resolved else {
                    if dependency.dependency_type == DependencyType::Required {
                        return Err(AppError::coded("content_dependency_missing"));
                    }
                    continue;
                };
                let requirement = dependency_version
                    .as_ref()
                    .map(|value| ContentVersionRequirement::Exact {
                        version: value.version_id.clone(),
                    })
                    .unwrap_or(ContentVersionRequirement::Any);
                dependencies.push(ContentDependency {
                    content_id: project_id,
                    kind: dependency_kind_from_modrinth(dependency.dependency_type),
                    version: requirement,
                });
                if matches!(
                    dependency.dependency_type,
                    DependencyType::Required | DependencyType::Optional
                ) {
                    if let Some(dependency_version) = dependency_version {
                        if dependency.dependency_type == DependencyType::Required {
                            queue.push_back((dependency_version, dependency_kind));
                        }
                    }
                }
            }
            let (release, activation) = self
                .download_modrinth_release(stage_id, version, kind, manifest, dependencies)
                .await?;
            releases.push(release);
            activations.push(activation);
        }
        Ok(DownloadedContentGraph {
            releases,
            activations,
        })
    }

    async fn resolve_modrinth_dependency(
        &self,
        dependency: &ModrinthDependency,
        manifest: &ProfileManifestV2,
    ) -> AppResult<Option<(String, Option<ProjectVersion>, ContentKind)>> {
        if let Some(version_id) = dependency.version_id.as_deref() {
            let version = self.modrinth.version_detail(version_id).await?;
            if dependency
                .project_id
                .as_ref()
                .is_some_and(|project_id| project_id != &version.project_id)
            {
                return Err(AppError::coded("modrinth_dependency_identity_mismatch"));
            }
            let detail = self.modrinth.project_detail(&version.project_id).await?;
            let kind = content_kind(detail.project_type);
            if dependency.dependency_type != DependencyType::Incompatible {
                ensure_version_compatible(&version, kind, manifest)?;
            }
            return Ok(Some((version.project_id.clone(), Some(version), kind)));
        }
        let Some(project_id) = dependency.project_id.as_deref() else {
            return Ok(None);
        };
        let detail = self.modrinth.project_detail(project_id).await?;
        let kind = content_kind(detail.project_type);
        if dependency.dependency_type == DependencyType::Incompatible {
            return Ok(Some((project_id.to_string(), None, kind)));
        }
        let version = self
            .compatible_versions(project_id, kind, manifest)
            .await?
            .into_iter()
            .find(|version| version.status == VersionStatus::Listed);
        Ok(version.map(|version| (project_id.to_string(), Some(version), kind)))
    }

    async fn dependencies_for_pinned_version(
        &self,
        version: &ProjectVersion,
    ) -> AppResult<Vec<ContentDependency>> {
        let mut dependencies = Vec::new();
        for dependency in &version.dependencies {
            if dependency.dependency_type == DependencyType::Embedded {
                continue;
            }
            let project_id = if let Some(project_id) = dependency.project_id.as_ref() {
                project_id.clone()
            } else if let Some(version_id) = dependency.version_id.as_deref() {
                self.modrinth.version_detail(version_id).await?.project_id
            } else if dependency.dependency_type == DependencyType::Required {
                return Err(AppError::coded("content_dependency_missing"));
            } else {
                continue;
            };
            dependencies.push(ContentDependency {
                content_id: project_id,
                kind: dependency_kind_from_modrinth(dependency.dependency_type),
                version: dependency.version_id.as_ref().map_or(
                    ContentVersionRequirement::Any,
                    |version| ContentVersionRequirement::Exact {
                        version: version.clone(),
                    },
                ),
            });
        }
        dependencies.sort_by(|left, right| {
            left.content_id
                .cmp(&right.content_id)
                .then_with(|| left.kind.cmp(&right.kind))
        });
        dependencies.dedup();
        Ok(dependencies)
    }

    fn extract_mrpack_overrides(
        &self,
        stage_id: &str,
        archive_path: &Path,
        pack_content_id: &str,
        inventory: &[MrpackOverrideEntry],
    ) -> AppResult<Vec<ExtractedMrpackOverride>> {
        let file = fs::File::open(archive_path)?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|_| AppError::coded("content_modpack_invalid"))?;
        let mut overrides = Vec::with_capacity(inventory.len());
        for (output_index, inventory_entry) in inventory.iter().enumerate() {
            let mut entry = archive
                .by_index(inventory_entry.archive_index)
                .map_err(|_| AppError::coded("content_modpack_invalid"))?;
            if !entry.is_file() || entry.size() != inventory_entry.size_bytes {
                return Err(AppError::coded("content_override_entry_changed"));
            }
            let extracted_target = entry
                .name()
                .strip_prefix("overrides/")
                .or_else(|| entry.name().strip_prefix("client-overrides/"))
                .ok_or_else(|| AppError::coded("content_override_entry_changed"))?;
            if extracted_target != inventory_entry.relative_target {
                return Err(AppError::coded("content_override_entry_changed"));
            }

            let staging_relative = format!("{stage_id}/overrides/{output_index:08}.bin");
            let staging = self
                .registry
                .resolve("staging-operations", &staging_relative)?;
            let mut output = secure_fs::open_new_file(&staging)?;
            let mut hasher = sha2::Sha256::new();
            let mut written = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = entry.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                written = written
                    .checked_add(read as u64)
                    .ok_or_else(|| AppError::coded("content_override_size_overflow"))?;
                if written > inventory_entry.size_bytes {
                    return Err(AppError::coded("content_override_entry_changed"));
                }
                output.write_all(&buffer[..read])?;
                use sha2::Digest as _;
                hasher.update(&buffer[..read]);
            }
            output.sync_all()?;
            validate_existing_chain(staging.anchor(), staging.absolute())?;
            if written != inventory_entry.size_bytes
                || fs::metadata(staging.absolute())?.len() != inventory_entry.size_bytes
            {
                return Err(AppError::coded("content_override_entry_changed"));
            }
            use sha2::Digest as _;
            let sha256 = hex::encode(hasher.finalize());
            overrides.push(ExtractedMrpackOverride {
                staging_relative,
                resolved: ResolvedContentOverrideV1 {
                    pack_content_id: pack_content_id.to_string(),
                    relative_target: inventory_entry.relative_target.clone(),
                    sha256,
                    size_bytes: inventory_entry.size_bytes,
                },
            });
        }
        Ok(overrides)
    }

    async fn download_modrinth_release(
        &self,
        stage_id: &str,
        version: ProjectVersion,
        kind: ContentKind,
        manifest: &ProfileManifestV2,
        dependencies: Vec<ContentDependency>,
    ) -> AppResult<(ContentReleaseV1, PendingCacheActivation)> {
        let file = version
            .primary_file()
            .cloned()
            .or_else(|| (version.files.len() == 1).then(|| version.files[0].clone()))
            .ok_or_else(|| AppError::coded("modrinth_primary_file_missing"))?;
        let staging_relative = format!(
            "{stage_id}/downloads/{}/{}/{}",
            version.project_id, version.version_id, file.file_name
        );
        let request = self.downloads.resolve_upstream_sha512(
            ProviderId::Modrinth,
            file.validated_download_url().as_str(),
            &staging_relative,
            file.size_bytes,
            &file.upstream_sha512,
        )?;
        let result = self
            .downloads
            .download(&request, &crate::download::CancellationToken::default())
            .await?;
        let release = ContentReleaseV1 {
            format: CONTENT_RELEASE_FORMAT.into(),
            format_version: CONTENT_RELEASE_FORMAT_VERSION,
            content_id: version.project_id.clone(),
            version: version.version_id.clone(),
            kind,
            compatibility: compatibility_from_version(kind, &version, manifest),
            dependencies,
            source: Some(ContentSourceV1::Modrinth {
                project_id: version.project_id,
                version_id: version.version_id,
                file_name: file.file_name.clone(),
            }),
            artifact: ContentArtifactV1 {
                relative_target: content_target(kind, &file.file_name)?,
                sha256: result.sha256.clone(),
                size_bytes: result.size_bytes,
            },
        };
        let staging = self
            .registry
            .resolve("staging-operations", &staging_relative)?;
        validate_local_content(&staging, &release)?;
        let activation = PendingCacheActivation {
            staging_relative: result.target_relative_path,
            sha256: result.sha256,
            size_bytes: result.size_bytes,
        };
        Ok((release, activation))
    }

    pub fn set_enabled(
        &self,
        profile_id: &str,
        content_id: &str,
        enabled: bool,
    ) -> AppResult<Phase6OperationResult> {
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, lock) = self.read_active_documents(&profile)?;
        let current = lock
            .content
            .as_ref()
            .ok_or_else(|| AppError::coded("content_not_installed"))?;
        let item = current
            .items
            .iter()
            .find(|item| item.content_id == content_id)
            .ok_or_else(|| AppError::coded("content_not_installed"))?;
        if !enabled && required_by_enabled_item(current, content_id) {
            return Err(AppError::coded_with(
                "content_required_dependency_cannot_disable",
                [("contentId", content_id)],
            ));
        }
        let mut next_members = current.pack_members.clone();
        if item.kind != ContentKind::Modpack
            && next_members
                .iter()
                .any(|member| member.content_id == content_id)
        {
            if !enabled {
                let pack_enabled = current
                    .requested
                    .iter()
                    .map(|selection| (selection.content_id.as_str(), selection.enabled))
                    .collect::<BTreeMap<_, _>>();
                if next_members.iter().any(|member| {
                    member.content_id == content_id
                        && member.enabled_by_default
                        && pack_enabled
                            .get(member.pack_content_id.as_str())
                            .copied()
                            .unwrap_or(false)
                }) {
                    return Err(AppError::coded_with(
                        "content_pack_member_required_by_enabled_pack",
                        [("contentId", content_id)],
                    ));
                }
            }
            // A direct user action claims this selection as manual intent.
            // That preserves its state if every contributing pack is removed.
            for member in next_members
                .iter_mut()
                .filter(|member| member.content_id == content_id)
            {
                member.owns_selection = false;
            }
        }
        let mut requested = current.requested.clone();
        match requested
            .iter_mut()
            .find(|selection| selection.content_id == content_id)
        {
            Some(selection) => selection.enabled = enabled,
            None => requested.push(ContentSelection {
                content_id: content_id.to_string(),
                version: ContentVersionRequirement::Exact {
                    version: item.version.clone(),
                },
                enabled,
            }),
        }
        let requested = reconcile_pack_member_selections(requested, &next_members)?;
        let mut next = resolve_next_content(&manifest, Some(current), requested, Vec::new(), true)?;
        next.pack_members = next_members;
        next.resolution_sha256 = content_lock_sha256(&next)?;
        validate_resolved_content_lock(&next)?;
        let changed = changed_content_ids(Some(current), &next);
        self.commit_content_revision(
            &profile,
            manifest,
            lock,
            Some(next),
            OperationType::ContentChange,
            changed,
        )
    }

    pub fn remove(&self, profile_id: &str, content_id: &str) -> AppResult<Phase6OperationResult> {
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, lock) = self.read_active_documents(&profile)?;
        let current = lock
            .content
            .as_ref()
            .ok_or_else(|| AppError::coded("content_not_installed"))?;
        let item = current
            .items
            .iter()
            .find(|item| item.content_id == content_id)
            .ok_or_else(|| AppError::coded("content_not_installed"))?;
        if required_by_enabled_item(current, content_id) {
            return Err(AppError::coded_with(
                "content_required_dependency_cannot_remove",
                [("contentId", content_id)],
            ));
        }
        if item.kind != ContentKind::Modpack
            && current
                .pack_members
                .iter()
                .any(|member| member.content_id == content_id)
        {
            return Err(AppError::coded_with(
                "content_pack_member_cannot_remove",
                [("contentId", content_id)],
            ));
        }
        let mut removals = BTreeSet::from([content_id.to_string()]);
        let mut next_members = current.pack_members.clone();
        if item.kind == ContentKind::Modpack {
            let mut removed_members = Vec::new();
            next_members.retain(|member| {
                if member.pack_content_id == content_id {
                    removed_members.push(member.clone());
                    false
                } else {
                    true
                }
            });
            for removed in removed_members {
                if removed.owns_selection {
                    if let Some(successor) = next_members
                        .iter_mut()
                        .find(|member| member.content_id == removed.content_id)
                    {
                        successor.owns_selection = true;
                    } else {
                        removals.insert(removed.content_id);
                    }
                }
            }
        }
        let requested = current
            .requested
            .iter()
            .filter(|selection| !removals.contains(&selection.content_id))
            .cloned()
            .collect::<Vec<_>>();
        let requested = reconcile_pack_member_selections(requested, &next_members)?;
        let mut next = if requested.is_empty() {
            None
        } else {
            Some(resolve_next_content(
                &manifest,
                Some(current),
                requested,
                Vec::new(),
                true,
            )?)
        };
        if let Some(next) = next.as_mut() {
            let item_ids = next
                .items
                .iter()
                .map(|item| item.content_id.as_str())
                .collect::<BTreeSet<_>>();
            next.pack_members = next_members
                .into_iter()
                .filter(|member| {
                    item_ids.contains(member.pack_content_id.as_str())
                        && item_ids.contains(member.content_id.as_str())
                })
                .collect();
            next.resolution_sha256 = content_lock_sha256(next)?;
            validate_resolved_content_lock(next)?;
        }
        let mut changed = removals.into_iter().collect::<BTreeSet<_>>();
        if let Some(next) = next.as_ref() {
            changed.extend(changed_content_ids(Some(current), next));
        }
        self.commit_content_revision(
            &profile,
            manifest,
            lock,
            next,
            OperationType::ContentChange,
            changed.into_iter().collect(),
        )
    }

    pub fn add_local_file(
        &self,
        profile_id: &str,
        source_path: &str,
        content_type: ContentKind,
    ) -> AppResult<Phase6OperationResult> {
        if content_type == ContentKind::Modpack {
            return Err(AppError::coded("content_modpack_import_required"));
        }
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, lock) = self.read_active_documents(&profile)?;
        let source = secure_external_file(source_path)?;
        let file_name = source
            .absolute()
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::coded("content_local_file_name_invalid"))?
            .to_string();
        let operation_id = new_identifier("op");
        let current = lock.content.as_ref();
        let staged = (|| -> AppResult<(String, ResolvedContentLockV1)> {
            let staging_relative = format!("{operation_id}/local/{file_name}");
            let staging = self
                .registry
                .resolve("staging-operations", &staging_relative)?;
            let copied = copy_external_to_staging_bounded(&source, &staging, 1_073_741_824)?;
            let sha256 = hash_file(staging.absolute())?;
            let content_id = format!("local-{}", &sha256[..32]);
            let version = format!("sha256-{}", &sha256[..16]);
            let release = ContentReleaseV1 {
                format: CONTENT_RELEASE_FORMAT.into(),
                format_version: CONTENT_RELEASE_FORMAT_VERSION,
                content_id: content_id.clone(),
                version: version.clone(),
                kind: content_type,
                compatibility: compatibility_for_runtime(content_type, &manifest),
                dependencies: Vec::new(),
                source: Some(ContentSourceV1::Local {
                    file_name: file_name.clone(),
                }),
                artifact: ContentArtifactV1 {
                    relative_target: content_target(content_type, &file_name)?,
                    sha256: sha256.clone(),
                    size_bytes: copied,
                },
            };
            validate_local_content(&staging, &release)?;
            let mut requested = current
                .map(|content| content.requested.clone())
                .unwrap_or_default();
            requested.retain(|selection| selection.content_id != content_id);
            requested.push(ContentSelection {
                content_id: content_id.clone(),
                version: ContentVersionRequirement::Exact { version },
                enabled: true,
            });
            let next = resolve_next_content(&manifest, current, requested, vec![release], true)?;
            self.cache
                .activate_verified_copy(&staging_relative, &sha256, copied)?;
            Ok((content_id, next))
        })();
        let cleanup = self.operations.cleanup_staging(&operation_id);
        let (content_id, next) = match (staged, cleanup) {
            (Ok(value), Ok(())) => value,
            (Ok(_), Err(cleanup)) => return Err(cleanup),
            (Err(primary), Ok(())) => return Err(primary),
            (Err(primary), Err(cleanup)) => {
                return Err(AppError::coded_with(
                    "content_local_cleanup_failed",
                    [
                        ("primary", primary.descriptor().code),
                        ("cleanup", cleanup.descriptor().code),
                    ],
                ));
            }
        };
        self.commit_content_revision(
            &profile,
            manifest,
            lock,
            Some(next),
            OperationType::ContentImport,
            vec![content_id],
        )
    }

    pub async fn import_modrinth_pack(
        &self,
        profile_id: &str,
        source_path: &str,
    ) -> AppResult<Phase6OperationResult> {
        let profile = self.profile_for_mutation(profile_id)?;
        let (manifest, lock) = self.read_active_documents(&profile)?;
        let source = secure_external_file(source_path)?;
        let file_name = source
            .absolute()
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::coded("content_local_file_name_invalid"))?
            .to_string();
        if !file_name.to_ascii_lowercase().ends_with(".mrpack") {
            return Err(AppError::coded("content_modpack_extension_invalid"));
        }

        let stage_id = new_identifier("mrpack-import");
        let result = self
            .import_modrinth_pack_inner(&stage_id, &profile, manifest, lock, &source, &file_name)
            .await;
        self.finish_modpack_staging(&stage_id, result)
    }

    async fn import_modrinth_pack_inner(
        &self,
        stage_id: &str,
        profile: &ProfileRecord,
        manifest: ProfileManifestV2,
        lock: ProfileLockV2,
        source: &SecurePath,
        file_name: &str,
    ) -> AppResult<Phase6OperationResult> {
        let source_relative = format!("{stage_id}/source/{file_name}");
        let staged_source = self
            .registry
            .resolve("staging-operations", &source_relative)?;
        let size_bytes = copy_external_to_staging_bounded(source, &staged_source, 1_073_741_824)?;
        let sha256 = hash_file(staged_source.absolute())?;
        let pack_content_id = format!("mrpack-{}", &sha256[..32]);
        let pack_version = format!("sha256-{}", &sha256[..16]);
        let pack_release = ContentReleaseV1 {
            format: CONTENT_RELEASE_FORMAT.into(),
            format_version: CONTENT_RELEASE_FORMAT_VERSION,
            content_id: pack_content_id.clone(),
            version: pack_version.clone(),
            kind: ContentKind::Modpack,
            compatibility: compatibility_for_runtime(ContentKind::Modpack, &manifest),
            dependencies: Vec::new(),
            source: Some(ContentSourceV1::Local {
                file_name: file_name.to_string(),
            }),
            artifact: ContentArtifactV1 {
                relative_target: content_target(ContentKind::Modpack, file_name)?,
                sha256: sha256.clone(),
                size_bytes,
            },
        };
        self.import_staged_modrinth_pack(
            stage_id,
            profile,
            manifest,
            lock,
            &source_relative,
            &staged_source,
            pack_release,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn import_staged_modrinth_pack(
        &self,
        stage_id: &str,
        profile: &ProfileRecord,
        manifest: ProfileManifestV2,
        lock: ProfileLockV2,
        source_relative: &str,
        staged_source: &SecurePath,
        pack_release: ContentReleaseV1,
    ) -> AppResult<Phase6OperationResult> {
        let pack_content_id = pack_release.content_id.clone();
        let pack_version = pack_release.version.clone();
        let pack_sha256 = pack_release.artifact.sha256.clone();
        let pack_size_bytes = pack_release.artifact.size_bytes;
        validate_local_content(staged_source, &pack_release)?;
        let current = lock.content.as_ref();
        if matches!(pack_release.source, Some(ContentSourceV1::Local { .. }))
            && current.is_some_and(|content| {
                content
                    .items
                    .iter()
                    .any(|item| item.content_id == pack_content_id)
            })
        {
            return Err(AppError::coded("content_version_already_installed"));
        }
        let pack_source = pack_release.source.clone();

        // Run the complete structural and lifecycle preflight before adding
        // the pack blob to the immutable cache. The same inspection is
        // repeated against the verified cache object below so every later
        // extraction is bound to the content-addressed bytes.
        let (preflight_files, preflight_overrides) = inspect_mrpack(
            staged_source.absolute(),
            &manifest,
            pack_release.artifact.size_bytes,
        )?;
        let preflight_members = mrpack_member_specs(&preflight_files);
        let preflight_transition = prepare_pack_transition(
            current,
            &pack_content_id,
            pack_source.as_ref(),
            &preflight_members,
        )?;
        validate_mrpack_profile_budget(
            &self.registry,
            &profile.id,
            current,
            &preflight_transition,
            &pack_release,
            &preflight_files,
            &preflight_overrides,
        )?;
        validate_existing_chain(staged_source.anchor(), staged_source.absolute())?;
        if fs::metadata(staged_source.absolute())?.len() != pack_release.artifact.size_bytes
            || hash_file(staged_source.absolute())? != pack_release.artifact.sha256
        {
            return Err(AppError::coded("content_modpack_changed_during_preflight"));
        }
        let (files, override_inventory) = inspect_mrpack(
            staged_source.absolute(),
            &manifest,
            pack_release.artifact.size_bytes,
        )?;
        let extracted_overrides = self.extract_mrpack_overrides(
            stage_id,
            staged_source.absolute(),
            &pack_content_id,
            &override_inventory,
        )?;
        validate_existing_chain(staged_source.anchor(), staged_source.absolute())?;
        if fs::metadata(staged_source.absolute())?.len() != pack_release.artifact.size_bytes
            || hash_file(staged_source.absolute())? != pack_release.artifact.sha256
        {
            return Err(AppError::coded("content_modpack_changed_during_import"));
        }
        let member_specs = mrpack_member_specs(&files);
        let transition = prepare_pack_transition(
            current,
            &pack_content_id,
            pack_source.as_ref(),
            &member_specs,
        )?;
        validate_mrpack_profile_budget(
            &self.registry,
            &profile.id,
            current,
            &transition,
            &pack_release,
            &files,
            &override_inventory,
        )?;

        let mut releases = Vec::with_capacity(files.len() + 1);
        let mut requested_from_pack = Vec::with_capacity(files.len() + 1);
        let mut pending_cache_activations = Vec::with_capacity(files.len());
        requested_from_pack.push(ContentSelection {
            content_id: pack_content_id.clone(),
            version: ContentVersionRequirement::Exact {
                version: pack_version.clone(),
            },
            enabled: true,
        });
        releases.push(pack_release);
        for (index, file) in files.into_iter().enumerate() {
            let pinned_version = self.modrinth.version_detail(&file.version_id).await?;
            verify_mrpack_file_version(&file, &pinned_version)?;
            ensure_version_compatible(&pinned_version, file.kind, &manifest)?;
            let dependencies = self
                .dependencies_for_pinned_version(&pinned_version)
                .await?;
            let staging_relative = format!("{stage_id}/downloads/{index:04}-{}", file.file_name);
            let request = self.downloads.resolve_upstream_sha512(
                ProviderId::Modrinth,
                &file.download,
                &staging_relative,
                file.size_bytes,
                &file.sha512,
            )?;
            let result = match self
                .downloads
                .download(&request, &crate::download::CancellationToken::default())
                .await
            {
                Ok(result) => result,
                Err(error) => return Err(error),
            };
            let release = ContentReleaseV1 {
                format: CONTENT_RELEASE_FORMAT.into(),
                format_version: CONTENT_RELEASE_FORMAT_VERSION,
                content_id: file.project_id.clone(),
                version: file.version_id.clone(),
                kind: file.kind,
                compatibility: compatibility_from_version(file.kind, &pinned_version, &manifest),
                dependencies,
                source: Some(ContentSourceV1::Modrinth {
                    project_id: file.project_id.clone(),
                    version_id: file.version_id.clone(),
                    file_name: file.file_name.clone(),
                }),
                artifact: ContentArtifactV1 {
                    relative_target: file.relative_target,
                    sha256: result.sha256.clone(),
                    size_bytes: result.size_bytes,
                },
            };
            let staging = self
                .registry
                .resolve("staging-operations", &staging_relative)?;
            validate_local_content(&staging, &release)?;
            pending_cache_activations.push(PendingCacheActivation {
                staging_relative: result.target_relative_path,
                sha256: result.sha256,
                size_bytes: result.size_bytes,
            });
            requested_from_pack.push(ContentSelection {
                content_id: file.project_id.clone(),
                version: ContentVersionRequirement::Exact {
                    version: file.version_id,
                },
                enabled: file.enabled,
            });
            releases.push(release);
        }
        let mut requested = current
            .map(|content| {
                content
                    .requested
                    .iter()
                    .filter(|selection| {
                        !transition
                            .selection_removals
                            .contains(&selection.content_id)
                    })
                    .cloned()
                    .map(|selection| (selection.content_id.clone(), selection))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        for selection in requested_from_pack {
            if selection.content_id == pack_content_id {
                requested.insert(selection.content_id.clone(), selection);
            } else {
                requested
                    .entry(selection.content_id.clone())
                    .and_modify(|existing| existing.version = selection.version.clone())
                    .or_insert(selection);
            }
        }
        let requested = reconcile_pack_member_selections(
            requested.into_values().collect(),
            &transition.members,
        )?;
        let mut next = resolve_next_content(&manifest, current, requested, releases, false)?;
        let next_item_ids = next
            .items
            .iter()
            .map(|item| item.content_id.as_str())
            .collect::<BTreeSet<_>>();
        next.pack_members = transition
            .members
            .into_iter()
            .filter(|member| {
                next_item_ids.contains(member.pack_content_id.as_str())
                    && next_item_ids.contains(member.content_id.as_str())
            })
            .collect();
        next.overrides = current
            .map(|content| content.overrides.clone())
            .unwrap_or_default();
        next.overrides.retain(|override_file| {
            override_file.pack_content_id != pack_content_id
                && transition
                    .previous_pack_content_id
                    .as_ref()
                    .is_none_or(|previous| override_file.pack_content_id != *previous)
        });
        next.overrides.extend(
            extracted_overrides
                .iter()
                .map(|override_file| override_file.resolved.clone()),
        );
        next.overrides
            .sort_by(|left, right| left.relative_target.cmp(&right.relative_target));
        next.resolution_sha256 = content_lock_sha256(&next)?;
        validate_resolved_content_lock(&next)?;
        self.cache
            .activate_verified_copy(source_relative, &pack_sha256, pack_size_bytes)?;
        for override_file in extracted_overrides {
            self.cache.activate_verified_copy(
                &override_file.staging_relative,
                &override_file.resolved.sha256,
                override_file.resolved.size_bytes,
            )?;
        }
        for activation in pending_cache_activations {
            self.cache.activate_verified_copy(
                &activation.staging_relative,
                &activation.sha256,
                activation.size_bytes,
            )?;
        }
        let changed = changed_content_ids(current, &next);
        self.commit_content_revision(
            profile,
            manifest,
            lock,
            Some(next),
            OperationType::ContentImport,
            changed,
        )
    }

    fn finish_modpack_staging(
        &self,
        stage_id: &str,
        result: AppResult<Phase6OperationResult>,
    ) -> AppResult<Phase6OperationResult> {
        let cleanup = self.operations.cleanup_staging(stage_id);
        match (result, cleanup) {
            (Ok(value), _) => Ok(value),
            (Err(primary), Ok(())) => Err(primary),
            (Err(primary), Err(cleanup)) => Err(AppError::coded_with(
                "content_modpack_cleanup_failed",
                [
                    ("primary", primary.descriptor().code),
                    ("cleanup", cleanup.descriptor().code),
                ],
            )),
        }
    }

    pub fn export_profile(&self, profile_id: &str) -> AppResult<Phase6ProfileTransferResult> {
        let profile = self.profile_for_read(profile_id)?;
        let (manifest, lock) = self.read_active_documents(&profile)?;
        let revision_id = profile
            .active_revision_id
            .as_deref()
            .ok_or_else(|| AppError::coded("profile_active_revision_missing"))?;
        let document = ProfileExportV1::new(
            profile.display_name.clone(),
            manifest.runtime,
            manifest.s9lab_component,
            manifest.desired_content,
            lock.content.clone(),
        );
        let mut artifacts_by_hash = BTreeMap::new();
        for (relative_target, sha256, size_bytes) in lock.content.iter().flat_map(|content| {
            content
                .items
                .iter()
                .map(|item| {
                    (
                        item.relative_target.as_str(),
                        item.sha256.as_str(),
                        item.size_bytes,
                    )
                })
                .chain(content.overrides.iter().map(|override_file| {
                    (
                        override_file.relative_target.as_str(),
                        override_file.sha256.as_str(),
                        override_file.size_bytes,
                    )
                }))
        }) {
            let source = self.registry.resolve(
                "profiles",
                format!(
                    "{}/revisions/{revision_id}/content/{}",
                    profile.id, relative_target
                ),
            )?;
            let artifact = ProfileExportArtifactSource {
                sha256: sha256.to_string(),
                size_bytes,
                source,
            };
            if let Some(existing) = artifacts_by_hash.insert(sha256.to_string(), artifact) {
                if existing.size_bytes != size_bytes {
                    return Err(AppError::coded("profile_export_artifact_identity_conflict"));
                }
            }
        }
        let artifacts = artifacts_by_hash.into_values().collect::<Vec<_>>();
        let operation_id = new_identifier("profile-export");
        let file_name = format!(
            "{}-{}.s9profile",
            safe_export_stem(&profile.display_name),
            operation_id
                .rsplit('-')
                .next()
                .unwrap_or("profile")
                .chars()
                .take(12)
                .collect::<String>()
        );
        let destination = self.registry.resolve("exports", &file_name)?;
        export_profile_v1(&destination, &document, &artifacts)?;
        Ok(Phase6ProfileTransferResult {
            operation_id,
            profile_id: profile.id,
            display_name: profile.display_name,
            file_name: Some(file_name),
        })
    }

    pub async fn import_profile(
        &self,
        source_path: &str,
    ) -> AppResult<Phase6ProfileTransferResult> {
        let source = secure_external_file(source_path)?;
        let operation_id = new_identifier("profile-import");
        let imported = import_profile_v1(
            &source,
            &self.registry,
            "staging-operations",
            format!("{operation_id}/profile-format"),
        )?;
        for artifact in &imported.artifacts {
            self.cache.activate_verified_copy(
                &artifact.staged_path.relative().to_string_lossy(),
                &artifact.sha256,
                artifact.size_bytes,
            )?;
        }
        let document = imported.document;
        self.operations.cleanup_staging(&operation_id)?;
        let created = self.profiles.create_profile(&document.display_name)?;
        let display_name = document.display_name;
        let desired_content = document.desired_content;
        let resolved_content = document.resolved_content;
        let runtime_result = self
            .runtime
            .install(&created.id, document.runtime, document.s9lab_component)
            .await?;
        let mut completed_operation_id = runtime_result.operation_id;
        if let Some(content) = resolved_content {
            let profile = self.profile_for_mutation(&created.id)?;
            let (manifest, lock) = self.read_active_documents(&profile)?;
            if content.requested != desired_content {
                return Err(AppError::coded("profile_export_content_request_mismatch"));
            }
            completed_operation_id = self
                .commit_content_revision(
                    &profile,
                    manifest,
                    lock,
                    Some(content),
                    OperationType::ContentImport,
                    desired_content
                        .iter()
                        .map(|selection| selection.content_id.clone())
                        .collect(),
                )?
                .operation_id;
        }
        Ok(Phase6ProfileTransferResult {
            operation_id: completed_operation_id,
            profile_id: created.id,
            display_name,
            file_name: None,
        })
    }

    fn commit_content_revision(
        &self,
        profile: &ProfileRecord,
        mut manifest: ProfileManifestV2,
        mut lock: ProfileLockV2,
        content: Option<ResolvedContentLockV1>,
        operation_type: OperationType,
        mut changed_content_ids: Vec<String>,
    ) -> AppResult<Phase6OperationResult> {
        let operation_id = new_identifier("op");
        let revision_id = new_identifier("rev");
        manifest.created_at_unix = Utc::now().timestamp();
        manifest.desired_content = content
            .as_ref()
            .map(|value| value.requested.clone())
            .unwrap_or_default();
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        lock.revision_id = revision_id.clone();
        lock.manifest_sha256 = manifest_sha256.clone();
        lock.content = content;

        let mut cache_blobs = lock
            .runtime
            .items
            .iter()
            .map(|item| LockedCacheBlob {
                sha256: item.sha256.clone(),
                size_bytes: item.size_bytes,
            })
            .chain(
                lock.content
                    .iter()
                    .flat_map(|content| content.items.iter())
                    .map(|item| LockedCacheBlob {
                        sha256: item.sha256.clone(),
                        size_bytes: item.size_bytes,
                    }),
            )
            .collect::<Vec<_>>();
        cache_blobs.extend(
            lock.content
                .iter()
                .flat_map(|content| content.overrides.iter())
                .map(|override_file| LockedCacheBlob {
                    sha256: override_file.sha256.clone(),
                    size_bytes: override_file.size_bytes,
                }),
        );
        cache_blobs.sort();
        cache_blobs.dedup();
        lock.cache_blobs = cache_blobs;
        let lock_json = canonical_json(&lock)?;
        let lock_sha256 = sha256_hex(lock_json.as_bytes());

        let mut cache_materializations = lock
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
            .collect::<Vec<_>>();
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
        let previous_runtime_projection = self
            .storage
            .runtime_projection(&profile.id)?
            .ok_or_else(|| AppError::coded("content_runtime_projection_missing"))?;
        if Some(previous_runtime_projection.revision_id.as_str())
            != profile.active_revision_id.as_deref()
        {
            return Err(AppError::coded("content_runtime_projection_stale"));
        }
        let mut runtime_projection = previous_runtime_projection.clone();
        runtime_projection.revision_id = revision_id.clone();
        runtime_projection.updated_at_unix = Utc::now().timestamp();
        let plan = ProfileInstallPlan {
            operation_id: operation_id.clone(),
            profile_id: profile.id.clone(),
            revision_id: revision_id.clone(),
            previous_revision_id: profile.active_revision_id.clone(),
            manifest_json,
            manifest_sha256,
            lock_json,
            lock_sha256,
            payload_files: Vec::new(),
            cache_materializations,
            runtime_projection: Some(runtime_projection),
            previous_runtime_projection: Some(previous_runtime_projection),
            cleanup_profile_on_rollback: false,
        };
        self.operations
            .plan_profile_operation(&plan, operation_type)?;
        self.operations.execute(&operation_id)?;
        changed_content_ids.sort();
        changed_content_ids.dedup();
        Ok(Phase6OperationResult {
            operation_id,
            profile_id: profile.id.clone(),
            revision_id,
            changed_content_ids,
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
            return Err(AppError::coded("content_profile_not_active"));
        }
        if profile.active_revision_id.is_none() {
            return Err(AppError::coded("content_profile_runtime_required"));
        }
        Ok(profile)
    }

    fn read_active_documents(
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
            .map_err(|_| AppError::coded("content_runtime_manifest_required"))?;
        let lock: ProfileLockV2 = serde_json::from_slice(&lock_bytes)
            .map_err(|_| AppError::coded("content_runtime_lock_required"))?;
        if manifest.format != PROFILE_MANIFEST_FORMAT
            || manifest.format_version != PROFILE_FORMAT_VERSION
            || manifest.profile_id != profile.id
            || lock.format != PROFILE_LOCK_FORMAT
            || lock.format_version != PROFILE_FORMAT_VERSION
            || lock.profile_id != profile.id
            || lock.revision_id != revision_id
            || lock.manifest_sha256 != sha256_hex(&manifest_bytes)
            || manifest.runtime != lock.runtime.runtime
        {
            return Err(AppError::coded("content_profile_documents_invalid"));
        }
        if let Some(content) = lock.content.as_ref() {
            validate_resolved_content_lock(content)?;
            if content.runtime.minecraft_version != manifest.runtime.minecraft_version
                || content.runtime.loader != manifest.runtime.loader
                || content.requested != manifest.desired_content
            {
                return Err(AppError::coded("content_profile_lock_incompatible"));
            }
        } else if !manifest.desired_content.is_empty() {
            return Err(AppError::coded("content_profile_lock_missing"));
        }
        Ok((manifest, lock))
    }
}

fn installed_content(
    item: &ResolvedContentItemV1,
    managed_by_pack: bool,
) -> Phase6InstalledContent {
    let (project_id, version_id, source, file_name) = match item.source.as_ref() {
        Some(ContentSourceV1::Modrinth {
            project_id,
            version_id,
            file_name,
        }) => (
            Some(project_id.clone()),
            Some(version_id.clone()),
            "modrinth".to_string(),
            file_name.clone(),
        ),
        Some(ContentSourceV1::Local { file_name }) => {
            (None, None, "local".to_string(), file_name.clone())
        }
        None => (None, None, "local".to_string(), item.content_id.clone()),
    };
    let dependencies = item
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind != ContentDependencyKind::Incompatible)
        .map(|dependency| Phase6Dependency {
            project_id: dependency.content_id.clone(),
            display_name: dependency.content_id.clone(),
            relation: dependency.kind.as_str().to_string(),
            satisfied: true,
        })
        .collect();
    let conflicts = item
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind == ContentDependencyKind::Incompatible)
        .map(|dependency| Phase6Conflict {
            content_id: dependency.content_id.clone(),
            display_name: dependency.content_id.clone(),
            reason_code: "content_incompatible".into(),
        })
        .collect();
    Phase6InstalledContent {
        content_id: item.content_id.clone(),
        project_id,
        version_id,
        display_name: display_name(&file_name),
        version_number: item.version.clone(),
        content_type: item.kind,
        source,
        enabled: item.enabled,
        managed_by_pack,
        size_bytes: item.size_bytes,
        sha256: item.sha256.clone(),
        dependencies,
        conflicts,
        update: None,
    }
}

fn required_by_enabled_item(lock: &ResolvedContentLockV1, content_id: &str) -> bool {
    lock.items.iter().any(|item| {
        item.enabled
            && item.content_id != content_id
            && item.dependencies.iter().any(|dependency| {
                dependency.content_id == content_id
                    && dependency.kind == ContentDependencyKind::Required
                    && dependency.resolved_version.is_some()
            })
    })
}

fn same_pack_source(left: Option<&ContentSourceV1>, right: Option<&ContentSourceV1>) -> bool {
    match (left, right) {
        (
            Some(ContentSourceV1::Modrinth {
                project_id: left, ..
            }),
            Some(ContentSourceV1::Modrinth {
                project_id: right, ..
            }),
        ) => left == right,
        // A normal local import is an independent content-addressed pack.
        // Matching by its display filename would silently replace unrelated
        // packs that happen to share a common name such as `modpack.mrpack`.
        _ => false,
    }
}

fn prepare_pack_transition(
    current: Option<&ResolvedContentLockV1>,
    pack_content_id: &str,
    pack_source: Option<&ContentSourceV1>,
    new_members: &BTreeMap<String, (String, bool)>,
) -> AppResult<PackTransition> {
    let previous_pack_content_id = current.and_then(|content| {
        content
            .items
            .iter()
            .find(|item| {
                item.kind == ContentKind::Modpack
                    && same_pack_source(item.source.as_ref(), pack_source)
            })
            .map(|item| item.content_id.clone())
    });
    let mut members = current
        .map(|content| content.pack_members.clone())
        .unwrap_or_default();
    let mut previous_members = Vec::new();
    if let Some(previous_pack_content_id) = previous_pack_content_id.as_deref() {
        members.retain(|member| {
            if member.pack_content_id == previous_pack_content_id {
                previous_members.push(member.clone());
                false
            } else {
                true
            }
        });
    }

    let requested_ids = current
        .into_iter()
        .flat_map(|content| content.requested.iter())
        .map(|selection| selection.content_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut selection_removals = previous_pack_content_id
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for previous in &previous_members {
        if previous.owns_selection && !new_members.contains_key(&previous.content_id) {
            if let Some(successor) = members
                .iter_mut()
                .find(|member| member.content_id == previous.content_id)
            {
                successor.owns_selection = true;
            } else {
                selection_removals.insert(previous.content_id.clone());
            }
        }
    }

    let current_items = current
        .into_iter()
        .flat_map(|content| content.items.iter())
        .map(|item| (item.content_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    for (content_id, (version, default_enabled)) in new_members {
        let previous_member = previous_members
            .iter()
            .find(|member| member.content_id == *content_id);
        let carried_ownership = previous_member.is_some_and(|member| member.owns_selection);
        if let Some(conflict) = members
            .iter()
            .find(|member| member.content_id == *content_id && member.version != *version)
        {
            return Err(AppError::coded_with(
                "content_pack_member_version_conflict",
                [
                    ("contentId", content_id.clone()),
                    ("existingPackId", conflict.pack_content_id.clone()),
                ],
            ));
        }
        let already_requested =
            requested_ids.contains(content_id.as_str()) && !selection_removals.contains(content_id);
        if !carried_ownership
            && current_items
                .get(content_id.as_str())
                .is_some_and(|item| item.version != *version)
        {
            return Err(AppError::coded_with(
                "content_pack_selection_conflict",
                [("contentId", content_id.clone())],
            ));
        }
        let existing_owner = members
            .iter()
            .any(|member| member.content_id == *content_id && member.owns_selection);
        members.push(ResolvedContentPackMemberV1 {
            pack_content_id: pack_content_id.to_string(),
            content_id: content_id.clone(),
            version: version.clone(),
            enabled_by_default: *default_enabled,
            owns_selection: carried_ownership || (!already_requested && !existing_owner),
        });
    }
    members.sort_by(|left, right| {
        left.pack_content_id
            .cmp(&right.pack_content_id)
            .then_with(|| left.content_id.cmp(&right.content_id))
    });
    if members.len() > 4_096 {
        return Err(AppError::coded("content_pack_member_count_invalid"));
    }
    let mut resulting_selection_ids = current
        .into_iter()
        .flat_map(|content| content.requested.iter())
        .filter(|selection| !selection_removals.contains(&selection.content_id))
        .map(|selection| selection.content_id.clone())
        .collect::<BTreeSet<_>>();
    resulting_selection_ids.insert(pack_content_id.to_string());
    resulting_selection_ids.extend(new_members.keys().cloned());
    if resulting_selection_ids.len() > MAX_RESOLVED_CONTENT_ITEMS {
        return Err(AppError::coded("content_selection_count_invalid"));
    }
    Ok(PackTransition {
        previous_pack_content_id,
        members,
        selection_removals,
    })
}

fn reconcile_pack_member_selections(
    requested: Vec<ContentSelection>,
    members: &[ResolvedContentPackMemberV1],
) -> AppResult<Vec<ContentSelection>> {
    let mut requested = requested
        .into_iter()
        .map(|selection| (selection.content_id.clone(), selection))
        .collect::<BTreeMap<_, _>>();
    let pack_enabled = requested
        .iter()
        .map(|(content_id, selection)| (content_id.clone(), selection.enabled))
        .collect::<BTreeMap<_, _>>();
    let mut by_content = BTreeMap::<String, Vec<&ResolvedContentPackMemberV1>>::new();
    for member in members {
        by_content
            .entry(member.content_id.clone())
            .or_default()
            .push(member);
    }
    for (content_id, memberships) in by_content {
        let selection = requested
            .get_mut(&content_id)
            .ok_or_else(|| AppError::coded("content_pack_member_selection_missing"))?;
        let version = memberships
            .first()
            .map(|member| member.version.clone())
            .ok_or_else(|| AppError::coded("content_pack_member_invalid"))?;
        if memberships.iter().any(|member| member.version != version) {
            return Err(AppError::coded("content_pack_member_version_conflict"));
        }
        let owner_count = memberships
            .iter()
            .filter(|member| member.owns_selection)
            .count();
        if owner_count > 1 {
            return Err(AppError::coded("content_pack_member_owner_duplicate"));
        }
        let manual_enabled = owner_count == 0 && selection.enabled;
        let demanded_by_pack = memberships.iter().any(|member| {
            member.enabled_by_default
                && pack_enabled
                    .get(&member.pack_content_id)
                    .copied()
                    .unwrap_or(false)
        });
        selection.version = ContentVersionRequirement::Exact { version };
        selection.enabled = manual_enabled || demanded_by_pack;
    }
    Ok(requested.into_values().collect())
}

fn ensure_version_compatible(
    version: &ProjectVersion,
    kind: ContentKind,
    manifest: &ProfileManifestV2,
) -> AppResult<()> {
    if !version
        .game_versions
        .iter()
        .any(|candidate| candidate == &manifest.runtime.minecraft_version)
    {
        return Err(AppError::coded("content_minecraft_version_incompatible"));
    }
    if matches!(kind, ContentKind::Mod | ContentKind::Modpack)
        && !version
            .loaders
            .contains(&modrinth_loader(manifest.runtime.loader.kind))
    {
        return Err(AppError::coded("content_loader_incompatible"));
    }
    Ok(())
}

fn dependency_kind_from_modrinth(kind: DependencyType) -> ContentDependencyKind {
    match kind {
        DependencyType::Required => ContentDependencyKind::Required,
        DependencyType::Optional => ContentDependencyKind::Optional,
        DependencyType::Incompatible => ContentDependencyKind::Incompatible,
        DependencyType::Embedded => unreachable!("embedded dependencies are filtered"),
    }
}

fn compatibility_from_version(
    kind: ContentKind,
    version: &ProjectVersion,
    manifest: &ProfileManifestV2,
) -> ContentCompatibility {
    let mut minecraft_versions = version.game_versions.clone();
    minecraft_versions.sort();
    minecraft_versions.dedup();
    let loaders = if matches!(kind, ContentKind::Mod | ContentKind::Modpack) {
        let mut kinds = version
            .loaders
            .iter()
            .map(|loader| match loader {
                ModrinthLoader::Vanilla => LoaderKind::Vanilla,
                ModrinthLoader::Fabric => LoaderKind::Fabric,
                ModrinthLoader::Neoforge => LoaderKind::Neoforge,
            })
            .collect::<BTreeSet<_>>();
        if kinds.is_empty() {
            kinds.insert(manifest.runtime.loader.kind);
        }
        kinds
            .into_iter()
            .map(|kind| ContentLoaderCompatibility {
                kind,
                loader_versions: if kind == manifest.runtime.loader.kind {
                    manifest
                        .runtime
                        .loader
                        .loader_version
                        .iter()
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                },
            })
            .collect()
    } else {
        Vec::new()
    };
    ContentCompatibility {
        minecraft_versions,
        loaders,
    }
}

fn changed_content_ids(
    previous: Option<&ResolvedContentLockV1>,
    next: &ResolvedContentLockV1,
) -> Vec<String> {
    let previous = previous
        .map(|lock| {
            lock.items
                .iter()
                .map(|item| (item.content_id.clone(), item.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let next_by_id = next
        .items
        .iter()
        .map(|item| (item.content_id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    previous
        .keys()
        .chain(next_by_id.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|content_id| previous.get(*content_id) != next_by_id.get(*content_id).copied())
        .cloned()
        .collect()
}

fn resolve_next_content(
    manifest: &ProfileManifestV2,
    current: Option<&ResolvedContentLockV1>,
    requested: Vec<ContentSelection>,
    added: Vec<ContentReleaseV1>,
    preserve_pack_metadata: bool,
) -> AppResult<ResolvedContentLockV1> {
    let mut releases = current
        .map(releases_from_lock)
        .transpose()?
        .unwrap_or_default();
    releases.extend(added);
    let mut next = resolve_content(
        &ContentResolutionRequest {
            runtime: ContentTargetRuntime {
                minecraft_version: manifest.runtime.minecraft_version.clone(),
                loader: manifest.runtime.loader.clone(),
            },
            requested,
            include_optional_dependencies: current
                .is_some_and(|lock| lock.include_optional_dependencies),
        },
        &releases,
    )?;
    if let Some(current) = current.filter(|_| preserve_pack_metadata) {
        let pack_ids = next
            .items
            .iter()
            .filter(|item| item.kind == ContentKind::Modpack)
            .map(|item| item.content_id.as_str())
            .collect::<BTreeSet<_>>();
        let item_ids = next
            .items
            .iter()
            .map(|item| item.content_id.as_str())
            .collect::<BTreeSet<_>>();
        next.pack_members = current
            .pack_members
            .iter()
            .filter(|member| {
                pack_ids.contains(member.pack_content_id.as_str())
                    && item_ids.contains(member.content_id.as_str())
            })
            .cloned()
            .collect();
        next.overrides = current
            .overrides
            .iter()
            .filter(|override_file| pack_ids.contains(override_file.pack_content_id.as_str()))
            .cloned()
            .collect();
        next.resolution_sha256 = content_lock_sha256(&next)?;
        validate_resolved_content_lock(&next)?;
    }
    Ok(next)
}

fn releases_from_lock(lock: &ResolvedContentLockV1) -> AppResult<Vec<ContentReleaseV1>> {
    lock.items
        .iter()
        .map(|item| {
            let release = ContentReleaseV1 {
                format: CONTENT_RELEASE_FORMAT.into(),
                format_version: CONTENT_RELEASE_FORMAT_VERSION,
                content_id: item.content_id.clone(),
                version: item.version.clone(),
                kind: item.kind,
                compatibility: ContentCompatibility {
                    minecraft_versions: vec![lock.runtime.minecraft_version.clone()],
                    loaders: if matches!(item.kind, ContentKind::Mod | ContentKind::Modpack) {
                        vec![ContentLoaderCompatibility {
                            kind: lock.runtime.loader.kind,
                            loader_versions: lock
                                .runtime
                                .loader
                                .loader_version
                                .iter()
                                .cloned()
                                .collect(),
                        }]
                    } else {
                        Vec::new()
                    },
                },
                dependencies: item
                    .dependencies
                    .iter()
                    .map(|dependency| ContentDependency {
                        content_id: dependency.content_id.clone(),
                        kind: dependency.kind,
                        version: dependency.version_requirement.clone(),
                    })
                    .collect(),
                source: item.source.clone(),
                artifact: ContentArtifactV1 {
                    relative_target: item.relative_target.clone(),
                    sha256: item.sha256.clone(),
                    size_bytes: item.size_bytes,
                },
            };
            crate::content::validate_content_release(&release)?;
            Ok(release)
        })
        .collect()
}

fn compatibility_for_runtime(
    kind: ContentKind,
    manifest: &ProfileManifestV2,
) -> ContentCompatibility {
    ContentCompatibility {
        minecraft_versions: vec![manifest.runtime.minecraft_version.clone()],
        loaders: if matches!(kind, ContentKind::Mod | ContentKind::Modpack) {
            vec![ContentLoaderCompatibility {
                kind: manifest.runtime.loader.kind,
                loader_versions: manifest
                    .runtime
                    .loader
                    .loader_version
                    .iter()
                    .cloned()
                    .collect(),
            }]
        } else {
            Vec::new()
        },
    }
}

fn content_target(kind: ContentKind, file_name: &str) -> AppResult<String> {
    if file_name.is_empty()
        || file_name.contains(['/', '\\', ':'])
        || file_name.starts_with(['.', ' '])
        || file_name.ends_with(['.', ' '])
    {
        return Err(AppError::coded("content_local_file_name_invalid"));
    }
    let directory = match kind {
        ContentKind::Mod => "mods",
        ContentKind::Modpack => "modpacks",
        ContentKind::ShaderPack => "shaderpacks",
        ContentKind::ResourcePack => "resourcepacks",
    };
    Ok(format!("{directory}/{file_name}"))
}

fn secure_external_file(value: &str) -> AppResult<SecurePath> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.contains('\0')
        || (trimmed.contains('/') && trimmed.contains('\\'))
        || trimmed.contains("//")
        || trimmed.contains("\\\\")
    {
        return Err(AppError::coded("path_ambiguous_separator"));
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(AppError::coded("content_local_absolute_path_required"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::coded("content_local_parent_missing"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::coded("content_local_file_name_invalid"))?;
    let anchor = path
        .ancestors()
        .last()
        .ok_or_else(|| AppError::coded("content_local_anchor_missing"))?;
    let registry = PathRegistry::new(
        anchor,
        [RegisteredRoot {
            id: "selected-import".into(),
            path: parent.to_path_buf(),
        }],
    )?;
    let secure = registry.resolve("selected-import", Path::new(file_name))?;
    if !secure.absolute().is_file() {
        return Err(AppError::coded("content_local_not_regular_file"));
    }
    Ok(secure)
}

fn copy_external_to_staging_bounded(
    source: &SecurePath,
    destination: &SecurePath,
    max_bytes: u64,
) -> AppResult<u64> {
    validate_existing_chain(source.anchor(), source.absolute())?;
    let before = fs::symlink_metadata(source.absolute())?;
    if !before.is_file() || before.len() == 0 || before.len() > max_bytes {
        return Err(AppError::coded("content_local_size_invalid"));
    }
    let before_modified = before.modified().ok();
    let result = (|| -> AppResult<u64> {
        let mut input = fs::File::open(source.absolute())?;
        let mut output = secure_fs::open_new_file(destination)?;
        let mut copied = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            copied = copied
                .checked_add(read as u64)
                .ok_or_else(|| AppError::coded("content_local_size_invalid"))?;
            if copied > max_bytes || copied > before.len() {
                return Err(AppError::coded("content_local_changed_during_copy"));
            }
            output.write_all(&buffer[..read])?;
        }
        output.sync_all()?;
        validate_existing_chain(source.anchor(), source.absolute())?;
        validate_existing_chain(destination.anchor(), destination.absolute())?;
        let after = fs::symlink_metadata(source.absolute())?;
        if copied != before.len()
            || after.len() != before.len()
            || (before_modified.is_some() && after.modified().ok() != before_modified)
            || fs::metadata(destination.absolute())?.len() != copied
        {
            return Err(AppError::coded("content_local_changed_during_copy"));
        }
        Ok(copied)
    })();
    if result.is_err() && destination.absolute().exists() {
        let _ = secure_fs::remove_tree(destination);
    }
    result
}

fn hash_file(path: &Path) -> AppResult<String> {
    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn project_detail_dto(detail: ProjectDetail, versions: Vec<ProjectVersion>) -> Phase6ProjectDetail {
    Phase6ProjectDetail {
        project_id: detail.project_id,
        slug: detail.slug,
        title: detail.title,
        description: detail.description,
        content_type: content_kind(detail.project_type),
        author: "Modrinth".into(),
        license: detail.license.name,
        icon_url: detail.icon_url,
        downloads: detail.downloads,
        followers: detail.followers,
        updated_at_unix: detail.updated_at.timestamp(),
        categories: detail.categories,
        versions: versions.into_iter().map(project_version_dto).collect(),
    }
}

fn project_version_dto(version: ProjectVersion) -> Phase6ProjectVersion {
    let mut dependencies = Vec::new();
    let mut conflicts = Vec::new();
    for dependency in version.dependencies {
        let id = dependency
            .project_id
            .or(dependency.version_id)
            .unwrap_or_else(|| dependency.file_name.unwrap_or_else(|| "unknown".into()));
        if dependency.dependency_type == DependencyType::Incompatible {
            conflicts.push(Phase6Conflict {
                content_id: id.clone(),
                display_name: id,
                reason_code: "content_incompatible".into(),
            });
        } else if dependency.dependency_type != DependencyType::Embedded {
            dependencies.push(Phase6Dependency {
                project_id: id.clone(),
                display_name: id,
                relation: match dependency.dependency_type {
                    DependencyType::Required => "required",
                    DependencyType::Optional => "optional",
                    DependencyType::Incompatible | DependencyType::Embedded => unreachable!(),
                }
                .into(),
                satisfied: false,
            });
        }
    }
    Phase6ProjectVersion {
        version_id: version.version_id,
        version_number: version.version_number,
        name: version.name,
        published_at_unix: version.published_at.timestamp(),
        compatible: true,
        incompatibility_reason: None,
        dependencies,
        conflicts,
    }
}

fn project_type(kind: ContentKind) -> ProjectType {
    match kind {
        ContentKind::Mod => ProjectType::Mod,
        ContentKind::Modpack => ProjectType::Modpack,
        ContentKind::ShaderPack => ProjectType::Shader,
        ContentKind::ResourcePack => ProjectType::Resourcepack,
    }
}

fn content_kind(kind: ProjectType) -> ContentKind {
    match kind {
        ProjectType::Mod => ContentKind::Mod,
        ProjectType::Modpack => ContentKind::Modpack,
        ProjectType::Shader => ContentKind::ShaderPack,
        ProjectType::Resourcepack => ContentKind::ResourcePack,
    }
}

fn modrinth_loader(loader: LoaderKind) -> ModrinthLoader {
    match loader {
        LoaderKind::Vanilla => ModrinthLoader::Vanilla,
        LoaderKind::Fabric => ModrinthLoader::Fabric,
        LoaderKind::Neoforge => ModrinthLoader::Neoforge,
    }
}

fn display_name(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(file_name)
        .to_string()
}

fn safe_export_stem(display_name: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in display_name.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            separator = false;
        } else if !separator && !output.is_empty() {
            output.push('-');
            separator = true;
        }
        if output.len() >= 48 {
            break;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() || is_windows_reserved_stem(&output) {
        "s9lab-profile".into()
    } else {
        output
    }
}

fn is_windows_reserved_stem(value: &str) -> bool {
    let value = value.to_ascii_uppercase();
    matches!(value.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (value.len() == 4
            && matches!(&value[..3], "COM" | "LPT")
            && matches!(value.as_bytes()[3], b'1'..=b'9'))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MrpackIndex {
    format_version: u32,
    game: String,
    version_id: String,
    name: String,
    #[serde(default)]
    summary: Option<String>,
    files: Vec<MrpackFile>,
    dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MrpackFile {
    path: String,
    hashes: MrpackHashes,
    #[serde(default)]
    env: Option<MrpackEnvironment>,
    downloads: Vec<String>,
    file_size: u64,
}

#[derive(Debug, Deserialize)]
struct MrpackHashes {
    sha1: String,
    sha512: String,
    #[serde(flatten)]
    additional: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MrpackEnvironment {
    client: String,
    server: String,
}

struct ValidatedMrpackFile {
    project_id: String,
    version_id: String,
    file_name: String,
    relative_target: String,
    kind: ContentKind,
    enabled: bool,
    download: String,
    sha512: String,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
struct MrpackOverrideEntry {
    archive_index: usize,
    relative_target: String,
    size_bytes: u64,
}

struct ExtractedMrpackOverride {
    staging_relative: String,
    resolved: ResolvedContentOverrideV1,
}

struct PendingCacheActivation {
    staging_relative: String,
    sha256: String,
    size_bytes: u64,
}

struct DownloadedContentGraph {
    releases: Vec<ContentReleaseV1>,
    activations: Vec<PendingCacheActivation>,
}

struct PackTransition {
    previous_pack_content_id: Option<String>,
    members: Vec<ResolvedContentPackMemberV1>,
    selection_removals: BTreeSet<String>,
}

fn read_mrpack_index(path: &Path) -> AppResult<MrpackIndex> {
    let file = fs::File::open(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| AppError::coded("content_modpack_invalid"))?;
    let index = archive
        .by_name("modrinth.index.json")
        .map_err(|_| AppError::coded("content_modpack_index_missing"))?;
    if !index.is_file() || index.size() == 0 || index.size() > 8 * 1024 * 1024 {
        return Err(AppError::coded("content_modpack_index_size_invalid"));
    }
    let capacity = usize::try_from(index.size())
        .map_err(|_| AppError::coded("content_modpack_index_size_invalid"))?;
    let mut bytes = Vec::with_capacity(capacity);
    index.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(AppError::coded("content_modpack_index_size_invalid"));
    }
    serde_json::from_slice(&bytes).map_err(|_| AppError::coded("content_modpack_index_invalid"))
}

fn inspect_mrpack(
    path: &Path,
    manifest: &ProfileManifestV2,
    pack_size_bytes: u64,
) -> AppResult<(Vec<ValidatedMrpackFile>, Vec<MrpackOverrideEntry>)> {
    let index = read_mrpack_index(path)?;
    validate_mrpack_runtime(&index, manifest)?;
    let files = validate_mrpack_files(&index.files)?;
    let overrides = inventory_mrpack_overrides(path)?;
    validate_mrpack_target_layout(&files, &overrides)?;
    let payload_bytes = files
        .iter()
        .map(|file| file.size_bytes)
        .chain(overrides.iter().map(|entry| entry.size_bytes))
        .try_fold(pack_size_bytes, |total, size| total.checked_add(size))
        .ok_or_else(|| AppError::coded("content_modpack_size_overflow"))?;
    if payload_bytes > 8 * 1024 * 1024 * 1024 {
        return Err(AppError::coded("content_modpack_total_size_invalid"));
    }
    Ok((files, overrides))
}

fn mrpack_member_specs(files: &[ValidatedMrpackFile]) -> BTreeMap<String, (String, bool)> {
    files
        .iter()
        .map(|file| {
            (
                file.project_id.clone(),
                (file.version_id.clone(), file.enabled),
            )
        })
        .collect()
}

fn validate_mrpack_target_layout(
    files: &[ValidatedMrpackFile],
    overrides: &[MrpackOverrideEntry],
) -> AppResult<()> {
    validate_mrpack_target_set(
        files
            .iter()
            .map(|file| file.relative_target.as_str())
            .chain(overrides.iter().map(|entry| entry.relative_target.as_str())),
    )
}

fn validate_mrpack_target_set<'a>(candidates: impl IntoIterator<Item = &'a str>) -> AppResult<()> {
    let mut targets = BTreeMap::<String, String>::new();
    for target in candidates {
        let key = crate::security::paths::collision_key(Path::new(target))?;
        if targets.insert(key, target.to_string()).is_some() {
            return Err(AppError::coded("content_modpack_target_collision"));
        }
    }
    for (key, target) in &targets {
        for (separator, _) in key.match_indices('/') {
            if targets.contains_key(&key[..separator]) {
                return Err(AppError::coded_with(
                    "content_modpack_target_ancestor_collision",
                    [("relativeTarget", target.clone())],
                ));
            }
        }
    }
    Ok(())
}

fn validate_mrpack_profile_budget(
    registry: &PathRegistry,
    profile_id: &str,
    current: Option<&ResolvedContentLockV1>,
    transition: &PackTransition,
    pack_release: &ContentReleaseV1,
    files: &[ValidatedMrpackFile],
    overrides: &[MrpackOverrideEntry],
) -> AppResult<()> {
    let mut item_targets = current
        .into_iter()
        .flat_map(|content| content.items.iter())
        .filter(|item| !transition.selection_removals.contains(&item.content_id))
        .map(|item| (item.content_id.clone(), item.relative_target.clone()))
        .collect::<BTreeMap<_, _>>();
    item_targets.insert(
        pack_release.content_id.clone(),
        pack_release.artifact.relative_target.clone(),
    );
    for file in files {
        item_targets.insert(file.project_id.clone(), file.relative_target.clone());
    }
    if item_targets.len() > MAX_RESOLVED_CONTENT_ITEMS {
        return Err(AppError::coded("content_item_count_invalid"));
    }
    let retained_overrides = current
        .into_iter()
        .flat_map(|content| content.overrides.iter())
        .filter(|override_file| {
            override_file.pack_content_id != pack_release.content_id
                && transition
                    .previous_pack_content_id
                    .as_ref()
                    .is_none_or(|previous| override_file.pack_content_id != *previous)
        })
        .collect::<Vec<_>>();
    let override_count = retained_overrides
        .len()
        .checked_add(overrides.len())
        .ok_or_else(|| AppError::coded("content_override_count_invalid"))?;
    if override_count > MAX_RESOLVED_CONTENT_OVERRIDES {
        return Err(AppError::coded("content_override_count_invalid"));
    }
    let override_targets = retained_overrides
        .iter()
        .map(|entry| entry.relative_target.as_str())
        .chain(overrides.iter().map(|entry| entry.relative_target.as_str()))
        .collect::<Vec<_>>();
    validate_mrpack_target_set(
        item_targets
            .values()
            .map(String::as_str)
            .chain(override_targets.iter().copied()),
    )?;
    let revision_id = "rev-00000000000000000000000000000000";
    for target in item_targets
        .values()
        .map(String::as_str)
        .chain(override_targets.iter().copied())
    {
        registry.resolve(
            "profiles",
            format!("{profile_id}/revisions/{revision_id}/content/{target}"),
        )?;
        registry.resolve("profiles", format!("{profile_id}/instance/{target}"))?;
    }

    let mut item_sizes = current
        .into_iter()
        .flat_map(|content| content.items.iter())
        .filter(|item| !transition.selection_removals.contains(&item.content_id))
        .map(|item| (item.content_id.clone(), item.size_bytes))
        .collect::<BTreeMap<_, _>>();
    item_sizes.insert(
        pack_release.content_id.clone(),
        pack_release.artifact.size_bytes,
    );
    for file in files {
        item_sizes.insert(file.project_id.clone(), file.size_bytes);
    }
    let mut total = item_sizes
        .values()
        .copied()
        .try_fold(0u64, |total, size| total.checked_add(size))
        .ok_or_else(|| AppError::coded("content_lock_size_overflow"))?;
    total = retained_overrides
        .into_iter()
        .map(|override_file| override_file.size_bytes)
        .chain(overrides.iter().map(|entry| entry.size_bytes))
        .try_fold(total, |total, size| total.checked_add(size))
        .ok_or_else(|| AppError::coded("content_lock_size_overflow"))?;
    if total > 8 * 1024 * 1024 * 1024 {
        return Err(AppError::coded("content_lock_total_size_invalid"));
    }
    Ok(())
}

fn validate_mrpack_runtime(index: &MrpackIndex, manifest: &ProfileManifestV2) -> AppResult<()> {
    if index.format_version != 1
        || index.game != "minecraft"
        || index.version_id.trim().is_empty()
        || index.version_id.len() > 128
        || index.name.trim().is_empty()
        || index.name.len() > 256
        || index
            .summary
            .as_ref()
            .is_some_and(|value| value.len() > 4_096)
        || index.files.len() > 4_095
    {
        return Err(AppError::coded("content_modpack_index_invalid"));
    }
    if index.dependencies.get("minecraft") != Some(&manifest.runtime.minecraft_version) {
        return Err(AppError::coded("content_modpack_minecraft_mismatch"));
    }
    let expected_loader = match manifest.runtime.loader.kind {
        LoaderKind::Vanilla => None,
        LoaderKind::Fabric => Some("fabric-loader"),
        LoaderKind::Neoforge => Some("neoforge"),
    };
    let known_loaders = ["fabric-loader", "neoforge", "forge", "quilt-loader"];
    let present_loaders = known_loaders
        .into_iter()
        .filter(|loader| index.dependencies.contains_key(*loader))
        .collect::<BTreeSet<_>>();
    if let Some(expected_loader) = expected_loader {
        if present_loaders != BTreeSet::from([expected_loader])
            || index.dependencies.get(expected_loader)
                != manifest.runtime.loader.loader_version.as_ref()
        {
            return Err(AppError::coded("content_modpack_loader_mismatch"));
        }
    } else if !present_loaders.is_empty() {
        return Err(AppError::coded("content_modpack_loader_mismatch"));
    }
    Ok(())
}

fn validate_mrpack_files(files: &[MrpackFile]) -> AppResult<Vec<ValidatedMrpackFile>> {
    let mut outputs = Vec::with_capacity(files.len());
    let mut targets = BTreeSet::new();
    let mut project_ids = BTreeSet::new();
    let mut total_size = 0u64;
    for file in files {
        if file.path.contains('\\') || file.path.contains("//") {
            return Err(AppError::coded("path_ambiguous_separator"));
        }
        let normalized = crate::security::paths::normalize_relative_path(Path::new(&file.path))?;
        let normalized_text = normalized
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if normalized_text != file.path || normalized.components().count() != 2 {
            return Err(AppError::coded("content_modpack_target_invalid"));
        }
        let mut components = normalized.iter();
        let directory = components
            .next()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::coded("content_modpack_target_invalid"))?;
        let file_name = components
            .next()
            .and_then(|value| value.to_str())
            .ok_or_else(|| AppError::coded("content_modpack_target_invalid"))?;
        let kind = match directory {
            "mods" => ContentKind::Mod,
            "resourcepacks" => ContentKind::ResourcePack,
            "shaderpacks" => ContentKind::ShaderPack,
            _ => return Err(AppError::coded("content_modpack_target_invalid")),
        };
        if content_target(kind, file_name)? != normalized_text {
            return Err(AppError::coded("content_modpack_target_invalid"));
        }
        let collision = crate::security::paths::collision_key(&normalized)?;
        if !targets.insert(collision) {
            return Err(AppError::coded("content_modpack_target_collision"));
        }
        if file.file_size == 0 || file.file_size > 1_073_741_824 {
            return Err(AppError::coded("content_modpack_file_size_invalid"));
        }
        total_size = total_size
            .checked_add(file.file_size)
            .ok_or_else(|| AppError::coded("content_modpack_size_overflow"))?;
        if total_size > 8 * 1024 * 1024 * 1024 {
            return Err(AppError::coded("content_modpack_total_size_invalid"));
        }
        validate_lower_hex(&file.hashes.sha1, 40, "content_modpack_sha1_invalid")?;
        validate_lower_hex(&file.hashes.sha512, 128, "content_modpack_sha512_invalid")?;
        if file.hashes.additional.len() > 16
            || file.hashes.additional.iter().any(|(algorithm, value)| {
                algorithm.is_empty()
                    || algorithm.len() > 32
                    || !algorithm.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                    || value.is_empty()
                    || value.len() > 256
                    || value.len() % 2 != 0
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
        {
            return Err(AppError::coded("content_modpack_additional_hash_invalid"));
        }
        let (download, project_id, version_id) = select_modrinth_download(&file.downloads)?;
        if !project_ids.insert(project_id.clone()) {
            return Err(AppError::coded("content_modpack_identity_collision"));
        }
        let client = file
            .env
            .as_ref()
            .map(|environment| environment.client.as_str())
            .unwrap_or("required");
        if file.env.as_ref().is_some_and(|environment| {
            !matches!(
                environment.server.as_str(),
                "required" | "optional" | "unsupported"
            )
        }) || !matches!(client, "required" | "optional" | "unsupported")
        {
            return Err(AppError::coded("content_modpack_environment_invalid"));
        }
        if client == "unsupported" {
            continue;
        }
        outputs.push(ValidatedMrpackFile {
            project_id,
            version_id,
            file_name: file_name.to_string(),
            relative_target: normalized_text,
            kind,
            enabled: client == "required",
            download,
            sha512: file.hashes.sha512.clone(),
            size_bytes: file.file_size,
        });
    }
    Ok(outputs)
}

fn modrinth_identity_from_download(value: &str) -> AppResult<(String, String)> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| AppError::coded("content_modpack_download_url_invalid"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("cdn.modrinth.com")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::coded("content_modpack_download_url_invalid"));
    }
    let segments = url
        .path_segments()
        .ok_or_else(|| AppError::coded("content_modpack_download_url_invalid"))?
        .collect::<Vec<_>>();
    if segments.len() != 5 || segments[0] != "data" || segments[2] != "versions" {
        return Err(AppError::coded("content_modpack_download_url_invalid"));
    }
    crate::modrinth::validate_modrinth_id(segments[1])?;
    crate::modrinth::validate_modrinth_id(segments[3])?;
    Ok((segments[1].to_string(), segments[3].to_string()))
}

fn select_modrinth_download(downloads: &[String]) -> AppResult<(String, String, String)> {
    if downloads.is_empty() || downloads.len() > 8 {
        return Err(AppError::coded("content_modpack_download_count_invalid"));
    }
    let allowed_hosts = [
        "cdn.modrinth.com",
        "github.com",
        "raw.githubusercontent.com",
        "gitlab.com",
    ];
    let mut selected: Option<(String, String, String)> = None;
    for value in downloads {
        if value.len() > 2_048 {
            return Err(AppError::coded("content_modpack_download_url_invalid"));
        }
        let url = reqwest::Url::parse(value)
            .map_err(|_| AppError::coded("content_modpack_download_url_invalid"))?;
        let host = url
            .host_str()
            .ok_or_else(|| AppError::coded("content_modpack_download_url_invalid"))?;
        if url.scheme() != "https"
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || !allowed_hosts.contains(&host)
        {
            return Err(AppError::coded("content_modpack_download_url_invalid"));
        }
        if host != "cdn.modrinth.com" {
            continue;
        }
        let (project_id, version_id) = modrinth_identity_from_download(value)?;
        if selected
            .as_ref()
            .is_some_and(|(_, selected_project, selected_version)| {
                selected_project != &project_id || selected_version != &version_id
            })
        {
            return Err(AppError::coded(
                "content_modpack_download_identity_ambiguous",
            ));
        }
        selected.get_or_insert_with(|| (value.clone(), project_id, version_id));
    }
    selected.ok_or_else(|| AppError::coded("content_modpack_modrinth_download_missing"))
}

fn verify_mrpack_file_version(
    file: &ValidatedMrpackFile,
    version: &ProjectVersion,
) -> AppResult<()> {
    if version.project_id != file.project_id || version.version_id != file.version_id {
        return Err(AppError::coded("content_modpack_version_identity_mismatch"));
    }
    if !version.files.iter().any(|candidate| {
        candidate.size_bytes == file.size_bytes
            && candidate.upstream_sha512 == file.sha512
            && candidate.validated_download_url().as_str() == file.download
    }) {
        return Err(AppError::coded("content_modpack_version_file_missing"));
    }
    Ok(())
}

fn inventory_mrpack_overrides(path: &Path) -> AppResult<Vec<MrpackOverrideEntry>> {
    let file = fs::File::open(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| AppError::coded("content_modpack_invalid"))?;
    let mut selected = BTreeMap::<String, (u8, MrpackOverrideEntry)>::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::coded("content_modpack_invalid"))?;
        if !entry.is_file() || entry.name() == "modrinth.index.json" {
            continue;
        }
        let (priority, relative_target) =
            if let Some(value) = entry.name().strip_prefix("overrides/") {
                (0, value)
            } else if let Some(value) = entry.name().strip_prefix("client-overrides/") {
                (1, value)
            } else if entry.name().starts_with("server-overrides/") {
                continue;
            } else {
                return Err(AppError::coded("content_modpack_entry_unsupported"));
            };
        validate_content_override_target(relative_target)?;
        if entry.size() > 536_870_912 {
            return Err(AppError::coded("content_override_size_invalid"));
        }
        let key = crate::security::paths::collision_key(Path::new(relative_target))?;
        let candidate = MrpackOverrideEntry {
            archive_index: index,
            relative_target: relative_target.to_string(),
            size_bytes: entry.size(),
        };
        match selected.get(&key) {
            Some((existing_priority, existing))
                if existing.relative_target != candidate.relative_target
                    || *existing_priority == priority =>
            {
                return Err(AppError::coded("content_override_target_collision"));
            }
            Some((existing_priority, _)) if *existing_priority > priority => {}
            _ => {
                selected.insert(key, (priority, candidate));
            }
        }
    }
    if selected.len() > 8_192 {
        return Err(AppError::coded("content_override_count_invalid"));
    }
    let total_size = selected.values().try_fold(0u64, |total, (_, entry)| {
        total.checked_add(entry.size_bytes)
    });
    if total_size.ok_or_else(|| AppError::coded("content_override_size_overflow"))? > 4_294_967_296
    {
        return Err(AppError::coded("content_override_total_size_invalid"));
    }
    let mut overrides = selected
        .into_values()
        .map(|(_, entry)| entry)
        .collect::<Vec<_>>();
    overrides.sort_by(|left, right| left.relative_target.cmp(&right.relative_target));
    Ok(overrides)
}

fn validate_lower_hex(value: &str, length: usize, code: &str) -> AppResult<()> {
    if value.len() != length
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded(code));
    }
    Ok(())
}

fn read_bounded_document(path: &Path) -> AppResult<Vec<u8>> {
    validate_existing_chain(
        path.parent()
            .ok_or_else(|| AppError::coded("content_document_parent_missing"))?,
        path,
    )?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_PROFILE_DOCUMENT_BYTES {
        return Err(AppError::coded("content_profile_document_size_invalid"));
    }
    Ok(fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::{CONTENT_LOCK_FORMAT, CONTENT_LOCK_FORMAT_VERSION},
        profiles::model::S9labComponentSelection,
        runtime::{JavaPolicy, LoaderSelection, ProfileRuntimeIntent},
    };
    use zip::write::SimpleFileOptions;

    fn selection(content_id: &str, version: &str, enabled: bool) -> ContentSelection {
        ContentSelection {
            content_id: content_id.into(),
            version: ContentVersionRequirement::Exact {
                version: version.into(),
            },
            enabled,
        }
    }

    fn item(
        content_id: &str,
        version: &str,
        kind: ContentKind,
        enabled: bool,
        source: Option<ContentSourceV1>,
    ) -> ResolvedContentItemV1 {
        let directory = match kind {
            ContentKind::Mod => "mods",
            ContentKind::Modpack => "modpacks",
            ContentKind::ShaderPack => "shaderpacks",
            ContentKind::ResourcePack => "resourcepacks",
        };
        ResolvedContentItemV1 {
            content_id: content_id.into(),
            version: version.into(),
            kind,
            enabled,
            source,
            relative_target: format!("{directory}/{content_id}.jar"),
            sha256: "a".repeat(64),
            size_bytes: 1,
            dependencies: Vec::new(),
        }
    }

    fn content_lock(
        requested: Vec<ContentSelection>,
        items: Vec<ResolvedContentItemV1>,
        pack_members: Vec<ResolvedContentPackMemberV1>,
    ) -> ResolvedContentLockV1 {
        ResolvedContentLockV1 {
            format: CONTENT_LOCK_FORMAT.into(),
            format_version: CONTENT_LOCK_FORMAT_VERSION,
            runtime: ContentTargetRuntime {
                minecraft_version: "1.21.1".into(),
                loader: LoaderSelection {
                    kind: LoaderKind::Fabric,
                    loader_version: Some("0.16.10".into()),
                },
            },
            include_optional_dependencies: false,
            requested,
            items,
            pack_members,
            overrides: Vec::new(),
            resolution_sha256: "0".repeat(64),
        }
    }

    fn manifest(loader: LoaderKind, loader_version: Option<&str>) -> ProfileManifestV2 {
        ProfileManifestV2 {
            format: PROFILE_MANIFEST_FORMAT.into(),
            format_version: PROFILE_FORMAT_VERSION,
            profile_id: "profile-test".into(),
            created_at_unix: 0,
            runtime: ProfileRuntimeIntent {
                minecraft_version: "1.21.1".into(),
                loader: LoaderSelection {
                    kind: loader,
                    loader_version: loader_version.map(str::to_string),
                },
                java: JavaPolicy::Managed { major_version: 21 },
            },
            s9lab_component: S9labComponentSelection::Disabled,
            desired_content: Vec::new(),
            mutable_directories: Vec::new(),
            isolation_policy: "verified-copy-no-hardlinks".into(),
        }
    }

    #[test]
    fn shared_pack_member_enabled_state_is_the_or_of_active_pack_demands() {
        let members = vec![
            ResolvedContentPackMemberV1 {
                pack_content_id: "pack-a".into(),
                content_id: "shared".into(),
                version: "v1".into(),
                enabled_by_default: false,
                owns_selection: true,
            },
            ResolvedContentPackMemberV1 {
                pack_content_id: "pack-b".into(),
                content_id: "shared".into(),
                version: "v1".into(),
                enabled_by_default: true,
                owns_selection: false,
            },
        ];
        let requested = vec![
            selection("pack-a", "a1", true),
            selection("pack-b", "b1", true),
            selection("shared", "v1", false),
        ];
        let enabled =
            reconcile_pack_member_selections(requested, &members).expect("shared member enabled");
        assert!(
            enabled
                .iter()
                .find(|value| value.content_id == "shared")
                .expect("shared selection")
                .enabled
        );

        let requested = enabled
            .into_iter()
            .map(|mut value| {
                if value.content_id == "pack-b" {
                    value.enabled = false;
                }
                value
            })
            .collect();
        let disabled = reconcile_pack_member_selections(requested, &members)
            .expect("optional-only member disabled");
        assert!(
            !disabled
                .iter()
                .find(|value| value.content_id == "shared")
                .expect("shared selection")
                .enabled
        );
    }

    #[test]
    fn manual_pack_member_selection_remains_enabled_without_an_active_pack() {
        let members = vec![ResolvedContentPackMemberV1 {
            pack_content_id: "pack-a".into(),
            content_id: "shared".into(),
            version: "v1".into(),
            enabled_by_default: false,
            owns_selection: false,
        }];
        let requested = vec![
            selection("pack-a", "a1", false),
            selection("shared", "v1", true),
        ];
        let reconciled = reconcile_pack_member_selections(requested, &members)
            .expect("manual selection preserved");
        assert!(
            reconciled
                .iter()
                .find(|value| value.content_id == "shared")
                .expect("shared selection")
                .enabled
        );
    }

    #[test]
    fn local_packs_with_the_same_filename_are_independent() {
        let source = ContentSourceV1::Local {
            file_name: "modpack.mrpack".into(),
        };
        let current = content_lock(
            vec![selection("old-pack", "old", true)],
            vec![item(
                "old-pack",
                "old",
                ContentKind::Modpack,
                true,
                Some(source.clone()),
            )],
            Vec::new(),
        );
        let transition =
            prepare_pack_transition(Some(&current), "new-pack", Some(&source), &BTreeMap::new())
                .expect("independent local pack");
        assert!(transition.previous_pack_content_id.is_none());
        assert!(transition.selection_removals.is_empty());
    }

    #[test]
    fn modrinth_pack_update_replaces_only_the_same_project() {
        let old_source = ContentSourceV1::Modrinth {
            project_id: "project-a".into(),
            version_id: "version-old".into(),
            file_name: "old.mrpack".into(),
        };
        let new_source = ContentSourceV1::Modrinth {
            project_id: "project-a".into(),
            version_id: "version-new".into(),
            file_name: "new.mrpack".into(),
        };
        let current = content_lock(
            vec![selection("project-a", "version-old", true)],
            vec![item(
                "project-a",
                "version-old",
                ContentKind::Modpack,
                true,
                Some(old_source),
            )],
            Vec::new(),
        );
        let transition = prepare_pack_transition(
            Some(&current),
            "project-a",
            Some(&new_source),
            &BTreeMap::new(),
        )
        .expect("same Modrinth project update");
        assert_eq!(
            transition.previous_pack_content_id.as_deref(),
            Some("project-a")
        );
        assert!(transition.selection_removals.contains("project-a"));
    }

    #[test]
    fn mrpack_accepts_additional_hashes_and_selects_the_modrinth_url() {
        let file = MrpackFile {
            path: "mods/example.jar".into(),
            hashes: MrpackHashes {
                sha1: "a".repeat(40),
                sha512: "b".repeat(128),
                additional: BTreeMap::from([("sha256".into(), "c".repeat(64))]),
            },
            env: None,
            downloads: vec![
                "https://github.com/example/project/releases/download/v1/example.jar".into(),
                "https://cdn.modrinth.com/data/projectA/versions/versionA/example.jar".into(),
            ],
            file_size: 1,
        };
        let validated = validate_mrpack_files(&[file]).expect("valid fallback list");
        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].project_id, "projectA");
        assert_eq!(validated[0].version_id, "versionA");
    }

    #[test]
    fn mrpack_rejects_mixed_known_loader_dependencies() {
        let index = MrpackIndex {
            format_version: 1,
            game: "minecraft".into(),
            version_id: "pack-version".into(),
            name: "Pack".into(),
            summary: None,
            files: Vec::new(),
            dependencies: BTreeMap::from([
                ("minecraft".into(), "1.21.1".into()),
                ("fabric-loader".into(), "0.16.10".into()),
                ("forge".into(), "52.0.1".into()),
            ]),
        };
        let error = validate_mrpack_runtime(&index, &manifest(LoaderKind::Fabric, Some("0.16.10")))
            .expect_err("mixed loader keys");
        assert_eq!(error.descriptor().code, "content_modpack_loader_mismatch");
    }

    #[test]
    fn client_overrides_take_precedence_and_zero_byte_files_are_valid() {
        let path = std::env::temp_dir().join(format!(
            "s9lab-mrpack-overrides-{}.mrpack",
            new_identifier("test")
        ));
        let file = fs::File::create(&path).expect("archive");
        let mut writer = zip::ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer
            .start_file("overrides/config/example.toml", options)
            .expect("base override");
        writer.write_all(b"base").expect("base bytes");
        writer
            .start_file("client-overrides/config/example.toml", options)
            .expect("client override");
        writer.finish().expect("finish archive");

        let overrides = inventory_mrpack_overrides(&path).expect("override inventory");
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].relative_target, "config/example.toml");
        assert_eq!(overrides[0].size_bytes, 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn mrpack_target_layout_rejects_file_ancestor_collisions() {
        let overrides = vec![
            MrpackOverrideEntry {
                archive_index: 0,
                relative_target: "config".into(),
                size_bytes: 1,
            },
            MrpackOverrideEntry {
                archive_index: 1,
                relative_target: "config/example.toml".into(),
                size_bytes: 1,
            },
        ];
        let error = validate_mrpack_target_layout(&[], &overrides)
            .expect_err("ancestor collision rejected");
        assert_eq!(
            error.descriptor().code,
            "content_modpack_target_ancestor_collision"
        );
    }

    #[test]
    fn mrpack_profile_budget_rejects_unmaterializable_windows_paths_before_downloads() {
        let root = crate::foundation::test_root("phase6-mrpack-path-budget");
        let core = CoreServices::open_fixed(&root).expect("core");
        let long_name = format!("{}.jar", "a".repeat(170));
        let file = ValidatedMrpackFile {
            project_id: "projectA".into(),
            version_id: "versionA".into(),
            file_name: long_name.clone(),
            relative_target: format!("mods/{long_name}"),
            kind: ContentKind::Mod,
            enabled: true,
            download: "https://cdn.modrinth.com/data/projectA/versions/versionA/file.jar".into(),
            sha512: "a".repeat(128),
            size_bytes: 1,
        };
        let pack = ContentReleaseV1 {
            format: CONTENT_RELEASE_FORMAT.into(),
            format_version: CONTENT_RELEASE_FORMAT_VERSION,
            content_id: "pack-a".into(),
            version: "version-a".into(),
            kind: ContentKind::Modpack,
            compatibility: ContentCompatibility {
                minecraft_versions: vec!["1.21.1".into()],
                loaders: Vec::new(),
            },
            dependencies: Vec::new(),
            source: Some(ContentSourceV1::Local {
                file_name: "pack.mrpack".into(),
            }),
            artifact: ContentArtifactV1 {
                relative_target: "modpacks/pack.mrpack".into(),
                sha256: "a".repeat(64),
                size_bytes: 1,
            },
        };
        let transition = PackTransition {
            previous_pack_content_id: None,
            members: Vec::new(),
            selection_removals: BTreeSet::new(),
        };
        let error = validate_mrpack_profile_budget(
            core.registry(),
            "profile-00000000000000000000000000000000",
            None,
            &transition,
            &pack,
            &[file],
            &[],
        )
        .expect_err("path budget preflight");
        assert_eq!(error.descriptor().code, "path_too_long");
        let _ = fs::remove_dir_all(root);
    }
}
