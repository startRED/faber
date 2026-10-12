//! Local git integration runs in a contained helper on the hidden desktop, never in unit tests or a real checkout.
use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::sandbox::{Launch, child_env};
use faber_engine::project::git::{Broker, Executor, GitError, Invocation, Output, Process, Repository, enroll};
use faber_engine::project::lease::{LeaseError, hash};
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{DeleteOp, FileOp, WriteOp};
use faber_engine::project::{GitMode, Grant, Lease};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "projects git: detached work, CAS, hooks, filters, junctions and dirty owner preservation", run: local_git },
        Scenario { name: "projects git review 2 and 6: deletions commit with real git", run: deletion },
        Scenario { name: "projects git review 5: edits during staging and commit never receive acceptance", run: race },
    ]
}
fn local_git(ctx: &mut Ctx) -> Result {
    run_probe(ctx, "--project-git")
}
fn deletion(ctx: &mut Ctx) -> Result {
    run_probe(ctx, "--project-git-delete")
}
fn race(ctx: &mut Ctx) -> Result {
    run_probe(ctx, "--project-git-race")
}
fn run_probe(ctx: &mut Ctx, mode: &str) -> Result {
    let git = faber_engine::cli::find_cli("git").ok_or("git is not installed")?;
    ensure(git.prefix.is_empty(), || "git must be a native executable".into())?;
    let parent: Vec<_> = std::env::vars().collect();
    let mut env = child_env(&parent, &ctx.sb, &Launch::default(), &ctx.paths.fake_cli);
    let mut path = vec![Path::new(&git.command).parent().ok_or("git has no folder")?.to_path_buf()];
    if let Some(system) = std::env::var_os("SystemRoot") {
        path.push(PathBuf::from(system).join("System32"));
    }
    env.insert("PATH".into(), std::env::join_paths(path).map_err(|e| e.to_string())?.to_string_lossy().into());
    // Require the hidden desktop even if this probe is invoked in CI; it uses no webview.
    env.remove("GITHUB_ACTIONS");
    std::fs::copy(ctx.paths.fixtures.join("projects/git-junction.ps1"), ctx.sb.sandbox.join("git-junction.ps1"))
        .map_err(|e| e.to_string())?;
    let args = vec![mode.into(), ctx.sb.sandbox.to_string_lossy().into()];
    let (owned, _, stderr) =
        Owned::start(&ctx.paths.fake_cli, &args, &env, &ctx.sb.sandbox, &ctx.paths.fake_cli).map_err(|e| e.to_string())?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut errors = String::new();
        if let Some(mut pipe) = stderr {
            let _ = pipe.read_to_string(&mut errors);
        }
        let _ = sender.send(errors);
    });
    let waited = eventually("the local git probe to exit", Duration::from_secs(60), || !owned.is_running());
    owned.terminate();
    waited?;
    let errors = receiver.recv_timeout(Duration::from_secs(10)).map_err(|_| "git probe diagnostic pipe did not close")?;
    ensure(ctx.sb.sandbox.join("project-git.ok").is_file(), || errors)?;
    if let Ok(evidence) = std::fs::read_to_string(ctx.sb.sandbox.join("deletion-result.json")) {
        ctx.notes.push(evidence);
    }
    ctx.notes.push("temporary repos only; no remote; owner index, ref and dirty files unchanged".into());
    Ok(())
}

fn git(program: &str, root: &Path, args: &[&str]) -> Result<String> {
    let output = git_output(program, root, args)?;
    ensure(output.status.success(), || format!("fixture git {args:?}: {}", String::from_utf8_lossy(&output.stderr)))?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
}
fn git_output(program: &str, root: &Path, args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new(program);
    command
        .current_dir(root)
        .args([
            "-c",
            "core.hooksPath=NUL",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "filter.trap.required=false",
            "-c",
            "filter.trap.clean=",
            "-c",
            "filter.trap.smudge=",
            "-c",
            "filter.trap.process=",
            "-c",
            "filter.include.required=false",
            "-c",
            "filter.include.clean=",
            "-c",
            "filter.include.smudge=",
            "-c",
            "filter.include.process=",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::contain::NO_WINDOW);
    }
    command.output().map_err(|e| e.to_string())
}
fn check<T>(result: std::result::Result<T, GitError>) -> Result<T> {
    result.map_err(|e| e.key().into())
}

struct Traced(Process);
impl Executor for Traced {
    fn execute(&mut self, call: &Invocation) -> std::result::Result<Output, GitError> {
        let result = self.0.execute(call);
        if !result.as_ref().is_ok_and(|out| out.success) {
            eprintln!("fixture command failed: {:?} in {}", call.args, call.cwd.display());
        }
        result
    }
}

/// Called only by the test helper under `Owned`, with its sandbox environment and hidden desktop.
pub fn probe(sandbox: &Path) -> Result {
    let git_cli = faber_engine::cli::find_cli("git").ok_or("git is unavailable in the sandbox")?;
    let repo = sandbox.join("repo");
    std::fs::create_dir(&repo).map_err(|e| e.to_string())?;
    let program = &git_cli.command;
    git(program, &repo, &["init", "--initial-branch=owner"])?;
    git(program, &repo, &["config", "user.name", "Fixture"])?;
    git(program, &repo, &["config", "user.email", "fixture@example.invalid"])?;
    std::fs::create_dir(repo.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(repo.join("src/a.rs"), "base").map_err(|e| e.to_string())?;
    std::fs::write(repo.join(".gitignore"), "ignored\n").map_err(|e| e.to_string())?;
    std::fs::write(repo.join(".gitattributes"), "src/a.rs filter=trap\nsrc/new.rs filter=include\n").map_err(|e| e.to_string())?;
    git(program, &repo, &["add", "--", "src/a.rs", ".gitignore", ".gitattributes"])?;
    git(program, &repo, &["commit", "--no-gpg-sign", "-m", "fixture base"])?;
    let base = git(program, &repo, &["rev-parse", "HEAD"])?;
    std::fs::write(repo.join("staged"), "index only").map_err(|e| e.to_string())?;
    git(program, &repo, &["add", "--", "staged"])?;
    for (name, bytes) in [("src/a.rs", "owner edit"), ("untracked", "untracked"), ("ignored", "ignored")] {
        std::fs::write(repo.join(name), bytes).map_err(|e| e.to_string())?;
    }
    std::fs::write(repo.join(".git/hooks/post-checkout"), "#!/bin/sh\nprintf escaped > hook-ran\nexit 99\n").map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(repo.join(".git/hooks/post-checkout"), std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    for setting in ["filter.trap.smudge", "filter.trap.clean", "filter.trap.process"] {
        git(program, &repo, &["config", setting, "exit 99"])?;
    }
    git(program, &repo, &["config", "filter.trap.required", "true"])?;
    git(program, &repo, &["config", "core.fsmonitor", "exit 99"])?;
    std::fs::write(
        repo.join(".git/extra-config"),
        "[filter \"include\"]\n process = exit 99\n clean = exit 99\n smudge = exit 99\n required = true\n",
    )
    .map_err(|e| e.to_string())?;
    git(program, &repo, &["config", "include.path", "extra-config"])?;
    git(program, &repo, &["config", "extensions.worktreeConfig", "true"])?;
    let status = git(program, &repo, &["status", "--porcelain=v1", "-uall"])?;
    let index = std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())?;
    let mut process = Traced(check(Process::find())?);
    let enrolled = check(enroll(&mut process, &repo, "HEAD"))?;
    let mut grant = Grant::new(&enrolled.identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = enrolled.author.clone();
    let request = Lease::new(&enrolled.identity.common_dir, "m1", "writer", 1, vec!["src".into()], &base, &"a".repeat(64), 0);
    let records = sandbox.join("records");
    let journal = Journal::open(&repo, &records, "m1").map_err(|e| e.key())?;
    let mut broker = check(Broker::open(enrolled.clone(), journal, "m1", 3, process))?;
    check(broker.create_branch(Some(&grant), "branch"))?;
    let lease = broker.leases().acquire(request).map_err(|e| e.key())?;
    let worktree = check(broker.worktree(Some(&grant), &lease, "writer-worktree"))?;
    git(program, &worktree, &["config", "--worktree", "filter.include.process", "exit 99"])?;
    ensure(!worktree.join("hook-ran").exists(), || "a repository hook ran".into())?;
    ensure(std::fs::read(worktree.join("src/a.rs")).map_err(|e| e.to_string())? == b"base", || {
        "the owner's edit entered the candidate".into()
    })?;
    let mut process = Traced(check(Process::find())?);
    let linked = check(enroll(&mut process, &worktree, "HEAD"))?;
    ensure(linked.identity.common_dir == enrolled.identity.common_dir, || "linked worktree got another repository identity".into())?;
    let patch = FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"base")), content: "accepted".into() });
    broker.leases().patch(&lease, Some(&grant), 3, &patch).map_err(|e| e.key())?;
    let create = FileOp::Write(WriteOp { path: "src/new.rs".into(), expect: None, content: "new".into() });
    broker.leases().patch(&lease, Some(&grant), 3, &create).map_err(|e| e.key())?;
    let paths = vec!["src/a.rs".into(), "src/new.rs".into()];
    let commit = check(broker.commit(Some(&grant), &lease, "commit", "feat: fixture change", &paths))?;
    check(broker.fast_forward(Some(&grant), "land", &base, &commit))?;
    ensure(broker.fast_forward(Some(&grant), "stale", &base, &commit) == Err(GitError::Command), || "a stale CAS landed".into())?;
    let occupied = sandbox.join("occupied");
    git(
        program,
        &repo,
        &[
            "-c",
            "filter.trap.required=false",
            "-c",
            "filter.trap.process=",
            "-c",
            "filter.trap.smudge=",
            "worktree",
            "add",
            "--",
            occupied.to_str().ok_or("invalid path")?,
            "faber/m1",
        ],
    )?;
    ensure(broker.fast_forward(Some(&grant), "checked-out", &commit, &commit) == Err(GitError::Lease(LeaseError::Owned)), || {
        "a checked-out branch was moved".into()
    })?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let link = repo.join("alias");
        let powershell = PathBuf::from(std::env::var_os("SystemRoot").ok_or("missing SystemRoot")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let created = Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(sandbox.join("git-junction.ps1"))
            .arg("-Link")
            .arg(&link)
            .arg("-Target")
            .arg(repo.join("src"))
            .creation_flags(crate::contain::NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        ensure(created.success(), || "could not create the temporary junction fixture".into())?;
        let mut aliased = lease.clone();
        aliased.task = "alias".into();
        aliased.paths = vec!["alias".into()];
        aliased.generation = 0;
        aliased.extra.clear();
        let denied = broker.leases().acquire(aliased);
        std::fs::remove_dir(&link).map_err(|e| e.to_string())?;
        ensure(denied == Err(LeaseError::Path), || "junction alias was admitted".into())?;
    }
    ensure(git(program, &repo, &["rev-parse", "HEAD"])? == base, || "the owner's ref moved".into())?;
    ensure(git(program, &repo, &["status", "--porcelain=v1", "-uall"])? == status, || "the owner's status changed".into())?;
    ensure(std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())? == index, || "the owner's index changed".into())?;
    for (name, bytes) in [("src/a.rs", "owner edit"), ("untracked", "untracked"), ("ignored", "ignored")] {
        ensure(std::fs::read_to_string(repo.join(name)).map_err(|e| e.to_string())? == bytes, || format!("owner file {name} changed"))?;
    }
    drop(broker);
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}

fn review_repository(sandbox: &Path) -> Result<(PathBuf, Repository, String)> {
    let repo = sandbox.join("repo");
    std::fs::create_dir_all(repo.join("src")).map_err(|e| e.to_string())?;
    let program = faber_engine::cli::find_cli("git").ok_or("missing git")?.command;
    git(&program, &repo, &["init", "--initial-branch=owner"])?;
    git(&program, &repo, &["config", "user.name", "Fixture"])?;
    git(&program, &repo, &["config", "user.email", "fixture@example.invalid"])?;
    std::fs::write(repo.join("src/a.rs"), "base").map_err(|e| e.to_string())?;
    std::fs::write(repo.join("src/delete.rs"), "delete me").map_err(|e| e.to_string())?;
    git(&program, &repo, &["add", "--", "src/a.rs", "src/delete.rs"])?;
    git(&program, &repo, &["commit", "--no-gpg-sign", "-m", "fixture base"])?;
    let repository = check(enroll(&mut check(Process::find())?, &repo, "HEAD"))?;
    Ok((repo, repository, program))
}
fn review_writer<E: Executor>(sandbox: &Path, repository: Repository, executor: E) -> Result<(Broker<E>, Grant, Lease, PathBuf)> {
    let mut grant = Grant::new(&repository.identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repository.author.clone();
    let lease = Lease::new(&repository.identity.common_dir, "m1", "writer", 1, vec!["src".into()], &repository.base, &"a".repeat(64), 0);
    let records = sandbox.join("records");
    let journal = Journal::open(Path::new(&repository.identity.root), &records, "m1").map_err(|e| e.key())?;
    let mut broker = check(Broker::open(repository, journal, "m1", 3, executor))?;
    let lease = broker.leases().acquire(lease).map_err(|e| e.key())?;
    let root = check(broker.worktree(Some(&grant), &lease, "worktree"))?;
    Ok((broker, grant, lease, root))
}
pub fn probe_deletion(sandbox: &Path) -> Result {
    let (repo, repository, program) = review_repository(sandbox)?;
    let raw_root = sandbox.join("raw-deletion");
    git(&program, &repo, &["worktree", "add", "--detach", "--", raw_root.to_str().ok_or("bad path")?, &repository.base])?;
    std::fs::remove_file(raw_root.join("src/delete.rs")).map_err(|e| e.to_string())?;
    git(&program, &raw_root, &["add", "--", "src/delete.rs"])?;
    let raw = git_output(&program, &raw_root, &["commit", "--only", "--no-gpg-sign", "-m", "raw deletion", "--", "src/delete.rs"])?;
    let evidence = serde_json::json!({"git": git(&program, &repo, &["--version"])?, "exit": raw.status.code(), "stderr": String::from_utf8_lossy(&raw.stderr)}).to_string();
    std::fs::write(sandbox.join("deletion-result.json"), &evidence).map_err(|e| e.to_string())?;
    let (mut broker, grant, lease, root) = review_writer(sandbox, repository, Traced(check(Process::find())?))?;
    broker
        .leases()
        .patch(&lease, Some(&grant), 3, &FileOp::Delete(DeleteOp { path: "src/delete.rs".into(), expect: hash(b"delete me") }))
        .map_err(|e| e.key())?;
    let commit = broker
        .commit(Some(&grant), &lease, "delete", "fix: delete fixture file", &["src/delete.rs".into()])
        .map_err(|e| format!("broker deletion: {}; real git: {evidence}", e.key()))?;
    ensure(!git(&program, &root, &["ls-tree", "--name-only", &commit, "--", "src/delete.rs"])?.contains("src/delete.rs"), || {
        "deleted file was committed".into()
    })?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}
struct EditDuringGit {
    inner: Process,
    during: &'static str,
}
impl Executor for EditDuringGit {
    fn execute(&mut self, call: &Invocation) -> std::result::Result<Output, GitError> {
        let editing = call.args.iter().any(|arg| arg == if self.during == "index" { "add" } else { self.during });
        if editing && self.during == "commit" {
            std::fs::write(call.cwd.join("src/a.rs"), "unreviewed").map_err(|_| GitError::Process)?;
        }
        let out = self.inner.execute(call)?;
        if editing && self.during == "index" {
            std::fs::write(call.cwd.join("src/a.rs"), "unreviewed").map_err(|_| GitError::Process)?;
            self.inner.execute(call)?;
        }
        if editing {
            std::fs::write(
                call.cwd.join("src/a.rs"),
                if self.during == "commit" || self.during == "index" { "approved" } else { "unreviewed" },
            )
            .map_err(|_| GitError::Process)?;
        }
        Ok(out)
    }
}
pub fn probe_race(sandbox: &Path) -> Result {
    let mut failures = vec![];
    for during in ["add", "index", "commit"] {
        let area = sandbox.join(during);
        let (_, repository, _) = review_repository(&area)?;
        let (mut broker, grant, lease, _) = review_writer(&area, repository, EditDuringGit { inner: check(Process::find())?, during })?;
        broker
            .leases()
            .patch(
                &lease,
                Some(&grant),
                3,
                &FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"base")), content: "approved".into() }),
            )
            .map_err(|e| e.key())?;
        let result = broker.commit(Some(&grant), &lease, "race", "fix: approved fixture", &["src/a.rs".into()]);
        if result != Err(GitError::Lease(LeaseError::Quarantined)) {
            failures.push(format!("edit during {during} was accepted: {result:?}"));
        }
        if during == "commit" {
            let pending = broker.journal().pending().map_err(|e| e.key())?;
            ensure(pending.len() == 1 && pending[0].id == "race" && pending[0].expected.contains_key("tree"), || {
                "unverified commit lost its pinned pending intent".into()
            })?;
        }
    }
    ensure(failures.is_empty(), || failures.join("; "))?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}
