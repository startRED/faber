//! A short hold before a routine report wakes an idle lead (D2 of the harness study). Conversations an agent started often end
//! seconds apart; each report used to start a lead turn of its own that re-read the whole conversation to say "noted, waiting
//! for the next one". A report of a conversation that concluded its work, while others the lead started still work, now waits
//! `SETTLE` seconds, pushed back by each report that joins it and never past `MOST` after the first; then all go out as one
//! turn (`merge_reports`). Trouble never waits: a report that failed, stopped, was cut off, never concluded or needs an answer
//! goes out at once and takes the held ones with it, and so does the last report of a team, since nothing would join it.
//! A lead at work is not held for: what arrives during its turn already joins that turn (`hand_over_queue`).
//! Survives a restart: a held conversation is saved as owing its report (`pending_parent_report`), and the next start sends
//! what is owed together (`refill_parent_reports`), written from each conversation's latest turn. The timer is not saved;
//! nothing is lost if Faber closes inside it. A held report keeps the text its turn wrote, so a turn the person starts in that
//! conversation before the hold goes out never replaces it.
//! Cost: one map entry and one sleeping task per lead with held reports, gone when they go out. No process, no polling.
use super::*;
use crate::conversation_state::state_of;
use crate::sibling::State;

/// Seconds a routine report waits for the next one.
pub(super) const SETTLE: f64 = 5.0;
/// Seconds after the first held report by which all go out, however many keep coming.
pub(super) const MOST: f64 = 30.0;

/// The reports held for one lead.
pub(super) struct Hold {
    /// In the order they arrived, which is the order the lead reads them: the child, its report as written when its turn
    /// ended (a later turn the person starts in it does not rewrite it), and the report's id.
    children: Vec<(String, String, String)>,
    first: f64,
    until: f64,
}

impl Core {
    /// Whether the report of `child` waits instead of going out now. A held report goes out with the rest (`flush_hold`).
    pub(super) fn hold_report(&mut self, child: &str, parent: &str, state: State, text: &str, id: &str) -> bool {
        if !self.batch_reports || self.collecting.contains(parent) {
            return false;
        }
        let now = self.secs();
        let urgent = state != State::Concluded;
        let others = self.team_at_work(parent, child);
        let idle = self.lead_idle(parent);
        if let Some(h) = self.holds.get_mut(parent) {
            if !h.children.iter().any(|(_, _, held)| held == id) {
                h.children.push((child.to_string(), text.to_string(), id.to_string()));
                h.until = (now + SETTLE).min(h.first + MOST);
            }
            self.owe_report(child);
            if urgent || !others || !idle {
                self.flush_hold(parent);
            }
            return true;
        }
        if urgent || !others || !idle {
            return false;
        }
        self.holds.insert(
            parent.to_string(),
            Hold { children: vec![(child.to_string(), text.to_string(), id.to_string())], first: now, until: now + SETTLE },
        );
        self.owe_report(child);
        self.arm_hold(parent, now);
        true
    }

    /// Sends every report held for `parent` as one turn, or into the turn that runs.
    pub(super) fn flush_hold(&mut self, parent: &str) {
        let Some(h) = self.holds.remove(parent) else { return };
        let mine = self.collecting.insert(parent.to_string());
        // One at a time, so they queue in the order they arrived.
        for (child, text, id) in h.children {
            if self.convs.contains_key(&child) && self.load(parent).is_some_and(|p| p.archived_at.is_none()) {
                self.send_child_report(&child, parent.to_string(), text, id);
            }
        }
        if mine {
            self.collecting.remove(parent);
        }
        self.hand_over_queue(parent, true);
        self.drain(parent);
    }

    /// Sends the holds whose time came; the timer calls it, and tests after moving the clock.
    pub(super) fn check_holds(&mut self) {
        let now = self.secs();
        let due: Vec<String> = self.holds.iter().filter(|(_, h)| h.until <= now).map(|(p, _)| p.clone()).collect();
        for parent in due {
            self.flush_hold(&parent);
        }
    }

    /// One sleeping task for the hold that began at `first`; it ends once that hold went out.
    fn arm_hold(&mut self, parent: &str, first: f64) {
        let me = self.me.clone();
        let parent = parent.to_string();
        self.opts.runtime.spawn(async move {
            loop {
                let wait = {
                    let Some(core) = me.upgrade() else { return };
                    let core = core.lock().unwrap_or_else(|e| e.into_inner());
                    match core.holds.get(&parent).filter(|h| h.first == first) {
                        Some(h) => h.until - core.secs(),
                        None => return,
                    }
                };
                tokio::time::sleep(Duration::from_secs_f64(wait.clamp(0.05, MOST))).await;
                let Some(core) = me.upgrade() else { return };
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                match core.holds.get(&parent) {
                    Some(h) if h.first == first && h.until <= core.secs() => {
                        core.flush_hold(&parent);
                        return;
                    }
                    Some(h) if h.first == first => {}
                    _ => return,
                }
            }
        });
    }

    /// Saved as owing its report, so a restart inside the hold still sends it.
    fn owe_report(&mut self, child: &str) {
        if let Some(c) = self.convs.get_mut(child).filter(|c| !c.pending_parent_report) {
            c.pending_parent_report = true;
            self.save(child);
        }
    }

    /// Whether another conversation `parent` started is still at work, so its report may soon join this one.
    fn team_at_work(&self, parent: &str, except: &str) -> bool {
        self.convs.iter().any(|(id, c)| {
            id != except
                && c.started_by.as_deref() == Some(parent)
                && c.archived_at.is_none()
                && matches!(state_of(self.live.get(id).is_some_and(|l| l.turn.is_some()), c), State::Working | State::Stuck)
        })
    }

    /// The lead runs no turn and nothing else waits to reach it: a report would start a turn of its own now.
    fn lead_idle(&self, parent: &str) -> bool {
        !self.live.get(parent).is_some_and(|l| l.turn.is_some())
            && !self.compacts.contains_key(parent)
            && !self.compactions.contains_key(parent)
            && self.convs.get(parent).is_some_and(|c| {
                c.queued.is_empty()
                    && !c.stopped
                    && !c.compacting
                    && c.archived_at.is_none()
                    && !c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
            })
    }
}

impl Engine {
    /// Reports wait together and go out together (on by default). Off only to measure the difference (`fanout_eval`).
    pub fn set_batch_reports(&self, on: bool) {
        self.with(|c| c.batch_reports = on)
    }

    /// Sends the held reports whose time came, as the timer does; tests call it after moving the clock.
    pub fn check_holds(&self) {
        self.with(|c| c.check_holds())
    }
}

#[cfg(test)]
mod tests {
    use super::super::answer::tests::{engine, seed};
    use super::*;
    use crate::budget::Billing;
    use crate::cli::Cli;
    use crate::events::Session;
    use crate::model::Wake;
    use crate::router::Choice;

    /// A lead at rest that may start turns, and `n` conversations it started, all at work; the clock the test moves.
    fn team(dir: &crate::workspace::TestRoot, n: usize) -> (Engine, String, Vec<String>, Arc<Mutex<f64>>) {
        let (engine, _) = engine(dir);
        let parent = seed(&engine, Session::from_fn(|_| {}), &[("lead", &[])]);
        let kids: Vec<String> = (0..n).map(|_| seed(&engine, Session::from_fn(|_| {}), &[("work", &[])])).collect();
        let clock = Arc::new(Mutex::new(1000.0));
        let read = clock.clone();
        engine.set_time_source(Arc::new(move || *read.lock().unwrap()));
        engine.with(|core| {
            admit(core);
            core.live.remove(&parent);
            core.convs.get_mut(&parent).unwrap().messages.last_mut().unwrap().status = Some(Status::Done);
            core.save(&parent);
            for (i, kid) in kids.iter().enumerate() {
                let c = core.convs.get_mut(kid).unwrap();
                c.started_by = Some(parent.clone());
                c.title = format!("Kid {i}");
                core.save(kid);
            }
        });
        (engine, parent, kids, clock)
    }

    /// An AI may answer, so a report the lead is sent starts its turn.
    fn admit(core: &mut Core) {
        core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
        core.ledger.claude.billing = Billing::Subscription;
    }

    /// The kid's turn ends at `at`, concluded or failed, and its report goes to the lead.
    fn end(engine: &Engine, clock: &Mutex<f64>, kid: &str, at: f64, ok: bool) {
        *clock.lock().unwrap() = at;
        engine.with(|core| {
            core.live.remove(kid);
            let reply = core.convs.get_mut(kid).unwrap().messages.last_mut().unwrap();
            reply.status = Some(if ok { Status::Done } else { Status::Error });
            reply.text = format!("{kid} finished its part.");
            core.tell_parent(kid);
        });
    }

    fn at(engine: &Engine, clock: &Mutex<f64>, secs: f64) {
        *clock.lock().unwrap() = secs;
        engine.check_holds();
    }

    /// The turns Faber sent the lead by itself, each one a wake.
    fn wakes(engine: &Engine, parent: &str) -> Vec<Message> {
        engine.with(|core| core.convs[parent].messages.iter().filter(|m| m.wake.is_some()).cloned().collect())
    }

    #[tokio::test]
    async fn reports_seconds_apart_wake_an_idle_lead_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kids, clock) = team(&dir, 3);
        end(&engine, &clock, &kids[0], 1000.0, true);
        assert!(wakes(&engine, &parent).is_empty(), "a routine report waits while others still work");
        assert!(engine.with(|c| c.store.get(&kids[0]).unwrap().pending_parent_report), "saved as owed, so a restart sends it");
        end(&engine, &clock, &kids[1], 1002.0, true);
        at(&engine, &clock, 1006.0);
        assert!(wakes(&engine, &parent).is_empty(), "the second report pushed the hold back to 1007");
        at(&engine, &clock, 1007.0);
        let woke = wakes(&engine, &parent);
        assert_eq!(woke.len(), 1, "two reports, one lead turn");
        let wake = woke[0].wake.as_ref().unwrap();
        assert_eq!(wake.why, "wake.childDone");
        assert_eq!(wake.ids, vec![format!("report-{}-work", kids[1])]);
        assert_eq!(woke[0].id, format!("report-{}-work", kids[0]));
        for kid in &kids[..2] {
            assert!(woke[0].text.contains(&format!("{kid} finished its part.")), "{}", woke[0].text);
            assert!(!engine.with(|c| c.store.get(kid).unwrap().pending_parent_report), "delivered, no longer owed");
        }
        assert!(woke[0].text.starts_with("Several conversations you started stopped working."));
        assert!(engine.with(|c| c.holds.is_empty()));
        // A repeated end of the same turn finds its report in the merged one and sends nothing.
        engine.with(|core| core.tell_parent(&kids[1]));
        assert_eq!(wakes(&engine, &parent).len(), 1);
        assert!(engine.with(|c| c.convs[&parent].queued.is_empty()));
        engine.shutdown();
    }

    #[tokio::test]
    async fn trouble_wakes_the_lead_at_once_and_brings_the_held_reports() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kids, clock) = team(&dir, 3);
        end(&engine, &clock, &kids[0], 1000.0, true);
        end(&engine, &clock, &kids[1], 1001.0, false);
        let woke = wakes(&engine, &parent);
        assert_eq!(woke.len(), 1, "a failed report does not wait, and the held one comes with it");
        assert_eq!(woke[0].wake.as_ref().unwrap().ids.len(), 1);
        assert!(woke[0].text.contains("stopped with an error"), "{}", woke[0].text);
        assert!(engine.with(|c| c.holds.is_empty()));
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_held_report_goes_out_as_written_when_its_turn_ended() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kids, clock) = team(&dir, 3);
        end(&engine, &clock, &kids[0], 1000.0, true);
        // The person starts another turn in the held conversation before the hold goes out.
        engine.with(|core| core.convs.get_mut(&kids[0]).unwrap().messages.last_mut().unwrap().text = "a later turn".into());
        end(&engine, &clock, &kids[1], 1001.0, false);
        let woke = wakes(&engine, &parent);
        assert_eq!(woke.len(), 1);
        assert!(woke[0].text.contains(&format!("{} finished its part.", kids[0])), "{}", woke[0].text);
        assert!(!woke[0].text.contains("a later turn"), "the held report is the one its turn wrote");
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_lone_report_and_the_last_of_a_team_go_out_at_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kids, clock) = team(&dir, 1);
        end(&engine, &clock, &kids[0], 1000.0, true);
        assert_eq!(wakes(&engine, &parent).len(), 1, "nothing else works, so nothing would join it");
        engine.shutdown();
    }

    #[tokio::test]
    async fn the_hold_never_passes_thirty_seconds() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kids, clock) = team(&dir, 10);
        for (i, kid) in kids.iter().take(8).enumerate() {
            end(&engine, &clock, kid, 1000.0 + 4.0 * i as f64, true);
        }
        at(&engine, &clock, 1029.9);
        assert!(wakes(&engine, &parent).is_empty(), "each report pushed it back by five seconds");
        at(&engine, &clock, 1030.0);
        let woke = wakes(&engine, &parent);
        assert_eq!(woke.len(), 1, "thirty seconds after the first, all eight go out together");
        assert_eq!(woke[0].wake.as_ref().unwrap().ids.len(), 7);
        engine.shutdown();
    }

    #[tokio::test]
    async fn without_batching_each_report_is_its_own_wake() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kids, clock) = team(&dir, 3);
        engine.set_batch_reports(false);
        end(&engine, &clock, &kids[0], 1000.0, true);
        assert_eq!(wakes(&engine, &parent).len(), 1, "today's behaviour, kept to measure against");
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_restart_inside_the_hold_sends_each_report_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, kids, clock) = team(&dir, 3);
        end(&engine, &clock, &kids[0], 1000.0, true);
        end(&engine, &clock, &kids[1], 1001.0, true);
        engine.with(|core| {
            core.save(&kids[0]);
            core.save(&kids[1]);
        });
        engine.shutdown();
        drop(engine);
        // Faber opens again with no AI to answer yet: what was owed waits in the lead's queue, as one turn.
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|core| {
            let saved = core.store.get(&parent).unwrap();
            let reports: Vec<_> = saved.queued.iter().filter(|q| q.wake.as_ref().is_some_and(|w| w.why == "wake.childDone")).collect();
            assert_eq!(reports.len(), 1, "the owed reports wait as one: {:?}", saved.queued);
            for kid in &kids[..2] {
                assert!(reports[0].text.contains(&format!("{kid} finished its part.")), "{}", reports[0].text);
                let report = format!("report-{kid}-work");
                assert!(Wake::carries(&reports[0].id, reports[0].wake.as_ref(), &report));
            }
            for kid in &kids[..2] {
                core.tell_parent(kid);
            }
            assert_eq!(core.store.get(&parent).unwrap().queued.len(), saved.queued.len(), "never sent twice");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn reports_waiting_behind_a_turn_go_out_as_one_and_the_person_keeps_their_place() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, parent, _, _) = team(&dir, 0);
        engine.with(|core| {
            let item = |id: &str, wake: Option<Wake>| QueuedTurn {
                id: id.into(),
                text: format!("text of {id}"),
                images: None,
                attachments: None,
                choice: Choice { vendor: "auto".into(), model: None, effort: None },
                at: 1,
                sending: false,
                wake,
            };
            let report = || Some(Wake::of("wake.childDone"));
            let c = core.convs.get_mut(&parent).unwrap();
            c.queued = vec![item("r1", report()), item("r2", report()), item("r3", report()), item("mine", None), item("r4", report())];
            core.drain(&parent);
            let c = &core.convs[&parent];
            let sent = c.messages.iter().find(|m| m.id == "r1").expect("the first report went out");
            assert_eq!(sent.wake.as_ref().unwrap().ids, vec!["r2".to_string(), "r3".to_string()]);
            for id in ["r1", "r2", "r3"] {
                assert!(sent.text.contains(&format!("text of {id}")));
            }
            let left: Vec<&str> = c.queued.iter().map(|q| q.id.as_str()).collect();
            assert_eq!(left, ["mine", "r4"], "the person's message keeps its turn, and what follows it waits");
        });
        engine.shutdown();
    }
}
