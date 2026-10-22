use crate::support;

use faber_engine::codex::{
    CodexServer, CodexServerOptions, CodexThread, CodexTurnState, codex_args, codex_thread, list_codex_models, list_codex_threads,
    parse_codex_notification, read_codex_limits, thread_params,
};
use faber_engine::events::{Delivery, EngineEvent, Session, UserTurn};
use faber_engine::model::{Area, Permissions};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use support::{Entry, drive, fake, fixture, next_permission, text_of};

#[tokio::test]
async fn r160_stop_before_codex_resume_cancels_the_approved_plan_before_it_is_sent() {
    stop_before_resume("Go ahead with the plan.", true).await;
}

#[tokio::test]
async fn r160_stop_before_codex_resume_cancels_an_ordinary_message_before_it_is_sent() {
    stop_before_resume("ordinary message", false).await;
}

async fn stop_before_resume(text: &str, leave_plan: bool) {
    let (spawn, mut input, mut output) = support::controlled();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn,
        tools: Default::default(),
    });
    let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let session = server.open(
        CodexThread { resume: Some("t".into()), leave_plan, ..mode_thread(Permissions::Ask, Default::default()) },
        Arc::new(move |event| drop(tx.send(event))),
    );
    support::expect_method(&mut input, "initialize").await;
    support::emit(&mut output, json!({"id":1,"result":{}})).await;
    support::expect_method(&mut input, "initialized").await;
    let resume = support::expect_method(&mut input, "thread/resume").await;
    session.send(UserTurn::text(text));
    session.interrupt();
    // The answer is a queue barrier: Stop was processed before the held resume reply is released.
    assert_eq!(session.answer("missing", false).await, Ok(Delivery::Unknown));
    support::emit(&mut output, json!({"id":resume["id"],"result":{"thread":{"id":"t","model":"test"}}})).await;
    support::expect_method(&mut input, "skills/list").await;
    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    assert!(tokio::time::timeout(Duration::from_millis(50), input.read_line(&mut line)).await.is_err(), "stopped work was sent: {line}");
    let done = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(event @ EngineEvent::Done { .. }) = events.recv().await {
                break event;
            }
        }
    })
    .await
    .expect("a cancelled, never-started turn still finishes the engine reply");
    assert!(matches!(done, EngineEvent::Done { ok: false, reason, input_tokens: 0, .. } if reason == "interrupted"));
    // Stop applies to the cancelled message, not the person's next deliberate message.
    session.send(UserTurn::text("after Stop"));
    let next = support::expect_method(&mut input, "turn/start").await;
    assert_eq!(next["params"]["input"][0]["text"], "after Stop");
    session.close();
    server.retire();
}

#[tokio::test]
async fn r160_stop_before_codex_turn_id_interrupts_when_the_start_reply_supplies_it() {
    stop_before_turn_id(false, false).await;
}

#[tokio::test]
async fn r160_stop_before_codex_turn_id_interrupts_when_the_notification_supplies_it() {
    stop_before_turn_id(true, false).await;
}

#[tokio::test(start_paused = true)]
async fn r160_stop_before_codex_turn_id_survives_retirement_after_the_stop_grace_expires() {
    tokio::time::timeout(Duration::from_secs(20), stop_before_turn_id(false, true))
        .await
        .expect("a late turn id is interrupted without shutting down before completion");
}

async fn stop_before_turn_id(notification_first: bool, delayed: bool) {
    let (spawn, mut input, mut output) = support::controlled();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn,
        tools: Default::default(),
    });
    let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let session = server.open(mode_thread(Permissions::Ask, Default::default()), Arc::new(move |event| drop(tx.send(event))));
    support::expect_method(&mut input, "initialize").await;
    support::emit(&mut output, json!({"id":1,"result":{}})).await;
    support::expect_method(&mut input, "initialized").await;
    let open = support::expect_method(&mut input, "thread/start").await;
    support::emit(&mut output, json!({"id":open["id"],"result":{"thread":{"id":"t","model":"test"}}})).await;
    session.send(UserTurn::text("ordinary message"));
    let start = support::expect_method(&mut input, "turn/start").await;
    session.interrupt();
    assert_eq!(session.answer("missing", false).await, Ok(Delivery::Unknown));
    if delayed {
        tokio::time::advance(Duration::from_secs(6)).await;
        let failure = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Some(event @ EngineEvent::Error { fatal: true, .. }) = events.recv().await {
                    break event;
                }
            }
        })
        .await
        .expect("the unconfirmed Stop retires the engine's live session");
        assert!(matches!(failure, EngineEvent::Error { message, .. } if message == "error.cli.stopUnconfirmed"));
        session.close();
        assert_eq!(session.answer("missing", false).await, Ok(Delivery::Unknown));
        // Unrelated process news must not let the closing-thread cleanup forget the pending Stop either.
        support::emit(&mut output, json!({"method":"account/updated","params":{}})).await;
    }
    let reply = json!({"id":start["id"],"result":{"turn":{"id":"one"}}});
    let notification = json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"one"}}});
    support::emit(&mut output, if notification_first { notification.clone() } else { reply.clone() }).await;
    let interrupt = support::expect_method(&mut input, "turn/interrupt").await;
    assert_eq!(interrupt["params"], json!({"threadId":"t","turnId":"one"}));
    support::emit(&mut output, if notification_first { reply } else { notification }).await;
    assert_eq!(session.answer("missing", false).await, Ok(Delivery::Unknown));
    use tokio::io::AsyncBufReadExt;
    let mut line = String::new();
    assert!(tokio::time::timeout(Duration::from_millis(50), input.read_line(&mut line)).await.is_err(), "duplicate interrupt: {line}");
    support::emit(&mut output, json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"one","status":"interrupted"}}}))
        .await;
    session.close();
    server.retire();
}

#[tokio::test]
async fn r160_plan_collaboration_uses_the_model_returned_by_the_thread() {
    let (spawn, mut input, mut output) = support::controlled();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::Plan,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn,
        tools: Default::default(),
    });
    let session = server.open(
        CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: false,
            cwd: ".".into(),
            model: None,
            effort: Some("high".into()),
            resume: None,
            instructions: None,
            research: None,
            group_instructions: None,
            user_name: None,
            permissions: Permissions::Plan,
            tuning: Default::default(),
        },
        Arc::new(|_| {}),
    );
    support::expect_method(&mut input, "initialize").await;
    support::emit(&mut output, json!({"id":1,"result":{}})).await;
    support::expect_method(&mut input, "initialized").await;
    support::expect_method(&mut input, "thread/start").await;
    support::emit(&mut output, json!({"id":2,"result":{"thread":{"id":"t","model":"thread-model"}}})).await;
    session.send(UserTurn::text("make a plan"));
    let start = support::expect_method(&mut input, "turn/start").await;
    assert_eq!(start["params"]["collaborationMode"]["mode"], "plan");
    assert_eq!(start["params"]["collaborationMode"]["settings"]["model"], "thread-model");
    assert_eq!(start["params"]["collaborationMode"]["settings"]["reasoning_effort"], "high");
    assert_eq!(start["params"]["input"][0]["text"], "make a plan");
    session.close();
    server.retire();
}

#[tokio::test]
async fn r160_ordinary_default_turn_omits_collaboration_mode() {
    let (spawn, mut input, mut output) = support::controlled();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn,
        tools: Default::default(),
    });
    let session = server.open(
        CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: false,
            cwd: ".".into(),
            model: None,
            effort: None,
            resume: None,
            instructions: None,
            research: None,
            group_instructions: None,
            user_name: None,
            permissions: Permissions::Ask,
            tuning: Default::default(),
        },
        Arc::new(|_| {}),
    );
    support::expect_method(&mut input, "initialize").await;
    support::emit(&mut output, json!({"id":1,"result":{}})).await;
    support::expect_method(&mut input, "initialized").await;
    support::expect_method(&mut input, "thread/start").await;
    support::emit(&mut output, json!({"id":2,"result":{"thread":{"id":"t","model":"thread-model"}}})).await;
    session.send(UserTurn::text("ordinary turn"));
    let start = support::expect_method(&mut input, "turn/start").await;
    assert!(start["params"].get("collaborationMode").is_none());
    session.close();
    server.retire();
}

#[tokio::test]
async fn r160_resumed_plan_sends_default_once_then_omits_collaboration_mode() {
    let (spawn, mut input, mut output) = support::controlled();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::AcceptEdits,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn,
        tools: Default::default(),
    });
    let session = server.open(
        CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: true,
            cwd: ".".into(),
            model: None,
            effort: None,
            resume: Some("t".into()),
            instructions: None,
            research: None,
            group_instructions: None,
            user_name: None,
            permissions: Permissions::AcceptEdits,
            tuning: Default::default(),
        },
        Arc::new(|_| {}),
    );
    support::expect_method(&mut input, "initialize").await;
    support::emit(&mut output, json!({"id":1,"result":{}})).await;
    support::expect_method(&mut input, "initialized").await;
    support::expect_method(&mut input, "thread/resume").await;
    session.send(UserTurn::text("approved run"));
    session.send(UserTurn::text("next ordinary turn"));
    support::emit(&mut output, json!({"id":2,"result":{"thread":{"id":"t","model":"thread-model"}}})).await;
    let leaving = support::expect_method(&mut input, "turn/start").await;
    assert_eq!(leaving["params"]["collaborationMode"]["mode"], "default");
    assert_eq!(leaving["params"]["collaborationMode"]["settings"]["model"], "thread-model");
    let ordinary = support::expect_method(&mut input, "turn/start").await;
    assert!(ordinary["params"].get("collaborationMode").is_none());
    session.close();
    server.retire();
}

fn visible(event: &EngineEvent) -> bool {
    !matches!(event, EngineEvent::Writers { .. } | EngineEvent::Cursor { .. } | EngineEvent::WriterExit { .. })
}

async fn next_visible<T>(rx: &mut tokio::sync::mpsc::UnboundedReceiver<(T, EngineEvent)>) -> Option<(T, EngineEvent)> {
    while let Some((owner, event)) = rx.recv().await {
        if visible(&event) {
            return Some((owner, event));
        }
    }
    None
}

/// A real contained process tree with only its protocol pipes replaced by the deterministic test wire.
pub(crate) struct ContainedWire {
    pub spawn: faber_engine::proc::Spawner,
    pub input: tokio::io::BufReader<tokio::io::DuplexStream>,
    pub output: tokio::io::DuplexStream,
    pub tree: faber_engine::proc::OwnedTree,
    pub workers: Vec<u32>,
    directory: tempfile::TempDir,
    _stdin: Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
}

impl ContainedWire {
    pub fn release(&self, worker: usize) {
        std::fs::write(self.directory.path().join(format!("release-{worker}")), b"exit").unwrap();
    }
}

impl Drop for ContainedWire {
    fn drop(&mut self) {
        for worker in 0..self.workers.len() {
            let _ = std::fs::write(self.directory.path().join(format!("release-{worker}")), b"exit");
        }
    }
}

pub(crate) fn process_alive(pid: u32) -> bool {
    let mut system = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(pid);
    system.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid]), true, sysinfo::ProcessRefreshKind::nothing());
    system.process(pid).is_some_and(|process| process.status() != sysinfo::ProcessStatus::Zombie)
}

pub(crate) fn contained_diagnostics(tree: &faber_engine::proc::OwnedTree) -> String {
    let members = tree.descendants();
    #[cfg(windows)]
    let names = {
        let snapshot = faber_winproc::processes().unwrap_or_default();
        members
            .as_ref()
            .map(|members| {
                members
                    .iter()
                    .map(|member| {
                        let process = snapshot.iter().find(|process| process.pid == member.pid && process.created == member.created);
                        json!({"pid":member.pid,"created":member.created,"name":process.map(|process| &process.name),
                "parent":process.map(|process| process.parent)})
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    #[cfg(not(windows))]
    let names: Vec<serde_json::Value> = vec![];
    format!("root={:?}, descendants={members:?}, process_names={names:?}", tree.identity())
}

pub(crate) async fn contained_wire(workers: usize) -> ContainedWire {
    contained_wire_with_groups(workers, false).await
}

async fn contained_wire_with_groups(workers: usize, separate_groups: bool) -> ContainedWire {
    contained_wire_with_descendant(workers, separate_groups, false).await
}

async fn contained_wire_with_descendant(workers: usize, separate_groups: bool, descendant: bool) -> ContainedWire {
    let directory = tempfile::tempdir().unwrap();
    let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "codex_session::rewind_contained_process_helper", "--ignored", "--nocapture"])
        .env("FABER_REWIND_PROCESS_HELPER", "root")
        .env("FABER_REWIND_PROCESS_FOLDER", directory.path())
        .env("FABER_REWIND_PROCESS_SEPARATE_GROUPS", if separate_groups { "1" } else { "0" })
        .env("FABER_REWIND_PROCESS_DESCENDANT", if descendant { "1" } else { "0" })
        .env("FABER_REWIND_PROCESS_WORKERS", workers.to_string());
    let mut process = faber_engine::proc::spawn_command(command).unwrap();
    assert!(process.child.is_some() && process.contained.is_some(), "exercise the driver's real-process branch");
    let tree = process.contained.as_ref().unwrap().owned_tree().unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(bytes) = std::fs::read(directory.path().join("ready.json"))
                && let Ok(ready) = serde_json::from_slice::<serde_json::Value>(&bytes)
            {
                break ready;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the contained helper did not announce its workers");
    assert_eq!(ready["root"].as_u64().unwrap() as u32, tree.identity().pid);
    let workers = ready["workers"].as_array().unwrap().iter().map(|pid| pid.as_u64().unwrap() as u32).collect();
    let (stdin, input) = tokio::io::duplex(65536);
    let (output, mut injected) = tokio::io::duplex(65536);
    let (stdout, mut forwarded) = tokio::io::duplex(65536);
    let real_stdin = std::mem::replace(&mut process.stdin, Box::new(stdin));
    let mut real_stdout = std::mem::replace(&mut process.stdout, Box::new(stdout));
    tokio::spawn(async move {
        let mut discard = tokio::io::sink();
        // A dead real server closes its protocol pipe even while the test still holds its injection endpoint.
        tokio::select! {
            _ = tokio::io::copy(&mut real_stdout, &mut discard) => (),
            _ = tokio::io::copy(&mut injected, &mut forwarded) => (),
        }
    });
    let process = Arc::new(std::sync::Mutex::new(Some(process)));
    ContainedWire {
        spawn: Arc::new(move |_, _, _| Ok(process.lock().unwrap().take().expect("one server per real tree"))),
        input: tokio::io::BufReader::new(input),
        output,
        tree,
        workers,
        directory,
        _stdin: real_stdin,
    }
}

#[test]
#[ignore = "child entry point used only by the contained-process integration tests"]
fn rewind_contained_process_helper() {
    use std::io::Read;
    let Ok(role) = std::env::var("FABER_REWIND_PROCESS_HELPER") else { return };
    let folder = std::path::PathBuf::from(std::env::var_os("FABER_REWIND_PROCESS_FOLDER").unwrap());
    if role == "worker" {
        let worker = std::env::var("FABER_REWIND_PROCESS_WORKER").unwrap();
        if worker == "0" && std::env::var("FABER_REWIND_PROCESS_DESCENDANT").as_deref() == Ok("1") {
            #[allow(clippy::zombie_processes, reason = "the regression requires a child to outlive its immediate parent")]
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "codex_session::rewind_contained_process_helper", "--ignored", "--nocapture"])
                .env("FABER_REWIND_PROCESS_WORKER", "1")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            std::fs::write(folder.join("descendant"), child.id().to_string()).unwrap();
        }
        let release = folder.join(format!("release-{worker}"));
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        while folder.exists() && !release.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if release.exists() && std::env::var("FABER_REWIND_PROCESS_SEPARATE_GROUPS").as_deref() == Ok("1") {
            std::fs::write(folder.join(format!("written-{worker}")), b"opened after the turn ended").unwrap();
        }
        return;
    }
    assert_eq!(role, "root");
    let count: usize = std::env::var("FABER_REWIND_PROCESS_WORKERS").unwrap().parse().unwrap();
    let mut children = Vec::new();
    for worker in 0..count {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "codex_session::rewind_contained_process_helper", "--ignored", "--nocapture"])
            .env("FABER_REWIND_PROCESS_HELPER", "worker")
            .env("FABER_REWIND_PROCESS_WORKER", worker.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        #[cfg(unix)]
        if std::env::var("FABER_REWIND_PROCESS_SEPARATE_GROUPS").as_deref() == Ok("1") {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        children.push(command.spawn().unwrap());
    }
    let mut workers = children.iter().map(std::process::Child::id).collect::<Vec<_>>();
    if std::env::var("FABER_REWIND_PROCESS_DESCENDANT").as_deref() == Ok("1") {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Ok(value) = std::fs::read_to_string(folder.join("descendant"))
                && let Ok(pid) = value.parse::<u32>()
            {
                workers.push(pid);
                break;
            }
            assert!(std::time::Instant::now() < deadline, "the terminal did not announce its child");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    std::fs::write(folder.join("ready.json"), serde_json::to_vec(&json!({"root":std::process::id(),"workers":workers})).unwrap()).unwrap();
    let (closed, input) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read(&mut [0]);
        let _ = closed.send(());
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    while input.try_recv().is_err() && std::time::Instant::now() < deadline {
        children.retain_mut(|child| child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
    for mut child in children {
        let _ = child.kill();
        let _ = child.wait();
    }
}

pub(crate) async fn contained_message(input: &mut tokio::io::BufReader<tokio::io::DuplexStream>) -> serde_json::Value {
    use tokio::io::AsyncBufReadExt;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let mut line = String::new();
            input.read_line(&mut line).await.unwrap();
            let msg: serde_json::Value = serde_json::from_str(&line).unwrap();
            if msg["method"] != "skills/list" {
                return msg;
            }
        }
    })
    .await
    .expect("the real-contained driver stopped responding")
}

async fn contained_server(wire: &mut ContainedWire) -> CodexServer {
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "unused".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Project,
        user_hooks: true,
        windows: cfg!(windows),
        spawn: wire.spawn.clone(),
        tools: Default::default(),
    });
    let initialize = contained_message(&mut wire.input).await;
    assert_eq!(initialize["method"], "initialize");
    support::emit(&mut wire.output, json!({"id":initialize["id"],"result":{}})).await;
    assert_eq!(contained_message(&mut wire.input).await["method"], "initialized");
    server
}

async fn contained_thread(server: &CodexServer, wire: &mut ContainedWire, id: &str, sink: faber_engine::claude::EventSink) -> Session {
    let session = server.open(
        CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: false,
            cwd: ".".into(),
            model: None,
            effort: None,
            resume: None,
            instructions: None,
            research: None,
            group_instructions: None,
            user_name: None,
            permissions: Permissions::Ask,
            tuning: Default::default(),
        },
        sink,
    );
    let open = contained_message(&mut wire.input).await;
    assert_eq!(open["method"], "thread/start");
    support::emit(&mut wire.output, json!({"id":open["id"],"result":{"thread":{"id":id}}})).await;
    session
}

async fn contained_turn(session: &Session, wire: &mut ContainedWire, turn: &str) {
    session.send(UserTurn::text(turn));
    let start = contained_message(&mut wire.input).await;
    assert_eq!(start["method"], "turn/start");
    support::emit(&mut wire.output, json!({"id":start["id"],"result":{"turn":{"id":turn}}})).await;
}

pub(crate) async fn process_exited(pid: u32) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while process_alive(pid) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the contained worker did not exit");
}

/// The OS lists a process as a group member until its exit completes, which can lag the moment `process_alive` stops
/// seeing it (macOS reports a dying process as running for a few more milliseconds): wait, bounded, for the listing to
/// empty. A member that never leaves still fails, with the diagnostics.
pub(crate) async fn descendants_gone(tree: &faber_engine::proc::OwnedTree, what: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while !tree.descendants().unwrap().is_empty() {
        assert!(tokio::time::Instant::now() < deadline, "{what}: {}", contained_diagnostics(tree));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn catalog_kits_close_releases_a_real_drained_thread_while_the_peer_keeps_the_server() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut wire = contained_wire(1).await;
        let server = contained_server(&mut wire).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let owner_tx = tx.clone();
        let owner = contained_thread(&server, &mut wire, "owner", Arc::new(move |event| drop(owner_tx.send(("owner", event))))).await;
        let peer = contained_thread(&server, &mut wire, "peer", Arc::new(move |event| drop(tx.send(("peer", event))))).await;
        contained_turn(&peer, &mut wire, "peer-turn").await;
        owner.close();
        // A barrier confirms Close reached the driver while an unreported native descendant still owns resources.
        assert_eq!(peer.answer("missing", false).await, Ok(Delivery::Unknown));
        while let Ok((who, event)) = rx.try_recv() {
            assert!(!matches!((who, event), ("owner", EngineEvent::WriterExit { confirmed: true })), "a live descendant lost its kit");
        }
        wire.release(0);
        process_exited(wire.workers[0]).await;
        descendants_gone(&wire.tree, "the native writer exited").await;
        loop {
            match rx.recv().await.unwrap() {
                ("owner", EngineEvent::WriterExit { confirmed: true }) => break,
                ("peer", EngineEvent::WriterExit { .. }) => panic!("closing the owner retired its active peer"),
                _ => (),
            }
        }
        assert!(process_alive(wire.tree.identity().pid), "the server survives after the owner's delivery is dropped");
        support::emit(
            &mut wire.output,
            json!({"method":"item/agentMessage/delta","params":{
            "threadId":"peer","turnId":"peer-turn","itemId":"words","delta":"peer still active"}}),
        )
        .await;
        while !matches!(rx.recv().await.unwrap(), ("peer", EngineEvent::Text(text)) if text == "peer still active") {}
        support::emit(
            &mut wire.output,
            json!({"method":"turn/completed","params":{
            "threadId":"peer","turn":{"id":"peer-turn","status":"completed"}}}),
        )
        .await;
        while !matches!(rx.recv().await.unwrap(), ("peer", EngineEvent::Done { .. })) {}
        peer.close();
        server.retire();
        while !matches!(rx.recv().await.unwrap(), ("peer", EngineEvent::WriterExit { confirmed: true })) {}
    })
    .await
    .expect("a closed native thread releases without waiting for its active peer");
}

#[tokio::test]
async fn rewind_real_contained_idle_stop_after_native_exit_keeps_the_shared_peer_and_clears_the_owner() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut wire = contained_wire(1).await;
        let server = contained_server(&mut wire).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let owner_tx = tx.clone();
        let owner = contained_thread(&server, &mut wire, "owner", Arc::new(move |event| drop(owner_tx.send(("owner", event))))).await;
        let peer = contained_thread(&server, &mut wire, "peer", Arc::new(move |event| drop(tx.send(("peer", event))))).await;
        contained_turn(&owner, &mut wire, "owner-turn").await;
        support::emit(&mut wire.output, json!({"method":"item/started","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"native","processId":"owner-terminal-handle"}}})).await;
        while !matches!(rx.recv().await.unwrap(), ("owner", EngineEvent::Writers {active, ..}) if active.iter().any(|id| id == "native-command:native")) {}
        contained_turn(&peer, &mut wire, "peer-turn").await;
        wire.release(0);
        process_exited(wire.workers[0]).await;
        descendants_gone(&wire.tree, "the native command really exited, not only its protocol item").await;
        support::emit(&mut wire.output, json!({"method":"item/completed","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"native","processId":"owner-terminal-handle","exitCode":0}}})).await;
        support::emit(&mut wire.output, json!({"method":"turn/completed","params":{"threadId":"owner","turn":{"id":"owner-turn","status":"completed"}}})).await;
        let (mut done, mut clear) = (false, false);
        while !done || !clear {
            match rx.recv().await.unwrap() {
                ("owner", EngineEvent::Done { .. }) => done = true,
                ("owner", EngineEvent::Writers {active, unknown:false}) if active.is_empty() => clear = true,
                _ => (),
            }
        }
        owner.interrupt();
        server.tools(faber_engine::tools::Delivery { skills: Some(wire.directory.path().into()), ..Default::default() });
        let barrier = contained_message(&mut wire.input).await;
        assert_eq!(barrier["method"], "skills/extraRoots/set", "idle Stop needs no cancellation request");
        support::emit(&mut wire.output, json!({"method":"item/agentMessage/delta","params":{"threadId":"peer","turnId":"peer-turn","itemId":"words","delta":"peer still active"}})).await;
        loop {
            match rx.recv().await.unwrap() {
                ("owner", EngineEvent::Writers {active, unknown}) => assert!(active.is_empty() && !unknown, "idle Stop invented an unresolved writer"),
                (_, EngineEvent::Error {message, ..}) => panic!("idle Stop failed: {message}"),
                ("peer", EngineEvent::Text(text)) if text == "peer still active" => break,
                ("peer", EngineEvent::WriterExit { .. } | EngineEvent::Done { .. }) => panic!("idle Stop ended the peer"),
                _ => (),
            }
        }
        assert!(process_alive(wire.tree.identity().pid), "the shared server stayed alive for the peer");
        support::emit(&mut wire.output, json!({"method":"turn/completed","params":{"threadId":"peer","turn":{"id":"peer-turn","status":"completed"}}})).await;
        while !matches!(rx.recv().await.unwrap(), ("peer", EngineEvent::Done { .. })) {}
        owner.close(); peer.close(); server.retire();
        let mut exits = std::collections::BTreeSet::new();
        while exits.len() < 2 {
            if let (owner, EngineEvent::WriterExit {confirmed:true}) = rx.recv().await.unwrap() { exits.insert(owner); }
        }
        process_exited(wire.tree.identity().pid).await;
    }).await.expect("the real-contained idle Stop exchange hung");
}

#[cfg(unix)]
#[tokio::test]
async fn rewind_a_reported_terminal_outside_the_original_group_stays_unknown_until_the_cli_reports_exit() {
    tokio::time::timeout(Duration::from_secs(60), async {
        for disconnect in [false, true] {
        let mut wire = contained_wire_with_groups(1, true).await;
        let server = contained_server(&mut wire).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let owner = contained_thread(&server, &mut wire, "owner", Arc::new(move |event| drop(tx.send(event)))).await;
        contained_turn(&owner, &mut wire, "owner-turn").await;
        support::emit(&mut wire.output, json!({"method":"item/started","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"terminal","processId":"opaque-pty-terminal"}}})).await;
        while !matches!(rx.recv().await.unwrap(), EngineEvent::Writers {active, unknown:true}
            if active.iter().any(|id| id == "native-command:terminal")) {}
        assert!(wire.tree.descendants().unwrap().is_empty(), "the terminal is deliberately in its own process group");
        assert!(process_alive(wire.workers[0]), "absence from the original group does not mean the terminal ended");
        support::emit(&mut wire.output, json!({"method":"item/completed","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"terminal","processId":"opaque-pty-terminal","exitCode":null}}})).await;
        support::emit(&mut wire.output, json!({"method":"turn/completed","params":{"threadId":"owner","turn":{"id":"owner-turn","status":"completed"}}})).await;
        support::emit(&mut wire.output, json!({"method":"item/agentMessage/delta","params":{"threadId":"owner","turnId":"owner-turn","itemId":"barrier","delta":"turn ended"}})).await;
        let mut done = false;
        loop {
            match rx.recv().await.unwrap() {
                EngineEvent::Done { .. } => done = true,
                EngineEvent::Writers {active, unknown} => {
                    assert!(unknown && active.iter().any(|id| id == "native-command:terminal"),
                        "the original group's empty snapshot cannot release a reported terminal");
                }
                EngineEvent::Text(text) if text == "turn ended" => break,
                _ => (),
            }
        }
        assert!(done);
        if disconnect {
            use tokio::io::AsyncWriteExt;
            wire.output.shutdown().await.unwrap();
            loop {
                if let EngineEvent::WriterExit {confirmed} = rx.recv().await.unwrap() {
                    assert!(!confirmed, "ending the original process group cannot confirm a detached terminal's exit");
                    break;
                }
            }
            process_exited(wire.tree.identity().pid).await;
            assert!(process_alive(wire.workers[0]), "the reported terminal outlives its disconnected app-server");
        }
        assert!(!wire.directory.path().join("written-0").exists());
        std::fs::write(wire.directory.path().join("written-0"), b"the version a restore would put here").unwrap();
        wire.release(0);
        if disconnect {
            // The original parent is gone; the file proves execution reached the fresh open even before orphan reaping.
            tokio::time::timeout(Duration::from_secs(30), async {
                while std::fs::read(wire.directory.path().join("written-0")).unwrap() != b"opened after the turn ended" {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }).await.expect("the detached terminal did not reach its release barrier");
            process_exited(wire.workers[0]).await;
            owner.interrupt();
            while !matches!(rx.recv().await.unwrap(), EngineEvent::WriterExit {confirmed:true}) {}
            drop(owner);
            server.retire();
            drop(server);
            continue;
        }
        process_exited(wire.workers[0]).await;
        assert_eq!(std::fs::read(wire.directory.path().join("written-0")).unwrap(), b"opened after the turn ended",
            "the unreported terminal could freshly open a file after the original group was already empty");
        support::emit(&mut wire.output, json!({"method":"item/completed","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"terminal","processId":"opaque-pty-terminal","exitCode":0}}})).await;
        while !matches!(rx.recv().await.unwrap(), EngineEvent::Writers {active, unknown:false} if active.is_empty()) {}
        owner.close(); server.retire();
        while !matches!(rx.recv().await.unwrap(), EngineEvent::WriterExit {confirmed:true}) {}
        process_exited(wire.tree.identity().pid).await;
        }
    }).await.expect("the separate-process-group terminal regression hung");
}

#[cfg(windows)]
#[tokio::test]
async fn rewind_real_contained_idle_stop_retires_the_whole_job_after_a_premature_native_exit_code() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut wire = contained_wire(1).await;
        let server = contained_server(&mut wire).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let owner = contained_thread(&server, &mut wire, "owner", Arc::new(move |event| drop(tx.send(event)))).await;
        contained_turn(&owner, &mut wire, "owner-turn").await;
        support::emit(&mut wire.output, json!({"method":"item/started","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"native","processId":"owner-terminal-handle"}}})).await;
        while !matches!(rx.recv().await.unwrap(), EngineEvent::Writers {active, ..} if active.iter().any(|id| id == "native-command:native")) {}
        support::emit(&mut wire.output, json!({"method":"item/completed","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"native","processId":"owner-terminal-handle","exitCode":0}}})).await;
        support::emit(&mut wire.output, json!({"method":"turn/completed","params":{"threadId":"owner","turn":{"id":"owner-turn","status":"completed"}}})).await;
        support::emit(&mut wire.output, json!({"method":"item/agentMessage/delta","params":{"threadId":"owner","turnId":"owner-turn","itemId":"barrier","delta":"turn ended"}})).await;
        let (mut done, mut blocked) = (false, false);
        loop {
            match rx.recv().await.unwrap() {
                EngineEvent::Done { .. } => done = true,
                EngineEvent::Writers {active, unknown} => {
                    blocked |= !active.is_empty() && unknown;
                    assert!(!active.is_empty() || unknown, "an exit code cannot release a real live native writer");
                }
                EngineEvent::Text(text) if text == "turn ended" => break,
                EngineEvent::Error {message, ..} => panic!("premature native completion failed: {message}"),
                _ => (),
            }
        }
        assert!(done && blocked && process_alive(wire.workers[0]));
        owner.interrupt();
        while !matches!(rx.recv().await.unwrap(), EngineEvent::WriterExit {confirmed:true}) {}
        process_exited(wire.workers[0]).await;
        process_exited(wire.tree.identity().pid).await;
        assert!(wire.tree.descendants().unwrap().is_empty());
        server.retire();
    }).await.expect("idle Stop did not retire the real native command container");
}

#[cfg(unix)]
#[tokio::test]
async fn rewind_a_native_exit_keeps_every_shared_conversation_blocked_until_its_sampled_child_exits() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut wire = contained_wire_with_descendant(1, true, true).await;
        assert_eq!(wire.workers.len(), 2, "the native terminal has a real child");
        let server = contained_server(&mut wire).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let owner_tx = tx.clone();
        let owner = contained_thread(&server, &mut wire, "owner", Arc::new(move |event| drop(owner_tx.send(("owner", event))))).await;
        let peer = contained_thread(&server, &mut wire, "peer", Arc::new(move |event| drop(tx.send(("peer", event))))).await;
        contained_turn(&owner, &mut wire, "owner-turn").await;
        support::emit(
            &mut wire.output,
            json!({"method":"item/started","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"native","processId":"opaque-terminal-handle"}}}),
        )
        .await;
        let mut states = std::collections::BTreeMap::new();
        loop {
            if let (thread, EngineEvent::Writers { active, unknown }) = rx.recv().await.unwrap() {
                states.insert(thread, unknown);
                if thread == "owner" && unknown && active.iter().any(|id| id == "native-command:native") {
                    break;
                }
            }
        }
        assert!(wire.tree.descendants().unwrap().is_empty(), "both terminal processes are outside the original server group");
        assert!(wire.workers.iter().all(|pid| process_alive(*pid)), "the report samples the complete live parent chain");
        support::emit(
            &mut wire.output,
            json!({"method":"turn/completed","params":{"threadId":"owner","turn":{"id":"owner-turn","status":"completed"}}}),
        )
        .await;
        loop {
            match rx.recv().await.unwrap() {
                ("owner", EngineEvent::Done { .. }) => break,
                (thread, EngineEvent::Writers { unknown, .. }) => {
                    states.insert(thread, unknown);
                }
                _ => (),
            }
        }

        wire.release(0);
        process_exited(wire.workers[0]).await;
        assert!(process_alive(wire.workers[1]), "successful terminal exit leaves its child alive and reparented");
        support::emit(
            &mut wire.output,
            json!({"method":"item/completed","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"native","processId":"opaque-terminal-handle","exitCode":0}}}),
        )
        .await;
        let mut seen = std::collections::BTreeSet::new();
        for thread in ["owner", "peer"] {
            support::emit(
                &mut wire.output,
                json!({"method":"item/agentMessage/delta","params":{
                "threadId":thread,"turnId":"owner-turn","itemId":"barrier","delta":"parent exited"}}),
            )
            .await;
        }
        while seen.len() < 2 {
            match rx.recv().await.unwrap() {
                (thread, EngineEvent::Writers { unknown, .. }) => {
                    assert!(unknown, "{thread} was released while the native command's sampled child can still write");
                    states.insert(thread, unknown);
                }
                (thread, EngineEvent::Text(text)) if text == "parent exited" => {
                    seen.insert(thread);
                }
                (_, EngineEvent::WriterExit { confirmed: true }) => panic!("the live native child was declared gone"),
                _ => (),
            }
        }
        assert_eq!(
            states,
            std::collections::BTreeMap::from([("owner", true), ("peer", true)]),
            "the shared server blocks both conversations"
        );
        let destination = wire.directory.path().join("written-1");
        std::fs::write(&destination, b"the restored version").unwrap();
        wire.release(1);
        process_exited(wire.workers[1]).await;
        assert_eq!(
            std::fs::read(destination).unwrap(),
            b"opened after the turn ended",
            "the surviving child freshly opened the file after exitCode: 0"
        );
        let mut clear = std::collections::BTreeSet::new();
        while clear.len() < 2 {
            if let (thread, EngineEvent::Writers { active, unknown: false }) = rx.recv().await.unwrap()
                && active.is_empty()
            {
                clear.insert(thread);
            }
        }
        owner.close();
        peer.close();
        server.retire();
        let mut exits = std::collections::BTreeSet::new();
        while exits.len() < 2 {
            if let (thread, EngineEvent::WriterExit { confirmed: true }) = rx.recv().await.unwrap() {
                exits.insert(thread);
            }
        }
        process_exited(wire.tree.identity().pid).await;
    })
    .await
    .expect("the native terminal's surviving-child regression hung");
}

#[tokio::test]
async fn rewind_real_contained_shared_stop_uses_terminal_handles_without_claiming_a_peers_process() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let mut wire = contained_wire(2).await;
        let server = contained_server(&mut wire).await;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let owner_tx = tx.clone();
        let owner = contained_thread(&server, &mut wire, "owner", Arc::new(move |event| drop(owner_tx.send(("owner", event))))).await;
        let peer = contained_thread(&server, &mut wire, "peer", Arc::new(move |event| drop(tx.send(("peer", event))))).await;
        // The owner's opaque terminal handle happens to equal the peer's operating-system PID.
        let owner_terminal = wire.workers[1].to_string();
        let peer_terminal = "peer-terminal-handle";
        for (id, session, terminal) in [("owner", &owner, owner_terminal.as_str()), ("peer", &peer, peer_terminal)] {
            contained_turn(session, &mut wire, &format!("{id}-turn")).await;
            support::emit(&mut wire.output, json!({"method":"item/started","params":{"threadId":id,"item":{
                "type":"commandExecution","id":"native","processId":terminal}}})).await;
            while !matches!(rx.recv().await.unwrap(), (key, EngineEvent::Writers {active, ..})
                if key == id && active.iter().any(|item| item == "native-command:native")) {}
        }
        assert_eq!(wire.tree.descendants().unwrap().len(), 2, "both native commands are actual owned child processes: {}", contained_diagnostics(&wire.tree));
        support::emit(&mut wire.output, json!({"method":"item/completed","params":{"threadId":"owner","item":{
            "type":"commandExecution","id":"native","processId":owner_terminal,"exitCode":null}}})).await;
        support::emit(&mut wire.output, json!({"method":"turn/completed","params":{"threadId":"owner","turn":{"id":"owner-turn","status":"completed"}}})).await;
        while !matches!(rx.recv().await.unwrap(), ("owner", EngineEvent::Done { .. })) {}
        owner.interrupt();
        let mut reported_gone = false;
        let mut stopped = false;
        let mut listed = false;
        while !reported_gone || !stopped || !listed {
            tokio::select! {
                message = contained_message(&mut wire.input) => {
                    match message["method"].as_str().unwrap() {
                        "thread/backgroundTerminals/terminate" => {
                            assert_eq!(message["params"]["threadId"], "owner", "Stop cannot address the peer");
                            assert_eq!(message["params"]["processId"], owner_terminal);
                            assert!(process_alive(wire.workers[1]), "a terminal handle must never authorize killing a matching OS PID");
                            // The CLI, which knows the opaque handle's owner, stops its own command.
                            wire.release(0);
                            process_exited(wire.workers[0]).await;
                            support::emit(&mut wire.output, json!({"id":message["id"],"result":{}})).await;
                            stopped = true;
                        }
                        "thread/backgroundTerminals/list" => {
                            assert_eq!(message["params"]["threadId"], "owner");
                            support::emit(&mut wire.output, json!({"id":message["id"],"result":{"data":[]}})).await;
                            listed = true;
                        }
                        method => panic!("unexpected stop request: {method}"),
                    }
                }
                event = rx.recv() => match event.unwrap() {
                    ("owner", EngineEvent::Writers {active, unknown:true}) if stopped && listed && !active.iter().any(|id| id == "native-command:native") => {
                        assert!(active.iter().any(|id| id == "process-tree"), "unattributed server children still block restore");
                        reported_gone = true;
                    }
                    ("peer", EngineEvent::Done { .. } | EngineEvent::WriterExit { .. } | EngineEvent::Error { .. }) => panic!("Stop interrupted the shared peer"),
                    ("owner", EngineEvent::Error {message, ..}) => assert_eq!(message, "error.rewind.stopUnconfirmed"),
                    _ => (),
                },
            }
        }
        assert!(process_alive(wire.workers[1]), "the peer's command stayed alive");
        assert!(process_alive(wire.tree.identity().pid), "the peer's app-server stayed alive");
        assert_eq!(wire.tree.descendants().unwrap().iter().map(|process| process.pid).collect::<Vec<_>>(), vec![wire.workers[1]]);
        wire.release(1);
        process_exited(wire.workers[1]).await;
        support::emit(&mut wire.output, json!({"method":"item/completed","params":{"threadId":"peer","item":{
            "type":"commandExecution","id":"native","processId":peer_terminal,"exitCode":0}}})).await;
        support::emit(&mut wire.output, json!({"method":"turn/completed","params":{"threadId":"peer","turn":{"id":"peer-turn","status":"completed"}}})).await;
        while !matches!(rx.recv().await.unwrap(), ("peer", EngineEvent::Done { .. })) {}
        owner.close(); peer.close(); server.retire();
        let mut exits = std::collections::BTreeSet::new();
        while exits.len() < 2 {
            if let (owner, EngineEvent::WriterExit {confirmed:true}) = rx.recv().await.unwrap() { exits.insert(owner); }
        }
        process_exited(wire.tree.identity().pid).await;
    }).await.expect("the real-contained scoped Stop exchange hung");
}

#[tokio::test(start_paused = true)]
async fn opus_1_codex_manual_turns_stay_available_after_long_silence_or_start_error() {
    for area in [Area::Chat, Area::Project] {
        let (spawn, mut input, mut output) = support::controlled();
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::Ask,
            cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
            cwd: ".".into(),
            area,
            user_hooks: false,
            windows: false,
            spawn,
            tools: Default::default(),
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let session = server.open(
            CodexThread {
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
                leave_plan: false,
                cwd: ".".into(),
                model: None,
                effort: None,
                resume: None,
                instructions: None,
                research: None,
                user_name: None,
                group_instructions: None,
                permissions: Permissions::Ask,
                tuning: Default::default(),
            },
            Arc::new(move |e| {
                if visible(&e) {
                    drop(tx.send(e));
                }
            }),
        );
        support::expect_method(&mut input, "initialize").await;
        support::emit(&mut output, json!({"id":1,"result":{}})).await;
        support::expect_method(&mut input, "initialized").await;
        support::expect_method(&mut input, "thread/start").await;
        support::emit(&mut output, json!({"id":2,"result":{"thread":{"id":"t"}}})).await;
        assert!(matches!(rx.recv().await, Some(EngineEvent::Started { .. })));
        session.send(UserTurn::text("long tool"));
        let first = support::expect_method(&mut input, "turn/start").await;
        support::emit(&mut output, json!({"id":first["id"],"result":{"turn":{"id":"one"}}})).await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(600)).await;
        tokio::task::yield_now().await;
        assert!(!std::iter::from_fn(|| rx.try_recv().ok()).any(|event| visible(&event)));
        session.send(UserTurn::text("second"));
        let second = support::expect_method(&mut input, "turn/start").await;
        support::emit(&mut output, json!({"id":second["id"],"error":{"code":-1,"message":"rejected"}})).await;
        assert!(matches!(rx.recv().await, Some(EngineEvent::Error { fatal: false, .. })));
        tokio::time::advance(Duration::from_secs(600)).await;
        tokio::task::yield_now().await;
        assert!(!std::iter::from_fn(|| rx.try_recv().ok()).any(|event| visible(&event)), "a failed start left the watchdog armed");
        session.send(UserTurn::text("third"));
        support::expect_method(&mut input, "turn/start").await;
        session.close();
        server.retire();
    }
}

#[tokio::test]
async fn opus_unconfirmed_completion_before_the_start_response_is_not_lost() {
    let mut entries = vec![];
    let mut response = None;
    for entry in fixture("codex", "chat") {
        if entry.msg["method"] == "turn/started" {
            continue;
        }
        if entry.msg["result"]["turn"]["id"].is_string() {
            response = Some(entry);
            continue;
        }
        let completed = entry.msg["method"] == "turn/completed";
        if completed {
            let mut stale = entry.msg.clone();
            stale["params"]["turn"]["id"] = "stale".into();
            entries.push(Entry { input: false, msg: stale });
        }
        entries.push(entry);
        if completed && let Some(reply) = response.take() {
            entries.push(reply);
        }
    }
    check_chat(entries).await;
}

#[tokio::test]
async fn excessive_unidentified_completions_fail_closed_without_accepting_a_stale_turn() {
    let mut entries = vec![];
    for entry in fixture("codex", "chat") {
        if entry.msg["method"] == "turn/started" || entry.msg["result"]["turn"]["id"].is_string() {
            continue;
        }
        if entry.msg["method"] == "turn/completed" {
            for id in ["stale-a", "stale-b", "stale-c"] {
                let mut msg = entry.msg.clone();
                msg["params"]["turn"]["id"] = id.into();
                entries.push(Entry { input: false, msg });
            }
            break;
        }
        entries.push(entry);
    }
    let (events, mismatches) =
        run(entries, "In one sentence: why is the sky blue?", None, |e, _| matches!(e, EngineEvent::Error { fatal: true, .. })).await;
    assert!(mismatches.is_empty());
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::Done { .. })));
    assert!(matches!(events.last(),Some(EngineEvent::Error {message,fatal:true}) if message == "error.cli.stopUnconfirmed"));
}

#[tokio::test]
async fn review_3_completion_without_started_uses_the_turn_start_response_identity() {
    let entries = fixture("codex", "chat").into_iter().filter(|e| e.msg["method"] != "turn/started").collect();
    check_chat(entries).await;
}

#[tokio::test]
async fn review_7_codex_preserves_a_turn_sent_before_interrupt_acknowledgement() {
    let (spawn, mut input, mut output) = support::controlled();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn,
        tools: Default::default(),
    });
    let session = server.open(
        CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: false,
            cwd: ".".into(),
            model: None,
            effort: None,
            resume: None,
            instructions: None,
            research: None,
            user_name: None,
            group_instructions: None,
            permissions: Permissions::Ask,
            tuning: Default::default(),
        },
        Arc::new(|_| {}),
    );
    support::expect_method(&mut input, "initialize").await;
    support::emit(&mut output, json!({"id":1,"result":{}})).await;
    support::expect_method(&mut input, "initialized").await;
    support::expect_method(&mut input, "thread/start").await;
    support::emit(&mut output, json!({"id":2,"result":{"thread":{"id":"t","model":"test"}}})).await;
    session.send(UserTurn::text("first"));
    support::expect_method(&mut input, "turn/start").await;
    support::emit(&mut output, json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"one"}}})).await;
    tokio::task::yield_now().await;
    session.interrupt();
    support::expect_method(&mut input, "turn/interrupt").await;
    session.send(UserTurn::text("second"));
    tokio::time::sleep(Duration::from_millis(10)).await;
    support::emit(&mut output, json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"one","status":"interrupted"}}}))
        .await;
    let next = support::expect_method(&mut input, "turn/start").await;
    assert_eq!(next["params"]["input"][0]["text"], "second");
    session.close();
    server.retire();
}

#[tokio::test(start_paused = true)]
async fn an_ignored_interrupt_quarantines_only_that_thread_and_preserves_a_running_peer() {
    let f = fake(fixture("projects", "policy-shared-stop"));
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
        tools: Default::default(),
    };
    let server = CodexServer::start(opts);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let open = |name: &'static str| {
        let tx = tx.clone();
        server.open(
            CodexThread {
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
                leave_plan: false,
                cwd: ".".into(),
                model: None,
                effort: None,
                resume: None,
                instructions: None,
                research: None,
                user_name: None,
                group_instructions: None,
                permissions: Permissions::Ask,
                tuning: Default::default(),
            },
            Arc::new(move |e| drop(tx.send((name, e)))),
        )
    };
    let (a, b) = (open("a"), open("b"));
    a.send(UserTurn::text("a"));
    b.send(UserTurn::text("b"));
    loop {
        let event = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .unwrap_or_else(|_| panic!("stalled: {:?}", f.mismatches.lock().unwrap()));
        if let Some(("b", EngineEvent::Permission { .. })) = event {
            break;
        }
    }
    a.interrupt();
    let (name, event) = tokio::time::timeout(Duration::from_secs(10), next_visible(&mut rx)).await.unwrap().unwrap();
    assert_eq!(name, "a");
    assert!(matches!(event, EngineEvent::Error { message, fatal:true } if message == "error.cli.stopUnconfirmed"));
    assert!(server.is_alive(), "a thread's timeout must not kill the shared server");
    a.send(UserTurn::text("must not dispatch"));
    assert_eq!(b.answer("approval-b", true).await, Ok(Delivery::Delivered));
    assert!(matches!(next_visible(&mut rx).await, Some(("b", EngineEvent::Done { ok: true, .. }))));
    assert_eq!(f.wire.lines_with("turn/start"), 2, "the quarantined thread dispatched nothing");
    assert!(f.mismatches.lock().unwrap().is_empty(), "{:?}", f.mismatches.lock().unwrap());
    a.close();
    b.close();
    server.retire();
}

async fn run(
    entries: Vec<Entry>,
    first: &str,
    resume: Option<&str>,
    react: impl FnMut(&EngineEvent, &Session) -> bool,
) -> (Vec<EngineEvent>, Vec<String>) {
    let thread = CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: resume.map(String::from),
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        tuning: Default::default(),
    };
    drive(
        entries,
        |spawn, sink| {
            let opts = CodexServerOptions {
                permissions: Permissions::Ask,
                cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: ".".into(),
                area: Area::Chat,
                user_hooks: false,
                windows: false,
                spawn,
                tools: Default::default(),
            };
            CodexServer::start(opts).open(thread, sink)
        },
        first,
        react,
    )
    .await
}

fn recorded_answers(entries: &[Entry]) -> String {
    entries
        .iter()
        .filter(|e| e.msg["method"] == "item/completed" && e.msg["params"]["item"]["type"] == "agentMessage")
        .map(|e| e.msg["params"]["item"]["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

async fn check_chat(entries: Vec<Entry>) {
    let expected = recorded_answers(&entries);
    let mut dones = 0;
    let (events, mismatches) = run(entries, "In one sentence: why is the sky blue?", None, |e, s| {
        if matches!(e, EngineEvent::Done { .. }) {
            dones += 1;
            if dones == 1 {
                s.send(UserTurn::text("Reply with just the word: pong"));
            }
        }
        dones == 2
    })
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(events[0], EngineEvent::Started { session_id: "01a12513-0e42-7b63-9a21-c8698f522c12".into(), model: "gpt-6-luna".into() });
    assert_eq!(text_of(&events), expected);
    assert!(
        events.iter().any(|e| matches!(e, EngineEvent::Quota { window, used, .. } if window == "seven_day" && (*used - 0.47).abs() < 1e-9))
    );
    let done: Vec<&EngineEvent> = events.iter().filter(|e| matches!(e, EngineEvent::Done { .. })).collect();
    assert!(
        done.iter().all(
            |d| matches!(d, EngineEvent::Done { ok: true, input_tokens, output_tokens, .. } if *input_tokens > 0 && *output_tokens > 0)
        )
    );
}

#[tokio::test]
async fn codex_chat_two_turns_with_text_model_quota_and_tokens() {
    check_chat(fixture("codex", "chat")).await;
}

#[tokio::test]
#[should_panic]
async fn codex_a_renamed_field_in_a_recorded_session_turns_the_check_red() {
    let entries = fixture("codex", "chat")
        .into_iter()
        .map(|mut e| {
            if e.msg["method"] == "item/agentMessage/delta" {
                let delta = e.msg["params"]["delta"].take();
                e.msg["params"]["text"] = delta;
                e.msg["params"].as_object_mut().unwrap().remove("delta");
            }
            e
        })
        .collect();
    check_chat(entries).await;
}

#[tokio::test]
async fn codex_permissions_the_first_command_is_allowed_and_the_second_declined() {
    let mut asks = Vec::new();
    let (events, mismatches) = run(
        fixture("codex", "tools"),
        "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.",
        None,
        |e, s| {
            if let EngineEvent::Permission { request_id, tool, .. } = e {
                asks.push(tool.clone());
                drop(s.answer(request_id, asks.len() == 1));
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(asks, ["command", "command"]);
    let results: Vec<(bool, bool)> = events
        .iter()
        .filter_map(|e| if let EngineEvent::ToolResult { ok, declined, .. } = e { Some((*ok, *declined)) } else { None })
        .collect();
    assert_eq!(results, [(true, false), (false, true)], "a refused approval is declined, not a failure");
}

/// R124b, item 5: the request names only `auth.json`; the command's folder is what says it is the sign-in. The driver refuses it
/// itself (a refusal is always safe, so nothing races a mode switch) and the person is asked only about the other command.
#[tokio::test]
async fn r124b_a_command_run_inside_a_credential_folder_is_refused_without_a_card() {
    let mut entries = fixture("codex", "tools");
    let at = entries.iter().position(|e| !e.input && e.msg["method"] == "item/commandExecution/requestApproval").unwrap();
    entries[at].msg["params"]["cwd"] = "C:\\Users\\me\\.codex".into();
    entries[at].msg["params"]["command"] = "type auth.json".into();
    entries[at + 1].msg["result"]["decision"] = "decline".into();
    let mut asks = Vec::new();
    let (_, mismatches) =
        run(entries, "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.", None, |e, s| {
            if let EngineEvent::Permission { request_id, input, .. } = e {
                asks.push(input.to_string());
                drop(s.answer(request_id, false));
            }
            matches!(e, EngineEvent::Done { .. })
        })
        .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(asks.len(), 1, "only the second command is asked: {asks:?}");
    assert!(!asks[0].contains("auth.json"));
}

/// R161: a Codex subagent (recorded with codex-cli 0.162.1, Ask mode) is the agent's `Agent` step. Its command runs under that
/// step, its approval is the conversation's card, its last words are the step's report, and none of its words join the answer.
#[tokio::test]
async fn r161_a_codex_subagent_is_a_step_of_the_reply_and_its_approval_is_the_conversations() {
    let entries = fixture("codex", "subagent");
    let mut asks = Vec::new();
    let (events, mismatches) = run(
        entries,
        "Spawn exactly one subagent named copier. Its task: create the file c.txt containing the word gamma using a shell command, then reply with the word done. Wait for it, then answer with its reply. Do not do the work yourself.",
        None,
        |e, s| {
            if let EngineEvent::Permission { request_id, tool, .. } = e {
                asks.push(tool.clone());
                drop(s.answer(request_id, true));
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert_eq!(asks, ["command"]);
    let call = "call_abd8fed50e4141a6b7feaebc0a02089c";
    let tools: Vec<(&str, &str, Option<&str>)> = events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::Tool { id, name, parent, .. } => Some((id.as_str(), name.as_str(), parent.as_deref())),
            _ => None,
        })
        .collect();
    assert_eq!(tools.len(), 2, "only the subagent and its command, no bookkeeping: {tools:?}");
    assert_eq!(tools[0], (call, "Agent", None));
    assert_eq!((tools[1].1, tools[1].2), ("commandExecution", Some(call)));
    assert!(events.iter().any(|e| matches!(e, EngineEvent::Tool { input, .. } if input["description"] == "copier")));
    assert!(
        events.iter().any(|e| matches!(e, EngineEvent::ToolResult { id, ok: true, text, .. } if id == call && text == "done")),
        "the subagent's last words settle its step"
    );
    assert_eq!(text_of(&events), "done", "the subagent's own words never join the answer");
    // Codex counts the subagent's thread on its own (59420 in, 108 out); the reply's usage holds both threads.
    let done = events.iter().find(|e| matches!(e, EngineEvent::Done { .. })).unwrap();
    assert!(matches!(done, EngineEvent::Done { input_tokens: 129_470, output_tokens: 199, cost_total_usd: Some(_), .. }), "{done:?}");
}

/// R161 step 3: a Codex subagent's finished messages are its own words on its step, in order with its commands. The recording's
/// subagent writes only its report (phase final_answer), so one line of words is added by hand before its command.
#[tokio::test]
async fn r161_a_codex_subagents_words_reach_its_step_in_order_with_its_commands() {
    let mut entries = fixture("codex", "subagent");
    let at = entries
        .iter()
        .position(|e| e.msg["method"] == "item/started" && e.msg["params"]["item"]["type"] == "commandExecution")
        .expect("the subagent's command");
    let thread = entries[at].msg["params"]["threadId"].clone();
    let said = json!({ "method": "item/completed", "params": { "threadId": thread, "item": { "type": "agentMessage", "id": "m1", "text": "I will copy the file first.", "phase": "commentary" } } });
    entries.insert(at, Entry { input: false, msg: said });
    let (events, mismatches) = run(
        entries,
        "Spawn exactly one subagent named copier. Its task: create the file c.txt containing the word gamma using a shell command, then reply with the word done. Wait for it, then answer with its reply. Do not do the work yourself.",
        None,
        |e, s| {
            if let EngineEvent::Permission { request_id, .. } = e {
                drop(s.answer(request_id, true));
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    let call = "call_abd8fed50e4141a6b7feaebc0a02089c";
    let seen: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            EngineEvent::ChildWords { parent, text } => Some(format!("words {parent} {text}")),
            EngineEvent::Tool { name, parent: Some(parent), .. } => Some(format!("tool {parent} {name}")),
            _ => None,
        })
        .collect();
    assert_eq!(
        seen,
        [format!("words {call} I will copy the file first."), format!("tool {call} commandExecution")],
        "its words come before its command, and its final answer (the report) is not kept as words"
    );
    assert_eq!(text_of(&events), "done", "none of them join the answer");
}

/// R161: a subagent's approval belongs to the turn it serves. One still open when that turn ends, and one that arrives after
/// it, are refused on the wire: neither hangs unseen nor becomes a card of another reply.
#[tokio::test]
async fn r161_a_codex_subagent_approval_outside_its_turn_is_refused() {
    let recorded = fixture("codex", "subagent");
    let ask = recorded.iter().position(|e| !e.input && e.msg["method"] == "item/commandExecution/requestApproval").unwrap();
    let parent = recorded.iter().find_map(|e| e.msg["result"]["thread"]["id"].as_str()).unwrap();
    let end =
        recorded.iter().rposition(|e| !e.input && e.msg["method"] == "turn/completed" && e.msg["params"]["threadId"] == parent).unwrap();
    // The subagent's command starts just before it asks.
    let command = (0..ask).rev().find(|&i| recorded[i].msg["params"]["item"]["type"] == "commandExecution").unwrap();
    let decline = Entry { input: true, msg: json!({"id":0,"result":{"decision":"decline"}}) };
    // Open when the turn ends: the request, then the parent's end. Late: the parent's end, then the command and the request.
    for late in [false, true] {
        let mut entries = recorded[..if late { command } else { ask }].to_vec();
        if late {
            entries.extend([recorded[end].clone(), recorded[command].clone(), recorded[ask].clone()]);
        } else {
            entries.extend([recorded[ask].clone(), recorded[end].clone()]);
        }
        entries.push(decline.clone());
        let f = fake(entries);
        let opts = CodexServerOptions {
            permissions: Permissions::Ask,
            cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
            cwd: ".".into(),
            area: Area::Chat,
            user_hooks: false,
            windows: false,
            spawn: f.spawn.clone(),
            tools: Default::default(),
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let thread = CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: false,
            cwd: ".".into(),
            model: None,
            effort: None,
            resume: None,
            instructions: None,
            research: None,
            user_name: None,
            group_instructions: None,
            permissions: Permissions::Ask,
            tuning: Default::default(),
        };
        let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
        session.send(UserTurn::text(
            recorded.iter().find(|e| e.msg["method"] == "turn/start").unwrap().msg["params"]["input"][0]["text"].as_str().unwrap(),
        ));
        let mut cards = 0;
        loop {
            match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("stalled").expect("ended") {
                EngineEvent::Permission { .. } => cards += 1,
                EngineEvent::Done { incomplete, .. } => {
                    assert!(incomplete, "late={late}: a subagent still at work leaves the reply's usage incomplete");
                    break;
                }
                _ => {}
            }
        }
        tokio::time::timeout(Duration::from_secs(3), async {
            while f.wire.lines_with("\"decision\":\"decline\"") == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("late={late}: the request was left unanswered"));
        assert_eq!(f.wire.lines_with("\"decision\":\"accept\""), 0);
        while let Ok(event) = rx.try_recv() {
            assert!(!matches!(event, EngineEvent::Tool { .. }), "late={late}: a subagent's late step reaches no reply: {event:?}");
        }
        assert_eq!(cards, usize::from(!late), "late={late}: a late request shows no card");
        session.close();
        assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new(), "late={late}");
    }
}

#[tokio::test]
async fn codex_an_answer_that_fails_to_write_keeps_the_approval_and_the_retry_is_written_once() {
    let f = fake(fixture("codex", "tools"));
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
        tools: Default::default(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let thread = CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        tuning: Default::default(),
    };
    let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text("Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step."));
    let request = next_permission(&mut rx).await;
    assert_eq!(request, "0", "Codex numbers its requests from 0 in each server");
    let written = || f.wire.lines_with("\"decision\"");

    f.wire.refuse_next(1);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed));
    assert_eq!(written(), 0, "the refused write left nothing on the wire");
    // The thread still holds the approval, so the same answer can be given again.
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    assert_eq!(written(), 1);
    // An approval the thread never held, and one it has already delivered, write nothing.
    assert_eq!(session.answer("7", true).await, Ok(Delivery::Unknown));
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::Unknown));
    assert_eq!(written(), 1);
    session.close();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
}

#[tokio::test]
async fn codex_an_answer_that_may_be_half_written_is_never_written_again() {
    let f = fake(fixture("codex", "tools"));
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
        tools: Default::default(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let thread = CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        tuning: Default::default(),
    };
    let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
    session.send(UserTurn::text("Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step."));
    let request = next_permission(&mut rx).await;
    assert_eq!(request, "0", "Codex numbers its requests from 0 in each server");
    let before = f.wire.len();

    f.wire.tear_next(10);
    assert_eq!(session.answer(&request, true).await, Ok(Delivery::WriteFailed));
    let after = f.wire.len();
    assert!(after > before, "the first attempt put bytes on the wire");

    // The same answer again must not write a byte: the shared process is ending, for every thread in it.
    assert_ne!(session.answer(&request, true).await, Ok(Delivery::Delivered));
    assert_eq!(f.wire.len(), after, "nothing was written again");
    let ended = loop {
        match tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("the session did not end") {
            Some(EngineEvent::Error { fatal: true, .. }) | None => break true,
            Some(_) => {}
        }
    };
    assert!(ended);
}

#[tokio::test]
async fn codex_interrupt_stops_the_turn_and_the_thread_takes_the_next_one() {
    let (mut deltas, mut dones) = (0, 0);
    let (events, mismatches) = run(fixture("codex", "interrupt"), "Write a 600-word essay about the history of bread.", None, |e, s| {
        if matches!(e, EngineEvent::Text(_)) {
            deltas += 1;
            if deltas == 5 {
                s.interrupt();
            }
        }
        if matches!(e, EngineEvent::Done { .. }) {
            dones += 1;
            if dones == 1 {
                s.send(UserTurn::text("Reply with just the word: pong"));
            }
        }
        dones == 2
    })
    .await;
    assert_eq!(mismatches, Vec::<String>::new());
    let done: Vec<(bool, String)> = events
        .iter()
        .filter_map(|e| if let EngineEvent::Done { ok, reason, .. } = e { Some((*ok, reason.clone())) } else { None })
        .collect();
    assert_eq!(done[0], (false, "interrupted".to_string()));
    assert!(done[1].0);
}

#[tokio::test]
async fn codex_resume_a_new_process_continues_the_same_thread() {
    let id = "01a1251a-252b-7031-bcb9-dd0b0adfc7ce";
    let (events, mismatches) =
        run(fixture("codex", "resume"), "What single word did you reply with last time? Answer with only that word.", Some(id), |e, _| {
            matches!(e, EngineEvent::Done { .. })
        })
        .await;
    assert_eq!(mismatches, Vec::<String>::new());
    assert!(matches!(&events[0], EngineEvent::Started { session_id, .. } if session_id == id));
    assert_eq!(text_of(&events).trim(), "pong");
}

#[test]
fn codex_a_message_that_arrives_whole_without_deltas_is_still_shown_once() {
    let mut state = CodexTurnState::default();
    let done = |id: &str| json!({ "method": "item/completed", "params": { "item": { "type": "agentMessage", "id": id, "text": "hi" } } });
    assert_eq!(parse_codex_notification(&done("a"), &mut state), [EngineEvent::Text("hi".into())]);
    parse_codex_notification(&json!({ "method": "item/agentMessage/delta", "params": { "itemId": "b", "delta": "hi" } }), &mut state);
    assert_eq!(parse_codex_notification(&done("b"), &mut state), []);
}

#[test]
fn codex_chat_is_lean_and_sandboxed_projects_keep_the_user_setup_without_hooks() {
    let chat = codex_args(Area::Chat, false, false, Permissions::Ask).join(" ");
    assert!(chat.contains("--disable plugins") && chat.contains("mcp_servers={}") && chat.contains("project_doc_max_bytes=0"));
    assert!(chat.contains("--disable hooks") && !chat.contains("windows.sandbox"));
    let project = codex_args(Area::Project, true, false, Permissions::Ask).join(" ");
    assert!(project.contains("windows.sandbox=\"unelevated\"") && project.contains("--disable hooks"));
    assert!(!project.contains("plugins") && !project.contains("mcp_servers"));
    assert!(!codex_args(Area::Project, false, true, Permissions::Ask).join(" ").contains("hooks"));
    for area in [Area::Chat, Area::Project] {
        assert_eq!(codex_thread(area, None, Permissions::Ask)["sandbox"], "workspace-write");
        // "untrusted" asks before every command that is not a plain read and before every file change, inside the folder too
        // (checked against the installed Codex: "on-request" let sandboxed commands in the folder run unasked).
        assert_eq!(codex_thread(area, None, Permissions::Ask)["approvalPolicy"], "untrusted");
    }
    assert!(codex_thread(Area::Chat, None, Permissions::Ask).contains_key("baseInstructions"));
}

#[tokio::test]
async fn review3_user_mcp_servers_are_disabled_before_start_and_resume_without_copying_values() {
    for area in [Area::Chat, Area::Project] {
        for resume in [None, Some("thread".into())] {
            let method = if resume.is_some() { "thread/resume" } else { "thread/start" };
            let entries = vec![
                Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
                Entry { input: false, msg: json!({"id":1,"result":{}}) },
                Entry { input: true, msg: json!({"method":"initialized"}) },
                Entry { input: true, msg: json!({"id":"faber-config-1","method":"config/read"}) },
                Entry {
                    input: false,
                    msg: json!({"id":"faber-config-1","result":{"config":{"mcp_servers":{"github.push":{"env":{"TOKEN":"synthetic-do-not-copy"}}}}}}),
                },
                Entry { input: true, msg: json!({"id":2,"method":method}) },
                Entry { input: false, msg: json!({"id":2,"result":{"thread":{"id":"thread","model":"model"}}}) },
            ];
            let f = fake(entries);
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let server = CodexServer::start(CodexServerOptions {
                permissions: Permissions::Isolated,
                cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: ".".into(),
                area,
                user_hooks: false,
                windows: true,
                spawn: f.spawn.clone(),
                tools: Default::default(),
            });
            let session = server.open(
                CodexThread {
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    progress: false,
                    leave_plan: false,
                    cwd: ".".into(),
                    model: None,
                    effort: None,
                    resume,
                    instructions: None,
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    permissions: Permissions::Isolated,
                    tuning: Default::default(),
                },
                Arc::new(move |e| drop(tx.send(e))),
            );
            tokio::time::timeout(Duration::from_secs(3), async { while !matches!(rx.recv().await, Some(EngineEvent::Started { .. })) {} })
                .await
                .unwrap();
            let messages = f.wire.messages();
            let thread = messages.iter().find(|msg| msg["method"] == method).unwrap();
            assert_eq!(thread["params"]["config"]["mcp_servers"]["github.push"], json!({"enabled":false}));
            assert!(!serde_json::to_string(&messages).unwrap().contains("synthetic-do-not-copy"));
            assert!(f.mismatches.lock().unwrap().is_empty());
            session.close();
            server.retire();
        }
    }
}

#[tokio::test]
async fn the_chatgpt_model_list_comes_from_the_account_without_hidden_models() {
    let f = fake(fixture("codex", "models"));
    let models = list_codex_models("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_secs(3)).await;
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    let got: Vec<(String, String, bool)> = models.into_iter().map(|m| (m.id, m.label, m.is_default)).collect();
    assert_eq!(got, [("gpt-6.1-sol".into(), "GPT-6.1-Sol".into(), true), ("gpt-6-astra".into(), "GPT-6-Astra".into(), false)]);
}

/// Parity F5: the sessions started in a terminal in the project folder, newest first, named by their title or else their
/// first message on one line, without the ones Codex never saved.
#[tokio::test]
async fn the_terminal_sessions_of_a_folder_come_from_the_cli_own_list() {
    let f = fake(fixture("codex", "threads"));
    let list = list_codex_threads("codex", "/work/app".as_ref(), false, f.spawn.clone(), Duration::from_secs(3)).await;
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    let got: Vec<(&str, &str, u64)> = list.iter().map(|s| (s.id.as_str(), s.title.as_str(), s.updated_at)).collect();
    assert_eq!(
        got,
        [
            ("019a2b3c-0001-7000-8000-000000000001", "Fix the login form", 1_791_623_000_000),
            ("019a2b3c-0002-7000-8000-000000000002", "why does the build fail on windows", 1_791_510_000_000),
        ]
    );
    assert!(list.iter().all(|s| s.vendor == faber_engine::model::Vendor::Codex));
    assert_eq!((list[0].model.as_deref(), list[1].model.as_deref()), (Some("gpt-6.1-sol"), None));
    let silent = fake(vec![]);
    assert!(list_codex_threads("codex", "/work/app".as_ref(), false, silent.spawn.clone(), Duration::from_millis(50)).await.is_empty());
}

#[tokio::test]
async fn a_cli_that_never_answers_gives_an_empty_list_instead_of_hanging() {
    let f = fake(vec![]);
    let models = list_codex_models("codex", ".".as_ref(), false, f.spawn.clone(), Duration::from_millis(50)).await;
    assert!(models.is_empty());
}

#[tokio::test]
async fn one_process_serves_two_conversations_at_once_and_keeps_their_answers_apart() {
    let f = fake(fixture("codex", "two"));
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
        tools: Default::default(),
    };
    let server = CodexServer::start(opts);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let open = |name: &'static str| {
        let tx = tx.clone();
        let thread = CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: false,
            cwd: ".".into(),
            model: None,
            effort: Some("medium".into()),
            resume: None,
            instructions: None,
            research: None,
            user_name: None,
            group_instructions: None,
            permissions: Permissions::Ask,
            tuning: Default::default(),
        };
        server.open(thread, Arc::new(move |e| drop(tx.send((name, e)))))
    };
    let (a, b) = (open("a"), open("b"));
    a.send(UserTurn::text("In one word: what color is the sky on a clear day?"));
    b.send(UserTurn::text("In one word: what color is fresh grass?"));
    let (mut events_a, mut events_b) = (vec![], vec![]);
    let done = |events: &Vec<EngineEvent>| events.iter().any(|e| matches!(e, EngineEvent::Done { .. }));
    while !(done(&events_a) && done(&events_b)) {
        let (name, e) = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.expect("stalled").expect("server gone");
        if name == "a" { events_a.push(e) } else { events_b.push(e) }
    }
    a.close();
    b.close();
    assert_eq!(*f.mismatches.lock().unwrap(), Vec::<String>::new());
    assert_eq!(f.args.lock().unwrap().len(), 1, "both conversations share one process");
    assert_eq!(text_of(&events_a).trim(), "Blue");
    assert_eq!(text_of(&events_b).trim(), "Green");
    let started = |events: &[EngineEvent]| {
        events.iter().find_map(|e| if let EngineEvent::Started { session_id, .. } = e { Some(session_id.clone()) } else { None })
    };
    assert_ne!(started(&events_a), started(&events_b));
}

#[test]
fn a_side_question_thread_has_its_own_instructions_restricted_reads_no_writes_and_is_not_saved() {
    let thread = CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: ".".into(),
        model: Some("gpt-6-luna".into()),
        effort: None,
        resume: None,
        instructions: Some("Route.".into()),
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        tuning: Default::default(),
    };
    let (method, params) = thread_params(Area::Chat, &thread);
    assert_eq!(method, "thread/start");
    assert_eq!((&params["baseInstructions"], &params["approvalPolicy"]), (&"Route.".into(), &"never".into()));
    assert_eq!(params["model"], "gpt-6-luna");
    assert_eq!(params["ephemeral"], true, "it stays out of the user's Codex history");
}

#[test]
fn permissions_keep_the_workspace_sandbox_and_disable_project_network_without_approvals() {
    for area in [Area::Chat, Area::Project] {
        for permissions in [Permissions::Isolated, Permissions::Ask, Permissions::FullAccess] {
            for resume in [None, Some("thread-id".into())] {
                let t = CodexThread {
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    progress: false,
                    leave_plan: false,
                    cwd: "project".into(),
                    model: Some("model".into()),
                    effort: None,
                    resume,
                    instructions: None,
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    permissions,
                    tuning: Default::default(),
                };
                let (method, params) = thread_params(area, &t);
                assert_eq!(method, if t.resume.is_some() { "thread/resume" } else { "thread/start" });
                assert_eq!(params["cwd"], "project");
                assert_eq!(params["model"], "model");
                if permissions == Permissions::Isolated {
                    assert!(params.get("sandbox").is_none());
                    assert_eq!(params["approvalPolicy"], "never");
                    assert_eq!(params["config"]["sandbox_workspace_write.network_access"], false);
                } else if permissions == Permissions::FullAccess {
                    assert_eq!((&params["sandbox"], &params["approvalPolicy"]), (&"danger-full-access".into(), &"never".into()));
                    assert!(
                        params["config"] == json!({"web_search": "disabled"}) && params.get("dynamicTools").is_none(),
                        "full access sends only the web search setting"
                    );
                } else {
                    assert_eq!(params["sandbox"], "workspace-write");
                    assert_eq!(params["approvalPolicy"], "untrusted");
                    assert!(params["config"] == json!({"web_search": "disabled"}), "no profile: only the web search setting");
                }
                assert_eq!(params.get("baseInstructions").is_some(), area == Area::Chat);
                let (_, side) = thread_params(area, &CodexThread { instructions: Some("Route.".into()), ..t });
                assert!(side.get("sandbox").is_none());
                assert_eq!(side["approvalPolicy"], "never");
                let id = side["config"]["default_permissions"].as_str().unwrap();
                assert_eq!(side["config"][format!("permissions.{id}")]["filesystem"][":workspace_roots"]["."], "read");
            }
        }
    }
}

#[tokio::test]
async fn l2_serialized_start_and_resume_carry_the_no_approval_offline_policy() {
    for area in [Area::Chat, Area::Project] {
        for resume in [false, true] {
            let f = fake(fixture("codex", if resume { "resume" } else { "chat" }));
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let server = CodexServer::start(CodexServerOptions {
                permissions: Permissions::Ask,
                cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: ".".into(),
                area,
                user_hooks: false,
                windows: false,
                spawn: f.spawn.clone(),
                tools: Default::default(),
            });
            let session = server.open(
                CodexThread {
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    progress: false,
                    leave_plan: false,
                    cwd: ".".into(),
                    model: None,
                    effort: None,
                    resume: resume.then(|| "thread-id".into()),
                    instructions: None,
                    research: None,
                    user_name: None,
                    group_instructions: None,
                    permissions: Permissions::Isolated,
                    tuning: Default::default(),
                },
                Arc::new(move |e| {
                    let _ = tx.send(e);
                }),
            );
            loop {
                let e = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
                if matches!(e, EngineEvent::Started { .. }) {
                    break;
                }
            }
            let messages = f.wire.messages();
            let message = messages.iter().find(|m| m["method"] == if resume { "thread/resume" } else { "thread/start" }).unwrap();
            assert_eq!(message["params"]["approvalPolicy"], "never");
            assert!(message["params"].get("sandbox").is_none());
            let id = message["params"]["config"]["default_permissions"].as_str().unwrap();
            assert_eq!(message["params"]["config"][format!("permissions.{id}")]["filesystem"][":root"], "deny");
            assert_eq!(message["params"]["config"]["sandbox_workspace_write.network_access"], false);
            assert_eq!(message["params"]["config"]["shell_environment_policy.set"]["GIT_ALLOW_PROTOCOL"], "");
            session.close();
        }
    }
}

#[tokio::test]
async fn a_project_with_no_approvals_declines_a_residual_request_without_emitting_a_card() {
    for area in [Area::Chat, Area::Project] {
        let entries = fixture("codex", "tools")
            .into_iter()
            .map(|mut e| {
                if e.input && e.msg.get("result").is_some() && e.msg["result"].get("decision").is_some() {
                    e.msg["result"]["decision"] = "decline".into();
                }
                e
            })
            .collect();
        let (events, mismatches) = drive(
            entries,
            |spawn, sink| {
                let server = CodexServer::start(CodexServerOptions {
                    permissions: Permissions::Ask,
                    cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                    cwd: ".".into(),
                    area,
                    user_hooks: false,
                    windows: false,
                    spawn,
                    tools: Default::default(),
                });
                server.open(
                    CodexThread {
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        verify: false,
                        progress: false,
                        leave_plan: false,
                        cwd: ".".into(),
                        model: None,
                        effort: None,
                        resume: None,
                        instructions: None,
                        research: None,
                        user_name: None,
                        group_instructions: None,
                        permissions: Permissions::Isolated,
                        tuning: Default::default(),
                    },
                    sink,
                )
            },
            "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.",
            |event, _| matches!(event, EngineEvent::Done { .. }),
        )
        .await;
        assert!(mismatches.is_empty(), "{mismatches:?}");
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })));
    }
}

#[test]
fn the_name_is_a_developer_instruction_that_resume_repeats_and_a_side_question_never_gets() {
    let thread = |resume: Option<&str>, instructions: Option<&str>, name: Option<&str>| CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: resume.map(String::from),
        instructions: instructions.map(String::from),
        research: None,
        user_name: name.map(String::from),
        group_instructions: None,
        permissions: Permissions::Ask,
        tuning: Default::default(),
    };
    let (method, start) = thread_params(Area::Project, &thread(None, None, Some("Erick")));
    assert_eq!(method, "thread/start");
    assert!(start["developerInstructions"].as_str().unwrap().contains("called Erick."));
    assert!(start.get("baseInstructions").is_none(), "a project keeps Codex's own prompt");
    let (method, resume) = thread_params(Area::Project, &thread(Some("t1"), None, Some("Erick")));
    assert_eq!((method, &resume["developerInstructions"]), ("thread/resume", &start["developerInstructions"]));
    let (_, chat) = thread_params(Area::Chat, &thread(None, None, Some("Erick")));
    assert!(chat["baseInstructions"].as_str().unwrap().starts_with("You are a helpful"));
    let project_only = faber_engine::persona::PROJECT_DETAILS; // only a project's prompt names the Technical details view
    assert_eq!(start["developerInstructions"].as_str().unwrap().strip_suffix(project_only), chat["developerInstructions"].as_str());
    // Only the agent given the offer_bigger tool is told to use it; a helper shares the details and has no such tool.
    let (_, agent) = thread_params(Area::Project, &CodexThread { progress: true, ..thread(None, None, Some("Erick")) });
    assert!(agent["developerInstructions"].as_str().unwrap().ends_with(faber_engine::persona::BIGGER_OFFER));
    assert!(!start["developerInstructions"].as_str().unwrap().contains("offer_bigger"));
    let (_, none) = thread_params(Area::Chat, &thread(None, None, None));
    assert!(none["developerInstructions"].as_str().unwrap().contains("You are Faber"));
    let (_, side) = thread_params(Area::Chat, &thread(None, Some("Route."), Some("Erick")));
    assert!(side.get("developerInstructions").is_none());
}

const COUNT: &str = "Count from 1 to 30, one number per line, each followed by a five-word comment.";
const STEER: &str = "stop at 10";

/// Sends a steer when the first words of the answer arrive and runs the session to the end of the turn.
async fn steer_after_first_words(fixture_name: &str, until: fn(&EngineEvent) -> bool) -> (Vec<EngineEvent>, Vec<String>) {
    let mut steered = false;
    run(fixture("codex", fixture_name), COUNT, None, |e, s| {
        if matches!(e, EngineEvent::Text(_)) && !steered {
            steered = true;
            s.steer(UserTurn::text(STEER), "faber-probe-1");
        }
        until(e)
    })
    .await
}

#[tokio::test]
async fn a_steered_message_reaches_the_turn_and_the_driver_says_when_codex_took_it_in() {
    let (events, mismatches) = steer_after_first_words("steer", |e| matches!(e, EngineEvent::Done { .. })).await;
    assert_eq!(mismatches, Vec::<String>::new());
    // Live capture (codex-cli 0.162.1): Codex echoes the client id on the steered user message, between the answer it
    // cut short and the new answer, which it begins with a new agent message item.
    let injected: Vec<_> = events.iter().enumerate().filter(|(_, e)| matches!(e, EngineEvent::Injected { .. })).collect();
    assert_eq!(injected.len(), 1);
    assert_eq!(injected[0].1, &EngineEvent::Injected { tag: Some("faber-probe-1".into()) });
    let before = text_of(&events[..injected[0].0]);
    let after = text_of(&events[injected[0].0..]);
    assert!(before.starts_with("1 #"), "{before:?}");
    assert!(after.contains("Counting starts") && after.trim_end().ends_with("as you asked."), "{after:?}");
    assert!(!events.iter().any(|e| matches!(e, EngineEvent::SteerFailed { .. } | EngineEvent::Error { .. })));
}

#[tokio::test]
async fn a_steer_codex_refuses_is_reported_as_not_delivered_and_is_no_error() {
    for name in ["steer-rejected", "steer-late"] {
        let (events, mismatches) = steer_after_first_words(name, |e| matches!(e, EngineEvent::SteerFailed { .. })).await;
        assert_eq!(mismatches, Vec::<String>::new(), "{name}");
        assert!(events.contains(&EngineEvent::SteerFailed { tag: "faber-probe-1".into() }), "{name}: {events:?}");
        assert!(!events.iter().any(|e| matches!(e, EngineEvent::Injected { .. } | EngineEvent::Error { .. })), "{name}");
    }
}

#[tokio::test]
async fn a_steer_before_the_turn_has_an_id_is_not_delivered() {
    // The thread exists but no turn/started came yet, so there is nothing to name in expectedTurnId.
    let entries: Vec<Entry> = fixture("codex", "steer").into_iter().take(8).collect();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let f = fake(entries);
    let thread = CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        tuning: Default::default(),
    };
    let opts = CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
        tools: Default::default(),
    };
    let session = CodexServer::start(opts).open(thread, Arc::new(move |e| drop(tx.send(e))));
    session.steer(UserTurn::text("early"), "q9");
    let seen = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(EngineEvent::SteerFailed { tag }) = rx.recv().await {
                return tag;
            }
        }
    })
    .await
    .expect("the driver never said the steer was dropped");
    assert_eq!(seen, "q9");
    session.close();
}

#[test]
fn review3_dynamic_command_failures_keep_their_output_and_failed_status() {
    let events = parse_codex_notification(
        &json!({"method":"item/completed","params":{"item":{
            "id":"tool","type":"dynamicToolCall","status":"completed","success":false,
            "contentItems":[{"type":"inputText","text":"sandbox refused"}]
        }}}),
        &mut CodexTurnState::default(),
    );
    assert_eq!(events, vec![EngineEvent::ToolResult { id: "tool".into(), ok: false, declined: false, text: "sandbox refused".into() }]);
}

#[tokio::test]
async fn review3_command_gate_declines_git_mutations_and_runs_builds_only_in_the_sandbox() {
    for area in [Area::Chat, Area::Project] {
        let root = tempfile::tempdir().unwrap();
        let mut entries = vec![
            Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
            Entry { input: false, msg: json!({"id":1,"result":{}}) },
            Entry { input: true, msg: json!({"method":"initialized"}) },
            Entry { input: true, msg: json!({"id":2,"method":"thread/start"}) },
            Entry { input: false, msg: json!({"id":2,"result":{"thread":{"id":"thread","model":"model"}}}) },
        ];
        for (i, command) in ["git reset --hard HEAD~1", "git clean -fdx", "git rebase", "git push", "cargo test"].into_iter().enumerate() {
            entries.push(Entry{input:false,msg:json!({"id":format!("call-{i}"),"method":"item/tool/call","params":{"threadId":"thread","tool":"run_command","arguments":{"command":command}}})});
            entries.push(Entry { input: true, msg: json!({"id":format!("call-{i}"),"result":{"success":i==4}}) });
        }
        let f = fake(entries);
        let argv = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (start, capture) = (f.spawn.clone(), argv.clone());
        let spawn: faber_engine::proc::Spawner = Arc::new(move |bin, args, cwd| {
            if args.first().is_some_and(|arg| arg == "app-server") {
                return start(bin, args, cwd);
            }
            capture.lock().unwrap().push(args.to_vec());
            Ok(faber_engine::proc::Proc {
                stdin: Box::new(tokio::io::sink()),
                stdout: Box::new(std::io::Cursor::new(b"build completed".to_vec())),
                stderr: None,
                child: None,
                contained: None,
            })
        });
        let server = CodexServer::start(CodexServerOptions {
            cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
            cwd: root.path().into(),
            area,
            user_hooks: false,
            windows: true,
            permissions: Permissions::Isolated,
            spawn,
            tools: Default::default(),
        });
        let session = server.open(
            CodexThread {
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
                leave_plan: false,
                cwd: root.path().into(),
                model: None,
                effort: None,
                resume: None,
                instructions: None,
                research: None,
                user_name: None,
                group_instructions: None,
                permissions: Permissions::Isolated,
                tuning: Default::default(),
            },
            Arc::new(|_| {}),
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            while f.wire.lines_with("\"id\":\"call-") < 5 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let messages = f.wire.messages();
        for i in 0..5 {
            let reply = messages.iter().find(|msg| msg["id"] == format!("call-{i}")).unwrap();
            assert_eq!(reply["result"]["success"], i == 4);
        }
        let argv = argv.lock().unwrap();
        assert_eq!(argv.len(), 1, "only the build may start a child");
        assert!(argv[0].iter().any(|arg| arg == "sandbox"));
        assert!(argv[0].ends_with(&["cargo".into(), "test".into()]));
        assert!(f.mismatches.lock().unwrap().is_empty());
        session.close();
        server.retire();
    }
}

#[test]
fn only_a_user_message_after_the_first_of_a_turn_counts_as_taken_in() {
    let started = |client: serde_json::Value| json!({ "method": "item/started", "params": { "item": { "type": "userMessage", "id": "u", "clientId": client, "content": [] } } });
    let mut state = CodexTurnState::default();
    assert!(parse_codex_notification(&started(json!(null)), &mut state).is_empty(), "the turn's own prompt");
    let again = parse_codex_notification(&started(json!(null)), &mut state);
    assert_eq!(again, vec![EngineEvent::Injected { tag: None }]);
    // Our own tag, echoed back, is enough even if the first message was not seen.
    let mut fresh = CodexTurnState::default();
    let tagged = parse_codex_notification(&started(json!("faber-probe-1")), &mut fresh);
    assert_eq!(tagged, vec![EngineEvent::Injected { tag: Some("faber-probe-1".into()) }]);
    // A new turn starts counting again.
    parse_codex_notification(&json!({ "method": "turn/started", "params": { "turn": { "id": "t2" } } }), &mut state);
    assert!(parse_codex_notification(&started(json!(null)), &mut state).is_empty());
}

#[test]
fn r121_full_access_starts_codex_as_the_person_would_and_isolated_keeps_every_restriction() {
    for area in [Area::Chat, Area::Project] {
        for user_hooks in [false, true] {
            // Nothing disabled, no sandbox choice, no emptied MCP list, no cut-off project instructions.
            assert_eq!(codex_args(area, true, user_hooks, Permissions::FullAccess), ["app-server"], "{area:?}");
        }
        let isolated = codex_args(area, true, true, Permissions::Isolated);
        for feature in ["apps", "plugins", "computer_use", "browser_use", "hooks", "skill_mcp_dependency_install"] {
            assert!(isolated.windows(2).any(|pair| pair == ["--disable", feature]), "{area:?} {feature}");
        }
        assert!(isolated.contains(&"mcp_servers={}".into()) && isolated.contains(&"windows.sandbox=\"mxc\"".into()));
    }
}

/// Codex's only filesystem deny rules live in a permission profile. On Windows a profile that grants the whole disk and denies
/// the credential folders is refused ("MXC cannot verify it does not alias a denied object" when a drive letter is unreachable,
/// "requires the elevated Windows sandbox" otherwise), so full access sends no profile and no credential block exists for Codex.
/// This test states that limit; docs/security-r97-review.md and the Settings text say it to the person.
#[test]
fn r121_codex_full_access_has_no_sandbox_no_approvals_and_no_host_command_tool() {
    for area in [Area::Chat, Area::Project] {
        for resume in [None, Some("thread".into())] {
            let t = CodexThread {
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
                leave_plan: false,
                cwd: "project".into(),
                model: None,
                effort: None,
                resume,
                instructions: None,
                research: None,
                user_name: None,
                group_instructions: None,
                permissions: Permissions::FullAccess,
                tuning: Default::default(),
            };
            let (_, params) = thread_params(area, &t);
            assert_eq!((&params["sandbox"], &params["approvalPolicy"]), (&"danger-full-access".into(), &"never".into()), "{area:?}");
            for key in ["dynamicTools", "runtimeWorkspaceRoots"] {
                assert!(params.get(key).is_none(), "{area:?} {key}: full access sends no profile, no tool override and no workspace roots");
            }
            // R140: the only thing sent is the web search setting; no permission profile, no shell override.
            assert_eq!(params["config"], json!({"web_search": "disabled"}), "{area:?}");
        }
    }
}

#[test]
fn r121_a_side_question_stays_isolated_whatever_mode_the_person_chose() {
    for permissions in [Permissions::FullAccess, Permissions::Ask, Permissions::Isolated] {
        let t = CodexThread {
            helper: Default::default(),
            browse: false,
            conversations: false,
            verify: false,
            progress: false,
            leave_plan: false,
            cwd: ".".into(),
            model: None,
            effort: None,
            resume: None,
            instructions: Some("Route.".into()),
            research: None,
            user_name: None,
            group_instructions: None,
            permissions,
            tuning: Default::default(),
        };
        let (_, params) = thread_params(Area::Chat, &t);
        assert!(params.get("sandbox").is_none() && params.get("dynamicTools").is_none(), "{permissions:?}");
        assert_eq!(params["approvalPolicy"], "never");
        assert_eq!(params["ephemeral"], true);
        assert_eq!(params["config"]["features.shell_tool"], false, "{permissions:?}");
        let id = params["config"]["default_permissions"].as_str().unwrap();
        let profile = &params["config"][format!("permissions.{id}")];
        assert_eq!(profile["network"]["enabled"], false, "{permissions:?}");
        assert_eq!(profile["filesystem"][":workspace_roots"]["."], "read");
        assert_eq!(profile["filesystem"]["~/.codex"], "deny");
    }
}

#[tokio::test]
async fn r121_the_model_list_and_the_limits_reading_start_an_isolated_server() {
    let f = fake(vec![]);
    let _ = list_codex_models("codex", ".".as_ref(), true, f.spawn.clone(), Duration::from_millis(50)).await;
    let _ = read_codex_limits("codex", ".".as_ref(), true, f.spawn.clone(), Duration::from_millis(50)).await;
    let args = f.args.lock().unwrap();
    assert_eq!(args.len(), 2);
    for started in args.iter() {
        assert_eq!(*started, codex_args(Area::Chat, true, false, Permissions::Isolated));
        assert!(started.contains(&"mcp_servers={}".into()) && started.contains(&"windows.sandbox=\"mxc\"".into()));
    }
}

#[tokio::test]
async fn r121_a_full_access_thread_keeps_the_persons_mcp_servers_and_starts_without_a_sandbox() {
    for resume in [None, Some("thread".to_string())] {
        let method = if resume.is_some() { "thread/resume" } else { "thread/start" };
        let entries = vec![
            Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
            Entry { input: false, msg: json!({"id":1,"result":{}}) },
            Entry { input: true, msg: json!({"method":"initialized"}) },
            Entry { input: true, msg: json!({"id":2,"method":method}) },
            Entry { input: false, msg: json!({"id":2,"result":{"thread":{"id":"thread","model":"model"}}}) },
        ];
        let f = fake(entries);
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::FullAccess,
            cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
            cwd: ".".into(),
            area: Area::Chat,
            user_hooks: false,
            windows: true,
            spawn: f.spawn.clone(),
            tools: Default::default(),
        });
        let session = server.open(
            CodexThread {
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
                leave_plan: false,
                cwd: ".".into(),
                model: None,
                effort: None,
                resume,
                instructions: None,
                research: None,
                user_name: None,
                group_instructions: None,
                permissions: Permissions::FullAccess,
                tuning: Default::default(),
            },
            Arc::new(move |e| drop(tx.send(e))),
        );
        tokio::time::timeout(Duration::from_secs(3), async { while !matches!(rx.recv().await, Some(EngineEvent::Started { .. })) {} })
            .await
            .unwrap();
        assert_eq!(f.wire.lines_with("config/read"), 0, "the MCP servers are not listed in order to be switched off");
        assert_eq!(f.args.lock().unwrap()[0], ["app-server"]);
        let messages = f.wire.messages();
        let thread = messages.iter().find(|msg| msg["method"] == method).unwrap();
        assert_eq!(thread["params"]["sandbox"], "danger-full-access");
        assert_eq!(thread["params"]["config"], json!({"web_search": "disabled"}), "no profile and no tool override");
        assert!(f.mismatches.lock().unwrap().is_empty());
        session.close();
        server.retire();
    }
}

#[tokio::test]
async fn r124_a_full_access_thread_hands_a_residual_approval_to_the_engine_and_never_answers_it_alone() {
    for area in [Area::Chat, Area::Project] {
        let entries = fixture("codex", "tools")
            .into_iter()
            .map(|mut e| {
                if e.input && e.msg.get("result").is_some() && e.msg["result"].get("decision").is_some() {
                    e.msg["result"]["decision"] = "accept".into();
                }
                e
            })
            .collect();
        let (events, mismatches) = drive(
            entries,
            |spawn, sink| {
                let server = CodexServer::start(CodexServerOptions {
                    permissions: Permissions::FullAccess,
                    cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                    cwd: ".".into(),
                    area,
                    user_hooks: false,
                    windows: false,
                    spawn,
                    tools: Default::default(),
                });
                server.open(
                    CodexThread {
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        verify: false,
                        progress: false,
                        leave_plan: false,
                        cwd: ".".into(),
                        model: None,
                        effort: None,
                        resume: None,
                        instructions: None,
                        research: None,
                        user_name: None,
                        group_instructions: None,
                        permissions: Permissions::FullAccess,
                        tuning: Default::default(),
                    },
                    sink,
                )
            },
            "Create a.txt containing one. Then create b.txt containing two. Make each file in a separate step.",
            |event, session| {
                // What the engine does for full access: it answers yes at once. The driver itself writes nothing before.
                if let EngineEvent::Permission { request_id, .. } = event {
                    drop(session.answer(request_id, true));
                }
                matches!(event, EngineEvent::Done { .. })
            },
        )
        .await;
        assert!(mismatches.is_empty(), "{mismatches:?}");
        assert!(events.iter().any(|e| matches!(e, EngineEvent::Permission { .. })), "the request reaches the engine, which decides");
    }
}

#[test]
fn codex_work_log_items_from_a_recorded_turn_keep_their_raw_shape_for_the_screen_to_tidy() {
    let mut state = CodexTurnState::default();
    let events: Vec<EngineEvent> = fixture("codex", "worklog").iter().flat_map(|e| parse_codex_notification(&e.msg, &mut state)).collect();
    let thinking: String = events.iter().filter_map(|e| if let EngineEvent::Thinking(t) = e { Some(t.as_str()) } else { None }).collect();
    assert_eq!(thinking, "**Minimizing output verbosity****Assessing time budget and test approach****Preparing for final retry**");
    let tool = events.iter().find_map(|e| if let EngineEvent::Tool { name, input, .. } = e { Some((name, input)) } else { None });
    let (name, input) = tool.expect("the command item becomes a tool step");
    assert_eq!(name, "commandExecution");
    let command = input.as_str().expect("a command is a string");
    assert!(command.starts_with("\"C:\\\\WINDOWS\\\\System32\\\\WindowsPowerShell"), "{command}");
    assert!(command.contains("Set-Content -LiteralPath 'a.txt'"), "{command}");
    let text: String = events.iter().filter_map(|e| if let EngineEvent::Text(t) = e { Some(t.as_str()) } else { None }).collect();
    assert_eq!(text.matches("The window control did not connect.").count(), 2, "the engine keeps both; the screen shows it once");
}

// R140: the vendors' own range of modes.

fn mode_thread(permissions: Permissions, tuning: faber_engine::model::Tuning) -> CodexThread {
    CodexThread {
        helper: Default::default(),
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: "project".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        research: None,
        user_name: None,
        group_instructions: None,
        permissions,
        tuning,
    }
}

#[tokio::test]
async fn slash_skills_are_requested_once_with_the_thread_folder_and_become_commands() {
    let f = fake(fixture("codex", "slash-skills"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::Ask,
        cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Chat,
        user_hooks: false,
        windows: false,
        spawn: f.spawn.clone(),
        tools: Default::default(),
    });
    let session = server.open(mode_thread(Permissions::Ask, Default::default()), Arc::new(move |event| drop(tx.send(event))));
    assert!(matches!(rx.recv().await, Some(EngineEvent::Started { .. })));
    let event = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await.unwrap().unwrap();
    let EngineEvent::Commands(commands) = event else { panic!("skills answer advertises commands") };
    assert_eq!(
        commands,
        [
            faber_engine::model::Command { name: "garden-plan".into(), skill: true, note: Some("Plan a garden".into()) },
            faber_engine::model::Command { name: "tidy".into(), skill: true, note: Some("Tidy the folder.".into()) },
        ]
    );
    let requests: Vec<_> = f.wire.messages().into_iter().filter(|m| m["method"] == "skills/list").collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["params"], json!({"cwds":["project"]}));
    assert!(f.mismatches.lock().unwrap().is_empty());
    session.close();
    server.retire();
}

#[tokio::test]
async fn slash_failed_or_odd_skills_answers_are_silent_and_notes_are_bounded() {
    for result in [
        json!({"error":{"message":"unavailable"}}),
        json!({"result":{"data":null}}),
        json!({"result":{"data":[{"cwd":"other","skills":[]}]}}),
        json!({"result":{"data":[{"cwd":"project","skills":[{"name":"long","enabled":true,"shortDescription":"é".repeat(140)}]}]}}),
    ] {
        let (spawn, mut input, mut output) = support::controlled();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let server = CodexServer::start(CodexServerOptions {
            permissions: Permissions::Ask,
            cli: faber_engine::cli::Cli { command: "fake".into(), prefix: vec![] },
            cwd: ".".into(),
            area: Area::Chat,
            user_hooks: false,
            windows: false,
            spawn,
            tools: Default::default(),
        });
        let session = server.open(mode_thread(Permissions::Ask, Default::default()), Arc::new(move |e| drop(tx.send(e))));
        support::expect_method(&mut input, "initialize").await;
        support::emit(&mut output, json!({"id":1,"result":{}})).await;
        support::expect_method(&mut input, "initialized").await;
        support::expect_method(&mut input, "thread/start").await;
        support::emit(&mut output, json!({"id":2,"result":{"thread":{"id":"t"}}})).await;
        assert!(matches!(rx.recv().await, Some(EngineEvent::Started { .. })));
        let request = support::expect_method(&mut input, "skills/list").await;
        let mut answer = result;
        answer["id"] = request["id"].clone();
        let long = answer["result"]["data"][0]["skills"][0]["name"] == "long";
        support::emit(&mut output, answer).await;
        session.send(UserTurn::text("still works"));
        support::expect_method(&mut input, "turn/start").await;
        if long {
            let commands = loop {
                let event = rx.recv().await.expect("a valid list is delivered");
                if !visible(&event) {
                    continue;
                }
                let EngineEvent::Commands(commands) = event else { panic!("a valid list is delivered") };
                break commands;
            };
            assert_eq!(commands[0].note.as_ref().unwrap().chars().count(), 120);
        }
        while let Ok(event) = rx.try_recv() {
            assert!(!visible(&event), "a failed list never becomes an event on screen: {event:?}");
        }
        session.close();
        server.retire();
    }
}

#[test]
fn r140_each_mode_maps_to_the_nearest_codex_policy_sandbox_and_reviewer() {
    let table = [
        (Permissions::Ask, "untrusted", "workspace-write", Some("user")),
        (Permissions::AcceptEdits, "on-request", "workspace-write", Some("user")),
        (Permissions::Plan, "on-request", "read-only", Some("user")),
        (Permissions::Auto, "on-request", "workspace-write", Some("auto_review")),
        (Permissions::FullAccess, "never", "danger-full-access", None),
    ];
    for area in [Area::Chat, Area::Project] {
        for (permissions, approval, sandbox, reviewer) in table {
            for resume in [None, Some("t1".to_string())] {
                let t = CodexThread { resume, ..mode_thread(permissions, Default::default()) };
                let (_, params) = thread_params(area, &t);
                assert_eq!(params["approvalPolicy"], approval, "{permissions:?} {area:?}");
                assert_eq!(params["sandbox"], sandbox, "{permissions:?} {area:?}");
                assert_eq!(params.get("approvalsReviewer").and_then(|r| r.as_str()), reviewer, "{permissions:?} {area:?}");
            }
        }
    }
}

#[test]
fn r140_every_guarded_mode_starts_the_server_the_way_ask_does() {
    for permissions in [Permissions::AcceptEdits, Permissions::Plan, Permissions::Auto] {
        for area in [Area::Chat, Area::Project] {
            for windows in [false, true] {
                assert_eq!(
                    codex_args(area, windows, false, permissions),
                    codex_args(area, windows, false, Permissions::Ask),
                    "{permissions:?} {area:?} {windows}"
                );
            }
        }
        assert!(!codex_args(Area::Project, false, true, permissions).join(" ").contains("hooks"), "the hooks switch still works");
        assert!(codex_args(Area::Chat, false, false, permissions).join(" ").contains("mcp_servers={}"));
    }
}

#[test]
fn r140_web_search_defaults_to_live_and_summary_and_verbosity_to_the_models_own() {
    use faber_engine::model::{ReasoningSummary, ReplyStyle, Tuning, WebSearch};
    for permissions in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Plan, Permissions::Auto, Permissions::FullAccess] {
        for resume in [None, Some("t1".to_string())] {
            let t = CodexThread { resume, ..mode_thread(permissions, faber_engine::model::Settings::default().tuning()) };
            let (_, params) = thread_params(Area::Chat, &t);
            assert_eq!(params["config"], json!({"web_search": "live"}), "{permissions:?}: nothing but web search is set by default");
            let (_, bare) = thread_params(Area::Chat, &CodexThread { tuning: Tuning::default(), ..t });
            assert_eq!(bare["config"], json!({"web_search": "disabled"}), "a launch nobody chose for gets no web");
        }
        for (web, name) in [(WebSearch::Off, "disabled"), (WebSearch::Cached, "cached"), (WebSearch::Live, "live")] {
            let (_, params) = thread_params(Area::Project, &mode_thread(permissions, Tuning { web, ..Default::default() }));
            assert_eq!(params["config"]["web_search"], name);
        }
    }
    let chosen = Tuning { web: WebSearch::Live, summary: ReasoningSummary::Detailed, reply_style: ReplyStyle::Short, ..Default::default() };
    let (_, params) = thread_params(Area::Chat, &mode_thread(Permissions::Auto, chosen.clone()));
    assert_eq!(params["config"], json!({"web_search": "live", "model_reasoning_summary": "detailed", "model_verbosity": "low"}));
    for (summary, name) in [
        (ReasoningSummary::Auto, "auto"),
        (ReasoningSummary::Concise, "concise"),
        (ReasoningSummary::Detailed, "detailed"),
        (ReasoningSummary::Hidden, "none"),
    ] {
        let (_, params) = thread_params(Area::Chat, &mode_thread(Permissions::FullAccess, Tuning { summary, ..Default::default() }));
        assert_eq!(params["config"]["model_reasoning_summary"], name);
    }
    for (reply_style, name) in [(ReplyStyle::Short, "low"), (ReplyStyle::Explained, "high")] {
        let (_, params) = thread_params(Area::Chat, &mode_thread(Permissions::FullAccess, Tuning { reply_style, ..Default::default() }));
        assert_eq!(params["config"]["model_verbosity"], name);
    }
}

#[test]
fn r140_every_isolated_or_internal_codex_thread_has_web_search_disabled_and_takes_no_setting() {
    use faber_engine::model::{ReasoningSummary, ReplyStyle, Tuning, WebSearch};
    let chosen =
        Tuning { web: WebSearch::Live, summary: ReasoningSummary::Detailed, reply_style: ReplyStyle::Explained, ..Default::default() };
    let keys = |params: &serde_json::Value| {
        ["web_search", "model_reasoning_summary", "model_verbosity"]
            .map(|k| params["config"].get(k).and_then(|v| v.as_str().map(String::from)))
    };
    let closed = [Some("disabled".to_string()), None, None];
    for area in [Area::Chat, Area::Project] {
        for resume in [None, Some("t1".to_string())] {
            let isolated = CodexThread { resume: resume.clone(), ..mode_thread(Permissions::Isolated, chosen.clone()) };
            assert_eq!(keys(&thread_params(area, &isolated).1), closed, "{area:?} isolated");
            for permissions in [
                Permissions::Ask,
                Permissions::AcceptEdits,
                Permissions::Plan,
                Permissions::Auto,
                Permissions::FullAccess,
                Permissions::Isolated,
            ] {
                let side =
                    CodexThread { resume: resume.clone(), instructions: Some("Route.".into()), ..mode_thread(permissions, chosen.clone()) };
                let (_, side) = thread_params(area, &side);
                assert_eq!(keys(&side), closed, "{area:?} side question under {permissions:?}");
                assert_eq!(side["approvalPolicy"], "never", "a side question never asks, whatever the mode");
            }
        }
    }
    for writable in [false, true] {
        assert_eq!(faber_engine::permissions::sandbox_config(".".as_ref(), writable)["web_search"], "disabled", "{writable}");
    }
}

// R161: Faber's helper tool on Codex.

#[test]
fn r161_only_a_project_agent_gets_the_helper_tool_and_a_helper_keeps_no_history() {
    let with = |helper, instructions: Option<&str>, resume: Option<&str>| {
        let t = CodexThread {
            helper,
            instructions: instructions.map(String::from),
            resume: resume.map(String::from),
            ..mode_thread(Permissions::Isolated, Default::default())
        };
        thread_params(Area::Project, &t).1
    };
    let names = |p: &serde_json::Value| -> Vec<String> {
        p["dynamicTools"].as_array().into_iter().flatten().map(|t| t["name"].as_str().unwrap().to_string()).collect()
    };
    use faber_engine::helper::Helper;
    assert_eq!(names(&with(Helper::Offers, None, None)), ["run_command", "helper"], "beside the tool the mode gives");
    assert_eq!(names(&with(Helper::Off, None, None)), ["run_command"]);
    assert!(names(&with(Helper::Offers, Some("Route."), None)).is_empty(), "a side question never gets it");
    assert!(with(Helper::Offers, None, Some("t1")).get("dynamicTools").is_none(), "a resumed thread keeps the tools it started with");
    assert_eq!(with(Helper::Is, None, None)["ephemeral"], true);
    assert!(with(Helper::Offers, None, None).get("ephemeral").is_none());
}

/// R250: a quick question on the side is a thread of its own that reads Faber's record: no shell, no dynamic tool, never asks,
/// no network or writes, and nothing left to resume.
#[test]
fn r250_a_codex_quick_question_runs_nothing_and_leaves_no_history() {
    use faber_engine::helper::Helper;
    for permissions in [Permissions::FullAccess, Permissions::Isolated, Permissions::Ask] {
        let t = CodexThread {
            helper: Helper::Aside,
            instructions: Some("Record.".into()),
            browse: true,
            verify: true,
            ..mode_thread(permissions, Default::default())
        };
        let p = thread_params(Area::Project, &t).1;
        assert_eq!(p["ephemeral"], true);
        assert_eq!(p["approvalPolicy"], "never");
        assert_eq!(p["config"]["features.shell_tool"], false);
        assert!(p.get("dynamicTools").is_none() && p.get("sandbox").is_none(), "{p}");
        let profile = p["config"]["default_permissions"].as_str().unwrap();
        let profile = &p["config"][format!("permissions.{profile}")];
        assert_eq!(profile["network"]["enabled"], false);
        assert_eq!(profile["filesystem"][":workspace_roots"]["."], "read");
        assert_eq!(p["baseInstructions"], "Record.");
    }
}

/// R161 step 7: Faber's browser is a dynamic tool beside the others, offered only where the engine says so.
#[test]
fn r161_the_browser_is_a_dynamic_tool_where_it_is_offered() {
    use faber_engine::helper::Helper;
    let names = |helper, browse, instructions: Option<&str>| -> Vec<String> {
        let t = CodexThread {
            helper,
            browse,
            instructions: instructions.map(String::from),
            ..mode_thread(Permissions::FullAccess, Default::default())
        };
        let p = thread_params(Area::Project, &t).1;
        p["dynamicTools"].as_array().into_iter().flatten().map(|t| t["name"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(names(Helper::Offers, true, None), ["helper", "browse"]);
    assert_eq!(names(Helper::Off, true, None), ["browse"]);
    assert!(names(Helper::Off, false, None).is_empty());
    assert!(names(Helper::Off, true, Some("Route.")).is_empty(), "a side question never gets it");
}

#[tokio::test]
async fn r161_codex_calling_the_helper_waits_for_its_report_and_gets_it_as_the_tools_result() {
    let thread = CodexThread {
        helper: faber_engine::helper::Helper::Offers,
        browse: false,
        conversations: false,
        verify: false,
        progress: false,
        leave_plan: false,
        cwd: ".".into(),
        model: Some("gpt-6-luna".into()),
        ..mode_thread(Permissions::FullAccess, Default::default())
    };
    let first =
        "Ask the helper tool to check config.toml for risky values (title \"Check config\"). Then tell me in one sentence what it found.";
    let mut asked = None;
    let (events, mismatches) = drive(
        fixture("codex", "helper-call"),
        |spawn, sink| {
            let opts = CodexServerOptions {
                permissions: Permissions::FullAccess,
                cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
                cwd: ".".into(),
                area: Area::Project,
                user_hooks: false,
                windows: false,
                spawn,
                tools: Default::default(),
            };
            CodexServer::start(opts).open(thread, sink)
        },
        first,
        |e, session| {
            if let EngineEvent::HelperCall { id, input } = e {
                asked = Some((id.clone(), input["title"].clone()));
                session.helper_reply(id, true, "config.toml sets timeout = 30 and retries = 3; nothing risky.".into());
            }
            matches!(e, EngineEvent::Done { .. })
        },
    )
    .await;
    assert!(mismatches.is_empty(), "{mismatches:?}");
    let (id, title) = asked.expect("the call reaches the engine");
    assert_eq!(title, "Check config");
    let tool = events.iter().find_map(|e| if let EngineEvent::Tool { id, name, .. } = e { Some((id, name)) } else { None });
    assert_eq!(tool, Some((&id, &"Helper".to_string())), "the call is one step, named by its tool call id");
    assert!(text_of(&events).contains("timeout = 30"), "{}", text_of(&events));
}

#[tokio::test]
async fn the_catalog_s_skill_root_is_set_once_before_the_first_thread_opens_and_its_servers_go_with_each_thread() {
    let entries = vec![
        Entry { input: true, msg: json!({"id":1,"method":"initialize"}) },
        Entry { input: false, msg: json!({"id":1,"result":{}}) },
        Entry { input: true, msg: json!({"method":"initialized"}) },
        Entry {
            input: true,
            msg: json!({"id":"faber-skills","method":"skills/extraRoots/set","params":{"extraRoots":["C:/data/tools/plugin/skills"]}}),
        },
        Entry { input: false, msg: json!({"id":"faber-skills","result":{}}) },
        Entry { input: true, msg: json!({"id":2,"method":"thread/start"}) },
        Entry { input: false, msg: json!({"id":2,"result":{"thread":{"id":"thread","model":"model"}}}) },
        // A store that could not be made clean takes the root away from the running server.
        Entry { input: true, msg: json!({"id":"faber-skills","method":"skills/extraRoots/set","params":{"extraRoots":[]}}) },
        Entry { input: false, msg: json!({"id":"faber-skills","result":{}}) },
    ];
    let f = fake(entries);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::FullAccess,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Project,
        user_hooks: false,
        windows: true,
        spawn: f.spawn.clone(),
        tools: tools(Some("C:/data/tools/plugin/skills")),
    });
    let session = server.open(mode_thread(Permissions::FullAccess, Default::default()), Arc::new(move |e| drop(tx.send(e))));
    tokio::time::timeout(Duration::from_secs(3), async { while !matches!(rx.recv().await, Some(EngineEvent::Started { .. })) {} })
        .await
        .unwrap();
    let messages = f.wire.messages();
    let at = |method: &str| messages.iter().position(|m| m["method"] == method).unwrap();
    assert!(at("initialized") < at("skills/extraRoots/set") && at("skills/extraRoots/set") < at("thread/start"));
    assert_eq!(messages.iter().filter(|m| m["method"] == "skills/extraRoots/set").count(), 1);
    let start = &messages[at("thread/start")]["params"]["config"];
    assert_eq!(
        start["mcp_servers.measure"],
        json!({"command":"C:/node/node.exe","args":["C:/data/tools/servers/measure/run.js","--headless"],"env":{"NO_STATS":"1"},"enabled":true}),
        "{start}"
    );
    assert!(start.get("mcp_servers").is_none(), "the person's own servers are left as they are: {start}");
    server.tools(tools(Some("C:/data/tools/plugin/skills")));
    server.tools(tools(None));
    server.tools(tools(None));
    let sets = || f.wire.messages().into_iter().filter(|m| m["method"] == "skills/extraRoots/set").collect::<Vec<_>>();
    tokio::time::timeout(Duration::from_secs(3), async {
        while sets().len() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let sets = sets();
    assert_eq!(sets.len(), 2, "the same root is never sent twice: {sets:?}");
    assert_eq!(sets[1]["params"]["extraRoots"], json!([]));
    assert!(f.mismatches.lock().unwrap().is_empty(), "{:?}", f.mismatches.lock().unwrap());
    session.close();
    server.retire();
}

/// A store's delivery with one server and, when given, a skill root.
fn tools(skills: Option<&str>) -> faber_engine::tools::Delivery {
    faber_engine::tools::Delivery {
        catalog_ready: true,
        lease: None,
        prepared: None,
        catalog_root: None,
        plugin: None,
        skills: skills.map(Into::into),
        skill_ids: None,
        servers: vec![faber_engine::tools::Launch {
            name: "measure".into(),
            command: "C:/node/node.exe".into(),
            args: vec!["C:/data/tools/servers/measure/run.js".into(), "--headless".into()],
            env: [("NO_STATS".to_string(), "1".to_string())].into(),
        }],
        disabled_servers: vec![],
        guide: Default::default(),
        card: None,
    }
}

#[test]
fn reply_style_does_not_replace_an_internal_codex_tasks_fixed_instructions() {
    use faber_engine::model::{ReplyStyle, Settings, Tuning};
    let plain = CodexThread { instructions: Some("Answer in one word.".into()), ..mode_thread(Permissions::Ask, Tuning::default()) };
    for style in [ReplyStyle::Default, ReplyStyle::Short, ReplyStyle::Explained, ReplyStyle::Own] {
        let tuning =
            Settings { reply_style: Some(style), reply_style_text: Some("My custom style.".into()), ..Default::default() }.tuning();
        let (_, params) = thread_params(Area::Chat, &CodexThread { tuning, ..plain.clone() });
        assert_eq!(params["baseInstructions"], "Answer in one word.");
        assert!(params.get("developerInstructions").is_none());
        assert!(params["config"].get("model_verbosity").is_none());
        assert_eq!(params["config"]["web_search"], "disabled");
    }
}

/// The 0.162.1 app-server schema's paginated status list; the catalog never reads an account's own configuration.
#[tokio::test(start_paused = true)]
async fn mcp_servers_codex_lists_only_on_request_and_keeps_project_launch_choices_separate() {
    let f = fake(fixture("codex", "mcp-servers"));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let server = CodexServer::start(CodexServerOptions {
        permissions: Permissions::FullAccess,
        cli: faber_engine::cli::Cli { command: "codex".into(), prefix: vec![] },
        cwd: ".".into(),
        area: Area::Project,
        user_hooks: false,
        windows: true,
        spawn: f.spawn.clone(),
        tools: tools(None),
    });
    let sink = Arc::new(move |e| drop(tx.send(e)));
    let on = server.open_with_tools(mode_thread(Permissions::FullAccess, Default::default()), sink.clone(), tools(None));
    let mut disabled = tools(None);
    disabled.disabled_servers.push("measure".into());
    let off = server.open_with_tools(mode_thread(Permissions::FullAccess, Default::default()), sink, disabled);
    for _ in 0..2 {
        assert!(matches!(rx.recv().await, Some(EngineEvent::Started { .. })));
    }
    tokio::time::advance(Duration::from_secs(600)).await;
    tokio::task::yield_now().await;
    let messages = f.wire.messages();
    assert!(!messages.iter().any(|m| m["method"] == "mcpServerStatus/list"), "the closed catalog never asks for status");
    assert!(!messages.iter().any(|m| m["method"] == "config/read"), "this list never requests account config");
    let starts = messages.iter().filter(|m| m["method"] == "thread/start").collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0]["params"]["config"]["mcp_servers.measure"]["enabled"], true);
    assert_eq!(starts[1]["params"]["config"]["mcp_servers.measure"]["enabled"], false);
    assert_eq!(starts[1]["params"]["config"]["mcp_servers.measure"]["command"], "C:/node/node.exe");
    assert!(starts.iter().all(|m| m["params"]["config"].get("mcp_servers").is_none()), "own servers stay configured");

    let rows = tokio::time::timeout(Duration::from_secs(3), on.mcp_status()).await.unwrap().unwrap().unwrap();
    assert_eq!(
        rows,
        vec![
            faber_engine::mcp_servers::Reported { name: "measure".into(), status: Some("failed".into()) },
            faber_engine::mcp_servers::Reported { name: "own".into(), status: Some("connected".into()) },
            faber_engine::mcp_servers::Reported { name: "project".into(), status: Some("disabled".into()) },
        ]
    );
    assert!(tokio::time::timeout(Duration::from_secs(3), off.mcp_status()).await.unwrap().unwrap().is_none());
    assert!(rx.try_recv().is_err(), "an unsupported status query is never a conversation error");
    let messages = f.wire.messages();
    let requests = messages.iter().filter(|m| m["method"] == "mcpServerStatus/list").collect::<Vec<_>>();
    assert_eq!(requests.len(), 3);
    for (i, request) in requests.iter().enumerate() {
        assert_eq!(request["params"]["threadId"], if i < 2 { "thread-on" } else { "thread-off" });
        assert_eq!(request["params"]["detail"], "toolsAndAuthOnly");
        assert_eq!(request["params"]["limit"], 100);
    }
    assert_eq!(requests[1]["params"]["cursor"], "second-page");
    assert!(f.mismatches.lock().unwrap().is_empty(), "{:?}", f.mismatches.lock().unwrap());
    on.close();
    off.close();
    server.retire();
}
