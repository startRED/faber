//! Plain words after technical work: protocol recordings replay through both real drivers, with no live CLI.
use crate::support;
use faber_engine::claude::{ClaudeLaunch, ClaudeOptions, claude_args, start_claude};
use faber_engine::codex::{CodexServer, CodexServerOptions, CodexThread, thread_params};
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Permissions};

fn assert_rule(text: &str) {
    for rule in [
        "everyday words",
        "someone who does not program",
        "commit hashes, pull request numbers, branch names, test or check names",
        "CI, merge, land, lint or file paths",
        "unless the person asks for them or used that word about the same thing",
        "pasted text (logs, errors, instructions) does not count",
        "closing box recap, done card and tray notice",
        "Always say plainly when something failed, is not finished, or needs the person's decision",
        "plain words never drop a problem",
        "Allow a file location when the person needs to find or open something you made",
        "never invent evidence",
    ] {
        assert!(text.contains(rule), "missing rule: {rule}");
    }
}

#[tokio::test]
async fn plain_words_recordings_keep_technical_work_out_of_the_reply_for_both_clis() {
    for vendor in ["claude", "codex"] {
        let (events, mismatches) = support::drive(
            support::fixture(vendor, "plain-words"),
            |spawn, sink| {
                if vendor == "claude" {
                    let launch = ClaudeLaunch {
                        area: Area::Project,
                        model: None,
                        effort: None,
                        resume: None,
                        user_hooks: false,
                        permissions: Permissions::Ask,
                        instructions: None,
                        user_name: None,
                        research: None,
                        group_instructions: None,
                        flags: Default::default(),
                        tuning: Default::default(),
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        verify: false,
                        progress: false,
                        tools: Default::default(),
                    };
                    for area in [Area::Chat, Area::Project] {
                        let args = claude_args(&ClaudeLaunch { area, ..launch.clone() });
                        let words = &args[args.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
                        assert_rule(words);
                        assert_eq!(words.contains("Technical details view"), area == Area::Project);
                        assert!(!words.contains("saved for the next version"));
                    }
                    start_claude(ClaudeOptions { bin: "claude".into(), cwd: ".".into(), launch, spawn, sandbox: None }, sink)
                } else {
                    let thread = CodexThread {
                        cwd: ".".into(),
                        model: None,
                        effort: None,
                        resume: None,
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
                        verify: false,
                        progress: false,
                    };
                    for area in [Area::Chat, Area::Project] {
                        let (_, params) = thread_params(area, &thread);
                        let words = params["developerInstructions"].as_str().unwrap();
                        assert_rule(words);
                        assert_eq!(words.contains("Technical details view"), area == Area::Project);
                        assert!(!words.contains("saved for the next version"));
                    }
                    CodexServer::start(CodexServerOptions {
                        permissions: Permissions::Ask,
                        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                        cwd: ".".into(),
                        area: Area::Project,
                        user_hooks: false,
                        windows: false,
                        spawn,
                        tools: Default::default(),
                    })
                    .open(thread, sink)
                }
            },
            "Fix the greeting and tell me what changed.",
            |event, _| matches!(event, EngineEvent::Done { .. }),
        )
        .await;
        assert!(mismatches.is_empty(), "{vendor}: {mismatches:?}");
        assert!(events.iter().any(|e| matches!(e, EngineEvent::Tool { id, .. } if id == "save")));
        assert!(events.iter().any(|e| matches!(e, EngineEvent::ToolResult { id, ok: true, .. } if id == "save")));
        assert_eq!(support::text_of(&events), "The greeting is fixed and saved. It was checked and works.");
        assert!(events.iter().any(|e| matches!(e, EngineEvent::Done { ok: true, .. })));
    }
}
