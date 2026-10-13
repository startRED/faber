//! Auto with the quota in it: spare quota buys a step up, a vendor running low is left, and nothing doubtful or unpaid is used.
//! Two vendors are connected; each is a replayed recording, and the router's verdict comes from a hook.
//!
//! What Auto chose is read from what was sent to the CLIs (the arguments of a Claude process, the lines written to Codex), not
//! from the answer's `model`: the recordings report their own model as soon as they start, and that overwrites it.

mod support;

use faber_engine::budget::Billing;
use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Message, PaceNote, Role, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{Fake, fake, fixture};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
const CODE: &str = "Fix the failing test in my billing service";
/// A moment before the recorded windows reset.
const WHEN: u64 = 1_791_620_000;
const HOUR: u64 = 3600;
const DAY: u64 = 86_400;
type Time = Arc<AtomicU64>;

struct Rig {
    engine: Engine,
    rx: UnboundedReceiver<AppEvent>,
    time: Time,
    claude: Fake,
    codex: Fake,
}

/// What Auto sent where: the model and effort of the Claude process it started, or the Codex model it asked for.
#[derive(Debug, PartialEq)]
enum Went {
    Claude(String, String),
    Codex(&'static str),
    Nowhere,
}

impl Rig {
    /// Waits for the answer to be routed and started, and says where it went.
    async fn went(&self, id: &str, n: usize) -> (Message, Went) {
        for _ in 0..150 {
            let reply = self.engine.open(id).and_then(|c| c.messages.get(n * 2 + 1).cloned());
            if let Some(reply) = reply.filter(|m| m.vendor.is_some()) {
                tokio::time::sleep(Duration::from_millis(60)).await;
                let went = match reply.vendor {
                    Some(Vendor::Claude) => {
                        let args = self.claude.args.lock().unwrap().get(n).cloned().unwrap_or_default();
                        Went::Claude(flag(&args, "--model").unwrap_or_default(), flag(&args, "--effort").unwrap_or_default())
                    }
                    _ => ["gpt-6.1-sol", "gpt-6-luna"]
                        .into_iter()
                        .find(|m| self.codex.wire.lines_with(&format!("\"model\":\"{m}\"")) > 0)
                        .map_or(Went::Nowhere, Went::Codex),
                };
                return (reply, went);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("message {n} was never routed");
    }
}

fn claude(model: &str, effort: &str) -> Went {
    Went::Claude(model.into(), effort.into())
}

/// The verdicts the router gives in turn, then keeps the last one.
fn verdicts(list: Vec<(&'static str, &'static str)>) -> Ask {
    let calls = Arc::new(Mutex::new(0usize));
    Arc::new(move |_| {
        let mut n = calls.lock().unwrap();
        let (kind, level) = list[(*n).min(list.len() - 1)];
        *n += 1;
        Box::pin(async move { Some(format!(r#"{{"kind":"{kind}","difficulty":"{level}","confidence":0.95}}"#)) })
    })
}

fn rig(dir: &Path, vendors: &[Vendor], ask: Ask) -> Rig {
    rig_env(dir, vendors, ask, false)
}

fn rig_env(dir: &Path, vendors: &[Vendor], ask: Ask, claude_env_billed: bool) -> Rig {
    let (claude, codex) = (fake(fixture("claude", "chat")), fake(fixture("codex", "chat")));
    let (c, x) = (claude.spawn.clone(), codex.spawn.clone());
    let spawn: Spawner = Arc::new(move |bin, args, cwd| if bin == "claude" { c(bin, args, cwd) } else { x(bin, args, cwd) });
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: vendors.iter().map(|v| (*v, Cli { command: v.name().into(), prefix: vec![] })).collect(),
        spawn,
        windows: false,
        claude_env_billed,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: Some(ask),
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    let time: Time = Arc::new(AtomicU64::new(WHEN));
    let clock = time.clone();
    engine.set_time_source(Arc::new(move || clock.load(Ordering::SeqCst) as f64));
    for vendor in vendors {
        engine.note_account_events(
            *vendor,
            vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), paid_fallback: Some(false), ..Signals::default() })],
        );
    }
    Rig { engine, rx, time, claude, codex }
}

#[tokio::test]
async fn auto_after_a_manual_turn_does_not_bypass_unknown_billing() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = rig(dir.path(), &[Vendor::Claude], verdicts(vec![("chat", "easy")]));
    r.engine
        .note_account_events(Vendor::Claude, vec![EngineEvent::Signals(Signals { billing: Some(Billing::Unknown), ..Signals::default() })]);
    let chosen = SendRequest { choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None }, ..auto(SKY) };
    let id = r.engine.send(chosen).unwrap();
    finished(&mut r.rx).await;
    let sent = r.claude.wire.lines_with("\"type\":\"user\"");
    let result = r.engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto(SKY) });
    assert_eq!(result.unwrap_err(), "error.billingUnknown");
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(r.claude.wire.lines_with("\"type\":\"user\""), sent);
    assert_eq!(r.engine.open(&id).unwrap().messages.last().unwrap().status, Some(Status::Error));
    r.engine.shutdown();
}

fn both(dir: &Path, ask: Ask) -> Rig {
    rig(dir, &[Vendor::Claude, Vendor::Codex], ask)
}

fn auto(text: &str) -> SendRequest {
    let choice = Choice { vendor: "auto".into(), model: None, effort: None };
    SendRequest { conversation_id: None, area: Area::Chat, project_id: None, text: text.into(), images: vec![], files: vec![], choice }
}

fn quota(window: &str, used: f64, resets_in: u64) -> EngineEvent {
    EngineEvent::Quota { window: window.into(), used, resets_at: (WHEN + resets_in) as f64 }
}

fn plan(plan: &str) -> EngineEvent {
    EngineEvent::Signals(Signals {
        plan: Some(plan.into()),
        billing: Some(Billing::Subscription),
        paid_fallback: Some(false),
        ..Signals::default()
    })
}

fn billed_by_use() -> EngineEvent {
    EngineEvent::Signals(Signals { billing: Some(Billing::UsageBilled), ..Signals::default() })
}

/// A Claude with a lot of quota to spare: the week half over and a fifth used, the five-hour window barely touched.
fn spare_claude(engine: &Engine) {
    engine.note_account_events(
        Vendor::Claude,
        vec![plan("max"), quota("five_hour", 0.05, 9000), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)],
    );
}

/// A Codex with its week more than half over and nearly all used.
fn low_codex(engine: &Engine) {
    engine.note_account_events(Vendor::Codex, vec![plan("pro"), quota("seven_day", 0.95, 3 * DAY)]);
}

async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> Message {
    loop {
        let e = tokio::time::timeout(Duration::from_secs(6), rx.recv()).await.expect("no finished answer in time").expect("engine gone");
        if let AppEvent::Message { message, .. } = e
            && message.role == Role::Assistant
            && message.status != Some(Status::Streaming)
        {
            return *message;
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).map(|i| args[i + 1].clone())
}

#[tokio::test]
async fn a_claude_with_quota_to_spare_buys_one_step_up_on_a_task_that_matters_and_the_answer_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
    spare_claude(&r.engine);
    let id = r.engine.send(auto(CODE)).unwrap();
    let (reply, went) = r.went(&id, 0).await;
    assert_eq!(went, claude("claude-sonnet-5-5", "high"), "one rung above medium");
    assert_eq!(reply.reason.as_deref(), Some("reason.route.code.medium"), "the stored reason is unchanged");
    assert_eq!(reply.pace, Some(PaceNote { kind: "step".into(), vendor: Vendor::Claude }));
    r.engine.shutdown();
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("conversations").join(format!("{id}.json"))).unwrap()).unwrap();
    assert_eq!(saved["messages"][1]["pace"], serde_json::json!({ "kind": "step", "vendor": "claude" }));
}

#[tokio::test]
async fn nothing_changes_for_an_easy_chat_or_when_there_is_no_reading() {
    let dir = tempfile::tempdir().unwrap();
    let r = both(dir.path(), verdicts(vec![("chat", "easy")]));
    spare_claude(&r.engine);
    let id = r.engine.send(auto(SKY)).unwrap();
    let (reply, went) = r.went(&id, 0).await;
    assert_eq!((went, reply.pace), (Went::Codex("gpt-6-luna"), None), "easy chat stays on the fast model");
    r.engine.shutdown();

    let dir = tempfile::tempdir().unwrap();
    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
    let id = r.engine.send(auto(CODE)).unwrap();
    let (reply, went) = r.went(&id, 0).await;
    assert_eq!((went, reply.pace), (Went::Codex("gpt-6.1-sol"), None), "no reading, no news: today's pick");
    assert_eq!(r.codex.wire.lines_with("\"effort\":\"low\""), 1);
    r.engine.shutdown();
}

#[tokio::test]
async fn a_reading_that_may_be_wrong_gives_no_discount() {
    // Each case sets up the same spare Claude and then spoils the reading in one way; the pick stays today's.
    type Spoil = fn(&Engine, &Time);
    let cases: [(&str, Spoil); 3] = [
        ("older than six hours", |e, t| {
            spare_claude(e);
            t.store(WHEN + 7 * HOUR, Ordering::SeqCst);
            // The plan is seen again, so that only the age of the reading is wrong.
            e.note_account_events(Vendor::Claude, vec![plan("max")]);
        }),
        ("use from outside just jumped", |e, t| {
            t.store(WHEN - 2 * HOUR, Ordering::SeqCst);
            e.note_account_events(Vendor::Claude, vec![quota("five_hour", 0.10, 4 * HOUR), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)]);
            t.store(WHEN, Ordering::SeqCst);
            e.note_account_events(
                Vendor::Claude,
                vec![plan("max"), quota("five_hour", 0.16, 4 * HOUR), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)],
            );
        }),
        ("the plan was last seen more than half an hour ago", |e, t| {
            spare_claude(e);
            t.store(WHEN + 31 * 60, Ordering::SeqCst);
            e.note_account_events(Vendor::Claude, vec![quota("five_hour", 0.05, 9000), quota("seven_day", 0.2, 3 * DAY + 12 * HOUR)]);
        }),
    ];
    for (name, spoil) in cases {
        let dir = tempfile::tempdir().unwrap();
        let r = both(dir.path(), verdicts(vec![("code", "medium")]));
        spoil(&r.engine, &r.time);
        let id = r.engine.send(auto(CODE)).unwrap();
        let (reply, went) = r.went(&id, 0).await;
        assert_eq!((went, reply.pace), (Went::Codex("gpt-6.1-sol"), None), "{name}");
        r.engine.shutdown();
    }
}

#[tokio::test]
async fn a_clock_set_backwards_does_not_make_an_old_reading_look_new() {
    let dir = tempfile::tempdir().unwrap();
    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
    spare_claude(&r.engine);
    // The PC's clock is now an hour before the moment the readings were taken.
    r.time.store(WHEN - HOUR, Ordering::SeqCst);
    r.engine.note_account_events(Vendor::Claude, vec![plan("max")]);
    let id = r.engine.send(auto(CODE)).unwrap();
    let (reply, went) = r.went(&id, 0).await;
    assert_eq!((went, reply.pace), (Went::Codex("gpt-6.1-sol"), None));
    r.engine.shutdown();
}

#[tokio::test]
async fn a_vendor_running_low_is_left_for_the_other_and_the_answer_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
    low_codex(&r.engine);
    let id = r.engine.send(auto(CODE)).unwrap();
    let (reply, went) = r.went(&id, 0).await;
    assert_eq!(went, claude("claude-sonnet-5-5", "medium"));
    assert_eq!(reply.pace, Some(PaceNote { kind: "low".into(), vendor: Vendor::Codex }));
    r.engine.shutdown();
}

#[tokio::test]
async fn a_vendor_that_runs_out_between_the_question_and_the_dispatch_is_not_used() {
    let dir = tempfile::tempdir().unwrap();
    let slow: Ask = Arc::new(|_| {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(250)).await;
            Some(r#"{"kind":"code","difficulty":"medium","confidence":0.95}"#.to_string())
        })
    });
    let r = both(dir.path(), slow);
    spare_claude(&r.engine);
    let id = r.engine.send(auto(CODE)).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    // While the router thinks, the Claude week runs out.
    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 1.0, 2 * DAY)]);
    let (_, went) = r.went(&id, 0).await;
    assert_eq!(went, Went::Codex("gpt-6.1-sol"));
    assert!(r.claude.args.lock().unwrap().is_empty(), "no Claude process was started for it");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_family_that_is_out_leaves_the_rest_of_the_vendor_alone() {
    let dir = tempfile::tempdir().unwrap();
    let r = both(dir.path(), verdicts(vec![("code", "hard")]));
    spare_claude(&r.engine);
    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day_opus", 1.0, 2 * DAY)]);
    let id = r.engine.send(auto("Redesign the locking in the scheduler")).unwrap();
    let (_, went) = r.went(&id, 0).await;
    assert_ne!(went, claude("claude-opus-5-5", "medium"), "only the Opus profiles are out");
    assert!(!matches!(&went, Went::Claude(model, _) if model.contains("opus")));
    r.engine.shutdown();
}

#[tokio::test]
async fn a_login_billed_by_use_is_never_picked_by_auto_but_the_user_can_pick_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = both(dir.path(), verdicts(vec![("code", "hard")]));
    spare_claude(&r.engine);
    r.engine.note_account_events(Vendor::Claude, vec![billed_by_use()]);
    let id = r.engine.send(auto("Redesign the locking in the scheduler")).unwrap();
    let (reply, went) = r.went(&id, 0).await;
    assert_eq!((reply.vendor, went), (Some(Vendor::Codex), Went::Codex("gpt-6.1-sol")));
    assert!(r.claude.args.lock().unwrap().is_empty());

    let by_hand = SendRequest { choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None }, ..auto(SKY) };
    r.engine.send(by_hand).unwrap();
    let reply = finished(&mut r.rx).await;
    assert_eq!((reply.vendor, reply.status), (Some(Vendor::Claude), Some(Status::Done)), "the user's own choice stands");
    r.engine.shutdown();
}

#[tokio::test]
async fn with_only_a_login_billed_by_use_auto_ends_with_an_error_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = rig(dir.path(), &[Vendor::Claude], verdicts(vec![("chat", "easy")]));
    r.engine.note_account_events(Vendor::Claude, vec![billed_by_use()]);
    r.engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut r.rx).await;
    assert_eq!((reply.status, reply.error.as_deref()), (Some(Status::Error), Some("error.billedPerUse")));
    assert!(r.claude.args.lock().unwrap().is_empty());
    r.engine.shutdown();
}

#[tokio::test]
async fn an_easy_start_still_moves_up_after_a_turn_that_carried_a_quota_reason() {
    let dir = tempfile::tempdir().unwrap();
    let mut r = both(dir.path(), verdicts(vec![("chat", "easy"), ("code", "hard")]));
    low_codex(&r.engine);
    let id = r.engine.send(auto(SKY)).unwrap();
    let first = finished(&mut r.rx).await;
    assert!(r.claude.args.lock().unwrap()[0].iter().any(|a| a == "claude-haiku-5-5"));
    assert_eq!(first.reason.as_deref(), Some("reason.route.chat.easy"));
    assert_eq!(first.pace, Some(PaceNote { kind: "low".into(), vendor: Vendor::Codex }));
    r.engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("Now refactor my billing service") }).unwrap();
    let (second, went) = r.went(&id, 1).await;
    assert_eq!(second.reason.as_deref(), Some("reason.route.code.hard"));
    assert!(matches!(&went, Went::Claude(model, _) if model != "claude-haiku-5-5"), "{went:?}");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_conversation_file_from_before_pace_existed_opens() {
    let dir = tempfile::tempdir().unwrap();
    let r = both(dir.path(), verdicts(vec![("code", "medium")]));
    spare_claude(&r.engine);
    let id = r.engine.send(auto(CODE)).unwrap();
    r.went(&id, 0).await;
    r.engine.shutdown();
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    let mut saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    saved["messages"][1].as_object_mut().unwrap().remove("pace");
    std::fs::write(&file, saved.to_string()).unwrap();
    let again = both(dir.path(), verdicts(vec![("code", "medium")]));
    assert_eq!(again.engine.open(&id).unwrap().messages[1].pace, None);
    again.engine.shutdown();
}

#[tokio::test]
async fn an_environment_override_cannot_be_overruled_by_a_subscription_signal() {
    let dir = tempfile::tempdir().unwrap();
    let r = rig_env(dir.path(), &[Vendor::Claude], verdicts(vec![("chat", "easy")]), true);
    let id = r.engine.send(auto(SKY)).unwrap();
    for _ in 0..100 {
        if r.engine.open(&id).unwrap().messages.last().unwrap().status == Some(Status::Error) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(r.engine.open(&id).unwrap().messages.last().unwrap().error.as_deref(), Some("error.billedPerUse"));
    assert!(r.claude.args.lock().unwrap().is_empty());
    r.engine.shutdown();
}

#[tokio::test]
async fn a_week_spent_faster_than_one_seventh_a_day_is_priced_up_on_either_vendor() {
    // One day of seven gone and a fifth of the week used (14.3% a day would be 14.3%): the vendor is left for the other.
    for (spent, other) in [(Vendor::Claude, Vendor::Codex), (Vendor::Codex, Vendor::Claude)] {
        let dir = tempfile::tempdir().unwrap();
        let r = both(dir.path(), verdicts(vec![("code", "medium")]));
        r.engine.note_account_events(spent, vec![plan("max"), quota("seven_day", 0.2, 6 * DAY)]);
        r.engine.note_account_events(other, vec![plan("pro"), quota("seven_day", 0.1, 6 * DAY)]);
        let id = r.engine.send(auto(CODE)).unwrap();
        let (reply, _) = r.went(&id, 0).await;
        assert_eq!(reply.vendor, Some(other), "{spent:?} is ahead of the daily line");
        r.engine.shutdown();
    }
}
