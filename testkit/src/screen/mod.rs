//! The `screen` suite: scenarios that drive a hidden test build of Faber through the DevTools port. Windows only, because
//! only WebView2 has the port. Each scenario gets its own sandbox, and is audited when it ends.

pub mod app;
#[cfg(windows)]
pub mod containment;
pub mod first_run;
#[cfg(windows)]
pub mod local_update;
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
use crate::plan::Suite;
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
        .chain(projects_ui::scenarios())
        .chain(remote_git::scenarios())
        .chain(shell::scenarios())
        .chain(review::scenarios())
        .chain(settings::scenarios())
        .chain(setup::scenarios())
        .chain(first_run::scenarios())
        .chain(sentinel::scenarios());
    #[cfg(windows)]
    let all = all.chain(containment::scenarios());
    #[cfg(windows)]
    let all = all.chain(project_drivers::scenarios());
    #[cfg(windows)]
    let all = all.chain(local_update::scenarios());
    all.collect()
}

/// Runs each scenario (those whose name contains `filter`, if there is one) and audits it. A scenario fails if its own
/// checks fail or if its audit finds a leaked process, an outside write or a changed environment. `done` hears of each
/// outcome as soon as its scenario ends, so the runner can print it while the next one runs.
pub fn run_all(paths: &Paths, filter: Option<&str>, done: &mut dyn FnMut(&Outcome)) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    for scenario in all().iter().filter(|s| filter.is_none_or(|f| s.name.contains(f))) {
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
