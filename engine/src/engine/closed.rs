//! A reply Faber was closed under: loading the conversation marks it (`Message::closed`), the screen says so under it, and
//! "Continue" sends one turn Faber writes by itself (`wake.closed`), never the person's message again.
use super::*;

/// At most this many continues per conversation in `CONTINUES_SECS`, so a Faber that keeps closing cannot loop the agent.
const CONTINUES: usize = 3;
const CONTINUES_SECS: u64 = 5 * 60;

const CONTINUE: &str = "Faber was closed while your last answer was running, which cut it short. Before you go on, look at the \
files and the work as they are now: some steps may have finished and others not. Then continue from where you stopped; do not \
start over.";

impl Engine {
    /// Continues `id` after its last reply, `turn_id`, was cut by Faber closing. Refused when that reply is no longer the last
    /// one or was not cut that way (it was continued already, or the person wrote since), while the turn would only wait in
    /// the queue, and past the cap.
    pub fn continue_closed(&self, id: &str, turn_id: &str) -> Result<(), String> {
        self.with(|c| {
            let conv = c.load(id).ok_or("error.conversationGone")?;
            // Only a turn that goes out now: one that would wait in the queue leaves this reply last, and a second click would
            // queue another.
            let waits = conv.stopped || !conv.queued.is_empty() || verifying::checking(conv);
            if !conv.messages.last().is_some_and(|m| m.id == turn_id && m.closed)
                || waits
                || c.live.get(id).is_some_and(|l| l.turn.is_some())
            {
                return Err("error.closed.stale".into());
            }
            let at = now();
            let times = c.closed_continues.entry(id.to_string()).or_default();
            times.retain(|t| at.saturating_sub(*t) < CONTINUES_SECS * 1000);
            if times.len() >= CONTINUES {
                return Err("error.closed.tooMany".into());
            }
            c.send_by_itself(id, CONTINUE.into(), crate::model::Wake::of("wake.closed"))?;
            c.closed_continues.entry(id.to_string()).or_default().push(at);
            Ok(())
        })
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
