//! Faber's checks (R190): a project's agent calls the `verify` tool and gets the project's own checks back as short
//! failures; and on Windows, a check leaves no process behind, whether it ends, starts one of its own or runs out of time.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, ChecksState, Message, Permissions, Settings, Status, Vendor};
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::router::Choice;
use faber_engine::verify::{Runner, Spec, Started, Verifier};
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::{next_pipe, pipes};

/// A runner that starts nothing: `npm run lint` fails with one type error, everything else passes.
#[derive(Default)]
struct Fake(Mutex<Vec<Spec>>);

impl Runner for Fake {
    fn start(&self, spec: &Spec) -> std::io::Result<Started> {
        self.0.lock().unwrap().push(spec.clone());
        let (out, code) = if spec.args.last().is_some_and(|a| a == "lint") {
            ("ui/App.tsx(3,7): error TS2322: Type 'string' is not assignable to type 'number'.\n", 2)
        } else {
            ("", 0)
        };
        Ok(Started {
            confirmed: None,
            throttle: None,
            peak: None,
            stdout: Box::new(std::io::Cursor::new(out.as_bytes().to_vec())),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async move { Some(code) }),
        })
    }
}

fn engine(dir: &crate::support::DataFolder, spawn: faber_engine::proc::Spawner) -> Engine {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
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
    Engine::new(opts, Arc::new(move |e| drop(tx.send(e)))).unwrap()
}

const AGENTS: &str = "# Rules\n\n## Proof commands\n\n```bash\nnpm run lint\ncargo fmt --all --check\n```\n";

/// What Faber's MCP server answered to one control request, once it did.
async fn answer(pipe: &support::Pipe, request: &str) -> serde_json::Value {
    for _ in 0..500 {
        if let Some(m) = pipe.lines.lock().unwrap().iter().find(|m| m["response"]["request_id"] == request) {
            return m["response"]["response"]["mcp_response"].clone();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{request} was never answered");
}

fn tools(answer: &serde_json::Value) -> Vec<String> {
    answer["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect()
}

#[allow(non_snake_case)]
fn LIST(id: u64) -> serde_json::Value {
    json!({"type":"control_request","request_id":format!("l{id}"),"request":{"subtype":"mcp_message","server_name":"faber",
        "message":{"jsonrpc":"2.0","id":id,"method":"tools/list"}}})
}

#[tokio::test]
async fn the_agent_runs_the_project_s_own_checks_and_reads_what_failed() {
    let dir = support::DataFolder::new();
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("AGENTS.md"), AGENTS).unwrap();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    let runner = Arc::new(Fake::default());
    engine.set_verifier(Arc::new(Verifier::new(runner.clone())));
    let project = engine.add_project(&folder.to_string_lossy()).unwrap();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "check it".into(),
            images: vec![],
            files: vec![],
            choice,
        })
        .unwrap();
    let mut agent = next_pipe(&made).await;
    agent.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    agent.say(LIST(1)).await;
    assert!(tools(&answer(&agent, "l1").await).contains(&"verify".to_string()), "a project's agent in full access gets the tool");

    agent.say(json!({"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","id":"toolu_v","name":"mcp__faber__verify","input":{}}]}})).await;
    agent
        .say(json!({"type":"control_request","request_id":"r2","request":{"subtype":"mcp_message","server_name":"faber",
            "message":{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"verify","arguments":{"level":"full"},"_meta":{"claudecode/toolUseId":"toolu_v"}}}}}))
        .await;
    let result = answer(&agent, "r2").await;
    let text = result["result"]["content"][0]["text"].as_str().unwrap().to_string();
    assert_eq!(result["result"]["isError"], true, "a failed check reads as a failed step: {text}");
    assert!(text.contains("1 failed, 1 passed") && text.contains("FAILED npm run lint"), "{text}");
    assert!(text.contains("ui/App.tsx:3:7: error TS2322"), "the short failure, with its place: {text}");
    let started = runner.0.lock().unwrap().clone();
    assert_eq!(started.iter().map(|s| s.program.as_str()).collect::<Vec<_>>(), ["npm", "cargo"]);
    let folder = folder.canonicalize().unwrap();
    assert!(started.iter().all(|s| s.cwd.canonicalize().unwrap() == folder), "the checks run in the conversation's folder");
    // The step is one row of the reply, named Faber's way.
    let c = engine.state().conversations.into_iter().next().unwrap();
    let reply = engine.open(&c.id).unwrap().messages.last().cloned().unwrap();
    assert!(reply.tools.unwrap().iter().any(|t| t.name == faber_engine::verify::TOOL));
    engine.shutdown();
}

#[tokio::test]
async fn the_checks_are_offered_only_where_the_agent_already_runs_commands_without_asking() {
    let dir = support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    engine.set_verifier(Arc::new(Verifier::new(Arc::new(Fake::default()))));
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Project,
            project_id: Some(project.id),
            text: "check it".into(),
            images: vec![],
            files: vec![],
            choice,
        })
        .unwrap();
    let mut agent = next_pipe(&made).await;
    agent.say(LIST(1)).await;
    let listed = answer(&agent, "l1").await;
    assert!(listed["result"]["tools"].as_array().is_none_or(|t| t.iter().all(|t| t["name"] != "verify")), "{listed}");
    engine.shutdown();
}

/// "Faber decides done" (R146): a runner whose `npm run lint` fails the first `fails` times, everything else passing.
struct Flaky {
    fails: std::sync::atomic::AtomicUsize,
    started: Mutex<Vec<String>>,
}

impl Flaky {
    fn new(fails: usize) -> Arc<Flaky> {
        Arc::new(Flaky { fails: fails.into(), started: Mutex::new(vec![]) })
    }
}

impl Runner for Flaky {
    fn start(&self, spec: &Spec) -> std::io::Result<Started> {
        let line = std::iter::once(spec.program.clone()).chain(spec.args.iter().cloned()).collect::<Vec<_>>().join(" ");
        self.started.lock().unwrap().push(line);
        use std::sync::atomic::Ordering::SeqCst;
        let lint = spec.args.last().is_some_and(|a| a == "lint");
        let failing = lint && self.fails.try_update(SeqCst, SeqCst, |n| n.checked_sub(1)).is_ok();
        let (out, code) =
            if failing { ("ui/App.tsx(3,7): error TS2322: Type 'string' is not assignable to type 'number'.\n", 2) } else { ("", 0) };
        Ok(Started {
            confirmed: None,
            throttle: None,
            peak: None,
            stdout: Box::new(std::io::Cursor::new(out.as_bytes().to_vec())),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async move { Some(code) }),
        })
    }
}

/// A runner whose `npm run lint` ends only once the test opens the gate, failing when `fails`; everything else passes.
struct Gated {
    gate: Arc<tokio::sync::Semaphore>,
    fails: bool,
}

impl Gated {
    fn new(fails: bool) -> Arc<Gated> {
        Arc::new(Gated { gate: Arc::new(tokio::sync::Semaphore::new(0)), fails })
    }

    fn open(&self) {
        self.gate.add_permits(100);
    }
}

impl Runner for Gated {
    fn start(&self, spec: &Spec) -> std::io::Result<Started> {
        let lint = spec.args.last().is_some_and(|a| a == "lint");
        let out =
            if lint && self.fails { "ui/App.tsx(3,7): error TS2322: Type 'string' is not assignable to type 'number'.\n" } else { "" };
        let code = if lint && self.fails { 2 } else { 0 };
        let gate = self.gate.clone();
        Ok(Started {
            confirmed: None,
            throttle: None,
            peak: None,
            stdout: Box::new(std::io::Cursor::new(out.as_bytes().to_vec())),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async move {
                if lint {
                    drop(gate.acquire().await);
                }
                Some(code)
            }),
        })
    }
}

type Made = Arc<Mutex<Vec<support::Pipe>>>;

fn ask(conversation_id: Option<String>, project_id: Option<String>, text: &str) -> SendRequest {
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    SendRequest { conversation_id, area: Area::Project, project_id, text: text.into(), images: vec![], files: vec![], choice }
}

/// A project conversation in full access on a hand-driven Claude process, Faber's checks run by `runner`, on a known
/// subscription (Faber sends a turn by itself only on one).
async fn decided<R: Runner + 'static>(dir: &support::DataFolder, runner: Arc<R>) -> (Engine, support::Pipe, String) {
    let (engine, pipe, id, _, _) = decided_in(dir, runner).await;
    (engine, pipe, id)
}

/// The same, with what starts the next process and the project's id, for a second conversation of the project.
async fn decided_in<R: Runner + 'static>(dir: &support::DataFolder, runner: Arc<R>) -> (Engine, support::Pipe, String, Made, String) {
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("AGENTS.md"), AGENTS).unwrap();
    let (spawn, made) = pipes();
    let engine = engine(dir, spawn);
    let plan = faber_engine::signals::Signals { billing: Some(faber_engine::budget::Billing::Subscription), ..Default::default() };
    engine.note_account_events(Vendor::Claude, vec![faber_engine::events::EngineEvent::Signals(plan)]);
    engine.set_verifier(Arc::new(Verifier::new(runner)));
    let project = engine.add_project(&folder.to_string_lossy()).unwrap();
    let id = engine.send(ask(None, Some(project.id.clone()), "fix the page")).unwrap();
    let mut pipe = next_pipe(&made).await;
    until_turns(&pipe, 1).await;
    pipe.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    (engine, pipe, id, made, project.id)
}

/// One turn of the agent: an edit when `edits`, then its answer.
async fn replies(pipe: &mut support::Pipe, n: usize, edits: bool) {
    replies_with(pipe, n, edits, false, 0.1).await;
}

/// One turn of the agent: an edit when `edits`, a question to the person when `asks`, then its answer, with the week's
/// quota used as `week` says.
async fn replies_with(pipe: &mut support::Pipe, n: usize, edits: bool, asks: bool, week: f64) {
    if asks {
        let id = format!("ask{n}");
        pipe.say(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":id,"name":"AskUserQuestion",
            "input":{"questions":[{"question":"Which page?","header":"Page","options":[{"label":"A","description":"a"},{"label":"B","description":"b"}],"multiSelect":false}]}}]}}))
            .await;
        pipe.say(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"A","is_error":false}]}}))
            .await;
    }
    if edits {
        let id = format!("edit{n}");
        pipe.say(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":id,"name":"Edit",
            "input":{"file_path":"ui/App.tsx","old_string":"a","new_string":"b"}}]}}))
            .await;
        pipe.say(json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":id,"content":"ok","is_error":false}]}}))
            .await;
    }
    pipe.say(json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done."}]}})).await;
    // A quota reading with room to spare: Faber sends a turn by itself only when the pace allows it.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let windows = json!({ "five_hour": { "utilization": 0.05, "resetsAt": now + 4 * 3600 },
        "seven_day": { "utilization": week, "resetsAt": now + 6 * 86_400 } });
    pipe.say(json!({ "type": "rate_limit_event", "rate_limit_info": { "status": "allowed", "resetsAt": now + 4 * 3600,
        "rateLimitType": "five_hour", "unifiedWindows": windows } }))
        .await;
    pipe.say(json!({"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0,"usage":{}})).await;
}

/// The turns the CLI was given so far, as the text of each user line.
fn turns(pipe: &support::Pipe) -> Vec<String> {
    let text = |c: &serde_json::Value| match c {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(parts) => parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(""),
        _ => String::new(),
    };
    pipe.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").map(|m| text(&m["message"]["content"])).collect()
}

async fn until_turns(pipe: &support::Pipe, n: usize) {
    for _ in 0..300 {
        if turns(pipe).len() >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the CLI got {} turns, not {n}: {:?}", turns(pipe).len(), turns(pipe));
}

async fn until(engine: &Engine, id: &str, done: impl Fn(&[Message]) -> bool) -> Vec<Message> {
    for _ in 0..300 {
        let messages = engine.open(id).unwrap().messages;
        if done(&messages) {
            return messages;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the conversation never got there: {:?}", engine.open(id).unwrap().messages);
}

fn state(m: &Message) -> Option<ChecksState> {
    m.checks.as_ref().map(|c| c.state)
}

#[tokio::test]
async fn a_reply_that_changed_files_is_done_once_the_checks_pass_and_a_failure_asks_the_agent_to_fix_it() {
    let dir = support::DataFolder::new();
    let runner = Flaky::new(1);
    let (engine, mut pipe, id) = decided(&dir, runner.clone()).await;
    replies(&mut pipe, 1, true).await;
    // The checks failed: the reply says which, and Faber sends one turn by itself, drawn as its quiet line.
    until_turns(&pipe, 2).await;
    assert!(turns(&pipe)[1].starts_with(faber_engine::engine::FIX) && turns(&pipe)[1].contains("ui/App.tsx:3:7"), "{:?}", turns(&pipe));
    let messages = until(&engine, &id, |m| m.len() == 4).await;
    let row = messages[1].checks.clone().unwrap();
    assert_eq!((row.state, row.passed), (ChecksState::Failed, 1));
    assert_eq!(row.failed[0].command, "npm run lint");
    assert!(row.failed[0].lines[0].contains("error TS2322"), "{row:?}");
    let wake = messages[2].wake.clone().unwrap();
    assert_eq!((wake.why.as_str(), wake.checks), ("wake.checksFixing", vec!["npm run lint".to_string()]));
    // The fix passes: the reply is done, and nothing more is sent. The reply to a fix is checked even when no step of it
    // reads as a file change (a command can change files too).
    replies(&mut pipe, 2, false).await;
    let messages = until(&engine, &id, |m| m.len() == 4 && state(&m[3]) == Some(ChecksState::Passed)).await;
    assert_eq!(messages[3].checks.as_ref().unwrap().passed, 2);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 2);
    engine.shutdown();
}

#[tokio::test]
async fn checks_that_still_fail_after_two_fixes_need_the_person_and_a_reply_that_changed_nothing_is_not_checked() {
    let dir = support::DataFolder::new();
    let runner = Flaky::new(usize::MAX);
    let (engine, mut pipe, id) = decided(&dir, runner.clone()).await;
    engine.keep_going(&id, true).unwrap();
    for n in 1..=3 {
        replies(&mut pipe, n, true).await;
        if n < 3 {
            until_turns(&pipe, n + 1).await;
        }
    }
    let messages = until(&engine, &id, |m| m.len() == 6 && state(&m[5]).is_some_and(|s| s != ChecksState::Running)).await;
    assert_eq!(messages.iter().filter_map(state).collect::<Vec<_>>(), [ChecksState::Failed, ChecksState::Failed, ChecksState::NeedsYou]);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 3, "no third fix");
    assert_eq!(messages[5].halt.as_deref(), Some("reason.keepGoing.checks"), "keep going says it stopped for the checks");
    // The person writes; a reply with no file change ends as it is.
    let ran = runner.started.lock().unwrap().len();
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let request = SendRequest {
        conversation_id: Some(id.clone()),
        area: Area::Project,
        project_id: None,
        text: "what failed?".into(),
        images: vec![],
        files: vec![],
        choice,
    };
    engine.send(request).unwrap();
    until_turns(&pipe, 4).await;
    replies(&mut pipe, 4, false).await;
    let messages = until(&engine, &id, |m| m.len() == 8 && m[7].status == Some(Status::Done)).await;
    assert_eq!(messages[7].checks, None);
    assert_eq!(runner.started.lock().unwrap().len(), ran, "nothing ran");
    engine.shutdown();
}

/// Git as the verifier asks it: a repository at `top` with one commit and the new files `untracked` lists, which a test
/// changes the way a command would. It never makes a copy (no `--show-prefix`), so the conversation works in the folder.
#[derive(Clone)]
struct Git {
    top: String,
    untracked: Arc<Mutex<Vec<String>>>,
    listed: Arc<std::sync::atomic::AtomicUsize>,
    rewind_listed: Arc<Mutex<Vec<Invocation>>>,
}

impl Executor for Git {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        let args: Vec<&str> = call.args.iter().map(String::as_str).skip_while(|a| *a == "-c" || a.contains('=')).collect();
        let say = |s: String| Ok(Output { success: true, stdout: s.into_bytes() });
        match args.as_slice() {
            ["rev-parse", "--show-toplevel"] => say(self.top.clone()),
            ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"] => say("a".repeat(40)),
            ["merge-base", ..] => say("b".repeat(40)),
            ["diff", ..] => say(String::new()),
            ["ls-files", "--others", "--exclude-standard", "-z", "--full-name"] => {
                let names = self.untracked.lock().unwrap().join("\0");
                self.listed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                say(names)
            }
            ["ls-files", ..] if call.args.iter().any(|arg| arg.starts_with("--git-dir=")) => {
                self.rewind_listed.lock().unwrap().push(call.clone());
                say(if args.contains(&"--ignored") { String::new() } else { self.untracked.lock().unwrap().join("\0") })
            }
            // Exercise rewind's private-history fallback, separately from the verifier's listing above.
            ["ls-files", ..] => Ok(Output { success: false, stdout: vec![] }),
            _ => Ok(Output { success: false, stdout: vec![] }),
        }
    }
}

#[tokio::test]
async fn a_reply_that_changes_files_only_by_a_command_gets_its_checks_and_one_that_changes_nothing_does_not() {
    let dir = support::DataFolder::new();
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("AGENTS.md"), AGENTS).unwrap();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    let git = Git {
        top: folder.to_string_lossy().into_owned(),
        untracked: Arc::default(),
        listed: Arc::default(),
        rewind_listed: Arc::default(),
    };
    let runner = git.clone();
    engine.set_git(Arc::new(move || Some(Box::new(runner.clone()) as Box<dyn Executor + Send>)));
    let checks = Flaky::new(0);
    engine.set_verifier(Arc::new(Verifier::new(checks.clone())));
    let project = engine.add_project(&folder.to_string_lossy()).unwrap();
    let id = engine.send(ask(None, Some(project.id.clone()), "format the page")).unwrap();
    let mut pipe = next_pipe(&made).await;
    until_turns(&pipe, 1).await;
    pipe.say(json!({"type":"system","subtype":"init","session_id":"s1","model":"claude-haiku-5-5"})).await;
    // Git has told what the folder held when the reply began; then a command (`cargo fmt`, `sed -i`) writes a file.
    let read = || git.listed.load(std::sync::atomic::Ordering::SeqCst);
    for _ in 0..300 {
        if read() > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(read(), 1, "the reply's start asked git once");
    {
        let captures = git.rewind_listed.lock().unwrap();
        assert_eq!(captures.len(), 2, "rewind separately listed saved and ignored files");
        let history = dir.path().join("rewind").canonicalize().unwrap();
        assert!(
            captures.iter().all(|call| call
                .args
                .iter()
                .filter_map(|arg| arg.strip_prefix("--git-dir="))
                .any(|path| { Path::new(path).canonicalize().is_ok_and(|path| path.parent() == Some(history.as_path())) })),
            "rewind listed through its private git directory"
        );
    }
    std::fs::write(folder.join("ui.rs"), "fn main() {}\n").unwrap();
    git.untracked.lock().unwrap().push("ui.rs".into());
    replies(&mut pipe, 1, false).await;
    let messages = until(&engine, &id, |m| m.len() == 2 && state(&m[1]) == Some(ChecksState::Passed)).await;
    assert_eq!(messages[1].checks.as_ref().unwrap().passed, 2);
    let ran = checks.started.lock().unwrap().len();
    assert_eq!(ran, 2, "{:?}", checks.started.lock().unwrap());

    // The next reply leaves the folder as it was: no row, and nothing runs.
    engine.send(ask(Some(id.clone()), None, "anything else?")).unwrap();
    until_turns(&pipe, 2).await;
    replies(&mut pipe, 2, false).await;
    let messages = until(&engine, &id, |m| m.len() == 4 && m[3].status == Some(Status::Done)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(engine.open(&id).unwrap().messages[3].checks, None, "{messages:?}");
    assert_eq!(checks.started.lock().unwrap().len(), ran, "nothing ran");

    // A third reply rewrites the same file by a command: weighed against the folder as the reply before left it, its new
    // content counts, and the reply gets its row.
    engine.send(ask(Some(id.clone()), None, "format it again")).unwrap();
    until_turns(&pipe, 3).await;
    std::fs::write(folder.join("ui.rs"), "fn main() {\n}\n").unwrap();
    replies(&mut pipe, 3, false).await;
    until(&engine, &id, |m| m.len() == 6 && state(&m[5]) == Some(ChecksState::Passed)).await;
    engine.shutdown();
}

#[tokio::test]
async fn another_conversation_of_the_same_folder_never_stops_these_checks() {
    let dir = support::DataFolder::new();
    let runner = Gated::new(false);
    let (engine, mut pipe, id, made, project) = decided_in(&dir, runner.clone()).await;
    replies(&mut pipe, 1, true).await;
    until(&engine, &id, |m| m.get(1).and_then(state) == Some(ChecksState::Running)).await;
    // The person leaves a message while the checks run; it goes out once they pass.
    engine.send(ask(Some(id.clone()), None, "then the footer")).unwrap();
    // A second conversation of the project, in the same folder, ends a turn of its own while those checks run.
    let other = engine.send(ask(None, Some(project), "look around")).unwrap();
    let mut second = next_pipe(&made).await;
    until_turns(&second, 1).await;
    second.say(json!({"type":"system","subtype":"init","session_id":"s2","model":"claude-haiku-5-5"})).await;
    replies(&mut second, 1, false).await;
    until(&engine, &other, |m| m.len() == 2 && m[1].status == Some(Status::Done)).await;
    runner.open();
    let messages = until(&engine, &id, |m| m.get(1).and_then(state) == Some(ChecksState::Passed)).await;
    assert_eq!(messages[1].checks.as_ref().unwrap().passed, 2);
    until_turns(&pipe, 2).await;
    assert_eq!(turns(&pipe)[1], "then the footer", "the queue went out once the checks passed");
    engine.shutdown();
}

#[tokio::test]
async fn while_the_checks_run_the_conversation_is_at_work_and_stop_ends_them_with_no_fix() {
    let dir = support::DataFolder::new();
    let runner = Gated::new(true);
    let (engine, mut pipe, id) = decided(&dir, runner.clone()).await;
    replies(&mut pipe, 1, true).await;
    until(&engine, &id, |m| m.get(1).and_then(state) == Some(ChecksState::Running)).await;
    let running = engine.state().running;
    assert_eq!(running.items.iter().map(|i| i.conversation_id.as_str()).collect::<Vec<_>>(), [id.as_str()], "it shows as at work");
    // Keep going turned on now waits for the checks; a message the person writes waits too.
    engine.keep_going(&id, true).unwrap();
    engine.send(ask(Some(id.clone()), None, "and the header")).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 1, "nothing went out while the checks run: {:?}", turns(&pipe));
    // "Send now" on a later one has no turn to hand it to: it goes first, still waiting.
    engine.send(ask(Some(id.clone()), None, "and the footer")).unwrap();
    let later = engine.open(&id).unwrap().queued[1].id.clone();
    engine.send_queued(&id, &later).unwrap();
    let order: Vec<String> = engine.open(&id).unwrap().queued.iter().map(|q| q.text.clone()).collect();
    assert_eq!(order, ["and the footer", "and the header"]);
    assert_eq!(turns(&pipe).len(), 1);
    // The person stops: the checks end, the reply keeps no row, and no fix is asked even once the check would answer.
    engine.interrupt(&id);
    runner.open();
    let messages = until(&engine, &id, |m| m.get(1).is_some_and(|m| m.checks.is_none())).await;
    assert_eq!(messages.len(), 2);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 1, "{:?}", turns(&pipe));
    let request = faber_engine::rewind::RestoreRequest {
        conversation_id: id.clone(),
        message_id: messages[0].id.clone(),
        mode: faber_engine::rewind::Mode::Conversation,
        saved_versions: vec![],
    };
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if engine.rewind_preview(request.clone()).unwrap().running.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("confirmed stopped checks must release rewind admission");
    assert_eq!(turns(&pipe).len(), 1, "confirmed stopped checks never restart work");
    engine.shutdown();
}

#[tokio::test]
async fn keep_going_turned_on_while_the_checks_run_goes_on_once_they_pass() {
    let dir = support::DataFolder::new();
    let runner = Gated::new(false);
    let (engine, mut pipe, id) = decided(&dir, runner.clone()).await;
    replies(&mut pipe, 1, true).await;
    until(&engine, &id, |m| m.get(1).and_then(state) == Some(ChecksState::Running)).await;
    engine.keep_going(&id, true).unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert_eq!(turns(&pipe).len(), 1, "keep going waits for the checks");
    runner.open();
    until_turns(&pipe, 2).await;
    let messages = until(&engine, &id, |m| m.len() == 4).await;
    assert_eq!(state(&messages[1]), Some(ChecksState::Passed));
    assert_eq!(messages[2].wake.as_ref().map(|w| w.why.as_str()), Some("wake.keepGoing"));
    engine.shutdown();
}

#[tokio::test]
async fn no_fix_is_asked_after_a_question_to_the_person() {
    {
        let (asks, week) = (true, 0.1);
        let dir = support::DataFolder::new();
        let runner = Flaky::new(usize::MAX);
        let (engine, mut pipe, id) = decided(&dir, runner.clone()).await;
        replies_with(&mut pipe, 1, true, asks, week).await;
        let messages = until(&engine, &id, |m| m.get(1).and_then(state).is_some_and(|s| s != ChecksState::Running)).await;
        assert_eq!(state(&messages[1]), Some(ChecksState::NeedsYou), "asks {asks}, week {week}");
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(turns(&pipe).len(), 1, "asks {asks}, week {week}: {:?}", turns(&pipe));
        engine.shutdown();
    }
}

/// Section 5 of the spec, the parts a hidden test can prove: whatever a check started is gone when the verify returns, and a
/// check past its deadline is ended with everything it started. A process still holding its file would make the delete fail.
#[cfg(windows)]
mod processes {
    use super::*;
    use faber_engine::verify::{Level, Request, Verdict};

    /// A check that runs `script` with Node, from a file: a check's command is plain words, with no quotes.
    fn node(folder: &Path, script: &str) -> String {
        std::fs::write(folder.join("check.js"), script).unwrap();
        "node check.js".into()
    }

    fn request(folder: &Path, command: String, level: Level) -> Request {
        Request { folder: folder.into(), level: Some(level), commands: vec![command], ..Request::default() }
    }

    #[tokio::test]
    async fn a_process_a_check_left_running_is_gone_when_the_verify_returns() {
        if faber_engine::cli::find_cli("node").is_none() {
            return;
        }
        let dir = support::DataFolder::new();
        // The check starts a child that holds hold.txt open for a minute, and ends at once.
        let script = "require('child_process').spawn('ping',['-n','60','127.0.0.1'],{windowsHide:true,stdio:['ignore',require('fs').openSync('hold.txt','w'),'ignore']}).unref()";
        let v = Verifier::real(None, None);
        let started = std::time::Instant::now();
        let out = v.verify(request(dir.path(), node(dir.path(), script), Level::Full)).await;
        assert!(started.elapsed() < Duration::from_secs(30), "the verify waited for the child's whole run");
        assert_eq!(out.checks[0].verdict, Verdict::Passed, "{out:?}");
        std::fs::remove_file(dir.path().join("hold.txt")).expect("the process the check left behind was still running");
        assert!(!v.busy(dir.path()));
    }

    #[tokio::test]
    async fn a_check_tells_the_most_memory_it_held_which_its_build_is_admitted_against() {
        if faber_engine::cli::find_cli("node").is_none() {
            return;
        }
        let dir = support::DataFolder::new();
        let spec = faber_engine::verify::Spec {
            program: "node".into(),
            args: vec!["-e".into(), "Buffer.alloc(200*1024*1024, 1)".into()],
            cwd: dir.path().to_path_buf(),
            env: vec![],
            memory: None,
        };
        let started = faber_engine::verify::Runner::start(&faber_engine::verify::Real, &spec).unwrap();
        let peak = started.peak.clone().expect("a contained check tells its peak");
        assert_eq!(started.exit.await, Some(0));
        let peak = peak.load(std::sync::atomic::Ordering::Relaxed);
        assert!(peak >= 200 << 20, "the 200 MB it touched: {peak}");
        // A check ended at its deadline (its exit dropped) tells its peak too.
        use tokio::io::AsyncReadExt;
        let spec = faber_engine::verify::Spec {
            args: vec!["-e".into(), "globalThis.b=Buffer.alloc(200*1024*1024, 1);console.log('ready');setTimeout(()=>{},60000)".into()],
            ..spec
        };
        let mut started = faber_engine::verify::Runner::start(&faber_engine::verify::Real, &spec).unwrap();
        let mut ready = [0u8; 5];
        started.stdout.read_exact(&mut ready).await.unwrap();
        let peak = started.peak.clone().unwrap();
        drop(started);
        assert!(peak.load(std::sync::atomic::Ordering::Relaxed) >= 200 << 20, "the peak of a check ended early");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn a_check_past_its_memory_cap_fails_instead_of_taking_the_memory() {
        if faber_engine::cli::find_cli("node").is_none() {
            return;
        }
        let dir = support::DataFolder::new();
        let spec = faber_engine::verify::Spec {
            program: "node".into(),
            args: vec!["-e".into(), "Buffer.alloc(600*1024*1024, 1)".into()],
            cwd: dir.path().to_path_buf(),
            env: vec![],
            memory: Some(300 << 20),
        };
        let started = faber_engine::verify::Runner::start(&faber_engine::verify::Real, &spec).unwrap();
        let peak = started.peak.clone().unwrap();
        assert_ne!(started.exit.await, Some(0), "600 MB do not fit under a 300 MB cap");
        // Windows counts the refused commit in the peak, so the failure reads as the cap reached (`at_cap`).
        assert!(peak.load(std::sync::atomic::Ordering::Relaxed) >= 300 << 20);
        let uncapped = faber_engine::verify::Spec { memory: None, ..spec };
        assert_eq!(faber_engine::verify::Runner::start(&faber_engine::verify::Real, &uncapped).unwrap().exit.await, Some(0));
    }

    #[tokio::test]
    async fn a_check_past_its_deadline_is_ended_with_everything_it_started() {
        if faber_engine::cli::find_cli("node").is_none() {
            return;
        }
        let dir = support::DataFolder::new();
        let script = "require('fs').openSync('hold.txt','w');setTimeout(()=>{},60000)";
        let v = Arc::new(Verifier::with_deadline(Arc::new(faber_engine::verify::Real), Duration::from_secs(3)));
        let out = v.verify(request(dir.path(), node(dir.path(), script), Level::Full)).await;
        assert_eq!(out.checks[0].verdict, Verdict::NotFinished, "{out:?}");
        // Windows closes an ended process's files a moment after the job reports it gone.
        let mut gone = false;
        for _ in 0..50 {
            if std::fs::remove_file(dir.path().join("hold.txt")).is_ok() {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(gone, "the check that ran out of time was still running");
    }
}
