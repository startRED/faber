use crate::support;
use faber_engine::{
    claude::{ClaudeLaunch, claude_fork_args},
    codex::{CodexServer, CodexServerOptions, CodexThread},
    engine::NativePoint,
    events::{EngineEvent, UserTurn},
    model::{Area, Permissions},
    rewind::Cursor,
};
use serde_json::json;
use std::sync::Arc;
fn thread() -> CodexThread {
    CodexThread {
        temporary: false,
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        title: None,
        instructions: None,
        user_name: None,
        research: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        leave_plan: false,
        tuning: Default::default(),
        helper: Default::default(),
        browse: false,
        conversations: false,
        leads: false,
        verify: false,
        progress: false,
    }
}
#[tokio::test(start_paused = true)]
async fn native_branch_codex_validates_the_exact_tail_before_any_turn_and_never_reuses_the_source() {
    for result in ["success", "source", "wrong", "missing", "unsupported"] {
        let (spawn, mut input, mut output) = support::controlled();
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::Ask,
            cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
            cwd: ".".into(),
            area: Area::Chat,
            user_hooks: false,
            windows: false,
            spawn,
            tools: Default::default(),
        });
        let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        let point = NativePoint { session: "source".into(), cursor: Cursor { entry: None, turn: Some("kept".into()) } };
        let session = server.open_fork(thread(), point, Arc::new(move |e| drop(tx.send(e))));
        session.send(UserTurn::text("try again"));
        support::expect_method(&mut input, "initialize").await;
        support::emit(&mut output, json!({"id":1,"result":{}})).await;
        support::expect_method(&mut input, "initialized").await;
        let fork = support::expect_method(&mut input, "thread/fork").await;
        assert_eq!(fork["params"]["threadId"], "source");
        assert_eq!(fork["params"]["lastTurnId"], "kept");
        assert_eq!(fork["params"]["excludeTurns"], true);
        if result == "unsupported" {
            support::emit(&mut output, json!({"id":fork["id"],"error":{"code":-32601,"message":"unsupported"}})).await;
        } else {
            let id = if result == "source" { "source" } else { "forked" };
            support::emit(&mut output, json!({"id":fork["id"],"result":{"thread":{"id":id}}})).await;
            if result != "source" {
                let tail = support::expect_method(&mut input, "thread/turns/list").await;
                assert_eq!(tail["params"]["limit"], 1);
                assert_eq!(tail["params"]["itemsView"], "summary");
                let data = if result == "missing" {
                    json!([])
                } else {
                    json!([{ "id":if result=="wrong"{"later"}else{"kept"},"status":"completed"}])
                };
                support::emit(&mut output, json!({"id":tail["id"],"result":{"data":data}})).await;
            }
        }
        let mut started = false;
        loop {
            match events.recv().await.unwrap() {
                EngineEvent::Started { session_id, .. } => {
                    assert_eq!(session_id, "forked");
                    started = true;
                    break;
                }
                EngineEvent::Error { fatal: true, .. } => break,
                _ => {}
            }
        }
        assert_eq!(started, result == "success");
        if started {
            support::expect_method(&mut input, "skills/list").await;
            let turn = support::expect_method(&mut input, "turn/start").await;
            assert_eq!(turn["params"]["threadId"], "forked");
        } else {
            assert!(tokio::time::timeout(std::time::Duration::from_millis(1), support::read_message(&mut input)).await.is_err());
        }
        session.close();
        server.retire();
    }
}
#[test]
fn native_branch_claude_uses_a_new_session_and_an_inclusive_validated_entry_only_when_supported() {
    let launch = ClaudeLaunch {
        temporary: false,
        helper: Default::default(),
        browse: false,
        conversations: false,
        leads: false,
        verify: false,
        progress: false,
        area: Area::Chat,
        model: None,
        effort: None,
        resume: None,
        user_hooks: false,
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        flags: Default::default(),
        tuning: Default::default(),
        tools: Default::default(),
    };
    let point = NativePoint {
        session: "13114353-380b-4dcb-8ee7-4df689be7c1d".into(),
        cursor: Cursor { entry: Some("8d292943-1c7f-49e2-9812-64063b277598".into()), turn: None },
    };
    assert!(claude_fork_args(&launch, &point, false).is_err());
    let args = claude_fork_args(&launch, &point, true).unwrap();
    assert!(args.contains(&"--fork-session".into()));
    assert!(args.iter().any(|a| a == "--resume-session-at=8d292943-1c7f-49e2-9812-64063b277598"));
    assert!(args.windows(2).any(|a| a == ["--resume", point.session.as_str()]));
    assert!(claude_fork_args(&launch, &NativePoint { session: "bad".into(), ..point }, true).is_err());
    assert!(faber_engine::claude::supports_fork("  --fork-session  Fork history"));
    assert!(!faber_engine::claude::supports_fork("  --resume <id>  Use --fork-session"));
}
