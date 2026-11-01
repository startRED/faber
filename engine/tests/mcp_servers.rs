use crate::support;
use faber_engine::claude::{ClaudeLaunch, claude_args, parse_claude_message};
use faber_engine::events::EngineEvent;
use faber_engine::mcp_servers::{choose, project_names_with, reported, rows};
use faber_engine::model::{Area, Permissions, Vendor};
use faber_engine::tools::{Delivery, Launch};
use serde_json::json;
use std::collections::BTreeSet;

fn names(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn mcp_servers_recorded_names_and_statuses_keep_each_source_and_hide_faber() {
    let claude = support::fixture("claude", "mcp-servers");
    let servers = parse_claude_message(&claude[1].msg)
        .into_iter()
        .find_map(|event| match event {
            EngineEvent::McpServers(rows) => Some(rows),
            _ => None,
        })
        .expect("the CLI's init reports its servers");
    let list = rows(
        &servers,
        &names(&["faber-chrome-devtools"]),
        &names(&["faber-chrome-devtools"]),
        &BTreeSet::new(),
        &names(&["project-reader"]),
    );
    assert_eq!(list.len(), 3, "Faber's internal server is part of the app");
    let value = serde_json::to_value(list).unwrap();
    assert_eq!(
        value,
        json!([
            {"name":"faber-chrome-devtools","source":"catalog","status":"connected","enabled":true},
            {"name":"personal-reader","source":"user","status":"needs-auth"},
            {"name":"project-reader","source":"project","status":"failed"}
        ])
    );
    let codex = support::fixture("codex", "mcp-servers");
    let all = codex.iter().filter_map(|e| e.msg["result"]["data"].as_array()).flatten().cloned().collect::<Vec<_>>();
    let reports = reported(&all, "runtimeStatus");
    let project = reports.iter().find(|r| r.name == "project").expect("the Codex recording has a project server");
    let list = rows(&reports, &names(&["measure"]), &names(&["measure"]), &names(&["measure"]), &names(&["project"]));
    assert_eq!(list.iter().find(|r| r.name == project.name).unwrap().source, "project");
    assert_eq!(list.iter().find(|r| r.name == "own").unwrap().source, "user");
    let catalog = list.iter().find(|r| r.name == "measure").unwrap();
    assert_eq!(catalog.source, "catalog");
    assert_eq!(catalog.enabled, Some(false));
    assert_eq!(catalog.status.as_deref(), Some("failed"), "keep the runtime state, never the auth state");
}

#[test]
fn mcp_servers_launch_choice_filters_only_catalog_servers_without_strict_claude_config() {
    let mut delivery = Delivery {
        servers: vec![Launch {
            name: "faber-chrome-devtools".into(),
            command: "node".into(),
            args: vec!["server.js".into()],
            env: Default::default(),
        }],
        ..Default::default()
    };
    for enabled in [true, false] {
        choose(&mut delivery, &if enabled { BTreeSet::new() } else { names(&["faber-chrome-devtools", "personal-reader"]) });
        let launch = ClaudeLaunch {
            area: Area::Project,
            model: None,
            effort: None,
            resume: None,
            user_hooks: false,
            permissions: Permissions::FullAccess,
            instructions: None,
            user_name: None,
            flags: Default::default(),
            tuning: Default::default(),
            group_instructions: None,
            research: None,
            helper: Default::default(),
            browse: false,
            conversations: false,
            leads: false,
            verify: false,
            progress: true,
            tools: delivery.clone(),
        };
        let args = claude_args(&launch);
        assert!(!args.iter().any(|a| a == "--strict-mcp-config"), "the person's own servers still load");
        let at = args.iter().position(|a| a == "--mcp-config").unwrap();
        let config: serde_json::Value = serde_json::from_str(&args[at + 1]).unwrap();
        assert_eq!(config["mcpServers"].get("faber-chrome-devtools").is_some(), enabled);
        assert!(config["mcpServers"].get("faber").is_some(), "Faber's tools are always separate");
        assert!(config["mcpServers"].get("personal-reader").is_none());
        assert_eq!(delivery.disabled_servers, if enabled { vec![] } else { vec!["faber-chrome-devtools"] });
    }
}

#[test]
fn mcp_servers_project_names_read_only_project_files_and_never_home_setup() {
    use std::cell::RefCell;
    let home = support::work_folder();
    let project = support::work_folder();
    for root in [home.path(), project.path()] {
        std::fs::create_dir(root.join(".codex")).unwrap();
        std::fs::write(root.join(".codex/config.toml"), "[mcp_servers.\"project.reader\"]\ncommand = 'private value'\n").unwrap();
        std::fs::write(root.join(".mcp.json"), r#"{"mcpServers":{"project-reader":{"env":{"TOKEN":"private value"}}}}"#).unwrap();
    }
    let reads = RefCell::new(vec![]);
    let read = |path: &std::path::Path| {
        reads.borrow_mut().push(path.to_path_buf());
        assert!(path.starts_with(project.path().canonicalize().unwrap()), "a home file was read: {path:?}");
        std::fs::read_to_string(path).ok()
    };
    for vendor in [Vendor::Claude, Vendor::Codex] {
        assert!(project_names_with(home.path(), vendor, Some(home.path()), &read).is_empty());
        let listed = project_names_with(project.path(), vendor, Some(home.path()), &read);
        assert_eq!(listed, names(&[if vendor == Vendor::Claude { "project-reader" } else { "project.reader" }]));
    }
    assert_eq!(reads.borrow().len(), 2, "only the two project files were read");
    #[cfg(windows)]
    {
        let linked = support::work_folder();
        junction::create(home.path().join(".codex"), linked.path().join(".codex")).unwrap();
        assert!(project_names_with(linked.path(), Vendor::Codex, Some(home.path()), &read).is_empty());
        assert_eq!(reads.borrow().len(), 2, "a junction to the person's setup is never read");
        let ancestor = support::work_folder();
        let nested_home = ancestor.path().join("person");
        std::fs::create_dir_all(nested_home.join(".codex")).unwrap();
        std::fs::write(nested_home.join(".codex/config.toml"), "[mcp_servers.private]\ncommand='never read'\n").unwrap();
        junction::create(nested_home.join(".codex"), ancestor.path().join(".codex")).unwrap();
        assert!(project_names_with(ancestor.path(), Vendor::Codex, Some(&nested_home), &read).is_empty());
        assert_eq!(reads.borrow().len(), 2, "a project containing home must not follow a link to its CLI setup");
    }
}
