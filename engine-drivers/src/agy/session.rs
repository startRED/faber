use super::*;
use crate::events::{Delivery, Session, SessionCommand};
use crate::model::Permissions;
use crate::proc::{self, Spawner, TurnWatch};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::Instant;

/// What this driver cannot keep away from a protected folder, for the engine and the lead to read.
pub const PROTECTED_FOLDER_LIMIT: &str = "Antigravity cannot keep out a protected folder within the working folder, within its      own temp folder, within an approved command's or MCP tool's reach, behind a junction changed after the request was      judged, or named by a tool whose input does not carry `file_path`.";
pub const BRIDGE_CLOSED: &str = "agy.bridgeClosed";

pub struct AgyOptions {
    pub bin: String,
    pub cwd: PathBuf,
    pub data: PathBuf,
    pub model: Option<String>,
    pub effort: String,
    pub resume: Option<String>,
    pub permissions: Permissions,
    pub builtin: Option<(PathBuf, crate::helper::Tools)>,
    pub leads: bool,
    pub spawn: Spawner,
}

fn environment(tmp: &Path) -> Vec<(&'static str, Option<OsString>)> {
    let mut changes = [
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "GOOGLE_GENAI_USE_VERTEXAI",
        "GOOGLE_CLOUD_PROJECT",
        "GOOGLE_CLOUD_LOCATION",
        "GCLOUD_PROJECT",
        "GOOGLE_CLOUD_QUOTA_PROJECT",
        "GOOGLE_OAUTH_ACCESS_TOKEN",
        "CLOUDSDK_AUTH_ACCESS_TOKEN",
    ]
    .into_iter()
    .map(|name| (name, None))
    .collect::<Vec<_>>();
    for name in ["TEMP", "TMP"].into_iter().chain((!cfg!(windows)).then_some("TMPDIR")) {
        changes.push((name, Some(tmp.as_os_str().to_owned())));
    }
    changes
}

fn open_folder(base: &Path, name: &str) -> io::Result<PathBuf> {
    let folder = base.join(name);
    match std::fs::create_dir(&folder) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let meta = folder.symlink_metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("Antigravity folder is a reparse point"));
        }
    }
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(io::Error::other("Antigravity folder is not a regular directory"));
    }
    Ok(folder)
}

fn real_path(cwd: &Path, path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let path = cwd.join(path);
    if !path.is_absolute() {
        return None;
    }
    let mut parent = path.as_path();
    let mut missing = Vec::new();
    let mut real = loop {
        match parent.canonicalize() {
            Ok(real) => break real,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if !parent.symlink_metadata().is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
                    return None;
                }
                missing.push(parent.file_name()?);
                parent = parent.parent()?;
            }
            Err(_) => return None,
        }
    };
    if !missing.is_empty() && !real.is_dir() {
        return None;
    }
    real.extend(missing.into_iter().rev());
    let path = real.into_os_string().into_string().ok()?;
    Some(
        path.strip_prefix(r"\\?\UNC\").map(|p| format!(r"\\{p}")).or_else(|| path.strip_prefix(r"\\?\").map(str::to_owned)).unwrap_or(path),
    )
}

pub fn start(opts: AgyOptions, servers: &[Launch], mut on_event: crate::claude::EventSink) -> Result<Session, String> {
    if !matches!(opts.permissions, Permissions::Ask | Permissions::FullAccess) {
        return Err("error.notConnected".into());
    }
    // A tool server runs outside any guard, so only Full access gets one (R195), as with the other drivers.
    let mut servers = if opts.permissions == Permissions::FullAccess { servers.to_vec() } else { Vec::new() };
    let mut bridge = None;
    let mut tools = crate::helper::Tools::default();
    let mut builtin_name = None;
    if let Some((command, offered)) = opts.builtin.filter(|(_, offered)| offered.any())
        && let Ok(link) =
            bridge::Bridge::bind().inspect_err(|error| eprintln!("faber: Antigravity's local tool relay could not start: {error}"))
    {
        on_event = crate::codex::redact_sink(on_event, link.secret.clone());
        // A project or profile can already name a server "faber". Only this fresh name earns ACP auto-approval.
        let name = format!("faber{}", uuid::Uuid::new_v4().simple());
        servers.push(Launch {
            name: name.clone(),
            command,
            args: vec![stdio::FLAG.into(), link.port.to_string()],
            env: [(bridge::ENV.into(), link.secret.clone())].into(),
        });
        builtin_name = Some(name);
        tools = offered;
        bridge = Some(link);
    }
    let cwd = opts.cwd.to_str().ok_or("error.driverStopped")?;
    let folder = open_folder(&opts.data, "antigravity").map_err(|_| "error.driverStopped")?;
    let profile = open_folder(&folder, "profile").map_err(|_| "error.driverStopped")?;
    // One temp folder per process, new each time and removed with it: the CLI reads and writes its temp folder unasked,
    // so nothing one conversation left there may wait for the next.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tmp = open_folder(&folder, "tmp").map_err(|_| "error.driverStopped")?.join(format!("{}-{stamp}", std::process::id()));
    std::fs::create_dir(&tmp).map_err(|_| "error.driverStopped")?;
    let args = ["acp".into(), "--gemini-dir".into(), profile.to_str().ok_or("error.driverStopped")?.into()];
    let started = proc::with_env(environment(&tmp), || (opts.spawn)(&opts.bin, &args, &opts.cwd));
    let tree = started.as_ref().ok().and_then(|p| p.contained.as_ref()).and_then(proc::Containment::owned_tree);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let open = match &opts.resume {
        Some(id) => session_resume(2, id, cwd, &servers),
        None => session_new(2, cwd, &servers),
    };
    let secret = bridge.as_ref().map(|link| link.secret.clone());

    tokio::spawn(async move {
        let mut child = match started {
            Ok(child) => child,
            Err(error) => {
                eprintln!("faber: Antigravity could not start: {error}");
                let _ = std::fs::remove_dir_all(&tmp);
                on_event(EngineEvent::Error { message: "error.driverStopped".into(), fatal: true });
                return on_event(EngineEvent::WriterExit { confirmed: true });
            }
        };
        let tail = proc::stderr_tail(child.stderr.take());
        let (requests, mut calls_in) = tokio::sync::mpsc::channel(1);
        let (calls_out, replies) = tokio::sync::mpsc::channel(32);
        let mut link = bridge.map(|bridge| Box::pin(bridge.run(requests, replies)));
        let mut calls = bridge::Calls::default();
        let mut lines = BufReader::new(child.stdout).lines();
        let mut reader = AcpReader::new();
        let builtin_prefix = builtin_name.as_ref().map(|name| format!("mcp__{name}__"));
        let mut bridge_ready = false;
        let mut queued = VecDeque::new();
        let mut startup = VecDeque::from([initialize(1, env!("CARGO_PKG_VERSION")), open]);
        let mut waiting = None;
        let mut next_id = 2;
        let mut ready = false;
        let mut retiring = false;
        let mut bridge_closed = false;
        let mut guided = false;
        let mut watch = TurnWatch::default();
        let mut startup_deadline = Some(Instant::now() + Duration::from_secs(30));
        let terminal = 'dialog: loop {
            if builtin_name.is_some() && link.is_none() && !bridge_closed {
                if !ready {
                    break Some("error.driverStopped");
                }
                // Let the current answer finish. The next turn resumes with a fresh tool server and secret.
                bridge_closed = true;
                retiring = true;
                bridge_ready = false;
                drop(calls.cancel(&on_event));
                on_event(EngineEvent::Reason(BRIDGE_CLOSED));
                if !watch.active() {
                    // Core may already have queued a prompt that we have not read. It ignores this error
                    // after retiring a truly idle session and settles any such prompt that raced the close.
                    break Some("error.driverStopped");
                }
            }
            let message = if waiting.is_none() {
                if let Some(setup) = startup.pop_front() {
                    Some(setup)
                } else if ready && !retiring {
                    queued.pop_front().map(|turn: crate::events::UserTurn| {
                        next_id += 1;
                        watch.begin();
                        let text = if let Some(name) = builtin_name.as_deref().filter(|_| !guided) {
                            guided = true;
                            let mut instructions =
                                format!("Use the tools registered under mcp_{name}_ whenever these instructions refer to a Faber tool.");
                            if tools.progress {
                                instructions.push_str(crate::persona::PROJECT_DETAILS);
                                instructions.push_str(crate::persona::BIGGER_OFFER);
                            }
                            if tools.helper {
                                instructions.push_str(crate::helper::HELPERS);
                            }
                            if opts.leads {
                                instructions.push_str(crate::persona::LEADS);
                            }
                            format!("{instructions}\n\nUser request:\n{}", turn.text)
                        } else {
                            turn.text
                        };
                        let mut msg = prompt(next_id, reader.session(), &text);
                        if let Some(parts) = msg["params"]["prompt"].as_array_mut() {
                            parts.extend(
                                turn.images
                                    .into_iter()
                                    .map(|image| json!({ "type": "image", "mimeType": image.media_type, "data": image.base64 })),
                            );
                        }
                        msg
                    })
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(msg) = message {
                waiting = msg["id"].as_u64();
                if proc::write_line(&mut *child.stdin, &msg).await.is_err() {
                    break Some("error.driverStopped");
                }
                reader.sent(&msg);
            }

            tokio::select! {
                _ = proc::wait_deadline(watch.deadline().or(startup_deadline)) => {
                    break Some(if watch.stopping() { "reason.interrupted" } else { "error.driverStopped" });
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else {
                        let code = tokio::time::timeout(Duration::from_millis(250), proc::exit_code(&mut child.child)).await;
                        let stderr = tail.lock().unwrap_or_else(|e| e.into_inner());
                        let scrubbed = secret.as_ref().map(|secret| stderr.replace(secret, "[redacted]"));
                        let detail = scrubbed.as_deref().unwrap_or(stderr.as_str());
                        eprintln!("faber: Antigravity exited (code {}): {detail}", code.as_deref().unwrap_or("unknown"));
                        break Some(if watch.stopping() { "reason.interrupted" } else { "error.driverStopped" });
                    };
                    let safe = secret.as_ref().filter(|secret| line.contains(secret.as_str())).map(|secret| line.replace(secret, "[redacted]"));
                    let Ok(msg) = serde_json::from_str::<Value>(safe.as_deref().unwrap_or(&line)) else {
                        reader.read(safe.as_deref().unwrap_or(&line));
                        continue;
                    };
                    let matched = msg["method"].is_null() && waiting.is_some() && msg["id"].as_u64() == waiting;
                    let events = reader.read_value(&msg);
                    let mut configured = false;
                    if matched {
                        waiting = None;
                        if !ready && msg["error"].is_object() {
                            let limit = msg["error"].to_string().contains("RESOURCE_EXHAUSTED").then_some("reason.fail.limit");
                            let reason = events.iter().find_map(|event| match event {
                                EngineEvent::Reason(key) => Some(*key),
                                _ => None,
                            });
                            if let Some(key) = reason.or(limit) {
                                on_event(EngineEvent::Reason(key));
                            }
                            // Opening a new conversation lost nothing; only a saved one that does not come back did.
                            let lost = msg["id"] == 2 && opts.resume.is_some();
                            break Some(reason.or(limit).unwrap_or(if lost { "error.sessionLost" } else { "error.driverStopped" }));
                        }
                        if msg["id"] == 2 {
                            if reader.session().is_empty() || opts.resume.as_deref().is_some_and(|id| id != reader.session()) {
                                break Some("error.sessionLost");
                            }
                            if let Some(model) = opts.model.as_deref().filter(|model| !model.is_empty()) {
                                next_id += 1;
                                if msg["result"]["models"]["availableModels"].as_array().into_iter().flatten().any(|m| m["modelId"] == model) {
                                    startup.push_back(set_model(next_id, reader.session(), model));
                                } else {
                                    startup.push_back(set_config_option(next_id, reader.session(), "model", model));
                                    next_id += 1;
                                    startup.push_back(set_config_option(next_id, reader.session(), "thought_level", &opts.effort));
                                }
                            }
                            next_id += 1;
                            startup.push_back(set_mode(next_id, reader.session(), "default"));
                        }
                        if !ready {
                            if !msg["result"].is_object() {
                                break Some("error.driverStopped");
                            }
                            ready = startup.is_empty();
                            if ready {
                                startup_deadline = None;
                                configured = true;
                            }
                        } else {
                            watch.finish();
                            for reply in calls.cancel(&on_event) {
                                let _ = calls_out.try_send(reply);
                            }
                        }
                    }
                    // A question in a form the reader cannot show is declined: agy would wait for its answer forever.
                    if msg["method"] == "elicitation/create" && events.is_empty()
                        && proc::write_line(&mut *child.stdin, &elicitation_answer(&msg["id"], None)).await.is_err()
                    {
                        break Some("error.driverStopped");
                    }
                    let fatal = events.iter().any(|event| matches!(event, EngineEvent::Error { fatal: true, .. }));
                    // ACP can reuse its tool IDs across prompts. The engine's asynchronous tool replies cannot.
                    let tool_id = |id: &str| builtin_name.as_ref().map_or_else(|| id.to_owned(), |name| format!("{name}:{next_id}:{id}"));
                    for mut event in events {
                        if msg["error"].is_object() && let EngineEvent::Done { reason, .. } = &mut event {
                            *reason = "reason.failed".into();
                        }
                        if let EngineEvent::Permission { request_id, tool, mut input } = event {
                            if !ready || watch.stopping() {
                                for answer in reader.cancel_pending() {
                                    if proc::write_line(&mut *child.stdin, &answer).await.is_err() {
                                        break 'dialog Some(if watch.stopping() { "reason.interrupted" } else { "error.driverStopped" });
                                    }
                                }
                                continue;
                            }
                            if let Some(name) = builtin_prefix.as_deref().and_then(|prefix| tool.strip_prefix(prefix)) {
                                // The bridge authorizes each tool with the same policy as the in-process MCP server.
                                let allow = bridge_ready && watch.active()
                                    && crate::helper::called(&json!(name), tools).is_some_and(|tool| {
                                        matches!(reader.asked(&request_id), Some(Asked::Permission { call, .. })
                                            if calls.approve(tool, tool_id(call), input))
                                    });
                                if let Some(answer) = reader.answer_permission(&request_id, allow)
                                    && proc::write_line(&mut *child.stdin, &answer).await.is_err()
                                {
                                    break 'dialog Some("error.driverStopped");
                                }
                                continue;
                            }
                            // The reader gives every tool of agy's own a `file_path` that is its target or empty; a tool
                            // server's arguments are its own business.
                            let path = input.get("file_path").and_then(Value::as_str);
                            if !tool.starts_with("mcp__") && input.get("file_path").is_some() {
                                if let Some(real) = path.and_then(|path| real_path(&opts.cwd, path)) {
                                    input["file_path"] = real.into();
                                } else if let Some(answer) = reader.answer_permission(&request_id, false) {
                                    if proc::write_line(&mut *child.stdin, &answer).await.is_err() {
                                        break 'dialog Some("error.driverStopped");
                                    }
                                    continue;
                                }
                            }
                            on_event(EngineEvent::Permission { request_id, tool, input });
                        } else {
                            match &mut event {
                                EngineEvent::Tool { id, name, .. } => {
                                    if builtin_prefix.as_deref().is_some_and(|prefix| name.starts_with(prefix)) {
                                        // The MCP invocation records this step with its actual arguments.
                                        continue;
                                    }
                                    *id = tool_id(id);
                                }
                                EngineEvent::ToolResult { id, .. } => {
                                    *id = tool_id(id);
                                    calls.observed_result(id);
                                }
                                _ => {}
                            }
                            on_event(event);
                        }
                    }
                    if configured && let Some(model) = opts.model.as_ref().filter(|model| !model.is_empty()) {
                        on_event(EngineEvent::Started { session_id: reader.session().into(), model: model.clone() });
                    }
                    if fatal || retiring && matched {
                        break None;
                    }
                }
                _ = async { link.as_mut().expect("bridge is present").await }, if link.is_some() => {
                    link = None;
                }
                Some(request) = calls_in.recv(), if link.is_some() => {
                    bridge_ready |= request["method"] == "tools/list";
                    if let Some(reply) = calls.receive(request, tools, ready && watch.active() && !watch.stopping(), &on_event)
                        && calls_out.try_send(reply).is_err()
                    {
                        link = None;
                    }
                }
                command = rx.recv() => match command {
                    Some(SessionCommand::HelperReply { id, ok, text, pictures, ack }) => {
                        if let Some(reply) = calls.finish(&id, ok, &text, &pictures, ack, &on_event)
                            && calls_out.try_send(reply).is_err()
                        {
                            link = None;
                        }
                    }
                    Some(SessionCommand::Send(turn)) if !retiring => queued.push_back(turn),
                    Some(SessionCommand::Steer { tag, .. }) => on_event(EngineEvent::SteerFailed { tag }),
                    Some(SessionCommand::McpStatus { reply }) => { let _ = reply.send(None); }
                    Some(SessionCommand::StopBackground { ack, .. }) => { let _ = ack.send(Delivery::Unknown); }
                    Some(SessionCommand::Compact) => on_event(EngineEvent::CompactFinished(crate::events::CompactOutcome::Failed)),
                    Some(SessionCommand::Answer { request_id, allow, reply, ack }) => {
                        let answer = match reader.asked(&request_id) {
                            Some(Asked::Question { .. }) => reader.answer_question(&request_id, reply.as_ref()),
                            _ => reader.answer_permission(&request_id, allow),
                        };
                        let delivered = match answer {
                            Some(answer) if proc::write_line(&mut *child.stdin, &answer).await.is_ok() => Delivery::Delivered,
                            Some(_) => Delivery::WriteFailed,
                            None => Delivery::Unknown,
                        };
                        let _ = ack.send(delivered);
                        if delivered == Delivery::WriteFailed {
                            break Some("error.driverStopped");
                        }
                    }
                    Some(SessionCommand::Interrupt) if watch.active() => {
                        if !watch.stopping() {
                            queued.clear();
                            watch.interrupt();
                            for reply in calls.cancel(&on_event) {
                                let _ = calls_out.try_send(reply);
                            }
                            let deadline = watch.deadline().expect("interrupt starts the deadline");
                            let cancelled = tokio::time::timeout_at(deadline, async {
                                proc::write_line(&mut *child.stdin, &cancel(reader.session())).await?;
                                for answer in reader.cancel_pending() {
                                    proc::write_line(&mut *child.stdin, &answer).await?;
                                }
                                Ok::<_, proc::WriteError>(())
                            }).await;
                            if !matches!(cancelled, Ok(Ok(()))) {
                                break Some("reason.interrupted");
                            }
                        }
                    }
                    Some(SessionCommand::Interrupt) if !ready || !queued.is_empty() => break Some("reason.interrupted"),
                    Some(SessionCommand::Retire) if watch.active() => retiring = true,
                    Some(SessionCommand::Retire | SessionCommand::Close) | None => break None,
                    _ => {}
                }
            }
        };
        drop(calls.cancel(&on_event));
        drop(link);
        let fake = child.child.is_none() && child.contained.is_none();
        let mut confirmed = proc::stop_confirmed(&mut *child.stdin, &mut child.child, &mut child.contained).await || fake;
        on_event(EngineEvent::WriterExit { confirmed });
        if let Some(message) = terminal {
            on_event(EngineEvent::Error { message: message.into(), fatal: true });
        }
        while !confirmed && child.contained.is_some() {
            match rx.recv().await {
                Some(SessionCommand::Interrupt | SessionCommand::Close) => {
                    confirmed = proc::stop_confirmed(&mut *child.stdin, &mut child.child, &mut child.contained).await;
                    on_event(EngineEvent::WriterExit { confirmed });
                }
                None => break,
                _ => {}
            }
        }
        // shortcut: a temp folder whose process never confirmed its end, or outlived Faber, stays; sweep them once seen.
        if confirmed {
            let _ = std::fs::remove_dir_all(&tmp);
        }
    });
    Ok(Session::new(tx).with_tree(tree))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_paths() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "").unwrap();
        let resolved = real_path(dir.path(), "file").unwrap();
        assert_eq!(Path::new(&resolved).canonicalize().unwrap(), file.canonicalize().unwrap());
        assert!(!resolved.starts_with(r"\\?\"));
        assert_eq!(
            real_path(dir.path(), "missing"),
            Some(dir.path().canonicalize().unwrap().join("missing").to_string_lossy().replace(r"\\?\", ""))
        );
        assert_eq!(real_path(dir.path(), "file/missing"), None);
        assert_eq!(real_path(dir.path(), ""), None);
        assert_eq!(real_path(dir.path(), file.to_str().unwrap()), Some(resolved));
    }

    #[test]
    fn child_environment_is_private_and_scrubs_credentials_by_name() {
        let tmp = Path::new("private-temp");
        let changes = environment(tmp).into_iter().collect::<HashMap<_, _>>();
        for name in
            ["GEMINI_API_KEY", "GOOGLE_API_KEY", "GOOGLE_APPLICATION_CREDENTIALS", "GOOGLE_GENAI_USE_VERTEXAI", "GOOGLE_CLOUD_PROJECT"]
        {
            assert_eq!(changes[name], None);
        }
        for name in ["TEMP", "TMP"].into_iter().chain((!cfg!(windows)).then_some("TMPDIR")) {
            assert_eq!(changes[name].as_deref(), Some(tmp.as_os_str()));
        }
    }

    #[tokio::test]
    async fn project_tools_are_added_to_new_and_resumed_ask_sessions() {
        tokio::time::timeout(Duration::from_secs(15), check_project_tools()).await.unwrap();
    }

    async fn check_project_tools() {
        for (resume, idle_link_loss) in [(None, false), (Some("existing".to_string()), false), (None, true)] {
            let data = tempfile::tempdir().unwrap();
            let (driver, fake) = tokio::io::duplex(16384);
            let (stdout, stdin) = tokio::io::split(driver);
            let process = std::sync::Mutex::new(Some(proc::Proc {
                stdin: Box::new(stdin),
                stdout: Box::new(stdout),
                stderr: None,
                child: None,
                contained: None,
            }));
            let spawn = std::sync::Arc::new(move |_: &str, _: &[String], _: &Path| Ok(process.lock().unwrap().take().unwrap()));
            let opts = AgyOptions {
                bin: "fake-agy".into(),
                cwd: data.path().into(),
                data: data.path().into(),
                model: None,
                effort: "low".into(),
                resume: resume.clone(),
                permissions: Permissions::Ask,
                builtin: Some(("fake-faber".into(), crate::helper::Tools { progress: true, ..Default::default() })),
                leads: true,
                spawn,
            };
            let (events, mut emitted) = tokio::sync::mpsc::unbounded_channel();
            let session = start(
                opts,
                &[],
                std::sync::Arc::new(move |event| {
                    let _ = events.send(event);
                }),
            )
            .unwrap();
            let (read, mut write) = tokio::io::split(fake);
            let mut lines = BufReader::new(read).lines();
            assert!(lines.next_line().await.unwrap().unwrap().contains("initialize"));
            proc::write_line(&mut write, &json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}})).await.unwrap();
            let open: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(open["method"], if resume.is_some() { "session/resume" } else { "session/new" });
            let launch = &open["params"]["mcpServers"][0];
            let name = launch["name"].as_str().unwrap();
            assert!(name.starts_with("faber"));
            let port = launch["args"][1].as_str().unwrap().parse::<u16>().unwrap();
            let secret = launch["env"][0]["value"].as_str().unwrap();
            let address = (std::net::Ipv4Addr::LOCALHOST, port);
            let mut intruder = tokio::net::TcpStream::connect(address).await.unwrap();
            tokio::io::AsyncWriteExt::write_all(&mut intruder, &[b'x'; 64]).await.unwrap();
            assert_eq!(tokio::io::AsyncReadExt::read(&mut intruder, &mut [0]).await.unwrap(), 0);

            let stream = tokio::net::TcpStream::connect(address).await.unwrap();
            let (mcp_read, mut mcp_write) = stream.into_split();
            let mut mcp_output = BufReader::new(mcp_read).lines();
            tokio::io::AsyncWriteExt::write_all(&mut mcp_write, secret.as_bytes()).await.unwrap();
            proc::write_line(&mut mcp_write, &json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})).await.unwrap();
            let list: Value = serde_json::from_str(&mcp_output.next_line().await.unwrap().unwrap()).unwrap();
            let listed = list["result"]["tools"].as_array().unwrap();
            assert!(listed.iter().any(|tool| tool["name"] == "status"));
            assert!(!listed.iter().any(|tool| tool["name"] == "verify" || tool["name"] == "browse"));
            assert!(tokio::net::TcpStream::connect(address).await.is_err());

            let call =
                json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"status","arguments":{"text":"Checking the bridge"}}});
            proc::write_line(&mut mcp_write, &call).await.unwrap();
            let denied: Value = serde_json::from_str(&mcp_output.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(denied["result"]["isError"], true);
            assert!(emitted.try_recv().is_err());

            proc::write_line(&mut write, &json!({"id":2,"result":{"sessionId":"existing"}})).await.unwrap();
            let mode: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(mode["method"], "session/set_mode");
            proc::write_line(&mut write, &json!({"id":mode["id"],"result":{}})).await.unwrap();
            session.send(crate::events::UserTurn { text: "Check".into(), images: vec![] });
            let prompt: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(prompt["method"], "session/prompt");
            assert!(prompt["params"]["prompt"][0]["text"].as_str().unwrap().contains(&format!("mcp_{name}_")));
            for (tool, option) in [("status", "allow"), ("verify", "reject")] {
                proc::write_line(
                    &mut write,
                    &json!({
                        "jsonrpc":"2.0", "id":"permit", "method":"session/request_permission",
                        "params":{
                            "sessionId":"existing",
                            "toolCall":{"toolCallId":tool,"name":format!("mcp_{name}_{tool}"),"kind":"other","rawInput":{}},
                            "options":[{"optionId":"allow","kind":"allow_once"},{"optionId":"reject","kind":"reject_once"}]
                        }
                    }),
                )
                .await
                .unwrap();
                let permission: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
                assert_eq!(permission["result"]["outcome"]["optionId"], option);
            }
            proc::write_line(&mut mcp_write, &call).await.unwrap();
            let mut step = None;
            loop {
                match emitted.recv().await.unwrap() {
                    EngineEvent::Tool { id, name, .. } if name == "Status" => step = Some(id),
                    EngineEvent::ProgressCall { id, tool, input } => {
                        assert_eq!(tool, crate::progress::STATUS_TOOL);
                        assert_eq!(input["text"], "Checking the bridge");
                        assert_eq!(step.as_deref(), Some(id.as_str()));
                        session.helper_reply(&id, true, "Status set.".into());
                        break;
                    }
                    _ => {}
                }
            }
            let answered: Value = serde_json::from_str(&mcp_output.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(answered["result"]["content"][0]["text"], "Status set.");
            if idle_link_loss {
                proc::write_line(&mut write, &json!({"id":prompt["id"],"result":{"stopReason":"end_turn"}})).await.unwrap();
                while !matches!(emitted.recv().await.unwrap(), EngineEvent::Done { ok: true, .. }) {}
                tokio::io::AsyncWriteExt::shutdown(&mut mcp_write).await.unwrap();
                let mut retired = false;
                while let Some(event) = emitted.recv().await {
                    match event {
                        EngineEvent::Reason(BRIDGE_CLOSED) => retired = true,
                        EngineEvent::Error { message, fatal: true } => {
                            assert!(retired);
                            assert_eq!(message, "error.driverStopped");
                            break;
                        }
                        _ => {}
                    }
                }
            } else if resume.is_none() {
                session.close();
                assert!(mcp_output.next_line().await.unwrap().is_none());
            } else {
                tokio::io::AsyncWriteExt::shutdown(&mut mcp_write).await.unwrap();
                assert!(mcp_output.next_line().await.unwrap().is_none());
                proc::write_line(&mut write, &json!({"id":prompt["id"],"result":{"stopReason":"end_turn"}})).await.unwrap();
                let mut finished = false;
                let mut retired = false;
                while let Some(event) = emitted.recv().await {
                    match event {
                        EngineEvent::Reason(BRIDGE_CLOSED) => retired = true,
                        EngineEvent::Done { ok, reason, .. } => {
                            assert!(ok);
                            assert!(retired);
                            assert_ne!(reason, BRIDGE_CLOSED);
                            finished = true;
                        }
                        EngineEvent::WriterExit { confirmed: true } => break,
                        _ => {}
                    }
                }
                assert!(finished);
            }
        }
    }
}
