//! What Auto learns from the answers the person retries or keeps: the signals, the one-rung bound, the way back down, the
//! limits it never passes, and the file it lives in (`auto-learned.json`). The CLIs are replayed recordings; nothing real runs.

use crate::support;

use faber_engine::auto_learned::{Learned, Signal};
use faber_engine::budget::Billing;
use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Message, Role, Status, Vendor};
use faber_engine::route::{Factor, PROFILES, Profile, Routed, candidates, choose, choose_paced, lifted};
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::store::Store;
use faber_engine::{AppEvent, BranchRequest, Engine, EngineOptions, SendRequest};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{fake, fixture};
use tokio::sync::mpsc::UnboundedReceiver;

const SKY: &str = "In one sentence: why is the sky blue?";
type Args = Arc<Mutex<Vec<Vec<String>>>>;

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

fn engine(dir: &crate::support::DataFolder, spawn: faber_engine::proc::Spawner, ask: Ask) -> (Engine, UnboundedReceiver<AppEvent>) {
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
        ask: Some(ask),
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap();
    engine.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), ..Default::default() })],
    );
    engine.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Quota { window: "five_hour".into(), used: 0.0, resets_at: now() as f64 + 18_000.0 }],
    );
    (engine, rx)
}

/// A router that gives these verdicts in turn, then keeps the last one.
fn verdicts(list: Vec<(&'static str, &'static str)>) -> Ask {
    let calls = Arc::new(Mutex::new(0usize));
    Arc::new(move |_| {
        let mut n = calls.lock().unwrap();
        let (kind, level) = list[(*n).min(list.len() - 1)];
        *n += 1;
        Box::pin(async move { Some(format!(r#"{{"kind":"{kind}","difficulty":"{level}","confidence":0.95}}"#)) })
    })
}

fn auto_choice() -> Choice {
    Choice { vendor: "auto".into(), model: None, effort: None }
}

fn auto(text: &str) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area: Area::Chat,
        project_id: None,
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: auto_choice(),
    }
}

async fn finished(rx: &mut UnboundedReceiver<AppEvent>) -> Message {
    loop {
        let e = tokio::time::timeout(crate::support::DEADLINE, rx.recv()).await.expect("no finished answer in time").expect("engine gone");
        if let AppEvent::Message { message, .. } = e
            && message.role == Role::Assistant
            && message.status != Some(Status::Streaming)
        {
            return *message;
        }
    }
}

/// The answer to a conversation's `n`th message once its model is known (the recording may never answer it).
async fn routed(engine: &Engine, id: &str, n: usize) -> Message {
    let mut found = None;
    support::until(&format!("message {n} was never routed"), || {
        found = engine.open(id).and_then(|c| c.messages.get(n * 2 + 1).cloned()).filter(|m| m.model.is_some());
        found.is_some()
    })
    .await;
    found.unwrap()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).map(|i| args[i + 1].clone())
}

/// (model, effort) the n-th CLI start asked for.
fn started(args: &Args, n: usize) -> (String, String) {
    let all = args.lock().unwrap();
    (flag(&all[n], "--model").unwrap(), flag(&all[n], "--effort").unwrap())
}

fn rank(pick: &(String, String)) -> (u8, u8) {
    PROFILES.iter().find(|p| p.model == pick.0 && p.effort == pick.1).map(|p| (p.iq, p.agent)).expect("a model of the table")
}

fn file(dir: &Path) -> std::path::PathBuf {
    dir.join("auto-learned.json")
}

/// What a person who retried code answers three times left behind: a lifted kind, written the way the engine writes it.
fn lift(dir: &Path, kind: &str) {
    std::fs::write(file(dir), format!(r#"{{"kinds":{{"{kind}":{{"marks":[],"lifted":true,"at":{}}}}}}}"#, now())).unwrap();
}

/// Starts an engine, lets `setup` act, sends one Auto message with this verdict and returns what answered it.
async fn first_message(
    dir: &crate::support::DataFolder,
    verdict: (&'static str, &'static str),
    setup: impl FnOnce(&Engine),
) -> ((String, String), Message) {
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(dir, f.spawn.clone(), verdicts(vec![verdict]));
    setup(&engine);
    engine.send(auto(SKY)).unwrap();
    let reply = finished(&mut rx).await;
    engine.shutdown();
    (started(&f.args, 0), reply)
}

fn learned_pace(m: &Message) -> bool {
    m.pace.as_ref().is_some_and(|p| p.kind == "learned")
}

fn stored(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(file(dir)).unwrap()).unwrap()
}

#[tokio::test]
async fn the_third_retry_of_a_kind_starts_the_next_conversation_one_rung_up_and_the_answer_says_why() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(&dir, f.spawn.clone(), verdicts(vec![("code", "medium")]));
    let id = engine.send(auto(SKY)).unwrap();
    let first = finished(&mut rx).await;
    assert_eq!(first.reason.as_deref(), Some("reason.route.code.medium"));
    assert!(!learned_pace(&first));
    let question = engine.open(&id).unwrap().messages[0].id.clone();
    let mut answers = vec![];
    for _ in 0..3 {
        let request = BranchRequest { conversation_id: id.clone(), message_id: question.clone(), text: SKY.into(), choice: auto_choice() };
        engine.branch(request).unwrap();
        answers.push(finished(&mut rx).await);
    }
    engine.shutdown();
    for (n, m) in answers.iter().enumerate().take(2) {
        assert!(!learned_pace(m), "retry {} is not a habit yet", n + 1);
        assert_eq!(started(&f.args, n + 1), started(&f.args, 0));
    }
    // The retry that makes it a habit already gets the stronger pick, and says why.
    assert!(learned_pace(&answers[2]));
    assert!(rank(&started(&f.args, 3)) > rank(&started(&f.args, 0)), "{:?} then {:?}", started(&f.args, 0), started(&f.args, 3));
    assert_eq!(stored(dir.path())["kinds"]["code"]["lifted"], true);
}

#[tokio::test]
async fn an_edited_request_teaches_nothing() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(&dir, f.spawn.clone(), verdicts(vec![("code", "medium")]));
    let id = engine.send(auto(SKY)).unwrap();
    finished(&mut rx).await;
    let question = engine.open(&id).unwrap().messages[0].id.clone();
    // Words changed: a new request, so nothing about the pick.
    for n in 0..4 {
        let request = BranchRequest {
            conversation_id: id.clone(),
            message_id: question.clone(),
            text: format!("{SKY} (variant {n})"),
            choice: auto_choice(),
        };
        engine.branch(request).unwrap();
    }
    engine.shutdown();
    assert!(!file(dir.path()).exists() || stored(dir.path())["kinds"].as_object().is_none_or(|k| k.is_empty()), "{:?}", stored(dir.path()));
}

#[tokio::test]
async fn a_message_sent_after_an_answer_keeps_it_and_eight_in_a_row_bring_a_lifted_kind_back_down() {
    let dir = crate::support::DataFolder::new();
    let marks = ["kept"; 7].join("\",\"");
    std::fs::write(file(dir.path()), format!(r#"{{"kinds":{{"code":{{"marks":["{marks}"],"lifted":true,"at":{}}}}}}}"#, now())).unwrap();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(&dir, f.spawn.clone(), verdicts(vec![("code", "medium")]));
    // Lifted: this first message starts one rung up, and is the answer that is kept below.
    let id = engine.send(auto(SKY)).unwrap();
    let first = finished(&mut rx).await;
    assert!(learned_pace(&first));
    assert_eq!(stored(dir.path())["kinds"]["code"]["lifted"], true, "an answer waiting for its next message is not yet kept");
    engine.send(SendRequest { conversation_id: Some(id), ..auto("And at sunset?") }).unwrap();
    engine.shutdown();
    // Seven kept before, this one the eighth: the nudge is gone.
    assert_eq!(stored(dir.path())["kinds"]["code"]["lifted"], false);
}

#[tokio::test]
async fn a_lifted_kind_starts_one_rung_up_and_forgetting_it_goes_back_to_the_table() {
    let (a, b, c) = (crate::support::DataFolder::new(), crate::support::DataFolder::new(), crate::support::DataFolder::new());
    let (usual, usual_reply) = first_message(&a, ("code", "medium"), |_| {}).await;
    lift(b.path(), "code");
    let (raised, raised_reply) = first_message(&b, ("code", "medium"), |_| {}).await;
    assert!(rank(&raised) > rank(&usual));
    assert!(learned_pace(&raised_reply) && !learned_pace(&usual_reply));
    // Another kind of task is untouched.
    let (writing, _) = first_message(&b, ("writing", "medium"), |_| {}).await;
    let (writing_usual, _) = first_message(&a, ("writing", "medium"), |_| {}).await;
    assert_eq!(writing, writing_usual);
    // Forgotten: the file is empty and the next conversation starts as the table says.
    lift(c.path(), "code");
    let (forgotten, reply) = first_message(&c, ("code", "medium"), |e| e.forget_auto_learned().unwrap()).await;
    assert_eq!(forgotten, usual);
    assert!(!learned_pace(&reply));
    assert!(stored(c.path())["kinds"].as_object().unwrap().is_empty());
    assert!(Store::new(c.path()).auto_learned().is_empty());
}

#[tokio::test]
async fn a_vendor_near_its_limit_gets_no_learned_step_up() {
    let (a, b) = (crate::support::DataFolder::new(), crate::support::DataFolder::new());
    let near = |e: &Engine| {
        // No paid fallback: with it possible, a window past 90% is refused outright (the gate), which is not what is tested.
        e.note_account_events(Vendor::Claude, vec![EngineEvent::Signals(Signals { paid_fallback: Some(false), ..Default::default() })]);
        e.note_account_events(
            Vendor::Claude,
            vec![EngineEvent::Quota { window: "five_hour".into(), used: 0.95, resets_at: now() as f64 + 18_000.0 }],
        )
    };
    let (usual, _) = first_message(&a, ("code", "medium"), near).await;
    lift(b.path(), "code");
    let (same, reply) = first_message(&b, ("code", "medium"), near).await;
    assert_eq!(same, usual, "a window past 90% buys no step up, learned or paced");
    assert!(!learned_pace(&reply));
}

#[tokio::test]
async fn what_was_learned_never_changes_a_conversation_under_way() {
    let (a, b) = (crate::support::DataFolder::new(), crate::support::DataFolder::new());
    // A greeting first, then code: the one place Auto moves a conversation up (rule 3, once).
    let second = |dir: crate::support::DataFolder| async move {
        let f = fake(fixture("claude", "chat"));
        let (engine, mut rx) = engine(&dir, f.spawn.clone(), verdicts(vec![("chat", "easy"), ("code", "medium")]));
        let id = engine.send(auto(SKY)).unwrap();
        finished(&mut rx).await;
        engine.send(SendRequest { conversation_id: Some(id.clone()), ..auto("Now refactor my billing service") }).unwrap();
        let reply = routed(&engine, &id, 1).await;
        engine.shutdown();
        (started(&f.args, 1), reply)
    };
    let (usual, usual_reply) = second(a).await;
    lift(b.path(), "code");
    let (with_learning, reply) = second(b).await;
    assert_eq!(with_learning, usual);
    assert!(!learned_pace(&reply) && !learned_pace(&usual_reply));
}

#[tokio::test]
async fn the_file_round_trips_and_a_damaged_one_means_nothing_learned() {
    let dir = crate::support::DataFolder::new();
    let store = Store::new(dir.path());
    assert!(store.auto_learned().is_empty(), "no file, nothing learned");
    let mut learned = Learned::default();
    for _ in 0..3 {
        learned.note("code", Signal::Redo, now());
    }
    learned.note("writing", Signal::Kept, now());
    store.save_auto_learned(&learned).unwrap();
    assert_eq!(Store::new(dir.path()).auto_learned(), learned);
    assert!(Store::new(dir.path()).auto_learned().lifted("code", now()));
    // Damaged, cut short, of another shape, or with kinds nobody knows: learning starts again, and nothing breaks.
    for bad in
        ["not json", r#"{"kinds":{"code":{"marks":["redo""#, r#"{"kinds":[1,2]}"#, r#"{"kinds":{"code":{"marks":["sometimes"]}}}"#, ""]
    {
        std::fs::write(file(dir.path()), bad).unwrap();
        assert!(Store::new(dir.path()).auto_learned().is_empty(), "{bad:?}");
    }
    std::fs::write(file(dir.path()), r#"{"kinds":{"poetry":{"marks":["redo"],"lifted":true,"at":1}}}"#).unwrap();
    assert!(Store::new(dir.path()).auto_learned().is_empty(), "a kind Auto does not have is dropped");
}

#[tokio::test]
async fn a_damaged_file_at_start_changes_no_pick_and_learning_writes_a_good_one_over_it() {
    let (a, b) = (crate::support::DataFolder::new(), crate::support::DataFolder::new());
    let (usual, _) = first_message(&a, ("code", "medium"), |_| {}).await;
    std::fs::write(file(b.path()), "{ this is not json").unwrap();
    let f = fake(fixture("claude", "chat"));
    let (engine, mut rx) = engine(&b, f.spawn.clone(), verdicts(vec![("code", "medium")]));
    let id = engine.send(auto(SKY)).unwrap();
    finished(&mut rx).await;
    assert_eq!(started(&f.args, 0), usual);
    let question = engine.open(&id).unwrap().messages[0].id.clone();
    for _ in 0..3 {
        engine
            .branch(BranchRequest { conversation_id: id.clone(), message_id: question.clone(), text: SKY.into(), choice: auto_choice() })
            .unwrap();
        finished(&mut rx).await;
    }
    engine.shutdown();
    assert!(Store::new(b.path()).auto_learned().lifted("code", now()));
}

fn both() -> Vec<Profile> {
    candidates(&[Vendor::Claude, Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()])
}

fn usual(kind: &'static str, level: &'static str, from: &[Profile]) -> faber_engine::route::Routed {
    choose(kind, level, "glance", 0.9, from).unwrap()
}

fn neutral(_: &Profile) -> Factor {
    Factor::NEUTRAL
}

#[test]
fn the_lift_is_one_rung_of_the_ladder_and_says_it_is_learned() {
    let all = both();
    let base = usual("chat", "easy", &all);
    assert_eq!(base.profile.model, "gpt-6-luna");
    let up = lifted(base.clone(), &all, &neutral);
    // Easy's bar is nothing; medium's is a general intelligence of 41: the cheapest model that clears it.
    assert_eq!((up.profile.model, up.profile.effort), ("gpt-6.1-sol", "low"));
    assert_eq!(up.pace.as_ref().map(|p| (p.kind.as_str(), p.vendor)), Some(("learned", Vendor::Codex)));
    let hard = lifted(usual("code", "medium", &all), &all, &neutral);
    assert!((hard.profile.iq, hard.profile.agent) >= (47, 44), "medium code goes to hard's bar, no further");
    assert!(hard.profile.secs <= 60.0);
}

#[test]
fn the_lift_only_uses_models_the_account_lists_and_leaves_the_table_when_none_is_stronger() {
    // ChatGPT lists only the small model: nothing stronger exists, so the pick stands.
    let luna = candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]);
    let base = usual("chat", "easy", &luna);
    assert_eq!(lifted(base.clone(), &luna, &neutral), base);
    // A vendor that is not connected is not in the pool.
    let claude = candidates(&[Vendor::Claude], &[]);
    let up = lifted(usual("chat", "easy", &claude), &claude, &neutral);
    assert_eq!(up.profile.vendor, Vendor::Claude);
    // The strongest model of the table has nowhere to go.
    let strongest = *PROFILES.iter().max_by_key(|p| (p.iq, p.agent)).unwrap();
    let top = Routed { profile: strongest, ..usual("code", "hard", &both()) };
    assert_eq!(lifted(top.clone(), &both(), &neutral), top);
}

#[test]
fn the_lift_respects_the_quota_rules_of_the_vendor_it_would_use() {
    let all = both();
    let base = usual("code", "medium", &all);
    // No step up from a vendor with a window past 90%: every candidate is refused, so the table's pick stands.
    let no_step = |_: &Profile| Factor { m: 1.0, w: 1.0, step_ok: false };
    assert_eq!(lifted(base.clone(), &all, &no_step), base);
    // None from a vendor that is being used ahead of its daily line.
    let fast = |_: &Profile| Factor { m: 4.0, w: 1.0, step_ok: true };
    assert_eq!(lifted(base.clone(), &all, &fast), base);
    // Only the vendor with room is used.
    let codex_only_has_room = |p: &Profile| Factor { m: 1.0, w: 1.0, step_ok: p.vendor == Vendor::Codex };
    let up = lifted(base.clone(), &all, &codex_only_has_room);
    assert_eq!(up.profile.vendor, Vendor::Codex);
    // A step over the time cap is not taken: hard research caps at one minute, and ChatGPT's strongest takes longer.
    let sol = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]);
    let hard = usual("research", "hard", &sol);
    assert_eq!(lifted(hard.clone(), &sol, &neutral), hard);
}

/// Quota to spare at Claude makes it cheap: the table buys Claude's step up on its own, with no learning in it.
fn claude_has_spare_quota(p: &Profile) -> Factor {
    Factor { m: if p.vendor == Vendor::Claude { 0.1 } else { 1.0 }, w: 1.0, step_ok: true }
}

#[test]
fn a_step_up_the_quota_already_bought_is_not_lifted_again() {
    let all = both();
    let paced = choose_paced("code", "medium", "glance", 0.9, &all, &claude_has_spare_quota).unwrap();
    assert_eq!(paced.pace.as_ref().map(|p| p.kind.as_str()), Some("step"), "the discount buys a step up on its own");
    assert_eq!(lifted(paced.clone(), &all, &claude_has_spare_quota), paced, "one rung over the table, never two");
}

#[test]
fn a_pick_that_already_carries_a_quota_note_is_not_lifted_so_a_shown_change_always_says_learned() {
    let all = both();
    let note = faber_engine::model::PaceNote { kind: "fast".into(), vendor: Vendor::Codex };
    let base = Routed { pace: Some(note), ..usual("chat", "easy", &all) };
    assert_eq!(lifted(base.clone(), &all, &neutral), base, "the quota spoke first; learning acts only where it said nothing");
}

#[tokio::test]
async fn a_forget_that_cannot_reach_the_disk_says_so() {
    let dir = crate::support::DataFolder::new();
    lift(dir.path(), "code");
    let f = fake(fixture("claude", "chat"));
    let (engine, _rx) = engine(&dir, f.spawn.clone(), verdicts(vec![("code", "medium")]));
    // A folder where the temporary file goes: the write fails like a locked or read-only disk.
    std::fs::create_dir(dir.path().join("auto-learned.json.tmp")).unwrap();
    assert_eq!(engine.forget_auto_learned(), Err("error.autoLearned.forget".to_string()));
    assert_eq!(stored(dir.path())["kinds"]["code"]["lifted"], true, "the old file stays as it was");
    std::fs::remove_dir(dir.path().join("auto-learned.json.tmp")).unwrap();
    assert_eq!(engine.forget_auto_learned(), Ok(()));
    assert!(stored(dir.path())["kinds"].as_object().unwrap().is_empty());
    engine.shutdown();
}
