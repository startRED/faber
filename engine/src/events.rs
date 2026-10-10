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
    /// `cost_usd` is the CLI's own API-priced figure when it reports one (Claude Code does, Codex does not).
    Done {
        ok: bool,
        reason: String,
        cost_usd: Option<f64>,
        input_tokens: u64,
        output_tokens: u64,
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

/// What the engine can ask of a live CLI session.
#[derive(Debug)]
pub enum SessionCommand {
    Send(UserTurn),
    Answer { request_id: String, allow: bool },
    Interrupt,
    Close,
}

/// One live conversation with a CLI. Commands are queued, so the caller never waits on the process.
#[derive(Debug, Clone)]
pub struct Session {
    tx: tokio::sync::mpsc::UnboundedSender<SessionCommand>,
}

impl Session {
    pub(crate) fn new(tx: tokio::sync::mpsc::UnboundedSender<SessionCommand>) -> Self {
        Session { tx }
    }
    pub fn send(&self, turn: UserTurn) {
        let _ = self.tx.send(SessionCommand::Send(turn));
    }
    pub fn answer(&self, request_id: &str, allow: bool) {
        let _ = self.tx.send(SessionCommand::Answer { request_id: request_id.to_string(), allow });
    }
    pub fn interrupt(&self) {
        let _ = self.tx.send(SessionCommand::Interrupt);
    }
    pub fn close(&self) {
        let _ = self.tx.send(SessionCommand::Close);
    }
}
