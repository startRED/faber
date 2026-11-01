//! Regressions from the independent review of plan approval. Every CLI is a recording in memory.

use crate::support::{fake, fixture};
use faber_engine::cli::Cli;
use faber_engine::events::EngineEvent;
use faber_engine::model::{Area, Conversation, Permissions, Role, RunState, Settings, Status, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::{Engine, EngineOptions, SendRequest, WarmRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn engine(dir: &crate::support::DataFolder, vendor: Vendor, spawn: Spawner) -> Engine {
    Engine::new(
        EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.chat_path().into(),
            clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
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
    .unwrap()
}

fn choice(vendor: &str) -> Choice {
    Choice { vendor: vendor.into(), model: None, effort: None }
}

fn request(id: Option<&str>, vendor: &str, text: &str) -> SendRequest {
    SendRequest {
        conversation_id: id.map(String::from),
        area: Area::Chat,
        project_id: None,
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: choice(vendor),
    }
}

async fn until(engine: &Engine, id: &str, check: impl Fn(&Conversation) -> bool) -> Conversation {
    tokio::time::timeout(crate::support::DEADLINE, async {
        loop {
            let conversation = engine.open(id).unwrap();
            if check(&conversation) {
                return conversation;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the recorded turn did not reach the expected state")
}

async fn plan(engine: &Engine) -> Conversation {
    engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
    let id = engine.send(request(None, "codex", "Plan this change.")).unwrap();
    until(engine, &id, |c| {
        c.messages.last().is_some_and(|m| m.status == Some(Status::Done) && m.permissions.as_ref().is_some_and(|p| !p.is_empty()))
    })
    .await
}

fn save(dir: &Path, conversation: &serde_json::Value) {
    let file = dir.join("conversations").join(format!("{}.json", conversation["id"].as_str().unwrap()));
    std::fs::write(file, serde_json::to_vec(conversation).unwrap()).unwrap();
}

struct Capabilities;

impl faber_engine::browser::Browser for Capabilities {
    fn act(
        &self,
        _: String,
        _: String,
        _: faber_engine::browser::Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<faber_engine::browser::Page, String>> + Send>> {
        Box::pin(async { Err("the capability listing must not open a browser".into()) })
    }
}

impl faber_engine::verify::Runner for Capabilities {
    fn start(&self, _: &faber_engine::verify::Spec) -> std::io::Result<faber_engine::verify::Started> {
        Err(std::io::Error::other("the capability listing must not run a check"))
    }
}

async fn full_access_relaunch(area: Area) {
    let dir = crate::support::DataFolder::new();
    let mut planning = fixture("claude", "approve-plan");
    let mut next = fixture("claude", "approve-plan-run");
    if area == Area::Project {
        for (recording, request_id) in [(&mut planning, "list-plan"), (&mut next, "list-work")] {
            recording.splice(
                2..2,
                [
                    crate::support::Entry {
                        input: false,
                        msg: json!({"type":"control_request","request_id":request_id,"request":{"subtype":"mcp_message",
                        "server_name":"faber","message":{"jsonrpc":"2.0","id":1,"method":"tools/list"}}}),
                    },
                    crate::support::Entry {
                        input: true,
                        msg: json!({"type":"control_response","response":{"subtype":"success","request_id":request_id,
                        "response":{"mcp_response":{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}}}}),
                    },
                ],
            );
        }
    }
    let first = fake(planning);
    let resumed = fake(next);
    let (a, b) = (first.spawn.clone(), resumed.spawn.clone());
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let spawn: Spawner =
        Arc::new(
            move |bin, args, cwd| {
                if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 { a(bin, args, cwd) } else { b(bin, args, cwd) }
            },
        );
    let engine = engine(&dir, Vendor::Claude, spawn);
    let folder = crate::support::work_folder();
    let project_id = if area == Area::Project {
        engine.set_browser(Arc::new(Capabilities));
        engine.set_verifier(Arc::new(faber_engine::verify::Verifier::new(Arc::new(Capabilities))));
        Some(engine.add_project(&folder.path().to_string_lossy()).unwrap().id)
    } else {
        None
    };
    engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
    let id = engine.send(SendRequest { area, project_id, ..request(None, "claude", "Plan this change.") }).unwrap();
    let before = until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
    let reply = before.messages.last().unwrap();
    let card = &reply.permissions.as_ref().unwrap()[0];
    engine.approve_plan(&id, &reply.id, &card.request_id, Some(Permissions::FullAccess)).await.unwrap();
    let done = until(&engine, &id, |c| c.messages.len() == 4 && c.messages.last().unwrap().status == Some(Status::Done)).await;
    assert_eq!(engine.state().settings.permissions, Some(Permissions::FullAccess));
    assert_eq!(done.messages[1].status, Some(Status::Done), "approval finishes planning successfully");
    assert_eq!(done.messages[1].input_tokens, Some(100), "finishing Plan keeps its prompt count");
    assert_eq!(done.messages[1].output_tokens, Some(20), "finishing Plan keeps its answer count");
    assert!((done.messages[1].cost_usd.unwrap() - 0.03).abs() < 1e-9, "planning retains its own cost");
    assert_eq!(done.messages[3].input_tokens, Some(200));
    assert_eq!(done.messages[3].output_tokens, Some(30));
    assert!((done.messages[3].cost_usd.unwrap() - 0.02).abs() < 1e-9, "the resumed work costs only its own share");
    if area == Area::Chat {
        assert!(first.args.lock().unwrap()[0].contains(&"--restricted".into()));
    }
    let args = &resumed.args.lock().unwrap()[0];
    assert!(!args.contains(&"--restricted".into()));
    assert!(args.windows(2).any(|a| a == ["--permission-mode", "bypassPermissions"]));
    assert!(args.windows(2).any(|a| a == ["--resume", "plan-session"]));
    if area == Area::Project {
        assert!(args.windows(2).any(|a| a[0] == "--mcp-config" && a[1].contains("\"type\":\"sdk\"")));
        let names = |messages: Vec<serde_json::Value>, request_id: &str| {
            messages.into_iter().find(|m| m["response"]["request_id"] == request_id).unwrap()["response"]["response"]
                ["mcp_response"]["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        let planning_tools = names(first.wire.messages(), "list-plan");
        let work_tools = names(resumed.wire.messages(), "list-work");
        for name in ["verify", "map", "browse"] {
            assert!(!planning_tools.iter().any(|t| t == name), "Plan must not offer {name}");
            assert!(work_tools.iter().any(|t| t == name), "approved Full access must offer {name}");
        }
    }
    let response = first.wire.messages().into_iter().find(|m| m["response"]["request_id"] == "plan-exit").unwrap();
    assert_eq!(response["response"]["response"]["behavior"], "allow");
    assert!(response["response"]["response"].get("updatedPermissions").is_none(), "approval never changes the old CLI mode");
    assert_eq!(done.messages[2].wake.as_ref().unwrap().why, "wake.approvedPlan", "approval sends a quiet ordinary turn");
    assert!(first.mismatches.lock().unwrap().is_empty(), "{:?}", first.mismatches.lock().unwrap());
    assert!(resumed.mismatches.lock().unwrap().is_empty(), "{:?}", resumed.mismatches.lock().unwrap());
    engine.shutdown();
}

#[tokio::test]
async fn r160_review_full_access_approval_resumes_chat_outside_its_restricted_plan_process_and_keeps_usage() {
    full_access_relaunch(Area::Chat).await;
}

#[tokio::test]
async fn r160_review_full_access_approval_relaunches_projects_with_the_full_tools_and_keeps_usage() {
    full_access_relaunch(Area::Project).await;
}

#[tokio::test]
async fn r160_review_both_clis_start_the_same_quiet_approved_turn_in_every_non_plan_mode() {
    for vendor in Vendor::ALL {
        for mode in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Auto, Permissions::FullAccess] {
            let dir = crate::support::DataFolder::new();
            let first = fake(fixture(vendor.name(), "approve-plan"));
            let resumed = fake(fixture(vendor.name(), "approve-plan-run"));
            let (a, b) = (first.spawn.clone(), resumed.spawn.clone());
            let calls = std::sync::atomic::AtomicUsize::new(0);
            let spawn: Spawner =
                Arc::new(
                    move |bin, args, cwd| {
                        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 { a(bin, args, cwd) } else { b(bin, args, cwd) }
                    },
                );
            let engine = engine(&dir, vendor, spawn);
            engine
                .save_settings(Settings {
                    permissions: Some(if vendor == Vendor::Claude && mode == Permissions::Ask {
                        Permissions::FullAccess
                    } else {
                        Permissions::Plan
                    }),
                    ..Settings::default()
                })
                .unwrap();
            let id = engine.send(request(None, vendor.name(), "Plan this change.")).unwrap();
            let before =
                until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
            let reply = before.messages.last().unwrap();
            let request_id = &reply.permissions.as_ref().unwrap()[0].request_id;
            if vendor == Vendor::Claude {
                assert!(reply.text.is_empty(), "the CLI need not repeat the plan as reply text");
                assert_eq!(reply.permissions.as_ref().unwrap()[0].input["plan"], "1. Inspect the parser.\n2. Add the check.");
            }
            engine.approve_plan(&id, &reply.id, request_id, Some(mode)).await.unwrap();
            assert_eq!(engine.approve_plan(&id, &reply.id, request_id, Some(mode)).await.unwrap_err(), "error.permission.stale");
            let done = until(&engine, &id, |c| c.messages.len() == 4 && c.messages.last().unwrap().status == Some(Status::Done)).await;
            assert_eq!(engine.state().settings.permissions, Some(mode), "{vendor:?} {mode:?}");
            assert_eq!(done.messages[1].status, Some(Status::Done), "planning ended successfully");
            assert_eq!(done.messages[2].text, "Go ahead with the plan.");
            assert_eq!(done.messages[2].wake.as_ref().unwrap().why, "wake.approvedPlan");
            assert!(done.queued.is_empty());
            assert!(done.messages.iter().all(|m| m.permissions.as_ref().is_none_or(Vec::is_empty)));
            if vendor == Vendor::Claude {
                let messages = first.wire.messages();
                let response = messages.iter().find(|m| m["response"]["request_id"] == "plan-exit").unwrap();
                assert_eq!(response["response"]["response"]["behavior"], "allow");
                assert!(response["response"]["response"].get("updatedPermissions").is_none());
                let args = &resumed.args.lock().unwrap()[0];
                let native = faber_engine::claude::permission_mode(mode.for_claude(Default::default()));
                assert!(args.windows(2).any(|a| a == ["--permission-mode", native]));
            } else {
                let messages = resumed.wire.messages();
                let resume = messages.iter().find(|m| m["method"] == "thread/resume").unwrap();
                assert_eq!(
                    resume["params"]["sandbox"],
                    if mode == Permissions::FullAccess { "danger-full-access" } else { "workspace-write" }
                );
            }
            assert!(first.mismatches.lock().unwrap().is_empty(), "{:?}", first.mismatches.lock().unwrap());
            assert!(resumed.mismatches.lock().unwrap().is_empty(), "{:?}", resumed.mismatches.lock().unwrap());
            engine.shutdown();
        }
    }
}

#[tokio::test]
async fn r160_review_a_later_permission_choice_revokes_an_old_plan_approval_even_after_restart() {
    for change_back in [false, true] {
        let dir = crate::support::DataFolder::new();
        let f = fake(fixture("codex", "approve-plan"));
        let original = engine(&dir, Vendor::Codex, f.spawn.clone());
        let before = plan(&original).await;
        let reply = before.messages.last().unwrap();
        let card = &reply.permissions.as_ref().unwrap()[0];
        assert_eq!(card.input["mode"], "dontAsk");
        original.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        if change_back {
            original.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Settings::default() }).unwrap();
        }
        original.shutdown();
        let reopened = engine(&dir, Vendor::Codex, fake(vec![]).spawn);
        let current = reopened.state().settings.permissions;
        for _ in 0..2 {
            assert_eq!(
                reopened.approve_plan(&before.id, &reply.id, &card.request_id, Some(Permissions::FullAccess)).await,
                Err("error.permission.planChanged".into()),
                "an old card cannot replace a later choice, including a choice changed back to its old value"
            );
            assert_eq!(reopened.state().settings.permissions, current);
            let after = reopened.open(&before.id).unwrap();
            assert_eq!(serde_json::to_value(&after.messages).unwrap(), serde_json::to_value(&before.messages).unwrap());
            assert!(after.queued.is_empty());
        }
        // Keeping planning does not grant permissions, and remains possible on the expired card.
        reopened.approve_plan(&before.id, &reply.id, &card.request_id, None).await.unwrap();
        assert_eq!(reopened.state().settings.permissions, current);
        assert!(reopened.open(&before.id).unwrap().messages.last().unwrap().permissions.as_ref().is_none_or(Vec::is_empty));
        reopened.shutdown();
    }
}

#[tokio::test]
async fn r160_review_claude_approval_queues_immediately_and_stop_removes_the_approved_work() {
    let dir = crate::support::DataFolder::new();
    let mut entries = fixture("claude", "approve-plan");
    let result = entries.iter().position(|e| e.msg["type"] == "result").unwrap();
    entries.insert(result, crate::support::Entry { input: false, msg: json!({"faber_hold":1000}) });
    let f = fake(entries);
    let engine = engine(&dir, Vendor::Claude, f.spawn.clone());
    engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
    let id = engine.send(request(None, "claude", "Plan this change.")).unwrap();
    let before = until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
    let reply = before.messages.last().unwrap();
    let request_id = &reply.permissions.as_ref().unwrap()[0].request_id;
    tokio::time::timeout(Duration::from_millis(250), engine.approve_plan(&id, &reply.id, request_id, Some(Permissions::FullAccess)))
        .await
        .expect("approval must not wait for the planning process to finish")
        .unwrap();
    let queued = engine.open(&id).unwrap();
    assert_eq!(queued.messages.len(), 2, "the work waits for the planning result");
    assert_eq!(queued.queued.len(), 1);
    assert_eq!(queued.queued[0].wake.as_ref().unwrap().why, "wake.approvedPlan");
    let screen = engine.state();
    let working = screen.running.items.iter().find(|item| item.conversation_id == id).expect("the screen keeps the approved queue at work");
    assert_eq!(working.state, RunState::Working);
    assert!(!working.settled, "the composer must keep offering Stop while the plan closes");
    assert!(queued.messages.last().unwrap().permissions.as_ref().is_none_or(Vec::is_empty));
    assert_eq!(engine.approve_plan(&id, &reply.id, request_id, Some(Permissions::FullAccess)).await.unwrap_err(), "error.permission.stale");
    engine.interrupt(&id);
    assert!(engine.open(&id).unwrap().queued.is_empty(), "Stop cancels the queued approval");
    assert!(
        engine.state().running.items.iter().all(|item| item.conversation_id != id),
        "cancelled approved work leaves the screen's running list"
    );
    let done = until(&engine, &id, |c| c.messages.last().unwrap().input_tokens == Some(100)).await;
    assert_eq!(done.messages.len(), 2, "Stop does not send the approved work after the planning result");
    assert_eq!(f.args.lock().unwrap().len(), 1);
    assert!(f.mismatches.lock().unwrap().is_empty(), "{:?}", f.mismatches.lock().unwrap());
    engine.shutdown();
}

#[tokio::test]
async fn r160_review_ask_before_plan_is_the_approval_default_even_for_the_first_conversation() {
    for vendor in Vendor::ALL {
        let dir = crate::support::DataFolder::new();
        let engine = engine(&dir, vendor, fake(fixture(vendor.name(), "approve-plan")).spawn);
        engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
        let id = engine.send(request(None, vendor.name(), "Plan this change.")).unwrap();
        let card = until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))).await;
        assert_eq!(card.messages.last().unwrap().permissions.as_ref().unwrap()[0].input["mode"], "ask");
        engine.shutdown();
    }
}

#[tokio::test]
async fn r160_review_legacy_conversation_modes_are_ignored_and_siblings_share_settings() {
    let dir = crate::support::DataFolder::new();
    let first = engine(&dir, Vendor::Codex, fake(fixture("codex", "approve-plan")).spawn);
    let seed = plan(&first).await;
    first.shutdown();
    let ids = ["11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222"];
    for (id, old_mode) in ids.iter().zip(["dontAsk", "plan"]) {
        let mut saved = serde_json::to_value(&seed).unwrap();
        saved["id"] = (*id).into();
        saved["permissions"] = old_mode.into();
        saved["permissionsEpoch"] = "obsolete-conversation-override".into();
        saved["beforePlan"] = "ask".into();
        save(dir.path(), &saved);
    }
    let f = fake(vec![]);
    let reopened = engine(&dir, Vendor::Claude, f.spawn.clone());
    reopened.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    for id in ids {
        let saved = serde_json::to_value(reopened.open(id).unwrap()).unwrap();
        assert!(saved.get("permissions").is_none(), "the obsolete override is never retained");
        assert!(saved.get("permissionsEpoch").is_none(), "no revision is needed for one shared mode");
        assert!(saved.get("beforePlan").is_none(), "approval history belongs to Settings too");
        reopened.warm(WarmRequest { conversation_id: Some(id.into()), area: Area::Chat, project_id: None, choice: choice("claude") });
    }
    assert_eq!(reopened.state().settings.permissions, Some(Permissions::Ask));
    assert!(f.args.lock().unwrap().iter().all(|args| args.windows(2).any(|a| a == ["--permission-mode", "default"])));
    reopened.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Settings::default() }).unwrap();
    for id in ids {
        reopened.warm(WarmRequest { conversation_id: Some(id.into()), area: Area::Chat, project_id: None, choice: choice("claude") });
    }
    assert_eq!(reopened.state().settings.permissions, Some(Permissions::FullAccess));
    let args = f.args.lock().unwrap();
    assert_eq!(args.len(), 4);
    assert!(args[2..].iter().all(|args| args.windows(2).any(|a| a == ["--permission-mode", "bypassPermissions"])));
    drop(args);
    reopened.shutdown();
}

#[tokio::test]
async fn r160_review_a_warmed_new_chat_follows_the_default_until_its_first_send() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("claude", "chat"));
    let engine = engine(&dir, Vendor::Claude, f.spawn.clone());
    engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
    engine.warm(WarmRequest { conversation_id: None, area: Area::Chat, project_id: None, choice: choice("claude") });
    engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
    let id = engine.send(request(None, "claude", "In one sentence: why is the sky blue?")).unwrap();
    until(&engine, &id, |c| c.messages.last().is_some_and(|m| m.status == Some(Status::Done))).await;
    let args = f.args.lock().unwrap();
    assert!(
        args.last().unwrap().windows(2).any(|a| a == ["--permission-mode", "default"]),
        "the first message uses the visible Ask default"
    );
    assert!(f.mismatches.lock().unwrap().is_empty(), "{:?}", f.mismatches.lock().unwrap());
    drop(args);
    engine.shutdown();
}

#[tokio::test]
async fn r160_review_a_failed_codex_approval_keeps_the_original_mode_and_card() {
    let dir = crate::support::DataFolder::new();
    let engine = engine(&dir, Vendor::Codex, fake(fixture("codex", "approve-plan")).spawn);
    let before = plan(&engine).await;
    let reply = before.messages.last().unwrap();
    let card = &reply.permissions.as_ref().unwrap()[0];
    engine.replace_clis(BTreeMap::new());
    assert_eq!(
        engine.approve_plan(&before.id, &reply.id, &card.request_id, Some(Permissions::FullAccess)).await.unwrap_err(),
        "error.noSubscription"
    );
    let after = engine.open(&before.id).unwrap();
    assert_eq!(engine.state().settings.permissions, Some(Permissions::Plan), "an undelivered approval cannot change the mode");
    assert_eq!(after.messages.last().unwrap().permissions, reply.permissions, "the person can retry the same card");
    engine.shutdown();
}

#[tokio::test]
async fn r160_review_a_failed_settings_write_keeps_the_shared_mode_and_never_starts_work() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("codex", "approve-plan"));
    let engine = engine(&dir, Vendor::Codex, f.spawn.clone());
    let before = plan(&engine).await;
    let reply = before.messages.last().unwrap();
    let request_id = &reply.permissions.as_ref().unwrap()[0].request_id;
    let settings = std::fs::read(dir.path().join("settings.json")).unwrap();
    let temporary = dir.path().join("settings.json.tmp");
    std::fs::create_dir(&temporary).unwrap();
    assert_eq!(
        engine.approve_plan(&before.id, &reply.id, request_id, Some(Permissions::FullAccess)).await.unwrap_err(),
        "error.settings.saveFailed"
    );
    assert_eq!(engine.state().settings.permissions, Some(Permissions::Plan));
    assert_eq!(std::fs::read(dir.path().join("settings.json")).unwrap(), settings);
    let failed = engine.open(&before.id).unwrap();
    assert!(failed.messages.last().unwrap().permissions.as_ref().is_none_or(Vec::is_empty), "the accepted card cannot be approved twice");
    assert_eq!(failed.messages.last().unwrap().error.as_deref(), Some("error.settings.saveFailed"));
    assert!(failed.queued.is_empty());
    assert_eq!(f.wire.messages().iter().filter(|m| m["method"] == "turn/start").count(), 1);
    std::fs::remove_dir(temporary).unwrap();
    assert_eq!(engine.approve_plan(&before.id, &reply.id, request_id, None).await, Err("error.permission.stale".into()));
    assert!(engine.open(&before.id).unwrap().messages.last().unwrap().permissions.as_ref().is_none_or(Vec::is_empty));
    engine.shutdown();
}

#[tokio::test]
async fn r160_review_codex_approval_checks_quota_before_changing_the_mode_messages_or_card() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("codex", "approve-plan"));
    let engine = engine(&dir, Vendor::Codex, f.spawn.clone());
    let before = plan(&engine).await;
    let reply = before.messages.last().unwrap();
    let request_id = &reply.permissions.as_ref().unwrap()[0].request_id;
    engine.set_time_source(Arc::new(|| 1_800_000_000.0));
    engine
        .note_account_events(Vendor::Codex, vec![EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: 1_800_003_600.0 }]);
    for _ in 0..2 {
        assert_eq!(
            engine.approve_plan(&before.id, &reply.id, request_id, Some(Permissions::FullAccess)).await.unwrap_err(),
            "error.quotaOut",
            "the same clicked card remains retryable after an exhausted-quota refusal"
        );
        let after = engine.open(&before.id).unwrap();
        assert_eq!(engine.state().settings.permissions, Some(Permissions::Plan));
        assert_eq!(after.messages, before.messages, "no approval prompt or failed reply replaces the waiting card");
    }
    assert_eq!(f.wire.messages().iter().filter(|m| m["method"] == "turn/start").count(), 1);
    engine.shutdown();
}

#[tokio::test]
async fn r160_review_a_new_person_message_clears_the_older_codex_plan_card_after_restart() {
    let dir = crate::support::DataFolder::new();
    let first = engine(&dir, Vendor::Codex, fake(fixture("codex", "approve-plan")).spawn);
    let before = plan(&first).await;
    let old_reply = before.messages.last().unwrap().id.clone();
    first.shutdown();
    let mut recording = fixture("codex", "approve-plan-run");
    recording.iter_mut().find(|e| e.input && e.msg["method"] == "turn/start").unwrap().msg["params"]["input"][0]["text"] =
        "Revise the second step.".into();
    let f = fake(recording);
    let reopened = engine(&dir, Vendor::Codex, f.spawn.clone());
    reopened.send(request(Some(&before.id), "codex", "Revise the second step.")).unwrap();
    let done = until(&reopened, &before.id, |c| c.messages.len() == 4 && c.messages.last().unwrap().status == Some(Status::Done)).await;
    assert!(done.messages.iter().find(|m| m.id == old_reply).unwrap().permissions.as_ref().is_none_or(Vec::is_empty));
    assert!(f.mismatches.lock().unwrap().is_empty(), "{:?}", f.mismatches.lock().unwrap());
    reopened.shutdown();
    let again = engine(&dir, Vendor::Codex, fake(vec![]).spawn);
    assert!(again.open(&before.id).unwrap().messages.iter().all(|m| m.permissions.as_ref().is_none_or(Vec::is_empty)));
    again.shutdown();
}

#[tokio::test]
async fn r160_review_a_new_person_message_clears_the_codex_plan_card_even_when_quota_stops_dispatch() {
    let dir = crate::support::DataFolder::new();
    let f = fake(fixture("codex", "approve-plan"));
    let engine = engine(&dir, Vendor::Codex, f.spawn.clone());
    let before = plan(&engine).await;
    let old_reply = before.messages.last().unwrap().id.clone();
    engine.set_time_source(Arc::new(|| 1_800_000_000.0));
    engine
        .note_account_events(Vendor::Codex, vec![EngineEvent::Quota { window: "five_hour".into(), used: 1.0, resets_at: 1_800_003_600.0 }]);
    assert!(engine.open(&before.id).unwrap().messages.last().unwrap().permissions.as_ref().is_some_and(|p| !p.is_empty()));
    engine.send(request(Some(&before.id), "codex", "Revise the second step.")).unwrap();
    let after = engine.open(&before.id).unwrap();
    assert_eq!(after.messages.len(), 4, "the new person message was accepted");
    assert_eq!(after.messages.last().unwrap().error.as_deref(), Some("error.quotaOut"));
    assert_eq!(f.wire.messages().iter().filter(|m| m["method"] == "turn/start").count(), 1, "quota prevented a second CLI turn");
    assert!(
        after.messages.iter().find(|m| m.id == old_reply).unwrap().permissions.as_ref().is_none_or(Vec::is_empty),
        "accepting a new message retires the old approval even when dispatch fails before starting the CLI"
    );
    engine.shutdown();
    let reopened = self::engine(&dir, Vendor::Codex, fake(vec![]).spawn);
    assert!(reopened.open(&before.id).unwrap().messages.iter().all(|m| m.permissions.as_ref().is_none_or(Vec::is_empty)));
    reopened.shutdown();
}

#[tokio::test]
async fn r160_review_codex_keep_planning_drains_the_message_held_behind_the_card() {
    let dir = crate::support::DataFolder::new();
    let first = engine(&dir, Vendor::Codex, fake(fixture("codex", "approve-plan")).spawn);
    let before = plan(&first).await;
    first.shutdown();
    let mut saved = serde_json::to_value(&before).unwrap();
    saved["queued"] =
        json!([{ "id": "queued-plan-details", "text": "Explain the second step.", "choice": { "vendor": "codex" }, "at": 1 }]);
    save(dir.path(), &saved);
    let mut recording = fixture("codex", "approve-plan-run");
    recording.iter_mut().find(|e| e.input && e.msg["method"] == "turn/start").unwrap().msg["params"]["input"][0]["text"] =
        "Explain the second step.".into();
    let f = fake(recording);
    let reopened = engine(&dir, Vendor::Codex, f.spawn.clone());
    let reply = before.messages.last().unwrap();
    let request_id = &reply.permissions.as_ref().unwrap()[0].request_id;
    reopened.approve_plan(&before.id, &reply.id, request_id, None).await.unwrap();
    let done = until(&reopened, &before.id, |c| c.messages.len() == 4 && c.messages.last().unwrap().status == Some(Status::Done)).await;
    assert_eq!(reopened.state().settings.permissions, Some(Permissions::Plan), "Keep planning keeps the same mode");
    assert!(done.queued.is_empty());
    assert!(done.messages.iter().any(|m| m.role == Role::User && m.text == "Explain the second step."));
    let turn = f.wire.messages().into_iter().find(|m| m["method"] == "turn/start").unwrap();
    assert_eq!(turn["params"]["collaborationMode"]["mode"], "plan");
    assert!(f.mismatches.lock().unwrap().is_empty(), "{:?}", f.mismatches.lock().unwrap());
    reopened.shutdown();
}

#[tokio::test]
async fn r160_approval_drains_the_idle_queue_in_order_before_its_approved_work() {
    for earlier_outcome in ["done", "failed", "unavailable"] {
        let dir = crate::support::DataFolder::new();
        let first = engine(&dir, Vendor::Codex, fake(fixture("codex", "approve-plan")).spawn);
        let before = plan(&first).await;
        first.shutdown();
        let mut saved = serde_json::to_value(&before).unwrap();
        let queued_text = "Read the queued notes.";
        saved["queued"] = json!([{ "id": "queued-other-cli", "text": queued_text, "choice": { "vendor": "claude" }, "at": 1 }]);
        save(dir.path(), &saved);
        let mut earlier = fixture("claude", "approve-plan-run");
        earlier[0].msg["message"]["content"] =
            faber_engine::engine::with_history(&before.messages, queued_text, &|name| name.to_string()).into();
        if earlier_outcome == "failed" {
            earlier.iter_mut().find(|e| e.msg["type"] == "result").unwrap().msg["is_error"] = true.into();
        }
        let earlier = fake(earlier);
        let approved = fake(fixture("codex", "approve-plan-run"));
        let (a, b) = (earlier.spawn.clone(), approved.spawn.clone());
        let spawn: Spawner =
            Arc::new(move |bin, args, cwd| if bin == std::ffi::OsStr::new("claude") { a(bin, args, cwd) } else { b(bin, args, cwd) });
        let reopened = engine(&dir, Vendor::Codex, spawn);
        reopened.replace_clis(
            Vendor::ALL
                .into_iter()
                .filter(|v| earlier_outcome != "unavailable" || *v != Vendor::Claude)
                .map(|v| (v, Cli { command: v.name().into(), prefix: vec![] }))
                .collect(),
        );
        let reply = before.messages.last().unwrap();
        let request_id = &reply.permissions.as_ref().unwrap()[0].request_id;
        reopened.approve_plan(&before.id, &reply.id, request_id, Some(Permissions::Ask)).await.unwrap();
        if earlier_outcome == "unavailable" {
            let held = reopened.open(&before.id).unwrap();
            assert_eq!(held.messages.len(), 2);
            assert_eq!(held.queued.iter().map(|q| q.text.as_str()).collect::<Vec<_>>(), [queued_text, "Go ahead with the plan."]);
            assert!(
                reopened.state().running.items.iter().all(|item| item.conversation_id != before.id),
                "a refused dispatch is held for manual controls, not Working"
            );
            reopened.remove_queued(&before.id, &held.queued[0].id);
            reopened.send_queued(&before.id, &held.queued[1].id).unwrap();
            let done =
                until(&reopened, &before.id, |c| c.messages.len() == 4 && c.messages.last().unwrap().status == Some(Status::Done)).await;
            assert!(done.queued.is_empty());
            assert!(earlier.wire.messages().is_empty());
            assert!(approved.mismatches.lock().unwrap().is_empty(), "{:?}", approved.mismatches.lock().unwrap());
            reopened.shutdown();
            continue;
        }
        if earlier_outcome == "failed" {
            let held =
                until(&reopened, &before.id, |c| c.messages.len() == 4 && c.messages.last().unwrap().status == Some(Status::Stopped)).await;
            assert_eq!(held.queued.len(), 1);
            assert_eq!(held.queued[0].wake.as_ref().unwrap().why, "wake.approvedPlan");
            assert!(
                reopened.state().running.items.iter().all(|item| item.conversation_id != before.id),
                "approved work held after a failure keeps the manual controls"
            );
            assert!(approved.wire.messages().is_empty(), "a failure does not automatically send the approved work");
            reopened.send_queued(&before.id, &held.queued[0].id).unwrap();
        }
        let done = until(&reopened, &before.id, |c| c.messages.len() == 6 && c.messages.last().unwrap().status == Some(Status::Done)).await;
        assert_eq!(
            done.messages.iter().filter(|m| m.role == Role::User).map(|m| m.text.as_str()).collect::<Vec<_>>(),
            ["Plan this change.", queued_text, "Go ahead with the plan."]
        );
        assert_eq!(done.messages[3].vendor, Some(Vendor::Claude));
        assert_eq!(done.messages[5].vendor, Some(Vendor::Codex));
        assert!(done.queued.is_empty());
        assert!(earlier.mismatches.lock().unwrap().is_empty(), "{:?}", earlier.mismatches.lock().unwrap());
        assert!(approved.mismatches.lock().unwrap().is_empty(), "{:?}", approved.mismatches.lock().unwrap());
        reopened.shutdown();
    }
}

#[derive(Default)]
struct ProjectToolProbe {
    pages: std::sync::atomic::AtomicUsize,
    checks: std::sync::atomic::AtomicUsize,
}

impl faber_engine::browser::Browser for ProjectToolProbe {
    fn act(
        &self,
        _: String,
        _: String,
        _: faber_engine::browser::Request,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<faber_engine::browser::Page, String>> + Send>> {
        self.pages.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { Ok(faber_engine::browser::Page { text: "The approved project page.".into(), ..Default::default() }) })
    }
}

impl faber_engine::verify::Runner for ProjectToolProbe {
    fn start(&self, _: &faber_engine::verify::Spec) -> std::io::Result<faber_engine::verify::Started> {
        self.checks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(faber_engine::verify::Started {
            confirmed: None,
            throttle: None,
            peak: None,
            stdout: Box::new(tokio::io::empty()),
            stderr: Box::new(tokio::io::empty()),
            exit: Box::pin(async { Some(0) }),
        })
    }
}

/// Real drivers replay Plan and its resumed work. Tool definitions must survive with the thread while calls follow its mode.
#[tokio::test]
async fn r160_codex_project_started_in_plan_keeps_tools_for_approved_full_access_and_gates_plan_calls() {
    let dir = crate::support::DataFolder::new();
    let mut planning = fixture("codex", "approve-plan");
    let mut work = fixture("codex", "approve-plan-run");
    for (recording, turn, base) in [(&mut planning, "plan-turn", 100), (&mut work, "work-turn", 200)] {
        let at = recording.iter().position(|e| e.msg["method"] == "item/completed").unwrap();
        let mut calls = Vec::new();
        for (n, tool) in ["verify", "map", "browse"].into_iter().enumerate() {
            let id = base + n;
            calls.push(crate::support::Entry {
                input: false,
                msg: json!({"id":id,"method":"item/tool/call",
                "params":{"threadId":"plan-thread","turnId":turn,"callId":format!("{turn}-{tool}"),"tool":tool,
                "arguments":if tool == "browse" { json!({"url":"https://example.org/"}) } else { json!({}) }}}),
            });
            calls.push(crate::support::Entry { input: true, msg: json!({"id":id,"result":{}}) });
        }
        recording.splice(at..at, calls);
    }
    let first = fake(planning);
    let resumed = fake(work);
    let (a, b) = (first.spawn.clone(), resumed.spawn.clone());
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let engine = engine(
        &dir,
        Vendor::Codex,
        Arc::new(
            move |bin, args, cwd| {
                if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 { a(bin, args, cwd) } else { b(bin, args, cwd) }
            },
        ),
    );
    let probe = Arc::new(ProjectToolProbe::default());
    engine.set_browser(probe.clone());
    engine.set_verifier(Arc::new(faber_engine::verify::Verifier::new(probe.clone())));
    let folder = crate::support::work_folder();
    std::fs::write(folder.path().join("AGENTS.md"), "# Rules\n\n## Proof commands\n\n```bash\ncargo fmt --all --check\n```\n").unwrap();
    let project = engine.add_project(&folder.path().to_string_lossy()).unwrap();
    engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
    let id = engine
        .send(SendRequest { area: Area::Project, project_id: Some(project.id), ..request(None, "codex", "Plan this change.") })
        .unwrap();
    let before = until(&engine, &id, |c| {
        c.messages.last().is_some_and(|m| m.status == Some(Status::Done) && m.permissions.as_ref().is_some_and(|p| !p.is_empty()))
    })
    .await;
    let messages = first.wire.messages();
    let start = messages.iter().find(|m| m["method"] == "thread/start").unwrap();
    for tool in ["verify", "map", "browse"] {
        assert!(
            start["params"]["dynamicTools"].as_array().unwrap().iter().any(|t| t["name"] == tool),
            "the Plan thread must retain {tool} for approved work"
        );
    }
    assert_eq!(probe.pages.load(std::sync::atomic::Ordering::SeqCst), 0, "Plan cannot use the browser");
    assert_eq!(probe.checks.load(std::sync::atomic::Ordering::SeqCst), 0, "Plan cannot run checks");
    for id in 100..103 {
        let result = &messages.iter().find(|m| m["id"] == id && m.get("result").is_some()).unwrap()["result"];
        assert_eq!(result["success"], false, "Plan must refuse every gated tool: {result}");
        assert!(result["contentItems"][0]["text"].as_str().unwrap().contains("full access"), "the denial explains the mode: {result}");
    }
    let reply = before.messages.last().unwrap();
    engine.approve_plan(&id, &reply.id, &reply.permissions.as_ref().unwrap()[0].request_id, Some(Permissions::FullAccess)).await.unwrap();
    until(&engine, &id, |c| c.messages.len() == 4 && c.messages.last().unwrap().status == Some(Status::Done)).await;
    let messages = resumed.wire.messages();
    let resume = messages.iter().find(|m| m["method"] == "thread/resume").unwrap();
    assert_eq!(resume["params"]["threadId"], "plan-thread");
    assert!(resume["params"].get("dynamicTools").is_none(), "the installed resume protocol cannot redefine tools");
    for id in 200..203 {
        let result = &messages.iter().find(|m| m["id"] == id && m.get("result").is_some()).unwrap()["result"];
        assert_eq!(result["success"], true, "approved Full access must execute its tool: {result}");
    }
    assert_eq!(probe.pages.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(probe.checks.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(first.mismatches.lock().unwrap().is_empty(), "{:?}", first.mismatches.lock().unwrap());
    assert!(resumed.mismatches.lock().unwrap().is_empty(), "{:?}", resumed.mismatches.lock().unwrap());
    engine.shutdown();
}
