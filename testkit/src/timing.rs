//! Where a run's time goes: each phase (a build, a suite, a scenario, its launch and its audit) and each kind of small
//! step that repeats (a process snapshot, a wait on the page, a fixed pause). Every phase and every scenario's breakdown
//! goes to the run's log as a `time` line, the totals go to `target/faber-test/timing.json`, and the screen gets one line
//! (R154: a measurement). The numbers decide what to make faster; nothing here changes what a test proves.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How often a kind of step ran, how long it took in all, and its longest single run.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Tally {
    pub count: u32,
    pub total: Duration,
    pub longest: Duration,
}

impl Tally {
    fn add(&mut self, took: Duration) {
        self.count += 1;
        self.total += took;
        self.longest = self.longest.max(took);
    }
}

/// One phase that ended: its kind (`build`, `suite`, `scenario`, ...), its name and how long it took.
#[derive(Debug, Clone, PartialEq)]
pub struct Phase {
    pub kind: &'static str,
    pub name: String,
    pub took: Duration,
}

#[derive(Default)]
struct Run {
    steps: BTreeMap<&'static str, Tally>,
    phases: Vec<Phase>,
    /// Lines for the log that the runner has not written yet.
    pending: Vec<String>,
}

static RUN: Mutex<Run> = Mutex::new(Run { steps: BTreeMap::new(), phases: Vec::new(), pending: Vec::new() });

thread_local! {
    /// The steps of the scenario this thread runs, for its own breakdown.
    static SCENARIO: RefCell<Option<BTreeMap<&'static str, Tally>>> = const { RefCell::new(None) };
}

fn run() -> std::sync::MutexGuard<'static, Run> {
    RUN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Counts one step of `kind` that took `took`, for the run and for the scenario this thread runs, if any.
pub fn step(kind: &'static str, took: Duration) {
    run().steps.entry(kind).or_default().add(took);
    SCENARIO.with(|s| {
        if let Some(steps) = s.borrow_mut().as_mut() {
            steps.entry(kind).or_default().add(took);
        }
    });
}

/// Runs `f` and counts it as one step of `kind`.
pub fn timed<T>(kind: &'static str, f: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = f();
    step(kind, started.elapsed());
    value
}

/// A phase that is counted when it is dropped, so an early return is timed too.
pub struct Span {
    kind: &'static str,
    name: String,
    started: Instant,
}

/// Starts timing a phase.
pub fn span(kind: &'static str, name: &str) -> Span {
    Span { kind, name: name.to_string(), started: Instant::now() }
}

impl Drop for Span {
    fn drop(&mut self) {
        record(self.kind, &std::mem::take(&mut self.name), self.started.elapsed());
    }
}

/// Counts a phase that ended: `took` long, of `kind`, named `name`.
pub fn record(kind: &'static str, name: &str, took: Duration) {
    let mut run = run();
    run.pending.push(format!("time  {kind:<8}  {name} ({})", seconds(took)));
    run.phases.push(Phase { kind, name: name.to_string(), took });
    drop(run);
    // A phase inside a scenario is also one of its steps.
    SCENARIO.with(|s| {
        if let Some(steps) = s.borrow_mut().as_mut() {
            steps.entry(kind).or_default().add(took);
        }
    });
}

/// Starts the breakdown of a scenario on this thread.
pub fn scenario_begins() {
    SCENARIO.with(|s| *s.borrow_mut() = Some(BTreeMap::new()));
}

/// Ends the breakdown of the scenario on this thread and queues its `time` line for the log.
pub fn scenario_ends(name: &str, took: Duration) {
    let steps = SCENARIO.with(|s| s.borrow_mut().take()).unwrap_or_default();
    let mut run = run();
    run.pending.push(format!("time  scenario  {name} ({}): {}", seconds(took), breakdown(&steps)));
    run.phases.push(Phase { kind: "scenario", name: name.to_string(), took });
}

/// `launch 2×3.1 s, snapshot 14×1.6 s`: each kind with its count and total, the costliest first.
pub fn breakdown(steps: &BTreeMap<&'static str, Tally>) -> String {
    let mut list: Vec<(&&str, &Tally)> = steps.iter().collect();
    list.sort_by(|a, b| b.1.total.cmp(&a.1.total).then(a.0.cmp(b.0)));
    list.iter().map(|(kind, t)| format!("{kind} {}×{}", t.count, seconds(t.total))).collect::<Vec<_>>().join(", ")
}

/// The lines the log has not had yet.
pub fn drain() -> Vec<String> {
    std::mem::take(&mut run().pending)
}

fn seconds(time: Duration) -> String {
    if time < Duration::from_secs(1) { format!("{} ms", time.as_millis()) } else { format!("{:.1} s", time.as_secs_f64()) }
}

/// The run's totals: the whole run, each kind of phase and each kind of step, for the log and for `timing.json`.
pub fn totals(whole: Duration) -> (String, serde_json::Value) {
    let run = run();
    let mut kinds: BTreeMap<&str, Tally> = BTreeMap::new();
    for phase in &run.phases {
        kinds.entry(phase.kind).or_default().add(phase.took);
    }
    let shown: Vec<String> = ["build", "lint", "rust", "ui", "screen"]
        .iter()
        .filter_map(|k| kinds.get(k).map(|t| format!("{k} {}", seconds(t.total))))
        .collect();
    let line = format!("Time: {} in all{}{}", seconds(whole), if shown.is_empty() { "" } else { "; " }, shown.join(", "));
    let tally = |t: &Tally| serde_json::json!({ "count": t.count, "seconds": t.total.as_secs_f64(), "longest": t.longest.as_secs_f64() });
    let json = serde_json::json!({
        "seconds": whole.as_secs_f64(),
        "phases": run.phases.iter().map(|p| serde_json::json!({ "kind": p.kind, "name": p.name, "seconds": p.took.as_secs_f64() })).collect::<Vec<_>>(),
        "kinds": kinds.iter().map(|(k, t)| (k.to_string(), tally(t))).collect::<serde_json::Map<_, _>>(),
        "steps": run.steps.iter().map(|(k, t)| (k.to_string(), tally(t))).collect::<serde_json::Map<_, _>>(),
    });
    (line, json)
}

/// The step totals as a log line.
pub fn steps_line() -> String {
    format!("time  steps     {}", breakdown(&run().steps))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_breakdown_lists_the_costliest_kind_first_with_its_count() {
        let mut steps = BTreeMap::new();
        steps.insert("wait", Tally { count: 3, total: Duration::from_millis(400), longest: Duration::from_millis(200) });
        steps.insert("snapshot", Tally { count: 12, total: Duration::from_millis(1600), longest: Duration::from_millis(180) });
        assert_eq!(breakdown(&steps), "snapshot 12×1.6 s, wait 3×400 ms");
        assert_eq!(breakdown(&BTreeMap::new()), "");
    }

    #[test]
    fn a_scenario_counts_only_the_steps_of_its_own_thread_and_its_phases() {
        // Another thread's steps are not this scenario's, so scenarios running side by side keep their own numbers.
        std::thread::spawn(|| {
            scenario_begins();
            step("snapshot", Duration::from_millis(100));
            step("snapshot", Duration::from_millis(300));
            drop(Span { kind: "launch", name: "x".into(), started: Instant::now() - Duration::from_secs(2) });
            std::thread::spawn(|| step("snapshot", Duration::from_secs(9))).join().unwrap();
            let steps = SCENARIO.with(|s| s.borrow().clone()).unwrap();
            assert_eq!(steps["snapshot"].count, 2);
            assert_eq!(steps["snapshot"].longest, Duration::from_millis(300));
            assert_eq!(steps["launch"].count, 1);
            scenario_ends("timing test", Duration::from_secs(3));
            assert!(SCENARIO.with(|s| s.borrow().is_none()));
        })
        .join()
        .unwrap();
        let lines = drain();
        assert!(lines.iter().any(|l| l.starts_with("time  scenario  timing test (3.0 s): launch 1×2.0 s, snapshot 2×400 ms")), "{lines:?}");
        assert!(lines.iter().any(|l| l == "time  launch    x (2.0 s)"), "{lines:?}");
    }

    #[test]
    fn timed_returns_the_value_and_counts_the_step() {
        assert_eq!(timed("test-step", || 7), 7);
        assert!(run().steps["test-step"].count >= 1);
    }
}
