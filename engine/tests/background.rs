use crate::support;
use faber_engine::background::{Command, MAX_BYTES, MAX_COMMANDS, MAX_LINES, State};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Vendor};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

fn engine(path: &crate::support::DataFolder, vendor: Vendor, spawn: faber_engine::proc::Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let options = EngineOptions {
        data_dir: path.path().into(),
        #[cfg(windows)]
        chat_dir: path.chat_path().into(),
        clis: BTreeMap::from([(vendor, Cli { command: "fake".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    (Engine::new(options, Arc::new(move |event| drop(tx.send(event)))).unwrap(), rx)
}

fn request(vendor: &str) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: "Run a background command".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: vendor.into(), model: None, effort: None },
    }
}

async fn rows(rx: &mut UnboundedReceiver<AppEvent>, accept: impl Fn(&[Command]) -> bool) -> Vec<Command> {
    tokio::time::timeout(crate::support::DEADLINE, async {
        while let Some(event) = rx.recv().await {
            if let AppEvent::Background { items, .. } = event
                && accept(&items)
            {
                return items;
            }
        }
        panic!("engine closed before background command update");
    })
    .await
    .expect("no matching background command update")
}

async fn codex_start(input: &mut tokio::io::BufReader<tokio::io::DuplexStream>, output: &mut tokio::io::DuplexStream) {
    let init = support::expect_method(input, "initialize").await;
    support::emit(output, json!({"id":init["id"],"result":{}})).await;
    support::expect_method(input, "initialized").await;
    let thread = support::expect_method(input, "thread/start").await;
    support::emit(output, json!({"id":thread["id"],"result":{"thread":{"id":"background-thread","model":"test"}}})).await;
    let turn = support::expect_method(input, "turn/start").await;
    support::emit(output, json!({"id":turn["id"],"result":{"turn":{"id":"background-turn","status":"inProgress"}}})).await;
    support::emit(output, json!({"method":"turn/started","params":{"threadId":"background-thread","turn":{"id":"background-turn"}}})).await;
}

fn fixture(vendor: &str) -> Vec<Value> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../tests/fixtures/{vendor}-background.jsonl"));
    std::fs::read_to_string(path).unwrap().lines().map(|line| serde_json::from_str::<Value>(line).unwrap()["msg"].clone()).collect()
}

#[tokio::test]
async fn background_codex_recording_starts_prints_finishes_and_never_saves_output() {
    let (spawn, mut input, mut output) = support::controlled();
    let folder = support::DataFolder::new();
    let (engine, mut rx) = engine(&folder, Vendor::Codex, spawn);
    let conversation = engine.send(request("codex")).unwrap();
    codex_start(&mut input, &mut output).await;
    let recorded = fixture("codex");
    support::emit(&mut output, recorded[0].clone()).await;
    let started = rows(&mut rx, |items| items.len() == 1).await;
    assert_eq!(started[0].command, "node long-task.js");
    assert_eq!(started[0].conversation_id, conversation);
    assert_eq!(started[0].state, State::Running);
    assert!(started[0].started > 0);
    assert!(!started[0].stoppable, "Codex's processId is an opaque PTY id, never an OS pid");
    assert_eq!(engine.stop_background(&started[0].id).await, Err("error.backgroundGone"));
    for event in &recorded[1..4] {
        support::emit(&mut output, event.clone()).await;
    }
    let running = rows(&mut rx, |items| items.len() == 1 && items[0].output == "started\nworking\ntransient-only-tail\n").await;
    assert_eq!(running[0].state, State::Running, "a yielded process with no exit code is still running");
    support::emit(&mut output, recorded[4].clone()).await;
    let ended = rows(&mut rx, |items| items.len() == 1 && items[0].state == State::Done).await;
    assert_eq!(ended[0].id, started[0].id);
    assert_eq!(ended[0].output, "started\nworking\nfinished\n");
    support::emit(
        &mut output,
        json!({"method":"turn/completed","params":{"threadId":"background-thread","turn":{"id":"background-turn","status":"completed"}}}),
    )
    .await;
    rows(&mut rx, |items| items.is_empty()).await;
    assert!(engine.background_commands().items.is_empty());
    let saved = std::fs::read_to_string(folder.path().join("conversations").join(format!("{conversation}.json"))).unwrap();
    assert!(!saved.contains("transient-only-tail"), "the streamed output tail is ephemeral");
    assert!(!saved.contains(&started[0].id), "the background row's identity is ephemeral");
    assert!(serde_json::from_str::<Value>(&saved).unwrap().get("background").is_none());
    assert!(engine.stop_background(&started[0].id).await.is_err());
    engine.shutdown();
}

async fn claude_output(
    input: &mut tokio::io::BufReader<tokio::io::DuplexStream>,
    output: &mut tokio::io::DuplexStream,
    task: &str,
    text: &str,
) {
    let request = support::read_message(input).await;
    assert_eq!(request["type"], "control_request");
    assert_eq!(request["request"], json!({"subtype":"get_task_output","task_id":task}));
    support::emit(output, json!({"type":"control_response","response":{"subtype":"success","request_id":request["request_id"],"response":{"output":text,"total_bytes":text.len(),"truncated":false}}})).await;
}

#[tokio::test]
async fn background_claude_recording_reads_stops_without_a_message_and_ends_naturally() {
    let (spawn, mut input, mut output) = support::controlled();
    let folder = support::DataFolder::new();
    let (engine, mut rx) = engine(&folder, Vendor::Claude, spawn);
    let conversation = engine.send(request("claude")).unwrap();
    assert_eq!(support::read_message(&mut input).await["type"], "user");
    support::emit(&mut output, json!({"type":"system","subtype":"init","session_id":"background-session","model":"test"})).await;
    let recorded = fixture("claude");
    for event in &recorded[..2] {
        support::emit(&mut output, event.clone()).await;
    }
    claude_output(&mut input, &mut output, "background-task", "started\n").await;
    let started = rows(&mut rx, |items| items.len() == 1 && items[0].output == "started\n").await;
    assert_eq!(started[0].command, "node long-task.js");
    assert_eq!(started[0].conversation_id, conversation);
    assert!(started[0].stoppable);
    support::emit(&mut output, recorded[2].clone()).await;
    claude_output(&mut input, &mut output, "background-task", "started\nworking\n").await;
    rows(&mut rx, |items| items.len() == 1 && items[0].output == "started\nworking\n").await;
    let stopping = engine.stop_background(&started[0].id);
    let reply = async {
        let stop = support::read_message(&mut input).await;
        assert_eq!(stop["type"], "control_request", "Stop sends no agent message and spends no tokens");
        assert_eq!(stop["request"], json!({"subtype":"stop_task","task_id":"background-task"}));
        support::emit(
            &mut output,
            json!({"type":"control_response","response":{"subtype":"success","request_id":stop["request_id"],"response":{}}}),
        )
        .await;
    };
    let (stopped, ()) = tokio::join!(stopping, reply);
    assert_eq!(stopped, Ok(()));
    assert_eq!(engine.background_commands().items[0].state, State::Running, "only the CLI completion event ends a row");
    support::emit(&mut output, recorded[3].clone()).await;
    claude_output(&mut input, &mut output, "background-task", "started\nworking\nstopped\n").await;
    rows(&mut rx, |items| items.len() == 1 && items[0].state == State::Stopped && items[0].output.ends_with("stopped\n")).await;
    for event in &recorded[4..6] {
        support::emit(&mut output, event.clone()).await;
    }
    claude_output(&mut input, &mut output, "natural-task", "running\n").await;
    rows(&mut rx, |items| items.len() == 2 && items[1].output == "running\n").await;
    support::emit(&mut output, recorded[6].clone()).await;
    claude_output(&mut input, &mut output, "natural-task", "running\nfinished\n").await;
    rows(&mut rx, |items| items.len() == 2 && items[1].state == State::Done && items[1].output.ends_with("finished\n")).await;
    support::emit(&mut output, json!({"type":"result","subtype":"success","is_error":false,"usage":{}})).await;
    rows(&mut rx, |items| items.is_empty()).await;
    let saved = std::fs::read_to_string(folder.path().join("conversations").join(format!("{conversation}.json"))).unwrap();
    assert!(!saved.contains("started\\nworking"));
    engine.shutdown();
}

#[tokio::test]
async fn background_claude_late_task_start_keeps_its_bash_calls_original_turn() {
    for (owed_helper, next_turn) in [(false, false), (false, true), (true, false)] {
        let (spawn, mut input, mut output) = support::controlled();
        let folder = support::DataFolder::new();
        let (engine, mut rx) = engine(&folder, Vendor::Claude, spawn);
        let conversation = engine.send(request("claude")).unwrap();
        assert_eq!(support::read_message(&mut input).await["type"], "user");
        support::emit(&mut output, json!({"type":"system","subtype":"init","session_id":"late-task-session","model":"test"})).await;
        if owed_helper {
            support::emit(&mut output, json!({"type":"system","subtype":"task_started","task_type":"local_agent","task_id":"helper-task","run_id":"helper-run","tool_use_id":"helper-call","is_backgrounded":true})).await;
        }
        let recorded = fixture("claude");
        support::emit(&mut output, recorded[4].clone()).await;
        support::emit(&mut output, json!({"type":"result","subtype":"success","is_error":false,"usage":{}})).await;
        if !owed_helper {
            tokio::time::timeout(crate::support::DEADLINE, async {
                while let Some(event) = rx.recv().await {
                    if let AppEvent::Message { message, .. } = event
                        && message.status == Some(faber_engine::model::Status::Done)
                    {
                        break;
                    }
                }
            })
            .await
            .expect("the Bash call's original turn did not end");
            assert!(engine.background_commands().items.is_empty());
        }
        if next_turn {
            engine.send(SendRequest { conversation_id: Some(conversation), ..request("claude") }).unwrap();
            assert_eq!(support::read_message(&mut input).await["type"], "user");
            let mut current_call = recorded[4].clone();
            current_call["message"]["content"][0]["id"] = "current-call".into();
            support::emit(&mut output, current_call).await;
        }
        support::emit(&mut output, recorded[5].clone()).await;
        claude_output(&mut input, &mut output, "natural-task", "late start\n").await;
        rows(&mut rx, |items| items.len() == 1 && items[0].state == State::Running && items[0].output == "late start\n").await;
        support::emit(&mut output, recorded[6].clone()).await;
        claude_output(&mut input, &mut output, "natural-task", "finished\n").await;
        let ended = rows(&mut rx, |items| items.is_empty() || items[0].state == State::Done).await;
        if owed_helper {
            assert_eq!(ended.len(), 1, "a held helper result has not ended the original reply");
            support::emit(
                &mut output,
                json!({"type":"result","subtype":"success","is_error":false,"origin":{"runId":"helper-run"},"usage":{}}),
            )
            .await;
            rows(&mut rx, |items| items.is_empty()).await;
        } else {
            assert!(ended.is_empty(), "a command started after its original turn ended must leave on completion (next_turn={next_turn})");
        }
        if next_turn {
            let mut start = recorded[5].clone();
            start["task_id"] = "current-task".into();
            start["tool_use_id"] = "current-call".into();
            support::emit(&mut output, start).await;
            claude_output(&mut input, &mut output, "current-task", "current turn\n").await;
            rows(&mut rx, |items| items.len() == 1 && items[0].state == State::Running && items[0].output == "current turn\n").await;
            let mut completed = recorded[6].clone();
            completed["task_id"] = "current-task".into();
            completed["tool_use_id"] = "current-call".into();
            support::emit(&mut output, completed).await;
            claude_output(&mut input, &mut output, "current-task", "current done\n").await;
            rows(&mut rx, |items| items.len() == 1 && items[0].state == State::Done).await;
            support::emit(&mut output, json!({"type":"result","subtype":"success","is_error":false,"usage":{}})).await;
            rows(&mut rx, |items| items.is_empty()).await;
        }
        engine.shutdown();
    }
}

#[tokio::test]
async fn move_chat_to_project_refuses_background_writers_after_the_reply_until_the_command_finishes() {
    let (spawn, mut input, mut output) = support::controlled();
    let folder = support::DataFolder::new();
    let destination = support::work_folder();
    let (engine, mut rx) = engine(&folder, Vendor::Claude, spawn);
    let project = engine.add_project(&destination.path().to_string_lossy()).unwrap();
    let conversation = engine.send(request("claude")).unwrap();
    assert_eq!(support::read_message(&mut input).await["type"], "user");
    support::emit(&mut output, json!({"type":"system","subtype":"init","session_id":"moving-task-session","model":"test"})).await;
    let recorded = fixture("claude");
    support::emit(&mut output, recorded[4].clone()).await;
    support::emit(&mut output, json!({"type":"result","subtype":"success","is_error":false,"usage":{}})).await;
    tokio::time::timeout(crate::support::DEADLINE, async {
        while let Some(event) = rx.recv().await {
            if let AppEvent::Message { message, .. } = event
                && message.status == Some(faber_engine::model::Status::Done)
            {
                return;
            }
        }
        panic!("the original reply never finished");
    })
    .await
    .unwrap();
    let before = engine.open(&conversation).unwrap();
    let store = faber_engine::store::Store::new(folder.path());
    // The task has not announced its row yet, but its background Bash call still owns a writer.
    assert!(engine.background_commands().items.is_empty());
    assert_eq!(engine.move_to_project(&conversation, &project.id), Err("error.moveChat.busy".into()));
    assert_eq!(engine.open(&conversation).unwrap(), before);
    assert_eq!(store.get(&conversation).unwrap(), before);
    support::emit(&mut output, recorded[5].clone()).await;
    claude_output(&mut input, &mut output, "natural-task", "still running\n").await;
    rows(&mut rx, |items| items.len() == 1 && items[0].output == "still running\n").await;
    assert_eq!(engine.move_to_project(&conversation, &project.id), Err("error.moveChat.busy".into()));
    assert_eq!(engine.open(&conversation).unwrap(), before);
    assert_eq!(store.get(&conversation).unwrap(), before);
    // Stop requests cancellation; only the command's completion confirms that it no longer writes.
    engine.interrupt(&conversation);
    let interrupt = support::read_message(&mut input).await;
    assert_eq!(interrupt["request"]["subtype"], "interrupt");
    assert_eq!(engine.move_to_project(&conversation, &project.id), Err("error.moveChat.busy".into()));
    assert_eq!(engine.background_commands().items[0].state, State::Running);
    support::emit(&mut output, recorded[6].clone()).await;
    claude_output(&mut input, &mut output, "natural-task", "finished\n").await;
    rows(&mut rx, |items| items.is_empty()).await;
    let finished = engine.open(&conversation).unwrap();
    let moved = engine.move_to_project(&conversation, &project.id).unwrap();
    assert_eq!(moved.area, Area::Project);
    assert_eq!(moved.messages, finished.messages);
    engine.shutdown();
}

#[tokio::test]
async fn background_claude_caps_output_and_commands_from_real_wire_events() {
    let (spawn, mut input, mut output) = support::controlled();
    let folder = support::DataFolder::new();
    let (engine, mut rx) = engine(&folder, Vendor::Claude, spawn);
    engine.send(request("claude")).unwrap();
    assert_eq!(support::read_message(&mut input).await["type"], "user");
    support::emit(&mut output, json!({"type":"system","subtype":"init","session_id":"cap-session","model":"test"})).await;
    for index in 0..MAX_COMMANDS {
        let id = format!("cap-{index}");
        support::emit(&mut output, json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":id,"name":"Bash","input":{"command":format!("run {index}"),"run_in_background":true}}]}})).await;
        support::emit(
            &mut output,
            json!({"type":"system","subtype":"task_started","task_type":"local_bash","task_id":id,"tool_use_id":id}),
        )
        .await;
        claude_output(&mut input, &mut output, &id, &"界".repeat(MAX_BYTES)).await;
    }
    let full = rows(&mut rx, |items| items.len() == MAX_COMMANDS && items.iter().all(|row| !row.output.is_empty())).await;
    assert!(full.iter().all(|row| row.output.len() <= MAX_BYTES));
    assert!(full.iter().all(|row| row.output.chars().all(|c| c == '界')));
    support::emit(&mut output, json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"overflow","name":"Bash","input":{"command":"overflow","run_in_background":true}}]}})).await;
    support::emit(
        &mut output,
        json!({"type":"system","subtype":"task_started","task_type":"local_bash","task_id":"overflow","tool_use_id":"overflow"}),
    )
    .await;
    support::emit(&mut output, json!({"type":"system","subtype":"task_progress","task_id":"cap-0","tool_use_id":"cap-0"})).await;
    claude_output(&mut input, &mut output, "cap-0", &(0..30).map(|i| format!("line {i}\n")).collect::<String>()).await;
    let bounded = rows(&mut rx, |items| items.first().is_some_and(|row| row.output.ends_with("line 29\n"))).await;
    assert_eq!(bounded.len(), MAX_COMMANDS);
    assert_eq!(bounded[0].output.lines().count(), MAX_LINES);
    assert!(bounded[0].output.starts_with("line 10\n"));
    engine.shutdown();
}

#[tokio::test]
async fn background_claude_refreshes_once_when_progress_arrives_during_an_output_read() {
    let (spawn, mut input, mut output) = support::controlled();
    let folder = support::DataFolder::new();
    let (engine, mut rx) = engine(&folder, Vendor::Claude, spawn);
    engine.send(request("claude")).unwrap();
    assert_eq!(support::read_message(&mut input).await["type"], "user");
    support::emit(&mut output, json!({"type":"system","subtype":"init","session_id":"slow-read-session","model":"test"})).await;
    let recorded = fixture("claude");
    for event in &recorded[..2] {
        support::emit(&mut output, event.clone()).await;
    }
    let first = support::read_message(&mut input).await;
    assert_eq!(first["request"], json!({"subtype":"get_task_output","task_id":"background-task"}));
    for _ in 0..3 {
        support::emit(&mut output, recorded[2].clone()).await;
    }
    support::emit(&mut output, json!({"type":"control_response","response":{"subtype":"success","request_id":first["request_id"],"response":{"output":"older\n"}}})).await;
    let followup = support::read_message(&mut input).await;
    assert_eq!(followup["request"], first["request"]);
    assert_ne!(followup["request_id"], first["request_id"]);
    support::emit(&mut output, recorded[3].clone()).await;
    support::emit(&mut output, json!({"type":"control_response","response":{"subtype":"success","request_id":followup["request_id"],"response":{"output":"newer\n"}}})).await;
    claude_output(&mut input, &mut output, "background-task", "final output\n").await;
    let final_rows = rows(&mut rx, |items| items.len() == 1 && items[0].output == "final output\n").await;
    assert_eq!(final_rows[0].state, State::Stopped);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), support::read_message(&mut input)).await.is_err(),
        "one task edge burst creates at most one follow-up read"
    );
    engine.shutdown();
}

#[tokio::test]
async fn background_codex_caps_output_and_commands_from_real_wire_events() {
    let (spawn, mut input, mut output) = support::controlled();
    let folder = support::DataFolder::new();
    let (engine, mut rx) = engine(&folder, Vendor::Codex, spawn);
    engine.send(request("codex")).unwrap();
    codex_start(&mut input, &mut output).await;
    for index in 0..=MAX_COMMANDS {
        let id = format!("cap-{index}");
        support::emit(&mut output, json!({"method":"item/started","params":{"threadId":"background-thread","turnId":"background-turn","item":{"type":"commandExecution","id":id,"command":format!("run {index}"),"status":"inProgress","processId":index.to_string()}}})).await;
        support::emit(&mut output, json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"background-thread","itemId":id,"delta":"界".repeat(MAX_BYTES)}})).await;
    }
    let full = rows(&mut rx, |items| items.len() == MAX_COMMANDS && items.iter().all(|row| !row.output.is_empty())).await;
    assert_eq!(full.len(), MAX_COMMANDS);
    assert!(full.iter().all(|row| row.output.len() <= MAX_BYTES));
    assert!(full.iter().all(|row| row.output.chars().all(|c| c == '界')));
    let lines = (0..30).map(|i| format!("line {i}\n")).collect::<String>();
    support::emit(
        &mut output,
        json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"background-thread","itemId":"cap-0","delta":lines}}),
    )
    .await;
    let bounded = rows(&mut rx, |items| items.first().is_some_and(|row| row.output.ends_with("line 29\n"))).await;
    assert_eq!(bounded.len(), MAX_COMMANDS);
    assert_eq!(bounded[0].output.lines().count(), MAX_LINES);
    assert!(bounded[0].output.starts_with("line 10\n"));
    engine.shutdown();
}

#[tokio::test]
async fn background_codex_child_command_can_print_and_finish_after_its_parent_turn() {
    for (starts_after_end, next_turn) in [(false, false), (true, false), (true, true)] {
        let (spawn, mut input, mut output) = support::controlled();
        let folder = support::DataFolder::new();
        let (engine, mut rx) = engine(&folder, Vendor::Codex, spawn);
        let conversation = engine.send(request("codex")).unwrap();
        codex_start(&mut input, &mut output).await;
        support::emit(&mut output, json!({"method":"item/started","params":{"threadId":"background-thread","turnId":"background-turn","item":{"type":"subAgentActivity","kind":"started","id":"child-call","agentThreadId":"child-thread"}}})).await;
        support::emit(
            &mut output,
            json!({"method":"turn/started","params":{"threadId":"child-thread","turn":{"id":"child-turn","rootTurnId":"background-turn"}}}),
        )
        .await;
        let parent_end = json!({"method":"turn/completed","params":{"threadId":"background-thread","turn":{"id":"background-turn","status":"completed"}}});
        if starts_after_end {
            support::emit(&mut output, parent_end.clone()).await;
            tokio::time::timeout(crate::support::DEADLINE, async {
                while let Some(event) = rx.recv().await {
                    if let AppEvent::Message { message, .. } = event
                        && message.status == Some(faber_engine::model::Status::Done)
                    {
                        break;
                    }
                }
            })
            .await
            .expect("parent turn did not end");
            assert!(engine.background_commands().items.is_empty());
            if next_turn {
                engine.send(SendRequest { conversation_id: Some(conversation), ..request("codex") }).unwrap();
                let turn = support::expect_method(&mut input, "turn/start").await;
                support::emit(&mut output, json!({"id":turn["id"],"result":{"turn":{"id":"next-turn","status":"inProgress"}}})).await;
                support::emit(
                    &mut output,
                    json!({"method":"turn/started","params":{"threadId":"background-thread","turn":{"id":"next-turn"}}}),
                )
                .await;
            }
        }
        support::emit(&mut output, json!({"method":"item/started","params":{"threadId":"child-thread","turnId":"child-turn","item":{"type":"commandExecution","id":"child-command","command":"node child-task.js","processId":"opaque-child-terminal","status":"inProgress"}}})).await;
        support::emit(&mut output, json!({"method":"item/completed","params":{"threadId":"child-thread","turnId":"child-turn","item":{"type":"commandExecution","id":"child-command","command":"node child-task.js","processId":"opaque-child-terminal","status":"completed","exitCode":null,"aggregatedOutput":"yielded\n"}}})).await;
        let started = rows(&mut rx, |items| items.len() == 1 && items[0].output == "yielded\n").await;
        assert_eq!(started[0].state, State::Running);
        if !starts_after_end {
            support::emit(&mut output, parent_end).await;
        }
        support::emit(&mut output, json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"child-thread","turnId":"child-turn","itemId":"child-command","delta":"after parent\n"}})).await;
        let later = rows(&mut rx, |items| items.len() == 1 && items[0].output.ends_with("after parent\n")).await;
        assert_eq!(later[0].id, started[0].id);
        support::emit(&mut output, json!({"method":"item/completed","params":{"threadId":"child-thread","turnId":"child-turn","item":{"type":"commandExecution","id":"child-command","command":"node child-task.js","processId":"opaque-child-terminal","status":"completed","exitCode":0,"aggregatedOutput":"finished\n"}}})).await;
        rows(&mut rx, |items| items.is_empty()).await;
        assert!(engine.background_commands().items.is_empty());
        engine.shutdown();
    }
}
