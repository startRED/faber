//! Ephemeral command rows. They never enter a conversation's saved messages.
use crate::events::Session;
use crate::model::Vendor;
use serde::Serialize;
use std::collections::VecDeque;

pub const MAX_COMMANDS: usize = 32;
pub const MAX_LINES: usize = 20;
pub const MAX_BYTES: usize = 8192;
const FINISHED_LIMIT: usize = 32;
const LABEL_BYTES: usize = 512;
const ID_BYTES: usize = 256;

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
    Output { id: String, text: String, capture: Capture, source_truncated: bool },
    End { id: String, state: State },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capture {
    Delta,
    Snapshot,
    Terminal,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecord {
    pub id: String,
    pub conversation_id: String,
    pub vendor: Vendor,
    pub command: Option<String>,
    pub started: u64,
    pub ended: Option<u64>,
    pub state: Option<State>,
    pub output: String,
    pub output_truncated: bool,
    pub terminal_output: bool,
}

struct Entry {
    row: Command,
    native: String,
    sid: u64,
    session: Session,
    turn_ended: bool,
    output_truncated: bool,
    terminal_output: bool,
    retain_result: bool,
}

struct FinishedEntry {
    row: CommandRecord,
    native: String,
    sid: u64,
}

#[derive(Default)]
pub struct Commands {
    revision: u64,
    entries: Vec<Entry>,
    finished: VecDeque<FinishedEntry>,
    finished_version: u64,
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

fn append_output(output: &mut String, cut: &mut bool, terminal: &mut bool, text: &str, capture: Capture, source_truncated: bool) -> bool {
    if capture == Capture::Delta && text.is_empty() && !source_truncated {
        return false;
    }
    let kept = tail(text);
    let (next, truncated) = if capture != Capture::Delta || source_truncated {
        (kept.to_string(), source_truncated || kept.len() < text.len())
    } else {
        let mut joined = String::with_capacity(output.len() + kept.len());
        if kept.len() == text.len() {
            joined.push_str(output);
        }
        joined.push_str(kept);
        let next = tail(&joined);
        (next.to_string(), *cut || kept.len() < text.len() || next.len() < joined.len())
    };
    *cut = truncated;
    *terminal = capture == Capture::Terminal;
    if *output == next {
        return false;
    }
    *output = next;
    true
}

impl Commands {
    pub fn finished_version(&self) -> u64 {
        self.finished_version
    }

    /// Terminal metadata and bounded capture, kept only in memory. The caller authorizes and masks before display.
    pub fn recent(&self, conv: &str, id: Option<&str>, limit: usize) -> Vec<CommandRecord> {
        self.finished
            .iter()
            .rev()
            .filter(|e| e.row.conversation_id == conv && id.is_none_or(|id| e.row.id == id))
            .take(limit)
            .map(|e| e.row.clone())
            .collect()
    }

    /// Active metadata only. Do not clone partial capture into project reads.
    pub fn running(&self, conv: &str, id: Option<&str>, limit: usize) -> Vec<CommandRecord> {
        self.entries
            .iter()
            .rev()
            .filter(|e| {
                e.retain_result && e.row.conversation_id == conv && e.row.state == State::Running && id.is_none_or(|id| e.row.id == id)
            })
            .take(limit)
            .map(|e| CommandRecord {
                id: e.row.id.clone(),
                conversation_id: e.row.conversation_id.clone(),
                vendor: e.row.vendor,
                command: (e.row.command.len() <= LABEL_BYTES).then(|| e.row.command.clone()),
                started: e.row.started,
                ended: None,
                state: Some(State::Running),
                output: String::new(),
                output_truncated: false,
                terminal_output: false,
            })
            .collect()
    }

    fn remember(finished: &mut VecDeque<FinishedEntry>, entry: &Entry, ended: u64, state: Option<State>) {
        let row = &entry.row;
        if !entry.retain_result || entry.native.len() > ID_BYTES || row.conversation_id.len() > ID_BYTES {
            return;
        }
        if let Some(old) = finished.iter_mut().find(|e| e.row.id == row.id) {
            old.row.state = state;
            return;
        }
        if finished.len() == FINISHED_LIMIT {
            finished.pop_front();
        }
        finished.push_back(FinishedEntry {
            row: CommandRecord {
                id: row.id.clone(),
                conversation_id: row.conversation_id.clone(),
                vendor: row.vendor,
                command: (row.command.len() <= LABEL_BYTES).then(|| row.command.clone()),
                started: row.started,
                ended: Some(ended),
                state,
                output: row.output.clone(),
                output_truncated: entry.output_truncated,
                terminal_output: entry.terminal_output,
            },
            native: entry.native.clone(),
            sid: entry.sid,
        });
    }

    /// A late event can update its exact result. Session IDs are unique within the engine.
    pub fn late_update(&mut self, conv: Option<&str>, sid: u64, update: &Update) {
        let id = match update {
            Update::Output { id, .. } | Update::End { id, .. } => id,
            Update::Start { .. } => return,
        };
        let Some(entry) = self
            .finished
            .iter_mut()
            .rev()
            .find(|e| e.sid == sid && conv.is_none_or(|conv| e.row.conversation_id == conv) && e.native == *id)
        else {
            return;
        };
        match update {
            Update::Output { text, capture, source_truncated, .. } => {
                append_output(
                    &mut entry.row.output,
                    &mut entry.row.output_truncated,
                    &mut entry.row.terminal_output,
                    text,
                    *capture,
                    *source_truncated,
                );
            }
            Update::End { state, .. } if entry.row.state.is_none() && *state != State::Running => entry.row.state = Some(*state),
            _ => {}
        }
    }

    pub fn forget_finished(&mut self, conv: &str) {
        self.finished_version = self.finished_version.wrapping_add(1);
        self.finished.retain(|entry| entry.row.conversation_id != conv);
        for entry in self.entries.iter_mut().filter(|entry| entry.row.conversation_id == conv) {
            entry.retain_result = false;
        }
    }

    pub fn list(&self) -> List {
        List { revision: self.revision, items: self.entries.iter().map(|e| e.row.clone()).collect() }
    }

    pub fn running_in(&self, conv: &str) -> bool {
        self.entries.iter().any(|e| e.row.conversation_id == conv && e.row.state == State::Running)
    }

    pub fn running_for(&self, session: u64) -> bool {
        self.entries.iter().any(|e| e.sid == session && e.row.state == State::Running)
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
                    output_truncated: false,
                    terminal_output: false,
                    retain_result: true,
                });
            }
            (Some(at), Update::Start { stoppable, turn_ended, .. }) => {
                self.entries[at].turn_ended |= turn_ended;
                if self.entries[at].row.stoppable == stoppable {
                    return false;
                }
                self.entries[at].row.stoppable = stoppable;
            }
            (Some(at), Update::Output { text, capture, source_truncated, .. }) => {
                let entry = &mut self.entries[at];
                let changed = append_output(
                    &mut entry.row.output,
                    &mut entry.output_truncated,
                    &mut entry.terminal_output,
                    &text,
                    capture,
                    source_truncated,
                );
                if entry.row.state != State::Running
                    && let Some(finished) = self.finished.iter_mut().find(|e| e.row.id == entry.row.id)
                {
                    finished.row.output.clone_from(&entry.row.output);
                    finished.row.output_truncated = entry.output_truncated;
                    finished.row.terminal_output = entry.terminal_output;
                }
                if !changed {
                    return false;
                }
            }
            (Some(at), Update::End { state, .. }) => {
                let entry = &mut self.entries[at];
                if entry.row.state == state || state == State::Running {
                    return false;
                }
                entry.row.state = state;
                Self::remember(&mut self.finished, entry, now, Some(state));
                if entry.turn_ended {
                    self.entries.remove(at);
                }
            }
            (None, update @ (Update::Output { .. } | Update::End { .. })) => {
                self.late_update(Some(conv), sid, &update);
                return false;
            }
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

    pub fn exited(&mut self, conv: &str, sid: u64, now: u64) -> bool {
        let before = self.entries.len();
        let finished = &mut self.finished;
        self.entries.retain(|entry| {
            if entry.row.conversation_id != conv || entry.sid != sid {
                return true;
            }
            if entry.row.state == State::Running {
                Self::remember(finished, entry, now, None);
            }
            false
        });
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
        let capture = if replace { Capture::Snapshot } else { Capture::Delta };
        let update = Update::Output { id: id.into(), text: text.into(), capture, source_truncated: false };
        commands.update("one", 1, Vendor::Codex, &session(), update, 124);
    }

    #[test]
    fn a_running_page_is_scoped_limited_and_skips_forgotten_commands() {
        let mut commands = Commands::default();
        start(&mut commands, "one", 1, "first");
        start(&mut commands, "two", 2, "unrelated");
        start(&mut commands, "one", 1, "second");
        let page = commands.running("one", None, 1);
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].command.as_deref(), Some("run second"));
        assert!(commands.running("two", Some(&page[0].id), 1).is_empty());
        commands.forget_finished("one");
        assert!(commands.running("one", None, 2).is_empty());
        assert!(commands.running_in("one"), "restoring history leaves the running processes alone");
    }

    #[test]
    fn a_command_that_finishes_after_its_reply_remains_readable_by_its_public_id() {
        let mut commands = Commands::default();
        start(&mut commands, "one", 1, "build");
        let public_id = commands.list().items[0].id.clone();
        output(&mut commands, "build", "intermediate log\n", false);
        commands.end_turn("one", 1);
        commands.update("one", 1, Vendor::Codex, &session(), Update::End { id: "build".into(), state: State::Failed }, 125);
        assert!(commands.list().items.is_empty(), "the screen no longer owns the row");
        let found = commands.recent("one", Some(&public_id), 1);
        assert_eq!(found.len(), 1, "a resumed agent can still find the result");
        assert_eq!(found[0].state, Some(State::Failed));
        assert_eq!(found[0].ended, Some(125));
        assert_eq!(found[0].output, "intermediate log\n");
        assert!(!found[0].terminal_output);

        let final_log =
            Update::Output { id: "build".into(), text: "final log\n".into(), capture: Capture::Terminal, source_truncated: false };
        commands.late_update(Some("two"), 1, &final_log);
        commands.late_update(Some("one"), 2, &final_log);
        assert_eq!(commands.recent("one", Some(&public_id), 1)[0].output, "intermediate log\n");
        let tail = Update::Output { id: "build".into(), text: "final log\n".into(), capture: Capture::Snapshot, source_truncated: true };
        commands.late_update(Some("one"), 1, &tail);
        assert!(commands.recent("one", Some(&public_id), 1)[0].output_truncated);
        commands.late_update(Some("one"), 1, &final_log);
        let found = commands.recent("one", Some(&public_id), 1);
        assert_eq!(found[0].output, "final log\n");
        assert!(found[0].terminal_output, "the terminal replacement can have identical text");
        assert!(!found[0].output_truncated, "a complete replacement clears the flag even when the text matches");
        assert_eq!(found[0].ended, Some(125), "a late log does not reorder results");
        assert!(commands.recent("two", Some(&public_id), 1).is_empty());
        output(&mut commands, "build", "", false);
        assert!(commands.recent("one", Some(&public_id), 1)[0].terminal_output, "an empty delta adds no progress data");
        output(&mut commands, "build", "late progress\n", false);
        let mixed = commands.recent("one", Some(&public_id), 1);
        assert_eq!(mixed[0].output, "final log\nlate progress\n");
        assert!(!mixed[0].terminal_output, "the mixed output has bytes beyond the terminal snapshot");
        commands.late_update(Some("one"), 1, &final_log);
        output(&mut commands, "build", "final log\n", true);
        assert!(!commands.recent("one", Some(&public_id), 1)[0].terminal_output, "a later progress snapshot replaces identical bytes");
    }

    #[test]
    fn a_confirmed_process_exit_leaves_unknown_state_and_marks_a_clipped_capture() {
        let mut commands = Commands::default();
        commands.update(
            "one",
            1,
            Vendor::Claude,
            &session(),
            Update::Start { id: "build".into(), command: "x".repeat(LABEL_BYTES + 1), stoppable: true, turn_ended: false },
            123,
        );
        output(&mut commands, "build", &"line\n".repeat(MAX_LINES + 1), true);
        output(&mut commands, "build", "last\n", false);
        assert!(commands.exited("one", 1, 126));
        assert!(commands.list().items.is_empty());
        let done = &commands.recent("one", None, 1)[0];
        assert_eq!(done.state, None);
        assert_eq!(done.ended, Some(126));
        assert!(done.command.is_none());
        assert!(done.output.len() <= MAX_BYTES && done.output.lines().count() <= MAX_LINES);
        assert!(done.output_truncated);
        assert!(!done.terminal_output, "process exit cannot confirm the source of progress output");
        commands.late_update(None, 1, &Update::End { id: "build".into(), state: State::Done });
        assert_eq!(commands.recent("one", None, 1)[0].state, Some(State::Done));
    }

    #[test]
    fn forgetting_results_prevents_reinsertion_when_an_old_command_ends() {
        let mut commands = Commands::default();
        start(&mut commands, "one", 1, "prior");
        commands.update("one", 1, Vendor::Codex, &session(), Update::End { id: "prior".into(), state: State::Done }, 124);
        start(&mut commands, "one", 1, "current");
        commands.forget_finished("one");
        assert!(commands.recent("one", None, 1).is_empty());
        assert!(commands.running_in("one"));
        commands.exited("one", 1, 125);
        assert!(commands.recent("one", None, 1).is_empty());
    }

    #[test]
    fn finished_command_retention_is_global_and_follows_terminal_order() {
        let mut commands = Commands::default();
        for i in 0..=FINISHED_LIMIT {
            let conv = if i == 0 { "two" } else { "one" };
            start(&mut commands, conv, 1, &i.to_string());
            commands.end_turn(conv, 1);
            commands.update(conv, 1, Vendor::Codex, &session(), Update::End { id: i.to_string(), state: State::Done }, 200 + i as u64);
        }
        assert!(commands.recent("two", None, 1).is_empty());
        let recent = commands.recent("one", None, FINISHED_LIMIT + 1);
        assert_eq!(recent.len(), FINISHED_LIMIT);
        assert_eq!(recent.first().unwrap().ended, Some(200 + FINISHED_LIMIT as u64));
        assert_eq!(recent.last().unwrap().ended, Some(201));
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
        assert!(!commands.exited("one", 1, 124));
        assert!(commands.exited("two", 1, 124));
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
