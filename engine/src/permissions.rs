//! Project-only approval decisions. Unsupported shell syntax fails closed; this is not an OS sandbox.

use serde_json::{Value, json};
use std::path::{Component, Path, PathBuf};

/// Child commands get no Git transport or GitHub authentication, without reading any credential value.
pub fn child_env(root: &Path) -> Value {
    json!({
        "GIT_ALLOW_PROTOCOL": "", "GIT_TERMINAL_PROMPT": "0", "GIT_OPTIONAL_LOCKS": "0", "GIT_CONFIG_COUNT": "2",
        "GIT_CONFIG_KEY_0": "core.fsmonitor", "GIT_CONFIG_VALUE_0": "false",
        "GIT_CONFIG_KEY_1": "core.hooksPath", "GIT_CONFIG_VALUE_1": "/dev/null",
        "GH_CONFIG_DIR": root.join(".faber/no-gh-auth").to_string_lossy(), "GH_PROMPT_DISABLED": "1",
        "GH_TOKEN": "", "GITHUB_TOKEN": "", "GH_ENTERPRISE_TOKEN": "", "GITHUB_ENTERPRISE_TOKEN": ""
    })
}

fn secret(path: &Path, reading: bool) -> bool {
    let names: Vec<String> =
        path.components().map(|p| p.as_os_str().to_string_lossy().trim_end_matches(['.', ' ']).to_ascii_lowercase()).collect();
    let Some(name) = names.last() else { return true };
    if name.contains("updater.key")
        || [".claude.json", ".git-credentials", ".npmrc", "hosts.yml"].contains(&name.as_str())
        || names
            .iter()
            .any(|p| [".ssh", ".aws", ".claude", ".codex", ".gemini", ".docker", ".azure", ".kube", "no-gh-auth"].contains(&p.as_str()))
    {
        return true;
    }
    reading
        && (name.ends_with(".pem")
            || name.ends_with(".key")
            || name.ends_with(".p12")
            || name.ends_with(".pfx")
            || (name.starts_with(".env") && ![".env.example", ".env.sample", ".env.template", ".env.dist"].contains(&name.as_str())))
}

/// Existing ancestors are resolved too, so a new file under a junction cannot escape the canonical root.
fn target(root: &Path, value: &str, writing: bool, strict: bool) -> Option<PathBuf> {
    if value.is_empty() || value.contains(['\0', '*', '?', '[', ']', '$', '`', '~']) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let path = Path::new(value);
    if path.components().any(|p| matches!(p, Component::ParentDir)) {
        return None;
    }
    #[cfg(windows)]
    if value.starts_with("\\\\")
        || (value.contains(':')
            && (value.matches(':').count() != 1
                || value.as_bytes().get(1) != Some(&b':')
                || !value.as_bytes().get(2).is_some_and(|b| *b == b'/' || *b == b'\\')))
    {
        return None;
    }
    let path = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
    let protected = |path: &Path| {
        path.components().any(|p| {
            let part = p.as_os_str().to_string_lossy();
            let name = part.trim_end_matches(['.', ' ']);
            name.eq_ignore_ascii_case(".git") || name.eq_ignore_ascii_case(".mcp.json")
        })
    };
    if writing && protected(&path) {
        return None;
    }
    let mut parent = path.as_path();
    let mut tail = Vec::new();
    let mut resolved = loop {
        match parent.canonicalize() {
            Ok(path) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A dangling link must not be mistaken for a missing file.
                if parent.symlink_metadata().is_ok() {
                    return None;
                }
                tail.push(parent.file_name()?.to_os_string());
                parent = parent.parent()?;
            }
            Err(_) => return None,
        }
    };
    for part in tail.iter().rev() {
        resolved.push(part);
    }
    if !resolved.starts_with(&root) || (strict && resolved == root) || (writing && protected(&resolved)) || secret(&resolved, !writing) {
        return None;
    }
    Some(resolved)
}

// shortcut: a literal argv subset, not a shell parser; refuse expansions and compound commands until an OS sandbox is available.
fn words(command: &str) -> Option<Vec<String>> {
    if command.len() > 16_384
        || command
            .contains(['$', '`', ';', '|', '&', '<', '>', '\n', '\r', '\0', '(', ')', '{', '}', '*', '?', '[', ']', '%', '!', ',', '~'])
    {
        return None;
    }
    let (mut args, mut word, mut quote, mut started) = (Vec::new(), String::new(), None, false);
    for c in command.chars() {
        if quote == Some(c) {
            quote = None;
        } else if quote.is_some() {
            word.push(c);
        } else if c == '\'' || c == '"' {
            quote = Some(c);
            started = true;
        } else if c.is_whitespace() {
            if started {
                args.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(c);
            started = true;
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        args.push(word);
    }
    (!args.is_empty()).then_some(args)
}

fn git(root: &Path, args: &[String]) -> bool {
    let mut at = 0;
    while let Some(arg) = args.get(at) {
        match arg.as_str() {
            "--no-pager" => at += 1,
            "-C" => {
                let Some(dir) = args.get(at + 1) else { return false };
                if target(root, dir, false, false).is_none() {
                    return false;
                }
                at += 2;
            }
            // Configuration, alternate metadata directories and aliases can run arbitrary programs.
            option if option.starts_with('-') => return false,
            _ => break,
        }
    }
    let Some(sub) = args.get(at) else { return false };
    let rest = &args[at + 1..];
    // Git accepts abbreviated long options, so a blacklist of full names cannot constrain their effects.
    if rest.iter().any(|arg| {
        arg.starts_with('-')
            && ![
                "--",
                "--no-ext-diff",
                "--no-textconv",
                "--stat",
                "--numstat",
                "--shortstat",
                "--name-only",
                "--name-status",
                "--check",
                "--patch",
                "--no-patch",
                "--raw",
                "--summary",
                "--oneline",
                "--graph",
                "--all",
                "--decorate",
                "--no-decorate",
                "--abbrev-commit",
                "--no-abbrev-commit",
                "--color",
                "--no-color",
                "--max-count",
                "--grep",
                "--short",
                "--porcelain",
                "--branch",
                "--show-toplevel",
                "--show-prefix",
                "--verify",
                "--cached",
                "--others",
                "--exclude-standard",
                "-n",
                "-p",
                "-s",
                "-z",
                "-b",
            ]
            .contains(&arg.split('=').next().unwrap())
            && !arg.strip_prefix("-n").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    }) {
        return false;
    }
    match sub.as_str() {
        "status" | "log" | "rev-parse" | "ls-files" => true,
        "diff" | "show" => rest.iter().any(|a| a == "--no-ext-diff") && rest.iter().any(|a| a == "--no-textconv"),
        // Faber owns git mutations. Local history inspection stays available; even a safe-looking commit writes .git.
        _ => false,
    }
}

fn command(root: &Path, value: &str) -> bool {
    let Some(args) = words(value) else { return false };
    let rest = &args[1..];
    match args[0].to_ascii_lowercase().as_str() {
        "git" | "git.exe" => git(root, rest),
        "rm" | "rmdir" | "unlink" | "remove-item" => {
            let mut operands = 0;
            let mut flags = true;
            for arg in rest {
                if flags && arg == "--" {
                    flags = false;
                    continue;
                }
                if flags && arg.starts_with('-') {
                    if !["-r", "-f", "-rf", "-fr", "--recursive", "--force", "-Recurse", "-Force", "-LiteralPath"]
                        .iter()
                        .any(|flag| arg.eq_ignore_ascii_case(flag))
                    {
                        return false;
                    }
                    continue;
                }
                if !deletable(root, arg) {
                    return false;
                }
                operands += 1;
            }
            operands > 0
        }
        // No unsandboxed interpreter, build script, executable wrapper or remote CLI is approved automatically.
        _ => false,
    }
}

fn deletable(root: &Path, value: &str) -> bool {
    safe_tree(root, value, true, true)
}

// Inspect descendants too: recursive deletion and search must not follow a junction into secrets or .git.
fn safe_tree(root: &Path, value: &str, writing: bool, strict: bool) -> bool {
    let Some(path) = target(root, value, writing, strict) else { return false };
    let mut pending = vec![path];
    let mut seen = std::collections::HashSet::new();
    while let Some(path) = pending.pop() {
        // shortcut: inspect at most 10,000 paths; larger trees require an OS sandbox or Ask mode.
        if seen.len() >= 10_000 {
            return false;
        }
        let Some(path) = target(root, &path.to_string_lossy().replace("\\\\?\\", ""), writing, strict) else { return false };
        if !seen.insert(path.clone()) {
            continue;
        }
        if path.is_dir() {
            let Ok(entries) = path.read_dir() else { return false };
            for entry in entries {
                let Ok(entry) = entry else { return false };
                pending.push(entry.path());
                if pending.len() + seen.len() > 10_000 {
                    return false;
                }
            }
        }
    }
    true
}

pub fn allows(root: &Path, tool: &str, input: &Value) -> bool {
    match tool {
        "Read" => input["file_path"].as_str().is_some_and(|p| target(root, p, false, false).is_some()),
        "Write" | "Edit" => input["file_path"].as_str().is_some_and(|p| target(root, p, true, true).is_some()),
        "NotebookEdit" => input["notebook_path"].as_str().is_some_and(|p| target(root, p, true, true).is_some()),
        "Grep" => safe_tree(root, input["path"].as_str().unwrap_or("."), false, false),
        "Glob" => target(root, input["path"].as_str().unwrap_or("."), false, false).is_some(),
        "Bash" | "PowerShell" => input["command"].as_str().is_some_and(|c| command(root, c)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h2_h3_m1_paths_resolve_existing_ancestors_and_never_touch_git_or_the_root() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        for path in ["src/new.rs", "docs/a b.txt", ".env"] {
            assert!(allows(root.path(), "Write", &json!({ "file_path": path })), "{path}");
        }
        for path in [".", "../other", ".git", ".git/config", ".git/hooks/pre-commit", "~/.bashrc", ".claude/settings.json", ".mcp.json"] {
            assert!(!allows(root.path(), "Write", &json!({ "file_path": path })), "{path}");
        }
        assert!(!allows(root.path(), "Write", &json!({ "file_path": outside.path().join("new") })));
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join("escape")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), root.path().join("escape")).unwrap();
        assert!(!allows(root.path(), "Write", &json!({ "file_path": "escape/new" })));
        assert!(!allows(root.path(), "Read", &json!({ "file_path": "escape/key" })));
        std::fs::create_dir(root.path().join("dist")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), root.path().join("dist/escape")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join("dist/escape")).unwrap();
        assert!(!allows(root.path(), "Bash", &json!({ "command": "rm -rf dist" })));
        std::fs::create_dir_all(root.path().join("nested/.git/hooks")).unwrap();
        assert!(!allows(root.path(), "Bash", &json!({ "command": "rm -rf nested" })));
    }

    #[test]
    fn c1_l1_secrets_are_refused_but_environment_templates_and_local_env_writes_work() {
        let root = tempfile::tempdir().unwrap();
        for path in
            [".env", ".env.local", ".env.production", ".env.test.local", "backup-updater.key.old", ".claude.json", ".npmrc", "private.pem"]
        {
            assert!(!allows(root.path(), "Read", &json!({ "file_path": path })), "{path}");
        }
        for path in [".env.example", ".env.sample", ".env.template", ".env.dist", "src/main.rs"] {
            assert!(allows(root.path(), "Read", &json!({ "file_path": path })), "{path}");
        }
        assert!(allows(root.path(), "Write", &json!({ "file_path": ".env" })));
    }

    #[test]
    fn grep_must_not_read_secret_descendants() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".env"), "synthetic secret").unwrap();
        assert!(!allows(root.path(), "Grep", &json!({ "path": ".", "pattern": "secret" })));
    }

    #[test]
    fn configuration_names_cannot_bypass_protection_with_case_or_windows_suffixes() {
        let root = tempfile::tempdir().unwrap();
        for path in [".MCP.JSON", ".ClAuDe/settings.json", ".GIT/config"] {
            assert!(!allows(root.path(), "Write", &json!({ "file_path": path })), "{path}");
        }
        #[cfg(windows)]
        for path in [".git./config", ".mcp.json.", ".claude. /settings.json"] {
            assert!(!allows(root.path(), "Write", &json!({ "file_path": path })), "{path}");
        }
    }

    #[test]
    fn m1_m2_m3_h4_commands_are_decided_by_argv_and_unknown_syntax_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        for cmd in [
            "rm -rf dist src/legacy",
            "rm -- 'a b.txt'",
            "rmdir build",
            "Remove-Item -LiteralPath 'a b.txt' -Recurse -Force",
            "git status",
            "git -C . log --grep push -n 5",
            "git log --grep 'explain rebase flow'",
            "git diff --no-ext-diff --no-textconv",
        ] {
            assert!(allows(root.path(), "Bash", &json!({ "command": cmd })), "{cmd}");
        }
        for cmd in [
            "rm -rf ..",
            "rm -rf .git",
            "rm -rf .",
            "rm -rf $HOME",
            "rm -rf ~",
            "rm -rf dist; rm -rf /",
            "rm -rf dist/*",
            "cmd /c rd /s /q dist",
            "git push",
            "git.exe 'push' origin HEAD",
            "git -C . push origin HEAD",
            "git config core.fsmonitor evil",
            "git -c alias.p=push p",
            "git restore .",
            "git checkout .",
            "git stash drop",
            "git stash clear",
            "git switch -f main",
            "git branch --delete --force old",
            "git reset --merge",
            "git reset --keep",
            "git update-ref -d refs/heads/main",
            "git reflog expire --expire=now",
            "git worktree remove --force other",
            "gh pr merge 42",
            "gh release create v1",
            "gh api -X DELETE repos/o/r",
            "python -c 'open(\"../x\",\"w\")'",
            "npm run build",
            "git log --output=../outside",
            "git diff --ext-diff",
            "git diff --no-ext-diff --no-textconv --out=../outside",
            "git log --textc",
            "git 'sta'",
        ] {
            assert!(!allows(root.path(), "Bash", &json!({ "command": cmd })), "{cmd}");
        }
    }
}
