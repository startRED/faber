//! Owning what a launched app starts. The runner ends processes only through what it owns, never through a process id
//! it looked up: ids are handed to the next program that starts, so ending "the process with this id" can end someone
//! else's.
//!
//! - Windows: the app starts inside its own job object (kill on close, no breakaway). The job is made and filled before
//!   the app has run a single instruction: the runner starts the fake helper in `--gate` mode, puts it in the job, and
//!   only then tells it to start the app. The app, and everything it starts, is in the job from its first moment on.
//!   (Starting the app suspended and resuming it would need unsafe code, which the workspace forbids.) Ending the job
//!   ends all of them; closing the last handle to the job, which is what happens when the runner dies, does the same.
//!   Locally the app also starts on the hidden desktop the runner made (see `desktop`), so it can never show a window
//!   to the person using the PC. GitHub Actions uses the default desktop because WebView2 needs it there.
//! - macOS and Linux: the app is started directly and ended through the `Child` the runner keeps.
//!   ponytail: no process group is signalled (a group cannot prove who is in it) and nothing is adopted by id, so a
//!   program the app started and left running outlives it; the audit reports it and fails the scenario. Containing
//!   descendants there needs a Linux cgroup or a subreaper; CI on those systems is the gate until then.
//!
//! A process that is not in the job (it was there before the job existed, or somehow left it) is never ended: the audit
//! reports it.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::{Mutex, MutexGuard};

#[cfg(windows)]
use std::process::ChildStdin;

/// `CREATE_NO_WINDOW`: a console program started by the runner gets no console window of its own.
#[cfg(windows)]
pub const NO_WINDOW: u32 = 0x0800_0000;

pub struct Owned {
    /// The app on macOS and Linux; on Windows the gate helper, which lives exactly as long as the app does.
    child: Mutex<Child>,
    pid: u32,
    #[cfg(windows)]
    gate: Mutex<Option<ChildStdin>>,
    #[cfg(windows)]
    job: Mutex<Option<win32job::Job>>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Owned {
    /// Starts the program with exactly this environment and working folder, no input, and diagnostic streams
    /// piped back. It starts inside its containment; if it cannot be contained it is ended and this fails. `helper` is
    /// the fake helper binary, which the Windows containment needs.
    pub fn start(
        program: &Path,
        args: &[String],
        env: &BTreeMap<String, String>,
        cwd: &Path,
        helper: &Path,
    ) -> io::Result<(Owned, Option<ChildStdout>, Option<ChildStderr>)> {
        #[cfg(not(windows))]
        {
            let _ = helper;
            let mut command = Command::new(program);
            command.args(args).env_clear().envs(env).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
            let mut child = command.spawn()?;
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let pid = child.id();
            Ok((Owned { child: Mutex::new(child), pid }, stdout, stderr))
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // WebView2 cannot use a private desktop in the GitHub runner's service session.
            // Locally, no hidden desktop means no app; there is no fallback to the person's desktop.
            let desktop = if crate::desktop::hidden_required(env.get("GITHUB_ACTIONS").map(String::as_str)) {
                crate::desktop::name().map_err(io::Error::other)?
            } else {
                "default"
            };
            let mut command = Command::new(helper);
            command.arg("--gate").arg("--desktop").arg(desktop).arg(program).args(args).env_clear().envs(env).current_dir(cwd);
            command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(NO_WINDOW);
            let mut helper = command.spawn()?;
            let gate = helper.stdin.take();
            let stdout = helper.stdout.take();
            let stderr = helper.stderr.take();
            let fail = |helper: &mut Child, message: String| {
                let _ = helper.kill();
                let _ = helper.wait();
                io::Error::other(message)
            };
            let job = match job_for(&helper) {
                Ok(job) => job,
                Err(e) => return Err(fail(&mut helper, format!("cannot put the app in a job object: {e}"))),
            };
            // The helper is in the job and has started nothing: now it may start the app.
            let (Some(mut gate), Some(stdout)) = (gate, stdout) else {
                return Err(fail(&mut helper, "the helper has no pipes".into()));
            };
            if let Err(e) = io::Write::write_all(&mut gate, b"go\n").and_then(|()| io::Write::flush(&mut gate)) {
                return Err(fail(&mut helper, format!("cannot tell the helper to start the app: {e}")));
            }
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut line = String::new();
                let _ = io::BufRead::read_line(&mut io::BufReader::new(stdout), &mut line);
                let _ = sender.send(line);
            });
            let line = receiver.recv_timeout(std::time::Duration::from_secs(10)).unwrap_or_default();
            let Some(pid) = line.trim().strip_prefix("pid ").and_then(|n| n.parse::<u32>().ok()) else {
                drop(job);
                let _ = helper.wait();
                return Err(io::Error::other(format!("the app did not start: {}", line.trim())));
            };
            let owned = Owned { child: Mutex::new(helper), pid, gate: Mutex::new(Some(gate)), job: Mutex::new(Some(job)) };
            Ok((owned, None, stderr))
        }
    }

    /// The app's process id. It cannot be reused while the app is unreaped: on Windows the helper holds the app's handle
    /// until the app ends, elsewhere the runner does.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Whether the app itself has not ended. Asking reaps it if it has, so a later call can never name a reused id.
    pub fn is_running(&self) -> bool {
        matches!(locked(&self.child).try_wait(), Ok(None))
    }

    /// The exit status, once the app has ended (on Windows the helper ends with the app's own status).
    pub fn status(&self) -> Option<String> {
        locked(&self.child).try_wait().ok().flatten().map(|s| s.to_string())
    }

    /// The ids of every process in the app's job right now (Windows). `None` where there is no job, or it is closed.
    pub fn members(&self) -> Option<Vec<u32>> {
        #[cfg(windows)]
        {
            let job = locked(&self.job);
            job.as_ref().and_then(|job| job.query_process_id_list().ok()).map(|ids| ids.into_iter().map(|id| id as u32).collect())
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Ends the app itself and waits for it, leaving whatever it started alone so the audit can see whether it follows.
    pub fn stop_app(&self) {
        #[cfg(windows)]
        {
            // The helper ends the app through the handle it holds, and then ends itself.
            if let Some(gate) = locked(&self.gate).as_mut() {
                let _ = io::Write::write_all(gate, b"kill\n").and_then(|()| io::Write::flush(gate));
            }
            let mut child = locked(&self.child);
            for _ in 0..100 {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        #[cfg(not(windows))]
        {
            let mut child = locked(&self.child);
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Ends everything it can: the whole job on Windows, the app itself elsewhere. On Windows it returns once the system
    /// says the job holds no running process: ending a process only starts its end, and on a loaded PC, where everything
    /// the runner starts is below normal priority, a tree of webview processes can take seconds to finish going. Closing
    /// the job alone gave no way to know when that was, so the cleanup could look too early and find them still there.
    pub fn terminate(&self) {
        #[cfg(windows)]
        if let Some(job) = locked(&self.job).take() {
            let handle = job.handle();
            if faber_winproc::job_terminate(handle).is_ok() {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                while faber_winproc::job_active_processes(handle).is_ok_and(|n| n > 0) && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            drop(job);
        }
        let mut child = locked(&self.child);
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(windows)]
fn job_for(child: &Child) -> Result<win32job::Job, String> {
    use std::os::windows::io::AsRawHandle;
    let job = win32job::Job::create().map_err(|e| e.to_string())?;
    let mut info = job.query_extended_limit_info().map_err(|e| e.to_string())?;
    info.limit_kill_on_job_close();
    job.set_extended_limit_info(&info).map_err(|e| e.to_string())?;
    job.assign_process(child.as_raw_handle() as isize).map_err(|e| e.to_string())?;
    Ok(job)
}

// The containment is checked by a scenario of the `screen` suite (`screen/containment.rs`), not here: it starts real
// processes, which no unit test may do.
