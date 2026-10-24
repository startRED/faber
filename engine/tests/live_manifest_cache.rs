//! Interrupted publication must never erase an accepted suspension or revision floor.

use crate::support;
use faber_engine::live_manifest_cache::{Error, load, publish};
use faber_engine::store::{Disk, RealDisk};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

const NOW: &str = "2026-10-24T12:00:00Z";
const APP: &str = "0.2.0";

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/live-manifest-cache.json")).unwrap()
}

fn pins() -> BTreeMap<String, [u8; 32]> {
    let fixture = fixture();
    let hex = fixture["public_key_hex"].as_str().unwrap();
    let key: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    BTreeMap::from([("fixture".into(), key.try_into().unwrap())])
}

fn envelope(name: &str) -> Vec<u8> {
    serde_json::to_vec(&fixture()[name]).unwrap()
}

fn save(root: &Path, name: &str) {
    publish(root, &RealDisk, &envelope(name), &pins(), NOW, APP).unwrap();
}

#[test]
fn manifest_cache_reopens_exact_authenticated_bytes_and_rechecks_current_authority() {
    let dir = support::tempdir();
    assert!(load(dir.path(), &pins(), NOW, APP).unwrap().is_none());
    save(dir.path(), "r8");
    let reopened = load(dir.path(), &pins(), NOW, APP).unwrap().unwrap();
    assert_eq!(reopened.document().revision, 8);
    assert_eq!(reopened.document().policies[0].install, "suspend");
    assert_eq!(fs::read(dir.path().join("manifest.json")).unwrap(), envelope("r8"));
    assert!(load(dir.path(), &BTreeMap::new(), NOW, APP).is_err());
    assert!(load(dir.path(), &pins(), "2026-10-26T12:00:00Z", APP).is_err());
    assert!(load(dir.path(), &pins(), NOW, "0.0.1").is_err());
}

#[test]
fn manifest_cache_rejects_delayed_rollback_and_equal_revision_equivocation() {
    let dir = support::tempdir();
    save(dir.path(), "r8");
    for name in ["r7", "equivocation"] {
        assert!(publish(dir.path(), &RealDisk, &envelope(name), &pins(), NOW, APP).is_err());
        assert_eq!(load(dir.path(), &pins(), NOW, APP).unwrap().unwrap().document().revision, 8);
    }
    save(dir.path(), "r8");
    save(dir.path(), "r9");
    assert!(publish(dir.path(), &RealDisk, &envelope("r8"), &pins(), NOW, APP).is_err());
    save(dir.path(), "r10");
    let lifted = load(dir.path(), &pins(), NOW, APP).unwrap().unwrap();
    for policy in &lifted.document().policies {
        assert_eq!(policy.install, "allow");
        assert_eq!(policy.launch, "allow");
    }
}

#[test]
fn manifest_cache_partial_initialization_never_becomes_first_use() {
    for files in [vec!["initialized"], vec!["accepted.json"], vec!["manifest.json"], vec!["accepted.json", "manifest.json"]] {
        let dir = support::tempdir();
        for file in files {
            fs::write(dir.path().join(file), b"damaged").unwrap();
        }
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
    }
}

#[test]
fn manifest_cache_refused_candidate_does_not_initialize_a_new_store() {
    let dir = support::tempdir();
    for bytes in [b"bad signature".to_vec(), vec![b'x'; 1024 * 1024 + 1]] {
        assert!(publish(dir.path(), &RealDisk, &bytes, &pins(), NOW, APP).is_err());
        assert!(load(dir.path(), &pins(), NOW, APP).unwrap().is_none());
    }
    assert!(publish(dir.path(), &RealDisk, &envelope("r8"), &pins(), "2026-10-26T12:00:00Z", APP).is_err());
    assert!(publish(dir.path(), &RealDisk, &envelope("r8"), &pins(), NOW, "0.0.1").is_err());
    assert!(load(dir.path(), &pins(), NOW, APP).unwrap().is_none());
}

#[test]
fn manifest_cache_missing_or_invalid_floor_cannot_be_reconstructed_from_cached_bytes() {
    let digest = "a".repeat(64);
    let invalid = [
        b"null".to_vec(),
        b"[]".to_vec(),
        b"{}".to_vec(),
        vec![b'x'; 4097],
        format!(r#"[8,"{digest}"]"#).into_bytes(),
        format!(r#"{{"revision":8,"digest":"{digest}","extra":true}}"#).into_bytes(),
        format!(r#"{{"revision":0,"digest":"{digest}"}}"#).into_bytes(),
        format!(r#"{{"revision":9007199254740992,"digest":"{digest}"}}"#).into_bytes(),
        format!(r#"{{"revision":8,"digest":"{}"}}"#, "A".repeat(64)).into_bytes(),
        format!(r#"{{"revision":8,"revision":7,"digest":"{digest}"}}"#).into_bytes(),
        format!(r#"{{"revision":8,"digest":"{digest}","digest":"{digest}"}}"#).into_bytes(),
        format!(r#"{{"revision":8,"digest":"{digest}"}} null"#).into_bytes(),
        br#"{"revision":8,"revision":7,"digest":"bad"}"#.to_vec(),
    ];
    for bytes in invalid {
        let dir = support::tempdir();
        save(dir.path(), "r8");
        fs::write(dir.path().join("accepted.json"), bytes).unwrap();
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
    }
    for file in ["accepted.json", "initialized"] {
        let dir = support::tempdir();
        save(dir.path(), "r8");
        fs::remove_file(dir.path().join(file)).unwrap();
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
    }
}

#[test]
fn manifest_cache_unreadable_state_and_bad_marker_fail_closed() {
    for name in ["initialized", "accepted.json", "manifest.json"] {
        let dir = support::tempdir();
        save(dir.path(), "r8");
        let path = dir.path().join(name);
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        if name != "manifest.json" {
            assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
        }
    }
    for bytes in [b"damaged".to_vec(), vec![b'x'; 4097]] {
        let dir = support::tempdir();
        save(dir.path(), "r8");
        fs::write(dir.path().join("initialized"), bytes).unwrap();
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
    }
}

#[test]
fn manifest_cache_oversized_otherwise_valid_state_is_not_parsed_as_authority() {
    for (file, limit) in [("initialized", 16), ("accepted.json", 4096), ("manifest.json", 1024 * 1024)] {
        let dir = support::tempdir();
        save(dir.path(), "r8");
        let path = dir.path().join(file);
        let mut bytes = fs::read(&path).unwrap();
        bytes.resize(limit + 1, b' ');
        fs::write(&path, bytes).unwrap();
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        if file != "manifest.json" {
            assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
        }
    }
}

#[test]
fn manifest_cache_requires_exact_floor_even_for_an_authentic_newer_cache() {
    for name in ["r7", "r9", "equivocation"] {
        let dir = support::tempdir();
        save(dir.path(), "r8");
        fs::write(dir.path().join("manifest.json"), envelope(name)).unwrap();
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        save(dir.path(), "r8");
        assert_eq!(load(dir.path(), &pins(), NOW, APP).unwrap().unwrap().document().revision, 8);
    }
}

#[test]
fn manifest_cache_damage_is_recoverable_only_against_the_surviving_floor() {
    for bytes in [None, Some(b"{".to_vec()), Some(vec![b'x'; 1024 * 1024 + 1])] {
        let dir = support::tempdir();
        save(dir.path(), "r8");
        let path = dir.path().join("manifest.json");
        if let Some(bytes) = bytes {
            fs::write(&path, bytes).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
        assert!(load(dir.path(), &pins(), NOW, APP).is_err());
        assert!(publish(dir.path(), &RealDisk, &envelope("r7"), &pins(), NOW, APP).is_err());
        save(dir.path(), "r9");
        assert_eq!(load(dir.path(), &pins(), NOW, APP).unwrap().unwrap().document().revision, 9);
    }
}

struct FailingDisk {
    target: &'static str,
    rename: bool,
}

impl Disk for FailingDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if !self.rename && path.file_name().unwrap() == format!("{}.tmp", self.target).as_str() {
            fs::write(path, &bytes[..bytes.len() / 2])?;
            return Err(io::Error::other("injected incomplete write"));
        }
        RealDisk.write(path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.rename && to.file_name().unwrap() == self.target {
            return Err(io::Error::other("injected failed rename"));
        }
        RealDisk.rename(from, to)
    }
}

#[test]
fn manifest_cache_initial_write_failures_preserve_closed_restart_states() {
    for target in ["initialized", "accepted.json", "manifest.json"] {
        for rename in [false, true] {
            let dir = support::tempdir();
            let disk = FailingDisk { target, rename };
            assert!(publish(dir.path(), &disk, &envelope("r8"), &pins(), NOW, APP).is_err());
            match target {
                "initialized" => assert!(load(dir.path(), &pins(), NOW, APP).unwrap().is_none()),
                "accepted.json" => {
                    assert!(load(dir.path(), &pins(), NOW, APP).is_err());
                    assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
                }
                _ => {
                    assert!(load(dir.path(), &pins(), NOW, APP).is_err());
                    assert!(publish(dir.path(), &RealDisk, &envelope("r7"), &pins(), NOW, APP).is_err());
                    save(dir.path(), "r8");
                }
            }
        }
    }
}

#[test]
fn manifest_cache_failed_publication_never_rolls_back_the_accepted_suspension() {
    for target in ["accepted.json", "manifest.json"] {
        for rename in [false, true] {
            let dir = support::tempdir();
            save(dir.path(), "r7");
            let disk = FailingDisk { target, rename };
            assert!(publish(dir.path(), &disk, &envelope("r8"), &pins(), NOW, APP).is_err());
            if target == "accepted.json" {
                assert_eq!(load(dir.path(), &pins(), NOW, APP).unwrap().unwrap().document().revision, 7);
            } else {
                assert!(load(dir.path(), &pins(), NOW, APP).is_err());
                assert!(publish(dir.path(), &RealDisk, &envelope("r7"), &pins(), NOW, APP).is_err());
            }
            save(dir.path(), "r8");
            assert_eq!(load(dir.path(), &pins(), NOW, APP).unwrap().unwrap().document().policies[0].install, "suspend");
        }
    }
}

#[test]
fn manifest_cache_another_instance_cannot_read_or_publish_during_publication() {
    let dir = support::tempdir();
    save(dir.path(), "r8");
    let held = fs::OpenOptions::new().read(true).write(true).open(dir.path().join("manifest.lock")).unwrap();
    held.try_lock().unwrap();
    assert!(matches!(load(dir.path(), &pins(), NOW, APP), Err(Error::Busy)));
    assert!(matches!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP), Err(Error::Busy)));
    drop(held);
    save(dir.path(), "r9");
}

struct ContendingDisk<'a> {
    root: &'a Path,
}

impl Disk for ContendingDisk<'_> {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        assert!(matches!(load(self.root, &pins(), NOW, APP), Err(Error::Busy)));
        assert!(matches!(publish(self.root, &RealDisk, &envelope("r7"), &pins(), NOW, APP), Err(Error::Busy)));
        RealDisk.write(path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        assert!(matches!(load(self.root, &pins(), NOW, APP), Err(Error::Busy)));
        assert!(matches!(publish(self.root, &RealDisk, &envelope("r9"), &pins(), NOW, APP), Err(Error::Busy)));
        RealDisk.rename(from, to)
    }
}

#[test]
fn manifest_cache_holds_the_same_lock_through_every_replacement_and_retry() {
    let dir = support::tempdir();
    let disk = ContendingDisk { root: dir.path() };
    publish(dir.path(), &disk, &envelope("r8"), &pins(), NOW, APP).unwrap();
    publish(dir.path(), &disk, &envelope("r9"), &pins(), NOW, APP).unwrap();
    assert_eq!(load(dir.path(), &pins(), NOW, APP).unwrap().unwrap().document().revision, 9);
}

struct InterruptedDisk {
    target: &'static str,
}

impl Disk for InterruptedDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        RealDisk.write(path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealDisk.rename(from, to)?;
        if to.file_name().unwrap() == self.target {
            return Err(io::Error::other("interrupted after publication"));
        }
        Ok(())
    }
}

#[test]
fn manifest_cache_restart_after_each_publication_boundary_retains_its_floor() {
    for target in ["initialized", "accepted.json", "manifest.json"] {
        let dir = support::tempdir();
        assert!(publish(dir.path(), &InterruptedDisk { target }, &envelope("r8"), &pins(), NOW, APP).is_err());
        if target == "initialized" {
            assert!(load(dir.path(), &pins(), NOW, APP).is_err());
            assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, APP).is_err());
        } else {
            assert!(publish(dir.path(), &RealDisk, &envelope("r7"), &pins(), NOW, APP).is_err());
            if target == "manifest.json" {
                assert_eq!(load(dir.path(), &pins(), NOW, APP).unwrap().unwrap().document().revision, 8);
            } else {
                assert!(load(dir.path(), &pins(), NOW, APP).is_err());
            }
            save(dir.path(), "r8");
        }
    }
}

#[test]
fn manifest_cache_invalid_recovery_does_not_change_a_surviving_floor() {
    let dir = support::tempdir();
    save(dir.path(), "r8");
    fs::write(dir.path().join("manifest.json"), b"damaged").unwrap();
    let floor = fs::read(dir.path().join("accepted.json")).unwrap();
    assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), "2026-10-26T12:00:00Z", APP).is_err());
    assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &BTreeMap::new(), NOW, APP).is_err());
    assert!(publish(dir.path(), &RealDisk, &envelope("r9"), &pins(), NOW, "0.0.1").is_err());
    assert_eq!(fs::read(dir.path().join("accepted.json")).unwrap(), floor);
    assert!(load(dir.path(), &pins(), NOW, APP).is_err());
    save(dir.path(), "r8");
}
