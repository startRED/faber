//! T13 run: one small real Faber goal, taken from its text to a landing by the production coordinator.
//!
//! This harness only builds the setup and starts and observes the mission. Every model turn, spend, lease, proof,
//! review and landing is the coordinator's own work through the engine's calls (`Coordinator::new(engine, ...)`);
//! nothing here plays a role. What it prepares is the world: a fresh clone of Faber whose base carries one staged
//! regression (a failing unit test), a project with a Seguro grant that also trusts self-update, and the single-use
//! R132 waiver of this harness. The landed mission is left in the engine's data folder for the development app to
//! build, canary and promote (`promote.rs`).
use faber_engine::engine::{Engine, EngineOptions};
use faber_engine::model::Vendor;
use faber_engine::project::Lease;
use faber_engine::project::budget::Budget;
use faber_engine::project::budget::Invocation;
use faber_engine::project::canary::{caps, cargo_proof};
use faber_engine::project::coordinator::{Coordinator, Outcome, Pins, Roles, Setup, Stop};
use faber_engine::project::git::{Broker, Process, enroll};
use faber_engine::project::lease::{Leases, Termination};
use faber_engine::project::run::Runner;
use faber_engine::project::store::Journal;
use faber_engine::project::wire::Envelope;
use faber_engine::project::worker::ModelPin;
use faber_engine::project::{GitMode, Grant};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The staged regression: a resumed turn is charged the sum of the two totals instead of their difference.
const TEST: &str = "usage::tests::a_resumed_turn_costs_the_difference";
const FILE: &str = "engine/src/usage.rs";
const GOAL: &str = "In engine/src/usage.rs, settle() charges a resumed turn the sum of the reported total and the last trusted total instead of their difference. Fix settle() so the unit test usage::tests::a_resumed_turn_costs_the_difference passes. Do not change any test and do not change anything else in the file.";
const BUG: (&str, &str) = ("(total >= b.total).then_some(total - b.total)", "(total >= b.total).then_some(total + b.total)");
/// What the whole run may cost, in millionths of a dollar, and what the mission alone is admitted under.
const MISSION_CEILING_MICROS: u64 = faber_engine::project::canary::REAL_CAP_MICROS;

/// The engine's roles, except that the first Writer start is refused as an interruption would be (nothing is spent).
struct Interrupt {
    engine: Engine,
    fired: AtomicBool,
}
impl Roles for Interrupt {
    async fn start(&self, journal: &Journal, request: Invocation, writer: Option<(&mut Leases, &Lease)>) -> Result<(), String> {
        if writer.is_some() && !self.fired.swap(true, Ordering::SeqCst) {
            return Err("reason.interrupted".into());
        }
        Roles::start(&self.engine, journal, request, writer).await
    }
    async fn wait(&self, journal: &Journal, invocation: &str) -> Result<Envelope, String> {
        Roles::wait(&self.engine, journal, invocation).await
    }
    fn live(&self, invocation: &str) -> bool {
        Roles::live(&self.engine, invocation)
    }
    async fn settle(&self, invocation: &str) -> Result<Option<Termination>, String> {
        Roles::settle(&self.engine, invocation).await
    }
    async fn abandon(&self, journal: &Journal, invocation: &str) -> Result<(), String> {
        Roles::abandon(&self.engine, journal, invocation).await
    }
    fn pause(&self, mission: &str) {
        Roles::pause(&self.engine, mission);
    }
}

/// The ids of the ledger's invocations and the grant revision the first project holds.
fn ledger_view(engine: &Engine) -> io::Result<(Vec<String>, u64)> {
    let ids = Budget::load(&engine.ledger()).map_err(fail)?.invocations.keys().cloned().collect();
    let revision = engine.projects_checked().map_err(fail)?.first().and_then(|p| p.grant.as_ref().map(|g| g.revision)).unwrap_or(0);
    Ok((ids, revision))
}

fn fail<T: std::fmt::Display>(error: T) -> io::Error {
    io::Error::other(error.to_string())
}

struct Log {
    steps: Vec<Value>,
    started: Instant,
}
impl Log {
    fn step(&mut self, name: &str, detail: Value) {
        eprintln!("t13: {name}");
        self.steps.push(json!({"step": name, "atSecs": self.started.elapsed().as_secs(), "detail": detail}));
    }
}

/// Samples the private working set of this process and everything it owns (the engine and the CLI sessions) until
/// stopped; the peak is what the run reports.
struct Sampler {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Vec<(u64, f64)>>>,
}
impl Sampler {
    fn start(started: Instant) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut samples = vec![];
            while !flag.load(Ordering::Relaxed) {
                if let Ok(mb) = faber_testkit::proctree::private_mb(std::process::id()) {
                    samples.push((started.elapsed().as_secs(), mb));
                }
                for _ in 0..30 {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
            samples
        });
        Self { stop, thread: Some(thread) }
    }
    fn finish(mut self) -> Value {
        self.stop.store(true, Ordering::Relaxed);
        let samples = self.thread.take().and_then(|t| t.join().ok()).unwrap_or_default();
        let peak = samples.iter().map(|s| s.1).fold(0.0_f64, f64::max);
        json!({"peakPrivateMb": peak, "samples": samples.len(), "timeline": samples.iter().step_by((samples.len() / 40).max(1)).collect::<Vec<_>>()})
    }
}

fn git(program: &str, root: &Path, args: &[&str]) -> io::Result<String> {
    use std::os::windows::process::CommandExt;
    let output = Command::new(program)
        .current_dir(root)
        .args(["-c", "core.hooksPath=NUL", "-c", "core.fsmonitor=false"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .creation_flags(faber_testkit::contain::NO_WINDOW)
        .output()?;
    if !output.status.success() {
        return Err(fail(format!("git {args:?}: {}", String::from_utf8_lossy(&output.stderr))));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}

fn version_of(command: &str, prefix: &[String]) -> io::Result<String> {
    use std::os::windows::process::CommandExt;
    let output = Command::new(command)
        .args(prefix)
        .arg("--version")
        .stdin(Stdio::null())
        .creation_flags(faber_testkit::contain::NO_WINDOW)
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    text.split_whitespace()
        .find(|t| t.bytes().any(|b| b.is_ascii_digit()) && t.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
        .map(str::to_string)
        .ok_or_else(|| fail("no version in the CLI output"))
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A fresh clone of Faber at the source's head, with the regression committed on top and the installed dependencies
/// beside it (the local build needs them and never runs a package manager).
fn clone_with_regression(git_program: &str, source: &Path, branch: &str, repo: &Path) -> io::Result<()> {
    let parent = repo.parent().ok_or_else(|| fail("no workspace"))?;
    std::fs::create_dir_all(parent)?;
    git(
        git_program,
        parent,
        &["clone", "--quiet", "--no-hardlinks", "--single-branch", "--branch", branch, &source.to_string_lossy(), &repo.to_string_lossy()],
    )?;
    git(git_program, repo, &["remote", "remove", "origin"])?;
    git(git_program, repo, &["config", "user.name", "Faber T13"])?;
    git(git_program, repo, &["config", "user.email", "t13@example.invalid"])?;
    let path = repo.join(FILE);
    let text = std::fs::read_to_string(&path)?;
    if std::env::var("FABER_T13_REGRESSED").is_ok() {
        // The source already carries the regression, so the running baseline carries it too.
        if !text.contains(BUG.1) {
            return Err(fail("the source does not carry the staged regression"));
        }
        copy_tree(&source.join("node_modules"), &repo.join("node_modules"))?;
        return Ok(());
    }
    if !text.contains(BUG.0) {
        return Err(fail("the source no longer has the line the regression replaces"));
    }
    std::fs::write(&path, text.replacen(BUG.0, BUG.1, 1))?;
    git(git_program, repo, &["add", "--", FILE])?;
    git(git_program, repo, &["commit", "--no-gpg-sign", "--quiet", "-m", "stage a regression in settle for the T13 run"])?;
    copy_tree(&source.join("node_modules"), &repo.join("node_modules"))?;
    Ok(())
}

pub async fn run(data: &Path) -> io::Result<()> {
    let mut log = Log { steps: vec![], started: Instant::now() };
    let sampler = Sampler::start(log.started);
    let result = drive(&mut log).await;
    let ram = sampler.finish();
    let verdict = match &result {
        Ok(()) => json!({"completed": true}),
        Err(error) => json!({"completed": false, "stoppedAt": error.to_string()}),
    };
    let report =
        json!({"schema": 2, "t13Mission": verdict, "steps": log.steps, "ram": ram, "elapsedSecs": log.started.elapsed().as_secs()});
    std::fs::write(data.join("t13-mission.json"), serde_json::to_vec_pretty(&report).map_err(fail)?)?;
    result
}

async fn drive(log: &mut Log) -> io::Result<()> {
    let engine_data = PathBuf::from(std::env::var("FABER_QUALIFICATION_ENGINE_DATA").map_err(|_| fail("no engine data folder"))?);
    let workspace = PathBuf::from(std::env::var("FABER_QUALIFICATION_WORKSPACE").map_err(|_| fail("no workspace"))?);
    let source = PathBuf::from(std::env::var("FABER_T13_SOURCE").map_err(|_| fail("no source checkout to clone"))?);
    let branch = std::env::var("FABER_T13_BRANCH").map_err(|_| fail("no source branch"))?;
    let proof_target = PathBuf::from(std::env::var("FABER_T13_PROOF_TARGET").map_err(|_| fail("no proof build folder"))?);
    let clis = [(Vendor::Claude, "claude"), (Vendor::Codex, "codex")]
        .into_iter()
        .map(|(vendor, name)| faber_engine::cli::find_cli(name).map(|cli| (vendor, cli)).ok_or_else(|| fail("CLI unavailable")))
        .collect::<io::Result<BTreeMap<_, _>>>()?;
    let versions: BTreeMap<Vendor, String> =
        clis.iter().map(|(v, c)| version_of(&c.command, &c.prefix).map(|ver| (*v, ver))).collect::<io::Result<_>>()?;
    let engine = Engine::new(
        EngineOptions {
            data_dir: engine_data.clone(),
            clis,
            spawn: faber_engine::proc::real_spawner(),
            windows: true,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(30),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );

    let git_program = faber_engine::cli::find_cli("git").ok_or_else(|| fail("git unavailable"))?.command;
    let repo = workspace.join("faber-clone");
    clone_with_regression(&git_program, &source, &branch, &repo)?;
    // R138: the proof may write only in the clone and its own folders, so it has a cargo home of its own. It is filled here,
    // from the tree the harness staged, before any role changes a file; the proof itself then stays offline.
    let cargo = faber_engine::cli::find_cli("cargo").ok_or_else(|| fail("cargo unavailable"))?.command;
    let mut fetch = Command::new(cargo);
    fetch.args(["fetch", "--locked"]).current_dir(&repo).env("CARGO_HOME", proof_target.join("cargo-home")).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        fetch.creation_flags(0x0800_0000);
    }
    if !fetch.status()?.success() {
        return Err(fail("the proof's cargo home could not be filled"));
    }
    let mut process = Process::find().map_err(|e| fail(e.key()))?;
    let repository = enroll(&mut process, &repo, "HEAD").map_err(|e| fail(e.key()))?;
    let base = repository.base.clone();
    let mut grant = Grant::new(&repository.identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repository.author.clone();
    grant.self_update = true;
    let mut project = engine.add_project(&repo.to_string_lossy());
    let before = project.clone();
    project.repo = Some(repository.identity.clone());
    project.grant = Some(grant.clone());
    engine.update_project(&before, &project).map_err(fail)?;
    log.step("repository", json!({"base": base, "clone": repo.to_string_lossy(), "source": source.to_string_lossy(), "branch": branch}));

    // Budget: the real account check, the harness-only R132 waiver (single use, this mission), then the caps.
    faber_engine::project::canary::read_accounts(&engine).await;
    let ledger = engine.ledger();
    let readings: Value =
        Vendor::ALL.iter().map(|v| (v.name().to_string(), serde_json::to_value(ledger.of(*v)).unwrap_or(Value::Null))).collect();
    log.step("account-checks", readings.clone());
    let mission = engine.new_mission(&project, GOAL, &base).map_err(fail)?;
    let mid = mission.id.clone();
    faber_engine::project::budget::r132::waive(faber_engine::project::budget::r132::Exception::R132, &engine_data, &mid, &readings)?;
    caps(&engine, &mid, MISSION_CEILING_MICROS).map_err(|e| fail(format!("caps: {e}")))?;
    log.step("budget", json!({"ceilingMicros": MISSION_CEILING_MICROS, "waiver": "R132"}));

    let journal = Journal::open(Path::new(&repository.identity.root), &engine_data, &mid).map_err(|e| fail(e.key()))?;
    Runner::start(&journal, &engine, mission).map_err(fail)?;
    let pin = |vendor: Vendor, model: &str| ModelPin {
        vendor,
        model: model.into(),
        effort: "medium".into(),
        cli_version: versions[&vendor].clone(),
    };
    let text = std::fs::read_to_string(repo.join(FILE))?;
    // Authority and the proof are re-read each time the coordinator is made, as after a restart.
    let build = || -> io::Result<(Broker<Process>, Setup)> {
        let broker = Broker::open(repository.clone(), journal.clone(), &mid, 3, Process::find().map_err(|e| fail(e.key()))?)
            .map_err(|e| fail(e.key()))?;
        let (definition, proof_spawn) =
            cargo_proof("usage-test", &["-p", "faber-engine", "--lib", TEST], &proof_target, true).map_err(fail)?;
        let grant = grant.clone();
        let setup = Setup {
            pins: Pins {
                lead: pin(Vendor::Claude, "claude-haiku-5-5"),
                writer: pin(Vendor::Claude, "claude-haiku-5-5"),
                reviewer: pin(Vendor::Codex, "gpt-6-luna"),
            },
            proof: definition,
            proof_spawn,
            sources: vec![(FILE.into(), text.clone())],
            writable: vec!["engine/src".into()],
            grant: Arc::new(move || Ok(Some(grant.clone()))),
            revision: 3,
            micros: 900_000,
            turn_limit: Duration::from_secs(300),
            plan: None,
        };
        Ok((broker, setup))
    };
    log.step("mission-started", json!({"mission": mid, "goal": GOAL}));
    let roles = Interrupt { engine: engine.clone(), fired: AtomicBool::new(false) };
    let (broker, setup) = build()?;
    let mut coordinator = Coordinator::new(roles, journal.clone(), broker, setup).map_err(fail)?;
    let mut outcome = coordinator.drive().await;
    drop(coordinator);
    if outcome == Outcome::Stopped(Stop::Wait("reason.interrupted".into())) {
        // The Writer's start was refused once, as a person's Stop would; the saved wait is resumed through the engine's
        // own Resume, which must keep every grant and charge nothing twice.
        let before = ledger_view(&engine)?;
        let mut runner = Runner::load(&journal).map_err(fail)?;
        let paused = runner.mission.state;
        runner.resume(&journal, &engine).map_err(fail)?;
        let after = ledger_view(&engine)?;
        log.step(
            "positive-resume",
            json!({"stateBefore": format!("{paused:?}"), "stateAfter": format!("{:?}", runner.mission.state),
                "revisionBefore": before.1, "revisionAfter": after.1, "invocationsBefore": before.0, "invocationsAfter": after.0,
                "grantBefore": serde_json::to_value(&project.grant).unwrap_or(Value::Null),
                "grantAfter": serde_json::to_value(engine.projects_checked().map_err(fail)?.first().and_then(|p| p.grant.clone())).unwrap_or(Value::Null)}),
        );
        let roles = Interrupt { engine: engine.clone(), fired: AtomicBool::new(true) };
        let (broker, setup) = build()?;
        outcome = Coordinator::new(roles, journal.clone(), broker, setup).map_err(fail)?.drive().await;
    } else {
        log.step("resume-not-exercised", json!({"outcome": format!("{outcome:?}")}));
    }
    let Outcome::Done { head } = &outcome else {
        log.step("mission-stopped", json!({"outcome": format!("{outcome:?}")}));
        return Err(fail(format!("the mission did not finish: {outcome:?}")));
    };
    let tree = git(&git_program, &repo, &["rev-parse", &format!("{head}^{{tree}}")])?;
    let landed = git(&git_program, &repo, &["show", &format!("faber/{mid}:{FILE}")]).unwrap_or_default();
    log.step(
        "landed",
        json!({"head": head, "tree": tree, "ref": format!("faber/{mid}"),
            "fixed": landed.contains("then_some(total - b.total)") && !landed.contains("then_some(total + b.total)"),
            "ownerHead": git(&git_program, &repo, &["rev-parse", "HEAD"])?, "ownerBase": base}),
    );

    // Settlement and calibration, then the spend of the whole mission.
    let calibration = engine.calibrate_mission(&mid).await;
    log.step("calibration", json!({"status": format!("{calibration:?}")}));
    let spent = Budget::load(&engine.ledger()).map_err(fail)?;
    let invocations: Value = spent
        .invocations
        .iter()
        .map(|(id, r)| (id.clone(), json!({"vendor": r.request.vendor, "model": r.request.model, "paidMicros": r.paid_micros, "settled": r.settled, "bounded": r.bounded})))
        .collect();
    let total: u64 = spent.invocations.values().map(|r| r.paid_micros).sum();
    log.step("spend", json!({"totalMicros": total, "invocations": invocations}));
    faber_engine::project::budget::r132::close();
    engine.shutdown();
    if !matches!(calibration, Ok(faber_engine::project::budget::Status::Ready)) {
        return Err(fail(format!("calibration did not complete: {calibration:?}")));
    }
    Ok(())
}
