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

/// Waits for a slot, saying so once. A folder that cannot be made gives no slot and no wait: the run goes on.
pub fn take(dir: &Path) -> Option<File> {
    fs::create_dir_all(dir).ok()?;
    let count = count(std::env::var("FABER_BUILD_SLOTS").ok().as_deref());
    let mut said = false;
    loop {
        if let Some(slot) = try_take(dir, count) {
            return Some(slot);
        }
        if !said {
            println!("Waiting for a build slot: {count} test runs already build or test on this PC");
            said = true;
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
