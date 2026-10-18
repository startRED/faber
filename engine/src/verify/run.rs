//! One check as a process: started with no shell and no window, inside its own kill-on-close job and below normal priority
//! (`crate::proc::spawn_check`), its output streamed through a bounded channel to the run's log on disk. Only a bounded
//! head and tail of the output stay in memory, for R155's readers, and they go when the check ends.

use std::future::Future;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt};

/// One command to start: the program by its bare name (found on the `PATH` of `env`), its arguments, where it runs and
/// what it adds to the engine's environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

/// A started check: its two pipes and its end. Dropping `exit` before it resolves ends the process and everything it
/// started.
pub struct Started {
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    pub stderr: Box<dyn AsyncRead + Send + Unpin>,
    /// The exit code, `None` when it ended without one.
    pub exit: Pin<Box<dyn Future<Output = Option<i32>> + Send>>,
    /// Caps the check's processor share while a game is in front (true) or lifts the cap (false), saying whether it took;
    /// none where nothing can.
    pub throttle: Option<Box<dyn Fn(bool) -> bool + Send + Sync>>,
}

/// Whether a game or another full-screen app is in front of the person (the shell's R191 reading), so no new check starts
/// and a running one is held to `GAME_CPU`.
pub type Guard = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;

/// The processor share, in percent of the whole machine, a check keeps while a game is in front.
pub const GAME_CPU: u32 = 10;

/// How often the guard is read while a run waits for a game to end or a check runs; never while idle.
pub const GUARD_EVERY: Duration = Duration::from_secs(5);

/// Starts checks. The shell gives the real one; tests give a fake that starts nothing.
pub trait Runner: Send + Sync {
    fn start(&self, spec: &Spec) -> io::Result<Started>;
}

/// Programs that are shells: a check never runs one (AGENTS.md: child processes never go through a shell).
const SHELLS: [&str; 7] = ["cmd", "powershell", "pwsh", "sh", "bash", "wscript", "cscript"];

/// The real runner: resolves the program like the CLIs are (an npm `.cmd` shim becomes `node script.js`), never a shell.
pub struct Real;

impl Runner for Real {
    fn start(&self, spec: &Spec) -> io::Result<Started> {
        let path = spec
            .env
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("PATH"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| std::env::var("PATH").or_else(|_| std::env::var("Path")).unwrap_or_default());
        let exists = |p: &str| Path::new(p).is_file();
        let cli =
            crate::cli::resolve_cli(&spec.program, &path, cfg!(windows), &exists, &|p| std::fs::read_to_string(p).unwrap_or_default())
                .or_else(|| crate::project::mission::node_tool(&spec.program, &path, cfg!(windows), &exists))
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{} is not on the PATH", spec.program)))?;
        let name = Path::new(&cli.command).file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
        let lower = cli.command.to_ascii_lowercase();
        if SHELLS.contains(&name.as_str()) || lower.ends_with(".cmd") || lower.ends_with(".bat") {
            return Err(io::Error::other("a check never runs a shell"));
        }
        let mut cmd = tokio::process::Command::new(&cli.command);
        cmd.args(&cli.prefix).args(&spec.args).current_dir(&spec.cwd).envs(spec.env.iter().map(|(k, v)| (k, v)));
        let p = crate::proc::spawn_check(cmd)?;
        // Nothing is ever typed into a check.
        drop(p.stdin);
        let (child, job) = (p.child, p.contained);
        let throttle = job.as_ref().map(|job| {
            let cap = job.cpu_cap();
            Box::new(move |game: bool| cap(game.then_some(GAME_CPU))) as Box<dyn Fn(bool) -> bool + Send + Sync>
        });
        let exit = Box::pin(async move {
            // The job lives as long as this future: once the check exited, or when the future is dropped before, closing
            // it ends whatever the check left running (on Windows), so its pipes close and nothing outlives it.
            let _job = job;
            let mut child = child;
            match child.as_mut() {
                Some(child) => child.wait().await.ok().and_then(|s| s.code()),
                None => None,
            }
        });
        Ok(Started { stdout: p.stdout, stderr: p.stderr.unwrap_or_else(|| Box::new(tokio::io::empty())), exit, throttle })
    }
}

/// The log of one verify, on disk; a log that cannot be written leaves the run without one.
pub struct Log {
    file: Option<io::BufWriter<std::fs::File>>,
    /// What it may still take.
    left: u64,
}

/// The most logs kept per folder.
pub const KEPT_LOGS: usize = 20;

/// The most one log holds: a check that prints without end fills no disk, and the pipes are still read to their end.
pub const MAX_LOG: u64 = 64 * 1024 * 1024;

/// Whether a file in the log folder is one of Faber's logs (`<millis>-<fast|full>.log`): nothing else there is removed.
fn log_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix("-fast.log").or_else(|| name.strip_suffix("-full.log")) else { return false };
    !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `path` is a link (a symbolic link or, on Windows, a junction), which the log folder never follows.
fn linked(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

impl Log {
    /// A new log in `<folder>/.faber/verify/`, which ignores itself in git; the oldest beyond the last 20 go. A `.faber`
    /// or `verify` that is a link, or that resolves outside the folder, gets no log: Faber never writes or removes a file
    /// somewhere a repository pointed it to.
    pub fn open(folder: &Path, name: &str) -> (Log, Option<String>) {
        let none = (Log::none(), None);
        let (faber, dir) = (folder.join(".faber"), folder.join(".faber").join("verify"));
        if linked(&faber) || linked(&dir) || std::fs::create_dir_all(&dir).is_err() || linked(&faber) || linked(&dir) {
            return none;
        }
        match (folder.canonicalize(), dir.canonicalize()) {
            (Ok(root), Ok(real)) if real.starts_with(&root) => {}
            _ => return none,
        }
        let ignore = dir.join(".gitignore");
        if !ignore.is_file() {
            let _ = std::fs::write(&ignore, "*\n");
        }
        let mut logs: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|d| {
                d.filter_map(Result::ok)
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_file()) && e.file_name().to_str().is_some_and(log_name))
                    .map(|e| e.path())
                    .collect()
            })
            .unwrap_or_default();
        logs.sort();
        for old in logs.iter().take((logs.len() + 1).saturating_sub(KEPT_LOGS)) {
            let _ = std::fs::remove_file(old);
        }
        match std::fs::File::create(dir.join(name)) {
            Ok(file) => (Log { file: Some(io::BufWriter::new(file)), left: MAX_LOG }, Some(format!(".faber/verify/{name}"))),
            Err(_) => none,
        }
    }

    pub fn none() -> Log {
        Log { file: None, left: 0 }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let Some(file) = &mut self.file else { return };
        let room = usize::try_from(self.left).unwrap_or(usize::MAX);
        let take = room.min(bytes.len());
        self.left -= take as u64;
        let mut ok = file.write_all(&bytes[..take]).is_ok();
        if take < bytes.len() {
            // Full: one last line says so, and nothing more is written.
            let _ = file.write_all(b"\n... the log stops here: it reached its 64 MiB.\n");
            let _ = file.flush();
            ok = false;
        }
        if !ok {
            self.file = None;
        }
    }

    pub fn line(&mut self, text: &str) {
        self.write(format!("{text}\n").as_bytes());
    }

    pub fn flush(&mut self) {
        if let Some(file) = &mut self.file {
            let _ = file.flush();
        }
    }
}

/// What R155's readers get of an output: its first and its last bytes, and how many were left out between.
const HEAD: usize = 256 * 1024;
const TAIL: usize = 768 * 1024;

#[derive(Default)]
pub struct Capture {
    head: Vec<u8>,
    tail: Vec<u8>,
    skipped: u64,
}

impl Capture {
    pub fn push(&mut self, mut bytes: &[u8]) {
        let room = HEAD - self.head.len();
        let take = room.min(bytes.len());
        self.head.extend_from_slice(&bytes[..take]);
        bytes = &bytes[take..];
        self.tail.extend_from_slice(bytes);
        if self.tail.len() > TAIL {
            let cut = self.tail.len() - TAIL;
            self.skipped += cut as u64;
            self.tail.drain(..cut);
        }
    }

    pub fn text(&self) -> String {
        let mut text = String::from_utf8_lossy(&self.head).into_owned();
        if self.skipped > 0 {
            text.push_str(&format!("\n... ({} bytes left out here; the log has them)\n", self.skipped));
        }
        text.push_str(&String::from_utf8_lossy(&self.tail));
        text
    }

    /// The bytes held, never more than the head and the tail.
    #[cfg(test)]
    pub fn held(&self) -> usize {
        self.head.len() + self.tail.len()
    }
}

/// How one check ended.
pub struct Ran {
    /// `None`: it did not finish (it ran out of time) or ended without a code.
    pub exit: Option<i32>,
    pub output: String,
    pub took: Duration,
    /// Why it could not start, when it did not.
    pub refused: Option<String>,
    pub timed_out: bool,
}

/// Chunks in flight between the pipes and the log: a slow disk holds the pipes back instead of growing memory.
const CHANNEL: usize = 16;

async fn pump(mut pipe: Box<dyn AsyncRead + Send + Unpin>, tx: tokio::sync::mpsc::Sender<Vec<u8>>) {
    let mut buf = vec![0u8; 8192];
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if tx.send(buf[..n].to_vec()).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// How long the pipes are still read once the check itself exited.
const GRACE: Duration = Duration::from_secs(2);

/// Runs one check to its end or its deadline, with every byte it prints going to `log`. With a `guard`, the check's
/// processor share is capped while a game is in front and freed when it goes.
pub async fn check(runner: &dyn Runner, spec: &Spec, log: &mut Log, deadline: Duration, guard: Option<&Guard>) -> Ran {
    let began = Instant::now();
    let started = match runner.start(spec) {
        Ok(started) => started,
        Err(e) => {
            log.line(&format!("could not start: {e}"));
            return Ran { exit: None, output: String::new(), took: began.elapsed(), refused: Some(e.to_string()), timed_out: false };
        }
    };
    let Started { stdout, stderr, exit, throttle } = started;
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(CHANNEL);
    let mut capture = Capture::default();
    let code = {
        let consume = async {
            while let Some(chunk) = rx.recv().await {
                log.write(&chunk);
                capture.push(&chunk);
            }
        };
        let work = async {
            let read = async { tokio::join!(pump(stdout, tx.clone()), pump(stderr, tx), consume) };
            tokio::pin!(read);
            let mut exit = exit;
            tokio::select! {
                // Something the check started may still hold its pipes (off Windows no job ends it): what it printed by
                // now is read, and the check ends with its own code instead of waiting for its deadline.
                code = &mut exit => {
                    let _ = tokio::time::timeout(GRACE, &mut read).await;
                    code
                }
                _ = &mut read => exit.await,
            }
        };
        // Time spent capped for a game does not count against the deadline: a check slowed on purpose is no failure.
        let held_for = std::sync::atomic::AtomicU64::new(0);
        let start = tokio::time::Instant::now();
        let held = async {
            let (Some(guard), Some(throttle)) = (guard, throttle.as_ref()) else { return std::future::pending().await };
            let mut capped = false;
            loop {
                let game = guard();
                // A cap Windows refused is asked again at the next reading.
                if game != capped && throttle(game) {
                    capped = game;
                }
                tokio::time::sleep(GUARD_EVERY).await;
                if capped {
                    held_for.fetch_add(GUARD_EVERY.as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
                }
            }
        };
        let expiry = async {
            loop {
                let end = start + deadline + Duration::from_millis(held_for.load(std::sync::atomic::Ordering::Relaxed));
                if tokio::time::Instant::now() >= end {
                    return;
                }
                tokio::time::sleep_until(end).await;
            }
        };
        tokio::select! {
            code = work => Some(code),
            never = held => never,
            () = expiry => None,
        }
    };
    let (exit, timed_out) = match code {
        Some(code) => (code, false),
        // Dropping the work ended the process and its job.
        None => (None, true),
    };
    log.flush();
    Ran { exit, output: capture.text(), took: began.elapsed(), refused: None, timed_out }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A check that exits at once with `code` while something it left keeps its output open forever.
    struct Held(i32);

    impl Runner for Held {
        fn start(&self, _: &Spec) -> io::Result<Started> {
            let code = self.0;
            let (open, holder) = tokio::io::duplex(64);
            // The other end never closes, as a grandchild that inherited the pipe would keep it.
            std::mem::forget(holder);
            Ok(Started {
                stdout: Box::new(open),
                stderr: Box::new(tokio::io::empty()),
                exit: Box::pin(async move { Some(code) }),
                throttle: None,
            })
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_check_that_exited_ends_with_its_code_even_when_something_holds_its_pipes() {
        let spec = Spec { program: "x".into(), args: vec![], cwd: ".".into(), env: vec![] };
        let ran = check(&Held(3), &spec, &mut Log::none(), Duration::from_secs(900), None).await;
        assert_eq!((ran.exit, ran.timed_out), (Some(3), false));
        assert!(ran.took < Duration::from_secs(10), "{:?}", ran.took);
    }

    #[test]
    fn a_log_stops_at_its_limit() {
        let dir = tempfile::tempdir().unwrap();
        let (mut log, path) = Log::open(dir.path(), "1-fast.log");
        assert_eq!(log.left, MAX_LOG);
        log.left = 1000;
        for _ in 0..5 {
            log.write(&[b'x'; 400]);
        }
        log.flush();
        let text = std::fs::read_to_string(dir.path().join(path.unwrap())).unwrap();
        assert_eq!(text.bytes().filter(|b| *b == b'x').count(), 1000);
        assert!(text.ends_with("the log stops here: it reached its 64 MiB.\n"));
    }

    #[test]
    fn only_faber_s_own_logs_are_removed_and_a_linked_folder_gets_none() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join(".faber").join("verify");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("notes.log"), "mine").unwrap();
        for n in 0..25 {
            Log::open(dir.path(), &format!("{n}-full.log")).0.flush();
        }
        assert!(folder.join("notes.log").is_file(), "a file Faber did not write stays");
        assert!(log_name("17-fast.log") && !log_name("x-fast.log") && !log_name("-fast.log") && !log_name("1-fast.txt"));
        // A `.faber` that points somewhere else is never written to.
        let elsewhere = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(elsewhere.path(), project.path().join(".faber")).is_ok();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(elsewhere.path(), project.path().join(".faber")).is_ok();
        if linked {
            let (_, path) = Log::open(project.path(), "1-fast.log");
            assert_eq!(path, None);
            assert!(std::fs::read_dir(elsewhere.path()).unwrap().next().is_none());
        }
    }

    #[test]
    fn only_the_head_and_the_tail_of_a_long_output_stay_in_memory() {
        let mut capture = Capture::default();
        capture.push(b"error[E0308]: first\n");
        for _ in 0..400 {
            capture.push(&[b'x'; 8192]);
        }
        capture.push(b"\ntest result: FAILED\n");
        assert!(capture.held() <= HEAD + TAIL);
        let text = capture.text();
        assert!(text.starts_with("error[E0308]: first") && text.ends_with("test result: FAILED\n"));
        assert!(text.contains("bytes left out here"));
    }

    #[test]
    fn the_logs_of_a_folder_keep_the_last_twenty_and_ignore_themselves_in_git() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..25 {
            let (mut log, path) = Log::open(dir.path(), &format!("{n:03}-fast.log"));
            log.line("x");
            log.flush();
            assert_eq!(path.as_deref(), Some(format!(".faber/verify/{n:03}-fast.log").as_str()));
        }
        let folder = dir.path().join(".faber").join("verify");
        let logs =
            std::fs::read_dir(&folder).unwrap().filter(|e| e.as_ref().unwrap().path().extension().is_some_and(|x| x == "log")).count();
        assert_eq!(logs, KEPT_LOGS);
        assert!(!folder.join("000-fast.log").exists() && folder.join("024-fast.log").exists());
        assert_eq!(std::fs::read_to_string(folder.join(".gitignore")).unwrap(), "*\n");
    }
}
