//! Archiving a conversation: the person, or the conversation's own agent through Faber's conversation tool once its work
//! is done (`crate::sibling`), puts it away. It leaves the sidebar and the pick-up lists, and opened, its composer gives
//! way to one line that brings it back. A turn that runs goes on, but Keep going ends: an archived conversation's work is
//! over, and what Faber queued by itself goes. Writing to it brings it back by itself (`Core::send`).
use super::*;

pub(super) struct Reopened {
    at: u64,
    updated: u64,
    kept: Option<bool>,
}

pub(super) fn reopen(c: &mut Conversation) -> Option<Reopened> {
    let at = c.archived_at.take()?;
    let updated = std::mem::replace(&mut c.updated_at, now());
    let kept = c.messages.iter_mut().rev().find_map(|m| m.follow.as_mut()).map(|f| std::mem::replace(&mut f.kept, true));
    Some(Reopened { at, updated, kept })
}

pub(super) fn restore(c: &mut Conversation, reopened: Option<Reopened>) {
    if let Some(before) = reopened {
        c.archived_at = Some(before.at);
        c.updated_at = before.updated;
        if let Some(kept) = before.kept
            && let Some(follow) = c.messages.iter_mut().rev().find_map(|m| m.follow.as_mut())
        {
            follow.kept = kept;
        }
    }
}

impl Engine {
    pub fn pin(&self, id: &str, on: bool) -> Result<(), String> {
        self.with(|core| {
            let c = core.load(id).ok_or("error.conversationGone")?;
            if on == c.pinned_at.is_some() || (on && c.archived_at.is_some()) {
                return Ok(());
            }
            c.pinned_at = on.then(now);
            core.save(id);
            core.list_changed();
            Ok(())
        })
    }

    pub fn archive(&self, id: &str, on: bool) -> Result<(), String> {
        self.with(|c| c.set_archived(id, on))
    }
}

impl Core {
    pub(super) fn set_archived(&mut self, id: &str, on: bool) -> Result<(), String> {
        let c = self.load(id).ok_or("error.conversationGone")?;
        if on == c.archived_at.is_some() {
            return Ok(());
        }
        if !on {
            // Brought back, it is as new as a message: Faber does not put it away again for the week it was quiet, nor for
            // the pull request that ended (`auto_archive`).
            let _ = reopen(c);
        }
        if on {
            c.archived_at = Some(now());
            c.waiting_on = None;
            c.pinned_at = None;
            // What Faber queued by itself (a report, a question, Keep going) never wakes an archived conversation.
            c.queued.retain(|q| q.sending || q.wake.is_none());
        }
        self.save(id);
        if on {
            self.emit_queue(id);
            self.keep_going_off(id);
        }
        self.list_changed();
        Ok(())
    }
}
