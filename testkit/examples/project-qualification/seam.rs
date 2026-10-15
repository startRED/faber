//! T13 seam run: one tiny mission driven by the engine's own role machinery with real provider sessions.
//!
//! A Writer on one vendor proposes a patch, the engine applies it under a lease and proves it, a Reviewer on the other
//! vendor reviews the exact tree, and the engine lands it on a mission ref of a throwaway repository. What this harness
//! scripts is the coordinator (there is no production one yet); every model turn, every spend, every lease, proof,
//! tree and landing goes through the same engine calls a coordinator would make. The R132 waiver is opened here only.
use faber_engine::engine::{Engine, EngineOptions};
use faber_engine::model::Vendor;
use faber_engine::project::budget::{Budget, Change, Grant as Budgeted, Invocation, Kind};
use faber_engine::project::git::{Broker, Process, enroll};
use faber_engine::project::kit::{Assembly, KitReceipt, Machine, assemble, inventory::Inventory};
use faber_engine::project::proof::{Definition, ProofSet};
use faber_engine::project::queue::{Candidate, Review};
use faber_engine::project::run::Runner;
use faber_engine::project::store::Journal;
use faber_engine::project::wire::FileOp;
use faber_engine::project::wire::{Action, Facets, Fresh, Labels, Risk, Verb, Verbatim};
use faber_engine::project::worker::{ModelPin, Worker};
use faber_engine::project::{Brief, ContractPin, Extra, GitMode, Grant, Lease, MissionState, Plan, Role, Task, TaskKind, TaskState};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

const WRITER: &str = "writer";
const REVIEWER: &str = "reviewer";
const DIGEST: &str = "07509c71c43834df7adc4dd63b848a6e050c4b04ffddb484597f341bedf6dbe0";
const CHECK_JS: &str = "const add = require('./src/add.js');\nif (add(2, 3) !== 5 || add(-1, 1) !== 0 || add(0, 0) !== 0) {\n  console.error('add is wrong');\n  process.exit(1);\n}\nconsole.log('ok');\n";

fn fail<T: std::fmt::Display>(error: T) -> io::Error {
    io::Error::other(error.to_string())
}

struct Log {
    steps: Vec<Value>,
    ram: Vec<Value>,
    started: Instant,
}
impl Log {
    fn step(&mut self, name: &str, detail: Value) {
        eprintln!("seam: {name}");
        self.steps.push(json!({"step": name, "atSecs": self.started.elapsed().as_secs(), "detail": detail}));
    }
    fn sample(&mut self, label: &str) {
        let mb = faber_testkit::proctree::private_mb(std::process::id()).ok();
        self.ram.push(json!({"label": label, "atSecs": self.started.elapsed().as_secs(), "privateMb": mb}));
    }
}

fn git(program: &str, root: &Path, args: &[&str]) -> io::Result<String> {
    use std::os::windows::process::CommandExt;
    let output = Command::new(program)
        .current_dir(root)
        .args(["-c", "core.hooksPath=NUL", "-c", "core.fsmonitor=false"])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "NUL")
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

fn node() -> io::Result<PathBuf> {
    let path = std::env::var_os("PATH").ok_or_else(|| fail("no PATH"))?;
    std::env::split_paths(&path).map(|d| d.join("node.exe")).find(|p| p.is_file()).ok_or_else(|| fail("node.exe is not on PATH"))
}

fn brief(role: Role, goal: &str, base: &str, writes: &[&str], lease: Option<u64>, quotes: Vec<Verbatim>, proofs: &[&str]) -> Brief {
    let mut b = Brief::new(
        match role {
            Role::Writer => WRITER,
            _ => REVIEWER,
        },
        1,
        role,
        goal,
        base,
        ContractPin { id: "c1".into(), digest: DIGEST.into() },
    );
    b.scope = vec!["check.js".into(), "src".into()];
    b.writes = writes.iter().map(|s| s.to_string()).collect();
    b.sources = vec!["check.js".into()];
    b.proofs = proofs.iter().map(|s| s.to_string()).collect();
    b.lease = lease;
    b.quotes = quotes;
    b.stop = "Stop after the single envelope.".into();
    b
}

fn envelope_example(task: &str, base: &str, lease: u64, action: Value, ping: Option<&str>) -> String {
    let mut env = json!({"schema": 2, "task": task, "attempt": 1, "base": base, "contract": DIGEST, "lease": lease, "action": action});
    if let Some(ping) = ping {
        env["ping"] = json!(ping);
    }
    env.to_string()
}

fn wire_worker(
    journal: &Journal,
    runner: &mut Runner,
    role: Role,
    brief: Brief,
    vendor: Vendor,
    model: &str,
    cli_version: &str,
) -> io::Result<()> {
    let labels = Labels { verb: Verb::Build, needs: vec![], risk: Risk::Low, fresh: Fresh::Stable, facets: Facets::default() };
    let kit = assemble(&Assembly {
        labels,
        fingerprint: vec![],
        inventory: Inventory::new(vec![]).map_err(|e| fail(e.key()))?,
        evidence: vec![],
        pins: vec![],
        role,
        vendor,
        model: model.into(),
        effort: "medium".into(),
        mandatory_proofs: vec![],
        machine: Machine { os: "windows".into(), apps: vec![], gpu_gb: 0 },
    })
    .map_err(|e| fail(e.key()))?;
    let receipt = KitReceipt::new(&runner.mission.id, &brief.task, 1, kit, 1).map_err(|e| fail(e.key()))?;
    journal.record_kit(&receipt).map_err(|e| fail(e.key()))?;
    let mut brief = brief;
    brief.kit = Some(receipt.pin());
    brief.grant_micros = Some(400_000);
    let pin = ModelPin { vendor, model: model.into(), effort: "medium".into(), cli_version: cli_version.into() };
    let id = brief.task.clone();
    runner.workers.insert(id, Worker::new(brief, pin, 3).map_err(fail)?);
    runner.save(journal).map_err(fail)
}

async fn await_reply(journal: &Journal, log: &mut Log, id: &str, secs: u64) -> Result<faber_engine::project::wire::Envelope, String> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut next_sample = Instant::now();
    while Instant::now() < deadline {
        if let Ok(Some(envelope)) = journal.reply(id) {
            log.sample(&format!("{id}-reply"));
            return Ok(envelope);
        }
        if let Ok(Some(key)) = journal.refusal(id) {
            return Err(format!("the turn ended with no accepted envelope ({key})"));
        }
        if Instant::now() >= next_sample {
            log.sample(&format!("{id}-running"));
            next_sample = Instant::now() + Duration::from_secs(20);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    Err(format!("no reply within {secs} s"))
}

pub async fn run(data: &Path) -> io::Result<()> {
    let mut log = Log { steps: vec![], ram: vec![], started: Instant::now() };
    let result = drive(data, &mut log).await;
    let verdict = match &result {
        Ok(()) => json!({"completed": true}),
        Err(error) => json!({"completed": false, "stoppedAt": error.to_string()}),
    };
    let report = json!({"schema": 1, "seam": verdict, "steps": log.steps, "ram": log.ram});
    std::fs::write(data.join("seam.json"), serde_json::to_vec_pretty(&report).map_err(fail)?)?;
    result
}

async fn drive(data: &Path, log: &mut Log) -> io::Result<()> {
    let engine_data = PathBuf::from(std::env::var("FABER_QUALIFICATION_ENGINE_DATA").map_err(|_| fail("no engine data folder"))?);
    let workspace = PathBuf::from(std::env::var("FABER_QUALIFICATION_WORKSPACE").map_err(|_| fail("no workspace"))?);
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
    log.sample("engine-idle");

    // A throwaway repository with one proof: check.js needs src/add.js, which the Writer must create.
    let repo = workspace.join("mission-repo");
    std::fs::create_dir_all(repo.join("src"))?;
    let git_program = faber_engine::cli::find_cli("git").ok_or_else(|| fail("git unavailable"))?.command;
    git(&git_program, &repo, &["init", "--initial-branch=owner"])?;
    git(&git_program, &repo, &["config", "user.name", "Fixture"])?;
    git(&git_program, &repo, &["config", "user.email", "fixture@example.invalid"])?;
    std::fs::write(repo.join("check.js"), CHECK_JS)?;
    std::fs::write(repo.join("src/README.txt"), "add.js goes here\n")?;
    git(&git_program, &repo, &["add", "--", "check.js", "src/README.txt"])?;
    git(&git_program, &repo, &["commit", "--no-gpg-sign", "-m", "base"])?;
    let repository = enroll(&mut Process::find().map_err(|e| fail(e.key()))?, &repo, "HEAD").map_err(|e| fail(e.key()))?;
    let base = repository.base.clone();
    let mut grant = Grant::new(&repository.identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repository.author.clone();
    let mut project = engine.add_project(&repo.to_string_lossy());
    let before = project.clone();
    project.repo = Some(repository.identity.clone());
    project.grant = Some(grant.clone());
    engine.update_project(&before, &project).map_err(fail)?;
    log.step("repository", json!({"base": base, "checkout": repo.to_string_lossy()}));

    // Budget: the real account check, then the R132 waiver (single use, this process only), then the caps.
    engine.check_plans(Duration::from_secs(20)).await;
    // Provider readings are stamped to the whole second; admission rejects a reading from the future.
    tokio::time::sleep(Duration::from_secs(2)).await;
    let ledger = engine.ledger();
    let readings: Value =
        Vendor::ALL.iter().map(|v| (v.name().to_string(), serde_json::to_value(ledger.of(*v)).unwrap_or(Value::Null))).collect();
    log.step("account-checks", readings.clone());
    let mission = engine.new_mission(&project, "Add src/add.js so check.js passes", &base).map_err(fail)?;
    let mid = mission.id.clone();
    faber_engine::project::budget::r132::waive(&data.join("r132-waiver.json"), &mid, &readings)?;
    let models = [(Vendor::Claude, "claude-haiku-5-5"), (Vendor::Codex, "gpt-6-luna")];
    for (vendor, model) in models {
        engine
            .mission_budget(Change::AllocateModel { vendor, model: model.into(), cap_micros: 1_000_000 })
            .map_err(|e| fail(format!("allocate {model}: {e}")))?;
    }
    let budget = Budgeted {
        cap_micros: 2_000_000,
        worker_micros: 800_000,
        kind_caps: BTreeMap::new(),
        vendors: BTreeMap::from([(Vendor::Claude, 1_000_000), (Vendor::Codex, 1_000_000)]),
        review_micros: 400_000,
        repair_micros: 400_000,
        canary_micros: 400_000,
        paused: false,
        extra: Extra::new(),
    };
    engine.mission_budget(Change::Grant { mission: mid.clone(), grant: budget }).map_err(|e| fail(format!("grant: {e}")))?;
    log.step("budget", json!({"capMicros": 2_000_000, "waiver": "R132"}));

    let journal = Journal::open(Path::new(&repository.identity.root), &engine_data, &mid).map_err(|e| fail(e.key()))?;
    let mut runner = Runner::start(&journal, &engine, mission).map_err(fail)?;
    runner.mission.state = MissionState::Running;
    runner.save(&journal).map_err(fail)?;
    let mut task = Task::new(WRITER, "Add src/add.js", TaskKind::Writer);
    task.state = TaskState::Review;
    journal.save_plan(&Plan::new(&mid, 1, vec![task])).map_err(|e| fail(e.key()))?;
    let mut broker = Broker::open(repository.clone(), journal.clone(), &mid, 3, Process::find().map_err(|e| fail(e.key()))?)
        .map_err(|e| fail(e.key()))?;
    broker.create_branch(|| Ok(Some(grant.clone())), "branch").map_err(|e| fail(e.key()))?;
    let lease = Lease::new(&repository.identity.common_dir, &mid, WRITER, 1, vec!["src".into()], &base, DIGEST, 0);
    let lease = broker.leases().acquire(lease).map_err(|e| fail(e.key()))?;
    let root = broker.worktree(|| Ok(Some(grant.clone())), &lease, "worktree").map_err(|e| fail(e.key()))?;
    log.step("lease", json!({"generation": lease.generation, "worktree": root.to_string_lossy()}));

    // The proof is run by the engine, never by a model: red before the patch, green after.
    let node = node()?;
    let environment: BTreeMap<String, String> =
        ["SystemRoot", "TEMP", "TMP"].iter().filter_map(|n| std::env::var(n).ok().map(|v| (n.to_string(), v))).collect();
    let definition = Definition {
        id: "node-check".into(),
        version: 1,
        program: node,
        argv: vec!["check.js".into()],
        checks: vec!["C1".into()],
        toolchain: "node".into(),
        dependencies: "none".into(),
        environment: environment.clone(),
        heavy: false,
    };
    let proof_spawn = faber_engine::proc::isolated_spawner(environment);
    let mut red = ProofSet::new(base.clone(), "seam".into(), vec![definition.clone()], vec!["src/add.js".into()]).map_err(fail)?;
    let red_receipt = runner.run_proof(&journal, WRITER, "node-check", &mut red, &root, &proof_spawn).await.map_err(fail)?;
    if red.verified() {
        return Err(fail("the base must be red"));
    }
    log.step("red-proof", json!({"exit": red_receipt.extra["exit"]}));

    // Writer on Claude.
    let write_env = envelope_example(
        WRITER,
        &base,
        lease.generation,
        json!({"patch": {"ops": [{"write": {"path": "src/add.js", "expect": null, "content": "module.exports = (a, b) => a + b;\n"}}]}}),
        None,
    );
    let quotes = vec![
        Verbatim { id: "check".into(), text: CHECK_JS.into() },
        Verbatim {
            id: "format".into(),
            text: format!(
                "Reply with exactly one JSON object and nothing else: no prose, no code fence. This is the shape, with the real code for src/add.js in content:\n{write_env}"
            ),
        },
    ];
    let w = brief(Role::Writer, "Create src/add.js so that check.js passes", &base, &["src/add.js"], Some(lease.generation), quotes, &[]);
    wire_worker(&journal, &mut runner, Role::Writer, w, Vendor::Claude, "claude-haiku-5-5", &versions[&Vendor::Claude])?;
    broker.leases().prepare_launch(&lease, "inv-writer", 3, 1).map_err(|e| fail(e.key()))?;
    let request = |id: &str, task: &str, kind: Kind, vendor: Vendor, model: &str| Invocation {
        id: id.into(),
        mission: mid.clone(),
        worker: task.into(),
        kind,
        vendor,
        model: model.into(),
        effort: "medium".into(),
        session: "unused".into(),
        fresh: true,
        exposure_micros: None,
        extra: Extra::new(),
    };
    log.sample("before-writer");
    engine
        .dispatch_mission_writer(
            journal.clone(),
            request("inv-writer", WRITER, Kind::Worker, Vendor::Claude, "claude-haiku-5-5"),
            broker.leases(),
            &lease,
        )
        .await
        .map_err(|e| fail(format!("dispatch writer: {e}")))?;
    log.step("writer-dispatched", json!({"vendor": "claude", "model": "claude-haiku-5-5"}));
    let reply = await_reply(&journal, log, "inv-writer", 300).await;
    let termination = engine.worker_termination("inv-writer").await.map_err(|e| fail(format!("writer termination: {e}")))?;
    broker.leases().settle_process(&lease, termination).map_err(|e| fail(e.key()))?;
    let reply = reply.map_err(|why| fail(format!("Writer: {why}")))?;
    let Action::Patch(patch) = &reply.action else {
        return Err(fail("the Writer's accepted envelope is not a patch"));
    };
    for op in &patch.ops {
        broker.leases().patch(&lease, Some(&grant), 3, op as &FileOp).map_err(|e| fail(format!("patch: {}", e.key())))?;
    }
    let landed_source = std::fs::read_to_string(root.join("src/add.js"))?;
    log.step("patch-applied", json!({"ops": patch.ops.len(), "src/add.js": landed_source}));

    let tree = broker.compose_tree(|| Ok(Some(grant.clone())), &lease, &["src/add.js".into()]).map_err(|e| fail(e.key()))?;
    let mut proof = ProofSet::new(tree.clone(), "seam".into(), vec![definition], vec!["src/add.js".into()]).map_err(fail)?;
    runner = Runner::load(&journal).map_err(fail)?;
    let green = runner.run_proof(&journal, WRITER, "node-check", &mut proof, &root, &proof_spawn).await.map_err(fail)?;
    log.step("green-proof", json!({"exit": green.extra["exit"], "verified": proof.verified(), "tree": tree}));
    if !proof.verified() {
        return Err(fail("the patched tree does not prove green"));
    }

    // Reviewer on Codex, fresh session, shown the exact candidate.
    let verdict_env = envelope_example(
        REVIEWER,
        &base,
        0,
        json!({"complete": {"summary": "The candidate is correct and minimal.", "verdict": "pass", "next": "land", "receipt": green.id, "findings": []}}),
        Some("DoneReady"),
    );
    let quotes = vec![
        Verbatim { id: "check".into(), text: CHECK_JS.into() },
        Verbatim {
            id: "candidate".into(),
            text: format!(
                "src/add.js (new file):\n{landed_source}\nThe engine ran check.js on this tree and it exited 0. Engine receipt id: {}.",
                green.id
            ),
        },
        Verbatim {
            id: "format".into(),
            text: format!(
                "Reply with exactly one JSON object and nothing else: no prose, no code fence. The only keys allowed at the top level are schema, task, attempt, base, contract, lease, ping, action and note; any other key (status, result, kit, proofs, checks, assumptions) makes the reply invalid. The value of contract is a plain string: the digest from the contract field of the brief header, not an object. Copy this shape exactly, character for character, changing only the text of summary and, if you find a defect, verdict (\"block\") and findings. Keep lease 0, keep ping \"DoneReady\" and keep the receipt id; action is an object, never a string.\n{verdict_env}"
            ),
        },
    ];
    let r = brief(Role::Reviewer, "Review the candidate src/add.js against check.js", &base, &[], None, quotes, &[]);
    runner = Runner::load(&journal).map_err(fail)?;
    wire_worker(&journal, &mut runner, Role::Reviewer, r, Vendor::Codex, "gpt-6-luna", &versions[&Vendor::Codex])?;
    log.sample("before-reviewer");
    engine
        .dispatch_mission_worker(journal.clone(), request("inv-reviewer", REVIEWER, Kind::Review, Vendor::Codex, "gpt-6-luna"))
        .await
        .map_err(|e| fail(format!("dispatch reviewer: {e}")))?;
    log.step("reviewer-dispatched", json!({"vendor": "codex", "model": "gpt-6-luna"}));
    let review = await_reply(&journal, log, "inv-reviewer", 300).await;
    engine.finish_role("inv-reviewer").await.map_err(|e| fail(format!("reviewer end: {e}")))?;
    let review = review.map_err(|why| fail(format!("Reviewer: {why}")))?;
    let Action::Complete(done) = &review.action else {
        return Err(fail("the Reviewer's accepted envelope is not a completion"));
    };
    let accepted = done.verdict == Some(faber_engine::project::wire::Verdict::Pass);
    log.step("review", json!({"verdict": done.verdict, "findings": done.findings, "summary": done.summary}));

    // Landing: the engine's queue refuses without proof and a fresh other-vendor review of the same tree.
    runner = Runner::load(&journal).map_err(fail)?;
    runner.mission.state = MissionState::Running;
    let candidate = Candidate::new(
        WRITER,
        base.clone(),
        tree.clone(),
        3,
        Vendor::Claude,
        "seam".into(),
        vec!["src/add.js".into()],
        proof.definitions().to_vec(),
    )
    .map_err(fail)?;
    runner.queue.enqueue(candidate).map_err(fail)?;
    let digest = proof.digest();
    runner.queue.prove(WRITER, proof).map_err(fail)?;
    runner
        .queue
        .review(
            WRITER,
            Review {
                vendor: Vendor::Codex,
                tree: tree.clone(),
                proof_digest: digest,
                session: "seam-fresh-review".into(),
                fresh: true,
                accepted,
                findings: done.findings.clone(),
            },
        )
        .map_err(fail)?;
    if !accepted {
        return Err(fail("the Reviewer did not pass the candidate"));
    }
    let head =
        runner.land(&mut broker, WRITER, &lease, || Ok(Some(grant.clone())), "Add src/add.js", termination, |_| Ok(())).map_err(fail)?;
    let shown = git(&git_program, &repo, &["show", &format!("faber/{mid}:src/add.js")]).unwrap_or_default();
    log.step("landed", json!({"head": head, "ref": format!("faber/{mid}"), "content": shown, "ownerHead": git(&git_program, &repo, &["rev-parse", "HEAD"])?}));
    broker.finish().map_err(|e| fail(e.key()))?;

    // Settlement and calibration, then the spend.
    let calibration = engine.calibrate_mission(&mid).await;
    log.step("calibration", json!({"status": format!("{calibration:?}")}));
    if !matches!(calibration, Ok(faber_engine::project::budget::Status::Ready)) {
        return Err(fail(format!("calibration did not complete: {calibration:?}")));
    }
    let spent = Budget::load(&engine.ledger()).map_err(fail)?;
    let invocations: Value = spent
        .invocations
        .iter()
        .map(|(id, r)| (id.clone(), json!({"vendor": r.request.vendor, "model": r.request.model, "paidMicros": r.paid_micros, "settled": r.settled, "bounded": r.bounded})))
        .collect();
    let total: u64 = spent.invocations.values().map(|r| r.paid_micros).sum();
    log.step("spend", json!({"totalMicros": total, "invocations": invocations}));
    log.sample("done");
    faber_engine::project::budget::r132::close();
    engine.shutdown();
    Ok(())
}
