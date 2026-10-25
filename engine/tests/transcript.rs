use faber_engine::store::{Disk, Fault, RealDisk};
use faber_engine::transcript::{MAX_PAGE_BYTES, TranscriptEntry, TranscriptKind, TranscriptStore};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

fn text(at: usize, value: impl Into<String>) -> TranscriptEntry {
    TranscriptEntry { at, kind: TranscriptKind::Text, tool: None, text: value.into() }
}

fn tool(at: usize, kind: TranscriptKind, id: &str, value: &str) -> TranscriptEntry {
    TranscriptEntry { at, kind, tool: Some(id.into()), text: value.into() }
}

#[test]
fn pages_reopen_and_round_trip_large_multilingual_text() {
    let dir = tempdir();
    let original = format!("{}{}{}", "Olá 🌎 ".repeat(3_000), "日本語 ".repeat(3_000), "مرحبا ".repeat(3_000));
    let entries: Vec<_> = faber_engine::transcript::split_text(&original).map(|part| text(17, part)).collect();
    assert!(original.len() > 16 * 1024);
    assert!(entries.len() > 1);
    let store = TranscriptStore::new(dir.path().join("transcripts"));
    for (page, entry) in entries.iter().enumerate() {
        store.append_page("c1", "child/unsafe:call", page as u64, std::slice::from_ref(entry)).unwrap();
    }

    let reopened = TranscriptStore::new(dir.path().join("transcripts"));
    let restored: Vec<_> =
        (0..entries.len()).flat_map(|page| reopened.read_page("c1", "child/unsafe:call", page as u64).unwrap().unwrap()).collect();
    assert_eq!(restored, entries);
    assert_eq!(restored.iter().map(|entry| entry.text.as_str()).collect::<Vec<_>>().concat(), original);
}

#[test]
fn append_is_idempotent_for_same_page_and_rejects_rewriting_it() {
    let dir = tempdir();
    let store = TranscriptStore::new(dir.path().join("transcripts"));
    let first = [text(1, "first")];
    store.append_page("c1", "call", 0, &first).unwrap();
    store.append_page("c1", "call", 0, &first).unwrap();

    assert_eq!(store.append_page("c1", "call", 0, &[text(2, "different")]), Err(Fault::Write));
    assert_eq!(store.read_page("c1", "call", 0).unwrap(), Some(first.to_vec()));
}

#[derive(Clone, Copy)]
enum FailAt {
    Write,
    Rename,
}

struct FailingDisk {
    fail_at: FailAt,
}

impl Disk for FailingDisk {
    fn write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if matches!(self.fail_at, FailAt::Write) {
            return Err(io::Error::other("injected write failure"));
        }
        RealDisk.write(path, bytes)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        if matches!(self.fail_at, FailAt::Rename) {
            return Err(io::Error::other("injected rename failure"));
        }
        RealDisk.rename(from, to)
    }
}

#[test]
fn failed_write_or_rename_leaves_old_page_and_removes_temp_file() {
    for fail_at in [FailAt::Write, FailAt::Rename] {
        let dir = tempdir();
        let root = dir.path().join("transcripts");
        let store = TranscriptStore::new(&root);
        let previous = [text(0, "saved")];
        store.append_page("c1", "call", 0, &previous).unwrap();

        let failing = TranscriptStore::with_disk(&root, Arc::new(FailingDisk { fail_at }));
        assert!(matches!(failing.append_page("c1", "call", 1, &[text(1, "new")]), Err(Fault::Write)));
        assert_eq!(store.read_page("c1", "call", 0).unwrap(), Some(previous.to_vec()));
        assert_eq!(store.read_page("c1", "call", 1).unwrap(), None);
        let call_dir = root.join("c1").join(faber_engine::hex(&Sha256::digest(b"call")));
        assert!(!call_dir.join("00000000000000000001.json.tmp").exists());
    }
}

#[test]
fn copy_keeps_only_selected_calls_and_survives_parent_deletion() {
    let dir = tempdir();
    let store = TranscriptStore::new(dir.path().join("transcripts"));
    let selected = [text(1, "kept")];
    let other = [text(2, "not copied")];
    store.append_page("parent", "selected-call", 0, &selected).unwrap();
    store.append_page("parent", "selected-call", 1, &[tool(2, TranscriptKind::Tool, "run", "")]).unwrap();
    store.append_page("parent", "other-call", 0, &other).unwrap();
    store.copy_calls("parent", "branch", &[("selected-call".into(), 2)]).unwrap();

    assert_eq!(store.read_page("branch", "selected-call", 0).unwrap(), Some(selected.to_vec()));
    assert_eq!(store.read_page("branch", "selected-call", 1).unwrap().unwrap()[0].kind, TranscriptKind::Tool);
    assert_eq!(store.read_page("branch", "other-call", 0).unwrap(), None);
    store.remove_conversation("parent").unwrap();
    assert_eq!(store.read_page("parent", "selected-call", 0).unwrap(), None);
    assert_eq!(store.read_page("branch", "selected-call", 0).unwrap(), Some(selected.to_vec()));
}

#[test]
fn copying_a_missing_page_fails_explicitly() {
    let dir = tempdir();
    let store = TranscriptStore::new(dir.path().join("transcripts"));
    store.append_page("parent", "call", 0, &[text(0, "available")]).unwrap();
    assert_eq!(store.copy_calls("parent", "branch", &[("call".into(), 2)]), Err(Fault::Corrupt));
    // Copy is page-atomic, not conversation-transactional; the caller owns rollback.
    store.remove_conversation("branch").unwrap();
    assert_eq!(store.read_page("branch", "call", 0).unwrap(), None);
}

#[test]
fn bounded_reader_rejects_corrupt_and_oversized_pages() {
    let dir = tempdir();
    let root = dir.path().join("transcripts");
    let store = TranscriptStore::new(&root);
    store.append_page("c1", "call", 0, &[text(0, "valid")]).unwrap();
    let call_dir = root.join("c1").join(faber_engine::hex(&Sha256::digest(b"call")));
    let page = call_dir.join("00000000000000000000.json");

    fs::write(&page, b"not json").unwrap();
    assert_eq!(store.read_page("c1", "call", 0), Err(Fault::Corrupt));
    fs::write(&page, vec![b' '; MAX_PAGE_BYTES + 1]).unwrap();
    assert_eq!(store.read_page("c1", "call", 0), Err(Fault::Corrupt));
}

#[test]
fn invalid_ids_fail_without_creating_paths_outside_root() {
    let dir = tempdir();
    let root = dir.path().join("transcripts");
    let store = TranscriptStore::new(&root);
    assert_eq!(store.append_page("../outside", "call", 0, &[text(0, "x")]), Err(Fault::Corrupt));
    assert_eq!(store.append_page("c1", "", 0, &[text(0, "x")]), Err(Fault::Corrupt));
    assert!(!root.exists());
    assert!(!dir.path().join("outside").exists());
}
