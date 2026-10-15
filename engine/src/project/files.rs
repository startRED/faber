//! Broker file IO anchored to real directories, never to a second traversal through a worker-controlled link.
use super::lease::LeaseError;
use crate::store::Fault;
#[cfg(windows)]
use std::fs::{self, OpenOptions};
use std::fs::{File, Permissions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct ScopedFile {
    path: PathBuf,
    #[cfg(windows)]
    _pins: Vec<File>,
    #[cfg(unix)]
    parent: File,
}

#[cfg(windows)]
fn pins(path: &Path, create: bool) -> Result<Vec<File>, LeaseError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let mut pins = vec![];
    let mut part = PathBuf::new();
    for component in path.components() {
        part.push(component);
        if matches!(component, std::path::Component::Prefix(_)) {
            continue;
        }
        let mut options = OpenOptions::new();
        // BACKUP_SEMANTICS opens directories; OPEN_REPARSE_POINT inspects links; denying DELETE pins each ancestor.
        options.read(true).share_mode(3).custom_flags(0x0220_0000);
        let file = match options.open(&part) {
            Ok(file) => file,
            Err(e) if create && e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&part).map_err(|e| Fault::of_write(&e))?;
                options.open(&part).map_err(|_| LeaseError::Path)?
            }
            Err(_) => return Err(LeaseError::Path),
        };
        let meta = file.metadata().map_err(|_| Fault::Read)?;
        if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
            return Err(LeaseError::Path);
        }
        pins.push(file);
    }
    Ok(pins)
}

#[cfg(unix)]
fn directory(path: &Path, create: bool) -> Result<File, LeaseError> {
    use rustix::fs::{Mode, OFlags, mkdirat, openat};
    let mut parent = File::open("/").map_err(|_| Fault::Read)?;
    for part in path.components() {
        let std::path::Component::Normal(name) = part else { continue };
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let fd = match openat(&parent, name, flags, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if create => {
                mkdirat(&parent, name, Mode::from_bits_truncate(0o755)).map_err(|_| LeaseError::Path)?;
                openat(&parent, name, flags, Mode::empty()).map_err(|_| LeaseError::Path)?
            }
            Err(_) => return Err(LeaseError::Path),
        };
        parent = File::from(fd);
    }
    Ok(parent)
}

impl ScopedFile {
    pub fn new(path: &Path, create_parents: bool) -> Result<Self, LeaseError> {
        let parent = path.parent().ok_or(LeaseError::Path)?;
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(LeaseError::Path);
        }
        Ok(Self {
            path: path.into(),
            #[cfg(windows)]
            _pins: pins(parent, create_parents)?,
            #[cfg(unix)]
            parent: directory(parent, create_parents)?,
        })
    }

    fn open(&self) -> std::io::Result<File> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            OpenOptions::new().read(true).share_mode(1).custom_flags(0x0020_0000).open(&self.path)
        }
        #[cfg(unix)]
        {
            use rustix::fs::{Mode, OFlags, openat};
            openat(
                &self.parent,
                self.path.file_name().unwrap(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map(File::from)
            .map_err(std::io::Error::from)
        }
    }

    fn checked(&self) -> Result<Option<File>, LeaseError> {
        let file = match self.open() {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(LeaseError::Path),
        };
        let metadata = file.metadata().map_err(|_| Fault::Read)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(LeaseError::Path);
            }
        }
        if !metadata.is_file() {
            return Err(LeaseError::Path);
        }
        Ok(Some(file))
    }

    pub fn read(&self) -> Result<Option<(Vec<u8>, Permissions)>, LeaseError> {
        self.read_limited(super::wire::MAX_TEXT as u64)
    }

    pub fn read_limited(&self, limit: u64) -> Result<Option<(Vec<u8>, Permissions)>, LeaseError> {
        let Some(file) = self.checked()? else { return Ok(None) };
        let metadata = file.metadata().map_err(|_| Fault::Read)?;
        if metadata.len() > limit {
            return Err(LeaseError::Path);
        }
        let mut bytes = vec![];
        file.take(limit.saturating_add(1)).read_to_end(&mut bytes).map_err(|_| Fault::Read)?;
        if bytes.len() as u64 > limit {
            return Err(LeaseError::Path);
        }
        Ok(Some((bytes, metadata.permissions())))
    }

    pub fn hash(&self) -> Result<Option<String>, LeaseError> {
        self.checked()?.map(|mut file| super::lease::hash_reader(&mut file)).transpose()
    }

    /// Claim a leaf before inspecting it for deletion; a producer may safely reuse the original name.
    pub fn claim(&self, expected_hash: &str) -> Result<Self, LeaseError> {
        let target = Self::new(&self.path.with_file_name(format!(".faber-patch-{}.tmp", uuid::Uuid::new_v4())), false)?;
        self.rename(&target)?;
        // Keep a raced artifact under its claimed name for inspection; never return unproven bytes for deletion.
        if target.hash()?.as_deref() != Some(expected_hash) {
            return Err(LeaseError::Hash);
        }
        Ok(target)
    }

    pub fn write(&self, bytes: &[u8], permissions: Option<Permissions>) -> Result<(), LeaseError> {
        let temp = format!(".faber-patch-{}.tmp", uuid::Uuid::new_v4());
        #[cfg(windows)]
        let mut file =
            OpenOptions::new().write(true).create_new(true).open(self.path.with_file_name(&temp)).map_err(|e| Fault::of_write(&e))?;
        #[cfg(unix)]
        let mut file = {
            use rustix::fs::{Mode, OFlags, openat};
            File::from(
                openat(
                    &self.parent,
                    &temp,
                    OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::from_bits_truncate(0o644),
                )
                .map_err(|_| Fault::Write)?,
            )
        };
        let result = (|| -> Result<(), LeaseError> {
            file.write_all(bytes).and_then(|()| file.sync_all()).map_err(|e| Fault::of_write(&e))?;
            if let Some(permissions) = permissions {
                file.set_permissions(permissions).map_err(|e| Fault::of_write(&e))?;
            }
            drop(file);
            #[cfg(windows)]
            fs::rename(self.path.with_file_name(&temp), &self.path).map_err(|e| Fault::of_write(&e))?;
            #[cfg(unix)]
            rustix::fs::renameat(&self.parent, &temp, &self.parent, self.path.file_name().unwrap()).map_err(|_| Fault::Write)?;
            Ok(())
        })();
        if result.is_err() {
            #[cfg(windows)]
            let _ = fs::remove_file(self.path.with_file_name(&temp));
            #[cfg(unix)]
            let _ = rustix::fs::unlinkat(&self.parent, &temp, rustix::fs::AtFlags::empty());
        }
        result
    }

    pub fn rename(&self, target: &Self) -> Result<(), LeaseError> {
        #[cfg(windows)]
        fs::rename(&self.path, &target.path).map_err(|e| Fault::of_write(&e))?;
        #[cfg(unix)]
        rustix::fs::renameat(&self.parent, self.path.file_name().unwrap(), &target.parent, target.path.file_name().unwrap())
            .map_err(|_| Fault::Write)?;
        Ok(())
    }

    pub fn delete(&self) -> Result<(), LeaseError> {
        #[cfg(windows)]
        fs::remove_file(&self.path).map_err(|e| Fault::of_write(&e))?;
        #[cfg(unix)]
        rustix::fs::unlinkat(&self.parent, self.path.file_name().unwrap(), rustix::fs::AtFlags::empty()).map_err(|_| Fault::Write)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> tempfile::TempDir {
        // Match enrolled broker roots; macOS's ambient /var temp path is a system symlink.
        tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
    }

    #[test]
    fn failed_atomic_replacement_removes_its_temporary_payload() {
        let dir = tempdir();
        let target = dir.path().join("target");
        let file = ScopedFile::new(&target, false).unwrap();
        std::fs::create_dir(&target).unwrap();
        assert!(file.write(b"payload", None).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "failed writes must not leak .faber-patch files");
    }

    #[test]
    fn ancestor_swap_cannot_redirect_a_scoped_read_or_write() {
        let area = tempdir();
        let candidate = area.path().join("candidate");
        let outside = area.path().join("outside");
        std::fs::create_dir(&candidate).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(candidate.join("a"), "candidate").unwrap();
        std::fs::write(outside.join("a"), "outside").unwrap();
        let file = ScopedFile::new(&candidate.join("a"), false).unwrap();
        let moved = area.path().join("moved");
        #[cfg(windows)]
        assert!(std::fs::rename(&candidate, &moved).is_err());
        #[cfg(unix)]
        {
            std::fs::rename(&candidate, &moved).unwrap();
            std::os::unix::fs::symlink(&outside, &candidate).unwrap();
        }
        assert_eq!(file.read().unwrap().unwrap().0, b"candidate");
        file.write(b"approved", None).unwrap();
        assert_eq!(file.read().unwrap().unwrap().0, b"approved");
        assert_eq!(std::fs::read(outside.join("a")).unwrap(), b"outside");
    }

    #[test]
    fn existing_file_ancestors_and_directory_targets_are_refused() {
        let area = tempdir();
        std::fs::write(area.path().join("file"), "bytes").unwrap();
        assert!(matches!(ScopedFile::new(&area.path().join("file/child"), true), Err(LeaseError::Path)));
        std::fs::create_dir(area.path().join("folder")).unwrap();
        assert!(matches!(ScopedFile::new(&area.path().join("folder"), false).unwrap().read(), Err(LeaseError::Path)));
    }

    #[test]
    fn verified_cleanup_cannot_delete_a_new_file_reusing_the_staging_name() {
        let area = tempdir();
        let path = area.path().join("stale-build");
        std::fs::write(&path, b"old build").unwrap();
        let file = ScopedFile::new(&path, false).unwrap();
        assert_eq!(file.hash().unwrap().unwrap(), super::super::lease::hash(b"old build"));
        let claimed = file.claim(&super::super::lease::hash(b"old build")).unwrap();
        std::fs::write(&path, b"new build").unwrap();
        assert_eq!(claimed.hash().unwrap().unwrap(), super::super::lease::hash(b"old build"));
        claimed.delete().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new build");
    }

    #[test]
    fn claimed_cleanup_rechecks_a_leaf_replaced_after_the_inventory_hash() {
        let area = tempdir();
        let path = area.path().join("stale-build");
        std::fs::write(&path, b"old build").unwrap();
        let file = ScopedFile::new(&path, false).unwrap();
        let inventory_hash = file.hash().unwrap().unwrap();
        std::fs::write(&path, b"new build").unwrap();
        assert!(matches!(file.claim(&inventory_hash), Err(LeaseError::Hash)));
        let remaining: Vec<_> = std::fs::read_dir(area.path()).unwrap().map(|entry| entry.unwrap().path()).collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(std::fs::read(&remaining[0]).unwrap(), b"new build");
    }

    #[cfg(unix)]
    #[test]
    fn uncanonical_ancestor_aliases_are_still_refused() {
        let area = tempdir();
        let real = area.path().join("real");
        let alias = area.path().join("alias");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("a"), "bytes").unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert!(matches!(ScopedFile::new(&alias.join("a"), false), Err(LeaseError::Path)));
        assert_eq!(ScopedFile::new(&real.join("a"), false).unwrap().read().unwrap().unwrap().0, b"bytes");
    }
}
