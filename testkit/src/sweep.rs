//! The build folder budget (R106). Every worktree builds into a folder of the main checkout's `target`, and Cargo never
//! deletes anything there, so the runner sweeps it at start, at most once an hour:
//!
//! 1. A worktree's folder whose worktree is gone, idle for a day, is removed whole.
//! 2. In every build profile (`debug`, `release`, nested ones), an artifact in `deps/` or an `incremental/` folder that
//!    a newer build of the same crate replaced more than a day ago is removed. A crate's newest build always stays.
//! 3. If `target` is still above the cap, whole worktree folders go, the least recently used first.
//!
//! A folder is never touched while Cargo holds its lock or one of its programs runs, and the main checkout's own
//! folders (`debug`, `release`, `faber-test`) are only ever swept, never removed whole.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Above this, whole worktree folders are removed. Each worktree needs about 5 to 15 GB once swept.
pub const CAP: u64 = 60 << 30;
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
/// How long a replaced build stays, so a build that just moved on can still go back cheaply.
const STALE: Duration = DAY;
const EVERY: Duration = Duration::from_secs(60 * 60);
const STAMP: &str = ".faber-sweep";
/// The main checkout's own build folders that carry a `CACHEDIR.TAG` of their own.
const MAIN_FOLDERS: [&str; 1] = ["faber-test"];
const LOCKS: [&str; 3] = [".cargo-lock", ".cargo-build-lock", ".cargo-artifact-lock"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub before: u64,
    pub after: u64,
    pub removed: Vec<String>,
}

/// The worktree paths in `git worktree list --porcelain`, the main checkout first.
pub fn worktrees(porcelain: &str) -> Vec<PathBuf> {
    porcelain.lines().filter_map(|l| l.strip_prefix("worktree ")).map(PathBuf::from).collect()
}

/// Whether the folder `target/<name>` belongs to a live worktree: one named `<name>` or `<anything>-<name>`.
pub fn owned(name: &str, worktrees: &[PathBuf]) -> bool {
    let suffix = format!("-{name}");
    worktrees.iter().filter_map(|w| w.file_name()?.to_str()).any(|w| w == name || w.ends_with(&suffix))
}

/// `libserde-0123456789abcdef.rlib` → (`libserde`, `rlib`). Files without Cargo's 16-digit hash give `None`.
pub fn dep_key(file_name: &str) -> Option<(&str, &str)> {
    let (first, _) = file_name.split_once('.')?;
    let (krate, hash) = first.rsplit_once('-')?;
    let ext = file_name.rsplit_once('.')?.1;
    (hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some((krate, ext))
}

/// Which folders to remove to get `total` under `cap`: the least recently used first, busy ones never.
/// Each candidate is (name, size, last used, busy).
pub fn over_budget(mut candidates: Vec<(String, u64, SystemTime, bool)>, mut total: u64, cap: u64) -> Vec<String> {
    candidates.sort_by_key(|c| c.2);
    let mut picked = Vec::new();
    for (name, size, _, busy) in candidates {
        if total <= cap {
            break;
        }
        if !busy {
            total = total.saturating_sub(size);
            picked.push(name);
        }
    }
    picked
}

fn mtime(path: &Path) -> SystemTime {
    fs::symlink_metadata(path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect()
}

/// The build profiles (folders with a `.fingerprint`) in `dir`, down to four levels.
fn profiles(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut level = vec![dir.to_path_buf()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for d in level {
            if d.join(".fingerprint").is_dir() {
                found.push(d);
            } else {
                next.extend(subdirs(&d));
            }
        }
        level = next;
    }
    found
}

/// Takes every Cargo lock of a profile, or `None` while Cargo holds one. Held, Cargo waits for the sweep to end.
fn lock(profile: &Path) -> Option<Vec<File>> {
    let mut held = Vec::new();
    for name in LOCKS {
        let path = profile.join(name);
        if !path.exists() {
            continue;
        }
        let file = OpenOptions::new().read(true).write(true).open(path).ok()?;
        file.try_lock().ok()?;
        held.push(file);
    }
    Some(held)
}

/// Whether one of the profile's programs runs: Windows refuses to open a running program for writing.
fn running(profile: &Path) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Ok(entries) = fs::read_dir(profile) else { return false };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "exe"))
        .any(|p| OpenOptions::new().write(true).open(p).is_err())
}

fn busy(dir: &Path) -> bool {
    profiles(dir).iter().any(|p| running(p) || lock(p).is_none())
}

fn last_used(dir: &Path) -> SystemTime {
    profiles(dir).iter().flat_map(|p| [mtime(p), mtime(&p.join("deps"))]).chain([mtime(dir)]).max().unwrap_or(SystemTime::UNIX_EPOCH)
}

pub fn size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn remove(path: &Path) -> u64 {
    let bytes = if path.is_dir() { size(path) } else { fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0) };
    let gone = if path.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) };
    if gone.is_ok() { bytes } else { 0 }
}

/// The entries, each (path, key, modified), older than `cutoff` that a newer entry with the same key replaced.
pub fn replaced(entries: &[(PathBuf, String, SystemTime)], cutoff: SystemTime) -> Vec<PathBuf> {
    let mut newest: HashMap<&str, SystemTime> = HashMap::new();
    for (_, key, time) in entries {
        let slot = newest.entry(key).or_insert(*time);
        *slot = (*slot).max(*time);
    }
    entries.iter().filter(|(_, key, time)| *time < cutoff && newest[key.as_str()] > *time).map(|(path, ..)| path.clone()).collect()
}

/// Removes the replaced builds older than `cutoff` in one profile: artifacts in `deps/` keyed by crate and extension,
/// `incremental/` folders keyed by crate. A crate's newest build always stays. Returns bytes freed.
pub fn sweep_profile(profile: &Path, cutoff: SystemTime) -> u64 {
    let Some(_locks) = lock(profile) else { return 0 };
    let listed = |dir: &str, key: fn(&str) -> Option<String>| -> Vec<(PathBuf, String, SystemTime)> {
        let Ok(entries) = fs::read_dir(profile.join(dir)) else { return Vec::new() };
        entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| !t.is_symlink()))
            .filter_map(|e| Some((e.path(), key(e.file_name().to_str()?)?, e.metadata().ok()?.modified().ok()?)))
            .collect()
    };
    let deps = listed("deps", |name| dep_key(name).map(|(krate, ext)| format!("{krate}.{ext}")));
    let incremental = listed("incremental", |name| Some(name.rsplit_once('-')?.0.to_string()));
    replaced(&deps, cutoff).iter().chain(&replaced(&incremental, cutoff)).map(|p| remove(p)).sum()
}

/// Sweeps the shared `target` of the main checkout. `own` is this run's build folder, never removed whole.
pub fn sweep(target: &Path, own: &Path, worktrees: &[PathBuf], now: SystemTime, cap: u64) -> Report {
    let before = size(target);
    let mut report = Report { before, after: before, removed: Vec::new() };
    let worktree_folders = |report: &Report| -> Vec<(String, PathBuf)> {
        subdirs(target)
            .into_iter()
            .filter(|d| d.join("CACHEDIR.TAG").is_file() && d.as_path() != own)
            .filter_map(|d| Some((d.file_name()?.to_str()?.to_string(), d)))
            .filter(|(name, _)| !MAIN_FOLDERS.contains(&name.as_str()) && !report.removed.contains(name))
            .collect()
    };
    for (name, dir) in worktree_folders(&report) {
        if !owned(&name, worktrees) && last_used(&dir) + DAY < now && !busy(&dir) {
            report.after -= remove(&dir).min(report.after);
            report.removed.push(name);
        }
    }
    for profile in profiles(target) {
        report.after -= sweep_profile(&profile, now - STALE).min(report.after);
    }
    if report.after > cap {
        let candidates = worktree_folders(&report).into_iter().map(|(name, dir)| (name, size(&dir), last_used(&dir), busy(&dir))).collect();
        for name in over_budget(candidates, report.after, cap) {
            report.after -= remove(&target.join(&name)).min(report.after);
            report.removed.push(name);
        }
    }
    report
}

/// Whether the last sweep of `target` is older than an hour; marks this one as done.
pub fn due(target: &Path, now: SystemTime) -> bool {
    let stamp = target.join(STAMP);
    if mtime(&stamp) + EVERY > now {
        return false;
    }
    File::create(stamp).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    fn write(path: &Path, bytes: usize, age: Duration) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
        File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - age).unwrap();
    }

    #[test]
    fn porcelain_lists_every_worktree_and_a_folder_belongs_to_one_by_its_last_name_part() {
        let list =
            worktrees("worktree E:/p/faber\nHEAD eb53a93\nbranch refs/heads/main\n\nworktree E:/p/faber-wt-projhome\nHEAD 1\ndetached\n");
        assert_eq!(list, [PathBuf::from("E:/p/faber"), PathBuf::from("E:/p/faber-wt-projhome")]);
        assert!(owned("projhome", &list));
        assert!(owned("faber", &list));
        assert!(!owned("rightclick", &list), "its worktree was removed");
        assert!(!owned("home", &list), "only a whole name after a dash counts");
    }

    #[test]
    fn a_dependency_key_is_the_crate_and_the_extension_and_needs_cargo_s_hash() {
        assert_eq!(dep_key("libserde-0123456789abcdef.rlib"), Some(("libserde", "rlib")));
        assert_eq!(dep_key("faber_engine-43ee06eff087acaa.006z0o0i5w.0229au1.rcgu.o"), Some(("faber_engine", "o")));
        assert_eq!(dep_key("faber.exe"), None);
        assert_eq!(dep_key("my-tool-notahash.d"), None);
    }

    #[test]
    fn the_budget_removes_the_least_recently_used_first_skips_busy_ones_and_stops_under_the_cap() {
        let at = |days| SystemTime::UNIX_EPOCH + DAY * days;
        let candidates = vec![
            ("new".to_string(), 20 * GB, at(9), false),
            ("old-busy".to_string(), 20 * GB, at(1), true),
            ("old".to_string(), 20 * GB, at(2), false),
            ("mid".to_string(), 20 * GB, at(5), false),
        ];
        assert_eq!(over_budget(candidates.clone(), 90 * GB, 60 * GB), ["old", "mid"]);
        assert!(over_budget(candidates, 60 * GB, 60 * GB).is_empty());
    }

    #[test]
    fn a_profile_sweep_removes_replaced_old_artifacts_and_old_incremental_folders_only() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("debug");
        fs::create_dir_all(profile.join(".fingerprint")).unwrap();
        let deps = profile.join("deps");
        write(&deps.join("libold-0000000000000001.rlib"), 10, STALE * 2);
        write(&deps.join("libold-0000000000000002.rlib"), 10, Duration::ZERO);
        write(&deps.join("liblone-0000000000000003.rlib"), 10, STALE * 2);
        write(&deps.join("libfresh-0000000000000004.rlib"), 10, EVERY);
        write(&deps.join("libfresh-0000000000000005.rlib"), 10, Duration::ZERO);
        write(&profile.join("incremental/faber-abc/s-1/work.bin"), 10, Duration::ZERO);

        assert_eq!(sweep_profile(&profile, SystemTime::now() - STALE), 10);
        assert!(!deps.join("libold-0000000000000001.rlib").exists(), "replaced and older than the cutoff");
        assert!(deps.join("libold-0000000000000002.rlib").exists());
        assert!(deps.join("liblone-0000000000000003.rlib").exists(), "nothing replaced it");
        assert!(deps.join("libfresh-0000000000000004.rlib").exists(), "replaced, but after the cutoff");

        assert_eq!(sweep_profile(&profile, SystemTime::now() + DAY), 10, "a cutoff in the future makes every replaced file old");
        assert!(profile.join("incremental/faber-abc").exists(), "a crate's only incremental folder stays");
    }

    #[test]
    fn an_incremental_folder_goes_once_a_newer_one_of_the_same_crate_replaced_it() {
        let at = |days| SystemTime::UNIX_EPOCH + DAY * days;
        let entry = |name: &str, day| (PathBuf::from(name), name.rsplit_once('-').unwrap().0.to_string(), at(day));
        let entries = [entry("faber_engine-0aa", 1), entry("faber_engine-1bb", 5), entry("faber_engine-2cc", 9), entry("faber-3dd", 1)];
        assert_eq!(replaced(&entries, at(6)), [PathBuf::from("faber_engine-0aa"), PathBuf::from("faber_engine-1bb")]);
        assert_eq!(replaced(&entries, at(2)), [PathBuf::from("faber_engine-0aa")]);
    }

    #[test]
    fn a_profile_whose_lock_cargo_holds_is_left_alone() {
        let temp = tempfile::tempdir().unwrap();
        let profile = temp.path().join("debug");
        write(&profile.join(".cargo-lock"), 0, Duration::ZERO);
        write(&profile.join("deps/liba-0000000000000001.rlib"), 10, STALE * 2);
        write(&profile.join("deps/liba-0000000000000002.rlib"), 10, Duration::ZERO);
        let cargo = File::options().read(true).write(true).open(profile.join(".cargo-lock")).unwrap();
        cargo.lock().unwrap();
        assert_eq!(sweep_profile(&profile, SystemTime::now()), 0);
        drop(cargo);
        assert_eq!(sweep_profile(&profile, SystemTime::now()), 10);
    }

    #[test]
    fn the_sweep_removes_gone_worktrees_then_keeps_the_main_folders_and_this_run_s_own_under_any_cap() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let folder = |name: &str, bytes| {
            write(&target.join(name).join("CACHEDIR.TAG"), 0, Duration::ZERO);
            write(&target.join(name).join("debug/.fingerprint/x"), 0, Duration::ZERO);
            write(&target.join(name).join("debug/faber.pdb"), bytes, Duration::ZERO);
        };
        folder("gone", 100);
        folder("live", 100);
        folder("mine", 100);
        folder("faber-test", 100);
        write(&target.join("debug/.fingerprint/x"), 0, Duration::ZERO);
        write(&target.join("debug/faber.exe"), 100, Duration::ZERO);
        let worktrees = [temp.path().join("faber"), temp.path().join("faber-wt-live"), temp.path().join("faber-wt-mine")];

        let today = sweep(&target, &target.join("mine"), &worktrees, SystemTime::now(), u64::MAX);
        assert!(today.removed.is_empty(), "a folder whose worktree is gone keeps a day of grace");

        let later = SystemTime::now() + DAY * 2;
        let report = sweep(&target, &target.join("mine"), &worktrees, later, u64::MAX);
        assert_eq!(report.removed, ["gone"]);
        assert_eq!((report.before, report.after), (500, 400));

        let report = sweep(&target, &target.join("mine"), &worktrees, later, 0);
        assert_eq!(report.removed, ["live"], "over the cap, only other worktrees' folders go");
        assert!(target.join("debug/faber.exe").exists() && target.join("faber-test").exists() && target.join("mine").exists());
    }

    #[test]
    fn the_sweep_runs_at_most_once_an_hour() {
        let temp = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        assert!(due(temp.path(), now));
        assert!(!due(temp.path(), now));
        assert!(due(temp.path(), now + EVERY * 2));
    }
}
