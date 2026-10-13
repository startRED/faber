use faber::local_update::*;
use faber_engine::project::update::{CanaryVerdict, ExactTree, Slot, SwapCheck};
use faber_engine::project::{GitMode, Grant, RepoIdentity, SCHEMA};
use std::fs;

fn area() -> tempfile::TempDir {
    tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

fn config(dir: &std::path::Path) -> Development {
    Development::new("development", dir.join("dev"), dir.join("stable-data"), dir.join("release")).unwrap()
}

fn tree(bytes: &[u8]) -> ExactTree {
    ExactTree::new(&"1".repeat(40), &"2".repeat(40), &ExactTree::compute_build_hash(bytes), SCHEMA).unwrap()
}

#[test]
fn explicit_channel_and_separate_roots_are_required() {
    let dir = area();
    assert!(Development::new("release", dir.path().join("dev"), dir.path().join("data"), dir.path().join("release")).is_err());
    assert!(Development::new("development", dir.path().join("data/dev"), dir.path().join("data"), dir.path().join("release")).is_err());
    assert!(Development::new("development", "E:/Apps/Faber".into(), dir.path().join("data"), dir.path().join("release")).is_err());
    let dev = config(dir.path());
    assert_eq!(dev.data(), dir.path().join("dev/data"));
}

#[test]
fn development_data_is_anchored_before_any_engine_or_webview_start() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    fs::write(dev.data(), b"not a data directory").unwrap();
    assert!(dev.data_lease().is_err());
    assert_eq!(fs::read(dev.data()).unwrap(), b"not a data directory");
    fs::remove_file(dev.data()).unwrap();
    let lease = dev.data_lease().unwrap();
    assert!(dev.data().is_dir());
    drop(lease);
}

#[test]
fn low_space_guard_rejects_unknown_and_insufficient_space_before_build() {
    assert!(check_space(None, 1024).is_err());
    assert!(check_space(Some(1023), 1024).is_err());
    assert!(check_space(Some(1024), 1024).is_ok());
    assert!(check_space(Some(u64::MAX), 0).is_err());
}

#[test]
fn helper_and_slot_commands_are_native_headless_and_use_separate_data() {
    let dir = area();
    let dev = config(dir.path());
    let command = dev.slot_command(Slot::B);
    assert_eq!(command.program, dir.path().join(format!("dev/slots/b/faber{}", std::env::consts::EXE_SUFFIX)));
    assert!(command.args.contains(&"--faber-local-start".into()));
    assert_eq!(command.environment.get("FABER_DATA_DIR"), Some(&dev.data().to_string_lossy().into_owned()));
    assert!(!command.environment.keys().any(|key| key.contains("TOKEN") || key.contains("API_KEY")));
}

fn pending(dev: &Development) {
    let manager = dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    fs::write(dev.binary(Slot::B), b"next").unwrap();
    manager.adopt_baseline(Slot::A, tree(b"old"), b"old", 100).unwrap();
    let repo = RepoIdentity::new("repo", "checkout");
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, 100);
    grant.self_update = true;
    manager.prepare_next(tree(b"next"), &grant, &repo, 1, &[], 101).unwrap();
    manager
        .record_canary_verdict(
            CanaryVerdict::Pass {
                verified_at: 102,
                receipt_id: "canary".into(),
                tree: tree(b"next"),
                proof_receipt: "proof".into(),
                compatible_schemas: vec![SCHEMA],
            },
            b"next",
            102,
        )
        .unwrap();
    manager.enter_quiescence(false, false).unwrap();
    manager.begin_swap(&grant, &repo, 1, SwapCheck { payload: b"next", active_window: false, active_operations: false }, 103).unwrap();
}

#[test]
fn monitor_waits_for_health_and_first_operation_then_five_minutes() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let manager = dev.manager().unwrap();
    manager.begin_probation(104).unwrap();
    assert_eq!(monitor(&dev, true, 134).unwrap(), Monitor::Waiting);
    manager.acknowledge_health(134).unwrap();
    manager.complete_first_operation(135).unwrap();
    assert_eq!(monitor(&dev, true, 403).unwrap(), Monitor::Waiting);
    assert_eq!(monitor(&dev, true, 404).unwrap(), Monitor::Healthy);
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
}

#[test]
fn timeout_crash_and_interrupted_swap_restore_immutable_last_good_once() {
    for cause in ["timeout", "crash", "interrupted"] {
        let dir = area();
        let dev = config(dir.path());
        pending(&dev);
        let manager = dev.manager().unwrap();
        let before = manager.manifest().unwrap().unwrap().last_good;
        if cause != "interrupted" {
            manager.begin_probation(104).unwrap();
        }
        let result = if cause == "interrupted" { recover(&dev, 105).unwrap() } else { monitor(&dev, cause != "crash", 135).unwrap() };
        assert_eq!(result, Monitor::Rollback(Slot::A));
        assert_eq!(manager.manifest().unwrap().unwrap().last_good, before);
        assert_eq!(recover(&dev, 136).unwrap(), Monitor::Stopped);
        assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
        assert!(!dev.binary(Slot::B).exists(), "R106 removes failed owned output after process settlement");
    }
}

#[test]
fn tampered_last_good_is_never_launched() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    fs::write(dev.binary(Slot::A), b"tampered").unwrap();
    assert!(recover(&dev, 105).is_err());
}

#[test]
fn updater_channel_selection_never_downloads_public_release_for_local_builds() {
    assert!(release_updates("release", false, false));
    assert!(!release_updates("development", false, false));
    assert!(!release_updates("release", true, false));
    assert!(!release_updates("release", false, true));
    assert!(!release_updates("unknown", false, false));
}

#[test]
fn one_coordinator_owns_the_transition_and_lock_survives_reopen() {
    let dir = area();
    let dev = config(dir.path());
    let lock = dev.lock().unwrap();
    assert!(dev.lock().is_err());
    drop(lock);
    assert!(dev.lock().is_ok());
}

#[test]
fn recovery_requires_an_existing_unchanged_trusted_helper() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    assert!(dev.verify_helper().is_err());
    let binary = dir.path().join("baseline");
    let helper = dir.path().join("trusted-helper");
    fs::write(&binary, b"old").unwrap();
    fs::write(&helper, b"helper").unwrap();
    dev.adopt(tree(b"old"), &binary, &helper, 100).unwrap();
    dev.verify_helper().unwrap();
    fs::write(dev.helper(), b"changed helper").unwrap();
    assert!(dev.verify_helper().is_err());
}

#[test]
fn revoked_transition_refuses_before_reading_or_rotating_the_helper() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let grant = Grant::new("repo", GitMode::Seguro, 1, 100);
    let result = dev.transition(
        &grant,
        &RepoIdentity::new("repo", "checkout"),
        1,
        SwapCheck { payload: b"next", active_window: false, active_operations: false },
        104,
    );
    assert!(matches!(result, Err(faber_engine::project::update::UpdateError::Grant(_))));
}

#[test]
fn source_snapshot_refuses_links_git_metadata_and_parent_paths() {
    let dir = area();
    for (mode, name) in [
        ("120000", "link"),
        ("160000", "submodule"),
        ("100644", "../escape"),
        ("100644", ".git/config"),
        ("100644", "E:/escape"),
        ("100644", "node_modules/escape"),
    ] {
        let record = format!("{mode} blob {}\t{name}\0", "3".repeat(40));
        assert!(snapshot_entries(record.as_bytes(), dir.path()).is_err(), "{name}");
    }
    let record = format!("100644 blob {}\tsrc/a.rs\0", "3".repeat(40));
    let entries = snapshot_entries(record.as_bytes(), dir.path()).unwrap();
    assert_eq!(entries[0].path, dir.path().join("src/a.rs"));
}

#[test]
fn compatibility_requires_lossless_old_and_new_round_trips() {
    let saved = serde_json::json!({"schema": 1, "spent": 7, "grant": {"consumed": true}, "extension": "keep"});
    assert!(compatible_round_trip(&saved, &saved, &saved).is_ok());
    for changed in [
        serde_json::json!({"schema": 1, "spent": 0}),
        serde_json::json!({"schema": 1, "spent": 7, "grant": {"consumed": false}, "extension": "keep"}),
    ] {
        assert!(compatible_round_trip(&saved, &changed, &saved).is_err());
        assert!(compatible_round_trip(&saved, &saved, &changed).is_err());
    }
}

#[test]
fn cleanup_preserves_current_and_last_good_and_removes_owned_build_output() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let manager = dev.manager().unwrap();
    manager.begin_probation(104).unwrap();
    manager.acknowledge_health(105).unwrap();
    manager.complete_first_operation(106).unwrap();
    manager.finish_probation(404).unwrap();
    dev.begin_build_output().unwrap();
    fs::write(dev.build_dir().join("large.obj"), b"output").unwrap();
    dev.clean_build_output().unwrap();
    assert!(!dev.build_dir().exists());
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
    assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next");
    assert!(dev.remove_slot(Slot::A).is_err());
    assert!(dev.remove_slot(Slot::B).is_err());
}

#[test]
fn startup_never_resumes_before_compatibility_health_and_operation() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    dev.manager().unwrap().begin_probation(104).unwrap();
    assert!(dev.resume_allowed().is_err());
    dev.manager().unwrap().acknowledge_health(105).unwrap();
    assert!(dev.resume_allowed().is_err());
    dev.manager().unwrap().complete_first_operation(106).unwrap();
    assert!(dev.resume_allowed().is_ok());
}

#[test]
fn active_slot_lock_blocks_cleanup_and_candidate_reuse() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let lease = dev.slot_lease(Slot::B).unwrap();
    dev.manager().unwrap().recover_transition(105).unwrap();
    assert!(dev.remove_slot(Slot::B).is_err());
    assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next");
    drop(lease);
    dev.remove_slot(Slot::B).unwrap();
    assert!(!dev.binary(Slot::B).exists());
}

#[test]
fn local_build_has_fixed_native_offline_commands_owned_target_and_no_installer() {
    let dir = area();
    let commands = build_commands(dir.path(), &dir.path().join("target"), &dir.path().join("node.exe"), &dir.path().join("cargo.exe"));
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0].args[0], "node_modules/vite/bin/vite.js");
    assert_eq!(commands[0].args[1], "build");
    let cargo = &commands[1];
    assert!(cargo.args.contains(&"--offline".into()));
    assert!(cargo.args.contains(&"--locked".into()));
    assert!(cargo.args.contains(&"--release".into()));
    assert!(cargo.args.contains(&"faber-local".into()));
    assert_eq!(cargo.environment["CARGO_TARGET_DIR"], dir.path().join("target").to_string_lossy());
    assert!(!cargo.args.contains(&"--bundle".into()));
    assert!(!cargo.args.contains(&"install".into()));
}

#[test]
fn user_open_cancels_quiescence_or_waits_for_health() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    assert!(!dev.request_open().unwrap());
    assert!(dev.open_pending().unwrap());
    let manager = dev.manager().unwrap();
    manager.begin_probation(104).unwrap();
    assert!(!dev.request_open().unwrap());
    manager.acknowledge_health(105).unwrap();
    manager.complete_first_operation(106).unwrap();
    assert!(dev.request_open().unwrap());
    assert!(!dev.open_pending().unwrap());
}

#[test]
fn cleanup_refuses_uncertain_build_directory() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    fs::create_dir_all(dev.build_dir()).unwrap();
    fs::write(dev.build_dir().join("unknown"), b"retain").unwrap();
    assert!(dev.clean_build_output().is_err());
    assert_eq!(fs::read(dev.build_dir().join("unknown")).unwrap(), b"retain");
}

#[test]
fn new_or_corrupt_saved_data_blocks_health_without_resetting_spend() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    fs::create_dir_all(dev.data()).unwrap();
    for saved in [b"{\"schema\":999,\"spent\":12}".as_slice(), b"invalid"] {
        let path = dev.data().join("mission.json");
        fs::write(&path, saved).unwrap();
        assert!(validate_saved_data(&dev.data()).is_err());
        assert_eq!(fs::read(path).unwrap(), saved);
    }
}

#[test]
fn probation_clock_reversal_and_missing_operation_roll_back() {
    for time in [103, 404] {
        let dir = area();
        let dev = config(dir.path());
        pending(&dev);
        dev.manager().unwrap().begin_probation(104).unwrap();
        if time == 404 {
            dev.manager().unwrap().acknowledge_health(105).unwrap();
        }
        assert_eq!(monitor(&dev, true, time).unwrap(), Monitor::Rollback(Slot::A));
    }
}

#[test]
fn corrupted_startup_manifest_recovers_from_the_independent_transition_checkpoint() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    fs::write(dev.manager().unwrap().layout().manifest_path(), b"corrupt").unwrap();
    assert_eq!(recover(&dev, 105).unwrap(), Monitor::Rollback(Slot::A));
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
    assert_eq!(recover(&dev, 106).unwrap(), Monitor::Stopped);
    fs::write(dev.manager().unwrap().layout().manifest_path(), b"corrupt again").unwrap();
    assert!(recover(&dev, 107).is_err(), "a consumed checkpoint must never restart last-good twice");
}

struct Source {
    calls: usize,
    tree: String,
}
impl faber_engine::project::git::Executor for Source {
    fn execute(
        &mut self,
        call: &faber_engine::project::git::Invocation,
    ) -> std::result::Result<faber_engine::project::git::Output, faber_engine::project::git::GitError> {
        self.calls += 1;
        let output = match call.args[0].as_str() {
            "rev-parse" => format!("{}\n", self.tree).into_bytes(),
            "ls-tree" => format!("100644 blob {}\tsrc/accepted.rs\0", "3".repeat(40)).into_bytes(),
            "cat-file" => b"accepted bytes".to_vec(),
            _ => panic!("unexpected git command: {:?}", call.args),
        };
        assert!(!call.identity);
        Ok(faber_engine::project::git::Output { success: true, stdout: output })
    }
}

struct Gates {
    fail: &'static str,
    built: bool,
}
impl Runner for Gates {
    fn available_space(&self, _: &std::path::Path) -> Option<u64> {
        Some(u64::MAX - 1)
    }
    fn accepted(
        &mut self,
        _: &RepoIdentity,
        _: &str,
        _: &str,
        snapshot: &std::path::Path,
    ) -> std::result::Result<String, faber_engine::project::update::UpdateError> {
        assert_eq!(fs::read(snapshot.join("src/accepted.rs")).unwrap(), b"accepted bytes");
        assert!(!snapshot.join("unaccepted.rs").exists());
        if self.fail == "proof-review" {
            return Err(faber_engine::project::update::UpdateError::InvalidState("not accepted"));
        }
        Ok("proof".into())
    }
    fn build(
        &mut self,
        snapshot: &std::path::Path,
        target: &std::path::Path,
    ) -> std::result::Result<(std::path::PathBuf, std::path::PathBuf), faber_engine::project::update::UpdateError> {
        self.built = true;
        if self.fail == "mutated-source" {
            fs::write(snapshot.join("src/accepted.rs"), b"unaccepted mutation").unwrap();
        }
        if self.fail == "injected-source" {
            fs::write(snapshot.join("src/injected.rs"), b"unaccepted addition").unwrap();
        }
        fs::create_dir_all(target).unwrap();
        let exe = target.join("faber.exe");
        let helper = target.join("faber-local.exe");
        fs::write(&exe, b"next").unwrap();
        fs::write(&helper, b"next helper").unwrap();
        Ok((exe, helper))
    }
    fn compatible(
        &mut self,
        _: &std::path::Path,
        _: &std::path::Path,
        saved: &std::path::Path,
    ) -> std::result::Result<(), faber_engine::project::update::UpdateError> {
        assert!(saved.ends_with("compatibility"));
        if self.fail == "compatibility" {
            return Err(faber_engine::project::update::UpdateError::InvalidState("incompatible"));
        }
        Ok(())
    }
    fn canary(
        &mut self,
        _: &std::path::Path,
        tree: &ExactTree,
        isolated: &std::path::Path,
    ) -> std::result::Result<CanaryVerdict, faber_engine::project::update::UpdateError> {
        assert!(isolated.ends_with("canary"));
        assert!(fs::read_to_string(isolated.join("src/lib.rs")).unwrap().contains("trims_and_lowercases"));
        let goal: serde_json::Value = serde_json::from_slice(&fs::read(isolated.join("goal.json")).unwrap()).unwrap();
        assert_eq!(goal["recursiveSelfUpdateAllowed"], false);
        assert_eq!(goal["remoteEffectsAllowed"], false);
        assert_eq!(goal["spendingReservationRequired"], true);
        if self.fail == "canary-unavailable" {
            return Ok(CanaryVerdict::NotRun);
        }
        Ok(CanaryVerdict::Pass {
            verified_at: now(),
            receipt_id: "fake-and-real-canary".into(),
            tree: tree.clone(),
            proof_receipt: if self.fail == "stale-proof" { "wrong" } else { "proof" }.into(),
            compatible_schemas: vec![SCHEMA],
        })
    }
}

#[test]
fn exact_tree_stage_enforces_every_gate_and_preserves_dirty_owner_checkout() {
    for failure in [
        "",
        "proof-review",
        "compatibility",
        "canary-unavailable",
        "stale-proof",
        "wrong-tree",
        "no-space",
        "revoked",
        "mutated-source",
        "injected-source",
    ] {
        let dir = area();
        let dev = config(dir.path());
        let manager = dev.manager().unwrap();
        fs::write(dev.binary(Slot::A), b"old").unwrap();
        manager.adopt_baseline(Slot::A, tree(b"old"), b"old", now()).unwrap();
        let checkout = dir.path().join("checkout");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(checkout.join("unaccepted.rs"), b"dirty owner bytes").unwrap();
        let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
        let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
        grant.self_update = failure != "revoked";
        let mut source = Source { calls: 0, tree: if failure == "wrong-tree" { "9" } else { "2" }.repeat(40) };
        let mut gates = Gates { fail: failure, built: false };
        let result = dev.stage(
            Stage {
                repo: &repo,
                commit: &"1".repeat(40),
                tree: &"2".repeat(40),
                grant: &grant,
                revision: 1,
                required_space: if failure == "no-space" { u64::MAX } else { 1 },
            },
            &mut source,
            &mut gates,
        );
        assert_eq!(result.is_ok(), failure.is_empty() || failure == "canary-unavailable", "{failure}: {result:?}");
        assert_eq!(fs::read(checkout.join("unaccepted.rs")).unwrap(), b"dirty owner bytes");
        assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
        assert!(!dev.build_dir().exists());
        if ["no-space", "revoked"].contains(&failure) {
            assert_eq!(source.calls, 0);
        }
        if failure.is_empty() {
            assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next");
        }
        if failure == "canary-unavailable" {
            assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next");
            assert!(manager.enter_quiescence(false, false).is_err());
            let mut ready = Gates { fail: "", built: false };
            assert_eq!(
                dev.stage(
                    Stage { repo: &repo, commit: &"4".repeat(40), tree: &"2".repeat(40), grant: &grant, revision: 1, required_space: 1 },
                    &mut source,
                    &mut ready
                )
                .unwrap(),
                Slot::B
            );
            assert!(matches!(manager.manifest().unwrap().unwrap().state, faber_engine::project::update::UpdateState::Ready { .. }));
        }
        if ["proof-review", "wrong-tree", "no-space", "revoked"].contains(&failure) {
            assert!(!gates.built);
        }
    }
}

#[test]
fn only_new_accepted_candidate_can_supersede_a_waiting_build() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let manager = dev.manager().unwrap();
    assert!(manager.supersede_next(&Grant::new("repo", GitMode::Seguro, 1, 100), &RepoIdentity::new("repo", "checkout"), 1, 104).is_err());
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, 100);
    grant.self_update = true;
    assert!(manager.supersede_next(&grant, &RepoIdentity::new("repo", "checkout"), 1, 104).is_err(), "swap cannot be coalesced");
}
