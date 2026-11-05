//! The agent's plan, its now line and its closing box (`crate::progress`, task 017): each call is written down where the
//! screen reads it (the plan on the conversation, the other two on the reply under way) and answered at once.
use super::*;
use crate::progress;

impl Engine {
    /// Accepts a ready task through the ordinary send path, or dismisses its unchanged file.
    pub fn follow_up(&self, id: &str, file: &str, dismiss: bool) -> Result<(), String> {
        self.with(|c| c.follow_up(id, file, dismiss))
    }
}

impl Core {
    fn boundary_demand(&self, conv: &str, tool: &str, input: &serde_json::Value) -> (crate::model::PlanSize, Option<&'static str>) {
        use crate::model::PlanSize;
        let Some(c) = self.convs.get(conv).filter(|c| c.area == Area::Project) else { return (PlanSize::Small, None) };
        let Ok(cwd) = self.cwd_of(c) else {
            return (PlanSize::Risky, Some("unknown"));
        };
        let Some(cwd) = cwd.to_str() else {
            return (PlanSize::Risky, Some("unknown"));
        };
        let roots = Store::data_roots().iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>();
        let credentials =
            crate::permissions::relocated_credentials(&|key| std::env::var(key).ok()).into_iter().map(|(p, _)| p).collect::<Vec<_>>();
        let classify = |tool: &str, input: &serde_json::Value| {
            crate::permissions::plan_boundary(cwd, cwd, &roots, &credentials, c.copy.is_some(), tool, input, false)
        };
        let demand = classify(tool, input);
        if matches!(demand, (PlanSize::Risky, Some("credentials" | "data" | "unknown"))) {
            return demand;
        }
        let probed = crate::permissions::plan_probe(cwd, tool, input, |prefix| {
            let metadata = prefix.symlink_metadata()?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Ok(PlanSize::Risky);
                }
            }
            Ok(if metadata.file_type().is_symlink() { PlanSize::Risky } else { PlanSize::Small })
        });
        if probed.0 >= demand.0 { probed } else { demand }
    }

    pub(super) fn plan_allows(&self, conv: &str, tool: &str, input: &serde_json::Value) -> bool {
        let demand = self.boundary_demand(conv, tool, input).0;
        demand == crate::model::PlanSize::Small || self.convs.get(conv).and_then(|c| c.plan.as_ref()).is_some_and(|p| p.covers(demand))
    }

    pub(super) fn hold_plan_boundary(&mut self, conv: &str, request: &str, tool: &str, input: &serde_json::Value) -> bool {
        use crate::model::PlanSize;
        if ["AskUserQuestion", "ExitPlanMode"].contains(&tool) {
            return false;
        }
        if self.turn(conv).is_none() {
            if let Some(live) = self.live.get(conv) {
                drop(live.session.answer(request, false));
            }
            return true;
        }
        let (size, reason) = self.boundary_demand(conv, tool, input);
        if size == PlanSize::Small {
            return false;
        }
        let Some(c) = self.convs.get_mut(conv) else { return false };
        let plan = c.plan.get_or_insert_with(|| Plan {
            steps: vec![],
            size: PlanSize::Small,
            raised_because: None,
            contract: None,
            at: now(),
            estimate: None,
        });
        if size > plan.size {
            plan.size = size;
            plan.raised_because = reason.map(str::to_string);
            self.save(conv);
            (self.emit)(AppEvent::Plan { conversation_id: conv.into(), plan: self.shown_plan(conv) });
        }
        if self.convs.get(conv).and_then(|c| c.plan.as_ref()).is_some_and(|p| p.covers(size)) {
            return false;
        }
        let card = serde_json::json!({
            "planBoundary": {"tool": tool, "input": input},
            "questions": [{"question": "plan.contractQuestion", "header": "plan.label", "multiSelect": false,
                "options": [{"label": "plan.stateContract", "description": "plan.contractDefault"}]}]
        });
        let turn = if let Some(m) = self.turn(conv) {
            let list = m.permissions.get_or_insert_default();
            list.retain(|p| p.request_id != request);
            list.push(Permission { request_id: request.into(), tool: "AskUserQuestion".into(), input: card, epoch: None });
            Some(m.id.clone())
        } else {
            None
        };
        self.save(conv);
        if let Some(turn) = turn {
            self.weigh(conv);
            self.watch_question(conv, &turn, request);
            self.push(conv, Some(&turn), true);
        }
        true
    }

    fn resume_plan_permissions(&mut self, conv: &str) {
        if self.convs.get(conv).is_some_and(|c| c.stopped) {
            return;
        }
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        let asks = self.turn(conv).and_then(|m| m.permissions.take()).unwrap_or_default();
        for p in asks {
            let boundary = &p.input["planBoundary"];
            let (tool, input) = if let Some(tool) = boundary["tool"].as_str() {
                (tool.to_string(), boundary["input"].clone())
            } else if ["AskUserQuestion", "ExitPlanMode"].contains(&p.tool.as_str()) {
                if let Some(m) = self.turn(conv) {
                    m.permissions.get_or_insert_default().push(p);
                }
                continue;
            } else {
                (p.tool, p.input)
            };
            self.handle(conv, sid, EngineEvent::Permission { request_id: p.request_id, tool, input });
        }
    }

    fn follow_up_project(&self, id: &str) -> Option<crate::model::Project> {
        let c = self.convs.get(id)?;
        if c.area != Area::Project {
            return None;
        }
        self.store.projects().into_iter().find(|p| Some(&p.id) == c.project_id.as_ref())
    }

    pub(super) fn ready_follow_ups(&self, id: &str, tasks: &[crate::model::FollowUpTask]) -> Vec<crate::model::FollowUpTask> {
        if tasks.is_empty() {
            return vec![];
        }
        // Saved rows also project offers after restart, before their conversation has loaded.
        let project_id = self.convs.get(id).and_then(|c| c.project_id.clone()).or_else(|| self.store.get(id).and_then(|c| c.project_id));
        let Some(project) = self.store.projects().into_iter().find(|p| Some(&p.id) == project_id.as_ref()) else { return vec![] };
        let root = Path::new(&project.path);
        // shortcut: adopted task folders remain read-only; add an approved writer before offering new tasks there.
        if !matches!(self.store.read_adopted_tasks(&project.id, root), Ok(None)) {
            return vec![];
        }
        let active = self.live.get(id).is_some_and(|l| l.turn.is_some())
            || self.convs.get(id).is_some_and(|c| c.compacting || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming)));
        let tasks: Vec<_> = tasks.iter().map(|t| crate::model::FollowUpTask { sent: t.sent && active, ..t.clone() }).collect();
        crate::project::memory::ready_follow_ups(root, &tasks).unwrap_or_default()
    }

    fn follow_up(&mut self, id: &str, file: &str, dismiss: bool) -> Result<(), String> {
        let c = self.load(id).ok_or("error.conversationGone")?;
        let task = c.follow_ups.iter().find(|t| t.file == file).cloned().ok_or("error.memory.changed")?;
        let project = self.follow_up_project(id).ok_or("error.projectGone")?;
        if dismiss {
            crate::project::memory::remove_follow_up(Path::new(&project.path), &task)?;
            self.convs.get_mut(id).unwrap().follow_ups.retain(|t| t.file != file);
            self.try_save(id)?;
        } else {
            let c = &self.convs[id];
            if self.live.get(id).is_some_and(|l| l.turn.is_some())
                || c.compacting
                || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
                || !self.ready_follow_ups(id, &c.follow_ups).iter().any(|t| t.file == file)
            {
                return Err("error.memory.changed".into());
            }
            let request = SendRequest {
                conversation_id: Some(id.into()),
                area: c.area,
                project_id: c.project_id.clone(),
                text: format!("{}\n\n{}", task.title, Path::new(&project.path).join(&task.file).display()),
                images: vec![],
                files: vec![],
                choice: c.choice.clone().ok_or("error.noSubscription")?,
            };
            // The task and its message commit together in send's admission save, before dispatch starts.
            if let Err(error) = self.send_task(request, None, Some(file)) {
                *self.convs.get_mut(id).unwrap().follow_ups.iter_mut().find(|t| t.file == file).unwrap() = task;
                self.try_save(id)?;
                return Err(error);
            }
        }
        (self.emit)(AppEvent::List);
        Ok(())
    }

    fn close_follow_ups(&mut self, conv: &str, input: &serde_json::Value, done: bool) -> Result<&'static str, String> {
        let tasks: Vec<progress::FollowUp> = match input.get("follow_ups") {
            Some(value) => serde_json::from_value(value.clone()).map_err(|_| "Send follow_ups as task objects with title and done.")?,
            None => vec![],
        };
        if tasks.len() > 32 {
            return Err("Send at most 32 follow-ups at once.".into());
        }
        for task in &tasks {
            task.task_file()?;
        }
        let Some(project) = self.follow_up_project(conv) else {
            return if tasks.is_empty() { Ok("") } else { Err("Follow-ups need a project folder.".into()) };
        };
        let root = Path::new(&project.path);
        if !matches!(self.store.read_adopted_tasks(&project.id, root), Ok(None)) {
            return if tasks.is_empty() {
                Ok("")
            } else {
                Err("This project's task folder is read-only in Faber. No new task was written.".into())
            };
        }
        let mut notice = "";
        if done {
            let question = self.convs[conv].messages.iter().rev().find(|m| m.role == Role::User && m.wake.is_none()).map(|m| &m.id);
            let sent: Vec<_> = self.convs[conv].follow_ups.iter().filter(|t| t.sent && t.message.as_ref() == question).cloned().collect();
            for task in sent {
                if crate::project::memory::remove_follow_up(root, &task).is_ok() {
                    self.convs.get_mut(conv).unwrap().follow_ups.retain(|t| t.file != task.file);
                } else {
                    self.convs.get_mut(conv).unwrap().follow_ups.iter_mut().find(|t| t.file == task.file).unwrap().sent = false;
                    notice = "The follow-up file was kept because it changed or could not be removed. The work can still conclude.";
                }
            }
        }
        if self.convs[conv].follow_ups.len() + tasks.len() > 256 {
            return Err("Dismiss existing follow-ups before adding more.".into());
        }
        for task in tasks {
            let revision = crate::project::memory::revision(task.task_file()?.as_bytes());
            if self.convs[conv].follow_ups.iter().any(|t| t.revision == revision) {
                continue;
            }
            let saved = crate::project::memory::write_follow_up(root, &task)?;
            self.convs.get_mut(conv).unwrap().follow_ups.push(saved.clone());
            if let Err(error) = self.try_save(conv) {
                crate::project::memory::remove_follow_up(root, &saved)?;
                self.convs.get_mut(conv).unwrap().follow_ups.retain(|t| t.file != saved.file);
                return Err(error);
            }
        }
        Ok(notice)
    }
    /// Adds what something cost, in US$, to the job's running total (rule 7): the conversation's, and its parent's when an
    /// agent started it. It is called where each cost is written (a turn, a helper, a quick side question), so the spend is
    /// read in one step and never counted again from the conversations. A cost not known adds nothing, so the warning can
    /// come late; an estimate sent mid-turn counts that whole turn. The parent is saved here; the caller saves the conversation, as it does for its cost.
    pub(super) fn add_spend(&mut self, conv: &str, usd: Option<f64>) {
        let Some(usd) = usd.filter(|usd| usd.is_finite() && *usd > 0.0) else { return };
        let Some(c) = self.convs.get_mut(conv) else { return };
        c.spent_usd += usd;
        let (parent, fix) = (c.started_by.clone(), c.goal_fix.clone());
        if let Some(parent) = parent.clone().filter(|p| self.load(p).map(|p| p.spent_usd += usd).is_some()) {
            self.save(&parent);
        }
        // A fix Faber started for a standing goal counts against the project's day (`goals.rs`), helpers and children included.
        if let Some(project) = fix {
            self.goal_spent(&project, usd);
        }
    }

    /// Whether the conversation's job has passed its estimate by half (rule 7): support work runs cheaper, extras wait. A
    /// conversation an agent started follows the job of the agent that started it.
    pub(super) fn economy(&self, conv: &str) -> bool {
        let on = |id: &str| self.convs.get(id).and_then(|c| c.estimate).is_some_and(|e| e.economy);
        on(conv) || self.convs.get(conv).and_then(|c| c.started_by.as_deref()).is_some_and(on)
    }

    /// At the end of a turn: when the job has spent more than half again its estimate, switch to economy mode once and tell
    /// the screen. It only warns and changes how support work runs; it never stops anything.
    pub(super) fn check_estimate(&mut self, conv: &str) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        let spent = c.spent_cents();
        let Some(e) = c.estimate.as_mut().filter(|e| !e.economy && progress::passed(e, spent)) else { return };
        e.economy = true;
        self.save(conv);
        let plan = self.shown_plan(conv);
        (self.emit)(AppEvent::Plan { conversation_id: conv.to_string(), plan });
        self.refresh_running();
    }

    /// The plan as the screen draws it: the estimate in it, and for each step a conversation this one started does, where that
    /// conversation stands (task 040), including the saved state of a conversation not in memory.
    pub(super) fn shown_plan(&self, conv: &str) -> Option<Plan> {
        let mut plan = self.convs.get(conv)?.shown_plan()?;
        for step in &mut plan.steps {
            if let Some(worker) = &step.worker {
                step.conversation = Some(worker.conversation.clone());
            }
            step.member = step.conversation.as_deref().map(|id| self.plan_member_state(id));
            if step.state == StepState::Done && matches!(step.member, Some(MemberState::Running | MemberState::Waiting)) {
                step.state = StepState::Active;
            }
        }
        Some(plan)
    }

    fn plan_member_state(&self, id: &str) -> MemberState {
        match self.convs.get(id) {
            Some(c) => siblings::member_state(self.live.get(id).is_some_and(|l| l.turn.is_some()), c),
            None => self.store.get(id).map_or(MemberState::Stopped, |mut c| {
                for message in &mut c.messages {
                    message.closed |= message.status == Some(Status::Streaming);
                    settle(message);
                }
                siblings::member_state(false, &c)
            }),
        }
    }

    /// Work resumed after a step was marked done: the agent must confirm it again after this turn ends.
    pub(super) fn reopen_busy_plan_steps(&mut self, lead: &str, cached_only: bool) {
        let Some(plan) = self.convs.get(lead).and_then(|c| c.plan.as_ref()) else { return };
        let busy: Vec<_> = plan
            .steps
            .iter()
            .enumerate()
            .filter_map(|(i, step)| {
                let id = step.worker.as_ref().map(|w| w.conversation.as_str()).or(step.conversation.as_deref())?;
                (step.state == StepState::Done
                    && (!cached_only || self.convs.contains_key(id))
                    && matches!(self.plan_member_state(id), MemberState::Running | MemberState::Waiting))
                .then_some(i)
            })
            .collect();
        if busy.is_empty() {
            return;
        }
        if let Some(plan) = self.convs.get_mut(lead).and_then(|c| c.plan.as_mut()) {
            for i in busy {
                plan.steps[i].state = StepState::Active;
            }
        }
        self.save(lead);
    }

    /// The plans of conversations whose steps name one of `changed`, sent again: a step's state follows its conversation.
    pub(super) fn plans_naming(&mut self, changed: &HashSet<String>) {
        let leaders: Vec<String> = self
            .convs
            .iter()
            .filter(|(_, c)| {
                c.plan.as_ref().is_some_and(|p| {
                    p.steps.iter().any(|s| {
                        s.worker
                            .as_ref()
                            .map(|w| w.conversation.as_str())
                            .or(s.conversation.as_deref())
                            .is_some_and(|id| changed.contains(id))
                    })
                })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in leaders {
            let plan = self.shown_plan(&id);
            (self.emit)(AppEvent::Plan { conversation_id: id, plan });
        }
    }

    pub(super) fn progress_call(&mut self, conv: &str, call: String, tool: &'static str, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        let (ok, text) = match self.write_progress(conv, tool, &input) {
            Ok(text) => (true, text),
            Err(why) => (false, why),
        };
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(&call, ok, text);
        }
    }

    /// Method 2, slice 6: a done closing box is one line in the project's local count (`method_log`), with the seconds since
    /// the person's last message. shortcut: that is the time of one attempt, not of the request to its accepted close; keep the
    /// request's first message if the whole task's time is ever needed.
    fn count_close(&self, conv: &str) {
        let Some((project, c)) = self.method_project(conv) else { return };
        let Some(last) = c.messages.iter().rev().find(|m| m.role == Role::User).map(|m| m.at) else { return };
        let at = now();
        let (dir, secs) = (self.opts.data_dir.clone(), at.saturating_sub(last) / 1000);
        self.opts.runtime.spawn_blocking(move || crate::method_log::note(&dir, &project, at, "closed", secs));
    }

    /// Method 2, slice 6: a message from the person right after a done closing box says the result was not accepted as it
    /// stood. It is read once the message and its reply are in the conversation (`count_correction`, on every path that starts
    /// or steers a turn), so a send that was refused counts nothing; Faber's own messages (`wake`) are not the person's, and
    /// a turn started again shows the same message, whose time the log takes once. shortcut: any next message counts, with
    /// no reading of its words (a thanks counts too); classify it if the number turns out too noisy.
    pub(super) fn count_correction(&self, conv: &str) {
        let Some((project, c)) = self.method_project(conv) else { return };
        let [.., closing, question, reply] = c.messages.as_slice() else { return };
        let done = closing.role == Role::Assistant && closing.wrap_up.as_ref().is_some_and(|w| w.done);
        if !done || question.role != Role::User || question.wake.is_some() || reply.role != Role::Assistant {
            return;
        }
        let (dir, at) = (self.opts.data_dir.clone(), question.at);
        self.opts.runtime.spawn_blocking(move || crate::method_log::note(&dir, &project, at, "corrected", 0));
    }

    /// A project's own conversation and the project, the only place the count is kept.
    fn method_project(&self, conv: &str) -> Option<(String, &Conversation)> {
        let c = self.convs.get(conv).filter(|c| c.area == Area::Project)?;
        Some((c.project_id.clone()?, c))
    }

    pub(super) fn write_progress(&mut self, conv: &str, tool: &str, input: &serde_json::Value) -> Result<String, String> {
        match tool {
            progress::PLAN_TOOL => {
                // A conversation of a step that was deleted frees its step before the list is compared with it.
                self.free_dead_workers(conv);
                let c = self.convs.get(conv).ok_or("The conversation is gone.")?;
                let leads = siblings::offers(c.area) && c.started_by.is_none();
                let estimate = progress::estimate(c.estimate, input, c.spent_cents())?;
                let mut plan = progress::plan(c.plan.as_ref(), input, now())?;
                if let Some(had) = &c.plan {
                    progress::keep_workers(had, &mut plan)?;
                }
                if let Some(plan) = &plan {
                    for step in plan.steps.iter().filter(|s| s.state == StepState::Done) {
                        let id = step.worker.as_ref().map(|w| w.conversation.as_str()).or(step.conversation.as_deref());
                        if id.is_some_and(|id| matches!(self.plan_member_state(id), MemberState::Running | MemberState::Waiting)) {
                            return Err(format!("{}: {} still has a conversation working or waiting", progress::RUNNING, step.id));
                        }
                    }
                    progress::check_linked(plan)?;
                }
                // A step may only name a conversation this one started, so the plan never opens someone else's work.
                let started = |id: &str| match self.convs.get(id) {
                    Some(m) => m.started_by.as_deref() == Some(conv),
                    None => self.store.get(id).is_some_and(|m| m.started_by.as_deref() == Some(conv)),
                };
                if let Some(id) = plan.iter().flat_map(|p| &p.steps).filter_map(|s| s.conversation.as_deref()).find(|id| !started(id)) {
                    return Err(format!(
                        "No step was changed: {id} is not a conversation you started. Give a step only the id that start answered."
                    ));
                }
                let c = self.convs.get_mut(conv).ok_or("The conversation is gone.")?;
                let mut shown = plan.as_ref().map(|p| p.count()).map_or("The plan is cleared.".to_string(), |n| {
                    format!("The person sees the plan: {} of {} steps done.", n.done, n.total)
                });
                if let Some(p) = plan.as_ref().filter(|_| leads) {
                    let wave: Vec<_> = progress::wave(p).iter().map(|s| s.id.as_str()).collect();
                    if !wave.is_empty() {
                        shown = format!("{shown} Steps that can start together now: {}.", wave.join(", "));
                    }
                }
                if estimate.is_some_and(|e| e.economy) {
                    shown = format!("{shown} {}", progress::ECONOMY_LINE);
                }
                (c.plan, c.estimate) = (plan, estimate);
                self.save(conv);
                self.resume_plan_permissions(conv);
                let plan = self.shown_plan(conv);
                (self.emit)(AppEvent::Plan { conversation_id: conv.to_string(), plan });
                self.refresh_running();
                Ok(shown)
            }
            progress::STATUS_TOOL => {
                let now_line = progress::status(input)?;
                let m = self.turn(conv).ok_or("No reply is under way.")?;
                m.now = Some(now_line);
                let id = m.id.clone();
                self.push(conv, Some(&id), true);
                self.refresh_running();
                Ok("The person sees it.".into())
            }
            progress::WRAP_UP_TOOL => {
                let box_ = progress::wrap_up(input)?;
                if self.turn(conv).is_none() {
                    return Err("No reply is under way.".into());
                }
                let follow_ups_notice = self.close_follow_ups(conv, input, box_.done)?;
                let m = self.turn(conv).ok_or("No reply is under way.")?;
                let done = box_.done;
                let concluded = box_.outcome.is_some();
                m.wrap_up = Some(box_);
                let id = m.id.clone();
                if done {
                    self.count_close(conv);
                }
                // The job is over only when the agent says so: its estimate, and economy mode with it, go; a later list starts
                // with none. The spend stays, and a new estimate counts from it.
                let ended = done && self.convs.get_mut(conv).and_then(|c| c.estimate.take()).is_some();
                self.save(conv);
                if ended {
                    let plan = self.shown_plan(conv);
                    (self.emit)(AppEvent::Plan { conversation_id: conv.to_string(), plan });
                }
                self.push(conv, Some(&id), true);
                let lead = if concluded {
                    "Concluded. Now write your closing message to the person in your own words: what you did, the result, what is not                      done and what you checked. Then end your turn."
                } else {
                    "The person sees the box. End your turn now, without repeating the recap."
                };
                let r = crate::results::tally(self.convs.get(conv).ok_or("The conversation is gone.")?);
                let (d, c) = (r.delivered, r.cost);
                Ok(format!(
                    "{lead}\n{follow_ups_notice}\nResults: {} files, {}/{} checks passed, {} commits, {} pull requests, {} artifacts, {} answers; cost: {}${} API value, {} input and {} output tokens, {} ms work (current turn may be incomplete).",
                    d.files,
                    d.checks_passed,
                    d.checks_run,
                    d.commits,
                    d.prs,
                    d.artifacts,
                    d.answers,
                    if c.unpriced > 0 { "at least " } else { "" },
                    c.usd,
                    c.input_tokens,
                    c.output_tokens,
                    c.work_ms,
                    lead = lead
                ))
            }
            _ => Err("Faber has no such tool.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn size_boundary_engine_call_including_existing_parent_probes_is_measured() {
        let dir = crate::workspace::TestRoot::new();
        let (app, _) = crate::engine::answer::tests::engine(&dir);
        let id = crate::engine::answer::tests::seed(&app, crate::events::Session::from_fn(|_| {}), &[("reply", &[])]);
        let folder = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
        app.with(|core| {
            let conv = core.convs.get_mut(&id).unwrap();
            conv.area = Area::Project;
            conv.project_id = Some(project.id);
            let input = serde_json::json!({"file_path":"src/new.txt"});
            let started = std::time::Instant::now();
            for _ in 0..1_000 {
                assert_eq!(core.boundary_demand(&id, "Read", &input).0, crate::model::PlanSize::Small);
            }
            eprintln!(
                "size_boundary_engine: {:.0} ns/call including existing-parent probes over 1000 approvals (debug)",
                started.elapsed().as_nanos() as f64 / 1_000.0
            );
        });
        app.shutdown();
    }

    #[tokio::test]
    async fn follow_up_refuses_compaction_and_streaming_admission_without_queueing_duplicates() {
        let dir = crate::workspace::TestRoot::new();
        let (app, _) = crate::engine::answer::tests::engine(&dir);
        let id = crate::engine::answer::tests::seed(&app, crate::events::Session::from_fn(|_| {}), &[("finished", &[])]);
        let folder = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
        let task: crate::progress::FollowUp =
            serde_json::from_value(serde_json::json!({"title":"Fix export", "done":"Export test passes"})).unwrap();
        let task = crate::project::memory::write_follow_up(Path::new(&project.path), &task).unwrap();
        app.with(|c| {
            c.live.remove(&id);
            let conv = c.convs.get_mut(&id).unwrap();
            conv.area = Area::Project;
            conv.project_id = Some(project.id);
            conv.follow_ups.push(task.clone());
            conv.messages.last_mut().unwrap().status = Some(Status::Done);
            conv.compacting = true;
        });
        assert_eq!(app.follow_up(&id, &task.file, false), Err("error.memory.changed".into()));
        app.with(|c| {
            let conv = c.convs.get_mut(&id).unwrap();
            assert!(conv.queued.is_empty() && !conv.follow_ups[0].sent);
            conv.compacting = false;
            conv.messages.last_mut().unwrap().status = Some(Status::Streaming);
        });
        assert_eq!(app.follow_up(&id, &task.file, false), Err("error.memory.changed".into()));
        assert!(app.open(&id).unwrap().queued.is_empty());
        app.shutdown();
    }
}
