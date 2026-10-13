//! Answering a permission question. The engine hands the answer to the session, waits for the session to say the CLI
//! has it, and takes the question off the list only then, so the user is never told a request was answered when it was
//! not.

use super::*;
use crate::events::Delivery;
use tokio::sync::oneshot;
use tokio::time::{Instant, timeout_at};

/// How long the engine waits, in all, for the sessions to confirm one answer.
const ACK_WAIT: Duration = Duration::from_secs(5);

const STALE: &str = "error.permission.stale";
const UNDELIVERED: &str = "error.permission.undelivered";

/// An answer given to the sessions and not yet settled.
pub(super) struct Handed {
    /// Each request answered with the receiver of its acknowledgement. The request the user clicked comes first.
    requests: Vec<(String, oneshot::Receiver<Delivery>)>,
    /// The tool of the request the user clicked.
    tool: String,
    /// The process the answer went to; settling checks it is still the one in use.
    sid: u64,
}

impl Engine {
    /// Answers one question of one turn. The call returns once the CLI has the answer (`Ok`), or says why not:
    /// `error.permission.stale` when the question is not waiting any more, `error.permission.undelivered` when the
    /// answer could not be written or was not confirmed in time. The lock is free while the engine waits, so nothing
    /// else in the engine waits for the CLI.
    pub async fn answer(&self, conv: &str, turn_id: &str, request_id: &str, allow: bool, always: bool) -> Result<(), String> {
        let Handed { requests, tool, sid } = self.with(|c| c.prepare_answer(conv, turn_id, request_id, allow, always))?;
        let deadline = Instant::now() + ACK_WAIT;
        let mut results = Vec::new();
        for (id, ack) in requests {
            // A queue that closed, or a session that said nothing in time, leaves the request waiting.
            results.push((id, timeout_at(deadline, ack).await.ok().and_then(|r| r.ok())));
        }
        self.with(|c| c.settle_answer(conv, turn_id, sid, &tool, allow && always, &results))
    }
}

impl Core {
    /// Checks that the conversation is live, that `turn_id` is its current turn and that the request waits in it, then
    /// hands the answer to the session. It changes no permission. "Always" also answers the other waiting requests for
    /// the same tool.
    fn prepare_answer(&mut self, conv: &str, turn_id: &str, request_id: &str, allow: bool, always: bool) -> Result<Handed, String> {
        let live = self.live.get(conv).filter(|l| l.turn.as_deref() == Some(turn_id)).ok_or(STALE)?;
        let reply = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == turn_id)).ok_or(STALE)?;
        let waiting = reply.permissions.as_deref().unwrap_or_default();
        let tool = waiting.iter().find(|p| p.request_id == request_id).ok_or(STALE)?.tool.clone();
        let others = waiting.iter().filter(|p| allow && always && p.tool == tool && p.request_id != request_id);
        let ids = std::iter::once(request_id).chain(others.map(|p| p.request_id.as_str()));
        let requests = ids.map(|id| (id.to_string(), live.session.answer(id, allow))).collect();
        Ok(Handed { requests, tool, sid: live.sid })
    }

    /// Applies what the sessions said. Other calls ran while the engine waited, so it looks again for the conversation,
    /// the process and the turn: if the turn ended or the process was replaced meanwhile, the answer is stale and
    /// nothing changes. A request leaves the list when the CLI took the answer or no longer waits for it; one that was not
    /// delivered stays, so the user can answer again. With `remember`, the tool joins the conversation's allow list once
    /// the clicked request was delivered, and a question of that tool that came in during the wait is answered at once.
    /// The result is the clicked request's, the first of `results`.
    fn settle_answer(
        &mut self,
        conv: &str,
        turn_id: &str,
        sid: u64,
        tool: &str,
        remember: bool,
        results: &[(String, Option<Delivery>)],
    ) -> Result<(), String> {
        self.live.get(conv).filter(|l| l.sid == sid && l.turn.as_deref() == Some(turn_id)).ok_or(STALE)?;
        let c = self.convs.get_mut(conv).ok_or(STALE)?;
        let reply = c.messages.iter_mut().find(|m| m.id == turn_id).ok_or(STALE)?;
        let clicked = results.first().and_then(|r| r.1);
        let over = |id: &str| results.iter().any(|(r, d)| r == id && matches!(d, Some(Delivery::Delivered | Delivery::Unknown)));
        let (mut changed, mut late) = (false, Vec::new());
        if let Some(waiting) = reply.permissions.as_mut() {
            let before = waiting.len();
            waiting.retain(|p| !over(&p.request_id));
            changed = waiting.len() != before;
        }
        if remember && clicked == Some(Delivery::Delivered) {
            // The tool is allowed from now on, so a question that came in while the engine waited gets its answer too.
            if let Some(waiting) = reply.permissions.as_mut() {
                (late, *waiting) = std::mem::take(waiting).into_iter().partition(|p| p.tool == tool);
                changed |= !late.is_empty();
            }
            let list = c.allow_always.get_or_insert_default();
            if !list.iter().any(|t| t == tool) {
                list.push(tool.to_string());
                changed = true;
            }
        }
        if changed {
            self.save(conv);
            self.push(conv, Some(turn_id), true);
        }
        self.auto_answer(conv, late);
        match clicked {
            Some(Delivery::Delivered) => Ok(()),
            Some(Delivery::Unknown) => Err(STALE.into()),
            Some(Delivery::WriteFailed) | None => Err(UNDELIVERED.into()),
        }
    }
}

impl Core {
    /// Allows the questions without asking the user. The answers are watched without the lock: one the CLI did not
    /// take, or did not confirm in time, becomes a question on the list again, so the turn is never left blocked behind
    /// a row that says it is working.
    pub(super) fn auto_answer(&self, conv: &str, asks: Vec<Permission>) {
        let Some(l) = self.live.get(conv).filter(|_| !asks.is_empty()) else { return };
        let (sid, turn) = (l.sid, l.turn.clone());
        let sent: Vec<_> = asks.into_iter().map(|p| (l.session.answer(&p.request_id, true), p)).collect();
        let (me, conv) = (self.me.clone(), conv.to_string());
        tokio::spawn(async move {
            let deadline = Instant::now() + ACK_WAIT;
            let mut failed = Vec::new();
            for (ack, p) in sent {
                // `Unknown` means the CLI no longer waits for it, so there is nothing to ask.
                if !matches!(timeout_at(deadline, ack).await, Ok(Ok(Delivery::Delivered | Delivery::Unknown))) {
                    failed.push(p);
                }
            }
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).restore_questions(&conv, sid, turn, failed);
        });
    }

    /// Puts questions the CLI did not take back on the turn's list, if that turn is still the one running.
    fn restore_questions(&mut self, conv: &str, sid: u64, turn: Option<String>, asks: Vec<Permission>) {
        let live = self.live.get(conv).is_some_and(|l| l.sid == sid && l.turn == turn);
        let Some(turn) = turn.filter(|_| live && !asks.is_empty()) else { return };
        let Some(reply) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == turn)) else { return };
        let waiting = reply.permissions.get_or_insert_default();
        for p in asks {
            if !waiting.iter().any(|w| w.request_id == p.request_id) {
                waiting.push(p);
            }
        }
        self.push(conv, Some(&turn), true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use crate::events::SessionCommand;
    use serde_json::json;
    use std::collections::VecDeque;

    /// What a stand-in session does with the next answer it is given.
    enum Reply {
        Say(Delivery),
        /// The queue is closed: the command, and the acknowledgement in it, are dropped.
        Closed,
        /// The session takes the command and says nothing yet; the test holds the acknowledgement.
        Hold,
    }

    /// A session the test controls, and what it was asked.
    struct Stub {
        session: Session,
        asked: Arc<Mutex<Vec<String>>>,
        held: Arc<Mutex<Vec<oneshot::Sender<Delivery>>>>,
    }

    /// A stand-in that follows `script`, one reply per answer, and then says `Delivered`.
    fn stub(script: Vec<Reply>) -> Stub {
        let (asked, held) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
        let (log, keep, script) = (asked.clone(), held.clone(), Mutex::new(VecDeque::from(script)));
        let session = Session::from_fn(move |cmd| {
            if let SessionCommand::Answer { request_id, ack, .. } = cmd {
                log.lock().unwrap().push(request_id);
                match script.lock().unwrap().pop_front().unwrap_or(Reply::Say(Delivery::Delivered)) {
                    Reply::Say(delivery) => {
                        let _ = ack.send(delivery);
                    }
                    Reply::Closed => {}
                    Reply::Hold => keep.lock().unwrap().push(ack),
                }
            }
        });
        Stub { session, asked, held }
    }

    fn engine(dir: &Path) -> (Engine, Arc<Mutex<Vec<AppEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let opts = EngineOptions {
            data_dir: dir.into(),
            clis: BTreeMap::new(),
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no process in this test"))),
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        };
        (Engine::new(opts, Arc::new(move |e| sink.lock().unwrap().push(e))), events)
    }

    /// A chat whose replies are `turns`, each a message id with the (request id, tool) pairs it waits on. The last is
    /// the turn in flight, on a live `session`.
    fn seed(engine: &Engine, session: Session, turns: &[(&str, &[(&str, &str)])]) -> String {
        engine.with(|c| {
            let id = c.blank(Area::Chat, None);
            let conv = c.convs.get_mut(&id).unwrap();
            conv.title = "Seeded".into();
            for (n, (turn, waiting)) in turns.iter().enumerate() {
                conv.messages.push(Message::new(format!("question-{n}"), Role::User, "question".into(), 1));
                let mut reply = Message::new((*turn).into(), Role::Assistant, String::new(), 2 + n as u64);
                reply.vendor = Some(Vendor::Claude);
                reply.status = Some(if n + 1 == turns.len() { Status::Streaming } else { Status::Done });
                let ask = |(request_id, tool): &(&str, &str)| Permission {
                    request_id: request_id.to_string(),
                    tool: tool.to_string(),
                    input: json!({}),
                };
                reply.permissions = (!waiting.is_empty()).then(|| waiting.iter().map(ask).collect());
                conv.messages.push(reply);
            }
            let live = Live {
                vendor: Vendor::Claude,
                model: None,
                running: None,
                effort: "medium".into(),
                sid: 1,
                session,
                turn: turns.last().map(|t| t.0.to_string()),
                break_pending: false,
                answer_started: false,
                used: 0,
                cli: Cli { command: "claude".into(), prefix: vec![] },
                resumed: false,
                permissions: Permissions::Ask,
                by_hand: true,
                approved: false,
                started: 0.0,
                served: false,
                steering: vec![],
            };
            c.live.insert(id.clone(), live);
            c.refresh_running();
            id
        })
    }

    /// The request ids waiting in the message `turn`.
    fn waiting(engine: &Engine, conv: &str, turn: &str) -> Vec<String> {
        let reply = engine.open(conv).unwrap().messages.into_iter().find(|m| m.id == turn).unwrap();
        reply.permissions.unwrap_or_default().into_iter().map(|p| p.request_id).collect()
    }

    fn revision(engine: &Engine) -> u64 {
        engine.state().running.revision
    }

    fn running_events(events: &Mutex<Vec<AppEvent>>) -> usize {
        events.lock().unwrap().iter().filter(|e| matches!(e, AppEvent::Running { .. })).count()
    }

    #[tokio::test]
    async fn a_delivered_answer_takes_the_question_off_the_list_and_the_row_goes_back_to_working() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, events) = engine(dir.path());
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.state().running.items[0].state, RunState::NeedsYou);
        let before = (revision(&engine), running_events(&events));

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Ok(()));

        assert_eq!(*s.asked.lock().unwrap(), ["0"]);
        assert!(waiting(&engine, &id, "t1").is_empty());
        assert_eq!(engine.state().running.items[0].state, RunState::Working);
        assert_eq!((revision(&engine), running_events(&events)), (before.0 + 1, before.1 + 1));
        assert_eq!(engine.open(&id).unwrap().allow_always, None, "no Always was asked for");
    }

    #[tokio::test]
    async fn a_session_that_does_not_know_the_request_makes_it_stale_and_the_question_goes() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::Unknown)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(STALE.to_string()));
        assert!(waiting(&engine, &id, "t1").is_empty(), "the CLI no longer waits for it");
    }

    #[tokio::test]
    async fn a_write_that_fails_keeps_the_question_and_the_revision_and_a_second_answer_can_land() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, events) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::WriteFailed), Reply::Say(Delivery::Delivered)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let before = (revision(&engine), running_events(&events));

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(UNDELIVERED.to_string()));
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
        assert_eq!((revision(&engine), running_events(&events)), before, "nothing changed, so nothing was said");

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Ok(()));
        assert!(waiting(&engine, &id, "t1").is_empty());
        assert_eq!(revision(&engine), before.0 + 1);
    }

    #[tokio::test]
    async fn a_closed_queue_is_undelivered_and_the_question_stays() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Closed]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(UNDELIVERED.to_string()));
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
    }

    #[tokio::test(start_paused = true)]
    async fn no_acknowledgement_in_five_seconds_is_undelivered_and_a_late_one_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, events) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let before = (revision(&engine), running_events(&events));

        let started = tokio::time::Instant::now();
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(UNDELIVERED.to_string()));
        assert_eq!(started.elapsed(), ACK_WAIT);

        let late = s.held.lock().unwrap().pop().unwrap();
        assert!(late.send(Delivery::Delivered).is_err(), "nobody listens to the late acknowledgement");
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
        assert_eq!((revision(&engine), running_events(&events)), before);
    }

    #[tokio::test]
    async fn the_turn_id_is_checked_so_a_late_answer_never_lands_on_the_next_turn() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![]);
        // Turn 1 ended with request 0 still listed; turn 2 asks its own request 0.
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")]), ("t2", &[("0", "Write")])]);

        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(STALE.to_string()));
        assert!(s.asked.lock().unwrap().is_empty(), "nothing was sent");
        assert_eq!(waiting(&engine, &id, "t2"), ["0"], "turn 2 still waits");

        assert_eq!(engine.answer(&id, "t2", "0", true, false).await, Ok(()));
        assert!(waiting(&engine, &id, "t2").is_empty());
        assert_eq!(*s.asked.lock().unwrap(), ["0"]);
    }

    #[tokio::test]
    async fn an_unknown_conversation_turn_or_request_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        for (conv, turn, request) in [("nope", "t1", "0"), (id.as_str(), "nope", "0"), (id.as_str(), "t1", "9")] {
            assert_eq!(engine.answer(conv, turn, request, true, false).await, Err(STALE.to_string()), "{conv} {turn} {request}");
        }
        assert!(s.asked.lock().unwrap().is_empty());
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
    }

    #[tokio::test]
    async fn always_answers_the_other_waiting_requests_of_the_tool_and_each_has_its_own_acknowledgement() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::Delivered), Reply::Say(Delivery::Unknown)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write"), ("b", "Bash"), ("c", "Write")])]);

        assert_eq!(engine.answer(&id, "t1", "a", true, true).await, Ok(()));

        assert_eq!(*s.asked.lock().unwrap(), ["a", "c"], "the clicked request first, then the other Write");
        assert_eq!(waiting(&engine, &id, "t1"), ["b"], "c was unknown to the CLI, so it goes too");
        assert_eq!(engine.open(&id).unwrap().allow_always, Some(vec!["Write".to_string()]));
    }

    #[tokio::test]
    async fn deny_with_always_answers_only_the_clicked_request_and_remembers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write"), ("c", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "a", false, true).await, Ok(()));
        assert_eq!(*s.asked.lock().unwrap(), ["a"]);
        assert_eq!(waiting(&engine, &id, "t1"), ["c"]);
        assert_eq!(engine.open(&id).unwrap().allow_always, None);
    }

    #[tokio::test]
    async fn an_always_that_was_not_delivered_does_not_join_the_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Say(Delivery::WriteFailed)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "a", true, true).await, Err(UNDELIVERED.to_string()));
        assert_eq!(engine.open(&id).unwrap().allow_always, None);
    }

    #[tokio::test]
    async fn two_clicks_on_the_two_copies_of_a_card_give_one_ok_and_one_stale() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        // Both clicks pass the first check, because the lock is free while they wait. The session handles the commands in
        // order: the first is delivered, the second finds nothing.
        let s = stub(vec![Reply::Hold, Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let release = async {
            while s.held.lock().unwrap().len() < 2 {
                tokio::task::yield_now().await;
            }
            let mut held = s.held.lock().unwrap();
            let _ = held.remove(0).send(Delivery::Delivered);
            let _ = held.remove(0).send(Delivery::Unknown);
        };
        let (first, second, ()) =
            tokio::join!(engine.answer(&id, "t1", "0", true, false), engine.answer(&id, "t1", "0", true, false), release);
        assert_eq!((first, second), (Ok(()), Err(STALE.to_string())));
        assert_eq!(s.asked.lock().unwrap().len(), 2);
        assert!(waiting(&engine, &id, "t1").is_empty());
    }

    #[tokio::test]
    async fn a_conversation_removed_while_the_engine_waits_makes_the_answer_stale_without_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let call = tokio::spawn({
            let (engine, id) = (engine.clone(), id.clone());
            async move { engine.answer(&id, "t1", "0", true, false).await }
        });
        while s.held.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        engine.remove(&id);
        s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();
        assert_eq!(call.await.unwrap(), Err(STALE.to_string()));
        assert!(engine.state().running.items.is_empty());
    }

    #[tokio::test]
    async fn the_engine_is_not_blocked_while_it_waits_for_an_acknowledgement() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let call = tokio::spawn({
            let (engine, id) = (engine.clone(), id.clone());
            async move { engine.answer(&id, "t1", "0", true, false).await }
        });
        while s.held.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        // Another thread uses the engine while the answer waits; if the lock were held, it would not come back.
        let (done, back) = std::sync::mpsc::channel();
        std::thread::spawn({
            let engine = engine.clone();
            move || {
                let listed = engine.state().running.items.len();
                let sent = engine.send(SendRequest {
                    conversation_id: None,
                    area: Area::Chat,
                    project_id: None,
                    text: "hello".into(),
                    images: vec![],
                    files: vec![],
                    choice: Choice { vendor: "claude".into(), model: None, effort: None },
                });
                let _ = done.send((listed, sent));
            }
        });
        let (listed, sent) = back.recv_timeout(Duration::from_secs(5)).expect("state() and send() waited for the answer");
        assert_eq!((listed, sent), (1, Err("error.noSubscription".to_string())));
        s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();
        assert_eq!(call.await.unwrap(), Ok(()));
    }

    /// Hands the engine a question from the CLI, as the session would.
    fn ask(engine: &Engine, conv: &str, request: &str, tool: &str) {
        let event = EngineEvent::Permission { request_id: request.into(), tool: tool.into(), input: json!({}) };
        engine.with(|c| c.handle(conv, 1, event));
    }

    /// Lets the answer wait for its acknowledgement and returns the call.
    async fn waiting_call(engine: &Engine, s: &Stub, id: &str, always: bool) -> tokio::task::JoinHandle<Result<(), String>> {
        let call = tokio::spawn({
            let (engine, id) = (engine.clone(), id.to_string());
            async move { engine.answer(&id, "t1", "0", true, always).await }
        });
        while s.held.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        call
    }

    #[tokio::test]
    async fn a_question_of_the_tool_that_comes_in_while_always_waits_is_answered_too() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let call = waiting_call(&engine, &s, &id, true).await;

        ask(&engine, &id, "late", "Write");
        ask(&engine, &id, "other", "Bash");
        s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();

        assert_eq!(call.await.unwrap(), Ok(()));
        assert_eq!(*s.asked.lock().unwrap(), ["0", "late"], "the late Write was answered, the Bash was not");
        assert_eq!(waiting(&engine, &id, "t1"), ["other"]);
    }

    #[tokio::test]
    async fn an_automatic_approval_the_cli_did_not_take_becomes_a_question_again() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        for reply in [Reply::Say(Delivery::WriteFailed), Reply::Closed] {
            let s = stub(vec![reply]);
            let id = seed(&engine, s.session.clone(), &[("t1", &[])]);
            engine.with(|c| c.convs.get_mut(&id).unwrap().allow_always = Some(vec!["Write".into()]));

            ask(&engine, &id, "7", "Write");
            assert_eq!(*s.asked.lock().unwrap(), ["7"], "answered at once, without a card");

            let mut tries = 0;
            while waiting(&engine, &id, "t1").is_empty() {
                tries += 1;
                assert!(tries < 1000, "the card never came back");
                tokio::task::yield_now().await;
            }
            assert_eq!(waiting(&engine, &id, "t1"), ["7"]);
            assert_eq!(engine.state().running.items[0].state, RunState::NeedsYou);
        }
    }

    #[tokio::test]
    async fn an_automatic_approval_the_cli_took_leaves_no_card() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[])]);
        engine.with(|c| c.convs.get_mut(&id).unwrap().allow_always = Some(vec!["Write".into()]));
        ask(&engine, &id, "7", "Write");
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        assert!(waiting(&engine, &id, "t1").is_empty());
        assert_eq!(engine.state().running.items[0].state, RunState::Working);
    }

    #[tokio::test]
    async fn a_turn_that_ended_during_the_wait_makes_the_answer_stale_and_remembers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        let call = waiting_call(&engine, &s, &id, true).await;

        let done = EngineEvent::Done {
            ok: true,
            reason: String::new(),
            cost_total_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        };
        engine.with(|c| c.handle(&id, 1, done));
        s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();

        assert_eq!(call.await.unwrap(), Err(STALE.to_string()));
        assert_eq!(engine.open(&id).unwrap().allow_always, None);
    }

    #[tokio::test]
    async fn a_process_or_turn_that_replaced_the_one_answered_makes_the_answer_stale_and_remembers_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = engine(dir.path());
        for replace in [|l: &mut Live| l.sid = 2, |l: &mut Live| l.turn = Some("t2".into())] {
            let s = stub(vec![Reply::Hold]);
            let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
            let call = waiting_call(&engine, &s, &id, true).await;

            engine.with(|c| replace(c.live.get_mut(&id).unwrap()));
            s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();

            assert_eq!(call.await.unwrap(), Err(STALE.to_string()));
            assert_eq!(engine.open(&id).unwrap().allow_always, None);
        }
    }
}
