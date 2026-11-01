//! Settings changes wait for scheduled work; retired app-servers close without a later message or shutdown.
use crate::support::{self, Pipe, next_pipe, pipes};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Conversation, Permissions, ReplyStyle, Settings, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;

fn engine(dir: &crate::support::DataFolder, vendor: Vendor, spawn: Spawner) -> Engine {
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
    billing(&app, vendor, faber_engine::budget::Billing::Subscription);
    app
}

fn billing(app: &Engine, vendor: Vendor, billing: faber_engine::budget::Billing) {
    let plan = faber_engine::signals::Signals { billing: Some(billing), ..Default::default() };
    app.note_account_events(vendor, vec![faber_engine::events::EngineEvent::Signals(plan)]);
}

fn request(vendor: Vendor, area: Area, project: Option<String>, id: Option<String>) -> SendRequest {
    SendRequest {
        conversation_id: id,
        area,
        project_id: project,
        text: "Finish the whole list".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
    }
}

async fn until(app: &Engine, id: &str, ready: impl Fn(&Conversation) -> bool) -> Conversation {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let c = app.open(id).unwrap();
            if ready(&c) {
                return c;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("conversation did not finish its work: {:?}", app.open(id).unwrap()))
}

async fn wire(pipe: &Pipe, ready: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(m) = pipe.lines.lock().unwrap().iter().find(|m| ready(m)).cloned() {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing request on fake wire: {:?}", pipe.lines.lock().unwrap()))
}

async fn closed(pipe: &Pipe) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !pipe.closed.load(SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("retired process stayed open after its last work, before any later message or engine shutdown");
}

fn patch(permissions: bool) -> Settings {
    if permissions {
        Settings { permissions: Some(Permissions::FullAccess), ..Settings::default() }
    } else {
        Settings { reply_style: Some(ReplyStyle::Short), ..Settings::default() }
    }
}

async fn boot(pipe: &mut Pipe) {
    let m = wire(pipe, |m| m["method"] == "initialize").await;
    pipe.say(json!({"id":m["id"],"result":{}})).await;
}

async fn thread(pipe: &mut Pipe, method: &str, id: &str) {
    let m = wire(pipe, |m| m["method"] == method && (method == "thread/start" || m["params"]["threadId"] == id)).await;
    pipe.say(json!({"id":m["id"],"result":{"thread":{"id":id}}})).await;
    let turn = wire(pipe, |m| m["method"] == "turn/start" && m["params"]["threadId"] == id).await;
    pipe.say(json!({"id":turn["id"],"result":{"turn":{"id":format!("turn-{id}")}}})).await;
}

async fn done(pipe: &mut Pipe, vendor: Vendor, id: &str) {
    if vendor == Vendor::Claude {
        pipe.say(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Checked the first part."}]}}))
            .await;
        pipe.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0,"usage":{}})).await;
    } else {
        pipe.say(json!({"method":"item/agentMessage/delta","params":{"threadId":id,"turnId":format!("turn-{id}"),"itemId":"answer","delta":"Checked the first part."}})).await;
        pipe.say(json!({"method":"turn/completed","params":{"threadId":id,"turn":{"id":format!("turn-{id}"),"status":"completed"}}})).await;
    }
}

#[derive(Clone, Copy)]
enum ReplyEnd {
    Done,
    Failed,
    Stop,
}

async fn retired_codex_closes(area: Area, permissions: bool, end: ReplyEnd) {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let (spawn, made) = pipes();
    let app = engine(&dir, Vendor::Codex, spawn);
    app.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    let project = (area == Area::Project).then(|| app.add_project(&folder.path().to_string_lossy()).unwrap().id);
    let req = |id| request(Vendor::Codex, area, project.clone(), id);
    let main = app.send(req(None)).unwrap();
    let mut old = next_pipe(&made).await;
    boot(&mut old).await;
    thread(&mut old, "thread/start", "main").await;
    done(&mut old, Vendor::Codex, "main").await;
    until(&app, &main, |c| c.messages.last().unwrap().status == Some(Status::Done)).await;
    // The peer is left answering on the old server, while main resumes on the new server.
    let first = wire(&old, |m| m["method"] == "thread/start").await;
    let peer = app.send(req(None)).unwrap();
    let start = wire(&old, |m| m["method"] == "thread/start" && m["id"] != first["id"]).await;
    old.say(json!({"id":start["id"],"result":{"thread":{"id":"peer"}}})).await;
    let turn = wire(&old, |m| m["method"] == "turn/start" && m["params"]["threadId"] == "peer").await;
    old.say(json!({"id":turn["id"],"result":{"turn":{"id":"turn-peer"}}})).await;
    app.save_settings(patch(permissions)).unwrap();
    app.send(req(Some(main.clone()))).unwrap();
    let mut new = next_pipe(&made).await;
    boot(&mut new).await;
    thread(&mut new, "thread/resume", "main").await;
    assert!(!old.closed.load(SeqCst));
    assert!(!old.lines.lock().unwrap().iter().any(|m| m["method"] == "turn/interrupt"));
    assert_eq!(app.open(&peer).unwrap().messages.last().unwrap().status, Some(Status::Streaming));
    if matches!(end, ReplyEnd::Stop) {
        app.interrupt(&peer);
        wire(&old, |m| m["method"] == "turn/interrupt" && m["params"]["threadId"] == "peer").await;
        old.say(json!({"method":"turn/completed","params":{"threadId":"peer","turn":{"id":"turn-peer","status":"interrupted"}}})).await;
    } else if matches!(end, ReplyEnd::Failed) {
        old.say(json!({"method":"turn/completed","params":{"threadId":"peer","turn":{"id":"turn-peer","status":"failed","error":{"message":"Test failure"}}}})).await;
    } else {
        done(&mut old, Vendor::Codex, "peer").await;
    }
    closed(&old).await;
    let expected = if matches!(end, ReplyEnd::Done) { Status::Done } else { Status::Stopped };
    assert_eq!(app.open(&peer).unwrap().messages.last().unwrap().status, Some(expected));
    assert!(!new.closed.load(SeqCst), "a peer on the new server is still working");
    done(&mut new, Vendor::Codex, "main").await;
    until(&app, &main, |c| c.messages.last().unwrap().status == Some(Status::Done)).await;
    app.shutdown();
}

#[tokio::test]
async fn retired_codex_closes_after_last_reply_in_chat_and_projects_for_both_settings() {
    for area in [Area::Chat, Area::Project] {
        for permissions in [false, true] {
            for end in [ReplyEnd::Done, ReplyEnd::Failed] {
                retired_codex_closes(area, permissions, end).await;
            }
        }
    }
}

#[tokio::test]
async fn retired_codex_closes_after_stop_in_chat_and_projects_for_both_settings() {
    for area in [Area::Chat, Area::Project] {
        for permissions in [false, true] {
            retired_codex_closes(area, permissions, ReplyEnd::Stop).await;
        }
    }
}

#[tokio::test]
async fn retired_codex_closes_when_keep_going_halts_itself_after_a_rest_for_both_settings() {
    ends_rest(RestEnd::Halt).await;
}

#[tokio::test]
async fn retired_codex_closes_when_archived_during_a_keep_going_rest() {
    ends_rest(RestEnd::Archive).await;
}

#[tokio::test]
async fn retired_codex_closes_when_keep_going_is_turned_off_during_a_rest() {
    ends_rest(RestEnd::Off).await;
}

#[tokio::test]
async fn retired_codex_closes_when_removed_during_a_keep_going_rest() {
    ends_rest(RestEnd::Remove).await;
}

#[derive(Clone, Copy)]
enum RestEnd {
    Halt,
    Archive,
    Off,
    Remove,
}

async fn ends_rest(end: RestEnd) {
    for permissions in [false, true] {
        let dir = support::DataFolder::new();
        let folder = support::work_folder();
        let (spawn, made) = pipes();
        let app = engine(&dir, Vendor::Codex, spawn);
        app.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        app.set_keep_going_pace(0.01);
        let project = app.add_project(&folder.path().to_string_lossy()).unwrap().id;
        let id = app.send(request(Vendor::Codex, Area::Project, Some(project), None)).unwrap();
        let mut old = next_pipe(&made).await;
        boot(&mut old).await;
        thread(&mut old, "thread/start", "main").await;
        app.keep_going(&id, true).unwrap();
        app.save_settings(patch(permissions)).unwrap();
        done(&mut old, Vendor::Codex, "main").await;
        until(&app, &id, |c| c.keep_going.as_ref().is_some_and(|k| k.waiting.as_ref().is_some_and(|w| w.why == "rest"))).await;
        assert!(!old.closed.load(SeqCst), "the scheduled Keep going wake still has work");
        match end {
            // The timer's wake, outside end_turn, finds no unattended subscription and halts by itself.
            RestEnd::Halt => billing(&app, Vendor::Codex, faber_engine::budget::Billing::UsageBilled),
            RestEnd::Archive => app.archive(&id, true).unwrap(),
            RestEnd::Off => app.keep_going(&id, false).unwrap(),
            RestEnd::Remove => app.remove(&id),
        }
        if matches!(end, RestEnd::Remove) {
            assert!(app.open(&id).is_none());
        } else {
            let ended = until(&app, &id, |c| c.keep_going.is_none()).await;
            if matches!(end, RestEnd::Archive) {
                assert!(ended.archived_at.is_some());
            }
            if matches!(end, RestEnd::Halt) {
                assert!(ended.messages.last().unwrap().halt.is_some());
            }
        }
        closed(&old).await;
        assert_eq!(old.lines.lock().unwrap().iter().filter(|m| m["method"] == "turn/start").count(), 1);
        assert!(made.lock().unwrap().is_empty(), "ending a rest starts no new process");
        app.shutdown();
    }
}

#[tokio::test]
async fn retired_codex_closes_when_quota_stops_its_last_turn_in_chat_and_projects() {
    for area in [Area::Chat, Area::Project] {
        let dir = support::DataFolder::new();
        let folder = support::work_folder();
        let (spawn, made) = pipes();
        let app = engine(&dir, Vendor::Codex, spawn);
        let project = (area == Area::Project).then(|| app.add_project(&folder.path().to_string_lossy()).unwrap().id);
        let id = app.send(request(Vendor::Codex, area, project, None)).unwrap();
        let mut old = next_pipe(&made).await;
        boot(&mut old).await;
        thread(&mut old, "thread/start", "main").await;
        // The driver must consume turn/start's response before the engine orders a quota stop.
        old.say(json!({"method":"item/agentMessage/delta","params":{"threadId":"main","turnId":"turn-main","itemId":"answer","delta":"Working"}})).await;
        until(&app, &id, |c| c.messages.last().unwrap().text == "Working").await;
        app.save_settings(patch(false)).unwrap();
        let paid = faber_engine::signals::Signals { overage_active: Some(true), ..Default::default() };
        app.note_account_events(Vendor::Codex, vec![faber_engine::events::EngineEvent::Signals(paid)]);
        wire(&old, |m| m["method"] == "turn/interrupt").await;
        old.say(json!({"method":"turn/completed","params":{"threadId":"main","turn":{"id":"turn-main","status":"interrupted"}}})).await;
        closed(&old).await;
        let c = app.open(&id).unwrap();
        assert_eq!(c.messages.last().unwrap().status, Some(Status::Stopped));
        assert_eq!(c.messages.last().unwrap().error.as_deref(), Some("error.quotaOut"));
        assert!(made.lock().unwrap().is_empty());
        app.shutdown();
    }
}

#[tokio::test]
async fn settings_relaunch_routes_a_delayed_background_start_to_its_retired_owner() {
    use faber_engine::background::State;

    for permissions in [false, true] {
        let dir = support::DataFolder::new();
        let (spawn, made) = pipes();
        let app = engine(&dir, Vendor::Claude, spawn);
        app.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        let id = app.send(request(Vendor::Claude, Area::Chat, None, None)).unwrap();
        let mut old = next_pipe(&made).await;
        wire(&old, |m| m["type"] == "user").await;
        old.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
        old.say(json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"background-call","name":"Bash","input":{"command":"long-task","run_in_background":true}}]}})).await;
        done(&mut old, Vendor::Claude, "main").await;
        until(&app, &id, |c| c.messages.last().unwrap().status == Some(Status::Done)).await;
        assert!(app.background_commands().items.is_empty(), "the CLI has not announced the task yet");

        app.save_settings(patch(permissions)).unwrap();
        app.send(request(Vendor::Claude, Area::Chat, None, Some(id.clone()))).unwrap();
        let mut new = next_pipe(&made).await;
        wire(&new, |m| m["type"] == "user").await;
        new.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
        assert!(!old.closed.load(SeqCst), "the unannounced command still owns its original driver");

        old.say(json!({"type":"system","subtype":"task_started","task_type":"local_bash","task_id":"late-task","tool_use_id":"background-call"})).await;
        let output = wire(&old, |m| m["request"]["subtype"] == "get_task_output").await;
        old.say(json!({"type":"control_response","response":{"subtype":"success","request_id":output["request_id"],"response":{"output":"late output\n","total_bytes":12,"truncated":false}}})).await;
        until(&app, &id, |_| app.background_commands().items.first().is_some_and(|r| r.output == "late output\n")).await;
        let row = app.background_commands().items.remove(0);
        assert_eq!(row.conversation_id, id);
        assert_eq!(row.vendor, Vendor::Claude);
        assert_eq!(row.command, "long-task");
        assert_eq!(row.state, State::Running);
        assert!(row.stoppable);

        let stopping = app.stop_background(&row.id);
        let acknowledge = async {
            let stop = wire(&old, |m| m["request"]["subtype"] == "stop_task").await;
            assert_eq!(stop["request"], json!({"subtype":"stop_task","task_id":"late-task"}));
            old.say(json!({"type":"control_response","response":{"subtype":"success","request_id":stop["request_id"],"response":{}}}))
                .await;
        };
        let (stopped, ()) = tokio::join!(stopping, acknowledge);
        assert_eq!(stopped, Ok(()));
        assert!(new.lines.lock().unwrap().iter().all(|m| m["request"]["subtype"] != "stop_task"));
        old.say(
            json!({"type":"system","subtype":"task_notification","task_id":"late-task","tool_use_id":"background-call","status":"stopped"}),
        )
        .await;
        closed(&old).await;
        assert!(app.background_commands().items.is_empty());
        assert_eq!(app.open(&id).unwrap().messages.last().unwrap().status, Some(Status::Streaming));
        assert!(!new.closed.load(SeqCst));
        done(&mut new, Vendor::Claude, "main").await;
        app.shutdown();
    }
}

#[tokio::test]
async fn settings_relaunch_keeps_background_commands_running_until_they_end() {
    for vendor in Vendor::ALL {
        for permissions in [false, true] {
            let dir = support::DataFolder::new();
            let (spawn, made) = pipes();
            let app = engine(&dir, vendor, spawn);
            app.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
            let id = app.send(request(vendor, Area::Chat, None, None)).unwrap();
            let mut old = next_pipe(&made).await;
            if vendor == Vendor::Codex {
                boot(&mut old).await;
                thread(&mut old, "thread/start", "main").await;
                old.say(json!({"method":"item/started","params":{"threadId":"main","turnId":"turn-main","item":{"type":"commandExecution","id":"background","command":"long-task","status":"inProgress","processId":"opaque-terminal"}}})).await;
                old.say(json!({"method":"item/completed","params":{"threadId":"main","turnId":"turn-main","item":{"type":"commandExecution","id":"background","command":"long-task","status":"completed","processId":"opaque-terminal","exitCode":null}}})).await;
            } else {
                wire(&old, |m| m["type"] == "user").await;
                old.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
                old.say(json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"background","name":"Bash","input":{"command":"long-task","run_in_background":true}}]}})).await;
                old.say(
                    json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"background","content":"Started"}]}}),
                )
                .await;
                old.say(json!({"type":"system","subtype":"task_started","task_type":"local_bash","task_id":"background","tool_use_id":"background"})).await;
            }
            done(&mut old, vendor, "main").await;
            until(&app, &id, |c| c.messages.last().unwrap().status == Some(Status::Done)).await;
            assert_eq!(app.background_commands().items.len(), 1);
            app.save_settings(patch(permissions)).unwrap();
            app.send(request(vendor, Area::Chat, None, Some(id.clone()))).unwrap();
            let mut new = next_pipe(&made).await;
            if vendor == Vendor::Codex {
                boot(&mut new).await;
                thread(&mut new, "thread/resume", "main").await;
                old.say(json!({"method":"item/completed","params":{"threadId":"main","turnId":"turn-main","item":{"type":"commandExecution","id":"background","command":"long-task","status":"completed","processId":"opaque-terminal","exitCode":0}}})).await;
            } else {
                wire(&new, |m| m["type"] == "user").await;
                new.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
                old.say(json!({"type":"system","subtype":"task_notification","task_id":"background","tool_use_id":"background","status":"completed"})).await;
            }
            closed(&old).await;
            assert!(
                old.lines.lock().unwrap().iter().all(|m| {
                    m["method"] != "turn/interrupt"
                        && m["method"] != "thread/backgroundTerminals/terminate"
                        && !matches!(m["request"]["subtype"].as_str(), Some("interrupt" | "stop_task"))
                }),
                "replacing settings must not interrupt the earlier command"
            );
            assert!(app.background_commands().items.is_empty(), "the command's natural completion was delivered");
            assert!(!new.closed.load(SeqCst), "the replacement's reply keeps running");
            assert_eq!(app.open(&id).unwrap().messages.last().unwrap().status, Some(Status::Streaming));
            done(&mut new, vendor, "main").await;
            app.shutdown();
        }
    }
}
