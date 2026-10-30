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
    /// conversation stands (task 040). One not in memory has run nothing since Faber started, so it reads as finished, as
    /// the list of conversations reads it.
    pub(super) fn shown_plan(&self, conv: &str) -> Option<Plan> {
        let mut plan = self.convs.get(conv)?.shown_plan()?;
        for step in &mut plan.steps {
            step.member = step.conversation.as_deref().map(|id| match self.convs.get(id) {
                Some(c) => siblings::member_state(self.live.get(id).is_some_and(|l| l.turn.is_some()), c),
                None => MemberState::Done,
            });
        }
        Some(plan)
    }

    /// The plans of conversations whose steps name one of `changed`, sent again: a step's state follows its conversation.
    pub(super) fn plans_naming(&mut self, changed: &HashSet<String>) {
        let leaders: Vec<String> = self
            .convs
            .iter()
            .filter(|(_, c)| {
                c.plan.as_ref().is_some_and(|p| p.steps.iter().any(|s| s.conversation.as_deref().is_some_and(|id| changed.contains(id))))
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

    fn write_progress(&mut self, conv: &str, tool: &str, input: &serde_json::Value) -> Result<String, String> {
        match tool {
            progress::PLAN_TOOL => {
                let c = self.convs.get_mut(conv).ok_or("The conversation is gone.")?;
                let estimate = progress::estimate(c.estimate, input, c.spent_cents())?;
                let plan = progress::plan(c.plan.as_ref(), input, now())?;
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
                // The job is over only when the agent says so: its estimate, and economy mode with it, go; a later list starts
                // with none. The spend stays, and a new estimate counts from it.
                let ended = done && self.convs.get_mut(conv).and_then(|c| c.estimate.take()).is_some();
                self.save(conv);
                if ended {
                    let plan = self.shown_plan(conv);
                    (self.emit)(AppEvent::Plan { conversation_id: conv.to_string(), plan });
                }
                self.push(conv, Some(&id), true);
                Ok("The person sees the box. End your turn now, without repeating the recap.".into())
            }
            _ => Err("Faber has no such tool.".into()),
        }
    }
}
