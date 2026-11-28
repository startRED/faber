//! The fair benchmark's meter (R113, `docs/superpowers/specs/2026-10-14-test-system-rework-design.md` section 3): runs one
//! command inside a fresh Windows job object (started suspended, joined, then resumed, so no child escapes it) and reads
//! the job's own accounting once every process in it has ended: wall time, processes started, processor time, peak
//! committed memory and bytes written. It appends one JSON line to `--out`. The same rules hold for every side, so a run
//! cannot drift: it waits for an idle PC (no input for `--idle` seconds, no full-screen app, no other Cargo build) and
//! starts the command below normal priority with no window and no shell. How to run it: `docs/dev/testing.md`.
//!
//!     faber-bench --label <name> --dir <folder> --out <file.jsonl> [--idle 60] [--unfair] -- <program> [args...]
//!
//! `--unfair` starts at once, whatever the PC does, and marks the line `"fair": false`: for checking the meter itself.

use std::path::PathBuf;

#[derive(Debug, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
struct Args {
    label: String,
    dir: PathBuf,
    out: PathBuf,
    idle: u64,
    /// Start without waiting for an idle PC; the line says so.
    unfair: bool,
    program: String,
    args: Vec<String>,
}

fn parse(mut raw: impl Iterator<Item = String>) -> Result<Args, String> {
    let (mut label, mut dir, mut out, mut idle, mut unfair) = (None, None, None, 60, false);
    loop {
        match raw.next().as_deref() {
            Some("--label") => label = raw.next(),
            Some("--dir") => dir = raw.next().map(PathBuf::from),
            Some("--out") => out = raw.next().map(PathBuf::from),
            Some("--idle") => idle = raw.next().and_then(|s| s.parse().ok()).ok_or("--idle takes seconds")?,
            Some("--unfair") => unfair = true,
            Some("--") => break,
            Some(other) => return Err(format!("unknown argument {other}")),
            None => return Err("the command goes after --".into()),
        }
    }
    let program = raw.next().ok_or("the command goes after --")?;
    fn need<T>(v: Option<T>, name: &str) -> Result<T, String> {
        v.ok_or_else(|| format!("{name} is required"))
    }
    Ok(Args {
        label: need(label, "--label")?,
        dir: need(dir, "--dir")?,
        out: need(out, "--out")?,
        idle,
        unfair,
        program,
        args: raw.collect(),
    })
}

/// What one run measured (only Windows measures).
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, PartialEq)]
struct Measured {
    exit: Option<i32>,
    /// Processes still running in the job `LEFT` after the command ended, then ended by the meter.
    leaked: u32,
    wall_s: f64,
    processes: u32,
    cpu_s: f64,
    peak_commit_mb: f64,
    written_mb: f64,
}

/// The JSON line of one run, with what it needs to be compared later: the folder's commit, the core budget and the time.
#[cfg_attr(not(windows), allow(dead_code))]
fn line(args: &Args, m: &Measured, commit: &str, jobs: Option<&str>, at: u64) -> String {
    let round = |x: f64| (x * 10.0).round() / 10.0;
    serde_json::json!({
        "label": args.label,
        "command": std::iter::once(args.program.as_str()).chain(args.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" "),
        "exit": m.exit,
        "leaked": m.leaked,
        "wall_s": round(m.wall_s),
        "procs_started": m.processes,
        "cpu_s": round(m.cpu_s),
        "peak_commit_mb": round(m.peak_commit_mb),
        "written_mb": round(m.written_mb),
        "commit": commit,
        "jobs": jobs,
        "fair": !args.unfair,
        "at": at,
    })
    .to_string()
}

fn main() {
    let args = match parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(error) => {
            eprintln!(
                "faber-bench: {error}\nusage: faber-bench --label <name> --dir <folder> --out <file.jsonl> [--idle 60] [--unfair] -- <program> [args...]"
            );
            std::process::exit(2);
        }
    };
    #[cfg(windows)]
    match windows::run(&args) {
        Ok(m) => {
            let commit = windows::commit(&args.dir);
            let jobs = std::env::var("CARGO_BUILD_JOBS").ok();
            let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let text = line(&args, &m, &commit, jobs.as_deref(), at);
            use std::io::Write;
            let written = std::fs::OpenOptions::new().create(true).append(true).open(&args.out).and_then(|mut f| writeln!(f, "{text}"));
            if let Err(error) = written {
                eprintln!("faber-bench: could not write {}: {error}", args.out.display());
                std::process::exit(1);
            }
            println!("{text}");
        }
        Err(error) => {
            eprintln!("faber-bench: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        eprintln!("faber-bench: the meter reads Windows job accounting and runs only on Windows");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod windows {
    use super::{Args, Measured};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
    /// How long what the command left running may go on before the meter ends it and says how many there were.
    const LEFT: Duration = Duration::from_secs(30);
    /// Programs whose run outside the job means another session builds: the numbers would carry its load.
    const BUSY: [&str; 3] = ["cargo.exe", "rustc.exe", "link.exe"];

    /// Seconds since the last keyboard or mouse input; none known counts as input now.
    fn idle_seconds() -> u64 {
        let Ok(info) = winsafe::GetLastInputInfo() else { return 0 };
        let last = u64::from(info.dwTime);
        // The input time is the low 32 bits of the tick count.
        let now = winsafe::GetTickCount64();
        ((now & 0xFFFF_FFFF).wrapping_sub(last) & 0xFFFF_FFFF) / 1000
    }

    fn busy() -> Option<String> {
        let list = faber_winproc::processes().ok()?;
        list.iter().find(|p| BUSY.iter().any(|b| p.name.eq_ignore_ascii_case(b))).map(|p| p.name.clone())
    }

    /// Waits until the PC is idle for `seconds`, no full-screen app is in front and no other build runs, saying once why.
    fn wait_for_idle(seconds: u64) {
        let mut said = String::new();
        loop {
            let why = if faber_winproc::full_screen_in_front() {
                Some("a full-screen app is in front".to_string())
            } else if let Some(name) = busy() {
                Some(format!("another build runs ({name})"))
            } else if idle_seconds() < seconds {
                Some(format!("waiting for {seconds} s with no input"))
            } else {
                None
            };
            let Some(why) = why else { return };
            if why != said {
                eprintln!("faber-bench: {why}");
                said = why;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    }

    pub fn run(args: &Args) -> Result<Measured, String> {
        if !args.unfair {
            wait_for_idle(args.idle);
        }
        let began = Instant::now();
        let mut child = Command::new(&args.program)
            .args(&args.args)
            .current_dir(&args.dir)
            .stdin(Stdio::null())
            // Its output is thrown away: written to a log, it would count as the command's own writes.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS)
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", args.program))?;
        let job = join(&child).inspect_err(|_| {
            let _ = child.kill();
        })?;
        faber_winproc::resume_process(child.id()).map_err(|e| e.to_string())?;
        let exit = child.wait().map_err(|e| e.to_string())?.code();
        // What the command left running still counts: the run ends when its whole tree has, or after `LEFT`.
        let ended = Instant::now();
        let active = || faber_winproc::job_active_processes(job.handle()).map_err(|e| format!("the job's processes: {e}"));
        let mut left = active()?;
        while left > 0 && ended.elapsed() < LEFT {
            std::thread::sleep(Duration::from_millis(100));
            left = active()?;
        }
        let wall_s = began.elapsed().as_secs_f64();
        if left > 0 {
            // Best effort: the job also ends them when it closes (kill on close).
            let _ = faber_winproc::job_terminate(job.handle());
        }
        let used = faber_winproc::job_accounting(job.handle()).map_err(|e| e.to_string())?;
        let peak = faber_winproc::job_peak_memory(job.handle()).map_err(|e| e.to_string())?;
        let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
        Ok(Measured {
            exit,
            leaked: left,
            wall_s,
            processes: used.processes,
            cpu_s: used.cpu_seconds,
            peak_commit_mb: mb(peak),
            written_mb: mb(used.written_bytes),
        })
    }

    fn join(child: &std::process::Child) -> Result<win32job::Job, String> {
        use std::os::windows::io::AsRawHandle;
        let mut limits = win32job::ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let job = win32job::Job::create_with_limit_info(&limits).map_err(|e| e.to_string())?;
        job.assign_process(child.as_raw_handle() as isize).map_err(|e| e.to_string())?;
        Ok(job)
    }

    /// The folder's commit, or "unknown" outside git (git run directly, no shell, no window).
    pub fn commit(dir: &std::path::Path) -> String {
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Args, String> {
        parse(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn the_command_comes_after_the_dashes_and_every_place_is_required() {
        let parsed = args(&["--label", "faber", "--dir", "f", "--out", "o.jsonl", "--", "cargo", "t", "--all"]).unwrap();
        assert_eq!((parsed.program.as_str(), parsed.args.as_slice(), parsed.idle), ("cargo", &["t".to_string(), "--all".into()][..], 60));
        assert_eq!(args(&["--label", "x", "--dir", "f", "--out", "o", "--idle", "0", "--", "node"]).unwrap().idle, 0);
        assert!(args(&["--label", "x", "--dir", "f", "--out", "o", "--unfair", "--", "node"]).unwrap().unfair);
        assert!(!parsed.unfair);
        assert!(args(&["--label", "x", "--dir", "f", "--", "node"]).unwrap_err().contains("--out"));
        assert!(args(&["--label", "x", "--dir", "f", "--out", "o"]).is_err(), "no command");
        assert!(args(&["--what"]).is_err());
    }

    #[test]
    fn a_run_is_one_json_line_with_its_commit_and_core_budget() {
        let a = args(&["--label", "tl-ade suite", "--dir", "f", "--out", "o", "--", "node", "vitest.mjs", "run"]).unwrap();
        let m = Measured { exit: Some(0), leaked: 0, wall_s: 12.345, processes: 3, cpu_s: 1.06, peak_commit_mb: 150.04, written_mb: 0.0 };
        let text = line(&a, &m, "abc", Some("8"), 7);
        assert!(!text.contains('\n'));
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["command"], "node vitest.mjs run");
        assert_eq!((v["wall_s"].as_f64(), v["cpu_s"].as_f64(), v["procs_started"].as_u64()), (Some(12.3), Some(1.1), Some(3)));
        assert_eq!((v["commit"].as_str(), v["jobs"].as_str(), v["at"].as_u64()), (Some("abc"), Some("8"), Some(7)));
        assert_eq!(v["fair"], true);
    }
}
