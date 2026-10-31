//! Faber puts away a project conversation that is done (R222): its pull request was merged or closed, or a week passed
//! with nothing in it. Never one that is at work, waiting for the person, on Keep going, or watching an open pull request,
//! and never Chat. gh is a fake here: no process runs, and a week is a clock handed to the engine.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Conversation, Permissions, Settings, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
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
const DAY: u64 = 24 * 3600 * 1000;

type Reads = Arc<Mutex<VecDeque<Look>>>;

fn boot(dir: &support::DataFolder, spawn: Spawner) -> (Engine, Reads) {
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
    engine.set_pr_reader(Arc::new(move |_, _| fake.lock().unwrap().pop_front().ok_or("reason.remote.unreachable")));
    (engine, reads)
}

fn look(state: PrState, ci: CiState) -> Look {
    let failures = if ci == CiState::Failing { vec![Failure { name: "gate".into(), link: None, said: None }] } else { vec![] };
    Look { state, head: "aaa".into(), ci: Ci { state: ci, head: "aaa".into(), failed: vec![], total: 3 }, conflict: false, failures }
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

fn init() -> Value {
    json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })
}

fn ask(area: Area, project: &str, conversation: Option<&str>, text: &str) -> SendRequest {
    SendRequest {
        conversation_id: conversation.map(str::to_string),
        area,
        project_id: (area == Area::Project).then(|| project.to_string()),
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    }
}

async fn until(engine: &Engine, id: &str, what: &str, ok: impl Fn(&Conversation) -> bool) -> Conversation {
    for _ in 0..300 {
        let c = engine.open(id).unwrap();
        if ok(&c) {
            return c;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what}: {:?}", engine.open(id).unwrap().messages);
}

fn ended(c: &Conversation) -> bool {
    c.messages.last().is_some_and(|m| m.status == Some(Status::Done))
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

/// A turn that answers and ends.
async fn answered(engine: &Engine, made: &Arc<Mutex<Vec<support::Pipe>>>, req: SendRequest) -> (String, support::Pipe) {
    let id = engine.send(req).unwrap();
    let mut pipe = next_pipe(made).await;
    pipe.say(init()).await;
    pipe.say(done()).await;
    until(engine, &id, "the turn ends", ended).await;
    (id, pipe)
}

/// A turn in which the agent opens pull request #93, run to its end; Faber may then spend turns of its own.
async fn opened(engine: &Engine, made: &Arc<Mutex<Vec<support::Pipe>>>, project: &str) -> (String, support::Pipe) {
    let id = engine.send(ask(Area::Project, project, None, "fix the bug and open a pull request")).unwrap();
    let mut pipe = next_pipe(made).await;
    pipe.say(init()).await;
    pipe.say(step("b1", "git push -u origin fix && gh pr create --fill")).await;
    pipe.say(result("b1", &format!("Creating pull request for fix into main in acme/app\n\n{URL}\n"))).await;
    pipe.say(done()).await;
    until(engine, &id, "the turn ends", ended).await;
    let plan = Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![EngineEvent::Signals(plan)]);
    (id, pipe)
}

fn archived(engine: &Engine, id: &str) -> bool {
    engine.open(id).unwrap().archived_at.is_some()
}

#[tokio::test]
async fn auto_archive_spares_pinned_conversations_for_both_age_and_merged_pull_requests() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = boot(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (idle, _a) = answered(&engine, &made, ask(Area::Project, &project.id, None, "look around")).await;
    engine.pin(&idle, true).unwrap();
    engine.archive_at(now() + 8 * DAY, None);
    assert!(!archived(&engine, &idle), "a pin keeps an old conversation");

    let (merged, _b) = opened(&engine, &made, &project.id).await;
    engine.pin(&merged, true).unwrap();
    reads.lock().unwrap().push_back(look(PrState::Merged, CiState::Passing));
    engine.look_at_prs().await;
    assert!(!archived(&engine, &merged), "a pin also keeps a conversation whose pull request ended");
    engine.archive_at(now() + 8 * DAY, None);
    assert!(!archived(&engine, &merged));
    engine.pin(&idle, false).unwrap();
    engine.pin(&merged, false).unwrap();
    engine.archive_at(now() + 8 * DAY, None);
    assert!(archived(&engine, &idle));
    assert!(archived(&engine, &merged));
    engine.shutdown();
}

#[tokio::test]
async fn r_next_a_merged_or_closed_pull_request_archives_its_conversation_and_an_open_one_does_not() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = boot(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (merged, _a) = opened(&engine, &made, &project.id).await;
    reads.lock().unwrap().push_back(look(PrState::Open, CiState::Pending));
    engine.look_at_prs().await;
    assert!(!archived(&engine, &merged), "an open pull request is still watched");
    reads.lock().unwrap().push_back(look(PrState::Merged, CiState::Passing));
    engine.look_at_prs().await;
    assert!(archived(&engine, &merged), "merged: put away as soon as it is read");
    assert!(engine.state().conversations.iter().any(|c| c.id == merged && c.archived));
    // The person takes it back: the pull request that ended does not put it away again.
    engine.archive(&merged, false).unwrap();
    engine.archive_at(now(), None);
    assert!(!archived(&engine, &merged));

    let (closed, _b) = opened(&engine, &made, &project.id).await;
    reads.lock().unwrap().push_back(look(PrState::Closed, CiState::Pending));
    engine.look_at_prs().await;
    assert!(archived(&engine, &closed), "closed counts the same");
    engine.shutdown();
}

/// A green pull request stays followed until CI merges it (a queue run may still turn it red): the daily pass leaves it, and
/// archives it once the follow reads it merged.
#[tokio::test]
async fn r_next_a_green_pull_request_is_archived_once_it_is_merged() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = boot(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (id, _pipe) = opened(&engine, &made, &project.id).await;
    reads.lock().unwrap().push_back(look(PrState::Open, CiState::Passing));
    engine.look_at_prs().await;
    assert!(!archived(&engine, &id), "green is not merged");
    engine.archive_at(now(), None);
    assert!(!archived(&engine, &id), "still followed: it is not put away");
    reads.lock().unwrap().push_back(look(PrState::Merged, CiState::Passing));
    engine.look_at_prs().await;
    engine.archive_at(now(), None);
    assert!(archived(&engine, &id));
    engine.shutdown();
}

#[tokio::test]
async fn r_next_a_week_with_nothing_in_a_project_conversation_archives_it_but_never_a_chat() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, _reads) = boot(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let (id, _a) = answered(&engine, &made, ask(Area::Project, &project.id, None, "look around")).await;
    let (chat, _b) = answered(&engine, &made, ask(Area::Chat, "", None, "hello")).await;
    engine.archive_at(now() + 6 * DAY, None);
    assert!(!archived(&engine, &id), "six days is not a week");
    engine.archive_at(now() + 8 * DAY, None);
    assert!(archived(&engine, &id));
    assert!(!archived(&engine, &chat), "Chat is never put away");

    // Brought back, it is as new as a message: the week starts again.
    let at = engine.open(&id).unwrap().archived_at.unwrap();
    engine.archive(&id, false).unwrap();
    assert!(engine.open(&id).unwrap().updated_at >= at);
    engine.archive_at(now() + 6 * DAY, None);
    assert!(!archived(&engine, &id));
    engine.shutdown();
}

#[tokio::test]
async fn r_next_nothing_at_work_or_owed_to_the_person_is_archived() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, _reads) = boot(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let late = now() + 8 * DAY;

    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    // Answering.
    let id = engine.send(ask(Area::Project, &project.id, None, "work")).unwrap();
    let mut pipe = next_pipe(&made).await;
    pipe.say(init()).await;
    engine.archive_at(late, None);
    assert!(!archived(&engine, &id), "a turn is running");

    // Waiting for the person: a permission card on the reply.
    pipe.say(json!({ "type": "control_request", "request_id": "r1", "request": { "subtype": "can_use_tool", "tool_name": "Write", "input": { "file_path": "a.txt", "content": "x" } } })).await;
    until(&engine, &id, "the card", |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
    engine.archive_at(late, None);
    assert!(!archived(&engine, &id), "a question waits for the person");
    engine.shutdown();

    // Keep going on.
    let (spawn, made) = pipes();
    let (engine, _reads) = boot(&dir, spawn);
    let (id, _pipe) = answered(&engine, &made, ask(Area::Project, &project.id, None, "work")).await;
    let plan = Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![EngineEvent::Signals(plan)]);
    engine.keep_going(&id, true).unwrap();
    engine.archive_at(late, None);
    assert!(!archived(&engine, &id), "Keep going is on");
    engine.shutdown();
}

#[tokio::test]
async fn r_next_a_pull_request_still_watched_and_a_conversation_written_to_since_are_not_archived() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (engine, reads) = boot(&dir, spawn);
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let late = now() + 8 * DAY;

    // Merged, but the person wrote to the conversation after that: its work went on.
    let (moved, mut pipe) = opened(&engine, &made, &project.id).await;
    reads.lock().unwrap().push_back(look(PrState::Merged, CiState::Passing));
    engine.look_at_prs().await;
    assert!(archived(&engine, &moved));
    engine.send(ask(Area::Project, &project.id, Some(&moved), "now something else")).unwrap();
    pipe.say(done()).await;
    until(&engine, &moved, "the new turn ends", |c| ended(c) && c.messages.len() == 4).await;
    engine.archive_at(now(), None);
    assert!(!archived(&engine, &moved), "it was written to after its pull request ended");

    // Red checks while the quota is tight: the person is told, the pull request is watched, the conversation waits.
    let (watched, _a) = opened(&engine, &made, &project.id).await;
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    engine.note_account_events(Vendor::Claude, vec![EngineEvent::Quota { window: "five_hour".into(), used: 0.95, resets_at: at + 3600.0 }]);
    reads.lock().unwrap().push_back(look(PrState::Open, CiState::Failing));
    engine.look_at_prs().await;
    engine.archive_at(late, None);
    assert!(!archived(&engine, &watched), "an open pull request keeps its conversation");

    engine.shutdown();
}

/// What the scripted repository answers: whether the copy has uncommitted changes, and whether another branch holds its commit.
#[derive(Default)]
struct Repo {
    dirty: bool,
    held: bool,
}

#[derive(Clone)]
struct Scripted(Arc<Mutex<Repo>>);

impl Executor for Scripted {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        let repo = self.0.lock().unwrap();
        let args: Vec<&str> = call.args.iter().map(String::as_str).collect();
        let say = |text: &str| Ok(Output { success: true, stdout: text.as_bytes().to_vec() });
        // The fixed `-c key value` pairs come first; what follows is the command.
        let mut command = args.as_slice();
        while command.first() == Some(&"-c") {
            command = &command[2..];
        }
        match command {
            ["status", ..] => say(if repo.dirty { " M a.rs\0" } else { "" }),
            ["rev-parse", ..] => say("1111111111111111111111111111111111111111\n"),
            ["for-each-ref", ..] => say(if repo.held { "refs/heads/faber/x\nrefs/heads/main\n" } else { "refs/heads/faber/x\n" }),
            other => panic!("unexpected command {other:?}"),
        }
    }
}

/// A week quiet is not enough while the conversation's own copy holds work found nowhere else.
#[tokio::test]
async fn r_next_a_quiet_conversation_whose_copy_holds_unsaved_work_is_not_archived() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let (first, _reads) = boot(&dir, spawn);
    let project = first.add_project(&dir.path().to_string_lossy()).unwrap();
    let (id, _pipe) = answered(&first, &made, ask(Area::Project, &project.id, None, "change things")).await;
    first.shutdown();
    // The conversation gets a copy, as one made at its first message would be (the folder exists; the repository is the script).
    let file = dir.path().join("conversations").join(format!("{id}.json"));
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    let folder = dir.path().to_string_lossy().into_owned();
    saved["copy"] = json!({ "path": folder, "branch": "faber/x", "root": folder });
    std::fs::write(&file, serde_json::to_vec(&saved).unwrap()).unwrap();

    let (spawn, _made) = pipes();
    let (engine, _reads) = boot(&dir, spawn);
    let repo = Arc::new(Mutex::new(Repo { dirty: true, held: false }));
    let script = Scripted(repo.clone());
    engine.set_git(Arc::new(move || Some(Box::new(script.clone()) as Box<dyn Executor + Send>)));
    let week = now() + 8 * DAY;
    engine.archive_at(week, None);
    assert!(!archived(&engine, &id), "uncommitted changes");
    repo.lock().unwrap().dirty = false;
    engine.archive_at(week, None);
    assert!(!archived(&engine, &id), "commits on its branch alone");
    repo.lock().unwrap().held = true;
    engine.archive_at(week, None);
    assert!(archived(&engine, &id), "saved elsewhere: put away");
    engine.shutdown();
}
