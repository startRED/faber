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
        #[cfg(windows)]
        Some("--desktop-probe") => Some(crate::desktop::probe()),
        _ => None,
    }
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
