//! Build slots (R129, E3): however many lanes work at once, only a few `cargo t` runs build and test together on one
//! PC, so parallel lanes queue instead of filling the memory. A slot is a locked file in the main checkout's `target`;
//! the lock ends with the process, even when it crashes.

use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::time::Duration;

/// About 8 GB at the peak of one run on a 36 GB PC. `FABER_BUILD_SLOTS` changes it for another PC.
pub const DEFAULT: usize = 3;

pub fn count(env: Option<&str>) -> usize {
    env.and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(DEFAULT)
}

/// The first free slot in `dir`, held as long as the file lives, or `None` while all `count` are taken.
pub fn try_take(dir: &Path, count: usize) -> Option<File> {
    (0..count).find_map(|i| {
        let file = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(format!("slot-{i}"))).ok()?;
        file.try_lock().ok()?;
        Some(file)
    })
}

/// What one run takes at its peak. A new run starts only while this much memory is free, so the person's game or work
/// never pages to make room for a build (on 2026-10-22 three runs left 2.2 GB of 36 free under a game).
pub const RUN_PEAK: u64 = 8 << 30;

/// How long a run waits for memory at most. A PC whose own work keeps it short would otherwise hold every lane forever;
/// a game in front holds for as long as it stays there.
pub const MEMORY_WAIT: Duration = Duration::from_secs(10 * 60);

/// Why a new run waits before its slot, after `waited`: a full-screen app in front (a game, a presentation), or less free
/// memory than one run takes. Idle priority already gives the processor away, but not the memory, the bus or the heat.
pub fn hold_reason(full_screen: bool, free: u64, waited: Duration) -> Option<&'static str> {
    if full_screen {
        return Some("a full-screen app is in front");
    }
    (free < RUN_PEAK && waited < MEMORY_WAIT).then_some("less memory is free than a run takes (8 GB); waiting at most 10 minutes")
}

/// The reason to hold on this PC now; never on CI, whose machine is the job's alone, nor where Windows cannot say.
fn hold(waited: Duration) -> Option<&'static str> {
    #[cfg(windows)]
    if !crate::plan::on_ci() {
        let free = winsafe::GlobalMemoryStatusEx().map_or(u64::MAX, |m| m.ullAvailPhys);
        return hold_reason(faber_winproc::full_screen_in_front(), free, waited);
    }
    let _ = waited;
    None
}

/// Waits for a slot, saying so once per reason. A folder that cannot be made gives no slot and no wait: the run goes on.
pub fn take(dir: &Path) -> Option<File> {
    fs::create_dir_all(dir).ok()?;
    let count = count(std::env::var("FABER_BUILD_SLOTS").ok().as_deref());
    let busy = format!("{count} test runs already build or test on this PC");
    let (started, mut said) = (std::time::Instant::now(), "");
    loop {
        let why = match hold(started.elapsed()) {
            Some(why) => why,
            None => match try_take(dir, count) {
                Some(slot) => return Some(slot),
                None => busy.as_str(),
            },
        };
        if why != said {
            println!("Waiting: {why}");
            said = why;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_comes_from_the_variable_when_it_is_a_positive_number() {
        assert_eq!(count(None), DEFAULT);
        assert_eq!(count(Some("5")), 5);
        assert_eq!(count(Some("0")), DEFAULT);
        assert_eq!(count(Some("many")), DEFAULT);
    }

    #[test]
    fn a_new_run_holds_while_a_full_screen_app_is_in_front_or_memory_is_short_of_one_run() {
        let (now, late) = (Duration::ZERO, MEMORY_WAIT);
        assert_eq!(hold_reason(true, u64::MAX, late), Some("a full-screen app is in front"), "a game holds however long");
        assert!(hold_reason(false, 2 << 30, now).is_some_and(|why| why.starts_with("less memory")));
        assert_eq!(hold_reason(false, 2 << 30, late), None, "short memory holds ten minutes at most");
        assert_eq!(hold_reason(false, RUN_PEAK, now), None);
    }

    #[test]
    fn slots_are_held_until_dropped_and_no_more_than_the_count_are_given() {
        let temp = tempfile::tempdir().unwrap();
        let first = try_take(temp.path(), 2).expect("a free slot");
        let second = try_take(temp.path(), 2).expect("another free slot");
        assert!(try_take(temp.path(), 2).is_none(), "both slots are taken");
        drop(first);
        assert!(try_take(temp.path(), 2).is_some(), "a dropped slot is free again");
        drop(second);
    }
}
