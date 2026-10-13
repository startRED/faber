//! The remote module's git calls against real git, in a contained helper on the hidden desktop: temporary repositories
//! and a temporary bare repository as the "remote", never a network, a login or a real checkout. The unit tests of
//! `remote::tests` prove the rules against a scripted git; this proves the options mean what the rules assume.
use super::projects::{git, run_probe};
use super::{Result, Scenario, ensure};
use faber_engine::project::RepoIdentity;
use faber_engine::project::remote::{
    RemoteError, Runner, Target, allowed_key, git_net, guard_config, guard_layout, private_dir, private_path, push_call, scan_call,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario {
        name: "remote git: a repository cannot make the scan or the push run its commands, reroute them or blind them",
        run: |ctx| run_probe(ctx, "--remote-git"),
    }]
}

fn program() -> Result<String> {
    Ok(faber_engine::cli::find_cli("git").ok_or("missing git")?.command)
}

/// A marker file is how a command that must not run would show that it did.
fn marker(sandbox: &Path, name: &str) -> PathBuf {
    sandbox.join(format!("ran-{name}"))
}

fn ran(path: &Path) -> bool {
    path.exists()
}

/// A shell script git runs as a program or a hook: it leaves its marker, then does `tail`.
fn script(path: &Path, marker: &Path, tail: &str) -> Result {
    let marker = marker.to_string_lossy().replace('\\', "/");
    std::fs::write(path, format!("#!/bin/sh\necho ran > \"{marker}\"\n{tail}\n")).map_err(|e| e.to_string())
}

fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Real git with none of the module's options and the hooks left on: what an unguarded call would do.
fn raw(program: &str, cwd: &Path, args: &[&str]) -> Result<std::process::Output> {
    let mut command = Command::new(program);
    command
        .current_dir(cwd)
        .args(args)
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

fn repository(program: &str, root: &Path) -> Result<String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    git(program, root, &["init", "--initial-branch=main"])?;
    git(program, root, &["config", "user.name", "Fixture"])?;
    git(program, root, &["config", "user.email", "fixture@example.invalid"])?;
    std::fs::write(root.join("file"), "base").map_err(|e| e.to_string())?;
    git(program, root, &["add", "--", "file"])?;
    git(program, root, &["commit", "--no-gpg-sign", "-m", "base"])?;
    git(program, root, &["rev-parse", "HEAD"])
}

fn target(cwd: &Path, head: &str) -> Target {
    Target {
        cwd: cwd.into(),
        identity: RepoIdentity::new(&slashes(&cwd.join(".git")), &slashes(cwd)),
        revision: 1,
        repo: "owner/repo".into(),
        remote: "origin".into(),
        url: "https://example.invalid/owner/repo.git".into(),
        branch: "faber/m1".into(),
        head: head.into(),
        base: "main".into(),
        attempt: 1,
    }
}

fn process(neutral: &Path) -> faber_engine::project::remote::Process {
    faber_engine::project::remote::Process::find(neutral.into())
}

/// N3: the config of a repository chooses a program to verify signatures, and `log` runs it for a forged one.
fn forged_signature(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("signature");
    let base = repository(program, &repo)?;
    let tree = git(program, &repo, &["rev-parse", "HEAD^{tree}"])?;
    let raw_commit = format!(
        "tree {tree}\nparent {base}\nauthor A <a@example.invalid> 1 +0000\ncommitter A <a@example.invalid> 1 +0000\n\
         gpgsig -----BEGIN PGP SIGNATURE-----\n \n forged\n -----END PGP SIGNATURE-----\n\nforged\n"
    );
    std::fs::write(sandbox.join("forged-commit"), raw_commit).map_err(|e| e.to_string())?;
    let forged = git(program, &repo, &["hash-object", "-t", "commit", "-w", "--", &slashes(&sandbox.join("forged-commit"))])?;
    let ran_gpg = marker(sandbox, "gpg");
    let gpg = sandbox.join("fake-gpg");
    script(&gpg, &ran_gpg, "exit 1")?;
    git(program, &repo, &["config", "log.showSignature", "true"])?;
    git(program, &repo, &["config", "gpg.program", &slashes(&gpg)])?;
    // The control: a scan without the module's options runs the repository's program. Without this the rest proves nothing.
    let control = raw(program, &repo, &["log", "-p", "-1", &forged])?;
    ensure(ran(&ran_gpg), || {
        format!(
            "control: the forged signature did not run the repository's gpg.program on this git: {}",
            String::from_utf8_lossy(&control.stderr)
        )
    })?;
    std::fs::remove_file(&ran_gpg).map_err(|e| e.to_string())?;
    // Layer one: the allowlist refuses the repository before anything is run in it.
    let mut runner = process(&sandbox.join("neutral"));
    ensure(guard_config(&mut runner, &target(&repo, &forged)) == Err(RemoteError::Config), || {
        "log.showSignature and gpg.program in the repository were not refused".into()
    })?;
    ensure(!ran(&ran_gpg), || "reading the config ran gpg.program".into())?;
    // Layer two: even if the config were let through, the scan's own command line shows no signature.
    let reply = runner.run(&scan_call(&repo, &forged, &[])).map_err(|_| "the scan did not run")?;
    ensure(reply.code == 0, || format!("the scan failed with {}", reply.code))?;
    ensure(reply.stdout.contains("forged"), || "the scan did not print the forged commit".into())?;
    ensure(!ran(&ran_gpg), || "the scan ran the repository's gpg.program".into())
}

/// What a clean repository carries passes, and includes, worktree config and everything else is refused.
fn allowlist(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("allowlist");
    let head = repository(program, &repo)?;
    let mut runner = process(&sandbox.join("neutral"));
    let guard = |runner: &mut faber_engine::project::remote::Process| guard_config(runner, &target(&repo, &head));
    git(program, &repo, &["remote", "add", "origin", "https://example.invalid/owner/repo.git"])?;
    git(program, &repo, &["config", "branch.faber/m1.remote", "origin"])?;
    git(program, &repo, &["config", "branch.faber/m1.merge", "refs/heads/faber/m1"])?;
    ensure(guard(&mut runner) == Ok(()), || "an ordinary repository was refused".into())?;
    // Every key real git wrote on its own is on the list, so the list is not missing something `git init` needs.
    let listed = git(program, &repo, &["config", "--local", "--list", "--name-only"])?;
    for key in listed.lines() {
        ensure(allowed_key(key, "origin", "faber/m1"), || format!("git wrote {key} itself and the list refuses it"))?;
    }
    git(program, &repo, &["config", "extensions.worktreeConfig", "true"])?;
    git(program, &repo, &["config", "--worktree", "user.name", "Worktree"])?;
    ensure(guard(&mut runner) == Ok(()), || "a worktree config with a listed key was refused".into())?;
    git(program, &repo, &["config", "--worktree", "gpg.program", "x"])?;
    ensure(guard(&mut runner) == Err(RemoteError::Config), || "an unlisted key in the worktree config was let through".into())?;
    git(program, &repo, &["config", "--worktree", "--unset", "gpg.program"])?;
    ensure(guard(&mut runner) == Ok(()), || "the worktree config did not recover".into())?;
    // An include is followed by `--list`, so what it brings in is judged like the rest, and the include itself is refused.
    std::fs::write(repo.join(".git").join("extra.cfg"), "[user]\n\tname = Included\n").map_err(|e| e.to_string())?;
    git(program, &repo, &["config", "include.path", "extra.cfg"])?;
    ensure(guard(&mut runner) == Err(RemoteError::Config), || "an include in the repository was let through".into())?;
    git(program, &repo, &["config", "--unset", "include.path"])?;
    git(program, &repo, &["config", "includeIf.gitdir:**.path", "extra.cfg"])?;
    ensure(guard(&mut runner) == Err(RemoteError::Config), || "an includeIf in the repository was let through".into())?;
    git(program, &repo, &["config", "--unset", "includeIf.gitdir:**.path"])?;
    for (key, value) in [
        ("core.sshCommand", "x"),
        ("core.hooksPath", "x"),
        ("url.https://evil.invalid/.insteadOf", "https://example.invalid/"),
        ("remote.origin.pushurl", "https://evil.invalid/x.git"),
        ("remote.origin.promisor", "true"),
        ("credential.helper", "x"),
    ] {
        git(program, &repo, &["config", key, value])?;
        ensure(guard(&mut runner) == Err(RemoteError::Config), || format!("{key} in the repository was let through"))?;
        git(program, &repo, &["config", "--unset", key])?;
    }
    ensure(guard(&mut runner) == Ok(()), || "the repository did not recover".into())
}

/// Hooks of the repository, in `.git/hooks` or at a `core.hooksPath` it sets, do not run during a push.
fn hooks(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("hooks");
    let head = repository(program, &repo)?;
    let remote = sandbox.join("hooks-remote.git");
    std::fs::create_dir_all(&remote).map_err(|e| e.to_string())?;
    git(program, &remote, &["init", "--bare", "--initial-branch=main"])?;
    let url = slashes(&remote);
    let (inner, outer) = (marker(sandbox, "pre-push"), marker(sandbox, "pre-push-path"));
    script(&repo.join(".git").join("hooks").join("pre-push"), &inner, "exit 0")?;
    let hooks_path = sandbox.join("hooks-elsewhere");
    std::fs::create_dir_all(&hooks_path).map_err(|e| e.to_string())?;
    script(&hooks_path.join("pre-push"), &outer, "exit 0")?;
    let mut runner = process(&sandbox.join("neutral"));
    for (name, set, ran_marker) in [("in .git/hooks", false, &inner), ("at core.hooksPath", true, &outer)] {
        if set {
            git(program, &repo, &["config", "core.hooksPath", &slashes(&hooks_path)])?;
        }
        // The control: an unguarded push runs the hook on this git.
        let control = raw(program, &repo, &["push", "--porcelain", &url, &format!("{head}:refs/heads/control")])?;
        ensure(control.status.success(), || format!("control push failed: {}", String::from_utf8_lossy(&control.stderr)))?;
        ensure(ran(ran_marker), || format!("control: the hook {name} did not run on an unguarded push"))?;
        std::fs::remove_file(ran_marker).map_err(|e| e.to_string())?;
        let reply = runner.run(&push_call(&repo, &url, &head, "refs/heads/faber/m1")).map_err(|_| "the push did not run")?;
        ensure(reply.code == 0, || format!("the pinned push failed with {}", reply.code))?;
        ensure(!ran(ran_marker), || format!("the pinned push ran the hook {name}"))?;
        git(program, &remote, &["update-ref", "-d", "refs/heads/faber/m1"])?;
        git(program, &remote, &["update-ref", "-d", "refs/heads/control"])?;
    }
    let sent = git(program, &remote, &["rev-parse", "refs/heads/faber/m1"]);
    ensure(sent.is_err(), || "the cleanup left the branch".into())?;
    // And the pinned push does deliver the commit when nothing is wrong.
    let reply = runner.run(&push_call(&repo, &url, &head, "refs/heads/faber/m1")).map_err(|_| "the push did not run")?;
    ensure(reply.code == 0, || format!("the last pinned push failed with {}", reply.code))?;
    ensure(git(program, &remote, &["rev-parse", "refs/heads/faber/m1"])? == head, || "the pinned push did not deliver the commit".into())
}

/// N4: the scan does not fetch from a promisor remote the repository chose.
fn lazy_fetch(program: &str, sandbox: &Path) -> Result {
    let origin = sandbox.join("promisor-origin");
    repository(program, &origin)?;
    for (key, value) in [("uploadpack.allowFilter", "true"), ("uploadpack.allowAnySHA1InWant", "true")] {
        git(program, &origin, &["config", key, value])?;
    }
    let clone = sandbox.join("promisor-clone");
    let source = format!("file:///{}", slashes(&origin).trim_start_matches('/'));
    let made = raw(program, sandbox, &["clone", "--no-local", "--filter=blob:none", "--no-checkout", &source, &slashes(&clone)])?;
    ensure(made.status.success(), || format!("partial clone fixture failed: {}", String::from_utf8_lossy(&made.stderr)))?;
    let missing = |program: &str| -> Result<usize> {
        let listed = git(program, &clone, &["rev-list", "--objects", "--missing=print", "HEAD"])?;
        Ok(listed.lines().filter(|l| l.starts_with('?')).count())
    };
    let before = missing(program)?;
    ensure(before > 0, || "fixture: the partial clone has every blob already".into())?;
    // Guard: the repository's own promisor keys are not on the list.
    let head = git(program, &clone, &["rev-parse", "HEAD"])?;
    let mut runner = process(&sandbox.join("neutral"));
    ensure(guard_config(&mut runner, &target(&clone, &head)) == Err(RemoteError::Config), || {
        "a partial clone's promisor config was let through".into()
    })?;
    // The scan, which the guard would normally precede, still fetches nothing.
    let _ = runner.run(&scan_call(&clone, &head, &[]));
    ensure(missing(program)? == before, || "the scan fetched objects from the promisor remote".into())?;
    // The control: git fetches on its own when nothing says otherwise.
    git(program, &clone, &["log", "-p", "-1", "HEAD"])?;
    ensure(missing(program)? < before, || "control: an unguarded log did not fetch from the promisor remote".into())
}

/// N5a: a bare repository's layout in the folder git starts in is neither adopted nor accepted as the neutral folder.
fn bare_layout(program: &str, sandbox: &Path) -> Result {
    let bare = sandbox.join("bare-neutral");
    std::fs::create_dir_all(&bare).map_err(|e| e.to_string())?;
    git(program, &bare, &["init", "--bare", "--initial-branch=main"])?;
    let control = raw(program, &bare, &["rev-parse", "--git-dir"])?;
    ensure(control.status.success(), || "control: this git does not adopt a bare layout in the working folder".into())?;
    ensure(!private_dir(&bare), || "a bare repository's layout was accepted as the neutral folder".into())?;
    let mut elsewhere = process(&sandbox.join("neutral"));
    let reply = elsewhere.run(&faber_engine::project::remote::git(&bare, &["rev-parse", "--git-dir"])).map_err(|_| "git did not run")?;
    ensure(reply.code != 0, || "the pinned git adopted a bare layout in its working folder".into())
}

/// A junction on Windows, a symlink elsewhere: a folder that is really another one.
fn make_link(sandbox: &Path, link: &Path, target: &Path) -> Result {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let powershell =
            PathBuf::from(std::env::var_os("SystemRoot").ok_or("no SystemRoot")?).join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let made = Command::new(powershell)
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(sandbox.join("git-junction.ps1"))
            .arg("-Link")
            .arg(link)
            .arg("-Target")
            .arg(target)
            .creation_flags(crate::contain::NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        ensure(made.success(), || "could not create the junction fixture".into())
    }
    #[cfg(unix)]
    {
        let _ = sandbox;
        std::os::unix::fs::symlink(target, link).map_err(|e| e.to_string())
    }
}

/// N5b: a link as the neutral folder, or above it, leads git out of the ceiling it is given, into a repository.
fn link(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("linked");
    repository(program, &repo)?;
    std::fs::create_dir_all(repo.join("inside")).map_err(|e| e.to_string())?;
    let parent = sandbox.join("private");
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let link = parent.join("remote");
    make_link(sandbox, &link, &repo.join("inside"))?;
    // A link in the middle of the path: the folder itself is a real one, but it lies in the repository.
    let above = parent.join("above");
    make_link(sandbox, &above, &repo.join("inside"))?;
    let below = above.join("remote");
    ensure(!private_dir(&below) && private_path(&below).is_none(), || "a folder below a link into a repository was accepted".into())?;
    // A link to a harmless folder is refused too: the folder git runs in is then not the one Faber meant.
    let harmless = sandbox.join("harmless");
    std::fs::create_dir_all(&harmless).map_err(|e| e.to_string())?;
    let to_harmless = parent.join("to-harmless");
    make_link(sandbox, &to_harmless, &harmless)?;
    ensure(!private_dir(&to_harmless) && private_path(&to_harmless).is_none(), || "a link to a plain folder was accepted".into())?;
    // The control: git started in the link finds the repository behind it, whatever ceiling it was given.
    let found = raw(program, &link, &["rev-parse", "--show-toplevel"])?;
    ensure(found.status.success(), || "control: git did not find the repository behind the link".into())?;
    ensure(!private_dir(&link) && private_path(&link).is_none(), || "a link was accepted as the neutral folder".into())?;
    let mut runner = process(&link);
    let reply = runner.run(&faber_engine::project::remote::git(&link, &["rev-parse", "--show-toplevel"])).map_err(|_| "no reply")?;
    ensure(reply.code == 126 && reply.stdout.is_empty(), || format!("a read ran from the link: {}", reply.code))?;
    // A plain private folder works with real git, as its real path.
    let plain = sandbox.join("plain-private").join("remote");
    let remote = sandbox.join("plain-remote.git");
    std::fs::create_dir_all(&remote).map_err(|e| e.to_string())?;
    git(program, &remote, &["init", "--bare", "--initial-branch=main"])?;
    let mut runner = process(&plain);
    let reply = runner
        .run(&git_net(&runner.neutral(), &slashes(&remote), &["ls-remote", "--heads", "--tags", &slashes(&remote)]))
        .map_err(|_| "no reply")?;
    ensure(reply.code == 0, || format!("a read from the private folder failed with {}", reply.code))
}

/// Other remotes and branches in the repository's config: git keeps "Origin" and "origin" apart, so the check does too.
fn other_remotes(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("remotes");
    let head = repository(program, &repo)?;
    let mut runner = process(&sandbox.join("neutral"));
    let guard = |runner: &mut faber_engine::project::remote::Process| guard_config(runner, &target(&repo, &head));
    git(program, &repo, &["remote", "add", "origin", "https://example.invalid/owner/repo.git"])?;
    ensure(guard(&mut runner) == Ok(()), || "the granted remote alone was refused".into())?;
    // The control: this git has two remotes here, "Origin" and "origin".
    git(program, &repo, &["config", "remote.Origin.url", "https://attacker.invalid/exfil.git"])?;
    let remotes = git(program, &repo, &["remote"])?;
    ensure(remotes.lines().any(|l| l == "Origin") && remotes.lines().any(|l| l == "origin"), || {
        format!("control: git did not keep Origin and origin apart: {remotes}")
    })?;
    ensure(guard(&mut runner) == Err(RemoteError::Config), || "a remote named Origin was taken for origin".into())?;
    git(program, &repo, &["config", "--unset", "remote.Origin.url"])?;
    for (key, value) in [
        ("remote.mirror.url", "ext::sh -c touch% x"),
        ("remote.leak.url", "https://attacker.invalid/leak.git"),
        ("remote.leak.fetch", "+refs/heads/*:refs/remotes/leak/*"),
        ("branch.main.merge", "refs/heads/main"),
        ("branch.main.remote", "leak"),
    ] {
        git(program, &repo, &["config", key, value])?;
        ensure(guard(&mut runner) == Err(RemoteError::Config), || format!("{key} in the repository was let through"))?;
        git(program, &repo, &["config", "--unset", key])?;
    }
    ensure(guard(&mut runner) == Ok(()), || "the repository did not recover".into())
}

/// The environment of this probe allows every transport and sets config of its own, as a hostile one could. The calls of
/// the module ignore both, and the controls show that git itself would not.
fn protocols(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("protocols");
    let head = repository(program, &repo)?;
    let remote = sandbox.join("protocols-remote.git");
    std::fs::create_dir_all(&remote).map_err(|e| e.to_string())?;
    git(program, &remote, &["init", "--bare", "--initial-branch=main"])?;
    let mut runner = process(&sandbox.join("neutral"));
    // ext:: runs a command. The control: an unguarded push in this environment runs it.
    let ran_ext = marker(sandbox, "ext");
    let ext = format!("ext::sh -c echo% ran% >% {}", slashes(&ran_ext));
    let control = raw(program, &repo, &["push", "--porcelain", &ext, &format!("{head}:refs/heads/x")])?;
    ensure(ran(&ran_ext), || {
        format!("control: an unguarded push to an ext:: URL did not run the command: {}", String::from_utf8_lossy(&control.stderr))
    })?;
    std::fs::remove_file(&ran_ext).map_err(|e| e.to_string())?;
    let reply = runner.run(&push_call(&repo, &ext, &head, "refs/heads/faber/m1")).map_err(|_| "the push did not run")?;
    ensure(reply.code != 0 && !ran(&ran_ext), || "the module's push ran an ext:: URL".into())?;
    // A file:// URL is refused for a call granted for https, though the environment allows file.
    let file_url = format!("file:///{}", slashes(&remote).trim_start_matches('/'));
    let control = raw(program, &repo, &["ls-remote", "--heads", &file_url])?;
    ensure(control.status.success(), || "control: the environment does not allow file:// in this probe".into())?;
    let https = "https://example.invalid/owner/repo.git";
    let reply = runner.run(&git_net(&repo, https, &["ls-remote", "--heads", &file_url])).map_err(|_| "no reply")?;
    ensure(reply.code != 0, || "a call granted for https read a file:// URL".into())?;
    // And a call with no URL of its own may use no transport at all.
    let reply = runner.run(&faber_engine::project::remote::git(&repo, &["ls-remote", "--heads", &file_url])).map_err(|_| "no reply")?;
    ensure(reply.code != 0, || "a call with no granted URL read a file:// URL".into())?;
    // The granted local path itself still works, which shows the refusals are the pins and not a broken setup.
    let reply = runner.run(&git_net(&repo, &file_url, &["ls-remote", "--heads", &file_url])).map_err(|_| "no reply")?;
    ensure(reply.code == 0, || format!("the one granted transport did not work: {}", reply.code))
}

/// Config the environment injects (`GIT_CONFIG_COUNT` and the rest) never reaches a call, in the repository or outside.
fn inherited_config(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("inherited");
    let head = repository(program, &repo)?;
    let listed = raw(program, &repo, &["config", "--list", "--show-scope"])?;
    let listed = String::from_utf8_lossy(&listed.stdout).replace('\r', "");
    ensure(listed.contains("command\thttp.proxy="), || format!("control: the probe's environment injects no config: {listed}"))?;
    let mut runner = process(&sandbox.join("neutral"));
    ensure(guard_config(&mut runner, &target(&repo, &head)) == Ok(()), || {
        "the environment's config reached a call in the repository".into()
    })?;
    let outside = runner
        .run(&faber_engine::project::remote::git(&runner.neutral(), &["config", "--list", "--show-scope"]))
        .map_err(|_| "no reply")?;
    ensure(outside.code == 0 && !outside.stdout.contains("http.proxy"), || {
        "the environment's config reached a call in the neutral folder".into()
    })
}

/// Grafts, a shallow file and alternates change what the scan walks and reads, so none may exist.
fn layout(program: &str, sandbox: &Path) -> Result {
    let repo = sandbox.join("layout");
    let first = repository(program, &repo)?;
    // The second commit adds a marker and the third removes it, so only the history in between shows it.
    std::fs::write(repo.join("second"), "SECRET-MARK").map_err(|e| e.to_string())?;
    git(program, &repo, &["add", "--", "second"])?;
    git(program, &repo, &["commit", "--no-gpg-sign", "-m", "second"])?;
    git(program, &repo, &["rm", "-q", "--", "second"])?;
    git(program, &repo, &["commit", "--no-gpg-sign", "-m", "third"])?;
    let head = git(program, &repo, &["rev-parse", "HEAD"])?;
    let mut runner = process(&sandbox.join("neutral"));
    let guard = |runner: &mut faber_engine::project::remote::Process, at: &Path| guard_layout(runner, &target(at, &head));
    ensure(guard(&mut runner, &repo) == Ok(()), || "an ordinary repository was refused".into())?;
    let scan = |runner: &mut faber_engine::project::remote::Process| -> Result<String> {
        Ok(runner.run(&scan_call(&repo, &head, &[])).map_err(|_| "the scan did not run")?.stdout)
    };
    ensure(scan(&mut runner)?.contains("SECRET-MARK"), || "the scan does not show the commit between".into())?;
    // Grafts: the control is that git, in this version, drops the commit from the walk.
    let grafts = repo.join(".git").join("info").join("grafts");
    std::fs::create_dir_all(grafts.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
    std::fs::write(&grafts, format!("{head} {first}\n")).map_err(|e| e.to_string())?;
    ensure(!scan(&mut runner)?.contains("SECRET-MARK"), || "control: this git ignores info/grafts, so the check proves nothing".into())?;
    ensure(guard(&mut runner, &repo) == Err(RemoteError::Layout), || "info/grafts was let through".into())?;
    std::fs::remove_file(&grafts).map_err(|e| e.to_string())?;
    ensure(guard(&mut runner, &repo) == Ok(()), || "the repository did not recover from grafts".into())?;
    // http-alternates is a file git reads the same way.
    let http = repo.join(".git").join("objects").join("info").join("http-alternates");
    std::fs::create_dir_all(http.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
    std::fs::write(&http, "https://example.invalid/objects\n").map_err(|e| e.to_string())?;
    ensure(guard(&mut runner, &repo) == Err(RemoteError::Layout), || "objects/info/http-alternates was let through".into())?;
    std::fs::remove_file(&http).map_err(|e| e.to_string())?;
    // A link that points nowhere is still a file git would read: it counts.
    let gone = sandbox.join("layout-gone");
    std::fs::create_dir_all(&gone).map_err(|e| e.to_string())?;
    let alternates = repo.join(".git").join("objects").join("info").join("alternates");
    std::fs::create_dir_all(alternates.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
    make_link(sandbox, &alternates, &gone)?;
    std::fs::remove_dir(&gone).map_err(|e| e.to_string())?;
    ensure(guard(&mut runner, &repo) == Err(RemoteError::Layout), || "a dangling link for alternates was let through".into())?;
    std::fs::remove_dir(&alternates).map_err(|e| e.to_string())?;
    // A shallow clone and a shared clone are made by git itself.
    let source = format!("file:///{}", slashes(&repo).trim_start_matches('/'));
    let shallow = sandbox.join("layout-shallow");
    let made = raw(program, sandbox, &["clone", "--depth", "1", "--no-local", &source, &slashes(&shallow)])?;
    ensure(made.status.success(), || format!("fixture: shallow clone failed: {}", String::from_utf8_lossy(&made.stderr)))?;
    ensure(shallow.join(".git").join("shallow").is_file(), || "fixture: the clone is not shallow".into())?;
    ensure(guard(&mut runner, &shallow) == Err(RemoteError::Layout), || "a shallow repository was let through".into())?;
    let shared = sandbox.join("layout-shared");
    let made = raw(program, sandbox, &["clone", "--shared", &slashes(&repo), &slashes(&shared)])?;
    ensure(made.status.success(), || format!("fixture: shared clone failed: {}", String::from_utf8_lossy(&made.stderr)))?;
    ensure(shared.join(".git").join("objects").join("info").join("alternates").is_file(), || {
        "fixture: the clone has no alternates".into()
    })?;
    ensure(guard(&mut runner, &shared) == Err(RemoteError::Layout), || "a repository with alternates was let through".into())?;
    // A linked worktree follows the common dir, where git keeps these files.
    let linked = sandbox.join("layout-linked");
    git(program, &repo, &["worktree", "add", "--detach", "--", &slashes(&linked), "HEAD"])?;
    ensure(guard(&mut runner, &linked) == Ok(()), || "a linked worktree of an ordinary repository was refused".into())?;
    std::fs::write(&grafts, format!("{head} {first}\n")).map_err(|e| e.to_string())?;
    ensure(guard(&mut runner, &linked) == Err(RemoteError::Layout), || {
        "grafts of the common dir were not seen from a linked worktree".into()
    })?;
    std::fs::remove_file(&grafts).map_err(|e| e.to_string())
}

/// Called only by the test helper under `Owned`, with its sandbox environment and hidden desktop.
pub fn probe(sandbox: &Path) -> Result {
    let program = program()?;
    forged_signature(&program, sandbox)?;
    allowlist(&program, sandbox)?;
    hooks(&program, sandbox)?;
    lazy_fetch(&program, sandbox)?;
    bare_layout(&program, sandbox)?;
    link(&program, sandbox)?;
    other_remotes(&program, sandbox)?;
    protocols(&program, sandbox)?;
    inherited_config(&program, sandbox)?;
    layout(&program, sandbox)?;
    std::fs::write(sandbox.join("project-git.ok"), "ok").map_err(|e| e.to_string())
}
