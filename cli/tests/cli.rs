//! The `faber` program against a Faber of its own: a fake command host served on the real channel of a temporary data
//! folder. Each test runs the built program as a person or an agent would, and reads what it prints and how it exits.

use faber_channel::server::Answer;
use faber_channel::{Backend, serve};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

struct Fake {
    events: broadcast::Sender<Value>,
    calls: Mutex<Vec<(String, Value)>>,
    /// The reply to `send` asks the person a question instead of answering.
    asks: bool,
}

fn state() -> Value {
    json!({
        "version": "9.9.9",
        "state": {
            "conversations": [
                { "id": "c1", "title": "Old one", "area": "chat", "updatedAt": 1000 },
                { "id": "c2", "title": "New one", "area": "chat", "updatedAt": 2000 },
                { "id": "c3", "title": "Archived", "area": "chat", "updatedAt": 3000, "archived": true }
            ],
            "projects": [{ "id": "p1", "name": "Site", "path": "C:/site" }],
            "settings": { "language": "en" },
            "running": { "revision": 1, "items": [] },
            "quotas": [{ "vendor": "claude", "window": "five_hour", "used": 0.42, "resetsAt": 0 }],
            "usage": []
        }
    })
}

impl Backend for Fake {
    fn version(&self) -> String {
        "9.9.9".into()
    }
    fn commands(&self) -> Value {
        json!([{ "name": "send", "args": [["req", "SendRequest"]], "kind": "StartsTurn" }, { "name": "state", "args": [], "kind": "Read" }])
    }
    fn starts_turn(&self, cmd: &str) -> bool {
        cmd == "send"
    }
    fn dispatch(&self, cmd: String, args: Value) -> Answer {
        self.calls.lock().unwrap().push((cmd.clone(), args.clone()));
        let (events, asks) = (self.events.clone(), self.asks);
        Box::pin(async move {
            match cmd.as_str() {
                "state" => Ok(state()),
                "setup_check" => Ok(json!({ "vendor": args["vendor"], "installed": true, "account": "signedIn", "canInstall": false })),
                "update_status" => Ok(json!({ "phase": "idle" })),
                "open" => {
                    Ok(json!({ "id": args["id"], "title": "Hi", "messages": [{ "id": "u1", "role": "user", "text": "hi", "at": 1 }] }))
                }
                "send" => {
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        let at = now_ms();
                        let message = |text: &str, status: &str| {
                            json!({ "type": "message", "conversationId": "c-new", "message": {
                                "id": "a1", "role": "assistant", "text": text, "at": at, "status": status, "vendor": "claude",
                                "tools": [{ "id": "t1", "name": "Read", "input": { "file_path": "a.rs" }, "ok": true }]
                            } })
                        };
                        if asks {
                            let _ = events.send(json!({ "type": "running", "revision": 2, "items": [{
                                "conversationId": "c-new", "turnId": "a1", "state": "needsYou",
                                "permissions": [{ "requestId": "r1", "tool": "AskUserQuestion", "input": { "questions": [] } }]
                            }] }));
                            return;
                        }
                        let _ = events.send(message("Hel", "streaming"));
                        let _ = events.send(json!({ "type": "message", "conversationId": "c-new", "message": {
                            "id": "a1", "role": "assistant", "text": "Hello", "at": at, "status": "done", "vendor": "claude",
                            "toolsFrom": 0, "tools": [{ "id": "t1", "same": true }]
                        } }));
                    });
                    Ok(json!("c-new"))
                }
                "refuse" => Err("error.test.refused".into()),
                _ => Ok(json!({ "cmd": cmd, "args": args })),
            }
        })
    }
    fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Serves a fake Faber on a new data folder and runs `faber` with `args` (and `stdin`) against it.
async fn faber(asks: bool, args: &[&str], stdin: &str, env: &[(&str, &str)]) -> (Run, Arc<Fake>) {
    let data = tempfile::tempdir().unwrap();
    let (events, _) = broadcast::channel(64);
    let fake = Arc::new(Fake { events, calls: Mutex::default(), asks });
    let served = tokio::spawn({
        let (dir, fake) = (data.path().to_path_buf(), fake.clone());
        async move { serve(&dir, fake).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let run = run(data.path(), args, stdin, env).await;
    served.abort();
    (run, fake)
}

async fn run(data: &Path, args: &[&str], stdin: &str, env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_faber-cli"));
    command.args(args).arg("--data").arg(data).arg("--no-start");
    command.env_remove("FABER_AGENT_DEPTH").env("NO_COLOR", "1");
    for (name, value) in env {
        command.env(name, value);
    }
    let stdin = stdin.to_string();
    tokio::task::spawn_blocking(move || {
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
        let out = child.wait_with_output().unwrap();
        Run {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into(),
            stderr: String::from_utf8_lossy(&out.stderr).into(),
        }
    })
    .await
    .unwrap()
}

fn lines(text: &str) -> Vec<Value> {
    text.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("not JSON: {l}"))).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn status_says_whether_each_ai_is_signed_in_without_anyones_screen() {
    let (run, _) = faber(false, &["status", "--json"], "", &[]).await;
    assert_eq!(run.code, 0, "{}", run.stderr);
    let status: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(status["version"], "9.9.9");
    assert_eq!(status["accounts"][0]["account"], "signedIn");
    assert_eq!(status["accounts"][1]["vendor"], "codex");
    assert_eq!(status["conversations"], 3);

    let (run, _) = faber(false, &["status"], "", &[]).await;
    assert!(run.stdout.contains("Claude Code") && run.stdout.contains("signed in") && run.stdout.contains("42%"), "{}", run.stdout);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_shows_the_newest_first_and_hides_the_archived() {
    let (run, _) = faber(false, &["list", "--json"], "", &[]).await;
    let ids: Vec<String> =
        serde_json::from_str::<Value>(&run.stdout).unwrap().as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap().into()).collect();
    assert_eq!(ids, ["c2", "c1"]);
    let (run, _) = faber(false, &["list", "--archived", "--json"], "", &[]).await;
    assert_eq!(serde_json::from_str::<Value>(&run.stdout).unwrap()[0]["id"], "c3");
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_streams_the_answer_and_ends_with_its_result() {
    let (run, fake) = faber(false, &["chat", "hi there", "--vendor", "claude", "--json"], "", &[]).await;
    assert_eq!(run.code, 0, "{}{}", run.stdout, run.stderr);
    let lines = lines(&run.stdout);
    let text: String = lines.iter().filter(|l| l["type"] == "text").map(|l| l["delta"].as_str().unwrap()).collect();
    assert_eq!(text, "Hello");
    assert!(lines.iter().any(|l| l["type"] == "step" && l["state"] == "done"), "{lines:?}");
    let result = lines.last().unwrap();
    assert_eq!(
        (result["type"].as_str(), result["status"].as_str(), result["text"].as_str()),
        (Some("result"), Some("done"), Some("Hello"))
    );
    let calls = fake.calls.lock().unwrap();
    let send = &calls.iter().find(|(cmd, _)| cmd == "send").unwrap().1;
    assert_eq!(send["req"]["text"], "hi there");
    assert_eq!(send["req"]["choice"]["vendor"], "claude");
    assert_eq!(send["req"]["area"], "chat");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_with_nobody_at_the_terminal_is_handed_back_and_waits() {
    let (run, _) = faber(true, &["chat", "hi", "--json"], "", &[]).await;
    assert_eq!(run.code, 4, "{}{}", run.stdout, run.stderr);
    let card = lines(&run.stdout).pop().unwrap();
    assert_eq!((card["type"].as_str(), card["turnId"].as_str()), (Some("needsYou"), Some("a1")));
    assert_eq!(card["permission"]["requestId"], "r1");
}

#[tokio::test(flavor = "multi_thread")]
async fn call_reaches_any_command_and_a_refusal_keeps_its_key() {
    let (run, fake) = faber(false, &["call", "rename", r#"{"id":"c1","title":"New"}"#], "", &[]).await;
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(fake.calls.lock().unwrap().iter().any(|(cmd, args)| cmd == "rename" && args["title"] == "New"));
    let (run, _) = faber(false, &["call", "refuse", "--json"], "", &[]).await;
    assert_eq!(run.code, 1);
    assert_eq!(serde_json::from_str::<Value>(&run.stdout).unwrap()["error"], "error.test.refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ai_deep_in_faber_reads_but_cannot_start_a_turn() {
    let (run, _) = faber(false, &["chat", "again", "--json"], "", &[("FABER_AGENT_DEPTH", "2")]).await;
    assert_eq!(run.code, 1, "{}", run.stdout);
    assert_eq!(serde_json::from_str::<Value>(&run.stdout).unwrap()["error"], "error.channel.agentDepth");
    let (run, _) = faber(false, &["list", "--json"], "", &[("FABER_AGENT_DEPTH", "2")]).await;
    assert_eq!(run.code, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_use_and_no_faber_have_their_own_exit_codes() {
    let (run, _) = faber(false, &["fly"], "", &[]).await;
    assert_eq!(run.code, 2);
    assert!(run.stderr.contains("fly"), "{}", run.stderr);
    let (run, _) = faber(false, &["status", "--colour"], "", &[]).await;
    assert_eq!(run.code, 2);
    let empty = tempfile::tempdir().unwrap();
    let alone = run_alone(empty.path()).await;
    assert_eq!(alone.code, 3, "{}", alone.stderr);
}

async fn run_alone(data: &Path) -> Run {
    run(data, &["status"], "", &[]).await
}

#[tokio::test(flavor = "multi_thread")]
async fn rpc_carries_the_channel_on_stdin_and_stdout() {
    let input = concat!(r#"{"id":1,"cmd":"hello","args":{}}"#, "\n", r#"{"id":2,"cmd":"state"}"#, "\n", r#"{"id":3,"cmd":"refuse"}"#, "\n");
    let (run, _) = faber(false, &["rpc"], input, &[]).await;
    assert_eq!(run.code, 0, "{}", run.stderr);
    let mut replies = lines(&run.stdout);
    replies.sort_by_key(|r| r["id"].as_u64());
    assert_eq!(replies[0]["ok"]["version"], "9.9.9");
    assert_eq!(replies[1]["ok"]["state"]["projects"][0]["id"], "p1");
    assert_eq!(replies[2]["err"], "error.test.refused");
}

#[tokio::test(flavor = "multi_thread")]
async fn mcp_lists_its_tools_and_answers_a_status_call() {
    let input = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "faber_status", "arguments": {} } }),
    ]
    .iter()
    .map(|m| format!("{m}\n"))
    .collect::<String>();
    let (run, _) = faber(false, &["mcp"], &input, &[]).await;
    assert_eq!(run.code, 0, "{}", run.stderr);
    let mut replies = lines(&run.stdout);
    replies.sort_by_key(|r| r["id"].as_u64());
    assert_eq!(replies.len(), 3, "the notification gets no answer: {replies:?}");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "faber");
    assert!(replies[1]["result"]["tools"].as_array().unwrap().iter().any(|t| t["name"] == "faber_chat"));
    let status: Value = serde_json::from_str(replies[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(status["accounts"][0]["account"], "signedIn");
}
