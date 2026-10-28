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
    Engine::new(opts, Arc::new(|_| {})).unwrap()
}

fn folders(n: usize) -> (tempfile::TempDir, Vec<String>) {
    let root = crate::support::work_folder();
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
    again.forget_recent(&f[2]).unwrap();
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
    e.forget_environment(&id("Both")).unwrap();
    assert_eq!(e.recents().environments.len(), 1);
    assert_eq!(e.start_environment("nothing").unwrap_err(), "error.environment.gone");
    e.shutdown();
}

fn alias(path: &str) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    let name = Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
    format!("{path}{sep}..{sep}{name}")
}

#[tokio::test]
async fn a_save_that_fails_is_reported_and_the_old_file_is_kept() {
    let data = tempfile::tempdir().unwrap();
    let (_root, f) = folders(2);
    let e = engine(data.path());
    e.open_folder(&f[0]).unwrap();
    let before = std::fs::read_to_string(data.path().join("recents.json")).unwrap();
    // A directory where the temp file goes makes every write fail.
    std::fs::create_dir(data.path().join("recents.json.tmp")).unwrap();
    assert_eq!(e.save_environment("Both", &f).unwrap_err(), "error.recents.saveFailed");
    assert_eq!(e.forget_recent(&f[0]).unwrap_err(), "error.recents.saveFailed");
    assert_eq!(std::fs::read_to_string(data.path().join("recents.json")).unwrap(), before, "the old file is untouched");
    assert!(e.recents().environments.is_empty());
    std::fs::remove_dir(data.path().join("recents.json.tmp")).unwrap();
    e.save_environment("Both", &f).unwrap();
    assert_eq!(e.recents().environments.len(), 1);
    e.shutdown();
}

#[tokio::test]
async fn an_edited_file_cannot_show_more_than_the_limits() {
    let data = tempfile::tempdir().unwrap();
    let (_root, f) = folders(10);
    let many = |n: usize| f.iter().cycle().take(n).cloned().collect::<Vec<_>>();
    let envs: Vec<_> =
        (0..15).map(|i| serde_json::json!({ "id": format!("e{i}"), "name": format!("env {i}"), "folders": many(12) })).collect();
    let mut all = envs;
    all.push(serde_json::json!({ "id": "blank", "name": "  ", "folders": many(2) }));
    all.push(serde_json::json!({ "id": "empty", "name": "empty", "folders": [] }));
    std::fs::write(data.path().join("recents.json"), serde_json::json!({ "folders": f, "environments": all }).to_string()).unwrap();
    let e = engine(data.path());
    let view = e.recents();
    assert_eq!(view.folders.len(), 5);
    assert_eq!(view.environments.len(), 12);
    assert!(view.environments.iter().all(|x| x.folders.len() <= 8 && !x.name.trim().is_empty()));
    assert_eq!(e.start_environment("e0").unwrap().len(), 8, "an environment never opens more than eight folders");
    e.shutdown();
}

#[tokio::test]
async fn a_corrupt_file_is_an_empty_list_and_the_next_save_replaces_it() {
    let data = tempfile::tempdir().unwrap();
    let (_root, f) = folders(1);
    std::fs::write(data.path().join("recents.json"), "{ not json").unwrap();
    let e = engine(data.path());
    assert!(e.recents().folders.is_empty());
    e.open_folder(&f[0]).unwrap();
    let text = std::fs::read_to_string(data.path().join("recents.json")).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&text).is_ok(), "{text}");
    assert_eq!(e.recents().folders.len(), 1);
    e.shutdown();
}

#[tokio::test]
async fn another_spelling_of_a_stored_folder_is_the_same_recent_and_the_same_project() {
    let data = tempfile::tempdir().unwrap();
    let (_root, f) = folders(1);
    let other = alias(&f[0]);
    std::fs::write(data.path().join("recents.json"), serde_json::json!({ "folders": [other] }).to_string()).unwrap();
    std::fs::write(data.path().join("projects.json"), serde_json::json!([{ "id": "old", "name": "f0", "path": other }]).to_string())
        .unwrap();
    let e = engine(data.path());
    let opened = e.open_folder(&f[0]).unwrap();
    assert_eq!(opened.id, "old", "the project that already holds this folder is reused");
    assert_eq!(e.state().projects.len(), 1);
    assert_eq!(e.recents().folders.len(), 1);
    e.shutdown();
}

#[tokio::test]
async fn a_damaged_project_list_refuses_to_open_or_remove_and_keeps_every_byte() {
    let data = tempfile::tempdir().unwrap();
    let (_root, f) = folders(1);
    let e = engine(data.path());
    let damaged = r#"[{"id":"p1","name":"kept","path":"C:/kept","grant":"#;
    std::fs::write(data.path().join("projects.json"), damaged).unwrap();
    assert_eq!(e.open_folder(&f[0]).unwrap_err(), "error.store.corrupt");
    assert_eq!(e.remove_project("p1").unwrap_err(), "error.store.corrupt");
    assert_eq!(std::fs::read_to_string(data.path().join("projects.json")).unwrap(), damaged, "the grants it held are not overwritten");
    e.shutdown();
}
