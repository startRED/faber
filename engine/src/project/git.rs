//! Running git without a shell: a fixed argv, a cleaned environment, and every configured filter and hook switched off.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitError {
    Command,
    Process,
}
impl GitError {
    pub fn key(self) -> &'static str {
        match self {
            Self::Command => "reason.git.command",
            Self::Process => "reason.git.unknown",
        }
    }
}

pub fn valid_oid(oid: &str) -> bool {
    [40, 64].contains(&oid.len()) && oid.bytes().all(|b| b.is_ascii_hexdigit()) && oid.bytes().any(|b| b != b'0')
}
pub(crate) fn native_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if let Some(unc) = raw.strip_prefix("\\\\?\\UNC\\") { format!("\\\\{unc}") } else { raw.strip_prefix("\\\\?\\").unwrap_or(&raw).into() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub cwd: PathBuf,
    pub args: Vec<String>,
}
impl Invocation {
    pub(crate) fn new(cwd: &Path, args: &[&str]) -> Self {
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
        Self { cwd: cwd.into(), args: fixed }
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
        command
            .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_LITERAL_PATHSPECS", "1");
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
            &["config", "--includes", "--name-only", "--get-regexp", "^(filter\\..*\\.(clean|smudge|process|required)|hook\\..*)$"],
        );
        // It looks at the same repository the call does: an explicit `--git-dir`/`--work-tree` leads the call's own options.
        let scope = repository_options(&call.args);
        let found = self.command(&query).args(&scope).args(&query.args).output().map_err(|_| GitError::Process)?;
        if !found.status.success() && found.status.code() != Some(1) {
            return Err(GitError::Command);
        }
        let mut command = self.command(call);
        command.args(config_overrides(&found.stdout)?);
        let out = command.args(&call.args).output().map_err(|_| GitError::Process)?;
        Ok(Output { success: out.status.success(), stdout: out.stdout })
    }
}

/// The `--git-dir=` and `--work-tree=` options a call gives before its subcommand (after the fixed `-c` pairs).
fn repository_options(args: &[String]) -> Vec<&String> {
    let mut it = args.iter().peekable();
    while it.next_if(|a| a.as_str() == "-c").is_some() {
        it.next();
    }
    let mut found = vec![];
    while let Some(option) = it.next_if(|a| a.starts_with("--git-dir=") || a.starts_with("--work-tree=")) {
        found.push(option);
    }
    found
}

/// Pure argv construction for command-bearing config found through local, included and worktree config.
pub fn config_overrides(keys: &[u8]) -> Result<Vec<String>, GitError> {
    let keys = std::str::from_utf8(keys).map_err(|_| GitError::Command)?;
    let mut overrides = std::collections::BTreeSet::new();
    for key in keys.lines() {
        if !key.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_/".contains(&b)) {
            return Err(GitError::Command);
        }
        if key.starts_with("filter.") {
            overrides.insert(format!("{key}={}", if key.ends_with(".required") { "false" } else { "" }));
        } else if let Some(name) = key.strip_prefix("hook.").and_then(|s| s.rsplit_once('.').map(|(name, _)| name)) {
            if name.is_empty() {
                return Err(GitError::Command);
            }
            overrides.insert(format!("hook.{name}.enabled=false"));
        } else {
            return Err(GitError::Command);
        }
    }
    Ok(overrides.into_iter().flat_map(|value| ["-c".into(), value]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_query_follows_a_call_that_names_its_repository() {
        let named = Invocation::new(Path::new("w"), &["--git-dir=g", "--work-tree=w", "status", "--git-dir=late"]);
        assert_eq!(repository_options(&named.args), ["--git-dir=g", "--work-tree=w"]);
        assert!(repository_options(&Invocation::new(Path::new("w"), &["status"]).args).is_empty());
    }
}
