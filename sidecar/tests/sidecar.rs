//! The sidecar process end to end: requests in on stdin are answered by id on stdout, the engine's events come as lines of
//! their own, the shell's orders are lines of their own too, and closing stdin ends the process cleanly. It runs with no CLI
//! and a data folder of its own, hidden.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn the_sidecar_answers_by_id_streams_events_and_ends_cleanly_when_stdin_closes() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_faber-sidecar"));
    command
        .env("FABER_DATA_DIR", dir.path())
        .env("FABER_HIDDEN", "1")
        // An empty override means no CLI of that name: the test never starts a real one.
        .env("FABER_CLI_CLAUDE", "")
        .env("FABER_CLI_CODEX", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window of its own, whoever runs the test.
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let (lines, heard) = mpsc::channel::<String>();
    let reader = std::thread::spawn({
        let stdout = child.stdout.take().unwrap();
        move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if lines.send(line).is_err() {
                    break;
                }
            }
        }
    });

    for request in [
        r#"{"id":1,"cmd":"create_chat_group","args":{"name":"Taxes"}}"#,
        r#"{"id":2,"cmd":"no_such_command"}"#,
        "this is not a request",
        r#"{"id":3,"cmd":"screen_ready"}"#,
        r#"{"id":4,"cmd":"open","args":{"id":7}}"#,
    ] {
        writeln!(stdin, "{request}").unwrap();
    }
    stdin.flush().unwrap();

    // Replies may come out of order, and events are lines of their own: read until all five replies and a list event are in.
    let (mut replies, mut events) = (std::collections::BTreeMap::new(), Vec::new());
    while replies.len() < 5 || events.is_empty() {
        let line = heard.recv_timeout(Duration::from_secs(60)).expect("the sidecar went quiet");
        let value: Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("not one JSON object per line: {line}"));
        match (value.get("event"), value.get("order")) {
            (Some(event), _) => events.push(event.clone()),
            // The shell's orders (the tray, ...) are lines of their own, and answer nothing.
            (None, Some(_)) => {}
            (None, None) => {
                replies.insert(value["id"].as_u64().unwrap(), value);
            }
        }
    }
    assert_eq!(replies[&1]["ok"]["name"], "Taxes");
    assert_eq!(replies[&2]["err"], json!("error.host.unknownCommand"));
    assert_eq!(replies[&0]["err"], json!("error.host.badRequest"), "a line that is no request is answered under id 0");
    assert_eq!(replies[&3]["err"], json!("error.host.noWindow"));
    assert_eq!(replies[&4]["err"], json!("error.host.badArgs"));
    assert!(events.iter().any(|event| event["type"] == "list"), "{events:?}");

    // One line past the limit is refused by key and thrown away; the next request is read and answered as if nothing happened.
    let huge = vec![b'x'; faber_host::wire::MAX_LINE + 4096];
    stdin.write_all(&huge).unwrap();
    writeln!(stdin).unwrap();
    writeln!(stdin, r#"{{"id":5,"cmd":"screen_ready"}}"#).unwrap();
    stdin.flush().unwrap();
    let (mut refused, mut answered) = (false, false);
    while !(refused && answered) {
        let line = heard.recv_timeout(Duration::from_secs(60)).expect("the sidecar went quiet after a long line");
        let Ok(value) = serde_json::from_str::<Value>(&line) else { panic!("not one JSON object per line") };
        match value["id"].as_u64() {
            Some(0) => refused = value["err"] == json!("error.host.badRequest"),
            Some(5) => answered = value["err"] == json!("error.host.noWindow"),
            _ => {}
        }
    }

    // A line of exactly the limit fits with its break, so the request after it is not taken for the rest of it.
    stdin.write_all(&vec![b'x'; faber_host::wire::MAX_LINE]).unwrap();
    writeln!(stdin).unwrap();
    writeln!(stdin, r#"{{"id":6,"cmd":"screen_ready"}}"#).unwrap();
    stdin.flush().unwrap();
    loop {
        let line = heard.recv_timeout(Duration::from_secs(60)).expect("the request after a line of exactly the limit was lost");
        if serde_json::from_str::<Value>(&line).is_ok_and(|value| value["id"] == 6) {
            break;
        }
    }

    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success(), "closing stdin is a clean end: {status}");
    reader.join().unwrap();
}

/// Reads lines until `wanted` accepts one, failing if the sidecar goes quiet.
fn read_until(heard: &mpsc::Receiver<String>, what: &str, mut wanted: impl FnMut(&Value) -> bool) -> Value {
    loop {
        let line = heard.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| panic!("the sidecar went quiet before {what}"));
        let value: Value = serde_json::from_str(&line).unwrap_or_else(|_| panic!("not one JSON object per line: {line}"));
        if wanted(&value) {
            return value;
        }
    }
}

#[test]
fn the_sidecar_orders_the_tray_in_the_words_the_screen_sent_and_reports_what_the_shell_told_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_faber-sidecar"));
    command
        .env("FABER_DATA_DIR", dir.path())
        .env("FABER_HIDDEN", "1")
        .env("FABER_CLI_CLAUDE", "")
        .env("FABER_CLI_CODEX", "")
        // What only the shell knows reaches the sidecar as environment variables.
        .env("FABER_VERSION", "9.8.7")
        .env("FABER_AUTOSTART", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let (lines, heard) = mpsc::channel::<String>();
    let reader = std::thread::spawn({
        let stdout = child.stdout.take().unwrap();
        move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if lines.send(line).is_err() {
                    break;
                }
            }
        }
    });

    // The tray is ordered once the engine is there, in English before any window has sent words.
    let first = read_until(&heard, "the first tray order", |line| line["order"]["kind"] == "tray");
    assert_eq!(first["order"]["tooltip"], "Faber");
    assert_eq!(first["order"]["needsYou"], 0);
    assert_eq!(first["order"]["menu"], json!({ "show": "Open Faber", "quit": "Quit Faber" }));
    for picture in ["picture", "picture16"] {
        let png = first["order"][picture].as_str().expect("a base64 picture");
        assert!(png.starts_with("iVBORw0KGgo"), "{picture} is a PNG: {png}");
    }

    let words = json!({
        "texts": {
            "open": "Abrir", "quit": "Sair",
            "running": { "one": "Faber: {n} em andamento", "other": "Faber: {n} em andamento" },
            "needsYou": { "one": ", {n} precisa de você", "other": ", {n} precisam de você" },
            "notify": "{title} precisa de você"
        }
    });
    writeln!(stdin, r#"{{"id":1,"cmd":"set_shell_texts","args":{words}}}"#).unwrap();
    writeln!(stdin, r#"{{"id":2,"cmd":"state"}}"#).unwrap();
    stdin.flush().unwrap();
    let (mut answered, mut state, mut menu) = (false, None, None);
    while !(answered && state.is_some() && menu.is_some()) {
        let line = read_until(&heard, "the answers and the tray order for the new words", |_| true);
        if line["order"]["kind"] == "tray" {
            menu = Some(line["order"]["menu"].clone());
        } else if line["id"] == 1 {
            assert_eq!(line["ok"], Value::Null, "{line}");
            answered = true;
        } else if line["id"] == 2 {
            state = Some(line);
        }
    }
    assert_eq!(menu.unwrap(), json!({ "show": "Abrir", "quit": "Sair" }));
    let state = state.unwrap();
    assert_eq!(state["ok"]["version"], "9.8.7", "the version is the shell's");
    assert_eq!(state["ok"]["settings"]["autostart"], true, "the switch is the shell's registration");
    let kept = std::fs::read_to_string(dir.path().join("shell-texts.json")).unwrap();
    assert!(kept.contains("Abrir"), "the words are kept in the data folder: {kept}");

    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success(), "closing stdin is a clean end: {status}");
    reader.join().unwrap();
}
