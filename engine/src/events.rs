//! What every driver reports to the engine, whatever CLI is behind it.

use crate::model::Image;
use serde_json::Value;

/// Startup advertisements are information, never authority to admit an automated role.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

/// The `reason.*` key for how a turn ended, from the CLI's own word for it: Claude Code's terminal reason (its result subtype
/// when it names none) or ChatGPT's turn status. A word Faber does not know is `reason.other`, whose sentence says so: a raw
/// string never reaches the screen (rule 6 of docs/product/models.md).
pub fn terminal_reason(raw: &str) -> &'static str {
    match raw {
        "success" | "completed" | "end_turn" => "reason.complete",
        "interrupted" | "cancelled" | "aborted_streaming" | "aborted_tools" => "reason.interrupted",
        "error_max_turns" | "max_turns" => "reason.turnLimit",
        "error_max_budget_usd" | "max_budget_usd" => "reason.budget",
        "error_during_execution" | "failed" | "model_error" | "api_error" => "reason.failed",
        _ => "reason.other",
    }
}

/// The key of a turn that did not end well, from its own words, the terminal reason first and the result subtype after it: the
/// first word Faber knows that says why. A word that says "completed" on a turn that failed is a failure. When neither word is
/// known the key is generic and the unknown word goes to the engine's stderr, once, never to the screen.
pub fn failure_reason(terminal: Option<&str>, subtype: Option<&str>) -> &'static str {
    let words = [terminal, subtype].into_iter().flatten().filter(|w| !w.is_empty());
    let keys: Vec<_> = words.clone().map(|w| (w, terminal_reason(w))).collect();
    if let Some((_, key)) = keys.iter().find(|(_, k)| !matches!(*k, "reason.other" | "reason.complete")) {
        return key;
    }
    if keys.iter().any(|(_, k)| *k == "reason.complete") {
        return "reason.failed";
    }
    for word in words {
        if unseen(word) {
            eprintln!("faber: a turn ended with a word this version does not know: {}", word.chars().take(WORD_MAX).collect::<String>());
        }
    }
    "reason.other"
}

const WORD_MAX: usize = 80;
const WORDS_MAX: usize = 64;

/// True the first time this process sees a word (cut to `WORD_MAX` characters), so each unknown ending is logged once. Past
/// `WORDS_MAX` different words nothing is stored and nothing more is logged.
fn unseen(word: &str) -> bool {
    static SEEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let word: String = word.chars().take(WORD_MAX).collect();
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    // shortcut: a short list scanned linearly, a few words ever come in a run; use a set if CLIs start to send many.
    if seen.len() >= WORDS_MAX || seen.contains(&word) {
        return false;
    }
    seen.push(word);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_ending_word_is_logged_once() {
        assert!(unseen("a_word_only_this_test_sends"));
        assert!(!unseen("a_word_only_this_test_sends"));
        // A long word is stored cut, so its repeat is known; and the list is bounded, so a full one logs nothing more.
        let long = "x".repeat(200);
        assert!(unseen(&long) && !unseen(&long));
        for n in 0..WORDS_MAX * 2 {
            unseen(&format!("bounded_{n}"));
        }
        assert!(!unseen("one_more_after_the_list_is_full"));
    }

    #[test]
    fn the_subtype_says_why_when_the_terminal_reason_is_unknown_and_nothing_known_is_generic() {
        assert_eq!(failure_reason(Some("future_word"), Some("error_max_turns")), "reason.turnLimit");
        assert_eq!(failure_reason(Some("completed"), Some("error_max_budget_usd")), "reason.budget");
        assert_eq!(failure_reason(Some("completed"), Some("success")), "reason.failed");
        assert_eq!(failure_reason(None, Some("api_error")), "reason.failed");
        assert_eq!(failure_reason(Some("future_word"), Some("future_subtype")), "reason.other");
        assert_eq!(failure_reason(None, None), "reason.other");
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineEvent {
    McpServers(Vec<crate::mcp_servers::Reported>),
    Background(crate::background::Update),
    /// This session has ended; true means every process it owns was confirmed stopped.
    WriterExit {
        confirmed: bool,
    },
    /// Tracked writers still owned by this session, including work that outlives its main turn.
    Writers {
        active: Vec<String>,
        unknown: bool,
    },
    /// The last native entry or completed turn, saved only as a continuation boundary.
    Cursor {
        entry: Option<String>,
        turn: Option<String>,
    },
    Commands(Vec<crate::model::Command>),
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
    /// The main thread's message ended with `end_turn` (Claude Code's `message_delta`): the agent has said all it will say
    /// unless a tool call is still open. The CLI's own `result` may come much later (whatever runs after the answer, such as
    /// the person's Stop hooks), so the screen reads the reply as done now while the engine still waits for `Done`.
    Settled,
    /// Like `Settled`, while helpers the agent left in the background still run: the reply is not done, but the main thread
    /// waits for no step, so a message written now goes in at once.
    MainIdle,
    Tool {
        id: String,
        name: String,
        input: Value,
        /// The subagent that called it (the id of the agent tool call that started that subagent); None on the main thread.
        parent: Option<String>,
    },
    /// A subagent wrote words of its own (`parent` is the agent call that started it). They never join the answer: they stay
    /// on that call, in order with the steps it ran (R161 step 3).
    ChildWords {
        parent: String,
        text: String,
    },
    ToolResult {
        id: String,
        ok: bool,
        /// The tool was refused before it ran (a declined approval), which is not a failure of the work.
        declined: bool,
        text: String,
    },
    /// A subagent finished (Claude Code's `task_notification`): `id` is the agent call that started it. It settles a
    /// helper that ran in the background, whose call returned as soon as it started.
    HelperDone {
        id: String,
        ok: bool,
        text: String,
    },
    /// The agent asked Faber's browser to open a page (R161 step 7); the engine answers with `Session::helper_reply`.
    BrowseCall {
        id: String,
        input: Value,
    },
    /// The agent called Faber's conversation tool (`crate::sibling`): it starts a conversation or reads one it started. The
    /// engine answers at once with `Session::helper_reply`.
    ConversationCall {
        id: String,
        input: Value,
    },
    /// The agent called Faber's checks (`crate::verify`). The engine answers with `Session::helper_reply` once they ended.
    VerifyCall {
        id: String,
        input: Value,
    },
    /// The agent asked for the project's map (`crate::map`). The engine answers with `Session::helper_reply`.
    MapCall {
        id: String,
        input: Value,
    },
    /// The agent called `quota` or `catalog` (`crate::toolbox`; `tool` is the name both drivers give it). The engine answers at
    /// once with `Session::helper_reply`.
    ToolboxCall {
        id: String,
        tool: &'static str,
        input: Value,
    },
    /// The agent called `plan`, `status` or `wrap_up` (`crate::progress`; `tool` is the name both drivers give it). The engine
    /// writes it down and answers at once with `Session::helper_reply`.
    ProgressCall {
        id: String,
        tool: &'static str,
        input: Value,
    },
    /// The agent called Faber's helper tool (R161, `crate::helper`): `id` is that tool call. The CLI waits for the answer,
    /// which the engine sends with `Session::helper_reply` once the helper is done.
    HelperCall {
        id: String,
        input: Value,
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
    /// What the conversation's own agent (never a subagent or helper) holds in its context now, as the CLI said it: `used`
    /// tokens after a request (Claude: the latest main-thread response's prompt; Codex: `last.totalTokens`), and the model's
    /// `window` when the CLI names it. Either may be absent in one report; the engine keeps the last of each.
    Context {
        used: Option<u64>,
        window: Option<u64>,
    },
    /// The CLI compacted the conversation, whoever asked (Faber, the agent, the CLI's own limit). `after` is the size it
    /// reports for the summary when it does (Claude Code's `post_tokens`).
    Compacted {
        after: Option<u64>,
    },
    /// What fills the context, part by part, in answer to `Session::context_usage` (only Claude Code counts it).
    ContextParts {
        parts: Vec<crate::model::ContextPart>,
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
    /// What the CLI said at start-up it can do (Claude Code's `capabilities` in `system/init`). The engine keeps it with the
    /// session; a feature is detected from it, never from the CLI's version.
    Capabilities(Capabilities),
    /// How a turn that did not end well ended, as a `reason.*` key the screen translates. Sent before its `Done`, only when the
    /// CLI named a reason.
    Reason(&'static str),
    /// `fatal`: the CLI process is gone and the next turn needs a new one.
    Error {
        message: String,
        fatal: bool,
    },
}

impl EngineEvent {
    /// Whether the event is the reply's own words or work, which move it: `Some(true)` from the main thread (it also undoes a
    /// finish), `Some(false)` from a subagent's tool call or a tool's result.
    pub fn moves_reply(&self) -> Option<bool> {
        match self {
            EngineEvent::Text(_) | EngineEvent::Thinking(_) => Some(true),
            EngineEvent::Tool { parent, .. } => Some(parent.is_none()),
            EngineEvent::ChildWords { .. } | EngineEvent::ToolResult { .. } | EngineEvent::HelperDone { .. } => Some(false),
            _ => None,
        }
    }
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
    McpStatus {
        reply: tokio::sync::oneshot::Sender<Option<Vec<crate::mcp_servers::Reported>>>,
    },
    StopBackground {
        id: String,
        ack: tokio::sync::oneshot::Sender<Delivery>,
    },
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
        /// What the user chose for a question the agent asked (`AskUserQuestion`): question text to the labels picked.
        reply: Option<serde_json::Value>,
        ack: tokio::sync::oneshot::Sender<Delivery>,
    },
    /// The helper's report for a `HelperCall`, which becomes that tool call's result.
    HelperReply {
        id: String,
        ok: bool,
        text: String,
        /// JPEG pictures that go with the text, base64, in order: the agent sees them (a check at several screen sizes).
        pictures: Vec<String>,
    },
    /// Compact the conversation's context now (Claude Code: the `/compact` command as the next line; Codex: the thread's
    /// compaction request). It ends like a turn, with a `Done`.
    Compact,
    /// Ask what fills the context, part by part, without a turn (Claude Code's `get_context_usage` control request, from
    /// the last response's usage and local estimates). Its answer is an `EngineEvent::ContextParts`; Codex counts no parts.
    ContextUsage,
    Interrupt,
    /// A replacement takes future turns; let this session's existing work finish before closing its driver.
    Retire,
    Close,
}

/// One live conversation with a CLI. Commands are queued, so the caller never waits on the process.
#[derive(Clone)]
pub struct Session {
    send: std::sync::Arc<dyn Fn(SessionCommand) + Send + Sync>,
    tree: Option<crate::proc::OwnedTree>,
    identity: Option<std::sync::Arc<crate::proc::ConversationIdentity>>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Session")
    }
}

impl Session {
    pub fn mcp_status(&self) -> tokio::sync::oneshot::Receiver<Option<Vec<crate::mcp_servers::Reported>>> {
        let (reply, receive) = tokio::sync::oneshot::channel();
        (self.send)(SessionCommand::McpStatus { reply });
        receive
    }
    pub fn stop_background(&self, id: &str) -> tokio::sync::oneshot::Receiver<Delivery> {
        let (ack, delivered) = tokio::sync::oneshot::channel();
        (self.send)(SessionCommand::StopBackground { id: id.into(), ack });
        delivered
    }
    pub(crate) fn new(tx: tokio::sync::mpsc::UnboundedSender<SessionCommand>) -> Self {
        Self::from_fn(move |cmd| drop(tx.send(cmd)))
    }
    /// A session whose commands go wherever `send` puts them, such as a thread in a shared process.
    pub(crate) fn from_fn(send: impl Fn(SessionCommand) + Send + Sync + 'static) -> Self {
        Session { send: std::sync::Arc::new(send), tree: None, identity: None }
    }
    pub(crate) fn with_identity(mut self, identity: std::sync::Arc<crate::proc::ConversationIdentity>) -> Self {
        self.identity = Some(identity);
        self
    }
    pub(crate) fn with_tree(mut self, tree: Option<crate::proc::OwnedTree>) -> Self {
        self.tree = tree;
        self
    }
    /// The real owned container; several shared-server sessions return the same tree.
    pub fn owned_tree(&self) -> Option<crate::proc::OwnedTree> {
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
        self.answer_with(request_id, allow, None)
    }
    /// Like `answer`, carrying the user's choices for a question the agent asked.
    pub fn answer_with(&self, request_id: &str, allow: bool, reply: Option<serde_json::Value>) -> tokio::sync::oneshot::Receiver<Delivery> {
        let (ack, delivered) = tokio::sync::oneshot::channel();
        (self.send)(SessionCommand::Answer { request_id: request_id.to_string(), allow, reply, ack });
        delivered
    }
    pub fn helper_reply(&self, id: &str, ok: bool, text: String) {
        self.helper_reply_with_pictures(id, ok, text, Vec::new());
    }
    /// Like `helper_reply`, with JPEG pictures (base64) the agent sees after the text.
    pub fn helper_reply_with_pictures(&self, id: &str, ok: bool, text: String, pictures: Vec<String>) {
        (self.send)(SessionCommand::HelperReply { id: id.to_string(), ok, text, pictures });
    }
    pub fn compact(&self) {
        (self.send)(SessionCommand::Compact);
    }
    pub fn context_usage(&self) {
        (self.send)(SessionCommand::ContextUsage);
    }
    pub fn interrupt(&self) {
        (self.send)(SessionCommand::Interrupt);
    }
    pub fn close(&self) {
        if let Some(identity) = &self.identity {
            identity.revoke();
        }
        (self.send)(SessionCommand::Close);
    }
    pub fn retire(&self) {
        (self.send)(SessionCommand::Retire);
    }
}
