use super::{model::ContentKind, resolver::validate_content_release, ContentReleaseV1};
use crate::{
    error::{AppError, AppResult},
    security::{
        paths::{collision_key, normalize_relative_path, validate_existing_chain},
        SecurePath,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const MAX_CONTENT_ARCHIVE_BYTES: u64 = 1_073_741_824;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentArchiveLimits {
    pub max_entries: usize,
    pub max_entry_uncompressed_bytes: u64,
    pub max_total_uncompressed_bytes: u64,
    pub max_compression_ratio: u64,
}

impl ContentArchiveLimits {
    pub fn for_kind(kind: ContentKind) -> Self {
        let max_total_uncompressed_bytes = match kind {
            ContentKind::Mod => 1_073_741_824,
            ContentKind::Modpack => 4_294_967_296,
            ContentKind::ShaderPack | ContentKind::ResourcePack => 2_147_483_648,
        };
        Self {
            max_entries: 100_000,
            max_entry_uncompressed_bytes: 536_870_912,
            max_total_uncompressed_bytes,
            max_compression_ratio: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentArchiveSummary {
    pub entry_count: usize,
    pub file_count: usize,
    pub total_compressed_bytes: u64,
    pub total_uncompressed_bytes: u64,
    pub descriptor_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidatedLocalContent {
    pub content_id: String,
    pub version: String,
    pub kind: ContentKind,
    pub sha256: String,
    pub size_bytes: u64,
    pub archive: ContentArchiveSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryType {
    Directory,
    File,
}

pub fn validate_local_content(
    source: &SecurePath,
    release: &ContentReleaseV1,
) -> AppResult<ValidatedLocalContent> {
    validate_content_release(release)?;
    validate_existing_chain(source.anchor(), source.absolute())?;

    let mut file = File::open(source.absolute())?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(AppError::coded("content_local_not_regular_file"));
    }
    let size_bytes = metadata.len();
    if size_bytes == 0 || size_bytes > MAX_CONTENT_ARCHIVE_BYTES {
        return Err(AppError::coded("content_local_size_invalid"));
    }
    if size_bytes != release.artifact.size_bytes {
        return Err(AppError::coded_with(
            "content_local_size_mismatch",
            [
                ("expectedSizeBytes", release.artifact.size_bytes.to_string()),
                ("actualSizeBytes", size_bytes.to_string()),
            ],
        ));
    }

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let sha256 = hex::encode(hasher.finalize());
    if sha256 != release.artifact.sha256 {
        return Err(AppError::coded("content_local_hash_mismatch"));
    }

    validate_existing_chain(source.anchor(), source.absolute())?;
    if file.metadata()?.len() != size_bytes {
        return Err(AppError::coded("content_local_changed_during_validation"));
    }
    file.seek(SeekFrom::Start(0))?;
    let archive = inspect_archive(
        file,
        release.kind,
        ContentArchiveLimits::for_kind(release.kind),
    )?;
    validate_existing_chain(source.anchor(), source.absolute())?;

    Ok(ValidatedLocalContent {
        content_id: release.content_id.clone(),
        version: release.version.clone(),
        kind: release.kind,
        sha256,
        size_bytes,
        archive,
    })
}

fn inspect_archive<R: Read + Seek>(
    reader: R,
    kind: ContentKind,
    limits: ContentArchiveLimits,
) -> AppResult<ContentArchiveSummary> {
    validate_limits(limits)?;
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|_| AppError::coded("content_archive_invalid"))?;
    if archive.is_empty() || archive.len() > limits.max_entries {
        return Err(AppError::coded_with(
            "content_archive_entry_count_invalid",
            [
                ("entryCount", archive.len().to_string()),
                ("maxEntryCount", limits.max_entries.to_string()),
            ],
        ));
    }

    let mut entries = BTreeMap::<String, EntryType>::new();
    let mut original_paths = BTreeMap::<String, String>::new();
    let mut file_count = 0usize;
    let mut total_compressed_bytes = 0u64;
    let mut total_uncompressed_bytes = 0u64;

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::coded("content_archive_invalid"))?;
        if entry.encrypted() {
            return Err(AppError::coded("content_archive_encrypted_entry_forbidden"));
        }
        let raw_name = std::str::from_utf8(entry.name_raw())
            .map_err(|_| AppError::coded("content_archive_entry_name_invalid_utf8"))?;
        let (normalized, entry_type) = validate_entry_path(raw_name, entry.is_dir())?;
        validate_unix_mode(entry.unix_mode(), entry_type)?;
        validate_entry_size(entry.compressed_size(), entry.size(), entry_type, limits)?;

        let key = collision_key(&normalized)?;
        if let Some(first) = original_paths.insert(key.clone(), raw_name.to_string()) {
            return Err(AppError::coded_with(
                "content_archive_entry_collision",
                [
                    ("firstPath", first),
                    ("secondPath", raw_name.to_string()),
                    ("normalizedPath", key),
                ],
            ));
        }
        entries.insert(key, entry_type);

        if entry_type == EntryType::File {
            file_count += 1;
            total_compressed_bytes = total_compressed_bytes
                .checked_add(entry.compressed_size())
                .ok_or_else(|| AppError::coded("content_archive_size_overflow"))?;
            total_uncompressed_bytes = total_uncompressed_bytes
                .checked_add(entry.size())
                .ok_or_else(|| AppError::coded("content_archive_size_overflow"))?;
            if total_compressed_bytes > MAX_CONTENT_ARCHIVE_BYTES {
                return Err(AppError::coded(
                    "content_archive_total_compressed_too_large",
                ));
            }
            if total_uncompressed_bytes > limits.max_total_uncompressed_bytes {
                return Err(AppError::coded(
                    "content_archive_total_uncompressed_too_large",
                ));
            }
        }
    }

    validate_file_directory_conflicts(&entries)?;
    let descriptor_path = validate_kind_descriptor(kind, &entries)?;
    Ok(ContentArchiveSummary {
        entry_count: entries.len(),
        file_count,
        total_compressed_bytes,
        total_uncompressed_bytes,
        descriptor_path,
    })
}

fn validate_limits(limits: ContentArchiveLimits) -> AppResult<()> {
    if limits.max_entries == 0
        || limits.max_entry_uncompressed_bytes == 0
        || limits.max_total_uncompressed_bytes == 0
        || limits.max_compression_ratio == 0
        || limits.max_entry_uncompressed_bytes > limits.max_total_uncompressed_bytes
    {
        return Err(AppError::coded("content_archive_limits_invalid"));
    }
    Ok(())
}

fn validate_entry_path(raw_name: &str, is_directory: bool) -> AppResult<(PathBuf, EntryType)> {
    if raw_name.contains('\\') {
        return Err(AppError::coded("content_archive_separator_noncanonical"));
    }
    let canonical = if is_directory {
        raw_name
            .strip_suffix('/')
            .filter(|value| !value.is_empty() && !value.ends_with('/'))
            .ok_or_else(|| AppError::coded("content_archive_directory_path_invalid"))?
    } else {
        if raw_name.ends_with('/') {
            return Err(AppError::coded("content_archive_file_path_invalid"));
        }
        raw_name
    };
    let normalized = normalize_relative_path(Path::new(canonical))?;
    let normalized_text = normalized
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    if normalized_text != canonical {
        return Err(AppError::coded("content_archive_path_noncanonical"));
    }
    Ok((
        normalized,
        if is_directory {
            EntryType::Directory
        } else {
            EntryType::File
        },
    ))
}

fn validate_unix_mode(mode: Option<u32>, entry_type: EntryType) -> AppResult<()> {
    let Some(mode) = mode else {
        return Ok(());
    };
    let file_type = mode & 0o170000;
    let expected = match entry_type {
        EntryType::Directory => 0o040000,
        EntryType::File => 0o100000,
    };
    if file_type != 0 && file_type != expected {
        return Err(AppError::coded("content_archive_special_entry_forbidden"));
    }
    Ok(())
}

fn validate_entry_size(
    compressed: u64,
    uncompressed: u64,
    entry_type: EntryType,
    limits: ContentArchiveLimits,
) -> AppResult<()> {
    if entry_type == EntryType::Directory {
        if uncompressed != 0 || compressed > 64 {
            return Err(AppError::coded("content_archive_directory_size_invalid"));
        }
        return Ok(());
    }
    if uncompressed > limits.max_entry_uncompressed_bytes {
        return Err(AppError::coded("content_archive_entry_too_large"));
    }
    if uncompressed > 0
        && (compressed == 0
            || uncompressed > compressed.saturating_mul(limits.max_compression_ratio))
    {
        return Err(AppError::coded(
            "content_archive_compression_ratio_exceeded",
        ));
    }
    Ok(())
}

fn validate_file_directory_conflicts(entries: &BTreeMap<String, EntryType>) -> AppResult<()> {
    for (path, entry_type) in entries {
        let mut prefix = String::new();
        let components = path.split('/').collect::<Vec<_>>();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(component);
            if entries.get(&prefix) == Some(&EntryType::File) {
                return Err(AppError::coded_with(
                    "content_archive_file_directory_conflict",
                    [("path", path.clone()), ("fileAncestor", prefix)],
                ));
            }
        }
        if *entry_type == EntryType::File {
            let descendant_prefix = format!("{path}/");
            if entries
                .range(descendant_prefix.clone()..)
                .next()
                .is_some_and(|(candidate, _)| candidate.starts_with(&descendant_prefix))
            {
                return Err(AppError::coded_with(
                    "content_archive_file_directory_conflict",
                    [("filePath", path.clone())],
                ));
            }
        }
    }
    Ok(())
}

fn validate_kind_descriptor(
    kind: ContentKind,
    entries: &BTreeMap<String, EntryType>,
) -> AppResult<String> {
    let exact_file = |path: &str| entries.get(path) == Some(&EntryType::File);
    let descriptor = match kind {
        ContentKind::Mod => [
            "fabric.mod.json",
            "META-INF/neoforge.mods.toml",
            "META-INF/mods.toml",
        ]
        .into_iter()
        .find(|path| exact_file(path)),
        ContentKind::Modpack => ["modrinth.index.json", "manifest.json", "s9lab-modpack.json"]
            .into_iter()
            .find(|path| exact_file(path)),
        ContentKind::ShaderPack => entries
            .iter()
            .find(|(path, entry_type)| {
                **entry_type == EntryType::File && path.starts_with("shaders/")
            })
            .map(|_| "shaders/"),
        ContentKind::ResourcePack => exact_file("pack.mcmeta").then_some("pack.mcmeta"),
    };
    descriptor
        .map(str::to_string)
        .ok_or_else(|| AppError::coded("content_archive_descriptor_missing"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        content::{
            ContentArtifactV1, ContentCompatibility, ContentReleaseV1, ContentSourceV1,
            CONTENT_RELEASE_FORMAT, CONTENT_RELEASE_FORMAT_VERSION,
        },
        security::{PathRegistry, RegisteredRoot},
    };
    use std::{fs, io::Cursor};
    use zip::write::SimpleFileOptions;

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start ZIP entry");
            std::io::Write::write_all(&mut writer, bytes).expect("write ZIP entry");
        }
        writer.finish().expect("finish ZIP").into_inner()
    }

    fn zip_with_symlink() -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        writer
            .start_file("fabric.mod.json", options)
            .expect("start descriptor");
        std::io::Write::write_all(&mut writer, b"{}").expect("write descriptor");
        writer
            .add_symlink("alias.class", "real.class", options)
            .expect("add ZIP symlink");
        writer.finish().expect("finish ZIP").into_inner()
    }

    fn release(kind: ContentKind, target: &str, bytes: &[u8]) -> ContentReleaseV1 {
        ContentReleaseV1 {
            format: CONTENT_RELEASE_FORMAT.into(),
            format_version: CONTENT_RELEASE_FORMAT_VERSION,
            content_id: format!("test-{}", kind.as_str().to_ascii_lowercase()),
            version: "1.0.0".into(),
            kind,
            compatibility: ContentCompatibility {
                minecraft_versions: vec!["1.21.1".into()],
                loaders: if matches!(kind, ContentKind::Mod | ContentKind::Modpack) {
                    vec![crate::content::ContentLoaderCompatibility {
                        kind: crate::runtime::LoaderKind::Fabric,
                        loader_versions: vec!["0.16.10".into()],
                    }]
                } else {
                    vec![]
                },
            },
            dependencies: vec![],
            source: Some(ContentSourceV1::Local {
                file_name: target.rsplit('/').next().expect("target name").into(),
            }),
            artifact: ContentArtifactV1 {
                relative_target: target.into(),
                sha256: hex::encode(Sha256::digest(bytes)),
                size_bytes: bytes.len() as u64,
            },
        }
    }

    fn with_secure_file<T>(bytes: &[u8], run: impl FnOnce(&SecurePath) -> T) -> T {
        let root = std::env::temp_dir().join(format!(
            "s9lab-content-archive-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("test root");
        let file = root.join("content.zip");
        fs::write(&file, bytes).expect("test archive");
        let registry = PathRegistry::new(
            &root,
            [RegisteredRoot {
                id: "imports".into(),
                path: root.clone(),
            }],
        )
        .expect("registry");
        let secure = registry
            .resolve("imports", "content.zip")
            .expect("secure file");
        let result = run(&secure);
        fs::remove_dir_all(root).expect("remove fixture");
        result
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected content archive failure")
            .descriptor()
            .code
    }

    #[test]
    fn validates_all_supported_archive_kinds_without_extracting() {
        let cases = [
            (
                ContentKind::Mod,
                "mods/example.jar",
                vec![("fabric.mod.json", b"{}".as_slice())],
            ),
            (
                ContentKind::Modpack,
                "modpacks/example.zip",
                vec![("modrinth.index.json", b"{}".as_slice())],
            ),
            (
                ContentKind::ShaderPack,
                "shaderpacks/example.zip",
                vec![("shaders/program.fsh", b"shader".as_slice())],
            ),
            (
                ContentKind::ResourcePack,
                "resourcepacks/example.zip",
                vec![("pack.mcmeta", b"{}".as_slice())],
            ),
        ];
        for (kind, target, entries) in cases {
            let bytes = zip_bytes(&entries);
            with_secure_file(&bytes, |source| {
                let result = validate_local_content(source, &release(kind, target, &bytes))
                    .expect("valid local content");
                assert_eq!(result.kind, kind);
                assert_eq!(result.archive.file_count, 1);
            });
        }
    }

    #[test]
    fn rejects_traversal_ads_ambiguous_separator_and_case_collisions() {
        for (entries, expected) in [
            (
                vec![
                    ("fabric.mod.json", b"{}".as_slice()),
                    ("../escape.txt", b"x".as_slice()),
                ],
                "path_traversal",
            ),
            (
                vec![
                    ("fabric.mod.json", b"{}".as_slice()),
                    ("payload.txt:ads", b"x".as_slice()),
                ],
                "path_alternate_data_stream",
            ),
            (
                vec![
                    ("fabric.mod.json", b"{}".as_slice()),
                    ("dir\\payload.txt", b"x".as_slice()),
                ],
                "content_archive_separator_noncanonical",
            ),
            (
                vec![
                    ("fabric.mod.json", b"{}".as_slice()),
                    ("dir//payload.txt", b"x".as_slice()),
                ],
                "path_ambiguous_separator",
            ),
            (
                vec![
                    ("fabric.mod.json", b"{}".as_slice()),
                    ("CON.txt", b"x".as_slice()),
                ],
                "path_windows_reserved_name",
            ),
            (
                vec![
                    ("fabric.mod.json", b"{}".as_slice()),
                    ("Data/File.txt", b"x".as_slice()),
                    ("data/file.txt", b"y".as_slice()),
                ],
                "content_archive_entry_collision",
            ),
        ] {
            let bytes = zip_bytes(&entries);
            with_secure_file(&bytes, |source| {
                assert_eq!(
                    error_code(validate_local_content(
                        source,
                        &release(ContentKind::Mod, "mods/example.jar", &bytes)
                    )),
                    expected
                );
            });
        }
    }

    #[test]
    fn rejects_archive_symlinks_and_file_directory_aliasing() {
        let symlink = zip_with_symlink();
        with_secure_file(&symlink, |source| {
            assert_eq!(
                error_code(validate_local_content(
                    source,
                    &release(ContentKind::Mod, "mods/example.jar", &symlink)
                )),
                "content_archive_special_entry_forbidden"
            );
        });

        let alias = zip_bytes(&[
            ("fabric.mod.json", b"{}"),
            ("data", b"file"),
            ("data/child.txt", b"child"),
        ]);
        with_secure_file(&alias, |source| {
            assert_eq!(
                error_code(validate_local_content(
                    source,
                    &release(ContentKind::Mod, "mods/example.jar", &alias)
                )),
                "content_archive_file_directory_conflict"
            );
        });
    }

    #[test]
    fn registered_source_rejects_hardlinks_before_content_is_opened() {
        let root = std::env::temp_dir().join(format!(
            "s9lab-content-hardlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("hardlink root");
        let first = root.join("first.zip");
        let second = root.join("second.zip");
        fs::write(&first, b"content").expect("hardlink source");
        fs::hard_link(&first, &second).expect("hardlink fixture");
        let registry = PathRegistry::new(
            &root,
            [RegisteredRoot {
                id: "imports".into(),
                path: root.clone(),
            }],
        )
        .expect("registry");
        assert_eq!(
            error_code(registry.resolve("imports", "first.zip")),
            "path_hardlink_forbidden"
        );
        fs::remove_dir_all(root).expect("remove hardlink fixture");
    }

    #[test]
    fn rejects_zip_bomb_metadata_and_missing_kind_descriptor() {
        let repeated = vec![0u8; 2 * 1024 * 1024];
        let bomb = zip_bytes(&[
            ("fabric.mod.json", b"{}"),
            ("payload.bin", repeated.as_slice()),
        ]);
        with_secure_file(&bomb, |source| {
            assert_eq!(
                error_code(validate_local_content(
                    source,
                    &release(ContentKind::Mod, "mods/example.jar", &bomb)
                )),
                "content_archive_compression_ratio_exceeded"
            );
        });

        let missing = zip_bytes(&[("readme.txt", b"not a pack")]);
        with_secure_file(&missing, |source| {
            assert_eq!(
                error_code(validate_local_content(
                    source,
                    &release(
                        ContentKind::ResourcePack,
                        "resourcepacks/example.zip",
                        &missing
                    )
                )),
                "content_archive_descriptor_missing"
            );
        });
    }

    #[test]
    fn artifact_size_and_hash_are_bound_before_archive_parsing() {
        let bytes = zip_bytes(&[("pack.mcmeta", b"{}")]);
        with_secure_file(&bytes, |source| {
            let mut wrong_size = release(
                ContentKind::ResourcePack,
                "resourcepacks/example.zip",
                &bytes,
            );
            wrong_size.artifact.size_bytes += 1;
            assert_eq!(
                error_code(validate_local_content(source, &wrong_size)),
                "content_local_size_mismatch"
            );

            let mut wrong_hash = release(
                ContentKind::ResourcePack,
                "resourcepacks/example.zip",
                &bytes,
            );
            wrong_hash.artifact.sha256 = "a".repeat(64);
            assert_eq!(
                error_code(validate_local_content(source, &wrong_hash)),
                "content_local_hash_mismatch"
            );
        });
    }
}
