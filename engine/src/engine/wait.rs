//! One finite return per declared wait, shared by timers and checks of an exact saved deadline.
use super::*;
use crate::model::WaitRestart;

fn admission_blocker(problem: &str) -> bool {
    matches!(problem, "error.noSubscription" | "reason.budget.other")
}

const STATE_CHECK: &str = "Check the actual state of the work you were waiting for, including any background command or helper interrupted by restart. Continue if possible; otherwise state the concrete blocker and who must act.";
const SAVE_RETRY_MS: u64 = 15_000;

pub(super) struct WaitTimer {
    since: u64,
    until: u64,
    at: u64,
    retrying_save: bool,
    task: tokio::task::AbortHandle,
}

impl WaitTimer {
    fn matches(&self, wait: &WaitingOn) -> bool {
        self.since == wait.since && self.until == wait.until
    }
}

enum WaitSaveRetry {
    Restore,
    CheckDeadline(String),
    Return(String),
}

fn storage_error(key: &str) -> bool {
    matches!(key, "error.chat.saveFailed" | "error.chat.saveFull")
}

fn storage_blocked_wait(c: &Conversation) -> Option<&WaitingOn> {
    c.waiting_on
        .as_ref()
        .filter(|w| !c.stopped && c.archived_at.is_none() && w.storage_blocked && w.problem.as_deref().is_some_and(storage_error))
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum WaitCheck {
    Obsolete,
    Closing,
    TooSoon { until: u64 },
    Deferred { until: u64 },
    DecisionPending,
    AlreadyHandled,
    Admitted,
    Blocked { reason: String },
}

fn receipt_id(since: u64, until: u64) -> String {
    format!("wait-{since}-{until}")
}

fn has_receipt(c: &Conversation, receipt: &str) -> bool {
    c.queued.iter().any(|q| q.id == receipt) || c.messages.iter().any(|m| m.id == receipt)
}

pub(super) fn has_open_permission(c: &Conversation) -> bool {
    c.messages.last().is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))
}

pub(super) fn hold_queued_returns(c: &mut Conversation) {
    let Some(last) = c.messages.last() else { return };
    for wake in c.queued.iter_mut().filter_map(|q| q.wake.as_mut()).filter(|w| w.why == "wake.wait") {
        wake.wait_restart = Some(WaitRestart::NeedsSend { after: last.id.clone() });
    }
}

pub(super) fn needs_send(q: &QueuedTurn) -> bool {
    q.wake.as_ref().is_some_and(|w| matches!(w.wait_restart, Some(WaitRestart::NeedsSend { .. })))
}

pub(super) fn release_hold(q: &mut QueuedTurn) -> Option<WaitRestart> {
    let wake = q.wake.as_mut().filter(|w| matches!(w.wait_restart, Some(WaitRestart::NeedsSend { .. })))?;
    wake.wait_restart.take()
}

pub(super) fn permission_holds_return(c: &Conversation, q: &QueuedTurn) -> bool {
    q.wake.as_ref().is_some_and(|w| w.why == "wake.wait") && has_open_permission(c)
}

pub(super) fn cut_while_resuming_wait(c: &Conversation) -> bool {
    c.messages.last().is_some_and(|m| m.closed)
        && c.messages.iter().rev().nth(1).and_then(|m| m.wake.as_ref()).is_some_and(|w| w.wait_restart == Some(WaitRestart::Resumed))
}

impl Engine {
    /// The local control channel may check a wait it already observed, with no words of its own to send.
    pub fn wake_wait(&self, id: &str, since: u64, until: u64) -> WaitCheck {
        self.with(|c| c.wake_wait(id, since, until))
    }
}

impl Core {
    /// A saved head return can already do the work of a new restart turn. Leave everything parked if it cannot go out.
    pub(super) fn resume_queued_wait(&mut self, id: &str) -> Option<bool> {
        let c = self.convs.get(id)?;
        if c.queued.iter().any(|q| {
            permission_holds_return(c, q)
                || matches!(
                    q.wake.as_ref().and_then(|w| w.wait_restart.as_ref()),
                    Some(WaitRestart::NeedsSend { after }) if c.messages.last().is_some_and(|m| m.id == *after)
                )
        }) {
            return Some(false);
        }
        let at = c.queued.iter().position(|q| !needs_send(q))?;
        let head = &c.queued[at];
        if head.wake.as_ref().is_none_or(|w| w.why != "wake.wait") {
            return None;
        }
        if self.closing
            || c.archived_at.is_some()
            || c.stopped
            || c.compacting
            || head.sending
            || c.messages.last().is_some_and(|m| {
                m.error.as_deref() == Some(silence::SHARED_DELIVERY_UNKNOWN) || (m.status == Some(Status::Stopped) && !m.closed)
            })
            || self.preparing_rewind_steer(&head.id)
            || self.goal_out_of_money(id)
            || !self.may_spend(id)
        {
            return Some(false);
        }
        let mut item = self.convs.get_mut(id).expect("queued conversation exists").queued.remove(at);
        item.wake.as_mut().expect("wait return").wait_restart = Some(WaitRestart::Resumed);
        // The durable admission saves the receipt and removes the queue entry together. A refusal puts it back at the head.
        let sent = self.dispatch_queued(id, item).is_ok();
        self.emit_queue(id);
        Some(sent)
    }

    fn wake_wait(&mut self, id: &str, since: u64, until: u64) -> WaitCheck {
        if self.closing {
            return WaitCheck::Closing;
        }
        if until <= since {
            return WaitCheck::Obsolete;
        }
        let Some(c) = self.load(id).filter(|c| c.area == Area::Project && !c.temporary) else { return WaitCheck::Obsolete };
        let project = c.project_id.clone();
        if project.is_none_or(|id| !self.store.projects().iter().any(|p| p.id == id)) {
            return WaitCheck::Obsolete;
        }
        let c = &self.convs[id];
        let receipt = receipt_id(since, until);
        if has_receipt(c, &receipt) {
            return WaitCheck::AlreadyHandled;
        }
        let Some(wait) = c.waiting_on.as_ref().filter(|w| w.since == since && w.until == until) else { return WaitCheck::Obsolete };
        let blocked = if c.stopped {
            Some("reason.wait.stopped")
        } else if c.archived_at.is_some() {
            Some("reason.wait.archived")
        } else {
            wait.problem.as_deref()
        };
        if let Some(reason) = blocked {
            return WaitCheck::Blocked { reason: reason.into() };
        }
        if until > (self.secs() * 1000.0) as u64 {
            return WaitCheck::TooSoon { until };
        }
        if has_open_permission(c) {
            return WaitCheck::DecisionPending;
        }
        if let Some(at) = self.wait_save_retry_at(id, wait) {
            return WaitCheck::Deferred { until: at };
        }
        self.cancel_wait_timer(id);
        self.check_wait_deadline(id, until, "A local control client requested a check of this overdue saved wait.", true);
        let c = &self.convs[id];
        // A receipt that was saved before starting the reply still counts if the reply later fails.
        if has_receipt(c, &receipt) {
            return WaitCheck::Admitted;
        }
        match c.waiting_on.as_ref() {
            Some(wait) => match &wait.problem {
                Some(reason) => WaitCheck::Blocked { reason: reason.clone() },
                None => WaitCheck::Deferred { until: self.wait_save_retry_at(id, wait).unwrap_or(wait.until) },
            },
            None => WaitCheck::Admitted,
        }
    }

    pub(super) fn restore_wait(&mut self, id: &str) {
        self.restore_wait_attempt(id, true);
    }

    pub(super) fn wait_storage_saved(&self, id: &str) {
        let Some(wait) = self.convs.get(id).and_then(storage_blocked_wait).filter(|_| !self.closing) else { return };
        if self.wait_save_retry_at(id, wait).is_some() {
            return;
        }
        let (since, until) = (wait.since, wait.until);
        let (me, runtime, id) = (self.me.clone(), self.opts.runtime.clone(), id.to_string());
        self.opts.runtime.spawn(async move {
            let Some(core) = me.upgrade() else { return };
            Engine { core, runtime }.with(|c| {
                if !c.closing
                    && c.convs
                        .get(&id)
                        .and_then(storage_blocked_wait)
                        .is_some_and(|w| w.since == since && w.until == until && c.wait_save_retry_at(&id, w).is_none())
                {
                    c.restore_wait_attempt(&id, true);
                }
            });
        });
    }

    fn restore_wait_attempt(&mut self, id: &str, retry_storage: bool) {
        let Some(c) = self.load(id) else { return };
        let Some(wait) = c.waiting_on.clone() else { return };
        let recovered = storage_blocked_wait(c).is_some();
        if recovered {
            let current = c.waiting_on.as_mut().unwrap();
            current.problem = None;
            current.storage_blocked = false;
        }
        // Also writes the finite fallback for pre-deadline records, without moving their clock.
        match self.try_save(id) {
            Ok(()) => {
                self.arm_wait(id);
                self.restore_wait_pr(id);
                if recovered {
                    self.list_changed();
                }
            }
            Err(error) if recovered || wait.problem.is_none() => {
                self.wait_save_failed(id, wait, error, retry_storage.then_some(WaitSaveRetry::Restore));
            }
            Err(_) => {}
        }
    }

    /// Only account/quota evidence reopens a refusal made before dispatch. Delivery reservations never reopen.
    pub(super) fn recheck_wait_admission(&mut self) {
        if self.closing {
            return;
        }
        let ready: Vec<String> = self
            .convs
            .iter()
            .filter(|(_, c)| {
                !c.stopped
                    && c.archived_at.is_none()
                    && c.waiting_on.as_ref().and_then(|w| w.problem.as_deref()).is_some_and(admission_blocker)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let mut changed = false;
        for id in ready {
            if self.goal_out_of_money(&id) {
                continue;
            }
            if let Some(problem) = self.autonomous_spend_blocker(&id) {
                let Some(wait) = self.convs.get_mut(&id).and_then(|c| c.waiting_on.as_mut()) else { continue };
                if wait.problem.as_deref() != Some(problem) {
                    wait.problem = Some(problem.into());
                    self.save(&id);
                    changed = true;
                }
                continue;
            }
            self.return_from_wait(&id, "New account or quota evidence now permits the saved state check.");
        }
        if changed {
            self.list_changed();
        }
    }

    pub(super) fn arm_wait(&mut self, id: &str) {
        let Some(w) = self.convs.get(id).and_then(|c| c.waiting_on.as_ref()).filter(|w| w.problem.is_none()) else { return };
        if self.wait_timers.get(id).is_some_and(|timer| timer.matches(w)) {
            return;
        }
        self.schedule_wait(id, w.until, None);
    }

    pub(super) fn cancel_wait_timer(&mut self, id: &str) {
        if let Some(timer) = self.wait_timers.remove(id) {
            timer.task.abort();
        }
    }

    fn wait_save_retry_at(&self, id: &str, wait: &WaitingOn) -> Option<u64> {
        self.wait_timers.get(id).filter(|timer| timer.retrying_save && timer.matches(wait)).map(|timer| timer.at)
    }

    fn schedule_wait(&mut self, id: &str, at: u64, retry: Option<WaitSaveRetry>) {
        let Some(wait) = self.convs.get(id).and_then(|c| c.waiting_on.as_ref()).filter(|_| !self.closing) else { return };
        let (since, until, problem) = (wait.since, wait.until, wait.problem.clone());
        let retrying_save = retry.is_some();
        self.cancel_wait_timer(id);
        let delay = Duration::from_millis(at.saturating_sub((self.secs() * 1000.0) as u64));
        let (me, runtime, conv) = (self.me.clone(), self.opts.runtime.clone(), id.to_string());
        let task = self.opts.runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            let Some(core) = me.upgrade() else { return };
            Engine { core, runtime }.with(|c| {
                if c.wait_timers.get(&conv).is_none_or(|timer| timer.task.id() != tokio::task::id()) {
                    return;
                }
                c.wait_timers.remove(&conv);
                let current =
                    c.convs.get(&conv).filter(|row| !row.stopped && row.archived_at.is_none()).and_then(|row| row.waiting_on.as_ref());
                if c.closing || current.is_none_or(|w| w.since != since || w.until != until || w.problem != problem) {
                    return;
                }
                match retry {
                    None => c.check_wait_deadline(&conv, until, "The saved wait deadline has arrived.", true),
                    Some(WaitSaveRetry::Restore) => c.restore_wait_attempt(&conv, false),
                    Some(WaitSaveRetry::CheckDeadline(cause)) => c.check_wait_deadline(&conv, until, &cause, false),
                    Some(WaitSaveRetry::Return(cause)) => c.return_from_wait_attempt(&conv, &cause, false),
                }
            });
        });
        self.wait_timers.insert(id.to_string(), WaitTimer { since, until, at, retrying_save, task: task.abort_handle() });
    }

    fn check_wait_deadline(&mut self, id: &str, until: u64, cause: &str, retry_storage: bool) {
        let children = &self.convs[id].waiting_on.as_ref().unwrap().ids;
        let live_child = !children.is_empty()
            && children.iter().all(|id| {
                self.convs.get(id).is_some_and(|child| {
                    let asks_person = child
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.role == Role::Assistant)
                        .is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()));
                    !asks_person
                        && child.waiting_on.as_ref().is_none_or(|w| w.problem.is_none())
                        && siblings::reports_later(self.live_sibling_state(child), child)
                })
            });
        let live_helper = children.is_empty() && !self.working_helpers(id).is_empty();
        if live_child || live_helper || has_open_permission(&self.convs[id]) {
            self.defer_wait(id, until, retry_storage.then(|| WaitSaveRetry::CheckDeadline(cause.to_string())));
        } else {
            self.return_from_wait_attempt(id, &format!("{cause} {STATE_CHECK}"), retry_storage);
        }
    }

    fn defer_wait(&mut self, id: &str, until: u64, retry: Option<WaitSaveRetry>) {
        let interval = (300_000.0 * self.keep_pace).max(1.0) as u64;
        let next = until.max((self.secs() * 1000.0) as u64).saturating_add(interval);
        let wait = self.convs.get_mut(id).unwrap().waiting_on.as_mut().unwrap();
        let previous = wait.clone();
        wait.problem = None;
        wait.until = next;
        if let Err(error) = self.try_save(id) {
            self.wait_save_failed(id, previous, error, retry);
            return;
        }
        self.arm_wait(id);
        self.list_changed();
    }

    fn wait_save_failed(&mut self, id: &str, mut wait: WaitingOn, error: String, retry: Option<WaitSaveRetry>) {
        let Some(c) = self.convs.get_mut(id) else { return };
        let active = !self.closing && !c.stopped && c.archived_at.is_none();
        let retry = retry.filter(|_| active);
        if retry.is_none() {
            if active && storage_error(&error) {
                wait.problem = Some(error);
                wait.storage_blocked = true;
            } else {
                wait.problem.get_or_insert(error);
            }
        }
        c.waiting_on = Some(wait);
        if retry.is_some() {
            // Keep the original saved deadline while waiting for a second local write.
            let at = ((self.secs() * 1000.0) as u64).saturating_add(SAVE_RETRY_MS);
            self.schedule_wait(id, at, retry);
        }
        self.list_changed();
    }

    /// Failed admission remains visible and durable, rather than retrying a paid turn on a loop.
    pub(super) fn return_from_wait(&mut self, id: &str, cause: &str) {
        self.return_from_wait_attempt(id, cause, true);
    }

    fn return_from_wait_attempt(&mut self, id: &str, cause: &str, retry_storage: bool) {
        if self.closing {
            return;
        }
        let Some(wait) = self.load(id).and_then(|c| c.waiting_on.clone()) else { return };
        let receipt = receipt_id(wait.since, wait.until);
        let c = &self.convs[id];
        if has_receipt(c, &receipt) {
            self.convs.get_mut(id).unwrap().waiting_on = None;
            self.cancel_wait_timer(id);
            self.save(id);
            self.list_changed();
            return;
        }
        if wait
            .problem
            .as_deref()
            .is_some_and(|p| !admission_blocker(p) && !matches!(p, "reason.wait.stopped" | "reason.wait.archived" | goals::CAP))
        {
            return;
        }
        // Reserve before dispatch: an uncertain crash leaves a visible blocker, never a second paid attempt.
        let blocked = if c.stopped {
            Some("reason.wait.stopped")
        } else if c.archived_at.is_some() {
            Some("reason.wait.archived")
        } else if has_open_permission(c) {
            self.defer_wait(id, wait.until, retry_storage.then(|| WaitSaveRetry::Return(cause.to_string())));
            return;
        } else if self.goal_out_of_money(id) {
            Some(goals::CAP)
        } else {
            self.autonomous_spend_blocker(id)
        };
        let mut reserved = wait.clone();
        reserved.storage_blocked = false;
        reserved.problem = Some(blocked.unwrap_or("reason.wait.delivery").into());
        self.convs.get_mut(id).unwrap().waiting_on = Some(reserved.clone());
        if let Err(error) = self.try_save(id) {
            if blocked.is_none() {
                self.wait_save_failed(id, wait, error, retry_storage.then(|| WaitSaveRetry::Return(cause.to_string())));
            } else {
                // Keep the actual admission blocker. A storage recovery is not new account or quota evidence.
                self.cancel_wait_timer(id);
                self.list_changed();
            }
            return;
        }
        self.cancel_wait_timer(id);
        if blocked.is_none() {
            let text = format!(
                "[Faber] {cause}\nYour saved wait: {}\nReply in the conversation's language. Do not repeat a wait without new evidence.",
                wait.note
            );
            if let Err(why) = self.send_by_itself_with_id(id, text, Wake::of("wake.wait"), receipt) {
                reserved.problem = Some(why);
                self.convs.get_mut(id).unwrap().waiting_on = Some(reserved);
                self.save(id);
            }
        }
        self.list_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Session;
    use crate::store::{Disk, RealDisk};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct FailWaitWrite {
        path: PathBuf,
        remaining: Mutex<u64>,
    }

    impl Disk for FailWaitWrite {
        fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
            if path == self.path {
                let mut remaining = self.remaining.lock().unwrap();
                if *remaining > 0 {
                    *remaining -= 1;
                    return Err(std::io::Error::other("the wait could not be saved"));
                }
            }
            RealDisk.write(path, bytes)
        }

        fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
            RealDisk.rename(from, to)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_child_waiting_for_its_command_keeps_the_lead_waiting() {
        assert_command_wait_keeps_lead_waiting(false);
    }

    #[tokio::test(start_paused = true)]
    async fn a_child_waiting_for_its_command_in_keep_going_keeps_the_lead_waiting() {
        assert_command_wait_keeps_lead_waiting(true);
    }

    fn assert_command_wait_keeps_lead_waiting(keep_going: bool) {
        use super::super::command_wait::tests::{WAITING, turn_ends_with_render};

        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let child = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with(|core| {
            if keep_going {
                core.command_waits.remove(&child);
                core.convs.get_mut(&child).unwrap().keep_going = Some(KeepGoing::default());
                core.arm(&child, "r1", 900.0, "command");
            }
            core.handle(&child, 1, EngineEvent::Error { message: "error.driverStopped".into(), fatal: true });
            assert!(!core.live.contains_key(&child));
            assert_eq!(core.keep_going_command_wait(&child).is_some(), keep_going);
            assert_eq!(core.waiting_command(&child).is_some(), !keep_going);
        });
        let lead = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
        engine.with_saves(false, |core| {
            let parent = core.convs.get_mut(&lead).unwrap();
            parent.area = Area::Project;
            parent.project_id = Some("test-project".into());
            core.convs.get_mut(&child).unwrap().started_by = Some(lead.clone());
        });
        engine.session_wait(&lead, vec![child], Some("child render result".into())).unwrap();
        engine.with_saves(false, |core| {
            let deadline = core.convs[&lead].waiting_on.as_ref().unwrap().until;
            core.cancel_wait_timer(&lead);
            core.check_wait_deadline(&lead, deadline, "Simulated deadline.", true);
            let parent = &core.convs[&lead];
            let wait = parent.waiting_on.as_ref().expect("the lead is still waiting for the child's command");
            assert!(wait.until > deadline && wait.problem.is_none());
            assert!(parent.queued.is_empty(), "the lead was woken while the child still has work running");
        });
        engine.shutdown();
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_deadline_extension_gets_one_storage_retry() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
        let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
        engine.session_wait(&id, vec![], Some("child result".into())).unwrap();
        let disk =
            Arc::new(FailWaitWrite { path: dir.path().join("conversations").join(format!("{id}.json.tmp")), remaining: Mutex::new(1) });
        let before = engine.with_saves(false, |core| {
            core.store = Arc::new(Store::with_disk(dir.path(), disk.clone()));
            let wait = core.convs.get_mut(&id).unwrap().waiting_on.as_mut().unwrap();
            wait.ids.push(child);
            let deadline = wait.until;
            core.cancel_wait_timer(&id);
            core.check_wait_deadline(&id, deadline, "Simulated deadline.", true);
            assert_eq!(*disk.remaining.lock().unwrap(), 0, "the saved deadline must really fail once");
            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().until, deadline);
            assert!(core.wait_timers.contains_key(&id), "a storage failure before any send must get a retry");
            deadline
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(16)).await;
        tokio::task::yield_now().await;
        engine.with_saves(false, |core| {
            let saved = core.store.get(&id).unwrap();
            let wait = saved.waiting_on.unwrap();
            assert!(wait.until > before);
            assert!(wait.problem.is_none());
            assert!(saved.queued.is_empty(), "the child is still working");
            assert!(core.wait_timers.contains_key(&id));
        });
        engine.shutdown();
    }

    #[tokio::test(start_paused = true)]
    async fn pre_dispatch_save_retries_respect_stop_and_a_replacement_wait() {
        for (case, failures) in [
            ("return", 1),
            ("exhausted", 2),
            ("stopped", 1),
            ("archived", 1),
            ("decision", 1),
            ("replaced", 1),
            ("restore", 1),
            ("account", 1),
        ] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = super::super::answer::tests::engine(&dir);
            let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
            engine.set_time_source(Arc::new(|| 1000.0));
            engine.session_wait(&id, vec![], Some("original wait".into())).unwrap();
            let disk = Arc::new(FailWaitWrite {
                path: dir.path().join("conversations").join(format!("{id}.json.tmp")),
                remaining: Mutex::new(failures),
            });
            let original = engine.with_saves(false, |core| {
                if case != "account" {
                    core.opts.clis.insert(Vendor::Claude, Cli { command: "not-spawned".into(), prefix: vec![] });
                    core.ledger.of_mut(Vendor::Claude).billing = Billing::Subscription;
                    core.convs.get_mut(&id).unwrap().choice =
                        Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
                }
                let wait = core.convs[&id].waiting_on.clone().unwrap();
                core.store = Arc::new(Store::with_disk(dir.path(), disk.clone()));
                if case == "restore" {
                    core.cancel_wait_timer(&id);
                    core.restore_wait(&id);
                } else {
                    core.return_from_wait(&id, "Worker returned.");
                }
                assert_eq!(*disk.remaining.lock().unwrap(), failures - 1, "{case}");
                if case == "account" {
                    assert!(!core.wait_timers.contains_key(&id));
                    return wait;
                }
                assert_eq!(core.convs[&id].waiting_on.as_ref(), Some(&wait), "{case}");
                assert!(core.convs[&id].queued.is_empty(), "{case}");
                assert!(core.wait_save_retry_at(&id, &wait).is_some(), "{case}");
                match case {
                    "stopped" => core.interrupt(&id, true),
                    "archived" => core.convs.get_mut(&id).unwrap().archived_at = Some(1),
                    "decision" => {
                        core.turn(&id).unwrap().permissions = Some(vec![Permission {
                            request_id: "newly-asked".into(),
                            tool: "ExitPlanMode".into(),
                            input: serde_json::json!({}),
                            epoch: None,
                        }]);
                    }
                    _ => {}
                }
                wait
            });
            if case == "replaced" {
                engine.session_wait(&id, vec![], Some("replacement wait".into())).unwrap();
            }
            tokio::task::yield_now().await;
            tokio::time::advance(Duration::from_millis(SAVE_RETRY_MS + 1)).await;
            tokio::task::yield_now().await;
            engine.with_saves(false, |core| {
                let c = &core.convs[&id];
                if case == "return" {
                    let receipt = receipt_id(original.since, original.until);
                    assert!(c.waiting_on.is_none());
                    assert_eq!(c.queued.len(), 1);
                    assert_eq!(c.queued[0].id, receipt);
                    assert!(has_receipt(&core.store.get(&id).unwrap(), &receipt));
                    return;
                }
                assert!(c.queued.is_empty(), "{case}");
                let wait = c.waiting_on.as_ref().unwrap();
                assert!(core.wait_save_retry_at(&id, wait).is_none(), "{case}");
                match case {
                    "exhausted" => {
                        assert_eq!(*disk.remaining.lock().unwrap(), 0);
                        assert_eq!(wait.problem.as_deref(), Some("error.chat.saveFailed"));
                        assert!(wait.storage_blocked);
                        assert!(!core.wait_timers.contains_key(&id));
                        core.try_save(&id).unwrap();
                        core.try_save(&id).unwrap();
                    }
                    "stopped" => {
                        assert_eq!(wait.problem.as_deref(), Some("reason.wait.stopped"));
                        assert!(!core.wait_timers.contains_key(&id));
                    }
                    "replaced" => assert_eq!(wait.note, "replacement wait"),
                    "archived" => {
                        assert_eq!(wait, &original);
                        assert!(!core.wait_timers.contains_key(&id));
                    }
                    "decision" => {
                        assert!(has_open_permission(c));
                        assert!(wait.until > original.until);
                        assert!(core.wait_timers.contains_key(&id));
                    }
                    "restore" => {
                        assert_eq!(wait, &original);
                        assert!(core.wait_timers.contains_key(&id));
                    }
                    "account" => {
                        assert_eq!(wait.problem.as_deref(), Some("error.noSubscription"));
                        assert!(!wait.storage_blocked);
                    }
                    _ => unreachable!(),
                }
            });
            if case == "exhausted" {
                tokio::task::yield_now().await;
                engine.with(|core| {
                    let saved = core.store.get(&id).unwrap();
                    assert_eq!(saved.waiting_on.as_ref(), Some(&original), "a successful later save clears the local blocker");
                    assert!(saved.queued.is_empty());
                    assert!(core.wait_timers.contains_key(&id));
                });
            }
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn startup_recovers_a_pre_dispatch_storage_blocker_and_preserves_uncertain_delivery() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
        engine.session_wait(&id, vec![], Some("saved wait".into())).unwrap();
        engine.with_saves(false, |core| {
            for (reason, restored) in [("error.chat.saveFailed", true), ("reason.wait.delivery", false)] {
                core.cancel_wait_timer(&id);
                let c = core.convs.get_mut(&id).unwrap();
                let wait = c.waiting_on.as_mut().unwrap();
                wait.problem = Some(reason.into());
                wait.storage_blocked = true;
                core.store.save_checked(c).unwrap();
                let saved = core.store.get(&id).unwrap().waiting_on.unwrap();
                assert!(saved.storage_blocked);
                c.waiting_on = Some(saved);
                core.restore_wait(&id);
                let c = core.store.get(&id).unwrap();
                let wait = c.waiting_on.unwrap();
                assert_eq!(wait.problem.as_deref(), (!restored).then_some(reason));
                assert_eq!(wait.storage_blocked, !restored);
                assert_eq!(core.wait_timers.contains_key(&id), restored);
                assert!(c.queued.is_empty());
            }
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn local_wake_respects_an_open_decision_and_records_one_admission_across_restart() {
        struct FailClear(PathBuf);
        impl Disk for FailClear {
            fn write(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
                if path == self.0
                    && serde_json::from_slice::<Conversation>(bytes).is_ok_and(|c| c.waiting_on.is_none() && !c.queued.is_empty())
                {
                    return Err(std::io::Error::other("cannot clear the saved wait"));
                }
                RealDisk.write(path, bytes)
            }
            fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
                RealDisk.rename(from, to)
            }
        }

        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let project = engine.add_project(&dir.path().to_string_lossy()).unwrap();
        let clock = Arc::new(AtomicU64::new(1000));
        let test_clock = clock.clone();
        engine.set_time_source(Arc::new(move || test_clock.load(Ordering::Relaxed) as f64));
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[("plan", "ExitPlanMode")])]);
        let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
        engine.with_saves(false, |core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "not-spawned".into(), prefix: vec![] });
            core.ledger.of_mut(Vendor::Claude).billing = Billing::Subscription;
            let c = core.convs.get_mut(&id).unwrap();
            c.area = Area::Project;
            c.project_id = Some(project.id.clone());
            c.choice = Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
        });
        engine.session_wait(&id, vec![], Some("child result".into())).unwrap();
        let w = engine.with(|core| {
            let wait = core.convs.get_mut(&id).unwrap().waiting_on.as_mut().unwrap();
            wait.ids.push(child.clone());
            wait.clone()
        });
        assert_eq!(engine.wake_wait(&id, w.since, w.until), WaitCheck::TooSoon { until: w.until });
        clock.store(1400, Ordering::Relaxed);
        assert_eq!(engine.wake_wait(&id, w.since, w.until), WaitCheck::DecisionPending);
        engine.with(|core| core.turn(&id).unwrap().permissions = None);
        for reason in ["reason.wait.stopped", "reason.wait.archived", "reason.wait.delivery"] {
            engine.with(|core| {
                let c = core.convs.get_mut(&id).unwrap();
                c.stopped = reason == "reason.wait.stopped";
                c.archived_at = (reason == "reason.wait.archived").then_some(1);
                c.waiting_on.as_mut().unwrap().problem = (reason == "reason.wait.delivery").then(|| reason.into());
            });
            assert_eq!(engine.wake_wait(&id, w.since, w.until), WaitCheck::Blocked { reason: reason.into() });
        }
        engine.with(|core| core.convs.get_mut(&id).unwrap().waiting_on.as_mut().unwrap().problem = None);
        let WaitCheck::Deferred { until } = engine.wake_wait(&id, w.since, w.until) else { panic!("the child still owns work") };
        assert!(until > w.until);
        assert_eq!(engine.wake_wait(&id, w.since, w.until), WaitCheck::Obsolete);
        engine.with(|core| {
            core.turn(&child).unwrap().status = Some(Status::Done);
            core.live.get_mut(&child).unwrap().turn = None;
            // Admission must withstand a failure to save the queue and clear the wait together.
            let temp = dir.path().join("conversations").join(format!("{id}.json.tmp"));
            core.store = Arc::new(Store::with_disk(dir.path(), Arc::new(FailClear(temp))));
        });
        clock.store(1800, Ordering::Relaxed);
        assert_eq!(engine.wake_wait(&id, w.since, until), WaitCheck::Blocked { reason: "error.chat.saveFailed".into() });
        engine.with(|core| {
            assert!(core.convs[&id].queued.is_empty());
            assert!(!core.wait_timers.contains_key(&id), "the queue save is past the pre-dispatch retry boundary");
            let saved = core.store.get(&id).unwrap();
            assert!(saved.queued.is_empty());
            assert!(saved.waiting_on.unwrap().problem.is_some());
            core.store = Arc::new(Store::new(dir.path()));
        });
        engine.session_wait(&id, vec![], Some("a new wait after the save failure".into())).unwrap();
        let w = engine.with(|core| core.convs[&id].waiting_on.clone().unwrap());
        let until = w.until;
        clock.store(2200, Ordering::Relaxed);
        assert_eq!(engine.wake_wait(&id, w.since, until), WaitCheck::Admitted);
        assert_eq!(engine.wake_wait(&id, w.since, until), WaitCheck::AlreadyHandled);
        engine.with_saves(false, |core| {
            // The active reply asked for a decision after the wait was accepted into the queue.
            let reply = core.turn(&id).unwrap();
            reply.status = Some(Status::Done);
            reply.permissions = Some(vec![crate::model::Permission {
                request_id: "late-plan".into(),
                tool: "ExitPlanMode".into(),
                input: serde_json::json!({}),
                epoch: None,
            }]);
            core.live.get_mut(&id).unwrap().turn = None;
            core.drain(&id);
            let lead = &core.convs[&id];
            assert_eq!(lead.messages.last().unwrap().id, "answer", "the queued return leaves the later plan decision in place");
            assert_eq!(lead.queued.len(), 1);
            let c = core.store.get(&id).unwrap();
            assert_eq!(c.queued.len(), 1);
            assert_eq!(c.queued[0].id, receipt_id(w.since, until));
            assert_eq!(c.queued[0].wake.as_ref().unwrap().why, "wake.wait");
            assert!(c.queued[0].text.contains("A local control client requested"));
            assert!(c.waiting_on.is_none());

            // The approval is the person's explicit send, even when the old turn is still closing.
            core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().permissions = None;
            core.live.get_mut(&id).unwrap().turn = Some("answer".into());
            core.send_by_itself(&id, "Go ahead with the plan.".into(), Wake::of("wake.approvedPlan")).unwrap();
            let queued = &core.convs[&id].queued;
            assert_eq!(queued[0].wake.as_ref().unwrap().why, "wake.approvedPlan");
            assert_eq!(queued[1].id, receipt_id(w.since, until));
        });
        engine.shutdown();
        drop(engine);
        let (engine, _) = super::super::answer::tests::engine(&dir);
        assert_eq!(engine.wake_wait(&id, w.since, until), WaitCheck::AlreadyHandled);
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_person_stop_immediately_saves_blocker_and_aborts_timer() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, events) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("helper result".into())).unwrap();
        engine.with_saves(false, |core| {
            assert!(core.wait_timers.contains_key(&id));
            core.turn(&id).unwrap().status = Some(Status::Done);
            core.live.get_mut(&id).unwrap().turn = None;
            events.lock().unwrap().clear();
            core.interrupt(&id, true);
            assert_eq!(core.store.get(&id).unwrap().waiting_on.unwrap().problem.as_deref(), Some("reason.wait.stopped"));
            assert!(!core.wait_timers.contains_key(&id));
            assert!(core.running_item(&core.convs[&id]).is_none(), "a stopped wait is not a running conversation");
            assert!(events.lock().unwrap().iter().any(|e| matches!(e, AppEvent::List)), "the list drops the stale deadline at once");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_account_confirmation_after_deadline_resumes_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("result".into())).unwrap();
        engine.with_saves(false, |core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "not-spawned".into(), prefix: vec![] });
            core.convs.get_mut(&id).unwrap().choice = Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
            core.return_from_wait(&id, "expired while account unknown");
            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().problem.as_deref(), Some("error.noSubscription"));
            let deadline = core.convs[&id].waiting_on.as_ref().unwrap().until;
            let reset = core.secs() + 3600.0;
            core.note_quota(Vendor::Claude, "five_hour", 0.95, reset);
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            let held = core.store.get(&id).unwrap().waiting_on.unwrap();
            assert_eq!(held.problem.as_deref(), Some("reason.budget.other"));
            assert_eq!(held.until, deadline);
            assert!(core.convs[&id].queued.is_empty());
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::UsageBilled), ..Default::default() });
            let held = core.store.get(&id).unwrap().waiting_on.unwrap();
            assert_eq!(held.problem.as_deref(), Some("error.noSubscription"));
            assert_eq!(held.until, deadline);
            assert!(core.convs[&id].queued.is_empty());
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            core.note_quota(Vendor::Claude, "five_hour", 0.10, reset);
            assert!(core.convs[&id].waiting_on.is_none());
            assert_eq!(core.convs[&id].queued.len(), 1);
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            assert_eq!(core.convs[&id].queued.len(), 1);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn saved_wait_reads_keep_checks_working_until_they_finish() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("Waiting for another result".into())).unwrap();
        for check in [ChecksState::Running, ChecksState::Passed] {
            engine.with(|core| {
                let reply = core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap();
                reply.status = Some(Status::Done);
                reply.checks = Some(Checks { state: check, passed: 0, failed: vec![] });
                core.refresh_running();
            });
            let (_, active) = engine.conversation_list();
            assert_eq!(
                active.iter().find(|(conversation, _)| conversation == &id).map(|(_, state)| *state),
                (check == ChecksState::Running).then_some(RunState::Working)
            );
            assert!(engine.state().running.items.iter().any(|item| item.conversation_id == id));
        }
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_new_account_reading_keeps_a_pending_plan_and_the_saved_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[("plan", "ExitPlanMode")])]);
        engine.session_wait(&id, vec![], Some("result".into())).unwrap();
        engine.with_saves(false, |core| {
            core.opts.clis.insert(Vendor::Codex, Cli { command: "not-spawned".into(), prefix: vec![] });
            let c = core.convs.get_mut(&id).unwrap();
            c.choice = Some(Choice { vendor: "codex".into(), model: None, effort: None });
            let reply = c.messages.last_mut().unwrap();
            reply.vendor = Some(Vendor::Codex);
            reply.status = Some(Status::Done);
            let wait = c.waiting_on.as_mut().unwrap();
            let previous = wait.until;
            wait.problem = Some("reason.budget.other".into());
            core.live.get_mut(&id).unwrap().turn = None;

            core.apply_signals(Vendor::Codex, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            let c = &core.convs[&id];
            assert_eq!(c.messages.last().unwrap().id, "answer");
            assert!(c.queued.is_empty() && c.messages.last().unwrap().permissions.is_some());
            let wait = c.waiting_on.as_ref().unwrap();
            assert!(wait.problem.is_none() && wait.until > previous);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_account_event_after_restart_preserves_stop_and_uncertain_delivery() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("result".into())).unwrap();
        engine.with_saves(false, |core| core.return_from_wait(&id, "deadline"));
        engine.shutdown();
        drop(engine);
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "not-spawned".into(), prefix: vec![] });
            core.convs.get_mut(&id).unwrap().choice = Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
            core.convs.get_mut(&id).unwrap().stopped = true;
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            assert!(core.convs[&id].queued.is_empty());
            assert!(core.convs[&id].waiting_on.is_some());
            core.convs.get_mut(&id).unwrap().stopped = false;
            core.convs.get_mut(&id).unwrap().waiting_on.as_mut().unwrap().problem = Some("reason.wait.delivery".into());
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            core.return_from_wait(&id, "late external event");
            assert!(core.convs[&id].queued.is_empty(), "uncertain paid delivery must never retry");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_success_is_admitted_once_behind_the_current_turn() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("synthetic result".into())).unwrap();
        engine.with_saves(false, |core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "not-spawned".into(), prefix: vec![] });
            core.ledger.of_mut(Vendor::Claude).billing = Billing::Subscription;
            core.convs.get_mut(&id).unwrap().choice = Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
            core.return_from_wait(&id, "deadline");
            assert!(core.convs[&id].waiting_on.is_none(), "{:?}", core.convs[&id].waiting_on);
            assert_eq!(core.convs[&id].queued.len(), 1);
            assert_eq!(core.convs[&id].queued[0].wake.as_ref().unwrap().why, "wake.wait");
            core.return_from_wait(&id, "duplicate deadline");
            assert_eq!(core.store.get(&id).unwrap().queued.len(), 1);
        });
        engine.shutdown();
    }

    #[tokio::test(start_paused = true)]
    async fn durable_wait_checks_live_children_without_buying_a_lead_turn() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
        let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
        engine.set_keep_going_pace(0.001);
        engine.session_wait(&id, vec![], Some("child result".into())).unwrap();
        let before = engine.with(|core| {
            let wait = core.convs.get_mut(&id).unwrap().waiting_on.as_mut().unwrap();
            wait.ids = vec![child.clone()];
            wait.until
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        engine.with_saves(false, |core| {
            let wait = core.convs[&id].waiting_on.as_ref().unwrap();
            assert!(wait.problem.is_none(), "live child checks do not request paid admission");
            assert!(wait.until > before);
            assert!(core.convs[&id].queued.is_empty());
            assert!(core.wait_timers.contains_key(&id));
            core.turn(&child).unwrap().status = Some(Status::Done);
            core.live.get_mut(&child).unwrap().turn = None;
        });
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        engine.with_saves(false, |core| {
            assert!(
                core.convs[&id].waiting_on.as_ref().unwrap().problem.is_some(),
                "an ended child requests a real check instead of rearming forever"
            );
            assert!(!core.wait_timers.contains_key(&id));
        });
        engine.shutdown();
    }

    #[tokio::test(start_paused = true)]
    async fn durable_wait_deadline_leaves_an_open_decision_in_place() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[("plan", "ExitPlanMode")])]);
        engine.set_keep_going_pace(0.001);
        engine.session_wait(&id, vec![], Some("check".into())).unwrap();
        let until = engine.with(|core| core.convs[&id].waiting_on.as_ref().unwrap().until);
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        engine.with_saves(false, |core| {
            let c = &core.convs[&id];
            let wait = c.waiting_on.as_ref().unwrap();
            assert!(wait.until > until && wait.problem.is_none());
            assert!(c.queued.is_empty() && c.messages.last().unwrap().permissions.is_some());
        });
        engine.shutdown();
    }

    #[tokio::test(start_paused = true)]
    async fn durable_wait_timer_is_finite_and_does_not_rearm_a_blocked_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        // A held-back request was already refused. Reading the notice is not a pending permission decision.
        engine.with(|core| core.turn(&id).unwrap().parked_more = Some(1));
        engine.set_keep_going_pace(0.001);
        engine.session_wait(&id, vec![], Some("synthetic command".into())).unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        engine.with_saves(false, |core| {
            assert!(!core.wait_timers.contains_key(&id));
            assert!(core.convs[&id].waiting_on.as_ref().unwrap().problem.is_some());
            core.restore_wait(&id);
            assert!(!core.wait_timers.contains_key(&id));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_receipt_prevents_a_second_send_after_restart() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("synthetic result".into())).unwrap();
        engine.with_saves(false, |core| {
            let w = core.convs[&id].waiting_on.as_ref().unwrap();
            let receipt = format!("wait-{}-{}", w.since, w.until);
            core.convs.get_mut(&id).unwrap().messages.push(Message::new(receipt, Role::User, "Already admitted".into(), 1));
            core.save(&id);
            core.return_from_wait(&id, "duplicate event");
            assert!(core.store.get(&id).unwrap().waiting_on.is_none());
            assert!(core.convs[&id].queued.is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_restores_note_only_and_records_blocked_return_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("background command".into())).unwrap();
        let saved = engine.open(&id).unwrap().waiting_on.unwrap();
        assert!(saved.until > saved.since);
        engine.shutdown();
        drop(engine);
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().until, saved.until);
            assert!(core.wait_timers.contains_key(&id));
            core.return_from_wait(&id, "deadline");
            let w = core.store.get(&id).unwrap().waiting_on.unwrap();
            assert_eq!(w.problem.as_deref(), Some("error.noSubscription"), "no connected AI must leave a saved account blocker");
            assert!(core.convs[&id].queued.is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn durable_wait_stop_archive_shutdown_do_not_send() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.session_wait(&id, vec![], Some("check".into())).unwrap();
        engine.with_saves(false, |core| {
            core.convs.get_mut(&id).unwrap().stopped = true;
            core.return_from_wait(&id, "deadline");
            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().problem.as_deref(), Some("reason.wait.stopped"));
            core.convs.get_mut(&id).unwrap().stopped = false;
            core.convs.get_mut(&id).unwrap().archived_at = Some(1);
            core.return_from_wait(&id, "event");
            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().problem.as_deref(), Some("reason.wait.archived"));
            core.closing = true;
            assert_eq!(core.wake_wait(&id, 10, 20), WaitCheck::Closing);
            core.return_from_wait(&id, "shutdown");
            assert!(core.convs[&id].queued.is_empty());
        });
        engine.shutdown();
    }
}
