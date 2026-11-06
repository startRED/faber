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
async fn a_signed_in_account_reads_current_limits_without_a_user_or_model_turn() {
    let dir = support::DataFolder::new();
    // Synthetic replay of SDK 0.3.312's get_usage contract, not an authenticated recording.
    let f = fake(support::fixture("claude", "usage-control"));
    let (auth, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
    let quota = f.spawn.clone();
    let spawn: Spawner =
        Arc::new(move |bin, args, cwd| if args == ["auth", "status"] { auth(bin, args, cwd) } else { quota(bin, args, cwd) });
    let (engine, _, _) = start(&dir, spawn, None);
    engine.check_plans(FIVE).await;
    let usage = engine.state().usage.remove(0);
    assert_eq!(usage.plan.as_deref(), Some("max"));
    assert_eq!(usage.windows.len(), 2, "a signed-in subscriber needs no prior model response");
    assert_eq!(usage.windows.iter().find(|w| w.name == "seven_day").unwrap().used, 0.31);
    assert_eq!(engine.ledger().claude.zero_turn_readings.len(), 2);
    assert!(engine.ledger().claude.own_spend.is_empty());
    assert!(f.wire.messages().iter().all(|m| m["type"] == "control_request"));
    assert_eq!(f.wire.messages()[1]["request"]["skip_behaviors"], true);
    assert!(f.mismatches.lock().unwrap().is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn failed_or_unsolicited_control_answers_cannot_publish_quota() {
    for bad in [
        json!({"type":"control_response","response":{"subtype":"error","request_id":"limits-usage","error":"unsupported"}}),
        json!({"type":"control_response","response":{"subtype":"success","request_id":"unrelated","response":{}}}),
        json!({"type":"assistant","message":{"content":[]}}),
        json!({"type":"control_request","request_id":"permission","request":{"subtype":"can_use_tool"}}),
    ] {
        let dir = support::DataFolder::new();
        let mut entries = support::fixture("claude", "usage-control");
        entries.last_mut().unwrap().msg = bad;
        let f = fake(entries);
        let (auth, _) = status(Some(r#"{"loggedIn":true,"subscriptionType":"max"}"#));
        let quota = f.spawn.clone();
        let spawn: Spawner =
            Arc::new(move |bin, args, cwd| if args == ["auth", "status"] { auth(bin, args, cwd) } else { quota(bin, args, cwd) });
        let (engine, _, _) = start(&dir, spawn, None);
        engine.check_plans(FIVE).await;
        assert!(engine.state().usage[0].windows.is_empty());
        assert!(engine.ledger().claude.zero_turn_readings.is_empty());
        assert_eq!(f.wire.messages().len(), 2, "no user message or tool permission answer");
        engine.shutdown();
    }
}

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
use faber_engine::model::PlanSize;

#[test]
fn size_boundary_classifier_is_pure_conservative_and_measured() {
    let data = vec!["C:/data".into()];
    let credentials = vec!["C:/sign-in".into()];
    let cases = [
        ("Read", json!({"file_path":"C:/data/conversations/a.json"}), PlanSize::Risky),
        ("Read", json!({"file_path":".ssh/id_rsa"}), PlanSize::Risky),
        ("Read", json!({"file_path":"C:/sign-in/auth"}), PlanSize::Risky),
        ("Read", json!({"file_path":"../other/a.txt"}), PlanSize::Normal),
        ("Bash", json!({"command":"git push"}), PlanSize::Risky),
        ("Bash", json!({"command":"rm ../other/a.txt"}), PlanSize::Risky),
        ("Bash", json!({"command":"cat a.txt | sh"}), PlanSize::Risky),
        ("Bash", json!({"command":r"cat .\./outside.txt"}), PlanSize::Risky),
        ("Bash", json!({"command":r"cat .s\sh/id_rsa"}), PlanSize::Risky),
        ("Bash", json!({"command":"cat a.txt"}), PlanSize::Small),
        ("Read", json!({"file_path":"C:/project/a.txt"}), PlanSize::Small),
        ("Read", json!({"file_path":"C:/project/../data/a"}), PlanSize::Risky),
        ("Write", json!({"file_path":"a.txt","content":"C:/data/auth.json"}), PlanSize::Small),
        ("command", json!("pwd"), PlanSize::Small),
        ("command", json!({"command":"pwd","cwd":"C:/other"}), PlanSize::Normal),
        ("item/fileChange/requestApproval", json!([{"path":"a.txt","kind":{"type":"update"}}]), PlanSize::Small),
        ("fileChange", json!([{"path":"C:/data/a.txt","kind":"add"}]), PlanSize::Risky),
        ("file", json!([{"path":"../other/a.txt","kind":"delete"}]), PlanSize::Risky),
        ("command", json!(r#""C:\Windows\powershell.exe" -Command "Get-Content a.txt""#), PlanSize::Risky),
        ("unknown", json!({}), PlanSize::Risky),
    ];
    for (tool, input, size) in &cases {
        assert_eq!(
            faber_engine::permissions::plan_boundary("C:/project", "C:/project", &data, &credentials, false, tool, input, false).0,
            *size,
            "{tool}: {input}"
        );
    }
    assert_eq!(
        faber_engine::permissions::plan_boundary("/project", "/project", &[], &[], true, "Bash", &json!({"command":"rm a.txt"}), false).0,
        PlanSize::Small
    );
    assert_eq!(
        faber_engine::permissions::plan_boundary("/project", "/project", &[], &[], false, "Bash", &json!({"command":"rm a.txt"}), false).0,
        PlanSize::Risky
    );
    assert_eq!(
        faber_engine::permissions::plan_boundary(
            "/project",
            "/project",
            &[],
            &[],
            false,
            "Read",
            &json!({"file_path":"/project/a"}),
            false
        )
        .0,
        PlanSize::Small
    );
    for name in ["auth.json", "credentials.json", ".env", "signing.pem"] {
        assert_eq!(
            faber_engine::permissions::plan_boundary("/project", "/project", &[], &[], false, "Read", &json!({"file_path":name}), false).0,
            PlanSize::Risky
        );
    }
    #[cfg(not(windows))]
    assert_eq!(
        faber_engine::permissions::plan_boundary(
            "/project",
            "/project",
            &[],
            &[],
            false,
            "Read",
            &json!({"file_path":"/PROJECT/a"}),
            false
        )
        .0,
        PlanSize::Normal
    );
    let start = std::time::Instant::now();
    let count = 100_000;
    for n in 0..count {
        let (tool, input, _) = &cases[n % cases.len()];
        std::hint::black_box(faber_engine::permissions::plan_boundary(
            "C:/project",
            "C:/project",
            &data,
            &credentials,
            false,
            tool,
            std::hint::black_box(input),
            false,
        ));
    }
    eprintln!(
        "size_boundary_classifier: {:.0} ns/call over {count} mixed approvals (debug build)",
        start.elapsed().as_nanos() as f64 / count as f64
    );
}

#[test]
fn size_boundary_plan_input_defaults_and_never_lowers_or_expands_an_old_contract() {
    let input = json!({"steps":[{"text":"Check","state":"in_progress"}]});
    let first = faber_engine::progress::plan(None, &input, 1).unwrap().unwrap();
    assert_eq!(first.size, PlanSize::Small);
    let normal = faber_engine::progress::plan(
        Some(&first),
        &json!({"size":"normal", "contract":{"extends":"Current feature","proof":"Run its checks"}}),
        2,
    )
    .unwrap()
    .unwrap();
    assert!(normal.covers(PlanSize::Normal));
    let risky = faber_engine::progress::plan(Some(&normal), &json!({"size":"risky"}), 3).unwrap().unwrap();
    assert!(!risky.covers(PlanSize::Risky));
    for input in [json!({"size":"small"}), json!({"steps":[]}), input] {
        assert_eq!(faber_engine::progress::plan(Some(&risky), &input, 4).unwrap().unwrap().size, PlanSize::Risky);
    }
    let cleared = faber_engine::progress::plan(
        Some(&normal),
        &json!({"steps":[],"size":"risky","contract":{"extends":"Current feature","proof":"Run checks"}}),
        5,
    )
    .unwrap()
    .unwrap();
    assert!(cleared.steps.is_empty() && cleared.size == PlanSize::Risky && cleared.covers(PlanSize::Risky));
    assert!(faber_engine::progress::plan(None, &json!({"size":"huge","steps":[]}), 1).is_err());
    assert_eq!(serde_json::from_value::<faber_engine::model::Plan>(json!({"steps":[],"at":1})).unwrap().size, PlanSize::Small);
}

async fn boundary_started() -> (support::DataFolder, tempfile::TempDir, Engine, support::Pipe, String) {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let (spawn, made) = support::pipes();
    let (engine, _, _) = start(&dir, spawn, Some(Arc::new(|_| Box::pin(async { None }))));
    engine
        .save_settings(faber_engine::model::Settings { permissions: Some(faber_engine::model::Permissions::Ask), ..Default::default() })
        .unwrap();
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    let id = engine
        .send(faber_engine::SendRequest {
            conversation_id: None,
            area: faber_engine::model::Area::Project,
            project_id: Some(project.id),
            text: "Check this project".into(),
            images: vec![],
            files: vec![],
            choice: faber_engine::router::Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let mut pipe = support::next_pipe(&made).await;
    support::until("first turn missing", || pipe.lines.lock().unwrap().iter().any(|m| m["type"] == "user")).await;
    pipe.say(json!({"type":"system","subtype":"init","session_id":"boundary","model":"claude-haiku-5-5"})).await;
    boundary_plan(&mut pipe, "initial-plan", json!({"steps":[{"text":"Check","state":"in_progress"}]})).await;
    (dir, folder, engine, pipe, id)
}

async fn boundary_plan(pipe: &mut support::Pipe, request: &str, input: serde_json::Value) {
    pipe.say(json!({"type":"control_request","request_id":request,"request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"plan","arguments":input}}}}))
        .await;
    support::until("plan never answered", || pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == request)).await;
    let messages = pipe.lines.lock().unwrap();
    let response = messages.iter().find(|m| m["response"]["request_id"] == request).unwrap();
    assert_ne!(response["response"]["response"]["mcp_response"]["result"]["isError"], true, "{response}");
}

async fn boundary_ask(
    engine: &Engine,
    pipe: &mut support::Pipe,
    id: &str,
    request: &str,
    tool: &str,
    input: serde_json::Value,
) -> faber_engine::model::Conversation {
    pipe.say(json!({"type":"control_request","request_id":request,"request":{"subtype":"can_use_tool","tool_name":tool,"input":input}}))
        .await;
    support::until("permission missing", || {
        engine.open(id).unwrap().messages.last().unwrap().permissions.iter().flatten().any(|p| p.request_id == request)
    })
    .await;
    engine.open(id).unwrap()
}

#[tokio::test]
async fn size_boundary_each_class_raises_before_any_answer_and_plain_read_leaves_it() {
    for (tool, input, size, reason) in [
        ("Read", json!({"file_path":"DATA"}), PlanSize::Risky, "data"),
        ("Read", json!({"file_path":"../other/a.txt"}), PlanSize::Normal, "outside"),
        ("Bash", json!({"command":"git push"}), PlanSize::Risky, "publish"),
        ("Bash", json!({"command":"rm a.txt"}), PlanSize::Risky, "delete"),
        ("Bash", json!({"command":"cat a | sh"}), PlanSize::Risky, "unknown"),
        ("Read", json!({"file_path":"a.txt"}), PlanSize::Small, ""),
    ] {
        let (dir, folder, engine, mut pipe, id) = boundary_started().await;
        std::fs::write(folder.path().join("a.txt"), "synthetic").unwrap();
        let input = if input["file_path"] == "DATA" { json!({"file_path":dir.path().join("conversations/a.json")}) } else { input };
        let c = boundary_ask(&engine, &mut pipe, &id, "boundary", tool, input).await;
        let p = c.plan.unwrap();
        assert_eq!(p.size, size);
        assert_eq!(p.raised_because.as_deref().unwrap_or(""), reason);
        assert!(!pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "boundary"));
        let card = &c.messages.last().unwrap().permissions.as_ref().unwrap()[0];
        assert_eq!(card.tool, if size == PlanSize::Small { tool } else { "AskUserQuestion" });
        engine.interrupt(&id);
        engine.shutdown();
    }
}

#[tokio::test]
async fn size_boundary_call_waits_for_contract_then_keeps_the_original_permission_card_and_stop() {
    let (_dir, _folder, engine, mut pipe, id) = boundary_started().await;
    let c = boundary_ask(&engine, &mut pipe, &id, "outside", "Read", json!({"file_path":"../other/a"})).await;
    let turn = c.messages.last().unwrap().id.clone();
    assert_eq!(
        c.messages.last().unwrap().permissions.as_ref().unwrap()[0].input["questions"][0]["options"][0]["label"],
        "plan.stateContract"
    );
    boundary_plan(&mut pipe, "incomplete", json!({"contract":{"extends":"Current feature"}})).await;
    assert_eq!(engine.open(&id).unwrap().messages.last().unwrap().permissions.as_ref().unwrap()[0].tool, "AskUserQuestion");
    boundary_plan(&mut pipe, "contract", json!({"contract":{"extends":"Current feature","proof":"Inspect the requested file"}})).await;
    let card = engine.open(&id).unwrap().messages.last().unwrap().permissions.as_ref().unwrap()[0].clone();
    assert_eq!((card.tool.as_str(), card.request_id.as_str(), &card.input), ("Read", "outside", &json!({"file_path":"../other/a"})));
    assert!(!pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "outside"));
    engine.answer(&id, &turn, "outside", true, false).await.unwrap();
    assert_eq!(
        pipe.lines
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m["response"]["request_id"] == "outside" && m["response"]["response"]["behavior"] == "allow")
            .count(),
        1
    );
    let c = boundary_ask(&engine, &mut pipe, &id, "risky", "Bash", json!({"command":"git push"})).await;
    assert_eq!(c.plan.unwrap().size, PlanSize::Risky);
    assert_eq!(
        c.messages.last().unwrap().permissions.as_ref().unwrap()[0].tool,
        "AskUserQuestion",
        "a normal contract does not cover risky"
    );
    engine.interrupt(&id);
    assert!(engine.answer_question(&id, &turn, "risky", json!({})).await.is_err());
    engine.shutdown();
}

#[tokio::test]
async fn size_boundary_default_states_the_contract_instead_of_allowing_the_call() {
    let (_dir, _folder, engine, mut pipe, id) = boundary_started().await;
    let c = boundary_ask(&engine, &mut pipe, &id, "outside", "Read", json!({"file_path":"../other/a"})).await;
    let turn = c.messages.last().unwrap().id.clone();
    engine.answer_question(&id, &turn, "outside", json!({"plan.contractQuestion":"plan.stateContract"})).await.unwrap();
    let messages = pipe.lines.lock().unwrap();
    let result = &messages.iter().find(|m| m["response"]["request_id"] == "outside").unwrap()["response"]["response"];
    assert_eq!(result["behavior"], "deny");
    assert!(result["message"].as_str().unwrap().contains("plan tool"));
    assert!(!engine.open(&id).unwrap().plan.unwrap().covers(PlanSize::Normal));
    drop(messages);
    engine.interrupt(&id);
    engine.shutdown();
}

#[tokio::test]
async fn size_boundary_skip_declines_without_instructing_a_retry() {
    let (_dir, _folder, engine, mut pipe, id) = boundary_started().await;
    let c = boundary_ask(&engine, &mut pipe, &id, "outside", "Read", json!({"file_path":"../other/a"})).await;
    engine.answer(&id, &c.messages.last().unwrap().id, "outside", false, false).await.unwrap();
    let messages = pipe.lines.lock().unwrap();
    let result = &messages.iter().find(|m| m["response"]["request_id"] == "outside").unwrap()["response"]["response"];
    assert_eq!(result["behavior"], "deny");
    assert_eq!(result["message"], "The user declined.");
    drop(messages);
    assert!(engine.open(&id).unwrap().queued.is_empty());
    engine.interrupt(&id);
    engine.shutdown();
}

#[tokio::test]
async fn size_boundary_git_metadata_git_reads_and_glob_escape_wait_for_a_contract() {
    for (tool, input) in [
        ("Write", json!({"file_path":".git/config","content":"[core]\nfsmonitor = helper"})),
        ("Bash", json!({"command":"git diff"})),
        ("Glob", json!({"path":".","pattern":"../../**"})),
        ("Glob", json!({"path":".","pattern":"{..,src}/*"})),
        ("PowerShell", json!({"command":"Get-Content env:GH_TOKEN"})),
    ] {
        let (_dir, _folder, engine, mut pipe, id) = boundary_started().await;
        let c = boundary_ask(&engine, &mut pipe, &id, "unsafe", tool, input).await;
        assert!(c.plan.unwrap().size > PlanSize::Small);
        assert_eq!(c.messages.last().unwrap().permissions.as_ref().unwrap()[0].tool, "AskUserQuestion");
        assert!(!pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "unsafe"));
        engine.interrupt(&id);
        engine.shutdown();
    }
}

#[tokio::test]
async fn size_boundary_existing_links_are_risky_even_for_missing_children_and_parent_traversal() {
    #[allow(unused_mut)]
    let mut cases = vec![
        ("Write", json!({"file_path":"link/new.txt","content":"ordinary text"})),
        ("Bash", json!({"command":"rm -rf link/victim"})),
        ("Read", json!({"file_path":"link/../victim"})),
        ("Write", json!({"file_path":"missing/../link/new.txt","content":"ordinary"})),
    ];
    #[cfg(unix)]
    cases.push(("Write", json!({"file_path":"*/link/new.txt","content":"ordinary"})));
    for (tool, input) in cases {
        let (_dir, folder, engine, mut pipe, id) = boundary_started().await;
        let outside = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            std::fs::create_dir(folder.path().join("*")).unwrap();
            std::os::unix::fs::symlink(outside.path(), folder.path().join("*/link")).unwrap();
        }
        #[cfg(windows)]
        junction::create(outside.path(), folder.path().join("link")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), folder.path().join("link")).unwrap();
        let c = boundary_ask(&engine, &mut pipe, &id, "linked", tool, input).await;
        let plan = c.plan.unwrap();
        assert_eq!((plan.size, plan.raised_because.as_deref()), (PlanSize::Risky, Some("outside")));
        assert_eq!(c.messages.last().unwrap().permissions.as_ref().unwrap()[0].tool, "AskUserQuestion");
        engine.interrupt(&id);
        engine.shutdown();
    }
}

#[tokio::test]
async fn size_boundary_network_paths_and_probe_limits_wait_for_a_contract() {
    for (tool, input) in [
        ("Read", json!({"file_path":"//attacker/share/x"})),
        ("Read", json!({"file_path":r"\attacker\s"})),
        ("command", json!({"command":"cat x","cwd":"//attacker/share"})),
        ("command", json!({"command":"cat x","cwd":r"\attacker\s"})),
        ("Bash", json!({"command":format!("cat {}", "x ".repeat(8_000))})),
        ("Read", json!({"file_path":"a/".repeat(65)})),
    ] {
        let (_dir, _folder, engine, mut pipe, id) = boundary_started().await;
        let start = std::time::Instant::now();
        let c = boundary_ask(&engine, &mut pipe, &id, "bounded", tool, input).await;
        assert_eq!(c.plan.unwrap().size, PlanSize::Risky);
        assert_eq!(c.messages.last().unwrap().permissions.as_ref().unwrap()[0].tool, "AskUserQuestion");
        eprintln!("size_boundary_network_or_limit: {:.3} ms/call", start.elapsed().as_secs_f64() * 1_000.0);
        engine.interrupt(&id);
        engine.shutdown();
    }
}

#[tokio::test]
async fn size_boundary_directory_searches_require_risky_even_with_a_normal_contract() {
    for (tool, input) in [
        ("Grep", json!({"path":"src","pattern":"."})),
        ("Grep", json!({"pattern":"."})),
        ("Glob", json!({"path":"src","pattern":"*.rs"})),
        ("Glob", json!({"pattern":"src/**/*.rs"})),
        ("LS", json!({"path":"src"})),
        ("LS", json!({})),
        ("Bash", json!({"command":"rg . src"})),
        ("Bash", json!({"command":"rg ."})),
        ("Bash", json!({"command":"grep -r . src"})),
        ("Bash", json!({"command":"grep -R . src"})),
        ("Bash", json!({"command":"find src"})),
        ("Bash", json!({"command":"ls -R src"})),
        ("Bash", json!({"command":"ls --recursive src"})),
        ("PowerShell", json!({"command":"Get-ChildItem -Recurse src"})),
    ] {
        let (_dir, folder, engine, mut pipe, id) = boundary_started().await;
        std::fs::create_dir(folder.path().join("src")).unwrap();
        boundary_plan(&mut pipe, "normal", json!({"size":"normal","contract":{"extends":"Search files","proof":"Check results"}})).await;
        let c = boundary_ask(&engine, &mut pipe, &id, "directory", tool, input.clone()).await;
        let plan = c.plan.unwrap();
        assert_eq!((plan.size, plan.raised_because.as_deref()), (PlanSize::Risky, Some("unknown")), "{tool}: {input}");
        assert_eq!(c.messages.last().unwrap().permissions.as_ref().unwrap()[0].tool, "AskUserQuestion");
        assert!(!pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "directory"));
        engine.interrupt(&id);
        engine.shutdown();
    }
}

#[tokio::test]
async fn size_boundary_single_file_reads_and_searches_stay_small() {
    for (tool, input) in [
        ("Read", json!({"file_path":"plain.txt"})),
        ("Bash", json!({"command":"cat plain.txt"})),
        ("Bash", json!({"command":"rg . plain.txt"})),
        ("Bash", json!({"command":"grep . plain.txt"})),
        ("Grep", json!({"path":"plain.txt","pattern":"."})),
    ] {
        let (_dir, folder, engine, mut pipe, id) = boundary_started().await;
        std::fs::write(folder.path().join("plain.txt"), "synthetic").unwrap();
        let c = boundary_ask(&engine, &mut pipe, &id, "file", tool, input.clone()).await;
        assert_eq!(c.plan.unwrap().size, PlanSize::Small, "{tool}: {input}");
        assert_eq!(c.messages.last().unwrap().permissions.as_ref().unwrap()[0].tool, tool);
        engine.interrupt(&id);
        engine.shutdown();
    }
}

#[cfg(unix)]
#[tokio::test]
async fn size_boundary_unix_literal_backslash_cannot_disguise_an_external_write() {
    let (_dir, folder, engine, mut pipe, id) = boundary_started().await;
    let outside = tempfile::Builder::new().prefix("boundary-outside-").suffix(r"\..").tempdir_in(folder.path().parent().unwrap()).unwrap();
    std::fs::create_dir(outside.path().join(folder.path().file_name().unwrap())).unwrap();
    let raw = format!(
        "../{}/{}/report.txt",
        outside.path().file_name().unwrap().to_str().unwrap(),
        folder.path().file_name().unwrap().to_str().unwrap()
    );
    let c = boundary_ask(&engine, &mut pipe, &id, "literal", "Write", json!({"file_path":raw,"content":"ordinary"})).await;
    assert_eq!(c.plan.unwrap().size, PlanSize::Risky);
    engine.interrupt(&id);
    engine.shutdown();
}

#[tokio::test]
async fn size_boundary_without_a_current_turn_denies_instead_of_hanging() {
    for covered in [false, true] {
        let (_dir, _folder, engine, mut pipe, id) = boundary_started().await;
        if covered {
            boundary_plan(&mut pipe, "covered", json!({"size":"risky","contract":{"extends":"Current feature","proof":"Run checks"}}))
                .await;
        }
        pipe.say(json!({"type":"result","subtype":"error_during_execution","is_error":true,"session_id":"boundary"})).await;
        support::until("turn did not end", || {
            engine.open(&id).unwrap().messages.last().unwrap().status != Some(faber_engine::model::Status::Streaming)
        })
        .await;
        pipe.say(json!({"type":"control_request","request_id":"late","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"git diff"}}})).await;
        support::until("late approval request was never answered", || {
            pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "late")
        })
        .await;
        let lines = pipe.lines.lock().unwrap();
        assert_eq!(lines.iter().find(|m| m["response"]["request_id"] == "late").unwrap()["response"]["response"]["behavior"], "deny");
        drop(lines);
        assert!(engine.open(&id).unwrap().messages.iter().all(|m| m.permissions.iter().flatten().all(|p| p.request_id != "late")));
        engine.shutdown();
    }
}

#[tokio::test]
async fn size_boundary_credentials_still_refuse_silently_without_a_contract_card() {
    let (_dir, _folder, engine, mut pipe, id) = boundary_started().await;
    pipe.say(json!({"type":"control_request","request_id":"secret","request":{"subtype":"can_use_tool","tool_name":"Read","input":{"file_path":".ssh/id_rsa"}}})).await;
    support::until("credential request was never refused", || {
        pipe.lines.lock().unwrap().iter().any(|m| m["response"]["request_id"] == "secret")
    })
    .await;
    let lines = pipe.lines.lock().unwrap();
    assert_eq!(lines.iter().find(|m| m["response"]["request_id"] == "secret").unwrap()["response"]["response"]["behavior"], "deny");
    drop(lines);
    let c = engine.open(&id).unwrap();
    assert_eq!(c.plan.unwrap().size, PlanSize::Small);
    assert!(c.messages.iter().all(|m| m.permissions.iter().flatten().all(|p| p.request_id != "secret")));
    engine.interrupt(&id);
    engine.shutdown();
}
