//! Archiving a conversation: the person, or the conversation's own agent through Faber's conversation tool once its work
//! is done (`crate::sibling`), puts it away. It leaves the sidebar and the pick-up lists, and opened, its composer gives
//! way to one line that brings it back. A turn that runs goes on, but Keep going ends: an archived conversation's work is
//! over, and what Faber queued by itself goes. Writing to it brings it back by itself (`Core::send`).
use super::*;

impl Engine {
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
        c.archived_at = on.then(now);
        if on {
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
