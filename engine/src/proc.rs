//! The slice of a child process a driver needs, so tests can replay a recording without spawning anything.

pub use faber_winproc::process_tree::ProcessIdentity;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::time::{Duration, Instant};

/// A running turn, and the fixed grace period an interrupt gives it before the process is stopped.
#[derive(Default)]
pub(crate) struct TurnWatch {
    running: bool,
    interrupt: Option<Instant>,
}

impl TurnWatch {
    pub fn begin(&mut self) {
        self.running = true;
    }
    pub fn interrupt(&mut self) {
        self.interrupt.get_or_insert_with(|| Instant::now() + Duration::from_secs(5));
    }
    pub fn finish(&mut self) {
        self.running = false;
        self.interrupt = None;
    }
    pub fn stopping(&self) -> bool {
        self.interrupt.is_some()
    }
    pub fn active(&self) -> bool {
        self.running || self.interrupt.is_some()
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.interrupt
    }
}

pub(crate) async fn wait_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

pub struct Proc {
    pub stdin: Box<dyn AsyncWrite + Send + Unpin>,
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    pub stderr: Option<Box<dyn AsyncRead + Send + Unpin>>,
    /// The real process, if there is one; a replayed recording has none.
    pub child: Option<Child>,
    /// What ends the process together with everything it started (a Windows job object); none for a replayed recording.
    pub contained: Option<Containment>,
}

/// Each executable embedding the engine dispatches this before starting its UI/runtime. The trusted guard waits
/// for the parent to assign its job, acknowledges one byte, then launches the unmodified CLI with inherited pipes.
pub fn guard_mode() -> Option<i32> {
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.first().is_some_and(|s| s == "--faber-process-guard") {
            return Some(guard(&args[1..]).unwrap_or(1));
        }
    }
    None
}

#[cfg(windows)]
fn guard(args: &[std::ffi::OsString]) -> io::Result<i32> {
    use std::io::{Read, Write};
    use std::os::windows::process::CommandExt;
    let (binary, args) = args.split_first().ok_or_else(|| io::Error::other("missing guarded program"))?;
    let mut go = [0];
    io::stdin().read_exact(&mut go)?;
    if go != *b"G" {
        return Err(io::Error::other("guard not admitted"));
    }
    // The parent writes no CLI input until this ack, so stdin's buffer cannot swallow a protocol message.
    io::stdout().write_all(b"R")?;
    io::stdout().flush()?;
    let status = std::process::Command::new(binary)
        .args(args)
        .creation_flags(0x0800_0000)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(status.code().unwrap_or(1))
}

pub type Spawner = Arc<dyn Fn(&str, &[String], &Path) -> io::Result<Proc> + Send + Sync>;

thread_local! {
    // ponytail: a thread-local read by the spawner, which assumes the Spawner starts the process synchronously on the calling thread
    // (real_spawner and the shell’s wrapper do). An async or offloaded Spawner would silently lose idle priority; pass the
    // priority through the Spawner signature if that ever changes.
    /// Set while a background reader is being started: the process is created at idle priority.
    static BACKGROUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Starts a short background reader (a quota reading) so that it never competes with a game or the person’s work. On
/// Windows the process is created at idle priority, before it runs a single instruction. Elsewhere there is no safe way to
/// lower the priority from Rust (it would need unsafe code, which Faber keeps to one Windows file), so it runs normally.
pub fn spawn_background(spawn: &Spawner, bin: &str, args: &[String], cwd: &Path) -> io::Result<Proc> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            BACKGROUND.with(|b| b.set(false));
        }
    }
    BACKGROUND.with(|b| b.set(true));
    let _reset = Reset;
    spawn(bin, args, cwd)
}

/// Whether the process being started now is a background reader (the spawner reads this).
pub fn starting_in_background() -> bool {
    BACKGROUND.with(|b| b.get())
}

/// The Windows process creation flags: no console window, started suspended (to join its job first), and at idle priority for
/// a background reader.
#[cfg(windows)]
fn creation_flags(background: bool) -> u32 {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
    CREATE_NO_WINDOW | CREATE_SUSPENDED | if background { IDLE_PRIORITY_CLASS } else { 0 }
}

pub(crate) struct CommandOutput {
    pub success: bool,
    pub text: String,
    pub confirmed: bool,
    pub unresolved: Option<OwnedTree>,
}

/// Drain both pipes; retain ownership for a later Stop if the whole container cannot yet be confirmed empty.
pub(crate) async fn command_output(mut p: Proc, mut cancel: tokio::sync::oneshot::Receiver<()>) -> CommandOutput {
    async fn drain(mut pipe: Box<dyn AsyncRead + Send + Unpin>) -> io::Result<String> {
        let mut tail = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = pipe.read(&mut buf).await?;
            if n == 0 {
                return Ok(String::from_utf8_lossy(&tail).into_owned());
            }
            tail.extend_from_slice(&buf[..n]);
            if tail.len() > 4096 {
                tail.drain(..tail.len() - 4096);
            }
        }
    }
    let mut tree_gone = p.child.is_none() && p.contained.is_none();
    drop(p.stdin);
    let stderr = p.stderr.take().unwrap_or_else(|| Box::new(tokio::io::empty()));
    let (out, err) = (drain(p.stdout), drain(stderr));
    tokio::pin!(out, err);
    let (mut output, mut errors, mut code, mut interrupted) = (None, None, None, false);
    while output.is_none() || errors.is_none() || code.is_none() {
        tokio::select! {
            result = &mut out, if output.is_none() => output = Some(result),
            result = &mut err, if errors.is_none() => errors = Some(result),
            result = exit_code(&mut p.child), if code.is_none() => code = Some(result),
            _ = &mut cancel, if !interrupted => {
                interrupted = true;
                let owned = p.contained.is_some();
                tree_gone |= stop(&mut tokio::io::sink(), &mut p.child, &mut p.contained).await;
                if owned {
                    // A held pipe must not hide the retained identity from a subsequent scoped Stop.
                    return CommandOutput {
                        success: false,
                        text: format!("{}{}", output.take().and_then(Result::ok).unwrap_or_default(), errors.take().and_then(Result::ok).unwrap_or_default()),
                        confirmed: tree_gone,
                        unresolved: if tree_gone { None } else { p.contained.as_ref().and_then(Containment::owned_tree) },
                    };
                }
            }
        }
    }
    let (out, err, code) = (output.unwrap(), errors.unwrap(), code.unwrap());
    if let Some(job) = p.contained.as_ref() {
        tree_gone = job.finish().await;
    }
    CommandOutput {
        success: code == "0" && out.is_ok() && err.is_ok() && !interrupted && tree_gone,
        text: format!("{}{}", out.unwrap_or_default(), err.unwrap_or_default()),
        confirmed: tree_gone,
        unresolved: if tree_gone { None } else { p.contained.as_ref().and_then(Containment::owned_tree) },
    }
}

/// Set to `1` on every CLI Faber starts, and so on everything that CLI runs: Faber's own command line (`faber`, in `cli/`)
/// then knows an AI is driving it and holds it to the channel's limits on starting turns (`channel/src/guard.rs`).
pub const AGENT_MARK: &str = "FABER_AGENT_DEPTH";

static CLI: OnceLock<PathBuf> = OnceLock::new();
/// Mints the token a conversation's AI carries (set once by the channel).
pub type TokenMinter = Arc<dyn Fn(&str) -> String + Send + Sync>;

static TOKEN_MINTER: OnceLock<TokenMinter> = OnceLock::new();
static TOKEN_REVOKER: OnceLock<Arc<dyn Fn(&str) + Send + Sync>> = OnceLock::new();

thread_local! {
    static CONVERSATION: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static IDENTITY: std::cell::RefCell<Option<Arc<ConversationIdentity>>> = const { std::cell::RefCell::new(None) };
}

pub struct ConversationIdentity {
    pub id: String,
    pub token: Option<String>,
}

impl ConversationIdentity {
    pub fn new(id: &str) -> Arc<Self> {
        Arc::new(Self { id: id.into(), token: conversation_token(id) })
    }

    pub fn revoke(&self) {
        if let (Some(token), Some(revoke)) = (&self.token, TOKEN_REVOKER.get()) {
            revoke(token);
        }
    }
}

impl Drop for ConversationIdentity {
    fn drop(&mut self) {
        self.revoke();
    }
}

pub fn set_token_revoker(f: Arc<dyn Fn(&str) + Send + Sync>) {
    let _ = TOKEN_REVOKER.set(f);
}

pub fn with_identity<T>(identity: &Arc<ConversationIdentity>, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<ConversationIdentity>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            IDENTITY.with(|current| *current.borrow_mut() = self.0.take());
        }
    }
    let previous = IDENTITY.with(|current| current.replace(Some(identity.clone())));
    let _restore = Restore(previous);
    f()
}

/// Installs the current channel's token minter once for this process.
pub fn set_token_minter(f: Arc<dyn Fn(&str) -> String + Send + Sync>) {
    let _ = TOKEN_MINTER.set(f);
}

/// Mints a token using the running channel's secret, if its minter is installed.
pub fn conversation_token(id: &str) -> Option<String> {
    TOKEN_MINTER.get().map(|mint| mint(id))
}

/// Runs a synchronous session start with its conversation identity, restoring it even on panic.
pub fn as_conversation<T>(id: &str, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CONVERSATION.with(|current| *current.borrow_mut() = self.0.take());
        }
    }
    let previous = CONVERSATION.with(|current| current.replace(Some(id.to_owned())));
    let _restore = Restore(previous);
    f()
}

/// Sets the command line shipped with this Faber, once at startup.
pub fn set_cli(path: PathBuf) {
    let _ = CLI.set(path);
}

pub(crate) fn cli_binary() -> Option<PathBuf> {
    CLI.get().cloned()
}

/// Starts a real process with piped stdio, no shell and, on Windows, no console window.
pub fn real_spawner() -> Spawner {
    Arc::new(|bin, args, cwd| spawn_command(marked(bin, args, cwd)))
}

pub(crate) fn marked(bin: &str, args: &[String], cwd: &Path) -> Command {
    marked_with_minter(bin, args, cwd, TOKEN_MINTER.get().map(Arc::as_ref))
}

fn marked_with_minter(bin: &str, args: &[String], cwd: &Path, mint: Option<&(dyn Fn(&str) -> String + Send + Sync)>) -> Command {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(cwd).env(AGENT_MARK, "1").env_remove("FABER_AGENT_TOKEN").env_remove("FABER_CONVERSATION");
    CONVERSATION.with(|current| {
        if let Some(id) = current.borrow().as_deref()
            && let Some(mint) = mint
        {
            cmd.env("FABER_AGENT_TOKEN", mint(id)).env("FABER_CONVERSATION", id);
        }
    });
    IDENTITY.with(|current| {
        if let Some(identity) = current.borrow().as_ref()
            && let Some(token) = &identity.token
        {
            cmd.env("FABER_AGENT_TOKEN", token).env("FABER_CONVERSATION", &identity.id);
        }
    });
    if let Some(cli) = CLI.get()
        && let Some(folder) = cli.parent()
        && let Ok(mut path) = std::env::join_paths([folder])
    {
        if let Some(original) = std::env::var_os("PATH") {
            path.push(if cfg!(windows) { ";" } else { ":" });
            path.push(original);
        }
        cmd.env("FABER_CLI", cli).env("PATH", path);
    }
    cmd
}

/// Starts `cmd` with piped stdio and, on Windows, inside its own job object. The process is created suspended, put in the job
/// and only then allowed to run, so nothing it starts can be outside the job. If that fails the process is killed and the
/// error returned: no CLI ever runs uncontained.
pub fn spawn_command(cmd: Command) -> io::Result<Proc> {
    spawn_in_job(cmd, join_job)
}

thread_local! {
    /// Set while a project check (`crate::verify`) is being started: the process is created at below-normal priority.
    static CHECK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Starts a project check like `spawn_command` (no window, its own kill-on-close job), at below-normal priority on Windows
/// so a build never makes the person's apps or a game stutter, and with its job's memory capped at `memory` bytes before
/// it runs. Elsewhere it runs at the normal priority, uncapped.
pub fn spawn_check(cmd: Command, memory: Option<u64>) -> io::Result<Proc> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CHECK.with(|c| c.set(false));
        }
    }
    CHECK.with(|c| c.set(true));
    let _reset = Reset;
    spawn_in_job(cmd, |child| {
        let job = join_job(child)?;
        #[cfg(windows)]
        if let Some(bytes) = memory {
            // ponytail: a cap Windows refuses leaves the check uncapped rather than not run; the admission still holds.
            if let Some(job) = job.tree.job.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                let _ = faber_winproc::job_memory_cap(job.handle(), bytes);
            }
        }
        #[cfg(not(windows))]
        let _ = memory;
        Ok(job)
    })
}

fn spawn_in_job(mut cmd: Command, join: impl FnOnce(&Child) -> io::Result<Containment>) -> io::Result<Proc> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    #[cfg(windows)]
    {
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        let background = starting_in_background();
        let below = !background && CHECK.with(|c| c.get());
        cmd.creation_flags(creation_flags(background) | if below { BELOW_NORMAL_PRIORITY_CLASS } else { 0 });
    }
    let mut child = cmd.spawn()?;
    let joined = join(&child);
    #[cfg(windows)]
    let joined = joined.and_then(|job| {
        let pid = child.id().ok_or_else(|| io::Error::other("the process already ended"))?;
        faber_winproc::resume_process(pid)?;
        Ok(job)
    });
    let contained = match joined {
        Ok(job) => Some(job),
        Err(error) => {
            let _ = child.start_kill();
            return Err(error);
        }
    };
    Ok(Proc {
        stdin: Box::new(child.stdin.take().expect("piped stdin")),
        stdout: Box::new(child.stdout.take().expect("piped stdout")),
        stderr: child.stderr.take().map(|s| Box::new(s) as Box<dyn AsyncRead + Send + Unpin>),
        child: Some(child),
        contained,
    })
}

/// Keeps the end of stderr so a crash can say why.
pub(crate) fn stderr_tail(stderr: Option<Box<dyn AsyncRead + Send + Unpin>>) -> Arc<Mutex<String>> {
    let tail = Arc::new(Mutex::new(String::new()));
    if let Some(mut stderr) = stderr {
        let keep = tail.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let mut t = keep.lock().unwrap();
                t.push_str(&String::from_utf8_lossy(&buf[..n]));
                if t.len() > 2000 {
                    let cut = t.len() - 2000;
                    let cut = (cut..t.len()).find(|&i| t.is_char_boundary(i)).unwrap_or(t.len());
                    t.drain(..cut);
                }
            }
        });
    }
    tail
}

/// Why a line may not have reached the CLI. When `torn` is set some bytes, or all of them, were written before the
/// failure, so sending the line again could corrupt the stream or repeat the line; the caller must end the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WriteError {
    pub torn: bool,
}

/// Writes one JSON line and flushes it. Only a caller that must know whether the CLI got the line looks at the
/// result; for the others a dead process shows up as the end of its output.
pub(crate) async fn write_line(stdin: &mut (dyn AsyncWrite + Send + Unpin), msg: &serde_json::Value) -> Result<(), WriteError> {
    let mut line = msg.to_string();
    line.push('\n');
    let mut done = 0;
    while done < line.len() {
        match stdin.write(&line.as_bytes()[done..]).await {
            Ok(n) if n > 0 => done += n,
            _ => return Err(WriteError { torn: done > 0 }),
        }
    }
    // A flush that fails may still have let the whole line through.
    stdin.flush().await.map_err(|_| WriteError { torn: true })
}

/// The owned job or dedicated process group, retained until every member is confirmed gone.
pub struct Containment {
    tree: Arc<TreeState>,
}

struct TreeState {
    #[cfg(windows)]
    job: Mutex<Option<win32job::Job>>,
    #[cfg(unix)]
    pgid: u32,
    #[cfg(unix)]
    external: Mutex<faber_winproc::process_tree::ExternalDescendants>,
    identity: crate::project::ProcessId,
    root: Option<ProcessIdentity>,
    confirmed: AtomicBool,
    #[cfg(test)]
    probe: Option<LifecycleProbe>,
}

#[cfg(test)]
struct LifecycleProbe {
    empty: Arc<dyn Fn() -> io::Result<bool> + Send + Sync>,
    terminate: Arc<dyn Fn() + Send + Sync>,
    members: Option<Arc<dyn Fn() -> io::Result<Vec<ProcessIdentity>> + Send + Sync>>,
    #[cfg(unix)]
    external: Option<Arc<dyn Fn() -> io::Result<Vec<ProcessIdentity>> + Send + Sync>>,
}

impl TreeState {
    fn members(&self) -> io::Result<Vec<ProcessIdentity>> {
        if self.confirmed.load(Ordering::Acquire) {
            return Ok(Vec::new());
        }
        #[cfg(test)]
        if let Some(probe) = &self.probe {
            return probe.members.as_ref().ok_or_else(|| io::Error::other("scripted member query unavailable"))?();
        }
        #[cfg(windows)]
        let members = self
            .job
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .ok_or_else(|| io::Error::other("owned job handle is unavailable"))
            .and_then(|job| faber_winproc::process_tree::job_members(job.handle()))?;
        #[cfg(unix)]
        let members = faber_winproc::process_tree::group_members(self.pgid)?;
        Ok(members.into_iter().map(|member| member.identity).collect())
    }

    fn descendants(&self) -> io::Result<Vec<ProcessIdentity>> {
        Ok(self.members()?.into_iter().filter(|member| Some(*member) != self.root).collect())
    }

    #[cfg(unix)]
    fn external_descendants(&self) -> io::Result<Vec<ProcessIdentity>> {
        #[cfg(test)]
        if let Some(probe) = &self.probe {
            return probe.external.as_ref().map_or_else(|| Ok(Vec::new()), |sample| sample());
        }
        let root = self.root.ok_or_else(|| io::Error::other("owned process identity is unavailable"))?;
        // Group termination does not end separately grouped descendants observed before their parent exited.
        self.external.lock().unwrap_or_else(|error| error.into_inner()).sample(root, self.pgid)
    }

    fn end(&self) {
        if self.confirmed.load(Ordering::Acquire) {
            return;
        }
        #[cfg(test)]
        if let Some(probe) = &self.probe {
            (probe.terminate)();
            return;
        }
        #[cfg(windows)]
        if let Some(job) = self.job.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = faber_winproc::job_terminate(job.handle());
        }
        #[cfg(unix)]
        {
            let _ = faber_winproc::process_group_terminate(self.pgid);
        }
    }

    fn empty(&self) -> io::Result<bool> {
        if self.confirmed.load(Ordering::Acquire) {
            return Ok(true);
        }
        #[cfg(test)]
        if let Some(probe) = &self.probe {
            let empty = (probe.empty)()?;
            if empty {
                self.confirmed.store(true, Ordering::Release);
            }
            return Ok(empty);
        }
        #[cfg(windows)]
        let empty = self
            .job
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .ok_or_else(|| io::Error::other("owned job handle is unavailable"))
            .and_then(|job| faber_winproc::job_active_processes(job.handle()))?
            == 0;
        #[cfg(unix)]
        let empty = faber_winproc::process_group_empty(self.pgid)?;
        if empty {
            self.confirmed.store(true, Ordering::Release);
        }
        Ok(empty)
    }

    async fn finish(&self) -> bool {
        self.end();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.empty() {
                Ok(true) => return true,
                Ok(false) if Instant::now() < deadline => tokio::time::sleep(Duration::from_millis(10)).await,
                _ => return false,
            }
        }
    }
}

impl Drop for TreeState {
    fn drop(&mut self) {
        self.end();
    }
}

impl Containment {
    /// Every live container member except the original CLI instance; unknown queries are errors, never idle.
    pub fn descendants(&self) -> io::Result<Vec<ProcessIdentity>> {
        self.tree.descendants()
    }
    /// A clone of ownership, never a reconstructed PID or a snapshot of the process list.
    pub fn owned_tree(&self) -> Option<OwnedTree> {
        Some(OwnedTree { tree: self.tree.clone() })
    }

    /// What caps the processor share of the job's processes (`Some(percent)`) or lifts the cap (`None`): the game guard of
    /// a check (`crate::verify`), saying whether Windows took it. Nothing elsewhere than Windows, which has no job.
    pub fn cpu_cap(&self) -> Box<dyn Fn(Option<u32>) -> bool + Send + Sync> {
        #[cfg(windows)]
        {
            let tree = self.tree.clone();
            Box::new(move |percent| {
                tree.job
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                    .is_some_and(|job| faber_winproc::job_cpu_cap(job.handle(), percent).is_ok())
            })
        }
        #[cfg(not(windows))]
        {
            Box::new(|_| true)
        }
    }

    /// The most memory the job's processes held together so far, in bytes: what a check's RAM admission learns from
    /// (`crate::verify`). None elsewhere than Windows, or when Windows does not say.
    pub fn peak_memory(&self) -> Option<u64> {
        #[cfg(windows)]
        {
            let job = self.tree.job.lock().unwrap_or_else(|e| e.into_inner());
            job.as_ref().and_then(|job| faber_winproc::job_peak_memory(job.handle()).ok())
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Request termination while retaining the owned job/group for subsequent confirmation.
    fn end(&self) {
        self.tree.end();
    }

    /// Root exit and closed pipes are insufficient: the owned container must have no remaining member.
    pub(crate) async fn finish(&self) -> bool {
        self.tree.finish().await
    }

    #[cfg(test)]
    pub(crate) fn scripted(
        empty: impl Fn() -> io::Result<bool> + Send + Sync + 'static,
        terminate: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            tree: Arc::new(TreeState {
                #[cfg(windows)]
                job: Mutex::new(None),
                #[cfg(unix)]
                pgid: 0,
                #[cfg(unix)]
                external: Mutex::default(),
                identity: crate::project::ProcessId { pid: 42, started_at: 1 },
                root: Some(ProcessIdentity { pid: 42, created: 1 }),
                confirmed: AtomicBool::new(false),
                probe: Some(LifecycleProbe {
                    empty: Arc::new(empty),
                    terminate: Arc::new(terminate),
                    members: None,
                    #[cfg(unix)]
                    external: None,
                }),
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn scripted_descendants(
        root: ProcessIdentity,
        members: impl Fn() -> io::Result<Vec<ProcessIdentity>> + Send + Sync + 'static,
        terminate: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let mut container = Self::scripted(|| Ok(false), terminate);
        let state = Arc::get_mut(&mut container.tree).unwrap();
        state.root = Some(root);
        state.identity.pid = root.pid;
        let members = Arc::new(members);
        let empty = members.clone();
        let probe = state.probe.as_mut().unwrap();
        probe.empty = Arc::new(move || empty().map(|members| members.is_empty()));
        probe.members = Some(members);
        container
    }

    #[cfg(all(test, unix))]
    pub(crate) fn scripted_external_descendants(
        root: ProcessIdentity,
        members: impl Fn() -> io::Result<Vec<ProcessIdentity>> + Send + Sync + 'static,
        external: impl Fn() -> io::Result<Vec<ProcessIdentity>> + Send + Sync + 'static,
    ) -> Self {
        let mut container = Self::scripted_descendants(root, members, || {});
        Arc::get_mut(&mut container.tree).unwrap().probe.as_mut().unwrap().external = Some(Arc::new(external));
        container
    }
}

impl Drop for Containment {
    fn drop(&mut self) {
        self.end();
    }
}

/// What a lease needs to know about one launched CLI tree, shared with whoever later releases the lease.
/// It exists only for a process this engine started inside its own kill-on-close job (Windows); nothing can build
/// one from a PID, a snapshot or a recorded file.
#[derive(Clone)]
pub struct OwnedTree {
    tree: Arc<TreeState>,
}

impl std::fmt::Debug for OwnedTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OwnedTree({})", self.tree.identity.pid)
    }
}

/// Proof that an owned tree is gone. Only `OwnedTree::retire` makes one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retired {
    identity: crate::project::ProcessId,
}

impl Retired {
    pub fn identity(&self) -> crate::project::ProcessId {
        self.identity
    }
}

impl OwnedTree {
    /// Actual OS identities, including the root; this never reconstructs ownership from PIDs.
    pub fn members(&self) -> io::Result<Vec<ProcessIdentity>> {
        let members = self.tree.members()?;
        #[cfg(unix)]
        let members = {
            let mut members = members;
            members.extend(self.tree.external_descendants()?);
            members
        };
        Ok(members)
    }

    pub(crate) fn same_tree(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.tree, &other.tree)
    }

    pub(crate) fn is_retired(&self) -> bool {
        self.tree.confirmed.load(Ordering::Acquire)
    }

    /// Confirm the whole owned container is empty without sending any termination signal.
    #[cfg(any(windows, target_os = "macos", target_os = "linux", test))]
    pub(crate) fn confirm_empty(&self) -> io::Result<bool> {
        #[cfg(unix)]
        if !self.tree.external_descendants()?.is_empty() {
            return Ok(false);
        }
        self.tree.empty()
    }

    pub fn descendants(&self) -> io::Result<Vec<ProcessIdentity>> {
        self.tree.descendants()
    }

    /// Also remember descendants which outlive the parent or leave the original group; this grants no signalling rights.
    #[cfg(unix)]
    pub fn external_descendants(&self) -> io::Result<Vec<ProcessIdentity>> {
        self.tree.external_descendants()
    }

    /// The root's pid, with the microsecond this engine launched it: a reused pid has another start.
    pub fn identity(&self) -> crate::project::ProcessId {
        self.tree.identity
    }

    /// Keep ownership and query the whole container after termination; a root PID cannot prove its workers ended.
    pub async fn retire(&self) -> io::Result<Retired> {
        if !self.tree.finish().await {
            return Err(io::Error::other("owned tree did not end"));
        }
        Ok(Retired { identity: self.tree.identity })
    }
}

/// The jobs the CLIs and checks Faber started run in, so the command line's channel can tell a program one of them started,
/// however it was launched: nothing in such a job can leave it.
#[cfg(windows)]
static JOBS: Mutex<Vec<std::sync::Weak<TreeState>>> = Mutex::new(Vec::new());

#[cfg(windows)]
fn known(tree: &Arc<TreeState>) {
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    jobs.retain(|tree| tree.strong_count() > 0);
    jobs.push(Arc::downgrade(tree));
}

/// Whether `inside` finds a process in one of the jobs Faber's CLIs and checks run in now. A job it cannot ask counts as a
/// yes: in doubt, the process is held to an AI's limits.
#[cfg(windows)]
pub fn in_a_job_we_started(inside: impl Fn(isize) -> io::Result<bool>) -> bool {
    let mut jobs = JOBS.lock().unwrap_or_else(|e| e.into_inner());
    jobs.retain(|tree| tree.strong_count() > 0);
    jobs.iter()
        .filter_map(std::sync::Weak::upgrade)
        .any(|tree| tree.job.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|job| inside(job.handle()).unwrap_or(true)))
}

#[cfg(windows)]
fn join_job(child: &Child) -> io::Result<Containment> {
    let handle = child.raw_handle().ok_or_else(|| io::Error::other("the process already ended"))?;
    let mut limits = win32job::ExtendedLimitInfo::new();
    limits.limit_kill_on_job_close();
    let job = win32job::Job::create_with_limit_info(&limits).map_err(io::Error::other)?;
    job.assign_process(handle as isize).map_err(io::Error::other)?;
    let started_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_micros() as u64).max(1);
    let identity = crate::project::ProcessId { pid: child.id().ok_or_else(|| io::Error::other("the process already ended"))?, started_at };
    let root = faber_winproc::process_tree::job_process_identity(job.handle(), identity.pid)?;
    let tree = Arc::new(TreeState {
        job: Mutex::new(Some(job)),
        identity,
        root: Some(root),
        confirmed: AtomicBool::new(false),
        #[cfg(test)]
        probe: None,
    });
    known(&tree);
    Ok(Containment { tree })
}

#[cfg(unix)]
fn join_job(child: &Child) -> io::Result<Containment> {
    let pid = child.id().ok_or_else(|| io::Error::other("the process already ended"))?;
    let started_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_micros() as u64).max(1);
    // A short command can exit before this query; its group remains owned and an unknown root excludes nothing.
    let root = faber_winproc::process_tree::group_process_identity(pid, pid).ok();
    Ok(Containment {
        tree: Arc::new(TreeState {
            pgid: pid,
            external: Mutex::default(),
            identity: crate::project::ProcessId { pid, started_at },
            root,
            confirmed: AtomicBool::new(false),
            #[cfg(test)]
            probe: None,
        }),
    })
}

/// Ends a process and everything it started, then closes its input. The processes go first: a CLI that quits when its input
/// closes would otherwise be gone before the tree could be found, and its workers would keep running.
pub(crate) async fn stop(
    stdin: &mut (dyn AsyncWrite + Send + Unpin),
    child: &mut Option<Child>,
    contained: &mut Option<Containment>,
) -> bool {
    if let Some(job) = contained.as_ref() {
        job.end();
    }
    if let Some(child) = child {
        if contained.is_none() {
            kill_tree(child.id()).await;
        }
        let _ = child.start_kill();
    }
    let _ = stdin.shutdown().await;
    // Reap the root too: a Unix zombie still belongs to its group until its parent waits.
    if let Some(child) = child {
        let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    }
    let confirmed = match contained.as_ref() {
        Some(job) => job.finish().await,
        None => false,
    };
    if confirmed {
        contained.take();
    }
    confirmed
}

/// Keep the owned job until it proves every member ended; closing a pipe alone proves nothing about workers.
pub(crate) async fn stop_confirmed(
    stdin: &mut (dyn AsyncWrite + Send + Unpin),
    child: &mut Option<Child>,
    contained: &mut Option<Containment>,
) -> bool {
    stop(stdin, child, contained).await
}

/// Fallback for a process that is in no job (a test double, or off Windows). Killing only the CLI leaves a command it started (a build, a script)
/// running, and in full access that command can keep changing files after the person switched to asking. On Windows
/// `taskkill /T` ends the whole tree while its root is still alive. It is run directly, never through a shell, and a failure
/// is logged. Elsewhere only the CLI itself is ended.
#[cfg(windows)]
async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    let exe = std::env::var_os("SystemRoot")
        .map(|root| std::path::PathBuf::from(root).join("System32").join("taskkill.exe"))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| "taskkill".into());
    let mut cmd = Command::new(exe);
    cmd.args(["/PID", &pid.to_string(), "/T", "/F"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    match tokio::time::timeout(std::time::Duration::from_secs(5), cmd.status()).await {
        Ok(Ok(status)) if status.success() => {}
        // The CLI having already exited is the usual reason; either way the process is ended next.
        other => eprintln!("faber: taskkill did not end the process tree of {pid}: {other:?}"),
    }
}

#[cfg(not(windows))]
async fn kill_tree(_pid: Option<u32>) {}

pub(crate) async fn exit_code(child: &mut Option<Child>) -> String {
    match child {
        Some(child) => match child.wait().await {
            Ok(status) => status.code().map_or("unknown".into(), |c| c.to_string()),
            Err(_) => "unknown".into(),
        },
        None => "0".into(),
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn the_channel_knows_the_jobs_of_the_clis_running_now_and_only_those() {
        let job = win32job::Job::create().unwrap();
        let handle = job.handle();
        let tree = Arc::new(TreeState {
            job: Mutex::new(Some(job)),
            identity: crate::project::ProcessId { pid: 1, started_at: 1 },
            root: None,
            confirmed: AtomicBool::new(false),
            probe: None,
        });
        known(&tree);
        assert!(in_a_job_we_started(|job| Ok(job == handle)));
        assert!(in_a_job_we_started(|_| Err(io::Error::other("cannot ask"))), "in doubt, an AI");
        drop(tree);
        assert!(!in_a_job_we_started(|job| Ok(job == handle)), "a job that ended no longer counts");
    }

    #[tokio::test]
    async fn rewind_cancelled_command_waits_for_its_pipes_to_close() {
        let (output, writer) = tokio::io::duplex(64);
        let p = Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(output), stderr: None, child: None, contained: None };
        let (stop, cancel) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(command_output(p, cancel));
        // Stderr and the scripted child finish before cancellation; stdout stays open.
        tokio::task::yield_now().await;
        stop.send(()).unwrap();
        tokio::task::yield_now().await;
        assert!(!task.is_finished(), "a writer that still holds a pipe cannot be reported idle");
        drop(writer);
        assert!(!task.await.unwrap().success, "an interrupted command is not a success");
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_interrupts_cannot_extend_the_stop_grace() {
        let mut watch = TurnWatch::default();
        assert!(!watch.active());
        watch.begin();
        assert_eq!(watch.deadline(), None);
        watch.interrupt();
        let stop = watch.deadline();
        assert_eq!(stop, Some(Instant::now() + Duration::from_secs(5)));
        tokio::time::advance(Duration::from_secs(4)).await;
        watch.interrupt();
        assert_eq!(watch.deadline(), stop);
        watch.finish();
        assert!(!watch.stopping());
        assert_eq!(watch.deadline(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_command_inherits_exact_session_identity() {
        let identity = Arc::new(ConversationIdentity { id: "one".into(), token: Some("test-key".into()) });
        let command = with_identity(&identity, || marked_with_minter("faber", &[], Path::new("."), None));
        let env: std::collections::HashMap<_, _> = command.as_std().get_envs().filter_map(|(k, v)| v.map(|v| (k, v))).collect();
        assert_eq!(
            env.get(std::ffi::OsStr::new("FABER_AGENT_TOKEN")).map(|value| value.as_os_str()),
            Some(std::ffi::OsStr::new("test-key"))
        );
        assert_eq!(env.get(std::ffi::OsStr::new("FABER_CONVERSATION")).map(|value| value.as_os_str()), Some(std::ffi::OsStr::new("one")));
    }

    #[test]
    fn conversation_without_a_minter_gets_no_identity() {
        as_conversation("one", || {
            let cmd = marked_with_minter("cli", &[], Path::new("."), None);
            for name in ["FABER_AGENT_TOKEN", "FABER_CONVERSATION"] {
                assert_eq!(cmd.as_std().get_envs().find(|(key, _)| *key == name).unwrap().1, None);
            }
        });
    }

    #[test]
    fn conversation_identity_is_scoped_to_the_starting_thread() {
        set_token_minter(Arc::new(|id| format!("test-{id}")));
        let env = || {
            let cmd = marked("cli", &[], Path::new("."));
            cmd.as_std()
                .get_envs()
                .filter_map(|(k, v)| v.map(|v| (k.to_owned(), v.to_owned())))
                .collect::<std::collections::HashMap<_, _>>()
        };
        assert!(!env().contains_key(std::ffi::OsStr::new("FABER_AGENT_TOKEN")));
        as_conversation("one", || {
            let values = env();
            assert_eq!(values.get(std::ffi::OsStr::new("FABER_AGENT_TOKEN")).unwrap(), "test-one");
            assert_eq!(values.get(std::ffi::OsStr::new("FABER_CONVERSATION")).unwrap(), "one");
            assert!(!std::thread::spawn(env).join().unwrap().contains_key(std::ffi::OsStr::new("FABER_AGENT_TOKEN")));
            as_conversation("two", || assert_eq!(env().get(std::ffi::OsStr::new("FABER_CONVERSATION")).unwrap(), "two"));
            assert_eq!(env().get(std::ffi::OsStr::new("FABER_CONVERSATION")).unwrap(), "one");
        });
        assert!(!env().contains_key(std::ffi::OsStr::new("FABER_CONVERSATION")));
    }

    #[test]
    fn every_cli_faber_starts_is_marked_as_an_ai() {
        let cmd = marked("cli", &["--x".into()], Path::new("."));
        let marks: Vec<_> = cmd.as_std().get_envs().filter(|(name, _)| *name == AGENT_MARK).collect();
        assert_eq!(marks, [(std::ffi::OsStr::new(AGENT_MARK), Some(std::ffi::OsStr::new("1")))]);
    }

    #[test]
    fn marked_commands_find_the_shipped_cli_only_after_it_is_set() {
        let before = marked("cli", &[], Path::new("."));
        assert!(!before.as_std().get_envs().any(|(key, _)| key == "FABER_CLI" || key.eq_ignore_ascii_case("PATH")));
        let cli = std::env::current_dir().unwrap().join("bin").join("faber.exe");
        set_cli(cli.clone());
        let after = marked("cli", &[], Path::new("."));
        let env: Vec<_> = after.as_std().get_envs().collect();
        assert!(env.iter().any(|(key, value)| *key == "FABER_CLI" && *value == Some(cli.as_os_str())));
        let path = env.iter().find(|(key, _)| key.eq_ignore_ascii_case("PATH")).unwrap().1.unwrap();
        let paths: Vec<_> = std::env::split_paths(path).collect();
        assert_eq!(paths[0], cli.parent().unwrap());
        assert_eq!(&paths[1..], std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
    }

    #[test]
    fn rewind_member_observation_excludes_only_the_actual_cli_generation() {
        let root = ProcessIdentity { pid: 42, created: 1 };
        let child = ProcessIdentity { pid: 99, created: 2 };
        let members = Arc::new(Mutex::new(vec![root, child]));
        let query = members.clone();
        let container = Containment::scripted_descendants(root, move || Ok(query.lock().unwrap().clone()), || {});
        assert_eq!(container.descendants().unwrap(), [child]);
        *members.lock().unwrap() = vec![root];
        assert!(container.descendants().unwrap().is_empty(), "a warm CLI alone has no child writer");
        let reused = ProcessIdentity { pid: root.pid, created: 3 };
        *members.lock().unwrap() = vec![reused];
        assert_eq!(container.descendants().unwrap(), [reused], "a recycled PID is never excluded as the original CLI");
    }

    #[tokio::test]
    async fn rewind_closing_an_uncontained_pipe_is_not_proof_its_writers_ended() {
        assert!(!stop_confirmed(&mut tokio::io::sink(), &mut None, &mut None).await);
    }

    #[tokio::test]
    async fn rewind_normal_close_confirms_an_owned_job_has_no_remaining_writers() {
        let mut contained = Some(Containment::scripted(|| Ok(true), || {}));
        assert!(stop_confirmed(&mut tokio::io::sink(), &mut None, &mut contained).await);
        assert!(contained.is_none(), "release the job only after its members are confirmed gone");
    }

    /// Rule 5 of models.md: stopping ends the whole owned tree (a job on Windows, the process group elsewhere, both behind
    /// `Containment`), not the CLI alone, and is confirmed only once no member is left.
    #[tokio::test]
    async fn stopping_ends_the_whole_owned_tree_and_confirms_it_only_when_no_member_is_left() {
        let ended = Arc::new(AtomicBool::new(false));
        let (ends, asks) = (ended.clone(), ended.clone());
        let mut contained =
            Some(Containment::scripted(move || Ok(asks.load(Ordering::SeqCst)), move || ends.store(true, Ordering::SeqCst)));
        assert!(stop(&mut tokio::io::sink(), &mut None, &mut contained).await);
        assert!(ended.load(Ordering::SeqCst), "the members were ended");
        assert!(contained.is_none());
    }

    #[tokio::test]
    async fn rewind_an_unknown_tree_query_keeps_ownership_for_a_retry() {
        let unknown = Arc::new(AtomicBool::new(true));
        let query = unknown.clone();
        let mut contained = Some(Containment::scripted(
            move || {
                if query.load(Ordering::Acquire) { Err(io::Error::new(io::ErrorKind::PermissionDenied, "query denied")) } else { Ok(true) }
            },
            || {},
        ));
        assert!(!stop_confirmed(&mut tokio::io::sink(), &mut None, &mut contained).await);
        assert!(contained.is_some(), "a failed query must not throw away the owned container");
        unknown.store(false, Ordering::Release);
        assert!(stop_confirmed(&mut tokio::io::sink(), &mut None, &mut contained).await);
        assert!(contained.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn rewind_retiring_an_owned_tree_waits_for_its_last_worker_even_after_root_drop() {
        let empty = Arc::new(AtomicBool::new(false));
        let query = empty.clone();
        let terminations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let terminate = terminations.clone();
        let contained = Containment::scripted(
            move || Ok(query.load(Ordering::Acquire)),
            move || {
                terminate.fetch_add(1, Ordering::Relaxed);
            },
        );
        let owner = contained.owned_tree().unwrap();
        drop(contained);
        let retire = tokio::spawn({
            let owner = owner.clone();
            async move { owner.retire().await }
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(!retire.is_finished(), "a dead root or closed pipes do not prove its worker ended");
        empty.store(true, Ordering::Release);
        tokio::time::advance(Duration::from_millis(10)).await;
        assert_eq!(retire.await.unwrap().unwrap().identity(), owner.identity());
        let stopped = terminations.load(Ordering::Relaxed);
        empty.store(false, Ordering::Release);
        owner.retire().await.unwrap();
        assert_eq!(terminations.load(Ordering::Relaxed), stopped, "a confirmed retired identity is never signalled again");
    }

    #[tokio::test(start_paused = true)]
    async fn rewind_closed_command_pipes_and_root_exit_cannot_release_a_live_descendant() {
        let empty = Arc::new(AtomicBool::new(false));
        let query = empty.clone();
        let contained = Containment::scripted(move || Ok(query.load(Ordering::Acquire)), || {});
        let p = Proc {
            stdin: Box::new(tokio::io::sink()),
            stdout: Box::new(tokio::io::empty()),
            stderr: None,
            child: None,
            contained: Some(contained),
        };
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        let command = tokio::spawn(command_output(p, cancel));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(!command.is_finished(), "a descendant may have closed both pipes while it keeps writing files");
        empty.store(true, Ordering::Release);
        tokio::time::advance(Duration::from_millis(10)).await;
        assert!(command.await.unwrap().confirmed, "only the empty-container probe releases the writer");
    }

    #[tokio::test(start_paused = true)]
    async fn rewind_a_command_whose_descendants_cannot_be_confirmed_reports_unknown() {
        let p = Proc {
            stdin: Box::new(tokio::io::sink()),
            stdout: Box::new(tokio::io::empty()),
            stderr: None,
            child: None,
            contained: Some(Containment::scripted(|| Err(io::Error::other("unknown members")), || {})),
        };
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        let result = command_output(p, cancel).await;
        assert!(!result.success && !result.confirmed, "a zero exit code and two closed pipes cannot hide a failed tree query");
        assert!(result.unresolved.is_some(), "Stop must retain the same owned query handle for retry");
    }

    #[tokio::test(start_paused = true)]
    async fn rewind_a_cancelled_owned_command_exposes_retry_before_its_last_pipe_closes() {
        let empty = Arc::new(AtomicBool::new(false));
        let query = empty.clone();
        let contained = Containment::scripted(
            move || {
                if query.load(Ordering::Acquire) { Ok(true) } else { Err(io::Error::other("query unavailable")) }
            },
            || {},
        );
        let identity = contained.owned_tree().unwrap().identity();
        let (output, _still_open) = tokio::io::duplex(64);
        let p =
            Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(output), stderr: None, child: None, contained: Some(contained) };
        let (stop, cancel) = tokio::sync::oneshot::channel();
        let command = tokio::spawn(command_output(p, cancel));
        tokio::task::yield_now().await;
        stop.send(()).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(1), command)
            .await
            .expect("a held pipe cannot prevent the next Stop from retrying")
            .unwrap();
        assert!(!result.confirmed);
        let tree = result.unresolved.expect("keep the original owned container, not a PID-derived replacement");
        assert_eq!(tree.identity(), identity);
        empty.store(true, Ordering::Release);
        assert_eq!(tree.retire().await.unwrap().identity(), identity);
    }

    /// R124c, item 2: a CLI that cannot be put in its job never runs. It is created suspended, so killing it at that point
    /// means not one instruction of it ran.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_process_that_cannot_join_its_job_is_killed_before_it_runs_and_the_spawn_fails() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran.txt");
        let script = dir.path().join("cli.cmd");
        std::fs::write(&script, "@echo off\r\necho x> %~dp0ran.txt\r\n").unwrap();
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/c", &script.to_string_lossy()]).current_dir(dir.path());
        let failed = spawn_in_job(cmd, |_| Err(io::Error::other("no job today")));
        assert!(failed.is_err_and(|e| e.to_string().contains("no job today")));
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(!marker.exists(), "the process ran although it was not contained");
    }

    #[tokio::test]
    async fn command_output_drains_large_pipes_and_keeps_a_bounded_tail() {
        let (mut write, read) = tokio::io::duplex(32);
        tokio::spawn(async move {
            write.write_all(&vec![b'x'; 100_000]).await.unwrap();
            write.write_all(b"TAIL_OK").await.unwrap();
        });
        let p = Proc {
            stdin: Box::new(tokio::io::sink()),
            stdout: Box::new(read),
            stderr: Some(Box::new(std::io::Cursor::new(b"STDERR_OK".to_vec()))),
            child: None,
            contained: None,
        };
        let (_keep, cancel) = tokio::sync::oneshot::channel();
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), command_output(p, cancel)).await.unwrap();
        assert!(result.confirmed, "the recording started no process");
        assert!(result.success && result.text.ends_with("TAIL_OKSTDERR_OK"));
        assert!(result.text.len() <= 8192);
    }

    #[cfg(windows)]
    #[test]
    fn a_background_reader_is_created_at_idle_priority_and_an_ordinary_process_is_not() {
        const NO_WINDOW_SUSPENDED: u32 = 0x0800_0000 | 0x0000_0004;
        const IDLE: u32 = 0x0000_0040;
        assert_eq!(creation_flags(false), NO_WINDOW_SUSPENDED);
        assert_eq!(creation_flags(true), NO_WINDOW_SUSPENDED | IDLE);
    }

    #[test]
    fn the_background_mark_is_set_only_while_the_reader_is_being_started() {
        let seen = Arc::new(Mutex::new(vec![]));
        let record = seen.clone();
        let spawn: Spawner = Arc::new(move |_, _, _| {
            record.lock().unwrap().push(starting_in_background());
            Err(io::Error::other("nothing starts in this test"))
        });
        assert!(!starting_in_background());
        assert!(spawn_background(&spawn, "x", &[], Path::new(".")).is_err());
        assert!(spawn("x", &[], Path::new(".")).is_err());
        assert!(!starting_in_background(), "the mark is lifted afterwards");
        assert_eq!(*seen.lock().unwrap(), vec![true, false]);
    }
}
