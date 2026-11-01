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
