//! What the terminal screen knows and how it changes: the state, the things that can happen to it (`Input`) and the things
//! it asks the program to do (`Effect`). Nothing here touches a terminal or the channel, so the tests drive it with keys and
//! events alone.

use super::card::{self, Card};
use super::editor::Editor;
use super::palette::Palette;
use super::transcript::Transcript;
use crate::thread::Thread;
use crate::words::{self, Words};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::Rect;
use serde_json::Value;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// A second Ctrl+C within this long after the first leaves.
pub(super) const LEAVE_WITHIN: Duration = Duration::from_secs(2);
/// Below this many columns the conversation list is hidden until the person asks for it.
pub const SIDEBAR_FROM: u16 = 100;
/// The working mark moves at most this often (8 frames a second).
pub const FRAME: Duration = Duration::from_millis(125);
/// Accounts are checked again on a list change no more often than this.
pub(super) const RECHECK: Duration = Duration::from_secs(60);
/// The commands of the `/` list, in the order it shows them.
pub const COMMANDS: [&str; 8] = ["new", "rename", "archive", "pin", "stop", "quota", "help", "quit"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    List,
    Thread,
    Box,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    /// The box holds the new title of the open conversation.
    Rename,
    /// The box holds the person's own answer to the card's question.
    Answer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Account {
    Unknown,
    SignedIn,
    SignedOut,
    Missing,
}

pub struct Conv {
    pub id: String,
    pub title: String,
    pub area: String,
    pub project_id: Option<String>,
    pub updated_at: u64,
    pub pinned: bool,
}

pub struct Quota {
    pub vendor: String,
    pub window: String,
    pub used: f64,
    pub resets_at: f64,
}

pub struct Run {
    pub conversation: String,
    pub turn: String,
    pub needs_you: bool,
    pub permissions: Vec<Value>,
}

/// A list the person reads and closes: the keys, or the quota.
pub struct Popup {
    pub title: String,
    pub lines: Vec<String>,
}

/// Where each area was drawn last, for the mouse.
#[derive(Default, Clone, Copy)]
pub struct Geometry {
    pub list: Option<Rect>,
    pub thread: Rect,
    pub input: Rect,
}

pub enum Input {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
    Resize(u16, u16),
    /// The working mark's next frame.
    Tick,
    /// An event from Faber.
    Faber(Value),
    /// The answer of `state`.
    State(Value),
    /// The answer of `open`: the conversation's id and the conversation.
    Opened(String, Value),
    /// The answer of `send`: the conversation's id.
    Sent(String),
    /// The answer of `setup_check`.
    Account(Value),
    /// A command was refused: the key of the reason.
    Failed(String),
    /// Faber closed the channel.
    Lost,
}

pub enum Effect {
    State,
    Open(String),
    Send(Value),
    /// A command about a conversation; `refresh` reads the state again when it is done.
    Call {
        cmd: &'static str,
        args: Value,
        refresh: bool,
    },
    Accounts,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Leave {
    Done,
    /// Faber went away while the screen was open.
    Gone,
}

pub struct App {
    pub words: Words,
    pub(super) language: String,
    pub palette: Palette,
    pub reduced_motion: bool,
    pub now: Instant,
    pub size: (u16, u16),
    pub focus: Focus,
    pub mode: Mode,
    /// Some(false) or Some(true) once the person toggled it; None follows the width.
    pub sidebar: Option<bool>,
    pub conversations: Vec<Conv>,
    pub projects: Vec<(String, String)>,
    pub running: Vec<Run>,
    pub quotas: Vec<Quota>,
    pub accounts: [Account; 2],
    pub checked: Option<Instant>,
    pub active: Option<String>,
    pub thread: Option<Thread>,
    pub loading: bool,
    pub transcript: Transcript,
    pub scroll: usize,
    pub last_total: usize,
    pub list_cursor: usize,
    pub list_top: usize,
    pub editor: Editor,
    pub(super) history: Vec<String>,
    pub(super) history_at: Option<usize>,
    pub command_cursor: usize,
    pub card: Option<Card>,
    pub(super) answered: HashSet<String>,
    pub(super) deferred: HashSet<String>,
    pub popup: Option<Popup>,
    pub toast: Option<String>,
    pub leave_armed: Option<Instant>,
    pub frame: usize,
    pub geo: Geometry,
    pub leave: Option<Leave>,
    pub(super) state_pending: bool,
    pub(super) state_again: bool,
}

pub const VENDORS: [&str; 2] = ["claude", "codex"];

fn str_of(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or("").to_string()
}

/// The parts of a message the screen never shows, dropped as it arrives: only the open conversation is kept, and lean.
pub fn slim(message: &mut Value) {
    let Some(object) = message.as_object_mut() else { return };
    for key in ["thinking", "images", "visualFiles", "toolKit", "files", "commits"] {
        object.remove(key);
    }
    for tool in object.get_mut("tools").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(tool) = tool.as_object_mut() {
            tool.remove("output");
            tool.remove("facts");
        }
    }
}

impl App {
    pub fn new(words: Words, palette: Palette, reduced_motion: bool, now: Instant) -> App {
        App {
            words,
            language: String::new(),
            palette,
            reduced_motion,
            now,
            size: (80, 24),
            focus: Focus::Box,
            mode: Mode::Normal,
            sidebar: None,
            conversations: Vec::new(),
            projects: Vec::new(),
            running: Vec::new(),
            quotas: Vec::new(),
            accounts: [Account::Unknown; 2],
            checked: None,
            active: None,
            thread: None,
            loading: false,
            transcript: Transcript::default(),
            scroll: 0,
            last_total: 0,
            list_cursor: 0,
            list_top: 0,
            editor: Editor::default(),
            history: Vec::new(),
            history_at: None,
            command_cursor: 0,
            card: None,
            answered: HashSet::new(),
            deferred: HashSet::new(),
            popup: None,
            toast: None,
            leave_armed: None,
            frame: 0,
            geo: Geometry::default(),
            leave: None,
            state_pending: false,
            state_again: false,
        }
    }

    pub fn t(&self, key: &str) -> String {
        self.words.get(key)
    }

    // ---- what is known ----

    /// The whole `state` answer (or the inner state): the lists, the running items, the quotas and the language.
    pub fn apply_state(&mut self, answer: &Value) {
        let state = if answer.get("state").is_some() { &answer["state"] } else { answer };
        let language = words::language(state["settings"]["language"].as_str());
        if language != self.language {
            self.words = Words::new(&language);
            self.language = language;
            self.transcript.clear();
        }
        self.conversations = state["conversations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|c| c["archived"].as_bool() != Some(true))
            .map(|c| Conv {
                id: str_of(c, "id"),
                title: str_of(c, "title"),
                area: str_of(c, "area"),
                project_id: c["projectId"].as_str().map(str::to_string),
                updated_at: c["updatedAt"].as_u64().unwrap_or(0),
                pinned: c["pinnedAt"].as_u64().is_some(),
            })
            .collect();
        self.conversations.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.updated_at.cmp(&a.updated_at)));
        self.projects = state["projects"].as_array().into_iter().flatten().map(|p| (str_of(p, "id"), str_of(p, "name"))).collect();
        self.quotas = state["quotas"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|q| Quota {
                vendor: str_of(q, "vendor"),
                window: str_of(q, "window"),
                used: q["used"].as_f64().unwrap_or(0.0),
                resets_at: q["resetsAt"].as_f64().unwrap_or(0.0),
            })
            .collect();
        self.set_running(state["running"]["items"].as_array().cloned().unwrap_or_default());
        self.list_cursor = self.list_cursor.min(self.conversations.len());
    }

    pub(super) fn set_running(&mut self, items: Vec<Value>) {
        self.running = items
            .iter()
            .map(|i| Run {
                conversation: str_of(i, "conversationId"),
                turn: str_of(i, "turnId"),
                needs_you: i["state"].as_str() == Some("needsYou"),
                permissions: i["permissions"].as_array().cloned().unwrap_or_default(),
            })
            .collect();
        // An answer that went out is forgotten once Faber no longer lists the card.
        let pending: HashSet<String> = self.pending_keys();
        self.answered.retain(|k| pending.contains(k));
        self.deferred.retain(|k| pending.contains(k));
        self.sync_card();
    }

    fn pending_keys(&self) -> HashSet<String> {
        self.running
            .iter()
            .flat_map(|r| r.permissions.iter().map(|p| card::key_of(&r.turn, p["requestId"].as_str().unwrap_or(""))))
            .collect()
    }

    /// The card to show for the open conversation: the first one waiting that was not answered or put off.
    pub(super) fn sync_card(&mut self) {
        let Some(active) = self.active.clone() else {
            self.card = None;
            return;
        };
        let waiting: Vec<(String, String, &Value)> = self
            .running
            .iter()
            .filter(|r| r.conversation == active)
            .flat_map(|r| r.permissions.iter().map(|p| (card::key_of(&r.turn, p["requestId"].as_str().unwrap_or("")), r.turn.clone(), p)))
            .filter(|(key, _, _)| !self.answered.contains(key))
            .collect();
        if let Some(card) = &self.card
            && waiting.iter().any(|(key, _, _)| *key == card.key)
            && !self.deferred.contains(&card.key)
        {
            return;
        }
        self.card = waiting.iter().find(|(key, _, _)| !self.deferred.contains(key)).map(|(_, turn, p)| Card::new(&active, turn, p));
        if self.mode == Mode::Answer && self.card.as_ref().is_none_or(|c| !c.own) {
            self.mode = Mode::Normal;
        }
    }

    /// A card of the open conversation was put off with Esc.
    pub fn card_deferred(&self) -> bool {
        let Some(active) = &self.active else { return false };
        self.card.is_none()
            && self
                .running
                .iter()
                .filter(|r| &r.conversation == active)
                .flat_map(|r| r.permissions.iter().map(|p| card::key_of(&r.turn, p["requestId"].as_str().unwrap_or(""))))
                .any(|k| self.deferred.contains(&k) && !self.answered.contains(&k))
    }

    pub fn is_running(&self, conversation: &str) -> bool {
        self.running.iter().any(|r| r.conversation == conversation)
            || (self.active.as_deref() == Some(conversation)
                && self.thread.as_ref().and_then(Thread::last).is_some_and(|m| m["role"] == "assistant" && m["status"] == "streaming"))
    }

    pub fn needs_you(&self, conversation: &str) -> bool {
        self.running.iter().any(|r| r.conversation == conversation && r.needs_you)
    }

    /// Whether the working mark moves: something is answering and the person did not ask for less motion.
    pub fn animating(&self) -> bool {
        !self.reduced_motion && (self.running.iter().any(|r| !r.needs_you) || self.active.as_deref().is_some_and(|a| self.is_running(a)))
    }

    pub fn show_sidebar(&self) -> bool {
        self.sidebar.unwrap_or(self.size.0 >= SIDEBAR_FROM)
    }

    /// The title of the open conversation, and its project's name when it belongs to one.
    pub fn crumb(&self) -> (String, String) {
        let Some(conv) = self.active.as_ref().and_then(|id| self.conversations.iter().find(|c| &c.id == id)) else {
            return (self.t("cli.list.chat"), self.t("cli.list.untitled"));
        };
        let area = match conv.project_id.as_ref().and_then(|id| self.projects.iter().find(|(p, _)| p == id)) {
            Some((_, name)) => name.clone(),
            None => self.t("cli.list.chat"),
        };
        (area, if conv.title.is_empty() { self.t("cli.list.untitled") } else { conv.title.clone() })
    }

    /// The commands that match what is typed after the `/`; empty when the box is not asking for a command.
    pub fn command_matches(&self) -> Vec<&'static str> {
        let text = self.editor.text();
        match text.strip_prefix('/') {
            Some(rest) if self.mode == Mode::Normal && !rest.contains([' ', '\n']) => {
                let rest = rest.to_lowercase();
                COMMANDS.iter().copied().filter(|c| c.starts_with(&rest)).collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn commands_open(&self) -> bool {
        self.mode == Mode::Normal && self.editor.text().starts_with('/') && !self.editor.text().contains([' ', '\n'])
    }

    /// The conversation list: one row per conversation after the row that starts a new one.
    pub fn list_len(&self) -> usize {
        self.conversations.len() + 1
    }

    pub fn leave_hint_live(&self) -> bool {
        self.leave_armed.is_some_and(|at| self.now.saturating_duration_since(at) < LEAVE_WITHIN)
    }
}
