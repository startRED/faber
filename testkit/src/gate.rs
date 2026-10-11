//! The small programs the fake helper (`fake-cli`) can be besides a CLI, all without a shell:
//!
//! - `--gate PROGRAM ARGS...`: waits for the runner to put it in its job object (the runner writes `go`), then starts
//!   PROGRAM, which therefore starts inside the job, and writes `pid N`. It keeps PROGRAM's handle until it ends, so that
//!   id cannot belong to anything else. It ends PROGRAM when told `kill` or when the runner is gone, and ends with its
//!   status. See `contain`.
//! - `--family`: starts two `--idle` copies of itself, then idles. A program with descendants, for the containment check.
//! - `--idle`: does nothing for a minute. A program that belongs to no one.

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

fn gate(args: &[String]) -> i32 {
    let Some((program, rest)) = args.split_first() else { return 2 };
    let input = io::stdin();
    let mut line = String::new();
    if input.lock().read_line(&mut line).unwrap_or(0) == 0 || line.trim() != "go" {
        return 1;
    }
    let mut command = Command::new(program);
    command.args(rest).stdin(Stdio::null()).stdout(Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut command, crate::contain::NO_WINDOW);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            println!("error {e}");
            return 1;
        }
    };
    println!("pid {}", child.id());
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
            let _ = child.kill();
        }
        match child.try_wait() {
            Ok(Some(status)) => return status.code().unwrap_or(1),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return 1,
        }
    }
}
