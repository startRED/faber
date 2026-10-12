//! The local git broker: fixed argv, detached private worktrees and CAS of the mission's own ref. No remote operations.
use super::lease::{LeaseError, Leases, real_path};
use super::store::{Journal, JournalError};
use super::{Author, Effect, Grant, Intent, IntentKind, Lease, Outcome, Receipt, RepoIdentity, valid_id};
use crate::store::Fault;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitError {
    Repository,
    Base,
    Ref,
    Author,
    Command,
    Process,
    Authority(&'static str),
    Lease(LeaseError),
    Journal(JournalError),
}
impl GitError {
    pub fn key(self) -> &'static str {
        match self {
            Self::Repository => "reason.git.repository",
            Self::Base => "reason.git.base",
            Self::Ref => "reason.git.ref",
            Self::Author => "reason.grant.noAuthor",
            Self::Command => "reason.git.command",
            Self::Process => "reason.git.unknown",
            Self::Authority(key) => key,
            Self::Lease(e) => e.key(),
            Self::Journal(e) => e.key(),
        }
    }
}
impl From<LeaseError> for GitError {
    fn from(e: LeaseError) -> Self {
        Self::Lease(e)
    }
}
impl From<JournalError> for GitError {
    fn from(e: JournalError) -> Self {
        Self::Journal(e)
    }
}

pub fn valid_oid(oid: &str) -> bool {
    [40, 64].contains(&oid.len()) && oid.bytes().all(|b| b.is_ascii_hexdigit()) && oid.bytes().any(|b| b != b'0')
}
fn valid_ref(reference: &str) -> bool {
    !reference.is_empty()
        && reference.len() <= 256
        && !reference.starts_with(['-', '/'])
        && !reference.ends_with(['/', '.'])
        && !reference.contains(['\\', ':', ' ', '~', '^', '?', '*', '[', ';', '&', '|', '$', '`', '"', '\''])
        && !reference.contains("..")
        && !reference.contains("@{")
        && reference.bytes().all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
        && reference.split('/').all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}
fn valid_author(author: &Author) -> bool {
    [&author.name, &author.email]
        .iter()
        .all(|s| !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(|c| c.is_control() || matches!(c, '<' | '>')))
}
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub cwd: PathBuf,
    pub args: Vec<String>,
    /// Only ordinary `config --get user.*` enrollment reads see the user's global git configuration.
    pub identity: bool,
    pub author: Option<Author>,
}
impl Invocation {
    fn new(cwd: &Path, args: &[&str]) -> Self {
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        let mut fixed = vec![];
        for config in [
            format!("core.hooksPath={null}"),
            "core.fsmonitor=false".into(),
            "credential.helper=".into(),
            "protocol.allow=never".into(),
            "protocol.file.allow=never".into(),
            "core.pager=".into(),
            "gc.auto=0".into(),
            "maintenance.auto=false".into(),
            "commit.gpgSign=false".into(),
            "tag.gpgSign=false".into(),
            "submodule.recurse=false".into(),
            "core.sparseCheckout=false".into(),
            format!("core.attributesFile={null}"),
            "diff.external=".into(),
        ] {
            fixed.extend(["-c".into(), config]);
        }
        fixed.extend(args.iter().map(|s| (*s).into()));
        Self { cwd: cwd.into(), args: fixed, identity: false, author: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
}
pub trait Executor {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError>;
}

/// Resolves only a native git executable. It never launches a shim, shell, pager, hook or credential helper.
pub struct Process {
    executable: PathBuf,
}
impl Process {
    pub fn find() -> Result<Self, GitError> {
        let cli = crate::cli::find_cli("git").ok_or(GitError::Process)?;
        let path = PathBuf::from(cli.command);
        if !cli.prefix.is_empty()
            || !path.is_absolute()
            || (cfg!(windows) && !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        {
            return Err(GitError::Process);
        }
        Ok(Self { executable: path })
    }
    fn command(&self, call: &Invocation) -> Command {
        let mut command = Command::new(&self.executable);
        command.env_clear().current_dir(&call.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        for name in ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP", "TMPDIR", "LANG"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        if call.identity {
            for name in ["HOME", "USERPROFILE", "APPDATA", "XDG_CONFIG_HOME"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
        } else {
            command.env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" });
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_LITERAL_PATHSPECS", "1");
        if let Some(author) = &call.author {
            command
                .env("GIT_AUTHOR_NAME", &author.name)
                .env("GIT_AUTHOR_EMAIL", &author.email)
                .env("GIT_COMMITTER_NAME", &author.name)
                .env("GIT_COMMITTER_EMAIL", &author.email);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command
    }
}
impl Executor for Process {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        // A repository can configure arbitrary filter commands. Discover only their names and disable every driver,
        // including process filters and required filters, before any checkout, staging or commit.
        let query = Invocation::new(
            &call.cwd,
            &["config", "--includes", "--name-only", "--get-regexp", "^filter\\..*\\.(clean|smudge|process|required)$"],
        );
        let found = self.command(&query).args(&query.args).output().map_err(|_| GitError::Process)?;
        if !found.status.success() && found.status.code() != Some(1) {
            return Err(GitError::Command);
        }
        let mut command = self.command(call);
        let names = String::from_utf8(found.stdout).map_err(|_| GitError::Command)?;
        for key in names.lines() {
            if !key.starts_with("filter.") || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_/".contains(&b)) {
                return Err(GitError::Command);
            }
            command.args(["-c", &format!("{key}={}", if key.ends_with(".required") { "false" } else { "" })]);
        }
        let out = command.args(&call.args).output().map_err(|_| GitError::Process)?;
        Ok(Output { success: out.status.success(), stdout: out.stdout })
    }
}

fn text(output: Output, error: GitError) -> Result<String, GitError> {
    if !output.success {
        return Err(error);
    }
    String::from_utf8(output.stdout).map(|s| s.trim_end_matches(['\r', '\n']).into()).map_err(|_| error)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Repository {
    pub identity: RepoIdentity,
    pub base: String,
    pub author: Option<Author>,
}
/// Read-only enrollment; no grant is created and no owner's dirty state is incorporated.
pub fn enroll(executor: &mut impl Executor, folder: &Path, base: &str) -> Result<Repository, GitError> {
    if !valid_ref(base) {
        return Err(GitError::Ref);
    }
    let folder = real_path(folder)?.canonicalize().map_err(|_| GitError::Repository)?;
    let bare = text(executor.execute(&Invocation::new(&folder, &["rev-parse", "--is-bare-repository"]))?, GitError::Repository)?;
    if bare != "false" {
        return Err(GitError::Repository);
    }
    let root = text(executor.execute(&Invocation::new(&folder, &["rev-parse", "--show-toplevel"]))?, GitError::Repository)?;
    let common = text(
        executor.execute(&Invocation::new(&folder, &["rev-parse", "--path-format=absolute", "--git-common-dir"]))?,
        GitError::Repository,
    )?;
    let root = real_path(Path::new(&root))?.canonicalize().map_err(|_| GitError::Repository)?;
    let common = real_path(Path::new(&common))?.canonicalize().map_err(|_| GitError::Repository)?;
    if root != folder {
        return Err(GitError::Repository);
    }
    let base = text(
        executor.execute(&Invocation::new(&root, &["rev-parse", "--verify", "--end-of-options", &format!("{base}^{{commit}}")]))?,
        GitError::Base,
    )?;
    if !valid_oid(&base) {
        return Err(GitError::Base);
    }
    let mut identity = |key| -> Result<Option<String>, GitError> {
        let mut call = Invocation::new(&root, &["config", "--get", key]);
        call.identity = true;
        let output = executor.execute(&call)?;
        if !output.success {
            return Ok(None);
        }
        Ok(Some(text(output, GitError::Author)?))
    };
    let name = identity("user.name")?;
    let email = identity("user.email")?;
    let author = name.zip(email).map(|(name, email)| Author { name, email }).filter(valid_author);
    Ok(Repository { identity: RepoIdentity::new(&common.to_string_lossy(), &root.to_string_lossy()), base, author })
}

pub struct Broker<E> {
    repository: Repository,
    leases: Leases,
    mission: String,
    revision: u64,
    executor: E,
}
impl<E: Executor> Broker<E> {
    pub fn open(repository: Repository, journal: Journal, mission: &str, revision: u64, executor: E) -> Result<Self, GitError> {
        if !valid_id(mission) || !valid_oid(&repository.base) {
            return Err(GitError::Ref);
        }
        let leases = Leases::open(repository.identity.clone(), journal, mission)?;
        Ok(Self { repository, leases, mission: mission.into(), revision, executor })
    }
    pub fn leases(&mut self) -> &mut Leases {
        &mut self.leases
    }
    pub fn journal(&self) -> &Journal {
        self.leases.journal()
    }
    pub fn executor(&self) -> &E {
        &self.executor
    }
    fn authority(&self, grant: Option<&Grant>, effect: Effect) -> Result<(), GitError> {
        grant
            .ok_or(GitError::Authority("reason.grant.none"))?
            .allows(&self.repository.identity, self.revision, effect)
            .map_err(GitError::Authority)
    }
    fn effect(
        &mut self,
        grant: Option<&Grant>,
        id: &str,
        kind: IntentKind,
        calls: &[Invocation],
        expected: &[(&str, &str)],
        commit_lease: Option<&Lease>,
    ) -> Result<Output, GitError> {
        self.authority(grant, kind.effect().ok_or(GitError::Command)?)?;
        if !self.journal().pending()?.is_empty() {
            return Err(GitError::Process);
        }
        let mut intent = Intent::new(id, &self.mission, kind, self.revision, now());
        intent.expected = expected.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect();
        self.journal().record_intent(&intent)?;
        let mut output = Output { success: true, stdout: vec![] };
        for call in calls {
            self.authority(grant, kind.effect().ok_or(GitError::Command)?)?;
            // Missing delivery acknowledgment stays pending. Recovery must reconcile it rather than repeat it.
            output = self.executor.execute(call)?;
            if !output.success {
                break;
            }
        }
        if kind == IntentKind::Commit && output.success {
            let head = text(output.clone(), GitError::Base)?;
            if !valid_oid(&head) || expected.iter().any(|(key, value)| *key == "parent" && *value == head) {
                return Err(GitError::Process);
            }
            intent.expected.insert("head".into(), head);
            let root = &calls.last().ok_or(GitError::Process)?.cwd;
            let actual_tree = text(self.executor.execute(&Invocation::new(root, &["rev-parse", "HEAD^{tree}"]))?, GitError::Process)?;
            if intent.expected.get("tree") != Some(&actual_tree) {
                return Err(LeaseError::Quarantined.into());
            }
            self.leases.audit(commit_lease.ok_or(LeaseError::Stale)?)?;
        }
        let mut receipt = Receipt::new(id, if output.success { Outcome::Done } else { Outcome::Failed }, now());
        receipt.evidence = intent.expected;
        if !output.success {
            receipt.reason = Some(GitError::Command.key().into());
        }
        self.journal().record_receipt(&receipt)?;
        if !output.success {
            return Err(GitError::Command);
        }
        Ok(output)
    }
    pub fn create_branch(&mut self, grant: Option<&Grant>, id: &str) -> Result<(), GitError> {
        let branch = format!("refs/heads/faber/{}", self.mission);
        if self.leases.owns_branch(&branch) {
            return Err(LeaseError::Owned.into());
        }
        let base = self.repository.base.clone();
        let zero = "0".repeat(base.len());
        let call = Invocation::new(Path::new(&self.repository.identity.root), &["update-ref", &branch, &base, &zero]);
        self.effect(grant, id, IntentKind::FastForward, &[call], &[("ref", &branch), ("old", &zero), ("new", &base)], None)?;
        self.leases.record_branch(&branch)?;
        Ok(())
    }
    pub fn worktree(&mut self, grant: Option<&Grant>, lease: &Lease, id: &str) -> Result<PathBuf, GitError> {
        self.authority(grant, Effect::Worktree)?;
        // A live current holder is required even before its worktree exists.
        self.leases.index(lease)?;
        let root = self
            .journal()
            .private()
            .canonicalize()
            .map_err(|_| GitError::Lease(LeaseError::Store(Fault::Read)))?
            .join(format!("writer-{}", lease.generation));
        real_path(&root)?;
        if root.exists() {
            return Err(GitError::Lease(LeaseError::Owned));
        }
        let base = lease.base.clone();
        let raw = root.to_str().ok_or(GitError::Repository)?;
        let path = if let Some(unc) = raw.strip_prefix("\\\\?\\UNC\\") {
            format!("\\\\{unc}")
        } else {
            raw.strip_prefix("\\\\?\\").unwrap_or(raw).into()
        };
        let call = Invocation::new(Path::new(&self.repository.identity.root), &["worktree", "add", "--detach", "--", &path, &base]);
        if let Err(e) = self.effect(grant, id, IntentKind::Worktree, &[call], &[("path", &path), ("base", &base)], None) {
            self.leases.quarantine(lease)?;
            return Err(e);
        }
        if let Err(e) = self.leases.bind(lease, &root) {
            self.leases.quarantine(lease)?;
            return Err(e.into());
        }
        Ok(root)
    }
    pub fn fast_forward(&mut self, grant: Option<&Grant>, id: &str, old: &str, new: &str) -> Result<(), GitError> {
        self.authority(grant, Effect::FastForward)?;
        if !valid_oid(old) || !valid_oid(new) || old.len() != new.len() {
            return Err(GitError::Ref);
        }
        if !self.journal().pending()?.is_empty() {
            return Err(GitError::Process);
        }
        let branch = format!("refs/heads/faber/{}", self.mission);
        if !self.leases.owns_branch(&branch) {
            return Err(LeaseError::Owned.into());
        }
        let root = Path::new(&self.repository.identity.root);
        let worktrees =
            text(self.executor.execute(&Invocation::new(root, &["worktree", "list", "--porcelain", "-z"]))?, GitError::Command)?;
        if worktrees.split('\0').any(|field| field == format!("branch {branch}")) {
            return Err(LeaseError::Owned.into());
        }
        let ancestry = self.executor.execute(&Invocation::new(root, &["merge-base", "--is-ancestor", old, new]))?;
        if !ancestry.success {
            return Err(GitError::Base);
        }
        let call = Invocation::new(root, &["update-ref", &branch, new, old]);
        self.effect(grant, id, IntentKind::FastForward, &[call], &[("ref", &branch), ("old", old), ("new", new)], None)?;
        Ok(())
    }
    /// Task 7 supplies accepted paths after its proof/review gates. Git never stages the entire directory.
    pub fn commit(
        &mut self,
        grant: Option<&Grant>,
        lease: &Lease,
        id: &str,
        message: &str,
        accepted: &[String],
    ) -> Result<String, GitError> {
        self.authority(grant, Effect::Commit)?;
        let author = grant.and_then(|g| g.author.as_ref()).filter(|a| valid_author(a)).ok_or(GitError::Author)?.clone();
        let changed = self.leases.audit(lease)?;
        let mut accepted = accepted.to_vec();
        accepted.sort();
        if accepted.is_empty() || accepted != changed || message.is_empty() || message.len() > 8192 || message.contains('\0') {
            return Err(GitError::Lease(LeaseError::Scope));
        }
        let root = self.leases.worktree(lease)?;
        let head = text(self.executor.execute(&Invocation::new(&root, &["rev-parse", "HEAD"]))?, GitError::Base)?;
        if head != lease.base {
            return Err(GitError::Base);
        }
        let staged = self.executor.execute(&Invocation::new(&root, &["diff", "--cached", "--name-only", "-z", "--"]))?;
        if !staged.success || !staged.stdout.is_empty() {
            self.leases.quarantine(lease)?;
            return Err(LeaseError::Quarantined.into());
        }
        let mut stage = Invocation::new(&root, &["add", "--"]);
        stage.args.extend(accepted.clone());
        let mut call = Invocation::new(&root, &["commit", "--only", "--no-gpg-sign", "-m", message, "--"]);
        call.args.extend(accepted.clone());
        call.author = Some(author);
        let read_head = Invocation::new(&root, &["rev-parse", "HEAD"]);
        let result = (|| {
            // Staging is a disposable worktree effect; the commit's durable intent pins the verified index tree.
            let stage_id = format!("stage-{}", uuid::Uuid::new_v4());
            self.effect(grant, &stage_id, IntentKind::Worktree, &[stage], &[("phase", "stage"), ("commit", id), ("parent", &head)], None)?;
            self.leases.audit(lease)?;
            let diff = self.executor.execute(&Invocation::new(&root, &["diff", "--quiet", "--no-ext-diff", "--no-textconv", "--"]))?;
            if !diff.success {
                return Err(LeaseError::Quarantined.into());
            }
            let paths =
                text(self.executor.execute(&Invocation::new(&root, &["diff", "--cached", "--name-only", "-z", "--"]))?, GitError::Command)?;
            let mut paths: Vec<String> = paths.split('\0').filter(|p| !p.is_empty()).map(String::from).collect();
            paths.sort();
            if paths != accepted {
                return Err(LeaseError::Quarantined.into());
            }
            let tree = text(self.executor.execute(&Invocation::new(&root, &["write-tree"]))?, GitError::Process)?;
            if !valid_oid(&tree) {
                return Err(GitError::Process);
            }
            self.effect(grant, id, IntentKind::Commit, &[call, read_head], &[("parent", &head), ("tree", &tree)], Some(lease))
        })();
        let output = match result {
            Ok(output) => output,
            Err(e) => {
                if self.leases.index(lease).is_ok() {
                    self.leases.quarantine(lease)?;
                }
                return Err(e);
            }
        };
        text(output, GitError::Base)
    }
    pub fn finish(self) -> Result<(), GitError> {
        Ok(self.leases.finish()?)
    }
}
