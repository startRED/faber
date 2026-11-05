//! A write of a conversation that fails always reaches the screen's save-error toast, and never loses what it could not write.
//! A directory where the temp file goes makes a write fail while the old file stays valid.

use crate::support;
use faber_engine::cli::Cli;
use faber_engine::model::{Area, BranchOf, Status, Vendor};
use faber_engine::router::Choice;
use faber_engine::store::Store;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn engine(root: &crate::support::DataFolder) -> (Engine, Arc<Mutex<Vec<AppEvent>>>) {
    engine_with(root, support::fake(vec![]).spawn)
}

fn engine_with(root: &crate::support::DataFolder, spawn: faber_engine::proc::Spawner) -> (Engine, Arc<Mutex<Vec<AppEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let heard = events.clone();
    let app = Engine::new(
        EngineOptions {
            data_dir: root.path().into(),
            #[cfg(windows)]
            chat_dir: root.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(move |event| heard.lock().unwrap().push(event)),
    )
    .unwrap();
    (app, events)
}

/// The save-failed events heard since the last call, as their keys.
fn failures(events: &Mutex<Vec<AppEvent>>) -> Vec<String> {
    let heard = std::mem::take(&mut *events.lock().unwrap());
    heard.into_iter().filter_map(|e| if let AppEvent::SaveFailed { key } = e { Some(key) } else { None }).collect()
}

/// A chat with a saved record and nothing else, and the folder that blocks its next write.
fn chat(app: &Engine, dir: &Path) -> (String, std::path::PathBuf) {
    let group = app.create_chat_group("Work").unwrap();
    let id = app.new_group_chat(&group.id).unwrap().id;
    app.move_chat_to_group(&id, None).unwrap();
    let blocker = dir.join("conversations").join(format!("{id}.json.tmp"));
    (id, blocker)
}

#[tokio::test]
async fn a_conversation_that_cannot_be_written_says_so_once_and_is_written_whole_by_the_next_save() {
    let dir = support::DataFolder::new();
    let (app, events) = engine(&dir);
    let (id, blocker) = chat(&app, dir.path());
    assert!(failures(&events).is_empty(), "nothing failed yet");

    std::fs::create_dir(&blocker).unwrap();
    app.pin(&id, true).unwrap();
    assert_eq!(failures(&events), ["error.chat.saveFailed"], "a pin that could not be saved is told");
    app.rename(&id, "Kept in memory");
    app.archive(&id, true).unwrap();
    assert!(failures(&events).is_empty(), "a run of failures is told once, not at every save");
    let live = app.open(&id).unwrap();
    assert_eq!(live.title, "Kept in memory");
    assert!(live.archived_at.is_some(), "what could not be written stays in memory");
    assert!(Store::new(dir.path()).get(&id).unwrap().archived_at.is_none(), "the old file is untouched");

    std::fs::remove_dir(&blocker).unwrap();
    app.rename(&id, "Written whole");
    assert!(failures(&events).is_empty(), "a save that works says nothing");
    let saved = Store::new(dir.path()).get(&id).unwrap();
    assert_eq!(saved.title, "Written whole");
    assert!(saved.archived_at.is_some(), "the next save writes everything held in memory");

    std::fs::create_dir(&blocker).unwrap();
    app.archive(&id, false).unwrap();
    assert_eq!(failures(&events), ["error.chat.saveFailed"], "a new run of failures is told again");
    app.shutdown();
}

#[tokio::test]
async fn another_conversation_saving_fine_does_not_make_a_failing_one_tell_again() {
    let dir = support::DataFolder::new();
    let (app, events) = engine(&dir);
    let (a, blocker) = chat(&app, dir.path());
    let (b, _) = chat(&app, dir.path());
    failures(&events);

    std::fs::create_dir(&blocker).unwrap();
    app.rename(&a, "A one");
    assert_eq!(failures(&events), ["error.chat.saveFailed"]);
    app.rename(&b, "B works");
    assert_eq!(Store::new(dir.path()).get(&b).unwrap().title, "B works");
    app.rename(&a, "A two");
    assert!(failures(&events).is_empty(), "B's good save does not end A's run of failures");

    std::fs::remove_dir(&blocker).unwrap();
    app.rename(&a, "A three");
    std::fs::create_dir(&blocker).unwrap();
    app.rename(&a, "A four");
    assert_eq!(failures(&events), ["error.chat.saveFailed"], "once A was saved, a new failure is told");
    app.shutdown();
}

#[tokio::test]
async fn closing_faber_writes_a_conversation_whose_last_save_failed() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let (id, blocker) = chat(&app, dir.path());
    std::fs::create_dir(&blocker).unwrap();
    app.rename(&id, "Typed while the disk failed");
    assert_ne!(Store::new(dir.path()).get(&id).unwrap().title, "Typed while the disk failed");

    std::fs::remove_dir(&blocker).unwrap();
    app.shutdown();
    assert_eq!(Store::new(dir.path()).get(&id).unwrap().title, "Typed while the disk failed", "it is not left to a save that never comes");
}

#[tokio::test]
async fn deleting_a_chat_whose_group_cannot_be_updated_deletes_nothing() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let group = app.create_chat_group("Work").unwrap();
    let id = app.new_group_chat(&group.id).unwrap().id;
    let notes = dir.path().join("attachments").join(&id).join("notes.txt");
    std::fs::create_dir_all(notes.parent().unwrap()).unwrap();
    std::fs::write(&notes, b"keep these notes").unwrap();
    let blocker = dir.path().join("chat-groups.json.tmp");
    std::fs::create_dir(&blocker).unwrap();

    assert_eq!(app.remove_with(&id, false).unwrap_err(), "error.chat.deleteFailed");
    assert!(app.open(&id).is_some() && Store::new(dir.path()).get(&id).is_some(), "the chat stays");
    assert_eq!(app.state().groups[0].conversation_ids, std::slice::from_ref(&id), "and so does its place in the group");
    assert!(notes.is_file(), "so do its attachments");

    std::fs::remove_dir(&blocker).unwrap();
    app.remove_with(&id, false).unwrap();
    assert!(Store::new(dir.path()).get(&id).is_none());
    assert!(!notes.exists(), "a deletion that works takes the attachments too");
    app.shutdown();
}

/// Keeps `path` from being deleted until the returned guard is dropped; `None` where this computer cannot stop a deletion
/// (a Unix account that may delete anything). With `readable` false, Windows also refuses to read it.
fn undeletable(path: &Path, readable: bool) -> Option<Box<dyn std::any::Any>> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let held = std::fs::OpenOptions::new().read(true).share_mode(1).open(path).unwrap();
        if !readable {
            // An existing reader may still be open; block contents without racing its handle or allowing deletion.
            held.try_lock().unwrap();
        }
        Some(Box::new(held))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = readable;
        struct Reopen(std::path::PathBuf);
        impl Drop for Reopen {
            fn drop(&mut self) {
                std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        let folder = path.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        let guard = Reopen(folder.clone());
        if std::fs::write(folder.join("probe"), b"").is_ok() {
            return None;
        }
        Some(Box::new(guard))
    }
}

#[cfg(windows)]
#[test]
fn unreadable_guard_coexists_with_an_existing_reader_and_releases_its_locks() {
    let dir = tempfile::tempdir().unwrap();
    let record = dir.path().join("record.json");
    std::fs::write(&record, b"saved record").unwrap();
    let reader = std::fs::File::open(&record).unwrap();

    let held = undeletable(&record, false).unwrap();
    assert!(std::fs::read(&record).is_err(), "the guard must refuse actual reads");
    assert!(std::fs::remove_file(&record).is_err(), "the guard must also refuse deletion");
    drop(held);

    assert_eq!(std::fs::read(&record).unwrap(), b"saved record");
    std::fs::remove_file(&record).unwrap();
    assert_eq!(reader.metadata().unwrap().len(), 12, "the earlier reader stayed open throughout the check");
}

#[tokio::test]
async fn deleting_a_chat_whose_record_cannot_be_deleted_says_so_and_keeps_everything_of_it() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let group = app.create_chat_group("Work").unwrap();
    let id = app.new_group_chat(&group.id).unwrap().id;
    let record = dir.path().join("conversations").join(format!("{id}.json"));
    let notes = dir.path().join("attachments").join(&id).join("notes.txt");
    std::fs::create_dir_all(notes.parent().unwrap()).unwrap();
    std::fs::write(&notes, b"keep these notes").unwrap();

    let Some(held) = undeletable(&record, true) else { return };
    assert_eq!(
        app.remove_with(&id, false).unwrap_err(),
        "error.chat.deleteFailed",
        "a deletion that did not happen is not reported as done"
    );
    assert!(record.is_file() && app.open(&id).is_some(), "the chat stays");
    assert!(notes.is_file(), "so do its attachments");
    assert_eq!(
        app.state().groups[0].conversation_ids,
        std::slice::from_ref(&id),
        "it is back in its group, which was written before the record failed"
    );
    drop(held);

    app.remove_with(&id, false).unwrap();
    assert!(!record.exists() && !notes.exists());
    app.shutdown();
}

#[tokio::test]
async fn a_chat_that_stays_keeps_the_versions_that_branched_from_it() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let (parent, _) = chat(&app, dir.path());
    let first = app.open(&parent).unwrap();
    app.shutdown();
    let store = Store::new(dir.path());
    let made: Vec<String> = ["b1", "b2"]
        .iter()
        .map(|name| {
            let mut branch = first.clone();
            branch.id = format!("{parent}-{name}");
            branch.branch_of = Some(BranchOf { conversation_id: parent.clone(), message_id: "m1".into() });
            store.save_checked(&branch).unwrap();
            branch.id
        })
        .collect();
    let (app, _) = engine(&dir);
    let record = dir.path().join("conversations").join(format!("{parent}.json"));

    let Some(held) = undeletable(&record, true) else { return };
    assert_eq!(app.remove_with(&parent, false).unwrap_err(), "error.chat.deleteFailed");
    for id in &made {
        assert_eq!(store.get(id).unwrap().branch_of.unwrap().conversation_id, parent, "{id} still branches from the chat that stayed");
    }
    drop(held);

    app.remove_with(&parent, false).unwrap();
    assert_eq!(store.get(&made[0]).unwrap().branch_of, None, "a deletion that works hands the family on");
    assert_eq!(store.get(&made[1]).unwrap().branch_of.unwrap().conversation_id, made[0]);
    app.shutdown();
}

#[tokio::test]
async fn a_damaged_record_is_deleted_not_mistaken_for_one_that_is_gone() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let (id, _) = chat(&app, dir.path());
    app.shutdown();
    let record = dir.path().join("conversations").join(format!("{id}.json"));
    std::fs::write(&record, "{").unwrap();
    let (app, _) = engine(&dir);

    app.remove_with(&id, false).unwrap();
    assert!(!record.exists(), "the file is gone, so it does not come back at the next start");
    app.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn a_record_nothing_can_read_is_not_reported_deleted() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let (id, _) = chat(&app, dir.path());
    let record = dir.path().join("conversations").join(format!("{id}.json"));
    let notes = dir.path().join("attachments").join(&id).join("notes.txt");
    std::fs::create_dir_all(notes.parent().unwrap()).unwrap();
    std::fs::write(&notes, b"keep these notes").unwrap();

    let held = undeletable(&record, false).unwrap();
    assert!(std::fs::read(&record).is_err(), "the fixture must make the record unreadable before testing deletion");
    assert_eq!(app.remove_with(&id, false).unwrap_err(), "error.chat.deleteFailed");
    assert!(record.is_file() && notes.is_file(), "a file that stayed keeps its attachments");
    drop(held);
    app.remove_with(&id, false).unwrap();
    assert!(!record.exists());
    app.shutdown();
}

/// A disk whose first rename finds the file held for a moment, as Windows does under an antivirus.
#[cfg(windows)]
struct BusyOnce(std::sync::atomic::AtomicBool);
#[cfg(windows)]
impl faber_engine::store::Disk for BusyOnce {
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        faber_engine::store::Disk::write(&faber_engine::store::RealDisk, path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        if self.0.swap(true, std::sync::atomic::Ordering::Relaxed) {
            faber_engine::store::Disk::rename(&faber_engine::store::RealDisk, from, to)
        } else {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        }
    }
}

#[cfg(windows)]
#[test]
fn a_rename_held_for_a_moment_is_tried_again_before_a_save_counts_as_failed() {
    let dir = support::DataFolder::new();
    let file = dir.path().join("record.json");
    faber_engine::store::replace(&BusyOnce(Default::default()), &file, b"{}").unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), b"{}");
    assert!(!dir.path().join("record.json.tmp").exists());
}

/// A disk that keeps refusing the rename, and counts how often it was asked.
#[cfg(windows)]
struct Denied(std::sync::atomic::AtomicUsize);
#[cfg(windows)]
impl faber_engine::store::Disk for Denied {
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        faber_engine::store::Disk::write(&faber_engine::store::RealDisk, path, bytes)
    }
    fn rename(&self, _: &Path, _: &Path) -> std::io::Result<()> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    }
}

#[cfg(windows)]
#[tokio::test]
async fn a_file_that_stays_denied_costs_the_wait_once_not_at_every_save() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let (id, _) = chat(&app, dir.path());
    let conversation = app.open(&id).unwrap();
    app.shutdown();
    let denied = Arc::new(Denied(Default::default()));
    let other = support::tempdir();
    let store = Store::with_disk(other.path(), denied.clone());
    let asked = || denied.0.swap(0, std::sync::atomic::Ordering::Relaxed);

    assert!(store.save_checked(&conversation).is_err());
    assert!(asked() > 1, "the first failure waits and tries again");
    assert!(store.save_checked_with(&conversation, false).is_err());
    assert_eq!(asked(), 1, "a file already known to fail is asked once");
}

#[tokio::test]
async fn a_chat_deleted_after_it_failed_does_not_hide_the_next_failure() {
    let dir = support::DataFolder::new();
    let (app, events) = engine(&dir);
    let (a, blocker_a) = chat(&app, dir.path());
    let (b, blocker_b) = chat(&app, dir.path());
    failures(&events);

    std::fs::create_dir(&blocker_a).unwrap();
    app.rename(&a, "A fails");
    assert_eq!(failures(&events), ["error.chat.saveFailed"]);
    app.remove_with(&a, false).unwrap();
    std::fs::create_dir(&blocker_b).unwrap();
    app.rename(&b, "B fails");
    assert_eq!(failures(&events), ["error.chat.saveFailed"], "A is gone and will never be saved, so B's failure is told");
    app.shutdown();
}

#[tokio::test]
async fn a_delete_that_stops_after_closing_a_live_answer_leaves_a_stopped_reply_not_a_typing_one() {
    let dir = support::DataFolder::new();
    let (spawn, pipes) = support::pipes();
    let (app, _) = engine_with(&dir, spawn);
    let id = app
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "write a long answer".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&pipes).await;
    pipe.say(serde_json::json!({"type":"system","subtype":"init","session_id":"live","model":"test"})).await;
    pipe.say(serde_json::json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Typing"}]}})).await;
    let streaming =
        |app: &Engine| app.open(&id).and_then(|c| c.messages.last().cloned()).is_some_and(|m| m.status == Some(Status::Streaming));
    support::until("the reply never went live", || streaming(&app)).await;
    let record = dir.path().join("conversations").join(format!("{id}.json"));

    let Some(held) = undeletable(&record, true) else { return };
    assert_eq!(app.remove_with(&id, false).unwrap_err(), "error.chat.deleteFailed");
    let reply = app.open(&id).unwrap().messages.last().cloned().unwrap();
    assert_eq!(reply.status, Some(Status::Stopped), "the answer that was stopped ends as a stopped one, not typing for ever");
    assert!(reply.permissions.is_none());
    drop(held);
    app.shutdown();
}

struct DelayedDisk {
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    writes: std::sync::atomic::AtomicUsize,
}

struct PageFlushDisk {
    fail_at: usize,
    flushes: std::sync::atomic::AtomicUsize,
}

impl faber_engine::store::Disk for PageFlushDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        faber_engine::store::Disk::write(&faber_engine::store::RealDisk, path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        faber_engine::store::Disk::rename(&faber_engine::store::RealDisk, from, to)
    }
    fn flush(&self, path: &Path) -> std::io::Result<()> {
        if self.flushes.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == self.fail_at {
            return Err(std::io::ErrorKind::StorageFull.into());
        }
        faber_engine::store::Disk::flush(&faber_engine::store::RealDisk, path)
    }
}

#[test]
fn failed_page_flush_keeps_old_inline_input() {
    use faber_engine::transcript::TranscriptStore;
    use std::sync::atomic::Ordering;

    for fail_at in 0..4 {
        let dir = support::DataFolder::new();
        let mut conversation = support::big_conversation(dir.path(), 20_000);
        let file = dir.path().join("conversations").join(format!("{}.json", conversation.id));
        let original = std::fs::read(&file).unwrap();
        let disk = Arc::new(PageFlushDisk { fail_at, flushes: Default::default() });
        let store = Store::with_disk(dir.path(), disk.clone());
        let pages = TranscriptStore::with_disk(dir.path().join("child-transcripts"), disk.clone());
        let tool =
            conversation.messages.iter_mut().flat_map(|m| m.tools.iter_mut().flatten()).find(|t| t.input.to_string().len() > 4096).unwrap();
        let input = tool.input.clone();
        for _ in 0..2 {
            disk.flushes.store(0, Ordering::Relaxed);
            assert!(pages.keep_input(&conversation.id, &tool.id, &tool.name, &mut tool.input).is_none());
            assert_eq!(tool.input, input);
            assert!(tool.detail.is_none());
            assert_eq!(disk.flushes.load(Ordering::Relaxed), fail_at + 1);
        }
        store.save_checked(&conversation).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), original);
        assert_eq!(store.get(&conversation.id).unwrap(), conversation);
    }
}

impl faber_engine::store::Disk for DelayedDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let number = self.writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.entered.send(()).unwrap();
        if number == 0 {
            self.release.lock().unwrap().recv().unwrap();
        }
        faber_engine::store::Disk::write(&faber_engine::store::RealDisk, path, bytes)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        faber_engine::store::Disk::rename(&faber_engine::store::RealDisk, from, to)
    }
}

#[tokio::test]
async fn snapshots_keep_the_newest_and_never_share_a_temp_file() {
    let source = support::DataFolder::new();
    let (app, _) = engine(&source);
    let (id, _) = chat(&app, source.path());
    let mut conversation = app.open(&id).unwrap();
    app.shutdown();
    conversation.title = "newest".into();
    let newest = Store::snapshot(&conversation);
    conversation.title = "older".into();
    let older = Store::snapshot(&conversation);
    let target = support::tempdir();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
    let disk = Arc::new(DelayedDisk { entered: entered_tx, release: Mutex::new(release_rx), writes: Default::default() });
    let store = Arc::new(Store::with_disk(target.path(), disk.clone()));
    let first = store.clone();
    let first_id = id.clone();
    let first_bytes = newest.clone();
    let first = std::thread::spawn(move || first.save_snapshot(&first_id, 6, first_bytes));
    entered_rx.recv().unwrap();
    let second = store.clone();
    let second_id = id.clone();
    let second_bytes = newest.clone();
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(0);
    let second = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        second.save_snapshot(&second_id, 7, second_bytes)
    });
    started_rx.recv().unwrap();
    release_tx.send(()).unwrap();
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();
    store.save_snapshot(&id, 5, older).unwrap();
    assert_eq!(store.get(&id).unwrap().title, "newest");
    assert_eq!(disk.writes.load(std::sync::atomic::Ordering::SeqCst), 2);

    conversation.title = "failed".into();
    let blocker = target.path().join("conversations").join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocker).unwrap();
    assert_eq!(store.save_snapshot(&id, 8, Store::snapshot(&conversation)), Err(faber_engine::store::Fault::Write));
    assert_eq!(store.get(&id).unwrap().title, "newest");
    std::fs::remove_dir(&blocker).unwrap();
    store.save_snapshot(&id, 8, Store::snapshot(&conversation)).unwrap();
    assert_eq!(store.get(&id).unwrap().title, "failed");
    conversation.title = "synchronous".into();
    store.save_checked(&conversation).unwrap();
    assert_eq!(store.next_generation(&id), 10);
}

#[tokio::test]
async fn deleted_conversation_is_not_recreated_by_a_delayed_snapshot() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let (id, _) = chat(&app, dir.path());
    let conversation = app.open(&id).unwrap();
    app.shutdown();
    let store = Store::new(dir.path());
    let generation = store.next_generation(&id);
    store.remove_checked(&id).unwrap();
    store.save_snapshot(&id, generation, Store::snapshot(&conversation)).unwrap();
    assert!(!dir.path().join("conversations").join(format!("{id}.json")).exists());
}

#[tokio::test]
async fn move_refuses_a_conversation_deleted_after_it_was_loaded() {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let (app, _) = engine(&dir);
    let group = app.create_chat_group("Work").unwrap();
    let chat = app.new_group_chat(&group.id).unwrap();
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
    let store = Store::new(dir.path());
    store.remove_checked(&chat.id).unwrap();

    assert!(app.move_to_project(&chat.id, &project.id).is_err());
    assert!(store.get(&chat.id).is_none());
    assert_eq!(store.chat_groups().unwrap()[0].conversation_ids, vec![chat.id]);
    app.shutdown();
}

#[tokio::test]
async fn move_preserves_the_latest_saved_conversation_and_caches_it() {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let (app, _) = engine(&dir);
    let group = app.create_chat_group("Work").unwrap();
    let mut chat = app.new_group_chat(&group.id).unwrap();
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
    let store = Store::new(dir.path());
    app.rename(&chat.id, "newer title");
    chat.title = "older disk title".into();
    store.save_checked(&chat).unwrap();

    let moved = app.move_to_project(&chat.id, &project.id).unwrap();
    assert_eq!(moved.title, "newer title");
    assert_eq!(app.open(&chat.id).unwrap().title, "newer title");
    assert_eq!(store.get(&chat.id).unwrap().title, "newer title");
    app.shutdown();
}

#[tokio::test]
async fn move_copies_attachments_from_the_latest_saved_folder() {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let source = support::work_folder();
    let (app, _) = engine(&dir);
    let group = app.create_chat_group("Work").unwrap();
    let chat = app.new_group_chat(&group.id).unwrap();
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
    let store = Store::new(dir.path());
    let mut latest = store.get(&chat.id).unwrap();
    let fixture: faber_engine::model::Conversation =
        serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    latest.messages = fixture.messages;
    for message in &mut latest.messages {
        message.attachments = None;
    }
    latest.messages[0].attachments = Some(vec!["latest.txt".into()]);
    latest.working_folder = Some(source.path().to_string_lossy().into_owned());
    std::fs::create_dir_all(source.path().join("attachments")).unwrap();
    std::fs::write(source.path().join("attachments/latest.txt"), "latest attachment").unwrap();
    store.save_checked(&latest).unwrap();
    app.shutdown();
    drop(app);
    let (app, _) = engine(&dir);
    let moved = app.move_to_project(&chat.id, &project.id).unwrap();
    assert_eq!(moved.messages, latest.messages);
    assert_eq!(std::fs::read_to_string(dir.path().join("attachments").join(&chat.id).join("latest.txt")).unwrap(), "latest attachment");
    app.shutdown();
}

#[tokio::test]
async fn group_recovery_and_snapshot_writes_use_the_same_lock() {
    let dir = support::DataFolder::new();
    let (app, _) = engine(&dir);
    let group = app.create_chat_group("Work").unwrap();
    let conversation = app.new_group_chat(&group.id).unwrap();
    let groups = app.state().groups;
    app.shutdown();
    std::fs::write(
        dir.path().join("chat-group-move.json"),
        serde_json::json!({"conversationId": conversation.id, "groups": groups}).to_string(),
    )
    .unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
    let disk = Arc::new(DelayedDisk { entered: entered_tx, release: Mutex::new(release_rx), writes: Default::default() });
    let store = Arc::new(Store::with_disk(dir.path(), disk));
    let recovery = store.clone();
    let recovery = std::thread::spawn(move || recovery.chat_groups());
    entered_rx.recv().unwrap();
    let writer = store.clone();
    let id = conversation.id.clone();
    let bytes = Store::snapshot(&conversation);
    let (started_tx, started_rx) = std::sync::mpsc::sync_channel(0);
    let writer = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        writer.save_snapshot(&id, 1, bytes)
    });
    started_rx.recv().unwrap();
    release_tx.send(()).unwrap();
    recovery.join().unwrap().unwrap();
    writer.join().unwrap().unwrap();
    assert_eq!(store.get(&conversation.id).unwrap().id, conversation.id);
}
