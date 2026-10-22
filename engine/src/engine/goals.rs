//! Standing goals (phase 1, `crate::project::goals`): a project keeps goals that never finish, each one of its confirmed checks.
//! While the person has the project's switch on, a tick of the 30 second loop (`watch_accounts`) looks for a goal that is
//! due and runs its check with Faber's checks (`crate::verify`). Two failures ten minutes apart mean it broke, and Faber
//! starts a conversation in a copy of its own to reproduce and fix it, within the project's day cap.
//!
//! Idle costs nothing: the goals of every project are read once (a read of `projects.json`) and kept, and while no project has
//! a goal with its switch on, a tick is a look at that map and returns. A check runs only when the project is quiet (no
//! conversation working, nothing from the person for 15 minutes), no game is in front, the tree is clean at its HEAD and the
//! quota gate is open for a turn Faber starts by itself. One check runs at a time, and it counts as work: an update waits.
use super::*;
use crate::project::checks;
use crate::project::goals::{self, Check, Goal, Goals, Next, State};
use crate::verify::{self, Level, Local, Outcome, Request};

/// The key shown wherever the day's money for fixes stopped a turn Faber would have sent by itself.
pub(super) const CAP: &str = "reason.goal.cap";

/// A gate that failed while the check was being prepared (a dirty tree, a stopped run) is asked again after this long (seconds).
const RETRY: f64 = 300.0;

/// Why a fix did not start.
enum Refused {
    /// Something in the way may pass (a game, a conversation at work, an update): nothing changes and the check is asked
    /// again after `RETRY`.
    Wait,
    /// The person decides; the key says why.
    Needs(&'static str),
}

/// How a check came out, before the result is read.
enum Ran {
    /// It is no longer on the list the person confirmed: not run, asked again at the next turn of the clock.
    Unconfirmed,
    /// Not run now (a tree with changes, a run that could not start).
    Skipped,
    Done(Outcome),
}

/// What the engine keeps for standing goals between ticks.
#[derive(Default)]
pub(super) struct Watch {
    /// The goals of every project that has some, read once; `None` until the first look and after a project was changed
    /// elsewhere (Settings, removing it).
    pub(super) loaded: Option<HashMap<String, Goals>>,
    /// When the first tick ran: a goal is first due `FIRST_AFTER` later.
    started: Option<f64>,
    /// The goals whose check runs now.
    pub(super) running: HashSet<String>,
    /// A goal not asked again before this time.
    wait: HashMap<String, f64>,
    /// Projects whose goals record is there but cannot be read: a fix of theirs has no money until it can (`goal_out_of_money`).
    unreadable: HashSet<String>,
    /// What fixes cost whose write failed, by project: counted against the day by the gate until a write goes through.
    unpaid: HashMap<String, f64>,
}

/// The first line of the brief a fixing conversation gets. It is for the model, so it is English like every prompt of the
/// engine; the person sees `wake.goalBroke` in their language instead.
fn brief(goal: &Goal, output: &str) -> String {
    format!(
        "Faber watches this goal for the project: \"{}\". Its check `{}` failed twice, ten minutes apart. The output of the \
         second run:\n\n{output}\n\nReproduce it first in this copy and say how you did. Fix the cause, run Faber's checks to \
         prove it, commit and open a pull request. Never merge it or push to the main branch: the person reads the pull request and approves it. If you cannot reproduce it, say so and stop. Do not ask the person.",
        goal.words, goal.check
    )
}

impl Core {
    /// The goals of every project that has some. Read once; `None` while the list cannot be read, and it is asked again at the
    /// next look (never taken for an empty one, which would undercount the day's spending). A project whose record cannot be
    /// read is left out: nothing is watched or started for it.
    fn goals_all(&mut self) -> Option<&HashMap<String, Goals>> {
        if self.goals.loaded.is_none()
            && let Ok(projects) = self.store.projects_checked()
        {
            let mut unreadable = HashSet::new();
            let all = projects
                .iter()
                .filter_map(|p| match Goals::read(p) {
                    Ok(g) => Some((p.id.clone(), g)),
                    Err(_) => {
                        unreadable.insert(p.id.clone());
                        None
                    }
                })
                .filter(|(_, g)| *g != Goals::default())
                .collect();
            self.goals.unreadable = unreadable;
            self.goals.loaded = Some(all);
        }
        self.goals.loaded.as_ref()
    }

    /// Whether `conv` is a goal's fix, or a conversation that fix's agent started (it carries `goal_fix` from the start, whatever
    /// the goal's own pointer says later), and the project's day has no money left. Then no turn Faber starts by itself and no
    /// new conversation goes on there; a turn already running is never cut. It fails closed: when the projects list or that
    /// project's goals record cannot be read, the day's money is unknown, so there is none.
    pub(super) fn goal_out_of_money(&mut self, conv: &str) -> bool {
        let Some(project) = self.convs.get(conv).and_then(|c| c.goal_fix.clone()) else { return false };
        let (today, unpaid) = (self.goal_day(), self.goals.unpaid.get(&project).copied().unwrap_or(0.0));
        match self.goals_all().map(|all| all.get(&project).map(|g| g.over_cap(today, unpaid))) {
            None => true,
            Some(Some(out)) => out,
            // No record: nothing was ever spent, unless the record is there and cannot be read.
            Some(None) => self.goals.unreadable.contains(&project) || unpaid >= goals::DAILY_CAP_USD,
        }
    }

    /// A turn Faber would have sent by itself was refused for lack of the day's money: the person is told on the latest reply
    /// (`reason.goal.cap`), and checks that asked for the fix are theirs to decide.
    pub(super) fn goal_capped(&mut self, conv: &str) {
        let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().rev().find(|m| m.role == Role::Assistant)) else {
            return;
        };
        m.halt = Some(CAP.into());
        if let Some(k) = m.checks.as_mut().filter(|k| k.state == ChecksState::Failed) {
            k.state = ChecksState::NeedsYou;
        }
        let id = m.id.clone();
        self.save(conv);
        self.push(conv, Some(&id), true);
    }

    /// The PC's day number, for the day's cap.
    fn goal_day(&self) -> i64 {
        ((self.secs() + self.utc_offset as f64) / 86_400.0).floor() as i64
    }

    /// Rewrites one project's goals, on disk and in memory, when `f` agrees. The error is a key.
    pub(super) fn change_goals<T>(
        &mut self,
        project: &str,
        f: impl FnOnce(&mut Goals) -> Result<T, &'static str>,
    ) -> Result<T, &'static str> {
        let mut projects = self.store.projects_checked().map_err(|e| e.key())?;
        let found = projects.iter_mut().find(|p| p.id == project).ok_or("error.project.notFound")?;
        // A record that is there but cannot be read is never written over with an empty one.
        let mut goals = Goals::read(found)?;
        let out = f(&mut goals)?;
        // Costs whose write failed earlier are paid with this one.
        let owed = self.goals.unpaid.get(project).copied().unwrap_or(0.0);
        if owed > 0.0 {
            goals.today(self.goal_day()).usd += owed;
        }
        goals.store(found);
        self.store.save_projects(&projects).map_err(|_| "error.store.write")?;
        self.goals.unpaid.remove(project);
        self.goals.unreadable.remove(project);
        if let Some(all) = self.goals.loaded.as_mut() {
            if goals == Goals::default() {
                all.remove(project);
            } else {
                all.insert(project.to_string(), goals);
            }
        }
        Ok(out)
    }

    /// Whether the conversation still works: a turn, its copy being made, a message waiting, its checks. One that is gone
    /// or was never opened since Faber started has ended.
    fn goal_conv_working(&self, id: &str) -> bool {
        self.copying.contains_key(id)
            || self.convs.get(id).is_some_and(|c| {
                running_item(c, self.live.get(id)).is_some() || c.messages.is_empty() || !c.queued.is_empty() || verifying::checking(c)
            })
    }

    /// Whether anything of the project is at work now.
    fn project_busy(&self, project: &str) -> bool {
        self.convs.values().any(|c| c.project_id.as_deref() == Some(project) && self.goal_conv_working(&c.id) && !c.messages.is_empty())
    }

    /// Whether the person wrote in the project in the last 15 minutes. A message Faber sent by itself carries its reason,
    /// so it is not theirs.
    fn person_there(&self, project: &str, now: f64) -> bool {
        let since = (now * 1000.0) as u64;
        self.convs
            .values()
            .filter(|c| c.project_id.as_deref() == Some(project) && c.started_by.is_none())
            .filter_map(|c| c.messages.iter().rev().find(|m| m.role == Role::User && m.wake.is_none()))
            .any(|m| since.saturating_sub(m.at) < (goals::QUIET * 1000.0) as u64)
    }

    /// No game in front, no conversation of the project at work and nothing from the person for 15 minutes. Asked when a
    /// check is due and again when a fix is about to start, since the check itself takes a while.
    fn project_quiet(&self, project: &str, now: f64) -> bool {
        self.verifier.as_ref().is_some_and(|v| !v.game_in_front()) && !self.project_busy(project) && !self.person_there(project, now)
    }

    /// Every gate that keeps a check from starting, cheapest first. None of them reads a file.
    fn goal_gates_open(&self, project: &str, now: f64) -> bool {
        self.git.is_some()
            && self.project_quiet(project, now)
            && self.vendors().into_iter().any(|v| self.allows(v, None, Purpose::UserTurn { by_hand: false }, false).is_ok())
    }

    /// The tick: runs the check of the goal that is due, if every gate is open.
    pub(super) fn goals_tick(&mut self) {
        if self.closing || self.installing || self.projects_closed {
            return;
        }
        let now = self.secs();
        let started = *self.goals.started.get_or_insert(now);
        if !self.goals_all().is_some_and(|all| all.values().any(Goals::any_on)) || !self.goals.running.is_empty() {
            return;
        }
        let mut due: Vec<(f64, String, Goal)> = vec![];
        let Some(all) = self.goals.loaded.as_ref() else { return };
        for (project, g) in all.iter().filter(|(_, g)| g.any_on()) {
            for goal in g.items.iter().filter(|i| i.state != State::Paused) {
                let fixed = goal.state == State::Broke && !goal.conversation.as_deref().is_some_and(|c| self.goal_conv_working(c));
                if goal.state == State::Broke && !fixed {
                    continue;
                }
                let at = goals::due_at(goal, started, fixed).max(self.goals.wait.get(&goal.id).copied().unwrap_or(0.0));
                if at <= now {
                    due.push((at, project.clone(), goal.clone()));
                }
            }
        }
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.2.id.cmp(&b.2.id)));
        if let Some((_, project, goal)) = due.into_iter().find(|(_, project, _)| self.goal_gates_open(project, now)) {
            self.goal_run(project, goal);
        }
    }

    /// Starts the check of `goal`. Everything that reads the project's files or asks git (its confirmed list, its local file,
    /// whether the tree is clean) happens away from the lock; the one read left here is the project's own record, for its
    /// folder. Only a clean tree is checked.
    fn goal_run(&mut self, project_id: String, goal: Goal) {
        let Some(project) = self.project_of(Some(project_id.as_str())) else { return };
        let (Some(verifier), Some(git)) = (self.verifier.clone(), self.git.clone()) else { return };
        let folder = PathBuf::from(&project.path);
        self.goals.running.insert(goal.id.clone());
        let (me, runtime, goal_id) = (self.me.clone(), self.opts.runtime.clone(), goal.id);
        let (check, read) = (goal.check, folder.clone());
        self.opts.runtime.spawn(async move {
            let prepared = tokio::task::spawn_blocking(move || {
                let view = checks::view(&project);
                // The check runs only while it is still on the list the person confirmed.
                if !view.confirmed || !view.commands.contains(&check) {
                    return Err(Ran::Unconfirmed);
                }
                let clean = git()
                    .and_then(|mut git| verify::changed(&mut *git, &read, Some("HEAD")))
                    .is_some_and(|changed| changed.files.is_empty());
                if !clean {
                    return Err(Ran::Skipped);
                }
                Ok(Request {
                    folder: read.clone(),
                    level: Some(Level::Full),
                    commands: vec![check],
                    setup: view.setup,
                    local: Local::read(&read),
                    changed: None,
                    project: Some(read),
                })
            })
            .await
            .unwrap_or(Err(Ran::Skipped));
            let ran = match prepared {
                Ok(request) => {
                    Ran::Done(verifier.start(&runtime, folder, &format!("goal:{goal_id}"), Level::Full, async move { request }).await)
                }
                Err(ran) => ran,
            };
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).goal_checked(&project_id, &goal_id, ran);
            }
        });
    }

    /// The check ended (or did not run: a dirty tree, a run stopped by a newer one, a check that could not start).
    fn goal_checked(&mut self, project: &str, goal_id: &str, ran: Ran) {
        self.goals.running.remove(goal_id);
        let now = self.secs();
        let outcome = match ran {
            Ran::Unconfirmed => {
                self.goals.wait.insert(goal_id.to_string(), now + goals::EVERY);
                return;
            }
            Ran::Skipped => None,
            Ran::Done(outcome) => Some(outcome),
        };
        let seen = outcome
            .as_ref()
            .filter(|o| !matches!(o.reason.as_deref(), Some("reason.verify.stopped" | "reason.verify.noChecks")))
            .and_then(|o| match goals::signature(o) {
                Some((sig, shown)) => Some((Check::Failed(sig), shown)),
                None if o.ok() => Some((Check::Pass, String::new())),
                None => None,
            });
        let Some((check, shown)) = seen else {
            self.goals.wait.insert(goal_id.to_string(), now + RETRY);
            return;
        };
        self.goals.wait.remove(goal_id);
        let found = self
            .goals_all()
            .and_then(|all| all.get(project))
            .and_then(|g| g.items.iter().find(|i| i.id == goal_id).map(|i| (g.auto, i.clone())));
        let Some((auto, goal)) = found else { return };
        // The person turned the switch off while the check ran (or paused the goal): the result is dropped, the goal stays as it
        // was and nobody is told.
        if !auto || goal.state == State::Paused {
            return;
        }
        let sig = match &check {
            Check::Failed(sig) => Some(sig.clone()),
            Check::Pass => None,
        };
        let update: Box<dyn FnOnce(&mut Goal)> = match goals::next(&goal, &check, now) {
            Next::Holding => Box::new(|g| {
                (g.state, g.failed, g.woke, g.conversation, g.reason) = (State::Holding, None, None, None, None);
            }),
            Next::Wait => {
                let sig = sig.clone().unwrap_or_default();
                Box::new(move |g| g.failed = Some(goals::Failure { sig, at: now }))
            }
            Next::NeedsYou(why) => Box::new(move |g| (g.state, g.failed, g.reason) = (State::NeedsYou, None, Some(why.to_string()))),
            Next::Stay => Box::new(|_| {}),
            Next::Start => match self.start_goal_conversation(project, &goal, &shown, sig.clone().unwrap_or_default()) {
                // The goal was written as Broke, with its conversation, before the conversation started.
                Ok(()) => Box::new(|_| {}),
                // Something in the way may pass: nothing changes and the check is asked again in a few minutes.
                Err(Refused::Wait) => {
                    self.goals.wait.insert(goal_id.to_string(), now + RETRY);
                    return;
                }
                // Nothing could be started: the person is told, and the next check is six hours away like any other. A goal in
                // this state starts nothing while its check still fails (`goals::next`) until the check passes or they resume it.
                Err(Refused::Needs(why)) => {
                    Box::new(move |g| (g.state, g.failed, g.reason) = (State::NeedsYou, None, Some(why.to_string())))
                }
            },
        };
        let entering = goal.state != State::NeedsYou;
        let mut needs_you = false;
        let id = goal_id.to_string();
        let saved = self
            .change_goals(project, |goals| {
                if let Some(g) = goals.items.iter_mut().find(|g| g.id == id).filter(|g| g.state != State::Paused) {
                    g.last_run = Some(now);
                    update(g);
                    needs_you = g.state == State::NeedsYou && entering;
                }
                Ok(())
            })
            .is_ok();
        // A change that could not be kept is tried again in a few minutes, not at every look.
        if !saved {
            self.goals.wait.insert(goal_id.to_string(), now + RETRY);
        }
        // The tray tells the person once, when a goal first needs them and the change was kept; the screen ignores the event. A goal with no
        // conversation yet carries its own id, never an empty one.
        if needs_you && saved {
            (self.emit)(AppEvent::Halted {
                conversation_id: goal.conversation.unwrap_or(goal.id),
                title: goal.words,
                why: "reason.goal.needsYou".into(),
                vendor: None,
            });
        }
    }

    /// Starts the conversation that reproduces and fixes a broken goal, or says why none can start now. It never works in the
    /// person's folder, never needs the person, and only starts within the project's day. The quiet guards are asked again
    /// here, since the check took a while. The start is written (counted, the goal Broke with its conversation) before the
    /// conversation is launched, and a write that fails starts nothing.
    fn start_goal_conversation(&mut self, project_id: &str, goal: &Goal, output: &str, sig: String) -> Result<(), Refused> {
        let project = self.project_of(Some(project_id)).ok_or(Refused::Needs("reason.goal.off"))?;
        let goals = Goals::of(&project);
        if !goals.auto {
            return Err(Refused::Needs("reason.goal.off"));
        }
        if self.installing || !self.project_quiet(project_id, self.secs()) {
            return Err(Refused::Wait);
        }
        if self.store.settings().permissions() != Permissions::FullAccess {
            return Err(Refused::Needs("reason.goal.mode"));
        }
        if self.git.is_none() || !project.separate_copies {
            return Err(Refused::Needs("reason.goal.copies"));
        }
        goals.room(self.goal_day(), self.goals.unpaid.get(project_id).copied().unwrap_or(0.0)).map_err(Refused::Needs)?;
        if goals.items.iter().any(|g| g.id != goal.id && g.conversation.as_deref().is_some_and(|c| self.goal_conv_working(c))) {
            return Err(Refused::Needs("reason.goal.busy"));
        }
        let asked: Vec<_> = self.vendors().into_iter().map(|v| self.allows(v, None, Purpose::UserTurn { by_hand: false }, false)).collect();
        if !asked.iter().any(Result::is_ok) {
            return Err(Refused::Needs(if !asked.is_empty() && asked.iter().all(|r| *r == Err(Why::Billing)) {
                "reason.goal.billing"
            } else {
                "reason.goal.quota"
            }));
        }
        let id = self.blank(Area::Project, Some(project_id.to_string()));
        if let Some(c) = self.convs.get_mut(&id) {
            c.title = goal.words.chars().take(60).collect();
            // The link stays on the conversation for good: the day's money holds it and what it starts after the goal's own
            // pointer is cleared or points at a newer fix.
            c.goal_fix = Some(project_id.to_string());
        }
        let (today, goal_id, conversation) = (self.goal_day(), goal.id.clone(), id.clone());
        let written = self.change_goals(project_id, |g| {
            g.today(today).started += 1;
            if let Some(item) = g.items.iter_mut().find(|i| i.id == goal_id) {
                (item.state, item.failed, item.woke, item.conversation, item.reason) =
                    (State::Broke, None, Some(sig), Some(conversation), None);
            }
            Ok(())
        });
        if written.is_err() {
            let _ = self.remove(&id);
            return Err(Refused::Needs("reason.goal.start"));
        }
        let req = SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Project,
            project_id: Some(project_id.to_string()),
            text: brief(goal, output),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "auto".into(), model: None, effort: None },
        };
        let wake = Wake { why: "wake.goalBroke".into(), pr: None, checks: vec![goal.words.clone()] };
        // Saved with its first message before it failed: it stays, with the error in its reply. Otherwise nothing is left.
        match self.launch_sibling(&id, req, Some(wake)) {
            Ok(()) => {}
            Err(_) if self.convs.get(&id).is_some_and(|c| !c.messages.is_empty()) => {}
            Err(_) => {
                let _ = self.remove(&id);
                // The start stays counted. The goal no longer points at a conversation that is gone.
                let goal_id = goal.id.clone();
                let _ = self.change_goals(project_id, |g| {
                    if let Some(item) = g.items.iter_mut().find(|i| i.id == goal_id) {
                        (item.conversation, item.woke) = (None, None);
                    }
                    Ok(())
                });
                return Err(Refused::Needs("reason.goal.start"));
            }
        }
        Ok(())
    }

    /// A cost was written for a conversation fixing a goal of `project` (or one its agent started): it counts against the
    /// project's day. A write that fails keeps the cost owed in memory, counted by the gate and paid with the next write.
    pub(super) fn goal_spent(&mut self, project: &str, usd: f64) {
        *self.goals.unpaid.entry(project.to_string()).or_default() += usd;
        let _ = self.change_goals(project, |_| Ok(()));
    }

    /// The person pressed Stop in a conversation: when it is the one fixing a goal, the goal is paused until they resume it.
    pub(super) fn goal_stopped(&mut self, conv: &str) {
        let Some(project) = self.convs.get(conv).and_then(|c| c.project_id.clone()) else { return };
        let fixing = |g: &Goals| g.goal_of(conv).is_some_and(|g| g.state == State::Broke);
        if !self.goals_all().and_then(|all| all.get(&project)).is_some_and(fixing) {
            return;
        }
        let _ = self.change_goals(&project, |g| {
            if let Some(goal) = g.items.iter_mut().find(|i| i.conversation.as_deref() == Some(conv)) {
                (goal.state, goal.reason) = (State::Paused, None);
            }
            Ok(())
        });
    }

    fn goals_view(&mut self, project_id: &str) -> Result<goals::View, String> {
        let projects = self.store.projects_checked().map_err(|e| e.key().to_string())?;
        let project = projects.into_iter().find(|p| p.id == project_id).ok_or("error.project.notFound")?;
        let g = Goals::read(&project).map_err(str::to_string)?;
        let proof = checks::view(&project);
        let today = self.goal_day();
        let goals = g
            .items
            .iter()
            .map(|i| goals::Row {
                id: i.id.clone(),
                words: i.words.clone(),
                check: i.check.clone(),
                state: match i.state {
                    State::Holding if self.goals.running.contains(&i.id) => "checking",
                    State::Holding => "holding",
                    State::Broke
                        if !self.goals.running.contains(&i.id) && i.conversation.as_deref().is_some_and(|c| self.goal_conv_working(c)) =>
                    {
                        "fixing"
                    }
                    State::Broke => "checking",
                    State::NeedsYou => "needsYou",
                    State::Paused => "paused",
                },
                reason: i.reason.clone(),
                last_run: i.last_run,
                conversation: i.conversation.clone(),
            })
            .collect();
        Ok(goals::View {
            auto: g.auto,
            cap_seen: g.cap_seen,
            cap_usd: goals::DAILY_CAP_USD,
            spent_usd: g.spent(today).usd,
            checks: if proof.confirmed { proof.commands } else { vec![] },
            goals,
        })
    }

    /// Changes one project's goals, then answers with how they stand.
    fn goals_edit(&mut self, project: &str, f: impl FnOnce(&mut Goals) -> Result<(), &'static str>) -> Result<goals::View, String> {
        self.change_goals(project, f).map_err(String::from)?;
        self.goals_view(project)
    }
}

impl Engine {
    /// A project's goals, for Settings and the project page.
    pub fn project_goals(&self, project: &str) -> Result<goals::View, String> {
        self.with(|c| c.goals_view(project))
    }

    /// The project's switch (Settings > Projects). Turning it on records that the person saw the day's limit.
    pub fn set_project_goals(&self, project: &str, on: bool) -> Result<goals::View, String> {
        self.with(|c| {
            c.goals_edit(project, |g| {
                g.auto = on;
                g.cap_seen |= on;
                Ok(())
            })
        })
    }

    /// Adds a goal made of the person's words and one of the project's confirmed checks.
    pub fn add_project_goal(&self, project: &str, words: &str, check: &str) -> Result<goals::View, String> {
        self.with(|c| {
            let found = c.store.projects_checked().map_err(|e| e.key().to_string())?.into_iter().find(|p| p.id == project);
            let proof = found.map(|p| checks::view(&p)).ok_or("error.project.notFound")?;
            let confirmed = if proof.confirmed { proof.commands } else { vec![] };
            let id = new_id();
            c.goals_edit(project, |g| g.add(id, words, check, &confirmed))
        })
    }

    /// Takes a goal off. A conversation already fixing it goes on, and stays held to the project's day through `goal_fix`.
    pub fn remove_project_goal(&self, project: &str, goal: &str) -> Result<goals::View, String> {
        self.with(|c| {
            c.goals.wait.remove(goal);
            c.goals_edit(project, |g| {
                g.items.retain(|i| i.id != goal);
                Ok(())
            })
        })
    }

    /// A goal that was paused or needs the person is checked again from scratch.
    pub fn resume_project_goal(&self, project: &str, goal: &str) -> Result<goals::View, String> {
        self.with(|c| {
            c.goals.wait.remove(goal);
            c.goals_edit(project, |g| {
                if let Some(i) = g.items.iter_mut().find(|i| i.id == goal && matches!(i.state, State::Paused | State::NeedsYou)) {
                    (i.state, i.last_run, i.failed, i.woke, i.reason) = (State::Holding, None, None, None, None);
                }
                Ok(())
            })
        })
    }

    /// One look of the goals watcher, as the 30 second loop makes it; tests call it after moving the clock.
    #[doc(hidden)]
    pub fn goals_tick(&self) {
        self.with(|c| c.goals_tick());
    }
}
