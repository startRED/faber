//! A reply that goes silent (rule 5 of `docs/product/models.md`). The CLI says something every few seconds while it works, so a
//! reply whose CLI has said nothing for `BOUND`, with no step running and nobody to answer, is hung: Faber stops it like the
//! person's Stop does (the interrupt, then its end after the driver's grace), and the reply ends with `reason.silent`.
//! A step of the agent's own that runs (a command, a tool server's call) is not cut, since its process may still be at work
//! and Faber cannot see it end, but it no longer hides the silence either (task 110): once it has said nothing for longer
//! than its kind of work allows (`stuck::QUIET`, `stuck::QUIET_COMMAND` while a command runs), the reply says so, with Stop,
//! until the CLI speaks again. A subagent at work has its own signs (`stuck_watch.rs`).
//! Cost: one map entry per running reply, written on events that already arrive, and one task that sleeps `EVERY` between two
//! looks, alive only while a reply runs. No process and no timer per reply.
use super::*;

/// What the reply says on screen when Faber stopped it for its silence.
pub(super) const KEY: &str = "reason.silent";
/// The same when the CLI did not confirm that it stopped (a Codex thread that ignored the interrupt keeps its shared process).
const UNCONFIRMED: &str = "reason.silent.unconfirmed";

/// What a reply Faber cut for its silence says when the driver ended it with `error`.
pub(super) fn said(error: &str) -> &'static str {
    if error == "error.cli.stopUnconfirmed" { UNCONFIRMED } else { KEY }
}

/// Seconds without a word from the CLI after which a reply is stopped: 15 minutes. The longest whole turn in the recordings
/// (`tests/fixtures`) took 35 s, a subagent with nothing running shows as stuck after 10 minutes (R174, `stuck::QUIET`) and
/// Claude Code's longest command timeout is 10 minutes, so thinking, a long message and a retrying request stay under it. A
/// stopped answer costs more than a flag on a subagent's row, hence the larger number. A guess against short recordings: the
/// longest gap between two events of a healthy long run has not been measured yet (R396).
pub(super) const BOUND: f64 = 900.0;

/// How often the check looks again while a reply runs; the bound is counted in minutes.
const EVERY: Duration = Duration::from_secs(30);

/// The last word of a running reply's CLI.
pub(super) struct Heard {
    turn: String,
    /// The process that runs the reply: one that replaced it is another reply's.
    sid: u64,
    at: f64,
    /// Faber stopped it for its silence; what follows says so.
    cut: bool,
    /// Whole minutes a step of its own has run without a word, once past its kind's limit: the reply says so.
    quiet: Option<u32>,
}

/// What a running reply waits for, when it is not the CLI's words.
enum Wait {
    /// The person, a compaction, or the process closing after a finished answer: never silence.
    Other,
    /// A subagent of its own, which has its own signs: its steps are the reply's words.
    Subagent,
    /// Steps of the agent's own: never cut, but told once they have said nothing for long; `command` when one runs a command.
    Steps { command: bool },
}

impl Core {
    /// A reply started on the live process: its silence counts from now, and the check looks while it runs.
    pub(super) fn listen(&mut self, conv: &str) {
        let Some((turn, sid)) = self.live.get(conv).and_then(|l| Some((l.turn.clone()?, l.sid))) else { return };
        let at = self.secs();
        self.heard.insert(conv.to_string(), Heard { turn, sid, at, cut: false, quiet: None });
        if std::mem::replace(&mut self.listening, true) {
            return;
        }
        self.looked = at;
        let me = self.me.clone();
        self.opts.runtime.spawn(async move {
            loop {
                tokio::time::sleep(EVERY).await;
                let Some(core) = me.upgrade() else { return };
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if !core.check_silence() {
                    core.listening = false;
                    return;
                }
            }
        });
    }

    /// The CLI of the live process said something: the reply is not silent.
    pub(super) fn hear(&mut self, conv: &str, sid: u64) {
        let at = self.secs();
        if let Some(h) = self.heard.get_mut(conv).filter(|h| h.sid == sid && !h.cut) {
            h.at = at;
            if h.quiet.take().is_some() {
                self.show_stall(conv);
            }
        }
    }

    /// The minutes a step of this running reply has gone without a word, once past its limit.
    pub(super) fn quiet_for(&self, conv: &str, turn: &str) -> Option<u32> {
        self.heard.get(conv).filter(|h| h.turn == turn && !h.cut).and_then(|h| h.quiet)
    }

    /// Whether Faber stopped this conversation's running reply for its silence.
    pub(super) fn silenced(&self, conv: &str) -> bool {
        let turn = self.live.get(conv).and_then(|l| l.turn.as_ref());
        self.heard.get(conv).is_some_and(|h| h.cut && Some(&h.turn) == turn)
    }

    /// What a reply waits for other than the CLI's words, if anything: a question for the person, a compaction, an answer
    /// already finished that only waits for the process to close, a subagent, or steps of its own (a build, a tool server).
    fn waits(&self, conv: &str, turn: &str) -> Option<Wait> {
        let Some(c) = self.convs.get(conv) else { return Some(Wait::Other) };
        let Some(m) = c.messages.iter().find(|m| m.id == turn) else { return Some(Wait::Other) };
        if c.compacting || m.status != Some(Status::Streaming) || m.finished_at.is_some() || m.permissions.iter().flatten().next().is_some()
        {
            return Some(Wait::Other);
        }
        let mut open = m.tools.iter().flatten().filter(|t| t.ok.is_none()).peekable();
        open.peek()?;
        let mut command = false;
        for t in open {
            if t.parent.is_some() || matches!(t.name.as_str(), "Agent" | "Task" | "Helper") {
                return Some(Wait::Subagent);
            }
            command |= crate::stuck::is_command(&t.name);
        }
        Some(Wait::Steps { command })
    }

    /// Looks at every running reply once; false when none is left to look at.
    pub(super) fn check_silence(&mut self) -> bool {
        let now = self.secs();
        // The clock jumped (the PC slept): nothing could be heard meanwhile, so no reply was silent for that long.
        let gap = now - std::mem::replace(&mut self.looked, now);
        if gap > 4.0 * EVERY.as_secs_f64() {
            self.heard.values_mut().for_each(|h| h.at += gap);
        }
        let live = &self.live;
        self.heard.retain(|conv, h| live.get(conv).is_some_and(|l| l.sid == h.sid && l.turn.as_ref() == Some(&h.turn)));
        let mut looked = vec![];
        for (conv, h) in &self.heard {
            if h.cut {
                continue;
            }
            let quiet = now - h.at;
            let next = match self.waits(conv, &h.turn) {
                // What it waits for is not silence: the count starts again when that ends.
                Some(Wait::Other | Wait::Subagent) => Look::Again,
                Some(Wait::Steps { command }) => {
                    let limit = if command { crate::stuck::QUIET_COMMAND } else { crate::stuck::QUIET };
                    Look::Quiet((quiet >= limit).then(|| (quiet / 60.0) as u32))
                }
                None if quiet >= BOUND => Look::Cut,
                None => Look::Quiet(None),
            };
            looked.push((conv.clone(), next));
        }
        for (conv, next) in looked {
            let Some(h) = self.heard.get_mut(&conv) else { continue };
            let quiet = match next {
                Look::Again => {
                    h.at = now;
                    None
                }
                Look::Quiet(minutes) => minutes,
                Look::Cut => {
                    self.go_silent(&conv);
                    continue;
                }
            };
            if std::mem::replace(&mut h.quiet, quiet) != quiet {
                self.show_stall(&conv);
            }
        }
        !self.heard.is_empty()
    }

    /// Stops the CLI as the person's Stop would, but leaves the conversation's own state alone: nobody pressed Stop. The reply
    /// says why when it ends (`Done` not ok, or the driver's fatal error), never while it may still finish.
    fn go_silent(&mut self, conv: &str) {
        if let Some(h) = self.heard.get_mut(conv) {
            h.cut = true;
        }
        self.interrupt(conv, false);
    }
}

/// What one look decided for a reply.
enum Look {
    /// It waits for something that is not silence: its count starts again.
    Again,
    /// Its steps have said nothing for this many minutes past their limit, or it is within its limits.
    Quiet(Option<u32>),
    /// Silent for the bound with nothing running: Faber stops it.
    Cut,
}

impl Engine {
    /// Looks at every running reply for silence now, as the timer does every half minute; tests call it after moving the clock.
    pub fn check_silence(&self) {
        self.with(|c| {
            c.check_silence();
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::answer::tests::{engine, seed};
    use super::*;
    use crate::events::SessionCommand;
    use serde_json::json;

    /// A running reply on a session that counts the interrupts it gets, and the clock the test moves.
    fn running() -> (Engine, String, Arc<Mutex<u32>>, Arc<Mutex<f64>>, crate::workspace::TestRoot) {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let stops = Arc::new(Mutex::new(0));
        let counted = stops.clone();
        let session = Session::from_fn(move |cmd| {
            if matches!(cmd, SessionCommand::Interrupt) {
                *counted.lock().unwrap() += 1;
            }
        });
        let id = seed(&engine, session, &[("t1", &[])]);
        let clock = Arc::new(Mutex::new(1000.0));
        let read = clock.clone();
        engine.set_time_source(Arc::new(move || *read.lock().unwrap()));
        engine.with(|c| c.listen(&id));
        (engine, id, stops, clock, dir)
    }

    /// The timer's looks from the clock's present to `secs` after the start, one every half minute, the last one exactly there.
    fn walk(engine: &Engine, clock: &Mutex<f64>, secs: f64) {
        let step = EVERY.as_secs_f64();
        let mut now = *clock.lock().unwrap();
        let end = 1000.0 + secs;
        while now < end {
            now = (now + step).min(end);
            *clock.lock().unwrap() = now;
            engine.check_silence();
        }
    }

    fn say(engine: &Engine, id: &str, event: EngineEvent) {
        engine.with(|c| c.handle(id, 1, event));
    }

    fn done(ok: bool) -> EngineEvent {
        EngineEvent::Done {
            ok,
            reason: if ok { "success" } else { "interrupted" }.into(),
            cost_total_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        }
    }

    fn last(engine: &Engine, id: &str) -> Message {
        engine.open(id).unwrap().messages.pop().unwrap()
    }

    fn stops(n: &Arc<Mutex<u32>>) -> u32 {
        *n.lock().unwrap()
    }

    #[tokio::test]
    async fn a_reply_with_no_word_from_its_cli_for_the_bound_is_interrupted_once_and_says_why() {
        let (engine, id, n, clock, _dir) = running();
        walk(&engine, &clock, BOUND - 1.0);
        assert_eq!(stops(&n), 0, "a quarter of an hour less a second is still thinking");
        walk(&engine, &clock, BOUND);
        engine.check_silence();
        assert_eq!(stops(&n), 1, "stopped once, not once per look");
        assert_eq!(last(&engine, &id).error, None, "it may still finish: nothing is said yet");
        // The driver gave up waiting for the interrupt: the fatal error names the silence, not the timeout.
        say(&engine, &id, EngineEvent::Error { message: "error.cli.interruptTimeout".into(), fatal: true });
        let m = last(&engine, &id);
        assert_eq!((m.status, m.error.as_deref()), (Some(Status::Error), Some("reason.silent")));
        engine.shutdown();
    }

    // Nobody pressed Stop: the person who was away still gets the end card and the tray notice, and Keep going sees a failure.
    #[tokio::test]
    async fn faber_stopping_a_silent_reply_is_not_the_persons_stop() {
        let (engine, id, n, clock, _dir) = running();
        engine.with(|c| c.convs.get_mut(&id).unwrap().send_after_stop = Some("later".into()));
        walk(&engine, &clock, BOUND);
        assert_eq!(stops(&n), 1);
        let held = engine.open(&id).unwrap();
        assert!(!held.stopped, "no one stopped it");
        assert_eq!(held.send_after_stop.as_deref(), Some("later"), "what waits for the end still waits");
        say(&engine, &id, EngineEvent::Error { message: "error.cli.interruptTimeout".into(), fatal: true });
        let ended = engine.open(&id).unwrap();
        assert!(!ended.stopped);
        assert!(ended.ended.is_some(), "the end is marked, so the card and the tray tell the person");
        // The person's own Stop still marks the conversation, as before.
        let (engine, id, _, _, _dir) = running();
        engine.interrupt(&id);
        assert!(engine.open(&id).unwrap().stopped);
        engine.shutdown();
    }

    #[tokio::test]
    async fn an_answer_that_finishes_in_the_grace_says_nothing_and_one_that_stops_says_why() {
        let (engine, id, _, clock, _dir) = running();
        walk(&engine, &clock, BOUND);
        say(&engine, &id, done(true));
        let m = last(&engine, &id);
        assert_eq!((m.status, m.error), (Some(Status::Done), None), "it finished after all");
        engine.shutdown();

        let (engine, id, _, clock, _dir) = running();
        walk(&engine, &clock, BOUND);
        say(&engine, &id, done(false));
        let m = last(&engine, &id);
        assert_eq!((m.status, m.error.as_deref()), (Some(Status::Stopped), Some("reason.silent")));
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_thread_that_does_not_confirm_the_stop_says_so() {
        let (engine, id, _, clock, _dir) = running();
        walk(&engine, &clock, BOUND);
        say(&engine, &id, EngineEvent::Error { message: "error.cli.stopUnconfirmed".into(), fatal: true });
        assert_eq!(last(&engine, &id).error.as_deref(), Some("reason.silent.unconfirmed"));
        engine.shutdown();
    }

    #[tokio::test]
    async fn any_word_from_the_cli_starts_the_count_again() {
        let (engine, id, n, clock, _dir) = running();
        walk(&engine, &clock, BOUND - 1.0);
        say(&engine, &id, EngineEvent::Text("still here".into()));
        walk(&engine, &clock, BOUND + 10.0);
        assert_eq!(stops(&n), 0);
        walk(&engine, &clock, 2.0 * BOUND - 1.0);
        assert_eq!(stops(&n), 1, "the new count ran out too");
        engine.shutdown();
    }

    fn stall(engine: &Engine, id: &str) -> Option<(StallKind, Option<u32>)> {
        last(engine, id).stall.map(|s| (s.kind, s.minutes))
    }

    #[tokio::test]
    async fn a_step_that_runs_and_a_question_that_waits_are_never_silence() {
        let (engine, id, n, clock, _dir) = running();
        let tool =
            |id: &str| EngineEvent::Tool { id: id.into(), name: "Bash".into(), input: json!({"command": "cargo build"}), parent: None };
        say(&engine, &id, tool("build"));
        walk(&engine, &clock, 5.0 * BOUND);
        assert_eq!(stops(&n), 0, "a build the CLI reports as running");
        say(&engine, &id, EngineEvent::ToolResult { id: "build".into(), ok: true, declined: false, text: String::new() });
        assert_eq!(stall(&engine, &id), None, "a word from the CLI takes the notice away");
        walk(&engine, &clock, 5.0 * BOUND + BOUND - 1.0);
        assert_eq!(stops(&n), 0, "the count starts when the step ends");
        say(&engine, &id, EngineEvent::Permission { request_id: "q".into(), tool: "Write".into(), input: json!({"file_path": "a"}) });
        walk(&engine, &clock, 20.0 * BOUND);
        assert_eq!(stops(&n), 0, "a question the person has not answered");
        engine.shutdown();
    }

    // Task 110: an open step used to start the count again at every look, so a command or a tool server that never answers
    // kept the reply "working" for ever with nothing said. Its silence now shows, with Stop, and nothing is cut.
    #[tokio::test]
    async fn a_step_that_says_nothing_past_its_limit_is_told_with_its_minutes_and_never_cut() {
        let (engine, id, n, clock, _dir) = running();
        say(&engine, &id, EngineEvent::Tool { id: "q".into(), name: "mcp__db__query".into(), input: json!({}), parent: None });
        walk(&engine, &clock, crate::stuck::QUIET - 1.0);
        assert_eq!(stall(&engine, &id), None, "within the limit of a step that is not a command");
        walk(&engine, &clock, 12.0 * 60.0);
        assert_eq!(stall(&engine, &id), Some((StallKind::Quiet, Some(12))), "twelve minutes without news");
        walk(&engine, &clock, 10.0 * BOUND);
        assert_eq!(stops(&n), 0, "silence alone never stops a step whose end Faber cannot see");
        assert_eq!(stall(&engine, &id), Some((StallKind::Quiet, Some(150))));
        say(&engine, &id, EngineEvent::Text("done querying".into()));
        assert_eq!(stall(&engine, &id), None);
        engine.shutdown();

        // A command is given longer, since builds and test suites say nothing until they end.
        let (engine, id, _, clock, _dir) = running();
        say(
            &engine,
            &id,
            EngineEvent::Tool { id: "b".into(), name: "Bash".into(), input: json!({"command": "cargo build"}), parent: None },
        );
        say(&engine, &id, EngineEvent::Tool { id: "r".into(), name: "Read".into(), input: json!({"file_path": "a"}), parent: None });
        walk(&engine, &clock, crate::stuck::QUIET + 60.0);
        assert_eq!(stall(&engine, &id), None, "a command runs beside it");
        walk(&engine, &clock, crate::stuck::QUIET_COMMAND);
        assert_eq!(stall(&engine, &id), Some((StallKind::Quiet, Some(30))));
        engine.shutdown();

        // A subagent at work has signs of its own on its row: the reply adds none.
        let (engine, id, n, clock, _dir) = running();
        say(&engine, &id, EngineEvent::Tool { id: "a".into(), name: "Agent".into(), input: json!({"prompt": "look"}), parent: None });
        walk(&engine, &clock, 4.0 * BOUND);
        assert_eq!((stops(&n), stall(&engine, &id)), (0, None));
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_finished_answer_waiting_for_its_process_and_a_compaction_are_never_silence() {
        let (engine, id, n, clock, _dir) = running();
        engine.with(|c| c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().finished_at = Some(1));
        walk(&engine, &clock, 3.0 * BOUND);
        assert_eq!(stops(&n), 0, "the answer is finished and only waits for its process to close");
        engine.with(|c| {
            let conv = c.convs.get_mut(&id).unwrap();
            conv.messages.last_mut().unwrap().finished_at = None;
            conv.compacting = true;
        });
        walk(&engine, &clock, 6.0 * BOUND);
        assert_eq!(stops(&n), 0, "the context is being compacted");
        engine.with(|c| {
            let conv = c.convs.get_mut(&id).unwrap();
            conv.compacting = false;
            conv.messages.last_mut().unwrap().status = Some(Status::Done);
        });
        walk(&engine, &clock, 9.0 * BOUND);
        assert_eq!(stops(&n), 0, "a reply that is already Done");
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_reply_that_ended_or_moved_to_another_process_is_not_looked_at_any_more() {
        let (engine, id, n, clock, _dir) = running();
        engine.with(|c| c.live.get_mut(&id).unwrap().turn = None);
        walk(&engine, &clock, 3.0 * BOUND);
        assert_eq!(stops(&n), 0);
        assert!(engine.with(|c| c.heard.is_empty()), "nothing is kept for it");
        engine.shutdown();

        let (engine, id, n, clock, _dir) = running();
        engine.with(|c| c.live.get_mut(&id).unwrap().sid = 2);
        walk(&engine, &clock, 3.0 * BOUND);
        assert_eq!(stops(&n), 0, "the new process's reply counts from its own start");
        assert!(engine.with(|c| c.heard.is_empty()));
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_reply_that_starts_again_after_a_cut_is_counted_afresh() {
        let (engine, id, n, clock, _dir) = running();
        walk(&engine, &clock, BOUND);
        assert!(engine.with(|c| c.silenced(&id)));
        engine.with(|c| c.listen(&id));
        assert!(!engine.with(|c| c.silenced(&id)), "a new count replaces the cut one");
        walk(&engine, &clock, BOUND + 60.0);
        assert_eq!(stops(&n), 1, "the new count has not run out");
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_jump_of_the_clock_is_not_silence() {
        let (engine, id, n, clock, _dir) = running();
        walk(&engine, &clock, EVERY.as_secs_f64());
        // The PC slept for two hours between two looks.
        *clock.lock().unwrap() += 7200.0;
        engine.check_silence();
        assert_eq!(stops(&n), 0, "nothing could be heard while the PC slept");
        assert!(!engine.with(|c| c.silenced(&id)));
        // The count goes on from the wake-up: a reply that stays silent is still cut after the bound.
        let woke = *clock.lock().unwrap() - 1000.0;
        walk(&engine, &clock, woke + BOUND);
        assert_eq!(stops(&n), 1);
        engine.shutdown();
    }
}
