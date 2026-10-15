//! Recent folders and saved environments through the engine: what is used is remembered, a refused folder adds nothing, and
//! an environment opens the folders that still exist.

use faber_engine::cli::Cli;
use faber_engine::{Engine, EngineOptions};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn engine(dir: &Path) -> Engine {
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: BTreeMap::<_, Cli>::new(),
        spawn: crate::support::fake(vec![]).spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 1,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    Engine::new(opts, Arc::new(|_| {}))
}

fn folders(n: usize) -> (tempfile::TempDir, Vec<String>) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-workspaces");
    std::fs::create_dir_all(&base).unwrap();
    let root = tempfile::tempdir_in(base).unwrap();
    let paths = (0..n)
        .map(|i| {
            let d = root.path().join(format!("f{i}"));
            std::fs::create_dir(&d).unwrap();
            d.to_string_lossy().into_owned()
        })
        .collect();
    (root, paths)
}

#[tokio::test]
async fn opening_folders_fills_the_recents_and_survives_a_restart_in_the_file() {
    let data = tempfile::tempdir().unwrap();
    let (_root, f) = folders(3);
    let e = engine(data.path());
    for p in &f {
        e.open_folder(p).unwrap();
    }
    e.open_folder(&f[0]).unwrap();
    let names = |e: &Engine| e.recents().folders.into_iter().map(|f| f.name).collect::<Vec<_>>();
    assert_eq!(names(&e), ["f0", "f2", "f1"]);
    assert_eq!(e.state().projects.len(), 3, "the same folder is one project");
    e.shutdown();
    let again = engine(data.path());
    assert_eq!(names(&again), ["f0", "f2", "f1"]);
    assert!(!data.path().join("recents.json.tmp").exists(), "the temp file of the atomic write is gone");
    again.forget_recent(&f[2]);
    assert_eq!(names(&again), ["f0", "f1"]);
    again.shutdown();
}

#[tokio::test]
async fn a_refused_or_missing_folder_is_not_opened_and_not_remembered() {
    let data = tempfile::tempdir().unwrap();
    let (root, f) = folders(1);
    let e = engine(data.path());
    let ssh = root.path().join(".ssh");
    std::fs::create_dir(&ssh).unwrap();
    assert_eq!(e.open_folder(&ssh.to_string_lossy()).unwrap_err(), "error.workingFolder");
    assert_eq!(e.open_folder(&root.path().join("nope").to_string_lossy()).unwrap_err(), "error.recents.gone");
    assert!(e.recents().folders.is_empty() && e.state().projects.is_empty());
    e.open_folder(&f[0]).unwrap();
    std::fs::remove_dir(&f[0]).unwrap();
    assert_eq!(e.recents().folders[0].reason, Some("reason.recents.gone"));
    e.shutdown();
}

#[tokio::test]
async fn an_environment_opens_its_folders_first_one_most_recent_and_skips_the_ones_that_are_gone() {
    let data = tempfile::tempdir().unwrap();
    let (_root, f) = folders(3);
    let e = engine(data.path());
    e.save_environment("Both", &f[..2]).unwrap();
    e.save_environment("Three", &f).unwrap();
    std::fs::remove_dir(&f[2]).unwrap();
    let envs = e.recents().environments;
    let id = |name: &str| envs.iter().find(|x| x.name == name).unwrap().id.clone();
    let opened = e.start_environment(&id("Three")).unwrap();
    assert_eq!(opened.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["f0", "f1"]);
    assert_eq!(e.recents().folders[0].name, "f0");
    e.forget_environment(&id("Both"));
    assert_eq!(e.recents().environments.len(), 1);
    assert_eq!(e.start_environment("nothing").unwrap_err(), "error.environment.gone");
    e.shutdown();
}
