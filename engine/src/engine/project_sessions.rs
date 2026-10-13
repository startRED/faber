//! Dedicated role sessions share the runner command lock, never Chat's live sessions.
use super::*;
use crate::project::{Role as ProjectRole, run::Runner, store::Journal};

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
    pub fn mission_lock(&self) -> Arc<tokio::sync::Mutex<()>> {
        self.with(|c| c.mission_lock.clone())
    }

    pub async fn dispatch_mission_worker(&self, journal: Journal, request: crate::project::budget::Invocation) -> Result<(), String> {
        let coordinator = self.mission_lock();
        let _turn = coordinator.lock().await;
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
        self.with(|c| c.mission_sessions.entry(runner.mission.id.clone()).or_default().push(session.clone()));
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
