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
        Scenario { name: "T13 1: red acceptance receipt cannot satisfy landing", run: mission_red },
        Scenario { name: "T13 2: exact reviewed tree lands locally in Seguro", run: mission_landing },
        Scenario { name: "T13 3: persisted report retains exact evidence and unknown measurements", run: mission_report },
        Scenario { name: "T13 7: original checkout and sandbox stable install hashes stay unchanged", run: mission_unchanged },
        Scenario { name: "projects T5: fixture mission lands an exact tree and retires operational data", run: mission },
        Scenario {
            name: "projects git and T11: detached work, CAS, hooks, filters, junction ownership and dirty owner preservation",
            run: local_git,
        },
        Scenario { name: "projects git review 2 and 6: deletions commit with real git", run: deletion },
        Scenario { name: "projects git review 5 and opus H3: stage races refused, commits use the verified tree", run: race },
        Scenario { name: "projects git opus M3 and L4: candidate junction swap cannot redirect broker IO", run: path_race },
        Scenario { name: "projects git opus M1: git clean cannot erase pending effect authority", run: cleanup },
    ]
}
fn acceptance(ctx: &mut Ctx, key: &str) -> Result {
    run_probe(ctx, "--project-mission")?;
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ctx.sb.sandbox.join("t13-scripted.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    ensure(report[key] == true, || format!("missing T13 {key} evidence: {report}"))?;
    ctx.notes.push(format!(
        "T13 {key}: head={}, tree={}, red exit={}, owner={}, index={}, stable={}; fixture only",
        report["head"], report["tree"], report["redReceipt"]["exit"], report["ownerSha256"], report["indexSha256"], report["stableSha256"]
    ));
    Ok(())
}
fn mission_red(ctx: &mut Ctx) -> Result {
    acceptance(ctx, "red")
}
fn mission_landing(ctx: &mut Ctx) -> Result {
    acceptance(ctx, "landing")
}
fn mission_report(ctx: &mut Ctx) -> Result {
    acceptance(ctx, "report")
}
fn mission_unchanged(ctx: &mut Ctx) -> Result {
    acceptance(ctx, "unchanged")
}
fn mission(ctx: &mut Ctx) -> Result {
    run_probe(ctx, "--project-mission")
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
fn path_race(ctx: &mut Ctx) -> Result {
    run_probe(ctx, "--project-path-race")
}
fn cleanup(ctx: &mut Ctx) -> Result {
    run_probe(ctx, "--project-git-clean")
}
pub(super) fn run_probe(ctx: &mut Ctx, mode: &str) -> Result {
    let git = faber_engine::cli::find_cli("git").ok_or("git is not installed")?;
    ensure(git.prefix.is_empty(), || "git must be a native executable".into())?;
    let parent: Vec<_> = std::env::vars().collect();
    let mut env = child_env(&parent, &ctx.sb, &Launch::default(), &ctx.paths.fake_cli);
    let mut path = vec![Path::new(&git.command).parent().ok_or("git has no folder")?.to_path_buf()];
    if let Some(system) = std::env::var_os("SystemRoot") {
        path.push(PathBuf::from(system).join("System32"));
    }
    if mode == "--remote-git" {
        // A hostile environment: every transport allowed and config of its own. The module's calls must ignore both.
        for (name, value) in [
            ("GIT_ALLOW_PROTOCOL", "file:ext:https:ssh"),
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "http.proxy"),
            ("GIT_CONFIG_VALUE_0", "http://127.0.0.1:9"),
            ("GIT_CONFIG_PARAMETERS", "'http.proxy=http://127.0.0.1:9'"),
        ] {
            env.insert(name.into(), value.into());
        }
        // The marker programs of that probe are shell scripts, which git finds a shell for through the PATH only.
        let shell = Path::new(&git.command).ancestors().map(|folder| folder.join("usr/bin")).find(|bin| bin.join("sh.exe").is_file());
        path.extend(shell);
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

/// Seed only a temporary repository, on the same hidden desktop as the existing git probes.
pub fn screen_fixture(sandbox: &Path) -> Result {
    use faber_engine::model::Project;
    use faber_engine::project::{Decision, DecisionKind, Mission, MissionState, Plan, RepoIdentity, Task, TaskKind, TaskState};
    let cli = faber_engine::cli::find_cli("git").ok_or("git is unavailable")?;
    let repo = sandbox.join("repo");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    git(&cli.command, &repo, &["init", "--initial-branch=owner"])?;
    git(&cli.command, &repo, &["config", "user.name", "Fixture"])?;
    git(&cli.command, &repo, &["config", "user.email", "fixture@example.invalid"])?;
    std::fs::write(repo.join("fixture"), "original").map_err(|e| e.to_string())?;
    git(&cli.command, &repo, &["add", "--", "fixture"])?;
    git(&cli.command, &repo, &["commit", "--no-gpg-sign", "-m", "fixture"])?;
    let base = git(&cli.command, &repo, &["rev-parse", "HEAD"])?;
    let mut process = Process::find().map_err(|e| e.key().to_string())?;
    let enrolled = enroll(&mut process, &repo, "HEAD").map_err(|e| e.key().to_string())?;
    let mut project = Project::new("demo".into(), "Demo".into(), repo.to_string_lossy().into());
    project.repo = Some(RepoIdentity::new(&enrolled.identity.common_dir, &project.path));
    let mut grant = Grant::new(&enrolled.identity.common_dir, GitMode::Seguro, 1, 100);
    grant.author = enrolled.author;
    project.grant = Some(grant);
    let data = sandbox.join("data");
    std::fs::write(data.join("projects.json"), serde_json::to_vec(&vec![project.clone()]).unwrap()).map_err(|e| e.to_string())?;
    let journal = Journal::open(&repo, &data, "screen_mission").map_err(|e| e.key().to_string())?;
    let mut ledger = faber_engine::ledger::Ledger::default();
    let mut budget = faber_engine::project::budget::Budget::default();
    budget.grants.insert(
        "screen_mission".into(),
        faber_engine::project::budget::Grant {
            cap_micros: 100_000,
            worker_micros: 50_000,
            kind_caps: Default::default(),
            vendors: std::collections::BTreeMap::from([(faber_engine::model::Vendor::Codex, 100_000)]),
            review_micros: 20_000,
            repair_micros: 20_000,
            canary_micros: 10_000,
            paused: false,
            extra: Default::default(),
        },
    );
    ledger.extra.insert("missionBudget".into(), serde_json::to_value(budget).unwrap());
    std::fs::write(data.join("budget.json"), serde_json::to_vec(&ledger).unwrap()).map_err(|e| e.to_string())?;
    let mut mission = Mission::new("screen_mission", "demo", &enrolled.identity.common_dir, "A measured project goal", &base, 1, 100);
    mission.state = MissionState::Paused;
    journal.save_mission(&mission).map_err(|e| e.key().to_string())?;
    let tasks = (0..200)
        .map(|i| {
            let mut t =
                Task::new(&format!("task{i}"), &format!("Task {i}: a long requirement that must remain readable"), TaskKind::Writer);
            t.state = [TaskState::Ready, TaskState::Running, TaskState::Review, TaskState::Done, TaskState::Blocked][i % 5];
            t.writes = vec!["ui/Projects.tsx".into()];
            if i > 0 {
                t.requires.push(format!("task{}", i - 1));
            }
            t.proofs = vec!["screen".into()];
            t
        })
        .collect();
    journal.save_plan(&Plan::new("screen_mission", 1, tasks)).map_err(|e| e.key().to_string())?;
    let mut d = Decision::new("choice", "task0", "Pick a direction", DecisionKind::Direction);
    d.options = vec!["Keep".into(), "Change".into()];
    d.default = Some("Keep".into());
    journal.save_decision(&d).map_err(|e| e.key().to_string())?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}

pub(super) fn git(program: &str, root: &Path, args: &[&str]) -> Result<String> {
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
            "-c",
            "hook.fixture-post-checkout.enabled=false",
            "-c",
            "hook.fixture-pre-commit.enabled=false",
            "-c",
            "hook.fixture-post-commit.enabled=false",
            "-c",
            "hook.fixture-reference-transaction.enabled=false",
            "-c",
            "hook.included.enabled=false",
            "-c",
            "hook.worktree.enabled=false",
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
    // git 2.54 config hooks are independent of core.hooksPath.
    for event in ["post-checkout", "pre-commit", "post-commit", "reference-transaction"] {
        // git 2.55 forbids friendly names that collide with event names, even for disabled hooks.
        let name = format!("fixture-{event}");
        git(program, &repo, &["config", &format!("hook.{name}.event"), event])?;
        git(program, &repo, &["config", &format!("hook.{name}.command"), "exit 99"])?;
    }
    let included = repo.join(".git/extra-config");
    let mut file = std::fs::OpenOptions::new().append(true).open(included).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut file, b"[hook \"included\"]\n event = post-checkout\n command = exit 99\n")
        .map_err(|e| e.to_string())?;
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
    check(broker.create_branch(|| Ok(Some(grant.clone())), "branch"))?;
    let lease = broker.leases().acquire(request).map_err(|e| e.key())?;
    let worktree = check(broker.worktree(|| Ok(Some(grant.clone())), &lease, "writer-worktree"))?;
    git(program, &worktree, &["config", "--worktree", "filter.include.process", "exit 99"])?;
    git(program, &worktree, &["config", "--worktree", "hook.worktree.event", "pre-commit"])?;
    git(program, &worktree, &["config", "--worktree", "hook.worktree.command", "exit 99"])?;
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
    let commit = check(broker.commit(|| Ok(Some(grant.clone())), &lease, "commit", "feat: fixture change", &paths))?;
    check(broker.fast_forward(|| Ok(Some(grant.clone())), "land", &base, &commit))?;
    ensure(broker.fast_forward(|| Ok(Some(grant.clone())), "stale", &base, &commit) == Err(GitError::Command), || {
        "a stale CAS landed".into()
    })?;
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
    ensure(
        broker.fast_forward(|| Ok(Some(grant.clone())), "checked-out", &commit, &commit) == Err(GitError::Lease(LeaseError::Owned)),
        || "a checked-out branch was moved".into(),
    )?;
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
        ensure(denied == Err(LeaseError::Path), || "junction alias was admitted".into())?;
        use faber_engine::project::update::{SlotLayout, UpdateManager};
        let installed_data = sandbox.join("installed-data");
        let installed_release = sandbox.join("installed-release");
        ensure(SlotLayout::new(&link, &installed_data, &installed_release).is_err(), || "T11 accepted a junction root".into())?;
        let layout = SlotLayout::new(&sandbox.join("development"), &installed_data, &installed_release).map_err(|e| format!("{e:?}"))?;
        layout.init_dirs().map_err(|e| format!("{e:?}"))?;
        std::fs::remove_dir(layout.staging_dir()).map_err(|e| e.to_string())?;
        std::fs::rename(&link, layout.staging_dir()).map_err(|e| e.to_string())?;
        let outside = repo.join("src/a.rs");
        let before = std::fs::read(&outside).map_err(|e| e.to_string())?;
        let aliased = layout.staging_dir().join("a.rs");
        ensure(!layout.is_owned_path(&aliased), || "T11 owns a junction target".into())?;
        ensure(layout.clean_stale_builds(&[(aliased, hash(&before))]).is_err(), || "T11 cleaned a junction target".into())?;
        ensure(UpdateManager::new(layout.clone()).is_err(), || "T11 initialized through a junction".into())?;
        ensure(std::fs::read(&outside).map_err(|e| e.to_string())? == before, || "T11 changed junction target bytes".into())?;
        std::fs::remove_dir(layout.staging_dir()).map_err(|e| e.to_string())?;
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
    let root = check(broker.worktree(|| Ok(Some(grant.clone())), &lease, "worktree"))?;
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
        .commit(|| Ok(Some(grant.clone())), &lease, "delete", "fix: delete fixture file", &["src/delete.rs".into()])
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
        let command = match self.during {
            "index" => "add",
            "commit" | "commit-dirty" => "commit-tree",
            other => other,
        };
        let editing = call.args.iter().any(|arg| arg == command);
        if editing && matches!(self.during, "commit" | "commit-dirty") {
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
    for during in ["add", "index", "commit", "commit-dirty"] {
        let area = sandbox.join(during);
        let (_, repository, _) = review_repository(&area)?;
        let (mut broker, grant, lease, root) = review_writer(&area, repository, EditDuringGit { inner: check(Process::find())?, during })?;
        broker
            .leases()
            .patch(
                &lease,
                Some(&grant),
                3,
                &FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"base")), content: "approved".into() }),
            )
            .map_err(|e| e.key())?;
        let result = broker.commit(|| Ok(Some(grant.clone())), &lease, "race", "fix: approved fixture", &["src/a.rs".into()]);
        if during == "commit" {
            let head = result.map_err(|e| format!("verified tree commit failed: {}", e.key()))?;
            let program = faber_engine::cli::find_cli("git").ok_or("git unavailable")?.command;
            ensure(git(&program, &root, &["show", &format!("{head}:src/a.rs")])? == "approved", || {
                "commit re-read unreviewed worktree bytes".into()
            })?;
        } else if result != Err(GitError::Lease(LeaseError::Quarantined)) {
            failures.push(format!("edit during {during} was accepted: {result:?}"));
        }
        if matches!(during, "commit" | "commit-dirty") {
            let pending = broker.journal().pending().map_err(|e| e.key())?;
            ensure(pending.is_empty(), || "a certain commit outcome stayed pending".into())?;
        }
    }
    ensure(failures.is_empty(), || failures.join("; "))?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}

struct SwapDisk {
    armed: std::sync::atomic::AtomicBool,
    root: PathBuf,
    redirect: PathBuf,
}
impl faber_engine::store::Disk for SwapDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        faber_engine::store::Disk::write(&faber_engine::store::RealDisk, path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        faber_engine::store::Disk::rename(&faber_engine::store::RealDisk, from, to)?;
        if to.ends_with("runtime.json")
            && self.armed.swap(false, std::sync::atomic::Ordering::SeqCst)
            && std::fs::rename(self.root.join("src"), self.root.join("original-src")).is_ok()
        {
            std::fs::rename(&self.redirect, self.root.join("src"))?;
        }
        Ok(())
    }
}

pub fn probe_path_race(sandbox: &Path) -> Result {
    use faber_engine::project::lease::Leases;
    use std::sync::{Arc, atomic::Ordering};
    let (repo, repository, _) = review_repository(sandbox)?;
    let data = sandbox.join("data");
    let root = data.join("missions/m1/candidate");
    let redirect = data.join("redirect");
    let disk = Arc::new(SwapDisk { armed: false.into(), root: root.clone(), redirect: redirect.clone() });
    let journal = Journal::open_with(&repo, &data, "m1", disk.clone()).map_err(|e| e.key())?;
    let mut leases = Leases::open(repository.identity.clone(), journal, "m1").map_err(|e| e.key())?;
    let request = Lease::new(&repository.identity.common_dir, "m1", "writer", 1, vec!["src".into()], &repository.base, &"a".repeat(64), 0);
    let lease = leases.acquire(request).map_err(|e| e.key())?;
    let mut grant = Grant::new(&repository.identity.common_dir, GitMode::Seguro, 3, 1);
    grant.author = repository.author;
    std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("src/a.rs"), "base").map_err(|e| e.to_string())?;
    leases.bind(&lease, &root).map_err(|e| e.key())?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let powershell = PathBuf::from(std::env::var_os("SystemRoot").ok_or("missing SystemRoot")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let status = Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(sandbox.join("git-junction.ps1"))
            .arg("-Link")
            .arg(&redirect)
            .arg("-Target")
            .arg(repo.join("src"))
            .creation_flags(crate::contain::NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        ensure(status.success(), || "candidate junction fixture failed".into())?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(repo.join("src"), &redirect).map_err(|e| e.to_string())?;
    disk.armed.store(true, Ordering::SeqCst);
    let result = leases.patch(
        &lease,
        Some(&grant),
        3,
        &FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"base")), content: "approved".into() }),
    );
    ensure(std::fs::read(repo.join("src/a.rs")).map_err(|e| e.to_string())? == b"base", || {
        format!("junction redirected broker write: {result:?}")
    })?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}

pub fn probe_cleanup(sandbox: &Path) -> Result {
    use faber_engine::project::{Intent, IntentKind, Outcome, Receipt};
    let (repo, repository, program) = review_repository(sandbox)?;
    let (broker, _, _, _) = review_writer(sandbox, repository.clone(), check(Process::find())?)?;
    let intent = Intent::new("uncertain", "m1", IntentKind::FastForward, 3, 1);
    broker.journal().record_intent(&intent).map_err(|e| e.key())?;
    broker.journal().record_receipt(&Receipt::new("uncertain", Outcome::Unknown, 2)).map_err(|e| e.key())?;
    drop(broker);
    // Only this disposable, clean fixture repository is cleaned. The private sidecar is outside it.
    git(&program, &repo, &["clean", "-fdx"])?;
    ensure(!repo.join(".faber").exists(), || "git clean did not erase the public mirror fixture".into())?;
    let journal = Journal::open(&repo, &sandbox.join("records"), "m1").map_err(|e| e.key())?;
    ensure(journal.pending().map_err(|e| e.key())? == vec![intent.clone()], || "git clean erased an Unknown intent".into())?;
    ensure(journal.record_intent(&intent).is_err(), || "git clean permitted replay".into())?;
    let broker = check(Broker::open(repository, journal, "m1", 3, check(Process::find())?))?;
    ensure(broker.finish() == Err(GitError::Lease(LeaseError::Live)), || "cleaned mission released unreconciled ownership".into())?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}

pub fn probe_mission(sandbox: &Path) -> Result {
    use faber_engine::model::Vendor;
    use faber_engine::project::disk::{ResourceKind, Resources};
    use faber_engine::project::lease::Termination;
    use faber_engine::project::proof::{Definition, ProofSet};
    use faber_engine::project::queue::{Candidate, Review};
    use faber_engine::project::run::Runner;
    use faber_engine::project::{Mission, MissionState, Plan, Task, TaskKind, TaskState};
    let (repo, repository, program) = review_repository(sandbox)?;
    let base = repository.base.clone();
    let identity = repository.identity.common_dir.clone();
    std::fs::write(repo.join("src/a.rs"), "dirty owner").map_err(|e| e.to_string())?;
    let owner_index = std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())?;
    let stable = sandbox.join("stable-install");
    std::fs::create_dir_all(&stable).map_err(|e| e.to_string())?;
    std::fs::write(stable.join("faber.exe"), b"stable fixture, never executable").map_err(|e| e.to_string())?;
    let stable_hash = hash(&std::fs::read(stable.join("faber.exe")).map_err(|e| e.to_string())?);
    let owner_hash = hash(&std::fs::read(repo.join("src/a.rs")).map_err(|e| e.to_string())?);
    let (mut broker, grant, lease, root) = review_writer(sandbox, repository, Traced(check(Process::find())?))?;
    check(broker.create_branch(|| Ok(Some(grant.clone())), "branch"))?;
    let mut runner =
        Runner::create(broker.journal(), Mission::new("m1", "p1", &identity, "Fixture fix", &base, 3, 1)).map_err(str::to_string)?;
    runner.mission.state = MissionState::Running;
    runner.save(broker.journal()).map_err(str::to_string)?;
    let mut task = Task::new("writer", "Fixture fix", TaskKind::Writer);
    task.state = TaskState::Review;
    broker.journal().save_plan(&Plan::new("m1", 1, vec![task])).map_err(|e| e.key())?;
    let mut resources = Resources::load(broker.journal()).map_err(str::to_string)?;
    let build = resources.register_for(broker.journal(), "build-writer", "writer", ResourceKind::Build, None).map_err(str::to_string)?;
    std::fs::create_dir_all(&build).map_err(|e| e.to_string())?;
    std::fs::write(build.join("old-output"), "temporary").map_err(|e| e.to_string())?;
    let definition = Definition {
        id: "fixture-check".into(),
        version: 1,
        program: std::env::current_exe().map_err(|e| e.to_string())?,
        argv: vec!["--project-proof".into(), root.join("src/a.rs").to_string_lossy().into_owned()],
        checks: vec!["C1".into()],
        toolchain: "first-party-fixture".into(),
        dependencies: "none".into(),
        environment: std::collections::BTreeMap::new(),
        heavy: false,
    };
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    let proof_spawn = faber_engine::proc::isolated_spawner(definition.environment.clone());
    let mut red =
        ProofSet::new(base.clone(), "fixture".into(), vec![definition.clone()], vec!["src/a.rs".into()]).map_err(str::to_string)?;
    let red_receipt = runtime
        .block_on(runner.run_proof(broker.journal(), "writer", "fixture-check", &mut red, &root, &proof_spawn))
        .map_err(str::to_string)?;
    ensure(!red.verified(), || "the fixture must begin red".into())?;
    ensure(red_receipt.extra["exit"] == 1, || "missing nonzero red exit".into())?;
    broker
        .leases()
        .patch(
            &lease,
            Some(&grant),
            3,
            &FileOp::Write(WriteOp { path: "src/a.rs".into(), expect: Some(hash(b"base")), content: "after".into() }),
        )
        .map_err(|e| e.key())?;
    let tree = check(broker.compose_tree(|| Ok(Some(grant.clone())), &lease, &["src/a.rs".into()]))?;
    let mut proof = ProofSet::new(tree.clone(), "fixture".into(), vec![definition], vec!["src/a.rs".into()]).map_err(str::to_string)?;
    runtime
        .block_on(runner.run_proof(broker.journal(), "writer", "fixture-check", &mut proof, &root, &proof_spawn))
        .map_err(str::to_string)?;
    let candidate = Candidate::new(
        "writer",
        base.clone(),
        tree.clone(),
        3,
        Vendor::Codex,
        "fixture".into(),
        vec!["src/a.rs".into()],
        proof.definitions().to_vec(),
    )
    .map_err(str::to_string)?;
    runner.queue.enqueue(candidate).map_err(str::to_string)?;
    let review = Review {
        vendor: Vendor::Claude,
        tree: tree.clone(),
        proof_digest: proof.digest(),
        session: "fixture-fresh-review".into(),
        fresh: true,
        accepted: true,
        findings: vec![],
    };
    ensure(runner.queue.review("writer", review.clone()).is_err(), || "review must wait for proof".into())?;
    runner.queue.prove("writer", proof).map_err(str::to_string)?;
    runner.queue.review("writer", review).map_err(str::to_string)?;
    let head = runner
        .land(
            &mut broker,
            "writer",
            &lease,
            || Ok(Some(grant.clone())),
            "Fix the fixture",
            Termination { process: None, tree_dead: true, cost_settled: true },
            |_| Ok(()),
        )
        .map_err(str::to_string)?;
    ensure(!root.exists() && !build.exists(), || "R106 retained unused worktree or build".into())?;
    ensure(git(&program, &repo, &["rev-parse", "faber/m1"])? == head, || "mission ref did not land".into())?;
    ensure(git(&program, &repo, &["show", "faber/m1:src/a.rs"])? == "after", || "wrong landed content".into())?;
    ensure(git(&program, &repo, &["rev-parse", "HEAD"])? == base, || "owner ref moved".into())?;
    ensure(std::fs::read(repo.join(".git/index")).map_err(|e| e.to_string())? == owner_index, || "owner index changed".into())?;
    ensure(std::fs::read(repo.join("src/a.rs")).map_err(|e| e.to_string())? == b"dirty owner", || "owner edit changed".into())?;
    ensure(broker.journal().plan().map_err(|e| e.key())?.unwrap().tasks[0].state == TaskState::Done, || "landing was not recorded".into())?;
    let report_file = repo.join(".faber/missions/m1/report.json");
    ensure(report_file.exists(), || "report.json was not written on landing".into())?;
    let report_content = std::fs::read_to_string(&report_file).map_err(|e| e.to_string())?;
    ensure(report_content.contains("\"tokensPerRole\""), || "report missing tokensPerRole".into())?;
    ensure(report_content.contains("\"checks\""), || "report missing checks".into())?;
    ensure(report_content.contains("\"commitReceipt\""), || "report missing commitReceipt".into())?;
    ensure(report_content.contains("\"treeReceipt\""), || "report missing treeReceipt".into())?;
    ensure(report_content.contains("\"timings\""), || "report missing timings".into())?;
    let report: faber_engine::project::report::MissionReport = serde_json::from_str(&report_content).map_err(|e| e.to_string())?;
    let commit = report.commit_receipt.as_ref().ok_or("missing commit receipt")?;
    ensure(
        report.state == "done"
            && report.checks.len() == 1
            && report.checks[0].result == "pass"
            && commit.head == faber_engine::project::report::UnknownOr::Known(head.clone())
            && commit.tree == faber_engine::project::report::UnknownOr::Known(tree.clone())
            && report.tokens_per_role.values().all(|tokens| tokens == &Default::default()),
        || "persisted report altered exact tree or invented measurements".into(),
    )?;
    ensure(broker.journal().check(&red_receipt.id).map_err(|e| e.key())?.as_ref() == Some(&red_receipt), || {
        "red receipt did not survive persistence".into()
    })?;
    let unchanged = owner_hash == hash(&std::fs::read(repo.join("src/a.rs")).map_err(|e| e.to_string())?)
        && stable_hash == hash(&std::fs::read(stable.join("faber.exe")).map_err(|e| e.to_string())?);
    ensure(unchanged, || "owner or stable fixture hash changed".into())?;
    std::fs::write(
        sandbox.join("t13-scripted.json"),
        serde_json::json!({"red":true,"landing":true,"report":true,"unchanged":unchanged,
            "head":head,"tree":tree,"ownerSha256":owner_hash,"indexSha256":hash(&owner_index),
            "stableSha256":stable_hash,"redReceipt":red_receipt,"reportReceipt":report,
            "scope":"scripted fixture; no qualified provider session"})
        .to_string(),
    )
    .map_err(|e| e.to_string())?;
    check(broker.finish())?;
    std::fs::write(sandbox.join("project-git.ok"), "fixture mission proved").map_err(|e| e.to_string())
}
