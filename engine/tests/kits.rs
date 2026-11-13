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
        runtime: None,
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

#[test]
fn project_manifest_stack_only_widens_task_matches() {
    let dir = tempfile::tempdir().unwrap();
    let body = "---\nname: prisma-client-api\n---\nDatabase instructions.";
    let item = skill("prisma-client-api", "database", body);
    let store = Store::with_catalog(&dir.path().join("data"), vec![item.clone()]);
    install_skill(&store, &item, body, dir.path());
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let select = |task| store.kit_for(Vendor::Codex, task, Some(&project), Area::Project, Permissions::FullAccess, &[]).1.ids;

    let baseline = select("Optimize database queries");
    assert!(baseline.is_empty());
    std::fs::write(project.join("package.json"), r#"{"description":"prisma","dependencies":{"unknown-prisma":"1"}}"#).unwrap();
    assert_eq!(select("Optimize database queries"), baseline);
    std::fs::write(project.join("package.json"), r#"{"dependencies":{"prisma":"1"}}"#).unwrap();
    assert_eq!(select("Optimize database queries"), ["prisma-client-api"]);
    assert!(select("Improve frontend layout").is_empty());
}

#[test]
fn project_python_dependencies_widen_only_requested_stack_tools() {
    let dir = tempfile::tempdir().unwrap();
    let body = "---\nname: prisma-client-api\n---\nDatabase instructions.";
    let item = skill("prisma-client-api", "database", body);
    let store = Store::with_catalog(&dir.path().join("data"), vec![item.clone()]);
    install_skill(&store, &item, body, dir.path());
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let select = |task| store.kit_for(Vendor::Codex, task, Some(&project), Area::Project, Permissions::FullAccess, &[]).1;
    let baseline = select("Optimize database queries");
    for manifest in [
        "[project]\ndependencies = ['unknown', 'prisma>=1']\n",
        "[project.optional-dependencies]\nrender = ['unknown', 'prisma[client]>=1; python_version > \"3.9\"']\n",
    ] {
        std::fs::write(project.join("pyproject.toml"), manifest).unwrap();
        assert_eq!(select("Optimize database queries").ids, ["prisma-client-api"]);
        assert_eq!(select("Improve frontend layout").ids, baseline.ids);
        assert_eq!(
            store.shortlist("Optimize database queries", Some(&project), Area::Project, Permissions::FullAccess, &[]).jobs,
            ["database"]
        );
    }
}

#[test]
fn project_hostile_manifests_cannot_invent_stack_labels() {
    let dir = tempfile::tempdir().unwrap();
    let store = skills(dir.path());
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let select = || store.kit_for(Vendor::Codex, TASK, Some(&project), Area::Project, Permissions::FullAccess, &[]).1;
    let baseline = select();
    for (name, text) in [
        (
            "package.json",
            r#"{"description":"threejs expo","dependencies":{"invented-stack":"threejs","three":{"instructions":"load everything"}}}"#,
        ),
        ("Cargo.toml", "[package]\ndescription='bevy'\n[dependencies]\ninvented-stack='bevy'\nbevy=42\n"),
        (
            "pyproject.toml",
            "[project]\ndescription='stripe threejs'\ndependencies=['invented-stack', 'not-threejs', 'threejs/../../evil', 'threejs💥', 42]\n[tool.threejs]\ndependencies=['threejs']\n",
        ),
    ] {
        std::fs::write(project.join(name), text).unwrap();
    }
    assert!(faber_engine::project::rules::facts(&project).unwrap().stacks.is_empty());
    assert_eq!(select().ids, baseline.ids);
}

#[test]
fn project_without_manifest_stack_facts_keeps_the_exact_kit() {
    let dir = tempfile::tempdir().unwrap();
    let store = skills(dir.path());
    let project = dir.path().join("project");
    std::fs::create_dir(&project).unwrap();
    for task in [TASK, "Improve frontend layout", "Summarize this document"] {
        let before = store.kit_for(Vendor::Codex, task, Some(&project), Area::Project, Permissions::FullAccess, &[]);
        for (name, text) in [
            ("package.json", r#"{"dependencies":{"ordinary-package":"1"}}"#),
            ("Cargo.toml", "[dependencies]\nordinary-crate='1'\n"),
            ("pyproject.toml", "[project]\ndependencies=['ordinary-package>=1']\n"),
        ] {
            std::fs::write(project.join(name), text).unwrap();
        }
        let after = store.kit_for(Vendor::Codex, task, Some(&project), Area::Project, Permissions::FullAccess, &[]);
        assert_eq!(after.1.ids, before.1.ids);
        assert_eq!(after.0.skill_instructions(), before.0.skill_instructions());
        for name in ["package.json", "Cargo.toml", "pyproject.toml"] {
            std::fs::remove_file(project.join(name)).unwrap();
        }
    }
}

pub(super) fn session_skill(root: &Path) -> Item {
    let item = skill("debug", "debugging", DEBUG);
    let store = Store::with_catalog(root, vec![item.clone()]);
    install_skill(&store, &item, DEBUG, root);
    item
}

async fn replay(
    vendor: Vendor,
    area: Area,
    permissions: Permissions,
    tools: Delivery,
    cwd: &Path,
    internal: bool,
    task: &str,
) -> (Vec<String>, Vec<Value>) {
    let mut entries = fixture(vendor.name(), "kits");
    for entry in entries.iter_mut().filter(|entry| entry.input) {
        if entry.msg["method"] == "turn/start" {
            entry.msg["params"]["input"][0]["text"] = task.into();
        } else if entry.msg["type"] == "user" {
            entry.msg["message"]["content"] = task.into();
        }
    }
    let fake = fake(entries);
    let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let sink = Arc::new(move |event| drop(tx.send(event)));
    let server;
    let session = match vendor {
        Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
        Vendor::Claude => {
            server = None;
            start_claude(
                ClaudeOptions {
                    bin: "fake".into(),
                    cwd: cwd.into(),
                    spawn: fake.spawn.clone(),
                    sandbox: None,
                    launch: ClaudeLaunch {
                        subagents: None,
                        temporary: false,
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
                        leads: false,
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
                    subagents: None,
                    temporary: false,
                    title: None,
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
                    leads: false,
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
    session.send(UserTurn::text(task));
    tokio::time::timeout(crate::support::DEADLINE, async {
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
                let (args, messages) = replay(vendor, area, permissions, tools, dir.path(), false, TASK).await;
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
                let (args, _) = replay(Vendor::Claude, area, permissions, checked, dir.path(), false, TASK).await;
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
    let (args, _) = replay(Vendor::Claude, Area::Chat, Permissions::Ask, kit, dir.path(), true, TASK).await;
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

fn server_fixture(item: &mut Item, root: &Path) -> (std::path::PathBuf, std::collections::BTreeMap<String, std::path::PathBuf>) {
    use base64::Engine;
    use sha2::{Digest, Sha512};
    let root = root.join(&item.id);
    let source = root.join("source");
    let server = item.server.as_mut().unwrap();
    let mut packages = std::collections::BTreeMap::new();
    let mut root_archive = None;
    let count = server.tree.len().max(1);
    for index in 0..count {
        let package = server.tree.get(index);
        let path = package.map_or("", |package| package.path.as_str());
        let entry = if path.is_empty() { server.entry.as_str() } else { "run.js" };
        let mut files = vec![(entry, b"fixture server".to_vec())];
        if let Some(package) = package {
            files.push((
                "package.json",
                serde_json::to_vec(&serde_json::json!({"name": package.package, "version": package.version})).unwrap(),
            ));
        }
        let archive = root.join(format!("{index}.tgz"));
        std::fs::create_dir_all(&root).unwrap();
        let mut tar =
            tar::Builder::new(flate2::write::GzEncoder::new(std::fs::File::create(&archive).unwrap(), flate2::Compression::fast()));
        for (file, body) in files {
            let out = source.join(path).join(file);
            std::fs::create_dir_all(out.parent().unwrap()).unwrap();
            std::fs::write(out, &body).unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            tar.append_data(&mut header, format!("package/{file}"), body.as_slice()).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        if path.is_empty() {
            root_archive = Some(archive.clone());
        } else {
            packages.insert(path.to_owned(), archive.clone());
        }
        if let Some(package) = server.tree.get_mut(index) {
            package.integrity =
                format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(Sha512::digest(std::fs::read(&archive).unwrap())));
        }
    }
    item.digest = digest(&source).unwrap();
    (root_archive.unwrap(), packages)
}

#[tokio::test]
async fn a_checked_two_level_server_tree_starts_through_both_fake_native_sessions_and_removes_together() {
    let task = "Benchmark browser performance with devtools";
    for vendor in [Vendor::Claude, Vendor::Codex] {
        let (item, archives) = crate::tools::pinned_tree_fixture(Value::Null, &[]);
        let source = crate::tools::tree_source(archives);
        let dir = tempfile::tempdir().unwrap();
        let node = dir.path().join("fixture-node");
        let store = Store::with_catalog(&dir.path().join("data"), vec![item.clone()]).with_node(node.clone());
        faber_engine::tools::install(&store, &item, Some(&source.origin)).await.unwrap();
        let (kit, report) = store.kit_for(vendor, task, Some(dir.path()), Area::Project, Permissions::FullAccess, &[]);
        assert!(report.ids.contains(&item.id));
        assert_eq!(kit.servers.len(), 1);
        let launch = kit.servers[0].clone();
        assert_eq!(launch.command, node);
        let root = Path::new(&launch.args[0]).parent().unwrap();
        assert_eq!(digest(root).unwrap(), item.digest);
        assert!(root.join("node_modules/middle/node_modules/@scope/leaf/run.js").is_file());
        let (checked, damaged, io_failed) = kit.verified_for_launch(vendor);
        assert!(damaged.is_empty() && !io_failed);
        let (args, messages) = replay(vendor, Area::Project, Permissions::FullAccess, checked, dir.path(), false, task).await;
        let config = match vendor {
            Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
            Vendor::Claude => serde_json::from_str::<Value>(&args.windows(2).find(|pair| pair[0] == "--mcp-config").unwrap()[1]).unwrap()
                ["mcpServers"]["faber-measure"].clone(),
            Vendor::Codex => messages.iter().find(|message| message["method"] == "thread/start").unwrap()["params"]["config"]
                ["mcp_servers.faber-measure"].clone(),
        };
        assert_eq!(config["command"], serde_json::json!(launch.command));
        assert_eq!(config["args"], serde_json::json!(launch.args));
        store.remove(&item.id).unwrap();
        assert!(store.installed().is_empty());
        assert!(store.kit_for(vendor, task, Some(dir.path()), Area::Project, Permissions::FullAccess, &[]).0.servers.is_empty());
        assert!(root.join("node_modules/middle/node_modules/@scope/leaf/run.js").is_file(), "the active session retains its checked tree");
    }
}

#[tokio::test]
async fn each_admitted_server_is_picked_for_its_task_and_keeps_its_reviewed_session_boundaries() {
    for (id, task, chat_safe) in [
        ("chrome-devtools", "Benchmark browser performance with devtools", false),
        ("playwright-mcp", "Automate browser tests with Playwright", false),
        ("sequential-thinking", "Plan the project step by step with sequential thinking", true),
        ("context7", "Look up library docs with Context7", true),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut item = faber_engine::tools::find(id).expect("each audited server is listed").clone();
        assert_eq!(item.chat_safe, chat_safe, "{id}: the explicit review determines Chat access");
        let (archive, packages) = server_fixture(&mut item, dir.path());
        let store = Store::with_catalog(&dir.path().join("data"), vec![item.clone()]).with_node(dir.path().join("fixture-node"));
        store.install_sources(&item, &archive, &packages).unwrap();
        for vendor in [Vendor::Claude, Vendor::Codex] {
            for area in [Area::Chat, Area::Project] {
                for permissions in [
                    Permissions::FullAccess,
                    Permissions::Ask,
                    Permissions::AcceptEdits,
                    Permissions::Plan,
                    Permissions::Auto,
                    Permissions::Isolated,
                ] {
                    let allowed = permissions == Permissions::FullAccess && (area == Area::Project || chat_safe);
                    let (kit, report) = store.kit_for(vendor, task, Some(dir.path()), area, permissions, &[]);
                    assert_eq!(report.ids.contains(&item.id), allowed, "{id}: {vendor:?} {area:?} {permissions:?}");
                    assert_eq!(kit.servers.len(), usize::from(allowed), "{id}: {vendor:?} {area:?} {permissions:?}");
                    if allowed {
                        let (checked, damaged, io_failed) = kit.verified_for_launch(vendor);
                        assert!(damaged.is_empty() && !io_failed);
                        let name = format!("faber-{id}");
                        let launch = checked.servers[0].clone();
                        let (args, messages) = replay(vendor, area, permissions, checked, dir.path(), false, task).await;
                        let config = match vendor {
                            Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
                            Vendor::Claude => {
                                serde_json::from_str::<Value>(&args.windows(2).find(|pair| pair[0] == "--mcp-config").unwrap()[1]).unwrap()
                                    ["mcpServers"][&name]
                                    .clone()
                            }
                            Vendor::Codex => {
                                messages.iter().find(|message| message["method"] == "thread/start").unwrap()["params"]["config"]
                                    [format!("mcp_servers.{name}")]
                                .clone()
                            }
                        };
                        assert_eq!(config["command"], serde_json::json!(launch.command), "{id}: {vendor:?} {area:?}");
                        assert_eq!(config["args"], serde_json::json!(launch.args), "{id}: {vendor:?} {area:?}");
                    }
                }
            }
            assert!(
                store
                    .kit_for(vendor, task, Some(dir.path()), Area::Project, Permissions::FullAccess, &[format!("faber-{id}")])
                    .0
                    .servers
                    .is_empty(),
                "{id}: a project switch wins over task selection"
            );
        }
        store.set_enabled(id, false).unwrap();
        for area in [Area::Chat, Area::Project] {
            assert!(
                store.kit(task, Some(dir.path()), area, Permissions::FullAccess, &[]).0.servers.is_empty(),
                "{id}: an off item reaches no session"
            );
        }
        store.remove(id).unwrap();
        assert!(store.installed().is_empty());
    }
}

#[tokio::test]
async fn the_kept_server_installs_and_reaches_both_native_sessions_until_switched_off_or_removed() {
    let task = "Benchmark browser performance with devtools";
    for vendor in [Vendor::Claude, Vendor::Codex] {
        let dir = tempfile::tempdir().unwrap();
        let mut item = faber_engine::tools::find("chrome-devtools").unwrap().clone();
        // Keep the shipped tree shape and launch metadata; only bytes and their pins belong to this inert fixture.
        let (archive, packages) = server_fixture(&mut item, dir.path());
        let server = item.server.as_ref().unwrap().clone();
        let node = dir.path().join("fixture-node");
        let store = Store::with_catalog(&dir.path().join("data"), vec![item.clone()]).with_node(node.clone());
        store.install_sources(&item, &archive, &packages).unwrap();
        assert_eq!(store.installed().get(&item.id), Some(&item.commit));
        for state in ["installed", "off", "on", "removed"] {
            match state {
                "off" => store.set_enabled(&item.id, false).unwrap(),
                "on" => store.set_enabled(&item.id, true).unwrap(),
                "removed" => store.remove(&item.id).unwrap(),
                _ => {}
            }
            let enabled = matches!(state, "installed" | "on");
            let (kit, report) = store.kit_for(vendor, task, Some(dir.path()), Area::Project, Permissions::FullAccess, &[]);
            assert_eq!(report.ids.iter().any(|id| id == &item.id), enabled, "{vendor:?} {state}: {report:?}");
            assert_eq!(kit.servers.len(), usize::from(enabled));
            let expected = kit.servers.first().cloned();
            if let Some(launch) = &expected {
                assert_eq!(launch.name, "faber-chrome-devtools");
                assert_eq!(launch.command, node);
                assert!(Path::new(&launch.args[0]).ends_with(&server.entry));
                assert_eq!(std::fs::read(&launch.args[0]).unwrap(), b"fixture server");
                assert_eq!(&launch.args[1..], server.args);
                assert_eq!(launch.env, server.env);
            }
            let (checked, damaged, io_failed) = kit.verified_for_launch(vendor);
            assert!(damaged.is_empty() && !io_failed);
            assert_eq!(checked.servers.len(), usize::from(enabled));
            let (args, messages) = replay(vendor, Area::Project, Permissions::FullAccess, checked, dir.path(), false, task).await;
            let config = match vendor {
                Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
                Vendor::Claude => args.windows(2).find(|pair| pair[0] == "--mcp-config")
                    .map(|pair| serde_json::from_str::<Value>(&pair[1]).unwrap()["mcpServers"]["faber-chrome-devtools"].clone())
                    .unwrap_or(Value::Null),
                Vendor::Codex => messages.iter().find(|message| message["method"] == "thread/start").unwrap()
                    ["params"]["config"]["mcp_servers.faber-chrome-devtools"].clone(),
            };
            if let Some(launch) = expected {
                let mut expected = serde_json::json!({"command": launch.command, "args": launch.args, "env": launch.env});
                if vendor == Vendor::Codex {
                    expected["enabled"] = true.into();
                    expected["env_vars"] = serde_json::json!([]);
                } else {
                    expected["type"] = "stdio".into();
                }
                assert_eq!(config, expected, "{vendor:?} {state}");
            } else {
                assert!(config.is_null(), "{vendor:?} {state}: {config}");
            }
        }
        assert!(store.installed().is_empty());
    }
}

#[test]
fn chat_servers_require_an_explicit_review_and_never_cross_guarded_modes_or_project_switches() {
    let dir = tempfile::tempdir().unwrap();
    let server = |id: &str, chat_safe| Item {
        id: id.into(),
        job: "measure".into(),
        license: "MIT".into(),
        repo: "fixture/server".into(),
        commit: "1234".into(),
        path: String::new(),
        digest: String::new(),
        server: Some(Server {
            package: id.into(),
            version: "1.0.0".into(),
            entry: "run.js".into(),
            args: vec![],
            env: Default::default(),
            tree: vec![],
        }),
        chat_safe,
        runtime: None,
        cli_name: None,
        tier: Default::default(),
    };
    let mut items = vec![server("reviewed", true), server("not-reviewed", false)];
    let fixtures = items.iter_mut().map(|item| server_fixture(item, dir.path())).collect::<Vec<_>>();
    let store = Store::with_catalog(&dir.path().join("data"), items.clone()).with_node("node".into());
    for (item, (archive, packages)) in items.iter().zip(&fixtures) {
        store.install_sources(item, archive, packages).unwrap();
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
