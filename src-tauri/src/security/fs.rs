use crate::{
    error::{AppError, AppResult},
    security::paths::{validate_existing_chain, SecurePath},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

pub fn create_parent_directories(path: &SecurePath) -> AppResult<()> {
    let parent = path
        .absolute()
        .parent()
        .ok_or_else(|| AppError::coded("path_parent_missing"))?;
    create_directories_within(path.anchor(), path.root(), parent)
}

pub fn create_directories_within(
    anchor: &Path,
    allowed_root: &Path,
    target: &Path,
) -> AppResult<()> {
    if !allowed_root.starts_with(anchor) || !target.starts_with(allowed_root) {
        return Err(AppError::coded("path_outside_registered_root"));
    }
    let mut current = anchor.to_path_buf();
    fs::create_dir_all(&current)?;
    validate_existing_chain(anchor, &current)?;
    for component in target
        .strip_prefix(anchor)
        .map_err(|_| AppError::coded("path_outside_registered_root"))?
        .components()
    {
        current.push(component.as_os_str());
        if !current.exists() {
            fs::create_dir(&current)?;
        }
        validate_existing_chain(anchor, &current)?;
    }
    Ok(())
}

pub fn open_new_file(path: &SecurePath) -> AppResult<File> {
    create_parent_directories(path)?;
    if path.absolute().exists() {
        return Err(AppError::coded_with(
            "path_target_exists",
            [("path", path.relative().display().to_string())],
        ));
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.absolute())?;
    if let Err(error) = validate_existing_chain(path.anchor(), path.absolute()) {
        drop(file);
        let _ = fs::remove_file(path.absolute());
        return Err(error);
    }
    Ok(file)
}

pub fn write_new(path: &SecurePath, bytes: &[u8]) -> AppResult<()> {
    let mut file = open_new_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    validate_existing_chain(path.anchor(), path.absolute())?;
    sync_parent(path.absolute())?;
    Ok(())
}

pub fn copy_new(source: &SecurePath, destination: &SecurePath) -> AppResult<u64> {
    validate_existing_chain(source.anchor(), source.absolute())?;
    let source_metadata = fs::symlink_metadata(source.absolute())?;
    if !source_metadata.is_file() {
        return Err(AppError::coded("path_source_not_file"));
    }
    let mut input = File::open(source.absolute())?;
    let mut output = open_new_file(destination)?;
    let mut buffer = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        copied = copied
            .checked_add(read as u64)
            .ok_or_else(|| AppError::coded("file_size_overflow"))?;
    }
    output.sync_all()?;
    validate_existing_chain(destination.anchor(), destination.absolute())?;
    sync_parent(destination.absolute())?;
    Ok(copied)
}

pub fn rename_new(source: &SecurePath, destination: &SecurePath) -> AppResult<()> {
    if source.anchor() != destination.anchor() || source.root() != destination.root() {
        return Err(AppError::coded("path_cross_root_rename_forbidden"));
    }
    validate_move(source, destination)?;
    fs::rename(source.absolute(), destination.absolute())?;
    sync_move_parents(source.absolute(), destination.absolute())
}

pub fn rename_new_within_parent(
    source: &SecurePath,
    destination: &SecurePath,
    allowed_parent: &Path,
) -> AppResult<()> {
    if source.anchor() != destination.anchor()
        || !allowed_parent.starts_with(source.anchor())
        || !source.absolute().starts_with(allowed_parent)
        || !destination.absolute().starts_with(allowed_parent)
    {
        return Err(AppError::coded("path_cross_root_rename_forbidden"));
    }
    validate_move(source, destination)?;
    fs::rename(source.absolute(), destination.absolute())?;
    sync_move_parents(source.absolute(), destination.absolute())
}

fn validate_move(source: &SecurePath, destination: &SecurePath) -> AppResult<()> {
    validate_existing_chain(source.anchor(), source.absolute())?;
    create_parent_directories(destination)?;
    if destination.absolute().exists() {
        return Err(AppError::coded("path_target_exists"));
    }
    Ok(())
}

pub fn remove_tree(path: &SecurePath) -> AppResult<()> {
    if !path.absolute().exists() {
        return Ok(());
    }
    validate_existing_chain(path.anchor(), path.absolute())?;
    validate_tree(path.anchor(), path.absolute())?;
    let metadata = fs::symlink_metadata(path.absolute())?;
    if metadata.is_dir() {
        fs::remove_dir_all(path.absolute())?;
    } else {
        fs::remove_file(path.absolute())?;
    }
    sync_parent(path.absolute())?;
    Ok(())
}

fn validate_tree(anchor: &Path, path: &Path) -> AppResult<()> {
    validate_existing_chain(anchor, path)?;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            validate_tree(anchor, &entry?.path())?;
        }
    }
    Ok(())
}

fn sync_move_parents(source: &Path, destination: &Path) -> AppResult<()> {
    sync_parent(destination)?;
    if source.parent() != destination.parent() {
        sync_parent(source)?;
    }
    Ok(())
}

fn sync_parent(path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        #[cfg(unix)]
        {
            File::open(parent)?.sync_all()?;
        }
        #[cfg(not(unix))]
        {
            let _ = parent;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{PathRegistry, RegisteredRoot};

    #[test]
    fn secure_write_cannot_escape_registered_root() {
        let parent = std::env::temp_dir().join(format!(
            "s9lab-secure-write-test-{}-{}",
            std::process::id(),
            crate::operations::model::new_identifier("write")
        ));
        let root = parent.join("registered");
        std::fs::create_dir_all(&root).expect("registered root");
        let registry = PathRegistry::new(
            &root,
            [RegisteredRoot {
                id: "test".into(),
                path: root.clone(),
            }],
        )
        .expect("registry");
        let outside = parent.join("outside.bin");
        let attempted = registry
            .resolve("test", "../outside.bin")
            .and_then(|path| write_new(&path, b"must not escape"));
        assert!(attempted.is_err());
        assert!(!outside.exists());
        let _ = std::fs::remove_dir_all(parent);
    }
}
