use crate::support::{self, DataFolder};
use faber_engine::account::AccountStatus;
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Permissions, Settings, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader, DuplexStream};

fn boot(dir: &DataFolder, permissions: Permissions, spawn: Spawner) -> Engine {
    let engine = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.scratch_path(),
            clis: [(Vendor::Gemini, Cli { command: "fake-agy".into(), prefix: vec![] })].into(),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    engine.save_settings(Settings { permissions: Some(permissions), ..Settings::default() }).unwrap();
    engine.note_antigravity_account(
        AccountStatus::SignedIn,
        Some(faber_engine::signals::antigravity_plan(&json!({
            "_meta":{"antigravity":{"tier":{"name":"Google AI Pro","id":"g1-pro-tier"}}}
        }))),
    );
    engine
}

fn recording(raw: &str) -> Vec<(String, Value)> {
    raw.lines()
        .map(|line| {
            let envelope: Value = serde_json::from_str(line).unwrap();
            (envelope["dir"].as_str().unwrap().into(), envelope["msg"].clone())
        })
        .collect()
}

fn send(engine: &Engine, id: Option<String>, text: &str) -> String {
    send_with_model(engine, id, text, None)
}

fn send_with_model(engine: &Engine, id: Option<String>, text: &str, model: Option<&str>) -> String {
    engine
        .send(SendRequest {
            conversation_id: id,
            area: Area::Chat,
            project_id: None,
            text: text.into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "gemini".into(), model: model.map(str::to_owned), effort: None },
        })
        .unwrap()
}

type Wire = (BufReader<DuplexStream>, DuplexStream);

fn start(dir: &DataFolder, permissions: Permissions) -> (Engine, Wire) {
    let (spawn, input, output) = support::controlled();
    let profile = dir.path().join("antigravity/profile");
    let spawn = Arc::new(move |bin: &str, args: &[String], cwd: &std::path::Path| {
        assert_eq!(bin, "fake-agy");
        assert_eq!(&args[..2], ["acp", "--gemini-dir"]);
        assert_eq!(std::path::Path::new(&args[2]), profile);
        assert!(profile.is_dir());
        assert!(profile.with_file_name("tmp").is_dir());
        let command = faber_engine::proc::marked(bin, args, cwd);
        let variables: std::collections::HashMap<_, _> = command.as_std().get_envs().collect();
        assert_eq!(variables.get(std::ffi::OsStr::new("GEMINI_API_KEY")), Some(&None));
        let temp = std::path::Path::new(variables[std::ffi::OsStr::new("TEMP")].unwrap());
        assert!(temp.is_dir() && temp.parent() == Some(&profile.with_file_name("tmp")), "a temp folder of this process alone");
        spawn(bin, args, cwd)
    });
    (boot(dir, permissions, spawn), (input, output))
}

fn entries(name: &str) -> Vec<(String, Value)> {
    let raw = std::fs::read_to_string(format!("{}/../tests/fixtures/agy/acp-{name}.jsonl", env!("CARGO_MANIFEST_DIR"))).unwrap();
    recording(&raw)
}

fn output(entry: &Value) -> Value {
    serde_json::from_str(entry.as_str().unwrap()).unwrap()
}

fn response(recording: &[(String, Value)], id: i32) -> Value {
    recording.iter().filter(|(dir, _)| dir == "stdout").map(|(_, msg)| output(msg)).find(|msg| msg["id"] == id).unwrap()
}

async fn handshake(wire: &mut Wire, recording: &[(String, Value)], resume: Option<&str>, opened: Value, mode: bool) {
    let hello = support::expect_method(&mut wire.0, "initialize").await;
    assert_eq!(hello["params"]["clientInfo"]["name"], "faber");
    assert!(hello["params"]["clientCapabilities"]["elicitation"]["form"].is_object());
    support::emit(&mut wire.1, response(recording, 1)).await;
    let open = support::expect_method(&mut wire.0, if resume.is_some() { "session/resume" } else { "session/new" }).await;
    assert_eq!(open["params"]["sessionId"].as_str(), resume);
    assert_eq!(open["params"]["mcpServers"], json!([]));
    let mut opened = opened;
    opened["id"] = open["id"].clone();
    support::emit(&mut wire.1, opened).await;
    if mode {
        default_startup(wire).await;
    }
}

async fn family_and_effort(wire: &mut Wire, model: &str, effort: &str) {
    for (option, value) in [("model", model), ("thought_level", effort)] {
        let req = support::expect_method(&mut wire.0, "session/set_config_option").await;
        assert_eq!(req["params"]["configId"], option);
        assert_eq!(req["params"]["value"], value);
        support::emit(&mut wire.1, json!({"jsonrpc":"2.0","id":req["id"],"result":{}})).await;
    }
}

async fn default_startup(wire: &mut Wire) {
    family_and_effort(wire, "gemini-4-hydrogen", "medium").await;
    default_mode(wire).await;
}

async fn default_mode(wire: &mut Wire) {
    let req = support::expect_method(&mut wire.0, "session/set_mode").await;
    assert_eq!(req["params"]["modeId"], "default");
    support::emit(&mut wire.1, json!({"jsonrpc":"2.0","id":req["id"],"result":{}})).await;
}

fn prompt_text(recording: &[(String, Value)]) -> &str {
    recording.iter().find(|(dir, msg)| dir == "in" && msg["method"] == "session/prompt").unwrap().1["params"]["prompt"][0]["text"]
        .as_str()
        .unwrap()
}

async fn replay_turn(wire: &mut Wire, recording: &[(String, Value)], mut answered: Option<tokio::sync::oneshot::Receiver<()>>) {
    let mut turn = false;
    let mut prompt = Value::Null;
    for (dir, entry) in recording {
        match dir.as_str() {
            "in" if entry["method"] == "session/prompt" => {
                turn = true;
                let live = support::expect_method(&mut wire.0, "session/prompt").await;
                assert!(
                    live["params"]["prompt"][0]["text"].as_str().unwrap().ends_with(entry["params"]["prompt"][0]["text"].as_str().unwrap())
                );
                assert_eq!(live["params"]["sessionId"], entry["params"]["sessionId"]);
                prompt = live["id"].clone();
            }
            "in" if turn => {
                let actual = support::read_message(&mut wire.0).await;
                assert_eq!(actual, *entry);
                if let Some(answered) = answered.take() {
                    answered.await.unwrap();
                }
            }
            "stdout" if turn => {
                let mut msg = output(entry);
                if msg.get("id").is_some() && msg.get("method").is_none() {
                    msg["id"] = prompt.clone();
                }
                support::emit(&mut wire.1, msg).await;
            }
            _ => {}
        }
    }
}

async fn settled(engine: &Engine, id: &str) -> faber_engine::model::Message {
    support::until("Antigravity reply settled", || {
        engine.open(id).unwrap().messages.last().is_some_and(|m| m.status.is_some_and(|s| s != Status::Streaming))
    })
    .await;
    engine.open(id).unwrap().messages.pop().unwrap()
}

async fn card(engine: &Engine, id: &str) -> (String, faber_engine::model::Permission) {
    let mut found = None;
    support::until("Antigravity permission card", || {
        let m = engine.open(id).unwrap().messages.pop().unwrap();
        found = m.permissions.and_then(|mut cards| cards.pop()).map(|p| (m.id, p));
        found.is_some()
    })
    .await;
    found.unwrap()
}

async fn recorded(name: &str, permissions: Permissions, answer: Option<bool>) -> faber_engine::model::Message {
    let dir = DataFolder::new();
    let (engine, mut wire) = start(&dir, permissions);
    let recording = entries(name);
    let id = send(&engine, None, prompt_text(&recording));
    handshake(&mut wire, &recording, None, response(&recording, 2), true).await;
    let (release, answered) = tokio::sync::oneshot::channel();
    let replay = tokio::spawn(async move {
        replay_turn(&mut wire, &recording, answer.map(|_| answered)).await;
        wire
    });
    if let Some(allow) = answer {
        let (turn, card) = card(&engine, &id).await;
        if card.tool == "AskUserQuestion" {
            engine.answer_question(&id, &turn, &card.request_id, json!({ "Which color do you prefer?": "blue" })).await.unwrap();
        } else {
            engine.answer(&id, &turn, &card.request_id, allow, false).await.unwrap();
        }
    }
    let _ = release.send(());
    let message = settled(&engine, &id).await;
    let _wire = replay.await.unwrap();
    engine.shutdown();
    message
}

#[tokio::test]
async fn agy_launch_and_recorded_text() {
    let message = recorded("plain", Permissions::Ask, None).await;
    assert_eq!(message.status, Some(Status::Done));
    assert_eq!(message.text.trim(), "pong");
    assert_eq!(Vendor::RUNNABLE.len(), 2);
}

#[tokio::test]
async fn claude_starts_an_antigravity_child_in_a_project_with_three_connected_ais() {
    let dir = DataFolder::new();
    let folder = support::work_folder();
    let (lead_spawn, leads) = support::pipes();
    let (child_spawn, input, output) = support::controlled();
    let spawn: Spawner = Arc::new(move |bin, args, cwd| match bin {
        "claude" => lead_spawn(bin, args, cwd),
        "gemini" => child_spawn(bin, args, cwd),
        _ => panic!("unexpected start of {bin}"),
    });
    let engine = boot(&dir, Permissions::Ask, spawn);
    engine.replace_clis(Vendor::ALL.map(|v| (v, Cli { command: v.name().into(), prefix: vec![] })).into());
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    let parent = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "Delegate the next task".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("claude-haiku-5-5".into()), effort: None },
        })
        .unwrap();
    let mut lead = support::next_pipe(&leads).await;
    lead.say(json!({"type":"system","subtype":"init","session_id":"lead","model":"claude-haiku-5-5"})).await;
    let recording = entries("plain");
    lead.say(json!({"type":"control_request","request_id":"start-child","request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conversation","arguments":{
            "title":"Check the result","brief":prompt_text(&recording),"ai":"antigravity"
        },"_meta":{"claudecode/toolUseId":"child"}}}}}))
        .await;
    support::until("the lead received its child conversation", || {
        lead.lines.lock().unwrap().iter().any(|line| {
            let response = &line["response"];
            if response["request_id"] != "start-child" {
                return false;
            }
            assert_eq!(response["response"]["mcp_response"]["result"]["isError"], false, "{response}");
            true
        })
    })
    .await;
    let child = engine.state().conversations.into_iter().find(|c| c.title == "Check the result").unwrap().id;
    let mut wire = (input, output);
    support::expect_method(&mut wire.0, "initialize").await;
    support::emit(&mut wire.1, response(&recording, 1)).await;
    let open = support::expect_method(&mut wire.0, "session/new").await;
    assert!(open["params"]["mcpServers"].as_array().is_some_and(|servers| !servers.is_empty()));
    support::emit(&mut wire.1, response(&recording, 2)).await;
    default_startup(&mut wire).await;
    replay_turn(&mut wire, &recording, None).await;
    let reply = settled(&engine, &child).await;
    assert_eq!(reply.text.trim(), "pong");
    let conversation = engine.open(&child).unwrap();
    assert_eq!(conversation.started_by.as_deref(), Some(parent.as_str()));
    assert!(conversation.sessions.contains_key(&Vendor::Gemini));
    engine.shutdown();
}

#[tokio::test]
async fn agy_tools_and_full_access() {
    let message = recorded("tools", Permissions::FullAccess, None).await;
    assert_eq!(message.status, Some(Status::Done));
    let tools = message.tools.unwrap();
    for name in ["Read", "Write", "Bash"] {
        assert!(tools.iter().any(|tool| tool.name == name && tool.ok == Some(true)), "{name}: {tools:?}");
    }
}

#[tokio::test]
async fn agy_question_in_both_modes() {
    for permissions in [Permissions::Ask, Permissions::FullAccess] {
        let message = recorded("question-elicitation", permissions, Some(true)).await;
        assert_eq!(message.text.trim(), "blue");
    }
}

#[tokio::test]
async fn agy_approval_and_denial() {
    for (name, allowed) in [("approval-allow", true), ("approval-deny", false)] {
        let message = recorded(name, Permissions::Ask, Some(allowed)).await;
        let tools = message.tools.unwrap();
        assert!(tools.iter().any(|tool| tool.name == "Bash" && tool.ok == Some(allowed) && tool.declined != allowed));
    }
}

#[tokio::test]
async fn agy_early_exit() {
    let dir = DataFolder::new();
    let (engine, mut wire) = start(&dir, Permissions::FullAccess);
    let recording = entries("killed");
    let id = send(&engine, None, prompt_text(&recording));
    handshake(&mut wire, &recording, None, response(&recording, 2), true).await;
    replay_turn(&mut wire, &recording[..recording.iter().position(|(dir, _)| dir == "exit").unwrap()], None).await;
    drop(wire.1);
    let message = settled(&engine, &id).await;
    assert_eq!(message.error.as_deref(), Some("error.driverStopped"));
    engine.shutdown();
}

#[tokio::test]
async fn agy_stop_ends_unresponsive_child_and_keeps_prompt() {
    let dir = DataFolder::new();
    let (engine, mut wire) = start(&dir, Permissions::Ask);
    let recording = entries("cancel");
    let text = prompt_text(&recording);
    let id = send(&engine, None, text);
    handshake(&mut wire, &recording, None, response(&recording, 2), true).await;
    let prompt = support::expect_method(&mut wire.0, "session/prompt").await;
    tokio::time::pause();
    engine.interrupt(&id);
    let cancel = support::expect_method(&mut wire.0, "session/cancel").await;
    assert_eq!(cancel["params"]["sessionId"], prompt["params"]["sessionId"]);
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(wire.0.read_line(&mut String::new()).await.unwrap(), 0, "the fake child's input closed");
    tokio::time::resume();
    let message = settled(&engine, &id).await;
    assert_eq!(message.status, Some(Status::Stopped));
    assert_eq!(engine.open(&id).unwrap().messages[0].text, text);
    engine.shutdown();
}

#[tokio::test]
async fn agy_resume_and_wrong_identity() {
    let dir = DataFolder::new();
    let recording = entries("resume");
    let first_exit = recording.iter().position(|(dir, _)| dir == "exit").unwrap();
    let (first, later) = recording.split_at(first_exit);
    let (engine, mut wire) = start(&dir, Permissions::Ask);
    let opened = response(first, 2);
    let native = opened["result"]["sessionId"].as_str().unwrap().to_owned();
    let id = send(&engine, None, prompt_text(first));
    handshake(&mut wire, first, None, opened, true).await;
    replay_turn(&mut wire, first, None).await;
    assert_eq!(settled(&engine, &id).await.text.trim(), "ok");
    engine.shutdown();
    drop(engine);
    drop(wire);

    let (engine, mut wire) = start(&dir, Permissions::Ask);
    send(&engine, Some(id.clone()), prompt_text(later));
    handshake(&mut wire, later, Some(&native), response(later, 3), true).await;
    replay_turn(&mut wire, later, None).await;
    assert_eq!(settled(&engine, &id).await.text.trim(), "kiwi");
    assert_eq!(engine.open(&id).unwrap().sessions[&Vendor::Gemini], native);
    engine.shutdown();
    drop(engine);
    drop(wire);

    let (engine, mut wire) = start(&dir, Permissions::Ask);
    send(&engine, Some(id.clone()), "try the saved conversation");
    let unknown = entries("resume-unknown");
    handshake(&mut wire, &unknown, Some(&native), response(&unknown, 4), false).await;
    assert_eq!(wire.0.read_line(&mut String::new()).await.unwrap(), 0, "no prompt sent to the replacement session");
    assert_eq!(settled(&engine, &id).await.error.as_deref(), Some("error.sessionLost"));
    engine.shutdown();
}

#[tokio::test]
async fn agy_quota_marks_account() {
    let dir = DataFolder::new();
    let (engine, mut wire) = start(&dir, Permissions::Ask);
    let recording = entries("plain");
    let id = send(&engine, None, "check quota");
    handshake(&mut wire, &recording, None, response(&recording, 2), true).await;
    let req = support::expect_method(&mut wire.0, "session/prompt").await;
    let mut error = response(&entries("synthetic-quota"), 4);
    error["id"] = req["id"].clone();
    support::emit(&mut wire.1, error).await;
    assert_eq!(settled(&engine, &id).await.error.as_deref(), Some("error.quotaOut"));
    assert_eq!(engine.quota_view().1.into_iter().find(|u| u.vendor == Vendor::Gemini).unwrap().state, "out");
    engine.shutdown();

    // The same error while the session opens marks the account too.
    let dir = DataFolder::new();
    let (engine, mut wire) = start(&dir, Permissions::Ask);
    let id = send(&engine, None, "check quota");
    handshake(&mut wire, &recording, None, response(&entries("synthetic-quota"), 4), false).await;
    assert_eq!(settled(&engine, &id).await.error.as_deref(), Some("error.quotaOut"));
    assert_eq!(engine.quota_view().1.into_iter().find(|u| u.vendor == Vendor::Gemini).unwrap().state, "out");
    engine.shutdown();
}

#[tokio::test]
async fn agy_tools_offered_in_full_access_only() {
    let plain = entries("plain");
    for (permissions, offered) in [(Permissions::FullAccess, true), (Permissions::Ask, false)] {
        let dir = DataFolder::new();
        let (spawn, mut input, mut output) = support::controlled();
        let server = faber_engine::tools::Launch {
            name: "faber-measure".into(),
            command: "C:/node/node.exe".into(),
            args: vec!["run.js".into()],
            env: [("NO_STATS".to_string(), "1".to_string())].into(),
        };
        let options = faber_engine::agy::session::AgyOptions {
            bin: "fake-agy".into(),
            cwd: dir.path().into(),
            data: dir.path().into(),
            model: None,
            effort: "medium".into(),
            resume: None,
            permissions,
            builtin: None,
            leads: false,
            spawn,
        };
        let session = faber_engine::agy::session::start(options, &[server], Arc::new(|_| {})).unwrap();
        support::expect_method(&mut input, "initialize").await;
        support::emit(&mut output, response(&plain, 1)).await;
        let open = support::expect_method(&mut input, "session/new").await;
        let launch = json!({"name":"faber-measure","command":"C:/node/node.exe","args":["run.js"],"env":[{"name":"NO_STATS","value":"1"}]});
        assert_eq!(open["params"]["mcpServers"], if offered { json!([launch]) } else { json!([]) }, "{permissions:?}");
        drop(session);
    }
}

struct ProjectTools;

impl faber_engine::browser::Browser for ProjectTools {
    fn act(
        &self,
        _: String,
        _: String,
        _: faber_engine::browser::Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<faber_engine::browser::Page, String>> + Send>> {
        panic!("listing tools does not open a browser");
    }
}

impl faber_engine::verify::Runner for ProjectTools {
    fn start(&self, _: &faber_engine::verify::Spec) -> std::io::Result<faber_engine::verify::Started> {
        panic!("listing tools does not run project checks");
    }
}

#[tokio::test]
async fn agy_project_tools_follow_permissions_and_write_the_projects_plan() {
    tokio::time::timeout(support::DEADLINE, async {
        for permissions in [Permissions::Ask, Permissions::FullAccess] {
            let dir = DataFolder::new();
            let (engine, mut wire) = start(&dir, permissions);
            engine.set_browser(Arc::new(ProjectTools));
            engine.set_verifier(Arc::new(faber_engine::verify::Verifier::new(Arc::new(ProjectTools))));
            let folder = tempfile::tempdir().unwrap();
            let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
            let id = engine
                .send(SendRequest {
                    conversation_id: None,
                    area: Area::Project,
                    project_id: Some(project.id),
                    text: "Make a plan".into(),
                    images: vec![],
                    files: vec![],
                    choice: Choice { vendor: "gemini".into(), model: None, effort: None },
                })
                .unwrap();
            let plain = entries("plain");
            support::expect_method(&mut wire.0, "initialize").await;
            support::emit(&mut wire.1, response(&plain, 1)).await;
            let open = support::expect_method(&mut wire.0, "session/new").await;
            let launch = &open["params"]["mcpServers"][0];
            let name = launch["name"].as_str().unwrap();
            let port = launch["args"][1].as_str().unwrap().parse::<u16>().unwrap();
            let stream = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut replies = BufReader::new(reader).lines();
            use tokio::io::AsyncWriteExt;
            writer.write_all(launch["env"][0]["value"].as_str().unwrap().as_bytes()).await.unwrap();
            faber_engine::proc::write_line(&mut writer, &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})).await.unwrap();
            let listing: Value = serde_json::from_str(&replies.next_line().await.unwrap().unwrap()).unwrap();
            let tools = listing["result"]["tools"].as_array().unwrap();
            let has = |name: &str| tools.iter().any(|tool| tool["name"] == name);
            assert!(["plan", "helper", "conversation"].into_iter().all(has));
            for name in ["browse", "verify", "map"] {
                assert_eq!(has(name), permissions == Permissions::FullAccess, "{name}");
            }
            let opened = response(&plain, 2);
            support::emit(&mut wire.1, opened.clone()).await;
            default_startup(&mut wire).await;
            support::expect_method(&mut wire.0, "session/prompt").await;
            let args = json!({"steps":[{"id":"inspect","text":"Inspect the project","state":"in_progress"}]});
            support::emit(
                &mut wire.1,
                json!({
                    "jsonrpc":"2.0","id":"approval","method":"session/request_permission",
                    "params":{"sessionId":opened["result"]["sessionId"],
                        "toolCall":{"toolCallId":"plan-step","name":format!("mcp_{name}_plan"),"kind":"other","rawInput":{"toolSummary":"Planning the project"}},
                        "options":[{"optionId":"allow","kind":"allow_once"}]}
                }),
            )
            .await;
            let mut reply = String::new();
            wire.0.read_line(&mut reply).await.unwrap();
            assert_eq!(serde_json::from_str::<Value>(&reply).unwrap()["result"]["outcome"]["optionId"], "allow");
            faber_engine::proc::write_line(
                &mut writer,
                &json!({
                    "jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"plan","arguments":args}
                }),
            )
            .await
            .unwrap();
            let answered: Value = serde_json::from_str(&replies.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(answered["result"]["isError"], false, "{answered}");
            let saved = engine.open(&id).unwrap();
            assert_eq!(saved.plan.unwrap().steps[0].text, "Inspect the project");
            let steps = saved.messages.last().unwrap().tools.as_ref().unwrap();
            let plan_steps: Vec<_> = steps.iter().filter(|step| step.name == "Plan").collect();
            assert_eq!(plan_steps.len(), 1);
            assert!(plan_steps[0].id.ends_with(":plan-step"));
            assert_eq!(plan_steps[0].input, args);
            engine.shutdown();
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn agy_declines_a_question_it_cannot_show() {
    let dir = DataFolder::new();
    let (engine, mut wire) = start(&dir, Permissions::Ask);
    let recording = entries("plain");
    send(&engine, None, "ask me");
    handshake(&mut wire, &recording, None, response(&recording, 2), true).await;
    support::expect_method(&mut wire.0, "session/prompt").await;
    let ask = json!({"jsonrpc":"2.0","id":77,"method":"elicitation/create","params":{"requestedSchema":{"properties":{}}}});
    support::emit(&mut wire.1, ask).await;
    let mut line = String::new();
    wire.0.read_line(&mut line).await.unwrap();
    let answer: Value = serde_json::from_str(&line).unwrap();
    assert_eq!((&answer["id"], &answer["result"]["action"]), (&json!(77), &json!("decline")));
    engine.shutdown();
}

#[cfg(windows)]
#[tokio::test]
async fn agy_refuses_credential_path_through_junction() {
    let dir = DataFolder::new();
    let scratch = tempfile::tempdir().unwrap();
    let protected = scratch.path().join(".codex");
    let alias = scratch.path().join("alias");
    std::fs::create_dir(&protected).unwrap();
    junction::create(&protected, &alias).unwrap();
    // The second round names no target of its own, only another argument that looks like one.
    // The third names a harmless path beside its real target, under a name and kind the reader does not know.
    let mut cases = vec![
        (Permissions::Ask, 0, &alias),
        (Permissions::FullAccess, 0, &alias),
        (Permissions::Ask, 1, &alias),
        (Permissions::Ask, 2, &alias),
    ];
    let short = scratch.path().join("CODEX~1");
    if short.canonicalize().ok() == Some(protected.canonicalize().unwrap()) {
        cases.extend([(Permissions::Ask, 0, &short), (Permissions::FullAccess, 0, &short)]);
    }
    for (permissions, masked, source) in cases {
        let (engine, mut wire) = start(&dir, permissions);
        let recording = entries("guard-write-junction");
        let opened = response(&recording, 2);
        let session = opened["result"]["sessionId"].clone();
        let id = send(&engine, None, "Check the junction");
        handshake(&mut wire, &recording, None, opened, true).await;
        let prompt = support::expect_method(&mut wire.0, "session/prompt").await;
        let mut request = recording
            .iter()
            .filter(|(dir, _)| dir == "stdout")
            .map(|(_, msg)| output(msg))
            .find(|msg| msg["method"] == "session/request_permission" && msg["params"]["toolCall"]["kind"] == "read")
            .unwrap();
        request["params"]["sessionId"] = session;
        let path = source.join("auth.json");
        request["params"]["toolCall"]["rawInput"]["Path"] = json!(path);
        request["params"]["toolCall"]["locations"] = json!([{"path":path}]);
        if masked > 0 {
            request["params"]["toolCall"]["rawInput"] = json!({ "file_path": scratch.path().join("harmless.txt") });
        }
        if masked == 2 {
            request["params"]["toolCall"]["rawInput"]["Path"] = json!(path);
            request["params"]["toolCall"]["name"] = json!("Write");
            request["params"]["toolCall"]["kind"] = json!("other");
        }
        support::emit(&mut wire.1, request).await;
        let reply = support::read_message(&mut wire.0).await;
        assert_eq!(reply["result"]["outcome"]["optionId"], "deny");
        assert!(engine.open(&id).unwrap().messages.last().unwrap().permissions.as_ref().is_none_or(Vec::is_empty));
        support::emit(&mut wire.1, json!({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}})).await;
        settled(&engine, &id).await;
        engine.shutdown();
    }
    junction::delete(&alias).unwrap();
}

#[tokio::test]
async fn agy_sets_listed_model_or_family_and_effort_before_prompt() {
    for model in ["gemini-3.8-flash-low", "gemini-3.8-flash"] {
        let dir = DataFolder::new();
        let (engine, mut wire) = start(&dir, Permissions::Ask);
        let recording = entries("plain");
        let id = send_with_model(&engine, None, prompt_text(&recording), Some(model));
        handshake(&mut wire, &recording, None, response(&recording, 2), false).await;
        if model.ends_with("-low") {
            let req = support::expect_method(&mut wire.0, "session/set_model").await;
            assert_eq!(req["params"]["modelId"], model);
            support::emit(&mut wire.1, json!({"jsonrpc":"2.0","id":req["id"],"result":{}})).await;
        } else {
            family_and_effort(&mut wire, model, "medium").await;
        }
        default_mode(&mut wire).await;
        replay_turn(&mut wire, &recording, None).await;
        assert_eq!(settled(&engine, &id).await.text.trim(), "pong");
        engine.shutdown();
    }
}
