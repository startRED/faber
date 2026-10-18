//! The git executor runs in a contained helper on the hidden desktop, never in unit tests or a real checkout.
use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::sandbox::{Launch, child_env};
use faber_engine::project::git::{Executor, Invocation, Process};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "projects git: the filters a repository configures never run through the git executor", run: local_git }]
}
fn local_git(ctx: &mut Ctx) -> Result {
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
    let args = vec!["--project-git".into(), ctx.sb.sandbox.to_string_lossy().into()];
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
        // Nothing of the person's git reaches the fixture: no inherited GIT_* (a GIT_DIR would write elsewhere), and the
        // home and config folders are the fixture's own, so no global attributes or ignore files apply.
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, _) in std::env::vars_os().filter(|(k, _)| k.to_string_lossy().to_ascii_uppercase().starts_with("GIT_")) {
        command.env_remove(key);
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::contain::NO_WINDOW);
    }
    command.output().map_err(|e| e.to_string())
}

/// Called only by the test helper under `Owned`, with its sandbox environment and hidden desktop.
pub fn probe(sandbox: &Path) -> Result {
    let program = faber_engine::cli::find_cli("git").ok_or("git is unavailable in the sandbox")?.command;
    let repo = sandbox.join("repo");
    std::fs::create_dir(&repo).map_err(|e| e.to_string())?;
    git(&program, &repo, &["init", "--initial-branch=owner"])?;
    std::fs::write(repo.join("a.txt"), "base").map_err(|e| e.to_string())?;
    std::fs::write(repo.join("b.txt"), "base").map_err(|e| e.to_string())?;
    std::fs::write(repo.join(".gitattributes"), "a.txt filter=trap\nb.txt filter=include\n").map_err(|e| e.to_string())?;
    // A required filter that fails when it runs: the repository's own config and an included file both name one.
    for setting in ["filter.trap.clean", "filter.trap.smudge", "filter.trap.process"] {
        git(&program, &repo, &["config", setting, "exit 99"])?;
    }
    git(&program, &repo, &["config", "filter.trap.required", "true"])?;
    std::fs::write(
        repo.join(".git/extra-config"),
        "[filter \"include\"]\n process = exit 99\n clean = exit 99\n smudge = exit 99\n required = true\n",
    )
    .map_err(|e| e.to_string())?;
    git(&program, &repo, &["config", "include.path", "extra-config"])?;
    let mut process = Process::find().map_err(|e| e.key().to_string())?;
    let call = Invocation { cwd: repo.clone(), args: ["add", "--", "a.txt", "b.txt"].map(String::from).to_vec() };
    let staged = process.execute(&call).map_err(|e| e.key().to_string())?;
    ensure(staged.success, || "a repository's filter ran through the git executor".into())?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}
