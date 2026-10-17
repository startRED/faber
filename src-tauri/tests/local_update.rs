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
    // The slot reaches the CLIs' sign-in and the build toolchain through the session's folders, and nothing else of it.
    assert_eq!(command.environment.get("USERPROFILE"), std::env::var("USERPROFILE").ok().as_ref());
    assert_eq!(command.environment.get("CARGO_HOME"), std::env::var("CARGO_HOME").ok().as_ref());
}

#[test]
fn development_autostart_uses_the_trusted_launcher_instead_of_a_slot() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    fs::write(dev.helper(), b"trusted helper").unwrap();
    fs::write(dir.path().join("dev/helper.sha256"), ExactTree::compute_build_hash(b"trusted helper")).unwrap();
    fs::write(
        dir.path().join("dev/development.json"),
        serde_json::to_vec(&serde_json::json!({
            "channel": "development", "installedData": dir.path().join("stable-data"), "installedRelease": dir.path().join("release")
        }))
        .unwrap(),
    )
    .unwrap();
    for slot in [Slot::A, Slot::B] {
        let command = faber::autostart::launch_command(&dev.binary(slot), Some(&dev)).unwrap();
        assert_eq!(command.program, dev.helper());
        assert_eq!(command.args, ["--launch"]);
        assert!(!command.environment.contains_key("FABER_CHANNEL"));
    }
    fs::write(dev.helper(), b"changed helper").unwrap();
    assert!(faber::autostart::launch_command(&dev.binary(Slot::A), Some(&dev)).is_err());
    let installed = dir.path().join(format!("release/faber{}", std::env::consts::EXE_SUFFIX));
    let command = faber::autostart::launch_command(&installed, None).unwrap();
    assert_eq!(command.program, installed);
    assert_eq!(command.args, [faber::autostart::AUTOSTART_FLAG]);
}

#[test]
fn legacy_slot_autostart_discovers_enrollment_and_refuses_an_untrusted_entry() {
    let dir = area();
    let dev = config(dir.path());
    fs::write(dir.path().join("baseline"), b"old").unwrap();
    fs::write(dir.path().join("helper"), b"trusted helper").unwrap();
    dev.adopt(tree(b"old"), &dir.path().join("baseline"), &dir.path().join("helper"), 100).unwrap();
    for slot in [Slot::A, Slot::B] {
        let discovered = Development::from_slot(&dev.binary(slot)).unwrap().unwrap();
        assert_eq!(faber::autostart::launch_command(&dev.binary(slot), Some(&discovered)).unwrap().program, dev.helper());
        #[cfg(windows)]
        Development::from_slot(&std::path::PathBuf::from(dev.binary(slot).to_string_lossy().to_uppercase()))
            .unwrap()
            .unwrap()
            .verify_helper()
            .unwrap();
    }
    assert!(Development::from_slot(&dir.path().join(format!("release/faber{}", std::env::consts::EXE_SUFFIX))).unwrap().is_none());
    assert!(Development::from_slot(&dir.path().join(format!("dev/slots/unknown/faber{}", std::env::consts::EXE_SUFFIX))).is_err());
    fs::write(dir.path().join("dev/development.json"), b"corrupt enrollment").unwrap();
    assert!(Development::from_slot(&dev.binary(Slot::A)).is_err());
}

fn ready(dev: &Development) {
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
}

fn pending(dev: &Development) {
    ready(dev);
    let manager = dev.manager().unwrap();
    let repo = RepoIdentity::new("repo", "checkout");
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, 100);
    grant.self_update = true;
    manager.enter_quiescence(false, false).unwrap();
    manager.begin_swap(&grant, &repo, 1, SwapCheck { payload: b"next", active_window: false, active_operations: false }, 103).unwrap();
}

fn running_mission(
    dev: &Development,
    repo: &std::path::Path,
) -> (faber_engine::project::store::Journal, faber_engine::project::run::Runner) {
    let j = faber_engine::project::store::Journal::open(repo, &dev.data(), "m1").unwrap();
    let mission = faber_engine::project::Mission::new("m1", "p1", "repo", "Fix", &"1".repeat(40), 1, 100);
    let mut runner = faber_engine::project::run::Runner::create(&j, mission).unwrap();
    runner.mission.state = faber_engine::project::MissionState::Running;
    runner.save(&j).unwrap();
    (j, runner)
}

#[test]
fn running_mission_blocks_promotion_until_paused() {
    use faber_engine::project::update::{UpdateError, UpdateState};
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let (j, mut runner) = running_mission(&dev, &dir.path().join("repo"));
    let manager = dev.manager().unwrap();
    manager.begin_probation(104).unwrap();
    manager.acknowledge_health(105).unwrap();
    manager.complete_first_operation(105).unwrap();
    let before = manager.manifest().unwrap().unwrap();
    assert_eq!(monitor(&dev, true, 404), Err(UpdateError::ActiveOperations));
    assert_eq!(manager.manifest().unwrap().unwrap(), before);
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
    runner.pause(&j).unwrap();
    assert_eq!(monitor(&dev, true, 404).unwrap(), Monitor::Healthy);
    assert_eq!(manager.manifest().unwrap().unwrap().state, UpdateState::Healthy);
}

#[test]
fn running_mission_blocks_transition_despite_the_callers_idle_claim() {
    use faber_engine::project::update::UpdateError;
    let dir = area();
    let dev = config(dir.path());
    ready(&dev);
    fs::write(dev.helper(), b"trusted helper").unwrap();
    fs::write(dir.path().join("dev/helper.sha256"), ExactTree::compute_build_hash(b"trusted helper")).unwrap();
    let (j, mut runner) = running_mission(&dev, &dir.path().join("repo"));
    let before = dev.manager().unwrap().manifest().unwrap();
    let repo = RepoIdentity::new("repo", "checkout");
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, 100);
    grant.self_update = true;
    // A visible-window refusal keeps this unit test process-free after the mission becomes safe.
    let check = || SwapCheck { payload: b"next", active_window: true, active_operations: false };
    assert_eq!(dev.transition(&grant, &repo, 1, check(), 103), Err(UpdateError::ActiveOperations));
    assert_eq!(dev.manager().unwrap().manifest().unwrap(), before);
    runner.pause(&j).unwrap();
    assert_eq!(dev.transition(&grant, &repo, 1, check(), 103), Err(UpdateError::ActiveWindow));
}

#[test]
fn paused_mission_with_an_active_lease_blocks_rollback_without_losing_last_good() {
    use faber_engine::project::update::UpdateError;
    use faber_engine::project::{Lease, LeaseState, Runtime};
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let (j, mut runner) = running_mission(&dev, &dir.path().join("repo"));
    runner.pause(&j).unwrap();
    let mut runtime = Runtime::new();
    runtime.leases.push(Lease::new("repo", "m1", "w1", 1, vec!["src".into()], &"1".repeat(40), &"2".repeat(64), 1));
    let before = dev.manager().unwrap().manifest().unwrap();
    for state in [LeaseState::Held, LeaseState::Releasing, LeaseState::Quarantined] {
        runtime.leases[0].state = state;
        j.save_runtime(&runtime).unwrap();
        assert_eq!(recover(&dev, 104), Err(UpdateError::ActiveOperations));
        assert_eq!(dev.manager().unwrap().manifest().unwrap(), before);
        for slot in [Slot::A, Slot::B] {
            assert!(dev.binary(slot).exists());
        }
    }
    runtime.leases[0].state = LeaseState::Released;
    j.save_runtime(&runtime).unwrap();
    assert_eq!(recover(&dev, 104).unwrap(), Monitor::Rollback(Slot::A));
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
}

#[test]
fn pending_effect_and_heavy_proof_block_rollback_without_mutating_builds() {
    use faber_engine::project::update::UpdateError;
    use faber_engine::project::{Intent, IntentKind};
    for heavy in [false, true] {
        let dir = area();
        let dev = config(dir.path());
        pending(&dev);
        let (j, mut runner) = running_mission(&dev, &dir.path().join("repo"));
        runner.pause(&j).unwrap();
        if heavy {
            let path = j.private().join("run.json");
            let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            json["heavyProof"] = serde_json::json!(["w1", "C1", "identity", "receipt"]);
            fs::write(path, serde_json::to_vec(&json).unwrap()).unwrap();
        } else {
            j.record_intent(&Intent::new("dispatch", "m1", IntentKind::Dispatch, 1, 100)).unwrap();
        }
        let before = dev.manager().unwrap().manifest().unwrap();
        assert_eq!(recover(&dev, 104), Err(UpdateError::ActiveOperations));
        assert_eq!(dev.manager().unwrap().manifest().unwrap(), before);
        assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
        assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next");
    }
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
    let promoted = manager.manifest().unwrap().unwrap().current_good.unwrap();
    let evidence: CanaryVerdict = serde_json::from_value(promoted.extra["canaryEvidence"].clone()).unwrap();
    assert!(matches!(evidence, CanaryVerdict::Pass { proof_receipt, .. } if proof_receipt == "proof"));
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
fn enrolled_launcher_has_explicit_configuration_without_a_slot_or_environment() {
    let dir = area();
    let dev = config(dir.path());
    let executable = dir.path().join("baseline");
    let helper = dir.path().join("helper");
    fs::write(&executable, b"old").unwrap();
    fs::write(&helper, b"helper").unwrap();
    dev.adopt(tree(b"old"), &executable, &helper, 100).unwrap();
    let descriptor: serde_json::Value = serde_json::from_slice(&fs::read(dir.path().join("dev/development.json")).unwrap()).unwrap();
    assert_eq!(descriptor["channel"], "development");
    assert_eq!(descriptor["installedData"], dir.path().join("stable-data").to_string_lossy().as_ref());
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
fn executable_data_probe_round_trips_real_records_and_extensions_without_writes() {
    let dir = area();
    let data = dir.path().join("data");
    fs::create_dir(&data).unwrap();
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, 100);
    grant.extra.insert("consumed".into(), true.into());
    let mut runtime = faber_engine::project::Runtime::new();
    runtime.extra.insert("spent".into(), 7.into());
    let mut mission = faber_engine::project::Mission::new("m1", "p1", "repo", "goal", "base", 1, 100);
    mission.extra.insert("extension".into(), "keep".into());
    for (name, value) in [
        ("grant.json", serde_json::to_value(grant).unwrap()),
        ("runtime.json", serde_json::to_value(runtime).unwrap()),
        ("mission.json", serde_json::to_value(mission).unwrap()),
    ] {
        fs::write(data.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    let before: Vec<_> = fs::read_dir(&data)
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            (path.clone(), fs::read(path).unwrap())
        })
        .collect();
    let output = dir.path().join("probe.json");
    probe_saved_data(&data, &output).unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(receipt["grant.json"]["consumed"], true);
    assert_eq!(receipt["runtime.json"]["spent"], 7);
    assert_eq!(receipt["mission.json"]["extension"], "keep");
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn the_engine_budget_file_is_a_ledger_with_mission_spending_inside_and_round_trips() {
    let dir = area();
    let data = dir.path().join("data");
    fs::create_dir(&data).unwrap();
    let mut ledger = faber_engine::ledger::Ledger::default();
    ledger.extra.insert("missionBudget".into(), serde_json::to_value(faber_engine::project::budget::Budget::default()).unwrap());
    ledger.extra.insert("fromANewerFaber".into(), serde_json::json!({"keep": [1, 2]}));
    faber_engine::store::Store::new(&data).save_ledger(&ledger).unwrap();
    let output = dir.path().join("probe.json");
    probe_saved_data(&data, &output).unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    assert_eq!(receipt["budget.json"]["fromANewerFaber"]["keep"], serde_json::json!([1, 2]));
    assert!(receipt["budget.json"]["missionBudget"].is_object());
    // Spending that does not read back is refused.
    let mut broken = faber_engine::ledger::Ledger::default();
    broken.extra.insert("missionBudget".into(), serde_json::json!({"schema": 99}));
    faber_engine::store::Store::new(&data).save_ledger(&broken).unwrap();
    assert!(probe_saved_data(&data, &dir.path().join("probe2.json")).is_err());
}

#[test]
fn user_open_really_cancels_quiescence_and_requires_a_fresh_canary() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let path = dir.path().join("dev/manifest.json");
    let mut saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    saved["state"] = "quiescent".into();
    fs::write(path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert!(dev.request_open().unwrap());
    assert!(matches!(
        dev.manager().unwrap().manifest().unwrap().unwrap().state,
        faber_engine::project::update::UpdateState::CanaryPending { .. }
    ));
    assert!(dev.manager().unwrap().enter_quiescence(false, false).is_err());
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
        fs::write(
            target.join("local-identity.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema": if self.fail == "new-schema" { SCHEMA + 1 } else { SCHEMA },
                "buildHash": ExactTree::compute_build_hash(b"next")
            }))
            .unwrap(),
        )
        .unwrap();
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
fn healthy_checkpoint_never_replays_an_older_transition() {
    for prepare_again in [false, true] {
        let dir = area();
        let dev = config(dir.path());
        pending(&dev);
        let manager = dev.manager().unwrap();
        manager.begin_probation(104).unwrap();
        manager.acknowledge_health(105).unwrap();
        manager.complete_first_operation(106).unwrap();
        manager.finish_probation(404).unwrap();
        if prepare_again {
            let mut grant = Grant::new("repo", GitMode::Seguro, 1, 100);
            grant.self_update = true;
            manager.prepare_next(tree(b"third"), &grant, &RepoIdentity::new("repo", "checkout"), 1, &[], 405).unwrap();
            fs::write(dev.binary(Slot::A), b"third").unwrap();
        }
        fs::write(dir.path().join("dev/manifest.json"), b"corrupt").unwrap();
        let _ = recover(&dev, 406);
        assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next", "healthy payload is immutable");
        let manifest = manager.manifest().unwrap().unwrap();
        assert_eq!(manifest.active_slot, Slot::B);
        if prepare_again {
            assert_eq!(manifest.last_good.as_ref().unwrap().slot, Slot::B);
        }
        assert!(!manifest.rejected_build_hashes.contains(&tree(b"next").build_hash));
    }
}

#[test]
fn rollback_relaunch_decision_survives_cleanup_failures() {
    for locked in [false, true] {
        let dir = area();
        let dev = config(dir.path());
        pending(&dev);
        dev.manager().unwrap().begin_probation(104).unwrap();
        let lease = locked.then(|| dev.slot_lease(Slot::B).unwrap());
        if !locked {
            fs::create_dir_all(dev.build_dir()).unwrap();
            fs::write(dev.build_dir().join("unowned"), b"retain").unwrap();
        }
        assert_eq!(monitor(&dev, false, 105).unwrap(), Monitor::Rollback(Slot::A));
        assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
        drop(lease);
        if locked {
            dev.cleanup_obsolete().unwrap();
            assert!(!dev.binary(Slot::B).exists());
        }
    }
}

#[test]
fn healthy_handoff_survives_cleanup_failure() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let manager = dev.manager().unwrap();
    manager.begin_probation(104).unwrap();
    manager.acknowledge_health(105).unwrap();
    manager.complete_first_operation(106).unwrap();
    fs::create_dir_all(dev.build_dir()).unwrap();
    fs::write(dev.build_dir().join("unowned"), b"retain").unwrap();
    assert_eq!(monitor(&dev, true, 404).unwrap(), Monitor::Healthy);
    assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next");
}

#[test]
fn interrupted_empty_build_cleanup_does_not_wedge_staging() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    fs::create_dir_all(dev.build_dir()).unwrap();
    dev.clean_build_output().unwrap();
    dev.begin_build_output().unwrap();
    dev.clean_build_output().unwrap();
}

#[test]
fn clean_quit_pauses_probation_without_quarantine_or_relaunch() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let manager = dev.manager().unwrap();
    manager.begin_probation(104).unwrap();
    manager.acknowledge_health(105).unwrap();
    manager.complete_first_operation(106).unwrap();
    fs::write(dir.path().join("dev/exit.json"), serde_json::to_vec(&serde_json::json!({"tree": tree(b"next")})).unwrap()).unwrap();
    assert_eq!(monitor(&dev, false, 107).unwrap(), Monitor::Stopped);
    let manifest = manager.manifest().unwrap().unwrap();
    assert!(manifest.rejected_build_hashes.is_empty());
    assert_eq!(fs::read(dev.binary(Slot::B)).unwrap(), b"next");
}

#[test]
fn orderly_session_end_before_health_retries_full_probation_without_rejection() {
    let dir = area();
    let dev = config(dir.path());
    pending(&dev);
    let manager = dev.manager().unwrap();
    manager.begin_probation(104).unwrap();
    manager.pause_probation(105).unwrap();
    assert!(manager.manifest().unwrap().unwrap().rejected_build_hashes.is_empty());
    manager.begin_probation(106).unwrap();
    let state = manager.manifest().unwrap().unwrap().state;
    assert!(matches!(
        state,
        faber_engine::project::update::UpdateState::Probation {
            health_acknowledged: false,
            health_ack_deadline: 136,
            probation_deadline: 406,
            ..
        }
    ));
}

#[test]
fn refused_transition_does_not_rotate_or_leak_a_helper() {
    for mapped_helper in [false, true] {
        let dir = area();
        let dev = config(dir.path());
        pending(&dev);
        fs::write(dev.helper(), b"trusted helper").unwrap();
        fs::write(dir.path().join("dev/helper.sha256"), ExactTree::compute_build_hash(b"trusted helper")).unwrap();
        fs::write(dir.path().join(format!("dev/slots/a/faber-local{}", std::env::consts::EXE_SUFFIX)), b"future helper").unwrap();
        fs::write(dir.path().join("dev/slots/a/helper.sha256"), ExactTree::compute_build_hash(b"future helper")).unwrap();
        let mut grant = Grant::new("repo", GitMode::Seguro, 1, 100);
        grant.self_update = true;
        #[cfg(windows)]
        let mapped = mapped_helper.then(|| {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new().read(true).share_mode(1).open(dev.helper()).unwrap()
        });
        #[cfg(not(windows))]
        let _ = mapped_helper;
        assert!(matches!(
            dev.transition(
                &grant,
                &RepoIdentity::new("repo", "checkout"),
                1,
                SwapCheck { payload: b"next", active_window: true, active_operations: false },
                104
            ),
            Err(faber_engine::project::update::UpdateError::ActiveWindow)
        ));
        assert_eq!(fs::read(dev.helper()).unwrap(), b"trusted helper");
        assert!(
            !fs::read_dir(dir.path().join("dev")).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".faber-patch-"))
        );
        #[cfg(windows)]
        drop(mapped);
    }
}

#[test]
fn candidate_reported_schema_cannot_bypass_rollback_compatibility() {
    let dir = area();
    let dev = config(dir.path());
    let manager = dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    manager.adopt_baseline(Slot::A, tree(b"old"), b"old", now()).unwrap();
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    let result = dev.stage(
        Stage { repo: &repo, commit: &"1".repeat(40), tree: &"2".repeat(40), grant: &grant, revision: 1, required_space: 1 },
        &mut Source { calls: 0, tree: "2".repeat(40) },
        &mut Gates { fail: "new-schema", built: false },
    );
    assert!(result.is_err(), "candidate schema must come from the candidate, not this process");
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
}

#[cfg(windows)]
#[test]
fn startup_accepts_windows_case_and_verbatim_aliases() {
    let dir = area();
    let dev = config(dir.path());
    let manager = dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    manager.adopt_baseline(Slot::A, tree(b"old"), b"old", 100).unwrap();
    let alias = std::path::PathBuf::from(dev.binary(Slot::A).to_string_lossy().trim_start_matches(r"\\?\").to_uppercase());
    dev.startup_ready(&alias).unwrap();
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

#[test]
fn waiting_canary_retries_the_same_payload_without_rebuilding() {
    let dir = area();
    let dev = config(dir.path());
    let manager = dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    manager.adopt_baseline(Slot::A, tree(b"old"), b"old", now()).unwrap();
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    let commit = "1".repeat(40);
    let source_tree = "2".repeat(40);
    let request = || Stage { repo: &repo, commit: &commit, tree: &source_tree, grant: &grant, revision: 1, required_space: 1 };
    let mut source = Source { calls: 0, tree: "2".repeat(40) };
    dev.stage(request(), &mut source, &mut Gates { fail: "canary-unavailable", built: false }).unwrap();
    assert!(matches!(manager.manifest().unwrap().unwrap().state, faber_engine::project::update::UpdateState::CanaryPending { .. }));
    let mut incompatible = Gates { fail: "compatibility", built: false };
    assert!(dev.stage(request(), &mut source, &mut incompatible).is_err(), "retry must probe the latest saved data");
    assert!(!incompatible.built);
    let mut retry = Gates { fail: "", built: false };
    dev.stage(request(), &mut source, &mut retry).unwrap();
    assert!(!retry.built, "quota/review availability must not force a rebuild");
    assert!(matches!(manager.manifest().unwrap().unwrap().state, faber_engine::project::update::UpdateState::Ready { .. }));
}

#[test]
fn incomplete_waiting_payload_requires_a_new_accepted_build() {
    let dir = area();
    let dev = config(dir.path());
    let manager = dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    manager.adopt_baseline(Slot::A, tree(b"old"), b"old", now()).unwrap();
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    let commit = "1".repeat(40);
    let source_tree = "2".repeat(40);
    let request = || Stage { repo: &repo, commit: &commit, tree: &source_tree, grant: &grant, revision: 1, required_space: 1 };
    let mut source = Source { calls: 0, tree: source_tree.clone() };
    dev.stage(request(), &mut source, &mut Gates { fail: "canary-unavailable", built: false }).unwrap();
    fs::remove_file(dir.path().join(format!("dev/slots/b/faber-local{}", std::env::consts::EXE_SUFFIX))).unwrap();
    let mut retry = Gates { fail: "", built: false };
    dev.stage(request(), &mut source, &mut retry).unwrap();
    assert!(retry.built, "partial slot publication must never reuse a canary-only shortcut");
}

#[test]
fn stage_landed_mission_skips_when_self_update_is_false() {
    let dir = area();
    let dev = config(dir.path());
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
    let grant = Grant::new("repo", GitMode::Seguro, 1, now());
    let mut source = Source { calls: 0, tree: "2".repeat(40) };
    let mut runner = Gates { fail: "", built: false };
    let staged = dev.stage_landed_mission(&repo, &grant, &"1".repeat(40), &"2".repeat(40), 1, &mut source, &mut runner).unwrap();
    assert_eq!(staged, None);
}

#[test]
fn stage_landed_mission_stages_when_self_update_is_true() {
    let dir = area();
    let dev = config(dir.path());
    let manager = dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    manager.adopt_baseline(Slot::A, tree(b"old"), b"old", now()).unwrap();
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    let commit = "1".repeat(40);
    let source_tree = "2".repeat(40);
    let mut source = Source { calls: 0, tree: source_tree.clone() };
    let mut runner = Gates { fail: "canary-unavailable", built: false };
    let staged = dev.stage_landed_mission(&repo, &grant, &commit, &source_tree, 1, &mut source, &mut runner).unwrap();
    assert_eq!(staged, Some(Slot::B));
}

#[test]
fn landed_build_waits_for_running_missions_before_any_build_effect() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    dev.manager().unwrap().adopt_baseline(Slot::A, tree(b"old"), b"old", now()).unwrap();
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    let j = faber_engine::project::store::Journal::open(&checkout, &dev.data(), "m1").unwrap();
    let mut active = faber_engine::project::run::Runner::create(
        &j,
        faber_engine::project::Mission::new("m1", "p1", "repo", "Active", &"1".repeat(40), 1, now()),
    )
    .unwrap();
    active.mission.state = faber_engine::project::MissionState::Running;
    active.save(&j).unwrap();
    let mut source = Source { calls: 0, tree: "2".repeat(40) };
    let mut runner = Gates { fail: "canary-unavailable", built: false };
    assert!(dev.stage_landed_mission(&repo, &grant, &"1".repeat(40), &"2".repeat(40), 1, &mut source, &mut runner).is_err());
    assert!(!runner.built);
    assert_eq!(source.calls, 0);
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
}

#[test]
fn production_approval_rejects_forged_repository_receipts() {
    let dir = area();
    let repo_dir = dir.path().join("repo");
    let receipts_dir = repo_dir.join(".faber/missions/m1/receipts");
    fs::create_dir_all(&receipts_dir).unwrap();
    let commit = "1".repeat(40);
    let tree_oid = "2".repeat(40);
    let receipt_json = serde_json::json!({
        "intent": {"id": "commit-123"},
        "receipt": {
            "outcome": "done",
            "evidence": {
                "head": commit,
                "tree": tree_oid
            }
        }
    });
    fs::write(receipts_dir.join("r1.json"), serde_json::to_vec(&receipt_json).unwrap()).unwrap();
    let repo = RepoIdentity::new("repo", &repo_dir.to_string_lossy());
    let snapshot = dir.path().join("snapshot");
    fs::create_dir(&snapshot).unwrap();
    let mut runner = ProductionRunner::new(
        dir.path().join("node_modules"),
        dir.path().join("node.exe"),
        dir.path().join("cargo.exe"),
        dir.path().join("private"),
        "m1".into(),
    );
    assert!(runner.accepted(&repo, &commit, &tree_oid, &snapshot).is_err());
    let unaccepted = runner.accepted(&repo, &"9".repeat(40), &tree_oid, &snapshot);
    assert!(unaccepted.is_err());
}

#[test]
fn production_runner_releases_power_after_error_or_panic() {
    use faber::awake::{CountedAwake, PowerKeepAwake};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    struct Power(Arc<AtomicBool>);
    impl PowerKeepAwake for Power {
        fn set_awake(&self, awake: bool) -> Result<(), String> {
            self.0.store(awake, Ordering::SeqCst);
            Ok(())
        }
    }
    let dir = area();
    let held = Arc::new(AtomicBool::new(false));
    let awake = Arc::new(Mutex::new(CountedAwake::new(Box::new(Power(held.clone())), true).unwrap()));
    for panic in [false, true] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut runner = ProductionRunner::new(
                dir.path().join("node_modules"),
                dir.path().join("node"),
                dir.path().join("cargo"),
                dir.path().join("private"),
                "m1".into(),
            )
            .with_awake(awake.clone());
            assert!(held.load(Ordering::SeqCst));
            if panic {
                panic!("build worker crashed");
            }
            assert!(runner.canary(&dir.path().join("missing"), &tree(b"payload"), dir.path()).is_err());
        }));
        assert_eq!(result.is_err(), panic);
        assert!(!held.load(Ordering::SeqCst), "production runner leaked power");
    }
}

#[test]
fn production_canary_rejects_a_non_executable_payload() {
    let dir = area();
    let exe = dir.path().join(format!("faber{}", std::env::consts::EXE_SUFFIX));
    fs::write(&exe, b"payload").unwrap();
    let tree_obj = tree(b"payload");
    let mut runner = ProductionRunner::new(
        dir.path().join("node_modules"),
        dir.path().join("node.exe"),
        dir.path().join("cargo.exe"),
        dir.path().join("private"),
        "m1".into(),
    );
    runner.canary = faber::local_update::CanaryMode::Unavailable;
    assert_eq!(runner.canary(&exe, &tree_obj, dir.path()).unwrap(), CanaryVerdict::NotRun);
    runner.canary = faber::local_update::CanaryMode::Offline;
    assert!(runner.canary(&exe, &tree_obj, dir.path()).is_err());
}

#[test]
fn canary_handshake_rejects_replay_hash_schema_and_fabricated_pass() {
    let candidate = tree(b"candidate");
    let mut response = CanaryHandshake {
        nonce: "challenge".into(),
        receipt: "challenge".into(),
        build_hash: candidate.build_hash.clone(),
        schema: SCHEMA,
        verdict: CanaryVerdict::NotRun,
        evidence: None,
        stopped: None,
    };
    assert_eq!(verify_canary_handshake(&response, "challenge", &candidate).unwrap(), CanaryVerdict::NotRun);
    assert!(verify_canary_handshake(&response, "previous", &candidate).is_err());
    response.build_hash = "0".repeat(64);
    assert!(verify_canary_handshake(&response, "challenge", &candidate).is_err());
    response.build_hash = candidate.build_hash.clone();
    response.schema += 1;
    assert!(verify_canary_handshake(&response, "challenge", &candidate).is_err());
    response.schema = SCHEMA;
    response.verdict = CanaryVerdict::Pass {
        verified_at: now(),
        receipt_id: "made-up".into(),
        tree: candidate.clone(),
        proof_receipt: "made-up".into(),
        compatible_schemas: vec![SCHEMA],
    };
    assert!(verify_canary_handshake(&response, "challenge", &candidate).is_err());
}

fn accepted_mission(
    dev: &Development,
    checkout: &std::path::Path,
) -> (faber_engine::Engine, faber_engine::project::store::Journal, RepoIdentity) {
    use faber_engine::project::proof::{Definition, Observation, ProofSet};
    use faber_engine::project::queue::{Candidate, Queue, Review};
    use faber_engine::project::{Intent, IntentKind, Mission, MissionState, Outcome, Receipt};
    let runtime = tokio::runtime::Handle::current();
    let engine = faber_engine::Engine::new(
        faber_engine::EngineOptions {
            data_dir: dev.data(),
            clis: Default::default(),
            spawn: std::sync::Arc::new(|_, _, _| panic!("unit test spawned a process")),
            windows: false,
            claude_env_billed: false,
            max_warm: 0,
            idle: std::time::Duration::from_secs(30),
            runtime,
            ask: None,
        },
        std::sync::Arc::new(|_| {}),
    );
    let repo = RepoIdentity::new("repo", &checkout.to_string_lossy());
    let mut project = engine.add_project(&checkout.to_string_lossy()).unwrap();
    let before = project.clone();
    project.repo = Some(repo.clone());
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    project.grant = Some(grant);
    engine.update_project(&before, &project).unwrap();
    let journal = faber_engine::project::store::Journal::open(checkout, &dev.data(), "m1").unwrap();
    let mut run =
        faber_engine::project::run::Runner::create(&journal, Mission::new("m1", &project.id, "repo", "Fix", &"3".repeat(40), 1, now()))
            .unwrap();
    let definition = Definition {
        id: "rust".into(),
        version: 1,
        program: std::env::temp_dir().join("cargo"),
        argv: vec!["test".into()],
        checks: vec!["C1".into()],
        toolchain: "rust".into(),
        dependencies: "deps".into(),
        environment: Default::default(),
        heavy: true,
    };
    let mut proof = ProofSet::new("2".repeat(40), "input".into(), vec![definition], vec!["src/accepted.rs".into()]).unwrap();
    proof
        .record(
            "proof",
            "rust",
            Observation {
                exit: Some(0),
                stdout: b"pass".to_vec(),
                stderr: vec![],
                evidence: faber_engine::project::wire::ArtifactRef {
                    path: "proof.log".into(),
                    sha256: faber_engine::project::lease::hash(b"pass"),
                },
            },
        )
        .unwrap();
    let candidate = Candidate::new(
        "w1",
        "3".repeat(40),
        "2".repeat(40),
        1,
        faber_engine::model::Vendor::Codex,
        "input".into(),
        vec!["src/accepted.rs".into()],
        proof.definitions().to_vec(),
    )
    .unwrap();
    let mut queue = Queue::default();
    queue.enqueue(candidate).unwrap();
    queue.prove("w1", proof.clone()).unwrap();
    queue
        .review(
            "w1",
            Review {
                vendor: faber_engine::model::Vendor::Claude,
                tree: "2".repeat(40),
                proof_digest: proof.digest(),
                session: "fresh".into(),
                fresh: true,
                accepted: true,
                findings: vec![],
            },
        )
        .unwrap();
    run.landings.push(faber_engine::project::run::Landing {
        candidate: queue.get("w1").unwrap().clone(),
        commit_intent: "commit".into(),
        forward_intent: "forward".into(),
        at: now(),
    });
    for (id, kind, evidence) in [
        ("commit", IntentKind::Commit, vec![("head", "1".repeat(40)), ("tree", "2".repeat(40)), ("parent", "3".repeat(40))]),
        ("forward", IntentKind::FastForward, vec![("new", "1".repeat(40)), ("old", "3".repeat(40))]),
    ] {
        journal.record_intent(&Intent::new(id, "m1", kind, 1, now())).unwrap();
        let mut receipt = Receipt::new(id, Outcome::Done, now());
        receipt.evidence = evidence.into_iter().map(|(k, v)| (k.into(), v)).collect();
        journal.record_receipt(&receipt).unwrap();
    }
    run.mission.state = MissionState::Done;
    run.promotion_pending = true;
    run.save(&journal).unwrap();
    (engine, journal, repo)
}

#[tokio::test]
async fn production_approval_requires_private_proof_review_commit_and_forward() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let (engine, journal, repo) = accepted_mission(&dev, &checkout);
    let mut runner = ProductionRunner::new(
        checkout.join("node_modules"),
        dir.path().join("node.exe"),
        dir.path().join("cargo.exe"),
        dev.data(),
        "m1".into(),
    );
    let accepted = runner.accepted(&repo, &"1".repeat(40), &"2".repeat(40), dir.path()).unwrap();
    assert_eq!(accepted, "m1-commit-forward");
    fs::remove_dir_all(journal.records().join("receipts")).unwrap();
    assert_eq!(runner.accepted(&repo, &"1".repeat(40), &"2".repeat(40), dir.path()).unwrap(), accepted);
    assert!(runner.accepted(&repo, &"9".repeat(40), &"2".repeat(40), dir.path()).is_err());
    let mut raw: serde_json::Value = serde_json::from_slice(&fs::read(journal.private().join("run.json")).unwrap()).unwrap();
    raw["landings"][0]["candidate"]["review"]["accepted"] = false.into();
    fs::write(journal.private().join("run.json"), serde_json::to_vec(&raw).unwrap()).unwrap();
    assert!(runner.accepted(&repo, &"1".repeat(40), &"2".repeat(40), dir.path()).is_err());
    engine.shutdown();
}

#[tokio::test]
async fn landed_checkpoint_drives_exact_tree_staging_at_quiescence() {
    let dir = area();
    let dev = config(dir.path());
    dev.manager().unwrap();
    fs::write(dev.binary(Slot::A), b"old").unwrap();
    dev.manager().unwrap().adopt_baseline(Slot::A, tree(b"old"), b"old", now()).unwrap();
    let checkout = dir.path().join("checkout");
    fs::create_dir(&checkout).unwrap();
    let (engine, journal, _repo) = accepted_mission(&dev, &checkout);
    let older_checkout = dir.path().join("older-checkout");
    fs::create_dir(&older_checkout).unwrap();
    let mut older_project = engine.add_project(&older_checkout.to_string_lossy()).unwrap();
    let before = older_project.clone();
    older_project.repo = Some(RepoIdentity::new("repo", &older_checkout.to_string_lossy()));
    engine.update_project(&before, &older_project).unwrap();
    let older_journal = faber_engine::project::store::Journal::open(&older_checkout, &dev.data(), "m0").unwrap();
    let mut older = faber_engine::project::run::Runner::create(
        &older_journal,
        faber_engine::project::Mission::new("m0", &older_project.id, "repo", "Older", &"3".repeat(40), 1, 1),
    )
    .unwrap();
    older.mission.state = faber_engine::project::MissionState::Done;
    older.promotion_pending = true;
    older.save(&older_journal).unwrap();
    let mut source = Source { calls: 0, tree: "2".repeat(40) };
    assert!(
        dev.service_landed_missions(&engine, true, &mut source, |mission, repo| {
            assert_eq!(mission, "m1");
            assert_eq!(repo, checkout);
            Ok(Gates { fail: "canary-unavailable", built: false })
        })
        .unwrap()
    );
    let manifest = dev.manager().unwrap().manifest().unwrap().unwrap();
    let next = manifest.staged_tree.unwrap();
    assert_eq!(next.commit_oid, "1".repeat(40));
    assert_eq!(next.tree_oid, "2".repeat(40));
    assert_eq!(fs::read(dev.binary(Slot::A)).unwrap(), b"old");
    assert!(!faber_engine::project::run::Runner::load(&journal).unwrap().promotion_pending);
    assert!(!faber_engine::project::run::Runner::load(&older_journal).unwrap().promotion_pending);
    assert!(!checkout.join(".faber/missions/m0").exists(), "coalescing wrote the other project's mirror into this checkout");
    assert!(!dev.service_landed_missions::<Gates>(&engine, true, &mut source, |_, _| panic!("request replayed")).unwrap());
    engine.shutdown();
}

#[test]
fn a_canary_that_stopped_waits_and_evidence_from_outside_the_run_fails() {
    use faber_engine::project::canary::{Evidence, Mode};
    let dir = area();
    let candidate = tree(b"candidate");
    let mut handshake = CanaryHandshake {
        nonce: "challenge".into(),
        receipt: "challenge".into(),
        build_hash: candidate.build_hash.clone(),
        schema: SCHEMA,
        verdict: CanaryVerdict::NotRun,
        evidence: None,
        stopped: Some("reason.budget.cap".into()),
    };
    // A mission that stopped on a wait is not a verdict on the build: it stays ready and the canary runs again.
    let (verdict, summary) = judge_canary(&handshake, dir.path(), &candidate, CanaryMode::Real);
    assert_eq!(verdict, CanaryVerdict::NotRun);
    assert_eq!(summary["wait"], "reason.budget.cap");
    // Evidence whose repository is not under the run's own work folder is a lie, whatever it claims.
    fs::create_dir_all(dir.path().join("work")).unwrap();
    fs::create_dir_all(dir.path().join("elsewhere")).unwrap();
    handshake.stopped = None;
    handshake.evidence = Some(Evidence {
        mode: Mode::Real,
        mission: "m1".into(),
        repo_root: dir.path().join("elsewhere").to_string_lossy().into_owned(),
        millis: 1,
    });
    let (verdict, _) = judge_canary(&handshake, dir.path(), &candidate, CanaryMode::Real);
    assert!(matches!(verdict, CanaryVerdict::Fail { .. }), "{verdict:?}");
    // A replay reported while a real run was requested is refused as well.
    handshake.evidence = Some(Evidence {
        mode: Mode::Offline,
        mission: "m1".into(),
        repo_root: dir.path().join("work").join("repo").to_string_lossy().into_owned(),
        millis: 1,
    });
    let (verdict, _) = judge_canary(&handshake, dir.path(), &candidate, CanaryMode::Real);
    assert!(matches!(verdict, CanaryVerdict::Fail { .. }), "{verdict:?}");
}

#[test]
fn a_real_canary_needs_an_allowance_that_covers_its_cost() {
    let dir = area();
    assert!(!real_canary_allowed(dir.path()));
    fs::write(dir.path().join("real-canary.json"), b"{\"capMicros\":1}").unwrap();
    assert!(!real_canary_allowed(dir.path()));
    let cap = faber_engine::project::canary::REAL_CAP_MICROS;
    fs::write(dir.path().join("real-canary.json"), format!("{{\"capMicros\":{}}}", cap - 1)).unwrap();
    assert!(!real_canary_allowed(dir.path()));
    fs::write(dir.path().join("real-canary.json"), format!("{{\"capMicros\":{cap}}}")).unwrap();
    assert!(real_canary_allowed(dir.path()));
    fs::write(dir.path().join("real-canary.json"), b"not json").unwrap();
    assert!(!real_canary_allowed(dir.path()));
}
