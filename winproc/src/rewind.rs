//! Handle-based disk operations for the private rewind store and project files.
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::{Component, Path};

/// Ordinary identity/change fingerprint, optional authoritative cache generation, and byte length.
pub type CaptureMetadata = (Vec<u8>, Option<Vec<u8>>, u64);

/// Durable barriers: callers record intent before a rename and preserve the displaced handle before cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapStage {
    Prepared,
    BeforeDisplace,
    Displaced,
    BeforeInstall,
}

/// Both Unix names live inside an exclusive owner-only directory held for the entire swap.
pub fn swap_paths(path: &str, temporary: &str) -> (String, String) {
    let parent = Path::new(path).parent().unwrap_or(Path::new(""));
    #[cfg(unix)]
    let (new, old) = (parent.join(temporary).join("new"), parent.join(temporary).join("old"));
    #[cfg(windows)]
    let (new, old) = (parent.join(temporary), parent.join(format!("{temporary}-aside")));
    (new.to_string_lossy().replace('\\', "/"), old.to_string_lossy().replace('\\', "/"))
}

fn compare_displaced(file: &File, original: &File, bytes: &[u8]) -> io::Result<()> {
    if file_identity(file)? != file_identity(original)? {
        return Err(changed());
    }
    // Renaming can change ctime, so compare identity separately, then validate the held bytes again.
    compare(file, (&fingerprint(file)?, bytes))
}

#[cfg(unix)]
#[path = "rewind/unix.rs"]
pub(crate) mod platform;
#[cfg(windows)]
#[path = "rewind/windows.rs"]
pub(crate) mod platform;
pub use platform::{
    Root, allocated, cache_fingerprint, capture_metadata, file_identity, fingerprint, private_create, private_dir, private_write,
    protection, resolved_path,
};

fn relative(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty() || path.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    #[cfg(windows)]
    if path.components().any(|part| part.as_os_str().to_string_lossy().contains(':')) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

fn temporary_name(name: &str) -> io::Result<()> {
    relative(Path::new(name))?;
    if !name.starts_with(".faber-rewind-") || name.len() <= ".faber-rewind-".len() || name.contains(['/', '\\', ':']) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(())
}

fn changed() -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, "rewind target changed")
}

fn compare(file: &File, expected: (&[u8], &[u8])) -> io::Result<()> {
    if fingerprint(file)? != expected.0 {
        return Err(changed());
    }
    let mut buffer = vec![0; 1024 * 1024];
    let mut file = file;
    file.rewind()?;
    let mut offset = 0;
    loop {
        if fingerprint(file)? != expected.0 {
            return Err(changed());
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        if expected.1.get(offset..offset + count) != Some(&buffer[..count]) {
            return Err(changed());
        }
        offset += count;
    }
    if offset != expected.1.len() || fingerprint(file)? != expected.0 { Err(changed()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn rewind_private_create_reads_back_through_the_same_private_handle() {
        use std::io::Write;
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut file = root.private_create("private").unwrap();
        file.write_all(b"staged pack bytes").unwrap();
        file.rewind().unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"staged pack bytes");
    }

    #[test]
    fn rewind_owned_removal_deletes_only_the_validated_file_without_reading_its_bytes() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        std::fs::create_dir(folder.path().join("objects")).unwrap();
        std::fs::write(folder.path().join("objects/orphan"), b"orphan bytes").unwrap();
        std::fs::write(folder.path().join("keep"), b"not selected").unwrap();
        root.remove_owned("objects/orphan").unwrap();
        assert!(!folder.path().join("objects/orphan").exists());
        assert_eq!(std::fs::read(folder.path().join("keep")).unwrap(), b"not selected");
        assert_eq!(root.remove_owned("objects/orphan").unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn rewind_preserved_cleanup_refuses_a_replacement_installed_after_preservation() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("displaced");
        std::fs::write(&path, b"preserved version B").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut preserved = root.open_removable("displaced").unwrap();
        let mut saved = vec![];
        preserved.read_to_end(&mut saved).unwrap();
        assert_eq!(saved, b"preserved version B");
        std::fs::rename(&path, folder.path().join("moved")).unwrap();
        std::fs::write(&path, b"unpreserved version C").unwrap();
        let kept = root.remove_preserved("displaced", &preserved).unwrap().unwrap();
        assert_eq!(std::fs::read(folder.path().join(kept)).unwrap(), b"unpreserved version C");
        assert_eq!(std::fs::read(folder.path().join("moved")).unwrap(), saved);
    }

    #[test]
    fn rewind_preserved_cleanup_can_delete_while_the_preservation_handle_is_still_open() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("displaced"), b"already saved").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let preserved = root.open_removable("displaced").unwrap();
        assert_eq!(root.remove_preserved("displaced", &preserved).unwrap(), None);
        drop(preserved);
        assert!(!folder.path().join("displaced").exists());
    }

    #[test]
    fn rewind_owned_removal_refuses_hardlinks_directories_and_traversal() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("original"), b"preserve both names").unwrap();
        std::fs::hard_link(folder.path().join("original"), folder.path().join("alias")).unwrap();
        std::fs::create_dir(folder.path().join("directory")).unwrap();
        let root = Root::open(folder.path()).unwrap();
        for path in ["alias", "original", "directory", "../outside"] {
            assert!(root.remove_owned(path).is_err(), "{path}");
        }
        assert_eq!(std::fs::read(folder.path().join("alias")).unwrap(), b"preserve both names");
        assert!(folder.path().join("original").exists());
        assert!(folder.path().join("directory").is_dir());
    }

    #[test]
    #[ignore = "manual native cleanup cost probe; ordinary files, no Git or CLI processes"]
    fn rewind_owned_removal_cost() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut names = Vec::new();
        for n in 0..1000 {
            let name = format!("owned/objects/{:02x}/{n:062x}", n % 256);
            let path = folder.path().join(&name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, [0u8; 16 * 1024]).unwrap();
            names.push(name);
        }
        #[cfg(windows)]
        let stats = || crate::processes().unwrap().into_iter().find(|p| p.pid == std::process::id()).unwrap();
        #[cfg(windows)]
        let before = stats();
        let began = std::time::Instant::now();
        for name in &names {
            root.remove_owned(name).unwrap();
        }
        let elapsed = began.elapsed();
        println!(
            "rewind_native_cleanup files=1000 logical_deleted_bytes=16384000 content_read_bytes=0 wall_ms={:.3}",
            elapsed.as_secs_f64() * 1000.0
        );
        #[cfg(windows)]
        {
            let after = stats();
            println!(
                "rewind_native_cleanup cpu_ms={:.3} private_before_bytes={} private_after_bytes={}",
                (after.cpu - before.cpu) as f64 / 10_000.0,
                before.private_bytes,
                after.private_bytes
            );
        }
        assert!(names.iter().all(|name| !folder.path().join(name).exists()));
    }

    #[test]
    fn rewind_capture_metadata_matches_held_identity_and_length() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("leaf"), b"contents").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("leaf").unwrap();
        let (identity, cache, len) = capture_metadata(&file).unwrap();
        assert_eq!(identity, fingerprint(&file).unwrap());
        assert_eq!(cache, cache_fingerprint(&file).unwrap());
        assert_eq!(len, 8);
    }

    #[test]
    fn rewind_root_reports_held_path_and_available_space() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert_eq!(root.path(), folder.path().canonicalize().unwrap());
        assert!(root.available_space().unwrap() > 0);
    }

    fn assert_protection_matches(file: &File, saved: &[u8]) {
        let actual = protection(file).unwrap();
        #[cfg(windows)]
        assert!(platform::same_protection(&actual, saved));
        #[cfg(unix)]
        assert_eq!(actual, saved);
    }

    #[test]
    fn rewind_rejects_hardlinks_before_reading_and_traversal() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("private");
        std::fs::write(&path, b"secret").unwrap();
        std::fs::hard_link(&path, folder.path().join("alias")).unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert!(root.open_file("alias").is_err());
        assert!(root.open_file("../private").is_err());
    }

    #[test]
    fn rewind_recreates_replaces_and_removes_with_protection() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("history")).unwrap();
        std::fs::write(folder.path().join("history/saved"), b"before").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("history/saved").unwrap();
        let security = protection(&file).unwrap();
        drop(file);
        root.mutate("restored", None, Some((b"before", &security))).unwrap();
        let file = root.open_file("restored").unwrap();
        let identity = fingerprint(&file).unwrap();
        assert_protection_matches(&file, &security);
        drop(file);
        root.mutate("restored", Some((&identity, b"before")), Some((b"after", &security))).unwrap();
        let mut file = root.open_file("restored").unwrap();
        let identity = fingerprint(&file).unwrap();
        let mut bytes = vec![];
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"after");
        drop(file);
        root.mutate("restored", Some((&identity, b"after")), None).unwrap();
        assert!(!folder.path().join("restored").exists());
    }

    #[test]
    fn rewind_changed_bytes_identity_and_absence_keep_the_current_file() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("file"), b"before").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("file").unwrap();
        let security = protection(&file).unwrap();
        let identity = fingerprint(&file).unwrap();
        drop(file);
        std::fs::write(folder.path().join("file"), b"someone else").unwrap();
        assert!(root.mutate("file", Some((&identity, b"before")), Some((b"saved", &security))).is_err());
        assert!(root.mutate("file", None, Some((b"saved", &security))).is_err());
        assert_eq!(std::fs::read(folder.path().join("file")).unwrap(), b"someone else");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[test]
    fn rewind_temporary_collision_never_removes_an_unowned_file() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("file"), b"before").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("file").unwrap();
        let security = protection(&file).unwrap();
        let identity = fingerprint(&file).unwrap();
        drop(file);
        for i in 0..64 {
            std::fs::write(folder.path().join(format!(".faber-rewind-{}-{i}", std::process::id())), b"owned by someone else").unwrap();
        }
        assert!(root.mutate("file", Some((&identity, b"before")), Some((b"after", &security))).is_err());
        for i in 0..64 {
            assert_eq!(
                std::fs::read(folder.path().join(format!(".faber-rewind-{}-{i}", std::process::id()))).unwrap(),
                b"owned by someone else"
            );
        }
    }

    #[test]
    fn rewind_private_file_in_a_shared_folder_is_recreated_private() {
        use std::io::Write;
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("private");
        let mut file = private_create(&path).unwrap();
        let saved = protection(&file).unwrap();
        file.write_all(b"private bytes").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let root = Root::open(folder.path()).unwrap();
        root.mutate("recreated", None, Some((b"private bytes", &saved))).unwrap();
        let recreated = root.open_file("recreated").unwrap();
        assert_protection_matches(&recreated, &saved);
    }

    #[test]
    fn rewind_missing_parent_recreation() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("source"), b"saved").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let protection = protection(&root.open_file("source").unwrap()).unwrap();
        root.mutate("missing/nested/file", None, Some((b"saved", &protection))).unwrap();
        assert_eq!(std::fs::read(folder.path().join("missing/nested/file")).unwrap(), b"saved");
    }

    #[test]
    fn rewind_protection_failure_cleans_the_private_temporary() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert!(root.mutate("new", None, Some((b"saved", &[]))).is_err());
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[test]
    fn rewind_journal_callback_sees_an_empty_private_file_with_stable_identity() {
        let folder = tempfile::tempdir().unwrap();
        let sample = private_create(&folder.path().join("sample")).unwrap();
        let security = protection(&sample).unwrap();
        drop(sample);
        std::fs::remove_file(folder.path().join("sample")).unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut saved_id = None;
        root.mutate_journaled("new", None, Some((b"saved", &security)), ".faber-rewind-journal-0", |file| {
            assert_eq!(file.metadata()?.len(), 0);
            assert_protection_matches(file, &security);
            saved_id = Some(file_identity(file)?);
            Ok(())
        })
        .unwrap();
        assert_eq!(file_identity(&root.open_file("new").unwrap()).unwrap(), saved_id.unwrap());
        assert_eq!(std::fs::read(folder.path().join("new")).unwrap(), b"saved");
        assert!(!folder.path().join(".faber-rewind-journal-0").exists());
    }

    #[test]
    fn rewind_journal_failure_cleans_only_the_owned_temporary() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        let called = std::cell::Cell::new(false);
        let result = root.mutate_journaled("new", None, Some((b"saved", &[])), ".faber-rewind-journal-0", |file| {
            called.set(true);
            assert_eq!(file.metadata()?.len(), 0);
            Err(io::ErrorKind::StorageFull.into())
        });
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::StorageFull);
        assert!(called.get());
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
        std::fs::write(folder.path().join(".faber-rewind-journal-0"), b"not owned").unwrap();
        assert!(
            root.mutate_journaled("new", None, Some((b"saved", &[])), ".faber-rewind-journal-0", |_| panic!("collision callback")).is_err()
        );
        assert_eq!(std::fs::read(folder.path().join(".faber-rewind-journal-0")).unwrap(), b"not owned");
    }

    #[test]
    fn rewind_journal_rejects_unsafe_temporary_names_and_racing_creation() {
        let folder = tempfile::tempdir().unwrap();
        let root = Root::open(folder.path()).unwrap();
        for name in [
            "other",
            ".faber-rewind-",
            "../.faber-rewind-escape",
            ".faber-rewind-nested/file",
            ".faber-rewind-nested\\file",
            ".faber-rewind-stream:name",
        ] {
            assert_eq!(
                root.mutate_journaled("new", None, Some((b"saved", &[])), name, |_| panic!("invalid name callback")).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        let source = private_create(&folder.path().join("source")).unwrap();
        let security = protection(&source).unwrap();
        drop(source);
        assert!(
            root.mutate_journaled("new", None, Some((b"saved", &security)), ".faber-rewind-journal-0", |_| {
                std::fs::write(folder.path().join("new"), b"racing creation")
            })
            .is_err()
        );
        assert_eq!(std::fs::read(folder.path().join("new")).unwrap(), b"racing creation");
        assert!(!folder.path().join(".faber-rewind-journal-0").exists());
    }

    #[test]
    fn rewind_owned_rename_refuses_replacement() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("source"), b"saved").unwrap();
        std::fs::write(folder.path().join("existing"), b"mine").unwrap();
        let root = Root::open(folder.path()).unwrap();
        assert!(root.rename_new("source", "existing").is_err());
        assert_eq!(std::fs::read(folder.path().join("existing")).unwrap(), b"mine");
        root.rename_new("source", "new").unwrap();
        assert_eq!(std::fs::read(folder.path().join("new")).unwrap(), b"saved");
        assert!(!folder.path().join("source").exists());
    }

    #[test]
    fn rewind_private_metadata_keeps_its_canonical_name_until_atomic_commit() {
        use std::io::Write;
        let folder = tempfile::tempdir().unwrap();
        // Metadata replacement requires an owner-only parent; TempDir follows the runner's umask.
        #[cfg(unix)]
        private_dir(folder.path()).unwrap();
        let root = Root::open(folder.path()).unwrap();
        let mut file = root.private_create("journal.json").unwrap();
        file.write_all(b"old complete journal").unwrap();
        file.sync_all().unwrap();
        drop(file);
        let file = root.open_file("journal.json").unwrap();
        let protection = protection(&file).unwrap();
        let identity = fingerprint(&file).unwrap();
        drop(file);
        let failed =
            root.replace_owned_with("journal.json", (&identity, b"old complete journal"), (b"new complete journal", &protection), || {
                assert_eq!(std::fs::read(folder.path().join("journal.json"))?, b"old complete journal");
                Err(io::ErrorKind::Interrupted.into())
            });
        assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert_eq!(std::fs::read(folder.path().join("journal.json")).unwrap(), b"old complete journal");
        root.replace_owned("journal.json", (&identity, b"old complete journal"), (b"new complete journal", &protection)).unwrap();
        assert_eq!(std::fs::read(folder.path().join("journal.json")).unwrap(), b"new complete journal");
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[test]
    fn rewind_final_validation_race_preserves_every_displaced_version() {
        for replacement in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("file");
            std::fs::write(&path, b"version A").unwrap();
            let root = Root::open(folder.path()).unwrap();
            let file = root.open_file("file").unwrap();
            let security = protection(&file).unwrap();
            let expected = fingerprint(&file).unwrap();
            drop(file);
            let mut displaced = Vec::new();
            let result = root.mutate_staged(
                "file",
                Some((&expected, b"version A")),
                replacement.then_some((b"restored".as_slice(), security.as_slice())),
                ".faber-rewind-final-race",
                |stage, file| {
                    if stage == SwapStage::BeforeDisplace {
                        std::fs::rename(&path, folder.path().join("external-A"))?;
                        std::fs::write(&path, b"version B")?;
                    }
                    if stage == SwapStage::Displaced {
                        let mut held = file;
                        held.rewind()?;
                        held.read_to_end(&mut displaced)?;
                    }
                    Ok(())
                },
            );
            assert!(result.is_err());
            #[cfg(unix)]
            {
                assert_eq!(std::fs::read(folder.path().join("external-A")).unwrap(), b"version A");
                assert_eq!(displaced, b"version B");
                assert_eq!(std::fs::read(folder.path().join(swap_paths("file", ".faber-rewind-final-race").1)).unwrap(), b"version B");
            }
            #[cfg(windows)]
            {
                // Windows moves the held A, so the writer's B keeps its original name.
                assert_eq!(displaced, b"version A");
                assert_eq!(std::fs::read(folder.path().join(".faber-rewind-final-race-aside")).unwrap(), b"version A");
                assert_eq!(std::fs::read(&path).unwrap(), b"version B");
            }
        }
    }

    #[test]
    fn rewind_install_race_never_replaces_a_new_destination_and_keeps_recovery_files() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("file");
        std::fs::write(&path, b"version A").unwrap();
        let root = Root::open(folder.path()).unwrap();
        let file = root.open_file("file").unwrap();
        let security = protection(&file).unwrap();
        let expected = fingerprint(&file).unwrap();
        drop(file);
        let result = root.mutate_staged(
            "file",
            Some((&expected, b"version A")),
            Some((b"restored", &security)),
            ".faber-rewind-install-race",
            |stage, _| {
                if stage == SwapStage::BeforeInstall {
                    std::fs::write(&path, b"version B")?;
                }
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"version B");
        assert_eq!(std::fs::read(folder.path().join(swap_paths("file", ".faber-rewind-install-race").1)).unwrap(), b"version A");
        assert_eq!(std::fs::read(folder.path().join(swap_paths("file", ".faber-rewind-install-race").0)).unwrap(), b"restored");
    }
}
