//! Keys and the mouse: what each does in the list, the thread and the box, and on a card.

use super::app::{App, Effect, Focus, Leave, Mode};
use super::card::Step;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use serde_json::json;

/// How many lines the mouse wheel moves.
const WHEEL: usize = 3;

fn inside(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
}

impl App {
    pub(super) fn key(&mut self, key: KeyEvent, fx: &mut Vec<Effect>) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        self.toast = None;
        if ctrl && matches!(key.code, KeyCode::Char('c' | 'C')) {
            self.ctrl_c(fx);
            return;
        }
        self.leave_armed = None;
        if self.popup.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.popup = None;
            }
            return;
        }
        match key.code {
            KeyCode::Char('n' | 'N') if ctrl => self.new_chat(),
            KeyCode::Char('b' | 'B') if ctrl => {
                let shown = !self.show_sidebar();
                self.sidebar = Some(shown);
                self.focus = if shown {
                    Focus::List
                } else if self.focus == Focus::List {
                    Focus::Box
                } else {
                    self.focus
                };
            }
            KeyCode::Char('k' | 'K') if ctrl => {
                self.deferred.clear();
                self.sync_card();
            }
            KeyCode::Tab => self.cycle(true),
            KeyCode::BackTab => self.cycle(false),
            KeyCode::F(1) => self.popup = Some(self.help_popup()),
            KeyCode::Esc => self.escape(),
            _ if self.card_key(&key, fx) => {}
            _ => match self.focus {
                Focus::Box => self.box_key(&key, fx),
                Focus::List => self.list_key(&key, fx),
                Focus::Thread => self.thread_key(&key),
            },
        }
    }

    /// Ctrl+C: stops the answer; a second one within two seconds, or one with nothing running, leaves.
    fn ctrl_c(&mut self, fx: &mut Vec<Effect>) {
        if self.leave_hint_live() {
            self.leave = Some(Leave::Done);
            return;
        }
        let running = self.active.as_deref().filter(|id| self.is_running(id)).map(str::to_string);
        if let Some(id) = running {
            fx.push(Effect::Call { cmd: "interrupt", args: json!({ "id": id }), refresh: false });
            self.leave_armed = Some(self.now);
            self.toast = Some(format!("{} {}", self.t("cli.chat.stopped"), self.t("cli.screen.leave")));
        } else if !self.editor.is_empty() {
            self.editor.clear();
            self.history_at = None;
            if self.mode == Mode::Answer
                && let Some(card) = self.card.as_mut()
            {
                card.cancel_own();
            }
            self.mode = Mode::Normal;
        } else {
            self.leave = Some(Leave::Done);
        }
    }

    fn escape(&mut self) {
        if self.mode != Mode::Normal {
            if self.mode == Mode::Answer
                && let Some(card) = self.card.as_mut()
            {
                card.cancel_own();
            }
            self.mode = Mode::Normal;
            self.editor.clear();
        } else if self.commands_open() {
            self.editor.clear();
        } else if let Some(card) = self.card.take() {
            self.deferred.insert(card.key);
        } else if self.focus != Focus::Box {
            self.focus = Focus::Box;
        }
    }

    fn cycle(&mut self, forward: bool) {
        let mut order = vec![Focus::Thread, Focus::Box];
        if self.show_sidebar() {
            order.insert(0, Focus::List);
        }
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if forward { (at + 1) % order.len() } else { (at + order.len() - 1) % order.len() };
        self.focus = order[next];
    }

    // ---- the card ----

    /// Number keys, the arrows and Enter answer a card while the box is empty; true when the key was for the card.
    fn card_key(&mut self, key: &KeyEvent, fx: &mut Vec<Effect>) -> bool {
        let plain = !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        let Some(card) = self.card.as_mut().filter(|_| plain && self.mode == Mode::Normal && self.editor.is_empty()) else { return false };
        let step = match key.code {
            KeyCode::Char(d @ '1'..='9') => card.pick(d as usize - '1' as usize),
            KeyCode::Up => {
                card.move_cursor(false);
                Step::Stay
            }
            KeyCode::Down => {
                card.move_cursor(true);
                Step::Stay
            }
            KeyCode::Enter => card.confirm(),
            _ => return false,
        };
        self.card_step(step, fx);
        true
    }

    pub(super) fn card_step(&mut self, step: Step, fx: &mut Vec<Effect>) {
        match step {
            Step::Send(cmd, args) => {
                if let Some(card) = self.card.take() {
                    self.answered.insert(card.key);
                }
                self.mode = Mode::Normal;
                self.editor.clear();
                fx.push(Effect::Call { cmd, args, refresh: false });
                self.sync_card();
            }
            Step::Stay => {
                if self.card.as_ref().is_some_and(|c| c.own) {
                    self.mode = Mode::Answer;
                    self.editor.clear();
                    self.focus = Focus::Box;
                }
            }
        }
    }

    // ---- the box ----

    fn input_width(&self) -> usize {
        let width = if self.geo.input.width > 2 { self.geo.input.width } else { self.size.0 };
        usize::from(width).saturating_sub(2).max(4)
    }

    fn box_key(&mut self, key: &KeyEvent, fx: &mut Vec<Effect>) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let width = self.input_width();
        match key.code {
            KeyCode::Enter if alt || shift => self.editor.newline(),
            KeyCode::Enter => self.submit(fx),
            KeyCode::Char('j') if ctrl => self.editor.newline(),
            KeyCode::Char('a') if ctrl => self.editor.home(),
            KeyCode::Char('e') if ctrl => self.editor.end(),
            KeyCode::Char('u') if ctrl => self.editor.delete_to_line_start(),
            KeyCode::Char('w') if ctrl => self.editor.delete_word_back(),
            KeyCode::Char(c) if !ctrl && !alt => {
                self.editor.insert(&c.to_string());
                self.history_at = None;
                self.command_cursor = 0;
            }
            KeyCode::Backspace if ctrl || alt => self.editor.delete_word_back(),
            KeyCode::Backspace => {
                self.editor.backspace();
                self.command_cursor = 0;
            }
            KeyCode::Delete => self.editor.delete(),
            KeyCode::Left if ctrl || alt => self.editor.word_left(),
            KeyCode::Left => self.editor.left(),
            KeyCode::Right if ctrl || alt => self.editor.word_right(),
            KeyCode::Right => self.editor.right(),
            KeyCode::End if ctrl => self.scroll = 0,
            KeyCode::Home if ctrl => self.scroll = usize::MAX / 2,
            KeyCode::Home => self.editor.home(),
            KeyCode::End => self.editor.end(),
            KeyCode::Up if self.commands_open() => self.command_cursor = self.command_cursor.saturating_sub(1),
            KeyCode::Down if self.commands_open() => {
                self.command_cursor = (self.command_cursor + 1).min(self.command_matches().len().saturating_sub(1))
            }
            KeyCode::Up => {
                if !self.editor.up(width) {
                    self.history_back();
                }
            }
            KeyCode::Down => {
                if !self.editor.down(width) {
                    self.history_forward();
                }
            }
            KeyCode::PageUp => self.scroll_up(self.page()),
            KeyCode::PageDown => self.scroll_down(self.page()),
            _ => {}
        }
    }

    /// Up on the first row brings back what was sent; it never replaces text the person is writing.
    fn history_back(&mut self) {
        if self.history.is_empty() || !(self.editor.is_empty() || self.history_at.is_some()) {
            self.editor.home();
            return;
        }
        let at = self.history_at.map_or(self.history.len() - 1, |at| at.saturating_sub(1));
        self.history_at = Some(at);
        let text = self.history[at].clone();
        self.editor.set(&text);
    }

    fn history_forward(&mut self) {
        match self.history_at {
            Some(at) if at + 1 < self.history.len() => {
                self.history_at = Some(at + 1);
                let text = self.history[at + 1].clone();
                self.editor.set(&text);
            }
            Some(_) => {
                self.history_at = None;
                self.editor.clear();
            }
            None => self.editor.end(),
        }
    }

    fn submit(&mut self, fx: &mut Vec<Effect>) {
        let text = self.editor.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        match self.mode {
            Mode::Rename => {
                if let Some(id) = self.active.clone() {
                    fx.push(Effect::Call { cmd: "rename", args: json!({ "id": id, "title": text }), refresh: true });
                }
                self.mode = Mode::Normal;
                self.editor.clear();
                return;
            }
            Mode::Answer => {
                let step = self.card.as_mut().map_or(Step::Stay, |c| c.own_answer(&text));
                self.editor.clear();
                if step == Step::Stay && self.card.as_ref().is_none_or(|c| !c.own) {
                    self.mode = Mode::Normal;
                }
                self.card_step(step, fx);
                return;
            }
            Mode::Normal => {}
        }
        if let Some(rest) = text.strip_prefix('/') {
            let (name, arg) = rest.split_once(char::is_whitespace).map_or((rest, ""), |(n, a)| (n, a.trim()));
            let matches = self.command_matches();
            let chosen = if self.commands_open() && !matches.is_empty() {
                Some(matches[self.command_cursor.min(matches.len() - 1)])
            } else {
                super::app::COMMANDS.iter().copied().find(|c| *c == name.to_lowercase())
            };
            if let Some(cmd) = chosen {
                self.editor.clear();
                self.command_cursor = 0;
                self.run_command(cmd, arg, fx);
                return;
            }
        }
        let conv = self.active.as_ref().and_then(|id| self.conversations.iter().find(|c| &c.id == id));
        let area = conv.map_or("chat", |c| if c.area.is_empty() { "chat" } else { &c.area });
        let req = json!({
            "conversationId": self.active,
            "area": area,
            "projectId": conv.and_then(|c| c.project_id.clone()),
            "text": text,
            "choice": { "vendor": "auto" },
        });
        if self.history.last() != Some(&text) {
            self.history.push(text);
        }
        self.history_at = None;
        self.editor.clear();
        self.scroll = 0;
        fx.push(Effect::Send(req));
    }

    // ---- the list and the thread ----

    fn list_key(&mut self, key: &KeyEvent, fx: &mut Vec<Effect>) {
        let last = self.list_len() - 1;
        match key.code {
            KeyCode::Up => self.list_cursor = self.list_cursor.saturating_sub(1),
            KeyCode::Down => self.list_cursor = (self.list_cursor + 1).min(last),
            KeyCode::PageUp => self.list_cursor = self.list_cursor.saturating_sub(10),
            KeyCode::PageDown => self.list_cursor = (self.list_cursor + 10).min(last),
            KeyCode::Home => self.list_cursor = 0,
            KeyCode::End => self.list_cursor = last,
            KeyCode::Enter => self.open_row(self.list_cursor, fx),
            _ => self.type_into_box(key),
        }
    }

    pub(super) fn open_row(&mut self, row: usize, fx: &mut Vec<Effect>) {
        self.list_cursor = row;
        if row == 0 {
            self.new_chat();
        } else if let Some(id) = self.conversations.get(row - 1).map(|c| c.id.clone()) {
            self.open_conversation(&id, fx);
        }
    }

    fn thread_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Up => self.scroll_up(1),
            KeyCode::Down => self.scroll_down(1),
            KeyCode::PageUp => self.scroll_up(self.page()),
            KeyCode::PageDown => self.scroll_down(self.page()),
            KeyCode::Home => self.scroll = usize::MAX / 2,
            KeyCode::End => self.scroll = 0,
            _ => self.type_into_box(key),
        }
    }

    /// A letter typed while the list or the thread has the focus is the start of a message.
    fn type_into_box(&mut self, key: &KeyEvent) {
        if let KeyCode::Char(c) = key.code
            && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            self.focus = Focus::Box;
            self.editor.insert(&c.to_string());
        }
    }

    fn page(&self) -> usize {
        usize::from(self.geo.thread.height).saturating_sub(1).max(3)
    }

    fn scroll_up(&mut self, lines: usize) {
        self.scroll = self.scroll.saturating_add(lines).min(usize::MAX / 2);
    }

    fn scroll_down(&mut self, lines: usize) {
        self.scroll = self.scroll.saturating_sub(lines);
    }

    pub(super) fn paste(&mut self, text: &str) {
        self.focus = Focus::Box;
        self.editor.insert(text);
        self.history_at = None;
    }

    pub(super) fn mouse(&mut self, mouse: MouseEvent, fx: &mut Vec<Effect>) {
        let (x, y) = (mouse.column, mouse.row);
        let list = self.geo.list.filter(|r| inside(*r, x, y));
        match mouse.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = mouse.kind == MouseEventKind::ScrollUp;
                if list.is_some() {
                    let last = self.list_len() - 1;
                    self.list_cursor = if up { self.list_cursor.saturating_sub(WHEEL) } else { (self.list_cursor + WHEEL).min(last) };
                } else if up {
                    self.scroll_up(WHEEL);
                } else {
                    self.scroll_down(WHEEL);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(rect) = list {
                    let row = self.list_top + usize::from(y - rect.y);
                    if row < self.list_len() {
                        self.open_row(row, fx);
                    }
                    self.focus = if self.active.is_some() && row > 0 { Focus::Box } else { Focus::List };
                } else if inside(self.geo.thread, x, y) {
                    self.focus = Focus::Thread;
                } else if inside(self.geo.input, x, y) {
                    self.focus = Focus::Box;
                }
            }
            _ => {}
        }
    }
}
