//! What a conversation carries to the other vendor when its own vendor cannot answer: the earlier messages as plain text, or
//! a summary of them when they are too long, and, when a failed turn had already done work, a request to continue it.

use crate::model::{Message, Role};

/// Up to this many characters of earlier messages go over whole.
pub const FULL_LIMIT: usize = 24_000;
/// A tool result is cut to this many characters.
pub const TOOL_CUT: usize = 200;
/// A summary is written from the first message and this many characters of the end.
pub const SUMMARY_TAIL: usize = 40_000;
/// The summary's length, in words.
pub const SUMMARY_WORDS: usize = 600;

/// What the failed turn had done before the vendor stopped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Partial {
    pub text: String,
    /// (tool name, result) of each tool that ran.
    pub tools: Vec<(String, String)>,
}

impl Partial {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.tools.is_empty()
    }

    /// The partial answer of a reply that did not finish.
    pub fn of(reply: &Message) -> Partial {
        let tools =
            reply.tools.iter().flatten().map(|t| (t.name.clone(), cut(t.output.as_deref().unwrap_or_default(), TOOL_CUT))).collect();
        Partial { text: reply.text.clone(), tools }
    }
}

fn cut(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let kept: String = text.chars().take(limit.saturating_sub(3)).collect();
    format!("{kept}...")
}

/// Every message as `User: ...` / `Assistant: ...`, tool results shortened.
pub fn transcript(history: &[Message]) -> String {
    history
        .iter()
        .map(|m| {
            let who = if m.role == Role::User { "User" } else { "Assistant" };
            let tools: String = m
                .tools
                .iter()
                .flatten()
                .map(|t| format!("\n[{} -> {}]", t.name, cut(t.output.as_deref().unwrap_or_default(), TOOL_CUT)))
                .collect();
            format!("{who}: {}{tools}", m.text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// How the other vendor learns what came before.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// The whole transcript fits: the text to send as the first message on the new vendor.
    Full(String),
    /// Too long: the prompt that asks the other vendor's cheap model for a summary.
    Summarise(String),
}

pub fn plan(history: &[Message]) -> Plan {
    let all = transcript(history);
    if all.chars().count() <= FULL_LIMIT {
        return Plan::Full(all);
    }
    let first = history.first().map(|m| transcript(std::slice::from_ref(m))).unwrap_or_default();
    let tail: String = all.chars().skip(all.chars().count().saturating_sub(SUMMARY_TAIL)).collect();
    Plan::Summarise(format!("{first}\n\n[...]\n\n{tail}"))
}

pub fn with_full(history: &str, text: &str, done: &Partial) -> String {
    with(&format!("Earlier in this conversation, with another assistant:\n\n{history}"), text, done)
}

/// The first message on the new vendor once the summary is written.
pub fn with_summary(summary: &str, text: &str, done: &Partial) -> String {
    with(&format!("Summary of this conversation so far, written by another assistant:\n\n{}", summary.trim()), text, done)
}

fn with(before: &str, text: &str, done: &Partial) -> String {
    if done.is_empty() {
        return format!("{before}\n\nNow the user says:\n{text}");
    }
    let tools: String = done.tools.iter().map(|(name, out)| format!("\n- {name}: {out}")).collect();
    format!(
        "{before}\n\nThe user then asked:\n{text}\n\nThe other assistant had started on it and was cut off. Its partial answer:\n{}\n\n\
         Tool actions it started (verify effects before repeating any action):{}\n\nContinue from there.",
        done.text.trim(),
        if tools.is_empty() { "\n- none".to_string() } else { tools }
    )
}
