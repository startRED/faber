use super::*;
use serde_json::json;

const CONVERSATION: &str = "3611dcb7-6387-47c6-8b25-1b6f20903171";

fn point(history: &mut History, folder: &Path, message: &str, at: u64, before: &[(&str, &[u8])], after: &[(&str, &[u8])]) {
    let root = Root::open(folder).unwrap();
    let identity = root.identity().unwrap();
    let key = history.folder_id(folder, &identity);
    history.prepare_folder(&key).unwrap();
    let snapshot = |files: &[(&str, &[u8])]| {
        let mut saved = Snapshot { complete: true, ..Snapshot::default() };
        for (name, bytes) in files {
            let image = history.save_bytes(&key, bytes, vec![], vec![]).unwrap();
            saved.files.insert(name.to_string(), image);
        }
        Arc::new(saved)
    };
    let before = snapshot(before);
    let after = snapshot(after);
    let conversation: Conversation = serde_json::from_value(json!({
        "id":CONVERSATION,"title":"Answer changes","area":"chat","createdAt":1,"updatedAt":1,"messages":[]
    }))
    .unwrap();
    let point = Point {
        review_shared: Some(false),
        review_pending: false,
        info: PointInfo {
            id: Uuid::new_v4().to_string(),
            message_id: message.into(),
            turn_id: format!("turn-{message}"),
            at,
            coverage: before.coverage(),
            sealed: true,
            cursor: Cursor::default(),
        },
        folder: key,
        root: folder.into(),
        root_identity: identity,
        conversation: CONVERSATION.into(),
        parent: None,
        sequence: at,
        before,
        after: Some(after),
        saved: SavedConversation::bindings(&conversation),
        writers: vec![],
        unknown_author: false,
        active: false,
    };
    history.save_point(&point).unwrap();
    history.points.insert(point.info.id.clone(), point);
}

#[test]
fn answer_changes_distinguish_empty_files_and_properties_without_decoding_contents() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    point(
        &mut history,
        folder.path(),
        "first",
        1,
        &[("properties", b"same\n"), ("empty-removed", b"")],
        &[("properties", b"same\n"), ("empty-added", b"")],
    );
    let saved = history.points.values_mut().next().unwrap();
    Arc::make_mut(saved.after.as_mut().unwrap()).files.get_mut("properties").unwrap().protection.push(1);
    history.reset_work();
    for (path, notice) in [
        ("properties", "answerChanges.properties"),
        ("empty-added", "answerChanges.emptyAdded"),
        ("empty-removed", "answerChanges.emptyDeleted"),
    ] {
        let patch = history.review_patch(CONVERSATION, "first", path, 0).unwrap();
        assert_eq!(patch.notice, Some(notice));
        assert!(patch.lines.is_empty());
    }
    assert_eq!(history.work().object_reads, 0);
}

#[test]
fn answer_changes_failed_publication_reports_unavailable_without_a_loaded_conversation() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    point(&mut history, folder.path(), "first", 1, &[("file", b"before\n")], &[("file", b"after\n")]);
    let id = history.list(CONVERSATION)[0].id.clone();
    let current = history.points.get_mut(&id).unwrap();
    let snapshot = current.after.take().unwrap();
    current.info.sealed = false;
    current.active = true;
    let current = current.clone();
    history.save_point(&current).unwrap();
    let publication = history.prepare_seal(Sealing { ids: vec![id.clone()], snapshot: (*snapshot).clone() });
    history.publish_seal(&publication);
    let destination = data.path().join("rewind").join(&current.folder).join("points").join(format!("{id}.json"));
    std::fs::rename(&destination, data.path().join("preserved-point.json")).unwrap();
    std::fs::create_dir(&destination).unwrap();
    assert!(history.persist_seal(publication, 2).is_err());
    let unavailable = history.review(CONVERSATION, "first", 0).unwrap();
    assert!(!unavailable.sealed && unavailable.coverage.unavailable && unavailable.coverage.incomplete);
    assert!(history.review_patch(CONVERSATION, "first", "file", 0).is_err());
}

#[test]
fn answer_changes_wait_for_durable_publication_before_exposing_saved_bytes() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    point(&mut history, folder.path(), "first", 1, &[("file", b"before\n")], &[("file", b"after\n")]);
    let id = history.list(CONVERSATION)[0].id.clone();
    let current = history.points.get_mut(&id).unwrap();
    let snapshot = current.after.take().unwrap();
    current.info.sealed = false;
    current.active = true;
    let current = current.clone();
    history.save_point(&current).unwrap();
    let publication = history.prepare_seal(Sealing { ids: vec![id], snapshot: (*snapshot).clone() });
    history.publish_seal(&publication);
    let preparing = history.review(CONVERSATION, "first", 0).unwrap();
    assert!(!preparing.sealed && preparing.files.is_empty(), "memory publication is not a saved file version");
    assert!(history.review_patch(CONVERSATION, "first", "file", 0).is_err());
    history.persist_seal(publication, 2).unwrap();
    assert!(history.review(CONVERSATION, "first", 0).unwrap().sealed);
    assert!(history.review_patch(CONVERSATION, "first", "file", 0).unwrap().lines.iter().any(|line| line == "+after"));
}

#[test]
fn answer_changes_interrupted_capture_is_unavailable_after_restart() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    point(&mut history, folder.path(), "first", 1, &[("file", b"before\n")], &[("file", b"after\n")]);
    let id = history.list(CONVERSATION)[0].id.clone();
    let stored = history.points.get_mut(&id).unwrap();
    stored.after = None;
    stored.info.sealed = false;
    stored.active = true;
    let stored = stored.clone();
    history.save_point(&stored).unwrap();
    drop(history);
    let history = History::open(data.path(), 3).unwrap();
    let review = history.review(CONVERSATION, "first", 0).unwrap();
    assert!(!review.sealed && review.coverage.unavailable && review.coverage.incomplete);
    assert!(review.files.is_empty());
}

#[test]
fn answer_changes_are_exactly_one_turn_and_survive_restart_and_personal_edits() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    point(&mut history, folder.path(), "first", 1, &[("file", b"before\n")], &[("file", b"first answer\n")]);
    point(&mut history, folder.path(), "second", 2, &[("file", b"first answer\n")], &[("file", b"second answer\n")]);
    std::fs::write(folder.path().join("file"), b"personal edits\n").unwrap();
    drop(history);
    let history = History::open(data.path(), 3).unwrap();
    let listed = history.review(CONVERSATION, "first", 0).unwrap();
    assert_eq!(listed.total, 1);
    assert_eq!(listed.files[0].path, "file");
    let patch = history.review_patch(CONVERSATION, "first", "file", 0).unwrap();
    assert!(patch.lines.iter().any(|line| line == "+first answer"));
    assert!(!patch.lines.iter().any(|line| line.contains("second answer") || line.contains("personal edits")));
    assert_eq!(std::fs::read(folder.path().join("file")).unwrap(), b"personal edits\n");
    assert!(history.review("different-conversation", "first", 0).is_err());
    assert!(history.review_patch(CONVERSATION, "first", "../outside", 0).is_err());
}

#[test]
fn answer_changes_list_is_metadata_only_and_paged() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    let names: Vec<_> = (0..73).map(|index| format!("file-{index:03}")).collect();
    let files: Vec<_> = names.iter().map(|name| (name.as_str(), b"saved\n".as_slice())).collect();
    point(&mut history, folder.path(), "first", 1, &[], &files);
    history.reset_work();
    let first = history.review(CONVERSATION, "first", 0).unwrap();
    let second = history.review(CONVERSATION, "first", 50).unwrap();
    assert_eq!((first.files.len(), second.files.len(), first.total), (50, 23, 73));
    assert_eq!(history.work().object_reads, 0);
    assert!(first.files.iter().all(|file| !file.counts_known));
    assert!(history.review(CONVERSATION, "first", usize::MAX).unwrap().files.is_empty());
}

#[test]
fn answer_change_patch_keeps_equal_middle_lines_and_missing_final_newline() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    let before = (1..=100).map(|line| format!("line {line}\n")).collect::<String>();
    let after = before.replace("line 2\n", "changed 2\n").replace("line 99\n", "changed 99\n");
    point(
        &mut history,
        folder.path(),
        "first",
        1,
        &[("file", before.as_bytes()), ("end", b"old")],
        &[("file", after.as_bytes()), ("end", b"new\n")],
    );
    let patch = history.review_patch(CONVERSATION, "first", "file", 0).unwrap();
    assert_eq!(patch.lines.iter().filter(|line| line.starts_with('+') && !line.starts_with("+++")).count(), 2);
    assert_eq!(patch.lines.iter().filter(|line| line.starts_with('-') && !line.starts_with("---")).count(), 2);
    assert!(!patch.lines.iter().any(|line| line.contains("line 50")));
    assert_eq!(patch.lines.iter().filter(|line| line.starts_with("@@")).count(), 2);
    let patch = history.review_patch(CONVERSATION, "first", "end", 0).unwrap();
    assert!(patch.lines.windows(2).any(|lines| lines == ["-old", "\\ No newline at end of file"]));
}

#[test]
fn answer_review_pages_patch_bytes_and_handles_binary_and_budget_limits() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    let long = (0..60).map(|line| format!("{line}:{}\n", "word ".repeat(1600))).collect::<String>();
    let old = (0..5_000).map(|line| format!("old {line}\n")).collect::<String>();
    let new = (0..5_000).map(|line| format!("new {line}\n")).collect::<String>();
    point(
        &mut history,
        folder.path(),
        "first",
        1,
        &[("dense", old.as_bytes())],
        &[("dense", new.as_bytes()), ("long", long.as_bytes()), ("binary", b"\0\xff")],
    );
    let mut combined = vec![];
    loop {
        let page = history.review_patch(CONVERSATION, "first", "long", combined.len()).unwrap();
        assert!(!page.binary && !page.too_large);
        assert!(page.lines.len() <= 200);
        assert!(page.lines.iter().map(String::len).sum::<usize>() <= 128 * 1024);
        assert!(!page.lines.is_empty());
        combined.extend(page.lines);
        if combined.len() == page.total {
            break;
        }
    }
    assert_eq!(combined.iter().filter(|line| line.starts_with('+') && !line.starts_with("+++")).count(), 60);
    assert!(history.review_patch(CONVERSATION, "first", "binary", 0).unwrap().binary);
    let dense = history.review_patch(CONVERSATION, "first", "dense", 0).unwrap();
    assert!(dense.too_large && dense.lines.is_empty() && dense.total == 0);
    assert!(history.review_patch(CONVERSATION, "first", "missing", 0).is_err());
}

#[test]
fn answer_review_refuses_unsealed_and_unknown_file_versions_without_reading_objects() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    point(&mut history, folder.path(), "first", 1, &[("file", b"old")], &[("file", b"new")]);
    let point = history.points.values_mut().next().unwrap();
    point.info.sealed = false;
    point.after = None;
    history.reset_work();
    let page = history.review(CONVERSATION, "first", 0).unwrap();
    assert!(!page.sealed && page.files.is_empty() && page.coverage.incomplete);
    assert!(history.review_patch(CONVERSATION, "first", "file", 0).is_err());
    assert_eq!(history.work().object_reads, 0);
}

#[test]
fn a_shared_saved_interval_is_disclosed_without_changing_restore_authority() {
    let data = tempfile::tempdir().unwrap();
    let folder = tempfile::tempdir().unwrap();
    let mut history = History::open(data.path(), 1).unwrap();
    point(&mut history, folder.path(), "first", 1, &[("file", b"old")], &[("file", b"first")]);
    point(&mut history, folder.path(), "second", 2, &[("file", b"first")], &[("file", b"second")]);
    let snapshot =
        history.points.values().find(|point| point.info.message_id == "second").unwrap().after.as_ref().unwrap().as_ref().clone();
    let ids = history.points.keys().cloned().collect();
    history.seal(Sealing { ids, snapshot }, 3).unwrap();
    assert!(history.points.values().all(|point| !point.unknown_author));
    drop(history);
    let history = History::open(data.path(), 4).unwrap();
    assert!(history.review(CONVERSATION, "first", 0).unwrap().shared);
    assert!(history.review(CONVERSATION, "second", 0).unwrap().shared);
}

#[test]
fn small_diffs_reconstruct_both_versions_with_minimum_repeated_line_edits() {
    let sequences: Vec<Vec<&str>> = (0..=5)
        .flat_map(|len| (0..1 << len).map(move |bits| (0..len).map(|bit| if bits & (1 << bit) == 0 { "a\n" } else { "b\n" }).collect()))
        .collect();
    for old in &sequences {
        for new in &sequences {
            let mut lcs = vec![vec![0; new.len() + 1]; old.len() + 1];
            for (i, a) in old.iter().enumerate() {
                for (j, b) in new.iter().enumerate() {
                    lcs[i + 1][j + 1] = if a == b { lcs[i][j] + 1 } else { lcs[i][j + 1].max(lcs[i + 1][j]) };
                }
            }
            let edits = shortest_edits(old, new, DIFF_WORK).unwrap();
            assert_eq!(edits.iter().filter(|(sign, _)| *sign != ' ').count(), old.len() + new.len() - 2 * lcs[old.len()][new.len()]);
            assert_eq!(edits.iter().filter(|(sign, _)| *sign != '+').map(|(_, line)| *line).collect::<Vec<_>>(), *old);
            assert_eq!(edits.iter().filter(|(sign, _)| *sign != '-').map(|(_, line)| *line).collect::<Vec<_>>(), *new);
        }
    }
}
