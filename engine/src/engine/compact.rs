//! The context of a conversation's own agent: the fill the screen's meter shows, and the compaction the agent asks for with
//! Faber's conversation tool (`compact`). A subagent's or a helper's context never counts here.
//!
//! Compacting happens once the agent's turn is over, never inside it: the CLI replaces the conversation with a summary, so a
//! turn that goes on would lose what it still needs. When the turn ends well and nothing else runs (Faber's checks included),
//! Faber asks the CLI to compact (`Session::compact`); the conversation counts as at work meanwhile (`Conversation::compacting`),
//! so what the person writes and what Faber sends by itself wait in the queue. When the CLI says it ended, the agent's own
//! `next` joins the end of the queue as a message Faber writes (`wake.compacted`), after whatever already waited.
//!
//! Under Keep going Faber also compacts by itself, at the moments a careful person would, before it sends the next turn
//! (`compact_at_boundary`): when the reply closed a task and the context is already large, or when it is large whatever
//! the reply did. It compacts while the prompt cache is still warm (the turn just ended), where reading the context costs a
//! tenth of the plain price. Every later turn rereads the whole context, and that reading is most of what a long run costs.
//!
//! The agent may instead start its context fresh (`clear`), when the task it takes next has nothing to do with what the
//! conversation did so far: at the same moment Faber ends the CLI session and forgets every session of the conversation, so
//! the next turn starts a new one, which costs nothing and carries no summary. That turn is a message Faber writes
//! (`wake.cleared`) with the person's own requests, word for word, and the agent's handoff note; no CLI is ever given what
//! came before it again (`Conversation::fresh_after`). The person still sees the whole conversation.
//!
//! A compaction says nothing until it ends, so its time is watched (task 110): past `NOTICE` the reply it follows says
//! so, with Stop; at `DEADLINE` Faber stops it like a compaction that failed, so the context stays as it was, and what
//! waited in the queue and Keep going go on. Cost: one map entry and one task that sleeps `EVERY` while a compaction runs.
use super::*;
use crate::model::ContextFill;
use crate::sibling;

/// What the agent asked for with `compact`, kept until its turn ends.
pub(super) struct Plan {
    /// Everything it needs to continue; sent back to it word for word.
    next: Option<String>,
    /// A fresh context (`clear`) instead of a summary: the reply whose turn asked for it. It survives the process, so it
    /// happens only after that very reply; any other turn first (one that failed outside `end_turn`) drops it.
    fresh: Option<String>,
}

/// After a reply that closed a task (its closing box), Faber compacts once the context holds a quarter of its window...
const TASK_END_SHARE: u64 = 4;
/// ...and never below this many tokens, where the summary would save little (a model's base context alone is ~40K).
const FLOOR: u64 = 80_000;
/// At any other turn Keep going sends, from three fifths of the window.
const ANY_TURN_SHARE: (u64, u64) = (3, 5);

/// A compaction that runs longer than this is told on the reply it follows. The longest in the recordings took seconds;
/// summarizing a full window is one long answer of the model, a few minutes at most.
const NOTICE: f64 = 300.0;
/// One that runs this long is stopped: as long as a silent reply is given (`silence::BOUND`).
const DEADLINE: f64 = silence::BOUND;
/// How often a running compaction is looked at.
const EVERY: Duration = Duration::from_secs(30);

/// A compaction the CLI is running: since when, the last look (a jump of the clock is the PC asleep, not the compaction's
/// time), and whether Faber already stopped it.
pub(super) struct Timed {
    since: f64,
    looked: f64,
    cut: bool,
}

/// What Keep going knows before its next turn.
#[derive(Clone, Copy)]
struct Moment {
    fill: ContextFill,
    /// The reply closed a task with its closing box.
    closed_task: bool,
}

/// Whether Keep going compacts before its next turn.
fn boundary_due(m: Moment) -> bool {
    let point = match m.closed_task {
        true => (m.fill.window / TASK_END_SHARE).max(FLOOR),
        false => m.fill.window / ANY_TURN_SHARE.1 * ANY_TURN_SHARE.0,
    };
    m.fill.used >= point
}

impl Core {
    /// The CLI reported how full the main agent's context is, or the window of its model (either may be missing).
    pub(super) fn note_context(&mut self, conv: &str, vendor: Vendor, used: Option<u64>, window: Option<u64>) {
        let model = self.live.get(conv).and_then(|l| l.model.clone().or_else(|| l.running.clone()));
        // Claude Code is started with the point where it compacts (`--autocompact`): that is the size the conversation is
        // counted against, since it never grows beyond it.
        let point = (vendor == Vendor::Claude && self.claude_flags.autocompact)
            .then(|| model.as_deref().and_then(crate::claude::autocompact_tokens))
            .flatten();
        let Some(c) = self.convs.get_mut(conv) else { return };
        c.reported = (used.or(c.reported.0), window.or(c.reported.1));
        let window = match (point, c.reported.1) {
            (Some(point), Some(window)) => Some(point.min(window)),
            (point, window) => point.or(window),
        };
        let fill = c.reported.0.zip(window).map(|(used, window)| ContextFill { used, window });
        if fill.is_none() || fill == c.context {
            return;
        }
        c.context = fill;
        (self.emit)(AppEvent::Context { conversation_id: conv.to_string(), context: fill });
    }

    /// The CLI compacted the conversation: what it holds is the summary now. Claude Code says how big it is; Codex reports
    /// the new size as a context update of its own.
    pub(super) fn note_compacted(&mut self, conv: &str, vendor: Vendor, after: Option<u64>) {
        // The summary replaces a first input that told the session of Faber's guide (Claude Code, a resumed Codex thread); a new
        // Codex thread's instructions are kept.
        if let Some(l) = self.live.get_mut(conv)
            && l.guide.compacted()
            && let Some(c) = self.convs.get_mut(conv)
        {
            c.session_guide_digests.remove(&vendor);
        }
        if after.is_some() {
            self.note_context(conv, vendor, after, None);
        }
    }

    /// The agent called `context`.
    pub(super) fn context_answer(&self, conv: &str) -> String {
        sibling::context(self.convs.get(conv).and_then(|c| c.context).map(|c| (c.used, c.window)))
    }

    /// The agent called `compact`: it happens when its turn ends.
    pub(super) fn plan_compaction(&mut self, conv: &str, next: Option<String>) -> Result<String, &'static str> {
        if !self.live.contains_key(conv) {
            return Err("this conversation has no running agent");
        }
        if self.convs.get(conv).is_some_and(|c| c.compacting) {
            return Err("it is being compacted already");
        }
        let answer = sibling::compact_planned(next.is_some());
        self.compacts.insert(conv.to_string(), Plan { next, fresh: None });
        Ok(answer)
    }

    /// The agent called `clear`: its context starts fresh when its turn ends.
    pub(super) fn plan_clear(&mut self, conv: &str, next: String) -> Result<(), &'static str> {
        if !self.live.contains_key(conv) {
            return Err("this conversation has no running agent");
        }
        if self.convs.get(conv).is_some_and(|c| c.compacting) {
            return Err("it is being compacted already");
        }
        let reply = self.convs.get(conv).and_then(|c| c.messages.last()).filter(|m| m.role == Role::Assistant).map(|m| m.id.clone());
        let Some(reply) = reply else { return Err("this conversation has no turn under way") };
        self.compacts.insert(conv.to_string(), Plan { next: Some(next), fresh: Some(reply) });
        Ok(())
    }

    /// The person pressed "Compact now" in the context meter: the same compaction the agent asks for, with no `next`, done
    /// at once since no turn runs. A conversation whose agent has no process
    /// (Claude Code closes an idle one) gets one with its last reply's choice, resuming its own session.
    pub(super) fn compact_now(&mut self, conv: &str) -> Result<(), String> {
        let c = self.convs.get(conv).ok_or("error.conversationGone")?;
        if c.compacting || self.compacts.contains_key(conv) {
            return Ok(());
        }
        let turn = self.live.get(conv).is_some_and(|l| l.turn.is_some());
        if turn || verifying::checking(c) || !c.queued.is_empty() || c.messages.last().is_none_or(|m| m.status != Some(Status::Done)) {
            return Err("error.compact.busy".into());
        }
        if let Err(error) = self.resume_compaction(conv) {
            if error == super::mcp_servers::CHECK_PENDING {
                self.compacts.insert(conv.to_string(), Plan { next: None, fresh: None });
                if let Some(c) = self.convs.get_mut(conv) {
                    c.compacting = true;
                }
                self.list_changed();
                let key = conv.to_string();
                self.after_kit_check(conv, move |core| {
                    if core.compacts.contains_key(&key) && !core.closing && !core.installing {
                        if let Some(c) = core.convs.get_mut(&key) {
                            c.compacting = false;
                        }
                        if !core.compact_due(&key) {
                            core.compacts.remove(&key);
                            core.list_changed();
                        }
                    }
                });
                return Ok(());
            }
            return Err(error);
        }
        self.compacts.insert(conv.to_string(), Plan { next: None, fresh: None });
        if !self.compact_due(conv) {
            self.compacts.remove(conv);
            return Err("error.compact.busy".into());
        }
        Ok(())
    }

    /// Both requested and scheduled compactions resume the last reply's session if its process was replaced.
    fn resume_compaction(&mut self, conv: &str) -> Result<(), String> {
        let c = self.convs.get(conv).ok_or("error.conversationGone")?;
        let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant && m.vendor.is_some()).ok_or("error.compact.busy")?;
        let pick = crate::router::Pick {
            vendor: last.vendor.unwrap_or(Vendor::Claude),
            model: last.model.clone(),
            effort: last.effort.clone().unwrap_or_else(|| "medium".into()),
            reason: String::new(),
            pace: None,
        };
        // The context to compact is the last reply's agent's; a process warmed for another CLI picked in the composer is not it.
        if self.live.get(conv).is_some_and(|l| l.vendor != pick.vendor)
            && let Some(l) = self.retire(conv)
        {
            l.session.retire();
        }
        if !self.live.contains_key(conv) {
            self.launch_session(conv, &pick, kits::SessionStart::Compact)?;
        }
        Ok(())
    }

    /// The person opened the context meter: Claude Code is asked what fills the context, part by part (no turn, no
    /// process started for it). False when nobody can answer: no running Claude Code process for this conversation.
    pub(super) fn ask_context_parts(&self, conv: &str) -> bool {
        match self.live.get(conv).filter(|l| l.vendor == Vendor::Claude) {
            Some(l) => {
                l.session.context_usage();
                true
            }
            None => false,
        }
    }

    /// The point where the queue, Faber's checks and Keep going would go on (`drain`): when a compaction was asked for and
    /// the agent's last turn ended well, it starts now. True while it starts or runs, so nothing else goes out meanwhile.
    pub(super) fn compact_due(&mut self, conv: &str) -> bool {
        if self.rewind_blocked(conv) {
            return false;
        }
        let Some(fresh) = self.compacts.get(conv).map(|p| p.fresh.clone()) else { return false };
        if self.live.get(conv).is_some_and(|l| l.turn.is_some()) {
            return false;
        }
        let Some(c) = self.convs.get(conv) else { return false };
        if c.compacting {
            return true;
        }
        if verifying::checking(c) || c.messages.last().is_none_or(|m| m.status != Some(Status::Done)) {
            return false;
        }
        if let Some(reply) = fresh {
            match c.messages.last().is_some_and(|m| m.id == reply) {
                true => self.clear(conv),
                false => drop(self.compacts.remove(conv)),
            }
            return false;
        }
        if !self.live.contains_key(conv)
            && let Err(error) = self.resume_compaction(conv)
        {
            if error == super::mcp_servers::CHECK_PENDING {
                if let Some(c) = self.convs.get_mut(conv) {
                    c.compacting = true;
                }
                self.list_changed();
                let key = conv.to_string();
                self.after_kit_check(conv, move |core| {
                    if core.compacts.contains_key(&key) && !core.closing && !core.installing {
                        if let Some(c) = core.convs.get_mut(&key) {
                            c.compacting = false;
                        }
                        if !core.compact_due(&key) {
                            core.compacts.remove(&key);
                            core.list_changed();
                        }
                    }
                });
                return true;
            }
            return false;
        }
        let (Some(l), Some(c)) = (self.live.get(conv), self.convs.get_mut(conv)) else { return false };
        c.compacting = true;
        self.rewind.bump(conv);
        l.session.compact();
        self.time_compaction(conv);
        self.list_changed();
        true
    }

    /// The CLI started compacting: its time is looked at until it ends.
    fn time_compaction(&mut self, conv: &str) {
        let now = self.secs();
        self.compactions.insert(conv.to_string(), Timed { since: now, looked: now, cut: false });
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts.runtime.spawn(async move {
            loop {
                tokio::time::sleep(EVERY).await;
                let Some(core) = me.upgrade() else { return };
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if !core.look_at_compaction(&conv) {
                    return;
                }
            }
        });
    }

    /// Looks at a running compaction once: tells the person past `NOTICE`, stops it at `DEADLINE`. False once it is over.
    pub(super) fn look_at_compaction(&mut self, conv: &str) -> bool {
        let now = self.secs();
        let compacting = self.convs.get(conv).is_some_and(|c| c.compacting);
        let Some(t) = self.compactions.get_mut(conv).filter(|_| compacting) else {
            self.end_compaction_watch(conv);
            return false;
        };
        let gap = now - std::mem::replace(&mut t.looked, now);
        if gap > 4.0 * EVERY.as_secs_f64() {
            t.since += gap;
        }
        let took = now - t.since;
        if took >= DEADLINE && !std::mem::replace(&mut t.cut, true) {
            // Its plan stays: the end the CLI reports is a compaction that failed (`compaction_ended`), so the context is
            // the one before, and the queue and Keep going go on from there.
            if let Some(l) = self.live.get(conv) {
                l.session.interrupt();
            }
        }
        self.show_compaction(conv, (took >= NOTICE).then(|| (took / 60.0) as u32));
        true
    }

    /// The compaction ended, one way or another: nothing is watched and its notice goes.
    fn end_compaction_watch(&mut self, conv: &str) {
        if self.compactions.remove(conv).is_some() {
            self.show_compaction(conv, None);
        }
    }

    /// The notice of a long compaction, on the reply it follows.
    fn show_compaction(&mut self, conv: &str, minutes: Option<u32>) {
        let stall = minutes.map(|minutes| Stall { kind: StallKind::Compacting, minutes: Some(minutes) });
        let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.last_mut()).filter(|m| m.role == Role::Assistant) else { return };
        if m.stall == stall {
            return;
        }
        m.stall = stall;
        let id = m.id.clone();
        self.push(conv, Some(&id), true);
    }

    /// Keep going is about to send its next turn after `reply_id`: when the context is large enough (`boundary_due`), Faber
    /// compacts it first, with no `next` of its own, and the end of the compaction goes on with the turn (`compaction_ended`
    /// calls `go_on`). Once per turn Faber sent, so a compaction that failed is not asked again in a loop; never before the
    /// check turn, which rereads the original request. True while it compacts.
    pub(super) fn compact_at_boundary(&mut self, conv: &str, reply_id: &str) -> bool {
        // A compaction is a paid turn: a goal's fix with the day's money gone does not start one (`send_next` refuses next).
        if self.goal_out_of_money(conv) {
            return false;
        }
        let Some(c) = self.convs.get(conv) else { return false };
        let Some(kg) = c.keep_going.as_ref() else { return false };
        let turns = kg.turns;
        if kg.checked || self.boundary.get(conv) == Some(&turns) || self.compacts.contains_key(conv) {
            return false;
        }
        let Some(fill) = c.context else { return false };
        let reply = c.messages.iter().rev().find(|m| m.role == Role::Assistant).filter(|m| m.id == reply_id);
        let closed_task = reply.and_then(|m| m.wrap_up.as_ref()).is_some_and(|w| !w.done);
        if !boundary_due(Moment { fill, closed_task }) {
            return false;
        }
        // The compaction runs on the session that holds the context, so it must be one Faber may spend on unattended (a
        // known subscription with room), whatever AI the next turn would go to.
        let Some(l) = self.live.get(conv) else { return false };
        if self.room(l.vendor, l.running.as_deref().or(l.model.as_deref())).is_err() {
            return false;
        }
        self.compacts.insert(conv.to_string(), Plan { next: None, fresh: None });
        if self.compact_due(conv) {
            self.boundary.insert(conv.to_string(), turns);
            return true;
        }
        self.compacts.remove(conv);
        false
    }

    /// The agent's context starts fresh, at once: its session ends, every session of the conversation is forgotten, and the
    /// handoff waits behind existing messages, which enter the fresh session first; Keep going follows the whole queue.
    fn clear(&mut self, conv: &str) {
        let Some(Plan { next: Some(next), .. }) = self.compacts.remove(conv) else { return };
        if let Some(l) = self.retire(conv) {
            l.session.retire();
        }
        let Some(c) = self.convs.get_mut(conv) else { return };
        let requests: Vec<&str> = c.messages.iter().filter(|m| m.role == Role::User && m.wake.is_none()).map(|m| m.text.as_str()).collect();
        let text = sibling::cleared_prompt(&requests, &next);
        c.fresh_after = c.messages.last().map(|m| m.id.clone());
        c.sessions.clear();
        c.tentative.clear();
        c.reported = (None, None);
        c.context = None;
        let choice = c.choice.clone().unwrap_or(Choice { vendor: "auto".into(), model: None, effort: None });
        c.queued.push(QueuedTurn {
            id: new_id(),
            text,
            images: None,
            attachments: None,
            choice,
            at: now(),
            sending: false,
            wake: Some(Wake::of("wake.cleared")),
        });
        c.updated_at = now();
        self.save(conv);
        self.emit_queue(conv);
        (self.emit)(AppEvent::Context { conversation_id: conv.to_string(), context: None });
        self.list_changed();
    }

    /// The CLI is compacting this conversation's context.
    pub(super) fn compacting(&self, conv: &str) -> bool {
        self.convs.get(conv).is_some_and(|c| c.compacting)
    }

    /// Pending work belongs to the conversation and follows its next session. Only a compaction already running ends
    /// with this process: nothing would ever hear its end.
    pub(super) fn retire(&mut self, conv: &str) -> Option<Live> {
        self.settings_changes.remove(conv);
        self.revoke_replacement(conv);
        self.release_codex_preparation(conv);
        let live = self.live.remove(conv)?;
        self.rewind_retire(conv, &live);
        if let Some(c) = self.convs.get_mut(conv).filter(|c| c.compacting) {
            self.compacts.remove(conv);
            c.compacting = false;
            self.list_changed();
        }
        Some(live)
    }

    /// `keep_going_after`, except while the conversation compacts: the end of the compaction goes on from there.
    pub(super) fn keep_going_unless_compacting(&mut self, conv: &str, reply_id: Option<String>) {
        if !self.convs.get(conv).is_some_and(|c| c.compacting) {
            self.keep_going_after(conv, reply_id);
        }
    }

    /// What the CLI reports while it compacts: only its end matters. Whatever else it says belongs to no reply.
    /// True when the event was taken here.
    pub(super) fn compaction_event(&mut self, conv: &str, event: &EngineEvent) -> bool {
        match event {
            // Context news and account news are the same as at any time.
            EngineEvent::Context { .. }
            | EngineEvent::ContextParts { .. }
            | EngineEvent::Compacted { .. }
            | EngineEvent::Quota { .. }
            | EngineEvent::Signals(_) => false,
            EngineEvent::Done { ok, .. } => {
                self.compaction_ended(conv, *ok);
                true
            }
            // The process is gone: the generic handling ends the conversation's work; the compaction is over with it.
            EngineEvent::Error { fatal: true, .. } => {
                self.compacts.remove(conv);
                if let Some(c) = self.convs.get_mut(conv) {
                    c.compacting = false;
                }
                self.end_compaction_watch(conv);
                false
            }
            EngineEvent::Error { .. } => {
                self.compaction_ended(conv, false);
                true
            }
            _ => true,
        }
    }

    /// The CLI finished compacting, or could not. The agent's `next` joins the queue after messages already waiting;
    /// then the queue, Keep going and the report to a parent go on as after any turn.
    fn compaction_ended(&mut self, conv: &str, ok: bool) {
        self.end_compaction_watch(conv);
        let plan = self.compacts.remove(conv);
        let Some(c) = self.convs.get_mut(conv) else { return };
        c.compacting = false;
        // No plan left: Stop took it (`interrupt`). Nothing goes out by itself after a Stop, as after a stopped turn.
        let Some(plan) = plan else {
            c.updated_at = now();
            self.save(conv);
            self.list_changed();
            return;
        };
        if let Some(next) = plan.next {
            let (text, why) = match ok {
                true => (sibling::compacted_prompt(&next), "wake.compacted"),
                false => (sibling::compact_failed_prompt(&next), "wake.compactFailed"),
            };
            let choice = c.choice.clone().unwrap_or(Choice { vendor: "auto".into(), model: None, effort: None });
            c.queued.push(QueuedTurn {
                id: new_id(),
                text,
                images: None,
                attachments: None,
                choice,
                at: now(),
                sending: false,
                wake: Some(Wake::of(why)),
            });
        }
        c.updated_at = now();
        self.save(conv);
        self.drain(conv);
        self.emit_queue(conv);
        self.list_changed();
        let reply = self
            .convs
            .get(conv)
            .filter(|c| c.keep_going.is_some())
            .and_then(|c| c.messages.last())
            .filter(|m| m.role == Role::Assistant)
            .map(|m| m.id.clone());
        match reply {
            Some(reply) => self.go_on(conv, &reply),
            // Nothing follows: a conversation an agent started reports to its parent now.
            None => self.turn_ended(conv),
        }
    }
}

impl Engine {
    /// Looks at every running compaction now, as its timer does every half minute; tests call it after moving the clock.
    pub fn check_compactions(&self) {
        self.with(|c| {
            let convs: Vec<String> = c.compactions.keys().cloned().collect();
            for conv in convs {
                c.look_at_compaction(&conv);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(used: u64, window: u64, closed_task: bool) -> bool {
        boundary_due(Moment { fill: ContextFill { used, window }, closed_task })
    }

    #[test]
    fn keep_going_compacts_after_a_task_at_a_quarter_and_otherwise_at_three_fifths() {
        assert!(!at(99_999, 400_000, true));
        assert!(at(100_000, 400_000, true), "Opus: a quarter of its 400K point once a task closed");
        assert!(!at(239_999, 400_000, false));
        assert!(at(240_000, 400_000, false), "three fifths at any turn");
        // A small context is mostly the model's own base: never worth a summary.
        assert!(!at(79_999, 150_000, true) && at(80_000, 150_000, true));
    }
}
