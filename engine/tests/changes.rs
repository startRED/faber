//! Conversation diffs use a scripted git runner, never a process or a real repository.
use crate::support;
use faber_engine::changes::{self, PATCH_BYTES};
use faber_engine::model::{Area, Conversation};
use faber_engine::project::copy::WorkCopy;
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HEAD: &str = "1111111111111111111111111111111111111111";
const FROM: &str = "2222222222222222222222222222222222222222";

/// Runs just before git is asked for a patch, to change the folder at that moment.
type PatchHook = Box<dyn Fn(&Invocation) + Send>;

#[derive(Default)]
struct Repo {
    stats: Vec<u8>,
    others: Vec<u8>,
    patch: Vec<u8>,
    blob: Vec<u8>,
    indexed_blob: Vec<u8>,
    autocrlf: String,
    text_attr: String,
    eol_attr: String,
    tracked: Vec<u8>,
    base_names: Vec<u8>,
    no_base: bool,
    base_sizes: BTreeMap<String, u64>,
    calls: Vec<(std::path::PathBuf, Vec<String>)>,
    look: Option<Box<dyn Fn() + Send>>,
    after_stats: Option<Box<dyn Fn() + Send>>,
    after_tree: Option<Box<dyn Fn() + Send>>,
    before_patch: Option<PatchHook>,
    fail_blob: bool,
    fail_patch: bool,
    fail_stats_at: Option<usize>,
    stats_calls: usize,
    assert_copy_root: bool,
    index_path: Option<std::path::PathBuf>,
    debug: Vec<u8>,
}

#[derive(Clone, Default)]
struct Git(Arc<Mutex<Repo>>);

impl Executor for Git {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        if self.0.lock().unwrap().assert_copy_root {
            let root = call.cwd.to_string_lossy().replace("\\\\?\\", "");
            assert!(
                call.args.contains(&format!("core.worktree={root}")),
                "git must use the folder checked by the reader, overriding repository config"
            );
        }
        let mut args = call.args.as_slice();
        while args.first().is_some_and(|a| a == "-c") {
            args = &args[2..];
        }
        let mut repo = self.0.lock().unwrap();
        repo.calls.push((call.cwd.clone(), args.to_vec()));
        if let Some(look) = repo.look.take() {
            drop(repo);
            look();
            repo = self.0.lock().unwrap();
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let stdout = match args.as_slice() {
            ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"] => format!("{HEAD}\n").into_bytes(),
            ["merge-base", "HEAD", "refs/heads/main"] if !repo.no_base => format!("{FROM}\n").into_bytes(),
            ["merge-base", ..] => return Ok(Output { success: false, stdout: vec![] }),
            ["rev-parse", "--git-path", "index"] => {
                repo.index_path.as_ref().map(|p| p.to_string_lossy().as_bytes().to_vec()).unwrap_or_default()
            }
            ["config", "--default", "false", "--get", "core.autocrlf"] => {
                if repo.autocrlf.is_empty() {
                    b"false\n".to_vec()
                } else {
                    repo.autocrlf.as_bytes().to_vec()
                }
            }
            ["check-attr", "-z", "text", "eol", "--", path] => {
                let text = if repo.text_attr.is_empty() { "unspecified" } else { &repo.text_attr };
                let eol = if repo.eol_attr.is_empty() { "unspecified" } else { &repo.eol_attr };
                format!("{path}\0text\0{text}\0{path}\0eol\0{eol}\0").into_bytes()
            }
            ["ls-files", "--stage", "--debug", "-z", "--full-name", "--", _] => repo.debug.clone(),
            ["ls-files", "--stage", "-z", "--full-name", tail @ ..] => {
                let names = if repo.tracked.is_empty() {
                    repo.stats
                        .split(|b| *b == 0)
                        .filter_map(|record| record.splitn(3, |b| *b == b'\t').nth(2))
                        .flat_map(|p| p.iter().copied().chain([0]))
                        .collect::<Vec<_>>()
                } else {
                    repo.tracked.clone()
                };
                let paths = tail.strip_prefix(&["--"]).unwrap_or(&[]);
                names
                    .split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .filter(|p| {
                        paths.is_empty() || paths.iter().any(|want| p == &want.as_bytes() || p.starts_with(format!("{want}/").as_bytes()))
                    })
                    .flat_map(|p| format!("100644 {HEAD} 0\t{}\0", String::from_utf8_lossy(p)).into_bytes())
                    .collect()
            }
            ["ls-tree", "-r", "-l", "-z", _, tail @ ..] => {
                let paths = tail.strip_prefix(&["--"]).unwrap_or(&[]);
                repo.base_names
                    .split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .filter(|p| {
                        paths.is_empty() || paths.iter().any(|want| p == &want.as_bytes() || p.starts_with(format!("{want}/").as_bytes()))
                    })
                    .flat_map(|p| {
                        let name = String::from_utf8_lossy(p);
                        format!("100644 blob {HEAD} {}\t{name}\0", repo.base_sizes.get(name.as_ref()).copied().unwrap_or(0)).into_bytes()
                    })
                    .collect()
            }
            ["diff", "--raw", "--numstat", "-z", "--no-renames", "--no-textconv", "--no-ext-diff", from, "--", paths @ ..]
                if [FROM, HEAD].contains(from) =>
            {
                assert!(!paths.is_empty(), "a diff must never read the whole repository");
                repo.stats_calls += 1;
                if repo.fail_stats_at == Some(repo.stats_calls) {
                    return Err(GitError::Command);
                }
                repo.stats
                    .split(|b| *b == 0)
                    .filter(|record| {
                        record
                            .splitn(3, |b| *b == b'\t')
                            .nth(2)
                            .and_then(|path| std::str::from_utf8(path).ok())
                            .is_some_and(|path| paths.contains(&path))
                    })
                    .flat_map(|record| record.iter().copied().chain([0]))
                    .collect()
            }
            ["ls-files", "--others", "--exclude-standard", "-z", "--full-name", tail @ ..] => {
                let paths = tail.strip_prefix(&["--"]).unwrap_or(&[]);
                repo.others
                    .split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .filter(|p| paths.is_empty() || paths.iter().any(|want| p == &want.as_bytes()))
                    .flat_map(|p| p.iter().copied().chain([0]))
                    .collect()
            }
            ["cat-file", "blob", object] => {
                assert!(object.starts_with(&format!("{FROM}:")) || object.starts_with(&format!("{HEAD}:")) || *object == HEAD);
                if repo.fail_blob {
                    return Err(GitError::Command);
                }
                if *object == HEAD { repo.indexed_blob.clone() } else { repo.blob.clone() }
            }
            ["diff", "--no-index", "--no-ext-diff", "--no-textconv", "--no-color", "-U3", "--", "before", "after"] => {
                assert!(call.deadline.is_some(), "snapshot comparisons share the request deadline");
                assert!(call.cwd.file_name().unwrap().to_string_lossy().starts_with("faber-changes-"));
                assert!(call.cwd.join("before").is_file() && call.cwd.join("after").is_file());
                if let Some(check) = repo.before_patch.take() {
                    check(call);
                }
                if repo.fail_patch {
                    return Err(GitError::Command);
                }
                repo.patch.clone()
            }
            other => panic!("unexpected git invocation: {other:?}"),
        };
        let after = if args.contains(&"--numstat") {
            repo.after_stats.take()
        } else if args.first() == Some(&"ls-tree") && args.contains(&"--") {
            repo.after_tree.take()
        } else {
            None
        };
        drop(repo);
        if let Some(after) = after {
            after();
        }
        Ok(Output { success: true, stdout })
    }
}

fn copy(root: &Path) -> WorkCopy {
    WorkCopy {
        path: root.to_string_lossy().into_owned(),
        root: "person".into(),
        sub: "app".into(),
        branch: "faber/change".into(),
        base: Some("refs/heads/main".into()),
    }
}

#[test]
fn untracked_files_are_all_added_and_patch_requests_must_be_in_the_list() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("new file.txt"), "one\ntwo").unwrap();
    std::fs::write(root.path().join("image.bin"), b"\0data").unwrap();
    let mut git = Git::default();
    git.0.lock().unwrap().others = b"new file.txt\0image.bin\0../outside\0.codex/auth.json\0".to_vec();
    let copy = copy(root.path());
    let files = changes::list(&mut git, &copy);
    assert_eq!(files.len(), 2);
    assert!(files[0].binary);
    assert!(!files[0].counts_known);
    assert_eq!((files[1].path.as_str(), files[1].added, files[1].removed), ("new file.txt", 2, 0));
    let patch = changes::patch(&mut git, &copy, "new file.txt");
    assert!(patch.text.contains("+one\n+two\n"));
    assert!(changes::patch(&mut git, &copy, "missing.txt").text.is_empty());
    assert!(
        !git.0
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|(_, args)| args.first().is_some_and(|a| a == "diff") && args.last().is_some_and(|p| p == "missing.txt"))
    );
}

#[test]
fn tracked_patches_are_lazy_capped_and_use_repository_paths_even_for_a_subfolder_project() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("image.png"), b"\0binary").unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.stats = b"2\t1\tapp/with spaces.rs\0-\t-\timage.png\0".to_vec();
        repo.patch = b"@@ -1 +1,2 @@\n-old\n+new\n+second\n".to_vec();
    }
    let copy = copy(root.path());
    let files = changes::list(&mut git, &copy);
    assert_eq!((files[0].added, files[0].removed), (2, 1));
    assert!(!git.0.lock().unwrap().calls.iter().any(|(_, args)| args.iter().any(|s| s == "-U3")), "listing reads no patches");
    assert!(changes::patch(&mut git, &copy, "app/with spaces.rs").text.contains("-old\n+new"));
    assert!(changes::patch(&mut git, &copy, "image.png").binary);
    git.0.lock().unwrap().patch = vec![b'x'; PATCH_BYTES + 1];
    let large = changes::patch(&mut git, &copy, "app/with spaces.rs");
    assert!(large.too_large && large.text.is_empty());
    git.0.lock().unwrap().patch = b"+line\n".repeat(4_001);
    assert!(changes::patch(&mut git, &copy, "app/with spaces.rs").too_large);
    assert!(git.0.lock().unwrap().calls.iter().all(|(cwd, args)| cwd == root.path() || args.contains(&"--no-index".into())));
    assert!(git.0.lock().unwrap().calls.iter().filter(|(_, args)| args.contains(&"--no-index".into())).all(|(cwd, _)| !cwd.exists()));
}

#[test]
fn new_files_growing_past_the_cap_keep_no_partial_patch() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("large.txt"), vec![b'x'; PATCH_BYTES + 10]).unwrap();
    let mut git = Git::default();
    git.0.lock().unwrap().others = b"large.txt\0".to_vec();
    assert!(!changes::list(&mut git, &copy(root.path()))[0].counts_known);
    let patch = changes::patch(&mut git, &copy(root.path()), "large.txt");
    assert!(patch.too_large && patch.text.is_empty());
}

#[test]
fn a_new_text_file_cut_in_the_middle_of_a_character_is_large_not_binary() {
    let root = tempfile::tempdir().unwrap();
    let text = format!("{}éx", "a".repeat(PATCH_BYTES));
    std::fs::write(root.path().join("large.txt"), text).unwrap();
    let mut git = Git::default();
    git.0.lock().unwrap().others = b"large.txt\0".to_vec();
    let files = changes::list(&mut git, &copy(root.path()));
    assert!(!files[0].binary && !files[0].counts_known);
    let patch = changes::patch(&mut git, &copy(root.path()), "large.txt");
    assert!(patch.too_large && !patch.binary);
}

#[test]
fn exhausting_the_aggregate_count_budget_keeps_the_remaining_files_with_unknown_counts() {
    let root = tempfile::tempdir().unwrap();
    let mut others = vec![];
    let content = vec![b'x'; PATCH_BYTES];
    for index in 0..11 {
        let name = format!("{index:02}.txt");
        std::fs::write(root.path().join(&name), &content).unwrap();
        others.extend_from_slice(name.as_bytes());
        others.push(0);
    }
    std::fs::write(root.path().join("last.txt"), "small\n").unwrap();
    others.extend_from_slice(b"last.txt\0");
    let mut git = Git::default();
    git.0.lock().unwrap().others = others;
    let files = changes::list(&mut git, &copy(root.path()));
    assert_eq!(files.len(), 12, "the budget limits counting, never the list of changed paths");
    assert!(files[..10].iter().all(|file| file.counts_known && file.added == 1));
    assert!(files[10..].iter().all(|file| !file.counts_known && !file.binary));
    assert_eq!(files[11].path, "last.txt");
    assert!(!files[11].too_large, "an exhausted counting budget does not make a small file too large");
    assert!(changes::patch(&mut git, &copy(root.path()), "last.txt").text.contains("+small"));
}

#[test]
fn a_copy_without_any_resolvable_base_falls_back_to_head() {
    let root = tempfile::tempdir().unwrap();
    let mut git = Git::default();
    git.0.lock().unwrap().no_base = true;
    git.0.lock().unwrap().stats = b"1\t0\tchanged.txt\0".to_vec();
    assert_eq!(changes::list(&mut git, &copy(root.path())).len(), 1);
    assert!(
        git.0.lock().unwrap().calls.iter().any(|(_, args)| args.first().is_some_and(|s| s == "diff") && args.iter().any(|s| s == HEAD))
    );
}

#[test]
fn protected_tracked_paths_are_removed_before_git_can_read_any_contents() {
    let root = tempfile::tempdir().unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.tracked = b"safe.txt\0.codex/auth.json\0.env\0".to_vec();
        repo.base_names = b"deleted.txt\0.claude/.credentials.json\0".to_vec();
        repo.stats = b"1\t0\tsafe.txt\x000\t2\tdeleted.txt\x0020\t10\t.codex/auth.json\0".to_vec();
    }
    let files = changes::list(&mut git, &copy(root.path()));
    assert_eq!(files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["deleted.txt", "safe.txt"]);
    let repo = git.0.lock().unwrap();
    let diffs: Vec<_> = repo.calls.iter().filter(|(_, args)| args.first().is_some_and(|arg| arg == "diff")).collect();
    assert!(diffs.iter().all(|(_, args)| args[9..].iter().all(|p| ["deleted.txt", "safe.txt"].contains(&p.as_str()))));
}

#[test]
fn explicit_tracked_paths_are_split_below_the_windows_argument_limit() {
    let root = tempfile::tempdir().unwrap();
    let mut git = Git::default();
    for index in 0..200 {
        let path = format!("file{index:03}-{}.txt", "a".repeat(100));
        git.0.lock().unwrap().stats.extend_from_slice(format!("1\t0\t{path}\0").as_bytes());
    }
    assert_eq!(changes::list(&mut git, &copy(root.path())).len(), 200);
    let repo = git.0.lock().unwrap();
    let diffs: Vec<_> = repo.calls.iter().filter(|(_, args)| args.first().is_some_and(|arg| arg == "diff")).collect();
    assert!(diffs.len() > 1);
    assert!(diffs.iter().all(|(_, args)| args[9..].iter().map(|path| 2 * path.len() + 4).sum::<usize>() <= 12 * 1024));
}

#[cfg(unix)]
#[test]
fn untracked_links_never_read_their_targets() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "private").unwrap();
    symlink(outside.path(), root.path().join("escape")).unwrap();
    symlink(outside.path().join("secret"), root.path().join("link")).unwrap();
    let mut git = Git::default();
    git.0.lock().unwrap().others = b"escape/secret\0link\0".to_vec();
    git.0.lock().unwrap().stats = b"1\t0\tescape/secret\0".to_vec();
    assert!(changes::list(&mut git, &copy(root.path())).is_empty());
}

#[cfg(windows)]
#[test]
fn untracked_junctions_never_read_their_targets() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "private").unwrap();
    junction::create(outside.path(), root.path().join("escape")).unwrap();
    let mut git = Git::default();
    git.0.lock().unwrap().others = b"escape/secret\0".to_vec();
    git.0.lock().unwrap().stats = b"1\t0\tescape/secret\0".to_vec();
    assert!(changes::list(&mut git, &copy(root.path())).is_empty());
    junction::delete(root.path().join("escape")).unwrap();
}

fn saved(dir: &Path, copy: Option<WorkCopy>) {
    let conv: Conversation = serde_json::from_value(json!({
        "id":"conversation", "title":"Change two files", "area":"project", "copy": copy,
        "createdAt":1,"updatedAt":1,"messages":[]
    }))
    .unwrap();
    Store::new(dir).save(&conv);
}

#[tokio::test]
async fn a_conversation_without_a_copy_has_no_changes_and_git_runs_off_the_engine_lock() {
    let data = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    saved(data.path(), None);
    let engine = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            clis: BTreeMap::new(),
            spawn: support::fake(vec![]).spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    let git = Git::default();
    let runner = git.clone();
    engine.set_git(Arc::new(move || Some(Box::new(runner.clone()))));
    assert!(engine.changes("conversation").is_empty());
    let page = engine.changes_page("conversation", None);
    assert!(page.files.is_empty() && !page.incomplete && page.next.is_none());
    assert!(engine.change_patch("conversation", "a.txt").text.is_empty());
    assert!(git.0.lock().unwrap().calls.is_empty());
    // A second engine loads a conversation with its copy, as after a restart.
    engine.shutdown();
    saved(data.path(), Some(copy(root.path())));
    let engine = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            clis: BTreeMap::new(),
            spawn: support::fake(vec![]).spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    let runner = git.clone();
    engine.set_git(Arc::new(move || Some(Box::new(runner.clone()))));
    git.0.lock().unwrap().stats = b"1\t2\ta.txt\0".to_vec();
    let looking = engine.clone();
    git.0.lock().unwrap().look = Some(Box::new(move || {
        assert_eq!(looking.open("conversation").unwrap().area, Area::Project);
    }));
    assert_eq!(engine.changes("conversation")[0].removed, 2);
    engine.shutdown();
}

#[test]
fn file_directory_transitions_never_select_a_subtree_on_either_side() {
    for (base, index, disk_directory) in [
        (b"cache\0".as_slice(), b"cache/auth.json\0".as_slice(), true),
        (b"cache/auth.json\0".as_slice(), b"cache\0".as_slice(), false),
        // A sparse worktree has no directory to catch with filesystem metadata.
        (b"cache\0".as_slice(), b"cache/auth.json\0".as_slice(), false),
    ] {
        let root = tempfile::tempdir().unwrap();
        if disk_directory {
            std::fs::create_dir(root.path().join("cache")).unwrap();
        }
        let mut git = Git::default();
        {
            let mut repo = git.0.lock().unwrap();
            repo.base_names = base.to_vec();
            repo.tracked = index.to_vec();
            repo.stats = b"0\t1\tcache\x001\t0\tcache/auth.json\0".to_vec();
            repo.patch = b"+private credential content\n".to_vec();
        }
        assert!(changes::list(&mut git, &copy(root.path())).is_empty());
        assert!(changes::patch(&mut git, &copy(root.path()), "cache").text.is_empty());
        assert!(
            !git.0.lock().unwrap().calls.iter().any(|(_, args)| args.first().is_some_and(|a| a == "diff")),
            "metadata may enumerate names, but no contents from either subtree may be read"
        );
    }
}

#[test]
fn a_file_replaced_by_a_directory_after_metadata_is_rejected_by_the_safe_open() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("cache"), "plain\n").unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.stats = b"1\t1\tcache\0".to_vec();
        repo.patch = b"+private credential content\n".to_vec();
        let folder = root.path().to_path_buf();
        repo.after_tree = Some(Box::new(move || {
            std::fs::remove_file(folder.join("cache")).unwrap();
            std::fs::create_dir(folder.join("cache")).unwrap();
            std::fs::write(folder.join("cache/auth.json"), "private").unwrap();
        }));
    }
    let patch = changes::patch(&mut git, &copy(root.path()), "cache");
    assert!(patch.text.is_empty());
    assert_eq!(patch.error, Some("error.changes.read"));
    assert!(!git.0.lock().unwrap().calls.iter().any(|(_, args)| args.iter().any(|a| a == "-U3")));
}

#[test]
fn oversized_tracked_worktree_and_base_blobs_are_skipped_before_content_commands() {
    for on_disk in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let mut git = Git::default();
        {
            let mut repo = git.0.lock().unwrap();
            repo.tracked = b"large.txt\0".to_vec();
            repo.base_names = b"large.txt\0".to_vec();
            repo.stats = b"1\t1\tlarge.txt\0".to_vec();
            if !on_disk {
                repo.base_sizes.insert("large.txt".into(), changes::FILE_BYTES + 1);
            }
        }
        if on_disk {
            std::fs::File::create(root.path().join("large.txt")).unwrap().set_len(changes::FILE_BYTES + 1).unwrap();
        }
        let files = changes::list(&mut git, &copy(root.path()));
        assert_eq!(files.len(), 1);
        assert!(files[0].too_large && !files[0].counts_known);
        let patch = changes::patch(&mut git, &copy(root.path()), "large.txt");
        assert!(patch.too_large && patch.text.is_empty());
        assert!(!git.0.lock().unwrap().calls.iter().any(|(_, args)| args.first().is_some_and(|a| a == "diff")));
    }
}

#[test]
fn tracked_growth_after_metadata_is_capped_by_the_safe_read() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("large.txt"), "small\n").unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.stats = b"1\t1\tlarge.txt\0".to_vec();
        let folder = root.path().to_path_buf();
        repo.after_tree =
            Some(Box::new(move || std::fs::File::create(folder.join("large.txt")).unwrap().set_len(changes::FILE_BYTES + 1).unwrap()));
    }
    assert!(changes::patch(&mut git, &copy(root.path()), "large.txt").too_large);
    assert!(!git.0.lock().unwrap().calls.iter().any(|(_, args)| args.iter().any(|a| a == "-U3")));
}

#[test]
fn listing_file_and_byte_caps_keep_found_changes_and_continue_without_losing_names() {
    // macOS refuses paths over 1024 bytes (PATH_MAX), so the listing rightly skips the long names below there: the byte cap
    // is checked on the other systems, the file cap everywhere.
    for long in [false, true].into_iter().filter(|long| !(*long && cfg!(target_os = "macos"))) {
        let root = tempfile::tempdir().unwrap();
        let mut git = Git::default();
        let total = if long { 1950 } else { 2101 };
        {
            let mut repo = git.0.lock().unwrap();
            for i in 0..total {
                let prefix = if long { format!("{}/", vec!["a".repeat(75); 15].join("/")) } else { String::new() };
                repo.stats.extend_from_slice(format!("1\t0\t{prefix}file{i:05}.txt\0").as_bytes());
            }
        }
        let mut cursor = None;
        let mut found = BTreeMap::new();
        let mut pages = 0;
        loop {
            let page = changes::list_page(&mut git, &copy(root.path()), cursor.as_deref());
            if pages == 0 {
                assert!(page.incomplete && page.next.is_some());
                assert!(!page.files.is_empty(), "a capped listing keeps its complete records");
            }
            for file in page.files {
                found.insert(file.path, ());
            }
            pages += 1;
            if !page.incomplete {
                assert!(page.next.is_none());
                break;
            }
            assert_ne!(page.next, cursor, "continuation advances");
            cursor = page.next;
            assert!(pages < 10);
        }
        assert_eq!(found.len(), total);
        assert!(pages >= 2);
    }
}

#[test]
fn partial_batch_failure_continues_after_completed_records_instead_of_repeating_them() {
    let root = tempfile::tempdir().unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        for i in 0..200 {
            repo.stats.extend_from_slice(format!("1\t0\tfile{i:03}-{}.txt\0", "a".repeat(100)).as_bytes());
        }
        repo.fail_stats_at = Some(2);
    }
    let first = changes::list_page(&mut git, &copy(root.path()), None);
    assert!(first.incomplete && !first.files.is_empty());
    assert_ne!(first.next.as_deref(), Some("0:0"), "the cursor includes completed batches");
    let second = changes::list_page(&mut git, &copy(root.path()), first.next.as_deref());
    assert!(!second.incomplete);
    assert!(first.files.iter().all(|f| !second.files.iter().any(|g| g.path == f.path)), "completed names are not processed twice");
    assert_eq!(first.files.len() + second.files.len(), 200);
}

#[test]
fn git_reads_are_pinned_to_the_copy_folder_even_if_repository_config_disagrees() {
    let root = tempfile::tempdir().unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.assert_copy_root = true;
        repo.stats = b"1\t1\tsafe.txt\0".to_vec();
    }
    assert_eq!(changes::list(&mut git, &copy(root.path())).len(), 1);
    let _ = changes::patch(&mut git, &copy(root.path()), "safe.txt");
}

#[test]
fn an_unchanged_oversized_file_is_omitted_using_only_git_cached_metadata() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("large.txt");
    std::fs::File::create(&file).unwrap().set_len(changes::FILE_BYTES + 1).unwrap();
    let meta = std::fs::metadata(&file).unwrap();
    let modified = meta.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap();
    #[cfg(windows)]
    let (ctime, dev, ino, uid, gid) = (meta.created().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap(), 0, 0, 0, 0);
    #[cfg(unix)]
    let (ctime, dev, ino, uid, gid) = {
        use std::os::unix::fs::MetadataExt;
        (Duration::new(meta.ctime() as u64, meta.ctime_nsec() as u32), meta.dev(), meta.ino(), u64::from(meta.uid()), u64::from(meta.gid()))
    };
    let index = root.path().join("index");
    std::fs::File::create(&index)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(meta.modified().unwrap() + Duration::from_secs(2)))
        .unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.tracked = b"large.txt\0".to_vec();
        repo.base_names = b"large.txt\0".to_vec();
        repo.base_sizes.insert("large.txt".into(), changes::FILE_BYTES + 1);
        repo.index_path = Some(index);
        repo.debug=format!("100644 {HEAD} 0\tlarge.txt\0  ctime: {}:{}\n  mtime: {}:{}\n  dev: {dev}\tino: {ino}\n  uid: {uid}\tgid: {gid}\n  size: {}\tflags: 0\n",ctime.as_secs(),ctime.subsec_nanos(),modified.as_secs(),modified.subsec_nanos(),meta.len()).into_bytes();
    }
    let list = changes::list_page(&mut git, &copy(root.path()), None);
    assert!(list.files.is_empty() && !list.incomplete, "an intact large file must not invent a change");
    assert!(
        !git.0.lock().unwrap().calls.iter().any(|(_, args)| args.first().is_some_and(|a| a == "diff")),
        "the unchanged check reads only index stat data"
    );
    std::fs::File::create(file).unwrap().set_len(changes::FILE_BYTES + 2).unwrap();
    let list = changes::list_page(&mut git, &copy(root.path()), None);
    assert_eq!(list.files.len(), 1);
    assert!(list.files[0].too_large, "changed metadata must retain the unknown row");
}

#[test]
fn tracked_count_budget_does_not_prevent_opening_a_small_file_later() {
    let root = tempfile::tempdir().unwrap();
    for path in ["first.txt", "second.txt"] {
        std::fs::write(root.path().join(path), vec![b'x'; 3 * 1024 * 1024]).unwrap();
    }
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.stats = b"1\t1\tfirst.txt\x001\t1\tsecond.txt\0".to_vec();
        repo.patch = b"@@ -1 +1 @@\n-old\n+new\n".to_vec();
    }
    let files = changes::list(&mut git, &copy(root.path()));
    let second = files.iter().find(|f| f.path == "second.txt").unwrap();
    assert!(!second.counts_known && !second.too_large);
    assert!(changes::patch(&mut git, &copy(root.path()), "second.txt").text.contains("+new"));
}

#[test]
fn tracked_changes_git_reads_only_private_snapshots_even_when_the_parent_is_swapped_before_git_opens() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("notes")).unwrap();
    std::fs::write(root.path().join("notes/report.txt"), "inside\n").unwrap();
    std::fs::write(outside.path().join("report.txt"), "private outside\n").unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.tracked = b"notes/report.txt\0".to_vec();
        repo.base_names = repo.tracked.clone();
        repo.blob = b"base\n".to_vec();
        repo.patch = b"diff --git a/before b/after\n--- a/before\n+++ b/after\n@@ -1 +1 @@\n-base\n+inside\n".to_vec();
        let folder = root.path().to_path_buf();
        let target = outside.path().to_path_buf();
        repo.before_patch = Some(Box::new(move |call| {
            std::fs::rename(folder.join("notes"), folder.join("moved")).unwrap();
            #[cfg(windows)]
            junction::create(&target, folder.join("notes")).unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, folder.join("notes")).unwrap();
            assert_eq!(std::fs::read(call.cwd.join("before")).unwrap(), b"base\n");
            assert_eq!(std::fs::read(call.cwd.join("after")).unwrap(), b"inside\n");
            assert!(!call.args.iter().any(|arg| arg.contains("notes/report.txt")));
        }));
    }
    let patch = changes::patch(&mut git, &copy(root.path()), "notes/report.txt");
    assert_eq!(patch.error, None);
    assert_eq!(patch.text, "--- a/notes/report.txt\n+++ b/notes/report.txt\n@@ -1 +1 @@\n-base\n+inside\n");
    let repo = git.0.lock().unwrap();
    assert!(repo.before_patch.is_none(), "the swap must run immediately before Git opens its inputs");
    assert!(repo.calls.iter().any(|(_, args)| args == &["cat-file", "blob", &format!("{FROM}:notes/report.txt")]));
    for (cwd, args) in &repo.calls {
        if args.contains(&"--no-index".into()) {
            assert!(!cwd.exists(), "private snapshots must be removed after success");
        }
    }
}

#[test]
fn tracked_changes_read_errors_and_timeouts_are_not_empty_patches_and_remove_snapshots() {
    for blob_failure in [false, true] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("report.txt"), "current\n").unwrap();
        let mut git = Git::default();
        {
            let mut repo = git.0.lock().unwrap();
            repo.tracked = b"report.txt\0".to_vec();
            repo.base_names = repo.tracked.clone();
            repo.fail_blob = blob_failure;
            // The process executor reports both command failures and timeouts as GitError::Command.
            repo.fail_patch = !blob_failure;
        }
        let patch = changes::patch(&mut git, &copy(root.path()), "report.txt");
        assert_eq!(patch.error, Some("error.changes.read"));
        assert!(patch.text.is_empty() && !patch.too_large && !patch.binary);
        let repo = git.0.lock().unwrap();
        assert_eq!(repo.calls.iter().any(|(_, args)| args.contains(&"--no-index".into())), !blob_failure);
        for (cwd, args) in &repo.calls {
            if args.contains(&"--no-index".into()) {
                assert!(!cwd.exists(), "private snapshots must be removed on error or timeout");
            }
        }
    }
}

#[test]
fn tracked_changes_missing_current_file_is_a_deletion_and_empty_diff_is_success() {
    let root = tempfile::tempdir().unwrap();
    let mut git = Git::default();
    {
        let mut repo = git.0.lock().unwrap();
        repo.base_names = b"gone.txt\0".to_vec();
        repo.blob = b"old\n".to_vec();
        repo.patch = b"@@ -1 +0,0 @@\n-old\n".to_vec();
        repo.before_patch = Some(Box::new(|call| assert_eq!(std::fs::read(call.cwd.join("after")).unwrap(), b"")));
    }
    let patch = changes::patch(&mut git, &copy(root.path()), "gone.txt");
    assert_eq!(patch.error, None);
    assert!(patch.text.starts_with("--- a/gone.txt\n+++ /dev/null\n"));
    git.0.lock().unwrap().patch.clear();
    let patch = changes::patch(&mut git, &copy(root.path()), "gone.txt");
    assert_eq!(patch.error, None);
    assert!(patch.text.is_empty());
}

#[test]
fn tracked_changes_crlf_snapshots_keep_native_text_policy_without_transforming_the_base() {
    for (autocrlf, text, eol, indexed_crlf, expected) in [
        ("true", "unspecified", "unspecified", false, "after !text"),
        ("input", "unspecified", "unspecified", false, "after !text"),
        ("false", "set", "crlf", false, "after text eol=crlf"),
        ("true", "unset", "crlf", false, "after -text eol=crlf"),
        ("true", "auto", "lf", false, "after text=auto eol=lf"),
        ("true", "unspecified", "unspecified", true, "after -text"),
        ("true", "auto", "crlf", true, "after -text"),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("report.txt"), b"a\r\nB\r\nc\r\n").unwrap();
        let mut git = Git::default();
        {
            let mut repo = git.0.lock().unwrap();
            repo.tracked = b"report.txt\0".to_vec();
            repo.base_names = repo.tracked.clone();
            repo.blob = b"a\nb\nc\n".to_vec();
            repo.indexed_blob = if indexed_crlf { b"a\r\nb\r\nc\r\n".to_vec() } else { repo.blob.clone() };
            repo.autocrlf = autocrlf.into();
            repo.text_attr = text.into();
            repo.eol_attr = eol.into();
            repo.patch = b"@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n".to_vec();
            repo.before_patch = Some(Box::new(move |call| {
                assert!(call.args.contains(&format!("core.autocrlf={autocrlf}")));
                let attrs = std::fs::read_to_string(call.cwd.join("attributes")).unwrap();
                assert_eq!(attrs, format!("before -text\n{expected}\n"));
                assert_eq!(std::fs::read(call.cwd.join("before")).unwrap(), b"a\nb\nc\n");
                assert_eq!(std::fs::read(call.cwd.join("after")).unwrap(), b"a\r\nB\r\nc\r\n");
                let attr_config = format!("core.attributesFile={}", call.cwd.join("attributes").display());
                assert!(call.args.contains(&attr_config));
            }));
        }
        let patch = changes::patch(&mut git, &copy(root.path()), "report.txt");
        assert_eq!(patch.error, None);
        assert!(patch.text.contains("-b\n+B\n"));
        assert!(git.0.lock().unwrap().before_patch.is_none());
    }
}
