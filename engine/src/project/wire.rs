//! The fixed contracts between the engine and the two other parties of a mission.
//!
//! - Workers answer each turn with one strict JSON [`Envelope`]: one action, a bounded size, the identity of the work it
//!   belongs to. Unknown actions or fields, stale identities, oversized content and paths that leave the worktree are
//!   refused before anything acts on them.
//! - The screen calls the fixed [`COMMANDS`] with the request types below and receives [`MissionView`] on the
//!   [`EVENT`] event. Commands fail with a key string (`error.*`), as the other shell commands do.
//!
//! Both are versioned and documented with examples in `docs/design.md`.

use super::{
    Author, DecisionKind, GitMode, GraphError, MAX_TASKS, MissionState, Plan, Role, RunMode, Task, TaskKind, TaskState, clean_path,
    valid_id,
};
use crate::model::Vendor;
use serde::{Deserialize, Serialize};

mod rework;
pub use rework::*;

/// The envelope version this build reads and writes.
pub const SCHEMA: u32 = 2;
/// The largest envelope, in bytes, read at all.
pub const MAX_ENVELOPE: usize = 512 * 1024;
/// The most paths (or other list items) one action may carry.
pub const MAX_PATHS: usize = 64;
/// The most file operations one patch may carry.
pub const MAX_OPS: usize = 64;
/// The largest file content one write may carry, in bytes.
pub const MAX_FILE: usize = 256 * 1024;
/// The largest free text (a summary, a question), in bytes.
pub const MAX_TEXT: usize = 8 * 1024;

/// Why an envelope was refused, as an `error.wire.*` key (or the graph's own key for a bad plan).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    TooBig,
    Malformed,
    Schema,
    Stale,
    Path,
    Role,
    Plan(GraphError),
}

impl WireError {
    pub fn key(&self) -> &'static str {
        match self {
            WireError::TooBig => "error.wire.tooBig",
            WireError::Malformed => "error.wire.malformed",
            WireError::Schema => "error.wire.schema",
            WireError::Stale => "error.wire.stale",
            WireError::Path => "error.wire.path",
            WireError::Role => "error.wire.role",
            WireError::Plan(e) => e.key(),
        }
    }
}

/// The identity the engine expects the next envelope to carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expect {
    pub task: String,
    pub attempt: u32,
    /// The base tree or revision the attempt works on.
    pub base: String,
    /// The digest of the contract it builds against.
    pub contract: String,
    /// The lease generation it holds (0 for work without a lease).
    pub lease: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Envelope {
    pub schema: u32,
    pub task: String,
    pub attempt: u32,
    pub base: String,
    pub contract: String,
    pub lease: u64,
    pub action: Action,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ping: Option<Ping>,
    /// Requirement ids; TaskDraft.requires remains the task dependency graph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// One action per turn, written as `{"<name>": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    Read(Read),
    Plan(PlanDraft),
    Amend(Amend),
    Patch(Patch),
    Proof(ProofRequest),
    Decision(Ask),
    Complete(Complete),
}

/// Source to read, and symbols whose callers to find, without loading the whole repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Read {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub callers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskDraft {
    pub id: String,
    pub title: String,
    pub kind: TaskKind,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    /// Contract ids; the engine pins their current digests.
    #[serde(default)]
    pub contracts: Vec<String>,
    #[serde(default)]
    pub deliverables: Vec<String>,
    #[serde(default)]
    pub writes: Vec<String>,
    #[serde(default)]
    pub proofs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Labels>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kit: Option<KitPin>,
}

impl TaskDraft {
    /// The task as the graph holds it, with its contracts not pinned yet.
    pub fn to_task(&self) -> Task {
        let mut t = Task::new(&self.id, &self.title, self.kind);
        t.requires = self.requires.clone();
        t.decisions = self.decisions.clone();
        t.contracts = self.contracts.iter().map(|id| super::ContractPin { id: id.clone(), digest: String::new() }).collect();
        t.deliverables = self.deliverables.clone();
        t.writes = self.writes.clone();
        t.proofs = self.proofs.clone();
        t.labels = self.labels.clone();
        t.checks = self.checks.clone();
        t.paths = self.paths.clone();
        t.kit = self.kit.clone();
        t
    }
}

/// The lead's plan: a short summary, the acceptance, and the graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanDraft {
    pub summary: String,
    pub acceptance: String,
    pub tasks: Vec<TaskDraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<DeliverableSheet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bar: Vec<BarCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compile_receipt: Option<String>,
}

/// Tasks to add or replace and task ids to drop; checked against the current plan by the coordinator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Amend {
    #[serde(default)]
    pub tasks: Vec<TaskDraft>,
    #[serde(default)]
    pub remove: Vec<String>,
    pub reason: String,
}

/// Explicit file operations with the expected SHA-256 of the current content, never executable text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Patch {
    pub ops: Vec<FileOp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileOp {
    Write(WriteOp),
    Rename(RenameOp),
    Delete(DeleteOp),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteOp {
    pub path: String,
    /// The hash of the file as the worker read it; none means the file must not exist yet.
    #[serde(default)]
    pub expect: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameOp {
    pub from: String,
    pub to: String,
    pub expect: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteOp {
    pub path: String,
    pub expect: String,
}

/// Named proofs to run; never command text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProofRequest {
    pub ids: Vec<String>,
}

/// A question for the decision list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ask {
    pub question: String,
    pub kind: DecisionKind,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    Pass,
    Block,
}

/// The hand-back: a short summary and what the lead needs; the evidence stays behind references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Complete {
    pub summary: String,
    #[serde(default)]
    pub changed: Vec<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub not_run: Vec<String>,
    /// A reviewer's verdict and its blocking findings.
    #[serde(default)]
    pub verdict: Option<Verdict>,
    #[serde(default)]
    pub findings: Vec<String>,
    pub next: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<CheckOutcome>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<Claim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumption_records: Vec<Assumption>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unconfirmed: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kit_used: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
}

/// Which actions each role may send. Workers cannot plan, ask the person, or spawn peers; only writers and test
/// proposals send patches, and a test proposal's patch is kept as an artifact outside the source.
pub fn allowed(role: Role, action: &Action) -> bool {
    use Action as A;
    match action {
        A::Read(_) | A::Complete(_) => true,
        A::Plan(_) | A::Amend(_) | A::Decision(_) => role == Role::Lead,
        A::Proof(_) => matches!(role, Role::Lead | Role::Writer),
        A::Patch(_) => matches!(role, Role::Writer | Role::TestProposal),
    }
}

fn text(s: &str) -> Result<(), WireError> {
    if s.len() > MAX_TEXT { Err(WireError::TooBig) } else { Ok(()) }
}

fn texts(list: &[String]) -> Result<(), WireError> {
    if list.len() > MAX_PATHS {
        return Err(WireError::TooBig);
    }
    list.iter().try_for_each(|s| text(s))
}

/// A path a worker may name: inside the worktree and not the worktree itself.
fn path(p: &str) -> Result<(), WireError> {
    super::kit::delivery::scoped_path(p)?;
    match clean_path(p) {
        Some(clean) if !clean.is_empty() => Ok(()),
        _ => Err(WireError::Path),
    }
}

fn paths(list: &[String]) -> Result<(), WireError> {
    if list.len() > MAX_PATHS {
        return Err(WireError::TooBig);
    }
    list.iter().try_for_each(|p| path(p))
}

fn hash(h: &str) -> Result<(), WireError> {
    let ok = h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if ok { Ok(()) } else { Err(WireError::Malformed) }
}

fn ids(list: &[String]) -> Result<(), WireError> {
    if list.len() > MAX_PATHS {
        return Err(WireError::TooBig);
    }
    if list.iter().all(|id| valid_id(id)) { Ok(()) } else { Err(WireError::Malformed) }
}

fn drafts(tasks: &[TaskDraft]) -> Result<(), WireError> {
    if tasks.len() > MAX_TASKS {
        return Err(WireError::TooBig);
    }
    for t in tasks {
        text(&t.title)?;
        for list in [&t.requires, &t.decisions, &t.contracts] {
            ids(list)?;
        }
        check_ids(&t.proofs)?;
        texts(&t.deliverables)?;
        texts(&t.writes)?;
        check_ids(&t.checks)?;
        paths(&t.paths)?;
        if let Some(labels) = &t.labels {
            labels.validate()?;
        }
        if let Some(kit) = &t.kit {
            kit.validate()?;
        }
    }
    Ok(())
}

fn check_ids(list: &[String]) -> Result<(), WireError> {
    if list.len() > MAX_PATHS {
        return Err(WireError::TooBig);
    }
    list.iter().try_for_each(|id| check_id(id))
}

impl Action {
    fn check(&self) -> Result<(), WireError> {
        match self {
            Action::Read(r) => {
                paths(&r.paths)?;
                texts(&r.callers)
            }
            Action::Plan(p) => {
                text(&p.summary)?;
                text(&p.acceptance)?;
                drafts(&p.tasks)?;
                if let Some(sheet) = &p.sheet {
                    sheet.validate()?;
                }
                if p.bar.len() > MAX_TASKS * MAX_PATHS {
                    return Err(WireError::TooBig);
                }
                for check in &p.bar {
                    check.validate()?;
                }
                if let Some(receipt) = &p.compile_receipt {
                    ids(std::slice::from_ref(receipt))?;
                }
                let mut plan = Plan::new("draft", 0, p.tasks.iter().map(TaskDraft::to_task).collect());
                plan.sheet = p.sheet.clone();
                plan.bar = p.bar.clone();
                plan.validate().map_err(WireError::Plan)
            }
            Action::Amend(a) => {
                text(&a.reason)?;
                ids(&a.remove)?;
                drafts(&a.tasks)
            }
            Action::Patch(p) => {
                if p.ops.is_empty() {
                    return Err(WireError::Malformed);
                }
                if p.ops.len() > MAX_OPS {
                    return Err(WireError::TooBig);
                }
                p.ops.iter().try_for_each(|op| match op {
                    FileOp::Write(w) => {
                        path(&w.path)?;
                        if w.content.len() > MAX_FILE {
                            return Err(WireError::TooBig);
                        }
                        w.expect.as_deref().map_or(Ok(()), hash)
                    }
                    FileOp::Rename(r) => {
                        path(&r.from)?;
                        path(&r.to)?;
                        hash(&r.expect)
                    }
                    FileOp::Delete(d) => {
                        path(&d.path)?;
                        hash(&d.expect)
                    }
                })
            }
            Action::Proof(p) => check_ids(&p.ids),
            Action::Decision(a) => {
                text(&a.question)?;
                texts(&a.options)?;
                a.default.as_deref().map_or(Ok(()), text)
            }
            Action::Complete(c) => {
                text(&c.summary)?;
                text(&c.next)?;
                paths(&c.changed)?;
                for list in [&c.assumptions, &c.risks, &c.not_run, &c.findings] {
                    texts(list)?;
                }
                if c.checks.len() > MAX_PATHS
                    || c.artifacts.len() > MAX_PATHS
                    || c.claims.len() > MAX_PATHS
                    || c.assumption_records.len() > MAX_PATHS
                    || c.needs.len() > 8
                {
                    return Err(WireError::TooBig);
                }
                if let Some(receipt) = &c.receipt {
                    ids(std::slice::from_ref(receipt))?;
                }
                for check in &c.checks {
                    check.validate()?;
                }
                for artifact in &c.artifacts {
                    artifact.validate()?;
                }
                for claim in &c.claims {
                    check_id(&claim.check)?;
                    text(&claim.text)?;
                }
                for assumption in &c.assumption_records {
                    assumption.validate()?;
                }
                texts(&c.unconfirmed)?;
                check_ids(&c.kit_used)?;
                for need in &c.needs {
                    normalize_need(need)?;
                }
                Ok(())
            }
        }
    }
}

impl Envelope {
    /// Reads one worker turn for `role`, refusing anything oversized, malformed, of another schema, stale against
    /// `expect`, outside the role, or naming a path outside the worktree.
    pub fn parse(text: &str, expect: &Expect, role: Role) -> Result<Envelope, WireError> {
        if text.len() > MAX_ENVELOPE {
            return Err(WireError::TooBig);
        }
        let value: serde_json::Value = serde_json::from_str(text).map_err(|_| WireError::Malformed)?;
        if value.get("schema").and_then(serde_json::Value::as_u64) != Some(u64::from(SCHEMA)) {
            return Err(if value.get("schema").is_some() { WireError::Schema } else { WireError::Malformed });
        }
        let env: Envelope = serde_json::from_value(value).map_err(|_| WireError::Malformed)?;
        let same = env.task == expect.task
            && env.attempt == expect.attempt
            && env.base == expect.base
            && env.contract == expect.contract
            && env.lease == expect.lease;
        if !same {
            return Err(WireError::Stale);
        }
        if !allowed(role, &env.action) {
            return Err(WireError::Role);
        }
        ids(&env.requires)?;
        check_ids(&env.checks)?;
        ids(&env.quotes)?;
        if env.note.len() > MAX_TEXT || env.note.split_whitespace().count() > 60 {
            return Err(WireError::TooBig);
        }
        env.action.check()?;
        Ok(env)
    }
}

// The screen contract

/// The shell commands of Projects, in the order of `docs/design.md`.
pub const COMMANDS: [&str; 9] = [
    "project_enroll",
    "project_grant",
    "project_revoke",
    "mission_start",
    "mission_state",
    "mission_mode",
    "mission_stop",
    "mission_resume",
    "mission_answer",
];

/// The event that carries a [`MissionView`] whenever a mission changes.
pub const EVENT: &str = "mission";

/// `project_enroll` and `project_revoke`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRequest {
    pub project_id: String,
}

/// `project_grant`: the mode and trust the person chose on the start card, for the revision that card showed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantRequest {
    pub project_id: String,
    pub mode: GitMode,
    pub self_update: bool,
    pub revision: u64,
}

/// `mission_start`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRequest {
    pub project_id: String,
    pub goal: String,
}

/// `mission_state`, `mission_stop` and `mission_resume`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MissionRequest {
    pub mission_id: String,
}

/// `mission_mode`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModeRequest {
    pub mission_id: String,
    pub mode: RunMode,
}

/// `mission_answer`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnswerRequest {
    pub mission_id: String,
    pub decision_id: String,
    pub answer: String,
}

/// What `project_enroll` returns for the start card. Nothing is granted until `project_grant`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollView {
    pub project_id: String,
    /// The folder shown to the person.
    pub repo: String,
    pub base: String,
    pub author: Option<Author>,
    /// Selected, not granted.
    pub mode: GitMode,
    /// The grant revision `project_grant` must echo.
    pub revision: u64,
    pub self_update: bool,
    pub remote: Option<String>,
    pub remote_account: Option<String>,
}

/// One mission as the board draws it. `revision` grows with every view the engine sends; the screen ignores an older
/// one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionView {
    pub mission_id: String,
    pub revision: u64,
    pub state: MissionState,
    pub mode: RunMode,
    pub git_mode: GitMode,
    pub goal: String,
    pub plan: Option<String>,
    pub tasks: Vec<TaskView>,
    pub cost: CostView,
    pub pace: Vec<PaceView>,
    pub decisions: Vec<DecisionView>,
    pub build: Option<BuildView>,
    /// A `reason.*` key while paused or blocked.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskView {
    pub id: String,
    pub title: String,
    pub state: TaskState,
    pub reason: Option<String>,
    pub builder: Option<Vendor>,
    pub reviewer: Option<Vendor>,
}

/// Own cost against the live cap, in millionths of an API-equivalent dollar. None is unknown, never zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostView {
    pub own_micros: Option<u64>,
    pub cap_micros: Option<u64>,
    pub complete: bool,
}

/// The shared account's quota and today's pace room, a separate ledger from own cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaceView {
    pub vendor: Vendor,
    pub window: String,
    pub used: Option<f64>,
    pub room: Option<f64>,
    /// Unix seconds of the reading, for freshness.
    pub read_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionView {
    pub id: String,
    pub question: String,
    pub kind: DecisionKind,
    pub options: Vec<String>,
    pub default: Option<String>,
    pub task: Option<String>,
}

/// The local build slots and why promotion waits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildView {
    pub current: Option<String>,
    pub next: Option<String>,
    pub last_good: Option<String>,
    pub waiting: Option<String>,
}

/// A full [`MissionView`], like the shorter example in `docs/design.md`, kept here so a test proves the shape parses.
pub const MISSION_EXAMPLE: &str = r#"{
  "missionId": "m1",
  "revision": 7,
  "state": "running",
  "mode": "normal",
  "gitMode": "seguro",
  "goal": "Fix the failing normalization test",
  "plan": "One writer fixes clean_path; the other vendor reviews the composed tree.",
  "tasks": [
    {"id": "fix", "title": "Fix clean_path", "state": "running", "reason": null, "builder": "codex", "reviewer": "claude"},
    {"id": "land", "title": "Prove and land", "state": "blocked", "reason": "reason.task.waiting", "builder": null, "reviewer": null}
  ],
  "cost": {"ownMicros": null, "capMicros": 2000000, "complete": false},
  "pace": [{"vendor": "claude", "window": "seven_day", "used": 0.31, "room": 0.04, "readAt": 1791000000.0}],
  "decisions": [],
  "build": null,
  "reason": null
}"#;
