use crate::support::{DataFolder, pipes};
use faber_engine::cli::Cli;
use faber_engine::engine::WaitCheck;
use faber_engine::model::{Area, EndKind, Role, Settings, Vendor};
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{sync::Arc, time::Duration};

fn engine(dir: &DataFolder, spawn: faber_engine::proc::Spawner) -> Engine {
    let e = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: [(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })].into_iter().collect(),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(30),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    e.set_keep_going_pace(0.001);
    e.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(faber_engine::budget::Billing::Subscription),
            ..Default::default()
        })],
    );
    e
}

#[tokio::test]
async fn a_queued_wait_is_the_only_continuation_after_close() {
    queued_wait_survives_restart(false).await;
}

#[tokio::test]
async fn a_queued_wait_is_the_only_continuation_after_update() {
    queued_wait_survives_restart(true).await;
}

async fn queue_wait(
    dir: &DataFolder,
    spawn: faber_engine::proc::Spawner,
    made: &Arc<std::sync::Mutex<Vec<crate::support::Pipe>>>,
) -> (Engine, String, faber_engine::model::QueuedTurn, crate::support::Pipe) {
    let app = engine(dir, spawn);
    let project = app.add_project(&dir.path().to_string_lossy()).unwrap();
    let now = std::time::UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
    let clock = Arc::new(AtomicU64::new(now));
    let time = clock.clone();
    app.set_time_source(Arc::new(move || time.load(Ordering::Relaxed) as f64 / 1000.0));
    let id = app
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "Original work".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let first = crate::support::next_pipe(made).await;
    app.session_wait(&id, vec![], Some("Verify the work".into())).unwrap();
    let wait = app.open(&id).unwrap().waiting_on.unwrap();
    clock.store(wait.until + 2, Ordering::Relaxed);
    assert_eq!(app.wake_wait(&id, wait.since, wait.until), WaitCheck::Admitted);
    let queued = app.open(&id).unwrap().queued.pop().unwrap();
    assert_eq!(queued.wake.as_ref().unwrap().why, "wake.wait");
    (app, id, queued, first)
}

async fn queued_wait_survives_restart(update: bool) {
    let dir = DataFolder::new();
    let (spawn, made) = pipes();
    let (app, id, queued, _first) = queue_wait(&dir, spawn.clone(), &made).await;
    if update {
        app.note_resume();
    }
    app.shutdown();
    drop(app);

    let app = engine(&dir, spawn.clone());
    app.resume_after_update();
    app.resume_interrupted();
    let resumed = crate::support::next_pipe(&made).await;
    let conv = app.open(&id).unwrap();
    assert_eq!(conv.messages[2].id, queued.id, "startup must deliver the saved wait, not create another turn");
    assert_eq!(conv.messages[2].text, queued.text);
    assert!(conv.queued.is_empty());
    crate::support::until("resumed model receives saved work", || {
        resumed.lines.lock().unwrap().iter().any(|line| {
            line["message"]["content"].as_str().is_some_and(|input| input.contains("Original work") && input.contains(&queued.text))
        })
    })
    .await;
    if update {
        app.note_resume();
    }
    app.shutdown();
    drop(app);

    let app = engine(&dir, spawn);
    app.resume_after_update();
    app.resume_interrupted();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let conv = app.open(&id).unwrap();
    assert_eq!(conv.messages.len(), 4, "a cut while already resuming requires Continue");
    assert_eq!(conv.messages[2].id, queued.id);
    assert_eq!(conv.ended.unwrap().kind, EndKind::Interrupted);
    app.shutdown();
}

#[tokio::test]
async fn a_lost_permission_holds_a_queued_wait_across_restarts() {
    use serde_json::json;
    let dir = DataFolder::new();
    let (spawn, made) = pipes();
    let (app, id, queued, mut first) = queue_wait(&dir, spawn.clone(), &made).await;
    first.say(json!({"type":"control_request","request_id":"q","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"notes.txt","content":"x"}}})).await;
    crate::support::until("the earlier reply still needs approval", || {
        app.open(&id).unwrap().messages.last().unwrap().permissions.as_ref().is_some_and(|p| !p.is_empty())
    })
    .await;
    app.note_resume();
    app.shutdown();
    drop(app);

    // One install restart and then an ordinary restart, each without the permission's original process.
    for _ in 0..2 {
        let app = engine(&dir, spawn.clone());
        app.resume_after_update();
        app.resume_interrupted();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let conv = app.open(&id).unwrap();
        assert_eq!(conv.messages.len(), 2);
        assert!(conv.messages[1].permissions.as_ref().is_none_or(Vec::is_empty));
        assert_eq!(conv.queued[0].id, queued.id);
        assert!(made.lock().unwrap().is_empty());
        app.shutdown();
    }
    let app = engine(&dir, spawn);
    let conv = app.open(&id).unwrap();
    for text in ["My new request", "Next step for my new request"] {
        app.send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: conv.project_id.clone(),
            text: text.into(),
            images: vec![],
            files: vec![],
            choice: conv.choice.clone().unwrap(),
        })
        .unwrap();
    }
    let mut current = crate::support::next_pipe(&made).await;
    current.say(json!({"type":"system","subtype":"init","session_id":"new","model":"haiku"})).await;
    current.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0,"usage":{}})).await;
    tokio::time::timeout(
        Duration::from_secs(3),
        crate::support::until("the unrelated message can pass the held return", || app.open(&id).unwrap().messages.len() == 6),
    )
    .await
    .expect("the unrelated message was held with the return");
    let conv = app.open(&id).unwrap();
    assert_eq!(conv.messages[4].text, "Next step for my new request");
    assert_eq!(conv.queued[0].id, queued.id, "other work still does not send the held return");
    app.send_queued(&id, &queued.id).unwrap();
    let waiting = app.open(&id).unwrap().queued.pop().unwrap();
    assert!(!waiting.sending, "a wait check still takes a separate turn");
    assert!(waiting.wake.unwrap().wait_restart.is_none(), "the person's Send now releases the old hold");
    app.shutdown();
    drop(app);

    let (spawn, made) = pipes();
    let app = engine(&dir, spawn);
    app.resume_interrupted();
    let _manual = crate::support::next_pipe(&made).await;
    assert_eq!(app.open(&id).unwrap().messages[6].id, queued.id);
    app.shutdown();
}

#[tokio::test]
async fn cut_work_resumes_once_and_off_leaves_a_card() {
    let dir = DataFolder::new();
    let (spawn, made) = pipes();
    let first = engine(&dir, spawn.clone());
    let id = first
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Work".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let _pipe = crate::support::next_pipe(&made).await;
    first.shutdown();
    drop(first);
    let store = faber_engine::store::Store::new(dir.path());
    let original = store.get(&id).unwrap();
    let mut archived = original.clone();
    archived.id = "archived".into();
    archived.archived_at = Some(1);
    store.save(&archived);
    let mut going = original;
    going.id = "keep-going".into();
    going.keep_going = Some(Default::default());
    store.save(&going);
    drop(store);
    let second = engine(&dir, spawn.clone());
    second.save_settings(Settings { resume_interrupted: Some(false), ..Default::default() }).unwrap();
    second.resume_interrupted();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let stopped = second.open(&id).unwrap();
    assert!(stopped.messages.iter().all(|m| m.wake.is_none()));
    assert_eq!(stopped.ended.unwrap().kind, EndKind::Interrupted);
    second.save_settings(Settings { resume_interrupted: Some(true), ..Default::default() }).unwrap();
    second.resume_interrupted();
    crate::support::until("automatic resume", || {
        second
            .open(&id)
            .is_some_and(|c| c.messages.iter().any(|m| m.role == Role::User && m.wake.as_ref().is_some_and(|w| w.why == "wake.resumed")))
    })
    .await;
    let _pipe = crate::support::next_pipe(&made).await;
    assert!(second.open("archived").unwrap().messages.iter().all(|m| m.wake.is_none()));
    assert!(second.open("keep-going").unwrap().messages.iter().all(|m| m.wake.is_none()));
    second.shutdown();
    drop(second);
    let third = engine(&dir, spawn.clone());
    third.resume_interrupted();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let c = third.open(&id).unwrap();
    assert_eq!(c.messages.iter().filter(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.resumed")).count(), 1);
    assert_eq!(c.ended.unwrap().kind, EndKind::Interrupted);
    third.shutdown();
    drop(third);
}

/// A conversation that worked on wakes for a day is one request for the person, but each cut-off turn is its own: a resume
/// Faber sent hours and turns ago must not stop it from going on when the close cuts a later turn off.
#[tokio::test]
async fn a_turn_cut_off_long_after_an_earlier_resume_is_resumed_again() {
    let dir = DataFolder::new();
    let (spawn, made) = pipes();
    let first = engine(&dir, spawn.clone());
    let id = first
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Work".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let _pipe = crate::support::next_pipe(&made).await;
    first.shutdown();
    drop(first);
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    let mut saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let messages = saved["messages"].as_array_mut().unwrap();
    let (user, reply) = (messages[0].clone(), messages[1].clone());
    // The turn cut off, resumed (that resume ended well), then another turn of the same request cut off at the next close.
    let turn = |wake: &str, n: u32, status: &str| {
        let (mut user, mut reply) = (user.clone(), reply.clone());
        user["id"] = format!("user{n}").into();
        user["wake"] = serde_json::json!({ "why": wake });
        reply["id"] = format!("reply{n}").into();
        reply["status"] = status.into();
        [user, reply]
    };
    messages.extend(turn("wake.resumed", 1, "done"));
    messages.extend(turn("wake.asked", 2, "streaming"));
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();
    let second = engine(&dir, spawn);
    second.resume_interrupted();
    crate::support::until("the later cut goes on", || {
        second
            .open(&id)
            .is_some_and(|c| c.messages.iter().filter(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.resumed")).count() == 2)
    })
    .await;
    second.shutdown();
}
