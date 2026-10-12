//! The `ui` suite: the TypeScript checks in `tests/ui/`, run inside a hidden test build of Faber. The app runs them when
//! its window loads, writes the result to the file named by `FABER_CHECKS_OUT` and exits. No DevTools port is involved,
//! so this works on every system.

use crate::app::{Ctx, Paths, startup};
use crate::plan::Suite;
use crate::report::Outcome;
use crate::sandbox::{CliSlot, Launch};
use serde_json::Value;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub message: String,
    pub ms: f64,
}

/// The page's report: a JSON array of `{ name, ok, message, ms }`.
pub fn parse_report(text: &str) -> Result<Vec<Check>, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("the checks report is not JSON: {e}"))?;
    let list = value.as_array().ok_or("the checks report is not a list")?;
    list.iter()
        .map(|item| {
            Ok(Check {
                name: item["name"].as_str().ok_or("a check has no name")?.to_string(),
                ok: item["ok"].as_bool().ok_or("a check has no verdict")?,
                message: item["message"].as_str().unwrap_or_default().to_string(),
                ms: item["ms"].as_f64().unwrap_or(0.0),
            })
        })
        .collect()
}

pub fn to_outcomes(checks: Vec<Check>, filter: Option<&str>) -> Vec<Outcome> {
    checks
        .into_iter()
        .filter(|c| filter.is_none_or(|f| c.name.contains(f)))
        .map(|c| {
            let time = Some(Duration::from_secs_f64(c.ms.max(0.0) / 1000.0));
            if c.ok { Outcome::pass(Suite::Ui, &c.name, time) } else { Outcome::fail(Suite::Ui, &c.name, time, &c.message) }
        })
        .collect()
}

fn read_report(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().filter(|text| !text.trim().is_empty())
}

/// Launches the hidden app in checks mode and returns what it reports. The audit of the launch is part of the result:
/// a leaked process or an outside write fails the suite even if every check passed.
pub fn run(paths: &Paths, filter: Option<&str>) -> Vec<Outcome> {
    let started = Instant::now();
    let mut ctx = match Ctx::new(paths) {
        Ok(ctx) => ctx,
        Err(e) => return vec![Outcome::fail(Suite::Ui, "launch", None, &e)],
    };
    let out = ctx.sb.sandbox.join("checks.json");
    // The `ui` suite never lets the engine look for a CLI: it cannot redirect `/usr/local/bin` on macOS or Linux.
    let launch = Launch { claude: CliSlot::Off, codex: CliSlot::Off, checks_out: Some(out.clone()), ..Launch::default() };
    let mut result = ctx.spawn(launch).and_then(|id| {
        let timeout = startup();
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(text) = read_report(&out) {
                return parse_report(&text);
            }
            if let Some(status) = ctx.exited(id) {
                // It may have written the report just before it exited.
                return match read_report(&out) {
                    Some(text) => parse_report(&text),
                    None => Err(format!("the app exited without a checks report ({status}). {}", ctx.output(id))),
                };
            }
            if Instant::now() >= deadline {
                return Err(format!("no checks report after {} s. {}", timeout.as_secs(), ctx.output(id)));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    if let Err(message) = &result {
        let log = paths.shots.join("ui-launch.log");
        if let Err(e) = std::fs::create_dir_all(&paths.shots).and_then(|()| std::fs::write(&log, message)) {
            result = Err(format!("{message}\ncannot write {}: {e}", log.display()));
        }
    }
    let audit = ctx.finish();
    let mut outcomes = match result {
        Ok(checks) if checks.is_empty() => vec![Outcome::fail(Suite::Ui, "report", None, "the page reported no checks")],
        Ok(checks) => to_outcomes(checks, filter),
        Err(e) => vec![Outcome::fail(Suite::Ui, "launch", Some(started.elapsed()), &e)],
    };
    if let Err(e) = audit {
        outcomes.push(Outcome::fail(Suite::Ui, "audit of the launch", None, &e));
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::Status;

    #[test]
    fn the_report_lists_each_check_with_its_verdict_message_and_time() {
        let checks =
            parse_report(r#"[{"name":"i18n: keys","ok":true,"message":"","ms":3.5},{"name":"models","ok":false,"message":"bad","ms":1}]"#)
                .unwrap();
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0], Check { name: "i18n: keys".into(), ok: true, message: String::new(), ms: 3.5 });
        assert!(!checks[1].ok && checks[1].message == "bad");
    }

    #[test]
    fn a_report_that_is_not_a_list_of_checks_is_an_error_and_never_a_pass() {
        for bad in ["", "not json", "{}", r#"[{"ok":true}]"#, r#"[{"name":"a"}]"#] {
            assert!(parse_report(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_filter_keeps_only_checks_whose_name_contains_the_text() {
        let checks =
            parse_report(r#"[{"name":"i18n: keys","ok":true,"ms":1},{"name":"models: ids","ok":false,"message":"x","ms":1}]"#).unwrap();
        let all = to_outcomes(checks.clone(), None);
        assert_eq!(all.len(), 2);
        let some = to_outcomes(checks, Some("models"));
        assert_eq!((some.len(), some[0].status), (1, Status::Fail));
        assert_eq!(some[0].suite, Suite::Ui);
    }
}
