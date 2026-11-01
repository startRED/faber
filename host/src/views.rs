//! Conversations as an AI or a script reads them through the command line and the AI's tools: one row each with its state
//! in plain words, and one conversation's messages without what only the screen draws. The screen keeps its own `state`
//! and `open`; these are built from the same engine data.

use faber_engine::AppState;
use faber_engine::model::{Conversation, Message, Role, RunState, Status, Summary};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// Where a conversation stands, in the words the command line and the AI's tools use.
///
/// - `working`: its AI is answering now; `waiting`: it waits for the person (a question, an approval).
/// - `concluded`: its last reply ended with the AI's own conclusion of the whole request (the closing box, marked done).
/// - `ended`: its last reply ended without such a conclusion.
/// - `interrupted`: Faber was closed under its last reply; `stopped`: someone stopped it; `failed`: it ended on an error.
/// - `new`: no reply yet. `idle`: not running, and only the conversation itself says how its last reply ended (a list
///   leaves the files of conversations that are not running unread).
pub const STATES: &[&str] = &["working", "waiting", "concluded", "ended", "interrupted", "stopped", "failed", "new", "idle"];

/// The state a running conversation is in, or that its last reply left it in.
pub fn state(running: Option<RunState>, last_reply: Option<&Message>) -> &'static str {
    match running {
        Some(RunState::Working) => return "working",
        Some(RunState::NeedsYou) => return "waiting",
        None => {}
    }
    let Some(reply) = last_reply else { return "new" };
    if reply.closed {
        return "interrupted";
    }
    match reply.status {
        Some(Status::Stopped) => "stopped",
        Some(Status::Error) => "failed",
        _ if reply.error.is_some() => "failed",
        _ if reply.wrap_up.as_ref().is_some_and(|w| w.done) => "concluded",
        _ => "ended",
    }
}

/// What a conversation that called `mark_waiting` waits for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingOn {
    pub ids: Vec<String>,
    pub note: String,
}

impl From<&faber_engine::model::WaitingOn> for WaitingOn {
    fn from(w: &faber_engine::model::WaitingOn) -> Self {
        Self { ids: w.ids.clone(), note: w.note.clone() }
    }
}

/// One conversation in a list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub title: String,
    pub area: faber_engine::model::Area,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// The conversation whose AI started this one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_by: Option<String>,
    /// The first conversation of its family: an AI acts freely within its own.
    pub family: String,
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_on: Option<WaitingOn>,
    /// How its last run ended, while the person has not opened it since (`done`, `needsYou`, `failed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended: Option<Value>,
    pub archived: bool,
    pub pinned: bool,
    pub keep_going: bool,
    /// Milliseconds since the Unix epoch.
    pub updated_at: u64,
}

/// What a list may be narrowed to.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Only these states (see [`STATES`]); `running` means working or waiting.
    pub states: Vec<String>,
    /// None: only the ones not archived; Some(true): only archived; Some(false): every one.
    pub archived: Option<bool>,
    pub project_id: Option<String>,
    /// Only the family of this conversation.
    pub family: Option<String>,
    /// Words the title holds, without case.
    pub query: Option<String>,
    pub limit: Option<usize>,
}

/// The families of every conversation, by who started whom.
fn families(summaries: &[Summary]) -> impl Fn(&str) -> Option<String> + '_ {
    let starters: HashMap<&str, Option<&str>> = summaries.iter().map(|s| (s.id.as_str(), s.started_by.as_deref())).collect();
    move |id| crate::reach::root(id, |at| starters.get(at).map(|s| s.map(String::from)))
}

/// The conversations, running ones first, then pinned, then the newest; a list reads no conversation's file. One that called
/// `mark_waiting` reads as `waiting` while nothing runs in it.
pub fn rows(state: &AppState, filter: &Filter) -> Vec<Row> {
    let running: HashMap<&str, RunState> = state.running.items.iter().map(|i| (i.conversation_id.as_str(), i.state)).collect();
    let family = families(&state.conversations);
    let wanted_family = filter.family.as_deref().and_then(&family);
    let query = filter.query.as_deref().map(str::to_lowercase);
    let mut rows: Vec<Row> = state
        .conversations
        .iter()
        .filter(|s| match filter.archived {
            None => !s.archived,
            Some(only) => !only || s.archived,
        })
        .filter(|s| filter.project_id.as_deref().is_none_or(|p| s.project_id.as_deref() == Some(p)))
        .filter(|s| query.as_deref().is_none_or(|q| s.title.to_lowercase().contains(q)))
        .map(|s| {
            let waiting_on = s.waiting_on.as_ref().map(WaitingOn::from);
            let state = match running.get(s.id.as_str()) {
                Some(RunState::Working) => "working",
                Some(RunState::NeedsYou) => "waiting",
                None if waiting_on.is_some() => "waiting",
                None => "idle",
            };
            Row {
                id: s.id.clone(),
                title: s.title.clone(),
                area: s.area,
                project_id: s.project_id.clone(),
                started_by: s.started_by.clone(),
                family: family(&s.id).unwrap_or_else(|| s.id.clone()),
                state,
                waiting_on,
                ended: s.ended.as_ref().and_then(|e| serde_json::to_value(e.kind).ok()),
                archived: s.archived,
                pinned: s.pinned_at.is_some(),
                keep_going: s.keep_going,
                updated_at: s.updated_at,
            }
        })
        .filter(|r| wanted_family.as_deref().is_none_or(|f| r.family == f))
        .filter(|r| {
            filter.states.is_empty()
                || filter.states.iter().any(|s| s == r.state || (s == "running" && matches!(r.state, "working" | "waiting")))
        })
        .collect();
    rows.sort_by_key(|r| (r.state == "idle", !r.pinned, std::cmp::Reverse(r.updated_at)));
    if let Some(limit) = filter.limit {
        rows.truncate(limit);
    }
    rows
}

/// A message without what only the screen draws.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Said {
    pub id: String,
    pub role: Role,
    pub text: String,
    pub at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<faber_engine::model::Vendor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Each tool the AI used in it, by name, and whether it worked.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

/// The AI's own conclusion, from the closing box of its latest reply that has one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conclusion {
    pub message_id: String,
    pub at: u64,
    /// What is now true, then what is not done or not verified.
    pub recap: String,
    /// The whole request is finished and verified.
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
}

/// One conversation as an AI reads it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Read {
    pub id: String,
    pub title: String,
    pub state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_on: Option<WaitingOn>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// How many messages it has; `messages` holds the last ones asked for.
    pub count: usize,
    pub messages: Vec<Said>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<Conclusion>,
    /// The reply Faber was closed under, which `continue_closed` goes on with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupted_turn: Option<String>,
}

pub fn read(conversation: &Conversation, running: Option<RunState>, last: Option<usize>) -> Read {
    let messages = &conversation.messages;
    let last_reply = messages.last().filter(|m| m.role == Role::Assistant);
    let from = last.map_or(0, |n| messages.len().saturating_sub(n));
    let conclusion = messages.iter().rev().filter(|m| m.role == Role::Assistant).find_map(|m| {
        m.wrap_up.as_ref().map(|w| Conclusion {
            message_id: m.id.clone(),
            at: m.at,
            recap: w.recap.clone(),
            done: w.done,
            ask: w.ask.clone(),
        })
    });
    let waiting_on = conversation.waiting_on.as_ref().map(WaitingOn::from);
    let state = match running {
        None if waiting_on.is_some() => "waiting",
        _ => state(running, last_reply),
    };
    Read {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        state,
        waiting_on,
        project_id: conversation.project_id.clone(),
        count: messages.len(),
        messages: messages[from..].iter().map(said).collect(),
        conclusion,
        interrupted_turn: last_reply.filter(|m| m.closed && running.is_none()).map(|m| m.id.clone()),
    }
}

fn said(m: &Message) -> Said {
    Said {
        id: m.id.clone(),
        role: m.role,
        text: m.text.clone(),
        at: m.at,
        vendor: m.vendor,
        model: m.model.clone(),
        status: m.status,
        error: m.error.clone(),
        tools: m.tools.iter().flatten().map(|t| Step { name: t.name.clone(), ok: t.ok }).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_engine::model::WrapUp;

    fn reply(status: Status) -> Message {
        let mut m = Message::new("r1".into(), Role::Assistant, "Here.".into(), 1);
        m.status = Some(status);
        m
    }

    #[test]
    fn a_reply_is_concluded_only_by_the_ais_own_closing_box_marked_done() {
        assert_eq!(state(Some(RunState::Working), None), "working");
        assert_eq!(state(Some(RunState::NeedsYou), Some(&reply(Status::Done))), "waiting");
        assert_eq!(state(None, None), "new");
        assert_eq!(state(None, Some(&reply(Status::Done))), "ended", "no conclusion");
        let mut concluded = reply(Status::Done);
        concluded.wrap_up = Some(WrapUp { recap: "All done.".into(), ask: None, actions: vec![], done: true, answered: None });
        assert_eq!(state(None, Some(&concluded)), "concluded");
        concluded.wrap_up.as_mut().unwrap().done = false;
        assert_eq!(state(None, Some(&concluded)), "ended", "a box that is not done concludes nothing");
        assert_eq!(state(None, Some(&reply(Status::Stopped))), "stopped");
        assert_eq!(state(None, Some(&reply(Status::Error))), "failed");
        let mut closed = reply(Status::Stopped);
        closed.closed = true;
        assert_eq!(state(None, Some(&closed)), "interrupted");
        for word in ["working", "waiting", "concluded", "ended", "interrupted", "stopped", "failed", "new", "idle"] {
            assert!(STATES.contains(&word));
        }
    }
}
