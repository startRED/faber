use faber_engine::claude::{ClaudeLaunch, claude_args};
use faber_engine::model::{Area, Permissions};
use serde_json::{Value, json};

fn launch(area: Area, permissions: Permissions) -> ClaudeLaunch {
    ClaudeLaunch {
        temporary: false,
        helper: Default::default(),
        browse: false,
        conversations: false,
        leads: false,
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

// Hold through fixture cleanup: even an unrelated damaged Store must refuse every command.
static STORE_FIXTURES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[test]
fn guarded_commands_cannot_skip_the_driver_with_a_remembered_cli_grant() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    for area in [Area::Chat, Area::Project] {
        for mode in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Auto, Permissions::Plan] {
            let mut options = launch(area, mode);
            options.user_hooks = true;
            let args = claude_args(&options);
            let config = settings(&args);
            let ask = config["permissions"]["ask"].as_array().expect("commands must always reach the host");
            for tool in ["Bash", "PowerShell"] {
                assert!(ask.contains(&json!(tool)), "{area:?} {mode:?} {tool}");
            }
            assert_eq!(config["sandbox"]["autoAllowBashIfSandboxed"], false);
            assert_eq!(config["disableAllHooks"], true, "sandboxed scripts must not plant a later host hook");
            assert!(args.contains(&"--strict-mcp-config".into()));
            assert_eq!(args[args.iter().position(|arg| arg == "--setting-sources").expect("no project startup settings") + 1], "user");
            assert!(config["permissions"]["deny"].as_array().unwrap().contains(&json!("Monitor")));
            assert_eq!(
                args[args.iter().position(|a| a == "--permission-mode").unwrap() + 1],
                faber_engine::claude::permission_mode(mode.for_claude(Default::default()))
            );
        }
        let config = settings(&claude_args(&launch(area, Permissions::FullAccess)));
        assert!(config["permissions"].get("ask").is_none());
        assert!(config.get("sandbox").is_none());
    }
}

#[test]
fn guarded_commands_protect_all_live_stores_and_refuse_a_damaged_peer() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let peer = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(data.path());
    let peer_store = faber_engine::store::Store::new(peer.path());
    let native = faber_engine::cli::Cli { command: project.path().join("node.exe").to_str().unwrap().into(), prefix: vec![] };
    let command = || {
        faber_engine::permissions::guarded_command_argv(
            project.path(),
            Permissions::Ask,
            "Bash",
            &json!({"command":"node task.js"}),
            Some(&native),
            &|_| Some(native.clone()),
        )
    };
    let argv = command().unwrap();
    let config: toml::Value = toml::from_str(
        &argv[1..argv.iter().position(|arg| arg == "sandbox").unwrap()]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| pair[1].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let id = config["default_permissions"].as_str().unwrap();
    for root in [data.path(), peer.path()] {
        let path = root.canonicalize().unwrap().to_string_lossy().replace("\\\\?\\", "").replace('\\', "/");
        assert_eq!(config["permissions"][id]["filesystem"][&path].as_str(), Some("deny"));
    }
    let record = peer.path().join("settings.json");
    std::fs::write(&record, "original").unwrap();
    let alias = project.path().join("alias.json");
    std::fs::hard_link(&record, alias).unwrap();
    assert!(command().is_none(), "a damaged peer Store must also fail closed");
    drop(peer_store);
    assert!(command().is_some(), "closed Stores leave the registry");
}

#[test]
fn guarded_commands_run_literal_project_programs_without_a_bash_runtime() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    use faber_engine::cli::Cli;
    let project = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(&project.path().join("data"));
    let native = Cli { command: project.path().join("node.exe").to_str().unwrap().into(), prefix: vec![] };
    let input = json!({"command":"node project-task.js"});
    let argv = faber_engine::permissions::guarded_command_argv(project.path(), Permissions::Ask, "Bash", &input, Some(&native), &|name| {
        (name == "node").then(|| native.clone())
    })
    .expect("literal native project commands do not require Git Bash to start inside MXC");
    assert_eq!(&argv[argv.iter().position(|arg| arg == "--").unwrap() + 1..], &[native.command, "project-task.js".into()]);
}

#[test]
fn guarded_commands_protect_the_ancestor_of_a_nested_store_from_rename() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    use faber_engine::cli::Cli;
    let project = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(&project.path().join("holder/data"));
    let native = Cli { command: project.path().join("node.exe").to_str().unwrap().into(), prefix: vec![] };
    for mode in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Auto, Permissions::Plan] {
        let argv = faber_engine::permissions::guarded_command_argv(
            project.path(),
            mode,
            "Bash",
            &json!({"command":"node task.js"}),
            Some(&native),
            &|_| Some(native.clone()),
        )
        .unwrap();
        let config: toml::Value = toml::from_str(
            &argv[1..argv.iter().position(|arg| arg == "sandbox").unwrap()]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| pair[1].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let id = config["default_permissions"].as_str().unwrap();
        assert_eq!(config["permissions"][id]["filesystem"][":workspace_roots"]["holder"].as_str(), Some("deny"));
    }
}

#[test]
fn guarded_commands_refuse_preexisting_hardlinks_to_either_approval_record() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    use faber_engine::cli::Cli;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(data.path());
    std::fs::create_dir(data.path().join("faber-trust")).unwrap();
    let native = Cli { command: project.path().join("node.exe").to_str().unwrap().into(), prefix: vec![] };
    for record in ["decisions-kept.json", "faber-trust/learning-kept.json", "settings.json", "conversation.json"] {
        let target = data.path().join(record);
        std::fs::write(&target, "original").unwrap();
        let alias = project.path().join("alias.json");
        std::fs::hard_link(&target, &alias).unwrap();
        for mode in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Auto, Permissions::Plan] {
            assert!(
                faber_engine::permissions::guarded_command_argv(
                    project.path(),
                    mode,
                    "Bash",
                    &json!({"command":"node task.js"}),
                    Some(&native),
                    &|_| Some(native.clone()),
                )
                .is_none(),
                "a hardlinked {record} must refuse commands before the sandbox starts"
            );
        }
        std::fs::remove_file(alias).unwrap();
        assert!(
            faber_engine::permissions::guarded_command_argv(
                project.path(),
                Permissions::Ask,
                "Bash",
                &json!({"command":"node task.js"}),
                Some(&native),
                &|_| Some(native.clone()),
            )
            .is_some(),
            "ordinary single-link records must keep working"
        );
    }
}

#[test]
fn guarded_commands_refuse_an_incomplete_store_scan() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    use faber_engine::cli::Cli;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(data.path());
    for index in 0..10_000 {
        std::fs::write(data.path().join(index.to_string()), "").unwrap();
    }
    let native = Cli { command: project.path().join("node.exe").to_str().unwrap().into(), prefix: vec![] };
    assert!(
        faber_engine::permissions::guarded_command_argv(
            project.path(),
            Permissions::Ask,
            "Bash",
            &json!({"command":"node task.js"}),
            Some(&native),
            &|_| Some(native.clone()),
        )
        .is_none(),
        "the Store also has a conversations directory, exceeding the entry budget"
    );
}

#[test]
fn guarded_commands_deny_the_exact_store_even_inside_the_workspace_and_preserve_script_bytes() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    use base64::Engine as _;
    use faber_engine::cli::Cli;
    use faber_engine::permissions::guarded_command_argv;
    let project = tempfile::tempdir().unwrap();
    let data = project.path().join("data folder");
    let _store = faber_engine::store::Store::new(&data);
    let nested = data.join("project");
    std::fs::create_dir(&nested).unwrap();
    let native = Cli { command: project.path().join("fake-native.exe").to_str().unwrap().into(), prefix: vec![] };
    std::fs::write(&native.command, "fake shell, never executed").unwrap();
    let resolve = |_: &str| Some(native.clone());
    let data_text = data.canonicalize().unwrap().to_string_lossy().replace("\\\\?\\", "").replace('\\', "/");
    for mode in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Auto, Permissions::Plan] {
        for cwd in [project.path(), nested.as_path()] {
            for tool in ["Bash", "PowerShell"] {
                let script = "node -e 'require(\"node:fs\").writeFileSync(\"ok.txt\",\"café\")'";
                let input = json!({"command":script});
                let checked = guarded_command_argv(cwd, mode, tool, &input, Some(&native), &resolve);
                if cwd == nested {
                    assert!(checked.is_none(), "a nested working folder must not reopen any Store subtree");
                    continue;
                }
                let argv = checked.unwrap();
                let config: toml::Value = toml::from_str(
                    &argv[1..argv.iter().position(|arg| arg == "sandbox").unwrap()]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|pair| pair[1].as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
                .unwrap();
                let id = config["default_permissions"].as_str().unwrap();
                let profile = &config["permissions"][id];
                assert_eq!(profile["filesystem"][&data_text].as_str(), Some("deny"));
                assert_eq!(profile["network"]["enabled"].as_bool(), Some(true));
                let fs = &profile["filesystem"][":workspace_roots"];
                for metadata in [".claude", ".codex", ".mcp.json", ".git", ".faber/verify.local.json"] {
                    assert_eq!(fs[metadata].as_str(), Some("read"), "commands must not plant a future unsandboxed startup: {metadata}");
                }
                assert_eq!(
                    fs["."].as_str(),
                    Some(if cwd == nested {
                        "deny"
                    } else if mode == Permissions::Plan {
                        "read"
                    } else {
                        "write"
                    })
                );
                if cwd == project.path() {
                    assert_eq!(fs["data folder"].as_str(), Some("deny"));
                }
                let last = argv.last().unwrap();
                if tool == "PowerShell" {
                    assert!(argv.contains(&"-EncodedCommand".into()));
                    let bytes = base64::engine::general_purpose::STANDARD.decode(last).unwrap();
                    let units: Vec<_> = bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
                    assert_eq!(String::from_utf16(&units).unwrap(), script);
                    if cfg!(windows) {
                        assert!(argv.windows(2).any(|pair| pair == ["-WindowStyle", "Hidden"]));
                        assert_eq!(config["windows"]["sandbox"].as_str(), Some("mxc"));
                    }
                } else {
                    assert_eq!(last, script, "arbitrary project shell syntax stays inside the sandbox");
                }
            }
        }
        let input = json!({"command":"node project-task.js"});
        assert!(guarded_command_argv(project.path(), mode, "Bash", &input, None, &resolve).is_none());
        assert!(guarded_command_argv(project.path(), mode, "Bash", &input, Some(&native), &|_| None).is_none());
        let prefixed = Cli { prefix: vec!["launcher.js".into()], ..native.clone() };
        assert!(guarded_command_argv(project.path(), mode, "Bash", &input, Some(&prefixed), &resolve).is_none());
        assert!(
            guarded_command_argv(project.path(), mode, "Bash", &json!({"command":"echo hi; pwd"}), Some(&native), &|_| Some(
                prefixed.clone()
            ))
            .is_none()
        );
        let missing = project.path().join("missing");
        assert!(guarded_command_argv(&missing, mode, "Bash", &input, Some(&native), &resolve).is_none());
        if cfg!(windows) {
            let wsl = Cli { command: "C:/Windows/System32/bash.exe".into(), prefix: vec![] };
            assert!(
                guarded_command_argv(project.path(), mode, "Bash", &json!({"command":"echo hi; pwd"}), Some(&native), &|_| Some(
                    wsl.clone()
                ))
                .is_none()
            );
        }
    }
}

#[cfg(windows)]
#[test]
fn guarded_commands_refuse_a_linked_approval_directory_even_when_its_target_is_outside_the_store() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    use faber_engine::cli::Cli;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(data.path());
    std::fs::write(outside.path().join("learning-kept.json"), "original").unwrap();
    junction::create(outside.path(), data.path().join("faber-trust")).unwrap();
    let native = Cli { command: project.path().join("node.exe").to_str().unwrap().into(), prefix: vec![] };
    assert!(
        faber_engine::permissions::guarded_command_argv(
            project.path(),
            Permissions::Ask,
            "Bash",
            &json!({"command":"node task.js"}),
            Some(&native),
            &|_| Some(native.clone()),
        )
        .is_none()
    );
}

#[cfg(windows)]
#[test]
fn guarded_commands_refuse_a_wsl_shell_hidden_by_a_junction() {
    let _fixture = STORE_FIXTURES.blocking_lock();
    use faber_engine::cli::Cli;
    use faber_engine::permissions::guarded_command_argv;
    let project = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(&project.path().join("data"));
    let system = project.path().join("Windows/System32");
    std::fs::create_dir_all(&system).unwrap();
    std::fs::write(system.join("bash.exe"), "fake WSL launcher, never executed").unwrap();
    let alias = project.path().join("shell alias");
    junction::create(&system, &alias).unwrap();
    let cli = Cli { command: alias.join("bash.exe").to_str().unwrap().into(), prefix: vec![] };
    assert!(
        guarded_command_argv(project.path(), Permissions::Ask, "Bash", &json!({"command":"pwd"}), Some(&cli), &|_| Some(cli.clone()))
            .is_none()
    );
}

#[tokio::test]
async fn guarded_commands_wrap_even_remembered_approvals_and_fail_closed_without_a_runner() {
    let _fixture = STORE_FIXTURES.lock().await;
    use faber_engine::events::{Delivery, EngineEvent, UserTurn};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let _store = faber_engine::store::Store::new(data.path());
    let data_text = data.path().canonicalize().unwrap().to_string_lossy().replace("\\\\?\\", "").replace('\\', "/");
    tokio::time::timeout(crate::support::DEADLINE, async {
        for mode in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Auto, Permissions::Plan] {
            for area in [Area::Chat, Area::Project] {
                for runner in [true, false] {
                    let (driver, script) = tokio::io::duplex(65536);
                    let (stdout, stdin) = tokio::io::split(driver);
                    let process = Arc::new(Mutex::new(Some(faber_engine::proc::Proc {
                        stdin: Box::new(stdin),
                        stdout: Box::new(stdout),
                        stderr: None,
                        child: None,
                        contained: None,
                    })));
                    let spawn: faber_engine::proc::Spawner = Arc::new(move |_, _, _| Ok(process.lock().unwrap().take().unwrap()));
                    let (read, mut write) = tokio::io::split(script);
                    let mut output = BufReader::new(read).lines();
                    let (sent, mut events) = tokio::sync::mpsc::unbounded_channel();
                    let session = faber_engine::claude::start_claude(
                        faber_engine::claude::ClaudeOptions {
                            bin: "unused".into(),
                            cwd: project.path().into(),
                            launch: launch(area, mode),
                            spawn,
                            sandbox: runner.then(|| faber_engine::cli::Cli {
                                command: project.path().join("fake-sandbox.exe").to_str().unwrap().into(),
                                prefix: vec![],
                            }),
                        },
                        Arc::new(move |event| drop(sent.send(event))),
                    );
                    session.send(UserTurn::text("work"));
                    assert_eq!(serde_json::from_str::<Value>(&output.next_line().await.unwrap().unwrap()).unwrap()["type"], "user");
                    let monitor = json!({"type":"control_request","request_id":"monitor","request":{
                        "subtype":"can_use_tool","tool_name":"Monitor","input":{"command":"node project-task.js"}}});
                    write.write_all(format!("{monitor}\n").as_bytes()).await.unwrap();
                    let refused: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
                    assert_eq!(refused["response"]["response"]["behavior"], "deny");
                    for tool in ["Bash", "PowerShell"] {
                        // Neither the real folder nor either approval record is in the request text.
                        // A project script can compute both names and follow the junction at execution time.
                        let input = json!({"command":"node project-task.js", "run_in_background":true});
                        for allow in [false, true, true] {
                            let request = json!({"type":"control_request","request_id":"cmd","request":{
                                "subtype":"can_use_tool","tool_name":tool,"input":input}});
                            write.write_all(format!("{request}\n").as_bytes()).await.unwrap();
                            loop {
                                if let EngineEvent::Permission { request_id, input: original, .. } = events.recv().await.unwrap() {
                                    assert_eq!(original, input, "the original command remains the approval/grant key");
                                    // The engine sends this same answer when a stored grant matches.
                                    assert_eq!(session.answer(&request_id, allow).await.unwrap(), Delivery::Delivered);
                                    break;
                                }
                            }
                            let msg: Value = serde_json::from_str(&output.next_line().await.unwrap().unwrap()).unwrap();
                            let response = &msg["response"]["response"];
                            let shell_present = if tool == "Bash" {
                                faber_engine::cli::find_cli("node").is_some() || faber_engine::cli::find_cli("bash").is_some()
                            } else {
                                faber_engine::cli::find_cli("pwsh").or_else(|| faber_engine::cli::find_cli("powershell")).is_some()
                            };
                            if allow && runner && shell_present {
                                assert_eq!(response["behavior"], "allow");
                                let updated = &response["updatedInput"];
                                let command = updated["command"].as_str().unwrap();
                                assert_ne!(updated, &input, "{mode:?} {tool}: an approval must never send the original command");
                                assert!(command.contains("sandbox") && command.contains("--permission-profile"), "{command}");
                                assert!(command.contains(&data_text) && command.contains("deny"), "{command}");
                                assert_eq!(updated["run_in_background"], false);
                                // A failed native start is a tool error. A later remembered approval still wraps anew.
                                let failure = json!({"type":"user","message":{"role":"user","content":[{
                                    "type":"tool_result","tool_use_id":"cmd","is_error":true,"content":"sandbox failed to start"}]}});
                                write.write_all(format!("{failure}\n").as_bytes()).await.unwrap();
                            } else {
                                assert_eq!(response["behavior"], "deny", "{mode:?} {tool}");
                                assert!(response.get("updatedInput").is_none());
                            }
                        }
                    }
                    session.close();
                    drop(write);
                }
            }
        }
    })
    .await
    .expect("guarded command answers must not hang");
}
