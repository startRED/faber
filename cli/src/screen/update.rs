//! How the screen changes: events from Faber, answers to its own requests, and the commands the person gives.
//! Keys are in `keys.rs`.

use super::app::{Account, App, Effect, Input, Leave, Mode, Popup, Quota, RECHECK, VENDORS};
use crate::thread::Thread;
use serde_json::{Value, json};
use std::time::Instant;

impl App {
    /// One thing that happened; what the program has to do about it.
    pub fn update(&mut self, input: Input, now: Instant) -> Vec<Effect> {
        self.now = now;
        let mut fx = Vec::new();
        match input {
            Input::Key(key) => self.key(key, &mut fx),
            Input::Mouse(mouse) => self.mouse(mouse, &mut fx),
            Input::Paste(text) => self.paste(&text),
            Input::Resize(w, h) => self.size = (w, h),
            Input::Tick => self.frame = self.frame.wrapping_add(1),
            Input::Faber(event) => self.faber(&event, &mut fx),
            Input::State(state) => {
                self.state_pending = false;
                self.apply_state(&state);
                if std::mem::take(&mut self.state_again) {
                    self.want_state(&mut fx);
                }
            }
            Input::Opened(id, mut conversation) => {
                if self.active.as_deref() == Some(&id) {
                    for message in conversation["messages"].as_array_mut().into_iter().flatten() {
                        super::app::slim(message);
                    }
                    self.thread = Some(Thread::from_conversation(&conversation));
                    self.loading = false;
                    self.scroll = 0;
                    self.sync_card();
                }
            }
            Input::Sent(id) => {
                if self.active.is_none() {
                    self.open_conversation(&id, &mut fx);
                    self.focus = super::app::Focus::Box;
                }
                self.want_state(&mut fx);
            }
            Input::Account(check) => self.account(&check),
            Input::Failed(key) => {
                self.state_pending = false;
                self.toast = Some(self.words.get(&key));
            }
            Input::Lost => self.leave = Some(Leave::Gone),
        }
        fx
    }

    pub(super) fn want_state(&mut self, fx: &mut Vec<Effect>) {
        if self.state_pending {
            self.state_again = true;
        } else {
            self.state_pending = true;
            fx.push(Effect::State);
        }
    }

    fn account(&mut self, check: &Value) {
        let Some(at) = VENDORS.iter().position(|v| check["vendor"].as_str() == Some(v)) else { return };
        self.checked.get_or_insert(self.now);
        self.accounts[at] = if check.get("error").is_some() {
            Account::Unknown
        } else if check["installed"].as_bool() != Some(true) {
            Account::Missing
        } else {
            match check["account"].as_str().unwrap_or("").to_ascii_lowercase().replace('_', "").as_str() {
                "signedin" => Account::SignedIn,
                "signedout" => Account::SignedOut,
                _ => Account::Unknown,
            }
        };
    }

    fn faber(&mut self, event: &Value, fx: &mut Vec<Effect>) {
        match event["type"].as_str() {
            Some("message") => {
                let conversation = event["conversationId"].as_str().unwrap_or("").to_string();
                let mut message = event["message"].clone();
                super::app::slim(&mut message);
                let ended = message["role"] == "assistant" && message["status"].as_str().is_some_and(|s| s != "streaming");
                if self.active.as_deref() == Some(&conversation)
                    && let Some(thread) = self.thread.as_mut()
                    && thread.take(message).is_err()
                {
                    // A reply this screen never saw whole: opening the conversation again sends the next update whole.
                    fx.push(Effect::Open(conversation.clone()));
                }
                if ended || !self.conversations.iter().any(|c| c.id == conversation) {
                    self.want_state(fx);
                }
            }
            Some("running") => {
                let items = event["items"].as_array().cloned().unwrap_or_default();
                self.set_running(items);
            }
            Some("quota") => {
                let (vendor, window) = (event["vendor"].as_str().unwrap_or(""), event["window"].as_str().unwrap_or(""));
                let quota = Quota {
                    vendor: vendor.into(),
                    window: window.into(),
                    used: event["used"].as_f64().unwrap_or(0.0),
                    resets_at: event["resetsAt"].as_f64().unwrap_or(0.0),
                };
                match self.quotas.iter_mut().find(|q| q.vendor == vendor && q.window == window) {
                    Some(old) => *old = quota,
                    None => self.quotas.push(quota),
                }
            }
            // Events were dropped while this screen was slow: read everything again.
            Some("lagged") => {
                self.want_state(fx);
                if let Some(id) = self.active.clone() {
                    fx.push(Effect::Open(id));
                }
            }
            Some("list" | "settings") => {
                self.want_state(fx);
                // A sign-in elsewhere shows up here too; checking is rate limited because a check itself announces a change.
                if event["type"] == "list" && self.checked.is_some_and(|at| self.now.saturating_duration_since(at) >= RECHECK) {
                    self.checked = Some(self.now);
                    fx.push(Effect::Accounts);
                }
            }
            _ => {}
        }
    }

    // ---- conversations ----

    pub(super) fn open_conversation(&mut self, id: &str, fx: &mut Vec<Effect>) {
        self.active = Some(id.to_string());
        self.thread = Some(Thread::default());
        self.loading = true;
        self.scroll = 0;
        self.mode = Mode::Normal;
        self.focus = super::app::Focus::Box;
        self.card = None;
        if self.size.0 < super::app::SIDEBAR_FROM {
            self.sidebar = Some(false);
        }
        if let Some(at) = self.conversations.iter().position(|c| c.id == id) {
            self.list_cursor = at + 1;
        }
        self.sync_card();
        fx.push(Effect::Open(id.to_string()));
    }

    pub(super) fn new_chat(&mut self) {
        self.active = None;
        self.thread = None;
        self.loading = false;
        self.scroll = 0;
        self.card = None;
        self.mode = Mode::Normal;
        self.focus = super::app::Focus::Box;
        self.list_cursor = 0;
        if self.size.0 < super::app::SIDEBAR_FROM {
            self.sidebar = Some(false);
        }
    }

    // ---- commands ----

    pub(super) fn run_command(&mut self, cmd: &str, arg: &str, fx: &mut Vec<Effect>) {
        let Some(id) = self.active.clone() else {
            match cmd {
                "new" => self.new_chat(),
                "quota" => self.popup = Some(self.quota_popup()),
                "help" => self.popup = Some(self.help_popup()),
                "quit" => self.leave = Some(Leave::Done),
                _ => self.toast = Some(self.t("cli.screen.noOpen")),
            }
            return;
        };
        match cmd {
            "new" => self.new_chat(),
            "rename" if !arg.is_empty() => fx.push(Effect::Call { cmd: "rename", args: json!({ "id": id, "title": arg }), refresh: true }),
            "rename" => {
                self.mode = Mode::Rename;
                self.editor.clear();
            }
            "archive" => {
                fx.push(Effect::Call { cmd: "archive", args: json!({ "id": id, "on": true }), refresh: true });
                self.new_chat();
            }
            "pin" => {
                let pinned = self.conversations.iter().find(|c| c.id == id).is_some_and(|c| c.pinned);
                fx.push(Effect::Call { cmd: "pin", args: json!({ "id": id, "on": !pinned }), refresh: true });
            }
            "stop" => fx.push(Effect::Call { cmd: "interrupt", args: json!({ "id": id }), refresh: false }),
            "quota" => self.popup = Some(self.quota_popup()),
            "help" => self.popup = Some(self.help_popup()),
            "quit" => self.leave = Some(Leave::Done),
            _ => {}
        }
    }

    pub(super) fn help_popup(&self) -> Popup {
        let lines = ["send", "newline", "stop", "focus", "scroll", "new", "list", "commands", "card", "select", "quit"]
            .iter()
            .map(|k| self.t(&format!("cli.screen.help.{k}")))
            .collect();
        Popup { title: self.t("cli.screen.help.title"), lines }
    }

    pub(super) fn quota_popup(&self) -> Popup {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let mut lines: Vec<String> = self
            .quotas
            .iter()
            .map(|q| {
                let used = q.used.clamp(0.0, 1.0);
                let filled = (used * 12.0).round() as usize;
                let window = self.window_name(&q.window);
                let resets = if q.resets_at > now {
                    format!("  {}", self.words.fill("cli.quota.resets", &[("when", &self.duration((q.resets_at - now) as u64))]))
                } else {
                    String::new()
                };
                format!(
                    "{:<12} {:<8} {}{} {:>3}%{resets}",
                    self.t(&format!("cli.vendor.{}", q.vendor)),
                    window,
                    "█".repeat(filled),
                    "░".repeat(12 - filled),
                    (used * 100.0).round()
                )
            })
            .collect();
        if lines.is_empty() {
            lines.push(self.t("cli.quota.none"));
        }
        Popup { title: self.t("cli.screen.quota.title"), lines }
    }

    pub fn window_name(&self, window: &str) -> String {
        let name = self.t(&format!("quota.{window}"));
        if name.starts_with("quota.") { window.to_string() } else { name }
    }

    fn duration(&self, seconds: u64) -> String {
        let (n, key) = match seconds {
            s if s < 3600 => ((s / 60).max(1), "cli.time.minutes"),
            s if s < 86_400 => (s / 3600, "cli.time.hours"),
            s => (s / 86_400, "cli.time.days"),
        };
        self.words.fill(key, &[("n", &n.to_string())])
    }
}
