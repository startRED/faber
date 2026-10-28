//! The build folder budget (R106). Local commands reuse a leased cache pool in the main checkout's `target`. Maintenance
//! also accounts for legacy worktree-local targets and sweeps at command boundaries, at most once an hour:
//!
//! 1. A build cache nothing used for two hours, in the shared `target` or a worktree's own, has its build output
//!    removed, whether its worktree is alive or gone: sccache makes the rebuild cheap, and an idle cache holds 3 to 30 GB.
//! 2. In every build profile (`debug`, `release`, nested ones), an artifact in `deps/` or an `incremental/` folder that
//!    a newer build of the same crate replaced more than two hours ago is removed. A crate's newest build always stays.
//! 3. If build output is still above the cap (12 GB, so all of Faber's builds on one PC stay under 15 GB), idle caches
//!    are cleared, the least recently used first. A folder used in the last half hour belongs to a lane at work and stays.
//!
//! A folder is never touched while Cargo holds its lock or one of its programs runs, and the main checkout's own
//! folders (`debug`, `release`, `faber-test`) are only ever swept, never cleared. Lock files and ownership records stay.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Above this, idle caches are cleared, so all of Faber's builds on one PC stay under 15 GB with `debug` (the owner,
/// 2026-10-25).
pub const CAP: u64 = 12 << 30;
/// A cache nothing used for this long is cleared, and a replaced build this old leaves its profile.
const IDLE: Duration = Duration::from_secs(2 * 60 * 60);
/// How long a replaced build stays, so a build that just moved on can still go back cheaply.
const STALE: Duration = IDLE;
const EVERY: Duration = Duration::from_secs(60 * 60);
/// Over the cap, a folder used more recently than this is a lane still at work, never evicted.
const ACTIVE: Duration = Duration::from_secs(30 * 60);
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

/// Where a lane builds when its sandbox may not write the main checkout (`%TEMP%\faber-target\<lane>`, as Codex lanes
/// do). Swept by the same rules: on 2026-10-22 it held 83 GB in 22 folders made that day, which no sweep reached.
pub fn loose_root() -> PathBuf {
    std::env::temp_dir().join("faber-target")
}

/// The worktree paths in `git worktree list --porcelain`, the main checkout first.
pub fn worktrees(porcelain: &str) -> Vec<PathBuf> {
    porcelain.lines().filter_map(|l| l.strip_prefix("worktree ")).map(PathBuf::from).collect()
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
        let file = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path).ok()?;
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
    leased(dir) || descendant_lease(dir) || profiles(dir).iter().any(|p| running(p) || lock(p).is_none())
}

fn leased(dir: &Path) -> bool {
    dir.ancestors().any(|p| p.join(".faber-lease").try_exists().unwrap_or(true))
}

fn descendant_lease(dir: &Path) -> bool {
    let Ok(mut entries) = fs::read_dir(dir) else { return true };
    entries.any(|entry| {
        let Ok(entry) = entry else { return true };
        if entry.file_name() == ".faber-lease" {
            return false;
        }
        let Ok(kind) = entry.file_type() else { return true };
        kind.is_dir() && (entry.path().join(".faber-lease").try_exists().unwrap_or(true) || descendant_lease(&entry.path()))
    })
}

struct Lease(PathBuf);

impl Lease {
    fn take(dir: &Path) -> Option<Self> {
        if leased(dir) {
            return None;
        }
        let path = dir.join(".faber-lease");
        fs::create_dir(&path).ok()?;
        // Both sides publish before checking overlaps: a racing parent and child cannot both enter.
        if dir.parent().is_some_and(leased) || descendant_lease(dir) {
            let _ = fs::remove_dir(&path);
            return None;
        }
        let pid = std::process::id();
        let created = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).ok()?.as_millis();
        let receipt = format!("{{\"pid\":{pid},\"childPid\":{pid},\"createdAt\":{created},\"tempName\":\"run-maintenance-{pid}\"}}");
        if fs::write(path.join("owner.json"), receipt).is_err() {
            let _ = fs::remove_dir(&path);
            return None;
        }
        Some(Self(path))
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.0.join("owner.json"));
        let _ = fs::remove_dir(&self.0);
    }
}

/// Keep Cargo's lock files in place while deleting: replacing an open lock would let another build enter.
fn remove_cache(dir: &Path) -> u64 {
    let Some(_lease) = Lease::take(dir) else { return 0 };
    let profiles = profiles(dir);
    let mut held = Vec::new();
    for profile in &profiles {
        if running(profile) {
            return 0;
        }
        let Some(locks) = lock(profile) else { return 0 };
        held.extend(locks);
    }
    fn clear(dir: &Path) -> u64 {
        let Ok(entries) = fs::read_dir(dir) else { return 0 };
        entries
            .flatten()
            .map(|entry| {
                if LOCKS.iter().any(|name| entry.file_name() == *name)
                    || entry.file_name() == "CACHEDIR.TAG"
                    || entry.file_name().to_string_lossy().starts_with(".faber-")
                {
                    return 0;
                }
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => {
                        let bytes = clear(&entry.path());
                        let _ = fs::remove_dir(entry.path());
                        bytes
                    }
                    Ok(kind) if kind.is_file() => remove(&entry.path()),
                    _ => 0,
                }
            })
            .sum()
    }
    clear(dir)
}

fn last_used(dir: &Path) -> SystemTime {
    // Acquiring a maintenance lease changes the cache root's mtime, but is not a build.
    profiles(dir).iter().flat_map(|p| [mtime(p), mtime(&p.join("deps"))]).max().unwrap_or_else(|| mtime(dir))
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
    let cache = profile.ancestors().find(|dir| dir.join("CACHEDIR.TAG").is_file()).or_else(|| profile.parent());
    let Some(_lease) = cache.and_then(Lease::take) else { return 0 };
    if running(profile) {
        return 0;
    }
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
pub fn sweep(target: &Path, own: &Path, now: SystemTime, cap: u64) -> Report {
    let before = size(target);
    let mut report = Report { before, after: before, removed: Vec::new() };
    let worktree_folders = |report: &Report| -> Vec<(String, PathBuf)> {
        subdirs(target)
            .into_iter()
            .filter(|d| d.join("CACHEDIR.TAG").is_file() && d.as_path() != own && !own.starts_with(d))
            .filter_map(|d| Some((d.file_name()?.to_str()?.to_string(), d)))
            .filter(|(name, _)| !MAIN_FOLDERS.contains(&name.as_str()) && !report.removed.contains(name))
            .collect()
    };
    for (name, dir) in worktree_folders(&report) {
        if last_used(&dir) + IDLE < now && !busy(&dir) {
            let freed = remove_cache(&dir);
            report.after -= freed.min(report.after);
            if freed > 0 {
                report.removed.push(name);
            }
        }
    }
    for profile in profiles(target) {
        report.after -= sweep_profile(&profile, now - STALE).min(report.after);
    }
    if report.after > cap {
        let candidates = worktree_folders(&report)
            .into_iter()
            .map(|(name, dir)| {
                let used = last_used(&dir);
                (name, size(&dir), used, used + ACTIVE > now || busy(&dir))
            })
            .collect();
        for name in over_budget(candidates, report.after, cap) {
            let freed = remove_cache(&target.join(&name));
            report.after -= freed.min(report.after);
            if freed > 0 {
                report.removed.push(name);
            }
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

/// Sweeps Cargo output belonging to all registered checkouts under one budget.
pub fn sweep_worktrees(target: &Path, own: &Path, worktrees: &[PathBuf], now: SystemTime, cap: u64) -> Report {
    let mut report = sweep(target, own, now, u64::MAX);
    let mut local = Vec::new();
    for worktree in worktrees.iter().skip(1) {
        let dir = worktree.join("target");
        if dir == target || !fs::symlink_metadata(&dir).is_ok_and(|m| m.is_dir() && !m.is_symlink()) || !dir.join("CACHEDIR.TAG").is_file()
        {
            continue;
        }
        let before = size(&dir);
        report.before += before;
        report.after += before;
        if dir != own && !own.starts_with(&dir) && last_used(&dir) + IDLE < now && !busy(&dir) {
            let freed = remove_cache(&dir);
            report.after -= freed.min(report.after);
            if freed > 0 {
                report.removed.push(dir.to_string_lossy().into_owned());
                continue;
            }
        }
        for profile in profiles(&dir) {
            report.after -= sweep_profile(&profile, now - STALE).min(report.after);
        }
        local.push(dir);
    }
    let mut candidates: Vec<_> = local
        .into_iter()
        .map(|dir| {
            let used = last_used(&dir);
            let busy = dir == own || own.starts_with(&dir) || used + ACTIVE > now || busy(&dir);
            (dir.to_string_lossy().into_owned(), size(&dir), used, busy)
        })
        .collect();
    candidates.extend(
        subdirs(target)
            .into_iter()
            .filter(|dir| {
                dir.join("CACHEDIR.TAG").is_file()
                    && dir != own
                    && !own.starts_with(dir)
                    && dir.file_name().is_some_and(|name| !MAIN_FOLDERS.iter().any(|kept| name == *kept))
            })
            .map(|dir| {
                let used = last_used(&dir);
                let busy = used + ACTIVE > now || busy(&dir);
                (dir.to_string_lossy().into_owned(), size(&dir), used, busy)
            }),
    );
    for name in over_budget(candidates, report.after, cap) {
        let freed = remove_cache(Path::new(&name));
        report.after -= freed.min(report.after);
        if freed > 0 {
            report.removed.push(name);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;
    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn fixture() -> tempfile::TempDir {
        // The command's TEMP is inside a live cache lease; simulated caches must be outside that lease.
        tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap()
    }

    fn write(path: &Path, bytes: usize, age: Duration) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; bytes]).unwrap();
        File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - age).unwrap();
    }

    #[test]
    fn porcelain_lists_every_worktree_the_main_checkout_first() {
        let list =
            worktrees("worktree E:/p/faber\nHEAD eb53a93\nbranch refs/heads/main\n\nworktree E:/p/faber-wt-projhome\nHEAD 1\ndetached\n");
        assert_eq!(list, [PathBuf::from("E:/p/faber"), PathBuf::from("E:/p/faber-wt-projhome")]);
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
        let temp = fixture();
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
        let temp = fixture();
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
    fn the_sweep_clears_idle_caches_alive_or_gone_and_keeps_the_main_folders_and_this_run_s_own_under_any_cap() {
        let temp = fixture();
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
        let now = SystemTime::now();

        let today = sweep(&target, &target.join("mine"), now, u64::MAX);
        assert!(today.removed.is_empty(), "nothing is idle yet");
        let active = sweep(&target, &target.join("mine"), now, 0);
        assert!(active.removed.is_empty(), "over the cap, a folder used in the last half hour belongs to a lane at work");

        let later = now + IDLE + Duration::from_secs(60);
        let mut report = sweep(&target, &target.join("mine"), later, u64::MAX);
        report.removed.sort();
        assert_eq!(report.removed, ["gone", "live"], "two idle hours clear a cache whether its worktree is alive or gone");
        assert_eq!((report.before, report.after), (500, 300));
        assert!(
            target.join("debug/faber.exe").exists()
                && target.join("faber-test/debug/faber.pdb").exists()
                && target.join("mine/debug/faber.pdb").exists()
        );
    }

    #[test]
    fn the_sweep_runs_at_most_once_an_hour() {
        let temp = fixture();
        let now = SystemTime::now();
        assert!(due(temp.path(), now));
        assert!(!due(temp.path(), now));
        assert!(due(temp.path(), now + EVERY * 2));
    }

    #[test]
    fn a_leased_cache_is_never_swept_even_between_cargo_commands() {
        let temp = fixture();
        let cache = temp.path().join("build-1");
        let profile = cache.join("debug");
        write(&cache.join(".faber-lease/owner.json"), 0, Duration::ZERO);
        write(&profile.join("deps/liba-0000000000000001.rlib"), 10, DAY * 2);
        write(&profile.join("deps/liba-0000000000000002.rlib"), 10, Duration::ZERO);
        assert_eq!(sweep_profile(&profile, SystemTime::now()), 0);
        assert!(profile.join("deps/liba-0000000000000001.rlib").exists());
    }

    #[test]
    fn nested_cache_leases_exclude_parent_cleanup_and_parent_leases_exclude_children() {
        let temp = fixture();
        let parent = temp.path().join("cache");
        let nested = parent.join("nested");
        write(&nested.join("debug/keep"), 100, Duration::ZERO);
        let child_lease = Lease::take(&nested).unwrap();
        assert!(Lease::take(&parent).is_none(), "a parent must not reserve an active descendant");
        assert_eq!(remove_cache(&parent), 0);
        assert!(nested.join("debug/keep").exists());
        drop(child_lease);
        let parent_lease = Lease::take(&parent).unwrap();
        assert!(Lease::take(&nested).is_none(), "a descendant must not enter an active parent");
        drop(parent_lease);
        assert!(Lease::take(&nested).is_some());
    }

    #[test]
    fn an_explicit_cache_nested_in_an_old_slot_protects_its_parent() {
        let temp = fixture();
        let target = temp.path().join("target");
        let parent = target.join("old-slot");
        let own = parent.join("nested");
        write(&parent.join("CACHEDIR.TAG"), 0, Duration::ZERO);
        write(&parent.join("debug/.fingerprint/x"), 0, Duration::ZERO);
        write(&own.join("keep"), 100, Duration::ZERO);
        sweep(&target, &own, SystemTime::now() + DAY * 2, 0);
        assert!(own.join("keep").exists());
    }

    #[test]
    fn reusable_build_slots_are_evicted_when_idle_and_over_budget() {
        let temp = fixture();
        let target = temp.path().join("target");
        let cache = target.join("build-1");
        write(&cache.join("CACHEDIR.TAG"), 0, Duration::ZERO);
        write(&cache.join("debug/.fingerprint/x"), 0, Duration::ZERO);
        write(&cache.join("debug/deps/liba-0000000000000001.rlib"), 100, Duration::ZERO);
        let worktrees = [temp.path().to_path_buf(), temp.path().join("build-1")];
        let report = sweep_worktrees(&target, &target, &worktrees, SystemTime::now() + DAY * 2, 50);
        assert_eq!((report.before, report.after), (100, 0));
        assert!(!cache.join(".faber-lease").exists());
    }

    #[test]
    fn local_worktree_targets_count_toward_the_shared_budget() {
        let temp = fixture();
        let main = temp.path().join("main");
        let lane = temp.path().join("lane");
        let target = lane.join("target");
        write(&target.join("CACHEDIR.TAG"), 0, Duration::ZERO);
        write(&target.join("debug/.fingerprint/x"), 0, Duration::ZERO);
        write(&target.join("debug/deps/liba-0000000000000001.rlib"), 100, Duration::ZERO);
        write(&main.join("target/debug/.fingerprint/x"), 0, Duration::ZERO);
        write(&main.join("target/debug/keep"), 20, Duration::ZERO);
        let report = sweep_worktrees(&main.join("target"), &main.join("target"), &[main.clone(), lane], SystemTime::now() + DAY * 2, 50);
        assert_eq!((report.before, report.after), (120, 20));
        assert!(!target.join("debug/deps/liba-0000000000000001.rlib").exists());
        assert!(main.join("target/debug/keep").exists());
    }

    #[test]
    fn local_worktree_targets_with_a_live_build_or_without_a_cache_tag_stay() {
        let temp = fixture();
        let main = temp.path().join("main");
        let lane = temp.path().join("lane");
        let target = lane.join("target");
        write(&target.join("debug/.fingerprint/x"), 0, Duration::ZERO);
        write(&target.join("debug/keep"), 100, Duration::ZERO);
        let worktrees = [main.clone(), lane];
        let later = SystemTime::now() + DAY * 2;
        sweep_worktrees(&main.join("target"), &main.join("target"), &worktrees, later, 0);
        assert!(target.join("debug/keep").exists(), "an arbitrary folder is not a Cargo cache");
        write(&target.join("CACHEDIR.TAG"), 0, Duration::ZERO);
        write(&target.join("debug/.cargo-lock"), 0, Duration::ZERO);
        let cargo = File::options().read(true).write(true).open(target.join("debug/.cargo-lock")).unwrap();
        cargo.lock().unwrap();
        sweep_worktrees(&main.join("target"), &main.join("target"), &worktrees, later, 0);
        assert!(target.join("debug/keep").exists(), "Cargo owns this cache");
    }
}
