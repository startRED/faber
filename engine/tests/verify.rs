//! Faber's checks (R190): a project's agent calls the `verify` tool and gets the project's own checks back as short
//! failures; and on Windows, a check leaves no process behind, whether it ends, starts one of its own or runs out of time.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, ChecksState, Message, Permissions, Settings, Status, Vendor};
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
    let mut found = None;
    support::until(&format!("{request} was never answered"), || {
        found = pipe
            .lines
            .lock()
            .unwrap()
            .iter()
            .find(|m| m["response"]["request_id"] == request)
            .map(|m| m["response"]["response"]["mcp_response"].clone());
        found.is_some()
    })
    .await;
    found.unwrap()
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

fn ask(conversation_id: Option<String>, project_id: Option<String>, text: &str) -> SendRequest {
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    SendRequest { conversation_id, area: Area::Project, project_id, text: text.into(), images: vec![], files: vec![], choice }
}

/// A project conversation in full access on a hand-driven Claude process, Faber's checks run by `runner`, on a known
/// subscription (Faber sends a turn by itself only on one).
async fn decided<R: Runner + 'static>(dir: &support::DataFolder, runner: Arc<R>) -> (Engine, support::Pipe, String) {
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
    (engine, pipe, id)
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
    support::until(&format!("the CLI never got {n} turns"), || turns(pipe).len() >= n).await;
}

async fn until(engine: &Engine, id: &str, done: impl Fn(&[Message]) -> bool) -> Vec<Message> {
    let start = std::time::Instant::now();
    loop {
        let messages = engine.open(id).unwrap().messages;
        if done(&messages) {
            return messages;
        }
        let seen = |m: &Message| (m.role, m.wake.as_ref().map(|w| w.why.clone()), m.status, state(m));
        assert!(
            start.elapsed() < support::DEADLINE,
            "the conversation never got there: {:?}",
            messages.iter().map(seen).collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn state(m: &Message) -> Option<ChecksState> {
    m.checks.as_ref().map(|c| c.state)
}

#[tokio::test]
async fn verifying_is_explicit_and_edited_replies_drain_the_queue_without_checks_or_fix_wakes() {
    let dir = support::DataFolder::new();
    let runner = Arc::new(Fake::default());
    let (engine, mut pipe, id) = decided(&dir, runner.clone()).await;
    engine.send(ask(Some(id.clone()), None, "Read the footer.")).unwrap();
    replies(&mut pipe, 1, true).await;
    until_turns(&pipe, 2).await;
    let content = pipe.lines.lock().unwrap().iter().filter(|m| m["type"] == "user").nth(1).unwrap()["message"]["content"].clone();
    pipe.say(json!({"type":"user","message":{"role":"user","content":content},"isReplay":true})).await;
    support::until("the queued message was not sent", || engine.open(&id).unwrap().queued.is_empty()).await;
    assert!(turns(&pipe)[1].contains("Read the footer."));
    replies(&mut pipe, 2, false).await;
    let messages = until(&engine, &id, |m| m.len() == 4 && m[3].status == Some(Status::Done)).await;
    assert!(messages.iter().all(|m| m.checks.is_none() && m.wake.is_none()), "no automatic check or fix row");
    assert!(runner.0.lock().unwrap().is_empty(), "a finished reply never runs checks by itself");
    assert_eq!(turns(&pipe).len(), 2, "only the person's messages were sent");
    engine.shutdown();
}

#[tokio::test]
async fn verifying_does_not_run_after_a_command_changes_files_either() {
    let dir = support::DataFolder::new();
    let runner = Arc::new(Fake::default());
    let (engine, mut pipe, id) = decided(&dir, runner.clone()).await;
    std::fs::write(dir.path().join("project/changed.txt"), "changed by a command").unwrap();
    replies(&mut pipe, 1, false).await;
    let messages = until(&engine, &id, |m| m.len() == 2 && m[1].status == Some(Status::Done)).await;
    assert!(messages[1].checks.is_none());
    assert!(runner.0.lock().unwrap().is_empty());
    engine.shutdown();
}

/// Section 5 of the spec, the parts a hidden test can prove: whatever a check started is gone when the verify returns, and a
/// check past its deadline is ended with everything it started. A process still holding its file would make the delete fail.
#[cfg(windows)]
mod processes {
    use super::*;
    use faber_engine::verify::{Level, Request, Verdict};

    /// A check that runs `script` with Node, from a file: a check's command is plain words, with no quotes.
    fn node(folder: &Path, script: &str) -> String {
        std::fs::write(folder.join("check.cjs"), script).unwrap();
        "node check.cjs".into()
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
        let held = dir.path().join("hold.txt");
        support::until("the check that ran out of time was still running", || std::fs::remove_file(&held).is_ok()).await;
    }
}
