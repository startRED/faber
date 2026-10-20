//! Faber sends a turn into a conversation by itself (R199). "Keep going" uses it, and so can anything else that wakes an
//! agent while nobody is there. The message joins the conversation like one the person wrote, marked with why Faber sent
//! it (`Wake`), so the screen draws a quiet line in its place and the person's words stay theirs.

use super::*;
use crate::model::Wake;

impl Core {
    /// Sends `text` as the next turn of `conv`, with the choice its last message used. While a turn runs, or the person's
    /// own messages wait, it waits in the queue behind them and is never steered into the running turn; otherwise it
    /// starts now. A refusal (no subscription, no quota) comes back as its key, and nothing is sent.
    pub(super) fn send_by_itself(&mut self, conv: &str, text: String, wake: Wake) -> Result<(), String> {
        let busy = self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let c = self.load(conv).ok_or("error.conversationGone")?;
        let choice = c.choice.clone().unwrap_or(Choice { vendor: "auto".into(), model: None, effort: None });
        let item = QueuedTurn { id: new_id(), text, images: None, attachments: None, choice, at: now(), sending: false, wake: Some(wake) };
        // Faber's checks of the last reply count as a turn that runs (`verifying.rs`).
        if busy || !c.queued.is_empty() || verifying::checking(c) || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
        {
            if c.queued.len() >= MAX_QUEUED {
                return Err("error.queueFull".into());
            }
            // Approval is the person's explicit send, even though Faber supplies its visible line.
            let requested = (c.stopped && item.wake.as_ref().is_some_and(|w| w.why == "wake.approvedPlan")).then(|| item.id.clone());
            c.queued.push(item);
            if let Some(requested) = requested {
                return self.send_queued(conv, &requested);
            }
            self.save(conv);
            self.emit_queue(conv);
            return Ok(());
        }
        let id = item.id.clone();
        let sent = self.dispatch_queued(conv, item);
        // A message nobody could answer goes back to the queue; one Faber wrote is dropped instead of waiting there.
        if sent.is_err()
            && let Some(c) = self.convs.get_mut(conv)
            && c.queued.iter().any(|q| q.id == id)
        {
            c.queued.retain(|q| q.id != id);
            self.save(conv);
            self.emit_queue(conv);
        }
        sent
    }
}
