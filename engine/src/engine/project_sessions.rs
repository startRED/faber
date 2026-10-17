//! Dedicated role sessions share the runner command lock, never Chat's live sessions.
use super::*;
use crate::project::{Role as ProjectRole, run::Runner, store::Journal};

pub struct LocalBuildGuard(Engine);
impl Drop for LocalBuildGuard {
    fn drop(&mut self) {
        self.0.with(|c| c.local_build = false);
    }
}

fn role_events() -> (EventSink, tokio::sync::mpsc::Receiver<EngineEvent>, Arc<std::sync::atomic::AtomicBool>) {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let overflow = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let full = overflow.clone();
    let sink: EventSink = Arc::new(move |event| {
        if matches!(&event, EngineEvent::Text(delta) if delta.len() > crate::project::wire::MAX_ENVELOPE) {
            full.store(true, std::sync::atomic::Ordering::SeqCst);
            let _ = tx.try_send(EngineEvent::Error { message: "error.wire.tooBig".into(), fatal: false });
        } else if tx.try_send(event).is_err() {
            full.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    });
    (sink, rx, overflow)
}

/// The notice a CLI process gives when it exits cleanly after its turn.
fn clean_exit_notice(message: &str) -> bool {
    message == "Codex stopped." || message.starts_with("Claude Code stopped (exit 0)") || message.starts_with("Codex stopped (exit 0)")
}

fn role_invocation(request: &mut crate::project::budget::Invocation, role: ProjectRole) -> Result<(), String> {
    // Dedicated role constructors always launch without resume; caller bases must not discount that fresh cost.
    request.fresh = true;
    request.session = Uuid::new_v4().to_string();
    request.kind = match role {
        ProjectRole::Lead => crate::project::budget::Kind::Lead,
        ProjectRole::Reviewer => crate::project::budget::Kind::Review,
        _ => crate::project::budget::Kind::Worker,
    };
    request.exposure_micros = Some(crate::project::budget::role_request_maximum(&request.model, role, None, 0).map_err(str::to_string)?);
    Ok(())
}

impl Engine {
    /// Reserve quiet Chat admission for the entire build/canary/promotion attempt.
    pub fn local_build_admission(&self) -> Result<LocalBuildGuard, String> {
        self.with(|c| {
            // A mission worker alive in memory is work too: the install closes it.
            let workers = c.mission_sessions.values().any(|sessions| !sessions.is_empty());
            if c.local_build || workers || !c.running_items().is_empty() || c.convs.values().any(|v| !v.queued.is_empty()) {
                return Err("error.update.activeOperations".into());
            }
            c.local_build = true;
            Ok(LocalBuildGuard(self.clone()))
        })
    }
    pub fn set_local_update(&self, layout: crate::project::update::SlotLayout) {
        self.with(|c| c.local_update = Some(layout));
    }

    pub(crate) fn mission_update_admission(&self) -> Result<Option<crate::project::update::Coordinator>, String> {
        let Some(layout) = self.with(|c| c.local_update.clone()) else { return Ok(None) };
        let guard = layout.coordinate().map_err(|e| e.key().to_string())?;
        let manifest = crate::project::update::UpdateManager::new(layout)
            .map_err(|e| e.key().to_string())?
            .manifest()
            .map_err(|e| e.key().to_string())?
            .ok_or("error.store.corrupt")?;
        if matches!(
            manifest.state,
            crate::project::update::UpdateState::Quiescent
                | crate::project::update::UpdateState::Swapping { .. }
                | crate::project::update::UpdateState::Probation { .. }
        ) {
            return Err("error.update.activeOperations".into());
        }
        Ok(Some(guard))
    }

    pub async fn pause_missions_for_update(&self) -> Result<(), String> {
        let coordinator = self.mission_lock();
        let _turn = coordinator.lock().await;
        let projects = self.projects_checked()?;
        let data = self.with(|c| c.opts.data_dir.clone());
        let root = crate::project::lease::real_path(&data.join("missions")).map_err(|e| e.key().to_string())?;
        if !root.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(root).map_err(|_| "error.store.read")? {
            let entry = entry.map_err(|_| "error.store.read")?;
            let path = crate::project::lease::real_path(&entry.path().join("run.json")).map_err(|e| e.key().to_string())?;
            if !path.exists() {
                continue;
            }
            let bytes = crate::project::files::ScopedFile::new(&path, false)
                .map_err(|e| e.key().to_string())?
                .read_limited(16 * 1024 * 1024)
                .map_err(|e| e.key().to_string())?
                .ok_or("error.store.corrupt")?
                .0;
            let recorded: Runner = serde_json::from_slice(&bytes).map_err(|_| "error.store.corrupt")?;
            // A mission of a project that left Faber cannot be running (removal waits for it to stop): nothing to pause.
            let Some(project) = projects.iter().find(|p| p.id == recorded.mission.project_id) else { continue };
            let journal =
                Journal::open(std::path::Path::new(&project.path), &data, &recorded.mission.id).map_err(|e| e.key().to_string())?;
            let mut runner = Runner::load(&journal).map_err(str::to_string)?;
            if runner.mission.state == crate::project::MissionState::Done {
                let paused = self.pause_mission_budget(&runner.mission.id);
                let sessions = self.mission_sessions(&runner.mission.id);
                for session in &sessions {
                    session.interrupt();
                }
                if !sessions.is_empty() {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
                for session in &sessions {
                    session.close();
                }
                self.with(|c| {
                    c.mission_sessions.remove(&runner.mission.id);
                    c.mission_projects.remove(&runner.mission.id);
                });
                paused?;
            } else if runner.mission.state != crate::project::MissionState::Paused || !self.mission_sessions(&runner.mission.id).is_empty()
            {
                runner.stop(&journal, self).await?;
                self.with(|c| {
                    c.mission_sessions.remove(&runner.mission.id);
                    c.mission_projects.remove(&runner.mission.id);
                });
            }
        }
        Ok(())
    }
    pub fn mission_lock(&self) -> Arc<tokio::sync::Mutex<()>> {
        self.with(|c| c.mission_lock.clone())
    }

    pub async fn dispatch_mission_worker(&self, journal: Journal, request: crate::project::budget::Invocation) -> Result<(), String> {
        self.dispatch_role(journal, request, None).await
    }

    /// A Writer launch. Its lease must already hold the durable launch intent (`Leases::prepare_launch`); the
    /// process identity attached afterwards comes from the owned tree the engine itself started, never from a caller.
    /// A session with no owned tree (a replayed recording, or a platform without the job) is closed and refused.
    pub async fn dispatch_mission_writer(
        &self,
        journal: Journal,
        request: crate::project::budget::Invocation,
        leases: &mut crate::project::lease::Leases,
        lease: &crate::project::Lease,
    ) -> Result<(), String> {
        self.dispatch_role(journal, request, Some((leases, lease))).await
    }

    async fn dispatch_role(
        &self,
        journal: Journal,
        mut request: crate::project::budget::Invocation,
        launcher: Option<(&mut crate::project::lease::Leases, &crate::project::Lease)>,
    ) -> Result<(), String> {
        let coordinator = self.mission_lock();
        let _turn = coordinator.lock().await;
        let _update = self.mission_update_admission()?;
        // Once an install or a build holds the admission, no worker begins: it would be closed with the process.
        if self.with(|c| c.local_build) {
            return Err("error.update.activeOperations".into());
        }
        let mut runner = Runner::load(&journal).map_err(str::to_string)?;
        runner.admit(request.vendor).map_err(str::to_string)?;
        // No trusted pre-request context census yet: use the published window, never the caller's estimate.
        let worker = runner.workers.get(&request.worker).ok_or("error.wire.stale")?.clone();
        role_invocation(&mut request, worker.brief.role)?;
        // Source can be unlocked only through the launcher that attaches T4's owned process identity, and only to a Writer.
        let writer = worker.brief.role == ProjectRole::Writer;
        if writer != launcher.is_some()
            || launcher.as_ref().is_some_and(|(_, lease)| lease.task != request.worker || lease.mission != request.mission)
        {
            return Err("reason.project.processTreeUnproven".into());
        }
        let prompt = runner.prompt(&journal, &request.worker, &[]).map_err(str::to_string)?;
        let mut resources = crate::project::disk::Resources::load(&journal).map_err(str::to_string)?;
        let cwd = resources
            .register_for(&journal, &format!("session-{}", Uuid::new_v4()), &request.worker, crate::project::disk::ResourceKind::Temp, None)
            .map_err(str::to_string)?;
        std::fs::create_dir_all(&cwd).map_err(|e| crate::store::Fault::of_write(&e).key().to_string())?;
        let (cli, spawn, windows) = self.with(|c| {
            let cli = c.opts.clis.get(&request.vendor).cloned().ok_or("error.cli.missing")?;
            Ok::<_, String>((cli.clone(), spawner(&c.opts.spawn, &cli), c.opts.windows))
        })?;
        runner.prepare_dispatch(&journal, self, request.clone())?;
        runner.save(&journal).map_err(str::to_string)?;
        let (weak, runtime) = (Arc::downgrade(&self.core), self.runtime.clone());
        let events_journal = journal.clone();
        let invocation = request.id.clone();
        let task = request.worker.clone();
        let mut text = String::new();
        let (sink, mut rx, overflow) = role_events();
        let executor = runtime.clone();
        executor.spawn(async move {
            while let Some(event) = rx.recv().await {
                let Some(core) = weak.upgrade() else {
                    return;
                };
                let engine = Engine { core, runtime: runtime.clone() };
                let coordinator = engine.mission_lock();
                let _turn = coordinator.lock().await;
                let result = if overflow.load(std::sync::atomic::Ordering::SeqCst) {
                    Err("error.wire.tooBig".into())
                } else {
                    engine.mission_event(&events_journal, &invocation, &task, &mut text, event)
                };
                if let Err(key) = result {
                    let _ = engine.pause_mission_budget(events_journal.mission_id());
                    for session in engine.mission_sessions(events_journal.mission_id()) {
                        session.interrupt();
                        session.close();
                    }
                    if let Ok(mut runner) = Runner::load(&events_journal) {
                        runner.mission.reason = Some(key);
                        let _ = runner.pause(&events_journal);
                    }
                    break;
                }
            }
        });
        let session = match request.vendor {
            Vendor::Claude => crate::claude::start_project_claude(
                ClaudeOptions {
                    bin: cli.command,
                    cwd,
                    launch: ClaudeLaunch {
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        verify: false,
                        area: Area::Project,
                        model: Some(worker.pin.model.clone()),
                        effort: Some(worker.pin.effort.clone()),
                        resume: None,
                        user_hooks: false,
                        instructions: None,
                        user_name: None,
                        // A role never inherits the person's mode; the role's own arguments are stricter still.
                        permissions: Permissions::Isolated,
                        flags: Default::default(),
                        tuning: Default::default(),
                    },
                    spawn,
                    sandbox: None,
                },
                worker.brief.role,
                sink,
            ),
            Vendor::Codex => CodexServer::start_project(
                CodexServerOptions {
                    permissions: Permissions::Isolated,
                    cli,
                    cwd: cwd.clone(),
                    area: Area::Project,
                    user_hooks: false,
                    windows,
                    spawn,
                },
                CodexThread {
                    helper: Default::default(),
                    browse: false,
                    conversations: false,
                    verify: false,
                    cwd,
                    model: Some(worker.pin.model.clone()),
                    effort: Some(worker.pin.effort.clone()),
                    resume: None,
                    instructions: None,
                    user_name: None,
                    permissions: Permissions::Isolated,
                    tuning: Default::default(),
                },
                worker.brief.role,
                sink,
            ),
        }
        .map_err(|e| e.key().to_string())?;
        if session.project_role() != Some(worker.brief.role) {
            session.close();
            return Err("error.project.invocation".into());
        }
        // Every role must run inside a tree the engine owns: its end is only ever confirmed by retiring that tree.
        let attached = match (session.tree(), launcher) {
            (Some(tree), Some((leases, lease))) => {
                leases.attach_process(lease, tree.identity()).map(|()| tree).map_err(|e| e.key().to_string())
            }
            (Some(tree), None) => Ok(tree),
            (None, _) => Err("reason.project.processTreeUnproven".to_string()),
        };
        match attached {
            Ok(tree) => self.with(|c| c.worker_trees.insert(request.id.clone(), tree)),
            Err(key) => {
                session.close();
                if let Some(tree) = session.tree() {
                    let _ = tree.retire().await;
                }
                return Err(key);
            }
        };
        self.with(|c| {
            c.mission_sessions.entry(runner.mission.id.clone()).or_default().push(session.clone());
            c.mission_projects.insert(runner.mission.id.clone(), runner.mission.project_id.clone());
        });
        session.send(UserTurn::text(prompt));
        Ok(())
    }

    /// The end of any role: its sessions are closed, its owned process tree is closed and reaped, and only then does
    /// the invocation's spending settle against what it reported (or its computed bound if it reported nothing). An
    /// unknown invocation, a recording or a platform without the job has no tree, so it has no proof and settles nothing.
    async fn end_role(&self, invocation: &str) -> Result<crate::project::lease::Termination, String> {
        let (tree, terminal) = self.with(|c| (c.worker_trees.get(invocation).cloned(), c.worker_terminals.get(invocation).cloned()));
        let tree = tree.ok_or("reason.project.processTreeUnproven")?;
        let mission = self.mission_reservation(invocation)?.request.mission;
        // From here on whatever the processes report is their shutdown, not a new failure of a finished turn.
        self.with(|c| c.worker_closing.insert(invocation.into()));
        for session in self.mission_sessions(&mission) {
            session.close();
        }
        self.with(|c| c.mission_sessions.remove(&mission));
        let retired = tree.retire().await.map_err(|_| "reason.project.processTreeUnproven".to_string())?;
        let event = terminal.unwrap_or(EngineEvent::Error { message: "reason.interrupted".into(), fatal: true });
        self.mission_budget(crate::project::budget::Change::Report { id: invocation.into(), event, terminated: true })?;
        let settled = self.mission_reservation(invocation)?.settled;
        Ok(crate::project::lease::Termination::from_retired(&retired, settled))
    }

    /// The release proof of a Writer launched through `dispatch_mission_writer`.
    pub async fn worker_termination(&self, invocation: &str) -> Result<crate::project::lease::Termination, String> {
        self.end_role(invocation).await
    }

    /// The end of a read-only role. It settles only after its process tree is confirmed gone, exactly like a Writer.
    pub async fn finish_role(&self, invocation: &str) -> Result<(), String> {
        self.end_role(invocation).await.map(|_| ())
    }

    /// Whether the role's process tree is still held by this process. After a restart no tree is held, so a turn that
    /// was running before it is not live.
    pub fn role_live(&self, invocation: &str) -> bool {
        self.with(|c| c.worker_trees.contains_key(invocation))
    }

    /// Settle a turn whose process is gone and whose reply never came, at its computed bound, and close the dispatch
    /// record that waited for it. An invocation that was never admitted has nothing to settle.
    pub async fn abandon_role(&self, journal: &Journal, invocation: &str) -> Result<(), String> {
        let reservation = match self.mission_reservation(invocation) {
            Ok(reservation) => reservation,
            Err(key) if key == "reason.budget.identity" => return Ok(()),
            Err(key) => return Err(key),
        };
        if !reservation.settled {
            let event = EngineEvent::Error { message: "reason.interrupted".into(), fatal: true };
            self.mission_budget(crate::project::budget::Change::Report { id: invocation.into(), event, terminated: true })?;
        }
        Runner::load(journal).map_err(str::to_string)?.settle_dispatches(journal, self)
    }

    /// Wakes when a role turn ends or a mission stops. A coordinator reads the journal when this fires, never on a timer.
    pub fn mission_wake(&self) -> Arc<tokio::sync::Notify> {
        self.with(|c| c.mission_wake.clone())
    }

    pub(super) fn mission_event(
        &self,
        journal: &Journal,
        invocation: &str,
        task: &str,
        text: &mut String,
        event: EngineEvent,
    ) -> Result<(), String> {
        let ends = matches!(event, EngineEvent::Done { .. } | EngineEvent::Error { .. });
        let result = self.mission_event_inner(journal, invocation, task, text, event);
        if ends || result.is_err() {
            self.mission_wake().notify_one();
        }
        result
    }

    fn mission_event_inner(
        &self,
        journal: &Journal,
        invocation: &str,
        task: &str,
        text: &mut String,
        event: EngineEvent,
    ) -> Result<(), String> {
        let mut runner = Runner::load(journal).map_err(str::to_string)?;
        match event {
            EngineEvent::Started { model, .. } => {
                let worker = runner.workers.get_mut(task).ok_or("error.wire.stale")?;
                let mut resolved = worker.pin.clone();
                resolved.model = model;
                worker.begin_turn(&resolved).map_err(str::to_string)?;
                runner.save(journal).map_err(str::to_string)?;
                // A Writer's launch receipt was written when its owned process identity was attached.
                if journal.effect_receipt(invocation).map_err(|e| e.key().to_string())?.is_none() {
                    let mut receipt = crate::project::Receipt::new(invocation, crate::project::Outcome::Done, self.mission_time());
                    receipt.evidence.insert("invocation".into(), invocation.into());
                    journal.record_receipt(&receipt).map_err(|e| e.key().to_string())?;
                }
            }
            EngineEvent::Text(delta) => {
                if text.len().saturating_add(delta.len()) > crate::project::wire::MAX_ENVELOPE {
                    return Err("error.wire.tooBig".into());
                }
                text.push_str(&delta);
            }
            event @ (EngineEvent::Done { .. } | EngineEvent::Error { .. }) => {
                let completed = self.with(|c| {
                    let done = matches!(c.worker_terminals.get(invocation), Some(EngineEvent::Done { .. }));
                    if !done {
                        c.worker_terminals.insert(invocation.into(), event.clone());
                    }
                    done
                });
                if completed {
                    // Only the shutdown of a finished turn is expected: the process exiting cleanly, or anything at all
                    // once the engine is closing it. A security failure or any other fatal error still stops the mission.
                    let closing = self.with(|c| c.worker_closing.contains(invocation));
                    return match &event {
                        EngineEvent::Error { message, fatal: true } => {
                            let restrictions = crate::project::policy::Blocker::Restrictions.key();
                            if message == restrictions {
                                Err(message.clone())
                            } else if closing || clean_exit_notice(message) {
                                Ok(())
                            } else {
                                Err("error.project.invocation".into())
                            }
                        }
                        _ => Ok(()),
                    };
                }
                let stopped = self.mission_budget(crate::project::budget::Change::Report {
                    id: invocation.into(),
                    event: event.clone(),
                    terminated: false,
                })?;
                if journal.effect_receipt(invocation).map_err(|e| e.key().to_string())?.is_none() {
                    let receipt = crate::project::Receipt::new(invocation, crate::project::Outcome::Failed, self.mission_time());
                    journal.record_receipt(&receipt).map_err(|e| e.key().to_string())?;
                }
                if let crate::project::budget::Status::Stop(key) = stopped {
                    return Err(key.into());
                }
                if let EngineEvent::Error { message, fatal: true } = &event {
                    let restrictions = crate::project::policy::Blocker::Restrictions.key();
                    return Err(if message == restrictions { message.clone() } else { "error.project.invocation".into() });
                }
                if matches!(event, EngineEvent::Done { .. }) && runner.mission.state != crate::project::MissionState::Paused {
                    let output = std::mem::take(text);
                    match runner.receive(journal, task, &output).map_err(str::to_string)? {
                        crate::project::worker::Reply::Accepted(envelope) | crate::project::worker::Reply::Proposal(envelope) => {
                            journal.record_reply(invocation, &envelope).map_err(|e| e.key().to_string())?;
                        }
                        crate::project::worker::Reply::Bounce(key) | crate::project::worker::Reply::Blocked(key) => {
                            journal.record_refusal(invocation, key).map_err(|e| e.key().to_string())?;
                        }
                        crate::project::worker::Reply::Continue => {
                            journal.record_refusal(invocation, "reason.stall").map_err(|e| e.key().to_string())?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fresh_role_invocations_cannot_use_a_callers_resume_base_or_purpose() {
        let mut request: crate::project::budget::Invocation = serde_json::from_value(serde_json::json!({
            "id":"dispatch_new", "mission":"mission", "worker":"reviewer", "kind":"worker", "vendor":"codex",
            "model":"gpt-6-luna", "effort":"medium", "session":"caller_old", "fresh":false, "exposureMicros":1
        }))
        .unwrap();
        let old = crate::usage::CostBase { session: request.session.clone(), total: 0.05 };
        role_invocation(&mut request, ProjectRole::Reviewer).unwrap();
        assert_eq!(crate::usage::settle(Some(&old), &request.session, request.fresh, Some(0.06)).cost, Some(0.06));
        assert!(request.fresh);
        assert_ne!(request.session, old.session);
        assert!(Uuid::parse_str(&request.session).is_ok());
        assert_eq!(request.kind, crate::project::budget::Kind::Review);
        assert_eq!(request.exposure_micros, Some(306_000));
    }
    #[tokio::test]
    async fn local_build_reserves_chat_admission_and_releases_on_unwind() {
        let data = tempfile::tempdir().unwrap();
        let e = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("build admission must not spawn")),
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(30),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        );
        let request = || SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Hi".into(),
            images: vec![],
            files: vec![],
            choice: crate::router::Choice { vendor: "auto".into(), model: None, effort: None },
        };
        let build = e.local_build_admission().unwrap();
        assert!(e.local_build_admission().is_err());
        assert_eq!(e.send(request()), Err("error.update.activeOperations".into()));
        drop(build);
        assert_eq!(e.send(request()), Err("error.noSubscription".into()));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _build = e.local_build_admission().unwrap();
            panic!("build worker crashed");
        }));
        assert!(e.local_build_admission().is_ok());
        let mut message = crate::model::Message::new("turn".into(), crate::model::Role::Assistant, "".into(), 1);
        message.status = Some(crate::model::Status::Streaming);
        e.with(|core| {
            let id = core.blank(Area::Chat, None);
            core.convs.get_mut(&id).unwrap().messages.push(message);
        });
        assert!(e.local_build_admission().is_err());
        e.shutdown();
    }
    #[tokio::test]
    async fn a_mission_worker_counts_as_work_for_an_install_and_nothing_starts_after_the_decision() {
        let data = tempfile::tempdir().unwrap();
        let e = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("an install must not spawn")),
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(30),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        );
        assert!(!e.is_working());
        e.with(|c| c.mission_sessions.insert("m1".into(), vec![Session::from_fn(|_| {})]));
        assert!(e.is_working(), "a worker alive in a mission is work, with no conversation running");
        assert!(e.local_build_admission().is_err(), "the install is not decided while it runs");
        e.with(|c| c.mission_sessions.remove("m1"));
        assert!(!e.is_working());
        let decided = e.local_build_admission().unwrap();
        assert!(e.with(|c| c.local_build), "from the decision on, no turn and no worker begins");
        drop(decided);
        e.shutdown();
    }
    /// The app drives these from spawned tasks: their futures must stay `Send` (no lock held across an await).
    #[allow(dead_code)]
    fn writer_futures_are_send(
        e: &Engine,
        journal: Journal,
        request: crate::project::budget::Invocation,
        leases: &mut crate::project::lease::Leases,
        lease: &crate::project::Lease,
    ) {
        fn send<T: Send>(_: T) {}
        send(e.dispatch_mission_writer(journal, request, leases, lease));
        send(e.worker_termination("invocation"));
    }
    #[tokio::test]
    async fn a_release_proof_needs_an_owned_tree_the_engine_started() {
        let data = tempfile::tempdir().unwrap();
        let e = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("a proof must not spawn")),
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(30),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        );
        // An invocation nobody launched, and a replayed recording's session, have no tree to retire.
        assert_eq!(e.worker_termination("never-launched").await.err().as_deref(), Some("reason.project.processTreeUnproven"));
        assert!(Session::from_fn(|_| {}).tree().is_none());
        e.shutdown();
    }
    #[test]
    fn role_event_order_and_bounds_preserve_unknown_reservations() {
        let (send, mut receive, overflow) = role_events();
        for n in 0..64 {
            send(EngineEvent::Text(n.to_string()));
        }
        assert!(!overflow.load(std::sync::atomic::Ordering::SeqCst));
        send(EngineEvent::Text("overflow".into()));
        assert!(overflow.load(std::sync::atomic::Ordering::SeqCst));
        for n in 0..64 {
            assert_eq!(receive.try_recv().unwrap(), EngineEvent::Text(n.to_string()));
        }
        let (send, mut receive, overflow) = role_events();
        send(EngineEvent::Text("x".repeat(crate::project::wire::MAX_ENVELOPE + 1)));
        assert!(overflow.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(receive.try_recv().unwrap(), EngineEvent::Error { .. }));
        assert!(Session::from_fn(|_| {}).project_role().is_none());
    }
}
