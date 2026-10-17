//! Equilibrado's remote effects: a non-forced push of the mission's own branch, one draft pull request, the PR's check
//! results and a merge that only happens after the person's explicit yes.
//!
//! git and gh run shell-free through [`Runner`], with the person's own git credential helper and gh login; Faber
//! never reads a token or a credential file. Every effect writes its intent before and its receipt after through
//! [`Ledger`]. A call that may have reached the remote but gave no clean answer is settled by reading the live remote
//! state, never by running it again; when that read fails too, or the call was killed, the receipt stays `Unknown` and
//! nothing else may leave until [`reconcile`] settles it. Errors are `reason.*` keys.
//!
//! The repository is not trusted: a worker can write its config, its refs and its attributes. So every command that
//! talks to the network either runs outside the repository against the granted URL itself (reads), or runs after the
//! repository's own config, in every scope but the person's, was found free of anything that runs a command or
//! reroutes a connection (the push).

use super::git::valid_oid;
use super::proof::scan_secrets;
use super::{DecidedBy, Decision, DecisionKind, Effect, Extra, Grant, Outcome, RepoIdentity};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
    /// It was started and gave no answer (a timeout, or output that never ended): it may or may not have taken effect,
    /// and nothing it started can be assumed gone, so it may still land. A tool that exits with an error is an
    /// answer, not this.
    Interrupted,
    /// It was killed, and everything it started is proven gone: it cannot take effect any more.
    Killed,
}

pub trait Runner {
    fn run(&mut self, call: &Call) -> Result<Reply, Lost>;

    /// A folder of Faber's own where calls that must not see any repository run: empty, private, and under no
    /// repository. Never a shared folder such as the temp folder, which anything can turn into a repository.
    fn neutral(&self) -> PathBuf;

    /// Whether a file or a link, even a dangling one, is there. Only the layout check reads the disk, through this.
    fn exists(&self, path: &Path) -> bool {
        path.symlink_metadata().is_ok()
    }
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
    /// What the `Unknown` receipt recorded (empty when the receipt was never written). `tree` says whether everything
    /// the call started was proven gone.
    pub evidence: BTreeMap<String, String>,
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
    /// Grafts, a shallow file or alternates change what git walks or reads, so what would leave cannot be proven.
    Layout,
    /// The repository has a second remote. Only the granted one is allowed, so the person sees why instead of a config error.
    OtherRemote,
    /// The remote no longer leads only to the URL the person granted.
    Url,
    Secret,
    /// What would leave is too big to check, so it does not leave.
    Oversized,
    Unreachable,
    Diverged,
    Rejected,
    /// It may or may not have happened and the remote could not be read. The receipt is `Unknown`.
    Maybe,
    NotPushed,
    HeadMoved,
    /// A PR that is not the mission's own branch in this repository.
    Foreign,
    /// gh acts as another account than the one the person granted.
    Account,
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
            Self::Layout => "reason.remote.layout",
            Self::OtherRemote => "reason.remote.otherRemote",
            Self::Url => "reason.remote.url",
            Self::Secret => "reason.remote.secret",
            Self::Oversized => "reason.remote.oversized",
            Self::Unreachable => "reason.remote.unreachable",
            Self::Diverged => "reason.remote.diverged",
            Self::Rejected => "reason.remote.rejected",
            Self::Maybe => "reason.remote.maybe",
            Self::NotPushed => "reason.remote.notPushed",
            Self::HeadMoved => "reason.remote.headMoved",
            Self::Foreign => "reason.remote.foreign",
            Self::Account => "reason.remote.account",
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
    /// The checkout root of the repository. Only local reads and the push run here.
    pub cwd: PathBuf,
    pub identity: RepoIdentity,
    /// The grant revision the work was admitted under.
    pub revision: u64,
    /// `owner/name`, from [`preflight`] and shown at the grant.
    pub repo: String,
    /// The git remote name the grant names.
    pub remote: String,
    /// That remote's push URL as shown at the grant. Every network call goes to this exact URL, not to the name.
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
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
        && name.split('/').all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}

fn name_ok(name: &str) -> bool {
    (1..=100).contains(&name.len()) && name != "." && name != ".." && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
}

fn slug_ok(repo: &str) -> bool {
    repo.split_once('/').is_some_and(|(owner, name)| name_ok(owner) && name_ok(name))
}

/// An https or ssh URL (or the scp form) that names `repo`, so a fork or another host cannot stand in for it.
fn url_shape(url: &str) -> bool {
    let known =
        url.starts_with("https://") || url.starts_with("ssh://") || (!url.contains("://") && url.contains('@') && url.contains(':'));
    known && url.len() <= 512 && !url.chars().any(|c| c.is_control() || c.is_whitespace()) && !url.starts_with('-')
}

fn url_ok(url: &str, repo: &str) -> bool {
    let path = url.strip_suffix(".git").unwrap_or(url).to_ascii_lowercase();
    let repo = repo.to_ascii_lowercase();
    url_shape(url) && (path.ends_with(&format!("/{repo}")) || path.ends_with(&format!(":{repo}")))
}

fn check(t: &Target) -> Result<(), RemoteError> {
    let ok = t.branch.starts_with("faber/")
        && ref_ok(&t.branch)
        && ref_ok(&t.base)
        && valid_oid(&t.head)
        && name_ok(&t.remote)
        && !t.remote.starts_with(['-', '.'])
        && slug_ok(&t.repo)
        && url_ok(&t.url, &t.repo);
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

/// The account gh acts as must be the one the person granted.
fn account_ok(ledger: &mut impl Ledger, login: &str) -> Result<(), RemoteError> {
    let grant = ledger.grant()?.ok_or(RemoteError::Grant("reason.grant.none"))?;
    match grant.remote_account.as_deref() {
        Some(granted) if granted.eq_ignore_ascii_case(login) => Ok(()),
        Some(_) => Err(RemoteError::Account),
        None => Err(RemoteError::Grant("reason.grant.noAccount")),
    }
}

fn acting(runner: &mut impl Runner, ledger: &mut impl Ledger) -> Result<(), RemoteError> {
    let cwd = runner.neutral();
    let login = login(runner, &cwd)?;
    account_ok(ledger, &login)
}

fn settled(ledger: &mut impl Ledger) -> Result<(), RemoteError> {
    if ledger.open()?.is_empty() { Ok(()) } else { Err(RemoteError::Pending) }
}

/// Every option that changes what git prints, or what it runs, is set here on the command line, which beats the
/// repository's own config: a worker can write that config, and a scan that trusts it can be made blind.
const GIT_OPTIONS: [&str; 17] = [
    "core.pager=",
    "core.fsmonitor=false",
    "diff.external=",
    "protocol.ext.allow=never",
    // Every transport is off unless `git_net` turns on the one the granted URL uses.
    "protocol.allow=never",
    "submodule.recurse=false",
    "push.recurseSubmodules=no",
    "push.followTags=false",
    "push.gpgSign=false",
    "core.attributesFile=",
    "log.showRoot=true",
    "log.diffMerges=separate",
    "i18n.logOutputEncoding=UTF-8",
    "diff.renames=false",
    "color.ui=never",
    // A signature shown by log runs the program the config names for it.
    "log.showSignature=false",
    // A bare repository in the working folder would be adopted as the repository.
    "safe.bareRepository=explicit",
];

/// A git call with every safety option of this module on its command line. Public so the real-git scenario runs the
/// very same calls.
pub fn git(cwd: &Path, args: &[&str]) -> Call {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    // Replacement refs would show the scan other objects than the ones the push packs.
    let mut all = vec!["--no-replace-objects".to_string(), "-c".to_string(), format!("core.hooksPath={null}")];
    for option in GIT_OPTIONS {
        all.extend(["-c".into(), option.into()]);
    }
    all.extend(args.iter().map(|s| (*s).to_string()));
    Call { tool: Tool::Git, cwd: cwd.into(), args: all }
}

/// The transport a URL needs: only https and ssh are ever granted, `file` is for a local path a test names, and
/// everything else (`ext::`, `fd::`, `git://`, plain `http://`, an option) is none.
pub fn scheme_of(url: &str) -> &'static str {
    let bytes = url.as_bytes();
    let drive = bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\');
    if url.starts_with("https://") {
        "https"
    } else if url.starts_with("ssh://") {
        "ssh"
    } else if url.starts_with("file://") || url.starts_with('/') || drive {
        "file"
    } else if !url.contains("://") && !url.contains("::") && url.contains('@') && url.contains(':') && !url.starts_with('-') {
        "ssh"
    } else {
        "none"
    }
}

/// A git call that talks to `url`. It turns on that one transport, over `protocol.allow=never`; the process gets the
/// same single transport in `GIT_ALLOW_PROTOCOL`, which would otherwise beat every `protocol.*.allow` in config.
pub fn git_net(cwd: &Path, url: &str, args: &[&str]) -> Call {
    let mut call = git(cwd, args);
    let scheme = scheme_of(url);
    if scheme != "none" {
        let pin = ["-c".to_string(), format!("protocol.{scheme}.allow=always")];
        call.args.splice(0..0, pin);
    }
    call
}

/// The transport a git call turned on with [`git_net`], or `none`.
pub fn allowed_protocol(args: &[String]) -> &str {
    args.windows(2)
        .filter(|pair| pair[0] == "-c")
        .find_map(|pair| pair[1].strip_prefix("protocol.")?.strip_suffix(".allow=always"))
        .unwrap_or("none")
}

fn gh(cwd: &Path, args: &[&str]) -> Call {
    Call { tool: Tool::Gh, cwd: cwd.into(), args: args.iter().map(|s| (*s).to_string()).collect() }
}

/// A receipt of an effect that may have landed. `gone` says everything the call started is proven dead, which is the
/// only case where a later read that finds nothing may settle it as failed.
fn unknown(expected: &BTreeMap<String, String>, gone: bool) -> BTreeMap<String, String> {
    let mut evidence = expected.clone();
    evidence.insert("tree".into(), if gone { "gone" } else { "unproven" }.into());
    evidence
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
        Lost::Interrupted | Lost::Killed => RemoteError::Unreachable,
    })
}

// Preflight

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    /// `owner/name` of the repository gh resolves from the remote URL.
    pub repo: String,
}

fn login(runner: &mut impl Runner, cwd: &Path) -> Result<String, RemoteError> {
    let reply = ask(runner, &gh(cwd, &["api", "user", "--jq", ".login"]))?;
    let login = reply.stdout.trim().to_string();
    if reply.code != 0 || login.is_empty() || !name_ok(&login) || login.len() > 39 {
        return Err(RemoteError::Command);
    }
    Ok(login)
}

/// Whether gh is installed and signed in, and who and where it would act. Shown on the Equilibrado start card.
/// It runs outside the checkout and resolves the repository from the URL, so the checkout's own config never applies.
pub fn preflight(runner: &mut impl Runner, url: &str) -> Result<Account, RemoteError> {
    if !url_shape(url) {
        return Err(RemoteError::Target);
    }
    let cwd = runner.neutral();
    if ask(runner, &gh(&cwd, &["auth", "status"]))?.code != 0 {
        return Err(RemoteError::GhLoggedOut);
    }
    let login = login(runner, &cwd)?;
    let repo = ask(runner, &gh(&cwd, &["repo", "view", url, "--json", "nameWithOwner", "--jq", ".nameWithOwner"]))?;
    let repo = repo.stdout.trim().to_string();
    if !slug_ok(&repo) || repo.len() > 201 {
        return Err(RemoteError::Command);
    }
    Ok(Account { login, repo })
}

// Repository config

/// The only keys the repository (or its worktree) may carry. A worker can write that config, and every key outside
/// this list is either unneeded by a push or can run a command, reroute the connection or change what the scan sees;
/// a list of the bad keys was missed three times, so this is the list of the good ones. Section and variable names are
/// case-insensitive, as in git; a subsection (a remote's or a branch's name) is compared exactly, as git does, and
/// only the granted remote's is listed: another one is another remote ([`other_remote`] gives it its own refusal).
/// Any branch's `remote`, `merge` and `vscode-merge-base` are listed, because a clone keeps one pair per tracked branch
/// and a mission worktree reads that shared config; [`inert_branch_value`] judges their values.
pub fn allowed_key(key: &str, remote: &str) -> bool {
    let Some((section, rest)) = key.split_once('.') else { return false };
    let (middle, variable) = match rest.rsplit_once('.') {
        Some((middle, variable)) => (Some(middle), variable),
        None => (None, rest),
    };
    let (section, variable) = (section.to_ascii_lowercase(), variable.to_ascii_lowercase());
    match (section.as_str(), middle) {
        // The remote's address and refspec; the push goes to the granted URL itself, and `pin_url` checks the name.
        ("remote", Some(name)) => name == remote && matches!(variable.as_str(), "url" | "fetch"),
        // Which remote and branch a branch follows; `vscode-merge-base` is an editor's note about it. Safe whatever the
        // branch: every network call names the granted URL and an explicit refspec, and the push is not run on a
        // branch's behalf (no `push.default`, `pushRemote` or `rebase`, which stay refused), so these keys can never
        // change where or what is pushed. Only their values are checked, in `guard_branches`.
        ("branch", Some(_)) => matches!(variable.as_str(), "remote" | "merge" | "vscode-merge-base"),
        (_, Some(_)) => false,
        (_, None) => matches!(
            format!("{section}.{variable}").as_str(),
            // What `git init` writes, and the platform switches it adds: they only describe the checkout.
            "core.repositoryformatversion"
                | "core.filemode"
                | "core.bare"
                | "core.logallrefupdates"
                | "core.symlinks"
                | "core.ignorecase"
                | "core.precomposeunicode"
                | "core.longpaths"
                // Line endings of a checkout; the scan and the push read objects as they are.
                | "core.autocrlf"
                // The layout the repository needs git to understand to open it at all.
                | "extensions.worktreeconfig"
                | "extensions.objectformat"
                // Who the commits are by.
                | "user.name"
                | "user.email"
        ),
    }
}

/// A remote other than the granted one, in any of its keys. The owner's repository has only one today; a second one
/// is refused with its own reason, never accepted silently.
pub fn other_remote(key: &str, remote: &str) -> bool {
    let Some((section, rest)) = key.split_once('.') else { return false };
    section.eq_ignore_ascii_case("remote") && rest.rsplit_once('.').is_some_and(|(name, _)| name != remote)
}

/// The value of a branch's `remote`, `merge` or `vscode-merge-base` does nothing: `remote` is the granted remote or
/// `.` (this repository), `merge` is a branch name under `refs/heads/`, and no value has a control character.
pub fn inert_branch_value(variable: &str, value: &str, remote: &str) -> bool {
    if value.len() > 1024 || value.chars().any(char::is_control) {
        return false;
    }
    match variable.to_ascii_lowercase().as_str() {
        "remote" => value == remote || value == ".",
        "merge" => value.strip_prefix("refs/heads/").is_some_and(|name| !name.is_empty()),
        "vscode-merge-base" => !value.is_empty(),
        _ => false,
    }
}

/// Keys Faber itself sets on the command line, which git lists as `command` scope.
fn pinned_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("core.hookspath")
        || GIT_OPTIONS.iter().any(|option| option.split('=').next().is_some_and(|name| name.eq_ignore_ascii_case(key)))
}

/// Nothing but [`allowed_key`] may come from the repository or its worktree, and the command line is Faber's own pins:
/// the environment's `-c` equivalents are cleared from every call. That covers includes too: `--list` follows them and
/// reports each key under the including scope, and `include.path` is not listed. System and global scopes are the
/// person's own: their credential helper and URL rewrites are what they want.
pub fn guard_config(runner: &mut impl Runner, t: &Target) -> Result<(), RemoteError> {
    let reply = ask(runner, &git(&t.cwd, &["config", "--list", "--show-scope", "--name-only"]))?;
    if reply.code != 0 {
        return Err(RemoteError::Command);
    }
    let mut branches = false;
    for line in reply.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let Some((scope, key)) = line.split_once('\t') else { return Err(RemoteError::Config) };
        match scope {
            "system" | "global" => {}
            "local" | "worktree" if other_remote(key, &t.remote) => return Err(RemoteError::OtherRemote),
            "local" | "worktree" if allowed_key(key, &t.remote) => branches |= key.to_ascii_lowercase().starts_with("branch."),
            "command" if pinned_key(key) => {}
            _ => return Err(RemoteError::Config),
        }
    }
    if branches { guard_branches(runner, t) } else { Ok(()) }
}

/// The values of the branch keys the repository carries, all in one read: the names alone were judged above. `-z`
/// keeps a value with a line break in one record (`scope NUL key LF value NUL`), so it cannot pass as two.
fn guard_branches(runner: &mut impl Runner, t: &Target) -> Result<(), RemoteError> {
    let reply = ask(runner, &git(&t.cwd, &["config", "--show-scope", "-z", "--get-regexp", "^branch[.]"]))?;
    if reply.code != 0 {
        return Err(RemoteError::Command);
    }
    // `scope NUL key LF value NUL`, repeated: with `-z` git separates the scope from the key with a NUL, not a tab.
    let mut fields = reply.stdout.split('\0');
    while let Some(scope) = fields.next().filter(|f| !f.is_empty()) {
        let Some(record) = fields.next() else { return Err(RemoteError::Config) };
        let Some((key, value)) = record.split_once('\n') else { return Err(RemoteError::Config) };
        if !matches!(scope, "local" | "worktree") {
            continue;
        }
        let variable = key.rsplit_once('.').map_or("", |(_, variable)| variable);
        if !allowed_key(key, &t.remote) || !inert_branch_value(variable, value, &t.remote) {
            return Err(RemoteError::Config);
        }
    }
    Ok(())
}

/// Files that change what git walks or which objects it reads, so what the scan prints is not what would leave.
const LAYOUT_FILES: [&str; 4] = ["info/grafts", "shallow", "objects/info/alternates", "objects/info/http-alternates"];

/// The repository has no grafts, no shallow file and no alternates. Git says where it would read each, so a linked
/// worktree or a moved git dir is followed. Refused if any exists, even as a dangling link.
pub fn guard_layout(runner: &mut impl Runner, t: &Target) -> Result<(), RemoteError> {
    let mut args = vec!["rev-parse", "--path-format=absolute"];
    for file in LAYOUT_FILES {
        args.extend(["--git-path", file]);
    }
    let reply = ask(runner, &git(&t.cwd, &args))?;
    let paths: Vec<&str> = reply.stdout.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if reply.code != 0 || paths.len() != LAYOUT_FILES.len() || !paths.iter().all(|p| Path::new(p).is_absolute()) {
        return Err(RemoteError::Command);
    }
    if paths.iter().any(|path| runner.exists(Path::new(path))) { Err(RemoteError::Layout) } else { Ok(()) }
}

/// The remote name must still lead to exactly the URL the person granted, and to no other.
fn pin_url(runner: &mut impl Runner, t: &Target) -> Result<(), RemoteError> {
    let reply = ask(runner, &git(&t.cwd, &["remote", "get-url", "--push", "--all", &t.remote]))?;
    let urls: Vec<&str> = reply.stdout.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if reply.code == 0 && urls == [t.url.as_str()] { Ok(()) } else { Err(RemoteError::Url) }
}

// The live remote

/// What the remote advertises right now. Read outside the repository, from the granted URL itself.
struct Live {
    branch: Option<String>,
    /// Every branch and tag head, whatever the repository's own tracking refs say.
    oids: Vec<String>,
}

fn live(runner: &mut impl Runner, t: &Target) -> Result<Live, RemoteError> {
    let cwd = runner.neutral();
    let reply = ask(runner, &git_net(&cwd, &t.url, &["ls-remote", "--heads", "--tags", &t.url]))?;
    if reply.code != 0 {
        return Err(RemoteError::Unreachable);
    }
    let full = format!("refs/heads/{}", t.branch);
    let (mut branch, mut oids) = (None, vec![]);
    for line in reply.stdout.lines().filter(|l| !l.trim().is_empty()) {
        let Some((oid, name)) = line.split_once('\t') else { return Err(RemoteError::Command) };
        if !valid_oid(oid) {
            return Err(RemoteError::Command);
        }
        if name == full {
            branch = Some(oid.to_string());
        }
        oids.push(oid.to_string());
    }
    oids.sort();
    oids.dedup();
    Ok(Live { branch, oids })
}

fn remote_branch(runner: &mut impl Runner, t: &Target) -> Result<Option<String>, RemoteError> {
    live(runner, t).map(|l| l.branch)
}

/// The most live heads excluded from the scan. Past it more is scanned, never less.
const EXCLUDE_CAP: usize = 200;

/// Everything that would leave, as patches: commits the live remote does not have, in full. Binary files are read as
/// text, merge commits are shown against every parent, and commit messages are included. Fails closed on any error.
fn scan_outgoing(runner: &mut impl Runner, t: &Target, live: &Live) -> Result<(), RemoteError> {
    let reply = ask(runner, &scan_call(&t.cwd, &t.head, &live.oids))?;
    match reply.code {
        0 => {}
        OVER => return Err(RemoteError::Oversized),
        _ => return Err(RemoteError::Command),
    }
    // A secret in UTF-16 has a zero byte between its letters.
    let squeezed = reply.stdout.replace('\0', "");
    scan_secrets(&reply.stdout).and_then(|()| scan_secrets(&squeezed)).map_err(|_| RemoteError::Secret)
}

/// The call that prints everything leaving: public so the real-git scenario runs the very same one.
pub fn scan_call(cwd: &Path, head: &str, live: &[String]) -> Call {
    let mut args = vec![
        "log",
        "--ignore-missing",
        "-p",
        "--text",
        "--diff-merges=separate",
        "--encoding=UTF-8",
        "--no-color",
        "--no-renames",
        "--no-ext-diff",
        "--no-textconv",
        "--no-show-signature",
        "--format=%B",
        head,
        "--not",
    ];
    args.extend(live.iter().take(EXCLUDE_CAP).map(String::as_str));
    git(cwd, &args)
}

/// The push itself: the exact commit to the own branch of the URL, never forced, never through a hook.
pub fn push_call(cwd: &Path, url: &str, head: &str, full_ref: &str) -> Call {
    git_net(cwd, url, &["push", "--no-verify", "--porcelain", url, &format!("{head}:{full_ref}")])
}

/// The exit code a reply gets when its output went past the cap.
const OVER: i32 = 125;

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
    guard_layout(runner, t)?;
    pin_url(runner, t)?;
    let now = live(runner, t)?;
    scan_outgoing(runner, t, &now)?;
    let old = now.branch.clone();
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
    let sent = runner.run(&push_call(&t.cwd, &t.url, &t.head, &full));
    let pushed = || Pushed { head: t.head.clone(), already: false };
    match sent {
        Ok(reply) if reply.code == 0 => {
            ledger.receipt(&id, Outcome::Done, evidence(&[("new", &t.head), ("via", "push")]), None)?;
            Ok(pushed())
        }
        Err(Lost::Missing) => {
            ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::GitMissing.key()))?;
            Err(RemoteError::GitMissing)
        }
        // Killed: what it started may still be running or may land later. Not read now; `reconcile` settles it, and
        // only a tree proven gone may ever settle as failed.
        Err(lost) => {
            ledger.receipt(&id, Outcome::Unknown, unknown(&expected, lost == Lost::Killed), Some(RemoteError::Maybe.key()))?;
            Err(RemoteError::Maybe)
        }
        Ok(_) => match probe_push(runner, t, &expected) {
            Seen::Yes(found) => {
                ledger.receipt(&id, Outcome::Done, found, None)?;
                Ok(pushed())
            }
            Seen::No => {
                ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::Rejected.key()))?;
                Err(RemoteError::Rejected)
            }
            Seen::Unsure => {
                ledger.receipt(&id, Outcome::Unknown, unknown(&expected, true), Some(RemoteError::Maybe.key()))?;
                Err(RemoteError::Maybe)
            }
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pushed {
    pub head: String,
    /// The remote already had the proven commit; nothing was sent.
    pub already: bool,
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
    /// The branch name the PR comes from.
    pub branch: String,
    /// The PR comes from another repository (a fork).
    pub cross: bool,
    pub base: String,
    /// Found rather than created by this call.
    pub existing: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrSpec {
    pub title: String,
    pub body: String,
}

const PR_FIELDS: &str = "number,url,isDraft,state,headRefName,headRefOid,baseRefName,isCrossRepository";
const PR_VIEW_FIELDS: &str = "number,url,isDraft,state,headRefName,headRefOid,baseRefName,isCrossRepository,statusCheckRollup";

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
        branch: text("headRefName")?,
        cross: value["isCrossRepository"].as_bool().ok_or(RemoteError::Command)?,
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

/// Every PR (any state) that comes from the own branch of this very repository. gh filters by branch name alone, so a
/// fork's PR of the same name is dropped here.
fn list_prs(runner: &mut impl Runner, t: &Target) -> Result<Vec<Pr>, RemoteError> {
    let cwd = runner.neutral();
    let call = gh(&cwd, &["pr", "list", "--repo", &t.repo, "--head", &t.branch, "--state", "all", "--limit", "20", "--json", PR_FIELDS]);
    let reply = ask(runner, &call)?;
    let all: Vec<Pr> = json(&reply)?.as_array().ok_or(RemoteError::Command)?.iter().map(parse_pr).collect::<Result<_, _>>()?;
    Ok(all.into_iter().filter(|pr| !pr.cross && pr.branch == t.branch).collect())
}

/// Whether the PR a lost `create` should have made is there. A PR of the branch that is not this commit and base is
/// not proof of anything.
fn probe_pr(runner: &mut impl Runner, t: &Target, expected: &BTreeMap<String, String>) -> Seen<Pr> {
    let (head, base) = (expected.get("head").map_or("", String::as_str), expected.get("base").map_or("", String::as_str));
    match list_prs(runner, t) {
        Ok(found) if found.is_empty() => Seen::No,
        Ok(found) => found.into_iter().find(|pr| pr.head == head && pr.base == base).map_or(Seen::Unsure, Seen::Yes),
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

/// Opens the one draft PR of the own branch. An existing PR of that branch in this repository, in any state, is
/// returned instead of a new one. The proven commit must already be on the remote.
pub fn open_pr(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target, spec: &PrSpec) -> Result<Pr, RemoteError> {
    check(t)?;
    if !pr_text_ok(spec) {
        return Err(RemoteError::Target);
    }
    authority(ledger, t, Effect::DraftPr)?;
    settled(ledger)?;
    scan_secrets(&format!("{}\n{}", spec.title, spec.body)).map_err(|_| RemoteError::Secret)?;
    acting(runner, ledger)?;
    if let Some(found) = list_prs(runner, t)?.into_iter().next() {
        if found.base != t.base {
            return Err(RemoteError::HeadMoved);
        }
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
    let cwd = runner.neutral();
    let created =
        runner.run(&gh(&cwd, &["pr", "create", "--draft", "--repo", &t.repo, "--head", &t.branch, "--base", &t.base, &title, &body]));
    let clean = matches!(&created, Ok(reply) if reply.code == 0);
    match created {
        Err(Lost::Missing) => {
            ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::GhMissing.key()))?;
            return Err(RemoteError::GhMissing);
        }
        Err(lost) => {
            ledger.receipt(&id, Outcome::Unknown, unknown(&expected, lost == Lost::Killed), Some(RemoteError::Maybe.key()))?;
            return Err(RemoteError::Maybe);
        }
        Ok(_) => {}
    }
    // The PR list is read back either way, so the receipt always carries the number and a lost answer cannot duplicate.
    match probe_pr(runner, t, &expected) {
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
            ledger.receipt(&id, Outcome::Unknown, unknown(&expected, true), Some(RemoteError::Maybe.key()))?;
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

/// The live PR, which must be the mission's own branch of this repository, and its checks.
fn view_pr(runner: &mut impl Runner, t: &Target, number: u64) -> Result<(Pr, Ci), RemoteError> {
    let number = number.to_string();
    let cwd = runner.neutral();
    let reply = ask(runner, &gh(&cwd, &["pr", "view", &number, "--repo", &t.repo, "--json", PR_VIEW_FIELDS]))?;
    let value = json(&reply)?;
    let pr = parse_pr(&value)?;
    if pr.cross || pr.branch != t.branch {
        return Err(RemoteError::Foreign);
    }
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

// Following a pull request

/// A pull request as a project conversation follows it (`crate::follow`): any repository the person's gh can read, not
/// a mission's own branch, and only read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Look {
    pub state: PrState,
    pub head: String,
    pub ci: Ci,
    /// GitHub found a merge conflict with the base branch.
    pub conflict: bool,
    /// Each failed check, with the link and the short words GitHub gave it.
    pub failures: Vec<Failure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub name: String,
    pub link: Option<String>,
    pub said: Option<String>,
}

const LOOK_FIELDS: &str = "number,url,isDraft,state,headRefName,headRefOid,baseRefName,isCrossRepository,statusCheckRollup,mergeable";

/// One short printable line: what a check said about itself, never a block of its log.
fn one_line(text: Option<&str>, cap: usize) -> Option<String> {
    let text = text?.split_whitespace().collect::<Vec<_>>().join(" ");
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    (!text.is_empty()).then(|| if text.chars().count() > cap { text.chars().take(cap).collect::<String>() + "…" } else { text })
}

fn failure(check: &Value) -> Option<Failure> {
    let (state, name) = classify(check);
    if state != CiState::Failing {
        return None;
    }
    let link = check["detailsUrl"]
        .as_str()
        .or(check["targetUrl"].as_str())
        .filter(|l| l.starts_with("https://") && l.len() <= 300 && !l.chars().any(|c| c.is_whitespace() || c.is_control()));
    let said = one_line(check["description"].as_str().or(check["title"].as_str()), 160);
    Some(Failure { name: one_line(Some(&name), 80).unwrap_or_else(|| "check".into()), link: link.map(str::to_string), said })
}

/// Reads one pull request and its checks with gh, outside every repository, as [`preflight`] does.
pub fn look(runner: &mut impl Runner, repo: &str, number: u64) -> Result<Look, RemoteError> {
    if !slug_ok(repo) || repo.len() > 201 || number == 0 {
        return Err(RemoteError::Target);
    }
    let cwd = runner.neutral();
    let n = number.to_string();
    let reply = ask(runner, &gh(&cwd, &["pr", "view", &n, "--repo", repo, "--json", LOOK_FIELDS]))?;
    let value = json(&reply)?;
    let pr = parse_pr(&value)?;
    if pr.number != number {
        return Err(RemoteError::Command);
    }
    let checks = value["statusCheckRollup"].as_array().map_or(&[][..], Vec::as_slice);
    let ci = summarize(checks, &pr.head);
    let failures = checks.iter().filter_map(failure).take(8).collect();
    Ok(Look { state: pr.state, head: pr.head, ci, conflict: value["mergeable"].as_str() == Some("CONFLICTING"), failures })
}

// Merge

/// What the person is asked: this PR of this repository, at this commit, into this base, with these checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeCard {
    pub repo: String,
    pub pr: u64,
    pub head: String,
    pub base: String,
    pub ci: Ci,
}

impl MergeCard {
    fn binding(&self) -> Value {
        json!({ "repo": self.repo, "pr": self.pr, "head": self.head, "base": self.base })
    }

    /// Names the exact repository, PR number, full head and base, so a yes for one cannot be found under another.
    pub fn decision_id(&self) -> String {
        let digest = Sha256::digest(self.binding().to_string().as_bytes());
        let hex: String = digest.iter().take(20).map(|b| format!("{b:02x}")).collect();
        format!("merge-{}-{hex}", self.pr)
    }

    /// The decision to put on the board. Only the person can answer it: it is a direction decision. It carries the
    /// binding in full, and [`OwnerYes::from_decision`] compares all of it.
    pub fn decision(&self) -> Decision {
        let mut decision = Decision::new(&self.decision_id(), "lead", "decision.remote.merge", DecisionKind::Direction);
        decision.options = vec!["yes".into(), "no".into()];
        let mut extra = Extra::new();
        extra.insert("card".into(), self.binding());
        decision.extra = extra;
        decision
    }
}

/// The person's yes to one card. It cannot be built any other way, so nothing the AI or an autonomous run answers
/// can unlock a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerYes {
    repo: String,
    pr: u64,
    head: String,
    base: String,
}

impl OwnerYes {
    pub fn from_decision(card: &MergeCard, decision: &Decision) -> Option<OwnerYes> {
        (decision.id == card.decision_id()
            && decision.extra.get("card") == Some(&card.binding())
            && decision.answered_by == Some(DecidedBy::Person)
            && decision.answer.as_deref() == Some("yes")
            && decision.answered_at.is_some())
        .then(|| OwnerYes { repo: card.repo.clone(), pr: card.pr, head: card.head.clone(), base: card.base.clone() })
    }

    fn is_for(&self, card: &MergeCard) -> bool {
        self.repo == card.repo && self.pr == card.pr && self.head == card.head && self.base == card.base
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
    if pr.head != t.head || pr.base != t.base {
        return Err(RemoteError::HeadMoved);
    }
    Ok(MergeCard { repo: t.repo.clone(), pr: number, head: pr.head, base: pr.base, ci })
}

fn probe_merge(runner: &mut impl Runner, t: &Target, number: u64) -> Seen<BTreeMap<String, String>> {
    match view_pr(runner, t, number) {
        Ok((pr, _)) if pr.state == PrState::Merged => Seen::Yes(evidence(&[("pr", &number.to_string()), ("via", "reconcile")])),
        Ok(_) => Seen::No,
        Err(_) => Seen::Unsure,
    }
}

/// Marks the PR ready and merges it, only with the person's yes for this card, green or absent checks and an
/// unmoved head and base. Merge commit, guarded by the head; the branch is never deleted.
pub fn merge(
    runner: &mut impl Runner,
    ledger: &mut impl Ledger,
    t: &Target,
    card: &MergeCard,
    yes: Option<&OwnerYes>,
) -> Result<(), RemoteError> {
    check(t)?;
    authority(ledger, t, Effect::DraftPr)?;
    yes.filter(|y| y.is_for(card)).ok_or(RemoteError::NeedsYes)?;
    if card.repo != t.repo || card.head != t.head || card.base != t.base {
        return Err(RemoteError::HeadMoved);
    }
    settled(ledger)?;
    acting(runner, ledger)?;
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
    let cwd = runner.neutral();
    let mut last = Ok(Reply { code: 0, stdout: String::new() });
    if pr.draft {
        last = runner.run(&gh(&cwd, &["pr", "ready", &number, "--repo", &t.repo]));
    }
    if matches!(&last, Ok(r) if r.code == 0) {
        last = runner.run(&gh(&cwd, &["pr", "merge", &number, "--repo", &t.repo, "--merge", "--match-head-commit", &card.head]));
    }
    match last {
        Ok(r) if r.code == 0 => {
            ledger.receipt(&id, Outcome::Done, evidence(&[("pr", &number), ("via", "merge")]), None)?;
            Ok(())
        }
        Err(Lost::Missing) => {
            ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::GhMissing.key()))?;
            Err(RemoteError::GhMissing)
        }
        Err(lost) => {
            ledger.receipt(&id, Outcome::Unknown, unknown(&expected, lost == Lost::Killed), Some(RemoteError::Maybe.key()))?;
            Err(RemoteError::Maybe)
        }
        Ok(_) => match probe_merge(runner, t, card.pr) {
            Seen::Yes(found) => {
                ledger.receipt(&id, Outcome::Done, found, None)?;
                Ok(())
            }
            Seen::No => {
                ledger.receipt(&id, Outcome::Failed, expected, Some(RemoteError::MergeFailed.key()))?;
                Err(RemoteError::MergeFailed)
            }
            Seen::Unsure => {
                ledger.receipt(&id, Outcome::Unknown, unknown(&expected, true), Some(RemoteError::Maybe.key()))?;
                Err(RemoteError::Maybe)
            }
        },
    }
}

// Recovery

/// Settles every open remote intent by reading the live remote state, under the same grant as the effects. Nothing is
/// ever run again. `None` means the remote could not be read and the intent stays open.
pub fn reconcile(runner: &mut impl Runner, ledger: &mut impl Ledger, t: &Target) -> Result<Vec<(String, Option<Outcome>)>, RemoteError> {
    check(t)?;
    authority(ledger, t, Effect::Push)?;
    let mut out = vec![];
    for open in ledger.open()? {
        let seen = match open.step {
            Step::Push => probe_push(runner, t, &open.expected),
            Step::Pr => match probe_pr(runner, t, &open.expected) {
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
            // Nothing there yet is only "failed" if nothing the call started can still land it; otherwise it stays open
            // for a later read, which can still find it done.
            Seen::No if open.evidence.get("tree").map(String::as_str) == Some("gone") => {
                ledger.settle(&open.id, Outcome::Failed, open.expected.clone())?;
                Some(Outcome::Failed)
            }
            Seen::No | Seen::Unsure => None,
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
    let account = preflight(runner, &t.url)?;
    if account.repo != t.repo {
        return Err(RemoteError::Url);
    }
    account_ok(ledger, &account.login)?;
    let pushed = push(runner, ledger, t)?;
    let pr = open_pr(runner, ledger, t, spec)?;
    Ok(Published { account, pushed, pr })
}

// The real tools

/// Runs the person's own git and gh, shell-free, hidden, never prompting.
pub struct Process {
    git: PathBuf,
    gh: PathBuf,
    /// Where reads run: Faber's own folder, see [`Runner::neutral`].
    neutral: PathBuf,
}

const OUTPUT_CAP: usize = 16 << 20;
const TIMEOUT: Duration = Duration::from_secs(180);
/// Variables that would aim git at another repository than the one the call names, or at other history or config than
/// the repository's own: the guards read the files at the default places, and these would move them. `GIT_CONFIG` is
/// what `git config --list` itself would read instead of the repository, and the others add config from outside it.
const REPO_VARIABLES: [&str; 13] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_GRAFT_FILE",
    "GIT_SHALLOW_FILE",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
];

/// The indexed config pairs (`GIT_CONFIG_KEY_<n>`, `GIT_CONFIG_VALUE_<n>`) among the names of an environment.
pub fn indexed_config_vars(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let indexed = |name: &str| {
        let upper = name.to_ascii_uppercase();
        upper.starts_with("GIT_CONFIG_KEY_") || upper.starts_with("GIT_CONFIG_VALUE_")
    };
    names.into_iter().filter(|name| indexed(name)).collect()
}

fn native(name: &str) -> PathBuf {
    let Some(cli) = crate::cli::find_cli(name) else { return PathBuf::new() };
    let path = PathBuf::from(&cli.command);
    if !cli.prefix.is_empty() || !path.is_absolute() || (cfg!(windows) && !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
    {
        return PathBuf::new();
    }
    path
}

/// Reads up to `cap` bytes and drains the rest, so the writer never blocks. The flag says whether more came.
pub fn read_capped(reader: &mut impl Read, cap: usize) -> (Vec<u8>, bool) {
    let (mut kept, mut over, mut chunk) = (Vec::new(), false, [0u8; 8192]);
    while let Ok(n) = reader.read(&mut chunk) {
        if n == 0 {
            break;
        }
        if kept.len() + n > cap {
            over = true;
        } else {
            kept.extend_from_slice(&chunk[..n]);
        }
    }
    (kept, over)
}

/// The code a call reports: output past the cap is a failure of its own, whatever the tool exited with.
pub fn reply_code(over: bool, exit: Option<i32>) -> i32 {
    if over { OVER } else { exit.unwrap_or(-1) }
}

/// Windows: a job that ends everything the tool started, so a killed push has no helper left that can still land it.
#[cfg(windows)]
fn confine(child: &std::process::Child) -> Option<win32job::Job> {
    use std::os::windows::io::AsRawHandle;
    let job = win32job::Job::create().ok()?;
    let mut limits = job.query_extended_limit_info().ok()?;
    limits.limit_kill_on_job_close();
    job.set_extended_limit_info(&limits).ok()?;
    // ponytail: assigned after the spawn, so a helper started in those first microseconds escapes; a suspended start would close it.
    job.assign_process(child.as_raw_handle() as isize).ok()?;
    Some(job)
}

impl Process {
    /// A missing tool is not an error here: its calls answer [`Lost::Missing`].
    pub fn find(neutral: PathBuf) -> Process {
        Process { git: native("git"), gh: native("gh"), neutral }
    }

    pub fn at(git: PathBuf, gh: PathBuf, neutral: PathBuf) -> Process {
        Process { git, gh, neutral }
    }

    /// The command for one call. The environment is the person's own, so their credential helper and gh login work,
    /// except for what would point git elsewhere or ask a question.
    pub fn command(&self, call: &Call) -> Command {
        self.command_with(call, indexed_config_vars(std::env::vars_os().map(|(name, _)| name.to_string_lossy().into_owned())))
    }

    /// [`Self::command`], given the indexed config variables of the environment to clear.
    pub fn command_with(&self, call: &Call, indexed: impl IntoIterator<Item = String>) -> Command {
        let program = match call.tool {
            Tool::Git => &self.git,
            Tool::Gh => &self.gh,
        };
        let mut command = Command::new(program);
        command.args(&call.args).current_dir(&call.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        for name in REPO_VARIABLES {
            command.env_remove(name);
        }
        for name in indexed {
            command.env_remove(name);
        }
        // The environment's own list of transports beats every `protocol.*.allow`, so it names the one this call needs.
        command.env("GIT_ALLOW_PROTOCOL", if call.tool == Tool::Git { allowed_protocol(&call.args) } else { "https:ssh" });
        if let Some(parent) = call.cwd.parent() {
            // git looks at the working folder itself and never above it, so a stray repository higher up is not adopted.
            command.env("GIT_CEILING_DIRECTORIES", parent);
        }
        command
            // A partial clone's promisor remote is the repository's choice: git may not fetch from it on its own.
            .env("GIT_NO_LAZY_FETCH", "1")
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

/// The code of a call that was refused before it started.
const REFUSED: i32 = 126;

/// The real path of the neutral folder, if it can be one: absolute, not the shared temp folder, made if missing, not a
/// link (a symlink or junction would put git somewhere else than the ceiling it is given), not a bare repository's
/// layout, and neither it nor any parent holds a `.git` (a folder, a file or a dangling link), so git finds no
/// repository and no config there. Git runs in the returned path.
pub fn private_path(dir: &Path) -> Option<PathBuf> {
    if !dir.is_absolute() || std::fs::create_dir_all(dir).is_err() || dir.symlink_metadata().ok()?.file_type().is_symlink() {
        return None;
    }
    let real = plain(std::fs::canonicalize(dir).ok()?);
    let shared = std::fs::canonicalize(std::env::temp_dir()).map(plain).unwrap_or_else(|_| std::env::temp_dir());
    let bare = real.join("HEAD").is_file() && real.join("objects").is_dir() && real.join("refs").is_dir();
    let free = real.ancestors().all(|folder| folder.join(".git").symlink_metadata().is_err());
    (real != shared && !bare && free).then_some(real)
}

/// Windows spells a canonical path with a `\\?\` prefix, which git and the ceiling comparison do not take.
fn plain(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with(r"UNC\") => PathBuf::from(rest),
        _ => path,
    }
}

pub fn private_dir(dir: &Path) -> bool {
    private_path(dir).is_some()
}

/// What a call that had to be killed reports: killed and proven gone only if the whole tree was confined.
pub fn lost_on_timeout(confined: bool) -> Lost {
    if confined { Lost::Killed } else { Lost::Interrupted }
}

impl Runner for Process {
    fn neutral(&self) -> PathBuf {
        self.neutral.clone()
    }

    fn run(&mut self, call: &Call) -> Result<Reply, Lost> {
        let program = if call.tool == Tool::Git { &self.git } else { &self.gh };
        if program.as_os_str().is_empty() {
            return Err(Lost::Missing);
        }
        // A repository planted in or above the neutral folder, or a link out of it, would answer reads with its own config.
        let neutral;
        let call = if call.cwd == self.neutral {
            let Some(cwd) = private_path(&self.neutral) else { return Ok(Reply { code: REFUSED, stdout: String::new() }) };
            neutral = Call { cwd, ..call.clone() };
            &neutral
        } else {
            call
        };
        let mut child = self.command(call).spawn().map_err(|_| Lost::Missing)?;
        #[cfg(windows)]
        let job = confine(&child);
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            return Err(Lost::Interrupted);
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_capped(&mut stdout, OUTPUT_CAP));
        });
        let deadline = Instant::now() + TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                _ => {
                    // ponytail: only Windows confines the tree (a job object). Elsewhere a killed call stays
                    // unproven; process groups would prove it.
                    #[cfg(windows)]
                    let confined = job.is_some();
                    #[cfg(not(windows))]
                    let confined = false;
                    #[cfg(windows)]
                    drop(job);
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(lost_on_timeout(confined));
                }
            }
        };
        let (kept, over) = rx.recv_timeout(Duration::from_secs(5)).map_err(|_| Lost::Interrupted)?;
        // Output beyond the cap cannot be scanned or trusted: report it as a failure, never as a partial success.
        let code = reply_code(over, status.code());
        Ok(Reply { code, stdout: String::from_utf8_lossy(&kept).into_owned() })
    }
}
