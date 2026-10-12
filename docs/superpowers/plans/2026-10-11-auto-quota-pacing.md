# Auto Quota Pacing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Auto spends each vendor's weekly and five-hour quota along a daily line (100% / 7 = 14.28% a day), buys a step up when quota is spare, leaves a vendor that runs low, and never spends paid credits or usage-billed logins on its own.

**Architecture:** The engine keeps a small ledger (`budget.json`) of quota readings, spans of Faber's own requests, a learned weekly shape and learned model weights. Pure modules (`budget`, `learn`, `ledger`, `signals`, `route`, `handover`) compute prices and choices; `engine.rs` and `engine/auto.rs` feed them and call one gate (`budget::gate`) before every request. The screen shows the pick and its reason, and Settings shows plan and windows.

**Tech Stack:** Rust (tokio, serde) in `engine/`, Tauri 2 shell in `src-tauri/`, SolidJS + TypeScript in `ui/`, `cargo t` with the fake CLI in `testkit/`.

**Spec:** `docs/superpowers/specs/2026-10-11-auto-quota-pacing-design.md` (decisions D1-D13, sections 1-8, review log S1-S12 and C1-C21). Read it first.

## Global Constraints

- Code, comments, docs and commit messages in English; original code only (TL-ADE ideas such as per-day quota may be reused, its code may not).
- Never read, copy, store or forward CLI credentials or tokens. The account email in `account/read` and `claude auth status` is dropped unread.
- Child processes never go through a shell. Every text on screen lives in `ui/locales/` (five languages, written by hand, English is the source); the engine returns keys (`error.*`, `reason.*`).
- No vendor names in product, feature or package names.
- Memory: Faber with no CLI running stays under 500 MB; the end-to-end test fails above 250 MB. `budget.json` stays under 100 KB (compact JSON, 200 points, 200 spans, 168 bins, 60 stretches).
- Telemetry stays off. Auto never spends paid credits or usage-billed logins without the user's approval (D11).
- Proof commands, all exit 0 before any commit: `npm run lint`, `npm run typecheck`, `npm run build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p faber --features test-hooks -- -D warnings`, `cargo t` (`cargo t --all` when the screen or the shell changes). On the maintainer's PC export `RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/rust/cargo/bin:$PATH"` first.
- One commit per task, made once its tests pass.
- **R89 (Erick, 2026-10-12):** the spec wins. Auto denies `Billing::Unknown`; only a confirmed subscription can receive an automatic request. Missing quota readings are allowed only after billing is confirmed. A hand-picked user turn and a quota-free plan check keep their documented exceptions.

## Review Focus

1. A message waiting for quota when the app closes or the user presses stop: it must not send itself later, and the text must not be lost (pinned in Task 10).
2. An API-key or usage-billed login: Auto never routes to it, a hand pick still works (Tasks 3 and 8).
3. A clock set backwards, or readings older than 6 hours: no discount, no crash (Tasks 3, 4 and 8).
4. An old, empty or corrupt `budget.json` or conversation file: the app starts and learns again (Tasks 1, 4, 5 and 8).
5. Nonsense readings (used above 1, reset in the past) and odd window names: dropped or priced as no information (Tasks 3, 4 and 5).

The daily line of D13 is pinned by `a_week_spent_faster_than_one_seventh_a_day_is_priced_up_on_either_vendor` (Task 8).

Task numbers follow the spec's order of work: Task 2 (the cache probe) is part of Task 1's change and Task 7 (the pacing table) is part of Task 8's. How to read the tasks: Tasks 1-9 were applied in order to a copy of `engine/` and every test below passed there. Each `diff` block is the exact change of that task against the previous one, so apply them in order (`git apply` after stripping the `a/` `b/` labels, or edit by hand). Task 10-12 are specified in code at the interface level and were not run in the copy; run them with the compiler's help.

---
### Task 1: Per-turn cost, cache numbers and the cache probe

Measure what a turn costs (the running total minus the base), so the ledger can learn weights.

**Files:**
- `engine/examples/cache_probe.rs`
- `engine/src/claude.rs`
- `engine/src/codex.rs`
- `engine/src/engine.rs`
- `engine/src/engine/answer.rs`
- `engine/src/events.rs`
- `engine/src/lib.rs`
- `engine/src/model.rs`
- `engine/src/usage.rs`
- `engine/tests/claude_session.rs`
- `engine/tests/engine.rs`
- `engine/tests/turn_usage.rs`

**Interfaces:**
- Consumes: `Done{cost_total_usd, cache_*}` from the drivers. Also adds `engine/examples/cache_probe.rs` (spec Task 2; run by hand when Erick is not playing: it spends real quota). Produces: `usage::settle(before: Option<&CostBase>, session: &str, fresh: bool, total: Option<f64>) -> Settled{cost, base}`, `Message.cost_usd`, `Message.cache_*`, `Conversation.cost_base`, `Live.resumed`.

- [ ] **Step 1: Add the tests (they fail or do not compile yet)**

```diff
--- a/engine/tests/claude_session.rs
+++ b/engine/tests/claude_session.rs
@@ -46,7 +46,7 @@
     assert_eq!(text_of(&events), expected);
     assert!(events.iter().any(|e| matches!(e, EngineEvent::Quota { window, used, .. } if window == "five_hour" && *used == 0.07)));
     match events.last() {
-        Some(EngineEvent::Done { ok: true, cost_usd: Some(cost), output_tokens: 73, .. }) => assert!(*cost > 0.0),
+        Some(EngineEvent::Done { ok: true, cost_total_usd: Some(cost), output_tokens: 73, .. }) => assert!(*cost > 0.0),
         other => panic!("unexpected end: {other:?}"),
     }
 }
```

```diff
--- a/engine/tests/engine.rs
+++ b/engine/tests/engine.rs
@@ -588,3 +588,55 @@
     assert_eq!(f.wire.lines_with("thread/start"), 1);
     assert_eq!(f.wire.lines_with("developerInstructions"), 0);
 }
+
+#[tokio::test]
+async fn a_resumed_turn_costs_only_itself_and_the_conversation_remembers_the_new_total() {
+    let dir = tempfile::tempdir().unwrap();
+    let (first, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
+    let id = first.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
+    let reply = finished(&mut rx, |_| {}).await;
+    first.shutdown();
+    let file = dir.path().join("conversations").join(format!("{id}.json"));
+    let mut saved = read_json(&file);
+    // A new CLI session starts from nothing: the turn costs its whole total, and that total is the base from now on.
+    assert_eq!(saved["costBase"]["claude"]["session"], "c5b9834c-dfc7-4086-beed-2212be93b722");
+    let total = saved["costBase"]["claude"]["total"].as_f64().unwrap();
+    assert!((reply.cost_usd.unwrap() - total).abs() < 1e-9, "{:?} against {total}", reply.cost_usd);
+    assert!(reply.cache_read_tokens.is_some() && reply.cache_write_tokens.is_some());
+
+    // The resume recording continues another session, which reports 0.00023211 in all, of which 0.00018326 came from the
+    // process before it.
+    let session = "3ee0bd99-72d2-4c5b-8420-a9f85ee53981";
+    saved["sessions"]["claude"] = session.into();
+    saved["costBase"]["claude"] = serde_json::json!({ "session": session, "total": 0.00018326 });
+    std::fs::write(&file, saved.to_string()).unwrap();
+    let (second, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "resume")).spawn);
+    let text = "What single word did you reply with last time? Answer with only that word.";
+    second.send(SendRequest { conversation_id: Some(id), ..ask(Area::Chat, text, choice("claude", Some("haiku"))) }).unwrap();
+    let reply = finished(&mut rx, |_| {}).await;
+    second.shutdown();
+    assert!((reply.cost_usd.unwrap() - 0.00004885).abs() < 1e-9, "{:?}", reply.cost_usd);
+    let saved = read_json(&file);
+    assert!((saved["costBase"]["claude"]["total"].as_f64().unwrap() - 0.00023211).abs() < 1e-9);
+}
+
+#[tokio::test]
+async fn a_conversation_saved_before_the_cost_base_existed_still_opens() {
+    let dir = tempfile::tempdir().unwrap();
+    let (first, mut rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
+    let id = first.send(ask(Area::Chat, SKY, choice("claude", Some("haiku")))).unwrap();
+    finished(&mut rx, |_| {}).await;
+    first.shutdown();
+    let file = dir.path().join("conversations").join(format!("{id}.json"));
+    let mut saved = read_json(&file);
+    saved.as_object_mut().unwrap().remove("costBase");
+    for message in saved["messages"].as_array_mut().unwrap() {
+        for key in ["cacheReadTokens", "cacheWriteTokens", "cacheWrite1hTokens", "incomplete"] {
+            message.as_object_mut().unwrap().remove(key);
+        }
+    }
+    std::fs::write(&file, saved.to_string()).unwrap();
+    let (second, _rx) = engine(dir.path(), Vendor::Claude, fake(fixture("claude", "resume")).spawn);
+    assert_eq!(second.open(&id).unwrap().messages.len(), 2);
+    second.shutdown();
+}
```

```diff
--- a/engine/tests/turn_usage.rs
+++ b/engine/tests/turn_usage.rs
@@ -0,0 +1,129 @@
+use faber_engine::claude::parse_claude_message;
+use faber_engine::codex::{CodexTurnState, parse_codex_notification};
+use faber_engine::events::EngineEvent;
+use serde_json::{Value, json};
+
+fn done(events: Vec<EngineEvent>) -> EngineEvent {
+    events.into_iter().next().expect("one Done")
+}
+
+#[test]
+fn a_claude_result_splits_the_cache_numbers_and_keeps_the_cumulative_cost() {
+    let msg = json!({
+        "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0123,
+        "usage": {"input_tokens": 4, "cache_read_input_tokens": 4537, "cache_creation_input_tokens": 5391, "output_tokens": 483,
+                  "cache_creation": {"ephemeral_1h_input_tokens": 5391, "ephemeral_5m_input_tokens": 0}},
+        "modelUsage": {"main": {"inputTokens": 4, "cacheReadInputTokens": 4537, "cacheCreationInputTokens": 5391, "outputTokens": 483}}
+    });
+    let EngineEvent::Done {
+        input_tokens, cache_read_tokens, cache_write_tokens, cache_write_1h_tokens, cost_total_usd, incomplete, ..
+    } = done(parse_claude_message(&msg))
+    else {
+        panic!("not a Done")
+    };
+    assert_eq!((input_tokens, cache_read_tokens, cache_write_tokens, cache_write_1h_tokens), (9932, 4537, 5391, 5391));
+    assert_eq!(cost_total_usd, Some(0.0123));
+    assert!(!incomplete);
+}
+
+#[test]
+fn sub_model_work_marks_the_numbers_incomplete_but_keeps_the_cost() {
+    // claude/parallel.jsonl: usage shows 9,932 prompt tokens, modelUsage 35,552.
+    let msg = json!({
+        "type": "result", "subtype": "success", "total_cost_usd": 0.4,
+        "usage": {"input_tokens": 4, "cache_read_input_tokens": 4537, "cache_creation_input_tokens": 5391, "output_tokens": 483},
+        "modelUsage": {
+            "main": {"inputTokens": 4, "cacheReadInputTokens": 4537, "cacheCreationInputTokens": 5391, "outputTokens": 483},
+            "fetch": {"inputTokens": 25620, "cacheReadInputTokens": 0, "cacheCreationInputTokens": 0, "outputTokens": 1557}
+        }
+    });
+    let EngineEvent::Done { incomplete, cost_total_usd, input_tokens, .. } = done(parse_claude_message(&msg)) else { panic!("not a Done") };
+    assert!(incomplete);
+    assert_eq!(cost_total_usd, Some(0.4));
+    assert_eq!(input_tokens, 9932);
+}
+
+#[test]
+fn an_aborted_result_has_no_counters() {
+    let msg = json!({"type": "result", "subtype": "error_during_execution", "is_error": true, "total_cost_usd": 0, "usage": {}});
+    let EngineEvent::Done { ok, cost_total_usd, input_tokens, incomplete, .. } = done(parse_claude_message(&msg)) else {
+        panic!("not a Done")
+    };
+    assert!(!ok);
+    assert_eq!((cost_total_usd, input_tokens, incomplete), (None, 0, false));
+    let missing = json!({"type": "result", "subtype": "success"});
+    let EngineEvent::Done { cost_total_usd, .. } = done(parse_claude_message(&missing)) else { panic!("not a Done") };
+    assert_eq!(cost_total_usd, None, "a result with no cost is not a cost of 0");
+}
+
+fn update(input: u64, cached: u64, output: u64) -> Value {
+    json!({"method": "thread/tokenUsage/updated", "params": {"tokenUsage": {"total": {"inputTokens": input, "cachedInputTokens": cached, "outputTokens": output}}}})
+}
+
+fn started() -> Value {
+    json!({"method": "turn/started", "params": {"turn": {"id": "t"}}})
+}
+
+fn completed(status: &str) -> Value {
+    json!({"method": "turn/completed", "params": {"turn": {"status": status}}})
+}
+
+#[test]
+fn a_resumed_codex_turn_is_the_difference_of_the_thread_totals() {
+    // codex/resume.jsonl: 14,204 input before the turn, 28,435 after; 14,080 of the difference was cached; 5 output.
+    let mut state = CodexTurnState::new(true);
+    parse_codex_notification(&update(14_204, 7_000, 20), &mut state);
+    parse_codex_notification(&started(), &mut state);
+    parse_codex_notification(&update(28_435, 21_080, 25), &mut state);
+    let EngineEvent::Done { ok, input_tokens, cache_read_tokens, output_tokens, incomplete, .. } =
+        done(parse_codex_notification(&completed("completed"), &mut state))
+    else {
+        panic!("not a Done")
+    };
+    assert!(ok);
+    assert_eq!((input_tokens, cache_read_tokens, output_tokens, incomplete), (14_231, 14_080, 5, false));
+}
+
+#[test]
+fn a_new_codex_thread_starts_from_nothing() {
+    let mut state = CodexTurnState::new(false);
+    parse_codex_notification(&started(), &mut state);
+    parse_codex_notification(&update(14_254, 6_912, 30), &mut state);
+    let EngineEvent::Done { input_tokens, cache_read_tokens, .. } = done(parse_codex_notification(&completed("completed"), &mut state))
+    else {
+        panic!("not a Done")
+    };
+    assert_eq!((input_tokens, cache_read_tokens), (14_254, 6_912));
+}
+
+#[test]
+fn a_resumed_thread_with_no_update_before_the_turn_reports_no_usage() {
+    let mut state = CodexTurnState::new(true);
+    parse_codex_notification(&started(), &mut state);
+    parse_codex_notification(&update(28_435, 21_080, 25), &mut state);
+    let EngineEvent::Done { input_tokens, incomplete, .. } = done(parse_codex_notification(&completed("completed"), &mut state)) else {
+        panic!("not a Done")
+    };
+    assert_eq!(input_tokens, 0, "not the whole thread");
+    assert!(incomplete);
+}
+
+#[test]
+fn an_interrupted_turn_reports_what_arrived_and_the_next_one_does_not_pay_for_it_again() {
+    let mut state = CodexTurnState::new(false);
+    parse_codex_notification(&started(), &mut state);
+    parse_codex_notification(&update(1_000, 0, 10), &mut state);
+    let EngineEvent::Done { ok, input_tokens, .. } = done(parse_codex_notification(&completed("interrupted"), &mut state)) else {
+        panic!("not a Done")
+    };
+    assert!(!ok);
+    assert_eq!(input_tokens, 1_000);
+    parse_codex_notification(&started(), &mut state);
+    parse_codex_notification(&update(1_800, 900, 14), &mut state);
+    let EngineEvent::Done { input_tokens, cache_read_tokens, output_tokens, .. } =
+        done(parse_codex_notification(&completed("completed"), &mut state))
+    else {
+        panic!("not a Done")
+    };
+    assert_eq!((input_tokens, cache_read_tokens, output_tokens), (800, 900, 4));
+}
```

- [ ] **Step 2: Run the new tests and see them fail**

Run: `cargo test -p faber-engine` (the named test files)
Expected: compile errors or failures naming the missing items.

- [ ] **Step 3: Implement**

`engine/examples/cache_probe.rs`

```diff
--- a/engine/examples/cache_probe.rs
+++ b/engine/examples/cache_probe.rs
@@ -0,0 +1,266 @@
+//! Manual measurement of the CLIs' prompt cache and of what a quota pulse costs. Never part of `cargo t`: it talks to the real
+//! CLIs with the user's own sign-in and spends a little real quota.
+//!
+//!     cargo run -p faber-engine --example cache_probe -- <scenario> [argument]
+//!
+//! Scenarios (each prints one markdown table row per turn):
+//!   claude-turns           three turns in one process
+//!   claude-resume <secs>   a turn, the process closes, `secs` pass, a new process resumes the session
+//!   claude-effort          a turn at medium effort, then a resumed turn at high
+//!   claude-pulse <n>       `n` one-word questions in a row, and how far the quota windows moved
+//!   codex-turns            three turns on one thread
+//!   codex-effort           a turn at low effort, then a turn at medium on the same thread
+//!   codex-gap <mins>       a turn, `mins` pass, another turn on the same thread
+//!   codex-resume           a turn, the thread is dropped, a new thread resumes it (the `total` after a resume)
+
+use faber_engine::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
+use faber_engine::cli::{Cli, find_cli};
+use faber_engine::codex::{CodexServer, CodexServerOptions, CodexThread};
+use faber_engine::events::{EngineEvent, Session, UserTurn};
+use faber_engine::model::Area;
+use faber_engine::proc::{Spawner, real_spawner};
+use std::path::PathBuf;
+use std::sync::Arc;
+use std::time::Duration;
+use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
+
+const CLAUDE_MODEL: &str = "claude-sonnet-5-5";
+const CODEX_MODEL: &str = "gpt-6-luna";
+/// More than a thousand tokens, so a cache has something to hold; the same text every time.
+const FILLER_SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the committee reviews the quarterly report. ";
+const TURN_TIMEOUT: Duration = Duration::from_secs(180);
+
+fn question(n: usize) -> String {
+    format!("{}\n\nQuestion {n}: reply with the single word ok.", FILLER_SENTENCE.repeat(120))
+}
+
+/// The CLI the way the engine starts it: the command and, for npm shims, the prefix that goes before the arguments.
+fn spawner(cli: &Cli) -> Spawner {
+    let (base, prefix, command) = (real_spawner(), cli.prefix.clone(), cli.command.clone());
+    Arc::new(move |_bin, args, cwd| base(&command, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd))
+}
+
+type Windows = Vec<(String, f64)>;
+
+/// One finished turn.
+#[derive(Debug, Default)]
+struct Turn {
+    session: String,
+    input: u64,
+    cached: u64,
+    written: u64,
+    output: u64,
+    incomplete: bool,
+    windows: Windows,
+}
+
+async fn finish(rx: &mut UnboundedReceiver<EngineEvent>) -> Turn {
+    let mut turn = Turn::default();
+    loop {
+        match tokio::time::timeout(TURN_TIMEOUT, rx.recv()).await.expect("no answer in time").expect("session ended") {
+            EngineEvent::Started { session_id, .. } => turn.session = session_id,
+            EngineEvent::Quota { window, used, .. } => turn.windows.push((window, used)),
+            EngineEvent::Done { input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, incomplete, .. } => {
+                (turn.input, turn.cached, turn.written, turn.output, turn.incomplete) =
+                    (input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, incomplete);
+                return turn;
+            }
+            EngineEvent::Error { message, fatal } => panic!("the CLI said: {message} (fatal: {fatal})"),
+            _ => {}
+        }
+    }
+}
+
+fn row(scenario: &str, label: &str, t: &Turn) {
+    let hit = if t.input == 0 { 0.0 } else { t.cached as f64 * 100.0 / t.input as f64 };
+    println!(
+        "| {scenario} | {label} | {} | {} | {} | {hit:.1}% | {} | {} |",
+        t.input,
+        t.cached,
+        t.written,
+        t.output,
+        if t.incomplete { "incomplete" } else { "" }
+    );
+}
+
+struct Probe {
+    dir: PathBuf,
+}
+
+impl Probe {
+    fn claude(&self, resume: Option<String>, effort: &str) -> (Session, UnboundedReceiver<EngineEvent>) {
+        let cli = find_cli("claude").expect("claude is not installed");
+        let (tx, rx) = unbounded_channel();
+        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
+        let launch = ClaudeLaunch {
+            area: Area::Chat,
+            model: Some(CLAUDE_MODEL.into()),
+            effort: Some(effort.into()),
+            resume,
+            user_hooks: false,
+            instructions: None,
+            user_name: None,
+        };
+        let session = start_claude(ClaudeOptions { bin: cli.command.clone(), cwd: self.dir.clone(), launch, spawn: spawner(&cli) }, sink);
+        (session, rx)
+    }
+
+    /// A side question with its own instructions: no tools, no saved session, which is what a pulse is.
+    fn pulse(&self) -> (Session, UnboundedReceiver<EngineEvent>) {
+        let cli = find_cli("claude").expect("claude is not installed");
+        let (tx, rx) = unbounded_channel();
+        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
+        let launch = ClaudeLaunch {
+            area: Area::Chat,
+            model: Some("claude-haiku-5-5".into()),
+            effort: Some("low".into()),
+            resume: None,
+            user_hooks: false,
+            instructions: Some("Reply with the single word ok.".into()),
+            user_name: None,
+        };
+        let session = start_claude(ClaudeOptions { bin: cli.command.clone(), cwd: self.dir.clone(), launch, spawn: spawner(&cli) }, sink);
+        (session, rx)
+    }
+
+    fn codex_server(&self) -> CodexServer {
+        let cli = find_cli("codex").expect("codex is not installed");
+        CodexServer::start(CodexServerOptions {
+            bin: cli.command.clone(),
+            cwd: self.dir.clone(),
+            area: Area::Chat,
+            user_hooks: false,
+            windows: cfg!(windows),
+            spawn: spawner(&cli),
+        })
+    }
+
+    fn codex_thread(&self, server: &CodexServer, effort: &str, resume: Option<String>) -> (Session, UnboundedReceiver<EngineEvent>) {
+        let (tx, rx) = unbounded_channel();
+        let sink: EventSink = Arc::new(move |e| drop(tx.send(e)));
+        let thread = CodexThread {
+            cwd: self.dir.clone(),
+            model: Some(CODEX_MODEL.into()),
+            effort: Some(effort.into()),
+            resume,
+            instructions: None,
+            user_name: None,
+        };
+        (server.open(thread, sink), rx)
+    }
+}
+
+async fn wait(label: &str, d: Duration) {
+    eprintln!("waiting {label} ...");
+    tokio::time::sleep(d).await;
+}
+
+#[tokio::main]
+async fn main() {
+    let args: Vec<String> = std::env::args().skip(1).collect();
+    let scenario = args.first().map(String::as_str).unwrap_or("help");
+    let number = |default: u64| args.get(1).and_then(|a| a.parse::<u64>().ok()).unwrap_or(default);
+    let dir = std::env::temp_dir().join("faber-cache-probe");
+    std::fs::create_dir_all(&dir).expect("a folder to work in");
+    let probe = Probe { dir };
+    println!("| scenario | turn | prompt tokens | cache read | cache written | hit | output | note |");
+    println!("|---|---|---|---|---|---|---|---|");
+    match scenario {
+        "claude-turns" => {
+            let (session, mut rx) = probe.claude(None, "medium");
+            for n in 1..=3 {
+                session.send(UserTurn::text(question(n)));
+                row(scenario, &format!("turn {n}"), &finish(&mut rx).await);
+            }
+            session.close();
+        }
+        "claude-resume" => {
+            let (first, mut rx) = probe.claude(None, "medium");
+            first.send(UserTurn::text(question(1)));
+            let a = finish(&mut rx).await;
+            row(scenario, "turn 1", &a);
+            first.close();
+            wait(&format!("{} s", number(360)), Duration::from_secs(number(360))).await;
+            let (second, mut rx) = probe.claude(Some(a.session.clone()), "medium");
+            second.send(UserTurn::text(question(2)));
+            row(scenario, &format!("resumed after {} s", number(360)), &finish(&mut rx).await);
+            second.close();
+        }
+        "claude-effort" => {
+            let (first, mut rx) = probe.claude(None, "medium");
+            first.send(UserTurn::text(question(1)));
+            let a = finish(&mut rx).await;
+            row(scenario, "medium", &a);
+            first.close();
+            let (second, mut rx) = probe.claude(Some(a.session.clone()), "high");
+            second.send(UserTurn::text(question(2)));
+            row(scenario, "high, resumed", &finish(&mut rx).await);
+            second.close();
+        }
+        "claude-pulse" => {
+            let n = number(200);
+            let (mut first, mut last): (Windows, Windows) = (vec![], vec![]);
+            for i in 0..n {
+                let (session, mut rx) = probe.pulse();
+                session.send(UserTurn::text("ok"));
+                let t = finish(&mut rx).await;
+                session.close();
+                if i == 0 {
+                    first = t.windows.clone();
+                }
+                last = t.windows;
+            }
+            println!("\npulses: {n}");
+            for (window, after) in &last {
+                let before = first.iter().find(|(w, _)| w == window).map_or(0.0, |(_, u)| *u);
+                println!(
+                    "{window}: {:.0}% -> {:.0}% : {:.4} points per pulse",
+                    before * 100.0,
+                    after * 100.0,
+                    (after - before) * 100.0 / n as f64
+                );
+            }
+        }
+        "codex-turns" | "codex-effort" | "codex-gap" | "codex-resume" => codex(&probe, scenario, number(35)).await,
+        _ => eprintln!(
+            "scenarios: claude-turns, claude-resume <secs>, claude-effort, claude-pulse <n>, codex-turns, codex-effort, codex-gap <mins>, codex-resume"
+        ),
+    }
+}
+
+async fn codex(probe: &Probe, scenario: &str, minutes: u64) {
+    let server = probe.codex_server();
+    let (first, mut rx) = probe.codex_thread(&server, "low", None);
+    first.send(UserTurn::text(question(1)));
+    let a = finish(&mut rx).await;
+    row(scenario, "turn 1", &a);
+    match scenario {
+        "codex-turns" => {
+            for n in 2..=3 {
+                first.send(UserTurn::text(question(n)));
+                row(scenario, &format!("turn {n}"), &finish(&mut rx).await);
+            }
+        }
+        "codex-effort" => {
+            first.close();
+            let (second, mut rx) = probe.codex_thread(&server, "medium", Some(a.session.clone()));
+            second.send(UserTurn::text(question(2)));
+            row(scenario, "medium, same thread", &finish(&mut rx).await);
+            second.close();
+        }
+        "codex-gap" => {
+            wait(&format!("{minutes} min"), Duration::from_secs(minutes * 60)).await;
+            first.send(UserTurn::text(question(2)));
+            row(scenario, &format!("after {minutes} min"), &finish(&mut rx).await);
+        }
+        _ => {
+            first.close();
+            let (second, mut rx) = probe.codex_thread(&server, "low", Some(a.session.clone()));
+            second.send(UserTurn::text(question(2)));
+            row(scenario, "resumed thread", &finish(&mut rx).await);
+            second.close();
+        }
+    }
+    first.close();
+    server.retire();
+}
```

`engine/src/claude.rs`

```diff
--- a/engine/src/claude.rs
+++ b/engine/src/claude.rs
@@ -142,21 +142,48 @@
                     .collect()
             })
             .unwrap_or_default(),
-        "result" => {
-            let u = &msg["usage"];
-            let n = |k: &str| u[k].as_u64().unwrap_or(0);
-            vec![EngineEvent::Done {
-                ok: msg["subtype"] == "success" && msg["is_error"] != true,
-                reason: msg["terminal_reason"].as_str().or(msg["subtype"].as_str()).unwrap_or_default().into(),
-                cost_usd: Some(msg["total_cost_usd"].as_f64().unwrap_or(0.0)),
-                input_tokens: n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens"),
-                output_tokens: n("output_tokens"),
-            }]
-        }
+        "result" => result_events(msg),
         _ => vec![],
     }
 }
 
+/// A finished turn. `usage` describes the main response only, while `modelUsage` adds the sub-model work of the same turn
+/// (web fetches and the like), so a turn whose `modelUsage` prompt is larger is marked incomplete; its cost stays valid.
+fn result_events(msg: &Value) -> Vec<EngineEvent> {
+    let u = &msg["usage"];
+    let n = |k: &str| u[k].as_u64().unwrap_or(0);
+    let (fresh, read, write) = (n("input_tokens"), n("cache_read_input_tokens"), n("cache_creation_input_tokens"));
+    let prompt = fresh + read + write;
+    // `usage` describes the main response only; `modelUsage` adds the sub-model work of the same turn.
+    let whole_turn: u64 = msg["modelUsage"]
+        .as_object()
+        .map(|models| {
+            models
+                .values()
+                .map(|m| {
+                    ["inputTokens", "cacheReadInputTokens", "cacheCreationInputTokens"]
+                        .iter()
+                        .map(|k| m[*k].as_u64().unwrap_or(0))
+                        .sum::<u64>()
+                })
+                .sum()
+        })
+        .unwrap_or(0);
+    // An aborted turn reports a total of 0 and an empty usage: no counters, which is not a cost of 0.
+    let total = msg["total_cost_usd"].as_f64();
+    let counters = total.is_some_and(|t| t > 0.0) || prompt + n("output_tokens") > 0;
+    vec![EngineEvent::Done {
+        ok: msg["subtype"] == "success" && msg["is_error"] != true,
+        reason: msg["terminal_reason"].as_str().or(msg["subtype"].as_str()).unwrap_or_default().into(),
+        cost_total_usd: total.filter(|_| counters),
+        input_tokens: prompt,
+        output_tokens: n("output_tokens"),
+        cache_read_tokens: read,
+        cache_write_tokens: write,
+        cache_write_1h_tokens: u["cache_creation"]["ephemeral_1h_input_tokens"].as_u64().unwrap_or(0),
+        incomplete: whole_turn > prompt,
+    }]
+}
 fn items(v: &Value) -> impl Iterator<Item = &Value> {
     v.as_array().into_iter().flatten()
 }
```

`engine/src/codex.rs`

```diff
--- a/engine/src/codex.rs
+++ b/engine/src/codex.rs
@@ -57,13 +57,41 @@
     thread
 }
 
-/// What parsing remembers across notifications: the latest usage, which Codex sends before turn/completed,
-/// and which messages already streamed, so a message that arrives whole is still shown.
-#[derive(Debug, Default)]
-pub struct CodexTurnState {
+/// The thread's token counts, as `thread/tokenUsage/updated` reports them in `total`.
+#[derive(Debug, Default, Clone, Copy, PartialEq)]
+pub struct Totals {
     pub input: u64,
+    pub cached: u64,
     pub output: u64,
+}
+
+impl Totals {
+    fn since(self, base: Totals) -> Totals {
+        Totals {
+            input: self.input.saturating_sub(base.input),
+            cached: self.cached.saturating_sub(base.cached),
+            output: self.output.saturating_sub(base.output),
+        }
+    }
+}
+
+/// What parsing remembers across notifications: the thread's usage so far and its value when the turn started, and which
+/// messages already streamed, so a message that arrives whole is still shown.
+#[derive(Debug, Default)]
+pub struct CodexTurnState {
     pub streamed: HashSet<String>,
+    /// The thread's usage so far, as the latest update said. None until one arrives (a resumed thread gets one before its
+    /// first turn).
+    pub total: Option<Totals>,
+    /// `total` when the current turn started.
+    pub base: Option<Totals>,
+}
+
+impl CodexTurnState {
+    /// A thread that starts empty has used nothing yet; a resumed one learns its total from the update that comes first.
+    pub fn new(resumed: bool) -> Self {
+        CodexTurnState { total: (!resumed).then(Totals::default), ..Default::default() }
+    }
 }
 
 const NOT_TOOLS: [&str; 3] = ["userMessage", "agentMessage", "reasoning"];
@@ -120,9 +148,14 @@
             let text = item["aggregatedOutput"].as_str().or(item["status"].as_str()).unwrap_or_default().into();
             vec![EngineEvent::ToolResult { id, ok, text }]
         }
+        "turn/started" => {
+            state.base = state.total;
+            vec![]
+        }
         "thread/tokenUsage/updated" => {
-            state.input = p["tokenUsage"]["last"]["inputTokens"].as_u64().unwrap_or(0);
-            state.output = p["tokenUsage"]["last"]["outputTokens"].as_u64().unwrap_or(0);
+            let t = &p["tokenUsage"]["total"];
+            let g = |k: &str| t[k].as_u64().unwrap_or(0);
+            state.total = Some(Totals { input: g("inputTokens"), cached: g("cachedInputTokens"), output: g("outputTokens") });
             vec![]
         }
         "account/rateLimits/updated" => [&p["rateLimits"]["primary"], &p["rateLimits"]["secondary"]]
@@ -136,12 +169,20 @@
             .collect(),
         "turn/completed" => {
             let status = s(&p["turn"]["status"]);
+            // The turn's usage is the thread's total now minus its total when the turn started. With no start to
+            // measure from, the turn reports no usage (and says so) rather than the whole thread.
+            let used = state.total.zip(state.base).map(|(total, base)| total.since(base));
+            let u = used.unwrap_or_default();
             vec![EngineEvent::Done {
                 ok: status == "completed",
                 reason: status,
-                cost_usd: None,
-                input_tokens: state.input,
-                output_tokens: state.output,
+                cost_total_usd: None,
+                input_tokens: u.input,
+                output_tokens: u.output,
+                cache_read_tokens: u.cached,
+                cache_write_tokens: 0,
+                cache_write_1h_tokens: 0,
+                incomplete: used.is_none(),
             }]
         }
         "error" => {
@@ -431,7 +472,8 @@
             }
             cmd = rx.recv(), if !stopping => match cmd {
                 Some(ServerCommand::Open { key, thread, sink }) => {
-                    let t = Thread { sink, params: thread, id: None, turn: None, queued: vec![], state: CodexTurnState::default(), approvals: HashMap::new() };
+                    let state = CodexTurnState::new(thread.resume.is_some());
+                    let t = Thread { sink, params: thread, id: None, turn: None, queued: vec![], state, approvals: HashMap::new() };
                     if ready {
                         let (method, params) = thread_params(area, &t.params);
                         request!(method, params, Waiting::Thread(key));
```

`engine/src/engine.rs`

```diff
--- a/engine/src/engine.rs
+++ b/engine/src/engine.rs
@@ -9,6 +9,7 @@
 use crate::proc::Spawner;
 use crate::router::{Choice, Pick, pick_model};
 use crate::store::Store;
+use crate::usage;
 use crate::workspace::{Attachment, inside_folder, new_files, save_files};
 use serde::{Deserialize, Serialize};
 use std::collections::{BTreeMap, HashMap, HashSet};
@@ -124,6 +125,8 @@
     used: u64,
     /// The CLI this process was started with; a replaced CLI starts a new process on the next turn.
     cli: Cli,
+    /// The process continues a session an earlier process began.
+    resumed: bool,
 }
 
 const HISTORY_LIMIT: usize = 20_000;
@@ -516,12 +519,34 @@
                 self.quotas.insert((vendor, window.clone()), reading);
                 return (self.emit)(AppEvent::Quota { vendor, window, used, resets_at });
             }
-            EngineEvent::Done { ok, cost_usd, input_tokens, output_tokens, .. } => {
+            EngineEvent::Done {
+                ok,
+                cost_total_usd,
+                input_tokens,
+                output_tokens,
+                cache_read_tokens,
+                cache_write_tokens,
+                cache_write_1h_tokens,
+                incomplete,
+                ..
+            } => {
+                // The CLI reports a total for the whole conversation; the turn costs what is new since the last total.
+                let session = self.convs.get(conv).and_then(|c| c.sessions.get(&vendor)).cloned().unwrap_or_default();
+                let fresh = self.live.get(conv).is_some_and(|l| !l.resumed);
+                let before = self.convs.get(conv).and_then(|c| c.cost_base.get(&vendor));
+                let settled = usage::settle(before, &session, fresh, cost_total_usd);
+                if let (Some(base), Some(c)) = (settled.base, self.convs.get_mut(conv)) {
+                    c.cost_base.insert(vendor, base);
+                }
                 if let Some(m) = self.turn(conv) {
                     m.status = Some(if ok { Status::Done } else { Status::Stopped });
-                    m.cost_usd = cost_usd;
+                    m.cost_usd = settled.cost;
                     m.input_tokens = Some(input_tokens);
                     m.output_tokens = Some(output_tokens);
+                    m.cache_read_tokens = Some(cache_read_tokens);
+                    m.cache_write_tokens = Some(cache_write_tokens);
+                    m.cache_write_1h_tokens = Some(cache_write_1h_tokens);
+                    m.incomplete = incomplete.then_some(true);
                     m.permissions = None;
                 }
                 return self.finish(conv, turn_id);
@@ -726,6 +751,7 @@
         });
         let user_hooks = self.store.settings().user_hooks == Some(true);
         let resume = c.sessions.get(&pick.vendor).cloned();
+        let resumed = resume.is_some();
         let area = c.area;
         // The conversation's own name, fixed when it began: a rename in Settings never reaches a conversation under way.
         let user_name = c.user_name.clone();
@@ -757,6 +783,7 @@
                 break_pending: false,
                 used: self.tick,
                 cli,
+                resumed,
             },
         );
         // ponytail: count-based limit on idle Claude processes; a RAM budget across all CLIs comes with parallel workers.
@@ -788,6 +815,7 @@
             area,
             project_id,
             sessions: BTreeMap::new(),
+            cost_base: BTreeMap::new(),
             allow_always: None,
             branch_of: None,
             user_name: self.store.settings().user_name.and_then(|name| clean_name(&name)),
```

`engine/src/engine/answer.rs`

```diff
--- a/engine/src/engine/answer.rs
+++ b/engine/src/engine/answer.rs
@@ -234,6 +234,7 @@
                 break_pending: false,
                 used: 0,
                 cli: Cli { command: "claude".into(), prefix: vec![] },
+                resumed: false,
             };
             c.live.insert(id.clone(), live);
             c.refresh_running();
@@ -554,7 +555,17 @@
         let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
         let call = waiting_call(&engine, &s, &id, true).await;
 
-        let done = EngineEvent::Done { ok: true, reason: String::new(), cost_usd: None, input_tokens: 0, output_tokens: 0 };
+        let done = EngineEvent::Done {
+            ok: true,
+            reason: String::new(),
+            cost_total_usd: None,
+            input_tokens: 0,
+            output_tokens: 0,
+            cache_read_tokens: 0,
+            cache_write_tokens: 0,
+            cache_write_1h_tokens: 0,
+            incomplete: false,
+        };
         engine.with(|c| c.handle(&id, 1, done));
         s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();
 
```

`engine/src/events.rs`

```diff
--- a/engine/src/events.rs
+++ b/engine/src/events.rs
@@ -33,13 +33,23 @@
         used: f64,
         resets_at: f64,
     },
-    /// `cost_usd` is the CLI's own API-priced figure when it reports one (Claude Code does, Codex does not).
+    /// What a finished turn used. `cost_total_usd` is Claude Code's own API-priced figure as it reports it: cumulative for
+    /// the conversation, so the engine subtracts what it saw before (`usage::settle`); None when the result carries no
+    /// counters (an aborted turn) and on Codex, which reports no cost. `input_tokens` is the prompt of the main response,
+    /// cached part included, so a hit rate is `cache_read_tokens / input_tokens`.
     Done {
         ok: bool,
         reason: String,
-        cost_usd: Option<f64>,
+        cost_total_usd: Option<f64>,
         input_tokens: u64,
         output_tokens: u64,
+        cache_read_tokens: u64,
+        cache_write_tokens: u64,
+        /// The part of the cache written with the 1-hour lifetime (0 on Codex, which bills no write).
+        cache_write_1h_tokens: u64,
+        /// True when the token numbers cover only part of the turn: Claude's sub-model work is not in `usage`, and a Codex
+        /// turn with no usage update to measure from reports none.
+        incomplete: bool,
     },
     /// `fatal`: the CLI process is gone and the next turn needs a new one.
     Error {
```

`engine/src/lib.rs`

```diff
--- a/engine/src/lib.rs
+++ b/engine/src/lib.rs
@@ -13,6 +13,7 @@
 pub mod route;
 pub mod router;
 pub mod store;
+pub mod usage;
 pub mod workspace;
 
 pub use account::AccountStatus;
```

`engine/src/model.rs`

```diff
--- a/engine/src/model.rs
+++ b/engine/src/model.rs
@@ -118,6 +118,17 @@
     pub input_tokens: Option<u64>,
     #[serde(default, skip_serializing_if = "Option::is_none")]
     pub output_tokens: Option<u64>,
+    /// Prompt tokens of the main response read from the CLI's cache, and written to it (the part with the 1-hour lifetime
+    /// separately). Claude Code only; Codex bills no write.
+    #[serde(default, skip_serializing_if = "Option::is_none")]
+    pub cache_read_tokens: Option<u64>,
+    #[serde(default, skip_serializing_if = "Option::is_none")]
+    pub cache_write_tokens: Option<u64>,
+    #[serde(default, skip_serializing_if = "Option::is_none")]
+    pub cache_write_1h_tokens: Option<u64>,
+    /// True when the token numbers cover only part of the turn (sub-model work, or no usage was seen).
+    #[serde(default, skip_serializing_if = "Option::is_none")]
+    pub incomplete: Option<bool>,
 }
 
 impl Message {
@@ -141,6 +152,10 @@
             cost_usd: None,
             input_tokens: None,
             output_tokens: None,
+            cache_read_tokens: None,
+            cache_write_tokens: None,
+            cache_write_1h_tokens: None,
+            incomplete: None,
         }
     }
 }
@@ -163,6 +178,9 @@
     /// One CLI session per vendor, so switching models keeps each vendor's own history and cache.
     #[serde(default)]
     pub sessions: BTreeMap<Vendor, String>,
+    /// The cumulative cost each CLI session last reported, so a turn costs the difference (`usage::settle`).
+    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
+    pub cost_base: BTreeMap<Vendor, crate::usage::CostBase>,
     /// Tools the user chose to always allow in this conversation.
     #[serde(default, skip_serializing_if = "Option::is_none")]
     pub allow_always: Option<Vec<String>>,
```

`engine/src/usage.rs`

```diff
--- a/engine/src/usage.rs
+++ b/engine/src/usage.rs
@@ -0,0 +1,99 @@
+//! The cost of one turn, from the figures a CLI reports.
+//!
+//! Claude Code's `total_cost_usd` is cumulative for the conversation, even across a resume, while `usage` is for the turn.
+//! The engine keeps the last total it trusted and a turn costs the difference.
+
+use serde::{Deserialize, Serialize};
+
+/// The last cumulative cost a CLI session reported.
+#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
+pub struct CostBase {
+    /// The CLI session id the total belongs to.
+    pub session: String,
+    pub total: f64,
+}
+
+#[derive(Debug, Clone, PartialEq)]
+pub struct Settled {
+    /// What this turn cost, when it can be known.
+    pub cost: Option<f64>,
+    /// The base to keep from now on; None leaves the old one alone.
+    pub base: Option<CostBase>,
+}
+
+/// `total` is None when the result carried no counters (an aborted turn reports 0 and an empty usage): that is "no
+/// information", never a reset. `fresh` is true when the CLI session started in this conversation without a resume.
+pub fn settle(base: Option<&CostBase>, session: &str, fresh: bool, total: Option<f64>) -> Settled {
+    let Some(total) = total.filter(|t| t.is_finite() && *t >= 0.0) else { return Settled { cost: None, base: None } };
+    let cost = match base {
+        Some(b) if b.session == session => (total >= b.total).then_some(total - b.total),
+        // A CLI session of its own starts again from zero.
+        Some(_) => Some(total),
+        None if fresh => Some(total),
+        // A resumed session this engine has no base for (a conversation saved before the base existed): the total is
+        // not this turn's, but it is the base from now on.
+        None => None,
+    };
+    Settled { cost, base: Some(CostBase { session: session.to_string(), total }) }
+}
+
+#[cfg(test)]
+mod tests {
+    use super::*;
+
+    fn near(got: Option<f64>, want: f64) {
+        assert!((got.expect("a cost") - want).abs() < 1e-9, "{got:?} is not {want}");
+    }
+
+    fn base(session: &str, total: f64) -> CostBase {
+        CostBase { session: session.into(), total }
+    }
+
+    #[test]
+    fn the_first_turn_of_a_new_session_costs_its_whole_total() {
+        let s = settle(None, "s1", true, Some(0.00018326));
+        near(s.cost, 0.00018326);
+        assert_eq!(s.base, Some(base("s1", 0.00018326)));
+    }
+
+    #[test]
+    fn a_resumed_turn_costs_the_difference() {
+        // claude/resume.jsonl after claude/interrupt.jsonl: 0.00023211 in all, 0.00018326 of it from the earlier process.
+        let s = settle(Some(&base("s1", 0.00018326)), "s1", false, Some(0.00023211));
+        near(s.cost, 0.00004885);
+        assert_eq!(s.base, Some(base("s1", 0.00023211)));
+    }
+
+    #[test]
+    fn an_aborted_result_changes_nothing() {
+        let old = base("s1", 0.5);
+        assert_eq!(settle(Some(&old), "s1", false, None), Settled { cost: None, base: None });
+    }
+
+    #[test]
+    fn a_new_session_id_starts_again_from_zero() {
+        let s = settle(Some(&base("s1", 0.5)), "s2", false, Some(0.01));
+        near(s.cost, 0.01);
+        assert_eq!(s.base, Some(base("s2", 0.01)));
+    }
+
+    #[test]
+    fn a_total_that_goes_down_inside_one_session_is_not_a_cost() {
+        let s = settle(Some(&base("s1", 0.5)), "s1", false, Some(0.2));
+        assert_eq!(s.cost, None);
+        assert_eq!(s.base, Some(base("s1", 0.2)), "the lower total is the base from now on");
+    }
+
+    #[test]
+    fn a_resumed_session_without_a_base_costs_nothing_known_but_sets_the_base() {
+        let s = settle(None, "s1", false, Some(0.3));
+        assert_eq!(s.cost, None);
+        assert_eq!(s.base, Some(base("s1", 0.3)));
+    }
+
+    #[test]
+    fn nonsense_totals_are_ignored() {
+        assert_eq!(settle(None, "s", true, Some(f64::NAN)).base, None);
+        assert_eq!(settle(None, "s", true, Some(-1.0)).base, None);
+    }
+}
```

- [ ] **Step 4: Run the engine suite, clippy and fmt**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): per-turn cost, cache numbers and the cache probe"
```

### Task 3: Budget maths and the gate

This task also removes the local test-build junction: `testkit` resolves its app and screenshots under `CARGO_TARGET_DIR/faber-test` (relative paths are resolved from the repository), and the build uses that same path. Tests cover default, relative and absolute target directories.

The multiplier, the guards (stale, jumped, relearning, billing) and the one gate.

**Files:**
- `engine/src/budget.rs`
- `engine/src/lib.rs`
- `engine/tests/budget.rs`
- `testkit/src/app.rs`, `testkit/src/testbuild.rs` (target-directory cleanup)

**Interfaces:**
- Consumes: nothing. Produces: `budget::{Billing, Reach, Purpose, Why, Snap, Shape, family_of, gate(snap, family, purpose, approved, now) -> Result<(), Why>}`, `Snap::{profile_m, step_ok}`, the price multiplier `m = clamp((0.3*sqrt(left)/(1-p))^2, 0.1, 25)` with `p = used / max(elapsed, 0.10)`; `elapsed` is uniform (1/7 a day) until a shape is learned (D13).

- [ ] **Step 1: Add the tests (they fail or do not compile yet)**

```diff
--- a/engine/tests/budget.rs
+++ b/engine/tests/budget.rs
@@ -0,0 +1,286 @@
+use faber_engine::budget::{
+    Billing, Purpose, Reach, Shape, Snap, State, Wait, Why, Window, bin, elapsed, family_of, gate, m_of, vendor_wide, wait, window_m,
+    window_secs,
+};
+
+/// 5 January 1970 00:00 UTC, a Monday.
+const MONDAY: f64 = 345_600.0;
+const HOUR: f64 = 3600.0;
+const DAY: f64 = 86_400.0;
+const NOW: f64 = MONDAY + 2.0 * DAY;
+
+fn near(got: f64, want: f64, tolerance: f64) {
+    assert!((got - want).abs() <= tolerance, "{got} is not {want}");
+}
+
+fn window(used: f64, resets_in: f64, length: f64) -> Window {
+    Window { used, resets_at: NOW + resets_in, read_at: NOW - 60.0, length }
+}
+
+fn five(used: f64, resets_in: f64) -> (String, Window) {
+    ("five_hour".into(), window(used, resets_in, 5.0 * HOUR))
+}
+
+fn seven(used: f64, resets_in: f64) -> (String, Window) {
+    ("seven_day".into(), window(used, resets_in, 7.0 * DAY))
+}
+
+fn snap(windows: Vec<(String, Window)>) -> Snap {
+    Snap { windows, billing: Billing::Subscription, plan_ok: true, ..Snap::default() }
+}
+
+fn m(s: &Snap) -> f64 {
+    s.vendor_m(&Shape::uniform(), 0, NOW)
+}
+
+#[test]
+fn the_multiplier_follows_the_projection() {
+    near(m_of(0.7, 1.0), 1.0, 1e-9);
+    near(m_of(0.5, 1.0), 0.36, 1e-9);
+    near(m_of(0.4, 1.0), 0.25, 1e-9);
+    near(m_of(0.2, 1.0), 0.140625, 1e-9);
+    near(m_of(0.94, 1.0), 25.0, 1e-6);
+    assert_eq!(m_of(1.0, 1.0), 25.0);
+    assert_eq!(m_of(3.0, 0.2), 25.0);
+    // One day of seven left: the margin shrinks, so a week at 45% is nearly all waste.
+    near(m_of(0.45, 1.0 / 7.0), 0.1, 1e-9);
+    near(m_of(0.887, 1.0 / 7.0), 1.0, 0.02);
+    near(m_of(0.95, 1.0 / 7.0), 5.1, 0.1);
+    assert_eq!(m_of(f64::NAN, 1.0), 1.0);
+    assert_eq!(m_of(-4.0, 1.0), 0.1);
+}
+
+#[test]
+fn windows_are_told_apart_by_name() {
+    assert_eq!(window_secs("five_hour"), Some(18_000.0));
+    assert_eq!(window_secs("seven_day"), Some(604_800.0));
+    assert_eq!(window_secs("seven_day_opus"), Some(604_800.0));
+    assert_eq!(window_secs("300_minutes"), Some(18_000.0));
+    assert_eq!(window_secs("0_minutes"), None);
+    assert_eq!(window_secs("odd"), None);
+    assert!(vendor_wide("five_hour") && vendor_wide("10080_minutes") && !vendor_wide("seven_day_opus") && !vendor_wide("odd"));
+    assert_eq!(family_of("seven_day_opus"), Some("opus"));
+    assert_eq!(family_of("claude-sonnet-5-5"), Some("sonnet"));
+    assert_eq!(family_of("gpt-6.1-sol"), None);
+}
+
+#[test]
+fn the_week_starts_on_monday_in_local_time() {
+    assert_eq!(bin(MONDAY, 0), 0);
+    assert_eq!(bin(MONDAY + 6.0 * DAY + 23.0 * HOUR, 0), 167);
+    assert_eq!(bin(0.0, 0), 3 * 24, "1 January 1970 was a Thursday");
+    // Brazil: 03:00 UTC on Monday is midnight on Monday.
+    assert_eq!(bin(MONDAY + 3.0 * HOUR, -3 * 3600), 0);
+    assert_eq!(bin(MONDAY + 2.0 * HOUR, -3 * 3600), 167, "and an hour before local midnight it is still Sunday night");
+}
+
+#[test]
+fn elapsed_time_is_weighted_by_the_shape() {
+    let mut monday_only = [0.0; 168];
+    monday_only[..24].fill(1.0 / 24.0);
+    let shape = Shape(monday_only);
+    let end = MONDAY + 7.0 * DAY;
+    near(elapsed(&shape, MONDAY, MONDAY + 12.0 * HOUR, end, 0), 0.5, 1e-9);
+    near(elapsed(&shape, MONDAY, MONDAY + 5.0 * DAY, end, 0), 1.0, 1e-9);
+    near(elapsed(&Shape::uniform(), MONDAY, MONDAY + 3.5 * DAY, end, 0), 0.5, 1e-9);
+}
+
+#[test]
+fn a_five_hour_window_across_the_end_of_the_week_wraps() {
+    let start = MONDAY + 6.0 * DAY + 21.0 * HOUR; // Sunday 21:00
+    near(elapsed(&Shape::uniform(), start, start + 2.5 * HOUR, start + 5.0 * HOUR, 0), 0.5, 1e-9);
+}
+
+#[test]
+fn a_shape_with_no_mass_falls_back_to_plain_time() {
+    let empty = Shape([0.0; 168]);
+    near(elapsed(&empty, 0.0, 250.0, 1000.0, 0), 0.25, 1e-9);
+    assert_eq!(elapsed(&Shape::uniform(), 100.0, 50.0, 100.0, 0), 1.0, "a window of no length is over");
+}
+
+#[test]
+fn a_window_is_priced_from_its_projection() {
+    let shape = Shape::uniform();
+    // Half the five hours passed, 10% used: the projection is 20%, far under the pivot.
+    let w = window(0.10, 2.5 * HOUR, 5.0 * HOUR);
+    near(window_m(&w, NOW, &shape, 0, true), 0.1, 1e-9);
+    // 80% used at the halfway mark runs out.
+    assert_eq!(window_m(&window(0.8, 2.5 * HOUR, 5.0 * HOUR), NOW, &shape, 0, true), 25.0);
+}
+
+#[test]
+fn a_discount_is_not_believed_when_the_reading_may_be_wrong() {
+    let shape = Shape::uniform();
+    let spare = window(0.10, 2.5 * HOUR, 5.0 * HOUR);
+    assert_eq!(window_m(&spare, NOW, &shape, 0, false), 1.0, "guards hold");
+    let old = Window { read_at: NOW - 7.0 * HOUR, ..spare };
+    assert_eq!(window_m(&old, NOW, &shape, 0, true), 1.0, "older than six hours");
+    let from_the_future = Window { read_at: NOW + 100.0, ..spare };
+    assert_eq!(window_m(&from_the_future, NOW, &shape, 0, true), 1.0, "the clock went back");
+    let over = Window { read_at: NOW - 7.0 * HOUR, ..window(0.8, 2.5 * HOUR, 5.0 * HOUR) };
+    assert_eq!(window_m(&over, NOW, &shape, 0, true), 25.0, "a stale reading still prices up");
+}
+
+#[test]
+fn a_reading_that_cannot_be_used_gives_no_information() {
+    let shape = Shape::uniform();
+    assert_eq!(window_m(&window(0.1, -10.0, 5.0 * HOUR), NOW, &shape, 0, true), 1.0, "the window already reset");
+    assert_eq!(window_m(&window(f64::NAN, 100.0, 5.0 * HOUR), NOW, &shape, 0, true), 1.0);
+    assert_eq!(window_m(&window(0.1, 100.0, 0.0), NOW, &shape, 0, true), 1.0, "no known length");
+}
+
+#[test]
+fn the_vendor_takes_its_worst_vendor_wide_window() {
+    let s = snap(vec![five(0.10, 2.5 * HOUR), seven(0.45, 3.5 * DAY)]);
+    near(m(&s), 4.5, 1e-6);
+    assert_eq!(m(&snap(vec![])), 1.0, "no reading, no information");
+}
+
+#[test]
+fn a_family_window_prices_only_its_own_family() {
+    let opus = ("seven_day_opus".to_string(), window(0.45, 3.5 * DAY, 7.0 * DAY));
+    let s = snap(vec![five(0.10, 2.5 * HOUR), opus]);
+    let shape = Shape::uniform();
+    assert_eq!(s.vendor_m(&shape, 0, NOW), 0.1);
+    near(s.profile_m(Some("opus"), &shape, 0, NOW), 4.5, 1e-6);
+    assert_eq!(s.profile_m(Some("sonnet"), &shape, 0, NOW), 0.1);
+    assert_eq!(s.profile_m(None, &shape, 0, NOW), 0.1);
+}
+
+#[test]
+fn doubt_removes_discounts_but_not_the_warning() {
+    let spare = vec![five(0.10, 2.5 * HOUR)];
+    let high = vec![seven(0.45, 3.5 * DAY)];
+    for doubt in [
+        Snap { jumped: true, ..snap(spare.clone()) },
+        Snap { relearning: true, ..snap(spare.clone()) },
+        Snap { plan_ok: false, ..snap(spare.clone()) },
+        Snap { billing: Billing::Unknown, ..snap(spare.clone()) },
+    ] {
+        assert_eq!(m(&doubt), 1.0);
+    }
+    near(m(&Snap { jumped: true, ..snap(high) }), 4.5, 1e-6); // running low still shows
+}
+
+#[test]
+fn the_screen_words_follow_the_multiplier() {
+    let shape = Shape::uniform();
+    let state = |s: &Snap| s.state(&shape, 0, NOW);
+    assert_eq!(state(&snap(vec![five(0.10, 2.5 * HOUR)])), State::Spare);
+    assert_eq!(state(&snap(vec![five(0.40, 2.5 * HOUR)])), State::Normal);
+    assert_eq!(state(&snap(vec![seven(0.45, 3.5 * DAY)])), State::Tight);
+    assert_eq!(state(&snap(vec![seven(0.92, 6.0 * DAY)])), State::Tight, "less than 10% left");
+    assert_eq!(state(&Snap { plan_ok: false, ..snap(vec![five(0.10, 2.5 * HOUR)]) }), State::Normal);
+}
+
+#[test]
+fn a_vendor_is_exhausted_when_it_cannot_answer_at_all() {
+    assert!(snap(vec![seven(0.99, DAY)]).exhausted(NOW));
+    assert!(!snap(vec![seven(0.99, -5.0)]).exhausted(NOW), "its reset has passed");
+    assert!(!snap(vec![seven(0.97, DAY)]).exhausted(NOW));
+    assert!(Snap { reach: Reach::Out(None), ..snap(vec![]) }.exhausted(NOW));
+    assert!(Snap { reach: Reach::Out(Some(NOW + 60.0)), ..snap(vec![]) }.exhausted(NOW));
+    assert!(!Snap { reach: Reach::Out(Some(NOW - 60.0)), ..snap(vec![]) }.exhausted(NOW));
+    assert!(Snap { overage_active: true, ..snap(vec![]) }.exhausted(NOW), "paid credits count as exhausted");
+}
+
+#[test]
+fn a_full_family_window_removes_only_that_family() {
+    let s = snap(vec![five(0.2, HOUR), ("seven_day_opus".into(), window(1.0, DAY, 7.0 * DAY))]);
+    assert!(!s.exhausted(NOW));
+    assert!(!s.usable(Some("opus"), NOW));
+    assert!(s.usable(Some("sonnet"), NOW));
+    assert!(s.usable(None, NOW));
+}
+
+#[test]
+fn nothing_is_usable_above_the_admission_limit() {
+    assert!(snap(vec![five(0.98, HOUR)]).usable(None, NOW));
+    assert!(!snap(vec![five(0.985, HOUR)]).usable(None, NOW));
+}
+
+#[test]
+fn a_step_needs_every_window_at_ninety_percent_or_less() {
+    assert!(snap(vec![five(0.9, HOUR), seven(0.5, DAY)]).step_ok(NOW));
+    assert!(!snap(vec![five(0.5, HOUR), seven(0.91, DAY)]).step_ok(NOW));
+}
+
+#[test]
+fn recovery_is_the_latest_blocking_reset() {
+    let s = snap(vec![five(1.0, 1800.0), seven(1.0, 2.0 * DAY)]);
+    assert_eq!(s.recovery(NOW), Some(NOW + 2.0 * DAY));
+    assert_eq!(Snap { reach: Reach::Out(None), ..s.clone() }.recovery(NOW), None, "a block with no reset time is unknown");
+    assert_eq!(Snap { overage_active: true, ..s }.recovery(NOW), None);
+    let five_only = snap(vec![five(1.0, 1800.0), seven(0.3, 2.0 * DAY)]);
+    assert_eq!(five_only.recovery(NOW), Some(NOW + 1800.0), "an open weekly window does not delay the five hours");
+}
+
+#[test]
+fn a_message_waits_for_the_first_vendor_back_if_that_is_soon() {
+    let soon = snap(vec![five(1.0, 2.0 * HOUR)]);
+    let late = snap(vec![seven(1.0, 2.0 * DAY)]);
+    let unknown = Snap { reach: Reach::Out(None), ..snap(vec![]) };
+    assert_eq!(wait(&[&soon, &late], NOW), Wait::Queue(NOW + 2.0 * HOUR));
+    assert_eq!(wait(&[&unknown, &soon], NOW), Wait::Queue(NOW + 2.0 * HOUR));
+    assert_eq!(wait(&[&unknown, &late], NOW), Wait::Refuse(Some(NOW + 2.0 * DAY)));
+    assert_eq!(wait(&[&unknown], NOW), Wait::Refuse(None));
+    let seven_hours = snap(vec![five(1.0, 7.0 * HOUR)]);
+    assert_eq!(wait(&[&seven_hours], NOW), Wait::Refuse(Some(NOW + 7.0 * HOUR)));
+}
+
+const TURN: Purpose = Purpose::UserTurn { by_hand: false };
+
+#[test]
+fn the_gate_lets_a_vendor_with_no_reading_answer() {
+    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary, Purpose::PlanCheck] {
+        assert_eq!(gate(&snap(vec![]), None, purpose, false, NOW), Ok(()));
+    }
+}
+
+#[test]
+fn the_gate_denies_unknown_billing_for_every_automatic_request() {
+    let unknown = Snap::default();
+    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary] {
+        assert_eq!(gate(&unknown, None, purpose, false, NOW), Err(Why::Billing));
+        assert_eq!(gate(&unknown, None, purpose, true, NOW), Err(Why::Billing));
+    }
+    assert_eq!(gate(&unknown, None, Purpose::PlanCheck, false, NOW), Ok(()));
+    assert_eq!(gate(&unknown, None, Purpose::UserTurn { by_hand: true }, false, NOW), Ok(()));
+}
+
+#[test]
+fn the_gate_keeps_usage_billed_logins_for_the_user_alone() {
+    let billed = Snap { billing: Billing::UsageBilled, ..snap(vec![]) };
+    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary] {
+        assert_eq!(gate(&billed, None, purpose, false, NOW), Err(Why::Billing));
+    }
+    assert_eq!(gate(&billed, None, Purpose::UserTurn { by_hand: true }, false, NOW), Ok(()));
+    assert_eq!(gate(&billed, None, Purpose::PlanCheck, false, NOW), Ok(()));
+    assert_eq!(gate(&billed, None, TURN, true, NOW), Err(Why::Billing), "an approval never lifts the billing rule");
+}
+
+#[test]
+fn the_gate_denies_what_is_out_or_nearly_out() {
+    assert_eq!(gate(&snap(vec![seven(1.0, DAY)]), None, TURN, false, NOW), Err(Why::Exhausted));
+    let opus_out = snap(vec![five(0.2, HOUR), ("seven_day_opus".into(), window(1.0, DAY, 7.0 * DAY))]);
+    assert_eq!(gate(&opus_out, Some("opus"), TURN, false, NOW), Err(Why::Exhausted));
+    assert_eq!(gate(&opus_out, Some("sonnet"), TURN, false, NOW), Ok(()));
+    assert_eq!(gate(&snap(vec![five(0.985, HOUR)]), None, Purpose::Pulse, false, NOW), Err(Why::Window));
+}
+
+#[test]
+fn the_gate_is_stricter_where_the_cli_could_switch_to_paid_use_by_itself() {
+    let near_limit = Snap { paid_fallback: true, ..snap(vec![five(0.91, HOUR)]) };
+    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary] {
+        assert_eq!(gate(&near_limit, None, purpose, false, NOW), Err(Why::Paid));
+    }
+    assert_eq!(gate(&Snap { paid_fallback: false, ..near_limit.clone() }, None, TURN, false, NOW), Ok(()));
+    assert_eq!(gate(&Snap { paid_fallback: true, ..snap(vec![five(0.9, HOUR)]) }, None, TURN, false, NOW), Ok(()));
+}
+
+#[test]
+fn a_one_message_approval_lifts_the_quota_rules() {
+    let out = Snap { paid_fallback: true, ..snap(vec![seven(1.0, DAY)]) };
+    assert_eq!(gate(&out, None, TURN, false, NOW), Err(Why::Exhausted));
+    assert_eq!(gate(&out, None, TURN, true, NOW), Ok(()));
+}
```

- [ ] **Step 2: Run the new tests and see them fail**

Run: `cargo test -p faber-engine` (the named test files)
Expected: compile errors or failures naming the missing items.

- [ ] **Step 3: Implement**

`engine/src/budget.rs`

```diff
--- a/engine/src/budget.rs
+++ b/engine/src/budget.rs
@@ -0,0 +1,333 @@
+//! Quota arithmetic: how fast a window is being used against the time it has left, what that does to a model's price, and
+//! whether a vendor may be asked at all. Pure functions: no clock, no process, no disk. Every constant is a starting value
+//! that the simulation (engine/tests/simulation.rs) is allowed to tune.
+
+/// The safe share of a window to reach at its reset is `1 - MARGIN` scaled by the time left (see `m_of`).
+pub const MARGIN: f64 = 0.3;
+/// A window counts as at least this much elapsed, so a burst in its first minutes is not a runaway.
+pub const ELAPSED_FLOOR: f64 = 0.10;
+pub const M_MIN: f64 = 0.1;
+pub const M_MAX: f64 = 25.0;
+/// A reading older than this is not believed when it says there is quota to spare.
+pub const STALE_SECS: f64 = 6.0 * 3600.0;
+/// A step up is bought only in a vendor whose windows are all at or below this share.
+pub const STEP_LIMIT: f64 = 0.90;
+/// No request goes to a vendor with a window above this share.
+pub const ADMIT_LIMIT: f64 = 0.98;
+/// A window at or above this share is out.
+pub const OUT: f64 = 0.99;
+/// A message waits for quota only when the wait is this long or shorter.
+pub const QUEUE_SECS: f64 = 6.0 * 3600.0;
+/// `m` at or above this is "running low" on the screen.
+pub const LOW_M: f64 = 4.0;
+/// `m` at or below this is "plenty left" on the screen.
+pub const SPARE_M: f64 = 0.6;
+
+/// The price multiplier of one window: `p` is the share expected to be used at the reset, `left` the share of the
+/// window still ahead. `p` of 0.7 is the pivot with a full window left (price 1); a window projected far under the pivot
+/// is cheap, one projected over it is dear, and one projected to run out is at the ceiling.
+pub fn m_of(p: f64, left: f64) -> f64 {
+    if p.is_nan() {
+        return 1.0;
+    }
+    if p >= 1.0 {
+        return M_MAX;
+    }
+    let margin = MARGIN * left.clamp(0.0, 1.0).sqrt();
+    (margin / (1.0 - p.max(0.0))).powi(2).clamp(M_MIN, M_MAX)
+}
+
+/// The length of a window, from its name. Claude names its windows; Codex reports `<n>_minutes`. None for a name Faber
+/// does not know, which then gives no information (price 1) but still counts for exhaustion.
+pub fn window_secs(name: &str) -> Option<f64> {
+    if name.starts_with("five_hour") {
+        Some(18_000.0)
+    } else if name.starts_with("seven_day") {
+        Some(604_800.0)
+    } else {
+        name.strip_suffix("_minutes")?.parse::<f64>().ok().filter(|m| *m > 0.0).map(|m| m * 60.0)
+    }
+}
+
+/// Windows that cover the whole vendor; every other window is a family window.
+pub fn vendor_wide(name: &str) -> bool {
+    matches!(name, "five_hour" | "seven_day") || name.strip_suffix("_minutes").is_some_and(|n| n.parse::<u64>().is_ok())
+}
+
+/// The model family a family window belongs to, or the family of a Claude model id.
+pub fn family_of(name: &str) -> Option<&'static str> {
+    ["opus", "sonnet", "haiku"].into_iter().find(|f| name.contains(f))
+}
+
+/// How a week of use is spread over the hours of the week: weekday x hour in the PC's local time, Monday 00:00 first,
+/// summing to 1.
+#[derive(Debug, Clone, PartialEq)]
+pub struct Shape(pub [f64; 168]);
+
+impl Shape {
+    pub fn uniform() -> Shape {
+        Shape([1.0 / 168.0; 168])
+    }
+
+    /// The shape's mass between two instants (Unix seconds). `offset` is the local time's distance from UTC in seconds.
+    pub fn mass(&self, from: f64, to: f64, offset: i64) -> f64 {
+        if !(from.is_finite() && to.is_finite()) || to <= from {
+            return 0.0;
+        }
+        let to = to.min(from + 31.0 * 86_400.0);
+        let (mut t, mut sum) = (from, 0.0);
+        while t < to {
+            let hour_end = ((t + offset as f64) / 3600.0).floor() + 1.0;
+            let end = (hour_end * 3600.0 - offset as f64).max(t + 1.0).min(to);
+            sum += self.0[bin(t, offset)] * (end - t) / 3600.0;
+            t = end;
+        }
+        sum
+    }
+}
+
+/// The weekday-hour bin of an instant: Monday 00:00 is 0 and Sunday 23:00 is 167.
+pub fn bin(t: f64, offset: i64) -> usize {
+    let local = (t + offset as f64).floor() as i64;
+    let weekday = (local.div_euclid(86_400) + 3).rem_euclid(7); // 1 January 1970 was a Thursday
+    (weekday * 24 + local.rem_euclid(86_400) / 3600) as usize
+}
+
+/// The share of a window already passed, weighted by the shape (so a quiet Sunday does not count as time gone).
+pub fn elapsed(shape: &Shape, start: f64, now: f64, end: f64, offset: i64) -> f64 {
+    let total = shape.mass(start, end, offset);
+    if total <= 0.0 {
+        return if end > start { ((now - start) / (end - start)).clamp(0.0, 1.0) } else { 1.0 };
+    }
+    (shape.mass(start, now.min(end), offset) / total).clamp(0.0, 1.0)
+}
+
+/// The latest reading of one quota window.
+#[derive(Debug, Clone, Copy, PartialEq)]
+pub struct Window {
+    /// 0 to 1.
+    pub used: f64,
+    /// Unix seconds; a window whose reset has passed is ignored.
+    pub resets_at: f64,
+    /// When Faber read it, Unix seconds.
+    pub read_at: f64,
+    /// Seconds; 0 when the window's name says nothing about it.
+    pub length: f64,
+}
+
+impl Window {
+    fn live(&self, now: f64) -> bool {
+        self.used.is_finite() && self.resets_at > now
+    }
+}
+
+/// The multiplier of one window. 1 when there is nothing to go on; never below 1 when `discount_ok` is false or the
+/// reading is older than six hours (or from the future, which a clock set backwards makes look like).
+pub fn window_m(w: &Window, now: f64, shape: &Shape, offset: i64, discount_ok: bool) -> f64 {
+    if !w.live(now) || w.length <= 0.0 {
+        return 1.0;
+    }
+    let e = elapsed(shape, w.resets_at - w.length, now, w.resets_at, offset);
+    let m = m_of(w.used / e.max(ELAPSED_FLOOR), 1.0 - e);
+    if discount_ok && (0.0..=STALE_SECS).contains(&(now - w.read_at)) { m } else { m.max(1.0) }
+}
+
+/// Whether the login is a subscription. Auto requires a confirmed subscription even before the first quota reading.
+/// An unknown or usage-billed login is available only for a hand-picked user turn.
+#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
+pub enum Billing {
+    Subscription,
+    UsageBilled,
+    #[default]
+    Unknown,
+}
+
+/// Whether the CLI itself said the vendor cannot answer.
+#[derive(Debug, Clone, Copy, Default, PartialEq)]
+pub enum Reach {
+    #[default]
+    Clear,
+    /// Out; the reset time in Unix seconds when the CLI gave one.
+    Out(Option<f64>),
+}
+
+/// Everything the router and the gate know about one vendor at one moment.
+#[derive(Debug, Clone, Default, PartialEq)]
+pub struct Snap {
+    pub windows: Vec<(String, Window)>,
+    pub billing: Billing,
+    pub reach: Reach,
+    /// The CLI says it is spending paid extra usage now.
+    pub overage_active: bool,
+    /// The CLI may switch to paid use by itself (extra usage on, or credits available).
+    pub paid_fallback: bool,
+    /// Use outside Faber jumped in the last two hours.
+    pub jumped: bool,
+    /// The plan changed and the weights are being learned again.
+    pub relearning: bool,
+    /// A plan was detected less than 30 minutes ago, or the user typed one that no detection contradicts.
+    pub plan_ok: bool,
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum State {
+    Spare,
+    Normal,
+    Tight,
+}
+
+impl Snap {
+    fn wide(&self, now: f64) -> impl Iterator<Item = &Window> {
+        self.windows.iter().filter(move |(n, w)| vendor_wide(n) && w.live(now)).map(|(_, w)| w)
+    }
+
+    /// Discounts (a multiplier below 1) are believed only while nothing makes the reading doubtful.
+    pub fn discount_ok(&self) -> bool {
+        self.plan_ok && !self.jumped && !self.relearning && self.billing == Billing::Subscription
+    }
+
+    /// The highest multiplier over the vendor-wide windows; 1 with no live reading.
+    pub fn vendor_m(&self, shape: &Shape, offset: i64, now: f64) -> f64 {
+        let ok = self.discount_ok();
+        self.windows
+            .iter()
+            .filter(|(n, _)| vendor_wide(n))
+            .map(|(_, w)| window_m(w, now, shape, offset, ok))
+            .reduce(f64::max)
+            .unwrap_or(1.0)
+    }
+
+    /// The multiplier of a profile: its vendor's, or its family window's when that is higher.
+    pub fn profile_m(&self, family: Option<&str>, shape: &Shape, offset: i64, now: f64) -> f64 {
+        let ok = self.discount_ok();
+        let own = self
+            .windows
+            .iter()
+            .filter(|(n, _)| !vendor_wide(n) && family.is_some() && family_of(n) == family)
+            .map(|(_, w)| window_m(w, now, shape, offset, ok))
+            .reduce(f64::max);
+        self.vendor_m(shape, offset, now).max(own.unwrap_or(0.0))
+    }
+
+    /// True when every vendor-wide window is at or below the step limit.
+    pub fn step_ok(&self, now: f64) -> bool {
+        self.wide(now).all(|w| w.used <= STEP_LIMIT)
+    }
+
+    fn top(&self, now: f64) -> f64 {
+        self.wide(now).map(|w| w.used).fold(0.0, f64::max)
+    }
+
+    /// The vendor cannot answer at all: a vendor-wide window is out, the CLI says so, or paid extra usage is running.
+    pub fn exhausted(&self, now: f64) -> bool {
+        self.wide(now).any(|w| w.used >= OUT)
+            || self.overage_active
+            || match self.reach {
+                Reach::Out(Some(until)) => until > now,
+                Reach::Out(None) => true,
+                Reach::Clear => false,
+            }
+    }
+
+    /// A model family is out while the rest of the vendor is not.
+    pub fn family_out(&self, family: &str, now: f64) -> bool {
+        self.windows.iter().any(|(n, w)| !vendor_wide(n) && family_of(n) == Some(family) && w.live(now) && w.used >= OUT)
+    }
+
+    /// May a profile of this family be chosen: its vendor is not exhausted, its family is not out, and no vendor-wide
+    /// window is above the admission limit. Paid credits never make anything usable.
+    pub fn usable(&self, family: Option<&str>, now: f64) -> bool {
+        !self.exhausted(now) && !family.is_some_and(|f| self.family_out(f, now)) && self.top(now) <= ADMIT_LIMIT
+    }
+
+    /// When the vendor can answer again, as Unix seconds: the latest reset among everything that blocks it. None when a
+    /// block has no reset time (it then counts as longer than any wait). Meaningful only when `exhausted`.
+    pub fn recovery(&self, now: f64) -> Option<f64> {
+        let mut latest = self.wide(now).filter(|w| w.used >= OUT).map(|w| w.resets_at).reduce(f64::max);
+        match self.reach {
+            Reach::Out(Some(until)) if until > now => latest = Some(latest.map_or(until, |a| a.max(until))),
+            Reach::Out(None) => return None,
+            _ => {}
+        }
+        if self.overage_active {
+            return None;
+        }
+        latest
+    }
+
+    /// The word the screen shows for the vendor.
+    pub fn state(&self, shape: &Shape, offset: i64, now: f64) -> State {
+        let m = self.vendor_m(shape, offset, now);
+        if m >= LOW_M || self.top(now) > 0.9 {
+            State::Tight
+        } else if m <= SPARE_M && self.discount_ok() {
+            State::Spare
+        } else {
+            State::Normal
+        }
+    }
+}
+
+/// What to do with a message when the vendor it needs is out.
+#[derive(Debug, Clone, Copy, PartialEq)]
+pub enum Wait {
+    /// Send it itself at this time (Unix seconds).
+    Queue(f64),
+    /// Do not queue it. The earliest known recovery, for the screen.
+    Refuse(Option<f64>),
+}
+
+/// The earliest recovery across the exhausted vendors; an unknown recovery counts as longer than the queue limit.
+pub fn wait(exhausted: &[&Snap], now: f64) -> Wait {
+    let earliest = exhausted.iter().filter_map(|s| s.recovery(now)).reduce(f64::min);
+    match earliest {
+        Some(t) if t - now <= QUEUE_SECS => Wait::Queue(t),
+        other => Wait::Refuse(other),
+    }
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum Purpose {
+    /// `by_hand`: the user picked this vendor themselves.
+    UserTurn {
+        by_hand: bool,
+    },
+    Router,
+    Pulse,
+    Summary,
+    /// A local status command that spends no quota; listed so every request goes through the same door.
+    PlanCheck,
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum Why {
+    Billing,
+    Exhausted,
+    Window,
+    Paid,
+}
+
+/// The one door every request to a CLI goes through. `approved` is the user's one-message approval to spend paid credits
+/// on this vendor; it lifts the quota rules for that message and nothing else.
+pub fn gate(s: &Snap, family: Option<&str>, purpose: Purpose, approved: bool, now: f64) -> Result<(), Why> {
+    if purpose == Purpose::PlanCheck {
+        return Ok(());
+    }
+    if s.billing != Billing::Subscription && purpose != (Purpose::UserTurn { by_hand: true }) {
+        return Err(Why::Billing);
+    }
+    if approved {
+        return Ok(());
+    }
+    if s.exhausted(now) || family.is_some_and(|f| s.family_out(f, now)) {
+        return Err(Why::Exhausted);
+    }
+    let top = s.top(now);
+    if top > ADMIT_LIMIT {
+        return Err(Why::Window);
+    }
+    if s.paid_fallback && top > STEP_LIMIT {
+        return Err(Why::Paid);
+    }
+    Ok(())
+}
```

`engine/src/lib.rs`

```diff
--- a/engine/src/lib.rs
+++ b/engine/src/lib.rs
@@ -2,6 +2,7 @@
 //! The app shell (Tauri) only calls into it; nothing here knows about windows or screens.
 
 pub mod account;
+pub mod budget;
 pub mod claude;
 pub mod cli;
 pub mod codex;
```

- [ ] **Step 4: Run the engine suite, clippy and fmt**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): budget maths and the gate (pure)"
```

### Task 4: Signals, learning and the ledger (pure)

Everything the engine will remember, as plain testable functions.

**Files:**
- `engine/src/learn.rs`
- `engine/src/ledger.rs`
- `engine/src/lib.rs`
- `engine/src/signals.rs`
- `engine/tests/learn.rs`
- `engine/tests/ledger.rs`
- `engine/tests/signals.rs`

**Interfaces:**
- Produces: `signals::{Signals, claude_signals, codex_signals, codex_account, claude_plan}`, `learn::{Point, Span, Learned, Stretch, Weights, outside, stretches, weights, capacity_changed}`, `ledger::{Ledger, VendorLedger, Reading, plausible}` with `record`, `span`, `learn_weights(now) -> bool`, `plan_detected`, `plan_failed`, `plan_missing`, `ask_for_plan`, `plan_ok`, `relearning`, `shape`, `snap`, `Ledger::sanitize`.

- [ ] **Step 1: Add the tests (they fail or do not compile yet)**

```diff
--- a/engine/tests/learn.rs
+++ b/engine/tests/learn.rs
@@ -0,0 +1,214 @@
+use faber_engine::learn::{Learned, Point, Span, Stretch, capacity_changed, outside, stretches, weights};
+
+/// 5 January 1970 00:00 UTC, a Monday.
+const MONDAY: f64 = 345_600.0;
+const HOUR: f64 = 3600.0;
+const DAY: f64 = 86_400.0;
+
+fn near(got: f64, want: f64, tolerance: f64) {
+    assert!((got - want).abs() <= tolerance, "{got} is not {want}");
+}
+
+fn at(t: f64, points: f64, resets_at: f64) -> Point {
+    Point { at: t, used: points / 100.0, resets_at }
+}
+
+#[test]
+fn without_data_the_week_is_flat() {
+    assert_eq!(Learned::default().shape(), faber_engine::budget::Shape::uniform());
+}
+
+#[test]
+fn movement_is_spread_over_the_hours_the_gap_overlaps() {
+    let mut l = Learned::default();
+    // 09:30 to 11:30 on a Monday: half an hour in the 09 bin, one in the 10, half in the 11; 4 points in all.
+    l.add(&at(MONDAY + 9.5 * HOUR, 10.0, 1.0), &at(MONDAY + 11.5 * HOUR, 14.0, 1.0), 0);
+    near(l.mass[9], 1.0, 1e-9);
+    near(l.mass[10], 2.0, 1e-9);
+    near(l.mass[11], 1.0, 1e-9);
+    near(l.covered, 2.0 * HOUR, 1e-9);
+}
+
+#[test]
+fn gaps_that_say_nothing_are_left_out() {
+    let mut l = Learned::default();
+    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + 13.0 * HOUR, 20.0, 1.0), 0);
+    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + HOUR, 20.0, 2.0), 0);
+    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + HOUR, 5.0, 1.0), 0);
+    l.add(&at(MONDAY + HOUR, 10.0, 1.0), &at(MONDAY, 10.0, 1.0), 0);
+    assert_eq!(l.covered, 0.0);
+    assert!(l.mass.iter().all(|m| *m == 0.0));
+    // No movement still counts as time covered: an idle night is information.
+    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + HOUR, 10.0, 1.0), 0);
+    assert_eq!(l.covered, HOUR);
+}
+
+#[test]
+fn two_weeks_of_weekday_use_teach_the_shape() {
+    let mut l = Learned::default();
+    for week in 0..2 {
+        let resets = MONDAY + (week + 1) as f64 * 7.0 * DAY;
+        let mut used = 0.0;
+        let mut previous = at(MONDAY + week as f64 * 7.0 * DAY, used, resets);
+        for h in 1..=168 {
+            let t = MONDAY + week as f64 * 7.0 * DAY + h as f64 * HOUR;
+            let hour_of_day = ((h - 1) % 24) as f64;
+            let weekday = (h - 1) / 24;
+            if weekday < 5 && (9.0..17.0).contains(&hour_of_day) {
+                used += 2.0;
+            }
+            let now = at(t, used, resets);
+            l.add(&previous, &now, 0);
+            previous = now;
+        }
+    }
+    let shape = l.shape();
+    assert!(shape.0[10] > 4.0 * shape.0[5 * 24 + 10], "Monday 10:00 against Saturday 10:00");
+    assert!(shape.0[10] > 4.0 * shape.0[3], "Monday 10:00 against Monday 03:00");
+    near(shape.0.iter().sum::<f64>(), 1.0, 1e-9);
+}
+
+#[test]
+fn a_little_data_changes_the_shape_only_a_little() {
+    let mut l = Learned::default();
+    l.add(&at(MONDAY, 0.0, 1.0), &at(MONDAY + HOUR, 8.0, 1.0), 0);
+    let shape = l.shape();
+    assert!(shape.0[0] > 1.0 / 168.0);
+    assert!(shape.0[0] < 2.0 / 168.0, "one hour of data out of two weeks does not make the week");
+    near(shape.0.iter().sum::<f64>(), 1.0, 1e-9);
+}
+
+#[test]
+fn movement_with_no_turn_of_faber_in_it_is_outside_use() {
+    let now = MONDAY + 2.0 * DAY;
+    let points = [
+        at(now - 20.0 * HOUR, 10.0, 9.0),
+        at(now - 18.0 * HOUR, 16.0, 9.0),
+        at(now - 16.0 * HOUR, 25.0, 9.0),
+        at(now - 14.0 * HOUR, 25.5, 9.0),
+    ];
+    let turn = Span { start: now - 17.0 * HOUR, end: now - 16.5 * HOUR, model: "m".into(), cost: Some(1.0) };
+    let o = outside(&points, &[turn], now);
+    near(o.outside, 6.0, 1e-9); // the first gap; the second has a turn in it; the third moved less than two points
+    near(o.observed, 15.5, 1e-9);
+    near(o.share().unwrap(), 6.0 / 15.5, 1e-9);
+    assert_eq!(outside(&[], &[], now).share(), None);
+}
+
+#[test]
+fn a_burst_from_outside_is_a_jump() {
+    let now = MONDAY + 2.0 * DAY;
+    // No history: any three points count.
+    let burst = [at(now - 3.0 * HOUR, 10.0, 9.0), at(now - HOUR, 15.0, 9.0)];
+    assert!(outside(&burst, &[], now).jumped());
+    let small = [at(now - 3.0 * HOUR, 10.0, 9.0), at(now - HOUR, 12.0, 9.0)];
+    assert!(!outside(&small, &[], now).jumped());
+    // A steady point an hour all day (read every two hours): five points in the last two hours are only a little over the
+    // average, eight points are a jump.
+    let mut steady: Vec<Point> = (0..13).map(|i| at(now - (26 - 2 * i) as f64 * HOUR, 10.0 + 2.0 * i as f64, 9.0)).collect();
+    let mut calm = steady.clone();
+    calm.push(at(now, 34.0 + 5.0, 9.0));
+    let o = outside(&calm, &[], now);
+    near(o.rate, 1.0, 1e-9);
+    assert!(!o.jumped(), "{o:?}");
+    steady.push(at(now, 34.0 + 8.0, 9.0));
+    assert!(outside(&steady, &[], now).jumped());
+}
+
+fn quiet_week() -> Vec<Point> {
+    (0..8).map(|i| at(MONDAY + i as f64 * HOUR, i as f64 * 5.0, 9.0)).collect()
+}
+
+#[test]
+fn a_clean_run_of_turns_on_one_model_is_a_stretch() {
+    let points = quiet_week();
+    let spans = [Span { start: MONDAY + 2.1 * HOUR, end: MONDAY + 2.5 * HOUR, model: "a".into(), cost: Some(0.5) }];
+    // The 5 points between 02:00 and 03:00 for 0.5 of cost: 10 points per cost unit. The neighbours (5 points each) have
+    // no Faber turn in them, so use from elsewhere may hide in the stretch and nothing is accepted.
+    assert_eq!(stretches(&points, &spans), vec![]);
+    let mut points = quiet_week();
+    // Make the neighbours quiet: less than two points moved on either side.
+    points[2].used = points[1].used + 0.005;
+    points[4].used = points[3].used + 0.005;
+    let spans = [Span { start: MONDAY + 2.1 * HOUR, end: MONDAY + 2.5 * HOUR, model: "a".into(), cost: Some(0.5) }];
+    let found = stretches(&points, &spans);
+    assert_eq!(found.len(), 1);
+    assert_eq!(found[0].model, "a");
+    near(found[0].ppd, (points[3].used - points[2].used) * 100.0 / 0.5, 1e-9);
+}
+
+#[test]
+fn parallel_turns_other_models_unknown_costs_and_small_moves_are_not_stretches() {
+    let mut points = quiet_week();
+    points[2].used = points[1].used + 0.005;
+    points[4].used = points[3].used + 0.005;
+    let span = |s: f64, e: f64, model: &str, cost| Span { start: MONDAY + s * HOUR, end: MONDAY + e * HOUR, model: model.into(), cost };
+    let overlapping = [span(2.1, 2.6, "a", Some(0.5)), span(2.4, 2.9, "a", Some(0.5))];
+    assert_eq!(stretches(&points, &overlapping), vec![]);
+    let two_models = [span(2.1, 2.3, "a", Some(0.5)), span(2.4, 2.9, "b", Some(0.5))];
+    assert_eq!(stretches(&points, &two_models), vec![]);
+    assert_eq!(stretches(&points, &[span(2.1, 2.5, "a", None)]), vec![]);
+    assert_eq!(stretches(&points, &[span(2.1, 2.5, "a", Some(0.0))]), vec![]);
+    let mut small = points.clone();
+    small[3].used = small[2].used + 0.02;
+    assert_eq!(stretches(&small, &[span(2.1, 2.5, "a", Some(0.5))]), vec![], "fewer than three points");
+}
+
+fn run(model: &str, values: &[f64]) -> Vec<Stretch> {
+    values.iter().map(|v| Stretch { model: model.into(), ppd: *v }).collect()
+}
+
+#[test]
+fn a_weight_is_the_raw_cost_in_points_against_the_vendors_median() {
+    let mut s = run("cheap", &[10.0; 8]);
+    s.extend(run("dear", &[20.0; 8]));
+    s.extend(run("middle", &[14.0; 8]));
+    let w = weights(&s);
+    near(w.capacity.unwrap(), 14.0, 1e-9);
+    near(w.w["middle"], 1.0, 1e-9);
+    near(w.w["cheap"], 10.0 / 14.0, 1e-9);
+    near(w.w["dear"], 20.0 / 14.0, 1e-9);
+}
+
+#[test]
+fn weights_wait_for_enough_stretches_and_are_clamped() {
+    assert_eq!(weights(&run("a", &[10.0; 7])), Default::default(), "seven stretches are not enough");
+    let mut s = run("a", &[10.0; 8]);
+    s.extend(run("b", &[100.0; 8]));
+    s.extend(run("c", &[11.0; 8]));
+    let w = weights(&s);
+    assert_eq!(w.w["b"], 2.0);
+}
+
+#[test]
+fn the_lower_quartile_ignores_use_from_elsewhere_and_a_spread_estimate_is_unreliable() {
+    // Mostly 10, with a few inflated by outside use: the lower quartile is 10.
+    let mut values = vec![10.0; 12];
+    values.extend([30.0, 35.0, 40.0]);
+    let mut s = run("a", &values);
+    s.extend(run("b", &[10.0; 8]));
+    let w = weights(&s);
+    near(w.w["a"], 1.0, 1e-9);
+    // Median more than twice the lower quartile: no estimate for the model.
+    let mut spread = run("a", &[5.0, 5.0, 12.0, 12.0, 13.0, 13.0, 14.0, 14.0]);
+    spread.extend(run("b", &[10.0; 8]));
+    let w = weights(&spread);
+    assert!(!w.w.contains_key("a"));
+    assert!(w.w.contains_key("b"));
+}
+
+#[test]
+fn only_the_last_twenty_stretches_count() {
+    let mut values = vec![100.0; 30];
+    values.extend([10.0; 20]);
+    near(weights(&run("a", &values)).capacity.unwrap(), 10.0, 1e-9);
+}
+
+#[test]
+fn a_capacity_that_moves_more_than_twice_is_a_plan_change() {
+    assert!(capacity_changed(10.0, 25.0));
+    assert!(capacity_changed(10.0, 4.0));
+    assert!(!capacity_changed(10.0, 19.0));
+    assert!(!capacity_changed(10.0, 6.0));
+    assert!(!capacity_changed(0.0, 6.0));
+}
```

```diff
--- a/engine/tests/ledger.rs
+++ b/engine/tests/ledger.rs
@@ -0,0 +1,223 @@
+use faber_engine::budget::{Billing, Reach};
+use faber_engine::learn::Span;
+use faber_engine::ledger::{Ledger, PLAN_FRESH, RELEARN, VendorLedger};
+
+const HOUR: f64 = 3600.0;
+const BASE: f64 = 345_600.0 + 2.0 * 86_400.0;
+const RESET: f64 = BASE + 30.0 * 86_400.0;
+
+fn near(got: f64, want: f64) {
+    assert!((got - want).abs() < 1e-9, "{got} is not {want}");
+}
+
+#[test]
+fn readings_that_make_no_sense_are_dropped() {
+    let mut l = VendorLedger::default();
+    for used in [f64::NAN, -0.1, 7.0] {
+        l.record("five_hour", used, RESET, BASE, 0);
+    }
+    l.record("five_hour", 0.2, f64::INFINITY, BASE, 0);
+    assert!(l.readings.is_empty() && l.history.is_empty());
+    l.record("five_hour", 0.2, RESET, BASE, 0);
+    assert_eq!(l.readings["five_hour"].used, 0.2);
+}
+
+#[test]
+fn the_history_is_bounded() {
+    let mut l = VendorLedger::default();
+    for i in 0..1000 {
+        l.record("five_hour", 0.2, RESET, BASE + i as f64 * 60.0, 0);
+    }
+    assert_eq!(l.history["five_hour"].len(), 200);
+    for i in 0..1000 {
+        l.span(Span { start: i as f64, end: i as f64 + 1.0, model: "a".into(), cost: None });
+    }
+    assert_eq!(l.spans.len(), 200);
+}
+
+#[test]
+fn the_shortest_vendor_wide_window_is_the_one_learned_from() {
+    let mut l = VendorLedger::default();
+    l.record("seven_day", 0.4, RESET, BASE, 0);
+    assert_eq!(l.primary(), Some("seven_day"));
+    l.record("five_hour", 0.1, RESET, BASE, 0);
+    l.record("seven_day_opus", 0.1, RESET, BASE, 0);
+    assert_eq!(l.primary(), Some("five_hour"));
+    l.record("5_minutes", 0.1, RESET, BASE, 0);
+    assert_eq!(l.primary(), Some("5_minutes"));
+}
+
+#[test]
+fn the_snapshot_carries_windows_with_their_length() {
+    let mut l = VendorLedger::default();
+    l.record("five_hour", 0.3, RESET, BASE, 0);
+    l.record("seven_day_opus", 0.9, RESET, BASE, 0);
+    l.billing = Billing::Subscription;
+    l.reach = Reach::Out(Some(RESET));
+    let snap = l.snap(BASE + 60.0);
+    assert_eq!(snap.windows.len(), 2);
+    assert_eq!(snap.windows.iter().find(|(n, _)| n == "five_hour").unwrap().1.length, 18_000.0);
+    assert_eq!(snap.billing, Billing::Subscription);
+    assert_eq!(snap.reach, Reach::Out(Some(RESET)));
+}
+
+#[test]
+fn a_discount_needs_a_fresh_plan_or_a_typed_one() {
+    let mut l = VendorLedger::default();
+    assert!(!l.plan_ok(BASE));
+    l.plan_detected(Some("max".into()), BASE);
+    assert!(l.plan_ok(BASE + PLAN_FRESH - 1.0));
+    assert!(!l.plan_ok(BASE + PLAN_FRESH + 1.0));
+    assert!(!l.plan_ok(BASE - 10.0), "a clock that went back");
+    l.typed_plan = Some("pro".into());
+    assert!(l.plan_ok(BASE + 10.0 * HOUR));
+}
+
+#[test]
+fn a_failed_check_does_not_refresh_and_three_in_a_row_ask_the_user() {
+    let mut l = VendorLedger::default();
+    l.plan_failed();
+    l.plan_failed();
+    assert!(!l.ask_for_plan(), "two failures ask nothing");
+    assert!(!l.plan_ok(BASE));
+    l.plan_failed();
+    assert!(l.ask_for_plan());
+    l.typed_plan = Some("pro".into());
+    assert!(!l.ask_for_plan(), "already answered");
+    let mut named_nothing = VendorLedger::default();
+    named_nothing.plan_missing();
+    assert!(named_nothing.ask_for_plan(), "the command worked but the field is missing");
+    named_nothing.plan_detected(Some("max".into()), BASE);
+    assert!(!named_nothing.ask_for_plan());
+}
+
+#[test]
+fn a_different_plan_name_restarts_the_learning_and_the_same_one_does_not() {
+    let mut l = VendorLedger::default();
+    l.plan_detected(Some("pro".into()), BASE);
+    l.weights.capacity = Some(10.0);
+    l.plan_detected(Some("pro".into()), BASE + HOUR);
+    assert_eq!(l.weights.capacity, Some(10.0));
+    assert!(!l.relearning(BASE + HOUR));
+    l.plan_detected(Some("max".into()), BASE + 2.0 * HOUR);
+    assert_eq!(l.weights.capacity, None);
+    assert!(l.relearning(BASE + 3.0 * HOUR));
+    assert!(!l.relearning(BASE + 2.0 * HOUR + RELEARN + 1.0), "48 hours is enough");
+    assert_eq!(l.since, BASE + 2.0 * HOUR);
+}
+
+#[test]
+fn a_typed_plan_does_not_survive_a_detection_that_disagrees() {
+    let mut l = VendorLedger { typed_plan: Some("pro".into()), ..VendorLedger::default() };
+    l.plan_detected(Some("max".into()), BASE);
+    assert_eq!(l.typed_plan, None);
+}
+
+/// Three hours of readings with one clean stretch in the middle: quiet, quiet, then `points` more with a turn of cost `cost`.
+fn block(l: &mut VendorLedger, k: usize, points: f64, cost: f64) {
+    let t = BASE + 3.0 * HOUR * k as f64;
+    let used = 0.05 + points * k as f64 / 100.0;
+    l.record("five_hour", used, RESET, t, 0);
+    l.record("five_hour", used, RESET, t + HOUR, 0);
+    l.span(Span { start: t + 1.2 * HOUR, end: t + 1.5 * HOUR, model: "a".into(), cost: Some(cost) });
+    l.record("five_hour", used + points / 100.0, RESET, t + 2.0 * HOUR, 0);
+}
+
+#[test]
+fn a_capacity_that_jumps_is_a_plan_change_the_name_did_not_announce() {
+    let mut l = VendorLedger::default();
+    for k in 0..8 {
+        block(&mut l, k, 3.0, 0.5);
+    }
+    let now = BASE + 30.0 * HOUR;
+    assert!(!l.learn_weights(now));
+    near(l.weights.capacity.unwrap(), 6.0);
+    for k in 8..28 {
+        block(&mut l, k, 3.0, 0.125);
+    }
+    let later = BASE + 100.0 * HOUR;
+    assert!(l.learn_weights(later), "24 points per cost unit against 6");
+    assert_eq!(l.weights.capacity, None);
+    assert_eq!(l.since, later);
+    assert!(l.relearning(later + HOUR));
+}
+
+#[test]
+fn outside_use_shows_in_the_snapshot_as_a_jump() {
+    let mut l = VendorLedger::default();
+    let now = BASE + 10.0 * HOUR;
+    l.record("five_hour", 0.10, RESET, now - 3.0 * HOUR, 0);
+    l.record("five_hour", 0.16, RESET, now - HOUR, 0);
+    assert!(l.snap(now).jumped);
+    assert!(l.outside(now).outside > 0.0);
+}
+
+#[test]
+fn the_week_is_remembered_but_the_moment_is_not() {
+    let mut ledger = Ledger::default();
+    ledger.claude.record("five_hour", 0.3, RESET, BASE, 0);
+    ledger.claude.plan_detected(Some("max".into()), BASE);
+    ledger.claude.reach = Reach::Out(None);
+    ledger.claude.overage_active = true;
+    ledger.claude.billing = Billing::UsageBilled;
+    let json = serde_json::to_string(&ledger).unwrap();
+    let back: Ledger = serde_json::from_str(&json).unwrap();
+    assert_eq!(back.claude.readings["five_hour"].used, 0.3);
+    assert_eq!(back.claude.plan.as_deref(), Some("max"));
+    assert_eq!(back.claude.reach, Reach::Clear);
+    assert!(!back.claude.overage_active);
+    assert_eq!(back.claude.billing, Billing::Unknown);
+    assert!(back.claude.plan_seen_at.is_none(), "a plan seen last week proves nothing now");
+    assert!(json.len() < 20_000, "{}", json.len());
+}
+
+#[test]
+fn a_file_that_is_not_a_ledger_is_no_ledger() {
+    assert!(serde_json::from_str::<Ledger>("not json").is_err());
+    // A file from a version that kept fewer fields still opens: every field has a default.
+    let older: Ledger = serde_json::from_str("{}").unwrap();
+    assert!(older.claude.readings.is_empty());
+}
+
+#[test]
+fn a_full_ledger_stays_small() {
+    let mut ledger = Ledger::default();
+    for claude in [true, false] {
+        let v = if claude { &mut ledger.claude } else { &mut ledger.codex };
+        for window in ["five_hour", "seven_day", "seven_day_opus"] {
+            for i in 0..500 {
+                v.record(window, (i % 100) as f64 / 100.0, 1_791_627_000.5 + i as f64, 1_791_000_000.123 + i as f64 * 60.0, -10_800);
+            }
+        }
+        for i in 0..500 {
+            v.span(Span {
+                start: 1_791_000_000.123 + i as f64,
+                end: 1_791_000_030.456 + i as f64,
+                model: "claude-sonnet-5-5".into(),
+                cost: Some(0.123456),
+            });
+        }
+    }
+    let size = serde_json::to_string(&ledger).unwrap().len();
+    assert!(size < 100_000, "{size} bytes");
+}
+
+#[test]
+fn a_ledger_read_from_a_damaged_file_cannot_hurt() {
+    let json = r#"{"claude": {
+        "readings": {"five_hour": {"used": -3.0, "resets_at": 1.0, "read_at": 1.0}, "seven_day": {"used": 0.4, "resets_at": 5.0, "read_at": 1.0}},
+        "history": {"five_hour": [{"at": 1.0, "used": 9.0, "resets_at": 5.0}]},
+        "spans": [{"start": 9.0, "end": 1.0, "model": "a", "cost": 0.1}, {"start": 1.0, "end": 2.0, "model": "a", "cost": -4.0}],
+        "learned": {"mass": [1.0, 2.0], "covered": 5.0},
+        "weights": {"w": {"a": 99.0, "b": 0.01}, "capacity": -1.0},
+        "stretches": [{"model": "a", "ppd": 0.0}]
+    }}"#;
+    let mut ledger: Ledger = serde_json::from_str(json).unwrap();
+    ledger.sanitize();
+    let v = &ledger.claude;
+    assert_eq!(v.readings.keys().collect::<Vec<_>>(), ["seven_day"]);
+    assert!(v.history.is_empty() && v.spans.is_empty() && v.stretches.is_empty());
+    assert_eq!(v.learned.mass.len(), 168, "a shape of the wrong length is reset");
+    assert_eq!((v.weights.w["a"], v.weights.w["b"], v.weights.capacity), (2.0, 0.5, None));
+    v.shape();
+}
```

```diff
--- a/engine/tests/signals.rs
+++ b/engine/tests/signals.rs
@@ -0,0 +1,61 @@
+use faber_engine::budget::{Billing, Reach};
+use faber_engine::signals::{Signals, claude_plan, claude_signals, codex_account, codex_signals};
+use serde_json::json;
+
+#[test]
+fn a_claude_event_with_room_clears_and_says_extra_usage_is_off() {
+    // The shape of tests/fixtures/claude/chat.jsonl.
+    let msg = json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "resetsAt": 1791627000, "rateLimitType": "five_hour",
+        "overageStatus": "rejected", "overageDisabledReason": "org_level_disabled", "isUsingOverage": false}});
+    assert_eq!(
+        claude_signals(&msg),
+        Signals { reach: Some(Reach::Clear), overage_active: Some(false), paid_fallback: Some(false), ..Signals::default() }
+    );
+}
+
+#[test]
+fn a_claude_event_that_blocks_a_vendor_wide_window_is_out_until_its_reset() {
+    let msg = json!({"rate_limit_info": {"status": "rejected", "resetsAt": 1791878400, "rateLimitType": "seven_day", "overageStatus": "allowed", "isUsingOverage": true}});
+    let s = claude_signals(&msg);
+    assert_eq!(s.reach, Some(Reach::Out(Some(1791878400.0))));
+    assert_eq!((s.overage_active, s.paid_fallback), (Some(true), Some(true)));
+}
+
+#[test]
+fn a_family_window_status_does_not_block_the_vendor() {
+    let msg = json!({"rate_limit_info": {"status": "rejected", "resetsAt": 1, "rateLimitType": "seven_day_opus"}});
+    assert_eq!(claude_signals(&msg).reach, None);
+}
+
+#[test]
+fn a_codex_rate_limit_update_gives_plan_credits_and_reach() {
+    // The shape of tests/fixtures/codex/chat.jsonl.
+    let msg = json!({"method": "account/rateLimits/updated", "params": {"rateLimits": {"primary": {"usedPercent": 47, "windowDurationMins": 10080, "resetsAt": 1791967370},
+        "secondary": null, "credits": {"hasCredits": false, "unlimited": false, "balance": "0"}, "spendControlReached": null, "planType": "pro", "rateLimitReachedType": null}}});
+    assert_eq!(
+        codex_signals(&msg),
+        Signals { reach: Some(Reach::Clear), paid_fallback: Some(false), plan: Some("pro".into()), ..Signals::default() }
+    );
+    let out = json!({"params": {"rateLimits": {"primary": {"resetsAt": 100}, "secondary": {"resetsAt": 900}, "rateLimitReachedType": "primary", "credits": {"hasCredits": true}}}});
+    let s = codex_signals(&out);
+    assert_eq!(s.reach, Some(Reach::Out(Some(900.0))));
+    assert_eq!(s.paid_fallback, Some(true));
+}
+
+#[test]
+fn the_codex_sign_in_mode_decides_the_billing() {
+    let chatgpt = json!({"method": "account/updated", "params": {"authMode": "chatgpt", "planType": "pro"}});
+    assert_eq!(codex_account(&chatgpt), Signals { plan: Some("pro".into()), billing: Some(Billing::Subscription), ..Signals::default() });
+    let key = json!({"params": {"authMode": "apikey"}});
+    assert_eq!(codex_account(&key).billing, Some(Billing::UsageBilled));
+}
+
+#[test]
+fn claude_auth_status_gives_the_plan_and_drops_the_rest() {
+    let s = claude_plan(r#"{"loggedIn": true, "email": "someone@example.com", "subscriptionType": "max"}"#);
+    assert_eq!(s, Signals { plan: Some("max".into()), billing: Some(Billing::Subscription), ..Signals::default() });
+    let key = claude_plan(r#"{"loggedIn": true, "authMethod": "api_key"}"#);
+    assert_eq!((key.plan, key.billing), (None, Some(Billing::UsageBilled)));
+    assert_eq!(claude_plan("not json"), Signals::default());
+    assert_eq!(claude_plan(r#"{"loggedIn": true}"#), Signals::default(), "the field is missing: ask the user");
+}
```

- [ ] **Step 2: Run the new tests and see them fail**

Run: `cargo test -p faber-engine` (the named test files)
Expected: compile errors or failures naming the missing items.

- [ ] **Step 3: Implement**

`engine/src/learn.rs`

```diff
--- a/engine/src/learn.rs
+++ b/engine/src/learn.rs
@@ -0,0 +1,256 @@
+//! What Faber learns about an account from the readings it takes: when the week's use happens, how much of it came from
+//! outside Faber, and how much quota each model really takes. Pure functions over plain records; the engine feeds them.
+
+use crate::budget::{Shape, bin};
+use serde::{Deserialize, Serialize};
+use std::collections::BTreeMap;
+
+/// Two readings of one window further apart than this say nothing about when the use happened.
+pub const GAP_MAX: f64 = 12.0 * 3600.0;
+/// Outside movement under this many points is ignored: one point is the resolution of a reading.
+pub const OUTSIDE_MIN: f64 = 2.0;
+/// A stretch must have moved the window this many points to count toward a weight.
+pub const STRETCH_MIN: f64 = 3.0;
+/// Stretches kept per model.
+pub const KEEP: usize = 20;
+/// Stretches a model needs before its weight is anything but 1.
+pub const ENOUGH: usize = 8;
+/// A vendor's capacity moving by more than this factor is a plan change.
+pub const CAPACITY_JUMP: f64 = 2.0;
+const WEEK: f64 = 7.0 * 86_400.0;
+const COVER_FULL: f64 = 336.0 * 3600.0;
+
+/// One reading of one window.
+#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
+pub struct Point {
+    /// Unix seconds.
+    pub at: f64,
+    /// 0 to 1.
+    pub used: f64,
+    pub resets_at: f64,
+}
+
+/// A span in which Faber itself was using the vendor: a user turn, a router question, a pulse or a summary.
+#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
+pub struct Span {
+    pub start: f64,
+    pub end: f64,
+    pub model: String,
+    /// What the span cost in the vendor's own unit (US dollars for Claude, credits for Codex); None when not known.
+    pub cost: Option<f64>,
+}
+
+/// Movement in points between two readings of one window, when they say something: the same reset, a gap of at most 12
+/// hours, and use that did not go down. Rounded to a thousandth of a point: readings are whole points held as fractions,
+/// and 0.12 - 0.10 is a hair under 2 points in floating point.
+fn movement(a: &Point, b: &Point) -> Option<f64> {
+    let dt = b.at - a.at;
+    (a.resets_at == b.resets_at && dt > 0.0 && dt <= GAP_MAX && b.used >= a.used).then(|| ((b.used - a.used) * 100_000.0).round() / 1000.0)
+}
+
+fn overlaps(span: &Span, a: f64, b: f64) -> bool {
+    span.start < b && span.end > a
+}
+
+/// The usage profile as it is learned: where in the week the movement of a window happened.
+#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
+pub struct Learned {
+    /// Points moved in each weekday-hour bin.
+    pub mass: Vec<f64>,
+    /// Seconds of accepted gaps.
+    pub covered: f64,
+}
+
+impl Default for Learned {
+    fn default() -> Self {
+        Learned { mass: vec![0.0; 168], covered: 0.0 }
+    }
+}
+
+impl Learned {
+    /// Adds the movement between two consecutive readings of one window, spread over the hours the gap overlaps in
+    /// proportion to the seconds in each; it is never put on one hour.
+    pub fn add(&mut self, a: &Point, b: &Point, offset: i64) {
+        let Some(d) = movement(a, b) else { return };
+        let dt = b.at - a.at;
+        let mut t = a.at;
+        while t < b.at {
+            let hour_end = ((t + offset as f64) / 3600.0).floor() + 1.0;
+            let end = (hour_end * 3600.0 - offset as f64).max(t + 1.0).min(b.at);
+            self.mass[bin(t, offset)] += d * (end - t) / dt;
+            t = end;
+        }
+        self.covered += dt;
+    }
+
+    /// The shape in use: uniform until there is data, then more and more of what was learned, all of it after two weeks.
+    /// Every bin starts with a prior of one average hour, so an hour never seen is not read as an hour of no use.
+    pub fn shape(&self) -> Shape {
+        let total: f64 = self.mass.iter().sum();
+        if self.covered <= 0.0 || total <= 0.0 {
+            return Shape::uniform();
+        }
+        let average_hour = total / (self.covered / 3600.0);
+        let sum = total + average_hour * 168.0;
+        let c = (self.covered / COVER_FULL).min(1.0);
+        let mut shape = [0.0; 168];
+        for (i, slot) in shape.iter_mut().enumerate() {
+            *slot = (1.0 - c) / 168.0 + c * (self.mass[i] + average_hour) / sum;
+        }
+        Shape(shape)
+    }
+}
+
+/// What the readings say about use from outside Faber over the last seven days.
+#[derive(Debug, Clone, Copy, Default, PartialEq)]
+pub struct Outside {
+    /// Points moved in gaps with no Faber turn in them (gaps under two points ignored): a lower bound.
+    pub outside: f64,
+    /// Points moved in all the gaps that said something.
+    pub observed: f64,
+    /// Hours of the gaps with no Faber turn in them.
+    pub quiet_hours: f64,
+    /// Outside movement in the last two hours.
+    pub recent: f64,
+    /// The average outside rate before those two hours, in points per hour.
+    pub rate: f64,
+}
+
+impl Outside {
+    /// The share of the observed movement that came from outside, for the screen ("at least"). None with nothing observed.
+    pub fn share(&self) -> Option<f64> {
+        (self.observed > 0.0).then(|| (self.outside / self.observed).min(1.0))
+    }
+
+    /// Use from outside just jumped: more than 3 points in the last two hours and more than 3 times the average rate;
+    /// with no history any 3 points count.
+    pub fn jumped(&self) -> bool {
+        self.recent > 3.0 && (self.rate <= 0.0 || self.recent / 2.0 > 3.0 * self.rate)
+    }
+}
+
+pub fn outside(points: &[Point], spans: &[Span], now: f64) -> Outside {
+    let mut o = Outside::default();
+    let (mut quiet_before, mut outside_before) = (0.0, 0.0);
+    for pair in points.windows(2) {
+        let (a, b) = (&pair[0], &pair[1]);
+        if b.at < now - WEEK || b.at > now {
+            continue;
+        }
+        let Some(d) = movement(a, b) else { continue };
+        o.observed += d;
+        if spans.iter().any(|s| overlaps(s, a.at, b.at)) {
+            continue;
+        }
+        let hours = (b.at - a.at) / 3600.0;
+        o.quiet_hours += hours;
+        if d < OUTSIDE_MIN {
+            if b.at <= now - 7200.0 {
+                quiet_before += hours;
+            }
+            continue;
+        }
+        o.outside += d;
+        if b.at > now - 7200.0 {
+            o.recent += d;
+        } else {
+            outside_before += d;
+            quiet_before += hours;
+        }
+    }
+    o.rate = if quiet_before > 0.0 { outside_before / quiet_before } else { 0.0 };
+    o
+}
+
+/// One stretch of Faber's own use that moved a window enough to measure: points of the window per unit of cost.
+#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
+pub struct Stretch {
+    pub model: String,
+    pub ppd: f64,
+}
+
+/// Stretches in a window's readings. A stretch is a pair of consecutive readings with, between them, Faber turns of one
+/// model that do not overlap each other, with known costs, the window moving at least 3 points, and no outside movement
+/// of 2 points or more in the gaps on either side (so use from elsewhere cannot hide in it).
+pub fn stretches(points: &[Point], spans: &[Span]) -> Vec<Stretch> {
+    let outside_moved =
+        |a: &Point, b: &Point| movement(a, b).is_some_and(|d| d >= OUTSIDE_MIN) && !spans.iter().any(|s| overlaps(s, a.at, b.at));
+    let mut found = vec![];
+    for i in 0..points.len().saturating_sub(1) {
+        let (a, b) = (&points[i], &points[i + 1]);
+        let Some(d) = movement(a, b) else { continue };
+        if d < STRETCH_MIN {
+            continue;
+        }
+        let inside: Vec<&Span> = spans.iter().filter(|s| overlaps(s, a.at, b.at)).collect();
+        let Some(first) = inside.first() else { continue };
+        let one_model = inside.iter().all(|s| s.model == first.model);
+        let apart = inside.iter().enumerate().all(|(j, s)| inside.iter().skip(j + 1).all(|t| s.end <= t.start || t.end <= s.start));
+        let cost: Option<f64> = inside.iter().map(|s| s.cost).sum();
+        let before = i > 0 && outside_moved(&points[i - 1], a);
+        let after = i + 2 < points.len() && outside_moved(b, &points[i + 2]);
+        if let (true, true, Some(cost), false, false) = (one_model, apart, cost, before, after)
+            && cost > 0.0
+        {
+            found.push(Stretch { model: first.model.clone(), ppd: d / cost });
+        }
+    }
+    found
+}
+
+/// What the stretches say about a vendor's models.
+#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
+pub struct Weights {
+    /// The weight of each model in its vendor's quota: raw points per cost over the vendor's median, 0.5 to 2. A model
+    /// with too few stretches, or an unreliable estimate, has no entry and counts as 1.
+    pub w: BTreeMap<String, f64>,
+    /// The median raw points per cost over the models: the size of the plan in one number. Kept before normalising, so a
+    /// plan four times larger shows.
+    pub capacity: Option<f64>,
+}
+
+fn sorted(mut v: Vec<f64>) -> Vec<f64> {
+    v.sort_by(f64::total_cmp);
+    v
+}
+
+fn quartile(v: &[f64]) -> f64 {
+    v[(v.len() - 1) / 4]
+}
+
+fn median(v: &[f64]) -> f64 {
+    let n = v.len();
+    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
+}
+
+/// `stretches` are oldest first. Outside use can only add points, so the lower quartile of the last 20 is kept, not the
+/// median; when the median is more than twice the lower quartile the estimate is unreliable and the model keeps 1.
+pub fn weights(stretches: &[Stretch]) -> Weights {
+    let mut per_model: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
+    for s in stretches {
+        per_model.entry(&s.model).or_default().push(s.ppd);
+    }
+    let mut raw: BTreeMap<String, f64> = BTreeMap::new();
+    for (model, values) in per_model {
+        let last = &values[values.len().saturating_sub(KEEP)..];
+        if last.len() < ENOUGH {
+            continue;
+        }
+        let v = sorted(last.to_vec());
+        let low = quartile(&v);
+        if low > 0.0 && median(&v) <= 2.0 * low {
+            raw.insert(model.to_string(), low);
+        }
+    }
+    if raw.is_empty() {
+        return Weights::default();
+    }
+    let capacity = median(&sorted(raw.values().copied().collect()));
+    let w = raw.into_iter().map(|(m, r)| (m, (r / capacity).clamp(0.5, 2.0))).collect();
+    Weights { w, capacity: Some(capacity) }
+}
+
+/// Whether a vendor's capacity moved enough to be a different plan.
+pub fn capacity_changed(old: f64, new: f64) -> bool {
+    old > 0.0 && new > 0.0 && (new / old > CAPACITY_JUMP || old / new > CAPACITY_JUMP)
+}
```

`engine/src/ledger.rs`

```diff
--- a/engine/src/ledger.rs
+++ b/engine/src/ledger.rs
@@ -0,0 +1,281 @@
+//! What Faber remembers about each vendor's quota: the latest reading of every window, the readings over the last week, the
+//! turns Faber itself ran, and what was learned from them. Plain data with explicit times, so it is testable without a clock;
+//! the engine keeps one and writes it to `budget.json`.
+
+use crate::budget::{Billing, Reach, Shape, Snap, Window, vendor_wide, window_secs};
+use crate::learn::{Learned, Outside, Point, Span, Stretch, Weights, capacity_changed, outside, stretches, weights};
+use crate::model::Vendor;
+use serde::{Deserialize, Serialize};
+use std::collections::BTreeMap;
+
+/// Readings kept per window, and spans kept per vendor: bounds the file at a few tens of kilobytes.
+const KEEP_POINTS: usize = 200;
+const KEEP_SPANS: usize = 200;
+const KEEP_STRETCHES: usize = 60;
+/// A plan check younger than this lets a discount stand.
+pub const PLAN_FRESH: f64 = 30.0 * 60.0;
+/// After a plan change, discounts wait for this long or for enough stretches.
+pub const RELEARN: f64 = 48.0 * 3600.0;
+/// Failed checks in a row before the user is asked which plan they have.
+pub const ASK_AFTER: u32 = 3;
+
+/// Whether a reading can be real: a share between 0 and a little over 1, and a reset time that is a number.
+pub fn plausible(used: f64, resets_at: f64) -> bool {
+    (0.0..=1.5).contains(&used) && resets_at.is_finite() && resets_at > 0.0
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
+pub struct Reading {
+    pub used: f64,
+    pub resets_at: f64,
+    pub read_at: f64,
+}
+
+/// What is kept for one vendor.
+#[derive(Debug, Clone, Default, Serialize, Deserialize)]
+#[serde(default)]
+pub struct VendorLedger {
+    pub readings: BTreeMap<String, Reading>,
+    pub history: BTreeMap<String, Vec<Point>>,
+    pub spans: Vec<Span>,
+    pub learned: Learned,
+    pub weights: Weights,
+    pub stretches: Vec<Stretch>,
+    /// The plan as last detected, and the one the user typed when detection failed.
+    pub plan: Option<String>,
+    pub typed_plan: Option<String>,
+    /// Readings and spans before this time are ignored for outside use and weights (a plan change starts them again).
+    pub since: f64,
+    pub relearn_until: f64,
+    pub stretches_since: usize,
+    // What follows is about the moment, not the week: never written to disk.
+    #[serde(skip)]
+    pub billing: Billing,
+    #[serde(skip)]
+    pub reach: Reach,
+    #[serde(skip)]
+    pub overage_active: bool,
+    #[serde(skip)]
+    pub paid_fallback: bool,
+    /// When a plan check last succeeded, and how many failed in a row since.
+    #[serde(skip)]
+    pub plan_seen_at: Option<f64>,
+    #[serde(skip)]
+    pub failed_checks: u32,
+    /// A check worked but named no plan.
+    #[serde(skip)]
+    pub nameless: bool,
+}
+
+#[derive(Debug, Clone, Default, Serialize, Deserialize)]
+#[serde(default)]
+pub struct Ledger {
+    pub claude: VendorLedger,
+    pub codex: VendorLedger,
+}
+
+impl Ledger {
+    /// Makes a ledger read from disk safe to use: a damaged or hand-edited file must neither crash anything nor make a number
+    /// up. Whatever does not make sense is dropped, which only means that part is learned again.
+    pub fn sanitize(&mut self) {
+        self.claude.sanitize();
+        self.codex.sanitize();
+    }
+
+    pub fn of(&self, vendor: Vendor) -> &VendorLedger {
+        match vendor {
+            Vendor::Claude => &self.claude,
+            Vendor::Codex => &self.codex,
+        }
+    }
+
+    pub fn of_mut(&mut self, vendor: Vendor) -> &mut VendorLedger {
+        match vendor {
+            Vendor::Claude => &mut self.claude,
+            Vendor::Codex => &mut self.codex,
+        }
+    }
+}
+
+impl VendorLedger {
+    fn sanitize(&mut self) {
+        self.readings.retain(|_, r| plausible(r.used, r.resets_at) && r.read_at.is_finite());
+        for list in self.history.values_mut() {
+            list.retain(|p| plausible(p.used, p.resets_at) && p.at.is_finite());
+            list.drain(..list.len().saturating_sub(KEEP_POINTS));
+        }
+        self.history.retain(|_, list| !list.is_empty());
+        self.spans.retain(|s| s.start.is_finite() && s.end.is_finite() && s.end >= s.start && s.cost.is_none_or(|c| c.is_finite() && c >= 0.0));
+        self.spans.drain(..self.spans.len().saturating_sub(KEEP_SPANS));
+        self.stretches.retain(|s| s.ppd.is_finite() && s.ppd > 0.0);
+        self.stretches.drain(..self.stretches.len().saturating_sub(KEEP_STRETCHES));
+        // The shape indexes 168 hours of the week; any other length would index out of range.
+        if self.learned.mass.len() != 168 || self.learned.mass.iter().any(|m| !m.is_finite() || *m < 0.0) || !self.learned.covered.is_finite() {
+            self.learned = Learned::default();
+        }
+        self.weights.w.retain(|_, w| w.is_finite());
+        self.weights.w.values_mut().for_each(|w| *w = w.clamp(0.5, 2.0));
+        self.weights.capacity = self.weights.capacity.filter(|c| c.is_finite() && *c > 0.0);
+        for t in [&mut self.since, &mut self.relearn_until] {
+            if !t.is_finite() {
+                *t = 0.0;
+            }
+        }
+    }
+
+    /// The window the week's shape and the weights are learned from: the shortest vendor-wide one, which moves the most
+    /// points per unit of use. Only its readings are kept as a history.
+    pub fn primary(&self) -> Option<&str> {
+        self.readings
+            .keys()
+            .filter(|n| vendor_wide(n))
+            .filter_map(|n| Some((window_secs(n)?, n.as_str())))
+            .min_by(|a, b| a.0.total_cmp(&b.0))
+            .map(|(_, n)| n)
+    }
+
+    /// A reading of a window. Readings that make no sense (not a number, below 0 or far above 1) are dropped.
+    pub fn record(&mut self, window: &str, used: f64, resets_at: f64, now: f64, offset: i64) {
+        if !plausible(used, resets_at) {
+            return;
+        }
+        // Whole seconds and hundredths are all a reading holds; the file stays small.
+        let (now, resets_at, used) = (now.round(), resets_at.round(), (used * 1000.0).round() / 1000.0);
+        self.readings.insert(window.to_string(), Reading { used, resets_at, read_at: now });
+        let point = Point { at: now, used, resets_at };
+        let primary = self.primary().map(String::from);
+        self.history.retain(|name, _| Some(name) == primary.as_ref());
+        if primary.as_deref() != Some(window) {
+            return;
+        }
+        let list = self.history.entry(window.to_string()).or_default();
+        let previous = list.last().copied();
+        list.push(point);
+        if list.len() > KEEP_POINTS {
+            list.drain(..list.len() - KEEP_POINTS);
+        }
+        if let Some(previous) = previous
+            && previous.at >= self.since
+        {
+            self.learned.add(&previous, &point, offset);
+        }
+    }
+
+    /// A turn Faber ran, for outside use and weights.
+    pub fn span(&mut self, span: Span) {
+        let span = Span { start: span.start.round(), end: span.end.round(), cost: span.cost.map(|c| (c * 1e6).round() / 1e6), ..span };
+        self.spans.push(span);
+        if self.spans.len() > KEEP_SPANS {
+            self.spans.drain(..self.spans.len() - KEEP_SPANS);
+        }
+    }
+
+    fn recent(&self, window: &str) -> Vec<Point> {
+        self.history.get(window).map(|h| h.iter().filter(|p| p.at >= self.since).copied().collect()).unwrap_or_default()
+    }
+
+    fn recent_spans(&self) -> Vec<Span> {
+        self.spans.iter().filter(|s| s.end >= self.since).cloned().collect()
+    }
+
+    /// Use from outside Faber over the last week, from the primary window.
+    pub fn outside(&self, now: f64) -> Outside {
+        self.primary().map(|w| outside(&self.recent(w), &self.recent_spans(), now)).unwrap_or_default()
+    }
+
+    /// Learns the weights again from the stretches in the history. Returns true when the vendor's capacity moved by more
+    /// than a factor of two, which is a plan change the name did not announce.
+    pub fn learn_weights(&mut self, now: f64) -> bool {
+        let Some(window) = self.primary().map(String::from) else { return false };
+        let found = stretches(&self.recent(&window), &self.recent_spans());
+        self.stretches = found.iter().rev().take(KEEP_STRETCHES).rev().cloned().collect();
+        self.stretches_since = found.len();
+        let fresh = weights(&found);
+        let changed = matches!((self.weights.capacity, fresh.capacity), (Some(old), Some(new)) if capacity_changed(old, new));
+        if changed {
+            self.restart(now);
+        } else {
+            self.weights = fresh;
+        }
+        changed
+    }
+
+    /// A different plan: what was learned about weights, capacity and outside use is cleared, and discounts wait.
+    fn restart(&mut self, now: f64) {
+        self.weights = Weights::default();
+        self.stretches.clear();
+        self.stretches_since = 0;
+        self.since = now;
+        self.relearn_until = now + RELEARN;
+    }
+
+    /// A plan name, from a successful check. A change from one name to another restarts the learning.
+    pub fn plan_detected(&mut self, plan: Option<String>, now: f64) {
+        self.plan_seen_at = Some(now);
+        self.failed_checks = 0;
+        self.nameless = self.nameless && plan.is_none();
+        if let (Some(old), Some(new)) = (&self.plan, &plan)
+            && old != new
+        {
+            self.restart(now);
+        }
+        if plan.is_some() {
+            self.plan = plan;
+        }
+        // A typed plan never outlives a detection that disagrees with it.
+        if self.plan.is_some() && self.typed_plan != self.plan {
+            self.typed_plan = None;
+        }
+    }
+
+    pub fn plan_failed(&mut self) {
+        self.failed_checks += 1;
+    }
+
+    /// A check worked but the answer named no plan.
+    pub fn plan_missing(&mut self) {
+        self.nameless = true;
+        self.failed_checks = 0;
+    }
+
+    /// Whether to ask the user which plan they have: detection failed three times in a row, or worked but named no plan.
+    pub fn ask_for_plan(&self) -> bool {
+        self.plan.is_none() && self.typed_plan.is_none() && (self.failed_checks >= ASK_AFTER || self.nameless)
+    }
+
+    /// A discount needs a plan detected less than half an hour ago, or typed by the user.
+    pub fn plan_ok(&self, now: f64) -> bool {
+        self.typed_plan.is_some() || self.plan_seen_at.is_some_and(|t| (0.0..=PLAN_FRESH).contains(&(now - t)))
+    }
+
+    pub fn relearning(&self, now: f64) -> bool {
+        now < self.relearn_until && self.stretches_since < crate::learn::ENOUGH
+    }
+
+    pub fn shape(&self) -> Shape {
+        self.learned.shape()
+    }
+
+    pub fn snap(&self, now: f64) -> Snap {
+        let windows = self
+            .readings
+            .iter()
+            .map(|(name, r)| {
+                (
+                    name.clone(),
+                    Window { used: r.used, resets_at: r.resets_at, read_at: r.read_at, length: window_secs(name).unwrap_or(0.0) },
+                )
+            })
+            .collect();
+        Snap {
+            windows,
+            billing: self.billing,
+            reach: self.reach,
+            overage_active: self.overage_active,
+            paid_fallback: self.paid_fallback,
+            jumped: self.outside(now).jumped(),
+            relearning: self.relearning(now),
+            plan_ok: self.plan_ok(now),
+        }
+    }
+}
```

`engine/src/lib.rs`

```diff
--- a/engine/src/lib.rs
+++ b/engine/src/lib.rs
@@ -9,10 +9,13 @@
 pub mod engine;
 pub mod events;
 pub mod install;
+pub mod learn;
+pub mod ledger;
 pub mod model;
 pub mod proc;
 pub mod route;
 pub mod router;
+pub mod signals;
 pub mod store;
 pub mod usage;
 pub mod workspace;
```

`engine/src/signals.rs`

```diff
--- a/engine/src/signals.rs
+++ b/engine/src/signals.rs
@@ -0,0 +1,70 @@
+//! What a CLI says about the account besides window readings, reduced to the few facts the router needs. Everything else in
+//! those messages is dropped unread.
+
+use crate::budget::{Billing, Reach, vendor_wide};
+use serde_json::Value;
+
+/// What a CLI said about the account besides window readings. Each field is `Some` only when the message says something about
+/// it; the engine merges them into what it knows.
+#[derive(Debug, Clone, Default, PartialEq)]
+pub struct Signals {
+    pub reach: Option<Reach>,
+    pub overage_active: Option<bool>,
+    pub paid_fallback: Option<bool>,
+    pub plan: Option<String>,
+    pub billing: Option<Billing>,
+}
+
+/// Claude Code's `rate_limit_event`. A status other than `allowed` counts only when it is about a vendor-wide window; the
+/// extra-usage switch is "possible" unless the CLI says it is rejected or off.
+pub fn claude_signals(msg: &Value) -> Signals {
+    let info = &msg["rate_limit_info"];
+    let wide = info["rateLimitType"].as_str().is_some_and(vendor_wide);
+    let status = info["status"].as_str().unwrap_or_default();
+    let reach = (wide && !status.is_empty())
+        .then(|| if status.starts_with("allowed") { Reach::Clear } else { Reach::Out(info["resetsAt"].as_f64()) });
+    Signals {
+        reach,
+        overage_active: info["isUsingOverage"].as_bool(),
+        paid_fallback: info["overageStatus"].as_str().map(|s| !matches!(s, "rejected" | "disabled" | "off" | "none")),
+        ..Signals::default()
+    }
+}
+
+/// Codex's `account/rateLimits/updated`: the plan, credits, and whether a limit was reached.
+pub fn codex_signals(msg: &Value) -> Signals {
+    let r = &msg["params"]["rateLimits"];
+    let reset = [&r["primary"]["resetsAt"], &r["secondary"]["resetsAt"]].into_iter().filter_map(Value::as_f64).reduce(f64::max);
+    let reached = !r["rateLimitReachedType"].is_null() || r["spendControlReached"] == true;
+    Signals {
+        reach: Some(if reached { Reach::Out(reset) } else { Reach::Clear }),
+        paid_fallback: Some(r["credits"]["hasCredits"] == true || r["credits"]["unlimited"] == true),
+        plan: r["planType"].as_str().map(String::from),
+        ..Signals::default()
+    }
+}
+
+/// Codex's `account/updated`: how the account signs in and its plan. The mode names are not credentials; nothing else in
+/// the message is kept.
+pub fn codex_account(msg: &Value) -> Signals {
+    let p = &msg["params"];
+    let billing =
+        p["authMode"].as_str().map(|m| if m.to_ascii_lowercase().contains("api") { Billing::UsageBilled } else { Billing::Subscription });
+    Signals { plan: p["planType"].as_str().map(String::from), billing, ..Signals::default() }
+}
+
+/// What `claude auth status` printed, reduced to the plan and the billing: everything else in the output is dropped
+/// unread.
+pub fn claude_plan(output: &str) -> Signals {
+    let v: Value = serde_json::from_str(output).unwrap_or(Value::Null);
+    let plan = v["subscriptionType"].as_str().filter(|s| !s.is_empty()).map(String::from);
+    let method = ["authMethod", "authMode", "loginMethod"].into_iter().find_map(|k| v[k].as_str()).unwrap_or_default().to_ascii_lowercase();
+    let billing = if plan.is_some() {
+        Some(Billing::Subscription)
+    } else if method.contains("api") || method.contains("console") {
+        Some(Billing::UsageBilled)
+    } else {
+        None
+    };
+    Signals { plan, billing, ..Signals::default() }
+}
```

- [ ] **Step 4: Run the engine suite, clippy and fmt**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): signals, learning and the ledger (pure)"
```

### Task 5: The engine keeps the ledger

R89: an authoritative account update with a missing or unrecognised mode clears any earlier subscription verdict to `Billing::Unknown`. A successful status response with no billing evidence also clears it; a plan name typed by the user never confirms billing. Integration tests pin this reset. Partial quota or credit messages retain the earlier guards, missing counters do not become zero, and failed account reads return no replacement evidence.

Quota readings and account signals reach the ledger, are saved at most every 30 s and survive a restart.

**Files:**
- `engine/src/claude.rs`
- `engine/src/codex.rs`
- `engine/src/engine.rs`
- `engine/src/engine/auto.rs`
- `engine/src/events.rs`
- `engine/src/ledger.rs`
- `engine/src/lib.rs`
- `engine/src/signals.rs`
- `engine/src/store.rs`
- `engine/tests/quota.rs`
- `engine/tests/signals.rs`

**Interfaces:**
- Consumes: Task 4. Produces: `EngineEvent::Signals`, `Store::{ledger, save_ledger}`, `Engine::{set_utc_offset, set_time_source, note_account_events, ledger}`, `pub type Clock`, `codex::read_codex_limits(bin, cwd, windows, spawn, timeout) -> Vec<EngineEvent>`.

- [ ] **Step 1: Add the tests (they fail or do not compile yet)**

```diff
--- a/engine/tests/quota.rs
+++ b/engine/tests/quota.rs
@@ -0,0 +1,241 @@
+mod support;
+
+use faber_engine::budget::{Billing, Reach};
+use faber_engine::cli::Cli;
+use faber_engine::codex::read_codex_limits;
+use faber_engine::events::EngineEvent;
+use faber_engine::ledger::Ledger;
+use faber_engine::model::{Area, Message, Role, Status, Vendor};
+use faber_engine::proc::Spawner;
+use faber_engine::router::Choice;
+use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
+use serde_json::json;
+use std::collections::BTreeMap;
+use std::path::Path;
+use std::sync::Arc;
+use std::time::Duration;
+use support::{Entry, fake, fixture};
+use tokio::sync::mpsc::UnboundedReceiver;
+
+const SKY: &str = "In one sentence: why is the sky blue?";
+/// A moment after the recordings were made and before their windows reset (the Claude five-hour window resets at
+/// 1791627000).
+const WHEN: f64 = 1_791_620_000.0;
+
+fn start(dir: &Path, vendor: Vendor, spawn: Spawner) -> (Engine, UnboundedReceiver<AppEvent>) {
+    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
+    let opts = EngineOptions {
+        data_dir: dir.into(),
+        clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
+        spawn,
+        windows: false,
+        max_warm: 3,
+        idle: Duration::from_secs(120),
+        runtime: tokio::runtime::Handle::current(),
+        ask: None,
+    };
+    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
+    engine.set_time_source(Arc::new(|| WHEN));
+    (engine, rx)
+}
+
+fn ask(vendor: &str) -> SendRequest {
+    SendRequest {
+        conversation_id: None,
+        area: Area::Chat,
+        project_id: None,
+        text: SKY.into(),
+        images: vec![],
+        files: vec![],
+        choice: Choice { vendor: vendor.into(), model: (vendor == "claude").then(|| "haiku".into()) },
+    }
+}
+
+async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> Message {
+    loop {
+        let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("no finished answer in time").expect("engine gone");
+        if let AppEvent::Message { message, .. } = e
+            && message.role == Role::Assistant
+            && message.status != Some(Status::Streaming)
+        {
+            return *message;
+        }
+    }
+}
+
+#[tokio::test]
+async fn a_claude_reading_reaches_the_ledger_is_saved_and_is_still_there_after_a_restart() {
+    let dir = tempfile::tempdir().unwrap();
+    let (engine, mut rx) = start(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
+    engine.send(ask("claude")).unwrap();
+    finished(&mut rx).await;
+    let claude = engine.ledger().claude;
+    assert_eq!(claude.readings["five_hour"].used, 0.07);
+    assert_eq!(claude.readings["seven_day"].resets_at, 1_791_878_400.0);
+    assert_eq!(claude.reach, Reach::Clear);
+    assert!(!claude.paid_fallback, "the recording says extra usage is rejected");
+    assert!(!claude.overage_active);
+    engine.shutdown();
+
+    let saved: Ledger = serde_json::from_str(&std::fs::read_to_string(dir.path().join("budget.json")).unwrap()).unwrap();
+    assert_eq!(saved.claude.readings["five_hour"].used, 0.07);
+    assert_eq!(saved.claude.billing, Billing::Unknown, "what is true only now is not written");
+
+    let (again, _rx) = start(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
+    let state = again.state();
+    let windows: Vec<&str> = state.quotas.iter().map(|q| q.window.as_str()).collect();
+    assert_eq!(windows, ["five_hour", "seven_day"], "Settings has numbers before the first answer");
+}
+
+#[tokio::test]
+async fn a_codex_session_tells_the_ledger_the_plan_and_that_it_is_a_subscription() {
+    let dir = tempfile::tempdir().unwrap();
+    let (engine, mut rx) = start(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn);
+    engine.send(ask("codex")).unwrap();
+    finished(&mut rx).await;
+    let codex = engine.ledger().codex;
+    assert_eq!(codex.plan.as_deref(), Some("pro"));
+    assert_eq!(codex.billing, Billing::Subscription);
+    assert_eq!(codex.readings["seven_day"].used, 0.47);
+    assert!(codex.plan_ok(WHEN), "a plan seen a moment ago");
+    assert!(!codex.paid_fallback, "no credits");
+    engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_budget_file_that_is_damaged_old_or_odd_never_stops_the_app() {
+    for (name, content) in [
+        ("garbage", "not json at all"),
+        ("empty", ""),
+        ("from an older version", "{}"),
+        ("wrong types", r#"{"claude": {"readings": 5}}"#),
+        (
+            "nonsense inside",
+            r#"{"claude": {"readings": {"five_hour": {"used": -3.0, "resets_at": 1791627000.0, "read_at": 1.0}},
+                "learned": {"mass": [1.0], "covered": 5.0}, "weights": {"w": {"a": 99.0}, "capacity": null}}}"#,
+        ),
+    ] {
+        let dir = tempfile::tempdir().unwrap();
+        std::fs::write(dir.path().join("budget.json"), content).unwrap();
+        let (engine, mut rx) = start(dir.path(), Vendor::Claude, fake(fixture("claude", "chat")).spawn);
+        assert!(engine.ledger().claude.readings.is_empty(), "{name}: nothing believable to start from");
+        assert!(engine.state().quotas.is_empty(), "{name}");
+        engine.send(ask("claude")).unwrap();
+        assert_eq!(finished(&mut rx).await.status, Some(Status::Done), "{name}");
+        assert_eq!(engine.ledger().claude.readings["five_hour"].used, 0.07, "{name}: the next reading is taken as usual");
+        engine.shutdown();
+    }
+}
+
+#[tokio::test]
+async fn a_reading_that_cannot_be_real_is_neither_shown_nor_remembered() {
+    let dir = tempfile::tempdir().unwrap();
+    let (engine, _rx) = start(dir.path(), Vendor::Claude, fake(vec![]).spawn);
+    let quota = |window: &str, used: f64, resets_at: f64| EngineEvent::Quota { window: window.into(), used, resets_at };
+    engine.note_account_events(
+        Vendor::Claude,
+        vec![
+            quota("five_hour", 7.0, 1_791_627_000.0),
+            quota("five_hour", -0.2, 1_791_627_000.0),
+            quota("seven_day", 0.3, 0.0),
+            quota("weird window", 0.3, 1_791_627_000.0),
+        ],
+    );
+    let ledger = engine.ledger().claude;
+    assert_eq!(ledger.readings.keys().collect::<Vec<_>>(), ["weird window"], "an odd name is kept, and prices as no information");
+    assert_eq!(ledger.snap(WHEN).windows[0].1.length, 0.0);
+    assert_eq!(engine.state().quotas.len(), 1);
+}
+
+#[tokio::test]
+async fn limits_are_read_without_a_turn_and_the_account_is_not_kept() {
+    let entries = vec![
+        Entry {
+            input: true,
+            msg: json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } }),
+        },
+        Entry { input: false, msg: json!({ "id": 1, "result": {} }) },
+        Entry { input: true, msg: json!({ "method": "initialized" }) },
+        Entry { input: true, msg: json!({ "id": 2, "method": "account/rateLimits/read" }) },
+        Entry { input: true, msg: json!({ "id": 3, "method": "account/read", "params": { "refreshToken": false } }) },
+        Entry {
+            input: false,
+            msg: json!({ "id": 3, "result": { "account": { "type": "chatgpt", "email": "someone@example.com", "planType": "plus" }, "requiresOpenaiAuth": true } }),
+        },
+        Entry {
+            input: false,
+            msg: json!({ "id": 2, "result": { "rateLimits": {
+                "primary": { "usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1791627000 },
+                "secondary": { "usedPercent": 30, "windowDurationMins": 10080, "resetsAt": 1791878400 },
+                "credits": { "hasCredits": false, "unlimited": false, "balance": "0" }, "planType": "plus", "rateLimitReachedType": null } } }),
+        },
+    ];
+    let f = fake(entries);
+    let events = read_codex_limits("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_secs(3)).await;
+    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
+    let windows: Vec<(&str, f64)> = events
+        .iter()
+        .filter_map(|e| if let EngineEvent::Quota { window, used, .. } = e { Some((window.as_str(), *used)) } else { None })
+        .collect();
+    assert_eq!(windows, [("five_hour", 0.12), ("seven_day", 0.3)]);
+    let plan = events.iter().find_map(|e| if let EngineEvent::Signals(s) = e { s.billing } else { None });
+    assert_eq!(plan, Some(Billing::Subscription));
+    assert!(!format!("{events:?}").contains("someone@example.com"), "the email is dropped unread");
+
+    let dir = tempfile::tempdir().unwrap();
+    let (engine, _rx) = start(dir.path(), Vendor::Codex, fake(vec![]).spawn);
+    engine.note_account_events(Vendor::Codex, events);
+    let codex = engine.ledger().codex;
+    assert_eq!((codex.plan.as_deref(), codex.billing), (Some("plus"), Billing::Subscription));
+    assert_eq!(codex.readings.len(), 2);
+}
+
+#[tokio::test]
+async fn a_cli_that_never_answers_gives_no_limits() {
+    let f = fake(vec![]);
+    let events = read_codex_limits("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_millis(50)).await;
+    assert!(events.is_empty());
+}
+
+#[tokio::test]
+async fn an_account_update_with_no_billing_clears_an_earlier_subscription_verdict() {
+    let dir = tempfile::tempdir().unwrap();
+    let (engine, _rx) = start(dir.path(), Vendor::Codex, fake(fixture("codex", "chat")).spawn);
+    let mut state = faber_engine::codex::CodexTurnState::default();
+    for mode in [json!("chatgpt"), serde_json::Value::Null] {
+        let events =
+            faber_engine::codex::parse_codex_notification(&json!({"method":"account/updated","params":{"authMode":mode}}), &mut state);
+        engine.note_account_events(Vendor::Codex, events);
+        assert_eq!(engine.ledger().codex.billing, if mode.is_null() { Billing::Unknown } else { Billing::Subscription });
+    }
+}
+
+#[test]
+fn incomplete_window_payloads_do_not_fabricate_an_empty_quota() {
+    let claude = faber_engine::claude::parse_claude_message(
+        &json!({"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"resetsAt":1791627000}}}}),
+    );
+    let codex = faber_engine::codex::parse_codex_notification(
+        &json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"primary":{"windowDurationMins":300,"resetsAt":1791627000}}}}),
+        &mut faber_engine::codex::CodexTurnState::default(),
+    );
+    for events in [claude, codex] {
+        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Quota { .. })));
+    }
+}
+
+#[tokio::test]
+async fn an_account_read_error_returns_no_replacement_evidence() {
+    let entries = vec![
+        Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
+        Entry { input: false, msg: json!({"id":1,"result":{}}) },
+        Entry { input: true, msg: json!({"method":"initialized"}) },
+        Entry { input: true, msg: json!({"id":2,"method":"account/rateLimits/read"}) },
+        Entry { input: true, msg: json!({"id":3,"method":"account/read"}) },
+        Entry { input: false, msg: json!({"id":2,"error":{"code":-1,"message":"unavailable"}}) },
+        Entry { input: false, msg: json!({"id":3,"result":{"account":{"type":"chatgpt","planType":"pro"}}}) },
+    ];
+    let f = fake(entries);
+    let dir = tempfile::tempdir().unwrap();
+    assert!(read_codex_limits("codex", dir.path(), false, f.spawn, Duration::from_secs(1)).await.is_empty());
+}
```

```diff
--- a/engine/tests/signals.rs
+++ b/engine/tests/signals.rs
@@ -48,6 +48,18 @@
     assert_eq!(codex_account(&chatgpt), Signals { plan: Some("pro".into()), billing: Some(Billing::Subscription), ..Signals::default() });
     let key = json!({"params": {"authMode": "apikey"}});
     assert_eq!(codex_account(&key).billing, Some(Billing::UsageBilled));
+    let cloud = json!({"params": {"authMode": "amazonBedrock"}});
+    assert_eq!(codex_account(&cloud).billing, Some(Billing::UsageBilled), "any way in but ChatGPT is billed by use");
+    let metered = json!({"params": {"authMode": "chatgpt", "planType": "self_serve_business_usage_based"}});
+    assert_eq!(codex_account(&metered).billing, Some(Billing::UsageBilled), "a plan billed by use is not a subscription");
+    assert_eq!(codex_account(&json!({"params": {}})).billing, Some(Billing::Unknown), "a missing mode clears the verdict");
+    let limits = json!({"params": {"rateLimits": {"planType": "enterprise_cbp_usage_based"}}});
+    assert_eq!(codex_signals(&limits).billing, Some(Billing::UsageBilled));
+    assert_eq!(
+        codex_signals(&json!({"params": {"rateLimits": {"planType": "pro"}}})).billing,
+        None,
+        "a limit update never proves a subscription"
+    );
 }
 
 #[test]
@@ -57,5 +69,29 @@
     let key = claude_plan(r#"{"loggedIn": true, "authMethod": "api_key"}"#);
     assert_eq!((key.plan, key.billing), (None, Some(Billing::UsageBilled)));
     assert_eq!(claude_plan("not json"), Signals::default());
-    assert_eq!(claude_plan(r#"{"loggedIn": true}"#), Signals::default(), "the field is missing: ask the user");
+    assert_eq!(claude_plan(r#"{"loggedIn": true}"#).billing, Some(Billing::Unknown), "a missing field does not prove a subscription");
 }
+
+#[test]
+fn an_unrecognised_or_signed_out_account_does_not_prove_subscription_billing() {
+    assert_eq!(codex_account(&json!({"params": {"authMode": "chatgpt-unrecognised"}})).billing, Some(Billing::Unknown));
+    assert_eq!(codex_account(&json!({"params": {"authMode": null}})).billing, Some(Billing::Unknown));
+    assert_eq!(claude_plan(r#"{"loggedIn":false,"subscriptionType":"max"}"#).billing, Some(Billing::Unknown));
+    assert_eq!(claude_plan(r#"{"loggedIn":true,"authMethod":"api_key","subscriptionType":"max"}"#).billing, Some(Billing::UsageBilled));
+}
+
+#[test]
+fn absent_credit_and_limit_fields_do_not_clear_previous_guards() {
+    let signals = codex_signals(&json!({"params":{"rateLimits":{"primary":{"usedPercent":50}}}}));
+    assert_eq!(signals.paid_fallback, None);
+    assert_eq!(signals.reach, None);
+    let partial = codex_signals(&json!({"params":{"rateLimits":{"credits":{"hasCredits":false}}}}));
+    assert_eq!(partial.paid_fallback, None);
+}
+
+#[test]
+fn a_message_that_says_nothing_about_the_account_is_empty() {
+    assert!(Signals::default().is_empty());
+    assert!(claude_signals(&json!({ "type": "rate_limit_event", "rate_limit_info": {} })).is_empty());
+    assert!(!claude_signals(&json!({ "rate_limit_info": { "isUsingOverage": false } })).is_empty());
+}
```

- [ ] **Step 2: Run the new tests and see them fail**

Run: `cargo test -p faber-engine` (the named test files)
Expected: compile errors or failures naming the missing items.

- [ ] **Step 3: Implement**

`engine/src/claude.rs`

```diff
--- a/engine/src/claude.rs
+++ b/engine/src/claude.rs
@@ -3,6 +3,7 @@
 use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
 use crate::model::{Area, name_instructions};
 use crate::proc::{self, Spawner};
+use crate::signals::claude_signals;
 use serde_json::{Value, json};
 use std::collections::HashMap;
 use std::path::PathBuf;
@@ -129,19 +130,12 @@
             tool: s(&msg["request"]["tool_name"]),
             input: msg["request"]["input"].clone(),
         }],
-        "rate_limit_event" => msg["rate_limit_info"]["unifiedWindows"]
-            .as_object()
-            .map(|windows| {
-                windows
-                    .iter()
-                    .map(|(window, w)| EngineEvent::Quota {
-                        window: window.clone(),
-                        used: w["utilization"].as_f64().unwrap_or(0.0),
-                        resets_at: w["resetsAt"].as_f64().unwrap_or(0.0),
-                    })
-                    .collect()
-            })
-            .unwrap_or_default(),
+        "rate_limit_event" => {
+            let windows = msg["rate_limit_info"]["unifiedWindows"].as_object().into_iter().flatten().filter_map(|(window, w)| {
+                Some(EngineEvent::Quota { window: window.clone(), used: w["utilization"].as_f64()?, resets_at: w["resetsAt"].as_f64()? })
+            });
+            windows.chain(Some(claude_signals(msg)).filter(|s| !s.is_empty()).map(EngineEvent::Signals)).collect()
+        }
         "result" => result_events(msg),
         _ => vec![],
     }
```

`engine/src/codex.rs`

```diff
--- a/engine/src/codex.rs
+++ b/engine/src/codex.rs
@@ -5,6 +5,7 @@
 use crate::events::{Delivery, EngineEvent, Session, SessionCommand, UserTurn};
 use crate::model::{Area, name_instructions};
 use crate::proc::{self, Spawner};
+use crate::signals::{codex_account, codex_signals};
 use serde::Serialize;
 use serde_json::{Map, Value, json};
 use std::collections::{HashMap, HashSet};
@@ -160,13 +161,16 @@
         }
         "account/rateLimits/updated" => [&p["rateLimits"]["primary"], &p["rateLimits"]["secondary"]]
             .into_iter()
-            .filter(|w| w.is_object())
-            .map(|w| EngineEvent::Quota {
-                window: window_name(w["windowDurationMins"].as_i64().unwrap_or(0)),
-                used: w["usedPercent"].as_f64().unwrap_or(0.0) / 100.0,
-                resets_at: w["resetsAt"].as_f64().unwrap_or(0.0),
+            .filter_map(|w| {
+                Some(EngineEvent::Quota {
+                    window: window_name(w["windowDurationMins"].as_i64()?),
+                    used: w["usedPercent"].as_f64()? / 100.0,
+                    resets_at: w["resetsAt"].as_f64()?,
+                })
             })
+            .chain(Some(codex_signals(msg)).filter(|s| !s.is_empty()).map(EngineEvent::Signals))
             .collect(),
+        "account/updated" => Some(codex_account(msg)).filter(|s| !s.is_empty()).map(EngineEvent::Signals).into_iter().collect(),
         "turn/completed" => {
             let status = s(&p["turn"]["status"]);
             // The turn's usage is the thread's total now minus its total when the turn started. With no start to
@@ -547,6 +551,58 @@
             }
         }
     }
+}
+
+/// Asks a short-lived `codex app-server` for the account's quota windows and plan, so the first reading does not wait for a
+/// turn. The answers are dressed as the notifications the server sends later and go through the same parser; the rest of
+/// them (the account's email and other details) is dropped unread. Gives no events on any failure.
+pub async fn read_codex_limits(bin: &str, cwd: &Path, windows: bool, spawn: Spawner, timeout: Duration) -> Vec<EngineEvent> {
+    let Ok(p) = spawn(bin, &codex_args(Area::Chat, windows, false), cwd) else { return vec![] };
+    let mut stdin = p.stdin;
+    let mut child = p.child;
+    let mut lines = BufReader::new(p.stdout).lines();
+    let ask = async {
+        let init = json!({ "id": 1, "method": "initialize", "params": { "clientInfo": { "name": "faber", "title": "Faber", "version": "0.0.0" } } });
+        let _ = proc::write_line(&mut *stdin, &init).await;
+        let mut events = vec![];
+        let mut answered = 0;
+        while let Ok(Some(line)) = lines.next_line().await {
+            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
+            if !msg["error"].is_null() {
+                return vec![];
+            }
+            match msg["id"].as_i64() {
+                Some(1) => {
+                    let _ = proc::write_line(&mut *stdin, &json!({ "method": "initialized" })).await;
+                    let _ = proc::write_line(&mut *stdin, &json!({ "id": 2, "method": "account/rateLimits/read" })).await;
+                    let _ =
+                        proc::write_line(&mut *stdin, &json!({ "id": 3, "method": "account/read", "params": { "refreshToken": false } }))
+                            .await;
+                }
+                Some(2) => {
+                    let note = json!({ "method": "account/rateLimits/updated", "params": { "rateLimits": msg["result"]["rateLimits"] } });
+                    events.extend(parse_codex_notification(&note, &mut CodexTurnState::default()));
+                    answered += 1;
+                }
+                Some(3) => {
+                    let account = &msg["result"]["account"];
+                    // `chatgpt`, `apiKey`, ...: the same mode names `account/updated` uses, in lower case.
+                    let mode = account["type"].as_str().map(str::to_ascii_lowercase);
+                    let note = json!({ "method": "account/updated", "params": { "authMode": mode, "planType": account["planType"] } });
+                    events.extend(parse_codex_notification(&note, &mut CodexTurnState::default()));
+                    answered += 1;
+                }
+                _ => {}
+            }
+            if answered == 2 {
+                break;
+            }
+        }
+        events
+    };
+    let events = tokio::time::timeout(timeout, ask).await.unwrap_or_default();
+    proc::stop(&mut *stdin, &mut child).await;
+    events
 }
 
 #[derive(Debug, Clone, PartialEq, Serialize)]
```

`engine/src/engine.rs`

```diff
--- a/engine/src/engine.rs
+++ b/engine/src/engine.rs
@@ -5,9 +5,11 @@
 use crate::cli::Cli;
 use crate::codex::{CodexServer, CodexServerOptions, CodexThread};
 use crate::events::{EngineEvent, Session, UserTurn};
+use crate::ledger::{Ledger, plausible};
 use crate::model::*;
 use crate::proc::Spawner;
 use crate::router::{Choice, Pick, pick_model};
+use crate::signals::Signals;
 use crate::store::Store;
 use crate::usage;
 use crate::workspace::{Attachment, inside_folder, new_files, save_files};
@@ -34,6 +36,9 @@
 
 pub type Emit = Arc<dyn Fn(AppEvent) + Send + Sync>;
 
+/// The time in Unix seconds. Tests replace it to place quota readings in time.
+pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;
+
 /// Answers a routing question in place of the CLIs; tests use it so no real model is called.
 pub type Ask = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;
 
@@ -147,6 +152,25 @@
     SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
 }
 
+fn unix_secs() -> f64 {
+    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
+}
+
+/// How often the quota ledger is written while readings keep coming.
+const LEDGER_EVERY: f64 = 30.0;
+
+/// The readings the ledger remembers that have not reset yet, so Settings has numbers before the first answer.
+fn remembered(ledger: &Ledger, now: f64) -> HashMap<(Vendor, String), QuotaReading> {
+    Vendor::ALL
+        .into_iter()
+        .flat_map(|vendor| {
+            ledger.of(vendor).readings.iter().filter(move |(_, r)| r.resets_at > now).map(move |(window, r)| {
+                ((vendor, window.clone()), QuotaReading { vendor, window: window.clone(), used: r.used, resets_at: r.resets_at })
+            })
+        })
+        .collect()
+}
+
 fn new_id() -> String {
     Uuid::new_v4().to_string()
 }
@@ -187,6 +211,12 @@
     guess: Option<auto::Guess>,
     next_draft: Option<auto::Draft>,
     spare: Option<auto::Spare>,
+    /// What Faber remembers about each vendor's quota (`budget.json`).
+    ledger: Ledger,
+    ledger_saved: f64,
+    /// The PC's offset from UTC in seconds, as the screen reports it.
+    utc_offset: i64,
+    clock: Option<Clock>,
 }
 
 /// A handle to the engine; clones share the same engine.
@@ -199,6 +229,8 @@
 impl Engine {
     pub fn new(opts: EngineOptions, emit: Emit) -> Engine {
         let store = Store::new(&opts.data_dir);
+        let ledger = store.ledger();
+        let quotas = remembered(&ledger, unix_secs());
         let runtime = opts.runtime.clone();
         let core = Arc::new_cyclic(|me| {
             Mutex::new(Core {
@@ -215,11 +247,15 @@
                 codex: HashMap::new(),
                 codex_recommended: None,
                 codex_models: vec![],
-                quotas: HashMap::new(),
+                quotas,
                 running: Running::default(),
                 guess: None,
                 next_draft: None,
                 spare: None,
+                ledger,
+                ledger_saved: 0.0,
+                utc_offset: 0,
+                clock: None,
             })
         });
         Engine { core, runtime }
@@ -233,6 +269,29 @@
     pub fn vendors(&self) -> Vec<Vendor> {
         self.with(|c| c.vendors())
     }
+    /// The PC's offset from UTC in seconds (east is positive). The engine has no time zone database, so the screen says.
+    pub fn set_utc_offset(&self, secs: i64) {
+        self.with(|c| c.utc_offset = secs.clamp(-14 * 3600, 14 * 3600))
+    }
+    /// Replaces the clock; the quota arithmetic reads the time through it.
+    pub fn set_time_source(&self, clock: Clock) {
+        self.with(|c| {
+            c.clock = Some(clock);
+            // Which remembered readings are still current depends on the clock.
+            for (key, reading) in remembered(&c.ledger, c.secs()) {
+                c.quotas.entry(key).or_insert(reading);
+            }
+        })
+    }
+    /// A copy of what the engine remembers about each vendor's quota, for Settings and for tests.
+    pub fn ledger(&self) -> Ledger {
+        self.with(|c| c.ledger.clone())
+    }
+    /// Quota readings and account facts that came from outside a conversation, such as a short-lived CLI process asked
+    /// for them. Anything else in `events` is ignored.
+    pub fn note_account_events(&self, vendor: Vendor, events: Vec<EngineEvent>) {
+        self.with(|c| events.into_iter().for_each(|e| c.note_account_event(vendor, e)))
+    }
     pub fn set_codex_models(&self, models: Vec<String>, recommended: Option<String>) {
         self.with(|c| {
             c.codex_models = models;
@@ -325,11 +384,69 @@
                 server.retire();
             }
             c.close_spare();
+            c.save_ledger(true);
         })
     }
 }
 
 impl Core {
+    /// The time in Unix seconds.
+    fn secs(&self) -> f64 {
+        self.clock.as_ref().map_or_else(unix_secs, |clock| clock())
+    }
+
+    /// Writes the ledger, but at most every half minute unless `force`: readings come with every answer.
+    fn save_ledger(&mut self, force: bool) {
+        let now = self.secs();
+        if force || !(0.0..LEDGER_EVERY).contains(&(now - self.ledger_saved)) {
+            self.store.save_ledger(&self.ledger);
+            self.ledger_saved = now;
+        }
+    }
+
+    /// A reading of one quota window: kept for the screen, remembered by the ledger, and told to the screen.
+    fn note_quota(&mut self, vendor: Vendor, window: &str, used: f64, resets_at: f64) {
+        // A reading that cannot be real (a CLI that changed its format) is shown nowhere and learned from by no one.
+        if !plausible(used, resets_at) {
+            return;
+        }
+        let now = self.secs();
+        let reading = QuotaReading { vendor, window: window.to_string(), used, resets_at };
+        self.quotas.insert((vendor, window.to_string()), reading);
+        self.ledger.of_mut(vendor).record(window, used, resets_at, now, self.utc_offset);
+        self.save_ledger(false);
+        (self.emit)(AppEvent::Quota { vendor, window: window.to_string(), used, resets_at });
+    }
+
+    /// What a CLI said about the account: merged into what the ledger knows, field by field.
+    fn apply_signals(&mut self, vendor: Vendor, s: Signals) {
+        let now = self.secs();
+        let v = self.ledger.of_mut(vendor);
+        if let Some(reach) = s.reach {
+            v.reach = reach;
+        }
+        if let Some(on) = s.overage_active {
+            v.overage_active = on;
+        }
+        if let Some(possible) = s.paid_fallback {
+            v.paid_fallback = possible;
+        }
+        if let Some(billing) = s.billing {
+            v.billing = billing;
+        }
+        if s.plan.is_some() {
+            v.plan_detected(s.plan, now);
+        }
+    }
+
+    fn note_account_event(&mut self, vendor: Vendor, event: EngineEvent) {
+        match event {
+            EngineEvent::Quota { window, used, resets_at } => self.note_quota(vendor, &window, used, resets_at),
+            EngineEvent::Signals(s) => self.apply_signals(vendor, s),
+            _ => {}
+        }
+    }
+
     fn vendors(&self) -> Vec<Vendor> {
         self.opts.clis.keys().copied().collect()
     }
@@ -514,11 +631,8 @@
                 }
                 return self.push(conv, turn_id.as_deref(), true);
             }
-            EngineEvent::Quota { window, used, resets_at } => {
-                let reading = QuotaReading { vendor, window: window.clone(), used, resets_at };
-                self.quotas.insert((vendor, window.clone()), reading);
-                return (self.emit)(AppEvent::Quota { vendor, window, used, resets_at });
-            }
+            EngineEvent::Quota { window, used, resets_at } => return self.note_quota(vendor, &window, used, resets_at),
+            EngineEvent::Signals(s) => return self.apply_signals(vendor, s),
             EngineEvent::Done {
                 ok,
                 cost_total_usd,
@@ -584,6 +698,7 @@
         }
         c.updated_at = now();
         self.save(conv);
+        self.save_ledger(false);
         self.push(conv, turn_id.as_deref(), true);
         self.list_changed();
         if self.live.get(conv).is_some_and(|l| self.opts.clis.get(&l.vendor) != Some(&l.cli))
```

`engine/src/engine/auto.rs`

```diff
--- a/engine/src/engine/auto.rs
+++ b/engine/src/engine/auto.rs
@@ -162,6 +162,7 @@
         let (tx, answer) = oneshot::channel();
         let text = Arc::new(Mutex::new(String::new()));
         let tx = Arc::new(Mutex::new(Some(tx)));
+        let me = self.me.clone();
         let sink: EventSink = Arc::new(move |e| match e {
             EngineEvent::Text(t) => text.lock().unwrap_or_else(|e| e.into_inner()).push_str(&t),
             EngineEvent::Done { .. } | EngineEvent::Error { .. } => {
@@ -169,6 +170,12 @@
                     let _ = tx.send(Some(std::mem::take(&mut *text.lock().unwrap_or_else(|e| e.into_inner()))));
                 }
             }
+            // The router's process sees account-wide news too (Codex hands it to one thread): the ledger must not miss it.
+            e @ (EngineEvent::Quota { .. } | EngineEvent::Signals(_)) => {
+                if let Some(core) = me.upgrade() {
+                    core.lock().unwrap_or_else(|e| e.into_inner()).note_account_event(vendor, e);
+                }
+            }
             _ => {}
         });
         let (cwd, instructions) = (self.opts.data_dir.clone(), Some(INSTRUCTIONS.to_string()));
```

`engine/src/events.rs`

```diff
--- a/engine/src/events.rs
+++ b/engine/src/events.rs
@@ -33,6 +33,8 @@
         used: f64,
         resets_at: f64,
     },
+    /// What the CLI said about the account besides window readings: whether it can answer, paid fallback, plan, billing.
+    Signals(crate::signals::Signals),
     /// What a finished turn used. `cost_total_usd` is Claude Code's own API-priced figure as it reports it: cumulative for
     /// the conversation, so the engine subtracts what it saw before (`usage::settle`); None when the result carries no
     /// counters (an aborted turn) and on Codex, which reports no cost. `input_tokens` is the prompt of the main response,
```

`engine/src/ledger.rs`

```diff
--- a/engine/src/ledger.rs
+++ b/engine/src/ledger.rs
@@ -1,281 +1,285 @@
-//! What Faber remembers about each vendor's quota: the latest reading of every window, the readings over the last week, the
-//! turns Faber itself ran, and what was learned from them. Plain data with explicit times, so it is testable without a clock;
-//! the engine keeps one and writes it to `budget.json`.
-
-use crate::budget::{Billing, Reach, Shape, Snap, Window, vendor_wide, window_secs};
-use crate::learn::{Learned, Outside, Point, Span, Stretch, Weights, capacity_changed, outside, stretches, weights};
-use crate::model::Vendor;
-use serde::{Deserialize, Serialize};
-use std::collections::BTreeMap;
-
-/// Readings kept per window, and spans kept per vendor: bounds the file at a few tens of kilobytes.
-const KEEP_POINTS: usize = 200;
-const KEEP_SPANS: usize = 200;
-const KEEP_STRETCHES: usize = 60;
-/// A plan check younger than this lets a discount stand.
-pub const PLAN_FRESH: f64 = 30.0 * 60.0;
-/// After a plan change, discounts wait for this long or for enough stretches.
-pub const RELEARN: f64 = 48.0 * 3600.0;
-/// Failed checks in a row before the user is asked which plan they have.
-pub const ASK_AFTER: u32 = 3;
-
-/// Whether a reading can be real: a share between 0 and a little over 1, and a reset time that is a number.
-pub fn plausible(used: f64, resets_at: f64) -> bool {
-    (0.0..=1.5).contains(&used) && resets_at.is_finite() && resets_at > 0.0
-}
-
-#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
-pub struct Reading {
-    pub used: f64,
-    pub resets_at: f64,
-    pub read_at: f64,
-}
-
-/// What is kept for one vendor.
-#[derive(Debug, Clone, Default, Serialize, Deserialize)]
-#[serde(default)]
-pub struct VendorLedger {
-    pub readings: BTreeMap<String, Reading>,
-    pub history: BTreeMap<String, Vec<Point>>,
-    pub spans: Vec<Span>,
-    pub learned: Learned,
-    pub weights: Weights,
-    pub stretches: Vec<Stretch>,
-    /// The plan as last detected, and the one the user typed when detection failed.
-    pub plan: Option<String>,
-    pub typed_plan: Option<String>,
-    /// Readings and spans before this time are ignored for outside use and weights (a plan change starts them again).
-    pub since: f64,
-    pub relearn_until: f64,
-    pub stretches_since: usize,
-    // What follows is about the moment, not the week: never written to disk.
-    #[serde(skip)]
-    pub billing: Billing,
-    #[serde(skip)]
-    pub reach: Reach,
-    #[serde(skip)]
-    pub overage_active: bool,
-    #[serde(skip)]
-    pub paid_fallback: bool,
-    /// When a plan check last succeeded, and how many failed in a row since.
-    #[serde(skip)]
-    pub plan_seen_at: Option<f64>,
-    #[serde(skip)]
-    pub failed_checks: u32,
-    /// A check worked but named no plan.
-    #[serde(skip)]
-    pub nameless: bool,
-}
-
-#[derive(Debug, Clone, Default, Serialize, Deserialize)]
-#[serde(default)]
-pub struct Ledger {
-    pub claude: VendorLedger,
-    pub codex: VendorLedger,
-}
-
-impl Ledger {
-    /// Makes a ledger read from disk safe to use: a damaged or hand-edited file must neither crash anything nor make a number
-    /// up. Whatever does not make sense is dropped, which only means that part is learned again.
-    pub fn sanitize(&mut self) {
-        self.claude.sanitize();
-        self.codex.sanitize();
-    }
-
-    pub fn of(&self, vendor: Vendor) -> &VendorLedger {
-        match vendor {
-            Vendor::Claude => &self.claude,
-            Vendor::Codex => &self.codex,
-        }
-    }
-
-    pub fn of_mut(&mut self, vendor: Vendor) -> &mut VendorLedger {
-        match vendor {
-            Vendor::Claude => &mut self.claude,
-            Vendor::Codex => &mut self.codex,
-        }
-    }
-}
-
-impl VendorLedger {
-    fn sanitize(&mut self) {
-        self.readings.retain(|_, r| plausible(r.used, r.resets_at) && r.read_at.is_finite());
-        for list in self.history.values_mut() {
-            list.retain(|p| plausible(p.used, p.resets_at) && p.at.is_finite());
-            list.drain(..list.len().saturating_sub(KEEP_POINTS));
-        }
-        self.history.retain(|_, list| !list.is_empty());
-        self.spans.retain(|s| s.start.is_finite() && s.end.is_finite() && s.end >= s.start && s.cost.is_none_or(|c| c.is_finite() && c >= 0.0));
-        self.spans.drain(..self.spans.len().saturating_sub(KEEP_SPANS));
-        self.stretches.retain(|s| s.ppd.is_finite() && s.ppd > 0.0);
-        self.stretches.drain(..self.stretches.len().saturating_sub(KEEP_STRETCHES));
-        // The shape indexes 168 hours of the week; any other length would index out of range.
-        if self.learned.mass.len() != 168 || self.learned.mass.iter().any(|m| !m.is_finite() || *m < 0.0) || !self.learned.covered.is_finite() {
-            self.learned = Learned::default();
-        }
-        self.weights.w.retain(|_, w| w.is_finite());
-        self.weights.w.values_mut().for_each(|w| *w = w.clamp(0.5, 2.0));
-        self.weights.capacity = self.weights.capacity.filter(|c| c.is_finite() && *c > 0.0);
-        for t in [&mut self.since, &mut self.relearn_until] {
-            if !t.is_finite() {
-                *t = 0.0;
-            }
-        }
-    }
-
-    /// The window the week's shape and the weights are learned from: the shortest vendor-wide one, which moves the most
-    /// points per unit of use. Only its readings are kept as a history.
-    pub fn primary(&self) -> Option<&str> {
-        self.readings
-            .keys()
-            .filter(|n| vendor_wide(n))
-            .filter_map(|n| Some((window_secs(n)?, n.as_str())))
-            .min_by(|a, b| a.0.total_cmp(&b.0))
-            .map(|(_, n)| n)
-    }
-
-    /// A reading of a window. Readings that make no sense (not a number, below 0 or far above 1) are dropped.
-    pub fn record(&mut self, window: &str, used: f64, resets_at: f64, now: f64, offset: i64) {
-        if !plausible(used, resets_at) {
-            return;
-        }
-        // Whole seconds and hundredths are all a reading holds; the file stays small.
-        let (now, resets_at, used) = (now.round(), resets_at.round(), (used * 1000.0).round() / 1000.0);
-        self.readings.insert(window.to_string(), Reading { used, resets_at, read_at: now });
-        let point = Point { at: now, used, resets_at };
-        let primary = self.primary().map(String::from);
-        self.history.retain(|name, _| Some(name) == primary.as_ref());
-        if primary.as_deref() != Some(window) {
-            return;
-        }
-        let list = self.history.entry(window.to_string()).or_default();
-        let previous = list.last().copied();
-        list.push(point);
-        if list.len() > KEEP_POINTS {
-            list.drain(..list.len() - KEEP_POINTS);
-        }
-        if let Some(previous) = previous
-            && previous.at >= self.since
-        {
-            self.learned.add(&previous, &point, offset);
-        }
-    }
-
-    /// A turn Faber ran, for outside use and weights.
-    pub fn span(&mut self, span: Span) {
-        let span = Span { start: span.start.round(), end: span.end.round(), cost: span.cost.map(|c| (c * 1e6).round() / 1e6), ..span };
-        self.spans.push(span);
-        if self.spans.len() > KEEP_SPANS {
-            self.spans.drain(..self.spans.len() - KEEP_SPANS);
-        }
-    }
-
-    fn recent(&self, window: &str) -> Vec<Point> {
-        self.history.get(window).map(|h| h.iter().filter(|p| p.at >= self.since).copied().collect()).unwrap_or_default()
-    }
-
-    fn recent_spans(&self) -> Vec<Span> {
-        self.spans.iter().filter(|s| s.end >= self.since).cloned().collect()
-    }
-
-    /// Use from outside Faber over the last week, from the primary window.
-    pub fn outside(&self, now: f64) -> Outside {
-        self.primary().map(|w| outside(&self.recent(w), &self.recent_spans(), now)).unwrap_or_default()
-    }
-
-    /// Learns the weights again from the stretches in the history. Returns true when the vendor's capacity moved by more
-    /// than a factor of two, which is a plan change the name did not announce.
-    pub fn learn_weights(&mut self, now: f64) -> bool {
-        let Some(window) = self.primary().map(String::from) else { return false };
-        let found = stretches(&self.recent(&window), &self.recent_spans());
-        self.stretches = found.iter().rev().take(KEEP_STRETCHES).rev().cloned().collect();
-        self.stretches_since = found.len();
-        let fresh = weights(&found);
-        let changed = matches!((self.weights.capacity, fresh.capacity), (Some(old), Some(new)) if capacity_changed(old, new));
-        if changed {
-            self.restart(now);
-        } else {
-            self.weights = fresh;
-        }
-        changed
-    }
-
-    /// A different plan: what was learned about weights, capacity and outside use is cleared, and discounts wait.
-    fn restart(&mut self, now: f64) {
-        self.weights = Weights::default();
-        self.stretches.clear();
-        self.stretches_since = 0;
-        self.since = now;
-        self.relearn_until = now + RELEARN;
-    }
-
-    /// A plan name, from a successful check. A change from one name to another restarts the learning.
-    pub fn plan_detected(&mut self, plan: Option<String>, now: f64) {
-        self.plan_seen_at = Some(now);
-        self.failed_checks = 0;
-        self.nameless = self.nameless && plan.is_none();
-        if let (Some(old), Some(new)) = (&self.plan, &plan)
-            && old != new
-        {
-            self.restart(now);
-        }
-        if plan.is_some() {
-            self.plan = plan;
-        }
-        // A typed plan never outlives a detection that disagrees with it.
-        if self.plan.is_some() && self.typed_plan != self.plan {
-            self.typed_plan = None;
-        }
-    }
-
-    pub fn plan_failed(&mut self) {
-        self.failed_checks += 1;
-    }
-
-    /// A check worked but the answer named no plan.
-    pub fn plan_missing(&mut self) {
-        self.nameless = true;
-        self.failed_checks = 0;
-    }
-
-    /// Whether to ask the user which plan they have: detection failed three times in a row, or worked but named no plan.
-    pub fn ask_for_plan(&self) -> bool {
-        self.plan.is_none() && self.typed_plan.is_none() && (self.failed_checks >= ASK_AFTER || self.nameless)
-    }
-
-    /// A discount needs a plan detected less than half an hour ago, or typed by the user.
-    pub fn plan_ok(&self, now: f64) -> bool {
-        self.typed_plan.is_some() || self.plan_seen_at.is_some_and(|t| (0.0..=PLAN_FRESH).contains(&(now - t)))
-    }
-
-    pub fn relearning(&self, now: f64) -> bool {
-        now < self.relearn_until && self.stretches_since < crate::learn::ENOUGH
-    }
-
-    pub fn shape(&self) -> Shape {
-        self.learned.shape()
-    }
-
-    pub fn snap(&self, now: f64) -> Snap {
-        let windows = self
-            .readings
-            .iter()
-            .map(|(name, r)| {
-                (
-                    name.clone(),
-                    Window { used: r.used, resets_at: r.resets_at, read_at: r.read_at, length: window_secs(name).unwrap_or(0.0) },
-                )
-            })
-            .collect();
-        Snap {
-            windows,
-            billing: self.billing,
-            reach: self.reach,
-            overage_active: self.overage_active,
-            paid_fallback: self.paid_fallback,
-            jumped: self.outside(now).jumped(),
-            relearning: self.relearning(now),
-            plan_ok: self.plan_ok(now),
-        }
-    }
-}
+//! What Faber remembers about each vendor's quota: the latest reading of every window, the readings over the last week, the
+//! turns Faber itself ran, and what was learned from them. Plain data with explicit times, so it is testable without a clock;
+//! the engine keeps one and writes it to `budget.json`.
+
+use crate::budget::{Billing, Reach, Shape, Snap, Window, vendor_wide, window_secs};
+use crate::learn::{Learned, Outside, Point, Span, Stretch, Weights, capacity_changed, outside, stretches, weights};
+use crate::model::Vendor;
+use serde::{Deserialize, Serialize};
+use std::collections::BTreeMap;
+
+/// Readings kept per window, and spans kept per vendor: bounds the file at a few tens of kilobytes.
+const KEEP_POINTS: usize = 200;
+const KEEP_SPANS: usize = 200;
+const KEEP_STRETCHES: usize = 60;
+/// A plan check younger than this lets a discount stand.
+pub const PLAN_FRESH: f64 = 30.0 * 60.0;
+/// After a plan change, discounts wait for this long or for enough stretches.
+pub const RELEARN: f64 = 48.0 * 3600.0;
+/// Failed checks in a row before the user is asked which plan they have.
+pub const ASK_AFTER: u32 = 3;
+
+/// Whether a reading can be real: a share between 0 and a little over 1, and a reset time that is a number.
+pub fn plausible(used: f64, resets_at: f64) -> bool {
+    (0.0..=1.5).contains(&used) && resets_at.is_finite() && resets_at > 0.0
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
+pub struct Reading {
+    pub used: f64,
+    pub resets_at: f64,
+    pub read_at: f64,
+}
+
+/// What is kept for one vendor.
+#[derive(Debug, Clone, Default, Serialize, Deserialize)]
+#[serde(default)]
+pub struct VendorLedger {
+    pub readings: BTreeMap<String, Reading>,
+    pub history: BTreeMap<String, Vec<Point>>,
+    pub spans: Vec<Span>,
+    pub learned: Learned,
+    pub weights: Weights,
+    pub stretches: Vec<Stretch>,
+    /// The plan as last detected, and the one the user typed when detection failed.
+    pub plan: Option<String>,
+    pub typed_plan: Option<String>,
+    /// Readings and spans before this time are ignored for outside use and weights (a plan change starts them again).
+    pub since: f64,
+    pub relearn_until: f64,
+    pub stretches_since: usize,
+    // What follows is about the moment, not the week: never written to disk.
+    #[serde(skip)]
+    pub billing: Billing,
+    #[serde(skip)]
+    pub reach: Reach,
+    #[serde(skip)]
+    pub overage_active: bool,
+    #[serde(skip)]
+    pub paid_fallback: bool,
+    /// When a plan check last succeeded, and how many failed in a row since.
+    #[serde(skip)]
+    pub plan_seen_at: Option<f64>,
+    #[serde(skip)]
+    pub failed_checks: u32,
+    /// A check worked but named no plan.
+    #[serde(skip)]
+    pub nameless: bool,
+}
+
+#[derive(Debug, Clone, Default, Serialize, Deserialize)]
+#[serde(default)]
+pub struct Ledger {
+    pub claude: VendorLedger,
+    pub codex: VendorLedger,
+}
+
+impl Ledger {
+    /// Makes a ledger read from disk safe to use: a damaged or hand-edited file must neither crash anything nor make a number
+    /// up. Whatever does not make sense is dropped, which only means that part is learned again.
+    pub fn sanitize(&mut self) {
+        self.claude.sanitize();
+        self.codex.sanitize();
+    }
+
+    pub fn of(&self, vendor: Vendor) -> &VendorLedger {
+        match vendor {
+            Vendor::Claude => &self.claude,
+            Vendor::Codex => &self.codex,
+        }
+    }
+
+    pub fn of_mut(&mut self, vendor: Vendor) -> &mut VendorLedger {
+        match vendor {
+            Vendor::Claude => &mut self.claude,
+            Vendor::Codex => &mut self.codex,
+        }
+    }
+}
+
+impl VendorLedger {
+    fn sanitize(&mut self) {
+        self.readings.retain(|_, r| plausible(r.used, r.resets_at) && r.read_at.is_finite());
+        for list in self.history.values_mut() {
+            list.retain(|p| plausible(p.used, p.resets_at) && p.at.is_finite());
+            list.drain(..list.len().saturating_sub(KEEP_POINTS));
+        }
+        self.history.retain(|_, list| !list.is_empty());
+        self.spans
+            .retain(|s| s.start.is_finite() && s.end.is_finite() && s.end >= s.start && s.cost.is_none_or(|c| c.is_finite() && c >= 0.0));
+        self.spans.drain(..self.spans.len().saturating_sub(KEEP_SPANS));
+        self.stretches.retain(|s| s.ppd.is_finite() && s.ppd > 0.0);
+        self.stretches.drain(..self.stretches.len().saturating_sub(KEEP_STRETCHES));
+        // The shape indexes 168 hours of the week; any other length would index out of range.
+        if self.learned.mass.len() != 168
+            || self.learned.mass.iter().any(|m| !m.is_finite() || *m < 0.0)
+            || !self.learned.covered.is_finite()
+        {
+            self.learned = Learned::default();
+        }
+        self.weights.w.retain(|_, w| w.is_finite());
+        self.weights.w.values_mut().for_each(|w| *w = w.clamp(0.5, 2.0));
+        self.weights.capacity = self.weights.capacity.filter(|c| c.is_finite() && *c > 0.0);
+        for t in [&mut self.since, &mut self.relearn_until] {
+            if !t.is_finite() {
+                *t = 0.0;
+            }
+        }
+    }
+
+    /// The window the week's shape and the weights are learned from: the shortest vendor-wide one, which moves the most
+    /// points per unit of use. Only its readings are kept as a history.
+    pub fn primary(&self) -> Option<&str> {
+        self.readings
+            .keys()
+            .filter(|n| vendor_wide(n))
+            .filter_map(|n| Some((window_secs(n)?, n.as_str())))
+            .min_by(|a, b| a.0.total_cmp(&b.0))
+            .map(|(_, n)| n)
+    }
+
+    /// A reading of a window. Readings that make no sense (not a number, below 0 or far above 1) are dropped.
+    pub fn record(&mut self, window: &str, used: f64, resets_at: f64, now: f64, offset: i64) {
+        if !plausible(used, resets_at) {
+            return;
+        }
+        // Whole seconds and hundredths are all a reading holds; the file stays small.
+        let (now, resets_at, used) = (now.round(), resets_at.round(), (used * 1000.0).round() / 1000.0);
+        self.readings.insert(window.to_string(), Reading { used, resets_at, read_at: now });
+        let point = Point { at: now, used, resets_at };
+        let primary = self.primary().map(String::from);
+        self.history.retain(|name, _| Some(name) == primary.as_ref());
+        if primary.as_deref() != Some(window) {
+            return;
+        }
+        let list = self.history.entry(window.to_string()).or_default();
+        let previous = list.last().copied();
+        list.push(point);
+        if list.len() > KEEP_POINTS {
+            list.drain(..list.len() - KEEP_POINTS);
+        }
+        if let Some(previous) = previous
+            && previous.at >= self.since
+        {
+            self.learned.add(&previous, &point, offset);
+        }
+    }
+
+    /// A turn Faber ran, for outside use and weights.
+    pub fn span(&mut self, span: Span) {
+        let span = Span { start: span.start.round(), end: span.end.round(), cost: span.cost.map(|c| (c * 1e6).round() / 1e6), ..span };
+        self.spans.push(span);
+        if self.spans.len() > KEEP_SPANS {
+            self.spans.drain(..self.spans.len() - KEEP_SPANS);
+        }
+    }
+
+    fn recent(&self, window: &str) -> Vec<Point> {
+        self.history.get(window).map(|h| h.iter().filter(|p| p.at >= self.since).copied().collect()).unwrap_or_default()
+    }
+
+    fn recent_spans(&self) -> Vec<Span> {
+        self.spans.iter().filter(|s| s.end >= self.since).cloned().collect()
+    }
+
+    /// Use from outside Faber over the last week, from the primary window.
+    pub fn outside(&self, now: f64) -> Outside {
+        self.primary().map(|w| outside(&self.recent(w), &self.recent_spans(), now)).unwrap_or_default()
+    }
+
+    /// Learns the weights again from the stretches in the history. Returns true when the vendor's capacity moved by more
+    /// than a factor of two, which is a plan change the name did not announce.
+    pub fn learn_weights(&mut self, now: f64) -> bool {
+        let Some(window) = self.primary().map(String::from) else { return false };
+        let found = stretches(&self.recent(&window), &self.recent_spans());
+        self.stretches = found.iter().rev().take(KEEP_STRETCHES).rev().cloned().collect();
+        self.stretches_since = found.len();
+        let fresh = weights(&found);
+        let changed = matches!((self.weights.capacity, fresh.capacity), (Some(old), Some(new)) if capacity_changed(old, new));
+        if changed {
+            self.restart(now);
+        } else {
+            self.weights = fresh;
+        }
+        changed
+    }
+
+    /// A different plan: what was learned about weights, capacity and outside use is cleared, and discounts wait.
+    fn restart(&mut self, now: f64) {
+        self.weights = Weights::default();
+        self.stretches.clear();
+        self.stretches_since = 0;
+        self.since = now;
+        self.relearn_until = now + RELEARN;
+    }
+
+    /// A plan name, from a successful check. A change from one name to another restarts the learning.
+    pub fn plan_detected(&mut self, plan: Option<String>, now: f64) {
+        self.plan_seen_at = Some(now);
+        self.failed_checks = 0;
+        self.nameless = self.nameless && plan.is_none();
+        if let (Some(old), Some(new)) = (&self.plan, &plan)
+            && old != new
+        {
+            self.restart(now);
+        }
+        if plan.is_some() {
+            self.plan = plan;
+        }
+        // A typed plan never outlives a detection that disagrees with it.
+        if self.plan.is_some() && self.typed_plan != self.plan {
+            self.typed_plan = None;
+        }
+    }
+
+    pub fn plan_failed(&mut self) {
+        self.failed_checks += 1;
+    }
+
+    /// A check worked but the answer named no plan.
+    pub fn plan_missing(&mut self) {
+        self.nameless = true;
+        self.failed_checks = 0;
+    }
+
+    /// Whether to ask the user which plan they have: detection failed three times in a row, or worked but named no plan.
+    pub fn ask_for_plan(&self) -> bool {
+        self.plan.is_none() && self.typed_plan.is_none() && (self.failed_checks >= ASK_AFTER || self.nameless)
+    }
+
+    /// A discount needs a plan detected less than half an hour ago, or typed by the user.
+    pub fn plan_ok(&self, now: f64) -> bool {
+        self.typed_plan.is_some() || self.plan_seen_at.is_some_and(|t| (0.0..=PLAN_FRESH).contains(&(now - t)))
+    }
+
+    pub fn relearning(&self, now: f64) -> bool {
+        now < self.relearn_until && self.stretches_since < crate::learn::ENOUGH
+    }
+
+    pub fn shape(&self) -> Shape {
+        self.learned.shape()
+    }
+
+    pub fn snap(&self, now: f64) -> Snap {
+        let windows = self
+            .readings
+            .iter()
+            .map(|(name, r)| {
+                (
+                    name.clone(),
+                    Window { used: r.used, resets_at: r.resets_at, read_at: r.read_at, length: window_secs(name).unwrap_or(0.0) },
+                )
+            })
+            .collect();
+        Snap {
+            windows,
+            billing: self.billing,
+            reach: self.reach,
+            overage_active: self.overage_active,
+            paid_fallback: self.paid_fallback,
+            jumped: self.outside(now).jumped(),
+            relearning: self.relearning(now),
+            plan_ok: self.plan_ok(now),
+        }
+    }
+}
```

`engine/src/lib.rs`

```diff
--- a/engine/src/lib.rs
+++ b/engine/src/lib.rs
@@ -21,4 +21,4 @@
 pub mod workspace;
 
 pub use account::AccountStatus;
-pub use engine::{AppEvent, AppState, BranchRequest, Engine, EngineOptions, SendRequest, TerminalCommand, WarmRequest};
+pub use engine::{AppEvent, AppState, BranchRequest, Clock, Engine, EngineOptions, SendRequest, TerminalCommand, WarmRequest};
```

`engine/src/signals.rs`

```diff
--- a/engine/src/signals.rs
+++ b/engine/src/signals.rs
@@ -13,6 +13,13 @@
     pub paid_fallback: Option<bool>,
     pub plan: Option<String>,
     pub billing: Option<Billing>,
+}
+
+impl Signals {
+    /// True when the message said nothing about the account, so there is nothing to pass on.
+    pub fn is_empty(&self) -> bool {
+        *self == Signals::default()
+    }
 }
 
 /// Claude Code's `rate_limit_event`. A status other than `allowed` counts only when it is about a vendor-wide window; the
@@ -36,35 +43,60 @@
     let r = &msg["params"]["rateLimits"];
     let reset = [&r["primary"]["resetsAt"], &r["secondary"]["resetsAt"]].into_iter().filter_map(Value::as_f64).reduce(f64::max);
     let reached = !r["rateLimitReachedType"].is_null() || r["spendControlReached"] == true;
+    let reach = if reached {
+        Some(Reach::Out(reset))
+    } else {
+        (r.get("rateLimitReachedType").is_some() && r.get("spendControlReached").is_some()).then_some(Reach::Clear)
+    };
+    let paid_fallback = match (r["credits"]["hasCredits"].as_bool(), r["credits"]["unlimited"].as_bool()) {
+        (Some(true), _) | (_, Some(true)) => Some(true),
+        (Some(false), Some(false)) => Some(false),
+        _ => None,
+    };
     Signals {
-        reach: Some(if reached { Reach::Out(reset) } else { Reach::Clear }),
-        paid_fallback: Some(r["credits"]["hasCredits"] == true || r["credits"]["unlimited"] == true),
+        reach,
+        paid_fallback,
         plan: r["planType"].as_str().map(String::from),
+        billing: r["planType"].as_str().filter(|p| usage_plan(p)).map(|_| Billing::UsageBilled),
         ..Signals::default()
     }
 }
 
+/// Plan names that bill by use rather than by subscription (Codex lists some as `..._usage_based`).
+fn usage_plan(plan: &str) -> bool {
+    plan.to_ascii_lowercase().contains("usage_based")
+}
+
 /// Codex's `account/updated`: how the account signs in and its plan. The mode names are not credentials; nothing else in
-/// the message is kept.
+/// the message is kept. Only a ChatGPT sign-in on a plan that is not billed by use counts as a subscription: any other
+/// way in is kept out of Auto unless its subscription billing is confirmed.
 pub fn codex_account(msg: &Value) -> Signals {
     let p = &msg["params"];
-    let billing =
-        p["authMode"].as_str().map(|m| if m.to_ascii_lowercase().contains("api") { Billing::UsageBilled } else { Billing::Subscription });
-    Signals { plan: p["planType"].as_str().map(String::from), billing, ..Signals::default() }
+    let plan = p["planType"].as_str();
+    let by_use = plan.is_some_and(usage_plan);
+    let billing = match p["authMode"].as_str().map(str::to_ascii_lowercase).as_deref() {
+        _ if by_use => Billing::UsageBilled,
+        Some("chatgpt" | "chatgptauthtokens") => Billing::Subscription,
+        Some("apikey" | "api_key" | "amazonbedrock" | "googlevertexai") => Billing::UsageBilled,
+        _ => Billing::Unknown,
+    };
+    Signals { plan: plan.map(String::from), billing: Some(billing), ..Signals::default() }
 }
 
 /// What `claude auth status` printed, reduced to the plan and the billing: everything else in the output is dropped
 /// unread.
 pub fn claude_plan(output: &str) -> Signals {
-    let v: Value = serde_json::from_str(output).unwrap_or(Value::Null);
-    let plan = v["subscriptionType"].as_str().filter(|s| !s.is_empty()).map(String::from);
+    let Ok(v) = serde_json::from_str::<Value>(output) else { return Signals::default() };
+    let plan = v["subscriptionType"].as_str().filter(|s| !s.is_empty() && v["loggedIn"] != false).map(String::from);
     let method = ["authMethod", "authMode", "loginMethod"].into_iter().find_map(|k| v[k].as_str()).unwrap_or_default().to_ascii_lowercase();
-    let billing = if plan.is_some() {
-        Some(Billing::Subscription)
+    let billing = if v["loggedIn"] == false {
+        Billing::Unknown
     } else if method.contains("api") || method.contains("console") {
-        Some(Billing::UsageBilled)
+        Billing::UsageBilled
+    } else if plan.is_some() {
+        Billing::Subscription
     } else {
-        None
+        Billing::Unknown
     };
-    Signals { plan, billing, ..Signals::default() }
+    Signals { plan, billing: Some(billing), ..Signals::default() }
 }
```

`engine/src/store.rs`

```diff
--- a/engine/src/store.rs
+++ b/engine/src/store.rs
@@ -1,6 +1,7 @@
 //! Plain JSON files in the app's data folder: readable by people and by any future tool.
 //! Writes go to a temp file first, so a crash never leaves a half-written conversation.
 
+use crate::ledger::Ledger;
 use crate::model::{Conversation, Project, Settings, Summary};
 use serde::Serialize;
 use serde::de::DeserializeOwned;
@@ -12,12 +13,16 @@
     conversations: PathBuf,
     projects: PathBuf,
     settings: PathBuf,
+    budget: PathBuf,
 }
 
 /// Writes through a temp file. On failure the temp file is removed and the error returned; the old file stays as it was.
 fn write<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
+    write_text(path, &serde_json::to_string_pretty(value).expect("plain data always serializes"))
+}
+
+fn write_text(path: &Path, json: &str) -> io::Result<()> {
     let tmp = path.with_extension("json.tmp");
-    let json = serde_json::to_string_pretty(value).expect("plain data always serializes");
     fs::write(&tmp, json).and_then(|()| fs::rename(&tmp, path)).inspect_err(|_| drop(fs::remove_file(&tmp)))
 }
 
@@ -29,7 +34,12 @@
     pub fn new(data_dir: &Path) -> Self {
         let conversations = data_dir.join("conversations");
         let _ = fs::create_dir_all(&conversations);
-        Store { conversations, projects: data_dir.join("projects.json"), settings: data_dir.join("settings.json") }
+        Store {
+            conversations,
+            projects: data_dir.join("projects.json"),
+            settings: data_dir.join("settings.json"),
+            budget: data_dir.join("budget.json"),
+        }
     }
 
     fn file(&self, id: &str) -> PathBuf {
@@ -82,6 +92,19 @@
     pub fn save_settings(&self, settings: &Settings) -> io::Result<()> {
         write(&self.settings, settings)
     }
+
+    /// What Faber remembers about each vendor's quota. A missing, damaged or unreadable file is an empty ledger: it only
+    /// holds what was learned, and learning starts again.
+    pub fn ledger(&self) -> Ledger {
+        let mut ledger: Ledger = read(&self.budget).unwrap_or_default();
+        ledger.sanitize();
+        ledger
+    }
+
+    /// Compact on purpose: it is rewritten often and a person never has to read it.
+    pub fn save_ledger(&self, ledger: &Ledger) {
+        let _ = write_text(&self.budget, &serde_json::to_string(ledger).expect("plain data always serializes"));
+    }
 }
 
 #[cfg(test)]
```

- [ ] **Step 4: Run the engine suite, clippy and fmt**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): the engine keeps the ledger"
```

### Task 6: Plan detection and the quota pulse

R89: pulses require confirmed subscription billing; naming a plan is insufficient. A successful signed-out status clears billing to unknown. Tests pin both cases.

Know the plan without asking, and refresh Claude's readings when they are old.

**Files:**
- `engine/src/account.rs`
- `engine/src/engine.rs`
- `engine/src/engine/auto.rs`
- `engine/src/engine/pulse.rs`
- `engine/tests/plan.rs`

**Interfaces:**
- Consumes: Task 5. Produces: `account::claude_account`, `Engine::{check_plans, watch_accounts, answer_plan}`, `Core::pulse_if_due` (haiku, low effort, 'Reply with the single word ok.', gate `Purpose::Pulse`, at most every 20 minutes).

- [ ] **Step 1: Add the tests (they fail or do not compile yet)**

```diff
--- a/engine/tests/plan.rs
+++ b/engine/tests/plan.rs
@@ -0,0 +1,302 @@
+//! Plan detection, the billing verdict and the quota pulse. No real CLI runs: status commands and the pulse are replayed.
+
+mod support;
+
+use faber_engine::budget::Billing;
+use faber_engine::cli::Cli;
+use faber_engine::engine::Ask;
+use faber_engine::events::EngineEvent;
+use faber_engine::model::{Area, Vendor};
+use faber_engine::proc::{Proc, Spawner};
+use faber_engine::router::Choice;
+use faber_engine::signals::Signals;
+use faber_engine::{AppEvent, Engine, EngineOptions, WarmRequest};
+use serde_json::json;
+use std::collections::BTreeMap;
+use std::path::Path;
+use std::sync::atomic::{AtomicU64, Ordering};
+use std::sync::{Arc, Mutex};
+use std::time::Duration;
+use support::{Entry, fake, fixture};
+use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};
+use tokio::sync::mpsc::UnboundedReceiver;
+
+/// A moment before the recorded windows reset (the Claude five-hour window resets at 1791627000).
+const WHEN: u64 = 1_791_620_000;
+type Calls = Arc<Mutex<Vec<(String, Vec<String>)>>>;
+type Time = Arc<AtomicU64>;
+
+struct Pending;
+
+impl AsyncRead for Pending {
+    fn poll_read(
+        self: std::pin::Pin<&mut Self>,
+        _: &mut std::task::Context<'_>,
+        _: &mut ReadBuf<'_>,
+    ) -> std::task::Poll<std::io::Result<()>> {
+        std::task::Poll::Pending
+    }
+}
+
+/// A status command that prints `stdout` and ends, or never answers when `stdout` is None.
+fn status(stdout: Option<&'static str>) -> (Spawner, Calls) {
+    let calls: Calls = Arc::default();
+    let recorded = calls.clone();
+    let spawn: Spawner = Arc::new(move |bin, args, _cwd| {
+        recorded.lock().unwrap().push((bin.into(), args.to_vec()));
+        let Some(stdout) = stdout else {
+            return Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(Pending), stderr: None, child: None });
+        };
+        let (mut out, reader) = tokio::io::duplex(4096);
+        tokio::spawn(async move { drop(out.write_all(stdout.as_bytes()).await) });
+        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(reader), stderr: None, child: None })
+    });
+    (spawn, calls)
+}
+
+fn start(dir: &Path, spawn: Spawner, ask: Option<Ask>) -> (Engine, UnboundedReceiver<AppEvent>, Time) {
+    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
+    let opts = EngineOptions {
+        data_dir: dir.into(),
+        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
+        spawn,
+        windows: false,
+        max_warm: 3,
+        idle: Duration::from_secs(120),
+        runtime: tokio::runtime::Handle::current(),
+        ask,
+    };
+    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
+    let time: Time = Arc::new(AtomicU64::new(WHEN));
+    let clock = time.clone();
+    engine.set_time_source(Arc::new(move || clock.load(Ordering::SeqCst) as f64));
+    (engine, rx, time)
+}
+
+const FIVE: Duration = Duration::from_millis(500);
+
+#[tokio::test]
+async fn the_plan_comes_from_the_status_command_and_nothing_else_of_it_is_kept() {
+    let dir = tempfile::tempdir().unwrap();
+    let (spawn, calls) = status(Some(r#"{"loggedIn":true,"email":"someone@example.com","orgName":"Acme","subscriptionType":"max"}"#));
+    let (engine, mut events, _) = start(dir.path(), spawn, None);
+    engine.check_plans(FIVE).await;
+    let claude = engine.ledger().claude;
+    assert_eq!((claude.plan.as_deref(), claude.billing), (Some("max"), Billing::Subscription));
+    assert!(claude.plan_ok(WHEN as f64));
+    assert_eq!(*calls.lock().unwrap(), [("claude".to_string(), vec!["auth".to_string(), "status".to_string()])]);
+    engine.shutdown();
+    let saved = std::fs::read_to_string(dir.path().join("budget.json")).unwrap();
+    assert!(!saved.contains("someone@example.com") && !saved.contains("Acme"));
+    assert!(events.try_recv().is_err(), "a status check says nothing to the screen");
+}
+
+#[tokio::test]
+async fn a_login_billed_per_use_is_known_as_such_and_needs_no_plan() {
+    let dir = tempfile::tempdir().unwrap();
+    let (spawn, _) = status(Some(r#"{"loggedIn":true,"authMethod":"api_key"}"#));
+    let (engine, _, _) = start(dir.path(), spawn, None);
+    engine.check_plans(FIVE).await;
+    let claude = engine.ledger().claude;
+    assert_eq!((claude.billing, claude.plan.as_deref()), (Billing::UsageBilled, None));
+    assert!(!claude.ask_for_plan(), "there is no plan to ask about");
+}
+
+#[tokio::test]
+async fn without_a_plan_in_the_answer_the_user_is_asked_and_the_answer_never_authorizes_billing() {
+    let dir = tempfile::tempdir().unwrap();
+    let (spawn, _) = status(Some(r#"{"loggedIn":true,"authMethod":"claude.ai"}"#));
+    let (engine, _, _) = start(dir.path(), spawn, None);
+    engine.check_plans(FIVE).await;
+    assert!(engine.ledger().claude.ask_for_plan());
+    engine.answer_plan(Vendor::Claude, "  Max 20x! ");
+    let claude = engine.ledger().claude;
+    assert_eq!(claude.typed_plan.as_deref(), Some("max20x"));
+    assert!(claude.plan_ok(WHEN as f64 + 3600.0 * 5.0), "a typed plan needs no fresh detection");
+    assert!(!claude.ask_for_plan());
+    assert_eq!(claude.billing, Billing::Unknown, "naming a plan proves nothing about how the login is billed");
+    engine.answer_plan(Vendor::Claude, "   ");
+    assert_eq!(engine.ledger().claude.typed_plan.as_deref(), Some("max20x"), "an empty answer changes nothing");
+}
+
+#[tokio::test]
+async fn one_failed_check_asks_nothing_three_in_a_row_ask_and_a_failure_never_refreshes_the_plan() {
+    let dir = tempfile::tempdir().unwrap();
+    let (good, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"pro"}"#));
+    let (engine, _, time) = start(dir.path(), good, None);
+    engine.check_plans(FIVE).await;
+    assert!(engine.ledger().claude.plan_ok(WHEN as f64));
+    engine.shutdown();
+
+    // The same data folder, a CLI that no longer answers.
+    let (hung, _) = status(None);
+    let (engine, _, time2) = start(dir.path(), hung, None);
+    time2.store(WHEN + 31 * 60, Ordering::SeqCst);
+    drop(time);
+    engine.check_plans(Duration::from_millis(30)).await;
+    assert!(!engine.ledger().claude.plan_ok((WHEN + 31 * 60) as f64), "the plan is as old as the last good check");
+    assert!(!engine.ledger().claude.ask_for_plan());
+    engine.check_plans(Duration::from_millis(30)).await;
+    engine.check_plans(Duration::from_millis(30)).await;
+    // A plan was known from before, so nothing is asked even after three failures; discounts simply wait.
+    assert!(!engine.ledger().claude.ask_for_plan());
+    assert_eq!(engine.ledger().claude.plan.as_deref(), Some("pro"));
+
+    let fresh = tempfile::tempdir().unwrap();
+    let (hung, _) = status(None);
+    let (engine, _, _) = start(fresh.path(), hung, None);
+    for _ in 0..2 {
+        engine.check_plans(Duration::from_millis(30)).await;
+    }
+    assert!(!engine.ledger().claude.ask_for_plan(), "a single failed or slow attempt asks nothing");
+    engine.check_plans(Duration::from_millis(30)).await;
+    assert!(engine.ledger().claude.ask_for_plan());
+}
+
+#[tokio::test]
+async fn a_changed_plan_name_starts_the_learning_again() {
+    let dir = tempfile::tempdir().unwrap();
+    let (spawn, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"pro"}"#));
+    let (engine, _, _) = start(dir.path(), spawn, None);
+    engine.check_plans(FIVE).await;
+    assert!(!engine.ledger().claude.relearning(WHEN as f64));
+    engine.shutdown();
+
+    // The plan is remembered across runs, so the next run notices that it is a different one.
+    let (spawn, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
+    let (engine, _, _) = start(dir.path(), spawn, None);
+    engine.check_plans(FIVE).await;
+    let claude = engine.ledger().claude;
+    assert_eq!(claude.plan.as_deref(), Some("max"));
+    assert!(claude.relearning(WHEN as f64 + 60.0), "no discount until it has learned the new plan");
+}
+
+#[tokio::test]
+async fn watching_the_accounts_checks_at_once() {
+    let dir = tempfile::tempdir().unwrap();
+    let (spawn, calls) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
+    let (engine, _, _) = start(dir.path(), spawn, None);
+    engine.watch_accounts();
+    for _ in 0..50 {
+        if engine.ledger().claude.plan.is_some() {
+            break;
+        }
+        tokio::time::sleep(Duration::from_millis(20)).await;
+    }
+    assert_eq!(engine.ledger().claude.plan.as_deref(), Some("max"));
+    assert_eq!(calls.lock().unwrap().len(), 1, "once now, then every quarter of an hour");
+    engine.shutdown();
+}
+
+/// The recording of a Claude chat, with the one-word question a pulse asks in place of the recorded one.
+fn pulse_recording() -> Vec<Entry> {
+    fixture("claude", "chat")
+        .into_iter()
+        .map(|mut e| {
+            if e.input && e.msg["type"] == "user" {
+                e.msg["message"]["content"] = json!("ok");
+            }
+            e
+        })
+        .collect()
+}
+
+fn typing() -> WarmRequest {
+    WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: Choice { vendor: "auto".into(), model: None } }
+}
+
+fn silent() -> Ask {
+    Arc::new(|_| Box::pin(async { None }))
+}
+
+async fn settle_for(engine: &Engine, window: &str) -> bool {
+    for _ in 0..100 {
+        if engine.ledger().claude.readings.contains_key(window) {
+            return true;
+        }
+        tokio::time::sleep(Duration::from_millis(20)).await;
+    }
+    false
+}
+
+#[tokio::test]
+async fn typing_in_auto_asks_for_a_fresh_reading_only_when_the_one_held_is_old() {
+    let dir = tempfile::tempdir().unwrap();
+    let f = fake(pulse_recording());
+    let (engine, _, time) = start(dir.path(), f.spawn.clone(), Some(silent()));
+    engine.note_account_events(
+        Vendor::Claude,
+        vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), ..Signals::default() })],
+    );
+    engine.route_draft(typing(), "Plan a trip".into());
+    assert!(settle_for(&engine, "five_hour").await, "the pulse brought a reading");
+    let args = f.args.lock().unwrap().clone();
+    assert_eq!(args.len(), 1);
+    assert!(args[0].contains(&"claude-haiku-5-5".to_string()) && args[0].contains(&"--system-prompt".to_string()));
+    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
+    assert_eq!(engine.ledger().claude.spans.len(), 1, "the pulse is Faber's own request, so the learning knows when it ran");
+
+    engine.route_draft(typing(), "Plan a trip to Lisbon".into());
+    tokio::time::sleep(Duration::from_millis(100)).await;
+    assert_eq!(f.args.lock().unwrap().len(), 1, "the reading is fresh");
+
+    time.store(WHEN + 21 * 60, Ordering::SeqCst);
+    engine.route_draft(typing(), "Plan a trip to Porto".into());
+    for _ in 0..50 {
+        if f.args.lock().unwrap().len() == 2 {
+            break;
+        }
+        tokio::time::sleep(Duration::from_millis(20)).await;
+    }
+    assert_eq!(f.args.lock().unwrap().len(), 2, "twenty minutes later the reading is old");
+    engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_pulse_is_a_request_like_any_other_and_waits_at_the_gate() {
+    let dir = tempfile::tempdir().unwrap();
+    let f = fake(pulse_recording());
+    let (engine, _, time) = start(dir.path(), f.spawn.clone(), Some(silent()));
+    // Nearly out, and the CLI may switch to paid use by itself: the pulse must not be the request that does it.
+    let quota = EngineEvent::Quota { window: "five_hour".into(), used: 0.95, resets_at: (WHEN + 3600) as f64 };
+    let paid = EngineEvent::Signals(Signals { paid_fallback: Some(true), billing: Some(Billing::Subscription), ..Signals::default() });
+    engine.note_account_events(Vendor::Claude, vec![quota, paid]);
+    time.store(WHEN + 21 * 60, Ordering::SeqCst);
+    engine.route_draft(typing(), "Plan a trip".into());
+    tokio::time::sleep(Duration::from_millis(100)).await;
+    assert!(f.args.lock().unwrap().is_empty(), "denied: a window above 90% with paid use possible");
+    engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_typed_plan_never_allows_a_pulse_with_unknown_billing() {
+    let dir = tempfile::tempdir().unwrap();
+    let f = fake(pulse_recording());
+    let (engine, _, _) = start(dir.path(), f.spawn.clone(), Some(silent()));
+    engine.answer_plan(Vendor::Claude, "max");
+    engine.route_draft(typing(), "Plan a trip".into());
+    tokio::time::sleep(Duration::from_millis(100)).await;
+    assert_eq!(engine.ledger().claude.billing, Billing::Unknown);
+    assert!(f.args.lock().unwrap().is_empty());
+    engine.shutdown();
+}
+
+#[tokio::test]
+async fn signing_out_clears_a_previously_confirmed_subscription() {
+    let dir = tempfile::tempdir().unwrap();
+    let (connected, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
+    let (disconnected, _) = status(Some(r#"{"loggedIn":false}"#));
+    let calls = Arc::new(AtomicU64::new(0));
+    let spawn: Spawner =
+        Arc::new(
+            move |bin, args, cwd| {
+                if calls.fetch_add(1, Ordering::SeqCst) == 0 { connected(bin, args, cwd) } else { disconnected(bin, args, cwd) }
+            },
+        );
+    let (engine, _, _) = start(dir.path(), spawn, None);
+    engine.check_plans(FIVE).await;
+    assert_eq!(engine.ledger().claude.billing, Billing::Subscription);
+    engine.check_plans(FIVE).await;
+    assert_eq!(engine.ledger().claude.billing, Billing::Unknown);
+    engine.shutdown();
+}
```

- [ ] **Step 2: Run the new tests and see them fail**

Run: `cargo test -p faber-engine` (the named test files)
Expected: compile errors or failures naming the missing items.

- [ ] **Step 3: Implement**

`engine/src/account.rs`

```diff
--- a/engine/src/account.rs
+++ b/engine/src/account.rs
@@ -9,6 +9,7 @@
 use crate::cli::Cli;
 use crate::model::Vendor;
 use crate::proc::Spawner;
+use crate::signals::{Signals, claude_plan};
 use serde::Serialize;
 use std::path::Path;
 use std::time::Duration;
@@ -40,22 +41,44 @@
 // Bound transient output, including diagnostics, before discarding it after the verdict.
 const OUTPUT_LIMIT: u64 = 64 * 1024;
 
-/// Runs the status command without a shell. Its output is read into memory only to reach a verdict, then dropped;
-/// a CLI that fails to start or does not answer within `timeout` is `Unknown`, and is killed when the check ends.
-pub async fn check(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> AccountStatus {
+/// What a status command printed and how it ended.
+struct Ran {
+    out: String,
+    err: String,
+    code: Option<i32>,
+}
+
+/// Runs the status command without a shell, with its output bounded. None when the CLI fails to start, does not answer within
+/// `timeout` (it is killed when the run ends) or prints more than the bound.
+async fn run(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Option<Ran> {
     let args: Vec<String> = cli.prefix.iter().cloned().chain(status_args(vendor).iter().map(|a| a.to_string())).collect();
-    let Ok(p) = spawn(&cli.command, &args, cwd) else { return AccountStatus::Unknown };
+    let p = spawn(&cli.command, &args, cwd).ok()?;
     drop(p.stdin); // nothing to type: a CLI waiting for input sees end of input instead
     let mut child = p.child;
     let run = async {
-        let (Some(out), Some(err)) = tokio::join!(read_all(Some(p.stdout)), read_all(p.stderr)) else { return AccountStatus::Unknown };
+        let (Some(out), Some(err)) = tokio::join!(read_all(Some(p.stdout)), read_all(p.stderr)) else { return None };
         let code = match &mut child {
             Some(c) => c.wait().await.ok().and_then(|s| s.code()),
             None => None,
         };
-        if vendor == Vendor::Claude { verdict(vendor, &out, code) } else { verdict(vendor, &format!("{out}\n{err}"), code) }
+        Some(Ran { out, err, code })
     };
-    tokio::time::timeout(timeout, run).await.unwrap_or(AccountStatus::Unknown)
+    tokio::time::timeout(timeout, run).await.ok().flatten()
+}
+
+/// Runs the status command. Its output is read into memory only to reach a verdict, then dropped; a CLI that fails to start or
+/// does not answer within `timeout` is `Unknown`, and is killed when the check ends.
+pub async fn check(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> AccountStatus {
+    let Some(Ran { out, err, code }) = run(vendor, cli, spawn, cwd, timeout).await else { return AccountStatus::Unknown };
+    if vendor == Vendor::Claude { verdict(vendor, &out, code) } else { verdict(vendor, &format!("{out}\n{err}"), code) }
+}
+
+/// What `claude auth status` says about the plan and how the login is billed; nothing else of its output is kept (the email
+/// and organisation in it are dropped unread). None when the command failed, timed out or returned invalid status data.
+pub async fn claude_account(cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Option<Signals> {
+    let ran = run(Vendor::Claude, cli, spawn, cwd, timeout).await?;
+    serde_json::from_str::<serde_json::Value>(&ran.out).ok()?["loggedIn"].as_bool()?;
+    Some(claude_plan(&ran.out))
 }
 
 async fn read_all(stream: Option<Box<dyn AsyncRead + Send + Unpin>>) -> Option<String> {
```

`engine/src/engine.rs`

```diff
--- a/engine/src/engine.rs
+++ b/engine/src/engine.rs
@@ -1,10 +1,13 @@
 //! The engine: conversations, live CLI sessions, permissions, branches and attachments.
 
 use crate::account::{self, AccountStatus};
+use crate::budget::Billing;
 use crate::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
 use crate::cli::Cli;
+use crate::codex::read_codex_limits;
 use crate::codex::{CodexServer, CodexServerOptions, CodexThread};
 use crate::events::{EngineEvent, Session, UserTurn};
+use crate::learn::Span;
 use crate::ledger::{Ledger, plausible};
 use crate::model::*;
 use crate::proc::Spawner;
@@ -24,6 +27,7 @@
 
 mod answer;
 mod auto;
+mod pulse;
 
 #[derive(Debug, Clone, PartialEq, Serialize)]
 #[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
@@ -214,6 +218,8 @@
     /// What Faber remembers about each vendor's quota (`budget.json`).
     ledger: Ledger,
     ledger_saved: f64,
+    /// When the last quota pulse started.
+    last_pulse: f64,
     /// The PC's offset from UTC in seconds, as the screen reports it.
     utc_offset: i64,
     clock: Option<Clock>,
@@ -254,6 +260,7 @@
                 spare: None,
                 ledger,
                 ledger_saved: 0.0,
+                last_pulse: 0.0,
                 utc_offset: 0,
                 clock: None,
             })
@@ -283,6 +290,50 @@
             }
         })
     }
+    /// Asks each connected CLI who is signed in and on which plan, and keeps the answers in the ledger: Claude Code through its
+    /// status command, Codex through a short-lived server (which also refreshes its quota readings). A check that fails or
+    /// times out refreshes nothing, and three in a row make Settings ask the user which plan they have.
+    pub async fn check_plans(&self, timeout: Duration) {
+        type Job = (Vendor, Cli, Spawner, PathBuf, bool);
+        let jobs: Vec<Job> = self.with(|c| {
+            c.opts.clis.iter().map(|(v, cli)| (*v, cli.clone(), c.opts.spawn.clone(), c.opts.data_dir.clone(), c.opts.windows)).collect()
+        });
+        for (vendor, cli, spawn, cwd, windows) in jobs {
+            match vendor {
+                Vendor::Claude => {
+                    let found = account::claude_account(&cli, &spawn, &cwd, timeout).await;
+                    self.with(|c| c.claude_plan_checked(found));
+                }
+                Vendor::Codex => {
+                    let events = read_codex_limits(&cli.command, &cwd, windows, spawner(&spawn, &cli), timeout).await;
+                    self.with(|c| c.codex_plan_checked(events));
+                }
+            }
+        }
+    }
+    /// Checks the plans now and every quarter of an hour while Faber runs. The shell starts it once.
+    pub fn watch_accounts(&self) {
+        let (me, runtime) = (self.with(|c| c.me.clone()), self.runtime.clone());
+        let handle = runtime.clone();
+        runtime.spawn(async move {
+            loop {
+                let Some(core) = me.upgrade() else { return };
+                Engine { core, runtime: handle.clone() }.check_plans(Duration::from_secs(20)).await;
+                tokio::time::sleep(Duration::from_secs(15 * 60)).await;
+            }
+        });
+    }
+    /// The plan the user named when it could not be detected. It never overrides how the login is billed, and a later
+    /// detection that disagrees removes it.
+    pub fn answer_plan(&self, vendor: Vendor, plan: &str) {
+        let plan: String = plan.trim().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect();
+        self.with(|c| {
+            if !plan.is_empty() {
+                c.ledger.of_mut(vendor).typed_plan = Some(plan);
+                c.save_ledger(true);
+            }
+        })
+    }
     /// A copy of what the engine remembers about each vendor's quota, for Settings and for tests.
     pub fn ledger(&self) -> Ledger {
         self.with(|c| c.ledger.clone())
@@ -439,6 +490,32 @@
         }
     }
 
+    /// A request Faber made itself (an answer, a router question, a pulse): the learning has to know when it ran, so that use
+    /// it did not cause can be told from use it did.
+    fn note_span(&mut self, vendor: Vendor, start: f64, model: &str, cost: Option<f64>) {
+        let end = self.secs();
+        self.ledger.of_mut(vendor).span(Span { start, end, model: model.to_string(), cost });
+    }
+
+    fn claude_plan_checked(&mut self, found: Option<Signals>) {
+        let Some(s) = found else { return self.ledger.claude.plan_failed() };
+        let (named, by_use) = (s.plan.is_some(), s.billing == Some(Billing::UsageBilled));
+        self.apply_signals(Vendor::Claude, s);
+        if !named && !by_use {
+            self.ledger.claude.plan_missing();
+        }
+    }
+
+    fn codex_plan_checked(&mut self, events: Vec<EngineEvent>) {
+        if events.is_empty() {
+            return self.ledger.codex.plan_failed();
+        }
+        events.into_iter().for_each(|e| self.note_account_event(Vendor::Codex, e));
+        if self.ledger.codex.plan.is_none() && self.ledger.codex.billing != Billing::UsageBilled {
+            self.ledger.codex.plan_missing();
+        }
+    }
+
     fn note_account_event(&mut self, vendor: Vendor, event: EngineEvent) {
         match event {
             EngineEvent::Quota { window, used, resets_at } => self.note_quota(vendor, &window, used, resets_at),
```

`engine/src/engine/auto.rs`

```diff
--- a/engine/src/engine/auto.rs
+++ b/engine/src/engine/auto.rs
@@ -33,7 +33,7 @@
     only: Option<Vendor>,
 }
 
-type Stop = Arc<Mutex<Option<Session>>>;
+pub(super) type Stop = Arc<Mutex<Option<Session>>>;
 
 /// A router session started ahead of its question.
 pub(super) struct Spare {
@@ -69,6 +69,8 @@
         if req.choice.vendor != "auto" || text.is_empty() {
             return;
         }
+        // The first keystroke in Auto is the moment to ask Claude Code for a fresh reading, if the one held is old.
+        self.pulse_if_due();
         let id = self.draft_for(&req);
         let Some(route) = self.next_route(&id) else { return };
         let only = route.map(|p| p.vendor);
@@ -284,7 +286,7 @@
     }
 }
 
-fn stop_router(stop: Option<Stop>) {
+pub(super) fn stop_router(stop: Option<Stop>) {
     if let Some(session) = stop.and_then(|s| s.lock().unwrap_or_else(|e| e.into_inner()).take()) {
         session.close();
     }
```

`engine/src/engine/pulse.rs`

```diff
--- a/engine/src/engine/pulse.rs
+++ b/engine/src/engine/pulse.rs
@@ -0,0 +1,58 @@
+//! The quota pulse: Claude Code reports its quota windows only when it answers, so when the reading is old, a one-word question
+//! to the cheapest model asks for a fresh one. It runs in the background and nobody waits for it.
+
+use super::auto::{Stop, stop_router};
+use super::*;
+use crate::budget::{Purpose, gate};
+
+/// A reading older than this is worth asking for again, and a pulse is not repeated sooner either.
+const PULSE_EVERY: f64 = 20.0 * 60.0;
+const PULSE_MODEL: &str = "claude-haiku-5-5";
+
+impl Core {
+    /// Starts a pulse when the Claude reading is old and the gate lets Claude answer. A clock that went back counts as old.
+    pub(super) fn pulse_if_due(&mut self) {
+        let Some(cli) = self.opts.clis.get(&Vendor::Claude).cloned() else { return };
+        let now = self.secs();
+        let newest = self.ledger.claude.readings.values().map(|r| r.read_at).fold(f64::MIN, f64::max);
+        let due = |t: f64| !(0.0..PULSE_EVERY).contains(&(now - t));
+        if !due(newest) || !due(self.last_pulse) {
+            return;
+        }
+        if gate(&self.ledger.claude.snap(now), None, Purpose::Pulse, false, now).is_err() {
+            return;
+        }
+        self.last_pulse = now;
+        let (me, stop) = (self.me.clone(), Stop::default());
+        let closer = stop.clone();
+        let sink: EventSink = Arc::new(move |e| match e {
+            EngineEvent::Quota { .. } | EngineEvent::Signals(_) => {
+                if let Some(core) = me.upgrade() {
+                    core.lock().unwrap_or_else(|e| e.into_inner()).note_account_event(Vendor::Claude, e);
+                }
+            }
+            EngineEvent::Done { .. } | EngineEvent::Error { .. } => {
+                stop_router(Some(closer.clone()));
+                if let Some(core) = me.upgrade() {
+                    core.lock().unwrap_or_else(|e| e.into_inner()).note_span(Vendor::Claude, now, PULSE_MODEL, None);
+                }
+            }
+            _ => {}
+        });
+        let launch = ClaudeLaunch {
+            area: Area::Chat,
+            model: Some(PULSE_MODEL.into()),
+            effort: Some("low".into()),
+            resume: None,
+            user_hooks: false,
+            instructions: Some("Reply with the single word ok.".into()),
+            user_name: None,
+        };
+        let session = start_claude(
+            ClaudeOptions { bin: cli.command.clone(), cwd: self.opts.data_dir.clone(), launch, spawn: spawner(&self.opts.spawn, &cli) },
+            sink,
+        );
+        *stop.lock().unwrap_or_else(|e| e.into_inner()) = Some(session.clone());
+        session.send(UserTurn::text("ok"));
+    }
+}
```

- [ ] **Step 4: Run the engine suite, clippy and fmt**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): plan detection and the quota pulse"
```

### Task 8: The pacing table and dispatch through the gate

Auto prices on the quota of the moment of sending, never of the moment of typing, and nothing reaches a CLI without the gate.

**Files:**
- `engine/src/engine.rs`
- `engine/src/engine/auto.rs`
- `engine/src/model.rs`
- `engine/src/route.rs`
- `engine/src/router.rs`
- `engine/tests/pacing.rs`
- `engine/tests/route.rs`
- `engine/tests/route_pacing.rs`

**Interfaces:**
- Consumes: Tasks 3-4. Produces: `route::{Asked, asked, Factor, choose, choose_paced, Routed{price, pace}}` (the table prices every eligible profile at cost x weight x multiplier, gives step-up discounts and names the reason `step` or `low`), `model::PaceNote{kind, vendor}`, `Message.pace`, `Pick.pace`, `Core::allows(vendor, family, purpose, approved)`, `Core::table(&Asked, only)`, `Engine::start` gated by `gate_turn` (`error.billedPerUse`, `error.quotaOut`). The router's verdict is cached; the table runs at dispatch on the quota of that moment.

- [ ] **Step 1: Add the tests (they fail or do not compile yet)**

```diff
--- a/engine/tests/pacing.rs
+++ b/engine/tests/pacing.rs
@@ -0,0 +1,356 @@
+//! Auto with the quota in it: spare quota buys a step up, a vendor running low is left, and nothing doubtful or unpaid is used.
+//! Two vendors are connected; each is a replayed recording, and the router's verdict comes from a hook.
+//!
+//! What Auto chose is read from what was sent to the CLIs (the arguments of a Claude process, the lines written to Codex), not
+//! from the answer's `model`: the recordings report their own model as soon as they start, and that overwrites it.
+
+mod support;
+
+use faber_engine::budget::Billing;
+use faber_engine::cli::Cli;
+use faber_engine::engine::Ask;
+use faber_engine::events::EngineEvent;
+use faber_engine::model::{Area, Message, PaceNote, Role, Status, Vendor};
+use faber_engine::proc::Spawner;
+use faber_engine::router::Choice;
+use faber_engine::signals::Signals;
+use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
+use std::path::Path;
+use std::sync::atomic::{AtomicU64, Ordering};
+use std::sync::{Arc, Mutex};
+use std::time::Duration;
+use support::{Fake, fake, fixture};
+use tokio::sync::mpsc::UnboundedReceiver;
+
+const SKY: &str = "In one sentence: why is the sky blue?";
+const CODE: &str = "Fix the failing test in my billing service";
+/// A moment before the recorded windows reset.
+const WHEN: u64 = 1_791_620_000;
+const HOUR: u64 = 3600;
+const DAY: u64 = 86_400;
+type Time = Arc<AtomicU64>;
+
+struct Rig {
+    engine: Engine,
+    rx: UnboundedReceiver<AppEvent>,
+    time: Time,
+    claude: Fake,
+    codex: Fake,
+}
+
+/// What Auto sent where: the model and effort of the Claude process it started, or the Codex model it asked for.
+#[derive(Debug, PartialEq)]
+enum Went {
+    Claude(String, String),
+    Codex(&'static str),
+    Nowhere,
+}
+
+impl Rig {
+    /// Waits for the answer to be routed and started, and says where it went.
+    async fn went(&self, id: &str, n: usize) -> (Message, Went) {
+        for _ in 0..150 {
+            let reply = self.engine.open(id).and_then(|c| c.messages.get(n * 2 + 1).cloned());
+            if let Some(reply) = reply.filter(|m| m.vendor.is_some()) {
+                tokio::time::sleep(Duration::from_millis(60)).await;
+                let went = match reply.vendor {
+                    Some(Vendor::Claude) => {
+                        let args = self.claude.args.lock().unwrap().get(n).cloned().unwrap_or_default();
+                        Went::Claude(flag(&args, "--model").unwrap_or_default(), flag(&args, "--effort").unwrap_or_default())
+                    }
+                    _ => ["gpt-6.1-sol", "gpt-6-luna"]
+                        .into_iter()
+                        .find(|m| self.codex.wire.lines_with(&format!("\"model\":\"{m}\"")) > 0)
+                        .map_or(Went::Nowhere, Went::Codex),
+                };
+                return (reply, went);
+            }
+            tokio::time::sleep(Duration::from_millis(20)).await;
+        }
+        panic!("message {n} was never routed");
+    }
+}
+
+fn claude(model: &str, effort: &str) -> Went {
+    Went::Claude(model.into(), effort.into())
+}
+
+/// The verdicts the router gives in turn, then keeps the last one.
+fn verdicts(list: Vec<(&'static str, &'static str)>) -> Ask {
+    let calls = Arc::new(Mutex::new(0usize));
+    Arc::new(move |_| {
+        let mut n = calls.lock().unwrap();
+        let (kind, level) = list[(*n).min(list.len() - 1)];
+        *n += 1;
+        Box::pin(async move { Some(format!(r#"{{"kind":"{kind}","difficulty":"{level}","confidence":0.95}}"#)) })
+    })
+}
+
+fn rig(dir: &Path, vendors: &[Vendor], ask: Ask) -> Rig {
+    let (claude, codex) = (fake(fixture("claude", "chat")), fake(fixture("codex", "chat")));
+    let (c, x) = (claude.spawn.clone(), codex.spawn.clone());
+    let spawn: Spawner = Arc::new(move |bin, args, cwd| if bin == "claude" { c(bin, args, cwd) } else { x(bin, args, cwd) });
+    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
+    let opts = EngineOptions {
+        data_dir: dir.into(),
+        clis: vendors.iter().map(|v| (*v, Cli { command: v.name().into(), prefix: vec![] })).collect(),
+        spawn,
+        windows: false,
+        max_warm: 3,
+        idle: Duration::from_secs(120),
+        runtime: tokio::runtime::Handle::current(),
+        ask: Some(ask),
+    };
+    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
+    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
+    let time: Time = Arc::new(AtomicU64::new(WHEN));
+    let clock = time.clone();
+    engine.set_time_source(Arc::new(move || clock.load(Ordering::SeqCst) as f64));
+    Rig { engine, rx, time, claude, codex }
+}
+
+fn both(dir: &Path, ask: Ask) -> Rig {
+    rig(dir, &[Vendor::Claude, Vendor::Codex], ask)
+}
+
+fn auto(text: &str) -> SendRequest {
+    let choice = Choice { vendor: "auto".into(), model: None };
+    SendRequest { conversation_id: None, area: Area::Chat, project_id: None, text: text.into(), images: vec![], files: vec![], choice }
+}
+
+fn quota(window: &str, used: f64, resets_in: u64) -> EngineEvent {
+    EngineEvent::Quota { window: window.into(), used, resets_at: (WHEN + resets_in) as f64 }
+}
+
+fn plan(plan: &str) -> EngineEvent {
+    EngineEvent::Signals(Signals { plan: Some(plan.into()), billing: Some(Billing::Subscription), ..Signals::default() })
+}
+
+fn billed_by_use() -> EngineEvent {
+    EngineEvent::Signals(Signals { billing: Some(Billing::UsageBilled), ..Signals::default() })
+}
+
+/// A Claude with a lot of quota to spare: the week half over and a fifth used, the five-hour window barely touched.
+fn spare_claude(engine: &Engine) {
+    engine.note_account_events(
+        Vendor::Claude,
+        vec![plan("max"), quota("five_hour", 0.05, 9000), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)],
+    );
+}
+
+/// A Codex with its week more than half over and nearly all used.
+fn low_codex(engine: &Engine) {
+    engine.note_account_events(Vendor::Codex, vec![plan("pro"), quota("seven_day", 0.95, 3 * DAY)]);
+}
+
+async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> Message {
+    loop {
+        let e = tokio::time::timeout(Duration::from_secs(6), rx.recv()).await.expect("no finished answer in time").expect("engine gone");
+        if let AppEvent::Message { message, .. } = e
+            && message.role == Role::Assistant
+            && message.status != Some(Status::Streaming)
+        {
+            return *message;
+        }
+    }
+}
+
+fn flag(args: &[String], name: &str) -> Option<String> {
+    args.iter().position(|a| a == name).map(|i| args[i + 1].clone())
+}
+
+#[tokio::test]
+async fn a_claude_with_quota_to_spare_buys_one_step_up_on_a_task_that_matters_and_the_answer_says_why() {
+    let dir = tempfile::tempdir().unwrap();
+    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
+    spare_claude(&r.engine);
+    let id = r.engine.send(auto(CODE)).unwrap();
+    let (reply, went) = r.went(&id, 0).await;
+    assert_eq!(went, claude("claude-sonnet-5-5", "high"), "one rung above medium");
+    assert_eq!(reply.reason.as_deref(), Some("reason.route.code.medium"), "the stored reason is unchanged");
+    assert_eq!(reply.pace, Some(PaceNote { kind: "step".into(), vendor: Vendor::Claude }));
+    r.engine.shutdown();
+    let saved: serde_json::Value =
+        serde_json::from_str(&std::fs::read_to_string(dir.path().join("conversations").join(format!("{id}.json"))).unwrap()).unwrap();
+    assert_eq!(saved["messages"][1]["pace"], serde_json::json!({ "kind": "step", "vendor": "claude" }));
+}
+
+#[tokio::test]
+async fn nothing_changes_for_an_easy_chat_or_when_there_is_no_reading() {
+    let dir = tempfile::tempdir().unwrap();
+    let r = both(dir.path(), verdicts(vec![("chat", "easy")]));
+    spare_claude(&r.engine);
+    let id = r.engine.send(auto(SKY)).unwrap();
+    let (reply, went) = r.went(&id, 0).await;
+    assert_eq!((went, reply.pace), (Went::Codex("gpt-6-luna"), None), "easy chat stays on the fast model");
+    r.engine.shutdown();
+
+    let dir = tempfile::tempdir().unwrap();
+    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
+    let id = r.engine.send(auto(CODE)).unwrap();
+    let (reply, went) = r.went(&id, 0).await;
+    assert_eq!((went, reply.pace), (Went::Codex("gpt-6.1-sol"), None), "no reading, no news: today's pick");
+    assert_eq!(r.codex.wire.lines_with("\"effort\":\"low\""), 1);
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_reading_that_may_be_wrong_gives_no_discount() {
+    // Each case sets up the same spare Claude and then spoils the reading in one way; the pick stays today's.
+    type Spoil = fn(&Engine, &Time);
+    let cases: [(&str, Spoil); 3] = [
+        ("older than six hours", |e, t| {
+            spare_claude(e);
+            t.store(WHEN + 7 * HOUR, Ordering::SeqCst);
+            // The plan is seen again, so that only the age of the reading is wrong.
+            e.note_account_events(Vendor::Claude, vec![plan("max")]);
+        }),
+        ("use from outside just jumped", |e, t| {
+            t.store(WHEN - 2 * HOUR, Ordering::SeqCst);
+            e.note_account_events(Vendor::Claude, vec![quota("five_hour", 0.10, 4 * HOUR), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)]);
+            t.store(WHEN, Ordering::SeqCst);
+            e.note_account_events(
+                Vendor::Claude,
+                vec![plan("max"), quota("five_hour", 0.16, 4 * HOUR), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)],
+            );
+        }),
+        ("the plan was last seen more than half an hour ago", |e, t| {
+            spare_claude(e);
+            t.store(WHEN + 31 * 60, Ordering::SeqCst);
+            e.note_account_events(Vendor::Claude, vec![quota("five_hour", 0.05, 9000), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)]);
+        }),
+    ];
+    for (name, spoil) in cases {
+        let dir = tempfile::tempdir().unwrap();
+        let r = both(dir.path(), verdicts(vec![("code", "medium")]));
+        spoil(&r.engine, &r.time);
+        let id = r.engine.send(auto(CODE)).unwrap();
+        let (reply, went) = r.went(&id, 0).await;
+        assert_eq!((went, reply.pace), (Went::Codex("gpt-6.1-sol"), None), "{name}");
+        r.engine.shutdown();
+    }
+}
+
+#[tokio::test]
+async fn a_clock_set_backwards_does_not_make_an_old_reading_look_new() {
+    let dir = tempfile::tempdir().unwrap();
+    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
+    spare_claude(&r.engine);
+    // The PC's clock is now an hour before the moment the readings were taken.
+    r.time.store(WHEN - HOUR, Ordering::SeqCst);
+    r.engine.note_account_events(Vendor::Claude, vec![plan("max")]);
+    let id = r.engine.send(auto(CODE)).unwrap();
+    let (reply, went) = r.went(&id, 0).await;
+    assert_eq!((went, reply.pace), (Went::Codex("gpt-6.1-sol"), None));
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_vendor_running_low_is_left_for_the_other_and_the_answer_says_so() {
+    let dir = tempfile::tempdir().unwrap();
+    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
+    low_codex(&r.engine);
+    let id = r.engine.send(auto(CODE)).unwrap();
+    let (reply, went) = r.went(&id, 0).await;
+    assert_eq!(went, claude("claude-sonnet-5-5", "medium"));
+    assert_eq!(reply.pace, Some(PaceNote { kind: "low".into(), vendor: Vendor::Codex }));
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_vendor_that_runs_out_between_the_question_and_the_dispatch_is_not_used() {
+    let dir = tempfile::tempdir().unwrap();
+    let slow: Ask = Arc::new(|_| {
+        Box::pin(async {
+            tokio::time::sleep(Duration::from_millis(250)).await;
+            Some(r#"{"kind":"code","difficulty":"medium","confidence":0.95}"#.to_string())
+        })
+    });
+    let r = both(dir.path(), slow);
+    spare_claude(&r.engine);
+    let id = r.engine.send(auto(CODE)).unwrap();
+    tokio::time::sleep(Duration::from_millis(60)).await;
+    // While the router thinks, the Claude week runs out.
+    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 1.0, 2 * DAY)]);
+    let (_, went) = r.went(&id, 0).await;
+    assert_eq!(went, Went::Codex("gpt-6.1-sol"));
+    assert!(r.claude.args.lock().unwrap().is_empty(), "no Claude process was started for it");
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_family_that_is_out_leaves_the_rest_of_the_vendor_alone() {
+    let dir = tempfile::tempdir().unwrap();
+    let r = both(dir.path(), verdicts(vec![("code", "hard")]));
+    spare_claude(&r.engine);
+    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day_opus", 1.0, 2 * DAY)]);
+    let id = r.engine.send(auto("Redesign the locking in the scheduler")).unwrap();
+    let (_, went) = r.went(&id, 0).await;
+    assert_ne!(went, claude("claude-opus-5-5", "medium"), "only the Opus profiles are out");
+    assert!(!matches!(&went, Went::Claude(model, _) if model.contains("opus")));
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_login_billed_by_use_is_never_picked_by_auto_but_the_user_can_pick_it() {
+    let dir = tempfile::tempdir().unwrap();
+    let mut r = both(dir.path(), verdicts(vec![("code", "hard")]));
+    spare_claude(&r.engine);
+    r.engine.note_account_events(Vendor::Claude, vec![billed_by_use()]);
+    let id = r.engine.send(auto("Redesign the locking in the scheduler")).unwrap();
+    let (reply, went) = r.went(&id, 0).await;
+    assert_eq!((reply.vendor, went), (Some(Vendor::Codex), Went::Codex("gpt-6.1-sol")));
+    assert!(r.claude.args.lock().unwrap().is_empty());
+
+    let by_hand = SendRequest { choice: Choice { vendor: "claude".into(), model: Some("haiku".into()) }, ..auto(SKY) };
+    r.engine.send(by_hand).unwrap();
+    let reply = finished(&mut r.rx).await;
+    assert_eq!((reply.vendor, reply.status), (Some(Vendor::Claude), Some(Status::Done)), "the user's own choice stands");
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn with_only_a_login_billed_by_use_auto_ends_with_an_error_and_starts_nothing() {
+    let dir = tempfile::tempdir().unwrap();
+    let mut r = rig(dir.path(), &[Vendor::Claude], verdicts(vec![("chat", "easy")]));
+    r.engine.note_account_events(Vendor::Claude, vec![billed_by_use()]);
+    r.engine.send(auto(SKY)).unwrap();
+    let reply = finished(&mut r.rx).await;
+    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.billedPerUse")));
+    assert!(r.claude.args.lock().unwrap().is_empty());
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn an_easy_start_still_moves_up_after_a_turn_that_carried_a_quota_reason() {
+    let dir = tempfile::tempdir().unwrap();
+    let mut r = both(dir.path(), verdicts(vec![("chat", "easy"), ("code", "hard")]));
+    low_codex(&r.engine);
+    let id = r.engine.send(auto(SKY)).unwrap();
+    let first = finished(&mut r.rx).await;
+    assert!(r.claude.args.lock().unwrap()[0].iter().any(|a| a == "claude-haiku-5-5"));
+    assert_eq!(first.reason.as_deref(), Some("reason.route.chat.easy"));
+    assert_eq!(first.pace, Some(PaceNote { kind: "low".into(), vendor: Vendor::Codex }));
+    r.engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("Now refactor my billing service") }).unwrap();
+    let (second, went) = r.went(&id, 1).await;
+    assert_eq!(second.reason.as_deref(), Some("reason.route.code.hard"));
+    assert!(matches!(&went, Went::Claude(model, _) if model != "claude-haiku-5-5"), "{went:?}");
+    r.engine.shutdown();
+}
+
+#[tokio::test]
+async fn a_conversation_file_from_before_pace_existed_opens() {
+    let dir = tempfile::tempdir().unwrap();
+    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
+    spare_claude(&r.engine);
+    let id = r.engine.send(auto(CODE)).unwrap();
+    r.went(&id, 0).await;
+    r.engine.shutdown();
+    let file = dir.path().join("conversations").join(format!("{id}.json"));
+    let mut saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
+    saved["messages"][1].as_object_mut().unwrap().remove("pace");
+    std::fs::write(&file, saved.to_string()).unwrap();
+    let again = both(dir.path(), verdicts(vec![("code", "medium")]));
+    assert_eq!(again.engine.open(&id).unwrap().messages[1].pace, None);
+    again.engine.shutdown();
+}
```

```diff
--- a/engine/tests/route.rs
+++ b/engine/tests/route.rs
@@ -1,5 +1,5 @@
 use faber_engine::model::Vendor;
-use faber_engine::route::{INSTRUCTIONS, Profile, candidates, choose, priced, question, read};
+use faber_engine::route::{INSTRUCTIONS, Profile, candidates, choose, question, read};
 use faber_engine::router::Pick;
 
 const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];
@@ -47,14 +47,6 @@
 }
 
 #[test]
-fn a_task_moves_to_the_other_subscription_when_little_quota_is_left() {
-    let codex_nearly_out = priced(&all(), &|v| if v == Vendor::Codex { 0.9 } else { 0.0 });
-    assert_eq!(picked("chat", "easy", 0.9, &codex_nearly_out), ("claude-haiku-5-5", "medium"));
-    let luna = priced(&candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]), &|_| 0.5);
-    assert!((luna[0].cost - 0.08).abs() < 1e-6);
-}
-
-#[test]
 fn with_one_vendor_the_table_stays_inside_it() {
     let claude = candidates(&[Vendor::Claude], &[]);
     assert_eq!(picked("code", "hard", 0.9, &claude), ("claude-sonnet-5-5", "high"));
@@ -82,8 +74,13 @@
 #[test]
 fn a_conversation_under_way_never_moves_down() {
     let codex = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]);
-    let current =
-        |model: &str, effort: &str| Pick { vendor: Vendor::Codex, model: Some(model.into()), effort: effort.into(), reason: String::new() };
+    let current = |model: &str, effort: &str| Pick {
+        vendor: Vendor::Codex,
+        model: Some(model.into()),
+        effort: effort.into(),
+        reason: String::new(),
+        pace: None,
+    };
     let easy_chat = choose("chat", "easy", 0.9, &codex).unwrap();
     assert!(easy_chat.weaker_than(&current("gpt-6.1-sol", "low")));
     assert!(!easy_chat.weaker_than(&current("gpt-6-luna", "medium")));
```

```diff
--- a/engine/tests/route_pacing.rs
+++ b/engine/tests/route_pacing.rs
@@ -0,0 +1,158 @@
+use faber_engine::model::Vendor;
+use faber_engine::route::{Asked, Factor, PROFILES, Profile, asked, candidates, choose, choose_paced};
+
+const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];
+
+fn all() -> Vec<Profile> {
+    candidates(&BOTH, &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()])
+}
+
+/// Every profile of a vendor gets that vendor's multiplier; `step_ok` as given for Claude.
+fn by_vendor(claude: f64, codex: f64) -> impl Fn(&Profile) -> Factor {
+    move |p| Factor { m: if p.vendor == Vendor::Claude { claude } else { codex }, w: 1.0, step_ok: true }
+}
+
+fn paced(
+    kind: &'static str,
+    level: &'static str,
+    from: &[Profile],
+    f: &dyn Fn(&Profile) -> Factor,
+) -> (&'static str, &'static str, Option<(String, Vendor)>) {
+    let r = choose_paced(kind, level, 0.9, from, f).unwrap();
+    (r.profile.model, r.profile.effort, r.pace.map(|n| (n.kind, n.vendor)))
+}
+
+fn pick(kind: &'static str, level: &'static str, from: &[Profile], f: &dyn Fn(&Profile) -> Factor) -> (&'static str, &'static str) {
+    let (m, e, _) = paced(kind, level, from, f);
+    (m, e)
+}
+
+const SONNET_HIGH: (&str, &str) = ("claude-sonnet-5-5", "high");
+const SONNET_MEDIUM: (&str, &str) = ("claude-sonnet-5-5", "medium");
+const OPUS_MEDIUM: (&str, &str) = ("claude-opus-5-5", "medium");
+const SOL_LOW: (&str, &str) = ("gpt-6.1-sol", "low");
+const SOL_MEDIUM: (&str, &str) = ("gpt-6.1-sol", "medium");
+const SOL_HIGH: (&str, &str) = ("gpt-6.1-sol", "high");
+
+#[test]
+fn medium_code_steps_up_on_spare_claude_until_codex_is_cheaper() {
+    let all = all();
+    assert_eq!(pick("code", "medium", &all, &by_vendor(0.14, 1.0)), SONNET_HIGH);
+    assert_eq!(pick("code", "medium", &all, &by_vendor(0.36, 1.0)), SONNET_HIGH);
+    assert_eq!(pick("code", "medium", &all, &by_vendor(0.5625, 1.0)), SOL_LOW);
+    // One day of the week left: Claude at the floor, Codex a little over the line.
+    assert_eq!(pick("code", "medium", &all, &by_vendor(0.1, 1.29)), SONNET_HIGH);
+}
+
+#[test]
+fn hard_code_steps_to_the_expert_bar() {
+    let all = all();
+    assert_eq!(pick("code", "hard", &all, &by_vendor(0.14, 1.0)), OPUS_MEDIUM);
+    assert_eq!(pick("code", "hard", &all, &by_vendor(0.30, 1.0)), OPUS_MEDIUM);
+    assert_eq!(pick("code", "hard", &all, &by_vendor(0.44, 1.0)), SOL_MEDIUM);
+    assert_eq!(pick("code", "hard", &all, &by_vendor(0.14, 0.14)), SOL_HIGH, "64.8 s is within the 180 s cap of hard code");
+}
+
+#[test]
+fn medium_math_steps_to_the_expert_bar() {
+    assert_eq!(pick("math", "medium", &all(), &by_vendor(0.18, 1.0)), OPUS_MEDIUM);
+}
+
+#[test]
+fn a_task_that_does_not_matter_never_gets_a_discount() {
+    assert_eq!(pick("chat", "easy", &all(), &by_vendor(0.14, 1.0)), ("gpt-6-luna", "medium"));
+}
+
+#[test]
+fn a_vendor_running_low_is_priced_up_and_the_answer_says_so() {
+    let (m, e, pace) = paced("code", "medium", &all(), &by_vendor(1.0, 25.0));
+    assert_eq!((m, e), SONNET_MEDIUM);
+    assert_eq!(pace, Some(("low".to_string(), Vendor::Codex)));
+    let (m, _, pace) = paced("chat", "easy", &all(), &by_vendor(1.0, 25.0));
+    assert_eq!(m, "claude-haiku-5-5");
+    assert_eq!(pace, Some(("low".to_string(), Vendor::Codex)));
+}
+
+#[test]
+fn a_step_over_the_time_cap_is_not_taken() {
+    let codex = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]);
+    // Hard research caps at 60 s: Sol high (64.8 s) is no step, so Sol medium wins on price.
+    assert_eq!(pick("research", "hard", &codex, &by_vendor(1.0, 0.14)), SOL_MEDIUM);
+    let claude = candidates(&[Vendor::Claude], &[]);
+    assert_eq!(pick("research", "hard", &claude, &by_vendor(0.14, 1.0)), OPUS_MEDIUM);
+}
+
+#[test]
+fn a_vendor_with_a_window_above_ninety_percent_gets_no_step() {
+    let no_step =
+        |p: &Profile| Factor { m: if p.vendor == Vendor::Claude { 0.1 } else { 1.0 }, w: 1.0, step_ok: p.vendor != Vendor::Claude };
+    assert_eq!(pick("code", "hard", &all(), &no_step), SONNET_HIGH);
+}
+
+#[test]
+fn learned_weights_change_the_pick_but_never_show_a_reason() {
+    let weighted = |p: &Profile| Factor { m: 1.0, w: if p.vendor == Vendor::Claude { 0.5 } else { 2.0 }, step_ok: true };
+    let (m, e, pace) = paced("code", "medium", &all(), &weighted);
+    assert_eq!((m, e), SONNET_MEDIUM);
+    assert_eq!(pace, None);
+}
+
+#[test]
+fn a_step_up_names_the_vendor_that_has_quota_to_spare() {
+    let (_, _, pace) = paced("code", "hard", &all(), &by_vendor(0.14, 1.0));
+    assert_eq!(pace, Some(("step".to_string(), Vendor::Claude)));
+}
+
+#[test]
+fn with_neutral_factors_the_picks_are_exactly_the_unpaced_table() {
+    let sets: Vec<Vec<Profile>> = vec![
+        all(),
+        candidates(&[Vendor::Claude], &[]),
+        candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]),
+        candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]),
+        candidates(&BOTH, &["gpt-6.1-sol".to_string()]),
+    ];
+    for set in &sets {
+        for kind in ["chat", "writing", "code", "math", "research", "image", "other"] {
+            for level in ["easy", "medium", "hard"] {
+                for confidence in [0.2, 0.9] {
+                    let plain = choose(kind, level, confidence, set).unwrap();
+                    let neutral = choose_paced(kind, level, confidence, set, &|_| Factor::NEUTRAL).unwrap();
+                    assert_eq!(plain.profile, neutral.profile, "{kind} {level} {confidence}");
+                    assert_eq!(neutral.pace, None);
+                }
+            }
+        }
+    }
+}
+
+#[test]
+fn the_best_price_never_rises_when_a_vendor_gets_cheaper() {
+    let all = all();
+    let steps = [25.0, 12.0, 6.0, 3.0, 1.5, 1.0, 0.8, 0.6, 0.45, 0.3, 0.2, 0.14, 0.1];
+    for (kind, level) in
+        [("code", "medium"), ("code", "hard"), ("math", "medium"), ("research", "hard"), ("writing", "medium"), ("chat", "easy")]
+    {
+        for codex in [0.1, 0.5, 1.0, 4.0] {
+            let mut last = f64::INFINITY;
+            for claude in steps {
+                let price = choose_paced(kind, level, 0.9, &all, &by_vendor(claude, codex)).unwrap().price;
+                assert!(price <= last + 1e-12, "{kind} {level}: price rose from {last} to {price} at m {claude} (codex {codex})");
+                last = price;
+            }
+        }
+    }
+}
+
+#[test]
+fn the_router_line_is_read_without_the_table() {
+    let a = asked("Sure.\n{\"kind\":\"code\",\"difficulty\":\"hard\",\"confidence\":0.8}").unwrap();
+    assert_eq!(a, Asked { kind: "code", difficulty: "hard", confidence: 0.8 });
+    assert_eq!(asked("no verdict"), None);
+}
+
+#[test]
+fn luna_at_medium_is_thirty() {
+    let luna = PROFILES.iter().find(|p| p.model == "gpt-6-luna").unwrap();
+    assert_eq!(luna.iq, 30);
+}
```

- [ ] **Step 2: Run the new tests and see them fail**

Run: `cargo test -p faber-engine` (the named test files)
Expected: compile errors or failures naming the missing items.

- [ ] **Step 3: Implement**

`engine/src/engine.rs`

```diff
--- a/engine/src/engine.rs
+++ b/engine/src/engine.rs
@@ -1,7 +1,7 @@
 //! The engine: conversations, live CLI sessions, permissions, branches and attachments.
 
 use crate::account::{self, AccountStatus};
-use crate::budget::Billing;
+use crate::budget::{Billing, Purpose, Why, family_of};
 use crate::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
 use crate::cli::Cli;
 use crate::codex::read_codex_limits;
@@ -901,8 +901,23 @@
         server
     }
 
+    /// The gate before a user turn (3.4): the everyday default and the profile of an existing conversation are checked here
+    /// too, not only what the table picked. A vendor the user picked by hand is allowed even when it is billed by use.
+    fn gate_turn(&self, pick: &Pick) -> Result<(), String> {
+        let family = pick.model.as_deref().and_then(family_of);
+        let by_hand = pick.reason == "reason.chosen";
+        self.allows(pick.vendor, family, Purpose::UserTurn { by_hand }, false).map_err(|why| {
+            match why {
+                Why::Billing => "error.billedPerUse",
+                Why::Exhausted | Why::Window | Why::Paid => "error.quotaOut",
+            }
+            .to_string()
+        })
+    }
+
     /// Hands a turn to the conversation's CLI, starting the CLI if needed.
     fn start(&mut self, id: &str, reply_id: &str, pick: &Pick, text: String, images: Vec<Image>) -> Result<(), String> {
+        self.gate_turn(pick)?;
         self.launch(id, pick)?;
         if let Some(running) = self.live.get(id).and_then(|l| l.running.clone())
             && let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id))
@@ -1102,11 +1117,13 @@
             self.push(&id, Some(&reply_id), true);
             let (me, conv, images) = (self.me.clone(), id.clone(), req.images);
             tokio::spawn(async move {
-                let routed = tokio::time::timeout(auto::ROUTE_BUDGET, route).await.ok().and_then(|r| r.ok()).flatten();
-                // A conversation under way only moves up.
-                let routed = routed.filter(|r| before.as_ref().is_none_or(|p| !r.weaker_than(p)));
+                let verdict = tokio::time::timeout(auto::ROUTE_BUDGET, route).await.ok().and_then(|r| r.ok()).flatten();
                 if let Some(core) = me.upgrade() {
-                    core.lock().unwrap_or_else(|e| e.into_inner()).after_route(&conv, &reply_id, routed, pick, text, images);
+                    let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
+                    // The table runs now, on the quota of this moment, not on the quota the user saw while typing. A
+                    // conversation under way only moves up.
+                    let routed = verdict.and_then(|v| core.table(&v, only)).filter(|r| before.as_ref().is_none_or(|p| !r.weaker_than(p)));
+                    core.after_route(&conv, &reply_id, routed, pick, text, images);
                 }
             });
             return Ok(id);
@@ -1271,7 +1288,7 @@
     let vendor = last.vendor?;
     let effort = live.filter(|l| l.vendor == vendor).map_or_else(|| "medium".to_string(), |l| l.effort.clone());
     let reason = last.reason.clone().filter(|r| r != "reason.routing").unwrap_or_else(|| "reason.autoDefault".into());
-    Some(Pick { vendor, model: last.model.clone(), effort, reason })
+    Some(Pick { vendor, model: last.model.clone(), effort, reason, pace: None })
 }
 
 /// The row of the running list for a conversation, if its latest reply is streaming or has unanswered permissions.
```

`engine/src/engine/auto.rs`

```diff
--- a/engine/src/engine/auto.rs
+++ b/engine/src/engine/auto.rs
@@ -2,7 +2,8 @@
 //! moment it is sent. Once a message needed more than an easy model, the conversation stays with it.
 
 use super::*;
-use crate::route::{INSTRUCTIONS, ROUTER, Routed, candidates, priced, question, read};
+use crate::budget::{Purpose, Shape, Snap, Why, family_of, gate};
+use crate::route::{Asked, Factor, INSTRUCTIONS, Profile, ROUTER, Routed, asked, candidates, choose_paced, question};
 use tokio::sync::oneshot;
 
 /// How long a sent message may wait for its route before the everyday default answers: a timeout, not a target (Erick,
@@ -16,8 +17,10 @@
 /// A route for a request, ready or on its way.
 pub(super) struct Guess {
     request: String,
-    result: Option<Option<Routed>>,
-    waiters: Vec<oneshot::Sender<Option<Routed>>>,
+    /// What the small model said about the request (kind, level, confidence). The table runs when the message is sent, on the
+    /// quota of that moment, so only this is kept.
+    result: Option<Option<Asked>>,
+    waiters: Vec<oneshot::Sender<Option<Asked>>>,
     /// The new conversation whose answering CLI starts as soon as the route is known.
     warm: Option<String>,
     /// A conversation under way is routed inside its own subscription, where the CLI keeps the history.
@@ -63,6 +66,34 @@
         (before.reason.starts_with("reason.route.") && before.reason.ends_with(".easy")).then_some(Some(before))
     }
 
+    /// The one door before a request reaches a CLI (3.4): may this vendor and family be asked for this purpose now.
+    pub(super) fn allows(&self, vendor: Vendor, family: Option<&str>, purpose: Purpose, approved: bool) -> Result<(), Why> {
+        let now = self.secs();
+        gate(&self.ledger.of(vendor).snap(now), family, purpose, approved, now)
+    }
+
+    /// The table's pick for the router's verdict, on the quota as it is now. Profiles the gate would refuse are not in the
+    /// pool; with every multiplier and weight at 1 the pick is exactly the table without quota (`choose`).
+    pub(super) fn table(&self, verdict: &Asked, only: Option<Vendor>) -> Option<Routed> {
+        let now = self.secs();
+        let vendors: Vec<Vendor> = self.vendors().into_iter().filter(|v| only.is_none_or(|o| o == *v)).collect();
+        let usable: Vec<Profile> = candidates(&vendors, &self.codex_models)
+            .into_iter()
+            .filter(|p| self.allows(p.vendor, family_of(p.model), Purpose::UserTurn { by_hand: false }, false).is_ok())
+            .collect();
+        let known: BTreeMap<Vendor, (Snap, Shape)> =
+            vendors.iter().map(|v| (*v, (self.ledger.of(*v).snap(now), self.ledger.of(*v).shape()))).collect();
+        let factor = |p: &Profile| {
+            let (snap, shape) = &known[&p.vendor];
+            Factor {
+                m: snap.profile_m(family_of(p.model), shape, self.utc_offset, now),
+                w: self.ledger.of(p.vendor).weights.w.get(p.model).copied().unwrap_or(1.0),
+                step_ok: snap.step_ok(now),
+            }
+        };
+        choose_paced(verdict.kind, verdict.difficulty, verdict.confidence, &usable, &factor)
+    }
+
     /// While the user types in Auto.
     pub(super) fn route_draft(&mut self, req: WarmRequest, text: String) {
         let text = text.trim().to_string();
@@ -82,10 +113,10 @@
     }
 
     /// The route for a sent message: the draft's while it still holds, or a new one.
-    pub(super) fn route_for(&mut self, request: &str, attached: usize, only: Option<Vendor>) -> oneshot::Receiver<Option<Routed>> {
+    pub(super) fn route_for(&mut self, request: &str, attached: usize, only: Option<Vendor>) -> oneshot::Receiver<Option<Asked>> {
         let (tx, rx) = oneshot::channel();
         match self.guess.as_mut().filter(|g| attached == 0 && g.only == only && covers(&g.request, request)) {
-            Some(Guess { result: Some(routed), .. }) => drop(tx.send(routed.clone())),
+            Some(Guess { result: Some(routed), .. }) => drop(tx.send(*routed)),
             Some(g) => g.waiters.push(tx),
             None => {
                 self.start_guess(request.trim().to_string(), attached, None, only);
@@ -98,14 +129,16 @@
     }
 
     /// The router answered (or gave up): the waiting message starts, and a routed draft gets its CLI ready.
-    fn guessed(&mut self, request: &str, only: Option<Vendor>, routed: Option<Routed>) {
+    fn guessed(&mut self, request: &str, only: Option<Vendor>, verdict: Option<Asked>) {
         let Some(g) = self.guess.as_mut().filter(|g| g.request == request && g.only == only && g.result.is_none()) else { return };
-        g.result = Some(routed.clone());
+        g.result = Some(verdict);
         let (waiters, warm) = (std::mem::take(&mut g.waiters), g.warm.take());
         for waiter in waiters {
-            let _ = waiter.send(routed.clone());
+            let _ = waiter.send(verdict);
         }
-        if let (Some(r), Some(id)) = (&routed, warm)
+        // A new conversation's CLI gets ready for the pick of this moment; the pick at dispatch may differ, and then the
+        // process started here is replaced.
+        if let (Some(r), Some(id)) = (verdict.and_then(|v| self.table(&v, only)), warm)
             && self.convs.get(&id).is_some_and(|c| c.messages.is_empty())
         {
             let _ = self.launch(&id, &r.pick());
@@ -118,9 +151,8 @@
     }
 
     fn start_guess(&mut self, request: String, attached: usize, warm: Option<String>, only: Option<Vendor>) {
-        let used = |vendor: Vendor| self.quotas.iter().filter(|((v, _), _)| *v == vendor).map(|(_, q)| q.used).fold(0.0, f64::max);
         let vendors: Vec<Vendor> = self.vendors().into_iter().filter(|v| only.is_none_or(|o| o == *v)).collect();
-        let cands = priced(&candidates(&vendors, &self.codex_models), &used);
+        let cands = candidates(&vendors, &self.codex_models);
         // One candidate needs no router: the everyday default answers.
         let (answer, stop) = if cands.len() > 1 { self.ask_router(question(&request, attached)) } else { (None, None) };
         if let Some(old) = self.guess.replace(Guess { request: request.clone(), result: None, waiters: vec![], warm, only, stop }) {
@@ -129,7 +161,7 @@
         let me = self.me.clone();
         tokio::spawn(async move {
             let routed = match answer {
-                Some(rx) => tokio::time::timeout(ASK_LIMIT, rx).await.ok().and_then(|r| r.ok()).flatten().and_then(|a| read(&a, &cands)),
+                Some(rx) => tokio::time::timeout(ASK_LIMIT, rx).await.ok().and_then(|r| r.ok()).flatten().and_then(|a| asked(&a)),
                 None => None,
             };
             if let Some(core) = me.upgrade() {
@@ -158,7 +190,9 @@
     /// A router session waiting for its question: a thread on the running Codex process, or a Claude process of its own.
     fn open_router(&mut self) -> Option<Spare> {
         let router = ROUTER.into_iter().find(|(vendor, model, _)| {
-            self.opts.clis.contains_key(vendor) && (*vendor != Vendor::Codex || self.codex_models.iter().any(|m| m == model))
+            self.opts.clis.contains_key(vendor)
+                && (*vendor != Vendor::Codex || self.codex_models.iter().any(|m| m == model))
+                && self.allows(*vendor, family_of(model), Purpose::Router, false).is_ok()
         });
         let ((vendor, model, effort), cli) = router.and_then(|r| Some((r, self.opts.clis.get(&r.0)?.clone())))?;
         let (tx, answer) = oneshot::channel();
@@ -236,6 +270,7 @@
             m.vendor = Some(pick.vendor);
             m.model = pick.model.clone();
             m.reason = Some(pick.reason.clone());
+            m.pace = pick.pace.clone();
         }
         self.save(id);
         if let Err(error) = self.start(id, reply_id, &pick, text, images) {
```

`engine/src/model.rs`

```diff
--- a/engine/src/model.rs
+++ b/engine/src/model.rs
@@ -129,6 +129,9 @@
     /// True when the token numbers cover only part of the turn (sub-model work, or no usage was seen).
     #[serde(default, skip_serializing_if = "Option::is_none")]
     pub incomplete: Option<bool>,
+    /// Why the quota changed the model Auto chose, when it did.
+    #[serde(default, skip_serializing_if = "Option::is_none")]
+    pub pace: Option<PaceNote>,
 }
 
 impl Message {
@@ -156,6 +159,7 @@
             cache_write_tokens: None,
             cache_write_1h_tokens: None,
             incomplete: None,
+            pace: None,
         }
     }
 }
@@ -282,6 +286,14 @@
     pub resets_at: f64,
 }
 
+/// Why the quota changed the pick of Auto, shown under the model name.
+#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
+pub struct PaceNote {
+    /// `step` (one rung up, bought with quota that would expire) or `low` (moved away from a vendor that is running low).
+    pub kind: String,
+    pub vendor: Vendor,
+}
+
 /// The user's choices, in one plain file. Appearance belongs to the screen, so the engine only keeps it.
 #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
 #[serde(rename_all = "camelCase")]
```

`engine/src/route.rs`

```diff
--- a/engine/src/route.rs
+++ b/engine/src/route.rs
@@ -1,166 +1,279 @@
-//! Auto: a small model that is already running reads the request and says what kind of task it is and how hard; a fixed
-//! table then picks the cheapest model that clears the bar for it, priced by the quota left on each subscription.
-//!
-//! The router never sees a model or company name, not even as anonymous candidates: each vendor's models tend to prefer
-//! that vendor's models, and on 44 real requests (10/10/2026) a router choosing among anonymous profiles still overspent
-//! on 5 of them. It judges requests well; weighing price against quality is the table's job.
-
-use crate::model::Vendor;
-use crate::router::Pick;
-use serde::Deserialize;
-
-/// One way to run a model. Numbers from Artificial Analysis (artificialanalysis.ai/models/<model>-<effort>), read
-/// between 22/09 and 08/10/2026: `iq` is the Intelligence Index, `agent` Terminal-Bench (multi-step coding and terminal
-/// work, %), `secs` a typical whole answer, `cost` US$ to run the index, which is how much a task weighs on the quota.
-#[derive(Debug, Clone, Copy, PartialEq)]
-pub struct Profile {
-    pub vendor: Vendor,
-    pub model: &'static str,
-    pub effort: &'static str,
-    pub iq: u8,
-    pub agent: u8,
-    pub secs: f32,
-    pub cost: f32,
-}
-
-const fn p(vendor: Vendor, model: &'static str, effort: &'static str, iq: u8, agent: u8, secs: f32, cost: f32) -> Profile {
-    Profile { vendor, model, effort, iq, agent, secs, cost }
-}
-
-// No maximum effort (minutes per answer at two to three times the cost) and no model another one here beats on every
-// number. ponytail: compiled in; it moves to the live manifest (phase F11) so a new model joins without a release.
-pub const PROFILES: [Profile; 9] = [
-    p(Vendor::Claude, "claude-opus-5-5", "high", 54, 57, 44.2, 1.82),
-    p(Vendor::Claude, "claude-opus-5-5", "medium", 51, 53, 27.9, 1.34),
-    p(Vendor::Claude, "claude-sonnet-5-5", "high", 47, 44, 17.5, 0.88),
-    p(Vendor::Claude, "claude-sonnet-5-5", "medium", 41, 30, 7.0, 0.48),
-    p(Vendor::Claude, "claude-haiku-5-5", "medium", 34, 15, 16.6, 0.05),
-    p(Vendor::Codex, "gpt-6.1-sol", "high", 50, 52, 64.8, 0.32),
-    p(Vendor::Codex, "gpt-6.1-sol", "medium", 48, 48, 13.4, 0.21),
-    p(Vendor::Codex, "gpt-6.1-sol", "low", 42, 31, 8.6, 0.13),
-    p(Vendor::Codex, "gpt-6-luna", "medium", 29, 3, 8.8, 0.02),
-];
-
-/// The router itself: the fastest cheap model of each vendor.
-pub const ROUTER: [(Vendor, &str, &str); 2] = [(Vendor::Codex, "gpt-6-luna", "medium"), (Vendor::Claude, "claude-haiku-5-5", "medium")];
-
-pub const KINDS: [&str; 7] = ["chat", "writing", "code", "math", "research", "image", "other"];
-pub const LEVELS: [&str; 3] = ["easy", "medium", "hard"];
-
-pub const INSTRUCTIONS: &str = "You classify a user's request for a router. Never answer the request itself.
-kind: chat (casual talk, greetings, short facts), writing (texts, emails, translation, stories, plans), code (programs, \
-debugging, code review, changes to software), math (calculations, equations, proofs), research (analysis, comparisons, \
-explanations, advice), image (making or editing pictures), other.
-difficulty: easy (a quick answer any competent person gets right), medium (needs care, several steps or some \
-expertise), hard (expert work where a subtle mistake is costly: proofs, multi-file changes, concurrency, architecture, \
-legal, medical or financial analysis).
-Reply with one line of JSON only: \
-{\"kind\":\"chat|writing|code|math|research|image|other\",\"difficulty\":\"easy|medium|hard\",\"confidence\":<0..1>}";
-
-/// The profiles this user can run: a connected CLI and, for Codex, a model the account lists.
-pub fn candidates(available: &[Vendor], codex_models: &[String]) -> Vec<Profile> {
-    PROFILES
-        .into_iter()
-        .filter(|p| available.contains(&p.vendor) && (p.vendor != Vendor::Codex || codex_models.iter().any(|m| m == p.model)))
-        .collect()
-}
-
-/// `used` is the share of each vendor's quota already spent (0 to 1): a task costs more where little is left.
-pub fn priced(candidates: &[Profile], used: &dyn Fn(Vendor) -> f64) -> Vec<Profile> {
-    candidates
-        .iter()
-        .map(|p| {
-            let left = (1.0 - used(p.vendor).clamp(0.0, 0.95)) as f32;
-            Profile { cost: p.cost / (left * left), ..*p }
-        })
-        .collect()
-}
-
-// Requests longer than this are cut for routing only; the answer gets the whole text.
-const REQUEST_CHARS: usize = 4000;
-
-/// What the router reads: the request alone.
-pub fn question(request: &str, attached: usize) -> String {
-    let mut text: String = request.chars().take(REQUEST_CHARS).collect();
-    if attached > 0 {
-        text.push_str(&format!("\n[{attached} attached file(s) or image(s)]"));
-    }
-    text
-}
-
-/// The router's verdict and the model the table picked for it.
-#[derive(Debug, Clone, PartialEq)]
-pub struct Routed {
-    pub profile: Profile,
-    pub kind: &'static str,
-    pub difficulty: &'static str,
-    pub confidence: f64,
-}
-
-impl Routed {
-    pub fn pick(&self) -> Pick {
-        Pick {
-            vendor: self.profile.vendor,
-            model: Some(self.profile.model.into()),
-            effort: self.profile.effort.into(),
-            reason: format!("reason.route.{}.{}", self.kind, self.difficulty),
-        }
-    }
-
-    /// True when the picked model is weaker than the one a conversation already has.
-    pub fn weaker_than(&self, current: &Pick) -> bool {
-        PROFILES
-            .iter()
-            .find(|p| p.vendor == current.vendor && Some(p.model) == current.model.as_deref() && p.effort == current.effort)
-            .is_some_and(|p| (p.iq, p.agent) > (self.profile.iq, self.profile.agent))
-    }
-}
-
-#[derive(Deserialize)]
-struct Answer {
-    #[serde(default)]
-    kind: String,
-    difficulty: String,
-    #[serde(default)]
-    confidence: f64,
-}
-
-/// Reads the router's JSON line; None when it is not one.
-pub fn read(answer: &str, candidates: &[Profile]) -> Option<Routed> {
-    let json = &answer[answer.find('{')?..=answer.rfind('}')?];
-    let a: Answer = serde_json::from_str(json).ok()?;
-    let kind = KINDS.into_iter().find(|k| *k == a.kind).unwrap_or("other");
-    let difficulty = LEVELS.into_iter().find(|l| *l == a.difficulty)?;
-    choose(kind, difficulty, a.confidence.clamp(0.0, 1.0), candidates)
-}
-
-/// The bar a task must clear: (general intelligence, agentic coding).
-fn bar(kind: &str, difficulty: &str) -> (u8, u8) {
-    match (difficulty, kind) {
-        ("easy", "code") => (0, 15),
-        ("easy", _) => (0, 0),
-        ("medium", "code") => (0, 30),
-        // A wrong number reads as right: math gets the hard bar a level early.
-        ("medium", "math") => (47, 0),
-        ("medium", _) => (41, 0),
-        ("hard", "code") => (47, 44),
-        _ => (47, 0),
-    }
-}
-
-/// The cheapest candidate that clears the bar (the faster on a tie), or the strongest when none does. An unsure verdict
-/// gets the next level's bar.
-pub fn choose(kind: &'static str, difficulty: &'static str, confidence: f64, candidates: &[Profile]) -> Option<Routed> {
-    let level = match (confidence < 0.5, difficulty) {
-        (true, "easy") => "medium",
-        (true, _) => "hard",
-        (false, level) => level,
-    };
-    let (iq, agent) = bar(kind, level);
-    let profile = candidates
-        .iter()
-        .filter(|p| p.iq >= iq && p.agent >= agent)
-        .min_by(|a, b| a.cost.total_cmp(&b.cost).then(a.secs.total_cmp(&b.secs)))
-        .or_else(|| candidates.iter().max_by_key(|p| (p.iq, p.agent)))?;
-    Some(Routed { profile: *profile, kind, difficulty, confidence })
-}
+//! Auto: a small model that is already running reads the request and says what kind of task it is and how hard; a fixed
+//! table then picks the cheapest model that clears the bar for it, priced by the quota left on each subscription.
+//!
+//! The router never sees a model or company name, not even as anonymous candidates: each vendor's models tend to prefer
+//! that vendor's models, and on 44 real requests (10/10/2026) a router choosing among anonymous profiles still overspent
+//! on 5 of them. It judges requests well; weighing price against quality is the table's job.
+
+use crate::budget::LOW_M;
+use crate::model::{PaceNote, Vendor};
+use crate::router::Pick;
+use serde::Deserialize;
+
+/// One way to run a model. Numbers from Artificial Analysis (artificialanalysis.ai/models/<model>-<effort>), read
+/// between 22/09 and 08/10/2026: `iq` is the Intelligence Index, `agent` Terminal-Bench (multi-step coding and terminal
+/// work, %), `secs` a typical whole answer, `cost` US$ to run the index, which is how much a task weighs on the quota.
+#[derive(Debug, Clone, Copy, PartialEq)]
+pub struct Profile {
+    pub vendor: Vendor,
+    pub model: &'static str,
+    pub effort: &'static str,
+    pub iq: u8,
+    pub agent: u8,
+    pub secs: f32,
+    pub cost: f32,
+}
+
+const fn p(vendor: Vendor, model: &'static str, effort: &'static str, iq: u8, agent: u8, secs: f32, cost: f32) -> Profile {
+    Profile { vendor, model, effort, iq, agent, secs, cost }
+}
+
+// No maximum effort (minutes per answer at two to three times the cost) and no model another one here beats on every
+// number. ponytail: compiled in; it moves to the live manifest (phase F11) so a new model joins without a release.
+pub const PROFILES: [Profile; 9] = [
+    p(Vendor::Claude, "claude-opus-5-5", "high", 54, 57, 44.2, 1.82),
+    p(Vendor::Claude, "claude-opus-5-5", "medium", 51, 53, 27.9, 1.34),
+    p(Vendor::Claude, "claude-sonnet-5-5", "high", 47, 44, 17.5, 0.88),
+    p(Vendor::Claude, "claude-sonnet-5-5", "medium", 41, 30, 7.0, 0.48),
+    p(Vendor::Claude, "claude-haiku-5-5", "medium", 34, 15, 16.6, 0.05),
+    p(Vendor::Codex, "gpt-6.1-sol", "high", 50, 52, 64.8, 0.32),
+    p(Vendor::Codex, "gpt-6.1-sol", "medium", 48, 48, 13.4, 0.21),
+    p(Vendor::Codex, "gpt-6.1-sol", "low", 42, 31, 8.6, 0.13),
+    p(Vendor::Codex, "gpt-6-luna", "medium", 30, 3, 8.8, 0.02),
+];
+
+/// The router itself: the fastest cheap model of each vendor.
+pub const ROUTER: [(Vendor, &str, &str); 2] = [(Vendor::Codex, "gpt-6-luna", "medium"), (Vendor::Claude, "claude-haiku-5-5", "medium")];
+
+pub const KINDS: [&str; 7] = ["chat", "writing", "code", "math", "research", "image", "other"];
+pub const LEVELS: [&str; 3] = ["easy", "medium", "hard"];
+
+pub const INSTRUCTIONS: &str = "You classify a user's request for a router. Never answer the request itself.
+kind: chat (casual talk, greetings, short facts), writing (texts, emails, translation, stories, plans), code (programs, \
+debugging, code review, changes to software), math (calculations, equations, proofs), research (analysis, comparisons, \
+explanations, advice), image (making or editing pictures), other.
+difficulty: easy (a quick answer any competent person gets right), medium (needs care, several steps or some \
+expertise), hard (expert work where a subtle mistake is costly: proofs, multi-file changes, concurrency, architecture, \
+legal, medical or financial analysis).
+Reply with one line of JSON only: \
+{\"kind\":\"chat|writing|code|math|research|image|other\",\"difficulty\":\"easy|medium|hard\",\"confidence\":<0..1>}";
+
+/// The profiles this user can run: a connected CLI and, for Codex, a model the account lists.
+pub fn candidates(available: &[Vendor], codex_models: &[String]) -> Vec<Profile> {
+    PROFILES
+        .into_iter()
+        .filter(|p| available.contains(&p.vendor) && (p.vendor != Vendor::Codex || codex_models.iter().any(|m| m == p.model)))
+        .collect()
+}
+
+// Requests longer than this are cut for routing only; the answer gets the whole text.
+const REQUEST_CHARS: usize = 4000;
+
+/// What the router reads: the request alone.
+pub fn question(request: &str, attached: usize) -> String {
+    let mut text: String = request.chars().take(REQUEST_CHARS).collect();
+    if attached > 0 {
+        text.push_str(&format!("\n[{attached} attached file(s) or image(s)]"));
+    }
+    text
+}
+
+/// The router's verdict and the model the table picked for it.
+#[derive(Debug, Clone, PartialEq)]
+pub struct Routed {
+    pub profile: Profile,
+    pub kind: &'static str,
+    pub difficulty: &'static str,
+    pub confidence: f64,
+    /// What the table compared for the pick: cost x weight x quota factor. Kept for tests and logs.
+    pub price: f64,
+    /// Set only when the quota changed the pick (see `choose_paced`).
+    pub pace: Option<PaceNote>,
+}
+
+impl Routed {
+    pub fn pick(&self) -> Pick {
+        Pick {
+            vendor: self.profile.vendor,
+            model: Some(self.profile.model.into()),
+            effort: self.profile.effort.into(),
+            reason: format!("reason.route.{}.{}", self.kind, self.difficulty),
+            pace: self.pace.clone(),
+        }
+    }
+
+    /// True when the picked model is weaker than the one a conversation already has.
+    pub fn weaker_than(&self, current: &Pick) -> bool {
+        PROFILES
+            .iter()
+            .find(|p| p.vendor == current.vendor && Some(p.model) == current.model.as_deref() && p.effort == current.effort)
+            .is_some_and(|p| (p.iq, p.agent) > (self.profile.iq, self.profile.agent))
+    }
+}
+
+#[derive(Deserialize)]
+struct Answer {
+    #[serde(default)]
+    kind: String,
+    difficulty: String,
+    #[serde(default)]
+    confidence: f64,
+}
+
+/// What the small model said about a request: the only thing kept from the router while the user types. The table runs
+/// when the message is sent, on the quota of that moment.
+#[derive(Debug, Clone, Copy, PartialEq)]
+pub struct Asked {
+    pub kind: &'static str,
+    pub difficulty: &'static str,
+    pub confidence: f64,
+}
+
+/// Reads the router's JSON line; None when it is not one.
+pub fn asked(answer: &str) -> Option<Asked> {
+    let json = &answer[answer.find('{')?..=answer.rfind('}')?];
+    let a: Answer = serde_json::from_str(json).ok()?;
+    let kind = KINDS.into_iter().find(|k| *k == a.kind).unwrap_or("other");
+    let difficulty = LEVELS.into_iter().find(|l| *l == a.difficulty)?;
+    Some(Asked { kind, difficulty, confidence: a.confidence.clamp(0.0, 1.0) })
+}
+
+/// The router's line and the table's pick with no quota information.
+pub fn read(answer: &str, candidates: &[Profile]) -> Option<Routed> {
+    let a = asked(answer)?;
+    choose(a.kind, a.difficulty, a.confidence, candidates)
+}
+
+/// The bar a task must clear: (general intelligence, agentic coding). `expert` is not a level the router can name; it is
+/// the rung above `hard` that spare quota can buy.
+fn bar(kind: &str, level: &str) -> (u8, u8) {
+    match (level, kind) {
+        ("easy", "code") => (0, 15),
+        ("easy", _) => (0, 0),
+        ("medium", "code") => (0, 30),
+        // A wrong number reads as right: math gets the hard bar a level early.
+        ("medium", "math") => (47, 0),
+        ("medium", _) => (41, 0),
+        ("hard", "code") => (47, 44),
+        ("expert", "code") => (50, 52),
+        ("expert", _) => (50, 0),
+        _ => (47, 0),
+    }
+}
+
+const LADDER: [&str; 4] = ["easy", "medium", "hard", "expert"];
+
+/// The bar of the nearest higher level whose bar is different; a task already at the top keeps its own.
+fn next_bar(kind: &str, level: &str) -> (u8, u8) {
+    let own = bar(kind, level);
+    let at = LADDER.iter().position(|l| *l == level).unwrap_or(2);
+    LADDER[at + 1..].iter().map(|l| bar(kind, l)).find(|b| *b != own).unwrap_or(own)
+}
+
+/// Whole-answer time a step up may take: hard code and math of medium or hard take about 3 minutes, the rest about 1.
+fn cap_secs(kind: &str, level: &str) -> f32 {
+    if (kind == "code" && level == "hard") || (kind == "math" && level != "easy") { 180.0 } else { 60.0 }
+}
+
+/// Spare quota only buys quality where quality matters.
+fn matters(kind: &str, level: &str) -> bool {
+    matches!(kind, "code" | "math" | "research") && matches!(level, "medium" | "hard")
+}
+
+/// An unsure verdict gets the next level's bar.
+fn level_of(difficulty: &'static str, confidence: f64) -> &'static str {
+    match (confidence < 0.5, difficulty) {
+        (true, "easy") => "medium",
+        (true, _) => "hard",
+        (false, level) => level,
+    }
+}
+
+/// What the quota says about one profile's price.
+#[derive(Debug, Clone, Copy, PartialEq)]
+pub struct Factor {
+    /// The price multiplier of the profile's windows (budget.rs): below 1 where quota would expire unused, above 1 where it
+    /// runs low.
+    pub m: f64,
+    /// The learned weight of the model in its vendor's quota, 0.5 to 2.
+    pub w: f64,
+    /// False when any vendor-wide window is above 0.90: such a vendor gets no step up.
+    pub step_ok: bool,
+}
+
+impl Factor {
+    pub const NEUTRAL: Factor = Factor { m: 1.0, w: 1.0, step_ok: true };
+}
+
+fn clears(p: &Profile, bar: (u8, u8)) -> bool {
+    p.iq >= bar.0 && p.agent >= bar.1
+}
+
+/// The cheapest candidate that clears the bar under `factor`, with its price and whether it was priced as a step up.
+fn cheapest(
+    kind: &str,
+    level: &str,
+    candidates: &[Profile],
+    factor: &dyn Fn(&Profile) -> Factor,
+    steps: bool,
+) -> Option<(Profile, f64, bool)> {
+    let (own, next, cap) = (bar(kind, level), next_bar(kind, level), cap_secs(kind, level));
+    let matters = matters(kind, level);
+    candidates
+        .iter()
+        .filter(|p| clears(p, own))
+        .map(|p| {
+            let f = factor(p);
+            let f_m = if matters { f.m } else { f.m.max(1.0) };
+            let step = steps && matters && next != own && clears(p, next) && p.secs <= cap && f.step_ok;
+            let price = p.cost as f64 * f.w * f_m * if step { f_m.min(1.0) } else { 1.0 };
+            (*p, price, step)
+        })
+        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.secs.total_cmp(&b.0.secs)))
+}
+
+fn strongest(candidates: &[Profile]) -> Option<Profile> {
+    candidates.iter().max_by_key(|p| (p.iq, p.agent)).copied()
+}
+
+/// The cheapest candidate that clears the bar (the faster on a tie), or the strongest when none does. An unsure verdict
+/// gets the next level's bar. This is the table with no quota information.
+pub fn choose(kind: &'static str, difficulty: &'static str, confidence: f64, candidates: &[Profile]) -> Option<Routed> {
+    let level = level_of(difficulty, confidence);
+    let (profile, price) = match cheapest(kind, level, candidates, &|_| Factor::NEUTRAL, false) {
+        Some((p, price, _)) => (p, price),
+        None => {
+            let p = strongest(candidates)?;
+            (p, p.cost as f64)
+        }
+    };
+    Some(Routed { profile, kind, difficulty, confidence, price, pace: None })
+}
+
+/// The table with the quota in it. Every candidate that clears the task's own bar is in the pool at `cost x w x f`, where
+/// `f` is the profile's `m` on tasks that matter and at least 1 on the others. On a task that matters, a candidate that
+/// also clears the next bar, fits the time cap and is in a vendor with room gets the discount twice. With every factor
+/// neutral the pick is exactly `choose`'s. `pace` says why the pick differs from `choose`'s, if the quota is the reason.
+pub fn choose_paced(
+    kind: &'static str,
+    difficulty: &'static str,
+    confidence: f64,
+    candidates: &[Profile],
+    factor: &dyn Fn(&Profile) -> Factor,
+) -> Option<Routed> {
+    let level = level_of(difficulty, confidence);
+    let usual = choose(kind, difficulty, confidence, candidates)?;
+    let Some((profile, price, step)) = cheapest(kind, level, candidates, factor, true) else {
+        return Some(usual);
+    };
+    let pace = if profile == usual.profile {
+        None
+    } else if step && (profile.iq, profile.agent) > (usual.profile.iq, usual.profile.agent) {
+        Some(PaceNote { kind: "step".into(), vendor: profile.vendor })
+    } else if factor(&usual.profile).m >= LOW_M && profile.vendor != usual.profile.vendor {
+        Some(PaceNote { kind: "low".into(), vendor: usual.profile.vendor })
+    } else {
+        None
+    };
+    Some(Routed { profile, kind, difficulty, confidence, price, pace })
+}
```

`engine/src/router.rs`

```diff
--- a/engine/src/router.rs
+++ b/engine/src/router.rs
@@ -1,6 +1,6 @@
 //! Which model answers a turn, and why.
 
-use crate::model::Vendor;
+use crate::model::{PaceNote, Vendor};
 use serde::{Deserialize, Serialize};
 
 #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
@@ -17,6 +17,8 @@
     pub model: Option<String>,
     pub effort: String,
     pub reason: String,
+    /// Why the quota changed the pick, when it did (Auto only).
+    pub pace: Option<PaceNote>,
 }
 
 /// Medium effort by default: high quality for most work without spending quota on easy turns.
@@ -31,6 +33,7 @@
         model: model.or(codex_recommended.filter(|_| vendor == Vendor::Codex)).map(String::from),
         effort: EFFORT.into(),
         reason: reason.into(),
+        pace: None,
     };
     if choice.vendor != "auto" {
         let vendor = Vendor::ALL.into_iter().find(|v| v.name() == choice.vendor)?;
```

- [ ] **Step 4: Run the engine suite, clippy and fmt**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): the pacing table and dispatch through the gate"
```

### Task 9: Learning wired to real turns

The weights and the outside-use estimate learn from what Faber really did.

**Files:**
- `engine/src/engine.rs`
- `engine/src/engine/answer.rs`
- `engine/src/engine/auto.rs`
- `engine/tests/learning.rs`
- `engine/tests/pacing.rs`

**Interfaces:**
- Consumes: Tasks 4, 8. Produces: `Core::note_turn`, `Live.started`, a span for every user turn and every router question, `learn_weights` after each user turn with a forced save when the capacity jumps.

- [ ] **Step 1: Add the tests (they fail or do not compile yet)**

```diff
--- a/engine/tests/learning.rs
+++ b/engine/tests/learning.rs
@@ -0,0 +1,156 @@
+//! What the engine hands to the learning: the span of every request Faber makes, and the weights learned from them.
+
+mod support;
+
+use faber_engine::budget::Billing;
+use faber_engine::cli::Cli;
+use faber_engine::engine::Ask;
+use faber_engine::events::EngineEvent;
+use faber_engine::model::{Area, Message, Role, Status, Vendor};
+use faber_engine::proc::Spawner;
+use faber_engine::route::question;
+use faber_engine::router::Choice;
+use faber_engine::signals::Signals;
+use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest, WarmRequest};
+use serde_json::json;
+use std::collections::BTreeMap;
+use std::path::Path;
+use std::sync::Arc;
+use std::sync::atomic::{AtomicU64, Ordering};
+use std::time::Duration;
+use support::{Entry, fake, fixture};
+use tokio::sync::mpsc::UnboundedReceiver;
+
+const SKY: &str = "In one sentence: why is the sky blue?";
+/// The recorded five-hour window resets at this time.
+const RESETS: u64 = 1_791_627_000;
+const START: u64 = 1_791_620_000;
+
+/// A clock that moves one second every time it is read, from a base the test sets: a turn then takes time, as it does.
+struct Ticking {
+    base: AtomicU64,
+    ticks: AtomicU64,
+}
+
+fn start(dir: &Path, spawn: Spawner, ask: Option<Ask>) -> (Engine, UnboundedReceiver<AppEvent>, Arc<Ticking>) {
+    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
+    let opts = EngineOptions {
+        data_dir: dir.into(),
+        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
+        spawn,
+        windows: false,
+        max_warm: 3,
+        idle: Duration::from_secs(120),
+        runtime: tokio::runtime::Handle::current(),
+        ask,
+    };
+    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
+    let clock = Arc::new(Ticking { base: AtomicU64::new(START), ticks: AtomicU64::new(0) });
+    let c = clock.clone();
+    engine.set_time_source(Arc::new(move || (c.base.load(Ordering::SeqCst) + c.ticks.fetch_add(1, Ordering::SeqCst)) as f64));
+    (engine, rx, clock)
+}
+
+fn by_hand() -> SendRequest {
+    SendRequest {
+        conversation_id: None,
+        area: Area::Chat,
+        project_id: None,
+        text: SKY.into(),
+        images: vec![],
+        files: vec![],
+        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()) },
+    }
+}
+
+async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> Message {
+    loop {
+        let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("no finished answer in time").expect("engine gone");
+        if let AppEvent::Message { message, .. } = e
+            && message.role == Role::Assistant
+            && message.status != Some(Status::Streaming)
+        {
+            return *message;
+        }
+    }
+}
+
+fn reading(used: f64) -> EngineEvent {
+    EngineEvent::Quota { window: "five_hour".into(), used, resets_at: RESETS as f64 }
+}
+
+#[tokio::test]
+async fn a_user_turn_leaves_its_span_with_the_model_and_the_cost() {
+    let dir = tempfile::tempdir().unwrap();
+    let (engine, mut rx, _) = start(dir.path(), fake(fixture("claude", "chat")).spawn, None);
+    engine.send(by_hand()).unwrap();
+    let reply = finished(&mut rx).await;
+    let spans = engine.ledger().claude.spans;
+    assert_eq!(spans.len(), 1);
+    assert_eq!(spans[0].model, "claude-haiku-5-5");
+    // The ledger keeps a millionth of a dollar.
+    let (kept, shown) = (spans[0].cost.unwrap(), reply.cost_usd.unwrap());
+    assert!((kept - shown).abs() < 1e-6, "the cost of the turn, not the running total: {kept} vs {shown}");
+    assert!(spans[0].cost.is_some_and(|c| c > 0.0));
+    assert!(spans[0].start >= START as f64 && spans[0].end > spans[0].start, "{spans:?}");
+    engine.shutdown();
+}
+
+#[tokio::test]
+async fn eight_measured_turns_teach_the_weights_and_the_stretches_are_kept() {
+    let dir = tempfile::tempdir().unwrap();
+    let (engine, mut rx, clock) = start(dir.path(), fake(fixture("claude", "chat")).spawn, None);
+    for i in 0..8 {
+        clock.base.store(START + i * 600, Ordering::SeqCst);
+        clock.ticks.store(0, Ordering::SeqCst);
+        // Three points below what the recording will report, so each turn moves the window by three points.
+        engine.note_account_events(Vendor::Claude, vec![reading(0.04)]);
+        engine.send(by_hand()).unwrap();
+        finished(&mut rx).await;
+    }
+    let claude = engine.ledger().claude;
+    assert_eq!(claude.spans.len(), 8);
+    assert_eq!(claude.stretches.len(), 8);
+    assert_eq!(claude.stretches_since, 8);
+    assert!(claude.weights.capacity.is_some_and(|c| c > 0.0), "{:?}", claude.weights);
+    assert!(claude.weights.w.contains_key("claude-haiku-5-5"));
+    engine.shutdown();
+    let saved: faber_engine::ledger::Ledger =
+        serde_json::from_str(&std::fs::read_to_string(dir.path().join("budget.json")).unwrap()).unwrap();
+    assert_eq!(saved.claude.stretches.len(), 8, "what was learned survives a restart");
+}
+
+#[tokio::test]
+async fn a_router_question_is_a_span_too_and_has_no_cost() {
+    let dir = tempfile::tempdir().unwrap();
+    // The router's session is the Claude chat recording, with the question it will be asked as the recorded input.
+    let text = "Plan a trip to Lisbon";
+    let entries: Vec<Entry> = fixture("claude", "chat")
+        .into_iter()
+        .map(|mut e| {
+            if e.input {
+                e.msg["message"]["content"] = json!(question(text, 0));
+            }
+            e
+        })
+        .collect();
+    let f = fake(entries);
+    let (engine, _rx, _) = start(dir.path(), f.spawn.clone(), None);
+    // A fresh reading and a seen plan, so that no pulse is due and the gate lets the router ask.
+    let plan = EngineEvent::Signals(Signals { plan: Some("max".into()), billing: Some(Billing::Subscription), ..Signals::default() });
+    engine.note_account_events(Vendor::Claude, vec![reading(0.05), plan]);
+    let typing =
+        WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: Choice { vendor: "auto".into(), model: None } };
+    engine.route_draft(typing, text.into());
+    for _ in 0..100 {
+        if !engine.ledger().claude.spans.is_empty() {
+            break;
+        }
+        tokio::time::sleep(Duration::from_millis(30)).await;
+    }
+    let spans = engine.ledger().claude.spans;
+    assert_eq!(spans.len(), 1, "one question, one span");
+    assert_eq!(spans[0].cost, None);
+    assert_eq!(f.args.lock().unwrap().len(), 1, "no pulse: the reading was fresh");
+    engine.shutdown();
+}
```

```diff
--- a/engine/tests/pacing.rs
+++ b/engine/tests/pacing.rs
@@ -354,3 +354,18 @@
     assert_eq!(again.engine.open(&id).unwrap().messages[1].pace, None);
     again.engine.shutdown();
 }
+
+#[tokio::test]
+async fn a_week_spent_faster_than_one_seventh_a_day_is_priced_up_on_either_vendor() {
+    // One day of seven gone and a fifth of the week used (14.3% a day would be 14.3%): the vendor is left for the other.
+    for (spent, other) in [(Vendor::Claude, Vendor::Codex), (Vendor::Codex, Vendor::Claude)] {
+        let dir = tempfile::tempdir().unwrap();
+        let r = both(dir.path(), verdicts(vec![("code", "medium")]));
+        r.engine.note_account_events(spent, vec![plan("max"), quota("seven_day", 0.2, 6 * DAY)]);
+        r.engine.note_account_events(other, vec![plan("pro"), quota("seven_day", 0.1, 6 * DAY)]);
+        let id = r.engine.send(auto(CODE)).unwrap();
+        let (reply, _) = r.went(&id, 0).await;
+        assert_eq!(reply.vendor, Some(other), "{spent:?} is ahead of the daily line");
+        r.engine.shutdown();
+    }
+}
```

- [ ] **Step 2: Run the new tests and see them fail**

Run: `cargo test -p faber-engine` (the named test files)
Expected: compile errors or failures naming the missing items.

- [ ] **Step 3: Implement**

`engine/src/engine.rs`

```diff
--- a/engine/src/engine.rs
+++ b/engine/src/engine.rs
@@ -136,6 +136,8 @@
     cli: Cli,
     /// The process continues a session an earlier process began.
     resumed: bool,
+    /// When the turn being answered was handed to the CLI (Unix seconds); the learning needs the span it ran in.
+    started: f64,
 }
 
 const HISTORY_LIMIT: usize = 20_000;
@@ -497,6 +499,22 @@
         self.ledger.of_mut(vendor).span(Span { start, end, model: model.to_string(), cost });
     }
 
+    /// A user turn ended: its span goes to the learning, and the weights are learned again from the stretches so far. A
+    /// capacity that moved by more than a factor of two is a plan change the name did not announce; the ledger restarts
+    /// its learning and the file is written at once.
+    fn note_turn(&mut self, conv: &str, cost: Option<f64>) {
+        let Some(l) = self.live.get(conv).filter(|l| l.started > 0.0) else { return };
+        let (vendor, start, model) = (l.vendor, l.started, l.running.clone().or_else(|| l.model.clone()).unwrap_or_default());
+        self.note_span(vendor, start, &model, cost);
+        let now = self.secs();
+        if self.ledger.of_mut(vendor).learn_weights(now) {
+            self.save_ledger(true);
+        }
+        if let Some(l) = self.live.get_mut(conv) {
+            l.started = 0.0;
+        }
+    }
+
     fn claude_plan_checked(&mut self, found: Option<Signals>) {
         let Some(s) = found else { return self.ledger.claude.plan_failed() };
         let (named, by_use) = (s.plan.is_some(), s.billing == Some(Billing::UsageBilled));
@@ -729,6 +747,7 @@
                 if let (Some(base), Some(c)) = (settled.base, self.convs.get_mut(conv)) {
                     c.cost_base.insert(vendor, base);
                 }
+                self.note_turn(conv, settled.cost);
                 if let Some(m) = self.turn(conv) {
                     m.status = Some(if ok { Status::Done } else { Status::Stopped });
                     m.cost_usd = settled.cost;
@@ -751,6 +770,7 @@
                     }
                 }
                 if fatal {
+                    self.note_turn(conv, None);
                     self.live.remove(conv);
                     return self.finish(conv, turn_id);
                 }
@@ -924,8 +944,10 @@
         {
             m.model = Some(running);
         }
+        let now = self.secs();
         if let Some(l) = self.live.get_mut(id) {
             l.turn = Some(reply_id.to_string());
+            l.started = now;
             l.session.send(UserTurn { text, images });
         }
         self.push(id, Some(reply_id), true);
@@ -991,6 +1013,7 @@
                 used: self.tick,
                 cli,
                 resumed,
+                started: 0.0,
             },
         );
         // ponytail: count-based limit on idle Claude processes; a RAM budget across all CLIs comes with parallel workers.
```

`engine/src/engine/answer.rs`

```diff
--- a/engine/src/engine/answer.rs
+++ b/engine/src/engine/answer.rs
@@ -235,6 +235,7 @@
                 used: 0,
                 cli: Cli { command: "claude".into(), prefix: vec![] },
                 resumed: false,
+                started: 0.0,
             };
             c.live.insert(id.clone(), live);
             c.refresh_running();
```

`engine/src/engine/auto.rs`

```diff
--- a/engine/src/engine/auto.rs
+++ b/engine/src/engine/auto.rs
@@ -44,6 +44,8 @@
     answer: oneshot::Receiver<Option<String>>,
     id: u64,
     vendor: Vendor,
+    /// When the question was sent (Unix seconds); set by `ask_router`. An idle router session uses no quota.
+    asked: Arc<Mutex<Option<f64>>>,
 }
 
 /// A draft's route still holds for the sent text when the text only grew a little past it.
@@ -182,7 +184,8 @@
             tokio::spawn(async move { drop(tx.send(answer.await)) });
             return (Some(rx), None);
         }
-        let Some(Spare { session, answer, .. }) = self.spare.take().or_else(|| self.open_router()) else { return (None, None) };
+        let Some(Spare { session, answer, asked, .. }) = self.spare.take().or_else(|| self.open_router()) else { return (None, None) };
+        *asked.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.secs());
         session.send(UserTurn::text(prompt));
         (Some(answer), Some(Arc::new(Mutex::new(Some(session)))))
     }
@@ -199,9 +202,16 @@
         let text = Arc::new(Mutex::new(String::new()));
         let tx = Arc::new(Mutex::new(Some(tx)));
         let me = self.me.clone();
+        let asked: Arc<Mutex<Option<f64>>> = Arc::default();
+        let when = asked.clone();
         let sink: EventSink = Arc::new(move |e| match e {
             EngineEvent::Text(t) => text.lock().unwrap_or_else(|e| e.into_inner()).push_str(&t),
             EngineEvent::Done { .. } | EngineEvent::Error { .. } => {
+                // The question used the vendor's quota: the learning must not count that as use from outside.
+                let start = when.lock().unwrap_or_else(|e| e.into_inner()).take();
+                if let (Some(start), Some(core)) = (start, me.upgrade()) {
+                    core.lock().unwrap_or_else(|e| e.into_inner()).note_span(vendor, start, model, None);
+                }
                 if let Some(tx) = tx.lock().unwrap_or_else(|e| e.into_inner()).take() {
                     let _ = tx.send(Some(std::mem::take(&mut *text.lock().unwrap_or_else(|e| e.into_inner()))));
                 }
@@ -239,7 +249,7 @@
             ),
         };
         self.next_sid += 1;
-        Some(Spare { session, answer, id: self.next_sid, vendor })
+        Some(Spare { session, answer, id: self.next_sid, vendor, asked })
     }
 
     /// Starts a routed first message once its route is known, or the everyday default when the route is late.
```

- [ ] **Step 4: Run the engine suite, clippy and fmt**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): learning wired to real turns"
```

### Task 10: Move or wait when a vendor cannot answer

Spec section 3.3 and decisions D6, D8, D11. Unlike Tasks 1-9, this task was not run in the copy: write the tests first and let the compiler guide the wiring.

**Files:**
- Create: `engine/src/handover.rs` (pure; a verified standalone version is in the spec's scratch crate: `FULL_LIMIT` 24,000 chars, otherwise the first message plus the last 40,000 characters go to a summary request of at most 600 words), `engine/tests/handover.rs`, `engine/tests/waiting.rs`
- Modify: `engine/src/lib.rs` (`pub mod handover`), `engine/src/engine.rs` (waiting messages, `Wait`), `engine/src/engine/auto.rs` (move/wait choice)

**Interfaces:**
- Consumes: `Core::allows` (Task 8), `Core::note_turn` (Task 9), `Purpose::Summary` (Task 3), `Message.pace` (Task 7, merged into Task 8).
- Produces:
  - `handover::{FULL_LIMIT, plan(history: &[Message]) -> Plan}` with `Plan::{Full(String), Summarise(String)}`.
  - `Wait::{Queue(f64), Refuse}` with a 6 h queue limit.
  - Error keys `error.quotaWait` and `error.moveFailed`.
  - `Engine::approve_credits(vendor)`: one vendor, one message, consumed when that turn starts.
  - A turn that fails with a plan-limit error marks the vendor `Reach::Out` in the ledger.

- [ ] **Step 1: Write the failing tests**

```rust
// engine/tests/waiting.rs (uses the rig of tests/pacing.rs: `both`, `auto`, `quota`, `plan`, `verdicts`)
#[tokio::test]
async fn a_message_for_a_vendor_that_resets_soon_waits_and_sends_itself() {
    // Only Claude is connected and its five-hour window is full but resets in 10 minutes.
    // Expect: the reply is Streaming with reason `reason.waiting`, no Claude process starts, then the clock moves past
    // the reset, the timer re-gates, and the turn starts.
}

#[tokio::test]
async fn a_wait_longer_than_six_hours_is_refused_and_the_text_stays_in_the_composer() {
    // Week window full, resets in 2 days. Expect: reply status Error with `error.quotaWait`; no spawn.
}

#[tokio::test]
async fn closing_the_app_or_stopping_cancels_a_waiting_message() {
    // Review focus 1. After `engine.interrupt(&id)`, advancing the clock past the reset starts no process, and the
    // reply is Stopped. After `engine.shutdown()` and a new Engine over the same data_dir, the old waiting reply loads as
    // Stopped (the existing `settle()` makes a Streaming message Stopped on load) and nothing is sent.
}

#[tokio::test]
async fn a_failed_hand_over_ends_with_move_failed_and_keeps_the_conversation() {
    // Claude family out in the middle of a long chat, Codex's summary request fails (fake returns Error).
    // Expect `error.moveFailed` and the conversation file still lists every earlier message.
}
```

Fill each test body from the rig in `engine/tests/pacing.rs` (the helpers `both`, `auto`, `quota`, `plan`, `verdicts`, `Rig::went`); the comments above are the assertions to write. `engine/tests/handover.rs` pins `plan()`: a 10,000-character history is `Full`, a 30,000-character one is `Summarise` and its text starts with the first message and ends with the last turn.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p faber-engine --test waiting --test handover`
Expected: compile errors for the missing items.

- [ ] **Step 3: Implement**

`handover::plan`:

```rust
pub const FULL_LIMIT: usize = 24_000;
const TAIL: usize = 40_000;

pub enum Plan {
    Full(String),
    Summarise(String),
}

/// The history as plain text, or what to ask a model to summarise when it is too long to hand over whole.
pub fn plan(history: &[Message]) -> Plan {
    let lines: Vec<String> = history.iter().map(|m| format!("{}: {}", if m.role == Role::User { "User" } else { "Assistant" }, m.text)).collect();
    let all = lines.join("\n\n");
    if all.chars().count() <= FULL_LIMIT {
        return Plan::Full(all);
    }
    let tail: String = all.chars().rev().take(TAIL).collect::<Vec<_>>().into_iter().rev().collect();
    let first = lines.first().cloned().unwrap_or_default();
    Plan::Summarise(format!("{first}\n\n[...]\n\n{tail}"))
}
```

In `Core::table` callers (`send` in `engine.rs`): when the pick for an existing conversation is a different vendor than the conversation's, call `handover::plan`; `Full` goes in front of the user's text as `with_history` does today, `Summarise` is sent to the other vendor's router model through `Core::allows(.., Purpose::Summary, false)` and the 600-word answer replaces the history (reason `reason.moved.summary`; `reason.moved.full` for `Full`). A same-vendor family-out continues with `--resume` and the next family.

The wait: when no profile of any vendor passes the gate, compute `earliest = min(resets_at)` of the windows that block; if `earliest - now <= 6 h` the reply gets reason `reason.waiting`, `status: Streaming`, and `tokio::spawn` sleeps until `earliest`, then re-runs `core.allows` and starts (or waits again); otherwise the reply ends with `error.quotaWait` and the text is returned to the composer by the screen. The sleeping task holds a `Weak` of the core and checks, under the lock, that the reply is still `Streaming` and still the same `reply_id`; `Engine::interrupt` and `shutdown` set the reply to `Stopped`, so a stopped or closed message never sends.

A turn that ends in a fatal error whose text matches the CLI's plan-limit wording sets `ledger.of_mut(vendor).reach = Reach::Out` (the existing `Reach` type) and is retried once through `table`, exactly once per message.

- [ ] **Step 4: Run the suite**

Run: `cargo test -p faber-engine && cargo clippy -p faber-engine --all-targets -- -D warnings && cargo fmt --all --check`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add engine
git commit -m "feat(engine): move to the other vendor or wait when a vendor cannot answer"
```

### Task 11: The screen and the shell

Spec section 4. Not run in the copy.

**Files:**
- Modify: `engine/src/engine.rs` (`AppState.usage`), `src-tauri/src/lib.rs` (commands), `ui/api.ts`, `ui/i18n.ts`, `ui/Thread.tsx`, `ui/Settings.tsx`, `ui/sidebar.ts`, `ui/running.ts`, `ui/styles.css`, `ui/locales/{en,pt-BR,de,es,fr}.ts`
- Create: `tests/ui/pace.check.ts`

**Interfaces:**
- Consumes: `Engine::{ledger, answer_plan, watch_accounts, note_account_events, set_utc_offset}` (Tasks 5-6), `Message.pace` (Task 7, merged into Task 8), the waiting reply (Task 10).
- Produces: `AppState.usage: Vec<VendorUsage>` with `{ vendor, plan, state, learning, relearning, outside_share, overage_on, billed_per_use, windows: [{ name, used, elapsed, expected }] }`; Tauri commands `set_clock(utc_offset_secs: i64)`, `answer_plan(vendor, plan)`, `approve_credits(vendor)`.

- [ ] **Step 1: Write the screen check**

`tests/ui/pace.check.ts` follows the other `*.check.ts` files: load the app with a state holding one message whose `pace` is `{ kind: "step", vendor: "claude" }` and assert the thread shows the text of `reason.pace.step` after the normal reason; then one with `kind: "low"`; then a Settings state with a vendor whose `windows` has `elapsed: 0.5, used: 0.3` and assert the usage row shows the window name, the used share and a tick at 50%.

- [ ] **Step 2: Run it and see it fail**

Run: `cargo t --all` (the screen suite runs on Windows)
Expected: the new check fails.

- [ ] **Step 3: Implement**

Engine view, in `engine.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct WindowUsage {
    pub name: String,
    pub used: f64,
    /// The share of the window gone, by the learned shape (uniform until it has data).
    pub elapsed: f64,
    /// Where the daily line says use should be now.
    pub expected: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VendorUsage {
    pub vendor: Vendor,
    pub plan: Option<String>,
    /// `ok`, `low`, `out`, `unknown`.
    pub state: &'static str,
    pub learning: bool,
    pub relearning: bool,
    pub outside_share: Option<f64>,
    pub overage_on: bool,
    pub billed_per_use: bool,
    pub ask_for_plan: bool,
    pub windows: Vec<WindowUsage>,
}
```

built from `Ledger::of(vendor)` (`snap(now).windows`, `outside(now).share()`, `ask_for_plan()`, `relearning(now)`) in `Engine::state()`.

Shell, in `src-tauri/src/lib.rs`: add `set_clock`, `answer_plan` and `approve_credits` to the fixed command list and to `generate_handler![...]`; `load_models` also calls `read_codex_limits` then `engine.note_account_events(Vendor::Codex, events)`; `setup` calls `engine.watch_accounts()` once; the screen calls `set_clock(-new Date().getTimezoneOffset() * 60)` on start.

UI: `Thread.tsx` prints `t(\`reason.pace.${pace.kind}\`, { vendor })` after the reason line; `Settings.tsx` gets a Usage section per vendor (plan or `usage.planUnknown` with the ask form when `ask_for_plan`, state, window rows with a bar and a tick at `expected` in `styles.css`); `running.ts` shows `run.waiting` with the time; `sidebar.ts` shows the waiting state.

Locales: add in all five files, written by hand: `reason.pace.step`, `reason.pace.low`, `reason.moved.summary`, `reason.moved.full`, `reason.waiting`, `run.waiting`, `quota.credits.offer`, `quota.credits.confirm`, `error.billedPerUse`, `error.quotaOut`, `error.quotaWait`, `error.moveFailed`, `quota.chosen.out`, `quota.chosen.move`, `quota.chosen.wait`, `usage.plan`, `usage.planUnknown`, `usage.planAsk`, `usage.state.ok`, `usage.state.low`, `usage.state.out`, `usage.state.unknown`, `usage.learning`, `usage.relearning`, `usage.outside`, `usage.overageOn`, `usage.billedPerUse`, `usage.pace`, `usage.expect`. English is the source; a native speaker reviews pt-BR, de, es and fr before release.

- [ ] **Step 4: Run all proof commands**

Run the full list under Global Constraints, with `cargo t --all`.
Expected: all exit 0.

- [ ] **Step 5: Commit**

```bash
git add engine src-tauri ui tests
git commit -m "feat(ui): show the pace reason, the usage section and the waiting state"
```

### Task 12: Simulation, docs and final proof

**Files:**
- Create: `engine/tests/simulation.rs` (copy of the verified scratch test: 200 seeds over 4 weeks is `#[ignore]`; a 4-seed smoke runs in `cargo t`)
- Modify: `docs/design.md`, `docs/roadmap.md`

**Interfaces:**
- Consumes: `route::choose_paced`, `budget::Snap` (Tasks 3, 7).
- Produces: nothing for other tasks.

- [ ] **Step 1: Add the simulation**

It draws a user week from a seed, runs it with pacing off and on, and asserts on the 4-seed smoke that pacing on has fewer exhausted hours than off and drops under 1% of messages. The full run is expected to measure: exhausted 0.045 against 0.145 off, unused 0.024, quality 81.42 against 80.58, dropped about 0.5%.

- [ ] **Step 2: Run it**

Run: `cargo test -p faber-engine --test simulation` then `cargo test -p faber-engine --test simulation -- --ignored`
Expected: both pass.

- [ ] **Step 3: Document**

`docs/design.md`: one paragraph on the pulse (one haiku word every 20 minutes at most, only when the reading is old and Auto is in use), the ledger file and the gate. `docs/roadmap.md`: the memory peak measured with a pulse, and the cache results from `cargo run -p faber-engine --example cache_probe` when Erick has run it.

- [ ] **Step 4: Final proof**

Run: `npm run lint && npm run typecheck && npm run build && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo clippy -p faber --features test-hooks -- -D warnings && cargo t --all`
Expected: all exit 0.

- [ ] **Step 5: Commit**

```bash
git add engine docs
git commit -m "test(engine): simulate a month of pacing and document it"
```

## Self-review

- **Spec coverage:** D1-D12 and sections 1-2 map to Tasks 3-6; section 3 (multiplier, choice, gate, move/wait) to Tasks 3, 7, 8, 10; section 4 to Task 11; section 5-6 (tests, cache first) to Tasks 1-2 and 12; D13 (daily line) to Task 3 (uniform `elapsed`) and the Task 8 test. Gap: none known; Tasks 10-12 carry the unverified parts.
- **Placeholders:** Task 10's four test bodies are described by their assertions and must be filled from the pacing rig; Tasks 10-11 were not run in the copy. Tasks 1-9 are real, tested diffs.
- **Type consistency:** names come from the verified copy (`Asked`, `Routed{price,pace}`, `Purpose`, `Why`, `Ledger`, `note_turn`, `allows`, `table`); Tasks 10-11 only consume them.
- **Review Focus:** items 1-5 map to Task 10 tests, Task 8 tests (`a_login_billed_by_use...`, `a_clock_set_backwards...`, `a_reading_that_may_be_wrong...`), Task 5 `a_budget_file_that_is_damaged_old_or_odd...`, Task 4 sanitize, and Task 5 `a_reading_that_cannot_be_real...`.

Plan complete and saved to `docs/superpowers/plans/2026-10-11-auto-quota-pacing.md`. Please review the plan. Does it capture what you want?
