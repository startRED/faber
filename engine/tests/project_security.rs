use faber_engine::claude::{ClaudeLaunch, claude_args};
use faber_engine::codex::{CodexThread, thread_params};
use faber_engine::model::{Area, Permissions, Settings};
use serde_json::{Value, json};

fn launch(area: Area, permissions: Permissions) -> ClaudeLaunch {
    ClaudeLaunch { area, permissions, model: None, effort: None, resume: None, user_hooks: false, instructions: None, user_name: None }
}

fn settings(args: &[String]) -> Value {
    serde_json::from_str(&args[args.iter().position(|a| a == "--settings").expect("host settings") + 1]).unwrap()
}

#[test]
fn c1_project_reads_are_confined_and_credentials_and_updater_keys_are_denied() {
    let args = claude_args(&launch(Area::Project, Permissions::DontAsk));
    let config = settings(&args);
    assert_eq!(config["permissions"]["blockReadsOutsideWorkingDirectories"], true);
    for rule in [
        "Read(~/.claude.json)",
        "Read(~/.git-credentials)",
        "Read(~/.npmrc)",
        "Read(~/AppData/Roaming/GitHub CLI/**)",
        "Read(~/.config/gh/**)",
        "Read(//**/*updater.key*)",
    ] {
        assert!(args.contains(&rule.into()), "missing {rule}");
    }
}

#[test]
fn c1_h4_codex_network_is_off_on_start_and_resume_even_if_user_settings_enable_it() {
    for resume in [None, Some("thread".into())] {
        let t = CodexThread {
            cwd: "project".into(),
            model: None,
            effort: None,
            resume,
            instructions: None,
            user_name: None,
            permissions: Permissions::DontAsk,
        };
        let (_, params) = thread_params(Area::Project, &t);
        assert_eq!(params["approvalPolicy"], "never");
        assert_eq!(params["sandbox"], "workspace-write");
        assert_eq!(params["config"]["sandbox_workspace_write.network_access"], false);
    }
}

#[test]
fn h2_h3_m1_file_writes_and_shell_commands_reach_the_host_policy() {
    let args = claude_args(&launch(Area::Project, Permissions::DontAsk));
    let config = settings(&args);
    for rule in ["Read(//**)", "Edit(//**)", "Bash", "PowerShell"] {
        assert!(config["permissions"]["ask"].as_array().unwrap().contains(&json!(rule)), "missing {rule}");
    }
    assert!(args.contains(&"Edit(//**/.git/**)".into()));
    assert!(!args.contains(&"Bash(rm *)".into()), "local deletion must reach the policy instead of a blanket deny");
}

#[test]
fn m2_l1_native_denies_do_not_match_harmless_messages_or_environment_templates() {
    let args = claude_args(&launch(Area::Project, Permissions::DontAsk));
    for rule in ["Bash(git * push *)", "Bash(git * rebase *)", "Read(//**/.env.*)"] {
        assert!(!args.contains(&rule.into()), "overbroad {rule}");
    }
}

#[test]
fn h4_remote_transports_and_github_tokens_are_disabled_for_project_children() {
    let args = claude_args(&launch(Area::Project, Permissions::DontAsk));
    let config = settings(&args);
    assert_eq!(config["env"]["GIT_ALLOW_PROTOCOL"], "");
    for token in ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"] {
        assert_eq!(config["env"][token], "");
    }
    assert_eq!(config["env"]["GIT_CONFIG_VALUE_0"], "false");
    assert_eq!(config["env"]["GIT_CONFIG_VALUE_1"], "/dev/null");
    assert_eq!(config["env"]["GIT_OPTIONAL_LOCKS"], "0");
}

#[test]
fn m3_all_git_commands_reach_the_host_instead_of_only_named_destructive_forms() {
    let config = settings(&claude_args(&launch(Area::Project, Permissions::DontAsk)));
    assert!(config["permissions"]["ask"].as_array().unwrap().contains(&json!("Bash")));
}

#[test]
fn h1_m4_l3_the_owner_scoped_permissions_to_projects_and_existing_users_default_to_dont_ask() {
    assert_eq!(claude_args(&launch(Area::Chat, Permissions::DontAsk)), claude_args(&launch(Area::Chat, Permissions::Ask)));
    for old in [json!({}), json!({ "setup": "done" })] {
        let old: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(old.permissions.unwrap_or_default(), Permissions::DontAsk);
    }
}

#[test]
fn m6_sandbox_failures_are_intended_and_writable_roots_must_not_expand_into_user_caches() {
    let t = CodexThread {
        cwd: "project".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        user_name: None,
        permissions: Permissions::DontAsk,
    };
    let (_, params) = thread_params(Area::Project, &t);
    assert_eq!(params["sandbox"], "workspace-write");
    assert_eq!(params["approvalPolicy"], "never");
    assert!(params["config"].get("sandbox_workspace_write.writable_roots").is_none());
    let error = faber_engine::codex::parse_codex_notification(
        &json!({ "method": "item/completed", "params": { "item": {
        "type": "commandExecution", "id": "build", "status": "completed", "exitCode": 1,
        "aggregatedOutput": "Permission denied: registry cache" } } }),
        &mut faber_engine::codex::CodexTurnState::default(),
    );
    assert!(
        error.iter().any(
            |e| matches!(e, faber_engine::events::EngineEvent::ToolResult { ok: false, text, .. } if text.contains("Permission denied"))
        )
    );
}
