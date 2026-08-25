const SETTINGS_VERSION: u32 = 4;

use crate::{
    app::paths,
    error::{AppError, AppResult},
    platform,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, MutexGuard,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_GAME_VERSION: &str = "1.21.11";
const SETTINGS_TEMP_ATTEMPTS: usize = 16;
static SETTINGS_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
// Protects the process-local launcher settings target across temporary writing,
// flushing, atomic replacement and failure cleanup. Cross-process atomicity
// continues to be provided by platform::atomic_replace.
static SETTINGS_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherSettings {
    #[serde(default = "default_settings_version")]
    pub settings_version: u32,
    #[serde(default)]
    pub ultimate_installer_mode: bool,

    pub game_version: String,
    pub memory_mb: u32,
    pub java_path: Option<String>,
    pub game_directory: String,
    pub close_on_launch: bool,
    #[serde(default)]
    pub show_minecraft_snapshots: bool,
    #[serde(default)]
    pub show_old_minecraft_versions: bool,

    #[serde(default = "default_accent_color")]
    pub accent_color: String,
    #[serde(default = "default_background_style")]
    pub background_style: String,
    #[serde(default = "default_ui_density")]
    pub ui_density: String,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default = "default_glow_intensity")]
    pub glow_intensity: u8,
    #[serde(default = "default_panel_style")]
    pub panel_style: String,
    #[serde(default = "default_corner_style")]
    pub corner_style: String,
    #[serde(default)]
    pub sidebar_labels: bool,
    #[serde(default = "default_skin_scale")]
    pub skin_scale: u16,
    #[serde(default = "default_skin_pose")]
    pub skin_pose: String,
    #[serde(default = "default_true")]
    pub skin_animation: bool,
    #[serde(default = "default_secondary_accent")]
    pub secondary_accent: String,
    #[serde(default = "default_surface_opacity")]
    pub surface_opacity: u8,
    #[serde(default = "default_true")]
    pub background_motion: bool,

    #[serde(default = "default_appearance")]
    pub appearance: String,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default = "default_navigation_mode")]
    pub navigation_mode: String,
    #[serde(default = "default_background_variant")]
    pub background_variant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShellSettings {
    pub appearance: String,
    pub locale: String,
    pub accent_color: String,
    pub density: String,
    pub navigation_mode: String,
    pub background_variant: String,
    pub reduced_motion: bool,
    pub show_minecraft_snapshots: bool,
    pub show_old_minecraft_versions: bool,
}

impl LauncherSettings {
    pub fn defaults() -> AppResult<Self> {
        let game_directory = paths::default_game_directory()?
            .to_string_lossy()
            .to_string();
        Ok(Self::defaults_for_game_directory(game_directory))
    }

    fn defaults_for_game_directory(game_directory: String) -> Self {
        Self {
            settings_version: SETTINGS_VERSION,
            ultimate_installer_mode: false,
            game_version: DEFAULT_GAME_VERSION.to_string(),
            memory_mb: 4096,
            java_path: None,
            game_directory,
            close_on_launch: false,
            show_minecraft_snapshots: false,
            show_old_minecraft_versions: false,
            accent_color: default_accent_color(),
            background_style: default_background_style(),
            ui_density: default_ui_density(),
            reduced_motion: false,
            glow_intensity: default_glow_intensity(),
            panel_style: default_panel_style(),
            corner_style: default_corner_style(),
            sidebar_labels: true,
            skin_scale: default_skin_scale(),
            skin_pose: default_skin_pose(),
            skin_animation: true,
            secondary_accent: default_secondary_accent(),
            surface_opacity: default_surface_opacity(),
            background_motion: true,
            appearance: default_appearance(),
            locale: default_locale(),
            navigation_mode: default_navigation_mode(),
            background_variant: default_background_variant(),
        }
    }

    fn normalize(mut self) -> AppResult<Self> {
        self.settings_version = SETTINGS_VERSION;
        self.memory_mb = self.memory_mb.clamp(2048, 16384);
        self.accent_color = normalize_accent(&self.accent_color)?;
        ensure_value(
            "appearance",
            &self.appearance,
            &["system", "light", "dark", "contrast"],
        )?;
        ensure_value("locale", &self.locale, &["system", "de", "en"])?;
        ensure_value("uiDensity", &self.ui_density, &["compact", "comfortable"])?;
        ensure_value(
            "navigationMode",
            &self.navigation_mode,
            &["compact", "expanded"],
        )?;
        ensure_value(
            "backgroundVariant",
            &self.background_variant,
            &["calm", "grid", "terrain"],
        )?;
        Ok(self)
    }

    pub fn shell_settings(&self) -> ShellSettings {
        ShellSettings {
            appearance: self.appearance.clone(),
            locale: self.locale.clone(),
            accent_color: self.accent_color.clone(),
            density: self.ui_density.clone(),
            navigation_mode: self.navigation_mode.clone(),
            background_variant: self.background_variant.clone(),
            reduced_motion: self.reduced_motion,
            show_minecraft_snapshots: self.show_minecraft_snapshots,
            show_old_minecraft_versions: self.show_old_minecraft_versions,
        }
    }

    pub fn apply_shell_settings(mut self, shell: ShellSettings) -> AppResult<Self> {
        self.appearance = shell.appearance;
        self.locale = shell.locale;
        self.accent_color = shell.accent_color;
        self.ui_density = shell.density;
        self.navigation_mode = shell.navigation_mode;
        self.sidebar_labels = self.navigation_mode == "expanded";
        self.background_variant = shell.background_variant;
        self.reduced_motion = shell.reduced_motion;
        self.show_minecraft_snapshots = shell.show_minecraft_snapshots;
        self.show_old_minecraft_versions = shell.show_old_minecraft_versions;
        self.normalize()
    }
}

fn normalize_accent(value: &str) -> AppResult<String> {
    if value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Ok(value.to_ascii_lowercase());
    }
    Err(AppError::coded_with(
        "settings_invalid_accent",
        [("value", value.to_string())],
    ))
}

fn ensure_value(field: &str, value: &str, allowed: &[&str]) -> AppResult<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(AppError::coded_with(
            "settings_invalid_value",
            [("field", field.to_string()), ("value", value.to_string())],
        ))
    }
}

fn default_accent_color() -> String {
    "#c83f49".into()
}

fn default_secondary_accent() -> String {
    "#7c5cff".into()
}

fn default_background_style() -> String {
    "void".into()
}

fn default_ui_density() -> String {
    "comfortable".into()
}

fn default_glow_intensity() -> u8 {
    65
}

fn default_surface_opacity() -> u8 {
    82
}

fn default_skin_scale() -> u16 {
    100
}

fn default_panel_style() -> String {
    "glass".into()
}

fn default_corner_style() -> String {
    "soft".into()
}

fn default_skin_pose() -> String {
    "hero".into()
}

fn default_true() -> bool {
    true
}

fn default_appearance() -> String {
    "system".into()
}

fn default_locale() -> String {
    "system".into()
}

fn default_navigation_mode() -> String {
    "expanded".into()
}

fn default_background_variant() -> String {
    "calm".into()
}

fn default_settings_version() -> u32 {
    SETTINGS_VERSION
}

pub fn load_settings() -> AppResult<LauncherSettings> {
    load_settings_from(&paths::launcher_paths()?.settings_file)
}

pub fn load_settings_from(file: &Path) -> AppResult<LauncherSettings> {
    if !file.exists() {
        let defaults = LauncherSettings::defaults()?;
        save_settings_to(file, &defaults)?;
        return Ok(defaults);
    }
    let raw = fs::read_to_string(file)?;
    let json: serde_json::Value = serde_json::from_str(&raw)?;
    let migrated = migrate_settings(json);
    let parsed: LauncherSettings = serde_json::from_value(migrated)?;
    parsed.normalize()
}

pub fn save_settings(settings: &LauncherSettings) -> AppResult<LauncherSettings> {
    save_settings_to(&paths::launcher_paths()?.settings_file, settings)
}

pub fn save_settings_to(file: &Path, settings: &LauncherSettings) -> AppResult<LauncherSettings> {
    let normalized = settings.clone().normalize()?;
    let _write_guard = acquire_settings_write_lock(&SETTINGS_WRITE_LOCK)?;
    let parent = file
        .parent()
        .ok_or_else(|| AppError::coded("settings_parent_missing"))?;
    fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(&normalized)?;
    let (temp, mut handle) = create_settings_temp_file(parent)?;
    let result = (|| -> AppResult<()> {
        handle.write_all(&bytes)?;
        handle.write_all(b"\n")?;
        handle.sync_all()?;
        drop(handle);
        platform::atomic_replace(&temp, file)?;
        #[cfg(unix)]
        {
            OpenOptions::new().read(true).open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(normalized)
}

fn acquire_settings_write_lock(lock: &Mutex<()>) -> AppResult<MutexGuard<'_, ()>> {
    resolve_settings_write_lock(lock.lock())
}

fn resolve_settings_write_lock<'a>(
    result: std::sync::LockResult<MutexGuard<'a, ()>>,
) -> AppResult<MutexGuard<'a, ()>> {
    result.map_err(|_| AppError::coded("settings_write_lock_poisoned"))
}

fn create_settings_temp_file(parent: &Path) -> AppResult<(PathBuf, fs::File)> {
    for _ in 0..SETTINGS_TEMP_ATTEMPTS {
        let counter = SETTINGS_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp = parent.join(format!(
            ".settings-{:x}-{counter:x}-{nonce:x}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(handle) => return Ok((temp, handle)),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::coded_with(
        "settings_write_failed",
        [(
            "detail",
            format!(
                "temporary settings filename collision after {SETTINGS_TEMP_ATTEMPTS} attempts"
            ),
        )],
    ))
}

fn migrate_settings(mut old: serde_json::Value) -> serde_json::Value {
    let version = old
        .get("settings_version")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u32;
    if version == 0 {
        old["ultimate_installer_mode"] = serde_json::Value::Bool(false);
    }
    if version < 2 {
        old["appearance"] = serde_json::Value::String(default_appearance());
        old["locale"] = serde_json::Value::String(default_locale());
        let expanded = old
            .get("sidebar_labels")
            .and_then(|value| value.as_bool())
            .unwrap_or(true);
        old["navigation_mode"] =
            serde_json::Value::String(if expanded { "expanded" } else { "compact" }.into());
        old["background_variant"] = serde_json::Value::String(default_background_variant());
    }
    if version < 4 {
        old["show_minecraft_snapshots"] = serde_json::Value::Bool(false);
        old["show_old_minecraft_versions"] = serde_json::Value::Bool(false);
    }
    old["settings_version"] = serde_json::Value::Number(SETTINGS_VERSION.into());
    old
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[derive(Debug)]
    struct TestDir {
        root: PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            Self {
                root: crate::foundation::test_root("config"),
            }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn settings_file(&self) -> PathBuf {
            self.root.join("settings.json")
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn test_settings(root: &Path) -> LauncherSettings {
        LauncherSettings::defaults_for_game_directory(
            root.join("minecraft").to_string_lossy().to_string(),
        )
    }

    #[test]
    fn migrates_v1_shell_settings_without_losing_existing_values() {
        let directory = TestDir::new();
        let file = directory.settings_file();
        let mut defaults = test_settings(directory.path());
        defaults.settings_version = 1;
        defaults.accent_color = "#445566".into();
        let mut value = serde_json::to_value(defaults).expect("json");
        value.as_object_mut().expect("object").remove("appearance");
        value.as_object_mut().expect("object").remove("locale");
        value
            .as_object_mut()
            .expect("object")
            .remove("navigation_mode");
        value
            .as_object_mut()
            .expect("object")
            .remove("background_variant");
        fs::write(&file, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let loaded = load_settings_from(&file).expect("load");
        assert_eq!(loaded.settings_version, 4);
        assert_eq!(loaded.accent_color, "#445566");
        assert_eq!(loaded.appearance, "system");
    }

    #[test]
    fn atomically_round_trips_shell_settings() {
        let directory = TestDir::new();
        let file = directory.settings_file();
        let defaults = test_settings(directory.path());
        save_settings_to(&file, &defaults).expect("first save");
        let changed = defaults
            .apply_shell_settings(ShellSettings {
                appearance: "dark".into(),
                locale: "de".into(),
                accent_color: "#336699".into(),
                density: "compact".into(),
                navigation_mode: "compact".into(),
                background_variant: "grid".into(),
                reduced_motion: true,
                show_minecraft_snapshots: true,
                show_old_minecraft_versions: false,
            })
            .expect("apply");
        save_settings_to(&file, &changed).expect("second save");
        let loaded = load_settings_from(&file).expect("load");
        assert_eq!(loaded.shell_settings(), changed.shell_settings());
        let parent = file.parent().unwrap();
        let leftovers = fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".settings-")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn rejects_invalid_shell_values() {
        let directory = TestDir::new();
        let defaults = test_settings(directory.path());
        let error = defaults
            .apply_shell_settings(ShellSettings {
                appearance: "neon".into(),
                locale: "de".into(),
                accent_color: "#336699".into(),
                density: "compact".into(),
                navigation_mode: "expanded".into(),
                background_variant: "calm".into(),
                reduced_motion: false,
                show_minecraft_snapshots: false,
                show_old_minecraft_versions: false,
            })
            .expect_err("invalid");
        assert_eq!(error.descriptor().code, "settings_invalid_value");
    }

    #[test]
    fn concurrent_settings_writes_remain_atomic_and_leave_no_temp_files() {
        let directory = TestDir::new();
        let file = Arc::new(directory.settings_file());
        let colors = ["#112233", "#224466", "#336699", "#4488aa"];
        let barrier = Arc::new(Barrier::new(colors.len() + 1));
        let handles = colors
            .iter()
            .map(|color| {
                let mut settings = test_settings(directory.path());
                settings.accent_color = (*color).to_string();
                let file = Arc::clone(&file);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    save_settings_to(file.as_ref(), &settings)
                        .expect("parallel atomic settings save")
                })
            })
            .collect::<Vec<_>>();

        barrier.wait();
        for handle in handles {
            handle.join().expect("settings writer thread");
        }

        let concurrent_raw =
            fs::read_to_string(file.as_ref()).expect("read complete settings JSON");
        let concurrent_document: LauncherSettings =
            serde_json::from_str(&concurrent_raw).expect("parse complete settings JSON");
        assert!(colors.contains(&concurrent_document.accent_color.as_str()));

        let mut final_settings = test_settings(directory.path());
        final_settings.accent_color = "#55aaee".into();
        save_settings_to(file.as_ref(), &final_settings).expect("defined final settings writer");
        let loaded = load_settings_from(file.as_ref()).expect("load final settings document");
        assert_eq!(loaded.accent_color, "#55aaee");

        let blocked_target = directory.path().join("blocked-settings.json");
        fs::create_dir(&blocked_target).expect("create blocking destination directory");
        fs::write(blocked_target.join("keep"), b"fixture")
            .expect("make blocking destination non-empty");
        let error = save_settings_to(&blocked_target, &final_settings)
            .expect_err("atomic replace over a directory must fail");
        assert!(matches!(error, AppError::Io(_) | AppError::Coded { .. }));

        let poison_fixture = Mutex::new(());
        let poison_fixture_guard = poison_fixture.lock().expect("lock local poison fixture");
        let poison_error = match resolve_settings_write_lock(Err(std::sync::PoisonError::new(
            poison_fixture_guard,
        ))) {
            Ok(_) => panic!("poisoned settings lock must fail"),
            Err(error) => error,
        };
        assert_eq!(
            poison_error.descriptor().code,
            "settings_write_lock_poisoned"
        );

        let leftovers = fs::read_dir(directory.path())
            .expect("read settings test directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".settings-")
            })
            .count();
        assert_eq!(leftovers, 0);
    }
}
