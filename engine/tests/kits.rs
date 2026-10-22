//! Session kits cross the real driver boundaries through fake CLI recordings, without spawning a process.
use crate::support::{fake, fixture};
use faber_engine::claude::{ClaudeFlags, ClaudeLaunch, ClaudeOptions, start_claude};
use faber_engine::codex::{CodexServer, CodexServerOptions, CodexThread};
use faber_engine::events::{EngineEvent, UserTurn};
use faber_engine::model::{Area, Permissions, Vendor};
use faber_engine::tools::{Delivery, Item, Server, Store, digest};
use serde_json::Value;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const TASK: &str = "Debug this failing test.";
const DEBUG: &str = "---\nname: debug\ndescription: Debug failures.\n---\nDEBUG_KIT_INSTRUCTIONS";

fn skill(id: &str, job: &str, body: &str) -> Item {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("SKILL.md"), body).unwrap();
    Item {
        id: id.into(),
        job: job.into(),
        license: "MIT".into(),
        repo: "fixture/skills".into(),
        commit: "1234".into(),
        path: format!("skills/{id}"),
        digest: digest(dir.path()).unwrap(),
        server: None,
        chat_safe: false,
        cli_name: None,
        tier: Default::default(),
    }
}

fn install_skill(store: &Store, item: &Item, body: &str, root: &Path) {
    let archive = root.join(format!("{}.zip", item.id));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    zip.start_file(format!("repo-1234/{}/SKILL.md", item.path), zip::write::SimpleFileOptions::default()).unwrap();
    zip.write_all(body.as_bytes()).unwrap();
    zip.finish().unwrap();
    store.install_from(item, &archive).unwrap();
}

fn skills(root: &Path) -> Store {
    let debug = skill("debug", "debugging", DEBUG);
    let design = skill("design", "design", "---\nname: design\n---\nDESIGN_KIT_INSTRUCTIONS");
    let missing = skill("not-installed", "debugging", "Not installed.");
    let store = Store::with_catalog(&root.join("data"), vec![debug.clone(), design.clone(), missing]);
    install_skill(&store, &debug, DEBUG, root);
    install_skill(&store, &design, "---\nname: design\n---\nDESIGN_KIT_INSTRUCTIONS", root);
    store
}

async fn replay(
    vendor: Vendor,
    area: Area,
    permissions: Permissions,
    tools: Delivery,
    cwd: &Path,
    internal: bool,
) -> (Vec<String>, Vec<Value>) {
    let fake = fake(fixture(vendor.name(), "kits"));
    let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let sink = Arc::new(move |event| drop(tx.send(event)));
    let server;
    let session = match vendor {
        Vendor::Claude => {
            server = None;
            start_claude(
                ClaudeOptions {
                    bin: "fake".into(),
                    cwd: cwd.into(),
                    spawn: fake.spawn.clone(),
                    sandbox: None,
                    launch: ClaudeLaunch {
                        area,
                        permissions,
                        model: None,
                        effort: None,
                        resume: None,
                        instructions: internal.then(|| "Faber internal request".into()),
                        user_hooks: false,
                        research: None,
                        user_name: None,
                        group_instructions: None,
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        verify: false,
                        progress: false,
                        flags: ClaudeFlags { plugin_dir: true, ..Default::default() },
                        tuning: Default::default(),
                        tools,
                    },
                },
                sink,
            )
        }
        Vendor::Codex => {
            let app = CodexServer::start(CodexServerOptions {
                permissions,
                cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
                cwd: cwd.into(),
                area,
                user_hooks: false,
                windows: false,
                spawn: fake.spawn.clone(),
                tools: Default::default(),
            });
            let session = app.open_with_tools(
                CodexThread {
                    cwd: cwd.into(),
                    permissions,
                    model: None,
                    effort: None,
                    resume: None,
                    instructions: None,
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    leave_plan: false,
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    progress: false,
                    tuning: Default::default(),
                },
                sink,
                tools,
            );
            server = Some(app);
            session
        }
    };
    session.send(UserTurn::text(TASK));
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match events.recv().await.expect("fake session ended before completing") {
                EngineEvent::Done { ok, .. } => {
                    assert!(ok);
                    break;
                }
                EngineEvent::Error { message, .. } => panic!("fake session failed: {message:?}"),
                _ => {}
            }
        }
    })
    .await
    .expect("fake session did not complete");
    let argv = fake.args.lock().unwrap()[0].clone();
    let messages = fake.wire.messages();
    assert!(fake.mismatches.lock().unwrap().is_empty(), "{:?}", fake.mismatches.lock().unwrap());
    session.close();
    if let Some(server) = server {
        server.retire();
    }
    (argv, messages)
}

#[tokio::test]
async fn kits_reach_both_cli_sessions_in_chat_and_projects_with_isolated_modes_still_empty() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        for area in [Area::Chat, Area::Project] {
            for permissions in [Permissions::FullAccess, Permissions::Ask, Permissions::Isolated] {
                let dir = tempfile::tempdir().unwrap();
                let store = skills(dir.path());
                let (tools, report) = store.kit_for(vendor, TASK, Some(dir.path()), area, permissions, &[]);
                let enabled = permissions != Permissions::Isolated;
                assert_eq!(report.ids.iter().any(|id| id == "debug"), enabled);
                assert!(!report.ids.iter().any(|id| id == "design"), "report names only the task selection");
                assert!(!report.ids.iter().any(|id| id == "not-installed"));
                let plugin = tools.plugin.clone();
                let tools_root = tools.tools_root();
                // Core verifies once before invoking the pure driver argument builder.
                let tools = tools.verified_for_launch(vendor).0;
                let (args, messages) = replay(vendor, area, permissions, tools, dir.path(), false).await;
                if vendor == Vendor::Claude {
                    let loaded = args.windows(2).find(|args| args[0] == "--plugin-dir").map(|args| args[1].clone());
                    assert_eq!(loaded, plugin.filter(|_| area == Area::Project).map(|path| path.to_string_lossy().into_owned()));
                    let instructions =
                        args.windows(2).find(|args| args[0] == "--append-system-prompt").map(|args| args[1].as_str()).unwrap_or_default();
                    assert!(!instructions.contains("faber:debug;") && !instructions.contains("faber:design;"));
                    assert!(
                        !instructions.contains("DEBUG_KIT_INSTRUCTIONS") && !instructions.contains("DESIGN_KIT_INSTRUCTIONS"),
                        "catalog context belongs to the current user turn, never a resumable system-prompt snapshot"
                    );
                    assert!(!args.iter().any(|arg| arg == "--add-dir"), "catalog instructions never grant working-directory write scope");
                    if area == Area::Chat {
                        let native = args.windows(2).find(|args| args[0] == "--tools").unwrap()[1].split(',').collect::<Vec<_>>();
                        assert!(!native.contains(&"Skill"), "personal and plugin skills never become callable in Chat");
                    }
                    if enabled && let Some(root) = tools_root {
                        let settings: Value =
                            serde_json::from_str(&args.windows(2).find(|args| args[0] == "--settings").unwrap()[1]).unwrap();
                        let rule = faber_engine::permissions::claude_rule_path(root.to_string_lossy().trim_start_matches(r"\\?\"));
                        assert!(settings["permissions"].get("allow").is_none(), "restricted Chat needs no external reads");
                        for tool in ["Edit", "Write"] {
                            assert!(
                                settings["permissions"]["deny"].as_array().unwrap().contains(&Value::String(format!("{tool}({rule}/**)")))
                            );
                        }
                    }
                } else {
                    let thread = messages.iter().find(|m| m["method"] == "thread/start").unwrap();
                    let instructions = thread["params"]["developerInstructions"].as_str().unwrap_or_default();
                    assert_eq!(instructions.contains("DEBUG_KIT_INSTRUCTIONS"), enabled, "{area:?} {permissions:?}: {thread}");
                    assert!(!instructions.contains("DESIGN_KIT_INSTRUCTIONS"));
                    assert!(!messages.iter().any(|m| m["method"] == "skills/extraRoots/set"), "a session kit never becomes process-wide");
                }
                assert!(!dir.path().join("SKILL.md").exists(), "skills never enter the conversation's files");
            }
        }
    }
}

#[tokio::test]
async fn the_launch_boundary_rechecks_skill_names_and_bodies_without_dropping_healthy_items() {
    for mutation in ["new-skill", "changed-body", "changed-manifest", "new-hooks"] {
        for area in [Area::Chat, Area::Project] {
            for permissions in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Plan, Permissions::Auto, Permissions::FullAccess] {
                let dir = tempfile::tempdir().unwrap();
                let store = skills(dir.path());
                let (tools, _) = store.kit_for(Vendor::Claude, TASK, None, area, permissions, &[]);
                let root = tools.skills.as_ref().unwrap();
                match mutation {
                    "changed-body" => {
                        std::fs::write(root.join("debug/SKILL.md"), "---\nname: debug\nallowed-tools: Bash\n---\nCHANGED_INSTRUCTIONS")
                            .unwrap();
                    }
                    "new-skill" => {
                        std::fs::create_dir(root.join("planted")).unwrap();
                        std::fs::write(root.join("planted/SKILL.md"), "---\nname: planted\nallowed-tools: Bash\n---\nPLANTED_INSTRUCTIONS")
                            .unwrap();
                    }
                    "changed-manifest" => {
                        std::fs::write(
                            tools.plugin.as_ref().unwrap().join(".claude-plugin/plugin.json"),
                            r#"{"name":"faber","hooks":{"SessionStart":[]}}"#,
                        )
                        .unwrap();
                    }
                    "new-hooks" => {
                        let hooks = tools.plugin.as_ref().unwrap().join("hooks");
                        std::fs::create_dir(&hooks).unwrap();
                        std::fs::write(hooks.join("hooks.json"), r#"{"hooks":{"SessionStart":[]}}"#).unwrap();
                    }
                    _ => unreachable!(),
                }
                // Mutation happened after selection: the driver must check immediately before its own launch.
                let checked = tools.verified_for_launch(Vendor::Claude).0;
                let checked_index = checked.skill_descriptions().unwrap_or_default();
                assert!(checked_index.contains("faber:design;"), "a healthy item survives: {checked_index}");
                assert_eq!(checked_index.contains("faber:debug;"), mutation != "changed-body");
                assert!(!checked_index.contains("planted") && !checked_index.contains("CHANGED_INSTRUCTIONS"));
                let (args, _) = replay(Vendor::Claude, area, permissions, checked, dir.path(), false).await;
                assert!(!args.iter().any(|arg| arg == "--plugin-dir" || arg == "--add-dir"), "{area:?} {permissions:?}: {args:?}");
                let index = &args.windows(2).find(|arg| arg[0] == "--append-system-prompt").unwrap()[1];
                assert!(!index.contains("faber:") && !index.contains("CHANGED_INSTRUCTIONS"));
                let settings: Value = serde_json::from_str(&args.windows(2).find(|arg| arg[0] == "--settings").unwrap()[1]).unwrap();
                assert!(settings["permissions"].get("allow").is_none());
                if area == Area::Chat {
                    assert!(!args.windows(2).find(|arg| arg[0] == "--tools").unwrap()[1].split(',').any(|tool| tool == "Skill"));
                }
            }
        }
    }
}

#[tokio::test]
async fn internal_claude_requests_never_load_catalog_descriptions_or_extra_read_roots() {
    let dir = tempfile::tempdir().unwrap();
    let store = skills(dir.path());
    let (kit, _) = store.kit_for(Vendor::Claude, TASK, None, Area::Chat, Permissions::Ask, &[]);
    assert!(kit.skill_descriptions().is_some());
    let (args, _) = replay(Vendor::Claude, Area::Chat, Permissions::Ask, kit, dir.path(), true).await;
    assert!(!args.iter().any(|arg| arg == "--add-dir" || arg == "--plugin-dir" || arg.contains("faber:debug;")));
    assert_eq!(args.windows(2).find(|args| args[0] == "--tools").unwrap()[1], "");
}

#[test]
fn a_helper_task_gets_its_own_kit_and_disabled_or_removed_items_never_enter_a_fresh_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = skills(dir.path());
    let (caller, caller_report) = store.kit("Design a new layout", None, Area::Project, Permissions::FullAccess, &[]);
    let (helper, helper_report) = store.kit(TASK, None, Area::Project, Permissions::FullAccess, &[]);
    assert_eq!(caller_report.ids, ["design"]);
    assert_eq!(helper_report.ids, ["debug"]);
    assert_eq!(caller.plugin, helper.plugin, "task picks share the verified catalog snapshot");
    assert!(helper.skill_instructions().unwrap().contains("DEBUG_KIT_INSTRUCTIONS"));
    assert!(!helper.skill_instructions().unwrap().contains("DESIGN_KIT_INSTRUCTIONS"));
    let path = helper.skills.as_ref().unwrap().join("debug/SKILL.md");
    store.set_enabled("debug", false).unwrap();
    let (next, report) = store.kit(TASK, None, Area::Chat, Permissions::FullAccess, &[]);
    assert!(!report.ids.iter().any(|id| id == "debug") && next.plugin.is_none());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), DEBUG, "a switch affects the next session, not this snapshot");
    store.remove("debug").unwrap();
    assert!(store.kit(TASK, None, Area::Project, Permissions::FullAccess, &[]).1.ids.is_empty());
    assert!(path.is_file(), "an active kit lives through removal until the session ends");
    drop(helper);
    drop(caller);
    store.sweep_kits();
    assert!(!path.exists(), "ending the last session releases its copy");
}

fn install_server(store: &Store, item: &Item, root: &Path) {
    let archive = root.join(format!("{}.tgz", item.id));
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(std::fs::File::create(&archive).unwrap(), flate2::Compression::fast()));
    let body = b"fixture server";
    let mut header = tar::Header::new_gnu();
    header.set_size(body.len() as u64);
    header.set_mode(0o644);
    tar.append_data(&mut header, "package/run.js", &body[..]).unwrap();
    tar.into_inner().unwrap().finish().unwrap();
    store.install_from(item, &archive).unwrap();
}

#[test]
fn chat_servers_require_an_explicit_review_and_never_cross_guarded_modes_or_project_switches() {
    let dir = tempfile::tempdir().unwrap();
    let content = tempfile::tempdir().unwrap();
    std::fs::write(content.path().join("run.js"), b"fixture server").unwrap();
    let server = |id: &str, chat_safe| Item {
        id: id.into(),
        job: "measure".into(),
        license: "MIT".into(),
        repo: "fixture/server".into(),
        commit: "1234".into(),
        path: String::new(),
        digest: digest(content.path()).unwrap(),
        server: Some(Server { package: id.into(), version: "1.0.0".into(), entry: "run.js".into(), args: vec![], env: Default::default() }),
        chat_safe,
        cli_name: None,
        tier: Default::default(),
    };
    let items = vec![server("reviewed", true), server("not-reviewed", false)];
    let store = Store::with_catalog(&dir.path().join("data"), items.clone()).with_node("node".into());
    for item in &items {
        install_server(&store, item, dir.path());
    }
    let task = "Benchmark browser performance with devtools";
    for area in [Area::Chat, Area::Project] {
        for permissions in [Permissions::FullAccess, Permissions::Ask, Permissions::Auto, Permissions::Isolated] {
            let (kit, _) = store.kit(task, None, area, permissions, &[]);
            let allowed = permissions == Permissions::FullAccess;
            assert_eq!(kit.servers.iter().any(|server| server.name == "faber-reviewed"), allowed);
            assert_eq!(kit.servers.iter().any(|server| server.name == "faber-not-reviewed"), allowed && area == Area::Project);
        }
    }
    let (kit, _) = store.kit(task, None, Area::Project, Permissions::FullAccess, &["faber-reviewed".into(), "faber-not-reviewed".into()]);
    assert!(kit.servers.is_empty(), "a project's switched-off server never reaches the CLI");
    let (first, _) = store.kit(task, None, Area::Project, Permissions::FullAccess, &[]);
    let (second, _) = store.kit(task, None, Area::Project, Permissions::FullAccess, &[]);
    let script = first.servers[0].args[0].clone();
    assert_eq!(second.servers[0].args[0], script);
    drop(first);
    assert!(Path::new(&script).is_file(), "another native session still owns the shared server package");
    drop(second);
    assert!(Path::new(&script).exists(), "current catalog snapshots survive idle session closure");
    let (resumed, _) = store.kit(task, None, Area::Project, Permissions::FullAccess, &[]);
    assert_eq!(resumed.servers[0].args[0], script, "a cached native server definition finds the recreated script at the same path");
    assert!(Path::new(&script).is_file());
}

/// Own work method, slice 1: a project's start card reaches Claude Code's appended system prompt and a new Codex thread's
/// developer instructions, after the persona.
#[tokio::test]
async fn the_start_card_reaches_both_cli_sessions_after_the_persona() {
    let card = "```faber-start-card\nSTART_CARD_MARK\n```";
    for vendor in [Vendor::Claude, Vendor::Codex] {
        let dir = tempfile::tempdir().unwrap();
        let tools = Delivery { card: Some(card.into()), ..Default::default() };
        let (args, messages) = replay(vendor, Area::Project, Permissions::FullAccess, tools, dir.path(), false).await;
        let instructions = if vendor == Vendor::Claude {
            args.windows(2).find(|args| args[0] == "--append-system-prompt").map(|args| args[1].clone()).unwrap_or_default()
        } else {
            let thread = messages.iter().find(|m| m["method"] == "thread/start").unwrap();
            thread["params"]["developerInstructions"].as_str().unwrap_or_default().to_string()
        };
        let at = instructions.find(card).unwrap_or_else(|| panic!("{vendor:?}: {instructions}"));
        assert!(at > instructions.find(faber_engine::persona::PROJECT_DETAILS.trim()).unwrap(), "{vendor:?}: after the persona");
    }
}
