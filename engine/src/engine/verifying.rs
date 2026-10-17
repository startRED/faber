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
/// to ask git, and the branch its copy came from.
pub(super) struct Prepared {
    verifier: Arc<Verifier>,
    request: Request,
    git: Option<GitRunner>,
    base: Option<String>,
}

impl Prepared {
    /// Starts the verify at once, so a stop that comes while git is asked what changed (away from any lock: it can take a
    /// second) stops it before any check starts. The future returned ends with the outcome.
    pub(super) fn start(self, runtime: &tokio::runtime::Handle) -> impl Future<Output = Outcome> + Send + 'static + use<> {
        let Prepared { verifier, mut request, git, base } = self;
        let (folder, level) = (request.folder.clone(), request.level.unwrap_or(verify::Level::Fast));
        let asked = folder.clone();
        let made = async move {
            request.changed = tokio::task::spawn_blocking(move || {
                let mut exec = git.and_then(|run| run())?;
                verify::changed(&mut *exec, &asked, base.as_deref())
            })
            .await
            .ok()
            .flatten();
            request
        };
        verifier.start(runtime, folder, level, made)
    }
}

impl Core {
    /// The agent of a project conversation in full access gets the tool, where the shell gave a verifier. The checks run
    /// code the agent may have written, so only a mode in which it already runs commands without asking may start them.
    pub(super) fn offers_verify(&self, area: Area, permissions: Permissions) -> bool {
        area == Area::Project && permissions == Permissions::FullAccess && self.verifier.is_some()
    }

    pub(super) fn verify_request(&self, conv: &str, level: verify::Level) -> Result<Prepared, &'static str> {
        let verifier = self.verifier.clone().ok_or("Faber's checks are not available here")?;
        let c = self.convs.get(conv).ok_or("the conversation is gone")?;
        let project =
            self.store.projects().into_iter().find(|p| Some(&p.id) == c.project_id.as_ref()).ok_or("this conversation has no project")?;
        let folder = self.cwd_of(c).map_err(|_| "the conversation's folder cannot be used")?;
        // The project's own checks (R137), as declared in this folder and confirmed by the person when they were.
        let view = crate::project::mission::view(&Project { path: folder.to_string_lossy().into_owned(), ..project.clone() });
        let request = Request {
            folder,
            level: Some(level),
            commands: view.commands,
            setup: view.setup,
            local: Local::read(Path::new(&project.path)),
            changed: None,
        };
        let base = c.copy.as_ref().and_then(|copy| copy.base.clone());
        Ok(Prepared { verifier, request, git: self.git.clone(), base })
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
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_verify(&conv, sid, &call, outcome.ok(), outcome.text());
        });
    }

    /// The agent's turn stopped or ended: the checks of its folder stop with it, and their processes end with their jobs.
    pub(super) fn stop_verify(&self, conv: &str) {
        let Some(verifier) = self.verifier.as_ref().filter(|v| !v.idle()) else { return };
        if let Some(folder) = self.convs.get(conv).filter(|c| c.area == Area::Project).and_then(|c| self.cwd_of(c).ok()) {
            verifier.stop(&folder);
        }
    }

    /// The answer goes back only to the process that asked: a conversation that was stopped or moved on drops it.
    fn answer_verify(&self, conv: &str, sid: u64, call: &str, ok: bool, text: String) {
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(call, ok, text);
        }
    }
}
