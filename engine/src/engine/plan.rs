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
        let plan = c.shown_plan();
        self.save(conv);
        (self.emit)(AppEvent::Plan { conversation_id: conv.to_string(), plan });
        self.refresh_running();
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

    /// What a project's close asks besides the box (own work method, "Staying small and current"): to fold each note of
    /// `.faber/` over its soft target. Asked once for each size a file has, so a file folding cannot bring under its target
    /// is asked again only after it changes (while Faber runs: a restart may ask once more). No timer and no watcher: only
    /// a close looks, through the folder's no-follow handle.
    fn fold_ask(&mut self, conv: &str) -> Option<String> {
        let c = self.convs.get(conv).filter(|c| c.area == Area::Project)?;
        let files = crate::project::memory::ProjectFiles::open(&self.folder_of(c).ok()?)?;
        let over: Vec<String> = crate::project::memory::TARGETS
            .iter()
            .filter_map(|(path, cap)| {
                let len = files.size(path)?;
                (len > *cap as u64 && self.folds_asked.insert(format!("{conv}\n{path}\n{len}")))
                    .then(|| format!("`{path}` is {:.1} KB, over its {} KB target", len as f64 / 1024.0, cap / 1024))
            })
            .collect();
        if over.is_empty() {
            return None;
        }
        Some(format!(
            "Before you end your turn, fold these notes: {}. Merge duplicates and drop what a later line overrides; in \
             `.faber/decisions.md`, move a decision a later one overrides to the end of `.faber/decisions-old.md` (searched, \
             never loaded) instead of deleting it. Never drop a live item: when the target cannot be reached without losing \
             one, leave the file over it. Then end your turn without calling wrap_up again.",
            over.join("; ")
        ))
    }

    fn write_progress(&mut self, conv: &str, tool: &str, input: &serde_json::Value) -> Result<String, String> {
        match tool {
            progress::PLAN_TOOL => {
                let c = self.convs.get_mut(conv).ok_or("The conversation is gone.")?;
                let estimate = progress::estimate(c.estimate, input, c.spent_cents())?;
                let plan = progress::plan(c.plan.as_ref(), input, now())?;
                let mut shown = plan.as_ref().map(|p| p.count()).map_or("The plan is cleared.".to_string(), |n| {
                    format!("The person sees the plan: {} of {} steps done.", n.done, n.total)
                });
                if estimate.is_some_and(|e| e.economy) {
                    shown = format!("{shown} {}", progress::ECONOMY_LINE);
                }
                (c.plan, c.estimate) = (plan, estimate);
                let plan = c.shown_plan();
                self.save(conv);
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
                    let plan = self.convs.get(conv).and_then(Conversation::shown_plan);
                    (self.emit)(AppEvent::Plan { conversation_id: conv.to_string(), plan });
                }
                self.push(conv, Some(&id), true);
                Ok(match self.fold_ask(conv) {
                    Some(fold) => format!("The person sees the box. {fold}"),
                    None => "The person sees the box. End your turn now, without repeating the recap.".into(),
                })
            }
            _ => Err("Faber has no such tool.".into()),
        }
    }
}
