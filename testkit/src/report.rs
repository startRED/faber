//! What the runner shows and logs: one line per test for the log, the failures and measurements for the screen, and a
//! summary. Pure functions, so the format is tested.

use crate::plan::Suite;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    /// The suite could not start (the build failed), which is never reported as a pass.
    NotRun,
    Skipped,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub suite: Suite,
    pub name: String,
    pub status: Status,
    /// How long it took, when it is known.
    pub time: Option<Duration>,
    /// Why it failed or was skipped; for a pass, a note such as a measurement.
    pub message: String,
}

impl Outcome {
    pub fn pass(suite: Suite, name: &str, time: Option<Duration>) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Pass, time, message: String::new() }
    }

    pub fn pass_with(suite: Suite, name: &str, time: Option<Duration>, note: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Pass, time, message: note.into() }
    }

    pub fn fail(suite: Suite, name: &str, time: Option<Duration>, message: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Fail, time, message: message.into() }
    }

    pub fn not_run(suite: Suite, name: &str, message: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::NotRun, time: None, message: message.into() }
    }

    pub fn skipped(suite: Suite, name: &str, message: &str) -> Outcome {
        Outcome { suite, name: name.into(), status: Status::Skipped, time: None, message: message.into() }
    }
}

fn seconds(time: Duration) -> String {
    if time < Duration::from_secs(1) { format!("{} ms", time.as_millis()) } else { format!("{:.1} s", time.as_secs_f64()) }
}

/// `pass  ui      i18n: every locale has every key (4 ms)`, and the reason on the lines after a failure.
pub fn line(outcome: &Outcome) -> String {
    let word = match outcome.status {
        Status::Pass => "pass",
        Status::Fail => "FAIL",
        Status::NotRun => "----",
        Status::Skipped => "skip",
    };
    let mut text = format!("{word}  {:<6}  {}", outcome.suite.name(), outcome.name);
    if let Some(time) = outcome.time {
        text.push_str(&format!(" ({})", seconds(time)));
    }
    if outcome.status == Status::Pass && !outcome.message.is_empty() {
        text.push_str(&format!("  [{}]", outcome.message));
    }
    if outcome.status != Status::Pass && !outcome.message.is_empty() {
        for message_line in outcome.message.lines() {
            text.push_str(&format!("\n          {message_line}"));
        }
    }
    text
}

/// The most lines of a reason the screen shows; the log keeps it whole.
const BRIEF: usize = 15;

/// A long reason cut for the screen: its first line, then its error lines (`error...`, ` --> file:line`), or its first
/// lines when none looks like an error.
pub fn brief(message: &str) -> String {
    let lines: Vec<&str> = message.lines().collect();
    if lines.len() <= BRIEF {
        return message.to_string();
    }
    let errors = lines[1..].iter().filter(|l| ["error", "-->"].iter().any(|start| l.trim_start().starts_with(start)));
    let mut kept: Vec<&str> = lines[..1].iter().chain(errors).take(BRIEF).copied().collect();
    if kept.len() == 1 {
        kept = lines[..BRIEF].to_vec();
    }
    format!("{}\n({} more lines in the log)", kept.join("\n"), lines.len() - kept.len())
}

/// Whether a pass's note measures memory or processor (`143 MB`, `8.3%`), which a change reports (R125). Other notes,
/// such as an audit's evidence, stay in the log.
fn measures(note: &str) -> bool {
    ["MB", "%"].iter().any(|unit| note.match_indices(unit).any(|(at, _)| note[..at].trim_end().ends_with(|c: char| c.is_ascii_digit())))
}

/// What the screen shows of an outcome (R154): a pass only when it measures memory or processor, and anything else with
/// its reason cut by [`brief`].
pub fn quiet(outcome: &Outcome) -> Option<String> {
    match outcome.status {
        Status::Pass if measures(&outcome.message) => Some(line(outcome)),
        Status::Pass => None,
        _ => Some(line(&Outcome { message: brief(&outcome.message), ..outcome.clone() })),
    }
}

pub fn summary(outcomes: &[Outcome]) -> String {
    let count = |status| outcomes.iter().filter(|o| o.status == status).count();
    format!(
        "{} passed, {} failed, {} not run, {} skipped",
        count(Status::Pass),
        count(Status::Fail),
        count(Status::NotRun),
        count(Status::Skipped)
    )
}

/// 1 when anything failed or could not run; skipping a suite that does not exist on this system is fine.
pub fn exit_code(outcomes: &[Outcome]) -> i32 {
    i32::from(outcomes.iter().any(|o| matches!(o.status, Status::Fail | Status::NotRun)))
}

/// A name that is safe to use as a file name, for the screenshot of a failed test.
pub fn file_name(test: &str) -> String {
    test.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_has_the_word_the_suite_the_name_and_the_time() {
        let pass = Outcome::pass(Suite::Ui, "i18n: every locale has every key", Some(Duration::from_millis(4)));
        assert_eq!(line(&pass), "pass  ui      i18n: every locale has every key (4 ms)");
        let slow = Outcome::pass(Suite::Screen, "chat", Some(Duration::from_millis(3210)));
        assert_eq!(line(&slow), "pass  screen  chat (3.2 s)");
        let measured = Outcome::pass_with(Suite::Screen, "idle memory", Some(Duration::from_millis(900)), "idle: 143 MB");
        assert_eq!(line(&measured), "pass  screen  idle memory (900 ms)  [idle: 143 MB]");
    }

    #[test]
    fn a_failure_shows_its_reason_under_the_line() {
        let fail = Outcome::fail(Suite::Screen, "chat", None, "no element matches\nstderr: boom");
        assert_eq!(line(&fail), "FAIL  screen  chat\n          no element matches\n          stderr: boom");
    }

    #[test]
    fn the_summary_counts_every_kind_and_only_failures_and_not_runs_fail_the_run() {
        let mut all = vec![Outcome::pass(Suite::Rust, "a", None), Outcome::skipped(Suite::Screen, "needs WebView2", "x")];
        assert_eq!(summary(&all), "1 passed, 0 failed, 0 not run, 1 skipped");
        assert_eq!(exit_code(&all), 0);
        all.push(Outcome::not_run(Suite::Ui, "ui", "the build failed"));
        assert_eq!(exit_code(&all), 1, "a suite that could not run is never a pass");
        all.pop();
        all.push(Outcome::fail(Suite::Ui, "b", None, ""));
        assert_eq!(summary(&all), "1 passed, 1 failed, 0 not run, 1 skipped");
        assert_eq!(exit_code(&all), 1);
    }

    #[test]
    fn the_screen_skips_plain_passes_and_keeps_measurements_and_failures() {
        assert_eq!(quiet(&Outcome::pass(Suite::Rust, "a", None)), None);
        let measured = Outcome::pass_with(Suite::Screen, "idle memory", None, "idle: 143 MB");
        assert_eq!(quiet(&measured).unwrap(), "pass  screen  idle memory  [idle: 143 MB]");
        assert!(quiet(&Outcome::pass_with(Suite::Screen, "dust", None, "processor of one core: focused 8.3%")).is_some());
        let evidence = Outcome::pass_with(Suite::Screen, "T13", None, "temporary repos only; no remote; MB of head=\"5a66e78\"");
        assert_eq!(quiet(&evidence), None, "an audit's evidence is not a measurement");
        let fail = Outcome::fail(Suite::Rust, "b", None, "panicked at engine/src/x.rs:1:2:\nboom");
        assert_eq!(quiet(&fail).unwrap(), line(&fail));
    }

    #[test]
    fn a_long_reason_keeps_its_first_line_and_its_errors_and_the_rest_is_in_the_log() {
        let compiling = (0..40).map(|n| format!("   Compiling crate{n} v1.0.0")).collect::<Vec<_>>().join("\n");
        let build =
            format!("the shell build failed (exit code: 1)\n{compiling}\nerror[E0308]: mismatched types\n  --> src-tauri/src/x.rs:3:9\n");
        assert_eq!(
            brief(&build),
            "the shell build failed (exit code: 1)\nerror[E0308]: mismatched types\n  --> src-tauri/src/x.rs:3:9\n(40 more lines in the log)"
        );
        let plain = (0..20).map(|n| format!("missing {n}")).collect::<Vec<_>>().join("\n");
        assert_eq!(brief(&plain).lines().count(), BRIEF + 1, "no error lines: the first ones");
        assert!(brief(&plain).ends_with("(5 more lines in the log)"));
        assert_eq!(brief("short\nreason"), "short\nreason");
    }

    #[test]
    fn a_screenshot_name_keeps_only_safe_characters() {
        assert_eq!(file_name("C5.1: installed signed-in Claude"), "C5_1__installed_signed-in_Claude");
    }
}
