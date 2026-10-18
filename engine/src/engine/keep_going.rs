//! "Keep going" (R199): the person tells a project conversation to go on while they are away, and each time a turn ends
//! Faber sends the next one by itself (`by_itself.rs`), until the whole request is done and checked, the agent needs the
//! person, only an AI billed per use is left, or the cap of turns is reached. Then it stops and says why on the reply, and
//! the tray tells the person (R92). Nothing else stops it. Quota never does: when the AI it runs on is out it moves to
//! another one, picked by hand or not, and only when every AI is out it waits for the first reset (or looks again after a
//! while when no reset is known), which the running list shows. An error passes after a longer rest each time, a turn that
//! only waited rests before the next, and a question nobody answered gets its first choice. It survives a restart of Faber.
//! It reacts to the end of a turn, to quota readings and to its own timers, and never polls.

use super::*;
use crate::budget::{Reach, Why};
use crate::model::{AutoAnswer, KeepGoing, KeepWait, Wake};

/// What the agent is told at each turn Faber sends. It is for the model, so it is English like every prompt of the engine;
/// the person sees `wake.keepGoing` in their language instead. It names no project's own tools: the project's
/// instructions say where its task list is.
pub const CONTINUE: &str = "The person is away and asked you to keep going on your own until their whole request is done. \
Continue with the next item of your plan, or take the next task from the project's task list as its instructions describe, do it, \
and check it the way the project says. Do not stop to propose work or to ask for a go-ahead: decide what is yours to decide. \
Waiting on CI, a queue, a build, a monitor or a review is never a reason to stop: check it again, or do the next item meanwhile. \
Stop only when the whole request is done and verified, or when only the person can decide something; in that case ask it with your \
question tool when you have one, and say exactly what you need. When the whole request is done and verified, say so in one short \
line and end your reply with this line alone: [keep going: stop]";

/// What the agent is told once, when it first says nothing is left: a second look at the original request before Faber believes it.
pub const CHECK: &str = "Before you stop: reread the person's original request word for word. List anything that is not done or not \
verified. If anything is left, continue it now. If nothing is left, end your reply with this line alone again: [keep going: stop]";

/// The line the agent ends its reply with when nothing is left. Faber takes it off the answer.
pub const STOP: &str = "[keep going: stop]";

/// At most this many turns that did real work Faber sends by itself before it stops and lets the person look. A turn that
/// only waited (a quick answer with no tool) does not count.
pub const MAX_TURNS: u32 = 200;

/// Failures in a row (an error, or no AI Faber can reach) before it stops.
pub const MAX_FAILED: u32 = 6;

/// The longest it waits for a reset it knows (a week and a day), so a reset it reads wrongly never holds it forever.
pub const MAX_WAIT_SECS: f64 = 8.0 * 24.0 * 3600.0;

/// Without a known reset (or one past `MAX_WAIT_SECS`) it looks again this often. A quota reading that shows room wakes it
/// sooner (`keep_going_recheck`).
pub const RETRY_SECS: f64 = 600.0;

/// Moves to another AI inside one turn.
pub const MAX_MOVES: u8 = 4;

/// How long it rests after the first, second and later failures in a row.
const BACKOFF: [f64; 3] = [60.0, 300.0, 900.0];
/// Faber was busy with an update and could not start the turn.
const BUSY_RETRY: f64 = 30.0;
/// After Faber opens, the accounts need a moment to be read before the work goes on.
const START_WAIT: f64 = 20.0;
/// A question nobody answered in this time gets its first choice.
const QUESTION_WAIT: f64 = 600.0;
/// A turn shorter than this with no tool did not work, it only waited.
const QUICK_MS: u64 = 60_000;
/// Errors that do not mend by waiting: one that comes twice in a row ends it.
const PERMANENT: [&str; 6] =
    ["error.notConnected", "error.noSubscription", "error.workingFolder", "error.projectGone", "error.conversationGone", "error.keepGoing"];
/// What the door says when an account is not read yet: a rest like a failure, never shown.
const UNKNOWN: &str = "reason.keepGoing.unknown";

/// What follows the end of a turn that finished while "keep going" is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Next {
    Continue,
    /// Send the next turn after `secs` (one of Faber's own waits, `why` for the screen).
    Wait {
        secs: f64,
        why: &'static str,
    },
    /// Stops, or waits for quota when the reason is the quota's (`wait_for_room`).
    Halt(&'static str),
}

/// The end of one turn that finished, as the decision reads it.
#[derive(Debug, Clone, Copy)]
pub struct Ended {
    /// A question the agent asked the person was still unanswered at the end of the turn.
    pub asked: bool,
    /// The reply ended with [`STOP`].
    pub said_stop: bool,
    /// The turn only waited: under a minute, no tool.
    pub idle: bool,
    /// Faber sent the turn itself (not the person).
    pub own: bool,
}

/// How long it rests after `n` failures in a row.
pub fn backoff(n: u32) -> f64 {
    BACKOFF[(n.max(1) as usize - 1).min(BACKOFF.len() - 1)]
}

/// How long a turn that only waited rests before the next: two minutes, up to five when it keeps happening.
pub fn rest(kg: &KeepGoing) -> f64 {
    120.0 + 60.0 * f64::from(kg.idle.min(3))
}

/// What follows a failure: a longer rest each time, then the end after [`MAX_FAILED`] in a row, or at once when an error that
/// waiting cannot mend comes twice.
pub fn failure(kg: &KeepGoing, error: &str) -> Next {
    let n = kg.failed + 1;
    if n >= MAX_FAILED || (PERMANENT.contains(&error) && kg.last_error.as_deref() == Some(error)) {
        return Next::Halt("reason.keepGoing.failed");
    }
    Next::Wait { secs: backoff(n), why: "retry" }
}

/// The decision after a turn that finished, without the engine: `room` is the door of the AI that answers next (`Err` holds
/// the reason it says). `kg.work` already counts the turn that ended.
pub fn next(kg: &KeepGoing, ended: Ended, room: Result<(), &'static str>) -> Next {
    if ended.asked {
        return Next::Halt("reason.keepGoing.asked");
    }
    // Only the answer to Faber's own check turn ends it: a stop in a turn of the person's is a first stop.
    if ended.said_stop && kg.checked && ended.own {
        return Next::Halt("reason.keepGoing.done");
    }
    if kg.work >= MAX_TURNS {
        return Next::Halt("reason.keepGoing.cap");
    }
    if let Err(why) = room {
        return Next::Halt(why);
    }
    // The first stop is not believed yet: the check turn goes at once.
    if ended.idle && !ended.said_stop {
        return Next::Wait { secs: rest(kg), why: "rest" };
    }
    Next::Continue
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

/// A question the agent asked the person that nobody has answered: its card still waits, or its call never got a result.
pub(super) fn unanswered_question(m: &Message) -> bool {
    m.tools.iter().flatten().any(|t| t.parent.is_none() && t.name == "AskUserQuestion" && t.ok != Some(true))
        || m.permissions.iter().flatten().any(|p| p.tool == "AskUserQuestion")
}

/// The first choice of every question of an `AskUserQuestion` call, the way the screen answers it (each question's text to the
/// label picked), and what to write on the reply. None when a question has no choices: only the person can answer that one.
fn first_choices(input: &serde_json::Value) -> Option<(serde_json::Value, Vec<AutoAnswer>)> {
    let mut answers = serde_json::Map::new();
    let mut chosen = vec![];
    for q in input.get("questions")?.as_array()? {
        let question = q.get("question")?.as_str()?.to_string();
        let first = q.get("options")?.as_array()?.first()?;
        let label = first.get("label").and_then(|l| l.as_str()).or_else(|| first.as_str())?.to_string();
        answers.insert(question.clone(), serde_json::Value::String(label.clone()));
        chosen.push(AutoAnswer { question, answer: label });
    }
    (!chosen.is_empty()).then_some((serde_json::Value::Object(answers), chosen))
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
        c.keep_going = on.then(|| KeepGoing { since: now(), ..KeepGoing::default() });
        drop_own_turns(c);
        // While Faber's checks of the reply run, their end goes on instead (`verifying.rs`).
        let idle_reply = c
            .messages
            .last()
            .filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming) && !verifying::checking(c))
            .map(|m| m.id.clone());
        // A question that already waits is answered for the person if they do not come back in time.
        let waiting: Vec<(String, String)> = c
            .messages
            .last()
            .filter(|m| m.status == Some(Status::Streaming))
            .map(|m| {
                let cards = m.permissions.iter().flatten().filter(|p| p.tool == "AskUserQuestion");
                cards.map(|p| (m.id.clone(), p.request_id.clone())).collect()
            })
            .unwrap_or_default();
        self.save(id);
        self.emit_queue(id);
        self.list_changed();
        self.refresh_running();
        for (turn, request) in waiting.into_iter().filter(|_| on) {
            self.watch_question(id, &turn, &request);
        }
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
            self.refresh_running();
        }
    }

    /// Faber just opened: each conversation "keep going" was on in goes on after a short wait for the accounts to be read.
    /// Whatever it waited for before (a reset, a rest) is looked at again then, by the same door as any other turn.
    pub(super) fn resume_keep_going(&mut self) {
        // A rest it was in goes on for what is left of it; the start wait is only the least.
        let (start, now) = (START_WAIT * self.keep_pace, self.secs());
        for id in self.store.keep_going_ids() {
            let Some(c) = self.load(&id) else { continue };
            if c.keep_going.is_none() {
                continue;
            }
            let Some(reply) = c.messages.iter().rev().find(|m| m.role == Role::Assistant).map(|m| m.id.clone()) else {
                c.keep_going = None;
                self.save(&id);
                continue;
            };
            let saved = c.keep_going.as_ref().and_then(|k| k.waiting.clone());
            match saved {
                Some(w) if w.until - now > start => self.arm(&id, &reply, w.until - now, static_why(&w.why)),
                _ => self.arm(&id, &reply, start, "start"),
            }
        }
        self.list_changed();
    }

    /// The end of a turn (`finish`), before a message the person queued goes out: the reply is read now (the stop line
    /// leaves it, a failure counts) and what follows is decided, so a turn of the person's in between never hides it. What
    /// comes back is the reply to go on from, once the queue went out (`keep_going_after`).
    pub(super) fn keep_going_ended(&mut self, conv: &str, turn_id: Option<&str>) -> Option<String> {
        let reply_id = turn_id?;
        let c = self.convs.get_mut(conv)?;
        c.keep_going.as_ref()?;
        // Whether Faber sent the turn itself: a turn of the person's in between never takes back a stop the agent already said.
        let own = c
            .messages
            .len()
            .checked_sub(2)
            .and_then(|i| c.messages.get(i))
            .is_some_and(|u| u.wake.as_ref().is_some_and(|w| w.why == "wake.keepGoing"));
        let m = c.messages.last_mut().filter(|m| m.id == reply_id && m.role == Role::Assistant)?;
        let ok = m.status == Some(Status::Done);
        let error = m.error.clone().unwrap_or_else(|| "error.turnFailed".into());
        let asked = ok && unanswered_question(m);
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
        let idle = ok && m.tools.iter().flatten().next().is_none() && now().saturating_sub(m.at) < QUICK_MS;
        let kg = c.keep_going.as_mut()?;
        // A reply `fail_reply` already failed (and decided on) is not decided on twice.
        if !ok && kg.failed_on.as_deref() == Some(reply_id) {
            return None;
        }
        // A turn ended: whatever wait was set before it is over, and this end decides again.
        kg.wait_until = None;
        kg.waiting = None;
        kg.moves = 0;
        if !ok {
            self.trouble(conv, reply_id, &error);
            return None;
        }
        if idle {
            kg.idle += 1;
        } else {
            kg.idle = 0;
            kg.work += 1;
        }
        let before = kg.clone();
        let decision = next(&before, Ended { asked, said_stop, idle, own }, self.room_next(conv));
        // The first stop is asked about once, by the check turn; whatever else follows asks nothing.
        if let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) {
            kg.checked = said_stop || (before.checked && !own);
            kg.failed = 0;
            kg.last_error = None;
        }
        match decision {
            Next::Continue => Some(reply_id.to_string()),
            Next::Halt(why) => {
                self.wait_for_room(conv, reply_id, why);
                None
            }
            Next::Wait { secs, why } => {
                self.arm(conv, reply_id, secs * self.keep_pace, why);
                None
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
        if let Some(kg) = c.keep_going.as_mut() {
            kg.moves = 0;
            kg.failed_on = None;
        }
        let waiting = c.queued.remove(0);
        self.save(conv);
        self.emit_queue(conv);
        // A refusal with a reply fails it (`fail_reply`), which decides what follows; one before any reply (the message waits
        // in the queue again) is handled here.
        if let Err(error) = self.dispatch_queued(conv, waiting)
            && !self.reported(conv, &error)
        {
            self.trouble(conv, &reply_id, &error);
        }
    }

    /// A reply that ended without `finish` (`fail_reply`: out of quota, a move that failed, a turn that could not start):
    /// nothing will end it again, so what follows is decided here.
    pub(super) fn keep_going_failed(&mut self, conv: &str, reply_id: &str, error: &str) {
        if self.convs.get(conv).is_some_and(|c| c.keep_going.is_some()) {
            self.trouble(conv, reply_id, error);
        }
    }

    /// Whether the last reply failed with this error already, and `fail_reply` decided what follows.
    fn reported(&self, conv: &str, error: &str) -> bool {
        self.convs
            .get(conv)
            .and_then(|c| c.messages.last())
            .is_some_and(|m| m.role == Role::Assistant && m.status == Some(Status::Error) && m.error.as_deref() == Some(error))
    }

    /// A turn could not be sent or ended in an error. A busy Faber tries again soon; quota or billing is a question of which
    /// AI can answer; anything else waits longer each time and ends after [`MAX_FAILED`] in a row.
    pub(super) fn trouble(&mut self, conv: &str, reply_id: &str, error: &str) {
        match error {
            "error.update.activeOperations" => self.arm(conv, reply_id, BUSY_RETRY * self.keep_pace, "busy"),
            "error.quotaOut" | "error.quotaWait" => self.wait_for_room(conv, reply_id, "reason.keepGoing.quota"),
            "error.billedPerUse" => self.wait_for_room(conv, reply_id, "reason.keepGoing.billed"),
            _ => self.fail_wait(conv, reply_id, error),
        }
    }

    /// A failure that waits and tries again, or ends it. A reply counts once, however many ways report it.
    fn fail_wait(&mut self, conv: &str, reply_id: &str, error: &str) {
        let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) else { return };
        if kg.failed_on.as_deref() == Some(reply_id) {
            return;
        }
        let next = failure(kg, error);
        kg.failed += 1;
        kg.last_error = Some(error.to_string());
        kg.failed_on = Some(reply_id.to_string());
        match next {
            Next::Wait { secs, why } => self.arm(conv, reply_id, secs * self.keep_pace, why),
            _ => self.halt(conv, reply_id, "reason.keepGoing.failed"),
        }
    }

    /// Turned on after a reply that ended (done, failed or stopped), or a wait that ended: only a lack of room can hold it
    /// back, and a message the person left goes first. A rest that has not ended yet holds it (the end of Faber's checks can
    /// call this while it runs).
    pub(super) fn go_on(&mut self, conv: &str, reply_id: &str) {
        if self.convs.get(conv).and_then(|c| c.keep_going.as_ref()).is_some_and(|k| k.waiting.is_some()) {
            return;
        }
        match self.room_next(conv) {
            Ok(()) => {
                if let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut())
                    && kg.wait_until.take().is_some()
                {
                    self.save(conv);
                    self.refresh_running();
                }
                self.keep_going_after(conv, Some(reply_id.into()))
            }
            Err(why) => self.wait_for_room(conv, reply_id, why),
        }
    }

    fn send_next(&mut self, conv: &str, reply_id: &str) {
        let mut text = CONTINUE;
        if let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) {
            kg.turns += 1;
            kg.moves = 0;
            kg.failed_on = None;
            if kg.checked {
                text = CHECK;
            }
        }
        self.save(conv);
        if let Err(error) = self.send_by_itself(conv, text.into(), Wake::of("wake.keepGoing"))
            && !self.reported(conv, &error)
        {
            // A turn that never became a reply (Faber was busy, nothing connected).
            self.trouble(conv, reply_id, &error);
        }
    }

    /// Whether any AI may take the next turn. One picked by hand counts like the rest: a turn Faber sends that finds it out
    /// of quota or billed per use moves to another AI (`recovery.rs`), so only every AI being out holds it back. Quota out
    /// comes first, as it is the one that comes back by itself.
    pub(super) fn room_next(&self, conv: &str) -> Result<(), &'static str> {
        if !self.convs.contains_key(conv) {
            return Ok(());
        }
        let mut why = None;
        for vendor in self.vendors() {
            match self.room(vendor, None) {
                Ok(()) => return Ok(()),
                Err(w) => {
                    if why.is_none() || w == "reason.keepGoing.quota" {
                        why = Some(w);
                    }
                }
            }
        }
        why.map_or(Ok(()), Err)
    }

    /// Why a turn Faber sends by itself cannot go to any AI, as a reason key. An AI out of quota is a refusal only when "keep
    /// going" is off: with it on, the turn waits for a reset instead.
    pub(super) fn room_refusal(&self, conv: &str) -> Option<&'static str> {
        let on = self.convs.get(conv).is_some_and(|c| c.keep_going.is_some());
        match self.room_next(conv) {
            Ok(()) => None,
            Err("reason.keepGoing.quota") if on => None,
            Err(UNKNOWN) => Some("reason.keepGoing.billed"),
            Err(why) => Some(why),
        }
    }

    /// Whether unattended work may go on this AI: the same door as the rest of the engine, with nothing picked by hand, so
    /// an AI billed per use is never spent on by itself. How fast the week's quota is spent never holds it back (R212):
    /// whoever started it trusts Faber to keep going, and Auto's pace only picks which AI answers. An account not read yet
    /// is its own reason, since it is read in a moment.
    fn room(&self, vendor: Vendor, model: Option<&str>) -> Result<(), &'static str> {
        match self.allows(vendor, model.and_then(family_of), Purpose::UserTurn { by_hand: false }, false) {
            Err(Why::Billing) if self.ledger.of(vendor).billing == Billing::Unknown => Err(UNKNOWN),
            Err(Why::Billing) => Err("reason.keepGoing.billed"),
            Err(_) => Err("reason.keepGoing.quota"),
            Ok(()) => Ok(()),
        }
    }

    /// When the first AI that is only out of quota (not billed per use) has it again: the ledger's recovery, or else the
    /// reset of the window that is full (a turn cut at the limit leaves the recovery unknown while the window still says
    /// it), if that is within a week and a day.
    fn room_back_at(&self) -> Option<f64> {
        let now = self.secs();
        let vendors: Vec<Vendor> = self.vendors().into_iter().filter(|v| self.room(*v, None) == Err("reason.keepGoing.quota")).collect();
        let at = crate::route::candidates(&vendors, &self.codex_models)
            .iter()
            .filter_map(|p| {
                let snap = self.ledger.of(p.vendor).snap(now);
                snap.recovery_for(family_of(p.model), now).or_else(|| snap.full_until(now))
            })
            .reduce(f64::min)?;
        (at - now <= MAX_WAIT_SECS).then_some(at)
    }

    /// A quota reading came in: a conversation that waits for quota goes on now when an AI has room again.
    pub(super) fn keep_going_recheck(&mut self) {
        let due: Vec<(String, String)> = self
            .convs
            .iter()
            .filter(|(id, c)| {
                c.keep_going.as_ref().is_some_and(|k| k.wait_until.is_some()) && !self.live.get(*id).is_some_and(|l| l.turn.is_some())
            })
            .filter_map(|(id, c)| {
                let m = c.messages.last().filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))?;
                Some((id.clone(), m.id.clone()))
            })
            .collect();
        for (id, reply) in due {
            if self.room_next(&id).is_ok() {
                self.go_on(&id, &reply);
            }
        }
    }

    /// Every AI out of quota is a wait, never an end: the conversation goes on by itself at the first known reset, or looks
    /// again after a while, as long as keep going is still on and nothing newer happened in it, and the running list shows
    /// until when. An account not read yet rests like a failure. Any other reason stops it.
    pub(super) fn wait_for_room(&mut self, conv: &str, reply_id: &str, why: &'static str) {
        match why {
            UNKNOWN => return self.fail_wait(conv, reply_id, "error.billingUnknown"),
            "reason.keepGoing.quota" => {}
            _ => return self.halt(conv, reply_id, why),
        }
        // Another AI has room (the turn hit its limit and was not moved): each such try counts as a failed turn and rests, so a
        // door that says yes while the turn keeps failing never loops.
        if self.room_next(conv).is_ok() {
            return self.fail_wait(conv, reply_id, "error.quotaOut");
        }
        let at = self.room_back_at().unwrap_or_else(|| self.secs() + RETRY_SECS * self.keep_pace);
        // Never sooner than half a minute, so a reset the ledger has not caught up with yet is not asked about in a loop.
        let at = at.max(self.secs() + 30.0 * self.keep_pace);
        let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) else { return };
        kg.wait_until = Some(at);
        kg.waiting = None;
        kg.moves = 0;
        self.save(conv);
        self.refresh_running();
        let wait = at - self.secs() + 5.0 * self.keep_pace;
        let (me, conv, reply) = (self.me.clone(), conv.to_string(), reply_id.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(wait)).await;
            if let Some(core) = me.upgrade() {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                // Only the latest wait wakes it: turning keep going off and on, or a newer turn, leaves this one behind.
                let waiting = core.convs.get(&conv).is_some_and(|c| {
                    c.keep_going.as_ref().is_some_and(|k| k.wait_until == Some(at))
                        && c.messages.last().is_some_and(|m| m.id == reply && m.status != Some(Status::Streaming))
                });
                if waiting && !core.live.get(&conv).is_some_and(|l| l.turn.is_some()) {
                    if let Some(kg) = core.convs.get_mut(&conv).and_then(|c| c.keep_going.as_mut()) {
                        kg.failed_on = None;
                    }
                    // A limit the CLI reported without a reset is only known to be over by trying: the flag goes, and the next
                    // turn finds out (it sets the flag again when the limit is still there).
                    for vendor in core.vendors() {
                        let v = core.ledger.of_mut(vendor);
                        if v.reach == Reach::Out(None) {
                            v.reach = Reach::Clear;
                        }
                    }
                    // A fresh reading may show room the ledger does not know yet; when it comes, it wakes it again.
                    core.quota_event();
                    core.go_on(&conv, &reply);
                }
            }
        });
    }

    /// Waits to send the next turn for a reason of Faber's own (an error to pass, a rest, a busy Faber, the start): saved on the
    /// conversation, so a restart waits out the rest, and shown on its row. Anything newer (the person's message, "keep going"
    /// off, a later wait) makes this timer do nothing when it comes.
    pub(super) fn arm(&mut self, conv: &str, reply_id: &str, secs: f64, why: &'static str) {
        let until = self.secs() + secs;
        let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) else { return };
        kg.wait_until = None;
        kg.waiting = Some(KeepWait { why: why.into(), until });
        self.save(conv);
        self.emit_queue(conv);
        self.push(conv, Some(reply_id), true);
        let (me, conv, reply) = (self.me.clone(), conv.to_string(), reply_id.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(secs.max(0.0))).await;
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).wake_keep_going(&conv, &reply, until);
            }
        });
    }

    /// A wait ended: the conversation goes on if it still waits for exactly this, with nothing newer in it.
    fn wake_keep_going(&mut self, conv: &str, reply_id: &str, until: f64) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        let current = c.keep_going.as_ref().and_then(|k| k.waiting.as_ref()).is_some_and(|w| w.until == until);
        let at_rest = c.messages.last().is_some_and(|m| m.id == reply_id && m.status != Some(Status::Streaming));
        if !current || !at_rest || self.live.get(conv).is_some_and(|l| l.turn.is_some()) {
            return;
        }
        if let Some(kg) = c.keep_going.as_mut() {
            kg.waiting = None;
            kg.failed_on = None;
        }
        self.go_on(conv, reply_id);
    }

    /// A question the agent asked the person: if nobody answers in a while and "keep going" is on, the first choice is sent for
    /// them (it is the one the agent put first, its recommendation). A permission card (Ask mode) is never answered this way.
    pub(super) fn watch_question(&mut self, conv: &str, turn: &str, request: &str) {
        if self.convs.get(conv).is_none_or(|c| c.keep_going.is_none()) {
            return;
        }
        let wait = Duration::from_secs_f64(QUESTION_WAIT * self.keep_pace);
        let (me, conv, turn, request) = (self.me.clone(), conv.to_string(), turn.to_string(), request.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(wait).await;
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).answer_for_away(&conv, &turn, &request);
            }
        });
    }

    fn answer_for_away(&mut self, conv: &str, turn: &str, request: &str) {
        let Some(c) = self.convs.get(conv).filter(|c| c.keep_going.is_some()) else { return };
        if !self.live.get(conv).is_some_and(|l| l.turn.as_deref() == Some(turn)) {
            return;
        }
        let card = c
            .messages
            .iter()
            .find(|m| m.id == turn)
            .and_then(|m| m.permissions.iter().flatten().find(|p| p.request_id == request && p.tool == "AskUserQuestion"));
        let Some((answers, chosen)) = card.and_then(|p| first_choices(&p.input)) else { return };
        self.answer_unattended(conv, turn, request, answers, chosen);
    }

    /// The CLI has the answer Faber gave for the person: it is written on the reply, for the screen to tell them.
    pub(super) fn note_answered_for_person(&mut self, conv: &str, turn: &str, chosen: Vec<AutoAnswer>) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        if let Some(kg) = c.keep_going.as_mut() {
            kg.answered += chosen.len() as u32;
        }
        if let Some(m) = c.messages.iter_mut().find(|m| m.id == turn) {
            m.auto_answers.get_or_insert_default().extend(chosen);
        }
        self.save(conv);
        self.push(conv, Some(turn), true);
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

    /// Another move to an AI with room inside the turn that runs, while the count allows it.
    pub(super) fn another_move(&mut self, conv: &str) -> bool {
        let Some(kg) = self.convs.get_mut(conv).and_then(|c| c.keep_going.as_mut()) else { return false };
        kg.moves += 1;
        kg.moves <= MAX_MOVES
    }

    /// Faber could not send a turn of its own (asking the agent to fix a failed check) while keep going is on: it tries
    /// again instead of stopping when it was busy with an update, or when the failure already set its own wait (`fail_reply`).
    /// True when it does.
    pub(super) fn keeps_trying(&mut self, conv: &str, reply_id: &str, error: &str) -> bool {
        if self.convs.get(conv).is_none_or(|c| c.keep_going.is_none()) {
            return false;
        }
        if error == "error.update.activeOperations" {
            self.arm(conv, reply_id, BUSY_RETRY * self.keep_pace, "busy");
            return true;
        }
        self.reported(conv, error)
    }
}

/// A saved wait's reason as the engine names it.
fn static_why(why: &str) -> &'static str {
    match why {
        "retry" => "retry",
        "rest" => "rest",
        "busy" => "busy",
        _ => "start",
    }
}

/// A continuation Faber queued (behind a turn of the person's) never goes out once keep going is off.
fn drop_own_turns(c: &mut Conversation) {
    c.queued.retain(|q| q.sending || q.wake.as_ref().is_none_or(|w| w.why != "wake.keepGoing"));
}

#[cfg(test)]
mod tests {
    use super::*;

    const DONE: Ended = Ended { asked: false, said_stop: false, idle: false, own: true };

    fn wait(secs: f64, why: &'static str) -> Next {
        Next::Wait { secs, why }
    }

    #[test]
    fn it_goes_on_until_a_question_the_checked_end_the_cap_or_no_ai() {
        let kg = KeepGoing::default();
        assert_eq!(next(&kg, DONE, Ok(())), Next::Continue);
        assert_eq!(next(&kg, Ended { asked: true, ..DONE }, Ok(())), Next::Halt("reason.keepGoing.asked"));
        assert_eq!(
            next(&kg, Ended { asked: true, ..DONE }, Err("reason.keepGoing.quota")),
            Next::Halt("reason.keepGoing.asked"),
            "a question is for the person whatever the quota"
        );
        // The first stop is not believed: the check turn goes (nothing halts); the second is.
        assert_eq!(next(&kg, Ended { said_stop: true, ..DONE }, Ok(())), Next::Continue);
        let checked = KeepGoing { checked: true, ..kg.clone() };
        assert_eq!(next(&checked, Ended { said_stop: true, ..DONE }, Ok(())), Next::Halt("reason.keepGoing.done"));
        assert_eq!(
            next(&checked, Ended { said_stop: true, own: false, ..DONE }, Ok(())),
            Next::Continue,
            "a stop in the person's own turn is not the answer to the check"
        );
        assert_eq!(next(&KeepGoing { work: MAX_TURNS, ..kg.clone() }, DONE, Ok(())), Next::Halt("reason.keepGoing.cap"));
        assert_eq!(next(&kg, DONE, Err("reason.keepGoing.billed")), Next::Halt("reason.keepGoing.billed"));
    }

    #[test]
    fn a_failure_waits_one_five_then_fifteen_minutes_and_ends_at_the_sixth() {
        let at = |failed| failure(&KeepGoing { failed, ..Default::default() }, "x");
        assert_eq!(at(0), wait(60.0, "retry"));
        assert_eq!(at(1), wait(300.0, "retry"));
        assert_eq!(at(2), wait(900.0, "retry"));
        assert_eq!(at(4), wait(900.0, "retry"));
        assert_eq!(at(MAX_FAILED - 1), Next::Halt("reason.keepGoing.failed"));
    }

    #[test]
    fn an_error_waiting_cannot_mend_ends_it_when_it_comes_twice() {
        let kg = KeepGoing { failed: 1, last_error: Some("error.noSubscription".into()), ..Default::default() };
        assert_eq!(failure(&kg, "error.noSubscription"), Next::Halt("reason.keepGoing.failed"));
        assert!(matches!(failure(&KeepGoing::default(), "error.noSubscription"), Next::Wait { .. }), "the first time is tried again");
        assert!(matches!(failure(&kg, "Claude Code stopped (exit 1)"), Next::Wait { .. }), "an unknown error is a possible outage");
    }

    #[test]
    fn a_turn_that_only_waited_rests_two_to_five_minutes_and_the_stop_check_does_not() {
        let kg = KeepGoing::default();
        let quick = Ended { idle: true, ..DONE };
        assert_eq!(next(&kg, quick, Ok(())), wait(120.0, "rest"));
        assert_eq!(next(&KeepGoing { idle: 3, ..kg.clone() }, quick, Ok(())), wait(300.0, "rest"));
        assert_eq!(next(&KeepGoing { idle: 40, ..kg.clone() }, quick, Ok(())), wait(300.0, "rest"), "five minutes at most");
        assert_eq!(next(&kg, Ended { said_stop: true, ..quick }, Ok(())), Next::Continue, "the check turn follows a first stop at once");
    }

    #[test]
    fn the_first_choice_of_each_question_answers_it_and_a_question_without_choices_stays() {
        let input = serde_json::json!({ "questions": [
            { "question": "Which database?", "options": [{ "label": "SQLite", "description": "x" }, { "label": "Postgres" }] },
            { "question": "Which port?", "options": ["8080", "9090"] }] });
        let (answers, chosen) = first_choices(&input).unwrap();
        assert_eq!(answers, serde_json::json!({ "Which database?": "SQLite", "Which port?": "8080" }));
        assert_eq!(chosen.len(), 2);
        assert_eq!(chosen[0], AutoAnswer { question: "Which database?".into(), answer: "SQLite".into() });
        assert_eq!(first_choices(&serde_json::json!({ "questions": [{ "question": "Name?", "options": [] }] })), None);
        assert_eq!(first_choices(&serde_json::json!({})), None);
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

    #[test]
    fn the_prompt_never_lets_the_agent_stop_to_wait() {
        assert!(CONTINUE.contains("never a reason to stop"));
        assert!(CONTINUE.contains("only the person can decide"));
        assert!(CHECK.contains("original request word for word") && CHECK.ends_with(STOP));
    }
}
