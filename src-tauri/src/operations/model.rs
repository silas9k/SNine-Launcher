use crate::{
    error::{AppError, AppResult},
    storage::models::RuntimeQueryProjection,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static IDENTIFIER_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationState {
    Planned,
    Staging,
    Verifying,
    ReadyToCommit,
    Committing,
    Validating,
    Completed,
    RollingBack,
    RolledBack,
    Failed,
}

impl OperationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Staging => "staging",
            Self::Verifying => "verifying",
            Self::ReadyToCommit => "ready-to-commit",
            Self::Committing => "committing",
            Self::Validating => "validating",
            Self::Completed => "completed",
            Self::RollingBack => "rolling-back",
            Self::RolledBack => "rolled-back",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "planned" => Ok(Self::Planned),
            "staging" => Ok(Self::Staging),
            "verifying" => Ok(Self::Verifying),
            "ready-to-commit" => Ok(Self::ReadyToCommit),
            "committing" => Ok(Self::Committing),
            "validating" => Ok(Self::Validating),
            "completed" => Ok(Self::Completed),
            "rolling-back" => Ok(Self::RollingBack),
            "rolled-back" => Ok(Self::RolledBack),
            "failed" => Ok(Self::Failed),
            _ => Err(AppError::coded_with(
                "operation_state_unknown",
                [("state", value.to_string())],
            )),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::RolledBack | Self::Failed)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        match self {
            Self::Planned => matches!(next, Self::Staging | Self::RollingBack | Self::Failed),
            Self::Staging => matches!(next, Self::Verifying | Self::RollingBack | Self::Failed),
            Self::Verifying => {
                matches!(next, Self::ReadyToCommit | Self::RollingBack | Self::Failed)
            }
            Self::ReadyToCommit => {
                matches!(next, Self::Committing | Self::RollingBack | Self::Failed)
            }
            Self::Committing => matches!(
                next,
                Self::Validating | Self::Completed | Self::RollingBack | Self::Failed
            ),
            Self::Validating => {
                matches!(next, Self::Completed | Self::RollingBack | Self::Failed)
            }
            Self::RollingBack => matches!(next, Self::RolledBack | Self::Failed),
            Self::Completed | Self::RolledBack | Self::Failed => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OperationType {
    SimulatedProfileInstall,
    ProfileRevision,
    RuntimeInstall,
    RuntimeRepair,
    ComponentChange,
    ContentInstall,
    ContentChange,
    ContentImport,
    ProfileRollback,
    ProfileBackupRestore,
}

impl OperationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SimulatedProfileInstall => "simulated-profile-install",
            Self::ProfileRevision => "profile-revision",
            Self::RuntimeInstall => "runtime-install",
            Self::RuntimeRepair => "runtime-repair",
            Self::ComponentChange => "component-change",
            Self::ContentInstall => "content-install",
            Self::ContentChange => "content-change",
            Self::ContentImport => "content-import",
            Self::ProfileRollback => "profile-rollback",
            Self::ProfileBackupRestore => "profile-backup-restore",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        match value {
            "simulated-profile-install" => Ok(Self::SimulatedProfileInstall),
            "profile-revision" => Ok(Self::ProfileRevision),
            "runtime-install" => Ok(Self::RuntimeInstall),
            "runtime-repair" => Ok(Self::RuntimeRepair),
            "component-change" => Ok(Self::ComponentChange),
            "content-install" => Ok(Self::ContentInstall),
            "content-change" => Ok(Self::ContentChange),
            "content-import" => Ok(Self::ContentImport),
            "profile-rollback" => Ok(Self::ProfileRollback),
            "profile-backup-restore" => Ok(Self::ProfileBackupRestore),
            _ => Err(AppError::coded_with(
                "operation_type_unknown",
                [("operationType", value.to_string())],
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalManifest {
    pub format: String,
    pub format_version: u32,
    pub profile_id: String,
    pub display_name: String,
    pub intent: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinimalLock {
    pub format: String,
    pub format_version: u32,
    pub profile_id: String,
    pub revision_id: String,
    pub manifest_sha256: String,
    pub resolved: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedFile {
    pub relative_path: String,
    pub content_utf8: String,
    pub sha256: String,
}

impl PlannedFile {
    pub fn new(relative_path: impl Into<String>, content_utf8: impl Into<String>) -> Self {
        let content_utf8 = content_utf8.into();
        Self {
            relative_path: relative_path.into(),
            sha256: sha256_hex(content_utf8.as_bytes()),
            content_utf8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheMaterialization {
    pub blob_sha256: String,
    pub size_bytes: u64,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInstallPlan {
    pub operation_id: String,
    pub profile_id: String,
    pub revision_id: String,
    pub previous_revision_id: Option<String>,
    pub manifest_json: String,
    pub manifest_sha256: String,
    pub lock_json: String,
    pub lock_sha256: String,
    pub payload_files: Vec<PlannedFile>,
    #[serde(default)]
    pub cache_materializations: Vec<CacheMaterialization>,
    #[serde(default)]
    pub runtime_projection: Option<RuntimeQueryProjection>,
    #[serde(default)]
    pub previous_runtime_projection: Option<RuntimeQueryProjection>,
    #[serde(default)]
    pub cleanup_profile_on_rollback: bool,
}

impl ProfileInstallPlan {
    pub fn new(
        profile_id: impl Into<String>,
        display_name: impl Into<String>,
        previous_revision_id: Option<String>,
    ) -> AppResult<Self> {
        let profile_id = profile_id.into();
        let operation_id = new_identifier("op");
        let revision_id = new_identifier("rev");
        let manifest = MinimalManifest {
            format: "site.s9lab.profile.phase1-demo".into(),
            format_version: 1,
            profile_id: profile_id.clone(),
            display_name: display_name.into(),
            intent: BTreeMap::from([
                ("purpose".into(), "phase1-transaction-demo".into()),
                ("dataAuthority".into(), "manifest".into()),
            ]),
        };
        let manifest_json = canonical_json(&manifest)?;
        let manifest_sha256 = sha256_hex(manifest_json.as_bytes());
        let lock = MinimalLock {
            format: "site.s9lab.profile-lock.phase1-demo".into(),
            format_version: 1,
            profile_id: profile_id.clone(),
            revision_id: revision_id.clone(),
            manifest_sha256: manifest_sha256.clone(),
            resolved: BTreeMap::from([
                ("state".into(), "simulated".into()),
                ("dataAuthority".into(), "lock".into()),
            ]),
        };
        let lock_json = canonical_json(&lock)?;
        let lock_sha256 = sha256_hex(lock_json.as_bytes());
        Ok(Self {
            operation_id,
            profile_id,
            revision_id,
            previous_revision_id,
            manifest_json,
            manifest_sha256,
            lock_json,
            lock_sha256,
            payload_files: vec![PlannedFile::new(
                "instance/phase1-installed.txt",
                "S9Lab Phase 1 simulated installation\n",
            )],
            cache_materializations: Vec::new(),
            runtime_projection: None,
            previous_runtime_projection: None,
            cleanup_profile_on_rollback: false,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailurePoint {
    AfterPlanned,
    AfterStaging,
    AfterVerifying,
    AfterReadyToCommit,
    AfterRevisionMoved,
    AfterDatabaseActivated,
    DuringValidation,
    AfterCacheReferences,
}

impl FailurePoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AfterPlanned => "after-planned",
            Self::AfterStaging => "after-staging",
            Self::AfterVerifying => "after-verifying",
            Self::AfterReadyToCommit => "after-ready-to-commit",
            Self::AfterRevisionMoved => "after-revision-moved",
            Self::AfterDatabaseActivated => "after-database-activated",
            Self::DuringValidation => "during-validation",
            Self::AfterCacheReferences => "after-cache-references",
        }
    }
}

pub trait FailureInjector: Send + Sync {
    fn checkpoint(&self, point: FailurePoint) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct NoFailure;

impl FailureInjector for NoFailure {
    fn checkpoint(&self, _: FailurePoint) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub struct FailAt(pub FailurePoint);

#[cfg(test)]
impl FailureInjector for FailAt {
    fn checkpoint(&self, point: FailurePoint) -> AppResult<()> {
        if point == self.0 {
            Err(AppError::coded_with(
                "operation_injected_failure",
                [("point", point.as_str().to_string())],
            ))
        } else {
            Ok(())
        }
    }
}

pub fn new_identifier(prefix: &str) -> String {
    let counter = IDENTIFIER_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(std::process::id().to_le_bytes());
    hasher.update(counter.to_le_bytes());
    hasher.update(nanos.to_le_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("{prefix}-{}", &digest[..32])
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn canonical_json<T: Serialize>(value: &T) -> AppResult<String> {
    let mut output = serde_json::to_string_pretty(value)?;
    output.push('\n');
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_types_round_trip_through_persistence_names() {
        for operation_type in [
            OperationType::SimulatedProfileInstall,
            OperationType::ProfileRevision,
            OperationType::RuntimeInstall,
            OperationType::RuntimeRepair,
            OperationType::ComponentChange,
            OperationType::ContentInstall,
            OperationType::ContentChange,
            OperationType::ContentImport,
            OperationType::ProfileRollback,
            OperationType::ProfileBackupRestore,
        ] {
            assert_eq!(
                OperationType::parse(operation_type.as_str()).expect("parse operation type"),
                operation_type
            );
        }
    }

    #[test]
    fn plans_without_cache_materializations_remain_deserializable() {
        let plan =
            ProfileInstallPlan::new("profile-legacy", "Legacy plan", None).expect("create plan");
        let mut value = serde_json::to_value(&plan).expect("serialize plan");
        value
            .as_object_mut()
            .expect("plan object")
            .remove("cacheMaterializations");

        let decoded: ProfileInstallPlan =
            serde_json::from_value(value).expect("deserialize legacy plan");
        assert!(decoded.cache_materializations.is_empty());
        assert_eq!(decoded.operation_id, plan.operation_id);
        assert_eq!(decoded.revision_id, plan.revision_id);
    }
}
