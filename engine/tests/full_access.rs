//! Reach follows the permission mode (R587): the engine tells the host whether a conversation's agent acts in Full Access,
//! from the mode it holds at the call, and keeps the line that says a command ran without asking. The host's side is tested
//! in `host/src/tests.rs` (`faber_tool`).
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Permissions, Settings, Status, Vendor};
use faber_engine::{Engine, EngineOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// A Chat on a hand-driven Codex in `mode`, with its first turn under way: the engine, its id, the process and the folder.
async fn chat(mode: Permissions) -> (Engine, String, support::Pipe, support::DataFolder) {
    let dir = support::DataFolder::new();
    let (spawn, made) = support::pipes();
    let app = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
            spawn,
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
    app.save_settings(Settings { permissions: Some(mode), ..Settings::default() }).unwrap();
    let id = app
        .send(faber_engine::SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Tidy my conversations".into(),
            images: vec![],
            files: vec![],
            choice: faber_engine::router::Choice { vendor: "codex".into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&made).await;
    let init = wire(&pipe, |m| m["method"] == "initialize").await;
    pipe.say(json!({"id": init["id"], "result": {}})).await;
    let start = wire(&pipe, |m| m["method"] == "thread/start").await;
    pipe.say(json!({"id": start["id"], "result": {"thread": {"id": "main"}}})).await;
    let turn = wire(&pipe, |m| m["method"] == "turn/start").await;
    pipe.say(json!({"id": turn["id"], "result": {"turn": {"id": "turn-main"}}})).await;
    (app, id, pipe, dir)
}

async fn wire(pipe: &support::Pipe, ready: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(support::DEADLINE, async {
        loop {
            if let Some(m) = pipe.lines.lock().unwrap().iter().find(|m| ready(m)).cloned() {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing line on the fake wire: {:?}", pipe.lines.lock().unwrap()))
}

/// The session's mode is the one the engine holds, read at each call, never anything the agent sends; a conversation with
/// no session running has none. The line of a command run without asking, done or failed, lands on the reply under way.
#[tokio::test]
async fn the_session_mode_is_the_engine_s_and_its_line_lands_on_the_reply_under_way() {
    let (app, id, _pipe, _dir) = chat(Permissions::FullAccess).await;
    assert_eq!(app.live_mode(&id), Some(Permissions::FullAccess));
    assert_eq!(app.live_mode("no-such-conversation"), None, "no session, no mode");
    app.note_ran_unasked(&id, "settings", true);
    app.note_ran_unasked(&id, "rewind", false);
    let reply = app.open(&id).unwrap().messages.last().cloned().unwrap();
    assert_eq!(reply.status, Some(Status::Streaming), "the reply under way");
    assert_eq!(reply.acts.iter().map(|act| act.what.as_str()).collect::<Vec<_>>(), ["act.reach.settings", "act.reach.failed.rewind"]);
    app.shutdown();
    let (app, id, _pipe, _dir) = chat(Permissions::Plan).await;
    assert_eq!(app.live_mode(&id), Some(Permissions::Plan), "a session in Plan is not in Full access");
    app.shutdown();
}
