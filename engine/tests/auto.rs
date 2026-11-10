use crate::support;

use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::model::{Area, Message, Role, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{fake, fixture};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
type Calls = Arc<Mutex<Vec<String>>>;

fn engine(dir: &crate::support::DataFolder, vendor: Vendor, spawn: Spawner, ask: Ask) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let clis = BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]);
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis,
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: Some(ask),
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    for vendor in engine.vendors() {
        engine.note_account_events(
            vendor,
            vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
                billing: Some(faber_engine::budget::Billing::Subscription),
                ..Default::default()
            })],
        );
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Quota { window: "five_hour".into(), used: 0.0, resets_at: now + 18_000.0 }],
    );

    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    (engine, rx)
}

/// A router that always gives this verdict.
fn router(kind: &'static str, level: &'static str, calls: Calls) -> Ask {
    verdicts(vec![(kind, level)], calls)
}

/// A router that gives these verdicts in turn, then keeps the last one.
fn verdicts(list: Vec<(&'static str, &'static str)>, calls: Calls) -> Ask {
    Arc::new(move |prompt: String| {
        let mut calls = calls.lock().unwrap();
        let (kind, level) = list[calls.len().min(list.len() - 1)];
        calls.push(prompt);
        Box::pin(async move { Some(format!(r#"{{"kind":"{kind}","difficulty":"{level}","confidence":0.95}}"#)) })
    })
}

/// The answer to a conversation's `n`th message once its model is known.
async fn routed(engine: &Engine, id: &str, n: usize) -> Message {
    let mut found = None;
    support::until(&format!("message {n} was never routed"), || {
        found = engine.open(id).and_then(|c| c.messages.get(n * 2 + 1).cloned()).filter(|m| m.model.is_some());
        found.is_some()
    })
    .await;
    found.unwrap()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).map(|i| args[i + 1].clone())
}

fn auto(text: &str) -> SendRequest {
    let choice = Choice { vendor: "auto".into(), model: None, effort: None };
    SendRequest { conversation_id: None, area: Area::Chat, project_id: None, text: text.into(), images: vec![], files: vec![], choice }
}

async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> Message {
    loop {
        let e = tokio::time::timeout(crate::support::DEADLINE, rx.recv()).await.expect("no finished answer in time").expect("engine gone");
        if let AppEvent::Message { message, .. } = e
            && message.role == Role::Assistant
            && message.status != Some(Status::Streaming)
        {
            return *message;
        }
    }
}

#[test]
fn agent_questions_and_child_results_explain_why_they_were_sent() {
    for (wake, reason) in [("wake.asked", "act.why.asked"), ("wake.childDone", "act.why.childDone")] {
        let act = faber_engine::act::FaberAct::wake(&faber_engine::model::Wake::of(wake), "sent");
        assert_eq!(act.why.as_deref(), Some(reason));
    }
}

#[tokio::test]
async fn auto_routes_the_first_message_blind_and_says_why() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, router("chat", "easy", calls.clone()));
    let id = engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    engine.shutdown();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("conversations").join(format!("{id}.json"))).unwrap()).unwrap();
    assert_eq!(saved["messages"][1]["acts"], serde_json::to_value(&reply).unwrap()["acts"]);
    assert_eq!(reply.model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(reply.reason.as_deref(), Some("reason.route.chat.easy"));
    let act = serde_json::to_value(&reply).unwrap()["acts"][0].clone();
    assert_eq!(act["kind"], "route");
    assert_eq!(act["params"]["model"], "gpt-6-luna");
    assert_eq!(act["why"], "reason.route.chat.easy");
    let prompt = calls.lock().unwrap()[0].to_lowercase();
    assert!(prompt.contains(&SKY.to_lowercase()) && !prompt.contains("luna") && !prompt.contains("gpt"));
}

#[tokio::test]
async fn a_slow_router_starts_the_turn_with_a_reason_within_the_budget() {
    let dir = crate::support::DataFolder::new();
    let slow: Ask = Arc::new(|_| {
        Box::pin(async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            None
        })
    });
    let f = fake(fixture("codex", "chat"));
    let (engine, _rx) = engine(&dir, Vendor::Codex, f.spawn.clone(), slow);
    let id = engine.send(auto(SKY)).unwrap();
    tokio::time::timeout(Duration::from_millis(3500), async {
        loop {
            if !f.args.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("turn held by router");
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!(reply.acts[0].why.as_deref(), Some("act.why.routerTimeout"));
    engine.shutdown();
}

#[tokio::test]
async fn a_failed_router_keeps_the_previous_choice() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let seen = calls.clone();
    let ask: Ask = Arc::new(move |prompt| {
        let mut calls = seen.lock().unwrap();
        calls.push(prompt);
        let first = calls.len() == 1;
        Box::pin(async move { first.then(|| r#"{"kind":"chat","difficulty":"easy","confidence":0.95}"#.to_string()) })
    });
    let (engine, mut rx) = engine(&dir, Vendor::Claude, fake(fixture("claude", "chat")).spawn, ask);
    let id = engine.send(auto(SKY)).unwrap();
    let first = finished(&mut rx).await;
    draft(&engine, Some(&id), "A harder follow-up").await;
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("A harder follow-up") }).unwrap();
    let second = routed(&engine, &id, 1).await;
    assert_eq!(second.model, first.model);
    assert_eq!(second.acts[0].why.as_deref(), Some("act.why.routerTimeout"));
    engine.shutdown();
}

#[tokio::test]
async fn a_router_without_an_answer_leaves_the_everyday_default() {
    let dir = crate::support::DataFolder::new();
    let silent: Ask = Arc::new(|_| Box::pin(async { None }));
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, silent);
    let models = vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()];
    engine.set_codex_models(models.clone(), None);
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    engine.shutdown();
    assert_eq!(reply.status, Some(Status::Done));
    assert_eq!(reply.reason.as_deref(), Some("reason.autoOnly"));
    assert_eq!(reply.acts[0].why.as_deref(), Some("act.why.routerTimeout"));
    assert_eq!(reply.acts[0].params.get("model"), Some(&faber_engine::router::default_model(Vendor::Codex, None, &models)));
}

#[tokio::test]
async fn one_usable_profile_skips_router_and_uses_that_profile() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, router("chat", "easy", calls.clone()));
    engine.set_codex_models(vec!["gpt-6-luna".into()], Some("gpt-6-luna".into()));
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    assert!(calls.lock().unwrap().is_empty(), "one model/effort profile needs no classification");
    assert_eq!(reply.model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(reply.acts[0].why.as_deref(), Some("reason.autoSingleProfile"));
    assert!(engine.ledger().codex.unmetered.is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn no_router_candidates_keep_the_default_reason() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, router("chat", "easy", calls.clone()));
    engine.set_codex_models(vec![], None);
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(reply.acts[0].why.as_deref(), Some("reason.autoOnly"));
    engine.shutdown();
}

#[tokio::test]
async fn a_draft_routed_while_typing_answers_without_asking_again() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, router("chat", "easy", calls.clone()));
    let typing = WarmRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "auto".into(), model: None, effort: None },
    };
    // Most of the message: the route still holds once the last words are typed.
    engine.route_draft(typing, SKY[..30].into());
    tokio::time::sleep(Duration::from_millis(200)).await;
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    engine.shutdown();
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(reply.reason.as_deref(), Some("reason.route.chat.easy"));
}

#[tokio::test]
async fn once_a_message_needs_more_than_an_easy_model_the_conversation_stays_with_it() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(&dir, Vendor::Claude, f.spawn.clone(), router("writing", "medium", calls.clone()));
    let id = engine.send(auto(SKY)).unwrap();
    let first = finished(&mut rx).await;
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("And at sunset?") }).unwrap();
    let second = engine.open(&id).unwrap().messages[3].clone();
    engine.shutdown();
    assert_eq!(calls.lock().unwrap().len(), 1, "only the first message is routed");
    let args = f.args.lock().unwrap()[0].clone();
    assert_eq!((flag(&args, "--model"), flag(&args, "--effort")), (Some("claude-sonnet-5-5".into()), Some("medium".into())));
    assert_eq!((second.vendor, second.model, second.reason), (Some(Vendor::Claude), first.model, first.reason));
}

#[tokio::test]
async fn a_conversation_that_began_with_a_greeting_moves_up_when_the_work_does() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let f = fake(fixture("claude", "chat"));
    let ask = verdicts(vec![("chat", "easy"), ("code", "hard")], calls.clone());
    let (engine, mut rx) = engine(&dir, Vendor::Claude, f.spawn.clone(), ask);
    let id = engine.send(auto(SKY)).unwrap();
    finished(&mut rx).await;
    draft(&engine, Some(&id), "Now refactor my billing service").await;
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("Now refactor my billing service") }).unwrap();
    let second = routed(&engine, &id, 1).await;
    engine.shutdown();
    assert_eq!((second.model.as_deref(), second.reason.as_deref()), (Some("claude-sonnet-5-5"), Some("reason.route.code.hard")));
    let args = f.args.lock().unwrap().clone();
    assert_eq!(flag(&args[0], "--model").as_deref(), Some("claude-haiku-5-5"));
    // The same subscription picks the conversation up where it was, with its history.
    assert_eq!((flag(&args[1], "--model").as_deref(), flag(&args[1], "--effort").as_deref()), (Some("claude-sonnet-5-5"), Some("high")));
    assert!(flag(&args[1], "--resume").is_some());
}

#[tokio::test]
async fn with_only_claude_the_router_process_starts_while_the_user_types() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("claude", "chat"));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let clis = BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]);
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis,
        spawn: f.spawn.clone(),
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    for vendor in engine.vendors() {
        engine.note_account_events(
            vendor,
            vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
                billing: Some(faber_engine::budget::Billing::Subscription),
                ..Default::default()
            })],
        );
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Quota { window: "five_hour".into(), used: 0.0, resets_at: now + 18_000.0 }],
    );

    let routers = || f.args.lock().unwrap().iter().filter(|a| a.contains(&"--system-prompt".to_string())).count();
    engine.warm(WarmRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "auto".into(), model: None, effort: None },
    });
    assert_eq!(routers(), 1, "a cold Claude takes about 9 s, so the router starts at the first keystroke");
    // R124: the router is Faber's own request. It runs without the person's hooks, settings files and MCP servers.
    let router = f.args.lock().unwrap().iter().find(|a| a.contains(&"--system-prompt".to_string())).cloned().unwrap();
    crate::support::assert_no_user_customizations(&router);
    engine.send(auto(SKY)).unwrap();
    engine.shutdown();
    assert_eq!(routers(), 1, "the question goes to the router that was already waiting");
}

#[tokio::test]
async fn stop_while_the_route_is_coming_stops_the_answer() {
    let dir = crate::support::DataFolder::new();
    let never: Ask = Arc::new(|_| Box::pin(std::future::pending()));
    let (engine, _rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, never);
    let id = engine.send(auto(SKY)).unwrap();
    assert_eq!(engine.open(&id).unwrap().messages[1].reason.as_deref(), Some("reason.routing"));
    engine.interrupt(&id);
    let reply = engine.open(&id).unwrap().messages[1].clone();
    engine.shutdown();
    assert_eq!(reply.status, Some(Status::Stopped));
}

/// R121: the router has no tools and reads nothing, and its Codex process is the isolated one in every mode, apart from the
/// process a conversation gets for the mode the person chose.
#[tokio::test]
async fn r121_the_codex_router_starts_its_own_isolated_process_while_conversations_follow_the_chosen_mode() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("codex", "chat"));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
        spawn: f.spawn.clone(),
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    engine.warm(WarmRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "auto".into(), model: None, effort: None },
    });
    let isolated = faber_engine::codex::codex_args(Area::Chat, false, false, faber_engine::model::Permissions::Isolated);
    assert_eq!(f.args.lock().unwrap().as_slice(), &[isolated], "the router starts the isolated process, in the default mode");
    engine.warm(WarmRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "codex".into(), model: None, effort: None },
    });
    let args = f.args.lock().unwrap().clone();
    assert_eq!(args.len(), 2);
    assert_eq!(args[1], ["app-server"], "a conversation in the default mode, full access, gets the plain process");
    assert_ne!(args[0], args[1]);
    engine.shutdown();
}

/// Task 3 of the model rules: a draft is routed at most twice while the person types it, and sending starts the count again.
#[tokio::test]
async fn a_draft_is_routed_at_most_twice_while_typing_and_sending_starts_the_count_again() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, router("chat", "easy", calls.clone()));
    let typing = |id: Option<&str>| WarmRequest {
        conversation_id: id.map(String::from),
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "auto".into(), model: None, effort: None },
    };
    // Four pauses in the same message, each with other words: the router hears the first two only.
    for words in ["Why", "Why is the", "Why is the sky", "Why is the sky blue at noon"] {
        engine.route_draft(typing(None), words.into());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(calls.lock().unwrap().len(), 2, "two routes for one message while typing");
    let id = engine.send(auto(SKY)).unwrap();
    finished(&mut rx).await;
    let sent = calls.lock().unwrap().len();
    // The next message of the same conversation (still easy) is a new message: two more routes.
    for words in ["And", "And at", "And at sunset", "And at sunset, then"] {
        engine.route_draft(typing(Some(&id)), words.into());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    engine.shutdown();
    assert_eq!(calls.lock().unwrap().len() - sent, 2, "sending reset the count");
}

/// Rule 3: without a finished draft verdict, Auto keeps the model, effort and live process.
#[tokio::test]
async fn a_second_message_without_a_verdict_keeps_the_process_model_and_effort() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let f = fake(support::claude_two_turns("req"));
    let (engine, _rx) = engine(&dir, Vendor::Claude, f.spawn.clone(), router("code", "medium", calls.clone()));
    // Each turn of the recording asks to write a file: answer it, then wait for the reply to end.
    let turn = |engine: &Engine, id: String, reply: usize| {
        let engine = engine.clone();
        async move {
            // Each pass answers a permission card (an await `support::until` cannot hold); real time bounds it.
            let start = std::time::Instant::now();
            while start.elapsed() < crate::support::DEADLINE {
                if let Some(m) = engine.open(&id).and_then(|c| c.messages.get(reply).cloned()) {
                    if let Some(p) = m.permissions.as_ref().and_then(|p| p.first()) {
                        let _ = engine.answer(&id, &m.id, &p.request_id, true, false).await;
                    }
                    if m.status == Some(Status::Done) {
                        return m;
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            engine.open(&id).unwrap().messages[reply].clone()
        }
    };
    let id = engine.send(auto("first")).unwrap();
    let one = turn(&engine, id.clone(), 1).await;
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("second") }).unwrap();
    let two = turn(&engine, id.clone(), 3).await;
    assert_eq!(f.mismatches.lock().unwrap().clone(), Vec::<String>::new(), "args {:?}", f.args.lock().unwrap());
    assert_eq!(two.status, Some(Status::Done), "args {:?}", f.args.lock().unwrap());
    engine.shutdown();
    let launches = f.args.lock().unwrap().clone();
    assert_eq!(launches.len(), 1, "one process served both messages: nothing was started again, so no flag changed");
    assert_eq!(calls.lock().unwrap().len(), 1, "only the first message was routed");
    assert_eq!((one.model.clone(), one.effort.clone()), (two.model.clone(), two.effort.clone()));
    assert!(flag(&launches[0], "--model").is_some() && flag(&launches[0], "--effort").is_some());
}

/// Wait for an injected draft verdict to settle, without adding any wait to the send path.
async fn draft(engine: &Engine, id: Option<&str>, text: &str) {
    engine.route_draft(
        WarmRequest { conversation_id: id.map(str::to_owned), area: Area::Chat, project_id: None, choice: auto("").choice },
        text.into(),
    );
    tokio::time::sleep(Duration::from_millis(80)).await;
}

/// A protocol fake that accepts arbitrary turns and resumes, recording the actual CLI boundary.
struct EffortCli {
    spawn: Spawner,
    args: Arc<Mutex<Vec<Vec<String>>>>,
    sent: Arc<Mutex<Vec<serde_json::Value>>>,
    release: Arc<tokio::sync::Notify>,
}

impl EffortCli {
    fn new(hold_first: bool) -> Self {
        use faber_engine::proc::Proc;
        use serde_json::{Value, json};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let args: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
        let sent: Arc<Mutex<Vec<Value>>> = Arc::default();
        let release = Arc::new(tokio::sync::Notify::new());
        let hold = Arc::new(std::sync::atomic::AtomicBool::new(hold_first));
        let (launches, wire, gate) = (args.clone(), sent.clone(), release.clone());
        let spawn: Spawner = Arc::new(move |_, argv, _| {
            launches.lock().unwrap().push(argv.to_vec());
            let model = flag(argv, "--model").unwrap_or_default();
            let (stdin, incoming) = tokio::io::duplex(65536);
            let (mut outgoing, stdout) = tokio::io::duplex(65536);
            let (wire, gate, hold) = (wire.clone(), gate.clone(), hold.clone());
            tokio::spawn(async move {
                let mut lines = BufReader::new(incoming).lines();
                let mut model = model;
                let mut thread = String::new();
                let mut count = 0;
                while let Ok(Some(line)) = lines.next_line().await {
                    let msg: Value = serde_json::from_str(&line).unwrap();
                    wire.lock().unwrap().push(msg.clone());
                    let mut replies = vec![];
                    if msg["type"] == "user" {
                        count += 1;
                        replies.push(json!({"type":"system", "subtype":"init", "session_id":"effort-session", "model":model}));
                        replies.push(json!({"type":"stream_event", "parent_tool_use_id":null,
                            "event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"answer"}}}));
                        replies.push(json!({"type":"result", "subtype":"success", "session_id":"effort-session",
                            "result":"answer", "is_error":false}));
                    }
                    let result = match msg["method"].as_str() {
                        Some("initialize") => Some(json!({})),
                        Some("config/read") => Some(json!({"config":{}})),
                        Some("thread/start" | "thread/resume") => {
                            model = msg["params"]["model"].as_str().unwrap_or(&model).to_owned();
                            thread = msg["params"]["threadId"].as_str().unwrap_or("effort-thread").to_owned();
                            Some(json!({"thread":{"id":thread,"model":model}}))
                        }
                        Some("turn/start") => {
                            count += 1;
                            Some(json!({"turn":{"id":format!("turn-{count}")}}))
                        }
                        Some("turn/steer") => Some(json!({"turnId":format!("turn-{count}")})),
                        _ => None,
                    };
                    if let Some(result) = result {
                        replies.push(json!({"id":msg["id"],"result":result}));
                        if msg["method"] == "turn/start" {
                            let turn = format!("turn-{count}");
                            replies.push(json!({"method":"turn/started","params":{"threadId":thread,"turn":{"id":turn}}}));
                            replies.push(json!({"method":"item/agentMessage/delta","params":{
                                "threadId":thread,"turnId":turn,"itemId":"answer","delta":"answer"}}));
                            replies.push(json!({"method":"turn/completed","params":{
                                "threadId":thread,"turn":{"id":turn,"status":"completed","items":[]}}}));
                        }
                    }
                    for reply in replies {
                        if count == 1
                            && (reply["type"] == "result" || reply["method"] == "turn/completed")
                            && hold.swap(false, std::sync::atomic::Ordering::SeqCst)
                        {
                            gate.notified().await;
                        }
                        if outgoing.write_all(format!("{reply}\n").as_bytes()).await.is_err() {
                            return;
                        }
                    }
                }
            });
            Ok(Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None })
        });
        Self { spawn, args, sent, release }
    }

    fn efforts(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["method"] == "turn/start")
            .map(|v| v["params"]["effort"].as_str().unwrap().into())
            .collect()
    }
}

fn json_router(verdict: Arc<Mutex<Option<String>>>) -> Ask {
    Arc::new(move |_| {
        let value = verdict.lock().unwrap().clone();
        Box::pin(async move { value })
    })
}

fn set_verdict(verdict: &Mutex<Option<String>>, kind: &str, difficulty: &str, check: &str, scope: &str, confidence: f64) {
    *verdict.lock().unwrap() =
        Some(serde_json::json!({"kind":kind,"difficulty":difficulty,"check":check,"scope":scope,"confidence":confidence}).to_string());
}

async fn next_effort(engine: &Engine, rx: &mut UnboundedReceiver<AppEvent>, id: &str, text: &str) -> Message {
    draft(engine, Some(id), text).await;
    engine.send(SendRequest { conversation_id: Some(id.into()), ..auto(text) }).unwrap();
    let answer = finished(rx).await;
    assert_eq!(answer.status, Some(Status::Done), "{answer:?}");
    answer
}

async fn first_word(rx: &mut UnboundedReceiver<AppEvent>, start: std::time::Instant) -> (Message, Duration) {
    let mut first = None;
    loop {
        let event = tokio::time::timeout(support::DEADLINE, rx.recv()).await.unwrap().unwrap();
        if let AppEvent::Message { message, .. } = event
            && message.role == Role::Assistant
        {
            if !message.text.is_empty() {
                first.get_or_insert_with(|| start.elapsed());
            }
            if message.status == Some(Status::Done) {
                return (*message, first.expect("fake answer had no visible text"));
            }
        }
    }
}

#[tokio::test]
async fn auto_effort_ten_mixed_messages_keep_model_vendor_and_deliver_each_effort_to_codex() {
    let dir = support::DataFolder::new();
    let cli = EffortCli::new(false);
    let verdict = Arc::new(Mutex::new(None));
    set_verdict(&verdict, "code", "hard", "machine", "open", 0.95);
    let (engine, mut rx) = engine(&dir, Vendor::Codex, cli.spawn.clone(), json_router(verdict.clone()));
    let id = engine.send(auto("hard first")).unwrap();
    let first = finished(&mut rx).await;
    assert_eq!((first.model.as_deref(), first.effort.as_deref()), (Some("gpt-6.1-sol"), Some("high")));
    assert!(!first.reason.as_deref().unwrap().starts_with("reason.autoEffort"));
    let cases = [
        ("chat", "easy", "glance", "tight", 0.95, "low", Some("reason.autoEffortLowered")),
        ("math", "medium", "machine", "tight", 0.95, "medium", Some("reason.autoEffortRaised")),
        ("code", "hard", "machine", "open", 0.95, "high", Some("reason.autoEffortRaised")),
        ("chat", "easy", "blind", "tight", 0.95, "high", None),
        ("chat", "easy", "glance", "tight", 0.69, "high", None),
        ("chat", "easy", "glance", "tight", 0.7, "low", Some("reason.autoEffortLowered")),
        ("code", "hard", "machine", "open", 0.95, "high", Some("reason.autoEffortRaised")),
        ("chat", "easy", "glance", "tight", 0.95, "low", Some("reason.autoEffortLowered")),
        ("chat", "easy", "glance", "tight", 0.95, "low", None),
    ];
    for (i, (kind, level, check, scope, confidence, effort, reason)) in cases.into_iter().enumerate() {
        set_verdict(&verdict, kind, level, check, scope, confidence);
        let reply = next_effort(&engine, &mut rx, &id, &format!("follow-up {i}")).await;
        assert_eq!((reply.vendor, reply.model.as_ref()), (first.vendor, first.model.as_ref()));
        assert_eq!(reply.effort.as_deref(), Some(effort), "message {i}: {reply:?}");
        assert_eq!(reply.reason.as_deref().filter(|r| r.starts_with("reason.autoEffort")), reason);
        let act = reply.acts.last().unwrap();
        assert!(act.change.is_none(), "effort selection adds no picker control");
        for (key, value) in [("kind", kind), ("difficulty", level), ("scope", scope), ("check", check)] {
            assert_eq!(act.params.get(key).map(String::as_str), Some(value));
        }
        assert_eq!(cli.efforts().last().map(String::as_str), Some(effort));
    }
    assert_eq!(cli.efforts().len(), 10);
    assert_eq!(cli.args.lock().unwrap().len(), 1, "effort changes reuse the Codex app-server process");
    engine.shutdown();
}

#[tokio::test]
async fn auto_effort_claude_reductions_need_two_messages_and_resume_but_raises_are_immediate() {
    let dir = support::DataFolder::new();
    let cli = EffortCli::new(false);
    let verdict = Arc::new(Mutex::new(None));
    set_verdict(&verdict, "code", "hard", "machine", "tight", 0.95);
    let (engine, mut rx) = engine(&dir, Vendor::Claude, cli.spawn.clone(), json_router(verdict.clone()));
    let start = std::time::Instant::now();
    let id = engine.send(auto("hard first")).unwrap();
    let (first, cold) = first_word(&mut rx, start).await;
    assert_eq!((first.model.as_deref(), first.effort.as_deref()), (Some("claude-sonnet-5-5"), Some("high")));
    set_verdict(&verdict, "chat", "easy", "glance", "tight", 0.95);
    draft(&engine, Some(&id), "first easy").await;
    let start = std::time::Instant::now();
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("first easy") }).unwrap();
    let (held, warm) = first_word(&mut rx, start).await;
    assert_eq!(held.effort.as_deref(), Some("high"));
    assert!(!held.reason.unwrap().starts_with("reason.autoEffort"));
    assert_eq!(cli.args.lock().unwrap().len(), 1);
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("gap without a verdict") }).unwrap();
    assert_eq!(finished(&mut rx).await.effort.as_deref(), Some("high"));
    let held = next_effort(&engine, &mut rx, &id, "easy after the gap").await;
    assert_eq!(held.effort.as_deref(), Some("high"), "a gap breaks consecutive reductions");
    for (text, check, confidence) in [("blind easy", "blind", 0.95), ("unsure easy", "glance", 0.69)] {
        set_verdict(&verdict, "chat", "easy", check, "tight", confidence);
        let held = next_effort(&engine, &mut rx, &id, text).await;
        assert_eq!(held.effort.as_deref(), Some("high"));
    }
    set_verdict(&verdict, "chat", "easy", "glance", "tight", 0.95);
    assert_eq!(next_effort(&engine, &mut rx, &id, "easy after the veto").await.effort.as_deref(), Some("high"));
    draft(&engine, Some(&id), "second easy").await;
    let start = std::time::Instant::now();
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("second easy") }).unwrap();
    let (lower, resumed) = first_word(&mut rx, start).await;
    assert_eq!(lower.effort.as_deref(), Some("medium"));
    assert_eq!(lower.reason.as_deref(), Some("reason.autoEffortLowered"));
    set_verdict(&verdict, "code", "hard", "machine", "tight", 0.95);
    let raised = next_effort(&engine, &mut rx, &id, "hard again").await;
    assert_eq!(raised.effort.as_deref(), Some("high"));
    assert_eq!(raised.reason.as_deref(), Some("reason.autoEffortRaised"));
    assert_eq!(raised.model, first.model);
    assert_eq!(next_effort(&engine, &mut rx, &id, "still hard").await.reason.as_deref(), Some("reason.route.code.hard"));
    let messages = engine.open(&id).unwrap().messages;
    let answers: Vec<_> = messages.iter().filter(|m| m.role == Role::Assistant).collect();
    assert_eq!(answers.len(), 10);
    assert!(answers.iter().all(|m| m.vendor == first.vendor && m.model == first.model));
    let launches = cli.args.lock().unwrap().clone();
    assert_eq!(launches.len(), 3);
    assert_eq!(flag(&launches[1], "--effort").as_deref(), Some("medium"));
    assert_eq!(flag(&launches[2], "--effort").as_deref(), Some("high"));
    for args in &launches[1..] {
        assert_eq!(flag(args, "--model"), first.model);
        assert_eq!(flag(args, "--resume").as_deref(), Some("effort-session"));
    }
    eprintln!(
        "auto_effort fake Claude send-to-first-visible-text: cold_ms={:.3}, warm_ms={:.3}, resumed_ms={:.3}; real CLI startup/inference excluded",
        cold.as_secs_f64() * 1000.0,
        warm.as_secs_f64() * 1000.0,
        resumed.as_secs_f64() * 1000.0
    );
    engine.shutdown();
}

#[tokio::test]
async fn auto_effort_no_verdict_and_hand_picked_effort_are_untouched() {
    let dir = support::DataFolder::new();
    let cli = EffortCli::new(false);
    let verdict = Arc::new(Mutex::new(None));
    set_verdict(&verdict, "math", "medium", "machine", "tight", 0.95);
    let (engine, mut rx) = engine(&dir, Vendor::Codex, cli.spawn.clone(), json_router(verdict.clone()));
    let id = engine.send(auto("medium first")).unwrap();
    let first = finished(&mut rx).await;
    assert_eq!(first.effort.as_deref(), Some("medium"));
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("no ready verdict") }).unwrap();
    let unchanged = finished(&mut rx).await;
    assert_eq!(unchanged.effort, first.effort);
    assert!(!unchanged.reason.unwrap().starts_with("reason.autoEffort"));
    set_verdict(&verdict, "code", "hard", "machine", "open", 0.95);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    engine.note_account_events(
        Vendor::Codex,
        vec![faber_engine::events::EngineEvent::Quota { window: "five_hour".into(), used: 0.75, resets_at: now + 18_000.0 }],
    );
    let higher = next_effort(&engine, &mut rx, &id, "hard follow-up").await;
    assert_eq!(higher.effort.as_deref(), Some("high"));
    assert_eq!(higher.model, first.model);
    // A previously classified easy draft cannot override a manual picker choice.
    set_verdict(&verdict, "chat", "easy", "glance", "tight", 0.95);
    draft(&engine, Some(&id), "manual now").await;
    let choice = Choice { vendor: "codex".into(), model: first.model.clone(), effort: Some(faber_engine::router::Effort::High) };
    engine.send(SendRequest { conversation_id: Some(id.clone()), choice: choice.clone(), ..auto("manual now") }).unwrap();
    assert_eq!(finished(&mut rx).await.effort.as_deref(), Some("high"));
    engine.route_draft(
        WarmRequest { conversation_id: Some(id.clone()), area: Area::Chat, project_id: None, choice: choice.clone() },
        "still manual".into(),
    );
    engine.send(SendRequest { conversation_id: Some(id.clone()), choice, ..auto("still manual") }).unwrap();
    let manual = finished(&mut rx).await;
    assert_eq!((manual.model, manual.effort, manual.reason), (first.model, Some("high".into()), Some("reason.chosen".into())));
    engine.shutdown();
}

#[tokio::test]
async fn auto_effort_model_below_the_bar_offers_a_stronger_branch_once() {
    let dir = support::DataFolder::new();
    let cli = EffortCli::new(false);
    let verdict = Arc::new(Mutex::new(None));
    set_verdict(&verdict, "writing", "medium", "glance", "tight", 0.95);
    let (engine, mut rx) = engine(&dir, Vendor::Claude, cli.spawn.clone(), json_router(verdict.clone()));
    let id = engine.send(auto("medium first")).unwrap();
    let first = finished(&mut rx).await;
    assert_eq!(first.model.as_deref(), Some("claude-sonnet-5-5"));
    set_verdict(&verdict, "code", "hard", "blind", "open", 0.95);
    let reply = next_effort(&engine, &mut rx, &id, "frontier follow-up").await;
    assert_eq!((reply.model, reply.effort), (first.model, Some("high".into())));
    let offer = reply.nudge.expect("a stronger branch must be offered");
    assert_eq!(offer.reason, "nudge.modelBar");
    assert_eq!(offer.vendor, Vendor::Claude);
    assert_ne!(offer.model, "claude-sonnet-5-5");
    for text in ["frontier again", "frontier once more"] {
        let reply = next_effort(&engine, &mut rx, &id, text).await;
        assert_eq!((reply.model.as_deref(), reply.effort.as_deref()), (Some("claude-sonnet-5-5"), Some("high")));
        assert!(reply.nudge.is_none(), "an ignored offer must not repeat");
        assert!(reply.acts.iter().any(|a| a.params.get("modelBar").is_some_and(|v| v == "unmet")), "route metadata stays");
    }
    engine.shutdown();
}

#[tokio::test]
async fn auto_effort_queued_and_steered_messages_never_change_the_running_answer() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        for steer in [false, true] {
            let dir = support::DataFolder::new();
            let cli = EffortCli::new(true);
            let verdict = Arc::new(Mutex::new(None));
            set_verdict(&verdict, "math", "medium", "machine", "tight", 0.95);
            let (engine, _rx) = engine(&dir, vendor, cli.spawn.clone(), json_router(verdict.clone()));
            let id = engine.send(auto("medium running")).unwrap();
            support::until("the first fake answer did not start", || engine.open(&id).is_some_and(|c| !c.messages[1].text.is_empty()))
                .await;
            let before = engine.open(&id).unwrap().messages[1].clone();
            set_verdict(&verdict, "code", "hard", "blind", "open", 0.95);
            draft(&engine, Some(&id), "hard while running").await;
            engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("hard while running") }).unwrap();
            let c = engine.open(&id).unwrap();
            assert_eq!(c.queued.len(), 1);
            if steer {
                engine.send_queued(&id, &c.queued[0].id).unwrap();
            }
            let after = engine.open(&id).unwrap().messages[1].clone();
            assert_eq!(
                (after.vendor, after.model, after.effort, after.reason),
                (before.vendor, before.model, before.effort, before.reason)
            );
            assert_eq!(cli.args.lock().unwrap().len(), 1, "a queued/steered message must not restart the running answer");
            if vendor == Vendor::Codex {
                assert_eq!(cli.efforts().len(), 1);
            }
            engine.shutdown();
            cli.release.notify_one();
        }
    }
}

#[tokio::test]
async fn auto_effort_queued_messages_keep_their_own_finished_verdicts() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        let dir = support::DataFolder::new();
        let cli = EffortCli::new(true);
        let verdict = Arc::new(Mutex::new(None));
        set_verdict(&verdict, if vendor == Vendor::Codex { "math" } else { "writing" }, "medium", "machine", "tight", 0.95);
        let (engine, mut rx) = engine(&dir, vendor, cli.spawn.clone(), json_router(verdict.clone()));
        let id = engine.send(auto("medium running")).unwrap();
        support::until("the first fake answer did not start", || engine.open(&id).is_some_and(|c| !c.messages[1].text.is_empty())).await;
        let first = engine.open(&id).unwrap().messages[1].clone();
        assert_eq!(first.effort.as_deref(), Some("medium"));
        set_verdict(&verdict, "code", "hard", "machine", "open", 0.95);
        draft(&engine, Some(&id), "hard queued").await;
        engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("hard queued") }).unwrap();
        // The next draft replaces the shared classifier result before the queue can drain.
        set_verdict(&verdict, "chat", "easy", "glance", "tight", 0.95);
        draft(&engine, Some(&id), "easy queued").await;
        engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("easy queued") }).unwrap();
        set_verdict(&verdict, "writing", "medium", "glance", "tight", 0.95);
        draft(&engine, Some(&id), "medium unsent").await;
        assert_eq!(engine.open(&id).unwrap().messages[1].effort, first.effort);
        cli.release.notify_one();
        assert_eq!(finished(&mut rx).await.effort, first.effort);
        let queued = finished(&mut rx).await;
        assert_eq!(queued.status, Some(Status::Done));
        assert_eq!((queued.vendor, queued.model, queued.effort.as_deref()), (first.vendor, first.model, Some("high")));
        assert_eq!(queued.reason.as_deref(), Some("reason.autoEffortRaised"));
        let easy = finished(&mut rx).await;
        assert_eq!(easy.status, Some(Status::Done));
        assert_eq!(easy.effort.as_deref(), Some(if vendor == Vendor::Codex { "low" } else { "high" }));
        if vendor == Vendor::Codex {
            assert_eq!(cli.efforts(), vec!["medium", "high", "low"]);
        } else {
            let args = cli.args.lock().unwrap();
            assert_eq!(args.len(), 2);
            assert_eq!(flag(&args[1], "--effort").as_deref(), Some("high"));
            assert_eq!(flag(&args[1], "--resume").as_deref(), Some("effort-session"));
        }
        engine.shutdown();
    }
}

#[tokio::test]
async fn auto_effort_queued_messages_ignore_late_and_edited_verdicts() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        for edit in [false, true] {
            let dir = support::DataFolder::new();
            let cli = EffortCli::new(true);
            let gate = Arc::new(tokio::sync::Notify::new());
            let (pending, calls) = (gate.clone(), Calls::default());
            let seen = calls.clone();
            let ask: Ask = Arc::new(move |text| {
                let pending = pending.clone();
                seen.lock().unwrap().push(text.clone());
                Box::pin(async move {
                    if text == "hard queued" && !edit {
                        pending.notified().await;
                    }
                    Some(
                        if text == "hard queued" {
                            r#"{"kind":"code","difficulty":"hard","check":"machine","scope":"open","confidence":0.95}"#
                        } else if vendor == Vendor::Codex {
                            r#"{"kind":"math","difficulty":"medium","check":"machine","scope":"tight","confidence":0.95}"#
                        } else {
                            r#"{"kind":"writing","difficulty":"medium","check":"glance","scope":"tight","confidence":0.95}"#
                        }
                        .into(),
                    )
                })
            });
            let (engine, mut rx) = engine(&dir, vendor, cli.spawn.clone(), ask);
            let id = engine.send(auto("medium running")).unwrap();
            support::until("the first fake answer did not start", || engine.open(&id).is_some_and(|c| !c.messages[1].text.is_empty()))
                .await;
            let first = engine.open(&id).unwrap().messages[1].clone();
            assert_eq!(first.effort.as_deref(), Some("medium"));
            draft(&engine, Some(&id), "hard queued").await;
            engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("hard queued") }).unwrap();
            if edit {
                let turn = engine.open(&id).unwrap().queued[0].id.clone();
                engine.edit_queued(&id, &turn, "edited easy").unwrap();
            } else {
                gate.notify_one();
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
            cli.release.notify_one();
            assert_eq!(finished(&mut rx).await.effort, first.effort);
            let queued = finished(&mut rx).await;
            assert_eq!(queued.status, Some(Status::Done));
            assert_eq!(queued.effort, first.effort, "late or stale verdicts must not change effort");
            assert!(!queued.reason.unwrap().starts_with("reason.autoEffort"));
            assert_eq!(calls.lock().unwrap().len(), 2, "queue dispatch must not start another classification");
            engine.shutdown();
        }
    }
}

#[tokio::test]
async fn auto_effort_pending_verdict_never_holds_a_send_or_changes_its_answer_later() {
    let dir = support::DataFolder::new();
    let cli = EffortCli::new(false);
    let gate = Arc::new(tokio::sync::Notify::new());
    let pending = gate.clone();
    let ask: Ask = Arc::new(move |text| {
        let pending = pending.clone();
        Box::pin(async move {
            if text == "pending easy" {
                pending.notified().await;
            }
            Some(
                if text == "pending easy" {
                    r#"{"kind":"chat","difficulty":"easy","check":"glance","confidence":0.95}"#
                } else {
                    r#"{"kind":"code","difficulty":"hard","scope":"open","check":"machine","confidence":0.95}"#
                }
                .into(),
            )
        })
    });
    let (engine, mut rx) = engine(&dir, Vendor::Codex, cli.spawn.clone(), ask);
    let id = engine.send(auto("hard first")).unwrap();
    let first = finished(&mut rx).await;
    draft(&engine, Some(&id), "pending easy").await;
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("pending easy") }).unwrap();
    // The router stays blocked until after the answer ends: no timeout can produce a lower effort.
    let reply = tokio::time::timeout(Duration::from_secs(1), finished(&mut rx)).await.expect("send waited for a pending verdict");
    assert_eq!(reply.effort, first.effort);
    gate.notify_one();
    tokio::time::sleep(Duration::from_millis(80)).await;
    let saved = engine.open(&id).unwrap().messages[3].clone();
    assert_eq!(saved.effort, first.effort);
    assert!(!saved.reason.unwrap().starts_with("reason.autoEffort"));
    assert_eq!(cli.efforts(), vec!["high", "high"]);
    engine.shutdown();
}
