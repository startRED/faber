//! A run that ended while nobody looked (task 016, the legible agent runs design): the conversation keeps one mark,
//! `Conversation::ended`, from the moment it leaves the running list until the person opens it. The running column draws it
//! as a done card and the sidebar as a dot, both from the conversation list (`Summary::ended`), so they agree and survive a
//! restart. A run the person was there for (their own Stop, an archive) leaves no mark, and a new run takes the old one away.
use super::*;
use crate::model::{Outcome, WrapUp};
use crate::sibling::State;

impl Engine {
    /// The person has the conversation open on a visible window: its done card and its dot go.
    pub fn seen(&self, id: &str) -> Result<(), String> {
        self.with(|c| c.seen(id))
    }
}

impl Core {
    /// Every completion path meets here, even when the turn ran no automatic checks.
    pub(super) fn turn_ended(&mut self, conv: &str) {
        if self.live.get(conv).is_some_and(|l| l.turn.is_some())
            || self.compacts.contains_key(conv)
            || self.compactions.contains_key(conv)
            || self.convs.get(conv).is_none_or(|c| {
                c.compacting
                    || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
                    || c.messages.last().is_some_and(|m| m.permissions.iter().flatten().any(|p| p.tool == "ExitPlanMode"))
            })
        {
            return;
        }
        self.drain(conv);
        self.tell_parent(conv);
    }

    pub(super) fn seen(&mut self, id: &str) -> Result<(), String> {
        let c = self.load(id).ok_or("error.conversationGone")?;
        if c.ended.take().is_none() {
            return Ok(());
        }
        self.save(id);
        (self.emit)(AppEvent::List);
        Ok(())
    }

    /// Reads the running list about to be sent against the last one: a conversation that leaves it ended its run; one back at
    /// work starts a new run and loses its old mark. A reply only closing (`settled`) has not left yet: showing a card before
    /// the turn-end decision would make it come and go.
    pub(super) fn mark_ended(&mut self, items: &[RunningItem]) {
        let at = now();
        let listed = |id: &str| items.iter().any(|i| i.conversation_id == id);
        let mut changed = Vec::new();
        for old in self.running.items.iter().filter(|i| !listed(&i.conversation_id)) {
            let Some(c) = self.convs.get_mut(&old.conversation_id) else { continue };
            // The person's own Stop: they were there.
            if c.stopped {
                continue;
            }
            let Some(kind) = end_kind(c) else { continue };
            let start = old.keep_progress.as_ref().map(|p| p.since).filter(|s| *s > 0).unwrap_or(old.since);
            c.ended = Some(Ended { at, kind, work_ms: at.saturating_sub(start), line: conclusion_line(c) });
            changed.push(old.conversation_id.clone());
        }
        for item in items.iter().filter(|i| !i.settled) {
            let Some(c) = self.convs.get_mut(&item.conversation_id) else { continue };
            if c.ended.take().is_some() {
                changed.push(item.conversation_id.clone());
            }
        }
        if changed.is_empty() {
            return;
        }
        for id in &changed {
            self.save(id);
        }
        (self.emit)(AppEvent::List);
    }
}

/// How the conversation's run ended, or None when it has not ended for the person: they put it away, or Keep going or a
/// waiting message goes on with it.
pub(super) fn end_kind(c: &Conversation) -> Option<EndKind> {
    if c.archived_at.is_some() || c.keep_going.is_some() || !c.queued.is_empty() {
        return None;
    }
    result_kind(c)
}

/// The truth about how the last turn ended, whether or not the conversation was archived or queued. A turn whose process
/// exited without an error is not a conclusion: the end is `Done` only when the agent concluded it (a plain answer in a chat
/// is its own conclusion), and `Unconcluded` otherwise. None while the reply still streams, or when no reply is last.
pub(super) fn result_kind(c: &Conversation) -> Option<EndKind> {
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))?;
    let outcome = reply.wrap_up.as_ref().and_then(WrapUp::outcome);
    Some(if reply.error.is_some() || reply.status == Some(Status::Error) {
        EndKind::Failed
    } else if reply.closed {
        EndKind::Interrupted
    } else if reply.halt.as_deref().is_some_and(|why| !matches!(why, "reason.keepGoing.done" | "reason.keepGoing.silent"))
        || keep_going::unanswered_question(reply)
        || reply.checks.as_ref().is_some_and(|k| k.state == crate::model::ChecksState::NeedsYou)
        || reply.wrap_up.as_ref().is_some_and(keep_going::asks)
        || (matches!(outcome, Some(Outcome::Blocked | Outcome::NeedsYou)) && reply.wrap_up.as_ref().is_some_and(|w| w.answered.is_none()))
    {
        EndKind::NeedsYou
    } else if reply.status == Some(Status::Stopped)
        || reply.halt.as_deref() == Some("reason.keepGoing.silent")
        || fresh_plan(c).is_some_and(|p| p.steps.iter().any(|s| s.state != StepState::Done))
        || (needs_conclusion(c) && outcome != Some(Outcome::Done))
        || outcome.is_some_and(|o| o != Outcome::Done)
        || (reply.wrap_up.is_some() && outcome.is_none())
        // A project turn that ended with no words and no conclusion has nothing a person can read as its answer.
        || (c.area == Area::Project && reply.text.trim().is_empty() && outcome.is_none())
    {
        EndKind::Unconcluded
    } else {
        EndKind::Done
    })
}

/// Where the conversation stands now, from what is stored and whether a turn runs (`busy`). The one answer the sidebar, the
/// running list, another agent's read and the command line share.
pub(super) fn run_state(c: &Conversation, busy: bool) -> State {
    let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant);
    let asking = last.is_some_and(|m| m.permissions.as_ref().is_some_and(|p| !p.is_empty()));
    let working = busy || c.compacting || !c.queued.is_empty() || last.is_some_and(|m| m.status == Some(Status::Streaming));
    match last {
        _ if asking => State::Waiting,
        // Its copy is still being made: the brief follows it.
        _ if c.messages.is_empty() => State::Working,
        _ if working && c.stuck.is_some() => State::Stuck,
        _ if working => State::Working,
        None => State::Unconcluded,
        Some(_) if c.messages.last().is_some_and(|m| m.role == Role::User) => State::Unconcluded,
        Some(m) if m.status == Some(Status::Stopped) && m.closed => State::Interrupted,
        Some(m) if m.status == Some(Status::Stopped) => State::Stopped,
        Some(_) => match result_kind(c) {
            Some(EndKind::Done) => State::Concluded,
            Some(EndKind::NeedsYou) => State::Waiting,
            Some(EndKind::Failed) => State::Failed,
            Some(EndKind::Interrupted) => State::Interrupted,
            Some(EndKind::Unconcluded) | None => State::Unconcluded,
        },
    }
}

/// The plan, when the agent wrote it for the request now under way: a checklist left over from an earlier request says
/// nothing about this one. A request starts at the person's last message; Faber's own wakes belong to the same request.
fn fresh_plan(c: &Conversation) -> Option<&Plan> {
    let start = c.messages.iter().rev().find(|m| m.role == Role::User && m.wake.is_none()).map_or(0, |m| m.at);
    c.plan.as_ref().filter(|p| p.at >= start)
}

/// Whether the request did work that wants a conclusion: a file was edited, checks ran or a plan was kept. An answer built from
/// reading, searching or a command that changed nothing is its own conclusion. Wakes belong to the same request: a closing
/// reply cannot erase the work before it.
fn needs_conclusion(c: &Conversation) -> bool {
    c.area == Area::Project
        && (fresh_plan(c).is_some()
            || c.messages.iter().rev().take_while(|m| m.role != Role::User || m.wake.is_some()).any(|m| {
                m.role == Role::Assistant
                    && (m.checks.is_some()
                        || m.tools
                            .as_ref()
                            .is_some_and(|t| t.iter().any(|t| !t.declined && t.ok != Some(false) && !crate::nudge::edited(t).is_empty())))
            }))
}

/// The line a card or a row shows for the end: the closing call's result and what is left, with the plan steps not marked done.
pub(super) fn conclusion_line(c: &Conversation) -> Option<String> {
    let pending = fresh_plan(c)
        .map(|p| p.steps.iter().filter(|s| s.state != StepState::Done).map(|s| s.text.as_str()).collect::<Vec<_>>().join("; "))
        .filter(|s| !s.is_empty());
    let recap = c.messages.last().and_then(|m| m.wrap_up.as_ref()).map(|w| crate::progress::first_sentence(&w.recap));
    match (recap, pending) {
        (Some(recap), Some(pending)) => Some(format!("{recap} · {pending}")),
        (Some(text), None) | (None, Some(text)) => Some(text),
        (None, None) => keep_going::progress_line(&c.messages),
    }
}

/// What Faber says, once, to an agent whose turn ended with work done and no conclusion.
const CONCLUDE: &str = "Your last turn ended without concluding the task. Check what really got done, then call wrap_up with \
the outcome: done only if the whole request is finished and you verified it, blocked if you cannot go on, needs_you if the next \
move is the person's. Say what is not done or not verified. Then write your closing message to the person.";

impl Core {
    /// One bounded wake, through the ordinary metered continuation path. A refusal is not retried.
    pub(super) fn request_conclusion(&mut self, conv: &str) {
        // A compaction coming carries the agent on with its own next step: that turn is the follow-up.
        if self.live.get(conv).is_some_and(|l| l.turn.is_some()) || self.compacts.contains_key(conv) || self.compactions.contains_key(conv)
        {
            return;
        }
        // Faber spends only on a subscription it knows, as Keep going does: an account not read yet, billed per use or out of
        // quota gets no request (it would only start a turn that fails), and the conversation stays honestly unconcluded.
        if self.room_refusal(conv).is_some() {
            return;
        }
        let Some(c) = self.convs.get_mut(conv) else { return };
        if c.stopped
            || c.archived_at.is_some()
            || c.keep_going.is_some()
            || !c.queued.is_empty()
            || c.compacting
            || result_kind(c) != Some(EndKind::Unconcluded)
            || !needs_conclusion(c)
        {
            return;
        }
        // Once per request: any earlier ask since the person's last message counts, whatever Faber wrote in between.
        let asked = c
            .messages
            .iter()
            .rev()
            .take_while(|m| m.role != Role::User || m.wake.is_some())
            .any(|m| m.conclusion_requested || m.wake.as_ref().is_some_and(|w| w.why == "wake.conclusion"));
        let Some(reply) = c.messages.last_mut() else { return };
        if asked || reply.status != Some(Status::Done) {
            return;
        }
        reply.conclusion_requested = true;
        self.save(conv);
        let _ = self.send_by_itself(conv, CONCLUDE.into(), Wake::of("wake.conclusion"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn turn_end_drains_queue_without_checks_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Default::default() }).unwrap();
        let sent = Arc::new(Mutex::new(Vec::new()));
        let capture = sent.clone();
        let id = super::super::answer::tests::seed(
            &engine,
            Session::from_fn(move |command| {
                if let crate::events::SessionCommand::Send(turn) = command {
                    capture.lock().unwrap().push(turn.text);
                }
            }),
            &[("reply", &[])],
        );
        engine.with(|core| {
            core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
            core.ledger.claude.billing = Billing::Subscription;
            let cwd = core.cwd_of(&core.convs[&id]).unwrap();
            let live = core.live.get_mut(&id).unwrap();
            live.cwd = cwd;
            live.model = Some(crate::router::default_model(Vendor::Claude, None, &[]));
            live.tuning = core.store.settings().tuning();
            let choice = Choice { vendor: "claude".into(), model: None, effort: None };
            let c = core.convs.get_mut(&id).unwrap();
            c.choice = Some(choice.clone());
            c.messages.last_mut().unwrap().status = Some(Status::Done);
            c.queued.push(QueuedTurn {
                id: "queued".into(),
                text: "Follow up".into(),
                images: None,
                attachments: None,
                choice,
                at: now(),
                sending: false,
                wake: None,
            });
            core.end_turn(&id, Some("reply".into()), None);
            core.turn_ended(&id);
            assert!(core.convs[&id].queued.is_empty());
            assert_eq!(core.convs[&id].messages.iter().filter(|m| m.id == "queued").count(), 1);
            assert!(core.convs[&id].messages.iter().all(|m| m.checks.is_none()));
            assert_eq!(core.convs[&id].messages.last().unwrap().status, Some(Status::Streaming));
            assert_eq!(sent.lock().unwrap().len(), 1, "repeated completion cannot dispatch twice");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_reply_that_never_started_reports_its_failed_end_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let parent = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("parent", &[])]);
        let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
        engine.with(|core| {
            core.live.remove(&child);
            core.convs.get_mut(&child).unwrap().started_by = Some(parent.clone());
            core.fail_reply(&child, "child", "error.turnFailed".into());
            assert_eq!(core.convs[&parent].queued.len(), 1, "the failed-start boundary itself delivers the report");
            core.turn_ended(&child);
            let reports: Vec<_> =
                core.convs[&parent].queued.iter().filter(|q| q.wake.as_ref().is_some_and(|w| w.why == "wake.childDone")).collect();
            assert_eq!(reports.len(), 1);
            assert_eq!(result_kind(&core.convs[&child]), Some(EndKind::Failed));
            assert!(reports[0].text.contains("stopped with an error in its latest turn"), "{}", reports[0].text);
        });
        engine.shutdown();
    }

    fn conv(messages: Vec<Message>) -> Conversation {
        let mut c: Conversation = serde_json::from_value(json!({
            "id": "c", "title": "Fix the login page", "area": "project", "createdAt": 0, "updatedAt": 0, "messages": []
        }))
        .unwrap();
        c.messages = messages;
        c
    }

    fn reply(status: Status) -> Message {
        let mut m = Message::new("r".into(), Role::Assistant, "Fixed.".into(), 0);
        m.status = Some(status);
        m
    }

    /// A finished reply that edited a file, as a project task does.
    fn worked() -> Message {
        let mut m = reply(Status::Done);
        m.tools = Some(vec![serde_json::from_value(json!({"id":"t","name":"Edit","input":{"file_path":"src/login.rs"}})).unwrap()]);
        m
    }

    fn person() -> Message {
        Message::new("u".into(), Role::User, "Fix it".into(), 0)
    }

    fn concluded(outcome: &str) -> crate::model::WrapUp {
        crate::progress::wrap_up(&json!({
            "outcome": outcome, "did": "Fixed the redirect.", "result": "Login works.", "verified": "Ran the login test: green."
        }))
        .unwrap()
    }

    #[test]
    fn a_tool_turn_that_exited_well_is_not_done_until_the_agent_concludes() {
        let mut m = worked();
        assert_eq!(end_kind(&conv(vec![person(), m.clone()])), Some(EndKind::Unconcluded), "the process ended well, nothing concluded");
        m.wrap_up = Some(concluded("done"));
        assert_eq!(end_kind(&conv(vec![person(), m.clone()])), Some(EndKind::Done));
        m.wrap_up = Some(concluded("blocked"));
        assert_eq!(end_kind(&conv(vec![person(), m.clone()])), Some(EndKind::NeedsYou), "blocked waits for the person");
        m.wrap_up = Some(concluded("needs_you"));
        assert_eq!(end_kind(&conv(vec![person(), m.clone()])), Some(EndKind::NeedsYou));
        m.wrap_up.as_mut().unwrap().answered = Some("Yes".into());
        assert_eq!(end_kind(&conv(vec![person(), m])), Some(EndKind::Unconcluded), "an answered box concluded nothing yet");
    }

    #[test]
    fn a_plain_answer_is_its_own_conclusion_but_tool_work_in_a_project_is_not() {
        let mut c = conv(vec![person(), reply(Status::Done)]);
        assert_eq!(end_kind(&c), Some(EndKind::Done), "an answer with no tool work is a conclusion in itself");
        c.messages[1] = worked();
        c.area = Area::Chat;
        assert_eq!(end_kind(&c), Some(EndKind::Done));
        c.area = Area::Project;
        assert_eq!(end_kind(&c), Some(EndKind::Unconcluded));
    }

    #[test]
    fn a_project_reply_with_no_words_and_no_conclusion_is_not_an_answer() {
        let mut empty = reply(Status::Done);
        empty.text.clear();
        let mut c = conv(vec![person(), empty]);
        assert_eq!(end_kind(&c), Some(EndKind::Unconcluded), "nothing to read, nothing concluded");
        c.area = Area::Chat;
        assert_eq!(end_kind(&c), Some(EndKind::Done));
    }

    #[test]
    fn unfinished_plan_steps_keep_a_done_claim_from_standing() {
        let mut m = worked();
        m.wrap_up = Some(concluded("done"));
        let mut c = conv(vec![person(), m]);
        c.plan = Some(crate::progress::plan(None, &json!({"steps":[{"text":"Check on a phone","state":"pending"}]}), 0).unwrap().unwrap());
        assert_eq!(end_kind(&c), Some(EndKind::Unconcluded));
        let line = conclusion_line(&c).unwrap();
        assert!(line.contains("Login works") && line.contains("Check on a phone"), "{line}");
    }

    #[test]
    fn a_turn_cut_by_faber_or_stopped_or_silent_is_never_done() {
        let mut cut = reply(Status::Stopped);
        cut.closed = true;
        assert_eq!(end_kind(&conv(vec![person(), cut])), Some(EndKind::Interrupted));
        assert_eq!(
            end_kind(&conv(vec![person(), reply(Status::Stopped)])),
            Some(EndKind::Unconcluded),
            "a turn that stopped for another reason"
        );
        let mut silent = reply(Status::Done);
        silent.halt = Some("reason.keepGoing.silent".into());
        assert_eq!(end_kind(&conv(vec![person(), silent])), Some(EndKind::Unconcluded));
        assert_eq!(end_kind(&conv(vec![person(), reply(Status::Streaming)])), None);
    }

    #[test]
    fn a_run_ends_done_failed_or_needing_the_person() {
        assert_eq!(end_kind(&conv(vec![person(), reply(Status::Done)])), Some(EndKind::Done));
        let mut failed = reply(Status::Error);
        failed.error = Some("error.turnFailed".into());
        assert_eq!(end_kind(&conv(vec![person(), failed])), Some(EndKind::Failed));
        let mut capped = reply(Status::Done);
        capped.halt = Some("reason.keepGoing.cap".into());
        assert_eq!(end_kind(&conv(vec![person(), capped])), Some(EndKind::NeedsYou));
        let mut done = reply(Status::Done);
        done.halt = Some("reason.keepGoing.done".into());
        assert_eq!(end_kind(&conv(vec![person(), done])), Some(EndKind::Done));
        // A closing box that asks the person something (the older shape) waits on them until it is answered.
        let mut boxed = reply(Status::Done);
        let ask = crate::model::WrapUp {
            recap: "Fixed.".into(),
            ask: Some("Ship it?".into()),
            actions: vec!["Yes".into()],
            ..Default::default()
        };
        boxed.wrap_up = Some(ask.clone());
        assert_eq!(end_kind(&conv(vec![person(), boxed.clone()])), Some(EndKind::NeedsYou));
        boxed.wrap_up = Some(crate::model::WrapUp { answered: Some("Yes".into()), ..ask });
        assert_eq!(end_kind(&conv(vec![person(), boxed])), Some(EndKind::Unconcluded));
        let mut archived = conv(vec![person(), reply(Status::Done)]);
        archived.archived_at = Some(1);
        assert_eq!(end_kind(&archived), None);
    }

    #[test]
    fn every_state_has_one_answer_for_the_sidebar_the_list_and_other_agents() {
        let mut m = worked();
        let mut c = conv(vec![person(), m.clone()]);
        assert_eq!(run_state(&c, false), State::Unconcluded);
        m.wrap_up = Some(concluded("done"));
        c.messages[1] = m.clone();
        assert_eq!(run_state(&c, false), State::Concluded);
        m.wrap_up = Some(concluded("blocked"));
        c.messages[1] = m.clone();
        assert_eq!(run_state(&c, false), State::Waiting);
        c.messages[1] = reply(Status::Streaming);
        assert_eq!(run_state(&c, false), State::Working, "a saved streaming reply is a turn in progress");
        c.stuck = Some(crate::results::Stuck { noticed_at: 1, since: 1, turns: 3, wall_ms: 0, usd: 0.0, asked_at: None, ask_turns: 0 });
        assert_eq!(run_state(&c, true), State::Stuck, "spending without a result is never working");
        c.stuck = None;
        let mut cut = reply(Status::Stopped);
        cut.closed = true;
        c.messages[1] = cut;
        assert_eq!(run_state(&c, false), State::Interrupted);
        c.messages[1] = reply(Status::Stopped);
        assert_eq!(run_state(&c, false), State::Stopped);
        let mut failed = reply(Status::Error);
        failed.error = Some("error.turnFailed".into());
        c.messages[1] = failed;
        assert_eq!(run_state(&c, false), State::Failed);
        c.messages.truncate(1);
        assert_eq!(run_state(&c, false), State::Unconcluded, "a request nothing answered");
        assert_eq!(run_state(&conv(vec![]), false), State::Working, "a conversation whose first turn is being made");
    }

    #[test]
    fn a_card_nobody_opens_goes_after_a_week_and_a_failure_after_a_day() {
        let day = 86_400_000;
        let mark = |kind| Ended { at: 0, kind, work_ms: 0, line: None };
        assert!(mark(EndKind::Done).current(6 * day) && !mark(EndKind::Done).current(7 * day));
        assert!(mark(EndKind::NeedsYou).current(6 * day));
        assert!(mark(EndKind::Failed).current(day - 1) && !mark(EndKind::Failed).current(day));
    }

    #[test]
    fn a_stored_end_from_before_conclusions_still_loads() {
        // Written by an older Faber: no outcome, no summary, no stuck mark, a `done` end kind.
        let old: Conversation = serde_json::from_value(json!({
            "id": "c", "title": "Old", "area": "project", "createdAt": 0, "updatedAt": 0,
            "ended": {"at": 5, "kind": "done", "workMs": 9},
            "messages": [{"id":"r","role":"assistant","text":"Fixed.","at":0,"status":"done",
                          "wrapUp":{"recap":"Fixed it.","done":true}}]
        }))
        .unwrap();
        assert_eq!(old.ended.as_ref().map(|e| e.kind), Some(EndKind::Done));
        let w = old.messages[0].wrap_up.as_ref().unwrap();
        assert_eq!((w.outcome(), w.summary.is_some()), (Some(Outcome::Done), false), "an older done box reads as a done conclusion");
        assert_eq!(result_kind(&old), Some(EndKind::Done));
        assert_eq!(run_state(&old, false), State::Concluded);
    }
}
