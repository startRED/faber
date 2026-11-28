//! The `faber` program against a Faber of its own: a fake command host served on the real channel of a temporary data
//! folder. Each test runs the built program as a person or an agent would, and reads what it prints and how it exits.

use faber_channel::secret::Secret;
use faber_channel::server::{Answer, serve_with_secret};
use faber_channel::{Backend, Hello, Who};
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
    ending: &'static str,
    commands: Value,
}
impl Backend for Fake {
    fn version(&self) -> String {
        "9.9.9".into()
    }
    fn commands(&self) -> Value {
        self.commands.clone()
    }
    fn starts_turn(&self, cmd: &str) -> bool {
        cmd == "send"
    }
    fn dispatch(&self, _who: &Who, cmd: String, args: Value) -> Answer {
        self.calls.lock().unwrap().push((cmd.clone(), args.clone()));
        let working = self.working.load(Ordering::SeqCst);
        let finish = self.working.clone();
        let asks = self.asks;
        let ending = self.ending;
        let events = self.events.clone();
        Box::pin(async move {
            match cmd.as_str() {
                "conversations" if ending == "waiting" => {
                    let problem = if working { "error.noSubscription" } else { "reason.budget.other" };
                    Ok(json!([{
                        "id":"c1", "title":"Example", "family":"c1", "state":"waiting",
                        "waitingOn":{"since":123,"until":456,"problem":problem,"note":"A saved note"}
                    }]))
                }
                "conversations" => {
                    Ok(json!([{ "id":"c1", "title":"Example", "family":"c1", "state":if working {"working"} else {"idle"} }]))
                }
                "conversation" => {
                    let state = match (args["id"].as_str(), working) {
                        (Some("slow"), _) | (Some("slow-after-notice"), false) => {
                            tokio::time::sleep(Duration::from_millis(1300)).await;
                            "concluded"
                        }
                        (Some("lag-while-reading" | "slow-after-notice"), true) => {
                            finish.store(false, Ordering::SeqCst);
                            let _ = events.send(json!({"type":"lagged"}));
                            "working"
                        }
                        (Some("c1"), true) => "working",
                        (Some("c1"), false) => ending,
                        (Some("lag-while-reading"), false) | (None, _) => "concluded",
                        (Some(state), _) => state,
                    };
                    let mut view = json!({"id":args["id"],"state":state});
                    if state == "concluded" || (args["id"] == "c1" && state == "failed") {
                        view["conclusion"] = json!({"recap":"Done","done":true});
                    }
                    Ok(view)
                }
                "conversation_markdown" => Ok(json!({"markdown":"# Example\n"})),
                "search_messages" => Ok(json!({"ready":true,"hits":[{"conversationId":"c1","title":"Example","snippet":"hello"}]})),
                "start_conversation" => Ok(json!("c1")),
                "wake_wait" => Ok(json!({"state":"blocked","reason":"reason.wait.stopped"})),
                "send" => {
                    if !asks {
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            finish.store(false, Ordering::SeqCst);
                            let _ = events.send(json!({"type":"running"}));
                        });
                    }
                    Ok(json!("c1"))
                }
                "show_conversation" if args["id"] == "missing-window" => Err("error.host.noWindow".into()),
                "show_conversation" => Ok(Value::Null),
                "refuse" => Err("error.test.refused".into()),
                "ask" => Err("error.reach.ask".into()),
                _ => Ok(json!({"cmd":cmd,"args":args})),
            }
        })
    }
    fn dispatch_detailed(&self, who: &Who, cmd: String, args: Value) -> faber_channel::server::DetailedAnswer {
        if cmd == "detailed_refusal" {
            return Box::pin(async {
                Err(faber_channel::wire::Error {
                    key: "error.test.refused".into(),
                    detail: Some("The request cannot go ahead.".into()),
                    data: None,
                })
            });
        }
        if cmd == "ask" {
            return Box::pin(async {
                Err(faber_channel::wire::Error { key: "error.reach.ask".into(), detail: None, data: Some(json!({"request":"abcd1234"})) })
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
    let fake = Arc::new(Fake {
        events,
        calls: Mutex::default(),
        asks,
        working: Arc::new(AtomicBool::new(true)),
        ending: "concluded",
        commands: serde_json::from_str(include_str!("../commands.json")).unwrap(),
    });
    let served = serve_test(data.path(), fake.clone()).await;
    let run = run(data.path(), args, stdin, env).await;
    served.abort();
    (run, fake)
}

async fn serve_test(data: &Path, fake: Arc<Fake>) -> tokio::task::JoinHandle<std::io::Result<()>> {
    let secret = Arc::new(Secret::fresh().unwrap());
    let token = secret.token("readiness").unwrap();
    let dir = data.to_path_buf();
    let served = tokio::spawn(async move { serve_with_secret(&dir, fake, secret).await });
    tokio::time::timeout(Duration::from_secs(10), async {
        // An explicit test identity keeps the probe independent of any agent that launched this suite.
        while faber_channel::Client::connect(data, Hello { token: Some(token.clone()), ..Default::default() }).await.is_err() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fake Faber did not start");
    served
}

fn test_command(data: &Path, no_start: bool, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_faber-cli"));
    command.arg("--data").arg(data);
    if no_start {
        command.arg("--no-start");
    }
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    for name in ["FABER_AGENT_AUTH", "FABER_AGENT_TOKEN", "FABER_AGENT_DEPTH", "FABER_CONVERSATION"] {
        command.env_remove(name);
    }
    command.env("NO_COLOR", "1");
    command
}

async fn run(data: &Path, args: &[&str], stdin: &str, env: &[(&str, &str)]) -> Run {
    let mut command = test_command(data, true, args);
    for (name, value) in env {
        command.env(name, value);
    }
    run_test_command(command, stdin).await
}

async fn run_test_command(mut command: Command, stdin: &str) -> Run {
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
        (vec!["read", "c1"], "", "conversation", json!({"id":"c1","last":20})),
        (vec!["read", "c1", "--from", "2"], "", "conversation", json!({"id":"c1","from":2,"limit":1})),
        (vec!["read", "c1", "--from", "2", "--limit", "5"], "", "conversation", json!({"id":"c1","from":2,"limit":5})),
        (
            vec!["start", "Work", "-", "--ai", "codex", "--effort", "high", "--keep-going"],
            "brief",
            "start_conversation",
            json!({"title":"Work","brief":"brief","ai":"codex","model":null,"effort":"high","step":null,"keepGoing":true}),
        ),
        (vec!["message", "c1", "hello"], "", "message_conversation", json!({"id":"c1","message":"hello"})),
        (vec!["resume", "c1"], "", "resume_conversation", json!({"id":"c1","message":null})),
        (vec!["mark-waiting", "--on", "c1,c2", "--note", "checks"], "", "mark_waiting", json!({"on":["c1","c2"],"note":"checks"})),
        (vec!["wake", "c1", "--since", "5", "--until", "300005"], "", "wake_wait", json!({"id":"c1","since":5,"until":300005})),
        (vec!["stop", "c1"], "", "interrupt", json!({"id":"c1"})),
        (vec!["archive", "c1"], "", "archive", json!({"id":"c1","on":true})),
    ] {
        let (result, fake) = faber(false, &args, stdin, &[]).await;
        assert_eq!(result.code, 0, "{}", result.stdout);
        assert!(fake.calls.lock().unwrap().iter().any(|(name, value)| name == command && value == &expected));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn wake_prints_the_state_and_reason_in_both_output_modes() {
    let args = ["wake", "c1", "--since", "5", "--until", "300005"];
    let (raw, _) = faber(false, &args, "", &[]).await;
    assert_eq!(raw.code, 0);
    assert_eq!(lines(&raw.stdout), [json!({"state":"blocked","reason":"reason.wait.stopped"})]);
    let human = [args.as_slice(), &["--human"]].concat();
    let (human, _) = faber(false, &human, "", &[]).await;
    assert_eq!(human.code, 0);
    assert!(!human.stdout.contains("reason.wait.stopped"));
    assert!(!human.stdout.contains("state: blocked"));
    assert_eq!(human.stdout.lines().count(), 2);
}
#[tokio::test(flavor = "multi_thread")]
async fn session_start_output_identifies_the_child_in_both_modes() {
    for args in [vec!["start", "Work", "brief"], vec!["call", "start_conversation", r#"{"title":"Work","brief":"brief"}"#]] {
        for mode in ["--json", "--human"] {
            let mut command = args.clone();
            command.push(mode);
            let (result, _) = faber(false, &command, "", &[]).await;
            assert_eq!(result.code, 0);
            let marker: Value = serde_json::from_str(result.stdout.lines().next().unwrap()).unwrap();
            assert_eq!(marker, json!({"event":"conversation_started", "childId":"c1"}));
        }
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
    let (r, _) = faber(false, &["call", "ask"], "", &[]).await;
    assert_eq!(lines(&r.stdout), vec![json!({"error":"error.reach.ask", "request":"abcd1234"})]);
    let (r, fake) = faber(false, &["asks"], "", &[]).await;
    assert_eq!(r.code, 0);
    assert!(fake.calls.lock().unwrap().iter().any(|(cmd, _)| cmd == "asks"));
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
    let r = run(data.path(), &["help", "read", "--json"], "", &[]).await;
    assert_eq!(r.code, 0);
    let help = lines(&r.stdout);
    assert_eq!(help[0]["commands"][0]["name"], "conversation");
    assert_eq!(help[0]["commands"][0]["inputSchema"]["properties"]["from"]["type"], "integer");
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn a_present_but_silent_faber_fails_as_unreachable_without_trying_to_start() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    use tokio::net::windows::named_pipe::ServerOptions;

    let data = tempfile::tempdir().unwrap();
    Secret::fresh().unwrap().write(data.path()).unwrap();
    let faber_channel::Address::Pipe(name) = faber_channel::Address::of(data.path()) else { unreachable!() };
    let pipe = ServerOptions::new().first_pipe_instance(true).create(name).unwrap();
    let peer = tokio::time::timeout(Duration::from_secs(8), async move {
        pipe.connect().await.unwrap();
        let mut stream = BufReader::new(pipe);
        let mut hello = String::new();
        stream.read_line(&mut hello).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&hello).unwrap()["cmd"], "hello");
        hello.clear();
        assert_eq!(stream.read_line(&mut hello).await.unwrap(), 0);
    });
    // With startup allowed, an absent executable would return noFaber if startup were attempted after the greeting.
    let mut command = test_command(data.path(), false, &["status", "--json"]);
    command.env("FABER_APP", data.path().join("missing.exe"));
    let (closed, result) = tokio::join!(peer, run_test_command(command, ""));
    closed.expect("the CLI should close the silent pipe");
    assert_eq!(result.code, 3, "{}", result.stderr);
    assert_eq!(lines(&result.stdout)[0]["error"], "cli.error.channel");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_numbered_read_does_not_fall_back_to_all_messages_on_an_older_host() {
    let data = tempfile::tempdir().unwrap();
    let (events, _) = broadcast::channel(64);
    let fake = Arc::new(Fake {
        events,
        calls: Mutex::default(),
        asks: false,
        working: Arc::new(AtomicBool::new(false)),
        ending: "concluded",
        commands: json!([{"name":"conversation","inputSchema":{"properties":{"id":{"type":"string"},"last":{"type":"integer"}}}}]),
    });
    let serving = serve_test(data.path(), fake.clone()).await;
    let r = run(data.path(), &["read", "c1", "--from", "2"], "", &[]).await;
    assert_eq!(r.code, 1);
    assert!(fake.calls.lock().unwrap().is_empty());
    let r = run(data.path(), &["read", "c1", "--last", "1"], "", &[]).await;
    assert_eq!(r.code, 0);
    serving.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_command_filter_checks_what_the_running_host_can_list() {
    for supports_running in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let (events, _) = broadcast::channel(64);
        let properties = if supports_running { json!({"runningOnly":{"type":"boolean"}}) } else { json!({}) };
        let command = "conversation_command_results";
        let fake = Arc::new(Fake {
            events,
            calls: Mutex::default(),
            asks: false,
            working: Arc::new(AtomicBool::new(false)),
            ending: "concluded",
            commands: json!([{"name":command,"inputSchema":{"properties":properties}}]),
        });
        let serving = serve_test(data.path(), fake.clone()).await;
        let running = run(data.path(), &["call", command, r#"{"runningOnly":true}"#], "", &[]).await;
        let calls = fake.calls.lock().unwrap();
        if supports_running {
            assert!(calls.iter().any(|(cmd, args)| cmd == command && args["runningOnly"] == true));
        } else {
            assert_eq!(running.code, 1);
            assert!(calls.is_empty(), "an older app would return ended commands instead");
        }
        serving.abort();
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_exact_ending_filter_checks_what_the_running_host_can_list() {
    let data = tempfile::tempdir().unwrap();
    let (events, _) = broadcast::channel(64);
    let fake = Arc::new(Fake {
        events,
        calls: Mutex::default(),
        asks: false,
        working: Arc::new(AtomicBool::new(false)),
        ending: "concluded",
        commands: json!([{
            "name":"conversations",
            "inputSchema":{"properties":{"state":{"type":"array","items":{"type":"string"}}}}
        }]),
    });
    let serving = serve_test(data.path(), fake.clone()).await;
    let old = run(data.path(), &["list", "--state", "failed,interrupted"], "", &[]).await;
    assert_eq!(old.code, 1, "an unsupported ending must not look like an empty list");
    assert!(fake.calls.lock().unwrap().is_empty());
    let compatible = run(data.path(), &["list", "--state", "running"], "", &[]).await;
    assert_eq!(compatible.code, 0);
    serving.abort();

    let (supported, fake) = faber(false, &["list", "--state", "failed,interrupted"], "", &[]).await;
    assert_eq!(supported.code, 0);
    assert!(
        fake.calls
            .lock()
            .unwrap()
            .iter()
            .any(|(cmd, args)| { cmd == "conversations" && args["state"] == json!(["failed", "interrupted"]) })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_reports_the_exact_end_state_and_conclusion() {
    let (_, done) = watch_transition("concluded", "working").await;
    assert_eq!(done["conclusion"]["recap"], "Done");
    let (_, failed) = watch_transition("failed", "working").await;
    assert!(failed.get("conclusion").is_none(), "an older success cannot describe the new failure");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_reports_a_changed_wait_blocker_without_a_state_change() {
    let (first, changed) = watch_transition("waiting", "waiting").await;
    assert_eq!(first["waitingOn"], json!({"since":123,"until":456,"problem":"error.noSubscription"}));
    assert_eq!(changed["waitingOn"], json!({"since":123,"until":456,"problem":"reason.budget.other"}));
}

async fn watch_transition(ending: &'static str, first_state: &str) -> (Value, Value) {
    use std::io::{BufRead, BufReader};
    let data = tempfile::tempdir().unwrap();
    let (events, _) = broadcast::channel(64);
    let working = Arc::new(AtomicBool::new(true));
    let fake = Arc::new(Fake {
        events: events.clone(),
        calls: Mutex::default(),
        asks: false,
        working: working.clone(),
        ending,
        commands: json!([]),
    });
    let served = serve_test(data.path(), fake).await;
    let mut child = test_command(data.path(), true, &["watch"]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
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
    let first: Value = serde_json::from_str(&first.unwrap()).unwrap();
    if first["state"] != first_state {
        let _ = child.kill();
        let _ = child.wait();
        served.abort();
        panic!("unexpected first state: {first}");
    }
    working.store(false, Ordering::SeqCst);
    events.send(json!({"type":"list"})).unwrap();
    let end = rx.recv_timeout(Duration::from_secs(5));
    let _ = child.kill();
    let _ = child.wait();
    served.abort();
    let end: Value = serde_json::from_str(&end.expect("watch should report the change after a list notice")).unwrap();
    assert_eq!(end["was"], first_state);
    assert_eq!(end["state"], ending);
    (first, end)
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

#[tokio::test(flavor = "multi_thread")]
async fn open_reaches_the_window_and_reports_when_it_is_missing() {
    let (result, fake) = faber(false, &["open", "c1"], "", &[]).await;
    assert_eq!(result.code, 0);
    assert_eq!(fake.calls.lock().unwrap()[0], ("show_conversation".into(), json!({"id":"c1"})));
    let (result, _) = faber(false, &["open", "missing-window"], "", &[]).await;
    assert_eq!(result.code, 1);
    assert_eq!(lines(&result.stdout)[0]["error"], "error.host.noWindow");
}

#[tokio::test(flavor = "multi_thread")]
async fn export_and_search_use_the_host_commands() {
    let (export, fake) = faber(false, &["export", "c1"], "", &[]).await;
    assert_eq!(export.code, 0);
    assert_eq!(lines(&export.stdout)[0]["markdown"], "# Example\n");
    assert_eq!(fake.calls.lock().unwrap()[0], ("conversation_markdown".into(), json!({"id":"c1"})));
    let (search, fake) = faber(false, &["search", "hello", "world", "--area", "projects"], "", &[]).await;
    assert_eq!(search.code, 0);
    assert_eq!(lines(&search.stdout)[0]["hits"][0]["conversationId"], "c1");
    assert_eq!(fake.calls.lock().unwrap()[0], ("search_messages".into(), json!({"query":"hello world","area":"project"})));
}

#[tokio::test(flavor = "multi_thread")]
async fn wait_reports_each_ending_and_keeps_waiting_for_people_or_children() {
    for state in ["concluded", "ended", "interrupted", "stopped", "failed"] {
        let (result, _) = faber(false, &["wait", state], "", &[]).await;
        assert_eq!(result.code, 0);
        assert_eq!(lines(&result.stdout)[0]["state"], state);
        if state != "concluded" {
            assert!(lines(&result.stdout)[0].get("conclusion").is_none());
        }
    }
    for state in ["working", "waiting", "new"] {
        let (result, fake) = faber(false, &["wait", state, "--timeout", "1"], "", &[]).await;
        assert_eq!(result.code, 5);
        assert_eq!(lines(&result.stdout)[0]["state"], state);
        assert_eq!(fake.calls.lock().unwrap().len(), 1, "no new read while the state is unchanged");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn wait_times_out_when_the_first_read_stalls() {
    let (result, _) = faber(false, &["wait", "slow", "--timeout", "1"], "", &[]).await;
    assert_eq!(result.code, 5);
    assert_eq!(lines(&result.stdout)[0]["error"], "cli.error.waitTimeout");
}

#[tokio::test(flavor = "multi_thread")]
async fn wait_checks_again_when_an_event_arrives_during_the_first_read() {
    let (result, fake) = faber(false, &["wait", "lag-while-reading", "--timeout", "1"], "", &[]).await;
    assert_eq!(result.code, 0);
    assert_eq!(lines(&result.stdout)[0]["conclusion"]["recap"], "Done");
    assert_eq!(fake.calls.lock().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn wait_reports_the_last_known_state_when_a_later_read_stalls() {
    let (result, fake) = faber(false, &["wait", "slow-after-notice", "--timeout", "1"], "", &[]).await;
    assert_eq!(result.code, 5);
    assert_eq!(lines(&result.stdout)[0]["state"], "working");
    assert_eq!(fake.calls.lock().unwrap().len(), 2);
}
