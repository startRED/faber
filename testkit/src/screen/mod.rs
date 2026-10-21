//! The `screen` suite: scenarios that drive a hidden test build of Faber through the DevTools port. Windows only, because
//! only WebView2 has the port. Each scenario gets its own sandbox, and is audited when it ends.

pub mod app;
pub mod approve_plan;
pub mod chat_groups;
#[cfg(windows)]
pub mod containment;
pub mod first_run;
pub mod homes;
pub mod mcp_servers;
pub mod menus;
pub mod permissions;
pub mod projects;
pub mod projects_ui;
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
use std::collections::HashMap;
use std::fmt::Debug;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
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
        .chain(approve_plan::scenarios())
        .chain(projects::scenarios())
        .chain(shell::scenarios())
        .chain(review::scenarios())
        .chain(settings::scenarios())
        .chain(setup::scenarios())
        .chain(first_run::scenarios())
        .chain(homes::scenarios())
        .chain(menus::scenarios())
        .chain(mcp_servers::scenarios())
        .chain(chat_groups::scenarios())
        .chain(sentinel::scenarios());
    #[cfg(windows)]
    let all = all.chain(containment::scenarios());
    #[cfg(windows)]
    let all = all.chain(project_drivers::scenarios());
    all.collect()
}

/// Seconds each scenario takes on CI, which `--shard` deals the parts by: the middle of the newest green runs, read from
/// their logs by `scripts/screen-costs.mjs` (R165). CI hands its parts that day's seconds (`--costs`); the copy in
/// `testkit/screen-costs.json` is what a run deals by without them. A scenario missing from the table counts as 5 s.
pub struct Costs(HashMap<String, u32>);

impl Costs {
    pub fn parse(text: &str) -> Result<Costs> {
        #[derive(serde::Deserialize)]
        struct Table {
            seconds: HashMap<String, f64>,
        }
        let table: Table = serde_json::from_str(text).map_err(|e| format!("the table of scenario seconds is not valid: {e}"))?;
        Ok(Costs(table.seconds.into_iter().map(|(name, seconds)| (name, seconds.ceil().max(1.0) as u32)).collect()))
    }

    pub fn committed() -> Costs {
        Costs::parse(include_str!("../../screen-costs.json")).expect("testkit/screen-costs.json is valid")
    }

    /// Every part of a CI run must deal by the same table, so a file that cannot be read stops the run instead of
    /// falling back to another one.
    pub fn read(path: &std::path::Path) -> Result<Costs> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        Costs::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    fn of(&self, name: &str) -> u32 {
        self.0.get(name).copied().unwrap_or(5)
    }
}

/// The scenarios whose result depends on the machine being quiet, with the reason. They run one at a time, after the
/// parallel batch, with nothing else running. The rule: a scenario that reads the processor or memory of the app, that
/// counts frames or times a motion, that depends on which window is shown or focused, that waits for the quota (read by a
/// background process at idle priority, which a busy processor starves), or that uses a resource the whole PC shares (the
/// clipboard) is listed here. A scenario that only checks what the page shows is not, and goes to a worker.
/// When unsure, list it: a wrong entry costs a few seconds, a missing one costs a flaky run. A test fails when a name says
/// "memory", "processor", "CPU", "MB", "budget", "frame", "focused", "quota" or "usage limits" and is not listed.
pub const SOLO: &[(&str, &str)] = &[
    ("chat groups: ten knowledge files survive reopening within the memory budget", "processor, memory"),
    ("with no CLI running, Faber stays within its memory budget", "memory"),
    ("with reduced motion on, the dust stays off and Faber stays within its memory budget", "memory"),
    (
        "a new project session shows the black hole, which moves only in a shown and focused window, within the memory budget",
        "processor, memory, window focus",
    ),
    ("after the six looks were applied in turn, Faber stays within its memory budget", "memory"),
    ("a conversation with a thousand messages opens quickly and stays within the budget", "time to open, memory"),
    ("conversation changes: thirty 3990-line files stay within the memory budget while switching and after closing", "processor, memory"),
    ("visual answers: one open frame stays within the memory budget while switching versions and after closing", "processor, memory"),
    ("visual answers: pane memory before opening, while open and after closing", "memory"),
    ("background commands: thirty-two capped outputs open in the column and one stops without a message", "processor, memory"),
    ("homes: idle life costs the processor little, and nothing while the window is hidden", "processor, window state"),
    ("homes: the Chat home and the Projects home look alive, and stand still with reduced motion", "animations running, window state"),
    ("C5.4: the open guide and the look step, with no conversation, use at most 400 MB", "memory"),
    ("projects: a warm CLI runs straight under the app, with no guard process, and its memory is measured", "memory"),
    ("review 2: rapid column toggles on a large thread stay below 400 MB without stage clones", "memory"),
    ("review 12: natural column motion has intermediate frames without stretching text", "frame intervals"),
    ("review 5: retained layout motion stays within the frame budget on a large thread", "frame intervals"),
    ("pointer respects delayed finite layout motion before clicking", "motion timing"),
    ("sentinel: panels move through frames, and reduced motion moves nothing", "frame clock"),
    ("sentinel: a permission card arrives through frames", "frame clock"),
    ("the quota of each vendor shows in the footer, and comes back after the screen reloads", "quota reading at idle priority"),
    ("the quota in the sidebar opens Settings on the usage limits, and a reload shows the same figures", "quota reading at idle priority"),
    ("every section of Settings says its part: the accounts, the usage limits and what stays on this PC", "quota reading at idle priority"),
    ("right-click opens Faber's short menus on a chat and a text field, and nothing anywhere else", "the PC's one clipboard"),
    (
        "with Auto, the answer names its model as soon as it starts, keeps the name to the end and after reopening",
        "reads Auto's 30 s routing state, which ended early beside three other scenarios (4 workers, 2026-10-17)",
    ),
];

/// Phrases that look like a word of the rule above and are not about the machine: focus inside the page (`activeElement`),
/// which each page has on its own, and the money a mission may spend, which is a number in the engine.
#[cfg(test)]
const NOT_ABOUT_THE_MACHINE: &[&str] = &["a focused heading", "waits on the budget"];

/// Whether the scenario `name` runs alone, after the parallel batch.
pub fn is_solo(name: &str) -> bool {
    SOLO.iter().any(|(solo, _)| *solo == name)
}

/// How many scenarios run at once when `FABER_SCREEN_WORKERS` does not say. Measured on the maintainer's PC (docs/dev/testing.md,
/// "Screen tests"). On GitHub Actions each part already runs on a small machine with seven others beside it, so it stays at
/// one there unless the workflow sets the variable.
const DEFAULT_WORKERS: usize = 4;

/// The workers a run uses: `FABER_SCREEN_WORKERS` when it is a number above zero, else the default.
pub fn workers(setting: Option<&str>, actions: bool) -> usize {
    setting.and_then(|v| v.trim().parse::<usize>().ok()).filter(|n| *n > 0).unwrap_or(if actions { 1 } else { DEFAULT_WORKERS })
}

/// What runs when: the scenarios for the workers (the slowest first, so no long one starts last) and the ones that run
/// alone after them, each as an index into the chosen scenarios.
#[derive(Debug, PartialEq, Eq)]
struct Batches {
    parallel: Vec<usize>,
    alone: Vec<usize>,
}

/// Splits the chosen scenarios (`(seconds, solo)` each) into the two batches. `dealt` is the shard: its part, and the part
/// each scenario belongs to; a scenario of another part is in neither batch.
fn batches(chosen: &[(u32, bool)], dealt: Option<(usize, &[usize])>) -> Batches {
    let mine = |index: &usize| dealt.is_none_or(|(part, to)| to[*index] == part);
    let (alone, mut parallel): (Vec<usize>, Vec<usize>) = (0..chosen.len()).filter(mine).partition(|index| chosen[*index].1);
    parallel.sort_by_key(|index| (std::cmp::Reverse(chosen[*index].0), *index));
    Batches { parallel, alone }
}

/// Runs the batches: the first on `workers` threads, each taking the next scenario when it is free; then the second, one at
/// a time, on this thread. `done` hears of each result on this thread as soon as it is in. The results come back in the
/// order of the scenarios, whatever order they ended in; a scenario in neither batch has none.
fn execute<T: Send>(
    batches: &Batches,
    total: usize,
    workers: usize,
    run: &(dyn Fn(usize) -> T + Sync),
    done: &mut dyn FnMut(&T),
) -> Vec<Option<T>> {
    let mut results: Vec<Option<T>> = (0..total).map(|_| None).collect();
    let next = AtomicUsize::new(0);
    let (next, parallel) = (&next, &batches.parallel);
    let (send, receive) = std::sync::mpsc::channel::<(usize, T)>();
    std::thread::scope(|scope| {
        for _ in 0..workers.min(parallel.len()) {
            let send = send.clone();
            scope.spawn(move || {
                while let Some(&index) = parallel.get(next.fetch_add(1, Ordering::SeqCst)) {
                    if send.send((index, run(index))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        for (index, outcome) in receive {
            done(&outcome);
            results[index] = Some(outcome);
        }
    });
    for &index in &batches.alone {
        let outcome = run(index);
        done(&outcome);
        results[index] = Some(outcome);
    }
    results
}

/// Runs each scenario (those whose name contains `filter`, if there is one, and is one of `only`, if there is a list of exact
/// names) and audits it. A scenario fails if its own
/// checks fail or if its audit finds a leaked process, an outside write or a changed environment. `shard` keeps only the
/// scenarios dealt to one part. The scenarios that measure the machine run alone at the end (`SOLO`); the others run on
/// several workers at once (`workers`), each in a sandbox, on a DevTools port and in a process tree of its own. `done`
/// hears of each outcome as soon as its scenario ends, so the runner can print it while the next ones run; the list that
/// comes back keeps the order of the scenarios.
pub fn run_all(
    paths: &Paths,
    filter: Option<&str>,
    only: Option<&[String]>,
    shard: Option<(usize, usize)>,
    costs: &Costs,
    done: &mut dyn FnMut(&Outcome),
) -> Vec<Outcome> {
    let chosen: Vec<Scenario> = all()
        .into_iter()
        .filter(|s| filter.is_none_or(|f| s.name.contains(f)) && only.is_none_or(|names| names.iter().any(|n| n == s.name)))
        .collect();
    let seconds: Vec<u32> = chosen.iter().map(|s| costs.of(s.name)).collect();
    let dealt = shard.map(|(part, parts)| (part - 1, deal(&seconds, parts)));
    let table: Vec<(u32, bool)> = chosen.iter().zip(&seconds).map(|(s, seconds)| (*seconds, is_solo(s.name))).collect();
    let plan = batches(&table, dealt.as_ref().map(|(part, to)| (*part, to.as_slice())));
    let actions = std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true");
    let count = workers(std::env::var("FABER_SCREEN_WORKERS").ok().as_deref(), actions);
    let run = |index: usize| {
        let scenario = &chosen[index];
        // A panic outside the scenario's own call (the sandbox, the audit) fails that scenario, never the other workers.
        catch_unwind(AssertUnwindSafe(|| run_one(paths, scenario)))
            .unwrap_or_else(|_| Outcome::fail(Suite::Screen, scenario.name, None, "the runner panicked while running the scenario"))
    };
    execute(&plan, chosen.len(), count, &run, done).into_iter().flatten().collect()
}

fn run_one(paths: &Paths, scenario: &Scenario) -> Outcome {
    crate::timing::scenario_begins();
    let outcome = run_timed(paths, scenario);
    crate::timing::scenario_ends(scenario.name, outcome.time.unwrap_or_default());
    outcome
}

fn run_timed(paths: &Paths, scenario: &Scenario) -> Outcome {
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
    crate::timing::timed("pause", || std::thread::sleep(Duration::from_secs_f64(seconds)));
}

/// Waits until `check` is true, for up to `wait`, polling every 20 ms. For things outside the page, such as a file.
pub fn eventually(what: &str, wait: Duration, mut check: impl FnMut() -> bool) -> Result {
    crate::timing::timed("eventually", || {
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
    })
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
    fn every_scenario_in_the_committed_table_exists() {
        let names: Vec<&str> = all().iter().map(|s| s.name).collect();
        for name in Costs::committed().0.keys() {
            assert!(
                names.contains(&name.as_str()),
                "testkit/screen-costs.json names {name:?}, which is no scenario: run `node scripts/screen-costs.mjs fetch --write`, or rename it there"
            );
        }
    }

    #[test]
    fn the_table_rounds_seconds_up_and_a_scenario_it_lacks_counts_as_five() {
        let costs = Costs::parse(r#"{"about": "", "runs": [1], "seconds": {"slow one": 96.2, "quick one": 0.3}}"#).unwrap();
        assert_eq!(costs.of("slow one"), 97);
        assert_eq!(costs.of("quick one"), 1);
        assert_eq!(costs.of("new one"), 5);
        assert!(Costs::parse("{}").is_err(), "a table without seconds");
        assert!(Costs::read(std::path::Path::new("no/such/costs.json")).is_err(), "a missing file is never a silent fallback");
        assert!(!Costs::committed().0.is_empty());
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

    // Windows has every scenario; elsewhere the Windows-only ones are not in the list to match.
    #[cfg(windows)]
    #[test]
    fn every_solo_scenario_exists() {
        let names: Vec<&str> = all().iter().map(|s| s.name).collect();
        for (name, why) in SOLO {
            assert!(names.contains(name), "SOLO names {name:?} ({why}), which is no scenario: it was renamed, so it would run in parallel");
        }
    }

    #[test]
    fn a_scenario_that_says_it_measures_the_machine_runs_alone() {
        const WORDS: [&str; 9] = ["memory", "processor", "CPU", "MB", "budget", "frame", "focused", "quota", "usage limits"];
        for scenario in all() {
            let mut name = scenario.name.to_string();
            for phrase in NOT_ABOUT_THE_MACHINE {
                name = name.replace(phrase, "");
            }
            if let Some(word) = WORDS.iter().find(|w| name.contains(*w)) {
                assert!(is_solo(scenario.name), "{:?} says {word:?}, so it must be in SOLO (it measures the machine)", scenario.name);
            }
        }
        let mut names: Vec<&str> = SOLO.iter().map(|(name, _)| *name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), SOLO.len(), "a scenario is listed twice");
    }

    #[test]
    fn the_worker_count_is_the_setting_when_it_is_a_number_above_zero_and_else_the_default() {
        assert_eq!(workers(Some("2"), false), 2);
        assert_eq!(workers(Some(" 6 "), false), 6);
        assert_eq!(workers(Some("2"), true), 2, "a workflow can ask for more");
        for bad in [None, Some("0"), Some("-1"), Some("many"), Some("")] {
            assert_eq!(workers(bad, false), DEFAULT_WORKERS, "{bad:?}");
            assert_eq!(workers(bad, true), 1, "CI stays serial unless told: {bad:?}");
        }
    }

    #[test]
    fn the_slowest_scenario_starts_first_and_the_ones_that_measure_start_none() {
        // (seconds, solo)
        let chosen = [(5, false), (50, true), (30, false), (30, false), (9, true), (116, false)];
        let plan = batches(&chosen, None);
        assert_eq!(plan.parallel, [5, 2, 3, 0], "longest first; equal ones keep their order");
        assert_eq!(plan.alone, [1, 4], "alone: in the order of the scenarios");
    }

    #[test]
    fn a_shard_picks_the_scenarios_and_the_workers_run_what_it_picked() {
        let chosen = [(5, false), (50, true), (30, false), (30, false), (9, true), (116, false)];
        let to = [0, 1, 0, 1, 1, 0];
        let first = batches(&chosen, Some((0, &to)));
        let second = batches(&chosen, Some((1, &to)));
        assert_eq!((first.parallel.as_slice(), first.alone.as_slice()), (&[5, 2, 0][..], &[][..]));
        assert_eq!((second.parallel.as_slice(), second.alone.as_slice()), (&[3][..], &[1, 4][..]));
        let mut every: Vec<usize> = [first, second].iter().flat_map(|b| b.parallel.iter().chain(&b.alone)).copied().collect();
        every.sort();
        assert_eq!(every, [0, 1, 2, 3, 4, 5], "together the parts run each scenario once");
    }

    /// What a stand-in scenario saw: its index, how many ran at once with it, and how many had ended before it ended.
    type Seen = (usize, usize, usize);

    /// Runs `chosen` through `execute` with a stand-in for a scenario, and reports for each how many ran at once with it.
    fn trial(chosen: &[(u32, bool)], workers: usize) -> (Vec<Option<Seen>>, Vec<usize>, usize) {
        use std::sync::atomic::AtomicUsize;
        let (active, ended, widest) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0));
        let plan = batches(chosen, None);
        let run = |index: usize| {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            widest.fetch_max(now, Ordering::SeqCst);
            // A scenario that is further down the list takes longer, so they end out of order.
            std::thread::sleep(Duration::from_millis(5 * (chosen.len() - index) as u64));
            let before = ended.load(Ordering::SeqCst);
            active.fetch_sub(1, Ordering::SeqCst);
            ended.fetch_add(1, Ordering::SeqCst);
            (index, now, before)
        };
        let mut heard = Vec::new();
        let results = execute(&plan, chosen.len(), workers, &run, &mut |outcome: &Seen| heard.push(outcome.0));
        (results, heard, widest.load(Ordering::SeqCst))
    }

    #[test]
    fn at_most_the_workers_run_at_once_and_a_solo_scenario_runs_after_all_of_them_and_alone() {
        let chosen = [(5, false), (50, true), (30, false), (30, false), (9, true), (116, false), (2, false), (1, false)];
        let (results, heard, widest) = trial(&chosen, 3);
        assert_eq!(widest, 3, "three workers were busy at once");
        for (index, (_, solo)) in chosen.iter().enumerate().filter(|(_, (_, solo))| *solo) {
            let (at, together, ended_before) = results[index].unwrap();
            assert_eq!((at, solo), (index, &true));
            assert_eq!(together, 1, "scenario {index} had company");
            assert_eq!(
                ended_before,
                6 + [1, 4].iter().take_while(|s| **s != index).count(),
                "scenario {index} did not wait for the others"
            );
        }
        assert_eq!(heard.len(), chosen.len(), "done heard of each one");
        assert!(heard[6..].iter().eq([1, 4].iter()), "the solo ones are the last two to be heard: {heard:?}");
    }

    #[test]
    fn the_results_come_back_in_the_order_of_the_scenarios_whatever_order_they_ended_in() {
        let chosen = [(1, false), (2, false), (3, true), (4, false), (5, false)];
        let (results, heard, _) = trial(&chosen, 4);
        let order: Vec<usize> = results.iter().map(|r| r.unwrap().0).collect();
        assert_eq!(order, [0, 1, 2, 3, 4]);
        assert_ne!(heard, order, "they did end in another order, or the test proves nothing");
        let (one, _, widest) = trial(&chosen, 1);
        assert_eq!(widest, 1);
        assert_eq!(one.iter().map(|r| r.unwrap().0).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_scenario_in_another_shard_has_no_result() {
        let chosen = [(1, false), (2, false), (3, true)];
        let plan = batches(&chosen, Some((0, &[0, 1, 0])));
        let results = execute(&plan, 3, 2, &|index| index, &mut |_| {});
        assert_eq!(results, [Some(0), None, Some(2)]);
    }
}

#[cfg(windows)]
pub mod project_drivers;
