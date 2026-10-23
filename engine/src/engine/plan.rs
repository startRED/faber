//! The agent's plan, its now line and its closing box (`crate::progress`, task 017): each call is written down where the
//! screen reads it (the plan on the conversation, the other two on the reply under way) and answered at once.
use super::*;
use crate::progress;

impl Engine {
    /// Keeps the saved proposal, never words supplied by the screen or a repository author mark.
    pub fn keep_project_note(&self, conv: &str, message: &str, kind: &str) -> Result<(), String> {
        self.with(|c| c.keep_project_note(conv, message, kind))
    }

    /// A follow-up is a normal new project conversation with the closing box's saved request.
    pub fn start_follow_up(&self, conv: &str, message: &str, index: usize, choice: Choice) -> Result<String, String> {
        self.with(|c| {
            let before = c.load(conv).filter(|c| c.area == Area::Project).ok_or("error.memory.refused")?;
            let project_id = before.project_id.clone().ok_or("error.projectGone")?;
            let text = before
                .messages
                .iter()
                .find(|m| m.id == message && m.role == Role::Assistant)
                .and_then(|m| m.wrap_up.as_ref())
                .and_then(|w| w.next.get(index).filter(|_| index < 3))
                .cloned()
                .ok_or("error.memory.refused")?;
            let id = c.blank(Area::Project, Some(project_id.clone()));
            c.convs.get_mut(&id).ok_or("error.conversationGone")?.next = Some(text.clone());
            let sent = c.send(
                SendRequest {
                    conversation_id: Some(id.clone()),
                    area: Area::Project,
                    project_id: Some(project_id),
                    text,
                    images: vec![],
                    files: vec![],
                    choice,
                },
                None,
            );
            if sent.is_err() && c.convs.get(&id).is_some_and(|c| c.messages.is_empty()) {
                c.convs.remove(&id);
            }
            sent
        })
    }
}

impl Core {
    fn keep_project_note(&mut self, conv: &str, message: &str, kind: &str) -> Result<(), String> {
        let c = self.load(conv).filter(|c| c.area == Area::Project).ok_or("error.memory.refused")?;
        let project_id = c.project_id.clone().ok_or("error.projectGone")?;
        let w = c
            .messages
            .iter()
            .find(|m| m.id == message && m.role == Role::Assistant)
            .and_then(|m| m.wrap_up.as_ref())
            .ok_or("error.messageGone")?;
        let (line, kept, name) = match kind {
            "taste" => (w.taste.clone(), w.taste_kept, "taste.md"),
            "lesson" => (w.lesson.clone(), w.lesson_kept, "lessons.md"),
            _ => return Err("error.memory.refused".into()),
        };
        let line = line.ok_or("error.memory.refused")?;
        if kept {
            return Ok(());
        }
        let project = self.project_of(Some(&project_id)).ok_or("error.projectGone")?;
        crate::project::memory::append_notes(Path::new(&project.path), name, std::slice::from_ref(&line)).map_err(str::to_string)?;
        let live = self.store.projects_checked().map_err(|e| e.key().to_string())?.into_iter().map(|p| p.id).collect::<Vec<_>>();
        self.store
            .keep_decision(&project_id, crate::project::memory::digest(&format!("- {line}")), &live)
            .map_err(|e| e.key().to_string())?;
        let w = self
            .convs
            .get_mut(conv)
            .and_then(|c| c.messages.iter_mut().find(|m| m.id == message))
            .and_then(|m| m.wrap_up.as_mut())
            .ok_or("error.messageGone")?;
        if kind == "taste" {
            w.taste_kept = true;
        } else {
            w.lesson_kept = true;
        }
        self.save(conv);
        self.push(conv, Some(message), true);
        Ok(())
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
        let folder = self.folder_of(c).ok()?;
        let project = self.project_of(c.project_id.as_deref())?;
        let files = crate::project::memory::ProjectFiles::open(&folder)?;
        let stored = crate::project::memory::ProjectFiles::open(Path::new(&project.path))?;
        let mut over: Vec<String> = crate::project::memory::TARGETS
            .iter()
            .filter_map(|(path, cap)| {
                let len = if *path == ".faber/decisions.md" { files.size(path) } else { stored.size(path) }?;
                (len > *cap as u64 && self.folds_asked.insert(format!("{conv}\n{path}\n{len}")))
                    .then(|| format!("`{path}` is {:.1} KB, over its {} KB target", len as f64 / 1024.0, cap / 1024))
            })
            .collect();
        if crate::project::memory::next_full(Path::new(&project.path)) {
            over.push("`.faber/next.md` is full (12 items); fold it before adding follow-ups".into());
        }
        if over.is_empty() {
            return None;
        }
        Some(format!(
            "Before you end your turn, fold these notes: {}. Merge duplicates and drop what a later line overrides; in \
             `.faber/decisions.md`, move a decision a later one overrides to the end of `.faber/decisions-old.md` (searched, \
             never loaded) instead of deleting it. Never drop a live item: when the target cannot be reached without losing \
             one, leave the file over it. For taste and lessons, propose edits to the person instead of writing those files. \
             Then end your turn without calling wrap_up again.",
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
                let mut box_ = progress::wrap_up(input)?;
                if self.turn(conv).is_none() {
                    return Err("No reply is under way.".into());
                }
                let c = self.convs.get(conv).ok_or("error.conversationGone")?;
                let mut unsaved = false;
                if c.area == Area::Project {
                    let path = self.project_of(c.project_id.as_deref()).map(|p| p.path.clone());
                    if box_.done
                        && let Some(line) = &c.next
                    {
                        unsaved |= path.as_ref().is_none_or(|p| crate::project::memory::finish_next(Path::new(p), line).is_err());
                        box_.next.retain(|next| next != line);
                    }
                    if !box_.next.is_empty() {
                        unsaved |= path
                            .as_ref()
                            .is_none_or(|p| crate::project::memory::append_notes(Path::new(p), "next.md", &box_.next).is_err());
                    }
                } else {
                    box_.taste = None;
                    box_.lesson = None;
                    box_.next.clear();
                }
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
                // The box is shown either way; a list Faber could not write is the agent's to mention, not a failed close.
                let unsaved = if unsaved { " The follow-up list in .faber/next.md could not be updated: say so in one line." } else { "" };
                Ok(match self.fold_ask(conv) {
                    Some(fold) => format!("The person sees the box.{unsaved} {fold}"),
                    None => format!("The person sees the box.{unsaved} End your turn now, without repeating the recap."),
                })
            }
            _ => Err("Faber has no such tool.".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::answer::tests::{engine, seed};
    use super::*;

    #[tokio::test]
    async fn a_follow_up_that_cannot_start_leaves_no_empty_conversation() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let id = seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
        let before = engine.with(|c| {
            let conv = c.convs.get_mut(&id).unwrap();
            conv.area = Area::Project;
            conv.project_id = Some("project".into());
            conv.messages.last_mut().unwrap().wrap_up = Some(WrapUp { next: vec!["Check saved state.".into()], ..Default::default() });
            c.convs.len()
        });
        let choice = Choice { vendor: "auto".into(), model: None, effort: None };
        assert!(engine.start_follow_up(&id, "reply", 0, choice).is_err(), "no AI is connected");
        assert_eq!(engine.with(|c| c.convs.len()), before, "retrying cannot accumulate empty conversations");
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_non_project_close_drops_all_proposals_even_if_the_tool_reaches_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let id = seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
        engine.with(|c| {
            c.write_progress(
                &id,
                progress::WRAP_UP_TOOL,
                &serde_json::json!({
                    "recap": "Explained.", "taste": "Always use plain words.",
                    "lesson": "Check reloads.", "next": ["Cover saved state."]
                }),
            )
            .unwrap();
            let wrap = c.convs[&id].messages.last().unwrap().wrap_up.as_ref().unwrap();
            assert!(wrap.taste.is_none() && wrap.lesson.is_none() && wrap.next.is_empty());
        });
        assert!(!dir.path().join("decisions-kept.json").exists());
        engine.shutdown();
    }
}
