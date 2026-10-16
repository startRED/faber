//! The production coordinator of a mission. It is event-driven (it sleeps on the engine's wake signal, never on a
//! timer) and it reads every worker reply envelope. For a one-task mission it drives, in order: the Lead's plan, a
//! lease on the mission branch, the engine's red proof, the Writer's patch, the engine's green proof, a fresh review
//! of the exact tree by a role of the other vendor, the landing on the mission ref, and the report.
//!
//! Nothing here is trusted state. Every step is either a durable journal fact (a reply, a check receipt, an effect
//! receipt, a queue entry) or a small progress record that only says which steps already finished. A step checks the
//! facts before it acts, so a restart after a crash at any point resumes without repeating an effect. What cannot be
//! proven from the facts is never guessed: a pending effect, a Writer whose turn was cut short and a lease that is
//! still held park the mission for a person.
//!
//! The lessons of building Faber by hand are engine rules here (R146, R147):
//!
//! - **The contract comes first.** The plan names the map areas the change touches, the closest thing that exists, and
//!   `Extends:` or `New:` with why. No Writer starts without them, and the change may not touch an area they omit.
//! - **Sized by risk.** The engine measures the paths the change really touched against the project's rules: one area
//!   and no security path takes one review, anything wider two reviews from different sessions.
//! - **Faber decides done.** Only the engine's proof of the exact composed tree, plus the reviews, opens the queue.
//! - **Reviews that end.** Each finding carries a class; only a break for a person, for data, for security or for a
//!   written rule blocks, and each round reads what earlier rounds decided. A finding with no class stops for a person.
//! - **One repair.** A blocking finding or a red proof gives the Writer one repair turn from a self-contained brief.
//! - **A red base stops the line.** When the candidate is red and so is the head under it, the head is what is broken:
//!   the line stops, this Writer owns the base's repair, and nothing else lands until it does.
//! - **Numbers at compose.** Decision and notes numbers are placeholders until the engine applies the patch.
//!
//! Limits of this version, kept on purpose: one task per plan and the landing gate is the exact-tree queue (proof plus
//! other-vendor reviews), not the full deliverable bar.
use super::{
    Brief, Contract, ContractPin, Extra, Grant, Lease, MissionState, Plan, Record, Role, SCHEMA, Task, TaskKind,
    budget::{Invocation, Kind},
    git::{Broker, Executor, GitError},
    kit::{Assembly, KitReceipt, Machine, assemble, inventory::Inventory},
    lease::{LeaseError, Leases, Termination, hash, normalized, retirement_known},
    proof::{Definition, ProofSet},
    queue::{Candidate, Review},
    rules::{Coherence, Rules, Sizing},
    run::Runner,
    sequence,
    store::{Journal, JournalError},
    wire::{Action, Envelope, Facets, FileOp, Finding, Fresh, Labels, Risk, Verb, Verbatim, Verdict},
    worker::{ModelPin, Worker},
};
use crate::model::Vendor;
use crate::proc::Spawner;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

const LEAD: &str = "lead";
const REVIEWER: &str = "reviewer";
const CONTRACT: &str = "c1";
const BRANCH_INTENT: &str = "branch";
const WORKTREE_INTENT: &str = "worktree";
/// Repair turns a Writer gets after its first attempt: one focused repair (MVP spec §7).
const REPAIRS: u32 = 1;
/// Review rounds a candidate gets in all: two for a full change, and two more after its one repair.
const ROUNDS: u32 = 4;
/// The largest context a repair may resume instead of starting fresh, in tokens.
const SMALL_CONTEXT: u64 = 40_000;

/// How far each vendor is behind its quota pace (more is further behind, so more to spare), or `None` when unknown.
pub type Pace = Arc<dyn Fn(Vendor) -> Option<f64> + Send + Sync>;

/// The reviewer of a round. The first round is the product's pin; later rounds go to the eligible pin whose vendor is
/// furthest behind its pace (R119 notes: spend what is spare on re-reviews). A pin of the Writer's vendor is never
/// eligible (MVP spec §9), so with two vendors every round stays on the other one.
pub fn pick_reviewer(first: &ModelPin, pool: &[ModelPin], writer: Vendor, round: u32, pace: Option<&Pace>) -> ModelPin {
    if round <= 1 {
        return first.clone();
    }
    let room = |pin: &ModelPin| pace.and_then(|pace| pace(pin.vendor)).unwrap_or(f64::MIN);
    std::iter::once(first)
        .chain(pool)
        .filter(|pin| pin.vendor != writer)
        .fold(None::<&ModelPin>, |best, pin| match best {
            Some(b) if room(b) >= room(pin) => Some(b),
            _ => Some(pin),
        })
        .unwrap_or(first)
        .clone()
}

/// Whether a repair starts a fresh session from its self-contained brief. Only a Writer whose cache is still warm and
/// whose context is known to be small resumes; anything unknown starts fresh.
pub fn repair_fresh(warm: bool, context_tokens: Option<u64>) -> bool {
    !(warm && context_tokens.is_some_and(|t| t <= SMALL_CONTEXT))
}

/// One finding a review round returned and what the engine made of it, so the next round does not argue it again.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Decided {
    round: u32,
    finding: Finding,
    blocks: bool,
}

/// What the Writer's next repair turn is for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repair {
    /// `review`, `proof` (the candidate is red on a green head) or `base` (the head itself is red).
    why: String,
    #[serde(default)]
    findings: Vec<Finding>,
    /// The engine receipts the repair answers to.
    #[serde(default)]
    receipts: Vec<String>,
}

/// What the coordinator needs from the model sessions. The engine implements it for production; a recorded script
/// implements it for the tests, so the whole mission can be replayed without a CLI.
pub trait Roles {
    /// Start one turn. The invocation is admitted against the budget here, and a Writer's launch is bound to its lease.
    fn start(
        &self,
        journal: &Journal,
        request: Invocation,
        writer: Option<(&mut Leases, &Lease)>,
    ) -> impl Future<Output = Result<(), String>> + Send;
    /// The accepted reply of the invocation, or the refusal key when the turn ended without one. Wakes on events.
    fn wait(&self, journal: &Journal, invocation: &str) -> impl Future<Output = Result<Envelope, String>> + Send;
    /// Whether the invocation's session is alive in this process.
    fn live(&self, invocation: &str) -> bool;
    /// Close the role and settle its spending. `None` means it was already settled earlier.
    fn settle(&self, invocation: &str) -> impl Future<Output = Result<Option<Termination>, String>> + Send;
    /// Settle a turn whose process is gone (its reply never came) at its computed bound.
    fn abandon(&self, journal: &Journal, invocation: &str) -> impl Future<Output = Result<(), String>> + Send;
    /// Close admission for the mission's spending while it waits.
    fn pause(&self, mission: &str);
}

/// The model each role runs on. The reviewer must belong to the other vendor than the Writer.
#[derive(Debug, Clone)]
pub struct Pins {
    pub lead: ModelPin,
    pub writer: ModelPin,
    pub reviewer: ModelPin,
}

/// Everything the product supplies; nothing in it comes from a model.
pub struct Setup {
    pub pins: Pins,
    /// The trusted proofs, in the order they run: the commands the project declares and its owner confirmed (R137), run
    /// by the engine in the mission clone and never described by a model. A candidate is green when all of them pass.
    pub proofs: Vec<Definition>,
    /// Whether the base must fail the proofs before any patch. A canary and a harness mission have a failing test to fix;
    /// a project's own goal usually does not, and its proof is then a regression gate, not a before and after.
    pub expect_red: bool,
    /// Every path the roles may be told about (the tracked files minus the refused ones), beyond the file texts in `sources`.
    pub tree: Vec<String>,
    pub proof_spawn: Spawner,
    /// Files the roles are shown, as (clean path, text), most useful first. The roles run in an empty folder, so this and
    /// `tree` are all they see.
    pub sources: Vec<(String, String)>,
    /// Path prefixes the Lead may plan to write under.
    pub writable: Vec<String>,
    /// Read for every effect: the current authority, never a snapshot.
    pub grant: Arc<dyn Fn() -> Result<Option<Grant>, GitError> + Send + Sync>,
    pub revision: u64,
    /// The cost grant of one turn, in millionths of an API-equivalent dollar.
    pub micros: u64,
    /// How long a turn may take before the mission stops with `reason.stall`.
    pub turn_limit: Duration,
    /// A plan fixed by the product instead of a Lead turn. Only the frozen canary goal has one.
    pub plan: Option<Task>,
    /// Called after every finished step and every stop, so the screen can show where the mission is.
    pub notify: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Where the setup and the proofs run: a persistent working copy of the project (see `workspace`). None runs them in the
    /// mission's own lease worktree, as the canary does.
    pub stage: Option<super::workspace::Stage>,
}

/// Why the coordinator stopped before the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// Spending, quota or a person's stop: the state is saved and a Resume continues.
    Wait(String),
    /// Something only a person can settle (a pending effect, a held lease). Nothing is repeated or guessed.
    Park(String),
    /// A reply, proof or review that must not be acted on. The mission stops with the key.
    Block(String),
    /// A test hook ended the run as a power cut would: nothing more is written.
    Crash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done { head: String },
    Stopped(Stop),
}

pub fn classify(key: &str) -> Stop {
    if key.starts_with("reason.budget.") || key.starts_with("reason.quota") || key == "reason.interrupted" {
        Stop::Wait(key.into())
    } else if matches!(
        key,
        "reason.project.reconcile"
            | "reason.lease.live"
            | "reason.project.processTreeUnproven"
            | "reason.disk.needed"
            | "reason.proof.busy"
            | "reason.queue.baseRed"
    ) {
        Stop::Park(key.into())
    } else {
        Stop::Block(key.into())
    }
}
impl From<String> for Stop {
    fn from(key: String) -> Self {
        classify(&key)
    }
}
impl From<&str> for Stop {
    fn from(key: &str) -> Self {
        classify(key)
    }
}
impl From<GitError> for Stop {
    fn from(e: GitError) -> Self {
        classify(e.key())
    }
}
impl From<LeaseError> for Stop {
    fn from(e: LeaseError) -> Self {
        classify(e.key())
    }
}
impl From<JournalError> for Stop {
    fn from(e: JournalError) -> Self {
        classify(e.key())
    }
}

type Step<T> = Result<T, Stop>;

/// `coordinator.json` in the private journal folder: which steps finished, nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    schema: u32,
    done: BTreeSet<String>,
    /// The next turn number of each role. A turn that died is replaced by the next number, never reused.
    turns: BTreeMap<String, u32>,
    /// Invocations this coordinator started, so a restart can tell a live turn from one that died with the old process.
    started: BTreeSet<String>,
    /// How many of the Writer's patch operations are on disk.
    applied: usize,
    /// Numbers given to the candidate's placeholders (see `sequence`), kept so a resume gives the same ones.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    numbers: sequence::Given,
    /// Review rounds finished; each one is its own fresh reviewer session.
    #[serde(default)]
    rounds: u32,
    /// Repair turns the Writer finished.
    #[serde(default)]
    repairs: u32,
    /// Every finding the rounds returned, with what was decided, for the next round to read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    decided: Vec<Decided>,
    /// The repair the Writer owes next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repair: Option<Repair>,
    #[serde(flatten)]
    extra: Extra,
}
impl Record for Progress {
    fn schema(&self) -> u32 {
        self.schema
    }
}
impl Progress {
    fn new() -> Self {
        Self {
            schema: SCHEMA,
            done: BTreeSet::new(),
            turns: BTreeMap::new(),
            started: BTreeSet::new(),
            applied: 0,
            numbers: sequence::Given::new(),
            rounds: 0,
            repairs: 0,
            decided: vec![],
            repair: None,
            extra: Extra::new(),
        }
    }
}

/// The steps of a one-task mission, in the order the coordinator takes them.
pub const STEPS: [&str; 7] = ["plan", "arm", "red", "write", "green", "review", "land"];

/// The step a mission is on: the first one the coordinator has not marked. `None` once all are marked.
pub fn current_step(journal: &Journal) -> Option<&'static str> {
    let progress = journal.get::<Progress>(&journal.private().join("coordinator.json")).ok().flatten().unwrap_or_else(Progress::new);
    STEPS.into_iter().find(|step| !progress.done.contains(*step))
}

pub struct Coordinator<R: Roles, E: Executor> {
    roles: R,
    j: Journal,
    broker: Option<Broker<E>>,
    setup: Setup,
    rules: Rules,
    pool: Vec<ModelPin>,
    pace: Option<Pace>,
    p: Progress,
    marks: usize,
    crash_after: Option<usize>,
    crash_before: Option<usize>,
}

fn example(task: &str, base: &str, digest: &str, lease: u64, action: serde_json::Value, ping: Option<&str>) -> String {
    example_of(task, 1, base, digest, lease, action, ping)
}

fn example_of(task: &str, attempt: u32, base: &str, digest: &str, lease: u64, action: serde_json::Value, ping: Option<&str>) -> String {
    let mut envelope = serde_json::json!({"schema": 2, "task": task, "attempt": attempt, "base": base, "contract": digest, "lease": lease, "action": action});
    if let Some(ping) = ping {
        envelope["ping"] = ping.into();
    }
    envelope.to_string()
}

const ENVELOPE_RULES: &str = "Reply with exactly one JSON object and nothing else: no prose, no code fence. The only keys allowed at the top level are schema, task, attempt, base, contract, lease, ping, action and note; any other key makes the reply invalid. The value of contract is a plain string, never an object, and action is an object, never a string. Copy this shape exactly, character for character, changing only what the task asks you to change.";

fn files_text(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, text)| format!("{path} ({} bytes, sha256 {}):\n{text}", text.len(), hash(text.as_bytes())))
        .collect::<Vec<_>>()
        .join("\n---\n")
}

/// Most bytes of one quote (the brief refuses a longer one) and the most quotes a kind of text may take.
const CHUNK: usize = 7 * 1024;
const SOURCE_CHUNKS: usize = 16;
const TREE_CHUNKS: usize = 3;
const FILE_CHUNKS: usize = 24;

/// Cut a text into quotes of at most `CHUNK` bytes, named `<id>-1`, `<id>-2`... A text that needs more than `max` quotes
/// is cut short and says so when `cut` allows it; otherwise it is refused, because a role must never act on a partial file.
fn chunks(id: &str, text: &str, max: usize, cut: bool) -> Step<Vec<Verbatim>> {
    let mut out: Vec<Verbatim> = vec![];
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = rest.len().min(CHUNK);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        if out.len() == max {
            if !cut {
                return Err(Stop::Block("error.wire.tooBig".into()));
            }
            if let Some(last) = out.last_mut() {
                let mut keep = last.text.len().min(CHUNK - 64);
                while !last.text.is_char_boundary(keep) {
                    keep -= 1;
                }
                last.text.truncate(keep);
                last.text.push_str("\n[the rest is not shown]");
            }
            break;
        }
        let (head, tail) = rest.split_at(end);
        out.push(Verbatim { id: format!("{id}-{}", out.len() + 1), text: head.into() });
        rest = tail;
    }
    Ok(out)
}

/// Merge a recorded patch over a JSON example: objects key by key, arrays of one length element by element.
pub fn merge(into: &mut serde_json::Value, patch: &serde_json::Value) {
    use serde_json::Value;
    match (into, patch) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in b {
                merge(a.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (x, y) in a.iter_mut().zip(b) {
                merge(x, y);
            }
        }
        (slot, value) => *slot = value.clone(),
    }
}

/// The reply of a recorded role: the example envelope its own prompt shows it, with the recorded patch merged over it.
pub fn recorded_reply(brief: &Brief, patch: &serde_json::Value) -> Result<String, &'static str> {
    let format = brief.quotes.iter().find(|q| q.id == "format").ok_or("error.wire.stale")?;
    let example = format.text.lines().last().ok_or("error.wire.stale")?;
    let mut envelope: serde_json::Value = serde_json::from_str(example).map_err(|_| "error.wire.malformed")?;
    merge(&mut envelope, patch);
    Ok(envelope.to_string())
}

/// Hand a role's final text to its worker exactly as the end of a live turn does, and record the reply or the refusal.
pub fn deliver(journal: &Journal, task: &str, invocation: &str, text: &str) -> Result<(), String> {
    use super::worker::Reply;
    let mut runner = Runner::load(journal).map_err(str::to_string)?;
    let worker = runner.workers.get_mut(task).ok_or("error.wire.stale")?;
    let pin = worker.pin.clone();
    worker.begin_turn(&pin)?;
    match runner.receive(journal, task, text)? {
        Reply::Accepted(envelope) | Reply::Proposal(envelope) => {
            journal.record_reply(invocation, &envelope).map_err(|e| e.key().to_string())
        }
        Reply::Bounce(key) | Reply::Blocked(key) => journal.record_refusal(invocation, key).map_err(|e| e.key().to_string()),
        Reply::Continue => journal.record_refusal(invocation, "reason.stall").map_err(|e| e.key().to_string()),
    }
}

/// The Lead's reply as a plan the engine accepts: exactly one Writer task, writing only where the product allows.
pub fn accept_plan(envelope: &Envelope, writable: &[String], proofs: &[Definition]) -> Result<Task, &'static str> {
    let Action::Plan(draft) = &envelope.action else { return Err("error.wire.role") };
    let [one] = draft.tasks.as_slice() else { return Err("reason.plan.oneTask") };
    if one.kind != TaskKind::Writer || one.id == LEAD || one.id == REVIEWER || one.writes.is_empty() || one.writes.len() > 8 {
        return Err("reason.plan.oneTask");
    }
    let ids: Vec<String> = proofs.iter().map(|p| p.id.clone()).collect();
    if !one.proofs.is_empty() && one.proofs != ids {
        return Err("reason.proof.definition");
    }
    for path in &one.writes {
        let clean = normalized(path).map_err(|e| e.key())?;
        if !writable.iter().any(|w| clean == *w || clean.starts_with(&format!("{}/", w.trim_end_matches('/')))) {
            return Err("error.wire.path");
        }
    }
    let mut task = one.to_task();
    task.proofs = ids;
    task.checks = proofs.iter().flat_map(|p| p.checks.clone()).collect();
    task.paths = one.writes.clone();
    Ok(task)
}

impl<R: Roles, E: Executor> Coordinator<R, E> {
    pub fn new(roles: R, journal: Journal, broker: Broker<E>, setup: Setup) -> Result<Self, &'static str> {
        if setup.pins.reviewer.vendor == setup.pins.writer.vendor {
            return Err("reason.queue.review");
        }
        let p = journal.get::<Progress>(&journal.private().join("coordinator.json")).map_err(|e| e.key())?.unwrap_or_else(Progress::new);
        let rules = Rules::from_setup(&setup.sources, &setup.tree)?;
        Ok(Self {
            roles,
            j: journal,
            broker: Some(broker),
            setup,
            rules,
            pool: vec![],
            pace: None,
            p,
            marks: 0,
            crash_after: None,
            crash_before: None,
        })
    }

    /// More reviewer pins for the rounds after the first, and the pace that chooses among them (see `pick_reviewer`).
    pub fn reviewers(&mut self, pool: Vec<ModelPin>, pace: Option<Pace>) {
        self.pool = pool;
        self.pace = pace;
    }

    /// Test hook: end the run as a power cut would right after the mark of this many finished steps. Production never
    /// sets it.
    pub fn crash_after(&mut self, steps: usize) {
        self.crash_after = Some(steps);
    }

    /// Test hook: the power goes out when this step's work is done and its mark is not yet written.
    pub fn crash_before(&mut self, steps: usize) {
        self.crash_before = Some(steps);
    }

    pub fn journal(&self) -> &Journal {
        &self.j
    }

    pub fn roles(&self) -> &R {
        &self.roles
    }

    fn save(&self) -> Step<()> {
        self.j.put(&self.j.private().join("coordinator.json"), &self.p)?;
        Ok(())
    }
    fn mark(&mut self, step: &str) -> Step<()> {
        self.marks += 1;
        if self.crash_before == Some(self.marks) {
            return Err(Stop::Crash);
        }
        self.p.done.insert(step.into());
        self.save()?;
        self.tell();
        if self.crash_after == Some(self.marks) { Err(Stop::Crash) } else { Ok(()) }
    }
    fn tell(&self) {
        if let Some(notify) = &self.setup.notify {
            notify();
        }
    }
    /// A git effect with an intent and no receipt has an unknown outcome. Whatever step comes next, the mission parks until
    /// a person reconciles it: nothing is written, proven or landed on top of it, and nothing is repeated.
    fn effects_settled(&self) -> Step<()> {
        if self.j.pending()?.iter().any(|intent| intent.kind.effect().is_some()) {
            return Err(Stop::Park("reason.project.reconcile".into()));
        }
        Ok(())
    }
    fn done(&self, step: &str) -> bool {
        self.p.done.contains(step)
    }
    fn runner(&self) -> Step<Runner> {
        Ok(Runner::load(&self.j)?)
    }
    fn broker(&mut self) -> Step<&mut Broker<E>> {
        self.broker.as_mut().ok_or_else(|| Stop::Park("error.wire.stale".into()))
    }
    fn grant(&self) -> impl FnMut() -> Result<Option<Grant>, GitError> + use<R, E> {
        let read = self.setup.grant.clone();
        move || read()
    }

    /// Run the mission as far as it can go. The mission record says why it stopped.
    pub async fn drive(&mut self) -> Outcome {
        let result = self.run().await;
        match result {
            Ok(head) => Outcome::Done { head },
            Err(Stop::Crash) => Outcome::Stopped(Stop::Crash),
            Err(stop) => {
                self.record_stop(&stop);
                Outcome::Stopped(stop)
            }
        }
    }

    fn record_stop(&mut self, stop: &Stop) {
        let Ok(mut runner) = Runner::load(&self.j) else { return };
        if matches!(runner.mission.state, MissionState::Done) {
            return;
        }
        let (Stop::Wait(key) | Stop::Park(key) | Stop::Block(key)) = stop else { return };
        runner.mission.reason = Some(key.clone());
        if matches!(stop, Stop::Block(_)) {
            runner.mission.state = MissionState::Blocked;
            let _ = runner.save(&self.j);
        } else {
            let _ = runner.pause(&self.j);
        }
        self.roles.pause(self.j.mission_id());
        self.tell();
    }

    async fn run(&mut self) -> Step<String> {
        let mut runner = self.runner()?;
        match runner.mission.state {
            MissionState::Done => {
                // The landing finished and the power went out before the mission was closed: close it now.
                let landing = runner.landings.last().ok_or_else(|| Stop::Block("error.wire.stale".into()))?;
                let head = self
                    .j
                    .effect_receipt(&landing.commit_intent)?
                    .and_then(|r| r.evidence.get("head").cloned())
                    .ok_or_else(|| Stop::Park("reason.project.reconcile".into()))?;
                if let Some(broker) = self.broker.take() {
                    broker.finish()?;
                }
                return Ok(head);
            }
            MissionState::Blocked => {
                return Err(Stop::Block(runner.mission.reason.clone().unwrap_or_else(|| "reason.project.blocked".into())));
            }
            MissionState::Paused => return Err(Stop::Wait(runner.mission.reason.clone().unwrap_or_else(|| "reason.budget.paused".into()))),
            MissionState::Planning => {
                runner.mission.state = MissionState::Running;
                runner.save(&self.j)?;
            }
            MissionState::Running => {}
        }
        let task = self.plan().await?;
        let (lease, root) = self.arm(&task).await?;
        self.red(&task, &root).await?;
        self.write(&task, &lease, &root).await?;
        loop {
            if let Some(repair) = self.p.repair.clone() {
                self.repair(&task, &lease, &root, repair).await?;
            }
            if !self.green(&task, &lease, &root).await? {
                continue;
            }
            if self.review(&task, &root).await? {
                break;
            }
        }
        self.land(&task, &lease).await
    }

    // ----- roles -----

    fn pin_of(&self, role: Role) -> ModelPin {
        match role {
            Role::Lead => self.setup.pins.lead.clone(),
            Role::Writer => self.setup.pins.writer.clone(),
            _ => self.setup.pins.reviewer.clone(),
        }
    }

    /// Record the worker (its brief, pinned kit and model) once; a replacement turn reuses it. A brief of a later attempt
    /// (a repair) replaces the worker, and says so.
    fn wire(&mut self, pin: &ModelPin, mut brief: Brief) -> Step<bool> {
        let mut runner = self.runner()?;
        if runner.workers.get(&brief.task).is_some_and(|w| w.brief.attempt >= brief.attempt) {
            return Ok(false);
        }
        let role = brief.role;
        let pin = pin.clone();
        let labels = Labels { verb: Verb::Build, needs: vec![], risk: Risk::Low, fresh: Fresh::Stable, facets: Facets::default() };
        let kit = assemble(&Assembly {
            labels,
            fingerprint: vec![],
            inventory: Inventory::new(vec![]).map_err(|e| e.key())?,
            evidence: vec![],
            pins: vec![],
            role,
            vendor: pin.vendor,
            model: pin.model.clone(),
            effort: pin.effort.clone(),
            mandatory_proofs: vec![],
            machine: Machine { os: std::env::consts::OS.into(), apps: vec![], gpu_gb: 0 },
        })
        .map_err(|e| e.key())?;
        let receipt =
            KitReceipt::new(&runner.mission.id, &brief.task, brief.attempt, kit, runner.mission.updated_at.max(1)).map_err(|e| e.key())?;
        self.j.record_kit(&receipt)?;
        brief.kit = Some(receipt.pin());
        brief.grant_micros = Some(self.setup.micros);
        runner.workers.insert(brief.task.clone(), Worker::new(brief, pin, 3)?);
        runner.save(&self.j)?;
        Ok(true)
    }

    /// One role turn, resumable: a recorded reply is reused, a live turn is awaited, a dead one is replaced.
    async fn turn(&mut self, pin: &ModelPin, kind: Kind, brief: Brief, lease: Option<&Lease>) -> Step<(String, Envelope)> {
        let name = brief.task.clone();
        let fresh = brief.extra.get("start").is_none_or(|s| s != "resume");
        if self.wire(pin, brief)? && lease.is_some() {
            // A new Writer attempt starts its patch from the first operation.
            self.p.applied = 0;
            self.save()?;
        }
        let pin = pin.clone();
        loop {
            let n = *self.p.turns.entry(name.clone()).or_insert(1);
            let inv = format!("{name}-{n}");
            if let Some(reply) = self.j.reply(&inv)? {
                self.close(&inv, lease.is_some()).await?;
                return Ok((inv, reply));
            }
            if let Some(key) = self.j.refusal(&inv)? {
                self.close(&inv, lease.is_some()).await?;
                return Err(Stop::Block(key));
            }
            if self.p.started.contains(&inv) && !self.roles.live(&inv) {
                // The turn died with the previous process. A Writer's lease is still held by it: a person closes that.
                if lease.is_some() {
                    return Err(Stop::Park("reason.lease.live".into()));
                }
                self.roles.abandon(&self.j, &inv).await?;
                self.p.turns.insert(name.clone(), n + 1);
                self.save()?;
                if n >= 3 {
                    return Err(Stop::Block("reason.turnLimit".into()));
                }
                continue;
            }
            if !self.p.started.contains(&inv) {
                self.p.started.insert(inv.clone());
                self.save()?;
                let request = Invocation {
                    id: inv.clone(),
                    mission: self.j.mission_id().into(),
                    worker: name.clone(),
                    kind,
                    vendor: pin.vendor,
                    model: pin.model.clone(),
                    effort: pin.effort.clone(),
                    session: "unused".into(),
                    fresh,
                    exposure_micros: None,
                    extra: Extra::new(),
                };
                let writer = match lease {
                    Some(lease) => {
                        let revision = self.setup.revision;
                        let leases = self.broker.as_mut().ok_or_else(|| Stop::Park("error.wire.stale".into()))?.leases();
                        leases.prepare_launch(lease, &inv, revision, 1)?;
                        Some((leases, lease))
                    }
                    None => None,
                };
                if let Err(key) = self.roles.start(&self.j, request, writer).await {
                    let stop = classify(&key);
                    if matches!(stop, Stop::Wait(_)) {
                        // Refused before anything ran (a budget wait): nothing to settle, so the turn starts afresh on resume.
                        self.p.started.remove(&inv);
                        if let Some(lease) = lease {
                            self.broker()?.leases().abandon_launch(lease, &inv)?;
                            self.p.turns.insert(name.clone(), n + 1);
                        }
                        self.save()?;
                    }
                    return Err(stop);
                }
            }
            let waited = tokio::time::timeout(self.setup.turn_limit, self.roles.wait(&self.j, &inv)).await;
            return match waited {
                Ok(Ok(reply)) => {
                    self.close(&inv, lease.is_some()).await?;
                    Ok((inv, reply))
                }
                Ok(Err(key)) => {
                    let _ = self.roles.settle(&inv).await;
                    Err(classify(&key))
                }
                Err(_) => {
                    let _ = self.roles.settle(&inv).await;
                    Err(Stop::Block("reason.stall".into()))
                }
            };
        }
    }

    /// End a finished turn: its process tree is gone and its spending settled. A Writer's lease is released here.
    async fn close(&mut self, inv: &str, writer: bool) -> Step<()> {
        let proof = match self.roles.settle(inv).await {
            Ok(proof) => proof,
            Err(_) => {
                // No tree to close: the process died after its reply was recorded. Settle at the bound instead.
                self.roles.abandon(&self.j, inv).await?;
                None
            }
        };
        if !writer {
            return Ok(());
        }
        let task = inv.rsplit_once('-').map_or(inv, |(task, _)| task).to_string();
        let lease = self.lease_of(&task)?;
        if settled(&lease).tree_dead {
            return Ok(());
        }
        let proof = proof.ok_or_else(|| Stop::Park("reason.lease.live".into()))?;
        self.broker()?.leases().settle_process(&lease, proof)?;
        Ok(())
    }

    fn lease_of(&self, task: &str) -> Step<Lease> {
        let runtime = self.j.runtime()?;
        runtime
            .leases
            .into_iter()
            .find(|l| l.task == task && l.mission == self.j.mission_id())
            .ok_or_else(|| Stop::Block("reason.lease.none".into()))
    }

    #[allow(clippy::too_many_arguments)]
    fn brief(
        &self,
        role: Role,
        task: &str,
        attempt: u32,
        goal: &str,
        writes: &[String],
        lease: Option<u64>,
        quotes: Vec<Verbatim>,
    ) -> Step<Brief> {
        let runner = self.runner()?;
        let pin = self.contract_pin()?;
        let mut brief = Brief::new(task, attempt, role, goal, &runner.mission.base, pin);
        // The folders the plan may write under come first: a Writer's paths must sit inside the scope.
        let shown = self.setup.sources.iter().map(|(path, _)| path.clone());
        brief.scope = self.setup.writable.iter().cloned().chain(shown.clone()).take(super::wire::MAX_PATHS).collect();
        brief.sources = shown.take(super::wire::MAX_PATHS).collect();
        brief.writes = writes.to_vec();
        brief.lease = lease;
        brief.quotes = quotes;
        brief.stop = "Stop after the single envelope.".into();
        Ok(brief)
    }

    /// The quotes every role gets: the goal, the project's files and path list, what the role needs besides, and the
    /// format of its reply.
    fn quotes(&self, goal: &str, extra: Vec<Verbatim>, format: String) -> Step<Vec<Verbatim>> {
        let mut quotes = vec![Verbatim { id: "goal".into(), text: goal.into() }];
        quotes.extend(chunks("files", &files_text(&self.setup.sources), SOURCE_CHUNKS, true)?);
        quotes.extend(chunks("tree", &self.setup.tree.join("\n"), TREE_CHUNKS, true)?);
        quotes.extend(extra);
        quotes.push(Verbatim { id: "format".into(), text: format });
        Ok(quotes)
    }

    fn contract(&self) -> Step<Contract> {
        self.j.contracts()?.into_iter().find(|c| c.id == CONTRACT).ok_or_else(|| Stop::Block("reason.contract.revised".into()))
    }

    fn contract_pin(&self) -> Step<ContractPin> {
        let contract = self.contract()?;
        Ok(ContractPin { id: contract.id, digest: contract.digest })
    }

    /// The contract's coherence fields, checked against what `paths` touch. Without them, or when they leave out an area
    /// the paths are in, nothing is written: the mission stops with `reason.contract.coherence`.
    fn coherent(&self, paths: &[String]) -> Step<(Coherence, Sizing)> {
        let refused = || Stop::Block("reason.contract.coherence".into());
        let contract = self.contract()?;
        let verified = contract.verify();
        let coherence = contract.coherence.filter(|_| verified).ok_or_else(refused)?;
        let mut sizing = self.rules.size(paths);
        coherence.check(&self.rules, &sizing).map_err(|_| refused())?;
        sizing.widen(&coherence);
        Ok((coherence, sizing))
    }

    /// Put the plan's coherence fields into the contract, once, as its next version.
    fn sign(&self, task: &Task) -> Step<()> {
        let mut contract = self.contract()?;
        if contract.coherence.is_some() || task.coherence.is_none() {
            return Ok(());
        }
        contract.coherence = task.coherence.clone();
        contract.version += 1;
        self.j.save_contract(&contract.seal())?;
        Ok(())
    }

    /// What every Writer and reviewer reads about the contract and the numbered entries.
    fn contract_quotes(&self, coherence: &Coherence) -> Step<Vec<Verbatim>> {
        let mut quotes = chunks("contract", &coherence.text(), 4, true)?;
        if let Some(text) = sequence::brief(&self.rules.sequences) {
            quotes.extend(chunks("numbers", &text, 1, true)?);
        }
        Ok(quotes)
    }

    // ----- steps -----

    async fn plan(&mut self) -> Step<Task> {
        if self.done("plan") {
            let plan = self.j.plan()?.ok_or_else(|| Stop::Block("reason.project.compile".into()))?;
            return plan.tasks.into_iter().next().ok_or_else(|| Stop::Block("reason.project.compile".into()));
        }
        let runner = self.runner()?;
        if self.j.contracts()?.iter().all(|c| c.id != CONTRACT) {
            self.j.save_contract(&Contract::new(CONTRACT, LEAD, &runner.mission.base).seal())?;
        }
        if let Some(mut task) = self.setup.plan.clone() {
            // The product wrote this plan, so the product names what it touches: the areas its writes fall in.
            if task.coherence.is_none() {
                task.coherence = Some(Coherence {
                    areas: self.rules.size(&task.writes).areas.into_iter().collect(),
                    closest: task.writes.join(", "),
                    extends: Some(task.title.clone()),
                    new: None,
                });
            }
            self.sign(&task)?;
            self.coherent(&task.writes)?;
            self.j.save_plan(&Plan::new(&runner.mission.id, 1, vec![task.clone()]))?;
            self.mark("plan")?;
            return Ok(task);
        }
        let pin = self.contract_pin()?;
        let plan_shape = serde_json::json!({"plan": {
            "summary": "One sentence on what changes and why.",
            "acceptance": "What the proof checks.",
            "tasks": [{"id": "writer", "title": "A short title", "kind": "writer", "writes": ["path/under/an/allowed/folder"],
                "coherence": {"areas": ["an area the writes fall in"], "closest": "The closest thing that already exists.",
                    "extends": "The existing thing this change extends."}}]
        }});
        let allowed = self.setup.writable.join(", ");
        let areas = if self.rules.areas.is_empty() {
            "An area is the top folder of a path (\".\" for a file at the root).".to_string()
        } else {
            format!("The areas are: {}.", self.rules.areas.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "))
        };
        let format = format!(
            "{ENVELOPE_RULES} Plan exactly one writer task. Its writes list only paths under: {allowed}. Do not name a proof: the engine runs its own. The path list names every tracked file, so a path you name may be one whose text is not shown. Fill coherence before anything is written: areas names every area the writes fall in ({areas}), closest names the closest thing that already exists, and either extends says what this change extends or, when nothing fits, replace extends with new saying why the closest thing does not fit.\n{}",
            example(LEAD, &runner.mission.base, &pin.digest, 0, plan_shape, None)
        );
        let quotes = self.quotes(&runner.mission.goal, vec![], format)?;
        let brief = self.brief(Role::Lead, LEAD, 1, "Plan the mission as one writer task", &[], None, quotes)?;
        let lead = self.pin_of(Role::Lead);
        let (_, reply) = self.turn(&lead, Kind::Lead, brief, None).await?;
        let task = accept_plan(&reply, &self.setup.writable, &self.setup.proofs)?;
        self.sign(&task)?;
        self.coherent(&task.writes)?;
        self.j.save_plan(&Plan::new(&runner.mission.id, 1, vec![task.clone()]))?;
        self.mark("plan")?;
        Ok(task)
    }

    /// The mission branch, the Writer's lease and its worktree. Each is an effect with its own receipt.
    async fn arm(&mut self, task: &Task) -> Step<(Lease, std::path::PathBuf)> {
        if !self.done("arm") {
            let runner = self.runner()?;
            let grant = self.grant();
            let pending = self.j.pending()?;
            if !pending.is_empty() {
                return Err(Stop::Park("reason.project.reconcile".into()));
            }
            let pin = self.contract_pin()?;
            let (mission, base, repo) = (runner.mission.id.clone(), runner.mission.base.clone(), runner.mission.repo.clone());
            let have_branch = self.j.effect_receipt(BRANCH_INTENT)?.is_some();
            let broker = self.broker()?;
            if !have_branch {
                broker.create_branch(grant, BRANCH_INTENT)?;
            }
            let existing = broker.journal().runtime()?.leases.into_iter().find(|l| l.task == task.id && l.mission == mission);
            let lease = match existing {
                Some(lease) => lease,
                None => broker.leases().acquire(Lease::new(&repo, &mission, &task.id, 1, task.writes.clone(), &base, &pin.digest, 0))?,
            };
            if self.j.effect_receipt(WORKTREE_INTENT)?.is_none() {
                let grant = self.grant();
                self.broker()?.worktree(grant, &lease, WORKTREE_INTENT)?;
            }
            self.mark("arm")?;
        }
        let lease = self.lease_of(&task.id)?;
        let root = self.broker()?.leases().worktree(&lease)?;
        Ok((lease, root))
    }

    fn proof_set(&self, tree: &str, input: &str, paths: Vec<String>) -> Step<ProofSet> {
        Ok(ProofSet::new(tree.into(), input.into(), self.setup.proofs.clone(), paths)?)
    }

    /// The engine proves the base red before any patch: a proof that already passes proves nothing.
    async fn red(&mut self, task: &Task, root: &std::path::Path) -> Step<()> {
        if self.done("red") {
            return Ok(());
        }
        if !self.setup.expect_red {
            return self.mark("red");
        }
        let mut runner = self.runner()?;
        let base = runner.mission.base.clone();
        let mut red = self.proof_set(&base, "red", task.writes.clone())?;
        let ground = self.ground(root, false).await?;
        self.prove(&mut runner, &task.id, &mut red, &ground).await?;
        if red.verified() {
            return Err(Stop::Block("reason.proof.notRed".into()));
        }
        self.mark("red")
    }

    /// The folder the setup and the proofs run in: the mission's lease worktree, or the project's persistent working copy
    /// brought to the same tree (its installed packages and build output are kept), with the setup run there first.
    async fn ground(&self, root: &std::path::Path, candidate: bool) -> Step<std::path::PathBuf> {
        let Some(stage) = self.setup.stage.clone() else { return Ok(root.to_path_buf()) };
        let base = self.runner()?.mission.base.clone();
        let from = root.to_path_buf();
        let space = stage.workspace.clone();
        let repo = stage.repo_root.clone();
        let synced = tokio::task::spawn_blocking(move || {
            let mut git = super::git::Process::find().map_err(|_| "reason.workspace.git")?;
            space.sync(&mut git, &repo, &base, &from)
        })
        .await
        .map_err(|_| Stop::Block("reason.workspace.git".into()))?;
        synced.map_err(|key| Stop::Block(key.into()))?;
        if candidate {
            // From here the candidate's setup and proofs leave state in the copy; it is trusted again only if the candidate lands.
            stage.workspace.taint().map_err(|key| Stop::Block(key.into()))?;
        }
        let dir = stage.workspace.dir().to_path_buf();
        for definition in &stage.setup {
            if stage.workspace.setup_current(definition) {
                continue;
            }
            let seen = super::proof::execute(definition, &dir, &self.setup.proof_spawn, "setup".into()).await;
            if seen.exit != Some(0) {
                stage.workspace.setup_forget(definition);
                return Err(Stop::Block("reason.setup.failed".into()));
            }
            stage.workspace.setup_done(definition);
        }
        Ok(dir)
    }

    /// Run the proofs in order on the tree in `root`, ending at the first one that does not pass: a candidate is green only
    /// when every proof passed, and a base is red as soon as one fails.
    async fn prove(&self, runner: &mut Runner, task: &str, set: &mut ProofSet, root: &std::path::Path) -> Step<()> {
        for definition in self.setup.proofs.clone() {
            let receipt = runner.run_proof(&self.j, task, &definition.id, set, root, &self.setup.proof_spawn).await?;
            if receipt.checks.iter().any(|c| c.result != super::wire::CheckResult::Pass) {
                break;
            }
        }
        Ok(())
    }

    fn applied(root: &std::path::Path, op: &FileOp) -> bool {
        let read = |path: &str| std::fs::read(root.join(path)).ok();
        match op {
            FileOp::Write(w) => read(&w.path).is_some_and(|bytes| bytes == w.content.as_bytes()),
            FileOp::Delete(d) => !root.join(&d.path).exists(),
            FileOp::Rename(r) => !root.join(&r.from).exists() && root.join(&r.to).exists(),
        }
    }

    /// The Writer's format: replace whole files, each with the hash of its current text, at this attempt and lease.
    fn patch_format(&self, task: &Task, attempt: u32, lease: &Lease, root: &std::path::Path) -> Step<String> {
        let base = self.runner()?.mission.base;
        let pin = self.contract_pin()?;
        let existing: Vec<String> = task.writes.iter().filter(|p| root.join(p).is_file()).cloned().collect();
        let shape = serde_json::json!({"patch": {"ops": [{"write": {
            "path": task.writes[0], "expect": existing.first().map(|p| hash(std::fs::read(root.join(p)).unwrap_or_default().as_slice())),
            "content": "the complete new text of the file"}}]}});
        Ok(format!(
            "{ENVELOPE_RULES} Write only these paths: {}. A file that exists is replaced whole and its expect is the sha256 of its current text shown above; a new file has expect null.\n{}",
            task.writes.join(", "),
            example_of(&task.id, attempt, &base, &pin.digest, lease.generation, shape, None)
        ))
    }

    fn targets(task: &Task, root: &std::path::Path) -> Step<Vec<Verbatim>> {
        let current: Vec<(String, String)> =
            task.writes.iter().map(|p| (p.clone(), std::fs::read_to_string(root.join(p)).unwrap_or_default())).collect();
        chunks("targets", &files_text(&current), FILE_CHUNKS, false)
    }

    async fn write(&mut self, task: &Task, lease: &Lease, root: &std::path::Path) -> Step<()> {
        if self.done("write") {
            return Ok(());
        }
        self.effects_settled()?;
        // The contract's coherence fields come before any patch: no Writer starts without them.
        let (coherence, _) = self.coherent(&task.writes)?;
        let runner = self.runner()?;
        let mut extra = Self::targets(task, root)?;
        extra.extend(self.contract_quotes(&coherence)?);
        let format = self.patch_format(task, 1, lease, root)?;
        let quotes = self.quotes(&runner.mission.goal, extra, format)?;
        let brief = self.brief(Role::Writer, &task.id, 1, &task.title, &task.writes, Some(lease.generation), quotes)?;
        let writer = self.pin_of(Role::Writer);
        let (_, reply) = self.turn(&writer, Kind::Worker, brief, Some(lease)).await?;
        self.apply(lease, root, &reply).await?;
        self.mark("write")
    }

    /// Put the Writer's patch on disk through the lease, its placeholders numbered first (see `sequence`).
    async fn apply(&mut self, lease: &Lease, root: &std::path::Path, reply: &Envelope) -> Step<()> {
        let Action::Patch(patch) = &reply.action else { return Err(Stop::Block("error.wire.role".into())) };
        let ops = self.numbered(root, &patch.ops)?;
        let grant = (self.setup.grant)()?.ok_or_else(|| Stop::Block("reason.grant.none".into()))?;
        let revision = self.setup.revision;
        for (index, op) in ops.iter().enumerate() {
            if index < self.p.applied || Self::applied(root, op) {
                self.p.applied = self.p.applied.max(index + 1);
                continue;
            }
            self.broker()?.leases().patch(lease, Some(&grant), revision, op)?;
            self.p.applied = index + 1;
            self.save()?;
        }
        Ok(())
    }

    /// The patch with the next free numbers of the head in place of the candidate's placeholders. The numbers given are
    /// saved before any operation lands, so a resume or a repair gives the same ones.
    fn numbered(&mut self, root: &std::path::Path, ops: &[FileOp]) -> Step<Vec<FileOp>> {
        if self.rules.sequences.is_empty() {
            return Ok(ops.to_vec());
        }
        let read = |path: &str| std::fs::read_to_string(root.join(path)).ok();
        let head: Vec<(String, String)> = self
            .setup
            .tree
            .iter()
            .filter(|p| self.rules.sequences.iter().any(|s| sequence::holds(s, p)))
            .filter_map(|p| read(p).map(|text| (p.clone(), text)))
            .collect();
        let befores: Vec<Option<String>> = ops.iter().map(|op| if let FileOp::Write(w) = op { read(&w.path) } else { None }).collect();
        let writes: Vec<sequence::Written> = ops
            .iter()
            .zip(&befores)
            .filter_map(|(op, before)| match op {
                FileOp::Write(w) => Some(sequence::Written { before: before.as_deref(), after: &w.content }),
                _ => None,
            })
            .collect();
        let mut given = self.p.numbers.clone();
        let mut texts = sequence::assign(&self.rules.sequences, &head, &writes, &mut given).into_iter();
        let out = ops
            .iter()
            .map(|op| match op {
                FileOp::Write(w) => {
                    let mut w = w.clone();
                    w.content = texts.next().unwrap_or(w.content);
                    FileOp::Write(w)
                }
                other => other.clone(),
            })
            .collect();
        if given != self.p.numbers {
            self.p.numbers = given;
            self.save()?;
        }
        Ok(out)
    }

    /// Compose the tree, prove it and queue the candidate with its size. `false` means a repair is owed first.
    async fn green(&mut self, task: &Task, lease: &Lease, root: &std::path::Path) -> Step<bool> {
        if self.done("green") {
            return Ok(true);
        }
        self.effects_settled()?;
        let mut runner = self.runner()?;
        let paths = self.broker()?.leases().audit(lease)?;
        // The engine sizes what the change really touched; the contract must name every area of it.
        let (_, sizing) = self.coherent(&paths)?;
        let grant = self.grant();
        let tree = self.broker()?.compose_tree(grant, lease, &paths)?;
        let input = runner.workers.get(&task.id).ok_or_else(|| Stop::Block("error.wire.stale".into()))?.digest();
        let proven =
            runner.queue.get(&task.id).is_some_and(|c| c.tree == tree && c.input == input && c.proof().is_some_and(ProofSet::verified));
        if !proven {
            match runner.queue.get(&task.id) {
                None => {
                    let candidate = Candidate::new(
                        &task.id,
                        runner.mission.base.clone(),
                        tree.clone(),
                        runner.mission.grant_revision,
                        self.setup.pins.writer.vendor,
                        input.clone(),
                        paths.clone(),
                        self.setup.proofs.clone(),
                    )?;
                    runner.queue.enqueue(candidate)?;
                }
                Some(c) if c.tree != tree || c.input != input => {
                    runner.queue.repaired(&task.id, tree.clone(), input.clone(), paths.clone())?
                }
                Some(_) => {}
            }
            runner.save(&self.j)?;
            let mut proof = self.proof_set(&tree, &input, paths.clone())?;
            let ground = self.ground(root, true).await?;
            self.prove(&mut runner, &task.id, &mut proof, &ground).await?;
            if !proof.verified() {
                return self.red_candidate(task, &mut runner, &proof).await;
            }
            runner.queue.prove(&task.id, proof)?;
        }
        let tier = sizing.tier();
        runner.queue.require_reviews(&task.id, tier.reviews())?;
        // The size is the engine's record, kept beside the candidate for the report.
        let size = serde_json::json!({"tier": tier, "areas": sizing.areas, "security": sizing.security, "unknown": sizing.unknown});
        runner.queue.note(&task.id, "size", size)?;
        runner.save(&self.j)?;
        self.mark("green")?;
        Ok(true)
    }

    /// The candidate is red. A red base under it stops the line and makes this Writer the owner of the base's repair;
    /// otherwise the Writer repairs its own change. Past the one repair, the mission stops.
    async fn red_candidate(&mut self, task: &Task, runner: &mut Runner, proof: &ProofSet) -> Step<bool> {
        if self.p.repairs >= REPAIRS {
            return Err(Stop::Block("reason.proof.failed".into()));
        }
        let mut receipts: Vec<String> = proof.receipts().map(|r| r.id.clone()).collect();
        let base = runner.mission.base.clone();
        let why = match self.head_red(runner, task).await? {
            Some(receipt) => {
                runner.queue.stop_line(&base, &receipt)?;
                runner.queue.own_repair(&task.id)?;
                runner.save(&self.j)?;
                receipts.push(receipt);
                "base"
            }
            None => "proof",
        };
        self.p.repair = Some(Repair { why: why.into(), findings: vec![], receipts });
        self.save()?;
        Ok(false)
    }

    /// Prove the integration head itself, in the project's working copy brought back to it. `Some(receipt)` is a red
    /// head. Without a working copy the head cannot be proven apart from the candidate, so it counts as green.
    async fn head_red(&self, runner: &mut Runner, task: &Task) -> Step<Option<String>> {
        if self.setup.expect_red {
            return Ok(None);
        }
        let Some(stage) = self.setup.stage.clone() else { return Ok(None) };
        let base = runner.mission.base.clone();
        let tree = base.clone();
        let space = stage.workspace.clone();
        let repo = stage.repo_root.clone();
        let synced = tokio::task::spawn_blocking(move || {
            let mut git = super::git::Process::find().map_err(|_| "reason.workspace.git")?;
            let own = space.dir().to_path_buf();
            space.sync(&mut git, &repo, &base, &own)
        })
        .await
        .map_err(|_| Stop::Block("reason.workspace.git".into()))?;
        synced.map_err(|key| Stop::Block(key.into()))?;
        let dir = stage.workspace.dir().to_path_buf();
        let mut head = self.proof_set(&tree, "head", vec![])?;
        self.prove(runner, &task.id, &mut head, &dir).await?;
        Ok((!head.verified()).then(|| head.receipts().last().map(|r| r.id.clone())).flatten())
    }

    /// The Writer's repair turn, from a brief that holds everything it needs: what is wrong (findings with their place,
    /// or the engine's receipts), the contract, and the current text of its files. It starts fresh unless its cache is warm
    /// and its context small.
    async fn repair(&mut self, task: &Task, lease: &Lease, root: &std::path::Path, repair: Repair) -> Step<()> {
        self.effects_settled()?;
        let attempt = 2 + self.p.repairs;
        // The last Writer turn is over and proven gone: the same lease may launch the repair.
        let held = self.lease_of(&task.id)?;
        if held.process.is_some() && settled(&held).tree_dead {
            self.broker()?.leases().reopen(&held)?;
        }
        let (coherence, _) = self.coherent(&task.writes)?;
        let runner = self.runner()?;
        let what = match repair.why.as_str() {
            "base" => {
                "The engine's proof is red on the composed tree and also on the base under it: the base itself is broken. Nothing else lands until it is green again, and you own its repair. Make the proof pass."
            }
            "proof" => {
                "The engine's proof is red on the composed tree, and the base under it is green: the change broke it. Make the proof pass."
            }
            _ => {
                "The review found what must change before the candidate may land. Fix each finding and nothing else; where a test can show the fault, add one that fails without the fix."
            }
        };
        let mut lines = vec![what.to_string()];
        lines.extend(repair.findings.iter().map(|f| format!("- {}", f.line())));
        if !repair.receipts.is_empty() {
            lines.push(format!("Engine receipts: {}.", repair.receipts.join(", ")));
        }
        let mut extra = chunks("repair", &lines.join("\n"), 2, true)?;
        extra.extend(Self::targets(task, root)?);
        extra.extend(self.contract_quotes(&coherence)?);
        let format = self.patch_format(task, attempt, lease, root)?;
        let quotes = self.quotes(&runner.mission.goal, extra, format)?;
        let mut brief = self.brief(Role::Writer, &task.id, attempt, &task.title, &task.writes, Some(lease.generation), quotes)?;
        // No session today reports its context size, so a repair always starts fresh.
        let start = if repair_fresh(false, None) { "fresh" } else { "resume" };
        brief.extra.insert("start".into(), start.into());
        let writer = self.pin_of(Role::Writer);
        let (_, reply) = self.turn(&writer, Kind::Worker, brief, Some(lease)).await?;
        self.apply(lease, root, &reply).await?;
        self.p.repairs += 1;
        self.p.repair = None;
        self.p.done.remove("green");
        self.p.done.remove("review");
        self.save()?;
        self.tell();
        Ok(())
    }

    /// Fresh reviewer rounds on the exact tree until the candidate has the reviews its size asks for. `false` means the
    /// Writer owes a repair first.
    async fn review(&mut self, task: &Task, root: &std::path::Path) -> Step<bool> {
        loop {
            if self.done("review") {
                return Ok(true);
            }
            let runner = self.runner()?;
            let candidate = runner.queue.get(&task.id).ok_or_else(|| Stop::Block("reason.queue.serial".into()))?.clone();
            if candidate.accepted_reviews().len() >= usize::from(candidate.reviews_needed())
                && candidate.review().is_some_and(|r| r.accepted && r.findings.is_empty())
            {
                self.mark("review")?;
                return Ok(true);
            }
            if self.p.rounds >= ROUNDS {
                return Err(Stop::Block("reason.queue.review".into()));
            }
            let round = self.p.rounds + 1;
            if !self.round(task, root, &candidate, round).await? {
                return Ok(false);
            }
        }
    }

    /// One review round. `false` means the round blocked and the Writer owes a repair.
    async fn round(&mut self, task: &Task, root: &std::path::Path, candidate: &Candidate, round: u32) -> Step<bool> {
        let runner = self.runner()?;
        let proof = candidate.proof().ok_or_else(|| Stop::Block("reason.queue.proof".into()))?;
        let receipt = proof.receipts().next().ok_or_else(|| Stop::Block("reason.queue.proof".into()))?.id.clone();
        let pin = self.contract_pin()?;
        let (coherence, _) = self.coherent(&candidate.paths)?;
        let changed: Vec<(String, String)> =
            candidate.paths.iter().map(|p| (p.clone(), std::fs::read_to_string(root.join(p)).unwrap_or_default())).collect();
        let verdict = serde_json::json!({"complete": {
            "summary": "One sentence on whether the candidate meets the goal.", "verdict": "pass", "next": "land",
            "receipt": receipt, "findings": []}});
        let proved = self.setup.proofs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>().join(", ");
        let evidence = format!(
            "{}\nThe engine ran the proof {proved} on this exact tree and every one exited 0. Engine receipt id: {receipt}.",
            files_text(&changed)
        );
        let name = if round == 1 { REVIEWER.to_string() } else { format!("{REVIEWER}{round}") };
        let format = format!(
            "{ENVELOPE_RULES} Keep lease 0, keep ping \"DoneReady\", keep next and keep the receipt id. Change only the summary and, if you find something, findings: one object each, {{\"class\": \"person\" | \"data\" | \"security\" | \"rule\" | \"note\", \"at\": \"path:line\", \"text\": \"one short line\"}}. Only a break for a person, for data, for security or for a written rule blocks the candidate; anything else is a note. Set verdict to \"block\" only with a finding that blocks. Do not raise again what the decided quote already settled.\n{}",
            example(&name, &runner.mission.base, &pin.digest, 0, verdict, Some("DoneReady"))
        );
        let mut extra = chunks("candidate", &evidence, FILE_CHUNKS, false)?;
        extra.extend(self.contract_quotes(&coherence)?);
        if !self.p.decided.is_empty() {
            let lines: Vec<String> = self
                .p
                .decided
                .iter()
                .map(|d| {
                    let outcome = if d.blocks { "blocked, and the Writer repaired it" } else { "kept as a note" };
                    format!("- round {}, {}: {outcome}", d.round, d.finding.line())
                })
                .collect();
            extra.extend(chunks("decided", &format!("Findings earlier rounds already decided:\n{}", lines.join("\n")), 2, true)?);
        }
        let quotes = self.quotes(&runner.mission.goal, extra, format)?;
        let brief = self.brief(Role::Reviewer, &name, 1, "Review the candidate against the goal", &[], None, quotes)?;
        let reviewer = pick_reviewer(&self.setup.pins.reviewer, &self.pool, self.setup.pins.writer.vendor, round, self.pace.as_ref());
        let (inv, reply) = self.turn(&reviewer, Kind::Review, brief, None).await?;
        let Action::Complete(done) = &reply.action else { return Err(Stop::Block("error.wire.role".into())) };
        let mut blocking: Vec<Finding> = done.findings.iter().filter(|f| f.blocks()).cloned().collect();
        if done.verdict == Some(Verdict::Block) && done.findings.is_empty() {
            blocking.push(Finding::from("The reviewer blocked without a finding."));
        }
        let accepted = blocking.is_empty();
        let mut runner = self.runner()?;
        // A crash after the review was saved resumes here: the same session is already on the candidate.
        let recorded = runner.queue.get(&task.id).and_then(Candidate::review).is_some_and(|r| r.session == inv);
        if !recorded {
            let review = Review {
                vendor: reviewer.vendor,
                tree: candidate.tree.clone(),
                proof_digest: proof.digest(),
                session: inv.clone(),
                fresh: true,
                accepted,
                findings: blocking.iter().map(Finding::line).collect(),
            };
            let outcome = runner.queue.review(&task.id, review);
            runner.save(&self.j)?;
            if accepted && outcome.is_err() {
                return Err(Stop::Block("reason.queue.review".into()));
            }
        }
        if self.p.rounds < round {
            self.p.rounds = round;
            self.p.decided.extend(done.findings.iter().map(|f| Decided { round, finding: f.clone(), blocks: f.blocks() }));
            if done.verdict == Some(Verdict::Block) && done.findings.is_empty() {
                self.p.decided.push(Decided { round, finding: blocking[0].clone(), blocks: true });
            }
            self.save()?;
        }
        if accepted {
            return Ok(true);
        }
        // A finding nobody classified, or one past the one repair, is for a person to settle.
        if blocking.iter().any(|f| f.class.is_none()) || self.p.repairs >= REPAIRS {
            return Err(Stop::Block("reason.queue.review".into()));
        }
        let receipts = vec![receipt];
        self.p.repair = Some(Repair { why: "review".into(), findings: blocking, receipts });
        self.save()?;
        Ok(false)
    }

    async fn land(&mut self, task: &Task, lease: &Lease) -> Step<String> {
        self.effects_settled()?;
        let mut runner = self.runner()?;
        let lease = self.lease_of(&lease.task)?;
        let termination = settled(&lease);
        let grant = self.grant();
        let broker = self.broker()?;
        let head = runner.land(broker, &task.id, &lease, grant, &task.title, termination, |_| Ok(()))?;
        self.mark("land")?;
        if let Some(stage) = &self.setup.stage {
            stage.workspace.release(&head);
        }
        if let Some(broker) = self.broker.take() {
            broker.finish()?;
        }
        Ok(head)
    }
}

/// What the lease itself records about its closed tree: `settle_process` saved it, and a lease that was released
/// already proved it. Anything else is not a proof, so the landing refuses it.
fn settled(lease: &Lease) -> Termination {
    let known = retirement_known(lease)
        || lease
            .extra
            .get("termination")
            .is_some_and(|t| t["process"] == serde_json::json!(lease.process) && t["treeDead"] == true && t["costSettled"] == true);
    Termination { process: lease.process, tree_dead: known, cost_settled: known }
}

// ----- the engine as the roles -----

impl Roles for crate::Engine {
    async fn start(&self, journal: &Journal, request: Invocation, writer: Option<(&mut Leases, &Lease)>) -> Result<(), String> {
        match writer {
            Some((leases, lease)) => self.dispatch_mission_writer(journal.clone(), request, leases, lease).await,
            None => self.dispatch_mission_worker(journal.clone(), request).await,
        }
    }

    async fn wait(&self, journal: &Journal, invocation: &str) -> Result<Envelope, String> {
        let wake = self.mission_wake();
        loop {
            if let Some(reply) = journal.reply(invocation).map_err(|e| e.key().to_string())? {
                return Ok(reply);
            }
            if let Some(key) = journal.refusal(invocation).map_err(|e| e.key().to_string())? {
                return Err(key);
            }
            if Runner::load(journal)?.mission.state == MissionState::Paused {
                return Err("reason.interrupted".into());
            }
            // A turn's end stores a permit, so an event between the checks above and this await is never lost.
            wake.notified().await;
        }
    }

    fn live(&self, invocation: &str) -> bool {
        self.role_live(invocation)
    }

    async fn settle(&self, invocation: &str) -> Result<Option<Termination>, String> {
        match self.worker_termination(invocation).await {
            Ok(proof) => Ok(Some(proof)),
            Err(e) if self.mission_reservation(invocation).is_ok_and(|r| r.settled) => {
                let _ = e;
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    async fn abandon(&self, journal: &Journal, invocation: &str) -> Result<(), String> {
        self.abandon_role(journal, invocation).await
    }

    fn pause(&self, mission: &str) {
        let _ = self.pause_mission_budget(mission);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::wire::Expect;

    fn proof() -> Definition {
        Definition {
            id: "node-check".into(),
            version: 1,
            program: std::path::PathBuf::from("node"),
            argv: vec!["check.js".into()],
            checks: vec!["C1".into()],
            toolchain: "node".into(),
            dependencies: "none".into(),
            environment: BTreeMap::new(),
            heavy: false,
        }
    }
    const BASE: &str = "1111111111111111111111111111111111111111";
    const DIGEST: &str = "07509c71c43834df7adc4dd63b848a6e050c4b04ffddb484597f341bedf6dbe0";

    fn expect(task: &str, lease: u64) -> Expect {
        Expect { task: task.into(), attempt: 1, base: BASE.into(), contract: DIGEST.into(), lease }
    }

    #[test]
    fn the_examples_shown_to_the_roles_are_envelopes_the_engine_accepts() {
        let plan = example(
            LEAD,
            BASE,
            DIGEST,
            0,
            serde_json::json!({"plan": {"summary": "s", "acceptance": "a",
                "tasks": [{"id": "writer", "title": "t", "kind": "writer", "writes": ["src/add.js"]}]}}),
            None,
        );
        let envelope = Envelope::parse(&plan, &expect(LEAD, 0), Role::Lead).unwrap();
        let task = accept_plan(&envelope, &["src".into()], &[proof()]).unwrap();
        assert_eq!(
            (task.id.as_str(), task.proofs.clone(), task.checks.clone()),
            ("writer", vec!["node-check".to_string()], vec!["C1".to_string()])
        );
        let patch = example(
            "writer",
            BASE,
            DIGEST,
            2,
            serde_json::json!({"patch": {"ops": [{"write": {"path": "src/add.js", "expect": null, "content": "x"}}]}}),
            None,
        );
        Envelope::parse(&patch, &expect("writer", 2), Role::Writer).unwrap();
        let verdict = example(
            REVIEWER,
            BASE,
            DIGEST,
            0,
            serde_json::json!({"complete": {"summary": "s", "verdict": "pass", "next": "land", "receipt": "r1", "findings": []}}),
            Some("DoneReady"),
        );
        Envelope::parse(&verdict, &expect(REVIEWER, 0), Role::Reviewer).unwrap();
    }

    #[test]
    fn long_text_becomes_quotes_the_brief_accepts_and_a_file_to_change_is_never_cut() {
        let text = "é".repeat(10_000);
        let quotes = chunks("files", &text, 16, true).unwrap();
        assert!(quotes.len() > 1 && quotes.iter().all(|q| q.text.len() <= CHUNK));
        assert_eq!(quotes.iter().map(|q| q.text.as_str()).collect::<String>(), text, "nothing lost, nothing split inside a letter");
        assert_eq!(quotes[1].id, "files-2");
        let long = "x".repeat(CHUNK * 3 + 1);
        let cut = chunks("tree", &long, 3, true).unwrap();
        assert_eq!(cut.len(), 3);
        assert!(cut[2].text.ends_with("[the rest is not shown]") && cut[2].text.len() <= CHUNK);
        assert_eq!(chunks("targets", &long, 3, false), Err(Stop::Block("error.wire.tooBig".into())));
        assert!(chunks("none", "", 3, false).unwrap().is_empty());
    }

    fn plan_with(tasks: serde_json::Value) -> Envelope {
        let text = example(LEAD, BASE, DIGEST, 0, serde_json::json!({"plan": {"summary": "s", "acceptance": "a", "tasks": tasks}}), None);
        Envelope::parse(&text, &expect(LEAD, 0), Role::Lead).unwrap()
    }

    #[test]
    fn a_plan_is_one_writer_task_inside_the_allowed_folders_naming_only_the_catalog_proof() {
        let writable = ["src".to_string()];
        let one = |extra: serde_json::Value| {
            let mut task = serde_json::json!({"id": "writer", "title": "t", "kind": "writer", "writes": ["src/a.js"]});
            task.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            plan_with(serde_json::json!([task]))
        };
        assert!(accept_plan(&one(serde_json::json!({})), &writable, &[proof()]).is_ok());
        assert!(accept_plan(&one(serde_json::json!({"proofs": ["node-check"]})), &writable, &[proof()]).is_ok());
        assert_eq!(accept_plan(&one(serde_json::json!({"proofs": ["rm-rf"]})), &writable, &[proof()]), Err("reason.proof.definition"));
        assert_eq!(accept_plan(&one(serde_json::json!({"writes": ["other/a.js"]})), &writable, &[proof()]), Err("error.wire.path"));
        assert_eq!(accept_plan(&one(serde_json::json!({"id": "reviewer"})), &writable, &[proof()]), Err("reason.plan.oneTask"));
        assert_eq!(
            accept_plan(&one(serde_json::json!({"kind": "research", "writes": []})), &writable, &[proof()]),
            Err("reason.plan.oneTask")
        );
        let two = plan_with(serde_json::json!([
            {"id": "writer", "title": "t", "kind": "writer", "writes": ["src/a.js"]},
            {"id": "second", "title": "t", "kind": "writer", "writes": ["src/b.js"]}
        ]));
        assert_eq!(accept_plan(&two, &writable, &[proof()]), Err("reason.plan.oneTask"));
        let patch = example(
            "writer",
            BASE,
            DIGEST,
            2,
            serde_json::json!({"patch": {"ops": [{"write": {"path": "src/a.js", "expect": null, "content": "x"}}]}}),
            None,
        );
        let patch = Envelope::parse(&patch, &expect("writer", 2), Role::Writer).unwrap();
        assert_eq!(accept_plan(&patch, &writable, &[proof()]), Err("error.wire.role"));
    }

    #[test]
    fn what_stops_a_mission_is_sorted_into_wait_park_or_block() {
        for key in ["reason.budget.cap", "reason.budget.daily", "reason.quota.weekly", "reason.interrupted"] {
            assert!(matches!(classify(key), Stop::Wait(_)), "{key}");
        }
        for key in ["reason.project.reconcile", "reason.lease.live", "reason.project.processTreeUnproven", "reason.disk.needed"] {
            assert!(matches!(classify(key), Stop::Park(_)), "{key}");
        }
        for key in ["error.wire.malformed", "reason.proof.failed", "reason.queue.review", "error.wire.stale"] {
            assert!(matches!(classify(key), Stop::Block(_)), "{key}");
        }
    }
}
