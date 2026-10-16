//! Live lifecycle proofs use only temporary development slots and the hidden test build.
use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::sandbox::{Launch, child_env};
use faber_engine::project::update::{CanaryVerdict, ExactTree, Slot, SlotLayout, SwapCheck, UpdateManager, UpdateState};
use faber_engine::project::{GitMode, Grant, RepoIdentity, SCHEMA};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How much faster the update clock runs in the probation scenario: the health window of 30 s becomes 6 s of real time.
const CLOCK_SPEED: u64 = 5;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "local_update: candidate starts hidden and returns a real canary handshake without inventing Pass", run: canary },
        Scenario { name: "T13 5: independent helper restores an early crash headlessly without changing saved grants", run: crash },
        Scenario { name: "T13 6: post-rollback Resume refuses unqualified dispatch without consuming the existing grant", run: resume },
        Scenario { name: "local_update: trusted launcher recovers before a broken candidate can enter main", run: interrupted },
        Scenario {
            name: "local_update: autostart uses the trusted launcher to recover a broken candidate without environment variables",
            run: autostart,
        },
        Scenario { name: "local_update: hidden session observer preserves next on orderly logoff or shutdown", run: session },
        Scenario { name: "T13 4: fixture promotion reaches Healthy; quiescence, probation and rollback remain headless", run: handoff },
    ]
}

/// Put the folders of git and cargo (and where the toolchain lives) into the candidate's environment.
fn canary_tools(env: &mut std::collections::BTreeMap<String, String>) -> Result<()> {
    let mut path = vec![];
    for tool in ["git", "cargo"] {
        let cli = faber_engine::cli::find_cli(tool).ok_or_else(|| format!("{tool} is not installed"))?;
        path.push(Path::new(&cli.command).parent().ok_or("a tool has no folder")?.to_path_buf());
    }
    if let Some(system) = std::env::var_os("SystemRoot") {
        path.push(std::path::PathBuf::from(system).join("System32"));
    }
    env.insert("PATH".into(), std::env::join_paths(path).map_err(|e| e.to_string())?.to_string_lossy().into());
    for name in ["RUSTUP_HOME", "CARGO_HOME", "ProgramData", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(value) = std::env::var(name) {
            env.insert(name.into(), value);
        }
    }
    Ok(())
}

fn canary(ctx: &mut Ctx) -> Result {
    let root = ctx.sb.sandbox.join("canary-dev");
    let data = root.join("data");
    let layout = SlotLayout::new(&root, &ctx.sb.data, &ctx.sb.release).map_err(|e| e.key())?;
    let manager = UpdateManager::new(layout).map_err(|e| e.key())?;
    let bytes = fs::read(&ctx.paths.app).map_err(|e| e.to_string())?;
    let candidate =
        ExactTree::new(&"1".repeat(40), &"2".repeat(40), &ExactTree::compute_build_hash(&bytes), SCHEMA).map_err(|e| e.key())?;
    let helper = ctx.paths.app.parent().ok_or("missing test build")?.join("faber-local.exe");
    let launcher = root.join("faber-local.exe");
    fs::copy(&helper, &launcher).map_err(|e| e.to_string())?;
    fs::copy(&ctx.paths.app, root.join("slots/a/faber.exe")).map_err(|e| e.to_string())?;
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    manager.adopt_baseline(Slot::A, candidate, &bytes, time).map_err(|e| e.key())?;
    let mut env = child_env(&std::env::vars().collect::<Vec<_>>(), &ctx.sb, &Default::default(), &ctx.paths.fake_cli);
    env.extend([
        ("FABER_CHANNEL".into(), "development".into()),
        ("FABER_DEV_ROOT".into(), root.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_DATA".into(), ctx.sb.data.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_RELEASE".into(), ctx.sb.release.to_string_lossy().into_owned()),
    ]);
    // The candidate runs a real miniature mission: it needs git and the Rust toolchain, nothing else from this computer.
    canary_tools(&mut env)?;
    let (host, _, _) =
        Owned::start(&launcher, &["--test-canary".into()], &env, &ctx.sb.sandbox, &ctx.paths.fake_cli).map_err(|e| e.to_string())?;
    let result = (|| {
        eventually("candidate canary receipt", Duration::from_secs(240), || !host.is_running())?;
        ensure(host.status().is_some_and(|s| s.contains('0')), || "candidate canary failed".into())?;
        let verdict: CanaryVerdict = serde_json::from_slice(&fs::read(data.join("test-canary-result.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        ensure(verdict == CanaryVerdict::NotRun, || {
            let found: Vec<String> = fs::read_dir(data.join("missions/canary"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| format!("{}: {}", e.file_name().to_string_lossy(), fs::read_to_string(e.path()).unwrap_or_default()))
                .collect();
            format!("the offline canary must stay a wait, got {verdict:?}; {found:?}")
        })?;
        let receipts = || -> std::result::Result<Vec<std::path::PathBuf>, String> {
            fs::read_dir(data.join("missions/canary"))
                .map_err(|e| e.to_string())?
                .map(|e| e.map(|e| e.path()).map_err(|e| e.to_string()))
                .collect()
        };
        let all = receipts()?;
        let receipt = all
            .iter()
            .find(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("canary-") && !n.to_string_lossy().starts_with("canary-verified-"))
            })
            .ok_or("missing private canary receipt")?
            .clone();
        let verified = all
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("canary-verified-")))
            .ok_or("the supervisor wrote no verified summary")?;
        let handshake: serde_json::Value =
            serde_json::from_slice(&fs::read(receipt).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        ensure(handshake["buildHash"] == ExactTree::compute_build_hash(&bytes) && handshake["nonce"] == handshake["receipt"], || {
            "wrong candidate handshake".into()
        })?;
        // The candidate ran its whole miniature mission on recorded replies: it is evidence, never a promotion.
        ensure(handshake["evidence"]["mode"] == "offline", || format!("the candidate did not report an offline mission: {handshake}"))?;
        let summary: serde_json::Value =
            serde_json::from_slice(&fs::read(verified).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        ensure(
            summary["mode"] == "offline"
                && summary["writer"] == "claude"
                && summary["reviewer"] == "codex"
                && summary["spentMicros"] == 0
                && summary["head"].as_str().is_some_and(|h| h.len() == 40),
            || format!("the supervisor did not verify the mission: {summary}"),
        )?;
        assert_hidden(ctx, &[&host], &env)
    })();
    host.terminate();
    ctx.notes.push(
        "candidate runs its own miniature mission on recorded replies (real git, cargo proof, other-vendor review, landing); supervisor reads its journal; offline evidence is never a Pass".into(),
    );
    result
}

fn crash(ctx: &mut Ctx) -> Result {
    rollback(ctx, false, false, false)
}
fn resume(ctx: &mut Ctx) -> Result {
    let data = ctx.sb.sandbox.join("development/data");
    fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let repo = ctx.sb.sandbox.join("resume-repo");
    fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    let mut project = faber_engine::model::Project::new("p1".into(), "Fixture".into(), repo.to_string_lossy().into_owned());
    project.repo = Some(RepoIdentity::new("repo", &repo.to_string_lossy()));
    project.grant = Some(Grant::new("repo", GitMode::Seguro, 1, 1));
    let store = faber_engine::store::Store::new(&data);
    store.save_projects(&[project]).map_err(|e| e.key())?;
    let mut ledger = faber_engine::ledger::Ledger::default();
    let mut budget = faber_engine::project::budget::Budget::default();
    budget.grants.insert(
        "m1".into(),
        faber_engine::project::budget::Grant {
            cap_micros: 2_000_000,
            worker_micros: 600_000,
            kind_caps: Default::default(),
            vendors: std::collections::BTreeMap::from([(faber_engine::model::Vendor::Codex, 2_000_000)]),
            review_micros: 400_000,
            repair_micros: 700_000,
            canary_micros: 0,
            paused: true,
            extra: Default::default(),
        },
    );
    ledger.extra.insert("missionBudget".into(), serde_json::to_value(budget).map_err(|e| e.to_string())?);
    store.save_ledger(&ledger).map_err(|e| e.key())?;
    let before = fs::read(data.join("budget.json")).map_err(|e| e.to_string())?;
    let journal = faber_engine::project::store::Journal::open(&repo, &data, "m1").map_err(|e| e.key())?;
    let mut runner = faber_engine::project::run::Runner::create(
        &journal,
        faber_engine::project::Mission::new("m1", "p1", "repo", "Fixture", &"1".repeat(40), 1, 1),
    )
    .map_err(str::to_string)?;
    runner.pause(&journal).map_err(str::to_string)?;
    rollback(ctx, false, false, false)?;
    let mut runner = faber_engine::project::run::Runner::load(&journal).map_err(str::to_string)?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let engine = faber_engine::Engine::new(
        faber_engine::EngineOptions {
            data_dir: data.clone(),
            clis: Default::default(),
            spawn: std::sync::Arc::new(|_, _, _| panic!("unqualified Resume must never spawn")),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(30),
            runtime: runtime.handle().clone(),
            ask: None,
        },
        std::sync::Arc::new(|_| {}),
    );
    for _ in 0..2 {
        ensure(runner.resume(&journal, &engine) == Err("reason.project.restrictionsUnproven".into()), || {
            "unqualified Resume admitted a role".into()
        })?;
        ensure(fs::read(data.join("budget.json")).map_err(|e| e.to_string())? == before, || {
            "refused Resume consumed or replaced the grant".into()
        })?;
    }
    engine.shutdown();
    ctx.notes
        .push("Resume admission remains blocked; repeated requests preserve the $2 fixture grant. Successful Resume is not proven.".into());
    Ok(())
}
fn interrupted(ctx: &mut Ctx) -> Result {
    rollback(ctx, true, false, false)
}
fn autostart(ctx: &mut Ctx) -> Result {
    rollback(ctx, true, false, true)
}
fn session(ctx: &mut Ctx) -> Result {
    rollback(ctx, false, true, false)
}
fn handoff(ctx: &mut Ctx) -> Result {
    let root = ctx.sb.sandbox.join("development");
    let stable_data = ctx.sb.sandbox.join("stable-data");
    let release = ctx.sb.sandbox.join("release");
    let manager = UpdateManager::new(SlotLayout::new(&root, &stable_data, &release).map_err(|e| e.key())?).map_err(|e| e.key())?;
    let payload = fs::read(&ctx.paths.app).map_err(|e| e.to_string())?;
    let helper = ctx.paths.target.join("debug/faber-local.exe");
    let helper_bytes = fs::read(&helper).map_err(|e| e.to_string())?;
    let launcher = root.join("faber-local.exe");
    fs::write(
        root.join("development.json"),
        serde_json::to_vec(&serde_json::json!({"channel": "development", "installedData": stable_data, "installedRelease": release}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(&launcher, &helper_bytes).map_err(|e| e.to_string())?;
    fs::write(root.join("helper.sha256"), ExactTree::compute_build_hash(&helper_bytes)).map_err(|e| e.to_string())?;
    let tree = ExactTree::new(&"1".repeat(40), &"2".repeat(40), &ExactTree::compute_build_hash(&payload), SCHEMA).map_err(|e| e.key())?;
    for slot in [Slot::A, Slot::B] {
        let dir = manager.layout().slot_dir(slot);
        fs::write(dir.join("faber.exe"), &payload).map_err(|e| e.to_string())?;
        fs::write(dir.join("faber-local.exe"), &helper_bytes).map_err(|e| e.to_string())?;
        fs::write(dir.join("helper.sha256"), ExactTree::compute_build_hash(&helper_bytes)).map_err(|e| e.to_string())?;
    }
    // The update flow runs its clock CLOCK_SPEED times faster from here (test builds only, R163), and this scenario writes
    // its own manifest changes on the same clock, so the 300-second probation and its 30-second health window are proven
    // whole in a fifth of the time.
    let real_ms = || u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap();
    let anchor = real_ms();
    let now = || anchor.saturating_add(real_ms().saturating_sub(anchor).saturating_mul(CLOCK_SPEED)) / 1000;
    manager.adopt_baseline(Slot::A, tree.clone(), &payload, now()).map_err(|e| e.key())?;
    let repo = RepoIdentity::new("repo", "checkout");
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    let ready = |tree: ExactTree| -> Result {
        let slot = manager.prepare_next(tree.clone(), &grant, &repo, 1, &[], now()).map_err(|e| e.key())?;
        // Reusing the obsolete slot must succeed while the healthy slot and its guard remain alive.
        fs::write(manager.layout().slot_dir(slot).join("faber.exe"), &payload).map_err(|e| e.to_string())?;
        fs::write(manager.layout().slot_dir(slot).join("faber-local.exe"), &helper_bytes).map_err(|e| e.to_string())?;
        manager
            .record_canary_verdict(
                CanaryVerdict::Pass {
                    verified_at: now(),
                    receipt_id: "fixture-canary".into(),
                    tree,
                    proof_receipt: "fixture-proof".into(),
                    compatible_schemas: vec![SCHEMA],
                },
                &payload,
                now(),
            )
            .map_err(|e| e.key().to_string())
    };
    ready(tree.clone())?;
    let data = root.join("data");
    fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let saved = b"{\"schema\":1,\"spent\":7,\"consumed\":true,\"extension\":\"keep\"}";
    fs::write(data.join("budget-sidecar.json"), saved).map_err(|e| e.to_string())?;
    let mut env = child_env(&std::env::vars().collect::<Vec<_>>(), &ctx.sb, &Launch::default(), &ctx.paths.fake_cli);
    for (key, value) in [
        ("FABER_CHANNEL", "development".into()),
        ("FABER_DEV_ROOT", root.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_DATA", stable_data.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_RELEASE", release.to_string_lossy().into_owned()),
    ] {
        env.insert(key.into(), value);
    }
    env.insert("FABER_TEST_CLOCK".into(), format!("{anchor}:{CLOCK_SPEED}"));
    let start = |arg: &str| {
        Owned::start(&launcher, &[arg.into()], &env, &ctx.sb.sandbox, &ctx.paths.fake_cli).map(|v| v.0).map_err(|e| e.to_string())
    };
    let mut saved_grant = Grant::new("repo", GitMode::Seguro, 1, now());
    saved_grant.extra.insert("consumed".into(), true.into());
    fs::write(data.join("grant.json"), serde_json::to_vec(&saved_grant).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let compatibility = start("--test-compatible")?;
    let compatible = eventually("old and next executable data round trips", Duration::from_secs(30), || !compatibility.is_running());
    let status = compatibility.status();
    compatibility.terminate();
    compatible?;
    ensure(status.is_some_and(|s| s.contains('0')), || "native compatibility rejected preserved saved data".into())?;
    let repo_dir = ctx.sb.sandbox.join("mission-repo");
    fs::create_dir_all(&repo_dir).map_err(|e| e.to_string())?;
    let mut project = faber_engine::model::Project::new("p1".into(), "Fixture".into(), repo_dir.to_string_lossy().into_owned());
    project.repo = Some(repo.clone());
    project.grant = Some(grant.clone());
    faber_engine::store::Store::new(&data).save_projects(&[project]).map_err(|e| e.key())?;
    let journal = faber_engine::project::store::Journal::open(&repo_dir, &data, "m1").map_err(|e| e.key())?;
    let mut runner = faber_engine::project::run::Runner::create(
        &journal,
        faber_engine::project::Mission::new("m1", "p1", "repo", "Fixture", &"1".repeat(40), 1, now()),
    )
    .map_err(str::to_string)?;
    runner.mission.state = faber_engine::project::MissionState::Running;
    runner.save(&journal).map_err(str::to_string)?;
    let rejected = start("--test-transition")?;
    let blocked = eventually("running mission refuses transition", Duration::from_secs(10), || !rejected.is_running());
    let status = rejected.status();
    rejected.terminate();
    blocked?;
    ensure(status.is_some_and(|s| s.contains('1')), || "running mission admitted promotion".into())?;
    let manifest = manager.manifest().map_err(|e| e.key())?.ok_or("missing ready build")?;
    ensure(matches!(manifest.state, UpdateState::Ready { .. }), || "refused promotion changed build slots".into())?;
    ensure(manifest.current_good.as_ref().is_some_and(|g| g.slot == Slot::A), || "current slot must be Slot::A".into())?;
    ensure(manifest.staged_tree.is_some(), || "next staged build slot must be present".into())?;
    ensure(manifest.last_good.as_ref().is_some_and(|g| g.slot == Slot::A), || "last good slot is Slot::A".into())?;
    let baseline = start("--launch")?;
    let paused = eventually("old engine services pause before promotion", Duration::from_secs(15), || {
        faber_engine::project::run::Runner::load(&journal).is_ok_and(|r| r.mission.state == faber_engine::project::MissionState::Paused)
    });
    if let Err(error) = paused {
        baseline.terminate();
        return Err(error);
    }
    let first = start("--test-transition")?;
    let result = (|| {
        eventually("first probation health", Duration::from_secs(30), || {
            manager.manifest().is_ok_and(|m| m.is_some_and(|m| m.first_operation_completed))
        })?;
        fs::write(data.join("test-mission-admission"), b"once").map_err(|e| e.to_string())?;
        eventually("probation's real engine refuses new mission admission", Duration::from_secs(10), || {
            fs::read_to_string(data.join("test-mission-admission-result")).is_ok_and(|result| result == "error.update.activeOperations")
        })?;
        fs::remove_file(data.join("test-mission-admission-result")).map_err(|e| e.to_string())?;
        assert_hidden(ctx, &[&first], &env)?;
        fs::write(data.join("test-clean-exit-next"), b"once").map_err(|e| e.to_string())?;
        eventually("clean quit pauses without restarting", Duration::from_secs(10), || {
            manager.manifest().is_ok_and(|m| m.is_some_and(|m| m.extra.get("cleanExit") == Some(&serde_json::Value::Bool(true))))
        })?;
        let paused = manager.manifest().map_err(|e| e.key())?.ok_or("missing pause")?;
        ensure(paused.rejected_build_hashes.is_empty() && root.join("slots/b/faber.exe").exists(), || {
            "clean quit quarantined candidate".into()
        })?;
        let lock = fs::OpenOptions::new().read(true).write(true).open(root.join("slots/a/running.lock")).map_err(|e| e.to_string())?;
        ensure(lock.try_lock().is_ok(), || "Quit relaunched last good".into())?;
        drop(lock);
        let resumed = start_autostart(ctx, &launcher, &env)?;
        let result = (|| {
            eventually("resumed full probation", Duration::from_secs(30), || {
                manager.manifest().is_ok_and(|m| m.is_some_and(|m| matches!(m.state, UpdateState::Probation { .. })))
            })?;
            fs::create_dir_all(root.join("staging/build")).map_err(|e| e.to_string())?;
            fs::write(root.join("staging/build/.faber-owned"), b"faber-local-build-v1").map_err(|e| e.to_string())?;
            let output = root.join("staging/build/output.obj");
            fs::write(&output, b"cleanup must not kill healthy next").map_err(|e| e.to_string())?;
            use std::os::windows::fs::OpenOptionsExt;
            let held = fs::OpenOptions::new().read(true).share_mode(1).open(&output).map_err(|e| e.to_string())?;
            eventually(
                "five minute promotion on the test clock despite failed cleanup",
                Duration::from_secs(320 / CLOCK_SPEED + 20),
                || manager.manifest().is_ok_and(|m| m.is_some_and(|m| m.state == UpdateState::Healthy)),
            )?;
            let healthy = manager.manifest().map_err(|e| e.key())?.ok_or("missing healthy manifest")?;
            ensure(
                healthy.active_slot == Slot::B
                    && healthy.current_good.as_ref().is_some_and(|good| good.slot == Slot::B && good.tree == tree)
                    && healthy.last_good.as_ref().is_some_and(|good| good.slot == Slot::A),
                || "promotion lost the exact candidate or last good slot".into(),
            )?;
            eventually("supervisor exits after healthy handoff", Duration::from_secs(10), || !resumed.is_running())?;
            eventually("previous helper image is retired before slot reuse", Duration::from_secs(10), || {
                fs::OpenOptions::new().write(true).open(root.join("slots/a/faber-local.exe")).is_ok()
            })?;
            assert_hidden(ctx, &[&first, &resumed], &env)?;
            let lease = fs::OpenOptions::new().read(true).write(true).open(root.join("slots/b/running.lock")).map_err(|e| e.to_string())?;
            ensure(lease.try_lock().is_err(), || "healthy handoff killed next".into())?;
            drop(lease);
            drop(held);
            ensure(fs::read(&launcher).map_err(|e| e.to_string())? == helper_bytes, || "enrolled launcher changed".into())?;
            ready(ExactTree::new(&"3".repeat(40), &"4".repeat(40), &tree.build_hash, SCHEMA).map_err(|e| e.key())?)?;
            fs::write(data.join("test-crash-next"), b"once").map_err(|e| e.to_string())?;
            let second = start("--test-transition")?;
            let result = (|| {
                eventually("second transition succeeds while B is running", Duration::from_secs(10), || {
                    manager.manifest().is_ok_and(|m| m.is_some_and(|m| matches!(m.state, UpdateState::Swapping { .. })))
                })?;
                eventually("second promotion early crash restores B", Duration::from_secs(40), || {
                    manager
                        .manifest()
                        .is_ok_and(|m| m.is_some_and(|m| matches!(m.state, UpdateState::RolledBack { restored_slot: Slot::B, .. })))
                })?;
                eventually("deferred owned cleanup retries automatically", Duration::from_secs(30), || {
                    !root.join("staging/build").exists()
                })?;
                eventually("B is relaunched after the second transition", Duration::from_secs(30), || {
                    fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(root.join("slots/b/running.lock"))
                        .is_ok_and(|file| file.try_lock().is_err())
                })?;
                ensure(fs::read(root.join("slots/b/faber.exe")).map_err(|e| e.to_string())? == payload, || {
                    "second transition lost last good B".into()
                })?;
                ensure(fs::read(data.join("budget-sidecar.json")).map_err(|e| e.to_string())? == saved, || {
                    "probation changed saved data".into()
                })?;
                ensure(
                    !fs::read_dir(&root)
                        .map_err(|e| e.to_string())?
                        .any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with(".faber-patch-"))),
                    || "helper rotation leaked a payload".into(),
                )?;
                for owned in [&first, &resumed, &second] {
                    if let Some(ids) = owned.members() {
                        ensure(!crate::desktop::windows()?.iter().any(|(_, pid, visible)| *visible && ids.contains(pid)), || {
                            "promotion opened a visible window".into()
                        })?;
                    }
                }
                Ok(())
            })();
            second.terminate();
            result
        })();
        resumed.terminate();
        result
    })();
    first.terminate();
    baseline.terminate();
    if result.is_ok() {
        ctx.notes.push("running mission refuses transition; old engine pauses through T5 stop; probation refuses admission; clean quit without rollback/relaunch; full 300 s probation on a clock five times faster; cleanup failure cannot kill healthy child; second transition while B lives; immutable launcher; saved spend/grants unchanged".into());
    }
    result
}
fn rollback(ctx: &mut Ctx, interrupted: bool, session: bool, autostart: bool) -> Result {
    let root = ctx.sb.sandbox.join("development");
    let release = ctx.sb.sandbox.join("release");
    let stable_data = ctx.sb.sandbox.join("stable-data");
    let manager = UpdateManager::new(SlotLayout::new(&root, &stable_data, &release).map_err(|e| e.key())?).map_err(|e| e.key())?;
    let payload = fs::read(&ctx.paths.app).map_err(|e| e.to_string())?;
    let tree = ExactTree::new(&"1".repeat(40), &"2".repeat(40), &ExactTree::compute_build_hash(&payload), SCHEMA).map_err(|e| e.key())?;
    for slot in [Slot::A, Slot::B] {
        fs::write(manager.layout().slot_dir(slot).join("faber.exe"), &payload).map_err(|e| e.to_string())?;
    }
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    manager.adopt_baseline(Slot::A, tree.clone(), &payload, time).map_err(|e| e.key())?;
    let last_good = manager.manifest().map_err(|e| e.key())?.ok_or("missing manifest")?.last_good;
    let repo = RepoIdentity::new("repo", "checkout");
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, time);
    grant.self_update = true;
    manager.prepare_next(tree.clone(), &grant, &repo, 1, &[], time).map_err(|e| e.key())?;
    manager
        .record_canary_verdict(
            CanaryVerdict::Pass {
                verified_at: time,
                receipt_id: "fixture-canary".into(),
                tree,
                proof_receipt: "fixture-proof".into(),
                compatible_schemas: vec![SCHEMA],
            },
            &payload,
            time,
        )
        .map_err(|e| e.key())?;
    manager.enter_quiescence(false, false).map_err(|e| e.key())?;
    manager
        .begin_swap(&grant, &repo, 1, SwapCheck { payload: &payload, active_window: false, active_operations: false }, time)
        .map_err(|e| e.key())?;
    let data = root.join("data");
    fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let saved = b"{\"schema\":1,\"spent\":7,\"consumed\":true,\"extension\":\"keep\"}";
    fs::write(data.join("mission.json"), saved).map_err(|e| e.to_string())?;
    if !interrupted && !session {
        fs::write(data.join("test-crash-next"), b"once").map_err(|e| e.to_string())?;
    }
    let mut env = child_env(&std::env::vars().collect::<Vec<_>>(), &ctx.sb, &Launch::default(), &ctx.paths.fake_cli);
    for (key, value) in [
        ("FABER_CHANNEL", "development".into()),
        ("FABER_DEV_ROOT", root.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_DATA", stable_data.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_RELEASE", release.to_string_lossy().into_owned()),
    ] {
        env.insert(key.into(), value);
    }
    let built_helper = ctx.paths.target.join("debug/faber-local.exe");
    let helper = root.join("faber-local.exe");
    let helper_bytes = fs::read(&built_helper).map_err(|e| e.to_string())?;
    fs::write(&helper, &helper_bytes).map_err(|e| e.to_string())?;
    fs::write(root.join("helper.sha256"), ExactTree::compute_build_hash(&helper_bytes)).map_err(|e| e.to_string())?;
    fs::write(
        root.join("development.json"),
        serde_json::to_vec(&serde_json::json!({"channel": "development", "installedData": stable_data, "installedRelease": release}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if interrupted && !autostart {
        for key in ["FABER_CHANNEL", "FABER_DEV_ROOT", "FABER_INSTALLED_DATA", "FABER_INSTALLED_RELEASE"] {
            env.remove(key);
        }
    }
    if interrupted {
        manager.begin_probation(time).map_err(|e| e.key())?;
        // This candidate exits before startup_lease can delegate recovery. The entry must never execute it.
        fs::write(root.join("slots/b/faber.exe"), b"not a runnable executable").map_err(|e| e.to_string())?;
    }
    for path in [&helper, &ctx.paths.app] {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let offset =
            bytes.get(60..64).and_then(|bytes| <[u8; 4]>::try_from(bytes).ok()).map(u32::from_le_bytes).ok_or("missing PE header")?
                as usize;
        let subsystem =
            bytes.get(offset + 24 + 68..offset + 24 + 70).and_then(|bytes| <[u8; 2]>::try_from(bytes).ok()).map(u16::from_le_bytes);
        ensure(subsystem == Some(2), || format!("{} has a visible console subsystem", path.display()))?;
    }
    let owned = if autostart {
        start_autostart(ctx, &helper, &env)?
    } else {
        Owned::start(&helper, &[if interrupted { "--launch" } else { "--transition" }.into()], &env, &ctx.sb.sandbox, &ctx.paths.fake_cli)
            .map_err(|e| e.to_string())?
            .0
    };
    let result = (|| {
        if session {
            eventually("session observer's supervised next", Duration::from_secs(30), || {
                manager.manifest().is_ok_and(|m| m.is_some_and(|m| m.first_operation_completed))
            })?;
            assert_hidden(ctx, &[&owned], &env)?;
            fs::write(data.join("test-session-end"), b"false").map_err(|e| e.to_string())?;
            eventually("cancelled shutdown leaves probation running", Duration::from_secs(10), || !data.join("test-session-end").exists())?;
            ensure(
                matches!(manager.manifest().map_err(|e| e.key())?.ok_or("missing probation")?.state, UpdateState::Probation { .. }),
                || "cancelled shutdown changed probation".into(),
            )?;
            fs::write(data.join("test-session-end"), b"true").map_err(|e| e.to_string())?;
            eventually("orderly shutdown parks without rejection", Duration::from_secs(10), || {
                manager.manifest().is_ok_and(|m| m.is_some_and(|m| m.extra.get("cleanExit") == Some(&serde_json::Value::Bool(true))))
            })?;
            eventually("session helper stops without relaunch", Duration::from_secs(10), || !owned.is_running())?;
            let manifest = manager.manifest().map_err(|e| e.key())?.ok_or("missing pause")?;
            ensure(manifest.rejected_build_hashes.is_empty() && manifest.last_good == last_good, || {
                "session end changed last good or rejected next".into()
            })?;
            ensure(fs::read(root.join("slots/b/faber.exe")).map_err(|e| e.to_string())? == payload, || "session end deleted next".into())?;
            ensure(fs::read(data.join("mission.json")).map_err(|e| e.to_string())? == saved, || {
                "session end changed spend or grants".into()
            })?;
            ctx.notes.push("hidden native session observer; cancelled shutdown ignored; orderly ending retries full probation; no candidate rejection or last-good relaunch".into());
            return Ok(());
        }
        eventually("independent rollback", Duration::from_secs(30), || {
            manager.manifest().is_ok_and(|m| m.is_some_and(|m| matches!(m.state, UpdateState::RolledBack { .. })))
        })?;
        let manifest = manager.manifest().map_err(|e| e.key())?.ok_or("missing rollback")?;
        ensure(manifest.active_slot == Slot::A && manifest.rollback_count == 1 && manifest.last_good == last_good, || {
            "rollback changed last good".into()
        })?;
        eventually("bad slot cleanup", Duration::from_secs(10), || !root.join("slots/b/faber.exe").exists())?;
        eventually("restored slot running headlessly", Duration::from_secs(30), || {
            let path = root.join("slots/a/running.lock");
            std::fs::OpenOptions::new().read(true).write(true).open(path).is_ok_and(|file| file.try_lock().is_err())
        })?;
        ensure(fs::read(data.join("mission.json")).map_err(|e| e.to_string())? == saved, || {
            "rollback reset spend or dropped fields".into()
        })?;
        ensure(fs::read(root.join("slots/a/faber.exe")).map_err(|e| e.to_string())? == payload, || "last-good executable changed".into())?;
        if autostart {
            let mut login_env = env.clone();
            for key in ["FABER_CHANNEL", "FABER_DEV_ROOT", "FABER_INSTALLED_DATA", "FABER_INSTALLED_RELEASE", "FABER_DATA_DIR"] {
                login_env.remove(key);
            }
            let legacy =
                Owned::start(&root.join("slots/a/faber.exe"), &["--autostart".into()], &login_env, &ctx.sb.sandbox, &ctx.paths.fake_cli)
                    .map_err(|e| e.to_string())?
                    .0;
            let result =
                eventually("legacy slot autostart delegates before GUI or slot lease", Duration::from_secs(10), || !legacy.is_running());
            assert_hidden(ctx, &[&owned, &legacy], &env)?;
            legacy.terminate();
            result?;
            ensure(legacy.status().is_some_and(|status| status.contains('0')), || "legacy autostart failed delegation".into())?;
        }
        // The helper has no Tauri runtime. Its restored engine has no WebView or console, even on the default CI desktop.
        if let Some(ids) = owned.members() {
            let windows = crate::desktop::windows()?;
            ensure(!windows.iter().any(|(_, pid, visible)| *visible && ids.contains(pid)), || {
                "local transition opened a visible window".into()
            })?;
        }
        assert_hidden(ctx, &[&owned], &env)?;
        ctx.notes.push("temporary slots; hidden test build; one rollback; immutable payload; saved spend/grants/extensions unchanged; no installer or visible window".into());
        Ok(())
    })();
    owned.terminate();
    result
}

fn start_autostart(ctx: &Ctx, helper: &std::path::Path, env: &std::collections::BTreeMap<String, String>) -> Result<Owned> {
    let registration =
        Owned::start(helper, &["--test-autostart-command".into()], env, &ctx.sb.sandbox, &ctx.paths.fake_cli).map_err(|e| e.to_string())?.0;
    let result = eventually("sandboxed autostart command", Duration::from_secs(10), || !registration.is_running());
    registration.terminate();
    result?;
    let root = helper.parent().ok_or("missing launcher root")?;
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("data/test-autostart-command.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    fs::remove_file(root.join("data/test-autostart-command.json")).map_err(|e| e.to_string())?;
    let program = std::path::PathBuf::from(report["program"].as_str().ok_or("missing autostart executable")?);
    let args = report["args"]
        .as_array()
        .ok_or("missing autostart arguments")?
        .iter()
        .map(|arg| arg.as_str().map(String::from).ok_or("invalid autostart argument".into()))
        .collect::<Result<Vec<_>>>()?;
    let mut login_env = env.clone();
    for key in ["FABER_CHANNEL", "FABER_DEV_ROOT", "FABER_INSTALLED_DATA", "FABER_INSTALLED_RELEASE", "FABER_DATA_DIR"] {
        login_env.remove(key);
    }
    Owned::start(&program, &args, &login_env, &ctx.sb.sandbox, &ctx.paths.fake_cli).map(|v| v.0).map_err(|e| e.to_string())
}

fn assert_hidden(ctx: &Ctx, jobs: &[&Owned], env: &std::collections::BTreeMap<String, String>) -> Result {
    let (probe, _, stderr) =
        Owned::start(&ctx.paths.fake_cli, &["--desktop-visible-probe".into()], env, &ctx.sb.sandbox, &ctx.paths.fake_cli)
            .map_err(|e| e.to_string())?;
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        }
        let _ = send.send(text);
    });
    let windows = receive.recv_timeout(Duration::from_secs(10));
    probe.terminate();
    let windows = windows.map_err(|_| "visible desktop probe timed out")?;
    let members: Vec<_> = jobs.iter().flat_map(|job| job.members().unwrap_or_default()).collect();
    let rows = windows.lines().next().and_then(|line| line.strip_prefix("windows")).ok_or("invalid desktop probe")?;
    let visible = rows
        .split(',')
        .filter(|row| !row.trim().is_empty())
        .map(|row| row.trim().split_once(':').and_then(|(_, pid)| pid.parse::<u32>().ok()).ok_or("invalid visible-window owner"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure(!visible.iter().any(|pid| members.contains(pid)), || {
        format!(
            "transition created visible windows on its own desktop: {}; owned {:?}; processes {:?}",
            windows.trim(),
            members,
            crate::proctree::snapshot().map(|rows| rows.into_iter().filter(|row| members.contains(&row.pid)).collect::<Vec<_>>())
        )
    })?;
    Ok(())
}
