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
    for _ in 0..100 {
        if let Some(m) = engine.open(id).and_then(|c| c.messages.get(n * 2 + 1).cloned()).filter(|m| m.model.is_some()) {
            return m;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("message {n} was never routed");
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
        let e = tokio::time::timeout(Duration::from_secs(6), rx.recv()).await.expect("no finished answer in time").expect("engine gone");
        if let AppEvent::Message { message, .. } = e
            && message.role == Role::Assistant
            && message.status != Some(Status::Streaming)
        {
            return *message;
        }
    }
}

#[tokio::test]
async fn auto_routes_the_first_message_blind_and_says_why() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, router("chat", "easy", calls.clone()));
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    engine.shutdown();
    assert_eq!(reply.model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(reply.reason.as_deref(), Some("reason.route.chat.easy"));
    let prompt = calls.lock().unwrap()[0].to_lowercase();
    assert!(prompt.contains(&SKY.to_lowercase()) && !prompt.contains("luna") && !prompt.contains("gpt"));
}

#[tokio::test]
async fn a_router_without_an_answer_leaves_the_everyday_default() {
    let dir = crate::support::DataFolder::new();
    let silent: Ask = Arc::new(|_| Box::pin(async { None }));
    let (engine, mut rx) = engine(&dir, Vendor::Codex, fake(fixture("codex", "chat")).spawn, silent);
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    engine.shutdown();
    assert_eq!(reply.status, Some(Status::Done));
    assert_eq!(reply.reason.as_deref(), Some("reason.autoOnly"));
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

/// Rule 3 of models.md: with nothing asked for, a conversation keeps its process, model and effort from one message to the next.
#[tokio::test]
async fn a_second_message_keeps_the_process_the_model_and_the_effort_of_the_first() {
    let dir = crate::support::DataFolder::new();
    let calls = Calls::default();
    let f = fake(support::claude_two_turns("req"));
    let (engine, _rx) = engine(&dir, Vendor::Claude, f.spawn.clone(), router("code", "medium", calls.clone()));
    // Each turn of the recording asks to write a file: answer it, then wait for the reply to end.
    let turn = |engine: &Engine, id: String, reply: usize| {
        let engine = engine.clone();
        async move {
            for _ in 0..250 {
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
