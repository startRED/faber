//! End-to-end MCP calls over the real local channel and a fake host.
use faber_channel::server::Answer;
use faber_channel::{Backend, Who, serve};
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
}
impl Backend for Fake {
    fn version(&self) -> String {
        "9.9.9".into()
    }
    fn commands(&self) -> Value {
        serde_json::from_str(include_str!("../commands.json")).unwrap()
    }
    fn starts_turn(&self, _: &str) -> bool {
        false
    }
    fn dispatch(&self, who: &Who, cmd: String, args: Value) -> Answer {
        self.calls.lock().unwrap().push((cmd, args.clone()));
        let caller = serde_json::to_value(who).unwrap();
        Box::pin(async move { Ok(json!({"id":args["id"], "caller":caller})) })
    }
    fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
    fn dispatch_detailed(&self, who: &Who, cmd: String, args: Value) -> faber_channel::server::DetailedAnswer {
        if cmd == "answer_ask" {
            return Box::pin(async {
                Err(faber_channel::wire::Error { key: "error.reach.ask".into(), data: Some(json!({"request":"abcd1234"})) })
            });
        }
        let answer = self.dispatch(who, cmd, args);
        Box::pin(async move { answer.await.map_err(Into::into) })
    }
}
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}
async fn ready(data: &Path) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(secret) = faber_channel::secret::Secret::read(data) {
                let hello = faber_channel::Hello { token: Some(secret.token("probe")), ..Default::default() };
                if faber_channel::Client::connect(data, hello).await.is_ok() {
                    break;
                }
            }
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
    command.env_remove("FABER_AGENT_DEPTH").env_remove("FABER_AGENT_TOKEN").env_remove("FABER_CONVERSATION");
    for (name, value) in env {
        command.env(name, value);
    }
    let input = stdin.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
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
    text.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
}
#[tokio::test(flavor = "multi_thread")]
async fn mcp_call_round_trip_uses_the_callers_identity() {
    let data = tempfile::tempdir().unwrap();
    let (events, _) = broadcast::channel(64);
    let fake = Arc::new(Fake { events, calls: Mutex::default() });
    let served = tokio::spawn({
        let (dir, fake) = (data.path().to_path_buf(), fake.clone());
        async move { serve(&dir, fake).await }
    });
    ready(data.path()).await;
    let token = faber_channel::secret::Secret::read(data.path()).unwrap().token("c1");
    let input = concat!(
        "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"conversation\",\"arguments\":{\"id\":\"c1\"}}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"answer_ask\",\"arguments\":{\"id\":\"a\",\"allow\":true}}}\n"
    );
    let result = run(data.path(), &["mcp"], input, &[("FABER_AGENT_TOKEN", &token), ("FABER_CONVERSATION", "c1")]).await;
    assert_eq!(result.code, 0, "{}", result.stderr);
    let responses = lines(&result.stdout);
    assert_eq!(responses.len(), 4);
    assert!(responses[1]["result"]["tools"].as_array().unwrap().iter().any(|tool| tool["name"] == "conversation"));
    let answer: Value = serde_json::from_str(responses[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(answer["id"], "c1");
    assert_eq!(answer["caller"], json!({"kind":"agent", "conversation":"c1"}));
    assert_eq!(responses[3]["result"]["isError"], true);
    let detail: Value = serde_json::from_str(responses[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(detail, json!({"key":"error.reach.ask", "data":{"request":"abcd1234"}}));
    assert!(fake.calls.lock().unwrap().iter().any(|(name, args)| name == "conversation" && args["id"] == "c1"));
    let no_token = run(data.path(), &["mcp"], input, &[("FABER_CONVERSATION", "c1")]).await;
    let responses = lines(&no_token.stdout);
    let answer: Value = serde_json::from_str(responses[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(answer["caller"], json!({"kind":"agent", "conversation":null}));
    let mismatch = run(data.path(), &["mcp"], input, &[("FABER_AGENT_TOKEN", &token), ("FABER_CONVERSATION", "other")]).await;
    assert_eq!(mismatch.code, 3);
    let invalid = run(data.path(), &["mcp"], "", &[("FABER_AGENT_TOKEN", "invalid"), ("FABER_CONVERSATION", "c1")]).await;
    assert_eq!(invalid.code, 3);
    served.abort();
}
