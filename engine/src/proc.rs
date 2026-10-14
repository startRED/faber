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

/// Ends a process: closes its input, then kills it and everything it started.
pub(crate) async fn stop(stdin: &mut (dyn AsyncWrite + Send + Unpin), child: &mut Option<Child>) {
    let _ = stdin.shutdown().await;
    if let Some(child) = child {
        kill_tree(child.id()).await;
        let _ = child.start_kill();
    }
}

/// Killing only the CLI leaves a command it started (a build, a script) running, and in full access that command can keep
/// changing files after the person switched to asking. On Windows `taskkill /T` ends the whole tree while its root is still
/// alive (the tree is found through the parent links, which a dead root no longer has). It is run directly, never through a
/// shell. Elsewhere only the CLI itself is ended.
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
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), cmd.status()).await;
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
        };
        let (ok, text) = tokio::time::timeout(std::time::Duration::from_secs(3), command_output(p)).await.unwrap();
        assert!(ok && text.ends_with("TAIL_OKSTDERR_OK"));
        assert!(text.len() <= 8192);
    }
}
