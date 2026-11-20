//! Conversations as an AI or a script reads them through the command line and the AI's tools: one row each with its state
//! in plain words, and one conversation's messages without what only the screen draws. The screen keeps its own `state`
//! and `open`; these are built from the same engine data.

use faber_engine::conversation_state::ReplyState;
use faber_engine::history::{Page, ReplyEnd};
use faber_engine::model::{Message, Role, RunState, Status, Summary};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// Where a conversation stands, in the words the command line and the AI's tools use.
///
/// - `working`: its AI is answering now; `waiting`: it waits for the person (a question, an approval).
/// - `concluded`: its last reply ended with the AI's own conclusion of the whole request (the closing box, marked done).
/// - `ended`: its last reply ended without such a conclusion.
/// - `interrupted`: Faber was closed under its last reply; `stopped`: someone stopped it; `failed`: it ended on an error.
/// - `new`: no reply to the latest message. `idle`: not running, with no exact ending selected by the list filter.
pub const STATES: &[&str] = &["working", "waiting", "concluded", "ended", "interrupted", "stopped", "failed", "new", "idle"];

#[derive(serde::Deserialize)]
#[serde(try_from = "String")]
pub struct ListState(String);

impl TryFrom<String> for ListState {
    type Error = &'static str;

    fn try_from(word: String) -> Result<Self, Self::Error> {
        if word == "running" || STATES.contains(&word.as_str()) { Ok(Self(word)) } else { Err("unknown conversation state") }
    }
}

impl From<ListState> for String {
    fn from(state: ListState) -> Self {
        state.0
    }
}

fn activity(running: Option<RunState>, waiting: bool, last_reply: Option<ReplyState>) -> Option<&'static str> {
    match running {
        Some(RunState::Working) => Some("working"),
        Some(RunState::NeedsYou) => Some("waiting"),
        None if waiting && !matches!(last_reply, Some(ReplyState::Failed | ReplyState::Stopped | ReplyState::Interrupted)) => {
            Some("waiting")
        }
        None => None,
    }
}

/// The state a running conversation is in, or that its last reply left it in.
fn state(running: Option<RunState>, waiting: bool, last_reply: Option<&ReplyEnd>) -> &'static str {
    let reply = last_reply.map(|r| ReplyState::from_reply(r.status, r.closed, r.error, r.done));
    activity(running, waiting, reply).unwrap_or_else(|| reply.unwrap_or(ReplyState::New).word())
}

/// What a conversation that called `mark_waiting` waits for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingOn {
    pub ids: Vec<String>,
    pub note: String,
    pub since: u64,
    pub until: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

impl From<&faber_engine::model::WaitingOn> for WaitingOn {
    fn from(w: &faber_engine::model::WaitingOn) -> Self {
        Self { ids: w.ids.clone(), note: w.note.clone(), since: w.since, until: w.until, problem: w.problem.clone() }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_choice: Option<faber_engine::router::Choice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_line: Option<String>,
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

/// The conversations, running ones first, then pinned, then the newest.
pub fn rows(summaries: &[Summary], running: &[(String, RunState)], filter: &Filter) -> Vec<Row> {
    let running: HashMap<&str, RunState> = running.iter().map(|(id, state)| (id.as_str(), *state)).collect();
    let family = families(summaries);
    let wanted_family = match filter.family.as_deref() {
        Some(id) => {
            let Some(root) = family(id) else { return Vec::new() };
            Some(root)
        }
        None => None,
    };
    let query = filter.query.as_deref().map(str::to_lowercase);
    let mut rows: Vec<Row> = summaries
        .iter()
        .filter(|s| match filter.archived {
            None => !s.archived,
            Some(only) => !only || s.archived,
        })
        .filter(|s| filter.project_id.as_deref().is_none_or(|p| s.project_id.as_deref() == Some(p)))
        .filter(|s| query.as_deref().is_none_or(|q| s.title.to_lowercase().contains(q)))
        .map(|s| {
            let waiting_on = s.waiting_on.as_ref().map(WaitingOn::from);
            let state = activity(running.get(s.id.as_str()).copied(), waiting_on.is_some(), s.reply_state)
                .or_else(|| s.reply_state.map(ReplyState::word).filter(|word| filter.states.iter().any(|s| s == word)))
                .unwrap_or("idle");
            Row {
                id: s.id.clone(),
                title: s.title.clone(),
                area: s.area,
                project_id: s.project_id.clone(),
                started_by: s.started_by.clone(),
                family: family(&s.id).unwrap_or_else(|| s.id.clone()),
                state,
                waiting_on,
                child_choice: s.child_choice.clone(),
                child_line: s.child_line.clone(),
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
    rows.sort_by_key(|r| (!matches!(r.state, "working" | "waiting"), !r.pinned, std::cmp::Reverse(r.updated_at)));
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
    /// Each tool's recorded result, including the command's exit code when known.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Step>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub declined: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
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
    /// How many messages it has; `messages` holds the selected ones.
    pub count: usize,
    /// Number of the first message returned by a numbered read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<usize>,
    pub messages: Vec<Said>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<Conclusion>,
    /// The reply Faber was closed under, which `continue_closed` goes on with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupted_turn: Option<String>,
}

pub fn read(page: Page<'_>, active: Option<RunState>, in_panel: bool, first: Option<usize>) -> Read {
    let conclusion = page.conclusion.map(|c| Conclusion {
        message_id: c.id,
        at: c.at,
        recap: c.closing.recap,
        done: c.closing.done,
        ask: c.closing.ask,
    });
    let waiting_on = page.waiting_on.as_ref().map(WaitingOn::from);
    let state = state(active, waiting_on.is_some(), page.last_reply.as_ref());
    Read {
        id: page.id,
        title: page.title,
        state,
        waiting_on,
        project_id: page.project_id,
        count: page.count,
        first: first.filter(|_| !page.messages.is_empty()),
        messages: page.messages.iter().map(said).collect(),
        conclusion,
        interrupted_turn: page.last_reply.filter(|m| m.closed && !in_panel).map(|m| m.id),
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
        tools: m
            .tools
            .iter()
            .flatten()
            .map(|t| Step { name: t.name.clone(), ok: t.ok, exit_code: t.exit_code, declined: t.declined, interrupted: t.interrupted })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(status: Status) -> ReplyEnd {
        ReplyEnd { id: "r1".into(), status: Some(status), closed: false, error: false, done: false }
    }

    #[test]
    fn a_reply_is_concluded_only_by_the_ais_own_closing_box_marked_done() {
        assert_eq!(state(Some(RunState::Working), false, None), "working");
        assert_eq!(state(Some(RunState::NeedsYou), false, Some(&reply(Status::Done))), "waiting");
        assert_eq!(state(None, false, None), "new");
        assert_eq!(state(None, false, Some(&reply(Status::Done))), "ended", "no conclusion");
        assert_eq!(state(None, true, Some(&reply(Status::Done))), "waiting", "a saved wait still stands");
        let mut concluded = reply(Status::Done);
        concluded.done = true;
        assert_eq!(state(None, false, Some(&concluded)), "concluded");
        concluded.done = false;
        assert_eq!(state(None, false, Some(&concluded)), "ended", "a box that is not done concludes nothing");
        assert_eq!(state(None, false, Some(&reply(Status::Stopped))), "stopped");
        assert_eq!(state(None, false, Some(&reply(Status::Error))), "failed");
        let mut closed = reply(Status::Stopped);
        closed.closed = true;
        assert_eq!(state(None, false, Some(&closed)), "interrupted");
        for word in ["working", "waiting", "concluded", "ended", "interrupted", "stopped", "failed", "new", "idle"] {
            assert!(STATES.contains(&word));
        }
    }

    #[test]
    fn an_exact_filter_yields_to_live_activity_and_ignores_rows_without_an_ending() {
        let mut summary: Summary = serde_json::from_value(serde_json::json!({
            "id":"test", "title":"Test", "area":"chat", "updatedAt":1, "replyState":"failed"
        }))
        .unwrap();
        let failed = Filter { states: vec!["failed".into()], ..Filter::default() };
        let running = [("test".into(), RunState::Working)];
        assert!(rows(std::slice::from_ref(&summary), &running, &failed).is_empty());
        assert_eq!(rows(std::slice::from_ref(&summary), &running, &Filter::default())[0].state, "working");
        summary.reply_state = None;
        assert!(rows(std::slice::from_ref(&summary), &[], &failed).is_empty());
        assert_eq!(rows(&[summary], &[], &Filter::default())[0].state, "idle");
    }
}
