use crate::support;

use faber_engine::claude::{ClaudeFlags, ClaudeLaunch, ClaudeOptions, claude_args, parse_claude_message, start_claude};
use faber_engine::events::{Delivery, EngineEvent, UserTurn};
use faber_engine::model::{Area, Permissions};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, drive, fake, fixture, next_permission, text_of};

fn launch(area: Area) -> ClaudeLaunch {
    ClaudeLaunch {
        area,
        model: None,
        effort: None,
        resume: None,
        user_hooks: false,
        instructions: None,
        user_name: None,
        permissions: Permissions::Ask,
        flags: Default::default(),
    }
}

#[tokio::test(start_paused = true)]
async fn opus_1_manual_turns_may_be_silent_for_a_long_tool() {
    for area in [Area::Chat, Area::Project] {
        let (spawn, mut input, _output) = support::controlled();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let session = start_claude(
            ClaudeOptions { bin: "fake".into(), cwd: ".".into(), launch: launch(area), spawn, sandbox: None },
            Arc::new(move |e| drop(tx.send(e))),
        );
        session.send(UserTurn::text("long tool"));
        assert_eq!(support::read_message(&mut input).await["type"], "user");
        tokio::time::advance(Duration::from_secs(600)).await;
        tokio::task::yield_now().await;
        assert!(rx.try_recv().is_err(), "a manual silent turn was interrupted");
        session.send(UserTurn::text("still available"));
        assert_eq!(support::read_message(&mut input).await["message"]["content"], "still available");
        session.close();
    }
}

#[tokio::test(start_paused = true)]
async fn opus_4_interrupt_after_completion_keeps_the_warm_session() {
    let (spawn, mut input, mut output) = support::controlled();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let session = start_claude(
        ClaudeOptions { bin: "fake".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn, sandbox: None },
        Arc::new(move |e| drop(tx.send(e))),
    );
    session.send(UserTurn::text("first"));
    support::read_message(&mut input).await;
    support::emit(&mut output, serde_json::json!({"type":"result","subtype":"success"})).await;
    assert!(matches!(rx.recv().await, Some(EngineEvent::Done { .. })));
    assert!(matches!(rx.recv().await, Some(EngineEvent::Signals(s)) if s.billing == Some(faber_engine::budget::Billing::Unknown)));
    session.interrupt();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(6)).await;
    tokio::task::yield_now().await;
    while let Ok(event) = rx.try_recv() {
        assert!(
            matches!(event, EngineEvent::Signals(s) if s.billing == Some(faber_engine::budget::Billing::Unknown)),
            "idle interrupt produced a lifecycle event"
        );
    }
    session.send(UserTurn::text("second"));
    assert_eq!(support::read_message(&mut input).await["message"]["content"], "second");
    session.close();
}

#[tokio::test]
async fn review_7_claude_preserves_a_turn_sent_before_interrupt_acknowledgement() {
    let (spawn, mut input, mut output) = support::controlled();
    let session = start_claude(
        ClaudeOptions { bin: "fake".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn, sandbox: None },
        Arc::new(|_| {}),
    );
    session.send(UserTurn::text("first"));
    let first = support::read_message(&mut input).await;
    assert_eq!(first["type"], "user");
    session.interrupt();
    assert_eq!(support::read_message(&mut input).await["request"]["subtype"], "interrupt");
    session.send(UserTurn::text("second"));
    tokio::time::sleep(Duration::from_millis(10)).await;
    support::emit(&mut output, serde_json::json!({"type":"result", "subtype":"interrupted", "is_error":true})).await;
    assert_eq!(support::read_message(&mut input).await["message"]["content"], "second");
    session.close();
}

#[tokio::test(start_paused = true)]
async fn waiting_for_a_person_is_not_cli_silence() {
    let f = fake(fixture("claude", "tools"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = ClaudeOptions { bin: "fake".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn: f.spawn.clone(), sandbox: None };
    let session = start_claude(opts, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text(
        "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
    ));
    let request = next_permission(&mut rx).await;
    tokio::time::advance(Duration::from_secs(130)).await;
    tokio::task::yield_now().await;
    assert_eq!(f.wire.lines_with("\"subtype\":\"interrupt\""), 0);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    let request = next_permission(&mut rx).await;
    assert_eq!(session.answer(&request, false).await, Ok(Delivery::Delivered));
    while !matches!(rx.recv().await, Some(EngineEvent::Done { .. })) {}
    session.close();
    assert!(f.mismatches.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_ignored_interrupt_ends_the_owned_pipe_without_waiting_forever() {
    let entries = vec![
        Entry { input: true, msg: serde_json::json!({"type":"user", "message":{"content":"work"}}) },
        Entry { input: false, msg: serde_json::json!({"type":"system", "subtype":"init", "session_id":"ignored", "model":"test"}) },
        Entry { input: true, msg: serde_json::json!({"type":"control_request", "request":{"subtype":"interrupt"}}) },
    ];
    let f = fake(entries);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let session = start_claude(
        ClaudeOptions { bin: "fake".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn: f.spawn.clone(), sandbox: None },
        Arc::new(move |e| drop(tx.send(e))),
    );
    session.send(UserTurn::text("work"));
    assert!(matches!(rx.recv().await, Some(EngineEvent::Started { .. })));
    session.interrupt();
    let event = tokio::time::timeout(Duration::from_secs(10), rx.recv()).await.unwrap().unwrap();
    assert!(f.mismatches.lock().unwrap().is_empty());
    assert!(matches!(event, EngineEvent::Error { message, fatal: true } if message == "error.cli.interruptTimeout"));
}

async fn run(
    entries: Vec<Entry>,
    first: &str,
    react: impl FnMut(&EngineEvent, &faber_engine::events::Session) -> bool,
) -> (Vec<EngineEvent>, Vec<String>) {
    run_with(launch(Area::Chat), entries, first, react).await
}

async fn run_with(
    launch: ClaudeLaunch,
    entries: Vec<Entry>,
    first: &str,
    react: impl FnMut(&EngineEvent, &faber_engine::events::Session) -> bool,
) -> (Vec<EngineEvent>, Vec<String>) {
    drive(
        entries,
        |spawn, sink| start_claude(ClaudeOptions { bin: "claude".into(), cwd: ".".into(), launch, spawn, sandbox: None }, sink),
        first,
        react,
    )
    .await
}

fn recorded_text(entries: &[Entry]) -> String {
    entries
        .iter()
        .filter(|e| e.msg["type"] == "assistant")
        .flat_map(|e| e.msg["message"]["content"].as_array().cloned().unwrap_or_default())
        .filter(|c| c["type"] == "text")
        .map(|c| c["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

async fn check_chat(entries: Vec<Entry>) {
    let expected = recorded_text(&entries);
    let (events, mismatches) = run(entries, "In one sentence: why is the sky blue?", |e, _| matches!(e, EngineEvent::Done { .. })).await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(
        events[0],
        EngineEvent::Started { session_id: "c5b9834c-dfc7-4086-beed-2212be93b722".into(), model: "claude-haiku-5-5".into() }
    );
    assert_eq!(text_of(&events), expected);
    assert!(events.iter().any(|e| matches!(e, EngineEvent::Quota { window, used, .. } if window == "five_hour" && *used == 0.07)));
    match events.last() {
        Some(EngineEvent::Done { ok: true, cost_total_usd: Some(cost), output_tokens: 73, .. }) => assert!(*cost > 0.0),
        other => panic!("unexpected end: {other:?}"),
    }
}

#[tokio::test]
async fn chat_streams_text_model_quota_and_cost_from_a_recorded_session() {
    check_chat(fixture("claude", "chat")).await;
}

#[tokio::test]
#[should_panic]
async fn a_renamed_field_in_a_recorded_session_turns_the_check_red() {
    let entries = fixture("claude", "chat")
        .into_iter()
        .map(|mut e| {
            if e.msg["subtype"] == "init" {
                let id = e.msg["session_id"].take();
                e.msg["sessionId"] = id;
            }
            e
        })
        .collect();
    check_chat(entries).await;
}

#[tokio::test]
async fn permissions_the_first_write_is_allowed_and_the_second_denied() {
    let mut asks = Vec::new();
    let (events, mismatches) = run(
        fixture("claude", "tools"),
        "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
        |e, s| {
            if let EngineEvent::Permission { request_id, tool, .. } = e {
                asks.push(tool.clone());
                drop(s.answer(request_id, asks.len() == 1));
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(asks, ["Write", "Write"]);
    let results: Vec<(bool, bool)> = events
        .iter()
        .filter_map(|e| if let EngineEvent::ToolResult { ok, declined, .. } = e { Some((*ok, *declined)) } else { None })
        .collect();
    assert_eq!(results, [(true, false), (false, true)], "a refused approval is declined, not a failure");
    // The recorded result lists the refusal in `permission_denials`; with the host answering that is the person's own No, not a
    // request nobody could answer, so nothing is parked.
    assert!(events.iter().all(|e| !matches!(e, EngineEvent::Parked { .. })));
}

#[tokio::test]
async fn an_answer_that_fails_to_write_keeps_the_request_and_the_retry_is_written_once() {
    let f = fake(fixture("claude", "tools"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = ClaudeOptions { bin: "claude".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn: f.spawn.clone(), sandbox: None };
    let session = start_claude(opts, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text(
        "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
    ));
    let request = next_permission(&mut rx).await;
    let written = || f.wire.lines_with("control_response");

    f.wire.refuse_next(1);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed));
    assert_eq!(written(), 0, "the refused write left nothing on the wire");
    // The session still holds the request, so the same answer can be given again.
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    assert_eq!(written(), 1);
    // A request the session never held, and one it has already delivered, write nothing.
    assert_eq!(session.answer("never-asked", true).await, Ok(Delivery::Unknown));
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Unknown));
    assert_eq!(written(), 1);
    session.close();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
}

/// A write the pipe cut short, or a flush that failed, may have put the line (or part of it) in front of the CLI, so
/// the session must not send it again: it ends the process, and the retry writes nothing.
#[tokio::test]
async fn an_answer_that_may_be_half_written_is_never_written_again() {
    for tear in [true, false] {
        let f = fake(fixture("claude", "tools"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let opts =
            ClaudeOptions { bin: "claude".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn: f.spawn.clone(), sandbox: None };
        let session = start_claude(opts, Arc::new(move |e| drop(tx.send(e))));
        session.send(UserTurn::text(
            "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
        ));
        let request = next_permission(&mut rx).await;
        let before = f.wire.len();

        if tear {
            f.wire.tear_next(10);
        } else {
            f.wire.refuse_next_flush();
        }
        assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed), "tear: {tear}");
        let after = f.wire.len();
        assert!(after > before, "the first attempt put bytes on the wire (tear: {tear})");

        // The same answer again must not write a byte: the session is ending.
        assert_ne!(session.answer(&request, true).await, Ok(Delivery::Delivered), "tear: {tear}");
        assert_eq!(f.wire.len(), after, "nothing was written again (tear: {tear})");
        let ended = loop {
            match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("the session did not end") {
                Some(EngineEvent::Error { fatal: true, .. }) | None => break true,
                Some(_) => {}
            }
        };
        assert!(ended);
    }
}

#[tokio::test]
async fn interrupt_stops_the_turn_and_the_same_session_takes_the_next_one() {
    let (mut deltas, mut dones) = (0, 0);
    let (events, mismatches) = run(fixture("claude", "interrupt"), "Write a 600-word essay about the history of bread.", |e, s| {
        if matches!(e, EngineEvent::Text(_)) {
            deltas += 1;
            if deltas == 5 {
                s.interrupt();
            }
        }
        if matches!(e, EngineEvent::Done { .. }) {
            dones += 1;
            if dones == 1 {
                s.send(UserTurn::text("Reply with just the word: pong"));
            }
        }
        dones == 2
    })
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    let done: Vec<(usize, bool)> =
        events.iter().enumerate().filter_map(|(i, e)| if let EngineEvent::Done { ok, .. } = e { Some((i, *ok)) } else { None }).collect();
    assert_eq!(done.iter().map(|d| d.1).collect::<Vec<_>>(), [false, true]);
    assert!(text_of(&events[done[0].0..]).contains("pong"));
}

#[tokio::test]
async fn resume_a_new_process_continues_the_same_session() {
    let (events, mismatches) =
        run(fixture("claude", "resume"), "What single word did you reply with last time? Answer with only that word.", |e, _| {
            matches!(e, EngineEvent::Done { .. })
        })
        .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(
        events[0],
        EngineEvent::Started { session_id: "3ee0bd99-72d2-4c5b-8420-a9f85ee53981".into(), model: "claude-haiku-5-5".into() }
    );
    assert_eq!(text_of(&events).trim(), "pong");
}

#[test]
fn chat_is_lean_and_confined_to_its_workspace_projects_get_the_user_setup_without_hooks() {
    let after = |args: &[String], flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    let chat = claude_args(&ClaudeLaunch { resume: Some("abc".into()), model: Some("haiku".into()), ..launch(Area::Chat) });
    assert!(chat.contains(&"--restricted".into()) && chat.contains(&"--strict-mcp-config".into()));
    assert_eq!(after(&chat, "--tools"), "Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch");
    assert_eq!(after(&chat, "--permission-mode"), "default", "Ask names its mode, so a CLI default can never pick a looser one");
    assert_eq!(after(&chat, "--resume"), "abc");
    assert_eq!(after(&chat, "--model"), "haiku");
    let project = claude_args(&launch(Area::Project));
    assert!(!project.contains(&"--restricted".into()) && !project.contains(&"--strict-mcp-config".into()));
    // Hooks stay off until the switch is on; the settings carry the credential denies in every case (R124b).
    let off = settings_of(&project);
    assert_eq!(off["disableAllHooks"], true);
    assert!(deny_rules(&project).contains(&"Read(~/.ssh/**)".to_string()));
    let on = settings_of(&claude_args(&ClaudeLaunch { user_hooks: true, ..launch(Area::Project) }));
    assert!(on.get("disableAllHooks").is_none() && on["permissions"]["deny"].is_array());
}

const CREDENTIAL_DENIES: [&str; 5] =
    ["Read(~/.claude/**)", "Read(~/.codex/**)", "Read(~/.ssh/**)", "Read(~/.gemini/**)", "Read(~/.claude.json*)"];

/// The settings JSON of a launch: where the deny rules travel (the command line would split a rule at its spaces).
fn settings_of(args: &[String]) -> serde_json::Value {
    serde_json::from_str(&args[args.iter().position(|a| a == "--settings").expect("settings") + 1]).unwrap()
}

fn deny_rules(args: &[String]) -> Vec<String> {
    settings_of(args)["permissions"]["deny"].as_array().expect("deny rules").iter().map(|r| r.as_str().unwrap().to_owned()).collect()
}

#[test]
fn r121_full_access_bypasses_approvals_keeps_integrations_and_denies_only_the_credential_folders() {
    for area in [Area::Chat, Area::Project] {
        for hooks in [false, true] {
            let args = claude_args(&ClaudeLaunch { permissions: Permissions::FullAccess, user_hooks: hooks, ..launch(area) });
            assert!(args.windows(2).any(|a| a == ["--permission-mode", "bypassPermissions"]), "{area:?}");
            // Deny rules still apply in bypassPermissions, so the credential folders stay out of reach of the file tools.
            let deny = deny_rules(&args);
            for rule in CREDENTIAL_DENIES {
                assert!(deny.contains(&rule.to_string()), "{area:?} {rule}");
            }
            assert!(!args.contains(&"--disallowedTools".into()), "the rules travel in the settings JSON");
            // Nothing that isolates: no sandbox, ask rules, folder confinement or scrubbed environment ...
            let config = settings_of(&args);
            assert!(config.get("sandbox").is_none() && config.get("env").is_none(), "{area:?}: {config}");
            assert!(
                config["permissions"].get("ask").is_none() && config["permissions"].get("blockReadsOutsideWorkingDirectories").is_none()
            );
            // ... and nothing that turns the person's own integrations off.
            for flag in ["--safe-mode", "--strict-mcp-config", "--setting-sources", "--restricted", "--bare"] {
                assert!(!args.contains(&flag.into()), "{area:?} {flag}");
            }
        }
    }
    let chat = claude_args(&ClaudeLaunch { permissions: Permissions::FullAccess, ..launch(Area::Chat) });
    assert!(
        chat.windows(2).any(|a| a == ["--tools", "Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch"]),
        "Chat keeps its lean tool list"
    );
}

#[test]
fn r121_the_person_choosing_do_not_ask_never_reaches_the_isolated_arguments() {
    // The old "Do not ask" arguments, now reachable only by naming Isolated.
    for area in [Area::Chat, Area::Project] {
        let full = claude_args(&ClaudeLaunch { permissions: Permissions::FullAccess, ..launch(area) });
        let isolated = claude_args(&ClaudeLaunch { permissions: Permissions::Isolated, ..launch(area) });
        assert_ne!(full, isolated);
        assert!(isolated.contains(&"--safe-mode".into()) && isolated.contains(&"--strict-mcp-config".into()));
    }
    // The stored settings name only the two choices on screen; the isolated mode cannot be written or read back.
    assert_eq!(serde_json::to_string(&Permissions::FullAccess).unwrap(), "\"dontAsk\"");
    assert_eq!(serde_json::to_string(&Permissions::Ask).unwrap(), "\"ask\"");
    assert_eq!(serde_json::from_str::<Permissions>("\"dontAsk\"").unwrap(), Permissions::FullAccess);
    assert!(serde_json::from_str::<Permissions>("\"isolated\"").is_err());
    assert!(serde_json::to_string(&Permissions::Isolated).is_err());
    assert_eq!(Permissions::default(), Permissions::FullAccess);
}

#[test]
fn r107_isolated_mode_covers_chat_and_projects_and_keeps_denies_with_user_hooks() {
    for hooks in [false, true] {
        let asking = ClaudeLaunch { user_hooks: hooks, ..launch(Area::Project) };
        let ask = claude_args(&asking);
        assert!(ask.windows(2).any(|a| a == ["--permission-mode", "default"]));
        assert!(!ask.contains(&"--disallowedTools".into()));
        assert!(ask.windows(2).any(|a| a == ["--permission-prompt-tool", "stdio"]));
        let skip = claude_args(&ClaudeLaunch { permissions: Permissions::Isolated, ..asking });
        assert!(skip.windows(2).any(|a| a == ["--permission-mode", "bypassPermissions"]));
        let config: serde_json::Value = serde_json::from_str(&skip[skip.iter().position(|a| a == "--settings").unwrap() + 1]).unwrap();
        assert_eq!(config["disableAllHooks"], true);
        assert_eq!(config["permissions"]["blockReadsOutsideWorkingDirectories"], true);
        assert!(deny_rules(&skip).contains(&"Edit(//**/.git/**)".to_string()));
    }
    let chat = claude_args(&ClaudeLaunch { permissions: Permissions::Isolated, ..launch(Area::Chat) });
    assert!(!chat.contains(&"--restricted".into()), "restricted refuses bypassPermissions");
    assert!(chat.contains(&"--strict-mcp-config".into()));
    assert!(chat.windows(2).any(|a| a == ["--permission-mode", "bypassPermissions"]));
    let config: serde_json::Value = serde_json::from_str(&chat[chat.iter().position(|a| a == "--settings").unwrap() + 1]).unwrap();
    assert_eq!(config["permissions"]["blockReadsOutsideWorkingDirectories"], true);
    assert!(chat.contains(&"--safe-mode".into()), "Chat excludes startup customizations");
    assert!(chat.windows(2).any(|a| a == ["--setting-sources", "user"]));
    // A side question has instructions of its own and no tools, whatever the mode.
    for permissions in [Permissions::Isolated, Permissions::FullAccess] {
        let side = claude_args(&ClaudeLaunch { permissions, instructions: Some("Route.".into()), ..launch(Area::Project) });
        assert!(!side.contains(&"bypassPermissions".into()) && !side.contains(&"--disallowedTools".into()));
    }
}

#[tokio::test]
async fn r124_full_access_hands_every_request_to_the_engine_and_writes_no_answer_itself() {
    // The engine decides with the mode in force at that moment (see engine.rs), so a switch to Ask can never be raced by an
    // answer the driver already wrote. The driver of the isolated mode still answers for itself.
    for area in [Area::Chat, Area::Project] {
        let root = tempfile::tempdir().unwrap();
        let f = fake(fixture("claude", "project-policy"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let session = start_claude(
            ClaudeOptions {
                bin: "claude".into(),
                cwd: root.path().into(),
                launch: ClaudeLaunch { permissions: Permissions::FullAccess, ..launch(area) },
                spawn: f.spawn.clone(),
                sandbox: None,
            },
            Arc::new(move |e| {
                let _ = tx.send(e);
            }),
        );
        session.send(UserTurn::text("check policy"));
        assert_eq!(next_permission(&mut rx).await, "write", "{area:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(f.wire.lines_with("control_response"), 0, "{area:?}: the driver answered by itself");
        session.close();
    }
}

#[tokio::test]
async fn the_isolated_mode_declines_residual_prompts_without_emitting_a_card() {
    for area in [Area::Chat, Area::Project] {
        let entries = support::claude_two_turns("req")
            .into_iter()
            .take(5)
            .map(|mut e| {
                if !e.input && e.msg["type"] == "control_request" {
                    e.msg["request"]["tool_name"] = "AskUserQuestion".into();
                }
                if e.input && e.msg["type"] == "control_response" {
                    e.msg["response"]["response"] = serde_json::json!({ "behavior": "deny", "message": "Approval is disabled in Faber." });
                }
                e
            })
            .collect();
        let (events, mismatches) = drive(
            entries,
            |spawn, sink| {
                start_claude(
                    ClaudeOptions {
                        bin: "claude".into(),
                        cwd: ".".into(),
                        launch: ClaudeLaunch { permissions: Permissions::Isolated, ..launch(area) },
                        spawn,
                        sandbox: None,
                    },
                    sink,
                )
            },
            "first",
            |event, _| matches!(event, EngineEvent::Done { .. }),
        )
        .await;
        assert!(mismatches.is_empty(), "{mismatches:?}");
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })));
    }
}

#[tokio::test]
async fn an_undelivered_automatic_permission_response_ends_the_turn_instead_of_waiting_for_a_card() {
    let f = fake(support::claude_two_turns("req"));
    let wire = f.wire.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let sink = std::sync::Arc::new(move |e| {
        if matches!(e, EngineEvent::Started { .. }) {
            wire.refuse_next(1);
        }
        tx.send(e).unwrap();
    });
    let session = start_claude(
        ClaudeOptions {
            bin: "claude".into(),
            cwd: ".".into(),
            spawn: f.spawn,
            launch: ClaudeLaunch { permissions: Permissions::Isolated, ..launch(Area::Project) },
            sandbox: None,
        },
        sink,
    );
    session.send(UserTurn::text("first"));
    assert!(matches!(tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap(), Some(EngineEvent::Started { .. })));
    assert!(matches!(tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap(),
        Some(EngineEvent::Error { message, fatal: true }) if message == "error.permission.undelivered"));
    session.close();
}

#[tokio::test]
async fn l2_project_file_and_command_decisions_reach_stdio_without_permission_cards() {
    for area in [Area::Chat, Area::Project] {
        let root = tempfile::tempdir().unwrap();
        let f = fake(fixture("claude", "project-policy"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let session = start_claude(
            ClaudeOptions {
                bin: "claude".into(),
                cwd: root.path().into(),
                launch: ClaudeLaunch { permissions: Permissions::Isolated, ..launch(area) },
                spawn: f.spawn.clone(),
                sandbox: Some(faber_engine::cli::Cli { command: "C:/Tools/codex.exe".into(), prefix: vec![] }),
            },
            Arc::new(move |e| {
                let _ = tx.send(e);
            }),
        );
        session.send(UserTurn::text("check policy"));
        loop {
            let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
            assert!(!matches!(e, EngineEvent::Permission { .. }));
            if matches!(e, EngineEvent::Done { .. }) {
                break;
            }
        }
        let responses: Vec<_> = f
            .wire
            .messages()
            .into_iter()
            .filter(|m| m["type"] == "control_response")
            .map(|m| {
                (
                    m["response"]["request_id"].as_str().unwrap().to_owned(),
                    m["response"]["response"]["behavior"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(
            responses,
            [("write", "allow"), ("outside", "deny"), ("git", "deny"), ("delete", "allow"), ("question", "deny")]
                .map(|(id, behavior)| (id.to_owned(), behavior.to_owned()))
        );
        assert!(f.mismatches.lock().unwrap().is_empty());
        session.close();
    }
}

#[test]
fn a_side_question_gets_its_own_instructions_no_tools_and_no_saved_session() {
    let after = |args: &[String], flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    let args = claude_args(&ClaudeLaunch { instructions: Some("Route.".into()), ..launch(Area::Project) });
    assert_eq!(after(&args, "--system-prompt"), "Route.");
    assert_eq!(after(&args, "--tools"), "");
    assert!(args.contains(&"--strict-mcp-config".into()) && args.contains(&"--safe-mode".into()));
    assert_eq!(after(&args, "--mcp-config"), r#"{"mcpServers":{}}"#);
    assert_eq!(after(&args, "--settings"), r#"{"disableAllHooks":true}"#);
    assert!(args.contains(&"--no-session-persistence".into()), "it stays out of the user's Claude history");
}

#[tokio::test]
async fn review2_h3_the_driver_sends_only_a_sandbox_wrapped_build_without_a_card() {
    for area in [Area::Chat, Area::Project] {
        let root = tempfile::tempdir().unwrap();
        let f = fake(fixture("claude", "project-build"));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let session = start_claude(
            ClaudeOptions {
                bin: "claude".into(),
                cwd: root.path().into(),
                launch: ClaudeLaunch { permissions: Permissions::Isolated, ..launch(area) },
                spawn: f.spawn.clone(),
                sandbox: Some(faber_engine::cli::Cli { command: "C:/Tools/codex.exe".into(), prefix: vec![] }),
            },
            Arc::new(move |e| {
                let _ = tx.send(e);
            }),
        );
        session.send(UserTurn::text("run project tests"));
        loop {
            let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
            assert!(!matches!(e, EngineEvent::Permission { .. }));
            if matches!(e, EngineEvent::Done { .. }) {
                break;
            }
        }
        let messages = f.wire.messages();
        let response = messages.iter().find(|m| m["type"] == "control_response").unwrap();
        let command = response["response"]["response"]["updatedInput"]["command"].as_str().unwrap();
        assert!(command.contains("sandbox") && command.ends_with("'cargo' 'test'"), "{command}");
        assert!(command.contains(":root") && command.contains("deny"));
        assert!(f.mismatches.lock().unwrap().is_empty());
        session.close();
    }
}

#[test]
fn the_name_is_appended_to_the_default_prompt_of_a_conversation_but_never_to_a_side_question() {
    let after = |args: &[String], flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    let named = |l: ClaudeLaunch| ClaudeLaunch { user_name: Some("Erick".into()), ..l };
    for area in [Area::Chat, Area::Project] {
        let args = claude_args(&named(launch(area)));
        assert!(after(&args, "--append-system-prompt").contains("called Erick."));
        assert!(!args.contains(&"--system-prompt".into()), "the default prompt stays");
        assert_eq!(args, claude_args(&named(launch(area))), "the same launch gives the same arguments");
        assert!(after(&claude_args(&launch(area)), "--append-system-prompt").contains("You are Faber"));
    }
    let resumed = claude_args(&named(ClaudeLaunch { resume: Some("abc".into()), ..launch(Area::Chat) }));
    assert_eq!(after(&resumed, "--append-system-prompt"), after(&claude_args(&named(launch(Area::Chat))), "--append-system-prompt"));
    let side = claude_args(&named(ClaudeLaunch { instructions: Some("Route.".into()), ..launch(Area::Project) }));
    assert!(!side.contains(&"--append-system-prompt".into()));
}

#[tokio::test]
async fn a_turn_without_a_quota_notice_clears_subscription_confirmation_after_settlement() {
    let entries = fixture("claude", "chat").into_iter().filter(|e| e.msg["type"] != "rate_limit_event").collect();
    let (events, mismatches) = run(
        entries,
        "In one sentence: why is the sky blue?",
        |e, _| matches!(e, EngineEvent::Signals(s) if s.billing == Some(faber_engine::budget::Billing::Unknown)),
    )
    .await;
    assert!(mismatches.is_empty());
    let done = events.iter().position(|e| matches!(e, EngineEvent::Done { .. })).unwrap();
    let cleared = events
        .iter()
        .position(|e| matches!(e, EngineEvent::Signals(s) if s.billing == Some(faber_engine::budget::Billing::Unknown)))
        .unwrap();
    assert!(done < cleared, "settle the own cost before clearing the billing verdict");
}

// Live captures (claude 2.1.296, the flags Faber uses), a second user line written while the first answer streams:
// - `queued`: the first answer was a plain reply. Claude Code finished it, sent its result, then started the second line
//   as a turn of its own with its own init and result.
// - `queued-midturn`: the first answer used a tool. Claude Code folded the line into that turn at the end of the tool
//   step: one result for both, and the answer to the second line sits in the same reply.
// Which of the two happens depends on the turn, so Faber holds the line itself and writes it after the result.
#[tokio::test]
async fn a_line_written_while_an_answer_streams_waits_for_the_result_and_is_a_turn_of_its_own() {
    let entries = fixture("claude", "queued");
    let (events, mismatches) = run(entries, "Count from 1 to 30, one number per line, each followed by a five-word comment.", {
        let mut dones = 0;
        move |e, s| {
            if matches!(e, EngineEvent::Done { .. }) {
                dones += 1;
                if dones == 1 {
                    s.send(UserTurn::text("stop at 10"));
                }
            }
            dones == 2
        }
    })
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    let done: Vec<_> = events.iter().filter(|e| matches!(e, EngineEvent::Done { ok: true, .. })).collect();
    assert_eq!(done.len(), 2, "each line has a result of its own");
    assert!(text_of(&events).contains("The journey begins with one."));
}

#[test]
fn a_line_written_during_a_tool_turn_is_folded_into_it_and_the_turn_has_one_result() {
    let events: Vec<EngineEvent> = fixture("claude", "queued-midturn")
        .iter()
        .filter(|e| !e.input)
        .flat_map(|e| faber_engine::claude::parse_claude_message(&e.msg))
        .collect();
    let dones = events.iter().filter(|e| matches!(e, EngineEvent::Done { .. })).count();
    assert_eq!(dones, 1, "Faber would take the first result for the end of the turn and lose what follows");
    assert!(text_of(&events).trim_end().ends_with("pong"), "{:?}", text_of(&events));
}

/// R124: the router, the pulse and every side question start a Claude with instructions of their own. Whatever the person
/// chose, they run with no hooks, no settings files, no MCP servers, no plugins and no tools.
#[test]
fn r124_every_internal_claude_request_runs_without_user_hooks_settings_or_mcp() {
    let after = |args: &[String], flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    for permissions in [Permissions::Ask, Permissions::FullAccess, Permissions::Isolated] {
        for area in [Area::Chat, Area::Project] {
            for user_hooks in [false, true] {
                let args = claude_args(&ClaudeLaunch { permissions, user_hooks, instructions: Some("Route.".into()), ..launch(area) });
                let why = format!("{permissions:?} {area:?} hooks={user_hooks}: {args:?}");
                assert!(args.contains(&"--safe-mode".into()), "{why}");
                assert!(args.contains(&"--strict-mcp-config".into()), "{why}");
                assert_eq!(after(&args, "--setting-sources"), "", "no user, project or local settings file: {why}");
                assert_eq!(after(&args, "--tools"), "", "{why}");
                assert_eq!(settings_of(&args)["disableAllHooks"], true, "{why}");
            }
        }
    }
}

/// R124: Ask names the CLI's own asking mode in both areas, so a person's CLI configuration (defaultMode: bypassPermissions) or
/// a looser mode can never win, and Chat no longer auto-accepts edits.
#[test]
fn r124_ask_names_its_permission_mode_in_both_areas_so_a_cli_default_never_wins() {
    for area in [Area::Chat, Area::Project] {
        for hooks in [false, true] {
            let args = claude_args(&ClaudeLaunch { user_hooks: hooks, ..launch(area) });
            let at =
                args.iter().position(|a| a == "--permission-mode").unwrap_or_else(|| panic!("{area:?}: no permission mode in {args:?}"));
            assert_eq!(args[at + 1], "default", "{area:?}");
            assert!(!args.iter().any(|a| a == "bypassPermissions" || a == "acceptEdits" || a == "auto"), "{area:?}");
        }
    }
}

/// R124: the deny rules are one JSON string each. A rule with a space (the GitHub CLI folder) cannot be split in two, and every
/// credential folder is denied for the Edit tool (which Write follows) as well as for Read.
#[test]
fn r124_the_deny_rules_travel_in_the_settings_json_each_one_intact_and_cover_edits_too() {
    for permissions in [Permissions::FullAccess, Permissions::Isolated, Permissions::Ask] {
        for area in [Area::Chat, Area::Project] {
            let args = claude_args(&ClaudeLaunch { permissions, ..launch(area) });
            assert!(
                !args.contains(&"--disallowedTools".into()),
                "{permissions:?} {area:?}: the command line would split a rule at a space"
            );
            let deny = deny_rules(&args);
            for folder in [
                "~/.claude",
                "~/.codex",
                "~/.gemini",
                "~/.ssh",
                "~/.aws",
                "~/.docker",
                "~/.azure",
                "~/.kube",
                "~/.config/gh",
                "~/AppData/Roaming/GitHub CLI",
            ] {
                for tool in ["Read", "Edit"] {
                    assert!(deny.contains(&format!("{tool}({folder}/**)")), "{permissions:?} {area:?} {tool} {folder}");
                }
            }
            for file in ["~/.claude.json*", "~/.git-credentials", "~/.npmrc"] {
                for tool in ["Read", "Edit"] {
                    assert!(deny.contains(&format!("{tool}({file})")), "{tool} {file}");
                }
            }
            assert!(deny.contains(&"Read(//**/*updater.key*)".to_string()) && deny.contains(&"Edit(//**/*updater.key*)".to_string()));
            assert!(deny.contains(&"Edit(//**/.git/**)".to_string()));
            for rule in &deny {
                let (tool, rest) = rule.split_once('(').unwrap();
                assert!(["Read", "Edit"].contains(&tool) && rest.ends_with(')') && rest.matches('(').count() == 0, "a broken rule: {rule}");
            }
        }
    }
}

/// R124: a sign-in folder the person moved with a documented variable is denied where it now is.
#[test]
fn r124_a_moved_sign_in_folder_is_denied_where_it_now_is() {
    use faber_engine::claude::tool_deny;
    let bs = char::from(92);
    let moved = format!("E:{bs}Accounts{bs}work{bs}");
    let env = |name: &str| match name {
        "CLAUDE_CONFIG_DIR" => Some(moved.clone()),
        "CODEX_HOME" => Some("/home/me/codex-home".to_string()),
        "AWS_SHARED_CREDENTIALS_FILE" => Some("/srv/aws/credentials".to_string()),
        "GEMINI_CLI_HOME" => Some("/home/me/gem".to_string()),
        "DOCKER_CONFIG" => Some("relative/docker".to_string()),
        _ => None,
    };
    let rules = tool_deny(&env);
    for tool in ["Read", "Edit"] {
        for expected in ["//e/Accounts/work/**", "//home/me/codex-home/**", "//srv/aws/credentials", "//home/me/gem/.gemini/**"] {
            assert!(rules.contains(&format!("{tool}({expected})")), "{tool} {expected}: {rules:?}");
        }
    }
    assert!(!rules.iter().any(|r| r.contains("relative")), "a relative value names nothing fixed");
    let unset = tool_deny(&|_| None);
    assert!(!unset.iter().any(|r| r.contains("Accounts") || r.contains("codex-home")));
    assert_eq!(unset.len(), tool_deny(&|_| None).len());
}

// ---- no blocking prompts in full access, cache warmth, Haiku autocompact (checked on Claude Code 2.1.296) ----

const NEWER: ClaudeFlags = ClaudeFlags { autocompact: true, permission_prompts: true };

fn full(area: Area, flags: ClaudeFlags) -> Vec<String> {
    claude_args(&ClaudeLaunch { permissions: Permissions::FullAccess, flags, ..launch(area) })
}

#[test]
fn full_access_runs_with_no_prompts_and_every_credential_deny_still_wins() {
    for area in [Area::Chat, Area::Project] {
        for hooks in [false, true] {
            let args = claude_args(&ClaudeLaunch { permissions: Permissions::FullAccess, flags: NEWER, user_hooks: hooks, ..launch(area) });
            assert!(args.windows(2).any(|a| a == ["--permission-prompts", "none"]), "{area:?}");
            assert!(args.windows(2).any(|a| a == ["--permission-mode", "bypassPermissions"]), "an explicit mode, never the CLI's own");
            assert!(!args.contains(&"--permission-prompt-tool".into()), "nobody answers, so no host tool is offered");
            // The credential denies are the very ones of the launch without the new flag, and none was dropped.
            let deny = deny_rules(&args);
            assert_eq!(deny, deny_rules(&full(area, ClaudeFlags::default())), "{area:?}");
            for rule in CREDENTIAL_DENIES.iter().chain(&["Edit(~/.ssh/**)", "Edit(~/.codex/**)", "Edit(//**/.git/**)"]) {
                assert!(deny.contains(&rule.to_string()), "{area:?} {rule}");
            }
            // Nothing in the settings allows anything: the denies are the only rules, and no allow rule or ask override exists.
            let config = settings_of(&args);
            assert!(config["permissions"].get("allow").is_none() && config["permissions"].get("ask").is_none());
            assert!(!args.iter().any(|a| a == "--allowedTools" || a == "--dangerously-skip-permissions"));
        }
    }
}

#[test]
fn a_cli_without_the_flag_starts_full_access_exactly_as_before() {
    for area in [Area::Chat, Area::Project] {
        let args = full(area, ClaudeFlags { autocompact: true, permission_prompts: false });
        assert!(args.windows(2).any(|a| a == ["--permission-prompt-tool", "stdio"]));
        assert!(!args.contains(&"--permission-prompts".into()));
        assert_eq!(args, full(area, ClaudeFlags::default()));
    }
}

#[test]
fn ask_the_isolated_mode_and_side_questions_never_change_with_the_new_flags() {
    for area in [Area::Chat, Area::Project] {
        for permissions in [Permissions::Ask, Permissions::Isolated] {
            let old = claude_args(&ClaudeLaunch { permissions, ..launch(area) });
            let new = claude_args(&ClaudeLaunch { permissions, flags: NEWER, ..launch(area) });
            assert_eq!(old, new, "{area:?} {permissions:?}: no model named, so no autocompact either");
            assert!(new.windows(2).any(|a| a == ["--permission-prompt-tool", "stdio"]) && !new.contains(&"--permission-prompts".into()));
        }
        let side = claude_args(&ClaudeLaunch {
            permissions: Permissions::FullAccess,
            flags: NEWER,
            instructions: Some("Route.".into()),
            ..launch(area)
        });
        assert!(side.windows(2).any(|a| a == ["--permission-prompt-tool", "stdio"]) && !side.contains(&"--permission-prompts".into()));
    }
    let ask = claude_args(&ClaudeLaunch { permissions: Permissions::Ask, flags: NEWER, ..launch(Area::Project) });
    assert!(ask.windows(2).any(|a| a == ["--permission-mode", "default"]));
}

#[test]
fn only_fabers_own_haiku_launches_compact_early_and_only_where_the_cli_has_the_flag() {
    let with = |model: &str, permissions, instructions: Option<&str>, flags| {
        claude_args(&ClaudeLaunch {
            model: Some(model.into()),
            permissions,
            instructions: instructions.map(Into::into),
            flags,
            ..launch(Area::Project)
        })
    };
    let compacts = |args: &[String]| args.windows(2).any(|a| a == ["--autocompact", "100k"]);
    for model in ["haiku", "claude-haiku-5-5", "Haiku[1m]"] {
        assert!(compacts(&with(model, Permissions::Isolated, None, NEWER)), "{model}");
        assert!(compacts(&with(model, Permissions::FullAccess, Some("Route."), NEWER)), "{model}");
        assert!(!compacts(&with(model, Permissions::Isolated, None, ClaudeFlags::default())), "an older CLI is not given the flag");
        assert!(!compacts(&with(model, Permissions::Ask, None, NEWER)), "a conversation the person chose keeps the CLI default");
        assert!(!compacts(&with(model, Permissions::FullAccess, None, NEWER)));
    }
    for model in ["sonnet", "opus", "claude-sonnet-5-5", "fable"] {
        assert!(!with(model, Permissions::Isolated, None, NEWER).contains(&"--autocompact".into()), "{model}");
    }
}

#[test]
fn the_flags_are_read_from_the_help_text_word_by_word() {
    let help = "  --autocompact <auto|tokens>           Auto-compact window size\n  --permission-prompt-tool <tool>  MCP tool\n";
    assert_eq!(ClaudeFlags::from_help(help), ClaudeFlags { autocompact: true, permission_prompts: false });
    let both = format!("{help}  --permission-prompts <target>         Who answers permission prompts\n");
    assert_eq!(ClaudeFlags::from_help(&both), NEWER);
    // A mention in a description is not the flag.
    assert_eq!(ClaudeFlags::from_help("use autocompact and --permission-prompts-x"), ClaudeFlags::default());
    assert_eq!(ClaudeFlags::from_help(""), ClaudeFlags::default());
}

fn result_with(usage: serde_json::Value, denials: serde_json::Value) -> serde_json::Value {
    json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.01, "usage": usage, "permission_denials": denials })
}

#[test]
fn the_cache_buckets_of_a_result_say_how_long_the_cache_stays_warm() {
    let warm = |usage: serde_json::Value| {
        parse_claude_message(&result_with(usage, json!([]))).into_iter().find_map(|e| match e {
            EngineEvent::CacheWarm { secs, .. } => Some(secs),
            _ => None,
        })
    };
    let buckets = |h: u64, m: u64| {
        json!({ "input_tokens": 2, "cache_creation_input_tokens": h + m,
            "cache_creation": { "ephemeral_1h_input_tokens": h, "ephemeral_5m_input_tokens": m } })
    };
    assert_eq!(warm(buckets(2583, 0)), Some(3600), "a subscription writes the one-hour bucket");
    assert_eq!(warm(buckets(0, 900)), Some(300));
    assert_eq!(warm(buckets(100, 900)), Some(3600), "mixed: the main conversation's long bucket counts");
    assert_eq!(warm(buckets(0, 0)), None, "nothing written, nothing known");
    assert_eq!(warm(json!({ "input_tokens": 2, "cache_creation_input_tokens": 500 })), None, "an older CLI names no bucket");
    // The recorded turn (tests/fixtures/claude/chat.jsonl) wrote 2,583 tokens into the one-hour bucket.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/claude/chat.jsonl")).unwrap();
    let results: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|l| l["msg"].clone())
        .filter(|m| m["type"] == "result")
        .collect();
    assert_eq!(results.len(), 1);
    let events = parse_claude_message(&results[0]);
    assert!(matches!(events.as_slice(), [EngineEvent::CacheWarm { secs: 3600, .. }, EngineEvent::Done { .. }]), "{events:?}");
}

#[test]
fn a_denial_in_the_result_becomes_one_parked_request_before_done_and_never_an_allow() {
    let denials = json!([
        { "tool_name": "Write", "tool_use_id": "toolu_1", "tool_input": { "file_path": "/w/note.txt", "content": "hi" } },
        { "tool_name": "Bash", "tool_use_id": "toolu_2", "tool_input": { "command": "git push" } },
    ]);
    let events = parse_claude_message(&result_with(json!({ "input_tokens": 1 }), denials));
    match events.as_slice() {
        [
            EngineEvent::Parked { tool_use_id: a, tool: ta, .. },
            EngineEvent::Parked { tool_use_id: b, tool: tb, input },
            EngineEvent::Done { ok: true, .. },
        ] => {
            assert_eq!((a.as_str(), ta.as_str(), b.as_str(), tb.as_str()), ("toolu_1", "Write", "toolu_2", "Bash"));
            assert_eq!(input["command"], "git push");
        }
        other => panic!("{other:?}"),
    }
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })));
    let none = parse_claude_message(&result_with(json!({ "input_tokens": 1 }), json!([])));
    assert!(none.iter().all(|e| !matches!(e, EngineEvent::Parked { .. })));
}

#[tokio::test]
async fn a_denial_is_parked_only_when_the_cli_ran_with_no_prompts() {
    let mut asked = 0;
    // The same recording, driven as full access on a CLI that has the flag: its one denial in the result is parked.
    let (events, _) = run_with(
        ClaudeLaunch { permissions: Permissions::FullAccess, flags: NEWER, ..launch(Area::Chat) },
        fixture("claude", "tools"),
        "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
        |e, s| {
            if let EngineEvent::Permission { request_id, .. } = e {
                asked += 1;
                drop(s.answer(request_id, asked == 1));
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    let parked: Vec<_> = events.iter().filter(|e| matches!(e, EngineEvent::Parked { tool, .. } if tool == "Write")).collect();
    assert_eq!(parked.len(), 1, "{events:?}");
    let at = |f: fn(&EngineEvent) -> bool| events.iter().position(f).unwrap();
    assert!(at(|e| matches!(e, EngineEvent::Parked { .. })) < at(|e| matches!(e, EngineEvent::Done { .. })));
}

#[test]
fn a_flag_named_in_a_diagnostic_or_a_description_is_no_support() {
    let none = ClaudeFlags::default();
    for text in [
        "error: --permission-prompts is unavailable in this build\n",
        "  --permission-prompts is unavailable in this build\n",
        "Warning: --autocompact <auto|tokens> was removed\n",
        "  --other-flag   Same as --permission-prompts <target> and --autocompact <x>\n",
        "  --permission-prompts-extra <x>  Not the flag\n",
        "--autocompact <x> at the start of the text, not indented\n",
    ] {
        assert_eq!(ClaudeFlags::from_help(text), none, "{text}");
    }
    // An option line with a short alias, and one with no value, still declare.
    assert!(ClaudeFlags::from_help("  -a, --autocompact <auto|tokens>  Window\n").autocompact);
    assert!(ClaudeFlags::from_help("  --permission-prompts\n").permission_prompts);
    assert!(ClaudeFlags::from_help("\t--permission-prompts [target]\tWho\n").permission_prompts);
}

/// A real hidden run on Claude Code 2.1.296 (tests/fixtures/claude/deny-wins.jsonl): `--permission-mode bypassPermissions
/// --permission-prompts none --safe-mode --tools Read` in a temp folder, with a synthetic file in a folder that a
/// `Read(...)` deny rule covers, asked to be read. The CLI denied the read itself, and the file's text never reached the stream.
#[tokio::test]
async fn deny_wins_in_the_no_stdio_path_a_real_recording_of_a_denied_credential_read() {
    let launch = ClaudeLaunch { permissions: Permissions::FullAccess, flags: NEWER, ..launch(Area::Project) };
    let args = claude_args(&launch);
    assert!(args.windows(2).any(|a| a == ["--permission-prompts", "none"]) && !args.contains(&"--permission-prompt-tool".into()));
    assert!(args.windows(2).any(|a| a == ["--permission-mode", "bypassPermissions"]));
    assert!(deny_rules(&args).contains(&"Read(~/.ssh/**)".to_string()), "the same kind of rule as the recording's");

    let entries = fixture("claude", "deny-wins");
    let recorded = serde_json::to_string(&entries.iter().map(|e| e.msg.clone()).collect::<Vec<_>>()).unwrap();
    assert!(!recorded.contains("SYNTHETIC-CREDENTIAL"), "the recording never holds the file's text");
    let question = entries[0].msg["message"]["content"].as_str().unwrap().to_string();
    assert!(question.contains("id_synthetic"));
    let (events, mismatches) = run_with(launch, entries, &question, |e, _| matches!(e, EngineEvent::Done { .. })).await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })), "nobody was asked");
    let denied: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::ToolResult { ok: false, text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(denied.len(), 1);
    assert!(denied[0].contains("denied by your permission settings"), "{denied:?}");
    assert!(!format!("{events:?}").contains("SYNTHETIC-CREDENTIAL"));
    // The CLI also lists the refusal in the result; the engine drops it, because it names a credential folder.
    let Some(EngineEvent::Parked { input, .. }) = events.iter().find(|e| matches!(e, EngineEvent::Parked { .. })) else {
        panic!("the recorded result lists the denial: {events:?}")
    };
    assert!(faber_engine::permissions::touches_credentials(input, None, &|_| None), "{input}");
}

#[tokio::test]
async fn the_cache_lifetime_starts_at_the_assistant_message_start_and_is_dropped_when_that_is_unknown() {
    let started = |e: &EngineEvent| match e {
        EngineEvent::CacheWarm { started_ms, .. } => Some(*started_ms),
        _ => None,
    };
    let clock = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let before = clock();
    let (events, _) =
        run(fixture("claude", "chat"), "In one sentence: why is the sky blue?", |e, _| matches!(e, EngineEvent::Done { .. })).await;
    let after = clock();
    let seen: Vec<_> = events.iter().filter_map(started).collect();
    assert!(matches!(seen.as_slice(), [Some(ms)] if (before..=after).contains(ms)), "{seen:?}");
    // The same recording with the start of its assistant message removed: no expiry can be trusted, so none is sent.
    let without: Vec<_> = fixture("claude", "chat").into_iter().filter(|e| e.msg["event"]["type"] != "message_start").collect();
    let (events, _) = run(without, "In one sentence: why is the sky blue?", |e, _| matches!(e, EngineEvent::Done { .. })).await;
    assert!(events.iter().all(|e| started(e).is_none()), "{events:?}");
}
