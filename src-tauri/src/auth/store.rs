use crate::{
    auth::model::{Account, AccountKind, AccountSession, AccountSessionState},
    error::{AppError, AppResult},
    operations::model::new_identifier,
    storage::{models::AccountRecord, Storage},
};
use keyring::Error as KeyringError;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

const SERVICE: &str = "S9Lab Launcher";
const LEGACY_SERVICE: &str = "S9LAB Client Launcher";
const LEGACY_USER: &str = "minecraft_session";
const LEGACY_FIELD_REFRESH: &str = "ms-refresh";
const LEGACY_FIELD_MINECRAFT: &str = "mc-access";
const LEGACY_FIELD_META: &str = "session-meta";
const SESSION_FIELD_REFRESH: &str = "refresh";
const SESSION_FIELD_MINECRAFT: &str = "minecraft";
const SESSION_FIELD_META: &str = "metadata";

pub(crate) trait CredentialVault: Send + Sync {
    fn put(&self, vault_ref: &str, session: &AccountSession) -> AppResult<()>;
    fn get(&self, vault_ref: &str) -> AppResult<Option<AccountSession>>;
    fn delete(&self, vault_ref: &str) -> AppResult<()>;
    fn legacy_session(&self, account_id: &str) -> AppResult<Option<AccountSession>>;
    fn delete_legacy_session(&self, account_id: &str) -> AppResult<()>;
    fn monolithic_legacy_session(&self) -> AppResult<Option<LegacySession>>;
    fn delete_monolithic_legacy_session(&self) -> AppResult<()>;
}

#[derive(Debug, Default)]
pub(crate) struct OsCredentialVault;

impl OsCredentialVault {
    fn entry(service: &str, user: &str) -> AppResult<keyring::Entry> {
        Ok(keyring::Entry::new(service, user)?)
    }

    fn read_entry(service: &str, user: &str) -> AppResult<Option<String>> {
        match Self::entry(service, user)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn delete_entry(service: &str, user: &str) -> AppResult<()> {
        match Self::entry(service, user)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn session_entry(vault_ref: &str, field: &str) -> String {
        format!("{vault_ref}:{field}")
    }

    fn delete_session_entries(vault_ref: &str) -> AppResult<()> {
        for field in [
            SESSION_FIELD_REFRESH,
            SESSION_FIELD_MINECRAFT,
            SESSION_FIELD_META,
        ] {
            Self::delete_entry(SERVICE, &Self::session_entry(vault_ref, field))?;
        }
        Ok(())
    }

    fn read_split_session(vault_ref: &str) -> AppResult<Option<AccountSession>> {
        let refresh = Self::read_entry(
            SERVICE,
            &Self::session_entry(vault_ref, SESSION_FIELD_REFRESH),
        )?;
        let minecraft = Self::read_entry(
            SERVICE,
            &Self::session_entry(vault_ref, SESSION_FIELD_MINECRAFT),
        )?;
        let metadata =
            Self::read_entry(SERVICE, &Self::session_entry(vault_ref, SESSION_FIELD_META))?;
        match (refresh, minecraft, metadata) {
            (None, None, None) => Ok(None),
            (Some(microsoft_refresh_token), minecraft_access_token, Some(metadata)) => {
                let metadata: LegacySessionMeta = serde_json::from_str(&metadata)
                    .map_err(|_| AppError::coded("credential_store_entry_invalid"))?;
                Ok(Some(AccountSession {
                    microsoft_refresh_token,
                    minecraft_access_token,
                    minecraft_expires_at_unix: metadata.minecraft_expires_at_unix,
                    xuid: metadata.xuid,
                }))
            }
            _ => Err(AppError::coded("credential_store_entry_invalid")),
        }
    }
}

impl CredentialVault for OsCredentialVault {
    fn put(&self, vault_ref: &str, session: &AccountSession) -> AppResult<()> {
        let refresh = Self::session_entry(vault_ref, SESSION_FIELD_REFRESH);
        let minecraft = Self::session_entry(vault_ref, SESSION_FIELD_MINECRAFT);
        let metadata = Self::session_entry(vault_ref, SESSION_FIELD_META);
        let metadata_value = serde_json::to_string(&LegacySessionMeta {
            minecraft_expires_at_unix: session.minecraft_expires_at_unix,
            xuid: session.xuid.clone(),
        })?;

        let result: AppResult<()> = (|| {
            Self::entry(SERVICE, &refresh)?.set_password(&session.microsoft_refresh_token)?;
            match &session.minecraft_access_token {
                Some(token) => Self::entry(SERVICE, &minecraft)?.set_password(token)?,
                None => Self::delete_entry(SERVICE, &minecraft)?,
            }
            Self::entry(SERVICE, &metadata)?.set_password(&metadata_value)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = Self::delete_session_entries(vault_ref);
        }
        result?;
        Ok(())
    }

    fn get(&self, vault_ref: &str) -> AppResult<Option<AccountSession>> {
        if let Some(legacy) = Self::read_entry(SERVICE, vault_ref)? {
            return serde_json::from_str(&legacy)
                .map(Some)
                .map_err(|_| AppError::coded("credential_store_entry_invalid"));
        }
        Self::read_split_session(vault_ref)
    }

    fn delete(&self, vault_ref: &str) -> AppResult<()> {
        Self::delete_entry(SERVICE, vault_ref)?;
        Self::delete_session_entries(vault_ref)
    }

    fn legacy_session(&self, account_id: &str) -> AppResult<Option<AccountSession>> {
        let refresh = Self::read_entry(SERVICE, &format!("{account_id}:{LEGACY_FIELD_REFRESH}"))?;
        let minecraft =
            Self::read_entry(SERVICE, &format!("{account_id}:{LEGACY_FIELD_MINECRAFT}"))?;
        let meta = Self::read_entry(SERVICE, &format!("{account_id}:{LEGACY_FIELD_META}"))?;
        let (Some(refresh), Some(minecraft), Some(meta)) = (refresh, minecraft, meta) else {
            return Ok(None);
        };
        let meta: LegacySessionMeta = serde_json::from_str(&meta)
            .map_err(|_| AppError::coded("credential_store_entry_invalid"))?;
        Ok(Some(AccountSession {
            microsoft_refresh_token: refresh,
            minecraft_access_token: Some(minecraft),
            minecraft_expires_at_unix: meta.minecraft_expires_at_unix,
            xuid: meta.xuid,
        }))
    }

    fn delete_legacy_session(&self, account_id: &str) -> AppResult<()> {
        for field in [
            LEGACY_FIELD_REFRESH,
            LEGACY_FIELD_MINECRAFT,
            LEGACY_FIELD_META,
        ] {
            Self::delete_entry(SERVICE, &format!("{account_id}:{field}"))?;
        }
        Ok(())
    }

    fn monolithic_legacy_session(&self) -> AppResult<Option<LegacySession>> {
        Self::read_entry(LEGACY_SERVICE, LEGACY_USER)?
            .map(|raw| {
                serde_json::from_str(&raw)
                    .map_err(|_| AppError::coded("credential_store_entry_invalid"))
            })
            .transpose()
    }

    fn delete_monolithic_legacy_session(&self) -> AppResult<()> {
        Self::delete_entry(LEGACY_SERVICE, LEGACY_USER)
    }
}

#[derive(Clone)]
pub struct AuthStore {
    storage: Storage,
    vault: Arc<dyn CredentialVault>,
}

impl AuthStore {
    pub fn system(storage: Storage, legacy_accounts_path: PathBuf) -> AppResult<Self> {
        let store = Self::new(storage, Arc::new(OsCredentialVault));
        store.migrate_legacy_metadata(&legacy_accounts_path)?;
        Ok(store)
    }

    pub(crate) fn new(storage: Storage, vault: Arc<dyn CredentialVault>) -> Self {
        Self { storage, vault }
    }

    pub fn list_accounts(&self) -> AppResult<Vec<Account>> {
        self.storage
            .accounts()?
            .into_iter()
            .map(account_from_record)
            .collect()
    }

    pub fn active_account_id(&self) -> AppResult<Option<String>> {
        self.storage.selected_account_id()
    }

    pub fn select_account(&self, account_id: &str) -> AppResult<Account> {
        account_from_record(self.storage.select_account(account_id)?)
    }

    pub fn persist_authenticated(
        &self,
        account_id: &str,
        username: &str,
        session: &AccountSession,
        verified_at_unix: i64,
    ) -> AppResult<Account> {
        validate_identity(account_id, username, session)?;
        let existing = self.storage.account(account_id)?;
        let vault_ref = new_identifier("vault");
        self.vault.put(&vault_ref, session)?;
        let record = AccountRecord {
            id: account_id.to_string(),
            username: username.to_string(),
            account_kind: "microsoft".into(),
            vault_ref: vault_ref.clone(),
            session_state: AccountSessionState::Active.as_str().into(),
            ownership_verified_at_unix: verified_at_unix,
            last_online_auth_at_unix: verified_at_unix,
            added_at_unix: existing
                .as_ref()
                .map(|record| record.added_at_unix)
                .unwrap_or(verified_at_unix),
            last_used_at_unix: verified_at_unix,
        };
        let previous_vault = match self.storage.upsert_account(&record) {
            Ok(previous) => previous,
            Err(error) => {
                let cleanup = self.vault.delete(&vault_ref);
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(AppError::coded_with(
                        "auth_persist_and_cleanup_failed",
                        [
                            ("primary", error.descriptor().code),
                            ("cleanup", cleanup_error.descriptor().code),
                        ],
                    )),
                };
            }
        };
        if let Some(previous_vault) = previous_vault.filter(|previous| previous != &vault_ref) {
            if let Err(error) = self.vault.delete(&previous_vault) {
                let rollback = existing
                    .as_ref()
                    .ok_or_else(|| AppError::coded("auth_rotation_previous_record_missing"))
                    .and_then(|previous| self.storage.upsert_account(previous).map(|_| ()));
                let cleanup = self.vault.delete(&vault_ref);
                return match (rollback, cleanup) {
                    (Ok(()), Ok(())) => Err(AppError::coded_with(
                        "auth_credential_rotation_failed",
                        [("cleanup", error.descriptor().code)],
                    )),
                    (rollback, cleanup) => Err(AppError::coded_with(
                        "auth_credential_rotation_rollback_failed",
                        [
                            ("cleanup", error.descriptor().code),
                            (
                                "rollback",
                                rollback
                                    .err()
                                    .map(|failure| failure.descriptor().code)
                                    .unwrap_or_else(|| "ok".to_string()),
                            ),
                            (
                                "newCredentialCleanup",
                                cleanup
                                    .err()
                                    .map(|failure| failure.descriptor().code)
                                    .unwrap_or_else(|| "ok".to_string()),
                            ),
                        ],
                    )),
                };
            }
        }
        account_from_record(record)
    }

    pub(crate) fn load_session(&self, account_id: &str) -> AppResult<AccountSession> {
        let record = self
            .storage
            .account(account_id)?
            .ok_or_else(|| AppError::AccountNotFound(account_id.to_string()))?;
        if record.session_state != AccountSessionState::Active.as_str() {
            return Err(AppError::coded_with(
                "auth_relogin_required",
                [("accountId", account_id.to_string())],
            ));
        }
        match self.vault.get(&record.vault_ref) {
            Ok(Some(session)) if !session.microsoft_refresh_token.trim().is_empty() => Ok(session),
            Ok(_) | Err(_) => {
                self.storage.mark_account_relogin_required(account_id)?;
                Err(AppError::coded_with(
                    "auth_relogin_required",
                    [("accountId", account_id.to_string())],
                ))
            }
        }
    }

    pub fn remove_account(&self, account_id: &str) -> AppResult<()> {
        let record = self
            .storage
            .account(account_id)?
            .ok_or_else(|| AppError::AccountNotFound(account_id.to_string()))?;
        let previous = self.vault.get(&record.vault_ref)?;
        self.vault.delete(&record.vault_ref)?;
        if let Err(error) = self.storage.delete_account(account_id) {
            if let Some(previous) = previous {
                let restore = self.vault.put(&record.vault_ref, &previous);
                return match restore {
                    Ok(()) => Err(error),
                    Err(restore_error) => Err(AppError::coded_with(
                        "auth_remove_and_restore_failed",
                        [
                            ("primary", error.descriptor().code),
                            ("restore", restore_error.descriptor().code),
                        ],
                    )),
                };
            }
            return Err(error);
        }
        audit("Microsoft-Account lokal abgemeldet; Credential-Eintrag entfernt")?;
        Ok(())
    }

    pub fn assign_profile_account(
        &self,
        profile_id: &str,
        account_id: Option<&str>,
    ) -> AppResult<()> {
        self.storage.assign_profile_account(profile_id, account_id)
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    fn migrate_legacy_metadata(&self, legacy_accounts_path: &Path) -> AppResult<()> {
        if !self.storage.accounts()?.is_empty() {
            return Ok(());
        }
        if legacy_accounts_path.exists() {
            let index: LegacyAccountIndex =
                serde_json::from_str(&fs::read_to_string(legacy_accounts_path)?)?;
            for account in index.accounts {
                self.migrate_one_legacy_account(account)?;
            }
            if let Some(active) = index.active_account_id {
                if self.storage.account(&active)?.is_some() {
                    let _ = self.storage.select_account(&active)?;
                }
            }
            let migrated = legacy_accounts_path.with_file_name("accounts.phase3-migrated.json");
            if !migrated.exists() {
                fs::rename(legacy_accounts_path, migrated)?;
            }
            audit("Accountmetadaten wurden nach SQLite migriert")?;
            return Ok(());
        }

        if let Some(legacy) = self.vault.monolithic_legacy_session()? {
            self.insert_relogin_record(&legacy.account.id, &legacy.account.username)?;
            self.vault.delete_monolithic_legacy_session()?;
            audit("Alter Einzelaccount wurde ohne Übernahme unverifizierter Tokens migriert")?;
        }
        Ok(())
    }

    fn migrate_one_legacy_account(&self, account: LegacyAccountMetadata) -> AppResult<()> {
        let _ = self.vault.legacy_session(&account.id)?;
        self.insert_relogin_record(&account.id, &account.username)?;
        self.vault.delete_legacy_session(&account.id)?;
        Ok(())
    }

    fn insert_relogin_record(&self, account_id: &str, username: &str) -> AppResult<()> {
        let now = chrono::Utc::now().timestamp();
        let record = AccountRecord {
            id: account_id.to_string(),
            username: username.to_string(),
            account_kind: "microsoft".into(),
            vault_ref: new_identifier("missing-vault"),
            session_state: AccountSessionState::ReloginRequired.as_str().into(),
            ownership_verified_at_unix: 0,
            last_online_auth_at_unix: 0,
            added_at_unix: now,
            last_used_at_unix: now,
        };
        let _ = self.storage.upsert_account(&record)?;
        Ok(())
    }
}

fn account_from_record(record: AccountRecord) -> AppResult<Account> {
    if record.account_kind != "microsoft" {
        return Err(AppError::coded("account_kind_invalid"));
    }
    Ok(Account {
        id: record.id,
        username: record.username,
        kind: AccountKind::Microsoft,
        session_state: AccountSessionState::parse(&record.session_state),
        ownership_verified_at_unix: record.ownership_verified_at_unix,
        last_online_auth_at_unix: record.last_online_auth_at_unix,
        added_at_unix: record.added_at_unix,
        last_used_at_unix: record.last_used_at_unix,
    })
}

fn validate_identity(account_id: &str, username: &str, session: &AccountSession) -> AppResult<()> {
    let compact_id: String = account_id
        .chars()
        .filter(|character| *character != '-')
        .collect();
    if compact_id.len() != 32
        || !compact_id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(AppError::coded("auth_profile_id_invalid"));
    }
    if username.trim().is_empty() || username.len() > 64 {
        return Err(AppError::coded("auth_profile_name_invalid"));
    }
    if session.microsoft_refresh_token.trim().is_empty() {
        return Err(AppError::coded("auth_refresh_token_missing"));
    }
    Ok(())
}

fn audit(message: &str) -> AppResult<()> {
    #[cfg(test)]
    {
        let _ = message;
        Ok(())
    }
    #[cfg(not(test))]
    {
        crate::logging::append(message)
    }
}

#[derive(Debug, Default, Deserialize)]
struct LegacyAccountIndex {
    #[serde(default)]
    active_account_id: Option<String>,
    #[serde(default)]
    accounts: Vec<LegacyAccountMetadata>,
}

#[derive(Debug, Deserialize)]
struct LegacyAccountMetadata {
    id: String,
    username: String,
    #[allow(dead_code)]
    kind: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    last_used_at_unix: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LegacySession {
    account: LegacyAccount,
}

#[derive(Debug, Deserialize)]
struct LegacyAccount {
    id: String,
    username: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacySessionMeta {
    minecraft_expires_at_unix: i64,
    #[serde(default)]
    xuid: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::Mutex,
    };

    #[derive(Default)]
    struct MemoryVault {
        values: Mutex<BTreeMap<String, AccountSession>>,
        failing_deletes: Mutex<BTreeSet<String>>,
    }

    impl CredentialVault for MemoryVault {
        fn put(&self, vault_ref: &str, session: &AccountSession) -> AppResult<()> {
            self.values
                .lock()
                .expect("vault lock")
                .insert(vault_ref.to_string(), session.clone());
            Ok(())
        }

        fn get(&self, vault_ref: &str) -> AppResult<Option<AccountSession>> {
            Ok(self
                .values
                .lock()
                .expect("vault lock")
                .get(vault_ref)
                .cloned())
        }

        fn delete(&self, vault_ref: &str) -> AppResult<()> {
            if self
                .failing_deletes
                .lock()
                .expect("vault lock")
                .contains(vault_ref)
            {
                return Err(AppError::coded("credential_store_delete_failed"));
            }
            self.values.lock().expect("vault lock").remove(vault_ref);
            Ok(())
        }

        fn legacy_session(&self, _: &str) -> AppResult<Option<AccountSession>> {
            Ok(None)
        }

        fn delete_legacy_session(&self, _: &str) -> AppResult<()> {
            Ok(())
        }

        fn monolithic_legacy_session(&self) -> AppResult<Option<LegacySession>> {
            Ok(None)
        }

        fn delete_monolithic_legacy_session(&self) -> AppResult<()> {
            Ok(())
        }
    }

    fn fixture() -> (PathBuf, AuthStore, Arc<MemoryVault>) {
        let root = crate::foundation::test_root("auth-store");
        let storage = Storage::initialize_for_test(root.join("launcher.db")).expect("storage");
        let vault = Arc::new(MemoryVault::default());
        let store = AuthStore::new(storage, vault.clone());
        (root, store, vault)
    }

    fn session() -> AccountSession {
        AccountSession {
            microsoft_refresh_token: ["refresh", "-fixture"].concat(),
            minecraft_access_token: Some(["access", "-fixture"].concat()),
            minecraft_expires_at_unix: 500,
            xuid: Some("xuid-fixture".into()),
        }
    }

    #[test]
    fn secrets_live_only_in_the_vault_and_missing_entries_require_relogin() {
        let (root, store, vault) = fixture();
        let account = store
            .persist_authenticated(
                "0123456789abcdef0123456789abcdef",
                "VerifiedPlayer",
                &session(),
                100,
            )
            .expect("persist");
        let database_bytes = fs::read(store.storage().database_path()).expect("database");
        for forbidden in [
            ["refresh", "-fixture"].concat(),
            ["access", "-fixture"].concat(),
        ] {
            assert!(!database_bytes
                .windows(forbidden.len())
                .any(|window| window == forbidden.as_bytes()));
        }
        let record = store
            .storage()
            .account(&account.id)
            .expect("account")
            .expect("record");
        vault.delete(&record.vault_ref).expect("delete credential");
        let error = store.load_session(&account.id).expect_err("relogin error");
        assert_eq!(error.descriptor().code, "auth_relogin_required");
        assert_eq!(
            store
                .storage()
                .account(&account.id)
                .expect("account")
                .expect("record")
                .session_state,
            "relogin-required"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn logout_removes_metadata_and_vault_entry() {
        let (root, store, vault) = fixture();
        let account = store
            .persist_authenticated(
                "fedcba9876543210fedcba9876543210",
                "LogoutPlayer",
                &session(),
                100,
            )
            .expect("persist");
        let vault_ref = store
            .storage()
            .account(&account.id)
            .expect("account")
            .expect("record")
            .vault_ref;
        store.remove_account(&account.id).expect("remove");
        assert!(store
            .storage()
            .account(&account.id)
            .expect("query")
            .is_none());
        assert!(vault.get(&vault_ref).expect("vault").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn failed_old_credential_cleanup_rolls_back_metadata_and_new_secret() {
        let (root, store, vault) = fixture();
        let account_id = "0123456789abcdef0123456789abcdef";
        store
            .persist_authenticated(account_id, "OriginalPlayer", &session(), 100)
            .expect("first persist");
        let original = store
            .storage()
            .account(account_id)
            .expect("account")
            .expect("record");
        vault
            .failing_deletes
            .lock()
            .expect("vault lock")
            .insert(original.vault_ref.clone());

        let mut replacement = session();
        replacement.microsoft_refresh_token = ["replacement", "-fixture"].concat();
        let error = store
            .persist_authenticated(account_id, "ReplacementPlayer", &replacement, 200)
            .expect_err("rotation must fail");
        assert_eq!(error.descriptor().code, "auth_credential_rotation_failed");
        let restored = store
            .storage()
            .account(account_id)
            .expect("account")
            .expect("record");
        assert_eq!(restored.vault_ref, original.vault_ref);
        assert_eq!(restored.username, original.username);
        let values = vault.values.lock().expect("vault lock");
        assert_eq!(values.len(), 1);
        assert!(values.contains_key(&original.vault_ref));
        drop(values);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "manual Windows Credential Manager integration probe"]
    fn windows_credential_manager_round_trip() {
        let vault_ref = format!("diagnostic-{}", new_identifier("vault"));
        let session = AccountSession {
            // Together these values exceed one Windows Generic Credential,
            // while each individual protected field remains below its limit.
            microsoft_refresh_token: "r".repeat(1_000),
            minecraft_access_token: Some("m".repeat(1_000)),
            minecraft_expires_at_unix: 1_700_000_000,
            xuid: Some("1234567890123456".into()),
        };
        let vault = OsCredentialVault;
        vault
            .put(&vault_ref, &session)
            .expect("write split Windows Credential Manager session");
        let result = vault
            .get(&vault_ref)
            .expect("read split Windows Credential Manager session");
        vault
            .delete(&vault_ref)
            .expect("delete split Windows Credential Manager session");
        let result = result.expect("session exists");
        assert_eq!(
            result.microsoft_refresh_token,
            session.microsoft_refresh_token
        );
        assert_eq!(
            result.minecraft_access_token,
            session.minecraft_access_token
        );
        assert_eq!(
            result.minecraft_expires_at_unix,
            session.minecraft_expires_at_unix
        );
        assert_eq!(result.xuid, session.xuid);
    }
}
