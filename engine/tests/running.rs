//! The running list and the quota readings: what the engine says is running, in which order, and when it says it.

use crate::support;

use faber_engine::cli::Cli;
use faber_engine::engine::Ask;
use faber_engine::model::{Area, Permissions, RunState, RunningItem, Settings, Status, Vendor};
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
        claude_env_billed: false,
        max_warm: 8,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask,
    };
    let engine = Engine::new(opts, Arc::new(move |e| drop(tx.send(e))));
    // These scenarios exercise permission queues and stale card answers, not the unattended default.
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    for vendor in engine.vendors() {
        engine.note_account_events(
            vendor,
            vec![faber_engine::events::EngineEvent::Signals(faber_engine::signals::Signals {
                billing: Some(faber_engine::budget::Billing::Subscription),
                ..Default::default()
            })],
        );
    }

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
        choice: Choice { vendor: vendor.into(), model: None, effort: None },
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
        taken(engine.answer(&id, &item.turn_id, &item.permissions[0].request_id, round == 0, false).await);
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
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
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
    // Faber itself will not remove a project that is answering; the folder leaves the list by another way (the file is
    // edited) while Auto chooses, so the answer has nowhere to run.
    assert_eq!(engine.remove_project(&project.id), Err("error.project.busy".to_string()));
    std::fs::write(dir.path().join("projects.json"), "[]").unwrap();
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

/// Answers a question as the screen does: by the ids of the conversation, the turn and the request.
async fn answer(engine: &Engine, item: &RunningItem, allow: bool) -> Result<(), String> {
    engine.answer(&item.conversation_id, &item.turn_id, &item.permissions[0].request_id, allow, false).await
}

/// The recorded CLI ends its turn the moment it has the answer, so the turn can be over before the engine settles. That
/// answer is stale, as the settlement check intends; anything else is a failure.
fn taken(result: Result<(), String>) {
    assert!(result.is_ok() || result == Err("error.permission.stale".to_string()), "{result:?}");
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
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
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
        taken(answer(&engine, &in_chat, round == 0).await);
        let after = engine.state().running.items;
        assert!(!asks(&after, &chat, &wanted), "the answered question is gone");
        assert!(asks(&after, &work, &wanted), "the other conversation's question, with the same id, still waits");
        taken(answer(&engine, &in_project, round == 0).await);
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
    taken(answer(&engine, &one, true).await);
    lists_until(&mut rx, WAIT, |rows| rows.is_empty()).await;

    engine.send(SendRequest { conversation_id: Some(id.clone()), ..request(Area::Chat, None, "second", "claude") }).unwrap();
    let two = lists_until(&mut rx, WAIT, first_is(RunState::NeedsYou)).await.pop().unwrap().1.remove(0);
    assert_eq!((two.permissions[0].request_id.as_str(), two.turn_id != one.turn_id), ("req-1", true));
    let written = f.wire.lines_with("control_response");

    assert_eq!(answer(&engine, &one, true).await, Err("error.permission.stale".to_string()));
    assert_eq!(f.wire.lines_with("control_response"), written, "nothing was written");
    assert_eq!(engine.state().running.items[0].permissions.len(), 1, "turn 2 still waits");

    taken(answer(&engine, &two, true).await);
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

    taken(answer(&engine, &rows.1[0], true).await);
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
async fn the_engine_is_working_from_the_first_row_until_the_last_one_leaves() {
    let dir = tempfile::tempdir().unwrap();
    let f = fake(fixture("claude", "interrupt"));
    let (engine, mut rx) = engine(dir.path(), &[Vendor::Claude], f.spawn.clone(), None);
    assert!(!engine.is_working(), "nothing has been sent");
    let id = engine.send(request(Area::Chat, None, "Write a 600-word essay about the history of bread.", "claude")).unwrap();
    lists_until(&mut rx, WAIT, |rows| !rows.is_empty()).await;
    assert!(engine.is_working());
    engine.interrupt(&id);
    lists_until(&mut rx, WAIT, |rows| rows.is_empty()).await;
    assert!(!engine.is_working());
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

#[tokio::test]
async fn a_driver_panic_removes_only_its_run_from_overlapping_chat_and_project_work() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use tokio::io::{AsyncRead, ReadBuf};
    struct Panics(tokio::sync::oneshot::Receiver<()>);
    impl AsyncRead for Panics {
        fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, _buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
            match Pin::new(&mut self.0).poll(cx) {
                Poll::Ready(Ok(())) => panic!("simulated driver panic"),
                Poll::Ready(Err(_)) => return Poll::Ready(Ok(())),
                Poll::Pending => {}
            }
            Poll::Pending
        }
    }
    for vendor in Vendor::ALL {
        let triggers = Arc::new(std::sync::Mutex::new(Vec::new()));
        let keep = triggers.clone();
        let spawn: Spawner = Arc::new(move |_, _, _| {
            let (tx, rx) = tokio::sync::oneshot::channel();
            keep.lock().unwrap().push(tx);
            Ok(faber_engine::proc::Proc {
                stdin: Box::new(tokio::io::sink()),
                stdout: Box::new(Panics(rx)),
                stderr: None,
                child: None,
                contained: None,
            })
        });
        let dir = tempfile::tempdir().unwrap();
        let (engine, mut rx) = engine(dir.path(), &[vendor], spawn, None);
        let project = engine.add_project(&dir.path().to_string_lossy());
        let chat = engine.send(request(Area::Chat, None, SKY, vendor.name())).unwrap();
        let work = engine.send(request(Area::Project, Some(&project.id), SKY, vendor.name())).unwrap();
        lists_until(&mut rx, WAIT, |rows| rows.len() == 2).await;
        // Give both driver futures a poll; Codex must install each thread's event sink first.
        tokio::task::yield_now().await;
        triggers.lock().unwrap().remove(0).send(()).unwrap();
        lists_until(&mut rx, WAIT, |rows| rows.len() == 1 && rows[0].conversation_id == work).await;
        assert_eq!(engine.open(&chat).unwrap().messages.last().unwrap().status, Some(Status::Error));
        engine.shutdown();
        assert!(engine.state().running.items.is_empty());
    }
}

/// A project with one mission record in the given state, under the folder and the data directory the engine uses.
fn mission_in(engine: &Engine, data: &Path, repo: &Path, state: faber_engine::project::MissionState) -> String {
    use faber_engine::project::{Mission, run::Runner, store::Journal};
    let project = engine.add_project(&repo.to_string_lossy());
    let journal = Journal::open(repo, data, "m1").unwrap();
    let mut runner = Runner::create(&journal, Mission::new("m1", &project.id, "repo", "Fix", &"1".repeat(40), 3, 1)).unwrap();
    runner.mission.state = state;
    runner.save(&journal).unwrap();
    project.id
}

#[tokio::test]
async fn a_project_with_a_planning_or_running_mission_cannot_be_removed_until_it_stops() {
    use faber_engine::project::MissionState::{Done, Paused, Planning, Running};
    for (state, removable) in [(Planning, false), (Running, false), (Paused, true), (Done, true)] {
        let (data, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (engine, _rx) = engine(data.path(), &[Vendor::Codex], fake(fixture("codex", "chat")).spawn, None);
        let id = mission_in(&engine, data.path(), repo.path(), state);
        let result = engine.remove_project(&id);
        if removable {
            assert_eq!(result, Ok(()), "{state:?}");
            assert!(engine.state().projects.is_empty(), "{state:?}");
        } else {
            assert_eq!(result, Err("error.project.busy".to_string()), "{state:?}");
            assert_eq!(engine.state().projects.len(), 1, "{state:?} stays in the list");
        }
        engine.shutdown();
    }
}

#[tokio::test]
async fn a_renamed_conversation_never_keeps_control_or_invisible_characters() {
    let dir = tempfile::tempdir().unwrap();
    let ask = router(Some(Duration::from_millis(200)));
    let (engine, _rx) = engine(dir.path(), &[Vendor::Codex], fake(fixture("codex", "chat")).spawn, Some(ask));
    let id = engine.send(request(Area::Chat, None, SKY, "auto")).unwrap();
    let before = engine.open(&id).unwrap().title;
    engine.rename(&id, "  A\u{7}B\u{202e}C\u{200b}D\u{2028}E\n ");
    assert_eq!(engine.open(&id).unwrap().title, "ABCDE");
    engine.rename(&id, "\u{7}\u{200b} \u{202e}");
    assert_eq!(engine.open(&id).unwrap().title, "ABCDE", "a name with nothing visible changes nothing");
    assert_ne!(before, "ABCDE");
    // The joiners belong to the spelling of Persian, Indic and emoji text and stay.
    let persian = "\u{646}\u{627}\u{645}\u{647}\u{200c}\u{627}\u{6cc}";
    engine.rename(&id, persian);
    assert_eq!(engine.open(&id).unwrap().title, persian);
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    engine.rename(&id, family);
    assert_eq!(engine.open(&id).unwrap().title, family);
    engine.shutdown();
}

#[tokio::test]
async fn a_mission_record_that_cannot_be_read_keeps_its_project_from_being_removed() {
    use faber_engine::project::MissionState::Paused;
    let (data, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (engine, _rx) = engine(data.path(), &[Vendor::Codex], fake(fixture("codex", "chat")).spawn, None);
    let id = mission_in(&engine, data.path(), repo.path(), Paused);
    // The checkpoint is damaged: nothing says whether the mission works, so the project stays.
    std::fs::write(data.path().join("missions").join("m1").join("run.json"), "{ not json").unwrap();
    assert_eq!(engine.remove_project(&id), Err("error.project.busy".to_string()));
    assert_eq!(engine.state().projects.len(), 1);
    engine.shutdown();
}

#[tokio::test]
async fn a_missions_folder_that_cannot_be_listed_keeps_the_project_but_a_missing_one_does_not() {
    let (data, repo) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (engine, _rx) = engine(data.path(), &[Vendor::Codex], fake(fixture("codex", "chat")).spawn, None);
    let project = engine.add_project(&repo.path().to_string_lossy());
    // `.faber/missions` is a file, so it cannot be listed: that is not "no missions".
    std::fs::create_dir_all(repo.path().join(".faber")).unwrap();
    std::fs::write(repo.path().join(".faber").join("missions"), "x").unwrap();
    assert_eq!(engine.remove_project(&project.id), Err("error.project.busy".to_string()));
    std::fs::remove_file(repo.path().join(".faber").join("missions")).unwrap();
    assert_eq!(engine.remove_project(&project.id), Ok(()));
    engine.shutdown();
}
