//! The live move status is held only by this host and its background worker.
use super::Choice;
use crate::{Host, Res, check_revoked, lock, request_flag};
use faber_engine::tools::{MoveControl, MoveEnd, MoveEvent, Store};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

const IN_USE: &str = "reason.storage.inUse";

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MoveStatus {
    running: bool,
    stopping: bool,
    stopped: bool,
    /// Change or Reset chose another folder while the move ran, so it stopped and dropped the item it was copying.
    superseded: bool,
    available: bool,
    old_copies: bool,
    target: Option<PathBuf>,
    item: Option<ItemProgress>,
    moved: usize,
    failures: Vec<ItemFailure>,
    error: Option<&'static str>,
}

#[derive(Clone, Serialize)]
struct ItemProgress {
    id: String,
    number: usize,
    total: usize,
    copied: u64,
    size: Option<u64>,
}

#[derive(Clone, Serialize)]
pub(crate) struct ItemFailure {
    id: String,
    error: &'static str,
}

pub(crate) fn status(state: &Mutex<MoveStatus>, target: Option<&Path>, tools: &Store) -> MoveStatus {
    let mut status = lock(state).clone();
    if !status.running {
        status.available = target.is_some() && tools.movable_catalog_ids().is_ok_and(|ids| !ids.is_empty());
        status.old_copies = tools.has_old_catalog_copies();
    }
    status
}

pub(crate) fn running_status(state: &Mutex<MoveStatus>) -> Option<MoveStatus> {
    let status = lock(state);
    status.running.then(|| status.clone())
}

/// The tools folder of the chosen storage folder, or None when new data goes to the main data folder.
fn chosen_tools(choice: &Mutex<Choice>, data: &Path) -> Option<PathBuf> {
    lock(choice).configuration(data).0.map(|root| root.join("tools"))
}

/// Asked before each item and before one is published: a choice changed by Change or Reset (or a fallback after an
/// unreadable record) abandons the move, so nothing more goes to a folder the person moved away from.
fn control(state: &Mutex<MoveStatus>, choice: &Mutex<Choice>, data: &Path, target: &Path) -> MoveControl {
    if chosen_tools(choice, data).as_deref() != Some(target) {
        MoveControl::Abandon
    } else if lock(state).stopping {
        MoveControl::Stop
    } else {
        MoveControl::Go
    }
}

pub(crate) fn start(h: &Host) -> Res<()> {
    check_revoked(request_flag().as_deref())?;
    if !h.engine.owns_storage_profile() {
        return Err("error.storage.move".into());
    }
    let target = chosen_tools(&h.storage_choice, &h.data_dir).ok_or("error.storage.unreachable")?;
    let tools = h.engine.tool_store();
    let state = h.storage_move.clone();
    let (choice, data) = (h.storage_choice.clone(), h.data_dir.clone());
    let mut status = lock(&state);
    if status.running {
        return Ok(());
    }
    *status = MoveStatus { running: true, target: target.parent().map(Path::to_path_buf), ..Default::default() };
    let worker = state.clone();
    if std::thread::Builder::new()
        .name("storage-move".into())
        .spawn(move || {
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            let _ = faber_winproc::background_thread();
            run(&worker, &tools, &target, || control(&worker, &choice, &data, &target));
        })
        .is_err()
    {
        status.running = false;
        return Err("error.storage.move".into());
    }
    Ok(())
}

fn run(state: &Mutex<MoveStatus>, tools: &Store, target: &Path, control: impl Fn() -> MoveControl) {
    let result = tools.move_catalog(target, control, |event| {
        let mut status = lock(state);
        match event {
            MoveEvent::Item { id, number, total } => {
                status.item = Some(ItemProgress { id: id.into(), number, total, copied: 0, size: None });
            }
            MoveEvent::Size(size) => {
                if let Some(item) = &mut status.item {
                    item.size = Some(size);
                }
            }
            MoveEvent::Copied(copied) => {
                if let Some(item) = &mut status.item {
                    item.copied = copied;
                }
            }
            MoveEvent::Moved => status.moved += 1,
            MoveEvent::Failed { id, error } => status.failures.push(ItemFailure { id: id.into(), error }),
        }
    });
    finish(&mut lock(state), result);
}

fn finish(status: &mut MoveStatus, result: Result<MoveEnd, &'static str>) {
    status.running = false;
    status.stopped = result == Ok(MoveEnd::Stopped);
    status.superseded = result == Ok(MoveEnd::Abandoned);
    status.stopping = false;
    status.error = result.err();
    status.item = None;
}

pub(crate) fn stop(state: &Mutex<MoveStatus>) -> Res<()> {
    check_revoked(request_flag().as_deref())?;
    let mut status = lock(state);
    if status.running {
        status.stopping = true;
    }
    Ok(())
}

/// Removes the moved originals no live session may still read, and says which ones stay for now and why.
pub(crate) fn remove(tools: &Store, live: &BTreeSet<String>, flag: Option<&AtomicBool>) -> Res<Vec<ItemFailure>> {
    check_revoked(flag)?;
    let kept = tools.remove_old_catalog_copies(|| check_revoked(flag).is_ok(), |id| in_use(live, id))?;
    Ok(kept.into_iter().map(|id| ItemFailure { id, error: IN_USE }).collect())
}

/// Whether a live kit holds the item; a plugin's skills are listed in kits as `<plugin>--<skill>`.
fn in_use(live: &BTreeSet<String>, id: &str) -> bool {
    live.iter().any(|kit| kit == id || kit.strip_prefix(id).is_some_and(|rest| rest.starts_with("--")))
}

pub(crate) type State = Arc<Mutex<MoveStatus>>;

#[cfg(test)]
mod tests {
    use super::*;
    use faber_engine::project::git;
    use std::fs;

    #[test]
    fn storage_move_abandons_once_change_or_reset_picks_another_folder() {
        let tmp = if cfg!(unix) { tempfile::tempdir_in(dirs::home_dir().unwrap()) } else { tempfile::tempdir() }.unwrap();
        let profile = PathBuf::from(git::native_path(&tmp.path().canonicalize().unwrap()));
        let data = profile.join("Faber");
        let (first, second) = (profile.join("first"), profile.join("second"));
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let mut picked = Choice::resolve(&data, data.with_file_name(super::super::CHOICE_FILE), false);
        let target = picked.save_folder(&data, Some(first)).unwrap().unwrap().join("tools");
        let (state, choice) = (Mutex::new(MoveStatus { running: true, ..Default::default() }), Mutex::new(picked));
        assert_eq!(control(&state, &choice, &data, &target), MoveControl::Go);
        lock(&state).stopping = true;
        assert_eq!(control(&state, &choice, &data, &target), MoveControl::Stop);
        lock(&choice).save_folder(&data, Some(second)).unwrap();
        assert_eq!(control(&state, &choice, &data, &target), MoveControl::Abandon, "Change");
        lock(&choice).save_folder(&data, None).unwrap();
        assert_eq!(control(&state, &choice, &data, &target), MoveControl::Abandon, "Reset");
    }

    #[test]
    fn storage_move_says_on_screen_that_a_new_choice_stopped_it() {
        let mut status = MoveStatus { running: true, stopping: true, ..Default::default() };
        finish(&mut status, Ok(MoveEnd::Abandoned));
        assert!(!status.running && !status.stopping && !status.stopped && status.superseded && status.error.is_none());
        assert_eq!(serde_json::to_value(&status).unwrap()["superseded"], true);
        finish(&mut status, Ok(MoveEnd::Stopped));
        assert!(status.stopped && !status.superseded);
    }

    #[test]
    fn storage_move_counts_an_item_or_its_plugin_skills_in_a_live_kit_as_in_use() {
        let live = BTreeSet::from(["voice".to_owned(), "toolkit--review".to_owned()]);
        assert!(in_use(&live, "voice") && in_use(&live, "toolkit"));
        assert!(!in_use(&live, "voic") && !in_use(&live, "tool") && !in_use(&live, "voice-2"));
        let tmp = tempfile::tempdir().unwrap();
        // Nothing was moved, so nothing stays.
        assert!(remove(&Store::new(&tmp.path().join("Faber")), &live, None).unwrap().is_empty());
    }
}
