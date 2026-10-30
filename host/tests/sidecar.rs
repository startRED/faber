//! The sidecar process end to end: requests in on stdin are answered by id on stdout, the engine's events come as lines of
//! their own, and closing stdin ends the process cleanly. It runs with no CLI and a data folder of its own, hidden.

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
        match value.get("event") {
            Some(event) => events.push(event.clone()),
            None => {
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
