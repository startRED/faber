//! What an AI does with other conversations, as one set of verbs for every way it asks (`crate::sibling`): Faber's
//! conversation tool inside the AI, and the commands of the host table (`start_conversation`, `message_conversation`,
//! `resume_conversation`, `interrupt`, `archive`, `mark_waiting`) that the command line and the AI's own tools send. `by` is the
//! conversation of the AI that asks. An AI acts on the conversations it started (and, through them, theirs), never on the
//! person's own or on its parent; the refusal is an English sentence the AI reads.
use super::*;
use crate::sibling::Ai;

/// What a new conversation an AI starts is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    pub title: String,
    pub brief: String,
    pub ai: Ai,
    /// A model of the AI named by `ai` (or of the starter's own AI when `ai` is `same`); never with `auto`.
    pub model: Option<String>,
    /// `low`, `medium` or `high`; never with `auto`.
    pub effort: Option<String>,
    pub keep_going: bool,
    pub step: Option<String>,
}

impl Engine {
    /// Starts a conversation of the project `by` works in, linked to it. Gives the new id.
    pub fn session_start(&self, by: &str, new: NewSession) -> Result<String, String> {
        self.with(|c| c.session_start(by, new))
    }

    /// Sends `message` to another conversation of `by`'s project as a message Faber labels. Gives what the AI is told.
    pub fn session_message(&self, by: &str, id: &str, message: &str) -> Result<String, String> {
        self.with(|c| c.session_message(by, id, message))
    }

    /// Stops the turn of a conversation `by` started. Gives what the AI is told.
    pub fn session_stop(&self, by: &str, id: &str) -> Result<String, String> {
        self.with(|c| c.session_stop(by, id))
    }

    /// Goes on with a conversation `by` started that is stopped, cut off, failed or ended, with `message` or the usual
    /// "Continue where you stopped" line. Gives what the AI is told.
    pub fn session_resume(&self, by: &str, id: &str, message: Option<&str>) -> Result<String, String> {
        self.with(|c| c.session_resume(by, id, message))
    }

    /// Archives a conversation `by` started that is not working, not waiting for the person and not pinned.
    pub fn session_archive(&self, by: &str, id: &str) -> Result<String, String> {
        self.with(|c| c.session_archive(by, id))
    }

    /// Declares that `by` waits for the conversations in `on` (all started by it) and/or for `note`; see `Conversation::waiting_on`.
    pub fn session_wait(&self, by: &str, on: Vec<String>, note: Option<String>) -> Result<String, String> {
        self.with(|c| c.session_wait(by, on, note))
    }
}
