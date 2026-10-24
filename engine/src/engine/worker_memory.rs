//! New unattended work waits for measured headroom; running work keeps its processes.
use super::*;
use crate::worker_memory::{Registry, Snapshot};
use std::collections::VecDeque;

mod policy;
pub(super) const PENDING: &str = "worker memory admission pending";

type Resume = Box<dyn FnOnce(&mut Core) + Send>;

struct Waiting {
    key: String,
    owner: String,
    resume: Resume,
}

#[derive(Default)]
pub(super) struct State {
    pub registry: Registry,
    pending: VecDeque<Waiting>,
    checking: bool,
    permitted: Option<String>,
    #[cfg(test)]
    probe: Option<Arc<dyn Fn() -> std::io::Result<Snapshot> + Send + Sync>>,
}

#[cfg(test)]
impl State {
    /// Kit-delivery fakes grant memory admission without reading the host's processes.
    pub(super) fn permit_for_test(&mut self, key: &str) {
        self.permitted = Some(key.into());
    }
}

impl Core {
    pub(super) fn worker_ready(&mut self, key: &str) -> bool {
        // shortcut: Linux keeps count limits until its exact native memory sampler is supported.
        if !self.opts.windows && !cfg!(target_os = "macos") {
            return true;
        }
        if self.worker_memory.permitted.as_deref() == Some(key) {
            self.worker_memory.permitted = None;
            return true;
        }
        false
    }

    pub(super) fn waiting_helpers(&self, conv: &str, except: &str) -> usize {
        self.worker_memory.pending.iter().filter(|wait| wait.owner == conv && wait.key.starts_with("helper:") && wait.key != except).count()
    }

    pub(super) fn worker_waiting(&self, key: &str) -> bool {
        self.worker_memory.pending.iter().any(|wait| wait.key == key)
    }

    pub(super) fn cancel_workers(&mut self, owner: &str) {
        self.worker_memory.pending.retain(|wait| wait.owner != owner);
    }

    pub(super) fn workers_waiting(&self) -> bool {
        !self.worker_memory.pending.is_empty()
    }

    pub(super) fn defer_worker(&mut self, key: String, owner: String, resume: impl FnOnce(&mut Core) + Send + 'static) {
        if let Some(wait) = self.worker_memory.pending.iter_mut().find(|wait| wait.key == key) {
            wait.resume = Box::new(resume);
            return;
        }
        self.worker_memory.pending.push_back(Waiting { key, owner, resume: Box::new(resume) });
        if self.worker_memory.checking {
            return;
        }
        self.worker_memory.checking = true;
        let (me, registry) = (self.me.clone(), self.worker_memory.registry.clone());
        self.opts.runtime.spawn(async move {
            loop {
                let sample = registry.clone();
                #[cfg(test)]
                let probe =
                    me.upgrade().and_then(|core| core.lock().unwrap_or_else(|error| error.into_inner()).worker_memory.probe.clone());
                let sample = tokio::task::spawn_blocking(move || {
                    #[cfg(test)]
                    if let Some(probe) = probe {
                        return probe();
                    }
                    sample.snapshot()
                })
                .await;
                let Some(core) = me.upgrade() else { return };
                {
                    let mut core = core.lock().unwrap_or_else(|error| error.into_inner());
                    if core.closing {
                        core.worker_memory.pending.clear();
                    }
                    if !core.installing
                        && let Ok(Ok(Snapshot { used, total, available })) = sample
                        && policy::fits(used, total, available)
                        && let Some(wait) = core.worker_memory.pending.pop_front()
                    {
                        core.worker_memory.permitted = Some(wait.key);
                        (wait.resume)(&mut core);
                        core.worker_memory.permitted = None;
                    }
                    if core.worker_memory.pending.is_empty() {
                        core.worker_memory.checking = false;
                        return;
                    }
                }
                // Only a nonempty queue samples; one fresh sample admits at most one new automatic launch.
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    fn unattended_reply(&self, id: &str, reply: &str) -> bool {
        let Some(conv) = self.convs.get(id) else { return false };
        let Some(end) = conv.messages.iter().position(|message| message.id == reply) else { return false };
        let mut questions = conv.messages[..end].iter().filter(|message| message.role == Role::User);
        let count = questions.clone().count();
        let last = questions.next_back();
        last.is_some_and(|message| message.wake.as_ref().is_some_and(|wake| wake.why != "wake.approvedPlan"))
            || (conv.started_by.is_some() && count == 1)
    }

    pub(super) fn defer_unattended(&mut self, id: &str, reply: &str, pick: &Pick, text: &str, images: &[Image], by_hand: bool) -> bool {
        if !self.unattended_reply(id, reply) || self.worker_ready(id) {
            return false;
        }
        let (id, reply, pick, text, images) = (id.to_string(), reply.to_string(), pick.clone(), text.to_string(), images.to_vec());
        self.defer_worker(id.clone(), id.clone(), move |core| {
            if core.convs.get(&id).is_none_or(|conv| {
                conv.stopped
                    || conv.archived_at.is_some()
                    || conv.messages.last().is_none_or(|message| message.id != reply || message.status != Some(Status::Streaming))
            }) {
                return;
            }
            if let Err(error) = core.dispatch(&id, &reply, &pick, text, images, by_hand) {
                core.fail_reply(&id, &reply, error);
            }
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::answer::tests::{engine, seed};
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    async fn observed(mut receive: tokio::sync::mpsc::UnboundedReceiver<String>, expected: &[&str]) {
        for expected in expected {
            assert_eq!(tokio::time::timeout(Duration::from_secs(5), receive.recv()).await.unwrap().as_deref(), Some(*expected));
        }
        assert!(receive.try_recv().is_err());
    }

    #[tokio::test]
    async fn worker_memory_pressure_and_unknown_hold_fifo_until_capacity_returns() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _) = engine(dir.path());
        let room = Arc::new(AtomicBool::new(false));
        let readings = room.clone();
        let (sent, receive) = tokio::sync::mpsc::unbounded_channel();
        app.with(|core| {
            core.opts.windows = true;
            core.worker_memory.probe = Some(Arc::new(move || {
                if !readings.load(Ordering::Acquire) {
                    return Err(std::io::Error::other("sample unavailable"));
                }
                Ok(Snapshot { used: 1 << 30, total: 16 << 30, available: 4 << 30 })
            }));
            for key in ["first", "second"] {
                let sent = sent.clone();
                core.defer_worker(key.into(), "owner".into(), move |core| {
                    assert!(core.worker_ready(key));
                    assert!(!core.worker_ready(key), "a sample never admits a second launch");
                    sent.send(key.to_string()).unwrap();
                });
            }
        });
        tokio::time::sleep(Duration::from_millis(25)).await;
        app.with(|core| assert_eq!(core.worker_memory.pending.len(), 2));
        room.store(true, Ordering::Release);
        observed(receive, &["first", "second"]).await;
        app.shutdown();
    }

    #[tokio::test]
    async fn worker_memory_stop_removes_waiting_work_and_keeps_other_owners() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _) = engine(dir.path());
        let owner = seed(&app, Session::from_fn(|_| {}), &[("reply", &[])]);
        let (sent, receive) = tokio::sync::mpsc::unbounded_channel();
        app.with(|core| {
            core.opts.windows = true;
            core.worker_memory.probe = Some(Arc::new(|| Ok(Snapshot { used: 0, total: 16 << 30, available: 4 << 30 })));
            core.defer_worker("helper:old".into(), owner.clone(), |_| panic!("stopped helper launched"));
            core.defer_worker("other".into(), "another-owner".into(), move |_| {
                sent.send("other".into()).unwrap();
            });
            core.interrupt(&owner, true);
            assert!(!core.worker_waiting("helper:old"));
        });
        observed(receive, &["other"]).await;
        app.shutdown();
    }

    #[tokio::test]
    async fn worker_memory_automatic_origin_survives_recovery_without_gating_manual_auto() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _) = engine(dir.path());
        let id = seed(&app, Session::from_fn(|_| {}), &[("reply", &[])]);
        app.with(|core| {
            assert!(!core.unattended_reply(&id, "reply"), "manual Auto has no automatic wake");
            core.convs.get_mut(&id).unwrap().started_by = Some("parent".into());
            assert!(core.unattended_reply(&id, "reply"), "a child's initial brief is automatic");
            let conv = core.convs.get_mut(&id).unwrap();
            conv.messages.push(Message::new("manual".into(), Role::User, "hello".into(), 3));
            conv.messages.push(Message::new("later".into(), Role::Assistant, String::new(), 4));
            assert!(!core.unattended_reply(&id, "later"), "the person's later child turn stays manual");
            core.convs.get_mut(&id).unwrap().messages[2].wake = Some(Wake::of("wake.asked"));
            assert!(core.unattended_reply(&id, "later"));
            core.convs.get_mut(&id).unwrap().messages[2].wake = Some(Wake::of("wake.approvedPlan"));
            assert!(!core.unattended_reply(&id, "later"), "approval is the person's own send");
            assert!(!core.unattended_reply(&id, "missing"));
        });
        app.shutdown();
    }

    #[tokio::test]
    async fn worker_memory_replaced_reply_cannot_resume_and_pending_work_blocks_install() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _) = engine(dir.path());
        let session = Session::from_fn(|command| {
            assert!(!matches!(command, crate::events::SessionCommand::Send(_)), "stale work reached the CLI");
        });
        let id = seed(&app, session, &[("reply", &[])]);
        app.with(|core| {
            core.opts.windows = true;
            core.worker_memory.probe = Some(Arc::new(|| Ok(Snapshot { used: 0, total: 16 << 30, available: 4 << 30 })));
            core.convs.get_mut(&id).unwrap().messages[0].wake = Some(Wake::of("wake.asked"));
            let pick = Pick { vendor: Vendor::Claude, model: None, effort: "medium".into(), reason: "reason.manual".into(), pace: None };
            assert!(core.defer_unattended(&id, "reply", &pick, "old brief", &[], false));
            core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().id = "replacement".into();
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if app.with(|core| !core.workers_waiting()) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        app.with(|core| {
            core.worker_memory.probe = Some(Arc::new(|| Err(std::io::Error::other("unknown"))));
            core.defer_worker("helper:pending".into(), "owner".into(), |_| panic!("unknown sample admitted work"));
            assert_eq!(core.waiting_helpers("owner", ""), 1);
            assert_eq!(core.waiting_helpers("owner", "helper:pending"), 0);
        });
        assert!(app.install_admission().is_err());
        app.with(|core| core.cancel_workers("owner"));
        app.shutdown();
    }
}
