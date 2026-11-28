//! A turn that ends while a background command it started still runs (a render, a build, a test run) has not handed the work
//! back to the person: the agent ended its turn to wait, as its instructions tell it, and the command's end is what it waits
//! for. Recorded 2026-11-12 (bench run v11, an explainer film): the agent started the draft render in the background, closed
//! with "the draft render is still running" and a `needs_you` box, and the reply read "the answer waits for the person"
//! with no video made. Without Keep going (`keep_going.rs` has its own wait), such a reply now waits for its command: its row
//! stays in the running list ("waiting for its command to finish"), it does not count as needing the person, its lead gets
//! no report yet, and its process is not closed under the command. When the last command ends, Claude Code wakes its agent
//! itself (`own_turn`) and Faber's own turn is only the net under it; for a CLI that does not (Codex) Faber sends the turn.
//! A command that never ends (a dev server) holds it for `COMMAND_LIMIT` at most; then the reply ends as it said.
//! It belongs to the reply, even when its driver retires before its commands are known to have ended. Never saved.

use super::*;
use crate::model::{Outcome, Wake};

/// The longest a reply waits for its background command before it ends as its closing box said (seconds).
pub(super) const COMMAND_LIMIT: f64 = 1800.0;

/// The reply, its original command deadline, and the fallback while no command runs.
pub(super) struct CommandWait {
    id: u64,
    reply: String,
    pub(super) until: f64,
    grace: Option<CommandGrace>,
    limit_passed: bool,
}

struct CommandGrace {
    id: u64,
    task: tokio::task::AbortHandle,
}

impl Drop for CommandGrace {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The exact wait a confirmed driver exit may release once retirement removes that driver.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum CommandWaitKey {
    KeepGoing(u64),
    Reply(u64),
}

impl Core {
    pub(super) fn waiting_command(&self, conv: &str) -> Option<&CommandWait> {
        let wait = self.command_waits.get(conv)?;
        let c = self.convs.get(conv)?;
        (!c.stopped
            && c.archived_at.is_none()
            && !wait::has_open_permission(c)
            && c.messages.last().is_some_and(|m| m.id == wait.reply && m.status == Some(Status::Done))
            && self.live.get(conv).is_none_or(|l| l.turn.is_none()))
        .then_some(wait)
    }

    pub(super) fn command_wait_key(&self, conv: &str) -> Option<CommandWaitKey> {
        self.keep_going_command_wait(conv)
            .filter(|w| w.command_limit.is_none())
            .map(|w| CommandWaitKey::KeepGoing(w.timer_id))
            .or_else(|| self.waiting_command(conv).filter(|w| w.grace.is_none()).map(|w| CommandWaitKey::Reply(w.id)))
    }

    /// The end of a turn (`finish`): the reply waits for a background command of the conversation that still runs, unless
    /// its closing box leaves nothing undone or says it is blocked, Keep going already waits for it, or the person put the
    /// conversation away.
    pub(super) fn arm_command_wait(&mut self, conv: &str, reply_id: Option<&str>) {
        // An idle driver failure ends no reply and must leave its existing wait in place.
        let Some(reply_id) = reply_id else { return };
        self.command_waits.remove(conv);
        let Some(c) = self.convs.get(conv) else { return };
        if !self.background.running_in(conv)
            || c.keep_going.is_some()
            || c.stopped
            || c.archived_at.is_some()
            || wait::has_open_permission(c)
        {
            return;
        }
        let Some(m) = c.messages.last().filter(|m| m.id == reply_id && m.status == Some(Status::Done)) else { return };
        // Only a reply that says work is left waits: no closing box yet, or one with `not_done` that is not blocked. A box with
        // nothing left (a finished job, a question only the person can answer) is theirs even while a dev server runs.
        let left = |w: &WrapUp| w.summary.as_ref().is_some_and(|s| !s.not_done.trim().is_empty());
        if m.wrap_up.as_ref().is_some_and(|w| w.outcome() == Some(Outcome::Blocked) || !left(w)) {
            return;
        }
        let secs = COMMAND_LIMIT * self.keep_pace;
        let until = self.secs() + secs;
        self.next_wait_timer += 1;
        let id = self.next_wait_timer;
        self.command_waits.insert(conv.into(), CommandWait { id, reply: reply_id.into(), until, grace: None, limit_passed: false });
        self.after(secs, conv, id, Self::command_wait_over);
    }

    /// A late start cancels the old fallback without giving the reply any more time to wait for commands.
    pub(super) fn command_wait_started(&mut self, conv: &str) {
        if !self.background.running_in(conv) || self.waiting_command(conv).is_none_or(|w| w.grace.is_none()) {
            return;
        }
        let Some(w) = self.command_waits.get_mut(conv) else { return };
        w.grace = None;
        if w.limit_passed {
            let id = w.id;
            self.command_wait_over(conv, id);
        }
    }

    /// The last background command of the conversation ended (`command_ended`): the reply that waited for it goes on, by the
    /// CLI's own turn or, a moment later, by Faber's.
    pub(super) fn command_wait_ended(&mut self, conv: &str, native: bool) {
        if self.background.running_in(conv) || self.waiting_command(conv).is_none_or(|w| w.grace.is_some()) {
            return;
        }
        self.next_wait_timer += 1;
        let id = self.next_wait_timer;
        let grace = if native { keep_going::COMMAND_OWN } else { keep_going::COMMAND_GRACE };
        let task = self.after(grace * self.keep_pace, conv, id, Self::command_wait_wake);
        self.command_waits.get_mut(conv).unwrap().grace = Some(CommandGrace { id, task });
    }

    /// The command ended and the CLI did not turn to it by itself: Faber sends the turn, as the CLI would have.
    fn command_wait_wake(&mut self, conv: &str, grace_id: u64) {
        let Some(wait) = self.command_waits.get(conv).filter(|w| w.grace.as_ref().is_some_and(|g| g.id == grace_id)) else { return };
        if self.closing || !self.take_command_wait(conv, wait.id) {
            return;
        }
        // An idle replacement can also exit during the grace period, before its failure reaches the engine.
        if self.live.get(conv).is_some_and(|l| self.session_exit_confirmed(conv, l.sid))
            && let Some(live) = self.retire(conv)
        {
            live.session.close();
        }
        if !self.bench.off(BenchPart::FollowUps)
            && let Some(c) = self.convs.get(conv)
        {
            let sent = self.send_by_itself(conv, by_itself::command_wake_text(c).into(), Wake::of("wake.commandDone"));
            if sent.is_ok() {
                return;
            }
        }
        self.command_wait_closed(conv);
    }

    /// The command did not end in time: the reply ends as its closing box said.
    fn command_wait_over(&mut self, conv: &str, id: u64) {
        // A command that ended in time still gets its turn, unless another start reaches us while we wait for it.
        if let Some(w) = self.command_waits.get_mut(conv).filter(|w| w.id == id && w.grace.is_some()) {
            w.limit_passed = true;
            return;
        }
        if self.take_command_wait(conv, id) {
            self.command_wait_closed(conv);
        }
    }

    /// Takes this timer's wait off; only acts on it if it still belongs to the latest reply and no turn runs.
    fn take_command_wait(&mut self, conv: &str, id: u64) -> bool {
        if self.command_waits.get(conv).is_none_or(|w| w.id != id) {
            return false;
        }
        let current = self.waiting_command(conv).is_some();
        self.command_waits.remove(conv);
        current
    }

    /// The reply stops waiting: the running list, the needs-you mark and the lead's report read it again.
    fn command_wait_closed(&mut self, conv: &str) {
        let reply = self.convs.get(conv).and_then(|c| c.messages.last()).map(|m| m.id.clone());
        self.refresh_running();
        self.push(conv, reply.as_deref(), true);
        self.list_changed();
        self.turn_ended(conv);
    }

    fn after(&self, secs: f64, conv: &str, id: u64, then: fn(&mut Core, &str, u64)) -> tokio::task::AbortHandle {
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts
            .runtime
            .spawn(async move {
                tokio::time::sleep(Duration::from_secs_f64(secs.max(0.0))).await;
                if let Some(core) = me.upgrade() {
                    then(&mut core.lock().unwrap_or_else(|e| e.into_inner()), &conv, id);
                }
            })
            .abort_handle()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::background::{State as Run, Update};
    use crate::events::Session;

    pub(in crate::engine) const WAITING: &str = "The draft render is still running.\n<|faber-wrap-up|>\n{\"outcome\":\"needs_you\",\"did\":\"Mixed the audio\",\"result\":\"The mix is done\",\"not_done\":\"Draft render running\",\"verified\":\"Loudness\",\"ask\":\"Should I continue to the final film?\",\"actions\":[\"Continue\",\"Wait\"]}\n<|/faber-wrap-up|>";
    const DONE: &str = "The film is rendered.\n<|faber-wrap-up|>\n{\"outcome\":\"done\",\"did\":\"Rendered\",\"result\":\"video.mp4\",\"verified\":\"Watched it\"}\n<|/faber-wrap-up|>";

    fn done() -> EngineEvent {
        EngineEvent::Done {
            ok: true,
            reason: String::new(),
            cost_total_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        }
    }

    fn render(id: &str) -> EngineEvent {
        EngineEvent::Background(Update::Start { id: id.into(), command: "hyperframes render .".into(), stoppable: true, turn_ended: false })
    }

    fn ended(id: &str) -> EngineEvent {
        EngineEvent::Background(Update::End { id: id.into(), state: Run::Done })
    }

    /// A project turn that starts a background render, then ends with `text` while the render still runs.
    pub(in crate::engine) fn turn_ends_with_render(engine: &Engine, vendor: Vendor, text: &str) -> String {
        let conv = super::super::answer::tests::seed(engine, Session::from_fn(|_| {}), &[("r1", &[])]);
        engine.with_saves(false, |core| {
            let conversation = core.convs.get_mut(&conv).unwrap();
            conversation.area = Area::Project;
            conversation.project_id = Some("test-project".into());
            let l = core.live.get_mut(&conv).unwrap();
            l.vendor = vendor;
            // The session runs the CLI Faber has for it, so the end of the turn keeps it.
            let cli = l.cli.clone();
            core.opts.clis.insert(vendor, cli);
            core.handle(&conv, 1, render("render"));
            core.handle(&conv, 1, EngineEvent::Text(text.into()));
            core.handle(&conv, 1, done());
        });
        conv
    }

    fn needs_you(engine: &Engine, conv: &str) -> bool {
        engine.open(conv).unwrap().messages.last().unwrap().needs_you
    }

    fn row(engine: &Engine, conv: &str) -> Option<RunningItem> {
        engine.state().running.items.into_iter().find(|i| i.conversation_id == conv)
    }

    #[tokio::test]
    async fn a_turn_that_ends_while_its_render_runs_waits_for_it_not_for_the_person() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        assert!(!needs_you(&engine, &conv), "the render is what the reply waits for: bench v11 read it as waiting for the person");
        let item = row(&engine, &conv).expect("the conversation is still at work");
        assert_eq!((item.state, item.keep_wait.map(|w| w.why)), (RunState::Working, Some("command".into())));
        engine.with(|core| {
            assert!(!core.session_idle(&conv), "the process that runs the render and will wake the agent stays");
            assert!(!core.can_request_conclusion(&conv), "it concludes after the render, not now");
        });

        // Claude Code wakes its agent itself when the render ends: that turn continues the reply's work.
        engine.with(|core| {
            core.handle(&conv, 1, ended("render"));
            core.handle(&conv, 1, EngineEvent::OwnTurn);
        });
        let c = engine.open(&conv).unwrap();
        assert_eq!(c.messages[c.messages.len() - 2].wake.as_ref().map(|w| w.why.as_str()), Some("wake.commandDone"));
        assert!(c.messages[c.messages.len() - 2].text.contains("command_results"));
        assert_eq!(c.messages.last().unwrap().status, Some(Status::Streaming), "the agent goes on with the film");
        engine.shutdown();
    }

    #[tokio::test]
    async fn losing_a_driver_does_not_turn_a_command_wait_into_a_request_for_the_person() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        let before = row(&engine, &conv).and_then(|item| item.keep_wait).expect("the reply is waiting for a command");
        engine.with(|core| {
            core.handle(&conv, 1, EngineEvent::Error { message: "error.driverStopped".into(), fatal: true });
            assert!(!core.live.contains_key(&conv), "the dead driver still has to be removed");
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: false });
            assert!(core.background.running_for(1), "the command needs confirmed exit");
            assert!(!core.can_request_conclusion(&conv));
        });
        let after = row(&engine, &conv).and_then(|item| item.keep_wait);
        assert_eq!(after, Some(before), "a reply still waiting for the command is not handed back to the person");
        assert!(!needs_you(&engine, &conv));
        engine.shutdown();
    }

    #[tokio::test]
    async fn stop_discards_a_command_wait_even_if_exit_arrives_later() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Codex, WAITING);
        let wait = engine.with(|core| core.waiting_command(&conv).unwrap().id);
        engine.with(|core| {
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            assert!(core.waiting_command(&conv).unwrap().grace.is_none(), "the idle driver has not retired");
        });
        engine.interrupt(&conv);
        engine.with(|core| {
            core.handle(&conv, 1, EngineEvent::Error { message: crate::codex::SHARED_SERVICE_STOPPED.into(), fatal: true });
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            core.command_wait_over(&conv, wait);
            assert!(!core.command_waits.contains_key(&conv));
            let c = &core.convs[&conv];
            assert_eq!(c.messages.last().unwrap().id, "r1");
            assert!(c.queued.is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_pending_plan_decision_takes_precedence_over_a_command_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Codex, WAITING);
        engine.with(|core| {
            // Codex can finish the reply while leaving its plan approval card open.
            core.convs.get_mut(&conv).unwrap().messages.last_mut().unwrap().permissions = Some(vec![Permission {
                request_id: "approve".into(),
                tool: "ExitPlanMode".into(),
                input: serde_json::Value::Null,
                epoch: None,
            }]);
            core.arm_command_wait(&conv, Some("r1"));
            assert!(!core.command_waits.contains_key(&conv), "a running command cannot replace the person's plan decision");
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            core.handle(&conv, 1, EngineEvent::Error { message: crate::codex::SHARED_SERVICE_STOPPED.into(), fatal: true });
            let c = &core.convs[&conv];
            assert!(wait::has_open_permission(c));
            assert_eq!(c.messages.last().unwrap().id, "r1");
            assert!(c.queued.is_empty());
        });
        assert!(needs_you(&engine, &conv));
        engine.shutdown();
    }

    #[tokio::test]
    async fn an_old_exit_and_timer_cannot_release_a_new_reply_wait_with_the_same_deadline() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|core| core.clock = Some(Arc::new(|| 1000.0)));
        let conv = turn_ends_with_render(&engine, Vendor::Codex, WAITING);
        engine.with(|core| {
            let (first, until) = {
                let w = core.waiting_command(&conv).unwrap();
                (w.id, w.until)
            };
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            core.background.update(
                &conv,
                2,
                Vendor::Codex,
                &Session::from_fn(|_| {}),
                Update::Start { id: "new".into(), command: "render final".into(), stoppable: false, turn_ended: true },
                now(),
            );
            core.convs.get_mut(&conv).unwrap().messages.last_mut().unwrap().id = "r2".into();
            core.arm_command_wait(&conv, Some("r2"));
            let second = core.waiting_command(&conv).unwrap().id;
            assert_ne!(first, second);
            core.handle(&conv, 1, EngineEvent::Error { message: crate::codex::SHARED_SERVICE_STOPPED.into(), fatal: true });
            core.command_wait_over(&conv, first);
            core.command_wait_wake(&conv, first);
            let w = core.waiting_command(&conv).unwrap();
            assert_eq!((w.id, w.until, w.grace.is_none()), (second, until, true));
            assert!(core.convs[&conv].queued.is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn the_last_command_can_release_its_reply_wait_after_an_idle_replacement() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with(|core| {
            core.convs.get_mut(&conv).unwrap().area = Area::Chat;
            core.handle(&conv, 1, ended("render"));
            core.handle(&conv, 1, late_render());
            let mut replacement = core.retire(&conv).unwrap();
            replacement.sid = 2;
            replacement.prepared = true;
            core.live.insert(conv.clone(), replacement);
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            assert_eq!(core.live[&conv].sid, 2);
            let grace = core.waiting_command(&conv).unwrap().grace.as_ref().expect("the idle replacement can serve the same reply").id;
            assert_eq!(core.background.recent(&conv, None, 1)[0].state, None);

            core.handle(&conv, 2, EngineEvent::WriterExit { confirmed: true });
            core.command_wait_wake(&conv, grace);
            assert_ne!(core.live.get(&conv).map(|l| l.sid), Some(2), "a second confirmed exit cannot receive the wake");
        });
        engine.shutdown();
    }

    fn late_render() -> EngineEvent {
        EngineEvent::Background(Update::Start {
            id: "late".into(),
            command: "render the final scene".into(),
            stoppable: true,
            turn_ended: true,
        })
    }

    #[tokio::test(start_paused = true)]
    async fn a_late_command_reopens_the_reply_wait_before_its_fallback() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        let (limit, cancelled) = engine.with(|core| {
            core.handle(&conv, 1, ended("render"));
            let wait = core.waiting_command(&conv).unwrap();
            let limit = wait.id;
            let first = wait.grace.as_ref().unwrap();
            let (first, task) = (first.id, first.task.clone());
            core.handle(&conv, 1, late_render());
            assert!(core.background.running_in(&conv));
            core.command_wait_wake(&conv, first);
            assert!(core.waiting_command(&conv).is_some(), "the fallback must still wait for the late command");
            assert!(core.convs[&conv].queued.is_empty());
            (limit, task)
        });
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert!(cancelled.is_finished(), "the unused timer is gone long before its grace expires");
        engine.with(|core| {
            core.command_wait_over(&conv, limit);
            assert!(core.waiting_command(&conv).is_none(), "the original limit still releases the reply");
            assert!(core.background.running_in(&conv), "the limit does not stop the command itself");
        });
        assert!(needs_you(&engine, &conv));
        engine.shutdown();
    }

    #[tokio::test]
    async fn the_latest_command_end_gets_its_own_fallback_grace() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with(|core| {
            core.convs.get_mut(&conv).unwrap().area = Area::Chat;
            core.handle(&conv, 1, ended("render"));
            let first = core.waiting_command(&conv).unwrap().grace.as_ref().unwrap().id;
            core.handle(&conv, 1, late_render());
            core.handle(&conv, 1, ended("late"));
            core.command_wait_wake(&conv, first);
            assert!(core.waiting_command(&conv).is_some(), "the old timer cannot use up the latest command's grace");
            assert!(core.convs[&conv].queued.is_empty());

            let latest = core.waiting_command(&conv).unwrap().grace.as_ref().unwrap().id;
            core.command_wait_wake(&conv, latest);
            core.command_wait_wake(&conv, latest);
            assert!(core.waiting_command(&conv).is_none());
            let c = &core.convs[&conv];
            let wakes = c.messages.iter().filter_map(|m| m.wake.as_ref()).chain(c.queued.iter().filter_map(|q| q.wake.as_ref()));
            assert_eq!(wakes.filter(|w| w.why == "wake.commandDone").count(), 1);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_late_start_cannot_extend_a_replys_expired_command_limit() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with(|core| {
            let hard_limit = core.waiting_command(&conv).unwrap().id;
            core.handle(&conv, 1, ended("render"));
            core.command_wait_over(&conv, hard_limit);
            assert!(core.waiting_command(&conv).is_some(), "the command finished in time");
            core.handle(&conv, 1, late_render());
            assert!(core.background.running_in(&conv));
            assert!(core.waiting_command(&conv).is_none(), "the original hard limit already passed");
            assert!(core.convs[&conv].queued.is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn when_the_cli_does_not_wake_its_agent_faber_sends_the_turn() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.set_keep_going_pace(0.001);
        let conv = turn_ends_with_render(&engine, Vendor::Codex, WAITING);
        engine.with(|core| {
            // No project folder behind the stub: the turn Faber sends goes out as a Chat turn would.
            core.convs.get_mut(&conv).unwrap().area = Area::Chat;
            core.handle(&conv, 1, ended("render"));
        });
        let sent = |engine: &Engine| {
            let c = engine.open(&conv).unwrap();
            let wake = |w: &Option<Wake>| w.as_ref().is_some_and(|w| w.why == "wake.commandDone");
            c.messages.iter().any(|m| wake(&m.wake)) || c.queued.iter().any(|q| wake(&q.wake))
        };
        for _ in 0..200 {
            if sent(&engine) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(sent(&engine), "Faber turns to the ended render for a CLI that does not");
        engine.shutdown();
    }

    #[tokio::test]
    async fn keep_going_wakes_early_when_the_last_command_loses_its_process() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        engine.with(|core| {
            assert!(core.background.running_for(1), "the live driver owns the first running command");
            core.command_waits.remove(&conv);
            core.convs.get_mut(&conv).unwrap().keep_going = Some(KeepGoing::default());
            // A retired session can still own another command for this conversation.
            core.background.update(
                &conv,
                2,
                Vendor::Claude,
                &Session::from_fn(|_| {}),
                Update::Start { id: "retired".into(), command: "render audio".into(), stoppable: false, turn_ended: true },
                now(),
            );
            core.arm(&conv, "r1", 900.0, "command");
            let wait = |core: &Core| core.convs[&conv].keep_going.as_ref().unwrap().waiting.as_ref().unwrap().until;
            let original = wait(core);
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: false });
            assert_eq!(wait(core), original, "uncertain process exit does not release the wait");
            core.handle(&conv, 1, EngineEvent::Error { message: "error.driverStopped".into(), fatal: true });
            assert!(!core.live.contains_key(&conv), "the failed driver cannot send its own turn");
            assert_eq!(wait(core), original, "the error alone cannot prove the command ended");
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            assert!(!core.background.running_for(1) && core.background.running_for(2));
            assert_eq!(wait(core), original, "another session's command is still running");
            core.handle(&conv, 2, EngineEvent::WriterExit { confirmed: true });
            let [result] = core.background.recent(&conv, None, 1).try_into().unwrap_or_else(|_| panic!("one result"));
            assert_eq!(result.state, None, "process exit does not prove the command succeeded or failed");
            let shortened = wait(core);
            assert!(shortened < original, "a confirmed driver loss releases the long wait");
            assert!(shortened <= core.secs() + keep_going::COMMAND_GRACE);
            core.handle(&conv, 2, EngineEvent::WriterExit { confirmed: true });
            assert_eq!(wait(core), shortened, "a duplicate exit cannot reset the new wait");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn keep_going_keeps_the_early_command_wake_when_codex_confirms_exit_before_error() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Codex, WAITING);
        engine.with(|core| {
            assert!(core.background.running_for(1));
            core.command_waits.remove(&conv);
            core.convs.get_mut(&conv).unwrap().keep_going = Some(KeepGoing::default());
            core.arm(&conv, "r1", 900.0, "command");
            let wait = |core: &Core| core.convs[&conv].keep_going.as_ref().unwrap().waiting.as_ref().unwrap().until;
            let original = wait(core);

            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            assert_eq!(wait(core), original, "the old session is still registered until the failure is handled");
            core.handle(&conv, 1, EngineEvent::Error { message: crate::codex::SHARED_SERVICE_STOPPED.into(), fatal: true });
            assert!(!core.live.contains_key(&conv));
            assert_eq!(core.background.recent(&conv, None, 1)[0].state, None);
            let shortened = wait(core);
            assert!(shortened < original, "the shared service reports confirmed exit before its failure");
            assert!(shortened <= core.secs() + keep_going::COMMAND_GRACE);
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            assert_eq!(wait(core), shortened, "the teardown's duplicate receipt cannot rearm the wake");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn the_exiting_command_driver_cannot_shorten_a_replacements_wait() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Codex, WAITING);
        engine.with(|core| {
            let mut replacement = core.retire(&conv).unwrap();
            replacement.sid = 2;
            core.command_waits.remove(&conv);
            core.live.insert(conv.clone(), replacement);
            core.convs.get_mut(&conv).unwrap().keep_going = Some(KeepGoing::default());
            core.arm(&conv, "r1", 900.0, "command");
            let wait = |core: &Core| core.convs[&conv].keep_going.as_ref().unwrap().waiting.as_ref().unwrap().until;
            let original = wait(core);
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            core.handle(&conv, 1, EngineEvent::Error { message: crate::codex::SHARED_SERVICE_STOPPED.into(), fatal: true });
            assert_eq!(core.live[&conv].sid, 2);
            assert_eq!(wait(core), original);
            assert!(!core.background.running_for(1));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_confirmed_codex_exit_does_not_shorten_a_changed_wait_at_retirement() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Codex, WAITING);
        engine.with(|core| {
            core.command_waits.remove(&conv);
            core.convs.get_mut(&conv).unwrap().keep_going = Some(KeepGoing::default());
            core.clock = Some(Arc::new(|| 1000.0));
            core.arm(&conv, "r1", 900.0, "command");
            let original = core.keep_going_command_wait(&conv).unwrap().clone();
            core.handle(&conv, 1, EngineEvent::WriterExit { confirmed: true });
            core.arm(&conv, "r1", 900.0, "command");
            let changed = core.keep_going_command_wait(&conv).unwrap().clone();
            assert_eq!(changed.until, original.until);
            assert_ne!(changed.timer_id, original.timer_id);
            core.handle(&conv, 1, EngineEvent::Error { message: crate::codex::SHARED_SERVICE_STOPPED.into(), fatal: true });
            assert_eq!(core.keep_going_command_wait(&conv), Some(&changed));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_reply_that_said_done_or_whose_command_never_ends_is_not_held() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, DONE);
        assert!(row(&engine, &conv).is_none(), "a dev server left running does not keep a finished request at work");
        const CHOICE: &str = "Two voices are ready.
<|faber-wrap-up|>
{\"outcome\":\"needs_you\",\"did\":\"Drafted voices\",\"result\":\"Both ready\",\"verified\":\"Listened\",\"ask\":\"Which voice?\",\"actions\":[\"Warm\",\"Bright\"]}
<|/faber-wrap-up|>";
        let conv = turn_ends_with_render(&engine, Vendor::Claude, CHOICE);
        assert!(needs_you(&engine, &conv), "a question with nothing left undone reaches the person at once");
        let left = DONE.replace("\"verified\"", "\"not_done\":\"Review the draft render\",\"verified\"");
        let conv = turn_ends_with_render(&engine, Vendor::Claude, &left);
        assert!(row(&engine, &conv).is_some_and(|i| i.keep_wait.is_some()), "done with work left waits for the render");

        engine.set_keep_going_pace(0.0001);
        let conv = turn_ends_with_render(&engine, Vendor::Claude, WAITING);
        assert!(!needs_you(&engine, &conv));
        for _ in 0..200 {
            if needs_you(&engine, &conv) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(needs_you(&engine, &conv), "past the limit the reply ends as its closing box said");
        assert!(row(&engine, &conv).is_none_or(|i| i.keep_wait.is_none()));
        engine.shutdown();
    }
}
