use crate::{
    error::{AppError, AppResult},
    security::{fs as secure_fs, paths::validate_existing_chain, PathRegistry},
    storage::{models::CacheBlobRecord, Storage},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CacheGcReport {
    pub scanned_blobs: usize,
    pub reachable_blobs: usize,
    pub eligible_for_quarantine: usize,
    pub eligible_bytes: u64,
    pub quarantined_this_run: usize,
    pub restored_this_run: usize,
    pub retained_in_quarantine: usize,
    pub deletion_policy: String,
}

#[derive(Clone)]
pub struct CacheStore {
    registry: Arc<PathRegistry>,
    storage: Storage,
    mutation_lock: Arc<Mutex<()>>,
}

impl CacheStore {
    pub fn new(registry: Arc<PathRegistry>, storage: Storage) -> Self {
        Self {
            registry,
            storage,
            mutation_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn blob_relative_path(sha256: &str) -> AppResult<String> {
        validate_sha256(sha256)?;
        Ok(format!("{}/{}", &sha256[..2], sha256))
    }

    pub fn activate_verified_copy(
        &self,
        staging_relative_path: &str,
        expected_sha256: &str,
        expected_size: u64,
    ) -> AppResult<String> {
        validate_sha256(expected_sha256)?;
        let _guard = self.lock_mutation()?;
        let source = self
            .registry
            .resolve("staging-operations", staging_relative_path)?;
        let source_metadata = fs::metadata(source.absolute())?;
        if !source_metadata.is_file() || source_metadata.len() != expected_size {
            return Err(AppError::coded("cache_source_size_mismatch"));
        }
        let source_hash = hash_file(source.absolute())?;
        if source_hash != expected_sha256 {
            return Err(AppError::coded("cache_source_hash_mismatch"));
        }

        let relative = Self::blob_relative_path(expected_sha256)?;
        self.reactivate_if_quarantined(expected_sha256)?;
        let destination = self.registry.resolve("cache-blobs", &relative)?;
        if destination.absolute().exists() {
            let existing = fs::metadata(destination.absolute())?;
            if existing.len() != expected_size
                || hash_file(destination.absolute())? != expected_sha256
            {
                return Err(AppError::coded("cache_existing_blob_invalid"));
            }
        } else {
            let copied = secure_fs::copy_new(&source, &destination)?;
            if copied != expected_size {
                let _ = secure_fs::remove_tree(&destination);
                return Err(AppError::coded("cache_copy_size_mismatch"));
            }
            if hash_file(destination.absolute())? != expected_sha256 {
                let _ = secure_fs::remove_tree(&destination);
                return Err(AppError::coded("cache_copy_hash_mismatch"));
            }
            make_read_only(destination.absolute())?;
        }
        self.storage
            .insert_cache_blob(expected_sha256, expected_size, &relative, "verified")?;
        Ok(relative)
    }

    pub fn materialize_profile_copy(
        &self,
        profile_id: &str,
        blob_sha256: &str,
        instance_relative_path: &str,
    ) -> AppResult<()> {
        validate_sha256(blob_sha256)?;
        let _guard = self.lock_mutation()?;
        let blob = self
            .storage
            .cache_blob(blob_sha256)?
            .ok_or_else(|| AppError::coded("cache_blob_missing"))?;
        if blob.state != "verified" {
            return Err(AppError::coded("cache_blob_not_verified"));
        }
        let source = self.registry.resolve("cache-blobs", &blob.relative_path)?;
        let destination = self.registry.resolve(
            "profiles",
            format!("{profile_id}/instance/{instance_relative_path}"),
        )?;
        let copied = secure_fs::copy_new(&source, &destination)?;
        if copied != blob.size_bytes || hash_file(destination.absolute())? != blob.sha256 {
            let _ = secure_fs::remove_tree(&destination);
            return Err(AppError::coded("profile_cache_copy_verification_failed"));
        }
        Ok(())
    }

    pub fn gc_preview(&self) -> AppResult<CacheGcReport> {
        let _guard = self.lock_mutation()?;
        let reachable = self.discover_references()?;
        let blobs = self.storage.cache_blobs()?;
        Ok(build_gc_report(&blobs, &reachable, 0, 0))
    }

    pub fn quarantine_unreferenced(&self) -> AppResult<CacheGcReport> {
        let _guard = self.lock_mutation()?;
        let first_mark = self.discover_references()?;
        let mut restored = 0usize;
        for blob in self.storage.cache_blobs()? {
            if blob.state == "quarantined" && first_mark.contains(&blob.sha256) {
                self.reactivate_blob(&blob)?;
                restored += 1;
            }
        }

        let second_mark = self.discover_references()?;
        let candidates: Vec<CacheBlobRecord> = self
            .storage
            .cache_blobs()?
            .into_iter()
            .filter(|blob| {
                blob.state == "verified"
                    && !first_mark.contains(&blob.sha256)
                    && !second_mark.contains(&blob.sha256)
            })
            .collect();
        let mut quarantined = 0usize;
        for blob in candidates {
            if self.discover_references()?.contains(&blob.sha256) {
                continue;
            }
            self.quarantine_blob(&blob)?;
            quarantined += 1;
        }
        let reachable = self.discover_references()?;
        let blobs = self.storage.cache_blobs()?;
        Ok(build_gc_report(&blobs, &reachable, quarantined, restored))
    }

    fn lock_mutation(&self) -> AppResult<MutexGuard<'_, ()>> {
        self.mutation_lock
            .lock()
            .map_err(|_| AppError::coded("cache_mutation_lock_poisoned"))
    }

    fn discover_references(&self) -> AppResult<BTreeSet<String>> {
        let mut reachable: BTreeSet<String> =
            self.storage.cache_reference_hashes()?.into_iter().collect();
        for operation in self.storage.incomplete_operations()? {
            collect_hashes(operation.planned_changes_json.as_bytes(), &mut reachable);
        }
        for root_id in ["profiles", "backups", "staging-operations"] {
            let root = self.registry.root(root_id)?;
            collect_hashes_from_tree(root, root, &mut reachable)?;
        }
        Ok(reachable)
    }

    fn reactivate_if_quarantined(&self, sha256: &str) -> AppResult<()> {
        if let Some(blob) = self.storage.cache_blob(sha256)? {
            if blob.state == "quarantined" {
                self.reactivate_blob(&blob)?;
            }
        }
        Ok(())
    }

    fn reactivate_blob(&self, blob: &CacheBlobRecord) -> AppResult<()> {
        let quarantine_relative = blob
            .quarantine_relative_path
            .as_deref()
            .ok_or_else(|| AppError::coded("cache_quarantine_metadata_missing"))?;
        let source = self
            .registry
            .resolve("cache-quarantine", quarantine_relative)?;
        let destination = self.registry.resolve("cache-blobs", &blob.relative_path)?;
        verify_cache_file(source.absolute(), blob)?;
        let cache_root = self.registry.root("cache")?;
        secure_fs::rename_new_within_parent(&source, &destination, cache_root)?;
        if let Err(primary) = self.storage.mark_cache_reactivated(&blob.sha256) {
            let rollback = secure_fs::rename_new_within_parent(&destination, &source, cache_root);
            return Err(combine_cache_cleanup_error(
                "cache_reactivation_and_rollback_failed",
                primary,
                rollback,
            ));
        }
        Ok(())
    }

    fn quarantine_blob(&self, blob: &CacheBlobRecord) -> AppResult<()> {
        let source = self.registry.resolve("cache-blobs", &blob.relative_path)?;
        verify_cache_file(source.absolute(), blob)?;
        let quarantine_relative = Self::blob_relative_path(&blob.sha256)?;
        let destination = self
            .registry
            .resolve("cache-quarantine", &quarantine_relative)?;
        let cache_root = self.registry.root("cache")?;
        secure_fs::rename_new_within_parent(&source, &destination, cache_root)?;
        if let Err(primary) = self
            .storage
            .mark_cache_quarantined(&blob.sha256, &quarantine_relative)
        {
            let rollback = secure_fs::rename_new_within_parent(&destination, &source, cache_root);
            return Err(combine_cache_cleanup_error(
                "cache_quarantine_and_rollback_failed",
                primary,
                rollback,
            ));
        }
        Ok(())
    }
}

fn build_gc_report(
    blobs: &[CacheBlobRecord],
    reachable: &BTreeSet<String>,
    quarantined_this_run: usize,
    restored_this_run: usize,
) -> CacheGcReport {
    let eligible: Vec<&CacheBlobRecord> = blobs
        .iter()
        .filter(|blob| blob.state == "verified" && !reachable.contains(&blob.sha256))
        .collect();
    CacheGcReport {
        scanned_blobs: blobs.len(),
        reachable_blobs: blobs
            .iter()
            .filter(|blob| reachable.contains(&blob.sha256))
            .count(),
        eligible_for_quarantine: eligible.len(),
        eligible_bytes: eligible
            .iter()
            .fold(0u64, |total, blob| total.saturating_add(blob.size_bytes)),
        quarantined_this_run,
        restored_this_run,
        retained_in_quarantine: blobs
            .iter()
            .filter(|blob| blob.state == "quarantined")
            .count(),
        deletion_policy: "unconfigured".into(),
    }
}

fn collect_hashes_from_tree(
    anchor: &Path,
    directory: &Path,
    reachable: &mut BTreeSet<String>,
) -> AppResult<()> {
    validate_existing_chain(anchor, directory)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::coded("cache_gc_symlink_forbidden"));
        }
        if metadata.is_dir() {
            collect_hashes_from_tree(anchor, &entry.path(), reachable)?;
        } else if metadata.is_file() {
            validate_existing_chain(anchor, &entry.path())?;
            let mut file = fs::File::open(entry.path())?;
            let mut overlap = Vec::new();
            let mut chunk = [0u8; 64 * 1024];
            loop {
                let read = file.read(&mut chunk)?;
                if read == 0 {
                    break;
                }
                overlap.extend_from_slice(&chunk[..read]);
                collect_hashes(&overlap, reachable);
                if overlap.len() > 63 {
                    overlap.drain(..overlap.len() - 63);
                }
            }
        } else {
            return Err(AppError::coded("cache_gc_special_file_forbidden"));
        }
    }
    Ok(())
}

fn collect_hashes(bytes: &[u8], reachable: &mut BTreeSet<String>) {
    for window in bytes.windows(64) {
        if window
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            if let Ok(value) = std::str::from_utf8(window) {
                reachable.insert(value.to_string());
            }
        }
    }
}

fn verify_cache_file(path: &Path, blob: &CacheBlobRecord) -> AppResult<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() != blob.size_bytes || hash_file(path)? != blob.sha256 {
        return Err(AppError::coded("cache_blob_integrity_failed"));
    }
    Ok(())
}

fn combine_cache_cleanup_error(code: &str, primary: AppError, rollback: AppResult<()>) -> AppError {
    match rollback {
        Ok(()) => primary,
        Err(rollback) => AppError::coded_with(
            code,
            [
                ("primary", primary.descriptor().code),
                ("rollback", rollback.descriptor().code),
            ],
        ),
    }
}

fn validate_sha256(value: &str) -> AppResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(AppError::coded("cache_sha256_invalid"));
    }
    Ok(())
}

fn hash_file(path: &std::path::Path) -> AppResult<String> {
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

fn make_read_only(path: &std::path::Path) -> AppResult<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{foundation::CoreServices, profiles::service::ProfileService};

    fn activate_fixture(core: &CoreServices, name: &str, bytes: &[u8]) -> String {
        let staging = format!("cache-{name}/source.bin");
        let source = core
            .registry()
            .resolve("staging-operations", &staging)
            .expect("source");
        secure_fs::write_new(&source, bytes).expect("write");
        let hash = crate::operations::model::sha256_hex(bytes);
        core.cache()
            .activate_verified_copy(&staging, &hash, bytes.len() as u64)
            .expect("activate");
        hash
    }

    #[test]
    fn cache_uses_verified_copies_not_hardlinks() {
        let root = crate::foundation::test_root("cache-copy");
        let core = CoreServices::open_fixed(&root).expect("core");
        let source = core
            .registry()
            .resolve("staging-operations", "cache-test/source.bin")
            .expect("source");
        secure_fs::write_new(&source, b"cache fixture").expect("write");
        let hash = crate::operations::model::sha256_hex(b"cache fixture");
        let relative = core
            .cache()
            .activate_verified_copy("cache-test/source.bin", &hash, 13)
            .expect("activate");
        let destination = core
            .registry()
            .resolve("cache-blobs", relative)
            .expect("destination");
        assert_eq!(
            fs::read(destination.absolute()).expect("read"),
            b"cache fixture"
        );
        assert_eq!(
            fs::read(source.absolute()).expect("read source"),
            b"cache fixture"
        );
        fs::write(source.absolute(), b"source changed").expect("change source");
        assert_eq!(
            fs::read(destination.absolute()).expect("read unchanged cache"),
            b"cache fixture"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cache_gc_marks_all_authorities_and_only_quarantines_unreferenced_blobs() {
        let root = crate::foundation::test_root("cache-gc");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let profile = profiles.create_profile("GC Profile").expect("profile");
        let profile_hash = activate_fixture(&core, "profile", b"profile-referenced");
        let backup_hash = activate_fixture(&core, "backup", b"backup-referenced");
        let unreferenced_hash = activate_fixture(&core, "unreferenced", b"unreferenced");

        let profile_reference = root
            .join("profiles")
            .join(&profile.id)
            .join("instance/config/cache-reference.json");
        fs::write(
            &profile_reference,
            format!("{{\"sha256\":\"{profile_hash}\"}}"),
        )
        .expect("profile reference");
        profiles.archive_profile(&profile.id).expect("archive");
        profiles.trash_profile(&profile.id).expect("trash");
        fs::write(
            root.join("backups/cache-reference.json"),
            format!("{{\"sha256\":\"{backup_hash}\"}}"),
        )
        .expect("backup reference");

        let preview = core.cache().gc_preview().expect("preview");
        assert_eq!(preview.eligible_for_quarantine, 1);
        assert_eq!(preview.deletion_policy, "unconfigured");
        let swept = core.cache().quarantine_unreferenced().expect("quarantine");
        assert_eq!(swept.quarantined_this_run, 1);
        assert_eq!(swept.retained_in_quarantine, 1);
        let quarantined = core
            .storage()
            .cache_blob(&unreferenced_hash)
            .expect("query")
            .expect("blob");
        assert_eq!(quarantined.state, "quarantined");
        assert!(root
            .join("cache/quarantine/sha256")
            .join(
                quarantined
                    .quarantine_relative_path
                    .expect("quarantine path")
            )
            .is_file());

        core.storage()
            .add_cache_reference(&unreferenced_hash, "backup", "late-reference")
            .expect("late reference");
        let restored = core
            .cache()
            .quarantine_unreferenced()
            .expect("restore referenced quarantine");
        assert_eq!(restored.restored_this_run, 1);
        assert_eq!(restored.retained_in_quarantine, 0);
        assert_eq!(
            core.storage()
                .cache_blob(&unreferenced_hash)
                .expect("query")
                .expect("blob")
                .state,
            "verified"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn profile_materialization_is_a_verified_independent_copy() {
        let root = crate::foundation::test_root("cache-profile-copy");
        let core = CoreServices::open_fixed(&root).expect("core");
        let profiles = ProfileService::from_core(&core);
        let profile = profiles.create_profile("Copy Profile").expect("profile");
        let hash = activate_fixture(&core, "profile-copy", b"immutable-cache");
        core.cache()
            .materialize_profile_copy(&profile.id, &hash, "mods/example.jar")
            .expect("profile copy");
        let blob = core
            .storage()
            .cache_blob(&hash)
            .expect("query")
            .expect("blob");
        let cache_file = root.join("cache/blobs/sha256").join(blob.relative_path);
        let profile_file = root
            .join("profiles")
            .join(profile.id)
            .join("instance/mods/example.jar");
        fs::write(&profile_file, b"locally-modified").expect("mutate profile copy");
        assert_eq!(fs::read(&cache_file).expect("cache"), b"immutable-cache");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_ne!(
                fs::metadata(cache_file).expect("cache metadata").ino(),
                fs::metadata(profile_file).expect("profile metadata").ino()
            );
        }
        let _ = fs::remove_dir_all(root);
    }
}
