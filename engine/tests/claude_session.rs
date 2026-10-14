use crate::support;

use faber_engine::claude::{ClaudeLaunch, ClaudeOptions, claude_args, start_claude};
use faber_engine::events::{Delivery, EngineEvent, UserTurn};
use faber_engine::model::{Area, Permissions};
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
    }
}

async fn run(
    entries: Vec<Entry>,
    first: &str,
    react: impl FnMut(&EngineEvent, &faber_engine::events::Session) -> bool,
) -> (Vec<EngineEvent>, Vec<String>) {
    drive(
        entries,
        |spawn, sink| {
            start_claude(ClaudeOptions { bin: "claude".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn, sandbox: None }, sink)
        },
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
    assert_eq!(after(&project, "--settings"), r#"{"disableAllHooks":true}"#);
    assert!(!claude_args(&ClaudeLaunch { user_hooks: true, ..launch(Area::Project) }).contains(&"--settings".into()));
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
        assert!(!claude_args(&launch(area)).contains(&"--append-system-prompt".into()));
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
    for permissions in [Permissions::FullAccess, Permissions::Isolated] {
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
