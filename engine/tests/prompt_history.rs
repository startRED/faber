use faber_engine::model::{Area, Conversation};
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use serde_json::{Value, json};

fn engine(directory: &std::path::Path) -> (tokio::runtime::Runtime, Engine) {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let engine = Engine::new(
        EngineOptions {
            data_dir: directory.into(),
            clis: Default::default(),
            spawn: std::sync::Arc::new(|_, _, _| panic!("prompt history must never start a CLI")),
            windows: false,
            claude_env_billed: false,
            max_warm: 0,
            idle: std::time::Duration::from_secs(120),
            runtime: runtime.handle().clone(),
            ask: None,
        },
        std::sync::Arc::new(|_| {}),
    );
    (runtime, engine)
}

fn save(store: &Store, id: &str, area: Area, project: Option<&str>, messages: Value) {
    let updated = messages.as_array().unwrap().iter().filter_map(|message| message["at"].as_u64()).max().unwrap_or(0);
    save_at(store, id, area, project, updated, messages);
}

fn save_at(store: &Store, id: &str, area: Area, project: Option<&str>, updated: u64, messages: Value) {
    let conversation: Conversation = serde_json::from_value(json!({
        "id": id, "title": id, "area": area, "projectId": project,
        "createdAt": 1, "updatedAt": updated, "messages": messages,
    }))
    .unwrap();
    store.save(&conversation);
}

fn prompt(text: &str, at: u64) -> Value {
    json!({"id": format!("message-{at}"), "role": "user", "text": text, "at": at})
}

fn wait_for_sidebar(engine: &Engine) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !engine.search_messages("", Area::Chat).unwrap().ready {
        assert!(std::time::Instant::now() < deadline, "sidebar index did not finish");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn prompt_history_orders_own_messages_before_newer_messages_from_the_same_area_and_project() {
    for area in [Area::Chat, Area::Project] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let project = (area == Area::Project).then_some("project-one");
        save(&store, "open", area, project, json!([prompt("own older", 1), prompt("own newest", 2)]));
        save(&store, "recent", area, project, json!([prompt("other newest", 90), prompt("other older", 20)]));
        save(&store, "middle", area, project, json!([prompt("other middle", 50)]));
        save(
            &store,
            "wrong-area",
            if area == Area::Chat { Area::Project } else { Area::Chat },
            project,
            json!([prompt("wrong area", 100)]),
        );
        if area == Area::Project {
            save(&store, "wrong-project", area, Some("project-two"), json!([prompt("wrong project", 100)]));
        }
        let (_runtime, engine) = engine(dir.path());
        wait_for_sidebar(&engine);
        let read = |id| engine.prompt_history(id, area, project).unwrap();
        assert_eq!(read(Some("open")), ["own newest", "own older", "other newest", "other older", "other middle"]);
        assert_eq!(read(None), ["other newest", "other older", "other middle", "own newest", "own older"]);
    }
}

#[test]
fn prompt_history_uses_last_activity_and_streams_past_replies_and_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    save_at(&store, "active", Area::Chat, None, 100, json!([prompt("older prompt in recently active conversation", 1)]));
    save_at(&store, "older", Area::Chat, None, 90, json!([prompt("newer prompt in older conversation", 50)]));
    let (_runtime, engine) = engine(dir.path());
    wait_for_sidebar(&engine);
    // Initialize the existing metadata before changing fields that only the prompt reader needs to skip.
    assert_eq!(
        engine.prompt_history(None, Area::Chat, None).unwrap(),
        ["older prompt in recently active conversation", "newer prompt in older conversation"]
    );
    // Replies and attachment payloads have no role in recall; their shapes need not be deserialized.
    let path = dir.path().join("conversations/active.json");
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    saved["messages"].as_array_mut().unwrap().push(json!({"role": "assistant", "text": {"ignored": "reply"}}));
    saved["messages"][0]["attachments"] = json!({"ignored": "attachment"});
    std::fs::write(path, serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(
        engine.prompt_history(None, Area::Chat, None).unwrap(),
        ["older prompt in recently active conversation", "newer prompt in older conversation",]
    );
}

#[test]
fn prompt_history_bounds_file_and_payload_work_and_discards_cancelled_requests() {
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    save_at(&store, "oversized", Area::Chat, None, 100, json!([prompt("must not read this oversized file", 1)]));
    let path = dir.path().join("conversations/oversized.json");
    // Valid trailing whitespace makes the saved file oversized, independent of its small prompt.
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        let padding = vec![b' '; 1024 * 1024];
        for _ in 0..17 {
            file.write_all(&padding).unwrap();
        }
    }
    save_at(&store, "valid", Area::Chat, None, 10, json!([prompt("kept", 1)]));
    let (_runtime, reader) = engine(dir.path());
    let history = reader.prompt_history(None, Area::Chat, None).unwrap();
    assert_eq!(history, ["kept"]);
    assert!(reader.prompt_history_cancellable(None, Area::Chat, None, &AtomicBool::new(true)).unwrap().is_empty());
    drop(reader);
    save_at(
        &store,
        "large-prompts",
        Area::Chat,
        None,
        200,
        json!([prompt(&"a".repeat(1024 * 1024), 1), prompt(&"b".repeat(1024 * 1024), 2), prompt(&"c".repeat(1024 * 1024), 3),]),
    );
    let (_runtime, reader) = engine(dir.path());
    let history = reader.prompt_history(None, Area::Chat, None).unwrap();
    assert_eq!(history.iter().map(String::len).sum::<usize>(), 2 * 1024 * 1024);
    assert!(history[0].starts_with('c'));
    assert!(history[1].starts_with('b'));
}

#[test]
fn prompt_history_deduplicates_before_the_cap_and_keeps_the_highest_ranked_copy() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    save(&store, "open", Area::Chat, None, json!([prompt("repeated", 1), prompt("repeated", 2)]));
    let mut messages: Vec<_> = (0..250).map(|i| prompt(&format!("other {i}"), i + 10)).collect();
    messages.extend((0..250).map(|i| prompt("repeated", i + 1000)));
    save(&store, "other", Area::Chat, None, json!(messages));
    let (_runtime, engine) = engine(dir.path());
    let history = engine.prompt_history(Some("open"), Area::Chat, None).unwrap();
    assert_eq!(history.len(), 200);
    assert_eq!(history[0], "repeated");
    assert_eq!(history[1], "other 249");
    assert_eq!(history[199], "other 51");
    assert_eq!(history.iter().filter(|text| *text == "repeated").count(), 1);
}

#[test]
fn prompt_history_excludes_fabers_messages_answers_side_questions_and_empty_text() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let mut wake = prompt("keep going", 2);
    wake["wake"] = json!({"why": "wake.keepGoing"});
    let mut answer = prompt("assistant answer", 3);
    answer["role"] = json!("assistant");
    save(&store, "open", Area::Chat, None, json!([prompt("  keep these spaces\n", 1), wake, answer, prompt("  ", 4)]));
    // Quick questions are deliberately separate from the conversation messages, even in an older saved file.
    let path = dir.path().join("conversations/open.json");
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    saved["asides"] = json!([{"question": "side question", "answer": "side answer"}]);
    std::fs::write(path, serde_json::to_vec(&saved).unwrap()).unwrap();
    std::fs::write(dir.path().join("conversations/broken.json"), "{").unwrap();
    let (_runtime, engine) = engine(dir.path());
    assert_eq!(engine.prompt_history(Some("open"), Area::Chat, None).unwrap(), ["  keep these spaces\n"]);
    store.remove("open");
    assert!(engine.prompt_history(Some("open"), Area::Chat, None).unwrap().is_empty(), "no cache outlives a read");
}

#[test]
#[ignore = "isolated 500-conversation prompt history time and sampled peak memory measurement"]
fn prompt_history_cost_for_500_saved_conversations() {
    measure_prompt_history_cost(false);
}

#[test]
#[ignore = "isolated first recall during sidebar initialization time and sampled peak memory measurement"]
fn prompt_history_cold_cost_for_500_saved_conversations() {
    measure_prompt_history_cost(true);
}

fn measure_prompt_history_cost(cold: bool) {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    fn usage(system: &mut System, pid: Pid) -> (u64, u64) {
        system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory().with_cpu());
        let process = system.process(pid).unwrap();
        (process.memory(), process.accumulated_cpu_time())
    }

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("conversations")).unwrap();
    let mut bytes = 0;
    for i in 0..500 {
        let messages: Vec<_> = (0..20)
            .flat_map(|m| {
                let user = prompt(&format!("Conversation {i}, prompt {m}: {}", "earlier words ".repeat(16)), i * 100 + m);
                let mut answer = prompt(&"A final answer with more words. ".repeat(128), i * 100 + m);
                answer["role"] = json!("assistant");
                [user, answer]
            })
            .collect();
        let conversation: Conversation = serde_json::from_value(json!({
            "id": format!("saved-{i}"), "title": format!("saved-{i}"), "area": "chat", "projectId": null,
            "createdAt": 1, "updatedAt": i, "messages": messages,
        }))
        .unwrap();
        let saved = serde_json::to_vec(&conversation).unwrap();
        bytes += saved.len();
        let path = dir.path().join(format!("conversations/saved-{i}.json"));
        std::fs::write(&path, saved).unwrap();
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(i)))
            .unwrap();
    }
    // Only the warm case initializes the sidebar before measurement; both cases read identical valid files.
    let warm = (!cold).then(|| {
        let pair = engine(dir.path());
        wait_for_sidebar(&pair.1);
        assert_eq!(pair.1.prompt_history(None, Area::Chat, None).unwrap().len(), 200);
        assert!(pair.1.search_messages("", Area::Chat).unwrap().ready);
        pair
    });
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    let (baseline, cpu) = usage(&mut system, pid);
    let done = Arc::new(AtomicBool::new(false));
    let stop = done.clone();
    let sampler = std::thread::spawn(move || {
        let mut system = System::new();
        let mut peak = baseline;
        while !stop.load(Ordering::Relaxed) {
            peak = peak.max(usage(&mut system, pid).0);
            std::thread::sleep(Duration::from_millis(1));
        }
        peak.max(usage(&mut system, pid).0)
    });
    let started = Instant::now();
    let (_runtime, engine) = warm.unwrap_or_else(|| engine(dir.path()));
    assert_eq!(engine.search_messages("", Area::Chat).unwrap().ready, !cold, "cold first Up must begin before the sidebar is ready");
    let first_up = Instant::now();
    let history = engine.prompt_history(Some("saved-0"), Area::Chat, None).unwrap();
    let elapsed = first_up.elapsed();
    let total = started.elapsed();
    let (after, cpu_after) = usage(&mut system, pid);
    done.store(true, Ordering::Relaxed);
    let peak = sampler.join().unwrap().max(after);
    assert_eq!(history.len(), 200);
    assert!(history[0].starts_with("Conversation 0, prompt 19:"));
    assert!(history[20].starts_with("Conversation 499, prompt 19:"));
    assert!(history[199].starts_with("Conversation 491, prompt 0:"));
    assert!(peak < 400 * 1024 * 1024, "prompt history alone exceeds R125");
    println!(
        "PROMPT HISTORY COST ({} index): 500 conversations / 10000 prompts / 10000 replies / {bytes} saved bytes; first Up {:.2} ms; engine-start + first Up {:.2} ms; process CPU {} ms; resident baseline {baseline} B, sampled peak {peak} B, growth {} B; returned payload {} B",
        if cold { "cold, not ready at first Up" } else { "warm" },
        elapsed.as_secs_f64() * 1000.0,
        total.as_secs_f64() * 1000.0,
        cpu_after.saturating_sub(cpu),
        peak.saturating_sub(baseline),
        history.iter().map(String::len).sum::<usize>()
    );
}
