use faber_channel::guard::Guard;
use faber_channel::server::{Answer, MAX_AGENT_DEPTH, connection_at};
use faber_channel::{Backend, Client, Hello, serve};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;

fn test_secret() -> Arc<faber_channel::secret::Secret> {
    Arc::new(faber_channel::secret::Secret::from_hex(&"11".repeat(32)).unwrap())
}

/// Echoes its arguments, refuses `refuse` with a key, and starts a turn on `send`.
struct Fake {
    events: broadcast::Sender<Value>,
}

impl Backend for Fake {
    fn version(&self) -> String {
        "9.9.9".into()
    }
    fn commands(&self) -> Value {
        json!([{ "name": "send", "args": [["req", "SendRequest"]], "kind": "startsTurn" }])
    }
    fn starts_turn(&self, cmd: &str) -> bool {
        cmd == "send"
    }
    fn dispatch(&self, _who: &faber_channel::Who, cmd: String, args: Value) -> Answer {
        Box::pin(async move {
            match cmd.as_str() {
                "refuse" => Err("error.test.refused".into()),
                "slow" => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok(json!("slow"))
                }
                "huge" => Ok(json!("x".repeat(65536))),
                "never" => std::future::pending().await,
                _ => Ok(json!({ "cmd": cmd, "args": args })),
            }
        })
    }
    fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }
}

#[tokio::test(start_paused = true)]
async fn a_disconnected_client_does_not_keep_its_slot_for_an_unfinished_command() {
    let (backend, _) = fake();
    let (client, server) = tokio::io::duplex(4096);
    let task = tokio::spawn(connection_at(server, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
    let (read, mut write) = tokio::io::split(client);
    write.write_all(format!("{}\n", json!({"id":1,"cmd":"hello","args":{"secret":test_secret().hex()}})).as_bytes()).await.unwrap();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    write.write_all(b"{\"id\":2,\"cmd\":\"never\"}\n").await.unwrap();
    tokio::task::yield_now().await;
    write.shutdown().await.unwrap();
    tokio::time::timeout(Duration::from_secs(6), task).await.unwrap().unwrap();
}

fn fake() -> (Arc<Fake>, broadcast::Sender<Value>) {
    let (events, _) = broadcast::channel(4);
    (Arc::new(Fake { events: events.clone() }), events)
}

async fn client(backend: Arc<Fake>, depth: u32) -> Client {
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection_at(theirs, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
    Client::over(ours, Hello { client: "test".into(), agent_depth: depth, secret: test_secret().hex(), ..Default::default() })
        .await
        .unwrap()
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
    tokio::spawn(connection_at(theirs, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
    let (read, mut write) = tokio::io::split(ours);
    write.write_all(b"{\"id\":1,\"cmd\":\"state\"}\n").await.unwrap();
    let mut line = String::new();
    let mut reader = BufReader::new(read);
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), json!({"id": 1, "err": "error.channel.hello"}));
    line.clear();
    assert_eq!(reader.read_line(&mut line).await.unwrap(), 0);
}

#[tokio::test]
async fn secrets_and_tokens_are_checked_before_any_command() {
    for (args, key) in [
        (json!({}), "error.channel.secret"),
        (json!({"secret": "wrong"}), "error.channel.secret"),
        (json!({"secret": test_secret().hex(), "token": "fa1.invalid.invalid"}), "error.channel.token"),
    ] {
        let (backend, _) = fake();
        let (mut client, server) = tokio::io::duplex(4096);
        tokio::spawn(connection_at(server, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
        client.write_all(format!("{}\n", json!({"id": 1, "cmd": "hello", "args": args})).as_bytes()).await.unwrap();
        let mut output = String::new();
        client.read_to_string(&mut output).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&output).unwrap(), json!({"id": 1, "err": key}));
    }
    let (backend, _) = fake();
    let (ours, theirs) = tokio::io::duplex(4096);
    let secret = test_secret();
    tokio::spawn(connection_at(theirs, backend.clone(), Guard::watching(backend.subscribe()), secret.clone(), 0));
    let hello = Hello { secret: secret.hex(), token: Some(secret.token("c1")), ..Default::default() };
    let client = Client::over(ours, hello).await.unwrap();
    assert_eq!(client.offer["caller"], json!({"kind":"agent", "conversation":"c1"}));
    assert_eq!(client.call("hello", json!({})).await.unwrap(), Err("error.channel.hello".into()));
}

#[tokio::test]
async fn revocation_stops_an_existing_connection_and_does_not_restore_it_on_resume() {
    let (backend, _) = fake();
    let secret = test_secret();
    let token = secret.token("c1");
    let (ours, theirs) = tokio::io::duplex(4096);
    tokio::spawn(connection_at(theirs, backend.clone(), Guard::watching(backend.subscribe()), secret.clone(), 0));
    let hello = Hello { secret: secret.hex(), token: Some(token.clone()), ..Default::default() };
    let client = Client::over(ours, hello).await.unwrap();
    assert!(client.call("state", Value::Null).await.unwrap().is_ok());
    secret.revoke(&token);
    let next = secret.token("c1");
    assert_ne!(token, next);
    assert_eq!(client.call("state", Value::Null).await.unwrap(), Err("error.channel.revoked".into()));
    let (ours, theirs) = tokio::io::duplex(4096);
    tokio::spawn(connection_at(theirs, backend.clone(), Guard::watching(backend.subscribe()), secret.clone(), 0));
    let hello = Hello { secret: secret.hex(), token: Some(token), ..Default::default() };
    assert_eq!(Client::over(ours, hello).await.err().unwrap().to_string(), "error.channel.revoked");
}

#[tokio::test]
async fn revocation_cancels_a_command_before_its_pending_mutation() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::sync::Notify;

    struct Pending {
        events: broadcast::Sender<Value>,
        started: Arc<Notify>,
        release: Arc<Notify>,
        dropped: Arc<Notify>,
        changed: Arc<AtomicBool>,
    }
    struct OnDrop(Arc<Notify>);
    impl Drop for OnDrop {
        fn drop(&mut self) {
            self.0.notify_one();
        }
    }
    impl Backend for Pending {
        fn version(&self) -> String {
            String::new()
        }
        fn commands(&self) -> Value {
            json!([])
        }
        fn starts_turn(&self, _: &str) -> bool {
            false
        }
        fn dispatch(&self, _: &faber_channel::Who, _: String, _: Value) -> Answer {
            let (started, release, dropped, changed) =
                (self.started.clone(), self.release.clone(), self.dropped.clone(), self.changed.clone());
            Box::pin(async move {
                let _on_drop = OnDrop(dropped);
                started.notify_one();
                release.notified().await;
                changed.store(true, Ordering::SeqCst);
                Ok(Value::Null)
            })
        }
        fn subscribe(&self) -> broadcast::Receiver<Value> {
            self.events.subscribe()
        }
    }

    let (events, _) = broadcast::channel(4);
    let backend = Arc::new(Pending {
        events,
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
        dropped: Arc::new(Notify::new()),
        changed: Arc::new(AtomicBool::new(false)),
    });
    let secret = test_secret();
    let token = secret.token("one");
    let other = secret.token("other");
    let (ours, theirs) = tokio::io::duplex(4096);
    tokio::spawn(connection_at(theirs, backend.clone(), Guard::watching(backend.subscribe()), secret.clone(), 0));
    let client = Client::over(ours, Hello { secret: secret.hex(), token: Some(token.clone()), ..Default::default() }).await.unwrap();
    let call = tokio::spawn(async move { client.call("mutate", Value::Null).await });
    backend.started.notified().await;
    secret.revoke(&other);
    tokio::task::yield_now().await;
    assert!(!call.is_finished(), "another session's revocation must not cancel this request");
    secret.revoke(&token);
    tokio::time::timeout(Duration::from_secs(1), backend.dropped.notified()).await.expect("the revoked command was not cancelled");
    backend.release.notify_one();
    assert!(!backend.changed.load(Ordering::SeqCst));
    let _ = call.await;
}

#[tokio::test(start_paused = true)]
async fn a_client_that_does_not_say_hello_in_five_seconds_is_closed() {
    let (backend, _) = fake();
    let (mut client, server) = tokio::io::duplex(4096);
    tokio::spawn(connection_at(server, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).await.unwrap(), 0);
}

#[tokio::test]
async fn an_oversized_request_closes_the_connection() {
    let (backend, _) = fake();
    let (mut client, server) = tokio::io::duplex(3 * 1024 * 1024);
    tokio::spawn(connection_at(server, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
    client.write_all(&vec![b'x'; 2 * 1024 * 1024]).await.unwrap();
    let mut byte = [0];
    assert_eq!(client.read(&mut byte).await.unwrap(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_disconnected_client_cannot_hold_a_slot_by_stopping_its_reader() {
    let (backend, _) = fake();
    let (client, server) = tokio::io::duplex(512);
    let task = tokio::spawn(connection_at(server, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
    let (read, mut write) = tokio::io::split(client);
    write.write_all(format!("{}\n", json!({"id":1,"cmd":"hello","args":{"secret":test_secret().hex()}})).as_bytes()).await.unwrap();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    write.write_all(b"{\"id\":2,\"cmd\":\"huge\"}\n").await.unwrap();
    write.shutdown().await.unwrap();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(6)).await;
    tokio::time::timeout(Duration::from_secs(6), task).await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_connected_client_that_stops_reading_releases_its_slot() {
    let (backend, _) = fake();
    let (client, server) = tokio::io::duplex(512);
    let task = tokio::spawn(connection_at(server, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 0));
    let (read, mut write) = tokio::io::split(client);
    write.write_all(format!("{}\n", json!({"id":1,"cmd":"hello","args":{"secret":test_secret().hex()}})).as_bytes()).await.unwrap();
    let mut reader = BufReader::new(read);
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();
    write.write_all(b"{\"id\":2,\"cmd\":\"huge\"}\n").await.unwrap();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(6)).await;
    tokio::time::timeout(Duration::from_secs(6), task).await.unwrap().unwrap();
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
        tokio::spawn(connection_at(theirs, backend.clone(), guard.clone(), test_secret(), 0));
        agents.push(
            Client::over(ours, Hello { client: "agent".into(), agent_depth: 1, secret: test_secret().hex(), ..Default::default() })
                .await
                .unwrap(),
        );
    }
    assert!(agents[0].call("send", json!({"req": {"conversationId": "c1"}})).await.unwrap().is_ok());
    assert!(agents[1].call("send", json!({"req": {"conversationId": "c2"}})).await.unwrap().is_ok());
    let third = agents[1].call("send", json!({"req": {"conversationId": "c3"}})).await.unwrap();
    assert_eq!(third, Err("error.channel.agentTurns".into()), "the limit holds across connections");
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection_at(theirs, backend.clone(), guard.clone(), test_secret(), 0));
    let person = Client::over(ours, Hello { client: "person".into(), agent_depth: 0, secret: test_secret().hex(), ..Default::default() })
        .await
        .unwrap();
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
async fn a_client_faber_knows_it_started_is_an_ai_whatever_it_says() {
    let (backend, _) = fake();
    let (ours, theirs) = tokio::io::duplex(1 << 16);
    tokio::spawn(connection_at(theirs, backend.clone(), Guard::watching(backend.subscribe()), test_secret(), 1));
    let liar = Client::over(ours, Hello { client: "agent".into(), agent_depth: 0, secret: test_secret().hex(), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(liar.offer["caller"], json!({"kind":"agent","conversation":null}));
}

#[tokio::test]
async fn a_reader_that_falls_behind_hears_it_lagged_even_when_no_event_follows() {
    let (ours, theirs) = tokio::io::duplex(1 << 20);
    let (read, mut write) = tokio::io::split(theirs);
    tokio::spawn(async move {
        let mut lines = BufReader::new(read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let asked: Value = serde_json::from_str(&line).unwrap();
            write
                .write_all(
                    format!(
                        "{}
",
                        json!({ "id": asked["id"], "ok": true })
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            if asked["cmd"] == "subscribe" {
                for n in 0..faber_channel::client::EVENTS_KEPT + 10 {
                    write
                        .write_all(
                            format!(
                                "{}
",
                                json!({ "event": { "type": "list", "n": n } })
                            )
                            .as_bytes(),
                        )
                        .await
                        .unwrap();
                }
            }
        }
    });
    let client = Client::over(ours, Hello { client: "test".into(), agent_depth: 0, secret: test_secret().hex(), ..Default::default() })
        .await
        .unwrap();
    let mut heard = client.subscribe().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut last = Value::Null;
    let mut count = 0;
    while let Ok(event) = heard.try_recv() {
        last = event;
        count += 1;
    }
    assert_eq!(count, faber_channel::client::EVENTS_KEPT);
    assert_eq!(last, json!({ "type": "lagged" }));
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
    assert!(Client::connect(data.path(), Hello::default()).await.is_ok(), "the running Faber keeps its secret");
    served.abort();
}

#[tokio::test]
async fn the_seventeenth_connection_is_busy() {
    let data = tempfile::tempdir().unwrap();
    let (backend, _) = fake();
    let served = tokio::spawn({
        let data = data.path().to_path_buf();
        async move { serve(&data, backend).await }
    });
    let mut clients = Vec::new();
    for _ in 0..100 {
        if let Ok(client) = Client::connect(data.path(), Hello::default()).await {
            clients.push(client);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(clients.len(), 1);
    for _ in 1..16 {
        clients.push(Client::connect(data.path(), Hello::default()).await.unwrap());
    }
    #[cfg(windows)]
    let stream = match faber_channel::Address::of(data.path()) {
        faber_channel::Address::Pipe(name) => tokio::net::windows::named_pipe::ClientOptions::new().open(name).unwrap(),
        _ => unreachable!(),
    };
    #[cfg(unix)]
    let stream = match faber_channel::Address::of(data.path()) {
        faber_channel::Address::Socket(path) => tokio::net::UnixStream::connect(path).await.unwrap(),
        _ => unreachable!(),
    };
    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader.read_line(&mut line).await.unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), json!({"id":0,"err":"error.channel.busy"}));
    line.clear();
    assert_eq!(reader.read_line(&mut line).await.unwrap(), 0);
    served.abort();
}

#[tokio::test]
async fn no_faber_means_not_found() {
    let data = tempfile::tempdir().unwrap();
    let error = Client::connect(data.path(), Hello::default()).await.err().unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}
