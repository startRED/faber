//! Settings > General > Storage, "Move tools here". The screen saves a request; the move itself runs at the next start, in
//! [`at_start`], before the engine exists, so nothing that uses a tool can run while it moves
//! (`faber_engine::tools::moving`). The result stays for Storage to show, and the originals stay until the person removes
//! them.
use super::{Choice, inside_a_project, validate};
use crate::{Host, Res, Shell, lock};
use faber_engine::tools::moving;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const REQUEST: &str = "request.json";
const LOCK: &str = "start.lock";
const WAIT: Duration = Duration::from_secs(120);
const RESULT: &str = "result.json";
const LIMIT: u64 = 64 * 1024;
const PROJECTS_LIMIT: u64 = 16 * 1024 * 1024;
/// The chosen folder is not the one asked for, or no longer passes the checks a choice must pass.
const TARGET: &str = "error.storage.moveTarget";
const INTERRUPTED: &str = "error.storage.moveInterrupted";

#[derive(Serialize, Deserialize)]
struct Request {
    /// The `Faber-storage` folder the person chose when asking: a change of choice since then cancels the move.
    target: PathBuf,
}

/// What Storage shows about moving.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    /// Installed tools in the main data folder that a move would take to the chosen folder, and their size.
    movable: usize,
    movable_bytes: u64,
    /// A move waits for the next start.
    pending: bool,
    /// What the originals kept from moves take; None when it could not be read.
    old_bytes: Option<u64>,
    /// How the last move ended.
    last: Option<moving::Outcome>,
}

/// A move's progress for the screen while Faber starts: item `number` of `total`, `copied` of its `bytes`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub number: usize,
    pub total: usize,
    pub copied: u64,
    pub bytes: u64,
    pub done: bool,
}

fn folder(data: &Path) -> PathBuf {
    data.join(moving::FOLDER)
}

fn read_small(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let meta = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        result => result?,
    };
    if !meta.is_file() || meta.len() > limit {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

fn write_small(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    faber_engine::project::lease::real_path(parent).map_err(|_| io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    io::Write::write_all(&mut temp, bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|failure| failure.error)?;
    Ok(())
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn last(data: &Path) -> Option<moving::Outcome> {
    serde_json::from_slice(&read_small(&folder(data).join(RESULT), LIMIT).ok()??).ok()
}

fn save_result(data: &Path, outcome: &moving::Outcome) {
    if let Ok(bytes) = serde_json::to_vec(outcome) {
        let _ = write_small(&folder(data).join(RESULT), &bytes);
    }
}

/// The projects as `projects.json` lists them, read without the engine; None when the file is there but unreadable.
fn projects(data: &Path) -> Option<Vec<PathBuf>> {
    #[derive(Deserialize)]
    struct Project {
        path: String,
    }
    match read_small(&data.join("projects.json"), PROJECTS_LIMIT) {
        Ok(None) => Some(Vec::new()),
        Ok(Some(bytes)) => {
            serde_json::from_slice::<Vec<Project>>(&bytes).ok().map(|list| list.into_iter().map(|p| PathBuf::from(p.path)).collect())
        }
        Err(_) => None,
    }
}

impl Choice {
    /// The chosen `Faber-storage` folder, checked again now as a new choice is: no link, no git work tree, no system folder,
    /// owned by this profile, writable, and inside no project (one added since the choice counts).
    fn move_target(&self, data: &Path) -> Option<PathBuf> {
        let folder = self.saved.folder.as_ref()?;
        if !self.status.custom || self.status.locked || self.status.folder == data {
            return None;
        }
        let root = validate(folder, data, &self.saved.roots, false)?;
        let projects = projects(data)?;
        let chosen = root.parent()?;
        // `Faber-storage` itself counts: a project added on it since would get the tools inside it.
        let in_a_project = [chosen, folder.as_path(), root.as_path()].into_iter().any(|path| inside_a_project(path, &projects));
        (root == self.status.folder && !in_a_project).then_some(root)
    }
}

/// Before the engine opens: puts right a move a crash cut, then runs the one the person asked for, if the chosen folder
/// is still the one asked for and still passes every check. The request goes before anything moves, so a move that keeps
/// failing the start is never tried again by itself.
/// Returns the lock the host keeps for its life ([`hold`]).
pub(crate) fn at_start(data: &Path, choice: &Choice, shell: &dyn Shell) -> Option<fs::File> {
    at_start_with(data, choice, shell, &moving::Disk::real())
}

/// Held by each host for its whole life: a start recovers or moves only once no other Faber on the same data folder runs
/// (one still closing as the next opens), so no running engine meets a moving tool. One that does not let go within
/// [`WAIT`] means two Faber run at once: this start then moves and recovers nothing, and opens anyway.
fn hold(data: &Path) -> io::Result<fs::File> {
    let folder = folder(data);
    faber_engine::project::lease::real_path(&folder).map_err(|_| io::ErrorKind::InvalidInput)?;
    fs::create_dir_all(&folder)?;
    let file = fs::File::options().read(true).write(true).create(true).truncate(false).open(folder.join(LOCK))?;
    // Nothing to recover or move: no reason to make the start wait for another Faber.
    let pending = [REQUEST, "journal.json", "leftovers.json"].iter().any(|name| fs::symlink_metadata(folder.join(name)).is_ok());
    let until = Instant::now() + if pending { WAIT } else { Duration::ZERO };
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) if Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
            Err(fs::TryLockError::WouldBlock) => return Err(io::ErrorKind::WouldBlock.into()),
            Err(fs::TryLockError::Error(error)) => return Err(error),
        }
    }
}

fn at_start_with(data: &Path, choice: &Choice, shell: &dyn Shell, disk: &moving::Disk) -> Option<fs::File> {
    let held = hold(data).ok()?;
    start_move(data, choice, shell, disk);
    Some(held)
}

fn start_move(data: &Path, choice: &Choice, shell: &dyn Shell, disk: &moving::Disk) {
    let request_path = folder(data).join(REQUEST);
    match moving::recover(data, disk) {
        Ok(None) => {}
        Ok(Some(outcome)) => {
            let _ = remove(&request_path);
            save_result(data, &outcome);
            return;
        }
        // Left for the next start, and said in Storage; nothing new moves meanwhile.
        Err(_) => {
            save_result(data, &moving::Outcome { stopped: Some(INTERRUPTED.into()), ..moving::Outcome::default() });
            return;
        }
    }
    // A shell that could not show the move leaves the request for one that can.
    if !shell.moves_tools() {
        return;
    }
    let Ok(Some(bytes)) = read_small(&request_path, LIMIT) else { return };
    if remove(&request_path).is_err() {
        return;
    }
    let target = serde_json::from_slice::<Request>(&bytes).ok().map(|request| request.target);
    let outcome = match choice.move_target(data) {
        Some(root) if target.as_ref() == Some(&root) => run(data, &root, shell, disk),
        _ => moving::Outcome { stopped: Some(TARGET.into()), ..moving::Outcome::default() },
    };
    save_result(data, &outcome);
}

fn run(data: &Path, root: &Path, shell: &dyn Shell, disk: &moving::Disk) -> moving::Outcome {
    let mut progress = Progress::default();
    let mut shown = Instant::now();
    let result = moving::run(data, root, disk, &mut |step| {
        match step {
            moving::Step::Item { number, total, bytes, .. } => {
                progress = Progress { number, total, copied: 0, bytes, done: false };
                shown = Instant::now();
                shell.storage_move(progress);
            }
            // Copying and reading back alike keep the line coming, so a screen that starts listening late still shows it.
            step if shown.elapsed() >= Duration::from_millis(250) => {
                if let moving::Step::Copied(copied) = step {
                    progress.copied = copied;
                }
                shown = Instant::now();
                shell.storage_move(progress);
            }
            moving::Step::Copied(copied) => progress.copied = copied,
            _ => {}
        }
        Ok(())
    });
    shell.storage_move(Progress { done: true, ..progress });
    match result {
        Ok(outcome) => outcome,
        // An error the move could not undo by itself: the same recovery as after a crash, now or at the next start.
        Err(_) => match moving::recover(data, disk) {
            Ok(Some(outcome)) => outcome,
            _ => moving::Outcome { stopped: Some(INTERRUPTED.into()), ..moving::Outcome::default() },
        },
    }
}

fn target_now(h: &Host) -> Option<PathBuf> {
    let choice = lock(&h.storage_choice);
    (choice.status.custom && !choice.status.locked && choice.status.folder != h.data_dir).then(|| choice.status.folder.clone())
}

pub(crate) async fn status(h: &Host) -> Res<Status> {
    // A shell that cannot show the move while it starts (the Tauri shell opens its window only after) never offers it.
    let (data, target) = (h.data_dir.clone(), target_now(h).filter(|_| h.shell.moves_tools()));
    tokio::task::spawn_blocking(move || {
        let _ = faber_winproc::background_thread();
        let planned = target.as_ref().map(|target| moving::plan(&data, Some(target)).unwrap_or_default()).unwrap_or_default();
        Status {
            movable: planned.len(),
            movable_bytes: planned.iter().map(|item| item.bytes).sum(),
            pending: fs::symlink_metadata(folder(&data).join(REQUEST)).is_ok(),
            old_bytes: moving::old_bytes(&data),
            last: last(&data),
        }
    })
    .await
    .map_err(|_| "error.storage.move".into())
}

/// Saves the person's request and, when no conversation works and nothing installs, closes Faber the usual way and opens
/// it again. Returns whether Faber is restarting; otherwise the move waits for the next start.
pub(crate) async fn request(h: &Host) -> Res<bool> {
    if !h.shell.moves_tools() {
        return Err("error.storage.move".into());
    }
    let target = target_now(h).ok_or(TARGET)?;
    let data = h.data_dir.clone();
    let saved = {
        let target = target.clone();
        tokio::task::spawn_blocking(move || -> Result<(), &'static str> {
            if moving::plan(&data, Some(&target)).map_err(|_| "error.storage.move")?.is_empty() {
                return Err("error.storage.moveNothing");
            }
            let bytes = serde_json::to_vec(&Request { target }).map_err(|_| "error.store.write")?;
            write_small(&folder(&data).join(REQUEST), &bytes).map_err(|_| "error.store.write")?;
            let _ = remove(&folder(&data).join(RESULT));
            Ok(())
        })
        .await
        .map_err(|_| "error.storage.move")?
    };
    saved?;
    // A change of choice that landed meanwhile cancels the request it no longer matches.
    if target_now(h).as_ref() != Some(&target) {
        let _ = remove(&folder(&h.data_dir).join(REQUEST));
        return Err(TARGET.into());
    }
    Ok(restart(h.engine.is_working(), h.shell.as_ref()))
}

/// The restart for a move happens only when it cuts nothing: no conversation working, no CLI or catalog install running.
/// The shell's own close still keeps a draft (Electron's refused close cancels the restart).
fn restart(working: bool, shell: &dyn Shell) -> bool {
    !working && !crate::installing() && shell.relaunch().is_ok()
}

/// Cancels a move that waits for the next start; a change of choice cancels it too.
pub(crate) fn cancel(data: &Path) -> Res<()> {
    remove(&folder(data).join(REQUEST)).map_err(|_| "error.store.write".into())
}

/// "Remove old copies": deletes the originals moves kept. No tool resolves to them, so nothing waits for a lock.
pub(crate) async fn remove_old(h: &Host) -> Res<()> {
    let data = h.data_dir.clone();
    tokio::task::spawn_blocking(move || {
        let _ = faber_winproc::background_thread();
        moving::remove_old(&data).map_err(|_| "error.storage.moveRemove")?;
        let _ = remove(&folder(&data).join(RESULT));
        Ok(())
    })
    .await
    .map_err(|_| "error.storage.moveRemove")?
}

#[cfg(test)]
mod tests {
    use super::super::{CHOICE_FILE, Saved, write};
    use super::*;
    use std::collections::BTreeSet;

    struct Quiet;
    impl Shell for Quiet {
        fn moves_tools(&self) -> bool {
            true
        }
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        data: PathBuf,
        file: PathBuf,
        chosen: PathBuf,
        root: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = if cfg!(unix) { tempfile::tempdir_in(dirs::home_dir().unwrap()) } else { tempfile::tempdir() }.unwrap();
        let profile = PathBuf::from(faber_engine::project::git::native_path(&tmp.path().canonicalize().unwrap()));
        let data = profile.join("Faber");
        let skill = data.join("tools/plugin/skills/notes");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), b"notes").unwrap();
        fs::write(data.join("tools/installed.json"), br#"{"notes":"c1"}"#).unwrap();
        let chosen = profile.join("Chosen");
        fs::create_dir(&chosen).unwrap();
        let root = validate(&chosen, &data, &BTreeSet::new(), true).unwrap();
        let file = data.with_file_name(CHOICE_FILE);
        write(&file, &Saved { folder: Some(chosen.clone()), roots: BTreeSet::from([root.clone()]) }).unwrap();
        Fixture { _tmp: tmp, data, file, chosen, root }
    }

    fn ask(f: &Fixture, target: &Path) {
        write_small(&folder(&f.data).join(REQUEST), &serde_json::to_vec(&Request { target: target.into() }).unwrap()).unwrap();
    }

    fn moved(f: &Fixture) -> bool {
        f.root.join("tools/plugin/skills/notes/SKILL.md").is_file() && !f.data.join("tools/plugin/skills/notes").exists()
    }

    #[test]
    fn storage_move_runs_at_start_only_for_the_folder_still_chosen() {
        let f = fixture();
        let elsewhere = f.data.parent().unwrap().join("Elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let other = validate(&elsewhere, &f.data, &BTreeSet::new(), true).unwrap();
        // Asked for one folder, then Change picked another before the restart: nothing moves anywhere.
        ask(&f, &other);
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet);
        assert!(!moved(&f) && !other.join("tools").exists());
        assert_eq!(last(&f.data).unwrap().stopped.as_deref(), Some(TARGET));
        assert!(!folder(&f.data).join(REQUEST).exists(), "the request is used once");
        // Asked for the chosen folder: it moves.
        ask(&f, &f.root);
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet);
        assert!(moved(&f));
        assert_eq!(last(&f.data).unwrap().moved, ["notes"]);
    }

    #[test]
    fn storage_move_checks_the_folder_again_when_it_runs() {
        // A project added since the choice now holds the chosen folder.
        let f = fixture();
        let project = serde_json::json!([{ "id": "p1", "name": "Site", "path": f.chosen.parent().unwrap() }]);
        fs::write(f.data.join("projects.json"), serde_json::to_vec(&project).unwrap()).unwrap();
        ask(&f, &f.root);
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet);
        assert!(!moved(&f), "a destination inside a project is refused");
        assert_eq!(last(&f.data).unwrap().stopped.as_deref(), Some(TARGET));
        // A git work tree made around it since is refused too.
        let f = fixture();
        fs::create_dir(f.chosen.join(".git")).unwrap();
        ask(&f, &f.root);
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet);
        assert!(!moved(&f));
        // So is a project made of `Faber-storage` itself.
        let f = fixture();
        let project = serde_json::json!([{ "id": "p1", "name": "Storage", "path": f.root }]);
        fs::write(f.data.join("projects.json"), serde_json::to_vec(&project).unwrap()).unwrap();
        ask(&f, &f.root);
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet);
        assert!(!moved(&f), "a project on the storage folder itself is refused");
        // And an unreadable project list refuses rather than guesses.
        let f = fixture();
        fs::write(f.data.join("projects.json"), b"{").unwrap();
        ask(&f, &f.root);
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet);
        assert!(!moved(&f));
    }

    #[test]
    fn storage_move_reports_progress_and_an_interrupted_move_at_the_next_start() {
        #[derive(Default)]
        struct Screen(std::sync::Mutex<Vec<Progress>>);
        impl Shell for Screen {
            fn storage_move(&self, progress: Progress) {
                lock(&self.0).push(progress);
            }
            fn moves_tools(&self) -> bool {
                true
            }
        }
        let f = fixture();
        ask(&f, &f.root);
        let screen = Screen::default();
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &screen);
        let shown = lock(&screen.0).clone();
        assert_eq!(shown.first().map(|p| (p.number, p.total)), Some((1, 1)));
        assert!(shown.last().is_some_and(|p| p.done));
        // A start whose move a crash cut puts it right and says so, without moving anything new.
        fs::write(folder(&f.data).join("journal.json"), b"{").unwrap();
        ask(&f, &f.root);
        at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet);
        assert_eq!(last(&f.data).unwrap().stopped.as_deref(), Some(INTERRUPTED));
        assert!(!folder(&f.data).join(REQUEST).exists());
    }

    #[test]
    fn storage_move_waits_for_a_shell_that_can_show_it() {
        // A start in a shell whose window opens only later (Tauri) leaves a waiting move for one that can show it.
        struct NoWindow;
        impl Shell for NoWindow {}
        let f = fixture();
        ask(&f, &f.root);
        drop(at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &NoWindow));
        assert!(!moved(&f) && folder(&f.data).join(REQUEST).exists());
        drop(at_start(&f.data, &Choice::resolve(&f.data, f.file.clone(), false), &Quiet));
        assert!(moved(&f));
    }

    #[test]
    fn storage_move_waits_for_a_faber_still_closing_on_the_same_folder() {
        let f = fixture();
        ask(&f, &f.root);
        let held = hold(&f.data).unwrap();
        let (data, file) = (f.data.clone(), f.file.clone());
        let start = std::thread::spawn(move || at_start(&data, &Choice::resolve(&data, file, false), &Quiet));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!moved(&f) && folder(&f.data).join(REQUEST).exists(), "nothing moves while the other one holds the folder");
        drop(held);
        start.join().unwrap();
        assert!(moved(&f));
    }

    #[test]
    fn storage_move_restarts_only_when_nothing_would_be_cut() {
        #[derive(Default)]
        struct Counting(std::sync::atomic::AtomicUsize);
        impl Shell for Counting {
            fn relaunch(&self) -> Res<()> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }
        let shell = Counting::default();
        assert!(!restart(true, &shell), "a working conversation keeps Faber open");
        let install = crate::Installing::begin(Box::new(()));
        assert!(!restart(false, &shell), "so does a CLI or catalog install");
        drop(install);
        assert_eq!(shell.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn storage_move_is_never_run_by_a_command() {
        // Only the start moves; the screen's commands save, cancel or remove the kept originals.
        let source = include_str!("../table.rs");
        assert!(!source.contains("moving::run") && !source.contains("at_start"));
        assert!(source.contains("storage_move_request") && source.contains("storage_move_remove"));
    }
}
