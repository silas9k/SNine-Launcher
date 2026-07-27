use crate::{
    error::{AppError, AppResult},
    storage::sqlite::{Connection, Value},
};
use chrono::Utc;

pub const LATEST_SCHEMA_VERSION: i64 = 5;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "phase1_core_schema",
        sql: r#"
CREATE TABLE profiles (
    id TEXT PRIMARY KEY NOT NULL,
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN ('active', 'archived', 'trash')),
    active_revision_id TEXT NULL,
    created_at_unix INTEGER NOT NULL,
    updated_at_unix INTEGER NOT NULL,
    archived_at_unix INTEGER NULL,
    deleted_at_unix INTEGER NULL
);

CREATE TABLE profile_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    manifest_sha256 TEXT NOT NULL CHECK (length(manifest_sha256) = 64),
    lock_sha256 TEXT NOT NULL CHECK (length(lock_sha256) = 64),
    manifest_relative_path TEXT NOT NULL,
    lock_relative_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('committed', 'invalidated')),
    created_at_unix INTEGER NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (operation_id) REFERENCES operations(id) ON DELETE RESTRICT,
    UNIQUE(profile_id, manifest_sha256, lock_sha256)
);

CREATE TABLE operations (
    id TEXT PRIMARY KEY NOT NULL,
    operation_type TEXT NOT NULL,
    profile_id TEXT NULL,
    state TEXT NOT NULL CHECK (state IN (
        'planned', 'staging', 'verifying', 'ready-to-commit', 'committing',
        'validating', 'completed', 'rolling-back', 'rolled-back', 'failed'
    )),
    planned_changes_json TEXT NOT NULL,
    staging_relative_path TEXT NOT NULL,
    previous_revision_id TEXT NULL,
    target_revision_id TEXT NULL,
    started_at_unix INTEGER NOT NULL,
    completed_at_unix INTEGER NULL,
    error_code TEXT NULL,
    error_params_json TEXT NULL,
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE RESTRICT
);

CREATE TABLE operation_journal (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    step TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('started', 'completed', 'compensated')),
    details_json TEXT NOT NULL,
    compensation_json TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL,
    FOREIGN KEY (operation_id) REFERENCES operations(id) ON DELETE CASCADE,
    UNIQUE(operation_id, sequence)
);

CREATE TABLE cache_blobs (
    sha256 TEXT PRIMARY KEY NOT NULL CHECK (length(sha256) = 64),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    relative_path TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('staged', 'verified', 'quarantined')),
    created_at_unix INTEGER NOT NULL,
    last_verified_at_unix INTEGER NULL
);

CREATE TABLE cache_references (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    blob_sha256 TEXT NOT NULL,
    owner_type TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL,
    FOREIGN KEY (blob_sha256) REFERENCES cache_blobs(sha256) ON DELETE RESTRICT,
    UNIQUE(blob_sha256, owner_type, owner_id)
);
"#,
    },
    Migration {
        version: 2,
        name: "phase1_indexes_and_guards",
        sql: r#"
CREATE INDEX idx_profile_revisions_profile ON profile_revisions(profile_id, created_at_unix DESC);
CREATE INDEX idx_operations_state ON operations(state, started_at_unix);
CREATE INDEX idx_operation_journal_operation ON operation_journal(operation_id, sequence);
CREATE INDEX idx_cache_references_owner ON cache_references(owner_type, owner_id);

CREATE TRIGGER profile_active_revision_belongs_to_profile
BEFORE UPDATE OF active_revision_id ON profiles
WHEN NEW.active_revision_id IS NOT NULL
BEGIN
    SELECT CASE
        WHEN NOT EXISTS (
            SELECT 1 FROM profile_revisions
            WHERE id = NEW.active_revision_id
              AND profile_id = NEW.id
              AND status = 'committed'
        )
        THEN RAISE(ABORT, 'active revision does not belong to profile')
    END;
END;
"#,
    },
    Migration {
        version: 3,
        name: "phase1_state_machine_and_revision_insert_guard",
        sql: r#"
CREATE TRIGGER profile_active_revision_insert_guard
BEFORE INSERT ON profiles
WHEN NEW.active_revision_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'profiles must be created without an active revision');
END;

CREATE TRIGGER operation_state_transition_guard
BEFORE UPDATE OF state ON operations
WHEN OLD.state <> NEW.state
BEGIN
    SELECT CASE
        WHEN NOT (
            (OLD.state = 'planned' AND NEW.state IN ('staging', 'rolling-back', 'failed')) OR
            (OLD.state = 'staging' AND NEW.state IN ('verifying', 'rolling-back', 'failed')) OR
            (OLD.state = 'verifying' AND NEW.state IN ('ready-to-commit', 'rolling-back', 'failed')) OR
            (OLD.state = 'ready-to-commit' AND NEW.state IN ('committing', 'rolling-back', 'failed')) OR
            (OLD.state = 'committing' AND NEW.state IN ('validating', 'completed', 'rolling-back', 'failed')) OR
            (OLD.state = 'validating' AND NEW.state IN ('completed', 'rolling-back', 'failed')) OR
            (OLD.state = 'rolling-back' AND NEW.state IN ('rolled-back', 'failed'))
        )
        THEN RAISE(ABORT, 'invalid operation state transition')
    END;
END;
"#,
    },
    Migration {
        version: 4,
        name: "phase3_account_metadata_and_profile_assignment",
        sql: r#"
CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL CHECK (length(trim(username)) BETWEEN 1 AND 64),
    account_kind TEXT NOT NULL CHECK (account_kind = 'microsoft'),
    vault_ref TEXT NOT NULL UNIQUE CHECK (length(vault_ref) BETWEEN 16 AND 128),
    session_state TEXT NOT NULL CHECK (session_state IN ('active', 'relogin-required')),
    ownership_verified_at_unix INTEGER NOT NULL CHECK (ownership_verified_at_unix >= 0),
    last_online_auth_at_unix INTEGER NOT NULL CHECK (last_online_auth_at_unix >= 0),
    added_at_unix INTEGER NOT NULL,
    last_used_at_unix INTEGER NOT NULL
);

CREATE TABLE launcher_account_selection (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    active_account_id TEXT NULL,
    updated_at_unix INTEGER NOT NULL,
    FOREIGN KEY (active_account_id) REFERENCES accounts(id) ON DELETE SET NULL
);

INSERT INTO launcher_account_selection(singleton, active_account_id, updated_at_unix)
VALUES (1, NULL, 0);

CREATE TABLE profile_account_assignments (
    profile_id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL,
    assigned_at_unix INTEGER NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX idx_accounts_last_used ON accounts(last_used_at_unix DESC, id);
CREATE INDEX idx_profile_account_assignments_account ON profile_account_assignments(account_id, profile_id);
"#,
    },
    Migration {
        version: 5,
        name: "phase4_profile_lifecycle_and_cache_quarantine",
        sql: r#"
CREATE TABLE profile_metadata (
    profile_id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL CHECK (length(trim(display_name)) BETWEEN 1 AND 64),
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    verification_state TEXT NOT NULL DEFAULT 'verified'
        CHECK (verification_state IN ('verified', 'unverified')),
    trashed_from_state TEXT NULL CHECK (trashed_from_state IN ('active', 'archived')),
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE
);

INSERT INTO profile_metadata(
    profile_id, display_name, favorite, verification_state, trashed_from_state
)
SELECT id, 'Profile ' || substr(id, 1, 8), 0, 'verified', NULL FROM profiles;

CREATE TABLE profile_lineage (
    profile_id TEXT PRIMARY KEY NOT NULL,
    source_profile_id TEXT NULL,
    duplicated_at_unix INTEGER NOT NULL,
    FOREIGN KEY (profile_id) REFERENCES profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (source_profile_id) REFERENCES profiles(id) ON DELETE SET NULL
);

INSERT INTO profile_lineage(profile_id, source_profile_id, duplicated_at_unix)
SELECT id, NULL, created_at_unix FROM profiles;

CREATE TABLE cache_quarantine (
    blob_sha256 TEXT PRIMARY KEY NOT NULL,
    quarantine_relative_path TEXT NOT NULL UNIQUE,
    quarantined_at_unix INTEGER NOT NULL,
    deletion_policy TEXT NOT NULL CHECK (deletion_policy = 'unconfigured'),
    FOREIGN KEY (blob_sha256) REFERENCES cache_blobs(sha256) ON DELETE RESTRICT
);

CREATE INDEX idx_profile_metadata_library
ON profile_metadata(favorite DESC, display_name, profile_id);

CREATE INDEX idx_profiles_lifecycle_updated
ON profiles(lifecycle_state, updated_at_unix DESC, id);

CREATE INDEX idx_cache_blobs_state_created
ON cache_blobs(state, created_at_unix, sha256);
"#,
    },
];

pub fn apply_all(connection: &Connection) -> AppResult<i64> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (\
            version INTEGER PRIMARY KEY NOT NULL,\
            name TEXT NOT NULL,\
            applied_at_unix INTEGER NOT NULL\
        );",
    )?;

    let current = current_version(connection)?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(AppError::coded_with(
            "storage_schema_too_new",
            [
                ("current", current.to_string()),
                ("supported", LATEST_SCHEMA_VERSION.to_string()),
            ],
        ));
    }

    for migration in MIGRATIONS.iter().filter(|item| item.version > current) {
        connection.transaction(|transaction| {
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, name, applied_at_unix) VALUES (?1, ?2, ?3)",
                &[
                    Value::Integer(migration.version),
                    Value::from(migration.name),
                    Value::Integer(Utc::now().timestamp()),
                ],
            )?;
            Ok(())
        })?;
    }

    current_version(connection)
}

pub fn current_version(connection: &Connection) -> AppResult<i64> {
    let row = connection.query_one(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        &[],
    )?;
    row.map(|row| row.integer(0)).unwrap_or(Ok(0))
}
