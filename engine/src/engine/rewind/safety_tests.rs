use super::*;

struct Listing(bool);
impl crate::project::git::Executor for Listing {
    fn execute(&mut self, _: &crate::project::git::Invocation) -> Result<crate::project::git::Output, crate::project::git::GitError> {
        if self.0 { Err(crate::project::git::GitError::Command) } else { Ok(crate::project::git::Output { success: true, stdout: vec![] }) }
    }
}

fn engine(events: Emit) -> (crate::workspace::TestRoot, Engine) {
    let data = crate::workspace::TestRoot::new();
    let engine = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            #[cfg(windows)]
            chat_dir: data.work().into(),
            clis: Default::default(),
            spawn: Arc::new(|_, _, _| panic!("history must not start a process")),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        events,
    )
    .unwrap();
    (data, engine)
}

#[tokio::test]
async fn default_chat_capture_reports_coverage_without_a_selected_folder() {
    let (_data, engine) = engine(Arc::new(|_| {}));
    engine.with(|core| {
        let id = core.blank(Area::Chat, None);
        let conversation = core.convs.get_mut(&id).unwrap();
        assert!(conversation.working_folder.is_none());
        conversation.messages.push(Message::new("question".into(), Role::User, "Change a file".into(), 1));
        conversation.messages.push(Message::new("reply".into(), Role::Assistant, String::new(), 2));
        core.git = None;
        let pick = Pick { vendor: Vendor::Claude, model: None, effort: "medium".into(), reason: String::new(), pace: None };
        assert!(!core.capture_rewind(&id, "reply", &pick, String::new(), vec![], true));
        assert!(core.convs[&id].messages[0].rewind.as_ref().is_some_and(|point| point.coverage.unavailable));
    });
    engine.shutdown();
}

#[tokio::test]
async fn default_chat_steering_reports_unavailable_coverage_without_dropping_the_input() {
    let (_data, engine) = engine(Arc::new(|_| {}));
    engine.with(|core| {
        let id = core.blank(Area::Chat, None);
        core.git = None;
        let mut item: QueuedTurn = serde_json::from_value(serde_json::json!({
            "id":"queued","text":"Keep this input","choice":{"vendor":"claude","effort":"medium"},"at":1
        }))
        .unwrap();
        assert!(!core.prepare_rewind_steer(&id, &mut item));
        assert_eq!(item.text, "Keep this input");
        assert!(core.rewind.steers.get("queued").is_some_and(|point| point.coverage.unavailable));
    });
    engine.shutdown();
}

fn seed(engine: &Engine) -> (String, PathBuf, Arc<Mutex<History>>, PointInfo) {
    let (id, root, history) = engine.with(|core| {
        let id = core.blank(Area::Chat, None);
        let conversation = core.convs.get_mut(&id).unwrap();
        conversation.messages.push(Message::new("question".into(), Role::User, "Change a file".into(), 1));
        conversation.messages.push(Message::new("reply".into(), Role::Assistant, String::new(), 2));
        core.save(&id);
        let root = core.cwd_of(&core.convs[&id]).unwrap();
        (id, root, core.rewind.history.clone().unwrap())
    });
    let conversation = engine.open(&id).unwrap();
    let info = history.lock().unwrap().capture(&mut Listing(false), &root, &conversation, "question", "reply", now(), false).unwrap();
    engine.with(|core| core.mark_rewind(&id, "reply", Ok(info.clone())));
    (id, root, history, info)
}

async fn final_scan(failed: bool, unloaded: bool) {
    let (sent, observed) = tokio::sync::oneshot::channel();
    let sent = Mutex::new(Some(sent));
    let (_data, engine) = engine(Arc::new(move |event| {
        if let AppEvent::Message { message, .. } = event
            && message.id == "question"
            && message.rewind.as_ref().is_some_and(|point| if failed { point.coverage.unavailable } else { point.sealed })
            && let Some(sent) = sent.lock().unwrap().take()
        {
            let _ = sent.send(());
        }
    }));
    let (id, _, history, _) = seed(&engine);
    engine.set_git(Arc::new(move || Some(Box::new(Listing(failed)))));
    engine.with(|core| {
        core.try_seal_rewind(&id);
        if unloaded {
            core.convs.remove(&id);
        }
    });
    tokio::time::timeout(Duration::from_secs(30), observed).await.expect("the final scan must publish its saved or failed state").unwrap();
    engine.with(|core| {
        if unloaded {
            assert!(!core.convs.contains_key(&id), "updating a saved marker must not retain the unloaded conversation");
        }
        let stored = core.store.get(&id).unwrap();
        let point = stored.messages[0].rewind.as_ref().unwrap();
        assert_eq!(point.sealed, !failed);
        assert_eq!(point.coverage.unavailable, failed);
    });
    let point = &engine.rewind_points(&id).unwrap()[0];
    assert_eq!(point.sealed, !failed);
    assert_eq!(point.coverage.unavailable, failed);
    if !failed {
        assert!(history.lock().unwrap().list(&id)[0].sealed);
    }
    engine.shutdown();
}

#[tokio::test]
async fn default_chat_final_scan_publishes_a_durably_saved_point() {
    final_scan(false, false).await;
}

#[tokio::test]
async fn failed_final_scan_updates_an_unloaded_conversation_and_survives_reopen() {
    final_scan(true, true).await;
}

#[tokio::test]
async fn failed_point_save_updates_an_unloaded_conversations_saved_marker() {
    let (data, engine) = engine(Arc::new(|_| {}));
    let (id, root, history, _) = seed(&engine);
    let mut history = history.lock().unwrap();
    let scan = history.scan_seal(&mut Listing(false), &root).unwrap();
    let publication = history.prepare_seal(scan);
    let mut infos = publication.infos();
    history.publish_seal(&publication);
    let mut files = vec![];
    for folder in std::fs::read_dir(data.path().join("rewind")).unwrap().flatten() {
        let points = folder.path().join("points");
        if points.is_dir() {
            files.extend(std::fs::read_dir(points).unwrap().flatten().map(|entry| entry.path()));
        }
    }
    assert_eq!(files.len(), 1);
    std::fs::rename(&files[0], data.path().join("preserved-point.json")).unwrap();
    std::fs::create_dir(&files[0]).unwrap();
    assert!(history.persist_seal(publication, 2).is_err());
    history.refresh_infos(&mut infos);
    drop(history);
    engine.with(|core| {
        core.convs.remove(&id);
        core.publish_rewind_markers(infos);
        assert!(!core.convs.contains_key(&id));
        assert!(core.store.get(&id).unwrap().messages[0].rewind.as_ref().unwrap().coverage.unavailable);
    });
    assert!(engine.rewind_points(&id).unwrap()[0].coverage.unavailable);
    engine.shutdown();
}

#[tokio::test]
async fn temporary_chat_never_retains_file_history_or_steering_markers() {
    let (_data, engine) = engine(Arc::new(|_| {}));
    engine.with(|core| {
        let id = core.blank(Area::Chat, None);
        let conversation = core.convs.get_mut(&id).unwrap();
        conversation.temporary = true;
        conversation.messages.push(Message::new("question".into(), Role::User, "Private request".into(), 1));
        conversation.messages.push(Message::new("reply".into(), Role::Assistant, String::new(), 2));
        core.git = None;
        let pick = Pick { vendor: Vendor::Claude, model: None, effort: "medium".into(), reason: String::new(), pace: None };
        assert!(!core.capture_rewind(&id, "reply", &pick, String::new(), vec![], true));
        assert!(core.convs[&id].messages[0].rewind.is_none(), "temporary chats must never get a durable-history marker");
        let mut queued: QueuedTurn = serde_json::from_value(serde_json::json!({
            "id":"temporary-input","text":"Still private","choice":{"vendor":"claude","effort":"medium"},"at":1
        }))
        .unwrap();
        assert!(!core.prepare_rewind_steer(&id, &mut queued));
        assert!(!core.rewind.steers.contains_key("temporary-input"));
        core.git = Some(Arc::new(|| panic!("temporary chats must never scan or seal private history")));
        core.try_seal_rewind(&id);
        assert!(!core.rewind.worker.load(Ordering::Acquire));
        assert!(core.rewind.history.as_ref().unwrap().lock().unwrap().list(&id).is_empty());
    });
    engine.shutdown();
}

#[tokio::test]
async fn a_failed_new_scan_preserves_older_durable_points_with_legacy_markers() {
    let (sent, observed) = tokio::sync::oneshot::channel();
    let sent = Mutex::new(Some(sent));
    let (_data, engine) = engine(Arc::new(move |event| {
        if let AppEvent::Message { message, .. } = event
            && message.id == "question-2"
            && message.rewind.as_ref().is_some_and(|point| point.coverage.unavailable)
            && let Some(sent) = sent.lock().unwrap().take()
        {
            let _ = sent.send(());
        }
    }));
    let (id, root, history, first) = seed(&engine);
    {
        let mut history = history.lock().unwrap();
        let scan = history.scan_seal(&mut Listing(false), &root).unwrap();
        history.seal(scan, now()).unwrap();
    }
    engine.with(|core| {
        let conversation = core.convs.get_mut(&id).unwrap();
        assert!(!conversation.messages[0].rewind.as_ref().unwrap().sealed, "older versions left this marker unsealed");
        conversation.messages.push(Message::new("question-2".into(), Role::User, "A later change".into(), now()));
        conversation.messages.push(Message::new("reply-2".into(), Role::Assistant, String::new(), now()));
        core.save(&id);
    });
    let conversation = engine.open(&id).unwrap();
    let second = history.lock().unwrap().capture(&mut Listing(false), &root, &conversation, "question-2", "reply-2", now(), false).unwrap();
    engine.set_git(Arc::new(|| Some(Box::new(Listing(true)))));
    engine.with(|core| {
        core.mark_rewind(&id, "reply-2", Ok(second.clone()));
        core.try_seal_rewind(&id);
    });
    tokio::time::timeout(Duration::from_secs(30), observed).await.expect("the new scan must report its failure").unwrap();
    let points = engine.rewind_points(&id).unwrap();
    let old = points.iter().find(|point| point.id == first.id).unwrap();
    assert!(old.sealed && !old.coverage.unavailable, "a new scan failure cannot invalidate an older durable point");
    assert!(points.iter().find(|point| point.id == second.id).unwrap().coverage.unavailable);
    engine.shutdown();
}

#[tokio::test]
async fn cleanup_failure_does_not_invalidate_a_durably_saved_point() {
    let (sent, observed) = tokio::sync::oneshot::channel();
    let sent = Mutex::new(Some(sent));
    let (data, engine) = engine(Arc::new(move |event| {
        if let AppEvent::Message { message, .. } = event
            && message.id == "question"
            && message.rewind.as_ref().is_some_and(|point| point.sealed || point.coverage.unavailable)
            && let Some(sent) = sent.lock().unwrap().take()
        {
            let _ = sent.send(());
        }
    }));
    let (id, _, history, _) = seed(&engine);
    let folder = std::fs::read_dir(data.path().join("rewind"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|folder| folder.join("journals").is_dir())
        .unwrap();
    struct CleanupFailure(PathBuf);
    impl crate::project::git::Executor for CleanupFailure {
        fn execute(&mut self, _: &crate::project::git::Invocation) -> Result<crate::project::git::Output, crate::project::git::GitError> {
            // A synthetic orphan with a valid object name but the wrong file type fails only during cleanup.
            std::fs::create_dir_all(self.0.join("objects").join("00").join("0".repeat(62))).unwrap();
            Ok(crate::project::git::Output { success: true, stdout: vec![] })
        }
    }
    engine.set_git(Arc::new(move || Some(Box::new(CleanupFailure(folder.clone())))));
    engine.with(|core| core.try_seal_rewind(&id));
    tokio::time::timeout(Duration::from_secs(30), observed).await.expect("sealing must report its result").unwrap();
    assert!(history.lock().unwrap().list(&id)[0].sealed, "the point itself was durably saved before cleanup failed");
    let points = engine.rewind_points(&id).unwrap();
    assert!(points[0].sealed && !points[0].coverage.unavailable, "cleanup failure must not report saved versions as missing");
    let (root, conversation) = engine.with(|core| (core.cwd_of(&core.convs[&id]).unwrap(), core.convs[&id].clone()));
    assert!(
        history.lock().unwrap().capture(&mut Listing(false), &root, &conversation, "question", "next-reply", now(), false).is_err(),
        "cleanup failure must keep history admission closed"
    );
    engine.shutdown();
}
