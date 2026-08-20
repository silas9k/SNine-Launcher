use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

pub trait PlatformAdapter: Send + Sync {
    fn data_root(&self) -> AppResult<PathBuf>;
    fn platform_name(&self) -> &'static str;
}

#[derive(Debug, Clone, Default)]
pub struct SystemPlatform;

impl PlatformAdapter for SystemPlatform {
    fn data_root(&self) -> AppResult<PathBuf> {
        // Windows: keep all SNine launcher/profile/game data beside the normal
        // .minecraft location under %APPDATA%\SNineLauncher. This makes the
        // instance's saves, mods, config, resourcepacks and screenshots easy to find
        // and removes the old visible "S9Lab Launcher" storage root.
        let base = dirs::data_dir()
            .or_else(dirs::data_local_dir)
            .ok_or_else(|| AppError::coded("platform_data_root_unavailable"))?;
        let target = base.join("SNineLauncher");

        if !target.exists() {
            let mut legacy_roots = Vec::new();
            if let Some(local) = dirs::data_local_dir() {
                legacy_roots.push(local.join("S9Lab Launcher"));
                legacy_roots.push(local.join("S9Lab"));
            }
            if let Some(roaming) = dirs::data_dir() {
                legacy_roots.push(roaming.join("S9Lab Launcher"));
                legacy_roots.push(roaming.join("S9Lab"));
            }
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            for legacy in legacy_roots {
                if legacy == target || !legacy.exists() {
                    continue;
                }
                // A directory rename on the same Windows drive is effectively
                // instant and preserves worlds/accounts/profiles without copying.
                if std::fs::rename(&legacy, &target).is_ok() {
                    break;
                }
            }
        }

        Ok(target)
    }

    fn platform_name(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "unknown"
        }
    }
}

#[derive(Debug, Clone)]
pub struct FixedPlatform {
    root: PathBuf,
}

impl FixedPlatform {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl PlatformAdapter for FixedPlatform {
    fn data_root(&self) -> AppResult<PathBuf> {
        Ok(self.root.clone())
    }

    fn platform_name(&self) -> &'static str {
        "test"
    }
}

#[cfg(windows)]
pub fn atomic_replace(source: &Path, destination: &Path) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let source_wide = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(AppError::coded_with(
            "settings_write_failed",
            [("detail", std::io::Error::last_os_error().to_string())],
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn atomic_replace(source: &Path, destination: &Path) -> AppResult<()> {
    std::fs::rename(source, destination).map_err(AppError::from)
}
