//! "Keep going" (R199): the person tells a project conversation to go on while they are away, and each time a turn ends
//! Faber sends the next one by itself (`by_itself.rs`), until the agent asks something or says nothing is left, two turns
//! in a row fail, Auto's quota pacing says that AI's room is used up, or the cap of turns is reached. Then it stops and
//! says why on the reply, and the tray tells the person (R92). It reacts to the end of a turn and never polls.

use super::*;
use crate::budget::Why;
use crate::model::{KeepGoing, Wake};

/// What the agent is told at each turn Faber sends. It is for the model, so it is English like every prompt of the engine;
/// the person sees `wake.keepGoing` in their language instead. It names no project's own tools: the project's
/// instructions say where its task list is.
pub const CONTINUE: &str = "The person is away and asked you to keep going on your own. Continue with the next item of your plan, \
or take the next task from the project's task list as its instructions describe, do it, and check it the way the project says. \
Do not stop to propose work or to ask for a go-ahead: decide what is yours to decide. If only the person can decide something, \
ask it with your question tool when you have one. When nothing is left to do, or nothing can go on until the person decides, \
say so in one short line and end your reply with this line alone: [keep going: stop]";

/// The line the agent ends its reply with when nothing is left. Faber takes it off the answer.
pub const STOP: &str = "[keep going: stop]";

/// At most this many turns Faber sends by itself before it stops and lets the person look.
pub const MAX_TURNS: u32 = 40;

/// Turns in a row that may end in an error before it stops.
pub const MAX_FAILED: u32 = 2;

/// The longest it waits for quota to come back (a week and a day), so a reset it reads wrongly never holds it forever.
pub const MAX_WAIT_SECS: f64 = 8.0 * 24.0 * 3600.0;

/// What follows the end of a turn while "keep going" is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Continue,
    Halt(&'static str),
}

/// The end of one turn, as the decision reads it.
#[derive(Debug, Clone, Copy)]
pub struct Ended {
    /// The reply finished (`Done`), rather than with an error or a stop.
    pub ok: bool,
    /// The agent asked the person a question during the turn.
    pub asked: bool,
    /// The reply ended with [`STOP`].
    pub said_stop: bool,
}

/// The decision, without the engine: `room` is Auto's pacing for the AI that answers next (`Err` holds the reason it says).
pub fn next(kg: &KeepGoing, ended: Ended, room: Result<(), &'static str>) -> Next {
    if ended.ok && ended.asked {
        return Next::Halt("reason.keepGoing.asked");
    }
    if ended.ok && ended.said_stop {
        return Next::Halt("reason.keepGoing.done");
    }
    if !ended.ok && kg.failed + 1 >= MAX_FAILED {
        return Next::Halt("reason.keepGoing.failed");
    }
    if kg.turns >= MAX_TURNS {
        return Next::Halt("reason.keepGoing.cap");
    }
    match room {
        Ok(()) => Next::Continue,
        Err(why) => Next::Halt(why),
    }
}

/// Where the answer ends once [`STOP`] is taken off, when it ends with it, whatever its case and the marks a model puts
/// around a line (quotes, bold, a final full stop). Marks are taken only from the line of the stop: the answer before it
/// keeps every character.
pub fn stop_at(text: &str) -> Option<usize> {
    let mark = |c: char| matches!(c, '`' | '*' | '_' | '"' | '\'');
    let end = text.trim_end_matches(|c: char| c.is_whitespace() || mark(c) || c == '.').len();
    let start = end.checked_sub(STOP.len())?;
    text.get(start..end)?.eq_ignore_ascii_case(STOP).then_some(())?;
    let line = text[..start].rfind('\n').map_or(0, |n| n + 1);
    let cut = if text[line..start].chars().all(|c| c.is_whitespace() || mark(c)) { line } else { start };
    Some(text[..cut].trim_end().len())
}

impl Engine {
    /// Turns "keep going" on or off in a project conversation. Turned on while the last reply has ended (done, failed or
    /// stopped), the next turn starts now; turned off, nothing more is sent (a turn that runs goes on until it ends or the person stops it).
    pub fn keep_going(&self, id: &str, on: bool) -> Result<(), String> {
        self.with(|c| c.set_keep_going(id, on))
    }
}

impl Core {
    fn set_keep_going(&mut self, id: &str, on: bool) -> Result<(), String> {
        let c = self.load(id).ok_or("error.conversationGone")?;
        if on && c.area != Area::Project {
            return Err("error.keepGoing".into());
        }
        if on == c.keep_going.is_some() {
            return Ok(());
        }
        c.keep_going = on.then(KeepGoing::default);
        drop_own_turns(c);
        // While Faber's checks of the reply run, their end goes on instead (`verifying.rs`).
        let idle_reply = c
            .messages
            .last()
            .filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming) && !verifying::checking(c))
            .map(|m| m.id.clone());
        self.save(id);
        self.emit_queue(id);
        self.list_changed();
        self.refresh_running();
        if on
            && let Some(reply) = idle_reply
            && !self.live.get(id).is_some_and(|l| l.turn.is_some())
        {
            self.go_on(id, &reply);
        }
        Ok(())
    }

    /// The person stopped the conversation: nothing more goes out by itself.
    pub(super) fn keep_going_off(&mut self, id: &str) {
        if let Some(c) = self.convs.get_mut(id)
            && c.keep_going.take().is_some()
        {
            drop_own_turns(c);
            self.save(id);
            self.emit_queue(id);
            self.list_changed();
        }
    }

    /// The end of a turn (`finish`), before a message the person queued goes out: the reply is read now (the stop line
    /// leaves it, a failure counts) and a stop is said on it, so a turn of the person's in between never hides it. What
    /// comes back is the reply to go on from, once the queue went out (`keep_going_after`).
    pub(super) fn keep_going_ended(&mut self, conv: &str, turn_id: Option<&str>) -> Option<String> {
        let reply_id = turn_id?;
        let c = self.convs.get_mut(conv)?;
        let kg = c.keep_going.clone()?;
        let m = c.messages.last_mut().filter(|m| m.id == reply_id && m.role == Role::Assistant)?;
        let ok = m.status == Some(Status::Done);
        let asked = m.tools.iter().flatten().any(|t| t.parent.is_none() && t.name == "AskUserQuestion");
        let said_stop = ok
            && stop_at(&m.text).is_some_and(|at| {
                m.text.truncate(at);
                let len = m.text.encode_utf16().count();
                m.activity_end = m.activity_end.map(|e| e.min(len));
                if let Some(breaks) = m.text_breaks.as_mut() {
                    breaks.retain(|b| *b < len);
                }
                true
            });
        match next(&kg, Ended { ok, asked, said_stop }, self.room_next(conv)) {
            Next::Halt(why) => {
                self.wait_for_room(conv, reply_id, why);
                None
            }
            Next::Continue => {
                if let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) {
                    kg.failed = if ok { 0 } else { kg.failed + 1 };
                }
                Some(reply_id.to_string())
            }
        }
    }

    /// After the queue went out. While a turn of the person's runs or is being routed, the end of it decides again. A
    /// message of theirs still waiting (the queue does not go out after a failed turn) is the next turn, as they wrote it;
    /// otherwise Faber sends its own.
    pub(super) fn keep_going_after(&mut self, conv: &str, reply_id: Option<String>) {
        let Some(reply_id) = reply_id else { return };
        let Some(c) = self.convs.get_mut(conv).filter(|c| c.keep_going.is_some()) else { return };
        let busy =
            self.live.get(conv).is_some_and(|l| l.turn.is_some()) || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming));
        if busy {
            return;
        }
        if c.queued.is_empty() {
            return self.send_next(conv, &reply_id);
        }
        let waiting = c.queued.remove(0);
        self.save(conv);
        self.emit_queue(conv);
        // A refusal with a reply fails it (`fail_reply`), which says why it stopped there; one before any reply (the message
        // waits in the queue again) stops on the reply before it.
        if let Err(error) = self.dispatch_queued(conv, waiting) {
            self.wait_for_room(conv, &reply_id, stop_for(&error));
        }
    }

    /// A reply that ended without `finish` (`fail_reply`: out of quota, a move that failed, a turn that could not start):
    /// nothing will end it again, so it stops here.
    pub(super) fn keep_going_failed(&mut self, conv: &str, reply_id: &str, error: &str) {
        if self.convs.get(conv).is_some_and(|c| c.keep_going.is_some()) {
            self.wait_for_room(conv, reply_id, stop_for(error));
        }
    }

    /// Turned on after a reply that ended (done, failed or stopped): only the pacing can hold it back, and a message the
    /// person left goes first.
    pub(super) fn go_on(&mut self, conv: &str, reply_id: &str) {
        match self.room_next(conv) {
            Ok(()) => self.keep_going_after(conv, Some(reply_id.into())),
            Err(why) => self.wait_for_room(conv, reply_id, why),
        }
    }

    fn send_next(&mut self, conv: &str, reply_id: &str) {
        if let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) {
            kg.turns += 1;
        }
        self.save(conv);
        if let Err(error) = self.send_by_itself(conv, CONTINUE.into(), Wake::of("wake.keepGoing")) {
            // A reply that failed already stopped it (`keep_going_failed`); this is a turn that never became one.
            self.wait_for_room(conv, reply_id, stop_for(&error));
        }
    }

    /// The room of the AI the next turn goes to: the one picked by hand when the conversation has one, otherwise any AI
    /// Auto's router may use (it picks among those with room, so one that is out never stops it while the other has some).
    pub(super) fn room_next(&self, conv: &str) -> Result<(), &'static str> {
        let Some(c) = self.convs.get(conv) else { return Ok(()) };
        let vendors = self.vendors();
        if let Some(choice) = c.choice.as_ref().filter(|ch| ch.vendor != "auto") {
            return pick_model(choice, &vendors, self.codex_recommended.as_deref())
                .map_or(Ok(()), |p| self.room(p.vendor, p.model.as_deref()));
        }
        let mut first = None;
        for vendor in vendors {
            match self.room(vendor, None) {
                Ok(()) => return Ok(()),
                Err(why) => {
                    first.get_or_insert(why);
                }
            }
        }
        first.map_or(Ok(()), Err)
    }

    /// Whether unattended work may go on this AI: the same door as the rest of the engine, with nothing picked by hand, so
    /// an AI billed per use is never spent on by itself. How fast the week's quota is spent never holds it back (R212):
    /// whoever started it trusts Faber to keep going, and Auto's pace only picks which AI answers.
    fn room(&self, vendor: Vendor, model: Option<&str>) -> Result<(), &'static str> {
        match self.allows(vendor, model.and_then(family_of), Purpose::UserTurn { by_hand: false }, false) {
            Err(Why::Billing) => Err("reason.keepGoing.billed"),
            Err(_) => Err("reason.keepGoing.quota"),
            Ok(()) => Ok(()),
        }
    }

    /// When an AI the next turn may go to has quota again (the earliest of them, or the one picked by hand), if that is
    /// known and within a week and a day.
    fn room_back_at(&self, conv: &str) -> Option<f64> {
        let now = self.secs();
        let vendors = self.vendors();
        let hand = self.convs.get(conv)?.choice.clone().filter(|ch| ch.vendor != "auto");
        let at = match hand.and_then(|ch| pick_model(&ch, &vendors, self.codex_recommended.as_deref())) {
            Some(p) => self.ledger.of(p.vendor).snap(now).recovery_for(p.model.as_deref().and_then(family_of), now),
            None => crate::route::candidates(&vendors, &self.codex_models)
                .iter()
                .filter_map(|p| self.ledger.of(p.vendor).snap(now).recovery_for(family_of(p.model), now))
                .reduce(f64::min),
        }?;
        (at - now <= MAX_WAIT_SECS).then_some(at)
    }

    /// Out of quota is a wait, not an end: when the reset is known the conversation goes on by itself then, as long as
    /// keep going is still on and nothing newer happened in it. Without a known reset it stops, as nothing would wake it.
    fn wait_for_room(&mut self, conv: &str, reply_id: &str, why: &'static str) {
        let Some(at) = (why == "reason.keepGoing.quota").then(|| self.room_back_at(conv)).flatten() else {
            return self.halt(conv, reply_id, why);
        };
        let wait = (at - self.secs()).max(1.0) + 5.0;
        let (me, conv, reply) = (self.me.clone(), conv.to_string(), reply_id.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            if let Some(core) = me.upgrade() {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                let waiting = core.convs.get(&conv).is_some_and(|c| {
                    c.keep_going.is_some() && c.messages.last().is_some_and(|m| m.id == reply && m.status != Some(Status::Streaming))
                });
                if waiting && !core.live.get(&conv).is_some_and(|l| l.turn.is_some()) {
                    core.go_on(&conv, &reply);
                }
            }
        });
    }

    /// It stops by itself: the reply says why, and the shell tells the person.
    pub(super) fn halt(&mut self, conv: &str, reply_id: &str, why: &'static str) {
        let Some(c) = self.convs.get_mut(conv).filter(|c| c.keep_going.is_some()) else { return };
        c.keep_going = None;
        drop_own_turns(c);
        let title = c.title.clone();
        let mut vendor = None;
        if let Some(m) = c.messages.iter_mut().find(|m| m.id == reply_id) {
            m.halt = Some(why.into());
            vendor = m.vendor;
        }
        self.save(conv);
        self.emit_queue(conv);
        self.push(conv, Some(reply_id), true);
        self.list_changed();
        (self.emit)(AppEvent::Halted { conversation_id: conv.into(), title, why: why.into(), vendor });
    }
}

/// A continuation Faber queued (behind a turn of the person's) never goes out once keep going is off.
fn drop_own_turns(c: &mut Conversation) {
    c.queued.retain(|q| q.sending || q.wake.as_ref().is_none_or(|w| w.why != "wake.keepGoing"));
}

/// Why it stops, for an error that ended a turn Faber could not go on from.
fn stop_for(error: &str) -> &'static str {
    match error {
        "error.quotaOut" | "error.quotaWait" => "reason.keepGoing.quota",
        "error.billingUnknown" | "error.billedPerUse" => "reason.keepGoing.billed",
        _ => "reason.keepGoing.failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DONE: Ended = Ended { ok: true, asked: false, said_stop: false };

    #[test]
    fn it_goes_on_until_a_question_nothing_left_two_failures_the_cap_or_the_pace() {
        let kg = KeepGoing::default();
        assert_eq!(next(&kg, DONE, Ok(())), Next::Continue);
        assert_eq!(next(&kg, Ended { asked: true, ..DONE }, Ok(())), Next::Halt("reason.keepGoing.asked"));
        assert_eq!(next(&kg, Ended { said_stop: true, ..DONE }, Ok(())), Next::Halt("reason.keepGoing.done"));
        let failed = Ended { ok: false, ..DONE };
        assert_eq!(next(&kg, failed, Ok(())), Next::Continue, "one failure is tried again");
        assert_eq!(next(&KeepGoing { failed: 1, ..kg.clone() }, failed, Ok(())), Next::Halt("reason.keepGoing.failed"));
        assert_eq!(next(&KeepGoing { turns: MAX_TURNS, ..kg.clone() }, DONE, Ok(())), Next::Halt("reason.keepGoing.cap"));
        assert_eq!(next(&kg, DONE, Err("reason.keepGoing.pace")), Next::Halt("reason.keepGoing.pace"));
        assert_eq!(next(&kg, Ended { asked: true, ..DONE }, Err("reason.keepGoing.pace")), Next::Halt("reason.keepGoing.asked"));
    }

    #[test]
    fn the_stop_line_is_found_at_the_end_whatever_the_marks_around_it() {
        for text in ["All done.\n\n[keep going: stop]", "All done.\n`[keep going: stop]`", "All done.\n**[Keep going: STOP]**.\n"] {
            let at = stop_at(text).unwrap_or_else(|| panic!("{text:?}"));
            assert_eq!(text[..at].trim_end(), "All done.", "{text:?}");
        }
        assert_eq!(stop_at("[keep going: stop] was the line, then more work"), None);
        assert_eq!(stop_at("Nothing to see"), None);
        assert_eq!(stop_at(""), None);
        assert_eq!(stop_at("é[keep going: stop]"), Some("é".len()));
        // The answer before the line keeps its own marks; a stop at the end of a line of text is cut there.
        let code = "Changed `a.rs`\n[keep going: stop]";
        assert_eq!(&code[..stop_at(code).unwrap()], "Changed `a.rs`");
        let quoted = "He said \"done\" [keep going: stop]";
        assert_eq!(&quoted[..stop_at(quoted).unwrap()], "He said \"done\"");
    }
}
