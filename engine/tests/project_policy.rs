use crate::support;

use faber_engine::claude::{ClaudeLaunch, ClaudeOptions, project_claude_args, start_project_claude};
use faber_engine::codex::{CodexServer, CodexServerOptions, CodexThread, project_codex_args, project_thread_params};
use faber_engine::events::{Capabilities, terminal_reason};
use faber_engine::model::{Area, Permissions, Vendor};
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
    assert_eq!(profile(&params)["filesystem"][":workspace_roots"]["."], "write");
}

#[test]
fn review_5_automatic_server_disables_customizations_without_changing_manual_projects() {
    let args = project_codex_args(true);
    let legacy = faber_engine::codex::codex_args(Area::Project, true, true, Permissions::Ask);
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
    // The person chose full access (see `thread`); the role still gets Faber's own profile.
    let profile = profile(&params);
    assert_eq!(profile["network"]["enabled"], false);
    assert_eq!(profile["filesystem"][":tmpdir"], "deny");
    assert_eq!(profile["filesystem"][":slash_tmp"], "deny");
    assert_eq!(profile["filesystem"]["~/.codex"], "deny");
    assert_eq!(params["config"]["sandbox_workspace_write.network_access"], false);
    assert_eq!(params["config"]["features.shell_tool"], false);
    assert!(params.get("sandbox").is_none() && params.get("dynamicTools").is_none(), "{params}");
    for feature in ["shell_tool", "multi_agent", "multi_agent_v2"] {
        assert!(project_codex_args(true).windows(2).any(|a| a == ["--disable", feature]));
    }
}

#[test]
fn every_codex_role_uses_a_closed_native_catalog_and_no_inherited_developer_instructions() {
    let args = project_codex_args(true);
    assert!(args.windows(2).any(|a| a[0] == "-c" && a[1].starts_with("model_catalog_json=")));
    for feature in ["view_image", "request_permissions_tool", "code_mode", "code_mode_only", "token_budget"] {
        assert!(args.windows(2).any(|a| a == ["--disable", feature]), "{feature}");
    }
    for setting in ["developer_instructions=\"\"", "tools.update_plan.enabled=false", "tools.experimental_request_user_input.enabled=false"]
    {
        assert!(args.windows(2).any(|a| a == ["-c", setting]), "{setting}");
    }
    let catalog = faber_engine::codex::project_codex_catalog();
    let models = catalog["models"].as_array().unwrap();
    // The installed catalog parser requires one instruction template even when the role supplies its own.
    assert_eq!(models[0]["model_messages"]["instructions_template"], "");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["slug"], "gpt-6-luna");
    assert_eq!(models[0]["shell_type"], "disabled");
    assert!(models[0]["apply_patch_tool_type"].is_null());
    assert_eq!(models[0]["experimental_supported_tools"], json!([]));
    assert_eq!(models[0]["tool_mode"], "direct");
    assert_eq!(models[0]["include_skills_usage_instructions"], false);
    for role in ROLES {
        for resume in [None, Some("thread")] {
            let (_, params) = project_thread_params(&thread(resume), role).unwrap();
            assert!(params.get("developerInstructions").is_none(), "{params}");
            assert_eq!(params["config"]["features.shell_tool"], false);
        }
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

#[tokio::test]
async fn unknown_permission_requests_stop_instead_of_hanging_or_guessing_a_decline() {
    let (spawn, _, _) = support::controlled();
    let mut process = spawn("fake", &[], ".".as_ref()).unwrap();
    for line in include_str!("../../tests/fixtures/projects/unknown-permissions.jsonl").lines() {
        let entry: Value = serde_json::from_str(line).unwrap();
        let vendor = if entry["vendor"] == "codex" { Vendor::Codex } else { Vendor::Claude };
        assert!(faber_engine::project::policy::reject_native(&mut *process.stdin, vendor, &entry["msg"]).await.is_err(), "{entry}");
    }
}

#[tokio::test]
async fn known_permission_and_elicitation_denials_use_their_protocol_forms() {
    for (method, result) in [
        ("item/permissions/requestApproval", json!({"permissions":{},"scope":"turn"})),
        ("mcpServer/elicitation/request", json!({"action":"decline","content":null})),
    ] {
        let (spawn, mut cli, _) = support::controlled();
        let mut process = spawn("fake", &[], ".".as_ref()).unwrap();
        let request = json!({"id":"deny","method":method,"params":{"threadId":"t"}});
        assert!(faber_engine::project::policy::reject_native(&mut *process.stdin, Vendor::Codex, &request).await.unwrap());
        assert_eq!(support::read_message(&mut cli).await, json!({"id":"deny","result":result}));
    }
}

#[tokio::test]
async fn ungranted_native_completions_fail_closed_for_both_vendors() {
    let (spawn, _, _) = support::controlled();
    let mut process = spawn("fake", &[], ".".as_ref()).unwrap();
    for kind in ["fileChange", "commandExecution", "webSearch", "mcpToolCall", "dynamicToolCall", "unknownFutureTool"] {
        for status in ["completed", "failed", "declined"] {
            let msg = json!({"method":"item/completed","params":{"item":{"type":kind,"status":status}}});
            let result = faber_engine::project::policy::reject_native(&mut *process.stdin, Vendor::Codex, &msg).await;
            assert_eq!(result.is_err(), status == "completed");
        }
    }
    for msg in [
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"NotebookEdit"}]}}),
        json!({"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","name":"Bash"}}}),
    ] {
        assert!(faber_engine::project::policy::reject_native(&mut *process.stdin, Vendor::Claude, &msg).await.is_err());
    }
    for (vendor, msg) in [
        (Vendor::Codex, json!({"method":"item/completed","params":{"item":{"type":"agentMessage"}}})),
        (Vendor::Claude, json!({"type":"assistant","message":{"content":[{"type":"text","text":"<Bash>literal text</Bash>"}]}})),
    ] {
        assert!(!faber_engine::project::policy::reject_native(&mut *process.stdin, vendor, &msg).await.unwrap());
    }
}

#[test]
fn t13_shared_exposure_discards_values_and_never_uses_missing_or_malformed_configuration() {
    let entry: Value = serde_json::from_str(include_str!("../../tests/fixtures/projects/inherited-mcp.jsonl").trim()).unwrap();
    for role in ROLES {
        let (_, mut params) = project_thread_params(&thread(None), role).unwrap();
        faber_engine::codex::thread_exposure(&mut params, &entry["msg"].to_string()).unwrap();
        assert_eq!(params["config"]["mcp_servers"].as_object().unwrap().len(), 4);
        assert!(!params.to_string().contains("synthetic"));
        assert!(!params.to_string().contains("SYNTHETIC"));
        assert_eq!(params["config"]["model_reasoning_effort"], "high");
        for bad in [
            "{}",
            "not JSON",
            r#"{"error":{"message":"SYNTHETIC"}}"#,
            r#"{"result":{"config":{"mcp_servers":{"":{}}}}}"#,
            r#"{"result":{"config":{"mcp_servers":[]}}}"#,
        ] {
            assert_eq!(faber_engine::codex::thread_exposure(&mut params, bad), Err(Blocker::Restrictions));
        }
    }
}

#[test]
fn t13_every_role_gets_explicit_empty_strict_mcp_and_no_user_startup_integrations() {
    for role in ROLES {
        for resume in [None, Some("550e8400-e29b-41d4-a716-446655440000")] {
            let args = project_claude_args(&launch(resume), role).unwrap();
            assert!(args.windows(2).any(|pair| pair == ["--mcp-config", "{\"mcpServers\":{}}"]));
            assert!(args.iter().any(|arg| arg == "--strict-mcp-config"));
            assert!(args.iter().any(|arg| arg == "--safe-mode"));
            assert!(args.windows(2).any(|pair| pair == ["--tools", ""]));
            let settings: Value = serde_json::from_str(&args[args.iter().position(|a| a == "--settings").unwrap() + 1]).unwrap();
            assert_eq!(settings["disableAllHooks"], true);
            assert_eq!(settings["env"]["CLAUDE_CODE_MAX_OUTPUT_TOKENS"], if role == Role::Writer { "32000" } else { "8000" });
            assert_eq!(after(&args, "--max-budget-usd"), "0.005");
            assert_eq!(after(&args, "--max-turns"), "1");
        }
    }
}

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
        helper: Default::default(),
        browse: false,
        area: Area::Project,
        model: Some("claude-sonnet-4-6".into()),
        effort: Some("high".into()),
        resume: resume.map(String::from),
        user_hooks: true,
        instructions: None,
        user_name: None,
        permissions: Permissions::FullAccess,
        // A CLI that knows every newer flag: full access would run it with nobody to answer (#28).
        flags: faber_engine::claude::ClaudeFlags { autocompact: true, permission_prompts: true, auto_mode: false },
        tuning: Default::default(),
    }
}

/// Faber's own permission profile in a role's thread parameters.
fn profile(params: &Value) -> &Value {
    params["config"].as_object().unwrap().iter().find(|(k, _)| k.starts_with("permissions.")).map(|(_, v)| v).unwrap()
}

fn thread(resume: Option<&str>) -> CodexThread {
    CodexThread {
        helper: Default::default(),
        browse: false,
        cwd: ".".into(),
        model: Some("gpt-6".into()),
        effort: Some("high".into()),
        resume: resume.map(String::from),
        instructions: None,
        user_name: None,
        permissions: Permissions::FullAccess,
        tuning: Default::default(),
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
            // The person chose full access (see `launch`); a role never inherits it.
            assert!(!args.iter().any(|a| a == "bypassPermissions"), "{args:?}");
            // Nor the full-access path that denies requests for want of anyone to answer them.
            assert!(!args.iter().any(|a| a == "--permission-prompts"), "{args:?}");
            assert_eq!(after(&args, "--permission-prompt-tool"), "stdio");
            assert_eq!(after(&args, "--tools"), "");
            assert!(args.iter().any(|a| a == "--strict-mcp-config"));
            assert!(args.iter().any(|a| a == "--disable-slash-commands"));
            let settings: Value = serde_json::from_str(after(&args, "--settings")).unwrap();
            assert_eq!(settings["disableAllHooks"], true);
            assert_eq!(args.iter().filter(|a| *a == "--settings").count(), 1, "{args:?}");
            let deny = settings["permissions"]["deny"].as_array().unwrap();
            assert!(deny.iter().any(|r| r.as_str().is_some_and(|r| r.starts_with("Read(") && r.contains(".codex"))), "{deny:?}");
            let denied = after(&args, "--disallowedTools");
            for tool in [
                "Bash",
                "Write",
                "Edit",
                "NotebookEdit",
                "Agent",
                "Task",
                "Bash(git push *)",
                "Bash(git reset *)",
                "Bash(git update-ref *)",
            ] {
                assert!(denied.split(',').any(|d| d == tool), "{tool}");
            }
            let (method, params) = project_thread_params(&thread(resume), role).unwrap();
            assert_eq!(method, if resume.is_some() { "thread/resume" } else { "thread/start" });
            assert_eq!(params["model"], "gpt-6");
            assert_eq!(params["config"]["model_reasoning_effort"], "high");
            assert_eq!(params["approvalPolicy"], "never");
            assert_eq!(profile(&params)["filesystem"][":workspace_roots"]["."], if role == Role::Writer { "write" } else { "read" });
            assert!(params.get("sandbox").is_none() && params.get("dynamicTools").is_none(), "{params}");
            if let Some(id) = resume {
                assert_eq!(after(&args, "--resume"), id);
                assert_eq!(params["threadId"], id);
            }
        }
    }
}

#[test]
fn uncached_invocation_bound_disables_more_expensive_cache_writes_for_every_role() {
    for role in ROLES {
        for resume in [None, Some("5e27c797-25b6-434d-a0a2-0e0e0c32ef9e")] {
            let args = project_claude_args(&launch(resume), role).unwrap();
            let settings: Value = serde_json::from_str(after(&args, "--settings")).unwrap();
            assert_eq!(settings["env"]["DISABLE_PROMPT_CACHING"], "1");
            for (name, value) in [
                ("DISABLE_COMPACT", "1"),
                ("CLAUDE_CODE_MAX_RETRIES", "0"),
                ("CLAUDE_CODE_NONSTREAMING_TIMEOUT_RETRIES", "0"),
                ("CLAUDE_CODE_DISABLE_NONSTREAMING_FALLBACK", "1"),
            ] {
                assert_eq!(settings["env"][name], value, "{role:?}: {name}");
            }
        }
    }
    let args = faber_engine::claude::claude_args(&launch(None));
    assert!(!args.iter().any(|arg| arg.contains("DISABLE_PROMPT_CACHING")));
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
            let opts = ClaudeOptions { bin: "fake".into(), cwd: ".".into(), launch: launch(resume), spawn: f.spawn.clone(), sandbox: None };
            assert_eq!(start_project_claude(opts, role, Arc::new(|_| {})).unwrap_err(), Blocker::Restrictions);
            let opts = CodexServerOptions {
                permissions: Permissions::FullAccess,
                cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
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

#[test]
fn automatic_parameters_reject_moving_model_aliases_before_admission() {
    let mut c = launch(None);
    c.model = Some("claude-sonnet-latest".into());
    assert!(project_claude_args(&c, Role::Writer).is_err());
    let mut t = thread(None);
    t.model = Some("gpt-latest".into());
    assert!(project_thread_params(&t, Role::Writer).is_err());
}

#[test]
fn r140_a_role_never_takes_the_mode_or_the_settings_the_person_chose() {
    use faber_engine::model::{ReasoningSummary, Tuning, Verbosity, WebSearch};
    let chosen = Tuning { web: WebSearch::Live, summary: ReasoningSummary::Detailed, verbosity: Verbosity::High };
    let off = Tuning { web: WebSearch::Off, ..chosen };
    for role in ROLES {
        for permissions in [Permissions::Auto, Permissions::AcceptEdits, Permissions::Plan, Permissions::Ask, Permissions::FullAccess] {
            // Each thread names its profile with a fresh id; the content is what counts.
            let plain = |params: &Value| {
                let text = params.to_string();
                let id = params["config"]["default_permissions"].as_str().unwrap().to_string();
                text.replace(&id, "<profile>")
            };
            let baseline = project_thread_params(&thread(None), role).unwrap();
            let (_, params) = project_thread_params(&CodexThread { permissions, tuning: chosen, ..thread(None) }, role).unwrap();
            assert_eq!(
                plain(&params),
                plain(&baseline.1),
                "{role:?} {permissions:?}: the thread of a role does not depend on the person's mode"
            );
            assert_eq!(params["config"]["web_search"], "disabled");
            assert_eq!(params["config"]["approvals_reviewer"], "user");
            assert_eq!(params["approvalPolicy"], "never");
            for key in ["model_reasoning_summary", "model_verbosity"] {
                assert!(params["config"].get(key).is_none(), "{role:?} {key}");
            }
            assert!(params.get("approvalsReviewer").is_none() && params.get("sandbox").is_none());
            let args = project_claude_args(&ClaudeLaunch { permissions, tuning: off, ..launch(None) }, role).unwrap();
            assert_eq!(args, project_claude_args(&launch(None), role).unwrap(), "{role:?} {permissions:?}");
            assert_eq!(after(&args, "--permission-mode"), "dontAsk");
        }
    }
}
