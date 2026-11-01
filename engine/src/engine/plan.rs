//! The agent's plan, its now line and its closing box (`crate::progress`, task 017): each call is written down where the
//! screen reads it (the plan on the conversation, the other two on the reply under way) and answered at once.
use super::*;
use crate::progress;

impl Core {
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
    /// stood. It is read once the message and its reply are in the conversation (`note_before`, on every path that starts
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

    fn write_progress(&mut self, conv: &str, tool: &str, input: &serde_json::Value) -> Result<String, String> {
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
                let m = self.turn(conv).ok_or("No reply is under way.")?;
                let done = box_.done;
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
                let r = crate::results::tally(self.convs.get(conv).ok_or("The conversation is gone.")?);
                let (d, c) = (r.delivered, r.cost);
                Ok(format!(
                    "The person sees the box. End your turn now, without repeating the recap.\nResults: {} files, {}/{} checks passed, {} commits, {} pull requests, {} artifacts, {} answers; cost: {}${} API value, {} input and {} output tokens, {} ms work (current turn may be incomplete).",
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
                    c.work_ms
                ))
            }
            _ => Err("Faber has no such tool.".into()),
        }
    }
}
