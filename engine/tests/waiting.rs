use crate::support;

use faber_engine::budget::{Billing, Reach};
use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Conversation, Message, Role, Status, Vendor};
use faber_engine::router::{Choice, Effort};
use faber_engine::signals::Signals;
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions, SendRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use support::{Fake, fake, fixture};

const WHEN: u64 = 1_791_620_000;

#[tokio::test]
async fn move_chat_to_project_other_ai_keeps_attachment_locations_in_full_handover() {
    moved_attachment_handover(false).await;
}

#[tokio::test]
async fn move_chat_to_project_other_ai_keeps_attachment_locations_in_summarized_handover() {
    moved_attachment_handover(true).await;
}

async fn moved_attachment_handover(long: bool) {
    let dir = support::DataFolder::new();
    dir.initialize_chat();
    let folder = support::work_folder();
    let mut before = seed(dir.path(), long);
    before.messages[0].attachments = Some(vec!["notes.txt".into()]);
    let attachments = dir.scratch_path().join(&before.id).join("attachments");
    std::fs::create_dir_all(&attachments).unwrap();
    std::fs::write(attachments.join("notes.txt"), "Earlier decisions").unwrap();
    Store::new(dir.path()).save_checked(&before).unwrap();
    let (spawn, made) = support::pipes();
    let engine = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: Vendor::ALL.into_iter().map(|v| (v, Cli { command: v.name().into(), prefix: vec![] })).collect(),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    engine.set_time_source(Arc::new(|| WHEN as f64));
    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    for v in Vendor::ALL {
        engine.note_account_events(
            v,
            vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), paid_fallback: Some(false), ..Default::default() })],
        );
    }
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    engine.move_to_project(&before.id, &project.id).unwrap();
    engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 172800, 1.0)]);
    engine.send(request(Some(&before.id), "claude")).unwrap();
    let blocked = reply(&engine, &before.id, Status::Error).await;
    engine.move_turn(&before.id, &blocked.id).unwrap();
    let mut agent = support::next_pipe(&made).await;
    let next = async |agent: &support::Pipe, method: &str| {
        for _ in 0..200 {
            if let Some(m) = agent.lines.lock().unwrap().iter().find(|m| m["method"] == method).cloned() {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no {method}: {:?}", agent.lines.lock().unwrap());
    };
    let init = next(&agent, "initialize").await;
    agent.say(json!({"id":init["id"],"result":{}})).await;
    if long {
        let config = next(&agent, "config/read").await;
        agent.say(json!({"id":config["id"],"result":{"config":{}}})).await;
    }
    let start = next(&agent, "thread/start").await;
    agent.say(json!({"id":start["id"],"result":{"thread":{"id":"handover"}}})).await;
    let turn = next(&agent, "turn/start").await;
    let prompt = if long {
        agent.say(json!({"id":turn["id"],"result":{"turn":{"id":"summary"}}})).await;
        agent.say(json!({"method":"item/completed","params":{"threadId":"handover","turnId":"summary","item":{"id":"summary-answer","type":"agentMessage","text":"Keep the earlier decisions."}}})).await;
        agent.say(json!({"method":"turn/completed","params":{"threadId":"handover","turn":{"id":"summary","status":"completed"}}})).await;
        let mut project_agent = support::next_pipe(&made).await;
        let init = next(&project_agent, "initialize").await;
        project_agent.say(json!({"id":init["id"],"result":{}})).await;
        let project = next(&project_agent, "thread/start").await;
        project_agent.say(json!({"id":project["id"],"result":{"thread":{"id":"project"}}})).await;
        next(&project_agent, "turn/start").await
    } else {
        turn
    };
    let location = dir.path().join("attachments").join(&before.id).join("notes.txt").to_string_lossy().into_owned();
    assert!(
        prompt["params"]["input"][0]["text"].as_str().unwrap().contains(&location),
        "the other AI must receive the attachment location, summarized={long}: {prompt}"
    );
    engine.shutdown();
}

#[tokio::test]
async fn move_chat_to_project_quota_wait_keeps_history_and_attachment_locations_for_each_cli() {
    for vendor in Vendor::ALL {
        for reopened in [false, true] {
            let dir = support::DataFolder::new();
            dir.initialize_chat();
            let folder = support::work_folder();
            let mut before: Conversation = serde_json::from_str(include_str!("../../tests/fixtures/conversation-export.json")).unwrap();
            before.working_folder = None;
            for m in &mut before.messages {
                m.attachments = None;
            }
            before.messages[0].attachments = Some(vec!["notes.txt".into()]);
            let attachments = dir.scratch_path().join(&before.id).join("attachments");
            std::fs::create_dir_all(&attachments).unwrap();
            std::fs::write(attachments.join("notes.txt"), "Earlier project decisions").unwrap();
            Store::new(dir.path()).save_checked(&before).unwrap();
            let expected = faber_engine::engine::with_history(&before.messages, &request(None, vendor.name()).text, &|name| {
                dir.path().join("attachments").join(&before.id).join(name).to_string_lossy().into_owned()
            });
            let mut entries = fixture(vendor.name(), "chat");
            for e in &mut entries {
                if e.input && e.msg["type"] == "user" {
                    e.msg["message"]["content"] = json!(expected);
                }
                if e.input && e.msg["method"] == "turn/start" {
                    e.msg["params"]["input"][0]["text"] = json!(expected);
                }
            }
            let make = || sources(&dir, &[vendor], fake(entries.clone()), fake(entries.clone()));
            let r = make();
            let project = r.engine.add_project(&folder.path().to_string_lossy()).unwrap();
            r.engine.move_to_project(&before.id, &project.id).unwrap();
            r.engine.note_account_events(vendor, vec![quota("five_hour", 600, 1.0)]);
            let id = r.engine.send(request(Some(&before.id), vendor.name())).unwrap();
            let blocked = reply(&r.engine, &id, Status::Error).await;
            assert_eq!(blocked.error.as_deref(), Some("error.quotaOut"));
            let r = if reopened {
                r.engine.shutdown();
                let back = make();
                back.engine.open(&id).unwrap();
                back.engine.note_account_events(vendor, vec![quota("five_hour", 600, 1.0)]);
                back
            } else {
                r
            };
            r.engine.wait_turn(&id, &blocked.id).unwrap();
            r.time.store(WHEN + 601, Ordering::SeqCst);
            r.engine.set_utc_offset(0);
            reply(&r.engine, &id, Status::Done).await;
            let running = if vendor == Vendor::Claude { &r.claude } else { &r.codex };
            let messages = running.wire.messages();
            let turn = messages
                .iter()
                .find_map(|m| match vendor {
                    Vendor::Claude if m["type"] == "user" => m["message"]["content"].as_str(),
                    Vendor::Codex if m["method"] == "turn/start" => m["params"]["input"][0]["text"].as_str(),
                    _ => None,
                })
                .expect("the recovered turn reaches its fresh session");
            assert_eq!(support::person_string(turn), expected, "vendor={vendor:?}, reopened={reopened}");
            assert!(running.mismatches.lock().unwrap().is_empty());
            r.engine.shutdown();
        }
    }
}

#[tokio::test]
async fn move_chat_to_project_refuses_quota_recovery_until_stopped_including_after_restart() {
    for vendor in ["claude", "auto"] {
        let dir = support::DataFolder::new();
        let folder = support::work_folder();
        let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
        let project = r.engine.add_project(&folder.path().to_string_lossy()).unwrap();
        r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 7 * 3600, 1.0)]);
        let id = r.engine.send(request(None, vendor)).unwrap();
        let blocked = reply(&r.engine, &id, Status::Error).await;
        assert_eq!(blocked.error.as_deref(), Some(if vendor == "claude" { "error.quotaOut" } else { "error.quotaWait" }));
        let before = r.engine.open(&id).unwrap();
        assert_eq!(r.engine.move_to_project(&id, &project.id), Err("error.moveChat.busy".into()), "live {vendor}");
        assert_eq!(r.engine.open(&id).unwrap(), before);
        r.engine.shutdown();
        drop(r);
        let back = rig(&dir, &[Vendor::Claude], fake(vec![]));
        back.engine.open(&id).unwrap();
        assert_eq!(back.engine.move_to_project(&id, &project.id), Err("error.moveChat.busy".into()), "reopened {vendor}");
        back.engine.interrupt(&id);
        assert_eq!(back.engine.open(&id).unwrap().messages.last().unwrap().status, Some(Status::Stopped));
        assert!(back.engine.wait_turn(&id, &blocked.id).is_err(), "stopping revokes the recovery");
        back.engine.shutdown();
        let later = rig(&dir, &[Vendor::Claude], fake(vec![]));
        assert_eq!(later.engine.move_to_project(&id, &project.id).unwrap().area, Area::Project, "Stop survives reopening");
        assert!(later.engine.approve_credits(&id, &blocked.id, Vendor::Claude).is_err());
        assert!(later.claude.args.lock().unwrap().is_empty(), "no recovery starts in the destination");
        later.engine.shutdown();
    }
}

#[tokio::test]
async fn restarting_cancels_a_queued_wait_instead_of_freezing_or_resending_it() {
    let dir = support::DataFolder::new();
    let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
    r.engine.note_account_events(Vendor::Claude, vec![quota("five_hour", 600, 1.0)]);
    let id = r.engine.send(request(None, "auto")).unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    let m = reply(&r.engine, &id, Status::Streaming).await;
    assert!(m.wait_until.is_some());
    drop(r);
    let back = rig(&dir, &[Vendor::Claude], fake(vec![]));
    let saved = back.engine.open(&id).unwrap();
    assert_eq!(saved.messages.last().unwrap().status, Some(Status::Stopped));
    back.time.store(WHEN + 601, Ordering::SeqCst);
    back.engine.set_utc_offset(0);
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(back.engine.state().running.items.is_empty());
    assert!(back.claude.args.lock().unwrap().is_empty());
    back.engine.shutdown();
}

#[tokio::test]
async fn restarting_keeps_explicit_quota_recovery_available_for_the_saved_reply() {
    for effort in [None, Some(Effort::High)] {
        for legacy in [false, true] {
            let dir = support::DataFolder::new();
            let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
            r.engine.note_account_events(Vendor::Claude, vec![quota("five_hour", 600, 1.0)]);
            let mut req = request(None, "claude");
            req.choice.model = Some("claude-haiku-5-5".into());
            req.choice.effort = effort;
            req.images = vec![faber_engine::model::Image { media_type: "image/png".into(), base64: "aW1hZ2U=".into() }];
            req.files = vec![faber_engine::workspace::Attachment { name: "notes.txt".into(), base64: "bm90ZXM=".into() }];
            let id = r.engine.send(req.clone()).unwrap();
            let m = reply(&r.engine, &id, Status::Error).await;
            assert_eq!(m.error.as_deref(), Some("error.quotaOut"));
            let store = Store::new(dir.path());
            let mut saved = store.get(&id).unwrap();
            assert_eq!(saved.messages.last().unwrap().recovery.as_ref().unwrap().pick.model, req.choice.model);
            drop(r);
            if legacy {
                saved.messages.last_mut().unwrap().recovery = None;
                saved.messages.last_mut().unwrap().effort = effort.map(|e| e.name().into());
                store.save(&saved);
            }
            let mut entries = fixture("claude", "chat");
            entries[0].msg["message"]["content"] = json!([
                {"type":"image","source":{"type":"base64","media_type":"image/png","data":"aW1hZ2U="}},
                {"type":"text","text":format!("{}\n\nAttached files (in your working folder):\n- attachments/notes.txt",req.text)}
            ]);
            let back = sources(&dir, &[Vendor::Claude], fake(entries), fake(vec![]));
            back.engine.open(&id).unwrap();
            back.engine.note_account_events(Vendor::Claude, vec![quota("five_hour", 600, 1.0)]);
            back.engine.wait_turn(&id, &m.id).unwrap();
            assert!(back.engine.open(&id).unwrap().messages.last().unwrap().wait_until.is_some());
            assert!(back.engine.wait_turn(&id, "another-reply").is_err());
            back.time.store(WHEN + 601, Ordering::SeqCst);
            back.engine.set_utc_offset(0);
            reply(&back.engine, &id, Status::Done).await;
            let expected = effort.unwrap_or(Effort::Medium).name();
            let args = back.claude.args.lock().unwrap().clone();
            let at = args[0].iter().position(|a| a == "--effort").unwrap();
            assert_eq!(args[0][at + 1], expected, "legacy={legacy}");
            assert_eq!(store.get(&id).unwrap().messages.last().unwrap().effort.as_deref(), Some(expected));
            assert_eq!(back.claude.wire.lines_with("\"type\":\"user\""), 1);
            assert!(back.claude.mismatches.lock().unwrap().is_empty());
            assert!(store.get(&id).unwrap().messages.last().unwrap().recovery.is_none());
            back.engine.shutdown();
        }
    }
}
struct Rig {
    engine: Engine,
    time: Arc<AtomicU64>,
    claude: Fake,
    codex: Fake,
}

#[tokio::test]
async fn restarted_recovery_never_uses_a_billing_exception_for_a_scheduled_resend() {
    for legacy in [false, true] {
        for billing in [Billing::Unknown, Billing::UsageBilled] {
            let dir = support::DataFolder::new();
            let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
            r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 7 * 3600, 1.0)]);
            let id = r.engine.send(request(None, "auto")).unwrap();
            let m = reply(&r.engine, &id, Status::Error).await;
            assert_eq!(m.error.as_deref(), Some("error.quotaWait"));
            drop(r);
            if legacy {
                let store = Store::new(dir.path());
                let mut saved = store.get(&id).unwrap();
                saved.messages.last_mut().unwrap().recovery = None;
                store.save(&saved);
            }
            let back = rig(&dir, &[Vendor::Claude], fake(vec![]));
            back.engine.open(&id).unwrap();
            back.time.store(WHEN + 7 * 3600 - 600, Ordering::SeqCst);
            back.engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 7 * 3600, 1.0)]);
            back.engine.wait_turn(&id, &m.id).unwrap();
            back.engine
                .note_account_events(Vendor::Claude, vec![EngineEvent::Signals(Signals { billing: Some(billing), ..Signals::default() })]);
            back.time.store(WHEN + 7 * 3600 + 1, Ordering::SeqCst);
            back.engine.set_utc_offset(0);
            tokio::time::sleep(Duration::from_millis(40)).await;
            assert!(back.claude.args.lock().unwrap().is_empty(), "no scheduled resend on {billing:?}, legacy={legacy}");
            assert_eq!(back.engine.open(&id).unwrap().messages.last().unwrap().status, Some(Status::Error));
            back.engine.shutdown();
        }
    }
}
fn rig(dir: &support::DataFolder, vendors: &[Vendor], codex: Fake) -> Rig {
    sources(dir, vendors, fake(fixture("claude", "chat")), codex)
}
fn sources(dir: &support::DataFolder, vendors: &[Vendor], claude: Fake, codex: Fake) -> Rig {
    asking(dir, vendors, claude, codex, r#"{"kind":"chat","difficulty":"easy","confidence":0.95}"#)
}
fn asking(dir: &support::DataFolder, vendors: &[Vendor], claude: Fake, codex: Fake, verdict: &'static str) -> Rig {
    let (c, x) = (claude.spawn.clone(), codex.spawn.clone());
    let engine = Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: vendors.iter().map(|v| (*v, Cli { command: v.name().into(), prefix: vec![] })).collect(),
            spawn: Arc::new(move |bin, args, cwd| if bin == "claude" { c(bin, args, cwd) } else { x(bin, args, cwd) }),
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: Some(Arc::new(move |_| Box::pin(async move { Some(verdict.into()) }))),
        },
        Arc::new(|_| {}),
    )
    .unwrap();
    let time = Arc::new(AtomicU64::new(WHEN));
    let clock = time.clone();
    engine.set_time_source(Arc::new(move || clock.load(Ordering::SeqCst) as f64));
    engine.set_codex_models(vec!["gpt-6.1-sol".into(), "gpt-6-luna".into()], Some("gpt-6.1-sol".into()));
    for v in vendors {
        engine.note_account_events(
            *v,
            vec![EngineEvent::Signals(Signals { billing: Some(Billing::Subscription), paid_fallback: Some(false), ..Signals::default() })],
        );
    }
    Rig { engine, time, claude, codex }
}

#[tokio::test]
async fn a_partial_plan_limit_continues_once_and_the_chat_stays_on_the_new_vendor() {
    use faber_engine::handover::{self, Partial};
    use support::Entry;
    for (narration, tool) in [("", true), ("Checking 🛠️.\n\n", true), ("", false)] {
        let dir = support::DataFolder::new();
        let before = seed(dir.path(), false);
        let output = "x".repeat(500);
        let mut claude_entries = vec![
            fixture("claude", "chat")[0].clone(),
            Entry { input: false, msg: json!({"type":"system","subtype":"init","session_id":"old-session","model":"claude-opus-5-5"}) },
            Entry {
                input: false,
                msg: json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"write","name":"Write","input":{"file_path":"done.txt"}}]}}),
            },
            Entry {
                input: false,
                msg: json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"write","content":output}]}}),
            },
            Entry {
                input: false,
                msg: json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Half done"}}}),
            },
            Entry {
                input: false,
                msg: json!({"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"seven_day","overageStatus":"off"}}),
            },
            Entry {
                input: false,
                msg: json!({"type":"result","subtype":"error_during_execution","is_error":true,"errors":["usage limit reached"],"total_cost_usd":0.02,"usage":{"input_tokens":100,"output_tokens":20}}),
            },
        ];
        if !tool {
            claude_entries.drain(2..4);
        }
        if !narration.is_empty() {
            claude_entries.insert(2, Entry { input: false, msg: json!({"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":narration}}}) });
        }
        let claude = fake(claude_entries);
        let partial = Partial {
            text: format!("{narration}Half done"),
            tools: if tool { vec![("Write".into(), format!("{}...", "x".repeat(197)))] } else { vec![] },
        };
        let prompt = handover::with_full(&handover::transcript(&before.messages), &request(None, "auto").text, &partial);
        let mut entries = fixture("codex", "chat");
        for e in &mut entries {
            if e.input && e.msg["method"] == "turn/start" {
                e.msg["params"]["input"][0]["text"] = json!(prompt);
            }
        }
        if !narration.is_empty() || !tool {
            let at = entries
                .iter()
                .position(|e| e.msg["method"] == "item/started" && e.msg["params"]["item"]["type"] == "agentMessage")
                .unwrap();
            entries.insert(at, Entry { input: false, msg: json!({"method":"item/started","params":{"item":{"type":"commandExecution","id":"resume-tool","command":"check"}}}) });
        }
        let r = sources(&dir, &Vendor::ALL, claude, fake(entries));
        let id = r.engine.send(request(Some(&before.id), "auto")).unwrap();
        let m = reply(&r.engine, &id, Status::Done).await;
        assert_eq!(m.vendor, Some(Vendor::Codex));
        assert_eq!(m.reason.as_deref(), Some("reason.moved.full"));
        assert_eq!(m.moved_from, Some(Vendor::Claude));
        assert!(m.text.starts_with(&partial.text));
        assert_eq!(
            m.activity_end,
            tool.then(|| narration.encode_utf16().count()),
            "the already-visible partial answer must remain outside activity"
        );
        let ledger = r.engine.ledger();
        let own: f64 = ledger.claude.own_spend.values().chain(ledger.codex.own_spend.values()).sum();
        assert!((m.cost_usd.unwrap() - own).abs() < 1e-9, "both vendors' own turn costs are kept");
        assert_eq!(m.tools.unwrap().len(), usize::from(tool) + usize::from(!narration.is_empty() || !tool));
        assert_eq!(r.codex.wire.lines_with("Continue from there."), 1);
        assert_eq!(r.claude.wire.lines_with("\"type\":\"user\""), 1);
        assert!(r.codex.mismatches.lock().unwrap().is_empty());
        r.engine
            .note_account_events(Vendor::Claude, vec![EngineEvent::Signals(Signals { reach: Some(Reach::Clear), ..Signals::default() })]);
        r.engine.send(request(Some(&id), "auto")).unwrap();
        assert_eq!(r.engine.open(&id).unwrap().messages.last().unwrap().vendor, Some(Vendor::Codex));
        assert_eq!(r.claude.wire.lines_with("\"type\":\"user\""), 1);
        r.engine.shutdown();
    }
}

#[tokio::test]
async fn a_family_limit_resumes_on_another_family_of_the_same_vendor() {
    let dir = support::DataFolder::new();
    let before = seed(dir.path(), false);
    let r = rig(&dir, &Vendor::ALL, fake(vec![]));
    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day_opus", 172800, 1.0)]);
    let id = r.engine.send(request(Some(&before.id), "auto")).unwrap();
    let m = reply(&r.engine, &id, Status::Done).await;
    assert_eq!(m.vendor, Some(Vendor::Claude));
    let args = r.claude.args.lock().unwrap().clone();
    assert_eq!(args.len(), 1);
    assert!(args[0].iter().any(|a| a == "--resume"));
    assert!(args[0].iter().any(|a| a == "old-session"));
    assert!(!args[0].iter().any(|a| a.contains("opus")));
    assert!(r.codex.args.lock().unwrap().is_empty());
    r.engine.shutdown();
}

#[tokio::test]
async fn an_explicit_wait_uses_the_pinned_familys_reset() {
    let dir = support::DataFolder::new();
    let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day_opus", 600, 1.0)]);
    let req =
        SendRequest { choice: Choice { vendor: "claude".into(), model: Some("opus".into()), effort: None }, ..request(None, "claude") };
    let id = r.engine.send(req).unwrap();
    let m = reply(&r.engine, &id, Status::Error).await;
    assert_eq!(m.error.as_deref(), Some("error.quotaOut"));
    r.engine.wait_turn(&id, &m.id).unwrap();
    assert_eq!(r.engine.open(&id).unwrap().messages.last().unwrap().wait_until, Some((WHEN + 600) as f64));
    assert!(r.claude.args.lock().unwrap().is_empty());
    r.engine.shutdown();
}
fn request(id: Option<&str>, vendor: &str) -> SendRequest {
    SendRequest {
        conversation_id: id.map(String::from),
        area: Area::Chat,
        project_id: None,
        text: "In one sentence: why is the sky blue?".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: vendor.into(), model: None, effort: None },
    }
}
fn quota(window: &str, after: u64, used: f64) -> EngineEvent {
    EngineEvent::Quota { window: window.into(), used, resets_at: (WHEN + after) as f64 }
}
async fn reply(engine: &Engine, id: &str, status: Status) -> Message {
    for _ in 0..200 {
        if let Some(m) = engine.open(id).and_then(|c| c.messages.last().cloned()).filter(|m| m.status == Some(status)) {
            return m;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("reply never became {status:?}: {:?}", engine.open(id));
}
#[tokio::test]
async fn a_message_for_a_vendor_that_resets_soon_waits_and_sends_itself() {
    let dir = support::DataFolder::new();
    let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
    r.engine.note_account_events(Vendor::Claude, vec![quota("five_hour", 600, 1.0)]);
    let id = r.engine.send(request(None, "auto")).unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    let m = reply(&r.engine, &id, Status::Streaming).await;
    assert_eq!(m.reason.as_deref(), Some("reason.waiting"));
    assert_eq!(m.wait_until, Some((WHEN + 600) as f64));
    assert!(r.claude.args.lock().unwrap().is_empty());
    assert!(r.engine.send(request(Some(&id), "auto")).is_ok());
    assert_eq!(r.engine.open(&id).unwrap().queued.len(), 1);
    let queued_id = r.engine.open(&id).unwrap().queued[0].id.clone();
    r.engine.remove_queued(&id, &queued_id);
    assert!(r.engine.open(&id).unwrap().queued.is_empty());
    r.time.store(WHEN + 601, Ordering::SeqCst);
    r.engine.set_utc_offset(0);
    reply(&r.engine, &id, Status::Done).await;
    assert_eq!(r.claude.wire.lines_with("\"type\":\"user\""), 1);
    r.engine.set_utc_offset(0);
    assert_eq!(r.claude.wire.lines_with("\"type\":\"user\""), 1);
    r.engine.shutdown();
}
#[tokio::test]
async fn a_wait_longer_than_six_hours_is_refused_and_the_original_text_is_kept() {
    let dir = support::DataFolder::new();
    let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 172800, 1.0)]);
    let id = r.engine.send(request(None, "auto")).unwrap();
    let m = reply(&r.engine, &id, Status::Error).await;
    assert_eq!(m.error.as_deref(), Some("error.quotaWait"));
    assert_eq!(r.engine.open(&id).unwrap().messages[0].text, request(None, "auto").text);
    assert!(r.claude.args.lock().unwrap().is_empty());
    r.engine.shutdown();
}
#[tokio::test]
async fn closing_the_app_or_stopping_cancels_a_waiting_message() {
    for closing in [false, true] {
        let dir = support::DataFolder::new();
        let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
        r.engine.note_account_events(Vendor::Claude, vec![quota("five_hour", 600, 1.0)]);
        let id = r.engine.send(request(None, "auto")).unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
        if closing {
            r.engine.shutdown();
        } else {
            r.engine.interrupt(&id);
        }
        assert_eq!(r.engine.open(&id).unwrap().messages.last().unwrap().status, Some(Status::Stopped));
        r.time.store(WHEN + 601, Ordering::SeqCst);
        r.engine.set_utc_offset(0);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(r.claude.args.lock().unwrap().is_empty());
        let reopened = rig(&dir, &[Vendor::Claude], fake(vec![]));
        assert_eq!(reopened.engine.open(&id).unwrap().messages.last().unwrap().status, Some(Status::Stopped));
        assert_eq!(reopened.engine.open(&id).unwrap().messages[0].text, request(None, "auto").text);
        assert!(reopened.claude.args.lock().unwrap().is_empty());
        r.engine.shutdown();
        reopened.engine.shutdown();
    }
}
fn seed(dir: &Path, long: bool) -> Conversation {
    let mut answer = Message::new("answer".into(), Role::Assistant, "Earlier answer".into(), 2);
    answer.status = Some(Status::Done);
    answer.vendor = Some(Vendor::Claude);
    answer.model = Some("claude-opus-5-5".into());
    answer.reason = Some("reason.route.chat.hard".into());
    let c = Conversation {
        session_kits: Default::default(),
        session_start_cards: Default::default(),
        session_resource_digests: Default::default(),
        session_guide_digests: Default::default(),
        commands: BTreeMap::new(),
        working_folder: None,
        project_start_pending: false,
        copy: None,
        earlier: 0,
        choice: None,
        id: "seeded".into(),
        title: "Existing chat".into(),
        rewind_notice: None,
        rewind_summary: None,
        area: Area::Chat,
        project_id: None,
        sessions: BTreeMap::from([(Vendor::Claude, "old-session".into())]),
        tentative: BTreeMap::new(),
        command_gate: false,
        codex_plan: false,
        cost_base: BTreeMap::new(),
        allow_always: None,
        allow_always_epochs: Default::default(),
        branch_of: None,
        started_by: None,
        pending_parent_report: false,
        waiting_on: None,
        goal_fix: None,
        spent_usd: 0.0,
        estimate: None,
        user_name: None,
        persona_version: None,
        created_at: 1,
        updated_at: 2,
        queued: vec![],
        keep_going: None,
        bigger_offered: false,
        archived_at: None,
        pinned_at: None,
        ended: None,
        plan: None,
        stuck: None,
        stopped: false,
        send_after_stop: None,
        context: None,
        fresh_after: None,
        reported: (None, None),
        compacting: false,
        piece_values: vec![],
        browser: None,
        messages: vec![
            Message::new("first".into(), Role::User, if long { "x".repeat(30_000) } else { "Original question".into() }, 1),
            answer,
        ],
        weighing: None,
    };
    Store::new(dir).save(&c);
    c
}
#[tokio::test]
async fn a_failed_hand_over_ends_with_move_failed_and_keeps_the_conversation() {
    let dir = support::DataFolder::new();
    let before = seed(dir.path(), true);
    let mut entries = fixture("codex", "chat");
    let at = entries.iter().position(|e| e.input && e.msg["method"] == "turn/start").unwrap();
    let faber_engine::handover::Plan::Summarise(prompt) = faber_engine::handover::plan(&before.messages) else { panic!("long chat") };
    entries[at].msg["params"]["input"][0]["text"] = json!(prompt);
    entries.truncate(at + 1);
    entries.push(support::Entry { input: false, msg: json!({"id":3,"error":{"code":-1,"message":"summary unavailable"}}) });
    let r = rig(&dir, &Vendor::ALL, fake(entries));
    r.engine.note_account_events(Vendor::Claude, vec![quota("seven_day", 172800, 1.0)]);
    let id = r.engine.send(request(Some(&before.id), "auto")).unwrap();
    let m = reply(&r.engine, &id, Status::Error).await;
    assert_eq!(m.error.as_deref(), Some("error.moveFailed"));
    let after = Store::new(dir.path()).get(&id).unwrap();
    assert_eq!(&after.messages[..2], &before.messages[..]);
    assert_eq!(after.messages.last().unwrap().vendor, Some(Vendor::Claude));
    assert_eq!(r.codex.wire.lines_with("\"method\":\"turn/start\""), 1, "only the summary was asked");
    r.engine.shutdown();
}
#[tokio::test]
async fn recovery_uses_the_latest_block_and_never_queues_unknown_billing() {
    let dir = support::DataFolder::new();
    let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
    r.engine.note_account_events(Vendor::Claude, vec![quota("five_hour", 600, 1.0), quota("seven_day", 172800, 0.985)]);
    let id = r.engine.send(request(None, "auto")).unwrap();
    assert_eq!(reply(&r.engine, &id, Status::Error).await.error.as_deref(), Some("error.quotaWait"));
    r.engine
        .note_account_events(Vendor::Claude, vec![EngineEvent::Signals(Signals { billing: Some(Billing::Unknown), ..Signals::default() })]);
    let id = r.engine.send(request(None, "auto")).unwrap();
    assert_eq!(reply(&r.engine, &id, Status::Error).await.error.as_deref(), Some("error.billingUnknown"));
    assert!(r.claude.args.lock().unwrap().is_empty());
    r.engine.shutdown();
}
#[tokio::test]
async fn paid_consent_is_bound_to_one_reply_and_is_consumed() {
    let dir = support::DataFolder::new();
    let r = rig(&dir, &[Vendor::Claude], fake(vec![]));
    r.engine.note_account_events(
        Vendor::Claude,
        vec![
            quota("five_hour", 600, 1.0),
            EngineEvent::Signals(Signals {
                paid_fallback: Some(true),
                reach: Some(Reach::Out(Some((WHEN + 600) as f64))),
                ..Signals::default()
            }),
        ],
    );
    let id = r.engine.send(request(None, "auto")).unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    let m = r.engine.open(&id).unwrap().messages.last().unwrap().clone();
    assert!(r.engine.approve_credits(&id, "wrong", Vendor::Claude).is_err());
    assert!(r.engine.approve_credits(&id, &m.id, Vendor::Codex).is_err());
    r.engine.approve_credits(&id, &m.id, Vendor::Claude).unwrap();
    reply(&r.engine, &id, Status::Done).await;
    assert!(r.engine.approve_credits(&id, &m.id, Vendor::Claude).is_err());
    r.engine.note_account_events(Vendor::Claude, vec![quota("five_hour", 600, 1.0)]);
    r.engine.send(request(Some(&id), "auto")).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(r.claude.wire.lines_with("\"type\":\"user\""), 1, "the next turn got no approval");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_quota_recovery_keeps_the_blind_quality_floor_of_the_original_verdict() {
    let dir = support::DataFolder::new();
    // Codex runs out of quota right after the turn starts.
    let mut codex = fixture("codex", "chat");
    let end = codex.iter().position(|e| e.msg["method"] == "turn/completed").unwrap();
    let start = codex.iter().position(|e| e.msg["method"] == "item/started").unwrap();
    let mut done = codex[end].clone();
    done.msg["params"]["turn"]["status"] = json!("failed");
    done.msg["params"]["turn"]["error"] = json!({"message":"usage limit reached"});
    codex.truncate(start);
    codex.push(done);
    let r = asking(
        &dir,
        &Vendor::ALL,
        fake(fixture("claude", "chat")),
        fake(codex),
        r#"{"kind":"code","difficulty":"medium","check":"blind","confidence":0.95}"#,
    );
    r.engine.send(request(None, "auto")).unwrap();
    for _ in 0..200 {
        if !r.claude.args.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let args = r.claude.args.lock().unwrap().first().cloned().expect("the turn moved to Claude");
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].clone());
    assert_eq!(flag("--model").as_deref(), Some("claude-sonnet-5-5"));
    assert_eq!(flag("--effort").as_deref(), Some("high"), "blind code keeps its step up after the move");
    r.engine.shutdown();
}

#[tokio::test]
async fn a_lifted_pick_that_runs_out_of_quota_is_re_picked_and_the_answer_no_longer_says_learned() {
    let dir = support::DataFolder::new();
    std::fs::write(dir.path().join("auto-learned.json"), format!(r#"{{"kinds":{{"code":{{"marks":[],"lifted":true,"at":{WHEN}}}}}}}"#))
        .unwrap();
    // Codex runs out of quota right after the turn starts, so recovery moves the answer to Claude.
    let mut codex = fixture("codex", "chat");
    let end = codex.iter().position(|e| e.msg["method"] == "turn/completed").unwrap();
    let start = codex.iter().position(|e| e.msg["method"] == "item/started").unwrap();
    let mut done = codex[end].clone();
    done.msg["params"]["turn"]["status"] = json!("failed");
    done.msg["params"]["turn"]["error"] = json!({"message":"usage limit reached"});
    codex.truncate(start);
    codex.push(done);
    let r = asking(
        &dir,
        &Vendor::ALL,
        fake(fixture("claude", "chat")),
        fake(codex),
        r#"{"kind":"code","difficulty":"medium","check":"blind","confidence":0.95}"#,
    );
    let id = r.engine.send(request(None, "auto")).unwrap();
    for _ in 0..300 {
        if !r.claude.args.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let m = r.engine.open(&id).unwrap().messages.last().unwrap().clone();
    assert!(!r.codex.args.lock().unwrap().is_empty(), "the lifted pick started at Codex");
    assert!(!r.claude.args.lock().unwrap().is_empty(), "recovery moved the turn to Claude");
    assert!(m.pace.as_ref().is_none_or(|p| p.kind != "learned"), "{:?}", m.pace);
    r.engine.shutdown();
}
