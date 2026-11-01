//! Conversations a project's agent starts by itself (`crate::sibling`): the engine makes a new conversation in the same
//! project, sends it the agent's brief as its first message on the conversation's own model choice, and answers the call
//! at once with the new id. In a project that keeps separate copies (R201) the brief waits for the new conversation's own
//! copy, made away from the engine's lock like the person's own first message, so it never runs in the project folder. A
//! read answers with where that conversation stands and its latest answer.
use super::*;
use crate::progress;
use crate::sibling::{self, Ai, Request, State};

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
    if !busy && c.waiting_on.is_some() {
        return State::Waiting;
    }
    let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
    let asking = last.is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()));
    match last.and_then(|m| m.status.as_ref()) {
        _ if asking => State::Waiting,
        // Its copy is still being made: the brief follows it.
        _ if c.messages.is_empty() => State::Working,
        Some(Status::Streaming) => State::Working,
        Some(Status::Stopped) if !busy && c.stopped && last.is_some_and(|m| m.closed) => State::Interrupted,
        Some(Status::Stopped) if !busy && c.stopped => State::Stopped,
        _ if busy || c.compacting || !c.queued.is_empty() => State::Working,
        Some(Status::Error) => State::Failed,
        Some(Status::Stopped) if last.is_some_and(|m| m.closed) => State::Interrupted,
        Some(Status::Stopped) => State::Stopped,
        _ => State::Finished,
    }
}

impl Core {
    /// Reports owed when Faber opens: those saved as owed (a close that cut a member's turn off marks it, `shutdown`), and
    /// those of members a crash cut off (their reply still streaming on disk), which no turn end will ever report. One sent
    /// before is not sent again (its id). With them, from the same pass, each leader none of whose members will wake it (none
    /// owes a report, is on Keep going or has a turn waiting) and its members' ids, for `resume_cut_leaders`.
    pub(super) fn restore_parent_reports(store: &Store) -> (HashMap<String, HashSet<String>>, HashMap<String, Vec<String>>) {
        let mut pending: HashMap<String, HashSet<String>> = HashMap::new();
        let mut teams: HashMap<String, (bool, Vec<String>)> = HashMap::new();
        for id in store.mentioning("\"startedBy\"") {
            let Some(c) = store.get(&id) else { continue };
            let Some(parent) = c.started_by.clone() else { continue };
            let cut_off = c.messages.iter().rev().find(|m| m.role == Role::Assistant).is_some_and(|m| m.status == Some(Status::Streaming));
            let owes = c.pending_parent_report || cut_off;
            let team = teams.entry(parent.clone()).or_default();
            team.0 |= owes || c.keep_going.is_some() || !c.queued.is_empty();
            team.1.push(id.clone());
            if owes {
                pending.entry(parent).or_default().insert(id);
            }
        }
        for parent in store.mentioning("\"waitingOn\"") {
            let Some(waiting) = store.get(&parent).and_then(|c| c.waiting_on) else { continue };
            for id in waiting.ids {
                let Some(mut child) = store.get(&id) else { continue };
                child.messages.iter_mut().for_each(settle);
                if child.keep_going.is_none() && !matches!(state_of(false, &child), State::Working | State::Waiting) {
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
        for child in pending {
            if self.convs.get(parent).is_none_or(|c| c.queued.len() >= MAX_QUEUED) {
                self.parent_reports.entry(parent.into()).or_default().insert(child);
                continue;
            }
            let owed = self.load(&child).is_some_and(|c| c.pending_parent_report);
            self.tell_parent(&child);
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
    }

    /// The agent called the conversation tool. Every call is answered at once, with why when nothing was done.
    pub(super) fn conversation_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        let (ok, text) = match sibling::request(&input) {
            Err(why) => (false, format!("Nothing was done: {why}.")),
            Ok(Request::Start { title, brief, ai, model, effort, keep_going, step }) => {
                match self.session_start(conv, NewSession { title: title.clone(), brief, ai, model, effort, keep_going, step }) {
                    Ok(id) => (true, sibling::started(&title, &id)),
                    Err(why) => (false, format!("No conversation was started: {why}.")),
                }
            }
            Ok(Request::Read { id }) => match self.read_sibling(conv, &id) {
                Ok(text) => (true, text),
                Err(why) => (false, format!("Nothing to read: {why}.")),
            },
            Ok(Request::List) => match self.list_siblings(conv) {
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
        // The answer goes back only to the process that asked.
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(&call, ok, text);
        }
    }

    fn start_sibling(
        &mut self,
        conv: &str,
        title: &str,
        brief: &str,
        ai: Ai,
        keep_going: bool,
        step: Option<String>,
        model: Option<String>,
        effort: Option<String>,
    ) -> Result<(String, bool), &'static str> {
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
            Ai::Named(vendor) if self.opts.clis.contains_key(&vendor) => {
                Choice { vendor: vendor.name().into(), model: None, effort: Some(crate::router::Effort::Medium) }
            }
            Ai::Named(_) => return Err("that AI is not connected on this computer; choose another, or leave ai out"),
        };
        if matches!(ai, Ai::Same) && choice.vendor == "auto" && (model.is_some() || effort.is_some()) {
            let vendor = self.live.get(conv).map(|l| l.vendor).ok_or("the current AI is not known yet")?;
            choice.vendor = vendor.name().into();
        }
        if choice.vendor == "auto" && (model.is_some() || effort.is_some()) {
            return Err("model and effort cannot be used with auto; choose an AI");
        }
        let selected_model = model.clone();
        if let Some(model) = model {
            let vendor = Vendor::ALL.into_iter().find(|v| v.name() == choice.vendor).ok_or("choose a named AI for a model")?;
            let valid = match vendor {
                Vendor::Claude => {
                    crate::route::PROFILES.iter().any(|p| p.vendor == vendor && p.model == crate::router::pinned(vendor, &model))
                }
                Vendor::Codex => self.codex_models.contains(&model),
            };
            if !valid {
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
        let gate = match (gate, selected_model) {
            (Some((vendor, _)), Some(model)) => Some((vendor, family_of(crate::router::pinned(vendor, &model)).map(str::to_string))),
            (gate, _) => gate,
        };
        self.sibling_allowed(&gate)?;
        let id = self.blank(Area::Project, Some(project.clone()));
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
            text: sibling::prompt(title, brief),
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
        let plan = c.shown_plan();
        self.save(lead);
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
            self.tell_parent(child);
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
        // Stopped working: the plan step waits for its work to be joined.
        self.edit_worker(conv, |w| w.joining = true);
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
            self.deliver_child_report(conv, parent, text.clone(), report_id.clone());
        }
    }

    fn deliver_child_report(&mut self, conv: &str, parent: String, text: String, report_id: String) {
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
        let wake = Wake::of("wake.asked");
        self.send_by_itself(id, sibling::ask_prompt(&from, conv, message), wake).map_err(|why| match why.as_str() {
            "error.queueFull" => "the conversation's inbox is full; it cannot take another message yet",
            _ => "it could not take the message now",
        })?;
        Ok(sibling::asked(&title, id, busy))
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
                    Vendor::Claude => crate::route::PROFILES.iter().filter(|p| p.vendor == vendor).map(|p| p.model.to_string()).collect(),
                    Vendor::Codex => self.codex_models.clone(),
                };
                if !valid.iter().any(|m| m == crate::router::pinned(vendor, model)) {
                    return Err(format!("unknown model; valid models are: {}", valid.join(", ")));
                }
            }
        }
        let (id, started) = self
            .start_sibling(by, &new.title, &new.brief, new.ai, new.keep_going, new.step, new.model, new.effort)
            .map_err(str::to_string)?;
        if started { Ok(id) } else { Err(sibling::failed(&new.title, &id)) }
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
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some());
        match state_of(busy, target) {
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
        let stopped = self.convs.get(id).is_some_and(|c| c.stopped);
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
    pub(super) fn session_archive(&mut self, by: &str, id: &str) -> Result<String, String> {
        self.own(by, id)?;
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
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some());
        match state_of(busy, c) {
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
        if note.chars().count() > 200 {
            return Err("the note is longer than 200 characters".into());
        }
        for id in &on {
            self.own(by, id)?;
        }
        // A conversation that reported before the wait is read by the asker already; waiting on it would never end.
        let mut reported = Vec::new();
        for id in &on {
            let child = &self.convs[id];
            let reply = child.messages.iter().rev().find(|m| m.role == Role::Assistant);
            let report_id = format!("report-{id}-{}", reply.map_or("empty", |m| m.id.as_str()));
            let busy = self.live.get(id).is_some_and(|l| l.turn.is_some());
            let ended = !matches!(state_of(busy, child), State::Working | State::Waiting);
            let delivered = self
                .convs
                .get(by)
                .is_some_and(|p| p.queued.iter().any(|q| q.id == report_id) || p.messages.iter().any(|m| m.id == report_id));
            if ended && delivered {
                reported.push(id.clone());
            }
        }
        let on: Vec<String> = on.into_iter().filter(|id| !reported.contains(id)).collect();
        if on.is_empty() && note.trim().is_empty() {
            return Ok("Those conversations already reported; read their reports and continue.".into());
        }
        let c = self.load(by).ok_or("the conversation is gone")?;
        let previous = c.waiting_on.replace(crate::model::WaitingOn { ids: on.clone(), note, since: now() });
        if self.try_save(by).is_err() {
            self.convs.get_mut(by).expect("waiting conversation exists").waiting_on = previous;
            return Err("the wait could not be saved".into());
        }
        self.list_changed();
        for id in on {
            self.tell_parent(&id);
        }
        Ok("End your turn now; Faber wakes you when one of these conversations reports.".into())
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
