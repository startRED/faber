//! A project conversation's own copy (R201, `crate::project::copy`): made before its first turn starts and used as
//! the agent's folder from then on, removed with the conversation, and removed once at start when its work is merged.
//! Git runs outside the engine's lock, since a checkout can take seconds; the lock only reads what to do and records it.
use super::*;
use crate::project::copy::{self, WorkCopy};
use crate::project::git::Executor;

/// Opens a git runner. The shell gives the real one; without one (tests, a PC without git) every project conversation
/// works in the project folder, as before copies.
pub type GitRunner = Arc<dyn Fn() -> Option<Box<dyn Executor + Send>> + Send + Sync>;

/// A copy made for conversation `.0`, not yet recorded on it.
type Made = Option<(String, WorkCopy)>;

/// A first message waiting for its copy: its reply, and what starts the answer once the copy is attached.
pub(super) struct Held {
    pub(super) reply: String,
    pub(super) tail: Option<Tail>,
}

/// The turn `Core::dispatch` was asked to start while the copy was pending.
pub(super) struct Tail {
    pub(super) pick: Pick,
    pub(super) text: String,
    pub(super) images: Vec<Image>,
    pub(super) by_hand: bool,
}

/// What making a copy needs, read under the lock.
pub(super) struct Plan {
    run: GitRunner,
    folder: PathBuf,
    data: PathBuf,
    id: String,
    branch: String,
    from: Option<String>,
    /// The commit the copy starts at (a standing goal's crashed build), when it is not the head.
    at: Option<String>,
}

impl Engine {
    pub fn set_git(&self, run: GitRunner) {
        self.with(|c| c.git = Some(run));
    }

    pub(super) fn make_copy(plan: Option<Plan>) -> Result<Made, String> {
        let Some(plan) = plan else { return Ok(None) };
        let Some(mut exec) = (plan.run)() else { return Ok(None) };
        copy::make(&mut *exec, &plan.folder, &plan.data, &plan.id, &plan.branch, plan.from.as_deref(), plan.at.as_deref())
            .map(|made| made.map(|c| (plan.id, c)))
            .map_err(str::to_string)
    }

    /// Sends a message. A project conversation's copy is made after the message is recorded, away from the lock, and the
    /// answer starts once it is there (`Core::hold_for_copy`), so the screen never waits for git.
    pub fn send(&self, req: SendRequest) -> Result<String, String> {
        self.with(|c| {
            let kept = c.kept_kind(&req);
            let sent = c.send(req, None);
            if let (Ok(_), Some(kind)) = (&sent, kept) {
                c.learn_kept(&kind);
            }
            sent
        })
    }

    /// "Forget what Auto learned": every nudge Auto took from the answers this person redid or kept, here and on disk. The error
    /// key says the file could not be written: what is in memory is forgotten, but the next start would load the old file.
    pub fn forget_auto_learned(&self) -> Result<(), String> {
        self.with(|c| c.forget_learned()).map_err(|_| "error.autoLearned.forget".to_string())
    }

    /// Edits a message or redoes an answer as a new conversation; in a project it gets a copy of its own, from the
    /// source's branch when the source has one.
    pub fn branch(&self, req: BranchRequest) -> Result<String, String> {
        self.branch_as(req, false)
    }

    /// "Branch from here" on an answer (parity T7): a new conversation with every message up to it, where nothing is sent
    /// until the person writes.
    pub fn branch_here(&self, conversation_id: &str, message_id: &str) -> Result<String, String> {
        let title = self.with(|c| c.load(conversation_id).map(|c| c.title.clone())).ok_or("error.messageGone")?;
        let choice = Choice { vendor: "auto".into(), model: None, effort: None };
        self.branch_as(BranchRequest { conversation_id: conversation_id.into(), message_id: message_id.into(), text: title, choice }, true)
    }

    fn branch_as(&self, req: BranchRequest, here: bool) -> Result<String, String> {
        let made = Self::make_copy(self.with(|c| c.copy_plan_branch(&req)))?;
        let (result, stray) = self.with(|c| {
            let id = made.as_ref().map(|(id, _)| id.clone());
            let result = c.branch(req, made.clone(), here);
            // A branch refused before its conversation existed leaves the copy behind.
            let stray = made.filter(|_| id.is_some_and(|id| !c.convs.contains_key(&id)));
            (result, stray)
        });
        self.drop_copy(stray);
        result
    }

    /// Deletes a conversation. A copy that holds work found nowhere else (changes not committed, commits no other branch
    /// has) is never deleted without `discard`: the answer is `error.copy.unsaved`, and the screen asks the person. An agent
    /// still at work there may write more after any look, so its copy counts as unsaved too.
    pub fn remove_with(&self, id: &str, discard: bool) -> Result<(), String> {
        let found = self.with_rewind_delete(id, |c| Ok::<_, String>(c.copy_state(id).map(|state| (state, c.git.clone()))))?;
        if let Some(((copy, busy, _), run)) = &found
            && !discard
        {
            let mut exec = run.as_ref().and_then(|run| run());
            if *busy || exec.as_mut().is_none_or(|exec| copy::unsaved(&mut **exec, copy)) {
                return Err("error.copy.unsaved".into());
            }
        }
        // A message sent while git looked may have started the agent again: then the look no longer holds, and it asks.
        let moved = self.with_rewind_delete(id, |c| {
            let seen = found.as_ref().map(|((_, _, mark), _)| *mark);
            let moved = !discard && seen.is_some() && c.copy_state(id).is_some_and(|(_, busy, now)| busy || Some(now) != seen);
            if !moved {
                c.remove(id)?;
            }
            Ok::<_, String>(moved)
        })?;
        if moved {
            return Err("error.copy.unsaved".into());
        }
        self.drop_copy(found.map(|((copy, ..), _)| (id.to_string(), copy)));
        Ok(())
    }

    /// Removes a copy away from the lock. The conversation's CLI has just been closed and may still hold its files for a
    /// moment, so it tries again a few times.
    pub(super) fn drop_copy(&self, made: Made) {
        let Some((id, copy)) = made else { return };
        let Some((run, data)) = self.with(|c| c.git.clone().map(|run| (run, c.opts.data_dir.clone()))) else { return };
        std::thread::spawn(move || {
            let Some(mut exec) = run() else { return };
            for attempt in 0..10 {
                match copy::remove(&mut *exec, &data, &id, &copy) {
                    Ok(()) => return,
                    Err(_) if attempt < 9 => std::thread::sleep(Duration::from_millis(500)),
                    Err(reason) => eprintln!("faber: a conversation's copy stays ({reason})"),
                }
            }
        });
    }

    /// Run once at start, away from the lock: the copy of a deleted conversation goes, and so does the copy of a
    /// conversation that is idle, clean and merged into the person's branch. That conversation keeps its record, and its
    /// next message makes the copy again in the same folder, so its CLI session resumes.
    pub fn sweep_copies(&self) {
        let Some((run, data)) = self.with(|c| c.git.clone().map(|run| (run, c.opts.data_dir.clone()))) else { return };
        let Some(mut exec) = run() else { return };
        for id in copy::found(&data) {
            let known = data.join("conversations").join(format!("{id}.json")).exists() || self.with(|c| c.convs.contains_key(&id));
            if !known {
                // A conversation is deleted only once its copy is saved or the person said so: what is left is removed.
                if let Some(orphan) = copy::orphan(&mut *exec, &data, &id) {
                    let _ = copy::remove(&mut *exec, &data, &id, &orphan);
                }
                continue;
            }
            // Idle: no CLI open there (it would hold the folder), nothing running, waiting for a route or queued.
            let idle = |c: &mut Core| match c.copy_state(&id) {
                Some((copy, false, _)) if !c.live.contains_key(&id) => Some(copy),
                _ => None,
            };
            let Some(record) = self.with(idle) else { continue };
            // Set aside under the lock while still idle, so a message sent meanwhile makes the copy again instead of
            // starting its agent in a folder being deleted.
            if copy::merged(&mut *exec, &record) && self.with(|c| idle(c).is_some() && copy::set_aside(&data, &id)) {
                let _ = copy::retire(&mut *exec, &data, &id, &record);
            }
        }
    }
}

impl Core {
    /// The project a conversation belongs to.
    pub(super) fn project_of(&self, id: Option<&str>) -> Option<Project> {
        let id = id?;
        self.store.projects().into_iter().find(|p| p.id == id)
    }

    /// Whether this conversation will get a copy at its first message: a warm-up must not start its CLI in the project
    /// folder only to move it a moment later.
    pub(super) fn copy_due(&self, id: &str) -> bool {
        self.git.is_some()
            && (self.copying.contains_key(id)
                || self.convs.get(id).is_some_and(|c| {
                    c.area == Area::Project
                        && (c.copy.as_ref().is_some_and(|copy| copy.root.is_empty())
                            || (c.copy.is_none()
                                && (c.messages.is_empty() || c.project_start_pending)
                                && self.project_of(c.project_id.as_deref()).is_some_and(|p| p.separate_copies)))
                }))
    }

    /// The copy is named and saved before its first project turn starts: whatever happens meanwhile, a restart
    /// included, the conversation's next message makes this copy and nothing runs in the project folder. A record without a
    /// repository (`root`) is only that name, until `attach_copy` replaces it.
    pub(super) fn name_copy(&mut self, id: &str, plan: &Plan) {
        if let Some(c) = self.convs.get_mut(id)
            && c.copy.is_none()
        {
            let path = copy::home(&plan.data).join(id).to_string_lossy().into_owned();
            c.copy = Some(WorkCopy { path, branch: plan.branch.clone(), root: String::new(), sub: String::new(), base: None });
        }
    }

    /// The message is recorded and its copy is made on a blocking thread; `tail` starts the answer once it is attached.
    pub(super) fn hold_for_copy(&mut self, id: &str, reply: &str, plan: Plan, tail: Tail) {
        self.copying.insert(id.to_string(), Held { reply: reply.to_string(), tail: Some(tail) });
        let (me, runtime, id) = (self.me.clone(), self.opts.runtime.clone(), id.to_string());
        self.opts.runtime.spawn_blocking(move || {
            let made = Engine::make_copy(Some(plan));
            let Some(core) = me.upgrade() else { return };
            let engine = Engine { core, runtime };
            let stray = engine.with(|c| c.copy_made(&id, made));
            engine.drop_copy(stray);
        });
    }

    /// The copy is ready, or git could not make it: attach it, then start what waited for it. An answer stopped or
    /// removed meanwhile starts nothing, and a copy that no longer fits comes back to be removed.
    fn copy_made(&mut self, id: &str, made: Result<Made, String>) -> Made {
        let failed = made.is_err();
        let mut stray = None;
        match made {
            Ok(Some((made_for, copy))) => {
                stray = self.attach_copy(&made_for, copy);
                // Saved now: an answer stopped meanwhile writes nothing else, and the next message must find its copy.
                if stray.is_none() {
                    self.save(&made_for);
                }
            }
            // Not a repository with a commit after all: the conversation works in the folder, and its placeholder goes.
            Ok(None) => {
                if let Some(c) = self.convs.get_mut(id)
                    && c.copy.as_ref().is_some_and(|copy| copy.root.is_empty())
                {
                    c.copy = None;
                    self.save(id);
                }
            }
            // The placeholder stays, so the next message tries this copy again.
            Err(_) => {}
        }
        let Some(Held { reply, tail }) = self.copying.remove(id) else { return stray };
        if !self.convs.contains_key(id) {
            self.try_seal_rewind(id);
        }
        let streaming = !self.closing
            && self
                .convs
                .get(id)
                .and_then(|c| c.messages.iter().find(|m| m.id == reply))
                .is_some_and(|m| m.status == Some(Status::Streaming));
        if !streaming {
            self.finish_worker_admission(&Self::turn_admission_key(id, &reply));
            return stray;
        }
        match tail {
            _ if failed => self.fail_reply(id, &reply, "error.copy.failed".into()),
            Some(Tail { pick, text, images, by_hand }) => {
                // An agent's brief may have been marked unattended after send returned, while git worked.
                let by_hand = by_hand && self.transfers.get(id).is_none_or(|t| t.by_hand);
                if let Err(error) = self.dispatch(id, &reply, &pick, text, images, by_hand) {
                    self.fail_reply(id, &reply, error);
                }
            }
            // Auto's route is not back yet: it starts the answer when it is, and nothing waits for the copy any more.
            None => {}
        }
        stray
    }

    /// What the common session entrance must make before the first project turn, or restore after a merged copy retired.
    pub(super) fn copy_plan(&self, id: &str) -> Option<Plan> {
        let c = self.convs.get(id)?;
        if c.area != Area::Project || self.installing {
            return None;
        }
        let run = self.git.clone()?;
        let project = self.project_of(c.project_id.as_deref())?;
        let branch = match &c.copy {
            // A placeholder (no `root`) is a copy not finished: it is made again.
            Some(copy) if !copy.root.is_empty() && Path::new(&copy.path).is_dir() => return None,
            Some(copy) => copy.branch.clone(),
            // A conversation an agent started has its title already: a better name than the brief.
            // A session picked up from a terminal (parity F5) goes on in the folder it ran in; a moved chat has none left.
            None if (c.messages.is_empty() || c.project_start_pending) && c.sessions.is_empty() && project.separate_copies => {
                copy::branch_for(&c.title, id)
            }
            None => return None,
        };
        let data = self.opts.data_dir.clone();
        Some(Plan {
            run,
            folder: PathBuf::from(project.path),
            data,
            id: id.to_string(),
            branch,
            from: None,
            at: self.goals.at.get(id).cloned(),
        })
    }

    fn copy_plan_branch(&mut self, req: &BranchRequest) -> Option<Plan> {
        let run = self.git.clone()?;
        let source = self.load(&req.conversation_id)?.clone();
        let project = self.project_of(source.project_id.as_deref())?;
        if source.area != Area::Project || (source.copy.is_none() && !project.separate_copies) {
            return None;
        }
        let id = new_id();
        let branch = copy::branch_for(&req.text, &id);
        let data = self.opts.data_dir.clone();
        Some(Plan { run, folder: PathBuf::from(project.path), data, id, branch, from: source.copy.map(|c| c.branch), at: None })
    }

    /// A conversation's copy, whether its agent is at work or about to be (a turn running, a reply still open while Auto
    /// picks who answers, messages queued), and a mark that changes with every message.
    fn copy_state(&mut self, id: &str) -> Option<(WorkCopy, bool, (usize, u64))> {
        let turn = self.live.get(id).is_some_and(|l| l.turn.is_some());
        let conv = self.load(id)?;
        let open = conv.messages.last().is_some_and(|m| m.status == Some(Status::Streaming));
        let busy = turn || open || !conv.queued.is_empty();
        // A placeholder has no repository to ask git about: nothing there is the person's work yet.
        Some((conv.copy.clone().filter(|copy| !copy.root.is_empty())?, busy, (conv.messages.len(), conv.updated_at)))
    }

    /// Records a copy on its conversation; one that no longer fits (another send got there first) comes back to be removed.
    pub(super) fn attach_copy(&mut self, id: &str, copy: WorkCopy) -> Made {
        match self.convs.get_mut(id) {
            Some(c) if c.copy.as_ref().map_or(c.messages.is_empty(), |had| had.path == copy.path) => {
                c.copy = Some(copy);
                self.goals.at.remove(id);
                None
            }
            _ => Some((id.to_string(), copy)),
        }
    }
}
