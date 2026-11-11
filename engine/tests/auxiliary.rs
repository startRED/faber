//! Auxiliary work through the real drivers and in-memory fake CLIs, never a model service.
use crate::support::{self, fake, fixture};
use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest, WarmRequest};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

const QUESTION: &str = "In one sentence: why is the sky blue?";

fn engine(dir: &support::DataFolder, spawn: Spawner, vendor: Vendor) -> Engine {
    let engine = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::from([(vendor, Cli { command: "fake".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    engine.note_account_events(
        vendor,
        vec![EngineEvent::Signals(faber_engine::signals::Signals {
            billing: Some(faber_engine::budget::Billing::Subscription),
            ..Default::default()
        })],
    );
    engine.set_codex_models(vec!["gpt-6-luna".into(), "gpt-6.1-sol".into()], Some("gpt-6.1-sol".into()));
    engine
}

fn choice() -> Choice {
    Choice { vendor: "auto".into(), model: None, effort: None }
}

fn send(engine: &Engine) -> String {
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: QUESTION.into(),
            images: vec![],
            files: vec![],
            choice: choice(),
        })
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn router_cold_send_starts_at_once_without_an_auxiliary_request() {
    let dir = support::DataFolder::new();
    let f = fake(fixture("codex", "chat"));
    let engine = engine(&dir, f.spawn.clone(), Vendor::Codex);
    let start = Instant::now();
    let id = send(&engine);
    engine.rename(&id, "Router test");
    support::until("the default never started", || engine.open(&id).unwrap().messages[1].model.is_some()).await;
    assert!(start.elapsed() < Duration::from_millis(500), "a cold router must not hold the first send: {:?}", start.elapsed());
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!(reply.acts[0].why.as_deref(), Some("reason.autoImmediate"));
    assert!(!f.wire.messages().iter().any(|m| m["params"]["baseInstructions"].as_str().is_some_and(|s| s.contains("You classify"))));
    assert!(engine.ledger().codex.unmetered.is_empty());
    engine.shutdown();
}

/// The owner sent before Auto had finished judging and the answer ran on a fixed everyday model (2026-11-11): a first send
/// now waits for the classifier that is already reading its text, but never past the budget.
#[tokio::test(start_paused = true)]
async fn router_pending_draft_holds_a_first_send_only_for_its_budget() {
    let dir = support::DataFolder::new();
    let mut recording = fixture("codex", "chat");
    recording.insert(0, support::Entry { input: false, msg: serde_json::json!({"faber_hold": 3200}) });
    let f = fake(recording);
    let engine = engine(&dir, f.spawn.clone(), Vendor::Codex);
    engine.route_draft(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice() }, QUESTION.into());
    let start = Instant::now();
    let id = send(&engine);
    engine.rename(&id, "Router test");
    support::until("the pending draft held the send", || engine.open(&id).unwrap().messages[1].model.is_some()).await;
    let waited = start.elapsed();
    assert!(waited >= Duration::from_millis(2500) && waited < Duration::from_millis(3200), "{waited:?}");
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!(reply.reason.as_deref(), Some("reason.autoImmediate"));
    assert_eq!(reply.acts[0].why.as_deref(), Some("act.why.routerTimeout"));
    engine.shutdown();
}

/// A classifier's run that gives an easy chat verdict `hold` ms after its question.
fn verdict_after(hold: u64) -> Vec<support::Entry> {
    let verdict = r#"{"kind":"chat","difficulty":"easy","check":"glance","confidence":0.95}"#;
    let mut recording = fixture("codex", "chat");
    let end = recording.iter().position(|e| e.msg["method"] == "turn/completed").unwrap();
    recording.truncate(end + 1);
    let mut first = true;
    for entry in &mut recording {
        if entry.msg["method"] == "item/agentMessage/delta" {
            entry.msg["params"]["delta"] = if std::mem::take(&mut first) { verdict } else { "" }.into();
        }
        if entry.msg["method"] == "item/completed" && entry.msg["params"]["item"]["type"] == "agentMessage" {
            entry.msg["params"]["item"]["text"] = verdict.into();
        }
    }
    let at = recording.iter().position(|e| e.msg["method"] == "item/agentMessage/delta").unwrap();
    recording.insert(at, support::Entry { input: false, msg: serde_json::json!({"faber_hold": hold}) });
    recording
}

/// An engine whose classifier is `router` and whose answers come from the recorded chat.
fn routed_engine(dir: &support::DataFolder, router: &support::Fake) -> Engine {
    let answer = fake(fixture("codex", "chat"));
    let (r, a) = (router.spawn.clone(), answer.spawn.clone());
    engine(
        dir,
        Arc::new(move |bin, args, cwd| if args.iter().any(|a| a == "mcp_servers={}") { r(bin, args, cwd) } else { a(bin, args, cwd) }),
        Vendor::Codex,
    )
}

#[tokio::test]
async fn router_pending_draft_answers_a_first_send_inside_the_budget() {
    let dir = support::DataFolder::new();
    let router = fake(verdict_after(1200));
    let engine = routed_engine(&dir, &router);
    engine.route_draft(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice() }, QUESTION.into());
    support::until("the draft question was not sent", || router.wire.lines_with("turn/start") == 1).await;
    let id = send(&engine);
    engine.rename(&id, "Router test");
    support::until("the routed draft never started", || engine.open(&id).unwrap().messages[1].model.is_some()).await;
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!(reply.reason.as_deref(), Some("reason.route.chat.easy"), "the send used the verdict it waited for");
    assert_eq!(reply.model.as_deref(), Some("gpt-6-luna"));
    engine.shutdown();
}

#[tokio::test]
async fn router_draft_can_finish_after_the_old_send_deadline() {
    let dir = support::DataFolder::new();
    let router = fake(verdict_after(3200));
    let engine = routed_engine(&dir, &router);
    engine.route_draft(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice() }, QUESTION.into());
    support::until("the draft question was not sent", || router.wire.lines_with("turn/start") == 1).await;
    tokio::time::sleep(Duration::from_millis(3500)).await;
    let id = send(&engine);
    engine.rename(&id, "Router test");
    support::until("the routed draft never started", || engine.open(&id).unwrap().messages[1].model.is_some()).await;
    let reply = engine.open(&id).unwrap().messages[1].clone();
    assert_eq!(reply.reason.as_deref(), Some("reason.route.chat.easy"));
    assert_eq!(reply.model.as_deref(), Some("gpt-6-luna"));
    assert!(engine.ledger().codex.unmetered.is_empty());
    assert!(router.mismatches.lock().unwrap().is_empty(), "{:?}", router.mismatches.lock().unwrap());
    engine.shutdown();
}

#[tokio::test]
async fn title_cheapest_models_use_low_effort_and_known_ledger_spend() {
    for (vendor, model) in [(Vendor::Codex, "gpt-6-luna"), (Vendor::Claude, "claude-haiku-5-5")] {
        let dir = support::DataFolder::new();
        let main_recording = fixture(vendor.name(), "chat");
        let end = main_recording.iter().position(|e| e.msg["method"] == "turn/completed" || e.msg["type"] == "result").unwrap();
        let mut title_recording = main_recording[..=end].to_vec();
        let assistant = if vendor == Vendor::Codex {
            title_recording
                .iter()
                .filter(|e| e.msg["method"] == "item/agentMessage/delta")
                .filter_map(|e| e.msg["params"]["delta"].as_str())
                .collect::<String>()
        } else {
            title_recording[end].msg["result"].as_str().unwrap().to_string()
        };
        let prompt =
            format!("Settings language: en\nExchange (user, assistant): {}", serde_json::to_string(&[QUESTION, &assistant]).unwrap());
        let mut first = true;
        for entry in &mut title_recording {
            if entry.input && entry.msg["method"] == "turn/start" {
                entry.msg["params"]["input"][0]["text"] = prompt.clone().into();
            }
            if entry.input && entry.msg["type"] == "user" {
                entry.msg["message"]["content"] = prompt.clone().into();
            }
            if entry.msg["method"] == "item/agentMessage/delta" {
                entry.msg["params"]["delta"] = if std::mem::take(&mut first) { "Blue sky" } else { "" }.into();
            }
            if entry.msg["method"] == "item/completed" && entry.msg["params"]["item"]["type"] == "agentMessage" {
                entry.msg["params"]["item"]["text"] = "Blue sky".into();
            }
            if entry.msg["type"] == "stream_event" && entry.msg["event"]["delta"]["type"] == "text_delta" {
                entry.msg["event"]["delta"]["text"] = if std::mem::take(&mut first) { "Blue sky" } else { "" }.into();
            }
            if entry.msg["type"] == "assistant" {
                entry.msg["message"]["content"] = serde_json::json!([{"type":"text","text":"Blue sky"}]);
            }
            if entry.msg["type"] == "result" {
                entry.msg["result"] = "Blue sky".into();
            }
        }
        let main = fake(main_recording);
        let title = fake(title_recording);
        let (m, t) = (main.spawn.clone(), title.spawn.clone());
        let engine = engine(
            &dir,
            Arc::new(move |bin, args, cwd| {
                if args.iter().any(|a| a == "mcp_servers={}" || a == "--system-prompt") { t(bin, args, cwd) } else { m(bin, args, cwd) }
            }),
            vendor,
        );
        let id = engine
            .send(SendRequest {
                conversation_id: None,
                area: Area::Chat,
                project_id: None,
                text: QUESTION.into(),
                images: vec![],
                files: vec![],
                choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
            })
            .unwrap();
        support::until("the title never arrived", || engine.open(&id).unwrap().title == "Blue sky").await;
        if vendor == Vendor::Codex {
            let messages = title.wire.messages();
            let open = messages.iter().find(|m| m["method"] == "thread/start").unwrap();
            assert_eq!(open["params"]["model"], model);
            let turn = messages.iter().find(|m| m["method"] == "turn/start").unwrap();
            assert_eq!(turn["params"]["effort"], "low");
        } else {
            let args = title.args.lock().unwrap();
            for (flag, value) in [("--model", model), ("--effort", "low")] {
                let at = args[0].iter().position(|a| a == flag).unwrap();
                assert_eq!(args[0][at + 1], value);
            }
        }
        let ledger = engine.ledger();
        assert!(ledger.of(vendor).unmetered.is_empty(), "a priced title is known spend");
        let conversation = engine.open(&id).unwrap();
        let answer = conversation.messages[1].cost_usd.unwrap();
        let total: f64 = ledger.of(vendor).own_spend.values().sum();
        assert!(total > answer && conversation.spent_usd > answer, "the title is charged in both ledgers");
        assert!((total - conversation.spent_usd).abs() < 1e-9);
        assert!(main.mismatches.lock().unwrap().is_empty(), "{:?}", main.mismatches.lock().unwrap());
        assert!(title.mismatches.lock().unwrap().is_empty(), "{:?}", title.mismatches.lock().unwrap());
        engine.shutdown();
    }
}
