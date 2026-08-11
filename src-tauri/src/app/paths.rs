use crate::{
    error::{AppError, AppResult},
    platform::{PlatformAdapter, SystemPlatform},
    security::{fs as secure_fs, paths::validate_existing_chain, PathRegistry},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct LauncherPaths {
    pub root: PathBuf,
    pub data: PathBuf,
    pub profiles: PathBuf,
    pub cache: PathBuf,
    pub cache_blobs: PathBuf,
    pub cache_quarantine: PathBuf,
    pub staging: PathBuf,
    pub staging_operations: PathBuf,
    pub migration: PathBuf,
    pub backups: PathBuf,
    pub exports: PathBuf,
    pub runtimes: PathBuf,
    pub logs: PathBuf,
    pub launcher_logs: PathBuf,
    pub database_file: PathBuf,
    pub settings_file: PathBuf,
    pub accounts_file: PathBuf,
    pub log_file: PathBuf,
    pub installation_file: PathBuf,
}

#[derive(Debug, Clone)]
pub struct GamePaths {
    pub root: PathBuf,
    pub assets: PathBuf,
    pub libraries: PathBuf,
    pub versions: PathBuf,
    pub natives: PathBuf,
    pub mods: PathBuf,
    pub logs: PathBuf,
}

impl LauncherPaths {
    pub fn from_platform(platform: &dyn PlatformAdapter) -> AppResult<Self> {
        Self::from_root(platform.data_root()?)
    }

    pub fn from_root(root: impl Into<PathBuf>) -> AppResult<Self> {
        let root = root.into();
        if root.as_os_str().is_empty() {
            return Err(AppError::coded("platform_data_root_empty"));
        }
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()?.join(root)
        };

        let data = root.join("data");
        let profiles = root.join("profiles");
        let cache = root.join("cache");
        let cache_blobs = cache.join("blobs").join("sha256");
        let cache_quarantine = cache.join("quarantine").join("sha256");
        let staging = root.join("staging");
        let staging_operations = staging.join("operations");
        let migration = root.join("migration");
        let backups = root.join("backups");
        let exports = root.join("exports");
        let runtimes = root.join("runtimes");
        let logs = root.join("logs");
        let launcher_logs = logs.join("launcher");

        let paths = Self {
            database_file: data.join("launcher.db"),
            settings_file: data.join("settings.json"),
            accounts_file: data.join("accounts.json"),
            log_file: launcher_logs.join("launcher.log"),
            installation_file: data.join("installation.json"),
            root,
            data,
            profiles,
            cache,
            cache_blobs,
            cache_quarantine,
            staging,
            staging_operations,
            migration,
            backups,
            exports,
            runtimes,
            logs,
            launcher_logs,
        };
        Ok(paths)
    }

    pub fn ensure_root(&self) -> AppResult<()> {
        let anchor = nearest_existing_ancestor(&self.root)?;
        validate_existing_chain(&anchor, &self.root)?;
        secure_fs::create_directories_within(&anchor, &self.root, &self.root)
    }

    pub fn create_registered_layout(&self, registry: &PathRegistry) -> AppResult<()> {
        for root_id in [
            "data",
            "profiles",
            "cache",
            "cache-blobs",
            "cache-quarantine",
            "staging-operations",
            "migration",
            "backups",
            "exports",
            "runtimes",
            "launcher-logs",
        ] {
            let directory = registry.root(root_id)?;
            secure_fs::create_directories_within(&self.root, directory, directory)?;
        }
        Ok(())
    }

    pub fn create_layout(&self) -> AppResult<()> {
        self.ensure_root()?;
        for directory in [
            &self.data,
            &self.profiles,
            &self.cache,
            &self.cache_blobs,
            &self.cache_quarantine,
            &self.staging,
            &self.staging_operations,
            &self.migration,
            &self.backups,
            &self.exports,
            &self.runtimes,
            &self.logs,
            &self.launcher_logs,
        ] {
            secure_fs::create_directories_within(&self.root, &self.root, directory)?;
        }
        Ok(())
    }
}

fn nearest_existing_ancestor(path: &Path) -> AppResult<PathBuf> {
    let mut current = path;
    loop {
        if current.exists() {
            return Ok(current.to_path_buf());
        }
        current = current
            .parent()
            .ok_or_else(|| AppError::coded("platform_data_root_has_no_existing_ancestor"))?;
    }
}

pub fn launcher_paths() -> AppResult<LauncherPaths> {
    let paths = LauncherPaths::from_platform(&SystemPlatform)?;
    paths.create_layout()?;
    Ok(paths)
}

pub fn default_game_directory() -> AppResult<PathBuf> {
    Ok(LauncherPaths::from_platform(&SystemPlatform)?
        .root
        .join("minecraft"))
}

pub fn game_paths(root: impl Into<PathBuf>) -> AppResult<GamePaths> {
    let root = root.into();
    let paths = GamePaths {
        assets: root.join("assets"),
        libraries: root.join("libraries"),
        versions: root.join("versions"),
        natives: root.join("natives"),
        mods: root.join("mods"),
        logs: root.join("logs"),
        root,
    };
    for dir in [
        &paths.root,
        &paths.assets,
        &paths.libraries,
        &paths.versions,
        &paths.natives,
        &paths.mods,
        &paths.logs,
    ] {
        fs::create_dir_all(dir)?;
    }
    Ok(paths)
}

#[cfg(test)]
mod phase1_tests {
    use super::*;

    #[test]
    fn fixed_root_layout_is_created_only_below_injected_root() {
        let root = std::env::temp_dir().join(format!(
            "s9lab-layout-test-{}-{}",
            std::process::id(),
            crate::operations::model::new_identifier("layout")
        ));
        let paths = LauncherPaths::from_root(&root).expect("paths");
        paths.create_layout().expect("layout");
        for expected in [
            "data",
            "profiles",
            "cache/blobs/sha256",
            "cache/quarantine/sha256",
            "staging/operations",
            "migration",
            "backups",
            "exports",
            "runtimes",
            "logs/launcher",
        ] {
            assert!(root.join(expected).is_dir(), "missing {expected}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_launcher_root_before_creating_children() {
        use std::os::unix::fs::symlink;

        let parent = std::env::temp_dir().join(format!(
            "s9lab-layout-link-test-{}-{}",
            std::process::id(),
            crate::operations::model::new_identifier("layout-link")
        ));
        let outside = parent.with_extension("outside");
        let linked_root = parent.join("launcher");
        std::fs::create_dir_all(&parent).expect("parent");
        std::fs::create_dir_all(&outside).expect("outside");
        symlink(&outside, &linked_root).expect("symlink");

        let paths = LauncherPaths::from_root(&linked_root).expect("paths");
        assert!(paths.create_layout().is_err());
        assert!(!outside.join("data").exists());

        let _ = std::fs::remove_file(linked_root);
        let _ = std::fs::remove_dir_all(parent);
        let _ = std::fs::remove_dir_all(outside);
    }
}
