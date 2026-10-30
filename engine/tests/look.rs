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

fn engine(dir: &crate::support::DataFolder, spawn: faber_engine::proc::Spawner) -> Engine {
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
    Engine::new(opts, Arc::new(|_: AppEvent| {})).unwrap()
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
    let mut found = None;
    support::until(what, || {
        found = engine.open(id).and_then(|c| c.messages.last().cloned()).filter(|m| done(m));
        found.is_some()
    })
    .await;
    found.unwrap()
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
    let dir = crate::support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    let id = send(&engine, "fix the date parsing");
    let folder = dir.scratch_path().join(&id);
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
    let dir = crate::support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    let id = send(&engine, "fix the spelling in the intro");
    let folder = dir.scratch_path().join(&id);
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

// Step 4: the local count (`look.jsonl`). The lines are written on a blocking thread, so the tests wait for them.

async fn lines(dir: &Path, want: usize) -> Vec<Value> {
    let mut found = vec![];
    support::until(&format!("fewer than {want} lines in look.jsonl"), || {
        let text = std::fs::read_to_string(dir.join("look.jsonl")).unwrap_or_default();
        found = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        found.len() >= want
    })
    .await;
    found
}

fn event(e: &str, sign: &str, area: &str) -> Value {
    json!({ "event": e, "sign": sign, "area": area })
}

/// Without the time, which changes, a line is the event, the sign and the area, and nothing of the person's work.
fn plain(lines: Vec<Value>) -> Vec<Value> {
    lines
        .into_iter()
        .map(|mut l| {
            assert!(l["at"].as_u64().is_some_and(|at| at > 0), "a time is kept: {l}");
            l.as_object_mut().unwrap().remove("at");
            l
        })
        .collect()
}

async fn red_to_green(pipe: &mut support::Pipe) {
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(step("t1", "Bash", json!({ "command": "cargo test" }))).await;
    pipe.say(result("t1", false)).await;
    pipe.say(step("e1", "Edit", json!({ "file_path": "src/a.rs", "old_string": BAD, "new_string": "let d = parse(s)?;" }))).await;
    pipe.say(result("e1", true)).await;
    pipe.say(step("t2", "Bash", json!({ "command": "cargo test" }))).await;
    pipe.say(result("t2", true)).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
}

fn request(id: &str, text: &str) -> SendRequest {
    let choice = Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None };
    SendRequest {
        conversation_id: Some(id.into()),
        area: Area::Chat,
        project_id: None,
        text: text.into(),
        images: vec![],
        files: vec![],
        choice,
    }
}

const ASK: &str = "Look for the same kind of mistake you just fixed in the other places it could be.";

/// An offer is one line when the reply ends (and not again when the read adds its places), and sending the request the
/// button writes is one more, for the sign that was offered.
#[tokio::test]
async fn an_offer_and_its_use_are_counted_in_the_local_log() {
    let dir = crate::support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    let id = send(&engine, "fix the date parsing");
    let folder = dir.scratch_path().join(&id);
    write(&folder, "src/b.rs", &format!("fn b() {{\n    {BAD}\n}}\n"));
    let mut pipe = next_pipe(&made).await;
    red_to_green(&mut pipe).await;
    reply_until(&engine, &id, "the read after the reply", |m| m.look.as_ref().is_some_and(|l| !l.places.is_empty())).await;
    assert_eq!(plain(lines(dir.path(), 1).await), vec![event("offered", "look.fixed", "chat")]);

    engine.send(request(&id, ASK)).unwrap();
    let all = plain(lines(dir.path(), 2).await);
    engine.shutdown();
    assert_eq!(all, vec![event("offered", "look.fixed", "chat"), event("used", "look.fixed", "chat")]);
    assert_eq!(faber_engine::look_log::counts(dir.path()).get(&("look.fixed".into(), "chat".into())), Some(&(1, 1)));
}

/// An offer made by the read alone (no check ran) is counted when the read makes it, under its own sign.
#[tokio::test]
async fn an_offer_made_by_the_read_is_counted_under_its_own_sign() {
    let dir = crate::support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    let id = send(&engine, "fix the spelling in the intro");
    let folder = dir.scratch_path().join(&id);
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
    reply_until(&engine, &id, "the read after the reply", |m| m.look.is_some()).await;
    let all = plain(lines(dir.path(), 1).await);
    engine.shutdown();
    assert_eq!(all, vec![event("offered", "look.sameText", "chat")]);
}

/// A turn with no sign leaves no log, and the request sent after it is no use of anything.
#[tokio::test]
async fn nothing_is_written_without_a_sign() {
    let dir = crate::support::DataFolder::new();
    let (spawn, made) = pipes();
    let engine = engine(&dir, spawn);
    let id = send(&engine, "hello");
    let mut pipe = next_pipe(&made).await;
    pipe.say(json!({ "type": "system", "subtype": "init", "session_id": "s", "model": "haiku" })).await;
    pipe.say(json!({ "type": "result", "subtype": "success", "is_error": false, "total_cost_usd": 0.0, "usage": {} })).await;
    reply_until(&engine, &id, "the reply", |m| m.status == Some(Status::Done)).await;
    engine.send(request(&id, ASK)).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    engine.shutdown();
    assert!(!dir.path().join("look.jsonl").exists());
}

/// The engine knows the request by its first words, so they must stay the words of the button in every language.
#[test]
fn the_request_is_recognised_in_every_language_of_the_screen() {
    for (file, start) in ["en", "pt-BR", "es", "fr", "de"].into_iter().zip(faber_engine::look_log::REQUESTS) {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui/locales").join(format!("{file}.ts"))).unwrap();
        let ask = text.lines().find_map(|l| l.trim().strip_prefix("'look.ask': '")).unwrap_or_else(|| panic!("{file} has no look.ask"));
        assert!(ask.starts_with(start) && faber_engine::look_log::is_request(ask), "{file}: {ask}");
    }
}
