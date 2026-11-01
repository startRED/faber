use crate::support;

use faber_engine::cli::Cli;
use faber_engine::engine::NewSession;
use faber_engine::model::{Area, Vendor};
use faber_engine::router::Choice;
use faber_engine::sibling::Ai;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

async fn tool(agent: &mut support::Pipe, id: &str, arguments: serde_json::Value) -> serde_json::Value {
    agent.say(json!({"type":"control_request","request_id":id,"request":{"subtype":"mcp_message","server_name":"faber","message":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conversation","arguments":arguments,"_meta":{"claudecode/toolUseId":id}}}}})).await;
    support::until("tool answers", || agent.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == id)).await;
    agent.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == id).unwrap()["response"]["response"]["mcp_response"]["result"].clone()
}

#[tokio::test]
async fn waiting_stops_keep_going_and_prevents_automatic_archiving() {
    let data = support::DataFolder::new();
    let (spawn, made) = support::pipes();
    let engine = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            #[cfg(windows)]
            chat_dir: data.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
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
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(faber_engine::budget::Billing::Subscription),
            ..Default::default()
        })],
    );
    let project = engine.add_project(&data.path().to_string_lossy()).unwrap();
    let id = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "Lead".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let mut agent = support::next_pipe(&made).await;
    agent.say(json!({"type":"system","subtype":"init","session_id":"lead","model":"haiku"})).await;
    engine.set_keep_going_pace(0.001);
    engine.keep_going(&id, true).unwrap();
    engine.session_wait(&id, vec![], Some("review".into())).unwrap();
    agent.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0,"usage":{}})).await;
    support::until("turn ends", || {
        engine.open(&id).unwrap().messages.last().is_some_and(|m| m.status == Some(faber_engine::model::Status::Done))
    })
    .await;
    tokio::time::sleep(Duration::from_millis(350)).await;
    let waiting = engine.open(&id).unwrap();
    assert_eq!(waiting.messages.len(), 2);
    assert!(waiting.queued.is_empty());
    engine.keep_going(&id, false).unwrap();
    engine.archive_at(u64::MAX, None);
    assert!(engine.open(&id).unwrap().archived_at.is_none());
    engine.shutdown();
}

#[tokio::test]
async fn an_agent_can_wait_for_a_child_and_cannot_act_on_itself() {
    let data = support::DataFolder::new();
    let (spawn, made) = support::pipes();
    let opts = EngineOptions {
        data_dir: data.path().into(),
        #[cfg(windows)]
        chat_dir: data.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(|_| {})).unwrap();
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(faber_engine::budget::Billing::Subscription),
            ..Default::default()
        })],
    );
    let project = engine.add_project(&data.path().to_string_lossy()).unwrap();
    let parent = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "Lead".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let mut agent = support::next_pipe(&made).await;
    agent.say(json!({"type":"system","subtype":"init","session_id":"lead","model":"claude-haiku-5-5"})).await;
    let new = |model: Option<&str>, effort: Option<&str>, ai| NewSession {
        title: "Requested".into(),
        brief: "Do it".into(),
        ai,
        model: model.map(str::to_string),
        effort: effort.map(str::to_string),
        keep_going: false,
        step: None,
    };
    assert!(engine.session_start(&parent, new(Some("unknown"), None, Ai::Same)).unwrap_err().contains("model"));
    assert!(engine.session_start(&parent, new(Some("haiku"), None, Ai::Auto)).unwrap_err().contains("auto"));
    assert!(engine.session_start(&parent, new(None, Some("maximum"), Ai::Same)).unwrap_err().contains("effort"));
    for (id, args, error) in [
        (
            "bad-start",
            json!({"action":"start", "title":"Requested", "brief":"Do it", "model":"unknown"}),
            engine.session_start(&parent, new(Some("unknown"), None, Ai::Same)).unwrap_err(),
        ),
        (
            "bad-ask",
            json!({"action":"ask", "id":"missing", "message":"hello"}),
            engine.session_message(&parent, "missing", "hello").unwrap_err(),
        ),
        ("bad-stop", json!({"action":"stop", "id":"missing"}), engine.session_stop(&parent, "missing").unwrap_err()),
        ("bad-resume", json!({"action":"resume", "id":"missing"}), engine.session_resume(&parent, "missing", None).unwrap_err()),
        ("bad-archive", json!({"action":"archive", "id":"missing"}), engine.session_archive(&parent, "missing").unwrap_err()),
        ("bad-wait", json!({"action":"wait", "on":["missing"]}), engine.session_wait(&parent, vec!["missing".into()], None).unwrap_err()),
    ] {
        let answer = tool(&mut agent, id, args).await;
        assert_eq!(answer["isError"], true, "{answer}");
        assert!(answer.to_string().contains(&error), "{answer}: {error}");
    }
    agent.say(json!({"type":"control_request","request_id":"start","request":{"subtype":"mcp_message","server_name":"faber","message":{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"conversation","arguments":{"action":"start","title":"Part","brief":"Do it","model":"haiku","effort":"high"},"_meta":{"claudecode/toolUseId":"start-call"}}}}})).await;
    support::until("start tool answers", || agent.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "start")).await;
    let response = agent.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == "start").cloned().unwrap();
    assert_eq!(response["response"]["response"]["mcp_response"]["result"]["isError"], false, "{response}");
    let child = engine.state().conversations.into_iter().find(|c| c.title == "Part").unwrap().id;
    let chosen = engine.open(&child).unwrap().choice.unwrap();
    assert_eq!(chosen.model.as_deref(), Some("haiku"));
    assert_eq!(chosen.effort, Some(faber_engine::router::Effort::High));
    for id in [&parent[..], "missing"] {
        assert!(engine.session_stop(&parent, id).is_err());
        assert!(engine.session_resume(&parent, id, None).is_err());
        assert!(engine.session_archive(&parent, id).is_err());
        assert!(engine.session_wait(&parent, vec![id.to_string()], None).is_err());
    }
    assert!(engine.session_resume(&parent, &child, None).is_err());
    assert!(engine.session_archive(&parent, &child).is_err());
    assert!(engine.session_wait(&parent, vec![], None).is_err());
    assert!(engine.session_wait(&parent, vec![child.clone(); 13], None).is_err());
    assert!(engine.session_wait(&parent, vec![], Some("a".repeat(201))).is_err());
    assert!(engine.session_wait(&parent, vec![child.clone()], None).is_ok());
    assert_eq!(engine.open(&parent).unwrap().waiting_on.unwrap().ids, vec![child.clone()]);
    let call = json!({"type":"control_request","request_id":"wait","request":{"subtype":"mcp_message","server_name":"faber","message":{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"conversation","arguments":{"action":"wait","on":[child],"note":"waiting"},"_meta":{"claudecode/toolUseId":"wait-call"}}}}});
    agent.say(call).await;
    support::until("wait tool answers", || agent.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "wait")).await;
    let response = agent.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == "wait").cloned().unwrap();
    assert_eq!(response["response"]["response"]["mcp_response"]["result"]["isError"], false, "{response}");
    assert_eq!(engine.open(&parent).unwrap().waiting_on.unwrap().note, "waiting");
    assert!(engine.state().conversations.iter().find(|c| c.id == parent).unwrap().waiting_on.is_some());
    let mut worker = support::next_pipe(&made).await;
    worker.say(json!({"type":"system","subtype":"init","session_id":"worker","model":"claude-haiku-5-5"})).await;
    worker.say(json!({"type":"control_request", "request_id":"question", "request":{"subtype":"can_use_tool", "tool_name":"AskUserQuestion", "input":{"questions":[{"question":"Which?","options":[{"label":"One"}]}]}}})).await;
    support::until("child asks the person", || {
        engine.open(&child).unwrap().messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))
    })
    .await;
    assert!(engine.session_resume(&parent, &child, None).unwrap_err().contains("waiting"));
    assert!(engine.session_archive(&parent, &child).unwrap_err().contains("waiting"));
    worker.say(json!({"type":"stream_event","parent_tool_use_id":null,"event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Work complete."}}})).await;
    worker.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0,"usage":{}})).await;
    support::until("child reports", || {
        engine.open(&parent).unwrap().queued.iter().any(|q| q.wake.as_ref().is_some_and(|w| w.why == "wake.childDone"))
    })
    .await;
    assert!(engine.open(&parent).unwrap().waiting_on.is_none());
    engine.pin(&child, true).unwrap();
    assert!(engine.session_archive(&parent, &child).unwrap_err().contains("pinned"));
    engine.pin(&child, false).unwrap();
    assert!(engine.session_archive(&parent, &child).is_ok());
    assert!(engine.session_resume(&parent, &child, None).unwrap_err().contains("archived"));
    assert!(engine.open(&child).unwrap().archived_at.is_some());
    // A conversation that reported before the wait is not waited for; and an agent never archives itself while the person pins it.
    assert!(engine.session_wait(&parent, vec![child.clone()], None).unwrap().contains("already reported"));
    assert!(engine.open(&parent).unwrap().waiting_on.is_none());
    engine.pin(&parent, true).unwrap();
    let pinned = tool(&mut agent, "archive-itself", json!({"action":"archive"})).await;
    assert_eq!(pinned["isError"], true, "{pinned}");
    assert!(pinned["content"][0]["text"].as_str().unwrap().contains("pinned"), "{pinned}");
    assert!(engine.open(&parent).unwrap().archived_at.is_none());
    engine.pin(&parent, false).unwrap();
    let second = engine.session_start(&parent, new(None, None, Ai::Same)).unwrap();
    let mut stopped = support::next_pipe(&made).await;
    stopped.say(json!({"type":"system","subtype":"init","session_id":"stopped","model":"claude-haiku-5-5"})).await;
    engine
        .send(SendRequest {
            conversation_id: Some(second.clone()),
            area: Area::Project,
            project_id: engine.open(&second).unwrap().project_id,
            text: "Queued by the person".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    assert!(engine.open(&second).unwrap().queued.iter().any(|q| q.text == "Queued by the person"));
    for i in 0..9 {
        engine
            .send(SendRequest {
                conversation_id: Some(second.clone()),
                area: Area::Project,
                project_id: engine.open(&second).unwrap().project_id,
                text: format!("Waiting {i}"),
                images: vec![],
                files: vec![],
                choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
            })
            .unwrap();
    }
    assert!(engine.session_stop(&parent, &second).is_ok());
    support::until("interrupt reaches the child", || stopped.lines.lock().unwrap().iter().any(|m| m["request"]["subtype"] == "interrupt"))
        .await;
    stopped.say(json!({"type":"result","subtype":"error_during_execution","is_error":true,"total_cost_usd":0.0,"usage":{}})).await;
    support::until("child stopped", || {
        engine.open(&second).unwrap().messages.last().is_some_and(|m| m.status == Some(faber_engine::model::Status::Stopped))
    })
    .await;
    support::until("stopped child reports", || {
        engine
            .open(&parent)
            .unwrap()
            .queued
            .iter()
            .any(|q| q.text.contains(&second) && q.wake.as_ref().is_some_and(|w| w.why == "wake.childDone"))
    })
    .await;
    let error = engine.session_resume(&parent, &second, None).unwrap_err();
    assert!(error.contains("inbox is full"), "{error}");
    for item in engine.open(&second).unwrap().queued.iter().skip(1) {
        engine.remove_queued(&second, &item.id);
    }
    assert!(engine.session_resume(&parent, &second, Some("Continue the check.")).is_ok());
    let resumed = engine.open(&second).unwrap();
    assert!(resumed.messages.iter().any(|m| m.text == "Queued by the person"));
    assert_eq!(resumed.messages.last().unwrap().status, Some(faber_engine::model::Status::Streaming));
    assert!(resumed.queued.iter().any(|q| q.text == "Continue the check." && q.wake.as_ref().is_some_and(|w| w.why == "wake.asked")));
    engine.archive_finished(Some(&parent));
    assert!(!engine.state().conversations.iter().find(|c| c.id == parent).unwrap().archived);
    assert!(engine.session_wait(&parent, vec![], Some("waiting for review".into())).is_ok());
    engine.shutdown();
    drop(engine);
    let folder = data.path().join("conversations");
    let mut saved: serde_json::Value = serde_json::from_slice(&std::fs::read(folder.join(format!("{second}.json"))).unwrap()).unwrap();
    let queued = saved["queued"][0].clone();
    saved["queued"] = json!([]);
    saved["id"] = json!("grandchild");
    saved["startedBy"] = json!(second);
    std::fs::write(folder.join("grandchild.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
    saved["id"] = json!("sideways");
    saved.as_object_mut().unwrap().remove("startedBy");
    std::fs::write(folder.join("sideways.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
    for (id, status, closed) in [("cut-off", "stopped", true), ("failed", "error", false), ("stopped-again", "stopped", false)] {
        saved["id"] = json!(id);
        saved["startedBy"] = json!(parent);
        saved["stopped"] = json!(true);
        saved["queued"] = json!([]);
        let reply = saved["messages"].as_array_mut().unwrap().iter_mut().rev().find(|m| m["role"] == "assistant").unwrap();
        reply["status"] = json!(status);
        reply["closed"] = json!(closed);
        std::fs::write(folder.join(format!("{id}.json")), serde_json::to_vec(&saved).unwrap()).unwrap();
    }
    saved["id"] = json!("queued-child");
    saved["queued"] = json!([queued]);
    std::fs::write(folder.join("queued-child.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
    saved["queued"] = json!([]);
    saved["id"] = json!("other-project");
    saved["projectId"] = json!("another-project");
    std::fs::write(folder.join("other-project.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
    let (spawn, reopened_pipes) = support::pipes();
    let opts = EngineOptions {
        data_dir: data.path().into(),
        #[cfg(windows)]
        chat_dir: data.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    let reopened = Engine::new(opts, Arc::new(|_| {})).unwrap();
    reopened.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(faber_engine::budget::Billing::Subscription),
            ..Default::default()
        })],
    );
    assert_eq!(reopened.open(&parent).unwrap().waiting_on.unwrap().note, "waiting for review");
    assert!(reopened.session_archive(&parent, "queued-child").unwrap_err().contains("queued"));
    reopened.archive_at(u64::MAX, Some(&parent));
    assert!(!reopened.state().conversations.iter().find(|c| c.id == parent).unwrap().archived);
    assert!(reopened.session_resume(&second, "grandchild", None).is_ok());
    assert!(reopened.session_wait(&parent, vec!["grandchild".into()], None).is_ok());
    let mut grandchild = support::next_pipe(&reopened_pipes).await;
    grandchild.say(json!({"type":"system","subtype":"init","session_id":"grandchild","model":"claude-haiku-5-5"})).await;
    grandchild.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0,"usage":{}})).await;
    support::until("grandchild reports to waiting ancestor", || reopened.open(&parent).unwrap().waiting_on.is_none()).await;
    let leader = reopened.open(&parent).unwrap();
    assert!(leader.queued.iter().any(|q| q.text.contains("grandchild")) || leader.messages.iter().any(|m| m.text.contains("grandchild")));
    reopened.session_wait(&parent, vec![], Some("waiting again".into())).unwrap();
    assert!(reopened.session_wait(&parent, vec!["sideways".into()], None).is_err());
    assert!(reopened.session_stop("grandchild", &parent).is_err());
    assert!(reopened.session_stop(&parent, "other-project").is_err());
    for id in ["cut-off", "failed", "stopped-again"] {
        reopened.session_resume(&parent, id, None).unwrap();
        assert!(
            reopened.open(id).unwrap().messages.iter().any(|m| m.text == "Continue where you stopped. Check the project's state first."
                && m.wake.as_ref().is_some_and(|w| w.why == "wake.asked"))
        );
    }
    reopened.archive_at(u64::MAX, Some(&parent));
    assert!(!reopened.state().conversations.iter().find(|c| c.id == parent).unwrap().archived);
    let project_id = reopened.open(&parent).unwrap().project_id;
    reopened
        .send(SendRequest {
            conversation_id: Some(parent.clone()),
            area: Area::Project,
            project_id,
            text: "Next turn".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    assert!(reopened.open(&parent).unwrap().waiting_on.is_none());
    reopened.session_wait(&parent, vec![], Some("waiting again".into())).unwrap();
    reopened.archive(&parent, true).unwrap();
    assert!(reopened.open(&parent).unwrap().waiting_on.is_none());
    reopened.shutdown();
}
