use faber_engine::claude::{ClaudeLaunch, claude_args};
use faber_engine::codex::{CodexThread, codex_args, thread_params};
use faber_engine::model::{Area, Permissions, Settings};
use serde_json::{Value, json};

fn launch(area: Area, permissions: Permissions) -> ClaudeLaunch {
    ClaudeLaunch {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        area,
        permissions,
        model: None,
        effort: None,
        resume: None,
        user_hooks: false,
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        flags: Default::default(),
        tuning: faber_engine::model::Tuning { web: faber_engine::model::WebSearch::Live, ..Default::default() },
        tools: Default::default(),
    }
}

fn settings(args: &[String]) -> Value {
    serde_json::from_str(&args[args.iter().position(|a| a == "--settings").expect("host settings") + 1]).unwrap()
}

#[test]
fn review3_catch_all_routes_untrusted_tools_to_the_host_in_both_areas() {
    let root = tempfile::tempdir().unwrap();
    for area in [Area::Chat, Area::Project] {
        let config = settings(&claude_args(&launch(area, Permissions::Isolated)));
        assert!(config["permissions"]["ask"].as_array().unwrap().contains(&json!("*")));
        let args = claude_args(&launch(area, Permissions::Isolated));
        for rule in ["Read(~/.claude/**)", "Read(~/.codex/**)"] {
            assert!(denied(&args).contains(&rule.to_string()));
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
                    leave_plan: false,
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    progress: false,
                    cwd: "project".into(),
                    model: None,
                    effort: None,
                    resume,
                    instructions: None,
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    permissions: Permissions::Isolated,
                    tuning: Default::default(),
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
    let args = codex_args(Area::Project, true, false, Permissions::Isolated);
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
        let mut cli = launch(area, Permissions::Isolated);
        cli.user_hooks = true;
        let args = claude_args(&cli);
        assert!(args.contains(&"--safe-mode".into()) && args.contains(&"--strict-mcp-config".into()));
        assert_eq!(settings(&args)["disableAllHooks"], true);
        let args = codex_args(area, true, true, Permissions::Isolated);
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
                leave_plan: false,
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
                cwd: ".".into(),
                model: None,
                effort: None,
                resume: None,
                instructions: None,
                research: None,
                user_name: None,
                group_instructions: None,
                permissions: Permissions::Isolated,
                tuning: Default::default(),
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
    let args = claude_args(&launch(Area::Project, Permissions::Isolated));
    let config = settings(&args);
    assert_eq!(config["permissions"]["blockReadsOutsideWorkingDirectories"], true);
    for rule in [
        "Read(~/.claude.json*)",
        "Read(~/.git-credentials)",
        "Read(~/.npmrc)",
        "Read(~/AppData/Roaming/GitHub CLI/**)",
        "Read(~/.config/gh/**)",
        "Read(//**/*updater.key*)",
    ] {
        assert!(denied(&args).contains(&rule.to_string()), "missing {rule}");
    }
}

#[test]
fn c1_h4_codex_network_is_off_on_start_and_resume_even_if_user_settings_enable_it() {
    for resume in [None, Some("thread".into())] {
        let t = CodexThread {
            leave_plan: false,
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            cwd: "project".into(),
            model: None,
            effort: None,
            resume,
            instructions: None,
            research: None,
            user_name: None,
            group_instructions: None,
            permissions: Permissions::Isolated,
            tuning: Default::default(),
        };
        let (_, params) = thread_params(Area::Project, &t);
        assert_eq!(params["approvalPolicy"], "never");
        assert!(params.get("sandbox").is_none());
        assert_eq!(params["config"]["sandbox_workspace_write.network_access"], false);
    }
}

#[test]
fn h2_h3_m1_file_writes_and_shell_commands_reach_the_host_policy() {
    let args = claude_args(&launch(Area::Project, Permissions::Isolated));
    let config = settings(&args);
    for rule in ["Read(//**)", "Edit(//**)", "Bash", "PowerShell"] {
        assert!(config["permissions"]["ask"].as_array().unwrap().contains(&json!(rule)), "missing {rule}");
    }
    assert!(denied(&args).contains(&"Edit(//**/.git/**)".to_string()));
    assert!(!args.contains(&"Bash(rm *)".into()), "local deletion must reach the policy instead of a blanket deny");
}

#[test]
fn m2_l1_native_denies_do_not_match_harmless_messages_or_environment_templates() {
    let args = claude_args(&launch(Area::Project, Permissions::Isolated));
    for rule in ["Bash(git * push *)", "Bash(git * rebase *)", "Read(//**/.env.*)"] {
        assert!(!args.contains(&rule.into()), "overbroad {rule}");
    }
}

#[test]
fn h4_remote_transports_and_github_tokens_are_disabled_for_project_children() {
    let args = claude_args(&launch(Area::Project, Permissions::Isolated));
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
    let config = settings(&claude_args(&launch(Area::Project, Permissions::Isolated)));
    assert_eq!(config["env"]["GIT_CONFIG_PARAMETERS"], "");
    let args = claude_args(&launch(Area::Project, Permissions::Isolated));
    assert!(args.windows(2).any(|pair| pair == ["--setting-sources", "user"]));
}

#[test]
fn review2_h2_codex_selects_a_restricted_read_profile_on_start_and_resume() {
    for resume in [None, Some("thread".into())] {
        let t = CodexThread {
            leave_plan: false,
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            cwd: "project".into(),
            model: None,
            effort: None,
            resume,
            instructions: None,
            research: None,
            user_name: None,
            group_instructions: None,
            permissions: Permissions::Isolated,
            tuning: Default::default(),
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
    let config = settings(&claude_args(&launch(Area::Project, Permissions::Isolated)));
    assert!(config["permissions"]["ask"].as_array().unwrap().contains(&json!("Bash")));
}

#[test]
fn r107_isolated_chat_reuses_project_protections_and_existing_users_default_to_full_access() {
    let chat = settings(&claude_args(&launch(Area::Chat, Permissions::Isolated)));
    let project = settings(&claude_args(&launch(Area::Project, Permissions::Isolated)));
    for key in ["permissions", "sandbox", "env"] {
        assert_eq!(chat[key], project[key], "shared protection {key}");
    }
    for old in [json!({}), json!({ "setup": "done" })] {
        let old: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(old.permissions.unwrap_or_default(), Permissions::FullAccess);
    }
}

#[test]
fn m6_sandbox_failures_are_intended_and_writable_roots_must_not_expand_into_user_caches() {
    let t = CodexThread {
        leave_plan: false,
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        cwd: "project".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: Permissions::Isolated,
        tuning: Default::default(),
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
        for permissions in [Permissions::Ask, Permissions::Isolated, Permissions::FullAccess] {
            let (_, params) = thread_params(
                area,
                &CodexThread {
                    leave_plan: false,
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    progress: false,
                    cwd: "private-question".into(),
                    model: None,
                    effort: None,
                    resume: None,
                    instructions: Some("Classify this request without tools.".into()),
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    permissions,
                    tuning: Default::default(),
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

/// The deny rules, which travel in the settings JSON (no command-line flag splits them at a space).
fn denied(args: &[String]) -> Vec<String> {
    let Some(at) = args.iter().position(|a| a == "--settings") else { return vec![] };
    let config: Value = serde_json::from_str(&args[at + 1]).unwrap();
    config["permissions"]["deny"].as_array().map(|rules| rules.iter().map(|r| r.as_str().unwrap().to_owned()).collect()).unwrap_or_default()
}

#[test]
fn r121_full_access_and_ask_deny_the_credential_folders_exactly_as_the_isolated_mode_does() {
    for area in [Area::Chat, Area::Project] {
        let full = denied(&claude_args(&launch(area, Permissions::FullAccess)));
        assert_eq!(full, denied(&claude_args(&launch(area, Permissions::Isolated))), "{area:?}");
        for rule in ["Read(~/.claude/**)", "Read(~/.codex/**)", "Read(~/.ssh/**)", "Read(~/.gemini/**)"] {
            assert!(full.contains(&rule.to_string()), "{area:?} {rule}");
        }
    }
    // Ask asks as well, and carries the same credential denies (R124b).
    for area in [Area::Chat, Area::Project] {
        assert_eq!(
            denied(&claude_args(&launch(area, Permissions::Ask))),
            denied(&claude_args(&launch(area, Permissions::FullAccess))),
            "{area:?}"
        );
    }
}

#[test]
fn r121_full_access_does_not_scrub_the_environment_so_git_and_the_github_cli_can_reach_the_network() {
    for area in [Area::Chat, Area::Project] {
        let isolated = settings(&claude_args(&launch(area, Permissions::Isolated)));
        assert_eq!(isolated["env"]["GIT_ALLOW_PROTOCOL"], "");
        assert_eq!(isolated["env"]["GH_TOKEN"], "");
        let full = claude_args(&launch(area, Permissions::FullAccess));
        let config = settings(&full);
        assert!(config.get("env").is_none() && config.get("sandbox").is_none(), "no env override, no sandbox block");
        assert!(config["permissions"].get("ask").is_none(), "no ask rules");
        assert!(!full.iter().any(|a| a.contains("GIT_ALLOW_PROTOCOL") || a.contains("GH_TOKEN") || a.contains("sandbox")));
    }
}

/// R124, item E: what a request names is cut into path pieces and compared by name, so a spelling cannot slip by.
#[test]
fn r124_a_request_naming_a_credential_folder_is_recognised_in_every_spelling_and_others_pass() {
    use faber_engine::permissions::touches_credentials;
    let bs = char::from(92);
    let none = |_: &str| None;
    for refused in [
        json!({ "file_path": format!("C:{bs}Users{bs}me{bs}.SSH{bs}id_rsa") }),
        json!({ "file_path": "C:/Users/me/.claude/settings.json" }),
        json!({ "command": "cat ~/.ssh/id_ed25519" }),
        json!({ "command": "cd ~/.codex && cat auth.json" }),
        json!({ "command": format!("type %USERPROFILE%{bs}.gemini{bs}oauth_creds.json") }),
        json!({ "command": "cp ~/.git-credentials /tmp/x" }),
        json!({ "changes": [{ "path": "/home/me/.aws/credentials", "kind": "update" }] }),
        json!({ "file_path": "/home/me/.claude.json.backup" }),
        json!({ "file_path": "keys/release.updater.key" }),
    ] {
        assert!(touches_credentials(&refused, None, &none), "{refused}");
    }
    for allowed in [
        json!({ "file_path": "src/main.rs" }),
        json!({ "command": "git status" }),
        json!({ "command": "echo ssh and claude are words" }),
        json!({ "file_path": "docs/my.ssh.notes.txt" }),
        json!({ "command": "ls -la" }),
    ] {
        assert!(!touches_credentials(&allowed, None, &none), "{allowed}");
    }
    // A moved folder is recognised by where it now is.
    let moved = |name: &str| (name == "CLAUDE_CONFIG_DIR").then(|| format!("E:{bs}Accounts{bs}work"));
    assert!(touches_credentials(&json!({ "file_path": "E:/Accounts/work/.credentials.json" }), None, &moved));
    assert!(!touches_credentials(&json!({ "file_path": "E:/Accounts/other/notes.txt" }), None, &moved));
}

/// R124b, item 5: a command can name a file with no folder in it; the folder it runs in (or a relative path from there) is what
/// says it is a credential. Matching a command line stays best effort: quote splicing and variables can hide a name.
#[test]
fn r124b_a_command_is_judged_with_the_folder_it_runs_in() {
    use faber_engine::permissions::touches_credentials;
    let none = |_: &str| None;
    let cat = json!({ "command": "cat auth.json" });
    assert!(touches_credentials(&cat, Some("/home/me/.codex"), &none));
    assert!(touches_credentials(&cat, Some("/home/me/.codex/sessions"), &none));
    assert!(touches_credentials(&json!({ "command": "type auth.json" }), Some("C:\\Users\\me\\.CODEX"), &none));
    assert!(!touches_credentials(&cat, Some("/home/me/project"), &none));
    assert!(!touches_credentials(&cat, None, &none));
    let moved = |name: &str| (name == "CODEX_HOME").then(|| "E:/work/cred".to_string());
    assert!(touches_credentials(&json!({ "command": "cat cred/auth.json" }), Some("E:/work"), &moved));
    assert!(touches_credentials(&json!({ "command": "cat ../cred/auth.json" }), Some("E:/work/sub"), &moved));
    assert!(touches_credentials(&json!({ "command": "cat auth.json" }), Some("E:/work/cred"), &moved));
    assert!(!touches_credentials(&json!({ "command": "cat notes.txt" }), Some("E:/work/sub"), &moved));
}

/// R124b, item 4: a sign-in folder moved to a network share is denied where it now is (and a relative value still names nothing).
#[test]
fn r124b_a_sign_in_folder_on_a_network_share_is_recognised() {
    use faber_engine::claude::tool_deny;
    use faber_engine::permissions::{relocated_credentials, touches_credentials};
    let bs = char::from(92);
    let env = |name: &str| match name {
        "CLAUDE_CONFIG_DIR" => Some(format!("{bs}{bs}fileserver{bs}home{bs}me{bs}claude")),
        "CODEX_HOME" => Some(format!("{bs}{bs}?{bs}UNC{bs}nas{bs}share{bs}codex")),
        "DOCKER_CONFIG" => Some("docker".to_string()),
        _ => None,
    };
    let moved: Vec<String> = relocated_credentials(&env).into_iter().map(|(p, _)| p).collect();
    assert_eq!(moved, ["//fileserver/home/me/claude", "//nas/share/codex"]);
    let rules = tool_deny(&env);
    for tool in ["Read", "Edit"] {
        assert!(rules.contains(&format!("{tool}(//fileserver/home/me/claude/**)")), "{rules:?}");
        assert!(rules.contains(&format!("{tool}(//nas/share/codex/**)")), "{rules:?}");
    }
    assert!(touches_credentials(&json!({ "file_path": format!("{bs}{bs}NAS{bs}share{bs}codex{bs}auth.json") }), None, &env));
}

/// R124b, item 3: a folder is judged by where it really is. Links, other spellings of the same place and the short names of the
/// system folders do not get past the refusal.
#[test]
fn r124b_a_folder_is_judged_by_its_resolved_path() {
    use faber_engine::permissions::forbidden_folder;
    let dir = tempfile::tempdir().unwrap();
    let keys = dir.path().join(".ssh").join("keys");
    std::fs::create_dir_all(&keys).unwrap();
    let plain = dir.path().join("work");
    std::fs::create_dir_all(&plain).unwrap();
    assert!(!forbidden_folder(&plain));
    let link = dir.path().join("looks-innocent");
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.path().join(".ssh"), &link).unwrap();
    #[cfg(windows)]
    junction(&link, &dir.path().join(".ssh"));
    assert!(forbidden_folder(&link), "a link into a sign-in folder");
    assert!(forbidden_folder(&link.join("keys")), "a folder below that link");
    // A folder that is absent is judged by its name.
    assert!(!forbidden_folder(&dir.path().join("not-yet")));
    assert!(forbidden_folder(&dir.path().join(".SSH").join("new")));
    #[cfg(windows)]
    {
        use std::path::Path;
        let root = std::env::var("SystemRoot").unwrap();
        let program_files = std::env::var("ProgramFiles").unwrap();
        let sys_link = dir.path().join("windir");
        junction(&sys_link, Path::new(&root));
        assert!(forbidden_folder(&sys_link), "a junction to the Windows folder");
        assert!(forbidden_folder(&sys_link.join("System32")));
        assert!(forbidden_folder(Path::new(&program_files.to_uppercase().replace('\\', "/"))), "another case and slash");
        assert!(forbidden_folder(Path::new("C:\\PROGRA~1")), "the short name of Program Files");
        assert!(forbidden_folder(Path::new(&format!("\\\\?\\{root}\\System32"))), "the verbatim form");
        assert!(forbidden_folder(Path::new("\\\\localhost\\C$\\Windows")), "the same folder through an administrative share");
        assert!(forbidden_folder(Path::new("\\\\?\\UNC\\localhost\\C$\\Windows\\System32")));
    }
}

#[cfg(windows)]
fn junction(link: &std::path::Path, target: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    let made = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert!(made.status.success(), "the junction could not be made");
}

/// R124d, item 1: the launch spelling of a folder names the folder that was checked. A folder called `safe.` loses its dot when the
/// verbatim prefix is stripped and then names `safe`, which can be a link into a refused folder.
#[cfg(windows)]
#[test]
fn r124d_a_folder_whose_plain_spelling_would_name_another_folder_keeps_its_verbatim_form() {
    use faber_engine::permissions::checked_folder;
    use std::os::windows::process::CommandExt;
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap(); // verbatim
    let refused = base.join(".ssh").join("keys");
    std::fs::create_dir_all(&refused).unwrap();
    let dotted = base.join("p").join("safe.");
    std::fs::create_dir_all(&dotted).unwrap();
    let made = std::process::Command::new("cmd.exe")
        .args(["/c", "mklink", "/J"])
        .arg(dir.path().join("p").join("safe"))
        .arg(dir.path().join(".ssh").join("keys"))
        .creation_flags(0x0800_0000)
        .output()
        .unwrap();
    assert!(made.status.success(), "the junction could not be made");
    let checked = checked_folder(&dotted).expect("the folder itself is allowed");
    assert_eq!(checked.canonicalize().unwrap(), dotted.canonicalize().unwrap(), "launching in {checked:?} would open another folder");
    assert!(checked.to_string_lossy().starts_with("\\\\?\\"), "the verbatim form is kept: {checked:?}");
    std::fs::remove_dir(&dotted).unwrap();
}

/// R124c, item 4: the CLI is started in the path that was checked (the resolved one), not in the link the person typed.
#[test]
fn r124c_a_checked_folder_is_returned_in_its_resolved_form() {
    use faber_engine::permissions::checked_folder;
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    let via = dir.path().join("via");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &via).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let made = std::process::Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&via)
            .arg(&real)
            .creation_flags(0x0800_0000)
            .output()
            .unwrap();
        assert!(made.status.success());
    }
    let verbatim = "\\\\?\\";
    let plain = |p: std::path::PathBuf| std::path::PathBuf::from(p.to_string_lossy().trim_start_matches(verbatim));
    let checked = checked_folder(&via).unwrap();
    assert!(!checked.to_string_lossy().starts_with(verbatim), "no verbatim prefix: {checked:?}");
    assert_eq!(checked, plain(real.canonicalize().unwrap()));
    // An absent folder is handed back as written.
    assert_eq!(checked_folder(&dir.path().join("later")).unwrap(), dir.path().join("later"));
}

/// R124c, item 3: a network path to an administrative share (`C$`, `ADMIN$`) is refused whatever the host is called; an alias
/// such as the PC's LAN address cannot be told from any other host by its name.
#[test]
fn r124c_a_network_path_to_an_administrative_share_is_refused_for_any_host() {
    use faber_engine::permissions::forbidden_folder;
    use std::path::Path;
    for refused in [
        "\\\\192.168.1.10\\C$\\Windows",
        "\\\\some-other-name\\d$\\work",
        "\\\\host\\ADMIN$\\System32",
        "\\\\?\\UNC\\10.0.0.7\\C$\\Users",
        "//nas/e$/stuff",
    ] {
        assert!(forbidden_folder(Path::new(refused)), "{refused}");
    }
}

/// R124, item F: the refusal covers the system folders and anything in or under a credential folder; the home folder, a drive
/// root and ordinary folders pass.
#[test]
fn r124_forbidden_folders_are_the_system_ones_and_the_credential_ones() {
    use faber_engine::permissions::forbidden_folder;
    use std::path::Path;
    for refused in ["/home/me/.ssh", "/home/me/.ssh/keys/deep", "/home/me/.CLAUDE", "/home/me/.codex", "/home/me/.gemini/antigravity-cli"] {
        assert!(forbidden_folder(Path::new(refused)), "{refused}");
    }
    for allowed in ["/home/me", "/home/me/code/app", "/tmp/work", "/"] {
        assert!(!forbidden_folder(Path::new(allowed)), "{allowed}");
    }
    #[cfg(windows)]
    {
        for refused in [
            std::env::var("SystemRoot").unwrap(),
            format!("{}\\System32", std::env::var("SystemRoot").unwrap()),
            std::env::var("ProgramFiles").unwrap(),
        ] {
            assert!(forbidden_folder(Path::new(&refused)), "{refused}");
        }
        for allowed in [format!("{}\\", std::env::var("SystemDrive").unwrap()), std::env::var("USERPROFILE").unwrap()] {
            assert!(!forbidden_folder(Path::new(&allowed)), "{allowed}");
        }
    }
}
