//! `faber-test`: reads what changed, checks the lane's claim, picks the suites and runs them. The screen gets the
//! failures, the measurements and a summary; every line goes to the run's log (R154). Cargo's output is read by the same
//! functions that read a mission's checks (`engine/src/project/proof/output.rs`, R155).

use crate::app::{Paths, emergency_cleanup};
use crate::base;
use crate::checks;
use crate::cross;
use crate::lanes;
use crate::plan::{Options, Suite, changed_paths, jobs, parse_args, parse_failed, select};
use crate::report::{Outcome, Status, exit_code, line, quiet, summary};
use crate::screen;
use crate::slots;
use crate::sweep;
use crate::testbuild::{build_for_run, build_side_by_side, cargo_command, prebuilt, streamed};
use crate::timing;
use faber_engine::project::proof::output::{cargo_output, died, failure_sections, parse_cargo_tests};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Instant;

const USAGE: &str = "usage: cargo t [--all] [--suite rust|ui|screen|lint] [--filter <text> | --failed] [--shard <i/n> [--costs <file>]] [--build-only | --prebuilt] [--claims]";

/// The two Clippy proofs, as CI runs them (a test keeps them equal). `--suite lint` runs them with [`lint_args`].
pub const LINTS: [&[&str]; 2] = [
    &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
    &["clippy", "-p", "faber", "--features", "test-hooks", "--", "-D", "warnings"],
];

/// The test programs of the `rust` suite: libraries, binaries and `tests/`. Plain `cargo test` also builds every example
/// and runs rustdoc on each library, though no example is a test program and no doc comment holds a test: about a fifth
/// of a rebuild after one changed engine file (cargo --timings, 2026-10-23). The lint suite still checks every example.
const TEST_PROGRAMS: [&str; 3] = ["--lib", "--bins", "--tests"];

/// The `cargo test` command line of the `rust` suite: every test program runs even after one fails, compiler messages
/// come as JSON, and `-j` and `--test-threads` are the same number.
pub fn cargo_test_args(jobs: usize, filter: Option<&str>) -> Vec<String> {
    let mut args = ["test", "--workspace"]
        .into_iter()
        .chain(TEST_PROGRAMS)
        .chain(["--no-fail-fast", "--message-format=json", "-j"])
        .map(String::from)
        .collect::<Vec<_>>();
    args.push(jobs.to_string());
    args.extend(filter.map(str::to_string));
    args.extend(["--".to_string(), "--test-threads".into(), jobs.to_string()]);
    args
}

/// The same, for exactly these tests (`--failed`): libtest takes several names, and `--exact` makes each one a whole name.
pub fn cargo_test_exact_args(jobs: usize, names: &[String]) -> Vec<String> {
    // Cargo takes one test name before `--`; the test programs take any number after it.
    let mut args = cargo_test_args(jobs, None);
    args.push("--exact".into());
    args.extend(names.iter().cloned());
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

/// Runs Cargo and reads its output as it comes, so the log tells how long the build took before the first test ran.
fn cargo(mut command: Command, root: &Path, args: &[String], what: &str) -> std::io::Result<Output> {
    let started = Instant::now();
    let mut built = false;
    command.current_dir(root).args(args);
    let output = streamed(&mut command, |line| {
        if !built && line.contains("\"reason\":\"build-finished\"") {
            built = true;
            timing::record("compile", what, started.elapsed());
        }
    });
    if !built {
        timing::record("compile", what, started.elapsed());
    }
    output
}

fn run_rust(root: &Path, jobs: usize, filter: Option<&str>, exact: &[String], log: &mut Log) -> Vec<Outcome> {
    let _rust = timing::span("rust", "cargo test --workspace");
    let started = Instant::now();
    let args = if exact.is_empty() { cargo_test_args(jobs, filter) } else { cargo_test_exact_args(jobs, exact) };
    let output = match cargo(cargo_command(), root, &args, "cargo test --workspace") {
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
    let mut outcomes: Vec<Outcome> = LINTS.iter().map(|proof| lint(cargo_command(), root, proof, jobs, log)).collect();
    // On a Windows PC the macOS code is checked too, before CI finds it (`cross`). A Mac's own Clippy already covers it.
    if cfg!(windows) {
        let name = format!("cargo {}", cross::PROOF.join(" "));
        let mut command = cargo_command();
        outcomes.push(match cross::find_zig() {
            None => Outcome::skipped(Suite::Lint, &name, "needs zig on PATH (docs/dev/testing.md#screen-tests)"),
            Some(zig) => match cross::use_zig(&mut command, &zig) {
                Ok(()) => lint(command, root, cross::PROOF, jobs, log),
                Err(e) => Outcome::fail(Suite::Lint, &name, None, &format!("cannot find the runner: {e}")),
            },
        });
    }
    outcomes
}

fn lint(command: Command, root: &Path, proof: &[&str], jobs: usize, log: &mut Log) -> Outcome {
    let (started, name) = (Instant::now(), format!("cargo {}", proof.join(" ")));
    let _lint = timing::span("lint", &name);
    let output = match cargo(command, root, &lint_args(proof, jobs), &name) {
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
}

/// Every line of a run, whole, in `target/faber-test/run.log`. The screen gets only what [`quiet`] keeps.
struct Log {
    path: PathBuf,
    file: Option<File>,
    quarantine: Vec<&'static str>,
    tolerate: bool,
}

impl Log {
    fn create(path: PathBuf) -> Log {
        let file = path.parent().and_then(|dir| std::fs::create_dir_all(dir).ok()).and_then(|()| File::create(&path).ok());
        let quarantine = crate::quarantine::names(include_str!("../quarantine.txt"));
        let tolerate = crate::quarantine::tolerated(
            std::env::var("GITHUB_ACTIONS").ok().as_deref(),
            std::env::var("GITHUB_EVENT_NAME").ok().as_deref(),
        );
        Log { path, file, quarantine, tolerate }
    }

    fn write(&mut self, text: &str) {
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{}", text.trim_end());
        }
    }

    fn show(&mut self, outcomes: &[Outcome]) {
        for timed in timing::drain() {
            self.write(&timed);
        }
        for outcome in outcomes {
            let outcome = crate::quarantine::judge(outcome, &self.quarantine, self.tolerate);
            self.write(&line(&outcome));
            if let Some(text) = quiet(&outcome) {
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
fn sweep_builds(paths: &Paths, porcelain: Option<&str>, loose: &Path, now: std::time::SystemTime) -> Option<PathBuf> {
    let worktrees = sweep::worktrees(porcelain?);
    let (Some(main), Some(own)) = (worktrees.first(), paths.target.parent()) else { return None };
    let target = main.join("target");
    sweep_root(loose, own, &worktrees, now);
    if !std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.is_dir() && !meta.is_symlink()) {
        return None;
    }
    sweep_root(&target, own, &worktrees, now);
    Some(target)
}

/// Sweeps one build root when its hour is due, and says so only when it freed space or stays above the cap.
fn sweep_root(target: &Path, own: &Path, worktrees: &[PathBuf], now: std::time::SystemTime) {
    if !std::fs::symlink_metadata(target).is_ok_and(|meta| meta.is_dir() && !meta.is_symlink()) || !sweep::due(target, now) {
        return;
    }
    let report = if worktrees.first().is_some_and(|main| target == main.join("target").as_path()) {
        sweep::sweep_worktrees(target, own, worktrees, now, sweep::CAP)
    } else {
        sweep::sweep(target, own, now, sweep::CAP)
    };
    let gb = |bytes: u64| bytes as f64 / (1u64 << 30) as f64;
    let at = target.display();
    if report.after < report.before {
        let removed = if report.removed.is_empty() { String::new() } else { format!(", removed {}", report.removed.join(", ")) };
        println!("Build folder {at}: {:.1} GB -> {:.1} GB{removed}", gb(report.before), gb(report.after));
    }
    if report.after > sweep::CAP {
        println!(
            "Build folder {at}: {:.1} GB is above the {:.0} GB cap, and nothing else is idle to remove",
            gb(report.after),
            gb(sweep::CAP)
        );
    }
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
    let started = Instant::now();
    let mut log = None;
    let code = run_suites(args, &mut log);
    let (line, json) = timing::totals(started.elapsed());
    if let Some(log) = &mut log {
        for timed in timing::drain() {
            log.write(&timed);
        }
        log.write(&timing::steps_line());
        log.write(&line);
    }
    if let Ok(paths) = Paths::discover() {
        let _ = std::fs::write(paths.target.join("timing.json"), serde_json::to_string_pretty(&json).unwrap_or_default());
    }
    if code != 2 {
        println!("{line}");
    }
    code
}

/// The run itself. The log it opens is handed back, so the timing lines go at its end.
fn run_suites(args: &[String], log_out: &mut Option<Log>) -> i32 {
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
    let shared = sweep_builds(&paths, worktrees.as_deref(), &sweep::loose_root(), std::time::SystemTime::now());
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
    if opts.claims {
        base::warn(&paths.root);
        println!("Claims hold; nothing was built. The suites run in CI on the queued commit.");
        return 0;
    }
    let jobs = jobs();
    if opts.build_only {
        return match build_side_by_side(&paths, jobs) {
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
    let failed =
        if opts.failed { Some(parse_failed(&std::fs::read_to_string(paths.target.join("run.log")).unwrap_or_default())) } else { None };
    let suites = match &failed {
        Some(failed) => failed.suites().into_iter().filter(|suite| opts.suite.is_none_or(|only| only == *suite)).collect(),
        None => select(&opts, &changed),
    };
    let costs = match opts.costs.as_deref().map(screen::Costs::read).unwrap_or_else(|| Ok(screen::Costs::committed())) {
        Ok(costs) => costs,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if suites.is_empty() && failed.is_some() {
        println!("No failed test is recorded in {}. Nothing ran.", paths.target.join("run.log").display());
        return 0;
    }
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
    let log = log_out.insert(Log::create(paths.target.join("run.log")));
    let filter = opts.filter.as_deref();
    let mut outcomes: Vec<Outcome> = Vec::new();
    if suites.contains(&Suite::Lint) {
        let found = run_lint(&paths.root, jobs, log);
        log.show(&found);
        outcomes.extend(found);
    }
    if suites.contains(&Suite::Rust) {
        // A failed build or a dead test program names no test, so the whole suite runs again.
        let exact: &[String] = failed.as_ref().filter(|f| !f.rust_whole).map_or(&[], |f| f.rust.as_slice());
        let found = run_rust(&paths.root, jobs, filter, exact, log);
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
            let built = if opts.prebuilt { prebuilt(&paths) } else { build_for_run(&paths, jobs) };
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
                        let _ui = timing::span("ui", "checks");
                        let found = checks::run(&paths, filter);
                        log.show(&found);
                        outcomes.extend(found);
                    }
                    if screen_runs {
                        let _screen = timing::span("screen", "scenarios");
                        // Each scenario is logged as it ends, and a failure shows at once: the suite takes minutes.
                        let only = failed.as_ref().map(|f| f.screen.as_slice());
                        let found = screen::run_all(&paths, filter, only, opts.shard, &costs, &mut |outcome| {
                            log.show(std::slice::from_ref(outcome))
                        });
                        // A name the log holds that no scenario has any more (it was renamed) is never a quiet pass.
                        let missing: Vec<Outcome> = only
                            .into_iter()
                            .flatten()
                            .filter(|name| !found.iter().any(|o| &o.name == *name))
                            .map(|name| Outcome::not_run(Suite::Screen, name, "no scenario has this name any more"))
                            .collect();
                        log.show(&missing);
                        outcomes.extend(found);
                        outcomes.extend(missing);
                    }
                }
            }
        }
    }
    if nothing_matched(&outcomes, filter) {
        println!("No test name contains {:?}. Nothing ran.", filter.unwrap_or_default());
        return 1;
    }
    let outcomes: Vec<_> = outcomes.iter().map(|o| crate::quarantine::judge(o, &log.quarantine, log.tolerate)).collect();
    let total = summary(&outcomes);
    log.write(&total);
    let every = if log.file.is_some() { format!(". Every line: {}", log.path.display()) } else { String::new() };
    println!("\n{total}{every}");
    if outcomes.iter().any(|o| matches!(o.status, Status::Fail | Status::Quarantined) && matches!(o.suite, Suite::Ui | Suite::Screen)) {
        println!("Screenshots of failures are in {}", paths.shots.display());
    }
    exit_code(&outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loose_build_root_is_swept_like_the_main_target_and_a_live_lane_s_folder_stays() {
        let temp = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let (main, loose) = (temp.path().join("faber"), temp.path().join("loose"));
        let lane = main.join(".claude/worktrees/lane");
        for dir in [main.join("target/debug/.fingerprint"), loose.join("gone/debug/.fingerprint"), loose.join("lane/debug/.fingerprint")] {
            std::fs::create_dir_all(&dir).unwrap();
        }
        for name in ["gone", "lane"] {
            std::fs::write(loose.join(name).join("CACHEDIR.TAG"), "").unwrap();
            std::fs::write(loose.join(name).join("debug/faber.pdb"), [0u8; 100]).unwrap();
        }
        let target = loose.join("lane/faber-test");
        let paths = Paths {
            root: lane.clone(),
            app: target.clone(),
            fake_cli: target.clone(),
            fixtures: target.clone(),
            shots: target.clone(),
            target,
        };
        let porcelain = format!("worktree {}\nworktree {}\n", main.display(), lane.display());

        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(2 * 24 * 60 * 60);
        assert_eq!(sweep_builds(&paths, Some(&porcelain), &loose, later), Some(main.join("target")));
        assert!(!loose.join("gone/debug/faber.pdb").exists(), "old loose output goes; its lock files stay");
        assert!(loose.join("lane/debug/faber.pdb").exists(), "the folder of a live lane stays");
    }

    #[test]
    fn a_filter_that_matched_no_test_anywhere_is_a_mistake_but_no_filter_and_no_tests_is_not_this_check() {
        let one = [Outcome::pass(Suite::Ui, "i18n: keys", None)];
        assert!(nothing_matched(&[], Some("zzz")));
        assert!(!nothing_matched(&one, Some("i18n")));
        assert!(!nothing_matched(&[], None), "without a filter an empty run is another kind of problem");
    }

    #[test]
    fn cargo_test_runs_every_program_with_json_messages_and_the_same_number_for_jobs_and_test_threads() {
        let start = ["test", "--workspace", "--lib", "--bins", "--tests", "--no-fail-fast", "--message-format=json", "-j"];
        assert_eq!(cargo_test_args(3, None), [&start[..], &["3", "--", "--test-threads", "3"]].concat());
        assert_eq!(cargo_test_args(1, Some("fake")), [&start[..], &["1", "fake", "--", "--test-threads", "1"]].concat());
    }

    #[test]
    fn failed_rust_tests_run_again_by_their_exact_names() {
        let names = ["a::b::one".to_string(), "c::two".to_string()];
        assert_eq!(
            cargo_test_exact_args(2, &names),
            [
                "test",
                "--workspace",
                "--lib",
                "--bins",
                "--tests",
                "--no-fail-fast",
                "--message-format=json",
                "-j",
                "2",
                "--",
                "--test-threads",
                "2",
                "--exact",
                "a::b::one",
                "c::two"
            ]
        );
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
