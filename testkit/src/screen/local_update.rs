//! Live lifecycle proofs use only temporary development slots and the hidden test build.
use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::sandbox::{Launch, child_env};
use faber_engine::project::update::{CanaryVerdict, ExactTree, Slot, SlotLayout, SwapCheck, UpdateManager, UpdateState};
use faber_engine::project::{GitMode, Grant, RepoIdentity, SCHEMA};
use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "local_update: independent helper restores an early crash headlessly without changing saved grants", run: crash },
        Scenario { name: "local_update: trusted launcher recovers before a broken candidate can enter main", run: interrupted },
        Scenario {
            name: "local_update: autostart uses the trusted launcher to recover a broken candidate without environment variables",
            run: autostart,
        },
        Scenario { name: "local_update: hidden session observer preserves next on orderly logoff or shutdown", run: session },
        Scenario {
            name: "local_update: running mission blocks promotion until paused; autostart probation and rollback remain headless",
            run: handoff,
        },
    ]
}

fn crash(ctx: &mut Ctx) -> Result {
    rollback(ctx, false, false, false)
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
    let now = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
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
    ensure(matches!(manager.manifest().map_err(|e| e.key())?.ok_or("missing ready build")?.state, UpdateState::Ready { .. }), || {
        "refused promotion changed build slots".into()
    })?;
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
            eventually("five minute wall-clock promotion despite failed cleanup", Duration::from_secs(320), || {
                manager.manifest().is_ok_and(|m| m.is_some_and(|m| m.state == UpdateState::Healthy))
            })?;
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
        ctx.notes.push("running mission refuses transition; old engine pauses through T5 stop; probation refuses admission; clean quit without rollback/relaunch; full 300 s probation; cleanup failure cannot kill healthy child; second transition while B lives; immutable launcher; saved spend/grants unchanged".into());
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
