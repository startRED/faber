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

impl Engine {
    /// Reserve quiet Chat admission for the entire build/canary/promotion attempt.
    pub fn local_build_admission(&self) -> Result<LocalBuildGuard, String> {
        self.with(|c| {
            if c.local_build || !c.running_items().is_empty() || c.convs.values().any(|v| !v.queued.is_empty()) {
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
            let project = projects.iter().find(|p| p.id == recorded.mission.project_id).ok_or("error.journal.mismatch")?;
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
        let coordinator = self.mission_lock();
        let _turn = coordinator.lock().await;
        let _update = self.mission_update_admission()?;
        let mut runner = Runner::load(&journal).map_err(str::to_string)?;
        runner.admit(request.vendor).map_err(str::to_string)?;
        let worker = runner.workers.get(&request.worker).ok_or("error.wire.stale")?.clone();
        // The qualified writer launcher must supply T4's owned process identity before source can be unlocked.
        if worker.brief.role == ProjectRole::Writer {
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
                        area: Area::Project,
                        model: Some(worker.pin.model.clone()),
                        effort: Some(worker.pin.effort.clone()),
                        resume: None,
                        user_hooks: false,
                        instructions: None,
                        user_name: None,
                    },
                    spawn,
                },
                worker.brief.role,
                sink,
            ),
            Vendor::Codex => CodexServer::start_project(
                CodexServerOptions { bin: cli.command, cwd: cwd.clone(), area: Area::Project, user_hooks: false, windows, spawn },
                CodexThread {
                    cwd,
                    model: Some(worker.pin.model.clone()),
                    effort: Some(worker.pin.effort.clone()),
                    resume: None,
                    instructions: None,
                    user_name: None,
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
        self.with(|c| {
            c.mission_sessions.entry(runner.mission.id.clone()).or_default().push(session.clone());
            c.mission_projects.insert(runner.mission.id.clone(), runner.mission.project_id.clone());
        });
        session.send(UserTurn::text(prompt));
        Ok(())
    }

    pub(super) fn mission_event(
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
                let mut receipt = crate::project::Receipt::new(invocation, crate::project::Outcome::Done, self.mission_time());
                receipt.evidence.insert("invocation".into(), invocation.into());
                journal.record_receipt(&receipt).map_err(|e| e.key().to_string())?;
            }
            EngineEvent::Text(delta) => {
                if text.len().saturating_add(delta.len()) > crate::project::wire::MAX_ENVELOPE {
                    return Err("error.wire.tooBig".into());
                }
                text.push_str(&delta);
            }
            event @ (EngineEvent::Done { .. } | EngineEvent::Error { .. }) => {
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
                if matches!(event, EngineEvent::Done { .. }) && runner.mission.state != crate::project::MissionState::Paused {
                    let output = std::mem::take(text);
                    runner.receive(journal, task, &output).map_err(str::to_string)?;
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
