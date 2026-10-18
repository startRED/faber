//! What a project list keeps: its fields survive a reload, the ones a newer Faber (or the retired missions) wrote included, and
//! a damaged list is an error for Projects, not an empty one. No process, git or CLI runs here.

use faber_engine::model::Project;
use faber_engine::store::{Fault, Store};
use std::fs;

#[test]
fn a_project_survives_a_reload() {
    let dir = tempfile::tempdir().unwrap();
    let project = Project::new("p1".into(), "faber".into(), "E:/src/faber".into());
    Store::new(dir.path()).save_projects(std::slice::from_ref(&project)).unwrap();
    assert_eq!(Store::new(dir.path()).projects_checked().unwrap(), vec![project]);
}

#[test]
fn fields_a_newer_faber_wrote_survive_an_older_reader() {
    let dir = tempfile::tempdir().unwrap();
    // `grant` is what the retired missions wrote: nothing reads it now, and nothing drops it.
    let json = r#"[{"id": "p1", "name": "faber", "path": "x", "future": {"a": 1},
        "grant": {"schema": 1, "repo": "x/.git", "mode": "seguro", "revision": 2, "grantedAt": 5, "selfUpdate": false, "later": true}}]"#;
    fs::write(dir.path().join("projects.json"), json).unwrap();
    let store = Store::new(dir.path());
    let projects = store.projects_checked().unwrap();
    store.save_projects(&projects).unwrap();
    let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.path().join("projects.json")).unwrap()).unwrap();
    assert_eq!(raw[0]["future"]["a"], 1);
    assert_eq!(raw[0]["grant"]["later"], true);
}

#[test]
fn a_damaged_project_list_is_an_error_for_projects() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("projects.json"), "[{").unwrap();
    let store = Store::new(dir.path());
    assert_eq!(store.projects_checked(), Err(Fault::Corrupt));
    assert_eq!(Fault::Corrupt.key(), "error.store.corrupt");
    assert!(Store::new(tempfile::tempdir().unwrap().path()).projects_checked().unwrap().is_empty(), "no file is no project");
}
