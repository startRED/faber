//! Long project paths keep their spelling through storage and a separate working copy.
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
    let out = Command::new("git").current_dir(folder).args(["-c", "core.longpaths=true"]).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
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
