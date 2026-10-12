mod support;

use faber_engine::claude::{ClaudeLaunch, ClaudeOptions, project_claude_args, start_project_claude};
use faber_engine::codex::{CodexServer, CodexServerOptions, CodexThread, project_codex_args, project_thread_params};
use faber_engine::events::{Capabilities, terminal_reason};
use faber_engine::model::{Area, Vendor};
use faber_engine::project::Role;
use faber_engine::project::policy::{Blocker, blockers};
use serde_json::{Value, json};
use std::sync::Arc;
use support::{fake, fixture};
use tokio::io::{AsyncBufReadExt, BufReader};

#[test]
fn review_1_and_8_project_parameters_do_not_grant_admission_or_reach_a_fallback() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        assert_eq!(faber_engine::project::policy::admit(vendor), Err(Blocker::Restrictions));
    }
    let (_, params) = project_thread_params(&thread(None), Role::Writer).unwrap();
    assert_eq!(params["approvalPolicy"], "never");
    assert_eq!(params["sandbox"], "workspace-write");
}

#[test]
fn review_5_automatic_server_disables_customizations_without_changing_manual_projects() {
    let args = project_codex_args(true);
    let legacy = faber_engine::codex::codex_args(Area::Project, true, true);
    assert!(!legacy.iter().any(|a| a == "mcp_servers={}"));
    for feature in ["hooks", "apps", "plugins", "computer_use", "browser_use"] {
        assert!(args.windows(2).any(|a| a == ["--disable", feature]));
    }
    assert!(args.windows(2).any(|a| a == ["-c", "mcp_servers={}"]));
}

#[test]
fn review_6_project_controls_include_the_installed_restricted_and_safe_mode_flags() {
    let args = project_claude_args(&launch(None), Role::Writer).unwrap();
    assert!(args.iter().any(|a| a == "--restricted"));
    assert!(args.iter().any(|a| a == "--safe-mode"));
}

#[test]
fn the_live_probe_writer_does_not_inherit_network_temp_roots_shell_or_subagents() {
    let (_, params) = project_thread_params(&thread(None), Role::Writer).unwrap();
    assert_eq!(params["config"]["sandbox_workspace_write"]["network_access"], false);
    assert_eq!(params["config"]["sandbox_workspace_write"]["exclude_tmpdir_env_var"], true);
    assert_eq!(params["config"]["sandbox_workspace_write"]["exclude_slash_tmp"], true);
    for feature in ["shell_tool", "multi_agent", "multi_agent_v2"] {
        assert!(project_codex_args(true).windows(2).any(|a| a == ["--disable", feature]));
    }
}

#[tokio::test]
async fn review_4_every_escape_request_receives_a_native_denial_on_the_fake_wire() {
    let entries = fixture("projects", "policy-escapes");
    let mut replay = vec![];
    for entry in entries {
        let request = entry.msg["request_id"].as_str().map(String::from);
        replay.push(entry);
        if let Some(request) = request {
            replay.push(support::Entry {
                input: true,
                msg: json!({"type":"control_response", "response":{"request_id":request,"response":{"behavior":"deny"}}}),
            });
        }
    }
    let f = fake(replay);
    let p = (f.spawn)("fake", &[], ".".as_ref()).unwrap();
    let mut stdin = p.stdin;
    let mut lines = BufReader::new(p.stdout).lines();
    let mut denied = 0;
    while denied < 9 {
        let line = tokio::time::timeout(std::time::Duration::from_secs(1), lines.next_line()).await.unwrap().unwrap().unwrap();
        let msg: Value = serde_json::from_str(&line).unwrap();
        if faber_engine::project::policy::reject_native(&mut *stdin, Vendor::Claude, &msg).await.unwrap() {
            denied += 1;
        }
    }
    assert_eq!(f.wire.lines_with("\"behavior\":\"deny\""), 9);
    assert!(f.mismatches.lock().unwrap().is_empty());
}

#[tokio::test]
async fn review_1_codex_native_approvals_are_declined_and_malformed_requests_fail_closed() {
    for method in ["item/commandExecution/requestApproval", "item/fileChange/requestApproval"] {
        let (spawn, mut cli, _output) = support::controlled();
        let mut p = spawn("fake", &[], ".".as_ref()).unwrap();
        let msg = json!({"id":17,"method":method,"params":{"command":"git reset --hard"}});
        assert!(faber_engine::project::policy::reject_native(&mut *p.stdin, Vendor::Codex, &msg).await.unwrap());
        assert_eq!(support::read_message(&mut cli).await, json!({"id":17,"result":{"decision":"decline"}}));
        let malformed = json!({"method":method});
        assert!(faber_engine::project::policy::reject_native(&mut *p.stdin, Vendor::Codex, &malformed).await.is_err());
    }
}

const ROLES: [Role; 5] = [Role::Lead, Role::Research, Role::TestProposal, Role::Writer, Role::Reviewer];

#[test]
fn opus_2_production_and_probe_use_the_same_pinned_provider() {
    assert!(project_codex_args(true).windows(2).any(|a| a == ["-c", "model_provider=\"openai\""]));
}

#[test]
fn opus_3_and_9_residual_native_tools_cannot_unlock_any_role() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        assert!(blockers(vendor).contains(&Blocker::Restrictions));
        assert_eq!(faber_engine::project::policy::admit(vendor), Err(Blocker::Restrictions));
    }
}

#[test]
fn opus_10_role_resume_rejects_flag_values_before_spawn() {
    for id in ["--add-dir", "-x", "saved-session", "", "--help"] {
        assert_eq!(project_claude_args(&launch(Some(id)), Role::Writer), Err(Blocker::Invocation));
    }
}

fn launch(resume: Option<&str>) -> ClaudeLaunch {
    ClaudeLaunch {
        area: Area::Project,
        model: Some("claude-sonnet-4-6".into()),
        effort: Some("high".into()),
        resume: resume.map(String::from),
        user_hooks: true,
        instructions: None,
        user_name: None,
    }
}

fn thread(resume: Option<&str>) -> CodexThread {
    CodexThread {
        cwd: ".".into(),
        model: Some("gpt-6".into()),
        effort: Some("high".into()),
        resume: resume.map(String::from),
        instructions: None,
        user_name: None,
    }
}

fn after<'a>(args: &'a [String], flag: &str) -> &'a str {
    &args[args.iter().position(|a| a == flag).unwrap() + 1]
}

#[test]
fn project_controls_are_explicit_on_launch_and_resume_without_changing_chat() {
    for role in ROLES {
        for resume in [None, Some("5e27c797-25b6-434d-a0a2-0e0e0c32ef9e")] {
            let args = project_claude_args(&launch(resume), role).unwrap();
            assert_eq!(after(&args, "--model"), "claude-sonnet-4-6");
            assert_eq!(after(&args, "--effort"), "high");
            assert_eq!(after(&args, "--permission-mode"), "dontAsk");
            assert_eq!(after(&args, "--tools"), "");
            assert!(args.iter().any(|a| a == "--strict-mcp-config"));
            assert!(args.iter().any(|a| a == "--disable-slash-commands"));
            let settings: Value = serde_json::from_str(after(&args, "--settings")).unwrap();
            assert_eq!(settings["disableAllHooks"], true);
            let denied = after(&args, "--disallowedTools");
            for tool in ["Bash", "Write", "Edit", "Agent", "Task", "Bash(git push *)", "Bash(git reset *)", "Bash(git update-ref *)"] {
                assert!(denied.split(',').any(|d| d == tool), "{tool}");
            }
            let (method, params) = project_thread_params(&thread(resume), role).unwrap();
            assert_eq!(method, if resume.is_some() { "thread/resume" } else { "thread/start" });
            assert_eq!(params["model"], "gpt-6");
            assert_eq!(params["config"]["model_reasoning_effort"], "high");
            assert_eq!(params["approvalPolicy"], "never");
            assert_eq!(params["sandbox"], if role == Role::Writer { "workspace-write" } else { "read-only" });
            if let Some(id) = resume {
                assert_eq!(after(&args, "--resume"), id);
                assert_eq!(params["threadId"], id);
            }
        }
    }
}

#[test]
fn project_policy_rejects_hidden_defaults_and_side_question_policy() {
    for role in ROLES {
        for missing_model in [true, false] {
            let mut l = launch(None);
            let mut t = thread(None);
            if missing_model {
                l.model = None;
                t.model = None;
            } else {
                l.effort = None;
                t.effort = None;
            }
            assert!(project_claude_args(&l, role).is_err());
            assert!(project_thread_params(&t, role).is_err());
        }
        let mut l = launch(None);
        l.instructions = Some("Route a side question".into());
        let mut t = thread(None);
        t.instructions = l.instructions.clone();
        assert!(project_claude_args(&l, role).is_err());
        assert!(project_thread_params(&t, role).is_err());
    }
}

#[tokio::test]
async fn every_automated_role_is_denied_before_spawn_even_with_explicit_controls() {
    for role in ROLES {
        for resume in [None, Some("5e27c797-25b6-434d-a0a2-0e0e0c32ef9e")] {
            let f = fake(vec![]);
            let opts = ClaudeOptions { bin: "fake".into(), cwd: ".".into(), launch: launch(resume), spawn: f.spawn.clone() };
            assert_eq!(start_project_claude(opts, role, Arc::new(|_| {})).unwrap_err(), Blocker::Restrictions);
            let opts = CodexServerOptions {
                bin: "fake".into(),
                cwd: ".".into(),
                area: Area::Project,
                user_hooks: false,
                windows: true,
                spawn: f.spawn.clone(),
            };
            assert_eq!(CodexServer::start_project(opts, thread(resume), role, Arc::new(|_| {})).unwrap_err(), Blocker::Restrictions);
            assert!(f.args.lock().unwrap().is_empty(), "a blocked role must not even spawn an idle CLI");
            assert_eq!(f.wire.len(), 0);
        }
    }
}

#[test]
fn advertised_capabilities_are_information_not_authority_or_a_spending_bound() {
    let msg = json!({"type":"system", "subtype":"init", "capabilities":["interrupt", "tools", "max-budget-usd"]});
    assert_eq!(Capabilities::from_init(&msg).unwrap().names, ["interrupt", "tools", "max-budget-usd"]);
    assert!(Capabilities::from_init(&json!({"type":"result", "capabilities":[]})).is_none());
    assert!(Capabilities::from_init(&json!({"type":"system", "subtype":"init", "capabilities":["interrupt",42]})).is_none());
    assert!(Capabilities::from_init(&json!({"type":"system", "subtype":"init"})).is_none());
    for vendor in [Vendor::Claude, Vendor::Codex] {
        #[cfg(windows)]
        assert_eq!(blockers(vendor), [Blocker::Restrictions, Blocker::Exposure]);
        #[cfg(not(windows))]
        assert_eq!(blockers(vendor), [Blocker::Restrictions, Blocker::ProcessTree, Blocker::Exposure]);
    }
}

#[test]
fn terminal_reasons_are_fixed_keys_including_unknown_and_interrupted_results() {
    for (raw, key) in [
        ("success", "reason.complete"),
        ("completed", "reason.complete"),
        ("interrupted", "reason.interrupted"),
        ("error_max_turns", "reason.turnLimit"),
        ("error_max_budget_usd", "reason.budget"),
        ("failed", "reason.failed"),
        ("private provider detail", "reason.unknown"),
        ("", "reason.unknown"),
    ] {
        assert_eq!(terminal_reason(raw), key);
    }
}
