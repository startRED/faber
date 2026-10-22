//! Standing goals (phase 1 of the owner's "Standing goals", approved 2026-10-22): things a project keeps true, such as "every
//! check passes". A goal is the person's words and one of the project's confirmed checks (`checks`); Faber runs the check
//! now and then, and when it breaks twice it starts a conversation to fix it (`engine/src/engine/goals.rs`).
//!
//! The goals live in `projects.json` under `extra.goals` and never in the project folder, where an agent writes. This file
//! holds the data and the rules that need no clock or process: when a goal is due, what a failure is called, what a result
//! means, the day's spending. Nothing here runs anything.
use super::Extra;
use crate::model::Project;
use crate::verify::{Outcome, Verdict};
use serde::{Deserialize, Serialize};

/// The project's key in `projects.json`.
pub const KEY: &str = "goals";
/// What fixes may cost a project in a day, in US$ at API price (the owner, 2026-10-22). Checked before a start: a turn that
/// is running is never cut.
pub const DAILY_CAP_USD: f64 = 2.0;
/// Conversations Faber starts for one project in a day.
pub const STARTS_PER_DAY: u32 = 2;
pub const MAX_GOALS: usize = 12;
pub const WORDS_MAX: usize = 160;
/// A goal is first checked this long after Faber starts (seconds), and again every `EVERY`.
pub const FIRST_AFTER: f64 = 300.0;
pub const EVERY: f64 = 6.0 * 3600.0;
/// A failure counts as broken when it fails again this long after the first time.
pub const RECHECK: f64 = 600.0;
/// Nothing from the person in the project for this long before a goal is checked.
pub const QUIET: f64 = 900.0;
/// Lines of the failing check the fixing conversation is given.
pub const BRIEF_LINES: usize = 20;

/// Where a goal stands. Checking and being fixed are not stored: the first is a check under way, the second a broken goal
/// whose conversation works.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    #[default]
    Holding,
    Broke,
    NeedsYou,
    Paused,
}

/// A failure seen once, waiting to be seen again `RECHECK` later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Failure {
    pub sig: String,
    pub at: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub id: String,
    /// The person's words: "no crashes".
    pub words: String,
    /// One of the project's confirmed checks, exactly as confirmed. Never free text.
    pub check: String,
    #[serde(default)]
    pub state: State,
    /// When the check last gave an answer, in Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<Failure>,
    /// The failure a conversation was started for: the same failure after it ended is not given a second one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub woke: Option<String>,
    /// The conversation fixing it (or the last one that did).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    /// Why it needs the person, as a `reason.goal.*` key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// The day's spending on fixes, by the PC's local day number.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Day {
    pub n: i64,
    #[serde(default)]
    pub usd: f64,
    /// Conversations started on that day.
    #[serde(default)]
    pub started: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goals {
    /// The person's switch for this project (Settings > Projects). Off until they turn it on.
    #[serde(default)]
    pub auto: bool,
    /// The person saw the daily limit when they first turned it on.
    #[serde(default)]
    pub cap_seen: bool,
    #[serde(default)]
    pub items: Vec<Goal>,
    #[serde(default)]
    pub day: Day,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Goals {
    /// The project's goals: none for a project that never had any, and an error key for a record that is there but cannot be
    /// read (damaged, or written by a newer Faber), so a change never writes an empty list over it.
    pub fn read(project: &Project) -> Result<Goals, &'static str> {
        match project.extra.get(KEY) {
            None => Ok(Goals::default()),
            Some(v) => serde_json::from_value(v.clone()).map_err(|_| "error.store.corrupt"),
        }
    }

    /// The goals for a look that never writes: a record that cannot be read counts as none.
    pub fn of(project: &Project) -> Goals {
        Goals::read(project).unwrap_or_default()
    }

    /// Writes them back into the project, leaving every other field as it was; an empty record writes nothing.
    pub fn store(&self, project: &mut Project) {
        if *self == Goals::default() {
            project.extra.remove(KEY);
        } else if let Ok(value) = serde_json::to_value(self) {
            project.extra.insert(KEY.into(), value);
        }
    }

    /// Adds a goal. The check must be one the person confirmed for the project, word for word.
    pub fn add(&mut self, id: String, words: &str, check: &str, confirmed: &[String]) -> Result<(), &'static str> {
        let words: String = words.chars().filter(|c| !c.is_control()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
        let words: String = words.chars().take(WORDS_MAX).collect();
        if words.is_empty() {
            return Err("reason.goal.words");
        }
        if !confirmed.iter().any(|c| c == check) {
            return Err("reason.goal.check");
        }
        if self.items.len() >= MAX_GOALS {
            return Err("reason.goal.many");
        }
        self.items.push(Goal {
            id,
            words,
            check: check.into(),
            state: State::Holding,
            last_run: None,
            failed: None,
            woke: None,
            conversation: None,
            reason: None,
            extra: Extra::new(),
        });
        Ok(())
    }

    /// What the day `today` (the PC's day number) holds so far. Only a later day starts over: a clock set back keeps the
    /// stored day, so moving it never gives the cap back.
    pub fn spent(&self, today: i64) -> Day {
        if today > self.day.n { Day { n: today, usd: 0.0, started: 0 } } else { self.day.clone() }
    }

    /// Whether the day's spending reached the cap: after that no turn Faber starts by itself and no conversation an agent
    /// starts goes on in a fix (a turn already running is never cut).
    pub fn over_cap(&self, today: i64, unpaid: f64) -> bool {
        self.spent(today).usd + unpaid >= DAILY_CAP_USD
    }

    /// Whether a fix may start on `today`: under the day's spending cap and the day's starts.
    pub fn room(&self, today: i64, unpaid: f64) -> Result<(), &'static str> {
        if self.over_cap(today, unpaid) {
            Err("reason.goal.cap")
        } else if self.spent(today).started >= STARTS_PER_DAY {
            Err("reason.goal.daily")
        } else {
            Ok(())
        }
    }

    /// Today's bucket, started over when a later day came.
    pub fn today(&mut self, today: i64) -> &mut Day {
        if today > self.day.n {
            self.day = Day { n: today, usd: 0.0, started: 0 };
        }
        &mut self.day
    }

    pub fn goal_of(&self, conversation: &str) -> Option<&Goal> {
        self.items.iter().find(|g| g.conversation.as_deref() == Some(conversation))
    }

    pub fn any_on(&self) -> bool {
        self.auto && !self.items.is_empty()
    }
}

/// When a goal is due, in Unix seconds: never before `FIRST_AFTER` after Faber started. A goal whose fix just ended
/// (`fixed`) is checked at once, to see whether it holds; one that failed once, also after a fix, is checked again after `RECHECK`.
pub fn due_at(g: &Goal, started: f64, fixed: bool) -> f64 {
    let at = match (&g.failed, g.last_run) {
        (Some(f), _) => f.at + RECHECK,
        _ if fixed => 0.0,
        (None, Some(last)) => last + EVERY,
        (None, None) => 0.0,
    };
    at.max(started + FIRST_AFTER)
}

/// What one run of the check said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Pass,
    /// Failed, with the failure's signature.
    Failed(String),
}

/// What a result makes of the goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Holding,
    /// The first failure: look again after `RECHECK`.
    Wait,
    /// Broke: a conversation should start.
    Start,
    NeedsYou(&'static str),
    /// Needs the person already and still fails: nothing changes and nobody is told again.
    Stay,
}

/// A failure counts after two runs `RECHECK` apart that show the same failure; a different one replaces the first and waits
/// again. A goal that needs the person starts nothing while it still fails (it holds again when the check passes, or when the
/// person watches it again). The failure a conversation was started for, seen again after that conversation ended, needs the
/// person: the fix is waiting for them, or Faber could not reproduce it. Any other failure right after a fix is a new one and
/// waits for a second run like the first.
pub fn next(g: &Goal, check: &Check, now: f64) -> Next {
    match check {
        Check::Pass => Next::Holding,
        Check::Failed(_) if g.state == State::NeedsYou => Next::Stay,
        Check::Failed(sig) if g.woke.as_deref() == Some(sig) => Next::NeedsYou("reason.goal.fixTried"),
        Check::Failed(sig) if g.failed.as_ref().is_some_and(|f| f.sig == *sig && now - f.at >= RECHECK) => Next::Start,
        Check::Failed(_) => Next::Wait,
    }
}

/// What a failed run is called, and the first lines of it for the conversation's brief: the failed checks' commands,
/// summaries and short failures. The name drops digits and spacing, so a line number, a timing or a temporary path that
/// changes between runs is still the same failure. `None` when no check failed outright (one that could not start, or was
/// cut short, says nothing about the code).
pub fn signature(outcome: &Outcome) -> Option<(String, String)> {
    let mut text = String::new();
    for check in outcome.checks.iter().filter(|c| c.verdict == Verdict::Failed) {
        text.push_str(&format!("{}\n{}\n", check.command, check.summary));
        check.failures.iter().for_each(|f| text.push_str(f));
        text.push('\n');
    }
    let lines: Vec<String> =
        text.lines().map(str::trim_end).filter(|l| !l.is_empty()).take(BRIEF_LINES).map(|l| l.chars().take(300).collect()).collect();
    if lines.is_empty() {
        return None;
    }
    let shown = lines.join("\n");
    let mut name = String::new();
    for c in shown.to_lowercase().chars() {
        match c {
            _ if c.is_ascii_digit() => {
                if !name.ends_with('#') {
                    name.push('#')
                }
            }
            _ if c.is_whitespace() => {
                if !name.ends_with(' ') {
                    name.push(' ')
                }
            }
            _ => name.push(c),
        }
    }
    Some((crate::project::lease::hash(name.as_bytes())[..16].to_string(), shown))
}

/// What the screen shows of one goal.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub words: String,
    pub check: String,
    /// `holding`, `checking`, `fixing`, `needsYou` or `paused`.
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Unix seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
}

/// A project's goals as Settings and the project page show them.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub auto: bool,
    pub cap_seen: bool,
    pub cap_usd: f64,
    /// Spent on fixes today, so far.
    pub spent_usd: f64,
    /// The project's confirmed checks a goal can use; none while the person has not confirmed the list.
    pub checks: Vec<String>,
    pub goals: Vec<Row>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::{CheckResult, Level};
    use serde_json::json;

    fn goal(id: &str) -> Goal {
        let mut goals = Goals::default();
        goals.add(id.into(), "no crashes", "cargo test", &["cargo test".into()]).unwrap();
        goals.items.remove(0)
    }

    fn failed(command: &str, summary: &str, failures: &[&str]) -> Outcome {
        Outcome {
            level: Level::Full,
            checks: vec![CheckResult {
                command: command.into(),
                verdict: Verdict::Failed,
                millis: 1,
                failures: failures.iter().map(|f| f.to_string()).collect(),
                summary: summary.into(),
            }],
            millis: 1,
            log: None,
            reason: None,
        }
    }

    #[test]
    fn only_a_confirmed_check_makes_a_goal_and_the_words_are_tidied() {
        let mut goals = Goals::default();
        let confirmed = ["cargo test".to_string()];
        assert_eq!(goals.add("a".into(), "no crashes", "rm -rf /", &confirmed), Err("reason.goal.check"), "never free text");
        assert_eq!(goals.add("a".into(), "   \n ", "cargo test", &confirmed), Err("reason.goal.words"));
        assert_eq!(goals.add("a".into(), "no\n  crashes\u{7} ", "cargo test", &confirmed), Ok(()));
        assert_eq!(goals.items[0].words, "no crashes");
        assert_eq!(goals.add("b".into(), "x", "cargo test", &[]), Err("reason.goal.check"), "an unconfirmed list offers none");
        for i in 0..MAX_GOALS {
            let _ = goals.add(format!("g{i}"), "again", "cargo test", &confirmed);
        }
        assert_eq!(goals.items.len(), MAX_GOALS);
        assert_eq!(goals.add("z".into(), "one more", "cargo test", &confirmed), Err("reason.goal.many"));
    }

    #[test]
    fn a_goal_is_due_five_minutes_after_start_then_every_six_hours_and_a_failure_again_after_ten_minutes() {
        let mut g = goal("a");
        assert_eq!(due_at(&g, 1000.0, false), 1300.0, "first look five minutes after Faber starts");
        g.last_run = Some(1400.0);
        assert_eq!(due_at(&g, 1000.0, false), 1400.0 + EVERY);
        g.last_run = Some(-EVERY * 3.0);
        assert_eq!(due_at(&g, 1000.0, false), 1300.0, "an old reading still waits the five minutes after start");
        g.failed = Some(Failure { sig: "s".into(), at: 2000.0 });
        assert_eq!(due_at(&g, 1000.0, false), 2600.0, "a failure is looked at again ten minutes later");
        assert_eq!(due_at(&g, 1000.0, true), 2600.0, "a failure waiting for its second run is not checked early, even after a fix");
        g.failed = None;
        assert_eq!(due_at(&g, 1000.0, true), 1300.0, "a goal whose fix ended is checked at once");
    }

    #[test]
    fn two_failures_ten_minutes_apart_break_a_goal_and_the_same_failure_after_its_fix_needs_the_person() {
        let mut g = goal("a");
        let fail = |s: &str| Check::Failed(s.into());
        assert_eq!(next(&g, &Check::Pass, 0.0), Next::Holding);
        assert_eq!(next(&g, &fail("s1"), 100.0), Next::Wait, "the first failure only waits");
        g.failed = Some(Failure { sig: "s1".into(), at: 100.0 });
        assert_eq!(next(&g, &fail("s1"), 100.0 + RECHECK - 1.0), Next::Wait, "not yet ten minutes");
        assert_eq!(next(&g, &fail("s1"), 100.0 + RECHECK), Next::Start, "the second one breaks it");
        assert_eq!(next(&g, &fail("s2"), 100.0 + RECHECK), Next::Wait, "a different failure is not the same one twice: it waits again");
        g.state = State::NeedsYou;
        assert_eq!(next(&g, &fail("s1"), 100.0 + RECHECK), Next::Stay, "a goal that needs the person starts nothing");
        assert_eq!(next(&g, &Check::Pass, 0.0), Next::Holding, "and holds again when the check passes");
        g.state = State::Broke;
        g.woke = Some("s1".into());
        assert_eq!(next(&g, &fail("s1"), 9000.0), Next::NeedsYou("reason.goal.fixTried"));
        assert_eq!(
            next(&g, &fail("s2"), 9000.0),
            Next::Wait,
            "another failure after the fix is a new one, and waits for a second run like the first"
        );
        g.failed = Some(Failure { sig: "s2".into(), at: 9000.0 });
        assert_eq!(next(&g, &fail("s2"), 9000.0 + RECHECK), Next::Start, "and starts a fix when it shows again");
        assert_eq!(
            due_at(&g, 0.0, true),
            9000.0 + RECHECK,
            "a failure waiting for its second run is not looked at at once, even when the last fix ended"
        );
        assert_eq!(next(&g, &Check::Pass, 9000.0), Next::Holding);
    }

    #[test]
    fn a_failure_keeps_its_name_when_only_numbers_and_spacing_change() {
        let one = failed(
            "cargo test",
            "failed (exit 101): 2 errors",
            &["thread 'main' panicked at src/lib.rs:12:5\nassertion failed in 3.4 s\n"],
        );
        let two = failed(
            "cargo test",
            "failed (exit 101): 3 errors",
            &["thread 'main' panicked at src/lib.rs:40:9\nassertion   failed in 11.9 s\n"],
        );
        let other = failed("cargo test", "failed (exit 101): 2 errors", &["thread 'main' panicked at src/main.rs:1:1\nsomething else\n"]);
        let (a, shown) = signature(&one).unwrap();
        assert_eq!(a, signature(&two).unwrap().0);
        assert_ne!(a, signature(&other).unwrap().0);
        assert!(shown.starts_with("cargo test\nfailed (exit 101)"), "{shown}");
        let mut none = one.clone();
        none.checks[0].verdict = Verdict::NotFinished;
        assert_eq!(signature(&none), None, "a check that did not finish says nothing about the code");
        let long = failed("cargo test", "failed", &[&"x\n".repeat(60)]);
        assert_eq!(signature(&long).unwrap().1.lines().count(), BRIEF_LINES);
    }

    #[test]
    fn the_day_cap_and_the_two_starts_a_day_stop_a_fix_and_a_new_day_starts_over() {
        let mut goals = Goals::default();
        assert_eq!(goals.room(10, 0.0), Ok(()));
        goals.today(10).usd = DAILY_CAP_USD - 0.01;
        assert_eq!(goals.room(10, 0.0), Ok(()), "under the cap, a start is allowed");
        goals.today(10).usd += 0.02;
        assert_eq!(goals.room(10, 0.0), Err("reason.goal.cap"), "at or over it, none starts");
        assert_eq!(goals.room(11, 0.0), Ok(()), "the next day starts over");
        goals.today(11).started = STARTS_PER_DAY;
        assert_eq!(goals.room(11, 0.0), Err("reason.goal.daily"));
        assert_eq!((goals.day.usd, goals.day.started), (0.0, STARTS_PER_DAY));
        // A clock set back is still the stored day: it never gives the day's money or starts back.
        goals.today(11).usd = DAILY_CAP_USD;
        assert!(goals.over_cap(10, 0.0) && goals.over_cap(11, 0.0));
        assert_eq!(goals.room(10, 0.0), Err("reason.goal.cap"));
        goals.today(9).usd += 0.5;
        assert_eq!((goals.day.n, goals.day.usd), (11, DAILY_CAP_USD + 0.5), "spending goes on the stored day");
    }

    #[test]
    fn a_record_that_is_there_but_unreadable_is_an_error_and_never_an_empty_list() {
        let mut project: Project = serde_json::from_str(r#"{"id":"p3","name":"R","path":"/r","goals":{"auto":"yes"}}"#).unwrap();
        assert_eq!(Goals::read(&project), Err("error.store.corrupt"));
        assert_eq!(Goals::of(&project), Goals::default(), "a look that never writes may count it as none");
        project.extra.remove(KEY);
        assert_eq!(Goals::read(&project), Ok(Goals::default()), "no key is no goals");
        project.extra.insert(KEY.into(), json!({"items":[{"id":"g","words":"w","check":"c","state":"fromTheFuture"}]}));
        assert_eq!(Goals::read(&project), Err("error.store.corrupt"), "a state of a newer Faber is not guessed");
    }

    #[test]
    fn projects_json_keeps_the_goals_and_every_field_it_does_not_know() {
        let text = r#"{"id":"p1","name":"P","path":"/p","separateCopies":true,"future":{"a":1},
            "goals":{"auto":true,"capSeen":true,"later":7,"day":{"n":5,"usd":0.5,"started":1},
            "items":[{"id":"g1","words":"no crashes","check":"cargo test","state":"needsYou","lastRun":12.5,
              "failed":{"sig":"s","at":3.0},"woke":"s","conversation":"c1","reason":"reason.goal.fixTried","kept":"yes"}]}}"#;
        let project: Project = serde_json::from_str(text).unwrap();
        let mut goals = Goals::of(&project);
        assert!(goals.auto && goals.cap_seen && goals.items[0].state == State::NeedsYou);
        assert_eq!(goals.items[0].conversation.as_deref(), Some("c1"));
        goals.items[0].last_run = Some(99.0);
        let mut back = project.clone();
        goals.store(&mut back);
        let json = serde_json::to_value(&back).unwrap();
        assert_eq!(json["future"], json!({"a": 1}), "a field of a newer Faber stays");
        assert_eq!(json["goals"]["later"], 7);
        assert_eq!(json["goals"]["items"][0]["kept"], "yes");
        assert_eq!(json["goals"]["items"][0]["lastRun"], 99.0);
        assert_eq!(Goals::of(&back), goals, "what is stored reads back the same");
        // A project that never had goals gains no key, and one emptied loses it.
        let mut plain: Project = serde_json::from_str(r#"{"id":"p2","name":"Q","path":"/q"}"#).unwrap();
        Goals::default().store(&mut plain);
        assert!(!plain.extra.contains_key(KEY));
        assert_eq!(Goals::of(&plain), Goals::default());
    }
}
