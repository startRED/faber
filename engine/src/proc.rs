//! The slice of a child process a driver needs, so tests can replay a recording without spawning anything.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(not(windows))]
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
    pub child: Option<Process>,
}

/// Owned handles only. Windows owns a non-inheritable kill-on-close job; no descendant is ever adopted by PID.
pub struct Process {
    #[cfg(not(windows))]
    inner: Child,
    #[cfg(windows)]
    inner: Arc<Mutex<std::process::Child>>,
    #[cfg(windows)]
    job: Arc<Mutex<Option<win32job::Job>>>,
    #[cfg(windows)]
    identity: crate::project::ProcessId,
}

/// What a lease needs to know about one launched CLI tree, shared with whoever later releases the lease.
/// It exists only for a process this engine started inside its own kill-on-close job (Windows); nothing can build
/// one from a PID, a snapshot or a recorded file.
#[derive(Clone)]
pub struct OwnedTree {
    identity: crate::project::ProcessId,
    #[cfg(windows)]
    job: Arc<Mutex<Option<win32job::Job>>>,
    #[cfg(windows)]
    root: Arc<Mutex<std::process::Child>>,
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

    /// Closes the kill-on-close job, so every member ends, then waits for the root to be reaped. Safe to repeat: a
    /// tree already retired answers with the same identity and never touches another process.
    pub async fn retire(&self) -> io::Result<Retired> {
        #[cfg(windows)]
        {
            drop(self.job.lock().unwrap().take());
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                {
                    let mut root = self.root.lock().unwrap();
                    if root.try_wait()?.is_some() {
                        break;
                    }
                    root.kill()?;
                }
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

impl Process {
    pub fn id(&self) -> Option<u32> {
        #[cfg(windows)]
        {
            Some(self.identity.pid)
        }
        #[cfg(not(windows))]
        {
            self.inner.id()
        }
    }

    /// The launched tree's owned identity, for a lease and its release proof. `None` where no job owns the tree.
    pub fn owned_tree(&self) -> Option<OwnedTree> {
        #[cfg(windows)]
        {
            Some(OwnedTree { identity: self.identity, job: self.job.clone(), root: self.inner.clone() })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub fn start_kill(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            drop(self.job.lock().unwrap().take());
            let mut root = self.inner.lock().unwrap();
            if root.try_wait()?.is_none() {
                root.kill()?;
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            self.inner.start_kill()
        }
    }

    pub async fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
        #[cfg(windows)]
        {
            loop {
                if let Some(status) = self.inner.lock().unwrap().try_wait()? {
                    return Ok(status);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
        #[cfg(not(windows))]
        {
            self.inner.wait().await
        }
    }
}

#[cfg(windows)]
impl Drop for Process {
    fn drop(&mut self) {
        drop(self.job.lock().unwrap().take());
        let _ = self.inner.lock().unwrap().kill();
    }
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

/// Starts a real process with piped stdio, no shell and, on Windows, no console window.
pub fn real_spawner() -> Spawner {
    configured_spawner(None)
}

/// Proof executors inherit only their recorded environment, including the process guard.
pub fn isolated_spawner(environment: std::collections::BTreeMap<String, String>) -> Spawner {
    configured_spawner(Some(environment))
}

fn configured_spawner(environment: Option<std::collections::BTreeMap<String, String>>) -> Spawner {
    Arc::new(move |bin, args, cwd| {
        #[cfg(windows)]
        {
            spawn_windows(bin, args, cwd, environment.as_ref())
        }
        #[cfg(not(windows))]
        {
            let mut cmd = Command::new(bin);
            if let Some(environment) = &environment {
                cmd.env_clear().envs(environment);
            }
            cmd.args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
            let mut child = cmd.spawn()?;
            Ok(Proc {
                stdin: Box::new(child.stdin.take().expect("piped stdin")),
                stdout: Box::new(child.stdout.take().expect("piped stdout")),
                stderr: child.stderr.take().map(|s| Box::new(s) as Box<dyn AsyncRead + Send + Unpin>),
                child: Some(Process { inner: child }),
            })
        }
    })
}

#[cfg(windows)]
fn spawn_windows(
    bin: &str,
    args: &[String],
    cwd: &Path,
    environment: Option<&std::collections::BTreeMap<String, String>>,
) -> io::Result<Proc> {
    use std::io::Write;
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    let job = win32job::Job::create().map_err(io::Error::other)?;
    let mut limits = job.query_extended_limit_info().map_err(io::Error::other)?;
    limits.limit_kill_on_job_close();
    job.set_extended_limit_info(&limits).map_err(io::Error::other)?;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    if let Some(environment) = environment {
        command.env_clear().envs(environment);
    }
    let child = command
        .arg("--faber-process-guard")
        .arg(bin)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .spawn()?;
    let started_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_micros() as u64).max(1);
    let identity = crate::project::ProcessId { pid: child.id(), started_at };
    let process = Process { inner: Arc::new(Mutex::new(child)), job: Arc::new(Mutex::new(Some(job))), identity };
    process
        .job
        .lock()
        .unwrap()
        .as_ref()
        .expect("owned job")
        .assign_process(process.inner.lock().unwrap().as_raw_handle() as isize)
        .map_err(io::Error::other)?;
    let mut stdin = process.inner.lock().unwrap().stdin.take().expect("piped stdin");
    stdin.write_all(b"G")?;
    stdin.flush()?;
    let mut stdin = tokio::process::ChildStdin::from_std(stdin)?;
    let mut stdout = tokio::process::ChildStdout::from_std(process.inner.lock().unwrap().stdout.take().expect("piped stdout"))?;
    let (input, mut pending) = tokio::io::duplex(8192);
    let (mut output, reader) = tokio::io::duplex(8192);
    let job = process.job.clone();
    // Await startup outside the synchronous spawner/engine lock; no CLI input can precede the guard ack.
    tokio::spawn(async move {
        let mut ack = [0];
        if !matches!(tokio::time::timeout(Duration::from_secs(5), stdout.read_exact(&mut ack)).await, Ok(Ok(_))) || ack != *b"R" {
            drop(job.lock().unwrap().take());
            return;
        }
        let input = async move {
            let _ = tokio::io::copy(&mut pending, &mut stdin).await;
            let _ = stdin.shutdown().await;
        };
        let output = async move {
            let _ = tokio::io::copy(&mut stdout, &mut output).await;
            let _ = output.shutdown().await;
        };
        tokio::join!(input, output);
    });
    let stderr = process.inner.lock().unwrap().stderr.take().map(tokio::process::ChildStderr::from_std).transpose()?;
    Ok(Proc {
        stdin: Box::new(input),
        stdout: Box::new(reader),
        stderr: stderr.map(|s| Box::new(s) as Box<dyn AsyncRead + Send + Unpin>),
        child: Some(process),
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

/// Ends the owned job on Windows and the held child elsewhere. It never accepts a PID or a process snapshot.
pub(crate) async fn stop(stdin: &mut (dyn AsyncWrite + Send + Unpin), child: &mut Option<Process>) {
    let _ = tokio::time::timeout(Duration::from_secs(1), stdin.shutdown()).await;
    if let Some(child) = child {
        let _ = child.start_kill();
    }
}

pub(crate) async fn exit_code(child: &mut Option<Process>) -> String {
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
