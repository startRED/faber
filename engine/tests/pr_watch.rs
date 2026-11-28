//! R200: a project conversation follows the pull request its agent opened. Red checks continue the conversation once,
//! with a short message that names them; merged ends the following with no turn spent; a tight quota tells the person
//! instead of the agent; and the person can stop following. gh is a fake here: no process runs.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, FollowState, Message, RunState, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::project::remote::{Ci, CiState, Failure, Look, PrState};
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{next_pipe, pipes};

const URL: &str = "https://github.com/acme/app/pull/93";

type Reads = Arc<Mutex<(VecDeque<Look>, Vec<(String, u64)>)>>;

fn engine(dir: &crate::support::DataFolder, spawn: Spawner) -> (Engine, Reads) {
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
        ask: None,
    };
    let engine = Engine::new(opts, Arc::new(|_| {})).unwrap();
    let reads: Reads = Arc::default();
    let fake = reads.clone();
    engine.set_pr_reader(Arc::new(move |repo, number| {
        let mut reads = fake.lock().unwrap();
        reads.1.push((repo.to_string(), number));
        reads.0.pop_front().ok_or("reason.remote.unreachable")
    }));
    (engine, reads)
}

fn look(state: PrState, ci: CiState, head: &str) -> Look {
    let failures = if ci == CiState::Failing {
        vec![Failure { name: "gate".into(), link: Some("https://github.com/acme/app/actions/runs/7".into()), said: None }]
    } else {
        vec![]
    };
    Look { state, head: head.into(), ci: Ci { state: ci, head: head.into(), failed: vec![], total: 3 }, conflict: false, failures }
}

fn step(id: &str, command: &str) -> Value {
    json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": id, "name": "Bash", "input": { "command": command } }] } })
}

fn result(id: &str, text: &str) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": id, "content": text, "is_error": false }] } })
}

fn done() -> Value {
    json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })
}

async fn until(engine: &Engine, id: &str, what: &str, ok: impl Fn(&[Message]) -> bool) -> Vec<Message> {
    let mut messages = vec![];
    support::until(what, || {
        messages = engine.open(id).unwrap().messages;
        ok(&messages)
    })
    .await;
    messages
}

fn follow_of(messages: &[Message]) -> Option<&faber_engine::model::Follow> {
    messages.iter().rev().find_map(|m| m.follow.as_ref())
}

/// A turn in which the agent opens pull request #93, run to its end.
async fn opened(engine: &Engine, made: &Arc<Mutex<Vec<support::Pipe>>>, project: &str) -> (String, support::Pipe) {
    let id = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.into()),
            text: "fix the bug and open a pull request".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let mut pipe = next_pipe(made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(step("b1", "git push -u origin fix && gh pr create --fill")).await;
    pipe.say(result("b1", &format!("Creating pull request for fix into main in acme/app\n\n{URL}\n"))).await;
    pipe.say(done()).await;
    until(engine, &id, "the turn ends", |m| m.last().is_some_and(|m| m.status == Some(Status::Done))).await;
    // Faber spends a turn of its own only on a known subscription (the replayed start says nothing of the plan).
    let plan = Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![EngineEvent::Signals(plan)]);
    (id, pipe)
}

fn sent_to_cli(pipe: &support::Pipe) -> Vec<String> {
    pipe.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").map(|m| m.to_string()).collect()
}

#[tokio::test]
async fn r_next_red_checks_wake_the_agent_once_and_merged_ends_with_no_turn() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = engine(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (id, mut pipe) = opened(&engine, &made, &project.id).await;
    let messages = engine.open(&id).unwrap().messages;
    let f = messages.last().unwrap().follow.clone().expect("the reply that opened the pull request follows it");
    assert_eq!((f.repo.as_str(), f.number, f.state), ("acme/app", 93, FollowState::Pending));

    // Pending: nothing happens.
    reads.lock().unwrap().0.push_back(look(PrState::Open, CiState::Pending, "aaa"));
    engine.look_at_prs().await;
    assert_eq!(engine.open(&id).unwrap().messages.len(), 2);

    // Red: one short message, sent by Faber, naming the failed check; the line moves under the new reply.
    reads.lock().unwrap().0.push_back(look(PrState::Open, CiState::Failing, "aaa"));
    engine.look_at_prs().await;
    let messages = until(&engine, &id, "a turn of Faber's own", |m| m.len() == 4).await;
    let wake = messages[2].wake.clone().expect("the message is marked as Faber's");
    assert_eq!((wake.why.as_str(), wake.pr, wake.checks.as_slice()), ("wake.checksFailed", Some(93), &["gate".to_string()][..]));
    assert!(
        messages[2].text.contains("Checks failed on pull request #93")
            && messages[2].text.contains("actions/runs/7")
            && messages[2].text.contains("Reply in the language of the conversation.")
    );
    assert!(messages[1].follow.is_none() && messages[3].follow.as_ref().is_some_and(|f| f.state == FollowState::Failing));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(sent_to_cli(&pipe).iter().any(|l| l.contains("Checks failed on pull request #93")), "the agent got it");
    pipe.say(done()).await;
    until(&engine, &id, "the fix turn ends", |m| m.last().is_some_and(|m| m.status == Some(Status::Done))).await;

    // The same failure again spends nothing.
    reads.lock().unwrap().0.push_back(look(PrState::Open, CiState::Failing, "aaa"));
    engine.look_at_prs().await;
    assert_eq!(engine.open(&id).unwrap().messages.len(), 4);

    // Merged: a quiet line, no turn, and gh is not asked again.
    reads.lock().unwrap().0.push_back(look(PrState::Merged, CiState::Passing, "bbb"));
    engine.look_at_prs().await;
    let messages = engine.open(&id).unwrap().messages;
    assert_eq!((messages.len(), follow_of(&messages).map(|f| f.state)), (4, Some(FollowState::Merged)));
    let asked = reads.lock().unwrap().1.len();
    engine.look_at_prs().await;
    assert_eq!(reads.lock().unwrap().1.len(), asked, "a merged pull request is not read again");
    assert!(reads.lock().unwrap().1.iter().all(|(repo, n)| repo == "acme/app" && *n == 93));
    engine.shutdown();
}

#[tokio::test]
async fn r_next_a_tight_quota_tells_the_person_and_they_can_stop_following() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = engine(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (id, _pipe) = opened(&engine, &made, &project.id).await;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    engine
        .note_account_events(Vendor::Claude, vec![EngineEvent::Quota { window: "five_hour".into(), used: 0.95, resets_at: now + 3600.0 }]);

    reads.lock().unwrap().0.push_back(look(PrState::Open, CiState::Failing, "aaa"));
    engine.look_at_prs().await;
    let messages = engine.open(&id).unwrap().messages;
    assert_eq!(messages.len(), 2, "no turn is spent on a tight quota");
    assert!(follow_of(&messages).is_some_and(|f| f.held && f.state == FollowState::Failing));
    let item = engine.state().running.items.into_iter().find(|i| i.conversation_id == id).expect("it needs the person");
    assert_eq!((item.state, item.held), (RunState::NeedsYou, Some((93, "aaa".to_string()))));

    engine.unfollow_pr(&id);
    let messages = engine.open(&id).unwrap().messages;
    assert!(follow_of(&messages).is_some_and(|f| f.state == FollowState::Stopped && !f.held));
    assert!(engine.state().running.items.is_empty());
    let asked = reads.lock().unwrap().1.len();
    engine.look_at_prs().await;
    assert_eq!(reads.lock().unwrap().1.len(), asked, "nothing is read once the person stops following");
    engine.shutdown();
}

/// An address only read (a file, a page) never lets `land.mjs`'s bare number bind another repository's pull request.
#[tokio::test]
async fn r_next_a_number_is_bound_only_to_an_address_the_conversation_opened() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, _reads) = engine(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let id = engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id.clone()),
            text: "queue it".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(step("r1", "cat notes.md")).await;
    pipe.say(result("r1", "see https://github.com/evil/other/pull/93")).await;
    pipe.say(step("s1", "rg 'gh pr create' README.md")).await;
    pipe.say(result("s1", "README.md: run gh pr create, then https://github.com/evil/other/pull/94")).await;
    pipe.say(step("q1", "node scripts/land.mjs 93")).await;
    pipe.say(result("q1", "#93 is queued as 1a2b3c4d5 on queue/pr-93.")).await;
    pipe.say(done()).await;
    let messages = until(&engine, &id, "the turn ends", |m| m.last().is_some_and(|m| m.status == Some(Status::Done))).await;
    assert_eq!(follow_of(&messages), None, "nothing this conversation opened was bound");
    engine.shutdown();
}

/// The pull request's own run goes green in minutes; the queue's status on the same commit can turn red half an hour later
/// (`land.mjs` posts it). Green does not end the following while the pull request is open, so that red wakes the agent once.
#[tokio::test]
async fn r_next_a_red_queue_after_green_checks_wakes_the_agent_once() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = engine(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (id, mut pipe) = opened(&engine, &made, &project.id).await;
    reads.lock().unwrap().0.push_back(look(PrState::Open, CiState::Passing, "aaa"));
    engine.look_at_prs().await;
    assert_eq!(follow_of(&engine.open(&id).unwrap().messages).map(|f| f.state), Some(FollowState::Passing));
    let mut queue = look(PrState::Open, CiState::Failing, "aaa");
    queue.failures = vec![Failure { name: "queue".into(), link: None, said: Some("Queued, but its checks failed.".into()) }];
    reads.lock().unwrap().0.push_back(queue.clone());
    engine.look_at_prs().await;
    let messages = until(&engine, &id, "a turn of Faber's own", |m| m.len() == 4).await;
    let wake = messages[2].wake.clone().expect("the message is marked as Faber's");
    assert_eq!((wake.why.as_str(), wake.checks.as_slice()), ("wake.checksFailed", &["queue".to_string()][..]));
    pipe.say(done()).await;
    until(&engine, &id, "the fix turn ends", |m| m.last().is_some_and(|m| m.status == Some(Status::Done))).await;
    reads.lock().unwrap().0.push_back(queue);
    engine.look_at_prs().await;
    assert_eq!(engine.open(&id).unwrap().messages.len(), 4, "once");
    reads.lock().unwrap().0.push_back(look(PrState::Merged, CiState::Passing, "aaa"));
    engine.look_at_prs().await;
    assert_eq!(follow_of(&engine.open(&id).unwrap().messages).map(|f| f.state), Some(FollowState::Merged));
    engine.shutdown();
}

/// Faber's own flow: `land.mjs` prints only the number, which binds the pull request whose address the conversation showed.
#[tokio::test]
async fn r_next_a_queued_pull_request_is_followed_again_after_it_passed() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = engine(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (id, mut pipe) = opened(&engine, &made, &project.id).await;
    reads.lock().unwrap().0.push_back(look(PrState::Open, CiState::Passing, "aaa"));
    engine.look_at_prs().await;
    assert_eq!(follow_of(&engine.open(&id).unwrap().messages).map(|f| f.state), Some(FollowState::Passing), "green is told quietly");

    engine
        .send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: Some(project.id.clone()),
            text: "queue it".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    until(&engine, &id, "the second turn starts", |m| m.len() == 4).await;
    pipe.say(step("q1", "node scripts/land.mjs 93")).await;
    pipe.say(result("q1", "#93 is queued as 1a2b3c4d5 on queue/pr-93. CI checks it, lands it and marks the pull request merged.")).await;
    pipe.say(done()).await;
    let messages = until(&engine, &id, "the queue turn ends", |m| m.len() == 4 && m[3].status == Some(Status::Done)).await;
    let f = messages[3].follow.clone().expect("followed again, under the newest reply");
    assert_eq!((f.number, f.state), (93, FollowState::Pending));
    assert!(messages[1].follow.is_none());
    engine.shutdown();
}
