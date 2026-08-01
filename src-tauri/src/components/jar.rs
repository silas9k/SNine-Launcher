use super::provider::{manifest_runtime, runtime_item_from_manifest};
use crate::{
    error::{AppError, AppResult},
    runtime::{
        validate_component_manifest, validate_jar_entries, JarEntryDescriptor, JarValidationLimits,
        JarValidationSummary, LoaderKind, S9labComponentManifestV1,
    },
    security::SecurePath,
};
use serde::de::{Deserializer as _, IgnoredAny, MapAccess, Visitor};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const FABRIC_DESCRIPTOR: &str = "fabric.mod.json";
const NEOFORGE_DESCRIPTOR: &str = "META-INF/neoforge.mods.toml";
const MAX_DESCRIPTOR_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedComponentJar {
    pub sha256: String,
    pub size_bytes: u64,
    pub entries: JarValidationSummary,
    pub descriptor_path: String,
    pub descriptor_mod_id: String,
}

pub fn inspect_component_jar(
    path: &SecurePath,
    manifest: &S9labComponentManifestV1,
) -> AppResult<InspectedComponentJar> {
    inspect_component_jar_path(path.absolute(), manifest)
}

fn inspect_component_jar_path(
    path: &Path,
    manifest: &S9labComponentManifestV1,
) -> AppResult<InspectedComponentJar> {
    validate_component_manifest(manifest, &manifest_runtime(manifest))?;
    let _ = runtime_item_from_manifest(manifest)?;

    let mut file = File::open(path)?;
    validate_artifact_metadata(&file.metadata()?, path)?;
    let size_bytes = file.metadata()?.len();
    if size_bytes != manifest.size_bytes {
        return Err(AppError::coded_with(
            "component_artifact_size_mismatch",
            [
                ("expectedSizeBytes", manifest.size_bytes.to_string()),
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
    if sha256 != manifest.sha256 {
        return Err(AppError::coded("component_artifact_hash_mismatch"));
    }
    file.seek(SeekFrom::Start(0))?;

    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| AppError::coded("component_jar_invalid"))?;
    let expected_descriptor = match manifest.loader.kind {
        LoaderKind::Fabric => FABRIC_DESCRIPTOR,
        LoaderKind::Neoforge => NEOFORGE_DESCRIPTOR,
        LoaderKind::Vanilla => {
            return Err(AppError::coded("component_loader_descriptor_unsupported"));
        }
    };

    let mut entries = Vec::with_capacity(archive.len());
    let mut descriptor_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|_| AppError::coded("component_jar_invalid"))?;
        let relative_path = std::str::from_utf8(entry.name_raw())
            .map_err(|_| AppError::coded("component_jar_entry_name_invalid_utf8"))?
            .to_string();
        if relative_path == expected_descriptor && descriptor_index.replace(index).is_some() {
            return Err(AppError::coded("component_descriptor_duplicate"));
        }
        entries.push(JarEntryDescriptor {
            relative_path,
            is_directory: entry.is_dir(),
            compressed_size_bytes: entry.compressed_size(),
            uncompressed_size_bytes: entry.size(),
            encrypted: entry.encrypted(),
            unix_mode: entry.unix_mode(),
        });
    }

    let summary = validate_jar_entries(&entries, JarValidationLimits::default())?;
    let descriptor_index =
        descriptor_index.ok_or_else(|| AppError::coded("component_descriptor_missing"))?;
    let descriptor_bytes = read_descriptor(&mut archive, descriptor_index)?;
    let descriptor_mod_id = match manifest.loader.kind {
        LoaderKind::Fabric => parse_fabric_mod_id(&descriptor_bytes)?,
        LoaderKind::Neoforge => parse_neoforge_mod_id(&descriptor_bytes)?,
        LoaderKind::Vanilla => unreachable!("vanilla was rejected above"),
    };
    if descriptor_mod_id != manifest.component_id {
        return Err(AppError::coded_with(
            "component_descriptor_id_mismatch",
            [
                ("expectedComponentId", manifest.component_id.clone()),
                ("descriptorModId", descriptor_mod_id),
            ],
        ));
    }

    Ok(InspectedComponentJar {
        sha256,
        size_bytes,
        entries: summary,
        descriptor_path: expected_descriptor.into(),
        descriptor_mod_id: manifest.component_id.clone(),
    })
}

fn validate_artifact_metadata(metadata: &fs::Metadata, path: &Path) -> AppResult<()> {
    if !metadata.is_file() {
        return Err(AppError::coded("component_artifact_not_regular_file"));
    }
    if metadata.file_type().is_symlink() || is_reparse_point(metadata) {
        return Err(AppError::coded(
            "component_artifact_reparse_point_forbidden",
        ));
    }
    if hard_link_count(metadata, path)? > 1 {
        return Err(AppError::coded("component_artifact_hardlink_forbidden"));
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(windows)]
fn hard_link_count(_: &fs::Metadata, path: &Path) -> AppResult<u64> {
    use std::{fs::OpenOptions, mem::MaybeUninit, os::windows::io::AsRawHandle};

    #[repr(C)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time_low: u32,
        creation_time_high: u32,
        last_access_time_low: u32,
        last_access_time_high: u32,
        last_write_time_low: u32,
        last_write_time_high: u32,
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        #[link_name = "GetFileInformationByHandle"]
        fn get_file_information_by_handle(
            handle: *mut core::ffi::c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    let file = OpenOptions::new().read(true).open(path)?;
    let mut information = MaybeUninit::<ByHandleFileInformation>::uninit();
    // SAFETY: `file` owns a live Windows file handle, `information` points to
    // writable storage of the exact ABI-compatible result type, and the value
    // is assumed initialized only after Windows reports success.
    let success = unsafe {
        get_file_information_by_handle(file.as_raw_handle().cast(), information.as_mut_ptr())
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: the successful API call above initialized every field.
    Ok(unsafe { information.assume_init() }.number_of_links.into())
}

#[cfg(unix)]
fn hard_link_count(metadata: &fs::Metadata, _: &Path) -> AppResult<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(metadata.nlink())
}

#[cfg(not(any(windows, unix)))]
fn hard_link_count(_: &fs::Metadata, _: &Path) -> AppResult<u64> {
    Ok(1)
}

fn read_descriptor<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    index: usize,
) -> AppResult<Vec<u8>> {
    let mut entry = archive
        .by_index(index)
        .map_err(|_| AppError::coded("component_jar_invalid"))?;
    if entry.is_dir() || entry.size() == 0 || entry.size() > MAX_DESCRIPTOR_BYTES {
        return Err(AppError::coded("component_descriptor_size_invalid"));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .by_ref()
        .take(MAX_DESCRIPTOR_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::coded("component_descriptor_read_failed"))?;
    if bytes.len() as u64 != entry.size() || bytes.len() as u64 > MAX_DESCRIPTOR_BYTES {
        return Err(AppError::coded("component_descriptor_size_invalid"));
    }
    Ok(bytes)
}

fn parse_fabric_mod_id(bytes: &[u8]) -> AppResult<String> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let id = deserializer
        .deserialize_map(FabricDescriptorVisitor)
        .map_err(|_| AppError::coded("component_fabric_descriptor_invalid"))?;
    deserializer
        .end()
        .map_err(|_| AppError::coded("component_fabric_descriptor_invalid"))?;
    id.ok_or_else(|| AppError::coded("component_descriptor_id_missing"))
}

struct FabricDescriptorVisitor;

impl<'de> Visitor<'de> for FabricDescriptorVisitor {
    type Value = Option<String>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a Fabric mod descriptor object")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut id = None;
        while let Some(key) = map.next_key::<String>()? {
            if key == "id" {
                if id.is_some() {
                    return Err(serde::de::Error::duplicate_field("id"));
                }
                id = Some(map.next_value::<String>()?);
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }
        Ok(id)
    }
}

fn parse_neoforge_mod_id(bytes: &[u8]) -> AppResult<String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| AppError::coded("component_neoforge_descriptor_invalid"))?;
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
    {
        return Err(AppError::coded("component_neoforge_descriptor_invalid"));
    }

    let mut in_mod = false;
    let mut mod_sections = 0usize;
    let mut mod_id = None;
    for raw_line in text.lines() {
        let line = strip_toml_comment(raw_line)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if line == "[[mods]]" {
                mod_sections += 1;
                in_mod = true;
            } else {
                in_mod = false;
            }
            continue;
        }
        if !in_mod {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(AppError::coded("component_neoforge_descriptor_invalid"));
        };
        if key.trim() == "modId" {
            if mod_id.is_some() {
                return Err(AppError::coded("component_descriptor_id_duplicate"));
            }
            mod_id = Some(parse_simple_toml_string(value.trim())?);
        }
    }

    if mod_sections != 1 {
        return Err(AppError::coded("component_neoforge_descriptor_ambiguous"));
    }
    mod_id.ok_or_else(|| AppError::coded("component_descriptor_id_missing"))
}

fn strip_toml_comment(line: &str) -> AppResult<&str> {
    let mut quote = None;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if character == '\\' => escaped = true,
            Some(active) if character == active => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if character == '#' => return Ok(&line[..index]),
            None => {}
        }
    }
    if quote.is_some() || escaped {
        return Err(AppError::coded("component_neoforge_descriptor_invalid"));
    }
    Ok(line)
}

fn parse_simple_toml_string(value: &str) -> AppResult<String> {
    let Some(quote) = value.chars().next() else {
        return Err(AppError::coded("component_neoforge_descriptor_invalid"));
    };
    if !matches!(quote, '"' | '\'') || !value.ends_with(quote) || value.len() < 2 {
        return Err(AppError::coded("component_neoforge_descriptor_invalid"));
    }
    let content = &value[1..value.len() - 1];
    if content.contains(['\\', '"', '\''])
        || content.is_empty()
        || !content.is_ascii()
        || !content
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(AppError::coded("component_neoforge_descriptor_invalid"));
    }
    Ok(content.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        operations::model::{new_identifier, sha256_hex},
        runtime::{
            LoaderSelection, COMPONENT_MANIFEST_FORMAT, COMPONENT_MANIFEST_FORMAT_VERSION,
            COMPONENT_SIGNATURE_DOMAIN,
        },
    };
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn build_jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(*name, options).expect("start JAR entry");
            writer.write_all(bytes).expect("write JAR entry");
        }
        writer.finish().expect("finish JAR").into_inner()
    }

    fn manifest(loader: LoaderKind, component_id: &str, jar: &[u8]) -> S9labComponentManifestV1 {
        S9labComponentManifestV1 {
            format: COMPONENT_MANIFEST_FORMAT.into(),
            format_version: COMPONENT_MANIFEST_FORMAT_VERSION,
            signature_domain: COMPONENT_SIGNATURE_DOMAIN.into(),
            key_id: "test-release-1".into(),
            component_id: component_id.into(),
            component_version: "1.0.8".into(),
            minecraft_version: "1.21.1".into(),
            loader: LoaderSelection {
                kind: loader,
                loader_version: Some(
                    match loader {
                        LoaderKind::Fabric => "0.16.10",
                        LoaderKind::Neoforge => "21.1.200",
                        LoaderKind::Vanilla => unreachable!(),
                    }
                    .into(),
                ),
            },
            size_bytes: jar.len() as u64,
            sha256: sha256_hex(jar),
            relative_target: format!("mods/s9lab/{component_id}.jar"),
            signature: "test-signature-placeholder-with-valid-length".into(),
        }
    }

    fn with_jar<T>(bytes: &[u8], run: impl FnOnce(&Path) -> T) -> T {
        let root = std::env::temp_dir().join(format!(
            "s9lab-component-jar-{}-{}",
            std::process::id(),
            new_identifier("jar")
        ));
        fs::create_dir_all(&root).expect("test directory");
        let path = root.join("component.jar");
        fs::write(&path, bytes).expect("test JAR");
        let result = run(&path);
        fs::remove_dir_all(&root).expect("remove test directory");
        result
    }

    fn error_code<T: std::fmt::Debug>(result: AppResult<T>) -> String {
        result
            .expect_err("expected JAR validation failure")
            .descriptor()
            .code
    }

    #[test]
    fn valid_fabric_and_neoforge_descriptors_are_read_without_extraction() {
        let fabric = build_jar(&[
            (
                FABRIC_DESCRIPTOR,
                br#"{"schemaVersion":1,"id":"s9lab_client","version":"1.0.8"}"#,
            ),
            ("com/s9lab/Client.class", b"class"),
        ]);
        with_jar(&fabric, |path| {
            let inspected = inspect_component_jar_path(
                path,
                &manifest(LoaderKind::Fabric, "s9lab_client", &fabric),
            )
            .expect("valid Fabric component");
            assert_eq!(inspected.descriptor_path, FABRIC_DESCRIPTOR);
            assert_eq!(inspected.descriptor_mod_id, "s9lab_client");
        });

        let neoforge = build_jar(&[
            (
                NEOFORGE_DESCRIPTOR,
                b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"s9lab_client\"\nversion=\"1.0.8\"\n",
            ),
            ("com/s9lab/Client.class", b"class"),
        ]);
        with_jar(&neoforge, |path| {
            let inspected = inspect_component_jar_path(
                path,
                &manifest(LoaderKind::Neoforge, "s9lab_client", &neoforge),
            )
            .expect("valid NeoForge component");
            assert_eq!(inspected.descriptor_path, NEOFORGE_DESCRIPTOR);
        });
    }

    #[test]
    fn malformed_jar_and_traversal_entry_are_rejected() {
        let malformed = b"not a JAR".to_vec();
        with_jar(&malformed, |path| {
            assert_eq!(
                error_code(inspect_component_jar_path(
                    path,
                    &manifest(LoaderKind::Fabric, "s9lab_client", &malformed)
                )),
                "component_jar_invalid"
            );
        });

        let traversal = build_jar(&[
            (
                FABRIC_DESCRIPTOR,
                br#"{"id":"s9lab_client","version":"1.0.8"}"#,
            ),
            ("../escape.class", b"class"),
        ]);
        with_jar(&traversal, |path| {
            assert_eq!(
                error_code(inspect_component_jar_path(
                    path,
                    &manifest(LoaderKind::Fabric, "s9lab_client", &traversal)
                )),
                "path_traversal"
            );
        });
    }

    #[test]
    fn zip_bomb_metadata_is_rejected_before_descriptor_is_read() {
        let oversized = vec![0u8; 2 * 1024 * 1024];
        let bomb = build_jar(&[
            (
                FABRIC_DESCRIPTOR,
                br#"{"id":"s9lab_client","version":"1.0.8"}"#,
            ),
            ("com/s9lab/Payload.bin", &oversized),
        ]);
        with_jar(&bomb, |path| {
            assert_eq!(
                error_code(inspect_component_jar_path(
                    path,
                    &manifest(LoaderKind::Fabric, "s9lab_client", &bomb)
                )),
                "component_jar_compression_ratio_exceeded"
            );
        });
    }

    #[test]
    fn descriptor_mismatch_duplicate_id_and_missing_descriptor_are_rejected() {
        let mismatch = build_jar(&[(
            FABRIC_DESCRIPTOR,
            br#"{"id":"different_mod","version":"1.0.8"}"#,
        )]);
        with_jar(&mismatch, |path| {
            assert_eq!(
                error_code(inspect_component_jar_path(
                    path,
                    &manifest(LoaderKind::Fabric, "s9lab_client", &mismatch)
                )),
                "component_descriptor_id_mismatch"
            );
        });

        let duplicate = build_jar(&[(
            FABRIC_DESCRIPTOR,
            br#"{"id":"s9lab_client","id":"s9lab_client"}"#,
        )]);
        with_jar(&duplicate, |path| {
            assert_eq!(
                error_code(inspect_component_jar_path(
                    path,
                    &manifest(LoaderKind::Fabric, "s9lab_client", &duplicate)
                )),
                "component_fabric_descriptor_invalid"
            );
        });

        let missing = build_jar(&[("META-INF/MANIFEST.MF", b"Manifest-Version: 1.0\n")]);
        with_jar(&missing, |path| {
            assert_eq!(
                error_code(inspect_component_jar_path(
                    path,
                    &manifest(LoaderKind::Fabric, "s9lab_client", &missing)
                )),
                "component_descriptor_missing"
            );
        });
    }

    #[test]
    fn artifact_size_and_hash_must_match_the_signed_manifest() {
        let jar = build_jar(&[(
            FABRIC_DESCRIPTOR,
            br#"{"id":"s9lab_client","version":"1.0.8"}"#,
        )]);
        with_jar(&jar, |path| {
            let mut wrong_size = manifest(LoaderKind::Fabric, "s9lab_client", &jar);
            wrong_size.size_bytes += 1;
            assert_eq!(
                error_code(inspect_component_jar_path(path, &wrong_size)),
                "component_artifact_size_mismatch"
            );

            let mut wrong_hash = manifest(LoaderKind::Fabric, "s9lab_client", &jar);
            wrong_hash.sha256 = "b".repeat(64);
            assert_eq!(
                error_code(inspect_component_jar_path(path, &wrong_hash)),
                "component_artifact_hash_mismatch"
            );
        });
    }
}
