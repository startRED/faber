//! Remembers an unattended install to avoid a restart loop.

use crate::now_secs;
use std::path::Path;
const TRIED_FOR: u64 = 24 * 3600;

/// The install of a version is written down before it starts: if the installer fails (or starts and dies), Faber comes back
/// on the same version and must not try the same install again at once, one relaunch after another.
pub const TRIED_FILE: &str = "update-tried";

/// Written and flushed to the disk: an install that cannot leave this marker behind is not started unattended, because a
/// failing installer would then bring Faber back to try it again, one relaunch after another.
pub fn note_tried(data_dir: &Path, version: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(data_dir.join(TRIED_FILE))?;
    file.write_all(format!("{version} {}", now_secs()).as_bytes())?;
    file.sync_all()
}

/// Whether `text` (the tried file) says `version` was tried less than `TRIED_FOR` seconds before `now`.
fn tried_recently(text: &str, version: &str, now: u64) -> bool {
    text.split_once(' ').is_some_and(|(v, at)| v == version && at.trim().parse::<u64>().is_ok_and(|at| now.saturating_sub(at) < TRIED_FOR))
}

pub fn was_tried(data_dir: &Path, version: &str) -> bool {
    std::fs::read_to_string(data_dir.join(TRIED_FILE)).is_ok_and(|text| tried_recently(&text, version, now_secs()))
}

#[cfg(test)]
mod tests {
    use super::{TRIED_FOR, tried_recently};

    #[test]
    fn only_the_same_version_is_held_for_a_day() {
        let now = 1_000_000;
        assert!(tried_recently(&format!("1.0.0 {}", now - TRIED_FOR + 1), "1.0.0", now));
        assert!(!tried_recently(&format!("1.0.0 {}", now - TRIED_FOR), "1.0.0", now));
        assert!(!tried_recently(&format!("1.0.0 {}", now - 1), "1.0.1", now));
        assert!(!tried_recently("invalid", "1.0.0", now));
    }

    #[test]
    fn a_tried_version_is_remembered_for_a_day_and_only_that_version_on_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!super::was_tried(dir.path(), "0.3.15"), "nothing was written yet");
        super::note_tried(dir.path(), "0.3.15").unwrap();
        assert!(super::was_tried(dir.path(), "0.3.15"));
        assert!(!super::was_tried(dir.path(), "0.3.16"), "a newer version is a new chance");
    }

    #[test]
    fn a_marker_that_cannot_be_written_is_an_error_not_silence() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-folder").join("deeper");
        assert!(super::note_tried(&missing, "1.2.3").is_err());
    }

    #[test]
    fn garbage_and_an_empty_file_hold_nothing() {
        let now = 1_000_000;
        assert!(!tried_recently("garbage", "0.3.15", now) && !tried_recently("", "0.3.15", now));
        assert!(tried_recently(&format!("0.3.15 {}", now - 60), "0.3.15", now));
    }
}
