//! The engine: conversations, live CLI sessions, permissions, branches and attachments.

use crate::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
use crate::cli::Cli;
use crate::codex::{CodexLaunch, CodexOptions, start_codex};
use crate::events::{EngineEvent, Session, UserTurn};
use crate::model::*;
use crate::proc::Spawner;
use crate::router::{Choice, Pick, pick_model};
use crate::store::Store;
use crate::workspace::{Attachment, inside_folder, new_files, save_files};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AppEvent {
    Message { conversation_id: String, message: Box<Message> },
    Quota { vendor: Vendor, window: String, used: f64, resets_at: f64 },
    List,
}

pub type Emit = Arc<dyn Fn(AppEvent) + Send + Sync>;

pub struct EngineOptions {
    pub data_dir: PathBuf,
    pub clis: BTreeMap<Vendor, Cli>,
    pub spawn: Spawner,
    pub windows: bool,
    pub max_warm: usize,
    /// Where session and timer tasks run, so the engine can be called from any thread.
    pub runtime: tokio::runtime::Handle,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub area: Area,
    #[serde(default)]
    pub project_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub images: Vec<Image>,
    #[serde(default)]
    pub files: Vec<Attachment>,
    pub choice: Choice,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRequest {
    pub conversation_id: String,
    pub message_id: String,
    pub text: String,
    pub choice: Choice,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub area: Area,
    #[serde(default)]
    pub project_id: Option<String>,
    pub choice: Choice,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppState {
    pub conversations: Vec<Summary>,
    pub projects: Vec<Project>,
    pub vendors: Vec<Vendor>,
    pub settings: Settings,
}

/// How to continue a conversation in the CLI's own terminal interface.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCommand {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

struct Live {
    vendor: Vendor,
    model: Option<String>,
    effort: String,
    /// Tells this process's events apart from those of a replaced one.
    sid: u64,
    session: Session,
    /// The answer being written right now.
    turn: Option<String>,
    break_pending: bool,
    used: u64,
}

const HISTORY_LIMIT: usize = 20_000;

/// A model that joins a conversation midway reads what was said before, as plain text.
pub fn with_history(history: &[Message], text: &str) -> String {
    let lines = history
        .iter()
        .map(|m| format!("{}: {}", if m.role == Role::User { "User" } else { "Assistant" }, m.text))
        .collect::<Vec<_>>()
        .join("\n\n");
    let skip = lines.chars().count().saturating_sub(HISTORY_LIMIT);
    let lines: String = lines.chars().skip(skip).collect();
    format!("Earlier in this conversation, with another assistant:\n\n{lines}\n\nNow the user says:\n{text}")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

struct Core {
    opts: EngineOptions,
    store: Store,
    emit: Emit,
    me: Weak<Mutex<Core>>,
    /// Conversations in use, the source of truth while the app runs; every change is written through to disk.
    convs: HashMap<String, Conversation>,
    live: HashMap<String, Live>,
    /// A new conversation still being typed: its CLI starts before the first message is sent.
    draft: Option<String>,
    /// Streaming text goes to the screen at most ~30 times a second; the message waiting per conversation.
    pending: HashMap<String, String>,
    next_sid: u64,
    tick: u64,
}

/// A handle to the engine; clones share the same engine.
#[derive(Clone)]
pub struct Engine {
    core: Arc<Mutex<Core>>,
    runtime: tokio::runtime::Handle,
}

impl Engine {
    pub fn new(opts: EngineOptions, emit: Emit) -> Engine {
        let store = Store::new(&opts.data_dir);
        let runtime = opts.runtime.clone();
        let core = Arc::new_cyclic(|me| {
            Mutex::new(Core {
                opts,
                store,
                emit,
                me: me.clone(),
                convs: HashMap::new(),
                live: HashMap::new(),
                draft: None,
                pending: HashMap::new(),
                next_sid: 0,
                tick: 0,
            })
        });
        Engine { core, runtime }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Core) -> T) -> T {
        let _inside = self.runtime.enter();
        f(&mut self.core.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn vendors(&self) -> Vec<Vendor> {
        self.with(|c| c.vendors())
    }
    pub fn state(&self) -> AppState {
        self.with(|c| c.state())
    }
    pub fn save_settings(&self, patch: Settings) -> Settings {
        self.with(|c| c.save_settings(patch))
    }
    pub fn open(&self, id: &str) -> Option<Conversation> {
        self.with(|c| c.load(id).cloned())
    }
    pub fn send(&self, req: SendRequest) -> Result<String, String> {
        self.with(|c| c.send(req))
    }
    pub fn branch(&self, req: BranchRequest) -> Result<String, String> {
        self.with(|c| c.branch(req))
    }
    pub fn warm(&self, req: WarmRequest) {
        self.with(|c| c.warm(req))
    }
    pub fn interrupt(&self, id: &str) {
        self.with(|c| {
            if let Some(l) = c.live.get(id) {
                l.session.interrupt();
            }
        })
    }
    pub fn answer(&self, id: &str, request_id: &str, allow: bool, always: bool) {
        self.with(|c| c.answer(id, request_id, allow, always))
    }
    pub fn rename(&self, id: &str, title: &str) {
        self.with(|c| c.rename(id, title))
    }
    pub fn remove(&self, id: &str) {
        self.with(|c| c.remove(id))
    }
    pub fn terminal(&self, id: &str) -> Option<TerminalCommand> {
        self.with(|c| c.terminal(id))
    }
    /// The absolute path of a file in a chat workspace, or None if the path points outside it or is missing.
    pub fn workspace_file(&self, id: &str, path: &str) -> Option<PathBuf> {
        self.with(|c| c.store.get(id).is_some().then(|| inside_folder(&c.workspace_of(id), path)).flatten().filter(|p| p.is_file()))
    }
    pub fn add_project(&self, path: &str) -> Project {
        self.with(|c| c.add_project(path))
    }
    pub fn remove_project(&self, id: &str) {
        self.with(|c| {
            let projects: Vec<Project> = c.store.projects().into_iter().filter(|p| p.id != id).collect();
            c.store.save_projects(&projects);
            (c.emit)(AppEvent::List);
        })
    }
    pub fn shutdown(&self) {
        self.with(|c| {
            for (_, l) in c.live.drain() {
                l.session.close();
            }
        })
    }
}

impl Core {
    fn vendors(&self) -> Vec<Vendor> {
        self.opts.clis.keys().copied().collect()
    }

    // Each chat conversation gets a private workspace folder: code runs and files are created there, never elsewhere.
    fn workspace_of(&self, id: &str) -> PathBuf {
        self.opts.data_dir.join("chat").join(id)
    }

    fn cwd_of(&self, c: &Conversation) -> Result<PathBuf, String> {
        if c.area == Area::Chat {
            let dir = self.workspace_of(&c.id);
            let _ = std::fs::create_dir_all(&dir);
            return Ok(dir);
        }
        self.store
            .projects()
            .into_iter()
            .find(|p| Some(&p.id) == c.project_id.as_ref())
            .map(|p| PathBuf::from(p.path))
            .ok_or_else(|| "error.projectGone".to_string())
    }

    fn load(&mut self, id: &str) -> Option<&mut Conversation> {
        if !self.convs.contains_key(id) {
            let c = self.store.get(id)?;
            self.convs.insert(id.to_string(), c);
        }
        self.convs.get_mut(id)
    }

    fn save(&self, id: &str) {
        if let Some(c) = self.convs.get(id) {
            self.store.save(c);
        }
    }

    fn state(&self) -> AppState {
        AppState {
            conversations: self.store.list().into_iter().filter(|c| !c.title.is_empty()).collect(),
            projects: self.store.projects(),
            vendors: self.vendors(),
            settings: self.store.settings(),
        }
    }

    fn save_settings(&mut self, patch: Settings) -> Settings {
        let before = self.store.settings();
        let settings = Settings {
            appearance: patch.appearance.or(before.appearance),
            user_hooks: patch.user_hooks.or(before.user_hooks),
            language: patch.language.or(before.language),
        };
        self.store.save_settings(&settings);
        // Idle project CLIs restart on their next turn, so a changed hooks choice applies right away.
        if settings.user_hooks != before.user_hooks {
            let idle: Vec<String> = self
                .live
                .iter()
                .filter(|(id, l)| l.turn.is_none() && self.convs.get(*id).is_some_and(|c| c.area == Area::Project))
                .map(|(id, _)| id.clone())
                .collect();
            for id in idle {
                if let Some(l) = self.live.remove(&id) {
                    l.session.close();
                }
            }
        }
        settings
    }

    fn turn(&mut self, conv: &str) -> Option<&mut Message> {
        let id = self.live.get(conv)?.turn.clone()?;
        self.convs.get_mut(conv)?.messages.iter_mut().find(|m| m.id == id)
    }

    fn handle(&mut self, conv: &str, sid: u64, event: EngineEvent) {
        let Some(entry) = self.live.get(conv) else { return };
        if entry.sid != sid {
            return;
        }
        let vendor = entry.vendor;
        let turn_id = entry.turn.clone();
        match event {
            EngineEvent::Started { session_id, model } => {
                if let Some(m) = self.turn(conv) {
                    m.model = Some(model);
                }
                let Some(c) = self.convs.get_mut(conv) else { return };
                c.sessions.insert(vendor, session_id);
                // a draft is saved only once something is sent
                if !c.messages.is_empty() {
                    self.save(conv);
                }
                return;
            }
            EngineEvent::Break => {
                let has_text = self.turn(conv).is_some_and(|m| !m.text.is_empty());
                if let (true, Some(l)) = (has_text, self.live.get_mut(conv)) {
                    l.break_pending = true;
                }
                return;
            }
            EngineEvent::Text(delta) => {
                let pending = self.live.get(conv).is_some_and(|l| l.break_pending);
                let Some(m) = self.turn(conv) else { return };
                if pending && !delta.is_empty() {
                    m.text.push_str("\n\n");
                }
                m.text.push_str(&delta);
                if pending
                    && !delta.is_empty()
                    && let Some(l) = self.live.get_mut(conv)
                {
                    l.break_pending = false;
                }
            }
            EngineEvent::Thinking(delta) => {
                if let Some(m) = self.turn(conv) {
                    m.thinking.get_or_insert_default().push_str(&delta);
                }
            }
            EngineEvent::Tool { id, name, input } => {
                if let Some(m) = self.turn(conv) {
                    m.tools.get_or_insert_default().push(ToolUse { id, name, input, ok: None, output: None });
                }
            }
            EngineEvent::ToolResult { id, ok, text } => {
                if let Some(tool) = self.turn(conv).and_then(|m| m.tools.as_mut()).and_then(|t| t.iter_mut().find(|t| t.id == id)) {
                    tool.ok = Some(ok);
                    tool.output = Some(text.chars().take(4000).collect());
                }
            }
            EngineEvent::Permission { request_id, tool, input } => {
                let allowed = self.convs.get(conv).and_then(|c| c.allow_always.as_ref()).is_some_and(|a| a.contains(&tool));
                if allowed {
                    if let Some(l) = self.live.get(conv) {
                        l.session.answer(&request_id, true);
                    }
                    return;
                }
                if let Some(m) = self.turn(conv) {
                    m.permissions.get_or_insert_default().push(Permission { request_id, tool, input });
                }
                return self.push(conv, turn_id.as_deref(), true);
            }
            EngineEvent::Quota { window, used, resets_at } => {
                return (self.emit)(AppEvent::Quota { vendor, window, used, resets_at });
            }
            EngineEvent::Done { ok, cost_usd, input_tokens, output_tokens, .. } => {
                if let Some(m) = self.turn(conv) {
                    m.status = Some(if ok { Status::Done } else { Status::Stopped });
                    m.cost_usd = cost_usd;
                    m.input_tokens = Some(input_tokens);
                    m.output_tokens = Some(output_tokens);
                    m.permissions = None;
                }
                return self.finish(conv, turn_id);
            }
            EngineEvent::Error { message, fatal } => {
                if let Some(m) = self.turn(conv) {
                    m.error = Some(message);
                    if fatal {
                        m.status = Some(Status::Error);
                        m.permissions = None;
                    }
                }
                if fatal {
                    self.live.remove(conv);
                    return self.finish(conv, turn_id);
                }
            }
        }
        self.push(conv, turn_id.as_deref(), false);
    }

    fn finish(&mut self, conv: &str, turn_id: Option<String>) {
        if let Some(l) = self.live.get_mut(conv) {
            l.turn = None;
        }
        let workspace = self.workspace_of(conv);
        let Some(c) = self.convs.get_mut(conv) else { return };
        if let Some(m) = turn_id.as_ref().and_then(|id| c.messages.iter_mut().find(|m| &m.id == id))
            && c.area == Area::Chat
        {
            let files = new_files(&workspace, m.at);
            if !files.is_empty() {
                m.files = Some(files);
            }
        }
        c.updated_at = now();
        self.save(conv);
        self.push(conv, turn_id.as_deref(), true);
        (self.emit)(AppEvent::List);
    }

    fn push(&mut self, conv: &str, message_id: Option<&str>, immediate: bool) {
        let Some(message_id) = message_id else { return };
        if immediate {
            self.pending.remove(conv);
            self.emit_message(conv, message_id);
            return;
        }
        if self.pending.contains_key(conv) {
            return;
        }
        self.pending.insert(conv.to_string(), message_id.to_string());
        let me = self.me.clone();
        let conv = conv.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(33)).await;
            if let Some(core) = me.upgrade() {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(id) = core.pending.remove(&conv) {
                    core.emit_message(&conv, &id);
                }
            }
        });
    }

    fn emit_message(&self, conv: &str, message_id: &str) {
        if let Some(m) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == message_id)) {
            (self.emit)(AppEvent::Message { conversation_id: conv.to_string(), message: Box::new(m.clone()) });
        }
    }

    fn launch(&mut self, conv: &str, pick: &Pick) -> Result<(), String> {
        self.tick += 1;
        if let Some(l) = self.live.get_mut(conv) {
            if l.vendor == pick.vendor && l.model == pick.model && l.effort == pick.effort {
                l.used = self.tick; // most recently used
                return Ok(());
            }
            l.session.close();
            self.live.remove(conv);
        }
        let c = self.convs.get(conv).ok_or("error.conversationGone")?;
        let cwd = self.cwd_of(c)?;
        let cli = self.opts.clis.get(&pick.vendor).ok_or("error.notConnected")?.clone();
        let base_spawn = self.opts.spawn.clone();
        let prefix = cli.prefix.clone();
        let command = cli.command.clone();
        let spawn: Spawner = Arc::new(move |_bin, args, cwd| {
            let all: Vec<String> = prefix.iter().cloned().chain(args.iter().cloned()).collect();
            base_spawn(&command, &all, cwd)
        });
        self.next_sid += 1;
        let sid = self.next_sid;
        let me = self.me.clone();
        let key = conv.to_string();
        let sink: EventSink = Arc::new(move |e| {
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).handle(&key, sid, e);
            }
        });
        let user_hooks = self.store.settings().user_hooks == Some(true);
        let resume = c.sessions.get(&pick.vendor).cloned();
        let session = match pick.vendor {
            Vendor::Claude => start_claude(
                ClaudeOptions {
                    bin: cli.command,
                    cwd,
                    launch: ClaudeLaunch { area: c.area, model: pick.model.clone(), effort: Some(pick.effort.clone()), resume, user_hooks },
                    spawn,
                },
                sink,
            ),
            Vendor::Codex => start_codex(
                CodexOptions {
                    bin: cli.command,
                    cwd,
                    launch: CodexLaunch { area: c.area, model: pick.model.clone(), effort: Some(pick.effort.clone()), resume, user_hooks },
                    windows: self.opts.windows,
                    spawn,
                },
                sink,
            ),
        };
        self.live.insert(
            conv.to_string(),
            Live {
                vendor: pick.vendor,
                model: pick.model.clone(),
                effort: pick.effort.clone(),
                sid,
                session,
                turn: None,
                break_pending: false,
                used: self.tick,
            },
        );
        // ponytail: count-based warm limit; switch to the visible RAM budget once memory is measured.
        while self.live.len() > self.opts.max_warm {
            let Some(oldest) =
                self.live.iter().filter(|(_, l)| l.turn.is_none() && l.sid != sid).min_by_key(|(_, l)| l.used).map(|(id, _)| id.clone())
            else {
                break;
            };
            if let Some(l) = self.live.remove(&oldest) {
                l.session.close();
            }
        }
        Ok(())
    }

    fn blank(&mut self, area: Area, project_id: Option<String>) -> String {
        let at = now();
        let c = Conversation {
            id: new_id(),
            title: String::new(),
            area,
            project_id,
            sessions: BTreeMap::new(),
            allow_always: None,
            branch_of: None,
            created_at: at,
            updated_at: at,
            messages: vec![],
        };
        let id = c.id.clone();
        self.convs.insert(id.clone(), c);
        id
    }

    fn send(&mut self, req: SendRequest) -> Result<String, String> {
        let pick = pick_model(&req.choice, &self.vendors()).ok_or("error.noSubscription")?;
        let existing = req.conversation_id.as_deref().and_then(|id| self.load(id).map(|c| c.id.clone()));
        if let Some(id) = &existing
            && self.live.get(id).is_some_and(|l| l.turn.is_some())
        {
            return Err("error.busy".into());
        }
        let draft = self.draft.clone().filter(|d| self.convs.get(d).is_some_and(|c| c.area == req.area && c.project_id == req.project_id));
        let id = match existing {
            Some(id) => id,
            None => match draft {
                Some(d) => {
                    self.draft = None;
                    d
                }
                None => self.blank(req.area, req.project_id.clone()),
            },
        };
        let attachments_dir = if req.area == Area::Chat {
            self.workspace_of(&id).join("attachments")
        } else {
            self.opts.data_dir.join("attachments").join(&id)
        };
        let at = now();
        let c = self.convs.get_mut(&id).ok_or("error.conversationGone")?;
        if c.title.is_empty() {
            c.title = req.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
        }
        let joining_late = !c.sessions.contains_key(&pick.vendor) && !c.messages.is_empty();
        let history = c.messages.clone();
        // Attached files are saved where the model can open them with its own tools (PDFs, spreadsheets, archives...).
        let saved = if req.files.is_empty() { vec![] } else { save_files(&attachments_dir, &req.files) };
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let shown: Vec<String> = saved
            .iter()
            .map(|p| if c.area == Area::Chat { format!("attachments/{}", name(p)) } else { p.to_string_lossy().into_owned() })
            .collect();
        let text = if shown.is_empty() {
            req.text.clone()
        } else {
            let place = if c.area == Area::Chat { " (in your working folder)" } else { "" };
            format!("{}\n\nAttached files{place}:\n{}", req.text, shown.iter().map(|p| format!("- {p}")).collect::<Vec<_>>().join("\n"))
        };
        let mut question = Message::new(new_id(), Role::User, req.text.clone(), at);
        question.images = (!req.images.is_empty()).then(|| req.images.clone());
        question.attachments = Some(saved.iter().map(|p| name(p)).collect());
        let mut reply = Message::new(new_id(), Role::Assistant, String::new(), at);
        reply.vendor = Some(pick.vendor);
        reply.model = pick.model.clone();
        reply.reason = Some(pick.reason.clone());
        reply.tools = Some(vec![]);
        reply.status = Some(Status::Streaming);
        let reply_id = reply.id.clone();
        c.messages.push(question);
        c.messages.push(reply);
        c.updated_at = at;
        self.save(&id);
        (self.emit)(AppEvent::List);
        self.launch(&id, &pick)?;
        if let Some(l) = self.live.get_mut(&id) {
            l.turn = Some(reply_id.clone());
            let text = if joining_late { with_history(&history, &text) } else { text };
            l.session.send(UserTurn { text, images: req.images });
        }
        self.push(&id, Some(&reply_id), true);
        Ok(id)
    }

    // Edit a message or redo an answer with another model: a new conversation keeps everything before that message,
    // with the chat workspace copied, and sends the new text. The original stays as it was.
    fn branch(&mut self, req: BranchRequest) -> Result<String, String> {
        let source = self.load(&req.conversation_id).cloned().ok_or("error.messageGone")?;
        let at = source.messages.iter().position(|m| m.id == req.message_id).ok_or("error.messageGone")?;
        let id = self.blank(source.area, source.project_id.clone());
        let from = self.workspace_of(&source.id);
        let to = self.workspace_of(&id);
        let c = self.convs.get_mut(&id).expect("just created");
        c.title = format!("↳ {}", source.title.trim_start_matches("↳ "));
        c.branch_of = Some(BranchOf { conversation_id: source.id.clone(), message_id: req.message_id.clone() });
        c.messages = source.messages[..at]
            .iter()
            .cloned()
            .map(|mut m| {
                m.permissions = None;
                if m.status == Some(Status::Streaming) {
                    m.status = Some(Status::Stopped);
                }
                m
            })
            .collect();
        if source.area == Area::Chat && from.is_dir() {
            copy_dir(&from, &to);
        }
        self.save(&id);
        self.send(SendRequest {
            conversation_id: Some(id),
            area: source.area,
            project_id: source.project_id,
            text: req.text,
            images: vec![],
            files: vec![],
            choice: req.choice,
        })
    }

    // Starts the CLI before the user finishes typing, so the first word arrives sooner.
    fn warm(&mut self, req: WarmRequest) {
        let Some(pick) = pick_model(&req.choice, &self.vendors()) else { return };
        let existing = req.conversation_id.as_deref().and_then(|id| self.load(id).map(|c| c.id.clone()));
        let id = match existing {
            Some(id) => id,
            None => {
                let same = self
                    .draft
                    .as_ref()
                    .and_then(|d| self.convs.get(d))
                    .is_some_and(|c| c.area == req.area && c.project_id == req.project_id);
                if !same {
                    if let Some(old) = self.draft.take() {
                        if let Some(l) = self.live.remove(&old) {
                            l.session.close();
                        }
                        self.convs.remove(&old);
                        let _ = std::fs::remove_dir_all(self.workspace_of(&old));
                    }
                    self.draft = Some(self.blank(req.area, req.project_id.clone()));
                }
                self.draft.clone().expect("draft set above")
            }
        };
        if !self.live.get(&id).is_some_and(|l| l.turn.is_some()) {
            let _ = self.launch(&id, &pick);
        }
    }

    fn answer(&mut self, conv: &str, request_id: &str, allow: bool, always: bool) {
        let Some(turn_id) = self.live.get(conv).and_then(|l| l.turn.clone()) else { return };
        let Some(m) = self.turn(conv) else { return };
        let waiting = m.permissions.clone().unwrap_or_default();
        let tool = waiting.iter().find(|p| p.request_id == request_id).map(|p| p.tool.clone());
        // "Always" also answers the other waiting requests for the same tool.
        let answered: Vec<String> = match (&tool, always && allow) {
            (Some(tool), true) => waiting.iter().filter(|p| &p.tool == tool).map(|p| p.request_id.clone()).collect(),
            _ => waiting.iter().filter(|p| p.request_id == request_id).map(|p| p.request_id.clone()).collect(),
        };
        m.permissions = Some(waiting.into_iter().filter(|p| !answered.contains(&p.request_id)).collect());
        if let (Some(tool), true) = (tool, always && allow)
            && let Some(c) = self.convs.get_mut(conv)
        {
            let list = c.allow_always.get_or_insert_default();
            if !list.contains(&tool) {
                list.push(tool);
            }
            self.save(conv);
        }
        if let Some(l) = self.live.get(conv) {
            for id in &answered {
                l.session.answer(id, allow);
            }
        }
        self.push(conv, Some(&turn_id), true);
    }

    fn rename(&mut self, id: &str, title: &str) {
        let Some(c) = self.load(id) else { return };
        let title: String = title.trim().chars().take(120).collect();
        if !title.is_empty() {
            c.title = title;
        }
        self.save(id);
        (self.emit)(AppEvent::List);
    }

    fn remove(&mut self, id: &str) {
        if let Some(l) = self.live.remove(id) {
            l.session.close();
        }
        if self.store.get(id).is_some() || self.convs.contains_key(id) {
            self.store.remove(id);
            self.convs.remove(id);
            let _ = std::fs::remove_dir_all(self.workspace_of(id));
            let _ = std::fs::remove_dir_all(self.opts.data_dir.join("attachments").join(id));
        }
        (self.emit)(AppEvent::List);
    }

    fn terminal(&mut self, id: &str) -> Option<TerminalCommand> {
        let c = self.load(id)?.clone();
        let vendor = c.messages.iter().rev().find_map(|m| m.vendor)?;
        let session = c.sessions.get(&vendor)?.clone();
        // Session ids are UUIDs; anything else must never reach a shell.
        if !session.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
            return None;
        }
        let cli = self.opts.clis.get(&vendor)?.clone();
        let resume = match vendor {
            Vendor::Claude => vec!["--resume".to_string(), session],
            Vendor::Codex => vec!["resume".to_string(), session],
        };
        Some(TerminalCommand { command: cli.command, args: cli.prefix.into_iter().chain(resume).collect(), cwd: self.cwd_of(&c).ok()? })
    }

    fn add_project(&mut self, path: &str) -> Project {
        let mut projects = self.store.projects();
        if let Some(found) = projects.iter().find(|p| p.path == path) {
            return found.clone();
        }
        let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
        let project = Project { id: new_id(), name, path: path.to_string() };
        projects.push(project.clone());
        self.store.save_projects(&projects);
        (self.emit)(AppEvent::List);
        project
    }
}

fn copy_dir(from: &Path, to: &Path) {
    let _ = std::fs::create_dir_all(to);
    for entry in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            let _ = std::fs::copy(entry.path(), target);
        }
    }
}
