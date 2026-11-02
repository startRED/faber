use super::*;
use serde_json::json;
use std::collections::BTreeMap;

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
    for screen_only in ["state", "open", "screen_ready", "site_open"] {
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
    assert_eq!(key(host.dispatch("open_link", json!({ "url": "https://example.com" })).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("open_link", json!({ "url": "file:///C:/Windows" })).await), "error.link.web");
    assert_eq!(key(host.dispatch("add_project", Value::Null).await), NO_WINDOW);
    assert_eq!(key(host.dispatch("set_shell_texts", json!({ "texts": {} })).await), NO_WINDOW);
    assert_eq!(host.dispatch("update_status", Value::Null).await.unwrap(), json!({ "phase": "none" }));
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

fn seed_conversation(host: &Host, id: &str, started_by: Option<&str>, waiting_on: Option<faber_engine::model::WaitingOn>) {
    let mut conv: faber_engine::model::Conversation =
        serde_json::from_value(json!({ "id": id, "title": id, "area": "chat", "createdAt": 1, "updatedAt": 2, "messages": [] })).unwrap();
    conv.started_by = started_by.map(String::from);
    conv.waiting_on = waiting_on;
    faber_engine::store::Store::new(&host.data_dir).save(&conv);
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
async fn archive_and_interrupt_belong_to_an_agents_own_family_and_leave_the_person_and_screen_unchanged() {
    assert_eq!(Host::reach("archive"), Some(Reach::Own("/id")));
    assert_eq!(Host::reach("interrupt"), Some(Reach::Own("/id")));
    let (host, _dir) = host().await;
    seed_conversation(&host, "c1", None, None);
    seed_conversation(&host, "c2", Some("c1"), None);
    seed_conversation(&host, "c9", None, None);

    let agent = Caller::Agent { conversation: Some("c1".into()) };
    assert_eq!(key(host.dispatch_as(&agent, "archive", json!({ "id": "c9", "on": true })).await), "error.reach.ask");
    assert_eq!(key(host.dispatch_as(&agent, "interrupt", json!({ "id": "c9" })).await), "error.reach.ask");
    // Within its own family, reach allows the call and the engine's session verb runs.
    assert_ne!(key(host.dispatch_as(&agent, "archive", json!({ "id": "c2", "on": true })).await), "error.reach.ask");

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
        Some(faber_engine::model::WaitingOn { ids: vec!["c2".into()], note: "waiting for child".into(), since: 5 }),
    );
    seed_conversation(&host, "c2", Some("c1"), None);

    let rows = host.dispatch("conversations", Value::Null).await.unwrap();
    let list = rows.as_array().unwrap();
    let r1 = list.iter().find(|r| r["id"] == "c1").unwrap();
    assert_eq!(r1["state"], "waiting");
    assert_eq!(r1["waitingOn"], json!({ "ids": ["c2"], "note": "waiting for child" }));
    let r2 = list.iter().find(|r| r["id"] == "c2").unwrap();
    assert_eq!(r2["state"], "idle");
    assert!(r2.get("waitingOn").is_none(), "{r2}");

    let c1 = host.dispatch("conversation", json!({ "id": "c1" })).await.unwrap();
    assert_eq!(c1["state"], "waiting");
    assert_eq!(c1["waitingOn"], json!({ "ids": ["c2"], "note": "waiting for child" }));
    let c2 = host.dispatch("conversation", json!({ "id": "c2" })).await.unwrap();
    assert_eq!(c2["state"], "new");
    assert!(c2.get("waitingOn").is_none(), "{c2}");
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
