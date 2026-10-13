//! Shared working-folder decisions and OS sandbox configuration. Unsupported shell syntax fails closed.

use serde_json::{Map, Value, json};
use std::path::{Component, Path, PathBuf};

/// A fresh name prevents a project's lower-priority settings from adding grants to the host profile.
pub fn sandbox_config(root: &Path, writable: bool) -> Map<String, Value> {
    let id = format!("faber-{}", uuid::Uuid::new_v4().simple());
    let mut profile = json!({ "extends": ":workspace", "filesystem": {
        ":root": "deny", ":minimal": "read", ":tmpdir": "deny", ":slash_tmp": "deny", "glob_scan_max_depth": 3,
        "~/.claude": "deny", "~/.codex": "deny", "~/.gemini": "deny", "~/.ssh": "deny", "~/.aws": "deny",
        "~/.claude.json": "deny", "~/.git-credentials": "deny", "~/.npmrc": "deny", "~/.config/gh": "deny",
        "~/AppData/Roaming/GitHub CLI": "deny", "~/.docker": "deny", "~/.azure": "deny", "~/.kube": "deny",
        ":workspace_roots": { ".": if writable { "write" } else { "read" }, ".git": "read", ".mcp.json": "read",
            ".claude": "deny", ".codex": "deny", ".gemini": "deny", ".ssh": "deny", ".aws": "deny",
            "**/*updater.key*": "deny", "**/.claude.json": "deny", "**/.git-credentials": "deny", "**/.npmrc": "deny",
            "**/hosts.yml": "deny", "**/credentials.toml": "deny", ".env": "deny", ".env.local": "deny",
            ".env.production": "deny", ".env.*.local": "deny" }
    }, "network": { "enabled": false } });
    for name in [".credentials.json", "credentials.json", "auth.json"] {
        profile["filesystem"][":workspace_roots"][format!("**/{name}")] = "deny".into();
    }
    if root.is_absolute() {
        let mut dirs = vec![root.to_path_buf()];
        let mut left = 10_000usize;
        'scan: while let Some(dir) = dirs.pop() {
            let entries = std::fs::read_dir(&dir).ok();
            if left == 0 || entries.is_none() {
                // A partial scan must not grant writes around unknown repository metadata.
                profile["filesystem"][":workspace_roots"]["."] = "read".into();
                break;
            }
            left -= 1;
            for entry in entries.unwrap() {
                let Ok(entry) = entry else {
                    profile["filesystem"][":workspace_roots"]["."] = "read".into();
                    break 'scan;
                };
                let path = entry.path();
                let Some(relative) = path.strip_prefix(root).ok().and_then(Path::to_str) else {
                    profile["filesystem"][":workspace_roots"]["."] = "read".into();
                    break 'scan;
                };
                if entry.file_name().to_string_lossy().trim_end_matches(['.', ' ']).eq_ignore_ascii_case(".git") {
                    profile["filesystem"][":workspace_roots"][relative.replace('\\', "/")] = "read".into();
                } else if !secret(&path, true) {
                    let Ok(kind) = entry.file_type() else {
                        profile["filesystem"][":workspace_roots"]["."] = "read".into();
                        break 'scan;
                    };
                    if kind.is_dir() && !kind.is_symlink() {
                        dirs.push(path);
                    }
                }
            }
        }
    }
    let mut config = json!({ "default_permissions": id, format!("permissions.{id}"): profile,
        "sandbox_workspace_write.network_access": false, "sandbox_workspace_write.writable_roots": [], "allow_login_shell": false,
        "shell_environment_policy.set": child_env(root), "shell_environment_policy.ignore_default_excludes": false
    })
    .as_object()
    .unwrap()
    .clone();
    if cfg!(windows) {
        config.insert("windows.sandbox".into(), "mxc".into());
    }
    config
}

pub fn prepare_tmp(root: &Path) -> bool {
    target(root, ".faber/tmp", true, true).is_some_and(|path| std::fs::create_dir_all(path).is_ok())
}

/// Canonical local roots reuse the file policy's network-link and credential guards.
pub fn working_root(root: &Path) -> Option<PathBuf> {
    let path = target(root, ".", false, false)?;
    #[cfg(windows)]
    if let Some(local) = path.to_str()?.strip_prefix("\\\\?\\") {
        return Some(PathBuf::from(local));
    }
    Some(path)
}

/// Broad personal/system roots are permitted only when the person keeps command approvals on.
pub fn unattended_root(root: &Path) -> Option<PathBuf> {
    let root = working_root(root)?;
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let mut system: Vec<PathBuf> = ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "APPDATA", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .collect();
    if cfg!(windows)
        && let Some(home) = &home
    {
        system.push(home.join("AppData"));
    }
    (!sensitive_root(&root, home.as_deref(), &system)).then_some(root)
}

fn sensitive_root(root: &Path, home: Option<&Path>, system: &[PathBuf]) -> bool {
    let same = |a: &Path, b: &Path| {
        if cfg!(windows) {
            a.to_string_lossy().trim_start_matches("\\\\?\\").eq_ignore_ascii_case(b.to_string_lossy().trim_start_matches("\\\\?\\"))
        } else {
            a == b
        }
    };
    root.parent().is_none()
        || home.is_some_and(|home| same(root, &home.canonicalize().unwrap_or_else(|_| home.to_path_buf())))
        || system.iter().any(|dir| {
            let dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            root.ancestors().any(|ancestor| same(ancestor, &dir))
        })
}

/// Only Faber's scratch temporary directory is disposable. Never follow a substituted link during cleanup.
pub fn clean_tmp(root: &Path) {
    let path = root.join(".faber/tmp");
    if [root.join(".faber"), path.clone()].iter().all(|p| p.symlink_metadata().is_ok_and(|m| !m.file_type().is_symlink()))
        && target(root, ".faber/tmp", true, true).is_some()
    {
        let _ = std::fs::remove_dir_all(path);
    }
}

fn inline(value: &Value) -> String {
    match value {
        Value::Object(table) => {
            format!("{{{}}}", table.iter().map(|(key, value)| format!("{}={}", literal(key), inline(value))).collect::<Vec<_>>().join(","))
        }
        Value::String(value) => literal(value),
        // Host-generated scalars are strings, booleans or integers, whose JSON syntax is also valid TOML.
        _ => value.to_string(),
    }
}

fn literal(value: &str) -> String {
    // Single-quoted TOML survives legacy PowerShell's native argument forwarding.
    if value.contains('\'') || value.contains(['\n', '\r']) { json!(value).to_string() } else { format!("'{value}'") }
}

fn quoted(value: &str, powershell: bool) -> String {
    format!("'{}'", if powershell { value.replace('\'', "''") } else { value.replace('\'', "'\\''") })
}

/// Build tools execute project code only inside an OS sandbox; never approve the original command as a fallback.
pub fn checked_input(
    root: &Path,
    tool: &str,
    input: &Value,
    sandbox: Option<&crate::cli::Cli>,
    resolve: &dyn Fn(&str) -> Option<crate::cli::Cli>,
) -> Option<Value> {
    let shell = ["Bash", "PowerShell"].contains(&tool);
    let allowed = allows(root, tool, input);
    if allowed && !shell {
        return Some(input.clone());
    }
    if !shell {
        return None;
    }
    let command = command_argv(root, tool, input, sandbox, resolve)?;
    let mut updated = input.clone();
    let command = command.iter().map(|arg| quoted(arg, tool == "PowerShell")).collect::<Vec<_>>().join(" ");
    updated["command"] = format!("{}{command}", if tool == "PowerShell" { "& " } else { "" }).into();
    if updated.get("run_in_background").is_some() {
        updated["run_in_background"] = false.into();
    }
    Some(updated)
}

/// The same validated argv feeds both vendors' sandboxed command tools.
pub fn command_argv(
    root: &Path,
    tool: &str,
    input: &Value,
    sandbox: Option<&crate::cli::Cli>,
    resolve: &dyn Fn(&str) -> Option<crate::cli::Cli>,
) -> Option<Vec<String>> {
    if !["Bash", "PowerShell"].contains(&tool) {
        return None;
    }
    let allowed = allows(root, tool, input);
    let mut args = words(input["command"].as_str()?)?;
    let program = args[0].to_ascii_lowercase();
    let sub = args.get(1).map(String::as_str).unwrap_or_default();
    let build = allowed
        || match program.as_str() {
            "cargo" | "cargo.exe" => ["build", "check", "test", "clippy", "fmt", "t"].contains(&sub),
            "npm" | "pnpm" | "yarn" | "bun" => ["run", "test", "build", "ci", "install"].contains(&sub),
            "dotnet" | "go" => ["build", "test", "run"].contains(&sub),
            "ls" | "rg" | "pwd" => true,
            "node" | "python" | "python3" | "bash" => !sub.starts_with('-') && target(root, sub, false, true).is_some(),
            "pwsh" | "powershell" => {
                args.windows(2).any(|pair| pair[0].eq_ignore_ascii_case("-File") && target(root, &pair[1], false, true).is_some())
                    && !args.iter().any(|arg| ["-c", "-Command", "-EncodedCommand"].iter().any(|flag| arg.eq_ignore_ascii_case(flag)))
            }
            _ => program.starts_with("./") && target(root, &args[0], true, true).is_some(),
        };
    if !build {
        return None;
    }
    let cli = sandbox?;
    // A JavaScript launcher could execute host NODE_OPTIONS before the sandbox exists.
    if !cli.prefix.is_empty() {
        return None;
    }
    // Windows cannot execute npm's .cmd shim directly; reuse the driver's native executable resolution.
    if cfg!(windows) && ["npm", "pnpm", "yarn", "bun"].contains(&program.as_str()) {
        let native = resolve(&args[0])?;
        args = std::iter::once(native.command).chain(native.prefix).chain(args.into_iter().skip(1)).collect();
    }
    if cfg!(windows) && ["rm", "rmdir", "unlink", "remove-item"].contains(&program.as_str()) {
        let (mut flags, mut recurse, mut force, mut paths) = (true, false, false, Vec::new());
        for arg in args.iter().skip(1) {
            if flags && arg == "--" {
                flags = false;
                continue;
            }
            if flags && arg.starts_with('-') {
                recurse |= ["-r", "-rf", "-fr", "--recursive", "-Recurse"].iter().any(|flag| arg.eq_ignore_ascii_case(flag));
                force |= ["-f", "-rf", "-fr", "--force", "-Force"].iter().any(|flag| arg.eq_ignore_ascii_case(flag));
            } else {
                paths.push(quoted(arg, true));
            }
        }
        let script = format!(
            "Microsoft.PowerShell.Management\\Remove-Item -LiteralPath @({}){}{}",
            paths.join(","),
            if recurse { " -Recurse" } else { "" },
            if force { " -Force" } else { "" }
        );
        args = vec!["powershell.exe".into(), "-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), script];
    }
    #[cfg(windows)]
    if !local_path(root.to_str()?) {
        return None;
    }
    #[cfg(windows)]
    if !local_links(root, &probe_link, 40) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let root_text = root.to_string_lossy().replace("\\\\?\\", "");
    let mut command = vec![cli.command.clone()];
    let config = sandbox_config(Path::new(&root_text), true);
    let profile = config["default_permissions"].as_str()?.to_owned();
    for (key, value) in config {
        command.extend(["-c".into(), format!("{key}={}", inline(&value))]);
    }
    command.extend([
        "sandbox".into(),
        "--permission-profile".into(),
        profile,
        "--include-managed-config".into(),
        "-C".into(),
        root_text,
        "--".into(),
    ]);
    command.extend(args);
    Some(command)
}

/// Child commands get no Git transport or GitHub authentication, without reading any credential value.
pub fn child_env(root: &Path) -> Value {
    let tmp = root.join(".faber/tmp");
    json!({
        "GIT_ALLOW_PROTOCOL": "", "GIT_TERMINAL_PROMPT": "0", "GIT_OPTIONAL_LOCKS": "0", "GIT_CONFIG_COUNT": "2", "GIT_CONFIG_PARAMETERS": "",
        "GIT_CONFIG_KEY_0": "core.fsmonitor", "GIT_CONFIG_VALUE_0": "false",
        "GIT_CONFIG_KEY_1": "core.hooksPath", "GIT_CONFIG_VALUE_1": "/dev/null",
        "GH_CONFIG_DIR": root.join(".faber/no-gh-auth").to_string_lossy(), "GH_PROMPT_DISABLED": "1",
        "GH_TOKEN": "", "GITHUB_TOKEN": "", "GH_ENTERPRISE_TOKEN": "", "GITHUB_ENTERPRISE_TOKEN": "",
        "TMP": tmp.to_string_lossy(), "TEMP": tmp.to_string_lossy(), "TMPDIR": tmp.to_string_lossy(),
        "NODE_OPTIONS": "--preserve-symlinks --preserve-symlinks-main"
    })
}

fn secret(path: &Path, reading: bool) -> bool {
    let names: Vec<String> =
        path.components().map(|p| p.as_os_str().to_string_lossy().trim_end_matches(['.', ' ']).to_ascii_lowercase()).collect();
    let Some(name) = names.last() else { return true };
    if name.contains("updater.key")
        || [".claude.json", ".credentials.json", "credentials.json", "auth.json", ".git-credentials", ".npmrc", "hosts.yml"]
            .contains(&name.as_str())
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

#[cfg(windows)]
fn local_path(value: &str) -> bool {
    let path = Path::new(value);
    match path.components().next() {
        Some(Component::Prefix(prefix)) => {
            matches!(prefix.kind(), std::path::Prefix::Disk(_)) && path.is_absolute() && value.matches(':').count() == 1
        }
        Some(Component::RootDir) => false,
        _ => !value.contains(':'),
    }
}

#[cfg(windows)]
fn local_links(path: &Path, probe: &dyn Fn(&Path) -> std::io::Result<Option<PathBuf>>, left: usize) -> bool {
    if left == 0 {
        return false;
    }
    let Some(text) = path.to_str() else { return false };
    let text = if matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::VerbatimDisk(_))) {
        &text[4..]
    } else {
        text
    };
    if !local_path(text) {
        return false;
    }
    let Ok(path) = std::path::absolute(text) else { return false };
    let mut part = PathBuf::new();
    for component in path.components() {
        part.push(component.as_os_str());
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match probe(&part) {
            Ok(Some(link)) => {
                if link.components().any(|c| matches!(c, Component::ParentDir)) {
                    return false;
                }
                let link = if link.is_absolute() { link } else { part.parent().unwrap_or(Path::new("")).join(link) };
                if !local_links(&link, probe, left - 1) {
                    return false;
                }
            }
            Ok(None) => {}
            Err(_) => return false,
        }
    }
    true
}

#[cfg(windows)]
fn probe_link(path: &Path) -> std::io::Result<Option<PathBuf>> {
    match path.symlink_metadata() {
        Ok(meta) if meta.file_type().is_symlink() => std::fs::read_link(path).map(Some),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Existing ancestors are resolved too, so a new file under a junction cannot escape the canonical root.
fn target(root: &Path, value: &str, writing: bool, strict: bool) -> Option<PathBuf> {
    if value.is_empty() || value.contains(['\0', '*', '?', '[', ']', '$', '`', '~']) {
        return None;
    }
    #[cfg(windows)]
    if !local_path(value) || !local_path(root.to_str()?) {
        return None;
    }
    #[cfg(windows)]
    if !local_links(root, &probe_link, 40) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let path = Path::new(value);
    if path.components().any(|p| matches!(p, Component::ParentDir)) {
        return None;
    }
    let path = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
    #[cfg(windows)]
    if !local_links(&path, &probe_link, 40) {
        return None;
    }
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
    let Some(root) = working_root(root) else { return false };
    // Validate user operands relative to the canonical root. A trusted Windows root can contain an 8.3
    // spelling (RUNNER~1), or a literal tilde, without making a relative operand a shell expansion.
    let checked = |path: &Path| {
        let value = match path.strip_prefix(&root) {
            Ok(relative) if relative.as_os_str().is_empty() => ".".into(),
            Ok(relative) => relative.to_string_lossy().into_owned(),
            Err(_) => path.to_string_lossy().replace("\\\\?\\", ""),
        };
        target(&root, &value, false, false)
    };
    let mut at = 0;
    let mut cwd = root.to_path_buf();
    while let Some(arg) = args.get(at) {
        match arg.as_str() {
            "--no-pager" => at += 1,
            "-C" => {
                let Some(dir) = args.get(at + 1) else { return false };
                let path = if Path::new(dir).is_absolute() { PathBuf::from(dir) } else { cwd.join(dir) };
                let Some(path) = checked(&path) else { return false };
                cwd = path;
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
    let mut positional = 0;
    while let Some(arg) = rest.get(positional) {
        if ["--grep", "-n", "--max-count"].contains(&arg.as_str()) {
            if rest.get(positional + 1).is_none() {
                return false;
            }
            positional += 2;
        } else {
            // A second diff operand outside the worktree implicitly enables --no-index in Git.
            if !arg.starts_with('-') {
                let path = if Path::new(arg).is_absolute() { PathBuf::from(arg) } else { cwd.join(arg) };
                if checked(&path).is_none() {
                    return false;
                }
            }
            positional += 1;
        }
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
        "WebSearch" | "TodoWrite" => true,
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
    fn review3_sensitive_roots_include_canonical_home_drive_and_system_subtrees() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let system: Vec<PathBuf> = ["Windows", "Program Files", "AppData"].into_iter().map(|name| dir.path().join(name)).collect();
        std::fs::create_dir(&home).unwrap();
        for root in &system {
            std::fs::create_dir_all(root.join("nested")).unwrap();
        }
        assert!(sensitive_root(&working_root(&home).unwrap(), Some(&home), &system));
        for root in &system {
            assert!(sensitive_root(&working_root(&root.join("nested")).unwrap(), Some(&home), &system));
        }
        let root = working_root(dir.path()).unwrap();
        assert!(sensitive_root(root.ancestors().last().unwrap(), Some(&home), &system));
        assert!(!sensitive_root(&root.join("Windows-other"), Some(&home), &system));
        assert!(!sensitive_root(&home.join("project"), Some(&home), &system));
    }

    #[test]
    #[cfg(windows)]
    fn network_link_targets_are_rejected_before_lookup_below_the_link() {
        let seen = std::cell::RefCell::new(Vec::new());
        let probe = |path: &Path| {
            seen.borrow_mut().push(path.to_path_buf());
            Ok((path == Path::new("E:/project/link")).then(|| PathBuf::from("\\\\?\\UNC\\host\\share")))
        };
        assert!(!local_links(Path::new("E:/project/link/key"), &probe, 40));
        assert!(!seen.borrow().iter().any(|p| p == Path::new("E:/project/link/key")));
    }

    #[test]
    fn project_temp_files_stay_in_the_root_and_cannot_follow_a_junction_outside() {
        let root = tempfile::tempdir().unwrap();
        assert!(prepare_tmp(root.path()));
        assert!(root.path().join(".faber/tmp").is_dir());
        let linked = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), linked.path().join(".faber")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), linked.path().join(".faber")).unwrap();
        assert!(!prepare_tmp(linked.path()));
        assert!(!outside.path().join("tmp").exists());
    }

    #[test]
    fn review2_h3_builds_tests_and_project_tools_get_a_sandboxed_command() {
        let root = tempfile::tempdir().unwrap();
        let cli = crate::cli::Cli { command: "C:/Program Files/Tools/codex.exe".into(), prefix: vec![] };
        let resolve = |name: &str| Some(crate::cli::Cli { command: format!("C:/Tools/{name}.exe"), prefix: vec![] });
        for command in ["cargo t --all", "cargo test", "npm run build", "npm test", "rg pattern src", "./scripts/check"] {
            let input = json!({ "command": command, "timeout": 120000 });
            let updated = checked_input(root.path(), "Bash", &input, Some(&cli), &resolve).expect(command);
            let rewritten = updated["command"].as_str().unwrap();
            assert!(rewritten.contains("sandbox"), "{rewritten}");
            if cfg!(windows) {
                assert!(rewritten.contains("mxc"), "{rewritten}");
            }
            assert!(rewritten.contains("--include-managed-config"), "{rewritten}");
            assert_eq!(updated["timeout"], input["timeout"]);
            assert!(checked_input(root.path(), "Bash", &input, None, &resolve).is_none());
        }
        for command in ["git push", "gh pr merge 42", "rm -rf ..", "npm run build; cat ../key", "python -c evil"] {
            assert!(checked_input(root.path(), "Bash", &json!({ "command": command }), Some(&cli), &resolve).is_none(), "{command}");
        }
        let updated = checked_input(root.path(), "PowerShell", &json!({ "command": "cargo test" }), Some(&cli), &resolve).unwrap();
        assert!(updated["command"].as_str().unwrap().starts_with("& "));
        let node_helper = crate::cli::Cli { command: "node".into(), prefix: vec!["codex.js".into()] };
        assert!(checked_input(root.path(), "Bash", &json!({ "command": "cargo test" }), Some(&node_helper), &resolve).is_none());
        let git = checked_input(root.path(), "Bash", &json!({ "command": "git status" }), Some(&cli), &resolve).unwrap();
        assert!(git["command"].as_str().unwrap().contains("sandbox"), "a project can shadow a bare executable name");
        let background =
            checked_input(root.path(), "Bash", &json!({ "command": "cargo test", "run_in_background": true }), Some(&cli), &resolve)
                .unwrap();
        assert_eq!(background["run_in_background"], false);
        #[cfg(windows)]
        for (command, recursive) in [("rm -rf dist", true), ("rm -- '-old'", false)] {
            let updated = checked_input(root.path(), "Bash", &json!({ "command": command }), Some(&cli), &resolve).unwrap();
            let command = updated["command"].as_str().unwrap();
            assert!(command.contains("Remove-Item"));
            assert_eq!(command.contains("-Recurse"), recursive);
            if !recursive {
                assert!(command.contains("-old"));
            }
        }
    }

    #[test]
    fn review2_h1_git_positionals_cannot_read_external_or_secret_files() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("README.md"), "synthetic project").unwrap();
        std::fs::write(outside.path().join("updater.key"), "synthetic key").unwrap();
        for command in [
            format!("git diff --no-ext-diff --no-textconv {} README.md", outside.path().join("updater.key").display()),
            "git diff --no-ext-diff --no-textconv ../outside README.md".into(),
            "git show --no-ext-diff --no-textconv .env".into(),
            "git log -- ../outside".into(),
            "git ls-files -- ../outside".into(),
        ] {
            assert!(!allows(root.path(), "Bash", &json!({ "command": command })), "{command}");
        }
        assert!(allows(root.path(), "Bash", &json!({ "command": "git diff --no-ext-diff --no-textconv HEAD README.md" })));
        assert!(allows(root.path(), "Bash", &json!({ "command": "git log --grep push -n 5" })));
    }

    #[test]
    fn review2_h1_git_positionals_use_the_directory_selected_with_c() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        #[cfg(windows)]
        junction::create(outside.path(), root.path().join("src/escape")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join("src/escape")).unwrap();
        assert!(!allows(
            root.path(),
            "Bash",
            &json!({ "command": "git -C src diff --no-ext-diff --no-textconv escape/private.txt README.md" })
        ));
    }

    #[test]
    fn git_relative_arguments_work_when_the_trusted_root_contains_a_tilde() {
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("working~1");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("README.md"), "synthetic project").unwrap();
        for command in ["git -C . log --grep push -n 5", "git diff --no-ext-diff --no-textconv HEAD README.md"] {
            assert!(allows(&root, "Bash", &json!({ "command": command })), "{command}");
        }
        assert!(!allows(&root, "Bash", &json!({ "command": "git -C ~/outside status" })));
        assert!(!allows(&root, "Bash", &json!({ "command": "git log -- ../outside" })));
    }

    #[test]
    #[cfg(windows)]
    fn review2_m1_network_prefixes_are_rejected_without_filesystem_lookup() {
        for value in ["//h/s", "\\/h/s", "//./C:/x", "\\\\h\\s", "\\\\?\\C:\\x", "C:relative", "\\drive-relative"] {
            assert!(!local_path(value), "{value}");
        }
        for value in ["E:/project/file", "src/file", "."] {
            assert!(local_path(value), "{value}");
        }
    }

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
