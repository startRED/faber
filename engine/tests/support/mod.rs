//! Recorded sessions replayed through the real drivers, without spawning anything.
#![allow(dead_code)]

use faber_engine::claude::EventSink;
use faber_engine::events::{EngineEvent, Session, UserTurn};
use faber_engine::proc::{Proc, Spawner};
use serde_json::{Value, json};
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::UnboundedReceiver;

/// Broker fixtures begin at canonical roots, including macOS's /var system alias.
pub fn canonical_tempdir() -> io::Result<tempfile::TempDir> {
    tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub input: bool,
    pub msg: Value,
}

pub fn controlled() -> (Spawner, BufReader<tokio::io::DuplexStream>, tokio::io::DuplexStream) {
    let (stdin, input) = tokio::io::duplex(65536);
    let (output, stdout) = tokio::io::duplex(65536);
    let process = Arc::new(Mutex::new(Some(Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None })));
    (Arc::new(move |_, _, _| Ok(process.lock().unwrap().take().unwrap())), BufReader::new(input), output)
}

pub async fn read_message(input: &mut BufReader<tokio::io::DuplexStream>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(1), input.read_line(&mut line)).await.expect("no message on fake wire").unwrap();
    serde_json::from_str(&line).unwrap()
}

pub async fn expect_method(input: &mut BufReader<tokio::io::DuplexStream>, method: &str) -> Value {
    let msg = read_message(input).await;
    assert_eq!(msg["method"], method);
    msg
}

pub async fn emit(output: &mut tokio::io::DuplexStream, msg: Value) {
    output.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
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

/// A hand-written Claude session of two turns. Each turn asks to use the Write tool under request id `request` and
/// ends once the question is allowed, so the same request id comes up again in the second turn.
pub fn claude_two_turns(request: &str) -> Vec<Entry> {
    let user = |text: &str| Entry {
        input: true,
        msg: json!({ "type": "user", "message": { "role": "user", "content": text }, "parent_tool_use_id": null, "session_id": "" }),
    };
    let ask = |file: &str| Entry {
        input: false,
        msg: json!({
            "type": "control_request",
            "request_id": request,
            "request": { "subtype": "can_use_tool", "tool_name": "Write", "input": { "file_path": file, "content": "x" } }
        }),
    };
    let allowed = |file: &str| Entry {
        input: true,
        msg: json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": request,
                "response": { "behavior": "allow", "updatedInput": { "file_path": file, "content": "x" } }
            }
        }),
    };
    let init =
        Entry { input: false, msg: json!({ "type": "system", "subtype": "init", "session_id": "two-turns", "model": "claude-haiku-5-5" }) };
    let result = || Entry {
        input: false,
        msg: json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} }),
    };
    vec![user("first"), init, ask("a.txt"), allowed("a.txt"), result(), user("second"), ask("b.txt"), allowed("b.txt"), result()]
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

/// What the fake processes were sent, and a switch that makes their next writes fail, as if the process had closed
/// its input. Every process a spawner makes shares one `Wire`.
#[derive(Clone, Default)]
pub struct Wire {
    written: Arc<Mutex<Vec<u8>>>,
    refuse: Arc<AtomicUsize>,
    /// Bytes the next write lets through before the write after it fails (0 = off).
    tear: Arc<AtomicUsize>,
    /// Set once a write was cut short: every write after it fails.
    torn: Arc<std::sync::atomic::AtomicBool>,
    /// The next flushes fail.
    refuse_flush: Arc<AtomicUsize>,
}

impl Wire {
    /// The next `n` writes fail; the ones after them pass through again.
    pub fn refuse_next(&self, n: usize) {
        self.refuse.store(n, Ordering::SeqCst);
    }

    /// The next write lets only its first `n` bytes through and succeeds; the write after it fails, as a pipe does that
    /// closes in the middle of a line.
    pub fn tear_next(&self, n: usize) {
        self.tear.store(n, Ordering::SeqCst);
    }

    /// The next flush fails, after the bytes before it went through.
    pub fn refuse_next_flush(&self) {
        self.refuse_flush.store(1, Ordering::SeqCst);
    }

    /// How many bytes went through so far.
    pub fn len(&self) -> usize {
        self.written.lock().unwrap().len()
    }

    /// How many of the lines written so far hold `needle`.
    pub fn lines_with(&self, needle: &str) -> usize {
        String::from_utf8_lossy(&self.written.lock().unwrap()).lines().filter(|l| l.contains(needle)).count()
    }

    fn wrap(&self, p: Proc) -> Proc {
        Proc { stdin: Box::new(Tap { inner: p.stdin, wire: self.clone() }), ..p }
    }
}

struct Tap {
    inner: Box<dyn AsyncWrite + Send + Unpin>,
    wire: Wire,
}

impl AsyncWrite for Tap {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        if self.wire.refuse.load(Ordering::SeqCst) > 0 {
            self.wire.refuse.fetch_sub(1, Ordering::SeqCst);
            return Poll::Ready(Err(io::Error::other("the process closed its input")));
        }
        if self.wire.torn.load(Ordering::SeqCst) {
            return Poll::Ready(Err(io::Error::other("the process closed its input")));
        }
        let tear = self.wire.tear.swap(0, Ordering::SeqCst);
        let buf = if tear > 0 {
            self.wire.torn.store(true, Ordering::SeqCst);
            &buf[..tear.min(buf.len())]
        } else {
            buf
        };
        let done = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &done {
            self.wire.written.lock().unwrap().extend_from_slice(&buf[..*n]);
        }
        done
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.wire.refuse_flush.swap(0, Ordering::SeqCst) > 0 {
            return Poll::Ready(Err(io::Error::other("the process closed its input")));
        }
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

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
            // A line cut short is the writer's failure, not a mismatch of the recording.
            let Ok(sent) = serde_json::from_str(&line) else { continue };
            let sent = key(&sent);
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

/// A spawner that replays `entries` each time it is called, recording the arguments it got and every line written.
pub struct Fake {
    pub spawn: Spawner,
    pub mismatches: Mismatches,
    pub args: Arc<Mutex<Vec<Vec<String>>>>,
    pub wire: Wire,
}

pub fn fake(entries: Vec<Entry>) -> Fake {
    let mismatches: Mismatches = Arc::default();
    let args: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
    let wire = Wire::default();
    let (m, a, w) = (mismatches.clone(), args.clone(), wire.clone());
    let spawn: Spawner = Arc::new(move |_bin, argv, _cwd| {
        a.lock().unwrap().push(argv.to_vec());
        Ok(w.wrap(replay(entries.clone(), m.clone())))
    });
    Fake { spawn, mismatches, args, wire }
}

/// The request id of the next permission question a session asks.
pub async fn next_permission(rx: &mut UnboundedReceiver<EngineEvent>) -> String {
    loop {
        let e =
            tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("no permission question in time").expect("session ended");
        if let EngineEvent::Permission { request_id, .. } = e {
            return request_id;
        }
    }
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
