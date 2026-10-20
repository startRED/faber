use super::{SwapStage, changed, compare, compare_displaced, relative, temporary_name};
use rustix::fs::Gid;
use rustix::fs::{AtFlags, Mode, OFlags, fchmod, fchown, mkdirat, open, openat, renameat, unlinkat};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const FLAGS: OFlags = OFlags::RDONLY.union(OFlags::NOFOLLOW).union(OFlags::NONBLOCK).union(OFlags::CLOEXEC);

fn private_staging(parent: &File, name: &str) -> io::Result<File> {
    let staging: File = openat(parent, name, FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
    let metadata = staging.metadata()?;
    if metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
        || metadata.dev() != parent.metadata()?.dev()
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(staging)
}

fn cleanup_name(name: &str) -> bool {
    name.strip_prefix(".faber-rewind-cleanup-").and_then(|suffix| suffix.split_once('-')).is_some_and(|(pid, number)| {
        !pid.is_empty() && !number.is_empty() && pid.bytes().chain(number.bytes()).all(|byte| byte.is_ascii_digit())
    })
}

fn identity_name(identity: &[u8]) -> String {
    format!(".identity-{}", identity.iter().map(|byte| format!("{byte:02x}")).collect::<String>())
}

fn saved_identity(name: &str) -> Option<Vec<u8>> {
    let hex = name.strip_prefix(".identity-")?;
    if hex.len() != 32 || !hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return None;
    }
    (0..hex.len()).step_by(2).map(|n| u8::from_str_radix(&hex[n..n + 2], 16).ok()).collect()
}

fn rename_noreplace(source: &File, from: &OsStr, target: &File, to: &OsStr) -> io::Result<()> {
    #[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android", target_os = "redox"))]
    {
        // One atomic syscall prevents a crash from leaving two links that safe history recovery must refuse.
        rustix::fs::renameat_with(source, from, target, to, rustix::fs::RenameFlags::NOREPLACE)?;
        Ok(())
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android", target_os = "redox")))]
    {
        let _ = (source, from, target, to);
        Err(io::ErrorKind::Unsupported.into())
    }
}

pub struct Root {
    dir: File,
    path: PathBuf,
    id: Vec<u8>,
}

pub fn file_identity(file: &File) -> io::Result<Vec<u8>> {
    let meta = file.metadata()?;
    Ok([meta.dev().to_le_bytes(), meta.ino().to_le_bytes()].concat())
}

pub fn resolved_path(_file: &File) -> io::Result<Option<PathBuf>> {
    // Validated Unix component names have no DOS short-name aliases.
    Ok(None)
}

impl Root {
    pub fn open(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let path = path.canonicalize()?;
        let mut dir: File = open("/", FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
        for part in path.components() {
            if let std::path::Component::Normal(name) = part {
                dir = openat(&dir, name, FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
            }
        }
        let id = file_identity(&dir)?;
        Ok(Self { dir, path, id })
    }

    pub fn identity(&self) -> io::Result<Vec<u8>> {
        Ok(self.id.clone())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn available_space(&self) -> io::Result<u64> {
        let stat = rustix::fs::fstatvfs(&self.dir)?;
        Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
    }

    fn parent(&self, path: &Path, create: bool) -> io::Result<(File, std::ffi::OsString)> {
        relative(path)?;
        if Self::open(&self.path)?.id != self.id {
            return Err(changed());
        }
        let mut parent = self.dir.try_clone()?;
        for part in path.parent().into_iter().flat_map(Path::components) {
            if create {
                match mkdirat(&parent, part.as_os_str(), Mode::from_raw_mode(0o700)) {
                    Ok(()) => parent.sync_all()?,
                    Err(rustix::io::Errno::EXIST) => (),
                    Err(e) => return Err(e.into()),
                }
            }
            parent = openat(&parent, part.as_os_str(), FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
            if part.as_os_str().to_string_lossy().starts_with(".faber-rewind-") {
                let metadata = parent.metadata()?;
                if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o777 != 0o700 {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
            }
        }
        Ok((parent, path.file_name().unwrap().to_owned()))
    }

    pub fn open_file(&self, path: &str) -> io::Result<File> {
        self.open_with(path, || {})
    }
    pub fn open_removable(&self, path: &str) -> io::Result<File> {
        self.open_file(path)
    }
    /// Quarantines a name before checking it against the exact object whose bytes were preserved.
    /// A different object stays on disk, at its original name when that name is still absent.
    pub fn remove_preserved(&self, path: &str, file: &File) -> io::Result<Option<String>> {
        self.remove_preserved_with(path, file, || Ok(()))
    }
    fn remove_preserved_with(&self, path: &str, file: &File, before_delete: impl FnOnce() -> io::Result<()>) -> io::Result<Option<String>> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let preserved = file_identity(&checked(file.try_clone()?)?)?;
        let (dir, name) = self.parent(Path::new(path), false)?;
        let source_parent = Path::new(path).parent().unwrap_or(Path::new(""));
        let (cleanup_parent, cleanup_path) = if name == "retained"
            && source_parent.file_name().and_then(OsStr::to_str).is_some_and(|name| name.starts_with(".faber-rewind-cleanup-"))
        {
            let old_name = source_parent.file_name().and_then(OsStr::to_str).ok_or(io::ErrorKind::InvalidInput)?;
            let base = source_parent.parent().ok_or(io::ErrorKind::InvalidInput)?;
            if !cleanup_name(old_name)
                || base.file_name().and_then(OsStr::to_str).is_some_and(|name| name.starts_with(".faber-rewind-cleanup-"))
            {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let (parent, _) = self.parent(source_parent, false)?;
            if file_identity(&private_staging(&parent, old_name)?)? != file_identity(&dir)? {
                return Err(changed());
            }
            // Abandoned quarantines are discoverable: move into a fresh sibling before the final identity check and unlink.
            (parent, base)
        } else {
            (dir.try_clone()?, source_parent)
        };
        let private = format!(".faber-rewind-cleanup-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        mkdirat(&cleanup_parent, private.as_str(), Mode::from_raw_mode(0o700))?;
        let staging = private_staging(&cleanup_parent, &private)?;
        let receipt = identity_name(&preserved);
        mkdirat(&staging, receipt.as_str(), Mode::from_raw_mode(0o700))?;
        staging.sync_all()?;
        cleanup_parent.sync_all()?;
        let retained = cleanup_path.join(&private).join("retained");
        if let Err(error) = rename_noreplace(&dir, &name, &staging, OsStr::new("retained")) {
            let _ = unlinkat(&staging, receipt.as_str(), AtFlags::REMOVEDIR);
            let _ = unlinkat(&cleanup_parent, private.as_str(), AtFlags::REMOVEDIR);
            return Err(error);
        }
        let moved = (|| {
            // No other operation knows this fresh private directory; the final unlink cannot race a project save.
            cleanup_parent.sync_all()?;
            dir.sync_all()?;
            staging.sync_all()?;
            let actual = match openat(&staging, "retained", FLAGS, Mode::empty()) {
                Ok(file) => File::from(file),
                Err(_) => return Ok(Some(())),
            };
            if file_identity(&actual)? != preserved || checked(actual).is_err() {
                return Ok(Some(()));
            }
            before_delete()?;
            unlinkat(&staging, "retained", AtFlags::empty())?;
            staging.sync_all()?;
            Ok(None)
        })();
        match moved {
            Ok(None) => {
                unlinkat(&staging, receipt.as_str(), AtFlags::REMOVEDIR)?;
                unlinkat(&cleanup_parent, private.as_str(), AtFlags::REMOVEDIR)?;
                cleanup_parent.sync_all()?;
                Ok(None)
            }
            other => {
                // Never replace an editor's newer save while returning an unpreserved object.
                let restored = rename_noreplace(&staging, OsStr::new("retained"), &dir, &name).is_ok();
                if restored {
                    dir.sync_all()?;
                    let _ = unlinkat(&staging, receipt.as_str(), AtFlags::REMOVEDIR);
                    let _ = unlinkat(&cleanup_parent, private.as_str(), AtFlags::REMOVEDIR);
                    cleanup_parent.sync_all()?;
                }
                match other {
                    Ok(Some(())) => Ok(Some(if restored { path.to_owned() } else { retained.to_string_lossy().into_owned() })),
                    Err(error) => {
                        // The retained name remains discoverable by recovery if returning it failed.
                        if !restored && rustix::fs::statat(&staging, "retained", AtFlags::SYMLINK_NOFOLLOW).is_ok() {
                            return Ok(Some(retained.to_string_lossy().into_owned()));
                        }
                        Err(error)
                    }
                    Ok(None) => unreachable!(),
                }
            }
        }
    }
    /// Deletes only an immutable private-history file whose ownership name the caller already validated.
    /// Skipping the directory flush is safe here: an orphan resurrected by a crash is cleaned again at startup.
    pub fn remove_owned(&self, path: &str) -> io::Result<()> {
        self.remove_owned_with(path, || Ok(()))
    }
    fn remove_owned_with(&self, path: &str, before_delete: impl FnOnce() -> io::Result<()>) -> io::Result<()> {
        let file = self.open_removable(path)?;
        before_delete()?;
        if self.remove_preserved(path, &file)?.is_some() {
            return Err(changed());
        }
        Ok(())
    }
    fn open_with(&self, path: &str, before_leaf: impl FnOnce()) -> io::Result<File> {
        let (dir, name) = self.parent(Path::new(path), false)?;
        before_leaf();
        checked(openat(&dir, &name, FLAGS, Mode::empty())?.into())
    }

    pub fn private_write(&self, path: &str, bytes: &[u8]) -> io::Result<()> {
        let mut file = self.private_create(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        let (dir, name) = self.parent(Path::new(path), false)?;
        let _ = name;
        dir.sync_all()
    }

    pub fn private_create(&self, path: &str) -> io::Result<File> {
        let (dir, name) = self.parent(Path::new(path), false)?;
        Ok(openat(
            &dir,
            &name,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?
        .into())
    }

    pub fn rename_new(&self, from: &str, to: &str) -> io::Result<()> {
        let (source, source_name) = self.parent(Path::new(from), false)?;
        let (target, target_name) = self.parent(Path::new(to), false)?;
        let metadata = rustix::fs::statat(&source, &source_name, AtFlags::SYMLINK_NOFOLLOW)?;
        if !rustix::fs::FileType::from_raw_mode(metadata.st_mode).is_file() || metadata.st_nlink != 1 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        // Returning a displaced version must work even when its bytes cannot be opened or captured.
        rename_noreplace(&source, &source_name, &target, &target_name)?;
        target.sync_all()?;
        source.sync_all()
    }

    /// Atomic replacement is restricted to metadata in the caller's owner-only history namespace.
    pub fn replace_owned(&self, path: &str, expected: (&[u8], &[u8]), desired: (&[u8], &[u8])) -> io::Result<()> {
        self.replace_owned_with(path, expected, desired, || Ok(()))
    }

    pub(crate) fn replace_owned_with(
        &self,
        path: &str,
        expected: (&[u8], &[u8]),
        desired: (&[u8], &[u8]),
        before: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let (dir, name) = self.parent(Path::new(path), false)?;
        let metadata = dir.metadata()?;
        if metadata.uid() != rustix::process::geteuid().as_raw() || metadata.mode() & 0o777 != 0o700 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let current = checked(openat(&dir, &name, FLAGS, Mode::empty())?.into())?;
        compare(&current, expected)?;
        let temp = format!(".faber-rewind-metadata-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        let mut file: File = openat(
            &dir,
            temp.as_str(),
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?
        .into();
        let result = (|| {
            file.write_all(desired.0)?;
            apply(&file, desired.1)?;
            file.sync_all()?;
            compare(&current, expected)?;
            before()?;
            renameat(&dir, temp.as_str(), &dir, &name)?;
            dir.sync_all()
        })();
        if result.is_err() {
            let _ = unlinkat(&dir, temp.as_str(), AtFlags::empty());
        }
        result
    }

    pub fn mutate(&self, path: &str, expected: Option<(&[u8], &[u8])>, desired: Option<(&[u8], &[u8])>) -> io::Result<()> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = format!(".faber-rewind-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
        self.mutate_journaled(path, expected, desired, &temp, |_| Ok(()))
    }

    pub fn mutate_journaled(
        &self,
        path: &str,
        expected: Option<(&[u8], &[u8])>,
        desired: Option<(&[u8], &[u8])>,
        temporary: &str,
        created: impl FnOnce(&File) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut created = Some(created);
        let result = self.mutate_staged(path, expected, desired, temporary, |stage, file| {
            if stage == SwapStage::Prepared {
                created.take().unwrap()(file)?;
            }
            Ok(())
        });
        let (_, aside) = super::swap_paths(path, temporary);
        if result.is_ok() {
            if expected.is_some() {
                self.remove_owned(&aside)?;
            }
        } else if self.open_file(path).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
            let _ = self.rename_new(&aside, path);
        }
        let _ = self.finish_swap(path, temporary);
        result
    }

    pub fn finish_swap(&self, path: &str, temporary: &str) -> io::Result<()> {
        if !self.recover_cleanup(path, temporary)?.is_empty() {
            return Err(changed());
        }
        temporary_name(temporary)?;
        let (parent, _) = self.parent(Path::new(path), false)?;
        match private_staging(&parent, temporary) {
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
        match unlinkat(&parent, temporary, AtFlags::REMOVEDIR) {
            Ok(()) => parent.sync_all(),
            Err(rustix::io::Errno::NOENT) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    /// Finds interrupted cleanup only inside the journal's validated private swap directory.
    /// Returned files must be preserved by the caller before removal; no file bytes are read here.
    #[allow(clippy::unnecessary_cast)]
    pub fn recover_cleanup(&self, path: &str, temporary: &str) -> io::Result<Vec<String>> {
        temporary_name(temporary)?;
        let (parent, _) = self.parent(Path::new(path), false)?;
        match private_staging(&parent, temporary) {
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(error),
        }
        let directory = Path::new(path).parent().unwrap_or(Path::new("")).join(temporary);
        self.recover_cleanup_directory(directory.to_str().ok_or(io::ErrorKind::InvalidInput)?)
    }

    /// Discovers cleanup beside project files or private history files, without reading a retained byte.
    pub fn recover_cleanup_directory(&self, directory: &str) -> io::Result<Vec<String>> {
        Ok(self.cleanup_records(directory, true)?.into_iter().map(|(path, _)| path).collect())
    }

    /// Bounded metadata-only traversal; symlinks and mounted volumes are never followed.
    pub fn recover_cleanup_tree(&self) -> io::Result<Vec<(String, Option<Vec<u8>>)>> {
        self.cleanup_tree(true)
    }

    /// Startup discovery can run while a live session still exists; it must not change project names.
    pub fn inspect_cleanup_tree(&self) -> io::Result<Vec<(String, Option<Vec<u8>>)>> {
        self.cleanup_tree(false)
    }

    fn cleanup_tree(&self, recover: bool) -> io::Result<Vec<(String, Option<Vec<u8>>)>> {
        let mut pending = vec![String::new()];
        let mut retained = vec![];
        let mut count = 0usize;
        let device = self.dir.metadata()?.dev();
        while let Some(relative) = pending.pop() {
            let (directory, _) = self.parent(&Path::new(&relative).join("entry"), false)?;
            retained.extend(self.cleanup_records(&relative, recover)?);
            for entry in rustix::fs::Dir::read_from(&directory)? {
                let entry = entry?;
                let Ok(name) = entry.file_name().to_str() else { continue };
                if matches!(name, "." | ".." | ".git") || cleanup_name(name) {
                    continue;
                }
                count += 1;
                if count > 200_000 {
                    return Err(io::ErrorKind::OutOfMemory.into());
                }
                let metadata = rustix::fs::statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW)?;
                if !rustix::fs::FileType::from_raw_mode(metadata.st_mode).is_dir() {
                    continue;
                }
                let child: File = openat(&directory, name, FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
                if child.metadata()?.dev() != device {
                    continue;
                }
                let path = if relative.is_empty() { name.into() } else { format!("{relative}/{name}") };
                pending.push(path);
            }
        }
        retained.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(retained)
    }

    #[allow(clippy::unnecessary_cast)]
    fn cleanup_records(&self, directory: &str, recover: bool) -> io::Result<Vec<(String, Option<Vec<u8>>)>> {
        let (staging, _) = self.parent(&Path::new(directory).join("entry"), false)?;
        let mut retained = vec![];
        for entry in rustix::fs::Dir::read_from(&staging)? {
            let entry = entry?;
            let Ok(name) = entry.file_name().to_str() else { continue };
            if !cleanup_name(name) {
                if name.starts_with(".faber-rewind-cleanup-") {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                continue;
            }
            let quarantine = private_staging(&staging, name)?;
            let mut found = false;
            let mut receipt = None;
            let mut marker = None;
            for child in rustix::fs::Dir::read_from(&quarantine)? {
                let child = child?;
                match child.file_name().to_bytes() {
                    b"." | b".." => continue,
                    b"retained" => (),
                    _ => {
                        let name = child.file_name().to_str().map_err(|_| io::ErrorKind::InvalidData)?;
                        let identity = saved_identity(name).ok_or(io::ErrorKind::PermissionDenied)?;
                        if receipt.is_some() {
                            return Err(io::ErrorKind::PermissionDenied.into());
                        }
                        let held = private_staging(&quarantine, name)?;
                        for entry in rustix::fs::Dir::read_from(&held)? {
                            if !matches!(entry?.file_name().to_bytes(), b"." | b"..") {
                                return Err(io::ErrorKind::PermissionDenied.into());
                            }
                        }
                        receipt = Some(identity);
                        marker = Some(name.to_owned());
                        continue;
                    }
                }
                let metadata = rustix::fs::statat(&quarantine, "retained", AtFlags::SYMLINK_NOFOLLOW)?;
                if !rustix::fs::FileType::from_raw_mode(metadata.st_mode).is_file()
                    || metadata.st_nlink != 1
                    || metadata.st_dev as u64 != staging.metadata()?.dev()
                {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
                found = true;
            }
            if found {
                retained.push((Path::new(directory).join(name).join("retained").to_string_lossy().into_owned(), receipt));
            } else if recover {
                if let Some(marker) = marker {
                    unlinkat(&quarantine, marker.as_str(), AtFlags::REMOVEDIR)?;
                    quarantine.sync_all()?;
                }
                unlinkat(&staging, name, AtFlags::REMOVEDIR)?;
                staging.sync_all()?;
            }
        }
        retained.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(retained)
    }

    pub fn validate_swap(&self, path: &str, temporary: &str) -> io::Result<()> {
        temporary_name(temporary)?;
        let (parent, _) = self.parent(Path::new(path), false)?;
        private_staging(&parent, temporary).map(|_| ())
    }

    // Native dev_t and ino_t have different widths across Unix platforms.
    #[allow(clippy::unnecessary_cast)]
    pub fn mutate_staged(
        &self,
        path: &str,
        expected: Option<(&[u8], &[u8])>,
        desired: Option<(&[u8], &[u8])>,
        temporary: &str,
        mut journal: impl FnMut(SwapStage, &File) -> io::Result<()>,
    ) -> io::Result<()> {
        temporary_name(temporary)?;
        let (dir, name) = self.parent(Path::new(path), expected.is_none() && desired.is_some())?;
        let current = match openat(&dir, &name, FLAGS, Mode::empty()) {
            Ok(file) => Some(checked(file.into())?),
            Err(rustix::io::Errno::NOENT) => None,
            Err(e) => return Err(e.into()),
        };
        match (&current, expected) {
            (Some(file), Some(wanted)) => compare(file, wanted)?,
            (None, None) => (),
            _ => return Err(changed()),
        }
        mkdirat(&dir, temporary, Mode::from_raw_mode(0o700))?;
        dir.sync_all()?;
        let staging = private_staging(&dir, temporary)?;
        let temp = "new";
        let aside = "old";
        let mut owned = None;
        let mut displacement_started = false;
        let result = (|| {
            let prepared = if let Some((bytes, saved)) = desired {
                let mut file: File = openat(
                    &staging,
                    temp,
                    OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o600),
                )?
                .into();
                owned = Some(file_identity(&file)?);
                journal(SwapStage::Prepared, &file)?;
                file.write_all(bytes)?;
                let security = if let Some(old) = current.as_ref() { protection(old)? } else { saved.to_vec() };
                apply(&file, &security)?;
                file.sync_all()?;
                let published = checked(openat(&staging, temp, FLAGS, Mode::empty())?.into())?;
                if file_identity(&published)? != file_identity(&file)? {
                    return Err(changed());
                }
                compare(&published, (&fingerprint(&file)?, bytes))?;
                Some(file)
            } else {
                None
            };
            if let (Some(original), Some(wanted)) = (&current, expected) {
                compare(original, wanted)?;
                journal(SwapStage::BeforeDisplace, original)?;
                displacement_started = true;
                rename_noreplace(&dir, &name, &staging, OsStr::new(aside))?;
                dir.sync_all()?;
                staging.sync_all()?;
                let moved = checked(openat(&staging, aside, FLAGS, Mode::empty())?.into())?;
                journal(SwapStage::Displaced, &moved)?;
                compare_displaced(&moved, original, wanted.1)?;
                if desired.is_none() {
                    journal(SwapStage::BeforeInstall, &moved)?;
                }
            }
            if let Some(file) = prepared {
                journal(SwapStage::BeforeInstall, &file)?;
                rename_noreplace(&staging, OsStr::new(temp), &dir, &name)?;
                dir.sync_all()?;
            }
            Ok(())
        })();
        // Recovery owns both names as soon as a displacement can have happened.
        if !displacement_started
            && let Some(owned) = owned
            && let Ok(meta) = rustix::fs::statat(&staging, temp, AtFlags::SYMLINK_NOFOLLOW)
            && [(meta.st_dev as u64).to_le_bytes(), (meta.st_ino as u64).to_le_bytes()].concat() == owned
        {
            let _ = unlinkat(&staging, temp, AtFlags::empty());
        }
        if !displacement_started {
            let _ = self.finish_swap(path, temporary);
        }
        result
    }
}

fn checked(file: File) -> io::Result<File> {
    let meta = file.metadata()?;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(file)
}

pub fn open_plain_file(root: &Path, relative: &Path) -> io::Result<File> {
    let text = relative.to_str().ok_or(io::ErrorKind::InvalidInput)?;
    Root::open(root)?.open_file(text)
}

pub fn protection(file: &File) -> io::Result<Vec<u8>> {
    let meta = file.metadata()?;
    Ok([meta.gid().to_le_bytes(), (meta.mode() & 0o7777).to_le_bytes()].concat())
}

fn apply(file: &File, protection: &[u8]) -> io::Result<()> {
    if protection.len() != 8 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let gid = u32::from_le_bytes(protection[..4].try_into().unwrap());
    let mode = u32::from_le_bytes(protection[4..].try_into().unwrap());
    if mode & !0o7777 != 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    fchown(file, None, Some(Gid::from_raw(gid)))?;
    fchmod(file, Mode::from_raw_mode(mode as _))?;
    Ok(())
}

pub fn fingerprint(file: &File) -> io::Result<Vec<u8>> {
    Ok(capture_metadata(file)?.0)
}

pub fn capture_metadata(file: &File) -> io::Result<super::CaptureMetadata> {
    let meta = file.metadata()?;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let fingerprint =
        [meta.dev(), meta.ino(), meta.len(), meta.mtime() as u64, meta.mtime_nsec() as u64, meta.ctime() as u64, meta.ctime_nsec() as u64]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
    Ok((fingerprint, None, meta.len()))
}

pub fn cache_fingerprint(file: &File) -> io::Result<Option<Vec<u8>>> {
    // Timestamp precision varies by filesystem; without an authoritative generation, always read the held file.
    Ok(capture_metadata(file)?.1)
}

pub fn allocated(file: &File) -> io::Result<u64> {
    Ok(file.metadata()?.blocks().saturating_mul(512))
}

pub fn private_dir(path: &Path) -> io::Result<()> {
    let parent = Root::open(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    match mkdirat(&parent.dir, name, Mode::from_raw_mode(0o700)) {
        Ok(()) => (),
        Err(rustix::io::Errno::EXIST) => (),
        Err(e) => return Err(e.into()),
    }
    let file: File = openat(&parent.dir, name, FLAGS | OFlags::DIRECTORY, Mode::empty())?.into();
    if file.metadata()?.uid() != rustix::process::geteuid().as_raw() {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    fchmod(&file, Mode::from_raw_mode(0o700))?;
    file.sync_all()?;
    parent.dir.sync_all()
}

pub fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    Root::open(parent)?.private_write(path.file_name().and_then(|s| s.to_str()).ok_or(io::ErrorKind::InvalidInput)?, bytes)
}

pub fn private_create(path: &Path) -> io::Result<File> {
    Root::open(path.parent().ok_or(io::ErrorKind::InvalidInput)?)?
        .private_create(path.file_name().and_then(|s| s.to_str()).ok_or(io::ErrorKind::InvalidInput)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewind_tree_finds_durable_preservation_receipts_at_project_and_history_depths() {
        for parent in ["", "objects/aa"] {
            let folder = tempfile::tempdir().unwrap();
            if !parent.is_empty() {
                std::fs::create_dir_all(folder.path().join(parent)).unwrap();
            }
            let root = Root::open(folder.path()).unwrap();
            let source = Path::new(parent).join("retired").to_string_lossy().into_owned();
            std::fs::write(folder.path().join(&source), b"preserved before cleanup").unwrap();
            let held = root.open_removable(&source).unwrap();
            let identity = file_identity(&held).unwrap();
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    root.remove_preserved_with(&source, &held, || panic!("crash after source rename")).unwrap();
                }))
                .is_err()
            );
            let discovered = root.inspect_cleanup_tree().unwrap();
            assert_eq!(discovered.len(), 1);
            assert_eq!(discovered[0].1.as_ref(), Some(&identity), "the receipt was durable before displacement");
            assert_eq!(root.inspect_cleanup_tree().unwrap(), discovered, "inspection never changes live names");
            let retained = root.open_removable(&discovered[0].0).unwrap();
            root.remove_preserved(&discovered[0].0, &retained).unwrap();
            assert!(root.recover_cleanup_tree().unwrap().is_empty());
            assert!(root.recover_cleanup_tree().unwrap().is_empty());
        }
    }

    #[test]
    fn rewind_tree_only_cleans_empty_receipt_directories_after_mutation_is_admitted() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let cleanup = ".faber-rewind-cleanup-0-4";
        mkdirat(&root.dir, cleanup, Mode::from_raw_mode(0o700)).unwrap();
        let held = private_staging(&root.dir, cleanup).unwrap();
        let receipt = identity_name(&[0; 16]);
        mkdirat(&held, receipt.as_str(), Mode::from_raw_mode(0o700)).unwrap();
        assert!(root.inspect_cleanup_tree().unwrap().is_empty());
        assert!(folder.path().join(cleanup).join(&receipt).is_dir(), "inspection cannot disrupt an in-flight rename");
        assert!(root.recover_cleanup_tree().unwrap().is_empty());
        assert!(!folder.path().join(cleanup).exists());
        mkdirat(&root.dir, cleanup, Mode::from_raw_mode(0o700)).unwrap();
        let held = private_staging(&root.dir, cleanup).unwrap();
        mkdirat(&held, receipt.as_str(), Mode::from_raw_mode(0o700)).unwrap();
        let unexpected = folder.path().join(cleanup).join(&receipt).join("do-not-remove");
        std::fs::write(&unexpected, b"unexpected bytes are retained").unwrap();
        assert!(root.inspect_cleanup_tree().is_err());
        assert!(root.recover_cleanup_tree().is_err());
        assert_eq!(std::fs::read(unexpected).unwrap(), b"unexpected bytes are retained");
    }

    #[test]
    fn rewind_recovered_cleanup_keeps_a_replacement_in_the_abandoned_quarantine() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let temporary = ".faber-rewind-restart";
        mkdirat(&root.dir, temporary, Mode::from_raw_mode(0o700)).unwrap();
        let staging = private_staging(&root.dir, temporary).unwrap();
        mkdirat(&staging, ".faber-rewind-cleanup-0-4", Mode::from_raw_mode(0o700)).unwrap();
        let retained = format!("{temporary}/.faber-rewind-cleanup-0-4/retained");
        let old_name = folder.path().join(&retained);
        std::fs::write(&old_name, b"preserved B").unwrap();
        let held = root.open_removable(&retained).unwrap();
        assert_eq!(
            root.remove_preserved_with(&retained, &held, || {
                // This used to rename the validated file and put C under the final unlink's name.
                if old_name.exists() {
                    std::fs::rename(&old_name, folder.path().join("moved-B"))?;
                }
                std::fs::write(&old_name, b"unpreserved C")
            })
            .unwrap(),
            None
        );
        assert_eq!(std::fs::read(&old_name).unwrap(), b"unpreserved C");
        assert_eq!(held.metadata().unwrap().nlink(), 0, "only the already preserved B was deleted");
        assert_eq!(root.recover_cleanup("file", temporary).unwrap(), vec![retained]);
    }

    #[test]
    fn rewind_restart_discovers_a_fresh_sibling_after_recovered_cleanup_crashes() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let temporary = ".faber-rewind-restart";
        mkdirat(&root.dir, temporary, Mode::from_raw_mode(0o700)).unwrap();
        let staging = private_staging(&root.dir, temporary).unwrap();
        let old_cleanup = ".faber-rewind-cleanup-0-4";
        mkdirat(&staging, old_cleanup, Mode::from_raw_mode(0o700)).unwrap();
        let retained = format!("{temporary}/{old_cleanup}/retained");
        std::fs::write(folder.path().join(&retained), b"kept through two crashes").unwrap();
        let held = root.open_removable(&retained).unwrap();
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            root.remove_preserved_with(&retained, &held, || panic!("simulated crash before final unlink")).unwrap();
        }));
        assert!(interrupted.is_err());
        drop(held);
        drop(staging);
        drop(root);
        let restarted = Root::open(folder.path()).unwrap();
        let discovered = restarted.recover_cleanup("file", temporary).unwrap();
        assert_eq!(discovered.len(), 1);
        assert_ne!(discovered[0], retained);
        assert_eq!(Path::new(&discovered[0]).parent().unwrap().parent().unwrap(), Path::new(temporary));
        assert!(!folder.path().join(temporary).join(old_cleanup).exists(), "the previous empty quarantine is finished");
        assert_eq!(std::fs::read(folder.path().join(&discovered[0])).unwrap(), b"kept through two crashes");
        assert_eq!(restarted.recover_cleanup("file", temporary).unwrap(), discovered);
        let held = restarted.open_removable(&discovered[0]).unwrap();
        restarted.remove_preserved(&discovered[0], &held).unwrap();
        drop(held);
        restarted.finish_swap("file", temporary).unwrap();
        assert!(!folder.path().join(temporary).exists());
    }

    #[test]
    fn rewind_recovered_cleanup_rejects_malformed_or_nested_cleanup_namespaces() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        for parent in [".faber-rewind-restart/.faber-rewind-cleanup-bad", ".faber-rewind-cleanup-0-4/.faber-rewind-cleanup-0-5"] {
            use std::os::unix::fs::DirBuilderExt;
            let path = folder.path().join(parent);
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&path).unwrap();
            std::fs::write(path.join("retained"), b"leave in place").unwrap();
            let held = root.open_removable(&format!("{parent}/retained")).unwrap();
            assert_eq!(root.remove_preserved(&format!("{parent}/retained"), &held).unwrap_err().kind(), io::ErrorKind::InvalidInput);
            assert_eq!(std::fs::read(path.join("retained")).unwrap(), b"leave in place");
        }
    }

    #[test]
    fn rewind_restart_discovers_quarantined_versions_and_finishes_after_preserving_them() {
        for original in ["old", "new"] {
            let folder = tempfile::tempdir().unwrap();
            let root = Root::open(folder.path()).unwrap();
            let temporary = ".faber-rewind-restart";
            let private = ".faber-rewind-cleanup-0-4";
            mkdirat(&root.dir, temporary, Mode::from_raw_mode(0o700)).unwrap();
            let staging = private_staging(&root.dir, temporary).unwrap();
            mkdirat(&staging, private, Mode::from_raw_mode(0o700)).unwrap();
            std::fs::write(folder.path().join(temporary).join(original), b"version preserved across the crash").unwrap();
            std::fs::rename(folder.path().join(temporary).join(original), folder.path().join(temporary).join(private).join("retained"))
                .unwrap();
            drop(staging);
            drop(root);
            let restarted = Root::open(folder.path()).unwrap();
            let retained = restarted.recover_cleanup("file", temporary).unwrap();
            assert_eq!(retained, vec![format!("{temporary}/{private}/retained")]);
            assert_eq!(restarted.recover_cleanup("file", temporary).unwrap(), retained, "discovery never consumes a version");
            assert!(restarted.finish_swap("file", temporary).is_err(), "completion cannot abandon an unpreserved version");
            let mut held = restarted.open_removable(&retained[0]).unwrap();
            let mut bytes = vec![];
            held.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"version preserved across the crash");
            assert_eq!(restarted.remove_preserved(&retained[0], &held).unwrap(), None);
            drop(held);
            restarted.finish_swap("file", temporary).unwrap();
            assert!(!folder.path().join(temporary).exists());
            assert!(restarted.recover_cleanup("file", temporary).unwrap().is_empty());
            restarted.finish_swap("file", temporary).unwrap();
        }
    }

    #[test]
    fn rewind_restart_finishes_empty_quarantines_without_deleting_unrelated_entries() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let temporary = ".faber-rewind-restart";
        mkdirat(&root.dir, temporary, Mode::from_raw_mode(0o700)).unwrap();
        let staging = private_staging(&root.dir, temporary).unwrap();
        mkdirat(&staging, ".faber-rewind-cleanup-0-4", Mode::from_raw_mode(0o700)).unwrap();
        assert!(root.recover_cleanup("file", temporary).unwrap().is_empty());
        assert!(!folder.path().join(temporary).join(".faber-rewind-cleanup-0-4").exists());
        mkdirat(&staging, ".faber-rewind-cleanup-unrelated", Mode::from_raw_mode(0o700)).unwrap();
        assert!(root.recover_cleanup("file", temporary).is_err());
        assert!(root.finish_swap("file", temporary).is_err());
        assert!(folder.path().join(temporary).join(".faber-rewind-cleanup-unrelated").is_dir());
        unlinkat(&staging, ".faber-rewind-cleanup-unrelated", AtFlags::REMOVEDIR).unwrap();
        root.finish_swap("file", temporary).unwrap();
        assert!(!folder.path().join(temporary).exists());
    }

    #[test]
    fn rewind_restart_returns_an_unreadable_quarantine_without_replacing_another_version() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let temporary = ".faber-rewind-restart";
        mkdirat(&root.dir, temporary, Mode::from_raw_mode(0o700)).unwrap();
        let staging = private_staging(&root.dir, temporary).unwrap();
        mkdirat(&staging, ".faber-rewind-cleanup-0-4", Mode::from_raw_mode(0o700)).unwrap();
        let retained = format!("{temporary}/.faber-rewind-cleanup-0-4/retained");
        std::fs::write(folder.path().join(&retained), b"cannot be read for capture").unwrap();
        std::fs::set_permissions(folder.path().join(&retained), std::fs::Permissions::from_mode(0o0)).unwrap();
        std::fs::write(folder.path().join("file"), b"another version").unwrap();
        assert_eq!(root.recover_cleanup("file", temporary).unwrap(), vec![retained.clone()]);
        assert_eq!(root.rename_new(&retained, "file").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(folder.path().join("file")).unwrap(), b"another version");
        std::fs::rename(folder.path().join("file"), folder.path().join("other-version")).unwrap();
        root.rename_new(&retained, "file").unwrap();
        assert_eq!(std::fs::metadata(folder.path().join("file")).unwrap().mode() & 0o777, 0);
        root.finish_swap("file", temporary).unwrap();
        assert!(!folder.path().join(temporary).exists());
        std::fs::set_permissions(folder.path().join("file"), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read(folder.path().join("file")).unwrap(), b"cannot be read for capture");
    }

    #[test]
    fn rewind_restart_refuses_shared_linked_or_unrecognized_quarantine_contents() {
        use std::os::unix::fs::PermissionsExt;
        for kind in ["shared", "symlink", "file-symlink", "hardlink", "unknown-child"] {
            let folder = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("retained"), b"never inspect or remove").unwrap();
            let root = Root::open(folder.path()).unwrap();
            let temporary = ".faber-rewind-restart";
            mkdirat(&root.dir, temporary, Mode::from_raw_mode(0o700)).unwrap();
            let staging = private_staging(&root.dir, temporary).unwrap();
            let name = ".faber-rewind-cleanup-0-4";
            let quarantine = folder.path().join(temporary).join(name);
            if kind == "symlink" {
                symlink(outside.path(), &quarantine).unwrap();
            } else {
                mkdirat(&staging, name, Mode::from_raw_mode(0o700)).unwrap();
                match kind {
                    "shared" => std::fs::set_permissions(&quarantine, std::fs::Permissions::from_mode(0o755)).unwrap(),
                    "file-symlink" => symlink(outside.path().join("retained"), quarantine.join("retained")).unwrap(),
                    "hardlink" => std::fs::hard_link(outside.path().join("retained"), quarantine.join("retained")).unwrap(),
                    "unknown-child" => std::fs::write(quarantine.join("not-owned"), b"leave alone").unwrap(),
                    _ => unreachable!(),
                }
            }
            assert!(root.recover_cleanup("file", temporary).is_err(), "{kind}");
            assert!(root.finish_swap("file", temporary).is_err(), "{kind}");
            assert_eq!(std::fs::read(outside.path().join("retained")).unwrap(), b"never inspect or remove");
            assert!(quarantine.symlink_metadata().is_ok(), "{kind}");
        }
    }

    #[test]
    fn rewind_staging_rejects_a_substituted_shared_directory() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        mkdirat(&root.dir, ".faber-rewind-stage", Mode::from_raw_mode(0o700)).unwrap();
        let held = private_staging(&root.dir, ".faber-rewind-stage").unwrap();
        fchmod(&held, Mode::from_raw_mode(0o755)).unwrap();
        assert_eq!(private_staging(&root.dir, ".faber-rewind-stage").unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn rewind_no_replace_rename_moves_one_link_and_preserves_an_existing_destination() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("points")).unwrap();
        std::fs::create_dir(folder.path().join("staging")).unwrap();
        std::fs::write(folder.path().join("points/source"), b"owned metadata").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let original = root.open_file("points/source").unwrap();
        let original_id = file_identity(&original).unwrap();
        root.rename_new("points/source", "staging/retired").unwrap();
        assert!(!folder.path().join("points/source").exists());
        let retired = root.open_file("staging/retired").unwrap();
        assert_eq!(file_identity(&retired).unwrap(), original_id);
        assert_eq!(original.metadata().unwrap().nlink(), 1);
        assert_eq!(retired.metadata().unwrap().nlink(), 1);
        std::fs::write(folder.path().join("points/source"), b"new metadata").unwrap();
        assert_eq!(root.rename_new("points/source", "staging/retired").unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(folder.path().join("points/source")).unwrap(), b"new metadata");
        assert_eq!(std::fs::read(folder.path().join("staging/retired")).unwrap(), b"owned metadata");
    }

    #[test]
    fn rewind_cache_requires_an_authoritative_filesystem_generation() {
        // Capture admits named single-link files, not anonymous temporaries (nlink == 0 on macOS).
        let file = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(cache_fingerprint(file.as_file()).unwrap(), None);
    }
    use std::io::Read;
    use std::os::unix::fs::symlink;

    #[test]
    fn rewind_owned_removal_refuses_symlinks_and_a_name_replaced_before_deletion() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("orphan");
        std::fs::write(&path, b"owned original").unwrap();
        symlink("orphan", folder.path().join("alias")).unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert!(root.remove_owned("alias").is_err());
        assert!(folder.path().join("alias").symlink_metadata().unwrap().file_type().is_symlink());
        let result = root.remove_owned_with("orphan", || {
            std::fs::rename(&path, folder.path().join("moved"))?;
            std::fs::write(&path, b"racing replacement")
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
        assert_eq!(std::fs::read(&path).unwrap(), b"racing replacement");
        assert_eq!(std::fs::read(folder.path().join("moved")).unwrap(), b"owned original");
    }

    #[test]
    fn rewind_owned_removal_refuses_a_hardlink_added_after_validation() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("orphan");
        std::fs::write(&path, b"owned original").unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert!(root.remove_owned_with("orphan", || std::fs::hard_link(&path, folder.path().join("alias"))).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"owned original");
        assert_eq!(std::fs::read(folder.path().join("alias")).unwrap(), b"owned original");
    }

    #[test]
    fn rewind_preserved_cleanup_unlinks_inside_its_private_directory_after_validation() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("displaced");
        std::fs::write(&path, b"preserved").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let preserved = root.open_removable("displaced").unwrap();
        assert_eq!(
            root.remove_preserved_with("displaced", &preserved, || {
                assert!(!path.exists(), "the final unlink must no longer use the public name");
                std::fs::write(&path, b"final-window replacement")
            })
            .unwrap(),
            None
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"final-window replacement");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[test]
    fn rewind_failed_cleanup_returns_its_retained_name_without_replacing_a_new_save() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("displaced");
        std::fs::write(&path, b"preserved").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let preserved = root.open_removable("displaced").unwrap();
        let retained = root
            .remove_preserved_with("displaced", &preserved, || {
                std::fs::write(&path, b"new save")?;
                Err(io::ErrorKind::Interrupted.into())
            })
            .unwrap()
            .unwrap();
        assert_ne!(retained, "displaced");
        assert_eq!(std::fs::read(folder.path().join(&retained)).unwrap(), b"preserved");
        assert_eq!(std::fs::read(&path).unwrap(), b"new save");
        assert_eq!(std::fs::metadata(folder.path().join(retained).parent().unwrap()).unwrap().mode() & 0o777, 0o700);
    }

    #[test]
    fn rewind_returning_an_unreadable_displaced_file_never_requires_its_contents() {
        use std::os::unix::fs::PermissionsExt;
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("displaced");
        std::fs::write(&path, b"cannot be captured").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o0)).unwrap();
        let root = Root::open(folder.path()).unwrap();
        root.rename_new("displaced", "original").unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::metadata(folder.path().join("original")).unwrap().mode() & 0o777, 0);
        std::fs::set_permissions(folder.path().join("original"), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::read(folder.path().join("original")).unwrap(), b"cannot be captured");
    }

    #[test]
    fn rewind_swapped_temporary_is_never_published_or_cleaned_as_owned() {
        for existing in [false, true] {
            for link in [false, true] {
                let folder = tempfile::tempdir().unwrap();
                let target = folder.path().join("file");
                std::fs::write(folder.path().join("source"), b"saved").unwrap();
                let root = Root::open(folder.path()).unwrap();
                let saved = protection(&root.open_file("source").unwrap()).unwrap();
                let before = if existing {
                    std::fs::write(&target, b"before").unwrap();
                    Some(fingerprint(&root.open_file("file").unwrap()).unwrap())
                } else {
                    None
                };
                let temp = folder.path().join(super::super::swap_paths("file", ".faber-rewind-swap-0").0);
                let moved = folder.path().join("moved-temp");
                let expected = before.as_ref().map(|id| (id.as_slice(), b"before".as_slice()));
                assert!(
                    root.mutate_journaled("file", expected, Some((b"saved", &saved)), ".faber-rewind-swap-0", |_| {
                        std::fs::rename(&temp, &moved)?;
                        if link { symlink("source", &temp) } else { std::fs::write(&temp, b"unexpected") }
                    })
                    .is_err()
                );
                if existing {
                    assert_eq!(std::fs::read(&target).unwrap(), b"before");
                } else {
                    assert!(!target.exists());
                }
                assert_eq!(std::fs::read(&moved).unwrap(), b"saved");
                if link {
                    assert_eq!(std::fs::read_link(&temp).unwrap(), Path::new("source"));
                } else {
                    assert_eq!(std::fs::read(&temp).unwrap(), b"unexpected");
                }
            }
        }
    }

    #[test]
    fn rewind_changes_at_the_journal_barrier_keep_the_current_bytes_and_identity() {
        for replace_identity in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("file");
            std::fs::write(&path, b"before").unwrap();
            let root = Root::open(folder.path()).unwrap();
            let file = root.open_file("file").unwrap();
            let before = fingerprint(&file).unwrap();
            let saved = protection(&file).unwrap();
            drop(file);
            assert!(
                root.mutate_journaled("file", Some((&before, b"before")), Some((b"restored", &saved)), ".faber-rewind-race-0", |_| {
                    if replace_identity {
                        std::fs::rename(&path, folder.path().join("previous"))?;
                    }
                    std::fs::write(&path, b"concurrent change")
                })
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"concurrent change");
            assert!(!folder.path().join(".faber-rewind-race-0").exists());
        }
    }

    #[test]
    fn changes_parent_swap_reads_the_pinned_directory_and_never_the_outside_target() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("notes")).unwrap();
        std::fs::write(root.path().join("notes/report.txt"), "inside").unwrap();
        std::fs::write(outside.path().join("report.txt"), "private outside").unwrap();
        let disk = Root::open(root.path()).unwrap();
        let mut file = disk
            .open_with("notes/report.txt", || {
                std::fs::rename(root.path().join("notes"), root.path().join("moved")).unwrap();
                symlink(outside.path(), root.path().join("notes")).unwrap();
            })
            .unwrap();
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        assert_eq!(text, "inside");
        assert!(disk.open_file("notes/report.txt").is_err());
    }

    #[test]
    fn rewind_leaf_fifo_swap_finishes_without_reading() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("leaf"), b"inside").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let started = std::time::Instant::now();
        assert!(
            root.open_with("leaf", || {
                std::fs::remove_file(folder.path().join("leaf")).unwrap();
                #[cfg(not(target_vendor = "apple"))]
                rustix::fs::mkfifoat(&root.dir, "leaf", Mode::from_raw_mode(0o600)).unwrap();
                #[cfg(target_vendor = "apple")]
                {
                    use std::os::unix::ffi::OsStrExt;
                    unsafe extern "C" {
                        fn mkfifo(path: *const std::ffi::c_char, mode: u16) -> i32;
                    }
                    let path = std::ffi::CString::new(folder.path().join("leaf").as_os_str().as_bytes()).unwrap();
                    // SAFETY: the test's NUL-terminated pathname outlives the call; Darwin mode_t is u16.
                    assert_eq!(unsafe { mkfifo(path.as_ptr(), 0o600) }, 0);
                }
            })
            .is_err()
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
