//! Standing goals (phase 1): a project keeps goals, each one of its confirmed checks. While the person's switch is on, Faber
//! runs a due check when the project is quiet, and starts a conversation in a copy of its own when a check fails twice.
//! The checks, git and the CLI are all fakes here: nothing is run, nothing real is asked, and the clock is the test's.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Permissions, Settings, Vendor};
use faber_engine::project::checks;
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::project::goals::DAILY_CAP_USD;
use faber_engine::project::remote::{Ci, CiState, Failure, Look, PrState};
use faber_engine::router::Choice;
use faber_engine::signals::Signals;
use faber_engine::verify::{Runner, Spec, Started, Verifier};
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const HEAD: &str = "1111111111111111111111111111111111111111";
const WORDS: &str = "every check passes";
const CHECK: &str = "cargo test";

/// The project's check: it passes until a test says it fails, and then prints `text`.
#[derive(Default)]
struct Fake {
    fails: AtomicBool,
    text: Mutex<String>,
    runs: AtomicUsize,
    /// Done once, as the next run starts: something that happens while a check runs.
    during: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Runner for Fake {
    fn start(&self, _spec: &Spec) -> std::io::Result<Started> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let hook = self.during.lock().unwrap().take();
        if let Some(hook) = hook {
            hook();
        }
        let failing = self.fails.load(Ordering::SeqCst);
        let out = if failing { self.text.lock().unwrap().clone() } else { String::new() };
        Ok(Started {
            confirmed: None,
            throttle: None,
            peak: None,
            stdout: Box::new(std::io::Cursor::new(out.into_bytes())),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async move { Some(if failing { 101 } else { 0 }) }),
        })
    }
}

/// What the scripted repository holds: whether the person's folder has changes, and what git was asked.
#[derive(Default)]
struct Repo {
    root: PathBuf,
    dirty: bool,
    calls: Vec<Vec<String>>,
}

#[derive(Clone)]
struct Git(Arc<Mutex<Repo>>);

impl Executor for Git {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        let mut args = call.args.as_slice();
        while args.first().is_some_and(|a| a == "-c") {
            args = &args[2..];
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut repo = self.0.lock().unwrap();
        repo.calls.push(args.iter().map(|a| a.to_string()).collect());
        let say = |text: &str| Ok(Output { success: true, stdout: text.as_bytes().to_vec() });
        let no = || Ok(Output { success: false, stdout: vec![] });
        match args.as_slice() {
            ["rev-parse", "--show-toplevel", "--show-prefix"] => say(&format!("{}\n\n", repo.root.display())),
            ["rev-parse", "--show-toplevel"] => say(&format!("{}\n", repo.root.display())),
            ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"] => say(&format!("{HEAD}\n")),
            ["rev-parse", "--verify", "--quiet", "HEAD"] => say(&format!("{HEAD}\n")),
            ["symbolic-ref", "--quiet", "HEAD"] if call.cwd == repo.root => say("refs/heads/main\n"),
            ["symbolic-ref", "--quiet", "HEAD"] => {
                let branch = std::fs::read_to_string(call.cwd.join(".branch")).unwrap_or_default();
                say(&format!("refs/heads/{branch}\n"))
            }
            ["symbolic-ref", "--quiet", "--short", "HEAD"] => say(&std::fs::read_to_string(call.cwd.join(".branch")).unwrap_or_default()),
            ["rev-parse", "--path-format=absolute", "--git-common-dir"] => say(&format!("{}\n", repo.root.join(".git").display())),
            ["rev-parse", "--verify", "--quiet", r] if r.starts_with("refs/heads/faber/") => {
                let name = r.trim_start_matches("refs/heads/");
                let made = repo.calls.iter().any(|c| c.iter().any(|a| a == name) && c.first().is_some_and(|a| a == "worktree"));
                if made { say(&format!("{HEAD}\n")) } else { no() }
            }
            ["worktree", "add", "-b", branch, "--", path, _] | ["worktree", "add", "--", path, branch] => {
                std::fs::create_dir_all(path).unwrap();
                std::fs::write(std::path::Path::new(path).join(".branch"), branch).unwrap();
                say("")
            }
            ["status", "--porcelain", "-z"] => say(if repo.dirty { " M src/lib.rs\0" } else { "" }),
            ["merge-base", "HEAD", "HEAD"] => say(&format!("{HEAD}\n")),
            ["diff", "--name-only", ..] => say(if repo.dirty { "src/lib.rs\0" } else { "" }),
            ["ls-files", ..] => say(""),
            ["for-each-ref", ..] | ["merge-base", ..] | ["diff", ..] => no(),
            ["worktree", "remove", "--force", "--", path] => {
                let _ = std::fs::remove_dir_all(path);
                say("")
            }
            ["worktree", "prune"] | ["branch", "-D", _] => say(""),
            other => panic!("unexpected git command {other:?}"),
        }
    }
}

fn real_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

struct Rig {
    engine: Engine,
    runner: Arc<Fake>,
    repo: Arc<Mutex<Repo>>,
    made: Arc<Mutex<Vec<support::Pipe>>>,
    events: Arc<Mutex<Vec<AppEvent>>>,
    in_front: Arc<AtomicBool>,
    skew: Arc<AtomicI64>,
    project: String,
    _folders: (tempfile::TempDir, tempfile::TempDir),
}

impl Rig {
    fn runs(&self) -> usize {
        self.runner.runs.load(Ordering::SeqCst)
    }

    fn git_calls(&self) -> usize {
        self.repo.lock().unwrap().calls.len()
    }

    /// Moves the engine's clock forward.
    fn advance(&self, secs: i64) {
        self.skew.fetch_add(secs, Ordering::SeqCst);
    }

    fn fail(&self, text: &str) {
        *self.runner.text.lock().unwrap() = text.into();
        self.runner.fails.store(true, Ordering::SeqCst);
    }

    fn view(&self) -> faber_engine::project::goals::View {
        self.engine.project_goals(&self.project).unwrap()
    }

    fn state(&self) -> &'static str {
        self.view().goals[0].state
    }

    fn conversations(&self) -> Vec<String> {
        self.engine.state().conversations.into_iter().filter(|c| c.title == WORDS).map(|c| c.id).collect()
    }

    fn told(&self) -> usize {
        self.events.lock().unwrap().iter().filter(|e| matches!(e, AppEvent::Halted { why, .. } if why == "reason.goal.needsYou")).count()
    }

    /// The conversation ids the tray notices carried.
    fn told_ids(&self) -> Vec<String> {
        let events = self.events.lock().unwrap();
        events
            .iter()
            .filter_map(|e| match e {
                AppEvent::Halted { conversation_id, .. } => Some(conversation_id.clone()),
                _ => None,
            })
            .collect()
    }

    /// `projects.json` on disk.
    fn projects_file(&self) -> PathBuf {
        self._folders.0.path().join("projects.json")
    }

    /// One look of the watcher, then a moment for what it started.
    async fn tick(&self) {
        self.engine.goals_tick();
        tokio::time::sleep(Duration::from_millis(60)).await;
    }

    /// Ticks until the check has run `n` times in all and its answer is recorded.
    async fn ran(&self, n: usize) {
        for _ in 0..300 {
            self.engine.goals_tick();
            if self.runs() >= n && !self.engine.is_working() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the check ran {} times, not {n}", self.runs());
    }
}

async fn until(mut done: impl FnMut() -> bool) {
    for _ in 0..300 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("it never happened");
}

async fn rig(billed: bool) -> Rig {
    rig_with(billed, false).await
}

/// `both`: Codex is connected too, with no plan known, so Faber starts nothing on it by itself but a helper can be offered.
async fn rig_with(billed: bool, both: bool) -> Rig {
    let data = support::work_folder();
    let folder = support::work_folder();
    std::fs::write(folder.path().join("AGENTS.md"), format!("## Proof commands\n\n```\n{CHECK}\n```\n")).unwrap();
    let (spawn, made) = support::pipes();
    let events: Arc<Mutex<Vec<AppEvent>>> = Arc::default();
    let seen = events.clone();
    let opts = EngineOptions {
        data_dir: data.path().into(),
        clis: BTreeMap::from(Vendor::ALL.map(|v| (v, Cli { command: v.name().into(), prefix: vec![] })))
            .into_iter()
            .filter(|(v, _)| both || *v == Vendor::Claude)
            .collect(),
        spawn,
        windows: false,
        claude_env_billed: billed,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        // Auto's router answers nothing, so the everyday default answers and no second process starts.
        ask: Some(Arc::new(|_| Box::pin(async { None }))),
    };
    let engine = Engine::new(opts, Arc::new(move |e| seen.lock().unwrap().push(e)));
    engine.note_account_events(
        Vendor::Claude,
        vec![EngineEvent::Signals(Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() })],
    );
    let repo = Arc::new(Mutex::new(Repo { root: folder.path().into(), ..Repo::default() }));
    let git = Git(repo.clone());
    engine.set_git(Arc::new(move || Some(Box::new(git.clone()) as Box<dyn Executor + Send>)));
    let runner = Arc::new(Fake::default());
    let in_front = Arc::new(AtomicBool::new(false));
    let guard = in_front.clone();
    engine.set_verifier(Arc::new(Verifier::new(runner.clone()).with_guard(Arc::new(move || guard.load(Ordering::SeqCst)))));
    let skew = Arc::new(AtomicI64::new(0));
    let clock = skew.clone();
    engine.set_time_source(Arc::new(move || real_secs() + clock.load(Ordering::SeqCst) as f64));
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    let confirmed = checks::confirm(&project, &[CHECK.into()], &[], &[CHECK.into()]).unwrap();
    engine.update_project(&project, &confirmed).unwrap();
    engine.add_project_goal(&project.id, WORDS, CHECK).unwrap();
    engine.set_project_goals(&project.id, true).unwrap();
    Rig { engine, runner, repo, made, events, in_front, skew, project: project.id, _folders: (data, folder) }
}

/// The first line a CLI process was sent as the person's message.
fn brief(agent: &support::Pipe) -> String {
    agent.lines.lock().unwrap().iter().find(|m| m["type"] == "user").map(|m| m.to_string()).unwrap_or_default()
}

fn person_asks(project: &str, text: &str) -> SendRequest {
    SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.into()),
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
    }
}

async fn finish(agent: &mut support::Pipe, cost: f64) {
    agent.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    agent.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":cost,"usage":{}})).await;
}

/// Two failures ten minutes apart start the fix; the rig's first tick sets the clock of "five minutes after start".
async fn break_it(r: &Rig) -> support::Pipe {
    r.fail("assertion failed: parser keeps the comment\nthread 'parse' panicked at src/lib.rs:12:5\n");
    r.tick().await;
    assert_eq!(r.runs(), 0, "nothing is checked in the first five minutes");
    r.advance(301);
    r.ran(1).await;
    assert!(r.conversations().is_empty(), "one failure only waits");
    r.advance(200);
    r.tick().await;
    assert_eq!(r.runs(), 1, "ten minutes have not passed since the first failure");
    r.advance(401);
    r.engine.goals_tick();
    until(|| !r.conversations().is_empty()).await;
    support::next_pipe(&r.made).await
}

#[tokio::test]
async fn a_goal_that_breaks_twice_starts_one_conversation_in_a_copy_and_the_same_failure_after_it_needs_the_person_once() {
    let r = rig(false).await;
    let agent = break_it(&r).await;
    assert_eq!(r.runs(), 2);
    let ids = r.conversations();
    assert_eq!(ids.len(), 1, "exactly one conversation");
    let started = r.engine.open(&ids[0]).unwrap();
    let said = brief(&agent);
    for part in
        [WORDS, CHECK, "assertion failed: parser keeps the comment", "Reproduce it first", "Never merge it", "Do not ask the person"]
    {
        assert!(said.contains(part), "the brief says {part:?}: {said}");
    }
    assert!(started.copy.as_ref().is_some_and(|c| c.path != r.repo.lock().unwrap().root.to_string_lossy()), "never in the person's folder");
    assert!(started.started_by.is_none() && started.keep_going.is_none(), "an ordinary conversation, one turn");
    let wake = started.messages[0].wake.as_ref().expect("it reads as Faber's own message");
    assert_eq!((wake.why.as_str(), wake.checks.as_slice()), ("wake.goalBroke", [WORDS.to_string()].as_slice()));
    assert_eq!(r.state(), "fixing");
    assert!(r.engine.is_working());

    // While it works, nothing else starts, however long it takes.
    r.advance(7 * 3600);
    r.tick().await;
    assert_eq!((r.runs(), r.conversations().len()), (2, 1));

    // It ends, and the same failure is still there: the fix waits for the person, and the tray is told once.
    let mut agent = agent;
    finish(&mut agent, 0.5).await;
    until(|| r.state() != "fixing").await;
    r.ran(3).await;
    assert_eq!((r.state(), r.view().goals[0].reason.as_deref()), ("needsYou", Some("reason.goal.fixTried")));
    assert_eq!(r.told(), 1);
    assert!(r.view().spent_usd > 0.4, "the fix's cost counts against the project's day");
    r.advance(7 * 3600);
    r.ran(4).await;
    assert_eq!((r.state(), r.conversations().len(), r.told()), ("needsYou", 1, 1), "no second conversation for the same failure");

    // A goal that needs the person starts nothing, whatever fails and however often, and nobody is told again.
    r.fail("error: a different thing broke in src/main.rs\n");
    for n in [5, 6] {
        r.advance(7 * 3600);
        r.ran(n).await;
    }
    assert_eq!((r.state(), r.conversations().len(), r.told()), ("needsYou", 1, 1), "it waits for the person");

    // The person merges the fix: the next check passes and the goal holds again.
    r.runner.fails.store(false, Ordering::SeqCst);
    r.advance(7 * 3600);
    r.ran(7).await;
    assert_eq!(r.state(), "holding");
    r.engine.shutdown();
}

#[tokio::test]
async fn the_daily_limit_stops_a_start_and_a_new_failure_after_the_fix_then_needs_the_person() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    finish(&mut agent, DAILY_CAP_USD + 0.5).await;
    until(|| r.view().spent_usd >= DAILY_CAP_USD).await;
    // Another failure comes after the fix, and the day's money is gone: nothing starts, and the reason is plain.
    r.fail("error: a different thing broke in src/main.rs\n");
    until(|| r.state() != "fixing").await;
    r.ran(3).await;
    assert_eq!(r.conversations().len(), 1, "a different failure only waits for a second run, even after a fix");
    r.advance(601);
    r.ran(4).await;
    assert_eq!((r.state(), r.view().goals[0].reason.as_deref()), ("needsYou", Some("reason.goal.cap")));
    assert_eq!(r.conversations().len(), 1, "no second conversation past the limit");
    assert_eq!(r.told(), 1);
    r.engine.shutdown();
}

#[tokio::test]
async fn pressing_stop_on_the_fix_pauses_the_goal_until_the_person_resumes_it() {
    let r = rig(false).await;
    let _agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    r.engine.interrupt(&id);
    assert_eq!(r.state(), "paused");
    r.advance(7 * 3600);
    r.tick().await;
    assert_eq!(r.runs(), 2, "a paused goal is not checked");
    r.engine.resume_project_goal(&r.project, &r.view().goals[0].id).unwrap();
    r.runner.fails.store(false, Ordering::SeqCst);
    r.ran(3).await;
    assert_eq!(r.state(), "holding");
    r.engine.shutdown();
}

#[tokio::test]
async fn nothing_is_checked_with_the_switch_off_or_on_a_billed_sign_in_and_nothing_else_is_asked() {
    for (billed, on) in [(false, false), (true, true)] {
        let r = rig(billed).await;
        r.engine.set_project_goals(&r.project, on).unwrap();
        r.fail("boom\n");
        r.tick().await;
        r.advance(10_000);
        r.tick().await;
        r.advance(10_000);
        r.tick().await;
        assert_eq!((r.runs(), r.git_calls(), r.conversations().len()), (0, 0, 0), "billed {billed}, switch {on}");
        assert!(!r.engine.is_working());
        r.engine.shutdown();
    }
}

#[tokio::test]
async fn a_game_in_front_a_dirty_tree_or_a_person_at_work_start_nothing_until_they_pass() {
    let r = rig(false).await;
    r.tick().await;
    r.advance(301);
    // A game in front: not even git is asked.
    r.in_front.store(true, Ordering::SeqCst);
    r.tick().await;
    assert_eq!((r.runs(), r.git_calls()), (0, 0));
    r.in_front.store(false, Ordering::SeqCst);
    // A tree with changes is asked once, and not again before five minutes.
    r.repo.lock().unwrap().dirty = true;
    r.tick().await;
    let asked = r.git_calls();
    assert!(asked > 0 && r.runs() == 0, "the tree is looked at, the check does not run");
    r.tick().await;
    assert_eq!(r.git_calls(), asked, "not asked again at once");
    r.repo.lock().unwrap().dirty = false;
    r.advance(301);
    r.ran(1).await;
    assert_eq!(r.state(), "holding");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_conversation_at_work_or_a_message_from_the_person_in_the_last_fifteen_minutes_holds_the_check_back() {
    let r = rig(false).await;
    r.tick().await;
    r.advance(301);
    // The person's turn runs (their message is stamped by the real clock, so the test clock only moves a little).
    r.engine.send(person_asks(&r.project, "look at the parser")).unwrap();
    let mut agent = support::next_pipe(&r.made).await;
    agent.say(json!({"type":"system","subtype":"init","session_id":"s9","model":"claude-haiku-5-5"})).await;
    r.tick().await;
    assert_eq!(r.runs(), 0, "a conversation of the project is at work");
    agent.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0,"usage":{}})).await;
    until(|| !r.engine.is_working()).await;
    r.tick().await;
    assert_eq!(r.runs(), 0, "the person wrote less than fifteen minutes ago");
    r.advance(700);
    r.ran(1).await;
    r.engine.shutdown();
}

#[tokio::test]
async fn a_fix_needs_separate_copies_and_full_access_and_otherwise_says_why() {
    for (copies, mode, why) in [(false, Permissions::FullAccess, "reason.goal.copies"), (true, Permissions::Ask, "reason.goal.mode")] {
        let r = rig(false).await;
        let project = r.engine.project(&r.project).unwrap();
        let mut changed = project.clone();
        changed.separate_copies = copies;
        r.engine.update_project(&project, &changed).unwrap();
        r.engine.save_settings(Settings { permissions: Some(mode), ..Settings::default() }).unwrap();
        r.fail("broken\n");
        r.tick().await;
        r.advance(301);
        r.ran(1).await;
        r.advance(601);
        r.ran(2).await;
        assert_eq!((r.state(), r.view().goals[0].reason.as_deref()), ("needsYou", Some(why)));
        assert!(r.conversations().is_empty(), "never in the person's folder, never without full access");
        assert_eq!(r.told(), 1);
        assert!(r.told_ids().iter().all(|id| !id.is_empty()), "a notice never carries an empty conversation id: {:?}", r.told_ids());
        r.engine.shutdown();
    }
}

#[tokio::test]
async fn a_goal_takes_only_a_confirmed_check_and_the_screen_is_told_the_choices() {
    let r = rig(false).await;
    assert_eq!(r.view().checks, [CHECK]);
    assert_eq!(r.engine.add_project_goal(&r.project, "x", "rm -rf /").unwrap_err(), "reason.goal.check");
    assert_eq!(r.engine.add_project_goal(&r.project, "  ", CHECK).unwrap_err(), "reason.goal.words");
    // A list that is no longer the confirmed one offers no check at all.
    std::fs::write(
        r.engine.project(&r.project).unwrap().path + "/AGENTS.md",
        "## Proof commands\n\n```\ncargo test\ncargo fmt --check\n```\n",
    )
    .unwrap();
    assert!(r.view().checks.is_empty());
    assert_eq!(r.engine.remove_project_goal(&r.project, &r.view().goals[0].id).unwrap().goals.len(), 0);
    r.engine.shutdown();
}

/// The answer Faber wrote to a driven Claude Code for one control request, once it is there.
async fn mcp_answer(pipe: &support::Pipe, request: &str) -> serde_json::Value {
    for _ in 0..300 {
        if let Some(m) = pipe.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == request) {
            return m["response"]["response"]["mcp_response"].clone();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{request} was never answered");
}

/// A first failure recorded; the clock is then at the moment the second would be due.
async fn first_failure(r: &Rig, text: &str) {
    r.fail(text);
    r.tick().await;
    r.advance(301);
    r.ran(1).await;
    assert!(r.conversations().is_empty(), "one failure only waits");
    r.advance(601);
}

#[tokio::test]
async fn once_the_days_money_is_gone_a_fix_goes_on_by_no_turn_of_faber_and_its_agent_starts_nothing() {
    let r = rig(false).await;
    r.engine.set_keep_going_pace(0.001);
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    // The turn that runs is not cut: it ends with the day's limit spent, and "keep going" has nothing to send after it.
    r.engine.keep_going(&id, true).unwrap();
    finish(&mut agent, DAILY_CAP_USD + 0.5).await;
    until(|| r.view().spent_usd >= DAILY_CAP_USD).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.last().is_some_and(|m| m.halt.as_deref() == Some("reason.goal.cap")))).await;
    let sent = agent.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").count();
    assert_eq!(sent, 1, "no second turn went to the agent");
    // Its agent may still ask for another conversation, and is told why not.
    let call = json!({"type":"control_request","request_id":"r2","request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"conversation",
        "arguments":{"title":"Docs","brief":"Write the docs."},"_meta":{"claudecode/toolUseId":"toolu_c"}}}}});
    agent.say(call).await;
    let refused = mcp_answer(&agent, "r2").await;
    let text = refused["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
    assert!(refused["result"]["isError"] == true && text.contains("limit"), "{refused}");
    assert!(r.engine.state().conversations.iter().all(|c| c.title != "Docs"), "no conversation was started");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_game_that_comes_in_front_while_the_check_runs_holds_the_fix_back_until_it_leaves() {
    let r = rig(false).await;
    first_failure(&r, "assertion failed: parser keeps the comment\n").await;
    let front = r.in_front.clone();
    *r.runner.during.lock().unwrap() = Some(Box::new(move || front.store(true, Ordering::SeqCst)));
    r.ran(2).await;
    assert!(r.conversations().is_empty() && r.made.lock().unwrap().is_empty(), "the guards are asked again before the fix starts");
    assert_eq!((r.state(), r.told()), ("holding", 0), "waiting is not a reason to bother the person");
    r.in_front.store(false, Ordering::SeqCst);
    r.advance(301);
    r.engine.goals_tick();
    until(|| !r.conversations().is_empty()).await;
    assert_eq!((r.runs(), r.conversations().len()), (3, 1), "asked again a few minutes later, the same failure starts one fix");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_start_that_cannot_be_written_down_starts_nothing_and_is_tried_again() {
    let r = rig(false).await;
    first_failure(&r, "assertion failed: parser keeps the comment\n").await;
    // A folder where the temporary file goes: nothing can be saved to projects.json.
    let blocker = r.projects_file().with_file_name("projects.json.tmp");
    std::fs::create_dir(&blocker).unwrap();
    r.ran(2).await;
    assert!(r.conversations().is_empty() && r.made.lock().unwrap().is_empty(), "no conversation starts whose start cannot be counted");
    assert_eq!(r.told(), 0, "a notice is not sent for a change that was not kept");
    std::fs::remove_dir(&blocker).unwrap();
    r.advance(301);
    until(|| {
        r.engine.goals_tick();
        !r.conversations().is_empty()
    })
    .await;
    assert_eq!(r.conversations().len(), 1);
    r.engine.shutdown();
}

#[tokio::test]
async fn only_the_same_failure_twice_starts_a_fix_a_different_one_waits_again() {
    let r = rig(false).await;
    first_failure(&r, "assertion failed: parser keeps the comment\n").await;
    r.fail("error: a different thing broke in src/main.rs\n");
    r.ran(2).await;
    assert!(r.conversations().is_empty(), "two different failures are not one failure twice");
    r.advance(601);
    until(|| {
        r.engine.goals_tick();
        !r.conversations().is_empty()
    })
    .await;
    assert_eq!(r.conversations().len(), 1, "the different failure, seen twice, starts it");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_goals_record_that_cannot_be_read_is_never_written_over_with_an_empty_one() {
    let r = rig(false).await;
    let mut file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(r.projects_file()).unwrap()).unwrap();
    file[0]["goals"]["auto"] = json!("yes");
    std::fs::write(r.projects_file(), serde_json::to_string_pretty(&file).unwrap()).unwrap();
    let before = std::fs::read(r.projects_file()).unwrap();
    assert_eq!(r.engine.project_goals(&r.project).unwrap_err(), "error.store.corrupt");
    assert_eq!(r.engine.set_project_goals(&r.project, true).unwrap_err(), "error.store.corrupt");
    assert_eq!(r.engine.add_project_goal(&r.project, "more", CHECK).unwrap_err(), "error.store.corrupt");
    assert_eq!(r.engine.remove_project_goal(&r.project, "g").unwrap_err(), "error.store.corrupt");
    assert_eq!(std::fs::read(r.projects_file()).unwrap(), before, "the file is exactly as it was");
    r.engine.shutdown();
}

#[tokio::test]
async fn goals_are_read_again_when_the_first_read_failed() {
    let r = rig(false).await;
    let good = std::fs::read(r.projects_file()).unwrap();
    std::fs::write(r.projects_file(), "not json").unwrap();
    r.tick().await;
    std::fs::write(r.projects_file(), good).unwrap();
    r.advance(301);
    r.ran(1).await;
    assert_eq!(r.state(), "holding");
    r.engine.shutdown();
}

#[tokio::test]
async fn turning_the_switch_off_while_a_check_runs_leaves_the_goal_holding_and_tells_nobody() {
    let r = rig(false).await;
    first_failure(&r, "assertion failed: parser keeps the comment\n").await;
    let (engine, project) = (r.engine.clone(), r.project.clone());
    *r.runner.during.lock().unwrap() = Some(Box::new(move || drop(engine.set_project_goals(&project, false))));
    r.ran(2).await;
    assert_eq!((r.state(), r.told(), r.conversations().len()), ("holding", 0, 0));
    assert_eq!(r.view().goals[0].reason, None);
    assert!(!r.view().auto);
    r.engine.shutdown();
}

/// A call of a Faber tool by the agent behind `agent`: whether it was refused, and the words it got back.
async fn tool(agent: &mut support::Pipe, request: &str, name: &str, arguments: serde_json::Value) -> (bool, String) {
    if name == "helper" {
        let tool_use = json!({"type":"tool_use","id":format!("toolu_{request}"),"name":"mcp__faber__helper","input":arguments});
        agent.say(json!({"type":"assistant","parent_tool_use_id":null,"message":{"content":[tool_use]}})).await;
    }
    let message = json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments,
        "_meta":{"claudecode/toolUseId":format!("toolu_{request}")}}});
    agent.say(json!({"type":"control_request","request_id":request,"request":{"subtype":"mcp_message","server_name":"faber","message":message}})).await;
    let answer = mcp_answer(agent, request).await;
    (answer["result"]["isError"] == true, answer["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string())
}

fn user_lines(agent: &support::Pipe) -> usize {
    agent.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").count()
}

/// The fix's turn ends with a total cost of `cost` so far, its check passes again, and the goal lets go of the fix.
async fn let_go(r: &Rig, agent: &mut support::Pipe, cost: f64) {
    agent.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":cost,"usage":{}})).await;
    until(|| r.state() != "fixing").await;
    r.runner.fails.store(false, Ordering::SeqCst);
    r.ran(3).await;
    assert_eq!((r.state(), r.view().goals[0].conversation.clone()), ("holding", None), "the goal let go of its fix");
}

#[tokio::test]
async fn a_fix_stays_held_to_the_days_money_and_counts_its_cost_after_its_goal_let_go_of_it() {
    let r = rig_with(false, true).await;
    r.engine.set_keep_going_pace(0.001);
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    agent.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    let_go(&r, &mut agent, 0.5).await;
    // The person goes on in that conversation, and the agent's next turn takes the day past its limit: the cost still counts.
    r.engine.send(SendRequest { conversation_id: Some(id.clone()), ..person_asks(&r.project, "one more thing") }).unwrap();
    r.engine.keep_going(&id, true).unwrap();
    let mut agent = support::next_pipe(&r.made).await;
    agent.say(json!({"type":"system","subtype":"init","session_id":"s2","model":"claude-haiku-5-5"})).await;
    agent.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":DAILY_CAP_USD + 0.5,"usage":{}})).await;
    until(|| r.view().spent_usd >= DAILY_CAP_USD).await;
    // The wake after it is refused, and the reply says why. Its agent may start nothing and call no helper.
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.last().is_some_and(|m| m.halt.as_deref() == Some("reason.goal.cap")))).await;
    assert_eq!(user_lines(&agent), 1, "no second turn went to the agent that took the day past its limit");
    let (refused, text) = tool(&mut agent, "r2", "conversation", json!({"title":"Docs","brief":"Write the docs."})).await;
    assert!(refused && text.contains("limit"), "{text}");
    let (refused, text) = tool(&mut agent, "r3", "helper", json!({"title":"Check","brief":"What is the answer?"})).await;
    assert!(refused && text.contains("limit"), "a helper costs too: {refused} {text}");
    assert!(r.engine.state().conversations.iter().all(|c| c.title != "Docs"));
    r.engine.shutdown();
}

#[tokio::test]
async fn a_followed_pull_request_wakes_no_fix_over_the_days_money_even_when_its_goal_let_go_of_it() {
    let r = rig(false).await;
    let reads: Arc<Mutex<VecDeque<Look>>> = Arc::default();
    let fake = reads.clone();
    r.engine.set_pr_reader(Arc::new(move |_, _| fake.lock().unwrap().pop_front().ok_or("reason.remote.unreachable")));
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    // The fix opens a pull request and ends with the day's money spent; its check passes, so the goal lets go of it.
    agent.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    let open = json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"b1","name":"Bash",
        "input":{"command":"git push -u origin fix && gh pr create --fill"}}]}});
    agent.say(open).await;
    let url = json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"b1",
        "content":"https://github.com/acme/app/pull/93\n","is_error":false}]}});
    agent.say(url).await;
    let_go(&r, &mut agent, DAILY_CAP_USD + 0.5).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.iter().any(|m| m.follow.is_some()))).await;
    // Its checks fail: Faber does not spend a turn on it, and the reply says the day's limit is why.
    let failing = Failure { name: "gate".into(), link: None, said: None };
    reads.lock().unwrap().push_back(Look {
        state: PrState::Open,
        head: "aaa".into(),
        ci: Ci { state: CiState::Failing, head: "aaa".into(), failed: vec![], total: 3 },
        conflict: false,
        failures: vec![failing],
    });
    r.engine.look_at_prs().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(user_lines(&agent), 1, "the pull request's red checks sent the agent nothing");
    let last = r.engine.open(&id).unwrap().messages.last().cloned().unwrap();
    assert_eq!(last.halt.as_deref(), Some("reason.goal.cap"), "the person is told why, not only that it is held");
    r.engine.shutdown();
}

/// The project is saved again, as Settings does, so the goals are read from the file at the next look.
fn reload(r: &Rig) {
    let before = r.engine.project(&r.project).unwrap();
    let mut after = before.clone();
    after.separate_copies = !after.separate_copies;
    r.engine.update_project(&before, &after).unwrap();
}

/// The fix's agent asks for another conversation, and is told the day's limit is why.
async fn refused_for_the_limit(r: &Rig, agent: &mut support::Pipe) {
    let (refused, text) = tool(agent, "r9", "conversation", json!({"title":"Docs","brief":"Write the docs."})).await;
    assert!(refused && text.contains("limit"), "{text}");
    assert!(r.engine.state().conversations.iter().all(|c| c.title != "Docs"), "no conversation was started");
}

#[tokio::test]
async fn a_fix_whose_goals_record_cannot_be_read_has_no_money() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    reload(&r);
    let mut file: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(r.projects_file()).unwrap()).unwrap();
    file[0]["goals"]["auto"] = json!("yes");
    std::fs::write(r.projects_file(), serde_json::to_string_pretty(&file).unwrap()).unwrap();
    refused_for_the_limit(&r, &mut agent).await;
    r.engine.shutdown();
}

#[tokio::test]
async fn a_fix_has_no_money_while_the_projects_list_cannot_be_read() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    reload(&r);
    std::fs::write(r.projects_file(), "not json").unwrap();
    refused_for_the_limit(&r, &mut agent).await;
    r.engine.shutdown();
}

#[tokio::test]
async fn a_cost_that_could_not_be_written_still_counts_against_the_day_and_is_written_with_the_next_change() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    let blocker = r.projects_file().with_file_name("projects.json.tmp");
    std::fs::create_dir(&blocker).unwrap();
    finish(&mut agent, DAILY_CAP_USD + 0.5).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.spent_usd >= DAILY_CAP_USD)).await;
    assert_eq!(r.view().spent_usd, 0.0, "the cost could not be written");
    refused_for_the_limit(&r, &mut agent).await;
    std::fs::remove_dir(&blocker).unwrap();
    assert!(r.engine.set_project_goals(&r.project, true).unwrap().spent_usd >= DAILY_CAP_USD, "the next write pays what was owed");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_fix_asks_only_the_conversations_it_started() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    let other = r.engine.send(person_asks(&r.project, "hello")).unwrap();
    let _person = support::next_pipe(&r.made).await;
    let (refused, text) = tool(&mut agent, "a1", "conversation", json!({"action":"ask","id":other,"message":"please work on this"})).await;
    assert!(refused && text.contains("only the conversations it started"), "{text}");
    let (refused, text) = tool(&mut agent, "a2", "conversation", json!({"title":"Docs","brief":"Write the docs."})).await;
    assert!(!refused, "{text}");
    let child = r.engine.state().conversations.into_iter().find(|c| c.title == "Docs").unwrap().id;
    assert_eq!(
        r.engine.open(&child).unwrap().goal_fix.as_deref(),
        Some(r.project.as_str()),
        "what the fix starts is held to the same money"
    );
    let (refused, text) = tool(&mut agent, "a3", "conversation", json!({"action":"ask","id":child,"message":"how far are you?"})).await;
    assert!(!refused, "its own child can be asked: {text}");
    r.engine.shutdown();
}

#[tokio::test]
async fn after_a_fix_a_different_failure_waits_for_a_second_run_like_the_first() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    finish(&mut agent, 0.1).await;
    until(|| r.state() != "fixing").await;
    r.fail("error: a different thing broke in src/main.rs\n");
    r.ran(3).await;
    assert_eq!(r.conversations().len(), 1, "one sighting of a different failure starts nothing");
    r.advance(601);
    until(|| {
        r.engine.goals_tick();
        r.conversations().len() == 2
    })
    .await;
    assert_eq!(r.runs(), 4, "the same failure, seen twice, starts a second fix");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_report_waiting_in_a_fixs_queue_is_dropped_with_a_reason_when_the_days_money_runs_out() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    let (refused, text) = tool(&mut agent, "c1", "conversation", json!({"title":"Docs","brief":"Write the docs."})).await;
    assert!(!refused, "{text}");
    let mut child = support::next_pipe(&r.made).await;
    // The child ends while the fix still works: its report waits behind the fix's turn.
    finish(&mut child, 0.1).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.queued.len() == 1)).await;
    finish(&mut agent, DAILY_CAP_USD + 0.5).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.last().is_some_and(|m| m.halt.as_deref() == Some("reason.goal.cap")))).await;
    assert!(r.engine.open(&id).unwrap().queued.is_empty(), "the report was dropped, not kept for later");
    assert_eq!(user_lines(&agent), 1, "and the agent was sent nothing");
    r.engine.shutdown();
}

#[tokio::test]
async fn what_the_person_queued_behind_a_dropped_report_still_goes_out() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    let (refused, text) = tool(&mut agent, "c1", "conversation", json!({"title":"Docs","brief":"Write the docs."})).await;
    assert!(!refused, "{text}");
    let mut child = support::next_pipe(&r.made).await;
    finish(&mut child, 0.1).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.queued.len() == 1)).await;
    // The person writes while the fix still works: their message waits behind Faber's report.
    r.engine.send(SendRequest { conversation_id: Some(id.clone()), ..person_asks(&r.project, "and say how it went") }).unwrap();
    until(|| r.engine.open(&id).is_some_and(|c| c.queued.len() == 2)).await;
    finish(&mut agent, DAILY_CAP_USD + 0.5).await;
    // Their own turn starts (in a process of its own), and the dropped report is not kept for later.
    let person = support::next_pipe(&r.made).await;
    until(|| brief(&person).contains("and say how it went")).await;
    assert!(r.engine.open(&id).unwrap().queued.is_empty(), "nothing is left stuck in the queue");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_child_report_the_cap_refuses_leaves_the_parent_a_reason() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    let (refused, text) = tool(&mut agent, "c1", "conversation", json!({"title":"Docs","brief":"Write the docs."})).await;
    assert!(!refused, "{text}");
    let mut child = support::next_pipe(&r.made).await;
    finish(&mut agent, DAILY_CAP_USD + 0.5).await;
    until(|| r.view().spent_usd >= DAILY_CAP_USD).await;
    let halt = || r.engine.open(&id).and_then(|c| c.messages.last().and_then(|m| m.halt.clone()));
    assert_eq!(halt(), None, "nothing said the limit yet");
    // The child ends: its report is refused, and the fix's reply says why nobody answered.
    finish(&mut child, 0.1).await;
    until(|| halt().as_deref() == Some("reason.goal.cap")).await;
    assert_eq!(user_lines(&agent), 1, "the fix's agent was sent nothing");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_fix_with_no_money_left_does_not_compact_its_big_context_first() {
    let r = rig(false).await;
    r.engine.set_keep_going_pace(0.001);
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    r.engine.keep_going(&id, true).unwrap();
    // The turn ends with the day's limit spent and a context of seven tenths of its window: a compaction is a paid turn too.
    agent.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    let big = json!({"type":"assistant","parent_tool_use_id":null,"message":{"role":"assistant","content":[{"type":"text","text":"ok"}],
        "usage":{"input_tokens":2,"cache_read_input_tokens":700_000,"cache_creation_input_tokens":0,"output_tokens":4}}});
    agent.say(big).await;
    let ended = json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":DAILY_CAP_USD + 0.5,"usage":{},
        "modelUsage":{"claude-haiku-5-5":{"inputTokens":2,"cacheReadInputTokens":700_000,"contextWindow":1_000_000}}});
    agent.say(ended).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.last().is_some_and(|m| m.halt.as_deref() == Some("reason.goal.cap")))).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(user_lines(&agent), 1, "no compaction and no turn went to the agent");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_fix_turn_with_tokens_but_no_known_price_counts_as_the_whole_days_money() {
    let r = rig(false).await;
    let mut agent = break_it(&r).await;
    agent.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-mystery-9"})).await;
    agent.say(json!({"type":"result","subtype":"success","is_error":false,"usage":{"input_tokens":1000,"output_tokens":500}})).await;
    until(|| r.view().spent_usd >= DAILY_CAP_USD).await;
    r.engine.shutdown();
}

#[tokio::test]
async fn a_report_waiting_for_quota_does_not_resume_once_the_days_money_is_gone() {
    let r = rig(false).await;
    // Claude's plan is known not to fall back to paid use, so a window that is used up waits instead of ending a turn.
    let plan = Signals { billing: Some(faber_engine::budget::Billing::Subscription), paid_fallback: Some(false), ..Default::default() };
    r.engine.note_account_events(Vendor::Claude, vec![EngineEvent::Signals(plan)]);
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    let (refused, text) = tool(&mut agent, "c1", "conversation", json!({"title":"Docs","brief":"Write the docs."})).await;
    assert!(!refused, "{text}");
    let mut child = support::next_pipe(&r.made).await;
    finish(&mut agent, 0.2).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.last().is_some_and(|m| m.status == Some(faber_engine::model::Status::Done))))
        .await;
    // Claude has no quota for ten minutes: when the child ends, its report to the fix waits for it.
    let wait = |used: f64| {
        let reset = real_secs() + r.skew.load(Ordering::SeqCst) as f64 + 600.0;
        r.engine.note_account_events(Vendor::Claude, vec![EngineEvent::Quota { window: "five_hour".into(), used, resets_at: reset }]);
    };
    wait(1.0);
    finish(&mut child, 0.1).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.last().is_some_and(|m| m.wait_until.is_some()))).await;
    // The quota is back, and the child goes on at the person's word and takes the day past its limit.
    wait(0.0);
    let child_id = r.engine.state().conversations.into_iter().find(|c| c.title == "Docs").unwrap().id;
    r.engine.send(SendRequest { conversation_id: Some(child_id), ..person_asks(&r.project, "go on") }).unwrap();
    let mut again = support::next_pipe(&r.made).await;
    again.say(json!({"type":"system","subtype":"init","session_id":"s3","model":"claude-haiku-5-5"})).await;
    again.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":DAILY_CAP_USD + 0.5,"usage":{}})).await;
    until(|| r.view().spent_usd >= DAILY_CAP_USD).await;
    // Its time comes: the report is not sent, and the reply says why.
    r.advance(601);
    r.engine.set_utc_offset(0);
    let m = until_reply(&r, &id).await;
    assert_eq!(m.error.as_deref(), Some("reason.goal.cap"));
    assert_eq!(user_lines(&agent), 1, "the fix's agent was sent nothing");
    r.engine.shutdown();
}

async fn until_reply(r: &Rig, id: &str) -> faber_engine::model::Message {
    for _ in 0..300 {
        if let Some(m) = r.engine.open(id).and_then(|c| c.messages.last().cloned()).filter(|m| m.error.is_some()) {
            return m;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the waiting reply never ended: {:?}", r.engine.open(id).map(|c| c.messages.last().cloned()));
}

#[tokio::test]
async fn failed_checks_of_a_fix_over_the_days_money_ask_nothing_and_the_reply_says_the_limit_is_why() {
    let r = rig(false).await;
    // The check keeps failing, so Faber's own checks of the fix's reply fail too and would ask the agent to fix them.
    let mut agent = break_it(&r).await;
    let id = r.conversations().remove(0);
    // Its copy declares the same checks as the project.
    let copy = r.engine.open(&id).unwrap().copy.unwrap().path;
    let checks = format!("## Proof commands\n\n```\n{CHECK}\n```\n");
    std::fs::write(std::path::Path::new(&copy).join("AGENTS.md"), checks).unwrap();
    // The agent edits a file in its copy during the turn, so its reply is checked.
    agent.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    let edit = json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"w1","name":"Write",
        "input":{"file_path":"src/lib.rs","content":"fn main() {}"}}]}});
    agent.say(edit).await;
    let wrote = json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"w1","content":"ok","is_error":false}]}});
    agent.say(wrote).await;
    finish(&mut agent, DAILY_CAP_USD + 0.5).await;
    until(|| r.engine.open(&id).is_some_and(|c| c.messages.last().is_some_and(|m| m.halt.as_deref() == Some("reason.goal.cap")))).await;
    let last = r.engine.open(&id).unwrap().messages.last().cloned().unwrap();
    assert_eq!(
        last.checks.map(|k| k.state),
        Some(faber_engine::model::ChecksState::NeedsYou),
        "the person decides what the failed checks mean"
    );
    assert_eq!(user_lines(&agent), 1, "and the agent was asked nothing");
    r.engine.shutdown();
}
