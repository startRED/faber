//! Conversations nothing uses leave memory (R461). A conversation holds every message, tool input and tool output it ever
//! had: 98 MB in memory for an 83 MB file, and `load` kept each one opened until Faber closed, so the engine reached 707 MB
//! after three hours of real use (measured 2026-10-27). One that the screen does not show and no part of the engine is
//! working on goes back to its file, and `load` reads it again when it is needed.
//!
//! The rule keeps a conversation whenever anything might still read it from memory: a live or closing CLI, a queue, a
//! running-list entry (a question, a check, a wait), Keep going, a helper, a follow-up, a recovery, a copy being made, a
//! parent of a kept conversation, or a message of the person's in the last 15 minutes (a mission waits for them,
//! `goals::person_there`). When unsure, it stays.
//!
//! The check runs when the screen opens a conversation, when an idle CLI closes, and every [`LET_GO_EVERY`] besides: a run
//! of a CLI that keeps no idle process (Codex, Antigravity) or a message turning 15 minutes old changes nothing on screen,
//! and its conversation stayed in memory until the person opened another one.
use super::*;

/// How often the engine looks for conversations nothing uses any more, without the screen doing anything.
pub(super) const LET_GO_EVERY: f64 = 5.0 * 60.0;

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
        if self.evict_idle() {
            faber_winproc::release_freed_memory();
        }
    }

    /// Drops every conversation nothing uses, after saving it, so its file holds all it had. True when one left: a conversation
    /// is millions of small allocations, and the caller then asks the allocator to give them back
    /// (`faber_winproc::release_freed_memory`), outside the engine's lock where it can.
    #[must_use]
    pub(super) fn evict_idle(&mut self) -> bool {
        // A run that just ended leaves its done card while the conversation is still here (`ended.rs`).
        self.refresh_running();
        // The engine's clock, so a test can move the 15 minutes forward (`Engine::set_time_source`).
        let now = (self.secs() * 1000.0) as u64;
        let gone: Vec<String> = self.convs.keys().filter(|id| self.unused(id, now)).cloned().collect();
        let before = self.convs.len();
        for id in gone {
            self.evict_saved(&id);
        }
        self.convs.len() < before
    }

    pub(super) fn evict_if_unused(&mut self, id: &str) {
        if self.unused(id, now()) {
            self.evict_saved(id);
        }
    }

    fn evict_saved(&mut self, id: &str) {
        let Some(c) = self.convs.get(id) else { return };
        // A file changed elsewhere cannot be overwritten. A conversation that cannot be saved stays in memory.
        if !self.store.unchanged(id) || self.store.save_checked(c).is_err() {
            return;
        }
        if let Some(c) = self.convs.remove(id) {
            self.transcript_saved(id);
            self.evicted.insert(id.to_string(), Evicted { commands: c.commands, reported: c.reported });
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
            && self.running_item(c).is_none()
            && c.queued.is_empty()
            && c.keep_going.is_none()
            && !c.compacting
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
            && !self.parent_reports.contains_key(id)
            && !self.codex_prepared.contains_key(id)
            && !self.bigger.contains_key(id)
            && !self.creative_runs.contains(id)
            && !self.convs.values().any(|other| other.started_by.as_deref() == Some(id))
    }
}
