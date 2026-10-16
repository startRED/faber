//! `faber-test`: reads what changed, checks the lane's claim, picks the suites and runs them. The screen gets the
//! failures, the measurements and a summary; every line goes to the run's log (R154). Cargo's output is read by the same
//! functions that read a mission's checks (`engine/src/project/proof/output.rs`, R155).

use crate::app::{Paths, emergency_cleanup};
use crate::base;
use crate::checks;
use crate::lanes;
use crate::plan::{Options, Suite, changed_paths, jobs, parse_args, select};
use crate::report::{Outcome, Status, exit_code, line, quiet, summary};
use crate::screen;
use crate::slots;
use crate::sweep;
use crate::testbuild::{build_test_app, cargo_command, ensure_fake_cli, prebuilt};
use faber_engine::project::proof::output::{cargo_output, died, failure_sections, parse_cargo_tests};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

const USAGE: &str = "usage: cargo t [--all] [--suite rust|ui|screen|lint] [--filter <text>] [--shard <i/n>] [--build-only | --prebuilt]";

/// The two Clippy proofs, as CI runs them (a test keeps them equal). `--suite lint` runs them with [`lint_args`].
pub const LINTS: [&[&str]; 2] = [
    &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    &["clippy", "-p", "faber", "--features", "test-hooks", "--", "-D", "warnings"],
];

/// The `cargo test` command line of the `rust` suite: every test program runs even after one fails, compiler messages
/// come as JSON, and `-j` and `--test-threads` are the same number.
pub fn cargo_test_args(jobs: usize, filter: Option<&str>) -> Vec<String> {
    let mut args = ["test", "--workspace", "--no-fail-fast", "--message-format=json", "-j"].map(String::from).to_vec();
    args.push(jobs.to_string());
    args.extend(filter.map(str::to_string));
    args.extend(["--".to_string(), "--test-threads".into(), jobs.to_string()]);
    args
}

/// A Clippy proof with its compiler messages as JSON and `jobs` cores; what follows `--` stays as it is.
pub fn lint_args(proof: &[&str], jobs: usize) -> Vec<String> {
    let split = proof.iter().position(|a| *a == "--").unwrap_or(proof.len());
    let mut args: Vec<String> = proof[..split].iter().map(|a| a.to_string()).collect();
    args.extend(["--message-format=json".to_string(), "-j".into(), jobs.to_string()]);
    args.extend(proof[split..].iter().map(|a| a.to_string()));
    args
}

/// Why a Cargo command failed when no test said so: its compiler errors, or the end of what it printed.
fn reason(errors: &[String], stderr: &str) -> String {
    if !errors.is_empty() {
        return errors.join("\n");
    }
    let lines: Vec<&str> = stderr.lines().collect();
    lines[lines.len().saturating_sub(20)..].join("\n")
}

fn cargo(root: &Path, args: &[String]) -> std::io::Result<Output> {
    cargo_command().current_dir(root).args(args).output()
}

fn run_rust(root: &Path, jobs: usize, filter: Option<&str>, log: &mut Log) -> Vec<Outcome> {
    let started = Instant::now();
    let output = match cargo(root, &cargo_test_args(jobs, filter)) {
        Ok(output) => output,
        Err(e) => return vec![Outcome::fail(Suite::Rust, "cargo test --workspace", None, &format!("cannot start cargo: {e}"))],
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    let (errors, text) = cargo_output(&String::from_utf8_lossy(&output.stdout), &stderr);
    log.write(&text);
    let failures = failure_sections(&text);
    let mut outcomes: Vec<Outcome> = parse_cargo_tests(&text)
        .into_iter()
        .map(|(name, ok)| {
            if ok {
                Outcome::pass(Suite::Rust, &name, None)
            } else {
                Outcome::fail(Suite::Rust, &name, None, failures.get(&name).map_or("", String::as_str))
            }
        })
        .collect();
    if !output.status.success() {
        // A failed test explains itself; a failed build, or a test program that died, needs its own line.
        let crashed = died(&stderr);
        if !outcomes.iter().any(|o| o.status == Status::Fail) {
            outcomes.push(Outcome::fail(Suite::Rust, "cargo test --workspace", Some(started.elapsed()), &reason(&errors, &stderr)));
        } else if !errors.is_empty() || !crashed.is_empty() {
            let shown: Vec<&str> = errors.iter().map(String::as_str).chain(crashed).collect();
            outcomes.push(Outcome::fail(Suite::Rust, "cargo test --workspace", Some(started.elapsed()), &shown.join("\n")));
        }
    } else if outcomes.is_empty() {
        // Nothing ran. With a filter that is the caller's mistake to hear about (see `nothing_matched`).
        if filter.is_none() {
            outcomes.push(Outcome::fail(Suite::Rust, "cargo test --workspace", Some(started.elapsed()), "cargo ran no test at all"));
        }
    } else {
        outcomes.push(Outcome::pass(Suite::Rust, &format!("cargo test --workspace ({} tests)", outcomes.len()), Some(started.elapsed())));
    }
    outcomes
}

fn run_lint(root: &Path, jobs: usize, log: &mut Log) -> Vec<Outcome> {
    LINTS
        .iter()
        .map(|proof| {
            let (started, name) = (Instant::now(), format!("cargo {}", proof.join(" ")));
            let output = match cargo(root, &lint_args(proof, jobs)) {
                Ok(output) => output,
                Err(e) => return Outcome::fail(Suite::Lint, &name, None, &format!("cannot start cargo: {e}")),
            };
            let stderr = String::from_utf8_lossy(&output.stderr);
            let (errors, text) = cargo_output(&String::from_utf8_lossy(&output.stdout), &stderr);
            log.write(&text);
            if output.status.success() {
                Outcome::pass(Suite::Lint, &name, Some(started.elapsed()))
            } else {
                Outcome::fail(Suite::Lint, &name, Some(started.elapsed()), &reason(&errors, &stderr))
            }
        })
        .collect()
}

/// Every line of a run, whole, in `target/faber-test/run.log`. The screen gets only what [`quiet`] keeps.
struct Log {
    path: PathBuf,
    file: Option<File>,
}

impl Log {
    fn create(path: PathBuf) -> Log {
        let file = path.parent().and_then(|dir| std::fs::create_dir_all(dir).ok()).and_then(|()| File::create(&path).ok());
        Log { path, file }
    }

    fn write(&mut self, text: &str) {
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{}", text.trim_end());
        }
    }

    fn show(&mut self, outcomes: &[Outcome]) {
        for outcome in outcomes {
            self.write(&line(outcome));
            if let Some(text) = quiet(outcome) {
                println!("{text}");
            }
        }
    }
}

/// What a git command printed, when it ran and succeeded.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git").current_dir(root).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Keeps the main checkout's `target` under the cap (R106), at most once an hour. Prints a line only when it freed space.
/// Returns that `target`, which every worktree shares.
fn sweep_builds(paths: &Paths, porcelain: Option<&str>) -> Option<PathBuf> {
    let worktrees = sweep::worktrees(porcelain?);
    let (Some(main), Some(own)) = (worktrees.first(), paths.target.parent()) else { return None };
    let target = main.join("target");
    let now = std::time::SystemTime::now();
    if !target.is_dir() {
        return None;
    }
    if !sweep::due(&target, now) {
        return Some(target);
    }
    let report = sweep::sweep(&target, own, &worktrees, now, sweep::CAP);
    let gb = |bytes: u64| bytes as f64 / (1u64 << 30) as f64;
    if report.after < report.before {
        let removed = if report.removed.is_empty() { String::new() } else { format!(", removed {}", report.removed.join(", ")) };
        println!("Build folder: {:.1} GB -> {:.1} GB{removed}", gb(report.before), gb(report.after));
    }
    if report.after > sweep::CAP {
        println!("Build folder: {:.1} GB is above the {:.0} GB cap, and nothing else is idle to remove", gb(report.after), gb(sweep::CAP));
    }
    Some(target)
}

fn changed(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["-c", "core.quotepath=false", "status", "--porcelain", "-uall"])
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(format!("git status failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(changed_paths(&String::from_utf8_lossy(&output.stdout)))
}

/// With `--filter`, a run in which no suite had a test of that name is a mistake, never a pass.
pub fn nothing_matched(outcomes: &[Outcome], filter: Option<&str>) -> bool {
    filter.is_some() && outcomes.is_empty()
}

/// Runs the runner. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let opts: Options = match parse_args(args) {
        Ok(opts) => opts,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let paths = match Paths::discover() {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let worktrees = git(&paths.root, &["worktree", "list", "--porcelain"]);
    let shared = sweep_builds(&paths, worktrees.as_deref());
    let changed = match changed(&paths.root) {
        Ok(list) => list,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let claim = worktrees.as_deref().map(|list| lanes::check(&paths.root, list, &changed)).unwrap_or_default();
    if !claim.is_empty() {
        for problem in claim {
            println!("FAIL  lane    {problem}");
        }
        return 1;
    }
    let jobs = jobs();
    if opts.build_only {
        return match ensure_fake_cli(&paths, jobs).and_then(|()| build_test_app(&paths, jobs)) {
            Ok(()) => {
                println!("Built the test app and its helpers.");
                0
            }
            Err(message) => {
                println!("FAIL  build   the test app\n{message}");
                1
            }
        };
    }
    // A red phase-1-base shows before anything runs: commits pushed onto it only wait behind the failure.
    base::warn(&paths.root);
    let suites = select(&opts, &changed);
    if suites.is_empty() {
        println!("Nothing changed that needs a test (use --all to run everything).");
        return 0;
    }
    let _slot = shared.and_then(|target| slots::take(&target.join(".faber-slots")));
    // Ctrl+C ends the apps this run started and removes its temporary folders before the runner goes.
    let _ = ctrlc::set_handler(|| {
        emergency_cleanup();
        std::process::exit(130);
    });
    let mut log = Log::create(paths.target.join("run.log"));
    let filter = opts.filter.as_deref();
    let mut outcomes: Vec<Outcome> = Vec::new();
    if suites.contains(&Suite::Lint) {
        let found = run_lint(&paths.root, jobs, &mut log);
        log.show(&found);
        outcomes.extend(found);
    }
    if suites.contains(&Suite::Rust) {
        let found = run_rust(&paths.root, jobs, filter, &mut log);
        log.show(&found);
        outcomes.extend(found);
    }
    let (wants_ui, wants_screen) = (suites.contains(&Suite::Ui), suites.contains(&Suite::Screen));
    // Without WebView2's DevTools port, `screen` is skipped, and the app is only built if `ui` needs it.
    let screen_runs = wants_screen && cfg!(windows);
    if wants_ui || wants_screen {
        if wants_screen && !screen_runs {
            let skipped = Outcome::skipped(Suite::Screen, "screen", "needs WebView2's DevTools port, which only Windows has");
            log.show(std::slice::from_ref(&skipped));
            outcomes.push(skipped);
        }
        if wants_ui || screen_runs {
            let built =
                if opts.prebuilt { prebuilt(&paths) } else { ensure_fake_cli(&paths, jobs).and_then(|()| build_test_app(&paths, jobs)) };
            match built {
                Err(message) => {
                    for suite in [Suite::Ui, Suite::Screen].into_iter().filter(|s| suites.contains(s) && (*s == Suite::Ui || screen_runs)) {
                        let not_run = Outcome::not_run(suite, "the test app", &message);
                        log.show(std::slice::from_ref(&not_run));
                        outcomes.push(not_run);
                    }
                }
                Ok(()) => {
                    if wants_ui {
                        let found = checks::run(&paths, filter);
                        log.show(&found);
                        outcomes.extend(found);
                    }
                    if screen_runs {
                        // Each scenario is logged as it ends, and a failure shows at once: the suite takes minutes.
                        outcomes
                            .extend(screen::run_all(&paths, filter, opts.shard, &mut |outcome| log.show(std::slice::from_ref(outcome))));
                    }
                }
            }
        }
    }
    if nothing_matched(&outcomes, filter) {
        println!("No test name contains {:?}. Nothing ran.", filter.unwrap_or_default());
        return 1;
    }
    let total = summary(&outcomes);
    log.write(&total);
    let every = if log.file.is_some() { format!(". Every line: {}", log.path.display()) } else { String::new() };
    println!("\n{total}{every}");
    if outcomes.iter().any(|o| o.status == Status::Fail && matches!(o.suite, Suite::Ui | Suite::Screen)) {
        println!("Screenshots of failures are in {}", paths.shots.display());
    }
    exit_code(&outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_that_matched_no_test_anywhere_is_a_mistake_but_no_filter_and_no_tests_is_not_this_check() {
        let one = [Outcome::pass(Suite::Ui, "i18n: keys", None)];
        assert!(nothing_matched(&[], Some("zzz")));
        assert!(!nothing_matched(&one, Some("i18n")));
        assert!(!nothing_matched(&[], None), "without a filter an empty run is another kind of problem");
    }

    #[test]
    fn cargo_test_runs_every_program_with_json_messages_and_the_same_number_for_jobs_and_test_threads() {
        let start = ["test", "--workspace", "--no-fail-fast", "--message-format=json", "-j"];
        assert_eq!(cargo_test_args(3, None), [&start[..], &["3", "--", "--test-threads", "3"]].concat());
        assert_eq!(cargo_test_args(1, Some("fake")), [&start[..], &["1", "fake", "--", "--test-threads", "1"]].concat());
    }

    #[test]
    fn the_lint_suite_runs_the_clippy_lines_ci_runs_with_json_messages() {
        let ci = include_str!("../../.github/workflows/ci.yml");
        for proof in LINTS {
            assert!(ci.contains(&format!("cargo {}", proof.join(" "))), "CI no longer runs {proof:?}");
        }
        assert_eq!(
            lint_args(LINTS[1], 4),
            ["clippy", "-p", "faber", "--features", "test-hooks", "--message-format=json", "-j", "4", "--", "-D", "warnings"]
        );
    }

    #[test]
    fn a_failed_cargo_command_is_explained_by_its_compiler_errors_or_else_its_last_twenty_lines() {
        let errors = ["engine/src/nudge.rs:126:23: error[E0308]: mismatched types".to_string()];
        assert_eq!(reason(&errors, "anything"), errors[0]);
        let long = (0..30).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n");
        assert_eq!(reason(&[], &long).lines().next(), Some("line 10"), "without compiler errors, the last 20 lines");
    }
}
