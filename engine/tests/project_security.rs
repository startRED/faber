use faber_engine::claude::{ClaudeLaunch, claude_args};
use faber_engine::codex::{CodexThread, codex_args, thread_params};
use faber_engine::model::{Area, Permissions, Settings};
use serde_json::{Value, json};

fn launch(area: Area, permissions: Permissions) -> ClaudeLaunch {
    ClaudeLaunch { area, permissions, model: None, effort: None, resume: None, user_hooks: false, instructions: None, user_name: None }
}

fn settings(args: &[String]) -> Value {
    serde_json::from_str(&args[args.iter().position(|a| a == "--settings").expect("host settings") + 1]).unwrap()
}

#[test]
fn review3_catch_all_routes_untrusted_tools_to_the_host_in_both_areas() {
    let root = tempfile::tempdir().unwrap();
    for area in [Area::Chat, Area::Project] {
        let config = settings(&claude_args(&launch(area, Permissions::DontAsk)));
        assert!(config["permissions"]["ask"].as_array().unwrap().contains(&json!("*")));
        let args = claude_args(&launch(area, Permissions::DontAsk));
        for rule in ["Read(~/.claude/**)", "Read(~/.codex/**)"] {
            assert!(args.contains(&rule.into()));
        }
        for tool in ["WebFetch", "Agent", "Skill", "mcp__github__push"] {
            assert!(!faber_engine::permissions::allows(root.path(), tool, &json!({"url":"https://example.invalid"})), "{tool}");
        }
    }
}

#[test]
fn review3_codex_credential_denies_survive_in_both_areas_and_on_resume() {
    for area in [Area::Chat, Area::Project] {
        for resume in [None, Some("thread".into())] {
            let (_, params) = thread_params(
                area,
                &CodexThread {
                    cwd: "project".into(),
                    model: None,
                    effort: None,
                    resume,
                    instructions: None,
                    user_name: None,
                    permissions: Permissions::DontAsk,
                },
            );
            let config = &params["config"];
            let id = config["default_permissions"].as_str().unwrap();
            let fs = &config[format!("permissions.{id}")]["filesystem"];
            for path in ["~/.claude", "~/.codex"] {
                assert_eq!(fs[path], "deny", "{path}");
            }
            for path in [".claude", ".codex", ".gemini"] {
                assert_eq!(fs[":workspace_roots"][path], "deny", "{path}");
            }
        }
    }
}

#[test]
fn review3_codex_unattended_projects_disable_integrations_and_pin_mxc() {
    let args = codex_args(Area::Project, true, false, Permissions::DontAsk);
    for feature in ["apps", "plugins", "computer_use", "browser_use"] {
        assert!(args.windows(2).any(|pair| pair == ["--disable", feature]), "{feature}");
    }
    assert!(args.contains(&"mcp_servers={}".into()));
    assert!(args.contains(&"windows.sandbox=\"mxc\"".into()));
    assert!(!args.iter().any(|arg| arg.contains("unelevated")));
}

#[test]
fn review3_unattended_startup_cannot_run_integration_hooks_even_after_opt_in() {
    for area in [Area::Chat, Area::Project] {
        let mut cli = launch(area, Permissions::DontAsk);
        cli.user_hooks = true;
        let args = claude_args(&cli);
        assert!(args.contains(&"--safe-mode".into()) && args.contains(&"--strict-mcp-config".into()));
        assert_eq!(settings(&args)["disableAllHooks"], true);
        let args = codex_args(area, true, true, Permissions::DontAsk);
        assert!(args.windows(2).any(|pair| pair == ["--disable", "hooks"]));
        assert!(args.windows(2).any(|pair| pair == ["--disable", "skill_mcp_dependency_install"]));
    }
}

#[test]
fn review3_codex_shells_use_the_shared_git_gate_instead_of_native_execution() {
    for area in [Area::Chat, Area::Project] {
        let (_, params) = thread_params(
            area,
            &CodexThread {
                cwd: ".".into(),
                model: None,
                effort: None,
                resume: None,
                instructions: None,
                user_name: None,
                permissions: Permissions::DontAsk,
            },
        );
        assert_eq!(params["config"]["features.shell_tool"], false);
        assert_eq!(params["dynamicTools"][0]["name"], "run_command");
    }
}

#[test]
fn review3_live_captures_contain_actual_denied_file_operations_and_git_requests() {
    for data in
        [include_str!("../../tests/fixtures/security-probes/claude.json"), include_str!("../../tests/fixtures/security-probes/codex.json")]
    {
        let capture: Value = serde_json::from_str(data).unwrap();
        assert_eq!(capture["finished"], true);
        assert!(capture["engineError"].is_null());
        let tools = capture["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        for tool in &tools[..2] {
            let output = tool["output"].as_str().unwrap();
            assert!(output.contains("EPERM") && output.contains("outside"), "startup errors are not filesystem proof");
            assert_eq!(tool["ok"], false);
        }
        assert!(tools[2]["input"]["command"].as_str().unwrap().contains("reset --hard HEAD~1"));
        assert_eq!(tools[2]["ok"], false);
        for finding in ["outsideReadObserved", "outsideWriteChanged", "publishedHeadChanged"] {
            assert_eq!(capture["evidence"][finding], false);
        }
    }
}

#[test]
fn review3_node_tools_do_not_need_broad_ancestor_read_grants() {
    let root = tempfile::tempdir().unwrap();
    let env = faber_engine::permissions::child_env(root.path());
    assert_eq!(env["NODE_OPTIONS"], "--preserve-symlinks --preserve-symlinks-main");
}

#[test]
fn review3_live_findings_require_a_profile_flag_and_valid_git_subtree_rules() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("repo/.git")).unwrap();
    let config = faber_engine::permissions::sandbox_config(root.path(), true);
    let id = config["default_permissions"].as_str().unwrap();
    let fs = config[&format!("permissions.{id}")]["filesystem"][":workspace_roots"].as_object().unwrap();
    assert!(fs.iter().all(|(path, access)| !path.contains('*') || access != "read"), "read globs require an exact subtree prefix");
    assert_eq!(fs["repo/.git"], "read");
    let cli = faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] };
    let argv =
        faber_engine::permissions::command_argv(root.path(), "Bash", &json!({"command":"cargo test"}), Some(&cli), &|_| None).unwrap();
    assert!(argv.iter().any(|arg| arg == "--permission-profile"));
}

#[test]
#[cfg(any(windows, target_os = "linux"))] // APFS refuses creation of non-UTF-8 names.
fn review3_unrepresentable_git_bindings_make_the_workspace_read_only() {
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[0xd800])
    };
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(vec![0xff])
    };
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(name).join(".git")).unwrap();
    let config = faber_engine::permissions::sandbox_config(root.path(), true);
    let id = config["default_permissions"].as_str().unwrap();
    assert_eq!(config[&format!("permissions.{id}")]["filesystem"][":workspace_roots"]["."], "read");
}

#[test]
fn review3_project_local_auth_files_are_unreadable_and_denied_in_the_profile() {
    let root = tempfile::tempdir().unwrap();
    let config = faber_engine::permissions::sandbox_config(root.path(), true);
    let id = config["default_permissions"].as_str().unwrap();
    let fs = &config[&format!("permissions.{id}")]["filesystem"][":workspace_roots"];
    for name in [".credentials.json", "credentials.json", "auth.json"] {
        std::fs::write(root.path().join(name), "synthetic").unwrap();
        assert!(!faber_engine::permissions::allows(root.path(), "Read", &json!({"file_path":name})), "{name}");
        assert_eq!(fs[format!("**/{name}")], "deny", "{name}");
    }
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
        assert!(params.get("sandbox").is_none());
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
fn review2_m2_child_git_parameters_cannot_override_host_configuration() {
    let config = settings(&claude_args(&launch(Area::Project, Permissions::DontAsk)));
    assert_eq!(config["env"]["GIT_CONFIG_PARAMETERS"], "");
    let args = claude_args(&launch(Area::Project, Permissions::DontAsk));
    assert!(args.windows(2).any(|pair| pair == ["--setting-sources", "user"]));
}

#[test]
fn review2_h2_codex_selects_a_restricted_read_profile_on_start_and_resume() {
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
        assert!(params.get("sandbox").is_none(), "legacy workspace-write must not override restricted reads");
        let config = &params["config"];
        let id = config["default_permissions"].as_str().expect("named project profile");
        let profile = &config[format!("permissions.{id}")];
        assert_eq!(profile["extends"], ":workspace");
        assert_eq!(profile["filesystem"][":root"], "deny");
        assert_eq!(profile["filesystem"][":minimal"], "read");
        assert_eq!(profile["filesystem"][":workspace_roots"][".git"], "read");
        assert_eq!(profile["network"]["enabled"], false);
        if cfg!(windows) {
            assert_eq!(config["windows.sandbox"], "mxc");
        }
    }
}

#[test]
fn m3_all_git_commands_reach_the_host_instead_of_only_named_destructive_forms() {
    let config = settings(&claude_args(&launch(Area::Project, Permissions::DontAsk)));
    assert!(config["permissions"]["ask"].as_array().unwrap().contains(&json!("Bash")));
}

#[test]
fn r107_chat_reuses_project_protections_and_existing_users_default_to_dont_ask() {
    let chat = settings(&claude_args(&launch(Area::Chat, Permissions::DontAsk)));
    let project = settings(&claude_args(&launch(Area::Project, Permissions::DontAsk)));
    for key in ["permissions", "sandbox", "env"] {
        assert_eq!(chat[key], project[key], "shared protection {key}");
    }
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
    assert!(params.get("sandbox").is_none());
    assert_eq!(params["approvalPolicy"], "never");
    assert_eq!(params["config"]["sandbox_workspace_write.writable_roots"], json!([]));
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

#[test]
fn r107_internal_questions_restrict_reads_and_never_grant_workspace_writes() {
    for area in [Area::Chat, Area::Project] {
        for permissions in [Permissions::Ask, Permissions::DontAsk] {
            let (_, params) = thread_params(
                area,
                &CodexThread {
                    cwd: "private-question".into(),
                    model: None,
                    effort: None,
                    resume: None,
                    instructions: Some("Classify this request without tools.".into()),
                    user_name: None,
                    permissions,
                },
            );
            assert_eq!(params["approvalPolicy"], "never");
            assert_eq!(params["ephemeral"], true);
            assert!(params.get("sandbox").is_none(), "legacy read-only permits disk-wide reads");
            let config = &params["config"];
            let id = config["default_permissions"].as_str().expect("restricted internal-question profile");
            let profile = &config[format!("permissions.{id}")];
            assert_eq!(profile["filesystem"][":root"], "deny");
            assert_eq!(profile["network"]["enabled"], false);
            let roots = profile["filesystem"][":workspace_roots"].as_object().unwrap();
            assert_eq!(roots["."], "read");
            assert!(!roots.values().any(|permission| permission == "write"));
        }
    }
}
