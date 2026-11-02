//! Rewind uses scripted Git name listings and real temporary disk files, never a Git or CLI process.
use crate::support;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Conversation, Message, Role, Vendor};
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::rewind::{Cursor, FILE_LIMIT, History, Mode, RETENTION, RedoRequest, RestoreRequest};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const CONVERSATION: &str = "3611dcb7-6387-47c6-8b25-1b6f20903171";

#[derive(Default)]
struct Repo {
    names: Vec<String>,
    ignored: Vec<String>,
    calls: Vec<Invocation>,
    non_git: bool,
    fail: bool,
}

#[derive(Clone, Default)]
struct Git(Arc<Mutex<Repo>>);

impl Executor for Git {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        let mut args = call.args.as_slice();
        while args.first().is_some_and(|a| a == "-c") {
            args = &args[2..];
        }
        assert!(call.deadline.is_some(), "history scans need a bounded deadline");
        assert!(call.args.iter().any(|a| a.starts_with("core.hooksPath=")));
        let mut repo = self.0.lock().unwrap();
        repo.calls.push(call.clone());
        let private = args.first().is_some_and(|a| a.starts_with("--git-dir="));
        if private {
            assert!(args[1].starts_with("--work-tree="));
            args = &args[2..];
        }
        let ignored = args.iter().any(|arg| arg == "--ignored");
        if ignored {
            assert_eq!(args, ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z", "--"]);
        } else if private {
            assert_eq!(args, ["ls-files", "--others", "--exclude-standard", "-z", "--"]);
        } else {
            assert_eq!(args, ["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--"]);
        }
        if repo.fail {
            return Err(GitError::Command);
        }
        Ok(Output {
            success: !repo.non_git || private,
            stdout: if ignored { &repo.ignored } else { &repo.names }
                .iter()
                .flat_map(|name| name.as_bytes().iter().copied().chain([0]))
                .collect(),
        })
    }
}

struct Setup {
    folder: tempfile::TempDir,
    data: support::DataFolder,
    history: History,
    git: Git,
    conversation: Conversation,
}

impl Setup {
    fn new() -> Self {
        let folder = support::canonical_tempdir().unwrap();
        let data = support::DataFolder::new();
        let history = History::open(data.path(), 1).expect("owner-only history must be usable by its owner");
        let conversation = serde_json::from_value(json!({
            "id": CONVERSATION, "title": "Rewind", "area": "project", "workingFolder": folder.path(),
            "createdAt": 1, "updatedAt": 1, "messages": []
        }))
        .unwrap();
        Self { folder, data, history, git: Git::default(), conversation }
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        let path = self.folder.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn names(&self, paths: &[&str]) {
        self.git.0.lock().unwrap().names = paths.iter().map(|p| p.to_string()).collect();
    }

    fn capture(&mut self, message: &str, at: u64) -> faber_engine::rewind::PointInfo {
        self.conversation.messages.push(Message::new(message.into(), Role::User, "Change the files".into(), at));
        self.history.capture(&mut self.git, self.folder.path(), &self.conversation, message, &format!("turn-{message}"), at, false).unwrap()
    }

    fn seal(&mut self, at: u64) {
        let scan = self.history.scan_seal(&mut self.git, self.folder.path()).unwrap();
        self.history.seal(scan, at).unwrap();
    }

    fn request(&self, message: &str, mode: Mode) -> RestoreRequest {
        RestoreRequest { conversation_id: CONVERSATION.into(), message_id: message.into(), mode, saved_versions: vec![] }
    }

    fn bytes(&self, path: &str) -> Vec<u8> {
        std::fs::read(self.folder.path().join(path)).unwrap()
    }

    fn redo(&mut self, id: &str, choices: &[&str], at: u64) -> faber_engine::rewind::RestoreResult {
        self.history
            .redo(
                &RedoRequest {
                    conversation_id: CONVERSATION.into(),
                    id: id.into(),
                    saved_versions: choices.iter().map(|p| p.to_string()).collect(),
                },
                &mut self.conversation,
                at,
            )
            .unwrap()
    }
}

#[test]
fn rewind_reuses_only_unchanged_validated_files_and_forgets_tokens_on_restart() {
    let mut s = Setup::new();
    s.write("source", b"original");
    s.names(&["source"]);
    s.capture("first", 1);
    let root = faber_winproc::rewind::Root::open(s.folder.path()).unwrap();
    let token = faber_winproc::rewind::cache_fingerprint(&root.open_file("source").unwrap()).unwrap();
    s.history.reset_work();
    s.seal(2);
    if token.is_some() {
        assert_eq!(s.history.work().files_reused, 1);
        assert_eq!(s.history.work().files_read, 0);
        assert_eq!(s.history.work().object_flushes, 0, "unchanged sealing publishes only its manifest");
    }

    let modified = std::fs::metadata(s.folder.path().join("source")).unwrap().modified().unwrap();
    {
        let mut file = std::fs::OpenOptions::new().write(true).open(s.folder.path().join("source")).unwrap();
        std::io::Write::write_all(&mut file, b"changed!").unwrap();
        file.set_modified(modified).unwrap();
    }
    s.history.reset_work();
    s.capture("second", 3);
    assert_eq!(s.history.work().files_read, 1, "a same-size edit with restored modification time must be read");
    assert_eq!(s.history.work().files_reused, 0);
    s.seal(4);
    drop(root);
    s.history = History::open(s.data.path(), 5).unwrap();
    s.history.reset_work();
    s.capture("third", 6);
    assert_eq!(s.history.work().files_read, 1, "unpersisted cache tokens cannot survive restart");
    assert_eq!(s.history.work().files_reused, 0);
    s.seal(7);

    // A formerly cached ordinary file becoming multiply linked must be rejected before content admission.
    std::fs::hard_link(s.folder.path().join("source"), s.folder.path().join("alias")).unwrap();
    s.history.reset_work();
    let point = s.capture("fourth", 8);
    assert_eq!(point.coverage.saved, 0);
    assert!(point.coverage.incomplete);
    assert_eq!(s.history.work().files_read, 0);
    assert_eq!(s.history.work().files_reused, 0);
}

#[test]
fn rewind_a_durable_pack_without_a_published_manifest_is_reclaimed_at_restart() {
    let mut s = Setup::new();
    s.write("source", b"original bytes");
    s.names(&["source"]);
    s.capture("first", 1);
    s.seal(2);
    let folder = std::fs::read_dir(s.data.path().join("rewind")).unwrap().next().unwrap().unwrap().path();
    let pack_names = || -> std::collections::BTreeSet<_> {
        std::fs::read_dir(folder.join("objects/pack")).unwrap().map(|entry| entry.unwrap().file_name()).collect()
    };
    let retained = pack_names();
    assert!(!retained.is_empty());
    // Refuse the manifest after payload publication, then simulate startup before another turn can clean it.
    let points = folder.join("points");
    let held = folder.join("held-points");
    std::fs::rename(&points, &held).unwrap();
    std::fs::write(&points, b"fault: this parent is not a directory").unwrap();
    s.write("source", b"different bytes");
    s.conversation.messages.push(Message::new("second".into(), Role::User, "Change again".into(), 3));
    s.history.reset_work();
    let error = s.history.capture(&mut s.git, s.folder.path(), &s.conversation, "second", "second-turn", 3, false).unwrap_err();
    assert!(error.starts_with("error."));
    assert_eq!(s.history.work().object_flushes, 1, "the payload was durable before the failed manifest");
    assert_eq!(s.history.list(CONVERSATION).len(), 1);
    assert!(pack_names().len() > retained.len());
    std::fs::remove_file(&points).unwrap();
    std::fs::rename(&held, &points).unwrap();
    s.history = History::open(s.data.path(), 4).unwrap();
    assert_eq!(s.history.list(CONVERSATION).len(), 1);
    assert_eq!(pack_names(), retained, "only unreachable payload/index files are reclaimed");
    assert_eq!(s.bytes("source"), b"different bytes");
}

#[test]
fn rewind_restores_dirty_staged_untracked_binary_deletion_and_creation_without_user_git_writes() {
    let mut s = Setup::new();
    for (path, bytes) in [
        ("dirty.txt", b"dirty before".as_slice()),
        ("staged.txt", b"staged before"),
        ("untracked.txt", b"untracked before"),
        ("binary.bin", b"\0before\xff"),
        ("removed.txt", b"recreate me"),
    ] {
        s.write(path, bytes);
    }
    s.write(".git/index", b"person index");
    s.write(".git/HEAD", b"ref: refs/heads/person");
    s.names(&["dirty.txt", "staged.txt", "untracked.txt", "binary.bin", "removed.txt"]);
    let point = s.capture("m1", 1);
    assert_eq!(point.coverage.saved, 5);
    for path in ["dirty.txt", "staged.txt", "untracked.txt", "binary.bin"] {
        s.write(path, b"after");
    }
    std::fs::remove_file(s.folder.path().join("removed.txt")).unwrap();
    s.write("created.txt", b"created by command");
    s.names(&["dirty.txt", "staged.txt", "untracked.txt", "binary.bin", "removed.txt", "created.txt"]);
    s.seal(2);
    s.write("unlisted.txt", b"keep outside saved range");
    let request = s.request("m1", Mode::Both);
    let preview = s.history.preview(&request).unwrap();
    assert_eq!((preview.replacements, preview.recreations, preview.removals, preview.conflicts), (4, 1, 1, 0));
    assert_eq!(preview.notice, "note.rewind.oldHandle");
    let restored = s.history.restore(&request, &mut s.conversation, 3).unwrap();
    assert!(!restored.partial && restored.failed.is_empty());
    assert_eq!(restored.restored.len(), 6);
    assert_eq!(s.bytes("binary.bin"), b"\0before\xff");
    assert_eq!(s.bytes("dirty.txt"), b"dirty before");
    assert_eq!(s.bytes("staged.txt"), b"staged before");
    assert_eq!(s.bytes("untracked.txt"), b"untracked before");
    assert_eq!(s.bytes("removed.txt"), b"recreate me");
    assert!(!s.folder.path().join("created.txt").exists());
    assert_eq!(s.bytes("unlisted.txt"), b"keep outside saved range");
    assert!(s.conversation.messages.is_empty());
    s.history.acknowledge(&restored.id).unwrap();
    let calls = s.git.0.lock().unwrap().calls.len();
    let redone = s.redo(&restored.id, &[], 4);
    assert!(!redone.partial);
    assert_eq!(s.bytes("created.txt"), b"created by command");
    assert!(!s.folder.path().join("removed.txt").exists());
    assert_eq!(s.bytes("binary.bin"), b"after");
    assert_eq!(s.conversation.messages[0].id, "m1");
    assert_eq!(s.git.0.lock().unwrap().calls.len(), calls, "redo never runs a command");
    assert_eq!(s.bytes(".git/index"), b"person index");
    assert_eq!(s.bytes(".git/HEAD"), b"ref: refs/heads/person");
}

#[test]
fn rewind_conversation_only_works_without_file_history_and_redo_preserves_grants_and_spend() {
    let mut s = Setup::new();
    s.write("file", b"never change");
    s.conversation.messages =
        vec![Message::new("earlier".into(), Role::User, "earlier".into(), 1), Message::new("cut".into(), Role::User, "later".into(), 2)];
    s.conversation.sessions.insert(Vendor::Claude, "old-native-session".into());
    s.conversation.allow_always = Some(vec!["Read".into()]);
    s.conversation.cost_base.insert(Vendor::Claude, faber_engine::usage::CostBase { session: "charged-session".into(), total: 4.25 });
    s.conversation.keep_going = Some(Default::default());
    let before = serde_json::to_value(&s.conversation).unwrap();
    let request = s.request("cut", Mode::Conversation);
    let restored = s.history.restore(&request, &mut s.conversation, 3).unwrap();
    assert_eq!(s.conversation.messages.len(), 1);
    assert!(s.conversation.sessions.is_empty());
    assert_eq!(s.conversation.rewind_notice.as_deref(), Some("note.rewind.savedMessages"));
    assert_eq!(s.conversation.allow_always, Some(vec!["Read".into()]));
    s.history.acknowledge(&restored.id).unwrap();
    assert!(!s.redo(&restored.id, &[], 4).partial);
    assert_eq!(s.conversation.messages.len(), 2);
    let after = serde_json::to_value(&s.conversation).unwrap();
    assert_eq!(before["allowAlways"], after["allowAlways"]);
    assert_eq!(before["costBase"], after["costBase"]);
    assert!(s.conversation.keep_going.is_none());
    assert_eq!(s.bytes("file"), b"never change");
    assert!(s.git.0.lock().unwrap().calls.is_empty());
}

#[test]
fn rewind_files_only_keeps_messages_and_adds_a_fresh_continuation_notice() {
    let mut s = Setup::new();
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("m1", 1);
    s.write("file", b"after");
    s.seal(2);
    let messages = s.conversation.messages.clone();
    let result = s.history.restore(&s.request("m1", Mode::Files), &mut s.conversation, 3).unwrap();
    assert!(!result.partial);
    assert_eq!(s.conversation.messages, messages);
    assert!(s.conversation.rewind_summary.as_deref().is_some_and(|summary| summary.contains("Do not replay earlier commands")));
    assert_eq!(s.bytes("file"), b"before");
    s.history.acknowledge(&result.id).unwrap();
    s.redo(&result.id, &[], 4);
    assert_eq!(s.conversation.messages, messages);
    assert_eq!(s.bytes("file"), b"after");
}

#[test]
fn rewind_redo_discovery_survives_restart_without_a_remaining_message_or_file_point() {
    for mode in [Mode::Conversation, Mode::Both] {
        let mut s = Setup::new();
        s.conversation.messages.push(Message::new("cut".into(), Role::User, "later".into(), 1));
        let result = s.history.restore(&s.request("cut", mode), &mut s.conversation, 2).unwrap();
        let pending = s.history.redos(&s.conversation, RETENTION + 2);
        assert_eq!(pending.len(), 1, "pending recovery stays discoverable without a remaining message, even after redo expires");
        assert!(pending[0].recovery);
        let pending_request = RedoRequest { conversation_id: CONVERSATION.into(), id: result.id.clone(), saved_versions: vec![] };
        assert!(s.history.redo_preview(&pending_request, &s.conversation, RETENTION + 2).unwrap().recovery);
        s.history.acknowledge(&result.id).unwrap();
        drop(s.history);
        s.history = History::open(s.data.path(), 3).unwrap();
        let infos = s.history.redos(&s.conversation, 3);
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, result.id);
        assert_eq!(infos[0].message_id, "cut");
        assert_eq!(infos[0].mode, mode);
        assert_eq!(infos[0].expires_at, RETENTION + 2);
        assert!(!infos[0].recovery);
        let request = RedoRequest { conversation_id: CONVERSATION.into(), id: result.id, saved_versions: vec![] };
        let preview = s.history.redo_preview(&request, &s.conversation, 3).unwrap();
        assert_eq!(preview.messages, 1);
        assert!(preview.files.is_empty());
        assert!(s.history.redos(&s.conversation, RETENTION + 2).is_empty());
        assert_eq!(s.history.redo_preview(&request, &s.conversation, RETENTION + 2).unwrap_err(), "error.rewind.unavailable");
        s.conversation.messages.push(Message::new("new".into(), Role::User, "new direction".into(), 4));
        assert!(s.history.redos(&s.conversation, 4).is_empty());
        assert_eq!(s.history.redo_preview(&request, &s.conversation, 4).unwrap_err(), "error.rewind.conversationChanged");
    }
}

#[test]
fn rewind_redo_preview_uses_the_mutations_conflict_checks_and_counts() {
    let mut s = Setup::new();
    s.names(&["replace", "remove"]);
    s.write("replace", b"before");
    s.write("remove", b"before");
    s.capture("cut", 1);
    s.write("replace", b"after");
    std::fs::remove_file(s.folder.path().join("remove")).unwrap();
    s.write("recreate", b"after");
    s.names(&["replace", "recreate"]);
    s.seal(2);
    let result = s.history.restore(&s.request("cut", Mode::Both), &mut s.conversation, 3).unwrap();
    assert!(!result.partial);
    s.history.acknowledge(&result.id).unwrap();
    let request = RedoRequest { conversation_id: CONVERSATION.into(), id: result.id, saved_versions: vec![] };
    let preview = s.history.redo_preview(&request, &s.conversation, 4).unwrap();
    assert_eq!((preview.replacements, preview.recreations, preview.removals, preview.conflicts), (1, 1, 1, 0));
    assert_eq!(preview.messages, 1);
    // Replacing the inode with the same bytes must still ask, just as redo itself does.
    std::fs::rename(s.folder.path().join("replace"), s.folder.path().join("held-original")).unwrap();
    s.write("replace", b"before");
    s.write("remove", b"my edit");
    let preview = s.history.redo_preview(&request, &s.conversation, 5).unwrap();
    assert_eq!(preview.conflicts, 2);
    let result = s.history.redo(&request, &mut s.conversation, 5).unwrap();
    assert_eq!(result.kept.len(), 2);
    assert_eq!(s.bytes("remove"), b"my edit");
    // File changes alone do not acknowledge the caller's durable conversation save.
    let pending = s.history.redos(&s.conversation, 6);
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, result.id);
    assert!(pending[0].recovery);
    assert!(s.history.redo_preview(&request, &s.conversation, 6).unwrap().recovery);
    assert_eq!(s.history.redo(&request, &mut s.conversation, 6).unwrap_err(), "error.rewind.unavailable");
    s.history.acknowledge(&result.id).unwrap();
    assert!(s.history.redos(&s.conversation, 6).is_empty(), "an applied redo cannot be offered twice");
    assert_eq!(s.history.redo_preview(&request, &s.conversation, 6).unwrap_err(), "error.rewind.unavailable");
    assert_eq!(s.history.redo(&request, &mut s.conversation, 6).unwrap_err(), "error.rewind.unavailable");
}

#[test]
fn rewind_both_without_a_file_point_cuts_messages_and_reports_partial_with_redo() {
    let mut s = Setup::new();
    s.write("file", b"keep this version");
    s.conversation.messages = vec![Message::new("cut".into(), Role::User, "later".into(), 1)];
    let request = s.request("cut", Mode::Both);
    let preview = s.history.preview(&request).unwrap();
    assert!(preview.coverage.unavailable && preview.coverage.incomplete);
    assert!(preview.files.is_empty());
    let restored = s.history.restore(&request, &mut s.conversation, 2).unwrap();
    assert!(restored.partial);
    assert!(s.conversation.messages.is_empty());
    assert_eq!(s.bytes("file"), b"keep this version");
    s.history.acknowledge(&restored.id).unwrap();
    assert!(s.redo(&restored.id, &[], 3).partial);
    assert_eq!(s.conversation.messages[0].id, "cut");
    assert_eq!(s.bytes("file"), b"keep this version");
}

#[test]
fn rewind_conflicts_default_to_keep_and_explicit_choices_keep_redo_versions() {
    let mut s = Setup::new();
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("m1", 1);
    s.write("file", b"turn output");
    s.seal(2);
    s.write("file", b"my later edit");
    let mut request = s.request("m1", Mode::Files);
    assert_eq!(s.history.preview(&request).unwrap().conflicts, 1);
    let kept = s.history.restore(&request, &mut s.conversation, 3).unwrap();
    assert!(kept.partial && kept.restored.is_empty());
    assert_eq!(kept.kept, ["file"]);
    assert_eq!(s.bytes("file"), b"my later edit");
    s.history.acknowledge(&kept.id).unwrap();
    request.saved_versions = vec!["file".into()];
    let restored = s.history.restore(&request, &mut s.conversation, 4).unwrap();
    assert_eq!(s.bytes("file"), b"before");
    s.history.acknowledge(&restored.id).unwrap();
    s.write("file", b"edit after restore");
    let redo = s.redo(&restored.id, &[], 5);
    assert!(redo.partial && redo.kept == ["file"]);
    assert_eq!(s.bytes("file"), b"edit after restore");
}

#[test]
fn rewind_missing_parents_are_recreated_and_redo_never_deletes_unlisted_files() {
    let mut s = Setup::new();
    s.write("deep/nested/file", b"before");
    s.names(&["deep/nested/file"]);
    s.capture("m1", 1);
    std::fs::remove_file(s.folder.path().join("deep/nested/file")).unwrap();
    std::fs::remove_dir(s.folder.path().join("deep/nested")).unwrap();
    std::fs::remove_dir(s.folder.path().join("deep")).unwrap();
    s.seal(2);
    let restored = s.history.restore(&s.request("m1", Mode::Files), &mut s.conversation, 3).unwrap();
    assert!(!restored.partial);
    assert_eq!(s.bytes("deep/nested/file"), b"before");
    s.write("deep/nested/unlisted", b"keep");
    s.history.acknowledge(&restored.id).unwrap();
    s.redo(&restored.id, &[], 4);
    assert!(!s.folder.path().join("deep/nested/file").exists());
    assert_eq!(s.bytes("deep/nested/unlisted"), b"keep");
}

#[test]
fn rewind_expired_later_points_require_consent_even_when_the_message_marker_was_available() {
    let mut s = Setup::new();
    s.write("file", b"zero");
    s.names(&["file"]);
    let first = s.capture("m1", 1);
    s.conversation.messages.last_mut().unwrap().rewind = Some(first);
    s.write("file", b"one");
    s.seal(2);
    let second = s.capture("m2", 3);
    s.conversation.messages.last_mut().unwrap().rewind = Some(second);
    s.write("file", b"two");
    s.seal(4);
    let request = s.request("m1", Mode::Files);
    let pinned = s.history.restore(&request, &mut s.conversation, 10).unwrap();
    s.history.acknowledge(&pinned.id).unwrap();
    s.history.cleanup(RETENTION + 3).unwrap();
    assert_eq!(s.history.list(CONVERSATION).len(), 1, "the first point is journal-pinned but the later point expired");
    s.write("file", b"one");
    let restored = s.history.restore(&request, &mut s.conversation, RETENTION + 4).unwrap();
    assert!(restored.partial);
    assert_eq!(restored.kept, ["file"]);
    assert_eq!(s.bytes("file"), b"one");
}

#[test]
fn rewind_redo_refuses_same_bytes_with_a_changed_file_identity() {
    let mut s = Setup::new();
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("m1", 1);
    s.write("file", b"after");
    s.seal(2);
    let result = s.history.restore(&s.request("m1", Mode::Files), &mut s.conversation, 3).unwrap();
    s.history.acknowledge(&result.id).unwrap();
    std::fs::rename(s.folder.path().join("file"), s.folder.path().join("held-inode")).unwrap();
    s.write("file", b"before");
    let redo = s.redo(&result.id, &[], 4);
    assert!(redo.partial);
    assert_eq!(redo.kept, ["file"]);
    assert_eq!(s.bytes("file"), b"before");
    assert_eq!(s.bytes("held-inode"), b"before");
}

#[test]
fn rewind_explicit_restore_and_redo_recover_the_persons_conflicting_version() {
    let mut s = Setup::new();
    s.write("file", b"baseline");
    s.names(&["file"]);
    s.capture("m1", 1);
    s.write("file", b"turn version");
    s.seal(2);
    s.write("file", b"my version");
    let mut request = s.request("m1", Mode::Files);
    request.saved_versions.push("file".into());
    let restored = s.history.restore(&request, &mut s.conversation, 3).unwrap();
    assert_eq!(s.bytes("file"), b"baseline");
    s.history.acknowledge(&restored.id).unwrap();
    let redo = s.redo(&restored.id, &[], 4);
    assert!(!redo.partial);
    assert_eq!(s.bytes("file"), b"my version", "redo restores the conflict saved before replacement");
}

#[test]
fn rewind_excludes_secret_names_nested_repositories_hardlinks_and_ignored_names() {
    let mut s = Setup::new();
    for path in ["safe", ".codex/auth.json", ".claude/token", ".env", ".git/config", "nested/.git/config", "nested/source", "ignored"] {
        s.write(path, b"private marker");
    }
    std::fs::hard_link(s.folder.path().join("safe"), s.folder.path().join("hardlink")).unwrap();
    s.write("ordinary", b"safe source");
    s.write("directory/file", b"not listed");
    s.names(&[
        "ordinary",
        "hardlink",
        "directory",
        ".codex/auth.json",
        ".claude/token",
        ".env",
        ".git/config",
        "nested/source",
        "../outside",
    ]);
    let point = s.capture("m1", 1);
    assert_eq!(point.coverage.saved, 1);
    assert!(point.coverage.incomplete);
    assert_eq!(point.coverage.skipped, 1, "only the unsafe hard link is incomplete; directories and secret names are silent exclusions");
    assert!(s.history.preview(&s.request("m1", Mode::Files)).unwrap().files.iter().all(|f| f.path == "ordinary"));
    assert_eq!(s.bytes("ignored"), b"private marker", "Git-ignored paths were never returned by the executor");
}

#[test]
fn rewind_oversize_is_partial_and_listing_failure_is_an_error_key() {
    let mut s = Setup::new();
    s.write("safe", b"before");
    std::fs::File::create(s.folder.path().join("large")).unwrap().set_len(FILE_LIMIT + 1).unwrap();
    s.names(&["safe", "large"]);
    let point = s.capture("m1", 1);
    assert_eq!((point.coverage.saved, point.coverage.skipped), (1, 1));
    assert!(point.coverage.incomplete);
    s.git.0.lock().unwrap().fail = true;
    let error = s.history.capture(&mut s.git, s.folder.path(), &s.conversation, "m1", "turn2", 2, false).unwrap_err();
    assert!(error.starts_with("error."));
    assert_eq!(s.bytes("safe"), b"before");
}

#[test]
fn rewind_non_git_folders_use_only_a_private_git_dir_for_ignore_listing() {
    let mut s = Setup::new();
    s.write("safe", b"before");
    s.names(&["safe"]);
    s.git.0.lock().unwrap().non_git = true;
    assert_eq!(s.capture("m1", 1).coverage.saved, 1);
    let repo = s.git.0.lock().unwrap();
    assert!(repo.calls.len() >= 2);
    let private = repo.calls.iter().find_map(|call| call.args.iter().find_map(|arg| arg.strip_prefix("--git-dir="))).unwrap();
    // Git takes a native path without Windows' verbatim prefix; containment compares resolved disk paths.
    let private = Path::new(private).canonicalize().unwrap();
    let data = s.data.path().canonicalize().unwrap();
    assert!(private.starts_with(&data));
    assert_eq!(private.parent().unwrap(), data.join("rewind"));
    assert!(!s.folder.path().join(".git").exists());
}

#[test]
fn rewind_a_previously_ignored_file_is_never_treated_as_a_new_file_to_remove() {
    let mut s = Setup::new();
    s.write("private-cache/file", b"preexisting ignored bytes");
    s.git.0.lock().unwrap().ignored = vec!["private-cache/".into()];
    s.capture("m1", 1);
    s.git.0.lock().unwrap().ignored.clear();
    s.names(&["private-cache/file"]);
    s.seal(2);
    let request = s.request("m1", Mode::Files);
    assert!(!s.history.preview(&request).unwrap().files.iter().any(|file| file.path == "private-cache/file"));
    s.history.restore(&request, &mut s.conversation, 3).unwrap();
    assert_eq!(s.bytes("private-cache/file"), b"preexisting ignored bytes");
}

#[test]
fn rewind_unknown_postimages_need_per_file_consent_and_cursors_survive_restart() {
    let mut s = Setup::new();
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("m1", 1);
    s.history.cursor(CONVERSATION, Cursor { entry: Some("native-entry".into()), turn: Some("native-turn".into()) });
    s.write("file", b"late helper output");
    assert_eq!(s.history.preview(&s.request("m1", Mode::Files)).unwrap().conflicts, 1);
    assert!(!s.history.list(CONVERSATION)[0].sealed);
    s.seal(2);
    let restarted = History::open(s.data.path(), 3).unwrap();
    let point = &restarted.list(CONVERSATION)[0];
    assert!(point.sealed);
    assert_eq!(point.cursor.entry.as_deref(), Some("native-entry"));
    assert_eq!(point.cursor.turn.as_deref(), Some("native-turn"));
}

#[test]
fn rewind_restart_recovers_unacknowledged_conversation_then_allows_redo() {
    let mut s = Setup::new();
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("m1", 1);
    s.write("file", b"after");
    s.seal(2);
    let mut durable = s.conversation.clone();
    let restored = s.history.restore(&s.request("m1", Mode::Both), &mut s.conversation, 3).unwrap();
    let mut restarted = History::open(s.data.path(), 4).unwrap();
    assert!(restarted.recovery_for(CONVERSATION));
    restarted.recover_conversation(&mut durable).unwrap();
    assert!(durable.messages.is_empty());
    assert_eq!(s.bytes("file"), b"before");
    restarted.acknowledge(&restored.id).unwrap();
    assert!(!restarted.recovery_for(CONVERSATION));
    restarted
        .redo(&RedoRequest { conversation_id: CONVERSATION.into(), id: restored.id, saved_versions: vec![] }, &mut durable, 5)
        .unwrap();
    assert_eq!(durable.messages[0].id, "m1");
    assert_eq!(s.bytes("file"), b"after");
}

#[test]
fn rewind_cleanup_pins_open_baselines_and_recovery_and_leaves_unowned_names() {
    let mut s = Setup::new();
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("m1", 1);
    std::fs::write(s.data.path().join("rewind/keep-user-file"), b"keep").unwrap();
    s.history.cleanup(RETENTION + 2).unwrap();
    assert_eq!(s.history.list(CONVERSATION).len(), 1, "an active baseline stays pinned");
    s.write("file", b"after");
    s.seal(2);
    let restored = s.history.restore(&s.request("m1", Mode::Both), &mut s.conversation, 3).unwrap();
    s.history.cleanup(RETENTION + 4).unwrap();
    assert!(s.history.recovery_for(CONVERSATION), "unfinished recovery never expires");
    assert_eq!(s.history.list(CONVERSATION).len(), 1);
    s.history.acknowledge(&restored.id).unwrap();
    s.history.cleanup(RETENTION + 5).unwrap();
    assert!(s.history.list(CONVERSATION).is_empty());
    assert_eq!(std::fs::read(s.data.path().join("rewind/keep-user-file")).unwrap(), b"keep");
}

#[tokio::test]
async fn rewind_recovery_can_be_retried_after_the_working_folder_returns() {
    use faber_engine::model::Area;
    use faber_engine::store::Store;
    use faber_engine::{Engine, EngineOptions};
    use std::collections::BTreeMap;
    use std::time::Duration;

    let mut s = Setup::new();
    s.conversation.area = Area::Chat;
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("cut", 1);
    s.write("file", b"after");
    s.seal(2);
    Store::new(s.data.path()).save(&s.conversation);
    s.history.restore(&s.request("cut", Mode::Both), &mut s.conversation, 3).unwrap();
    let mut other = s.conversation.clone();
    other.id = "e2c1b82e-98e8-4bca-9214-dd342896c10c".into();
    other.working_folder = Some(s.data.path().to_string_lossy().into_owned());
    Store::new(s.data.path()).save(&other);
    let held = s.data.path().join("held-working-folder");
    std::fs::rename(s.folder.path(), &held).unwrap();
    let (spawn, _pipes) = support::pipes();
    let engine = Engine::new(
        EngineOptions {
            data_dir: s.data.path().into(),
            #[cfg(windows)]
            chat_dir: s.data.chat_path().into(),
            clis: BTreeMap::new(),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    // Missing folders pass the folder policy; recovery fails when it tries to reopen the saved root.
    assert_eq!(engine.rewind_recover(CONVERSATION).unwrap_err(), "error.rewind.recovery");
    let pending = engine.rewind_redos(CONVERSATION).unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].recovery && !pending[0].id.is_empty(), "the original journal stays available for retry");
    for discard in [false, true] {
        assert_eq!(engine.remove_with(CONVERSATION, discard).unwrap_err(), "error.rewind.deleteRecovery");
        assert!(Store::new(s.data.path()).get(CONVERSATION).is_some(), "recovery must keep its saved conversation");
        assert!(engine.open(CONVERSATION).is_some(), "a refused delete keeps the loaded conversation too");
        assert_eq!(engine.rewind_redos(CONVERSATION).unwrap()[0].id, pending[0].id, "deletion cannot discard its journal");
    }
    let infos = engine.rewind_redos(&other.id).unwrap();
    assert_eq!(infos.len(), 1, "global recovery is reachable from an unrelated empty conversation");
    assert!(infos[0].recovery && infos[0].id.is_empty());
    let preview =
        engine.rewind_redo_preview(RedoRequest { conversation_id: other.id.clone(), id: String::new(), saved_versions: vec![] }).unwrap();
    assert!(preview.recovery);
    std::fs::rename(&held, s.folder.path()).unwrap();
    assert_eq!(engine.rewind_restore(s.request("cut", Mode::Conversation)).unwrap_err(), "error.rewind.recovery");
    engine.rewind_recover(&other.id).unwrap();
    engine.rewind_recover(CONVERSATION).unwrap();
    assert!(engine.rewind_redos(&other.id).unwrap().is_empty(), "successful recovery removes the global entry");
    assert!(engine.open(CONVERSATION).unwrap().messages.is_empty());
    assert_eq!(engine.open(CONVERSATION).unwrap().rewind_notice.as_deref(), Some("note.rewind.savedMessages"));
    assert_eq!(std::fs::read(s.folder.path().join("file")).unwrap(), b"before");
    engine.rewind_recover(CONVERSATION).unwrap();
    engine.remove_with(CONVERSATION, true).unwrap();
    assert!(Store::new(s.data.path()).get(CONVERSATION).is_none(), "a completed recovery no longer prevents deletion");
    engine.shutdown();
}

#[cfg(unix)]
#[tokio::test]
async fn rewind_startup_cleans_empty_project_quarantines_without_a_pending_journal_or_list_event() {
    use faber_engine::{Engine, EngineOptions};
    let mut s = Setup::new();
    // Engine startup uses the real clock; an epoch-dated sealed point expires before roots are gathered.
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    s.capture("m1", at);
    s.seal(at);
    let quarantine = s.folder.path().join(".faber-rewind-cleanup-17-9");
    faber_winproc::rewind::private_dir(&quarantine).unwrap();
    let (sent, mut events) = tokio::sync::mpsc::unbounded_channel();
    let engine = Engine::new(
        EngineOptions {
            data_dir: s.data.path().into(),
            #[cfg(windows)]
            chat_dir: s.data.chat_path().into(),
            clis: Default::default(),
            spawn: Arc::new(|_, _, _| panic!("startup cleanup must not launch a process")),
            windows: false,
            claude_env_billed: false,
            max_warm: 0,
            idle: std::time::Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(move |event| drop(sent.send(event))),
    )
    .unwrap();
    assert_eq!(
        History::open(s.data.path(), at).unwrap().list(CONVERSATION).len(),
        1,
        "the project root must still have a registered point"
    );
    assert!(!quarantine.exists(), "a sealed point's root must be cleaned even without a recovery journal");
    engine.recover_rewind();
    assert!(events.try_recv().is_err(), "empty cleanup does not refresh the conversation list");
    engine.shutdown();
}

#[test]
fn rewind_new_conversation_messages_refuse_stale_redo() {
    let mut s = Setup::new();
    s.conversation.messages.push(Message::new("m1".into(), Role::User, "later".into(), 1));
    let restored = s.history.restore(&s.request("m1", Mode::Conversation), &mut s.conversation, 2).unwrap();
    s.history.acknowledge(&restored.id).unwrap();
    s.conversation.messages.push(Message::new("new".into(), Role::User, "new direction".into(), 3));
    assert_eq!(
        s.history
            .redo(&RedoRequest { conversation_id: CONVERSATION.into(), id: restored.id, saved_versions: vec![] }, &mut s.conversation, 4)
            .unwrap_err(),
        "error.rewind.conversationChanged"
    );
    assert_eq!(s.conversation.messages[0].id, "new");
}

#[tokio::test]
async fn rewind_unreadable_history_keeps_recovery_reachable_from_an_empty_conversation() {
    use faber_engine::model::Area;
    use faber_engine::store::Store;
    use faber_engine::{Engine, EngineOptions};
    use std::collections::BTreeMap;
    use std::time::Duration;

    let mut s = Setup::new();
    s.conversation.area = Area::Chat;
    s.conversation.rewind_notice = Some("note.rewind.savedMessages".into());
    Store::new(s.data.path()).save(&s.conversation);
    let journals = s.data.path().join("rewind").join("a".repeat(64)).join("journals");
    std::fs::create_dir_all(&journals).unwrap();
    std::fs::write(journals.join("c4718dc3-5227-43ef-a205-868144f79dca.json"), b"{truncated").unwrap();
    let (spawn, _pipes) = support::pipes();
    let engine = Engine::new(
        EngineOptions {
            data_dir: s.data.path().into(),
            #[cfg(windows)]
            chat_dir: s.data.chat_path().into(),
            clis: BTreeMap::new(),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    let infos = engine.rewind_redos(CONVERSATION).unwrap();
    assert_eq!(infos.len(), 1);
    assert!(infos[0].recovery && infos[0].id.is_empty() && infos[0].message_id.is_empty());
    assert_eq!(infos[0].mode, Mode::Both);
    let preview = engine
        .rewind_redo_preview(RedoRequest { conversation_id: CONVERSATION.into(), id: String::new(), saved_versions: vec![] })
        .unwrap();
    assert!(preview.recovery && preview.running.is_empty());
    assert_eq!(engine.rewind_recover(CONVERSATION).unwrap_err(), "error.rewind.recovery");
    assert!(engine.rewind_redos(CONVERSATION).unwrap()[0].recovery, "a failed retry must leave recovery reachable");
    engine.shutdown();
}

#[test]
fn rewind_a_hundred_pinned_points_refuse_capture_instead_of_exceeding_the_folder_cap() {
    let mut s = Setup::new();
    for at in 1..=100 {
        s.capture(&format!("m{at}"), at);
    }
    assert_eq!(s.history.list(CONVERSATION).len(), 100);
    s.conversation.messages.push(Message::new("overflow".into(), Role::User, "keep working".into(), 101));
    let result = s.history.capture(&mut s.git, s.folder.path(), &s.conversation, "overflow", "overflow-turn", 101, false);
    assert!(result.is_err(), "fully pinned history must refuse another capture");
    assert_eq!(s.history.list(CONVERSATION).len(), 100);
    assert_eq!(s.conversation.messages.last().unwrap().text, "keep working", "history admission never discards the turn");
}

#[test]
fn rewind_fixture_receipts_are_unchanged_recorded_claude_and_codex_turns() {
    // Rewind reuses the checked-in chat recordings verbatim; no live probe or fabricated protocol receipt.
    for vendor in ["claude", "codex"] {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures").join(vendor);
        assert_eq!(std::fs::read(root.join("rewind.jsonl")).unwrap(), std::fs::read(root.join("chat.jsonl")).unwrap());
        let entries = support::fixture(vendor, "rewind");
        let mut state = faber_engine::codex::CodexTurnState::default();
        let events: Vec<_> = entries
            .iter()
            .filter(|entry| !entry.input)
            .flat_map(|entry| {
                if vendor == "claude" {
                    faber_engine::claude::parse_claude_message(&entry.msg)
                } else {
                    faber_engine::codex::parse_codex_notification(&entry.msg, &mut state)
                }
            })
            .collect();
        assert!(events.iter().any(|event| matches!(event, EngineEvent::Done { ok: true, .. })), "{vendor}: recorded completion");
        assert!(!support::text_of(&events).is_empty(), "{vendor}: saved continuation has an answer");
    }
}

#[tokio::test]
async fn rewind_recorded_turns_capture_before_cli_start_and_redo_never_spawns_or_spends() {
    use faber_engine::cli::Cli;
    use faber_engine::model::{Area, Status};
    use faber_engine::proc::Spawner;
    use faber_engine::router::Choice;
    use faber_engine::store::Store;
    use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
    use std::collections::BTreeMap;
    use std::time::Duration;

    for (name, vendor) in [("claude", Vendor::Claude), ("codex", Vendor::Codex)] {
        let s = Setup::new();
        s.write("file", b"before startup");
        s.names(&["file"]);
        let mut conversation = s.conversation.clone();
        conversation.area = Area::Chat;
        Store::new(s.data.path()).save(&conversation);
        let entries = support::fixture(name, "rewind");
        let prompt = entries
            .iter()
            .find_map(|entry| {
                if !entry.input {
                    return None;
                }
                if name == "claude" {
                    entry.msg["message"]["content"].as_str().map(String::from)
                } else if entry.msg["method"] == "turn/start" {
                    entry.msg["params"]["input"][0]["text"].as_str().map(String::from)
                } else {
                    None
                }
            })
            .unwrap();
        let fake = support::fake(entries);
        let inner = fake.spawn.clone();
        let file = s.folder.path().join("file");
        let spawn: Spawner = Arc::new(move |bin, args, cwd| {
            std::fs::write(&file, b"after startup").unwrap();
            inner(bin, args, cwd)
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine::new(
            EngineOptions {
                data_dir: s.data.path().into(),
                #[cfg(windows)]
                chat_dir: s.data.chat_path().into(),
                clis: BTreeMap::from([(vendor, Cli { command: "fake".into(), prefix: vec![] })]),
                spawn,
                windows: false,
                claude_env_billed: false,
                max_warm: 3,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(move |event| {
                let _ = tx.send(event);
            }),
        )
        .unwrap();
        let git = s.git.clone();
        engine.set_git(Arc::new(move || Some(Box::new(git.clone()))));
        engine.warm(WarmRequest {
            conversation_id: Some(CONVERSATION.into()),
            area: Area::Chat,
            project_id: None,
            choice: Choice { vendor: name.into(), model: None, effort: None },
        });
        assert_eq!(s.bytes("file"), b"before startup", "{name}: warm-up cannot write before capture");
        assert!(fake.args.lock().unwrap().is_empty(), "{name}: project warm-up waits for the message");
        let id = engine
            .send(SendRequest {
                conversation_id: Some(CONVERSATION.into()),
                area: Area::Chat,
                project_id: None,
                text: prompt,
                images: vec![],
                files: vec![],
                choice: Choice { vendor: name.into(), model: None, effort: None },
            })
            .unwrap();
        tokio::time::timeout(crate::support::DEADLINE, async {
            loop {
                if let AppEvent::Message { conversation_id, message } = rx.recv().await.unwrap()
                    && conversation_id == id
                    && message.role == Role::Assistant
                    && message.status == Some(Status::Done)
                {
                    break;
                }
            }
        })
        .await
        .expect("recorded turn finishes");
        let point = tokio::time::timeout(crate::support::DEADLINE, async {
            loop {
                if let Some(point) = engine.rewind_points(&id).unwrap().into_iter().find(|point| point.sealed) {
                    break point;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("writers drain and history seals");
        assert!(!point.coverage.unavailable && point.coverage.saved == 1, "{name}: capture succeeds");
        if vendor == Vendor::Claude {
            assert!(point.cursor.entry.is_some(), "the recorded native entry is retained");
        } else {
            assert!(point.cursor.turn.is_some(), "the recorded completed turn is retained");
        }
        let count = fake.args.lock().unwrap().len();
        let wires = fake.wire.len();
        let request =
            RestoreRequest { conversation_id: id.clone(), message_id: point.message_id, mode: Mode::Both, saved_versions: vec![] };
        assert!(engine.rewind_preview(request.clone()).unwrap().running.is_empty());
        let restored = engine.rewind_restore(request).unwrap();
        assert!(!restored.partial);
        assert_eq!(s.bytes("file"), b"before startup", "{name}: baseline precedes process startup");
        let infos = engine.rewind_redos(&id).unwrap();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, restored.id);
        let redo_request = RedoRequest { conversation_id: id.clone(), id: restored.id.clone(), saved_versions: vec![] };
        let preview = engine.rewind_redo_preview(redo_request).unwrap();
        assert_eq!(preview.replacements, 1);
        assert_eq!(preview.conflicts, 0);
        engine.rewind_recover(&id).unwrap();
        engine.rewind_redo(RedoRequest { conversation_id: id.clone(), id: restored.id, saved_versions: vec![] }).unwrap();
        assert_eq!(s.bytes("file"), b"after startup");
        assert_eq!(fake.args.lock().unwrap().len(), count, "redo starts no CLI");
        assert_eq!(fake.wire.len(), wires, "redo sends no turn");
        assert!(fake.mismatches.lock().unwrap().is_empty(), "{name}: recorded wire receipts match");
        assert!(engine.open(&id).unwrap().keep_going.is_none());
        engine.shutdown();
    }
}

#[tokio::test]
async fn rewind_overlap_refuses_restore_without_interrupting_another_conversations_background_command() {
    use faber_engine::cli::Cli;
    use faber_engine::model::{Area, Status};
    use faber_engine::router::Choice;
    use faber_engine::store::Store;
    use faber_engine::{Engine, EngineOptions, SendRequest};
    use std::collections::BTreeMap;
    use std::time::Duration;

    let s = Setup::new();
    let nested = s.folder.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let mut a = s.conversation.clone();
    a.area = Area::Chat;
    a.messages.push(Message::new("cut-a".into(), Role::User, "earlier".into(), 1));
    let mut b = a.clone();
    b.id = "e2c1b82e-98e8-4bca-9214-dd342896c10c".into();
    b.working_folder = Some(nested.to_string_lossy().into_owned());
    let store = Store::new(s.data.path());
    store.save(&a);
    store.save(&b);
    let (spawn, pipes) = support::pipes();
    let engine = Engine::new(
        EngineOptions {
            data_dir: s.data.path().into(),
            #[cfg(windows)]
            chat_dir: s.data.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    let git = s.git.clone();
    engine.set_git(Arc::new(move || Some(Box::new(git.clone()))));
    engine.open(&a.id).unwrap();
    engine
        .send(SendRequest {
            conversation_id: Some(b.id.clone()),
            area: Area::Chat,
            project_id: None,
            text: "work".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&pipes).await;
    let request =
        RestoreRequest { conversation_id: a.id.clone(), message_id: "cut-a".into(), mode: Mode::Conversation, saved_versions: vec![] };
    assert!(engine.rewind_preview(request.clone()).unwrap().running.iter().any(|run| run.conversation_id == b.id));
    assert_eq!(engine.rewind_restore(request.clone()).unwrap_err(), "error.rewind.running");
    assert!(!pipe.closed.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!pipe.lines.lock().unwrap().iter().any(|line| line["request"]["subtype"] == "interrupt"));

    // These protocol values are a scripted boundary test; the recorded turn receipts live in rewind.jsonl.
    pipe.say(json!({"type":"system","subtype":"init","session_id":"overlapping-writer","model":"claude-haiku-5-5"})).await;
    pipe.say(json!({"type":"system","subtype":"task_started","task_type":"local_bash","is_backgrounded":true,"run_id":"command"})).await;
    pipe.say(json!({"type":"result","subtype":"success","is_error":false,"usage":{}})).await;
    tokio::time::timeout(crate::support::DEADLINE, async {
        loop {
            if engine.open(&b.id).unwrap().messages.last().is_some_and(|message| message.status == Some(Status::Done)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let running = engine.rewind_preview(request.clone()).unwrap().running;
    assert!(running.iter().any(|run| run.conversation_id == b.id && run.writers.iter().any(|writer| writer.contains("command"))));
    assert_eq!(engine.rewind_restore(request.clone()).unwrap_err(), "error.rewind.running");
    assert!(!pipe.closed.load(std::sync::atomic::Ordering::SeqCst), "restore of A leaves B's owned pipe alive");
    assert!(!pipe.lines.lock().unwrap().iter().any(|line| line["request"]["subtype"] == "interrupt"));
    pipe.say(json!({"type":"system","subtype":"task_notification","run_id":"command","status":"completed"})).await;
    tokio::time::timeout(crate::support::DEADLINE, async {
        loop {
            if engine.rewind_preview(request.clone()).unwrap().running.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    engine.shutdown();
}

#[test]
fn rewind_restart_at_every_swap_barrier_preserves_versions_and_redo() {
    use faber_winproc::rewind::{Root, file_identity, swap_paths};
    use sha2::{Digest, Sha256};
    let mut crashes: Vec<_> = (0..=4).map(|stage| (stage, None, 0)).collect();
    crashes.extend([(0, Some(b"".as_slice()), 0), (0, Some(b"unexpected temporary bytes".as_slice()), 0)]);
    #[cfg(unix)]
    crashes.extend([(4, None, 1), (4, None, 2)]);
    for (stage, unrecorded_bytes, quarantine) in crashes {
        let mut s = Setup::new();
        s.write("file", b"before");
        s.names(&["file"]);
        s.capture("m1", 1);
        s.write("file", b"after");
        s.seal(2);
        let mut durable = s.conversation.clone();
        let restored = s.history.restore(&s.request("m1", Mode::Both), &mut s.conversation, 3).unwrap();
        let folder =
            std::fs::read_dir(s.data.path().join("rewind")).unwrap().map(|entry| entry.unwrap().path()).find(|path| path.is_dir()).unwrap();
        let journal_path = folder.join("journals").join(format!("{}.json", restored.id));
        let mut journal: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
        let temporary = format!(".faber-rewind-{}-0", restored.id);
        let (prepared, aside) = swap_paths("file", &temporary);
        s.write("file", b"after");
        #[cfg(unix)]
        faber_winproc::rewind::private_dir(&s.folder.path().join(&temporary)).unwrap();
        s.write(&prepared, unrecorded_bytes.unwrap_or(b"before"));
        let root = Root::open(s.folder.path()).unwrap();
        let temp_identity = unrecorded_bytes.is_none().then(|| file_identity(&root.open_file(&prepared).unwrap()).unwrap());
        let old_identity = file_identity(&root.open_file("file").unwrap()).unwrap();
        if stage >= 2 {
            std::fs::rename(s.folder.path().join("file"), s.folder.path().join(&aside)).unwrap();
        }
        if stage == 4 {
            std::fs::rename(s.folder.path().join(&prepared), s.folder.path().join("file")).unwrap();
        }
        if quarantine != 0 {
            let private = s.folder.path().join(&temporary).join(".faber-rewind-cleanup-91-2");
            faber_winproc::rewind::private_dir(&private).unwrap();
            if quarantine == 1 {
                std::fs::rename(s.folder.path().join(&aside), private.join("retained")).unwrap();
            } else {
                std::fs::remove_file(s.folder.path().join(&aside)).unwrap();
            }
        }
        journal["files_done"] = json!(false);
        journal["conversation_done"] = json!(false);
        let operation = &mut journal["operations"][0];
        operation["applied"] = json!(false);
        operation["after_identity"] = json!(null);
        operation["temporary"] = json!({ "path": prepared, "identity": temp_identity });
        operation["displaced"] = if stage >= 1 { json!({ "path": aside, "identity": old_identity }) } else { json!(null) };
        operation["installing"] = json!(stage >= 3);
        std::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();
        let mut restarted = History::open(s.data.path(), 4).unwrap();
        restarted.recover_conversation(&mut durable).unwrap();
        restarted.recover_conversation(&mut durable).unwrap();
        let recovered: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
        if let Some(unexpected) = unrecorded_bytes.filter(|bytes| !bytes.is_empty()) {
            let mut hash = Sha256::new();
            hash.update(format!("blob {}\0", unexpected.len()).as_bytes());
            hash.update(unexpected);
            let expected_oid: String = hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
            assert!(
                recovered["retained"].as_array().unwrap().iter().any(|entry| entry[0] == prepared && entry[1]["oid"] == expected_oid),
                "an unrecorded foreign temporary is saved before deletion"
            );
        }
        if quarantine == 1 {
            let mut hash = Sha256::new();
            hash.update(b"blob 5\0after");
            let expected_oid: String = hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
            assert!(
                recovered["retained"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|entry| entry[0].as_str().is_some_and(|path| path.ends_with("/retained")) && entry[1]["oid"] == expected_oid),
                "quarantined bytes are preserved after restart"
            );
        }
        assert_eq!(s.bytes("file"), if stage == 4 { b"before".as_slice() } else { b"after".as_slice() }, "barrier {stage}");
        assert!(!s.folder.path().join(&temporary).exists(), "prepared file cleaned after barrier {stage}");
        assert!(!s.folder.path().join(&aside).exists(), "displaced file preserved in history after barrier {stage}");
        restarted.acknowledge(&restored.id).unwrap();
        restarted
            .redo(
                &RedoRequest { conversation_id: CONVERSATION.into(), id: restored.id, saved_versions: vec!["file".into()] },
                &mut durable,
                5,
            )
            .unwrap();
        assert_eq!(s.bytes("file"), b"after", "redo after barrier {stage}");
    }
}

#[test]
fn rewind_recovery_saves_an_unexpected_displaced_version_before_reversing() {
    use faber_winproc::rewind::{Root, file_identity, swap_paths};
    for deleting in [false, true] {
        for external in [b"external version B".to_vec(), vec![b'B'; FILE_LIMIT as usize + 1]] {
            let mut s = Setup::new();
            s.write("file", b"before");
            s.names(&["file"]);
            s.capture("m1", 1);
            s.write("file", b"after");
            s.seal(2);
            let restored = s.history.restore(&s.request("m1", Mode::Files), &mut s.conversation, 3).unwrap();
            let folder = std::fs::read_dir(s.data.path().join("rewind"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.is_dir())
                .unwrap();
            let journal_path = folder.join("journals").join(format!("{}.json", restored.id));
            let mut journal: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
            let (_, aside) = swap_paths("file", &format!(".faber-rewind-{}-0", restored.id));
            let root = Root::open(s.folder.path()).unwrap();
            let validated_identity = file_identity(&root.open_file("file").unwrap()).unwrap();
            std::fs::remove_file(s.folder.path().join("file")).unwrap();
            #[cfg(unix)]
            faber_winproc::rewind::private_dir(s.folder.path().join(&aside).parent().unwrap()).unwrap();
            s.write(&aside, &external);
            journal["files_done"] = json!(false);
            journal["conversation_done"] = json!(false);
            journal["operations"][0]["displaced"] = json!({ "path": aside, "identity": validated_identity });
            journal["operations"][0]["installing"] = json!(false);
            if deleting {
                journal["operations"][0]["after"] = json!(null);
            }
            std::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();
            let mut restarted = History::open(s.data.path(), 4).unwrap();
            restarted.recover_conversation(&mut s.conversation).unwrap();
            assert_eq!(s.bytes("file"), external);
            assert!(!s.folder.path().join(&aside).exists());
            let recovered: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
            assert_eq!(recovered["operations"][0]["before"], journal["operations"][0]["before"], "the prior version stays saved");
            assert_eq!(recovered["operations"][0]["after"], journal["operations"][0]["after"], "the desired version stays saved");
            if external.len() as u64 > FILE_LIMIT {
                assert_eq!(recovered["partial"], true, "an uncaptured version reports partial recovery");
                // The restore itself already retained the displaced name; recovery adds nothing new.
                assert_eq!(recovered["retained"], journal["retained"], "oversize bytes are never read into history");
            } else {
                let before = journal["retained"].as_array().unwrap().len();
                assert_eq!(recovered["retained"].as_array().unwrap().len(), before + 1, "the external version is saved");
                assert_eq!(recovered["retained"][before][0], aside);
            }
            restarted.acknowledge(&restored.id).unwrap();
            assert!(!restarted.recovery_for(CONVERSATION), "returning the file releases recovery admission");
            let preview = restarted
                .redo_preview(
                    &RedoRequest { conversation_id: CONVERSATION.into(), id: restored.id, saved_versions: vec![] },
                    &s.conversation,
                    5,
                )
                .unwrap();
            assert_eq!(preview.coverage.incomplete, external.len() as u64 > FILE_LIMIT);
        }
    }
}

#[test]
fn rewind_recovery_returns_a_readable_displaced_version_when_history_is_full() {
    use faber_winproc::rewind::{Root, file_identity, private_create, swap_paths};
    #[cfg(windows)]
    let locations = [false];
    #[cfg(unix)]
    let locations = [false, true];
    for quarantined in locations {
        let mut s = Setup::new();
        s.write("file", b"before");
        s.names(&["file"]);
        s.capture("m1", 1);
        s.write("file", b"after");
        s.seal(2);
        let restored = s.history.restore(&s.request("m1", Mode::Files), &mut s.conversation, 3).unwrap();
        let folder =
            std::fs::read_dir(s.data.path().join("rewind")).unwrap().map(|entry| entry.unwrap().path()).find(|path| path.is_dir()).unwrap();
        let journal_path = folder.join("journals").join(format!("{}.json", restored.id));
        let mut journal: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
        let temporary = format!(".faber-rewind-{}-0", restored.id);
        let (_, aside) = swap_paths("file", &temporary);
        let root = Root::open(s.folder.path()).unwrap();
        let identity = file_identity(&root.open_file("file").unwrap()).unwrap();
        std::fs::remove_file(s.folder.path().join("file")).unwrap();
        #[cfg(unix)]
        faber_winproc::rewind::private_dir(s.folder.path().join(&aside).parent().unwrap()).unwrap();
        let external = vec![b'B'; 256 * 1024];
        s.write(&aside, &external);
        let retained = if quarantined {
            let private = s.folder.path().join(&temporary).join(".faber-rewind-cleanup-91-2");
            faber_winproc::rewind::private_dir(&private).unwrap();
            let retained = format!("{temporary}/.faber-rewind-cleanup-91-2/retained");
            std::fs::rename(s.folder.path().join(&aside), s.folder.path().join(&retained)).unwrap();
            retained
        } else {
            aside.clone()
        };
        journal["files_done"] = json!(false);
        journal["conversation_done"] = json!(false);
        journal["operations"][0]["displaced"] = json!({ "path": aside, "identity": identity });
        journal["operations"][0]["installing"] = json!(false);
        std::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();
        // Counted metadata leaves room for journal publication, but not the readable external version's blob.
        private_create(&folder.join("refs/heads/full-history")).unwrap().set_len(faber_engine::rewind::FOLDER_LIMIT - 192 * 1024).unwrap();
        let mut restarted = History::open(s.data.path(), 4).unwrap();
        restarted.recover_conversation(&mut s.conversation).unwrap();
        restarted.recover_conversation(&mut s.conversation).unwrap();
        assert_eq!(s.bytes("file"), external, "unsaved external bytes return without reading them again");
        assert!(!s.folder.path().join(&retained).exists());
        let recovered: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
        assert_eq!(recovered["partial"], true, "unavailable history is reported as partial recovery");
        assert_eq!(recovered["retained"], journal["retained"], "all previously retained versions stay saved");
        assert_eq!(recovered["operations"][0]["before"], journal["operations"][0]["before"]);
        assert_eq!(recovered["operations"][0]["after"], journal["operations"][0]["after"]);
        restarted.acknowledge(&restored.id).unwrap();
        assert!(!restarted.recovery_for(CONVERSATION), "a full history cannot trap the returned file in recovery");
        let preview = restarted
            .redo_preview(
                &RedoRequest { conversation_id: CONVERSATION.into(), id: restored.id, saved_versions: vec![] },
                &s.conversation,
                5,
            )
            .unwrap();
        assert!(preview.coverage.incomplete);
    }
}

#[tokio::test]
async fn rewind_stopped_checks_seal_only_after_confirmed_completion_and_release_overlapping_folders() {
    use faber_engine::cli::Cli;
    use faber_engine::model::Area;
    use faber_engine::router::Choice;
    use faber_engine::store::Store;
    use faber_engine::verify::{Runner, Spec, Started, Verifier};
    use faber_engine::{Engine, EngineOptions, SendRequest};
    use std::collections::BTreeMap;
    use std::time::Duration;

    #[derive(Clone)]
    struct CheckGit(Git);
    impl Executor for CheckGit {
        fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
            if call.args.iter().any(|a| a == "ls-files") && call.args.last().is_some_and(|a| a == "--") {
                self.0.execute(call)
            } else {
                Ok(Output { success: false, stdout: vec![] })
            }
        }
    }
    struct DrainingCheck {
        started: tokio::sync::Notify,
        confirmed: tokio::sync::watch::Receiver<bool>,
    }
    impl Runner for DrainingCheck {
        fn start(&self, _: &Spec) -> std::io::Result<Started> {
            self.started.notify_one();
            Ok(Started {
                stdout: Box::new(tokio::io::empty()),
                stderr: Box::new(tokio::io::empty()),
                exit: Box::pin(std::future::pending()),
                confirmed: Some(self.confirmed.clone()),
                throttle: None,
                peak: None,
            })
        }
    }
    let s = Setup::new();
    s.write("file", b"before checks");
    s.write("AGENTS.md", b"## Proof commands\n\n```bash\nnode check.js\n```\n");
    s.names(&["file"]);
    let (spawn, pipes) = support::pipes();
    let engine = Engine::new(
        EngineOptions {
            data_dir: s.data.path().into(),
            #[cfg(windows)]
            chat_dir: s.data.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    let mut project = engine.add_project(&s.folder.path().to_string_lossy()).unwrap();
    let before = project.clone();
    project.separate_copies = false;
    engine.update_project(&before, &project).unwrap();
    let mut conversation = s.conversation.clone();
    conversation.project_id = Some(project.id);
    Store::new(s.data.path()).save(&conversation);
    let nested = s.folder.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let mut other = conversation.clone();
    other.id = "e2c1b82e-98e8-4bca-9214-dd342896c10c".into();
    other.area = Area::Chat;
    other.working_folder = Some(nested.to_string_lossy().into_owned());
    other.messages.push(Message::new("other-cut".into(), Role::User, "Earlier".into(), 1));
    Store::new(s.data.path()).save(&other);
    engine.open(&other.id).unwrap();
    let git = CheckGit(s.git.clone());
    engine.set_git(Arc::new(move || Some(Box::new(git.clone()))));
    let (complete, confirmation) = tokio::sync::watch::channel(false);
    let checks = Arc::new(DrainingCheck { started: tokio::sync::Notify::new(), confirmed: confirmation });
    engine.set_verifier(Arc::new(Verifier::new(checks.clone())));
    engine
        .send(SendRequest {
            conversation_id: Some(CONVERSATION.into()),
            area: Area::Project,
            project_id: conversation.project_id,
            text: "change file".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&pipes).await;
    pipe.say(json!({"type":"system","subtype":"init","session_id":"checks","model":"haiku"})).await;
    s.write("file", b"main turn changed it");
    pipe.say(
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"edit","name":"Edit","input":{"file_path":"file"}}]}}),
    )
    .await;
    pipe.say(json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"edit","content":"ok","is_error":false}]}}))
        .await;
    pipe.say(json!({"type":"result","subtype":"success","is_error":false,"usage":{}})).await;
    tokio::time::timeout(crate::support::DEADLINE, checks.started.notified()).await.expect("the check starts");
    let point = engine.rewind_points(CONVERSATION).unwrap().pop().unwrap();
    assert!(!point.coverage.unavailable, "the test needs a real baseline: {point:?}; Git calls: {:?}", s.git.0.lock().unwrap().calls);
    assert!(!point.sealed);
    let request =
        RestoreRequest { conversation_id: CONVERSATION.into(), message_id: point.message_id, mode: Mode::Files, saved_versions: vec![] };
    let overlapping =
        RestoreRequest { conversation_id: other.id, message_id: "other-cut".into(), mode: Mode::Conversation, saved_versions: vec![] };
    engine.interrupt(CONVERSATION);
    assert!(!engine.rewind_preview(request.clone()).unwrap().running.is_empty());
    assert_eq!(engine.rewind_restore(overlapping.clone()).unwrap_err(), "error.rewind.running");
    s.write("file", b"late check write");
    assert!(!engine.rewind_points(CONVERSATION).unwrap()[0].sealed, "an aborted future is not process completion");
    complete.send(true).unwrap();
    tokio::time::timeout(crate::support::DEADLINE, async {
        loop {
            if engine.rewind_points(CONVERSATION).unwrap()[0].sealed
                && engine.rewind_preview(overlapping.clone()).unwrap().running.is_empty()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("confirmed stopped checks seal their late writes and release overlapping folders");
    let restored = engine.rewind_restore(request).unwrap();
    assert!(!restored.partial);
    assert_eq!(s.bytes("file"), b"before checks");
    engine.rewind_redo(RedoRequest { conversation_id: CONVERSATION.into(), id: restored.id, saved_versions: vec![] }).unwrap();
    assert_eq!(s.bytes("file"), b"late check write");
    engine.shutdown();
}

#[test]
fn rewind_failed_journal_update_keeps_the_old_journal_and_its_exclusive_blob() {
    use faber_winproc::rewind::Root;
    let mut s = Setup::new();
    s.write("file", b"before");
    s.names(&["file"]);
    s.capture("m1", 1);
    s.write("file", b"after");
    s.seal(2);
    s.write("file", b"version referenced only by the journal");
    let mut request = s.request("m1", Mode::Files);
    request.saved_versions.push("file".into());
    let restored = s.history.restore(&request, &mut s.conversation, 3).unwrap();
    let folder =
        std::fs::read_dir(s.data.path().join("rewind")).unwrap().map(|entry| entry.unwrap().path()).find(|path| path.is_dir()).unwrap();
    let name = format!("journals/{}.json", restored.id);
    let history_root = Root::open(&folder).unwrap();
    #[cfg(windows)]
    let lock = history_root.open_file(&name).unwrap();
    #[cfg(unix)]
    std::fs::hard_link(folder.join(&name), folder.join("locked-journal")).unwrap();
    let durable = std::fs::read(folder.join(&name)).unwrap();
    let request = RedoRequest { conversation_id: CONVERSATION.into(), id: restored.id.clone(), saved_versions: vec![] };
    assert!(s.history.redo(&request, &mut s.conversation, 4).is_err());
    assert_eq!(std::fs::read(folder.join(&name)).unwrap(), durable);
    #[cfg(windows)]
    drop(lock);
    #[cfg(unix)]
    std::fs::remove_file(folder.join("locked-journal")).unwrap();
    drop(history_root);
    let mut restarted = History::open(s.data.path(), 5).unwrap();
    restarted.recover_conversation(&mut s.conversation).unwrap();
    restarted.acknowledge(&restored.id).unwrap();
    restarted.redo(&request, &mut s.conversation, 6).unwrap();
    assert_eq!(s.bytes("file"), b"version referenced only by the journal");
}

#[cfg(unix)]
#[test]
fn rewind_restart_cleans_a_deletion_stage_created_before_displacement() {
    let mut s = Setup::new();
    s.capture("m1", 1);
    s.write("created", b"after");
    s.names(&["created"]);
    s.seal(2);
    let restored = s.history.restore(&s.request("m1", Mode::Files), &mut s.conversation, 3).unwrap();
    let folder =
        std::fs::read_dir(s.data.path().join("rewind")).unwrap().map(|entry| entry.unwrap().path()).find(|path| path.is_dir()).unwrap();
    let journal_path = folder.join("journals").join(format!("{}.json", restored.id));
    let mut journal: serde_json::Value = serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
    let staging = s.folder.path().join(format!(".faber-rewind-{}-0", restored.id));
    faber_winproc::rewind::private_dir(&staging).unwrap();
    s.write("created", b"after");
    journal["files_done"] = json!(false);
    journal["conversation_done"] = json!(false);
    journal["operations"][0]["temporary"] = json!(null);
    journal["operations"][0]["displaced"] = json!(null);
    journal["operations"][0]["installing"] = json!(false);
    std::fs::write(&journal_path, serde_json::to_vec(&journal).unwrap()).unwrap();
    let mut restarted = History::open(s.data.path(), 4).unwrap();
    restarted.recover_conversation(&mut s.conversation).unwrap();
    assert_eq!(s.bytes("created"), b"after");
    assert!(!staging.exists());
}
