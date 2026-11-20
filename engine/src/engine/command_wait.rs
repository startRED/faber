//! A turn that ends while a background command it started still runs (a render, a build, a test run) has not handed the work
//! back to the person: the agent ended its turn to wait, as its instructions tell it, and the command's end is what it waits
//! for. Recorded 2026-11-12 (bench run v11, an explainer film): the agent started the draft render in the background, closed
//! with "the draft render is still running" and a `needs_you` box, and the reply read "the answer waits for the person"
//! with no video made. Without Keep going (`keep_going.rs` has its own wait), such a reply now waits for its command: its row
//! stays in the running list ("waiting for its command to finish"), it does not count as needing the person, its lead gets
//! no report yet, and its process is not closed under the command. When the last command ends, Claude Code wakes its agent
//! itself (`own_turn`) and Faber's own turn is only the net under it; for a CLI that does not (Codex) Faber sends the turn.
//! A command that never ends (a dev server) holds it for `COMMAND_LIMIT` at most; then the reply ends as it said.
//! It lives on the process (`Live`), never on disk: a restart ends the CLI and its commands with it.

use super::*;
use crate::model::{KeepWait, Outcome, Wake};

/// The longest a reply waits for its background command before it ends as its closing box said (seconds).
pub(super) const COMMAND_LIMIT: f64 = 1800.0;

/// What the reply waits for: the reply, the time (`Core::secs`) the wait gives up, and whether its command already ended
/// (then only the turn that follows it is awaited, and the limit no longer ends it).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct CommandWait {
    pub reply: String,
    pub until: f64,
    pub ended: bool,
}

/// The wait of the conversation's latest reply for its command, as its running row shows it. The person's Stop or archive
/// ends it: they are there.
pub(super) fn waiting_command(c: &Conversation, live: Option<&Live>) -> Option<KeepWait> {
    let w = live.filter(|l| l.turn.is_none())?.command_wait.as_ref()?;
    if c.stopped || c.archived_at.is_some() {
        return None;
    }
    let reply = c.messages.last()?;
    (reply.id == w.reply && reply.status != Some(Status::Streaming)).then(|| KeepWait { why: "command".into(), until: w.until })
}

impl Core {
    /// The end of a turn (`finish`): the reply waits for a background command of the conversation that still runs, unless
    /// its closing box leaves nothing undone or says it is blocked, Keep going already waits for it, or the person put the
    /// conversation away.
    pub(super) fn arm_command_wait(&mut self, conv: &str, reply_id: Option<&str>) {
        let Some(l) = self.live.get_mut(conv) else { return };
        l.command_wait = None;
        let (Some(reply_id), Some(c)) = (reply_id, self.convs.get(conv)) else { return };
        if !self.background.running_in(conv) || c.keep_going.is_some() || c.stopped || c.archived_at.is_some() {
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
        if let Some(l) = self.live.get_mut(conv) {
            l.command_wait = Some(CommandWait { reply: reply_id.into(), until, ended: false });
        }
        self.after(secs, conv, until, Self::command_wait_over);
    }

    /// The last background command of the conversation ended (`command_ended`): the reply that waited for it goes on, by the
    /// CLI's own turn or, a moment later, by Faber's.
    pub(super) fn command_wait_ended(&mut self, conv: &str) {
        if self.background.running_in(conv) {
            return;
        }
        let Some(l) = self.live.get_mut(conv).filter(|l| l.turn.is_none()) else { return };
        let Some(w) = l.command_wait.as_mut() else { return };
        w.ended = true;
        let until = w.until;
        let grace = if l.vendor == Vendor::Claude { keep_going::COMMAND_OWN } else { keep_going::COMMAND_GRACE };
        self.after(grace * self.keep_pace, conv, until, Self::command_wait_wake);
    }

    /// The command ended and the CLI did not turn to it by itself: Faber sends the turn, as the CLI would have.
    fn command_wait_wake(&mut self, conv: &str, until: f64) {
        let Some(w) = self.take_command_wait(conv, until) else { return };
        if self.convs.get(conv).is_some_and(|c| !c.stopped && c.archived_at.is_none() && c.messages.last().is_some_and(|m| m.id == w.reply))
        {
            let sent = self.send_by_itself(conv, by_itself::OWN_TURN.into(), Wake::of("wake.commandDone"));
            if sent.is_ok() {
                return;
            }
        }
        self.command_wait_closed(conv);
    }

    /// The command did not end in time: the reply ends as its closing box said.
    fn command_wait_over(&mut self, conv: &str, until: f64) {
        // Its command ended just before: the turn after it is on its way.
        if self.live.get(conv).and_then(|l| l.command_wait.as_ref()).is_some_and(|w| w.ended) {
            return;
        }
        if self.take_command_wait(conv, until).is_some() {
            self.command_wait_closed(conv);
        }
    }

    /// The wait armed at `until`, taken off when it is still the one in place and no turn runs.
    fn take_command_wait(&mut self, conv: &str, until: f64) -> Option<CommandWait> {
        let l = self.live.get_mut(conv).filter(|l| l.turn.is_none())?;
        if l.command_wait.as_ref().is_none_or(|w| w.until != until) {
            return None;
        }
        l.command_wait.take()
    }

    /// The reply stops waiting: the running list, the needs-you mark and the lead's report read it again.
    fn command_wait_closed(&mut self, conv: &str) {
        let reply = self.convs.get(conv).and_then(|c| c.messages.last()).map(|m| m.id.clone());
        self.refresh_running();
        self.push(conv, reply.as_deref(), true);
        self.list_changed();
        self.turn_ended(conv);
    }

    fn after(&self, secs: f64, conv: &str, until: f64, then: fn(&mut Core, &str, f64)) {
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(secs.max(0.0))).await;
            if let Some(core) = me.upgrade() {
                then(&mut core.lock().unwrap_or_else(|e| e.into_inner()), &conv, until);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::{State as Run, Update};
    use crate::events::Session;

    const WAITING: &str = "The draft render is still running.\n<|faber-wrap-up|>\n{\"outcome\":\"needs_you\",\"did\":\"Mixed the audio\",\"result\":\"The mix is done\",\"not_done\":\"Draft render running\",\"verified\":\"Loudness\",\"ask\":\"Should I continue to the final film?\",\"actions\":[\"Continue\",\"Wait\"]}\n<|/faber-wrap-up|>";
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
    fn turn_ends_with_render(engine: &Engine, vendor: Vendor, text: &str) -> String {
        let conv = super::super::answer::tests::seed(engine, Session::from_fn(|_| {}), &[("r1", &[])]);
        engine.with_saves(false, |core| {
            core.convs.get_mut(&conv).unwrap().area = Area::Project;
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
        assert_eq!(c.messages.last().unwrap().status, Some(Status::Streaming), "the agent goes on with the film");
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
