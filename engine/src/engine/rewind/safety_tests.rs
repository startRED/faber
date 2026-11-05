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
    seed_at(engine, None)
}

fn seed_at(engine: &Engine, folder: Option<&Path>) -> (String, PathBuf, Arc<Mutex<History>>, PointInfo) {
    let (id, root, history) = engine.with(|core| {
        let id = core.blank(Area::Chat, None);
        let conversation = core.convs.get_mut(&id).unwrap();
        conversation.working_folder = folder.map(|folder| folder.to_string_lossy().into_owned());
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
async fn deleted_chat_purges_its_points_before_returning_and_after_restart() {
    let (data, engine) = engine(Arc::new(|_| {}));
    let folder = tempfile::tempdir().unwrap();
    let (id, root, history, _) = seed_at(&engine, Some(folder.path()));
    let sentinel = root.join("personal-work.txt");
    std::fs::write(&sentinel, "Keep my work").unwrap();
    engine.with(|core| core.remove(&id)).unwrap();
    assert!(history.lock().unwrap().list(&id).is_empty(), "deleted chats must release their restore points immediately");
    assert!(History::open(data.path(), now()).unwrap().list(&id).is_empty(), "deleted points must not return after restart");
    assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "Keep my work");
    engine.shutdown();
}

#[tokio::test]
async fn failed_chat_deletion_preserves_its_restore_points() {
    let (data, engine) = engine(Arc::new(|_| {}));
    let (id, _, history, point) = seed(&engine);
    let record = data.path().join("conversations").join(format!("{id}.json"));
    let preserved = record.with_extension("preserved");
    std::fs::rename(&record, &preserved).unwrap();
    std::fs::create_dir(&record).unwrap();
    assert_eq!(engine.with(|core| core.remove(&id)), Err("error.chat.deleteFailed".into()));
    assert_eq!(history.lock().unwrap().list(&id)[0].id, point.id);
    assert_eq!(History::open(data.path(), now()).unwrap().list(&id)[0].id, point.id);
    std::fs::remove_dir(&record).unwrap();
    std::fs::rename(preserved, record).unwrap();
    engine.shutdown();
}

#[tokio::test]
async fn deleted_chat_points_cannot_be_resurrected_by_a_prepared_seal() {
    let (data, engine) = engine(Arc::new(|_| {}));
    let (id, root, history, _) = seed(&engine);
    let publication = {
        let mut history = history.lock().unwrap();
        let scan = history.scan_seal(&mut Listing(false), &root).unwrap();
        history.prepare_seal(scan)
    };
    engine.with(|core| core.remove(&id)).unwrap();
    let mut history = history.lock().unwrap();
    history.publish_seal(&publication);
    history.persist_seal(publication, now()).unwrap();
    assert!(history.list(&id).is_empty(), "a late seal cannot recreate deleted history");
    assert!(History::open(data.path(), now()).unwrap().list(&id).is_empty());
    drop(history);
    engine.shutdown();
}

#[tokio::test]
async fn deleted_chat_reports_history_cleanup_failure_and_startup_retries() {
    let errors = Arc::new(Mutex::new(Vec::new()));
    let observed = errors.clone();
    let (data, engine) = engine(Arc::new(move |event| {
        if let AppEvent::SaveFailed { key } = event {
            observed.lock().unwrap().push(key);
        }
    }));
    let (id, _, history, point) = seed(&engine);
    let record = std::fs::read_dir(data.path().join("rewind"))
        .unwrap()
        .flatten()
        .map(|folder| folder.path().join("points").join(format!("{}.json", point.id)))
        .find(|path| path.is_file())
        .unwrap();
    let preserved = data.path().join("preserved-point.json");
    std::fs::rename(&record, &preserved).unwrap();
    std::fs::create_dir(&record).unwrap();
    engine.with(|core| core.remove(&id)).unwrap();
    assert!(engine.open(&id).is_none(), "the deleted conversation must not be recreated after cleanup fails");
    assert!(errors.lock().unwrap().iter().any(|key| key == "error.chat.historyKept"));
    std::fs::remove_dir(&record).unwrap();
    std::fs::rename(preserved, record).unwrap();
    engine.recover_rewind();
    assert!(history.lock().unwrap().list(&id).is_empty());
    assert!(History::open(data.path(), now()).unwrap().list(&id).is_empty());
    engine.shutdown();
}

#[tokio::test]
async fn dismissing_a_damaged_copy_keeps_the_original_chats_history() {
    let (data, engine) = engine(Arc::new(|_| {}));
    let (id, _, history, point) = seed(&engine);
    engine.with(|core| {
        core.convs.remove(&id);
    });
    std::fs::write(data.path().join("conversations").join(format!("{id}.json")), b"damaged record").unwrap();
    engine.recover_rewind();
    assert_eq!(history.lock().unwrap().list(&id)[0].id, point.id, "unreadable records are not deleted chats");
    engine.remove_damaged_conversation(&id).unwrap();
    assert_eq!(history.lock().unwrap().list(&id)[0].id, point.id);
    assert!(data.path().join("conversations").join(format!("{id}.json")).is_file());
    engine.shutdown();
}

#[tokio::test]
async fn deleting_another_chat_waits_for_an_unpublished_seal() {
    struct Named;
    impl crate::project::git::Executor for Named {
        fn execute(
            &mut self,
            call: &crate::project::git::Invocation,
        ) -> Result<crate::project::git::Output, crate::project::git::GitError> {
            let ignored = call.args.iter().any(|arg| arg == "--ignored");
            Ok(crate::project::git::Output { success: true, stdout: if ignored { vec![] } else { b"saved.txt\0".to_vec() } })
        }
    }
    let (_data, engine) = engine(Arc::new(|_| {}));
    let folder = tempfile::tempdir().unwrap();
    let (survivor, root, history, _) = seed_at(&engine, Some(folder.path()));
    let (deleted, _, _, _) = seed(&engine);
    std::fs::write(root.join("saved.txt"), "A new version awaiting publication\n").unwrap();
    engine.with(|core| core.rewind.worker.store(true, Ordering::Release));
    let publication = {
        let mut history = history.lock().unwrap();
        let scan = history.scan_seal(&mut Named, &root).unwrap();
        history.prepare_seal(scan)
    };
    assert_eq!(
        engine.with(|core| core.remove(&deleted)),
        Err("error.rewind.busy".into()),
        "deletion cannot collect another folder's unpublished objects"
    );
    {
        let mut history = history.lock().unwrap();
        history.publish_seal(&publication);
        history.persist_seal(publication, now()).unwrap();
    }
    engine.with(|core| {
        core.release_rewind_worker();
        core.remove(&deleted).unwrap();
    });
    assert!(
        history
            .lock()
            .unwrap()
            .review_patch(&survivor, "question", "saved.txt", 0)
            .unwrap()
            .lines
            .iter()
            .any(|line| line == "+A new version awaiting publication")
    );
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

// Windows can deny renaming a record directory while the store's readers are open.
#[cfg(unix)]
#[tokio::test]
async fn missing_record_directory_never_counts_as_deleted_chats() {
    let (data, engine) = engine(Arc::new(|_| {}));
    let (id, _, history, point) = seed(&engine);
    engine.with(|core| {
        core.convs.remove(&id);
    });
    let records = data.path().join("conversations");
    let preserved = data.path().join("preserved-conversations");
    std::fs::rename(&records, &preserved).unwrap();
    engine.recover_rewind();
    assert_eq!(
        history.lock().unwrap().list(&id)[0].id,
        point.id,
        "an unavailable parent directory is not proof that its conversations were deleted"
    );
    std::fs::rename(preserved, records).unwrap();
    engine.shutdown();
}
