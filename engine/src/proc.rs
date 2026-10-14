//! The slice of a child process a driver needs, so tests can replay a recording without spawning anything.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, Command};

pub struct Proc {
    pub stdin: Box<dyn AsyncWrite + Send + Unpin>,
    pub stdout: Box<dyn AsyncRead + Send + Unpin>,
    pub stderr: Option<Box<dyn AsyncRead + Send + Unpin>>,
    /// The real process, if there is one; a replayed recording has none.
    pub child: Option<Child>,
    /// What ends the process together with everything it started (a Windows job object); none for a replayed recording.
    pub contained: Option<Containment>,
}

pub type Spawner = Arc<dyn Fn(&str, &[String], &Path) -> io::Result<Proc> + Send + Sync>;

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

/// Starts `cmd` with piped stdio and, on Windows, inside its own job object. The process is created suspended, put in the job
/// and only then allowed to run, so nothing it starts can be outside the job. If that fails the process is killed and the
/// error returned: no CLI ever runs uncontained.
pub fn spawn_command(cmd: Command) -> io::Result<Proc> {
    spawn_in_job(cmd, join_job)
}

fn spawn_in_job(mut cmd: Command, join: impl FnOnce(&Child) -> io::Result<Containment>) -> io::Result<Proc> {
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000 | 0x0000_0004); // CREATE_NO_WINDOW | CREATE_SUSPENDED
    let mut child = cmd.spawn()?;
    let joined = join(&child);
    #[cfg(windows)]
    let joined = joined.and_then(|job| {
        faber_winproc::resume_process(child.id().ok_or_else(|| io::Error::other("the process already ended"))?)?;
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
    _job: win32job::Job,
}

#[cfg(windows)]
fn join_job(child: &Child) -> io::Result<Containment> {
    let handle = child.raw_handle().ok_or_else(|| io::Error::other("the process already ended"))?;
    let mut limits = win32job::ExtendedLimitInfo::new();
    limits.limit_kill_on_job_close();
    let job = win32job::Job::create_with_limit_info(&limits).map_err(io::Error::other)?;
    job.assign_process(handle as isize).map_err(io::Error::other)?;
    Ok(Containment { _job: job })
}

#[cfg(not(windows))]
fn join_job(_child: &Child) -> io::Result<Containment> {
    Ok(Containment {})
}

/// Ends a process and everything it started, then closes its input. The processes go first: a CLI that quits when its input
/// closes would otherwise be gone before the tree could be found, and its workers would keep running.
pub(crate) async fn stop(stdin: &mut (dyn AsyncWrite + Send + Unpin), child: &mut Option<Child>, contained: &mut Option<Containment>) {
    // Closing the last handle of the job ends every process in it.
    let jobbed = contained.take().is_some();
    if let Some(child) = child {
        if !jobbed {
            kill_tree(child.id()).await;
        }
        let _ = child.start_kill();
    }
    let _ = stdin.shutdown().await;
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
        let (ok, text) = tokio::time::timeout(std::time::Duration::from_secs(3), command_output(p)).await.unwrap();
        assert!(ok && text.ends_with("TAIL_OKSTDERR_OK"));
        assert!(text.len() <= 8192);
    }
}
