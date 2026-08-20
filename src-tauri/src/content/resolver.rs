use super::model::{
    ContentArtifactV1, ContentCompatibility, ContentDependency, ContentDependencyKind, ContentKind,
    ContentReleaseV1, ContentResolutionRequest, ContentSelection, ContentSourceV1,
    ContentTargetRuntime, ContentVersionRequirement, ResolvedContentItemV1, ResolvedContentLockV1,
    ResolvedContentOverrideV1, ResolvedContentPackMemberV1, ResolvedDependencyV1,
    CONTENT_LOCK_FORMAT, CONTENT_LOCK_FORMAT_VERSION, CONTENT_RELEASE_FORMAT,
    CONTENT_RELEASE_FORMAT_VERSION,
};
use crate::{
    error::{AppError, AppResult},
    runtime::LoaderKind,
    security::{
        paths::{collision_key, normalize_relative_path},
        PathRegistry, SecurePath,
    },
};
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const MAX_CONTENT_RELEASES: usize = 50_000;
pub const MAX_RESOLVED_CONTENT_ITEMS: usize = 4_096;
pub const MAX_RESOLVED_CONTENT_OVERRIDES: usize = 8_192;
pub const MAX_PROJECTED_CONTENT_ITEMS: usize =
    MAX_RESOLVED_CONTENT_ITEMS + MAX_RESOLVED_CONTENT_OVERRIDES;
const MAX_CONTENT_SELECTIONS: usize = MAX_RESOLVED_CONTENT_ITEMS;
const MAX_CONTENT_DEPENDENCIES: usize = 512;
const MAX_COMPATIBILITY_VALUES: usize = 512;
const MAX_CONTENT_ID_BYTES: usize = 128;
const MAX_VERSION_BYTES: usize = 128;
const MAX_MINECRAFT_VERSION_BYTES: usize = 64;
const MAX_PROVIDER_ID_BYTES: usize = 192;
const MAX_CONTENT_ARTIFACT_BYTES: u64 = 1_073_741_824;
const MAX_CONTENT_OVERRIDES: usize = MAX_RESOLVED_CONTENT_OVERRIDES;
const MAX_CONTENT_OVERRIDE_TOTAL_BYTES: u64 = 4_294_967_296;
const MAX_CONTENT_TOTAL_BYTES: u64 = 8_589_934_592;
const MAX_RESOLUTION_DEPTH: usize = 256;
const MAX_RESOLUTION_STEPS: usize = 250_000;

pub fn validate_content_resolution_request(request: &ContentResolutionRequest) -> AppResult<()> {
    validate_target_runtime(&request.runtime)?;
    if request.requested.len() > MAX_CONTENT_SELECTIONS {
        return Err(AppError::coded("content_selection_count_invalid"));
    }
    let mut ids = BTreeSet::new();
    for selection in &request.requested {
        validate_content_id(&selection.content_id)?;
        validate_version_requirement(&selection.version)?;
        if !ids.insert(selection.content_id.clone()) {
            return Err(AppError::coded_with(
                "content_selection_duplicate",
                [("contentId", selection.content_id.clone())],
            ));
        }
    }
    Ok(())
}

pub fn validate_content_release(release: &ContentReleaseV1) -> AppResult<()> {
    if release.format != CONTENT_RELEASE_FORMAT
        || release.format_version != CONTENT_RELEASE_FORMAT_VERSION
    {
        return Err(AppError::coded("content_release_format_unsupported"));
    }
    validate_content_id(&release.content_id)?;
    validate_version(&release.version, "content_version_invalid")?;
    validate_compatibility(release.kind, &release.compatibility)?;
    if release.dependencies.len() > MAX_CONTENT_DEPENDENCIES {
        return Err(AppError::coded("content_dependency_count_invalid"));
    }
    let mut dependency_keys = BTreeSet::new();
    let mut activated_dependency_ids = BTreeSet::new();
    for dependency in &release.dependencies {
        validate_content_id(&dependency.content_id)?;
        if dependency.content_id == release.content_id {
            return Err(AppError::coded("content_self_dependency_forbidden"));
        }
        validate_version_requirement(&dependency.version)?;
        if !dependency_keys.insert((dependency.content_id.clone(), dependency.kind)) {
            return Err(AppError::coded_with(
                "content_dependency_duplicate",
                [
                    ("contentId", release.content_id.clone()),
                    ("dependencyId", dependency.content_id.clone()),
                    ("kind", dependency.kind.as_str().to_string()),
                ],
            ));
        }
        if dependency.kind != ContentDependencyKind::Incompatible
            && !activated_dependency_ids.insert(dependency.content_id.clone())
        {
            return Err(AppError::coded_with(
                "content_dependency_activation_duplicate",
                [
                    ("contentId", release.content_id.clone()),
                    ("dependencyId", dependency.content_id.clone()),
                ],
            ));
        }
    }
    validate_artifact(release.kind, &release.artifact)?;
    if let Some(source) = &release.source {
        validate_source(source, &release.artifact.relative_target)?;
    }
    Ok(())
}

pub fn resolve_content(
    request: &ContentResolutionRequest,
    releases: &[ContentReleaseV1],
) -> AppResult<ResolvedContentLockV1> {
    validate_content_resolution_request(request)?;
    let catalog = build_catalog(releases)?;

    let mut active_constraints = BTreeMap::<String, Vec<ContentVersionRequirement>>::new();
    let all_selection_requirements = request
        .requested
        .iter()
        .map(|selection| (selection.content_id.clone(), selection.version.clone()))
        .collect::<BTreeMap<_, _>>();
    for selection in request
        .requested
        .iter()
        .filter(|selection| selection.enabled)
    {
        active_constraints
            .entry(selection.content_id.clone())
            .or_default()
            .push(selection.version.clone());
    }

    let mut resolution_budget = ResolutionBudget {
        remaining_steps: MAX_RESOLUTION_STEPS,
    };
    let resolution_inputs = ResolutionInputs {
        catalog: &catalog,
        runtime: &request.runtime,
        include_optional: request.include_optional_dependencies,
        selection_requirements: &all_selection_requirements,
    };
    let active = solve_active(
        &resolution_inputs,
        &mut resolution_budget,
        BTreeMap::new(),
        active_constraints,
        0,
    )
    .map_err(ResolutionFailure::into_error)?;

    let mut selected = active
        .into_iter()
        .map(|(id, release)| (id, (release, true)))
        .collect::<BTreeMap<_, _>>();
    let mut occupied_targets = selected
        .values()
        .map(|(release, _)| {
            collision_key(Path::new(&release.artifact.relative_target))
                .map(|key| (key, release.content_id.clone()))
        })
        .collect::<AppResult<BTreeMap<_, _>>>()?;

    let mut disabled = request
        .requested
        .iter()
        .filter(|selection| !selection.enabled)
        .collect::<Vec<_>>();
    disabled.sort_by(|left, right| left.content_id.cmp(&right.content_id));
    for selection in disabled {
        if let Some((release, enabled)) = selected.get(&selection.content_id) {
            if !requirement_matches(&selection.version, &release.version) {
                return Err(AppError::coded_with(
                    "content_disabled_selection_conflict",
                    [
                        ("contentId", selection.content_id.clone()),
                        ("resolvedVersion", release.version.clone()),
                    ],
                ));
            }
            debug_assert!(*enabled);
            continue;
        }

        let candidates = candidate_pool(
            &catalog,
            &selection.content_id,
            std::slice::from_ref(&selection.version),
            &request.runtime,
        )
        .map_err(ResolutionFailure::into_error)?;
        let mut chosen = None;
        for candidate in candidates {
            let target_key = collision_key(Path::new(&candidate.artifact.relative_target))?;
            if !occupied_targets.contains_key(&target_key) {
                chosen = Some((candidate.clone(), target_key));
                break;
            }
        }
        let (release, target_key) = chosen.ok_or_else(|| {
            AppError::coded_with(
                "content_target_conflict",
                [("contentId", selection.content_id.clone())],
            )
        })?;
        occupied_targets.insert(target_key, release.content_id.clone());
        selected.insert(selection.content_id.clone(), (release, false));
    }

    let requested = canonical_selections(&request.requested);
    let mut items = Vec::with_capacity(selected.len());
    for (content_id, (release, enabled)) in &selected {
        let mut dependencies = Vec::new();
        for dependency in &release.dependencies {
            let activated = *enabled
                && (dependency.kind == ContentDependencyKind::Required
                    || (request.include_optional_dependencies
                        && dependency.kind == ContentDependencyKind::Optional));
            let resolved_version = if activated {
                let (resolved, resolved_enabled) = selected
                    .get(&dependency.content_id)
                    .ok_or_else(|| AppError::coded("content_lock_dependency_missing"))?;
                if !resolved_enabled || !requirement_matches(&dependency.version, &resolved.version)
                {
                    return Err(AppError::coded("content_lock_dependency_invalid"));
                }
                Some(resolved.version.clone())
            } else {
                None
            };
            let mut version_requirement = dependency.version.clone();
            normalize_requirement(&mut version_requirement);
            dependencies.push(ResolvedDependencyV1 {
                content_id: dependency.content_id.clone(),
                kind: dependency.kind,
                version_requirement,
                resolved_version,
            });
        }
        dependencies.sort_by(resolved_dependency_order);
        items.push(ResolvedContentItemV1 {
            content_id: content_id.clone(),
            version: release.version.clone(),
            kind: release.kind,
            enabled: *enabled,
            source: release.source.clone(),
            relative_target: release.artifact.relative_target.clone(),
            sha256: release.artifact.sha256.clone(),
            size_bytes: release.artifact.size_bytes,
            dependencies,
        });
    }

    let mut lock = ResolvedContentLockV1 {
        format: CONTENT_LOCK_FORMAT.into(),
        format_version: CONTENT_LOCK_FORMAT_VERSION,
        runtime: request.runtime.clone(),
        include_optional_dependencies: request.include_optional_dependencies,
        requested,
        items,
        pack_members: Vec::new(),
        overrides: Vec::new(),
        resolution_sha256: String::new(),
    };
    lock.resolution_sha256 = content_lock_sha256(&lock)?;
    validate_resolved_content_lock(&lock)?;
    Ok(lock)
}

pub fn validate_resolved_content_lock(lock: &ResolvedContentLockV1) -> AppResult<()> {
    validate_lock_payload_fields(lock)?;
    validate_sha256(&lock.resolution_sha256, "content_lock_sha256_invalid")?;
    let actual = content_lock_sha256(lock)?;
    if actual != lock.resolution_sha256 {
        return Err(AppError::coded_with(
            "content_lock_hash_mismatch",
            [
                ("expectedSha256", lock.resolution_sha256.clone()),
                ("actualSha256", actual),
            ],
        ));
    }
    Ok(())
}

pub fn canonical_content_lock_payload(lock: &ResolvedContentLockV1) -> AppResult<Vec<u8>> {
    validate_lock_payload_fields(lock)?;
    let mut output = Vec::new();
    append_text(&mut output, CONTENT_LOCK_FORMAT);
    append_text(&mut output, &lock.format);
    output.extend_from_slice(&lock.format_version.to_be_bytes());
    append_runtime(&mut output, &lock.runtime);
    append_bool(&mut output, lock.include_optional_dependencies);
    append_len(&mut output, lock.requested.len());
    for selection in &lock.requested {
        append_text(&mut output, &selection.content_id);
        append_requirement(&mut output, &selection.version);
        append_bool(&mut output, selection.enabled);
    }
    append_len(&mut output, lock.items.len());
    for item in &lock.items {
        append_text(&mut output, &item.content_id);
        append_text(&mut output, &item.version);
        append_text(&mut output, item.kind.as_str());
        append_bool(&mut output, item.enabled);
        append_source(&mut output, item.source.as_ref());
        append_text(&mut output, &item.relative_target);
        append_text(&mut output, &item.sha256);
        output.extend_from_slice(&item.size_bytes.to_be_bytes());
        append_len(&mut output, item.dependencies.len());
        for dependency in &item.dependencies {
            append_text(&mut output, &dependency.content_id);
            append_text(&mut output, dependency.kind.as_str());
            append_requirement(&mut output, &dependency.version_requirement);
            append_optional_text(&mut output, dependency.resolved_version.as_deref());
        }
    }
    append_len(&mut output, lock.pack_members.len());
    for member in &lock.pack_members {
        append_text(&mut output, &member.pack_content_id);
        append_text(&mut output, &member.content_id);
        append_text(&mut output, &member.version);
        append_bool(&mut output, member.enabled_by_default);
        append_bool(&mut output, member.owns_selection);
    }
    append_len(&mut output, lock.overrides.len());
    for override_file in &lock.overrides {
        append_text(&mut output, &override_file.pack_content_id);
        append_text(&mut output, &override_file.relative_target);
        append_text(&mut output, &override_file.sha256);
        output.extend_from_slice(&override_file.size_bytes.to_be_bytes());
    }
    Ok(output)
}

pub fn content_lock_sha256(lock: &ResolvedContentLockV1) -> AppResult<String> {
    Ok(hex::encode(Sha256::digest(canonical_content_lock_payload(
        lock,
    )?)))
}

pub fn validate_registered_content_target(
    registry: &PathRegistry,
    root_id: &str,
    item: &ResolvedContentItemV1,
) -> AppResult<SecurePath> {
    validate_resolved_item(item)?;
    registry.resolve(root_id, &item.relative_target)
}

fn build_catalog(
    releases: &[ContentReleaseV1],
) -> AppResult<BTreeMap<String, Vec<ContentReleaseV1>>> {
    if releases.len() > MAX_CONTENT_RELEASES {
        return Err(AppError::coded("content_catalog_size_invalid"));
    }
    let mut identities = BTreeMap::<(String, String), Vec<u8>>::new();
    let mut kinds = BTreeMap::<String, ContentKind>::new();
    let mut catalog = BTreeMap::<String, Vec<ContentReleaseV1>>::new();
    for release in releases {
        validate_content_release(release)?;
        match kinds.insert(release.content_id.clone(), release.kind) {
            Some(kind) if kind != release.kind => {
                return Err(AppError::coded_with(
                    "content_catalog_kind_conflict",
                    [("contentId", release.content_id.clone())],
                ));
            }
            _ => {}
        }
        let identity = (release.content_id.clone(), release.version.clone());
        let canonical = canonical_release_bytes(release)?;
        if let Some(existing) = identities.get(&identity) {
            if existing != &canonical {
                return Err(AppError::coded_with(
                    "content_catalog_release_conflict",
                    [
                        ("contentId", release.content_id.clone()),
                        ("version", release.version.clone()),
                    ],
                ));
            }
            continue;
        }
        identities.insert(identity, canonical);
        catalog
            .entry(release.content_id.clone())
            .or_default()
            .push(normalize_release(release));
    }
    for candidates in catalog.values_mut() {
        candidates.sort_by(|left, right| {
            compare_versions(&right.version, &left.version)
                .then_with(|| left.artifact.sha256.cmp(&right.artifact.sha256))
        });
    }
    Ok(catalog)
}

#[derive(Debug, Clone)]
enum ResolutionFailure {
    Missing(String),
    Incompatible(String),
    Version(String),
    Conflict { left: String, right: String },
    Target { left: String, right: String },
    Limit,
}

impl ResolutionFailure {
    fn into_error(self) -> AppError {
        match self {
            Self::Missing(content_id) => {
                AppError::coded_with("content_dependency_missing", [("contentId", content_id)])
            }
            Self::Incompatible(content_id) => AppError::coded_with(
                "content_compatibility_unsatisfied",
                [("contentId", content_id)],
            ),
            Self::Version(content_id) => {
                AppError::coded_with("content_version_unsatisfied", [("contentId", content_id)])
            }
            Self::Conflict { left, right } => AppError::coded_with(
                "content_conflict",
                [("contentId", left), ("conflictingContentId", right)],
            ),
            Self::Target { left, right } => AppError::coded_with(
                "content_target_conflict",
                [("contentId", left), ("conflictingContentId", right)],
            ),
            Self::Limit => AppError::coded("content_resolution_limit_exceeded"),
        }
    }
}

struct ResolutionBudget {
    remaining_steps: usize,
}

struct ResolutionInputs<'a> {
    catalog: &'a BTreeMap<String, Vec<ContentReleaseV1>>,
    runtime: &'a ContentTargetRuntime,
    include_optional: bool,
    selection_requirements: &'a BTreeMap<String, ContentVersionRequirement>,
}

fn solve_active(
    inputs: &ResolutionInputs<'_>,
    budget: &mut ResolutionBudget,
    assignments: BTreeMap<String, ContentReleaseV1>,
    constraints: BTreeMap<String, Vec<ContentVersionRequirement>>,
    depth: usize,
) -> Result<BTreeMap<String, ContentReleaseV1>, ResolutionFailure> {
    if depth > MAX_RESOLUTION_DEPTH
        || assignments.len() > MAX_CONTENT_SELECTIONS
        || constraints.len() > MAX_CONTENT_SELECTIONS
        || budget.remaining_steps == 0
    {
        return Err(ResolutionFailure::Limit);
    }
    budget.remaining_steps -= 1;
    for (content_id, release) in &assignments {
        if constraints.get(content_id).is_some_and(|requirements| {
            requirements
                .iter()
                .any(|requirement| !requirement_matches(requirement, &release.version))
        }) {
            return Err(ResolutionFailure::Version(content_id.clone()));
        }
    }
    detect_assignment_conflict(&assignments)?;

    let Some(content_id) = constraints
        .keys()
        .find(|content_id| !assignments.contains_key(*content_id))
        .cloned()
    else {
        return Ok(assignments);
    };
    let candidates = candidate_pool(
        inputs.catalog,
        &content_id,
        constraints.get(&content_id).map_or(&[], Vec::as_slice),
        inputs.runtime,
    )?;

    let mut first_failure = None;
    for candidate in candidates {
        let mut next_assignments = assignments.clone();
        next_assignments.insert(content_id.clone(), candidate.clone());
        let mut next_constraints = constraints.clone();
        for dependency in &candidate.dependencies {
            let activated = dependency.kind == ContentDependencyKind::Required
                || (inputs.include_optional && dependency.kind == ContentDependencyKind::Optional);
            if !activated {
                continue;
            }
            let entry = next_constraints
                .entry(dependency.content_id.clone())
                .or_default();
            if let Some(selection_requirement) =
                inputs.selection_requirements.get(&dependency.content_id)
            {
                if !entry.contains(selection_requirement) {
                    entry.push(selection_requirement.clone());
                }
            }
            entry.push(dependency.version.clone());
        }
        match solve_active(
            inputs,
            budget,
            next_assignments,
            next_constraints,
            depth + 1,
        ) {
            Ok(resolved) => return Ok(resolved),
            Err(failure) => {
                if first_failure.is_none() {
                    first_failure = Some(failure);
                }
            }
        }
    }
    Err(first_failure.unwrap_or(ResolutionFailure::Version(content_id)))
}

fn candidate_pool<'a>(
    catalog: &'a BTreeMap<String, Vec<ContentReleaseV1>>,
    content_id: &str,
    requirements: &[ContentVersionRequirement],
    runtime: &ContentTargetRuntime,
) -> Result<Vec<&'a ContentReleaseV1>, ResolutionFailure> {
    let candidates = catalog
        .get(content_id)
        .ok_or_else(|| ResolutionFailure::Missing(content_id.to_string()))?;
    let compatible = candidates
        .iter()
        .filter(|release| release_supports_runtime(release, runtime))
        .collect::<Vec<_>>();
    if compatible.is_empty() {
        return Err(ResolutionFailure::Incompatible(content_id.to_string()));
    }
    let matching = compatible
        .into_iter()
        .filter(|release| {
            requirements
                .iter()
                .all(|requirement| requirement_matches(requirement, &release.version))
        })
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Err(ResolutionFailure::Version(content_id.to_string()));
    }
    Ok(matching)
}

fn detect_assignment_conflict(
    assignments: &BTreeMap<String, ContentReleaseV1>,
) -> Result<(), ResolutionFailure> {
    let values = assignments.values().collect::<Vec<_>>();
    for (index, left) in values.iter().enumerate() {
        for right in values.iter().skip(index + 1) {
            let incompatible = release_conflicts(left, right) || release_conflicts(right, left);
            if incompatible {
                return Err(ResolutionFailure::Conflict {
                    left: left.content_id.clone(),
                    right: right.content_id.clone(),
                });
            }
            let left_target =
                collision_key(Path::new(&left.artifact.relative_target)).map_err(|_| {
                    ResolutionFailure::Target {
                        left: left.content_id.clone(),
                        right: right.content_id.clone(),
                    }
                })?;
            let right_target =
                collision_key(Path::new(&right.artifact.relative_target)).map_err(|_| {
                    ResolutionFailure::Target {
                        left: left.content_id.clone(),
                        right: right.content_id.clone(),
                    }
                })?;
            if left_target == right_target {
                return Err(ResolutionFailure::Target {
                    left: left.content_id.clone(),
                    right: right.content_id.clone(),
                });
            }
        }
    }
    Ok(())
}

fn release_conflicts(left: &ContentReleaseV1, right: &ContentReleaseV1) -> bool {
    left.dependencies.iter().any(|dependency| {
        dependency.kind == ContentDependencyKind::Incompatible
            && dependency.content_id == right.content_id
            && requirement_matches(&dependency.version, &right.version)
    })
}

fn release_supports_runtime(release: &ContentReleaseV1, runtime: &ContentTargetRuntime) -> bool {
    if !release
        .compatibility
        .minecraft_versions
        .iter()
        .any(|version| version == &runtime.minecraft_version)
    {
        return false;
    }
    if release.compatibility.loaders.is_empty() {
        return true;
    }
    release.compatibility.loaders.iter().any(|loader| {
        loader.kind == runtime.loader.kind
            && (loader.loader_versions.is_empty()
                || runtime
                    .loader
                    .loader_version
                    .as_ref()
                    .is_some_and(|version| {
                        loader
                            .loader_versions
                            .iter()
                            .any(|candidate| candidate == version)
                    }))
    })
}

fn requirement_matches(requirement: &ContentVersionRequirement, version: &str) -> bool {
    match requirement {
        ContentVersionRequirement::Any => true,
        ContentVersionRequirement::Exact { version: expected } => expected == version,
        ContentVersionRequirement::OneOf { versions } => {
            versions.iter().any(|expected| expected == version)
        }
    }
}

fn validate_target_runtime(runtime: &ContentTargetRuntime) -> AppResult<()> {
    validate_version_token(
        &runtime.minecraft_version,
        MAX_MINECRAFT_VERSION_BYTES,
        "content_minecraft_version_invalid",
    )?;
    match (
        runtime.loader.kind,
        runtime.loader.loader_version.as_deref(),
    ) {
        (LoaderKind::Vanilla, None) => Ok(()),
        (LoaderKind::Vanilla, Some(_)) => {
            Err(AppError::coded("content_vanilla_loader_version_forbidden"))
        }
        (_, Some(version)) => validate_version(version, "content_loader_version_invalid"),
        (_, None) => Err(AppError::coded("content_loader_version_required")),
    }
}

fn validate_compatibility(
    kind: ContentKind,
    compatibility: &ContentCompatibility,
) -> AppResult<()> {
    if compatibility.minecraft_versions.is_empty()
        || compatibility.minecraft_versions.len() > MAX_COMPATIBILITY_VALUES
    {
        return Err(AppError::coded("content_minecraft_compatibility_invalid"));
    }
    let mut minecraft = BTreeSet::new();
    for version in &compatibility.minecraft_versions {
        validate_version_token(
            version,
            MAX_MINECRAFT_VERSION_BYTES,
            "content_minecraft_version_invalid",
        )?;
        if !minecraft.insert(version) {
            return Err(AppError::coded("content_minecraft_compatibility_duplicate"));
        }
    }
    if compatibility.loaders.len() > 3 {
        return Err(AppError::coded("content_loader_compatibility_invalid"));
    }
    if compatibility.loaders.is_empty() && matches!(kind, ContentKind::Mod | ContentKind::Modpack) {
        return Err(AppError::coded("content_loader_compatibility_required"));
    }
    let mut loader_kinds = BTreeSet::new();
    for loader in &compatibility.loaders {
        if !loader_kinds.insert(loader_rank(loader.kind)) {
            return Err(AppError::coded("content_loader_compatibility_duplicate"));
        }
        if kind == ContentKind::Mod && loader.kind == LoaderKind::Vanilla {
            return Err(AppError::coded("content_mod_vanilla_incompatible"));
        }
        if loader.loader_versions.len() > MAX_COMPATIBILITY_VALUES {
            return Err(AppError::coded("content_loader_compatibility_invalid"));
        }
        if loader.kind == LoaderKind::Vanilla && !loader.loader_versions.is_empty() {
            return Err(AppError::coded("content_vanilla_loader_version_forbidden"));
        }
        let mut versions = BTreeSet::new();
        for version in &loader.loader_versions {
            validate_version(version, "content_loader_version_invalid")?;
            if !versions.insert(version) {
                return Err(AppError::coded("content_loader_compatibility_duplicate"));
            }
        }
    }
    Ok(())
}

fn validate_version_requirement(requirement: &ContentVersionRequirement) -> AppResult<()> {
    match requirement {
        ContentVersionRequirement::Any => Ok(()),
        ContentVersionRequirement::Exact { version } => {
            validate_version(version, "content_version_requirement_invalid")
        }
        ContentVersionRequirement::OneOf { versions } => {
            if versions.is_empty() || versions.len() > MAX_COMPATIBILITY_VALUES {
                return Err(AppError::coded("content_version_requirement_invalid"));
            }
            let mut unique = BTreeSet::new();
            for version in versions {
                validate_version(version, "content_version_requirement_invalid")?;
                if !unique.insert(version) {
                    return Err(AppError::coded("content_version_requirement_duplicate"));
                }
            }
            Ok(())
        }
    }
}

fn validate_artifact(kind: ContentKind, artifact: &ContentArtifactV1) -> AppResult<()> {
    if artifact.size_bytes == 0 || artifact.size_bytes > MAX_CONTENT_ARTIFACT_BYTES {
        return Err(AppError::coded("content_artifact_size_invalid"));
    }
    validate_sha256(&artifact.sha256, "content_artifact_sha256_invalid")?;
    let normalized = normalize_relative_path(Path::new(&artifact.relative_target))?;
    if artifact.relative_target.contains('\\') {
        return Err(AppError::coded("content_target_separator_noncanonical"));
    }
    let canonical = normalized
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if canonical != artifact.relative_target {
        return Err(AppError::coded("content_target_noncanonical"));
    }
    let (prefix, suffixes): (&str, &[&str]) = match kind {
        ContentKind::Mod => ("mods/", &[".jar"]),
        ContentKind::Modpack => ("modpacks/", &[".mrpack", ".zip"]),
        ContentKind::ShaderPack => ("shaderpacks/", &[".zip"]),
        ContentKind::ResourcePack => ("resourcepacks/", &[".zip"]),
    };
    let remainder = artifact
        .relative_target
        .strip_prefix(prefix)
        .filter(|value| !value.is_empty() && !value.contains('/'));
    if remainder.is_none()
        || !suffixes
            .iter()
            .any(|suffix| artifact.relative_target.ends_with(suffix))
    {
        return Err(AppError::coded_with(
            "content_target_kind_invalid",
            [
                ("kind", kind.as_str().to_string()),
                ("relativeTarget", artifact.relative_target.clone()),
            ],
        ));
    }
    Ok(())
}

pub fn validate_content_override_target(relative_target: &str) -> AppResult<()> {
    if relative_target.is_empty()
        || relative_target.len() > 1_024
        || relative_target.contains('\\')
        || relative_target.contains("//")
    {
        return Err(AppError::coded("content_override_target_invalid"));
    }
    let normalized = normalize_relative_path(Path::new(relative_target))?;
    if normalized.components().count() > 32
        || normalized
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
            != relative_target
    {
        return Err(AppError::coded("content_override_target_invalid"));
    }
    let first = normalized
        .iter()
        .next()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::coded("content_override_target_invalid"))?
        .to_ascii_lowercase();
    if matches!(
        first.as_str(),
        ".s9lab"
            | "saves"
            | "logs"
            | "crash-reports"
            | "screenshots"
            | "versions"
            | "libraries"
            | "assets"
            | "runtime"
            | "natives"
            | "backups"
            | "exports"
    ) {
        return Err(AppError::coded("content_override_target_forbidden"));
    }
    let file_name = normalized
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::coded("content_override_target_invalid"))?
        .to_ascii_lowercase();
    if matches!(
        file_name.as_str(),
        "servers.dat"
            | "usercache.json"
            | "usernamecache.json"
            | "launcher_accounts.json"
            | "launcher_profiles.json"
    ) || [
        ".exe", ".com", ".bat", ".cmd", ".ps1", ".msi", ".msp", ".reg", ".scr", ".lnk", ".url",
    ]
    .iter()
    .any(|suffix| file_name.ends_with(suffix))
    {
        return Err(AppError::coded("content_override_target_forbidden"));
    }
    Ok(())
}

fn validate_resolved_override(override_file: &ResolvedContentOverrideV1) -> AppResult<()> {
    validate_content_id(&override_file.pack_content_id)?;
    validate_content_override_target(&override_file.relative_target)?;
    if override_file.size_bytes > MAX_CONTENT_ARTIFACT_BYTES {
        return Err(AppError::coded("content_override_size_invalid"));
    }
    validate_sha256(&override_file.sha256, "content_override_sha256_invalid")
}

fn validate_source(source: &ContentSourceV1, relative_target: &str) -> AppResult<()> {
    match source {
        ContentSourceV1::Modrinth {
            project_id,
            version_id,
            file_name,
        } => {
            validate_provider_id(project_id)?;
            validate_provider_id(version_id)?;
            validate_source_file_name(file_name)?;
        }
        ContentSourceV1::Local { file_name } => validate_source_file_name(file_name)?,
    }
    let target_name = Path::new(relative_target)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::coded("content_source_file_name_invalid"))?;
    if source.file_name() != target_name {
        return Err(AppError::coded("content_source_file_name_mismatch"));
    }
    Ok(())
}

fn validate_source_file_name(value: &str) -> AppResult<()> {
    let normalized = normalize_relative_path(Path::new(value))?;
    if value.contains(['/', '\\'])
        || normalized.components().count() != 1
        || normalized.as_os_str().to_string_lossy() != value
    {
        return Err(AppError::coded("content_source_file_name_invalid"));
    }
    Ok(())
}

fn validate_provider_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_PROVIDER_ID_BYTES
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(AppError::coded("content_source_identity_invalid"));
    }
    Ok(())
}

fn validate_content_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > MAX_CONTENT_ID_BYTES
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
        return Err(AppError::coded("content_id_invalid"));
    }
    Ok(())
}

fn validate_version(value: &str, code: &'static str) -> AppResult<()> {
    validate_version_token(value, MAX_VERSION_BYTES, code)
}

fn validate_version_token(value: &str, max_bytes: usize, code: &'static str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > max_bytes
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        || !value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        || !value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
    {
        return Err(AppError::coded(code));
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

fn normalize_release(release: &ContentReleaseV1) -> ContentReleaseV1 {
    let mut normalized = release.clone();
    normalized.compatibility.minecraft_versions.sort();
    for loader in &mut normalized.compatibility.loaders {
        loader.loader_versions.sort();
    }
    normalized
        .compatibility
        .loaders
        .sort_by_key(|loader| loader_rank(loader.kind));
    for dependency in &mut normalized.dependencies {
        normalize_requirement(&mut dependency.version);
    }
    normalized.dependencies.sort_by(dependency_order);
    normalized
}

fn canonical_release_bytes(release: &ContentReleaseV1) -> AppResult<Vec<u8>> {
    serde_json::to_vec(&normalize_release(release)).map_err(Into::into)
}

fn canonical_selections(selections: &[ContentSelection]) -> Vec<ContentSelection> {
    let mut normalized = selections.to_vec();
    for selection in &mut normalized {
        normalize_requirement(&mut selection.version);
    }
    normalized.sort_by(|left, right| left.content_id.cmp(&right.content_id));
    normalized
}

fn normalize_requirement(requirement: &mut ContentVersionRequirement) {
    if let ContentVersionRequirement::OneOf { versions } = requirement {
        versions.sort();
    }
}

fn dependency_order(left: &ContentDependency, right: &ContentDependency) -> Ordering {
    left.content_id
        .cmp(&right.content_id)
        .then_with(|| left.kind.cmp(&right.kind))
        .then_with(|| {
            requirement_sort_key(&left.version).cmp(&requirement_sort_key(&right.version))
        })
}

fn resolved_dependency_order(
    left: &ResolvedDependencyV1,
    right: &ResolvedDependencyV1,
) -> Ordering {
    left.content_id
        .cmp(&right.content_id)
        .then_with(|| left.kind.cmp(&right.kind))
        .then_with(|| {
            requirement_sort_key(&left.version_requirement)
                .cmp(&requirement_sort_key(&right.version_requirement))
        })
        .then_with(|| left.resolved_version.cmp(&right.resolved_version))
}

fn requirement_sort_key(requirement: &ContentVersionRequirement) -> String {
    match requirement {
        ContentVersionRequirement::Any => "0".into(),
        ContentVersionRequirement::Exact { version } => format!("1:{version}"),
        ContentVersionRequirement::OneOf { versions } => format!("2:{}", versions.join("\0")),
    }
}

fn validate_lock_payload_fields(lock: &ResolvedContentLockV1) -> AppResult<()> {
    if lock.format != CONTENT_LOCK_FORMAT || lock.format_version != CONTENT_LOCK_FORMAT_VERSION {
        return Err(AppError::coded("content_lock_format_unsupported"));
    }
    validate_target_runtime(&lock.runtime)?;
    if lock.requested.len() > MAX_CONTENT_SELECTIONS
        || lock.items.len() > MAX_CONTENT_SELECTIONS
        || lock.pack_members.len() > MAX_CONTENT_SELECTIONS
        || lock.overrides.len() > MAX_CONTENT_OVERRIDES
    {
        return Err(AppError::coded("content_lock_item_count_invalid"));
    }

    let mut previous_request = None;
    let mut requested = BTreeMap::new();
    for selection in &lock.requested {
        validate_content_id(&selection.content_id)?;
        validate_version_requirement(&selection.version)?;
        validate_canonical_requirement(&selection.version)?;
        if previous_request
            .as_ref()
            .is_some_and(|previous: &String| previous >= &selection.content_id)
        {
            return Err(AppError::coded("content_lock_request_order_invalid"));
        }
        previous_request = Some(selection.content_id.clone());
        requested.insert(selection.content_id.clone(), selection);
    }

    let mut previous_item = None;
    let mut items = BTreeMap::new();
    let mut target_keys = BTreeMap::new();
    let mut total_content_bytes = 0u64;
    for item in &lock.items {
        validate_resolved_item(item)?;
        if previous_item
            .as_ref()
            .is_some_and(|previous: &String| previous >= &item.content_id)
        {
            return Err(AppError::coded("content_lock_item_order_invalid"));
        }
        previous_item = Some(item.content_id.clone());
        let key = collision_key(Path::new(&item.relative_target))?;
        if let Some(existing) = target_keys.insert(key, item.content_id.clone()) {
            return Err(AppError::coded_with(
                "content_lock_target_collision",
                [
                    ("contentId", item.content_id.clone()),
                    ("conflictingContentId", existing),
                ],
            ));
        }
        total_content_bytes = total_content_bytes
            .checked_add(item.size_bytes)
            .ok_or_else(|| AppError::coded("content_lock_size_overflow"))?;
        items.insert(item.content_id.clone(), item);
    }
    if total_content_bytes > MAX_CONTENT_TOTAL_BYTES {
        return Err(AppError::coded("content_lock_total_size_invalid"));
    }

    let mut previous_member = None;
    let mut owned_member_selections = BTreeMap::new();
    for member in &lock.pack_members {
        validate_pack_member(member, &items)?;
        let key = (&member.pack_content_id, &member.content_id);
        if previous_member
            .as_ref()
            .is_some_and(|previous: &(String, String)| {
                (previous.0.as_str(), previous.1.as_str()) >= (key.0.as_str(), key.1.as_str())
            })
        {
            return Err(AppError::coded("content_pack_member_order_invalid"));
        }
        previous_member = Some((member.pack_content_id.clone(), member.content_id.clone()));
        let pack = items
            .get(&member.pack_content_id)
            .ok_or_else(|| AppError::coded("content_pack_member_invalid"))?;
        let child = items
            .get(&member.content_id)
            .ok_or_else(|| AppError::coded("content_pack_member_invalid"))?;
        let pack_selection = requested
            .get(&member.pack_content_id)
            .ok_or_else(|| AppError::coded("content_pack_member_selection_missing"))?;
        let child_selection = requested
            .get(&member.content_id)
            .ok_or_else(|| AppError::coded("content_pack_member_selection_missing"))?;
        if !requirement_matches(&pack_selection.version, &pack.version)
            || !requirement_matches(&child_selection.version, &member.version)
        {
            return Err(AppError::coded("content_pack_member_selection_invalid"));
        }
        if pack.enabled && member.enabled_by_default && !child.enabled {
            return Err(AppError::coded("content_pack_member_enabled_state_invalid"));
        }
        if member.owns_selection
            && owned_member_selections
                .insert(member.content_id.clone(), member.pack_content_id.clone())
                .is_some()
        {
            return Err(AppError::coded("content_pack_member_owner_duplicate"));
        }
    }

    let mut previous_override_target = None;
    let mut total_override_bytes = 0u64;
    for override_file in &lock.overrides {
        validate_resolved_override(override_file)?;
        if previous_override_target
            .as_ref()
            .is_some_and(|previous: &String| previous >= &override_file.relative_target)
        {
            return Err(AppError::coded("content_override_order_invalid"));
        }
        previous_override_target = Some(override_file.relative_target.clone());
        let pack = items
            .get(&override_file.pack_content_id)
            .filter(|item| item.kind == ContentKind::Modpack)
            .ok_or_else(|| AppError::coded("content_override_pack_missing"))?;
        let key = collision_key(Path::new(&override_file.relative_target))?;
        if let Some(existing) = target_keys.insert(key, pack.content_id.clone()) {
            return Err(AppError::coded_with(
                "content_lock_target_collision",
                [
                    ("contentId", pack.content_id.clone()),
                    ("conflictingContentId", existing),
                ],
            ));
        }
        total_override_bytes = total_override_bytes
            .checked_add(override_file.size_bytes)
            .ok_or_else(|| AppError::coded("content_override_size_overflow"))?;
        if total_override_bytes > MAX_CONTENT_OVERRIDE_TOTAL_BYTES {
            return Err(AppError::coded("content_override_total_size_invalid"));
        }
        total_content_bytes = total_content_bytes
            .checked_add(override_file.size_bytes)
            .ok_or_else(|| AppError::coded("content_lock_size_overflow"))?;
        if total_content_bytes > MAX_CONTENT_TOTAL_BYTES {
            return Err(AppError::coded("content_lock_total_size_invalid"));
        }
    }
    validate_no_target_ancestor_conflicts(&target_keys)?;

    for (content_id, selection) in &requested {
        let item = items.get(content_id).ok_or_else(|| {
            AppError::coded_with(
                "content_lock_requested_item_missing",
                [("contentId", content_id.clone())],
            )
        })?;
        if !requirement_matches(&selection.version, &item.version)
            || (selection.enabled && !item.enabled)
        {
            return Err(AppError::coded_with(
                "content_lock_requested_item_invalid",
                [("contentId", content_id.clone())],
            ));
        }
    }

    for item in items.values() {
        for dependency in &item.dependencies {
            let must_resolve = item.enabled
                && (dependency.kind == ContentDependencyKind::Required
                    || (dependency.kind == ContentDependencyKind::Optional
                        && lock.include_optional_dependencies));
            if must_resolve {
                let resolved_version = dependency.resolved_version.as_deref().ok_or_else(|| {
                    AppError::coded_with(
                        "content_lock_dependency_unresolved",
                        [("dependencyId", dependency.content_id.clone())],
                    )
                })?;
                let target = items.get(&dependency.content_id).ok_or_else(|| {
                    AppError::coded_with(
                        "content_lock_dependency_missing",
                        [("dependencyId", dependency.content_id.clone())],
                    )
                })?;
                if !target.enabled
                    || target.version != resolved_version
                    || !requirement_matches(&dependency.version_requirement, &target.version)
                {
                    return Err(AppError::coded("content_lock_dependency_invalid"));
                }
            } else if dependency.resolved_version.is_some() {
                return Err(AppError::coded(
                    "content_lock_dependency_resolution_unexpected",
                ));
            }

            if item.enabled && dependency.kind == ContentDependencyKind::Incompatible {
                if let Some(target) = items
                    .get(&dependency.content_id)
                    .filter(|item| item.enabled)
                {
                    if requirement_matches(&dependency.version_requirement, &target.version) {
                        return Err(AppError::coded_with(
                            "content_lock_conflict",
                            [
                                ("contentId", item.content_id.clone()),
                                ("conflictingContentId", target.content_id.clone()),
                            ],
                        ));
                    }
                }
            }
        }
    }

    let mut reachable = BTreeSet::new();
    let mut pending = lock
        .requested
        .iter()
        .filter(|selection| selection.enabled)
        .map(|selection| selection.content_id.clone())
        .collect::<Vec<_>>();
    while let Some(content_id) = pending.pop() {
        if !reachable.insert(content_id.clone()) {
            continue;
        }
        let item = items
            .get(&content_id)
            .ok_or_else(|| AppError::coded("content_lock_requested_item_missing"))?;
        pending.extend(
            item.dependencies
                .iter()
                .filter(|dependency| dependency.resolved_version.is_some())
                .map(|dependency| dependency.content_id.clone()),
        );
    }
    for item in items.values() {
        if item.enabled && !reachable.contains(&item.content_id) {
            return Err(AppError::coded_with(
                "content_lock_enabled_item_unreachable",
                [("contentId", item.content_id.clone())],
            ));
        }
        if !item.enabled
            && requested
                .get(&item.content_id)
                .is_none_or(|selection| selection.enabled)
        {
            return Err(AppError::coded_with(
                "content_lock_disabled_item_unrequested",
                [("contentId", item.content_id.clone())],
            ));
        }
    }
    Ok(())
}

fn validate_pack_member(
    member: &ResolvedContentPackMemberV1,
    items: &BTreeMap<String, &ResolvedContentItemV1>,
) -> AppResult<()> {
    validate_content_id(&member.pack_content_id)?;
    validate_content_id(&member.content_id)?;
    validate_version(&member.version, "content_pack_member_version_invalid")?;
    if member.pack_content_id == member.content_id
        || items
            .get(&member.pack_content_id)
            .is_none_or(|item| item.kind != ContentKind::Modpack)
        || items
            .get(&member.content_id)
            .is_none_or(|item| item.version != member.version)
    {
        return Err(AppError::coded("content_pack_member_invalid"));
    }
    Ok(())
}

fn validate_no_target_ancestor_conflicts(targets: &BTreeMap<String, String>) -> AppResult<()> {
    for (target, content_id) in targets {
        for (separator, _) in target.match_indices('/') {
            let ancestor = &target[..separator];
            if let Some(ancestor_content_id) = targets.get(ancestor) {
                return Err(AppError::coded_with(
                    "content_lock_target_ancestor_collision",
                    [
                        ("contentId", content_id.clone()),
                        ("conflictingContentId", ancestor_content_id.clone()),
                        ("normalizedPath", target.clone()),
                    ],
                ));
            }
        }
    }
    Ok(())
}

fn validate_resolved_item(item: &ResolvedContentItemV1) -> AppResult<()> {
    validate_content_id(&item.content_id)?;
    validate_version(&item.version, "content_version_invalid")?;
    validate_artifact(
        item.kind,
        &ContentArtifactV1 {
            relative_target: item.relative_target.clone(),
            sha256: item.sha256.clone(),
            size_bytes: item.size_bytes,
        },
    )?;
    if let Some(source) = &item.source {
        validate_source(source, &item.relative_target)?;
    }
    let mut previous = None;
    let mut dependency_keys = BTreeSet::new();
    let mut activated_dependency_ids = BTreeSet::new();
    for dependency in &item.dependencies {
        validate_content_id(&dependency.content_id)?;
        validate_version_requirement(&dependency.version_requirement)?;
        validate_canonical_requirement(&dependency.version_requirement)?;
        if let Some(version) = &dependency.resolved_version {
            validate_version(version, "content_version_invalid")?;
        }
        if dependency.content_id == item.content_id
            || !dependency_keys.insert((dependency.content_id.clone(), dependency.kind))
        {
            return Err(AppError::coded("content_lock_dependency_duplicate"));
        }
        if dependency.kind != ContentDependencyKind::Incompatible
            && !activated_dependency_ids.insert(dependency.content_id.clone())
        {
            return Err(AppError::coded(
                "content_lock_dependency_activation_duplicate",
            ));
        }
        if previous
            .as_ref()
            .is_some_and(|prior: &ResolvedDependencyV1| {
                resolved_dependency_order(prior, dependency) != Ordering::Less
            })
        {
            return Err(AppError::coded("content_lock_dependency_order_invalid"));
        }
        previous = Some(dependency.clone());
    }
    Ok(())
}

fn validate_canonical_requirement(requirement: &ContentVersionRequirement) -> AppResult<()> {
    if let ContentVersionRequirement::OneOf { versions } = requirement {
        if versions
            .windows(2)
            .any(|pair| pair.first().zip(pair.get(1)).is_some_and(|(a, b)| a >= b))
        {
            return Err(AppError::coded(
                "content_lock_version_requirement_order_invalid",
            ));
        }
    }
    Ok(())
}

fn append_runtime(output: &mut Vec<u8>, runtime: &ContentTargetRuntime) {
    append_text(output, &runtime.minecraft_version);
    append_text(output, runtime.loader.kind.as_str());
    append_optional_text(output, runtime.loader.loader_version.as_deref());
}

fn append_requirement(output: &mut Vec<u8>, requirement: &ContentVersionRequirement) {
    match requirement {
        ContentVersionRequirement::Any => output.push(0),
        ContentVersionRequirement::Exact { version } => {
            output.push(1);
            append_text(output, version);
        }
        ContentVersionRequirement::OneOf { versions } => {
            output.push(2);
            append_len(output, versions.len());
            for version in versions {
                append_text(output, version);
            }
        }
    }
}

fn append_source(output: &mut Vec<u8>, source: Option<&ContentSourceV1>) {
    match source {
        None => output.push(0),
        Some(ContentSourceV1::Modrinth {
            project_id,
            version_id,
            file_name,
        }) => {
            output.push(1);
            append_text(output, project_id);
            append_text(output, version_id);
            append_text(output, file_name);
        }
        Some(ContentSourceV1::Local { file_name }) => {
            output.push(2);
            append_text(output, file_name);
        }
    }
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

fn append_bool(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}

fn append_len(output: &mut Vec<u8>, value: usize) {
    output.extend_from_slice(&(value as u64).to_be_bytes());
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut left_index = 0usize;
    let mut right_index = 0usize;
    while left_index < left_bytes.len() && right_index < right_bytes.len() {
        let left_digit = left_bytes[left_index].is_ascii_digit();
        let right_digit = right_bytes[right_index].is_ascii_digit();
        if left_digit && right_digit {
            let left_end = digit_end(left_bytes, left_index);
            let right_end = digit_end(right_bytes, right_index);
            let left_number = trim_zeroes(&left_bytes[left_index..left_end]);
            let right_number = trim_zeroes(&right_bytes[right_index..right_end]);
            let compared = left_number
                .len()
                .cmp(&right_number.len())
                .then_with(|| left_number.cmp(right_number))
                .then_with(|| {
                    (left_end - left_index)
                        .cmp(&(right_end - right_index))
                        .reverse()
                });
            if compared != Ordering::Equal {
                return compared;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }
        if left_digit != right_digit {
            return left_digit.cmp(&right_digit);
        }
        let compared = left_bytes[left_index]
            .to_ascii_lowercase()
            .cmp(&right_bytes[right_index].to_ascii_lowercase())
            .then_with(|| left_bytes[left_index].cmp(&right_bytes[right_index]));
        if compared != Ordering::Equal {
            return compared;
        }
        left_index += 1;
        right_index += 1;
    }
    match (
        left_index == left_bytes.len(),
        right_index == right_bytes.len(),
    ) {
        (true, true) => Ordering::Equal,
        (true, false) if right_bytes[right_index] == b'-' => Ordering::Greater,
        (false, true) if left_bytes[left_index] == b'-' => Ordering::Less,
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => unreachable!(),
    }
}

fn digit_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    end
}

fn trim_zeroes(mut value: &[u8]) -> &[u8] {
    while value.len() > 1 && value.first() == Some(&b'0') {
        value = &value[1..];
    }
    value
}

fn loader_rank(kind: LoaderKind) -> u8 {
    match kind {
        LoaderKind::Vanilla => 0,
        LoaderKind::Fabric => 1,
        LoaderKind::Neoforge => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::ContentLoaderCompatibility, runtime::LoaderSelection, security::RegisteredRoot,
    };
    use std::fs;

    fn runtime() -> ContentTargetRuntime {
        ContentTargetRuntime {
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: LoaderKind::Fabric,
                loader_version: Some("0.16.10".into()),
            },
        }
    }

    fn selection(content_id: &str) -> ContentSelection {
        ContentSelection {
            content_id: content_id.into(),
            version: ContentVersionRequirement::Any,
            enabled: true,
        }
    }

    fn release(content_id: &str, version: &str) -> ContentReleaseV1 {
        ContentReleaseV1 {
            format: CONTENT_RELEASE_FORMAT.into(),
            format_version: CONTENT_RELEASE_FORMAT_VERSION,
            content_id: content_id.into(),
            version: version.into(),
            kind: ContentKind::Mod,
            compatibility: ContentCompatibility {
                minecraft_versions: vec!["1.21.1".into()],
                loaders: vec![ContentLoaderCompatibility {
                    kind: LoaderKind::Fabric,
                    loader_versions: vec!["0.16.10".into()],
                }],
            },
            dependencies: vec![],
            source: Some(ContentSourceV1::Modrinth {
                project_id: format!("project-{content_id}"),
                version_id: format!("version-{version}"),
                file_name: format!("{content_id}.jar"),
            }),
            artifact: ContentArtifactV1 {
                relative_target: format!("mods/{content_id}.jar"),
                sha256: hex::encode(Sha256::digest(format!("{content_id}:{version}"))),
                size_bytes: 1024,
            },
        }
    }

    fn pack_release(content_id: &str, version: &str) -> ContentReleaseV1 {
        let mut release = release(content_id, version);
        let file_name = format!("{content_id}.mrpack");
        release.kind = ContentKind::Modpack;
        release.artifact.relative_target = format!("modpacks/{file_name}");
        release.source = Some(ContentSourceV1::Modrinth {
            project_id: format!("project-{content_id}"),
            version_id: format!("version-{version}"),
            file_name,
        });
        release
    }

    fn shared_pack_member_lock() -> ResolvedContentLockV1 {
        let mut lock = resolve_content(
            &request(vec![
                selection("pack-a"),
                selection("pack-b"),
                selection("shared-member"),
            ]),
            &[
                pack_release("pack-a", "1.0.0"),
                pack_release("pack-b", "1.0.0"),
                release("shared-member", "2.0.0"),
            ],
        )
        .expect("shared pack member lock");
        lock.pack_members = vec![
            ResolvedContentPackMemberV1 {
                pack_content_id: "pack-a".into(),
                content_id: "shared-member".into(),
                version: "2.0.0".into(),
                enabled_by_default: true,
                owns_selection: true,
            },
            ResolvedContentPackMemberV1 {
                pack_content_id: "pack-b".into(),
                content_id: "shared-member".into(),
                version: "2.0.0".into(),
                enabled_by_default: true,
                owns_selection: false,
            },
        ];
        lock.resolution_sha256 = content_lock_sha256(&lock).expect("shared lock hash");
        lock
    }

    fn dependency(
        content_id: &str,
        kind: ContentDependencyKind,
        version: ContentVersionRequirement,
    ) -> ContentDependency {
        ContentDependency {
            content_id: content_id.into(),
            kind,
            version,
        }
    }

    fn request(requested: Vec<ContentSelection>) -> ContentResolutionRequest {
        ContentResolutionRequest {
            runtime: runtime(),
            requested,
            include_optional_dependencies: false,
        }
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected content resolution failure")
            .descriptor()
            .code
    }

    #[test]
    fn resolution_is_reproducible_across_catalog_and_request_order() {
        let mut core = release("core", "1.10.0");
        core.dependencies.push(dependency(
            "library",
            ContentDependencyKind::Required,
            ContentVersionRequirement::Any,
        ));
        let library_old = release("library", "1.9.0");
        let library_new = release("library", "1.10.0");
        let cosmetic = release("cosmetic", "2.0.0");
        let first = resolve_content(
            &request(vec![selection("cosmetic"), selection("core")]),
            &[
                library_old.clone(),
                core.clone(),
                cosmetic.clone(),
                library_new.clone(),
            ],
        )
        .expect("first resolution");
        let second = resolve_content(
            &request(vec![selection("core"), selection("cosmetic")]),
            &[cosmetic, library_new, core, library_old],
        )
        .expect("second resolution");
        assert_eq!(first, second);
        assert_eq!(first.items[2].version, "1.10.0");
        assert_eq!(first.resolution_sha256.len(), 64);
    }

    #[test]
    fn empty_content_graph_has_a_stable_canonical_hash_vector() {
        let lock = resolve_content(&request(vec![]), &[]).expect("empty content lock");
        assert!(lock.items.is_empty());
        assert_eq!(
            lock.resolution_sha256,
            "f58a0c2c6a3434c4450cdd4d66a2127c8195f1232bd40d41d162a94bed28e932"
        );
    }

    #[test]
    fn resolver_backtracks_when_newest_release_has_unsatisfied_dependency() {
        let mut newest = release("root", "2.0.0");
        newest.dependencies.push(dependency(
            "missing",
            ContentDependencyKind::Required,
            ContentVersionRequirement::Any,
        ));
        let older = release("root", "1.0.0");
        let lock = resolve_content(&request(vec![selection("root")]), &[newest, older])
            .expect("older compatible fallback");
        assert_eq!(lock.items[0].version, "1.0.0");
    }

    #[test]
    fn dependency_depth_is_bounded_before_recursive_input_can_exhaust_the_stack() {
        let mut releases = Vec::new();
        for index in 0..=(MAX_RESOLUTION_DEPTH + 1) {
            let content_id = format!("node-{index:03}");
            let mut item = release(&content_id, "1.0.0");
            if index <= MAX_RESOLUTION_DEPTH {
                item.dependencies.push(dependency(
                    &format!("node-{:03}", index + 1),
                    ContentDependencyKind::Required,
                    ContentVersionRequirement::Any,
                ));
            }
            releases.push(item);
        }
        assert_eq!(
            error_code(resolve_content(
                &request(vec![selection("node-000")]),
                &releases
            )),
            "content_resolution_limit_exceeded"
        );
    }

    #[test]
    fn minecraft_loader_version_and_required_version_are_enforced() {
        let mut incompatible = release("root", "1.0.0");
        incompatible.compatibility.minecraft_versions = vec!["1.20.1".into()];
        assert_eq!(
            error_code(resolve_content(
                &request(vec![selection("root")]),
                &[incompatible]
            )),
            "content_compatibility_unsatisfied"
        );

        let mut exact = selection("root");
        exact.version = ContentVersionRequirement::Exact {
            version: "2.0.0".into(),
        };
        assert_eq!(
            error_code(resolve_content(
                &request(vec![exact]),
                &[release("root", "1.0.0")]
            )),
            "content_version_unsatisfied"
        );
    }

    #[test]
    fn asymmetric_conflicts_and_case_colliding_targets_fail_closed() {
        let mut first = release("first", "1.0.0");
        first.dependencies.push(dependency(
            "second",
            ContentDependencyKind::Incompatible,
            ContentVersionRequirement::Any,
        ));
        assert_eq!(
            error_code(resolve_content(
                &request(vec![selection("first"), selection("second")]),
                &[first, release("second", "1.0.0")]
            )),
            "content_conflict"
        );

        let upper = release("upper", "1.0.0");
        let mut lower = release("lower", "1.0.0");
        lower.artifact.relative_target = "mods/UPPER.jar".into();
        lower.source = Some(ContentSourceV1::Local {
            file_name: "UPPER.jar".into(),
        });
        assert_eq!(
            error_code(resolve_content(
                &request(vec![selection("upper"), selection("lower")]),
                &[upper, lower]
            )),
            "content_target_conflict"
        );
    }

    #[test]
    fn optional_dependencies_follow_explicit_policy() {
        let mut root = release("root", "1.0.0");
        root.dependencies.push(dependency(
            "optional-lib",
            ContentDependencyKind::Optional,
            ContentVersionRequirement::Any,
        ));
        let optional = release("optional-lib", "1.0.0");
        let without = resolve_content(
            &request(vec![selection("root")]),
            &[root.clone(), optional.clone()],
        )
        .expect("optional disabled");
        assert_eq!(without.items.len(), 1);

        let mut with_request = request(vec![selection("root")]);
        with_request.include_optional_dependencies = true;
        let with = resolve_content(&with_request, &[root, optional]).expect("optional enabled");
        assert_eq!(with.items.len(), 2);
        assert_eq!(with.items[1].dependencies.len(), 1);
        assert_eq!(
            with.items[1].dependencies[0].kind,
            ContentDependencyKind::Optional
        );
    }

    #[test]
    fn disabled_roots_are_pinned_without_activating_their_graph() {
        let mut disabled = selection("disabled");
        disabled.enabled = false;
        let mut disabled_release = release("disabled", "1.0.0");
        disabled_release.dependencies.push(dependency(
            "missing",
            ContentDependencyKind::Required,
            ContentVersionRequirement::Any,
        ));
        let lock = resolve_content(&request(vec![disabled]), &[disabled_release])
            .expect("disabled content remains reproducibly pinned");
        assert_eq!(lock.items.len(), 1);
        assert!(!lock.items[0].enabled);
        assert_eq!(lock.items[0].dependencies.len(), 1);
        assert_eq!(
            lock.items[0].dependencies[0].kind,
            ContentDependencyKind::Required
        );
        assert!(lock.items[0].dependencies[0].resolved_version.is_none());
    }

    #[test]
    fn disabled_selection_becomes_enabled_when_transitively_required() {
        let mut disabled = selection("library");
        disabled.enabled = false;
        disabled.version = ContentVersionRequirement::Exact {
            version: "1.0.0".into(),
        };
        let mut root = release("root", "1.0.0");
        root.dependencies.push(dependency(
            "library",
            ContentDependencyKind::Required,
            ContentVersionRequirement::Any,
        ));
        let lock = resolve_content(
            &request(vec![selection("root"), disabled]),
            &[root, release("library", "1.0.0")],
        )
        .expect("transitive requirement activates selection");
        assert!(
            lock.items
                .iter()
                .find(|item| item.content_id == "library")
                .expect("library item")
                .enabled
        );
    }

    #[test]
    fn cycles_resolve_and_lock_dependencies_remain_canonical() {
        let mut first = release("first", "1.0.0");
        first.dependencies.push(dependency(
            "second",
            ContentDependencyKind::Required,
            ContentVersionRequirement::Any,
        ));
        let mut second = release("second", "1.0.0");
        second.dependencies.push(dependency(
            "first",
            ContentDependencyKind::Required,
            ContentVersionRequirement::Any,
        ));
        let lock = resolve_content(&request(vec![selection("first")]), &[second, first])
            .expect("compatible dependency cycle");
        assert_eq!(lock.items.len(), 2);
        validate_resolved_content_lock(&lock).expect("valid canonical lock");
    }

    #[test]
    fn one_way_incompatible_edges_survive_the_lock_and_guard_future_items() {
        let mut guarded = release("guarded", "1.0.0");
        guarded.dependencies.push(dependency(
            "forbidden",
            ContentDependencyKind::Incompatible,
            ContentVersionRequirement::OneOf {
                versions: vec!["2.0.0".into(), "1.0.0".into()],
            },
        ));
        let guarded_lock = resolve_content(
            &request(vec![selection("guarded")]),
            &[guarded, release("forbidden", "1.0.0")],
        )
        .expect("guarded item alone");
        let edge = &guarded_lock.items[0].dependencies[0];
        assert_eq!(edge.kind, ContentDependencyKind::Incompatible);
        assert_eq!(
            edge.version_requirement,
            ContentVersionRequirement::OneOf {
                versions: vec!["1.0.0".into(), "2.0.0".into()]
            }
        );
        assert!(edge.resolved_version.is_none());

        let forbidden_lock = resolve_content(
            &request(vec![selection("forbidden")]),
            &[release("forbidden", "1.0.0")],
        )
        .expect("independent item");
        let mut combined = guarded_lock;
        combined.requested.push(selection("forbidden"));
        combined
            .requested
            .sort_by(|left, right| left.content_id.cmp(&right.content_id));
        combined.items.push(forbidden_lock.items[0].clone());
        combined
            .items
            .sort_by(|left, right| left.content_id.cmp(&right.content_id));
        assert_eq!(
            error_code(validate_resolved_content_lock(&combined)),
            "content_lock_conflict"
        );
    }

    #[test]
    fn lock_hash_detects_tampering_and_order_is_mandatory() {
        let mut lock = resolve_content(
            &request(vec![selection("first"), selection("second")]),
            &[release("first", "1.0.0"), release("second", "1.0.0")],
        )
        .expect("lock");
        lock.items[0].size_bytes += 1;
        assert_eq!(
            error_code(validate_resolved_content_lock(&lock)),
            "content_lock_hash_mismatch"
        );

        let mut reordered = resolve_content(
            &request(vec![selection("first"), selection("second")]),
            &[release("first", "1.0.0"), release("second", "1.0.0")],
        )
        .expect("lock");
        reordered.items.swap(0, 1);
        assert_eq!(
            error_code(validate_resolved_content_lock(&reordered)),
            "content_lock_item_order_invalid"
        );
    }

    #[test]
    fn shared_pack_members_require_one_owner_and_enabled_active_defaults() {
        let lock = shared_pack_member_lock();
        validate_resolved_content_lock(&lock).expect("shared member is valid for two packs");

        let mut duplicate_owner = lock.clone();
        duplicate_owner.pack_members[1].owns_selection = true;
        assert_eq!(
            error_code(validate_resolved_content_lock(&duplicate_owner)),
            "content_pack_member_owner_duplicate"
        );

        let mut disabled_default_member = lock;
        disabled_default_member
            .items
            .iter_mut()
            .find(|item| item.content_id == "shared-member")
            .expect("shared member item")
            .enabled = false;
        assert_eq!(
            error_code(validate_resolved_content_lock(&disabled_default_member)),
            "content_pack_member_enabled_state_invalid"
        );
    }

    #[test]
    fn item_and_override_file_ancestor_targets_are_rejected() {
        let mut lock = shared_pack_member_lock();
        lock.overrides.push(ResolvedContentOverrideV1 {
            pack_content_id: "pack-a".into(),
            relative_target: "mods/shared-member.jar/settings.json".into(),
            sha256: hex::encode(Sha256::digest(b"settings")),
            size_bytes: 8,
        });
        assert_eq!(
            error_code(validate_resolved_content_lock(&lock)),
            "content_lock_target_ancestor_collision"
        );
    }

    #[test]
    fn empty_override_files_are_valid_and_hash_bound() {
        let mut lock = shared_pack_member_lock();
        lock.overrides.push(ResolvedContentOverrideV1 {
            pack_content_id: "pack-a".into(),
            relative_target: "config/empty.toml".into(),
            sha256: hex::encode(Sha256::digest([])),
            size_bytes: 0,
        });
        lock.resolution_sha256 = content_lock_sha256(&lock).expect("lock hash with empty override");
        validate_resolved_content_lock(&lock).expect("empty override remains valid");
    }

    #[test]
    fn release_rejects_unsafe_targets_sources_and_ambiguous_catalog_identity() {
        let mut traversal = release("unsafe", "1.0.0");
        traversal.artifact.relative_target = "mods/../unsafe.jar".into();
        assert_eq!(
            error_code(validate_content_release(&traversal)),
            "path_traversal"
        );

        let mut url_source =
            serde_json::to_value(release("safe", "1.0.0")).expect("serialize release");
        url_source["source"]["url"] =
            serde_json::json!(concat!("https", "://example.invalid/file.jar"));
        assert!(serde_json::from_value::<ContentReleaseV1>(url_source).is_err());

        let first = release("duplicate", "1.0.0");
        let mut conflicting = first.clone();
        conflicting.artifact.sha256 = "f".repeat(64);
        assert_eq!(
            error_code(resolve_content(
                &request(vec![selection("duplicate")]),
                &[first, conflicting]
            )),
            "content_catalog_release_conflict"
        );
    }

    #[test]
    fn registered_targets_reuse_registry_traversal_and_length_protection() {
        let root = std::env::temp_dir().join(format!(
            "s9lab-content-target-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("target root");
        let registry = PathRegistry::new(
            &root,
            [RegisteredRoot {
                id: "instance".into(),
                path: root.clone(),
            }],
        )
        .expect("registry");
        let lock = resolve_content(
            &request(vec![selection("safe")]),
            &[release("safe", "1.0.0")],
        )
        .expect("lock");
        let secure = validate_registered_content_target(&registry, "instance", &lock.items[0])
            .expect("secure target");
        assert!(secure.absolute().starts_with(&root));
        fs::remove_dir_all(root).expect("remove target root");
    }

    #[test]
    fn natural_version_order_prefers_numeric_ten_and_stable_release() {
        assert_eq!(compare_versions("1.10.0", "1.9.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0", "1.0.0-beta"), Ordering::Greater);
    }

    #[test]
    fn serde_defaults_enabled_and_rejects_unknown_security_fields() {
        let selection: ContentSelection = serde_json::from_value(serde_json::json!({
            "contentId": "example",
            "version": { "mode": "any" }
        }))
        .expect("selection default");
        assert!(selection.enabled);

        assert!(
            serde_json::from_value::<ContentSelection>(serde_json::json!({
                "contentId": "example",
                "enabled": true,
                "downloadUrl": concat!("https", "://example.invalid/file.jar")
            }))
            .is_err()
        );
    }
}
