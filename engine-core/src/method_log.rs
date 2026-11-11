//! Method 2, slice 6: a local count, per project, of how a result holds after the closing box said done. A line is an
//! event (`closed`: a done box, with the seconds from the person's last message; `corrected`: the person's next message
//! after a done box), never a word, a path or an id of the person's work. The lines sit in `method/<project id>.jsonl` in
//! the data folder (never in the person's repository), stay bounded, go with the project, and nothing sends them anywhere.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const DIR: &str = "method";
/// About 3,000 lines. Past it the oldest half is dropped, so the count follows the recent work.
const MAX: u64 = 100_000;
/// One lock for every write and every removal: two conversations of a project write from separate threads, and a rotation
/// that read an older file must not overwrite a line written meanwhile, nor a queued write bring back a removed log.
static LOCK: Mutex<()> = Mutex::new(());

/// The log of a project, or none for an id that is not a plain file name.
fn file(dir: &Path, project: &str) -> Option<PathBuf> {
    let plain = !project.is_empty() && project.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    plain.then(|| dir.join(DIR).join(format!("{project}.jsonl")))
}

/// Whether the project is still in the data folder's project list (`projects.json`).
fn listed(dir: &Path, project: &str) -> bool {
    let list = std::fs::read_to_string(dir.join("projects.json")).ok().and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok());
    list.is_some_and(|l| l.iter().any(|p| p["id"] == project))
}

/// One line of a project's log, for a project still in the list. The same event at the same time is written once: a turn
/// started again after a failure shows the same message. A failed write is ignored, so it can never break a turn.
pub fn note(dir: &Path, project: &str, at: u64, event: &str, secs: u64) {
    write_line(dir, project, at, event, secs, MAX);
}

fn write_line(dir: &Path, project: &str, at: u64, event: &str, secs: u64, max: u64) {
    let Some(path) = file(dir, project) else { return };
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !listed(dir, project) {
        return;
    }
    let line = json!({ "at": at, "event": event, "secs": secs }).to_string();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    // Any kept line, not only the last: another conversation of the project may have written in between.
    if text.lines().any(|l| l == line) {
        return;
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(dir));
    if text.len() as u64 >= max {
        let kept = text[text.len() / 2..].split_once('\n').map_or("", |(_, rest)| rest);
        if std::fs::write(&path, kept).is_err() {
            return;
        }
    }
    crate::store::append_bounded(&path, u64::MAX, &serde_json::from_str(&line).unwrap_or_default());
}

/// What the log of a project holds: the done boxes, the corrections after one, and the seconds the done boxes took.
/// shortcut: the seconds measure attempts, from the last message to each done box (also the ones later corrected), not
/// the request to its accepted close; pair a box with what follows it if the split ever matters.
#[derive(Debug, Default, PartialEq)]
pub struct Counts {
    pub closed: u32,
    pub corrected: u32,
    pub close_secs: u64,
}

/// A line that does not parse is skipped.
pub fn counts(dir: &Path, project: &str) -> Counts {
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let text = file(dir, project).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut out = Counts::default();
    for line in text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()) {
        match line.get("event").and_then(Value::as_str) {
            Some("closed") => {
                out.closed += 1;
                out.close_secs += line.get("secs").and_then(Value::as_u64).unwrap_or(0);
            }
            Some("corrected") => out.corrected += 1,
            _ => {}
        }
    }
    out
}

/// Removes the logs of the projects not in `kept`: a project taken out of the list takes its count with it.
pub fn forget(dir: &Path, kept: &[&str]) {
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Ok(entries) = std::fs::read_dir(dir.join(DIR)) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.strip_suffix(".jsonl").is_some_and(|id| !kept.contains(&id)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The data folder's project list holding these ids.
    fn listing(dir: &Path, ids: &[&str]) {
        let list: Vec<Value> = ids.iter().map(|id| json!({ "id": id, "name": "n", "path": "p" })).collect();
        std::fs::write(dir.join("projects.json"), serde_json::to_string(&list).unwrap()).unwrap();
    }

    #[test]
    fn the_counts_follow_the_lines_and_skip_what_does_not_parse() {
        let dir = tempfile::tempdir().unwrap();
        listing(dir.path(), &["p1", "p2"]);
        note(dir.path(), "p1", 1, "closed", 90);
        note(dir.path(), "p1", 2, "corrected", 0);
        note(dir.path(), "p1", 3, "closed", 30);
        note(dir.path(), "p2", 4, "closed", 5);
        let path = file(dir.path(), "p1").unwrap();
        std::fs::write(&path, format!("{}not json\n", std::fs::read_to_string(&path).unwrap())).unwrap();
        assert_eq!(counts(dir.path(), "p1"), Counts { closed: 2, corrected: 1, close_secs: 120 });
        assert_eq!(counts(dir.path(), "p2").closed, 1);
        assert_eq!(counts(dir.path(), "none"), Counts::default());
    }

    #[test]
    fn a_line_holds_only_the_event_the_seconds_and_the_time() {
        let dir = tempfile::tempdir().unwrap();
        listing(dir.path(), &["p1"]);
        note(dir.path(), "p1", 7, "closed", 12);
        let text = std::fs::read_to_string(file(dir.path(), "p1").unwrap()).unwrap();
        assert_eq!(serde_json::from_str::<Value>(text.trim()).unwrap(), json!({ "at": 7, "event": "closed", "secs": 12 }));
    }

    #[test]
    fn the_same_event_at_the_same_time_is_written_once() {
        let dir = tempfile::tempdir().unwrap();
        listing(dir.path(), &["p1"]);
        for _ in 0..3 {
            note(dir.path(), "p1", 7, "corrected", 0);
        }
        // Another conversation's line in between does not let the same message count again.
        note(dir.path(), "p1", 8, "closed", 5);
        note(dir.path(), "p1", 7, "corrected", 0);
        assert_eq!(counts(dir.path(), "p1").corrected, 1);
    }

    #[test]
    fn the_log_stays_bounded_and_drops_the_oldest_lines() {
        let dir = tempfile::tempdir().unwrap();
        listing(dir.path(), &["p1"]);
        for at in 0..5_000 {
            note(dir.path(), "p1", at, "closed", 1);
        }
        let path = file(dir.path(), "p1").unwrap();
        let len = std::fs::metadata(&path).unwrap().len();
        assert!(len < MAX + 100, "grew to {len}");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(text.lines().all(|l| serde_json::from_str::<Value>(l).is_ok()), "no half line is left");
        assert!(text.contains("\"at\":4999"), "the newest line stays");
        assert!(!text.contains("\"at\":0,"), "the oldest is gone");
    }

    /// Writers that rotate at the same time must not erase each other's lines. A rotation drops only the oldest lines, so
    /// what stays of each writer is an unbroken run of its lines; a gap means a rotation overwrote a line written meanwhile.
    #[test]
    fn writers_rotating_at_once_leave_no_gap_in_any_writers_lines() {
        for _ in 0..5 {
            let dir = tempfile::tempdir().unwrap();
            listing(dir.path(), &["p1"]);
            let writers: Vec<_> = (0..8u64)
                .map(|w| {
                    let dir = dir.path().to_path_buf();
                    // A limit of 6,000 bytes rotates every sixty lines or so, and keeps a run long enough to show a gap.
                    std::thread::spawn(move || (0..400).for_each(|n| write_line(&dir, "p1", 1_000 * (w + 1) + n, "closed", 1, 6_000)))
                })
                .collect();
            writers.into_iter().for_each(|w| w.join().unwrap());
            let text = std::fs::read_to_string(file(dir.path(), "p1").unwrap()).unwrap();
            for w in 1..=8u64 {
                let mut ours: Vec<u64> = text
                    .lines()
                    .filter_map(|l| serde_json::from_str::<Value>(l).ok()?["at"].as_u64())
                    .filter(|at| at / 1_000 == w)
                    .collect();
                ours.sort();
                assert!(ours.windows(2).all(|p| p[1] == p[0] + 1), "writer {w} has a gap: {ours:?}");
            }
        }
    }

    #[test]
    fn a_name_that_is_not_a_plain_id_writes_nothing_and_a_removed_project_takes_its_log() {
        let dir = tempfile::tempdir().unwrap();
        listing(dir.path(), &["keep", "gone", "../x"]);
        note(dir.path(), "../x", 1, "closed", 1);
        assert!(!dir.path().join(DIR).exists());
        note(dir.path(), "keep", 1, "closed", 1);
        note(dir.path(), "gone", 1, "closed", 1);
        listing(dir.path(), &["keep"]);
        forget(dir.path(), &["keep"]);
        assert_eq!((counts(dir.path(), "keep").closed, counts(dir.path(), "gone").closed), (1, 0));
    }

    /// A write queued before the project was removed runs after its log was deleted: it must not bring the log back.
    #[test]
    fn a_write_queued_before_the_removal_does_not_bring_the_log_back() {
        let dir = tempfile::tempdir().unwrap();
        listing(dir.path(), &["gone"]);
        note(dir.path(), "gone", 1, "closed", 1);
        listing(dir.path(), &[]);
        forget(dir.path(), &[]);
        note(dir.path(), "gone", 2, "closed", 1);
        assert!(!file(dir.path(), "gone").unwrap().exists());
    }
}
