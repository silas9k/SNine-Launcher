use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AccountKind {
    Microsoft,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AccountSessionState {
    Active,
    ReloginRequired,
}

impl AccountSessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::ReloginRequired => "relogin-required",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "active" => Self::Active,
            _ => Self::ReloginRequired,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub username: String,
    pub kind: AccountKind,
    pub session_state: AccountSessionState,
    pub ownership_verified_at_unix: i64,
    pub last_online_auth_at_unix: i64,
    pub added_at_unix: i64,
    pub last_used_at_unix: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AccountSession {
    pub microsoft_refresh_token: String,
    pub minecraft_access_token: Option<String>,
    pub minecraft_expires_at_unix: i64,
    pub xuid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OfflinePolicyStatus {
    pub policy: String,
    pub eligible: bool,
    pub reason: String,
}

impl OfflinePolicyStatus {
    pub fn unconfigured() -> Self {
        Self {
            policy: "unconfigured".into(),
            eligible: false,
            reason: "offline_policy_unconfigured".into(),
        }
    }
}
