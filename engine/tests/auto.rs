mod support;

use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::model::{Area, Message, Role, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{fake, fixture};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
// The anonymous numbers of two candidates, as the router sees them.
const LUNA: &str = "iq 29 agent 3 ";
const HAIKU: &str = "iq 34 agent 15 ";

type Calls = Arc<Mutex<Vec<String>>>;

fn engine(dir: &Path, vendor: Vendor, spawn: Spawner, ask: Ask) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let clis = BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]);
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis,
        spawn,
        windows: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: Some(ask),
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    (engine, rx)
}

/// A router that picks the candidate with these numbers, whatever letter the shuffle gave it.
fn router(numbers: &'static str, kind: &'static str, level: &'static str, calls: Calls) -> Ask {
    Arc::new(move |prompt: String| {
        let letter = prompt.lines().find(|l| l.contains(numbers)).and_then(|l| l.chars().next()).unwrap();
        calls.lock().unwrap().push(prompt);
        Box::pin(async move { Some(format!(r#"{{"pick":"{letter}","kind":"{kind}","difficulty":"{level}","confidence":0.95}}"#)) })
    })
}

fn auto(text: &str) -> SendRequest {
    let choice = Choice { vendor: "auto".into(), model: None };
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
    let dir = tempfile::tempdir().unwrap();
    let calls = Calls::default();
    let (engine, mut rx) =
        engine(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn, router(LUNA, "chat", "easy", calls.clone()));
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
    let dir = tempfile::tempdir().unwrap();
    let silent: Ask = Arc::new(|_| Box::pin(async { None }));
    let (engine, mut rx) = engine(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn, silent);
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    engine.shutdown();
    assert_eq!(reply.status, Some(Status::Done));
    assert_eq!(reply.reason.as_deref(), Some("reason.autoOnly"));
}

#[tokio::test]
async fn a_draft_routed_while_typing_answers_without_asking_again() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Calls::default();
    let (engine, mut rx) =
        engine(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn, router(LUNA, "chat", "easy", calls.clone()));
    let typing =
        WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: Choice { vendor: "auto".into(), model: None } };
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
async fn later_messages_stay_with_the_routed_model() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Calls::default();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, f.spawn.clone(), router(HAIKU, "chat", "easy", calls.clone()));
    let id = engine.send(auto(SKY)).unwrap();
    let first = finished(&mut rx).await;
    engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("And at sunset?") }).unwrap();
    let second = engine.open(&id).unwrap().messages[3].clone();
    engine.shutdown();
    assert_eq!(calls.lock().unwrap().len(), 1, "only the first message is routed");
    let args = f.args.lock().unwrap()[0].clone();
    let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    assert_eq!((after("--model"), after("--effort")), ("claude-haiku-5-5".to_string(), "medium".to_string()));
    assert_eq!((second.vendor, second.model, second.reason), (Some(Vendor::Claude), first.model, first.reason));
}

#[tokio::test]
async fn stop_while_the_route_is_coming_stops_the_answer() {
    let dir = tempfile::tempdir().unwrap();
    let never: Ask = Arc::new(|_| Box::pin(std::future::pending()));
    let (engine, _rx) = engine(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn, never);
    let id = engine.send(auto(SKY)).unwrap();
    assert_eq!(engine.open(&id).unwrap().messages[1].reason.as_deref(), Some("reason.routing"));
    engine.interrupt(&id);
    let reply = engine.open(&id).unwrap().messages[1].clone();
    engine.shutdown();
    assert_eq!(reply.status, Some(Status::Stopped));
}
