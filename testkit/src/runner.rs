//! `faber-test`: reads what changed, picks the suites, runs them and prints one line per test.

use crate::app::{Paths, emergency_cleanup};
use crate::checks;
use crate::plan::{Options, Suite, changed_paths, jobs, parse_args, select};
use crate::report::{Outcome, Status, exit_code, line, summary};
use crate::screen;
use crate::slots;
use crate::sweep;
use crate::testbuild::{build_test_app, ensure_fake_cli};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const USAGE: &str = "usage: cargo t [--all] [--suite rust|ui|screen] [--filter <text>] [--shard <i/n>]";

/// The result lines of `cargo test`: `test some::name ... ok`. Anything that is not a test line is ignored.
pub fn parse_cargo_tests(output: &str) -> Vec<(String, bool)> {
    output
        .lines()
        .filter_map(|l| l.strip_prefix("test "))
        .filter_map(|rest| {
            let (name, verdict) = rest.rsplit_once(" ... ")?;
            match verdict.trim() {
                "ok" => Some((name.to_string(), true)),
                "FAILED" => Some((name.to_string(), false)),
                _ => None,
            }
        })
        .collect()
}

/// The `cargo test` command line of the `rust` suite: `-j` and `--test-threads` both set to the same number.
pub fn cargo_test_args(jobs: usize, filter: Option<&str>) -> Vec<String> {
    let mut args = vec!["test".to_string(), "--workspace".into(), "-j".into(), jobs.to_string()];
    args.extend(filter.map(str::to_string));
    args.extend(["--".to_string(), "--test-threads".into(), jobs.to_string()]);
    args
}

fn failure_output(stdout: &str, stderr: &str) -> String {
    [stdout, stderr]
        .map(|text| {
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(60)..].join("\n")
        })
        .join("\n")
}

fn run_rust(root: &Path, jobs: usize, filter: Option<&str>) -> Vec<Outcome> {
    let started = Instant::now();
    let mut command = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    command.current_dir(root).args(cargo_test_args(jobs, filter));
    let output = match command.output() {
        Ok(output) => output,
        Err(e) => return vec![Outcome::fail(Suite::Rust, "cargo test --workspace", None, &format!("cannot start cargo: {e}"))],
    };
    let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let mut outcomes: Vec<Outcome> = parse_cargo_tests(&text)
        .into_iter()
        .map(|(name, ok)| if ok { Outcome::pass(Suite::Rust, &name, None) } else { Outcome::fail(Suite::Rust, &name, None, "") })
        .collect();
    if !output.status.success() {
        // A build error has no test lines at all; a test failure has its panic message in the output.
        let shown = failure_output(&String::from_utf8_lossy(&output.stdout), &String::from_utf8_lossy(&output.stderr));
        outcomes.push(Outcome::fail(Suite::Rust, "cargo test --workspace", Some(started.elapsed()), &shown));
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

/// Keeps the main checkout's `target` under the cap (R106), at most once an hour. Prints a line only when it freed space.
/// Returns that `target`, which every worktree shares.
fn sweep_builds(paths: &Paths) -> Option<PathBuf> {
    let output = Command::new("git").current_dir(&paths.root).args(["worktree", "list", "--porcelain"]).output().ok()?;
    let worktrees = sweep::worktrees(&String::from_utf8_lossy(&output.stdout));
    let (Some(main), Some(own)) = (worktrees.first(), paths.target.parent()) else { return None };
    let target = main.join("target");
    let now = std::time::SystemTime::now();
    if !output.status.success() || !target.is_dir() {
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

fn show(outcomes: &[Outcome]) {
    for outcome in outcomes {
        println!("{}", line(outcome));
    }
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
    let shared = sweep_builds(&paths);
    let changed = if opts.all || opts.suite.is_some() {
        Vec::new()
    } else {
        match changed(&paths.root) {
            Ok(list) => list,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        }
    };
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
    let jobs = jobs();
    let filter = opts.filter.as_deref();
    let mut outcomes: Vec<Outcome> = Vec::new();
    if suites.contains(&Suite::Rust) {
        let found = run_rust(&paths.root, jobs, filter);
        show(&found);
        outcomes.extend(found);
    }
    let (wants_ui, wants_screen) = (suites.contains(&Suite::Ui), suites.contains(&Suite::Screen));
    // Without WebView2's DevTools port, `screen` is skipped, and the app is only built if `ui` needs it.
    let screen_runs = wants_screen && cfg!(windows);
    if wants_ui || wants_screen {
        if wants_screen && !screen_runs {
            let skipped = Outcome::skipped(Suite::Screen, "screen", "needs WebView2's DevTools port, which only Windows has");
            show(std::slice::from_ref(&skipped));
            outcomes.push(skipped);
        }
        if wants_ui || screen_runs {
            match ensure_fake_cli(&paths, jobs).and_then(|()| build_test_app(&paths, jobs)) {
                Err(message) => {
                    for suite in [Suite::Ui, Suite::Screen].into_iter().filter(|s| suites.contains(s) && (*s == Suite::Ui || screen_runs)) {
                        let not_run = Outcome::not_run(suite, "the test app", &message);
                        show(std::slice::from_ref(&not_run));
                        outcomes.push(not_run);
                    }
                }
                Ok(()) => {
                    if wants_ui {
                        let found = checks::run(&paths, filter);
                        show(&found);
                        outcomes.extend(found);
                    }
                    if screen_runs {
                        // Each scenario is printed as it ends: the suite takes minutes.
                        outcomes.extend(screen::run_all(&paths, filter, opts.shard, &mut |outcome| println!("{}", line(outcome))));
                    }
                }
            }
        }
    }
    if nothing_matched(&outcomes, filter) {
        println!("No test name contains {:?}. Nothing ran.", filter.unwrap_or_default());
        return 1;
    }
    println!("\n{}", summary(&outcomes));
    if outcomes.iter().any(|o| o.status == Status::Fail) {
        println!("Screenshots of failures are in {}", paths.shots.display());
    }
    exit_code(&outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_failure_output_keeps_the_panic_when_cargo_stderr_is_long() {
        let stderr = (0..100).map(|n| format!("Running target {n}")).collect::<Vec<_>>().join("\n");
        let shown = failure_output("thread 'deadline' panicked: native socket timeout\nFAILED", &stderr);
        assert!(shown.contains("panicked: native socket timeout"));
        assert!(shown.contains("Running target 99"));
    }

    #[test]
    fn a_filter_that_matched_no_test_anywhere_is_a_mistake_but_no_filter_and_no_tests_is_not_this_check() {
        let one = [Outcome::pass(Suite::Ui, "i18n: keys", None)];
        assert!(nothing_matched(&[], Some("zzz")));
        assert!(!nothing_matched(&one, Some("i18n")));
        assert!(!nothing_matched(&[], None), "without a filter an empty run is another kind of problem");
    }

    #[test]
    fn cargo_test_uses_the_same_number_for_jobs_and_test_threads() {
        assert_eq!(cargo_test_args(3, None), ["test", "--workspace", "-j", "3", "--", "--test-threads", "3"]);
        assert_eq!(cargo_test_args(1, Some("fake")), ["test", "--workspace", "-j", "1", "fake", "--", "--test-threads", "1"]);
    }

    #[test]
    fn test_lines_become_pass_or_fail_and_everything_else_is_ignored() {
        let output = "running 3 tests\ntest plan::tests::a ... ok\ntest plan::tests::b ... FAILED\ntest plan::tests::c ... ignored\n\
                      test result: FAILED. 1 passed; 1 failed; 1 ignored\ntest sandbox::tests::long_name_with ... ok\n";
        assert_eq!(
            parse_cargo_tests(output),
            [
                ("plan::tests::a".to_string(), true),
                ("plan::tests::b".to_string(), false),
                ("sandbox::tests::long_name_with".to_string(), true)
            ]
        );
    }
}
