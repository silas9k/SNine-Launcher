use super::model::{
    CloudSyncSnapshot, LocalSyncRevisionSummary, SyncConflict, SyncContentEntryV1, SyncPayloadV1,
    SyncProfileMetadataV1, SyncRevisionV1, ThreeWayMerge, SYNC_PAYLOAD_FORMAT,
    SYNC_PAYLOAD_VERSION,
};
use crate::{
    app::config,
    content_service::Phase6ContentService,
    error::{AppError, AppResult},
    foundation::CoreServices,
    operations::model::{canonical_json, sha256_hex},
    profiles::service::ProfileService,
    storage::Storage,
};
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet};

pub trait CloudSyncProvider: Send + Sync {
    fn capability(&self) -> (&'static str, &'static str);
    fn link_account(&self) -> AppResult<()>;
    fn pull_revision(&self) -> AppResult<SyncRevisionV1>;
    fn push_revision(&self, revision: &SyncRevisionV1) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub struct UnconfiguredCloudSyncProvider;

impl CloudSyncProvider for UnconfiguredCloudSyncProvider {
    fn capability(&self) -> (&'static str, &'static str) {
        ("unconfigured", "cloud_provider_unconfigured")
    }

    fn link_account(&self) -> AppResult<()> {
        Err(AppError::coded("cloud_provider_unconfigured"))
    }

    fn pull_revision(&self) -> AppResult<SyncRevisionV1> {
        Err(AppError::coded("cloud_provider_unconfigured"))
    }

    fn push_revision(&self, _revision: &SyncRevisionV1) -> AppResult<()> {
        Err(AppError::coded("cloud_provider_unconfigured"))
    }
}

#[derive(Clone)]
pub struct CloudSyncService {
    storage: Storage,
    profiles: ProfileService,
    content: Phase6ContentService,
    settings_file: std::path::PathBuf,
    provider: std::sync::Arc<dyn CloudSyncProvider>,
}

impl CloudSyncService {
    pub fn from_core(core: &CoreServices) -> AppResult<Self> {
        Ok(Self {
            storage: core.storage().clone(),
            profiles: ProfileService::from_core(core),
            content: Phase6ContentService::from_core(core)?,
            settings_file: core.paths().settings_file.clone(),
            provider: std::sync::Arc::new(UnconfiguredCloudSyncProvider),
        })
    }

    pub fn snapshot(&self) -> AppResult<CloudSyncSnapshot> {
        let payload = self.local_payload()?;
        let payload_sha256 = sha256_hex(canonical_json(&payload)?.as_bytes());
        let (provider_state, reason_code) = self.provider.capability();
        let microsoft_base_account = self
            .storage
            .selected_account_id()?
            .map(|account_id| {
                self.storage
                    .account(&account_id)?
                    .map(|account| account.username)
                    .ok_or_else(|| AppError::coded("cloud_base_account_missing"))
            })
            .transpose()?;
        Ok(CloudSyncSnapshot {
            provider_state: provider_state.into(),
            reason_code: reason_code.into(),
            microsoft_base_account,
            linked_s9lab_account: None,
            session_state: "unavailable".into(),
            online: false,
            device_limit: 2,
            enrolled_devices: 0,
            scopes: vec![
                "profile-metadata".into(),
                "content-lists".into(),
                "settings".into(),
            ],
            local_revision: LocalSyncRevisionSummary {
                revision_id: format!("local-{}", &payload_sha256[..32]),
                payload_sha256,
                profile_count: payload.profiles.len() as u32,
                content_count: payload.content.len() as u32,
                settings_included: true,
            },
            pending_conflicts: 0,
        })
    }

    pub fn local_revision(
        &self,
        device_id: &str,
        parent_revision_id: Option<String>,
    ) -> AppResult<SyncRevisionV1> {
        validate_device_id(device_id)?;
        let payload = self.local_payload()?;
        let payload_sha256 = sha256_hex(canonical_json(&payload)?.as_bytes());
        Ok(SyncRevisionV1 {
            revision_id: format!("sync-{}", &payload_sha256[..32]),
            parent_revision_id,
            device_id: device_id.into(),
            created_at_unix: Utc::now().timestamp(),
            payload_sha256,
            payload,
        })
    }

    fn local_payload(&self) -> AppResult<SyncPayloadV1> {
        let mut profile_metadata = Vec::new();
        let mut content_entries = Vec::new();
        for profile in self
            .profiles
            .list_profiles()?
            .into_iter()
            .filter(|profile| profile.lifecycle_state != "trash")
        {
            let snapshot = self.content.snapshot(&profile.id)?;
            content_entries.extend(snapshot.content.into_iter().map(|item| SyncContentEntryV1 {
                profile_id: profile.id.clone(),
                content_id: item.content_id,
                content_type: item.content_type.as_str().into(),
                version: item.version_number,
                enabled: item.enabled,
            }));
            profile_metadata.push(SyncProfileMetadataV1 {
                profile_id: profile.id,
                display_name: profile.display_name,
                lifecycle_state: profile.lifecycle_state,
                favorite: profile.favorite,
                active_revision_id: profile.active_revision_id,
            });
        }
        profile_metadata.sort();
        content_entries.sort();
        Ok(SyncPayloadV1 {
            format: SYNC_PAYLOAD_FORMAT.into(),
            format_version: SYNC_PAYLOAD_VERSION,
            profiles: profile_metadata,
            content: content_entries,
            settings: config::load_settings_from(&self.settings_file)?.shell_settings(),
        })
    }
}

pub fn three_way_merge(
    base: &BTreeMap<String, String>,
    local: &BTreeMap<String, String>,
    remote: &BTreeMap<String, String>,
) -> ThreeWayMerge {
    let keys = base
        .keys()
        .chain(local.keys())
        .chain(remote.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut merged_fields = BTreeMap::new();
    let mut conflicts = Vec::new();
    for key in keys {
        let base_value = base.get(&key);
        let local_value = local.get(&key);
        let remote_value = remote.get(&key);
        let selected = if local_value == remote_value {
            local_value
        } else if local_value == base_value {
            remote_value
        } else if remote_value == base_value {
            local_value
        } else {
            conflicts.push(SyncConflict {
                field: key.clone(),
                base_value: base_value.cloned(),
                local_value: local_value.cloned(),
                remote_value: remote_value.cloned(),
            });
            base_value
        };
        if let Some(value) = selected {
            merged_fields.insert(key, value.clone());
        }
    }
    ThreeWayMerge {
        merged_fields,
        conflicts,
    }
}

pub fn resolve_conflicts(
    preview: &ThreeWayMerge,
    choices: &BTreeMap<String, String>,
) -> AppResult<BTreeMap<String, String>> {
    if preview.conflicts.len() != choices.len() {
        return Err(AppError::coded("cloud_conflict_resolution_incomplete"));
    }
    let mut resolved = preview.merged_fields.clone();
    for conflict in &preview.conflicts {
        match choices.get(&conflict.field).map(String::as_str) {
            Some("local") => set_optional(&mut resolved, &conflict.field, &conflict.local_value),
            Some("remote") => set_optional(&mut resolved, &conflict.field, &conflict.remote_value),
            _ => return Err(AppError::coded("cloud_conflict_resolution_invalid")),
        }
    }
    Ok(resolved)
}

fn set_optional(target: &mut BTreeMap<String, String>, key: &str, value: &Option<String>) {
    if let Some(value) = value {
        target.insert(key.into(), value.clone());
    } else {
        target.remove(key);
    }
}

fn validate_device_id(value: &str) -> AppResult<()> {
    if value.len() < 8
        || value.len() > 80
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(AppError::coded("cloud_device_id_invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_way_merge_combines_two_devices_and_requires_manual_conflict_choices() {
        let base = BTreeMap::from([
            ("profiles/a/name".into(), "Base".into()),
            ("settings/theme".into(), "dark".into()),
        ]);
        let local = BTreeMap::from([
            ("profiles/a/name".into(), "Local".into()),
            ("settings/theme".into(), "light".into()),
            ("content/a/mod-one".into(), "1.0".into()),
        ]);
        let remote = BTreeMap::from([
            ("profiles/a/name".into(), "Remote".into()),
            ("settings/theme".into(), "dark".into()),
            ("content/a/mod-two".into(), "2.0".into()),
        ]);
        let preview = three_way_merge(&base, &local, &remote);
        assert_eq!(preview.conflicts.len(), 1);
        assert_eq!(preview.conflicts[0].field, "profiles/a/name");
        assert_eq!(
            preview.merged_fields.get("settings/theme"),
            Some(&"light".into())
        );
        assert_eq!(
            preview.merged_fields.get("content/a/mod-one"),
            Some(&"1.0".into())
        );
        assert_eq!(
            preview.merged_fields.get("content/a/mod-two"),
            Some(&"2.0".into())
        );
        let resolved = resolve_conflicts(
            &preview,
            &BTreeMap::from([("profiles/a/name".into(), "remote".into())]),
        )
        .expect("manual resolution");
        assert_eq!(resolved.get("profiles/a/name"), Some(&"Remote".into()));
    }

    #[test]
    fn unconfigured_provider_fails_closed_without_a_network_fallback() {
        let provider = UnconfiguredCloudSyncProvider;
        assert_eq!(provider.capability().0, "unconfigured");
        assert_eq!(
            provider
                .link_account()
                .expect_err("link blocked")
                .descriptor()
                .code,
            "cloud_provider_unconfigured"
        );
        assert_eq!(
            provider
                .pull_revision()
                .expect_err("pull blocked")
                .descriptor()
                .code,
            "cloud_provider_unconfigured"
        );
    }

    #[test]
    fn sync_payload_type_cannot_serialize_tokens_worlds_or_arbitrary_files() {
        let payload = SyncPayloadV1 {
            format: SYNC_PAYLOAD_FORMAT.into(),
            format_version: SYNC_PAYLOAD_VERSION,
            profiles: vec![],
            content: vec![],
            settings: crate::app::config::ShellSettings {
                appearance: "dark".into(),
                locale: "en".into(),
                accent_color: "#c62847".into(),
                density: "compact".into(),
                navigation_mode: "expanded".into(),
                background_variant: "calm".into(),
                reduced_motion: false,
            },
        };
        let serialized = serde_json::to_string(&payload).expect("serialize");
        for forbidden in ["token", "world", "filePath", "gameDirectory", "javaPath"] {
            assert!(!serialized
                .to_ascii_lowercase()
                .contains(&forbidden.to_ascii_lowercase()));
        }
    }
}
