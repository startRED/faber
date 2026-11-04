//! Native Git integration: long paths and moved repositories preserve separate copies and unsaved work.
use crate::support;
use faber_engine::project::{copy, git::Process};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions, changes};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

fn git(folder: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let args: Vec<_> = args.iter().map(|arg| arg.strip_prefix("\\\\?\\").unwrap_or(arg)).collect();
    let out = command.current_dir(folder).args(["-c", "core.longpaths=true", "-c", "core.hooksPath="]).args(&args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn repair_repository(root: &Path) {
    fs::create_dir_all(root).unwrap();
    git(root, &["init"]);
    fs::write(root.join("original.txt"), "original\n").unwrap();
    git(root, &["add", "original.txt"]);
    git(root, &["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "initial"]);
}

fn repair_preserves_sentinel(destination: &str) {
    let temp = support::canonical_tempdir().unwrap();
    let old = temp.path().join("old");
    let moved = temp.path().join("moved");
    let data = temp.path().join("data");
    repair_repository(&old);
    let mut runner = Process::find().unwrap();
    let work = copy::make(&mut runner, &old, &data, "t515c", "faber/t515c", None, None).unwrap().unwrap();
    let sibling = temp.path().join("sibling");
    git(&old, &["worktree", "add", "-b", "sibling", sibling.to_str().unwrap()]);
    let target = match destination {
        "copy" => Path::new(&work.path).join(".git"),
        "sibling" => sibling.join(".git"),
        "gitdir" => old.join(".git/worktrees/t515c/gitdir"),
        _ => unreachable!(),
    };
    let sentinel = temp.path().join("sentinel");
    let bytes = fs::read(&target).unwrap();
    fs::write(&sentinel, &bytes).unwrap();
    fs::remove_file(&target).unwrap();
    fs::hard_link(&sentinel, &target).unwrap();
    let before = fs::read(&sentinel).unwrap();
    fs::rename(&old, &moved).unwrap();
    let result = copy::repair(&mut runner, &moved, &data, "t515c", &work);
    assert_eq!(fs::read(&sentinel).unwrap(), before, "repair must not truncate the external {destination} sentinel");
    assert!(result.is_err(), "shared {destination} metadata must refuse repair before any write");
}

#[test]
fn t515c_repair_refuses_hardlinked_copy_gitfile() {
    repair_preserves_sentinel("copy");
}

#[test]
fn t515c_repair_refuses_hardlinked_sibling_gitfile() {
    repair_preserves_sentinel("sibling");
}

#[test]
fn t515c_repair_refuses_hardlinked_admin_backlink() {
    repair_preserves_sentinel("gitdir");
}

#[test]
fn t515c_moved_linked_project_stays_registered_after_prune() {
    let temp = support::canonical_tempdir().unwrap();
    let main = temp.path().join("main");
    let old = temp.path().join("project");
    let moved = temp.path().join("moved");
    let data = temp.path().join("data");
    repair_repository(&main);
    git(&main, &["worktree", "add", "-b", "project", old.to_str().unwrap()]);
    let mut runner = Process::find().unwrap();
    let work = copy::make(&mut runner, &old, &data, "t515c", "faber/t515c", None, None).unwrap().unwrap();
    fs::write(Path::new(&work.path).join("edits.txt"), "unsaved work").unwrap();
    fs::rename(&old, &moved).unwrap();
    let repaired = copy::repair(&mut runner, &moved, &data, "t515c", &work).unwrap();
    let backlink = fs::read_to_string(main.join(".git/worktrees/project/gitdir")).unwrap();
    assert_eq!(
        Path::new(backlink.trim()).canonicalize().unwrap(),
        moved.join(".git").canonicalize().unwrap(),
        "moved project registration"
    );
    git(&main, &["worktree", "prune", "--expire", "now"]);
    git(&moved, &["status", "--porcelain"]);
    git(Path::new(&work.path), &["status", "--porcelain"]);
    assert_eq!(Path::new(&repaired.root).canonicalize().unwrap(), moved.canonicalize().unwrap());
    assert_eq!(fs::read(Path::new(&work.path).join("edits.txt")).unwrap(), b"unsaved work");
}

#[test]
fn t515c_moved_main_project_repairs_native_copy_without_touching_edits() {
    let temp = support::canonical_tempdir().unwrap();
    let old = temp.path().join("old");
    let moved = temp.path().join("moved");
    let data = temp.path().join("data");
    repair_repository(&old);
    let mut runner = Process::find().unwrap();
    let work = copy::make(&mut runner, &old, &data, "t515c", "faber/t515c", None, None).unwrap().unwrap();
    fs::write(Path::new(&work.path).join("edits.txt"), "unsaved work").unwrap();
    fs::rename(&old, &moved).unwrap();
    copy::repair(&mut runner, &moved, &data, "t515c", &work).unwrap();
    git(Path::new(&work.path), &["status", "--porcelain"]);
    assert_eq!(fs::read(Path::new(&work.path).join("edits.txt")).unwrap(), b"unsaved work");
}

fn repair_refuses_foreign_gitdir(admin_suffix: &str, target_suffix: &str) {
    let temp = support::canonical_tempdir().unwrap();
    let old = temp.path().join("old");
    let foreign = temp.path().join("foreign");
    let moved = temp.path().join("moved");
    let data = temp.path().join("data");
    repair_repository(&old);
    repair_repository(&foreign);
    let mut runner = Process::find().unwrap();
    let work = copy::make(&mut runner, &old, &data, "t515c", "faber/t515c", None, None).unwrap().unwrap();
    copy::make(&mut runner, &foreign, &temp.path().join("foreign-data"), "t515c", "faber/t515c", None, None).unwrap().unwrap();
    let original_admin = foreign.join(".git/worktrees/t515c");
    let admin = foreign.join(format!(".git/worktrees/t515c{admin_suffix}"));
    if admin != original_admin {
        fs::rename(&original_admin, &admin).unwrap();
    }
    let sentinel = temp.path().join("sentinel");
    fs::hard_link(admin.join("gitdir"), &sentinel).unwrap();
    let before = fs::read(&sentinel).unwrap();
    let target = admin.to_string_lossy().replace('\\', "/");
    let target = target.strip_prefix("//?/").unwrap_or(&target);
    fs::write(Path::new(&work.path).join(".git"), format!("gitdir: {target}{target_suffix}\n")).unwrap();
    fs::rename(&old, &moved).unwrap();
    let result = copy::repair(&mut runner, &moved, &data, "t515c", &work);
    assert_eq!(fs::read(&sentinel).unwrap(), before, "foreign gitdir sentinel must stay unchanged");
    assert!(result.is_err());
}

#[test]
fn t515c_repair_refuses_foreign_gitdir_with_trailing_separator() {
    repair_refuses_foreign_gitdir("", "/");
}

#[test]
fn t515c_repair_refuses_foreign_gitdir_with_embedded_nul() {
    repair_refuses_foreign_gitdir("-foreign", "\0/t515c");
}

#[cfg(unix)]
#[test]
fn t515c_repair_preserves_git_non_whitespace_suffixes() {
    for suffix in ["\u{c}", "\u{a0}"] {
        repair_refuses_foreign_gitdir(suffix, "");
    }
}

#[test]
fn t515c_moved_project_with_reclaimed_copy_can_be_repointed() {
    let temp = support::canonical_tempdir().unwrap();
    let old = temp.path().join("old");
    let moved = temp.path().join("moved");
    let data = temp.path().join("data");
    repair_repository(&old);
    let mut runner = Process::find().unwrap();
    let work = copy::make(&mut runner, &old, &data, "t515c", "faber/t515c", None, None).unwrap().unwrap();
    copy::remove(&mut runner, &data, "t515c", &work).unwrap();
    let admins = old.join(".git/worktrees");
    if admins.exists() {
        fs::remove_dir(&admins).unwrap();
    }
    fs::rename(&old, &moved).unwrap();
    let repaired = copy::repair(&mut runner, &moved, &data, "t515c", &work).unwrap();
    assert_eq!(Path::new(&repaired.root).canonicalize().unwrap(), moved.canonicalize().unwrap());
    assert!(!Path::new(&work.path).exists());
}

#[tokio::test]
async fn t515a_moved_project_repairs_loaded_and_unloaded_copies_without_touching_edits() {
    use faber_engine::model::{Area, Conversation, Project};
    let temp = support::canonical_tempdir().unwrap();
    let old = temp.path().join("old");
    let moved = temp.path().join("moved");
    let data = support::DataFolder::new();
    repair_repository(&old);
    fs::create_dir(old.join("app")).unwrap();
    fs::write(old.join("app/tracked.txt"), "tracked\n").unwrap();
    git(&old, &["add", "app/tracked.txt"]);
    git(&old, &["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "subfolder"]);
    let project = Project::new("t515c-project".into(), "Moved project".into(), old.to_string_lossy().into_owned());
    let store = Store::new(data.path());
    store.save_projects(std::slice::from_ref(&project)).unwrap();
    let mut runner = Process::find().unwrap();
    for id in ["t515c-loaded", "t515c-unloaded"] {
        let work = copy::make(&mut runner, &old.join("app"), data.path(), id, &format!("faber/{id}"), None, None).unwrap().unwrap();
        fs::write(work.folder().join("edits.txt"), "unsaved work").unwrap();
        let mut conv: Conversation = serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
        conv.id = id.into();
        conv.area = Area::Project;
        conv.project_id = Some(project.id.clone());
        conv.messages.clear();
        conv.sessions.clear();
        conv.copy = Some(work);
        store.save_checked(&conv).unwrap();
    }
    let engine = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            #[cfg(windows)]
            chat_dir: data.chat_path().into(),
            clis: BTreeMap::new(),
            spawn: support::fake(vec![]).spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    engine.set_git(Arc::new(|| Some(Box::new(Process::find().unwrap()))));
    engine.open("t515c-loaded").unwrap();
    fs::rename(&old, &moved).unwrap();
    let dotgit = copy::home(data.path()).join("t515c-loaded/.git");
    let sentinel = temp.path().join("sentinel");
    fs::hard_link(&dotgit, &sentinel).unwrap();
    assert!(engine.repoint_project(&project.id, &moved.join("app").to_string_lossy()).is_err());
    assert_eq!(engine.project(&project.id).unwrap().path, project.path);
    for id in ["t515c-loaded", "t515c-unloaded"] {
        assert_eq!(store.get(id).unwrap().copy.unwrap().root, old.to_string_lossy().trim_start_matches("\\\\?\\").replace('\\', "/"));
    }
    fs::remove_file(&dotgit).unwrap();
    fs::copy(&sentinel, &dotgit).unwrap();
    engine.repoint_project(&project.id, &moved.join("app").to_string_lossy()).unwrap();
    for id in ["t515c-loaded", "t515c-unloaded"] {
        let saved = store.get(id).unwrap().copy.unwrap();
        let loaded = engine.open(id).unwrap().copy.unwrap();
        assert_eq!(saved, loaded);
        assert_eq!(Path::new(&saved.root).canonicalize().unwrap(), moved.canonicalize().unwrap());
        assert_eq!(saved.sub, "app");
        assert_eq!(fs::read(saved.folder().join("edits.txt")).unwrap(), b"unsaved work");
        git(&saved.folder(), &["status", "--porcelain"]);
    }
    engine.shutdown();
}

#[tokio::test]
async fn project_paths_open_store_copy_changes_and_remove() {
    let root = support::canonical_tempdir().unwrap();
    let data = support::DataFolder::new();
    let folder = root
        .path()
        .canonicalize()
        .unwrap()
        .join("a".repeat(90))
        .join("b".repeat(90))
        .join("c".repeat(90))
        .join("Meu Projeto (final) d'água ação");
    fs::create_dir_all(&folder).unwrap();
    assert!(folder.to_string_lossy().encode_utf16().count() > 260);
    git(root.path(), &["init"]);
    fs::write(folder.join("original.txt"), "original\n").unwrap();
    git(root.path(), &["add", "--all"]);
    git(root.path(), &["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "initial"]);

    let engine = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            #[cfg(windows)]
            chat_dir: data.chat_path().into(),
            clis: BTreeMap::new(),
            spawn: support::fake(vec![]).spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    let project = engine.open_folder(&folder.to_string_lossy()).unwrap();
    let stored = Store::new(data.path()).projects_checked().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].path.as_bytes(), project.path.as_bytes());
    assert_eq!(Path::new(&project.path).canonicalize().unwrap(), folder.canonicalize().unwrap());

    let mut runner = Process::find().unwrap();
    let work = copy::make(&mut runner, &folder, data.path(), "test-206", "faber/test-206", None, None).unwrap().expect("repository copy");
    assert_ne!(work.folder().canonicalize().unwrap(), folder.canonicalize().unwrap());
    assert_eq!(fs::read_to_string(work.folder().join("original.txt")).unwrap(), "original\n");
    fs::write(work.folder().join("novo.txt"), "novo\n").unwrap();
    let files = changes::list(&mut runner, &work);
    let changed = format!("{}/novo.txt", work.sub);
    assert!(files.iter().any(|file| file.path == changed && file.added == 1), "{files:?}");
    copy::remove(&mut runner, data.path(), "test-206", &work).unwrap();
    assert!(!work.folder().exists());
    assert!(folder.join("original.txt").exists());
    #[cfg(windows)]
    {
        fs::create_dir(folder.join(".git")).unwrap();
        assert_eq!(
            copy::make(&mut runner, &folder, data.path(), "nested-206", "faber/nested-206", None, None),
            Err("error.copy.failed"),
            "a long nested repository must never use the outer repository"
        );
    }
    engine.shutdown();
}
