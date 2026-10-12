//! What every driver reports to the engine, whatever CLI is behind it.

use crate::model::Image;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    Started {
        session_id: String,
        model: String,
    },
    Text(String),
    /// A new block of text starts (for example after a tool ran), so it must not run into the previous one.
    Break,
    Thinking(String),
    Tool {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        ok: bool,
        text: String,
    },
    Permission {
        request_id: String,
        tool: String,
        input: Value,
    },
    Quota {
        window: String,
        used: f64,
        resets_at: f64,
    },
    /// What a finished turn used. `cost_total_usd` is Claude Code's own API-priced figure as it reports it: cumulative for
    /// the conversation, so the engine subtracts what it saw before (`usage::settle`); None when the result carries no
    /// counters (an aborted turn) and on Codex, which reports no cost. `input_tokens` is the prompt of the main response,
    /// cached part included, so a hit rate is `cache_read_tokens / input_tokens`.
    Done {
        ok: bool,
        reason: String,
        cost_total_usd: Option<f64>,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
        /// The part of the cache written with the 1-hour lifetime (0 on Codex, which bills no write).
        cache_write_1h_tokens: u64,
        /// True when the token numbers cover only part of the turn: Claude's sub-model work is not in `usage`, and a Codex
        /// turn with no usage update to measure from reports none.
        incomplete: bool,
    },
    /// `fatal`: the CLI process is gone and the next turn needs a new one.
    Error {
        message: String,
        fatal: bool,
    },
}

/// A user turn: plain text plus optional images.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserTurn {
    pub text: String,
    pub images: Vec<Image>,
}

impl UserTurn {
    pub fn text(text: impl Into<String>) -> Self {
        UserTurn { text: text.into(), images: Vec::new() }
    }
}

/// What a session says about an answer it was given, once it has done the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The answer was written and flushed to the CLI's input.
    Delivered,
    /// The session holds no such pending request, so it wrote nothing.
    Unknown,
    /// The write or the flush failed. The session keeps the request, so the same answer can be given again.
    WriteFailed,
}

/// What the engine can ask of a live CLI session.
#[derive(Debug)]
pub enum SessionCommand {
    Send(UserTurn),
    /// `ack` hears what became of the answer. A session that is gone drops it, and the receiver then fails.
    Answer {
        request_id: String,
        allow: bool,
        ack: tokio::sync::oneshot::Sender<Delivery>,
    },
    Interrupt,
    Close,
}

/// One live conversation with a CLI. Commands are queued, so the caller never waits on the process.
#[derive(Clone)]
pub struct Session {
    send: std::sync::Arc<dyn Fn(SessionCommand) + Send + Sync>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Session")
    }
}

impl Session {
    pub(crate) fn new(tx: tokio::sync::mpsc::UnboundedSender<SessionCommand>) -> Self {
        Self::from_fn(move |cmd| drop(tx.send(cmd)))
    }
    /// A session whose commands go wherever `send` puts them, such as a thread in a shared process.
    pub(crate) fn from_fn(send: impl Fn(SessionCommand) + Send + Sync + 'static) -> Self {
        Session { send: std::sync::Arc::new(send) }
    }
    pub fn send(&self, turn: UserTurn) {
        (self.send)(SessionCommand::Send(turn));
    }
    /// The receiver hears whether the CLI got the answer. If the session is gone it fails instead, because the
    /// command, and the sender in it, were dropped.
    pub fn answer(&self, request_id: &str, allow: bool) -> tokio::sync::oneshot::Receiver<Delivery> {
        let (ack, delivered) = tokio::sync::oneshot::channel();
        (self.send)(SessionCommand::Answer { request_id: request_id.to_string(), allow, ack });
        delivered
    }
    pub fn interrupt(&self) {
        (self.send)(SessionCommand::Interrupt);
    }
    pub fn close(&self) {
        (self.send)(SessionCommand::Close);
    }
}
