# B5a: Running List, Engine and Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The engine and the Tauri shell report which conversations are running or wait for the user, keep the latest quota reading of each window, and confirm every permission answer with the CLI before the question leaves the list.

**Architecture:** `Core` keeps the last running list it emitted and recomputes it in one place (`refresh_running`) wherever it already emits a `Message` or `List`; the list is emitted, with the next revision, only when it differs. `Engine::answer` becomes an `async fn` in three steps (validate and hand over under the lock, wait for the session's acknowledgement without it, settle under the lock again). The session handlers acknowledge each answer after the write and keep their pending request until the write works. The one screen caller of `answer` passes the turn id and ignores a rejection.

**Tech Stack:** Rust 2024 (tokio, serde), Tauri 2, SolidJS + TypeScript, the Rust test runner `cargo t` (suites `rust`, `ui`, `screen`).

**Spec:** `docs/superpowers/specs/2026-10-11-faber-redesign-design.md`: B5 ("Shell layout and running list", the engine half), A7, A19, AI-17 and AI-25, and the Review log (rounds 1 to 3) items about B5a. Read the "Engine: the running list and the quota readings" part of B5 before Task 3 and the "What `answer` takes and returns" part before Task 4. This is commit 6 of the order in B0: B5a. B5b (the layout and the running column) comes after it and uses it.

## Global Constraints

- Code, comments, docs and commit messages are in English. Original code only; nothing from TL-ADE, TL-Control or TL-Orchestrator.
- Never read, copy, store or forward CLI credentials or session tokens. The CLI binaries run unmodified.
- Child processes never go through a shell. This plan starts none.
- Every text on screen lives in `ui/locales/`. The engine returns keys (`error.*`, `reason.*`), never sentences. This plan adds two keys, `error.permission.stale` and `error.permission.undelivered`; their texts come with the screen in B5b, and the screen of this commit only ignores a rejected answer.
- Memory: the running list is one pass over the live conversations. No new process, no new long-lived task, no bigger DOM.
- Do not use vendor names in product, feature or package names.
- Tests never spawn real processes, touch real git or call a real CLI. The engine tests use the fake spawner and the recordings in `tests/fixtures/`.
- Spec values, verbatim: the revision starts at 0 and grows by one with every list emitted; the answer waits at most 5 s in all for the acknowledgements; Auto waits at most 30 s for a route; the errors are `error.permission.stale` and `error.permission.undelivered`.
- rustfmt: `max_width = 140`, `use_small_heuristics = "Max"`. The code in this plan is already formatted; `cargo fmt --all --check` must stay clean.
- One commit, made after every proof command exits 0 (Task 7). The proof commands: `npm run lint`, `npm run typecheck`, `npm run build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p faber --features test-hooks -- -D warnings`, `cargo t --all`.
- Rust environment on the maintainer's PC, before any `cargo`:

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/rust/cargo/bin:$PATH"
```

## Review Focus

Failure modes the spec implies and a person would meet, most likely first. Each one has a test in the task named after it.

1. **Both copies of a permission card are clicked before either answer is confirmed.** Expected: One click succeeds, the other is `error.permission.stale`, and the CLI gets one line. Pinned by: Task 4: `two_clicks_on_the_two_copies_of_a_card_give_one_ok_and_one_stale` (in `answer.rs`) and `a_second_answer_to_the_same_request_is_stale_and_the_cli_gets_one_line` (real Claude session).
2. **The CLI's input is closed, or stops answering, at the moment the user clicks Allow.** Expected: The card stays, the screen is told `error.permission.undelivered` (not "answered"), and the same card can be answered again; one line reaches the CLI in the end. Pinned by: Task 2: the two session tests with a failing first write. Task 4: `a_write_that_fails_keeps_the_question_and_the_revision_and_a_second_answer_can_land`, `a_closed_queue_is_undelivered_and_the_question_stays`, `no_acknowledgement_in_five_seconds_is_undelivered_and_a_late_one_changes_nothing`, `an_answer_the_cli_cannot_take_is_undelivered_and_the_same_card_can_be_answered_again`.
3. **A late answer from an old window names an earlier turn, and the next turn asks the same request id (Codex numbers its approvals from 0 again after a restart).** Expected: The old answer is `stale`, nothing is written, and the new turn's question still waits. Pinned by: Task 4: `the_turn_id_is_checked_so_a_late_answer_never_lands_on_the_next_turn`, `an_answer_that_names_the_previous_turn_is_stale_and_writes_nothing`, `request_ids_that_repeat_in_two_conversations_are_two_questions`.
4. **The user deletes the conversation, or removes the project, while Auto is choosing or while an answer waits for its acknowledgement.** Expected: No panic and no row left behind: the answer is `stale`, the reply ends with an error, the running list empties. Pinned by: Task 3: `a_start_that_fails_after_the_pick_ends_the_reply_with_an_error_and_drops_the_row`, `a_send_that_cannot_start_ends_the_reply_with_an_error_instead_of_leaving_it_running`. Task 4: `a_conversation_removed_while_the_engine_waits_makes_the_answer_stale_without_a_panic`, `removing_a_running_conversation_drops_its_row`.
5. **Auto's router never answers, or the user presses Stop while it is still choosing.** Expected: After 30 s the default model answers and the same row keeps running; Stop drops the row at once. Pinned by: Task 3: `a_route_that_never_arrives_falls_back_to_the_default_and_the_row_keeps_running`, `stopping_while_auto_still_chooses_drops_the_row`.

## Where this plan meets commit 4 (B3 looks)

Commit 4 is being applied in the same tree by another agent. This plan does not depend on anything B3 adds; it only has to edit around it. These are the files both touch:

| File | What commit 4 (B3 looks) changes there | What this plan changes there |
| :--- | :--- | :--- |
| `engine/src/engine.rs` | `Engine::settings` and `Engine::save_settings` (about lines 233 to 240) and `Core::save_settings` (about 385 to 415), which now persist with a `Result` | The imports, `AppEvent`, `AppState`, the `Core` fields and `new`, `Core::state`, `Core::emit_message`, the list emissions, `Core::send`, `Engine::answer` (removed), `Core::answer` (removed) and a helper at the end. None of these lines is one of B3's |
| `engine/tests/engine.rs` | New tests at the end of the file (about line 323) | A helper after `fn ask` and the three lines that call `engine.answer` |
| `src-tauri/src/main.rs` | Imports, `FullState`, new look commands, `create_window`, the command list | The `answer` command only |
| `ui/App.tsx` | The imports, the greeting, the ambient layer, the `<main>` block | The one `onAnswer=` line inside `<Thread>` |
| `ui/api.ts` | The look types, `AppState`, `saveSettings`, `errorText` | The one `answer:` line |
| `testkit/src/screen/app.rs` | Appearance scenarios | Not touched. The new scenario is in its own file so the two commits never meet there |

Every `Find` text in this plan comes from a region B3 does not change, so the edits apply the same whether B3 is committed, applied and uncommitted, or not yet applied. If a `Find` text is not found exactly once, stop and report it: the file changed under the plan.

## Line endings

`engine/src/codex.rs`, `engine/src/events.rs`, `engine/src/model.rs` and `ui/Thread.tsx` are stored with CRLF in the index; every other file in this plan is LF. The edits below are written with LF. Task 1 converts those four files to LF and Task 7 converts them back, so the commit shows only the real changes. Do not run any formatter over the whole tree in between.

## File structure

| File | Responsibility after this commit |
| :--- | :--- |
| `engine/src/model.rs` | The data of the running list: `RunState`, `RunningItem`, `Running`, `QuotaReading` |
| `engine/src/engine.rs` | `Core` keeps and recomputes the list (`running_items`, `refresh_running`, `list_changed`, `running_item`), keeps the quota readings, ends a reply that never started (`fail_reply`) |
| `engine/src/engine/answer.rs` | One concern: answering a question. `Engine::answer`, `Core::prepare_answer`, `Core::settle_answer`, and the tests that use stand-in sessions |
| `engine/src/engine/auto.rs` | Reads the used share from the quota readings; uses `fail_reply` |
| `engine/src/events.rs`, `claude.rs`, `codex.rs`, `proc.rs` | The acknowledgement: `Delivery`, the `ack` in `SessionCommand::Answer`, `Session::answer`, the handlers that remove their pending request only after the write |
| `engine/tests/running.rs` | The running list and the answers, through the real engine and the real session handlers |
| `engine/tests/support/mod.rs` | `Wire`, a stdin that records and can fail, `claude_two_turns`, `next_permission` |
| `src-tauri/src/main.rs` | The `answer` command with `turn_id` |
| `ui/api.ts`, `ui/Thread.tsx`, `ui/App.tsx` | The one caller of `answer` |
| `testkit/src/screen/permissions.rs` | The scenario that clicks a card |

How this plan was checked: every code block below was applied, task by task, to a fresh clone at commit `65793c1`, and the result passed all seven proof commands (`cargo t --all`: 231 passed, 0 failed). The text of each block is the text that was applied.

---

### Task 1: Prepare the working tree  (model: `haiku`)

**Files:**
- Modify: none (the working tree is prepared).

**Interfaces:**
- Consumes: nothing
- Produces: a tree where the four CRLF files are LF

- [ ] **Step 1: Check the starting point**

Run:

```bash
git status --short && git log --oneline -1
```

Expected: the branch is `phase-1-base`. If commit 4 (B3 looks) is already committed, the log shows it; if it is applied but uncommitted, `git status` lists its files (see the overlap table above). Either way the B5a edits below apply on top, and none of their `Find` texts is changed by B3. If a `Find` text is not found exactly once, stop and report which one: the file changed under the plan.

Run:

```bash
cargo t --suite rust
```

Expected: passes. Export the Rust environment from the top of this plan first. This is the baseline the plan changes.

- [ ] **Step 2: Make four files LF while they are edited**

Four files are stored with CRLF line endings in the index. The edits below are written with LF, so these files are converted first and converted back in Task 7, which leaves the commit with only the real changes in them. Check what the index holds before converting:

Run:

```bash
git ls-files --eol engine/src/codex.rs engine/src/events.rs engine/src/model.rs ui/Thread.tsx
```

Expected: each line starts with `i/crlf`. If one says `i/lf`, leave that file alone now and in Task 7.

Convert the files that show `i/crlf`. The command reads each file in full before it writes it back:

Run:

```bash
python -c "import pathlib, sys; [pathlib.Path(p).write_bytes(pathlib.Path(p).read_bytes().replace(b'\r\n', b'\n')) for p in sys.argv[1:]]" engine/src/codex.rs engine/src/events.rs engine/src/model.rs ui/Thread.tsx
```

Expected: no output. `git diff --stat -- engine/src/codex.rs engine/src/events.rs engine/src/model.rs ui/Thread.tsx` now shows these files as changed (every line), which Task 7 undoes.

---

### Task 2: Sessions confirm their answers  (model: `sonnet`)

**Files:**
- Modify: `engine/src/events.rs`, `engine/src/proc.rs`, `engine/src/claude.rs`, `engine/src/codex.rs`, `engine/src/engine.rs` (two lines)
- Test: `engine/tests/support/mod.rs` (replaced), `engine/tests/claude_session.rs`, `engine/tests/codex_session.rs`

**Interfaces:**
- Consumes: nothing
- Produces: `engine::events::Delivery { Delivered, Unknown, WriteFailed }`; `SessionCommand::Answer { request_id: String, allow: bool, ack: tokio::sync::oneshot::Sender<Delivery> }`; `Session::answer(&self, request_id: &str, allow: bool) -> tokio::sync::oneshot::Receiver<Delivery>` (a closed queue makes the receiver fail); `proc::write_line(..) -> std::io::Result<()>`. Test support: `Wire` (`refuse_next(n)`, `lines_with(needle)`), `Fake.wire`, `claude_two_turns(request: &str) -> Vec<Entry>`, `next_permission(rx)`.

- [ ] **Step 1: Write the test support and the two failing session tests**

`engine/tests/support/mod.rs` gains `Wire` (a tap on the fake process's input that records every line and can refuse the next writes), `claude_two_turns` (a hand-written two-turn Claude script, used in Task 4) and `next_permission`. The whole file is given so nothing has to be merged by hand. Everything outside the new items is today's file; only the imports and the `wire` field of `Fake` changed.

**Write the whole file** `engine/tests/support/mod.rs`:

```rust
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
}

impl Wire {
    /// The next `n` writes fail; the ones after them pass through again.
    pub fn refuse_next(&self, n: usize) {
        self.refuse.store(n, Ordering::SeqCst);
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
        let done = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &done {
            self.wire.written.lock().unwrap().extend_from_slice(&buf[..*n]);
        }
        done
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
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
```

The two session tests: one for Claude and one for Codex. Each runs the real handlers; the fake process's input fails its next write on demand.

**Edit** `engine/tests/claude_session.rs`. Find:

```rust
use faber_engine::events::{EngineEvent, UserTurn};
use faber_engine::model::Area;
use support::{Entry, drive, fixture, text_of};
```

Replace with:

```rust
use faber_engine::events::{Delivery, EngineEvent, UserTurn};
use faber_engine::model::Area;
use std::sync::Arc;
use support::{Entry, drive, fake, fixture, next_permission, text_of};
```

**Edit** `engine/tests/claude_session.rs`. Find:

```rust
                s.answer(request_id, asks.len() == 1);
```

Replace with:

```rust
                drop(s.answer(request_id, asks.len() == 1));
```

**Edit** `engine/tests/claude_session.rs`. Find:

```rust
#[tokio::test]
async fn interrupt_stops_the_turn_and_the_same_session_takes_the_next_one() {
```

Replace with:

```rust
#[tokio::test]
async fn an_answer_that_fails_to_write_keeps_the_request_and_the_retry_is_written_once() {
    let f = fake(fixture("claude", "tools"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = ClaudeOptions { bin: "claude".into(), cwd: ".".into(), launch: launch(Area::Chat), spawn: f.spawn.clone() };
    let session = start_claude(opts, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text(
        "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.",
    ));
    let request = next_permission(&mut rx).await;
    let written = || f.wire.lines_with("control_response");

    f.wire.refuse_next(1);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed));
    assert_eq!(written(), 0, "the refused write left nothing on the wire");
    // The session still holds the request, so the same answer can be given again.
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    assert_eq!(written(), 1);
    // A request the session never held, and one it has already delivered, write nothing.
    assert_eq!(session.answer("never-asked", true).await, Ok(Delivery::Unknown));
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Unknown));
    assert_eq!(written(), 1);
    session.close();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn interrupt_stops_the_turn_and_the_same_session_takes_the_next_one() {
```

**Edit** `engine/tests/codex_session.rs`. Find:

```rust
use faber_engine::events::{EngineEvent, Session, UserTurn};
use faber_engine::model::Area;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, drive, fake, fixture, text_of};
```

Replace with:

```rust
use faber_engine::events::{Delivery, EngineEvent, Session, UserTurn};
use faber_engine::model::Area;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, drive, fake, fixture, next_permission, text_of};
```

**Edit** `engine/tests/codex_session.rs`. Find:

```rust
                s.answer(request_id, asks.len() == 1);
```

Replace with:

```rust
                drop(s.answer(request_id, asks.len() == 1));
```

**Edit** `engine/tests/codex_session.rs`. Find:

```rust
#[tokio::test]
async fn codex_interrupt_stops_the_turn_and_the_thread_takes_the_next_one() {
```

Replace with:

```rust
#[tokio::test]
async fn codex_an_answer_that_fails_to_write_keeps_the_approval_and_the_retry_is_written_once() {
    let f = fake(fixture("codex", "tools"));
    let opts = CodexServerOptions {
        bin: "codex".into(),
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let thread = CodexThread { cwd: ".".into(), model: None, effort: None, resume: None, instructions: None };
    let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text("Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step."));
    let request = next_permission(&mut rx).await;
    assert_eq!(request, "0", "Codex numbers its requests from 0 in each server");
    let written = || f.wire.lines_with("\"decision\"");

    f.wire.refuse_next(1);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed));
    assert_eq!(written(), 0, "the refused write left nothing on the wire");
    // The thread still holds the approval, so the same answer can be given again.
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    assert_eq!(written(), 1);
    // An approval the thread never held, and one it has already delivered, write nothing.
    assert_eq!(session.answer("7", true).await, Ok(Delivery::Unknown));
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Unknown));
    assert_eq!(written(), 1);
    session.close();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn codex_interrupt_stops_the_turn_and_the_thread_takes_the_next_one() {
```

`drop(...)` is needed from now on: `Session::answer` returns a receiver, and an unused future is a warning (an error under clippy).

Run:

```bash
cargo test -p faber-engine --test claude_session --test codex_session
```

Expected: does not compile: `Delivery` is not in `faber_engine::events`, and `Session::answer` returns `()`, which has no `.await`. That is the failing test.

- [ ] **Step 2: Add `Delivery` and the acknowledgement to the session commands**

**Edit** `engine/src/events.rs`. Find:

```rust
/// What the engine can ask of a live CLI session.
#[derive(Debug)]
pub enum SessionCommand {
    Send(UserTurn),
    Answer { request_id: String, allow: bool },
```

Replace with:

```rust
/// What a session says about an answer it was given, once it has done the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The answer was written and flushed to the CLI's input.
    Delivered,
    /// The session holds no such pending request, so it wrote nothing.
    Unknown,
    /// The write or the flush failed. The session keeps the request, so the same answer can be given again.
    WriteFailed,
}

/// What the engine can ask of a live CLI session.
#[derive(Debug)]
pub enum SessionCommand {
    Send(UserTurn),
    /// `ack` hears what became of the answer. A session that is gone drops it, and the receiver then fails.
    Answer {
        request_id: String,
        allow: bool,
        ack: tokio::sync::oneshot::Sender<Delivery>,
    },
```

**Edit** `engine/src/events.rs`. Find:

```rust
    pub fn answer(&self, request_id: &str, allow: bool) {
        (self.send)(SessionCommand::Answer { request_id: request_id.to_string(), allow });
    }
```

Replace with:

```rust
    /// The receiver hears whether the CLI got the answer. If the session is gone it fails instead, because the
    /// command, and the sender in it, were dropped.
    pub fn answer(&self, request_id: &str, allow: bool) -> tokio::sync::oneshot::Receiver<Delivery> {
        let (ack, delivered) = tokio::sync::oneshot::channel();
        (self.send)(SessionCommand::Answer { request_id: request_id.to_string(), allow, ack });
        delivered
    }
```

- [ ] **Step 3: `write_line` reports a failed write**

**Edit** `engine/src/proc.rs`. Find:

```rust
pub(crate) async fn write_line(stdin: &mut (dyn AsyncWrite + Send + Unpin), msg: &serde_json::Value) {
    let mut line = msg.to_string();
    line.push('\n');
    // A dead process shows up as end of output; the write error itself adds nothing.
    let _ = stdin.write_all(line.as_bytes()).await;
    let _ = stdin.flush().await;
}
```

Replace with:

```rust
/// Writes one JSON line and flushes it. Only a caller that must know whether the CLI got the line looks at the
/// result; for the others a dead process shows up as the end of its output.
pub(crate) async fn write_line(stdin: &mut (dyn AsyncWrite + Send + Unpin), msg: &serde_json::Value) -> io::Result<()> {
    let mut line = msg.to_string();
    line.push('\n');
    stdin.write_all(line.as_bytes()).await?;
    stdin.flush().await
}
```

- [ ] **Step 4: The Claude handler answers from its own record and keeps it until the write worked**

**Edit** `engine/src/claude.rs`. Find:

```rust
use crate::events::{EngineEvent, Session, SessionCommand, UserTurn};
```

Replace with:

```rust
use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
```

**Edit** `engine/src/claude.rs`. Find:

```rust
                    Some(SessionCommand::Send(turn)) => proc::write_line(&mut *stdin, &user_message(&turn)).await,
                    Some(SessionCommand::Answer { request_id, allow }) => {
                        let input = pending.remove(&request_id).unwrap_or(Value::Null);
                        let response = if allow { json!({ "behavior": "allow", "updatedInput": input }) } else { json!({ "behavior": "deny", "message": "The user declined." }) };
                        let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } });
                        proc::write_line(&mut *stdin, &msg).await;
                    }
                    Some(SessionCommand::Interrupt) => {
                        control_id += 1;
                        let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                        proc::write_line(&mut *stdin, &msg).await;
                    }
```

Replace with:

```rust
                    Some(SessionCommand::Send(turn)) => {
                        let _ = proc::write_line(&mut *stdin, &user_message(&turn)).await;
                    }
                    Some(SessionCommand::Answer { request_id, allow, ack }) => {
                        // The request stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match pending.get(&request_id) {
                            None => Delivery::Unknown,
                            Some(input) => {
                                let response = if allow { json!({ "behavior": "allow", "updatedInput": input }) } else { json!({ "behavior": "deny", "message": "The user declined." }) };
                                let msg = json!({ "type": "control_response", "response": { "subtype": "success", "request_id": request_id, "response": response } });
                                match proc::write_line(&mut *stdin, &msg).await {
                                    Ok(()) => {
                                        pending.remove(&request_id);
                                        Delivery::Delivered
                                    }
                                    Err(_) => Delivery::WriteFailed,
                                }
                            }
                        };
                        let _ = ack.send(delivery);
                    }
                    Some(SessionCommand::Interrupt) => {
                        control_id += 1;
                        let msg = json!({ "type": "control_request", "request_id": format!("faber-{control_id}"), "request": { "subtype": "interrupt" } });
                        let _ = proc::write_line(&mut *stdin, &msg).await;
                    }
```

- [ ] **Step 5: The Codex handler does the same with its approvals**

**Edit** `engine/src/codex.rs`. Find:

```rust
use crate::events::{EngineEvent, Session, SessionCommand, UserTurn};
```

Replace with:

```rust
use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
```

**Edit** `engine/src/codex.rs`. Find:

```rust
            proc::write_line(&mut *stdin, &json!({ "id": next_id, "method": $method, "params": $params })).await;
```

Replace with:

```rust
            let _ = proc::write_line(&mut *stdin, &json!({ "id": next_id, "method": $method, "params": $params })).await;
```

**Edit** `engine/src/codex.rs`. Find:

```rust
                            Waiting::Initialize => {
                                proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
```

Replace with:

```rust
                            Waiting::Initialize => {
                                let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
```

**Edit** `engine/src/codex.rs`. Find:

```rust
                            (true, None) => proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": "decline" } })).await,
```

Replace with:

```rust
                            (true, None) => {
                                let _ = proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": "decline" } })).await;
                            }
```

**Edit** `engine/src/codex.rs`. Find:

```rust
                                proc::write_line(&mut *stdin, &error).await;
```

Replace with:

```rust
                                let _ = proc::write_line(&mut *stdin, &error).await;
```

**Edit** `engine/src/codex.rs`. Find:

```rust
                    SessionCommand::Answer { request_id, allow } => {
                        if let Some(id) = threads.get_mut(&key).and_then(|t| t.approvals.remove(&request_id)) {
                            let decision = if allow { "accept" } else { "decline" };
                            proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": decision } })).await;
                        }
                    }
```

Replace with:

```rust
                    SessionCommand::Answer { request_id, allow, ack } => {
                        // The approval stays pending until the write worked, so a failed write can be answered again.
                        let delivery = match threads.get(&key).and_then(|t| t.approvals.get(&request_id)).cloned() {
                            None => Delivery::Unknown,
                            Some(id) => {
                                let decision = if allow { "accept" } else { "decline" };
                                match proc::write_line(&mut *stdin, &json!({ "id": id, "result": { "decision": decision } })).await {
                                    Ok(()) => {
                                        if let Some(t) = threads.get_mut(&key) {
                                            t.approvals.remove(&request_id);
                                        }
                                        Delivery::Delivered
                                    }
                                    Err(_) => Delivery::WriteFailed,
                                }
                            }
                        };
                        let _ = ack.send(delivery);
                    }
```

**Edit** `engine/src/codex.rs`. Find:

```rust
        proc::write_line(&mut *stdin, &init).await;
```

Replace with:

```rust
        let _ = proc::write_line(&mut *stdin, &init).await;
```

**Edit** `engine/src/codex.rs`. Find:

```rust
                proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "model/list", "params": {} })).await;
```

Replace with:

```rust
                let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
                let _ = proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "model/list", "params": {} })).await;
```

- [ ] **Step 6: The engine ignores the receiver where it never needed one**

The engine still has its old, synchronous `answer`; Task 4 replaces it. Until then it only has to compile.

**Edit** `engine/src/engine.rs`. Find:

```rust
                    if let Some(l) = self.live.get(conv) {
                        l.session.answer(&request_id, true);
                    }
```

Replace with:

```rust
                    if let Some(l) = self.live.get(conv) {
                        drop(l.session.answer(&request_id, true));
                    }
```

**Edit** `engine/src/engine.rs`. Find:

```rust
            for id in &answered {
                l.session.answer(id, allow);
            }
```

Replace with:

```rust
            for id in &answered {
                drop(l.session.answer(id, allow));
            }
```

- [ ] **Step 7: Run the tests**

Run:

```bash
cargo test -p faber-engine --test claude_session --test codex_session
```

Expected: every test passes, including the two new ones. The refused write leaves nothing on the wire, the retry writes exactly one line, and a request the session never held, or one it already delivered, reports `Unknown` and writes nothing.

Run:

```bash
cargo test -p faber-engine
```

Expected: the whole engine suite still passes (the old `answer` and its callers are untouched).

---

### Task 3: The running list and the quota readings  (model: `sonnet`)

**Files:**
- Create: `engine/tests/running.rs`
- Modify: `engine/Cargo.toml`, `engine/src/model.rs`, `engine/src/engine.rs`, `engine/src/engine/auto.rs`

**Interfaces:**
- Consumes: Task 2's `engine/tests/support/mod.rs`
- Produces: `model::{RunState, RunningItem, Running, QuotaReading}`; `AppEvent::Running { revision: u64, items: Vec<RunningItem> }`; `AppState.running: Running` and `AppState.quotas: Vec<QuotaReading>`; on `Core`: `refresh_running(&mut self)`, `list_changed(&mut self)`, `fail_reply(&mut self, id: &str, reply_id: &str, error: String)`, and `emit_message(&mut self, ..)` (now `&mut`).

- [ ] **Step 1: Allow tests to pause the clock**

Two tests below wait out the router's 30 seconds and (in Task 4) the 5 seconds of an answer. Tokio's paused clock needs its `test-util` feature, for tests only.

**Edit** `engine/Cargo.toml`. Find:

```toml
[dev-dependencies]
tempfile = "3"
```

Replace with:

```toml
[dev-dependencies]
tempfile = "3"
# The paused clock of the tests that wait out the router (30 s) and an answer's acknowledgement (5 s).
tokio = { version = "1", features = ["test-util"] }
```

- [ ] **Step 2: Write the failing tests**

A new test file. It holds every engine test of the running list; Task 4 adds the tests of `Engine::answer` at the marker on its last line. In this task the first test still calls today's synchronous `engine.answer(&id, request_id, allow, always)`; Task 4 changes that one line.

**Write the whole file** `engine/tests/running.rs`:

```rust
//! The running list and the quota readings: what the engine says is running, in which order, and when it says it.

mod support;

use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::model::{Area, RunState, RunningItem, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use support::{Mismatches, fake, fixture, replay};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";
const TWO_FILES: &str = "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.";
const WAIT: Duration = Duration::from_secs(5);

/// A running list as the engine emitted it: its revision and its rows.
type List = (u64, Vec<RunningItem>);

fn engine(dir: &Path, vendors: &[Vendor], spawn: Spawner, ask: Option<Ask>) -> (Engine, UnboundedReceiver<AppEvent>) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let clis: BTreeMap<Vendor, Cli> = vendors.iter().map(|v| (*v, Cli { command: v.name().into(), prefix: vec![] })).collect();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis,
        spawn,
        windows: false,
        max_warm: 8,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    (engine, rx)
}

fn request(area: Area, project: Option<&str>, text: &str, vendor: &str) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area,
        project_id: project.map(String::from),
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: vendor.into(), model: None },
    }
}

/// The `Running` events up to the first list `done` accepts, oldest first. Other events are skipped.
async fn lists_until(rx: &mut UnboundedReceiver<AppEvent>, wait: Duration, mut done: impl FnMut(&[RunningItem]) -> bool) -> Vec<List> {
    let mut seen = Vec::new();
    loop {
        let e =
            tokio::time::timeout(wait, rx.recv()).await.expect("the running list never reached the expected state").expect("engine gone");
        if let AppEvent::Running { revision, items } = e {
            let stop = done(&items);
            seen.push((revision, items));
            if stop {
                return seen;
            }
        }
    }
}

fn first_is(state: RunState) -> impl FnMut(&[RunningItem]) -> bool {
    move |items| items.first().is_some_and(|i| i.state == state)
}

/// The state of the first row of each list, repeats collapsed. An empty list is `None`.
fn states(lists: &[List]) -> Vec<Option<RunState>> {
    let mut out: Vec<Option<RunState>> = Vec::new();
    for (_, items) in lists {
        let state = items.first().map(|i| i.state);
        if out.last() != Some(&state) {
            out.push(state);
        }
    }
    out
}

fn assert_consecutive(lists: &[List]) {
    let revisions: Vec<u64> = lists.iter().map(|l| l.0).collect();
    assert_eq!(revisions, (1..=revisions.len() as u64).collect::<Vec<_>>(), "each list is the previous revision plus one");
}

/// A router that says "chat, easy" after `delay`, or never answers.
fn router(delay: Option<Duration>) -> Ask {
    Arc::new(move |_prompt| {
        Box::pin(async move {
            match delay {
                Some(delay) => {
                    tokio::time::sleep(delay).await;
                    Some(r#"{"kind":"chat","difficulty":"easy","confidence":0.95}"#.to_string())
                }
                None => std::future::pending::<Option<String>>().await,
            }
        })
    })
}

#[tokio::test]
async fn a_turn_is_listed_as_working_a_permission_makes_it_needs_you_and_the_end_drops_it() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "tools"));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], f.spawn.clone(), None);
    let id = engine.send(request(Area::Chat, None, TWO_WRITES, "claude")).unwrap();
    let reply = engine.open(&id).unwrap().messages[1].clone();

    let mut lists = Vec::new();
    let mut answered = String::new();
    for round in 0..2 {
        // The CLI may ask the second question before the engine has settled the first, so wait for a list that holds
        // only a question not answered yet.
        let fresh = |items: &[RunningItem]| {
            items
                .first()
                .is_some_and(|i| i.state == RunState::NeedsYou && i.permissions.len() == 1 && i.permissions[0].request_id != answered)
        };
        lists.extend(lists_until(&mut rx, WAIT, fresh).await);
        let item = lists.last().unwrap().1[0].clone();
        answered = item.permissions[0].request_id.clone();
        assert_eq!((item.conversation_id.as_str(), item.turn_id.as_str()), (id.as_str(), reply.id.as_str()));
        assert_eq!((item.area, item.vendor, item.title.as_str()), (Area::Chat, Some(Vendor::Claude), &TWO_WRITES[..60]));
        assert_eq!((item.permissions.len(), item.permissions[0].tool.as_str(), item.since), (1, "Write", reply.at));
        // The recording allowed the first Write and denied the second.
        engine.answer(&id, &item.permissions[0].request_id, round == 0, false);
    }
    lists.extend(lists_until(&mut rx, WAIT, |items| items.is_empty()).await);
    engine.shutdown();

    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    assert_consecutive(&lists);
    // The CLI may ask its next question before the engine has settled the last answer, so the row can go from one
    // question straight to the next. What is fixed: it starts working, then asks, and in the end it leaves.
    let seen = states(&lists);
    assert_eq!(seen[..2], [Some(RunState::Working), Some(RunState::NeedsYou)]);
    assert_eq!(seen.last(), Some(&None));
    let now = engine.state().running;
    assert_eq!((now.revision, now.items), (lists.last().unwrap().0, vec![]), "the snapshot is the latest list");
}

#[tokio::test]
async fn an_answer_emits_only_the_lists_that_differ_from_the_one_before() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], fake(fixture("claude", "chat")).spawn, None);
    engine.send(request(Area::Chat, None, SKY, "claude")).unwrap();
    let mut lists = Vec::new();
    let (mut messages, mut ended) = (0, false);
    while !ended {
        match tokio::time::timeout(WAIT, rx.recv()).await.expect("the answer never ended").expect("engine gone") {
            AppEvent::Running { revision, items } => {
                ended = items.is_empty();
                lists.push((revision, items));
            }
            AppEvent::Message { .. } => messages += 1,
            _ => {}
        }
    }
    engine.shutdown();
    // Every message event re-reads the list, and the list is told only when it differs.
    assert!(messages >= 2, "the question and the answer both reached the screen");
    assert_consecutive(&lists);
    // The turn began, the CLI named its model, the turn ended: three lists at most.
    assert!(lists.len() <= 3, "{} lists: {lists:?}", lists.len());
    assert!(lists.windows(2).all(|w| w[0].1 != w[1].1), "two equal lists in a row");
}

#[tokio::test]
async fn blocks_with_a_question_come_first_then_the_sidebar_order_with_chat_last_and_questions_lead_each_block() {
    let dir = tempfile::tempdir().unwrap();
    let tools = fixture("claude", "tools");
    let calls = Arc::new(AtomicUsize::new(0));
    // The first two conversations never answer, so they are working; the next two stop at a question.
    let spawn: Spawner = Arc::new(move |_bin, _args, _cwd| {
        let entries = if calls.fetch_add(1, Ordering::SeqCst) < 2 { vec![] } else { tools.clone() };
        Ok(replay(entries, Mismatches::default()))
    });
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], spawn, None);
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let first = engine.add_project(&one.path().to_string_lossy());
    let second = engine.add_project(&two.path().to_string_lossy());

    let mut ids = Vec::new();
    for (area, project, text) in [
        (Area::Project, Some(first.id.as_str()), "alpha"),
        (Area::Project, Some(second.id.as_str()), "bravo"),
        (Area::Project, Some(second.id.as_str()), TWO_WRITES),
        (Area::Chat, None, TWO_WRITES),
    ] {
        ids.push(engine.send(request(area, project, text, "claude")).unwrap());
        // The rows are ordered by when their turn began, so each turn begins a little later than the one before.
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let [alpha, bravo, charlie, delta]: [String; 4] = ids.try_into().unwrap();

    let both_asking = |items: &[RunningItem]| items.iter().filter(|i| i.state == RunState::NeedsYou).count() == 2;
    let lists = lists_until(&mut rx, WAIT, |items| items.len() == 4 && both_asking(items)).await;
    let rows = &lists.last().unwrap().1;
    // Project 2 and Chat hold a question, so they lead, in the sidebar's order. Project 2 shows its question before
    // bravo, which started earlier. Project 1 only works, so it comes last.
    let order: Vec<(&str, RunState)> = rows.iter().map(|i| (i.conversation_id.as_str(), i.state)).collect();
    let expected = [
        (charlie.as_str(), RunState::NeedsYou),
        (bravo.as_str(), RunState::Working),
        (delta.as_str(), RunState::NeedsYou),
        (alpha.as_str(), RunState::Working),
    ];
    assert_eq!(order, expected);
    assert_consecutive(&lists);
    let now = engine.state().running;
    assert_eq!((now.revision, &now.items), (lists.last().unwrap().0, rows), "the snapshot is the latest list");
    engine.shutdown();
}

#[tokio::test]
async fn renaming_a_running_conversation_emits_a_list_with_the_new_title() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], fake(fixture("claude", "tools")).spawn, None);
    let id = engine.send(request(Area::Chat, None, TWO_WRITES, "claude")).unwrap();
    let before = lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await;
    engine.rename(&id, "Quarterly report");
    let after = lists_until(&mut rx, WAIT, |items| items.first().is_some_and(|i| i.title == "Quarterly report")).await;
    engine.shutdown();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].0, before.last().unwrap().0 + 1);
    assert_eq!(after[0].1[0].state, RunState::NeedsYou, "only the title changed");
}

#[tokio::test]
async fn the_latest_quota_readings_are_kept_with_their_reset_times() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], fake(fixture("claude", "chat")).spawn, None);
    assert!(engine.state().quotas.is_empty());
    engine.send(request(Area::Chat, None, SKY, "claude")).unwrap();
    lists_until(&mut rx, WAIT, |items| items.is_empty()).await;
    let quotas = engine.state().quotas;
    let readings: Vec<(Vendor, &str, f64, f64)> = quotas.iter().map(|q| (q.vendor, q.window.as_str(), q.used, q.resets_at)).collect();
    assert_eq!(readings, [(Vendor::Claude, "five_hour", 0.07, 1791627000.0), (Vendor::Claude, "seven_day", 0.47, 1791878400.0)]);
    // A window rebuilt after closing to the tray asks again and gets the same readings.
    assert_eq!(engine.state().quotas, quotas);
    engine.shutdown();
}

#[tokio::test]
async fn the_state_and_the_event_reach_the_screen_in_camel_case() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], fake(fixture("claude", "tools")).spawn, None);
    let state = serde_json::to_value(engine.state()).unwrap();
    assert_eq!(state["running"], serde_json::json!({ "revision": 0, "items": [] }));
    assert_eq!(state["quotas"], serde_json::json!([]));

    let id = engine.send(request(Area::Chat, None, TWO_WRITES, "claude")).unwrap();
    let lists = lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await;
    let (revision, items) = lists.last().unwrap().clone();
    // A window rebuilt while the question waits asks for the state and finds the question, at the revision of the event.
    let rebuilt = engine.state().running;
    engine.shutdown();
    assert_eq!((rebuilt.revision, &rebuilt.items), (revision, &items));
    let event = serde_json::to_value(AppEvent::Running { revision, items: items.clone() }).unwrap();
    assert_eq!((&event["type"], &event["revision"]), (&serde_json::json!("running"), &serde_json::json!(revision)));
    let row = &event["items"][0];
    assert_eq!(
        (&row["conversationId"], &row["state"], &row["area"], &row["vendor"]),
        (&id.clone().into(), &"needsYou".into(), &"chat".into(), &"claude".into())
    );
    assert!(row["turnId"].is_string() && row["since"].is_u64() && row.get("projectId").is_none());
    assert_eq!(row["permissions"][0]["tool"], "Write");
    assert!(row["permissions"][0]["requestId"].is_string());
}

#[tokio::test]
async fn an_auto_reply_is_listed_at_once_without_a_vendor_and_gets_one_when_the_pick_is_made() {
    let dir = tempfile::tempdir().unwrap();
    let ask = router(Some(Duration::from_millis(300)));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Codex], fake(fixture("codex", "chat")).spawn, Some(ask));
    engine.send(request(Area::Chat, None, SKY, "auto")).unwrap();
    let at_once = engine.state().running;
    assert_eq!(at_once.items.len(), 1);
    let waiting = &at_once.items[0];
    assert_eq!((waiting.vendor, waiting.model.as_deref(), waiting.state), (None, None, RunState::Working));

    let lists = lists_until(&mut rx, WAIT, |items| items.first().is_some_and(|i| i.vendor.is_some())).await;
    let picked = &lists.last().unwrap().1[0];
    assert_eq!((picked.vendor, picked.model.as_deref()), (Some(Vendor::Codex), Some("gpt-6-luna")));
    assert_eq!(picked.turn_id, waiting.turn_id, "the same row, not a new one");
    lists_until(&mut rx, WAIT, |items| items.is_empty()).await;
    engine.shutdown();
}

#[tokio::test]
async fn stopping_while_auto_still_chooses_drops_the_row() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Codex], fake(fixture("codex", "chat")).spawn, Some(router(None)));
    let id = engine.send(request(Area::Chat, None, SKY, "auto")).unwrap();
    assert_eq!(engine.state().running.items.len(), 1);
    engine.interrupt(&id);
    assert!(engine.state().running.items.is_empty());
    let lists = lists_until(&mut rx, WAIT, |items| items.is_empty()).await;
    assert_eq!(lists.len(), 2, "the row appeared and then left: {lists:?}");
    assert_eq!(engine.open(&id).unwrap().messages[1].status, Some(Status::Stopped));
    engine.shutdown();
}

#[tokio::test(start_paused = true)]
async fn a_route_that_never_arrives_falls_back_to_the_default_and_the_row_keeps_running() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Codex], fake(fixture("codex", "tools")).spawn, Some(router(None)));
    engine.send(request(Area::Chat, None, TWO_FILES, "auto")).unwrap();
    // The clock is paused, so the 30 seconds the engine waits for the router pass at once.
    let picked = lists_until(&mut rx, Duration::from_secs(120), |items| items.first().is_some_and(|i| i.vendor.is_some())).await;
    assert_eq!(picked[0].1[0].vendor, None, "the row began without a vendor");
    let fallback = &picked.last().unwrap().1[0];
    assert_eq!((fallback.vendor, fallback.model.as_deref()), (Some(Vendor::Codex), Some("gpt-6.1-sol")), "the account's recommended model");
    let asking = lists_until(&mut rx, Duration::from_secs(120), first_is(RunState::NeedsYou)).await;
    engine.shutdown();
    assert_eq!(asking.last().unwrap().1[0].turn_id, fallback.turn_id, "the same row all along");
    assert_consecutive(&[picked, asking].concat());
}

#[tokio::test]
async fn a_start_that_fails_after_the_pick_ends_the_reply_with_an_error_and_drops_the_row() {
    let dir = tempfile::tempdir().unwrap();
    let ask = router(Some(Duration::from_millis(200)));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Codex], fake(fixture("codex", "chat")).spawn, Some(ask));
    let project = engine.add_project(&dir.path().to_string_lossy());
    let id = engine.send(request(Area::Project, Some(&project.id), SKY, "auto")).unwrap();
    assert_eq!(engine.state().running.items.len(), 1);
    // The folder leaves the app while Auto chooses, so the answer has nowhere to run.
    engine.remove_project(&project.id);
    lists_until(&mut rx, WAIT, |items| items.is_empty()).await;
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.projectGone")));
    engine.shutdown();
}

#[tokio::test]
async fn a_send_that_cannot_start_ends_the_reply_with_an_error_instead_of_leaving_it_running() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _rx) = engine(dir.path(), &[Vendor::Claude], fake(fixture("claude", "chat")).spawn, None);
    let error = engine.send(request(Area::Project, Some("no-such-project"), SKY, "claude")).unwrap_err();
    assert_eq!(error, "error.projectGone");
    assert!(engine.state().running.items.is_empty(), "a reply that never started must not stay in the list");
    let saved = engine.open(&engine.state().conversations[0].id).unwrap();
    assert_eq!((saved.messages[1].status, saved.messages[1].error.as_deref()), (Some(Status::Error), Some("error.projectGone")));
    engine.shutdown();
}

// The tests of `Engine::answer` follow (Task 4).
```

Run:

```bash
cargo test -p faber-engine --test running
```

Expected: does not compile: `RunState`, `RunningItem`, `AppEvent::Running` and `AppState::running`/`quotas` do not exist yet.

- [ ] **Step 3: Add the types**

**Edit** `engine/src/model.rs`. Find:

```rust
/// The user's choices, in one plain file.
```

Replace with:

```rust
/// Where a running conversation stands: answering, or waiting for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    Working,
    NeedsYou,
}

/// One conversation that is answering now or waits for the user: a row of the running list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningItem {
    pub conversation_id: String,
    /// The id of the turn's reply message. With the request id it names a question, because request ids repeat.
    pub turn_id: String,
    /// Empty while the conversation has none.
    pub title: String,
    pub area: Area,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// None while Auto is still choosing the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<Vendor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub state: RunState,
    /// The `at` of the turn's reply message.
    pub since: u64,
    /// The questions the user has not answered yet.
    pub permissions: Vec<Permission>,
}

/// The running list as the engine last sent it. The revision grows by one with every list the engine emits, so a
/// snapshot and an event can be told apart by age.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Running {
    pub revision: u64,
    pub items: Vec<RunningItem>,
}

/// The latest reading of one quota window of one vendor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaReading {
    pub vendor: Vendor,
    pub window: String,
    pub used: f64,
    /// Seconds since the Unix epoch, as the CLI reports it; 0 when it did not say.
    pub resets_at: f64,
}

/// The user's choices, in one plain file.
```

- [ ] **Step 4: Teach the engine to keep and send the running list**

`Core` keeps the last list it emitted and the latest reading of each quota window (it used to keep only the used share; Auto reads the same number from the reading). `emit_message` and every `List` emission now go through `refresh_running`.

**Edit** `engine/src/engine.rs`. Find:

```rust
use std::collections::{BTreeMap, HashMap};
```

Replace with:

```rust
use std::collections::{BTreeMap, HashMap, HashSet};
```

**Edit** `engine/src/engine.rs`. Find:

```rust
    Quota { vendor: Vendor, window: String, used: f64, resets_at: f64 },
    List,
}
```

Replace with:

```rust
    Quota { vendor: Vendor, window: String, used: f64, resets_at: f64 },
    Running { revision: u64, items: Vec<RunningItem> },
    List,
}
```

**Edit** `engine/src/engine.rs`. Find:

```rust
    pub vendors: Vec<Vendor>,
    pub settings: Settings,
}
```

Replace with:

```rust
    pub vendors: Vec<Vendor>,
    pub settings: Settings,
    /// The running list as last emitted, with its revision.
    pub running: Running,
    /// The latest reading of each vendor's quota windows, with the time each resets. The screen decides which count as
    /// expired; the engine reports what the CLIs said.
    pub quotas: Vec<QuotaReading>,
}
```

**Edit** `engine/src/engine.rs`. Find:

```rust
    /// The latest share of each quota window spent, per vendor.
    used: HashMap<(Vendor, String), f64>,
```

Replace with:

```rust
    /// The latest reading of each quota window, per vendor. Auto prices the models by the share used.
    quotas: HashMap<(Vendor, String), QuotaReading>,
    /// The running list as last emitted.
    running: Running,
```

**Edit** `engine/src/engine.rs`. Find:

```rust
                used: HashMap::new(),
```

Replace with:

```rust
                quotas: HashMap::new(),
                running: Running::default(),
```

**Edit** `engine/src/engine.rs`. Find:

```rust
    fn state(&self) -> AppState {
        AppState {
            conversations: self.store.list().into_iter().filter(|c| !c.title.is_empty()).collect(),
            projects: self.store.projects(),
            vendors: self.vendors(),
            settings: self.store.settings(),
        }
    }
```

Replace with:

```rust
    fn state(&self) -> AppState {
        let mut quotas: Vec<QuotaReading> = self.quotas.values().cloned().collect();
        quotas.sort_by(|a, b| (a.vendor, &a.window).cmp(&(b.vendor, &b.window)));
        AppState {
            conversations: self.store.list().into_iter().filter(|c| !c.title.is_empty()).collect(),
            projects: self.store.projects(),
            vendors: self.vendors(),
            settings: self.store.settings(),
            running: self.running.clone(),
            quotas,
        }
    }
```

**Edit** `engine/src/engine.rs`. Find:

```rust
                if !c.messages.is_empty() {
                    self.save(conv);
                }
                return;
```

Replace with:

```rust
                if !c.messages.is_empty() {
                    self.save(conv);
                }
                // The turn's reply now names its model.
                self.refresh_running();
                return;
```

**Edit** `engine/src/engine.rs`. Find:

```rust
                self.used.insert((vendor, window.clone()), used);
```

Replace with:

```rust
                let reading = QuotaReading { vendor, window: window.clone(), used, resets_at };
                self.quotas.insert((vendor, window.clone()), reading);
```

**Edit** `engine/src/engine.rs`. Find:

```rust
        self.push(conv, turn_id.as_deref(), true);
        (self.emit)(AppEvent::List);
        if self.live.get(conv)
```

Replace with:

```rust
        self.push(conv, turn_id.as_deref(), true);
        self.list_changed();
        if self.live.get(conv)
```

**Edit** `engine/src/engine.rs`. Find:

```rust
    fn emit_message(&self, conv: &str, message_id: &str) {
        if let Some(m) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == message_id)) {
            (self.emit)(AppEvent::Message { conversation_id: conv.to_string(), message: Box::new(m.clone()) });
        }
    }
```

Replace with:

```rust
    fn emit_message(&mut self, conv: &str, message_id: &str) {
        if let Some(m) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == message_id)) {
            (self.emit)(AppEvent::Message { conversation_id: conv.to_string(), message: Box::new(m.clone()) });
        }
        self.refresh_running();
    }

    /// Tells the screen the conversations changed, and the running list with them.
    fn list_changed(&mut self) {
        (self.emit)(AppEvent::List);
        self.refresh_running();
    }

    /// The conversations to show in the running list, in the order the screen draws them: blocks that hold a question
    /// first, then the sidebar's project order with Chat last; inside a block the questions first, oldest first.
    fn running_items(&self) -> Vec<RunningItem> {
        let mut items: Vec<RunningItem> = self.convs.values().filter_map(running_item).collect();
        if items.is_empty() {
            return items;
        }
        // ponytail: projects.json is read on every refresh while something runs; keep the order in memory if a profile shows it.
        let projects = self.store.projects();
        let block = |i: &RunningItem| match (i.area, &i.project_id) {
            (Area::Chat, _) => (usize::MAX, String::new()),
            (Area::Project, Some(id)) => (projects.iter().position(|p| &p.id == id).unwrap_or(usize::MAX - 1), id.clone()),
            (Area::Project, None) => (usize::MAX - 1, String::new()),
        };
        let asking: HashSet<(usize, String)> = items.iter().filter(|i| i.state == RunState::NeedsYou).map(block).collect();
        items.sort_by_cached_key(|i| {
            let block = block(i);
            (!asking.contains(&block), block, i.state == RunState::Working, i.since, i.conversation_id.clone())
        });
        items
    }

    /// Recomputes the running list and emits it with the next revision, but only if it differs from the last one emitted.
    /// It runs wherever the engine emits a `Message` or a `List`, so every change of a turn or a permission passes here.
    fn refresh_running(&mut self) {
        let items = self.running_items();
        if items == self.running.items {
            return;
        }
        self.running = Running { revision: self.running.revision + 1, items: items.clone() };
        (self.emit)(AppEvent::Running { revision: self.running.revision, items });
    }

    /// Ends a reply that never got a CLI: it shows the error and leaves the running list.
    pub(super) fn fail_reply(&mut self, id: &str, reply_id: &str, error: String) {
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.status = Some(Status::Error);
            m.error = Some(error);
        }
        self.save(id);
        self.push(id, Some(reply_id), true);
    }
```

**Edit** `engine/src/engine.rs`. Find:

```rust
        self.save(&id);
        (self.emit)(AppEvent::List);
        // An open conversation only grows
```

Replace with:

```rust
        self.save(&id);
        self.list_changed();
        // An open conversation only grows
```

**Edit** `engine/src/engine.rs`. Find:

```rust
        self.start(&id, &reply_id, &pick, text, req.images)?;
        Ok(id)
```

Replace with:

```rust
        if let Err(error) = self.start(&id, &reply_id, &pick, text, req.images) {
            // The reply never got a CLI: it ends with the error instead of staying in the running list for good.
            self.fail_reply(&id, &reply_id, error.clone());
            return Err(error);
        }
        Ok(id)
```

**Edit** `engine/src/engine.rs`. Find:

```rust
        self.save(id);
        (self.emit)(AppEvent::List);
    }

    fn remove(&mut self, id: &str) {
```

Replace with:

```rust
        self.save(id);
        self.list_changed();
    }

    fn remove(&mut self, id: &str) {
```

**Edit** `engine/src/engine.rs`. Find:

```rust
            let _ = std::fs::remove_dir_all(self.opts.data_dir.join("attachments").join(id));
        }
        (self.emit)(AppEvent::List);
```

Replace with:

```rust
            let _ = std::fs::remove_dir_all(self.opts.data_dir.join("attachments").join(id));
        }
        self.list_changed();
```

**Edit** `engine/src/engine.rs`. Find:

```rust
        self.store.save_projects(&projects);
        (self.emit)(AppEvent::List);
        project
```

Replace with:

```rust
        self.store.save_projects(&projects);
        self.list_changed();
        project
```

**Edit** `engine/src/engine.rs`. Find:

```rust
            c.store.save_projects(&projects);
            (c.emit)(AppEvent::List);
```

Replace with:

```rust
            c.store.save_projects(&projects);
            c.list_changed();
```

**Edit** `engine/src/engine.rs`. Find:

```rust
fn copy_dir(from: &Path, to: &Path) {
```

Replace with:

```rust
/// The row of the running list for a conversation, if its latest reply is streaming or has unanswered permissions.
fn running_item(c: &Conversation) -> Option<RunningItem> {
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant)?;
    let permissions = reply.permissions.clone().unwrap_or_default();
    if reply.status != Some(Status::Streaming) && permissions.is_empty() {
        return None;
    }
    Some(RunningItem {
        conversation_id: c.id.clone(),
        turn_id: reply.id.clone(),
        title: c.title.clone(),
        area: c.area,
        project_id: c.project_id.clone(),
        vendor: reply.vendor,
        model: reply.model.clone(),
        state: if permissions.is_empty() { RunState::Working } else { RunState::NeedsYou },
        since: reply.at,
        permissions,
    })
}

fn copy_dir(from: &Path, to: &Path) {
```

Auto reads the used share from the reading now, and a failed start after the pick uses the same `fail_reply` as `send`.

**Edit** `engine/src/engine/auto.rs`. Find:

```rust
self.used.iter().filter(|((v, _), _)| *v == vendor).map(|(_, u)| *u).fold(0.0, f64::max);
```

Replace with:

```rust
self.quotas.iter().filter(|((v, _), _)| *v == vendor).map(|(_, q)| q.used).fold(0.0, f64::max);
```

**Edit** `engine/src/engine/auto.rs`. Find:

```rust
        if let Err(error) = self.start(id, reply_id, &pick, text, images) {
            if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
                m.status = Some(Status::Error);
                m.error = Some(error);
            }
            self.save(id);
            self.push(id, Some(reply_id), true);
        }
```

Replace with:

```rust
        if let Err(error) = self.start(id, reply_id, &pick, text, images) {
            self.fail_reply(id, reply_id, error);
        }
```

- [ ] **Step 5: Run the tests**

Run:

```bash
cargo test -p faber-engine --test running
```

Expected: all tests in `running.rs` pass.

Run:

```bash
cargo test -p faber-engine
```

Expected: the whole engine suite passes.

---

### Task 4: `Engine::answer`: turn id, acknowledgement, stale and undelivered  (model: `sonnet`)

**Files:**
- Create: `engine/src/engine/answer.rs` (code and its in-crate tests)
- Modify: `engine/src/engine.rs`, `src-tauri/src/main.rs`
- Test: `engine/tests/engine.rs` (three callers ported), `engine/tests/running.rs` (the answer tests added)

**Interfaces:**
- Consumes: Task 2's `Delivery` and `Session::answer`; Task 3's `RunningItem`, `RunState`, `Core::refresh_running`, `Core::push`, and the helpers of `engine/tests/running.rs`
- Produces: `Engine::answer(&self, conv: &str, turn_id: &str, request_id: &str, allow: bool, always: bool) -> Result<(), String>` (an `async fn`; the errors are the keys `error.permission.stale` and `error.permission.undelivered`); the shell command `answer(id, turn_id, request_id, allow, always)`.

- [ ] **Step 1: Port the callers in the engine tests and write the new tests**

`answer` becomes `async` and takes the turn id, so a test that answers from a synchronous `finished` callback must answer in the background. `engine/tests/engine.rs` gets a small helper for that, and its three callers use it with the id of the message that holds the card.

**Edit** `engine/tests/engine.rs`. Find:

```rust
fn ask(area: Area, text: &str, choice: Choice) -> SendRequest {
    SendRequest { conversation_id: None, area, project_id: None, text: text.into(), images: vec![], files: vec![], choice }
}
```

Replace with:

```rust
fn ask(area: Area, text: &str, choice: Choice) -> SendRequest {
    SendRequest { conversation_id: None, area, project_id: None, text: text.into(), images: vec![], files: vec![], choice }
}

/// Answers the way the screen does, in the background: the engine confirms an answer only once the CLI has it, and
/// the callbacks of `finished` cannot wait. `m` is the message that holds the card.
fn answer(engine: &Engine, id: &str, m: &Message, request_id: &str, allow: bool, always: bool) {
    let (engine, id, turn, request) = (engine.clone(), id.to_string(), m.id.clone(), request_id.to_string());
    tokio::spawn(async move {
        let _ = engine.answer(&id, &turn, &request, allow, always).await;
    });
}
```

**Edit** `engine/tests/engine.rs`. Find:

```rust
            engine.answer(&id, &p.request_id, true, true);
```

Replace with:

```rust
            answer(&engine, &id, m, &p.request_id, true, true);
```

**Edit** `engine/tests/engine.rs`. Find:

```rust
            engine.answer(&id, &first.request_id, true, false);
```

Replace with:

```rust
            answer(&engine, &id, m, &first.request_id, true, false);
```

**Edit** `engine/tests/engine.rs`. Find:

```rust
            engine.answer(&id, &p.request_id, allow, false);
```

Replace with:

```rust
            answer(&engine, &id, m, &p.request_id, allow, false);
```

In `engine/tests/running.rs`, the first test awaits the new call, and the tests of `Engine::answer` against the real sessions take the place of the marker on the last line.

**Edit** `engine/tests/running.rs`. Find:

```rust
        engine.answer(&id, &item.permissions[0].request_id, round == 0, false);
```

Replace with:

```rust
        engine.answer(&id, &item.turn_id, &item.permissions[0].request_id, round == 0, false).await.unwrap();
```

**Edit** `engine/tests/running.rs`. Find:

```rust

// The tests of `Engine::answer` follow (Task 4).
```

Replace with:

```rust

/// Answers a question as the screen does: by the ids of the conversation, the turn and the request.
async fn answer(engine: &Engine, item: &RunningItem, allow: bool) -> Result<(), String> {
    engine.answer(&item.conversation_id, &item.turn_id, &item.permissions[0].request_id, allow, false).await
}

fn only(rows: &[RunningItem], conversation: &str) -> RunningItem {
    rows.iter().find(|i| i.conversation_id == conversation).expect("the conversation is listed").clone()
}

/// Whether the conversation holds the question `request_id`.
fn asks(rows: &[RunningItem], conversation: &str, request_id: &str) -> bool {
    rows.iter().any(|i| i.conversation_id == conversation && i.permissions.iter().any(|p| p.request_id == request_id))
}

#[tokio::test]
async fn request_ids_that_repeat_in_two_conversations_are_two_questions() {
    let dir = tempfile::tempdir().unwrap();
    // A chat and a project use two Codex processes, and each numbers its approvals from 0.
    let f = fake(fixture("codex", "tools"));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Codex], f.spawn.clone(), None);
    let project = engine.add_project(&dir.path().to_string_lossy());
    let chat = engine.send(request(Area::Chat, None, TWO_FILES, "codex")).unwrap();
    let work = engine.send(request(Area::Project, Some(&project.id), TWO_FILES, "codex")).unwrap();

    for round in 0..2 {
        let wanted = round.to_string();
        let both_ask = |rows: &[RunningItem]| {
            rows.len() == 2 && rows.iter().all(|i| i.state == RunState::NeedsYou && i.permissions[0].request_id == wanted)
        };
        let rows = lists_until(&mut rx, WAIT, both_ask).await.pop().unwrap().1;
        let (in_chat, in_project) = (only(&rows, &chat), only(&rows, &work));
        assert_eq!(in_chat.permissions[0].request_id, in_project.permissions[0].request_id, "the same id twice");
        assert_ne!(in_chat.turn_id, in_project.turn_id);

        // The recording allowed the first approval and declined the second.
        answer(&engine, &in_chat, round == 0).await.unwrap();
        let after = engine.state().running.items;
        assert!(!asks(&after, &chat, &wanted), "the answered question is gone");
        assert!(asks(&after, &work, &wanted), "the other conversation's question, with the same id, still waits");
        answer(&engine, &in_project, round == 0).await.unwrap();
    }
    lists_until(&mut rx, WAIT, |rows| rows.is_empty()).await;
    engine.shutdown();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn a_second_answer_to_the_same_request_is_stale_and_the_cli_gets_one_line() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "tools"));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], f.spawn.clone(), None);
    engine.send(request(Area::Chat, None, TWO_WRITES, "claude")).unwrap();
    let item = lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await.pop().unwrap().1.remove(0);

    // Both copies of the card are clicked before either answer is confirmed.
    let (first, second) = tokio::join!(answer(&engine, &item, true), answer(&engine, &item, true));
    engine.shutdown();
    assert_eq!((first, second), (Ok(()), Err("error.permission.stale".to_string())));
    assert_eq!(f.wire.lines_with("control_response"), 1);
}

#[tokio::test]
async fn an_answer_that_names_the_previous_turn_is_stale_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    // Both turns ask for request `req-1`.
    let f = fake(support::claude_two_turns("req-1"));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], f.spawn.clone(), None);
    let id = engine.send(request(Area::Chat, None, "first", "claude")).unwrap();
    let one = lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await.pop().unwrap().1.remove(0);
    answer(&engine, &one, true).await.unwrap();
    lists_until(&mut rx, WAIT, |rows| rows.is_empty()).await;

    engine.send(SendRequest { conversation_id: Some(id.clone()), ..request(Area::Chat, None, "second", "claude") }).unwrap();
    let two = lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await.pop().unwrap().1.remove(0);
    assert_eq!((two.permissions[0].request_id.as_str(), two.turn_id != one.turn_id), ("req-1", true));
    let written = f.wire.lines_with("control_response");

    assert_eq!(answer(&engine, &one, true).await, Err("error.permission.stale".to_string()));
    assert_eq!(f.wire.lines_with("control_response"), written, "nothing was written");
    assert_eq!(engine.state().running.items[0].permissions.len(), 1, "turn 2 still waits");

    answer(&engine, &two, true).await.unwrap();
    lists_until(&mut rx, WAIT, |rows| rows.is_empty()).await;
    engine.shutdown();
    assert_eq!(f.wire.lines_with("control_response"), 2);
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn an_answer_the_cli_cannot_take_is_undelivered_and_the_same_card_can_be_answered_again() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "tools"));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], f.spawn.clone(), None);
    engine.send(request(Area::Chat, None, TWO_WRITES, "claude")).unwrap();
    let rows = lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await.pop().unwrap();

    f.wire.refuse_next(1);
    assert_eq!(answer(&engine, &rows.1[0], true).await, Err("error.permission.undelivered".to_string()));
    let kept = engine.state().running;
    assert_eq!((kept.revision, &kept.items), (rows.0, &rows.1), "the card and the revision are as they were");

    answer(&engine, &rows.1[0], true).await.unwrap();
    assert_eq!(f.wire.lines_with("control_response"), 1);
    engine.shutdown();
}

#[tokio::test]
async fn interrupting_a_running_conversation_drops_its_row() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "interrupt"));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], f.spawn.clone(), None);
    let id = engine.send(request(Area::Chat, None, "Write a 600-word essay about the history of bread.", "claude")).unwrap();
    lists_until(&mut rx, WAIT, |rows| !rows.is_empty()).await;
    engine.interrupt(&id);
    lists_until(&mut rx, WAIT, |rows| rows.is_empty()).await;
    assert_eq!(engine.open(&id).unwrap().messages[1].status, Some(Status::Stopped));
    engine.shutdown();
}

#[tokio::test]
async fn removing_a_running_conversation_drops_its_row() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], fake(fixture("claude", "tools")).spawn, None);
    let id = engine.send(request(Area::Chat, None, TWO_WRITES, "claude")).unwrap();
    lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await;
    engine.remove(&id);
    lists_until(&mut rx, WAIT, |rows| rows.is_empty()).await;
    assert!(engine.open(&id).is_none());
    engine.shutdown();
}
```

Run:

```bash
cargo test -p faber-engine --test running
```

Expected: does not compile: `Engine::answer` takes four arguments and is not `async`.

- [ ] **Step 2: Replace the old answer with the acknowledged one**

`Core::answer` (which removed the permission at once and forgot the write) and `Engine::answer` (which wrapped it) go. `engine/src/engine/answer.rs` holds `Engine::answer`, `Core::prepare_answer`, `Core::settle_answer` and their tests, with stand-in sessions built by `Session::from_fn`. The module is a child of `engine`, so it reaches `Core`'s private fields the way `auto.rs` does.

**Edit** `engine/src/engine.rs`. Find:

```rust
mod auto;
```

Replace with:

```rust
mod answer;
mod auto;
```

**Edit** `engine/src/engine.rs`. Find:

```rust
    pub fn answer(&self, id: &str, request_id: &str, allow: bool, always: bool) {
        self.with(|c| c.answer(id, request_id, allow, always))
    }
```

Replace with:

```rust

```

**Edit** `engine/src/engine.rs`. Find:

```rust
    fn answer(&mut self, conv: &str, request_id: &str, allow: bool, always: bool) {
        let Some(turn_id) = self.live.get(conv).and_then(|l| l.turn.clone()) else { return };
        let Some(m) = self.turn(conv) else { return };
        let waiting = m.permissions.clone().unwrap_or_default();
        let tool = waiting.iter().find(|p| p.request_id == request_id).map(|p| p.tool.clone());
        // "Always" also answers the other waiting requests for the same tool.
        let answered: Vec<String> = match (&tool, always && allow) {
            (Some(tool), true) => waiting.iter().filter(|p| &p.tool == tool).map(|p| p.request_id.clone()).collect(),
            _ => waiting.iter().filter(|p| p.request_id == request_id).map(|p| p.request_id.clone()).collect(),
        };
        m.permissions = Some(waiting.into_iter().filter(|p| !answered.contains(&p.request_id)).collect());
        if let (Some(tool), true) = (tool, always && allow)
            && let Some(c) = self.convs.get_mut(conv)
        {
            let list = c.allow_always.get_or_insert_default();
            if !list.contains(&tool) {
                list.push(tool);
            }
            self.save(conv);
        }
        if let Some(l) = self.live.get(conv) {
            for id in &answered {
                drop(l.session.answer(id, allow));
            }
        }
        self.push(conv, Some(&turn_id), true);
    }

    fn rename(&mut self, id: &str, title: &str) {
```

Replace with:

```rust
    fn rename(&mut self, id: &str, title: &str) {
```

**Write the whole file** `engine/src/engine/answer.rs`:

```rust
//! Answering a permission question. The engine hands the answer to the session, waits for the session to say the CLI
//! has it, and takes the question off the list only then, so the user is never told a request was answered when it was
//! not.

use super::*;
use crate::events::Delivery;
use tokio::sync::oneshot;
use tokio::time::{Instant, timeout_at};

/// How long the engine waits, in all, for the sessions to confirm one answer.
const ACK_WAIT: Duration = Duration::from_secs(5);

const STALE: &str = "error.permission.stale";
const UNDELIVERED: &str = "error.permission.undelivered";

/// An answer given to the sessions and not yet settled.
pub(super) struct Handed {
    /// Each request answered with the receiver of its acknowledgement. The request the user clicked comes first.
    requests: Vec<(String, oneshot::Receiver<Delivery>)>,
    /// The tool of the request the user clicked.
    tool: String,
}

impl Engine {
    /// Answers one question of one turn. The call returns once the CLI has the answer (`Ok`), or says why not:
    /// `error.permission.stale` when the question is not waiting any more, `error.permission.undelivered` when the
    /// answer could not be written or was not confirmed in time. The lock is free while the engine waits, so nothing
    /// else in the engine waits for the CLI.
    pub async fn answer(&self, conv: &str, turn_id: &str, request_id: &str, allow: bool, always: bool) -> Result<(), String> {
        let handed = self.with(|c| c.prepare_answer(conv, turn_id, request_id, allow, always))?;
        let deadline = Instant::now() + ACK_WAIT;
        let mut results = Vec::new();
        for (id, ack) in handed.requests {
            // A queue that closed, or a session that said nothing in time, leaves the request waiting.
            results.push((id, timeout_at(deadline, ack).await.ok().and_then(|r| r.ok())));
        }
        self.with(|c| c.settle_answer(conv, turn_id, &handed.tool, allow && always, &results))
    }
}

impl Core {
    /// Checks that the conversation is live, that `turn_id` is its current turn and that the request waits in it, then
    /// hands the answer to the session. It changes no permission. "Always" also answers the other waiting requests for
    /// the same tool.
    fn prepare_answer(&mut self, conv: &str, turn_id: &str, request_id: &str, allow: bool, always: bool) -> Result<Handed, String> {
        let live = self.live.get(conv).filter(|l| l.turn.as_deref() == Some(turn_id)).ok_or(STALE)?;
        let reply = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == turn_id)).ok_or(STALE)?;
        let waiting = reply.permissions.as_deref().unwrap_or_default();
        let tool = waiting.iter().find(|p| p.request_id == request_id).ok_or(STALE)?.tool.clone();
        let others = waiting.iter().filter(|p| allow && always && p.tool == tool && p.request_id != request_id);
        let ids = std::iter::once(request_id).chain(others.map(|p| p.request_id.as_str()));
        Ok(Handed { requests: ids.map(|id| (id.to_string(), live.session.answer(id, allow))).collect(), tool })
    }

    /// Applies what the sessions said. Other calls ran while the engine waited, so it looks again for the conversation
    /// and the turn. A request leaves the list when the CLI took the answer or no longer waits for it; one that was not
    /// delivered stays, so the user can answer again. With `remember`, the tool joins the conversation's allow list once
    /// the clicked request was delivered. The result is the clicked request's, the first of `results`.
    fn settle_answer(
        &mut self,
        conv: &str,
        turn_id: &str,
        tool: &str,
        remember: bool,
        results: &[(String, Option<Delivery>)],
    ) -> Result<(), String> {
        let c = self.convs.get_mut(conv).ok_or(STALE)?;
        let reply = c.messages.iter_mut().find(|m| m.id == turn_id).ok_or(STALE)?;
        let clicked = results.first().and_then(|r| r.1);
        let over = |id: &str| results.iter().any(|(r, d)| r == id && matches!(d, Some(Delivery::Delivered | Delivery::Unknown)));
        let mut changed = false;
        if let Some(waiting) = reply.permissions.as_mut() {
            let before = waiting.len();
            waiting.retain(|p| !over(&p.request_id));
            changed = waiting.len() != before;
        }
        if remember && clicked == Some(Delivery::Delivered) {
            let list = c.allow_always.get_or_insert_default();
            if !list.iter().any(|t| t == tool) {
                list.push(tool.to_string());
                changed = true;
            }
        }
        if changed {
            self.save(conv);
            self.push(conv, Some(turn_id), true);
        }
        match clicked {
            Some(Delivery::Delivered) => Ok(()),
            Some(Delivery::Unknown) => Err(STALE.into()),
            Some(Delivery::WriteFailed) | None => Err(UNDELIVERED.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::events::SessionCommand;
    use serde_json::json;
    use std::collections::VecDeque;

    /// What a stand-in session does with the next answer it is given.
    enum Reply {
        Say(Delivery),
        /// The queue is closed: the command, and the acknowledgement in it, are dropped.
        Closed,
        /// The session takes the command and says nothing yet; the test holds the acknowledgement.
        Hold,
    }

    /// A session the test controls, and what it was asked.
    struct Stub {
        session: Session,
        asked: Arc<Mutex<Vec<String>>>,
        held: Arc<Mutex<Vec<oneshot::Sender<Delivery>>>>,
    }

    /// A stand-in that follows `script`, one reply per answer, and then says `Delivered`.
    fn stub(script: Vec<Reply>) -> Stub {
        let (asked, held) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
        let (log, keep, script) = (asked.clone(), held.clone(), Mutex::new(VecDeque::from(script)));
        let session = Session::from_fn(move |cmd| {
            if let SessionCommand::Answer { request_id, ack, .. } = cmd {
                log.lock().unwrap().push(request_id);
                match script.lock().unwrap().pop_front().unwrap_or(Reply::Say(Delivery::Delivered)) {
                    Reply::Say(delivery) => {
                        let _ = ack.send(delivery);
                    }
                    Reply::Closed => {}
                    Reply::Hold => keep.lock().unwrap().push(ack),
                }
            }
        });
        Stub { session, asked, held }
    }

    fn engine(dir: &Path) -> (Engine, Arc<Mutex<Vec<AppEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let opts = EngineOptions {
            data_dir: dir.into(),
            clis: BTreeMap::new(),
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no process in this test"))),
            windows: false,
            max_warm: 1,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        };
        (Engine::new(opts, Arc::new(move |e| sink.lock().unwrap().push(e))), events)
    }

    /// A chat whose replies are `turns`, each a message id with the (request id, tool) pairs it waits on. The last is
    /// the turn in flight, on a live `session`.
    fn seed(engine: &Engine, session: Session, turns: &[(&str, &[(&str, &str)])]) -> String {
        engine.with(|c| {
            let id = c.blank(Area::Chat, None);
            let conv = c.convs.get_mut(&id).unwrap();
            conv.title = "Seeded".into();
            for (n, (turn, waiting)) in turns.iter().enumerate() {
                conv.messages.push(Message::new(format!("question-{n}"), Role::User, "question".into(), 1));
                let mut reply = Message::new((*turn).into(), Role::Assistant, String::new(), 2 + n as u64);
                reply.vendor = Some(Vendor::Claude);
                reply.status = Some(if n + 1 == turns.len() { Status::Streaming } else { Status::Done });
                let ask = |(request_id, tool): &(&str, &str)| Permission {
                    request_id: request_id.to_string(),
                    tool: tool.to_string(),
                    input: json!({}),
                };
                reply.permissions = (!waiting.is_empty()).then(|| waiting.iter().map(ask).collect());
                conv.messages.push(reply);
            }
            let live = Live {
                vendor: Vendor::Claude,
                model: None,
                running: None,
                effort: "medium".into(),
                sid: 1,
                session,
                turn: turns.last().map(|t| t.0.to_string()),
                break_pending: false,
                used: 0,
                cli: Cli { command: "claude".into(), prefix: vec![] },
            };
            c.live.insert(id.clone(), live);
            c.refresh_running();
            id
        })
    }

    /// The request ids waiting in the message `turn`.
    fn waiting(engine: &Engine, conv: &str, turn: &str) -> Vec<String> {
        let reply = engine.open(conv).unwrap().messages.into_iter().find(|m| m.id == turn).unwrap();
        reply.permissions.unwrap_or_default().into_iter().map(|p| p.request_id).collect()
    }

    fn revision(engine: &Engine) -> u64 {
        engine.state().running.revision
    }

    fn running_events(events: &Mutex<Vec<AppEvent>>) -> usize {
        events.lock().unwrap().iter().filter(|e| matches!(e, AppEvent::Running { .. })).count()
    }

    #[tokio::test]
    async fn a_delivered_answer_takes_the_question_off_the_list_and_the_row_goes_back_to_working() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, events) = engine(dir.path());
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.state().running.items[0].state, RunState::NeedsYou);
        let before = (revision(&engine), running_events(&events));

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Ok(()));

        assert_eq!(*s.asked.lock().unwrap(), ["0"]);
        assert!(waiting(&engine, &id, "t1").is_empty());
        assert_eq!(engine.state().running.items[0].state, RunState::Working);
        assert_eq!((revision(&engine), running_events(&events)), (before.0 + 1, before.1 + 1));
        assert_eq!(engine.open(&id).unwrap().allow_always, None, "no Always was asked for");
    }

    #[tokio::test]
    async fn a_session_that_does_not_know_the_request_makes_it_stale_and_the_question_goes() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::Unknown)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(STALE.to_string()));
        assert!(waiting(&engine, &id, "t1").is_empty(), "the CLI no longer waits for it");
    }

    #[tokio::test]
    async fn a_write_that_fails_keeps_the_question_and_the_revision_and_a_second_answer_can_land() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, events) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::WriteFailed), Reply::Say(Delivery::Delivered)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let before = (revision(&engine), running_events(&events));

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(UNDELIVERED.to_string()));
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
        assert_eq!((revision(&engine), running_events(&events)), before, "nothing changed, so nothing was said");

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Ok(()));
        assert!(waiting(&engine, &id, "t1").is_empty());
        assert_eq!(revision(&engine), before.0 + 1);
    }

    #[tokio::test]
    async fn a_closed_queue_is_undelivered_and_the_question_stays() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Closed]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(UNDELIVERED.to_string()));
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
    }

    #[tokio::test(start_paused = true)]
    async fn no_acknowledgement_in_five_seconds_is_undelivered_and_a_late_one_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, events) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let before = (revision(&engine), running_events(&events));

        let started = tokio::time::Instant::now();
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(UNDELIVERED.to_string()));
        assert_eq!(started.elapsed(), ACK_WAIT);

        let late = s.held.lock().unwrap().pop().unwrap();
        assert!(late.send(Delivery::Delivered).is_err(), "nobody listens to the late acknowledgement");
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
        assert_eq!((revision(&engine), running_events(&events)), before);
    }

    #[tokio::test]
    async fn the_turn_id_is_checked_so_a_late_answer_never_lands_on_the_next_turn() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![]);
        // Turn 1 ended with request 0 still listed; turn 2 asks its own request 0.
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")]), ("t2", &[("0", "Write")])]);

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(STALE.to_string()));
        assert!(s.asked.lock().unwrap().is_empty(), "nothing was sent");
        assert_eq!(waiting(&engine, &id, "t2"), ["0"], "turn 2 still waits");

        assert_eq!(engine.answer(&id, "t2", "0", true, false).await, Ok(()));
        assert!(waiting(&engine, &id, "t2").is_empty());
        assert_eq!(*s.asked.lock().unwrap(), ["0"]);
    }

    #[tokio::test]
    async fn an_unknown_conversation_turn_or_request_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        for (conv, turn, request) in [("nope", "t1", "0"), (id.as_str(), "nope", "0"), (id.as_str(), "t1", "9")] {
            assert_eq!(engine.answer(conv, turn, request, true, false).await, Err(STALE.to_string()), "{conv} {turn} {request}");
        }
        assert!(s.asked.lock().unwrap().is_empty());
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
    }

    #[tokio::test]
    async fn always_answers_the_other_waiting_requests_of_the_tool_and_each_has_its_own_acknowledgement() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::Delivered), Reply::Say(Delivery::Unknown)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write"), ("b", "Bash"), ("c", "Write")])]);

        assert_eq!(engine.answer(&id, "t1", "a", true, true).await, Ok(()));

        assert_eq!(*s.asked.lock().unwrap(), ["a", "c"], "the clicked request first, then the other Write");
        assert_eq!(waiting(&engine, &id, "t1"), ["b"], "c was unknown to the CLI, so it goes too");
        assert_eq!(engine.open(&id).unwrap().allow_always, Some(vec!["Write".to_string()]));
    }

    #[tokio::test]
    async fn deny_with_always_answers_only_the_clicked_request_and_remembers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write"), ("c", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "a", false, true).await, Ok(()));
        assert_eq!(*s.asked.lock().unwrap(), ["a"]);
        assert_eq!(waiting(&engine, &id, "t1"), ["c"]);
        assert_eq!(engine.open(&id).unwrap().allow_always, None);
    }

    #[tokio::test]
    async fn an_always_that_was_not_delivered_does_not_join_the_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::WriteFailed)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "a", true, true).await, Err(UNDELIVERED.to_string()));
        assert_eq!(engine.open(&id).unwrap().allow_always, None);
    }

    #[tokio::test]
    async fn two_clicks_on_the_two_copies_of_a_card_give_one_ok_and_one_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        // Both clicks pass the first check, because the lock is free while they wait. The session handles the commands in
        // order: the first is delivered, the second finds nothing.
        let s = stub(vec![Reply::Hold, Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let release = async {
            while s.held.lock().unwrap().len() < 2 {
                tokio::task::yield_now().await;
            }
            let mut held = s.held.lock().unwrap();
            let _ = held.remove(0).send(Delivery::Delivered);
            let _ = held.remove(0).send(Delivery::Unknown);
        };
        let (first, second, ()) =
            tokio::join!(engine.answer(&id, "t1", "0", true, false), engine.answer(&id, "t1", "0", true, false), release);
        assert_eq!((first, second), (Ok(()), Err(STALE.to_string())));
        assert_eq!(s.asked.lock().unwrap().len(), 2);
        assert!(waiting(&engine, &id, "t1").is_empty());
    }

    #[tokio::test]
    async fn a_conversation_removed_while_the_engine_waits_makes_the_answer_stale_without_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let call = tokio::spawn({
            let (engine, id) = (engine.clone(), id.clone());
            async move { engine.answer(&id, "t1", "0", true, false).await }
        });
        while s.held.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        engine.remove(&id);
        s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();
        assert_eq!(call.await.unwrap(), Err(STALE.to_string()));
        assert!(engine.state().running.items.is_empty());
    }

    #[tokio::test]
    async fn the_engine_is_not_blocked_while_it_waits_for_an_acknowledgement() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let call = tokio::spawn({
            let (engine, id) = (engine.clone(), id.clone());
            async move { engine.answer(&id, "t1", "0", true, false).await }
        });
        while s.held.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        // Another thread uses the engine while the answer waits; if the lock were held, it would not come back.
        let (done, back) = std::sync::mpsc::channel();
        std::thread::spawn({
            let engine = engine.clone();
            move || {
                let listed = engine.state().running.items.len();
                let sent = engine.send(SendRequest {
                    conversation_id: None,
                    area: Area::Chat,
                    project_id: None,
                    text: "hello".into(),
                    images: vec![],
                    files: vec![],
                    choice: Choice { vendor: "claude".into(), model: None },
                });
                let _ = done.send((listed, sent));
            }
        });
        let (listed, sent) = back.recv_timeout(Duration::from_secs(5)).expect("state() and send() waited for the answer");
        assert_eq!((listed, sent), (1, Err("error.noSubscription".to_string())));
        s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();
        assert_eq!(call.await.unwrap(), Ok(()));
    }
}
```

Run:

```bash
cargo test -p faber-engine
```

Expected: every suite passes, including the new tests in `engine/src/engine/answer.rs` and `engine/tests/running.rs`.

- [ ] **Step 3: Pass the turn id and the result through the shell**

**Edit** `src-tauri/src/main.rs`. Find:

```rust
async fn answer(faber: State<'_, Faber>, id: String, request_id: String, allow: bool, always: bool) -> Res<()> {
    faber.engine.answer(&id, &request_id, allow, always);
    Ok(())
}
```

Replace with:

```rust
async fn answer(faber: State<'_, Faber>, id: String, turn_id: String, request_id: String, allow: bool, always: bool) -> Res<()> {
    faber.engine.answer(&id, &turn_id, &request_id, allow, always).await
}
```

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: no warnings.

---

### Task 5: Port the one screen caller of `answer`  (model: `haiku`)

**Files:**
- Modify: `ui/api.ts`, `ui/Thread.tsx`, `ui/App.tsx`

**Interfaces:**
- Consumes: the shell command of Task 4, called by `invoke` with the keys `id`, `turnId`, `requestId`, `allow`, `always`
- Produces: `api.answer(id, turnId, requestId, allow, always = false): Promise<void>`; the `Thread` prop `onAnswer(turnId, requestId, allow, always?)`.

- [ ] **Step 1: Pass the turn id from the card to the engine**

The shell command now needs the turn id. The card knows it: it is rendered inside the reply message `m()`, and that message's id is the turn's id. `api.answer` takes it, `Thread.tsx` passes it up from the card, and `App.tsx` forwards it and ignores a rejection, which is what happens today to an answer that went nowhere. The messages for `error.permission.stale` and `error.permission.undelivered` come with the screen in B5b.

**Edit** `ui/api.ts`. Find:

```ts
  answer: (id: string, requestId: string, allow: boolean, always = false) => invoke<void>('answer', { id, requestId, allow, always }),
```

Replace with:

```ts
  answer: (id: string, turnId: string, requestId: string, allow: boolean, always = false) =>
    invoke<void>('answer', { id, turnId, requestId, allow, always }),
```

**Edit** `ui/Thread.tsx`. Find:

```tsx
type OnAnswer = (requestId: string, allow: boolean, always?: boolean) => void;
```

Replace with:

```tsx
/** `turnId` is the id of the message that holds the card. */
type OnAnswer = (turnId: string, requestId: string, allow: boolean, always?: boolean) => void;
```

**Edit** `ui/Thread.tsx`. Find:

```tsx
function Ask(props: { permission: Permission; onAnswer: OnAnswer }) {
```

Replace with:

```tsx
function Ask(props: { permission: Permission; turnId: string; onAnswer: OnAnswer }) {
```

**Edit** `ui/Thread.tsx`. Find:

```tsx
        <button class="btn primary" onClick={() => props.onAnswer(props.permission.requestId, true)}>
```

Replace with:

```tsx
        <button class="btn primary" onClick={() => props.onAnswer(props.turnId, props.permission.requestId, true)}>
```

**Edit** `ui/Thread.tsx`. Find:

```tsx
        <button class="btn" onClick={() => props.onAnswer(props.permission.requestId, true, true)}>
```

Replace with:

```tsx
        <button class="btn" onClick={() => props.onAnswer(props.turnId, props.permission.requestId, true, true)}>
```

**Edit** `ui/Thread.tsx`. Find:

```tsx
        <button class="btn" onClick={() => props.onAnswer(props.permission.requestId, false)}>
```

Replace with:

```tsx
        <button class="btn" onClick={() => props.onAnswer(props.turnId, props.permission.requestId, false)}>
```

**Edit** `ui/Thread.tsx`. Find:

```tsx
        {(p) => <Ask permission={p} onAnswer={props.onAnswer} />}
```

Replace with:

```tsx
        {(p) => <Ask permission={p} turnId={m().id} onAnswer={props.onAnswer} />}
```

**Edit** `ui/App.tsx`. Find:

```tsx
                      onAnswer={(requestId, allow, always) => void api.answer(c().id, requestId, allow, always)}
```

Replace with:

```tsx
                      onAnswer={(turnId, requestId, allow, always) => void api.answer(c().id, turnId, requestId, allow, always).catch(() => {})}
```

Run:

```bash
npm run lint
```

Expected: exit 0.

Run:

```bash
npm run typecheck
```

Expected: exit 0. The three callers agree on the new signature.

Run:

```bash
npm run build
```

Expected: exit 0.

---

### Task 6: A screen scenario for the ported answer  (model: `sonnet`)

**Files:**
- Create: `testkit/src/screen/permissions.rs`
- Modify: `testkit/src/screen/mod.rs`

**Interfaces:**
- Consumes: Tasks 2 to 5 and the production `dist/` (`npm run build`)
- Produces: one screen scenario

- [ ] **Step 1: Write the scenario**

Nothing else in the suite clicks a permission card, so this scenario is what proves that the ported caller reaches the engine with the turn id and that the card goes only after the engine confirms. It lives in its own file so it does not touch `testkit/src/screen/app.rs`, which the looks commit also edits.

**Write the whole file** `testkit/src/screen/permissions.rs`:

```rust
//! Permission cards: the screen answers a question by the ids of its turn and its request, and the card goes only once
//! the engine says the CLI has the answer.

use super::{Result, Scenario, ask, ensure, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};

/// The request of the recording `claude/tools.jsonl`, which allows the first Write and denies the second.
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "a permission card is answered from the screen and the answer reaches the CLI", run: answers_a_card }]
}

fn answers_a_card(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")),
        ..Launch::default()
    };
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    // Auto would send the request to a router process first, which would replay this recording instead of the chat.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;

    // The CLI asks to write a.txt. Allow it ("Always allow" holds the same word, so the name must match exactly); the card for b.txt follows, and the recording denies that one.
    let first = css(".ask").has_text("a.txt");
    page.wait_visible(&first)?;
    let allow = page.named("button", "ask.allow")?.exact();
    page.click(&allow)?;
    page.wait_gone(&first)?;
    let second = css(".ask").has_text("b.txt");
    page.wait_visible(&second)?;
    let deny = page.named("button", "ask.deny")?.exact();
    page.click(&deny)?;

    // The recording ends the turn only if the CLI got both answers as recorded.
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ensure(page.count(&css(".ask"))? == 0, || "no card should be left".to_string())
}
```

**Edit** `testkit/src/screen/mod.rs`. Find:

```rust
pub mod containment;
pub mod setup;
```

Replace with:

```rust
pub mod containment;
pub mod permissions;
pub mod setup;
```

**Edit** `testkit/src/screen/mod.rs`. Find:

```rust
    let all = app::scenarios().into_iter().chain(setup::scenarios());
```

Replace with:

```rust
    let all = app::scenarios().into_iter().chain(permissions::scenarios()).chain(setup::scenarios());
```

Run:

```bash
cargo t --suite screen --filter "permission card"
```

Expected: one scenario passes. The screen suite is Windows only and needs the production `dist/` from `npm run build` (Task 5 ran it; run it again if `dist/` is missing). The first run also builds the test app, which takes a few minutes.

---

### Task 7: Restore line endings, prove everything, commit once  (model: `sonnet`)

**Files:**
- Modify: none (line endings only). Commit: the 20 files of Tasks 2 to 6.

**Interfaces:**
- Consumes: Tasks 1 to 6
- Produces: one commit

- [ ] **Step 1: Give the four files their CRLF back**

Only the files Task 1 converted. The command reads each file in full before it writes it back.

Run:

```bash
python -c "import pathlib, sys; [pathlib.Path(p).write_bytes(pathlib.Path(p).read_bytes().replace(b'\r\n', b'\n').replace(b'\n', b'\r\n')) for p in sys.argv[1:]]" engine/src/codex.rs engine/src/events.rs engine/src/model.rs ui/Thread.tsx
```

Expected: no output.

Run:

```bash
git ls-files --eol engine/src/codex.rs engine/src/events.rs engine/src/model.rs ui/Thread.tsx
```

Expected: every line reads `i/crlf  w/crlf`.

Run:

```bash
git diff --stat -- engine/src/codex.rs engine/src/events.rs engine/src/model.rs ui/Thread.tsx
```

Expected: tens of changed lines per file, not the whole file. A count in the hundreds means a file is still LF.

- [ ] **Step 2: Run every proof command**

All of them exit 0. If one fails, fix the cause in the file the output names, then run the whole list again. Do not skip a command.

Run:

```bash
npm run lint
```

Run:

```bash
npm run typecheck
```

Run:

```bash
npm run build
```

Run:

```bash
cargo fmt --all --check
```

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Run:

```bash
cargo clippy -p faber --features test-hooks -- -D warnings
```

Run:

```bash
cargo t --all
```

Expected: every suite passes: `rust` (the engine tests, including `engine/tests/running.rs` and the tests in `engine/src/engine/answer.rs`), `ui`, and `screen` (Windows), which includes the scenario `a permission card is answered from the screen and the answer reaches the CLI`. The screen suite needs the production `dist/` from `npm run build` above.

- [ ] **Step 3: Commit once**

The commit holds exactly these files. If `git status` shows others you did not touch for this plan (B3's files, if B3 is still uncommitted), leave them out of this commit.

Run:

```bash
git add engine/Cargo.toml engine/src/claude.rs engine/src/codex.rs engine/src/engine.rs engine/src/engine/answer.rs engine/src/engine/auto.rs engine/src/events.rs engine/src/model.rs engine/src/proc.rs engine/tests/claude_session.rs engine/tests/codex_session.rs engine/tests/engine.rs engine/tests/running.rs engine/tests/support/mod.rs src-tauri/src/main.rs testkit/src/screen/mod.rs testkit/src/screen/permissions.rs ui/App.tsx ui/Thread.tsx ui/api.ts
git status --short
```

Expected: the 20 files above are staged and nothing else of this plan is left unstaged.

Run:

```bash
git commit -m 'feat(engine): running list, quota readings and acknowledged answers

The engine now tells the screen which conversations are answering or wait for the
user: AppState.running and a Running event, in a stable order, with a revision
that grows with every list sent. AppState.quotas keeps the latest reading of each
quota window with its reset time.

answer(conversation, turn, request) checks the turn against the live turn, hands
the answer to the session and takes the question off the list only when the
session confirms the CLI got it. Both session handlers keep their pending request
until the write succeeds, so a failed write can be answered again. A reply whose
start fails now ends with an error instead of staying in the running list.

The one old caller of answer (Thread.tsx, App.tsx, api.ts) passes the turn id.'
```

Expected: one commit on `phase-1-base`. Add the attribution trailer your harness requires, as the last lines of the message.

---

## Deviations from spec

Where this plan differs from the spec, and why. Nothing here changes what the spec asks the engine to do.

1. **The running-list tests are in a new file, `engine/tests/running.rs`.** The spec lists them under `engine/tests/engine.rs`. That file is also edited by commit 4 (B3) at its end, and the new tests need their own helpers (`lists_until`, `router`, a scripted spawner), so a separate file keeps both commits apart. `engine/tests/engine.rs` only has its three `answer` callers ported.
2. **A screen scenario was added: `testkit/src/screen/permissions.rs`, registered in `testkit/src/screen/mod.rs`.** The spec's B5a file list has no `testkit` change. Nothing else in the suite clicks a permission card, so without it the ported caller (`Thread.tsx`, `App.tsx`, `api.ts`) and the shell command would reach the commit untested. The scenario sits in its own file so it does not touch `testkit/src/screen/app.rs`, which commit 4 edits.
3. **The spec's repeated-request-id test ("a Claude conversation in Chat and a Codex conversation in a project both hold request `0`") uses two Codex conversations.** The recorded Claude requests carry UUIDs, not `0`. A Codex chat and a Codex project use two Codex processes that both number from 0, which is the real collision. The "same request id again in a new turn" case is covered with a hand-written two-turn Claude session (`claude_two_turns`) and with the in-crate stand-in.
4. **After an answer, a row can go from `needsYou` straight to `needsYou` (the next question), with no `working` list between.** The spec says `answer` "returns it to working". That holds when no other question follows. The CLI often asks its next question before the engine has settled the last answer, and then the list correctly shows only the new one. The unit test `a_delivered_answer_takes_the_question_off_the_list_and_the_row_goes_back_to_working` pins the first case; the integration test asserts only that a turn starts working, asks, and leaves.
5. **A send whose CLI fails to start now also ends its reply with an error (`Core::fail_reply`).** The spec only describes this for the Auto path ("a start that fails ends the reply with an error and drops the item"). The direct path, with a chosen model, left the reply `Streaming` forever, which would have put a ghost row in the list. Both paths now use the same function.
6. **`EngineEvent::Started` also refreshes the running list.** The spec's list of recompute points names `Message` and `List` emissions. A turn that begins with Auto learns its model from `Started`, which emits neither, and `RunningItem` carries `model`. One extra call keeps the row current.
7. **`dev-dependencies` of `engine` gain `tokio` with the `test-util` feature.** The spec asks for tokio's paused clock in two tests (the 5 s acknowledgement and the 30 s route). The feature is for tests only and does not reach the app.
8. **A failed write on the automatic "Always" answer path is not reported.** When a tool is already on the conversation's allow list, the engine answers the CLI's request itself and never listed it, as before. It drops the acknowledgement receiver there (`drop(l.session.answer(..))`). The spec does not cover this path, and nothing is shown to the user for it, so there is nothing to keep or undo.
9. **`Core.used` is replaced by `Core.quotas` (the whole reading); Auto reads `.used` from it.** The spec says "Auto's own use of the used share is unchanged". Its value is the same; only the storage widened.
10. **Running-list ties are broken by conversation id after `since`.** The spec says "oldest first" and "stable". Two replies started in the same millisecond need a fixed order, or rows would swap between lists.

## Self-review

**Spec coverage.** Each requirement of B5a and where it is built and tested:

| Requirement | Where |
| :--- | :--- |
| `RunningItem` and its fields, `AppState.running`, `AppEvent::Running`, revision from 0 growing by one | Task 3 (`model.rs`, `engine.rs`; tests `a_turn_is_listed_as_working...`, `an_answer_emits_only_the_lists_that_differ...`, `the_state_and_the_event_reach_the_screen_in_camel_case`) |
| Which conversations are listed, including a reply still waiting for Auto, no vendor yet | Task 3 (`running_item`; tests `an_auto_reply_is_listed_at_once...`) |
| Stop while Auto chooses; late or failed route; a start that fails | Task 3 (`stopping_while_auto_still_chooses...`, `a_route_that_never_arrives...`, `a_start_that_fails_after_the_pick...`, `a_send_that_cannot_start...`) |
| Order (R54): questions first, sidebar order, Chat last, oldest first | Task 3 (`running_items`; test `blocks_with_a_question_come_first...`) |
| `refresh_running` wherever a `Message` or `List` is emitted; emit only when the list differs; `rename` recomputes the title | Task 3 (`emit_message`, `list_changed`; tests `an_answer_emits_only...`, `renaming_a_running_conversation...`) |
| Quota readings in `AppState.quotas`; the state survives a rebuilt window | Task 3 (`QuotaReading`, `Core.quotas`; tests `the_latest_quota_readings...`, `the_state_and_the_event...`) |
| `answer` with the turn id, stale and undelivered, validation before sending, no permission changed before the acknowledgement | Task 4 (`answer.rs`; the thirteen in-crate tests and the integration tests) |
| `Engine::answer` as an `async fn` with `prepare_answer` and `settle_answer`; the lock is free while waiting; settling re-checks; 5 s deadline | Task 4 (`answer.rs`; tests `the_engine_is_not_blocked...`, `a_conversation_removed_while_the_engine_waits...`, `no_acknowledgement_in_five_seconds...`) |
| `SessionCommand::Answer` carries an acknowledgement; `Session::answer` returns the receiver; handlers remove their entry only after a successful write; `write_line` returns an `io::Result` | Task 2 (the two session tests through the real handlers) |
| Always also answers the other requests of the tool, each with its own acknowledgement | Task 4 (`always_answers_the_other_waiting_requests...`, `deny_with_always...`, `an_always_that_was_not_delivered...`) |
| The shell `answer` command takes `turn_id` and passes the result through | Task 4, step 3 |
| The one old caller is ported; a rejected answer is caught and ignored | Task 5; proved end to end by Task 6 |
| Request ids that repeat across conversations and turns | Task 4 (`request_ids_that_repeat...`, `an_answer_that_names_the_previous_turn...`) |
| Quota test: a reading with a reset time survives a new `state()` call | Task 3 (`the_latest_quota_readings_are_kept_with_their_reset_times`) |

Not in this plan, because the spec puts them in other commits: the `RunningItem` and quota types in `ui/api.ts`, the screen of the running column, the texts of the two error keys (all B5b), the tray number (B6).

**Placeholder scan.** Every step shows the exact text to find and the exact text to put in, or the whole file. No step says "similar to", "add error handling" or "TBD". The only text a step leaves to the executor is the attribution trailer of the commit message, which depends on the harness.

**Type consistency.** `Delivery`, `Session::answer`, `RunningItem`, `RunState`, `Running`, `QuotaReading`, `AppEvent::Running`, `Core::refresh_running`, `Core::list_changed`, `Core::fail_reply` and `Engine::answer` are defined in the task named under Interfaces and used with the same names and argument order in the later ones. The shell command `answer(id, turn_id, request_id, allow, always)` matches `api.answer(id, turnId, requestId, allow, always)` (Tauri turns `turn_id` into `turnId`).

**Review Focus.** All five lines have tests, named in the section above.


## Review fixes after the cross-vendor review

Applied on top of Tasks 2 to 6 before the commit, each with a test that fails without it:

1. **"Always" and a question that arrives during the wait.** `settle_answer` takes the waiting questions of the remembered tool off the list and answers them through `Core::auto_answer`. Test: `a_question_of_the_tool_that_comes_in_while_always_waits_is_answered_too`.
2. **Automatic approvals watched.** The allow-list path in `handle` uses `Core::auto_answer`, which waits for the acknowledgements outside the lock and puts the question back on the list (`restore_questions`) when the CLI did not take it. Tests: `an_automatic_approval_the_cli_did_not_take_becomes_a_question_again`, `..._took_leaves_no_card`.
3. **Settlement checks the live turn.** `Handed` carries the process (`sid`); `settle_answer` is stale unless the same process still runs the same turn, so nothing is remembered after the turn ended or was replaced. Tests: `a_turn_that_ended_during_the_wait_...`, `a_process_or_turn_that_replaced_...`.
4. **No replay of a possibly written line.** `proc::write_line` returns `WriteError { torn }`; after a partial write or a failed flush the handlers end the process (its end of output reports the fatal error) instead of leaving the request for a retry. Tests: `an_answer_that_may_be_half_written_is_never_written_again` (Claude), `codex_an_answer_that_may_be_half_written_is_never_written_again`.
