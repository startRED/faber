//! A project conversation's own copy (R201, `crate::project::copy`): made before its first turn starts and used as
//! the agent's folder from then on, removed with the conversation, and retired in an idle sweep when its work is merged.
//! Git runs outside the engine's lock, since a checkout can take seconds; the lock only reads what to do and records it.
use super::*;
use crate::project::copy::{self, WorkCopy};
use crate::project::git::{Executor, same_path};

/// Opens a git runner. The shell gives the real one; without one (tests, a PC without git) every project conversation
/// works in the project folder, as before copies.
pub type GitRunner = Arc<dyn Fn() -> Option<Box<dyn Executor + Send>> + Send + Sync>;

/// A copy made for conversation `.0`, not yet recorded on it.
type Made = Option<(String, WorkCopy)>;

struct Sweep(Weak<super::slow_lock::Timed<Core>>);
impl Drop for Sweep {
    fn drop(&mut self) {
        if let Some(core) = self.0.upgrade() {
            core.lock().unwrap_or_else(|e| e.into_inner()).copy_sweeps.finish();
        }
    }
}

enum SweepAction {
    Stop,
    Skip,
    Busy,
    Kept,
    Retire(copy::Retirement),
}

#[derive(Default)]
struct SweepCounts {
    retired: usize,
    kept_unmerged: usize,
    kept_busy: usize,
}

impl SweepCounts {
    fn log(self) {
        eprintln!(
            "faber: copy sweep retired {}, kept {} without a confirmed merge, kept {} busy",
            self.retired, self.kept_unmerged, self.kept_busy
        );
    }
}

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
    profile: PathBuf,
    id: String,
    branch: String,
    from: Option<String>,
    /// The commit the copy starts at (a standing goal's crashed build), when it is not the head.
    at: Option<String>,
    /// The copy is a plan step's: the commit it starts at is read once it is made, for the step to record.
    step: bool,
}

impl Engine {
    pub fn set_git(&self, run: GitRunner) {
        let missing = run().is_none();
        self.with(|c| {
            c.git = Some(run);
            c.git_missing = missing;
        });
    }

    pub(super) fn refresh_git(&self) {
        crate::project::git::Process::refresh();
        let run = self.with(|c| c.git.clone());
        let missing = run.is_some_and(|run| run().is_none());
        self.with(|c| {
            if c.git_missing != missing {
                c.git_missing = missing;
                c.list_changed();
            }
        });
    }

    pub(super) fn make_copy(plan: Option<Plan>) -> Result<Made, String> {
        let Some(plan) = plan else { return Ok(None) };
        if !storage_available(&plan.profile, &plan.data) {
            return Err("error.storage.unreachable".into());
        }
        let Some(mut exec) = (plan.run)() else { return Ok(None) };
        let fresh = std::fs::symlink_metadata(copy::home(&plan.data).join(&plan.id)).is_err();
        let made = copy::make(&mut *exec, &plan.folder, &plan.data, &plan.id, &plan.branch, plan.from.as_deref(), plan.at.as_deref())
            .map_err(str::to_string)?;
        if fresh && let Some(copy) = &made {
            seed_build_folders(&plan.data, &plan.id, copy);
        }
        Ok(made.map(|c| (plan.id, c)))
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
        let plan = self.with(|c| {
            let plan = c.copy_plan_branch(&req)?;
            c.copying.insert(plan.id.clone(), Held { reply: String::new(), tail: None });
            Some(plan)
        });
        let copying = plan.as_ref().map(|plan| plan.id.clone());
        let made = Self::make_copy(plan);
        let (result, stray) = self.with(|c| {
            if let Some(id) = copying {
                c.copying.remove(&id);
            }
            let made = made?;
            let id = made.as_ref().map(|(id, _)| id.clone());
            let result = c.branch(req, made.clone(), here);
            // A branch refused before its conversation existed leaves the copy behind.
            let stray = made.filter(|_| id.is_some_and(|id| !c.convs.contains_key(&id)));
            Ok::<_, String>((result, stray))
        })?;
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
            let had = exec.is_some();
            let lost = exec.as_mut().is_none_or(|exec| copy::unsaved(&mut **exec, copy));
            // One line for whoever reads the log: which look kept the copy (a person only sees "Discard changes?").
            eprintln!("faber: deleting {id} asks first (at work: {busy}, git answered: {had}, work found nowhere else: {lost})");
            if *busy || lost {
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
        let Some((run, data)) = self
            .with(|c| {
                let data = c.copy_storage(&id, &copy)?;
                c.copy_storage_available(&data).then(|| (c.git.clone(), data))
            })
            .and_then(|(run, data)| Some((run?, data)))
        else {
            return;
        };
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

    /// Run at start and idle turn boundaries, away from the lock: a deleted conversation's copy goes, and so does the copy of a
    /// conversation that is idle, clean and merged into the person's branch. That conversation keeps its record, and its
    /// next message makes the copy again in the same folder, so its CLI session resumes.
    pub fn sweep_copies(&self) {
        if self.with(Core::start_copy_sweep) {
            // Background thread priority must never leak into Tokio's reusable blocking pool.
            std::thread::scope(|scope| {
                let result = std::thread::Builder::new().name("project-storage".into()).spawn_scoped(scope, || {
                    let _runtime = self.runtime.enter();
                    self.sweep_project_storage().log();
                });
                if let Ok(worker) = result {
                    let _ = worker.join();
                } else {
                    self.with(|c| c.copy_sweeps.finish());
                }
            });
        }
    }

    fn sweep_project_storage(&self) -> SweepCounts {
        let _finish = Sweep(Arc::downgrade(&self.core));
        let mut counts = SweepCounts::default();
        // Linux has no background class here yet: the sweep keeps its normal priority there.
        #[cfg(any(windows, target_os = "macos"))]
        let _background = faber_winproc::background_thread();
        let Some((run, profile, roots)) = self.with(|c| c.git.clone().map(|run| (run, c.opts.data_dir.clone(), c.storage_roots.clone())))
        else {
            return counts;
        };
        let Some(mut exec) = run() else { return counts };
        if !self.with(|c| c.copy_sweep_allowed()) {
            return counts;
        }
        let roots = std::iter::once(profile.clone())
            .chain(roots.into_iter().map(|root| (*root).clone()))
            .filter(|root| storage_available(&profile, root));
        for (data, id) in roots
            .inspect(|root| crate::project::seed::sweep_leftovers(root))
            .flat_map(|root| copy::found(&root, || self.with(|c| c.copy_sweep_allowed())).into_iter().map(move |id| (root.clone(), id)))
        {
            if !self.with(|c| c.copy_sweep_allowed()) {
                break;
            }
            // A copy whose build folders are being copied in stays as it is until that ends.
            if crate::project::seed::seeding(&copy::home(&data).join(&id)) {
                continue;
            }
            let saved = profile.join("conversations").join(format!("{id}.json"));
            let known = |c: &Core| c.copying.contains_key(&id) || c.convs.contains_key(&id) || saved.exists();
            if !self.with(|c| known(c)) {
                // A conversation is deleted only once its copy is saved or the person said so: what is left is removed.
                if let Some(orphan) = copy::orphan(&mut *exec, &data, &id)
                    && self.with(|c| c.copy_sweep_allowed() && !known(c))
                {
                    let _ = copy::remove(&mut *exec, &data, &id, &orphan);
                }
                continue;
            }
            // Every writer in this copy counts, including ones outside the conversation's main session.
            let state = |c: &mut Core| {
                let (record, busy, mark) = c.copy_state(&id)?;
                if c.copy_storage(&id, &record).as_ref() != Some(&data) {
                    return None;
                }
                let busy = busy
                    || c.live.contains_key(&id)
                    || !c.session_idle(&id)
                    || c.rewind_blocked(&id)
                    // A closed session's final rewind scan still reads this folder.
                    || c.rewind_sealing_in(Path::new(&record.path))
                    || c.app_active(&id, Path::new(&record.path))
                    || c.asking.values().any(|a| a.conversation() == id)
                    || c.verifier.as_ref().is_some_and(|verifier| verifier.active_in(Path::new(&record.path)));
                Some((record, busy, mark))
            };
            let Some((record, busy, mark)) = self.with(state) else { continue };
            if busy {
                counts.kept_busy += 1;
                continue;
            }
            let merged = copy::merged(&mut *exec, &record);
            let unchanged = |current: &(WorkCopy, bool, (usize, u64))| !current.1 && current.2 == mark && current.0 == record;
            // Recheck and register the rename in the same lock that admits new sends.
            let action = self.with(|c| {
                if !c.copy_sweep_allowed() {
                    SweepAction::Stop
                } else {
                    match state(c) {
                        None => SweepAction::Skip,
                        Some(current) if !unchanged(&current) => SweepAction::Busy,
                        Some(_) if merged => copy::set_aside(&data, &id, &record).map_or(SweepAction::Busy, SweepAction::Retire),
                        Some(_) => SweepAction::Kept,
                    }
                }
            });
            match action {
                SweepAction::Stop => break,
                SweepAction::Skip => continue,
                SweepAction::Busy => {
                    counts.kept_busy += 1;
                    continue;
                }
                SweepAction::Kept => counts.kept_unmerged += 1,
                SweepAction::Retire(retirement) => {
                    if retirement.finish(&mut *exec, &record).is_ok() {
                        counts.retired += 1;
                    } else {
                        counts.kept_busy += 1;
                    }
                    continue;
                }
            }
            if mark.1 != 0 {
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                let age = now.saturating_sub(Duration::from_millis(mark.1));
                copy::cleanup(&mut *exec, &data, &id, &record, age, false, |remove| {
                    self.with(|c| {
                        if c.copy_sweep_allowed() && state(c).as_ref().is_some_and(unchanged) {
                            remove();
                        }
                    });
                });
            }
        }
        let roots = self
            .with(|c| c.project_storage_idle().then(|| c.store.projects().into_iter().map(|p| PathBuf::from(p.path)).collect::<Vec<_>>()));
        if let Some(roots) = roots {
            crate::project::maintenance::run(&mut *exec, &roots, || self.with(|c| c.project_storage_idle()));
        }
        counts
    }
}

/// A new copy starts its build folders from the project's on a thread of its own, so the answer never waits for the copying
/// (task 203). The thread reads and writes files only, starts no process and ends when the folders are in place.
fn seed_build_folders(data: &Path, id: &str, copy: &WorkCopy) {
    let Some(plan) = crate::project::seed::plan(data, id, Path::new(&copy.root), Path::new(&copy.path)) else { return };
    // Marked before the thread starts, so a sweep in between never retires the copy.
    let seeding = crate::project::seed::begin(plan);
    let id = id.to_string();
    let _ = std::thread::Builder::new().name("faber-copy-seed".into()).spawn(move || {
        let free = |path: &Path| faber_winproc::rewind::Root::open(path).ok()?.available_space().ok();
        for (folder, outcome) in seeding.run(crate::project::seed::limits(), free) {
            eprintln!("faber: copy {id} build folder {} {outcome:?}", folder.display());
        }
    });
}

fn storage_available(profile: &Path, root: &Path) -> bool {
    root == profile
        || (crate::storage::external_root_name(root)
            && crate::project::lease::real_path(&copy::home(root)).is_ok()
            && crate::storage::owns_external_root(profile, root))
}

impl Core {
    pub(super) fn copy_storage(&self, id: &str, copy: &WorkCopy) -> Option<PathBuf> {
        std::iter::once(&self.opts.data_dir)
            .chain(self.storage_roots.iter().map(Arc::as_ref))
            .find(|root| same_path(&copy::home(root).join(id), Path::new(&copy.path)))
            .cloned()
            .or_else(|| {
                // A copy recorded before storage could be chosen, or at a path no chosen folder holds, keeps the main
                // data folder's rules, as it always had; one inside a chosen folder under another name is refused.
                // No link on the way either: one could lead into a chosen folder's copy of another conversation.
                let path = Path::new(&copy.path);
                let chosen = self.storage_roots.iter().any(|root| crate::project::git::path_within(path, &copy::home(root)));
                (!chosen && crate::project::lease::real_path(path).is_ok()).then(|| self.opts.data_dir.clone())
            })
    }

    pub(super) fn copy_storage_available(&self, root: &Path) -> bool {
        storage_available(&self.opts.data_dir, root)
    }

    pub(super) fn checked_copy_folder(&self, id: &str, copy: &WorkCopy) -> Option<PathBuf> {
        let root = self.copy_storage(id, copy)?;
        let folder = copy.folder();
        if root == self.opts.data_dir {
            return Some(folder);
        }
        if !self.copy_storage_available(&root) || !crate::project::git::path_within(&folder, Path::new(&copy.path)) {
            return None;
        }
        crate::project::lease::real_path(&folder).ok()
    }

    fn new_copy_storage(&self) -> PathBuf {
        self.new_storage.as_ref().unwrap_or(&self.opts.data_dir).clone()
    }

    fn copy_sweep_allowed(&self) -> bool {
        !self.closing
            && self.agent_installs == 0
            && self.copying.is_empty()
            && self.verifier.as_ref().is_none_or(|v| v.idle() && !v.game_in_front())
    }

    fn project_storage_idle(&self) -> bool {
        self.copy_sweep_allowed() && self.goals.running.is_empty() && self.convs.keys().all(|id| self.session_idle(id))
    }

    fn start_copy_sweep(&mut self) -> bool {
        self.git.is_some() && self.copy_sweep_allowed() && self.copy_sweeps.begin(std::time::Instant::now())
    }

    pub(super) fn schedule_copy_sweep(&mut self) {
        if !self.start_copy_sweep() {
            return;
        }
        let Some(core) = self.me.upgrade() else {
            self.copy_sweeps.finish();
            return;
        };
        let engine = Engine { core, runtime: self.opts.runtime.clone() };
        let result = std::thread::Builder::new().name("project-storage".into()).spawn(move || {
            let _runtime = engine.runtime.enter();
            engine.sweep_project_storage().log();
        });
        if result.is_err() {
            self.copy_sweeps.finish();
        }
    }

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
            let (step, run) = (plan.step, plan.run.clone());
            let made = Engine::make_copy(Some(plan));
            let started = made.as_ref().ok().and_then(|m| m.as_ref()).filter(|_| step);
            let base = started.and_then(|(_, c)| run().and_then(|mut exec| copy::head(&mut *exec, Path::new(&c.path))));
            let Some(core) = me.upgrade() else { return };
            let engine = Engine { core, runtime };
            let stray = engine.with(|c| c.copy_made(&id, made, base));
            engine.drop_copy(stray);
        });
    }

    /// The copy is ready, or git could not make it: attach it, then start what waited for it. An answer stopped or
    /// removed meanwhile starts nothing, and a copy that no longer fits comes back to be removed.
    fn copy_made(&mut self, id: &str, made: Result<Made, String>, base: Option<String>) -> Made {
        let failure = made
            .as_ref()
            .err()
            .map(|key| if key == "error.storage.unreachable" { "error.storage.unreachable" } else { "error.copy.failed" });
        let mut stray = None;
        match made {
            Ok(Some((made_for, copy))) => {
                let branch = copy.branch.clone();
                stray = self.attach_copy(&made_for, copy);
                // Saved now: an answer stopped meanwhile writes nothing else, and the next message must find its copy.
                if stray.is_none() {
                    self.save(&made_for);
                    // A plan step's worker: the step records the branch and the leader's commit the copy started at.
                    self.edit_worker(&made_for, |w| {
                        if w.branch.is_empty() {
                            (w.branch, w.base) = (branch, base.unwrap_or_default());
                        }
                    });
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
            _ if let Some(failure) = failure => {
                self.fail_reply(id, &reply, failure.into());
                self.step_not_started(id);
            }
            Some(Tail { pick, text, images, by_hand }) => {
                // An agent's brief may have been marked unattended after send returned, while git worked.
                let by_hand = by_hand && self.transfers.get(id).is_none_or(|t| t.by_hand);
                if let Err(error) = self.dispatch(id, &reply, &pick, text, images, by_hand) {
                    self.fail_reply(id, &reply, error);
                    self.step_not_started(id);
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
        let data = match &c.copy {
            Some(copy) => self.copy_storage(id, copy)?,
            None => self.new_copy_storage(),
        };
        // A plan step's conversation starts from the leader's latest work, as a branched one starts from its source.
        let lead = self.step_lead(id);
        Some(Plan {
            run,
            folder: PathBuf::from(project.path),
            data,
            profile: self.opts.data_dir.clone(),
            id: id.to_string(),
            branch,
            from: lead.clone().flatten(),
            at: self.goals.at.get(id).cloned(),
            step: lead.is_some(),
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
        let data = self.new_copy_storage();
        Some(Plan {
            run,
            folder: PathBuf::from(project.path),
            data,
            profile: self.opts.data_dir.clone(),
            id,
            branch,
            from: source.copy.map(|c| c.branch),
            at: None,
            step: false,
        })
    }

    /// For a conversation started for a plan step: the branch of its leader's copy, None when the leader works in the project
    /// folder itself. None for any other conversation.
    fn step_lead(&self, id: &str) -> Option<Option<String>> {
        let lead = self.convs.get(self.convs.get(id)?.started_by.as_ref()?)?;
        let is = |s: &PlanStep| s.worker.as_ref().is_some_and(|w| w.conversation == id);
        lead.plan.as_ref()?.steps.iter().any(is).then(|| lead.copy.as_ref().map(|c| c.branch.clone()))
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
            Some(c) if c.copy.as_ref().map_or(c.messages.is_empty(), |had| same_path(Path::new(&had.path), Path::new(&copy.path))) => {
                c.copy = Some(copy);
                self.goals.at.remove(id);
                None
            }
            _ => Some((id.to_string(), copy)),
        }
    }
}

#[cfg(test)]
mod storage_choice_tests {
    use super::*;
    use crate::project::git::{GitError, Invocation, Output};

    struct Git(PathBuf);

    impl Executor for Git {
        fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
            let mut args = call.args.as_slice();
            while args.first().is_some_and(|arg| arg == "-c") {
                args = &args[2..];
            }
            let text = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
                ["rev-parse", "--show-toplevel", "--show-prefix"] => format!("{}\n\n", self.0.display()),
                ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"] => "1".repeat(40),
                ["symbolic-ref", "--quiet", "HEAD"] => "refs/heads/main".into(),
                ["rev-parse", ..] => return Ok(Output { success: false, stdout: vec![] }),
                // A copy on a chosen drive is locked against `git worktree prune` while that drive is away.
                ["worktree", "add", "--lock", "-b", _, "--", path, _] => {
                    std::fs::create_dir(path).unwrap();
                    String::new()
                }
                ["worktree", "prune"] | ["worktree", "unlock", "--", _] => String::new(),
                ["status", "--porcelain", "-z"] => " M changed.txt".into(),
                other => panic!("unexpected git arguments: {other:?}"),
            };
            Ok(Output { success: true, stdout: text.into_bytes() })
        }
    }

    #[tokio::test]
    async fn storage_choice_places_new_copies_and_keeps_the_previous_location_on_reset() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let temp = tempfile::tempdir().unwrap();
        let temp = temp.path().canonicalize().unwrap();
        let folder = temp.join("project");
        let storage = temp.join("Faber-storage");
        std::fs::create_dir(&folder).unwrap();
        std::fs::create_dir(&storage).unwrap();
        std::fs::write(storage.join(".faber-owner"), dir.path().to_str().unwrap()).unwrap();
        let project = engine.add_project(folder.to_str().unwrap()).unwrap();
        engine.set_git(Arc::new(move || Some(Box::new(Git(folder.clone())))));
        let first = engine.with(|core| core.blank(Area::Project, Some(project.id.clone())));
        engine.set_new_storage(Some(storage.clone()), [storage.clone()].into());
        let (_, copy) = Engine::make_copy(engine.with(|core| core.copy_plan(&first))).unwrap().unwrap();
        assert_eq!(Path::new(&copy.path), copy::home(&storage).join(&first));
        assert!(Path::new(&copy.path).is_dir());
        let detached = temp.join("detached");
        std::fs::rename(&storage, &detached).unwrap();
        let plan = engine.with(|core| {
            let next = core.blank(Area::Project, Some(project.id.clone()));
            core.copy_plan(&next)
        });
        assert_eq!(Engine::make_copy(plan).err().as_deref(), Some("error.storage.unreachable"));
        std::fs::rename(detached, &storage).unwrap();
        engine.set_new_storage(None, [storage.clone()].into());
        engine.with(|core| {
            assert_eq!(core.copy_storage(&first, &copy), Some(storage.clone()));
            assert!(core.checked_copy_folder(&first, &copy).is_some());
            let conversation = core.convs.get_mut(&first).unwrap();
            conversation.copy = Some(copy.clone());
            conversation.updated_at = 0;
            let next = core.blank(Area::Project, Some(project.id));
            assert_eq!(core.copy_plan(&next).unwrap().data, core.opts.data_dir);
        });
        assert_eq!(engine.sweep_project_storage().kept_unmerged, 1);
        std::fs::write(storage.join(".faber-owner"), "a different profile").unwrap();
        engine.with(|core| assert!(core.checked_copy_folder(&first, &copy).is_none()));
    }
}
