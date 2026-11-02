//! A reply Faber was closed under: loading the conversation marks it (`Message::closed`), the screen says so under it, and
//! "Continue" sends one turn Faber writes by itself (`wake.closed`), never the person's message again.
use super::*;

/// At most this many continues per conversation in `CONTINUES_SECS`, so a Faber that keeps closing cannot loop the agent.
pub(super) const CONTINUES: usize = 3;
pub(super) const CONTINUES_SECS: u64 = 5 * 60;

pub(super) const CONTINUE: &str = "Faber was closed while your last answer was running, which cut it short. Before you go on, look at the \
files and the work as they are now: some steps may have finished and others not. Then continue from where you stopped; do not \
start over.";

/// The wait at start before a cut-off team leader goes on, so the accounts and quotas are read first.
const LEADER_WAIT: f64 = 20.0;

impl Engine {
    /// Continues `id` after its last reply, `turn_id`, was cut by Faber closing. Refused when that reply is no longer the last
    /// one or was not cut that way (it was continued already, or the person wrote since), while the turn would only wait in
    /// the queue, and past the cap.
    pub fn continue_closed(&self, id: &str, turn_id: &str) -> Result<(), String> {
        self.with(|c| c.continue_closed_now(id, turn_id))
    }

    /// At start, once the accounts are read: a team leader the close cut off goes on by itself with the same turn Continue
    /// sends, when nothing else will wake it (every member finished, or it had none running). A member that still owes a
    /// report wakes it with that report instead, and a member on Keep going reports when it ends. A conversation that started
    /// nobody keeps its Continue button for the person.
    pub fn resume_cut_leaders(&self) {
        let (engine, wait) = (self.clone(), self.with(|c| LEADER_WAIT * c.keep_pace));
        self.with(|c| c.opts.runtime.clone()).spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            engine.with(|c| {
                for (leader, members) in std::mem::take(&mut c.quiet_teams) {
                    c.continue_cut_leader(&leader, &members);
                }
            });
        });
    }
}

impl Core {
    fn continue_closed_now(&mut self, id: &str, turn_id: &str) -> Result<(), String> {
        let conv = self.load(id).ok_or("error.conversationGone")?;
        // Only a turn that goes out now: one that would wait in the queue leaves this reply last, and a second click would
        // queue another.
        let waits = conv.stopped || !conv.queued.is_empty() || verifying::checking(conv);
        if !conv.messages.last().is_some_and(|m| m.id == turn_id && m.closed)
            || waits
            || self.live.get(id).is_some_and(|l| l.turn.is_some())
        {
            return Err("error.closed.stale".into());
        }
        let at = now();
        let times = self.closed_continues.entry(id.to_string()).or_default();
        times.retain(|t| at.saturating_sub(*t) < CONTINUES_SECS * 1000);
        if times.len() >= CONTINUES {
            return Err("error.closed.tooMany".into());
        }
        self.send_by_itself(id, CONTINUE.into(), crate::model::Wake::of("wake.closed"))?;
        self.closed_continues.entry(id.to_string()).or_default().push(at);
        Ok(())
    }

    /// One leader of `resume_cut_leaders`, whose `members` owed nothing when Faber opened.
    fn continue_cut_leader(&mut self, id: &str, members: &[String]) {
        if self.load(id).is_some_and(|c| c.waiting_on.is_some()) {
            return;
        }
        // Something a member does since (the person sent it a turn) will wake the leader when it ends.
        if !self.store.settings().resume_interrupted() {
            return;
        }
        let woken = members.iter().any(|m| {
            self.live.get(m).is_some_and(|l| l.turn.is_some())
                || self.convs.get(m).is_some_and(|c| c.pending_parent_report || c.keep_going.is_some() || !c.queued.is_empty())
        });
        if woken || self.parent_reports.contains_key(id) {
            return;
        }
        let Some(conv) = self.load(id) else { return };
        let Some(turn) = conv.messages.last().filter(|m| m.closed).map(|m| m.id.clone()) else { return };
        // A turn Faber sent to pick up after a close or an update that was itself cut waits for the person: the limit of
        // three lives in memory, so a Faber that keeps crashing would otherwise send one at every start.
        let again = conv
            .messages
            .len()
            .checked_sub(2)
            .and_then(|i| conv.messages.get(i))
            .and_then(|m| m.wake.as_ref())
            .is_some_and(|w| matches!(w.why.as_str(), "wake.closed" | "wake.updated"));
        if again || conv.keep_going.is_some() || conv.archived_at.is_some() {
            return;
        }
        let _ = self.continue_closed_now(id, &turn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Session;

    /// A conversation whose last reply was streaming when it was saved, read back from disk as after a restart.
    fn reloaded(engine: &Engine, status: Status) -> String {
        let id = super::super::answer::tests::seed(engine, Session::from_fn(|_| {}), &[("reply", &[])]);
        engine.with(|core| {
            core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().status = Some(status);
            core.save(&id);
            core.live.clear();
            core.convs.remove(&id);
            core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
            core.ledger.claude.billing = Billing::Subscription;
            core.load(&id);
        });
        id
    }

    #[tokio::test]
    async fn only_a_reply_cut_by_closing_faber_loads_as_closed() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let cut = reloaded(&engine, Status::Streaming);
        let stopped = reloaded(&engine, Status::Stopped);
        engine.with(|core| {
            let reply = core.convs[&cut].messages.last().unwrap();
            assert_eq!((reply.status, reply.closed), (Some(Status::Stopped), true), "a streaming reply was cut by the close");
            assert!(!core.convs[&stopped].messages.last().unwrap().closed, "the person's own Stop is no close");
        });
        assert_eq!(engine.continue_closed(&stopped, "reply"), Err("error.closed.stale".into()));
        // A branch from the cut reply never ran, so it has nothing to continue.
        let copy = engine.branch_here(&cut, "reply").unwrap();
        engine.with(|core| assert!(!core.convs[&copy].messages.last().unwrap().closed));
        assert_eq!(engine.continue_closed(&copy, "reply"), Err("error.closed.stale".into()));
        engine.shutdown();
    }

    #[tokio::test]
    async fn continue_sends_one_turn_of_faber_s_own_and_at_most_three_in_five_minutes() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = reloaded(&engine, Status::Streaming);
        // Three continues in the last five minutes: the fourth is refused and sends nothing.
        engine.with(|core| core.closed_continues.insert(id.clone(), vec![now(); 3]));
        assert_eq!(engine.continue_closed(&id, "reply"), Err("error.closed.tooMany".into()));
        engine.with(|core| assert_eq!(core.convs[&id].messages.len(), 2));
        // A turn that would only wait (here behind the person's Stop) is refused, so clicks never stack turns in the queue.
        engine.with(|core| core.convs.get_mut(&id).unwrap().stopped = true);
        engine.with(|core| core.closed_continues.clear());
        assert_eq!(engine.continue_closed(&id, "reply"), Err("error.closed.stale".into()));
        engine.with(|core| {
            let c = core.convs.get_mut(&id).unwrap();
            assert!(c.queued.is_empty(), "nothing waits");
            c.stopped = false;
        });
        // Older ones no longer count.
        engine.with(|core| core.closed_continues.insert(id.clone(), vec![now() - CONTINUES_SECS * 1000 - 1; 3]));
        assert_eq!(engine.continue_closed(&id, "reply"), Ok(()));
        engine.with(|core| {
            let sent = core.convs[&id].messages.iter().find(|m| m.wake.is_some()).expect("one turn went out");
            assert_eq!(sent.wake.as_ref().unwrap().why, "wake.closed", "the turn is Faber's own, said as such");
            assert_eq!(sent.text, CONTINUE, "the person's message is never sent again");
            assert_eq!(core.closed_continues[&id].len(), 1, "only continues in the last five minutes are kept");
        });
        // The reply is no longer the last one: a second click sends nothing more.
        assert_eq!(engine.continue_closed(&id, "reply"), Err("error.closed.stale".into()));
        engine.shutdown();
    }
}
