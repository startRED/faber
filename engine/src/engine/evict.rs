//! Conversations nothing uses leave memory (R461). A conversation holds every message, tool input and tool output it ever
//! had: 98 MB in memory for an 83 MB file, and `load` kept each one opened until Faber closed, so the engine reached 707 MB
//! after three hours of real use (measured 2026-10-27). One that the screen does not show and no part of the engine is
//! working on goes back to its file, and `load` reads it again when it is needed.
//!
//! The rule keeps a conversation whenever anything might still read it from memory: a live or closing CLI, a queue, a
//! running-list entry (a question, a check, a wait), Keep going, a helper, a follow-up, a recovery, a copy being made, a
//! parent of a kept conversation, or a message of the person's in the last 15 minutes (a mission waits for them,
//! `goals::person_there`). When unsure, it stays.
use super::*;

/// What a conversation knows only in memory and gets back when it is read again: its CLIs' commands and context readings.
/// `stopped` is not kept: it matters only while a turn closes or a queue waits, and neither lets a conversation go.
pub(super) struct Evicted {
    commands: BTreeMap<Vendor, Vec<Command>>,
    reported: (Option<u64>, Option<u64>),
}

impl Core {
    /// The screen opened `id`: it stays, and the conversations nothing uses leave memory.
    pub(super) fn shown(&mut self, id: &str) {
        self.shown = Some(id.to_string());
        self.evict_idle();
    }

    /// Drops every conversation nothing uses, after saving it, so its file holds all it had.
    pub(super) fn evict_idle(&mut self) {
        // A run that just ended leaves its done card while the conversation is still here (`ended.rs`).
        self.refresh_running();
        let now = now();
        let gone: Vec<String> = self.convs.keys().filter(|id| self.unused(id, now)).cloned().collect();
        for id in gone {
            let Some(c) = self.convs.get(&id) else { continue };
            // A file something else changed or removed since Faber last read or wrote it is not overwritten: the conversation
            // stays in memory, as it did before. One whose file could not be written stays too: memory is its only copy.
            if !self.store.unchanged(&id) || self.store.save_checked(c).is_err() {
                continue;
            }
            if let Some(c) = self.convs.remove(&id) {
                self.evicted.insert(id, Evicted { commands: c.commands, reported: c.reported });
            }
        }
    }

    /// Gives a conversation read again what it had only in memory.
    pub(super) fn restore_evicted(&mut self, c: &mut Conversation) {
        if let Some(e) = self.evicted.remove(&c.id) {
            c.commands = e.commands;
            c.reported = e.reported;
        }
    }

    fn unused(&self, id: &str, now: u64) -> bool {
        let Some(c) = self.convs.get(id) else { return false };
        let quiet = (crate::project::goals::QUIET * 1000.0) as u64;
        let person_recent =
            c.messages.iter().rev().find(|m| m.role == Role::User && m.wake.is_none()).is_some_and(|m| now.saturating_sub(m.at) < quiet);
        self.shown.as_deref() != Some(id)
            && self.draft.as_deref() != Some(id)
            && !person_recent
            && !self.live.contains_key(id)
            && self.session_idle(id)
            && running_item(c, None).is_none()
            && c.queued.is_empty()
            && c.keep_going.is_none()
            && !c.compacting
            && c.weighing.is_none()
            && c.send_after_stop.is_none()
            && !self.save_failed.lock().unwrap_or_else(std::sync::PoisonError::into_inner).contains(id)
            && !self.transfers.contains_key(id)
            && !self.copying.contains_key(id)
            && !self.asides.contains_key(id)
            && !self.asking.contains_key(id)
            && !self.watched.contains_key(id)
            && !self.heard.contains_key(id)
            && !self.draft_routes.contains_key(id)
            && !self.follows.contains_key(id)
            && !self.boundary.contains_key(id)
            && !self.pending.contains_key(id)
            && !self.compacts.contains_key(id)
            && !self.before.contains_key(id)
            && !self.parent_reports.contains_key(id)
            && !self.codex_prepared.contains_key(id)
            && !self.bigger.contains_key(id)
            && !self.creative_runs.contains(id)
            && !self.convs.values().any(|other| other.started_by.as_deref() == Some(id))
    }
}
