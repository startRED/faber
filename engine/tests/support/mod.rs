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
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc::UnboundedReceiver;

/// How long a shared wait gives what it waits for before it fails. A test that passes never waits it out, so it only bounds
/// a failure, and it holds on a loaded runner, where a 10 ms sleep can take far longer. Real time, never counted in ticks.
pub const DEADLINE: Duration = Duration::from_secs(30);

/// Polls `done` every 10 ms until it holds, and fails with `what` and the time waited after `DEADLINE`.
pub async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < DEADLINE, "{what} (waited {:.1?})", start.elapsed());
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Broker fixtures begin at canonical roots, including macOS's /var system alias.
pub fn canonical_tempdir() -> io::Result<tempfile::TempDir> {
    tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)
}

/// A project folder Faber works in, also unattended: one under the build's own `target`, or the system's temporary one
/// where the build sits in a folder Faber refuses (a worktree under `.claude` or a copy under AppData). An owned child of
/// home is the final candidate when the build's temporary directory also lives under AppData.
pub fn work_folder() -> tempfile::TempDir {
    let build = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/test-workspaces");
    std::fs::create_dir_all(&build).unwrap();
    [build, std::env::temp_dir()]
        .into_iter()
        .chain(std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from))
        // Resolve parent components and temporary aliases in the same plain form the engine uses.
        .filter_map(|root| faber_engine::permissions::checked_folder(&root))
        .map(|root| tempfile::tempdir_in(root).unwrap())
        .find(|dir| {
            faber_engine::permissions::checked_folder(dir.path()).is_some()
                && faber_engine::permissions::unattended_root(dir.path()).is_some()
        })
        .expect("a folder Faber works in")
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub input: bool,
    pub msg: Value,
}

pub fn controlled() -> (Spawner, BufReader<tokio::io::DuplexStream>, tokio::io::DuplexStream) {
    let (stdin, input) = tokio::io::duplex(65536);
    let (output, stdout) = tokio::io::duplex(65536);
    let process =
        Arc::new(Mutex::new(Some(Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None })));
    (Arc::new(move |_, _, _| Ok(process.lock().unwrap().take().unwrap())), BufReader::new(input), output)
}

pub async fn read_message(input: &mut BufReader<tokio::io::DuplexStream>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(DEADLINE, input.read_line(&mut line))
        .await
        .unwrap_or_else(|_| panic!("no message on fake wire (waited {DEADLINE:?})"))
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

pub async fn expect_method(input: &mut BufReader<tokio::io::DuplexStream>, method: &str) -> Value {
    let mut msg = read_message(input).await;
    while method != "skills/list" && msg["method"] == "skills/list" {
        msg = read_message(input).await;
    }
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

/// The person's own text of a first input: the line that points to the guide, which Faber puts ahead of it, is not part of it.
fn person_text(content: &Value) -> Value {
    let strip = |text: &str| {
        let rest = faber_engine::guide::without_note(text);
        (rest.len() != text.len()).then(|| rest.to_string())
    };
    match content {
        Value::String(text) => strip(text).map_or_else(|| content.clone(), Value::from),
        Value::Array(parts) => Value::Array(
            parts
                .iter()
                .map(|part| {
                    let mut part = part.clone();
                    if let Some(text) = part["text"].as_str().and_then(strip) {
                        part["text"] = text.into();
                    }
                    part
                })
                .collect(),
        ),
        _ => content.clone(),
    }
}

/// What a first input says without the guide Faber carries ahead of it at a session's start.
pub fn person_string(text: &str) -> String {
    person_text(&Value::from(text)).as_str().unwrap_or_default().to_string()
}

/// A line the driver wrote, with the person's own text where it carries a first input: words a test looks for are the person's.
fn person_line(line: &str) -> String {
    match serde_json::from_str::<Value>(line) {
        Ok(mut msg) if msg["type"] == "user" => {
            msg["message"]["content"] = person_text(&msg["message"]["content"]);
            msg.to_string()
        }
        _ => line.to_string(),
    }
}

// What must match between what the driver sends and what was recorded.
fn key(msg: &Value) -> String {
    let method = msg["method"].as_str();
    let id = &msg["id"];
    // Codex app-server speaks JSON-RPC: requests and notifications have a method, answers have an id.
    if method == Some("turn/start") {
        return format!("turn/start {} {}", id, person_text(&msg["params"]["input"]));
    }
    if method == Some("turn/steer") {
        return format!("turn/steer {} {}", id, msg["params"]["input"]);
    }
    if let Some(m) = method {
        return if id.is_null() { m.to_string() } else { format!("{m} {id}") };
    }
    if !id.is_null() {
        // A question's answer (Codex `item/tool/requestUserInput`) is checked whole: which question got which answer.
        if let Some(answers) = msg["result"].get("answers") {
            return format!("answer {id} {answers}");
        }
        return format!("answer {} {}", id, msg["result"]["decision"].as_str().unwrap_or(""));
    }
    match msg["type"].as_str().unwrap_or_default() {
        "user" => format!("user {}", person_text(&msg["message"]["content"])),
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
    /// The next write holding these words fails, as if the process had closed its input for it.
    refuse_words: Arc<Mutex<Option<String>>>,
}

impl Wire {
    /// The next `n` writes fail; the ones after them pass through again.
    pub fn refuse_next(&self, n: usize) {
        self.refuse.store(n, Ordering::SeqCst);
    }

    /// The next write holding `words` fails; the ones after it pass through again.
    pub fn refuse_words(&self, words: &str) {
        *self.refuse_words.lock().unwrap() = Some(words.to_string());
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
        String::from_utf8_lossy(&self.written.lock().unwrap()).lines().filter(|l| person_line(l).contains(needle)).count()
    }

    pub fn messages(&self) -> Vec<Value> {
        String::from_utf8_lossy(&self.written.lock().unwrap()).lines().map(|line| serde_json::from_str(line).unwrap()).collect()
    }

    pub fn wrap(&self, p: Proc) -> Proc {
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
        let mut words = self.wire.refuse_words.lock().unwrap();
        if words.as_ref().is_some_and(|w| person_line(&String::from_utf8_lossy(buf)).contains(w.as_str())) {
            *words = None;
            return Poll::Ready(Err(io::Error::other("the process closed its input")));
        }
        drop(words);
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
                // A recorded faber_hold with a number of milliseconds is a pause between two lines (the screen checks' fake CLI waits for
                // a release instead); it is never sent.
                if let Some(ms) = entries[at].msg["faber_hold"].as_u64() {
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                    at += 1;
                    continue;
                }
                let line = format!("{}\n", entries[at].msg);
                if fake_out.write_all(line.as_bytes()).await.is_err() {
                    return;
                }
                at += 1;
            }
            let Ok(Some(line)) = lines.next_line().await else { break };
            // A line cut short is the writer's failure, not a mismatch of the recording.
            let Ok(sent) = serde_json::from_str::<Value>(&line) else { continue };
            if sent["method"] == "skills/list" && entries.get(at).is_none_or(|entry| entry.msg["method"] != "skills/list") {
                continue;
            }
            if sent["method"] == "config/read" && entries.get(at).is_none_or(|entry| entry.msg["method"] != "config/read") {
                let reply = format!("{}\n", serde_json::json!({"id":sent["id"],"result":{"config":{}}}));
                if fake_out.write_all(reply.as_bytes()).await.is_err() {
                    return;
                }
                continue;
            }
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
    Proc { stdin: Box::new(driver_in), stdout: Box::new(driver_out), stderr: None, child: None, contained: None }
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

impl Fake {
    /// A project turn starts its CLI once its off-lock project check ends; waits until `count` CLIs have started.
    pub async fn spawned(&self, count: usize) {
        until(&format!("fewer than {count} CLIs started"), || self.args.lock().unwrap().len() >= count).await;
    }
}

/// The request id of the next permission question a session asks.
pub async fn next_permission(rx: &mut UnboundedReceiver<EngineEvent>) -> String {
    loop {
        let e = tokio::time::timeout(DEADLINE, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("no permission question (waited {DEADLINE:?})"))
            .expect("session ended");
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
        let e = tokio::time::timeout(DEADLINE, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("stalled after {:?} (waited {DEADLINE:?}); {:?}", events.last(), fake.mismatches.lock().unwrap()))
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
pub fn tempdir() -> tempfile::TempDir {
    tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

/// Owns a data root and its stable external chat root for the full test lifetime.
pub struct DataFolder {
    _owner: tempfile::TempDir,
    data: std::path::PathBuf,
    chat: std::path::PathBuf,
}

impl DataFolder {
    pub fn new() -> Self {
        let owner = tempdir();
        let parent = faber_engine::permissions::checked_folder(owner.path()).expect("a checked fixture parent");
        let data = parent.join("data");
        let chat = parent.join("work");
        std::fs::create_dir(&data).unwrap();
        Self { _owner: owner, data, chat }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.data
    }

    pub fn chat_path(&self) -> &std::path::Path {
        &self.chat
    }

    /// Existing-work fixtures use the real recovery protocol before seeding an external root.
    pub fn initialize_chat(&self) {
        #[cfg(windows)]
        drop(faber_engine::workspace::relocation::open(&self.data, &self.chat).unwrap());
    }

    pub fn scratch_path(&self) -> std::path::PathBuf {
        #[cfg(windows)]
        {
            self.chat.clone()
        }
        #[cfg(not(windows))]
        {
            self.data.join("chat")
        }
    }
}

/// A fake CLI process the test drives by hand: it writes the CLI's output lines itself, whenever it likes, and reads
/// everything the driver wrote to the process.
pub struct Pipe {
    pub out: tokio::io::DuplexStream,
    pub lines: Arc<Mutex<Vec<Value>>>,
    /// The driver closed the process's input (the end of a session).
    pub closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Pipe {
    pub async fn say(&mut self, msg: Value) {
        self.out.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
    }

    /// The answers (request id, behavior) written so far.
    pub fn answers(&self) -> Vec<(String, String)> {
        self.lines
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m["type"] == "control_response")
            .map(|m| {
                (
                    m["response"]["request_id"].as_str().unwrap().to_owned(),
                    m["response"]["response"]["behavior"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    }
}

/// A spawner whose processes are driven by hand. Every process it starts is queued for `next_pipe`.
pub fn pipes() -> (Spawner, Arc<Mutex<Vec<Pipe>>>) {
    let made: Arc<Mutex<Vec<Pipe>>> = Arc::default();
    let queue = made.clone();
    let spawn: Spawner = Arc::new(move |_bin, _argv, _cwd| {
        let (driver_in, fake_in) = tokio::io::duplex(1 << 20);
        let (fake_out, driver_out) = tokio::io::duplex(1 << 20);
        let lines: Arc<Mutex<Vec<Value>>> = Arc::default();
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (seen, done) = (lines.clone(), closed.clone());
        tokio::spawn(async move {
            let mut input = BufReader::new(fake_in).lines();
            while let Ok(Some(line)) = input.next_line().await {
                if let Ok(msg) = serde_json::from_str::<Value>(&line) {
                    seen.lock().unwrap().push(msg);
                }
            }
            done.store(true, Ordering::SeqCst);
        });
        queue.lock().unwrap().push(Pipe { out: fake_out, lines, closed });
        Ok(Proc { stdin: Box::new(driver_in), stdout: Box::new(driver_out), stderr: None, child: None, contained: None })
    });
    (spawn, made)
}

/// The first process a `pipes` spawner started, once it exists.
pub async fn next_pipe(made: &Arc<Mutex<Vec<Pipe>>>) -> Pipe {
    let mut pipe = None;
    until("no process was started", || {
        let mut made = made.lock().unwrap();
        pipe = (!made.is_empty()).then(|| made.remove(0));
        pipe.is_some()
    })
    .await;
    pipe.unwrap()
}

/// What Faber's own Claude requests (the router, the pulse) must start with whatever mode the person chose: no hooks, no settings
/// files, no MCP servers, no plugins. A hook that fired on every background request would run the person's commands unasked.
pub fn assert_no_user_customizations(args: &[String]) {
    let after = |flag: &str| args[args.iter().position(|a| a == flag).unwrap_or_else(|| panic!("{flag} missing in {args:?}")) + 1].clone();
    assert!(args.contains(&"--safe-mode".into()) && args.contains(&"--strict-mcp-config".into()), "{args:?}");
    assert_eq!(after("--setting-sources"), "", "{args:?}");
    let settings: Value = serde_json::from_str(&after("--settings")).unwrap();
    assert_eq!(settings["disableAllHooks"], true, "{args:?}");
}

/// Writes a conversation of about `bytes` on disk through `Store::save_checked`, shaped like the owner's biggest one (102 MB:
/// 1,362 messages, 35,902 tool records): a user turn, then an assistant reply with 52 tool records, 70 % of them a helper's
/// (`parent` set), each output under the engine's 4000-character cut and every 50th input a written file. Stand-in data
/// only, built in the given folder, and returned so a test can compare what comes back.
pub fn big_conversation(data: &std::path::Path, bytes: usize) -> faber_engine::model::Conversation {
    use faber_engine::model::{Conversation, Message, Role, ToolUse};
    let mut conversation: Conversation = serde_json::from_str(include_str!("../../../tests/fixtures/conversation-export.json")).unwrap();
    conversation.piece_values.clear();
    conversation.browser = None;
    conversation.messages.clear();
    let line = "test result: ok. 42 passed; 0 failed; finished in 0.31s\n";
    let text = |chars: usize| line.repeat(chars / line.len() + 1)[..chars].to_string();
    let (mut size, mut record) = (0, 0usize);
    while size < bytes {
        let n = conversation.messages.len() as u64;
        conversation.messages.push(Message::new(format!("u{n}"), Role::User, text(300), 1_700_000_000_000 + n * 1000));
        let mut reply = Message::new(format!("a{n}"), Role::Assistant, text(800), 1_700_000_000_500 + n * 1000);
        let agent = format!("agent-{n}");
        let tools = (0..52)
            .map(|i| {
                record += 1;
                let input = if record % 50 == 0 {
                    json!({ "file_path": format!("src/part{record}.rs"), "content": text(20_000) })
                } else {
                    json!({ "command": format!("cargo test --filter case{record}"), "description": text(200) })
                };
                let output = text(record * 997 % 4000 + 1);
                size += input.to_string().len() + output.len() + 200;
                let tool = json!({
                    "id": format!("call-{record}"),
                    "name": if record % 50 == 0 { "Write" } else { "Bash" },
                    "input": input,
                    "ok": true,
                    "output": output,
                    "parent": (i % 10 < 7).then(|| agent.clone()),
                    "started": 1_700_000_000_000u64 + record as u64,
                    "ended": 1_700_000_000_100u64 + record as u64,
                });
                serde_json::from_value::<ToolUse>(tool).unwrap()
            })
            .collect();
        reply.tools = Some(tools);
        size += 1100;
        conversation.messages.push(reply);
    }
    faber_engine::store::Store::new(data).save_checked(&conversation).unwrap();
    conversation
}
