//! A hard kill preserves the saved reply and Continue resumes its native project session.

use super::{Result, Scenario, ask, ensure, eventually, same, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::time::Duration;

const NAME: &str = "projects recovery: a hard kill preserves the answer and Continue resumes the native session";
const ID: &str = "5f1d0000-0000-4000-8000-000000000320";
const SESSION: &str = "5f1c0000-0000-4000-8000-000000000320";
const ASK: &str = "Write the recovery report.";
const NEXT: &str = "Write the next part.";
const SAVED: &str = "The first part of the recovery report is saved.";
const PARTIAL: &str = "The next part is still being written.";
const RESUMED: &str = "The recovery report continued in the same native session.";
const CONTINUE: &str = "Faber was closed while your last answer was running, which cut it short. Before you go on, look at the \
files and the work as they are now: some steps may have finished and others not. Then continue from where you stopped; do not \
start over.";

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: NAME, run: killed }]
}

fn recording(ctx: &Ctx, request: &str, reply: &str, finish: bool) -> Result {
    let mut lines = vec![
        json!({"dir":"in", "msg":{"type":"user", "message":{"role":"user", "content":request}}}),
        json!({"dir":"out", "msg":{"type":"system", "subtype":"init", "session_id":SESSION, "model":"claude-haiku-5-5"}}),
        json!({"dir":"out", "msg":{"type":"stream_event", "event":{"type":"content_block_start", "index":0,
            "content_block":{"type":"text", "text":""}}, "session_id":SESSION, "parent_tool_use_id":null}}),
        json!({"dir":"out", "msg":{"type":"stream_event", "event":{"type":"content_block_delta", "index":0,
            "delta":{"type":"text_delta", "text":reply}}, "session_id":SESSION, "parent_tool_use_id":null}}),
        json!({"dir":"out", "msg":{"type":"result", "subtype":"success", "is_error":false, "session_id":SESSION,
            "total_cost_usd":0, "usage":{"input_tokens":10, "output_tokens":10}}}),
    ];
    if !finish {
        // The fake stays mid-answer until the app dies; no result can race the kill.
        lines.extend([
            json!({"dir":"in", "msg":{"type":"user", "message":{"role":"user", "content":NEXT}}}),
            json!({"dir":"out", "msg":{"type":"stream_event", "event":{"type":"content_block_start", "index":0,
                "content_block":{"type":"text", "text":""}}, "session_id":SESSION, "parent_tool_use_id":null}}),
            json!({"dir":"out", "msg":{"type":"stream_event", "event":{"type":"content_block_delta", "index":0,
                "delta":{"type":"text_delta", "text":PARTIAL}}, "session_id":SESSION, "parent_tool_use_id":null}}),
            json!({"dir":"out", "msg":{"faber_hold":true}}),
        ]);
    }
    let text = lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    std::fs::write(ctx.sb.sandbox.join("recovery.jsonl"), text).map_err(|e| e.to_string())
}

fn killed(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let settings = ctx.sb.data.join("settings.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&settings).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // Exercise the person's Continue action, independently of automatic restart recovery.
    value["resumeInterrupted"] = json!(false);
    ctx.write_settings(&value)?;
    super::shell::seed_project(ctx, "Recovery project")?;
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join(format!("{ID}.json"));
    std::fs::write(
        &file,
        json!({"id":ID, "title":"Recovery report", "area":"project", "projectId":"demo",
        "sessions":{}, "createdAt":1, "updatedAt":1, "messages":[]})
        .to_string(),
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(ctx.sb.state.join("claude.signed-in"), "true").map_err(|e| e.to_string())?;
    let setup = ctx.sb.sandbox.join("recovery.setup.json");
    std::fs::write(&setup, json!({"vendor":"claude", "version":"2.1.0", "chat":"recovery.jsonl"}).to_string())
        .map_err(|e| e.to_string())?;
    recording(ctx, ASK, SAVED, false)?;
    let launch = Launch { claude: CliSlot::Fake, recording: Some(setup), ..Launch::default() };
    let mut app = ctx.launch(launch.clone())?;
    let projects = app.page.named("button", "area.projects")?;
    app.page.click(&projects)?;
    app.page.click(&css(".prow .item").has_text("Recovery project"))?;
    app.page.click(&css(".proj-chats .item").has_text("Recovery report"))?;
    super::select_model(&mut app.page, "Claude Haiku")?;
    ask(&mut app.page, ASK)?;
    app.page.wait_visible(&css("article.ai").has_text(SAVED).css(".actions"))?;
    ask(&mut app.page, NEXT)?;
    app.page.wait_visible(&css("article.ai").has_text(PARTIAL))?;
    let mut saved = Value::Null;
    eventually("the completed answer, running turn and native session to be saved before the kill", Duration::from_secs(10), || {
        saved = std::fs::read(&file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or(Value::Null);
        saved["sessions"]["claude"] == SESSION
            && saved["messages"].as_array().is_some_and(|messages| {
                messages.iter().any(|m| m["text"] == SAVED && m["status"] == "done")
                    && messages.last().is_some_and(|m| m["status"] == "streaming")
            })
    })?;
    let turn = saved["messages"].as_array().and_then(|m| m.last()).ok_or("no saved running turn")?["id"].clone();
    let answer = saved["messages"][1]["id"].clone();
    ctx.shot(&mut app, "recovery-before-kill")?;
    let first_pid = app.pid;
    // Ctx::close uses the gate's hard kill, never the window's graceful close command.
    ctx.close(app.id)?;
    eventually("the killed app process to exit", Duration::from_secs(10), || ctx.exited(app.id).is_some())?;
    ctx.audit_within(Duration::from_secs(15))?;
    let previous_commands = ctx.records()?.len();

    recording(ctx, CONTINUE, RESUMED, true)?;
    let mut app = ctx.launch(launch)?;
    ensure(app.pid != first_pid, || "recovery must launch a new native app process".into())?;
    app.page.wait_visible(&css("article.ai").has_text(SAVED))?;
    let interrupted = app.page.named("button", "answer.closedContinue")?;
    let closed = app.page.text_of("answer.closed")?;
    app.page.wait_visible(&css("article.ai .faber-act").has_text(&closed))?;
    app.page.wait_visible(&interrupted)?;
    ctx.shot(&mut app, "recovery-after-kill")?;
    app.page.click(&interrupted)?;
    app.page.wait_visible(&css("article.ai").has_text(RESUMED).css(".actions"))?;
    app.page.wait_visible(&css("article.ai").has_text(SAVED))?;
    let records = ctx.records()?;
    let resumed: Vec<_> = records[previous_commands..]
        .iter()
        .filter(|r| r.args.iter().any(|a| a == "-p") && !r.args.iter().any(|a| a == "--no-session-persistence"))
        .collect();
    same(resumed.len(), 1, "Continue starts exactly one native turn")?;
    ensure(resumed[0].args.windows(2).any(|a| a == ["--resume", SESSION]), || {
        format!("Continue did not resume the saved native session: {:?}", resumed[0].args)
    })?;
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let messages = saved["messages"].as_array().ok_or("no recovered messages")?;
    same(&saved["sessions"]["claude"], &json!(SESSION), "the native session remains saved after Continue")?;
    ensure(messages.iter().any(|m| m["id"] == answer && m["text"] == SAVED && m["status"] == "done"), || {
        "Continue lost or replaced the saved answer".into()
    })?;
    ensure(messages.iter().any(|m| m["id"] == turn && m["closed"] == true && m["status"] == "stopped"), || {
        "Continue lost or replaced the interrupted turn".into()
    })?;
    same(messages.iter().filter(|m| m["role"] == "user" && m["text"] == ASK).count(), 1, "the person's request is not replayed")?;
    same(messages.iter().filter(|m| m["role"] == "user" && m["text"] == NEXT).count(), 1, "the interrupted request is not replayed")?;
    same(messages.iter().filter(|m| m["wake"]["why"] == "wake.closed").count(), 1, "Continue sends one recovery request")?;
    ctx.shot(&mut app, "recovery-continued")?;
    Ok(())
}
