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

/// The most a run may take however alive it is: the progress note only ends a wait early, it never lengthens it forever.
const RUN_CEILING: Duration = Duration::from_secs(20 * 60);

/// Waits for the report, and for the page's progress notes in the meantime. Every note that differs from the last restarts the
/// `quiet` allowance, so a run that is slow but moving never times out, while a page stuck inside one check, or one that
/// never starts, fails after `quiet` and says where it stopped. The page's heartbeat (`BEAT`) never counts as progress: it
/// only shows, in the failure, whether the page itself was still running when the checks stopped.
struct Wait {
    quiet: Duration,
    ceiling: Instant,
    note: String,
    last: String,
    moved: Instant,
}

/// What `tests/ui/crash.ts` appends to the last note while the page is alive.
const BEAT: &str = " · page alive ";

impl Wait {
    fn new(now: Instant, quiet: Duration, ceiling: Duration) -> Wait {
        Wait { quiet, ceiling: now + ceiling, note: String::new(), last: String::new(), moved: now }
    }

    /// The failure message when the wait is over, nothing while the run may go on.
    fn over(&mut self, now: Instant, note: &str) -> Option<String> {
        let check = note.split(BEAT).next().unwrap_or_default();
        if check != self.note {
            self.note = check.to_string();
            self.moved = now;
        }
        self.last = note.to_string();
        let at = if self.last.is_empty() { "before its first check".to_string() } else { format!("at check {}", self.last) };
        if now >= self.ceiling {
            Some(format!("the checks did not finish within {} s ({at})", RUN_CEILING.as_secs()))
        } else if now.duration_since(self.moved) >= self.quiet {
            Some(format!("no checks report and no progress for {} s, stopped {at}", self.quiet.as_secs()))
        } else {
            None
        }
    }
}

/// Where a stalled app and its page were stuck (macOS only, where the page has stopped mid-run): the main thread's call
/// graph of the app and of the newest WebKit page process, as `sample` sees it in 2 s, at most 60 lines each.
#[cfg(target_os = "macos")]
fn stacks(pid: u32) -> String {
    let newest_page = std::process::Command::new("pgrep").args(["-n", "com.apple.WebKit.WebContent"]).output().ok();
    let page = newest_page.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|p| !p.is_empty());
    let mut out = String::new();
    for (name, pid) in [("app", Some(pid.to_string())), ("page", page)] {
        let Some(pid) = pid else { continue };
        let Ok(o) = std::process::Command::new("sample").args([pid.as_str(), "2"]).output() else { continue };
        let text = String::from_utf8_lossy(&o.stdout);
        let graph: Vec<&str> = text.lines().skip_while(|l| !l.starts_with("Call graph:")).take(60).collect();
        out.push_str(&format!("\n{name} {pid} main thread:\n{}", graph.join("\n")));
    }
    out
}

#[cfg(not(target_os = "macos"))]
fn stacks(_pid: u32) -> String {
    String::new()
}

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
        let progress = out.with_extension("progress");
        let mut wait = Wait::new(Instant::now(), startup(), RUN_CEILING);
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
            let note = std::fs::read_to_string(&progress).unwrap_or_default();
            if let Some(why) = wait.over(Instant::now(), note.trim()) {
                return Err(format!("{why}. {}{}", ctx.output(id), stacks(ctx.pid(id))));
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
    fn a_slow_run_that_keeps_moving_never_times_out_but_a_stuck_one_does() {
        let start = Instant::now();
        let at = |s: u64| start + Duration::from_secs(s);
        let mut wait = Wait::new(start, Duration::from_secs(90), Duration::from_secs(1200));
        assert_eq!(wait.over(at(89), ""), None);
        assert!(wait.over(at(90), "").unwrap().contains("before its first check"));
        let mut wait = Wait::new(start, Duration::from_secs(90), Duration::from_secs(1200));
        // 400 s in, a check every 60 s: alive, so no verdict.
        for (i, s) in (60..=420).step_by(60).enumerate() {
            assert_eq!(wait.over(at(s), &format!("{}/400 a check", i + 1)), None);
        }
        assert_eq!(wait.over(at(500), "7/400 a check"), None);
        let why = wait.over(at(510), "7/400 a check").unwrap();
        assert!(why.contains("no progress for 90 s") && why.contains("at check 7/400 a check"), "{why}");
        // The heartbeat is no progress, but the failure keeps it: the page was alive, its checks were not.
        let mut wait = Wait::new(start, Duration::from_secs(90), Duration::from_secs(1200));
        assert_eq!(wait.over(at(10), "3/400 a check"), None);
        assert_eq!(wait.over(at(60), "3/400 a check · page alive 50 s"), None);
        let why = wait.over(at(100), "3/400 a check · page alive 90 s").unwrap();
        assert!(why.contains("no progress for 90 s") && why.contains("3/400 a check · page alive 90 s"), "{why}");
        // Moving forever still ends at the ceiling.
        let mut wait = Wait::new(start, Duration::from_secs(90), Duration::from_secs(1200));
        assert!(wait.over(at(1200), "9/400 x").unwrap().contains("did not finish within 1200 s"));
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
