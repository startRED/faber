//! One finite return per declared wait. Only timers and real external events may request it.
use super::*;

impl Core {
    pub(super) fn restore_wait(&mut self, id: &str) {
        if self.load(id).and_then(|c| c.waiting_on.as_ref()).is_none() {
            return;
        }
        // Also writes the finite fallback for pre-deadline records, without moving their clock.
        if self.try_save(id).is_ok() {
            self.arm_wait(id);
            self.restore_wait_pr(id);
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
                    && c.waiting_on.as_ref().is_some_and(|w| w.problem.as_deref() == Some("reason.budget.other"))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ready {
            if self.goal_out_of_money(&id) || !self.may_spend(&id) {
                continue;
            }
            self.return_from_wait(&id, "New account or quota evidence now permits the saved state check.");
        }
    }

    pub(super) fn arm_wait(&mut self, id: &str) {
        let Some(w) = self.convs.get(id).and_then(|c| c.waiting_on.as_ref()).filter(|w| w.problem.is_none()) else { return };
        let until = w.until;
        if self.wait_timers.get(id).is_some_and(|(at, _)| *at == until) || self.closing {
            return;
        }
        if let Some((_, timer)) = self.wait_timers.remove(id) {
            timer.abort();
        }
        let delay = Duration::from_millis(until.saturating_sub((self.secs() * 1000.0) as u64));
        let (me, runtime, conv) = (self.me.clone(), self.opts.runtime.clone(), id.to_string());
        let task = self.opts.runtime.spawn(async move {
            tokio::time::sleep(delay).await;
            let Some(core) = me.upgrade() else { return };
            Engine { core, runtime }.with(|c| {
                if c.wait_timers.get(&conv).is_some_and(|(at, _)| *at == until) {
                    c.wait_timers.remove(&conv);
                    if c.convs.get(&conv).and_then(|c| c.waiting_on.as_ref()).is_some_and(|w| w.until == until && w.problem.is_none()) {
                        let children = &c.convs[&conv].waiting_on.as_ref().unwrap().ids;
                        let live_child = !children.is_empty() && children.iter().all(|id| c.convs.get(id).is_some_and(|child| {
                            let asks_person = child.messages.iter().rev().find(|m| m.role == Role::Assistant)
                                .is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()));
                            !asks_person && child.waiting_on.as_ref().is_none_or(|w| w.problem.is_none())
                                && siblings::reports_later(c.live.get(id).is_some_and(|l| l.turn.is_some()), child)
                        }));
                        let live_helper = children.is_empty() && !c.working_helpers(&conv).is_empty();
                        if live_child || live_helper {
                            let interval = (300_000.0 * c.keep_pace).max(1.0) as u64;
                            c.convs.get_mut(&conv).unwrap().waiting_on.as_mut().unwrap().until =
                                until.max((c.secs() * 1000.0) as u64).saturating_add(interval);
                            if c.try_save(&conv).is_ok() {
                                c.arm_wait(&conv);
                            } else {
                                c.convs.get_mut(&conv).unwrap().waiting_on.as_mut().unwrap().problem = Some("reason.wait.delivery".into());
                            }
                            c.list_changed();
                            return;
                        }
                        c.return_from_wait(&conv, "The saved wait deadline has arrived. Check the actual state of the work you were waiting for, including any background command or helper interrupted by restart. Continue if possible; otherwise state the concrete blocker and who must act.");
                    }
                }
            });
        });
        self.wait_timers.insert(id.to_string(), (until, task.abort_handle()));
    }

    /// Failed admission remains visible and durable, rather than retrying a paid turn on a loop.
    pub(super) fn return_from_wait(&mut self, id: &str, cause: &str) {
        if self.closing {
            return;
        }
        let Some(wait) = self.load(id).and_then(|c| c.waiting_on.clone()) else { return };
        let receipt = format!("wait-{}-{}", wait.since, wait.until);
        let c = &self.convs[id];
        let delivered = c.queued.iter().any(|q| q.id == receipt) || c.messages.iter().any(|m| m.id == receipt);
        if delivered {
            self.convs.get_mut(id).unwrap().waiting_on = None;
            self.save(id);
            self.list_changed();
            return;
        }
        if wait
            .problem
            .as_deref()
            .is_some_and(|p| !matches!(p, "reason.budget.other" | "reason.wait.stopped" | "reason.wait.archived" | goals::CAP))
        {
            return;
        }
        // Reserve before dispatch: an uncertain crash leaves a visible blocker, never a second paid attempt.
        let blocked = if c.stopped {
            Some("reason.wait.stopped")
        } else if c.archived_at.is_some() {
            Some("reason.wait.archived")
        } else if self.goal_out_of_money(id) {
            Some(goals::CAP)
        } else if !self.may_spend(id) {
            Some("reason.budget.other")
        } else {
            None
        };
        let mut reserved = wait.clone();
        reserved.problem = Some(blocked.unwrap_or("reason.wait.delivery").into());
        self.convs.get_mut(id).unwrap().waiting_on = Some(reserved.clone());
        if self.try_save(id).is_err() {
            reserved.problem = Some("reason.wait.delivery".into());
            self.convs.get_mut(id).unwrap().waiting_on = Some(reserved);
            self.list_changed();
            return;
        }
        if blocked.is_none() {
            let text = format!(
                "[Faber] {cause}\nYour saved wait: {}\nReply in the conversation's language. Do not repeat a wait without new evidence.",
                wait.note
            );
            match self.send_by_itself_with_id(id, text, Wake::of("wake.wait"), receipt) {
                Ok(()) => {
                    self.convs.get_mut(id).unwrap().waiting_on = None;
                    if self.try_save(id).is_err() {
                        self.convs.get_mut(id).unwrap().waiting_on = Some(reserved);
                    }
                }
                Err(why) => {
                    reserved.problem = Some(why);
                    self.convs.get_mut(id).unwrap().waiting_on = Some(reserved);
                    self.save(id);
                }
            }
        }
        self.list_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Session;

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
            assert!(super::super::running_item(&core.convs[&id], None).is_none(), "a stopped wait is not a running conversation");
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
            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().problem.as_deref(), Some("reason.budget.other"));
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            assert!(core.convs[&id].waiting_on.is_none());
            assert_eq!(core.convs[&id].queued.len(), 1);
            core.apply_signals(Vendor::Claude, Signals { billing: Some(Billing::Subscription), ..Default::default() });
            assert_eq!(core.convs[&id].queued.len(), 1);
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
    async fn durable_wait_timer_is_finite_and_does_not_rearm_a_blocked_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("answer", &[])]);
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
            assert!(w.problem.is_some(), "no connected AI must leave a visible blocker");
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
            core.return_from_wait(&id, "shutdown");
            assert!(core.convs[&id].queued.is_empty());
        });
        engine.shutdown();
    }
}
