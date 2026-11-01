//! Faber's checks in a project conversation (`crate::verify`): the agent calls the `verify` tool, the engine reads the
//! project's checks and the changed files of the conversation's folder (its copy, R201), and answers once they ended.
//! Git runs away from the lock; the checks run on the engine's runtime.
use super::*;
use crate::verify::{self, Local, Outcome, Request, Verifier};

impl Engine {
    /// Gives the engine the verifier; without one the tool is not offered.
    pub fn set_verifier(&self, verifier: Arc<Verifier>) {
        self.with(|c| c.verifier = Some(verifier));
    }
}

/// What a verify of a conversation needs, read under the lock: the request (folder, checks, the PC's environment), how
/// to ask git, the branch its copy came from, and the conversation it is for (only its own stop ends it).
pub(super) struct Prepared {
    verifier: Arc<Verifier>,
    owner: String,
    request: Request,
    git: Option<GitRunner>,
    base: Option<String>,
}

/// The changed files of `folder`, asked of git on a blocking thread.
async fn changed_now(git: Option<GitRunner>, folder: PathBuf, base: Option<String>) -> Option<verify::Changed> {
    tokio::task::spawn_blocking(move || {
        let mut exec = git.and_then(|run| run())?;
        verify::changed(&mut *exec, &folder, base.as_deref())
    })
    .await
    .ok()
    .flatten()
}

impl Prepared {
    /// Starts the verify at once, so a stop that comes while git is asked what changed (away from any lock: it can take a
    /// second) stops it before any check starts. The future returned ends with the outcome.
    pub(super) fn start(self, runtime: &tokio::runtime::Handle) -> impl Future<Output = Outcome> + Send + 'static + use<> {
        let Prepared { verifier, owner, mut request, git, base } = self;
        let (folder, level) = (request.folder.clone(), request.level.unwrap_or(verify::Level::Fast));
        let made = async move {
            request.changed = changed_now(git, request.folder.clone(), base).await;
            request
        };
        verifier.start(runtime, folder, &owner, level, made)
    }
}

impl Core {
    /// The agent of a project conversation in full access gets the tool, where the shell gave a verifier. The checks run
    /// code the agent may have written, so only a mode in which it already runs commands without asking may start them.
    pub(super) fn offers_verify(&self, area: Area, permissions: Permissions) -> bool {
        area == Area::Project && permissions == Permissions::FullAccess && self.verifier.is_some()
    }

    pub(super) fn verify_request(&self, conv: &str, level: verify::Level) -> Result<Prepared, &'static str> {
        if self.rewind_blocked(conv) {
            return Err("error.rewind.busy");
        }
        let verifier = self.verifier.clone().ok_or("Faber's checks are not available here")?;
        let c = self.convs.get(conv).ok_or("the conversation is gone")?;
        let project =
            self.store.projects().into_iter().find(|p| Some(&p.id) == c.project_id.as_ref()).ok_or("this conversation has no project")?;
        let folder = self.cwd_of(c).map_err(|_| "the conversation's folder cannot be used")?;
        // The project's own checks (R137), as declared in this folder and confirmed by the person when they were.
        let view = crate::project::checks::view_in(&project, &folder);
        let request = Request {
            folder,
            level: Some(level),
            commands: view.commands,
            setup: view.setup,
            local: Local::read(Path::new(&project.path)),
            changed: None,
            project: Some(PathBuf::from(&project.path)),
        };
        let base = c.copy.as_ref().and_then(|copy| copy.base.clone());
        Ok(Prepared { verifier, owner: conv.to_string(), request, git: self.git.clone(), base })
    }

    /// The agent called the checks. A call that cannot run is answered at once with why; so is one from a process that no
    /// longer runs in full access.
    pub(super) fn verify_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(l) = self.live.get(conv) else { return };
        let (sid, permissions) = (l.sid, l.permissions);
        let area = self.convs.get(conv).map_or(Area::Chat, |c| c.area);
        let prepared = verify::level(&input).and_then(|level| {
            if !self.offers_verify(area, permissions) {
                return Err("Faber's checks run only in full access");
            }
            self.verify_request(conv, level)
        });
        let prepared = match prepared {
            Ok(prepared) => prepared,
            Err(why) => return self.answer_verify(conv, sid, &call, false, format!("No check ran: {why}.")),
        };
        let pending = prepared.start(&self.opts.runtime);
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts.runtime.spawn(async move {
            let outcome = pending.await;
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            core.answer_verify(&conv, sid, &call, outcome.ok(), outcome.text());
            core.rewind_checks_ended(&conv);
        });
    }

    /// The agent asked for the project's map: its folder is read away from the lock, and the answer goes back to the
    /// process that asked. It is offered where the checks are.
    pub(super) fn map_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(l) = self.live.get(conv) else { return };
        let (sid, permissions) = (l.sid, l.permissions);
        let Some(c) = self.convs.get(conv) else { return };
        if !self.offers_verify(c.area, permissions) {
            return self.answer_verify(conv, sid, &call, false, "Faber's project map is offered only in full access.".into());
        }
        let Ok(folder) = self.cwd_of(c) else {
            return self.answer_verify(conv, sid, &call, false, "The conversation's folder cannot be read.".into());
        };
        let (me, conv) = (self.me.clone(), conv.to_string());
        let read = self.opts.runtime.spawn_blocking(move || crate::map::answer(&crate::map::scan(&folder), &input));
        self.opts.runtime.spawn(async move {
            let (ok, text) = read.await.unwrap_or_else(|_| (false, "Faber could not read the folder.".into()));
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_verify(&conv, sid, &call, ok, text);
        });
    }

    /// The agent's turn stopped or ended: the checks it started stop with it, and their processes end with their jobs.
    /// Another conversation's checks of the same folder (one without a copy of its own) go on.
    pub(super) fn stop_verify(&mut self, conv: &str) {
        let Some(verifier) = self.verifier.as_ref().filter(|v| !v.idle()) else { return };
        if let Some(folder) = self.convs.get(conv).filter(|c| c.area == Area::Project).and_then(|c| self.cwd_of(c).ok())
            && verifier.stop(&folder, conv)
        {
            self.rewind_checks_stopping(conv);
        }
    }

    /// The answer goes back only to the process that asked: a conversation that was stopped or moved on drops it.
    fn answer_verify(&self, conv: &str, sid: u64, call: &str, ok: bool, text: String) {
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(call, ok, text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn turn_end_reports_once_with_or_without_legacy_checks_and_after_reloading() {
        for checks in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = super::super::answer::tests::engine(&dir);
            let parent = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("parent", &[])]);
            let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
            engine.with(|core| {
                core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
                let c = core.convs.get_mut(&child).unwrap();
                c.area = Area::Project;
                c.started_by = Some(parent.clone());
                let reply = c.messages.last_mut().unwrap();
                reply.status = Some(Status::Done);
                reply.text = "The requested answer.".into();
                reply.wrap_up = Some(
                    crate::progress::wrap_up(&serde_json::json!({
                        "outcome": "done", "did": "Answered the request.", "result": "Answer ready.", "verified": "Read the source."
                    }))
                    .unwrap(),
                );
                if checks {
                    reply.checks = Some(Checks { state: ChecksState::Running, passed: 0, failed: vec![] });
                }
                core.end_turn(&child, Some("child".into()), None);
                core.turn_ended(&child);
                core.tell_parent(&child);
                let report_id = format!("report-{child}-child");
                let saved = core.store.get(&parent).unwrap();
                assert_eq!(saved.queued.iter().filter(|q| q.id == report_id).count(), 1);
                assert!(saved.queued.last().unwrap().text.contains("concluded"), "{}", saved.queued.last().unwrap().text);
                assert_eq!(core.live[&child].parent_reported.as_deref(), Some("child"));
                let mut next = Message::new("next-child".into(), Role::Assistant, "Another answer.".into(), now());
                next.status = Some(Status::Done);
                next.wrap_up = core.convs[&child].messages.last().unwrap().wrap_up.clone();
                core.convs.get_mut(&child).unwrap().messages.push(next);
                core.save(&child);
                core.turn_ended(&child);
                core.turn_ended(&child);
                assert_eq!(core.store.get(&parent).unwrap().queued.len(), 2, "a distinct reply gets its own receipt on the same session");
                core.live.remove(&child);
                core.convs.remove(&child);
                core.load(&child);
                core.turn_ended(&child);
                assert_eq!(core.store.get(&parent).unwrap().queued.len(), 2, "saved receipt survives retiring or restarting");
            });
            engine.shutdown();
        }
    }
}
