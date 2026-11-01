//! The `faber` program against a Faber of its own: a fake command host served on the real channel of a temporary data
//! folder. Each test runs the built program as a person or an agent would, and reads what it prints and how it exits.

use faber_channel::server::Answer;
use faber_channel::{Backend, Who, serve};
use serde_json::{Value, json};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::sync::broadcast;

struct Fake {
    events: broadcast::Sender<Value>,
    calls: Mutex<Vec<(String, Value)>>,
    asks: bool,
    working: Arc<AtomicBool>,
}
impl Backend for Fake {
    fn version(&self) -> String {
        "9.9.9".into()
    }
    fn commands(&self) -> Value {
        json!([])
    }
    fn starts_turn(&self, cmd: &str) -> bool {
        cmd == "send"
    }
    fn dispatch(&self, _who: &Who, cmd: String, args: Value) -> Answer {
        self.calls.lock().unwrap().push((cmd.clone(), args.clone()));
        let working = self.working.load(Ordering::SeqCst);
        let finish = self.working.clone();
        let asks = self.asks;
        Box::pin(async move {
            match cmd.as_str() {
                "conversations" => {
                    Ok(json!([{ "id":"c1", "title":"Example", "family":"c1", "state":if working {"working"} else {"idle"} }]))
                }
                "conversation" => {
                    Ok(json!({"id":args["id"],"state":if working {"working"} else {"concluded"},"conclusion":{"recap":"Done","done":true}}))
                }
                "send" => {
                    if !asks {
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            finish.store(false, Ordering::SeqCst);
                        });
                    }
                    Ok(json!("c1"))
                }
                "refuse" => Err("error.test.refused".into()),
                "ask" => Err("error.reach.ask".into()),
                _ => Ok(json!({"cmd":cmd,"args":args})),
            }
        })
    }
    fn dispatch_detailed(&self, who: &Who, cmd: String, args: Value) -> faber_channel::server::DetailedAnswer {
        if cmd == "detailed_refusal" {
            return Box::pin(async {
                Err(faber_channel::wire::Error { key: "error.test.refused".into(), detail: Some("The request cannot go ahead.".into()) })
            });
        }
        let answer = self.dispatch(who, cmd, args);
        Box::pin(async move { answer.await.map_err(Into::into) })
    }
    fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn refusal_detail_reaches_json_and_human_output() {
    let (json, _) = faber(false, &["call", "detailed_refusal", "--json"], "", &[]).await;
    assert_eq!(json.code, 1);
    assert_eq!(lines(&json.stdout)[0]["error"], "error.test.refused");
    assert_eq!(lines(&json.stdout)[0]["detail"], "The request cannot go ahead.");
    let (human, _) = faber(false, &["call", "detailed_refusal", "--human"], "", &[]).await;
    assert_eq!(human.code, 1);
    assert!(human.stderr.contains("The request cannot go ahead."));
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
    let fake = Arc::new(Fake { events, calls: Mutex::default(), asks, working: Arc::new(AtomicBool::new(true)) });
    let served = tokio::spawn({
        let (dir, fake) = (data.path().to_path_buf(), fake.clone());
        async move { serve(&dir, fake).await }
    });
    ready(data.path()).await;
    let run = run(data.path(), args, stdin, env).await;
    served.abort();
    (run, fake)
}

async fn ready(data: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while faber_channel::Client::connect(data, Default::default()).await.is_err() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fake Faber did not start");
}

async fn run(data: &Path, args: &[&str], stdin: &str, env: &[(&str, &str)]) -> Run {
    let mut command = Command::new(env!("CARGO_BIN_EXE_faber-cli"));
    command.arg("--data").arg(data).arg("--no-start").args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
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
async fn piped_output_and_list_arguments() {
    let (run, fake) = faber(
        false,
        &["list", "--state", "working,waiting", "--all", "--project", "p1", "--family", "c1", "--query", "site", "--limit", "3"],
        "",
        &[],
    )
    .await;
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(serde_json::from_str::<Value>(&run.stdout).unwrap().is_array());
    let calls = fake.calls.lock().unwrap();
    assert_eq!(
        calls.iter().find(|(c, _)| c == "conversations").unwrap().1,
        json!({"state":["working","waiting"],"archived":false,"projectId":"p1","family":"c1","query":"site","limit":3})
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn session_verbs_send_the_host_their_arguments() {
    for (args, stdin, command, expected) in [
        (
            vec!["start", "Work", "-", "--ai", "codex", "--effort", "high", "--keep-going"],
            "brief",
            "start_conversation",
            json!({"title":"Work","brief":"brief","ai":"codex","model":null,"effort":"high","step":null,"keepGoing":true}),
        ),
        (vec!["message", "c1", "hello"], "", "message_conversation", json!({"id":"c1","message":"hello"})),
        (vec!["resume", "c1"], "", "resume_conversation", json!({"id":"c1","message":null})),
        (vec!["mark-waiting", "--on", "c1,c2", "--note", "checks"], "", "mark_waiting", json!({"on":["c1","c2"],"note":"checks"})),
        (vec!["stop", "c1"], "", "interrupt", json!({"id":"c1"})),
        (vec!["archive", "c1"], "", "archive", json!({"id":"c1","on":true})),
    ] {
        let (result, fake) = faber(false, &args, stdin, &[]).await;
        assert_eq!(result.code, 0, "{}", result.stdout);
        assert!(fake.calls.lock().unwrap().iter().any(|(name, value)| name == command && value == &expected));
    }
}
#[tokio::test(flavor = "multi_thread")]
async fn errors_use_the_contract_exit_codes() {
    let data = tempfile::tempdir().unwrap();
    let bad = run(data.path(), &["list", "--bad"], "", &[]).await;
    assert_eq!(bad.code, 2);
    assert!(lines(&bad.stdout)[0]["error"].is_string());
    let absent = run(data.path(), &["status"], "", &[]).await;
    assert_eq!(absent.code, 3);
    for (cmd, code) in [("ask", 4), ("refuse", 1)] {
        let (r, _) = faber(false, &["call", cmd], "", &[]).await;
        assert_eq!(r.code, code, "{}", r.stdout);
    }
    let (r, _) = faber(true, &["wait", "c1", "--timeout", "1"], "", &[]).await;
    assert_eq!(r.code, 5);
    assert_eq!(lines(&r.stdout)[0]["state"], "working");
}
#[tokio::test(flavor = "multi_thread")]
async fn send_wait_returns_id_then_conclusion_and_call_reaches_the_host() {
    let (r, _) = faber(false, &["send", "hello", "--wait"], "", &[]).await;
    assert_eq!(r.code, 0, "{}", r.stderr);
    let out = lines(&r.stdout);
    assert_eq!(out[0], "c1");
    assert_eq!(out[1]["state"], "concluded");
    let (r, _) = faber(false, &["call", "anything", r#"{"x":1}"#], "", &[]).await;
    assert_eq!(lines(&r.stdout)[0], json!({"cmd":"anything","args":{"x":1}}));
}
#[tokio::test(flavor = "multi_thread")]
async fn help_works_offline_and_contains_the_entire_table() {
    let data = tempfile::tempdir().unwrap();
    let r = run(data.path(), &["help", "--json"], "", &[]).await;
    assert_eq!(r.code, 0);
    let help = lines(&r.stdout);
    let expected: Value = serde_json::from_str(include_str!("../commands.json")).unwrap();
    assert_eq!(help[0]["commands"], expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_reports_the_exact_end_state_and_conclusion() {
    use std::io::{BufRead, BufReader};
    let data = tempfile::tempdir().unwrap();
    let (events, _) = broadcast::channel(64);
    let working = Arc::new(AtomicBool::new(true));
    let fake = Arc::new(Fake { events: events.clone(), calls: Mutex::default(), asks: false, working: working.clone() });
    let served = tokio::spawn({
        let dir = data.path().to_path_buf();
        async move { serve(&dir, fake).await }
    });
    ready(data.path()).await;
    let mut command = Command::new(env!("CARGO_BIN_EXE_faber-cli"));
    command.args(["watch", "--no-start", "--data"]).arg(data.path()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let first = rx.recv_timeout(Duration::from_secs(5));
    if first.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        served.abort();
    }
    let first = first.unwrap();
    if serde_json::from_str::<Value>(&first).unwrap()["state"] != "working" {
        let _ = child.kill();
        let _ = child.wait();
        served.abort();
        panic!("unexpected first state: {first}");
    }
    working.store(false, Ordering::SeqCst);
    events.send(json!({"type":"running"})).unwrap();
    let end = rx.recv_timeout(Duration::from_secs(5));
    let _ = child.kill();
    let _ = child.wait();
    served.abort();
    let end: Value = serde_json::from_str(&end.unwrap()).unwrap();
    assert_eq!(end["was"], "working");
    assert_eq!(end["state"], "concluded");
    assert_eq!(end["conclusion"]["recap"], "Done");
}

#[tokio::test(flavor = "multi_thread")]
async fn human_overrides_json_and_the_end_of_flags_is_respected() {
    let (r, _) = faber(false, &["send", "hello", "--json", "--human"], "", &[]).await;
    assert_eq!(r.code, 0);
    assert_eq!(r.stdout.trim(), "c1");
    let (r, fake) = faber(false, &["send", "--", "--human"], "", &[]).await;
    assert_eq!(r.code, 0);
    assert_eq!(lines(&r.stdout)[0], "c1");
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.iter().find(|(cmd, _)| cmd == "send").unwrap().1["req"]["text"], "--human");
}
