use super::*;

const OWNER: &str = "3611dcb7-6387-47c6-8b25-1b6f20903171";
const OTHER: &str = "acde9652-83c0-44f6-8d93-c0f847746c95";

fn point(history: &mut History, root: &Path, owner: &str, snapshot: Snapshot) -> String {
    let held = Root::open(root).unwrap();
    let identity = held.identity().unwrap();
    let conversation: Conversation = serde_json::from_value(serde_json::json!({
        "id":owner,"title":"Private history","area":"chat","createdAt":1,"updatedAt":1,"messages":[]
    }))
    .unwrap();
    let id = Uuid::new_v4().to_string();
    let saved = Point {
        info: PointInfo {
            id: id.clone(),
            message_id: "question".into(),
            turn_id: "reply".into(),
            at: 1,
            coverage: snapshot.coverage(),
            sealed: false,
            cursor: Cursor::default(),
        },
        folder: history.folder_id(root, &identity),
        root: root.into(),
        root_identity: identity,
        conversation: owner.into(),
        parent: None,
        sequence: 1,
        before: Arc::new(snapshot),
        after: None,
        saved: SavedConversation::bindings(&conversation),
        writers: vec![],
        unknown_author: false,
        review_shared: None,
        review_pending: false,
        active: true,
    };
    history.save_point(&saved).unwrap();
    history.points.insert(id.clone(), saved);
    id
}

#[test]
fn purge_reclaims_unique_objects_but_preserves_another_chats_shared_pack() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    let root = Root::open(folder.path()).unwrap();
    let key = history.folder_id(folder.path(), &root.identity().unwrap());
    history.prepare_folder(&key).unwrap();
    let unique = history.save_bytes(&key, b"private to deleted chat", vec![], vec![]).unwrap();
    let shared = history.save_bytes(&key, b"shared version", vec![], vec![]).unwrap();
    history.begin_batch(&key).unwrap();
    let bytes = b"packed shared version";
    let (oid, pack, offset) = history.write_capture_blob(&key, &mut &bytes[..], bytes.len() as u64).unwrap();
    let mut snapshot = Snapshot::default();
    snapshot.files.insert(
        "packed.txt".into(),
        Image { oid, bytes: bytes.len() as u64, protection: vec![], identity: vec![], token: None, pack: Some(pack), offset },
    );
    history.finish_batch(&mut snapshot).unwrap();
    let mut deleted = snapshot.clone();
    deleted.files.insert("private.txt".into(), unique.clone());
    deleted.files.insert("shared.txt".into(), shared.clone());
    snapshot.files.insert("shared.txt".into(), shared.clone());
    let first = point(&mut history, folder.path(), OWNER, deleted);
    let second = point(&mut history, folder.path(), OTHER, snapshot.clone());
    std::fs::write(folder.path().join("personal.txt"), b"untouched work").unwrap();
    history.reset_work();
    let began = Instant::now();
    history.purge_conversation(OWNER).unwrap();
    println!(
        "purge_fixture: elapsed_us={} deleted_files={} object_reads={}",
        began.elapsed().as_micros(),
        history.work().owned_files_deleted,
        history.work().object_reads
    );
    assert!(history.list(OWNER).is_empty());
    assert_eq!(history.list(OTHER)[0].id, second);
    let storage = data.path().join("rewind").join(&key);
    assert!(!storage.join("points").join(format!("{first}.json")).exists());
    assert!(!storage.join("objects").join(&unique.oid[..2]).join(&unique.oid[2..]).exists());
    assert_eq!(history.blob(&key, &shared).unwrap(), b"shared version");
    assert_eq!(history.blob(&key, &snapshot.files["packed.txt"]).unwrap(), bytes);
    assert_eq!(std::fs::read(folder.path().join("personal.txt")).unwrap(), b"untouched work");
    let mut reopened = History::open(data.path(), 2).unwrap();
    assert!(reopened.list(OWNER).is_empty());
    assert_eq!(reopened.list(OTHER)[0].id, second);
    reopened.purge_conversation(OTHER).unwrap();
    assert_eq!(std::fs::read_dir(storage.join("objects/pack")).unwrap().count(), 0);
}

#[test]
fn purge_keeps_incomplete_restore_journals_and_removes_completed_redo_copies() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    let root = Root::open(folder.path()).unwrap();
    let key = history.folder_id(folder.path(), &root.identity().unwrap());
    history.prepare_folder(&key).unwrap();
    let point_id = point(&mut history, folder.path(), OWNER, Snapshot::default());
    let saved = &history.points[&point_id];
    let id = Uuid::new_v4().to_string();
    let mut journal = Journal {
        id: id.clone(),
        conversation: OWNER.into(),
        point: Some(point_id.clone()),
        message_id: None,
        mode: None,
        folder: key.clone(),
        root: folder.path().into(),
        root_identity: root.identity().unwrap(),
        at: 1,
        expires: RETENTION,
        operations: vec![],
        before: saved.saved.clone(),
        after: saved.saved.clone(),
        files_done: true,
        conversation_done: false,
        redo: false,
        retained: vec![],
        partial: false,
    };
    history.save_journal(&journal).unwrap();
    history.journals.insert(id.clone(), journal.clone());
    assert_eq!(history.purge_conversation(OWNER), Err("error.rewind.deleteRecovery".into()));
    assert_eq!(history.list(OWNER)[0].id, point_id);
    assert!(data.path().join("rewind").join(&key).join("journals").join(format!("{id}.json")).is_file());
    journal.conversation_done = true;
    history.save_journal(&journal).unwrap();
    history.journals.insert(id.clone(), journal);
    history.purge_conversation(OWNER).unwrap();
    assert!(history.list(OWNER).is_empty());
    assert!(history.journals.is_empty());
    assert!(History::open(data.path(), 2).unwrap().journals.is_empty());
}
