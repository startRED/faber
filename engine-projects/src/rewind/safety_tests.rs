use super::*;

struct Listing;
impl Executor for Listing {
    fn execute(&mut self, _: &Invocation) -> Result<crate::project::git::Output, crate::project::git::GitError> {
        Ok(crate::project::git::Output { success: true, stdout: vec![] })
    }
}

fn captured() -> (tempfile::TempDir, tempfile::TempDir, History, String) {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    let conversation: Conversation = serde_json::from_value(serde_json::json!({
        "id":"3611dcb7-6387-47c6-8b25-1b6f20903171","title":"Rewind safety","area":"chat",
        "createdAt":1,"updatedAt":1,"messages":[{"id":"question","role":"user","text":"Change files","at":1}]
    }))
    .unwrap();
    let info = history.capture(&mut Listing, folder.path(), &conversation, "question", "reply", 1, false).unwrap();
    (data, folder, history, info.id)
}

#[test]
fn rewind_list_never_claims_an_unwritten_seal_is_durable() {
    let (_data, folder, mut history, id) = captured();
    let scan = history.scan_seal(&mut Listing, folder.path()).unwrap();
    let publication = history.prepare_seal(scan);
    history.publish_seal(&publication);
    let pending = history.list("3611dcb7-6387-47c6-8b25-1b6f20903171");
    assert_eq!(pending[0].id, id);
    assert!(!pending[0].sealed, "memory publication cannot advertise a durable restore point");
    history.persist_seal(publication, 2).unwrap();
    assert!(history.list("3611dcb7-6387-47c6-8b25-1b6f20903171")[0].sealed);
}

#[test]
fn rewind_failed_seal_marks_unsaved_versions_unavailable() {
    let (data, folder, mut history, id) = captured();
    let scan = history.scan_seal(&mut Listing, folder.path()).unwrap();
    let publication = history.prepare_seal(scan);
    history.publish_seal(&publication);
    let point = history.points.get(&id).unwrap();
    let path = data.path().join("rewind").join(&point.folder).join("points").join(format!("{id}.json"));
    std::fs::rename(&path, data.path().join("preserved-point.json")).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert!(history.persist_seal(publication, 2).is_err());
    let info = &history.list("3611dcb7-6387-47c6-8b25-1b6f20903171")[0];
    assert!(!info.sealed && info.coverage.unavailable && info.coverage.incomplete);
}
