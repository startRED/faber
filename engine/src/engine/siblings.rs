//! Conversations a project's agent starts by itself (`crate::sibling`): the engine makes a new conversation in the same
//! project, sends it the agent's brief as its first message on the conversation's own model choice, and answers the call
//! at once with the new id. In a project that keeps separate copies (R201) the brief waits for the new conversation's own
//! copy, made away from the engine's lock like the person's own first message, so it never runs in the project folder. A
//! read answers with where that conversation stands and its latest answer.
use super::*;
use crate::sibling::{self, Request, State};

/// Whether a conversation's agent gets the tool: every project conversation's. Only one no other agent started may start
/// conversations, so one level only.
pub(super) fn offers(area: Area) -> bool {
    area == Area::Project
}

/// What the agent of a standing goal's fix is told once the project's day is out of money (`goals.rs`); said in a sentence the
/// call's answer completes ("No conversation was started: ...").
pub(super) const GOAL_CAP: &str = "the day's limit for fixes of the project's goals is used up, so this fix stops here for today";

/// The conversations a list names at most.
const MAX_LISTED: usize = 12;

/// Where a conversation stands; `busy` is whether its CLI runs a turn now.
fn state_of(busy: bool, c: &Conversation) -> State {
    let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
    let asking = last.is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()));
    match last.and_then(|m| m.status.as_ref()) {
        _ if asking => State::Waiting,
        // Its copy is still being made: the brief follows it.
        _ if c.messages.is_empty() => State::Working,
        Some(Status::Streaming) => State::Working,
        _ if busy || c.compacting || !c.queued.is_empty() => State::Working,
        Some(Status::Error) => State::Failed,
        Some(Status::Stopped) => State::Stopped,
        _ => State::Finished,
    }
}

impl Core {
    /// The agent called the conversation tool. Every call is answered at once, with why when nothing was done.
    pub(super) fn conversation_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        let (ok, text) = match sibling::request(&input) {
            Err(why) => (false, format!("Nothing was done: {why}.")),
            Ok(Request::Start { title, brief }) => match self.start_sibling(conv, &title, &brief) {
                Ok((id, true)) => (true, sibling::started(&title, &id)),
                Ok((id, false)) => (false, sibling::failed(&title, &id)),
                Err(why) => (false, format!("No conversation was started: {why}.")),
            },
            Ok(Request::Read { id }) => match self.read_sibling(conv, &id) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing to read: {why}.")),
            },
            Ok(Request::List) => match self.list_siblings(conv) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing to list: {why}.")),
            },
            Ok(Request::Ask { id, message }) => match self.ask_sibling(conv, &id, &message) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing was sent: {why}.")),
            },
            Ok(Request::Context) => (true, self.context_answer(conv)),
            Ok(Request::Compact { next }) => match self.plan_compaction(conv, next) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing was scheduled: {why}.")),
            },
            Ok(Request::Clear { next }) => match self.plan_clear(conv, next) {
                Ok(()) => (true, sibling::CLEAR_PLANNED.to_string()),
                Err(why) => (false, format!("Nothing was scheduled: {why}.")),
            },
            Ok(Request::Archive) => match self.set_archived(conv, true) {
                Ok(()) => (true, sibling::ARCHIVED.to_string()),
                Err(_) => (false, "Nothing was archived: the conversation is gone.".to_string()),
            },
        };
        // The answer goes back only to the process that asked.
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(&call, ok, text);
        }
    }

    fn start_sibling(&mut self, conv: &str, title: &str, brief: &str) -> Result<(String, bool), &'static str> {
        if self.goal_out_of_money(conv) {
            return Err(GOAL_CAP);
        }
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
        let project = parent.project_id.clone().ok_or("this conversation has no project")?;
        let choice = parent.choice.clone().ok_or("this conversation has no model chosen")?;
        // The cap is on what agents start: a conversation counts from the call on, its copy still being made, routing and
        // waiting for quota included, not only once its CLI runs. A message the person sends to one is the person's own.
        let running = self
            .convs
            .values()
            .filter(|c| c.started_by.is_some() && c.project_id.as_deref() == Some(project.as_str()))
            .filter(|c| {
                self.live.get(&c.id).is_some_and(|l| l.turn.is_some())
                    || !c.queued.is_empty()
                    || c.messages.is_empty()
                    || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
            })
            .count();
        if running >= sibling::MAX_RUNNING {
            return Err("three conversations started by agents are already working in this project; wait for one to finish");
        }
        // A conversation an agent starts is never the person's own turn: the same quota rules as a helper hold.
        let ai = self.live.get(conv).map(|l| (l.vendor, l.model.as_deref().and_then(family_of).map(str::to_string)));
        self.sibling_allowed(&ai)?;
        let id = self.blank(Area::Project, Some(project.clone()));
        if let Some(c) = self.convs.get_mut(&id) {
            c.started_by = Some(conv.to_string());
            c.goal_fix = fix;
            c.title = title.to_string();
        }
        let req = SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: Some(project),
            text: sibling::prompt(title, brief),
            images: vec![],
            files: vec![],
            choice,
        };
        // The common start path makes its copy off the lock and removes any copy that no longer belongs.
        match self.launch_sibling(&id, req, None) {
            Ok(()) => Ok((id, true)),
            // Saved with its first message before it failed: it stays, with the error in its reply, and the agent is told.
            Err(_) if self.convs.get(&id).is_some_and(|c| !c.messages.is_empty()) => Ok((id, false)),
            Err(_) => {
                let _ = self.remove(&id);
                Err("the conversation could not start")
            }
        }
    }

    /// Whether the parent's AI may run work an agent started (a helper's rules); no live process means nothing to check.
    fn sibling_allowed(&self, ai: &Option<(Vendor, Option<String>)>) -> Result<(), &'static str> {
        let Some((vendor, family)) = ai else { return Ok(()) };
        self.allows(*vendor, family.as_deref(), Purpose::UserTurn { by_hand: false }, false).map_err(|why| match why {
            Why::Billing => "this AI may be billed per use, so Faber does not start conversations on it by itself",
            _ => "this AI has no quota left right now",
        })
    }

    /// Sends the agent's brief as the new conversation's first message.
    pub(super) fn launch_sibling(&mut self, id: &str, req: SendRequest, wake: Option<Wake>) -> Result<(), String> {
        match self.send(req, wake) {
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

    fn read_sibling(&mut self, conv: &str, id: &str) -> Result<String, &'static str> {
        self.same_project(conv, id)?;
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some());
        let c = self.convs.get(id).ok_or("the conversation is gone")?;
        let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
        Ok(sibling::read(&c.title, state_of(busy, c), &last.map(Message::said).unwrap_or_default()))
    }

    /// The project's other conversations the person has not archived, the newest first. One never loaded since Faber
    /// started is idle, so its state comes from its file only when it is open.
    fn list_siblings(&mut self, conv: &str) -> Result<String, &'static str> {
        let project = self.convs.get(conv).and_then(|c| c.project_id.clone()).ok_or("this conversation has no project")?;
        let folder = self.store.projects().into_iter().find(|p| p.id == project).map(|p| p.path).unwrap_or_default();
        // ponytail: the store reads every conversation file, as the screen's list does; a kept index when lists grow large.
        let at = now();
        let copies = crate::project::copy::home(&self.opts.data_dir);
        let rows: Vec<sibling::Listed> = self
            .store
            .list()
            .into_iter()
            .filter(|s| s.id != conv && !s.archived && !s.title.is_empty() && s.project_id.as_deref() == Some(project.as_str()))
            .take(MAX_LISTED)
            .map(|s| {
                let busy = self.live.get(&s.id).is_some_and(|l| l.turn.is_some());
                let open = self.convs.get(&s.id);
                sibling::Listed {
                    state: open.map_or(if busy { State::Working } else { State::Finished }, |c| state_of(busy, c)),
                    folder: match open {
                        Some(c) => c.copy.as_ref().map(|copy| copy.path.clone()),
                        None => Some(copies.join(&s.id)).filter(|p| p.is_dir()).map(|p| p.to_string_lossy().into_owned()),
                    },
                    minutes: at.saturating_sub(s.updated_at) / 60_000,
                    id: s.id,
                    title: s.title,
                }
            })
            .collect();
        Ok(sibling::list(&folder, &rows))
    }

    /// A conversation an agent started stopped working (its turn ended, nothing is queued, Keep going is off): its report
    /// goes to the agent that started it by itself, so no agent waits or polls. An archived or gone parent hears nothing.
    pub(super) fn tell_parent(&mut self, conv: &str) {
        let busy = self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let Some(c) = self.convs.get(conv) else { return };
        let Some(parent) = c.started_by.clone() else { return };
        let state = state_of(busy, c);
        if c.keep_going.is_some() || state == State::Working || state == State::Waiting {
            return;
        }
        let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant).map(Message::said).unwrap_or_default();
        let text = sibling::finished(&c.title, conv, state, &last);
        if self.load(&parent).is_none_or(|p| p.archived_at.is_some()) {
            return;
        }
        // A report the day's money refuses is not lost without a word: the parent's latest reply says the limit is why.
        if self.send_by_itself(&parent, text, Wake::of("wake.childDone")).is_err_and(|e| e == goals::CAP) {
            self.goal_capped(&parent);
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
        let wake = Wake::of("wake.asked");
        self.send_by_itself(id, sibling::ask_prompt(&from, conv, message), wake).map_err(|why| match why.as_str() {
            "error.queueFull" => "the conversation's inbox is full; it cannot take another message yet",
            _ => "it could not take the message now",
        })?;
        Ok(sibling::asked(&title, id, busy))
    }
}
