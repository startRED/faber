//! The small programs the fake helper (`fake-cli`) can be besides a CLI, all without a shell:
//!
//! - `--gate [--desktop NAME] PROGRAM ARGS...`: waits for the runner to put it in its job object (the runner writes
//!   `go`), then starts PROGRAM, which therefore starts inside the job, and writes `pid N`. On Windows PROGRAM starts on
//!   the hidden desktop NAME (see `desktop`), and without one nothing is started. It keeps PROGRAM's handle until it ends, so that
//!   id cannot belong to anything else. It ends PROGRAM when told `kill` or when the runner is gone, and ends with its
//!   status. See `contain`.
//! - `--family`: starts two `--idle` copies of itself, then idles. A program with descendants, for the containment check.
//! - `--idle`: does nothing for a minute. A program that belongs to no one.
//! - `--desktop-probe` (Windows): writes the windows its own desktop has to its error stream, and ends. It makes no
//!   window itself. See `desktop::probe`.

use std::io::{self, BufRead, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Runs a helper mode if `args` names one; its result is the exit code. `None` when it is something else.
pub fn run(args: &[String]) -> Option<i32> {
    match args.first().map(String::as_str) {
        Some("--gate") => Some(gate(&args[1..])),
        Some("--family") => Some(family()),
        Some("--idle") => Some(idle()),
        Some("--tree-cli") => Some(tree_cli()),
        Some("--tree-child") => Some(tree_child()),
        Some("--engine-tree-host") => Some(engine_tree_host(&args[1..])),
        Some("--guard-proof-host") => Some(guard_proof_host(&args[1..], true)),
        Some("--cache-proof-host") => Some(guard_proof_host(&args[1..], false)),
        Some("--eof") => {
            use io::Read;
            let mut input = String::new();
            if io::stdin().read_to_string(&mut input).is_err() {
                return Some(1);
            }
            println!("eof");
            Some(0)
        }
        Some("--held-guard") | Some("--echo") => {
            let mut line = String::new();
            if io::stdin().read_line(&mut line).is_err() {
                return Some(1);
            }
            print!("{line}");
            Some(0)
        }
        #[cfg(windows)]
        Some("--desktop-probe") => Some(crate::desktop::probe(false)),
        #[cfg(windows)]
        Some("--desktop-visible-probe") => Some(crate::desktop::probe(true)),
        _ => None,
    }
}

fn guard_proof_host(args: &[String], spawn_probe: bool) -> i32 {
    let [ready, release, example] = args else { return 1 };
    let result: io::Result<()> = (|| {
        use io::{Read, Write};
        let binary = std::env::current_exe()?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        if spawn_probe {
            runtime.block_on(async {
                use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                let mut p = faber_engine::proc::real_spawner()(
                    binary.to_str().ok_or_else(|| io::Error::other("path"))?,
                    &["--held-guard".into(), release.into()],
                    &std::env::current_dir()?,
                )?;
                // This file must exist before the child can acknowledge; a synchronous ack wait deadlocks this proof.
                std::fs::write(release, b"spawn returned")?;
                p.stdin.write_all(b"echo\n").await?;
                let mut line = String::new();
                BufReader::new(p.stdout).read_line(&mut line).await?;
                if line != "echo\n" {
                    return Err(io::Error::other("guard consumed input"));
                }
                let p = faber_engine::proc::real_spawner()(
                    binary.to_str().ok_or_else(|| io::Error::other("path"))?,
                    &["--eof".into()],
                    &std::env::current_dir()?,
                )?;
                drop(p.stdin);
                let mut line = String::new();
                tokio::time::timeout(Duration::from_secs(2), BufReader::new(p.stdout).read_line(&mut line))
                    .await
                    .map_err(|_| io::Error::other("native stdin did not close"))??;
                if line != "eof\n" {
                    return Err(io::Error::other("native EOF was not delivered"));
                }
                Ok::<_, io::Error>(())
            })?;
        }
        let mut cmd = Command::new(example);
        cmd.args(["--faber-process-guard", binary.to_str().ok_or_else(|| io::Error::other("path"))?, "--echo"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(crate::contain::NO_WINDOW);
        }
        let mut child = cmd.spawn()?;
        let mut input = child.stdin.take().expect("piped");
        let mut output = child.stdout.take().expect("piped");
        input.write_all(b"G")?;
        input.flush()?;
        let mut ack = [0];
        output.read_exact(&mut ack)?;
        if ack != *b"R" {
            let _ = child.kill();
            return Err(io::Error::other("example omitted guard dispatch"));
        }
        input.write_all(b"echo\n")?;
        input.flush()?;
        let mut line = String::new();
        io::BufReader::new(output).read_line(&mut line)?;
        if line != "echo\n" || !child.wait()?.success() {
            return Err(io::Error::other("example guard failed"));
        }
        std::fs::write(ready, b"guard and example passed")?;
        Ok(())
    })();
    if result.is_ok() { 0 } else { 1 }
}

fn spawn_fake(mode: &str) -> io::Result<std::process::Child> {
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg(mode).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::contain::NO_WINDOW);
    }
    cmd.spawn()
}

fn tree_cli() -> i32 {
    let Ok(_child) = spawn_fake("--tree-child") else { return 1 };
    println!("ready");
    let _ = io::stdout().flush();
    idle()
}

fn tree_child() -> i32 {
    let Ok(_grandchild) = spawn_fake("--idle") else { return 1 };
    idle()
}

fn engine_tree_host(args: &[String]) -> i32 {
    let [ready, control] = args else { return 1 };
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(_) => return 1,
    };
    let result: io::Result<()> = runtime.block_on(async {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let binary = std::env::current_exe()?.to_string_lossy().into_owned();
        let cwd = std::env::current_dir()?;
        let spawn = faber_engine::proc::real_spawner();
        let mut p = spawn(&binary, &["--tree-cli".into()], &cwd)?;
        let root = p.child.as_ref().and_then(|p| p.id()).ok_or_else(|| io::Error::other("no root"))?;
        let mut line = String::new();
        BufReader::new(p.stdout).read_line(&mut line).await?;
        std::fs::write(ready, serde_json::json!({"root":root}).to_string())?;
        let action = wait_control(control, "action").await?;
        // As the engine does: dropping the session drops the job and the child; stopping ends the job, then the child.
        drop(p.contained.take());
        if action == "drop" {
            drop(p.child.take());
        } else if let Some(child) = p.child.as_mut() {
            child.start_kill()?;
            child.wait().await?;
        }
        if action == "stop" {
            let peer = spawn(&binary, &["--idle".into()], &cwd)?;
            let peer_id = peer.child.as_ref().and_then(|p| p.id()).unwrap_or(0);
            std::fs::write(ready, serde_json::json!({"root":root,"peer":peer_id}).to_string())?;
            wait_control(control, "stop-again").await?;
            if let Some(child) = p.child.as_mut() {
                child.start_kill()?;
            }
            std::fs::write(ready, serde_json::json!({"root":root,"peer":peer_id,"stoppedAgain":true}).to_string())?;
            tokio::time::sleep(Duration::from_secs(30)).await;
            drop(peer);
        } else {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
        Ok(())
    });
    if result.is_ok() { 0 } else { 1 }
}

async fn wait_control(path: &str, wanted: &str) -> io::Result<String> {
    for _ in 0..400 {
        if let Some(action) = std::fs::read(path)
            .ok()
            .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok())
            .and_then(|v| v["action"].as_str().map(String::from))
            && (wanted == "action" || action == wanted)
        {
            return Ok(action);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    Err(io::Error::other("control timeout"))
}

fn idle() -> i32 {
    std::thread::sleep(Duration::from_secs(60));
    0
}

fn family() -> i32 {
    let me = match std::env::current_exe() {
        Ok(me) => me,
        Err(_) => return 1,
    };
    let mut children = Vec::new();
    for _ in 0..2 {
        match Command::new(&me).arg("--idle").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            Ok(child) => children.push(child),
            Err(_) => return 1,
        }
    }
    idle()
}

/// The program the gate started: a child process, or on Windows a process on the hidden desktop.
struct App {
    #[cfg(windows)]
    inner: crate::desktop::Launched,
    #[cfg(not(windows))]
    inner: std::process::Child,
}

impl App {
    fn start(args: &[String]) -> Result<App, String> {
        #[cfg(windows)]
        {
            let [flag, desktop, program, rest @ ..] = args else {
                return Err("no hidden desktop was given, so the app is not started".into());
            };
            if flag != "--desktop" {
                return Err("no hidden desktop was given, so the app is not started".into());
            }
            Ok(App { inner: crate::desktop::launch(desktop, program, rest)? })
        }
        #[cfg(not(windows))]
        {
            let Some((program, rest)) = args.split_first() else { return Err("no program".into()) };
            let child = Command::new(program).args(rest).stdin(Stdio::null()).stdout(Stdio::null()).spawn().map_err(|e| e.to_string())?;
            Ok(App { inner: child })
        }
    }

    fn id(&self) -> u32 {
        #[cfg(windows)]
        return self.inner.pid;
        #[cfg(not(windows))]
        return self.inner.id();
    }

    fn kill(&mut self) {
        #[cfg(windows)]
        self.inner.kill();
        #[cfg(not(windows))]
        let _ = self.inner.kill();
    }

    /// The exit code once it has ended.
    fn exit(&mut self) -> Option<i32> {
        #[cfg(windows)]
        return self.inner.try_wait().map(|code| code as i32);
        #[cfg(not(windows))]
        return match self.inner.try_wait() {
            Ok(Some(status)) => Some(status.code().unwrap_or(1)),
            Ok(None) => None,
            Err(_) => Some(1),
        };
    }
}

fn gate(args: &[String]) -> i32 {
    let input = io::stdin();
    let mut line = String::new();
    if input.lock().read_line(&mut line).unwrap_or(0) == 0 || line.trim() != "go" {
        return 1;
    }
    let mut app = match App::start(args) {
        Ok(app) => app,
        Err(e) => {
            println!("error {e}");
            return 1;
        }
    };
    println!("pid {}", app.id());
    let _ = io::stdout().flush();
    // `kill`, or the runner going away (its end of the pipe closes), ends the program.
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            if input.lock().read_line(&mut line).unwrap_or(0) == 0 || line.trim() == "kill" {
                flag.store(true, Ordering::SeqCst);
                return;
            }
        }
    });
    loop {
        if stop.load(Ordering::SeqCst) {
            app.kill();
        }
        match app.exit() {
            Some(code) => return code,
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// Fake-host admission delay only; the production guard remains in the engine.
pub fn guard_mode() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--faber-process-guard")
        && let Some(i) = args.iter().position(|a| a == "--held-guard")
    {
        for _ in 0..200 {
            if std::path::Path::new(&args[i + 1]).exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
    faber_engine::proc::guard_mode()
}
