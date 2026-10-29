use faber_channel::guard::Guard;
use faber_channel::server::{Answer, MAX_AGENT_DEPTH, connection};
use faber_channel::{Backend, Client, Hello, serve};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;

/// Echoes its arguments, refuses `refuse` with a key, and starts a turn on `send`.
struct Fake {
    events: broadcast::Sender<Value>,
}

impl Backend for Fake {
    fn version(&self) -> String {
        "9.9.9".into()
    }
    fn commands(&self) -> Value {
        json!([{ "name": "send", "args": [["req", "SendRequest"]], "kind": "StartsTurn" }])
    }
    fn starts_turn(&self, cmd: &str) -> bool {
        cmd == "send"
    }
    fn dispatch(&self, cmd: String, args: Value) -> Answer {
        Box::pin(async move {
            match cmd.as_str() {
                "refuse" => Err("error.test.refused".into()),
                "slow" => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(json!("slow"))
                }
                _ => Ok(json!({ "cmd": cmd, "args": args })),
            }
        })
    }
    fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
}

fn fake() -> (Arc<Fake>, broadcast::Sender<Value>) {
    let (events, _) = broadcast::channel(4);
    (Arc::new(Fake { events: events.clone() }), events)
}

async fn client(backend: Arc<Fake>, depth: u32) -> Client {
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection(theirs, backend.clone(), Guard::watching(backend.subscribe())));
    Client::over(ours, Hello { client: "test".into(), agent_depth: depth }).await.unwrap()
}

#[tokio::test]
async fn hello_answers_the_version_protocol_and_the_command_table() {
    let (backend, _) = fake();
    let client = client(backend, 0).await;
    assert_eq!(client.offer["version"], "9.9.9");
    assert_eq!(client.offer["protocol"], faber_channel::PROTOCOL);
    assert_eq!(client.offer["commands"][0]["name"], "send");
}

#[tokio::test]
async fn a_command_before_hello_is_refused() {
    let (backend, _) = fake();
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection(theirs, backend.clone(), Guard::watching(backend.subscribe())));
    let (read, mut write) = tokio::io::split(ours);
    write.write_all(b"{\"id\":1,\"cmd\":\"state\"}\n").await.unwrap();
    let mut line = String::new();
    BufReader::new(read).read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), json!({"id": 1, "err": "error.channel.hello"}));
}

#[tokio::test]
async fn commands_reach_the_backend_and_refusals_keep_their_key() {
    let (backend, _) = fake();
    let client = client(backend, 0).await;
    let echo = client.call("open", json!({"id": "c1"})).await.unwrap().unwrap();
    assert_eq!(echo, json!({"cmd": "open", "args": {"id": "c1"}}));
    assert_eq!(client.call("refuse", Value::Null).await.unwrap(), Err("error.test.refused".into()));
}

#[tokio::test]
async fn a_slow_answer_does_not_hold_the_ones_after_it() {
    let (backend, _) = fake();
    let client = Arc::new(client(backend, 0).await);
    let slow = tokio::spawn({
        let client = client.clone();
        async move { client.call("slow", Value::Null).await.unwrap() }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let quick = tokio::time::timeout(Duration::from_millis(150), client.call("open", Value::Null)).await;
    assert!(quick.is_ok(), "the quick answer came while the slow one ran");
    assert_eq!(slow.await.unwrap(), Ok(json!("slow")));
}

#[tokio::test]
async fn an_agent_too_deep_reads_but_starts_no_turn() {
    let (backend, _) = fake();
    let below = client(backend.clone(), MAX_AGENT_DEPTH - 1).await;
    assert!(below.call("send", json!({})).await.unwrap().is_ok());
    let deep = client(backend, MAX_AGENT_DEPTH).await;
    assert_eq!(deep.call("send", json!({})).await.unwrap(), Err("error.channel.agentDepth".into()));
    assert!(deep.call("state", Value::Null).await.unwrap().is_ok());
}

#[tokio::test]
async fn an_ai_inside_faber_runs_two_turns_at_once_and_a_person_any_number() {
    let (backend, _) = fake();
    let guard = Guard::watching(backend.subscribe());
    let mut agents = Vec::new();
    for _ in 0..2 {
        let (ours, theirs) = tokio::io::duplex(1 << 16);
        tokio::spawn(connection(theirs, backend.clone(), guard.clone()));
        agents.push(Client::over(ours, Hello { client: "agent".into(), agent_depth: 1 }).await.unwrap());
    }
    assert!(agents[0].call("send", json!({"req": {"conversationId": "c1"}})).await.unwrap().is_ok());
    assert!(agents[1].call("send", json!({"req": {"conversationId": "c2"}})).await.unwrap().is_ok());
    let third = agents[1].call("send", json!({"req": {"conversationId": "c3"}})).await.unwrap();
    assert_eq!(third, Err("error.channel.agentTurns".into()), "the limit holds across connections");
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection(theirs, backend.clone(), guard.clone()));
    let person = Client::over(ours, Hello { client: "person".into(), agent_depth: 0 }).await.unwrap();
    assert!(person.call("send", json!({"req": {"conversationId": "c4"}})).await.unwrap().is_ok());
}

#[tokio::test]
async fn events_arrive_in_order_and_a_slow_reader_is_told_what_it_missed() {
    let (backend, events) = fake();
    let client = client(backend, 0).await;
    let mut heard = client.subscribe().await.unwrap();
    events.send(json!({"type": "list"})).unwrap();
    events.send(json!({"type": "catalog"})).unwrap();
    assert_eq!(heard.recv().await.unwrap(), json!({"type": "list"}));
    assert_eq!(heard.recv().await.unwrap(), json!({"type": "catalog"}));
    assert!(client.subscribe().await.is_err(), "events go to the first subscription only");
}

#[tokio::test]
async fn the_real_channel_serves_one_faber_per_data_folder() {
    let data = tempfile::tempdir().unwrap();
    let (backend, _) = fake();
    let served = tokio::spawn({
        let (data, backend) = (data.path().to_path_buf(), backend.clone());
        async move { serve(&data, backend).await }
    });
    let mut client = None;
    for _ in 0..100 {
        if let Ok(c) = Client::connect(data.path(), Hello::default()).await {
            client = Some(c);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let client = client.expect("the channel answered");
    assert_eq!(client.call("open", json!(1)).await.unwrap().unwrap()["args"], 1);
    let second = tokio::time::timeout(Duration::from_secs(2), serve(data.path(), backend)).await;
    assert!(matches!(second, Ok(Err(_))), "a second Faber on the same data is refused");
    served.abort();
}

#[tokio::test]
async fn no_faber_means_not_found() {
    let data = tempfile::tempdir().unwrap();
    let error = Client::connect(data.path(), Hello::default()).await.err().unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}
