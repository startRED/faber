//! Faber's checks (R190): a project's agent calls the `verify` tool and gets the project's own checks back as short
//! failures; and on Windows, a check leaves no process behind, whether it ends, starts one of its own or runs out of time.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Permissions, Settings, Vendor};
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
            stdout: Box::new(std::io::Cursor::new(out.as_bytes().to_vec())),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async move { Some(code) }),
        })
    }
}

fn engine(dir: &Path, spawn: faber_engine::proc::Spawner) -> Engine {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let opts = EngineOptions {
        data_dir: dir.join("data"),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    Engine::new(opts, Arc::new(move |e| drop(tx.send(e))))
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
    let dir = support::canonical_tempdir().unwrap();
    let folder = dir.path().join("project");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("AGENTS.md"), AGENTS).unwrap();
    let (spawn, made) = pipes();
    let engine = engine(dir.path(), spawn);
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
    let dir = support::canonical_tempdir().unwrap();
    let (spawn, made) = pipes();
    let engine = engine(dir.path(), spawn);
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
        let dir = tempfile::tempdir().unwrap();
        // The check starts a child that holds hold.txt open for a minute, and ends at once.
        let script = "require('child_process').spawn('ping',['-n','60','127.0.0.1'],{windowsHide:true,stdio:['ignore',require('fs').openSync('hold.txt','w'),'ignore']}).unref()";
        let v = Verifier::real();
        let started = std::time::Instant::now();
        let out = v.verify(request(dir.path(), node(dir.path(), script), Level::Full)).await;
        assert!(started.elapsed() < Duration::from_secs(30), "the verify waited for the child's whole run");
        assert_eq!(out.checks[0].verdict, Verdict::Passed, "{out:?}");
        std::fs::remove_file(dir.path().join("hold.txt")).expect("the process the check left behind was still running");
        assert!(!v.busy(dir.path()));
    }

    #[tokio::test]
    async fn a_check_past_its_deadline_is_ended_with_everything_it_started() {
        if faber_engine::cli::find_cli("node").is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
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
