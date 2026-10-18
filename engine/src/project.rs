//! What a project may do and the records that stay from the self-development missions (R96, retired by R215): the git
//! authority of a project, a decision, the roles a CLI session can take, and the limits the other parts share. The published
//! contracts are in `docs/design/projects.md`.
//!
//! Every record carries a schema version and keeps the fields a newer Faber wrote in `extra`, so an older build can read and
//! rewrite them without dropping anything.

pub mod budget;
pub mod checks;
pub mod copy;
pub mod files;
pub mod git;
pub mod kit;
pub mod lease;
pub mod proof;
pub mod qualification;
pub mod wire;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The schema version this build writes, and the newest it reads.
pub const SCHEMA: u32 = 1;
/// The most tasks a plan held; a check receipt names at most this many groups of checks.
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

// Roles and processes

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

/// A process by its id and the moment it started: the start time tells a reused id from the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessId {
    pub pid: u32,
    pub started_at: u64,
}

// Outcomes and records

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Done,
    Failed,
    /// It may or may not have happened; the work parks until reconciliation decides.
    Unknown,
}

/// A record with a schema version, so a reader can refuse one newer than itself.
pub trait Record {
    fn schema(&self) -> u32;
}

pub mod policy;
pub mod remote;
