use crate::support::{DataFolder, pipes};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, EndKind, Role, Settings, Vendor};
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
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
