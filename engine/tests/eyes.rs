//! The eyes (task 304, text only): a conversation is offered `look` only while the key is on, on Windows, and a call is
//! refused once the key is off, whatever the session was started with. No real window is read here: the windows are a
//! stand-in (`Engine::set_eyes_backend`).
use crate::support;
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Permissions, Settings, Vendor};
use faber_engine::{Engine, EngineOptions};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// A conversation in Chat on a hand-driven CLI, under these settings: for Codex with its thread and turn started (the
/// `thread/start` line is returned), for Claude Code once it said it is ready.
async fn chat_on(vendor: Vendor, settings: Settings) -> (Engine, support::Pipe, Value, support::DataFolder, String) {
    let dir = support::DataFolder::new();
    let (spawn, made) = support::pipes();
    let app = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
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
            choice: faber_engine::router::Choice { vendor: vendor.name().into(), model: None, effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&made).await;
    if vendor == Vendor::Claude {
        wire(&pipe, |m| m["type"] == "user").await;
        pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s1", "model": "claude-haiku-5-5" })).await;
        return (app, pipe, Value::Null, dir, id);
    }
    let init = wire(&pipe, |m| m["method"] == "initialize").await;
    pipe.say(json!({"id": init["id"], "result": {}})).await;
    let start = wire(&pipe, |m| m["method"] == "thread/start").await;
    pipe.say(json!({"id": start["id"], "result": {"thread": {"id": "main"}}})).await;
    let turn = wire(&pipe, |m| m["method"] == "turn/start").await;
    pipe.say(json!({"id": turn["id"], "result": {"turn": {"id": "turn-main"}}})).await;
    (app, pipe, start, dir, id)
}

async fn chat(settings: Settings) -> (Engine, support::Pipe, Value, support::DataFolder, String) {
    chat_on(Vendor::Codex, settings).await
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

/// The agent calls `look`; the answer is read with `result`.
async fn call(pipe: &mut support::Pipe, id: u64, arguments: Value) {
    pipe.say(json!({"id": id, "method": "item/tool/call", "params": {"threadId": "main", "turnId": "turn-main",
        "callId": format!("call-{id}"), "tool": "look", "arguments": arguments}}))
        .await;
}

async fn result(pipe: &support::Pipe, id: u64) -> (String, bool) {
    let result = wire(pipe, |m| m["id"] == id && m.get("result").is_some()).await["result"].clone();
    (result["contentItems"][0]["text"].as_str().unwrap_or_default().to_string(), result["success"] == true)
}

async fn look(pipe: &mut support::Pipe, id: u64, arguments: Value) -> (String, bool) {
    call(pipe, id, arguments).await;
    result(pipe, id).await
}

fn offers_look(start: &Value) -> bool {
    start["params"]["dynamicTools"].as_array().into_iter().flatten().any(|tool| tool["name"] == "look")
}

fn settings(mode: Permissions) -> Settings {
    Settings { permissions: Some(mode), see_other_windows: Some(true), ..Settings::default() }
}

#[tokio::test]
async fn the_key_off_offers_no_eyes_and_switching_it_off_refuses_the_next_call() {
    let (app, mut pipe, start, _dir, _) = chat(Settings { permissions: Some(Permissions::FullAccess), ..Settings::default() }).await;
    assert!(!offers_look(&start), "the key is off by default: {start}");
    let (said, ok) = look(&mut pipe, 700, json!({"action": "windows"})).await;
    assert!(!ok && said.contains("not available"), "{said}");
    app.shutdown();

    let (app, mut pipe, start, _dir, _) = chat(settings(Permissions::FullAccess)).await;
    assert_eq!(offers_look(&start), cfg!(windows), "offered with the key on, on Windows only: {start}");
    let tool = start["params"]["dynamicTools"].as_array().into_iter().flatten().find(|tool| tool["name"] == "look").cloned();
    if let Some(tool) = tool {
        assert_eq!(tool["inputSchema"]["properties"]["action"]["enum"], json!(["windows", "tree"]), "text only: {tool}");
    }
    app.save_settings(Settings { see_other_windows: Some(false), ..Settings::default() }).unwrap();
    let (said, ok) = look(&mut pipe, 701, json!({"action": "tree", "window": "0x10"})).await;
    assert!(!ok, "{said}");
    if cfg!(windows) {
        assert!(said.contains("turned off"), "the key is read again at the call, even in Full access: {said}");
    }
    app.shutdown();
}

#[tokio::test]
async fn a_cli_request_under_the_name_of_a_window_card_is_never_shown() {
    let (app, mut pipe, _, _dir, id) = chat_on(Vendor::Claude, settings(Permissions::Ask)).await;
    for (n, tool) in ["faber.eyes.window", "faber.eyes.windows"].into_iter().enumerate() {
        let request = format!("fake-{n}");
        pipe.say(json!({ "type": "control_request", "request_id": request, "request": { "subtype": "can_use_tool",
            "tool_name": tool, "tool_use_id": format!("toolu_{n}"), "input": { "program": "notepad.exe" } } }))
            .await;
        let answer = wire(&pipe, |m| m["response"]["request_id"] == request.as_str()).await;
        assert_eq!(answer["response"]["response"]["behavior"], "deny", "{answer}");
    }
    let cards = app.open(&id).unwrap().messages.iter().flat_map(|m| m.permissions.clone().unwrap_or_default()).count();
    assert_eq!(cards, 0, "no card dressed as Faber's own");
    app.shutdown();
}

/// One window, notepad's, drawn by process 10 from thread 5; every native step is written down. `hold_lookup` stops the
/// first look at the window until the test lets it go; `inside` is another process whose element the tree meets, and
/// `hold_tree` stops the tree right after it, until the test lets it go.
#[cfg(windows)]
#[derive(Default)]
struct Screen {
    calls: std::sync::Mutex<Vec<&'static str>>,
    class: std::sync::Mutex<Option<String>>,
    title: Option<&'static str>,
    inside: Option<faber_engine::eyes::Owner>,
    hold_lookup: std::sync::atomic::AtomicBool,
    looked: std::sync::atomic::AtomicBool,
    hold_tree: std::sync::atomic::AtomicBool,
    in_tree: std::sync::atomic::AtomicBool,
}

#[cfg(windows)]
fn notepad() -> faber_engine::eyes::Owner {
    faber_engine::eyes::Owner { pid: 10, started: 100, path: Some(r"C:\Windows\notepad.exe".into()), elevated: Some(false) }
}

#[cfg(windows)]
fn wait_while(flag: &std::sync::atomic::AtomicBool) {
    let start = std::time::Instant::now();
    while flag.load(std::sync::atomic::Ordering::SeqCst) && start.elapsed() < support::DEADLINE {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(windows)]
impl Screen {
    fn window(&self) -> faber_engine::eyes::Listed {
        faber_engine::eyes::Listed {
            handle: 0x10,
            title: self.title.unwrap_or("notes").into(),
            class: self.class.lock().unwrap().clone().unwrap_or_else(|| "Notepad".into()),
            thread: 5,
            width: 80,
            height: 60,
            owner: Some(notepad()),
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
        Some(vec![Screen::window(self)])
    }
    fn window(&self, handle: u64) -> Option<faber_engine::eyes::Listed> {
        self.looked.store(true, std::sync::atomic::Ordering::SeqCst);
        wait_while(&self.hold_lookup);
        (handle == 0x10).then(|| Screen::window(self))
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
        refused: &dyn Fn(&faber_engine::eyes::Owner) -> bool,
    ) -> Option<(Vec<faber_engine::eyes::Node>, faber_engine::eyes::Walked)> {
        self.calls.lock().unwrap().push("tree");
        let mut nodes = vec![faber_engine::eyes::Node { kind: "edit".into(), enabled: true, ..Default::default() }];
        if let Some(inside) = &self.inside {
            let kind = if refused(inside) { "(out of reach)" } else { "pane" };
            nodes.push(faber_engine::eyes::Node { depth: 1, kind: kind.into(), enabled: true, ..Default::default() });
        }
        self.in_tree.store(true, std::sync::atomic::Ordering::SeqCst);
        wait_while(&self.hold_tree);
        Some((nodes, faber_engine::eyes::Walked { count: 1, capped: false }))
    }
}

/// The card waiting on the conversation's newest turn, as (turn, permission), once one is there.
#[cfg(windows)]
async fn card(app: &Engine, id: &str) -> (String, faber_engine::model::Permission) {
    tokio::time::timeout(support::DEADLINE, async {
        loop {
            let found = app.open(id).and_then(|c| {
                c.messages.iter().rev().find_map(|m| m.permissions.iter().flatten().next().map(|p| (m.id.clone(), p.clone())))
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
    let (app, mut pipe, _, _dir, id) = chat(settings(Permissions::Ask)).await;
    let screen = Arc::new(Screen::default());
    app.set_eyes_backend(screen.clone());
    call(&mut pipe, 702, json!({"action": "tree", "window": "0x10"})).await;
    let (turn, card) = card(&app, &id).await;
    assert_eq!(card.tool, "faber.eyes.window", "a new window asks in Ask");
    app.save_settings(Settings { see_other_windows: Some(false), ..Settings::default() }).unwrap();
    let (said, ok) = result(&pipe, 702).await;
    assert!(!ok && said.contains("turned off"), "{said}");
    assert!(app.answer(&id, &turn, "call-702", true, false).await.is_err(), "the card is spent");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(screen.calls().is_empty(), "nothing was read: {:?}", screen.calls());
    app.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn the_list_of_windows_asks_once_a_session_outside_full_access_and_never_in_it() {
    let (app, mut pipe, _, _dir, id) = chat(settings(Permissions::Ask)).await;
    let screen = Arc::new(Screen::default());
    app.set_eyes_backend(screen.clone());
    call(&mut pipe, 703, json!({"action": "windows"})).await;
    let (turn, card) = card(&app, &id).await;
    assert_eq!(card.tool, "faber.eyes.windows");
    assert!(screen.calls().is_empty(), "nothing is listed before the person answers");
    app.answer(&id, &turn, "call-703", true, false).await.unwrap();
    let (said, _) = result(&pipe, 703).await;
    assert_eq!(said, "0x10 notepad.exe \"notes\" 80x60\n");
    let (said, ok) = look(&mut pipe, 704, json!({"action": "windows"})).await;
    assert!(ok && said.contains("notepad.exe"), "the same session is not asked again: {said}");
    app.shutdown();

    let (app, mut pipe, _, _dir, _) = chat(settings(Permissions::FullAccess)).await;
    app.set_eyes_backend(Arc::new(Screen::default()));
    let (said, ok) = look(&mut pipe, 705, json!({"action": "windows"})).await;
    assert!(ok && said.contains("notepad.exe"), "Full access does not ask: {said}");
    let (said, ok) = look(&mut pipe, 706, json!({"action": "tree", "window": "0x10"})).await;
    assert!(ok && said.starts_with("edit"), "{said}");
    let (said, ok) = look(&mut pipe, 707, json!({"action": "picture", "window": "0x10"})).await;
    assert!(!ok && said.contains("tree"), "no picture: {said}");
    app.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn a_key_switched_off_and_on_while_the_window_is_looked_up_still_stops_the_call() {
    let (app, mut pipe, _, _dir, _) = chat(settings(Permissions::FullAccess)).await;
    let screen = Arc::new(Screen { hold_lookup: true.into(), ..Screen::default() });
    app.set_eyes_backend(screen.clone());
    call(&mut pipe, 708, json!({"action": "tree", "window": "0x10"})).await;
    support::until("the window is being looked up", || screen.looked.load(std::sync::atomic::Ordering::SeqCst)).await;
    app.save_settings(Settings { see_other_windows: Some(false), ..Settings::default() }).unwrap();
    app.save_settings(Settings { see_other_windows: Some(true), ..Settings::default() }).unwrap();
    screen.hold_lookup.store(false, std::sync::atomic::Ordering::SeqCst);
    let (said, ok) = result(&pipe, 708).await;
    assert!(!ok && said.contains("turned off"), "{said}");
    assert!(screen.calls().is_empty(), "nothing was read: {:?}", screen.calls());
    app.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn a_program_put_out_of_reach_while_a_tree_reads_its_element_is_never_answered() {
    let (app, mut pipe, _, _dir, _) = chat(settings(Permissions::FullAccess)).await;
    let vault = faber_engine::eyes::Owner { pid: 40, started: 400, path: Some(r"C:\Apps\Vault.exe".into()), elevated: Some(false) };
    let screen = Arc::new(Screen { inside: Some(vault), hold_tree: true.into(), ..Screen::default() });
    app.set_eyes_backend(screen.clone());
    call(&mut pipe, 709, json!({"action": "tree", "window": "0x10"})).await;
    support::until("the tree met the other program", || screen.in_tree.load(std::sync::atomic::Ordering::SeqCst)).await;
    app.save_settings(Settings { programs_out_of_reach: Some(vec!["vault".into()]), ..Settings::default() }).unwrap();
    screen.hold_tree.store(false, std::sync::atomic::Ordering::SeqCst);
    let (said, ok) = result(&pipe, 709).await;
    assert!(!ok && said.contains("kept out of reach"), "{said}");
    // On the list from the start: its element is one line and the rest is read.
    screen.in_tree.store(false, std::sync::atomic::Ordering::SeqCst);
    let (said, ok) = look(&mut pipe, 710, json!({"action": "tree", "window": "0x10"})).await;
    assert!(ok && said == "edit\n  (out of reach)\n", "{said}");
    app.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn a_window_card_saves_the_program_never_the_title_and_a_handle_reused_by_another_window_of_the_process_asks_again() {
    let (app, mut pipe, _, dir, id) = chat(settings(Permissions::Ask)).await;
    let screen = Arc::new(Screen { title: Some("Payroll 2026 draft"), ..Screen::default() });
    app.set_eyes_backend(screen.clone());
    call(&mut pipe, 711, json!({"action": "tree", "window": "0x10"})).await;
    let (turn, first) = card(&app, &id).await;
    assert_eq!(first.input, json!({ "program": "notepad.exe" }), "no title before the person allows");
    let on_disk = |text: &str| {
        let mut stack = vec![dir.path().to_path_buf(), dir.chat_path().to_path_buf()];
        while let Some(at) = stack.pop() {
            for entry in std::fs::read_dir(&at).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if std::fs::read(&path).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(text)) {
                    return true;
                }
            }
        }
        false
    };
    support::until("the card is saved", || on_disk("faber.eyes.window")).await;
    assert!(!on_disk("Payroll 2026"), "the window's title is never written before consent");
    app.answer(&id, &turn, "call-711", true, false).await.unwrap();
    let (said, ok) = result(&pipe, 711).await;
    assert!(ok && said.starts_with("edit"), "{said}");
    let (said, ok) = look(&mut pipe, 712, json!({"action": "tree", "window": "0x10"})).await;
    assert!(ok, "an allowed window is not asked again: {said}");
    assert_eq!(screen.calls(), ["accessibility", "tree", "accessibility", "tree"]);
    // The same process now has another window under that handle: it is asked again, never read on the old Allow.
    *screen.class.lock().unwrap() = Some("#32770".into());
    call(&mut pipe, 713, json!({"action": "tree", "window": "0x10"})).await;
    let (_, again) = card(&app, &id).await;
    assert_eq!((again.tool.as_str(), again.request_id.as_str()), ("faber.eyes.window", "call-713"));
    assert_eq!(screen.calls().len(), 4, "nothing more was read: {:?}", screen.calls());
    app.shutdown();
}
