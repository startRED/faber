//! The public send-to-first-text path, isolated from other tests and real model latency.

mod support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Permissions, Role, Settings, Status, Vendor};
use faber_engine::proc::{Proc, Spawner};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedReceiver;

fn backend(spawns: Arc<AtomicUsize>, first_text: Arc<Notify>) -> Spawner {
    // The duplex pipes and protocol replies follow the fake backend in engine/kits.rs.
    Arc::new(move |_, _, _| {
        spawns.fetch_add(1, Ordering::SeqCst);
        let (stdin, incoming) = tokio::io::duplex(65536);
        let (mut outgoing, stdout) = tokio::io::duplex(65536);
        let first_text = first_text.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(incoming).lines();
            let mut turn = 0;
            while let Ok(Some(line)) = lines.next_line().await {
                let msg: Value = serde_json::from_str(&line).unwrap();
                let mut replies = Vec::new();
                if msg["type"] == "user" {
                    replies.push(json!({"type":"system", "subtype":"init", "session_id":"first-word", "model":"test"}));
                    replies.push(json!({"type":"stream_event", "parent_tool_use_id":null,
                        "event":{"type":"content_block_delta", "delta":{"type":"text_delta", "text":"first word"}}}));
                    replies.push(json!({"type":"result", "subtype":"success", "session_id":"first-word",
                        "result":"first word", "is_error":false}));
                }
                let result = match msg["method"].as_str() {
                    Some("initialize") => Some(json!({})),
                    Some("config/read") => Some(json!({"config":{}})),
                    Some("thread/start" | "thread/resume") => Some(json!({"thread":{"id":"first-word", "model":"test"}})),
                    Some("turn/start") => {
                        turn += 1;
                        Some(json!({"turn":{"id":format!("turn-{turn}")}}))
                    }
                    _ => None,
                };
                if let Some(result) = result {
                    replies.push(json!({"id":msg["id"], "result":result}));
                    if msg["method"] == "turn/start" {
                        let id = format!("turn-{turn}");
                        replies.push(json!({"method":"turn/started", "params":{"threadId":"first-word", "turn":{"id":id}}}));
                        replies.push(json!({"method":"item/agentMessage/delta", "params":{
                            "threadId":"first-word", "turnId":id, "itemId":"answer", "delta":"first word"}}));
                        replies.push(json!({"method":"turn/completed", "params":{
                            "threadId":"first-word", "turn":{"id":id, "status":"completed", "items":[]}}}));
                    }
                }
                for reply in replies {
                    // Finishing immediately would flush the text and bypass the engine's streaming publication timer.
                    if reply["type"] == "result" || reply["method"] == "turn/completed" {
                        first_text.notified().await;
                    }
                    if outgoing.write_all(format!("{reply}\n").as_bytes()).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None })
    })
}

async fn sample(
    engine: &Engine,
    events: &mut UnboundedReceiver<(AppEvent, Instant)>,
    vendor: Vendor,
    conversation: &mut Option<String>,
    previous_reply: &mut Option<String>,
    first_text: &Notify,
) -> Duration {
    let request = SendRequest {
        conversation_id: conversation.clone(),
        area: Area::Chat,
        project_id: None,
        text: "Reply with first word".into(),
        images: vec![],
        files: vec![],
        // Explicit choice keeps Auto's model-dependent routing out of the measurement.
        choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
    };
    let started = Instant::now();
    let id = engine.send(request).unwrap();
    *conversation = Some(id.clone());
    tokio::time::timeout(support::DEADLINE, async {
        let mut first = None;
        let mut reply_id = None;
        loop {
            let (event, emitted) = events.recv().await.expect("engine event stream ended");
            if let AppEvent::Message { conversation_id, message } = event {
                if conversation_id != id || message.role != Role::Assistant || previous_reply.as_ref() == Some(&message.id) {
                    continue;
                }
                assert!(message.error.is_none(), "the measured turn failed: {message:?}");
                if !message.text.is_empty() && first.is_none() {
                    assert_eq!(message.text, "first word");
                    assert_eq!(message.status, Some(Status::Streaming), "measure streamed text before the final reply");
                    first = Some(emitted.duration_since(started));
                    reply_id = Some(message.id.clone());
                    first_text.notify_one();
                }
                if reply_id.as_ref() == Some(&message.id) && message.status != Some(Status::Streaming) {
                    assert_eq!(message.status, Some(Status::Done));
                    *previous_reply = Some(message.id);
                    return first.unwrap();
                }
            }
        }
    })
    .await
    .expect("the fake CLI must answer and finish the measured turn")
}

#[tokio::test]
async fn first_word_cost_caps_the_warm_public_send_path() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        let data = support::DataFolder::new();
        let spawns = Arc::new(AtomicUsize::new(0));
        let first_text = Arc::new(Notify::new());
        let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        let engine = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.chat_path().into(),
                clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
                spawn: backend(spawns.clone(), first_text.clone()),
                windows: false,
                claude_env_billed: false,
                max_warm: 1,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: Some(Arc::new(|_| panic!("explicit vendor must not call a routing model"))),
            },
            Arc::new(move |event| drop(tx.send((event, Instant::now())))),
        )
        .unwrap();
        engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() }).unwrap();
        let mut conversation = None;
        let mut previous_reply = None;
        let cold = sample(&engine, &mut events, vendor, &mut conversation, &mut previous_reply, &first_text).await;
        let warmed_spawns = spawns.load(Ordering::SeqCst);
        assert_eq!(warmed_spawns, 1, "the cold turn starts one fake CLI");
        let mut times = Vec::new();
        for _ in 0..20 {
            times.push(sample(&engine, &mut events, vendor, &mut conversation, &mut previous_reply, &first_text).await);
            assert_eq!(spawns.load(Ordering::SeqCst), warmed_spawns, "every warm turn reuses the running fake CLI");
        }
        engine.shutdown();
        times.sort();
        let fastest = times[0];
        let median = (times[9] + times[10]) / 2;
        eprintln!(
            "first_word_cost {vendor:?}: cold_ms={:.3} fastest_ms={:.3} median_ms={:.3} warm_sends=20",
            cold.as_secs_f64() * 1000.0,
            fastest.as_secs_f64() * 1000.0,
            median.as_secs_f64() * 1000.0
        );
        // Prior manual Core-path samples: Claude 8.591 ms, Codex 15.918–16.856 ms (product-map.md).
        // Allow shared-runner contention; a fixed 500 ms delay before send must still fail the 400 ms fastest-sample cap.
        assert!(fastest < Duration::from_millis(400), "{vendor:?}: fastest warm send exceeded 400 ms: {times:?}");
    }
}
