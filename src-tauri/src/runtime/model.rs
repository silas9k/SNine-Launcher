use crate::download::ProviderId;
use serde::{Deserialize, Serialize};

pub const RUNTIME_LOCK_FORMAT: &str = "s9lab-runtime-lock";
pub const RUNTIME_LOCK_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Neoforge,
}

impl LoaderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Neoforge => "neoforge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoaderSelection {
    pub kind: LoaderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum JavaPolicy {
    Managed { major_version: u16 },
    System { major_version: u16 },
}

impl JavaPolicy {
    pub fn major_version(&self) -> u16 {
        match self {
            Self::Managed { major_version } | Self::System { major_version } => *major_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileRuntimeIntent {
    pub minecraft_version: String,
    pub loader: LoaderSelection,
    pub java: JavaPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeArtifactKind {
    MinecraftClient,
    MinecraftVersionMetadata,
    MinecraftLibrary,
    AssetIndex,
    AssetObject,
    LoggingConfiguration,
    LoaderMetadata,
    LoaderLibrary,
    S9labComponent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedRuntimeItem {
    pub provider_id: ProviderId,
    pub logical_id: String,
    pub relative_target: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub kind: RuntimeArtifactKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedRuntimeLockV1 {
    pub format: String,
    pub format_version: u32,
    pub runtime: ProfileRuntimeIntent,
    pub items: Vec<ResolvedRuntimeItem>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityState {
    Available,
    #[default]
    Unconfigured,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityStatus {
    pub capability_id: String,
    pub state: CapabilityState,
    pub reason_code: String,
}

impl Default for CapabilityStatus {
    fn default() -> Self {
        Self::unconfigured("unknown", "capability_unconfigured")
    }
}

impl CapabilityStatus {
    pub fn available(capability_id: impl Into<String>) -> Self {
        Self {
            capability_id: capability_id.into(),
            state: CapabilityState::Available,
            reason_code: String::new(),
        }
    }

    pub fn unconfigured(capability_id: impl Into<String>, reason_code: impl Into<String>) -> Self {
        Self {
            capability_id: capability_id.into(),
            state: CapabilityState::Unconfigured,
            reason_code: reason_code.into(),
        }
    }

    pub fn disabled(capability_id: impl Into<String>, reason_code: impl Into<String>) -> Self {
        Self {
            capability_id: capability_id.into(),
            state: CapabilityState::Disabled,
            reason_code: reason_code.into(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.state == CapabilityState::Available && self.reason_code.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_defaults_fail_closed() {
        let status = CapabilityStatus::default();
        assert_eq!(status.state, CapabilityState::Unconfigured);
        assert!(!status.is_available());
        assert!(!status.reason_code.is_empty());
    }

    #[test]
    fn security_models_reject_unknown_fields() {
        let unknown_loader = serde_json::json!({
            "kind": "fabric",
            "loaderVersion": "0.16.10",
            "rawUrl": (["https", "://example.invalid/uncontrolled"].concat())
        });
        assert!(serde_json::from_value::<LoaderSelection>(unknown_loader).is_err());

        let unknown_intent = serde_json::json!({
            "minecraftVersion": "1.21.1",
            "loader": { "kind": "vanilla" },
            "java": { "mode": "managed", "majorVersion": 21 },
            "javaPath": "C:\\uncontrolled\\java.exe"
        });
        assert!(serde_json::from_value::<ProfileRuntimeIntent>(unknown_intent).is_err());
    }
}
