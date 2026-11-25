//! The engine's one lock, timed. Every screen command, CLI event and timer takes it, so one that holds it for seconds
//! freezes the screen with no error: a click on a conversation changes the crumb but the conversation never shows
//! (2026-11-25, after a release that saves idle conversations on a timer). A wait or a hold of [`SLOW`] or more adds a
//! line to `engine-slow.log` in the data folder, written after the lock is released, naming where the lock was taken:
//!
//! `1795644021 waited=0ms held=2310ms at=engine/src/engine.rs:1423`
//!
//! Cost: two clock reads and a pointer per lock; nothing is kept between locks, and the file is written only when slow.
use std::fs::OpenOptions;
use std::io::Write as _;
use std::ops::{Deref, DerefMut};
use std::panic::Location;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A wait or a hold this long is a frozen screen for the person, and gets a line.
pub(super) const SLOW: Duration = Duration::from_secs(1);
/// The file starts over past this size, its older lines kept once in `engine-slow.old.log`.
const MAX_BYTES: u64 = 64 * 1024;
pub(super) const FILE: &str = "engine-slow.log";

pub(super) struct Timed<T> {
    inner: Mutex<T>,
    log: Option<PathBuf>,
    slow: Duration,
}

impl<T> Timed<T> {
    /// `data`: the folder the log goes in; `None` keeps no log.
    pub(super) fn new(value: T, data: Option<&Path>) -> Self {
        Self::with_threshold(value, data, SLOW)
    }

    pub(super) fn with_threshold(value: T, data: Option<&Path>, slow: Duration) -> Self {
        Self { inner: Mutex::new(value), log: data.map(|d| d.join(FILE)), slow }
    }

    /// Like [`Mutex::lock`]. The place recorded is the first caller outside `#[track_caller]` functions.
    #[track_caller]
    pub(super) fn lock(&self) -> Result<Guard<'_, T>, PoisonError<Guard<'_, T>>> {
        let at = Location::caller();
        let asked = Instant::now();
        let wrap = |guard| Guard { guard: Some(guard), at, waited: asked.elapsed(), since: Instant::now(), owner: self };
        match self.inner.lock() {
            Ok(guard) => Ok(wrap(guard)),
            Err(poisoned) => Err(PoisonError::new(wrap(poisoned.into_inner()))),
        }
    }
}

pub(super) struct Guard<'a, T> {
    guard: Option<MutexGuard<'a, T>>,
    at: &'static Location<'static>,
    waited: Duration,
    since: Instant,
    owner: &'a Timed<T>,
}

impl<'a, T> Guard<'a, T> {
    /// Like [`Condvar::wait`]: the hold so far is checked as one, the wait for the signal (the lock released) is not a hold,
    /// and the hold after it counts from when the lock comes back.
    pub(super) fn wait(mut self, changed: &Condvar) -> Self {
        let guard = self.guard.take().expect("held until dropped");
        self.report(self.since.elapsed());
        let guard = changed.wait(guard).unwrap_or_else(PoisonError::into_inner);
        Guard { guard: Some(guard), at: self.at, waited: Duration::ZERO, since: Instant::now(), owner: self.owner }
    }

    fn report(&self, held: Duration) {
        let Some(path) = &self.owner.log else { return };
        if self.waited < self.owner.slow && held < self.owner.slow {
            return;
        }
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        write(path, &line(secs, self.waited, held, self.at));
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.guard.as_ref().expect("held until dropped")
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.guard.as_mut().expect("held until dropped")
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        // A guard given up by `wait` has nothing left to report.
        let Some(guard) = self.guard.take() else { return };
        let held = self.since.elapsed();
        // Released first: the line is written by no one holding the lock.
        drop(guard);
        self.report(held);
    }
}

fn line(secs: u64, waited: Duration, held: Duration, at: &Location<'_>) -> String {
    let file = at.file().replace('\\', "/");
    format!("{secs} waited={}ms held={}ms at={file}:{}\n", waited.as_millis(), held.as_millis(), at.line())
}

/// Best effort: a log that cannot be written changes nothing else.
fn write(path: &Path, line: &str) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_file_name("engine-slow.old.log"));
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_hold_writes_a_line_naming_where_the_lock_was_taken() {
        let dir = tempfile::tempdir().unwrap();
        let lock = Timed::with_threshold(0u32, Some(dir.path()), Duration::ZERO);
        let taken = line!() + 1;
        *lock.lock().unwrap() += 1;
        let log = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert_eq!(log.lines().count(), 1, "{log}");
        assert!(log.contains(&format!("at=engine/src/engine/slow_lock.rs:{taken}\n")), "{log}");
        assert!(log.contains(" waited=") && log.contains(" held="), "{log}");
        assert_eq!(*lock.lock().unwrap(), 1);
    }

    #[test]
    fn a_quick_lock_writes_nothing_and_no_folder_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let quick = Timed::new(0u32, Some(dir.path()));
        *quick.lock().unwrap() += 1;
        assert!(!dir.path().join(FILE).exists());
        let nowhere = Timed::with_threshold(0u32, None, Duration::ZERO);
        *nowhere.lock().unwrap() += 1;
        assert_eq!(*nowhere.lock().unwrap(), 1);
    }

    #[test]
    fn a_poisoned_lock_still_gives_its_value() {
        let lock = std::sync::Arc::new(Timed::new(5u32, None));
        let other = lock.clone();
        let _ = std::thread::spawn(move || {
            let _held = other.lock().unwrap();
            panic!("poison");
        })
        .join();
        assert_eq!(*lock.lock().unwrap_or_else(|e| e.into_inner()), 5);
    }

    #[test]
    fn waiting_on_a_condvar_releases_the_lock_and_counts_no_hold() {
        let dir = tempfile::tempdir().unwrap();
        let lock = std::sync::Arc::new(Timed::with_threshold(false, Some(dir.path()), Duration::from_millis(200)));
        let changed = std::sync::Arc::new(Condvar::new());
        let (other, signal) = (lock.clone(), changed.clone());
        let setter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            *other.lock().unwrap() = true;
            signal.notify_all();
        });
        let mut ready = lock.lock().unwrap();
        while !*ready {
            ready = ready.wait(&changed);
        }
        drop(ready);
        setter.join().unwrap();
        // 400 ms passed waiting for the signal, past the 200 ms threshold, yet nobody held the lock that long.
        assert!(!dir.path().join(FILE).exists(), "{:?}", std::fs::read_to_string(dir.path().join(FILE)));
    }

    #[test]
    fn a_big_log_starts_over_and_keeps_its_older_lines_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        write(&path, "new\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
        assert_eq!(std::fs::metadata(dir.path().join("engine-slow.old.log")).unwrap().len(), MAX_BYTES + 1);
    }
}
