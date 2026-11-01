//! Settings changes wait for scheduled work; retired app-servers close without a later message or shutdown.
use crate::support::{self, Pipe, next_pipe, pipes};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, ChecksState, Conversation, Permissions, ReplyStyle, Settings, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::verify::{Runner, Spec, Started, Verifier};
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
            // These wire scenarios cover conversation turns; auxiliary models are mocked separately.
            ask: Some(Arc::new(|_| Box::pin(async { None }))),
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

async fn edit(pipe: &mut Pipe, vendor: Vendor) {
    if vendor == Vendor::Codex {
        pipe.say(json!({"method":"item/started","params":{"threadId":"main","turnId":"turn-main","item":{"id":"edit","type":"fileChange","changes":[{"path":"a.txt","kind":{"type":"update"},"diff":"+fixed"}]}}})).await;
        pipe.say(json!({"method":"item/completed","params":{"threadId":"main","turnId":"turn-main","item":{"id":"edit","type":"fileChange","status":"completed","changes":[{"path":"a.txt","kind":{"type":"update"},"diff":"+fixed"}]}}})).await;
    } else {
        pipe.say(json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"edit","name":"Edit","input":{"file_path":"a.txt","old_string":"a","new_string":"b"}}]}})).await;
        pipe.say(json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"edit","content":"ok"}]}})).await;
    }
}

struct Gated {
    gate: Arc<tokio::sync::Semaphore>,
    fail_next: std::sync::atomic::AtomicBool,
}

impl Runner for Gated {
    fn start(&self, _: &Spec) -> std::io::Result<Started> {
        let gate = self.gate.clone();
        let code = i32::from(self.fail_next.swap(false, SeqCst));
        Ok(Started {
            confirmed: None,
            throttle: None,
            peak: None,
            stdout: Box::new(tokio::io::empty()),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async move {
                let _permit = gate.acquire().await.unwrap();
                Some(code)
            }),
        })
    }
}

async fn checks_preserve_compaction(vendor: Vendor, permissions: bool, stop: bool, fail_first: bool) {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    std::fs::write(folder.path().join("AGENTS.md"), "# Rules\n\n## Proof commands\n\n```bash\npython check.py\n```\n").unwrap();
    let (spawn, made) = pipes();
    let app = engine(&dir, vendor, spawn);
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    app.set_verifier(Arc::new(Verifier::new(Arc::new(Gated { gate: gate.clone(), fail_next: fail_first.into() }))));
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap().id;
    let id = app.send(request(vendor, Area::Project, Some(project.clone()), None)).unwrap();
    let mut old = next_pipe(&made).await;
    if vendor == Vendor::Codex {
        boot(&mut old).await;
        thread(&mut old, "thread/start", "main").await;
    } else {
        wire(&old, |m| m["type"] == "user").await;
        old.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
    }
    edit(&mut old, vendor).await;
    let next = "Finish the lexer.\nKeep the parser's API; check café.txt and the remaining cases exactly once.";
    let args = json!({"action":"compact","next":next});
    if vendor == Vendor::Codex {
        old.say(json!({"id":900,"method":"item/tool/call","params":{"threadId":"main","turnId":"turn-main","callId":"compact","tool":"conversation","arguments":args}})).await;
        wire(&old, |m| m["id"] == 900 && m.get("result").is_some()).await;
    } else {
        old.say(json!({"type":"control_request","request_id":"compact","request":{"subtype":"mcp_message","server_name":"faber","message":{"jsonrpc":"2.0","id":900,"method":"tools/call","params":{"name":"conversation","arguments":args}}}})).await;
        wire(&old, |m| m["response"]["request_id"] == "compact").await;
    }
    done(&mut old, vendor, "main").await;
    until(&app, &id, |c| c.messages.last().and_then(|m| m.checks.as_ref()).is_some_and(|k| k.state == ChecksState::Running)).await;
    let queued = if stop {
        let mut follow_up = request(vendor, Area::Project, Some(project.clone()), Some(id.clone()));
        follow_up.text = "Check the footer later.".into();
        app.send(follow_up).unwrap();
        let queue = app.open(&id).unwrap().queued;
        assert_eq!(queue.len(), 1);
        Some(queue[0].clone())
    } else {
        None
    };
    // Real settings save, between the reply and its checks ending. A stricter mode has no running turn to interrupt here.
    let change = if permissions { Settings { permissions: Some(Permissions::Ask), ..Settings::default() } } else { patch(false) };
    app.save_settings(change).unwrap();
    // Let the driver process any Close from the save before checking that it is still alive.
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(!old.closed.load(SeqCst), "checks and scheduled compaction are still work");
    if stop {
        app.interrupt(&id);
        closed(&old).await;
        gate.add_permits(1);
        let paused = app.open(&id).unwrap();
        assert!(!paused.compacting);
        assert_eq!(paused.messages.len(), 2, "Stop starts neither compaction nor a continuation");
        let queued = queued.unwrap();
        assert_eq!(paused.queued.as_slice(), std::slice::from_ref(&queued), "Stop leaves the follow-up intact");
        assert!(made.lock().unwrap().is_empty(), "Stop sends no continuation");
        app.send_queued(&id, &queued.id).unwrap();
        let mut new = next_pipe(&made).await;
        let sent = if vendor == Vendor::Codex {
            boot(&mut new).await;
            thread(&mut new, "thread/resume", "main").await;
            support::person_string(wire(&new, |m| m["method"] == "turn/start").await["params"]["input"][0]["text"].as_str().unwrap())
        } else {
            let sent = wire(&new, |m| m["type"] == "user").await;
            new.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
            support::person_string(sent["message"]["content"].as_str().unwrap())
        };
        assert_eq!(sent, queued.text, "the saved follow-up starts later on a new session");
        let resumed = app.open(&id).unwrap();
        assert!(resumed.queued.is_empty());
        assert_eq!(resumed.messages.len(), 4);
        assert_eq!(resumed.messages[2].text, queued.text);
        done(&mut new, vendor, "main").await;
        until(&app, &id, |c| c.messages.last().unwrap().status == Some(Status::Done)).await;
        app.shutdown();
        return;
    }
    gate.add_permits(1);
    let mut correction = if fail_first {
        let mut pipe = next_pipe(&made).await;
        if vendor == Vendor::Codex {
            boot(&mut pipe).await;
            thread(&mut pipe, "thread/resume", "main").await;
            let fix = wire(&pipe, |m| m["method"] == "turn/start").await;
            assert!(fix["params"]["input"][0]["text"].as_str().unwrap().starts_with("Faber ran the project's fast checks"));
        } else {
            let fix = wire(&pipe, |m| m["type"] == "user").await;
            assert!(fix["message"]["content"].as_str().unwrap().starts_with("Faber ran the project's fast checks"));
            pipe.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
        }
        assert_eq!(app.open(&id).unwrap().messages[2].wake.as_ref().unwrap().why, "wake.checksFixing");
        closed(&old).await;
        done(&mut pipe, vendor, "main").await;
        Some(pipe)
    } else {
        None
    };
    let compacting = correction.as_mut().unwrap_or(&mut old);
    if vendor == Vendor::Codex {
        let compact = wire(compacting, |m| m["method"] == "thread/compact/start").await;
        compacting.say(json!({"id":compact["id"],"result":{}})).await;
        compacting
            .say(json!({"method":"turn/started","params":{"threadId":"main","turn":{"id":"compact-turn","status":"inProgress"}}}))
            .await;
        compacting
            .say(json!({"method":"turn/completed","params":{"threadId":"main","turn":{"id":"compact-turn","status":"completed"}}}))
            .await;
    } else {
        wire(compacting, |m| m["type"] == "user" && m["message"]["content"].as_str().is_some_and(|s| s.starts_with("/compact "))).await;
        compacting.say(json!({"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"manual","pre_tokens":90000,"post_tokens":1000}})).await;
        compacting
            .say(json!({"type":"result","subtype":"success","is_error":false,"num_turns":0,"local_command":"compact","usage":{}}))
            .await;
    }
    let mut new = match correction {
        Some(pipe) => pipe,
        None => {
            let mut pipe = next_pipe(&made).await;
            if vendor == Vendor::Codex {
                boot(&mut pipe).await;
                thread(&mut pipe, "thread/resume", "main").await;
            }
            pipe
        }
    };
    let continued = format!("The conversation was compacted. Continue: {next}");
    let sent = if vendor == Vendor::Codex {
        let turn = wire(&new, |m| {
            m["method"] == "turn/start" && support::person_string(m["params"]["input"][0]["text"].as_str().unwrap_or_default()) == continued
        })
        .await;
        if fail_first {
            new.say(json!({"id":turn["id"],"result":{"turn":{"id":"turn-main"}}})).await;
        }
        support::person_string(turn["params"]["input"][0]["text"].as_str().unwrap())
    } else {
        let sent = wire(&new, |m| {
            m["type"] == "user" && support::person_string(m["message"]["content"].as_str().unwrap_or_default()) == continued
        })
        .await;
        support::person_string(sent["message"]["content"].as_str().unwrap())
    };
    assert_eq!(sent, continued);
    let c = app.open(&id).unwrap();
    assert_eq!(c.messages[c.messages.len() - 2].wake.as_ref().unwrap().why, "wake.compacted");
    assert_eq!(c.messages.len(), if fail_first { 6 } else { 4 }, "one continuation after the original reply and any correction");
    closed(&old).await;
    done(&mut new, vendor, "main").await;
    // The continuation is the reply after the person's request and any correction; Faber may then ask once for a conclusion.
    let last = if fail_first { 5 } else { 3 };
    until(&app, &id, |c| c.messages.get(last).is_some_and(|m| m.status == Some(Status::Done))).await;
    app.shutdown();
}

#[tokio::test]
async fn claude_permission_change_during_checks_preserves_compaction_and_exact_next() {
    checks_preserve_compaction(Vendor::Claude, true, false, false).await;
}

#[tokio::test]
async fn claude_reply_style_change_during_checks_preserves_compaction_and_exact_next() {
    checks_preserve_compaction(Vendor::Claude, false, false, false).await;
}

#[tokio::test]
async fn codex_permission_change_during_checks_preserves_compaction_and_exact_next() {
    checks_preserve_compaction(Vendor::Codex, true, false, false).await;
}

#[tokio::test]
async fn codex_reply_style_change_during_checks_preserves_compaction_and_exact_next() {
    checks_preserve_compaction(Vendor::Codex, false, false, false).await;
}

#[tokio::test]
async fn claude_failed_checks_preserve_compaction_across_settings_relaunch() {
    for permissions in [false, true] {
        checks_preserve_compaction(Vendor::Claude, permissions, false, true).await;
    }
}

#[tokio::test]
async fn codex_failed_checks_preserve_compaction_across_settings_relaunch() {
    for permissions in [false, true] {
        checks_preserve_compaction(Vendor::Codex, permissions, false, true).await;
    }
}

#[tokio::test]
async fn claude_stop_during_checks_with_queued_follow_up_closes_retired_process() {
    for permissions in [false, true] {
        checks_preserve_compaction(Vendor::Claude, permissions, true, false).await;
    }
}

#[tokio::test]
async fn codex_stop_during_checks_with_queued_follow_up_closes_retired_process() {
    for permissions in [false, true] {
        checks_preserve_compaction(Vendor::Codex, permissions, true, false).await;
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
async fn a_keep_going_halt_at_turn_end_still_checks_edits_before_retiring() {
    for vendor in Vendor::ALL {
        for failed in [false, true] {
            let dir = support::DataFolder::new();
            let folder = support::work_folder();
            std::fs::write(folder.path().join("AGENTS.md"), "# Rules\n\n## Proof commands\n\n```bash\npython check.py\n```\n").unwrap();
            let (spawn, made) = pipes();
            let app = engine(&dir, vendor, spawn);
            let gate = Arc::new(tokio::sync::Semaphore::new(0));
            app.set_verifier(Arc::new(Verifier::new(Arc::new(Gated { gate: gate.clone(), fail_next: failed.into() }))));
            let project = app.add_project(&folder.path().to_string_lossy()).unwrap().id;
            let id = app.send(request(vendor, Area::Project, Some(project), None)).unwrap();
            let mut old = next_pipe(&made).await;
            if vendor == Vendor::Codex {
                boot(&mut old).await;
                thread(&mut old, "thread/start", "main").await;
            } else {
                wire(&old, |m| m["type"] == "user").await;
                old.say(json!({"type":"system","subtype":"init","session_id":"main","model":"claude-haiku-5-5"})).await;
            }
            app.keep_going(&id, true).unwrap();
            edit(&mut old, vendor).await;
            app.save_settings(patch(false)).unwrap();
            billing(&app, vendor, faber_engine::budget::Billing::UsageBilled);
            done(&mut old, vendor, "main").await;
            until(&app, &id, |c| {
                c.keep_going.is_none() && c.messages.last().and_then(|m| m.checks.as_ref()).is_some_and(|k| k.state == ChecksState::Running)
            })
            .await;
            assert!(!old.closed.load(SeqCst), "a halt leaves the edited reply's checks running");
            gate.add_permits(1);
            let expected = if failed { ChecksState::NeedsYou } else { ChecksState::Passed };
            until(&app, &id, |c| c.messages.last().and_then(|m| m.checks.as_ref()).is_some_and(|k| k.state == expected)).await;
            closed(&old).await;
            app.shutdown();
        }
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
