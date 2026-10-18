//! The results journal (the verifier spec's part 6): each check that ended, per project and command, with the content it
//! read (its input key), whether it passed and how long it took. It orders a run (failed first, then cheapest) and tells
//! a flaky check (one that passed and failed on the same content). One JSON line per result in the engine's data folder,
//! the last `KEPT` per check in memory; the file is rewritten from memory once it holds twice that. Idle it costs nothing:
//! it is read once, at the first run.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// The results kept per check.
pub const KEPT: usize = 50;
/// The checks kept: past it, the one with the oldest result goes.
const MAX_CHECKS: usize = 512;

/// One result of one check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The project folder the check belongs to (a copy's results count for its project).
    pub project: String,
    pub command: String,
    /// The content it read (`verify::input_key`); none when it could not be told.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub ok: bool,
    pub millis: u64,
    pub at: u64,
}

/// What the journal knows of one check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Known {
    /// Its last result failed.
    pub failed: bool,
    /// It passed and failed on the same content.
    pub flaky: bool,
    /// The median time of its passes, in milliseconds.
    pub cost: Option<u64>,
}

type Key = (String, String);

#[derive(Default)]
struct State {
    loaded: bool,
    checks: HashMap<Key, VecDeque<Entry>>,
    /// Lines in the file since it was last rewritten.
    lines: usize,
}

/// The journal: in memory only when it has no file (tests, a verifier with no data folder).
pub struct Journal {
    file: Option<PathBuf>,
    state: Mutex<State>,
}

impl Journal {
    pub fn new(file: Option<PathBuf>) -> Journal {
        Journal { file, state: Mutex::new(State::default()) }
    }

    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.loaded {
            state.loaded = true;
            // A line that cannot be read (a torn last write) is skipped: the rest still counts.
            let text = self.file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
            for entry in text.lines().filter_map(|l| serde_json::from_str::<Entry>(l).ok()) {
                state.lines += 1;
                keep(&mut state.checks, entry);
            }
        }
        f(&mut state)
    }

    /// What is known of `command` in `project`.
    pub fn known(&self, project: &str, command: &str) -> Known {
        self.with(|state| {
            let Some(entries) = state.checks.get(&(project.to_string(), command.to_string())) else { return Known::default() };
            let mut passes: Vec<u64> = entries.iter().filter(|e| e.ok).map(|e| e.millis).collect();
            passes.sort_unstable();
            let flaky = entries
                .iter()
                .filter(|e| e.ok)
                .any(|pass| pass.key.is_some() && entries.iter().any(|other| !other.ok && other.key == pass.key));
            Known { failed: entries.back().is_some_and(|e| !e.ok), flaky, cost: passes.get(passes.len() / 2).copied() }
        })
    }

    /// The content key of the last result of `command` (tests).
    #[cfg(test)]
    pub(crate) fn with_last_key(&self, project: &str, command: &str) -> Option<String> {
        self.with(|state| state.checks.get(&(project.to_string(), command.to_string()))?.back()?.key.clone())
    }

    /// Adds a result, in memory and on disk.
    pub fn add(&self, entry: Entry) {
        self.with(|state| {
            keep(&mut state.checks, entry.clone());
            let Some(file) = &self.file else { return };
            state.lines += 1;
            let kept: usize = state.checks.values().map(VecDeque::len).sum();
            let written = if state.lines > 2 * kept.max(KEPT) {
                // Rewritten from memory, oldest first, through a temporary file so a crash leaves the old one whole.
                let mut all: Vec<&Entry> = state.checks.values().flatten().collect();
                all.sort_by_key(|e| e.at);
                let text: String = all.iter().filter_map(|e| serde_json::to_string(e).ok()).map(|l| l + "\n").collect();
                let temporary = file.with_extension("jsonl.tmp");
                state.lines = all.len();
                std::fs::write(&temporary, text).and_then(|()| std::fs::rename(&temporary, file))
            } else {
                append(file, &entry)
            };
            // A journal that cannot be written loses history, never a check.
            drop(written);
        });
    }
}

fn append(file: &PathBuf, entry: &Entry) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)? + "\n";
    std::fs::OpenOptions::new().create(true).append(true).open(file)?.write_all(line.as_bytes())
}

fn keep(checks: &mut HashMap<Key, VecDeque<Entry>>, entry: Entry) {
    let key = (entry.project.clone(), entry.command.clone());
    if !checks.contains_key(&key) && checks.len() >= MAX_CHECKS {
        let oldest = checks.iter().min_by_key(|(_, e)| e.back().map_or(0, |e| e.at)).map(|(k, _)| k.clone());
        if let Some(oldest) = oldest {
            checks.remove(&oldest);
        }
    }
    let entries = checks.entry(key).or_default();
    entries.push_back(entry);
    while entries.len() > KEPT {
        entries.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(command: &str, key: &str, ok: bool, millis: u64, at: u64) -> Entry {
        Entry { project: "p".into(), command: command.into(), key: Some(key.into()), ok, millis, at }
    }

    #[test]
    fn it_tells_the_last_failure_a_flaky_check_and_the_median_cost() {
        let journal = Journal::new(None);
        assert_eq!(journal.known("p", "lint"), Known::default());
        for (n, millis) in [300, 100, 200].into_iter().enumerate() {
            journal.add(entry("lint", &format!("k{n}"), true, millis, n as u64));
        }
        assert_eq!(journal.known("p", "lint"), Known { failed: false, flaky: false, cost: Some(200) });
        journal.add(entry("lint", "k9", false, 50, 10));
        assert!(journal.known("p", "lint").failed && !journal.known("p", "lint").flaky, "a failure on new content");
        journal.add(entry("lint", "k1", false, 50, 11));
        assert!(journal.known("p", "lint").flaky, "it passed and failed on the same content");
        assert_eq!(journal.known("q", "lint"), Known::default(), "another project");
    }

    #[test]
    fn it_keeps_the_last_results_and_survives_a_restart_and_a_torn_line() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("verify").join("journal.jsonl");
        let journal = Journal::new(Some(file.clone()));
        for at in 0..(3 * KEPT as u64) {
            journal.add(entry("test", &format!("k{at}"), at % 2 == 0, at, at));
        }
        let lines = std::fs::read_to_string(&file).unwrap().lines().count();
        assert!(lines <= 2 * KEPT + 1, "the file is rewritten from memory: {lines} lines");
        std::fs::OpenOptions::new().append(true).open(&file).unwrap().write_all(b"{\"project\":\"p\",\"comm").unwrap();
        let again = Journal::new(Some(file));
        assert_eq!(again.known("p", "test").failed, journal.known("p", "test").failed);
        assert_eq!(again.with(|s| s.checks[&("p".to_string(), "test".to_string())].len()), KEPT);
    }
}
