//! Look around (R369, step 3): a reply that went from red to green is marked for the offer, and the text a change removed is
//! looked for in the rest of the conversation's folder after the reply ends, so the offer can say where the same mistake
//! still is. Driven by hand through a fake Claude Code; the folder is the chat's own, written by the test.
use crate::support;

use faber_engine::cli::Cli;
use faber_engine::model::{Area, Message, Status, Vendor};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use support::{next_pipe, pipes};

fn engine(dir: &Path, spawn: faber_engine::proc::Spawner) -> Engine {
    let opts = EngineOptions {
        data_dir: dir.into(),
        clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
        spawn,
        windows: false,
        claude_env_billed: false,
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: tokio::runtime::Handle::current(),
        ask: None,
    };
    Engine::new(opts, Arc::new(|_: AppEvent| {}))
}

fn send(engine: &Engine, text: &str) -> String {
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    let request =
        SendRequest { conversation_id: None, area: Area::Chat, project_id: None, text: text.into(), images: vec![], files: vec![], choice };
    engine.send(request).unwrap()
}

fn step(id: &str, name: &str, input: Value) -> Value {
    json!({ "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": id, "name": name, "input": input }] } })
}

fn result(id: &str, ok: bool) -> Value {
    json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": id, "content": if ok { "ok" } else { "1 test failed" }, "is_error": !ok }] } })
}

async fn reply_until(engine: &Engine, id: &str, what: &str, done: impl Fn(&Message) -> bool) -> Message {
    for _ in 0..300 {
        if let Some(m) = engine.open(id).and_then(|c| c.messages.last().cloned()).filter(|m| done(m)) {
            return m;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what}: {:?}", engine.open(id).and_then(|c| c.messages.last().cloned()));
}

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

const BAD: &str = "let d = parse(s).unwrap();";

/// Red to green in the chat's folder: the offer is marked when the reply ends, and once the read is done it names the
/// other file that still holds the line the fix removed.
#[tokio::test]
async fn r369_a_fix_is_offered_and_the_offer_names_where_the_same_line_still_is() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, made) = pipes();
    let engine = engine(dir.path(), spawn);
    let id = send(&engine, "fix the date parsing");
    let folder = dir.path().join("chat").join(&id);
    write(&folder, "src/a.rs", "fn a() {\n    let d = parse(s)?;\n}\n");
    write(&folder, "src/b.rs", &format!("fn b() {{\n    {BAD}\n}}\n"));
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(step("t1", "Bash", json!({ "command": "cargo test" }))).await;
    pipe.say(result("t1", false)).await;
    pipe.say(step("e1", "Edit", json!({ "file_path": "src/a.rs", "old_string": BAD, "new_string": "let d = parse(s)?;" }))).await;
    pipe.say(result("e1", true)).await;
    pipe.say(step("t2", "Bash", json!({ "command": "cargo test" }))).await;
    pipe.say(result("t2", true)).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
    let reply = reply_until(&engine, &id, "the read after the reply", |m| m.look.as_ref().is_some_and(|l| !l.places.is_empty())).await;
    engine.shutdown();
    let look = reply.look.unwrap();
    assert_eq!(reply.status, Some(Status::Done));
    assert_eq!((look.reason.as_str(), look.places), ("look.fixed", vec!["src/b.rs:2".to_string()]));
}

/// No check ran, so there is no red to green, but the wrong words a change removed are still in another file: the places
/// make an offer of their own. A change whose text is nowhere else offers nothing.
#[tokio::test]
async fn r369_text_a_change_removed_that_is_still_elsewhere_is_offered_on_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let (spawn, made) = pipes();
    let engine = engine(dir.path(), spawn);
    let id = send(&engine, "fix the spelling in the intro");
    let folder = dir.path().join("chat").join(&id);
    write(&folder, "intro.md", "The color of the sky\n");
    write(&folder, "notes/later.md", "# Later\n\nThe colour of the sky changes at dusk.\n");
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(step(
        "e1",
        "Edit",
        json!({ "file_path": "intro.md", "old_string": "The colour of the sky", "new_string": "The color of the sky" }),
    ))
    .await;
    pipe.say(result("e1", true)).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
    let reply = reply_until(&engine, &id, "the read after the reply", |m| m.look.is_some()).await;
    let look = reply.look.unwrap();
    assert_eq!((look.reason.as_str(), look.places), ("look.sameText", vec!["notes/later.md:3".to_string()]));

    // A second turn whose removed text is nowhere else: nothing is offered.
    engine
        .send(SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Chat,
            project_id: None,
            text: "and the title".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None },
        })
        .unwrap();
    pipe.say(step(
        "e2",
        "Edit",
        json!({ "file_path": "intro.md", "old_string": "A title nobody else wrote", "new_string": "A better title" }),
    ))
    .await;
    pipe.say(result("e2", true)).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
    let reply = reply_until(&engine, &id, "the second reply", |m| {
        m.status == Some(Status::Done) && m.tools.as_ref().is_some_and(|t| t.iter().any(|t| t.id == "e2"))
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after = engine.open(&id).unwrap().messages.into_iter().find(|m| m.id == reply.id).unwrap();
    engine.shutdown();
    assert_eq!(after.look, None);
}
