use crate::{
    app::paths::LauncherPaths,
    cache::CacheStore,
    download::DownloadService,
    error::AppResult,
    operations::{engine::OperationEngine, recovery::RecoveryResult},
    platform::{FixedPlatform, PlatformAdapter, SystemPlatform},
    security::{PathRegistry, RegisteredRoot},
    storage::Storage,
};
use serde::Serialize;
use std::{path::Path, sync::Arc};

#[derive(Clone)]
pub struct CoreServices {
    platform_name: String,
    paths: LauncherPaths,
    registry: Arc<PathRegistry>,
    storage: Storage,
    operations: OperationEngine,
    download: DownloadService,
    cache: CacheStore,
    startup_recovery: Arc<Vec<RecoveryResult>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreStatus {
    pub schema_version: i64,
    pub platform: String,
    pub registered_roots: Vec<String>,
    pub incomplete_operations: usize,
    pub startup_recovered_operations: usize,
}

impl CoreServices {
    pub fn open_system() -> AppResult<Self> {
        Self::open(&SystemPlatform)
    }

    pub fn open_fixed(root: impl AsRef<Path>) -> AppResult<Self> {
        Self::open(&FixedPlatform::new(root.as_ref()))
    }

    #[cfg(test)]
    pub(crate) fn open_fixed_with_absolute_path_limit(
        root: impl AsRef<Path>,
        max_absolute_utf16: usize,
    ) -> AppResult<Self> {
        Self::open_with_absolute_path_limit(
            &FixedPlatform::new(root.as_ref()),
            Some(max_absolute_utf16),
        )
    }

    pub fn open(platform: &dyn PlatformAdapter) -> AppResult<Self> {
        Self::open_with_absolute_path_limit(platform, None)
    }

    fn open_with_absolute_path_limit(
        platform: &dyn PlatformAdapter,
        max_absolute_utf16: Option<usize>,
    ) -> AppResult<Self> {
        let paths = LauncherPaths::from_platform(platform)?;
        paths.ensure_root()?;
        let registered_roots = [
            RegisteredRoot {
                id: "data".into(),
                path: paths.data.clone(),
            },
            RegisteredRoot {
                id: "profiles".into(),
                path: paths.profiles.clone(),
            },
            RegisteredRoot {
                id: "cache".into(),
                path: paths.cache.clone(),
            },
            RegisteredRoot {
                id: "cache-blobs".into(),
                path: paths.cache_blobs.clone(),
            },
            RegisteredRoot {
                id: "cache-quarantine".into(),
                path: paths.cache_quarantine.clone(),
            },
            RegisteredRoot {
                id: "staging-operations".into(),
                path: paths.staging_operations.clone(),
            },
            RegisteredRoot {
                id: "migration".into(),
                path: paths.migration.clone(),
            },
            RegisteredRoot {
                id: "backups".into(),
                path: paths.backups.clone(),
            },
            RegisteredRoot {
                id: "launcher-logs".into(),
                path: paths.launcher_logs.clone(),
            },
        ];
        let registry = Arc::new(match max_absolute_utf16 {
            #[cfg(test)]
            Some(limit) => PathRegistry::new_for_tests(&paths.root, registered_roots, limit)?,
            #[cfg(not(test))]
            Some(_) => return Err(crate::error::AppError::coded("path_test_limit_unavailable")),
            None => PathRegistry::new(&paths.root, registered_roots)?,
        });
        paths.create_registered_layout(&registry)?;
        let database_file = registry.resolve("data", "launcher.db")?;
        let storage = Storage::initialize(&database_file)?;
        let operations = OperationEngine::new(paths.clone(), registry.clone(), storage.clone());
        let startup_recovery = Arc::new(operations.recover_incomplete()?);
        let download = DownloadService::production(registry.clone())?;
        let cache = CacheStore::new(registry.clone(), storage.clone());
        Ok(Self {
            platform_name: platform.platform_name().into(),
            paths,
            registry,
            storage,
            operations,
            download,
            cache,
            startup_recovery,
        })
    }

    pub fn status(&self) -> AppResult<CoreStatus> {
        Ok(CoreStatus {
            schema_version: self.storage.schema_version()?,
            platform: self.platform_name.clone(),
            registered_roots: self.registry.root_ids(),
            incomplete_operations: self.storage.incomplete_operations()?.len(),
            startup_recovered_operations: self.startup_recovery.len(),
        })
    }

    pub fn paths(&self) -> &LauncherPaths {
        &self.paths
    }

    pub fn registry(&self) -> &Arc<PathRegistry> {
        &self.registry
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn operations(&self) -> &OperationEngine {
        &self.operations
    }

    pub fn download(&self) -> &DownloadService {
        &self.download
    }

    pub fn cache(&self) -> &CacheStore {
        &self.cache
    }
}

#[cfg(test)]
pub fn test_root(_name: &str) -> std::path::PathBuf {
    const MAX_ATTEMPTS: usize = 32;
    for _ in 0..MAX_ATTEMPTS {
        let identifier = crate::operations::model::new_identifier("t");
        let token = identifier
            .rsplit('-')
            .next()
            .expect("generated test identifier has a digest");
        let root = std::env::temp_dir().join(format!("s9t-{}", &token[..12]));
        match std::fs::create_dir(&root) {
            Ok(()) => return root,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("create isolated test root {root:?}: {error}"),
        }
    }
    panic!("could not allocate a collision-free test root after {MAX_ATTEMPTS} attempts");
}
