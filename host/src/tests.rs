use super::*;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn mcp_schema_maps_table_argument_types() {
    assert_eq!(
        argument_schema(&[
            ("text", "String"),
            ("enabled", "bool"),
            ("count", "u64"),
            ("ratio", "f64"),
            ("tags", "Option<Vec<String>>"),
            ("choice", "Vendor")
        ]),
        json!({"type":"object", "properties":{"text":{"type":"string"}, "enabled":{"type":"boolean"}, "count":{"type":"integer"}, "ratio":{"type":"number"}, "tags":{"type":"array","items":{"type":"string"}}, "choice":{"type":"object","description":"Vendor JSON value"}}, "required":["text","enabled","count","ratio","choice"]})
    );
}

#[test]
fn the_table_names_each_command_once_and_never_a_reserved_name() {
    let names: Vec<&str> = Host::commands().iter().map(|c| c.name).collect();
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "a command is listed twice");
    for reserved in RESERVED {
        assert!(!names.contains(&reserved), "{reserved} is answered by the connection, never by the table");
    }
}

#[test]
fn arguments_are_listed_as_the_screen_sends_them() {
    let answer = Host::commands().iter().find(|c| c.name == "answer").unwrap();
    let keys: Vec<&str> = answer.args.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["id", "turnId", "requestId", "allow", "always"]);
    let group = Host::commands().iter().find(|c| c.name == "move_chat_to_group").unwrap();
    assert_eq!(group.args[1], ("groupId".to_string(), "Option<String>"));
    assert_eq!(Host::kind("new_group_chat"), Some(Kind::StartsTurn));
    assert_eq!(Host::kind("hello"), None);
}

/// Every command name `ui/api.ts` invokes: the text between `invoke<...>(` or `invoke(` and the first quote pair after it.
fn names_the_screen_calls() -> Vec<String> {
    let source = include_str!("../../ui/api.ts");
    let mut names = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("invoke") {
        rest = &rest[at + "invoke".len()..];
        let open = match rest.chars().next() {
            Some('<') => rest.find(">('").map(|i| i + 1),
            Some('(') => Some(0),
            _ => None,
        };
        let Some(open) = open else { continue };
        let Some(quoted) = rest[open..].strip_prefix("('") else { continue };
        if let Some(end) = quoted.find('\'') {
            names.push(quoted[..end].to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn the_table_holds_every_command_the_screen_calls_once() {
    let called = names_the_screen_calls();
    assert!(called.len() > 100, "the scan of ui/api.ts found only {}", called.len());
    // The one command the screen names that only a test build has.
    let test_only = ["visual_policy"];
    for name in &called {
        if !cfg!(feature = "test-hooks") && test_only.contains(&name.as_str()) {
            continue;
        }
        // The sharing commands exist only in a build that has an endpoint (R583).
        if !cfg!(faber_telemetry) && name.starts_with("sharing_") {
            continue;
        }
        let listed = Host::commands().iter().filter(|c| c.name == name).count();
        assert_eq!(listed, 1, "`{name}` is called by ui/api.ts and listed {listed} times");
    }
}

/// The command line ships the table's exported commands in `cli/commands.json` for its help when Faber is not running, and
/// the AI's tools are made from the same list: a command anything but the screen may send that the list lacks fails here.
/// `FABER_WRITE_COMMANDS=1` writes the list again.
#[test]
fn the_command_line_lists_every_command_anything_but_the_screen_may_send() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/commands.json");
    let now = serde_json::to_string_pretty(&Host::exported().collect::<Vec<_>>()).unwrap() + "\n";
    if std::env::var_os("FABER_WRITE_COMMANDS").is_some() {
        std::fs::write(&path, &now).unwrap();
    }
    let kept = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    assert!(kept == now, "cli/commands.json is not the table's exported commands: run this test with FABER_WRITE_COMMANDS=1");
    for name in ["run_app_start", "run_app_confirm", "run_app_status", "run_app_stop"] {
        let app_command = Host::exported().find(|command| command.name == name).unwrap();
        assert_eq!(app_command.reach, "person", "{name} is reserved for the person");
    }
    for screen_only in [
        "state",
        "open",
        "screen_ready",
        "site_open",
        "media_enter",
        "media_grant",
        "media_grant_web",
        "media_release",
        "media_leave",
        "media_clear",
        "media_chunk",
    ] {
        assert!(Host::exported().all(|c| c.name != screen_only), "`{screen_only}` is the screen's own");
    }
}

#[test]
fn only_the_browser_pane_the_page_server_and_the_first_paint_are_the_windows() {
    let windows: Vec<&str> = Host::commands().iter().filter(|c| c.kind == Kind::Window).map(|c| c.name).collect();
    for name in &windows {
        assert!(
            name.starts_with("site_")
                || name.starts_with("cookie_")
                || name.starts_with("visual_")
                || name.starts_with("test_")
                || ["screen_ready", "report_checks", "show_conversation"].contains(name),
            "{name} needs the window, yet it is not one of the pane's, the page server's or the first paint's"
        );
    }
    assert!(windows.contains(&"site_place") && windows.contains(&"cookie_forget") && windows.contains(&"screen_ready"));
}

struct Quiet;
impl Shell for Quiet {}

#[tokio::test]
async fn damaged_conversation_commands_only_accept_screen_ids_and_keep_the_original() {
    let (host, dir) = host().await;
    let file = dir.path().join("data/conversations/x.json");
    let copy = file.with_extension("json.damaged");
    std::fs::write(&file, b"{").unwrap();
    host.engine.state();
    host.engine.state();
    assert!(copy.exists());
    for command in ["show_damaged_conversation", "remove_damaged_conversation"] {
        for caller in [Caller::Person, Caller::Agent { conversation: Some("x".into()) }] {
            assert_eq!(key(host.dispatch_as(&caller, command, json!({ "id": "x" })).await), "error.reach.screen");
        }
        assert!(host.dispatch(command, json!({ "id": "../x" })).await.is_err());
        assert!(copy.exists());
    }
    // A valid copy reaches the shell; this headless shell can only report that it has no window.
    assert_eq!(key(host.dispatch("show_damaged_conversation", json!({ "id": "x" })).await), NO_WINDOW);
    host.dispatch("remove_damaged_conversation", json!({ "id": "x" })).await.unwrap();
    assert!(!copy.exists());
    assert_eq!(std::fs::read(file).unwrap(), b"{");
    assert!(!host.engine.state().conversations.iter().any(|c| c.id == "x"));
}

/// A host over a real engine in a temporary data folder, with no CLI, no window and a hidden run's rules.
async fn host() -> (Host, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let wiring = Wiring::new(Arc::new(Quiet));
    let opts = EngineOptions {
        data_dir: data.clone(),
        // Private Chat work lives beside the data folder, never inside it (#383).
        #[cfg(windows)]
        chat_dir: dir.path().join("chat-work"),
        clis: BTreeMap::new(),
        spawn: real_spawner(),
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: Handle::current(),
        ask: None,
    };
    let engine = Engine::new(opts, wiring.emitter()).unwrap();
    (wiring.host(engine, data, true, Handle::current()), dir)
}

#[tokio::test]
async fn t515g_save_file_command_preserves_other_links_to_destination() {
    struct SaveTo(PathBuf);
    impl Shell for SaveTo {
        fn pick_save(&self, _filter: Option<Filter>, _name: String) -> BoxFuture<'_, Picked> {
            Box::pin(async { Ok(Some(self.0.clone())) })
        }
    }
    let (mut host, _dir) = host().await;
    let folder = tempfile::tempdir().unwrap();
    let conversation = host.engine.chat_folder(None, Some(folder.path())).unwrap();
    let source = folder.path().join("saved.txt");
    std::fs::write(&source, b"complete workspace contents").unwrap();
    let saved = tempfile::tempdir().unwrap();
    let other = saved.path().join("other.txt");
    let target = saved.path().join("saved.txt");
    std::fs::write(&other, b"unrelated contents").unwrap();
    std::fs::hard_link(&other, &target).unwrap();
    host.shell = Arc::new(SaveTo(target.clone()));
    assert_eq!(host.dispatch("save_file", json!({"id":conversation.id, "path":"saved.txt"})).await.unwrap(), json!(true));
    assert_eq!(std::fs::read(&source).unwrap(), b"complete workspace contents");
    assert_eq!(std::fs::read(&target).unwrap(), b"complete workspace contents");
    assert_eq!(std::fs::read(&other).unwrap(), b"unrelated contents");
    assert_eq!(std::fs::read_dir(saved.path()).unwrap().count(), 2, "temporary siblings are cleaned up");
    host.engine.shutdown();
}

fn key(result: Result<Value, Failure>) -> String {
    result.expect_err("the command should have failed").key
}

#[tokio::test]
async fn unidentified_agents_never_record_or_receive_a_grant() {
    let (host, _dir) = host().await;
    let agent = Caller::Agent { conversation: None };
    for _ in 0..2 {
        let failure = host.dispatch_as(&agent, "save_settings", json!({"patch": {"language": "en"}})).await.unwrap_err();
        assert_eq!(failure.key, "error.reach.ask");
        assert!(failure.data.is_none());
    }
    assert!(lock(&host.asks).pending().is_empty());
    assert_eq!(host.dispatch_as(&agent, "asks", json!({})).await.unwrap(), json!([]));
}

#[tokio::test]
async fn screen_can_recover_exact_pending_requests_without_exposing_them_to_other_agents() {
    let (host, _dir) = host().await;
    lock(&host.asks).record("one", "remove_project", json!({"id": "target"}), "projects");
    let pending = host.dispatch("asks", json!({})).await.unwrap();
    assert_eq!(pending[0]["command"], "remove_project");
    assert_eq!(pending[0]["args"]["id"], "target");
    let other = Caller::Agent { conversation: Some("other".into()) };
    assert_eq!(host.dispatch_as(&other, "asks", json!({})).await.unwrap(), json!([]));
}

#[tokio::test]
async fn arguments_that_do_not_fit_and_names_not_in_the_table_are_refused_by_key() {
    let (host, _dir) = host().await;
    assert_eq!(key(host.dispatch("open", json!({})).await), "error.host.badArgs");
    assert_eq!(key(host.dispatch("open", json!({ "id": 7 })).await), "error.host.badArgs");
    assert_eq!(key(host.dispatch("open", json!(["c1"])).await), "error.host.badArgs");
    assert_eq!(key(host.dispatch("no_such_command", Value::Null).await), "error.host.unknownCommand");
}

#[tokio::test]
async fn a_chat_group_made_through_the_table_is_in_the_state_the_screen_reads() {
    let (host, _dir) = host().await;
    let group = host.dispatch("create_chat_group", json!({ "name": "Taxes" })).await.unwrap();
    assert_eq!(group["name"], "Taxes");
    let state = host.dispatch("state", Value::Null).await.unwrap();
    assert_eq!(state["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(state["models"], json!([]));
    assert!(state["look"].is_object(), "the look of the saved appearance travels with the state");
    assert!(state["groups"].to_string().contains("Taxes"), "{}", state["groups"]);
}

#[tokio::test]
async fn what_needs_a_window_says_so_and_a_link_that_is_not_web_is_refused_first() {
    let (host, _dir) = host().await;
    assert_eq!(key(host.dispatch("show_conversation", json!({"id":"c1"})).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("screen_ready", Value::Null).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("visual_open", Value::Null).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("visual_page", json!({ "id": "x", "revision": 1, "document": "<p>x</p>" })).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("visual_close", json!({ "id": "x" })).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("visual_document", json!({ "path": "faber-visual://app/0.html" })).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("open_link", json!({ "url": "https://example.com" })).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("open_link", json!({ "url": "file:///C:/Windows" })).await), "error.link.web");
    assert_eq!(key(host.dispatch("add_project", Value::Null).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("set_shell_texts", json!({ "texts": {} })).await), NO_WINDOW);
    assert_eq!(host.dispatch("update_status", Value::Null).await.unwrap(), json!({ "phase": "none" }));
    assert_eq!(key(host.dispatch("update_check", Value::Null).await), NO_WINDOW);
}

#[tokio::test]
async fn the_panes_window_commands_reach_the_pane_a_front_end_gives_the_host_and_none_without() {
    let (host, _dir) = host().await;
    let asks = [
        ("cookie_held", json!({ "id": "c" })),
        ("site_history", json!({ "id": "c" })),
        ("site_place", json!({ "load": 1, "boxId": 1, "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0 })),
    ];
    for (name, args) in &asks {
        assert_eq!(
            key(host.dispatch(name, args.clone()).await),
            NO_WINDOW,
            "{name}: the Tauri shell and the command line have no pane here"
        );
    }
    let views = pane::Relay::new(Arc::new(|_| {}));
    host.set_pane(pane::Pane::new(views, pane::EngineHome::new(|_| {}), Handle::current()));
    let answers = [json!([]), json!([false, false]), json!(false)];
    for ((name, args), answer) in asks.iter().zip(answers) {
        assert_eq!(host.dispatch(name, args.clone()).await.unwrap(), answer, "{name}: the pane answers, with no page open");
    }
    // The commands that are still a window's own stay so.
    assert_eq!(key(host.dispatch("screen_ready", Value::Null).await), NO_WINDOW);
}

#[tokio::test]
async fn an_appearance_that_fails_comes_back_with_the_field_it_is_about() {
    let (host, _dir) = host().await;
    let failure = host.dispatch("preview_look", json!({ "appearance": { "base": "nowhere" } })).await.unwrap_err();
    assert_eq!(failure.key, "error.look.fileInvalid");
    assert_eq!(failure.detail.as_deref(), Some("base"));
}

#[tokio::test]
async fn the_last_notes_seen_is_kept_in_the_data_folder() {
    let (host, dir) = host().await;
    if !cfg!(feature = "test-hooks") {
        assert_eq!(host.dispatch("notes_seen", Value::Null).await.unwrap(), Value::Null);
    }
    host.dispatch("save_notes_seen", json!({ "id": 12 })).await.unwrap();
    assert_eq!(host.dispatch("notes_seen", Value::Null).await.unwrap(), json!(12));
    assert_eq!(std::fs::read_to_string(dir.path().join("data").join("notes-seen")).unwrap(), "12");
}

#[tokio::test]
async fn an_announcement_reaches_every_subscriber() {
    let (host, _dir) = host().await;
    let mut heard = host.subscribe();
    host.announce(&AppEvent::List);
    assert_eq!(heard.recv().await.unwrap()["type"], "list");
}

#[test]
fn quitting_ends_the_crash_marker_before_anything_else_closes() {
    let source = include_str!("lib.rs").replace("\r\n", "\n");
    let from = source.find("pub fn shut_down(&self)").expect("Host::shut_down is in this file");
    let body = &source[from..from + source[from..].find("\n    }\n").unwrap()];
    let order: Vec<usize> = ["crash::end_run(", "terminate_hidden(", "shell.shut_down(", "engine.shutdown("]
        .iter()
        .map(|call| body.find(call).expect(call))
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
}

#[tokio::test]
async fn markdown_read_matches_file_export_builder() {
    let (host, dir) = host().await;
    let conversation: faber_engine::model::Conversation =
        serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
    faber_engine::store::Store::new(&dir.path().join("data")).save_checked(&conversation).unwrap();
    host.engine.save_settings(Settings { language: Some("en".into()), ..Settings::default() }).unwrap();
    let words = faber_engine::export::Words {
        you: "You".into(),
        auto: "Auto".into(),
        attachments: "Attachments".into(),
        files: "Files changed".into(),
    };
    let (_, expected) = host.engine.export_conversation(&conversation.id, &words).unwrap();
    let value = host.dispatch("conversation_markdown", json!({"id":conversation.id})).await.unwrap();
    assert_eq!(value["markdown"], expected);
}

#[tokio::test]
async fn revoked_before_first_poll_refuses_mutation() {
    let (host, _dir) = host().await;
    let flag = Arc::new(AtomicBool::new(false));
    let call = host.dispatch_session(&Caller::Person, "create_chat_group", json!({"name":"must not exist"}), Some(flag.clone()));
    flag.store(true, Ordering::Release);
    assert_eq!(call.await.unwrap_err().key, "error.channel.revoked");
    assert!(host.engine.state().groups.iter().all(|group| group.name != "must not exist"));
}

fn seed_conversation(host: &Host, id: &str, started_by: Option<&str>, waiting_on: Option<faber_engine::model::WaitingOn>) {
    let mut conv: faber_engine::model::Conversation =
        serde_json::from_value(json!({ "id": id, "title": id, "area": "chat", "createdAt": 1, "updatedAt": 2, "messages": [] })).unwrap();
    conv.started_by = started_by.map(String::from);
    conv.waiting_on = waiting_on;
    faber_engine::store::Store::new(&host.data_dir).save(&conv);
}

#[tokio::test]
async fn session_control_accepts_a_codex_parent_and_checks_its_children() {
    let (host, dir) = host().await;
    let project = host.engine.add_project(&dir.path().to_string_lossy()).unwrap();
    for (id, parent) in [("lead", None), ("child", Some("lead")), ("busy", Some("lead")), ("other", None), ("foreign", None)] {
        let mut conv: faber_engine::model::Conversation = serde_json::from_value(json!({
            "id": id, "title": id, "area": "project", "projectId": project.id, "createdAt": 1, "updatedAt": 2,
            "messages": [], "choice": {"vendor": "codex"}
        }))
        .unwrap();
        conv.started_by = parent.map(String::from);
        if id != "busy" {
            conv.messages = serde_json::from_value(json!([{"id": "done", "role": "assistant", "text": "done", "at": 1}])).unwrap();
        }
        if id == "foreign" {
            conv.project_id = Some("another-project".into());
        }
        faber_engine::store::Store::new(&host.data_dir).save(&conv);
    }
    host.engine.open("lead").unwrap();
    let agent = Caller::Agent { conversation: Some("lead".into()) };
    let start = host.dispatch_session(&agent, "start_conversation", json!({"title": "Work", "brief": "work"}), None).await.unwrap_err();
    assert_eq!(start.key, "error.session.refused");
    // This host has no CLI: a valid request reaches launch, rather than failing validation or parent setup.
    assert_eq!(start.detail.as_deref(), Some("the conversation could not start"));
    for (command, args, expected_key, expected_detail) in [
        ("interrupt", json!({"id": "other"}), "error.reach.ask", None),
        ("resume_conversation", json!({"id": "other"}), "error.session.refused", Some("that conversation was not started by you")),
        ("archive", json!({"id": "foreign", "on": true}), "error.session.refused", Some("no conversation with that id is in this project")),
        (
            "archive",
            json!({"id": "other", "on": false}),
            "error.session.refused",
            Some("only the person can bring an archived conversation back"),
        ),
        ("mark_waiting", json!({"on": ["other"]}), "error.session.refused", Some("that conversation was not started by you")),
        (
            "message_conversation",
            json!({"id": "foreign", "message": "Status?"}),
            "error.session.refused",
            Some("no conversation with that id is in this project"),
        ),
    ] {
        let error = host.dispatch_session(&agent, command, args, None).await.unwrap_err();
        assert_eq!(error.key, expected_key, "{command}");
        assert_eq!(error.detail.as_deref(), expected_detail, "{command}");
    }
    host.dispatch_session(&agent, "mark_waiting", json!({"on": ["busy"], "by": "other"}), None).await.unwrap();
    assert_eq!(host.engine.open("lead").unwrap().waiting_on.unwrap().ids, vec!["busy"]);
    host.dispatch_session(&agent, "interrupt", json!({"id": "child"}), None).await.unwrap();
    assert!(host.engine.open("child").unwrap().stopped);
    let resume = host.dispatch_session(&agent, "resume_conversation", json!({"id": "child"}), None).await.unwrap_err();
    assert_eq!(resume.key, "error.session.refused");
    assert_eq!(resume.detail.as_deref(), Some("this AI may be billed per use, so Faber does not start conversations on it by itself"));
    host.dispatch_session(&agent, "message_conversation", json!({"id": "child", "message": "Status?"}), None).await.unwrap();
    let child = host.engine.open("child").unwrap();
    assert!(child.stopped);
    assert_eq!(child.queued.len(), 1);
    assert!(child.queued[0].text.contains("Status?"));
    assert_eq!(child.queued[0].choice.vendor, "codex");
    host.engine.remove_queued("child", &child.queued[0].id);
    host.dispatch_session(&agent, "archive", json!({"id": "child", "on": true}), None).await.unwrap();
    assert!(host.engine.open("child").unwrap().archived_at.is_some());
    let resume = host.dispatch_session(&agent, "resume_conversation", json!({"id": "child"}), None).await.unwrap_err();
    assert_eq!(resume.key, "error.session.refused");
    assert_eq!(resume.detail.as_deref(), Some("the conversation is archived"));
    // Archiving is organizing: any finished conversation of the project goes, not only the ones this AI started.
    host.dispatch_session(&agent, "archive", json!({"id": "other", "on": true}), None).await.unwrap();
    assert!(host.engine.open("other").unwrap().archived_at.is_some());
}

#[tokio::test]
async fn dispatch_as_sets_by_from_a_conversation_agent_and_strips_it_for_everyone_else() {
    let (host, _dir) = host().await;
    seed_conversation(&host, "c1", Some("c2"), None);
    seed_conversation(&host, "c2", None, None);

    for caller in [Caller::Agent { conversation: None }, Caller::Person, Caller::Screen] {
        assert_eq!(
            key(host.dispatch_as(&caller, "mark_waiting", json!({ "by": "c1", "note": "checks" })).await),
            "error.session.agentOnly",
            "{caller:?} must not pass `by` through"
        );
    }

    // Even when `args` claims `by: "c2"` (the parent of `c1`), `Caller::Agent { conversation: Some("c1") }` overwrites `by`
    // with `"c1"`, so `archive` and `interrupt` act as `c1` on itself and are refused by the engine (`error.session.refused`).
    let agent = Caller::Agent { conversation: Some("c1".into()) };
    let archived = host.dispatch_as(&agent, "archive", json!({ "id": "c1", "on": true, "by": "c2" })).await.unwrap_err();
    assert_eq!(archived.key, "error.session.refused");
    assert!(archived.detail.is_some());
    let stopped = host.dispatch_as(&agent, "interrupt", json!({ "id": "c1", "by": "c2" })).await.unwrap_err();
    assert_eq!(stopped.key, "error.session.refused");
    assert!(stopped.detail.is_some());
}

#[tokio::test]
async fn send_is_refused_for_agents_and_kept_for_the_person_and_the_screen() {
    let (host, _dir) = host().await;
    seed_conversation(&host, "c1", None, None);
    let args = json!({ "req": { "conversationId": "c1", "text": "hello", "area": "chat", "choice": { "vendor": "auto" } } });
    for agent in [Caller::Agent { conversation: Some("c1".into()) }, Caller::Agent { conversation: None }] {
        assert_eq!(key(host.dispatch_as(&agent, "send", args.clone()).await), "error.session.useSession");
    }
    for caller in [Caller::Person, Caller::Screen] {
        let result = host.dispatch_as(&caller, "send", args.clone()).await;
        let err = result.err().map(|f| f.key);
        assert_ne!(err.as_deref(), Some("error.session.useSession"), "{caller:?}");
        assert_ne!(err.as_deref(), Some("error.host.badArgs"), "{caller:?}");
    }
}

#[tokio::test]
async fn the_four_session_commands_need_an_agent_conversation() {
    let (host, _dir) = host().await;
    let commands_json = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/commands.json")).unwrap();
    for (cmd, args) in [
        ("start_conversation", json!({ "title": "Fix", "brief": "Run tests" })),
        ("message_conversation", json!({ "id": "c2", "message": "Status?" })),
        ("resume_conversation", json!({ "id": "c2" })),
        ("mark_waiting", json!({ "note": "waiting for build" })),
    ] {
        assert!(Host::exported().any(|c| c.name == cmd), "`{cmd}` is exported");
        assert!(commands_json.contains(&format!("\"name\": \"{cmd}\"")), "`{cmd}` is in cli/commands.json");
        assert_eq!(key(host.dispatch(cmd, args.clone()).await), "error.session.agentOnly", "{cmd}");
        for caller in [Caller::Agent { conversation: None }, Caller::Person, Caller::Screen] {
            let mut with_spoofed_by = args.clone();
            with_spoofed_by["by"] = json!("c1");
            assert_eq!(key(host.dispatch_as(&caller, cmd, with_spoofed_by).await), "error.session.agentOnly", "{cmd} for {caller:?}");
        }
    }
}

#[tokio::test]
async fn interrupt_belongs_to_an_agents_own_family_and_archive_is_free_within_its_project() {
    assert_eq!(Host::reach("archive"), Some(Reach::Free));
    assert_eq!(Host::reach("interrupt"), Some(Reach::Own("/id")));
    let (host, _dir) = host().await;
    seed_conversation(&host, "c1", None, None);
    seed_conversation(&host, "c2", Some("c1"), None);
    seed_conversation(&host, "c9", None, None);

    let agent = Caller::Agent { conversation: Some("c1".into()) };
    assert_eq!(key(host.dispatch_as(&agent, "interrupt", json!({ "id": "c9" })).await), "error.reach.ask");
    // Within its own family, reach allows the call and the engine's session verb runs.
    assert_ne!(key(host.dispatch_as(&agent, "archive", json!({ "id": "c2", "on": true })).await), "error.reach.ask");
    // Archiving is organizing: reach lets it through to another conversation of the project too.
    assert_ne!(key(host.dispatch_as(&agent, "archive", json!({ "id": "c9", "on": true })).await), "error.reach.ask");
    host.dispatch_as(&Caller::Person, "archive", json!({ "id": "c9", "on": false })).await.unwrap();

    // Person and screen archive and unarchive any conversation as before, even if `by` was sent.
    host.dispatch_as(&Caller::Person, "archive", json!({ "id": "c9", "on": true, "by": "c1" })).await.unwrap();
    let c9 = host.dispatch("open", json!({ "id": "c9" })).await.unwrap();
    assert!(c9["archivedAt"].is_number(), "{c9}");
    host.dispatch_as(&Caller::Screen, "archive", json!({ "id": "c9", "on": false, "by": "c1" })).await.unwrap();
    let c9 = host.dispatch("open", json!({ "id": "c9" })).await.unwrap();
    assert!(c9["archivedAt"].is_null(), "{c9}");
    host.dispatch_as(&Caller::Person, "interrupt", json!({ "id": "c9", "by": "c1" })).await.unwrap();
}

#[tokio::test]
async fn a_conversation_with_waiting_on_reads_as_waiting_in_conversations_and_conversation() {
    let (host, _dir) = host().await;
    seed_conversation(
        &host,
        "c1",
        None,
        Some(faber_engine::model::WaitingOn {
            storage_blocked: false,
            ids: vec!["c2".into()],
            note: "waiting for child".into(),
            since: 5,
            until: 300_005,
            problem: None,
            pr: None,
        }),
    );
    seed_conversation(&host, "c2", Some("c1"), None);

    let rows = host.dispatch("conversations", Value::Null).await.unwrap();
    let list = rows.as_array().unwrap();
    let r1 = list.iter().find(|r| r["id"] == "c1").unwrap();
    assert_eq!(r1["state"], "waiting");
    assert_eq!(r1["waitingOn"], json!({ "ids": ["c2"], "note": "waiting for child", "since": 5, "until": 300_005 }));
    let r2 = list.iter().find(|r| r["id"] == "c2").unwrap();
    assert_eq!(r2["state"], "idle");
    assert!(r2.get("waitingOn").is_none(), "{r2}");

    let c1 = host.dispatch("conversation", json!({ "id": "c1" })).await.unwrap();
    assert_eq!(c1["state"], "waiting");
    assert_eq!(c1["waitingOn"], json!({ "ids": ["c2"], "note": "waiting for child", "since": 5, "until": 300_005 }));
    let c2 = host.dispatch("conversation", json!({ "id": "c2" })).await.unwrap();
    assert_eq!(c2["state"], "new");
    assert!(c2.get("waitingOn").is_none(), "{c2}");
}

#[tokio::test]
async fn conversation_headers_locate_numbered_messages_without_their_bodies() {
    let (host, _dir) = host().await;
    let large = "Message body. ".repeat(20_000);
    let conversation = serde_json::from_value(json!({
        "id":"indexed", "title":"Records", "area":"chat", "createdAt":1, "updatedAt":6,
        "messages":[
            {"id":"q1","role":"user","text":large,"at":1},
            {"id":"a1","role":"assistant","text":"First answer","at":2,"status":"done"},
            {"id":"q2","role":"user","text":"Second question","at":3},
            {"id":"q3","role":"user","text":"Follow-up","at":4},
            {"id":"a2","role":"assistant","text":"Partial answer","at":5,"status":"streaming"}
        ]
    }))
    .unwrap();
    faber_engine::store::Store::new(&host.data_dir).save(&conversation);

    for open in [false, true] {
        if open {
            host.engine.view_conversation("indexed", |_, _| ()).unwrap();
        }
        let page = host.dispatch("conversation_headers", json!({"id":"indexed","role":"assistant","from":2,"limit":1})).await.unwrap();
        assert_eq!(
            page,
            json!({"id":"indexed","total":5,"matching":2,"headers":[
                {"number":5,"id":"a2","role":"assistant","at":5,"status":"stopped"}
            ]})
        );
        let read = host.dispatch("conversation", json!({"id":"indexed","from":page["headers"][0]["number"]})).await.unwrap();
        assert_eq!(read["messages"][0]["id"], "a2");
        assert_eq!(read["messages"][0]["status"], page["headers"][0]["status"]);
    }

    let all = host.dispatch("conversation_headers", json!({"id":"indexed"})).await.unwrap();
    assert_eq!(all["headers"].as_array().unwrap().len(), 5);
    assert_eq!(all["headers"][0], json!({"number":1,"id":"q1","role":"user","at":1}));
    assert!(all.to_string().len() < 600, "large text stays out of the header page");
    let empty = host.dispatch("conversation_headers", json!({"id":"indexed","role":"user","from":4})).await.unwrap();
    assert_eq!(empty, json!({"id":"indexed","total":5,"matching":3,"headers":[]}));
    for bad in [json!({"from":0}), json!({"limit":0}), json!({"limit":101}), json!({"role":"system"})] {
        let mut args = bad;
        args["id"] = json!("indexed");
        assert_eq!(host.dispatch("conversation_headers", args).await.unwrap_err().key, "error.host.badArgs");
    }
}

#[tokio::test]
async fn restored_saved_wait_is_waiting_in_a_read_and_exact_list_filters() {
    let (host, dir) = host().await;
    let project = host.engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let scenarios = [
        ("saved-wait", None, false, "waiting"),
        ("checks-interrupted", Some("running"), false, "waiting"),
        ("decision", Some("needsYou"), false, "waiting"),
        ("interrupted", None, true, "interrupted"),
    ];
    let store = faber_engine::store::Store::new(&host.data_dir);
    for (id, checks, closed, _) in scenarios {
        let conversation = serde_json::from_value(json!({
            "id": id, "title": "Check later", "area": "project", "projectId": project.id,
            "createdAt": since, "updatedAt": since,
            "messages": [{"id":"last", "role":"assistant", "text":"Waiting", "at":since, "status":"done", "closed":closed,
                "checks": checks.map(|state| json!({"state":state}))}],
            "waitingOn": {"note":"Waiting for a result", "since":since, "until":since + 300_000}
        }))
        .unwrap();
        store.save(&conversation);
    }
    host.engine.settle_start();
    assert!(host.engine.state().running.items.iter().any(|item| item.conversation_id == "saved-wait"));

    for (id, _, _, expected) in scenarios {
        let read = host.dispatch("conversation", json!({"id":id,"last":0})).await.unwrap();
        assert_eq!(read["waitingOn"]["until"], since + 300_000);
        assert_eq!(read["state"], expected, "conversation {id}");
        assert!(read.get("interruptedTurn").is_none(), "an item in the panel cannot be resumed manually");
    }
    for word in ["waiting", "working", "running", "interrupted"] {
        let list = host.dispatch("conversations", json!({"state":[word]})).await.unwrap();
        let rows = list.as_array().unwrap();
        for (id, _, _, expected) in scenarios {
            let present = rows.iter().any(|row| row["id"] == id);
            assert_eq!(present, expected == word || word == "running" && expected != "interrupted", "filter {word}: {list}");
        }
    }
}

#[tokio::test]
async fn a_local_supervisor_can_check_only_the_exact_saved_wait() {
    let (host, dir) = host().await;
    let project = host.engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let conv: faber_engine::model::Conversation = serde_json::from_value(json!({
        "id":"waiting", "title":"Build", "area":"project", "projectId":project.id,
        "createdAt":1, "updatedAt":2, "messages":[], "waitingOn":{"since":5,"until":300005,"note":"build"}
    }))
    .unwrap();
    let store = faber_engine::store::Store::new(&host.data_dir);
    store.save_checked(&conv).unwrap();
    let exact = json!({"id":"waiting","since":5,"until":300005});
    let denied = host.dispatch_as(&Caller::Agent { conversation: None }, "wake_wait", exact.clone()).await.unwrap_err();
    assert_eq!(denied.key, "error.reach.ask");
    let agent = Caller::Agent { conversation: Some("agent".into()) };
    let denied = host.dispatch_as(&agent, "wake_wait", exact.clone()).await.unwrap_err();
    assert_eq!(denied.key, "error.reach.ask");
    let requests = host.dispatch_as(&agent, "asks", json!({})).await.unwrap();
    assert_eq!(requests[0]["label"], "wake");
    let stale = host.dispatch_as(&Caller::Person, "wake_wait", json!({"id":"waiting","since":5,"until":300004})).await.unwrap();
    assert_eq!(stale, json!({"state":"obsolete"}));
    assert_eq!(store.get("waiting").unwrap().waiting_on.unwrap().problem, None);
    let blocked = host.dispatch_as(&Caller::Person, "wake_wait", exact).await.unwrap();
    assert_eq!(blocked, json!({"state":"blocked","reason":"error.noSubscription"}));
    let kept = store.get("waiting").unwrap();
    assert_eq!(kept.waiting_on.unwrap().problem.as_deref(), Some("error.noSubscription"));
    assert!(kept.messages.is_empty() && kept.queued.is_empty());
}

#[test]
fn by_is_the_askers_own_conversation_or_nothing() {
    let c1 = Caller::Agent { conversation: Some("c1".into()) };
    assert_eq!(with_by(&c1, json!({ "id": "x", "by": "c2" })), json!({ "id": "x", "by": "c1" }));
    assert_eq!(with_by(&c1, Value::Null), json!({ "by": "c1" }));
    for caller in [Caller::Agent { conversation: None }, Caller::Person, Caller::Screen] {
        assert_eq!(with_by(&caller, json!({ "id": "x", "by": "c2" })), json!({ "id": "x" }), "{caller:?}");
    }
}

#[tokio::test]
async fn an_ai_pins_but_never_unpins() {
    let (host, _dir) = host().await;
    seed_conversation(&host, "c1", None, None);
    host.dispatch_as(&Caller::Screen, "pin", json!({ "id": "c1", "on": true })).await.unwrap();
    let agent = Caller::Agent { conversation: Some("c9".into()) };
    assert_eq!(host.dispatch_as(&agent, "pin", json!({ "id": "c1", "on": false })).await.unwrap_err().key, "error.session.refused");
    let c1 = host.dispatch("open", json!({ "id": "c1" })).await.unwrap();
    assert!(c1["pinnedAt"].is_number(), "{c1}");
    host.dispatch_as(&Caller::Person, "pin", json!({ "id": "c1", "on": false, "by": "c9" })).await.unwrap();
    let c1 = host.dispatch("open", json!({ "id": "c1" })).await.unwrap();
    assert!(c1["pinnedAt"].is_null(), "{c1}");
}
#[tokio::test]
async fn an_answers_before_and_after_pictures_are_read_by_name_and_shown_by_site_picture() {
    let (host, _dir) = host().await;
    seed_conversation(&host, "c1", None, None);
    let answers = host.data_dir.join("attachments/c1/answer-pictures");
    std::fs::create_dir_all(&answers).unwrap();
    for name in ["before-reply-1.jpg", "after-reply-1.jpg", "after-reply-2.jpg"] {
        std::fs::write(answers.join(name), [0xff, 0xd8]).unwrap();
    }
    let record = json!([
        {"page": "a", "before": "before-reply-1.jpg", "after": "after-reply-1.jpg"},
        {"page": "b", "before": null, "after": "after-reply-2.jpg"},
        {"page": "c", "before": null, "after": null},
    ]);
    std::fs::write(answers.join("pictures-reply.json"), record.to_string()).unwrap();
    std::fs::write(answers.join("after-older-1.jpg"), [0xff, 0xd8]).unwrap();
    std::fs::write(answers.join("pictures-older.json"), json!([{"page": "a", "before": null, "after": "after-older-1.jpg"}]).to_string())
        .unwrap();
    // Records that name nothing kept, or another file, add no answer.
    std::fs::write(answers.join("pictures-empty.json"), json!([{"page": "a", "before": "../secret.jpg", "after": null}]).to_string())
        .unwrap();
    std::fs::write(answers.join("pictures-not json.json"), record.to_string()).unwrap();
    // One read gives every answer of the conversation.
    let all = host.dispatch("answer_pictures", json!({ "id": "c1" })).await.unwrap();
    assert_eq!(
        all,
        json!({
            "older": [{"page": 1, "before": null, "after": "after-older-1.jpg"}],
            "reply": [
                {"page": 1, "before": "before-reply-1.jpg", "after": "after-reply-1.jpg"},
                {"page": 2, "before": null, "after": "after-reply-2.jpg"},
            ],
        })
    );
    // A free read: another conversation's agent reads the names too, never the pictures.
    let agent = Caller::Agent { conversation: Some("c9".into()) };
    assert_eq!(host.dispatch_as(&agent, "answer_pictures", json!({ "id": "c1" })).await.unwrap(), all);
    seed_conversation(&host, "c2", None, None);
    // An empty id would name the attachments root itself: refused before the disk is looked at, as an unsafe one is, even
    // when a record file with no name exists.
    seed_conversation(&host, "", None, None);
    assert!(host.data_dir.join("conversations/.json").is_file());
    std::fs::create_dir_all(host.data_dir.join("attachments/answer-pictures")).unwrap();
    std::fs::copy(answers.join("pictures-reply.json"), host.data_dir.join("attachments/answer-pictures/pictures-reply.json")).unwrap();
    std::fs::copy(answers.join("after-reply-2.jpg"), host.data_dir.join("attachments/answer-pictures/after-reply-2.jpg")).unwrap();
    for id in ["c2", "gone", "../c1", ""] {
        assert_eq!(host.dispatch("answer_pictures", json!({ "id": id })).await.unwrap(), json!({}), "{id}");
        assert_eq!(host.dispatch("site_picture", json!({ "id": id, "call": "after-reply-2.jpg" })).await.unwrap(), Value::Null, "{id}");
    }
    let shown = host.dispatch("site_picture", json!({ "id": "c1", "call": "before-reply-1.jpg" })).await.unwrap();
    assert_eq!(shown, json!("data:image/jpeg;base64,/9g="));
    for call in ["before-reply-3.jpg", "pictures-reply.json", "../answer-pictures/before-reply-1.jpg"] {
        assert_eq!(host.dispatch("site_picture", json!({ "id": "c1", "call": call })).await.unwrap(), Value::Null, "{call}");
    }
}

#[tokio::test]
async fn project_rooms_lists_a_project_folder_and_nothing_for_a_chat() {
    let (host, dir) = host().await;
    let folder = dir.path().join("app");
    std::fs::create_dir_all(folder.join("src/pages")).unwrap();
    std::fs::write(folder.join("src/pages/Login.tsx"), "export function Login() {}\n").unwrap();
    let project = host.engine.add_project(&folder.to_string_lossy()).unwrap();
    let conv: faber_engine::model::Conversation = serde_json::from_value(json!({
        "id": "p1", "title": "p1", "area": "project", "projectId": project.id, "createdAt": 1, "updatedAt": 2, "messages": []
    }))
    .unwrap();
    faber_engine::store::Store::new(&host.data_dir).save(&conv);
    let rooms = host.dispatch("project_rooms", json!({ "id": "p1" })).await.unwrap();
    assert_eq!(rooms, json!([{"name": "Login", "kind": "screen", "at": "src/pages/Login.tsx", "recent": [], "touched": false}]));
    seed_conversation(&host, "c1", None, None);
    for id in ["c1", "gone", "../p1"] {
        assert_eq!(host.dispatch("project_rooms", json!({ "id": id })).await.unwrap(), json!([]), "{id}");
    }
}

#[cfg(not(faber_telemetry))]
#[test]
fn sharing_commands_are_absent_without_an_endpoint() {
    for name in ["sharing_state", "sharing_set", "sharing_preview", "sharing_language"] {
        assert!(super::Host::commands().iter().all(|entry| entry.name != name));
        assert!(super::Host::reach(name).is_none());
    }
}

#[cfg(faber_telemetry)]
#[test]
fn sharing_commands_are_screen_only_with_an_endpoint() {
    for name in ["sharing_state", "sharing_set", "sharing_preview", "sharing_language"] {
        assert_eq!(matches!(super::Host::reach(name), Some(super::Reach::Screen)), faber_engine::sharing::endpoint().is_some());
    }
}

/// R585: the agent's `faber` tool, as the engine holds it (`Host::agent_commands`).
mod faber_tool {
    use super::*;
    use faber_engine::commands::Scope;

    const CHAT: Scope = Scope { read_only: false, project: false };
    const PROJECT: Scope = Scope { read_only: false, project: true };
    const SCOPES: [Scope; 4] = [CHAT, PROJECT, Scope { read_only: true, project: false }, Scope { read_only: true, project: true }];

    fn names(found: &[Value]) -> Vec<&str> {
        found.iter().map(|command| command["name"].as_str().unwrap()).collect()
    }

    #[test]
    fn find_answers_a_command_by_its_words_and_never_one_the_agent_could_not_run() {
        let found = agent_commands::find("rename a conversation", CHAT);
        assert_eq!(names(&found).first(), Some(&"rename"), "{found:?}");
        assert!(found.len() <= faber_engine::commands::MAX_FOUND);
        assert!(found[0]["inputSchema"]["properties"]["title"].is_object() && found[0]["about"].is_string());
        assert_eq!(found[0]["kind"], "write");
        let archive = agent_commands::find("archive", CHAT);
        assert!(archive[0]["inputSchema"]["properties"].get("by").is_none(), "the host sets by itself");
        assert!(agent_commands::find("", PROJECT).is_empty() && agent_commands::find("zzqx", PROJECT).is_empty());
        // Words that name what only the person or the screen does find nothing of theirs.
        for words in ["sign in", "approve credits", "answer permission", "open state", "cookie import", "edit project memory", "branch"] {
            for command in agent_commands::find(words, PROJECT) {
                let reach = Host::reach(command["name"].as_str().unwrap()).unwrap();
                assert!(!matches!(reach, Reach::Person | Reach::Screen), "{words}: {command}");
            }
        }
        assert!(agent_commands::offered(PROJECT).all(|command| !matches!(command.reach, "person" | "screen")));
        assert!(!names(&agent_commands::find("project goals", CHAT)).iter().any(|name| name.starts_with("project_")));
        assert!(names(&agent_commands::find("project goals", PROJECT)).contains(&"project_goals"));
        assert!(!names(&agent_commands::find("setup check", SCOPES[3])).contains(&"setup_check"), "a read in name only");
    }

    /// `find` and `run` apply one rule: whatever `find` would never list, `run` refuses with the rule's own key, in every
    /// scope, before the host is asked.
    #[tokio::test]
    async fn find_never_lists_what_run_would_refuse() {
        let (host, _dir) = host().await;
        let host = Arc::new(host);
        let commands = Host::agent_commands(&host);
        let eligible = |name: &Value, scope| Host::commands().iter().any(|c| c.name == *name && agent_commands::eligible(c, scope).is_ok());
        for scope in SCOPES {
            for command in Host::commands() {
                let listed = agent_commands::find(&command.name.replace('_', " "), scope);
                assert!(listed.iter().all(|found| eligible(&found["name"], scope)), "{scope:?} {}: {listed:?}", command.name);
                if let Err(key) = agent_commands::eligible(command, scope) {
                    assert!(!names(&listed).contains(&command.name), "{scope:?} lists {}", command.name);
                    let refused = commands.run("c1".into(), command.name.into(), json!({}), scope).await;
                    assert_eq!(refused, Err(format!("\"{key}\"")), "{scope:?} {}", command.name);
                }
            }
            let send = Host::commands().iter().find(|c| c.name == "send").unwrap();
            assert_eq!(agent_commands::eligible(send, scope), Err("error.session.useSession"));
        }
        for name in agent_commands::READS_IN_NAME_ONLY.iter().chain(agent_commands::PROJECT_ONLY) {
            assert!(Host::kind(name).is_some(), "{name} is not in the table");
        }
        host.engine.shutdown();
    }

    #[tokio::test]
    async fn run_reaches_a_read_and_refuses_the_unknown_through_the_engines_handle() {
        let (host, _dir) = host().await;
        let host = Arc::new(host);
        let commands = Host::agent_commands(&host);
        assert_eq!(commands.run("c1".into(), "projects".into(), json!({}), SCOPES[2]).await, Ok(json!([])));
        let settings = commands.run("c1".into(), "settings".into(), json!({}), CHAT).await.unwrap();
        assert!(settings.is_object());
        for unknown in ["no_such_command", "hello", ""] {
            assert_eq!(commands.run("c1".into(), unknown.into(), json!({}), PROJECT).await, Err("\"error.host.unknownCommand\"".into()));
        }
        host.engine.shutdown();
    }

    /// The owner's line (R585): signing in, money and the agent's own permissions, trust and hooks are the person's.
    #[tokio::test]
    async fn the_vault_stays_with_the_person_through_the_tool() {
        let (host, _dir) = host().await;
        let host = Arc::new(host);
        let commands = Host::agent_commands(&host);
        let person = Err("\"error.reach.person\"".to_string());
        let run = |command: &str, args: Value| commands.run("c1".into(), command.into(), args, PROJECT);
        // Signing in.
        assert_eq!(run("setup_sign_in", json!({"vendor": "claude"})).await, person);
        assert_eq!(run("cookie_import", json!({"id": "c1", "ticket": "t", "sites": []})).await, person);
        // Money: paying past the plan, and every turn started outside the conversation's own.
        assert_eq!(run("approve_credits", json!({"id": "c1", "turnId": "t1", "vendor": "claude"})).await, person);
        let branch = json!({"req": {"conversationId": "c1", "messageId": "m1", "text": "again", "choice": {"vendor": "auto"}}});
        assert_eq!(run("branch", branch).await, person);
        assert_eq!(run("branch_here", json!({"conversationId": "c1", "messageId": "m1"})).await, person);
        assert_eq!(run("follow_up", json!({"id": "c1", "file": "a.md", "dismiss": false})).await, person);
        assert_eq!(run("ask_aside", json!({"id": "c1", "question": "why?"})).await, person);
        assert_eq!(run("edit_queued", json!({"id": "c1", "turnId": "t1", "text": "more"})).await, person);
        assert_eq!(run("send_queued", json!({"id": "c1", "turnId": "t1"})).await, person);
        // A project's own switches: opening an environment, separate copies, fixes that start by themselves.
        assert_eq!(run("start_environment", json!({"id": "e1"})).await, person);
        assert_eq!(run("project_copies", json!({"projectId": "p1", "on": true})).await, person);
        assert_eq!(run("project_goals_auto", json!({"projectId": "p1", "on": true})).await, person);
        // Its own permissions: a card's answer, the mode, a blocked site, a kept "never touch" rule.
        let card = json!({"id": "c1", "turnId": "t1", "requestId": "r1", "allow": true, "always": true});
        assert_eq!(run("answer", card).await, person);
        assert_eq!(run("save_settings", json!({"patch": {"permissions": "dontAsk"}})).await, person);
        assert_eq!(run("save_settings", json!({"patch": {"blockedHosts": []}})).await, person);
        let edit = json!({"projectId": "p1", "edit": {"path": ".faber/decisions.md", "revision": "r", "start": 0, "end": 1, "keep": true}});
        assert_eq!(run("edit_project_memory", edit).await, person);
        // Trust.
        assert_eq!(run("save_settings", json!({"patch": {"faberTrust": true}})).await, person);
        // Hooks.
        assert_eq!(run("save_settings", json!({"patch": {"userHooks": true}})).await, person);
        assert!(lock(&host.asks).pending().is_empty(), "no request is recorded for what the person alone decides");
        // Tool commands stay the person's to approve one by one.
        let asked: Value = serde_json::from_str(&run("tools_remove", json!({"id": "x"})).await.unwrap_err()).unwrap();
        assert_eq!(asked["key"], "error.reach.ask");
        host.engine.shutdown();
    }

    #[tokio::test]
    async fn plan_runs_only_reads_and_an_ask_comes_back_with_its_request() {
        let (host, _dir) = host().await;
        seed_conversation(&host, "c1", None, None);
        let host = Arc::new(host);
        let commands = Host::agent_commands(&host);
        let rename = json!({"id": "c1", "title": "Renamed"});
        let refused = commands.run("c1".into(), "rename".into(), rename.clone(), SCOPES[2]).await;
        assert_eq!(refused, Err("\"error.reach.readOnly\"".into()));
        assert_ne!(host.engine.state().conversations.iter().find(|c| c.id == "c1").unwrap().title, "Renamed");
        let refused = commands.run("c1".into(), "setup_check".into(), json!({}), SCOPES[2]).await;
        assert_eq!(refused, Err("\"error.reach.readOnly\"".into()), "a read in name only runs a program");
        assert!(commands.run("c1".into(), "rename".into(), rename, CHAT).await.is_ok(), "outside Plan the same write runs");
        let asked = commands.run("c1".into(), "forget_recent".into(), json!({"path": "C:/x"}), CHAT).await.unwrap_err();
        let asked: Value = serde_json::from_str(&asked).unwrap();
        assert_eq!(asked["key"], "error.reach.ask");
        assert!(asked["data"]["request"].is_string(), "the person's request is named: {asked}");
        host.engine.shutdown();
    }

    /// An agent reads its own family's background commands only, as its app renderer does; the screen reads them all.
    #[tokio::test]
    async fn background_commands_from_an_agent_are_its_familys_only() {
        let command = |id: &str| faber_engine::background::Command {
            id: id.into(),
            conversation_id: id.into(),
            vendor: faber_engine::model::Vendor::Codex,
            command: "private command".into(),
            started: 1,
            output: format!("{id} output"),
            state: faber_engine::background::State::Running,
            stoppable: true,
        };
        let family = |id: &str| match id {
            "lead" | "child" => Some("lead".to_string()),
            "other" => Some("other".to_string()),
            _ => None,
        };
        let all = faber_engine::background::List { revision: 1, items: ["lead", "child", "other"].map(command).to_vec() };
        for (by, expected) in
            [(Some("child"), vec!["lead", "child"]), (Some("other"), vec!["other"]), (Some("gone"), vec![]), (None, vec![])]
        {
            let mut list = all.clone();
            keep_family(&mut list, by, family);
            assert_eq!(list.items.iter().map(|i| i.conversation_id.as_str()).collect::<Vec<_>>(), expected, "{by:?}");
        }
        // Through the tool: the engine has none of this agent's family, and the screen reads the list unfiltered.
        let (host, _dir) = host().await;
        seed_conversation(&host, "c1", None, None);
        let host = Arc::new(host);
        let mine = Host::agent_commands(&host).run("c1".into(), "background_commands".into(), json!({}), CHAT).await.unwrap();
        assert_eq!(mine["items"], json!([]));
        let screen = host.dispatch_as(&Caller::Screen, "background_commands", json!({})).await.unwrap();
        assert!(screen["items"].is_array());
        host.engine.shutdown();
    }
}
