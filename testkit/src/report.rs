//! What the runner prints: one line per test, and a summary. Pure functions, so the format is tested.

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
    fn a_screenshot_name_keeps_only_safe_characters() {
        assert_eq!(file_name("C5.1: installed signed-in Claude"), "C5_1__installed_signed-in_Claude");
    }
}
