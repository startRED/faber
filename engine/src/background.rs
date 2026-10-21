//! Ephemeral command rows. They never enter a conversation's saved messages.
use crate::events::Session;
use crate::model::Vendor;
use serde::Serialize;

pub const MAX_COMMANDS: usize = 32;
pub const MAX_LINES: usize = 20;
pub const MAX_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Running,
    Done,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Start { id: String, command: String, stoppable: bool, turn_ended: bool },
    Output { id: String, text: String, replace: bool },
    End { id: String, state: State },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub id: String,
    pub conversation_id: String,
    pub vendor: Vendor,
    pub command: String,
    pub started: u64,
    pub output: String,
    pub state: State,
    pub stoppable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct List {
    pub revision: u64,
    pub items: Vec<Command>,
}

struct Entry {
    row: Command,
    native: String,
    sid: u64,
    session: Session,
    turn_ended: bool,
}

#[derive(Default)]
pub(crate) struct Commands {
    revision: u64,
    entries: Vec<Entry>,
}

/// A UTF-8 tail, bounded before allocating even when a delta is very large. A trailing newline belongs to its last line.
pub fn tail(text: &str) -> &str {
    let mut at = text.len().saturating_sub(MAX_BYTES);
    while !text.is_char_boundary(at) {
        at += 1;
    }
    let text = &text[at..];
    let end = text.strip_suffix('\n').unwrap_or(text);
    let at = end.match_indices('\n').rev().nth(MAX_LINES - 1).map_or(0, |(at, _)| at + 1);
    &text[at..]
}

impl Commands {
    pub fn list(&self) -> List {
        List { revision: self.revision, items: self.entries.iter().map(|e| e.row.clone()).collect() }
    }

    pub fn running_in(&self, conv: &str) -> bool {
        self.entries.iter().any(|e| e.row.conversation_id == conv && e.row.state == State::Running)
    }

    pub fn update(&mut self, conv: &str, sid: u64, vendor: Vendor, session: &Session, update: Update, now: u64) -> bool {
        let id = match &update {
            Update::Start { id, .. } | Update::Output { id, .. } | Update::End { id, .. } => id,
        };
        let at = self.entries.iter().position(|e| e.row.conversation_id == conv && e.sid == sid && e.native == *id);
        match (at, update) {
            (None, Update::Start { id, command, stoppable, turn_ended }) => {
                if self.entries.iter().filter(|e| e.row.conversation_id == conv).count() >= MAX_COMMANDS {
                    let Some(oldest) = self.entries.iter().position(|e| e.row.conversation_id == conv && e.row.state != State::Running)
                    else {
                        return false;
                    };
                    self.entries.remove(oldest);
                }
                self.entries.push(Entry {
                    row: Command {
                        id: uuid::Uuid::new_v4().to_string(),
                        conversation_id: conv.into(),
                        vendor,
                        command,
                        started: now,
                        output: String::new(),
                        state: State::Running,
                        stoppable,
                    },
                    native: id,
                    sid,
                    session: session.clone(),
                    turn_ended,
                });
            }
            (Some(at), Update::Start { stoppable, turn_ended, .. }) => {
                self.entries[at].turn_ended |= turn_ended;
                if self.entries[at].row.stoppable == stoppable {
                    return false;
                }
                self.entries[at].row.stoppable = stoppable;
            }
            (Some(at), Update::Output { text, replace, .. }) => {
                let row = &mut self.entries[at].row;
                let output = if replace {
                    tail(&text).to_string()
                } else {
                    let mut joined = String::with_capacity(row.output.len() + text.len().min(MAX_BYTES));
                    let kept = tail(&text);
                    if kept.len() == text.len() {
                        joined.push_str(&row.output);
                    }
                    joined.push_str(kept);
                    tail(&joined).to_string()
                };
                if row.output == output {
                    return false;
                }
                row.output = output;
            }
            (Some(at), Update::End { state, .. }) => {
                if self.entries[at].turn_ended {
                    self.entries.remove(at);
                } else {
                    if self.entries[at].row.state == state {
                        return false;
                    }
                    self.entries[at].row.state = state;
                }
            }
            _ => return false,
        }
        self.revision += 1;
        true
    }

    pub fn session(&self, conv: &str, sid: u64) -> Option<(Vendor, Session)> {
        self.entries.iter().find(|e| e.row.conversation_id == conv && e.sid == sid).map(|e| (e.row.vendor, e.session.clone()))
    }

    pub fn owner(&self, sid: u64) -> Option<(String, Vendor, Session)> {
        self.entries.iter().find(|e| e.sid == sid).map(|e| (e.row.conversation_id.clone(), e.row.vendor, e.session.clone()))
    }

    pub fn stop(&self, id: &str) -> Option<(Session, String)> {
        let e = self.entries.iter().find(|e| e.row.id == id && e.row.state == State::Running && e.row.stoppable)?;
        Some((e.session.clone(), e.native.clone()))
    }

    pub fn end_turn(&mut self, conv: &str, sid: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain_mut(|e| {
            if e.row.conversation_id != conv || e.sid != sid {
                return true;
            }
            e.turn_ended = true;
            e.row.state == State::Running
        });
        let changed = before != self.entries.len();
        if changed {
            self.revision += 1;
        }
        changed
    }

    pub fn exited(&mut self, conv: &str, sid: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.row.conversation_id != conv || e.sid != sid);
        let changed = before != self.entries.len();
        if changed {
            self.revision += 1;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        Session::from_fn(|_| {})
    }

    fn start(commands: &mut Commands, conv: &str, sid: u64, id: &str) -> bool {
        commands.update(
            conv,
            sid,
            Vendor::Codex,
            &session(),
            Update::Start { id: id.into(), command: format!("run {id}"), stoppable: true, turn_ended: false },
            123,
        )
    }

    fn output(commands: &mut Commands, id: &str, text: &str, replace: bool) {
        commands.update("one", 1, Vendor::Codex, &session(), Update::Output { id: id.into(), text: text.into(), replace }, 124);
    }

    #[test]
    fn background_tail_keeps_twenty_lines_eight_kib_and_valid_unicode() {
        let lines: String = (0..30).map(|i| format!("line {i}\n")).collect();
        assert_eq!(tail(&lines), (10..30).map(|i| format!("line {i}\n")).collect::<String>());
        let no_newline = lines.trim_end_matches('\n');
        assert_eq!(tail(no_newline), (10..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n"));
        let wide = "界".repeat(MAX_BYTES);
        let kept = tail(&wide);
        assert!(kept.len() <= MAX_BYTES);
        assert!(kept.len() > MAX_BYTES - 3);
        assert_eq!(kept.chars().count() * 3, kept.len());
        assert_eq!(tail(""), "");
    }

    #[test]
    fn background_output_handles_chunked_newlines_replacement_and_huge_deltas() {
        let mut commands = Commands::default();
        start(&mut commands, "one", 1, "a");
        output(&mut commands, "a", "first", false);
        output(&mut commands, "a", " line\nsec", false);
        output(&mut commands, "a", "ond\n", false);
        assert_eq!(commands.list().items[0].output, "first line\nsecond\n");
        output(&mut commands, "a", "replacement\n", true);
        assert_eq!(commands.list().items[0].output, "replacement\n");
        output(&mut commands, "a", &"🦀".repeat(MAX_BYTES), false);
        assert_eq!(commands.list().items[0].output.len(), MAX_BYTES);
        output(&mut commands, "a", "\nlast", false);
        let list = commands.list();
        assert!(list.items[0].output.len() <= MAX_BYTES);
        assert!(list.items[0].output.ends_with("\nlast"));
        output(&mut commands, "a", &(0..40).map(|i| format!("{i}\n")).collect::<String>(), false);
        assert_eq!(commands.list().items[0].output, (20..40).map(|i| format!("{i}\n")).collect::<String>());
    }

    #[test]
    fn background_limit_is_per_conversation_and_evicts_oldest_finished_first() {
        let mut commands = Commands::default();
        for i in 0..MAX_COMMANDS {
            assert!(start(&mut commands, "one", 1, &i.to_string()));
        }
        assert!(!start(&mut commands, "one", 1, "overflow"));
        assert!(start(&mut commands, "two", 1, "other"));
        for id in ["7", "3"] {
            assert!(commands.update("one", 1, Vendor::Codex, &session(), Update::End { id: id.into(), state: State::Done }, 125));
        }
        assert!(start(&mut commands, "one", 1, "replacement"));
        let list = commands.list();
        assert_eq!(list.items.iter().filter(|row| row.conversation_id == "one").count(), MAX_COMMANDS);
        assert!(!list.items.iter().any(|row| row.command == "run 3"));
        assert!(list.items.iter().any(|row| row.command == "run 7"));
        assert!(list.items.iter().any(|row| row.command == "run 0"));
        assert_eq!(list.items.last().unwrap().command, "run replacement");
    }

    #[test]
    fn background_session_ids_are_isolated_and_finished_rows_leave_at_turn_end() {
        let mut commands = Commands::default();
        for (conv, sid) in [("one", 1), ("one", 2), ("two", 1)] {
            start(&mut commands, conv, sid, "same");
        }
        output(&mut commands, "same", "only first", false);
        let list = commands.list();
        assert_eq!(list.items.iter().filter(|row| row.output == "only first").count(), 1);
        assert_ne!(list.items[0].id, list.items[1].id);
        assert_eq!(list.items[0].started, 123);
        assert!(commands.stop(&list.items[0].id).is_some());
        assert!(commands.update("one", 1, Vendor::Codex, &session(), Update::End { id: "same".into(), state: State::Stopped }, 125));
        assert!(commands.stop(&list.items[0].id).is_none());
        assert!(commands.end_turn("one", 1));
        assert_eq!(commands.list().items.len(), 2);
        assert!(!commands.end_turn("one", 2), "a still-running command survives its turn");
        assert!(commands.update("one", 2, Vendor::Codex, &session(), Update::End { id: "same".into(), state: State::Done }, 126));
        assert_eq!(commands.list().items.len(), 1, "a command ending after its turn leaves immediately");
        assert!(!commands.exited("one", 1));
        assert!(commands.exited("two", 1));
        assert!(commands.list().items.is_empty());
        assert!(Commands::default().list().items.is_empty(), "nothing survives a new engine's in-memory command list");
    }

    #[test]
    fn background_late_start_keeps_its_original_turn_end_when_another_turn_runs() {
        let mut commands = Commands::default();
        assert!(commands.update(
            "one",
            1,
            Vendor::Codex,
            &session(),
            Update::Start { id: "late".into(), command: "run late".into(), stoppable: true, turn_ended: true },
            123
        ));
        assert_eq!(commands.list().items[0].state, State::Running);
        assert!(!start(&mut commands, "one", 1, "late"), "a repeated start cannot reopen the original turn");
        assert!(start(&mut commands, "one", 1, "next-turn"));
        assert!(commands.update("one", 1, Vendor::Codex, &session(), Update::End { id: "late".into(), state: State::Done }, 124));
        assert_eq!(commands.list().items.len(), 1);
        assert_eq!(commands.list().items[0].command, "run next-turn");
        assert!(commands.update("one", 1, Vendor::Codex, &session(), Update::End { id: "next-turn".into(), state: State::Done }, 125));
        assert_eq!(commands.list().items.len(), 1, "the next turn's finished row waits for that turn's end");
        assert!(commands.end_turn("one", 1));
        assert!(commands.list().items.is_empty());
    }
}
