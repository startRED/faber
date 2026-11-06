//! Checks that fail by chance leave the gate on their own (lean plan D5). On CI a failed check runs once more: if it
//! passes, it is `Flaky`, and the run names it in `flaky-report.json`. The gate job keeps the list of flaky checks
//! (`scripts/flaky.mjs`) and hands it back to the next runs in the file `FABER_FLAKY_FILE` names; a listed check that
//! fails does not fail the run (`Quarantined`) except in the nightly run. Pure functions, so the decisions are tested.

use crate::plan::{Failed, Suite};
use crate::report::{Outcome, Status};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The most failed checks run again. More than that is a break, not chance, and a second try would only double a red run.
pub const RETRY_AT_MOST: usize = 8;

/// The runner's own whole-suite failure (a build that failed, a test program that died): never chance.
const WHOLE_RUST: &str = "cargo test --workspace";

/// The names in the state file (`{"checks": {"<name>": {...}}}`). A missing or unreadable file lists nothing.
pub fn names(text: &str) -> Vec<String> {
    let value: Value = serde_json::from_str(text).unwrap_or_default();
    value.get("checks").and_then(Value::as_object).map(|checks| checks.keys().cloned().collect()).unwrap_or_default()
}

pub fn tolerated(actions: Option<&str>, event: Option<&str>) -> bool {
    actions == Some("true") && event != Some("schedule")
}

pub fn judge(outcome: &Outcome, names: &[String], enabled: bool) -> Outcome {
    let mut result = outcome.clone();
    if enabled && result.status == Status::Fail && names.contains(&result.name) {
        result.status = Status::Quarantined;
    }
    result
}

/// What to run again: the exact failed `rust` tests, the `ui` suite when one of its checks failed, and the failed
/// `screen` scenarios. Nothing when no check failed that could be chance, or when too many failed. Lints never repeat,
/// since they give the same answer twice.
pub fn retry(outcomes: &[Outcome]) -> Option<Failed> {
    let failed: Vec<&Outcome> = outcomes.iter().filter(|o| o.status == Status::Fail && o.suite != Suite::Lint).collect();
    if failed.is_empty() || failed.len() > RETRY_AT_MOST {
        return None;
    }
    let mut again = Failed::default();
    for outcome in failed {
        match outcome.suite {
            Suite::Rust if outcome.name != WHOLE_RUST => again.rust.push(outcome.name.clone()),
            Suite::Ui => again.ui = true,
            Suite::Screen => again.screen.push(outcome.name.clone()),
            _ => {}
        }
    }
    (!again.suites().is_empty()).then_some(again)
}

/// The first run's outcomes with the second run's word on each failure: a check that passed the second time is `Flaky`
/// (its first reason kept), one that failed again stays a failure with the newest reason. The `ui` suite runs whole the
/// second time, so a check that passed before and fails now, or one the first run never had, is a failure too: the second
/// run may hide nothing.
pub fn settle(first: Vec<Outcome>, second: &[Outcome]) -> Vec<Outcome> {
    let new_trouble: Vec<&Outcome> = second
        .iter()
        .filter(|o| matches!(o.status, Status::Fail | Status::NotRun))
        .filter(|o| !first.iter().any(|f| f.suite == o.suite && f.name == o.name && f.status != Status::Pass))
        .collect();
    let mut settled: Vec<Outcome> = first
        .into_iter()
        .map(|outcome| {
            if outcome.status != Status::Fail {
                return outcome;
            }
            match second.iter().find(|o| o.suite == outcome.suite && o.name == outcome.name) {
                Some(again) if again.status == Status::Pass => Outcome { status: Status::Flaky, time: again.time, ..outcome },
                Some(again) if again.status == Status::Fail => again.clone(),
                _ => outcome,
            }
        })
        .collect();
    for trouble in new_trouble {
        match settled.iter_mut().find(|o| o.suite == trouble.suite && o.name == trouble.name) {
            Some(was_pass) => *was_pass = trouble.clone(),
            None => settled.push(trouble.clone()),
        }
    }
    settled
}

/// What a run tells the gate job about flaky checks (`target/faber-test/flaky-report.json`).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Failed, then passed on the second try: new to the list, or still on it.
    pub flaky: BTreeSet<String>,
    /// On the list and passed the first time.
    pub clean: BTreeSet<String>,
    /// On the list and failed both times.
    pub failed: BTreeSet<String>,
}

impl Report {
    pub fn of(outcomes: &[Outcome], listed: &[String]) -> Report {
        let mut report = Report::default();
        for outcome in outcomes {
            let on_list = listed.contains(&outcome.name);
            let set = match outcome.status {
                Status::Flaky => &mut report.flaky,
                Status::Pass if on_list => &mut report.clean,
                Status::Fail | Status::Quarantined if on_list => &mut report.failed,
                _ => continue,
            };
            set.insert(outcome.name.clone());
        }
        report
    }

    pub fn is_empty(&self) -> bool {
        self.flaky.is_empty() && self.clean.is_empty() && self.failed.is_empty()
    }

    /// One job runs `cargo t` once per suite, so the reports of its runs are put together.
    pub fn merge(mut self, other: Report) -> Report {
        self.flaky.extend(other.flaky);
        self.clean.extend(other.clean);
        self.failed.extend(other.failed);
        self
    }

    pub fn parse(text: &str) -> Report {
        let value: Value = serde_json::from_str(text).unwrap_or_default();
        let set = |key: &str| -> BTreeSet<String> {
            value.get(key).and_then(Value::as_array).into_iter().flatten().filter_map(|n| n.as_str().map(String::from)).collect()
        };
        Report { flaky: set("flaky"), clean: set("clean"), failed: set("failed") }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&json!({ "flaky": self.flaky, "clean": self.clean, "failed": self.failed })).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{exit_code, line, summary};

    fn listed(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_the_names_of_the_state_file_and_nothing_from_a_broken_one() {
        let text = r#"{"checks": {"first check": {"since": "2026-11-06", "clean": 0}, "second": {}}}"#;
        assert_eq!(names(text), ["first check", "second"]);
        for broken in ["", "not json", "{}", r#"{"checks": []}"#] {
            assert!(names(broken).is_empty(), "{broken}");
        }
    }

    #[test]
    fn only_non_scheduled_actions_tolerates_failures() {
        for event in ["push", "pull_request", "workflow_dispatch"] {
            assert!(tolerated(Some("true"), Some(event)));
        }
        assert!(!tolerated(Some("true"), Some("schedule")));
        assert!(!tolerated(None, Some("push")));
        assert!(!tolerated(Some("false"), Some("push")));
    }

    #[test]
    fn quarantined_failure_is_visible_but_other_failures_still_fail() {
        let failure = Outcome::fail(Suite::Ui, "listed", None, "first line\nmore detail");
        let names = listed(&["listed"]);
        let quarantined = judge(&failure, &names, true);
        assert_eq!(line(&quarantined), "QUARANTINED listed: first line");
        assert_eq!(exit_code(std::slice::from_ref(&quarantined)), 0);
        assert!(summary(std::slice::from_ref(&quarantined)).contains("quarantined failures: 1"));
        assert_eq!(judge(&failure, &names, false).status, Status::Fail);
        let other = Outcome::fail(Suite::Ui, "other", None, "bad");
        assert_eq!(exit_code(&[quarantined, judge(&other, &names, true)]), 1);
        let not_run = Outcome::not_run(Suite::Ui, "listed", "did not run");
        assert_eq!(exit_code(&[judge(&not_run, &names, true)]), 1);
    }

    #[test]
    fn a_failure_is_run_again_unless_it_is_a_lint_a_dead_build_or_too_many() {
        assert!(retry(&[Outcome::pass(Suite::Ui, "a", None)]).is_none());
        let rust = Outcome::fail(Suite::Rust, "store::tests::saves", None, "x");
        let screen = Outcome::fail(Suite::Screen, "a chat opens", None, "x");
        let again = retry(&[rust.clone(), screen, Outcome::fail(Suite::Ui, "check", None, "x")]).unwrap();
        assert_eq!(
            (again.rust.clone(), again.screen.clone(), again.ui),
            (listed(&["store::tests::saves"]), listed(&["a chat opens"]), true)
        );
        assert!(!again.rust_whole && !again.lint);
        assert!(retry(&[Outcome::fail(Suite::Lint, "cargo clippy", None, "x")]).is_none());
        assert!(retry(&[Outcome::fail(Suite::Rust, WHOLE_RUST, None, "does not build")]).is_none());
        assert!(retry(&[Outcome::not_run(Suite::Ui, "the test app", "no build")]).is_none());
        let many: Vec<Outcome> = (0..=RETRY_AT_MOST).map(|i| Outcome::fail(Suite::Screen, &format!("s{i}"), None, "x")).collect();
        assert!(retry(&many).is_none(), "more than {RETRY_AT_MOST} failures are a break, not chance");
        assert!(retry(&many[..RETRY_AT_MOST]).is_some());
    }

    #[test]
    fn a_check_that_passes_the_second_time_is_flaky_and_one_that_fails_twice_still_fails_the_run() {
        let first = vec![
            Outcome::fail(Suite::Screen, "chance", None, "dropped frame"),
            Outcome::fail(Suite::Screen, "real", None, "first reason"),
            Outcome::fail(Suite::Screen, "not tried", None, "x"),
            Outcome::pass(Suite::Screen, "fine", None),
        ];
        let second = vec![
            Outcome::pass(Suite::Screen, "chance", None),
            Outcome::fail(Suite::Screen, "real", None, "second reason"),
            Outcome::pass(Suite::Rust, "not tried", None),
        ];
        let settled = settle(first, &second);
        let status = |name: &str| settled.iter().find(|o| o.name == name).unwrap().status;
        assert_eq!(status("chance"), Status::Flaky);
        assert_eq!(status("real"), Status::Fail);
        assert_eq!(status("not tried"), Status::Fail, "a name in another suite says nothing");
        assert_eq!(status("fine"), Status::Pass);
        assert_eq!(settled.len(), 4);
        assert_eq!(line(&settled[0]), "FLAKY chance: dropped frame");
        assert_eq!(settled[1].message, "second reason");
        assert_eq!(exit_code(&settled), 1);
        assert_eq!(exit_code(&settled[..1]), 0, "a flaky check alone leaves the run green");
        assert!(summary(&settled[..1]).ends_with("flaky: 1"));
    }

    #[test]
    fn the_second_run_may_hide_nothing_a_check_that_fails_only_now_still_fails_the_run() {
        // The `ui` suite runs whole again: A failed then passed, B passed then failed, C is new and failed.
        let first = vec![Outcome::fail(Suite::Ui, "A", None, "x"), Outcome::pass(Suite::Ui, "B", None)];
        let second = vec![
            Outcome::pass(Suite::Ui, "A", None),
            Outcome::fail(Suite::Ui, "B", None, "now"),
            Outcome::fail(Suite::Ui, "C", None, "new"),
            Outcome::not_run(Suite::Ui, "the test app", "no build"),
        ];
        let settled = settle(first, &second);
        let status = |name: &str| settled.iter().find(|o| o.name == name).unwrap().status;
        assert_eq!(
            [status("A"), status("B"), status("C"), status("the test app")],
            [Status::Flaky, Status::Fail, Status::Fail, Status::NotRun]
        );
        assert_eq!(exit_code(&settled), 1);
        let unchanged = settle(vec![Outcome::fail(Suite::Ui, "A", None, "x")], &[Outcome::pass(Suite::Ui, "A", None)]);
        assert_eq!(exit_code(&unchanged), 0);
    }

    #[test]
    fn the_report_names_new_flaky_checks_and_what_happened_to_the_listed_ones() {
        let outcomes = vec![
            Outcome { status: Status::Flaky, ..Outcome::fail(Suite::Screen, "new", None, "x") },
            Outcome::pass(Suite::Screen, "listed and clean", None),
            Outcome::pass(Suite::Screen, "never listed", None),
            Outcome::fail(Suite::Screen, "listed and red", None, "x"),
            Outcome { status: Status::Quarantined, ..Outcome::fail(Suite::Ui, "listed and quarantined", None, "x") },
            Outcome::fail(Suite::Screen, "never listed and red", None, "x"),
        ];
        let names = listed(&["listed and clean", "listed and red", "listed and quarantined"]);
        let report = Report::of(&outcomes, &names);
        assert_eq!(report.flaky, BTreeSet::from(["new".to_string()]));
        assert_eq!(report.clean, BTreeSet::from(["listed and clean".to_string()]));
        assert_eq!(report.failed, BTreeSet::from(["listed and quarantined".to_string(), "listed and red".to_string()]));
        assert_eq!(Report::parse(&report.to_json()), report);
        assert!(Report::of(&[Outcome::pass(Suite::Ui, "a", None)], &[]).is_empty());
        let both = report.merge(Report::parse(r#"{"flaky": ["other"]}"#));
        assert_eq!(both.flaky.len(), 2);
        assert!(Report::parse("garbage").is_empty());
    }
}
