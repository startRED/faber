//! Conversations a project's agent starts by itself (`crate::sibling`): the engine makes a new conversation in the same
//! project, sends it the agent's brief as its first message on the conversation's own model choice, and answers the call
//! at once with the new id. In a project that keeps separate copies (R201) the brief waits for the new conversation's own
//! copy, made away from the engine's lock like the person's own first message, so it never runs in the project folder. A
//! read answers with where that conversation stands and its latest answer.
use super::*;
use crate::sibling::{self, Request, State};

/// Whether a conversation's agent gets the tool: every project conversation's. Only one no other agent started may start
/// conversations, so one level only.
/// One read of another conversation (`Core::sibling_reads`): the reader's turn, the answer it was given, and how full its
/// context was then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SeenRead {
    turn: String,
    answer: String,
    used: u64,
}

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

/// Where a plan step's conversation stands (task 040), in a subagent row's words.
pub(super) fn member_state(busy: bool, c: &Conversation) -> MemberState {
    match state_of(busy, c) {
        State::Working => MemberState::Running,
        State::Waiting => MemberState::Waiting,
        State::Finished => MemberState::Done,
        State::Failed => MemberState::Failed,
        State::Stopped => MemberState::Stopped,
    }
}

impl Core {
    pub(super) fn restore_parent_reports(store: &Store) -> HashMap<String, HashSet<String>> {
        let mut pending: HashMap<String, HashSet<String>> = HashMap::new();
        for id in store.mentioning("\"pendingParentReport\"") {
            if let Some(c) = store.get(&id).filter(|c| c.pending_parent_report)
                && let Some(parent) = c.started_by
            {
                pending.entry(parent).or_default().insert(id);
            }
        }
        pending
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
        for child in pending {
            if self.convs.get(parent).is_none_or(|c| c.queued.len() >= MAX_QUEUED) {
                self.parent_reports.entry(parent.into()).or_default().insert(child);
                continue;
            }
            self.load(&child);
            self.tell_parent(&child);
            if self.convs.get(&child).is_some_and(|c| c.pending_parent_report) {
                self.parent_reports.entry(parent.into()).or_default().insert(child);
            }
        }
    }

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
        self.sibling_room(&project)?;
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
    /// Whether the project has room for one more conversation an agent starts. The cap is on what agents start: a conversation
    /// counts from the call on, its copy still being made, routing and waiting for quota included, not only once its CLI runs.
    /// One waiting for the person to answer it frees a working place, so the rest of the job goes on, but stays open. A message
    /// the person sends to one is the person's own.
    fn sibling_room(&self, project: &str) -> Result<(), &'static str> {
        let (mut working, mut open) = (0, 0);
        for c in self.convs.values().filter(|c| c.started_by.is_some() && c.project_id.as_deref() == Some(project)) {
            let busy = self.live.get(&c.id).is_some_and(|l| l.turn.is_some());
            if busy
                || !c.queued.is_empty()
                || c.messages.is_empty()
                || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
            {
                open += 1;
                working += usize::from(state_of(busy, c) != State::Waiting);
            }
        }
        if working >= sibling::MAX_RUNNING {
            return Err("three conversations started by agents are already working in this project; wait for one to finish");
        }
        if open >= sibling::MAX_OPEN {
            return Err("five conversations started by agents are already open in this project, and some wait for the person to answer \
                them; tell the person once with wrap_up, then wait for an answer or for one to finish");
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

    /// Reading the same conversation again in one turn with nothing new answers briefly: the owner's conversations read one
    /// 101 times, 118 of 481 reads returned what the agent already had. A context that shrank since (compacted) reads in full.
    fn read_sibling(&mut self, conv: &str, id: &str) -> Result<String, &'static str> {
        self.same_project(conv, id)?;
        let reader = self.convs.get(conv).map(|r| SeenRead {
            turn: r.messages.last().map(|m| m.id.clone()).unwrap_or_default(),
            answer: String::new(),
            used: r.context.as_ref().map_or(0, |f| f.used),
        });
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some());
        let c = self.convs.get(id).ok_or("the conversation is gone")?;
        let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
        let latest = last.map(Message::said).unwrap_or_default();
        let seen = reader.map(|r| SeenRead { answer: format!("{}:{}", last.map_or("", |m| m.id.as_str()), latest.len()), ..r });
        let key = (conv.to_string(), id.to_string());
        let again = seen.as_ref().zip(self.sibling_reads.get(&key)).is_some_and(|(now, before)| {
            !latest.trim().is_empty() && now.turn == before.turn && now.answer == before.answer && now.used >= before.used
        });
        let text =
            if again { sibling::read_again(&c.title, state_of(busy, c)) } else { sibling::read(&c.title, state_of(busy, c), &latest) };
        if let Some(seen) = seen.filter(|_| !again) {
            self.sibling_reads.insert(key, seen);
        }
        Ok(text)
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
        let reply = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
        let report_id = format!("report-{conv}-{}", reply.map_or("empty", |m| m.id.as_str()));
        let last = reply.map(Message::said).unwrap_or_default();
        let text = sibling::finished(&c.title, conv, state, &last);
        if self.load(&parent).is_none_or(|p| p.archived_at.is_some()) {
            self.finish_parent_report(conv, &parent);
            return;
        }
        let p = &self.convs[&parent];
        if p.queued.iter().any(|q| q.id == report_id) || p.messages.iter().any(|m| m.id == report_id) {
            if self.try_save(&parent).is_ok() {
                self.finish_parent_report(conv, &parent);
            }
            return;
        }
        if p.queued.len() >= MAX_QUEUED {
            self.convs.get_mut(conv).expect("child exists").pending_parent_report = true;
            self.save(conv);
            self.parent_reports.entry(parent).or_default().insert(conv.into());
            return;
        }
        // A report the day's money refuses is not lost without a word: the parent's latest reply says the limit is why.
        if self.send_by_itself_with_id(&parent, text, Wake::of("wake.childDone"), report_id.clone()).is_err_and(|e| e == goals::CAP) {
            self.goal_capped(&parent);
        }
        if self
            .convs
            .get(&parent)
            .is_some_and(|p| p.queued.iter().any(|q| q.id == report_id) || p.messages.iter().any(|m| m.id == report_id))
            && self.try_save(&parent).is_ok()
        {
            self.finish_parent_report(conv, &parent);
        }
    }

    fn finish_parent_report(&mut self, child: &str, parent: &str) {
        if let Some(c) = self.convs.get_mut(child).filter(|c| c.pending_parent_report) {
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
        let wake = Wake::of("wake.asked");
        self.send_by_itself(id, sibling::ask_prompt(&from, conv, message), wake).map_err(|why| match why.as_str() {
            "error.queueFull" => "the conversation's inbox is full; it cannot take another message yet",
            _ => "it could not take the message now",
        })?;
        Ok(sibling::asked(&title, id, busy))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Permission;

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
    async fn a_member_waiting_for_the_person_frees_a_working_place_but_stays_open() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|core| {
            started(core, 3, true);
            assert_eq!(core.sibling_room("p"), Ok(()), "three waiting for the person leave room to work");
            started(core, 2, false);
            let full = core.sibling_room("p").unwrap_err();
            assert!(full.starts_with("five conversations") && full.contains("wrap_up"), "{full}");
            assert_eq!(core.sibling_room("other"), Ok(()), "the limits are per project");
            started(core, 1, false);
            assert!(core.sibling_room("p").unwrap_err().starts_with("three conversations"), "three working is the first limit");
        });
        engine.shutdown();
    }
}
