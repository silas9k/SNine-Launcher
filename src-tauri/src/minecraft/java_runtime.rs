use crate::{
    error::{AppError, AppResult},
    runtime::JavaPolicy,
    security::{paths::validate_existing_chain, PathRegistry},
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{process::Command, time::timeout};

const JAVA_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

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

    pub async fn resolve(&self, policy: &JavaPolicy) -> AppResult<ResolvedJavaRuntime> {
        match policy {
            JavaPolicy::Managed { major_version } => {
                let executable = self.managed_executable(*major_version)?;
                let trust_root = self.registry.root("runtimes")?.to_path_buf();
                self.probe(executable, trust_root, *major_version, "managed", true)
                    .await
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
