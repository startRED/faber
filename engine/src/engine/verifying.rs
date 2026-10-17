//! Faber's checks in a project conversation (`crate::verify`): the agent calls the `verify` tool, the engine reads the
//! project's checks and the changed files of the conversation's folder (its copy, R201), and answers once they ended.
//! Git runs away from the lock; the checks run on the engine's runtime.
//!
//! Faber also decides when a reply is done (R146): a project reply in full access whose steps changed files gets the fast
//! checks once it ends, shown under it. Until they end the conversation is at work: what the person writes, the queue and
//! "keep going" wait for them, and Stop ends them. A failure sends one turn by itself (`by_itself.rs`, the quiet line of
//! `wake.checksFixing`) asking the agent to fix it, at most `MAX_FIXES` in a row and only within Auto's pace, and
//! otherwise the reply says it needs the person.
use super::*;
use crate::verify::{self, Local, Outcome, Request, Verdict, Verifier};

/// The steps that change files: Claude Code's Write, Edit, MultiEdit and NotebookEdit, and Codex's file changes. A command
/// can change files too, but nothing tells which did, and the agent can still call the tool itself.
const CHANGES_FILES: [&str; 5] = ["Write", "Edit", "MultiEdit", "NotebookEdit", "fileChange"];

/// Fix turns Faber sends by itself in a row before the person decides.
pub const MAX_FIXES: usize = 2;

/// The first short failures of each check the screen keeps; the agent reads them all.
const SHOWN_LINES: usize = 3;

/// What the agent is told when the checks failed. It is for the model, so it is English like every prompt of the engine;
/// the person sees `wake.checksFixing` in their language instead.
pub const FIX: &str = "Faber ran the project's fast checks after your reply, and some failed. Fix what fails, then run Faber's \
verify tool to see them pass before you end your reply. Do not ask the person about it. What the checks said:";

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

impl Prepared {
    /// Starts the verify at once, so a stop that comes while git is asked what changed (away from any lock: it can take a
    /// second) stops it before any check starts. The future returned ends with the outcome.
    pub(super) fn start(self, runtime: &tokio::runtime::Handle) -> impl Future<Output = Outcome> + Send + 'static + use<> {
        let Prepared { verifier, owner, mut request, git, base } = self;
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
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_verify(&conv, sid, &call, outcome.ok(), outcome.text());
        });
    }

    /// The agent's turn stopped or ended: the checks it started stop with it, and their processes end with their jobs.
    /// Another conversation's checks of the same folder (one without a copy of its own) go on.
    pub(super) fn stop_verify(&self, conv: &str) {
        let Some(verifier) = self.verifier.as_ref().filter(|v| !v.idle()) else { return };
        if let Some(folder) = self.convs.get(conv).filter(|c| c.area == Area::Project).and_then(|c| self.cwd_of(c).ok()) {
            verifier.stop(&folder, conv);
        }
    }

    /// The person pressed Stop while the checks of the last reply run: they stop, the reply keeps no row, and nothing
    /// follows them, even when they ended just before and their answer is still on its way (`checks_ended` finds no row
    /// waiting).
    pub(super) fn stop_checks(&mut self, conv: &str) {
        self.stop_verify(conv);
        let Some(reply) = self.convs.get_mut(conv).filter(|c| checking(c)).and_then(|c| c.messages.last_mut()) else { return };
        reply.checks = None;
        let id = reply.id.clone();
        self.save(conv);
        self.push(conv, Some(&id), true);
    }

    /// The answer goes back only to the process that asked: a conversation that was stopped or moved on drops it.
    fn answer_verify(&self, conv: &str, sid: u64, call: &str, ok: bool, text: String) {
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(call, ok, text);
        }
    }

    /// The end of a reply that finished well (`finish`): when it is a project reply in full access whose steps changed
    /// files, Faber runs the fast checks before the turn counts as done, and true says it took the end over: the queue
    /// and "keep going" (`keep_going`, the reply to go on from) wait for `checks_ended`.
    pub(super) fn check_done(&mut self, conv: &str, reply_id: &str, keep_going: Option<String>) -> bool {
        let Some(permissions) = self.live.get(conv).map(|l| l.permissions) else { return false };
        let Some(c) = self.convs.get(conv) else { return false };
        let changed =
            |m: &Message| m.tools.iter().flatten().any(|t| CHANGES_FILES.contains(&t.name.as_str()) && t.ok != Some(false) && !t.declined);
        // The reply to a fix Faber asked for is checked whatever its steps: a command can change files too.
        let fixing = fixes_before(&c.messages, reply_id) > 0;
        if !self.offers_verify(c.area, permissions)
            || !c
                .messages
                .iter()
                .any(|m| m.id == reply_id && m.role == Role::Assistant && m.status == Some(Status::Done) && (fixing || changed(m)))
        {
            return false;
        }
        let Ok(prepared) = self.verify_request(conv, verify::Level::Fast) else { return false };
        if let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.checks = Some(Checks { state: ChecksState::Running, passed: 0, failed: vec![] });
        }
        let pending = prepared.start(&self.opts.runtime);
        let (me, conv, reply) = (self.me.clone(), conv.to_string(), reply_id.to_string());
        self.opts.runtime.spawn(async move {
            let outcome = pending.await;
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).checks_ended(&conv, &reply, keep_going, outcome);
        });
        true
    }

    /// The checks of a reply ended. Passed (or nothing to check, or stopped by a newer run of the same folder), the turn
    /// is done: the queue goes out and "keep going" goes on, also when it was turned on while they ran. Failed, Faber asks
    /// the agent to fix it by itself only where nobody else decides: the reply is still the last one, it asked the person
    /// nothing, "keep going" did not stop on it, the AI has room on Auto's pace, and fewer than `MAX_FIXES` fixes came in a
    /// row. Otherwise the reply needs the person, and "keep going" stops there. Stopped by the person, nothing follows.
    fn checks_ended(&mut self, conv: &str, reply_id: &str, keep_going: Option<String>, outcome: Outcome) {
        let Some(c) = self.convs.get(conv) else { return };
        let rounds = fixes_before(&c.messages, reply_id);
        let last = c.messages.last().is_some_and(|m| m.id == reply_id);
        let Some(m) = c.messages.iter().find(|m| m.id == reply_id) else { return };
        if m.checks.as_ref().is_none_or(|k| k.state != ChecksState::Running) {
            return;
        }
        let asked = m.tools.iter().flatten().any(|t| t.parent.is_none() && t.name == "AskUserQuestion");
        let halted = m.halt.is_some();
        let busy = self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let mut checks = settled(&outcome, rounds);
        let refused = match checks.as_ref().map(|k| k.state) {
            Some(ChecksState::Failed) if !last || busy || asked || halted => Some("reason.keepGoing.checks"),
            Some(ChecksState::Failed) => self.room_next(conv, reply_id).err(),
            Some(ChecksState::NeedsYou) => Some("reason.keepGoing.checks"),
            _ => None,
        };
        if let Some(k) = checks.as_mut().filter(|_| refused.is_some()) {
            k.state = ChecksState::NeedsYou;
        }
        self.set_checks(conv, reply_id, checks.clone());
        if !last || busy {
            return self.close_when_idle(conv);
        }
        match checks.map(|k| k.state) {
            Some(ChecksState::NeedsYou) => {
                self.halt(conv, reply_id, refused.unwrap_or("reason.keepGoing.checks"));
                self.drain(conv);
            }
            Some(ChecksState::Failed) => {
                let names = outcome.checks.iter().filter(|c| failed(c.verdict)).map(|c| c.command.clone()).collect();
                let wake = Wake { why: "wake.checksFixing".into(), pr: None, checks: names };
                // A message the person left goes out first; the fix waits behind it.
                if self.send_by_itself(conv, format!("{FIX}\n\n{}", outcome.text()), wake).is_err() {
                    // Faber could not ask (a full queue, a plan it does not spend on by itself): the person decides.
                    let mut row = checks_of(self, conv, reply_id);
                    if let Some(k) = row.as_mut() {
                        k.state = ChecksState::NeedsYou;
                    }
                    self.set_checks(conv, reply_id, row);
                    self.halt(conv, reply_id, "reason.keepGoing.checks");
                }
                self.drain(conv);
            }
            _ => {
                self.drain(conv);
                let on = self.convs.get(conv).is_some_and(|c| c.keep_going.is_some());
                match keep_going {
                    Some(reply) => self.keep_going_after(conv, Some(reply)),
                    // Turned on while the checks ran (`set_keep_going` waited for them).
                    None if on => self.go_on(conv, reply_id),
                    None => {}
                }
            }
        }
        self.close_when_idle(conv);
    }

    fn set_checks(&mut self, conv: &str, reply_id: &str, checks: Option<Checks>) {
        if let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.checks = checks;
        }
        self.save(conv);
        self.push(conv, Some(reply_id), true);
    }
}

fn checks_of(core: &Core, conv: &str, reply_id: &str) -> Option<Checks> {
    core.convs.get(conv)?.messages.iter().find(|m| m.id == reply_id)?.checks.clone()
}

/// The checks of the conversation's last reply still run: it is at work, so what the person writes, the queue and
/// "keep going" wait for them, and Stop ends them.
pub(super) fn checking(c: &Conversation) -> bool {
    c.messages.last().and_then(|m| m.checks.as_ref()).is_some_and(|k| k.state == ChecksState::Running)
}

fn failed(verdict: Verdict) -> bool {
    matches!(verdict, Verdict::Failed | Verdict::NotFinished)
}

/// The row an outcome leaves: none when nothing ran (no checks, nothing changed, a stop), passed, failed while Faber may
/// still ask for a fix, and otherwise needing the person.
fn settled(outcome: &Outcome, rounds: usize) -> Option<Checks> {
    if outcome.reason.is_some() {
        return None;
    }
    let passed = outcome.checks.iter().filter(|c| matches!(c.verdict, Verdict::Passed | Verdict::Unchanged)).count() as u32;
    let failed: Vec<FailedCheck> = outcome
        .checks
        .iter()
        .filter(|c| failed(c.verdict))
        .map(|c| FailedCheck { command: c.command.clone(), lines: c.failures.iter().take(SHOWN_LINES).cloned().collect() })
        .collect();
    let state = match (failed.is_empty(), rounds < MAX_FIXES) {
        (true, _) => ChecksState::Passed,
        (false, true) => ChecksState::Failed,
        (false, false) => ChecksState::NeedsYou,
    };
    Some(Checks { state, passed, failed })
}

/// How many fix turns Faber sent in a row right before this reply: its own messages asking for a fix, counted back
/// until a message of the person's (or any other turn Faber sent).
fn fixes_before(messages: &[Message], reply_id: &str) -> usize {
    let Some(at) = messages.iter().position(|m| m.id == reply_id) else { return 0 };
    messages[..at]
        .iter()
        .rev()
        .filter(|m| m.role == Role::User)
        .take_while(|m| m.wake.as_ref().is_some_and(|w| w.why == "wake.checksFixing"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::{CheckResult, Level};

    fn outcome(verdicts: &[(&str, Verdict)]) -> Outcome {
        Outcome {
            level: Level::Fast,
            checks: verdicts
                .iter()
                .map(|(command, verdict)| CheckResult {
                    command: command.to_string(),
                    verdict: *verdict,
                    millis: 0,
                    failures: (0..5).map(|n| format!("failure {n}\n")).collect(),
                    summary: String::new(),
                })
                .collect(),
            millis: 0,
            log: None,
            reason: None,
        }
    }

    #[test]
    fn the_row_says_passed_failed_or_needs_you_and_keeps_three_failures_a_check() {
        let mixed = outcome(&[("npm run lint", Verdict::Failed), ("cargo fmt", Verdict::Unchanged), ("npm test", Verdict::Passed)]);
        let row = settled(&mixed, 0).unwrap();
        assert_eq!((row.state, row.passed), (ChecksState::Failed, 2));
        assert_eq!(row.failed.len(), 1);
        assert_eq!(row.failed[0].lines.len(), SHOWN_LINES);
        assert_eq!(settled(&mixed, MAX_FIXES - 1).unwrap().state, ChecksState::Failed);
        assert_eq!(settled(&mixed, MAX_FIXES).unwrap().state, ChecksState::NeedsYou);
        assert_eq!(settled(&outcome(&[("a", Verdict::LeftOut), ("b", Verdict::Passed)]), 2).unwrap().state, ChecksState::Passed);
        let mut stopped = outcome(&[]);
        stopped.reason = Some("reason.verify.stopped".into());
        assert_eq!(settled(&stopped, 0), None, "nothing ran, so no row");
    }

    #[test]
    fn only_faber_s_fix_turns_in_a_row_count() {
        let user = |id: &str, why: Option<&str>| {
            let mut m = Message::new(id.into(), Role::User, String::new(), 0);
            m.wake = why.map(Wake::of);
            m
        };
        let reply = |id: &str| Message::new(id.into(), Role::Assistant, String::new(), 0);
        let fix = Some("wake.checksFixing");
        let messages = vec![
            user("u1", None),
            reply("r1"),
            user("u2", fix),
            reply("r2"),
            user("u3", fix),
            reply("r3"),
            user("u4", Some("wake.keepGoing")),
            reply("r4"),
        ];
        assert_eq!(fixes_before(&messages, "r1"), 0);
        assert_eq!(fixes_before(&messages, "r2"), 1);
        assert_eq!(fixes_before(&messages, "r3"), 2);
        assert_eq!(fixes_before(&messages, "r4"), 0, "a turn of keep going starts again");
    }
}
