//! Mission state and event-driven scheduling; no wave barriers or polling fleet.
use super::{
    Contract, Decision, Extra, Intent, IntentKind, Mission, MissionState, Plan, Record, Role, SCHEMA, TaskKind, TaskState, queue::Queue,
    store::Journal, worker::Worker,
};
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
pub struct Capacity {
    pub cost: bool,
    pub daily: bool,
    pub ram_slots: usize,
    pub review_slots: usize,
    pub waiting_candidates: usize,
    pub writer_active: bool,
}
pub struct Ready {
    pub start: Vec<String>,
    pub wait: BTreeMap<String, &'static str>,
    pub blocked: BTreeMap<String, &'static str>,
}
pub fn schedule(plan: &Plan, contracts: &[Contract], decisions: &[Decision], mut capacity: Capacity) -> Result<Ready, &'static str> {
    plan.validate().map_err(|e| e.key())?;
    let mut ready = Ready { start: vec![], wait: BTreeMap::new(), blocked: BTreeMap::new() };
    let done: BTreeSet<_> = plan.tasks.iter().filter(|t| t.state == TaskState::Done).map(|t| t.id.as_str()).collect();
    let mut lengths = BTreeMap::new();
    for _ in 0..plan.tasks.len() {
        for t in &plan.tasks {
            let next = plan
                .tasks
                .iter()
                .filter(|n| n.requires.contains(&t.id))
                .map(|n| lengths.get(&n.id).copied().unwrap_or(0))
                .max()
                .unwrap_or(0);
            lengths.insert(t.id.clone(), 1 + next);
        }
    }
    let mut tasks: Vec<_> = plan.tasks.iter().enumerate().collect();
    tasks.sort_by(|(a_index, a), (b_index, b)| {
        let number = |t: &super::Task, key: &str| t.extra.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0);
        lengths[&b.id]
            .cmp(&lengths[&a.id])
            .then_with(|| number(b, "value").cmp(&number(a, "value")))
            .then_with(|| number(a, "readyAt").cmp(&number(b, "readyAt")))
            .then_with(|| a_index.cmp(b_index))
    });
    for (_, task) in tasks {
        if matches!(task.state, TaskState::Running | TaskState::Review | TaskState::Done) {
            continue;
        }
        let blocked = if task
            .reason
            .as_deref()
            .is_some_and(|r| matches!(r, "reason.loop" | "reason.stall" | "reason.worker.blocked" | "reason.lease.quarantined"))
        {
            Some("reason.worker.blocked")
        } else if !task.requires.iter().all(|id| done.contains(id.as_str())) {
            Some("reason.task.dependency")
        } else if !task.decisions.iter().all(|id| decisions.iter().any(|d| &d.id == id && d.answer.is_some())) {
            Some("reason.task.decision")
        } else if !task.contracts.iter().all(|pin| contracts.iter().any(|c| c.id == pin.id && c.digest == pin.digest && c.verify())) {
            Some("reason.contract.revised")
        } else {
            None
        };
        if let Some(reason) = blocked {
            ready.blocked.insert(task.id.clone(), reason);
            continue;
        }
        let writer = task.kind.writes();
        let wait = if !capacity.cost {
            Some("reason.budget.cap")
        } else if !capacity.daily {
            Some("reason.budget.daily")
        } else if capacity.ram_slots == 0 {
            Some("reason.ram")
        } else if writer && capacity.writer_active {
            Some("reason.lease.conflict")
        } else if writer && capacity.waiting_candidates >= 2 {
            Some("reason.queue.capacity")
        } else if (writer || task.kind == TaskKind::Review) && capacity.review_slots == 0 {
            Some("reason.review.capacity")
        } else {
            None
        };
        if let Some(reason) = wait {
            ready.wait.insert(task.id.clone(), reason);
            continue;
        }
        capacity.ram_slots -= 1;
        if writer {
            capacity.writer_active = true;
            capacity.review_slots -= 1;
        } else if task.kind == TaskKind::Review {
            capacity.review_slots -= 1;
        }
        ready.start.push(task.id.clone());
    }
    Ok(ready)
}
pub fn invalidated(plan: &Plan, paths: &[String]) -> BTreeSet<String> {
    let roots: BTreeSet<String> = plan
        .tasks
        .iter()
        .filter(|t| t.paths.iter().any(|p| paths.iter().any(|changed| super::overlaps(p, changed))))
        .map(|t| t.id.clone())
        .collect();
    roots.union(&plan.descendants(&roots)).cloned().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Retirement {
    lease: super::Lease,
    process: Option<super::ProcessId>,
    cleanup_intent: String,
    #[serde(default)]
    abandoned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runner {
    schema: u32,
    pub mission: Mission,
    pub workers: BTreeMap<String, Worker>,
    pub queue: Queue,
    pub quota_wake: Option<u64>,
    pub promotion_pending: bool,
    #[serde(default)]
    pub landings: Vec<Landing>,
    halted: bool,
    #[serde(default)]
    pub role_tokens: BTreeMap<String, super::report::RoleTokens>,
    #[serde(default)]
    retirements: BTreeMap<String, Retirement>,
    #[serde(default)]
    heavy_proof: Option<(String, String, String, String)>,
    #[serde(flatten)]
    extra: Extra,
}
impl Record for Runner {
    fn schema(&self) -> u32 {
        self.schema
    }
}

/// Accepted evidence retained in the private checkpoint after queue retirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Landing {
    pub candidate: super::queue::Candidate,
    pub commit_intent: String,
    pub forward_intent: String,
    pub at: u64,
}
impl Runner {
    pub fn accepted_landing(&self, j: &Journal, commit: &str, tree: &str) -> Result<&Landing, &'static str> {
        for landing in &self.landings {
            let candidate = &landing.candidate;
            if candidate.tree != tree || candidate.grant_revision != self.mission.grant_revision {
                continue;
            }
            let committed = j.effect_receipt(&landing.commit_intent).map_err(|e| e.key())?.ok_or("reason.receipt.missing")?;
            let forwarded = j.effect_receipt(&landing.forward_intent).map_err(|e| e.key())?.ok_or("reason.receipt.missing")?;
            if committed.outcome != super::Outcome::Done
                || forwarded.outcome != super::Outcome::Done
                || committed.evidence.get("head").map(String::as_str) != Some(commit)
                || committed.evidence.get("tree").map(String::as_str) != Some(tree)
                || committed.evidence.get("parent") != Some(&candidate.base)
                || forwarded.evidence.get("old") != Some(&candidate.base)
                || forwarded.evidence.get("new").map(String::as_str) != Some(commit)
            {
                continue;
            }
            let mut queue = Queue::default();
            queue.enqueue(candidate.clone())?;
            queue.accepted(&candidate.id, &candidate.base, candidate.grant_revision)?;
            return Ok(landing);
        }
        Err("reason.receipt.missing")
    }
    pub fn record_tokens(
        &mut self,
        role: &str,
        input: Option<u64>,
        cache_read: Option<u64>,
        cache_write: Option<u64>,
        output: Option<u64>,
    ) {
        use super::report::{RoleTokens, UnknownOr};
        let to_metric = |opt: Option<u64>| opt.map_or(UnknownOr::Unknown("unknown".into()), UnknownOr::Known);
        self.role_tokens.insert(
            role.to_lowercase(),
            RoleTokens {
                input: to_metric(input),
                cache_read: to_metric(cache_read),
                cache_write: to_metric(cache_write),
                output: to_metric(output),
            },
        );
    }
    pub fn update_quiescent(&self) -> Result<(), &'static str> {
        if !matches!(self.mission.state, MissionState::Paused | MissionState::Done)
            || self.heavy_proof.is_some()
            || !self.retirements.is_empty()
        {
            return Err("reason.budget.paused");
        }
        Ok(())
    }
    fn new(mission: Mission) -> Self {
        Self {
            schema: SCHEMA,
            mission,
            workers: BTreeMap::new(),
            queue: Queue::default(),
            quota_wake: None,
            promotion_pending: false,
            landings: Vec::new(),
            halted: false,
            role_tokens: BTreeMap::new(),
            retirements: BTreeMap::new(),
            heavy_proof: None,
            extra: Extra::new(),
        }
    }
    pub fn start(j: &Journal, engine: &crate::Engine, mission: Mission) -> Result<Self, String> {
        let _update = engine.mission_update_admission()?;
        engine.mission_authority(&mission)?;
        Self::role_vendor(engine)?;
        Self::create(j, mission).map_err(str::to_string)
    }
    pub fn role_vendor(engine: &crate::Engine) -> Result<Vendor, String> {
        let mut vendors = engine.vendors();
        if vendors.is_empty() {
            vendors.extend(Vendor::ALL);
        }
        let mut reason = None;
        for vendor in vendors {
            match super::policy::admit(vendor) {
                Ok(()) => return Ok(vendor),
                Err(e) => {
                    reason.get_or_insert(e.key());
                }
            }
        }
        Err(reason.unwrap_or("reason.project.restrictionsUnproven").into())
    }
    /// Adopt a legacy T1/T9 record once; private checkpoints always win after that.
    pub fn open(j: &Journal) -> Result<Self, &'static str> {
        if j.get::<Self>(&j.private().join("run.json")).map_err(|e| e.key())?.is_some() {
            return Self::load(j);
        }
        let mut mission = j.mission().map_err(|e| e.key())?.ok_or("error.journal.mismatch")?;
        if !super::git::valid_oid(&mission.base) || mission.branch != format!("faber/{}", mission.id) || mission.grant_revision == 0 {
            return Err("error.store.corrupt");
        }
        if mission.state == MissionState::Running {
            mission.state = MissionState::Paused;
            mission.reason = Some("reason.interrupted".into());
        }
        let mut runner = Self::new(mission);
        runner.save(j)?;
        Ok(runner)
    }
    pub fn create(j: &Journal, mission: Mission) -> Result<Self, &'static str> {
        if mission.schema != SCHEMA
            || mission.state != MissionState::Planning
            || !super::valid_id(&mission.id)
            || !super::git::valid_oid(&mission.base)
            || mission.branch != format!("faber/{}", mission.id)
            || mission.grant_revision == 0
            || j.mission().map_err(|e| e.key())?.is_some()
            || j.get::<Self>(&j.private().join("run.json")).map_err(|e| e.key())?.is_some()
        {
            return Err("reason.project.exists");
        }
        let mut runner = Self::new(mission);
        runner.save(j)?;
        Ok(runner)
    }
    pub fn load(j: &Journal) -> Result<Self, &'static str> {
        let mut runner: Self = j.get(&j.private().join("run.json")).map_err(|e| e.key())?.ok_or("error.store.corrupt")?;
        if runner.schema != SCHEMA
            || j.private().file_name().and_then(|s| s.to_str()) != Some(&runner.mission.id)
            || runner.workers.values().any(|w| w.schema != SCHEMA)
        {
            return Err("error.store.corrupt");
        }
        match j.mission() {
            Ok(mirror) if mirror.as_ref() == Some(&runner.mission) => {}
            Err(super::store::JournalError::Newer) => return Err("error.journal.newer"),
            _ => j.save_mission(&runner.mission).map_err(|e| e.key())?,
        }
        if let Some((task, id, identity, receipt_id)) = runner.heavy_proof.clone()
            && let Some(receipt) = j.check(&receipt_id).map_err(|e| e.key())?
        {
            if receipt.task != task
                || receipt.id != receipt_id
                || Self::receipt_identity(&receipt) != identity
                || receipt.extra.get("definition").and_then(|d| d.get("id")).and_then(serde_json::Value::as_str) != Some(&id)
            {
                return Err("reason.proof.receipt");
            }
            j.put(&j.records().join("checks").join(format!("{receipt_id}.json")), &receipt).map_err(|e| e.key())?;
            runner.heavy_proof = None;
            runner.save(j)?;
        }
        Ok(runner)
    }
    pub fn save(&mut self, j: &Journal) -> Result<(), &'static str> {
        if self.halted {
            return Err("error.store.write");
        }
        let result =
            j.put(&j.private().join("run.json"), self).and_then(|()| j.save_mission(&self.mission)).and_then(|()| self.write_report(j));
        if let Err(e) = result {
            self.halt(j);
            return Err(e.key());
        }
        j.mission_checkpointed(&self.mission);
        Ok(())
    }
    fn halt(&mut self, j: &Journal) {
        self.halted = true;
        let mut interrupted = self.mission.clone();
        interrupted.state = MissionState::Paused;
        j.mission_checkpointed(&interrupted);
    }
    pub fn pause(&mut self, j: &Journal) -> Result<(), &'static str> {
        self.mission.state = MissionState::Paused;
        self.quota_wake = None;
        self.promotion_pending = false;
        self.save(j)
    }
    pub fn reconcile(&self, j: &Journal, revision: u64) -> Result<(), &'static str> {
        if self.halted || self.mission.state != MissionState::Paused || revision != self.mission.grant_revision {
            return Err("reason.grant.stale");
        }
        if !j.pending().map_err(|e| e.key())?.is_empty() {
            return Err("reason.project.reconcile");
        }
        if !self.retirements.is_empty() {
            return Err("reason.project.reconcile");
        }
        if self.heavy_proof.is_some() {
            return Err("reason.proof.busy");
        }
        let runtime = j.runtime().map_err(|e| e.key())?;
        if runtime.leases.iter().any(|l| l.process.is_some() && !super::lease::retirement_known(l)) {
            return Err("reason.lease.live");
        }
        // Reconciliation never adopts or kills a recorded PID. T2 owns handles; T3 owns unsettled reservations.
        for w in self.workers.values() {
            w.pin.validate()?;
        }
        Ok(())
    }
    pub fn resume(&mut self, j: &Journal, engine: &crate::Engine) -> Result<(), String> {
        engine.mission_authority(&self.mission)?;
        if self.workers.is_empty() {
            Self::role_vendor(engine)?;
        }
        for worker in self.workers.values() {
            super::policy::admit(worker.pin.vendor).map_err(|e| e.key().to_string())?;
        }
        self.resume_authorized(j, engine)
    }
    pub(crate) fn resume_authorized(&mut self, j: &Journal, engine: &crate::Engine) -> Result<(), String> {
        let _update = engine.mission_update_admission()?;
        let grant = engine.mission_authority(&self.mission)?;
        self.settle_dispatches(j, engine)?;
        self.reconcile(j, grant.revision).map_err(str::to_string)?;
        if engine.mission_unsettled(&self.mission.id)? {
            return Err("reason.project.reconcile".into());
        }
        engine.mission_budget(super::budget::Change::Pause { mission: self.mission.id.clone(), paused: false })?;
        self.mission.state = MissionState::Running;
        self.mission.reason = None;
        if let Err(key) = self.save(j) {
            let _ = engine.mission_budget(super::budget::Change::Pause { mission: self.mission.id.clone(), paused: true });
            return Err(key.into());
        }
        Ok(())
    }
    /// Only the shared ledger's known terminated and priced invocation can close a legacy dispatch intent.
    pub fn settle_dispatches(&mut self, j: &Journal, engine: &crate::Engine) -> Result<(), String> {
        for intent in j.pending().map_err(|e| e.key().to_string())? {
            if intent.kind != IntentKind::Dispatch
                || intent.expected.get("invocation") != Some(&intent.id)
                || intent.expected.contains_key("generation")
            {
                continue;
            }
            let reservation = engine.mission_reservation(&intent.id)?;
            if reservation.request.mission != self.mission.id
                || intent.mission != self.mission.id
                || intent.task.as_deref() != Some(&reservation.request.worker)
                || intent.grant_revision != self.mission.grant_revision
            {
                return Err("error.project.invocation".into());
            }
            if !reservation.terminated || !reservation.settled {
                continue;
            }
            let mut receipt = super::Receipt::new(&intent.id, super::Outcome::Done, self.mission.updated_at);
            receipt.evidence.insert("invocation".into(), intent.id);
            j.record_receipt(&receipt).map_err(|e| e.key().to_string())?;
        }
        Ok(())
    }
    pub fn admit(&self, vendor: Vendor) -> Result<(), &'static str> {
        if self.halted {
            return Err("error.store.write");
        }
        if self.mission.state == MissionState::Paused {
            return Err("reason.budget.paused");
        }
        super::policy::admit(vendor).map_err(|e| e.key())
    }
    fn proof_identity(id: &str, proof: &super::proof::ProofSet) -> String {
        super::lease::hash(
            serde_json::json!({"tree":proof.tree,"input":proof.input,
            "definition":proof.definitions().iter().find(|d| d.id == id),"paths":proof.paths()})
            .to_string()
            .as_bytes(),
        )
    }
    fn receipt_identity(receipt: &super::wire::CheckReceipt) -> String {
        super::lease::hash(
            serde_json::json!({"tree":receipt.candidate,"input":receipt.extra.get("input"),
            "definition":receipt.extra.get("definition"),"paths":receipt.extra.get("changedPaths")})
            .to_string()
            .as_bytes(),
        )
    }
    pub async fn run_proof(
        &mut self,
        j: &Journal,
        task: &str,
        id: &str,
        proof: &mut super::proof::ProofSet,
        cwd: &std::path::Path,
        spawn: &crate::proc::Spawner,
    ) -> Result<super::wire::CheckReceipt, &'static str> {
        let receipt = self.start_proof(j, task, id, proof)?;
        let definition = proof.definitions().iter().find(|d| d.id == id).ok_or("reason.proof.definition")?;
        let path = format!("proofs/{receipt}.log");
        let mut observation = super::proof::execute(definition, cwd, spawn, path.clone()).await;
        // The proof's output is code's output and may hold a printed secret: every line that looks like one is withheld before
        // the whole output is kept in the mission's private folder and read into the short report the roles get (R155).
        let mut printed = observation.stdout.clone();
        printed.extend(&observation.stderr);
        let whole = super::proof::output::withheld(&String::from_utf8_lossy(&printed));
        let mut report = super::proof::output::read(&whole, observation.exit, &cwd.to_string_lossy());
        report.summary = format!("{id} ({}) {}", definition.toolchain, report.summary);
        // The receipt's evidence is a log with the exit code and the sizes only, and it hashes exactly what is stored (the
        // journal checks that on acceptance).
        let log =
            format!("exit {:?}\nstdout {} bytes\nstderr {} bytes\n", observation.exit, observation.stdout.len(), observation.stderr.len());
        // From here the stored log is the observation's output, so the receipt checks the hash of exactly what is stored.
        observation.stdout = log.clone().into_bytes();
        observation.stderr = vec![];
        observation.evidence.sha256 = super::kit::digest(&observation.stdout);
        std::fs::create_dir_all(j.private().join("proofs")).map_err(|_| "error.store.write")?;
        std::fs::write(j.private().join(path), log).map_err(|_| "error.store.write")?;
        std::fs::write(j.private().join(format!("proofs/{receipt}.output")), whole).map_err(|_| "error.store.write")?;
        std::fs::write(j.private().join(format!("proofs/{receipt}.report")), super::proof::output::withheld(&report.text()))
            .map_err(|_| "error.store.write")?;
        self.finish_proof(j, task, proof, &receipt, id, observation)
    }
    /// What a role reads of a proof the engine ran (R155): its failures with `file:line` and the tool's fix, its
    /// measurements and one summary line. The whole output stays in `proofs/<receipt>.output` in the private folder.
    pub fn proof_report(j: &Journal, receipt: &str) -> Option<String> {
        if !super::valid_id(receipt) {
            return None;
        }
        std::fs::read_to_string(j.private().join(format!("proofs/{receipt}.report"))).ok()
    }
    pub fn start_proof(&mut self, j: &Journal, task: &str, id: &str, proof: &mut super::proof::ProofSet) -> Result<String, &'static str> {
        if self.halted || self.mission.state != MissionState::Running {
            return Err("reason.budget.paused");
        }
        if !super::valid_id(task) {
            return Err("error.wire.stale");
        }
        let definition = proof.definitions().iter().find(|d| d.id == id).ok_or("reason.proof.definition")?;
        if definition.heavy {
            let persisted: Self = j.get(&j.private().join("run.json")).map_err(|e| e.key())?.ok_or("error.store.corrupt")?;
            if self.heavy_proof.is_some() || persisted.heavy_proof.is_some() {
                return Err("reason.proof.busy");
            }
        }
        let heavy = definition.heavy;
        let receipt = uuid::Uuid::new_v4().to_string();
        proof.start(id)?;
        if heavy {
            self.heavy_proof = Some((task.into(), id.into(), Self::proof_identity(id, proof), receipt.clone()));
        }
        self.save(j)?;
        Ok(receipt)
    }
    pub fn finish_proof(
        &mut self,
        j: &Journal,
        task: &str,
        proof: &mut super::proof::ProofSet,
        receipt: &str,
        id: &str,
        observation: super::proof::Observation,
    ) -> Result<super::wire::CheckReceipt, &'static str> {
        let definition = proof.definitions().iter().find(|d| d.id == id).ok_or("reason.proof.definition")?;
        let heavy = definition.heavy;
        if heavy && self.heavy_proof.as_ref() != Some(&(task.into(), id.into(), Self::proof_identity(id, proof), receipt.into())) {
            return Err("reason.proof.busy");
        }
        let mut receipt = proof.record(receipt, id, observation)?;
        receipt.task = task.into();
        if let Err(e) = j.record_check(&receipt) {
            self.halt(j);
            return Err(e.key());
        }
        if heavy {
            self.heavy_proof = None;
        }
        self.save(j)?;
        Ok(receipt)
    }
    /// Persist reflexes before returning any action. Malformed input never invokes a driver.
    pub fn receive(&mut self, j: &Journal, task: &str, text: &str) -> Result<super::worker::Reply, &'static str> {
        if self.halted || matches!(self.mission.state, MissionState::Paused | MissionState::Done) {
            return Err("reason.budget.paused");
        }
        let reply = self.workers.get_mut(task).ok_or("error.wire.stale")?.reply(text);
        self.save(j)?;
        Ok(reply)
    }
    pub fn prompt(&self, j: &Journal, task: &str, bodies: &[super::kit::InlineBody]) -> Result<String, &'static str> {
        let worker = self.workers.get(task).ok_or("error.wire.stale")?;
        let kit = j.kit(task, worker.brief.attempt).map_err(|e| e.key())?.ok_or("reason.kit.missing")?;
        worker.kit_prompt(&kit, bodies)
    }
    /// The engine's current grant and shared spending ledger are read for each invocation, never cached in a worker.
    pub fn prepare_dispatch(&mut self, j: &Journal, engine: &crate::Engine, request: super::budget::Invocation) -> Result<(), String> {
        self.admit(request.vendor).map_err(str::to_string)?;
        let worker = self.workers.get(&request.worker).ok_or("error.wire.stale")?;
        let kind = match worker.brief.role {
            Role::Lead => super::budget::Kind::Lead,
            Role::Reviewer => super::budget::Kind::Review,
            _ => super::budget::Kind::Worker,
        };
        if request.mission != self.mission.id
            || request.model != worker.pin.model
            || request.effort != worker.pin.effort
            || request.vendor != worker.pin.vendor
            || request.kind != kind
        {
            return Err("error.project.invocation".into());
        }
        if worker.brief.role == Role::Writer {
            let launch =
                j.pending().map_err(|e| e.key().to_string())?.into_iter().find(|i| i.id == request.id).ok_or("reason.project.reconcile")?;
            if launch.kind != IntentKind::Dispatch
                || launch.task.as_deref() != Some(&request.worker)
                || launch.grant_revision != self.mission.grant_revision
                || launch.expected.get("generation") != worker.brief.lease.map(|n| n.to_string()).as_ref()
            {
                return Err("reason.project.reconcile".into());
            }
        }
        engine.mission_authority(&self.mission)?;
        engine.mission_budget(super::budget::Change::Admit(request.clone()))?;
        if worker.brief.role == Role::Writer {
            return Ok(());
        }
        let mut intent =
            Intent::new(&request.id, &self.mission.id, IntentKind::Dispatch, self.mission.grant_revision, self.mission.updated_at);
        intent.task = Some(request.worker);
        intent.expected.insert("invocation".into(), request.id);
        j.record_intent(&intent).map_err(|e| e.key().to_string())?;
        // The actual driver constructor also rechecks T2. No caller can select a legacy unrestricted session here.
        Ok(())
    }
    pub fn continuation(&self, engine: &crate::Engine, invocation: &str, vendor: Vendor) -> Result<(), String> {
        self.admit(vendor).map_err(str::to_string)?;
        engine.mission_authority(&self.mission)?;
        engine.mission_budget(super::budget::Change::Continue { id: invocation.into() }).map(|_| ())
    }
    fn changed(&mut self, at: u64) -> Result<(), &'static str> {
        if self.halted {
            return Err("error.store.write");
        }
        if self.mission.state == MissionState::Done {
            return Err("error.wire.stale");
        }
        let revision = self.mission.extra.get("viewRevision").and_then(serde_json::Value::as_u64).unwrap_or(self.mission.updated_at);
        self.mission.extra.insert("viewRevision".into(), revision.checked_add(1).ok_or("error.wire.stale")?.into());
        self.mission.updated_at = at;
        Ok(())
    }
    pub fn mode(&mut self, j: &Journal, mode: super::RunMode, at: u64) -> Result<(), &'static str> {
        self.changed(at)?;
        self.mission.mode = mode;
        self.save(j)
    }
    pub fn answer(&mut self, j: &Journal, id: &str, answer: &str, at: u64) -> Result<(), &'static str> {
        if !super::valid_id(id) {
            return Err("error.journal.badId");
        }
        if answer.len() > super::wire::MAX_TEXT {
            return Err("error.wire.tooBig");
        }
        if answer.trim().is_empty() || answer.contains('\0') {
            return Err("error.wire.malformed");
        }
        self.changed(at)?;
        let mut decision = j.decision(id).map_err(|e| e.key())?.ok_or("error.decision.option")?;
        decision.answer(super::DecidedBy::Person, answer, at)?;
        if let Err(e) = j.save_decision(&decision) {
            self.halt(j);
            return Err(e.key());
        }
        self.save(j)
    }
    pub async fn stop(&mut self, j: &Journal, engine: &crate::Engine) -> Result<(), String> {
        // Close memory admission before any fallible IO. Never free leases or reservations from a stop request.
        if self.mission.state == MissionState::Done {
            return Err("error.wire.stale".into());
        }
        let changed = self.changed(engine.mission_time()).map_err(str::to_string);
        self.mission.state = MissionState::Paused;
        self.mission.reason = Some("reason.interrupted".into());
        self.quota_wake = None;
        self.promotion_pending = false;
        let sessions = engine.mission_sessions(&self.mission.id);
        let budget = engine.pause_mission_budget(&self.mission.id);
        let saved = self.save(j).map_err(str::to_string);
        for session in &sessions {
            session.interrupt();
        }
        if !sessions.is_empty() {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        for session in &sessions {
            session.close();
        }
        budget?;
        changed?;
        saved
    }
    pub fn install_plan(
        &mut self,
        j: &Journal,
        plan: Plan,
        compile: impl FnOnce(&Plan) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        if self.halted || plan.mission != self.mission.id {
            return Err("error.wire.stale");
        }
        let plan = super::lead::accept(plan, &j.contracts().map_err(|e| e.key())?, compile)?;
        if let Err(e) = j.save_plan(&plan) {
            self.halt(j);
            return Err(e.key());
        }
        self.mission.state = MissionState::Running;
        self.save(j)
    }
    pub fn ready(&mut self, j: &Journal, capacity: Capacity) -> Result<Ready, &'static str> {
        let plan = j.plan().map_err(|e| e.key())?.ok_or("reason.project.compile")?;
        schedule(&plan, &j.contracts().map_err(|e| e.key())?, &j.decisions().map_err(|e| e.key())?, capacity)
    }
    pub fn prepare_build(&mut self, j: &Journal, task: &str, budget: super::disk::DiskBudget, bytes: u64) -> Result<String, &'static str> {
        self.prepare_build_with(j, task, budget, bytes, super::disk::available_bytes)
    }
    /// Sample every writable volume, including separate git common directories. No build writes precede the reservation.
    pub fn prepare_build_with(
        &mut self,
        j: &Journal,
        task: &str,
        budget: super::disk::DiskBudget,
        bytes: u64,
        mut sample: impl FnMut(&std::path::Path) -> Option<u64>,
    ) -> Result<String, &'static str> {
        let mut readings = vec![sample(j.private()), sample(j.records())];
        for lease in j.runtime().map_err(|e| e.key())?.leases {
            readings.push(sample(std::path::Path::new(&lease.repo)));
        }
        let free = readings.into_iter().try_fold(u64::MAX, |free, reading| reading.map(|n| free.min(n)));
        self.reserve_build(j, task, budget, bytes, free)
    }
    /// A missing native reading closes admission; fixtures supply recorded readings through this pure boundary.
    pub fn prepare_build_sample(
        &mut self,
        j: &Journal,
        budget: super::disk::DiskBudget,
        bytes: u64,
        free: Option<u64>,
    ) -> Result<String, &'static str> {
        self.reserve_build(j, "fixture", budget, bytes, free)
    }
    fn reserve_build(
        &mut self,
        j: &Journal,
        task: &str,
        budget: super::disk::DiskBudget,
        bytes: u64,
        free: Option<u64>,
    ) -> Result<String, &'static str> {
        if self.halted {
            return Err("error.store.write");
        }
        if matches!(self.mission.state, MissionState::Paused | MissionState::Done | MissionState::Blocked) {
            return Err("reason.budget.paused");
        }
        let mut resources = super::disk::Resources::load(j)?;
        let reserved = resources.outstanding_bytes(j)?;
        if let Err(key) = budget.admit(free.map(|n| n.saturating_sub(reserved)), resources.used_bytes(j)?, bytes) {
            self.mission.state = MissionState::Paused;
            self.mission.reason = Some(key.into());
            self.quota_wake = None;
            self.promotion_pending = false;
            self.save(j)?;
            return Err(key);
        }
        resources.reserve_build(j, task, bytes)
    }
    /// A worker's DoneReady is only a result proposal. The exact engine receipt decides verified status.
    pub fn completion(&mut self, j: &Journal, task: &str, tree: &str, complete: &super::wire::Complete) -> Result<bool, &'static str> {
        let w = self.workers.get(task).ok_or("error.wire.stale")?;
        let id = complete.receipt.as_deref().ok_or("reason.receipt.missing")?;
        let receipt = j.check(id).map_err(|e| e.key())?.ok_or("reason.receipt.missing")?;
        if receipt.candidate != tree
            || receipt.extra.get("input").and_then(serde_json::Value::as_str) != Some(w.digest().as_str())
            || receipt.checks.is_empty()
            || !w.brief.checks.iter().all(|expected| {
                receipt.checks.iter().any(|c| c.id == expected.id && c.result == super::wire::CheckResult::Pass && c.validate().is_ok())
            })
        {
            return Err("reason.receipt.stale");
        }
        Ok(receipt.checks.iter().any(|c| matches!(c.kind, super::wire::CheckKind::Program | super::wire::CheckKind::ProgramJudge)))
    }
    /// Only accepted engine receipts land source. T5B supplies the complete-bar gate at this seam.
    #[allow(clippy::too_many_arguments)]
    pub fn land<E: super::git::Executor>(
        &mut self,
        broker: &mut super::git::Broker<E>,
        task: &str,
        lease: &super::Lease,
        mut grant: impl FnMut() -> Result<Option<super::Grant>, super::git::GitError>,
        message: &str,
        termination: super::lease::Termination,
        bar: impl FnOnce(&super::queue::Candidate) -> Result<(), &'static str>,
    ) -> Result<String, &'static str> {
        if self.halted || self.mission.state != MissionState::Running {
            return Err("reason.budget.paused");
        }
        let candidate = self.queue.get(task).ok_or("reason.queue.serial")?.clone();
        if lease.task != task || lease.base != candidate.base || lease.mission != self.mission.id {
            return Err("error.wire.stale");
        }
        let (commit_id, forward_id) = self.queue.landing_ids(task)?;
        self.save(broker.journal())?;
        let done = |id: &str| broker.journal().effect_receipt(id).map_err(|e| e.key());
        let committed = done(&commit_id)?;
        let had_commit = committed.is_some();
        let forwarded = done(&forward_id)?;
        if forwarded.is_some() && !had_commit {
            return Err("reason.project.reconcile");
        }
        let head = if let Some(receipt) = committed {
            if receipt.outcome != super::Outcome::Done
                || receipt.evidence.get("tree") != Some(&candidate.tree)
                || receipt.evidence.get("parent") != Some(&candidate.base)
            {
                return Err("reason.project.reconcile");
            }
            receipt.evidence.get("head").filter(|s| super::git::valid_oid(s)).cloned().ok_or("reason.project.reconcile")?
        } else {
            candidate.base.clone()
        };
        let current = grant().map_err(|e| e.key())?.ok_or("reason.grant.none")?;
        if current.revision != candidate.grant_revision {
            return Err("reason.queue.stale");
        }
        let expected = if forwarded.is_some() { &head } else { &candidate.base };
        if broker.head().map_err(|e| e.key())? != *expected {
            return Err("reason.queue.stale");
        }
        self.queue.accepted(task, &candidate.base, current.revision)?;
        broker.authority(&mut grant, super::Effect::FastForward).map_err(|e| e.key())?;
        bar(&candidate)?;
        let head = if !had_commit {
            broker.commit_proven(&mut grant, lease, &commit_id, message, &candidate.paths, &candidate.tree).map_err(|e| e.key())?
        } else {
            head
        };
        if let Some(receipt) = forwarded {
            if receipt.outcome != super::Outcome::Done
                || receipt.evidence.get("new") != Some(&head)
                || receipt.evidence.get("old") != Some(&candidate.base)
            {
                return Err("reason.project.reconcile");
            }
        } else {
            broker.fast_forward(&mut grant, &forward_id, &candidate.base, &head).map_err(|e| e.key())?;
        }
        // Keep receipt identities on the task before retiring any operational files.
        let mut plan = broker.journal().plan().map_err(|e| e.key())?.ok_or("reason.project.compile")?;
        let row = plan.tasks.iter_mut().find(|t| t.id == task).ok_or("error.wire.stale")?;
        row.state = TaskState::Done;
        row.reason = None;
        if !self.landings.iter().any(|l| l.candidate.id == task) {
            self.landings.push(Landing {
                candidate: candidate.clone(),
                commit_intent: commit_id.clone(),
                forward_intent: forward_id.clone(),
                at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            });
        }
        row.extra.insert("commitReceipt".into(), commit_id.into());
        row.extra.insert("landingReceipt".into(), forward_id.into());
        broker.journal().save_plan(&plan).map_err(|e| e.key())?;
        self.queue_retirement(broker.journal(), lease, termination, false, current.self_update)?;
        self.recover_retirements(broker, &mut grant)?;
        Ok(head)
    }

    fn write_report(&self, j: &Journal) -> Result<(), super::store::JournalError> {
        use super::report::{
            CheckReport, CommitReceiptReport, MissionReport, ReportTimings, RoleTokens, TaskReport, TreeReceiptReport, UnknownOr,
        };
        let Some(latest) = self.landings.last() else { return Ok(()) };
        let mut report = MissionReport::new(&self.mission.id, &self.mission.goal, &format!("{:?}", self.mission.state).to_lowercase());
        for role in ["lead", "writer", "reviewer", "repair", "canary"] {
            report.tokens_per_role.insert(role.into(), self.role_tokens.get(role).cloned().unwrap_or_else(RoleTokens::default));
        }
        let checks = j.list_checks()?;
        for landing in &self.landings {
            let candidate = &landing.candidate;
            let mut task_checks = vec![];
            for receipt in checks.iter().filter(|r| {
                r.task == candidate.id
                    && r.candidate == candidate.tree
                    && r.extra.get("input").and_then(serde_json::Value::as_str) == Some(&candidate.input)
            }) {
                for check in &receipt.checks {
                    check.validate().map_err(|_| super::store::JournalError::Mismatch)?;
                    task_checks.push(CheckReport {
                        id: check.id.clone(),
                        result: format!("{:?}", check.result).to_lowercase(),
                        reason: check.reason.clone(),
                        evidence: check.evidence.iter().map(|e| e.path.clone()).collect(),
                    });
                }
            }
            for definition in candidate.definitions() {
                for id in &definition.checks {
                    if !task_checks.iter().any(|c| &c.id == id) {
                        task_checks.push(CheckReport { id: id.clone(), result: "unknown".into(), reason: None, evidence: vec![] });
                    }
                }
            }
            let commit = j.effect_receipt(&landing.commit_intent)?;
            let forward = j.effect_receipt(&landing.forward_intent)?;
            let evidence = |receipt: &Option<super::Receipt>, key: &str| {
                receipt.as_ref().and_then(|r| r.evidence.get(key)).cloned().map_or_else(UnknownOr::default, UnknownOr::Known)
            };
            let commit_receipt = CommitReceiptReport {
                intent: landing.commit_intent.clone(),
                head: evidence(&commit, "head"),
                tree: evidence(&commit, "tree"),
                parent: evidence(&commit, "parent"),
                outcome: commit.as_ref().map(|r| format!("{:?}", r.outcome).to_lowercase()),
            };
            let tree_receipt = TreeReceiptReport {
                intent: landing.forward_intent.clone(),
                old_head: evidence(&forward, "old"),
                new_head: evidence(&forward, "new"),
                tree: evidence(&commit, "tree"),
                outcome: forward.as_ref().map(|r| format!("{:?}", r.outcome).to_lowercase()),
            };
            report.checks.extend(task_checks.clone());
            if candidate.id == latest.candidate.id {
                report.commit_receipt = Some(commit_receipt.clone());
                report.tree_receipt = Some(tree_receipt.clone());
            }
            report
                .tasks
                .insert(candidate.id.clone(), TaskReport { checks: task_checks, commit_receipt, tree_receipt, landed_at: landing.at });
        }
        report.timings = ReportTimings {
            started_at: if self.mission.created_at > 0 { UnknownOr::Known(self.mission.created_at) } else { UnknownOr::default() },
            landed_at: UnknownOr::Known(latest.at),
            duration_seconds: if self.mission.created_at > 0 && latest.at >= self.mission.created_at {
                UnknownOr::Known(latest.at - self.mission.created_at)
            } else {
                UnknownOr::default()
            },
        };
        j.save_report(&report)
    }

    fn queue_retirement(
        &mut self,
        j: &Journal,
        lease: &super::Lease,
        proof: super::lease::Termination,
        abandoned: bool,
        self_update: bool,
    ) -> Result<(), &'static str> {
        if !proof.tree_dead || !proof.cost_settled || !j.pending().map_err(|e| e.key())?.is_empty() {
            self.save(j)?;
            return Err("reason.disk.needed");
        }
        if let Some(old) = self.retirements.get(&lease.task) {
            if old.lease != *lease || old.process != proof.process || old.abandoned != abandoned {
                return Err("reason.lease.live");
            }
        } else {
            self.retirements.insert(
                lease.task.clone(),
                Retirement {
                    lease: lease.clone(),
                    process: proof.process,
                    cleanup_intent: format!("cleanup-{}", uuid::Uuid::new_v4()),
                    abandoned,
                },
            );
        }
        if !abandoned {
            self.queue.landed(&lease.task)?;
            let plan = j.plan().map_err(|e| e.key())?.ok_or("reason.project.compile")?;
            if plan.tasks.iter().all(|t| t.state == TaskState::Done) {
                self.mission.state = MissionState::Done;
                self.quota_wake = None;
                self.promotion_pending = self_update;
            }
        }
        self.save(j)
    }

    /// The caller releases the last artifact consumer after failure/abandon/rollback; no completion is inferred.
    pub fn retire_task<E: super::git::Executor>(
        &mut self,
        broker: &mut super::git::Broker<E>,
        lease: &super::Lease,
        proof: super::lease::Termination,
        grant: impl FnMut() -> Result<Option<super::Grant>, super::git::GitError>,
    ) -> Result<(), &'static str> {
        self.queue_retirement(broker.journal(), lease, proof, true, false)?;
        self.recover_retirements(broker, grant)
    }

    /// Replays only local retirement checkpoints. Pending external effects still require T4 reconciliation.
    pub fn recover_retirements<E: super::git::Executor>(
        &mut self,
        broker: &mut super::git::Broker<E>,
        mut grant: impl FnMut() -> Result<Option<super::Grant>, super::git::GitError>,
    ) -> Result<(), &'static str> {
        for (task, retirement) in self.retirements.clone() {
            if retirement.abandoned
                && let Some(candidate) = self.queue.get(&task)
            {
                let id = format!("abandoned-{}", retirement.lease.generation);
                let name = format!("{id}.json");
                broker.journal().put(&broker.journal().private().join(&name), candidate).map_err(|e| e.key())?;
                let mut public = super::Artifact::new(
                    &id,
                    &task,
                    retirement.lease.attempt,
                    super::ArtifactKind::Findings,
                    &candidate.base,
                    &retirement.lease.contract_digest,
                );
                public.tree = Some(candidate.tree.clone());
                public.changed = candidate.paths.clone();
                public.extra.insert("input".into(), candidate.input.clone().into());
                public.extra.insert("conflicts".into(), serde_json::json!(candidate.conflicts));
                broker.journal().save_artifact(&public).map_err(|e| e.key())?;
                self.queue.abandon(&task)?;
                self.save(broker.journal())?;
            }
            let proof = super::lease::Termination { process: retirement.process, tree_dead: true, cost_settled: true };
            let runtime = broker.journal().runtime().map_err(|e| e.key())?;
            let held = runtime
                .leases
                .iter()
                .find(|l| l.generation == retirement.lease.generation && l.task == task)
                .ok_or("reason.lease.stale")?;
            match held.state {
                super::LeaseState::Held => broker.leases().release(&retirement.lease, proof).map_err(|e| e.key())?,
                super::LeaseState::Quarantined => broker.leases().archive(&retirement.lease, proof).map_err(|e| e.key())?,
                super::LeaseState::Released => {}
                super::LeaseState::Releasing => return Err("reason.lease.live"),
            }
            broker.retire_worktree(&mut grant, &retirement.lease, &retirement.cleanup_intent).map_err(|e| e.key())?;
            self.cleanup_task(broker.journal(), &task, proof)?;
            self.retirements.remove(&task);
            self.save(broker.journal())?;
        }
        Ok(())
    }
    pub fn cleanup_task(&mut self, j: &Journal, task: &str, proof: super::lease::Termination) -> Result<(), &'static str> {
        if !proof.tree_dead || !proof.cost_settled || !j.pending().map_err(|e| e.key())?.is_empty() {
            return Err("reason.disk.needed");
        }
        let mut resources = super::disk::Resources::load(j)?;
        let ids: Vec<_> = resources.entries().iter().filter(|r| r.task == task).map(|r| r.id.clone()).collect();
        for id in ids {
            let owner = resources.entries().iter().find(|r| r.id == id).ok_or("reason.disk.resource")?.process;
            if owner.is_some() && owner != proof.process {
                return Err("reason.disk.needed");
            }
            resources.unneeded(j, &id)?;
            resources.clean(
                j,
                &id,
                super::lease::Termination { process: owner, tree_dead: proof.tree_dead, cost_settled: proof.cost_settled },
            )?;
        }
        let runtime = j.runtime().map_err(|e| e.key())?;
        for lease in runtime.leases.iter().filter(|l| l.task == task && super::lease::retirement_known(l)) {
            let prefix = format!("patch-{}-", lease.generation);
            for entry in std::fs::read_dir(j.private()).map_err(|_| "error.store.read")? {
                let entry = entry.map_err(|_| "error.store.read")?;
                let name = entry.file_name();
                if name
                    .to_str()
                    .and_then(|s| s.strip_prefix(&prefix))
                    .and_then(|s| s.strip_suffix(".json"))
                    .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok())
                {
                    let path = super::lease::real_path(&entry.path()).map_err(|e| e.key())?;
                    std::fs::remove_file(path).map_err(|e| crate::store::Fault::of_write(&e).key())?;
                }
            }
        }
        Ok(())
    }
    pub fn proposal(
        &mut self,
        j: &Journal,
        task: &str,
        envelope: &super::wire::Envelope,
    ) -> Result<super::wire::ArtifactRef, &'static str> {
        let worker = self.workers.get(task).ok_or("error.wire.stale")?;
        if worker.brief.role != Role::TestProposal || !matches!(envelope.action, super::wire::Action::Patch(_)) {
            return Err("error.wire.role");
        }
        let name = format!("proposal-{}.json", uuid::Uuid::new_v4());
        let path = j.records().join("artifacts").join(&name);
        j.put(&path, envelope).map_err(|e| e.key())?;
        let bytes = std::fs::read(&path).map_err(|_| "error.store.read")?;
        Ok(super::wire::ArtifactRef { path: format!("artifacts/{name}"), sha256: super::kit::digest(&bytes) })
    }
}

/// G8's route is a report only. Scheduling never reads it, and fixture reports never claim live acceptance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteStep {
    pub task: String,
    pub role: Role,
    pub pin: super::worker::ModelPin,
    pub cost_micros: Option<u64>,
    pub receipts: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteV0 {
    pub schema: u32,
    pub shape: String,
    pub fixture: bool,
    pub steps: Vec<RouteStep>,
}
impl RouteV0 {
    pub fn record(&self, j: &Journal) -> Result<(), &'static str> {
        if self.schema != SCHEMA || self.shape != "faber.code-change" || self.steps.is_empty() || self.steps.len() > 200 {
            return Err("reason.route.evidence");
        }
        if !self.fixture
            && (j.mission().map_err(|e| e.key())?.is_none_or(|m| m.state != MissionState::Done)
                || ![Vendor::Claude, Vendor::Codex].iter().all(|v| self.steps.iter().any(|s| &s.pin.vendor == v)))
        {
            return Err("reason.route.evidence");
        }
        for step in &self.steps {
            step.pin.validate()?;
            if !super::valid_id(&step.task) || step.receipts.is_empty() || step.receipts.len() > 64 {
                return Err("reason.route.evidence");
            }
            for id in &step.receipts {
                let check = j.check(id).map_err(|e| e.key())?;
                let effect = j.effect_receipt(id).map_err(|e| e.key())?;
                if !check.is_some_and(|r| {
                    !r.checks.is_empty() && r.checks.iter().all(|c| c.result == super::wire::CheckResult::Pass && c.validate().is_ok())
                }) && !effect.is_some_and(|r| r.outcome == super::Outcome::Done)
                {
                    return Err("reason.route.evidence");
                }
            }
        }
        j.put(&j.records().join("route-v0.json"), self).map_err(|e| e.key())
    }
}

#[cfg(test)]
mod proof_log_tests {
    use super::*;
    use crate::project::proof::{Definition, ProofSet};

    /// The receipt's evidence is checked by the journal against the stored log: it must hash what was stored.
    #[tokio::test]
    async fn a_proof_receipt_hashes_the_log_that_is_stored() {
        let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        // The journal opens every folder without following links, and macOS keeps temp folders behind one (/var).
        let data_path = data.path().canonicalize().unwrap();
        let j = super::super::store::Journal::open(repo.path(), &data_path, "m1").unwrap();
        let mut runner = Runner::create(&j, crate::project::Mission::new("m1", "p1", "repo", "Fix", &"1".repeat(40), 3, 1)).unwrap();
        runner.mission.state = MissionState::Running;
        runner.save(&j).unwrap();
        let definition = Definition {
            id: "rust".into(),
            version: 1,
            program: "/native/cargo".into(),
            argv: vec!["t".into()],
            checks: vec!["C1".into()],
            toolchain: "rust-1".into(),
            dependencies: "deps-1".into(),
            environment: std::collections::BTreeMap::new(),
            heavy: true,
        };
        let mut proof = ProofSet::new("1".repeat(40), "input".into(), vec![definition], vec![]).unwrap();
        let spawn: crate::proc::Spawner = std::sync::Arc::new(|_, _, _| Err(std::io::Error::other("no process in a unit test")));
        let receipt = runner.run_proof(&j, "a", "rust", &mut proof, repo.path(), &spawn).await.unwrap();
        let art = &receipt.artifacts[0];
        let stored = std::fs::read_to_string(j.private().join(&art.path)).unwrap();
        assert!(stored.starts_with("exit "), "{stored}");
        assert!(super::super::store::verify_artifacts(j.private(), std::iter::once(art)).is_ok(), "the journal rejected the stored log");
    }

    /// The roles read the short report; the whole output stays in the private folder, both without a printed secret.
    #[tokio::test]
    async fn a_proof_keeps_its_whole_output_privately_and_gives_the_roles_its_short_report() {
        let (repo, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let data_path = data.path().canonicalize().unwrap();
        let j = super::super::store::Journal::open(repo.path(), &data_path, "m1").unwrap();
        let mut runner = Runner::create(&j, crate::project::Mission::new("m1", "p1", "repo", "Fix", &"1".repeat(40), 3, 1)).unwrap();
        runner.mission.state = MissionState::Running;
        runner.save(&j).unwrap();
        let definition = Definition {
            id: "types".into(),
            version: 1,
            program: "/native/tsc".into(),
            argv: vec![],
            checks: vec!["C1".into()],
            toolchain: "npm".into(),
            dependencies: "deps-1".into(),
            environment: std::collections::BTreeMap::new(),
            heavy: false,
        };
        let mut proof = ProofSet::new("1".repeat(40), "input".into(), vec![definition], vec![]).unwrap();
        let root = repo.path().to_string_lossy().into_owned();
        let token = format!("ghp_{}", "b".repeat(36));
        let printed = format!("> demo typecheck\n{root}/src/a.ts(4,3): error TS2322: Type 'number' is not assignable.\nprinted {token}\n");
        let spawn: crate::proc::Spawner = std::sync::Arc::new(move |_, _, _| {
            Ok(crate::proc::Proc {
                stdin: Box::new(tokio::io::sink()),
                stdout: Box::new(std::io::Cursor::new(printed.clone().into_bytes())),
                stderr: None,
                child: None,
                contained: None,
            })
        });
        let receipt = runner.run_proof(&j, "a", "types", &mut proof, repo.path(), &spawn).await.unwrap();
        let report = Runner::proof_report(&j, &receipt.id).unwrap();
        assert_eq!(
            report, "src/a.ts:4:3: error TS2322: Type 'number' is not assignable.\ntypes (npm) did not finish: 1 type error",
            "a fake process has no exit code, so it never passes"
        );
        let whole = std::fs::read_to_string(j.private().join(format!("proofs/{}.output", receipt.id))).unwrap();
        assert!(whole.contains("> demo typecheck") && whole.contains("[line withheld: it looks like a secret]"), "{whole}");
        assert!(!whole.contains(&token));
        assert_eq!(Runner::proof_report(&j, "../escape"), None);
    }
}
