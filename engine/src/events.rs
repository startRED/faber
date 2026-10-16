//! What every driver reports to the engine, whatever CLI is behind it.

use crate::model::Image;
use serde_json::Value;

/// Startup advertisements are information, never authority to admit an automated role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub names: Vec<String>,
}

impl Capabilities {
    pub fn from_init(msg: &Value) -> Option<Self> {
        if msg["type"] != "system" || msg["subtype"] != "init" {
            return None;
        }
        let names = msg["capabilities"].as_array()?.iter().map(|v| v.as_str().map(String::from)).collect::<Option<Vec<_>>>()?;
        Some(Self { names })
    }
}

/// Project event adapters use fixed keys; Chat keeps its existing result strings.
pub fn terminal_reason(raw: &str) -> &'static str {
    match raw {
        "success" | "completed" | "end_turn" => "reason.complete",
        "interrupted" | "cancelled" => "reason.interrupted",
        "error_max_turns" | "max_turns" => "reason.turnLimit",
        "error_max_budget_usd" | "max_budget_usd" => "reason.budget",
        "error_during_execution" | "failed" => "reason.failed",
        _ => "reason.unknown",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    Started {
        session_id: String,
        model: String,
    },
    /// The CLI accepted a turn on the session. The session may have no saved history until the turn shows output.
    TurnStarted,
    Text(String),
    /// A new block of text starts (for example after a tool ran), so it must not run into the previous one.
    Break {
        final_answer: bool,
    },
    Thinking(String),
    Tool {
        id: String,
        name: String,
        input: Value,
        /// The CLI's own subagent step this tool ran inside (Claude's `parent_tool_use_id`); none for the main reply.
        parent: Option<String>,
    },
    ToolResult {
        id: String,
        ok: bool,
        /// The tool was refused before it ran (a declined approval), which is not a failure of the work.
        declined: bool,
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
    /// What the CLI said about the account besides window readings: whether it can answer, paid fallback, plan, billing.
    Signals(crate::signals::Signals),
    /// How long the prompt cache stays warm after this turn, from the cache-write buckets of the result (Claude: one hour for
    /// the 1-hour bucket, five minutes for the 5-minute one). Sent before `Done`; absent when the CLI names no bucket.
    CacheWarm {
        secs: u64,
        /// When the last request of the turn began (the start of its assistant message in the stream), in milliseconds: the
        /// lifetime counts from the request, not from the result. The parser leaves it empty; the driver fills it in and drops
        /// the event when it never saw that start.
        started_ms: Option<u64>,
    },
    /// A request the CLI denied because nobody could answer it (`--permission-prompts none`): not done, and kept for the
    /// person. Sent before `Done`, one per denial.
    Parked {
        tool_use_id: String,
        tool: String,
        input: Value,
    },
    /// What a finished turn used. `cost_total_usd` is Claude Code's own API-priced figure as it reports it: cumulative for
    /// the conversation, so the engine subtracts what it saw before (`usage::settle`); None when the result carries no
    /// counters (an aborted turn). On Codex it is Faber's computed API-equivalent cost of this turn, priced per request,
    /// so the engine uses it directly. `input_tokens` is the prompt of the main response,
    /// cached part included, so a hit rate is `cache_read_tokens / input_tokens`.
    Done {
        ok: bool,
        reason: String,
        cost_total_usd: Option<f64>,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
        /// The part of the cache written with the 1-hour lifetime (0 on Codex, which reports one cache-write kind).
        cache_write_1h_tokens: u64,
        /// True when the token numbers cover only part of the turn: Claude's sub-model work is not in `usage`, and a Codex
        /// turn with no usage update to measure from reports none.
        incomplete: bool,
    },
    /// The running turn took in a message sent to it with `Session::steer`. `tag` is the one that call carried when the CLI
    /// echoes it back; without one, the oldest message still on its way is the one.
    Injected {
        tag: Option<String>,
    },
    /// The CLI did not take a message sent with `Session::steer` (the turn had already ended, or cannot be steered). The
    /// message was not delivered, so the engine keeps it for the next turn.
    SteerFailed {
        tag: String,
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
    /// Adds a message to the turn that is running. `tag` names it in `Injected` and `SteerFailed`.
    Steer {
        turn: UserTurn,
        tag: String,
    },
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
    project_role: Option<crate::project::Role>,
    tree: Option<crate::proc::OwnedTree>,
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
        Session { send: std::sync::Arc::new(send), project_role: None, tree: None }
    }
    pub(crate) fn for_project(mut self, role: crate::project::Role) -> Self {
        self.project_role = Some(role);
        self
    }
    pub(crate) fn project_role(&self) -> Option<crate::project::Role> {
        self.project_role
    }
    pub(crate) fn with_tree(mut self, tree: Option<crate::proc::OwnedTree>) -> Self {
        self.tree = tree;
        self
    }
    /// The owned process tree behind this session, if the engine started one: the source of a lease's process
    /// identity and of its termination proof. A replayed recording has none.
    pub fn tree(&self) -> Option<crate::proc::OwnedTree> {
        self.tree.clone()
    }
    pub fn send(&self, turn: UserTurn) {
        (self.send)(SessionCommand::Send(turn));
    }
    pub fn steer(&self, turn: UserTurn, tag: &str) {
        (self.send)(SessionCommand::Steer { turn, tag: tag.to_string() });
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
