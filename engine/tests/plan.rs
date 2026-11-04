//! Plan detection, the billing verdict and the quota pulse. No real CLI runs: status commands and the pulse are replayed.

use crate::support;

use faber_engine::budget::Billing;
use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::model::Vendor;
use faber_engine::proc::{Proc, Spawner};
use faber_engine::{AppEvent, Engine, EngineOptions};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{Entry, fake};
use tokio::io::{AsyncRead, AsyncWriteExt, ReadBuf};
use tokio::sync::mpsc::UnboundedReceiver;

/// A moment before the recorded windows reset (the Claude five-hour window resets at 1791627000).
const WHEN: u64 = 1_791_620_000;
type Calls = Arc<Mutex<Vec<(String, Vec<String>)>>>;
type Time = Arc<AtomicU64>;

struct Pending;

impl AsyncRead for Pending {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Pending
    }
}

/// A status command that prints `stdout` and ends, or never answers when `stdout` is None.
fn status(stdout: Option<&'static str>) -> (Spawner, Calls) {
    let calls: Calls = Arc::default();
    let recorded = calls.clone();
    let spawn: Spawner = Arc::new(move |bin, args, _cwd| {
        recorded.lock().unwrap().push((bin.into(), args.to_vec()));
        let Some(stdout) = stdout else {
            return Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(Pending), stderr: None, child: None, contained: None });
        };
        let (mut out, reader) = tokio::io::duplex(4096);
        tokio::spawn(async move { drop(out.write_all(stdout.as_bytes()).await) });
        Ok(Proc { stdin: Box::new(tokio::io::sink()), stdout: Box::new(reader), stderr: None, child: None, contained: None })
    });
    (spawn, calls)
}

fn start(dir: &crate::support::DataFolder, spawn: Spawner, ask: Option<Ask>) -> (Engine, UnboundedReceiver<AppEvent>, Time) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.path().into(),
        #[cfg(windows)]
        chat_dir: dir.chat_path().into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    let time: Time = Arc::new(AtomicU64::new(WHEN));
    let clock = time.clone();
    engine.set_time_source(Arc::new(move || clock.load(Ordering::SeqCst) as f64));
    (engine, rx, time)
}

const FIVE: Duration = Duration::from_millis(500);

#[tokio::test]
async fn the_plan_comes_from_the_status_command_and_nothing_else_of_it_is_kept() {
    let dir = support::DataFolder::new();
    let (spawn, calls) = status(Some(r#"{"loggedIn":true,"email":"someone@example.com","orgName":"Acme","subscriptionType":"max"}"#));
    let (engine, mut events, _) = start(&dir, spawn, None);
    engine.check_plans(FIVE).await;
    let claude = engine.ledger().claude;
    assert_eq!((claude.plan.as_deref(), claude.billing), (Some("max"), Billing::Subscription));
    assert!(claude.plan_ok(WHEN as f64));
    let statuses: Vec<_> = calls.lock().unwrap().iter().filter(|(_, args)| args == &["auth", "status"]).cloned().collect();
    assert_eq!(statuses, [("claude".to_string(), vec!["auth".to_string(), "status".to_string()])]);
    assert!(
        claude.zero_turn_readings.is_empty() && claude.spans.is_empty() && claude.own_spend.is_empty(),
        "an auth-status payload cannot become quota evidence or measured model use"
    );
    engine.shutdown();
    let saved = std::fs::read_to_string(dir.path().join("budget.json")).unwrap();
    assert!(!saved.contains("someone@example.com") && !saved.contains("Acme"));
    while let Ok(event) = events.try_recv() {
        assert!(matches!(event, AppEvent::Usage { .. }), "only the sanitized usage view reaches the screen");
        let shown = format!("{event:?}");
        assert!(!shown.contains("someone@example.com") && !shown.contains("Acme"));
    }
}

#[tokio::test]
async fn a_login_billed_per_use_is_known_as_such_and_needs_no_plan() {
    let dir = support::DataFolder::new();
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"authMethod":"api_key"}"#));
    let (engine, _, _) = start(&dir, spawn, None);
    engine.check_plans(FIVE).await;
    let claude = engine.ledger().claude;
    assert_eq!((claude.billing, claude.plan.as_deref()), (Billing::UsageBilled, None));
    assert!(!claude.ask_for_plan(), "there is no plan to ask about");
}

#[tokio::test]
async fn without_a_plan_in_the_answer_the_user_is_asked_and_the_answer_never_authorizes_billing() {
    let dir = support::DataFolder::new();
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"authMethod":"claude.ai"}"#));
    let (engine, _, _) = start(&dir, spawn, None);
    engine.check_plans(FIVE).await;
    assert!(engine.ledger().claude.ask_for_plan());
    engine.answer_plan(Vendor::Claude, "  Max 20x! ");
    let claude = engine.ledger().claude;
    assert_eq!(claude.typed_plan.as_deref(), Some("max20x"));
    assert!(claude.plan_ok(WHEN as f64 + 3600.0 * 5.0), "a typed plan needs no fresh detection");
    assert!(!claude.ask_for_plan());
    assert_eq!(claude.billing, Billing::Unknown, "naming a plan proves nothing about how the login is billed");
    engine.answer_plan(Vendor::Claude, "   ");
    assert_eq!(engine.ledger().claude.typed_plan.as_deref(), Some("max20x"), "an empty answer changes nothing");
}

#[tokio::test]
async fn one_failed_check_asks_nothing_three_in_a_row_ask_and_a_failure_never_refreshes_the_plan() {
    let dir = support::DataFolder::new();
    let (good, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"pro"}"#));
    let (engine, _, time) = start(&dir, good, None);
    engine.check_plans(FIVE).await;
    assert!(engine.ledger().claude.plan_ok(WHEN as f64));
    engine.shutdown();

    // The same data folder, a CLI that no longer answers.
    let (hung, _) = status(None);
    let (engine, _, time2) = start(&dir, hung, None);
    time2.store(WHEN + 31 * 60, Ordering::SeqCst);
    drop(time);
    engine.check_plans(Duration::from_millis(30)).await;
    assert!(!engine.ledger().claude.plan_ok((WHEN + 31 * 60) as f64), "the plan is as old as the last good check");
    assert!(!engine.ledger().claude.ask_for_plan());
    engine.check_plans(Duration::from_millis(30)).await;
    engine.check_plans(Duration::from_millis(30)).await;
    // A plan was known from before, so nothing is asked even after three failures; discounts simply wait.
    assert!(!engine.ledger().claude.ask_for_plan());
    assert_eq!(engine.ledger().claude.plan.as_deref(), Some("pro"));

    let fresh = support::DataFolder::new();
    let (hung, _) = status(None);
    let (engine, _, _) = start(&fresh, hung, None);
    for _ in 0..2 {
        engine.check_plans(Duration::from_millis(30)).await;
    }
    assert!(!engine.ledger().claude.ask_for_plan(), "a single failed or slow attempt asks nothing");
    engine.check_plans(Duration::from_millis(30)).await;
    assert!(engine.ledger().claude.ask_for_plan());
}

#[tokio::test]
async fn a_changed_plan_name_starts_the_learning_again() {
    let dir = support::DataFolder::new();
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"pro"}"#));
    let (engine, _, _) = start(&dir, spawn, None);
    engine.check_plans(FIVE).await;
    assert!(!engine.ledger().claude.relearning(WHEN as f64));
    engine.shutdown();

    // The plan is remembered across runs, so the next run notices that it is a different one.
    let (spawn, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
    let (engine, _, _) = start(&dir, spawn, None);
    engine.check_plans(FIVE).await;
    let claude = engine.ledger().claude;
    assert_eq!(claude.plan.as_deref(), Some("max"));
    assert!(claude.relearning(WHEN as f64 + 60.0), "no discount until it has learned the new plan");
}

#[tokio::test]
async fn watching_the_accounts_checks_at_once() {
    let dir = support::DataFolder::new();
    let (spawn, calls) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
    let (engine, _, _) = start(&dir, spawn, None);
    engine.set_utc_offset(0);
    engine.watch_accounts();
    support::until("the Claude plan was never read", || engine.ledger().claude.plan.is_some()).await;
    assert_eq!(engine.ledger().claude.plan.as_deref(), Some("max"));
    assert_eq!(
        calls.lock().unwrap().iter().filter(|(_, args)| args == &["auth", "status"]).count(),
        1,
        "one account check now, then every quarter of an hour; optional quota work is separate"
    );
    engine.shutdown();
}

#[tokio::test]
async fn startup_account_readings_wait_for_the_local_offset_and_do_not_spend() {
    let dir = support::DataFolder::new();
    let mut ledger = faber_engine::ledger::Ledger::default();
    ledger.codex.record("five_hour", 0.10, (WHEN + 7000) as f64, (WHEN - 60) as f64, -10800);
    faber_engine::store::Store::new(dir.path()).save_ledger(&ledger).unwrap();
    let f = fake(vec![
        Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
        Entry { input: false, msg: json!({"id":1,"result":{}}) },
        Entry { input: true, msg: json!({"method":"initialized"}) },
        Entry { input: true, msg: json!({"id":2,"method":"account/rateLimits/read"}) },
        Entry { input: true, msg: json!({"id":3,"method":"account/read","params":{"refreshToken":false}}) },
        Entry { input: false, msg: json!({"id":3,"result":{"account":{"type":"chatgpt","planType":"plus"}}}) },
        Entry {
            input: false,
            msg: json!({"id":2,"result":{"rateLimits":{"primary":{"usedPercent":13,"windowDurationMins":300,"resetsAt":WHEN + 7000}}}}),
        },
    ]);
    let (engine, _, _) = start(&dir, f.spawn.clone(), None);
    engine.replace_clis(BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]));
    engine.watch_accounts();
    tokio::time::sleep(Duration::from_millis(80)).await;
    let before_clock = f.args.lock().unwrap().len();
    assert!(engine.ledger().codex.own_spend.is_empty());
    engine.set_utc_offset(-10800);
    support::until("the Codex five-hour reading never reached 0.13", || engine.ledger().codex.readings["five_hour"].used == 0.13).await;
    let account = engine.ledger().codex;
    let local_bin = faber_engine::budget::bin((WHEN - 60) as f64, -10800);
    assert_eq!(account.learned.mass[local_bin], 3.0, "the startup drop belongs to the local weekday and hour");
    assert_eq!(before_clock, 0, "the watcher waits until the screen supplies the offset");
    assert!(account.own_spend.is_empty() && account.spans.is_empty(), "a free account check never meters a turn");
    assert_eq!(f.wire.lines_with("turn/start"), 0);
    assert!(f.mismatches.lock().unwrap().is_empty());
    engine.set_utc_offset(-10800);
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(f.args.lock().unwrap().len(), 1, "clock updates do not duplicate the watcher");
    engine.shutdown();
}

#[tokio::test]
async fn signing_out_clears_a_previously_confirmed_subscription() {
    let dir = support::DataFolder::new();
    let (connected, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
    let (disconnected, _) = status(Some(r#"{"loggedIn":false}"#));
    let calls = Arc::new(AtomicU64::new(0));
    let spawn: Spawner =
        Arc::new(
            move |bin, args, cwd| {
                if calls.fetch_add(1, Ordering::SeqCst) == 0 { connected(bin, args, cwd) } else { disconnected(bin, args, cwd) }
            },
        );
    let (engine, _, _) = start(&dir, spawn, None);
    engine.check_plans(FIVE).await;
    assert_eq!(engine.ledger().claude.billing, Billing::Subscription);
    engine.check_plans(FIVE).await;
    assert_eq!(engine.ledger().claude.billing, Billing::Unknown);
    engine.shutdown();
}

#[test]
fn work_size_is_saved_and_never_lowered_by_a_plan_rewrite() {
    use faber_engine::progress::plan;
    let steps = json!([{ "text": "Check the change", "state": "in_progress" }]);
    let mut before = None;
    for size in ["small", "in_one_area", "across_areas", "security"] {
        let next = plan(before.as_ref(), &json!({"steps": steps, "size": size}), 1).unwrap().unwrap();
        assert_eq!(serde_json::to_value(&next).unwrap()["size"], size);
        before = Some(next);
    }
    let next = plan(before.as_ref(), &json!({"steps": steps, "size": "small"}), 2).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&next).unwrap()["size"], "security");
    assert_eq!(serde_json::to_value(plan(Some(&next), &json!({"steps": []}), 3).unwrap().unwrap()).unwrap()["size"], "security");
    let old = plan(None, &json!({"steps": steps}), 1).unwrap().unwrap();
    assert_eq!(serde_json::to_value(old).unwrap()["size"], "small");
}

#[test]
fn work_size_boundaries_are_pure_and_conservative() {
    use faber_engine::model::WorkSize::*;
    use faber_engine::permissions::work_boundary;
    let data = vec!["C:/faber-data".into()];
    let credentials = vec!["C:/moved-login".into()];
    for (tool, input, copy, expected) in [
        ("Read", json!({"file_path":"src/main.rs"}), false, (Small, "local")),
        ("Read", json!({"file_path":"C:/faber-data/conversations/a.json"}), false, (Security, "data")),
        ("Write", json!({"file_path":"../elsewhere/a.txt"}), true, (AcrossAreas, "outside")),
        ("Read", json!({"file_path":"C:/moved-login/token"}), false, (Security, "credentials")),
        ("Read", json!({"file_path":".ssh/id_rsa"}), false, (Security, "credentials")),
        ("Read", json!({"file_path":"auth.json"}), false, (Security, "credentials")),
        ("Read", json!({"file_path":"private.pem"}), false, (Security, "credentials")),
        ("MultiEdit", json!({"file_path":"src/main.rs","edits":[]}), false, (Small, "local")),
        ("item/fileChange/requestApproval", json!({"changes":[{"path":"src/main.rs"}],"cwd":"C:/project"}), false, (Small, "local")),
        ("item/fileChange/requestApproval", json!({"changes":[{"path":"src/main.rs"}],"cwd":"C:/other"}), false, (AcrossAreas, "outside")),
        ("item/fileChange/requestApproval", json!([{"path":"src/main.rs"}]), false, (Security, "unknown")),
        (
            "item/fileChange/requestApproval",
            json!({"changes":[{"path":"src/old.rs","kind":{"type":"delete"}}],"cwd":"C:/project"}),
            false,
            (Security, "delete"),
        ),
        ("Bash", json!({"command":"npm publish"}), false, (Security, "publish")),
        ("Bash", json!({"command":"rm src/old.rs"}), false, (Security, "delete")),
        ("Bash", json!({"command":"rm src/old.rs"}), true, (InOneArea, "edit")),
        ("Bash", json!({"command":"rm ../other/old.rs"}), true, (Security, "delete")),
        ("Bash", json!({"command":"cat src/main.rs"}), false, (Small, "local")),
        ("PowerShell", json!({"command":"Get-Content C:\\faber-data\\settings.json"}), false, (Security, "data")),
        ("Bash", json!({"command":"cat src/main.rs; curl example.com"}), false, (Security, "unknown")),
        ("Bash", json!({"command":"rg --pre evil text"}), false, (Security, "unknown")),
        ("Bash", json!({"command":"pwd", "cwd":"../other"}), false, (AcrossAreas, "outside")),
        ("unknown-tool", json!({}), false, (Security, "unknown")),
        ("item/commandExecution/requestApproval", json!("cat ordinary.txt"), false, (Security, "unknown")),
    ] {
        assert_eq!(work_boundary(tool, &input, "C:/project", &data, &credentials, copy), expected, "{tool}: {input}");
    }
    assert_eq!(
        work_boundary("Read", &json!({"file_path":"src/main.rs"}), "C:/faber-data/copies/project", &data, &credentials, true),
        (Small, "local")
    );
    let call = json!({"command":"cat src/main.rs"});
    let at = std::time::Instant::now();
    for _ in 0..100_000 {
        std::hint::black_box(work_boundary("Bash", std::hint::black_box(&call), "C:/project", &data, &credentials, false));
    }
    eprintln!("work_boundary: {:.2} us/call, 100000 literal reads, no disk I/O", at.elapsed().as_secs_f64() * 10.0);
}

async fn work_plan_call(pipe: &mut support::Pipe, request: &str, input: serde_json::Value) {
    pipe.say(json!({"type":"control_request","request_id":request,"request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"plan","arguments":input}}}}))
        .await;
    support::until("plan call was not answered", || pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == request))
        .await;
}

#[tokio::test]
async fn work_size_holds_a_call_until_a_fresh_contract_then_keeps_native_permission_and_stop() {
    use faber_engine::model::{Area, Permissions, Settings, WorkSize};
    use faber_engine::router::Choice;
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let (spawn, made) = support::pipes();
    let (engine, _, _) = start(&dir, spawn, None);
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Default::default() }).unwrap();
    engine.note_account_events(
        Vendor::Claude,
        vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(Billing::Subscription),
            ..Default::default()
        })],
    );
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    let id = engine
        .send(faber_engine::SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "Check the change".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&made).await;
    support::until("turn was not sent", || pipe.lines.lock().unwrap().iter().any(|m| m["type"] == "user")).await;
    pipe.say(json!({"type":"system","subtype":"init","session_id":"s","model":"claude-haiku-5-5"})).await;
    let steps = json!([{"text":"Check the change","state":"in_progress"}]);
    let contract = json!({"extends":"Existing editor","proof":"Run editor checks"});
    work_plan_call(&mut pipe, "p1", json!({"steps":steps,"size":"in_one_area","contract":contract})).await;
    pipe.say(json!({"type":"control_request","request_id":"outside","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"../other.txt","content":"ok"}}})).await;
    support::until("boundary was not raised", || engine.open(&id).unwrap().plan.is_some_and(|p| p.size == WorkSize::AcrossAreas)).await;
    let c = engine.open(&id).unwrap();
    let reply = c.messages.last().unwrap();
    let turn = reply.id.clone();
    let card = reply.permissions.as_ref().unwrap()[0].clone();
    assert_eq!(card.tool, "AskUserQuestion");
    assert_eq!(card.input["workContract"]["reason"], "outside");
    assert!(!pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "outside"));
    // A native CLI cannot author the contract until its original call returns unexecuted.
    assert!(
        !pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "outside"),
        "stating intent never authorizes the call"
    );
    work_plan_call(&mut pipe, "p2", json!({"size":"small"})).await;
    assert_eq!(engine.open(&id).unwrap().messages.last().unwrap().permissions.as_ref().unwrap()[0].tool, "AskUserQuestion");
    engine.set_keep_going_pace(0.0001);
    engine.keep_going(&id, true).unwrap();
    support::until("default contract choice did not return the native call", || {
        pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "outside")
    })
    .await;
    support::until("contract question was not settled", || {
        engine.open(&id).unwrap().messages.last().unwrap().permissions.iter().flatten().all(|p| p.request_id != card.request_id)
    })
    .await;
    engine.keep_going(&id, false).unwrap();
    assert!(
        engine.open(&id).unwrap().messages.iter().all(|m| m.auto_answers.as_ref().is_none_or(Vec::is_empty)),
        "synthetic contract keys never become visible answers"
    );
    assert!(
        pipe.lines
            .lock()
            .unwrap()
            .iter()
            .any(|m| m["response"]["request_id"] == "outside" && m["response"]["response"]["behavior"] == "deny")
    );
    work_plan_call(&mut pipe, "p3", json!({"contract":contract})).await;
    pipe.say(json!({"type":"control_request","request_id":"outside-retry","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"../other.txt","content":"ok"}}})).await;
    support::until("retry did not return to native permissions", || {
        engine.open(&id).unwrap().messages.last().unwrap().permissions.iter().flatten().any(|p| p.request_id == "outside-retry")
    })
    .await;
    let c = engine.open(&id).unwrap();
    let native = c.messages.last().unwrap().permissions.as_ref().unwrap();
    assert_eq!((native.len(), native[0].tool.as_str(), native[0].request_id.as_str()), (1, "Write", "outside-retry"));
    engine.answer(&id, &turn, "outside-retry", false, false).await.unwrap();
    pipe.say(json!({"type":"control_request","request_id":"opaque","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"pwd; whoami"}}})).await;
    support::until("unknown syntax was not raised", || engine.open(&id).unwrap().plan.is_some_and(|p| p.size == WorkSize::Security)).await;
    engine.interrupt(&id);
    assert!(engine.open(&id).unwrap().messages.last().unwrap().permissions.as_ref().is_none_or(Vec::is_empty));
    work_plan_call(&mut pipe, "late-contract", json!({"contract":contract})).await;
    assert!(
        !pipe
            .lines
            .lock()
            .unwrap()
            .iter()
            .any(|m| m["response"]["request_id"] == "opaque" && m["response"]["response"]["behavior"] == "allow"),
        "a late contract after Stop cannot release the call"
    );
    assert!(engine.answer_question(&id, &turn, &card.request_id, json!({})).await.is_err());
    engine.shutdown();
}
