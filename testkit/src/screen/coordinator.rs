//! The production coordinator on real git: one mission from its goal to the landing, with the role turns replayed from
//! recorded replies. A crash is injected at every step (before and after its mark) and inside every role turn, and the
//! resumed run must land the same single commit without repeating a role or an effect. Runs in a contained helper on the
//! hidden desktop, never in unit tests and never in a real checkout.
use super::projects::{git, run_probe_for};
use super::{Result, Scenario, ensure};
use crate::app::Ctx;
use faber_engine::model::Vendor;
use faber_engine::proc::{OwnedTree, Proc};
use faber_engine::project::coordinator::{Coordinator, Outcome, Pins, Roles, Setup, Stop, deliver, recorded_reply};
use faber_engine::project::git::{Broker, Process, Repository, enroll};
use faber_engine::project::lease::{Leases, Termination};
use faber_engine::project::proof::Definition;
use faber_engine::project::run::Runner;
use faber_engine::project::store::Journal;
use faber_engine::project::wire::Envelope;
use faber_engine::project::worker::ModelPin;
use faber_engine::project::{GitMode, Grant, Lease, Mission, MissionState};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const FIXTURES: [&str; 5] = ["lead.json", "writer.json", "reviewer-pass.json", "reviewer-block.json", "reviewer-malformed.txt"];

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario {
        name: "coordinator: a mission lands from its goal, survives a crash at every step, stops safely and waits on the budget",
        run: acceptance,
    }]
}

fn acceptance(ctx: &mut Ctx) -> Result {
    let target = ctx.sb.sandbox.join("fixtures");
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for name in FIXTURES {
        std::fs::copy(ctx.paths.fixtures.join("projects/coordinator").join(name), target.join(name)).map_err(|e| format!("{name}: {e}"))?;
    }
    run_probe_for(ctx, "--project-coordinator", Duration::from_secs(300))?;
    let report: Value = serde_json::from_slice(&std::fs::read(ctx.sb.sandbox.join("coordinator.json")).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    for key in ["mission", "crashes", "stops", "budget"] {
        ensure(report[key]["ok"] == true, || format!("coordinator {key}: {}", report[key]))?;
        ctx.notes.push(format!("coordinator {key}: {}; recorded replies, real git, real owned stand-in process", report[key]));
    }
    Ok(())
}

/// What a recorded role does in this case.
#[derive(Clone, Default)]
struct Plot {
    reviewer: &'static str,
    /// The turn that never answers: the process dies while it runs.
    die: Option<&'static str>,
    /// The turn the budget refuses.
    refuse: Option<&'static str>,
}

/// Recorded replies for the three roles, with a real owned process standing in for the Writer's session.
struct Scripted {
    dir: PathBuf,
    plot: Plot,
    live: Mutex<HashSet<String>>,
    trees: Mutex<HashMap<String, (OwnedTree, Proc)>>,
    settled: Mutex<HashSet<String>>,
}

impl Scripted {
    fn new(dir: &Path, plot: Plot) -> Self {
        let settled = std::fs::read_to_string(dir.join("settled.txt")).unwrap_or_default().lines().map(str::to_string).collect();
        Self { dir: dir.into(), plot, live: Mutex::default(), trees: Mutex::default(), settled: Mutex::new(settled) }
    }
    fn note(&self, file: &str, line: &str) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join(file)) {
            let _ = writeln!(f, "{line}");
        }
    }
    fn reply_text(&self, journal: &Journal, task: &str, invocation: &str) -> std::result::Result<String, String> {
        let runner = Runner::load(journal).map_err(str::to_string)?;
        let brief = &runner.workers.get(task).ok_or("no worker")?.brief;
        let name = match task {
            "lead" => "lead.json".to_string(),
            "reviewer" => format!("{}.json", self.plot.reviewer),
            _ => "writer.json".to_string(),
        };
        if self.plot.reviewer.ends_with("malformed") && task == "reviewer" {
            return std::fs::read_to_string(self.dir.join("../fixtures").join("reviewer-malformed.txt"))
                .map_err(|e| format!("{invocation}: {e}"));
        }
        let patch: Value = serde_json::from_slice(&std::fs::read(self.dir.join("../fixtures").join(name)).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        recorded_reply(brief, &patch).map_err(str::to_string)
    }
}

impl Roles for Scripted {
    async fn start(
        &self,
        journal: &Journal,
        request: faber_engine::project::budget::Invocation,
        writer: Option<(&mut Leases, &Lease)>,
    ) -> std::result::Result<(), String> {
        let id = request.id.clone();
        if self.plot.refuse == Some(id.as_str()) {
            return Err("reason.budget.cap".into());
        }
        self.note("roles.log", &format!("start {id}"));
        self.live.lock().unwrap().insert(id.clone());
        if let Some((leases, lease)) = writer {
            // The Writer's session runs inside a tree the engine owns, exactly as in production.
            let ping = Path::new(&std::env::var("SystemRoot").map_err(|e| e.to_string())?).join("System32").join("ping.exe");
            let stand_in =
                faber_engine::proc::real_spawner()(&ping.to_string_lossy(), &["-n".into(), "60".into(), "127.0.0.1".into()], &self.dir)
                    .map_err(|e| e.to_string())?;
            let tree = stand_in.contained.as_ref().and_then(|c| c.owned_tree()).ok_or("the stand-in has no owned tree")?;
            leases.attach_process(lease, tree.identity()).map_err(|e| e.key().to_string())?;
            self.trees.lock().unwrap().insert(id.clone(), (tree, stand_in));
        }
        if self.plot.die == Some(id.as_str()) {
            return Ok(());
        }
        let text = self.reply_text(journal, &request.worker, &id)?;
        deliver(journal, &request.worker, &id, &text)
    }

    async fn wait(&self, journal: &Journal, invocation: &str) -> std::result::Result<Envelope, String> {
        loop {
            if let Some(reply) = journal.reply(invocation).map_err(|e| e.key().to_string())? {
                return Ok(reply);
            }
            if let Some(key) = journal.refusal(invocation).map_err(|e| e.key().to_string())? {
                return Err(key);
            }
            // The process died or never answered: a crash drops this future, a test never needs it to end.
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }

    fn live(&self, invocation: &str) -> bool {
        self.live.lock().unwrap().contains(invocation)
    }

    async fn settle(&self, invocation: &str) -> std::result::Result<Option<Termination>, String> {
        let tree = self.trees.lock().unwrap().remove(invocation);
        if let Some((tree, _stand_in)) = tree {
            let retired = tree.retire().await.map_err(|e| e.to_string())?;
            self.live.lock().unwrap().remove(invocation);
            self.settled.lock().unwrap().insert(invocation.into());
            self.note("settled.txt", invocation);
            return Ok(Some(Termination::from_retired(&retired, true)));
        }
        if self.live.lock().unwrap().remove(invocation) || self.settled.lock().unwrap().contains(invocation) {
            self.settled.lock().unwrap().insert(invocation.into());
            self.note("settled.txt", invocation);
            return Ok(None);
        }
        Err("reason.project.processTreeUnproven".into())
    }

    async fn abandon(&self, _journal: &Journal, invocation: &str) -> std::result::Result<(), String> {
        self.note("roles.log", &format!("abandon {invocation}"));
        self.settled.lock().unwrap().insert(invocation.into());
        self.note("settled.txt", invocation);
        Ok(())
    }

    fn pause(&self, _mission: &str) {
        self.note("roles.log", "pause");
    }
}

/// One throwaway repository with one source file that the engine's proof reads.
struct World {
    dir: PathBuf,
    repo: PathBuf,
    program: String,
    repository: Repository,
    grant: Grant,
    base: String,
}

fn world(sandbox: &Path, name: &str) -> Result<World> {
    let dir = sandbox.join(name);
    let repo = dir.join("repo");
    std::fs::create_dir_all(repo.join("src")).map_err(|e| e.to_string())?;
    let program = faber_engine::cli::find_cli("git").ok_or("missing git")?.command;
    git(&program, &repo, &["init", "--initial-branch=owner"])?;
    git(&program, &repo, &["config", "user.name", "Fixture"])?;
    git(&program, &repo, &["config", "user.email", "fixture@example.invalid"])?;
    std::fs::write(repo.join("src/a.rs"), "base").map_err(|e| e.to_string())?;
    git(&program, &repo, &["add", "--", "src/a.rs"])?;
    git(&program, &repo, &["commit", "--no-gpg-sign", "-m", "fixture base"])?;
    let repository = enroll(&mut Process::find().map_err(|e| e.key())?, &repo, "HEAD").map_err(|e| e.key())?;
    let mut grant = Grant::new(&repository.identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repository.author.clone();
    let base = repository.base.clone();
    let world = World { dir, repo, program, repository, grant, base };
    let (journal, _) = open(&world)?;
    let mission = Mission::new("m1", "p1", &world.repository.identity.common_dir, "Make src/a.rs say after", &world.base, 3, 1);
    Runner::create(&journal, mission).map_err(str::to_string)?;
    Ok(world)
}

fn open(world: &World) -> Result<(Journal, Broker<Process>)> {
    let journal = Journal::open(Path::new(&world.repository.identity.root), &world.dir.join("records"), "m1").map_err(|e| e.key())?;
    let broker =
        Broker::open(world.repository.clone(), journal.clone(), "m1", 3, Process::find().map_err(|e| e.key())?).map_err(|e| e.key())?;
    Ok((journal, broker))
}

fn setup(world: &World) -> Result<Setup> {
    let pin = |vendor, model: &str| ModelPin { vendor, model: model.into(), effort: "medium".into(), cli_version: "1.0.0".into() };
    let grant = world.grant.clone();
    Ok(Setup {
        pins: Pins {
            lead: pin(Vendor::Claude, "claude-sonnet-5-5"),
            writer: pin(Vendor::Claude, "claude-haiku-5-5"),
            reviewer: pin(Vendor::Codex, "gpt-6-luna"),
        },
        proof: Definition {
            id: "fixture-check".into(),
            version: 1,
            program: std::env::current_exe().map_err(|e| e.to_string())?,
            argv: vec!["--project-proof".into(), "src/a.rs".into()],
            checks: vec!["C1".into()],
            toolchain: "first-party-fixture".into(),
            dependencies: "none".into(),
            environment: BTreeMap::new(),
            heavy: false,
        },
        proof_spawn: faber_engine::proc::isolated_spawner(BTreeMap::new()),
        sources: vec![("src/a.rs".into(), "base".into())],
        writable: vec!["src".into()],
        grant: Arc::new(move || Ok(Some(grant.clone()))),
        revision: 3,
        micros: 100_000,
        turn_limit: Duration::from_secs(120),
        plan: None,
    })
}

fn coordinator(world: &World, plot: Plot) -> Result<Coordinator<Scripted, Process>> {
    let (journal, broker) = open(world)?;
    Coordinator::new(Scripted::new(&world.dir, plot), journal, broker, setup(world)?).map_err(str::to_string)
}

/// Everything the end state of a landed mission has to satisfy, whatever happened on the way.
fn landed(world: &World, outcome: &Outcome, starts: &[&str]) -> Result {
    let Outcome::Done { head } = outcome else { return Err(format!("the mission did not finish: {outcome:?}")) };
    let (p, r) = (&world.program, &world.repo);
    ensure(git(p, r, &["rev-parse", "faber/m1"])? == *head, || "the mission ref is not the landed head".into())?;
    ensure(git(p, r, &["show", "faber/m1:src/a.rs"])? == "after", || "wrong landed content".into())?;
    ensure(git(p, r, &["rev-list", "--count", &format!("{}..faber/m1", world.base)])? == "1", || "not exactly one commit landed".into())?;
    ensure(git(p, r, &["rev-parse", "HEAD"])? == world.base, || "the owner ref moved".into())?;
    ensure(git(p, r, &["status", "--porcelain"])?.is_empty(), || "the owner checkout was touched".into())?;
    ensure(std::fs::read_to_string(r.join("src/a.rs")).map_err(|e| e.to_string())? == "base", || "the owner file changed".into())?;
    let (journal, _) = open(world)?;
    ensure(journal.pending().map_err(|e| e.key())?.is_empty(), || "an effect is still pending".into())?;
    ensure(Runner::load(&journal).map_err(str::to_string)?.mission.state == MissionState::Done, || "the mission is not done".into())?;
    ensure(journal.report().map_err(|e| e.key())?.is_some_and(|r| r.state == "done"), || "no report".into())?;
    let log = std::fs::read_to_string(world.dir.join("roles.log")).unwrap_or_default();
    let started: Vec<&str> = log.lines().filter_map(|l| l.strip_prefix("start ")).collect();
    ensure(started == starts, || format!("roles started {started:?}, expected {starts:?}"))?;
    for id in ["branch", "worktree"] {
        ensure(journal.effect_receipt(id).map_err(|e| e.key())?.is_some(), || format!("no receipt for {id}"))?;
    }
    Ok(())
}

const ALL: [&str; 3] = ["lead-1", "writer-1", "reviewer-1"];
const PASS: Plot = Plot { reviewer: "reviewer-pass", die: None, refuse: None };

async fn run_mission(sandbox: &Path) -> Result<Value> {
    let w = world(sandbox, "mission")?;
    let mut c = coordinator(&w, PASS)?;
    let outcome = c.drive().await;
    landed(&w, &outcome, &ALL)?;
    drop(c);
    ensure(w.base != git(&w.program, &w.repo, &["rev-parse", "faber/m1"])?, || "nothing landed".into())?;
    Ok(json!({"ok": true, "outcome": format!("{outcome:?}")}))
}

/// The steps the coordinator marks, in order.
const MARKS: usize = 7;

async fn run_crashes(sandbox: &Path) -> Result<Value> {
    let mut cases = vec![];
    // The power goes out right after a step's mark, or right before it (the work is done and its mark is not).
    for before in [false, true] {
        for n in 1..=MARKS {
            let name = format!("crash-{}-{n}", if before { "before" } else { "after" });
            let w = world(sandbox, &name)?;
            let mut a = coordinator(&w, PASS).map_err(|e| format!("{name}: first start: {e}"))?;
            if before {
                a.crash_before(n);
            } else {
                a.crash_after(n);
            }
            let first = a.drive().await;
            ensure(first == Outcome::Stopped(Stop::Crash), || format!("{name}: expected the injected crash, got {first:?}"))?;
            drop(a);
            // A new process: new journal handle, new broker, no live session.
            let mut b = coordinator(&w, PASS).map_err(|e| format!("{name}: restart: {e}"))?;
            let second = b.drive().await;
            landed(&w, &second, &ALL).map_err(|e| format!("{name}: {e}"))?;
            cases.push(json!({"case": name, "resumed": true}));
        }
    }
    // The process dies while a role turn runs. Read-only roles are replaced by a new turn; a Writer is parked.
    for (turn, parks) in [("lead-1", false), ("reviewer-1", false), ("writer-1", true)] {
        let name = format!("die-{turn}");
        let w = world(sandbox, &name)?;
        let plot = Plot { die: Some(turn), ..PASS };
        let mut a = coordinator(&w, plot.clone()).map_err(|e| format!("{name}: first start: {e}"))?;
        let first = tokio::time::timeout(Duration::from_secs(5), a.drive()).await;
        ensure(first.is_err(), || format!("{name}: the turn that never answers ended: {first:?}"))?;
        drop(a);
        let mut b = coordinator(&w, PASS).map_err(|e| format!("{name}: restart: {e}"))?;
        let second = b.drive().await;
        drop(b);
        if parks {
            ensure(second == Outcome::Stopped(Stop::Park("reason.lease.live".into())), || {
                format!("{name}: expected a parked mission, got {second:?}")
            })?;
            let (journal, _) = open(&w)?;
            ensure(Runner::load(&journal).map_err(str::to_string)?.mission.state == MissionState::Paused, || {
                format!("{name}: not paused")
            })?;
            ensure(git(&w.program, &w.repo, &["rev-parse", "faber/m1"])? == w.base, || format!("{name}: something landed"))?;
        } else {
            let starts: Vec<&str> = match turn {
                "lead-1" => vec!["lead-1", "lead-2", "writer-1", "reviewer-1"],
                _ => vec!["lead-1", "writer-1", "reviewer-1", "reviewer-2"],
            };
            landed(&w, &second, &starts).map_err(|e| format!("{name}: {e}"))?;
        }
        cases.push(json!({"case": name, "parked": parks}));
    }
    Ok(json!({"ok": true, "cases": cases}))
}

async fn run_stops(sandbox: &Path) -> Result<Value> {
    let mut seen = vec![];
    for (name, plot, key) in [
        ("malformed-reviewer", Plot { reviewer: "reviewer-malformed", ..PASS }, "error.wire.malformed"),
        ("blocking-review", Plot { reviewer: "reviewer-block", ..PASS }, "reason.queue.review"),
    ] {
        let w = world(sandbox, name)?;
        let mut c = coordinator(&w, plot)?;
        let outcome = c.drive().await;
        ensure(matches!(&outcome, Outcome::Stopped(Stop::Block(k)) if k == key), || {
            format!("{name}: expected a block on {key}, got {outcome:?}")
        })?;
        drop(c);
        let (journal, _) = open(&w)?;
        let runner = Runner::load(&journal).map_err(str::to_string)?;
        ensure(runner.mission.state == MissionState::Blocked && runner.mission.reason.as_deref() == Some(key), || {
            format!("{name}: the mission does not say why it stopped")
        })?;
        ensure(git(&w.program, &w.repo, &["rev-parse", "faber/m1"])? == w.base, || format!("{name}: something landed"))?;
        ensure(git(&w.program, &w.repo, &["rev-parse", "HEAD"])? == w.base, || format!("{name}: the owner ref moved"))?;
        // The same coordinator, started again, stays stopped: a blocked mission is not retried by itself.
        let mut again = coordinator(&w, PASS)?;
        ensure(matches!(again.drive().await, Outcome::Stopped(Stop::Block(_))), || format!("{name}: a blocked mission ran again"))?;
        seen.push(json!({"case": name, "key": key}));
    }
    let w = world(sandbox, "same-vendor")?;
    let (journal, broker) = open(&w)?;
    let mut bad = setup(&w)?;
    bad.pins.reviewer =
        ModelPin { vendor: Vendor::Claude, model: "claude-sonnet-5-5".into(), effort: "medium".into(), cli_version: "1.0.0".into() };
    ensure(Coordinator::new(Scripted::new(&w.dir, PASS), journal, broker, bad).is_err(), || {
        "a reviewer of the Writer's vendor was accepted".into()
    })?;
    Ok(json!({"ok": true, "cases": seen}))
}

async fn run_budget(sandbox: &Path) -> Result<Value> {
    let w = world(sandbox, "budget")?;
    let mut a = coordinator(&w, Plot { refuse: Some("writer-1"), ..PASS })?;
    let first = a.drive().await;
    ensure(first == Outcome::Stopped(Stop::Wait("reason.budget.cap".into())), || format!("expected a wait on the budget, got {first:?}"))?;
    drop(a);
    let (journal, _) = open(&w)?;
    let runner = Runner::load(&journal).map_err(str::to_string)?;
    ensure(runner.mission.state == MissionState::Paused && runner.mission.reason.as_deref() == Some("reason.budget.cap"), || {
        "the wait was not saved on the mission".into()
    })?;
    drop(runner);
    // While paused, nothing runs.
    let mut still = coordinator(&w, PASS)?;
    ensure(matches!(still.drive().await, Outcome::Stopped(Stop::Wait(_))), || "a paused mission ran".into())?;
    drop(still);
    // The person resumes once the budget is back (the engine's own Resume is exercised by the real run).
    let mut runner = Runner::load(&journal).map_err(str::to_string)?;
    runner.mission.state = MissionState::Running;
    runner.mission.reason = None;
    runner.save(&journal).map_err(str::to_string)?;
    let mut b = coordinator(&w, PASS)?;
    let second = b.drive().await;
    landed(&w, &second, &["lead-1", "writer-2", "reviewer-1"])?;
    Ok(json!({"ok": true, "waitedOn": "reason.budget.cap"}))
}

/// Called only by the test helper under `Owned`, with its sandbox environment and hidden desktop.
pub fn probe(sandbox: &Path) -> Result {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e| e.to_string())?;
    let mut report = serde_json::Map::new();
    let mut failure = None;
    for (key, result) in [
        ("mission", runtime.block_on(run_mission(sandbox))),
        ("crashes", runtime.block_on(run_crashes(sandbox))),
        ("stops", runtime.block_on(run_stops(sandbox))),
        ("budget", runtime.block_on(run_budget(sandbox))),
    ] {
        match result {
            Ok(value) => {
                report.insert(key.into(), value);
            }
            Err(error) => {
                report.insert(key.into(), json!({"ok": false, "error": error}));
                failure.get_or_insert(format!("{key}: {error}"));
            }
        }
    }
    std::fs::write(sandbox.join("coordinator.json"), Value::Object(report).to_string()).map_err(|e| e.to_string())?;
    match failure {
        None => std::fs::write(sandbox.join("project-git.ok"), "coordinator proved").map_err(|e| e.to_string()),
        Some(error) => Err(error),
    }
}

/// The canary of a build: the frozen mission on recorded replies through the production coordinator, then the
/// supervisor's reading of what it left behind. A child that lies about its mode or its mission is not believed.
pub fn probe_canary(sandbox: &Path) -> Result {
    use faber_engine::project::canary::{self, Evidence, Mode};
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e| e.to_string())?;
    let data = sandbox.join("canary-data");
    let engine = faber_engine::Engine::new(
        faber_engine::EngineOptions {
            data_dir: data.clone(),
            clis: Default::default(),
            spawn: faber_engine::proc::real_spawner(),
            windows: true,
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(30),
            runtime: runtime.handle().clone(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    let work = sandbox.join("canary-work");
    let evidence = runtime.block_on(canary::run(&engine, &data, &work, Mode::Offline))?;
    engine.shutdown();
    let verified = canary::verify(&evidence, &data)?;
    ensure(verified.mode == Mode::Offline && verified.spent_micros == 0, || "a replay spent money".into())?;
    ensure(verified.writer == Vendor::Claude && verified.reviewer == Vendor::Codex, || "the review is not by the other vendor".into())?;
    let repo = Path::new(&evidence.repo_root);
    let program = faber_engine::cli::find_cli("git").ok_or("missing git")?.command;
    let lib = git(&program, repo, &["show", &format!("{}:src/lib.rs", verified.head)])?;
    ensure(lib.contains("trim().to_lowercase()") && lib.contains("fn trims_and_lowercases"), || "the landed file is not the fix".into())?;
    ensure(git(&program, repo, &["rev-parse", "HEAD"])? != verified.head, || "the owner ref moved".into())?;
    ensure(git(&program, repo, &["status", "--porcelain"])?.is_empty(), || "the checkout was touched".into())?;
    // Lies. A replay claiming to be real has no settled provider spending in the ledger; a mission that never existed
    // has no journal; a mission that is not finished cannot be accepted.
    let lie = Evidence { mode: Mode::Real, ..evidence.clone() };
    ensure(canary::verify(&lie, &data) == Err("reason.canary.notReal".into()), || "a replay passed for a real run".into())?;
    let ghost = Evidence { mission: "00000000-0000-4000-8000-000000000000".into(), ..evidence.clone() };
    ensure(canary::verify(&ghost, &data).is_err(), || "a mission with no journal was accepted".into())?;
    std::fs::write(
        sandbox.join("canary.json"),
        json!({"head": verified.head, "tree": verified.tree, "proofReceipt": verified.proof_receipt, "millis": evidence.millis})
            .to_string(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(sandbox.join("project-git.ok"), "canary proved").map_err(|e| e.to_string())
}

pub fn canary_scenarios() -> Vec<Scenario> {
    vec![Scenario {
        name: "canary: the frozen mission lands through the coordinator and a replay can never pass for a real run",
        run: canary_acceptance,
    }]
}

fn canary_acceptance(ctx: &mut Ctx) -> Result {
    run_probe_for(ctx, "--project-canary", Duration::from_secs(300))?;
    let report: Value = serde_json::from_slice(&std::fs::read(ctx.sb.sandbox.join("canary.json")).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    ctx.notes.push(format!("canary on recorded replies: {report}; real git, cargo proof, other-vendor review, landing; lies refused"));
    Ok(())
}
