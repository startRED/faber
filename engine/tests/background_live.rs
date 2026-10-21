//! Synthetic replay of existing parsers only; the UI regressions prove task 019. Monitor timing is unverified.
use crate::support;
use faber_engine::background::State;
use faber_engine::cli::Cli;
use faber_engine::model::{Area, FollowState, Message, Status, Vendor};
use faber_engine::project::remote::{Ci, CiState, Look, PrState};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

async fn message(rx: &mut UnboundedReceiver<AppEvent>, accept: impl Fn(&Message) -> bool) -> Message {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = rx.recv().await {
            if let AppEvent::Message { message, .. } = event
                && accept(&message)
            {
                return *message;
            }
        }
        panic!("the replay ended without its message");
    })
    .await
    .expect("the replay did not deliver its message")
}

#[tokio::test]
async fn background_live_fixture_keeps_launches_open_and_reuses_follow_updates() {
    let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/background-live.json")).unwrap();
    let folder = support::tempdir();
    let (spawn, made) = support::pipes();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let engine = Engine::new(
        EngineOptions {
            data_dir: folder.path().join("data"),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(move |event| drop(tx.send(event))),
    );
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    let id = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "Check the preview and save the change".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&made).await;
    for line in fixture["start"].as_array().unwrap() {
        pipe.say(line.clone()).await;
    }
    let started = message(&mut rx, |m| m.text == fixture["text"].as_str().unwrap()).await;
    assert_eq!(started.status, Some(Status::Streaming));
    let tools = started.tools.as_ref().unwrap();
    for expected in fixture["tools"].as_array().unwrap() {
        let actual = tools.iter().find(|t| t.id == expected["id"].as_str().unwrap()).unwrap();
        assert_eq!(actual.name, expected["name"].as_str().unwrap());
        assert_eq!(actual.input, expected["input"]);
        assert_eq!(actual.ok, expected["ok"].as_bool());
        if let Some(output) = expected["output"].as_str() {
            assert_eq!(actual.output.as_deref(), Some(output));
        }
    }
    let follow = started.follow.unwrap();
    assert_eq!(follow.state, FollowState::Pending);
    assert_eq!(follow.number, fixture["follow"]["number"].as_u64().unwrap());
    assert_eq!(follow.repo, fixture["follow"]["repo"].as_str().unwrap());
    assert_eq!(follow.url, fixture["follow"]["url"].as_str().unwrap());
    let commands = engine.background_commands().items;
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].state, State::Running);
    assert_eq!(commands[0].command, fixture["command"]["command"].as_str().unwrap());
    assert_eq!(commands[0].conversation_id, id);
    assert!(commands[0].stoppable);

    for line in fixture["finish"].as_array().unwrap() {
        pipe.say(line.clone()).await;
    }
    let ended =
        message(&mut rx, |m| m.tools.as_ref().is_some_and(|tools| tools.iter().any(|t| t.id == "monitor" && t.ok == Some(true)))).await;
    let tools = ended.tools.unwrap();
    assert!(tools.iter().any(|t| t.id == "helper" && t.ok == Some(true) && t.output.as_deref() == Some("No findings.")));
    assert_eq!(engine.background_commands().items[0].state, State::Failed);

    // The very same follow reader supplies the hash and terminal state, even while the pane is closed.
    engine.set_pr_reader(Arc::new(|_, _| {
        let head = "abcdef1234567890abcdef1234567890abcdef1234".to_string();
        Ok(Look {
            state: PrState::Merged,
            head: head.clone(),
            ci: Ci { state: CiState::Passing, head, failed: vec![], total: 1 },
            conflict: false,
            failures: vec![],
        })
    }));
    engine.look_at_prs().await;
    let follow = engine.open(&id).unwrap().messages.last().unwrap().follow.clone().unwrap();
    assert_eq!(follow.state, FollowState::Merged);
    assert!(follow.head.unwrap().starts_with("abcdef1234567"));
    pipe.say(fixture["end"].clone()).await;
    message(&mut rx, |m| m.status == Some(Status::Done)).await;
    assert!(engine.background_commands().items.is_empty(), "finished command rows leave at the reply end");
    engine.shutdown();
}
