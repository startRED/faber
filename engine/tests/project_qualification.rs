//! A receipt opens a role gate only while it is private, fresh and bound to the installed version and bytes.
use faber_engine::model::Vendor;
use faber_engine::project::qualification::{CHECKS, Identity, MAX_AGE_SECS, Receipt, Reject, fingerprint, issue, parse_version, verify};
use std::collections::BTreeMap;
use std::io;
use std::path::Path;

const NOW: u64 = 1_800_000_000;
const HASH: &str = "76de74f7694a5bdc03378a5f837c9e132827bf004c98c276bc4e9094ff06769a";

fn identity() -> Identity {
    Identity { version: "2.1.296 (Claude Code)".into(), binary_hash: HASH.into() }
}

fn passed() -> BTreeMap<String, bool> {
    CHECKS.iter().map(|name| (name.to_string(), true)).collect()
}

fn issued(dir: &Path, vendor: Vendor) {
    issue(dir, vendor, &identity(), passed(), NOW).unwrap();
}

fn check(dir: &Path, vendor: Vendor, now: u64, actual: Identity) -> Result<(), Reject> {
    verify(dir, vendor, now, &|| Ok(actual.clone()))
}

fn rewrite(dir: &Path, vendor: Vendor, change: impl FnOnce(&mut serde_json::Value)) {
    let path = dir.join(format!("{}.json", vendor.name()));
    let mut value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    change(&mut value);
    std::fs::write(path, value.to_string()).unwrap();
}

#[test]
fn a_fresh_private_receipt_for_the_installed_version_and_bytes_opens() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    assert_eq!(check(dir.path(), Vendor::Claude, NOW + 60, identity()), Ok(()));
    assert_eq!(check(dir.path(), Vendor::Claude, NOW + MAX_AGE_SECS, identity()), Ok(()));
}

#[test]
fn no_receipt_means_closed_and_nothing_is_observed() {
    let dir = tempfile::tempdir().unwrap();
    let probe = || -> io::Result<Identity> { panic!("an unqualified vendor must not cost a process") };
    assert_eq!(verify(dir.path(), Vendor::Claude, NOW, &probe), Err(Reject::Missing));
    assert_eq!(verify(&dir.path().join("absent"), Vendor::Codex, NOW, &probe), Err(Reject::Missing));
}

#[test]
fn a_changed_version_string_closes_the_receipt() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    for version in ["2.1.297 (Claude Code)", "2.1.296 (Claude Code) ", "2.1.296", ""] {
        let actual = Identity { version: version.into(), ..identity() };
        assert_eq!(check(dir.path(), Vendor::Claude, NOW, actual), Err(Reject::Version), "{version:?}");
    }
}

#[test]
fn a_changed_binary_closes_the_receipt() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    let actual = Identity { binary_hash: HASH.replace('7', "8"), ..identity() };
    assert_eq!(check(dir.path(), Vendor::Claude, NOW, actual), Err(Reject::Binary));
}

#[test]
fn the_hash_follows_the_program_bytes_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("cli.exe");
    std::fs::write(&program, b"program one").unwrap();
    let first = fingerprint(&program).unwrap();
    assert_eq!(first, fingerprint(&program).unwrap());
    assert_eq!(first.len(), 64);
    std::fs::write(&program, b"program onf").unwrap();
    assert_ne!(first, fingerprint(&program).unwrap());
    assert!(fingerprint(&dir.path().join("missing.exe")).is_err());
}

#[test]
fn old_or_future_dated_receipts_are_closed() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    assert_eq!(check(dir.path(), Vendor::Claude, NOW + MAX_AGE_SECS + 1, identity()), Err(Reject::Stale));
    assert_eq!(check(dir.path(), Vendor::Claude, NOW - 1, identity()), Err(Reject::Stale));
}

#[test]
fn a_receipt_never_opens_another_vendor() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    assert_eq!(check(dir.path(), Vendor::Codex, NOW, identity()), Err(Reject::Missing));
    // Copying one vendor's file under the other's name does not help: the vendor inside is bound too.
    std::fs::copy(dir.path().join("claude.json"), dir.path().join("codex.json")).unwrap();
    assert_eq!(check(dir.path(), Vendor::Codex, NOW, identity()), Err(Reject::Vendor));
}

#[test]
fn a_receipt_must_record_every_probe_as_passed() {
    let dir = tempfile::tempdir().unwrap();
    for name in CHECKS {
        let mut checks = passed();
        checks.insert(name.into(), false);
        assert!(issue(dir.path(), Vendor::Claude, &identity(), checks, NOW).is_err(), "{name}");
        let mut checks = passed();
        checks.remove(name);
        assert!(issue(dir.path(), Vendor::Claude, &identity(), checks, NOW).is_err(), "{name}");
        // A hand-edited file gets no further than the writer: the reader checks the same set.
        issued(dir.path(), Vendor::Claude);
        rewrite(dir.path(), Vendor::Claude, |v| v["checks"][name] = false.into());
        assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Incomplete), "{name}");
        rewrite(dir.path(), Vendor::Claude, |v| {
            v["checks"][name] = true.into();
            v["checks"].as_object_mut().unwrap().remove(name);
        });
        assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Incomplete), "{name}");
    }
    assert!(!dir.path().join("codex.json").exists());
}

#[test]
fn only_a_measured_identity_can_be_recorded() {
    let dir = tempfile::tempdir().unwrap();
    for bad in [
        Identity { binary_hash: HASH.to_uppercase(), ..identity() },
        Identity { binary_hash: "abc".into(), ..identity() },
        Identity { version: "".into(), ..identity() },
        Identity { version: "two\nlines".into(), ..identity() },
        Identity { version: "x".repeat(129), ..identity() },
    ] {
        assert!(issue(dir.path(), Vendor::Claude, &bad, passed(), NOW).is_err());
    }
    assert!(parse_version("codex-cli 0.162.1\n").is_ok());
    assert!(parse_version("a\u{0}b").is_err());
}

#[test]
fn smuggled_claims_and_malformed_files_are_closed() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    rewrite(dir.path(), Vendor::Claude, |v| v["admit"] = true.into());
    assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Malformed));
    issued(dir.path(), Vendor::Claude);
    rewrite(dir.path(), Vendor::Claude, |v| v["schema"] = 2.into());
    assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Vendor));
    std::fs::write(dir.path().join("claude.json"), b"not json").unwrap();
    assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Malformed));
    std::fs::write(dir.path().join("claude.json"), vec![b' '; 9000]).unwrap();
    assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Unsafe));
}

#[test]
fn a_receipt_that_is_not_a_plain_private_file_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("claude.json")).unwrap();
    assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Unsafe));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::remove_dir(dir.path().join("claude.json")).unwrap();
        issued(dir.path(), Vendor::Claude);
        let path = dir.path().join("claude.json");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Unsafe));
    }
}

#[test]
fn an_unobservable_cli_is_closed_even_with_a_perfect_receipt() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    let result = verify(dir.path(), Vendor::Claude, NOW, &|| Err(io::Error::other("cli missing")));
    assert_eq!(result, Err(Reject::Unobserved));
}

#[test]
fn requalification_replaces_the_receipt_atomically() {
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    let newer = Identity { version: "2.1.297 (Claude Code)".into(), binary_hash: HASH.replace('7', "8") };
    issue(dir.path(), Vendor::Claude, &newer, passed(), NOW + 5).unwrap();
    assert_eq!(check(dir.path(), Vendor::Claude, NOW + 5, identity()), Err(Reject::Version));
    assert_eq!(check(dir.path(), Vendor::Claude, NOW + 5, newer.clone()), Ok(()));
    let stored: Receipt = serde_json::from_slice(&std::fs::read(dir.path().join("claude.json")).unwrap()).unwrap();
    assert_eq!((stored.version, stored.issued_at), (newer.version, NOW + 5));
    // No temporary file is left next to the receipt.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn the_engine_gate_has_no_caller_input_and_stays_closed_without_a_receipt() {
    for vendor in Vendor::ALL {
        assert!(faber_engine::project::policy::admit(vendor).is_err());
        assert!(faber_engine::project::policy::admit_launch(vendor, "claude-that-does-not-exist").is_err());
    }
}

#[test]
fn a_linked_receipt_is_closed() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    issued(elsewhere.path(), Vendor::Claude);
    let (target, link) = (elsewhere.path().join("claude.json"), dir.path().join("claude.json"));
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&target, &link).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&target, &link).is_ok();
    // Creating a link needs a privilege on some machines; where it is refused there is nothing to open.
    if linked {
        assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Unsafe));
    }
}
