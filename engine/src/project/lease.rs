//! Link-safe paths and content hashes: a path is refused when any part of it is a link or a reserved name, and a file is
//! hashed as it is read. The git broker, the copies and the check receipts use them.
use super::clean_path;
use crate::store::Fault;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseError {
    Path,
    Hash,
    Store(Fault),
}
impl LeaseError {
    pub fn key(self) -> &'static str {
        match self {
            Self::Path => "error.wire.path",
            Self::Hash => "reason.lease.hash",
            Self::Store(e) => e.key(),
        }
    }
}
impl From<Fault> for LeaseError {
    fn from(e: Fault) -> Self {
        Self::Store(e)
    }
}

pub fn hash(bytes: &[u8]) -> String {
    crate::hex(&Sha256::digest(bytes))
}
pub(crate) fn hash_reader(reader: &mut impl Read) -> Result<String, LeaseError> {
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let read = reader.read(&mut buffer).map_err(|_| Fault::Read)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(crate::hex(&digest.finalize()))
}

pub(crate) fn reparse(metadata: &fs::Metadata) -> bool {
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

pub(crate) fn normalized(raw: &str) -> Result<String, LeaseError> {
    let clean = clean_path(raw).ok_or(LeaseError::Path)?;
    if !clean.is_empty() {
        super::kit::delivery::scoped_path(&clean).map_err(|_| LeaseError::Path)?;
    }
    for part in clean.split('/').filter(|p| !p.is_empty()) {
        let lower = part.to_lowercase();
        let stem = lower.split('.').next().unwrap_or_default();
        if lower == ".git"
            || lower == ".faber"
            || part.ends_with([' ', '.'])
            || part.chars().any(char::is_control)
            || part.contains(['~', '*', '?', '[', ']', '<', '>', '|', '"'])
            || matches!(stem, "con" | "prn" | "aux" | "nul" | "conin$" | "conout$")
            || stem
                .strip_prefix("com")
                .or_else(|| stem.strip_prefix("lpt"))
                .is_some_and(|suffix| matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"))
        {
            return Err(LeaseError::Path);
        }
    }
    Ok(clean)
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
    fn reserved_and_link_prone_names_are_not_paths() {
        assert_eq!(normalized("src/a.rs").as_deref(), Ok("src/a.rs"));
        for bad in [".git/config", ".faber/x", "src/con.txt", "src/a ", "src/b.", "src/lpt1", "a/../b", "/abs", "x:y"] {
            assert_eq!(normalized(bad), Err(LeaseError::Path), "{bad:?}");
        }
    }
}
