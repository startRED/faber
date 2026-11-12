//! A project turn that stops before the job the person asked for is done, with no real question for them, is sent on by
//! Faber instead of waiting for someone to say "continue" (owner, 2026-11-12: "it should be a good, seamless agent"). Seen
//! in the bench runs of that day: a closing box `needs_you` whose only question was "Should I continue to the final film?",
//! a box `done` that still listed what it would do next, and a turn that stopped at a checkpoint of its own. The closing
//! box says it in structure, in any language: an outcome `needs_you` or `done` with work left in `not_done`. Faber then
//! sends one turn under its own line (`wake.carryOn`) telling the agent to carry on, and to stop only for what only the person
//! can give. A real question survives it: the agent ends again with it, saying why it is theirs.
//!
//! Guards against loops and spend: Keep going has its own answers (`keep_going.rs`) and is left alone; never while a turn,
//! a compaction, a queue, a wait for members or for a background command (`command_wait.rs`) is in place, nor after Stop
//! or archive, nor where no AI has room or the goal is out of money (`send_by_itself`); an agent's own question card,
//! failing checks, a plan to approve and a `blocked` box are the person's. At most `MAX_CARRY_ON` per message of the person,
//! and none after a carry-on turn that changed no file: an agent that answered it with the same box meant it.

use super::*;
use crate::model::{Outcome, Wake};

/// Carry-on turns at most between two messages of the person.
pub(super) const MAX_CARRY_ON: usize = 3;

/// What the agent is told; `{left}` is its own `not_done`.
const CARRY_ON: &str = "Faber sends this, not the person: your closing box leaves work for later ({left}). The person asked for the whole job, so do not stop at a checkpoint or ask leave to go on: carry on now until it is done and verified. Stop only for what only the person can give (a choice of direction or taste, money, an account or access, or something only they can see or do); then end with outcome needs_you and ask exactly that, saying why it is theirs. If your question was already one only the person can answer, end again with the same box and change nothing. If what is left is only what cannot be done or checked here, end again with your closing box and say so in not_done.";

/// What is left, when the conversation's latest reply stopped short of the job with no real question for the person.
pub(super) fn left_undone(c: &Conversation) -> Option<String> {
    if c.area != Area::Project
        || c.keep_going.is_some()
        || c.waiting_on.is_some()
        || !c.queued.is_empty()
        || c.stopped
        || c.archived_at.is_some()
        || c.compacting
    {
        return None;
    }
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant && m.status == Some(Status::Done))?;
    if reply.error.is_some()
        || reply.closed
        || reply.halt.is_some()
        || keep_going::unanswered_question(reply)
        || reply.checks.as_ref().is_some_and(|k| k.state != ChecksState::Passed)
        || reply.permissions.iter().flatten().next().is_some()
        || reply.parked.as_ref().is_some_and(|p| !p.is_empty())
    {
        return None;
    }
    let wrap = reply.wrap_up.as_ref().filter(|w| w.answered.is_none())?;
    if !matches!(wrap.outcome(), Some(Outcome::NeedsYou | Outcome::Done)) {
        return None;
    }
    let left = wrap.summary.as_ref().map(|s| s.not_done.trim()).filter(|s| !s.is_empty())?;
    // Since the person's last message: how many carry-on turns went out, and whether the newest changed anything.
    let since: Vec<&Message> = c.messages.iter().rev().take_while(|m| m.role != Role::User || m.wake.is_some()).collect();
    let sent = since.iter().filter(|m| is_carry_on(m)).count();
    let answered_carry_on = c.messages.len() >= 2 && is_carry_on(&c.messages[c.messages.len() - 2]);
    let changed = reply.tools.iter().flatten().any(|t| !t.declined && t.ok != Some(false) && !crate::nudge::edited(t).is_empty());
    if sent >= MAX_CARRY_ON || (answered_carry_on && !changed) {
        return None;
    }
    Some(left.chars().take(600).collect())
}

fn is_carry_on(m: &Message) -> bool {
    m.role == Role::User && m.wake.as_ref().is_some_and(|w| w.why == "wake.carryOn")
}

impl Core {
    /// The end of a turn (`finish`): a reply that stopped short of the job goes on by itself. True when the turn went out.
    pub(super) fn carry_on(&mut self, conv: &str) -> bool {
        if self.live.get(conv).is_some_and(|l| l.turn.is_some() || l.command_wait.is_some())
            || self.compacts.contains_key(conv)
            || self.compactions.contains_key(conv)
            || self.room_refusal(conv).is_some()
        {
            return false;
        }
        let Some(left) = self.convs.get(conv).and_then(left_undone) else { return false };
        self.send_by_itself(conv, CARRY_ON.replace("{left}", &left), Wake::of("wake.carryOn")).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConclusionSummary, WrapUp};

    fn person(text: &str) -> Message {
        Message::new(new_id(), Role::User, text.into(), 1)
    }

    fn sent_on() -> Message {
        let mut m = person(CARRY_ON);
        m.wake = Some(Wake::of("wake.carryOn"));
        m
    }

    fn reply(outcome: Outcome, left: &str, wrote: bool) -> Message {
        let mut m = Message::new(new_id(), Role::Assistant, "Stopped here.".into(), 2);
        m.status = Some(Status::Done);
        m.wrap_up = Some(WrapUp {
            recap: "Part of it.".into(),
            outcome: Some(outcome),
            summary: Some(ConclusionSummary { did: "d".into(), result: "r".into(), not_done: left.into(), verified: String::new() }),
            ..Default::default()
        });
        if wrote {
            m.tools = Some(vec![
                serde_json::from_value(serde_json::json!({ "id": "w", "name": "Write", "input": { "file_path": "a.html" }, "ok": true }))
                    .unwrap(),
            ]);
        }
        m
    }

    fn project(messages: Vec<Message>) -> Conversation {
        let mut c: Conversation = serde_json::from_value(serde_json::json!({
            "id": "c", "title": "Film", "area": "project", "createdAt": 0, "updatedAt": 0, "messages": []
        }))
        .unwrap();
        c.messages = messages;
        c
    }

    #[test]
    fn work_left_is_sent_on_a_few_times_at_most_and_never_after_a_turn_that_changed_nothing() {
        let left = "the final render";
        assert!(left_undone(&project(vec![person("go"), reply(Outcome::NeedsYou, left, true)])).is_some());
        assert!(left_undone(&project(vec![person("go"), reply(Outcome::Done, left, false)])).is_some(), "done while work is left");
        assert!(left_undone(&project(vec![person("go"), reply(Outcome::Blocked, left, true)])).is_none(), "blocked is the person's");
        assert!(left_undone(&project(vec![person("go"), reply(Outcome::NeedsYou, " ", true)])).is_none(), "a pure decision");
        let same = project(vec![person("go"), reply(Outcome::NeedsYou, left, true), sent_on(), reply(Outcome::NeedsYou, left, false)]);
        assert!(left_undone(&same).is_none(), "the same box again after changing nothing: it meant it");
        let mut many = vec![person("go"), reply(Outcome::NeedsYou, left, true)];
        for _ in 0..MAX_CARRY_ON {
            many.extend([sent_on(), reply(Outcome::NeedsYou, left, true)]);
        }
        assert!(left_undone(&project(many.clone())).is_none(), "{MAX_CARRY_ON} at most per message of the person");
        many.extend([person("keep at it"), reply(Outcome::NeedsYou, left, true)]);
        assert!(left_undone(&project(many)).is_some(), "the person's next message starts the count again");
        let mut kept = project(vec![person("go"), reply(Outcome::NeedsYou, left, true)]);
        kept.keep_going = Some(Default::default());
        assert!(left_undone(&kept).is_none(), "Keep going answers its own boxes");
        let mut chat = project(vec![person("go"), reply(Outcome::NeedsYou, left, true)]);
        chat.area = Area::Chat;
        assert!(left_undone(&chat).is_none());
    }
}
