//! Self-development missions (R96): what a project may do, the records a mission keeps, and its task graph.
//! The design is `docs/superpowers/specs/2026-10-12-self-development-mvp-design.md`; the published contracts are in
//! `docs/design.md` (Projects: self-development missions).
//!
//! Every record carries a schema version and keeps the fields a newer Faber wrote in `extra`, so a last-good build can
//! read and rewrite them during a rollback without dropping anything. A record whose schema is newer than this build's
//! is refused rather than guessed at ([`store::Journal`]).

mod files;
pub mod git;
pub mod kit;
pub mod lease;
pub mod store;
pub mod update;
pub mod wire;

use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// The schema version this build writes, and the newest it reads.
pub const SCHEMA: u32 = 1;
/// The most tasks one plan may hold.
pub const MAX_TASKS: usize = 200;

/// Fields a newer Faber wrote that this build does not know, kept as they were.
pub type Extra = BTreeMap<String, Value>;

/// Whether `id` can name a record file: 1 to 64 ASCII letters, digits, `-` or `_`.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A path inside the repository, with forward slashes and no `.` parts; "" is the whole repository. None for anything
/// that could point outside it: an empty or absolute path, a drive or stream (`:`), or a `..` part. Symlinks, junctions
/// and case on disk are the git broker's to resolve; this is only the text.
pub fn clean_path(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.contains([':', '\0']) {
        return None;
    }
    let raw = raw.replace('\\', "/");
    if raw.starts_with('/') {
        return None;
    }
    let mut parts = vec![];
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// Whether two clean paths name the same area: equal, one inside the other, or the whole repository. Case is ignored,
/// because Windows and macOS ignore it.
pub fn overlaps(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    a.is_empty() || b.is_empty() || a == b || b.starts_with(&format!("{a}/")) || a.starts_with(&format!("{b}/"))
}

// Enrollment and authority

/// Which repository a project folder belongs to. The common git directory names it, so linked worktrees of one
/// repository share one identity; the folder the person opened is only shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoIdentity {
    pub schema: u32,
    /// The canonical path of the common git directory, as the git broker resolved it.
    pub common_dir: String,
    /// The working folder the person opened.
    pub root: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl RepoIdentity {
    pub fn new(common_dir: &str, root: &str) -> Self {
        RepoIdentity { schema: SCHEMA, common_dir: common_dir.into(), root: root.into(), extra: Extra::new() }
    }
}

/// The git authority the person grants once at mission start. Livre is not offered, so it does not exist here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitMode {
    /// Separate worktrees, local commits and fast-forwards of the mission's own `faber/...` branch. Selected by default,
    /// never granted by default.
    #[default]
    Seguro,
    /// Seguro plus a non-forced push of that exact branch and one draft pull request.
    Equilibrado,
}

/// The commit author, read from git's ordinary configuration. Missing means commits wait; Faber never invents one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub email: String,
}

/// What the person granted, bound to one repository and one revision. No grant means no effect: planning stays read-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub schema: u32,
    /// The common git directory of the repository it was granted for.
    pub repo: String,
    pub mode: GitMode,
    /// Grows with every change the person makes; work admitted under another revision takes no further effect.
    pub revision: u64,
    /// Unix seconds.
    pub granted_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
    /// Equilibrado only: the remote shown at the grant, and the account `gh` reported for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_account: Option<String>,
    /// The separate trust that lets an accepted change to Faber promote Faber's next local build.
    #[serde(default)]
    pub self_update: bool,
    /// Unix seconds. A revoked grant allows nothing, from the next effect on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// An external effect a mission can take. Dispatching read-only work is not one: the spending gate decides that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    Worktree,
    Commit,
    FastForward,
    Push,
    DraftPr,
    SelfUpdate,
}

impl Effect {
    pub const ALL: [Effect; 6] = [Effect::Worktree, Effect::Commit, Effect::FastForward, Effect::Push, Effect::DraftPr, Effect::SelfUpdate];
}

impl Grant {
    pub fn new(repo: &str, mode: GitMode, revision: u64, granted_at: u64) -> Self {
        Grant {
            schema: SCHEMA,
            repo: repo.into(),
            mode,
            revision,
            granted_at,
            author: None,
            remote: None,
            remote_account: None,
            self_update: false,
            revoked_at: None,
            extra: Extra::new(),
        }
    }

    /// Whether `effect` is allowed on `repo` for work admitted under `revision`; a `reason.grant.*` key says why not.
    /// Checked before every effect, so a revocation or a new revision stops the next one.
    pub fn allows(&self, repo: &RepoIdentity, revision: u64, effect: Effect) -> Result<(), &'static str> {
        if self.schema > SCHEMA {
            return Err("reason.grant.newer");
        }
        if self.revoked_at.is_some() {
            return Err("reason.grant.revoked");
        }
        if self.repo != repo.common_dir {
            return Err("reason.grant.otherRepo");
        }
        if self.revision != revision {
            return Err("reason.grant.stale");
        }
        let remote = matches!(effect, Effect::Push | Effect::DraftPr);
        match effect {
            Effect::Commit if self.author.is_none() => Err("reason.grant.noAuthor"),
            _ if remote && self.mode != GitMode::Equilibrado => Err("reason.grant.mode"),
            _ if remote && self.remote.is_none() => Err("reason.grant.noRemote"),
            Effect::SelfUpdate if !self.self_update => Err("reason.grant.selfUpdate"),
            _ => Ok(()),
        }
    }
}

// The mission

/// Normal waits for the person's answers; autonomous applies only harmless recorded defaults. Separate from the git mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunMode {
    #[default]
    Normal,
    Autonomous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MissionState {
    #[default]
    Planning,
    Running,
    /// Stopped by the person: admission closed, work checkpointed, nothing marked done.
    Paused,
    Done,
    Blocked,
}

/// `mission.json`: one goal on one repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mission {
    pub schema: u32,
    pub id: String,
    pub project_id: String,
    /// The common git directory of the repository.
    pub repo: String,
    pub goal: String,
    /// The committed revision the mission started from.
    pub base: String,
    /// `faber/<id>`, the only branch the mission moves.
    pub branch: String,
    /// The grant revision the mission was started under.
    pub grant_revision: u64,
    #[serde(default)]
    pub mode: RunMode,
    #[serde(default)]
    pub state: MissionState,
    /// A `reason.*` key while blocked or paused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Mission {
    pub fn new(id: &str, project_id: &str, repo: &str, goal: &str, base: &str, grant_revision: u64, at: u64) -> Self {
        Mission {
            schema: SCHEMA,
            id: id.into(),
            project_id: project_id.into(),
            repo: repo.into(),
            goal: goal.into(),
            base: base.into(),
            branch: format!("faber/{id}"),
            grant_revision,
            mode: RunMode::Normal,
            state: MissionState::Planning,
            reason: None,
            created_at: at,
            updated_at: at,
            extra: Extra::new(),
        }
    }
}

// Contracts

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Boundary {
    pub paths: Vec<String>,
    pub callers: Vec<String>,
    /// The public shape, in words or a type signature.
    pub shape: String,
    pub states: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Examples {
    pub success: String,
    pub failure: String,
    /// A saved-data example that must stay readable, when the contract touches saved data.
    pub saved: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContractProof {
    /// Named proof ids, never command text.
    pub commands: Vec<String>,
    pub expected: String,
    pub acceptance: String,
}

/// A contract as a task was built against it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ContractPin {
    pub id: String,
    pub digest: String,
}

/// `contracts/<id>.json`: the short fixed agreement tasks build against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub schema: u32,
    pub id: String,
    /// The task (or `lead`) that owns changes to it.
    pub owner: String,
    pub version: u32,
    /// SHA-256 of everything else in the record, with object keys sorted ([`Contract::seal`]).
    pub digest: String,
    pub base: String,
    #[serde(default)]
    pub boundary: Boundary,
    #[serde(default)]
    pub examples: Examples,
    #[serde(default)]
    pub proof: ContractProof,
    /// Accepted task ids and decision ids it depends on, and the contracts it takes as fixed input.
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<ContractPin>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Contract {
    pub fn new(id: &str, owner: &str, base: &str) -> Self {
        Contract {
            schema: SCHEMA,
            id: id.into(),
            owner: owner.into(),
            version: 1,
            digest: String::new(),
            base: base.into(),
            boundary: Boundary::default(),
            examples: Examples::default(),
            proof: ContractProof::default(),
            requires: vec![],
            decisions: vec![],
            inputs: vec![],
            extra: Extra::new(),
        }
    }

    /// The contract with its digest set over its current content.
    pub fn seal(mut self) -> Self {
        self.digest = self.content_digest();
        self
    }

    /// Whether the digest still matches the content.
    pub fn verify(&self) -> bool {
        !self.digest.is_empty() && self.digest == self.content_digest()
    }

    pub fn pin(&self) -> ContractPin {
        ContractPin { id: self.id.clone(), digest: self.digest.clone() }
    }

    fn content_digest(&self) -> String {
        let mut value = serde_json::to_value(self).expect("plain data always serializes");
        if let Some(fields) = value.as_object_mut() {
            fields.remove("digest");
        }
        let mut text = String::new();
        canonical(&value, &mut text);
        Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// JSON with object keys sorted, so the same content always has the same text whatever order it was read in.
fn canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(fields) => {
            let mut keys: Vec<&String> = fields.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                canonical(&fields[key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

// The graph

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    Research,
    /// Proposes a test file or patch as an artifact outside the source; only the writer applies it.
    TestProposal,
    Writer,
    Review,
    Integration,
}

impl TaskKind {
    /// Whether tasks of this kind may declare source writes.
    pub fn writes(self) -> bool {
        matches!(self, TaskKind::Writer | TaskKind::Integration)
    }
}

/// The board's five states. A Ready task waits only for a resource brake; a Blocked one names its reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Ready,
    Running,
    Review,
    Done,
    #[default]
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub kind: TaskKind,
    /// Tasks whose accepted artifacts this one needs.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Decisions that must be answered first.
    #[serde(default)]
    pub decisions: Vec<String>,
    /// The contracts it is built against, at the digest it was built against.
    #[serde(default)]
    pub contracts: Vec<ContractPin>,
    #[serde(default)]
    pub deliverables: Vec<String>,
    /// The source area a writer leases, as clean paths.
    #[serde(default)]
    pub writes: Vec<String>,
    /// Named proof ids.
    #[serde(default)]
    pub proofs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<wire::Labels>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit: Option<wire::KitPin>,
    #[serde(default)]
    pub state: TaskState,
    /// A `reason.*` key while blocked or waiting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Attempts so far; a retry never resets the caps they share.
    #[serde(default)]
    pub attempts: u32,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Task {
    pub fn new(id: &str, title: &str, kind: TaskKind) -> Self {
        Task {
            id: id.into(),
            title: title.into(),
            kind,
            requires: vec![],
            decisions: vec![],
            contracts: vec![],
            deliverables: vec![],
            writes: vec![],
            proofs: vec![],
            labels: None,
            checks: vec![],
            paths: vec![],
            kit: None,
            state: TaskState::default(),
            reason: None,
            attempts: 0,
            extra: Extra::new(),
        }
    }
}

/// Why a plan was refused, as an `error.plan.*` key with what it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    TooBig,
    BadId(String),
    DuplicateTask(String),
    MissingTask {
        task: String,
        missing: String,
    },
    /// The tasks that cannot be ordered, in plan order.
    Cycle(Vec<String>),
    DuplicateDeliverable {
        deliverable: String,
        first: String,
        second: String,
    },
    BadPath {
        task: String,
        path: String,
    },
    ReadOnlyWrites(String),
    /// Two writers that nothing orders would write the same area.
    Overlap {
        first: String,
        second: String,
    },
}

impl GraphError {
    pub fn key(&self) -> &'static str {
        match self {
            GraphError::TooBig => "error.plan.tooBig",
            GraphError::BadId(_) => "error.plan.badId",
            GraphError::DuplicateTask(_) => "error.plan.duplicateTask",
            GraphError::MissingTask { .. } => "error.plan.missingTask",
            GraphError::Cycle(_) => "error.plan.cycle",
            GraphError::DuplicateDeliverable { .. } => "error.plan.duplicateDeliverable",
            GraphError::BadPath { .. } => "error.plan.badPath",
            GraphError::ReadOnlyWrites(_) => "error.plan.readOnlyWrites",
            GraphError::Overlap { .. } => "error.plan.overlap",
        }
    }
}

/// `plan.json`: the mission's task graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub schema: u32,
    pub mission: String,
    /// Grows with every amendment.
    pub revision: u64,
    pub tasks: Vec<Task>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<wire::DeliverableSheet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bar: Vec<wire::BarCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compile_receipt: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Plan {
    pub fn new(mission: &str, revision: u64, tasks: Vec<Task>) -> Self {
        Plan {
            schema: SCHEMA,
            mission: mission.into(),
            revision,
            tasks,
            sheet: None,
            bar: vec![],
            compile_receipt: None,
            extra: Extra::new(),
        }
    }

    /// Refuses a graph with too many tasks, bad or repeated ids, missing dependencies, cycles, a deliverable claimed
    /// twice, bad write paths, writes on a read-only task, or unordered writers whose areas overlap.
    pub fn validate(&self) -> Result<(), GraphError> {
        if self.tasks.len() > MAX_TASKS {
            return Err(GraphError::TooBig);
        }
        let mut ids = BTreeSet::new();
        for t in &self.tasks {
            if !valid_id(&t.id) {
                return Err(GraphError::BadId(t.id.clone()));
            }
            if !ids.insert(t.id.as_str()) {
                return Err(GraphError::DuplicateTask(t.id.clone()));
            }
        }
        for t in &self.tasks {
            if let Some(missing) = t.requires.iter().find(|r| !ids.contains(r.as_str())) {
                return Err(GraphError::MissingTask { task: t.id.clone(), missing: missing.clone() });
            }
        }
        let ancestors = self.ancestors()?;
        let mut owners: BTreeMap<&str, &str> = BTreeMap::new();
        for t in &self.tasks {
            for d in t.deliverables.iter().map(|d| d.trim()) {
                if let Some(first) = owners.get(d)
                    && *first != t.id
                {
                    return Err(GraphError::DuplicateDeliverable { deliverable: d.into(), first: first.to_string(), second: t.id.clone() });
                }
                owners.insert(d, t.id.as_str());
            }
        }
        let mut areas: Vec<(&Task, Vec<String>)> = vec![];
        for t in &self.tasks {
            let mut clean = vec![];
            for w in &t.writes {
                clean.push(lease::normalized(w).map_err(|_| GraphError::BadPath { task: t.id.clone(), path: w.clone() })?);
            }
            if !clean.is_empty() && !t.kind.writes() {
                return Err(GraphError::ReadOnlyWrites(t.id.clone()));
            }
            areas.push((t, clean));
        }
        for (i, (a, a_paths)) in areas.iter().enumerate() {
            for (b, b_paths) in &areas[i + 1..] {
                let ordered = ancestors[&a.id].contains(&b.id) || ancestors[&b.id].contains(&a.id);
                if !ordered && a_paths.iter().any(|x| b_paths.iter().any(|y| overlaps(x, y))) {
                    return Err(GraphError::Overlap { first: a.id.clone(), second: b.id.clone() });
                }
            }
        }
        Ok(())
    }

    /// Every task's transitive requirements, or the tasks that cannot be ordered.
    fn ancestors(&self) -> Result<BTreeMap<String, BTreeSet<String>>, GraphError> {
        let mut done: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        while done.len() < self.tasks.len() {
            let before = done.len();
            for t in &self.tasks {
                if done.contains_key(&t.id) || !t.requires.iter().all(|r| done.contains_key(r)) {
                    continue;
                }
                let mut all = BTreeSet::new();
                for r in &t.requires {
                    all.insert(r.clone());
                    all.extend(done[r].iter().cloned());
                }
                done.insert(t.id.clone(), all);
            }
            if done.len() == before {
                let stuck = self.tasks.iter().filter(|t| !done.contains_key(&t.id)).map(|t| t.id.clone()).collect();
                return Err(GraphError::Cycle(stuck));
            }
        }
        Ok(done)
    }

    /// The tasks that depend, directly or not, on any of `roots`.
    pub fn descendants(&self, roots: &BTreeSet<String>) -> BTreeSet<String> {
        let mut found: BTreeSet<String> = BTreeSet::new();
        loop {
            let before = found.len();
            for t in &self.tasks {
                if t.requires.iter().any(|r| roots.contains(r) || found.contains(r)) {
                    found.insert(t.id.clone());
                }
            }
            if found.len() == before {
                return found;
            }
        }
    }

    /// A contract changed to `digest`: the tasks built against an older digest and everything that depends on them go
    /// back to Blocked with `reason.contract.revised`, and are returned in plan order. Everything else is untouched.
    pub fn revise(&mut self, contract: &str, digest: &str) -> Vec<String> {
        let direct: BTreeSet<String> = self
            .tasks
            .iter()
            .filter(|t| t.contracts.iter().any(|p| p.id == contract && p.digest != digest))
            .map(|t| t.id.clone())
            .collect();
        let mut hit = self.descendants(&direct);
        hit.extend(direct);
        let mut out = vec![];
        for t in self.tasks.iter_mut().filter(|t| hit.contains(&t.id)) {
            t.state = TaskState::Blocked;
            t.reason = Some("reason.contract.revised".into());
            out.push(t.id.clone());
        }
        out
    }
}

// Decisions

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DecisionKind {
    /// The AI may decide and log it.
    Technical,
    /// The rest belong to the person.
    Direction,
    Taste,
    Money,
    Legal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DecidedBy {
    Person,
    Ai,
}

/// `decisions/<id>.json`: a question with its author, default and answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub schema: u32,
    pub id: String,
    /// The task id, or `lead`, that asked.
    pub asked_by: String,
    pub question: String,
    pub kind: DecisionKind,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_by: Option<DecidedBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_at: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Decision {
    pub fn new(id: &str, asked_by: &str, question: &str, kind: DecisionKind) -> Self {
        Decision {
            schema: SCHEMA,
            id: id.into(),
            asked_by: asked_by.into(),
            question: question.into(),
            kind,
            options: vec![],
            default: None,
            answer: None,
            answered_by: None,
            answered_at: None,
            extra: Extra::new(),
        }
    }

    /// Records an answer once. The AI, autonomous defaults included, answers only technical choices; direction, taste,
    /// money and legal choices wait for the person.
    pub fn answer(&mut self, by: DecidedBy, answer: &str, at: u64) -> Result<(), &'static str> {
        if self.answer.is_some() {
            return Err("error.decision.answered");
        }
        if by == DecidedBy::Ai && self.kind != DecisionKind::Technical {
            return Err("error.decision.person");
        }
        if !self.options.is_empty() && !self.options.iter().any(|o| o == answer) {
            return Err("error.decision.option");
        }
        self.answer = Some(answer.into());
        self.answered_by = Some(by);
        self.answered_at = Some(at);
        Ok(())
    }
}

// Briefs, leases and artifacts

/// The session roles. A role name is not a router: Auto still picks the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Lead,
    Research,
    TestProposal,
    Writer,
    Reviewer,
}

/// `briefs/<task>.json`: everything a fresh worker session starts from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub schema: u32,
    pub task: String,
    pub attempt: u32,
    pub role: Role,
    pub goal: String,
    #[serde(default)]
    pub non_goals: Vec<String>,
    /// Clean paths it may read, and those it may write under its lease.
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
    pub base: String,
    pub contract: ContractPin,
    /// Source paths worth reading first.
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub proofs: Vec<String>,
    /// The cost grant in millionths of an API-equivalent dollar, once the spending gate set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_micros: Option<u64>,
    /// The lease generation a writer holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<u64>,
    #[serde(default)]
    pub stop: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<wire::Verbatim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<wire::Verbatim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<wire::Verbatim>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit: Option<wire::KitPin>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Brief {
    pub fn new(task: &str, attempt: u32, role: Role, goal: &str, base: &str, contract: ContractPin) -> Self {
        Brief {
            schema: SCHEMA,
            task: task.into(),
            attempt,
            role,
            goal: goal.into(),
            non_goals: vec![],
            scope: vec![],
            writes: vec![],
            base: base.into(),
            contract,
            sources: vec![],
            proofs: vec![],
            grant_micros: None,
            lease: None,
            stop: String::new(),
            requires: vec![],
            checks: vec![],
            quotes: vec![],
            kit: None,
            extra: Extra::new(),
        }
    }
}

/// A process as the lease knows it. The start time tells a reused PID from the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessId {
    pub pid: u32,
    pub started_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LeaseState {
    #[default]
    Held,
    /// Released only once the process tree is proven gone and its cost settled.
    Releasing,
    Released,
    /// Manual edits or an unknown termination: kept for a person, never reassigned.
    Quarantined,
}

/// One writer attempt's exclusive area. Kept in the private runtime sidecar: it names processes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub schema: u32,
    pub repo: String,
    pub mission: String,
    pub task: String,
    pub attempt: u32,
    pub paths: Vec<String>,
    pub base: String,
    pub contract_digest: String,
    /// Grows with every new holder; an envelope with another generation is stale.
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessId>,
    #[serde(default)]
    pub state: LeaseState,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Lease {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: &str,
        mission: &str,
        task: &str,
        attempt: u32,
        paths: Vec<String>,
        base: &str,
        contract_digest: &str,
        generation: u64,
    ) -> Self {
        Lease {
            schema: SCHEMA,
            repo: repo.into(),
            mission: mission.into(),
            task: task.into(),
            attempt,
            paths,
            base: base.into(),
            contract_digest: contract_digest.into(),
            generation,
            process: None,
            state: LeaseState::Held,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ArtifactKind {
    Patch,
    File,
    Findings,
    TestProposal,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CostCompleteness {
    Complete,
    Partial,
    #[default]
    Unknown,
}

/// A command the engine ran, with its receipt. A worker saying tests passed is not one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRun {
    pub proof: String,
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

/// `artifacts/<id>.json`: what an attempt handed back, with references to the full local evidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub schema: u32,
    pub id: String,
    pub task: String,
    pub attempt: u32,
    pub kind: ArtifactKind,
    pub base: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<String>,
    pub contract_digest: String,
    /// Files of the artifact, relative to its folder.
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub changed: Vec<String>,
    #[serde(default)]
    pub commands: Vec<CommandRun>,
    #[serde(default)]
    pub not_run: Vec<String>,
    #[serde(default)]
    pub cost: CostCompleteness,
    /// About 300 tokens.
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub next: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Artifact {
    pub fn new(id: &str, task: &str, attempt: u32, kind: ArtifactKind, base: &str, contract_digest: &str) -> Self {
        Artifact {
            schema: SCHEMA,
            id: id.into(),
            task: task.into(),
            attempt,
            kind,
            base: base.into(),
            tree: None,
            contract_digest: contract_digest.into(),
            files: vec![],
            changed: vec![],
            commands: vec![],
            not_run: vec![],
            cost: CostCompleteness::Unknown,
            summary: String::new(),
            assumptions: vec![],
            risks: vec![],
            next: String::new(),
            extra: Extra::new(),
        }
    }
}

// Intents and receipts

/// What an intent is about to do. Dispatch spends; the others are git or update effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntentKind {
    Dispatch,
    Worktree,
    Commit,
    FastForward,
    Push,
    DraftPr,
    Promote,
}

impl IntentKind {
    /// The grant effect it needs, if any.
    pub fn effect(self) -> Option<Effect> {
        match self {
            IntentKind::Dispatch => None,
            IntentKind::Worktree => Some(Effect::Worktree),
            IntentKind::Commit => Some(Effect::Commit),
            IntentKind::FastForward => Some(Effect::FastForward),
            IntentKind::Push => Some(Effect::Push),
            IntentKind::DraftPr => Some(Effect::DraftPr),
            IntentKind::Promote => Some(Effect::SelfUpdate),
        }
    }
}

/// Written before an effect. After a crash, an intent without a receipt is reconciled, never replayed blindly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub schema: u32,
    pub id: String,
    pub mission: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub kind: IntentKind,
    pub grant_revision: u64,
    /// What reconciliation matches against: parent, tree, ref, old and new head, invocation id.
    #[serde(default)]
    pub expected: BTreeMap<String, String>,
    pub at: u64,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Intent {
    pub fn new(id: &str, mission: &str, kind: IntentKind, grant_revision: u64, at: u64) -> Self {
        Intent {
            schema: SCHEMA,
            id: id.into(),
            mission: mission.into(),
            task: None,
            kind,
            grant_revision,
            expected: BTreeMap::new(),
            at,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Done,
    Failed,
    /// It may or may not have happened; the work parks until reconciliation decides.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub schema: u32,
    pub intent: String,
    pub outcome: Outcome,
    pub at: u64,
    #[serde(default)]
    pub evidence: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Receipt {
    pub fn new(intent: &str, outcome: Outcome, at: u64) -> Self {
        Receipt { schema: SCHEMA, intent: intent.into(), outcome, at, evidence: BTreeMap::new(), reason: None, extra: Extra::new() }
    }
}

/// The private sidecar in Faber's data folder: CLI sessions per task and the leases with their processes. Never in the
/// repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    pub schema: u32,
    #[serde(default)]
    pub sessions: BTreeMap<String, BTreeMap<Vendor, String>>,
    #[serde(default)]
    pub leases: Vec<Lease>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Runtime {
    pub fn new() -> Self {
        Runtime { schema: SCHEMA, sessions: BTreeMap::new(), leases: vec![], extra: Extra::new() }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime::new()
    }
}

/// A record with a schema version, so a reader can refuse one newer than itself.
pub trait Record {
    fn schema(&self) -> u32;
}

macro_rules! records {
    ($($t:ty),*) => {$(
        impl Record for $t {
            fn schema(&self) -> u32 {
                self.schema
            }
        }
    )*};
}

records!(
    Mission,
    Plan,
    Contract,
    Decision,
    Brief,
    Lease,
    Artifact,
    Intent,
    Receipt,
    Runtime,
    update::UpdateManifest,
    update::LastGoodRecord
);

pub mod policy;
