//! The eyes (task 304): a conversation is offered `look` only while the key is on, on Windows, and a call is refused once
//! the key is off, whatever the session was started with. No real window is read here: the windows are a stand-in
//! (`Engine::set_eyes_backend`).
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Permissions, Settings, Vendor};
use faber_engine::{Engine, EngineOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// A Chat on a hand-driven Codex with its thread and turn started, under these settings.
async fn chat(settings: Settings) -> (Engine, support::Pipe, Value, support::DataFolder, String) {
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
    app.save_settings(settings).unwrap();
    let id = app
        .send(faber_engine::SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "What is in my other window?".into(),
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
    (app, pipe, start, dir, id)
}

async fn wire(pipe: &support::Pipe, ready: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(support::DEADLINE, async {
        loop {
            if let Some(m) = pipe.lines.lock().unwrap().iter().rev().find(|m| ready(m)).cloned() {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing line on the fake wire: {:?}", pipe.lines.lock().unwrap()))
}

async fn look(pipe: &mut support::Pipe, id: u64, arguments: Value) -> (String, bool) {
    pipe.say(json!({"id": id, "method": "item/tool/call", "params": {"threadId": "main", "turnId": "turn-main",
        "callId": format!("call-{id}"), "tool": "look", "arguments": arguments}}))
        .await;
    let result = wire(pipe, |m| m["id"] == id && m.get("result").is_some()).await["result"].clone();
    (result["contentItems"][0]["text"].as_str().unwrap_or_default().to_string(), result["success"] == true)
}

fn offers_look(start: &Value) -> bool {
    start["params"]["dynamicTools"].as_array().into_iter().flatten().any(|tool| tool["name"] == "look")
}

#[tokio::test]
async fn the_key_off_offers_no_eyes_and_switching_it_off_refuses_the_next_call() {
    let (app, mut pipe, start, _dir, _) = chat(Settings { permissions: Some(Permissions::FullAccess), ..Settings::default() }).await;
    assert!(!offers_look(&start), "the key is off by default: {start}");
    let (said, ok) = look(&mut pipe, 700, json!({"action": "windows"})).await;
    assert!(!ok && said.contains("not available"), "{said}");
    app.shutdown();

    let on = Settings { permissions: Some(Permissions::FullAccess), see_other_windows: Some(true), ..Settings::default() };
    let (app, mut pipe, start, _dir, _) = chat(on).await;
    assert_eq!(offers_look(&start), cfg!(windows), "offered with the key on, on Windows only: {start}");
    app.save_settings(Settings { see_other_windows: Some(false), ..Settings::default() }).unwrap();
    let (said, ok) = look(&mut pipe, 701, json!({"action": "tree", "window": "0x10"})).await;
    assert!(!ok, "{said}");
    if cfg!(windows) {
        assert!(said.contains("turned off"), "the key is read again at the call, even in Full access: {said}");
    }
    app.shutdown();
}

/// One window, notepad's, drawn by process 10; every native step is written down.
#[cfg(windows)]
#[derive(Default)]
struct Screen {
    calls: std::sync::Mutex<Vec<&'static str>>,
}

#[cfg(windows)]
impl Screen {
    fn window() -> faber_engine::eyes::Listed {
        faber_engine::eyes::Listed {
            handle: 0x10,
            title: "notes".into(),
            class: "Notepad".into(),
            width: 80,
            height: 60,
            owner: Some(faber_engine::eyes::Owner {
                pid: 10,
                started: 100,
                path: Some(r"C:\Windows\notepad.exe".into()),
                elevated: Some(false),
            }),
            ..Default::default()
        }
    }
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

#[cfg(windows)]
impl faber_engine::eyes::Backend for Screen {
    fn me(&self) -> faber_engine::eyes::Me {
        faber_engine::eyes::Me::default()
    }
    fn windows(&self) -> Option<Vec<faber_engine::eyes::Listed>> {
        self.calls.lock().unwrap().push("windows");
        Some(vec![Screen::window()])
    }
    fn window(&self, handle: u64) -> Option<faber_engine::eyes::Listed> {
        (handle == 0x10).then(Screen::window)
    }
    fn hung(&self, _: u64) -> bool {
        false
    }
    fn request_accessibility(&self, _: u64, _: std::time::Instant) {
        self.calls.lock().unwrap().push("accessibility");
    }
    fn tree(
        &self,
        _: u64,
        _: usize,
        _: std::time::Instant,
        _: &dyn Fn(&faber_engine::eyes::Owner) -> bool,
    ) -> Option<(Vec<faber_engine::eyes::Node>, faber_engine::eyes::Walked)> {
        self.calls.lock().unwrap().push("tree");
        Some((
            vec![faber_engine::eyes::Node { kind: "edit".into(), enabled: true, ..Default::default() }],
            faber_engine::eyes::Walked { count: 1, capped: false },
        ))
    }
    fn picture(&self, _: u64, _: &dyn Fn() -> bool) -> Result<faber_engine::eyes::Image, &'static str> {
        self.calls.lock().unwrap().push("picture");
        Err("no picture here")
    }
}

/// The card waiting on the conversation's newest turn, as (turn, tool), once one is there.
#[cfg(windows)]
async fn card(app: &Engine, id: &str) -> (String, String) {
    tokio::time::timeout(support::DEADLINE, async {
        loop {
            let found = app.open(id).and_then(|c| {
                c.messages.iter().rev().find_map(|m| m.permissions.iter().flatten().next().map(|p| (m.id.clone(), p.tool.clone())))
            });
            if let Some(found) = found {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("a card")
}

#[cfg(windows)]
#[tokio::test]
async fn switching_the_key_off_answers_a_waiting_card_no_and_an_allow_after_it_reads_nothing() {
    let on = Settings { permissions: Some(Permissions::Ask), see_other_windows: Some(true), ..Settings::default() };
    let (app, mut pipe, _, _dir, id) = chat(on).await;
    let screen = Arc::new(Screen::default());
    app.set_eyes_backend(screen.clone());
    pipe.say(json!({"id": 702, "method": "item/tool/call", "params": {"threadId": "main", "turnId": "turn-main",
        "callId": "call-702", "tool": "look", "arguments": {"action": "tree", "window": "0x10"}}}))
        .await;
    let (turn, tool) = card(&app, &id).await;
    assert_eq!(tool, "faber.eyes.window", "a new window asks in Ask");
    app.save_settings(Settings { see_other_windows: Some(false), ..Settings::default() }).unwrap();
    let result = wire(&pipe, |m| m["id"] == 702 && m.get("result").is_some()).await["result"].clone();
    assert_eq!(result["success"], false);
    assert!(result["contentItems"][0]["text"].as_str().unwrap().contains("turned off"), "{result}");
    assert!(app.answer(&id, &turn, "call-702", true, false).await.is_err(), "the card is spent");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(screen.calls().is_empty(), "nothing was read: {:?}", screen.calls());
    app.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn the_list_of_windows_asks_once_a_session_outside_full_access_and_never_in_it() {
    let on = Settings { permissions: Some(Permissions::Ask), see_other_windows: Some(true), ..Settings::default() };
    let (app, mut pipe, _, _dir, id) = chat(on).await;
    let screen = Arc::new(Screen::default());
    app.set_eyes_backend(screen.clone());
    pipe.say(json!({"id": 703, "method": "item/tool/call", "params": {"threadId": "main", "turnId": "turn-main",
        "callId": "call-703", "tool": "look", "arguments": {"action": "windows"}}}))
        .await;
    let (turn, tool) = card(&app, &id).await;
    assert_eq!(tool, "faber.eyes.windows");
    assert!(screen.calls().is_empty(), "nothing is listed before the person answers");
    app.answer(&id, &turn, "call-703", true, false).await.unwrap();
    let result = wire(&pipe, |m| m["id"] == 703 && m.get("result").is_some()).await["result"].clone();
    assert_eq!(result["contentItems"][0]["text"], "0x10 notepad.exe \"notes\" 80x60\n", "{result}");
    let (said, ok) = look(&mut pipe, 704, json!({"action": "windows"})).await;
    assert!(ok && said.contains("notepad.exe"), "the same session is not asked again: {said}");
    app.shutdown();

    let full = Settings { permissions: Some(Permissions::FullAccess), see_other_windows: Some(true), ..Settings::default() };
    let (app, mut pipe, _, _dir, _) = chat(full).await;
    app.set_eyes_backend(Arc::new(Screen::default()));
    let (said, ok) = look(&mut pipe, 705, json!({"action": "windows"})).await;
    assert!(ok && said.contains("notepad.exe"), "Full access does not ask: {said}");
    let (said, ok) = look(&mut pipe, 706, json!({"action": "tree", "window": "0x10"})).await;
    assert!(ok && said.starts_with("edit"), "{said}");
    app.shutdown();
}
