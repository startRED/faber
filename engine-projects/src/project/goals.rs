//! Standing goals (phase 1 of the owner's "Standing goals", approved 2026-10-22): things a project keeps true, such as "every
//! check passes". A goal is the person's words and one of the project's confirmed checks (`checks`); Faber runs the check
//! now and then, and when it breaks twice it starts a conversation to fix it (`engine/src/engine/goals.rs`).
//!
//! The goals live in `projects.json` under `extra.goals` and never in the project folder, where an agent writes. This file
//! holds the data and the rules that need no clock or process: when a goal is due, what a failure is called, what a result
//! means, the day's spending. Nothing here runs anything.
//!
//! A goal has one of two sources. A check goal runs one of the project's confirmed checks. A crash goal (phase 2) breaks when
//! the installed Faber's crash file (`crate::crash`) gets a line it has not seen; only Faber's own repository offers it
//! ([`faber_repo`]), since that is the only project whose crashes that file records.
use super::Extra;
use crate::model::Project;
use crate::verify::{Outcome, Verdict};
use serde::{Deserialize, Serialize};
use std::path::Path;

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
/// Crash lines (the newest) the fixing conversation is given.
pub const BRIEF_CRASHES: usize = 10;

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
    /// One of the project's confirmed checks, exactly as confirmed. Never free text. Empty for a crash goal.
    pub check: String,
    /// Set for a crash goal: what it has seen of the crash file so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crashes: Option<CrashWatch>,
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

/// What a crash goal has seen of the installed Faber's crash file. Lines are told apart by their time, never by a place in
/// the file: the file drops its oldest half when it is full, so an offset would point at the wrong line afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrashWatch {
    /// Lines with a time after this (Unix milliseconds) are new. It starts at the time the goal was added and moves up when a
    /// fix is started for the lines that broke it; until then the same lines are seen again, as a failing check is.
    // shortcut: two lines written in the very same millisecond after a look, the first of them seen, lose the second; a
    // clock set back hides lines until it catches up. Upgrade when a crash file carries a counter.
    pub since: u64,
    /// The crashes a fix was started for (or that came back after one), with the build each came from: the installed app keeps
    /// its bug until it is updated, so the same crash from that build is no news. The newest `HANDLED_MAX`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed: Vec<Handled>,
}

/// One crash, as `crash_name` calls it, and the build that wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handled {
    pub sig: String,
    pub version: String,
    pub commit: String,
}

/// How many crashes a goal remembers having handled.
const HANDLED_MAX: usize = 50;

impl Handled {
    pub fn of(line: &crate::crash::Seen) -> Self {
        Handled { sig: crash_name(line), version: line.version.clone(), commit: line.commit.clone() }
    }
}

impl CrashWatch {
    /// The new lines told apart: `(to fix, came back)`. Only panics can be fixed: every unclean exit has the same name and
    /// nothing to reproduce. A panic already handled from the same build (or the same version) is no news, since that build
    /// cannot hold the fix; the same one from another build means the fix did not work.
    pub fn sort(&self, lines: Vec<crate::crash::Seen>) -> (Vec<crate::crash::Seen>, Vec<crate::crash::Seen>) {
        let (mut fresh, mut again) = (vec![], vec![]);
        for line in lines.into_iter().filter(|l| l.kind == "panic") {
            let name = crash_name(&line);
            let mut same = self.fixed.iter().filter(|h| h.sig == name).peekable();
            if same.peek().is_none() {
                fresh.push(line);
            } else if !same.any(|h| h.commit == line.commit || h.version == line.version) {
                again.push(line);
            }
        }
        (fresh, again)
    }

    /// The lines up to `newest` are seen. The mark never goes past `now_ms`: a line dated a little ahead must not hide the
    /// real ones that come before it.
    pub fn seen(&mut self, newest: u64, now_ms: u64) {
        self.since = self.since.max(newest.min(now_ms));
    }

    /// Notes crashes as handled, keeping the newest `HANDLED_MAX`.
    pub fn remember(&mut self, lines: &[Handled]) {
        for h in lines {
            if !self.fixed.contains(h) {
                self.fixed.push(h.clone());
            }
        }
        let extra = self.fixed.len().saturating_sub(HANDLED_MAX);
        self.fixed.drain(..extra);
    }
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

impl Goal {
    /// The fix could not be launched: the goal no longer points at it, and what it had seen of the crash file is as it was before
    /// the start (`before`), since nobody was given those lines: they are new again once the person resumes the goal.
    pub fn undo_start(&mut self, before: &Goal) {
        (self.conversation, self.woke) = (None, None);
        self.crashes.clone_from(&before.crashes);
    }
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
        let words = tidy(words)?;
        if !confirmed.iter().any(|c| c == check) {
            return Err("reason.goal.check");
        }
        self.push(id, words, check, None)
    }

    /// Adds a crash goal: lines older than `now_ms` are old. A project keeps one: two would start two fixes for the same lines.
    pub fn add_crashes(&mut self, id: String, words: &str, now_ms: u64) -> Result<(), &'static str> {
        let words = tidy(words)?;
        if self.items.iter().any(|g| g.crashes.is_some()) {
            return Err("reason.goal.crashesOne");
        }
        self.push(id, words, "", Some(CrashWatch { since: now_ms, ..CrashWatch::default() }))
    }

    fn push(&mut self, id: String, words: String, check: &str, crashes: Option<CrashWatch>) -> Result<(), &'static str> {
        if self.items.len() >= MAX_GOALS {
            return Err("reason.goal.many");
        }
        self.items.push(Goal {
            id,
            words,
            check: check.into(),
            crashes,
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

/// The person's words, tidied: one line of at most `WORDS_MAX` characters, never empty.
fn tidy(words: &str) -> Result<String, &'static str> {
    let words: String = words.chars().filter(|c| !c.is_control()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let words: String = words.chars().take(WORDS_MAX).collect();
    if words.is_empty() { Err("reason.goal.words") } else { Ok(words) }
}

/// Whether `folder` is Faber's own repository: its engine and shell crates are named `faber-engine` and `faber`. Two small
/// files read, nothing run. A copy of the repository passes too, which is the point; a folder that only imitates the names
/// gets a crash file of Faber's read for it, scrubbed, and only because the person added the goal.
pub fn faber_repo(folder: &Path) -> bool {
    let named = |file: &str, name: &str| {
        let Ok(text) = std::fs::File::open(folder.join(file)).and_then(|f| std::io::read_to_string(std::io::Read::take(f, 8192))) else {
            return false;
        };
        text.lines().take_while(|l| !l.starts_with("[dependencies]")).any(|l| l.trim().replace(' ', "") == format!("name=\"{name}\""))
    };
    named("engine/Cargo.toml", "faber-engine") && named("src-tauri/Cargo.toml", "faber")
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
    Some((name_of(&shown), shown))
}

/// A failure's name: the text without digits and spacing, hashed.
fn name_of(shown: &str) -> String {
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
    crate::project::lease::hash(name.as_bytes())[..16].to_string()
}

/// What makes one crash the same as another: kind, thread, message and place; never the time, version or build.
fn crash_text(l: &crate::crash::Seen) -> String {
    format!("{}|{}|{}|{}", l.kind, l.thread, l.message, l.location)
}

fn crash_name(l: &crate::crash::Seen) -> String {
    name_of(&crash_text(l))
}

/// What new crash lines are called, and the newest of them for the conversation's brief (`BRIEF_CRASHES` lines, oldest first,
/// with how many were left out). The name is that of the distinct crashes (kind, thread, message and place; never the time,
/// version or build), so the same crash seen twice is one failure, and the same one after a fix is told apart from a new one.
/// `None` when there is no line. The text is a file's, so the brief fences it as data.
pub fn crash_signature(lines: &[crate::crash::Seen]) -> Option<(String, String)> {
    if lines.is_empty() {
        return None;
    }
    let mut kinds: Vec<String> = lines.iter().map(crash_text).collect();
    kinds.sort();
    kinds.dedup();
    let line = |l: &crate::crash::Seen| {
        format!("{} | version {} | build {} | thread {} | {} | {}", l.kind, l.version, l.commit, l.thread, l.location, l.message)
    };
    let mut text: Vec<String> = lines.iter().rev().take(BRIEF_CRASHES).rev().map(line).collect();
    if lines.len() > BRIEF_CRASHES {
        text.insert(0, format!("({} older new lines not shown)", lines.len() - BRIEF_CRASHES));
    }
    Some((name_of(&kinds.join("\n")), text.join("\n")))
}

/// What the screen shows of one goal.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub words: String,
    pub check: String,
    /// A crash goal: it watches the installed Faber's crash file, and has no check.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub crashes: bool,
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
    /// A crash goal can be added: the project is Faber's own repository and the installed Faber's data folder is known.
    pub can_watch_crashes: bool,
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

    #[test]
    fn a_crash_goal_has_no_check_starts_at_the_time_it_was_added_and_counts_toward_the_twelve() {
        let mut goals = Goals::default();
        goals.add_crashes("c".into(), "  no\n new crashes ", 5_000).unwrap();
        assert_eq!((goals.items[0].words.as_str(), goals.items[0].check.as_str()), ("no new crashes", ""));
        assert_eq!(goals.items[0].crashes, Some(CrashWatch { since: 5_000, fixed: vec![] }));
        assert_eq!(goals.add_crashes("d".into(), "  ", 1), Err("reason.goal.words"));
        assert_eq!(goals.add_crashes("e".into(), "a second", 1), Err("reason.goal.crashesOne"), "one crash goal in a project");
        for i in 1..MAX_GOALS {
            goals.add(format!("g{i}"), "again", "cargo test", &["cargo test".into()]).unwrap();
        }
        let mut full = Goals { items: goals.items[1..].to_vec(), ..Goals::default() };
        full.add("x".into(), "again", "cargo test", &["cargo test".into()]).unwrap();
        assert_eq!(full.add_crashes("z".into(), "one more", 1), Err("reason.goal.many"), "a crash goal counts toward the twelve");
        let mut project: Project = serde_json::from_str(r#"{"id":"p","name":"P","path":"/p"}"#).unwrap();
        goals.store(&mut project);
        assert_eq!(Goals::of(&project).items[0].crashes, Some(CrashWatch { since: 5_000, fixed: vec![] }), "what is watched is kept");
    }

    #[test]
    fn crash_lines_are_named_by_the_crash_not_by_when_or_which_build() {
        let line = |at: u64, build: &str, message: &str| crate::crash::Seen {
            at,
            kind: "panic".into(),
            version: "0.1.0".into(),
            commit: build.into(),
            thread: "main".into(),
            message: message.into(),
            location: "a.rs:3:9".into(),
        };
        let (a, shown) = crash_signature(&[line(1, "aaaaaaaaaaaa", "index 7 out of range")]).unwrap();
        let (b, _) =
            crash_signature(&[line(9, "bbbbbbbbbbbb", "index 12 out of range"), line(10, "bbbbbbbbbbbb", "index 12 out of range")])
                .unwrap();
        let (c, _) = crash_signature(&[line(1, "aaaaaaaaaaaa", "something else")]).unwrap();
        assert_eq!(a, b, "the same crash on another build, seen twice, is one failure");
        assert_ne!(a, c);
        assert!(shown.contains("build aaaaaaaaaaaa") && shown.contains("index 7 out of range"), "{shown}");
        assert_eq!(crash_signature(&[]), None);
        let many: Vec<_> = (0..15).map(|n| line(n, "a", &format!("m{n}"))).collect();
        let (_, shown) = crash_signature(&many).unwrap();
        assert_eq!(shown.lines().count(), BRIEF_CRASHES + 1);
        assert!(shown.starts_with("(5 older new lines not shown)") && shown.contains("m14") && !shown.contains("| m4"), "{shown}");
    }

    #[test]
    fn a_crash_handled_is_news_only_from_a_newer_build_and_an_unclean_exit_is_never_one_to_fix() {
        let line = |at: u64, kind: &str, version: &str, build: &str, message: &str| crate::crash::Seen {
            at,
            kind: kind.into(),
            version: version.into(),
            commit: build.into(),
            thread: "main".into(),
            message: message.into(),
            location: "a.rs:3:9".into(),
        };
        let first = line(1, "panic", "0.1.0", "aaaaaaaaaaaa", "boom");
        let mut watch = CrashWatch::default();
        let (fresh, again) = watch.sort(vec![first.clone(), line(2, "unclean-exit", "0.1.0", "aaaaaaaaaaaa", "")]);
        assert_eq!((fresh.len(), again.len()), (1, 0), "an unclean exit starts nothing");
        watch.remember(&[Handled::of(&first)]);
        let same_build = line(3, "panic", "0.1.0", "aaaaaaaaaaaa", "boom");
        let same_version = line(4, "panic", "0.1.0", "bbbbbbbbbbbb", "boom");
        let newer = line(5, "panic", "0.1.1", "cccccccccccc", "boom");
        let other = line(6, "panic", "0.1.0", "aaaaaaaaaaaa", "another");
        let (fresh, again) = watch.sort(vec![same_build, same_version, newer.clone(), other.clone()]);
        assert_eq!((fresh, again), (vec![other], vec![newer.clone()]));
        watch.remember(&[Handled::of(&newer), Handled::of(&newer)]);
        assert_eq!(watch.fixed.len(), 2, "no duplicate");
        assert_eq!(watch.sort(vec![newer]), (vec![], vec![]), "once told, a build is no news again");
        let many: Vec<_> = (0..60).map(|n| Handled { sig: format!("s{n}"), version: "v".into(), commit: "c".into() }).collect();
        watch.remember(&many);
        assert_eq!((watch.fixed.len(), watch.fixed[0].sig.as_str(), watch.fixed[49].sig.as_str()), (HANDLED_MAX, "s10", "s59"));
        // The mark follows the newest line but never goes past now, and never back.
        let mut mark = CrashWatch::default();
        mark.seen(5_000, 1_000);
        assert_eq!(mark.since, 1_000);
        mark.seen(400, 9_000);
        assert_eq!(mark.since, 1_000);
        mark.seen(2_000, 9_000);
        assert_eq!(mark.since, 2_000);
    }

    #[test]
    fn a_fix_that_could_not_be_launched_gives_the_crash_lines_back() {
        let mut goals = Goals::default();
        goals.add_crashes("c".into(), "no new crashes", 5_000).unwrap();
        let before = goals.items[0].clone();
        let mut started = before.clone();
        (started.conversation, started.woke) = (Some("conv".into()), Some("sig".into()));
        started.crashes =
            Some(CrashWatch { since: 9_000, fixed: vec![Handled { sig: "s".into(), version: "v".into(), commit: "c".into() }] });
        started.undo_start(&before);
        assert_eq!(started, before, "the mark and what was handled are as before the start");
    }

    #[test]
    fn only_a_folder_with_fabers_engine_and_shell_crates_is_fabers_own_repository() {
        let dir = tempfile::tempdir().unwrap();
        let write = |file: &str, text: &str| {
            std::fs::create_dir_all(dir.path().join(file).parent().unwrap()).unwrap();
            std::fs::write(dir.path().join(file), text).unwrap();
        };
        assert!(!faber_repo(dir.path()));
        write("engine/Cargo.toml", "[package]\nname = \"faber-engine\"\n");
        assert!(!faber_repo(dir.path()), "the shell crate is missing");
        write("src-tauri/Cargo.toml", "[package]\nname = \"faber\"\n[dependencies]\n");
        assert!(faber_repo(dir.path()));
        write("src-tauri/Cargo.toml", "[package]\nname = \"other\"\n[dependencies]\nname = \"faber\"\n");
        assert!(!faber_repo(dir.path()), "a name outside the package table does not count");
    }
}
