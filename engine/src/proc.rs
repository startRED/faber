//! The slice of a child process a driver needs, so tests can replay a recording without spawning anything.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::time::{Duration, Instant};

/// Silence starts a graceful interrupt; output can never extend the interrupt's fixed grace period.
#[derive(Default)]
pub(crate) struct TurnWatch {
    silent: Option<Instant>,
    interrupt: Option<Instant>,
    awaiting_permission: bool,
}

impl TurnWatch {
    pub fn begin(&mut self) {
        self.silent = Some(Instant::now() + Duration::from_secs(120));
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
        self.silent.is_some() || self.interrupt.is_some()
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
}

pub type Spawner = Arc<dyn Fn(&str, &[String], &Path) -> io::Result<Proc> + Send + Sync>;

/// Starts a real process with piped stdio, no shell and, on Windows, no console window.
pub fn real_spawner() -> Spawner {
    Arc::new(|bin, args, cwd| {
        let mut cmd = Command::new(bin);
        cmd.args(args).current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn()?;
        Ok(Proc {
            stdin: Box::new(child.stdin.take().expect("piped stdin")),
            stdout: Box::new(child.stdout.take().expect("piped stdout")),
            stderr: child.stderr.take().map(|s| Box::new(s) as Box<dyn AsyncRead + Send + Unpin>),
            child: Some(child),
        })
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

/// Ends only the held child, never a looked-up PID. This is not proof that descendants died (Projects stays blocked).
pub(crate) async fn stop(stdin: &mut (dyn AsyncWrite + Send + Unpin), child: &mut Option<Child>) {
    let _ = tokio::time::timeout(Duration::from_secs(1), stdin.shutdown()).await;
    if let Some(child) = child {
        let _ = child.start_kill();
    }
}

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
        watch.begin();
        watch.pause();
        watch.activity();
        assert!(watch.active());
        assert_eq!(watch.deadline(), None);
        watch.interrupt();
        assert_eq!(watch.deadline(), Some(Instant::now() + Duration::from_secs(5)));
        watch.finish();
        watch.begin();
        watch.pause();
        watch.resume();
        assert_eq!(watch.deadline(), Some(Instant::now() + Duration::from_secs(120)));
    }

    #[tokio::test(start_paused = true)]
    async fn output_and_repeated_interrupts_cannot_extend_the_stop_grace() {
        let mut watch = TurnWatch::default();
        assert!(!watch.active());
        watch.begin();
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
