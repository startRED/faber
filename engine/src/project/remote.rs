//! Equilibrado's remote effects: a non-forced push of the mission's own branch, one draft pull request, the PR's check
//! results and a merge that only happens after the person's explicit yes.
//!
//! git and gh run shell-free through [`Runner`], with the person's own git credential helper and gh login; Faber
//! never reads a token or a credential file. Every effect writes its intent before and its receipt after through
//! [`Ledger`]. A call that may have reached the remote but gave no answer is settled by reading the live remote state,
//! never by running it again; when that read fails too the receipt stays `Unknown` and nothing else may leave.
//! Errors are `reason.*` keys.

use super::git::valid_oid;
use super::proof::scan_secrets;
use super::{DecidedBy, Decision, DecisionKind, Effect, Grant, Outcome, RepoIdentity};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// What runs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Git,
    Gh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub tool: Tool,
    pub cwd: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: i32,
    pub stdout: String,
}

/// A call that gave no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lost {
    /// The tool is not installed: nothing ran.
    Missing,
    /// It started but no answer came (a dropped link, a timeout): it may or may not have taken effect.
    Interrupted,
}

pub trait Runner {
    fn run(&mut self, call: &Call) -> Result<Reply, Lost>;
}

// What the journal keeps

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Push,
    Pr,
    Merge,
}

/// A remote intent that has no settled receipt: written but not answered, or `Unknown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub id: String,
    pub step: Step,
    pub expected: BTreeMap<String, String>,
}

/// The seam to the mission journal. `receipt` records how a live effect ended, once. `settle` is the only way out of
/// `Unknown` (or of an intent whose receipt was never written): it carries what the engine read from the remote.
pub trait Ledger {
    /// The current grant, read before every effect so a revocation stops the next one.
    fn grant(&mut self) -> Result<Option<Grant>, RemoteError>;
    fn open(&mut self) -> Result<Vec<Open>, RemoteError>;
    fn intent(&mut self, id: &str, step: Step, expected: BTreeMap<String, String>) -> Result<(), RemoteError>;
    fn receipt(&mut self, id: &str, outcome: Outcome, evidence: BTreeMap<String, String>, reason: Option<&str>) -> Result<(), RemoteError>;
    fn settle(&mut self, id: &str, outcome: Outcome, evidence: BTreeMap<String, String>) -> Result<(), RemoteError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteError {
    /// The grant refused, with its own `reason.grant.*` key.
    Grant(&'static str),
    GhMissing,
    GhLoggedOut,
    GitMissing,
    /// A value that could name something other than the own branch, the granted remote or a bounded PR text.
    Target,
    /// An earlier remote effect is not settled yet.
    Pending,
    /// The repository's own config would run a command or reroute the connection.
    Config,
    /// The remote's effective push URL is not the granted one.
    Url,
    Secret,
    Unreachable,
    Diverged,
    Rejected,
    /// It may or may not have happened and the remote could not be read. The receipt is `Unknown`.
    Maybe,
    NotPushed,
    HeadMoved,
    /// Output that is not what git or gh print.
    Command,
    NeedsYes,
    Ci,
    NotOpen,
    PrFailed,
    MergeFailed,
    /// The journal refused, with its own `error.*` key.
    Ledger(&'static str),
}

impl RemoteError {
    pub fn key(self) -> &'static str {
        match self {
            Self::Grant(key) | Self::Ledger(key) => key,
            Self::GhMissing => "reason.remote.ghMissing",
            Self::GhLoggedOut => "reason.remote.ghLoggedOut",
            Self::GitMissing => "reason.git.unknown",
            Self::Target => "reason.remote.target",
            Self::Pending => "reason.remote.pending",
            Self::Config => "reason.remote.config",
            Self::Url => "reason.remote.url",
            Self::Secret => "reason.remote.secret",
            Self::Unreachable => "reason.remote.unreachable",
            Self::Diverged => "reason.remote.diverged",
            Self::Rejected => "reason.remote.rejected",
            Self::Maybe => "reason.remote.maybe",
            Self::NotPushed => "reason.remote.notPushed",
            Self::HeadMoved => "reason.remote.headMoved",
            Self::Command => "reason.git.command",
            Self::NeedsYes => "reason.remote.needsYes",
            Self::Ci => "reason.remote.ci",
            Self::NotOpen => "reason.remote.notOpen",
            Self::PrFailed => "reason.remote.prFailed",
            Self::MergeFailed => "reason.remote.mergeFailed",
        }
    }
}

/// Everything one remote effect is bound to.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// A checkout of the repository; git and gh run here.
    pub cwd: PathBuf,
    pub identity: RepoIdentity,
    /// The grant revision the work was admitted under.
    pub revision: u64,
    /// `owner/name`, from [`preflight`] and shown at the grant.
    pub repo: String,
    /// The git remote name the grant names.
    pub remote: String,
    /// That remote's effective push URL as shown at the grant.
    pub url: String,
    /// The mission's own branch, `faber/...`.
    pub branch: String,
    /// The proven commit. Only this object is pushed, whatever the branch became since.
    pub head: String,
    /// The person's branch the PR targets.
    pub base: String,
    /// Grows with every deliberate retry after a failed effect, so each try has its own intent.
    pub attempt: u32,
}

fn ref_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && !name.contains("@{")
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
        && name.split('/').all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

fn name_ok(name: &str) -> bool {
    (1..=100).contains(&name.len()) && name != "." && name != ".." && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}

fn slug_ok(repo: &str) -> bool {
    repo.split_once('/').is_some_and(|(owner, name)| name_ok(owner) && name_ok(name) && !name.contains('/'))
}

fn check(t: &Target) -> Result<(), RemoteError> {
    let ok = t.branch.starts_with("faber/")
        && ref_ok(&t.branch)
        && ref_ok(&t.base)
        && valid_oid(&t.head)
        && t.head.len() >= 12
        && name_ok(&t.remote)
        && !t.remote.starts_with(['-', '.'])
        && slug_ok(&t.repo)
        && !t.url.is_empty()
        && t.url.len() <= 512
        && !t.url.chars().any(|c| c.is_control() || c.is_whitespace());
    if ok { Ok(()) } else { Err(RemoteError::Target) }
}

fn short(t: &Target) -> &str {
    &t.head[..12]
}

// Authority and calls

/// Seguro and every other refusal of the grant, read fresh. Reads of the remote need the same grant as its effects.
fn authority(ledger: &mut impl Ledger, t: &Target, effect: Effect) -> Result<(), RemoteError> {
    let grant = ledger.grant()?.ok_or(RemoteError::Grant("reason.grant.none"))?;
    grant.allows(&t.identity, t.revision, effect).map_err(RemoteError::Grant)?;
    if grant.remote.as_deref() != Some(t.remote.as_str()) {
        return Err(RemoteError::Grant("reason.grant.noRemote"));
    }
    Ok(())
}

fn settled(ledger: &mut impl Ledger) -> Result<(), RemoteError> {
    if ledger.open()?.is_empty() { Ok(()) } else { Err(RemoteError::Pending) }
}

const GIT_OPTIONS: [&str; 9] = [
    "core.pager=",
    "core.fsmonitor=false",
    "diff.external=",
    "protocol.ext.allow=never",
    "submodule.recurse=false",
    "push.recurseSubmodules=no",
    "push.followTags=false",
    "push.gpgSign=false",
    "core.attributesFile=",
];

fn git(cwd: &Path, args: &[&str]) -> Call {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut all = vec!["-c".to_string(), format!("core.hooksPath={null}")];
    for option in GIT_OPTIONS {
        all.extend(["-c".into(), option.into()]);
    }
    all.extend(args.iter().map(|s| (*s).to_string()));
    Call { tool: Tool::Git, cwd: cwd.into(), args: all }
}

fn gh(cwd: &Path, args: &[&str]) -> Call {
    Call { tool: Tool::Gh, cwd: cwd.into(), args: args.iter().map(|s| (*s).to_string()).collect() }
}

fn missing(tool: Tool) -> RemoteError {
    match tool {
        Tool::Git => RemoteError::GitMissing,
        Tool::Gh => RemoteError::GhMissing,
    }
}

/// A call whose answer is needed now. A call that gave none is a remote that cannot be reached.
fn ask(runner: &mut impl Runner, call: &Call) -> Result<Reply, RemoteError> {
    runner.run(call).map_err(|lost| match lost {
        Lost::Missing => missing(call.tool),
        Lost::Interrupted => RemoteError::Unreachable,
    })
}

// Preflight

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    /// `owner/name` of the repository gh resolves in the checkout.
    pub repo: String,
}

/// Whether gh is installed and signed in, and who and where it would act. Shown on the Equilibrado start card.
pub fn preflight(runner: &mut impl Runner, cwd: &Path) -> Result<Account, RemoteError> {
    if ask(runner, &gh(cwd, &["auth", "status"]))?.code != 0 {
        return Err(RemoteError::GhLoggedOut);
    }
    let login = ask(runner, &gh(cwd, &["api", "user", "--jq", ".login"]))?;
    let repo = ask(runner, &gh(cwd, &["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]))?;
    let (login, repo) = (login.stdout.trim().to_string(), repo.stdout.trim().to_string());
    if login.is_empty() || !name_ok(&login) || login.len() > 39 || !slug_ok(&repo) || repo.len() > 201 {
        return Err(RemoteError::Command);
    }
    Ok(Account { login, repo })
}

// Push

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pushed {
    pub head: String,
    /// The remote already had the proven commit; nothing was sent.
    pub already: bool,
}

const BLOCKING_CONFIG: &str = r"^(core\.(sshcommand|gitproxy|askpass)|credential\.(.*\.)?helper|remote\..*\.(proxy|receivepack|uploadpack|vcs)|hook\..*|http\.(.*\.)?proxy)$";

/// The repository's own config must not run a command or reroute the connection. The person's global config, with
/// the credential helper, is not looked at: it is the person's own.
fn guard_config(runner: &mut impl Runner, t: &Target) -> Result<(), RemoteError> {
    let reply = ask(runner, &git(&t.cwd, &["config", "--local", "--includes", "--name-only", "--get-regexp", BLOCKING_CONFIG]))?;
    match reply.code {
        1 => Ok(()),
        0 => Err(RemoteError::Config),
        _ => Err(RemoteError::Command),
    }
}

/// The remote name must still lead to the URL the person granted.
fn pin_url(runner: &mut impl Runner, t: &Target) -> Result<(), RemoteError> {
    let reply = ask(runner, &git(&t.cwd, &["remote", "get-url", "--push", &t.remote]))?;
    if reply.code == 0 && reply.stdout.trim() == t.url { Ok(()) } else { Err(RemoteError::Url) }
}

/// Everything that would leave: commits the remote does not have, as patches. Fails closed on any error.
fn scan_outgoing(runner: &mut impl Runner, t: &Target) -> Result<(), RemoteError> {
    let remotes = format!("--remotes={}", t.remote);
    let reply = ask(runner, &git(&t.cwd, &["log", "-p", "--format=", "--no-ext-diff", "--no-textconv", &t.head, "--not", &remotes]))?;
    if reply.code != 0 {
        return Err(RemoteError::Command);
    }
    scan_secrets(&reply.stdout).map_err(|_| RemoteError::Secret)
}

/// The branch on the live remote, if it exists there.
fn remote_branch(runner: &mut impl Runner, t: &Target) -> Result<Option<String>, RemoteError> {
    let full = format!("refs/heads/{}", t.branch);
    let reply = ask(runner, &git(&t.cwd, &["ls-remote", "--heads", &t.remote, &full]))?;
    if reply.code != 0 {
        return Err(RemoteError::Unreachable);
    }
    for line in reply.stdout.lines() {
        if let Some((oid, name)) = line.split_once('\t')
            && name == full
        {
            return if valid_oid(oid) { Ok(Some(oid.into())) } else { Err(RemoteError::Command) };
        }
    }
    Ok(None)
}

/// What a read of the remote says about an effect that gave no clean answer.
enum Seen<T> {
    Yes(T),
    No,
    Unsure,
}

fn evidence(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

fn probe_push(runner: &mut impl Runner, t: &Target, expected: &BTreeMap<String, String>) -> Seen<BTreeMap<String, String>> {
    let new = expected.get("new").map_or("", String::as_str);
    match remote_branch(runner, t) {
        Ok(now) if now.as_deref() == Some(new) => Seen::Yes(evidence(&[("new", new), ("via", "reconcile")])),
        // The old head or somebody else's: our push did not land.
        Ok(_) => Seen::No,
        Err(_) => Seen::Unsure,
    }
}

/// Pushes the proven commit to the own branch. Never forced, never through a hook, and never again after an uncertain
/// delivery: that is settled by reading the remote.
pub fn push(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target) -> Result<Pushed, RemoteError> {
    check(t)?;
    authority(ledger, t, Effect::Push)?;
    settled(ledger)?;
    guard_config(runner, t)?;
    pin_url(runner, t)?;
    scan_outgoing(runner, t)?;
    let old = remote_branch(runner, t)?;
    if old.as_deref() == Some(t.head.as_str()) {
        return Ok(Pushed { head: t.head.clone(), already: true });
    }
    if let Some(old) = &old {
        // An older head we do not even have, or one that is not behind ours, is a branch somebody else moved on.
        let behind = ask(runner, &git(&t.cwd, &["merge-base", "--is-ancestor", old, &t.head]))?;
        if behind.code != 0 {
            return Err(RemoteError::Diverged);
        }
    }
    let id = format!("push-{}-{}", short(t), t.attempt);
    let full = format!("refs/heads/{}", t.branch);
    let expected = evidence(&[("remote", &t.remote), ("ref", &full), ("old", old.as_deref().unwrap_or("")), ("new", &t.head)]);
    ledger.intent(&id, Step::Push, expected.clone())?;
    if let Err(e) = authority(ledger, t, Effect::Push) {
        ledger.receipt(&id, Outcome::Failed, expected, Some(e.key()))?;
        return Err(e);
    }
    let spec = format!("{}:{full}", t.head);
    let sent = runner.run(&git(&t.cwd, &["push", "--no-verify", "--porcelain", &t.remote, &spec]));
    match sent {
        Ok(reply) if reply.code == 0 => {
            ledger.receipt(&id, Outcome::Done, evidence(&[("new", &t.head), ("via", "push")]), None)?;
            Ok(Pushed { head: t.head.clone(), already: false })
        }
        Err(Lost::Missing) => {
            ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::GitMissing.key()))?;
            Err(RemoteError::GitMissing)
        }
        _ => match probe_push(runner, t, &expected) {
            Seen::Yes(found) => {
                ledger.receipt(&id, Outcome::Done, found, None)?;
                Ok(Pushed { head: t.head.clone(), already: false })
            }
            Seen::No => {
                ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::Rejected.key()))?;
                Err(RemoteError::Rejected)
            }
            Seen::Unsure => {
                ledger.receipt(&id, Outcome::Unknown, expected, Some(RemoteError::Maybe.key()))?;
                Err(RemoteError::Maybe)
            }
        },
    }
}

// Pull requests

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pr {
    pub number: u64,
    pub url: String,
    pub draft: bool,
    pub state: PrState,
    /// The commit the PR's branch points at.
    pub head: String,
    pub base: String,
    /// Found rather than created by this call.
    pub existing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSpec {
    pub title: String,
    pub body: String,
}

const PR_FIELDS: &str = "number,url,isDraft,state,headRefName,headRefOid,baseRefName";
const PR_VIEW_FIELDS: &str = "number,url,isDraft,state,headRefName,headRefOid,baseRefName,statusCheckRollup";

fn parse_pr(value: &Value) -> Result<Pr, RemoteError> {
    let state = match value["state"].as_str() {
        Some("OPEN") => PrState::Open,
        Some("MERGED") => PrState::Merged,
        Some("CLOSED") => PrState::Closed,
        _ => return Err(RemoteError::Command),
    };
    let text = |key: &str| value[key].as_str().map(str::to_string).ok_or(RemoteError::Command);
    Ok(Pr {
        number: value["number"].as_u64().ok_or(RemoteError::Command)?,
        url: text("url")?,
        draft: value["isDraft"].as_bool().ok_or(RemoteError::Command)?,
        state,
        head: text("headRefOid").and_then(|oid| if valid_oid(&oid) { Ok(oid) } else { Err(RemoteError::Command) })?,
        base: text("baseRefName")?,
        existing: false,
    })
}

fn json(reply: &Reply) -> Result<Value, RemoteError> {
    if reply.code != 0 {
        return Err(RemoteError::Unreachable);
    }
    serde_json::from_str(&reply.stdout).map_err(|_| RemoteError::Command)
}

/// Every PR (any state) whose head is the own branch.
fn list_prs(runner: &mut impl Runner, t: &Target) -> Result<Vec<Pr>, RemoteError> {
    // ponytail: matched by branch name alone; a fork's PR with the very same `faber/...` name would also match.
    let reply = ask(
        runner,
        &gh(&t.cwd, &["pr", "list", "--repo", &t.repo, "--head", &t.branch, "--state", "all", "--limit", "5", "--json", PR_FIELDS]),
    )?;
    json(&reply)?.as_array().ok_or(RemoteError::Command)?.iter().map(parse_pr).collect()
}

fn probe_pr(runner: &mut impl Runner, t: &Target) -> Seen<Pr> {
    match list_prs(runner, t) {
        Ok(found) => found.into_iter().next().map_or(Seen::No, Seen::Yes),
        Err(_) => Seen::Unsure,
    }
}

fn pr_text_ok(spec: &PrSpec) -> bool {
    let plain = |s: &str, newlines: bool| !s.chars().any(|c| c.is_control() && !(newlines && matches!(c, '\n' | '\r' | '\t')));
    (1..=256).contains(&spec.title.chars().count())
        && plain(&spec.title, false)
        && spec.body.chars().count() <= 16_000
        && plain(&spec.body, true)
}

/// Opens the one draft PR of the own branch. An existing PR of that branch, in any state, is returned instead of a new
/// one. The proven commit must already be on the remote.
pub fn open_pr(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target, spec: &PrSpec) -> Result<Pr, RemoteError> {
    check(t)?;
    if !pr_text_ok(spec) {
        return Err(RemoteError::Target);
    }
    authority(ledger, t, Effect::DraftPr)?;
    settled(ledger)?;
    scan_secrets(&format!("{}\n{}", spec.title, spec.body)).map_err(|_| RemoteError::Secret)?;
    if let Some(found) = list_prs(runner, t)?.into_iter().next() {
        return Ok(Pr { existing: true, ..found });
    }
    if remote_branch(runner, t)?.as_deref() != Some(t.head.as_str()) {
        return Err(RemoteError::NotPushed);
    }
    let id = format!("pr-{}-{}", short(t), t.attempt);
    let expected = evidence(&[("repo", &t.repo), ("branch", &t.branch), ("head", &t.head), ("base", &t.base)]);
    ledger.intent(&id, Step::Pr, expected.clone())?;
    if let Err(e) = authority(ledger, t, Effect::DraftPr) {
        ledger.receipt(&id, Outcome::Failed, expected, Some(e.key()))?;
        return Err(e);
    }
    let (title, body) = (format!("--title={}", spec.title), format!("--body={}", spec.body));
    let created =
        runner.run(&gh(&t.cwd, &["pr", "create", "--draft", "--repo", &t.repo, "--head", &t.branch, "--base", &t.base, &title, &body]));
    let clean = matches!(&created, Ok(reply) if reply.code == 0);
    if let Err(Lost::Missing) = created {
        ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::GhMissing.key()))?;
        return Err(RemoteError::GhMissing);
    }
    // The PR list is read back either way, so the receipt always carries the number and a lost answer cannot duplicate.
    match probe_pr(runner, t) {
        Seen::Yes(found) => {
            let number = found.number.to_string();
            ledger.receipt(&id, Outcome::Done, evidence(&[("number", &number), ("url", &found.url)]), None)?;
            Ok(found)
        }
        Seen::No if !clean => {
            ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::PrFailed.key()))?;
            Err(RemoteError::PrFailed)
        }
        Seen::No | Seen::Unsure => {
            ledger.receipt(&id, Outcome::Unknown, expected, Some(RemoteError::Maybe.key()))?;
            Err(RemoteError::Maybe)
        }
    }
}

// Checks

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiState {
    /// The PR has no checks.
    None,
    Pending,
    Passing,
    Failing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ci {
    pub state: CiState,
    /// The commit these results are for.
    pub head: String,
    pub failed: Vec<String>,
    pub total: usize,
}

impl Ci {
    pub fn none(head: &str) -> Ci {
        Ci { state: CiState::None, head: head.into(), failed: vec![], total: 0 }
    }
}

fn classify(check: &Value) -> (CiState, String) {
    let name = check["name"].as_str().or(check["context"].as_str()).unwrap_or("check").to_string();
    let state = match check["__typename"].as_str() {
        Some("CheckRun") => match (check["status"].as_str(), check["conclusion"].as_str()) {
            (Some("COMPLETED"), Some("SUCCESS" | "NEUTRAL" | "SKIPPED")) => CiState::Passing,
            (Some("COMPLETED"), _) => CiState::Failing,
            _ => CiState::Pending,
        },
        Some("StatusContext") => match check["state"].as_str() {
            Some("SUCCESS") => CiState::Passing,
            Some("PENDING" | "EXPECTED") => CiState::Pending,
            _ => CiState::Failing,
        },
        _ => CiState::Pending,
    };
    (state, name)
}

fn summarize(checks: &[Value], head: &str) -> Ci {
    let seen: Vec<(CiState, String)> = checks.iter().map(classify).collect();
    let failed: Vec<String> = seen.iter().filter(|(s, _)| *s == CiState::Failing).map(|(_, n)| n.clone()).collect();
    let state = if !failed.is_empty() {
        CiState::Failing
    } else if seen.iter().any(|(s, _)| *s == CiState::Pending) {
        CiState::Pending
    } else if seen.is_empty() {
        CiState::None
    } else {
        CiState::Passing
    };
    Ci { state, head: head.into(), failed, total: seen.len() }
}

fn view_pr(runner: &mut impl Runner, t: &Target, number: u64) -> Result<(Pr, Ci), RemoteError> {
    let number = number.to_string();
    let reply = ask(runner, &gh(&t.cwd, &["pr", "view", &number, "--repo", &t.repo, "--json", PR_VIEW_FIELDS]))?;
    let value = json(&reply)?;
    let pr = parse_pr(&value)?;
    let checks = value["statusCheckRollup"].as_array().map_or(&[][..], Vec::as_slice);
    let ci = summarize(checks, &pr.head);
    Ok((pr, ci))
}

/// The PR's check results for the proven commit.
pub fn ci(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target, number: u64) -> Result<Ci, RemoteError> {
    check(t)?;
    authority(ledger, t, Effect::DraftPr)?;
    let (pr, ci) = view_pr(runner, t, number)?;
    if pr.head != t.head {
        return Err(RemoteError::HeadMoved);
    }
    Ok(ci)
}

// Merge

/// What the person is asked: this PR, at this commit, with these checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeCard {
    pub pr: u64,
    pub head: String,
    pub base: String,
    pub ci: Ci,
}

impl MergeCard {
    pub fn decision_id(&self) -> String {
        format!("merge-{}-{}", self.pr, &self.head[..12.min(self.head.len())])
    }

    /// The decision to put on the board. Only the person can answer it: it is a direction decision.
    pub fn decision(&self) -> Decision {
        let mut decision = Decision::new(&self.decision_id(), "lead", "decision.remote.merge", DecisionKind::Direction);
        decision.options = vec!["yes".into(), "no".into()];
        decision
    }
}

/// The person's yes to one card. It cannot be built any other way, so nothing the AI or an autonomous run answers
/// can unlock a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerYes {
    pr: u64,
    head: String,
}

impl OwnerYes {
    pub fn from_decision(card: &MergeCard, decision: &Decision) -> Option<OwnerYes> {
        (decision.id == card.decision_id()
            && decision.answered_by == Some(DecidedBy::Person)
            && decision.answer.as_deref() == Some("yes")
            && decision.answered_at.is_some())
        .then(|| OwnerYes { pr: card.pr, head: card.head.clone() })
    }
}

/// Builds the merge card from the live PR. Merges nothing.
pub fn merge_card(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target, number: u64) -> Result<MergeCard, RemoteError> {
    check(t)?;
    authority(ledger, t, Effect::DraftPr)?;
    let (pr, ci) = view_pr(runner, t, number)?;
    if pr.state != PrState::Open {
        return Err(RemoteError::NotOpen);
    }
    if pr.head != t.head {
        return Err(RemoteError::HeadMoved);
    }
    Ok(MergeCard { pr: number, head: pr.head, base: pr.base, ci })
}

fn probe_merge(runner: &mut impl Runner, t: &Target, number: u64) -> Seen<BTreeMap<String, String>> {
    match view_pr(runner, t, number) {
        Ok((pr, _)) if pr.state == PrState::Merged => Seen::Yes(evidence(&[("pr", &number.to_string()), ("via", "reconcile")])),
        Ok(_) => Seen::No,
        Err(_) => Seen::Unsure,
    }
}

/// Marks the PR ready and merges it, only with the person's yes for this card, green or absent checks and an
/// unmoved head. Merge commit, guarded by the head; the branch is never deleted.
pub fn merge(
    runner: &mut impl Runner,
    ledger: &mut impl Ledger,
    t: &Target,
    card: &MergeCard,
    yes: Option<&OwnerYes>,
) -> Result<(), RemoteError> {
    check(t)?;
    authority(ledger, t, Effect::DraftPr)?;
    yes.filter(|y| y.pr == card.pr && y.head == card.head).ok_or(RemoteError::NeedsYes)?;
    if card.head != t.head {
        return Err(RemoteError::HeadMoved);
    }
    settled(ledger)?;
    let (pr, now) = view_pr(runner, t, card.pr)?;
    if pr.state != PrState::Open {
        return Err(RemoteError::NotOpen);
    }
    if pr.head != card.head || pr.base != card.base {
        return Err(RemoteError::HeadMoved);
    }
    if !matches!(now.state, CiState::Passing | CiState::None) {
        return Err(RemoteError::Ci);
    }
    let number = card.pr.to_string();
    let id = format!("merge-{}-{}-{}", card.pr, short(t), t.attempt);
    let expected = evidence(&[("repo", &t.repo), ("pr", &number), ("head", &card.head), ("base", &card.base)]);
    ledger.intent(&id, Step::Merge, expected.clone())?;
    if let Err(e) = authority(ledger, t, Effect::DraftPr) {
        ledger.receipt(&id, Outcome::Failed, expected, Some(e.key()))?;
        return Err(e);
    }
    let mut clean = true;
    if pr.draft {
        clean = matches!(runner.run(&gh(&t.cwd, &["pr", "ready", &number, "--repo", &t.repo])), Ok(r) if r.code == 0);
    }
    if clean {
        let sent = runner.run(&gh(&t.cwd, &["pr", "merge", &number, "--repo", &t.repo, "--merge", "--match-head-commit", &card.head]));
        if let Err(Lost::Missing) = sent {
            ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::GhMissing.key()))?;
            return Err(RemoteError::GhMissing);
        }
        if matches!(&sent, Ok(r) if r.code == 0) {
            ledger.receipt(&id, Outcome::Done, evidence(&[("pr", &number), ("via", "merge")]), None)?;
            return Ok(());
        }
    }
    match probe_merge(runner, t, card.pr) {
        Seen::Yes(found) => {
            ledger.receipt(&id, Outcome::Done, found, None)?;
            Ok(())
        }
        Seen::No => {
            ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::MergeFailed.key()))?;
            Err(RemoteError::MergeFailed)
        }
        Seen::Unsure => {
            ledger.receipt(&id, Outcome::Unknown, expected, Some(RemoteError::Maybe.key()))?;
            Err(RemoteError::Maybe)
        }
    }
}

// Recovery

/// Settles every open remote intent by reading the live remote state. Nothing is ever run again. `None` means the
/// remote could not be read and the intent stays open.
pub fn reconcile(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target) -> Result<Vec<(String, Option<Outcome>)>, RemoteError> {
    check(t)?;
    let mut out = vec![];
    for open in ledger.open()? {
        let seen = match open.step {
            Step::Push => probe_push(runner, t, &open.expected),
            Step::Pr => match probe_pr(runner, t) {
                Seen::Yes(pr) => Seen::Yes(evidence(&[("number", &pr.number.to_string()), ("url", &pr.url), ("via", "reconcile")])),
                Seen::No => Seen::No,
                Seen::Unsure => Seen::Unsure,
            },
            Step::Merge => match open.expected.get("pr").and_then(|n| n.parse().ok()) {
                Some(number) => probe_merge(runner, t, number),
                None => Seen::Unsure,
            },
        };
        let outcome = match seen {
            Seen::Yes(found) => {
                ledger.settle(&open.id, Outcome::Done, found)?;
                Some(Outcome::Done)
            }
            Seen::No => {
                ledger.settle(&open.id, Outcome::Failed, open.expected.clone())?;
                Some(Outcome::Failed)
            }
            Seen::Unsure => None,
        };
        out.push((open.id, outcome));
    }
    Ok(out)
}

// The whole publication

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub account: Account,
    pub pushed: Pushed,
    pub pr: Pr,
}

/// What the runner calls once a commit is proven and landed locally: gh check, push, one draft PR. A failure leaves the
/// local commit intact; the caller shows the remote receipt as pending or failed, never as Done.
pub fn publish(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target, spec: &PrSpec) -> Result<Published, RemoteError> {
    check(t)?;
    authority(ledger, t, Effect::Push)?;
    let account = preflight(runner, &t.cwd)?;
    if account.repo != t.repo {
        return Err(RemoteError::Url);
    }
    let pushed = push(runner, ledger, t)?;
    let pr = open_pr(runner, ledger, t, spec)?;
    Ok(Published { account, pushed, pr })
}

// The real tools

/// Runs the person's own git and gh, shell-free, hidden, never prompting.
pub struct Process {
    git: PathBuf,
    gh: PathBuf,
}

const OUTPUT_CAP: usize = 16 << 20;
const TIMEOUT: Duration = Duration::from_secs(180);
/// Variables that would aim git at another repository than the one the call names.
const REPO_VARIABLES: [&str; 8] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

fn native(name: &str) -> PathBuf {
    let Some(cli) = crate::cli::find_cli(name) else { return PathBuf::new() };
    let path = PathBuf::from(&cli.command);
    if !cli.prefix.is_empty() || !path.is_absolute() || (cfg!(windows) && !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
    {
        return PathBuf::new();
    }
    path
}

impl Process {
    /// A missing tool is not an error here: its calls answer [`Lost::Missing`].
    pub fn find() -> Process {
        Process { git: native("git"), gh: native("gh") }
    }

    pub fn at(git: PathBuf, gh: PathBuf) -> Process {
        Process { git, gh }
    }

    /// The command for one call. The environment is the person's own, so their credential helper and gh login work,
    /// except for what would point git elsewhere or ask a question.
    pub fn command(&self, call: &Call) -> Command {
        let program = match call.tool {
            Tool::Git => &self.git,
            Tool::Gh => &self.gh,
        };
        let mut command = Command::new(program);
        command.args(&call.args).current_dir(&call.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        for name in REPO_VARIABLES {
            command.env_remove(name);
        }
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("SSH_ASKPASS_REQUIRE", "never")
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_SPINNER_DISABLED", "1")
            .env("NO_COLOR", "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command
    }
}

impl Runner for Process {
    fn run(&mut self, call: &Call) -> Result<Reply, Lost> {
        let program = if call.tool == Tool::Git { &self.git } else { &self.gh };
        if program.as_os_str().is_empty() {
            return Err(Lost::Missing);
        }
        let mut child = self.command(call).spawn().map_err(|_| Lost::Missing)?;
        let Some(mut stdout) = child.stdout.take() else { return Err(Lost::Interrupted) };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut kept, mut over, mut chunk) = (Vec::new(), false, [0u8; 8192]);
            while let Ok(n) = stdout.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if kept.len() + n > OUTPUT_CAP {
                    over = true;
                } else {
                    kept.extend_from_slice(&chunk[..n]);
                }
            }
            let _ = tx.send((kept, over));
        });
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                // ponytail: kills the tool, not a helper it started; a push that outlives this is settled by reading the remote.
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Lost::Interrupted);
                }
            }
        };
        let (kept, over) = rx.recv_timeout(Duration::from_secs(5)).map_err(|_| Lost::Interrupted)?;
        // Output beyond the cap cannot be scanned or trusted: report it as a failure, never as a partial success.
        let code = if over { 125 } else { status.code().unwrap_or(-1) };
        Ok(Reply { code, stdout: String::from_utf8_lossy(&kept).into_owned() })
    }
}
