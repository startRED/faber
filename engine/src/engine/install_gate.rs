//! The engine's side of installing an update: while the install is decided nothing new starts. An unattended install cuts
//! nothing that runs; the person's own restart does, and the work goes on after it.
use super::*;

/// The engine's admission closed for an install; it reopens when this drops, even on an unwind.
pub struct InstallGuard(Engine);
impl Drop for InstallGuard {
    fn drop(&mut self) {
        self.0.with(|c| {
            c.installing = false;
            c.retry_reports();
        });
    }
}

impl Engine {
    /// Closes admission for the install: refused while an answer runs or a message waits, and every new turn is refused
    /// from here on until the guard drops.
    pub fn install_admission(&self) -> Result<InstallGuard, String> {
        self.with(|c| {
            // An install the agent was allowed to make is work too, though Stop left nothing in the list.
            if c.installing
                || c.agent_installs > 0
                || c.workers_waiting()
                || !c.running_items().is_empty()
                || c.convs.values().any(|v| !v.queued.is_empty())
            {
                return Err("error.update.activeOperations".into());
            }
            c.installing = true;
            Ok(InstallGuard(self.clone()))
        })
    }

    /// Closes admission for the install the person asked for ("Update ready, restart"), even while work runs: the
    /// shutdown that follows cuts it, and `note_resume` brings it back at the next start. Only an install the agent was
    /// allowed to make still refuses it, since cutting one leaves a half-installed program.
    pub fn install_admission_stopping(&self) -> Result<InstallGuard, String> {
        self.with(|c| {
            if c.installing || c.agent_installs > 0 {
                return Err("error.update.activeOperations".into());
            }
            c.installing = true;
            Ok(InstallGuard(self.clone()))
        })
    }

    /// Right before the restart for an update: writes down the conversations at work (a turn running, or messages waiting)
    /// that Keep going does not bring back by itself, so `resume_after_update` tells each to go on.
    pub fn note_resume(&self) {
        self.with(|c| {
            // Each with its last message, so a conversation the person took up again by the time it goes on is left alone.
            let cut: Vec<(&String, &str)> = c
                .convs
                .values()
                .filter(|v| v.keep_going.is_none())
                .filter(|v| c.live.get(&v.id).is_some_and(|l| l.turn.is_some()) || !v.queued.is_empty())
                .map(|v| (&v.id, v.messages.last().map_or("", |m| m.id.as_str())))
                .collect();
            let path = c.opts.data_dir.join(RESUME_FILE);
            if cut.is_empty() {
                let _ = std::fs::remove_file(path);
            } else if let Ok(text) = serde_json::to_string(&cut) {
                let _ = std::fs::write(path, text);
            }
        })
    }

    /// At start, once the accounts are read: each conversation the restart for an update cut goes on, after a short wait,
    /// with a turn Faber sends by itself (`wake.updated`). The list is read once and deleted, so a crash never repeats it.
    pub fn resume_after_update(&self) {
        let path = self.with(|c| c.opts.data_dir.join(RESUME_FILE));
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let _ = std::fs::remove_file(&path);
        let cut: Vec<(String, String)> = serde_json::from_str(&text).unwrap_or_default();
        if cut.is_empty() {
            return;
        }
        self.with(|c| c.update_resumes.extend(cut.iter().map(|(id, _)| id.clone())));
        let (engine, wait) = (self.clone(), self.with(|c| RESUME_WAIT * c.keep_pace));
        self.with(|c| c.opts.runtime.clone()).spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            engine.with(|c| cut.iter().for_each(|(id, last)| c.resume_one(id, last)));
        });
    }
}

impl Core {
    /// One conversation the restart cut goes on, unless something newer happened in it meanwhile (the person wrote, or it
    /// already works). Its waiting messages stay behind the turn that picks up: a cut reply does not drain the queue, so
    /// the turn goes out first and the queue follows when it ends.
    fn resume_one(&mut self, id: &str, last: &str) {
        let Some(v) = self.load(id) else { return };
        if v.keep_going.is_some() || v.messages.last().map_or("", |m| m.id.as_str()) != last {
            return;
        }
        if self.live.get(id).is_some_and(|l| l.turn.is_some()) {
            return;
        }
        let _ = self.send_by_itself(id, RESUME.into(), crate::model::Wake::of("wake.updated"));
        self.save(id);
        self.emit_queue(id);
    }
}
/// The conversations to bring back after the restart for an update, in the data folder.
const RESUME_FILE: &str = "resume-after-update.json";
/// The wait before they go on, so the accounts and quotas are read first.
pub(super) const RESUME_WAIT: f64 = 20.0;
/// What the agent is told when Faber brings it back; the person sees `wake.updated` in their language instead.
const RESUME: &str = "Faber restarted to install an update, which cut your last turn short. Pick up exactly where you left off \
and finish what you were doing; do not start over.";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn t515a_update_admission_snapshots_keep_accepted_queue_and_attachments() {
        recovery_snapshots(false).await;
    }

    #[tokio::test]
    async fn t515a_interrupted_admission_snapshots_keep_accepted_queue_and_attachments() {
        recovery_snapshots(true).await;
    }

    async fn recovery_snapshots(interrupted: bool) {
        let dir = crate::workspace::TestRoot::new();
        let (app, _) = crate::engine::answer::tests::engine(&dir);
        let id = crate::engine::answer::tests::seed(&app, crate::events::Session::from_fn(|_| {}), &[("cut", &[])]);
        let snapshots = Arc::new(Mutex::new(Vec::new()));
        let seen = snapshots.clone();
        let admitted = Arc::new(tokio::sync::Notify::new());
        let arrived = admitted.clone();
        let why = if interrupted { "wake.resumed" } else { "wake.updated" };
        let (data, conv_id) = (dir.path().to_path_buf(), id.clone());
        app.with_saves(false, |c| {
            c.live.clear();
            c.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
            let conv = c.convs.get_mut(&id).unwrap();
            conv.choice = Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
            let reply = conv.messages.last_mut().unwrap();
            reply.status = Some(Status::Stopped);
            reply.closed = true;
            conv.queued.push(QueuedTurn {
                id: "accepted".into(),
                text: "My next request".into(),
                images: None,
                attachments: Some(vec!["notes.txt".into()]),
                choice: conv.choice.clone().unwrap(),
                at: now(),
                sending: false,
                wake: None,
            });
            c.try_save(&id).unwrap();
            c.emit = Arc::new(move |_| {
                if let Some(saved) = crate::store::Store::new(&data).get(&conv_id) {
                    if saved.messages.iter().any(|m| m.wake.as_ref().is_some_and(|w| w.why == why)) {
                        arrived.notify_one();
                    }
                    seen.lock().unwrap().push(saved);
                }
            });
        });
        app.note_account_events(
            Vendor::Claude,
            vec![EngineEvent::Signals(crate::signals::Signals {
                billing: Some(crate::budget::Billing::Subscription),
                ..Default::default()
            })],
        );
        if interrupted {
            app.with(|c| {
                c.convs.remove(&id);
                c.running.items.clear();
            });
            app.set_keep_going_pace(0.0001);
            app.resume_interrupted();
            tokio::time::timeout(Duration::from_secs(5), admitted.notified()).await.expect("interrupted recovery admission");
        } else {
            app.with(|c| c.resume_one(&id, "cut"));
        }
        let snapshots = snapshots.lock().unwrap();
        assert!(
            snapshots.iter().any(|s| s.messages.iter().any(|m| m.wake.as_ref().is_some_and(|w| w.why == why))),
            "recovery must reach admission: {why}"
        );
        for saved in snapshots.iter() {
            assert_eq!(saved.queued.len(), 1, "recovery snapshot dropped accepted work: {why}");
            assert_eq!(saved.queued[0].id, "accepted");
            assert_eq!(saved.queued[0].attachments.as_deref(), Some(["notes.txt".to_string()].as_slice()));
        }
        drop(snapshots);
        app.shutdown();
    }

    #[tokio::test]
    async fn install_reserves_chat_admission_and_releases_on_unwind() {
        let data = crate::workspace::TestRoot::new();
        let e = Engine::new(
            EngineOptions {
                data_dir: data.path().into(),
                #[cfg(windows)]
                chat_dir: data.work().into(),
                clis: Default::default(),
                spawn: Arc::new(|_, _, _| panic!("install admission must not spawn")),
                windows: false,
                claude_env_billed: false,
                max_warm: 0,
                idle: Duration::from_secs(30),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        let request = || SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Hi".into(),
            images: vec![],
            files: vec![],
            choice: crate::router::Choice { vendor: "auto".into(), model: None, effort: None },
        };
        let install = e.install_admission().unwrap();
        assert!(e.install_admission().is_err());
        assert_eq!(e.send(request()), Err("error.update.activeOperations".into()));
        drop(install);
        assert_eq!(e.send(request()), Err("error.noSubscription".into()));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _install = e.install_admission().unwrap();
            panic!("install crashed");
        }));
        assert!(e.install_admission().is_ok());
        let mut message = crate::model::Message::new("turn".into(), crate::model::Role::Assistant, "".into(), 1);
        message.status = Some(crate::model::Status::Streaming);
        e.with(|core| {
            let id = core.blank(Area::Chat, None);
            core.convs.get_mut(&id).unwrap().messages.push(message);
        });
        assert!(e.install_admission().is_err());
        let stopping = e.install_admission_stopping().expect("the person's restart goes ahead while an answer runs");
        assert!(e.install_admission_stopping().is_err(), "one install at a time");
        assert_eq!(e.send(request()), Err("error.update.activeOperations".into()));
        drop(stopping);
        e.with(|core| core.agent_installs += 1);
        assert!(e.install_admission_stopping().is_err(), "an install the agent makes is never cut");

        // The restart notes the conversations with work waiting, leaves Keep going to itself, and the list is read once.
        let queued = || QueuedTurn {
            id: new_id(),
            text: "next".into(),
            images: None,
            attachments: None,
            choice: crate::router::Choice { vendor: "auto".into(), model: None, effort: None },
            at: now(),
            sending: false,
            wake: None,
        };
        let (waiting, going) = e.with(|core| {
            let (waiting, going) = (core.blank(Area::Chat, None), core.blank(Area::Chat, None));
            core.convs.get_mut(&waiting).unwrap().queued.push(queued());
            let g = core.convs.get_mut(&going).unwrap();
            g.queued.push(queued());
            g.keep_going = Some(Default::default());
            (waiting, going)
        });
        e.note_resume();
        let file = data.path().join(RESUME_FILE);
        let noted: Vec<(String, String)> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(noted, [(waiting.clone(), String::new())], "{going} goes on by Keep going");
        e.set_keep_going_pace(1e6);
        e.resume_after_update();
        assert!(!file.exists(), "read once, so a crash after it never repeats it");
        e.with(|core| {
            let first = core.convs[&waiting].queued[0].id.clone();
            core.resume_one(&waiting, "a newer message");
            assert_eq!(core.convs[&waiting].queued.len(), 1, "taken up meanwhile: left alone");
            // Nothing is connected here, so the turn is refused; the waiting message stays, first in line.
            core.resume_one(&waiting, "");
            assert_eq!(core.convs[&waiting].queued.iter().map(|q| q.id.clone()).collect::<Vec<_>>(), [first]);
        });
        e.with(|core| core.convs.get_mut(&waiting).unwrap().queued.clear());
        e.note_resume();
        assert!(!file.exists(), "nothing at work, nothing noted");
        e.shutdown();
    }
}
