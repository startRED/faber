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

pub struct AgyOptions {
    pub bin: String,
    pub cwd: PathBuf,
    pub data: PathBuf,
    pub model: Option<String>,
    pub effort: String,
    pub resume: Option<String>,
    pub permissions: Permissions,
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

pub fn start(opts: AgyOptions, servers: &[Launch], on_event: crate::claude::EventSink) -> Result<Session, String> {
    if !matches!(opts.permissions, Permissions::Ask | Permissions::FullAccess) {
        return Err("error.notConnected".into());
    }
    // A tool server runs outside any guard, so only Full access gets one (R195), as with the other drivers.
    let servers = if opts.permissions == Permissions::FullAccess { servers } else { &[] };
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
        Some(id) => session_resume(2, id, cwd, servers),
        None => session_new(2, cwd, servers),
    };

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
        let mut lines = BufReader::new(child.stdout).lines();
        let mut reader = AcpReader::new();
        let mut queued = VecDeque::new();
        let mut startup = VecDeque::from([initialize(1, env!("CARGO_PKG_VERSION")), open]);
        let mut waiting = None;
        let mut next_id = 2;
        let mut ready = false;
        let mut retiring = false;
        let mut watch = TurnWatch::default();
        let mut startup_deadline = Some(Instant::now() + Duration::from_secs(30));
        let terminal = 'dialog: loop {
            let message = if waiting.is_none() {
                if let Some(setup) = startup.pop_front() {
                    Some(setup)
                } else if ready && !retiring {
                    queued.pop_front().map(|turn: crate::events::UserTurn| {
                        next_id += 1;
                        watch.begin();
                        let mut msg = prompt(next_id, reader.session(), &turn.text);
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
                        eprintln!("faber: Antigravity exited (code {}): {}", code.as_deref().unwrap_or("unknown"), *stderr);
                        break Some(if watch.stopping() { "reason.interrupted" } else { "error.driverStopped" });
                    };
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                        reader.read(&line);
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
                        }
                    }
                    // A question in a form the reader cannot show is declined: agy would wait for its answer forever.
                    if msg["method"] == "elicitation/create" && events.is_empty()
                        && proc::write_line(&mut *child.stdin, &elicitation_answer(&msg["id"], None)).await.is_err()
                    {
                        break Some("error.driverStopped");
                    }
                    let fatal = events.iter().any(|event| matches!(event, EngineEvent::Error { fatal: true, .. }));
                    for mut event in events {
                        if msg["error"].is_object() && let EngineEvent::Done { reason, .. } = &mut event { *reason = "reason.failed".into(); }
                        if let EngineEvent::Permission { request_id, tool, mut input } = event {
                            if !ready || watch.stopping() {
                                for answer in reader.cancel_pending() {
                                    if proc::write_line(&mut *child.stdin, &answer).await.is_err() {
                                        break 'dialog Some(if watch.stopping() { "reason.interrupted" } else { "error.driverStopped" });
                                    }
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
                command = rx.recv() => match command {
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
        assert_eq!(Path::new(&resolved), file.canonicalize().unwrap().as_path().strip_prefix(r"\\?\").unwrap_or(&file));
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
}
