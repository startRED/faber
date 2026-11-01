//! Short-lived requests and one-use grants for a named agent conversation.

use serde::Serialize;
use serde_json::Value;
use std::{collections::VecDeque, time::Duration};
use tokio::time::Instant;

const LIMIT: usize = 20;
const LIFE: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum State {
    Pending,
    Allowed,
    Denied,
    Expired,
    Spent,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct View {
    id: String,
    conversation: String,
    label: &'static str,
    state: State,
}

struct Entry {
    view: View,
    command: String,
    args: Value,
    until: Instant,
}

#[derive(Default)]
pub(crate) struct Book {
    entries: VecDeque<Entry>,
}

impl Book {
    fn expire(&mut self) {
        let now = Instant::now();
        for entry in &mut self.entries {
            if matches!(entry.view.state, State::Pending | State::Allowed) && now >= entry.until {
                entry.view.state = State::Expired;
                entry.args = Value::Null;
            }
        }
    }

    pub(crate) fn record(&mut self, conversation: &str, command: &str, args: Value, label: &'static str) -> String {
        self.expire();
        if self.entries.len() == LIMIT {
            self.entries.pop_front();
        }
        let id = loop {
            let candidate = faber_engine::model::new_epoch()[..8].to_string();
            if self.entries.iter().all(|entry| entry.view.id != candidate) {
                break candidate;
            }
        };
        self.entries.push_back(Entry {
            view: View { id: id.clone(), conversation: conversation.into(), label, state: State::Pending },
            command: command.into(),
            args,
            until: Instant::now() + LIFE,
        });
        id
    }

    pub(crate) fn answer(&mut self, id: &str, allow: bool) -> bool {
        self.expire();
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.view.id == id && entry.view.state == State::Pending) else {
            return false;
        };
        entry.view.state = if allow { State::Allowed } else { State::Denied };
        entry.until = Instant::now() + LIFE;
        if !allow {
            entry.args = Value::Null;
        }
        true
    }

    pub(crate) fn spend(&mut self, conversation: &str, command: &str, args: &Value) -> bool {
        self.expire();
        let Some(entry) = self.entries.iter_mut().find(|entry| {
            entry.view.state == State::Allowed && entry.view.conversation == conversation && entry.command == command && entry.args == *args
        }) else {
            return false;
        };
        entry.view.state = State::Spent;
        entry.args = Value::Null;
        true
    }

    pub(crate) fn pending(&mut self) -> Vec<View> {
        self.expire();
        self.entries.iter().filter(|entry| entry.view.state == State::Pending).map(|entry| entry.view.clone()).collect()
    }

    pub(crate) fn for_conversation(&mut self, conversation: &str) -> Vec<View> {
        self.expire();
        self.entries
            .iter()
            .filter(|entry| entry.view.conversation == conversation && entry.view.state != State::Spent)
            .map(|entry| entry.view.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test(start_paused = true)]
    async fn grants_are_exact_single_use_and_expire() {
        let mut book = Book::default();
        let args = json!({"id": "other"});
        let id = book.record("one", "interrupt", args.clone(), "organize");
        assert!(book.answer(&id, true));
        assert!(!book.spend("two", "interrupt", &args));
        assert!(!book.spend("one", "send", &args));
        assert!(!book.spend("one", "interrupt", &json!({"id":"else"})));
        assert!(book.spend("one", "interrupt", &args));
        assert!(!book.spend("one", "interrupt", &args));
        let id = book.record("one", "interrupt", args.clone(), "organize");
        assert!(book.answer(&id, true));
        tokio::time::advance(LIFE).await;
        assert!(!book.spend("one", "interrupt", &args));
        assert_eq!(book.for_conversation("one")[0].state, State::Expired);
    }

    #[tokio::test(start_paused = true)]
    async fn cap_deny_and_pending_expiry() {
        let mut book = Book::default();
        let first = book.record("one", "a", json!({}), "organize");
        for _ in 0..LIMIT {
            book.record("one", "a", json!({}), "organize");
        }
        assert_eq!(book.pending().len(), LIMIT);
        assert!(!book.answer(&first, true));
        let id = book.pending()[0].id.clone();
        assert!(book.answer(&id, false));
        assert!(!book.answer(&id, true));
        assert!(!book.spend("one", "a", &json!({})));
        tokio::time::advance(LIFE).await;
        assert!(book.pending().is_empty());
        assert_eq!(book.for_conversation("one")[0].state, State::Denied);
        assert_eq!(book.for_conversation("one")[1].state, State::Expired);
    }
}
