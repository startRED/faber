//! The ask judge (decided 2026-11-13): one cheap read of a project conversation's first request, before its agent starts. A
//! vague request gets one question first and a look-and-feel request two or three directions; nothing else changes. The
//! judge is answered in place of a model (`set_judge_ask`) and the project agent is a Claude Code driven by hand (`pipes`).

use crate::support::{self, Pipe, next_pipe, pipes};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Role, Status, Vendor};
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const VAGUE: &str = "Faz um sistema pra minha loja";

fn new_engine(dir: &support::DataFolder) -> (Engine, Arc<Mutex<Vec<Pipe>>>) {
    let (spawn, made) = pipes();
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: Some(Arc::new(|_| Box::pin(async { None }))),
    };
    let engine = Engine::new(opts, Arc::new(|_| {})).unwrap();
    let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    (engine, made)
}

/// A judge that answers `answer` and counts the prompts it read.
fn judge(engine: &Engine, answer: &'static str) -> Arc<Mutex<Vec<String>>> {
    let prompts: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = prompts.clone();
    engine.set_judge_ask(Arc::new(move |prompt| {
        seen.lock().unwrap().push(prompt);
        Box::pin(async move { Some(answer.to_string()) })
    }));
    prompts
}

fn request(area: Area, project: Option<String>, conversation: Option<String>, text: &str) -> SendRequest {
    SendRequest {
        conversation_id: conversation,
        area,
        project_id: project,
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    }
}

fn turns(pipe: &Pipe) -> Vec<String> {
    pipe.lines
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m["type"] == "user")
        .map(|m| match &m["message"]["content"] {
            Value::String(s) => support::person_string(s),
            Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""),
            _ => String::new(),
        })
        .collect()
}

async fn until_turns(pipe: &Pipe, n: usize) {
    support::until(&format!("the CLI never got {n} turns"), || turns(pipe).len() >= n).await;
}

async fn answer(pipe: &mut Pipe) {
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "claude-haiku-5-5" })).await;
    pipe.say(json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "text", "text": "ok" }] } })).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
}

async fn first_turn(engine: &Engine, made: &Arc<Mutex<Vec<Pipe>>>, folder: &Path, text: &str) -> (String, Pipe, String) {
    let project = engine.add_project(&folder.to_string_lossy()).unwrap();
    let id = engine.send(request(Area::Project, Some(project.id), None, text)).unwrap();
    let pipe = next_pipe(made).await;
    until_turns(&pipe, 1).await;
    let first = turns(&pipe)[0].clone();
    (id, pipe, first)
}

#[tokio::test]
async fn a_vague_first_project_request_carries_the_judges_one_question_and_only_once() {
    let (dir, folder) = (support::DataFolder::new(), support::work_folder());
    let (engine, made) = new_engine(&dir);
    let prompts = judge(&engine, r#"{"verdict":"ask","question":"O sistema é para vender online ou para controlar o estoque?"}"#);
    let (id, mut pipe, first) = first_turn(&engine, &made, folder.path(), VAGUE).await;
    assert!(first.starts_with(VAGUE), "the person's words come first: {first}");
    assert!(first.contains("From Faber, not the person"), "{first}");
    assert!(first.contains("only between the brackets: [O sistema é para vender online ou para controlar o estoque?] If"), "{first}");
    assert!(first.contains("form piece") && first.contains("end your turn"), "{first}");
    let read = prompts.lock().unwrap().clone();
    assert_eq!(read.len(), 1);
    assert!(read[0].contains("Never answer or carry out the request") && read[0].contains(&format!("Request: \"{VAGUE}\"")));
    answer(&mut pipe).await;
    support::until("the first answer never ended", || engine.open(&id).unwrap().messages[1].status == Some(Status::Done)).await;
    engine.send(request(Area::Project, None, Some(id.clone()), "Para vender online")).unwrap();
    until_turns(&pipe, 2).await;
    assert_eq!(turns(&pipe)[1], "Para vender online", "a later message is never judged or noted");
    assert_eq!(prompts.lock().unwrap().len(), 1);
    engine.shutdown();
}

#[tokio::test]
async fn a_look_and_feel_request_gets_two_or_three_directions_first() {
    let (dir, folder) = (support::DataFolder::new(), support::work_folder());
    let (engine, made) = new_engine(&dir);
    judge(&engine, r#"{"verdict":"directions","question":""}"#);
    let (_, _pipe, first) = first_turn(&engine, &made, folder.path(), "Deixa a página inicial mais bonita").await;
    assert!(first.starts_with("Deixa a página inicial mais bonita\n\n(From Faber, not the person"), "{first}");
    assert!(first.contains("two or three visual directions") && first.contains("design system"), "{first}");
    engine.shutdown();
}

#[tokio::test]
async fn a_clear_or_unreadable_verdict_leaves_the_request_as_it_was() {
    for verdict in [r#"{"verdict":"clear","question":""}"#, "not json", r#"{"verdict":"ask","question":" "}"#] {
        let (dir, folder) = (support::DataFolder::new(), support::work_folder());
        let (engine, made) = new_engine(&dir);
        judge(&engine, verdict);
        let (_, _pipe, first) = first_turn(&engine, &made, folder.path(), "Corrija o erro de login da tela inicial").await;
        assert_eq!(first, "Corrija o erro de login da tela inicial", "{verdict}");
        engine.shutdown();
    }
}

#[tokio::test(start_paused = true)]
async fn a_judge_that_never_answers_holds_the_first_turn_only_for_its_budget() {
    let (dir, folder) = (support::DataFolder::new(), support::work_folder());
    let (engine, made) = new_engine(&dir);
    engine.set_judge_ask(Arc::new(|_| Box::pin(std::future::pending())));
    let start = tokio::time::Instant::now();
    let (_, _pipe, first) = first_turn(&engine, &made, folder.path(), VAGUE).await;
    let waited = start.elapsed();
    assert_eq!(first, VAGUE);
    assert!(waited >= Duration::from_secs(12) && waited < Duration::from_secs(13), "{waited:?}");
    engine.shutdown();
}

#[tokio::test]
async fn stop_while_the_judge_reads_starts_nothing() {
    let (dir, folder) = (support::DataFolder::new(), support::work_folder());
    let (engine, made) = new_engine(&dir);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let rx = Arc::new(Mutex::new(Some(rx)));
    engine.set_judge_ask(Arc::new(move |_| {
        let rx = rx.lock().unwrap().take().unwrap();
        Box::pin(async move {
            let _ = rx.await;
            Some(r#"{"verdict":"ask","question":"Para quem?"}"#.to_string())
        })
    }));
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    let id = engine.send(request(Area::Project, Some(project.id), None, VAGUE)).unwrap();
    engine.interrupt(&id);
    support::until("the reply never stopped", || engine.open(&id).unwrap().messages[1].status != Some(Status::Streaming)).await;
    tx.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(made.lock().unwrap().is_empty(), "no agent started after Stop");
    engine.shutdown();
}

#[tokio::test]
async fn chats_and_long_requests_are_never_judged() {
    let (dir, folder) = (support::DataFolder::new(), support::work_folder());
    let (engine, made) = new_engine(&dir);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    engine.set_judge_ask(Arc::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Some(r#"{"verdict":"ask","question":"Qual?"}"#.to_string()) })
    }));
    let chat = engine.send(request(Area::Chat, None, None, VAGUE)).unwrap();
    let pipe = next_pipe(&made).await;
    until_turns(&pipe, 1).await;
    assert_eq!(turns(&pipe)[0], VAGUE, "a chat is not a project task");
    assert_eq!(engine.open(&chat).unwrap().messages[0].role, Role::User);
    let long = format!("Faz um sistema pra minha loja. {}", "Detalhe que já explica o que eu quero. ".repeat(60));
    let (_, _pipe, first) = first_turn(&engine, &made, folder.path(), &long).await;
    assert_eq!(first, long, "a long request says what it wants");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    engine.shutdown();
}
