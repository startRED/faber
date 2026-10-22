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

fn engine(root: &Path) -> (Engine, Arc<Mutex<Vec<AppEvent>>>) {
    engine_with(root, support::fake(vec![]).spawn)
}

fn engine_with(root: &Path, spawn: faber_engine::proc::Spawner) -> (Engine, Arc<Mutex<Vec<AppEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let heard = events.clone();
    let app = Engine::new(
        EngineOptions {
            data_dir: root.into(),
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
    );
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
    let dir = support::tempdir();
    let (app, events) = engine(dir.path());
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
    let dir = support::tempdir();
    let (app, events) = engine(dir.path());
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
    let dir = support::tempdir();
    let (app, _) = engine(dir.path());
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
    let dir = support::tempdir();
    let (app, _) = engine(dir.path());
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
        let held = std::fs::OpenOptions::new().read(true).share_mode(u32::from(readable)).open(path).unwrap();
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

#[tokio::test]
async fn deleting_a_chat_whose_record_cannot_be_deleted_says_so_and_keeps_everything_of_it() {
    let dir = support::tempdir();
    let (app, _) = engine(dir.path());
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
    let dir = support::tempdir();
    let (app, _) = engine(dir.path());
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
    let (app, _) = engine(dir.path());
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
    let dir = support::tempdir();
    let (app, _) = engine(dir.path());
    let (id, _) = chat(&app, dir.path());
    app.shutdown();
    let record = dir.path().join("conversations").join(format!("{id}.json"));
    std::fs::write(&record, "{").unwrap();
    let (app, _) = engine(dir.path());

    app.remove_with(&id, false).unwrap();
    assert!(!record.exists(), "the file is gone, so it does not come back at the next start");
    app.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn a_record_nothing_can_read_is_not_reported_deleted() {
    let dir = support::tempdir();
    let (app, _) = engine(dir.path());
    let (id, _) = chat(&app, dir.path());
    let record = dir.path().join("conversations").join(format!("{id}.json"));
    let notes = dir.path().join("attachments").join(&id).join("notes.txt");
    std::fs::create_dir_all(notes.parent().unwrap()).unwrap();
    std::fs::write(&notes, b"keep these notes").unwrap();

    let held = undeletable(&record, false).unwrap();
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
    let dir = support::tempdir();
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
    let dir = support::tempdir();
    let (app, _) = engine(dir.path());
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
    let dir = support::tempdir();
    let (app, events) = engine(dir.path());
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
    let dir = support::tempdir();
    let (spawn, pipes) = support::pipes();
    let (app, _) = engine_with(dir.path(), spawn);
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
    for _ in 0..300 {
        if streaming(&app) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(streaming(&app), "the reply is live");
    let record = dir.path().join("conversations").join(format!("{id}.json"));

    let Some(held) = undeletable(&record, true) else { return };
    assert_eq!(app.remove_with(&id, false).unwrap_err(), "error.chat.deleteFailed");
    let reply = app.open(&id).unwrap().messages.last().cloned().unwrap();
    assert_eq!(reply.status, Some(Status::Stopped), "the answer that was stopped ends as a stopped one, not typing for ever");
    assert!(reply.permissions.is_none());
    drop(held);
    app.shutdown();
}
