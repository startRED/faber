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

#[tokio::test]
async fn recovery_320_missing_project_folder_refuses_send_and_warm_before_spawning_any_cli() {
    use faber_engine::cli::Cli;
    use faber_engine::model::{Area, Vendor};
    use faber_engine::router::Choice;
    use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    let tmp = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();
    #[cfg(windows)]
    let chat_dir = faber_engine::permissions::checked_folder(tmp.path()).expect("a checked parent").join("work");

    let folder = faber_engine::permissions::checked_folder(tmp.path()).expect("a checked parent").join("moved-project");
    fs::create_dir(&folder).unwrap();
    let folder_str = folder.to_string_lossy().into_owned();

    let spawns = Arc::new(Mutex::new(0usize));
    let spawns_sink = spawns.clone();
    let events: Arc<Mutex<Vec<AppEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let events_sink = events.clone();
    let mut clis = BTreeMap::new();
    clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });

    let e = Engine::new(
        EngineOptions {
            data_dir,
            #[cfg(windows)]
            chat_dir,
            clis,
            spawn: Arc::new(move |_, _, _| {
                *spawns_sink.lock().unwrap() += 1;
                Err(std::io::Error::other("no cli"))
            }),
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(move |ev| events_sink.lock().unwrap().push(ev)),
    )
    .unwrap();

    let project = e.open_folder(&folder_str).unwrap();
    let initial = serde_json::to_value(e.state()).unwrap();
    assert_eq!(initial["projects"][0]["state"], "present");

    fs::remove_dir_all(&folder).unwrap();
    events.lock().unwrap().clear();
    e.set_attention(true, true);
    assert!(
        events.lock().unwrap().iter().any(|ev| matches!(ev, AppEvent::List)),
        "focusing the window refreshes project folder states and notifies the screen when one went missing"
    );
    let after_move = serde_json::to_value(e.state()).unwrap();
    assert_eq!(after_move["projects"][0]["state"], "missing");

    let choice = Choice { vendor: "claude".into(), model: None, effort: None };
    e.warm(WarmRequest { conversation_id: None, area: Area::Project, project_id: Some(project.id.clone()), choice: choice.clone() });
    assert_eq!(*spawns.lock().unwrap(), 0, "warm to a missing project folder is ignored before spawning any CLI");

    let send_err = e
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id.clone()),
            text: "build it".into(),
            images: vec![],
            files: vec![],
            choice,
        })
        .unwrap_err();
    assert_eq!(send_err, "error.project.missing");
    assert_eq!(*spawns.lock().unwrap(), 0, "no CLI or router process may start when the project folder is missing");
    assert!(e.state().conversations.is_empty(), "refused send leaves no conversation or streaming message behind");
    e.shutdown();
}

#[tokio::test]
async fn recovery_320_unclean_exit_marker_is_reported_in_state_once_and_cleared_after_clean_shutdown() {
    use faber_engine::cli::Cli;
    use faber_engine::{Engine, EngineOptions};
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    let tmp = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let data_dir = tmp.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();
    #[cfg(windows)]
    let chat_dir = faber_engine::permissions::checked_folder(tmp.path()).expect("a checked parent").join("work");

    let make = || {
        Engine::new(
            EngineOptions {
                data_dir: data_dir.clone(),
                #[cfg(windows)]
                chat_dir: chat_dir.clone(),
                clis: BTreeMap::<_, Cli>::new(),
                spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no cli"))),
                windows: false,
                claude_env_billed: false,
                max_warm: 1,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap()
    };

    faber_engine::crash::start_run(&data_dir, "0.1.0", "abc");
    assert!(faber_engine::crash::start_run(&data_dir, "0.1.0", "abc"));
    let e = make();
    let state_json = serde_json::to_value(e.state()).unwrap();
    assert!(state_json["uncleanExit"].is_object(), "an unclean exit marker is reported in state()");
    assert_eq!(state_json["uncleanExit"]["panic"], false);
    e.shutdown();
    faber_engine::crash::end_run(&data_dir);

    assert!(!faber_engine::crash::start_run(&data_dir, "0.1.0", "abc"));
    let clean = make();
    let clean_json = serde_json::to_value(clean.state()).unwrap();
    assert!(clean_json["uncleanExit"].is_null(), "a clean shutdown leaves no unclean exit marker");
    clean.shutdown();
    faber_engine::crash::end_run(&data_dir);
}
