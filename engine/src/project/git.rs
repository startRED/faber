//! Running git without a shell: a fixed argv, a cleaned environment, and every configured filter and hook switched off.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

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
    pub deadline: Option<Instant>,
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
        Self { cwd: cwd.into(), args: fixed, deadline: None }
    }
    pub(crate) fn before(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
}
pub trait Executor {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError>;
    /// At most `limit + 1` bytes; the extra byte tells a reader the output did not fit.
    fn execute_bounded(&mut self, call: &Invocation, limit: usize) -> Result<Output, GitError> {
        self.execute_window(call, 0, limit)
    }
    /// Discards a prefix while keeping at most `limit + 1` bytes of a paginated name listing.
    fn execute_window(&mut self, call: &Invocation, skip: usize, limit: usize) -> Result<Output, GitError> {
        if call.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err(GitError::Command);
        }
        let mut out = self.execute(call)?;
        out.stdout.drain(..skip.min(out.stdout.len()));
        out.stdout.truncate(limit.saturating_add(1));
        Ok(out)
    }
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
const COMMAND_TIME: Duration = Duration::from_secs(10);

fn successful(call: &Invocation, code: Option<i32>) -> bool {
    // diff --no-index uses exit 1 for a successful comparison that found differences.
    code == Some(0) || (code == Some(1) && call.args.iter().any(|arg| arg == "--no-index"))
}

impl Executor for Process {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        // Existing copy operations can take longer than a read-only changes request.
        if call.deadline.is_some() {
            return self.execute_bounded(call, usize::MAX);
        }
        let query = Invocation::new(
            &call.cwd,
            &["config", "--includes", "--name-only", "--get-regexp", "^(filter\\..*\\.(clean|smudge|process|required)|hook\\..*)$"],
        );
        let scope = repository_options(&call.args);
        let found = self.command(&query).args(&scope).args(&query.args).output().map_err(|_| GitError::Process)?;
        if !found.status.success() && found.status.code() != Some(1) {
            return Err(GitError::Command);
        }
        let mut command = self.command(call);
        command.args(config_overrides(&found.stdout)?);
        let out = command.args(&call.args).output().map_err(|_| GitError::Process)?;
        Ok(Output { success: successful(call, out.status.code()), stdout: out.stdout })
    }

    fn execute_window(&mut self, call: &Invocation, skip: usize, limit: usize) -> Result<Output, GitError> {
        let now = Instant::now();
        let deadline = call.deadline.unwrap_or(now + COMMAND_TIME).min(now + COMMAND_TIME);
        if deadline <= now {
            return Err(GitError::Command);
        }
        // Discover only command-bearing config names, then disable every configured filter and hook.
        let query = Invocation::new(
            &call.cwd,
            &["config", "--includes", "--name-only", "--get-regexp", "^(filter\\..*\\.(clean|smudge|process|required)|hook\\..*)$"],
        );
        let scope = repository_options(&call.args);
        let found = capped(self.command(&query).args(&scope).args(&query.args), 0, 64 * 1024, deadline)?;
        if found.0 != Some(0) && found.0 != Some(1) || found.1.len() > 64 * 1024 {
            return Err(GitError::Command);
        }
        let mut command = self.command(call);
        command.args(config_overrides(&found.1)?).args(&call.args);
        let (code, stdout) = capped(&mut command, skip, limit, deadline)?;
        Ok(Output { success: successful(call, code) || stdout.len() > limit, stdout })
    }
}

trait RunningChild {
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>>;
    fn kill(&mut self) -> std::io::Result<()>;
    fn wait(&mut self) -> std::io::Result<ExitStatus>;
}
impl RunningChild for Child {
    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        Child::try_wait(self)
    }
    fn kill(&mut self) -> std::io::Result<()> {
        Child::kill(self)
    }
    fn wait(&mut self) -> std::io::Result<ExitStatus> {
        Child::wait(self)
    }
}

fn stop(child: &mut impl RunningChild) {
    let _ = child.kill();
    let _ = child.wait();
}

fn supervise(
    child: &mut impl RunningChild,
    read: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
    limit: usize,
    deadline: Instant,
) -> Result<(Option<i32>, Vec<u8>), GitError> {
    let mut bytes = None;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            stop(child);
            return Err(GitError::Command);
        }
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(_) => {
                stop(child);
                return Err(GitError::Process);
            }
        };
        if let Some(data) = bytes.take() {
            if let Some(status) = status {
                return Ok((status.code(), data));
            }
            bytes = Some(data);
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
            continue;
        }
        match read.recv_timeout(remaining.min(Duration::from_millis(5))) {
            Ok(Ok(data)) if data.len() > limit => {
                stop(child);
                return Ok((None, data));
            }
            Ok(Ok(data)) => bytes = Some(data),
            Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                stop(child);
                return Err(GitError::Process);
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn capped(command: &mut Command, skip: usize, limit: usize, deadline: Instant) -> Result<(Option<i32>, Vec<u8>), GitError> {
    if Instant::now() >= deadline {
        return Err(GitError::Command);
    }
    let mut child = command.spawn().map_err(|_| GitError::Process)?;
    let Some(stdout) = child.stdout.take() else {
        stop(&mut child);
        return Err(GitError::Process);
    };
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let _ = send.send(read_window(stdout, skip, limit));
    });
    let result = supervise(&mut child, receive, limit, deadline);
    let _ = reader.join();
    result
}

fn read_window(mut stdout: impl std::io::Read, skip: usize, limit: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    std::io::copy(&mut stdout.by_ref().take(skip as u64), &mut std::io::sink())?;
    let mut bytes = Vec::new();
    stdout.take(limit.saturating_add(1) as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
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
    fn changes_no_index_difference_is_success_but_a_command_error_is_not() {
        let diff = Invocation::new(Path::new("private"), &["diff", "--no-index", "before", "after"]);
        let other = Invocation::new(Path::new("copy"), &["diff", "HEAD"]);
        assert!(successful(&diff, Some(0)));
        assert!(successful(&diff, Some(1)));
        assert!(!successful(&diff, Some(2)));
        assert!(!successful(&diff, None));
        assert!(!successful(&other, Some(1)));
    }

    #[derive(Default)]
    struct HungChild {
        events: Vec<&'static str>,
    }
    impl RunningChild for HungChild {
        fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
            Ok(None)
        }
        fn kill(&mut self) -> std::io::Result<()> {
            self.events.push("kill");
            Ok(())
        }
        fn wait(&mut self) -> std::io::Result<ExitStatus> {
            self.events.push("reap");
            Err(std::io::Error::other("fake child has no operating-system exit status"))
        }
    }

    #[test]
    fn changes_command_deadline_kills_and_reaps_a_child_with_no_output() {
        let mut child = HungChild::default();
        let (_send, read) = std::sync::mpsc::channel();
        let result = supervise(&mut child, read, 100, Instant::now() + Duration::from_millis(2));
        assert_eq!(result, Err(GitError::Command));
        assert_eq!(child.events, ["kill", "reap"]);
    }

    #[test]
    fn changes_command_deadline_also_kills_a_child_after_stdout_closes() {
        let mut child = HungChild::default();
        let (send, read) = std::sync::mpsc::channel();
        send.send(Ok(vec![])).unwrap();
        drop(send);
        let result = supervise(&mut child, read, 100, Instant::now() + Duration::from_millis(2));
        assert_eq!(result, Err(GitError::Command));
        assert_eq!(child.events, ["kill", "reap"]);
    }

    #[test]
    fn changes_output_cap_stops_and_reaps_the_child() {
        let mut child = HungChild::default();
        let (send, read) = std::sync::mpsc::channel();
        send.send(Ok(vec![1, 2, 3])).unwrap();
        let result = supervise(&mut child, read, 2, Instant::now() + COMMAND_TIME).unwrap();
        assert_eq!(result, (None, vec![1, 2, 3]));
        assert_eq!(child.events, ["kill", "reap"]);
    }

    #[test]
    fn changes_name_windows_discard_prefix_and_read_only_one_extra_byte() {
        let mut input = std::io::Cursor::new(b"discard\0first\0second\0third\0");
        assert_eq!(read_window(&mut input, 8, 6).unwrap(), b"first\0s");
        assert_eq!(input.position(), 15);
    }

    #[test]
    fn changes_expired_request_never_calls_an_executor() {
        struct Never;
        impl Executor for Never {
            fn execute(&mut self, _: &Invocation) -> Result<Output, GitError> {
                panic!("an expired request must not run another command");
            }
        }
        let call = Invocation::new(Path::new("w"), &["status"]).before(Instant::now());
        assert_eq!(Never.execute_bounded(&call, 10), Err(GitError::Command));
    }

    #[test]
    fn the_config_query_follows_a_call_that_names_its_repository() {
        let named = Invocation::new(Path::new("w"), &["--git-dir=g", "--work-tree=w", "status", "--git-dir=late"]);
        assert_eq!(repository_options(&named.args), ["--git-dir=g", "--work-tree=w"]);
        assert!(repository_options(&Invocation::new(Path::new("w"), &["status"]).args).is_empty());
    }
}
