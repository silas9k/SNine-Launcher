use crate::error::{AppError, AppResult};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

const MAX_COMPONENT_UTF16: usize = 255;
const MAX_RELATIVE_UTF16: usize = 220;
/// Conservative visible UTF-16 path budget used until every consumer is
/// verified as long-path aware. It also keeps directory creation below the
/// legacy Windows directory limit instead of relying on an extended prefix.
pub const LEGACY_SAFE_MAX_ABSOLUTE_UTF16: usize = 247;
const PATH_SEPARATOR_UTF16: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathLengthBudget {
    pub root_utf16: usize,
    pub max_relative_utf16: usize,
    pub max_absolute_utf16: usize,
    pub available_relative_utf16: usize,
}

#[derive(Debug, Clone)]
pub struct RegisteredRoot {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PathRegistry {
    anchor: PathBuf,
    roots: BTreeMap<String, PathBuf>,
    max_absolute_utf16: usize,
}

#[derive(Debug, Clone)]
pub struct SecurePath {
    root_id: String,
    anchor: PathBuf,
    root: PathBuf,
    relative: PathBuf,
    absolute: PathBuf,
}

impl SecurePath {
    pub fn root_id(&self) -> &str {
        &self.root_id
    }

    pub fn anchor(&self) -> &Path {
        &self.anchor
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn relative(&self) -> &Path {
        &self.relative
    }

    pub fn absolute(&self) -> &Path {
        &self.absolute
    }
}

impl PathRegistry {
    pub fn new(
        anchor: impl AsRef<Path>,
        roots: impl IntoIterator<Item = RegisteredRoot>,
    ) -> AppResult<Self> {
        Self::new_with_absolute_limit(anchor, roots, LEGACY_SAFE_MAX_ABSOLUTE_UTF16)
    }

    #[cfg(test)]
    pub(crate) fn new_for_tests(
        anchor: impl AsRef<Path>,
        roots: impl IntoIterator<Item = RegisteredRoot>,
        max_absolute_utf16: usize,
    ) -> AppResult<Self> {
        Self::new_with_absolute_limit(anchor, roots, max_absolute_utf16)
    }

    fn new_with_absolute_limit(
        anchor: impl AsRef<Path>,
        roots: impl IntoIterator<Item = RegisteredRoot>,
        max_absolute_utf16: usize,
    ) -> AppResult<Self> {
        if max_absolute_utf16 == 0 {
            return Err(AppError::coded("path_absolute_limit_invalid"));
        }
        let anchor = absolute_lexical(anchor.as_ref())?;
        if !anchor.exists() {
            return Err(AppError::coded("path_anchor_missing"));
        }
        validate_existing_entry(&anchor)?;

        let mut map = BTreeMap::new();
        for root in roots {
            if root.id.trim().is_empty() {
                return Err(AppError::coded("path_root_id_empty"));
            }
            let absolute = absolute_lexical(&root.path)?;
            if !absolute.starts_with(&anchor) {
                return Err(AppError::coded_with(
                    "path_root_outside_anchor",
                    [
                        ("rootId", root.id),
                        ("path", absolute.display().to_string()),
                    ],
                ));
            }
            validate_existing_chain(&anchor, &absolute)?;
            if map.insert(root.id.clone(), absolute).is_some() {
                return Err(AppError::coded_with(
                    "path_root_duplicate",
                    [("rootId", root.id)],
                ));
            }
        }
        if map.is_empty() {
            return Err(AppError::coded("path_registry_empty"));
        }
        Ok(Self {
            anchor,
            roots: map,
            max_absolute_utf16,
        })
    }

    pub fn root_ids(&self) -> Vec<String> {
        self.roots.keys().cloned().collect()
    }

    pub fn root(&self, root_id: &str) -> AppResult<&Path> {
        self.roots
            .get(root_id)
            .map(PathBuf::as_path)
            .ok_or_else(|| {
                AppError::coded_with("path_root_unknown", [("rootId", root_id.to_string())])
            })
    }

    pub fn length_budget(&self, root_id: &str) -> AppResult<PathLengthBudget> {
        path_length_budget_with_limit(self.root(root_id)?, self.max_absolute_utf16)
    }

    pub fn resolve(&self, root_id: &str, relative: impl AsRef<Path>) -> AppResult<SecurePath> {
        let root = self.root(root_id)?.to_path_buf();
        let normalized = normalize_relative_path(relative.as_ref())?;
        let absolute = root.join(&normalized);
        enforce_length_limits_with_limit(&root, &normalized, &absolute, self.max_absolute_utf16)?;
        if !absolute.starts_with(&root) {
            return Err(AppError::coded("path_outside_registered_root"));
        }
        validate_existing_chain(&self.anchor, &absolute)?;
        Ok(SecurePath {
            root_id: root_id.to_string(),
            anchor: self.anchor.clone(),
            root,
            relative: normalized,
            absolute,
        })
    }

    pub fn validate_unique(
        &self,
        root_id: &str,
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> AppResult<Vec<SecurePath>> {
        let mut collision_keys = HashSet::new();
        let mut resolved = Vec::new();
        for path in paths {
            let secure = self.resolve(root_id, path)?;
            let key = collision_key(secure.relative())?;
            if !collision_keys.insert(key.clone()) {
                return Err(AppError::coded_with(
                    "path_collision",
                    [("normalizedPath", key)],
                ));
            }
            resolved.push(secure);
        }
        Ok(resolved)
    }
}

pub fn normalize_relative_path(path: &Path) -> AppResult<PathBuf> {
    let raw = path.to_string_lossy();
    if raw.is_empty() {
        return Err(AppError::coded("path_empty"));
    }
    if path.is_absolute() || has_windows_prefix(&raw) {
        return Err(AppError::coded("path_absolute_forbidden"));
    }
    if raw.contains('\0') {
        return Err(AppError::coded("path_nul_forbidden"));
    }

    let normalized_separators = raw.replace('\\', "/");
    let mut result = PathBuf::new();
    let mut component_count = 0usize;
    for component in normalized_separators.split('/') {
        if component.is_empty() || component == "." {
            return Err(AppError::coded("path_ambiguous_separator"));
        }
        if component == ".." {
            return Err(AppError::coded("path_traversal"));
        }
        validate_component(component)?;
        result.push(component);
        component_count += 1;
    }
    if component_count == 0 {
        return Err(AppError::coded("path_empty"));
    }
    Ok(result)
}

pub fn collision_key(path: &Path) -> AppResult<String> {
    let mut components = Vec::new();
    for component in path.components() {
        let text = component.as_os_str().to_string_lossy();
        components.push(normalize_unicode_key(&text)?);
    }
    Ok(components.join("/"))
}

fn validate_component(component: &str) -> AppResult<()> {
    if component.is_empty() {
        return Err(AppError::coded("path_component_empty"));
    }
    if component.ends_with(' ') || component.ends_with('.') {
        return Err(AppError::coded_with(
            "path_component_trailing_character",
            [("component", component.to_string())],
        ));
    }
    if component.chars().any(|c| c.is_control()) {
        return Err(AppError::coded("path_control_character"));
    }
    if component.contains(':') {
        return Err(AppError::coded("path_alternate_data_stream"));
    }
    if component
        .chars()
        .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(AppError::coded("path_windows_character_forbidden"));
    }
    if component.encode_utf16().count() > MAX_COMPONENT_UTF16 {
        return Err(AppError::coded("path_component_too_long"));
    }

    let basename = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    let reserved = matches!(basename.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || is_numbered_device(&basename, "COM")
        || is_numbered_device(&basename, "LPT");
    if reserved {
        return Err(AppError::coded_with(
            "path_windows_reserved_name",
            [("component", component.to_string())],
        ));
    }
    let _ = normalize_unicode_key(component)?;
    Ok(())
}

fn is_numbered_device(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|suffix| suffix.parse::<u8>().ok())
        .is_some_and(|number| (1..=9).contains(&number))
}

fn has_windows_prefix(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
        || raw.starts_with("//")
        || raw.starts_with("\\\\")
        || raw.starts_with("\\?")
        || raw.starts_with("\\.")
}

#[cfg(test)]
fn path_length_budget(root: &Path) -> AppResult<PathLengthBudget> {
    path_length_budget_with_limit(root, LEGACY_SAFE_MAX_ABSOLUTE_UTF16)
}

fn path_length_budget_with_limit(
    root: &Path,
    max_absolute_utf16: usize,
) -> AppResult<PathLengthBudget> {
    let root_utf16 = utf16_len(root);
    let root_text = root.to_string_lossy();
    let separator_utf16 = if root_text.ends_with('/') || root_text.ends_with('\\') {
        0
    } else {
        PATH_SEPARATOR_UTF16
    };
    let available_by_absolute = max_absolute_utf16
        .checked_sub(root_utf16.saturating_add(separator_utf16))
        .ok_or_else(|| {
            AppError::coded_with(
                "path_root_too_long",
                [
                    ("root", root.display().to_string()),
                    ("rootLength", root_utf16.to_string()),
                    ("maxAbsoluteLength", max_absolute_utf16.to_string()),
                ],
            )
        })?;
    let available_relative_utf16 = available_by_absolute.min(MAX_RELATIVE_UTF16);
    if available_relative_utf16 == 0 {
        return Err(AppError::coded_with(
            "path_root_too_long",
            [
                ("root", root.display().to_string()),
                ("rootLength", root_utf16.to_string()),
                ("maxAbsoluteLength", max_absolute_utf16.to_string()),
            ],
        ));
    }
    Ok(PathLengthBudget {
        root_utf16,
        max_relative_utf16: MAX_RELATIVE_UTF16,
        max_absolute_utf16,
        available_relative_utf16,
    })
}

#[cfg(test)]
fn enforce_length_limits(root: &Path, relative: &Path, absolute: &Path) -> AppResult<()> {
    enforce_length_limits_with_limit(root, relative, absolute, LEGACY_SAFE_MAX_ABSOLUTE_UTF16)
}

fn enforce_length_limits_with_limit(
    root: &Path,
    relative: &Path,
    absolute: &Path,
    max_absolute_utf16: usize,
) -> AppResult<()> {
    let budget = path_length_budget_with_limit(root, max_absolute_utf16)?;
    let relative_len = utf16_len(relative);
    let absolute_len = utf16_len(absolute);
    if relative_len > budget.available_relative_utf16 || absolute_len > budget.max_absolute_utf16 {
        return Err(AppError::coded_with(
            "path_too_long",
            [
                ("root", root.display().to_string()),
                ("rootLength", budget.root_utf16.to_string()),
                ("relativeLength", relative_len.to_string()),
                ("absoluteLength", absolute_len.to_string()),
                (
                    "availableRelativeLength",
                    budget.available_relative_utf16.to_string(),
                ),
                ("maxRelativeLength", budget.max_relative_utf16.to_string()),
                ("maxAbsoluteLength", budget.max_absolute_utf16.to_string()),
            ],
        ));
    }
    Ok(())
}

fn utf16_len(path: &Path) -> usize {
    path.to_string_lossy().encode_utf16().count()
}

fn absolute_lexical(path: &Path) -> AppResult<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(absolute)
}

pub fn validate_existing_chain(root: &Path, target: &Path) -> AppResult<()> {
    if !target.starts_with(root) {
        return Err(AppError::coded("path_outside_registered_root"));
    }
    let mut current = root.to_path_buf();
    if current.exists() {
        validate_existing_entry(&current)?;
    }
    let relative = target
        .strip_prefix(root)
        .map_err(|_| AppError::coded("path_outside_registered_root"))?;
    for component in relative.components() {
        current.push(component.as_os_str());
        if current.exists() {
            validate_existing_entry(&current)?;
        }
    }
    Ok(())
}

fn validate_existing_entry(path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path)?;

    // On Windows, directory junctions and symbolic links are both represented
    // by reparse points. Classify the Windows-specific reparse condition first
    // so a verified junction always produces the stable documented error code.
    // On non-Windows platforms `is_reparse_point` is false, so symbolic links
    // continue to use the platform-neutral symlink error code.
    if is_reparse_point(&metadata) {
        return Err(AppError::coded_with(
            "path_reparse_point_forbidden",
            [("path", path.display().to_string())],
        ));
    }
    if metadata.file_type().is_symlink() {
        return Err(AppError::coded_with(
            "path_symlink_forbidden",
            [("path", path.display().to_string())],
        ));
    }
    if metadata.is_file() && hard_link_count(&metadata, path)? > 1 {
        return Err(AppError::coded_with(
            "path_hardlink_forbidden",
            [("path", path.display().to_string())],
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn hard_link_count(metadata: &fs::Metadata, _: &Path) -> AppResult<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(metadata.nlink())
}

#[cfg(windows)]
fn hard_link_count(_: &fs::Metadata, path: &Path) -> AppResult<u64> {
    use std::{fs::OpenOptions, mem::MaybeUninit, os::windows::io::AsRawHandle};

    #[repr(C)]
    #[allow(dead_code)]
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
    let success = unsafe {
        get_file_information_by_handle(file.as_raw_handle().cast(), information.as_mut_ptr())
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(unsafe { information.assume_init() }.number_of_links.into())
}

#[cfg(not(any(unix, windows)))]
fn hard_link_count(_: &fs::Metadata, _: &Path) -> AppResult<u64> {
    Ok(1)
}

#[cfg(windows)]
fn normalize_unicode_key(value: &str) -> AppResult<String> {
    use std::ptr;
    const NORMALIZATION_C: i32 = 0x1;

    #[link(name = "normaliz")]
    extern "system" {
        #[link_name = "NormalizeString"]
        fn normalize_string(
            norm_form: i32,
            source: *const u16,
            source_length: i32,
            destination: *mut u16,
            destination_length: i32,
        ) -> i32;
    }

    let source: Vec<u16> = value.encode_utf16().collect();
    let required = unsafe {
        normalize_string(
            NORMALIZATION_C,
            source.as_ptr(),
            source.len() as i32,
            ptr::null_mut(),
            0,
        )
    };
    if required <= 0 {
        return Err(AppError::coded("path_unicode_normalization_failed"));
    }
    let mut destination = vec![0u16; required as usize];
    let written = unsafe {
        normalize_string(
            NORMALIZATION_C,
            source.as_ptr(),
            source.len() as i32,
            destination.as_mut_ptr(),
            destination.len() as i32,
        )
    };
    if written <= 0 {
        return Err(AppError::coded("path_unicode_normalization_failed"));
    }
    destination.truncate(written as usize);
    Ok(String::from_utf16_lossy(&destination).to_lowercase())
}

#[cfg(not(windows))]
fn normalize_unicode_key(value: &str) -> AppResult<String> {
    // Windows is the Phase-1 reference platform and performs full NFC normalization.
    // On other platforms combining-mark path components are rejected conservatively,
    // preventing ambiguous composed/decomposed aliases until a native adapter is added.
    if value.chars().any(is_combining_mark) {
        return Err(AppError::coded("path_unicode_combining_mark_forbidden"));
    }
    Ok(value.to_lowercase())
}

#[cfg(not(windows))]
fn is_combining_mark(value: char) -> bool {
    matches!(
        value as u32,
        0x0300..=0x036F
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> (PathRegistry, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "s9lab-path-test-{}-{}",
            std::process::id(),
            crate::operations::model::new_identifier("path")
        ));
        fs::create_dir_all(&root).expect("create root");
        let registry = PathRegistry::new(
            &root,
            [RegisteredRoot {
                id: "test".into(),
                path: root.clone(),
            }],
        )
        .expect("registry");
        (registry, root)
    }

    #[test]
    fn rejects_traversal_and_windows_special_cases() {
        let (registry, root) = registry();
        for invalid in [
            "../escape",
            "a/../escape",
            "C:/escape",
            "\\\\server\\share",
            "CON",
            "aux.txt",
            "LPT9.log",
            "file.txt:stream",
            "trailing.",
            "trailing ",
            "mixed\\..\\escape",
            "double//separator",
        ] {
            assert!(registry.resolve("test", invalid).is_err(), "{invalid}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_case_collisions() {
        let (registry, root) = registry();
        let result = registry.validate_unique("test", ["Mods/Test.jar", "mods/test.JAR"]);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn detects_unicode_normalization_collisions_on_windows() {
        let (registry, root) = registry();
        let result = registry.validate_unique("test", ["ä.txt", "a\u{0308}.txt"]);
        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(not(windows))]
    #[test]
    fn conservatively_rejects_combining_marks_off_windows() {
        let (registry, root) = registry();
        assert!(registry.resolve("test", "a\u{0308}.txt").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_existing_hardlinks() {
        let (registry, root) = registry();
        let first = root.join("first.bin");
        let second = root.join("second.bin");
        fs::write(&first, b"x").expect("write hardlink source");
        fs::hard_link(&first, &second).expect("create hardlink test fixture");

        let first_metadata = fs::symlink_metadata(&first).expect("read hardlink metadata");
        let link_count = hard_link_count(&first_metadata, &first)
            .expect("read hardlink count after fixture creation");
        assert!(
            link_count > 1,
            "hardlink fixture was not created; link count was {link_count}"
        );

        let first_error = registry
            .resolve("test", "first.bin")
            .expect_err("first hardlink must be rejected");
        let second_error = registry
            .resolve("test", "second.bin")
            .expect_err("second hardlink must be rejected");
        assert_eq!(first_error.descriptor().code, "path_hardlink_forbidden");
        assert_eq!(second_error.descriptor().code, "path_hardlink_forbidden");
        fs::remove_dir_all(root).expect("remove hardlink test root");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_existing_symlinks() {
        use std::os::unix::fs::symlink;
        let (registry, root) = registry();
        fs::create_dir_all(root.join("real")).expect("real");
        symlink(root.join("real"), root.join("link")).expect("symlink");
        assert!(registry.resolve("test", "link/file").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn validates_registered_root_from_launcher_anchor() {
        use std::os::unix::fs::symlink;

        let launcher_root = std::env::temp_dir().join(format!(
            "s9lab-path-anchor-test-{}-{}",
            std::process::id(),
            crate::operations::model::new_identifier("anchor")
        ));
        let profiles = launcher_root.join("profiles");
        let outside = launcher_root.with_extension("outside");
        fs::create_dir_all(&profiles).expect("profiles");
        fs::create_dir_all(&outside).expect("outside");
        let registry = PathRegistry::new(
            &launcher_root,
            [RegisteredRoot {
                id: "profiles".into(),
                path: profiles.clone(),
            }],
        )
        .expect("registry");

        fs::remove_dir(&profiles).expect("remove profiles");
        symlink(&outside, &profiles).expect("replace profiles with symlink");
        assert!(registry.resolve("profiles", "profile-a").is_err());

        let _ = fs::remove_file(&profiles);
        let _ = fs::remove_dir_all(&launcher_root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn safely_normalizes_non_ambiguous_backslash_separators() {
        let (registry, root) = registry();
        let resolved = registry
            .resolve("test", "mods\\example.jar")
            .expect("safe separator normalization");
        assert_eq!(resolved.relative(), Path::new("mods/example.jar"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn accepts_path_at_the_available_relative_boundary() {
        let root = if cfg!(windows) {
            PathBuf::from(r"C:\S9Lab")
        } else {
            PathBuf::from("/tmp/S9Lab")
        };
        let budget = path_length_budget(&root).expect("calculate path budget");
        let relative = PathBuf::from("a".repeat(budget.available_relative_utf16));
        let absolute = root.join(&relative);
        enforce_length_limits(&root, &relative, &absolute)
            .expect("path at the documented boundary must be accepted");
    }

    #[test]
    fn rejects_path_one_unit_beyond_the_available_relative_boundary() {
        let root = if cfg!(windows) {
            PathBuf::from(r"C:\S9Lab")
        } else {
            PathBuf::from("/tmp/S9Lab")
        };
        let budget = path_length_budget(&root).expect("calculate path budget");
        let relative = PathBuf::from("a".repeat(budget.available_relative_utf16 + 1));
        let absolute = root.join(&relative);
        let error = enforce_length_limits(&root, &relative, &absolute)
            .expect_err("path beyond the documented boundary must be rejected");
        assert_eq!(error.descriptor().code, "path_too_long");
    }

    #[test]
    fn absolute_path_budget_accounts_for_the_registered_root_length() {
        let prefix = if cfg!(windows) { r"C:\" } else { "/" };
        let prefix_len = prefix.encode_utf16().count();
        let desired_root_len = LEGACY_SAFE_MAX_ABSOLUTE_UTF16 - 21;
        let root = PathBuf::from(format!(
            "{prefix}{}",
            "r".repeat(desired_root_len - prefix_len)
        ));
        let budget = path_length_budget(&root).expect("calculate root-limited path budget");
        assert_eq!(budget.root_utf16, desired_root_len);
        assert_eq!(budget.available_relative_utf16, 20);

        let allowed = PathBuf::from("a".repeat(20));
        let allowed_absolute = root.join(&allowed);
        enforce_length_limits(&root, &allowed, &allowed_absolute)
            .expect("path at absolute budget must be accepted");

        let rejected = PathBuf::from("a".repeat(21));
        let rejected_absolute = root.join(&rejected);
        let error = enforce_length_limits(&root, &rejected, &rejected_absolute)
            .expect_err("path beyond absolute budget must be rejected");
        assert_eq!(error.descriptor().code, "path_too_long");
    }

    #[test]
    fn rejects_component_and_relative_paths_beyond_their_limits() {
        let (registry, root) = registry();
        let component = "a".repeat(MAX_COMPONENT_UTF16 + 1);
        let component_error = registry
            .resolve("test", component)
            .expect_err("overlong component must be rejected");
        assert_eq!(component_error.descriptor().code, "path_component_too_long");

        let budget = registry.length_budget("test").expect("path budget");
        let long_relative = "a".repeat(budget.available_relative_utf16 + 1);
        let relative_error = registry
            .resolve("test", long_relative)
            .expect_err("overlong relative path must be rejected");
        assert_eq!(relative_error.descriptor().code, "path_too_long");
        fs::remove_dir_all(root).expect("remove path length test root");
    }

    #[cfg(windows)]
    #[derive(Debug)]
    struct VerifiedWindowsJunctionFixture {
        launcher_root: PathBuf,
        profiles: PathBuf,
        target: PathBuf,
        target_marker: PathBuf,
    }

    #[cfg(windows)]
    impl VerifiedWindowsJunctionFixture {
        fn create() -> (PathRegistry, Self) {
            use serde::Deserialize;
            use std::process::Command;

            #[derive(Debug, Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct JunctionProbe {
                link_exists: bool,
                target_exists: bool,
                attributes: String,
                link_type: Option<String>,
            }

            let launcher_root = std::env::temp_dir().join(format!(
                "s9lab-path-junction-test-{}-{}",
                std::process::id(),
                crate::operations::model::new_identifier("junction")
            ));
            let profiles = launcher_root.join("profiles");
            let target = launcher_root.with_extension("junction-target");
            let target_marker = target.join("target-must-survive.txt");
            fs::create_dir_all(&profiles).expect("create registered profiles directory");
            fs::create_dir_all(&target).expect("create junction target directory");
            fs::write(&target_marker, b"junction target content")
                .expect("create junction target marker");
            let registry = PathRegistry::new(
                &launcher_root,
                [RegisteredRoot {
                    id: "profiles".into(),
                    path: profiles.clone(),
                }],
            )
            .expect("create path registry");

            fs::remove_dir(&profiles).expect("remove profiles directory before creating junction");
            let command = r#"
$ErrorActionPreference = 'Stop'
$link = $env:S9LAB_JUNCTION_LINK
$target = $env:S9LAB_JUNCTION_TARGET
$item = New-Item -ItemType Junction -Path $link -Target $target -ErrorAction Stop
$linkType = $null
if ($null -ne $item.PSObject.Properties['LinkType']) {
    $linkType = [string]$item.LinkType
}
[ordered]@{
    linkExists = Test-Path -LiteralPath $link -PathType Container
    targetExists = Test-Path -LiteralPath $target -PathType Container
    attributes = [string]$item.Attributes
    linkType = $linkType
} | ConvertTo-Json -Compress
"#;
            let output = Command::new("powershell.exe")
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    command,
                ])
                .env("S9LAB_JUNCTION_LINK", &profiles)
                .env("S9LAB_JUNCTION_TARGET", &target)
                .output()
                .expect("execute PowerShell junction fixture setup");
            assert!(
                output.status.success(),
                "junction fixture creation failed: status={:?}, stdout={}, stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );

            let probe: JunctionProbe = serde_json::from_slice(&output.stdout)
                .expect("PowerShell junction fixture must return valid JSON");
            assert!(probe.link_exists, "PowerShell did not create the junction");
            assert!(probe.target_exists, "junction target does not exist");
            assert!(
                probe.attributes.contains("ReparsePoint"),
                "PowerShell item lacks ReparsePoint attribute: {}",
                probe.attributes
            );
            if let Some(link_type) = probe.link_type.as_deref().filter(|value| !value.is_empty()) {
                assert_eq!(link_type, "Junction", "created link has wrong LinkType");
            }
            assert!(target_marker.is_file(), "junction target marker is missing");

            let junction_metadata =
                fs::symlink_metadata(&profiles).expect("read created junction metadata");
            assert!(
                is_reparse_point(&junction_metadata),
                "junction fixture exists but is not marked as a reparse point"
            );

            (
                registry,
                Self {
                    launcher_root,
                    profiles,
                    target,
                    target_marker,
                },
            )
        }

        fn cleanup(self) {
            fs::remove_dir(&self.profiles).expect("remove only the junction fixture");
            assert!(
                self.target_marker.is_file(),
                "removing the junction must not remove target content"
            );
            assert!(
                fs::symlink_metadata(&self.profiles).is_err(),
                "junction path still exists after cleanup"
            );
            fs::remove_dir_all(&self.launcher_root).expect("remove junction launcher root");
            fs::remove_dir_all(&self.target).expect("remove junction target after verification");
        }
    }

    #[cfg(windows)]
    #[test]
    fn classifies_verified_windows_junctions_with_the_stable_reparse_error() {
        let (_registry, fixture) = VerifiedWindowsJunctionFixture::create();

        let error = validate_existing_entry(&fixture.profiles)
            .expect_err("verified junction must be rejected as a reparse point");
        assert_eq!(error.descriptor().code, "path_reparse_point_forbidden");

        fixture.cleanup();
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_directory_junctions_after_verified_fixture_creation() {
        let (registry, fixture) = VerifiedWindowsJunctionFixture::create();

        let error = registry
            .resolve("profiles", "profile-a")
            .expect_err("junction-backed registered root must be rejected");
        assert_eq!(error.descriptor().code, "path_reparse_point_forbidden");

        fixture.cleanup();
    }
}
