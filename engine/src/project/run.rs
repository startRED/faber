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
pub struct Runner {
    schema: u32,
    pub mission: Mission,
    pub workers: BTreeMap<String, Worker>,
    pub queue: Queue,
    pub quota_wake: Option<u64>,
    pub promotion_pending: bool,
    halted: bool,
    #[serde(flatten)]
    extra: Extra,
}
impl Record for Runner {
    fn schema(&self) -> u32 {
        self.schema
    }
}
impl Runner {
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
        let mut runner = Self {
            schema: SCHEMA,
            mission,
            workers: BTreeMap::new(),
            queue: Queue::default(),
            quota_wake: None,
            promotion_pending: false,
            halted: false,
            extra: Extra::new(),
        };
        runner.save(j)?;
        Ok(runner)
    }
    pub fn load(j: &Journal) -> Result<Self, &'static str> {
        let runner: Self = j.get(&j.private().join("run.json")).map_err(|e| e.key())?.ok_or("error.store.corrupt")?;
        if runner.schema != SCHEMA
            || j.mission().map_err(|e| e.key())?.as_ref() != Some(&runner.mission)
            || runner.workers.values().any(|w| w.schema != SCHEMA)
        {
            return Err("error.store.corrupt");
        }
        Ok(runner)
    }
    pub fn save(&mut self, j: &Journal) -> Result<(), &'static str> {
        if self.halted {
            return Err("error.store.write");
        }
        let result = j.put(&j.private().join("run.json"), self).and_then(|()| j.save_mission(&self.mission));
        if let Err(e) = result {
            self.halted = true;
            return Err(e.key());
        }
        Ok(())
    }
    pub fn pause(&mut self, j: &Journal) -> Result<(), &'static str> {
        self.mission.state = MissionState::Paused;
        self.quota_wake = None;
        self.promotion_pending = false;
        self.save(j)
    }
    pub fn resume(&mut self, j: &Journal, revision: u64) -> Result<(), &'static str> {
        if self.halted || self.mission.state != MissionState::Paused || revision != self.mission.grant_revision {
            return Err("reason.grant.stale");
        }
        if !j.pending().map_err(|e| e.key())?.is_empty() {
            return Err("reason.project.reconcile");
        }
        let runtime = j.runtime().map_err(|e| e.key())?;
        if runtime.leases.iter().any(|l| l.process.is_some() && l.state != super::LeaseState::Released) {
            return Err("reason.lease.live");
        }
        // Reconciliation never adopts or kills a recorded PID. T2 owns handles; T3 owns unsettled reservations.
        for w in self.workers.values() {
            w.pin.validate()?;
        }
        self.mission.state = MissionState::Running;
        self.save(j)
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
        if request.mission != self.mission.id || request.model != worker.pin.model || request.effort != worker.pin.effort {
            return Err("error.project.invocation".into());
        }
        engine.mission_authority(&self.mission)?;
        engine.mission_budget(super::budget::Change::Admit(request.clone()))?;
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
    pub async fn stop(&mut self, j: &Journal, engine: &crate::Engine, sessions: &[crate::events::Session]) -> Result<(), String> {
        // Close memory admission before any fallible IO. Never free leases or reservations from a stop request.
        self.mission.state = MissionState::Paused;
        self.quota_wake = None;
        self.promotion_pending = false;
        let budget = engine.mission_budget(super::budget::Change::Pause { mission: self.mission.id.clone(), paused: true });
        let saved = self.save(j).map_err(str::to_string);
        for session in sessions {
            session.interrupt();
        }
        if !sessions.is_empty() {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
        for session in sessions {
            session.close();
        }
        budget?;
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
            self.halted = true;
            return Err(e.key());
        }
        self.mission.state = MissionState::Running;
        self.save(j)
    }
    pub fn ready(&mut self, j: &Journal, capacity: Capacity) -> Result<Ready, &'static str> {
        let plan = j.plan().map_err(|e| e.key())?.ok_or("reason.project.compile")?;
        schedule(&plan, &j.contracts().map_err(|e| e.key())?, &j.decisions().map_err(|e| e.key())?, capacity)
    }
    pub fn prepare_build(&mut self, j: &Journal, budget: super::disk::DiskBudget, bytes: u64) -> Result<(), &'static str> {
        self.prepare_build_sample(j, budget, bytes, super::disk::available_bytes(j.private()))
    }
    /// A missing native reading closes admission; fixtures supply recorded readings through this pure boundary.
    pub fn prepare_build_sample(
        &mut self,
        j: &Journal,
        budget: super::disk::DiskBudget,
        bytes: u64,
        free: Option<u64>,
    ) -> Result<(), &'static str> {
        if self.halted {
            return Err("error.store.write");
        }
        if matches!(self.mission.state, MissionState::Paused | MissionState::Done | MissionState::Blocked) {
            return Err("reason.budget.paused");
        }
        let resources = super::disk::Resources::load(j)?;
        if let Err(key) = budget.admit(free, resources.used_bytes(j)?, bytes) {
            self.mission.state = MissionState::Paused;
            self.mission.reason = Some(key.into());
            self.quota_wake = None;
            self.promotion_pending = false;
            self.save(j)?;
            return Err(key);
        }
        Ok(())
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
        self.queue.accepted(task, &candidate.base, candidate.grant_revision)?;
        bar(&candidate)?;
        if lease.task != task || lease.base != candidate.base || lease.mission != self.mission.id {
            return Err("error.wire.stale");
        }
        let (commit_id, forward_id) = self.queue.landing_ids(task)?;
        self.save(broker.journal())?;
        let done = |id: &str| broker.journal().effect_receipt(id).map_err(|e| e.key());
        let committed = done(&commit_id)?;
        let forwarded = done(&forward_id)?;
        let head = if let Some(receipt) = committed {
            if receipt.outcome != super::Outcome::Done
                || receipt.evidence.get("tree") != Some(&candidate.tree)
                || receipt.evidence.get("parent") != Some(&candidate.base)
            {
                return Err("reason.project.reconcile");
            }
            receipt.evidence.get("head").filter(|s| super::git::valid_oid(s)).cloned().ok_or("reason.project.reconcile")?
        } else {
            let current = grant().map_err(|e| e.key())?.ok_or("reason.grant.none")?;
            self.queue.accepted(task, &broker.head().map_err(|e| e.key())?, current.revision)?;
            broker.commit_proven(&mut grant, lease, &commit_id, message, &candidate.paths, &candidate.tree).map_err(|e| e.key())?
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
        row.extra.insert("commitReceipt".into(), commit_id.into());
        row.extra.insert("landingReceipt".into(), forward_id.into());
        broker.journal().save_plan(&plan).map_err(|e| e.key())?;
        self.queue.landed(task)?;
        if plan.tasks.iter().all(|t| t.state == TaskState::Done) {
            self.mission.state = MissionState::Done;
            self.quota_wake = None;
            self.promotion_pending = false;
        }
        self.save(broker.journal())?;
        broker
            .leases()
            .release(
                lease,
                super::lease::Termination {
                    process: termination.process,
                    tree_dead: termination.tree_dead,
                    cost_settled: termination.cost_settled,
                },
            )
            .map_err(|e| e.key())?;
        broker.retire_worktree(&mut grant, lease, &format!("cleanup-{}", uuid::Uuid::new_v4())).map_err(|e| e.key())?;
        self.cleanup_task(broker.journal(), task, termination)?;
        Ok(head)
    }
    pub fn cleanup_task(&mut self, j: &Journal, task: &str, proof: super::lease::Termination) -> Result<(), &'static str> {
        if !proof.tree_dead || !proof.cost_settled {
            return Err("reason.disk.needed");
        }
        let mut resources = super::disk::Resources::load(j)?;
        let ids: Vec<_> = resources.entries().iter().filter(|r| r.task == task).map(|r| r.id.clone()).collect();
        for id in ids {
            resources.unneeded(j, &id)?;
            resources.clean(
                j,
                &id,
                super::lease::Termination { process: proof.process, tree_dead: proof.tree_dead, cost_settled: proof.cost_settled },
            )?;
        }
        let runtime = j.runtime().map_err(|e| e.key())?;
        for lease in runtime.leases.iter().filter(|l| l.task == task && l.state == super::LeaseState::Released) {
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
