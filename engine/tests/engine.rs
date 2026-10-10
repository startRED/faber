mod support;

use faber_engine::cli::{Cli, resolve_cli};
use faber_engine::model::{Area, Message, Role, Settings, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::{Choice, pick_model};
use faber_engine::{AppEvent, BranchRequest, Engine, EngineOptions, SendRequest, WarmRequest};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, fake, fixture};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";

fn engine(dir: &Path, vendor: Vendor, spawn: Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
    engine_idle(dir, vendor, spawn, Duration::from_secs(120))
}

fn engine_idle(dir: &Path, vendor: Vendor, spawn: Spawner, idle: Duration) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let clis = BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]);
    let opts =
        EngineOptions { data_dir: dir.into(), clis, spawn, windows: false, max_warm: 3, idle, runtime: tokio::runtime::Handle::current() };
    (Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))), rx)
}

fn choice(vendor: &str, model: Option<&str>) -> Choice {
    Choice { vendor: vendor.into(), model: model.map(String::from) }
}

fn ask(area: Area, text: &str, choice: Choice) -> SendRequest {
    SendRequest { conversation_id: None, area, project_id: None, text: text.into(), images: vec![], files: vec![], choice }
}

/// Waits for the next finished answer, letting `react` answer what the screen would.
async fn finished(rx: &mut UnboundedReceiver<AppEvent>, mut react: impl FnMut(&Message)) -> Message {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("no finished answer in time").expect("engine gone");
        if let AppEvent::Message { message, .. } = e {
            react(&message);
            if message.status != Some(Status::Streaming) {
                return *message;
            }
        }
    }
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn a_chat_turn_streams_to_the_screen_and_is_saved_with_model_reason_session_and_cost() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, f.spawn.clone());
    let id = engine.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
    let reply = finished(&mut rx, |_| {}).await;
    engine.shutdown();
    assert_eq!(reply.status, Some(Status::Done));
    assert!(reply.text.contains("Rayleigh"));
    assert_eq!(reply.model.as_deref(), Some("claude-haiku-5-5"));
    assert_eq!(reply.reason.as_deref(), Some("reason.chosen"));
    assert!(f.args.lock().unwrap()[0].contains(&"--restricted".to_string()));
    let saved = read_json(&dir.path().join("conversations").join(format!("{id}.json")));
    assert_eq!(saved["sessions"]["claude"], "c5b9834c-dfc7-4086-beed-2212be93b722");
    assert_eq!(saved["title"], SKY);
    assert!(saved["messages"][1]["costUsd"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn quota_readings_reach_the_screen_with_their_vendor() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
    engine.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
    let mut quota = false;
    while let Ok(Some(e)) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await {
        quota |= matches!(&e, AppEvent::Quota { vendor: Vendor::Claude, .. });
        if matches!(&e, AppEvent::Message { message, .. } if message.status != Some(Status::Streaming)) {
            break;
        }
    }
    assert!(quota);
}

#[tokio::test]
async fn after_reopening_the_app_the_conversation_resumes_its_cli_session() {
    let dir = tempfile::tempdir().unwrap();
    let (first, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
    let id = first.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
    finished(&mut rx, |_| {}).await;
    first.shutdown();

    // The resume recording continues session 3ee0bd99; point the saved conversation at it.
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    let mut saved = read_json(&file);
    saved["sessions"]["claude"] = "3ee0bd99-72d2-4c5b-8420-a9f85ee53981".into();
    std::fs::write(&file, saved.to_string()).unwrap();

    let f = fake(fixture("claude", "resume"));
    let (second, mut rx) = engine(dir.path(), Vendor::Claude, f.spawn.clone());
    let text = "What single word did you reply with last time? Answer with only that word.";
    second.send(SendRequest { conversation_id: Some(id.clone()), ..ask(Area::Chat, text, choice("claude", Some("haiku"))) }).unwrap();
    let reply = finished(&mut rx, |_| {}).await;
    second.shutdown();
    let args = f.args.lock().unwrap()[0].clone();
    assert_eq!(args[args.iter().position(|a| a == "--resume").unwrap() + 1], "3ee0bd99-72d2-4c5b-8420-a9f85ee53981");
    assert_eq!(reply.text.trim(), "pong");
    assert_eq!(second.open(&id).unwrap().messages.len(), 4);
}

#[tokio::test]
async fn an_idle_claude_process_closes_and_the_next_message_resumes_its_session() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine_idle(dir.path(), Vendor::Claude, f.spawn.clone(), Duration::from_millis(50));
    let id = engine.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
    finished(&mut rx, |_| {}).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    engine.send(SendRequest { conversation_id: Some(id), ..ask(Area::Chat, "And at sunset?", choice("claude", Some("haiku"))) }).unwrap();
    engine.shutdown();
    let args = f.args.lock().unwrap();
    assert_eq!(args.len(), 2, "the first process closed while idle");
    assert!(args[1].contains(&"--resume".to_string()));
}

#[tokio::test]
async fn a_new_chat_starts_its_cli_while_the_user_types_and_keeps_it_for_the_first_message() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, f.spawn.clone());
    let warm = || WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice("claude", Some("haiku")) };
    engine.warm(warm());
    engine.warm(warm());
    assert_eq!(f.args.lock().unwrap().len(), 1);
    assert!(engine.state().conversations.is_empty());
    engine.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
    assert_eq!(finished(&mut rx, |_| {}).await.status, Some(Status::Done));
    assert_eq!(f.args.lock().unwrap().len(), 1);
    assert_eq!(engine.state().conversations.len(), 1);
    engine.shutdown();
}

#[tokio::test]
async fn always_allow_answers_the_next_request_for_the_same_tool_without_asking_again() {
    // The recording denied the second Write; here the engine must allow it by itself.
    let mut answers = 0;
    let entries: Vec<Entry> = fixture("claude", "tools")
        .into_iter()
        .map(|mut e| {
            if e.msg["type"] == "control_response" {
                answers += 1;
                if answers == 2 {
                    e.msg["response"]["response"] = serde_json::json!({ "behavior": "allow" });
                }
            }
            e
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, fake(entries).spawn);
    let id = engine.send(ask(Area::Chat, TWO_WRITES, choice("claude", None))).unwrap();
    let mut asked: Vec<String> = Vec::new();
    finished(&mut rx, |m| {
        if let Some(p) = m.permissions.as_ref().and_then(|p| p.first())
            && !asked.contains(&p.request_id)
        {
            asked.push(p.request_id.clone());
            engine.answer(&id, &p.request_id, true, true);
        }
    })
    .await;
    engine.shutdown();
    assert_eq!(asked.len(), 1);
    assert_eq!(engine.open(&id).unwrap().allow_always, Some(vec!["Write".to_string()]));
}

#[tokio::test]
async fn two_permission_requests_at_once_wait_in_a_queue_and_both_get_answered() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "parallel")).spawn);
    let text = "Search the web for the latest stable version of Node.js and, in parallel, for the latest version of Python. Run both searches at the same time, then answer in two lines.";
    let id = engine.send(ask(Area::Chat, text, choice("claude", None))).unwrap();
    let (mut shown, mut answered) = (Vec::new(), Vec::new());
    let reply = finished(&mut rx, |m| {
        let waiting = m.permissions.clone().unwrap_or_default();
        if !waiting.is_empty() {
            shown.push(waiting.len());
        }
        if let Some(first) = waiting.first()
            && !answered.contains(&first.request_id)
        {
            answered.push(first.request_id.clone());
            engine.answer(&id, &first.request_id, true, false);
        }
    })
    .await;
    engine.shutdown();
    assert_eq!(answered.len(), 2);
    assert!(shown.contains(&2), "both requests were waiting at the same time");
    assert_eq!(reply.status, Some(Status::Done));
    assert_eq!(reply.permissions, None);
}

#[tokio::test]
async fn text_blocks_around_a_tool_are_kept_apart() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), Vendor::Codex, fake(fixture("codex", "tools")).spawn);
    let project = engine.add_project(&dir.path().to_string_lossy());
    let text = "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.";
    let id = engine.send(SendRequest { project_id: Some(project.id), ..ask(Area::Project, text, choice("codex", None)) }).unwrap();
    let mut asked: HashMap<String, bool> = HashMap::new();
    let reply = finished(&mut rx, |m| {
        if let Some(p) = m.permissions.as_ref().and_then(|p| p.first())
            && !asked.contains_key(&p.request_id)
        {
            let allow = asked.is_empty();
            asked.insert(p.request_id.clone(), allow);
            engine.answer(&id, &p.request_id, allow, false);
        }
    })
    .await;
    engine.shutdown();
    let blocks: Vec<&str> = reply.text.split("\n\n").collect();
    assert!(blocks[0].contains("a.txt"), "{}", reply.text);
    assert!(blocks.len() > 1 && !blocks[1].is_empty(), "expected two blocks in: {}", reply.text);
}

#[tokio::test]
async fn editing_a_message_branches_into_a_new_conversation_and_leaves_the_original_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
    let pick = || choice("claude", Some("haiku"));
    let id = engine.send(ask(Area::Chat, SKY, pick())).unwrap();
    finished(&mut rx, |_| {}).await;
    std::fs::write(dir.path().join("chat").join(&id).join("notes.txt"), "made earlier").unwrap();
    let first = engine.open(&id).unwrap().messages[0].clone();
    let branch_id = engine
        .branch(BranchRequest { conversation_id: id.clone(), message_id: first.id.clone(), text: SKY.into(), choice: pick() })
        .unwrap();
    finished(&mut rx, |_| {}).await;
    engine.shutdown();
    assert_ne!(branch_id, id);
    assert_eq!(engine.open(&id).unwrap().messages.len(), 2);
    let branch = engine.open(&branch_id).unwrap();
    let of = branch.branch_of.clone().unwrap();
    assert_eq!((of.conversation_id, of.message_id), (id.clone(), first.id));
    assert_eq!(branch.messages.len(), 2);
    assert_eq!(std::fs::read_to_string(dir.path().join("chat").join(&branch_id).join("notes.txt")).unwrap(), "made earlier");
}

#[tokio::test]
async fn open_in_terminal_resumes_the_same_cli_session_in_the_conversation_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
    let id = engine.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
    finished(&mut rx, |_| {}).await;
    engine.shutdown();
    let t = engine.terminal(&id).unwrap();
    assert_eq!(t.command, "claude");
    assert_eq!(t.args, ["--resume", "c5b9834c-dfc7-4086-beed-2212be93b722"]);
    assert_eq!(t.cwd, dir.path().join("chat").join(&id));
    assert!(engine.terminal("missing").is_none());
}

#[tokio::test]
async fn the_hooks_choice_is_saved_and_restarts_idle_project_clis_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "chat"));
    let (engine, _rx) = engine(dir.path(), Vendor::Claude, f.spawn.clone());
    let project = engine.add_project(&dir.path().to_string_lossy());
    let warm =
        || WarmRequest { conversation_id: None, area: Area::Project, project_id: Some(project.id.clone()), choice: choice("claude", None) };
    let last_args = || f.args.lock().unwrap().last().cloned().unwrap_or_default().join(" ");
    engine.warm(warm());
    assert!(last_args().contains("disableAllHooks"));
    engine.save_settings(Settings { user_hooks: Some(true), ..Settings::default() });
    engine.warm(warm());
    assert!(!last_args().contains("disableAllHooks"));
    let appearance = serde_json::json!({ "mode": "light" }).as_object().cloned();
    engine.save_settings(Settings { appearance, ..Settings::default() });
    engine.shutdown();
    assert_eq!(read_json(&dir.path().join("settings.json")), serde_json::json!({ "userHooks": true, "appearance": { "mode": "light" } }));
}

#[tokio::test]
async fn attachments_are_saved_in_the_workspace_and_named_in_the_turn() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "chat"));
    let (engine, _rx) = engine(dir.path(), Vendor::Claude, f.spawn.clone());
    let files = vec![faber_engine::workspace::Attachment { name: "notes.pdf".into(), base64: "JVBERi0xLjQ=".into() }];
    let id = engine.send(SendRequest { files, ..ask(Area::Chat, "read it", choice("claude", None)) }).unwrap();
    engine.shutdown();
    assert!(dir.path().join("chat").join(&id).join("attachments").join("notes.pdf").is_file());
    assert_eq!(engine.open(&id).unwrap().messages[0].attachments, Some(vec!["notes.pdf".to_string()]));
}

#[test]
fn a_model_joining_midway_gets_the_earlier_conversation_as_text() {
    let history = [Message::new("1".into(), Role::User, "hi".into(), 0), Message::new("2".into(), Role::Assistant, "hello".into(), 0)];
    let text = faber_engine::engine::with_history(&history, "and now?");
    assert!(text.ends_with("User: hi\n\nAssistant: hello\n\nNow the user says:\nand now?"));
}

#[test]
fn auto_picks_a_connected_vendor_and_says_why_a_missing_one_is_refused() {
    let auto = choice("auto", None);
    let only_codex = pick_model(&auto, &[Vendor::Codex]).unwrap();
    assert_eq!((only_codex.vendor, only_codex.reason.as_str(), only_codex.effort.as_str()), (Vendor::Codex, "reason.autoOnly", "medium"));
    assert_eq!(pick_model(&auto, &[Vendor::Claude, Vendor::Codex]).unwrap().vendor, Vendor::Claude);
    assert!(pick_model(&choice("claude", None), &[Vendor::Codex]).is_none());
    assert!(pick_model(&auto, &[]).is_none());
}

#[test]
fn cli_lookup_follows_npm_cmd_shims_on_windows_to_the_real_program() {
    let files: HashMap<&str, &str> = HashMap::from([
        ("C:\\npm\\claude.cmd", "@ECHO off\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n"),
        ("C:\\npm\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe", ""),
        (
            "C:\\npm\\gemini.cmd",
            "@IF EXIST \"%~dp0\\node.exe\" (\r\n  \"%~dp0\\node.exe\"  \"%~dp0\\node_modules\\@google\\gemini-cli\\dist\\index.js\" %*\r\n)",
        ),
        ("C:\\node\\node.exe", ""),
        ("C:\\codex\\codex.exe", ""),
    ]);
    let path = "C:\\npm;C:\\codex;C:\\node";
    let has = |p: &str| files.contains_key(p);
    let read = |p: &str| files[p].to_string();
    let cli =
        |command: &str, prefix: &[&str]| Some(Cli { command: command.into(), prefix: prefix.iter().map(|s| s.to_string()).collect() });
    assert_eq!(
        resolve_cli("claude", path, true, &has, &read),
        cli("C:\\npm\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe", &[])
    );
    assert_eq!(resolve_cli("codex", path, true, &has, &read), cli("C:\\codex\\codex.exe", &[]));
    assert_eq!(
        resolve_cli("gemini", path, true, &has, &read),
        cli("C:\\node\\node.exe", &["C:\\npm\\node_modules\\@google\\gemini-cli\\dist\\index.js"])
    );
    assert_eq!(resolve_cli("missing", path, true, &has, &read), None);
    assert_eq!(resolve_cli("claude", "/usr/bin", false, &|p| p == "/usr/bin/claude", &read), cli("/usr/bin/claude", &[]));
}
