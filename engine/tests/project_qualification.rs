//! A receipt opens a role gate only while it is private, fresh and bound to the installed version and bytes.
use faber_engine::model::Vendor;
use faber_engine::project::qualification::{
    CHECKS, Identity, MAX_AGE_SECS, Presence, Proof, Receipt, Reject, SourceKind, fingerprint, fingerprint_source, hook_flags,
    identify_sources, issue, launch_policy, managed_policy_clear, observe_pinned, parse_version, path_presence, session_sources,
    sources_unchanged, verify, verify_current,
};
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

fn proof(vendor: Vendor) -> Proof {
    match vendor {
        Vendor::Claude => Proof { hook_flags: hook_flags(&launch()).unwrap(), instruction_sources: vec![] },
        Vendor::Codex => Proof::default(),
    }
}

/// The role launch flags that load no hooks.
fn launch() -> Vec<String> {
    faber_engine::claude::claude_exposure_args()
}

fn issued(dir: &Path, vendor: Vendor) {
    issue(dir, vendor, &identity(), passed(), proof(vendor), NOW).unwrap();
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
        assert!(issue(dir.path(), Vendor::Claude, &identity(), checks, proof(Vendor::Claude), NOW).is_err(), "{name}");
        let mut checks = passed();
        checks.remove(name);
        assert!(issue(dir.path(), Vendor::Claude, &identity(), checks, proof(Vendor::Claude), NOW).is_err(), "{name}");
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
        assert!(issue(dir.path(), Vendor::Claude, &bad, passed(), proof(Vendor::Claude), NOW).is_err());
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
    issue(dir.path(), Vendor::Claude, &newer, passed(), proof(Vendor::Claude), NOW + 5).unwrap();
    assert_eq!(check(dir.path(), Vendor::Claude, NOW + 5, identity()), Err(Reject::Binary));
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

#[test]
fn a_claude_launch_without_the_hook_flags_never_qualifies() {
    let args = launch();
    assert!(hook_flags(&args).is_some());
    let without = |flag: &str| -> Vec<String> {
        let at = args.iter().position(|a| a == flag).unwrap();
        // Safe mode is a bare flag; the others carry one value.
        let width = if flag == "--safe-mode" { 1 } else { 2 };
        args.iter().enumerate().filter(|(i, _)| *i < at || *i >= at + width).map(|(_, a)| a.clone()).collect()
    };
    for flag in ["--safe-mode", "--setting-sources", "--settings"] {
        assert!(hook_flags(&without(flag)).is_none(), "{flag}");
    }
    let mut hooks_on = args.clone();
    let at = hooks_on.iter().position(|a| a == "--settings").unwrap() + 1;
    hooks_on[at] = r#"{"disableAllHooks":false}"#.into();
    assert!(hook_flags(&hooks_on).is_none());
    // The writer refuses a missing proof, and a stored receipt without it (or with hooks on) stays closed.
    let dir = tempfile::tempdir().unwrap();
    assert!(issue(dir.path(), Vendor::Claude, &identity(), passed(), Proof::default(), NOW).is_err());
    issued(dir.path(), Vendor::Claude);
    rewrite(dir.path(), Vendor::Claude, |v| v["proof"]["hook_flags"] = serde_json::json!([]));
    assert_eq!(check(dir.path(), Vendor::Claude, NOW, identity()), Err(Reject::Incomplete));
}

fn sources_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let clone = tmp.path().join("clone");
    let global = tmp.path().join("home").join(".codex");
    std::fs::create_dir_all(clone.join("lease")).unwrap();
    std::fs::create_dir_all(&global).unwrap();
    (tmp, clone, global)
}

#[test]
fn codex_instruction_sources_accept_only_the_clone_rules_and_the_global_file() {
    let (_tmp, clone, global) = sources_fixture();
    std::fs::write(clone.join("AGENTS.md"), "project rules").unwrap();
    std::fs::write(global.join("AGENTS.md"), "my rules").unwrap();
    let lease = clone.join("lease");
    let declared = [clone.join("AGENTS.md"), global.join("AGENTS.md")].map(|p| p.to_string_lossy().into_owned());
    let found = identify_sources(&declared, &lease, &clone, &global).unwrap();
    assert_eq!(found.iter().map(|s| s.kind).collect::<Vec<_>>(), [SourceKind::Clone, SourceKind::Global]);
    assert_eq!(found[0].sha256, fingerprint(&clone.join("AGENTS.md")).unwrap());
    assert_ne!(found[0].sha256, found[1].sha256);
    assert!(identify_sources(&[], &lease, &clone, &global).unwrap().is_empty());
    // The receipt records them and still opens.
    let dir = tempfile::tempdir().unwrap();
    let with = Proof { hook_flags: vec![], instruction_sources: found };
    issue(dir.path(), Vendor::Codex, &identity(), passed(), with, NOW).unwrap();
    assert_eq!(check(dir.path(), Vendor::Codex, NOW, identity()), Ok(()));
}

#[test]
fn any_other_codex_instruction_source_refuses() {
    let (tmp, clone, global) = sources_fixture();
    std::fs::write(global.join("AGENTS.md"), "my rules").unwrap();
    let other = tmp.path().join("home").join("AGENTS.md");
    std::fs::write(&other, "elsewhere").unwrap();
    let credentials = global.join("auth.json");
    std::fs::write(&credentials, "{}").unwrap();
    let elsewhere_clone = tmp.path().join("other-clone");
    std::fs::create_dir_all(&elsewhere_clone).unwrap();
    std::fs::write(elsewhere_clone.join("AGENTS.md"), "not this clone").unwrap();
    let lease = clone.join("lease");
    let refuse = |path: &Path| identify_sources(&[path.to_string_lossy().into_owned()], &lease, &clone, &global);
    for path in [&other, &credentials, &elsewhere_clone.join("AGENTS.md"), &global.join("missing.md"), &global.join("config.toml")] {
        assert_eq!(refuse(path), Err(Reject::Source), "{path:?}");
    }
    // An AGENTS.md above the clone root, or in a sibling folder of the lease, is not the clone's own.
    std::fs::create_dir_all(clone.join("sibling")).unwrap();
    std::fs::write(clone.join("sibling").join("AGENTS.md"), "x").unwrap();
    assert_eq!(refuse(&clone.join("sibling").join("AGENTS.md")), Err(Reject::Source));
    // One bad source refuses the whole session.
    std::fs::write(clone.join("AGENTS.md"), "ok").unwrap();
    let mixed = [clone.join("AGENTS.md"), other].map(|p| p.to_string_lossy().into_owned());
    assert_eq!(identify_sources(&mixed, &lease, &clone, &global), Err(Reject::Source));
}

fn codex_receipt_with(dir: &Path, sources: Vec<faber_engine::project::qualification::Source>) {
    let with = Proof { hook_flags: vec![], instruction_sources: sources };
    issue(dir, Vendor::Codex, &identity(), passed(), with, NOW).unwrap();
}

fn current(dir: &Path, vendor: Vendor, managed: Result<(), Reject>) -> Result<(), Reject> {
    verify_current(dir, vendor, NOW, &|| Ok(identity()), &|| managed).map(|_| ())
}

#[test]
fn a_changed_or_vanished_instruction_file_closes_a_receipt_that_recorded_it() {
    let (_tmp, clone, global) = sources_fixture();
    std::fs::write(global.join("AGENTS.md"), "my rules").unwrap();
    let declared = [global.join("AGENTS.md").to_string_lossy().into_owned()];
    let found = identify_sources(&declared, &clone.join("lease"), &clone, &global).unwrap();
    let dir = tempfile::tempdir().unwrap();
    codex_receipt_with(dir.path(), found.clone());
    assert_eq!(current(dir.path(), Vendor::Codex, Ok(())), Ok(()));
    assert_eq!(sources_unchanged(&found), Ok(()));
    std::fs::write(global.join("AGENTS.md"), "my rules, now with a new hook-like instruction").unwrap();
    assert_eq!(current(dir.path(), Vendor::Codex, Ok(())), Err(Reject::Source));
    std::fs::write(global.join("AGENTS.md"), "my rules").unwrap();
    assert_eq!(current(dir.path(), Vendor::Codex, Ok(())), Ok(()));
    std::fs::remove_file(global.join("AGENTS.md")).unwrap();
    assert_eq!(current(dir.path(), Vendor::Codex, Ok(())), Err(Reject::Source));
}

#[test]
fn a_session_that_declares_another_instruction_source_than_the_receipt_is_refused() {
    let (_tmp, clone, global) = sources_fixture();
    std::fs::write(global.join("AGENTS.md"), "my rules").unwrap();
    std::fs::write(clone.join("AGENTS.md"), "project rules").unwrap();
    let both = [global.join("AGENTS.md"), clone.join("AGENTS.md")].map(|p| p.to_string_lossy().into_owned());
    let only_global = [both[0].clone()];
    let recorded = identify_sources(&only_global, &clone.join("lease"), &clone, &global).unwrap();
    assert_eq!(session_sources(&only_global, &recorded), Ok(()));
    // A source added after issuance, or one the receipt never recorded, refuses; so does a removed one.
    assert_eq!(session_sources(&both, &recorded), Err(Reject::Source));
    assert_eq!(session_sources(&[], &recorded), Err(Reject::Source));
    assert_eq!(session_sources(&["Z:/does/not/exist/AGENTS.md".to_string()], &recorded), Err(Reject::Source));
}

#[test]
fn managed_policy_must_be_confirmed_absent_everywhere_and_uncertainty_refuses() {
    let absent = |_: &str| Presence::Absent;
    assert_eq!(managed_policy_clear(&absent, &absent), Ok(()));
    for bad in [Presence::Present, Presence::Unproven] {
        let one_path = |p: &str| if p.ends_with("managed-settings.d") { bad } else { Presence::Absent };
        assert_eq!(managed_policy_clear(&one_path, &absent), Err(Reject::Policy), "{bad:?}");
        let one_key = |k: &str| if k.starts_with("HKCU") { bad } else { Presence::Absent };
        assert_eq!(managed_policy_clear(&absent, &one_key), Err(Reject::Policy), "{bad:?}");
    }
    // A Claude receipt closes the moment a policy appears, however fresh it is.
    let dir = tempfile::tempdir().unwrap();
    issued(dir.path(), Vendor::Claude);
    assert_eq!(current(dir.path(), Vendor::Claude, Ok(())), Ok(()));
    assert_eq!(current(dir.path(), Vendor::Claude, Err(Reject::Policy)), Err(Reject::Policy));
    // Real inspection: a missing path is absent, an existing one present.
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(path_presence(&tmp.path().join("nothing").to_string_lossy()), Presence::Absent);
    assert_eq!(path_presence(&tmp.path().to_string_lossy()), Presence::Present);
}

#[test]
fn an_instruction_file_is_hashed_through_one_handle_and_never_through_a_link() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("AGENTS.md");
    std::fs::write(&file, "rules").unwrap();
    let hash = fingerprint_source(&file).unwrap();
    assert_eq!(hash, fingerprint(&file).unwrap());
    assert!(fingerprint_source(tmp.path()).is_err(), "a folder is not an instruction file");
    assert!(fingerprint_source(&tmp.path().join("missing")).is_err());
    std::fs::write(tmp.path().join("big.md"), vec![b'x'; 4 * 1024 * 1024 + 1]).unwrap();
    assert!(fingerprint_source(&tmp.path().join("big.md")).is_err(), "an oversized file is refused");
    // A link to a file outside is refused even though its target is a plain file (needs a privilege on some machines).
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.md"), "outside").unwrap();
    let link = tmp.path().join("link.md");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(outside.path().join("secret.md"), &link).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(outside.path().join("secret.md"), &link).is_ok();
    if linked {
        assert!(fingerprint_source(&link).is_err());
        // The same link planted as the clone's own rules file is not hashed through: identify refuses it.
        let (_t, clone, global) = sources_fixture();
        std::fs::remove_dir_all(&clone).unwrap();
        std::fs::create_dir_all(clone.join("lease")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.md"), clone.join("AGENTS.md")).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(outside.path().join("secret.md"), clone.join("AGENTS.md")).unwrap();
        let declared = [clone.join("AGENTS.md").to_string_lossy().into_owned()];
        assert_eq!(identify_sources(&declared, &clone.join("lease"), &clone, &global), Err(Reject::Source));
    }
}

#[cfg(windows)]
#[test]
fn instruction_sources_match_by_the_real_path_whatever_the_case_or_prefix_of_the_declared_one() {
    let (_tmp, clone, global) = sources_fixture();
    std::fs::write(global.join("AGENTS.md"), "my rules").unwrap();
    let real = std::fs::canonicalize(global.join("AGENTS.md")).unwrap();
    let shown = real.to_string_lossy().into_owned();
    let plain = shown.trim_start_matches(r"\\?\").to_string();
    for declared in [shown.clone(), plain.clone(), plain.to_uppercase(), plain.replace('\\', "/")] {
        let found = identify_sources(std::slice::from_ref(&declared), &clone.join("lease"), &clone, &global)
            .unwrap_or_else(|e| panic!("{declared}: {e:?}"));
        assert_eq!(found[0].kind, SourceKind::Global);
        assert_eq!(session_sources(&[declared], &found), Ok(()));
    }
}

/// A receipt proves the probes passed under the exact role launch parameters the engine used then. One issued under other
/// parameters, or before they were recorded (every receipt from before this check), asks for a new qualification run, even
/// for an unchanged executable; one issued now opens. Each vendor's revision is its own.
#[test]
fn a_receipt_from_another_launch_policy_asks_to_requalify() {
    let dir = tempfile::tempdir().unwrap();
    for vendor in Vendor::ALL {
        issued(dir.path(), vendor);
        let current = |dir: &Path| verify_current(dir, vendor, NOW, &|| Ok(identity()), &|| Ok(())).map(|_| ());
        assert_eq!(current(dir.path()), Ok(()), "{vendor:?}");
        rewrite(dir.path(), vendor, |v| v.as_object_mut().unwrap().remove("launch_policy").map(|_| ()).unwrap());
        assert_eq!(current(dir.path()), Err(Reject::Requalify), "{vendor:?}: a pre-merge receipt");
        issued(dir.path(), vendor);
        rewrite(dir.path(), vendor, |v| v["launch_policy"] = "0".repeat(64).into());
        assert_eq!(current(dir.path()), Err(Reject::Requalify), "{vendor:?}: another policy");
    }
    assert_ne!(launch_policy(Vendor::Claude), launch_policy(Vendor::Codex));
    for vendor in Vendor::ALL {
        assert_eq!(launch_policy(vendor), launch_policy(vendor), "{vendor:?}: stable for the same parameters");
    }
}

/// F5: admission hashes the program before it runs anything. A program whose bytes are not the receipt's is never asked
/// for `--version` (here a file that cannot run at all answers with its hash), and the matching one is.
#[test]
fn a_program_is_hashed_before_it_is_asked_for_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("cli.exe");
    std::fs::write(&program, b"not a program").unwrap();
    let hash = fingerprint(&program).unwrap();
    let (other, pinned) = observe_pinned(&program.to_string_lossy(), &"0".repeat(64)).unwrap();
    assert_eq!((other.version.as_str(), other.binary_hash.as_str(), pinned.sha256.as_str()), ("", hash.as_str(), hash.as_str()));
    drop(pinned);
    assert!(observe_pinned(&program.to_string_lossy(), &hash).is_err(), "the matching program is run, and this one cannot run");
}
