//! The context of a conversation's own agent: the fill the screen's meter shows, and the compaction the agent asks for with
//! Faber's conversation tool (`compact`). A subagent's or a helper's context never counts here.
//!
//! Compacting happens once the agent's turn is over, never inside it: the CLI replaces the conversation with a summary, so a
//! turn that goes on would lose what it still needs. When the turn ends well and nothing else runs (Faber's checks included),
//! Faber asks the CLI to compact (`Session::compact`); the conversation counts as at work meanwhile (`Conversation::compacting`),
//! so what the person writes and what Faber sends by itself wait in the queue. When the CLI says it ended, the agent's own
//! `next` is sent to it as a message Faber writes (`wake.compacted`), ahead of whatever waited, and the conversation goes on.
use super::*;
use crate::model::ContextFill;
use crate::sibling;

/// What the agent asked for with `compact`, kept until its turn ends.
pub(super) struct Plan {
    /// Everything it needs to continue; sent back to it word for word.
    next: Option<String>,
}

impl Core {
    /// The CLI reported how full the main agent's context is, or the window of its model (either may be missing).
    pub(super) fn note_context(&mut self, conv: &str, vendor: Vendor, used: Option<u64>, window: Option<u64>) {
        let model = self.live.get(conv).and_then(|l| l.model.clone().or_else(|| l.running.clone()));
        // Claude Code is started with the point where it compacts (`--autocompact`): that is the size the conversation is
        // counted against, since it never grows beyond it.
        let point = (vendor == Vendor::Claude && self.claude_flags.autocompact)
            .then(|| model.as_deref().and_then(crate::claude::autocompact_tokens))
            .flatten();
        let Some(c) = self.convs.get_mut(conv) else { return };
        c.reported = (used.or(c.reported.0), window.or(c.reported.1));
        let window = match (point, c.reported.1) {
            (Some(point), Some(window)) => Some(point.min(window)),
            (point, window) => point.or(window),
        };
        let fill = c.reported.0.zip(window).map(|(used, window)| ContextFill { used, window });
        if fill.is_none() || fill == c.context {
            return;
        }
        c.context = fill;
        (self.emit)(AppEvent::Context { conversation_id: conv.to_string(), context: fill });
    }

    /// The CLI compacted the conversation: what it holds is the summary now. Claude Code says how big it is; Codex reports
    /// the new size as a context update of its own.
    pub(super) fn note_compacted(&mut self, conv: &str, vendor: Vendor, after: Option<u64>) {
        if after.is_some() {
            self.note_context(conv, vendor, after, None);
        }
    }

    /// The agent called `context`.
    pub(super) fn context_answer(&self, conv: &str) -> String {
        sibling::context(self.convs.get(conv).and_then(|c| c.context).map(|c| (c.used, c.window)))
    }

    /// The agent called `compact`: it happens when its turn ends.
    pub(super) fn plan_compaction(&mut self, conv: &str, next: Option<String>) -> Result<String, &'static str> {
        if !self.live.contains_key(conv) {
            return Err("this conversation has no running agent");
        }
        if self.convs.get(conv).is_some_and(|c| c.compacting) {
            return Err("it is being compacted already");
        }
        let answer = sibling::compact_planned(next.is_some());
        self.compacts.insert(conv.to_string(), Plan { next });
        Ok(answer)
    }

    /// The point where the queue, Faber's checks and Keep going would go on (`drain`): when a compaction was asked for and
    /// the agent's last turn ended well, it starts now. True while it starts or runs, so nothing else goes out meanwhile.
    pub(super) fn compact_due(&mut self, conv: &str) -> bool {
        if !self.compacts.contains_key(conv) {
            return false;
        }
        let Some(l) = self.live.get(conv) else {
            self.compacts.remove(conv);
            return false;
        };
        if l.turn.is_some() {
            return false;
        }
        let Some(c) = self.convs.get_mut(conv) else { return false };
        if c.compacting {
            return true;
        }
        if verifying::checking(c) || c.messages.last().is_none_or(|m| m.status != Some(Status::Done)) {
            return false;
        }
        c.compacting = true;
        l.session.compact();
        self.list_changed();
        true
    }

    /// The CLI is compacting this conversation's context.
    pub(super) fn compacting(&self, conv: &str) -> bool {
        self.convs.get(conv).is_some_and(|c| c.compacting)
    }

    /// Takes a session out of the live ones. A compaction that ran on it is over with it: nothing would ever hear its end.
    pub(super) fn retire(&mut self, conv: &str) -> Option<Live> {
        let live = self.live.remove(conv)?;
        self.compacts.remove(conv);
        if let Some(c) = self.convs.get_mut(conv).filter(|c| c.compacting) {
            c.compacting = false;
            self.list_changed();
        }
        Some(live)
    }

    /// `keep_going_after`, except while the conversation compacts: the end of the compaction goes on from there.
    pub(super) fn keep_going_unless_compacting(&mut self, conv: &str, reply_id: Option<String>) {
        if !self.convs.get(conv).is_some_and(|c| c.compacting) {
            self.keep_going_after(conv, reply_id);
        }
    }

    /// What the CLI reports while it compacts: only its end matters. Whatever else it says belongs to no reply.
    /// True when the event was taken here.
    pub(super) fn compaction_event(&mut self, conv: &str, event: &EngineEvent) -> bool {
        match event {
            // Context news and account news are the same as at any time.
            EngineEvent::Context { .. } | EngineEvent::Compacted { .. } | EngineEvent::Quota { .. } | EngineEvent::Signals(_) => false,
            EngineEvent::Done { ok, .. } => {
                self.compaction_ended(conv, *ok);
                true
            }
            // The process is gone: the generic handling ends the conversation's work; the compaction is over with it.
            EngineEvent::Error { fatal: true, .. } => {
                self.compacts.remove(conv);
                if let Some(c) = self.convs.get_mut(conv) {
                    c.compacting = false;
                }
                false
            }
            EngineEvent::Error { .. } => {
                self.compaction_ended(conv, false);
                true
            }
            _ => true,
        }
    }

    /// The CLI finished compacting, or could not. The agent's `next` goes to it as a message of Faber's, first in the queue;
    /// then the queue, Keep going and the report to a parent go on as after any turn.
    fn compaction_ended(&mut self, conv: &str, ok: bool) {
        let plan = self.compacts.remove(conv);
        let Some(c) = self.convs.get_mut(conv) else { return };
        c.compacting = false;
        // No plan left: Stop took it (`interrupt`). Nothing goes out by itself after a Stop, as after a stopped turn.
        let Some(plan) = plan else {
            c.updated_at = now();
            self.save(conv);
            self.list_changed();
            self.close_when_idle(conv);
            return;
        };
        if let Some(next) = plan.next {
            let (text, why) = match ok {
                true => (sibling::compacted_prompt(&next), "wake.compacted"),
                false => (sibling::compact_failed_prompt(&next), "wake.compactFailed"),
            };
            let choice = c.choice.clone().unwrap_or(Choice { vendor: "auto".into(), model: None, effort: None });
            c.queued.insert(
                0,
                QueuedTurn {
                    id: new_id(),
                    text,
                    images: None,
                    attachments: None,
                    choice,
                    at: now(),
                    sending: false,
                    wake: Some(Wake::of(why)),
                },
            );
        }
        c.updated_at = now();
        self.save(conv);
        self.emit_queue(conv);
        self.list_changed();
        self.drain(conv);
        let reply = self
            .convs
            .get(conv)
            .filter(|c| c.keep_going.is_some())
            .and_then(|c| c.messages.last())
            .filter(|m| m.role == Role::Assistant)
            .map(|m| m.id.clone());
        match reply {
            Some(reply) => self.go_on(conv, &reply),
            // Nothing follows: a conversation an agent started reports to its parent now.
            None => self.tell_parent(conv),
        }
        self.close_when_idle(conv);
    }
}
