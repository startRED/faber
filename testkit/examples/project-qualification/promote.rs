//! T13 promotion: enrol a baseline in a development root, start it on the hidden desktop and watch it turn a landed
//! mission into the running build. This file only lays out the root and observes; the build, the canary, the swap and
//! the probation are the product's own code inside the baseline and its helper.
use faber_engine::project::SCHEMA;
use faber_engine::project::update::{ExactTree, Slot, SlotLayout, UpdateManager, UpdateState};
use faber_testkit::contain::Owned;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn fail<T: std::fmt::Display>(error: T) -> io::Error {
    io::Error::other(error.to_string())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Where the development root lives and which binaries seed it.
pub struct Dev {
    pub root: PathBuf,
    installed_data: PathBuf,
    installed_release: PathBuf,
    app: PathBuf,
    helper: PathBuf,
}

impl Dev {
    /// `FABER_T13_DEVROOT` is a folder next to which the stand-ins for the installed app are made. They are empty
    /// and unrelated to the owner's installation.
    pub fn from_env() -> io::Result<Self> {
        let var = |name: &str| std::env::var(name).map(PathBuf::from).map_err(|_| fail(format!("{name} is not set")));
        Self::at(&var("FABER_T13_DEVROOT")?, var("FABER_T13_APP")?, var("FABER_T13_HELPER")?)
    }

    pub fn at(base: &Path, app: PathBuf, helper: PathBuf) -> io::Result<Self> {
        let installed_data = base.join("installed-data");
        let installed_release = base.join("installed-release");
        for dir in [base, &installed_data, &installed_release, &base.join("dev")] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(Self { root: base.join("dev"), installed_data, installed_release, app, helper })
    }

    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    pub fn manager(&self) -> io::Result<UpdateManager> {
        UpdateManager::new(SlotLayout::new(&self.root, &self.installed_data, &self.installed_release).map_err(|e| fail(e.key()))?)
            .map_err(|e| fail(e.key()))
    }

    /// Adopts the baseline build as the running slot, as `Development::adopt` does. Safe to repeat.
    pub fn enroll(&self, commit: &str, tree: &str) -> io::Result<()> {
        let manager = self.manager()?;
        if manager.manifest().map_err(|e| fail(e.key()))?.is_some() {
            return Ok(());
        }
        let payload = std::fs::read(&self.app)?;
        let helper = std::fs::read(&self.helper)?;
        let build = ExactTree::compute_build_hash(&payload);
        let helper_hash = ExactTree::compute_build_hash(&helper);
        let tree = ExactTree::new(commit, tree, &build, SCHEMA).map_err(|e| fail(e.key()))?;
        std::fs::write(
            self.root.join("development.json"),
            serde_json::to_vec(
                &json!({"channel": "development", "installedData": self.installed_data, "installedRelease": self.installed_release}),
            )
            .map_err(fail)?,
        )?;
        let dir = manager.layout().slot_dir(Slot::A);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("faber.exe"), &payload)?;
        std::fs::write(self.root.join("faber-local.exe"), &helper)?;
        std::fs::write(self.root.join("helper.sha256"), &helper_hash)?;
        std::fs::create_dir_all(self.data())?;
        // The one allowance for spending on the candidate's own canary: a person put it here. Without this file the
        // candidate would only replay its mission and wait.
        std::fs::write(
            self.data().join("real-canary.json"),
            format!("{{\"capMicros\":{}}}", faber_engine::project::canary::REAL_CAP_MICROS).as_bytes(),
        )?;
        manager.adopt_baseline(Slot::A, tree, &payload, now()).map_err(|e| fail(e.key()))
    }
}

fn state_name(state: &UpdateState) -> &'static str {
    match state {
        UpdateState::Idle => "idle",
        UpdateState::PreparingNext { .. } => "preparingNext",
        UpdateState::CanaryPending { .. } => "canaryPending",
        UpdateState::Ready { .. } => "ready",
        UpdateState::Quiescent => "quiescent",
        UpdateState::Swapping { .. } => "swapping",
        UpdateState::Probation { .. } => "probation",
        UpdateState::Healthy => "healthy",
        UpdateState::RolledBack { .. } => "rolledBack",
        UpdateState::Quarantined { .. } => "quarantined",
    }
}

fn lock_held(path: &Path) -> bool {
    std::fs::OpenOptions::new().read(true).write(true).open(path).is_ok_and(|file| file.try_lock().is_err())
}

/// Start the enrolled baseline and follow it until the landed build is healthy, or it fails, or time runs out.
/// `landed` is (commit, tree) the mission produced.
pub fn observe(dev: &Dev, env: &BTreeMap<String, String>, guard: &Path, landed: (&str, &str), budget: Duration) -> io::Result<Value> {
    let manager = dev.manager()?;
    let launcher = dev.root.join("faber-local.exe");
    let started = Instant::now();
    let (owned, _, stderr) = Owned::start(&launcher, &["--launch".into()], env, &dev.root, guard)?;
    drop(stderr);
    let mut timeline: Vec<Value> = vec![];
    let mut last = String::new();
    let mut peak = (0.0_f64, 0_u64, String::new());
    let mut next_sample = Instant::now();
    let mut verdict = json!({"outcome": "timeout"});
    while started.elapsed() < budget {
        if let Ok(Some(manifest)) = manager.manifest() {
            let name = format!("{}/{:?}", state_name(&manifest.state), manifest.active_slot);
            if name != last {
                eprintln!("t13: {name} at {}s", started.elapsed().as_secs());
                timeline.push(json!({"atSecs": started.elapsed().as_secs(), "state": name}));
                last = name;
            }
            match &manifest.state {
                UpdateState::Healthy if manifest.active_slot == Slot::B => {
                    verdict = json!({"outcome": "healthy"});
                    break;
                }
                UpdateState::RolledBack { .. } | UpdateState::Quarantined { .. } => {
                    verdict = json!({"outcome": state_name(&manifest.state), "state": serde_json::to_value(&manifest.state).unwrap_or(Value::Null)});
                    break;
                }
                _ => {}
            }
        }
        if Instant::now() >= next_sample {
            if let Ok(mb) = faber_testkit::proctree::private_mb(owned.pid())
                && mb > peak.0
            {
                peak = (mb, started.elapsed().as_secs(), last.clone());
            }
            next_sample = Instant::now() + Duration::from_secs(15);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let mut report = json!({"verdict": verdict, "timeline": timeline, "elapsedSecs": started.elapsed().as_secs(),
        "peakTreePrivateMb": {"mb": peak.0, "atSecs": peak.1, "state": peak.2}});
    if report["verdict"]["outcome"] == "healthy" {
        report["running"] = check_running(dev, &manager, landed)?;
        // A short idle reading of what is left running once the promotion is over.
        std::thread::sleep(Duration::from_secs(20));
        let idle = faber_testkit::proctree::private_mb(owned.pid()).ok();
        report["idleTreePrivateMb"] = json!(idle);
    }
    owned.terminate();
    Ok(report)
}

/// The accepted revision must be the one that runs: the manifest's good record, the slot's bytes, the image's own
/// identity and the slot's running lock all name the landed commit and tree.
fn check_running(dev: &Dev, manager: &UpdateManager, landed: (&str, &str)) -> io::Result<Value> {
    let manifest = manager.manifest().map_err(|e| fail(e.key()))?.ok_or_else(|| fail("no manifest"))?;
    let good = manifest.current_good.ok_or_else(|| fail("no good record"))?;
    let slot = manager.layout().slot_dir(good.slot);
    let bytes = std::fs::read(slot.join("faber.exe"))?;
    let hash = ExactTree::compute_build_hash(&bytes);
    let output = dev.root.join("running-identity.json");
    let _ = std::fs::remove_file(&output);
    let status = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(slot.join("faber.exe"))
            .arg("--faber-local-identity")
            .arg(&output)
            .env_clear()
            .creation_flags(faber_testkit::contain::NO_WINDOW)
            .status()?
    };
    let identity: Value = std::fs::read(&output).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    let lock = lock_held(&slot.join("running.lock"));
    let ok = good.slot == Slot::B
        && manifest.active_slot == Slot::B
        && good.tree.commit_oid == landed.0
        && good.tree.tree_oid == landed.1
        && good.tree.build_hash == hash
        && identity["buildHash"] == json!(hash)
        && status.success()
        && lock;
    Ok(json!({"ok": ok, "slot": format!("{:?}", good.slot), "commit": good.tree.commit_oid, "tree": good.tree.tree_oid,
        "buildHash": hash, "identityBuildHash": identity["buildHash"], "runningLockHeld": lock,
        "lastGood": manifest.last_good.map(|l| format!("{:?}", l.slot)),
        "landedCommit": landed.0, "landedTree": landed.1}))
}

/// The rollback drill. A second development root gets the real baseline build as its running slot and, as the next
/// slot, an executable that cannot start (the fake CLI used as the guard). The helper of a test build (which has the hooks to ask for a transition) swaps
/// to it; the product's own supervisor must notice, restore the baseline and start it again. Only the candidate and
/// its recorded verdict are stand-ins: this checks recovery, not the canary.
pub fn drill(
    base: &Path,
    app: &Path,
    test_helper: &Path,
    commit: &str,
    tree: &str,
    env: &BTreeMap<String, String>,
    guard: &Path,
) -> io::Result<Value> {
    use faber_engine::project::update::CanaryVerdict;
    use faber_engine::project::{GitMode, Grant, RepoIdentity};
    let dev = Dev::at(base, app.to_path_buf(), test_helper.to_path_buf())?;
    dev.enroll(commit, tree)?;
    let manager = dev.manager()?;
    let payload = std::fs::read(guard)?;
    let helper = std::fs::read(test_helper)?;
    let candidate =
        ExactTree::new(&"3".repeat(40), &"4".repeat(40), &ExactTree::compute_build_hash(&payload), SCHEMA).map_err(|e| fail(e.key()))?;
    let repo = RepoIdentity::new("repo", "checkout");
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, now());
    grant.self_update = true;
    let slot = manager.prepare_next(candidate.clone(), &grant, &repo, 1, &[], now()).map_err(|e| fail(e.key()))?;
    let dir = manager.layout().slot_dir(slot);
    std::fs::write(dir.join("faber.exe"), &payload)?;
    std::fs::write(dir.join("faber-local.exe"), &helper)?;
    std::fs::write(dir.join("helper.sha256"), ExactTree::compute_build_hash(&helper))?;
    manager
        .record_canary_verdict(
            CanaryVerdict::Pass {
                verified_at: now(),
                receipt_id: "drill-verdict".into(),
                tree: candidate,
                proof_receipt: "drill-proof".into(),
                compatible_schemas: vec![SCHEMA],
            },
            &payload,
            now(),
        )
        .map_err(|e| fail(e.key().to_string()))?;
    let launcher = dev.root.join("faber-local.exe");
    let started = Instant::now();
    let (baseline, _, e1) = Owned::start(&launcher, &["--launch".into()], env, &dev.root, guard)?;
    drop(e1);
    let a_lock = dev.root.join("slots/a/running.lock");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !lock_held(&a_lock) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }
    let running_before = lock_held(&a_lock);
    let (swap, _, e2) = Owned::start(&launcher, &["--test-transition".into()], env, &dev.root, guard)?;
    drop(e2);
    let mut timeline: Vec<Value> = vec![];
    let mut last = String::new();
    let mut outcome = "timeout".to_string();
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        if let Ok(Some(manifest)) = manager.manifest() {
            let name = format!("{}/{:?}", state_name(&manifest.state), manifest.active_slot);
            if name != last {
                timeline.push(json!({"atSecs": started.elapsed().as_secs(), "state": name}));
                last = name;
            }
            if let UpdateState::RolledBack { restored_slot, .. } = &manifest.state {
                outcome = format!("rolledBack/{restored_slot:?}");
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    // The restored baseline must be running again, and the failed candidate must not be the good one.
    let relaunch_deadline = Instant::now() + Duration::from_secs(60);
    let mut relaunched = false;
    while Instant::now() < relaunch_deadline {
        // The lock is held by the restored process; the first baseline has exited by now.
        if !baseline.is_running() && lock_held(&a_lock) {
            relaunched = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let manifest = manager.manifest().map_err(|e| fail(e.key()))?;
    let good_slot = manifest.as_ref().and_then(|m| m.current_good.as_ref()).map(|g| format!("{:?}", g.slot));
    let rejected = manifest.as_ref().map_or(0, |m| m.rejected_build_hashes.len() + m.rejected_trees.len());
    swap.terminate();
    baseline.terminate();
    Ok(json!({"outcome": outcome, "baselineRanBefore": running_before, "baselineRunningAgain": relaunched, "goodSlot": good_slot,
        "rejectedCandidates": rejected, "timeline": timeline, "elapsedSecs": started.elapsed().as_secs(),
        "note": "recovery drill: the candidate is an executable that cannot start and its canary verdict is recorded by the drill; it does not stand for a canary result"}))
}
