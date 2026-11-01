use crate::report::{Outcome, Status};

pub fn names(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_once(" #").map_or(line, |(name, _)| name).trim())
        .filter(|name| !name.is_empty())
        .collect()
}

pub fn tolerated(actions: Option<&str>, event: Option<&str>) -> bool {
    actions == Some("true") && event != Some("schedule")
}

pub fn judge(outcome: &Outcome, names: &[&str], enabled: bool) -> Outcome {
    let mut result = outcome.clone();
    if enabled && result.status == Status::Fail && names.contains(&result.name.as_str()) {
        result.status = Status::Quarantined;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Suite;
    use crate::report::{exit_code, line, summary};

    #[test]
    fn parses_names_and_ignores_comments_and_blanks() {
        assert_eq!(names("  # note\n\n first check # reason, date, run\n second check \n"), ["first check", "second check"]);
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
        let listed = names("listed # reason");
        let quarantined = judge(&failure, &listed, true);
        assert_eq!(line(&quarantined), "QUARANTINED listed: first line");
        assert_eq!(exit_code(std::slice::from_ref(&quarantined)), 0);
        assert!(summary(std::slice::from_ref(&quarantined)).contains("quarantined failures: 1"));
        assert_eq!(judge(&failure, &listed, false).status, Status::Fail);
        let other = Outcome::fail(Suite::Ui, "other", None, "bad");
        assert_eq!(exit_code(&[quarantined, judge(&other, &listed, true)]), 1);
        let not_run = Outcome::not_run(Suite::Ui, "listed", "did not run");
        assert_eq!(exit_code(&[judge(&not_run, &listed, true)]), 1);
    }
}
