//! The `screen` suite: scenarios that drive a hidden test build of Faber through the DevTools port. Windows only, because
//! only WebView2 has the port. Each scenario gets its own sandbox, and is audited when it ends.

pub mod app;
#[cfg(windows)]
pub mod containment;
pub mod coordinator;
pub mod first_run;
pub mod homes;
#[cfg(windows)]
pub mod local_update;
pub mod menus;
pub mod permissions;
pub mod projects;
pub mod projects_ui;
pub mod remote_git;
pub mod review;
pub mod sentinel;
pub mod settings;
pub mod setup;
pub mod shell;

use crate::app::{Ctx, Paths};
use crate::page::{Page, css};
use crate::plan::{Suite, deal};
use crate::report::{Outcome, file_name};
use crate::sandbox::{CliSlot, Launch};
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

pub type Result<T = ()> = std::result::Result<T, String>;

pub struct Scenario {
    pub name: &'static str,
    pub run: fn(&mut Ctx) -> Result,
}

pub fn all() -> Vec<Scenario> {
    let all = app::scenarios()
        .into_iter()
        .chain(permissions::scenarios())
        .chain(projects::scenarios())
        .chain(coordinator::scenarios())
        .chain(coordinator::canary_scenarios())
        .chain(projects_ui::scenarios())
        .chain(remote_git::scenarios())
        .chain(shell::scenarios())
        .chain(review::scenarios())
        .chain(settings::scenarios())
        .chain(setup::scenarios())
        .chain(first_run::scenarios())
        .chain(homes::scenarios())
        .chain(menus::scenarios())
        .chain(sentinel::scenarios());
    #[cfg(windows)]
    let all = all.chain(containment::scenarios());
    #[cfg(windows)]
    let all = all.chain(project_drivers::scenarios());
    #[cfg(windows)]
    let all = all.chain(local_update::scenarios());
    all.collect()
}

/// Seconds every scenario of 8 s or more took on CI (run 38945820716, 2026-10-16), so `--shard` deals them evenly; any
/// other counts as 5 (the rest average 4.4 s). A scenario missing here deals as 5 however long it takes, which is how one part
/// came to take twice the others: when a scenario takes 8 s or more on CI, add it. A renamed one fails a test.
const SLOW: [(&str, u32); 41] = [
    ("coordinator: a mission lands", 166),
    ("T13 4: fixture promotion", 120),
    ("with Auto, the answer names", 52),
    ("homes: idle life costs the", 50),
    ("a new project session shows", 33),
    ("with no CLI running, Faber", 28),
    ("remote git: a repository", 27),
    ("sentinel: panels move through", 25),
    ("first run, keyboard only", 22),
    ("the running column opens", 16),
    ("sentinel: no text is garbled", 16),
    ("sentinel: the composer grows", 16),
    ("local_update: candidate starts", 16),
    ("first run, tutorial: an installed", 15),
    ("a click on a running conversation", 13),
    ("C5.4: the open guide and", 13),
    ("review 5: retained layout", 13),
    ("local_update: hidden session", 13),
    ("first run, tutorial: a remembered", 13),
    ("T13 5: independent helper", 13),
    ("with reduced motion on, the", 13),
    ("a chat thread with a tool", 12),
    ("first run, step 2: a preview", 12),
    ("a question that waits when", 12),
    ("first run, tutorial: a beginner", 11),
    ("local_update: autostart uses", 11),
    ("first run, tutorial: waits", 11),
    ("a settings save that fails", 11),
    ("C5.3: None persists, shows", 11),
    ("the layout follows the width", 11),
    ("homes: the Chat home and", 10),
    ("local_update: trusted launcher", 10),
    ("reconnecting an account saves", 10),
    ("an installed CLI that is", 10),
    ("C5.2: missing ChatGPT installs", 9),
    ("first run, step 2: three", 9),
    ("sentinel: a permission card", 9),
    ("every look fits the new conversation", 9),
    ("C5.3: Skip persists, shows", 8),
    ("projects git review 5 and", 8),
    ("homes: the project home reads", 8),
];

fn cost(name: &str) -> u32 {
    SLOW.iter().find(|(start, _)| name.starts_with(start)).map_or(5, |(_, secs)| *secs)
}

/// Runs each scenario (those whose name contains `filter`, if there is one) and audits it. A scenario fails if its own
/// checks fail or if its audit finds a leaked process, an outside write or a changed environment. `done` hears of each
/// outcome as soon as its scenario ends, so the runner can print it while the next one runs.
pub fn run_all(paths: &Paths, filter: Option<&str>, shard: Option<(usize, usize)>, done: &mut dyn FnMut(&Outcome)) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    let chosen: Vec<Scenario> = all().into_iter().filter(|s| filter.is_none_or(|f| s.name.contains(f))).collect();
    let costs: Vec<u32> = chosen.iter().map(|s| cost(s.name)).collect();
    let dealt = shard.map(|(part, parts)| (part - 1, deal(&costs, parts)));
    for (index, scenario) in chosen.iter().enumerate() {
        if dealt.as_ref().is_some_and(|(part, to)| to[index] != *part) {
            continue;
        }
        let outcome = run_one(paths, scenario);
        done(&outcome);
        outcomes.push(outcome);
    }
    outcomes
}

fn run_one(paths: &Paths, scenario: &Scenario) -> Outcome {
    let started = Instant::now();
    let mut ctx = match Ctx::new(paths) {
        Ok(ctx) => ctx,
        Err(e) => return Outcome::fail(Suite::Screen, scenario.name, None, &e),
    };
    let result = catch_unwind(AssertUnwindSafe(|| (scenario.run)(&mut ctx))).unwrap_or_else(|_| Err("the scenario panicked".to_string()));
    // The last frame is taken before the apps end, whatever the result: the audit runs after that and can fail a
    // scenario whose own checks passed. It is kept only when the scenario fails.
    let shot = paths.shots.join(format!("{}.png", file_name(scenario.name)));
    ctx.failure_shot(&shot);
    let audit = ctx.finish();
    if result.is_ok() && audit.is_ok() {
        let _ = std::fs::remove_file(&shot);
    }
    let time = Some(started.elapsed());
    match (result, audit) {
        (Ok(()), Ok(())) => Outcome::pass_with(Suite::Screen, scenario.name, time, &ctx.notes.join(", ")),
        (Err(e), Ok(())) => Outcome::fail(Suite::Screen, scenario.name, time, &e),
        (Ok(()), Err(a)) => Outcome::fail(Suite::Screen, scenario.name, time, &format!("audit: {a}")),
        (Err(e), Err(a)) => Outcome::fail(Suite::Screen, scenario.name, time, &format!("{e}\naudit: {a}")),
    }
}

// What the scenarios share.

/// A launch with the recorded Claude conversation `chat.jsonl` and no Codex.
pub fn claude_chat(ctx: &Ctx) -> Launch {
    Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/chat.jsonl")),
        ..Launch::default()
    }
}

/// A launch with no CLI at all.
pub fn no_cli() -> Launch {
    Launch::default()
}

/// The settings a scenario starts with: this language, and the first run already done: the accounts step, the look step
/// (the default look, chosen) and the tutorial.
pub fn start_settings(ctx: &Ctx, language: &str) -> Result {
    ctx.write_settings(&serde_json::json!({
        "language": language,
        "setup": "done",
        "appearance": { "base": "sala" },
        "tutorialDone": true
    }))
}

/// Step 2 of the first run, passed with its defaults: the screen shows its heading, and Continue leaves it.
pub fn pass_look(page: &mut Page) -> Result {
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    let go = page.named("button", "first.continue")?;
    page.click(&go)?;
    page.wait_gone(&heading)
}

/// Ends the tutorial the way a person in a hurry does, with Esc, and waits for the screen to be usable again.
pub fn skip_tour(page: &mut Page) -> Result {
    page.wait_visible(&css(".coach"))?;
    page.press("Escape")?;
    page.wait_gone(&css(".tour"))
}

pub fn ensure(condition: bool, message: impl FnOnce() -> String) -> Result {
    if condition { Ok(()) } else { Err(message()) }
}

pub fn same<T: PartialEq + Debug>(actual: T, expected: T, what: &str) -> Result {
    if actual == expected { Ok(()) } else { Err(format!("{what}: expected {expected:?}, got {actual:?}")) }
}

/// Whether `text` holds `prefix` followed by a version such as `1.2.3`.
pub fn has_version_after(text: &str, prefix: &str) -> bool {
    text.match_indices(prefix).any(|(at, _)| {
        let rest = &text[at + prefix.len()..];
        let numbers: Vec<&str> = rest.splitn(3, '.').collect();
        numbers.len() == 3
            && numbers[0..2].iter().all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            && numbers[2].bytes().next().is_some_and(|b| b.is_ascii_digit())
    })
}

pub fn ask(page: &mut Page, question: &str) -> Result {
    let composer = page.labelled("composer.message")?;
    page.fill(&composer, question)?;
    // Enter is ignored until the app has found a CLI, which takes longer when a program was just built and is run for the
    // first time; Send turns enabled once the message can go.
    let send = page.named("button", "composer.send")?;
    page.wait_enabled(&send)?;
    // A native click establishes input focus; DOM focus alone can precede the window's input frame.
    page.click(&composer)?;
    page.press_on(&composer, "Enter")?;
    page.wait_for("the composer to empty", "document.querySelector('.composer textarea')?.value === ''")
}

/// Makes the page report the system's motion preference, whatever the machine says: `true` is "reduce motion" (the Windows
/// setting that is off on a CI runner), `false` is "no preference". Scenarios that look at animations set it, so they mean
/// the same thing on every machine.
pub fn system_motion(page: &mut Page, reduce: bool) -> Result {
    let value = if reduce { "reduce" } else { "no-preference" };
    page.cdp
        .call("Emulation.setEmulatedMedia", serde_json::json!({ "features": [{ "name": "prefers-reduced-motion", "value": value }] }))
        .map(|_| ())
}

pub fn pause(seconds: f64) {
    std::thread::sleep(Duration::from_secs_f64(seconds));
}

/// Waits until `check` is true, for up to `wait`, polling every 20 ms. For things outside the page, such as a file.
pub fn eventually(what: &str, wait: Duration, mut check: impl FnMut() -> bool) -> Result {
    let deadline = Instant::now() + wait;
    loop {
        if check() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out after {} s waiting for {what}", wait.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_after_a_prefix_needs_three_numbers() {
        assert!(has_version_after("Claude · v2.1.0", "Claude · v"));
        assert!(has_version_after("x Claude · v10.20.30 more", "Claude · v"));
        assert!(!has_version_after("Claude · v2.1", "Claude · v"));
        assert!(!has_version_after("Claude · vx.y.z", "Claude · v"));
        assert!(!has_version_after("Codex · v1.2.3", "Claude · v"));
    }

    #[test]
    fn ensure_and_same_carry_their_message() {
        assert!(ensure(true, || unreachable!()).is_ok());
        assert_eq!(ensure(false, || "no".into()), Err("no".into()));
        assert!(same(1, 1, "n").is_ok());
        assert_eq!(same("a", "b", "letter"), Err("letter: expected \"b\", got \"a\"".into()));
    }

    // Windows has every scenario; elsewhere the Windows-only ones are not in the list to match.
    #[cfg(windows)]
    #[test]
    fn every_slow_entry_names_exactly_one_scenario() {
        let names: Vec<&str> = all().iter().map(|s| s.name).collect();
        for (start, _) in SLOW {
            let matches = names.iter().filter(|name| name.starts_with(start)).count();
            assert_eq!(matches, 1, "SLOW entry {start:?} matches {matches} scenarios: a renamed scenario deals as 5 s");
        }
    }

    #[test]
    fn eventually_gives_up_with_what_it_waited_for() {
        let mut calls = 0;
        assert!(
            eventually("three calls", Duration::from_secs(2), || {
                calls += 1;
                calls == 3
            })
            .is_ok()
        );
        let error = eventually("never", Duration::from_millis(60), || false).unwrap_err();
        assert!(error.contains("waiting for never"), "{error}");
    }

    #[test]
    fn every_scenario_has_a_unique_name() {
        let mut names: Vec<&str> = all().iter().map(|s| s.name).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert!(names.len() == count, "{count} scenarios, {} different names", names.len());
    }
}

#[cfg(windows)]
pub mod project_drivers;
