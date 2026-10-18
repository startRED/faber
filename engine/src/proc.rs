//! The slice of a child process a driver needs, so tests can replay a recording without spawning anything.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::time::{Duration, Instant};

/// Only automatic roles arm silence; explicit interrupts always have a fixed grace period.
#[derive(Default)]
pub(crate) struct TurnWatch {
    running: bool,
    silent: Option<Instant>,
    interrupt: Option<Instant>,
    awaiting_permission: bool,
}

impl TurnWatch {
    pub fn begin(&mut self, automated: bool) {
        self.running = true;
        self.silent = automated.then(|| Instant::now() + Duration::from_secs(120));
        self.awaiting_permission = false;
    }
    pub fn activity(&mut self) {
        if self.silent.is_some() {
            self.silent = Some(Instant::now() + Duration::from_secs(120));
        }
    }
    pub fn interrupt(&mut self) {
        self.interrupt.get_or_insert_with(|| Instant::now() + Duration::from_secs(5));
    }
    pub fn finish(&mut self) {
        self.running = false;
        self.silent = None;
        self.interrupt = None;
        self.awaiting_permission = false;
    }
    pub fn pause(&mut self) {
        self.awaiting_permission = true;
    }
    pub fn resume(&mut self) {
        self.awaiting_permission = false;
        self.activity();
    }
    pub fn stopping(&self) -> bool {
        self.interrupt.is_some()
    }
    pub fn active(&self) -> bool {
        self.running || self.interrupt.is_some()
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.interrupt.or(if self.awaiting_permission { None } else { self.silent })
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

/// Drain both pipes without retaining an unbounded build log; dropping the future kills the owned child.
pub(crate) async fn command_output(mut p: Proc) -> (bool, String) {
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
    drop(p.stdin);
    let stderr = p.stderr.take().unwrap_or_else(|| Box::new(tokio::io::empty()));
    let (out, err, code) = tokio::join!(drain(p.stdout), drain(stderr), exit_code(&mut p.child));
    (code == "0" && out.is_ok() && err.is_ok(), format!("{}{}", out.unwrap_or_default(), err.unwrap_or_default()))
}

/// Starts a real process with piped stdio, no shell and, on Windows, no console window.
pub fn real_spawner() -> Spawner {
    Arc::new(|bin, args, cwd| {
        let mut cmd = Command::new(bin);
        cmd.args(args).current_dir(cwd);
        spawn_command(cmd)
    })
}

/// Starts a proof: code a role may have written, run by the engine. It inherits only its recorded environment and may write
/// only in the folder it runs in (the mission clone) and in `writable` (its build folder, its own tool caches and temporary
/// folder). On Windows the process runs at Low integrity and those folders are labelled Low, so Windows refuses its writes
/// anywhere else: the installed app, the engine's records, the owner checkout, the user profile. On macOS `sandbox-exec`
/// denies writes outside them. Reads and the network stay open (a residual, docs/design/projects.md); Linux has no write limit yet.
pub fn isolated_spawner(environment: std::collections::BTreeMap<String, String>, writable: Vec<std::path::PathBuf>) -> Spawner {
    // ponytail: a folder is labelled once per spawner; a folder deleted and made again meanwhile stays Medium, so the proof
    // fails (closed) instead of writing elsewhere.
    #[cfg(windows)]
    let labelled = Mutex::new(std::collections::HashSet::new());
    Arc::new(move |bin, args, cwd| {
        let mut roots = writable.clone();
        for root in &roots {
            std::fs::create_dir_all(root)?;
        }
        roots.push(cwd.to_path_buf());
        #[cfg(windows)]
        {
            let mut done = labelled.lock().unwrap_or_else(|e| e.into_inner());
            for root in &roots {
                if !done.contains(root) {
                    faber_winproc::label_low(root).map_err(|e| {
                        io::Error::new(e.kind(), format!("the proof folder {} cannot be labelled Low: {e}", root.display()))
                    })?;
                    done.insert(root.clone());
                }
            }
        }
        #[cfg(target_os = "macos")]
        let mut cmd = {
            let mut cmd = Command::new("/usr/bin/sandbox-exec");
            cmd.arg("-p").arg(write_profile(&roots)?).arg(bin);
            cmd
        };
        #[cfg(not(target_os = "macos"))]
        let mut cmd = Command::new(bin);
        cmd.env_clear().envs(&environment).args(args).current_dir(cwd);
        spawn_in_job(cmd, join_job, true)
    })
}

/// The sandbox profile of a proof on macOS: everything as usual, except writing, which only `roots` (and the null and
/// terminal devices) allow.
#[cfg(target_os = "macos")]
fn write_profile(roots: &[std::path::PathBuf]) -> io::Result<String> {
    let mut allowed = String::new();
    for root in roots {
        let real = std::fs::canonicalize(root)?;
        let text =
            real.to_str().filter(|t| !t.contains(['"', '\\'])).ok_or_else(|| io::Error::other("a proof folder needs a plain path"))?;
        allowed.push_str(&format!(" (subpath \"{text}\")"));
    }
    Ok(format!(
        "(version 1)(allow default)(deny file-write* (require-not (require-any{allowed} (literal \"/dev/null\") (literal \"/dev/tty\"))))"
    ))
}

thread_local! {
    /// Set while a qualified CLI is being started: the SHA-256 its image must have (see `spawn_expecting`).
    static EXPECTED_IMAGE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Starts a qualified CLI through `start`, which must spawn it on this thread (as `real_spawner` does). On Windows the
/// process is created suspended; before it runs, the file its image came from is hashed and compared with `sha256`, the
/// hash the qualification receipt names. Another program (a CLI swapped between admission and launch) is killed before it
/// runs a single instruction, and the start fails.
pub fn spawn_expecting<T>(sha256: &str, start: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            EXPECTED_IMAGE.with(|e| e.borrow_mut().take());
        }
    }
    EXPECTED_IMAGE.with(|e| *e.borrow_mut() = Some(sha256.to_string()));
    let _reset = Reset;
    start()
}

/// Starts `cmd` with piped stdio and, on Windows, inside its own job object. The process is created suspended, put in the job
/// and only then allowed to run, so nothing it starts can be outside the job. If that fails the process is killed and the
/// error returned: no CLI ever runs uncontained.
pub fn spawn_command(cmd: Command) -> io::Result<Proc> {
    spawn_in_job(cmd, join_job, false)
}

thread_local! {
    /// Set while a project check (`crate::verify`) is being started: the process is created at below-normal priority.
    static CHECK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Starts a project check like `spawn_command` (no window, its own kill-on-close job), at below-normal priority on Windows
/// so a build never makes the person's apps or a game stutter. Elsewhere it runs at the normal priority.
pub fn spawn_check(cmd: Command) -> io::Result<Proc> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CHECK.with(|c| c.set(false));
        }
    }
    CHECK.with(|c| c.set(true));
    let _reset = Reset;
    spawn_command(cmd)
}

/// `low`: lower the suspended process to Low integrity before it runs (Windows; a proof).
fn spawn_in_job(mut cmd: Command, join: impl FnOnce(&Child) -> io::Result<Containment>, low: bool) -> io::Result<Proc> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    {
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        let background = starting_in_background();
        let below = !background && CHECK.with(|c| c.get());
        cmd.creation_flags(creation_flags(background) | if below { BELOW_NORMAL_PRIORITY_CLASS } else { 0 });
    }
    let expected = EXPECTED_IMAGE.with(|e| e.borrow_mut().take());
    #[cfg(not(windows))]
    let _ = (low, expected);
    let mut child = cmd.spawn()?;
    let joined = join(&child);
    #[cfg(windows)]
    let joined = joined.and_then(|job| {
        let pid = child.id().ok_or_else(|| io::Error::other("the process already ended"))?;
        if low {
            faber_winproc::lower_to_low_integrity(pid)
                .map_err(|e| io::Error::new(e.kind(), format!("the proof cannot run at Low integrity: {e}")))?;
        }
        if let Some(expected) = expected {
            let image = faber_winproc::image_path(pid)?;
            if crate::project::qualification::fingerprint(&image)? != expected {
                return Err(io::Error::other("the started program is not the qualified one"));
            }
        }
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

/// Everything a CLI starts, held so it can all be ended together. On Windows this is a job object with kill-on-close and no
/// breakaway: every process the CLI starts joins the job, and ending the job ends them all, whether or not the CLI is still
/// alive (a process tree found through parent links is lost as soon as the parent exits). Dropping it ends the processes
/// too, so a CLI that quits on its own leaves no worker behind. Elsewhere there is no job and only the CLI itself is ended.
pub struct Containment {
    #[cfg(windows)]
    job: Arc<Mutex<Option<win32job::Job>>>,
    #[cfg(windows)]
    identity: crate::project::ProcessId,
}

impl Containment {
    /// The launched tree's owned identity, for a lease and its release proof. `None` where no job owns the tree.
    pub fn owned_tree(&self) -> Option<OwnedTree> {
        #[cfg(windows)]
        {
            Some(OwnedTree { identity: self.identity, job: self.job.clone() })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }
}

impl Containment {
    /// What caps the processor share of the job's processes (`Some(percent)`) or lifts the cap (`None`): the game guard of
    /// a check (`crate::verify`), saying whether Windows took it. Nothing elsewhere than Windows, which has no job.
    pub fn cpu_cap(&self) -> Box<dyn Fn(Option<u32>) -> bool + Send + Sync> {
        #[cfg(windows)]
        {
            let job = self.job.clone();
            Box::new(move |percent| {
                job.lock()
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
            let job = self.job.lock().unwrap_or_else(|e| e.into_inner());
            job.as_ref().and_then(|job| faber_winproc::job_peak_memory(job.handle()).ok())
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Ends every process of the job now. Closing the job does the same, but only asks: this is followed by `active`.
    fn end(&self) {
        #[cfg(windows)]
        if let Some(job) = self.job.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            let _ = faber_winproc::job_terminate(job.handle());
        }
    }

    /// How many processes of the job are still running; a job that cannot be asked counts as busy.
    fn active(&self) -> u32 {
        #[cfg(windows)]
        {
            self.job
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .map_or(0, |job| faber_winproc::job_active_processes(job.handle()).unwrap_or(1))
        }
        #[cfg(not(windows))]
        {
            0
        }
    }
}

/// Jobs whose processes were still alive when their wait ran out. The next use of a folder they could write in checks them.
static LINGERING: Mutex<Vec<Containment>> = Mutex::new(Vec::new());
const SETTLE: std::time::Duration = std::time::Duration::from_secs(10);

/// Ends a job's processes and waits until it reports none left, for about ten seconds. A job that is not empty by then is
/// kept (see `jobs_quiet`) and `false` is returned: the caller fails closed.
pub(crate) async fn settle(contained: Option<Containment>) -> bool {
    let Some(job) = contained else { return true };
    job.end();
    let until = Instant::now() + SETTLE;
    while job.active() > 0 {
        if Instant::now() >= until {
            LINGERING.lock().unwrap_or_else(|e| e.into_inner()).push(job);
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    // The job counts no process, but Windows closes the handles of an ended process a moment later (files it held open).
    #[cfg(windows)]
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    true
}

/// Whether no job that `settle` gave up on has a process left. Asked before Faber touches a folder a proof could write in;
/// it waits (blocking, up to ten seconds) for the stragglers to go.
pub fn jobs_quiet() -> bool {
    let until = Instant::now() + SETTLE;
    loop {
        let mut left = LINGERING.lock().unwrap_or_else(|e| e.into_inner());
        left.retain(|job| job.active() > 0);
        if left.is_empty() {
            return true;
        }
        drop(left);
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(windows)]
impl Drop for Containment {
    fn drop(&mut self) {
        // The job may be shared with an `OwnedTree`: closing it here ends the processes whoever holds a clone.
        drop(self.job.lock().unwrap_or_else(|e| e.into_inner()).take());
    }
}

/// What a lease needs to know about one launched CLI tree, shared with whoever later releases the lease.
/// It exists only for a process this engine started inside its own kill-on-close job (Windows); nothing can build
/// one from a PID, a snapshot or a recorded file.
#[derive(Clone)]
pub struct OwnedTree {
    identity: crate::project::ProcessId,
    #[cfg(windows)]
    job: Arc<Mutex<Option<win32job::Job>>>,
}

impl std::fmt::Debug for OwnedTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OwnedTree({})", self.identity.pid)
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
    /// The root's pid, with the microsecond this engine launched it: a reused pid has another start.
    pub fn identity(&self) -> crate::project::ProcessId {
        self.identity
    }

    /// Closes the kill-on-close job, so every member ends, then waits until the root has ended. Safe to repeat: a tree
    /// already retired answers with the same identity. A pid that was reused meanwhile only delays the answer until it fails.
    pub async fn retire(&self) -> io::Result<Retired> {
        #[cfg(windows)]
        {
            drop(self.job.lock().unwrap_or_else(|e| e.into_inner()).take());
            let deadline = Instant::now() + Duration::from_secs(5);
            while !faber_winproc::process_ended(self.identity.pid) {
                if Instant::now() >= deadline {
                    return Err(io::Error::other("owned tree did not end"));
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(Retired { identity: self.identity })
        }
        #[cfg(not(windows))]
        {
            Err(io::Error::other("owned trees need the Windows job"))
        }
    }
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
    Ok(Containment { job: Arc::new(Mutex::new(Some(job))), identity })
}

#[cfg(not(windows))]
fn join_job(_child: &Child) -> io::Result<Containment> {
    Ok(Containment {})
}

/// Ends a process and everything it started, then closes its input. The processes go first: a CLI that quits when its input
/// closes would otherwise be gone before the tree could be found, and its workers would keep running.
pub(crate) async fn stop(stdin: &mut (dyn AsyncWrite + Send + Unpin), child: &mut Option<Child>, contained: &mut Option<Containment>) {
    // Dropping the kept job closes it; on other systems there is nothing to close.
    let _ = stop_keeping(stdin, child, contained).await;
}

/// `stop`, but the job is handed back after its processes were told to end, so the caller can wait for them (`settle`).
pub(crate) async fn stop_keeping(
    stdin: &mut (dyn AsyncWrite + Send + Unpin),
    child: &mut Option<Child>,
    contained: &mut Option<Containment>,
) -> Option<Containment> {
    // Ending the job ends every process in it.
    let kept = contained.take();
    if let Some(job) = &kept {
        job.end();
    }
    let jobbed = kept.is_some();
    if let Some(child) = child {
        if !jobbed {
            kill_tree(child.id()).await;
        }
        let _ = child.start_kill();
    }
    let _ = stdin.shutdown().await;
    kept
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

    #[tokio::test(start_paused = true)]
    async fn waiting_for_permission_has_no_silence_deadline_but_stop_is_still_bounded() {
        let mut watch = TurnWatch::default();
        watch.begin(true);
        watch.pause();
        watch.activity();
        assert!(watch.active());
        assert_eq!(watch.deadline(), None);
        watch.interrupt();
        assert_eq!(watch.deadline(), Some(Instant::now() + Duration::from_secs(5)));
        watch.finish();
        watch.begin(true);
        watch.pause();
        watch.resume();
        assert_eq!(watch.deadline(), Some(Instant::now() + Duration::from_secs(120)));
    }

    #[tokio::test(start_paused = true)]
    async fn output_and_repeated_interrupts_cannot_extend_the_stop_grace() {
        let mut watch = TurnWatch::default();
        assert!(!watch.active());
        watch.begin(true);
        assert_eq!(watch.deadline(), Some(Instant::now() + Duration::from_secs(120)));
        watch.interrupt();
        let stop = watch.deadline();
        tokio::time::advance(Duration::from_secs(4)).await;
        watch.activity();
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
        let failed = spawn_in_job(cmd, |_| Err(io::Error::other("no job today")), false);
        assert!(failed.is_err_and(|e| e.to_string().contains("no job today")));
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(!marker.exists(), "the process ran although it was not contained");
    }

    /// Asks a proof process to write `target`, through the proof spawner, and says whether the file then holds its text.
    #[cfg(any(windows, target_os = "macos"))]
    async fn proof_writes(spawn: &Spawner, cwd: &Path, target: &Path) -> bool {
        #[cfg(windows)]
        let (program, args) = ("cmd.exe".to_string(), vec!["/d".into(), "/c".into(), "echo".into(), format!("proof>{}", target.display())]);
        #[cfg(target_os = "macos")]
        let (program, args) = ("/bin/sh".to_string(), vec!["-c".into(), format!("echo proof > '{}'", target.display())]);
        let Ok(p) = spawn(&program, &args, cwd) else { return false };
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), command_output(p)).await;
        std::fs::read_to_string(target).is_ok_and(|text| text.contains("proof"))
    }

    /// R138: a proof runs code a role wrote. It may write in the mission clone (an existing file and a new one) and in its
    /// own folders, and nowhere else: a folder standing in for the engine's records or the installed app stays untouched.
    #[cfg(any(windows, target_os = "macos"))]
    #[tokio::test]
    async fn a_proof_writes_inside_its_clone_and_own_folders_and_is_refused_everywhere_else() {
        let (clone, own, outside) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        std::fs::write(clone.path().join("kept.txt"), "before").unwrap();
        let mut environment = std::collections::BTreeMap::new();
        for name in ["SystemRoot", "PATH"] {
            if let Ok(value) = std::env::var(name) {
                environment.insert(name.to_string(), value);
            }
        }
        let tmp = own.path().join("tmp");
        let spawn = isolated_spawner(environment, vec![tmp.clone()]);
        assert!(proof_writes(&spawn, clone.path(), &clone.path().join("kept.txt")).await, "an existing file of the clone");
        assert!(proof_writes(&spawn, clone.path(), &clone.path().join("new.txt")).await, "a new file in the clone");
        assert!(proof_writes(&spawn, clone.path(), &tmp.join("scratch.txt")).await, "its own temporary folder");
        let journal = outside.path().join("journal.json");
        std::fs::write(&journal, "records").unwrap();
        assert!(!proof_writes(&spawn, clone.path(), &journal).await, "an existing file outside");
        assert_eq!(std::fs::read_to_string(&journal).unwrap(), "records");
        assert!(!proof_writes(&spawn, clone.path(), &outside.path().join("planted.exe")).await, "a new file outside");
        assert!(!outside.path().join("planted.exe").exists());
    }

    /// F5: a CLI swapped between its admission and its launch never runs. The image of the suspended process is hashed
    /// before it is resumed; a hash other than the receipt's kills it, and the right one lets it run.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_program_whose_image_is_not_the_qualified_one_is_killed_before_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("ran.txt");
        let cmd_exe = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap()).join("System32").join("cmd.exe");
        let args = vec!["/d".to_string(), "/c".into(), "echo".into(), format!("x>{}", marker.display())];
        let spawn = real_spawner();
        let wrong = "0".repeat(64);
        let failed = spawn_expecting(&wrong, || spawn(&cmd_exe.to_string_lossy(), &args, dir.path()));
        assert!(failed.is_err_and(|e| e.to_string().contains("not the qualified one")));
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(!marker.exists(), "the swapped program ran");
        let right = crate::project::qualification::fingerprint(&cmd_exe).unwrap();
        let p = spawn_expecting(&right, || spawn(&cmd_exe.to_string_lossy(), &args, dir.path())).unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), command_output(p)).await;
        assert!(marker.exists(), "the qualified program did not run");
        // The expectation covers one start only.
        assert!(spawn(&cmd_exe.to_string_lossy(), &args, dir.path()).is_ok());
    }

    /// F5, second round: between the hash and the run nothing can swap the program. The admission pins the file (no
    /// write, rename or delete while held), hashes it through that handle, and the launch runs it while still pinned.
    #[cfg(windows)]
    #[tokio::test]
    async fn a_pinned_program_cannot_be_swapped_until_it_has_started() {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("cli.exe");
        std::fs::copy(std::path::PathBuf::from(std::env::var("SystemRoot").unwrap()).join("System32").join("cmd.exe"), &cli).unwrap();
        let pinned = crate::project::qualification::pin(&cli).unwrap();
        assert_eq!(pinned.sha256, crate::project::qualification::fingerprint(&cli).unwrap());
        assert!(std::fs::OpenOptions::new().write(true).open(&cli).is_err(), "written while pinned");
        assert!(std::fs::rename(&cli, dir.path().join("moved.exe")).is_err(), "renamed while pinned");
        assert!(std::fs::remove_file(&cli).is_err(), "deleted while pinned");
        let marker = dir.path().join("ran.txt");
        let args = vec!["/d".to_string(), "/c".into(), "echo".into(), format!("x>{}", marker.display())];
        let p = spawn_expecting(&pinned.sha256, || real_spawner()(&cli.to_string_lossy(), &args, dir.path())).unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(10), command_output(p)).await;
        assert!(marker.exists(), "the pinned program did not run");
        drop(pinned);
        std::fs::rename(&cli, dir.path().join("moved.exe")).unwrap();
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
        let (ok, text) = tokio::time::timeout(std::time::Duration::from_secs(3), command_output(p)).await.unwrap();
        assert!(ok && text.ends_with("TAIL_OKSTDERR_OK"));
        assert!(text.len() <= 8192);
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
