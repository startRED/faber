//! Conversations a project's agent starts by itself (`crate::sibling`): the engine makes a new conversation in the same
//! project, sends it the agent's brief as its first message on the conversation's own model choice, and answers the call
//! at once with the new id. In a project that keeps separate copies (R201) the brief waits for the new conversation's own
//! copy, made away from the engine's lock like the person's own first message, so it never runs in the project folder. A
//! read answers with where that conversation stands and its latest answer, or with a page of numbered messages.
use super::*;
pub(super) use crate::conversation_state::state_of;
use crate::events::MAX_FAILURE_SCAN_BYTES;
use crate::history::{Headers, Page, ProjectHistory};
use crate::progress;
use crate::sibling::{self, Ai, Request, State};
use crate::store::{ChildOnStart, HistoryVersion};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};

const HIDDEN_LINE: &str = "[sensitive diagnostic line hidden]";
const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;
const DIAGNOSTIC_CHARS: usize = 1500;
const HISTORY_OUTSIDE_PROJECT: &str = "no conversation with that id is in this project";

enum ProjectPage<T> {
    Open(T),
    Saved(SavedProjectPage),
}

struct SavedProjectPage {
    store: Arc<Store>,
    project: String,
    id: String,
    start: usize,
    limit: usize,
}

impl SavedProjectPage {
    fn read(&self) -> Option<Page<'static>> {
        self.store.history_page(&self.id, self.start, self.limit, Some(&self.project))
    }

    fn current<'a>(&self, core: &'a Core, reader: &str, page: Option<Page<'static>>) -> Result<Page<'a>, &'static str> {
        if let Some(c) = self.current_target(core, reader)? {
            return Ok(Page::borrowed(c, self.start, self.limit));
        }
        let page = page.ok_or(HISTORY_OUTSIDE_PROJECT)?;
        core.store.history_is_current(&page).then_some(page).ok_or("the saved conversation changed; read it again")
    }

    fn read_headers(&self, role: Option<Role>) -> Option<(Headers, HistoryVersion)> {
        self.store.history_headers(&self.id, self.start, self.limit, role, Some(&self.project))
    }

    fn current_headers(
        &self,
        core: &Core,
        reader: &str,
        role: Option<Role>,
        saved: Option<(Headers, HistoryVersion)>,
    ) -> Result<Headers, &'static str> {
        if let Some(c) = self.current_target(core, reader)? {
            return Ok(Headers::of(c, self.start, self.limit, role));
        }
        let (headers, version) = saved.ok_or(HISTORY_OUTSIDE_PROJECT)?;
        core.store.history_version_is_current(&self.id, version).then_some(headers).ok_or("the saved conversation changed; read it again")
    }

    fn current_target<'a>(&self, core: &'a Core, reader: &str) -> Result<Option<&'a Conversation>, &'static str> {
        if core.history_project(reader) != Some(self.project.as_str()) {
            return Err(HISTORY_OUTSIDE_PROJECT);
        }
        // If the person opened or moved it while reading the file, use the current in-memory scope and history.
        core.open_project_conversation(&self.id, &self.project).transpose()
    }
}

impl Engine {
    pub fn project_command_results(&self, reader: &str, input: &Value) -> Result<Value, &'static str> {
        let Request::CommandResults { id, tool, limit, running_only } = sibling::request(input)? else {
            return Err("the request must read command results");
        };
        self.read_command_results(reader, id.as_deref().unwrap_or(reader), tool.as_deref(), limit, running_only, |_, result| result)
    }

    fn read_command_results<T>(
        &self,
        reader: &str,
        id: &str,
        tool: Option<&str>,
        limit: usize,
        running_only: bool,
        deliver: impl FnOnce(&Core, Result<Value, &'static str>) -> T,
    ) -> T {
        let Some((project, store)) = self.with(|core| {
            let project = core.history_project(reader)?.to_string();
            let store = (!core.convs.contains_key(id)).then(|| core.store.clone());
            Some((project, store))
        }) else {
            return self.with(|core| deliver(core, Err("this conversation has no project")));
        };
        let saved = store.and_then(|store| store.history_page(id, 0, 0, Some(&project)));
        let authorize = |core: &Core| {
            if core.closing || core.history_project(reader) != Some(project.as_str()) {
                return Err(HISTORY_OUTSIDE_PROJECT);
            }
            let starter = match core.open_project_conversation(id, &project).transpose()? {
                Some(c) => c.started_by.as_deref(),
                None => {
                    let page = saved.as_ref().filter(|page| core.store.history_is_current(page)).ok_or(HISTORY_OUTSIDE_PROJECT)?;
                    page.started_by.as_deref()
                }
            };
            if reader != id && starter != Some(reader) {
                return Err("command results are available to their conversation or its direct starter");
            }
            Ok(())
        };
        let found = self.with(|core| {
            authorize(core)?;
            let rows = if running_only { core.background.running(id, tool, limit) } else { core.background.recent(id, tool, limit) };
            Ok((core.background.finished_version(), rows))
        });
        let found = found.and_then(|(version, rows)| {
            if tool.is_some() && rows.is_empty() {
                return Err(if running_only {
                    "no running command with that id was found"
                } else {
                    "no result with that id remains in this Faber run"
                });
            }
            Ok((version, command_result_rows(id, rows, tool.is_some(), running_only)))
        });
        self.with(|core| {
            let result = authorize(core).and(found).and_then(|(version, result)| {
                (core.background.finished_version() == version).then_some(result).ok_or("recent results changed; read them again")
            });
            deliver(core, result)
        })
    }

    /// Read one project message's tool steps under the same scope and bounds as the session's conversation tool.
    pub fn project_steps(&self, reader: &str, input: &Value) -> Result<Value, &'static str> {
        let Request::ReadSteps { id, from, limit, offset, tool, failed_only } = sibling::request(input)? else {
            return Err("the request must read steps");
        };
        let id = id.as_deref().unwrap_or(reader);
        let render = |page: Page<'_>| Core::sibling_steps(&page, reader, from, limit, offset, tool.as_deref(), failed_only);
        let page = self.with(|core| -> Result<ProjectPage<Value>, &'static str> {
            Ok(match core.project_page(reader, id, from - 1, 1)? {
                ProjectPage::Open(page) => ProjectPage::Open(render(page)?),
                ProjectPage::Saved(read) => ProjectPage::Saved(read),
            })
        })?;
        match page {
            ProjectPage::Open(result) => Ok(result),
            ProjectPage::Saved(read) => {
                let saved = read.read();
                self.with(|core| read.current(core, reader, saved).and_then(render))
            }
        }
    }
}

fn command_result_rows(id: &str, rows: Vec<crate::background::CommandRecord>, details: bool, running_only: bool) -> Value {
    let rows: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let output_available = !row.output_truncated && !row.output.trim().is_empty();
            let mut result = json!({
                "id": row.id, "vendor": row.vendor, "started": row.started, "ended": row.ended,
                "state": row.state.map_or(json!("unknown"), |state| json!(state)),
                "outputAvailable": output_available, "outputTruncated": row.output_truncated,
                "terminalOutput": row.terminal_output,
            });
            if let Some(command) = row.command {
                result["label"] = json!(masked_diagnostic(&command, MAX_DIAGNOSTIC_BYTES));
            }
            if details && output_available {
                let masked = masked_diagnostic(&row.output, MAX_DIAGNOSTIC_BYTES);
                let start = masked.char_indices().rev().nth(DIAGNOSTIC_CHARS - 1).map_or(0, |(at, _)| at);
                result["output"] = json!(&masked[start..]);
                result["outputTruncated"] = json!(start > 0);
            }
            result
        })
        .collect();
    let scope = if running_only { "runningInThisFaberRun" } else { "recentInThisFaberRun" };
    json!({"id": id, "scope": scope, "items": rows})
}

fn plain_diagnostic_line(line: &str) -> Option<Cow<'_, str>> {
    if !line.contains(['\x1b', '\r']) {
        return Some(Cow::Borrowed(line));
    }
    let mut plain = String::with_capacity(line.len());
    let mut rest = line;
    while let Some((before, escape)) = rest.split_once('\x1b') {
        plain.push_str(before);
        let params = escape.strip_prefix('[')?;
        let end = params.find(|ch: char| !(ch.is_ascii_digit() || matches!(ch, ';' | ':' | '?')))?;
        let (args, terminal) = params.split_at(end);
        rest = match terminal.as_bytes().first()? {
            b'm' if !args.contains('?') => &terminal[1..],
            b'J' | b'K' if matches!(args, "" | "0" | "1" | "2") => &terminal[1..],
            b'A'..=b'G' if args.bytes().all(|b| b.is_ascii_digit()) => &terminal[1..],
            b'H' | b'f' if args.split(';').count() <= 2 && args.bytes().all(|b| b.is_ascii_digit() || b == b';') => &terminal[1..],
            b'h' | b'l' if args == "?25" => &terminal[1..],
            _ => return None,
        };
    }
    plain.push_str(rest);
    // Remove presentation controls without discarding any text that might carry a credential label.
    plain.retain(|ch| ch != '\r');
    Some(Cow::Owned(plain))
}

fn credential_assignment(line: &str) -> bool {
    line.match_indices([':', '=']).any(|(end, _)| {
        let key = line[..end].trim_end().trim_end_matches(['\'', '"']);
        [
            "authorization",
            "auth",
            "api key",
            "api-key",
            "api_key",
            "apikey",
            "token",
            "password",
            "passwd",
            "cookie",
            "credentials",
            "credential",
            "secret",
        ]
        .iter()
        .any(|field| key.strip_suffix(*field).is_some_and(|before| before.chars().next_back().is_none_or(|ch| !ch.is_ascii_alphanumeric())))
    })
}

fn masked_diagnostic(text: &str, limit_bytes: usize) -> String {
    if text.len() > limit_bytes {
        return "[oversized diagnostic hidden]".into();
    }
    let plain = text.lines().map(|line| plain_diagnostic_line(line).unwrap_or(Cow::Borrowed(HIDDEN_LINE))).collect::<Vec<_>>().join("\n");
    let text = crate::browser::mask::mask(&plain);
    let mut hide_continuation = false;
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let url_user_info = line.split("://").skip(1).any(|rest| {
                rest.split(['/', '\\', '?', '#', ' ', '\t', '\'', '"', '<', '>']).next().is_some_and(|authority| authority.contains('@'))
            });
            let sensitive = line == HIDDEN_LINE
                || credential_assignment(&lower)
                || lower.contains("bearer ")
                || lower.contains("bearer\t")
                || url_user_info
                || line.chars().any(|ch| ch.is_control() && ch != '\t');
            let continued = hide_continuation && line.starts_with([' ', '\t']);
            hide_continuation = sensitive || continued;
            if hide_continuation { HIDDEN_LINE } else { line }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn retained_failure_tail(text: &str) -> Option<Arc<str>> {
    if text.len() > MAX_FAILURE_SCAN_BYTES || text.chars().nth(TOOL_OUTPUT_CHARS).is_none() {
        return None;
    }
    // Mask before cutting so a private-key block or folded credential can start before the excerpt.
    let masked = masked_diagnostic(text, MAX_FAILURE_SCAN_BYTES);
    let from = masked.char_indices().rev().nth(DIAGNOSTIC_CHARS - 1).map_or(0, |(at, _)| at);
    let tail = &masked[from..];
    (!tail.trim().is_empty()).then(|| tail.into())
}

fn failed_step(call: &ToolUse) -> bool {
    call.ok == Some(false) && !call.declined && !call.interrupted
}

/// Whether a conversation's agent gets the tool: every project conversation's. Only one no other agent started may start
/// conversations, so one level only.
/// One read of another conversation (`Core::sibling_reads`): the reader's turn, the answer's fingerprint, and how full
/// its context was then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SeenRead {
    turn: String,
    answer: u64,
    used: u64,
}

struct ReplyPreview {
    title: String,
    state: State,
    fingerprint: Option<u64>,
    text: String,
}

impl ReplyPreview {
    fn new(c: &Conversation, state: State) -> Self {
        let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
        let latest = match last {
            Some(m) if m.wrap_up.is_some() => Cow::Owned(m.said()),
            Some(m) => Cow::Borrowed(m.text.as_str()),
            None => Cow::Borrowed(""),
        };
        Self::from_words(&c.title, state, last.map(|m| m.id.as_str()), &latest)
    }

    fn from_words(title: &str, state: State, id: Option<&str>, latest: &str) -> Self {
        let fingerprint = id.filter(|_| !latest.trim().is_empty()).map(|id| {
            let mut hasher = DefaultHasher::new();
            (id, latest).hash(&mut hasher);
            hasher.finish()
        });
        Self { title: title.to_string(), state, fingerprint, text: sibling::read(title, state, latest) }
    }
}

type SavedPreviews = HashMap<String, (HistoryVersion, ReplyPreview)>;

pub(super) fn offers(area: Area) -> bool {
    area == Area::Project
}

/// What the agent of a standing goal's fix is told once the project's day is out of money (`goals.rs`); said in a sentence the
/// call's answer completes ("No conversation was started: ...").
pub(super) const GOAL_CAP: &str = "the day's limit for fixes of the project's goals is used up, so this fix stops here for today";

/// A member whose lead hears from it again without anyone acting: it works, it waits for conversations it started, or a
/// question card of its turn is still open. One whose turn ended asking for the person (its closing box says blocked or
/// needs you) is not: nobody is told unless the person happens to open it, and its lead waited for ever (the owner's
/// project, 2026-11-11).
pub(super) fn reports_later(state: State, c: &Conversation) -> bool {
    match state {
        State::Working => true,
        State::Waiting => {
            c.waiting_on.is_some()
                || c.messages
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::Assistant)
                    .is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()))
        }
        _ => false,
    }
}

/// Where a plan step's conversation stands (task 040), in a subagent row's words.
pub(super) fn member_state(state: State) -> MemberState {
    match state {
        State::Working => MemberState::Running,
        State::Waiting => MemberState::Waiting,
        State::Concluded => MemberState::Done,
        // A turn that ended with no conclusion does not finish its step; a stuck run is still spending.
        State::Unconcluded => MemberState::Stopped,
        State::Stuck => MemberState::Running,
        State::Failed => MemberState::Failed,
        State::Stopped => MemberState::Stopped,
        // Cut off when Faber closed: not running, and a resume picks it up; the plan says stopped.
        State::Interrupted => MemberState::Stopped,
    }
}

/// Whether the report `id` already sits in the lead's queue or conversation, alone or merged with others (`Wake::ids`).
fn delivered(p: &Conversation, id: &str) -> bool {
    p.queued.iter().any(|q| Wake::carries(&q.id, q.wake.as_ref(), id))
        || p.messages.iter().any(|m| Wake::carries(&m.id, m.wake.as_ref(), id))
}

impl Core {
    /// Reports owed when Faber opens: those saved as owed (a close that cut a member's turn off marks it, `shutdown`), and
    /// those of members a crash cut off (their reply still streaming on disk), which no turn end will ever report. One sent
    /// before is not sent again (its id). With them, from the same pass, each leader none of whose members will wake it (none
    /// owes a report, is on Keep going or has a turn waiting) and its members' ids, for `resume_cut_leaders`.
    pub(super) fn restore_parent_reports(
        store: &Store,
        scan: &StartScan,
    ) -> (HashMap<String, HashSet<String>>, HashMap<String, Vec<String>>) {
        let mut pending: HashMap<String, HashSet<String>> = HashMap::new();
        let mut teams: HashMap<String, (bool, Vec<String>)> = HashMap::new();
        for id in &scan.started_by {
            let Some((parent, state)) = store.child_on_start(id) else { continue };
            let team = teams.entry(parent.clone()).or_default();
            team.0 |= state != ChildOnStart::Quiet;
            team.1.push(id.clone());
            if state == ChildOnStart::OwesReport {
                pending.entry(parent).or_default().insert(id.clone());
            }
        }
        for parent in &scan.waiting_on {
            let Some(waiting) = store.get(parent).and_then(|c| c.waiting_on) else { continue };
            for id in waiting.ids {
                let Some(mut child) = store.get(&id) else { continue };
                child.messages.iter_mut().for_each(settle);
                if child.keep_going.is_none() && !reports_later(state_of(false, &child), &child) {
                    pending.entry(parent.clone()).or_default().insert(id);
                }
            }
        }
        let quiet = teams.into_iter().filter(|(_, (woken, _))| !woken).map(|(leader, (_, members))| (leader, members)).collect();
        (pending, quiet)
    }

    pub(super) fn refill_parent_reports(&mut self, parent: &str) {
        if !self.parent_reports.contains_key(parent) {
            return;
        }
        if self.load(parent).is_none_or(|c| c.queued.len() >= MAX_QUEUED) {
            return;
        }
        // Remove the batch while sending: emit_queue can reenter here after each admitted report.
        let pending = self.parent_reports.remove(parent).unwrap_or_default();
        // The reports wait in the queue together and go out as one turn (`merge_reports`), never one turn each.
        let collect = self.batch_reports && pending.len() > 1 && self.collecting.insert(parent.to_string());
        for child in pending {
            if self.convs.get(parent).is_none_or(|c| c.queued.len() >= MAX_QUEUED) {
                self.parent_reports.entry(parent.into()).or_default().insert(child);
                continue;
            }
            let owed = self.load(&child).is_some_and(|c| c.pending_parent_report);
            self.turn_ended(&child);
            let pending = self.convs.get(&child).is_some_and(|c| {
                if c.started_by.as_deref() == Some(parent) {
                    c.pending_parent_report
                } else {
                    self.convs.get(parent).and_then(|p| p.waiting_on.as_ref()).is_some_and(|w| w.ids.contains(&child))
                }
            });
            if pending {
                self.parent_reports.entry(parent.into()).or_default().insert(child);
            } else if !owed {
                // A crash left its reply streaming on disk: saved settled, it is not found again at the next start.
                self.save(&child);
            }
        }
        if collect {
            self.collecting.remove(parent);
            self.hand_over_queue(parent, true);
            self.drain(parent);
        }
    }

    /// The agent called the conversation tool. Workspace inspection and saved history pages read off the engine lock.
    pub(super) fn conversation_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        let request = sibling::request(&input);
        let is_start = matches!(request, Ok(Request::Start { .. }));
        let (ok, text) = match request {
            Err(why) => (false, format!("Nothing was done: {why}.")),
            Ok(Request::Start { title, brief, ai, model, effort, work, keep_going, step }) => {
                match self.session_start(conv, NewSession { title: title.clone(), brief, ai, model, effort, work, keep_going, step }) {
                    Ok(id) => (true, sibling::started(&title, &id)),
                    Err(why) => (false, format!("No conversation was started: {why}.")),
                }
            }
            Ok(Request::Read { id }) => {
                self.read_siblings_tool(conv, call, vec![id], false);
                return;
            }
            Ok(Request::ReadMessages { id, from, limit, offset }) => {
                self.read_history_tool(conv, call, id.unwrap_or_else(|| conv.into()), from, limit, move |page, _| {
                    Ok(Self::sibling_messages(page, from, limit, offset))
                });
                return;
            }
            Ok(Request::ReadHeaders { id, from, limit, role }) => {
                self.read_headers_tool(conv, call, id.unwrap_or_else(|| conv.into()), from - 1, limit, role);
                return;
            }
            Ok(Request::ReadSteps { id, from, limit, offset, tool, failed_only }) => {
                self.read_history_tool(conv, call, id.unwrap_or_else(|| conv.into()), from, 1, move |page, reader| {
                    Self::sibling_steps(page, reader, from, limit, offset, tool.as_deref(), failed_only).map(|value| value.to_string())
                });
                return;
            }
            Ok(Request::CommandResults { id, tool, limit, running_only }) => {
                let (me, runtime, reader, turn) =
                    (self.me.clone(), self.opts.runtime.clone(), conv.to_string(), self.live[conv].turn.clone());
                let id = id.unwrap_or_else(|| reader.clone());
                self.opts.runtime.spawn_blocking(move || {
                    let Some(core) = me.upgrade() else { return };
                    Engine { core, runtime }.read_command_results(&reader, &id, tool.as_deref(), limit, running_only, |core, result| {
                        if let Some(live) = core.live.get(&reader).filter(|l| l.sid == sid && l.turn == turn && !core.closing) {
                            match result {
                                Ok(value) => live.session.helper_reply(&call, true, value.to_string()),
                                Err(why) => live.session.helper_reply(&call, false, format!("Nothing to read: {why}.")),
                            }
                        }
                    });
                });
                return;
            }
            Ok(Request::Workspace) => {
                drop(self.inspect_workspace(conv, call));
                return;
            }
            Ok(Request::ReadMany { ids }) => {
                self.read_siblings_tool(conv, call, ids, true);
                return;
            }
            Ok(Request::List { offset, limit }) => match self.list_siblings(conv, offset, limit) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing to list: {why}.")),
            },
            Ok(Request::Ask { id, message }) => result(self.session_message(conv, &id, &message)),
            Ok(Request::Stop { id }) => result(self.session_stop(conv, &id)),
            Ok(Request::Resume { id, message }) => result(self.session_resume(conv, &id, message.as_deref())),
            Ok(Request::Wait { on, note }) => result(self.session_wait(conv, on, note)),
            Ok(Request::Context) => (true, self.context_answer(conv)),
            Ok(Request::Compact { next }) => match self.plan_compaction(conv, next) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing was scheduled: {why}.")),
            },
            Ok(Request::Clear { next }) => match self.plan_clear(conv, next) {
                Ok(()) => (true, sibling::CLEAR_PLANNED.to_string()),
                Err(why) => (false, format!("Nothing was scheduled: {why}.")),
            },
            Ok(Request::Archive { id: Some(id) }) => result(self.session_archive(conv, &id)),
            Ok(Request::Archive { id: None }) => match self.archive_itself(conv) {
                Ok(()) => (true, sibling::ARCHIVED.to_string()),
                Err(why) => (false, format!("Nothing was archived: {why}.")),
            },
        };
        // Record the result on the call as well as returning it to the agent.
        if is_start && ok {
            self.handle(
                conv,
                sid,
                crate::events::EngineEvent::ToolResult { id: call.clone(), ok, declined: false, exit_code: None, text: text.clone() },
            );
        }
        // The answer goes back only to the process that asked.
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(&call, ok, text);
        }
    }

    fn start_sibling(&mut self, conv: &str, new: NewSession) -> Result<(String, bool), &'static str> {
        let NewSession { title, brief, ai, keep_going, step, model, mut effort, mut work } = new;
        if self.goal_out_of_money(conv) {
            return Err(GOAL_CAP);
        }
        self.free_dead_workers(conv);
        let parent = self.convs.get(conv).ok_or("the conversation is gone")?;
        let fix = parent.goal_fix.clone();
        if !offers(parent.area) || parent.started_by.is_some() {
            return Err("only a project's own agent can start conversations");
        }
        // Economy mode (rule 7): extras wait, so no new conversation starts. `ask` into a conversation already running stays
        // allowed: it continues work that was started.
        if self.economy(conv) {
            return Err("the job has passed its estimate by half, so no new conversation starts; keep to the plan");
        }
        // A plan step starts only inside the wave: ready, and sharing no file with a step that runs (`crate::progress::can_start`).
        if let Some(step) = &step {
            progress::can_start(parent.plan.as_ref().ok_or("there is no plan: send the plan with its steps first")?, step)?;
        }
        let project = parent.project_id.clone().ok_or("this conversation has no project")?;
        // A named AI overrides the person's pin for this background work only, never for the conversation that asked.
        let mut choice = match ai {
            Ai::Same => parent.choice.clone().ok_or("this conversation has no model chosen")?,
            Ai::Auto => Choice { vendor: "auto".into(), model: None, effort: None },
            Ai::Named(vendor) if self.vendors().contains(&vendor) => {
                Choice { vendor: vendor.name().into(), model: None, effort: Some(crate::router::Effort::Medium) }
            }
            Ai::Named(_) => return Err("that AI is not connected on this computer; choose another, or leave ai out"),
        };
        if matches!(ai, Ai::Same) && choice.vendor == "auto" && (model.is_some() || effort.is_some()) {
            let vendor = self.live.get(conv).map(|l| l.vendor).ok_or("the current AI is not known yet")?;
            // A model the lead's own AI does not run names its AI below, as under `auto`.
            if model.as_ref().is_none_or(|m| self.runs_model(vendor, m)) {
                choice.vendor = vendor.name().into();
            }
        }
        // Auto never refuses a model or an effort (the owner's leads asked for both, 2026-11-11): a model names its AI, and an
        // effort alone says how hard the work is, for the table to pick the model.
        if choice.vendor == "auto" {
            if let Some(model) = &model {
                let vendor = Vendor::RUNNABLE
                    .into_iter()
                    .find(|v| self.vendors().contains(v) && self.runs_model(*v, model))
                    .ok_or("unknown model; choose a model of a connected AI")?;
                choice.vendor = vendor.name().into();
            } else if let Some(effort) = effort.take() {
                let difficulty = match effort.as_str() {
                    "low" => "easy",
                    "medium" => "medium",
                    "high" => "hard",
                    _ => return Err("effort must be low, medium or high"),
                };
                work = work.or(Some(sibling::Work { kind: "code", difficulty, scope: "tight" }));
            }
        }
        // A named AI with the work stated runs the table's row for that work inside it, not its everyday model.
        if let (Ai::Named(vendor), Some(work), None, None) = (ai, work, &model, &effort)
            && let Some(routed) = self.table_first(&work.asked(), Some(vendor), true, &crate::route::Reading::default())
        {
            choice.model = Some(routed.profile.model.into());
            choice.effort = {
                use crate::router::Effort::{High, Low, Medium, Xhigh};
                [Low, Medium, High, Xhigh]
            }
            .into_iter()
            .find(|e| e.name() == routed.profile.effort);
        }
        let selected_model = model.clone().or_else(|| choice.model.clone().filter(|_| matches!(ai, Ai::Named(_))));
        if let Some(model) = model {
            let vendor = Vendor::ALL.into_iter().find(|v| v.name() == choice.vendor).ok_or("choose a named AI for a model")?;
            if !self.runs_model(vendor, &model) {
                return Err("unknown model; choose a model available for this AI");
            }
            choice.model = Some(model);
        }
        if let Some(effort) = effort {
            choice.effort = Some(match effort.as_str() {
                "low" => crate::router::Effort::Low,
                "medium" => crate::router::Effort::Medium,
                "high" => crate::router::Effort::High,
                _ => return Err("effort must be low, medium or high"),
            });
        }
        self.sibling_room(&project)?;
        // A conversation an agent starts is never the person's own turn: the same quota rules as a helper hold, for the AI that
        // runs it. Auto is held to them once it has routed the brief, like any turn nobody sent by hand.
        let gate = match ai {
            Ai::Same => self.live.get(conv).map(|l| (l.vendor, l.model.as_deref().and_then(family_of).map(str::to_string))),
            Ai::Auto => None,
            // The family of the default model it will run (Claude's quota is kept per family), as the launch resolves it.
            Ai::Named(vendor) => {
                let pick = crate::router::Pick { vendor, model: None, effort: String::new(), reason: String::new(), pace: None };
                let model = pick.resolved(self.codex_recommended.as_deref(), &self.codex_models).model;
                Some((vendor, model.as_deref().and_then(family_of).map(str::to_string)))
            }
        };
        // The AI that really runs it answers for its quota: a model named under Auto, or by an Auto lead, chose its own.
        let gate = match (gate, Vendor::ALL.into_iter().find(|v| v.name() == choice.vendor)) {
            (held, Some(runs)) if matches!(ai, Ai::Auto) || held.as_ref().is_some_and(|(vendor, _)| *vendor != runs) => {
                let pick = crate::router::Pick { vendor: runs, model: None, effort: String::new(), reason: String::new(), pace: None };
                let model = pick.resolved(self.codex_recommended.as_deref(), &self.codex_models).model;
                Some((runs, model.as_deref().and_then(family_of).map(str::to_string)))
            }
            (held, _) => held,
        };
        let gate = match (gate, selected_model) {
            (Some((vendor, _)), Some(model)) => Some((vendor, family_of(crate::router::pinned(vendor, &model)).map(str::to_string))),
            (gate, _) => gate,
        };
        self.sibling_allowed(&gate)?;
        let id = self.blank(Area::Project, Some(project.clone()));
        if let Some(work) = work.filter(|_| choice.vendor == "auto") {
            self.given.insert(id.clone(), work.asked());
        }
        if let Some(c) = self.convs.get_mut(&id) {
            c.started_by = Some(conv.to_string());
            c.goal_fix = fix;
            c.title = title.to_string();
        }
        // Linked before the first message, so the copy it waits for starts from the leader's latest work.
        if let Some(step) = &step {
            let worker = StepWorker { conversation: id.clone(), branch: String::new(), base: String::new(), joining: false };
            self.edit_step(conv, |s| s.id == *step, |s| (s.state, s.worker) = (StepState::Active, Some(worker)));
        }
        let req = SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: Some(project),
            text: sibling::prompt(&title, &brief),
            images: vec![],
            files: vec![],
            choice,
        };
        // The common start path makes its copy off the lock and removes any copy that no longer belongs.
        match self.launch_sibling(&id, req, None) {
            Ok(()) => {
                // Turned on while its first turn runs, Keep going acts when that turn ends, with the chip's own rules.
                if keep_going {
                    let _ = self.set_keep_going(&id, true);
                }
                Ok((id, true))
            }
            Err(_) => {
                self.given.remove(&id);
                // Its step is free to start again, whatever becomes of the conversation.
                self.edit_worker_step(conv, &id, |s| (s.state, s.worker) = (StepState::Pending, None));
                // Saved with its first message before it failed: it stays, with the error in its reply, and the agent is told.
                if self.convs.get(&id).is_some_and(|c| !c.messages.is_empty()) {
                    return Ok((id, false));
                }
                let _ = self.remove(&id);
                Err("the conversation could not start")
            }
        }
    }

    /// Whether `model` is one `vendor` runs: a row of Auto's table or a model the Codex account lists.
    fn runs_model(&self, vendor: Vendor, model: &str) -> bool {
        match vendor {
            Vendor::Claude | Vendor::Gemini => {
                crate::route::PROFILES.iter().any(|p| p.vendor == vendor && p.model == crate::router::pinned(vendor, model))
            }
            Vendor::Codex => self.codex_models.iter().any(|m| m == model),
        }
    }

    /// Changes the first plan step of conversation `lead` that `is` picks, and tells the screen and the disk when it changed
    /// (then true).
    pub(super) fn edit_step(&mut self, lead: &str, is: impl Fn(&PlanStep) -> bool, edit: impl FnOnce(&mut PlanStep)) -> bool {
        let Some(c) = self.load(lead) else { return false };
        let Some(step) = c.plan.as_mut().and_then(|p| p.steps.iter_mut().find(|s| is(s))) else { return false };
        let before = step.clone();
        edit(step);
        if *step == before {
            return false;
        }
        self.save(lead);
        let plan = self.shown_plan(lead);
        (self.emit)(AppEvent::Plan { conversation_id: lead.to_string(), plan });
        true
    }

    /// Changes the plan step of `lead` that conversation `child` works on.
    fn edit_worker_step(&mut self, lead: &str, child: &str, edit: impl FnOnce(&mut PlanStep)) -> bool {
        self.edit_step(lead, |s| s.worker.as_ref().is_some_and(|w| w.conversation == child), edit)
    }

    /// A step's conversation is gone (deleted): its step is free again, so no deleted conversation keeps files reserved.
    pub(super) fn free_dead_workers(&mut self, lead: &str) {
        let workers: Option<Vec<_>> =
            self.convs.get(lead).and_then(|c| c.plan.as_ref()).map(|p| p.steps.iter().filter_map(|s| s.worker.clone()).collect());
        for w in workers.unwrap_or_default() {
            if self.load(&w.conversation).is_none() {
                self.edit_worker_step(lead, &w.conversation, |s| {
                    s.worker = None;
                    if s.state == StepState::Active {
                        s.state = StepState::Pending;
                    }
                });
            }
        }
    }

    /// A step's conversation failed before its agent started (its copy could not be made, or its first answer could not
    /// start): the step may start again, and the agent that started it hears so. A later turn that fails leaves the step alone.
    pub(super) fn step_not_started(&mut self, child: &str) {
        let Some(c) = self.convs.get(child) else { return };
        if c.messages.iter().filter(|m| m.role == Role::Assistant).count() > 1 {
            return;
        }
        let Some(lead) = c.started_by.clone() else { return };
        if self.edit_worker_step(&lead, child, |s| (s.state, s.worker) = (StepState::Pending, None)) {
            self.turn_ended(child);
        }
    }

    /// Changes the worker record of the plan step that conversation `child` works on, in the plan of the agent that started it.
    pub(super) fn edit_worker(&mut self, child: &str, edit: impl FnOnce(&mut StepWorker)) {
        let Some(lead) = self.convs.get(child).and_then(|c| c.started_by.clone()) else { return };
        self.edit_worker_step(&lead, child, |s| _ = s.worker.as_mut().map(edit));
    }

    /// Whether the parent's AI may run work an agent started (a helper's rules); no live process means nothing to check.
    /// Whether the project has room for one more conversation an agent starts. The cap is on what agents start: a conversation
    /// counts from the call on, its copy still being made, routing and waiting for quota included, not only once its CLI runs.
    /// One waiting for the person to answer it frees a working place, so the rest of the job goes on, but stays open. A message
    /// the person sends to one is the person's own.
    fn sibling_room(&self, project: &str) -> Result<(), &'static str> {
        let (mut working, mut open) = (0, 0);
        for c in self.convs.values().filter(|c| c.started_by.is_some() && c.project_id.as_deref() == Some(project)) {
            if self.live.get(&c.id).is_some_and(|l| l.turn.is_some())
                || self.member_waits_for_command(c)
                || !c.queued.is_empty()
                || c.messages.is_empty()
                || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
            {
                open += 1;
                working += usize::from(self.live_sibling_state(c) != State::Waiting);
            }
        }
        if working >= sibling::MAX_RUNNING {
            return Err("three conversations started by agents are already working in this project; wait for one to finish");
        }
        if open >= sibling::MAX_OPEN {
            return Err("five conversations started by agents are already open in this project, and some wait for the person to answer \
                them; tell the person once with the faber-wrap-up block, then wait for an answer or for one to finish");
        }
        Ok(())
    }

    fn sibling_allowed(&self, ai: &Option<(Vendor, Option<String>)>) -> Result<(), &'static str> {
        let Some((vendor, family)) = ai else { return Ok(()) };
        self.allows(*vendor, family.as_deref(), Purpose::UserTurn { by_hand: false }, false).map_err(|why| match why {
            Why::Billing => "this AI may be billed per use, so Faber does not start conversations on it by itself",
            _ => "this AI has no quota left right now",
        })
    }

    /// Sends the agent's brief as the new conversation's first message.
    pub(super) fn launch_sibling(&mut self, id: &str, req: SendRequest, wake: Option<Wake>) -> Result<(), String> {
        // Never the person's own action on the screen: a temporary Chat they have open stays.
        match self.send_task(req, wake, None, false) {
            Ok(_) => {
                // The agent's brief is never the person's own turn, whatever model it runs: billing stops it like a helper. A
                // message the person writes to the conversation later is theirs.
                if let Some(t) = self.transfers.get_mut(id) {
                    t.by_hand = false;
                }
                if let Some(l) = self.live.get_mut(id) {
                    l.by_hand = false;
                }
                Ok(())
            }
            Err(why) => Err(why),
        }
    }

    /// The project of the conversation that called, and the other conversation it names when that one is in the same project.
    fn same_project(&mut self, conv: &str, id: &str) -> Result<String, &'static str> {
        let project = self.convs.get(conv).and_then(|c| c.project_id.clone()).ok_or("this conversation has no project")?;
        if id == conv {
            return Err("that id is this conversation");
        }
        self.load(id)
            .filter(|c| c.project_id.as_deref() == Some(project.as_str()))
            .ok_or("no conversation with that id is in this project")?;
        Ok(project)
    }

    fn history_project(&self, reader: &str) -> Option<&str> {
        self.convs.get(reader).filter(|c| !c.temporary).and_then(|c| c.project_id.as_deref())
    }

    fn open_project_conversation(&self, id: &str, project: &str) -> Option<Result<&Conversation, &'static str>> {
        self.convs.get(id).map(|c| {
            if c.temporary || c.project_id.as_deref() != Some(project) {
                return Err(HISTORY_OUTSIDE_PROJECT);
            }
            Ok(c)
        })
    }

    fn open_project_history(&self, id: &str, project: &str, start: usize, limit: usize) -> Option<Result<Page<'_>, &'static str>> {
        self.open_project_conversation(id, project).map(|result| result.map(|c| Page::borrowed(c, start, limit)))
    }

    fn project_page(&self, reader: &str, id: &str, start: usize, limit: usize) -> Result<ProjectPage<Page<'_>>, &'static str> {
        let project = self.history_project(reader).ok_or("this conversation has no project")?;
        if let Some(page) = self.open_project_history(id, project, start, limit) {
            return page.map(ProjectPage::Open);
        }
        Ok(ProjectPage::Saved(SavedProjectPage { store: self.store.clone(), project: project.into(), id: id.into(), start, limit }))
    }

    fn read_history_tool(
        &self,
        reader: &str,
        call: String,
        id: String,
        from: usize,
        limit: usize,
        render: impl FnOnce(&Page<'_>, &str) -> Result<String, &'static str> + Send + 'static,
    ) {
        fn answer(live: &Live, call: &str, reply: Result<String, &'static str>) {
            match reply {
                Ok(text) => live.session.helper_reply(call, true, text),
                Err(why) => live.session.helper_reply(call, false, format!("Nothing to read: {why}.")),
            }
        }
        let Some(live) = self.live.get(reader) else { return };
        let read = match self.project_page(reader, &id, from - 1, limit) {
            Ok(ProjectPage::Open(page)) => {
                answer(live, &call, render(&page, reader));
                return;
            }
            Ok(ProjectPage::Saved(read)) => read,
            Err(why) => {
                answer(live, &call, Err(why));
                return;
            }
        };
        let reader = reader.to_string();
        let (sid, turn, me) = (live.sid, live.turn.clone(), self.me.clone());
        self.opts.runtime.spawn_blocking(move || {
            let saved = read.read();
            let Some(core) = me.upgrade() else { return };
            let core = core.lock().unwrap_or_else(|e| e.into_inner());
            if core.closing {
                return;
            }
            let Some(live) = core.live.get(&reader).filter(|l| l.sid == sid && l.turn == turn) else { return };
            let page = read.current(&core, &reader, saved);
            answer(live, &call, page.and_then(|page| render(&page, &reader)));
        });
    }

    fn read_headers_tool(&self, reader: &str, call: String, id: String, start: usize, limit: usize, role: Option<Role>) {
        let Some(live) = self.live.get(reader) else { return };
        let reply = move |live: &Live, result: Result<Headers, &'static str>| match result {
            Ok(page) => live.session.helper_reply(&call, true, json!(page).to_string()),
            Err(why) => live.session.helper_reply(&call, false, format!("Nothing to read: {why}.")),
        };
        let Some(project) = self.history_project(reader) else {
            reply(live, Err("this conversation has no project"));
            return;
        };
        if let Some(target) = self.open_project_conversation(&id, project) {
            reply(live, target.map(|c| Headers::of(c, start, limit, role)));
            return;
        }
        let reader = reader.to_string();
        let read = SavedProjectPage { store: self.store.clone(), project: project.to_string(), id, start, limit };
        let (sid, turn, me) = (live.sid, live.turn.clone(), self.me.clone());
        self.opts.runtime.spawn_blocking(move || {
            let saved = read.read_headers(role);
            let Some(core) = me.upgrade() else { return };
            let core = core.lock().unwrap_or_else(|e| e.into_inner());
            if core.closing {
                return;
            }
            let Some(live) = core.live.get(&reader).filter(|l| l.sid == sid && l.turn == turn) else { return };
            reply(live, read.current_headers(&core, &reader, role, saved));
        });
    }

    fn sibling_messages(c: &Page<'_>, from: usize, limit: usize, offset: usize) -> String {
        let max_chars = sibling::MAX_READ / limit;
        let messages: Vec<_> = c
            .messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                let mut row = json!({ "id": message.id, "number": from + index, "role": message.role });
                if let Some(status) = message.status {
                    row["status"] = json!(status);
                }
                if message.closed {
                    row["closed"] = json!(true);
                }
                let failures = message.tools.iter().flatten().filter(|call| failed_step(call)).count();
                if failures > 0 {
                    row["failedSteps"] = json!(failures);
                }
                let mut text_budget = max_chars;
                if offset == 0
                    && let Some(error) = message.error.as_deref()
                {
                    let masked = masked_diagnostic(error, MAX_DIAGNOSTIC_BYTES);
                    let end = masked.char_indices().nth((max_chars / 4).min(400)).map_or(masked.len(), |(at, _)| at);
                    let excerpt = &masked[..end];
                    if end < masked.len() || error.len() > MAX_DIAGNOSTIC_BYTES {
                        row["errorTruncated"] = json!(true);
                    }
                    text_budget -= excerpt.chars().count();
                    row["error"] = json!(excerpt);
                }
                let body = &message.text;
                let recap =
                    message.wrap_up.as_ref().map(|w| masked_diagnostic(&w.visible_words(), MAX_DIAGNOSTIC_BYTES)).unwrap_or_default();
                let separator = if body.is_empty() || recap.is_empty() { "" } else { "\n\n" };
                let mut chars = body.chars().chain(separator.chars()).chain(recap.chars()).skip(offset);
                row["text"] = json!(chars.by_ref().take(text_budget).collect::<String>());
                if chars.next().is_some() {
                    row["nextOffset"] = json!(offset + text_budget);
                }
                row
            })
            .collect();
        json!({ "id": c.id, "count": c.count, "messages": messages }).to_string()
    }

    fn sibling_steps(
        c: &Page<'_>,
        reader: &str,
        from: usize,
        limit: usize,
        offset: usize,
        tool: Option<&str>,
        failed_only: bool,
    ) -> Result<Value, &'static str> {
        let m = c.messages.first().ok_or("there is no message at that number")?;
        let tools = m.tools.as_deref().unwrap_or_default();
        let owns = reader == c.id || c.started_by.as_deref() == Some(reader);
        let step = |call: &ToolUse| {
            let duration = call.ended.zip(call.started).and_then(|(ended, started)| ended.checked_sub(started));
            let failure_tail = call.failure_tail.as_deref().filter(|_| failed_step(call));
            let detail = if !owns {
                None
            } else if failed_step(call) {
                failure_tail.or(call.output.as_deref()).map(|text| (text, false))
            } else if call.ok == Some(true) && !call.declined && !call.interrupted {
                match call.name.as_str() {
                    "webSearch" => call.output.as_deref(),
                    "WebSearch" => call.input["query"].as_str(),
                    "WebFetch" => call.input["url"].as_str(),
                    _ => None,
                }
                .map(|text| (text, true))
            } else {
                None
            }
            .filter(|(text, _)| !text.trim().is_empty());
            let mut row = json!({
                "id": call.id, "name": call.name, "ok": call.ok, "declined": call.declined,
                "interrupted": call.interrupted, "check": call.check, "parent": call.parent, "durationMs": duration,
                "diagnosticAvailable": detail.is_some(),
            });
            if let Some(code) = call.exit_code {
                row["exitCode"] = json!(code);
            }
            if tool.is_some()
                && let Some((out, from_start)) = detail
            {
                // Older records do not say whether they hit the capture cap. The last line can contain a partial credential.
                let at_capture_cap = failure_tail.is_none() && out.chars().count() >= TOOL_OUTPUT_CHARS;
                let whole_lines = if at_capture_cap { out.rsplit_once('\n').map_or("", |(lines, _)| lines) } else { out };
                let masked = masked_diagnostic(whole_lines, MAX_DIAGNOSTIC_BYTES);
                let excerpt = if from_start {
                    let end = masked.char_indices().nth(DIAGNOSTIC_CHARS).map_or(masked.len(), |(at, _)| at);
                    &masked[..end]
                } else {
                    let start = masked.char_indices().rev().nth(DIAGNOSTIC_CHARS - 1).map_or(0, |(at, _)| at);
                    &masked[start..]
                };
                row["diagnostic"] = json!(excerpt);
                if excerpt.len() < masked.len() || at_capture_cap || failure_tail.is_some() {
                    row["diagnosticTruncated"] = json!(true);
                }
            }
            row
        };
        let mut result = json!({ "id": c.id, "messageId": m.id, "number": from, "count": tools.len() });
        if let Some(tool) = tool {
            let call = tools.iter().find(|call| call.id == tool).ok_or("no tool with that id in this message")?;
            result["tools"] = json!([step(call)]);
        } else {
            let end = offset.saturating_add(limit);
            let (count, page) = if failed_only {
                let mut count = 0;
                let mut page = Vec::new();
                for call in tools.iter().filter(|call| failed_step(call)) {
                    if (offset..end).contains(&count) {
                        page.push(step(call));
                    }
                    count += 1;
                }
                (count, page)
            } else {
                (tools.len(), tools.get(offset..end.min(tools.len())).unwrap_or_default().iter().map(step).collect())
            };
            result["count"] = json!(count);
            result["tools"] = json!(page);
            if end < count {
                result["nextOffset"] = json!(end);
            }
        }
        Ok(result)
    }

    fn read_siblings_tool(&mut self, reader: &str, call: String, ids: Vec<String>, many: bool) {
        let Some(live) = self.live.get(reader) else { return };
        let Some(project) = self.history_project(reader) else {
            live.session.helper_reply(&call, false, "Nothing to read: this conversation has no project.".into());
            return;
        };
        let missing: HashSet<_> = ids.iter().filter(|id| *id != reader && !self.convs.contains_key(*id)).cloned().collect();
        if missing.is_empty() {
            self.finish_sibling_reads(reader, &call, &ids, many, &SavedPreviews::new());
            return;
        }

        let (reader, project) = (reader.to_string(), project.to_string());
        let (sid, turn, store, me) = (live.sid, live.turn.clone(), self.store.clone(), self.me.clone());
        let gate = self.sibling_history_gate.clone();
        self.opts.runtime.spawn(async move {
            let Ok(permit) = gate.acquire_owned().await else { return };
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                let current = |core: &Core| !core.closing && core.live.get(&reader).is_some_and(|l| l.sid == sid && l.turn == turn);
                let Some(core) = me.upgrade() else { return };
                {
                    let core = core.lock().unwrap_or_else(|e| e.into_inner());
                    if !current(&core) {
                        return;
                    }
                }
                let saved = missing
                    .into_iter()
                    .filter_map(|id| {
                        let (read, version) = store.project_history_record(&id, &project)?;
                        let preview = match read {
                            ProjectHistory::Latest(reply) => {
                                ReplyPreview::from_words(&reply.title, reply.state, Some(&reply.id), &reply.words)
                            }
                            ProjectHistory::Complete(mut c) => {
                                // Match opening the saved conversation without adopting it or its permissions.
                                if let Some(reply) = c.messages.iter_mut().rev().find(|m| m.role == Role::Assistant) {
                                    reply.closed |= reply.status == Some(Status::Streaming);
                                    settle(reply);
                                }
                                ReplyPreview::new(&c, state_of(false, &c))
                            }
                        };
                        Some((id, (version, preview)))
                    })
                    .collect();
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if !current(&core) {
                    return;
                }
                if core.history_project(&reader) != Some(project.as_str()) {
                    core.live[&reader].session.helper_reply(&call, false, format!("Nothing to read: {HISTORY_OUTSIDE_PROJECT}."));
                    return;
                }
                core.finish_sibling_reads(&reader, &call, &ids, many, &saved);
            });
        });
    }

    fn finish_sibling_reads(&mut self, reader: &str, call: &str, ids: &[String], many: bool, saved: &SavedPreviews) {
        let mut ok = false;
        let texts: Vec<_> = ids
            .iter()
            .map(|id| match self.read_sibling(reader, id, saved) {
                Ok(text) => {
                    ok = true;
                    text
                }
                Err(why) if many => format!("Nothing to read in {id}: {why}."),
                Err(why) => format!("Nothing to read: {why}."),
            })
            .collect();
        self.live[reader].session.helper_reply(call, ok, texts.join("\n\n---\n\n"));
    }

    /// Reading the same conversation again in one turn with nothing new answers briefly: the owner's conversations read one
    /// 101 times, 118 of 481 reads returned what the agent already had. A context that shrank since (compacted) reads in full.
    fn read_sibling(&mut self, conv: &str, id: &str, saved: &SavedPreviews) -> Result<String, &'static str> {
        let project = self.history_project(conv).ok_or("this conversation has no project")?;
        if id == conv {
            return Err("that id is this conversation");
        }
        let open;
        let reply = if let Some(c) = self.convs.get(id) {
            if c.id != id || c.temporary || c.project_id.as_deref() != Some(project) {
                return Err(HISTORY_OUTSIDE_PROJECT);
            }
            open = ReplyPreview::new(c, self.live_sibling_state(c));
            &open
        } else {
            let (version, preview) = saved.get(id).ok_or(HISTORY_OUTSIDE_PROJECT)?;
            if !self.store.history_version_is_current(id, *version) {
                return Err("the saved conversation changed; read it again");
            }
            preview
        };
        let Some(answer) = reply.fingerprint else { return Ok(reply.text.clone()) };
        let reader = &self.convs[conv];
        let seen = SeenRead {
            turn: reader.messages.last().map(|m| m.id.clone()).unwrap_or_default(),
            answer,
            used: reader.context.as_ref().map_or(0, |f| f.used),
        };
        let key = (conv.to_string(), id.to_string());
        if self
            .sibling_reads
            .get(&key)
            .is_some_and(|before| seen.turn == before.turn && seen.answer == before.answer && seen.used >= before.used)
        {
            return Ok(sibling::read_again(&reply.title, reply.state));
        }
        self.sibling_reads.insert(key, seen);
        Ok(reply.text.clone())
    }

    /// The project's other conversations, using live state or the facts derived from their saved records.
    fn list_siblings(&self, conv: &str, offset: usize, limit: usize) -> Result<String, &'static str> {
        let project = self.history_project(conv).ok_or("this conversation has no project")?;
        let folder = self.store.projects().into_iter().find(|p| p.id == project).map(|p| p.path).unwrap_or_default();
        let at = now();
        let mut summaries = self.listed_rows();
        summaries.retain(|s| s.id != conv && !s.archived && !s.temporary && s.project_id.as_deref() == Some(project));
        summaries.sort_unstable_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.id.cmp(&b.id)));
        // Skipped summaries never need their work-copy details or journal recovery.
        let mut candidates = summaries.into_iter().enumerate().skip(offset);
        let rows: Vec<sibling::Listed> = candidates
            .by_ref()
            .filter_map(|(_, s)| {
                let (state, folder) = if let Some(c) = self.convs.get(&s.id) {
                    if c.id != s.id || c.temporary || c.archived_at.is_some() || c.project_id.as_deref() != Some(project) {
                        return None;
                    }
                    (self.live_sibling_state(c), c.copy.as_ref().map(|copy| copy.path.clone()))
                } else {
                    self.store.listed_project_detail(&s, project)?
                };
                Some(sibling::Listed { state, folder, minutes: at.saturating_sub(s.updated_at) / 60_000, id: s.id, title: s.title })
            })
            .take(limit)
            .collect();
        let mut text = if rows.is_empty() && offset > 0 {
            "No more project conversations at this offset.".into()
        } else {
            sibling::list(&folder, &rows)
        };
        if let Some((next, _)) = candidates.next() {
            text.push_str(&format!("\nMore conversations are available. Pass nextOffset as offset for the next page.\nnextOffset: {next}"));
        }
        Ok(text)
    }

    /// A member can still be working between CLI turns while its reply waits for a background command.
    pub(super) fn live_sibling_state(&self, c: &Conversation) -> State {
        if self.member_waits_for_command(c) { State::Working } else { state_of(self.live.get(&c.id).is_some_and(|l| l.turn.is_some()), c) }
    }

    fn member_waits_for_command(&self, c: &Conversation) -> bool {
        self.waiting_command(&c.id).is_some() || (self.keep_going_command_wait(&c.id).is_some() && !wait::has_open_permission(c))
    }

    /// A conversation an agent started stopped working (its turn ended, or it was stopped with messages queued): its report
    /// goes to the agent that started it by itself, so no agent waits or polls. An archived or gone parent hears nothing.
    pub(super) fn tell_parent(&mut self, conv: &str) {
        let Some(c) = self.convs.get(conv) else { return };
        let Some(parent) = c.started_by.clone() else { return };
        let state = self.live_sibling_state(c);
        if c.keep_going.is_some() || c.compacting || reports_later(state, c) {
            return;
        }
        let reply = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
        let report_id = format!("report-{conv}-{}", reply.map_or("empty", |m| m.id.as_str()));
        let last = ended::conclusion_line(c).unwrap_or_else(|| {
            let text = reply.map(Message::said).unwrap_or_default();
            format!("No confirmed conclusion. Latest output: {text}")
        });
        // The exact end reason travels even after delivered work; Faber's stop of a silent reply is not the person's.
        let error = reply.and_then(|m| m.error.as_deref());
        let state = if state == State::Stopped && !c.stopped && error.is_some() { State::Failed } else { state };
        let last = error.map_or(last.clone(), |key| {
            format!(
                "{last}
Failure reason: {key}"
            )
        });
        let record = crate::results::record_line(c);
        let title = c.title.clone();
        // A piece that ended twice with nothing changed: the report names the step up, or the move (`stronger.rs`).
        let rung = self.empty_end(conv, state);
        let moved = self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let record = [record, rung].into_iter().flatten().collect::<Vec<_>>().join(" ");
        let text = sibling::finished(&title, conv, state, &last, Some(record.as_str()).filter(|r| !r.is_empty()));
        // Stopped working: the plan step waits for its work to be joined. One that waits for an answer is not done yet.
        if state != State::Waiting && !moved {
            self.edit_worker(conv, |w| w.joining = true);
        }
        let mut recipients = vec![parent.clone()];
        let mut cursor = parent;
        let mut seen = HashSet::new();
        while seen.insert(cursor.clone()) {
            let Some(next) = self.load(&cursor).and_then(|c| c.started_by.clone()) else { break };
            if self.load(&next).and_then(|c| c.waiting_on.as_ref()).is_some_and(|w| w.ids.iter().any(|id| id == conv)) {
                recipients.push(next.clone());
            }
            cursor = next;
        }
        for parent in recipients {
            self.deliver_child_report(conv, parent, text.clone(), report_id.clone(), state);
        }
    }

    fn deliver_child_report(&mut self, conv: &str, parent: String, text: String, report_id: String, state: State) {
        let direct = self.convs.get(conv).is_some_and(|c| c.started_by.as_deref() == Some(parent.as_str()));
        let reported = self
            .convs
            .get(conv)
            .and_then(|c| c.messages.iter().rev().find(|m| m.role == Role::Assistant))
            .is_some_and(|m| self.live.get(conv).is_some_and(|l| l.parent_reported.as_deref() == Some(&m.id)));
        if direct && reported {
            self.finish_parent_report(conv, &parent);
            return;
        }
        if self.load(&parent).is_none_or(|p| p.archived_at.is_some()) {
            self.finish_parent_report(conv, &parent);
            return;
        }
        let p = &self.convs[&parent];
        if delivered(p, &report_id) {
            if self.try_save(&parent).is_ok() {
                self.finish_parent_report(conv, &parent);
            }
            return;
        }
        if direct && self.hold_report(conv, &parent, state, &text, &report_id) {
            return;
        }
        self.send_child_report(conv, parent, text, report_id);
    }

    /// Queues or sends one report, or keeps it owed when the lead's queue is full.
    pub(super) fn send_child_report(&mut self, conv: &str, parent: String, text: String, report_id: String) {
        let p = &self.convs[&parent];
        if p.queued.len() >= MAX_QUEUED {
            if self.convs.get(conv).is_some_and(|c| c.started_by.as_deref() == Some(parent.as_str())) {
                self.convs.get_mut(conv).expect("child exists").pending_parent_report = true;
                self.save(conv);
            }
            self.parent_reports.entry(parent).or_default().insert(conv.into());
            return;
        }
        // A report the day's money refuses is not lost without a word: the parent's latest reply says the limit is why.
        if self.send_by_itself_with_id(&parent, text, Wake::of("wake.childDone"), report_id.clone()).is_err_and(|e| e == goals::CAP) {
            self.goal_capped(&parent);
        }
        if self.convs.get(&parent).is_some_and(|p| delivered(p, &report_id)) && self.try_save(&parent).is_ok() {
            self.finish_parent_report(conv, &parent);
        }
    }

    fn finish_parent_report(&mut self, child: &str, parent: &str) {
        if self.convs.get(child).is_some_and(|c| c.started_by.as_deref() == Some(parent))
            && let Some(l) = self.live.get_mut(child)
        {
            l.parent_reported =
                self.convs.get(child).and_then(|c| c.messages.iter().rev().find(|m| m.role == Role::Assistant)).map(|m| m.id.clone());
        }
        if self.convs.get(parent).and_then(|p| p.waiting_on.as_ref()).is_some_and(|w| w.ids.iter().any(|id| id == child)) {
            let previous = self.convs.get_mut(parent).unwrap().waiting_on.take();
            if self.try_save(parent).is_err() {
                self.convs.get_mut(parent).unwrap().waiting_on = previous;
                self.parent_reports.entry(parent.into()).or_default().insert(child.into());
                return;
            }
            self.list_changed();
        }
        if let Some(c) = self.convs.get_mut(child).filter(|c| c.pending_parent_report && c.started_by.as_deref() == Some(parent)) {
            c.pending_parent_report = false;
            self.save(child);
        }
        if let Some(pending) = self.parent_reports.get_mut(parent) {
            pending.remove(child);
            if pending.is_empty() {
                self.parent_reports.remove(parent);
            }
        }
    }

    /// Sends one agent's question to another conversation of its project, as a turn Faber writes (never the person's).
    fn ask_sibling(&mut self, conv: &str, id: &str, message: &str) -> Result<String, &'static str> {
        if self.goal_out_of_money(conv) {
            return Err(GOAL_CAP);
        }
        self.same_project(conv, id)?;
        let target = self.convs.get(id).ok_or("the conversation is gone")?;
        // A goal's fix works with what it started and with nothing else of the project: the day's money holds only what is counted.
        if self.convs.get(conv).is_some_and(|c| c.goal_fix.is_some())
            && target.started_by.as_deref() != Some(conv)
            && self.convs.get(conv).and_then(|c| c.started_by.as_deref()) != Some(id)
        {
            return Err("a fix Faber started can ask only the conversations it started");
        }
        if target.archived_at.is_some() {
            return Err("the person archived that conversation");
        }
        let title = target.title.clone();
        let from = self.convs.get(conv).map(|c| c.title.clone()).unwrap_or_default();
        let ai = self.live.get(conv).map(|l| (l.vendor, l.model.as_deref().and_then(family_of).map(str::to_string)));
        self.sibling_allowed(&ai)?;
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some()) || state_of(false, target) == State::Working;
        let stopped =
            target.stopped || (!target.queued.is_empty() && matches!(state_of(false, target), State::Stopped | State::Interrupted));
        let wake = Wake::of("wake.asked");
        self.send_by_itself(id, sibling::ask_prompt(&from, conv, message), wake).map_err(|why| match why.as_str() {
            "error.queueFull" => "the conversation's inbox is full; it cannot take another message yet",
            _ => "it could not take the message now",
        })?;
        if stopped {
            Ok(format!(
                "Sent to \"{title}\". It will read the message when it resumes. Call this tool with {{\"action\": \"read\", \"id\": \"{id}\"}} later to read its answer."
            ))
        } else {
            Ok(sibling::asked(&title, id, busy))
        }
    }
}

fn result(value: Result<String, String>) -> (bool, String) {
    match value {
        Ok(text) => (true, text),
        Err(why) => (false, format!("Nothing was done: {why}.")),
    }
}

impl Core {
    pub(super) fn session_start(&mut self, by: &str, new: NewSession) -> Result<String, String> {
        if new.title.trim().is_empty() || new.brief.trim().is_empty() {
            return Err("start needs a title and a brief".into());
        }
        if let Some(model) = new.model.as_deref() {
            let vendor = match new.ai {
                Ai::Named(vendor) => Some(vendor),
                Ai::Same => self.live.get(by).map(|l| l.vendor).or_else(|| {
                    self.load(by).and_then(|c| c.choice.as_ref()).and_then(|c| Vendor::ALL.into_iter().find(|v| v.name() == c.vendor))
                }),
                Ai::Auto => None,
            };
            if let Some(vendor) = vendor {
                let valid: Vec<String> = match vendor {
                    Vendor::Claude | Vendor::Gemini => {
                        let mut models: Vec<_> =
                            crate::route::PROFILES.iter().filter(|p| p.vendor == vendor).map(|p| p.model.to_string()).collect();
                        models.dedup();
                        models
                    }
                    Vendor::Codex => self.codex_models.clone(),
                };
                if !valid.iter().any(|m| m == crate::router::pinned(vendor, model)) {
                    return Err(format!("unknown model; valid models are: {}", valid.join(", ")));
                }
            }
        }
        let title = new.title.clone();
        let (id, started) = self.start_sibling(by, new).map_err(str::to_string)?;
        if started { Ok(id) } else { Err(sibling::failed(&title, &id)) }
    }
    pub(super) fn session_message(&mut self, by: &str, id: &str, message: &str) -> Result<String, String> {
        if message.trim().is_empty() {
            return Err("the message was empty".into());
        }
        self.ask_sibling(by, id, message).map_err(str::to_string)
    }
    fn own(&mut self, by: &str, id: &str) -> Result<(), String> {
        self.same_project(by, id).map_err(str::to_string)?;
        let mut current = id.to_string();
        let mut seen = HashSet::new();
        while seen.insert(current.clone()) {
            let Some(parent) = self.load(&current).and_then(|c| c.started_by.clone()) else { break };
            if parent == by {
                return Ok(());
            }
            current = parent;
        }
        Err("that conversation was not started by you".into())
    }
    pub(super) fn session_stop(&mut self, by: &str, id: &str) -> Result<String, String> {
        self.own(by, id)?;
        let title = self.convs[id].title.clone();
        self.interrupt(id, true);
        Ok(format!("Stopped {title}; it can be resumed."))
    }
    pub(super) fn session_resume(&mut self, by: &str, id: &str, message: Option<&str>) -> Result<String, String> {
        self.own(by, id)?;
        if self.goal_out_of_money(by) {
            return Err(GOAL_CAP.into());
        }
        let target = &self.convs[id];
        if target.archived_at.is_some() {
            return Err("the conversation is archived".into());
        }
        match self.live_sibling_state(target) {
            State::Working => return Err("the conversation is working".into()),
            State::Waiting => return Err("the conversation is waiting for the person".into()),
            _ => {}
        }
        // The turn runs on the resumed conversation's own AI, so that AI's quota decides, not the asker's.
        let ai = self.convs[id].choice.as_ref().and_then(|c| {
            let vendor = Vendor::ALL.into_iter().find(|v| v.name() == c.vendor)?;
            let family = c.model.as_deref().and_then(|m| family_of(crate::router::pinned(vendor, m))).map(str::to_string);
            Some((vendor, family))
        });
        self.sibling_allowed(&ai).map_err(str::to_string)?;
        let text = message.unwrap_or("Continue where you stopped. Check the project's state first.");
        if text.trim().is_empty() {
            return Err("the message was empty".into());
        }
        let turn = new_id();
        let stopped = self.convs[id].stopped || matches!(state_of(false, &self.convs[id]), State::Stopped | State::Interrupted);
        self.send_by_itself_with_id(id, text.into(), Wake::of("wake.asked"), turn.clone()).map_err(|why| match why.as_str() {
            "error.queueFull" => "the conversation's inbox is full; it cannot take another message yet".into(),
            _ => format!("the conversation could not take the message: {why}"),
        })?;
        if stopped && self.convs.get(id).is_some_and(|c| c.queued.iter().any(|q| q.id == turn)) {
            let first = self.convs[id].queued.first().map(|q| q.id.clone()).unwrap_or_else(|| turn.clone());
            if let Err(why) = self.send_queued(id, &first) {
                self.remove_queued(id, &turn);
                if let Some(c) = self.convs.get_mut(id) {
                    c.stopped = true;
                }
                return Err(format!("the conversation could not start the queued message: {why}"));
            }
        }
        Ok("The conversation will continue with your message.".into())
    }
    /// An agent archives its own conversation as its last step, never one the person pinned (the person wants it kept).
    fn archive_itself(&mut self, conv: &str) -> Result<(), String> {
        if self.convs.get(conv).ok_or("the conversation is gone")?.pinned_at.is_some() {
            return Err("the person pinned this conversation, so it stays".into());
        }
        self.set_archived(conv, true)
    }
    /// Any finished conversation of the same project, not only the ones this one started: archiving is organizing, and the
    /// person brings it back from the sidebar (R499).
    pub(super) fn session_archive(&mut self, by: &str, id: &str) -> Result<String, String> {
        self.same_project(by, id).map_err(str::to_string)?;
        let c = &self.convs[id];
        if c.archived_at.is_some() {
            return Err("the conversation is already archived".into());
        }
        if c.pinned_at.is_some() {
            return Err("the person pinned this conversation".into());
        }
        if !c.queued.is_empty() {
            return Err("the conversation has a queued turn".into());
        }
        match self.live_sibling_state(c) {
            State::Working => return Err("the conversation is working or has a queued turn".into()),
            State::Waiting => return Err("the conversation is waiting".into()),
            _ => {}
        }
        self.set_archived(id, true)?;
        Ok("The conversation was archived.".into())
    }
    pub(super) fn session_wait(&mut self, by: &str, on: Vec<String>, note: Option<String>) -> Result<String, String> {
        let note = note.unwrap_or_default();
        if on.is_empty() && note.trim().is_empty() {
            return Err("wait needs child ids or a note".into());
        }
        if on.len() > 12 {
            return Err("wait takes at most 12 ids".into());
        }
        // Cut, not refused: a refusal makes the agent call again for a line the person only glances at.
        let note: String = if note.chars().count() > 200 { note.chars().take(199).chain(['…']).collect() } else { note };
        for id in &on {
            self.own(by, id)?;
        }
        let waiting_for_children = !on.is_empty();
        // A conversation that reported before the wait is read by the asker already; waiting on it would never end.
        let mut reported = Vec::new();
        for id in &on {
            let child = &self.convs[id];
            let reply = child.messages.iter().rev().find(|m| m.role == Role::Assistant);
            let report_id = format!("report-{id}-{}", reply.map_or("empty", |m| m.id.as_str()));
            let ended = !reports_later(self.live_sibling_state(child), child);
            if ended && self.convs.get(by).is_some_and(|p| delivered(p, &report_id)) {
                reported.push(id.clone());
            }
        }
        let on: Vec<String> = on.into_iter().filter(|id| !reported.contains(id)).collect();
        if on.is_empty() && waiting_for_children {
            return Ok("Those conversations already reported; read their reports and continue.".into());
        }
        let since = (self.secs() * 1000.0) as u64;
        let until = since.saturating_add((300_000.0 * self.keep_pace).max(1.0) as u64);
        let pr = self.wait_pr(by);
        let c = self.load(by).ok_or("the conversation is gone")?;
        // After the person's Stop the wait is only shown; it never returns by itself.
        let stopped = c.stopped;
        let problem = stopped.then(|| "reason.wait.stopped".to_string());
        let previous =
            c.waiting_on.replace(crate::model::WaitingOn { storage_blocked: false, ids: on.clone(), note, since, until, problem, pr });
        if self.try_save(by).is_err() {
            self.convs.get_mut(by).expect("waiting conversation exists").waiting_on = previous;
            return Err("the wait could not be saved".into());
        }
        self.cancel_wait_timer(by);
        self.list_changed();
        if stopped {
            return Ok("The person stopped this conversation: Faber will not return it by itself. End your turn now.".into());
        }
        self.arm_wait(by);
        self.refresh_wait_pr(by);
        for id in on {
            self.tell_parent(&id);
        }
        let helpers = self.working_helpers(by);
        if !helpers.is_empty() {
            return Ok(format!(
                "The wait is saved; do not end your turn yet if you need these reports now: {} still working. Use helper({{\"to\":name,\"wait\":true}}) for each report. If your turn ends, the actual workers keep running and can return you. This wait also has a state check at Unix milliseconds {until}.",
                helpers.join(", ")
            ));
        }
        Ok(format!(
            "End your turn now; Faber wakes you on a report or checks this wait at Unix milliseconds {until}, including after restart. A blocked return stays visible; do not poll."
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Permission;

    #[test]
    fn a_retained_failure_tail_masks_before_cutting_and_bounds_the_scan() {
        let text = format!(
            "Preparing files\n-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\nCould not open the copy.",
            "sample-key-line\n".repeat(300)
        );
        let retained = retained_failure_tail(&text).unwrap();
        assert!(retained.contains("[private key hidden]\nCould not open the copy."));
        assert!(!retained.contains("sample-key-line"));
        let text = format!("{}\nLast word", "é".repeat(5000));
        let retained = retained_failure_tail(&text).unwrap();
        assert_eq!(retained.chars().count(), DIAGNOSTIC_CHARS);
        assert!(retained.ends_with("Last word"));
        let progress = format!("{}\x1b[?25l\x1b[1G\x1b[A\x1b[2K\rPermission denied.", "1\n".repeat(40_000));
        assert!(retained_failure_tail(&progress).unwrap().ends_with("Permission denied."));
        assert!(retained_failure_tail(&"x".repeat(TOOL_OUTPUT_CHARS)).is_none());
        let mut exact = "x".repeat(MAX_FAILURE_SCAN_BYTES - "\nLast word".len());
        exact.push_str("\nLast word");
        assert!(retained_failure_tail(&exact).unwrap().ends_with("Last word"));
        exact.push('!');
        assert!(retained_failure_tail(&exact).is_none());
    }

    #[test]
    fn diagnostics_remove_known_terminal_controls_and_hide_unknown_ones() {
        let text = "\x1b[2J\x1b[1;1H\x1b[1;1fCannot save\nUnknown \x1b]8;;https://example.test/\x07hidden label";
        assert_eq!(masked_diagnostic(text, MAX_DIAGNOSTIC_BYTES), format!("Cannot save\n{HIDDEN_LINE}"));
    }

    fn history<'a>(core: &'a Core, reader: &str, id: &str, from: usize, limit: usize) -> Result<Page<'a>, &'static str> {
        match core.project_page(reader, id, from - 1, limit)? {
            ProjectPage::Open(page) => Ok(page),
            ProjectPage::Saved(read) => read.current(core, reader, read.read()),
        }
    }

    #[tokio::test]
    async fn a_saved_page_must_still_match_the_file_and_the_readers_project() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            let reader = core.blank(Area::Project, Some("p".into()));
            let id = core.blank(Area::Project, Some("p".into()));
            let mut saved = core.convs.remove(&id).unwrap();
            core.store.save_checked(&saved).unwrap();
            let ProjectPage::Saved(read) = core.project_page(&reader, &id, 0, 1).unwrap() else { panic!("expected a saved page") };
            let old = read.read().unwrap();
            let old_headers = read.read_headers(None).unwrap();
            saved.title.push_str(" changed on disk");
            core.store.save_checked(&saved).unwrap();
            assert_eq!(read.current(core, &reader, Some(old)).err(), Some("the saved conversation changed; read it again"));
            assert_eq!(
                read.current_headers(core, &reader, None, Some(old_headers)).err(),
                Some("the saved conversation changed; read it again")
            );
            let fresh = read.read();
            assert!(fresh.is_some());
            core.convs.get_mut(&reader).unwrap().project_id = Some("q".into());
            assert_eq!(read.current(core, &reader, fresh).err(), Some(HISTORY_OUTSIDE_PROJECT));
            assert_eq!(read.current_headers(core, &reader, None, read.read_headers(None)).err(), Some(HISTORY_OUTSIDE_PROJECT));
        });
        engine.shutdown();
    }

    /// A finished child whose latest reply has `status` and `error`, started by a parent that can take a report.
    fn child_report(status: Status, error: Option<&str>, stopped: bool) -> (State, String) {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let parent = super::super::answer::tests::seed(&engine, crate::events::Session::from_fn(|_| {}), &[("answer", &[])]);
        let out = engine.with_saves(false, |core| {
            core.opts.clis.insert(Vendor::Claude, crate::engine::Cli { command: "never-spawn".into(), prefix: vec![] });
            core.ledger.of_mut(Vendor::Claude).billing = crate::budget::Billing::Subscription;
            core.convs.get_mut(&parent).unwrap().choice =
                Some(Choice { vendor: "claude".into(), model: Some("haiku".into()), effort: None });
            let child = core.blank(Area::Project, None);
            let c = core.convs.get_mut(&child).unwrap();
            (c.started_by, c.title, c.stopped) = (Some(parent.clone()), "Ship PR".into(), stopped);
            // A wait declared in the turn stays saved; it never hides a Stop or a failure.
            let problem = stopped.then_some("reason.wait.stopped");
            c.waiting_on = serde_json::from_value(serde_json::json!({"note": "CI", "since": 1, "problem": problem})).unwrap();
            let mut reply = Message::new("r1".into(), Role::Assistant, "Pushed pull request #135.".into(), 2);
            (reply.status, reply.error) = (Some(status), error.map(str::to_string));
            reply.wrap_up = Some(
                crate::progress::wrap_up(
                    &serde_json::json!({"outcome": "done", "did": "Pushed #135.", "result": "PR open.", "verified": "tests"}),
                )
                .unwrap(),
            );
            c.messages.push(reply);
            let state = state_of(false, &core.convs[&child]);
            core.tell_parent(&child);
            let p = &core.convs[&parent];
            let report =
                p.queued.iter().map(|q| q.text.clone()).chain(p.messages.iter().map(|m| m.text.clone())).find(|t| t.contains("Ship PR"));
            (state, report.expect("the parent got the report"))
        });
        engine.shutdown();
        out
    }

    #[tokio::test]
    async fn a_child_cut_by_silence_after_delivering_reports_the_exact_reason() {
        for (status, key) in [(Status::Error, "reason.silent.unconfirmed"), (Status::Stopped, "reason.silent")] {
            let (_, report) = child_report(status, Some(key), false);
            assert!(report.contains(&format!("Failure reason: {key}")), "{report}");
            assert!(!report.contains("stopped by the person"), "Faber's silence stop is not the person's: {report}");
            assert!(report.contains("PR open."), "the delivered work stays in the report: {report}");
        }
    }

    #[tokio::test]
    async fn the_persons_stop_outranks_a_declared_wait() {
        let (state, report) = child_report(Status::Stopped, None, true);
        assert_eq!(state, State::Stopped);
        assert!(report.contains("stopped by the person"), "{report}");
    }

    #[tokio::test]
    async fn a_wait_declared_after_the_persons_stop_never_arms_a_return() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, crate::events::Session::from_fn(|_| {}), &[("answer", &[])]);
        engine.with_saves(false, |core| {
            core.convs.get_mut(&id).unwrap().stopped = true;
            core.session_wait(&id, vec![], Some("CI".into())).unwrap();
            assert_eq!(core.convs[&id].waiting_on.as_ref().unwrap().problem.as_deref(), Some("reason.wait.stopped"));
            assert!(!core.wait_timers.contains_key(&id), "no timer after the person's Stop");
        });
        engine.shutdown();
    }

    #[cfg(windows)]
    #[test]
    fn startup_keeps_quiet_team_members_when_stable_histories_cannot_be_opened() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = crate::workspace::TestRoot::new();
        let store = Store::new(dir.path());
        let conversations = dir.path().join("conversations");
        for (member, lead, status) in [("quiet", "quiet-lead", Status::Done), ("cut", "cut-lead", Status::Streaming)] {
            let record = json!({
                "id": member, "title": "Worker", "area": "project", "projectId": "p", "createdAt": 0, "updatedAt": 1,
                "startedBy": lead, "messages": [{"id": "answer", "role": "assistant", "text": "Some work", "at": 1, "status": status}]
            });
            std::fs::write(conversations.join(format!("{member}.json")), serde_json::to_vec(&record).unwrap()).unwrap();
        }
        assert_eq!(store.list().len(), 2);
        store.keep_list(true);
        drop(store);
        let path = conversations.join("quiet.json");
        let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(&path).unwrap();
        let store = Store::new(dir.path());
        let scan = store.start_scan();
        assert!(scan.started_by.iter().any(|id| id == "quiet"));
        assert!(store.get("quiet").is_none(), "test holds the full conversation closed to readers");
        let (pending, teams) = Core::restore_parent_reports(&store, &scan);
        drop(lock);
        assert_eq!(pending, HashMap::from([("cut-lead".into(), HashSet::from(["cut".into()]))]));
        assert_eq!(teams, HashMap::from([("quiet-lead".into(), vec!["quiet".into()])]));
    }

    #[tokio::test]
    async fn waiting_after_a_merged_child_report_only_tracks_children_that_still_owe_a_report() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            for pending in [true, false] {
                let lead = core.blank(Area::Project, Some("p".into()));
                let finished = core.blank(Area::Project, Some("p".into()));
                let c = core.convs.get_mut(&finished).unwrap();
                c.started_by = Some(lead.clone());
                let mut answer = Message::new("done".into(), Role::Assistant, "Work finished.".into(), 2);
                answer.status = Some(Status::Done);
                c.messages.push(answer);

                let mut merged = Message::new("another-report".into(), Role::User, "Two reports arrived.".into(), 3);
                let mut wake = Wake::of("wake.childDone");
                wake.ids.push(format!("report-{finished}-done"));
                merged.wake = Some(wake);
                core.convs.get_mut(&lead).unwrap().messages.push(merged);

                let mut on = vec![finished];
                if pending {
                    let working = core.blank(Area::Project, Some("p".into()));
                    core.convs.get_mut(&working).unwrap().started_by = Some(lead.clone());
                    on.push(working);
                }
                let result = core.session_wait(&lead, on.clone(), Some("Waiting for the reports".into())).unwrap();
                if pending {
                    assert_eq!(core.convs[&lead].waiting_on.as_ref().map(|w| w.ids.as_slice()), Some(&on[1..]));
                    assert!(core.wait_timers.contains_key(&lead));
                } else {
                    assert!(result.contains("already reported"), "{result}");
                    assert!(core.convs[&lead].waiting_on.is_none());
                    assert!(!core.wait_timers.contains_key(&lead));
                }
            }
        });
        engine.shutdown();
    }

    #[test]
    fn durable_wait_legacy_deadline_survives_serialization() {
        let wait: crate::model::WaitingOn = serde_json::from_value(serde_json::json!({"since":1000,"note":"build"})).unwrap();
        let saved = serde_json::to_value(wait).unwrap();
        assert_eq!(saved["until"], 301000, "legacy waits must have a finite persisted deadline");
    }

    #[test]
    fn session_control_stopped_with_queued_messages_is_not_working() {
        let mut c: Conversation = serde_json::from_value(serde_json::json!({
            "id":"child","title":"Work","area":"project","createdAt":0,"updatedAt":0,"messages":[],
            "queued":[{"id":"q","text":"Next","choice":{"vendor":"claude"},"at":0}]
        }))
        .unwrap();
        let mut reply = Message::new("reply".into(), Role::Assistant, "Done".into(), 0);
        reply.status = Some(Status::Done);
        c.messages.push(reply);
        c.stopped = true;
        assert_eq!(state_of(false, &c), State::Stopped);
        assert_eq!(state_of(true, &c), State::Working);
        c.stopped = false;
        c.messages[0].status = Some(Status::Stopped);
        assert_eq!(state_of(false, &c), State::Stopped);
        c.messages[0].permissions =
            Some(vec![Permission { request_id: "q".into(), tool: "AskUserQuestion".into(), input: serde_json::Value::Null, epoch: None }]);
        assert_eq!(state_of(false, &c), State::Waiting);
    }

    #[test]
    fn a_saved_streaming_or_unconcluded_conversation_is_never_finished() {
        let mut c: Conversation = serde_json::from_value(serde_json::json!({
            "id":"child","title":"Work","area":"project","createdAt":0,"updatedAt":0,"messages":[]
        }))
        .unwrap();
        let mut m = Message::new("reply".into(), Role::Assistant, "Still working".into(), 0);
        m.status = Some(Status::Streaming);
        c.messages.push(m);
        assert_eq!(state_of(false, &c), State::Working);
        settle(c.messages.last_mut().unwrap());
        assert_ne!(state_of(false, &c), State::Concluded);
        c.messages.last_mut().unwrap().status = Some(Status::Done);
        c.messages.last_mut().unwrap().closed = false;
        c.messages.last_mut().unwrap().wrap_up = Some(crate::model::WrapUp { recap: "Unfinished.".into(), ..Default::default() });
        assert_eq!(state_of(false, &c), State::Unconcluded);
    }

    #[tokio::test]
    async fn numbered_reads_page_long_text_and_leave_closed_conversations_out_of_memory() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("p".into()));
            let child = core.blank(Area::Project, Some("p".into()));
            let foreign = core.blank(Area::Project, Some("other".into()));
            let text = "á".repeat(9001);
            let c = core.convs.get_mut(&child).unwrap();
            c.messages.push(Message::new("u".into(), Role::User, "Request".into(), 0));
            c.messages.push(Message::new("a".into(), Role::Assistant, text.clone(), 1));
            core.save(&child);
            core.convs.remove(&child);

            let page = |from, limit, offset| {
                let page = history(core, &parent, &child, from, limit).unwrap();
                serde_json::from_str::<serde_json::Value>(&Core::sibling_messages(&page, from, limit, offset)).unwrap()
            };
            let both = page(1, 2, 0);
            assert_eq!(both["count"], 2);
            assert_eq!(both["messages"][0]["text"], "Request");
            assert_eq!(both["messages"][1]["nextOffset"], 2000);
            let mut joined = String::new();
            for offset in [0, 4000, 8000] {
                let one = page(2, 1, offset);
                let m = &one["messages"][0];
                assert_eq!(m["id"], "a");
                assert_eq!(m["role"], "assistant");
                assert_eq!(m["nextOffset"].as_u64(), (offset < 8000).then_some(offset as u64 + 4000));
                joined.push_str(m["text"].as_str().unwrap());
            }
            assert_eq!(joined, text);
            assert_eq!(page(3, 1, 0)["messages"], json!([]));
            assert!(!core.convs.contains_key(&child));
            assert!(history(core, &parent, &foreign, 1, 1).is_err());
            core.convs.get_mut(&foreign).unwrap().project_id = Some("p".into());
            core.convs.get_mut(&foreign).unwrap().temporary = true;
            assert!(history(core, &parent, &foreign, 1, 1).is_err());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn recent_command_results_keep_the_scope_and_hide_captured_secrets() {
        use crate::background::{Capture, State, Update};
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let seed = || super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
        let reader = seed();
        let child = seed();
        let peer = seed();
        let stranger = seed();
        let (selected, sid) = engine.with(|core| {
            for id in [&reader, &child, &peer, &stranger] {
                let c = core.convs.get_mut(id).unwrap();
                c.area = Area::Project;
                c.project_id = Some(if id == &stranger { "other-project" } else { "this-project" }.into());
                c.started_by = (id == &child).then(|| reader.clone());
            }
            let sid = core.live[&child].sid;
            core.handle(
                &child,
                sid,
                EngineEvent::Background(Update::Start {
                    id: "build".into(),
                    command: "TOKEN=example-test-label\nbuild".into(),
                    stoppable: false,
                    turn_ended: true,
                }),
            );
            let id = core.background.list().items[0].id.clone();
            core.handle(
                &child,
                sid,
                EngineEvent::Background(Update::Output {
                    id: "build".into(),
                    text: "unpublished partial output".into(),
                    capture: Capture::Delta,
                    source_truncated: false,
                }),
            );
            (id, sid)
        });
        let request = json!({"action":"command_results", "id":child, "tool":selected, "runningOnly":true});
        for caller in [&reader, &child] {
            let result = engine.project_command_results(caller, &request).unwrap();
            assert_eq!(result["scope"], "runningInThisFaberRun");
            assert_eq!(result["items"][0]["id"], selected);
            assert_eq!(result["items"][0]["state"], "running");
            assert!(result["items"][0]["ended"].is_null());
            assert_eq!(result["items"][0]["label"], format!("{HIDDEN_LINE}\nbuild"));
            assert_eq!(result["items"][0]["outputAvailable"], false);
            assert_eq!(result["items"][0]["terminalOutput"], false);
            assert!(!result.to_string().contains("unpublished partial output"));
        }
        assert!(engine.project_command_results(&peer, &request).is_err());
        assert!(engine.project_command_results(&stranger, &request).is_err());
        let recent = engine.project_command_results(&reader, &json!({"action":"command_results", "id":child})).unwrap();
        assert_eq!(recent["items"], json!([]));
        engine.with(|core| {
            core.handle(&child, sid, EngineEvent::Background(Update::End { id: "build".into(), state: State::Failed }));
            core.handle(
                &child,
                sid,
                EngineEvent::Background(Update::Output {
                    id: "build".into(),
                    text: "TOKEN=example-test-value\nMissing input".into(),
                    capture: Capture::Snapshot,
                    source_truncated: false,
                }),
            );
            assert!(core.background.list().items.is_empty());
            core.try_save(&child).unwrap();
            core.convs.remove(&child);
        });
        assert!(engine.project_command_results(&reader, &request).is_err());
        let list = engine.project_command_results(&reader, &json!({"action":"command_results", "id":child})).unwrap();
        assert_eq!(list["items"][0]["id"], selected);
        assert_eq!(list["items"][0]["label"], format!("{HIDDEN_LINE}\nbuild"));
        assert_eq!(list["items"][0]["state"], "failed");
        assert_eq!(list["items"][0]["outputAvailable"], true);
        assert_eq!(list["items"][0]["terminalOutput"], false, "ending alone does not turn an earlier read into terminal output");
        assert!(list["items"][0].get("output").is_none());
        let request = json!({"action":"command_results", "id":child, "tool":selected});
        let result = engine.project_command_results(&reader, &request).unwrap();
        assert_eq!(result["items"][0]["output"], format!("{HIDDEN_LINE}\nMissing input"));
        assert!(engine.project_command_results(&peer, &request).is_err());
        assert!(engine.project_command_results(&stranger, &request).is_err());
        assert!(engine.project_command_results(&reader, &json!({"action":"command_results","limit":21})).is_err());
        engine.with(|core| {
            let sid = core.live[&reader].sid;
            core.handle(
                &reader,
                sid,
                EngineEvent::Tool {
                    id: "result-read".into(),
                    name: sibling::TOOL.into(),
                    input: json!({"action":" command_results ", "id":child, "tool":selected}),
                    parent: None,
                },
            );
            core.handle(
                &reader,
                sid,
                EngineEvent::ToolResult { id: "result-read".into(), ok: true, declined: false, exit_code: None, text: result.to_string() },
            );
            let tool = core.tool_of(&reader, "result-read").unwrap();
            assert_eq!(tool.output.as_deref(), Some(""));
            core.try_save(&reader).unwrap();
            let saved = core.store.get(&reader).unwrap();
            assert!(!serde_json::to_string(&saved).unwrap().contains("Missing input"));
        });
        for (text, source_truncated, truncated) in
            [("omitted\n".repeat(50), false, true), ("a partial first line".into(), true, true), (String::new(), false, false)]
        {
            engine.with(|core| {
                let sid = core.live[&child].sid;
                core.handle(
                    &child,
                    sid,
                    EngineEvent::Background(Update::Output { id: "build".into(), text, capture: Capture::Terminal, source_truncated }),
                );
            });
            let clipped = engine.project_command_results(&reader, &request).unwrap();
            assert_eq!(clipped["items"][0]["terminalOutput"], true);
            assert_eq!(clipped["items"][0]["outputTruncated"], truncated);
            assert_eq!(clipped["items"][0]["outputAvailable"], false);
            assert!(clipped["items"][0].get("output").is_none());
        }
        engine.with(|core| core.background.forget_finished(&child));
        assert!(engine.project_command_results(&reader, &request).is_err());
    }

    #[tokio::test]
    async fn numbered_reply_details_and_selected_steps_respect_the_reader_scope() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("p".into()));
            let child = core.blank(Area::Project, Some("p".into()));
            let peer = core.blank(Area::Project, Some("p".into()));
            let foreign = core.blank(Area::Project, Some("other".into()));
            let c = core.convs.get_mut(&child).unwrap();
            c.started_by = Some(parent.clone());
            let body = "á".repeat(4100);
            let mut m = Message::new("reply".into(), Role::Assistant, body.clone(), 0);
            m.status = Some(Status::Error);
            m.closed = true;
            m.error = Some(format!("Connection refused\napi_key=sample-should-be-hidden\n{}", "more detail ".repeat(40)));
            m.wrap_up = Some(WrapUp {
                recap: "The check failed. Token usage stayed under the limit.".into(),
                ask: Some("Retry?".into()),
                actions: vec!["Retry".into()],
                ..Default::default()
            });
            let clipped: String = format!("Complete line\n{}", "unlabeled-fragment".repeat(300)).chars().take(4000).collect();
            let output = format!("{}\nAuthorization:\n  folded-value\n\x1b[31mSyntaxError: Unexpected token\x1b[0m\nPermission denied\nPASSWORD=plain-value\nhttps://local-user:sample-info@example.test/path", "prior detail ".repeat(140));
            m.tools = Some(
                serde_json::from_value(json!([
                    {"id":"failed", "name":"Bash", "input":"internal arguments", "ok":false, "check":true,
                     "output":output, "started":20, "ended":50},
                    {"id":"refused", "name":"Bash", "input":null, "ok":false, "declined":true, "output":"not run"},
                    {"id":"ok", "name":"Bash", "input":null, "ok":true, "output":"successful output"},
                    {"id":"clipped", "name":"Bash", "input":null, "ok":false, "output":clipped},
                    {"id":"native-search", "name":"webSearch", "input":null, "ok":true,
                     "output":format!("shipping docs\napi_key=sample-should-be-hidden\nversion notes\n{}", "é".repeat(1500))},
                    {"id":"clipped-search", "name":"webSearch", "input":null, "ok":true, "output":"x".repeat(4000)},
                    {"id":"search-request", "name":"WebSearch", "input":{"query":"release notes","other":"hidden input"},
                     "ok":true, "output":"hidden result"},
                    {"id":"page-request", "name":"WebFetch", "input":{"url":"https://example.test/docs","prompt":"hidden prompt"},
                     "ok":true, "output":"hidden page body"},
                    {"id":"retained", "name":"Bash", "input":null, "ok":false, "failure_tail":"Could not save the copy"}
                ]))
                .unwrap(),
            );
            c.messages.push(m);

            let read = |offset| {
                let page = history(core, &parent, &child, 1, 1).unwrap();
                serde_json::from_str::<serde_json::Value>(&Core::sibling_messages(&page, 1, 1, offset)).unwrap()
            };
            let first = read(0);
            let reply = &first["messages"][0];
            assert_eq!(reply["status"], "error");
            assert_eq!(reply["closed"], true);
            assert_eq!(reply["failedSteps"], 3);
            assert_eq!(reply["errorTruncated"], true);
            let error = reply["error"].as_str().unwrap();
            assert!(error.contains("Connection refused") && !error.contains("sample-should-be-hidden"));
            let part = reply["text"].as_str().unwrap();
            assert_eq!(part.chars().count() + error.chars().count(), sibling::MAX_READ);
            let next = read(reply["nextOffset"].as_u64().unwrap() as usize);
            let rest = &next["messages"][0];
            assert!(rest.get("error").is_none());
            assert_eq!(format!("{part}{}", rest["text"].as_str().unwrap()), format!("{body}\n\nThe check failed. Token usage stayed under the limit.\nRetry? (Retry)"));

            let page = |reader, offset, tool| {
                let page = history(core, reader, &child, 1, 1).unwrap();
                Core::sibling_steps(&page, reader, 1, 1, offset, tool, false).unwrap()
            };
            let overview = page(&parent, 0, None);
            assert_eq!(overview["count"], 9);
            assert_eq!(overview["nextOffset"], 1);
            assert_eq!(overview["tools"][0]["durationMs"], 30);
            assert_eq!(overview["tools"][0]["diagnosticAvailable"], true);
            assert!(overview["tools"][0].get("diagnostic").is_none());
            assert_eq!(page(&parent, 1, None)["tools"][0]["declined"], true);
            assert_eq!(page(&parent, 9, None)["tools"], json!([]));

            for reader in [&parent, &child] {
                let search = page(reader, 0, Some("native-search"));
                assert_eq!(search["tools"][0]["diagnosticAvailable"], true);
                let terms = search["tools"][0]["diagnostic"].as_str().unwrap();
                assert!(terms.starts_with(&format!("shipping docs\n{HIDDEN_LINE}\nversion notes\n")));
                assert_eq!(terms.chars().count(), 1500);
                assert_eq!(search["tools"][0]["diagnosticTruncated"], true);
                for (tool, text) in [("search-request", "release notes"), ("page-request", "https://example.test/docs")] {
                    let web = page(reader, 0, Some(tool));
                    assert_eq!(web["tools"][0]["diagnostic"], text);
                    assert!(!web.to_string().contains("hidden"));
                }
                let failure = page(reader, 0, Some("failed"));
                let words = failure["tools"][0]["diagnostic"].as_str().unwrap();
                assert!(words.contains("Permission denied") && words.contains("SyntaxError: Unexpected token"));
                assert_eq!(failure["tools"][0]["diagnosticTruncated"], true);
                for hidden in ["folded-value", "plain-value", "internal arguments", "local-user", "sample-info"] {
                    assert!(!failure.to_string().contains(hidden));
                }
            }
            let clipped = page(&parent, 0, Some("clipped"));
            assert_eq!(clipped["tools"][0]["diagnostic"], "Complete line");
            assert_eq!(clipped["tools"][0]["diagnosticTruncated"], true);
            let clipped_search = page(&parent, 0, Some("clipped-search"));
            assert_eq!(clipped_search["tools"][0]["diagnostic"], "");
            assert_eq!(clipped_search["tools"][0]["diagnosticTruncated"], true);
            assert_eq!(page(&parent, 0, Some("retained"))["tools"][0]["diagnostic"], "Could not save the copy");
            for tool in ["failed", "native-search", "search-request", "page-request", "retained"] {
                let detail = page(&peer, 0, Some(tool));
                assert_eq!(detail["tools"][0]["diagnosticAvailable"], false);
                assert!(detail["tools"][0].get("diagnostic").is_none());
            }
            for tool in ["ok", "refused"] {
                assert!(page(&parent, 0, Some(tool))["tools"][0].get("diagnostic").is_none());
            }
            assert!(history(core, &foreign, &child, 1, 1).is_err());
            core.convs.get_mut(&child).unwrap().temporary = true;
            assert!(history(core, &parent, &child, 1, 1).is_err());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn reading_again_returns_a_replaced_recap_of_the_same_length() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            let reader = core.blank(Area::Project, Some("p".into()));
            let target = core.blank(Area::Project, Some("p".into()));
            let mut reply = Message::new("reply".into(), Role::Assistant, "Checked the copy.".into(), 0);
            reply.wrap_up = Some(WrapUp { recap: "Pending".into(), ..Default::default() });
            core.convs.get_mut(&target).unwrap().messages.push(reply);

            assert!(core.read_sibling(&reader, &target, &SavedPreviews::new()).unwrap().contains("Pending"));
            assert!(core.read_sibling(&reader, &target, &SavedPreviews::new()).unwrap().contains("nothing new"));
            core.convs.get_mut(&target).unwrap().messages[0].wrap_up.as_mut().unwrap().recap = "Checked".into();
            let updated = core.read_sibling(&reader, &target, &SavedPreviews::new()).unwrap();
            assert!(updated.contains("Checked the copy.\n\nChecked"), "{updated}");
            assert!(core.read_sibling(&reader, &target, &SavedPreviews::new()).unwrap().contains("nothing new"));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn project_agent_reads_keep_a_member_working_while_its_command_runs() {
        use super::super::command_wait::tests::{WAITING, turn_ends_with_render};

        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let member = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with_saves(false, |core| {
            let reader = core.blank(Area::Project, Some("test-project".into()));
            assert!(core.waiting_command(&member).is_some());
            let list = core.list_siblings(&reader, 0, sibling::LIST_PAGE_SIZE).unwrap();
            let read = core.read_sibling(&reader, &member, &SavedPreviews::new()).unwrap();
            assert!(list.contains(&format!("(id {member}) is still working")), "{list}");
            assert!(read.contains("is still working"), "{read}");
            assert!(read.contains("The draft render is still running."), "{read}");

            core.convs.get_mut(&member).unwrap().messages.last_mut().unwrap().permissions = Some(vec![Permission {
                request_id: "approve".into(),
                tool: "ExitPlanMode".into(),
                input: serde_json::Value::Null,
                epoch: None,
            }]);
            let list = core.list_siblings(&reader, 0, sibling::LIST_PAGE_SIZE).unwrap();
            let read = core.read_sibling(&reader, &member, &SavedPreviews::new()).unwrap();
            assert!(list.contains("is waiting for the person"), "{list}");
            assert!(read.contains("is waiting for the person"), "{read}");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn project_agents_cannot_resume_or_archive_a_member_waiting_for_its_command() {
        use super::super::command_wait::tests::turn_ends_with_render;

        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let member = turn_ends_with_render(&engine, Vendor::Claude, "The draft render is still running.");
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("test-project".into()));
            core.convs.get_mut(&member).unwrap().started_by = Some(parent.clone());
            assert!(core.waiting_command(&member).is_some());
            let resumed = core.session_resume(&parent, &member, None);
            let archived = core.session_archive(&parent, &member);
            assert_eq!(
                (resumed, archived),
                (Err("the conversation is working".into()), Err("the conversation is working or has a queued turn".into()))
            );
            assert!(core.convs[&member].archived_at.is_none());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_member_waiting_for_its_command_keeps_its_working_place() {
        use super::super::command_wait::tests::{WAITING, turn_ends_with_render};

        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let member = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("test-project".into()));
            for child in [
                member.clone(),
                core.blank(Area::Project, Some("test-project".into())),
                core.blank(Area::Project, Some("test-project".into())),
            ] {
                core.convs.get_mut(&child).unwrap().started_by = Some(parent.clone());
            }
            assert!(core.sibling_room("test-project").unwrap_err().starts_with("three conversations"));
            assert_eq!(core.sibling_room("other"), Ok(()));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_members_command_wait_is_working_in_the_plan() {
        use super::super::command_wait::tests::{WAITING, turn_ends_with_render};

        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let member = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("test-project".into()));
            let mut plan =
                crate::progress::plan(None, &json!({"steps":[{"id":"render","text":"Render","state":"in_progress"}]}), 1).unwrap().unwrap();
            plan.steps[0].conversation = Some(member);
            core.convs.get_mut(&parent).unwrap().plan = Some(plan);
            assert_eq!(core.shown_plan(&parent).unwrap().steps[0].member, Some(MemberState::Running));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn list_and_read_do_not_finish_a_streaming_conversation_on_disk() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let (send, mut replies) = tokio::sync::mpsc::unbounded_channel();
        let session = Session::from_fn(move |cmd| {
            if let crate::events::SessionCommand::HelperReply { ok, text, .. } = cmd {
                send.send((ok, text)).unwrap();
            }
        });
        let parent = super::super::answer::tests::seed(&engine, session, &[("reading", &[])]);
        let (child, other_project) = engine.with_saves(false, |core| {
            let reader = core.convs.get_mut(&parent).unwrap();
            reader.area = Area::Project;
            reader.project_id = Some("p".into());
            let child = core.blank(Area::Project, Some("p".into()));
            let c = core.convs.get_mut(&child).unwrap();
            c.title = "Unfinished member".into();
            let mut m = Message::new("reply".into(), Role::Assistant, "Still working".into(), 0);
            m.status = Some(Status::Streaming);
            c.messages.push(m);
            core.save(&child);
            core.convs.remove(&child);
            let list = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(list.contains("Unfinished member") && list.contains("still working"), "{list}");
            assert!(!list.contains("concluded its task"), "{list}");
            let other_project = core.blank(Area::Project, Some("different-project".into()));
            let other = core.convs.get_mut(&other_project).unwrap();
            other.messages.push(Message::new("private".into(), Role::Assistant, "Other project words".into(), 0));
            core.save(&other_project);
            core.convs.remove(&other_project);

            core.conversation_call(&parent, "latest".into(), json!({"action":"read", "id":child}));
            assert!(!core.convs.contains_key(&child), "the read opened the saved history in the engine");
            (child, other_project)
        });
        let (ok, read) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        assert!(ok && read.contains("Still working"), "{read}");
        assert!(!read.contains("concluded its task") && !read.contains("is still working"), "{read}");

        engine.with_saves(false, |core| {
            assert!(!core.convs.contains_key(&child), "the read kept the saved history in the engine");
            let peer = core.blank(Area::Project, Some("p".into()));
            let open = core.convs.get_mut(&peer).unwrap();
            open.title = "Open peer".into();
            open.messages.push(Message::new("reply".into(), Role::Assistant, "Open words".into(), 0));
            core.conversation_call(&parent, "group".into(), json!({"action":"read","ids":[other_project, child, peer, child]}));
        });
        let (ok, read) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        let rows: Vec<_> = read.split("\n\n---\n\n").collect();
        assert!(ok && rows.len() == 4, "{read}");
        assert!(rows[0].contains(HISTORY_OUTSIDE_PROJECT) && !read.contains("Other project words"), "{read}");
        assert!(rows[1].contains("Unfinished member") && rows[1].contains("nothing new"), "{read}");
        assert!(rows[2].contains("Open words") && rows[3] == rows[1], "{read}");
        engine.with(|core| {
            assert!(!core.convs.contains_key(&child));
            assert!(!core.convs.contains_key(&other_project));
            core.conversation_call(&parent, "moved".into(), json!({"action":"read", "id":child}));
            // The person opens and moves the conversation while the saved read is queued.
            core.load(&child).unwrap().project_id = Some("different-project".into());
        });
        let (ok, read) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        assert!(!ok && read.contains(HISTORY_OUTSIDE_PROJECT) && !read.contains("Still working"), "{read}");
        engine.shutdown();
    }

    #[tokio::test]
    async fn the_project_agent_reads_header_positions_without_opening_other_histories() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let (send, mut replies) = tokio::sync::mpsc::unbounded_channel();
        let session = Session::from_fn(move |cmd| {
            if let crate::events::SessionCommand::HelperReply { ok, text, .. } = cmd {
                send.send((ok, text)).unwrap();
            }
        });
        let reader = super::super::answer::tests::seed(&engine, session, &[("read positions", &[])]);
        let (peer, foreign) = engine.with_saves(false, |core| {
            let caller = core.convs.get_mut(&reader).unwrap();
            (caller.area, caller.project_id) = (Area::Project, Some("p".into()));
            let mut saved = |project: &str| {
                let id = core.blank(Area::Project, Some(project.into()));
                let mut c = core.convs.remove(&id).unwrap();
                c.messages.push(Message::new("question".into(), Role::User, "Unselected words".into(), 1));
                c.messages.push(Message::new("answer".into(), Role::Assistant, "Private body words".into(), 2));
                core.store.save_checked(&c).unwrap();
                id
            };
            (saved("p"), saved("q"))
        });

        engine.with(|core| core.conversation_call(&reader, "headers".into(), json!({"action":"headers","id":peer,"role":"assistant"})));
        let (ok, text) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        assert!(ok, "{text}");
        let result: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(result["total"], 2);
        assert_eq!(result["matching"], 1);
        assert_eq!(result["headers"][0]["number"], 2);
        assert_eq!(result["headers"][0]["id"], "answer");
        assert!(!text.contains("Private body words"));

        engine.with(|core| core.conversation_call(&reader, "own".into(), json!({"action":"headers"})));
        let (ok, text) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        assert!(ok, "{text}");
        let result: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(result["id"], reader);
        assert_eq!(result["headers"][0]["role"], "user");
        assert_eq!(result["headers"][1], json!({"number":2,"id":"read positions","role":"assistant","at":2,"status":"streaming"}));

        engine.with(|core| {
            assert!(!core.convs.contains_key(&peer));
            core.conversation_call(&reader, "foreign".into(), json!({"action":"headers","id":foreign}));
        });
        let (ok, text) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        assert!(!ok && text.contains(HISTORY_OUTSIDE_PROJECT), "{text}");

        engine.with_saves(false, |core| {
            core.conversation_call(&reader, "moved".into(), json!({"action":"headers","id":peer}));
            core.load(&peer).unwrap().project_id = Some("q".into());
        });
        let (ok, text) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        assert!(!ok && text.contains(HISTORY_OUTSIDE_PROJECT), "{text}");

        engine.with_saves(false, |core| {
            core.convs.remove(&peer);
            core.conversation_call(&reader, "reader-moved".into(), json!({"action":"headers","id":peer}));
            core.convs.get_mut(&reader).unwrap().project_id = Some("q".into());
        });
        let (ok, text) = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
        assert!(!ok && text.contains(HISTORY_OUTSIDE_PROJECT), "{text}");
        engine.shutdown();
    }

    #[tokio::test]
    async fn the_project_agent_pages_older_conversations_without_opening_their_histories() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let (send, mut replies) = tokio::sync::mpsc::unbounded_channel();
        let session = Session::from_fn(move |cmd| {
            if let crate::events::SessionCommand::HelperReply { ok, text, .. } = cmd {
                send.send((ok, text)).unwrap();
            }
        });
        let reader = super::super::answer::tests::seed(&engine, session, &[("listing", &[])]);
        engine.with_saves(false, |core| {
            let lead = core.convs.get_mut(&reader).unwrap();
            lead.area = Area::Project;
            lead.project_id = Some("p".into());
            let mut expected = Vec::new();
            for n in 0..14 {
                let id = core.blank(Area::Project, Some("p".into()));
                let c = core.convs.get_mut(&id).unwrap();
                c.title = format!("History {n:02}");
                c.updated_at = 100 + n / 3;
                expected.push((std::cmp::Reverse(c.updated_at), id.clone()));
                core.save(&id);
                core.convs.remove(&id);
            }
            expected.sort();
            for (project, archived, temporary) in [("other", false, false), ("p", true, false), ("p", false, true)] {
                let id = core.blank(Area::Project, Some(project.into()));
                let c = core.convs.get_mut(&id).unwrap();
                c.title = "Excluded conversation".into();
                c.updated_at = 1_000;
                c.archived_at = archived.then_some(1);
                c.temporary = temporary;
                if !temporary {
                    core.save(&id);
                    core.convs.remove(&id);
                }
            }

            let mut call = |input| {
                core.conversation_call(&reader, "list".into(), input);
                let (ok, text) = replies.try_recv().unwrap();
                assert!(ok, "{text}");
                assert!(!text.contains("Excluded conversation") && !text.contains(&reader), "{text}");
                text
            };
            let first = call(json!({"action":"list"}));
            assert!(first.contains("nextOffset: 12"), "{first}");
            let second = call(json!({"action":"list","offset":12,"limit":1}));
            assert!(second.contains("nextOffset: 13"), "{second}");
            let last = call(json!({"action":"list","offset":13}));
            assert!(!last.contains("nextOffset"), "{last}");
            assert_eq!(call(json!({"action":"list","offset":14})), "No more project conversations at this offset.");
            for (index, (_, id)) in expected.iter().enumerate() {
                assert_eq!(first.contains(id), index < 12, "first page: {first}");
                assert_eq!(second.contains(id), index == 12, "second page: {second}");
                assert_eq!(last.contains(id), index == 13, "last page: {last}");
                assert!(!core.convs.contains_key(id), "listing reopened a saved conversation");
            }
            let lines: Vec<_> = first.lines().filter(|line| line.starts_with("- ")).collect();
            assert!(lines.iter().zip(&expected).all(|(line, (_, id))| line.contains(id)), "{first}");
        });
        engine.shutdown();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn listing_a_closed_project_conversation_does_not_open_its_history_again() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("p".into()));
            let child = core.blank(Area::Project, Some("p".into()));
            let folder = core.opts.data_dir.join("registered").to_string_lossy().into_owned();
            let c = core.convs.get_mut(&child).unwrap();
            c.title = "Waiting member".into();
            c.copy = Some(crate::model::WorkCopy {
                path: folder.clone(),
                branch: "faber/test".into(),
                root: "test project".into(),
                sub: "app".into(),
                base: None,
            });
            let mut reply = Message::new("reply".into(), Role::Assistant, "Finished my turn".into(), 1);
            reply.status = Some(Status::Done);
            reply.tools = Some(
                serde_json::from_value(json!([{"id":"helper","name":"Helper","input":null,"ok":true,
                    "transcript":{"pages":1,"failed":false}}]))
                .unwrap(),
            );
            c.messages.push(reply);
            c.waiting_on = Some(serde_json::from_value(json!({"since":1,"until":300001})).unwrap());
            c.stopped = true;
            let page = crate::transcript::TranscriptEntry {
                at: 0,
                kind: crate::transcript::TranscriptKind::Text,
                tool: None,
                text: "Saved helper note".into(),
            };
            core.store.transcripts.append_page(&child, "helper", 0, &[page]).unwrap();
            core.save(&child);
            let live = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(live.contains("was stopped by the person") && live.contains(&folder), "{live}");
            core.convs.remove(&child);

            let expected = format!("\"Waiting member\" (id {child}) is waiting");
            let first = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(first.contains(&expected) && first.contains(&folder), "{first}");
            core.store.keep_list(true);
            let file = core.opts.data_dir.join("conversations").join(format!("{child}.json"));
            let locked = std::fs::OpenOptions::new().read(true).write(true).open(&file).unwrap();
            locked.try_lock().unwrap();
            assert!(std::fs::metadata(&file).is_ok() && std::fs::read(&file).is_err());
            let cached = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(cached.contains(&expected) && cached.contains(&folder), "{cached}");
            let reopened = crate::store::Store::new(&core.opts.data_dir);
            let row = reopened.list().into_iter().find(|s| s.id == child).unwrap();
            assert_eq!(reopened.listed_project_detail(&row, "p"), Some((State::Waiting, Some(folder.clone()))));

            let later = crate::transcript::TranscriptEntry {
                at: 0,
                kind: crate::transcript::TranscriptKind::Text,
                tool: None,
                text: "New helper words".into(),
            };
            core.store.transcripts.append_page(&child, "helper", 1, std::slice::from_ref(&later)).unwrap();
            let pending = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(pending.contains("ended without a conclusion") && pending.contains(&folder), "{pending}");
            locked.unlock().unwrap();
            let recovered = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(recovered.contains(&expected) && recovered.contains(&folder), "{recovered}");

            locked.try_lock().unwrap();
            let unchanged = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(unchanged.contains(&expected) && unchanged.contains(&folder), "{unchanged}");
            core.store.transcripts.append_page(&child, "helper", 2, &[later]).unwrap();
            let changed = core.list_siblings(&parent, 0, sibling::LIST_PAGE_SIZE).unwrap();
            assert!(changed.contains("ended without a conclusion") && changed.contains(&folder), "{changed}");
            assert!(!core.convs.contains_key(&child));
        });
        engine.shutdown();
    }

    /// `count` conversations the lead started in project `p`, each mid-reply; `asking` ones hold a question for the person.
    fn started(core: &mut Core, count: usize, asking: bool) {
        for _ in 0..count {
            let id = core.blank(Area::Project, Some("p".into()));
            let mut reply = Message::new(new_id(), Role::Assistant, String::new(), 0);
            reply.status = Some(Status::Streaming);
            if asking {
                let question =
                    Permission { request_id: "q".into(), tool: "AskUserQuestion".into(), input: serde_json::Value::Null, epoch: None };
                reply.permissions = Some(vec![question]);
            }
            let c = core.convs.get_mut(&id).unwrap();
            c.started_by = Some("lead".into());
            c.messages.push(reply);
        }
    }

    #[tokio::test]
    async fn child_summary_tracks_its_reply_and_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            started(core, 1, false);
            let c = core.convs.values_mut().find(|c| c.started_by.is_some()).unwrap();
            assert_eq!(crate::conversation_state::child_state(c), "working");
            c.messages.last_mut().unwrap().status = Some(Status::Error);
            assert_eq!(crate::conversation_state::child_state(c), "failed");
            c.waiting_on = Some(crate::model::WaitingOn {
                storage_blocked: false,
                ids: vec![],
                note: "Waiting for review".into(),
                since: 0,
                until: 300_000,
                problem: None,
                pr: None,
            });
            assert_eq!(crate::conversation_state::child_state(c), "waiting");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_conversation_that_waits_for_others_stays_in_the_running_list() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            started(core, 1, false);
            let c = core.convs.values_mut().find(|c| c.started_by.is_some()).unwrap();
            c.messages.last_mut().unwrap().status = Some(Status::Done);
            assert!(crate::engine::running_item(c, None, None).is_none(), "a finished turn with nothing pending leaves the list");
            let wait = crate::model::WaitingOn {
                storage_blocked: false,
                ids: vec!["child".into()],
                note: "Waiting for the two lanes to land".into(),
                since: 7,
                until: 300_007,
                problem: None,
                pr: None,
            };
            c.waiting_on = Some(wait.clone());
            let item = crate::engine::running_item(c, None, None).expect("a waiting conversation is still at work");
            assert_eq!(item.state, crate::model::RunState::Working);
            assert_eq!(item.waiting_on, Some(wait));
            assert!(!item.settled);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_member_waiting_for_the_person_frees_a_working_place_but_stays_open() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            started(core, 3, true);
            assert_eq!(core.sibling_room("p"), Ok(()), "three waiting for the person leave room to work");
            started(core, 2, false);
            let full = core.sibling_room("p").unwrap_err();
            assert!(full.starts_with("five conversations") && full.contains("faber-wrap-up"), "{full}");
            assert_eq!(core.sibling_room("other"), Ok(()), "the limits are per project");
            started(core, 1, false);
            assert!(core.sibling_room("p").unwrap_err().starts_with("three conversations"), "three working is the first limit");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_member_that_ends_needing_the_person_tells_its_lead_and_ends_the_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("p".into()));
            let child = core.blank(Area::Project, Some("p".into()));
            // The lead is mid-turn, so the report waits in its queue where the test can read it.
            let mut turn = Message::new("lead".into(), Role::Assistant, String::new(), 1);
            turn.status = Some(Status::Streaming);
            let p = core.convs.get_mut(&parent).unwrap();
            p.messages.push(turn);
            p.waiting_on = Some(crate::model::WaitingOn {
                storage_blocked: false,
                ids: vec![child.clone()],
                note: String::new(),
                since: 1,
                until: 300001,
                problem: None,
                pr: None,
            });
            let c = core.convs.get_mut(&child).unwrap();
            c.started_by = Some(parent.clone());
            c.title = "Sidebar".into();
            let mut reply = Message::new("r1".into(), Role::Assistant, "Shipped; the checks still run.".into(), 2);
            reply.status = Some(Status::Done);
            reply.wrap_up = Some(
                crate::progress::wrap_up(&serde_json::json!({
                    "outcome": "needs_you",
                    "did": "Shipped the sidebar.",
                    "result": "The sidebar is on the base.",
                    "not_done": "The checks still run."
                }))
                .unwrap(),
            );
            c.messages.push(reply);
            assert_eq!(state_of(false, c), State::Waiting);
            assert!(!reports_later(state_of(false, c), c), "nothing else will ever wake its lead");
            c.waiting_on = Some(crate::model::WaitingOn {
                storage_blocked: false,
                ids: vec![],
                note: "review".into(),
                since: 2,
                until: 300002,
                problem: None,
                pr: None,
            });
            assert!(reports_later(state_of(false, c), c), "a member that waits for its own conversations reports when they do");
            c.waiting_on = None;

            core.tell_parent(&child);

            let p = &core.convs[&parent];
            let report = p.queued.iter().find(|q| q.id == format!("report-{child}-r1")).expect("the lead is told");
            assert!(report.text.contains("is waiting for the person to answer it"), "{}", report.text);
            assert!(report.text.contains("answer it with `ask`"), "{}", report.text);
            assert_eq!(p.waiting_on, None, "the lead no longer waits for a member that will not go on by itself");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_worker_conversation_ending_never_marks_its_plan_step_done() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with_saves(false, |core| {
            let parent = core.blank(Area::Project, Some("p".into()));
            let child = core.blank(Area::Project, Some("p".into()));
            let plan = crate::progress::plan(
                None,
                &serde_json::json!({
                    "steps": [
                        { "id": "model", "text": "Update model", "state": "in_progress" },
                        { "id": "screen", "text": "Update screen", "state": "pending" }
                    ]
                }),
                1,
            )
            .unwrap()
            .unwrap();
            let mut p = plan;
            p.steps[0].worker =
                Some(StepWorker { conversation: child.clone(), branch: "faber/model".into(), base: String::new(), joining: false });
            core.convs.get_mut(&parent).unwrap().plan = Some(p);

            let c = core.convs.get_mut(&child).unwrap();
            c.started_by = Some(parent.clone());
            c.title = "Update model".into();
            let mut reply = Message::new("r1".into(), Role::Assistant, "Model updated.".into(), 2);
            reply.status = Some(Status::Done);
            reply.closed = true;
            reply.wrap_up = Some(
                crate::progress::wrap_up(&serde_json::json!({
                    "outcome": "done",
                    "did": "Added evidence field to PlanStep.",
                    "result": "PlanStep stores optional evidence.",
                    "verified": "cargo test -p faber-engine --lib -- progress passed."
                }))
                .unwrap(),
            );
            c.messages.push(reply);

            core.tell_parent(&child);

            let step = &core.convs[&parent].plan.as_ref().unwrap().steps[0];
            assert_eq!(step.state, StepState::Active, "the worker ending never marks the parent's step done");
            assert_eq!(step.evidence, None);
            assert_eq!(
                step.worker.as_ref(),
                Some(&StepWorker { conversation: child.clone(), branch: "faber/model".into(), base: String::new(), joining: true }),
                "the worker stays attached and waits to be joined"
            );

            let refused = core.write_progress(&parent, crate::progress::PLAN_TOOL, &serde_json::json!({ "advance": true }));
            let refused = refused.unwrap_err();
            assert!(refused.contains(crate::progress::NO_EVIDENCE), "{refused}");
            assert_eq!(core.convs[&parent].plan.as_ref().unwrap().steps[0].state, StepState::Active);

            let ok = core
                .write_progress(
                    &parent,
                    crate::progress::PLAN_TOOL,
                    &serde_json::json!({ "advance": true, "evidence": "Joined branch and ran cargo test." }),
                )
                .unwrap();
            assert!(ok.contains("1 of 2"), "{ok}");
            let step = &core.convs[&parent].plan.as_ref().unwrap().steps[0];
            assert_eq!(step.state, StepState::Done);
            assert_eq!(step.evidence.as_deref(), Some("Joined branch and ran cargo test."));
            assert!(step.worker.is_some());
        });
        engine.shutdown();
    }
}
