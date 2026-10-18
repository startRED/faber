//! Link-safe paths and content hashes: a path is refused when any part of it is a link, and the SHA-256 of a text. The copies
//! and the checks use them.
use crate::store::Fault;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseError {
    Path,
    Store(Fault),
}

pub fn hash(bytes: &[u8]) -> String {
    crate::hex(&Sha256::digest(bytes))
}

fn reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Reject links in every existing ancestor, including Windows junctions. New paths resolve through their real parent.
pub fn real_path(path: &Path) -> Result<PathBuf, LeaseError> {
    if !path.is_absolute() {
        return Err(LeaseError::Path);
    }
    let mut part = PathBuf::new();
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(LeaseError::Path);
        }
        part.push(component);
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        if !part.has_root() {
            continue;
        }
        match fs::symlink_metadata(&part) {
            Ok(meta) if reparse(&meta) || (part != path && !meta.is_dir()) => return Err(LeaseError::Path),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(LeaseError::Store(Fault::Read)),
        }
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_validation_refuses_existing_file_ancestors_on_every_os() {
        let dir = tempfile::tempdir().unwrap();
        // macOS gives a temporary folder behind a link (/var is /private/var); compare real paths.
        let file = fs::canonicalize(dir.path()).unwrap().join("file");
        fs::write(&file, "bytes").unwrap();
        assert_eq!(real_path(&file.join("child")), Err(LeaseError::Path));
        assert_eq!(real_path(&file), Ok(file), "a regular final file is still a valid path");
    }

    #[test]
    fn a_hash_is_the_lowercase_sha256_of_the_bytes() {
        assert_eq!(hash(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
