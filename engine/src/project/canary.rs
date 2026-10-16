//! The miniature mission a candidate build runs on itself before it may be promoted.
//!
//! The candidate, in its own inactive slot, takes the frozen canary goal (`tests/fixtures/projects/canary`: one failing
//! unit test of a tiny crate) through the production coordinator: a Writer fixes it, the engine proves the exact tree
//! with `cargo test --offline`, a fresh role of the other vendor reviews it, and the mission lands on its own ref in a
//! throwaway repository. Two modes exist:
//!
//! - `Offline` replays recorded replies through the same coordinator, journal, lease, proof, queue and git calls. It
//!   proves the build can run a mission end to end without a provider, and it never promotes anything.
//! - `Real` runs the Writer and the Reviewer on the installed CLIs, with the spending admitted by the engine's own
//!   ledger. Only this mode can earn a pass, and only after `verify` read the journal and the ledger of the run.
//!
//! The child that ran the mission reports evidence; the supervisor never takes its word. `verify` reads the mission's
//! journal and spending itself.
use super::{
    GitMode, Grant, Lease, Mission, MissionState, Task, TaskKind,
    budget::{Budget, Change, Grant as Budgeted, Invocation},
    coordinator::{Coordinator, Outcome, Pins, Roles, Setup, Stop, deliver, recorded_reply},
    git::{Broker, Process, enroll},
    lease::{Leases, Termination},
    proof::Definition,
    run::Runner,
    store::Journal,
    wire::Envelope,
    worker::ModelPin,
};
use crate::engine::Engine;
use crate::model::Vendor;
use crate::proc::{OwnedTree, Proc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The frozen files of the canary repository, byte for byte. The supervisor lays the same bytes out beside the
/// candidate and the child refuses to run when they differ from what it carries.
pub const FILES: [(&str, &[u8]); 4] = [
    ("Cargo.toml", include_bytes!("../../../tests/fixtures/projects/canary/Cargo.toml")),
    ("Cargo.lock", include_bytes!("../../../tests/fixtures/projects/canary/Cargo.lock")),
    ("src/lib.rs", include_bytes!("../../../tests/fixtures/projects/canary/src/lib.rs")),
    ("goal.json", include_bytes!("../../../tests/fixtures/projects/canary/goal.json")),
];
const REPLAY_WRITER: &str = include_str!("../../../tests/fixtures/projects/canary/replay-writer.json");
const REPLAY_REVIEWER: &str = include_str!("../../../tests/fixtures/projects/canary/replay-reviewer.json");

const REVISION: u64 = 3;
const TASK: &str = "writer";
const PROOF: &str = "canary-test";
/// The ceiling one real mission is admitted under, in millionths of an API-equivalent dollar. Admission reserves the
/// most one turn could cost (a Claude Writer about 0.67 dollars, a Codex Reviewer 0.31) and settles to what it really
/// used, so a smaller ceiling could not admit even one turn. The ceiling is not spending: the settled cost of a small
/// mission is a few cents.
pub const REAL_CAP_MICROS: u64 = 1_110_000;
/// What a Codex turn may reserve at most: the share of the ceiling kept for the Reviewer.
const REVIEW_SHARE_MICROS: u64 = 310_000;
const WRITER_MODEL: (Vendor, &str) = (Vendor::Claude, "claude-haiku-5-5");
const REVIEWER_MODEL: (Vendor, &str) = (Vendor::Codex, "gpt-6-luna");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Offline,
    Real,
}

/// What the child says it did. Nothing here is believed: `verify` reads the journal and the ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evidence {
    pub mode: Mode,
    pub mission: String,
    pub repo_root: String,
    pub millis: u64,
}

/// What `verify` found in the journal and the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    pub mode: Mode,
    pub mission: String,
    pub head: String,
    pub tree: String,
    pub writer: Vendor,
    pub reviewer: Vendor,
    /// The engine's receipt of the proof that ran on the landed tree.
    pub proof_receipt: String,
    /// Settled provider spending of the run (zero for a replay).
    pub spent_micros: u64,
}

fn key(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Lay the frozen files out under `dir`. Existing files with other bytes are replaced.
pub fn lay_out(dir: &Path) -> Result<(), String> {
    for (name, bytes) in FILES {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(key)?;
        }
        std::fs::write(&path, bytes).map_err(key)?;
    }
    Ok(())
}

/// The files beside the candidate are exactly the frozen ones.
pub fn laid_out(dir: &Path) -> bool {
    FILES.iter().all(|(name, bytes)| std::fs::read(dir.join(name)).is_ok_and(|found| found == *bytes))
}

fn git(program: &Path, cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = std::process::Command::new(program);
    command
        .current_dir(cwd)
        .args(["-c", "core.hooksPath=NUL", "-c", "core.fsmonitor=false", "-c", "commit.gpgSign=false"])
        .args(args)
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "NUL")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for name in ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().map_err(key)?;
    if !output.status.success() {
        return Err("reason.canary.git".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// A throwaway repository holding the frozen crate as its only commit.
fn prepare(work: &Path) -> Result<PathBuf, String> {
    let repo = work.join("repo");
    if repo.exists() {
        return Err("reason.canary.workExists".into());
    }
    std::fs::create_dir_all(&repo).map_err(key)?;
    let program = PathBuf::from(crate::cli::find_cli("git").ok_or("reason.canary.git")?.command);
    for (name, bytes) in FILES.iter().filter(|(name, _)| *name != "goal.json") {
        let path = repo.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(key)?;
        }
        std::fs::write(path, bytes).map_err(key)?;
    }
    git(&program, &repo, &["init", "--initial-branch=owner"])?;
    git(&program, &repo, &["config", "user.name", "Faber canary"])?;
    git(&program, &repo, &["config", "user.email", "canary@example.invalid"])?;
    git(&program, &repo, &["add", "--", "Cargo.toml", "Cargo.lock", "src/lib.rs"])?;
    git(&program, &repo, &["commit", "--no-gpg-sign", "-m", "canary base"])?;
    Ok(repo)
}

/// A cargo proof the engine runs for a mission: the named test arguments, offline and locked, with only the environment a
/// Rust build needs. Everything it may write lives under `own`, a folder of the mission outside the checkout: the build
/// folder (`target`), its own cargo home and npm cache, and its temporary folder. The proof spawner lets it write there and
/// in the mission clone only (R138). `heavy` gives it the time of a cold compile.
pub fn cargo_proof(id: &str, test_args: &[&str], own: &Path, heavy: bool) -> Result<(Definition, crate::proc::Spawner), String> {
    let cargo = PathBuf::from(crate::cli::find_cli("cargo").ok_or("reason.canary.cargo")?.command);
    let mut environment = BTreeMap::new();
    for name in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "USERPROFILE",
        "HOME",
        "ProgramData",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "RUSTUP_HOME",
        "LIB",
        "INCLUDE",
        "LIBPATH",
        "VCINSTALLDIR",
        "VCToolsInstallDir",
    ] {
        if let Ok(value) = std::env::var(name) {
            environment.insert(name.to_string(), value);
        }
    }
    let folders = ["target", "cargo-home", "npm-cache", "tmp"].map(|name| own.join(name));
    let [target, cargo_home, npm_cache, tmp] = folders.clone().map(|path| path.to_string_lossy().into_owned());
    for (name, value) in [
        ("CARGO_TARGET_DIR", target),
        ("CARGO_HOME", cargo_home),
        ("npm_config_cache", npm_cache),
        ("TEMP", tmp.clone()),
        ("TMP", tmp.clone()),
        ("TMPDIR", tmp),
    ] {
        environment.insert(name.into(), value);
    }
    environment.insert("CARGO_INCREMENTAL".into(), "0".into());
    environment.insert("CARGO_NET_OFFLINE".into(), "true".into());
    let definition = Definition {
        id: id.into(),
        version: 1,
        program: cargo,
        argv: ["test", "--offline", "--locked"].into_iter().chain(test_args.iter().copied()).map(String::from).collect(),
        checks: vec!["C1".into()],
        toolchain: "rust".into(),
        dependencies: "none".into(),
        environment: environment.clone(),
        heavy,
    };
    Ok((definition, crate::proc::isolated_spawner(environment, folders.to_vec())))
}

/// The one proof of the canary: the named test of the frozen crate.
fn proof_definition(work: &Path) -> Result<(Definition, crate::proc::Spawner), String> {
    cargo_proof(PROOF, &["trims_and_lowercases"], &work.join("proof"), false)
}

fn goal() -> Result<String, String> {
    let (_, bytes) = FILES.iter().find(|(name, _)| *name == "goal.json").ok_or("reason.canary.goal")?;
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(key)?;
    value["goal"].as_str().map(str::to_string).ok_or_else(|| "reason.canary.goal".into())
}

fn pin(vendor: Vendor, model: &str, version: &str) -> ModelPin {
    ModelPin { vendor, model: model.into(), effort: "medium".into(), cli_version: version.into() }
}

fn cli(vendor: Vendor) -> Option<String> {
    crate::cli::find_cli(vendor.name()).filter(|c| c.prefix.is_empty()).map(|c| c.command)
}

/// The version of a CLI the receipt admits, through the same pinned, hash-first path as a role launch: a program whose hash
/// is not the receipt's is never asked for `--version`.
fn cli_version(vendor: Vendor, bin: &dyn Fn() -> Option<String>) -> Result<String, String> {
    let text = super::qualification::admitted(vendor, bin).map(|pinned| pinned.version).map_err(|_| "error.cli.missing".to_string())?;
    version_token(&text).ok_or_else(|| "error.cli.missing".to_string())
}

/// A CLI prints a sentence ("2.1.296 (Claude Code)", "codex-cli 0.162.1"); a model pin carries only the version token.
fn version_token(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|t| t.bytes().any(|b| b.is_ascii_digit()) && t.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)))
        .map(str::to_string)
}

#[cfg(test)]
mod version_tests {
    #[test]
    fn a_pin_takes_the_version_token_from_the_cli_sentence() {
        assert_eq!(super::version_token("2.1.296 (Claude Code)").as_deref(), Some("2.1.296"));
        assert_eq!(super::version_token("codex-cli 0.162.1").as_deref(), Some("0.162.1"));
        assert_eq!(super::version_token("no version here"), None);
    }

    /// Review 3, #1: the real canary takes the CLI version through the receipt's pinned, hash-first admission, so with no
    /// admitting receipt the CLI is not even looked up, let alone asked for `--version`.
    #[test]
    fn the_canary_asks_no_cli_its_version_before_a_receipt_admits_it() {
        let looked = std::cell::Cell::new(false);
        let found = || {
            looked.set(true);
            Some(std::env::current_exe().unwrap().to_string_lossy().into_owned())
        };
        assert!(super::cli_version(super::Vendor::Claude, &found).is_err());
        assert!(!looked.get(), "the CLI was looked up before a receipt admitted it");
    }
}

/// The fixed plan of the frozen goal: one Writer task on `src/lib.rs`, proven by the canary test.
fn fixed_plan(proof: &Definition) -> Task {
    let mut task = Task::new(TASK, "Fix normalize so the named proof passes", TaskKind::Writer);
    task.writes = vec!["src/lib.rs".into()];
    task.paths = task.writes.clone();
    task.proofs = vec![proof.id.clone()];
    task.checks = proof.checks.clone();
    task
}

/// Fresh account readings: provider readings are stamped to the whole second and admission rejects one from the future.
pub async fn read_accounts(engine: &Engine) {
    engine.check_plans(Duration::from_secs(20)).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
}

/// The cost grant of one real mission under a ceiling: the Reviewer's turn on Codex has its own share, every other
/// turn runs on Claude within the rest.
pub fn mission_grant(cap_micros: u64) -> Budgeted {
    let claude = cap_micros.saturating_sub(REVIEW_SHARE_MICROS);
    Budgeted {
        cap_micros,
        worker_micros: claude,
        kind_caps: BTreeMap::new(),
        vendors: BTreeMap::from([(Vendor::Claude, claude), (Vendor::Codex, REVIEW_SHARE_MICROS.min(cap_micros))]),
        review_micros: REVIEW_SHARE_MICROS.min(cap_micros),
        repair_micros: 0,
        canary_micros: 0,
        paused: false,
        extra: super::Extra::new(),
    }
}

/// The caps one real mission is admitted under (after `read_accounts`).
pub fn caps(engine: &Engine, mission: &str, cap_micros: u64) -> Result<(), String> {
    // R132/R136 (harness builds only): these allocations are made for this mission, the only one a waiver can cover.
    #[cfg(feature = "r132-harness")]
    let _for = super::budget::r132::for_mission(mission);
    for (vendor, model) in [WRITER_MODEL, REVIEWER_MODEL] {
        engine.mission_budget(Change::AllocateModel { vendor, model: model.into(), cap_micros })?;
    }
    engine.mission_budget(Change::Grant { mission: mission.into(), grant: mission_grant(cap_micros) })?;
    Ok(())
}

/// Run the miniature mission in `work` (which must not exist yet) on the engine whose data folder is `engine_data`.
pub async fn run(engine: &Engine, engine_data: &Path, work: &Path, mode: Mode) -> Result<Evidence, String> {
    let started = Instant::now();
    std::fs::create_dir_all(work).map_err(key)?;
    let repo = prepare(work)?;
    let mut process = Process::find().map_err(|e| e.key().to_string())?;
    let repository = enroll(&mut process, &repo, "HEAD").map_err(|e| e.key().to_string())?;
    let mut grant = Grant::new(&repository.identity.common_dir, GitMode::Seguro, REVISION, 1);
    grant.author = repository.author.clone();
    let mut project = engine.add_project(&repo.to_string_lossy());
    let before = project.clone();
    project.repo = Some(repository.identity.clone());
    project.grant = Some(grant.clone());
    engine.update_project(&before, &project)?;
    let mission: Mission = engine.new_mission(&project, &goal()?, &repository.base)?;
    let mid = mission.id.clone();

    let (definition, spawn) = proof_definition(work)?;
    let sources: Vec<(String, String)> = FILES
        .iter()
        .filter(|(name, _)| *name == "src/lib.rs")
        .map(|(name, bytes)| (name.to_string(), String::from_utf8_lossy(bytes).into_owned()))
        .collect();
    let pins = match mode {
        Mode::Offline => {
            let (writer, reviewer) = (pin(WRITER_MODEL.0, WRITER_MODEL.1, "0.0.0"), pin(REVIEWER_MODEL.0, REVIEWER_MODEL.1, "0.0.0"));
            Pins { lead: writer.clone(), writer, reviewer }
        }
        Mode::Real => {
            let writer = pin(WRITER_MODEL.0, WRITER_MODEL.1, &cli_version(WRITER_MODEL.0, &|| cli(WRITER_MODEL.0))?);
            let reviewer = pin(REVIEWER_MODEL.0, REVIEWER_MODEL.1, &cli_version(REVIEWER_MODEL.0, &|| cli(REVIEWER_MODEL.0))?);
            Pins { lead: writer.clone(), writer, reviewer }
        }
    };
    if mode == Mode::Real {
        read_accounts(engine).await;
        // R136: only a build with the qualification harness feature has this code; a product build never waives.
        #[cfg(feature = "r132-harness")]
        arm_waiver(engine_data, &mid);
        caps(engine, &mid, REAL_CAP_MICROS)?;
        #[cfg(feature = "r132-harness")]
        keep_waivers(engine, engine_data)?;
    }
    let journal = Journal::open(Path::new(&repository.identity.root), engine_data, &mid).map_err(|e| e.key().to_string())?;
    // A replay has no provider to qualify, so only a real run goes through the admission of a live mission.
    match mode {
        Mode::Offline => Runner::create(&journal, mission).map_err(str::to_string)?,
        Mode::Real => Runner::start(&journal, engine, mission)?,
    };
    let broker = Broker::open(repository.clone(), journal.clone(), &mid, REVISION, process).map_err(|e| e.key().to_string())?;
    let setup = Setup {
        pins,
        proof: definition.clone(),
        proof_spawn: spawn,
        sources,
        writable: vec!["src".into()],
        grant: Arc::new(move || Ok(Some(grant.clone()))),
        revision: REVISION,
        micros: 900_000,
        turn_limit: Duration::from_secs(300),
        plan: Some(fixed_plan(&definition)),
    };
    let outcome = match mode {
        Mode::Offline => {
            let roles = Replay::new(PathBuf::from(crate::cli::find_cli("git").ok_or("reason.canary.git")?.command));
            Coordinator::new(roles, journal, broker, setup)?.drive().await
        }
        Mode::Real => Coordinator::new(engine.clone(), journal, broker, setup)?.drive().await,
    };
    #[cfg(feature = "r132-harness")]
    super::budget::r132::close();
    #[cfg(feature = "r132-harness")]
    if mode == Mode::Real {
        keep_waivers(engine, engine_data)?;
    }
    match outcome {
        Outcome::Done { .. } => Ok(Evidence {
            mode,
            mission: mid,
            repo_root: repository.identity.root.clone(),
            millis: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        }),
        Outcome::Stopped(Stop::Wait(k) | Stop::Park(k) | Stop::Block(k)) => Err(k),
        Outcome::Stopped(Stop::Crash) => Err("reason.canary.crash".into()),
    }
}

/// R136: the real canary mission of a harness-built candidate may be admitted once without the two-point margin. Each
/// run deletes the canary's data folder, so the marker and the waiver records live one folder up, in the canary folder
/// that survives: the marker has its one fixed path there and is created once (a later run finds it and is refused),
/// the waiver is bound to this mission id and its allocations, the ledger records are kept there after every budget step,
/// and the waiver closes when the mission ends. A product build has no such code and reads no variable for it.
#[cfg(feature = "r132-harness")]
fn arm_waiver(engine_data: &Path, mission: &str) {
    use super::budget::r132::{Exception, waive};
    let evidence = serde_json::json!({"exception": "R136", "for": "canary mission of a harness-built candidate"});
    let _ = waive(Exception::R136, kept_root(engine_data), mission, &evidence);
}

/// The canary folder that survives the deletion of its data folder.
#[cfg(feature = "r132-harness")]
fn kept_root(engine_data: &Path) -> &Path {
    engine_data.parent().unwrap_or(engine_data)
}

#[cfg(feature = "r132-harness")]
const KEPT: &str = "waiver-ledger.json";

/// Copies the engine ledger's waiver records out of the data folder; a failure stops the mission (fail closed).
#[cfg(feature = "r132-harness")]
fn keep_waivers(engine: &Engine, engine_data: &Path) -> Result<(), String> {
    super::budget::r132::save_records(&engine.ledger(), &kept_root(engine_data).join(KEPT)).map_err(key)
}

/// Prepares the canary's data folder before its engine starts. R136 (harness builds only): a real canary starts from a
/// ledger of its own, keeping only the waiver records (the seeded ones and those kept outside the folder). A product
/// build changes nothing.
pub fn prepare_data(engine_data: &Path, mode: Mode) -> Result<(), String> {
    #[cfg(feature = "r132-harness")]
    if mode == Mode::Real {
        super::budget::r132::fresh_ledger(engine_data, &kept_root(engine_data).join(KEPT)).map_err(key)?;
    }
    let _ = (engine_data, mode);
    Ok(())
}

/// Read the mission's journal and ledger and say what really happened. The Landing must be a Done mission with a
/// proven exact tree and a fresh review by the other vendor; a real run must also have settled provider spending
/// for both vendors, which only the engine's own dispatch writes.
pub fn verify(evidence: &Evidence, engine_data: &Path) -> Result<Verified, String> {
    let journal = Journal::open(Path::new(&evidence.repo_root), engine_data, &evidence.mission).map_err(|e| e.key().to_string())?;
    let runner = Runner::load(&journal)?;
    if runner.mission.state != MissionState::Done || !journal.pending().map_err(|e| e.key().to_string())?.is_empty() {
        return Err("reason.canary.notDone".into());
    }
    let [landing] = runner.landings.as_slice() else { return Err("reason.canary.landing".into()) };
    let committed = journal.effect_receipt(&landing.commit_intent).map_err(|e| e.key().to_string())?.ok_or("reason.receipt.missing")?;
    let head = committed.evidence.get("head").cloned().ok_or("reason.receipt.missing")?;
    let tree = committed.evidence.get("tree").cloned().ok_or("reason.receipt.missing")?;
    runner.accepted_landing(&journal, &head, &tree)?;
    let candidate = &landing.candidate;
    let review = candidate.review().ok_or("reason.queue.review")?;
    let proof = candidate.proof().filter(|p| p.verified()).ok_or("reason.queue.proof")?;
    let proof_receipt = proof.receipts().next().ok_or("reason.queue.proof")?.id.clone();
    if !review.fresh || !review.accepted || review.vendor == candidate.writer {
        return Err("reason.queue.review".into());
    }
    let spent = {
        let ledger = crate::store::Store::new(engine_data).ledger_checked().map_err(|e| e.key().to_string())?;
        let budget = Budget::load(&ledger)?;
        let mine: Vec<_> = budget.invocations.values().filter(|r| r.request.mission == evidence.mission).collect();
        if evidence.mode == Mode::Real {
            let paid = |vendor: Vendor| {
                mine.iter().any(|r| r.request.vendor == vendor && r.settled && r.terminated && !r.bounded && r.paid_micros > 0)
            };
            if !paid(candidate.writer) || !paid(review.vendor) {
                return Err("reason.canary.notReal".into());
            }
        }
        mine.iter().map(|r| r.paid_micros).sum()
    };
    Ok(Verified {
        mode: evidence.mode,
        mission: evidence.mission.clone(),
        head,
        tree,
        writer: candidate.writer,
        reviewer: review.vendor,
        proof_receipt,
        spent_micros: spent,
    })
}

/// Recorded replies for the Writer and the Reviewer. The Writer's session is a real process this engine owns (git
/// asked for its version), so the lease binds to a genuine closed tree exactly as it does for a provider session.
struct Replay {
    git: PathBuf,
    live: Mutex<HashSet<String>>,
    trees: Mutex<HashMap<String, (OwnedTree, Proc)>>,
}

impl Replay {
    fn new(git: PathBuf) -> Self {
        Self { git, live: Mutex::default(), trees: Mutex::default() }
    }
}

impl Roles for Replay {
    async fn start(&self, journal: &Journal, request: Invocation, writer: Option<(&mut Leases, &Lease)>) -> Result<(), String> {
        self.live.lock().map_err(|_| "reason.canary.replay")?.insert(request.id.clone());
        if let Some((leases, lease)) = writer {
            let stand_in =
                crate::proc::real_spawner()(&self.git.to_string_lossy(), &["--version".into()], journal.private()).map_err(key)?;
            let tree = stand_in.contained.as_ref().and_then(|c| c.owned_tree()).ok_or("reason.project.processTreeUnproven")?;
            leases.attach_process(lease, tree.identity()).map_err(|e| e.key().to_string())?;
            self.trees.lock().map_err(|_| "reason.canary.replay")?.insert(request.id.clone(), (tree, stand_in));
        }
        let recorded: serde_json::Value =
            serde_json::from_str(if request.worker == TASK { REPLAY_WRITER } else { REPLAY_REVIEWER }).map_err(key)?;
        let runner = Runner::load(journal)?;
        let brief = &runner.workers.get(&request.worker).ok_or("error.wire.stale")?.brief;
        let text = recorded_reply(brief, &recorded).map_err(str::to_string)?;
        deliver(journal, &request.worker, &request.id, &text)
    }

    async fn wait(&self, journal: &Journal, invocation: &str) -> Result<Envelope, String> {
        if let Some(reply) = journal.reply(invocation).map_err(|e| e.key().to_string())? {
            return Ok(reply);
        }
        Err(journal.refusal(invocation).map_err(|e| e.key().to_string())?.unwrap_or_else(|| "reason.stall".into()))
    }

    fn live(&self, invocation: &str) -> bool {
        self.live.lock().is_ok_and(|live| live.contains(invocation))
    }

    async fn settle(&self, invocation: &str) -> Result<Option<Termination>, String> {
        let tree = self.trees.lock().map_err(|_| "reason.canary.replay")?.remove(invocation);
        self.live.lock().map_err(|_| "reason.canary.replay")?.remove(invocation);
        match tree {
            Some((tree, _stand_in)) => {
                let retired = tree.retire().await.map_err(key)?;
                Ok(Some(Termination::from_retired(&retired, true)))
            }
            None => Ok(None),
        }
    }

    async fn abandon(&self, _journal: &Journal, _invocation: &str) -> Result<(), String> {
        Ok(())
    }

    fn pause(&self, _mission: &str) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frozen_files_are_the_ones_the_goal_names() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!laid_out(dir.path()));
        lay_out(dir.path()).unwrap();
        assert!(laid_out(dir.path()));
        std::fs::write(dir.path().join("src/lib.rs"), "changed").unwrap();
        assert!(!laid_out(dir.path()));
        assert!(goal().unwrap().contains("normalize"));
    }

    #[test]
    fn the_recorded_replies_are_whole_files_for_the_one_leased_path() {
        let writer: serde_json::Value = serde_json::from_str(REPLAY_WRITER).unwrap();
        let op = &writer["action"]["patch"]["ops"][0]["write"];
        assert_eq!(op["path"], "src/lib.rs");
        let content = op["content"].as_str().unwrap();
        assert!(content.contains("trim().to_lowercase()") && content.contains("fn trims_and_lowercases"));
        let reviewer: serde_json::Value = serde_json::from_str(REPLAY_REVIEWER).unwrap();
        assert!(reviewer["action"]["complete"]["summary"].is_string());
    }
}
