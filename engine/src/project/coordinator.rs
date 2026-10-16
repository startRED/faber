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
//! Limits of this first version, kept on purpose: one task per plan, one Writer attempt, no repair loop, and the
//! landing gate is the exact-tree queue (proof plus other-vendor review), not the full deliverable bar.
use super::{
    Brief, Contract, ContractPin, Extra, Grant, Lease, MissionState, Plan, Record, Role, SCHEMA, Task, TaskKind,
    budget::{Invocation, Kind},
    git::{Broker, Executor, GitError},
    kit::{Assembly, KitReceipt, Machine, assemble, inventory::Inventory},
    lease::{LeaseError, Leases, Termination, hash, normalized, retirement_known},
    proof::{Definition, ProofSet},
    queue::{Candidate, Review},
    run::Runner,
    store::{Journal, JournalError},
    wire::{Action, Envelope, Facets, FileOp, Fresh, Labels, Risk, Verb, Verbatim, Verdict},
    worker::{ModelPin, Worker},
};
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
    /// The one trusted proof, named in the catalog, run by the engine and never described by a model.
    pub proof: Definition,
    pub proof_spawn: Spawner,
    /// Files the roles are shown, as (clean path, text). The roles run in an empty folder, so this is all they see.
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
        Self { schema: SCHEMA, done: BTreeSet::new(), turns: BTreeMap::new(), started: BTreeSet::new(), applied: 0, extra: Extra::new() }
    }
}

pub struct Coordinator<R: Roles, E: Executor> {
    roles: R,
    j: Journal,
    broker: Option<Broker<E>>,
    setup: Setup,
    p: Progress,
    marks: usize,
    crash_after: Option<usize>,
    crash_before: Option<usize>,
}

fn example(task: &str, base: &str, digest: &str, lease: u64, action: serde_json::Value, ping: Option<&str>) -> String {
    let mut envelope =
        serde_json::json!({"schema": 2, "task": task, "attempt": 1, "base": base, "contract": digest, "lease": lease, "action": action});
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
pub fn accept_plan(envelope: &Envelope, writable: &[String], proof: &Definition) -> Result<Task, &'static str> {
    let Action::Plan(draft) = &envelope.action else { return Err("error.wire.role") };
    let [one] = draft.tasks.as_slice() else { return Err("reason.plan.oneTask") };
    if one.kind != TaskKind::Writer || one.id == LEAD || one.id == REVIEWER || one.writes.is_empty() || one.writes.len() > 8 {
        return Err("reason.plan.oneTask");
    }
    if !one.proofs.is_empty() && one.proofs != [proof.id.clone()] {
        return Err("reason.proof.definition");
    }
    for path in &one.writes {
        let clean = normalized(path).map_err(|e| e.key())?;
        if !writable.iter().any(|w| clean == *w || clean.starts_with(&format!("{}/", w.trim_end_matches('/')))) {
            return Err("error.wire.path");
        }
    }
    let mut task = one.to_task();
    task.proofs = vec![proof.id.clone()];
    task.checks = proof.checks.clone();
    task.paths = one.writes.clone();
    Ok(task)
}

impl<R: Roles, E: Executor> Coordinator<R, E> {
    pub fn new(roles: R, journal: Journal, broker: Broker<E>, setup: Setup) -> Result<Self, &'static str> {
        if setup.pins.reviewer.vendor == setup.pins.writer.vendor {
            return Err("reason.queue.review");
        }
        let p = journal.get::<Progress>(&journal.private().join("coordinator.json")).map_err(|e| e.key())?.unwrap_or_else(Progress::new);
        Ok(Self { roles, j: journal, broker: Some(broker), setup, p, marks: 0, crash_after: None, crash_before: None })
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
        if self.crash_after == Some(self.marks) { Err(Stop::Crash) } else { Ok(()) }
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
        self.green(&task, &lease, &root).await?;
        self.review(&task, &root).await?;
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

    /// Record the worker (its brief, pinned kit and model) once; a replacement turn reuses it.
    fn wire(&mut self, role: Role, mut brief: Brief) -> Step<()> {
        let mut runner = self.runner()?;
        if runner.workers.contains_key(&brief.task) {
            return Ok(());
        }
        let pin = self.pin_of(role);
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
        Ok(())
    }

    /// One role turn, resumable: a recorded reply is reused, a live turn is awaited, a dead one is replaced.
    async fn turn(&mut self, role: Role, kind: Kind, brief: Brief, lease: Option<&Lease>) -> Step<(String, Envelope)> {
        let name = brief.task.clone();
        self.wire(role, brief)?;
        let pin = self.pin_of(role);
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
                    fresh: true,
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

    fn brief(&self, role: Role, task: &str, goal: &str, writes: &[String], lease: Option<u64>, quotes: Vec<Verbatim>) -> Step<Brief> {
        let runner = self.runner()?;
        let pin = self.contract_pin()?;
        let mut brief = Brief::new(task, 1, role, goal, &runner.mission.base, pin);
        brief.scope = self.setup.sources.iter().map(|(path, _)| path.clone()).collect();
        brief.sources = brief.scope.clone();
        brief.writes = writes.to_vec();
        brief.lease = lease;
        brief.quotes = quotes;
        brief.stop = "Stop after the single envelope.".into();
        Ok(brief)
    }

    fn contract_pin(&self) -> Step<ContractPin> {
        let contract =
            self.j.contracts()?.into_iter().find(|c| c.id == CONTRACT).ok_or_else(|| Stop::Block("reason.contract.revised".into()))?;
        Ok(ContractPin { id: contract.id, digest: contract.digest })
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
        if let Some(task) = self.setup.plan.clone() {
            self.j.save_plan(&Plan::new(&runner.mission.id, 1, vec![task.clone()]))?;
            self.mark("plan")?;
            return Ok(task);
        }
        let pin = self.contract_pin()?;
        let plan_shape = serde_json::json!({"plan": {
            "summary": "One sentence on what changes and why.",
            "acceptance": "What the proof checks.",
            "tasks": [{"id": "writer", "title": "A short title", "kind": "writer", "writes": ["path/under/an/allowed/folder"]}]
        }});
        let allowed = self.setup.writable.join(", ");
        let quotes = vec![
            Verbatim { id: "goal".into(), text: runner.mission.goal.clone() },
            Verbatim { id: "files".into(), text: files_text(&self.setup.sources) },
            Verbatim {
                id: "format".into(),
                text: format!(
                    "{ENVELOPE_RULES} Plan exactly one writer task. Its writes list only paths under: {allowed}. Do not name a proof: the engine runs its own.\n{}",
                    example(LEAD, &runner.mission.base, &pin.digest, 0, plan_shape, None)
                ),
            },
        ];
        let brief = self.brief(Role::Lead, LEAD, "Plan the mission as one writer task", &[], None, quotes)?;
        let (_, reply) = self.turn(Role::Lead, Kind::Lead, brief, None).await?;
        let task = accept_plan(&reply, &self.setup.writable, &self.setup.proof)?;
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
        Ok(ProofSet::new(tree.into(), input.into(), vec![self.setup.proof.clone()], paths)?)
    }

    /// The engine proves the base red before any patch: a proof that already passes proves nothing.
    async fn red(&mut self, task: &Task, root: &std::path::Path) -> Step<()> {
        if self.done("red") {
            return Ok(());
        }
        let mut runner = self.runner()?;
        let base = runner.mission.base.clone();
        let mut red = self.proof_set(&base, "red", task.writes.clone())?;
        let id = self.setup.proof.id.clone();
        runner.run_proof(&self.j, &task.id, &id, &mut red, root, &self.setup.proof_spawn).await?;
        if red.verified() {
            return Err(Stop::Block("reason.proof.notRed".into()));
        }
        self.mark("red")
    }

    fn applied(root: &std::path::Path, op: &FileOp) -> bool {
        let read = |path: &str| std::fs::read(root.join(path)).ok();
        match op {
            FileOp::Write(w) => read(&w.path).is_some_and(|bytes| bytes == w.content.as_bytes()),
            FileOp::Delete(d) => !root.join(&d.path).exists(),
            FileOp::Rename(r) => !root.join(&r.from).exists() && root.join(&r.to).exists(),
        }
    }

    async fn write(&mut self, task: &Task, lease: &Lease, root: &std::path::Path) -> Step<()> {
        if self.done("write") {
            return Ok(());
        }
        self.effects_settled()?;
        let runner = self.runner()?;
        let pin = self.contract_pin()?;
        let base = runner.mission.base.clone();
        let current: Vec<(String, String)> =
            task.writes.iter().map(|p| (p.clone(), std::fs::read_to_string(root.join(p)).unwrap_or_default())).collect();
        let existing: Vec<String> = task.writes.iter().filter(|p| root.join(p).is_file()).cloned().collect();
        let shape = serde_json::json!({"patch": {"ops": [{"write": {
            "path": task.writes[0], "expect": existing.first().map(|p| hash(std::fs::read(root.join(p)).unwrap_or_default().as_slice())),
            "content": "the complete new text of the file"}}]}});
        let quotes = vec![
            Verbatim { id: "goal".into(), text: runner.mission.goal.clone() },
            Verbatim { id: "files".into(), text: files_text(&self.setup.sources) },
            Verbatim { id: "targets".into(), text: files_text(&current) },
            Verbatim {
                id: "format".into(),
                text: format!(
                    "{ENVELOPE_RULES} Write only these paths: {}. A file that exists is replaced whole and its expect is the sha256 of its current text shown above; a new file has expect null.\n{}",
                    task.writes.join(", "),
                    example(&task.id, &base, &pin.digest, lease.generation, shape, None)
                ),
            },
        ];
        let brief = self.brief(Role::Writer, &task.id, &task.title, &task.writes, Some(lease.generation), quotes)?;
        let (_, reply) = self.turn(Role::Writer, Kind::Worker, brief, Some(lease)).await?;
        let Action::Patch(patch) = &reply.action else { return Err(Stop::Block("error.wire.role".into())) };
        let grant = (self.setup.grant)()?.ok_or_else(|| Stop::Block("reason.grant.none".into()))?;
        let revision = self.setup.revision;
        for (index, op) in patch.ops.iter().enumerate() {
            if index < self.p.applied || Self::applied(root, op) {
                self.p.applied = self.p.applied.max(index + 1);
                continue;
            }
            self.broker()?.leases().patch(lease, Some(&grant), revision, op)?;
            self.p.applied = index + 1;
            self.save()?;
        }
        self.mark("write")
    }

    /// Compose the tree, prove it green and queue the candidate with its proof.
    async fn green(&mut self, task: &Task, lease: &Lease, root: &std::path::Path) -> Step<()> {
        if self.done("green") {
            return Ok(());
        }
        self.effects_settled()?;
        let mut runner = self.runner()?;
        if runner.queue.get(&task.id).is_none() {
            let paths = self.broker()?.leases().audit(lease)?;
            let grant = self.grant();
            let tree = self.broker()?.compose_tree(grant, lease, &paths)?;
            let input = runner.workers.get(&task.id).ok_or_else(|| Stop::Block("error.wire.stale".into()))?.digest();
            let mut proof = self.proof_set(&tree, &input, paths.clone())?;
            let id = self.setup.proof.id.clone();
            runner.run_proof(&self.j, &task.id, &id, &mut proof, root, &self.setup.proof_spawn).await?;
            if !proof.verified() {
                return Err(Stop::Block("reason.proof.failed".into()));
            }
            let candidate = Candidate::new(
                &task.id,
                runner.mission.base.clone(),
                tree,
                runner.mission.grant_revision,
                self.setup.pins.writer.vendor,
                input,
                paths,
                vec![self.setup.proof.clone()],
            )?;
            runner.queue.enqueue(candidate)?;
            runner.queue.prove(&task.id, proof)?;
            runner.save(&self.j)?;
        }
        self.mark("green")
    }

    /// A fresh role of the other vendor reads the exact tree and the engine's receipt, and answers in one envelope.
    async fn review(&mut self, task: &Task, root: &std::path::Path) -> Step<()> {
        if self.done("review") {
            return Ok(());
        }
        let runner = self.runner()?;
        let candidate = runner.queue.get(&task.id).ok_or_else(|| Stop::Block("reason.queue.serial".into()))?.clone();
        let proof = candidate.proof().ok_or_else(|| Stop::Block("reason.queue.proof".into()))?;
        let receipt = proof.receipts().next().ok_or_else(|| Stop::Block("reason.queue.proof".into()))?.id.clone();
        let pin = self.contract_pin()?;
        let changed: Vec<(String, String)> =
            candidate.paths.iter().map(|p| (p.clone(), std::fs::read_to_string(root.join(p)).unwrap_or_default())).collect();
        let verdict = serde_json::json!({"complete": {
            "summary": "One sentence on whether the candidate meets the goal.", "verdict": "pass", "next": "land",
            "receipt": receipt, "findings": []}});
        let quotes = vec![
            Verbatim { id: "goal".into(), text: runner.mission.goal.clone() },
            Verbatim { id: "files".into(), text: files_text(&self.setup.sources) },
            Verbatim {
                id: "candidate".into(),
                text: format!(
                    "{}\nThe engine ran the proof {} on this exact tree and it exited 0. Engine receipt id: {receipt}.",
                    files_text(&changed),
                    self.setup.proof.id
                ),
            },
            Verbatim {
                id: "format".into(),
                text: format!(
                    "{ENVELOPE_RULES} Keep lease 0, keep ping \"DoneReady\", keep next and keep the receipt id. Change only the summary and, if you find a defect that makes the candidate wrong, verdict (\"block\") and findings (one short line each).\n{}",
                    example(REVIEWER, &runner.mission.base, &pin.digest, 0, verdict, Some("DoneReady"))
                ),
            },
        ];
        let brief = self.brief(Role::Reviewer, REVIEWER, "Review the candidate against the goal", &[], None, quotes)?;
        let (inv, reply) = self.turn(Role::Reviewer, Kind::Review, brief, None).await?;
        let Action::Complete(done) = &reply.action else { return Err(Stop::Block("error.wire.role".into())) };
        let accepted = done.verdict == Some(Verdict::Pass) && done.findings.is_empty();
        let digest = proof.digest();
        let mut runner = self.runner()?;
        // A crash after the review was saved resumes here: the same session is already on the candidate.
        if runner.queue.get(&task.id).and_then(Candidate::review).is_some_and(|r| r.session == inv && r.accepted && r.findings.is_empty()) {
            return self.mark("review");
        }
        let review = Review {
            vendor: self.setup.pins.reviewer.vendor,
            tree: candidate.tree.clone(),
            proof_digest: digest,
            session: inv,
            fresh: true,
            accepted,
            findings: done.findings.clone(),
        };
        let outcome = runner.queue.review(&task.id, review);
        runner.save(&self.j)?;
        if !accepted || outcome.is_err() {
            return Err(Stop::Block("reason.queue.review".into()));
        }
        self.mark("review")
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
        let task = accept_plan(&envelope, &["src".into()], &proof()).unwrap();
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
        assert!(accept_plan(&one(serde_json::json!({})), &writable, &proof()).is_ok());
        assert!(accept_plan(&one(serde_json::json!({"proofs": ["node-check"]})), &writable, &proof()).is_ok());
        assert_eq!(accept_plan(&one(serde_json::json!({"proofs": ["rm-rf"]})), &writable, &proof()), Err("reason.proof.definition"));
        assert_eq!(accept_plan(&one(serde_json::json!({"writes": ["other/a.js"]})), &writable, &proof()), Err("error.wire.path"));
        assert_eq!(accept_plan(&one(serde_json::json!({"id": "reviewer"})), &writable, &proof()), Err("reason.plan.oneTask"));
        assert_eq!(
            accept_plan(&one(serde_json::json!({"kind": "research", "writes": []})), &writable, &proof()),
            Err("reason.plan.oneTask")
        );
        let two = plan_with(serde_json::json!([
            {"id": "writer", "title": "t", "kind": "writer", "writes": ["src/a.js"]},
            {"id": "second", "title": "t", "kind": "writer", "writes": ["src/b.js"]}
        ]));
        assert_eq!(accept_plan(&two, &writable, &proof()), Err("reason.plan.oneTask"));
        let patch = example(
            "writer",
            BASE,
            DIGEST,
            2,
            serde_json::json!({"patch": {"ops": [{"write": {"path": "src/a.js", "expect": null, "content": "x"}}]}}),
            None,
        );
        let patch = Envelope::parse(&patch, &expect("writer", 2), Role::Writer).unwrap();
        assert_eq!(accept_plan(&patch, &writable, &proof()), Err("error.wire.role"));
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
