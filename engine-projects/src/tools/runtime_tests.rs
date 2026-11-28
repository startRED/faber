use super::*;
use std::io::Write;

#[test]
fn runtime_interpreter_snapshot_retry_repairs_incomplete_or_wrong_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("tools/runtimes/local-python");
    let directory = root.join(if cfg!(windows) { ".venv/Scripts" } else { ".venv/bin" });
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(root.join(".venv/pyvenv.cfg"), "installed").unwrap();
    std::fs::write(directory.join(if cfg!(windows) { "python.exe" } else { "python" }), "interpreter").unwrap();
    let item = fixture("local-python", b"source", None);
    record_interpreter(&root, &item).unwrap();
    let digest = std::fs::read_to_string(root.join("interpreter.json")).unwrap();
    let registry = interpreter_registry(&root).unwrap();
    let snapshot = registry.join(&digest);
    let original = std::fs::read(&snapshot).unwrap();
    for broken in [Vec::new(), b"partial".to_vec(), vec![b'x'; original.len()]] {
        std::fs::write(&snapshot, broken).unwrap();
        assert_eq!(interpreter_intact(&root, &item), Integrity::Tampered);
        record_interpreter(&root, &item).unwrap();
        assert_eq!(std::fs::read(&snapshot).unwrap(), original);
        assert_eq!(interpreter_intact(&root, &item), Integrity::Intact);
    }
    record_interpreter(&root, &item).unwrap();
    assert_eq!(std::fs::read(&snapshot).unwrap(), original);
    assert_eq!(std::fs::read_dir(registry).unwrap().count(), 1, "temporary files must be cleaned up");
}

#[test]
fn runtime_prune_preserves_a_healthy_install_when_hashing_fails() {
    let temp = tempfile::tempdir().unwrap();
    let item = fixture("io-audio", b"audio fixture", None);
    let source = temp.path().join("download");
    std::fs::write(&source, b"audio fixture").unwrap();
    let store = Store::with_catalog(temp.path(), vec![item.clone()]);
    store.install_from(&item, &source).unwrap();
    store.wait_prepared_for_test();
    let directory = store.dir(&item);
    for file in ["source", "payload/audio.dat"] {
        TEST_HASH_ERROR.with(|target| *target.borrow_mut() = Some(directory.join(file)));
        let state = verify_state(&directory, &item);
        let checked = store.prune().unwrap();
        TEST_HASH_ERROR.with(|target| target.borrow_mut().take());
        assert_eq!(state, Integrity::CannotCheck, "{file}");
        assert!(!checked.contains(&item.id));
        assert_eq!(std::fs::read(directory.join(file)).unwrap(), b"audio fixture");
        assert_eq!(store.installed().get(&item.id), Some(&item.commit));
        assert_eq!(verify_state(&directory, &item), Integrity::Intact);
    }
}

#[test]
fn runtime_integrity_only_discards_missing_unexpected_or_mismatched_contents() {
    for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock, io::ErrorKind::Other] {
        assert_eq!(content_integrity(Err(kind.into())), Integrity::CannotCheck, "{kind:?}");
    }
    for kind in [io::ErrorKind::NotFound, io::ErrorKind::NotADirectory, io::ErrorKind::InvalidData] {
        assert_eq!(content_integrity(Err(kind.into())), Integrity::Tampered, "{kind:?}");
    }
    let temp = tempfile::tempdir().unwrap();
    let item = fixture("changed-audio", b"audio fixture", None);
    let source = temp.path().join("download");
    let root = temp.path().join("runtime");
    std::fs::write(&source, b"audio fixture").unwrap();
    extract(&item, &source, &BTreeMap::new(), &root).unwrap();
    for name in ["source", "payload/audio.dat", "instructions/SKILL.md"] {
        let path = root.join(name);
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, vec![b'x'; original.len()]).unwrap();
        assert_eq!(verify_state(&root, &item), Integrity::Tampered, "{name}");
        std::fs::remove_file(&path).unwrap();
        assert_eq!(verify_state(&root, &item), Integrity::Tampered, "{name}");
        std::fs::create_dir(&path).unwrap();
        assert_eq!(verify_state(&root, &item), Integrity::Tampered, "{name}");
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, original).unwrap();
        assert_eq!(verify_state(&root, &item), Integrity::Intact);
    }
}

#[cfg(windows)]
#[test]
fn runtime_archive_and_package_reads_preserve_sharing_errors() {
    use std::os::windows::fs::OpenOptionsExt;
    let temp = tempfile::tempdir().unwrap();
    let zip = temp.path().join("archive.zip");
    std::fs::write(&zip, zip_fixture(&["module.py"], false)).unwrap();
    let payload = temp.path().join("payload");
    unpack(&zip, &payload).unwrap();
    let hash = crate::hex(&Sha256::digest(b"fixture"));
    let expected = BTreeMap::from([("module.py", hash.as_str())]);
    let tar = temp.path().join("archive.tar.gz");
    std::fs::write(&tar, tar_fixture(&[("module.py", tar::EntryType::Regular, 7)])).unwrap();
    let library = temp.path().join("library");
    unpack_tar(&tar, &library).unwrap();
    for path in [&zip, &tar, &payload.join("module.py"), &library.join("module.py")] {
        let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(path).unwrap();
        let result = if path == &zip || path.starts_with(&payload) { unpacked_intact(&zip, &payload) } else { tar_intact(&tar, &library) };
        assert_eq!(content_integrity(result), Integrity::CannotCheck, "{}", path.display());
        if path.starts_with(&payload) {
            assert_eq!(content_integrity(package_files_intact(&payload, &expected)), Integrity::CannotCheck);
        }
        drop(lock);
    }
    assert!(unpacked_intact(&zip, &payload).unwrap());
    assert!(tar_intact(&tar, &library).unwrap());
    assert!(package_files_intact(&payload, &expected).unwrap());
}

fn zip_fixture(names: &[&str], symlink: bool) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for name in names {
        if symlink {
            zip.add_symlink(*name, "../outside", options).unwrap();
        } else {
            zip.start_file(*name, options).unwrap();
            zip.write_all(b"fixture").unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}

fn tar_fixture(entries: &[(&str, tar::EntryType, u64)]) -> Vec<u8> {
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    for (name, kind, size) in entries {
        let mut header = tar::Header::new_gnu();
        // Raw names intentionally bypass the builder's own traversal rejection.
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        header.set_entry_type(*kind);
        header.set_mode(0o644);
        header.set_size(*size);
        if kind.is_symlink() || kind.is_hard_link() {
            header.set_link_name("../outside").unwrap();
        }
        header.set_cksum();
        gzip.write_all(header.as_bytes()).unwrap();
        // Oversized entries need only their declared length: extraction rejects before reading.
        if *size <= 512 {
            gzip.write_all(&vec![b'x'; *size as usize]).unwrap();
            let padding = (512 - size % 512) % 512;
            gzip.write_all(&vec![0; padding as usize]).unwrap();
        }
    }
    gzip.write_all(&[0; 1024]).unwrap();
    gzip.finish().unwrap()
}

#[test]
fn runtime_zip_rejects_hostile_paths_duplicates_links_and_declared_oversize() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("source.zip");
    let absolute = temp.path().join("absolute").to_string_lossy().replace('\\', "/");
    for names in [
        vec!["../outside"],
        vec![absolute.as_str()],
        vec!["nested/../../outside"],
        vec!["dir\\..\\outside"],
        vec!["module.py", "MODULE.py"],
    ] {
        std::fs::write(&archive, zip_fixture(&names, false)).unwrap();
        let dest = temp.path().join(uuid::Uuid::new_v4().to_string());
        assert_eq!(unpack(&archive, &dest).unwrap_err(), "error.tools.digest", "{names:?}");
    }
    std::fs::write(&archive, zip_fixture(&["linked.py"], true)).unwrap();
    assert_eq!(unpack(&archive, &temp.path().join("links")).unwrap_err(), "error.tools.digest");
    let mut oversized = zip_fixture(&["oversized.bin"], false);
    let central = oversized.windows(4).position(|bytes| bytes == b"PK\x01\x02").unwrap();
    let size = u32::try_from(FILE_LIMIT + 1).unwrap().to_le_bytes();
    oversized[central + 24..central + 28].copy_from_slice(&size);
    oversized[22..26].copy_from_slice(&size);
    let mut parsed = zip::ZipArchive::new(io::Cursor::new(&oversized)).unwrap();
    assert_eq!(parsed.by_index(0).unwrap().size(), FILE_LIMIT + 1);
    std::fs::write(&archive, oversized).unwrap();
    assert_eq!(unpack(&archive, &temp.path().join("oversized")).unwrap_err(), "error.tools.size");
    assert!(!temp.path().join("outside").exists());
}

#[test]
fn runtime_tar_rejects_hostile_paths_duplicates_links_and_declared_oversize() {
    let temp = tempfile::tempdir().unwrap();
    let archive = temp.path().join("source.tar.gz");
    let absolute = temp.path().join("absolute").to_string_lossy().replace('\\', "/");
    let regular = tar::EntryType::Regular;
    for entries in [
        vec![("../outside", regular, 1)],
        vec![(absolute.as_str(), regular, 1)],
        vec![("nested/../../outside", regular, 1)],
        vec![("module.py", regular, 1), ("MODULE.py", regular, 1)],
        vec![("duplicate", regular, 1), ("duplicate", regular, 1)],
        vec![("linked.py", tar::EntryType::Symlink, 0)],
        vec![("hardlinked.py", tar::EntryType::Link, 0)],
    ] {
        std::fs::write(&archive, tar_fixture(&entries)).unwrap();
        let dest = temp.path().join(uuid::Uuid::new_v4().to_string());
        assert_eq!(unpack_tar(&archive, &dest).unwrap_err(), "error.tools.digest", "{entries:?}");
    }
    std::fs::write(&archive, tar_fixture(&[("oversized.bin", regular, FILE_LIMIT + 1)])).unwrap();
    assert_eq!(unpack_tar(&archive, &temp.path().join("oversized")).unwrap_err(), "error.tools.size");
    assert!(!temp.path().join("outside").exists());
}

#[test]
fn runtime_every_pinned_wheel_path_is_portable() {
    for hash in wheel_index().keys() {
        assert!(pin(hash));
        let files = wheel_files(hash).unwrap();
        assert!(!files.is_empty(), "{hash}");
        for (path, digest) in files {
            assert!(relative(path), "nonportable pinned path: {path}");
            assert!(pin(digest), "invalid pinned digest: {path}");
        }
    }
}

#[test]
fn runtime_prune_preserves_inconclusive_python_without_calling_uv() {
    let temp = tempfile::tempdir().unwrap();
    let bytes = b"audio fixture";
    let mut item = fixture("legacy-python", bytes, None);
    let source = temp.path().join("download");
    std::fs::write(&source, bytes).unwrap();
    let store = Store::with_catalog(temp.path(), vec![item.clone()]);
    store.install_from(&item, &source).unwrap();
    store.wait_prepared_for_test();
    let directory = store.dir(&item);
    drop(store);
    item.runtime.as_mut().unwrap().kind = Kind::Python;
    let store = Store::with_catalog(temp.path(), vec![item.clone()]);
    TEST_UV.with(|test| *test.borrow_mut() = Some(Box::new(|_| panic!("prune must not regenerate a venv"))));
    assert_eq!(verify_state(&directory, &item), Integrity::CannotCheck);
    let checked = store.prune().unwrap();
    assert!(!checked.contains(&item.id), "an inconclusive runtime must never reach a session");
    assert!(directory.join("source").is_file(), "an inconclusive check must preserve installed bytes");
    assert_eq!(store.installed().get(&item.id), Some(&item.commit));
    TEST_UV.with(|test| test.borrow_mut().take());
}

#[test]
fn runtime_unlisted_root_weights_and_import_files_fail_integrity() {
    for extra in ["unlisted.txt", "weights/unlisted.bin", "library/antlr4/antlr4-python3-runtime-evil/src/faster_whisper/__init__.py"] {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"audio fixture";
        let item = fixture("exact-audio", bytes, None);
        let source = temp.path().join("download");
        std::fs::write(&source, bytes).unwrap();
        let store = Store::with_catalog(temp.path(), vec![item.clone()]);
        store.install_from(&item, &source).unwrap();
        store.wait_prepared_for_test();
        let directory = store.dir(&item);
        assert!(verify(&directory, &item));
        let injected = directory.join(extra);
        std::fs::create_dir_all(injected.parent().unwrap()).unwrap();
        std::fs::write(injected, b"unlisted content").unwrap();
        assert_eq!(verify_state(&directory, &item), Integrity::Tampered, "{extra}");
    }
}

#[test]
fn runtime_enabling_an_inconclusive_disabled_copy_preserves_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut item = fixture("legacy-disabled-python", b"audio fixture", None);
    let source = temp.path().join("download");
    std::fs::write(&source, b"audio fixture").unwrap();
    let store = Store::with_catalog(temp.path(), vec![item.clone()]);
    store.install_from(&item, &source).unwrap();
    store.wait_prepared_for_test();
    store.set_enabled(&item.id, false).unwrap();
    store.wait_prepared_for_test();
    drop(store);
    item.runtime.as_mut().unwrap().kind = Kind::Python;
    let store = Store::with_catalog(temp.path(), vec![item.clone()]);
    assert!(store.set_enabled(&item.id, true).is_err());
    assert!(store.disabled().join(&item.id).join("source").is_file());
    assert_eq!(store.installed().get(&item.id), Some(&item.commit));
}

#[cfg(unix)]
#[test]
fn runtime_system_python_updates_are_inconclusive_and_targets_are_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("tools/runtimes/python");
    std::fs::create_dir_all(root.join(".venv/bin")).unwrap();
    let native = temp.path().join("python3.12");
    std::fs::write(&native, "original").unwrap();
    std::os::unix::fs::symlink(&native, root.join(".venv/bin/python")).unwrap();
    std::fs::write(root.join(".venv/pyvenv.cfg"), "installed").unwrap();
    let item = fixture("local-python", b"source", None);
    record_interpreter(&root, &item).unwrap();
    assert_eq!(interpreter_intact(&root, &item), Integrity::Intact);
    std::fs::write(&native, "upgraded").unwrap();
    assert_eq!(interpreter_intact(&root, &item), Integrity::CannotCheck);
    std::fs::File::options().write(true).open(&native).unwrap().set_len(40 * 1024 * 1024 + 1).unwrap();
    assert!(interpreter_pin(&root.join(".venv/bin/python")).is_err());
}
