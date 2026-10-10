//! Recorded sessions replayed through the real drivers, without spawning anything.
#![allow(dead_code)]

use faber_engine::claude::EventSink;
use faber_engine::events::{EngineEvent, Session, UserTurn};
use faber_engine::proc::{Proc, Spawner};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone)]
pub struct Entry {
    pub input: bool,
    pub msg: Value,
}

pub fn fixture(vendor: &str, name: &str) -> Vec<Entry> {
    let path = format!("{}/../tests/fixtures/{vendor}/{name}.jsonl", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: Value = serde_json::from_str(l).unwrap();
            Entry { input: v["dir"] == "in", msg: v["msg"].clone() }
        })
        .collect()
}

pub fn text_of(events: &[EngineEvent]) -> String {
    events.iter().filter_map(|e| if let EngineEvent::Text(t) = e { Some(t.as_str()) } else { None }).collect()
}

// What must match between what the driver sends and what was recorded.
fn key(msg: &Value) -> String {
    let method = msg["method"].as_str();
    let id = &msg["id"];
    // Codex app-server speaks JSON-RPC: requests and notifications have a method, answers have an id.
    if method == Some("turn/start") {
        return format!("turn/start {} {}", id, msg["params"]["input"]);
    }
    if let Some(m) = method {
        return if id.is_null() { m.to_string() } else { format!("{m} {id}") };
    }
    if !id.is_null() {
        return format!("answer {} {}", id, msg["result"]["decision"].as_str().unwrap_or(""));
    }
    match msg["type"].as_str().unwrap_or_default() {
        "user" => format!("user {}", msg["message"]["content"]),
        "control_request" => format!("control_request {}", msg["request"]["subtype"].as_str().unwrap_or("")),
        "control_response" => {
            let r = &msg["response"];
            format!("control_response {} {}", r["request_id"].as_str().unwrap_or(""), r["response"]["behavior"].as_str().unwrap_or("ok"))
        }
        other => other.to_string(),
    }
}

pub type Mismatches = Arc<Mutex<Vec<String>>>;

/// A fake CLI process that replays a recorded session. Each line the driver writes is checked against the next
/// recorded input; then the recorded outputs up to the following input are streamed back.
pub fn replay(entries: Vec<Entry>, mismatches: Mismatches) -> Proc {
    let (driver_in, mut fake_in) = tokio::io::duplex(1 << 22);
    let (mut fake_out, driver_out) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        let mut at = 0;
        let mut lines = BufReader::new(&mut fake_in).lines();
        loop {
            while at < entries.len() && !entries[at].input {
                let line = format!("{}\n", entries[at].msg);
                if fake_out.write_all(line.as_bytes()).await.is_err() {
                    return;
                }
                at += 1;
            }
            let Ok(Some(line)) = lines.next_line().await else { break };
            let sent = key(&serde_json::from_str(&line).unwrap());
            let expected = if at < entries.len() && entries[at].input { key(&entries[at].msg) } else { "(no input expected)".into() };
            if sent == expected {
                at += 1;
            } else {
                mismatches.lock().unwrap().push(format!("sent {sent}, recorded {expected}"));
            }
        }
        // stdin closed: the process exits, so its output ends
    });
    Proc { stdin: Box::new(driver_in), stdout: Box::new(driver_out), stderr: None, child: None }
}

/// A spawner that replays `entries` each time it is called, recording the arguments it got.
pub struct Fake {
    pub spawn: Spawner,
    pub mismatches: Mismatches,
    pub args: Arc<Mutex<Vec<Vec<String>>>>,
}

pub fn fake(entries: Vec<Entry>) -> Fake {
    let mismatches: Mismatches = Arc::default();
    let args: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
    let (m, a) = (mismatches.clone(), args.clone());
    let spawn: Spawner = Arc::new(move |_bin, argv, _cwd| {
        a.lock().unwrap().push(argv.to_vec());
        Ok(replay(entries.clone(), m.clone()))
    });
    Fake { spawn, mismatches, args }
}

/// Runs a recorded session through a real driver. `react` answers like the app would and returns true when done.
pub async fn drive(
    entries: Vec<Entry>,
    start: impl FnOnce(Spawner, EventSink) -> Session,
    first: &str,
    mut react: impl FnMut(&EngineEvent, &Session) -> bool,
) -> (Vec<EngineEvent>, Vec<String>) {
    let fake = fake(entries);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let session = start(fake.spawn.clone(), Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text(first));
    let mut events = Vec::new();
    loop {
        let e = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("stalled after {:?}; {:?}", events.last(), fake.mismatches.lock().unwrap()))
            .expect("session ended early");
        events.push(e.clone());
        if react(&e, &session) {
            break;
        }
    }
    session.close();
    let mismatches = fake.mismatches.lock().unwrap().clone();
    (events, mismatches)
}
