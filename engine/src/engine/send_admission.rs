//! A send is admitted only after its user intent is on disk; these sessions never spawn a process.
use super::answer::tests::{engine, seed};
use super::*;
use crate::events::SessionCommand;

fn request(id: Option<&str>, text: &str) -> SendRequest {
    SendRequest {
        conversation_id: id.map(str::to_owned),
        area: Area::Chat,
        project_id: None,
        text: text.into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "claude".into(), model: None, effort: None },
    }
}

fn available(core: &mut Core) {
    core.opts.clis.insert(Vendor::Claude, Cli { command: "unused".into(), prefix: vec![] });
}

#[tokio::test]
async fn send_admission_idle_failure_restores_existing_turn_and_can_retry_once() {
    let dir = crate::workspace::TestRoot::new();
    let (app, events) = engine(&dir);
    let id = seed(&app, Session::from_fn(|_| {}), &[("old", &[])]);
    app.with(|c| {
        available(c);
        c.live.remove(&id);
        let reply = c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap();
        reply.status = Some(Status::Done);
        reply.vendor = Some(Vendor::Codex);
        reply.permissions =
            Some(vec![Permission { request_id: "plan".into(), tool: "ExitPlanMode".into(), input: serde_json::json!({}), epoch: None }]);
        c.save(&id);
    });
    let before = app.open(&id).unwrap();
    let saved_before = Store::new(dir.path()).get(&id).unwrap();
    let blocker = dir.path().join("conversations").join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocker).unwrap();
    let result = app.with(|c| c.send(request(Some(&id), "try again"), None));
    assert_eq!(result, Err("error.chat.saveFailed".into()));
    let rejected = app.open(&id).unwrap();
    assert_eq!(rejected.messages, before.messages);
    assert_eq!(rejected.choice, before.choice);
    assert_eq!(rejected.updated_at, before.updated_at);
    assert_eq!(Store::new(dir.path()).get(&id).unwrap().messages, saved_before.messages);
    assert!(events.lock().unwrap().iter().any(|e| matches!(e, AppEvent::SaveFailed { .. })));
    app.with(|c| assert!(!c.transfers.contains_key(&id)));
    std::fs::remove_dir(&blocker).unwrap();
    // A retained idle session observes dispatch only after the retried intent has reached disk.
    let observed = Arc::new(Mutex::new(0));
    let output = observed.clone();
    let root = dir.path().to_path_buf();
    let key = id.clone();
    let live_id = seed(
        &app,
        Session::from_fn(move |cmd| {
            if matches!(cmd, SessionCommand::Send(_)) {
                let saved = Store::new(&root).get(&key).unwrap();
                assert_eq!(saved.messages.iter().filter(|m| m.text == "try again").count(), 1);
                *output.lock().unwrap() += 1;
            }
        }),
        &[],
    );
    app.with(|c| {
        let mut live = c.live.remove(&live_id).unwrap();
        live.model = Some(crate::router::default_model(Vendor::Claude, None, &[]));
        live.cli = c.opts.clis[&Vendor::Claude].clone();
        live.cwd = c.cwd_of(&c.convs[&id]).unwrap();
        let settings = c.store.settings();
        live.permissions = settings.permissions();
        live.runs = live.permissions.for_claude(c.claude_flags_for(&live.cli));
        live.tuning = settings.tuning();
        live.served = true;
        c.live.insert(id.clone(), live);
        assert_eq!(c.send(request(Some(&id), "try again"), None), Ok(id.clone()));
        assert_eq!(c.convs[&id].messages.iter().filter(|m| m.text == "try again").count(), 1);
    });
    assert_eq!(*observed.lock().unwrap(), 1);
    app.shutdown();
}

#[tokio::test]
async fn send_admission_first_message_failure_keeps_the_same_empty_draft_for_retry() {
    for draft in [false, true] {
        let dir = crate::workspace::TestRoot::new();
        let (app, _) = engine(&dir);
        let prior = app.with(|c| {
            available(c);
            draft.then(|| {
                let id = c.blank(Area::Chat, None);
                c.draft = Some(id.clone());
                id
            })
        });
        let conversations = dir.path().join("conversations");
        std::fs::remove_dir_all(&conversations).unwrap();
        std::fs::write(&conversations, "block new records").unwrap();
        app.with(|c| {
            assert_eq!(c.send(request(None, "first"), None), Err("error.chat.saveFailed".into()));
            let id = c.draft.as_ref().expect("retry keeps this empty draft");
            if let Some(prior) = &prior {
                assert_eq!(id, prior);
            }
            assert!(c.convs[id].messages.is_empty());
            assert!(c.transfers.is_empty());
            assert_eq!(c.send(request(None, "first"), None), Err("error.chat.saveFailed".into()));
            assert_eq!(c.convs.len(), 1);
        });
        std::fs::remove_file(&conversations).unwrap();
        std::fs::create_dir(&conversations).unwrap();
        app.shutdown();
    }
}

#[tokio::test]
async fn send_admission_queue_failure_sends_nothing_and_retry_is_saved_before_steering() {
    for stopped in [false, true] {
        let dir = crate::workspace::TestRoot::new();
        let (app, _) = engine(&dir);
        let observed = Arc::new(Mutex::new(Vec::new()));
        let output = observed.clone();
        let root = dir.path().to_path_buf();
        let id = seed(
            &app,
            Session::from_fn(move |cmd| {
                if let SessionCommand::Steer { turn, tag } = cmd {
                    let saved = Store::new(&root)
                        .list()
                        .into_iter()
                        .filter_map(|c| Store::new(&root).get(&c.id))
                        .find(|c| c.queued.iter().any(|q| q.id == tag))
                        .expect("steering is saved first");
                    assert_eq!(saved.queued.iter().filter(|q| q.text == turn.text).count(), 1);
                    output.lock().unwrap().push(tag);
                }
            }),
            &[("running", &[])],
        );
        app.with(|c| {
            available(c);
            c.convs.get_mut(&id).unwrap().stopped = stopped;
            c.save(&id);
        });
        let before = app.open(&id).unwrap();
        let blocker = dir.path().join("conversations").join(format!("{id}.json.tmp"));
        std::fs::create_dir(&blocker).unwrap();
        app.with(|c| assert_eq!(c.send(request(Some(&id), "queued"), None), Err("error.chat.saveFailed".into())));
        let rejected = app.open(&id).unwrap();
        assert!(rejected.queued.is_empty());
        assert_eq!(rejected.send_after_stop, before.send_after_stop);
        assert_eq!(rejected.updated_at, before.updated_at);
        assert!(observed.lock().unwrap().is_empty());
        std::fs::remove_dir(&blocker).unwrap();
        app.with(|c| {
            assert_eq!(c.send(request(Some(&id), "queued"), None), Ok(id.clone()));
            assert!(observed.lock().unwrap().is_empty(), "the message stays editable until the step ends");
            c.hand_over_at_step(&id);
        });
        let saved = Store::new(dir.path()).get(&id).unwrap();
        assert_eq!(saved.queued.len(), 1);
        assert_eq!(observed.lock().unwrap().len(), usize::from(!stopped));
        if stopped {
            assert_eq!(app.open(&id).unwrap().send_after_stop.as_deref(), Some(saved.queued[0].id.as_str()));
        }
        app.shutdown();
    }
}

async fn edited_queue_waits_for_durable_save(mode: &str) {
    let dir = crate::workspace::TestRoot::new();
    let (app, _) = engine(&dir);
    let commands = Arc::new(Mutex::new(Vec::new()));
    let output = commands.clone();
    let root = dir.path().to_path_buf();
    let id = seed(
        &app,
        Session::from_fn(move |cmd| match cmd {
            SessionCommand::Send(turn) | SessionCommand::Steer { turn, .. } => {
                let store = Store::new(&root);
                assert!(
                    store
                        .list()
                        .iter()
                        .filter_map(|c| store.get(&c.id))
                        .any(|c| c.messages.iter().any(|m| m.text == "edited words") || c.queued.iter().any(|q| q.text == "edited words")),
                    "edited intent is saved before native input"
                );
                output.lock().unwrap().push(turn.text);
            }
            _ => {}
        }),
        &[("old", &[])],
    );
    app.with(|c| {
        available(c);
        if mode != "steer" && mode != "step" {
            let cwd = c.cwd_of(&c.convs[&id]).unwrap();
            let settings = c.store.settings();
            let cli = c.opts.clis[&Vendor::Claude].clone();
            let runs = settings.permissions().for_claude(c.claude_flags_for(&cli));
            let live = c.live.get_mut(&id).unwrap();
            live.turn = None;
            live.model = Some(crate::router::default_model(Vendor::Claude, None, &[]));
            live.cli = cli;
            live.cwd = cwd;
            live.permissions = settings.permissions();
            live.runs = runs;
            live.tuning = settings.tuning();
            live.served = true;
            c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().status = Some(Status::Done);
        }
        let item = QueuedTurn {
            id: "queued".into(),
            text: "saved words".into(),
            images: None,
            attachments: None,
            choice: request(None, "").choice,
            at: 1,
            sending: false,
            wake: None,
        };
        c.convs.get_mut(&id).unwrap().queued.push(item);
        c.save(&id);
    });
    let before = app.open(&id).unwrap();
    let blocker = dir.path().join("conversations").join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocker).unwrap();
    app.with(|c| {
        c.edit_queued(&id, "queued", "edited words").unwrap();
        if mode == "step" {
            c.hand_over_at_step(&id);
        } else if mode == "drain" {
            c.drain(&id);
        } else {
            assert_eq!(c.send_queued(&id, "queued"), Err("error.chat.saveFailed".into()));
        }
    });
    let rejected = app.open(&id).unwrap();
    assert_eq!(rejected.messages, before.messages, "no new turn is admitted");
    assert_eq!(rejected.queued.len(), 1, "the queued intent stays in memory");
    assert_eq!(rejected.queued[0].text, "edited words", "the rejected write never discards the edit");
    assert!(!rejected.queued[0].sending);
    assert!(commands.lock().unwrap().is_empty(), "nothing reaches the CLI");
    assert_eq!(Store::new(dir.path()).get(&id).unwrap().queued[0].text, "saved words");
    std::fs::remove_dir(&blocker).unwrap();
    app.with(|c| {
        if mode == "step" {
            c.hand_over_at_step(&id);
        } else if mode == "drain" {
            c.drain(&id);
        } else {
            c.send_queued(&id, "queued").unwrap();
        }
    });
    assert_eq!(commands.lock().unwrap().len(), 1, "the successful retry reaches the CLI once");
    let saved = Store::new(dir.path()).get(&id).unwrap();
    if mode == "steer" || mode == "step" {
        assert_eq!(saved.queued[0].text, "edited words");
    } else {
        assert_eq!(saved.messages.iter().filter(|m| m.text == "edited words").count(), 1);
        assert!(saved.queued.is_empty());
    }
    app.shutdown();
}

#[tokio::test]
async fn send_admission_drain_does_not_dispatch_an_unsaved_queue_edit() {
    edited_queue_waits_for_durable_save("drain").await;
}

#[tokio::test]
async fn send_admission_send_now_does_not_dispatch_an_unsaved_queue_edit() {
    edited_queue_waits_for_durable_save("send").await;
}

#[tokio::test]
async fn send_admission_send_now_does_not_steer_an_unsaved_queue_edit() {
    edited_queue_waits_for_durable_save("steer").await;
}

async fn rejected_archived_send_keeps_its_archive(active: bool) {
    let dir = crate::workspace::TestRoot::new();
    let (app, events) = engine(&dir);
    let id = seed(&app, Session::from_fn(|_| {}), &[("old", &[])]);
    app.with(|c| {
        available(c);
        if !active {
            c.live.remove(&id);
        }
        let conv = c.convs.get_mut(&id).unwrap();
        conv.archived_at = Some(10);
        conv.updated_at = 20;
        let reply = conv.messages.last_mut().unwrap();
        if !active {
            reply.status = Some(Status::Done);
        }
        reply.follow = Some(
            serde_json::from_value(serde_json::json!({
                "repo": "example/project", "number": 1, "url": "https://example.invalid/work/1", "state": "merged", "since": 1
            }))
            .unwrap(),
        );
        c.save(&id);
    });
    let before = app.open(&id).unwrap();
    events.lock().unwrap().clear();
    let blocker = dir.path().join("conversations").join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocker).unwrap();
    app.with(|c| assert_eq!(c.send(request(Some(&id), "keep archived on refusal"), None), Err("error.chat.saveFailed".into())));
    let rejected = app.open(&id).unwrap();
    assert_eq!(rejected.archived_at, before.archived_at);
    assert_eq!(rejected.updated_at, before.updated_at);
    assert_eq!(rejected.messages, before.messages, "a refused send does not keep the archived follow card");
    assert!(rejected.queued.is_empty());
    assert!(!events.lock().unwrap().iter().any(|e| matches!(e, AppEvent::List)), "no list change is announced before acceptance");
    std::fs::remove_dir(&blocker).unwrap();
    app.shutdown();
}

#[tokio::test]
async fn send_admission_archived_idle_refusal_preserves_archive_and_follow() {
    rejected_archived_send_keeps_its_archive(false).await;
}

#[tokio::test]
async fn send_admission_archived_queue_refusal_preserves_archive_and_follow() {
    rejected_archived_send_keeps_its_archive(true).await;
}

#[tokio::test]
async fn send_admission_rejected_queue_keeps_its_draft_route_count() {
    let dir = crate::workspace::TestRoot::new();
    let (app, _) = engine(&dir);
    let id = seed(&app, Session::from_fn(|_| {}), &[("old", &[])]);
    app.with(|c| {
        available(c);
        c.draft_routes.insert(id.clone(), 2);
        c.save(&id);
    });
    let blocker = dir.path().join("conversations").join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocker).unwrap();
    app.with(|c| {
        assert_eq!(c.send(request(Some(&id), "rejected queue"), None), Err("error.chat.saveFailed".into()));
        assert_eq!(c.draft_routes.get(&id), Some(&2));
    });
    std::fs::remove_dir(&blocker).unwrap();
    app.with(|c| {
        assert_eq!(c.send(request(Some(&id), "accepted queue"), None), Ok(id.clone()));
        assert!(!c.draft_routes.contains_key(&id));
    });
    app.shutdown();
}

#[tokio::test]
async fn send_admission_research_refusal_does_not_change_mode_grants_or_other_sessions() {
    let dir = crate::workspace::TestRoot::new();
    let (app, _) = engine(&dir);
    let commands = Arc::new(Mutex::new(0));
    let output = commands.clone();
    let other = seed(
        &app,
        Session::from_fn(move |cmd| {
            if matches!(cmd, SessionCommand::Interrupt | SessionCommand::Close | SessionCommand::Send(_)) {
                *output.lock().unwrap() += 1;
            }
        }),
        &[("other", &[])],
    );
    let id = app.with(|c| {
        available(c);
        c.store.save_settings(&Settings { permissions: Some(Permissions::FullAccess), ..Settings::default() }).unwrap();
        c.convs.get_mut(&other).unwrap().allow_always = Some(vec!["Read".into()]);
        c.save(&other);
        let id = c.blank(Area::Chat, None);
        c.save(&id);
        id
    });
    let blocker = dir.path().join("conversations").join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocker).unwrap();
    app.with(|c| {
        assert_eq!(c.send(request(Some(&id), "/research compare the choices"), None), Err("error.chat.saveFailed".into()));
        assert_eq!(c.store.settings().permissions(), Permissions::FullAccess);
        assert!(c.live.contains_key(&other));
        assert_eq!(c.convs[&other].allow_always.as_deref(), Some(["Read".to_string()].as_slice()));
        assert!(c.convs[&id].messages.is_empty());
    });
    assert_eq!(*commands.lock().unwrap(), 0, "a refused research request leaves the other AI working");
    std::fs::remove_dir(&blocker).unwrap();
    app.shutdown();
}

#[tokio::test]
async fn send_admission_research_settings_failure_keeps_the_durable_intent_accepted_once() {
    let dir = crate::workspace::TestRoot::new();
    let (app, _) = engine(&dir);
    let id = app.with(|c| {
        available(c);
        c.opts.spawn = Arc::new(|_, _, _| panic!("a settings failure starts no CLI"));
        let id = c.blank(Area::Chat, None);
        c.save(&id);
        id
    });
    let blocker = dir.path().join("settings.json.tmp");
    std::fs::create_dir(&blocker).unwrap();
    app.with(|c| {
        assert_eq!(
            c.send(request(Some(&id), "/research compare the choices"), None),
            Ok(id.clone()),
            "the durable send is already accepted"
        );
        assert_eq!(c.convs[&id].messages.iter().filter(|m| m.role == Role::User).count(), 1);
        let reply = c.convs[&id].messages.last().unwrap();
        assert_eq!(reply.status, Some(Status::Error));
        assert_eq!(reply.error.as_deref(), Some("error.settings.saveFailed"));
        assert!(c.live.is_empty());
    });
    let saved = Store::new(dir.path()).get(&id).unwrap();
    assert_eq!(saved.messages.iter().filter(|m| m.role == Role::User).count(), 1);
    assert_eq!(saved.messages.last().unwrap().error.as_deref(), Some("error.settings.saveFailed"));
    std::fs::remove_dir(&blocker).unwrap();
    app.shutdown();
}

#[tokio::test(start_paused = true)]
async fn send_admission_settings_failure_halts_keep_going_without_retrying_or_dropping_human_queue() {
    settings_failure_halts_keep_going(false).await;
}

#[tokio::test(start_paused = true)]
async fn send_admission_queued_settings_failure_halts_keep_going_without_retrying_or_dropping_human_queue() {
    settings_failure_halts_keep_going(true).await;
}

async fn settings_failure_halts_keep_going(queued: bool) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = crate::workspace::TestRoot::new();
    let (app, _) = engine(&dir);
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempted = attempts.clone();
    let id = app.with(|c| {
        available(c);
        c.opts.spawn = Arc::new(move |_, _, _| {
            attempted.fetch_add(1, Ordering::SeqCst);
            Err(std::io::Error::other("no process in this test"))
        });
        let id = c.blank(Area::Chat, None);
        let conv = c.convs.get_mut(&id).unwrap();
        conv.keep_going = Some(KeepGoing::default());
        conv.queued.push(QueuedTurn {
            id: "human-waiting".into(),
            text: "next human request".into(),
            images: None,
            attachments: None,
            choice: request(None, "").choice,
            at: 1,
            sending: false,
            wake: None,
        });
        if queued {
            let mut research = conv.queued[0].clone();
            research.id = "research-waiting".into();
            research.text = "/research compare the choices".into();
            conv.queued.insert(0, research);
        }
        c.save(&id);
        id
    });
    let blocker = dir.path().join("settings.json.tmp");
    std::fs::create_dir(&blocker).unwrap();
    app.with(|c| {
        if queued {
            let _ = c.send_queued(&id, "research-waiting");
        } else {
            assert_eq!(c.send(request(Some(&id), "/research compare the choices"), None), Ok(id.clone()));
        }
        let conv = &c.convs[&id];
        assert!(conv.keep_going.is_none(), "the mode failure schedules no continuation");
        assert_eq!(conv.queued.len(), 1);
        assert_eq!(conv.queued[0].id, "human-waiting");
        let reply = conv.messages.last().unwrap();
        assert_eq!(reply.status, Some(Status::Error));
        assert_eq!(reply.error.as_deref(), Some("error.settings.saveFailed"));
        assert_eq!(reply.halt.as_deref(), Some("reason.keepGoing.failed"));
    });
    tokio::time::advance(Duration::from_secs(3600)).await;
    tokio::task::yield_now().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 0, "no delayed retry starts a CLI");
    let saved = Store::new(dir.path()).get(&id).unwrap();
    assert!(saved.keep_going.is_none());
    assert_eq!(saved.queued[0].id, "human-waiting");
    assert_eq!(saved.messages.iter().filter(|m| m.role == Role::User).count(), 1);
    assert_eq!(saved.messages.last().unwrap().error.as_deref(), Some("error.settings.saveFailed"));
    std::fs::remove_dir(&blocker).unwrap();
    app.shutdown();
}

#[tokio::test]
async fn send_admission_step_end_does_not_steer_an_unsaved_queue_edit() {
    edited_queue_waits_for_durable_save("step").await;
}
