use crate::{
    error::{AppError, AppResult},
    runtime::JavaPolicy,
    security::{paths::validate_existing_chain, PathRegistry},
};
use futures_util::StreamExt;
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    sync::Mutex,
    time::timeout,
};

const JAVA_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const JAVA_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(360);
const MAX_MANAGED_JAVA_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MIN_MANAGED_JAVA_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024;
static MANAGED_JAVA_INSTALL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedJavaRuntime {
    #[serde(skip_serializing)]
    pub executable: PathBuf,
    pub major_version: u16,
    pub architecture: String,
    pub origin: String,
    pub managed: bool,
}

#[derive(Clone)]
pub struct JavaRuntimeResolver {
    registry: Arc<PathRegistry>,
}

impl JavaRuntimeResolver {
    pub fn new(registry: Arc<PathRegistry>) -> Self {
        Self { registry }
    }

    fn managed_java_root(&self, major_version: u16) -> AppResult<PathBuf> {
        Ok(self
            .registry
            .root("runtimes")?
            .to_path_buf()
            .join("java")
            .join(major_version.to_string()))
    }

    fn recover_managed_layout(&self, major_version: u16) -> AppResult<Option<PathBuf>> {
        let java_root = self.managed_java_root(major_version)?;
        fs::create_dir_all(&java_root)?;
        let target = java_root.join("current");
        let target_executable = target.join("bin").join(if cfg!(target_os = "windows") {
            "java.exe"
        } else {
            "java"
        });
        if target_executable.is_file() {
            return Ok(Some(target_executable));
        }

        let discovered = find_java_home(&java_root).filter(|candidate| candidate != &target);
        if let Some(home) = discovered {
            if target.exists() {
                let _ = fs::remove_dir_all(&target);
            }
            match fs::rename(&home, &target) {
                Ok(()) => {
                    let executable = target.join("bin").join(if cfg!(target_os = "windows") {
                        "java.exe"
                    } else {
                        "java"
                    });
                    if executable.is_file() {
                        return Ok(Some(executable));
                    }
                }
                Err(_) => {
                    // Leave the discovered runtime in place; the follow-up install path will
                    // replace it cleanly if it still cannot be used.
                }
            }
        }

        Ok(None)
    }

    pub async fn resolve(&self, policy: &JavaPolicy) -> AppResult<ResolvedJavaRuntime> {
        match policy {
            JavaPolicy::Managed { major_version } => {
                let executable = self.ensure_managed_executable(*major_version).await?;
                let trust_root = self.registry.root("runtimes")?.to_path_buf();

                // Managed Java is immutable from the launcher's point of view after
                // installation. Probe it once, persist a metadata-bound marker, and
                // avoid spawning a separate `java -version` process on every launch.
                if self.managed_probe_cache_hit(&executable, *major_version)? {
                    eprintln!(
                        "[snine-launch-fast] managed Java {} probe cache hit",
                        major_version
                    );
                    return Ok(ResolvedJavaRuntime {
                        executable,
                        major_version: *major_version,
                        architecture: std::env::consts::ARCH.to_string(),
                        origin: "managed-cached".into(),
                        managed: true,
                    });
                }

                let runtime = self
                    .probe(executable.clone(), trust_root, *major_version, "managed", true)
                    .await?;
                self.write_managed_probe_cache(&executable, *major_version);
                Ok(runtime)
            }
            JavaPolicy::System { major_version } => {
                let candidates = controlled_system_candidates(*major_version);
                for (executable, trust_root, origin) in candidates {
                    if let Ok(runtime) = self
                        .probe(executable, trust_root, *major_version, &origin, false)
                        .await
                    {
                        return Ok(runtime);
                    }
                }
                Err(AppError::coded_with(
                    "runtime_java_not_found",
                    [("majorVersion", major_version.to_string())],
                ))
            }
        }
    }

    pub async fn resolve_custom(
        &self,
        executable: &Path,
        expected_major: u16,
    ) -> AppResult<ResolvedJavaRuntime> {
        if !executable.is_absolute() {
            return Err(AppError::coded("runtime_java_path_uncontrolled"));
        }
        let executable = executable
            .canonicalize()
            .map_err(|_| AppError::coded("runtime_java_executable_invalid"))?;
        let trust_root = executable
            .parent()
            .ok_or_else(|| AppError::coded("runtime_java_path_uncontrolled"))?
            .to_path_buf();
        self.probe(executable, trust_root, expected_major, "custom", false)
            .await
    }

    async fn ensure_managed_executable(&self, major_version: u16) -> AppResult<PathBuf> {
        if let Ok(executable) = self.managed_executable(major_version) {
            return Ok(executable);
        }
        if let Ok(Some(executable)) = self.recover_managed_layout(major_version) {
            return Ok(executable);
        }

        let lock = MANAGED_JAVA_INSTALL_LOCK.get_or_init(|| Mutex::new(()));
        let _guard = lock.lock().await;
        if let Ok(executable) = self.managed_executable(major_version) {
            return Ok(executable);
        }
        if let Ok(Some(executable)) = self.recover_managed_layout(major_version) {
            return Ok(executable);
        }

        self.install_managed_runtime(major_version).await?;
        if let Ok(executable) = self.managed_executable(major_version) {
            return Ok(executable);
        }
        if let Ok(Some(executable)) = self.recover_managed_layout(major_version) {
            return Ok(executable);
        }
        self.managed_executable(major_version)
    }

    async fn install_managed_runtime(&self, major_version: u16) -> AppResult<()> {
        if !cfg!(target_os = "windows") {
            return Err(AppError::coded_with(
                "runtime_managed_java_auto_install_unsupported",
                [("platform", std::env::consts::OS.to_string())],
            ));
        }

        let architecture = match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "aarch64",
            other => {
                return Err(AppError::coded_with(
                    "runtime_managed_java_architecture_unsupported",
                    [("architecture", other.to_string())],
                ));
            }
        };
        let url = format!(
            "https://api.adoptium.net/v3/binary/latest/{major_version}/ga/windows/{architecture}/jre/hotspot/normal/eclipse?project=jdk"
        );

        let runtimes_root = self.registry.root("runtimes")?.to_path_buf();
        let java_root = runtimes_root.join("java").join(major_version.to_string());
        fs::create_dir_all(&java_root)?;
        let archive_path = java_root.join("temurin-jre.zip.part");
        let staging = java_root.join("installing");
        let target = java_root.join("current");
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_dir_all(&staging);

        eprintln!(
            "[snine-java] managed Java {major_version} missing; downloading Eclipse Temurin automatically"
        );
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(JAVA_DOWNLOAD_TIMEOUT)
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent("SNineLauncher/1.0.8")
            .build()?;
        let response = client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(AppError::coded_with(
                "runtime_managed_java_download_http",
                [("status", response.status().as_u16().to_string())],
            ));
        }
        if response
            .content_length()
            .is_some_and(|size| size > MAX_MANAGED_JAVA_ARCHIVE_BYTES)
        {
            return Err(AppError::coded("runtime_managed_java_download_too_large"));
        }

        let mut output = tokio::fs::File::create(&archive_path).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            downloaded = downloaded.saturating_add(chunk.len() as u64);
            if downloaded > MAX_MANAGED_JAVA_ARCHIVE_BYTES {
                let _ = tokio::fs::remove_file(&archive_path).await;
                return Err(AppError::coded("runtime_managed_java_download_too_large"));
            }
            output.write_all(&chunk).await?;
        }
        output.flush().await?;
        drop(output);
        if downloaded < MIN_MANAGED_JAVA_ARCHIVE_BYTES {
            let _ = tokio::fs::remove_file(&archive_path).await;
            return Err(AppError::coded_with(
                "runtime_managed_java_download_too_small",
                [("bytes", downloaded.to_string())],
            ));
        }

        let archive_for_extract = archive_path.clone();
        let staging_for_extract = staging.clone();
        let java_home = tokio::task::spawn_blocking(move || {
            extract_managed_java_archive(&archive_for_extract, &staging_for_extract)?;
            find_java_home(&staging_for_extract)
                .ok_or_else(|| AppError::coded("runtime_managed_java_archive_invalid"))
        })
        .await
        .map_err(|error| AppError::coded_with(
            "runtime_managed_java_extract_join_failed",
            [("detail", error.to_string())],
        ))??;

        let staged_executable = java_home.join("bin").join("java.exe");
        self.probe(
            staged_executable,
            java_root.clone(),
            major_version,
            "managed-download",
            true,
        )
        .await?;

        if target.exists() {
            fs::remove_dir_all(&target)?;
        }
        let java_home_is_staging = java_home == staging;
        fs::rename(&java_home, &target)?;
        if !java_home_is_staging {
            let _ = fs::remove_dir_all(&staging);
        }
        let _ = fs::remove_file(&archive_path);
        eprintln!(
            "[snine-java] Eclipse Temurin Java {major_version} installed at {}",
            target.display()
        );
        Ok(())
    }

    fn managed_probe_cache_path(&self, major_version: u16) -> AppResult<PathBuf> {
        Ok(self
            .managed_java_root(major_version)?
            .join("current")
            .join(".snine-java-probe-v1"))
    }

    fn managed_probe_fingerprint(executable: &Path, major_version: u16) -> AppResult<String> {
        use sha2::Digest as _;
        use std::time::UNIX_EPOCH;

        let metadata = fs::metadata(executable)?;
        if !metadata.is_file() {
            return Err(AppError::coded("runtime_java_executable_invalid"));
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let created = metadata
            .created()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"snine-managed-java-probe-v1\0");
        hasher.update(major_version.to_le_bytes());
        hasher.update(std::env::consts::ARCH.as_bytes());
        hasher.update(metadata.len().to_le_bytes());
        hasher.update(modified.to_le_bytes());
        hasher.update(created.to_le_bytes());
        Ok(hex::encode(hasher.finalize()))
    }

    fn managed_probe_cache_hit(&self, executable: &Path, major_version: u16) -> AppResult<bool> {
        let marker = self.managed_probe_cache_path(major_version)?;
        let expected = Self::managed_probe_fingerprint(executable, major_version)?;
        Ok(fs::read_to_string(marker)
            .ok()
            .is_some_and(|cached| cached.trim() == expected))
    }

    fn write_managed_probe_cache(&self, executable: &Path, major_version: u16) {
        let Ok(marker) = self.managed_probe_cache_path(major_version) else {
            return;
        };
        let Ok(fingerprint) = Self::managed_probe_fingerprint(executable, major_version) else {
            return;
        };
        let temporary = marker.with_extension("tmp");
        if fs::write(&temporary, fingerprint.as_bytes()).is_ok() {
            let _ = fs::remove_file(&marker);
            let _ = fs::rename(&temporary, &marker);
        } else {
            let _ = fs::remove_file(&temporary);
        }
    }

    fn managed_executable(&self, major_version: u16) -> AppResult<PathBuf> {
        let relative = if cfg!(target_os = "windows") {
            format!("java/{major_version}/current/bin/java.exe")
        } else {
            format!("java/{major_version}/current/bin/java")
        };
        let executable = self.registry.resolve("runtimes", relative)?;
        if !executable.absolute().is_file() {
            return Err(AppError::coded_with(
                "runtime_managed_java_unavailable",
                [("majorVersion", major_version.to_string())],
            ));
        }
        validate_java_executable(
            executable.anchor(),
            executable.root(),
            executable.absolute(),
        )?;
        Ok(executable.absolute().to_path_buf())
    }

    async fn probe(
        &self,
        executable: PathBuf,
        trust_root: PathBuf,
        expected_major: u16,
        origin: &str,
        managed: bool,
    ) -> AppResult<ResolvedJavaRuntime> {
        validate_java_executable(&trust_root, &trust_root, &executable)?;
        let output = timeout(
            JAVA_PROBE_TIMEOUT,
            Command::new(&executable)
                .arg("-XshowSettings:properties")
                .arg("-version")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .map_err(|_| AppError::coded("runtime_java_probe_timeout"))??;
        if !output.status.success() {
            return Err(AppError::coded("runtime_java_probe_failed"));
        }
        let mut diagnostic = String::from_utf8_lossy(&output.stderr).to_string();
        diagnostic.push_str(&String::from_utf8_lossy(&output.stdout));
        let (major_version, architecture) = parse_java_properties(&diagnostic)?;
        if major_version != expected_major {
            return Err(AppError::coded_with(
                "runtime_java_major_mismatch",
                [
                    ("expected", expected_major.to_string()),
                    ("actual", major_version.to_string()),
                ],
            ));
        }
        if !architecture_is_64_bit(&architecture) {
            return Err(AppError::coded_with(
                "runtime_java_architecture_unsupported",
                [("architecture", architecture)],
            ));
        }
        Ok(ResolvedJavaRuntime {
            executable,
            major_version,
            architecture,
            origin: origin.to_string(),
            managed,
        })
    }
}

fn extract_managed_java_archive(archive_path: &Path, destination: &Path) -> AppResult<()> {
    let _ = fs::remove_dir_all(destination);
    fs::create_dir_all(destination)?;
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(AppError::coded("runtime_managed_java_archive_symlink"));
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| AppError::coded("runtime_managed_java_archive_path_invalid"))?;
        let output_path = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&output_path)?;
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

fn find_java_home(root: &Path) -> Option<PathBuf> {
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = pending.pop() {
        let executable = directory.join("bin").join(if cfg!(target_os = "windows") {
            "java.exe"
        } else {
            "java"
        });
        if executable.is_file() {
            return Some(directory);
        }
        if depth >= 4 {
            continue;
        }
        let entries = fs::read_dir(&directory).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push((path, depth + 1));
            }
        }
    }
    None
}

fn controlled_system_candidates(major_version: u16) -> Vec<(PathBuf, PathBuf, String)> {
    let mut roots = BTreeSet::new();
    if cfg!(target_os = "windows") {
        for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Some(root) = std::env::var_os(variable).map(PathBuf::from) {
                if root.is_absolute() {
                    roots.insert(root.join("Eclipse Adoptium"));
                    roots.insert(root.join("Microsoft"));
                    roots.insert(root.join("Java"));
                    roots.insert(root.join("Zulu"));
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        roots.insert(PathBuf::from("/Library/Java/JavaVirtualMachines"));
    } else {
        roots.insert(PathBuf::from("/usr/lib/jvm"));
    }

    let mut candidates = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.contains(&major_version.to_string()) {
                continue;
            }
            let executable = if cfg!(target_os = "windows") {
                entry.path().join("bin/java.exe")
            } else if cfg!(target_os = "macos") {
                entry.path().join("Contents/Home/bin/java")
            } else {
                entry.path().join("bin/java")
            };
            candidates.push((
                executable,
                root.clone(),
                format!("system:{}", root.to_string_lossy()),
            ));
        }
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    candidates
}

fn validate_java_executable(anchor: &Path, root: &Path, executable: &Path) -> AppResult<()> {
    if !executable.is_absolute() || !executable.starts_with(root) {
        return Err(AppError::coded("runtime_java_path_uncontrolled"));
    }
    validate_existing_chain(anchor, executable)?;
    let metadata = std::fs::symlink_metadata(executable)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError::coded("runtime_java_executable_invalid"));
    }
    if metadata.len() == 0 {
        return Err(AppError::coded("runtime_java_executable_empty"));
    }
    Ok(())
}

fn parse_java_properties(value: &str) -> AppResult<(u16, String)> {
    let version = property(value, "java.version")
        .or_else(|| quoted_java_version(value))
        .ok_or_else(|| AppError::coded("runtime_java_version_unreadable"))?;
    let architecture = property(value, "os.arch")
        .ok_or_else(|| AppError::coded("runtime_java_architecture_unreadable"))?;
    let major_version = parse_java_major(&version)?;
    Ok((major_version, architecture))
}

fn property(value: &str, key: &str) -> Option<String> {
    value.lines().find_map(|line| {
        let line = line.trim();
        let (candidate, value) = line.split_once('=')?;
        (candidate.trim() == key).then(|| value.trim().to_string())
    })
}

fn quoted_java_version(value: &str) -> Option<String> {
    let marker = "version \"";
    let start = value.find(marker)? + marker.len();
    let rest = &value[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn parse_java_major(value: &str) -> AppResult<u16> {
    let mut parts = value.split('.');
    let first = parts
        .next()
        .and_then(parse_leading_u16)
        .ok_or_else(|| AppError::coded("runtime_java_version_unreadable"))?;
    if first == 1 {
        parts
            .next()
            .and_then(parse_leading_u16)
            .ok_or_else(|| AppError::coded("runtime_java_version_unreadable"))
    } else {
        Ok(first)
    }
}

fn parse_leading_u16(value: &str) -> Option<u16> {
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!digits.is_empty())
        .then(|| digits.parse::<u16>().ok())
        .flatten()
}

fn architecture_is_64_bit(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "amd64" | "x86_64" | "aarch64"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_and_legacy_java_versions() {
        assert_eq!(parse_java_major("21.0.7").expect("major"), 21);
        assert_eq!(parse_java_major("17-ea").expect("major"), 17);
        assert_eq!(parse_java_major("1.8.0_412").expect("major"), 8);
        assert!(parse_java_major("unknown").is_err());
    }

    #[test]
    fn parses_java_property_output_without_exposing_it() {
        let output = r#"
Property settings:
    java.version = 21.0.7
    os.arch = amd64
openjdk version "21.0.7" 2025-04-15
"#;
        assert_eq!(
            parse_java_properties(output).expect("properties"),
            (21, "amd64".into())
        );
    }

    #[test]
    fn rejects_non_64_bit_architectures() {
        assert!(architecture_is_64_bit("amd64"));
        assert!(architecture_is_64_bit("aarch64"));
        assert!(!architecture_is_64_bit("x86"));
    }

    #[test]
    fn system_candidates_never_use_a_bare_path_command() {
        for (candidate, trust_root, _) in controlled_system_candidates(21) {
            assert!(candidate.is_absolute());
            assert!(candidate.starts_with(trust_root));
            assert_ne!(candidate, PathBuf::from("java"));
            assert_ne!(candidate, PathBuf::from("java.exe"));
        }
    }
}
