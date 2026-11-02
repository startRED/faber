//! Native Git resolution and project fallback: no processes or real repositories.
use faber_engine::project::git::{GitError, config_overrides, valid_oid};

#[tokio::test]
async fn missing_git_starts_a_project_turn_in_place_and_reports_why() {
    use faber_engine::cli::Cli;
    use faber_engine::model::{Area, Status, Vendor};
    use faber_engine::router::Choice;
    use faber_engine::{Engine, EngineOptions, SendRequest};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    let data = crate::support::DataFolder::new();
    let folder = crate::support::work_folder();
    let cwds = Arc::new(Mutex::new(Vec::new()));
    let seen = cwds.clone();
    let spawn = crate::support::fake(vec![]).spawn;
    let engine = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            #[cfg(windows)]
            chat_dir: data.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
            spawn: Arc::new(move |bin, argv, cwd| {
                seen.lock().unwrap().push(cwd.to_path_buf());
                spawn(bin, argv, cwd)
            }),
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
    // An empty PATH with no installer locations, without mutating the test process's environment.
    struct Never;
    impl faber_engine::project::git::Executor for Never {
        fn execute(&mut self, _: &faber_engine::project::git::Invocation) -> Result<faber_engine::project::git::Output, GitError> {
            panic!("availability must not run git");
        }
    }
    let installed = Arc::new(AtomicBool::new(false));
    let available = installed.clone();
    engine.set_git(Arc::new(move || {
        assert!(Cli::find_in("git", "", cfg!(windows), &|_| None, &|_| false, &|_| String::new()).is_none());
        available.load(Ordering::SeqCst).then(|| Box::new(Never) as Box<dyn faber_engine::project::git::Executor + Send>)
    }));
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    let id = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "Work in this folder".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    crate::support::until("the project turn never started", || !cwds.lock().unwrap().is_empty()).await;
    let conv = engine.open(&id).unwrap();
    assert!(conv.copy.is_none());
    assert!(conv.messages.iter().all(|m| m.status != Some(Status::Error)));
    assert_eq!(cwds.lock().unwrap()[0].canonicalize().unwrap(), folder.path().canonicalize().unwrap());
    assert_eq!(engine.state().git_reason, Some("reason.git_missing"));
    assert!(engine.changes_page(&id, None).files.is_empty());
    installed.store(true, Ordering::SeqCst);
    engine.set_attention(false, false);
    assert_eq!(engine.state().git_reason, Some("reason.git_missing"));
    engine.set_attention(true, true);
    assert_eq!(engine.state().git_reason, None, "focus checks an installation made while away");
    assert!(engine.open(&id).unwrap().copy.is_none(), "a conversation that began in place stays there");
    installed.store(false, Ordering::SeqCst);
    engine.set_attention(true, true);
    assert_eq!(engine.state().git_reason, Some("reason.git_missing"));
    engine.shutdown();
}

#[test]
fn null_object_ids_are_not_ids() {
    for width in [40, 64] {
        assert!(!valid_oid(&"0".repeat(width)));
        assert!(valid_oid(&"a".repeat(width)));
    }
    assert!(!valid_oid(&"a".repeat(41)));
}

#[test]
fn command_config_overrides_are_pure_and_fail_closed() {
    let args =
        config_overrides(b"filter.trap.process\nfilter.trap.required\nhook.lint.command\nhook.lint.event\nhook.ref.enabled\n").unwrap();
    assert_eq!(
        args,
        ["-c", "filter.trap.process=", "-c", "filter.trap.required=false", "-c", "hook.lint.enabled=false", "-c", "hook.ref.enabled=false"]
    );
    for bad in ["hook.unsafe name.command", "hook..command", "hook.x.command\nremote.x.url", "filter.bad;command.clean"] {
        assert_eq!(config_overrides(bad.as_bytes()), Err(GitError::Command));
    }
}
