use crate::{
    app::paths,
    error::{AppError, AppResult},
};
use futures_util::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};
use tokio::io::AsyncWriteExt;

const MANAGED_JAVA_MAJOR: u32 = 21;
const ADOPTIUM_API_URL: &str = "https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=x64&heap_size=normal&image_type=jre&jvm_impl=hotspot&os=windows&project=jdk&vendor=eclipse";

#[derive(Debug, Clone, Serialize)]
pub struct JavaRuntime {
    pub path: String,
    pub major_version: u32,
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: String,
    #[serde(default)]
    size: Option<u64>,
}

pub fn resolve_java_optional(configured: Option<&str>) -> Option<JavaRuntime> {
    let mut seen = HashSet::new();

    for candidate in java_candidates(configured) {
        let key = candidate.to_string_lossy().to_lowercase();

        if !seen.insert(key) {
            continue;
        }

        if let Some(runtime) = inspect_java(&candidate) {
            if runtime.major_version >= MANAGED_JAVA_MAJOR {
                return Some(runtime);
            }
        }
    }

    None
}

pub async fn ensure_java(configured: Option<&str>) -> AppResult<JavaRuntime> {
    if let Some(runtime) = resolve_java_optional(configured) {
        return Ok(runtime);
    }

    install_managed_java().await?;

    resolve_java_optional(configured).ok_or_else(|| {
        AppError::Message(
            "Die heruntergeladene Java-21-Runtime konnte nicht gestartet werden.".into(),
        )
    })
}

async fn install_managed_java() -> AppResult<()> {
    if !cfg!(target_os = "windows") {
        return Err(AppError::Message(
            "Der automatische Java-Download ist aktuell nur unter Windows verfügbar.".into(),
        ));
    }

    let launcher = paths::launcher_paths()?;
    let runtime_root = launcher.root.join("runtime");
    let target = runtime_root.join("java-21");
    let java_executable = target.join("bin").join(java_binary());

    if inspect_java(&java_executable)
        .is_some_and(|runtime| runtime.major_version >= MANAGED_JAVA_MAJOR)
    {
        return Ok(());
    }

    fs::create_dir_all(&runtime_root)?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("S9Lab-Launcher/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(20))
        .timeout(std::time::Duration::from_secs(300))
        .build()?;

    let assets = client
        .get(ADOPTIUM_API_URL)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<AdoptiumAsset>>()
        .await?;

    let package = assets
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Message("Adoptium lieferte keine Java-21-Runtime.".into()))?
        .binary
        .package;

    let archive_path = runtime_root.join("java-21.zip.part");
    let staging_root = runtime_root.join("java-21.installing");

    let response = client.get(&package.link).send().await?.error_for_status()?;

    if let Some(expected_size) = package.size {
        if let Some(actual_size) = response.content_length() {
            if actual_size != expected_size {
                return Err(AppError::Message(format!(
                    "Java-Downloadgröße stimmt nicht: erwartet {expected_size}, erhalten {actual_size}."
                )));
            }
        }
    }

    let mut archive_file = tokio::fs::File::create(&archive_path).await?;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        archive_file.write_all(&chunk?).await?;
    }

    archive_file.flush().await?;
    drop(archive_file);

    let downloaded_checksum = file_sha256(&archive_path)?;

    if !downloaded_checksum.eq_ignore_ascii_case(&package.checksum) {
        let _ = fs::remove_file(&archive_path);

        return Err(AppError::HashMismatch { path: archive_path });
    }

    if staging_root.exists() {
        fs::remove_dir_all(&staging_root)?;
    }

    fs::create_dir_all(&staging_root)?;
    extract_zip_safely(&archive_path, &staging_root)?;

    let extracted_java = find_java_home(&staging_root).ok_or_else(|| {
        AppError::Message(
            "Im heruntergeladenen Java-Archiv wurde keine gültige Runtime gefunden.".into(),
        )
    })?;

    if target.exists() {
        fs::remove_dir_all(&target)?;
    }

    if extracted_java == staging_root {
        fs::rename(&staging_root, &target)?;
    } else {
        fs::rename(&extracted_java, &target)?;
        let _ = fs::remove_dir_all(&staging_root);
    }

    let _ = fs::remove_file(&archive_path);

    let installed_java = target.join("bin").join(java_binary());

    let runtime = inspect_java(&installed_java).ok_or_else(|| {
        AppError::Message(
            "Java wurde entpackt, konnte anschließend aber nicht gestartet werden.".into(),
        )
    })?;

    if runtime.major_version < MANAGED_JAVA_MAJOR {
        return Err(AppError::Message(format!(
            "Die heruntergeladene Runtime verwendet Java {}, benötigt wird Java {}.",
            runtime.major_version, MANAGED_JAVA_MAJOR
        )));
    }

    Ok(())
}

fn java_candidates(configured: Option<&str>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = configured {
        let path = PathBuf::from(path);
        candidates.push(if path.is_dir() {
            path.join(java_binary())
        } else {
            path
        });
    }

    if let Ok(launcher) = paths::launcher_paths() {
        candidates.push(
            launcher
                .root
                .join("runtime")
                .join("java-21")
                .join("bin")
                .join(java_binary()),
        );
    }

    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        candidates.push(PathBuf::from(java_home).join("bin").join(java_binary()));
    }

    candidates.push(PathBuf::from(java_binary()));
    candidates.push(PathBuf::from("java"));

    if cfg!(target_os = "windows") {
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            let root = PathBuf::from(program_files);

            candidates.push(root.join("Common Files/Oracle/Java/javapath/java.exe"));

            for vendor in ["Eclipse Adoptium", "Microsoft", "Java", "BellSoft", "Zulu"] {
                append_vendor_javas(&root.join(vendor), &mut candidates);
            }
        }
    } else {
        for path in [
            "/usr/bin/java",
            "/usr/local/bin/java",
            "/opt/homebrew/opt/openjdk@21/bin/java",
            "/Library/Java/JavaVirtualMachines",
        ] {
            let path = PathBuf::from(path);

            if path.is_dir() && path.to_string_lossy().contains("JavaVirtualMachines") {
                append_macos_javas(&path, &mut candidates);
            } else {
                candidates.push(path);
            }
        }
    }

    candidates
}

fn append_vendor_javas(root: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    let mut versions: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();

    versions.sort_by(|a, b| b.cmp(a));

    output.extend(versions.into_iter().map(|path| path.join("bin/java.exe")));
}

fn append_macos_javas(root: &Path, output: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    output.extend(
        entries
            .flatten()
            .map(|entry| entry.path().join("Contents/Home/bin/java")),
    );
}

fn inspect_java(path: &Path) -> Option<JavaRuntime> {
    let output = Command::new(path).arg("-version").output().ok()?;

    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let major = parse_java_major(&text)?;

    Some(JavaRuntime {
        path: path.to_string_lossy().to_string(),
        major_version: major,
    })
}

fn find_java_home(root: &Path) -> Option<PathBuf> {
    if root.join("bin").join(java_binary()).is_file() {
        return Some(root.to_path_buf());
    }

    let entries = fs::read_dir(root).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() && path.join("bin").join(java_binary()).is_file() {
            return Some(path);
        }
    }

    None
}

fn extract_zip_safely(archive_path: &Path, target: &Path) -> AppResult<()> {
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut item = archive.by_index(index)?;

        let Some(enclosed_name) = item.enclosed_name() else {
            continue;
        };

        let output = target.join(enclosed_name);

        if item.is_dir() {
            fs::create_dir_all(&output)?;
            continue;
        }

        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output_file = fs::File::create(&output)?;
        std::io::copy(&mut item, &mut output_file)?;
        output_file.flush()?;
    }

    Ok(())
}

fn file_sha256(path: &Path) -> AppResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let count = file.read(&mut buffer)?;

        if count == 0 {
            break;
        }

        hasher.update(&buffer[..count]);
    }

    Ok(hex::encode(hasher.finalize()))
}

fn parse_java_major(value: &str) -> Option<u32> {
    let regex = Regex::new(r#"version\s+"([0-9]+)(?:\.([0-9]+))?"#).ok()?;

    let captures = regex.captures(value)?;
    let first = captures.get(1)?.as_str().parse::<u32>().ok()?;

    if first == 1 {
        captures.get(2)?.as_str().parse().ok()
    } else {
        Some(first)
    }
}

fn java_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_and_legacy_java_versions() {
        assert_eq!(parse_java_major(r#"openjdk version "21.0.5""#), Some(21));

        assert_eq!(parse_java_major(r#"java version "1.8.0_401""#), Some(8));
    }
}
