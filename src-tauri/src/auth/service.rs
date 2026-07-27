use crate::{
    app::paths::LauncherPaths,
    auth::{
        microsoft::{DeviceCodeSecret, MicrosoftApi},
        model::{Account, AccountSession, OfflinePolicyStatus},
        store::AuthStore,
    },
    error::{AppError, AppResult},
    operations::model::new_identifier,
    storage::Storage,
};
use chrono::Utc;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

const MAX_PENDING_DEVICE_LOGINS: usize = 8;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLoginPrompt {
    pub login_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at_unix: i64,
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuthSnapshot {
    pub accounts: Vec<Account>,
    pub active_account_id: Option<String>,
    pub offline_policy: OfflinePolicyStatus,
}

#[derive(Clone)]
struct PendingLogin {
    secret: DeviceCodeSecret,
    expires_at_unix: i64,
    cancelled: Arc<AtomicBool>,
    in_progress: Arc<AtomicBool>,
}

pub struct AuthService {
    api: MicrosoftApi,
    store: AuthStore,
    pending: Mutex<BTreeMap<String, PendingLogin>>,
}

impl AuthService {
    pub fn system(storage: Storage, paths: &LauncherPaths) -> AppResult<Self> {
        Ok(Self {
            api: MicrosoftApi::new()?,
            store: AuthStore::system(storage, paths.accounts_file.clone())?,
            pending: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn snapshot(&self) -> AppResult<AuthSnapshot> {
        Ok(AuthSnapshot {
            accounts: self.store.list_accounts()?,
            active_account_id: self.store.active_account_id()?,
            offline_policy: OfflinePolicyStatus::unconfigured(),
        })
    }

    pub async fn start_device_login(&self, locale: &str) -> AppResult<DeviceLoginPrompt> {
        let secret = self.api.request_device_code(locale).await?;
        let now = Utc::now().timestamp();
        let expires_at_unix = now.saturating_add(i64::try_from(secret.expires_in).unwrap_or(i64::MAX));
        let login_id = new_identifier("login");
        let prompt = DeviceLoginPrompt {
            login_id: login_id.clone(),
            user_code: secret.user_code.clone(),
            verification_uri: secret.verification_uri.clone(),
            expires_at_unix,
            interval_seconds: secret.interval,
        };
        let mut pending = self.lock_pending()?;
        pending.retain(|_, login| login.expires_at_unix > now);
        if pending.len() >= MAX_PENDING_DEVICE_LOGINS {
            return Err(AppError::coded("auth_too_many_pending_logins"));
        }
        pending.insert(
            login_id,
            PendingLogin {
                secret,
                expires_at_unix,
                cancelled: Arc::new(AtomicBool::new(false)),
                in_progress: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(prompt)
    }

    pub async fn complete_device_login(&self, login_id: &str) -> AppResult<Account> {
        let pending = {
            let pending = self.lock_pending()?;
            pending
                .get(login_id)
                .cloned()
                .ok_or_else(|| AppError::coded("auth_device_login_not_found"))?
        };
        if pending.expires_at_unix <= Utc::now().timestamp() {
            self.remove_pending(login_id)?;
            return Err(AppError::coded("auth_device_code_expired"));
        }
        if pending
            .in_progress
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(AppError::coded("auth_device_login_in_progress"));
        }

        let result = self
            .api
            .complete_device_login(&pending.secret, pending.cancelled.clone())
            .await
            .and_then(|verified| {
                self.store.persist_authenticated(
                    &verified.account_id,
                    &verified.username,
                    &verified.session,
                    verified.verified_at_unix,
                )
            });
        self.remove_pending(login_id)?;
        if result.is_ok() {
            let _ = crate::logging::append(
                "Microsoft-Login, Besitzprüfung und sichere Ablage abgeschlossen",
            );
        }
        result
    }

    pub fn cancel_device_login(&self, login_id: &str) -> AppResult<()> {
        let pending = self
            .lock_pending()?
            .remove(login_id)
            .ok_or_else(|| AppError::coded("auth_device_login_not_found"))?;
        pending.cancelled.store(true, Ordering::Release);
        Ok(())
    }

    pub async fn refresh_account(&self, account_id: &str) -> AppResult<Account> {
        let current = self.store.load_session(account_id)?;
        let verified = self
            .api
            .refresh_session(&current.microsoft_refresh_token)
            .await?;
        if normalize_account_id(&verified.account_id) != normalize_account_id(account_id) {
            return Err(AppError::coded("auth_refreshed_identity_mismatch"));
        }
        let account = self.store.persist_authenticated(
            &verified.account_id,
            &verified.username,
            &verified.session,
            verified.verified_at_unix,
        )?;
        let _ = crate::logging::append("Microsoft-Sitzung erneuert und Besitz erneut geprüft");
        Ok(account)
    }

    pub(crate) async fn ensure_minecraft_session(
        &self,
        account_id: &str,
    ) -> AppResult<(Account, AccountSession)> {
        let session = self.store.load_session(account_id)?;
        let now = Utc::now().timestamp();
        if session.minecraft_expires_at_unix > now.saturating_add(300)
            && session.minecraft_access_token.is_some()
        {
            return Ok((self.store.select_account(account_id)?, session));
        }
        let account = self.refresh_account(account_id).await?;
        let session = self.store.load_session(&account.id)?;
        Ok((account, session))
    }

    pub fn select_account(&self, account_id: &str) -> AppResult<Account> {
        self.store.select_account(account_id)
    }

    pub fn remove_account(&self, account_id: &str) -> AppResult<()> {
        self.store.remove_account(account_id)
    }

    pub fn assign_profile_account(
        &self,
        profile_id: &str,
        account_id: Option<&str>,
    ) -> AppResult<()> {
        self.store.assign_profile_account(profile_id, account_id)
    }

    pub fn profile_account_id(&self, profile_id: &str) -> AppResult<Option<String>> {
        self.store.storage().profile_account_id(profile_id)
    }

    fn remove_pending(&self, login_id: &str) -> AppResult<()> {
        if let Some(pending) = self.lock_pending()?.remove(login_id) {
            pending.cancelled.store(true, Ordering::Release);
        }
        Ok(())
    }

    fn lock_pending(&self) -> AppResult<MutexGuard<'_, BTreeMap<String, PendingLogin>>> {
        self.pending
            .lock()
            .map_err(|_| AppError::coded("auth_pending_login_lock_poisoned"))
    }
}

fn normalize_account_id(account_id: &str) -> String {
    account_id
        .chars()
        .filter(|character| *character != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_prompt_never_serializes_the_device_secret() {
        let prompt = DeviceLoginPrompt {
            login_id: "login-fixture".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://www.microsoft.com/link".into(),
            expires_at_unix: 100,
            interval_seconds: 5,
        };
        let serialized = serde_json::to_string(&prompt).expect("serialize prompt");
        assert!(!serialized.contains("deviceCode"));
        assert!(!serialized.contains("device_code"));
    }

    #[test]
    fn offline_access_is_fail_closed_until_a_policy_is_approved() {
        let policy = OfflinePolicyStatus::unconfigured();
        assert_eq!(policy.policy, "unconfigured");
        assert!(!policy.eligible);
        assert_eq!(policy.reason, "offline_policy_unconfigured");
    }
}
