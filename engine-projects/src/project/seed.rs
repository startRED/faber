//! A new copy starts its own build folders as real copies of the project's (task 203, the owner's choice of 2026-11-28):
//! `target` beside the top `Cargo.toml` and `node_modules` beside the top `package.json`, so its first build reuses the
//! dependencies the project already built instead of building every one again. Each copy keeps folders of its own: two
//! copies never share one, so neither can pick up the other's program.
//!
//! In a copied `target`, everything the project's own crates left is removed in every profile folder, wherever it sits
//! (fingerprints, build-script output, compiled pieces, incremental state and the finished programs), so they always build
//! again from the copy's files; only downloaded dependencies are reused, with their file times kept. `node_modules` is
//! copied only when the copy's lockfiles are the project's, byte for byte, so the copy never runs other tool versions.
//!
//! Only the top of the copy is seeded, so the folder a copy lands in is the copy itself, never a folder inside it the
//! agent could swap for a link while the copy is made. Nothing staged holds a link but npm's `.bin` links on macOS and
//! Linux, each checked word by word to stay inside without passing through another link. The copy is made in a staging
//! folder beside the copies and moved into place with a move that never replaces anything: a build the agent started
//! first, even one that only made an empty folder, keeps its own, and the staged one is discarded. One seeding runs at a
//! time in every Faber that shares the data folder, under a lock file the leftover sweep needs too, so two never count
//! the same free room and a sweep never removes a folder being filled. No git call and no process.
//!
//! What stays open: a folder of the project swapped for a link in the instant between its check and its reading is
//! followed, so plain files it leads to can be copied into the copy (never written outside it); walking by handles would
//! close that, at the cost of more unsafe code.

use std::collections::BTreeSet;
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use super::copy::home;
use super::git::same_path;
use super::lease::real_path;

/// How far seeding goes: never a folder above `largest`, never below `keep_free` bytes left on the drive.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub largest: u64,
    pub keep_free: u64,
}

/// A large Rust build is a few GB; the drive keeps room for the copy's own builds and everything else.
pub const LIMITS: Limits = Limits { largest: 8 << 30, keep_free: 10 << 30 };

static KEEP_FREE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(LIMITS.keep_free);

/// The limits the engine seeds with: [`LIMITS`], unless a test lowered the room kept free.
pub fn limits() -> Limits {
    Limits { keep_free: KEEP_FREE.load(std::sync::atomic::Ordering::Relaxed), ..LIMITS }
}

/// For engine tests on machines with little free room: the room seeding keeps free.
#[cfg(feature = "test-support")]
pub fn keep_free_for_tests(bytes: u64) {
    KEEP_FREE.store(bytes, std::sync::atomic::Ordering::Relaxed);
}

/// What a file takes on the drive beyond its bytes, counted for the room check (one cluster).
const PER_FILE: u64 = 4096;
/// At most this many folders are read to find a checkout's `Cargo.toml` files.
const MANIFEST_SEARCH: usize = 5_000;
/// Beside the copies: held by the one seeding running in any Faber on this data folder, and by the leftover sweep.
const LOCK: &str = ".seed.lock";
/// The lockfiles npm, pnpm, Yarn and Bun keep; `node_modules` is reused only when the copy's match the project's.
const NODE_LOCKS: [&str; 6] = ["package-lock.json", "npm-shrinkwrap.json", "pnpm-lock.yaml", "yarn.lock", "bun.lock", "bun.lockb"];
/// What a profile folder keeps besides the dependencies' entries in them.
const PROFILE_KEEP: [&str; 4] = [".fingerprint", "build", "deps", "incremental"];
/// The folders whose files and folders cargo makes targets of without a manifest naming them.
const FOUND_TARGETS: [&str; 4] = ["src/bin", "tests", "benches", "examples"];

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
/// The copies being seeded or waiting their turn, so the copy sweep never retires one meanwhile.
static SEEDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// What a new copy can start from.
#[derive(Debug)]
pub struct Plan {
    root: PathBuf,
    /// The project's lockfiles when the plan was made: `node_modules` moves in only while both sides still hold these.
    locks: Locks,
    copy: PathBuf,
    staging: PathBuf,
    items: Vec<Item>,
    /// The names of the project's own packages and their library and program targets, in both spellings.
    own: BTreeSet<String>,
}

#[derive(Debug)]
struct Item {
    from: PathBuf,
    to: PathBuf,
    cargo: bool,
}

/// What happened to one folder, for the log line.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Copied and moved into place, with the room it was counted at.
    Seeded(u64),
    /// The copy already had the folder (a build started first): it kept its own.
    InUse,
    /// Above the size limit or the room left, or it grew past it while being copied.
    TooBig,
    /// Not enough room left on the drive, or the room is unknown.
    NoRoom,
    /// It holds a link or a special file; a link could lead a copy of it into the project.
    Links,
    /// Something could not be read or written, or the copy changed under it; nothing was moved into place.
    Failed,
}

fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// A link of any kind: a symbolic link, or on Windows any reparse point (junctions, cloud placeholders).
fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    let reparse = std::os::windows::fs::MetadataExt::file_attributes(meta) & 0x400 != 0;
    #[cfg(not(windows))]
    let reparse = false;
    meta.file_type().is_symlink() || reparse
}

fn plain_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !is_link(&m))
}

fn plain_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !is_link(&m))
}

/// Every name a checkout's own crates can leave in a `target`: each package's name and its library, program, test, bench
/// and example names, named in the manifest or found by cargo in `src/bin`, `tests`, `benches` and `examples`, with
/// dashes and with underscores. `None` when a manifest or a name cannot be read or the search ran out: a crate left
/// unfound would keep the project's build of it, so no `target` is seeded then.
fn own_names(top: &Path) -> Option<BTreeSet<String>> {
    let mut manifests = vec![];
    let mut stack = vec![top.to_path_buf()];
    let mut read = 0;
    while let Some(dir) = stack.pop() {
        read += 1;
        if read > MANIFEST_SEARCH {
            return None;
        }
        for entry in std::fs::read_dir(&dir).ok()? {
            let entry = entry.ok()?;
            if matches!(entry.file_name().to_str(), Some(".git" | "target" | "node_modules")) {
                continue;
            }
            if plain_dir(&entry.path()) {
                stack.push(entry.path());
            } else if entry.file_name() == "Cargo.toml" && plain_file(&entry.path()) {
                manifests.push(entry.path());
            }
        }
    }
    let mut names = BTreeSet::new();
    let mut add = |n: &str| {
        names.insert(n.replace('-', "_"));
        names.insert(n.to_string());
    };
    for manifest in manifests {
        let table: toml::Table = std::fs::read_to_string(&manifest).ok()?.parse().ok()?;
        let name = |value: Option<&toml::Value>| value.and_then(|v| v.get("name")).and_then(toml::Value::as_str).map(str::to_string);
        for n in [name(table.get("package")), name(table.get("lib"))].into_iter().flatten() {
            add(&n);
        }
        for kind in ["bin", "test", "bench", "example"] {
            for target in table.get(kind).and_then(toml::Value::as_array).into_iter().flatten() {
                if let Some(n) = name(Some(target)) {
                    add(&n);
                }
            }
        }
        let dir = manifest.parent()?;
        for found in FOUND_TARGETS {
            let Ok(entries) = std::fs::read_dir(dir.join(found)) else { continue };
            for entry in entries {
                let entry = entry.ok()?;
                let file = entry.file_name();
                let file = file.to_str()?;
                add(file.strip_suffix(".rs").unwrap_or(file));
            }
        }
    }
    Some(names)
}

type Locks = Vec<Option<Vec<u8>>>;

/// The top lockfiles of a checkout, each read or absent; `None` when one is there but is not a plain, readable file.
fn locks(top: &Path) -> Option<Locks> {
    NODE_LOCKS
        .iter()
        .map(|name| match std::fs::symlink_metadata(top.join(name)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Some(None),
            Ok(_) if plain_file(&top.join(name)) => std::fs::read(top.join(name)).ok().map(Some),
            _ => None,
        })
        .collect()
}

/// The canonical folder of conversation `id`'s copy, reached without a link on the way.
fn own_copy(data: &Path, id: &str) -> Option<PathBuf> {
    let path = home(data).join(id);
    real_path(&path).ok()?;
    path.canonicalize().ok()
}

/// What the new copy of conversation `id`, at `copy` and made from the checkout at `root`, can start from. `None` when
/// the project has no build folder to start from.
pub fn plan(data: &Path, id: &str, root: &Path, copy: &Path) -> Option<Plan> {
    if !valid_id(id) || !plain_dir(root) || !plain_dir(copy) {
        return None;
    }
    // Only a copy in Faber's own folder of copies, never the project itself or a folder that holds it.
    let (Ok(source), Ok(target), Some(own)) = (root.canonicalize(), copy.canonicalize(), own_copy(data, id)) else { return None };
    if target != own || source == target || source.starts_with(&target) {
        return None;
    }
    let mut items = vec![];
    let mut names = None;
    let project_locks = locks(&source);
    for (name, manifest, cargo) in [("target", "Cargo.toml", true), ("node_modules", "package.json", false)] {
        let (from, to) = (source.join(name), own.join(name));
        // A folder git checked out into the copy is tracked: it is never replaced or added to.
        if !plain_file(&own.join(manifest)) || !plain_dir(&from) || std::fs::symlink_metadata(&to).is_ok() {
            continue;
        }
        if cargo {
            // A package the project has and the copy's commit lacks left its build in the project's `target` too.
            names = own_names(&own).zip(own_names(&source)).map(|(mut copy, project)| {
                copy.extend(project);
                copy
            });
            if names.is_none() {
                continue;
            }
        } else if !locks(&own).is_some_and(|copy| Some(&copy) == project_locks.as_ref() && copy.iter().any(Option::is_some)) {
            continue;
        }
        items.push(Item { from, to, cargo });
    }
    if items.is_empty() {
        return None;
    }
    Some(Plan {
        root: source,
        locks: project_locks.unwrap_or_default(),
        copy: own,
        staging: home(data).join(format!(".seed-{id}")),
        own: names.unwrap_or_default(),
        items,
    })
}

/// Whether a link at `parent` (relative to the copied folder's top) reading `to` is one of npm's `.bin` links that stays
/// inside: relative, in a `.bin` folder, and resolving only through plain folders, since it never steps into a `.bin`
/// folder, where the only links are. Judged on the words of the link alone, read once, so the answer cannot change after.
fn bin_link(parent: &Path, to: &Path) -> bool {
    // Spelled any way: on a volume that ignores case, `.BIN` is the same folder. A name that is not text is taken for one
    // on the way, never as the folder the link sits in.
    let bin = |name: &std::ffi::OsStr| name.to_str().is_none_or(|n| n.eq_ignore_ascii_case(".bin"));
    let named_bin = |name: &std::ffi::OsStr| name.to_str().is_some_and(|n| n.eq_ignore_ascii_case(".bin"));
    if cfg!(windows) || parent.file_name().is_none_or(|n| !named_bin(n)) {
        return false;
    }
    let mut at: Vec<&std::ffi::OsStr> = vec![];
    for part in parent.components() {
        match part {
            Component::Normal(name) => at.push(name),
            _ => return false,
        }
    }
    for part in to.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if at.pop().is_none() {
                    return false;
                }
            }
            Component::Normal(name) if !bin(name) && at.last().is_none_or(|last| !bin(last)) => at.push(name),
            _ => return false,
        }
    }
    true
}

/// A folder that is not a link and whose real place is still inside `top`: checked right before each folder is read, so a
/// folder or one above it swapped for a link since is not followed.
fn still_plain(dir: &Path, top: &Path) -> bool {
    plain_dir(dir) && dir.canonicalize().is_ok_and(|real| real.starts_with(top))
}

/// The room `root` takes. A link or a special file stops it, but npm's `.bin` links outside a cargo folder.
fn measure(root: &Path, largest: u64, cargo: bool) -> Result<u64, Outcome> {
    let mut total = 0u64;
    let top = root.canonicalize().map_err(|_| Outcome::Failed)?;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if !still_plain(&dir, &top) {
            return Err(Outcome::Links);
        }
        let relative = dir.strip_prefix(root).map_err(|_| Outcome::Failed)?.to_path_buf();
        for entry in std::fs::read_dir(&dir).map_err(|_| Outcome::Failed)? {
            let entry = entry.map_err(|_| Outcome::Failed)?;
            let meta = std::fs::symlink_metadata(entry.path()).map_err(|_| Outcome::Failed)?;
            if is_link(&meta) {
                let to = std::fs::read_link(entry.path()).map_err(|_| Outcome::Links)?;
                if cargo || !bin_link(&relative, &to) {
                    return Err(Outcome::Links);
                }
            } else if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                total = total.saturating_add(meta.len()).saturating_add(PER_FILE);
                if total > largest {
                    return Err(Outcome::TooBig);
                }
            } else {
                return Err(Outcome::Links);
            }
        }
    }
    Ok(total)
}

/// Copies `from` into the new folder `to`, keeping file times, and stops when it would take more than `budget`. `at` is
/// where `to` sits in the copied folder, whose real place is `top`; only npm's `.bin` links are made again, and only
/// when `links`.
fn copy_tree(from: &Path, to: &Path, top: &Path, at: &Path, links: bool, budget: &mut u64) -> io::Result<()> {
    // Checked again here: the folder, or one above it, may have become a link since it was measured.
    if !still_plain(from, top) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (source, dest) = (entry.path(), to.join(entry.file_name()));
        let meta = std::fs::symlink_metadata(&source)?;
        if is_link(&meta) {
            let words = std::fs::read_link(&source)?;
            if !links || !bin_link(at, &words) {
                return Err(io::ErrorKind::InvalidData.into());
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(&words, &dest)?;
            #[cfg(not(unix))]
            return Err(io::ErrorKind::Unsupported.into());
        } else if meta.is_dir() {
            copy_tree(&source, &dest, top, &at.join(entry.file_name()), links, budget)?;
        } else {
            // Opened as a regular file, never through a link and never waiting on a pipe that took its place.
            let mut input = faber_winproc::open_regular(&source)?;
            let held = input.metadata()?;
            let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&dest)?;
            // Never more than the budget, even from a file that grows while it is read.
            let cap = *budget;
            let copied = io::copy(&mut input.by_ref().take(cap.saturating_add(1)), &mut output)?;
            *budget = cap.checked_sub(copied.saturating_add(PER_FILE)).ok_or(io::ErrorKind::FileTooLarge)?;
            // Cargo orders a build's pieces by their file times: fresh times would make dependencies look newer than
            // what was built on them and build them again.
            output.set_modified(held.modified()?)?;
            // On Windows the only permission is read-only, which would keep the staging folder from being removed.
            #[cfg(unix)]
            output.set_permissions(held.permissions())?;
        }
    }
    Ok(())
}

/// Whether `name` (a fingerprint, build, incremental or compiled entry) belongs to one of the project's own crates:
/// `<name>-<hash>` (hex from cargo, base 36 from rustc's incremental folders), with an optional `lib` in front and
/// anything after a dot.
fn owned(name: &str, own: &BTreeSet<String>) -> bool {
    let base = name.split('.').next().unwrap_or(name);
    base.rsplit_once('-').is_some_and(|(stem, hash)| {
        !hash.is_empty()
            && hash.bytes().all(|b| b.is_ascii_alphanumeric())
            && (own.contains(stem) || stem.strip_prefix("lib").is_some_and(|s| own.contains(s)))
    })
}

/// Removes from a copied `target` what the project's own crates left in each profile folder: at its top everything but
/// the four folders that hold the dependencies' pieces (finished programs, libraries, `.dSYM`, examples), and in those
/// folders the entries of its own crates. A profile is any folder that holds a `.fingerprint`, found in the staged copy
/// itself, so one a build made while the project's folder was copied is cleaned too. The staged copy is Faber's own and
/// holds no link.
fn scrub(target: &Path, own: &BTreeSet<String>) -> io::Result<()> {
    let mut profiles = vec![];
    let mut stack = vec![target.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if plain_dir(&entry.path()) {
                if entry.file_name() == ".fingerprint" {
                    profiles.push(dir.clone());
                }
                stack.push(entry.path());
            }
        }
    }
    for profile in profiles {
        // A profile inside a folder an earlier one removed (cargo run by a build script of the project's own) is gone.
        if std::fs::symlink_metadata(&profile).is_err() {
            continue;
        }
        if !plain_dir(&profile) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        for entry in std::fs::read_dir(&profile)? {
            let entry = entry?;
            if PROFILE_KEEP.iter().any(|keep| entry.file_name() == *keep) && plain_dir(&entry.path()) {
                for inner in std::fs::read_dir(entry.path())? {
                    let inner = inner?;
                    if inner.file_name().to_str().is_none_or(|name| owned(name, own)) {
                        remove(&inner.path())?;
                    }
                }
            } else {
                remove(&entry.path())?;
            }
        }
    }
    Ok(())
}

fn remove(path: &Path) -> io::Result<()> {
    if plain_dir(path) { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) }
}

/// Whether the copy at `copy` is being seeded or waits its turn, so the sweep leaves it in place.
pub fn seeding(copy: &Path) -> bool {
    // Marked by its canonical place: a short Windows name or a `..` on the way still names the same copy.
    let copy = copy.canonicalize().unwrap_or_else(|_| copy.to_path_buf());
    SEEDING.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|p| same_path(p, &copy))
}

/// Right before a folder is moved in: the copy is still where it was, reached without a link, and for `node_modules`
/// its lockfiles are still the project's (the agent may have changed them while the copy was made).
fn still_fits(plan: &Plan, item: &Item) -> bool {
    real_path(&plan.copy).is_ok()
        && plan.copy.canonicalize().is_ok_and(|p| p == plan.copy)
        && (item.cargo || [&plan.root, &plan.copy].iter().all(|top| locks(top).as_ref() == Some(&plan.locks)))
}

/// A seeding to come: from [`begin`] until it is dropped, the copy counts as being seeded, so the sweep leaves it alone
/// even before its thread runs.
#[derive(Debug)]
pub struct Seeding(Plan);

/// Marks the copy of `plan` as being seeded, before the thread that seeds it starts.
pub fn begin(plan: Plan) -> Seeding {
    SEEDING.lock().unwrap_or_else(|e| e.into_inner()).push(plan.copy.clone());
    Seeding(plan)
}

impl Drop for Seeding {
    fn drop(&mut self) {
        let mut seeding = SEEDING.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = seeding.iter().position(|p| same_path(p, &self.0.copy)) {
            seeding.remove(at);
        }
    }
}

impl Seeding {
    /// Copies each folder of the plan into place. `free` tells the room left on the drive that holds a path. Runs on its
    /// own thread, after the copy is made, so the answer never waits for it.
    pub fn run(self, limits: Limits, free: impl Fn(&Path) -> Option<u64>) -> Vec<(PathBuf, Outcome)> {
        let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        fill(&self.0, limits, free)
    }
}

/// [`begin`] and [`Seeding::run`] at once.
pub fn run(plan: Plan, limits: Limits, free: impl Fn(&Path) -> Option<u64>) -> Vec<(PathBuf, Outcome)> {
    begin(plan).run(limits, free)
}

/// The lock beside the copies, opened (and made when `make`), never through a link.
fn open_lock(copies: &Path, make: bool) -> Option<File> {
    if real_path(copies).is_err() || !plain_dir(copies) {
        return None;
    }
    let path = copies.join(LOCK);
    if std::fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || is_link(&m)) {
        return None;
    }
    std::fs::OpenOptions::new().read(true).write(true).create(make).truncate(false).open(path).ok()
}

fn fill(plan: &Plan, limits: Limits, free: impl Fn(&Path) -> Option<u64>) -> Vec<(PathBuf, Outcome)> {
    let failed = || plan.items.iter().map(|item| (item.to.clone(), Outcome::Failed)).collect();
    let copies = plan.staging.parent().unwrap_or(&plan.staging);
    // Held until the staging folder is gone: another seeding waits, and a sweep leaves the folder alone.
    let Some(lock) = open_lock(copies, true) else { return failed() };
    if lock.lock().is_err() {
        return failed();
    }
    if std::fs::symlink_metadata(&plan.staging).is_ok() && (!plain_dir(&plan.staging) || std::fs::remove_dir_all(&plan.staging).is_err()) {
        return failed();
    }
    if std::fs::create_dir(&plan.staging).is_err() {
        return failed();
    }
    let mut results = vec![];
    for (index, item) in plan.items.iter().enumerate() {
        let outcome = match measure(&item.from, limits.largest, item.cargo) {
            Err(outcome) => outcome,
            Ok(size) => match free(copies) {
                Some(room) if room >= size.saturating_add(limits.keep_free) => {
                    let staged = plan.staging.join(index.to_string());
                    let mut budget = limits.largest.min(room - limits.keep_free);
                    let filled = item
                        .from
                        .canonicalize()
                        .and_then(|top| copy_tree(&item.from, &staged, &top, Path::new(""), !item.cargo, &mut budget))
                        .and_then(|()| if item.cargo { scrub(&staged, &plan.own) } else { Ok(()) });
                    match filled {
                        Err(e) if e.kind() == io::ErrorKind::FileTooLarge => Outcome::TooBig,
                        Err(_) => Outcome::Failed,
                        // Small files can take more room than counted (a volume with large clusters): what is left
                        // after the copy decides.
                        Ok(()) if free(copies).is_none_or(|left| left < limits.keep_free) => Outcome::NoRoom,
                        Ok(()) if !still_fits(plan, item) => Outcome::Failed,
                        Ok(()) => match faber_winproc::rename_new(&staged, &item.to) {
                            Ok(()) => Outcome::Seeded(size),
                            // A build the agent started first keeps its own folder, even an empty one.
                            Err(e) if e.kind() == io::ErrorKind::AlreadyExists || std::fs::symlink_metadata(&item.to).is_ok() => {
                                Outcome::InUse
                            }
                            Err(_) => Outcome::Failed,
                        },
                    }
                }
                _ => Outcome::NoRoom,
            },
        };
        // A folder that did not move in gives its room back before the next one is counted.
        let _ = std::fs::remove_dir_all(plan.staging.join(index.to_string()));
        results.push((item.to.clone(), outcome));
    }
    let _ = std::fs::remove_dir_all(&plan.staging);
    drop(lock);
    results
}

/// Removes staging folders a seeding interrupted by a crash or a quit left beside the copies. While a seeding holds the
/// lock, in this Faber or another, nothing is removed; with no lock file no seeding ever ran, so nothing was left.
pub fn sweep_leftovers(data: &Path) {
    let copies = home(data);
    let Some(lock) = open_lock(&copies, false) else { return };
    if lock.try_lock().is_err() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&copies) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_str().and_then(|n| n.strip_prefix(".seed-")).is_some_and(valid_id) && plain_dir(&entry.path()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        data: PathBuf,
        root: PathBuf,
        copy: PathBuf,
    }

    const ID: &str = "c0ffee00-0000-4000-8000-000000000001";

    fn built() -> std::time::SystemTime {
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000)
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A project with a Rust crate `app` (a program `app-cli`, a test cargo finds by itself, one dependency `serde`
    /// already built, a member deep in the tree) and `node_modules` with its lockfile, and its new copy holding only the
    /// tracked files. The project also has a member the copy's older commit lacks.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        // macOS keeps temporary folders behind a link (`/var` is `/private/var`), and a copy reached through a link is
        // never seeded.
        let top = if cfg!(unix) { dir.path().canonicalize().unwrap() } else { dir.path().to_path_buf() };
        let data = top.join("data");
        let root = top.join("project");
        let copy = home(&data).join(ID);
        for top in [&root, &copy] {
            write(
                &top.join("Cargo.toml"),
                "[package] # the app\nname = \"app\" # its name\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"app-cli\"\npath = \"src/main.rs\"\n\n[dependencies]\nserde = \"1\"\n",
            );
            write(&top.join("src/main.rs"), "fn main() {}\n");
            write(&top.join("tests/smoke.rs"), "#[test]\nfn smoke() {}\n");
            write(&top.join("crates/core/parts/inner/Cargo.toml"), "[package]\nname = \"inner-part\"\nversion = \"0.1.0\"\n");
            write(&top.join("package.json"), "{}\n");
            write(&top.join("package-lock.json"), "{\"lockfileVersion\": 3}\n");
        }
        write(&root.join("crates/newer/Cargo.toml"), "[package]\nname = \"newer\"\nversion = \"0.1.0\"\n");
        let debug = root.join("target/debug");
        write(&debug.join(".fingerprint/app-0123456789abcdef/bin-app"), "own");
        write(&debug.join(".fingerprint/inner-part-00112233aabbccdd/lib-inner_part"), "own member");
        write(&debug.join(".fingerprint/newer-44556677aabbccdd/lib-newer"), "a member only the project has");
        write(&debug.join(".fingerprint/serde-fedcba9876543210/lib-serde"), "dependency");
        write(&debug.join("build/app-0123456789abcdef/output"), "own build script");
        write(&debug.join("build/serde-fedcba9876543210/output"), "dependency build script");
        write(&debug.join("deps/libserde-fedcba9876543210.rlib"), "dependency library");
        write(&debug.join("deps/app_cli-0123456789abcdef.exe"), "the project's program, compiled");
        write(&debug.join("deps/smoke-0123456789abcdef.exe"), "the project's test, compiled");
        write(&debug.join("deps/libinner_part-00112233aabbccdd.rlib"), "own member library");
        write(&debug.join("incremental/app_cli-1zx9q8/s-1/dep-graph.bin"), "own incremental, named in base 36");
        write(&debug.join("app-cli.exe"), "the project's program");
        write(&debug.join("app-cli.d"), "dep info");
        write(&debug.join("examples/demo.exe"), "example");
        // A custom `build.target-dir` inside target: its profile is found too.
        write(&root.join("target/cache/ci/debug/.fingerprint/app-0123456789abcdef/bin-app"), "own, elsewhere");
        write(&root.join("target/cache/ci/debug/app-cli.exe"), "the project's program, elsewhere");
        write(&root.join("target/CACHEDIR.TAG"), "tag");
        write(&root.join("node_modules/left-pad/index.js"), "module.exports = 1;\n");
        let rlib = std::fs::OpenOptions::new().write(true).open(debug.join("deps/libserde-fedcba9876543210.rlib")).unwrap();
        rlib.set_modified(built()).unwrap();
        Fixture { _dir: dir, data, root, copy }
    }

    fn roomy(_: &Path) -> Option<u64> {
        Some(u64::MAX)
    }

    fn seed(f: &Fixture, limits: Limits, free: impl Fn(&Path) -> Option<u64>) -> Vec<(PathBuf, Outcome)> {
        run(plan(&f.data, ID, &f.root, &f.copy).expect("a plan"), limits, free)
    }

    fn outcome_of<'a>(results: &'a [(PathBuf, Outcome)], name: &str) -> Option<&'a Outcome> {
        results.iter().find(|(to, _)| to.ends_with(name)).map(|(_, outcome)| outcome)
    }

    #[test]
    fn a_new_copy_starts_from_the_project_build_with_only_downloaded_dependencies_kept() {
        let f = fixture();
        let results = seed(&f, LIMITS, roomy);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, outcome)| matches!(outcome, Outcome::Seeded(_))), "{results:?}");
        let debug = f.copy.join("target/debug");
        assert!(debug.join(".fingerprint/serde-fedcba9876543210/lib-serde").is_file(), "the dependency stays built");
        assert!(debug.join("build/serde-fedcba9876543210/output").is_file());
        let rlib = std::fs::metadata(debug.join("deps/libserde-fedcba9876543210.rlib")).unwrap();
        assert_eq!(rlib.modified().unwrap(), built(), "built pieces keep their times, or cargo would build them again");
        assert!(f.copy.join("target/CACHEDIR.TAG").is_file());
        // The copy's code may differ from the project's: its own crates must build again, and nothing of the project's
        // own build can run or be reused in the copy before they do.
        for gone in [
            ".fingerprint/app-0123456789abcdef",
            ".fingerprint/inner-part-00112233aabbccdd",
            ".fingerprint/newer-44556677aabbccdd",
            "build/app-0123456789abcdef",
            "deps/app_cli-0123456789abcdef.exe",
            "deps/smoke-0123456789abcdef.exe",
            "deps/libinner_part-00112233aabbccdd.rlib",
            "incremental/app_cli-1zx9q8",
            "app-cli.exe",
            "app-cli.d",
            "examples",
        ] {
            assert!(!debug.join(gone).exists(), "{gone} is the project's own");
        }
        let elsewhere = f.copy.join("target/cache/ci/debug");
        assert!(!elsewhere.join(".fingerprint/app-0123456789abcdef").exists() && !elsewhere.join("app-cli.exe").exists());
        assert!(f.copy.join("node_modules/left-pad/index.js").is_file());
        assert!(f.root.join("target/debug/app-cli.exe").is_file(), "the project's own folder is untouched");
        assert!(!home(&f.data).join(format!(".seed-{ID}")).exists(), "no staging folder is left");
    }

    #[test]
    fn a_build_the_agent_started_first_keeps_its_own_folder_even_an_empty_one() {
        let f = fixture();
        let plan = plan(&f.data, ID, &f.root, &f.copy).unwrap();
        // The agent's first build made `target` after the plan and before the folder was moved into place.
        std::fs::create_dir(f.copy.join("target")).unwrap();
        let results = run(plan, LIMITS, roomy);
        assert_eq!(outcome_of(&results, "target"), Some(&Outcome::InUse), "{results:?}");
        assert_eq!(std::fs::read_dir(f.copy.join("target")).unwrap().count(), 0, "nothing was moved over or into it");
    }

    #[test]
    fn node_modules_needs_the_same_lockfile_then_and_right_before_the_move() {
        let f = fixture();
        write(&f.copy.join("package-lock.json"), "{\"lockfileVersion\": 3, \"older\": true}\n");
        let results = seed(&f, LIMITS, roomy);
        assert_eq!(results.len(), 1, "a copy from another commit installs its own packages: {results:?}");
        let f = fixture();
        std::fs::remove_file(f.root.join("package-lock.json")).unwrap();
        std::fs::remove_file(f.copy.join("package-lock.json")).unwrap();
        assert_eq!(seed(&f, LIMITS, roomy).len(), 1, "without a lockfile the versions are unknown");
        let f = fixture();
        write(&f.copy.join("node_modules/tracked.js"), "checked out by git");
        assert_eq!(seed(&f, LIMITS, roomy).len(), 1);
        assert!(!f.copy.join("node_modules/left-pad").exists());
        // The agent changed the lockfile while the copy was made.
        let f = fixture();
        let plan = plan(&f.data, ID, &f.root, &f.copy).unwrap();
        write(&f.copy.join("package-lock.json"), "{\"lockfileVersion\": 3, \"upgraded\": true}\n");
        let results = run(plan, LIMITS, roomy);
        assert_eq!(outcome_of(&results, "node_modules"), Some(&Outcome::Failed), "{results:?}");
        assert!(!f.copy.join("node_modules").exists());
        // The agent upgraded the packages on both sides alike while the older ones were copied.
        let f = fixture();
        let plan = super::plan(&f.data, ID, &f.root, &f.copy).unwrap();
        for top in [&f.root, &f.copy] {
            write(&top.join("package-lock.json"), "{\"lockfileVersion\": 3, \"upgraded\": true}\n");
        }
        assert_eq!(outcome_of(&run(plan, LIMITS, roomy), "node_modules"), Some(&Outcome::Failed));
    }

    #[test]
    fn an_unreadable_manifest_in_the_copy_or_the_project_seeds_no_target() {
        let f = fixture();
        write(&f.copy.join("crates/core/parts/inner/Cargo.toml"), "[package\nname = ");
        let results = seed(&f, LIMITS, roomy);
        assert!(results.iter().all(|(to, _)| to.ends_with("node_modules")), "{results:?}");
        let f = fixture();
        write(&f.root.join("crates/newer/Cargo.toml"), "[package\nname = ");
        assert!(seed(&f, LIMITS, roomy).iter().all(|(to, _)| to.ends_with("node_modules")));
    }

    #[test]
    fn too_big_or_too_little_room_seeds_nothing() {
        let f = fixture();
        assert!(seed(&f, Limits { largest: 10, keep_free: 0 }, roomy).iter().all(|(_, o)| *o == Outcome::TooBig));
        assert!(seed(&f, LIMITS, |_| Some(LIMITS.keep_free)).iter().all(|(_, o)| *o == Outcome::NoRoom));
        assert!(seed(&f, LIMITS, |_| None).iter().all(|(_, o)| *o == Outcome::NoRoom), "an unknown room is not room");
        // The room was there when counted, and the copy took more of it than counted.
        let asked = std::cell::Cell::new(0);
        let shrinking = |_: &Path| {
            asked.set(asked.get() + 1);
            Some(if asked.get() % 2 == 1 { u64::MAX } else { LIMITS.keep_free - 1 })
        };
        assert!(seed(&f, LIMITS, shrinking).iter().all(|(_, o)| *o == Outcome::NoRoom));
        // A `target` that took the room and was discarded gives it back before `node_modules` is counted.
        let g = fixture();
        let staging = home(&g.data).join(format!(".seed-{ID}"));
        let back = |_: &Path| Some(if staging.join("0").exists() { LIMITS.keep_free - 1 } else { u64::MAX });
        let results = seed(&g, LIMITS, back);
        assert_eq!(outcome_of(&results, "target"), Some(&Outcome::NoRoom), "{results:?}");
        assert!(matches!(outcome_of(&results, "node_modules"), Some(Outcome::Seeded(_))), "{results:?}");
        assert!(!f.copy.join("target").exists() && !f.copy.join("node_modules").exists());
    }

    #[test]
    fn the_copy_takes_no_more_than_the_room_above_what_stays_free() {
        let mut budget = 10;
        let f = fixture();
        let to = f.copy.join("staged");
        // The folder is measured small, then grows: the copy stops at the budget instead of eating the room kept free.
        let grown = f.root.join("grown");
        write(&grown.join("big.bin"), &"x".repeat(64));
        let copied = copy_tree(&grown, &to, &grown.canonicalize().unwrap(), Path::new(""), false, &mut budget);
        assert_eq!(copied.map_err(|e| e.kind()), Err(io::ErrorKind::FileTooLarge));
        assert!(std::fs::metadata(to.join("big.bin")).unwrap().len() <= 11, "it never read past the budget");
    }

    #[test]
    fn a_project_without_build_folders_or_a_copy_outside_faber_has_no_plan() {
        let f = fixture();
        std::fs::remove_dir_all(f.root.join("target")).unwrap();
        std::fs::remove_dir_all(f.root.join("node_modules")).unwrap();
        assert!(plan(&f.data, ID, &f.root, &f.copy).is_none());
        let f = fixture();
        assert!(plan(&f.data, "../escape", &f.root, &f.copy).is_none());
        assert!(plan(&f.data, ID, &f.root, &f.root).is_none(), "the project is never its own copy");
    }

    #[test]
    fn only_npm_bin_links_that_stay_inside_through_plain_folders_are_kept() {
        let ok = |at: &str, to: &str| bin_link(Path::new(at), Path::new(to));
        if cfg!(windows) {
            assert!(!ok(".bin", "../left-pad/index.js"), "no link is ever copied on Windows");
            return;
        }
        assert!(ok(".bin", "../left-pad/index.js"));
        assert!(ok("a/node_modules/.bin", "../../../x/cli.js"));
        assert!(!ok(".bin", "../../outside"), "climbs out of node_modules");
        assert!(!ok(".bin", "/usr/bin/node"));
        assert!(!ok(".bin", "other/../../left-pad"), "steps through another link in .bin first");
        assert!(!ok(".bin", "../a/.bin/other"), "ends on another link");
        assert!(!ok("a/b", "../.."), "only in a .bin folder");
        assert!(!ok(".bin", "../.BIN/other"), "on a volume that ignores case .BIN is .bin");
        assert!(!ok(".BIN", "other/../../x"));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let not_text = Path::new(std::ffi::OsStr::from_bytes(b"\xff"));
            assert!(!bin_link(not_text, Path::new("../pkg/cli.js")), "a folder whose name is not text is not .bin");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_copy_swapped_for_a_junction_into_the_project_receives_nothing() {
        let f = fixture();
        let plan = plan(&f.data, ID, &f.root, &f.copy).unwrap();
        std::fs::remove_dir_all(&f.copy).unwrap();
        std::fs::create_dir(f.root.join("elsewhere")).unwrap();
        junction::create(f.root.join("elsewhere"), &f.copy).unwrap();
        let results = run(plan, LIMITS, roomy);
        assert!(results.iter().all(|(_, o)| *o == Outcome::Failed), "{results:?}");
        assert_eq!(std::fs::read_dir(f.root.join("elsewhere")).unwrap().count(), 0, "nothing landed in the project");
        let f = fixture();
        junction::create(&f.root, f.root.join("node_modules/escape")).unwrap();
        assert_eq!(outcome_of(&seed(&f, LIMITS, roomy), "node_modules"), Some(&Outcome::Links));
    }

    #[cfg(unix)]
    #[test]
    fn bin_links_are_copied_as_links_and_any_other_link_stops_the_seed() {
        let f = fixture();
        let bin = f.root.join("node_modules/.bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink("../left-pad/index.js", bin.join("left-pad")).unwrap();
        let results = seed(&f, LIMITS, roomy);
        assert!(results.iter().all(|(_, outcome)| matches!(outcome, Outcome::Seeded(_))), "{results:?}");
        assert_eq!(std::fs::read_link(f.copy.join("node_modules/.bin/left-pad")).unwrap(), Path::new("../left-pad/index.js"));
        let f = fixture();
        std::fs::create_dir_all(f.root.join("node_modules/a/b")).unwrap();
        std::os::unix::fs::symlink("../..", f.root.join("node_modules/a/b/up")).unwrap();
        assert_eq!(outcome_of(&seed(&f, LIMITS, roomy), "node_modules"), Some(&Outcome::Links), "a relative link out of .bin");
        let f = fixture();
        std::os::unix::fs::symlink("../../..", f.root.join("target/debug/build-link")).unwrap();
        assert_eq!(outcome_of(&seed(&f, LIMITS, roomy), "target"), Some(&Outcome::Links), "no link in target");
    }

    #[test]
    fn a_profile_a_build_made_while_the_folder_was_copied_is_cleaned_too() {
        let f = fixture();
        let staged = f.copy.join("staged");
        write(&staged.join("release/.fingerprint/app-0123456789abcdef/bin-app"), "own, made after the folder was measured");
        write(&staged.join("release/app-cli.exe"), "the project's program, made after the folder was measured");
        write(&staged.join("release/deps/libserde-fedcba9876543210.rlib"), "dependency");
        let own = own_names(&f.copy).unwrap();
        scrub(&staged, &own).unwrap();
        assert!(!staged.join("release/app-cli.exe").exists() && !staged.join("release/.fingerprint/app-0123456789abcdef").exists());
        assert!(staged.join("release/deps/libserde-fedcba9876543210.rlib").is_file());
        // A build script of the project's own ran cargo in its output folder: that profile goes with its folder.
        write(&staged.join("debug/build/app-0123456789abcdef/out/target/debug/.fingerprint/app-0123456789abcdef/bin-app"), "nested");
        write(&staged.join("debug/.fingerprint/serde-fedcba9876543210/lib-serde"), "dependency");
        scrub(&staged, &own).unwrap();
        assert!(!staged.join("debug/build/app-0123456789abcdef").exists());
        assert!(staged.join("debug/.fingerprint/serde-fedcba9876543210/lib-serde").is_file());
    }

    #[test]
    fn a_copy_counts_as_seeding_from_before_its_thread_starts_until_it_ends() {
        let f = fixture();
        let seeding_now = begin(plan(&f.data, ID, &f.root, &f.copy).unwrap());
        assert!(seeding(&f.copy), "the sweep leaves it alone before the thread runs");
        assert!(seeding(&f.copy.join("src").join("..")), "another spelling of the copy's folder names it too");
        drop(seeding_now);
        assert!(!seeding(&f.copy));
    }

    #[test]
    fn a_leftover_staging_folder_is_swept_but_not_while_a_seeding_holds_the_lock() {
        let f = fixture();
        let leftover = home(&f.data).join(".seed-c0ffee00-0000-4000-8000-000000000002");
        write(&leftover.join("0/file"), "from a crash");
        let held = open_lock(&home(&f.data), true).unwrap();
        held.lock().unwrap();
        sweep_leftovers(&f.data);
        assert!(leftover.join("0/file").exists(), "a seeding in this Faber or another holds the lock");
        drop(held);
        sweep_leftovers(&f.data);
        assert!(!leftover.exists());
        assert!(f.copy.join("Cargo.toml").exists(), "copies are never touched");
    }
}
