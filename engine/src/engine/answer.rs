//! Answering a permission question. The engine hands the answer to the session, waits for the session to say the CLI
//! has it, and takes the question off the list only then, so the user is never told a request was answered when it was
//! not.

use super::*;
use crate::events::Delivery;
use tokio::sync::oneshot;
use tokio::time::{Instant, timeout_at};

/// How long the engine waits, in all, for the sessions to confirm one answer.
const ACK_WAIT: Duration = Duration::from_secs(5);

pub(super) const STALE: &str = "error.permission.stale";
const UNDELIVERED: &str = "error.permission.undelivered";

/// An answer given to the sessions and not yet settled.
pub(super) struct Handed {
    /// Each request answered with the receiver of its acknowledgement. The request the user clicked comes first.
    requests: Vec<(String, oneshot::Receiver<Delivery>)>,
    /// The tool of the request the user clicked.
    tool: String,
    /// The process the answer went to; settling checks it is still the one in use.
    sid: u64,
    /// "Always allow" holds: not in Plan, which takes no remembered grant.
    always: bool,
    /// The grants epoch the clicked card was made under.
    epoch: Option<String>,
}

impl Engine {
    /// Answers one question of one turn. The call returns once the CLI has the answer (`Ok`), or says why not:
    /// `error.permission.stale` when the question is not waiting any more, `error.permission.undelivered` when the
    /// answer could not be written or was not confirmed in time. The lock is free while the engine waits, so nothing
    /// else in the engine waits for the CLI.
    pub async fn answer(&self, conv: &str, turn_id: &str, request_id: &str, allow: bool, always: bool) -> Result<(), String> {
        // Faber's own self-change card (an install) is no request of the CLI: nothing is written to it, so the answer is settled here.
        if let Some(settled) = self.with(|c| c.answer_self_change(conv, turn_id, request_id, allow, always)) {
            return settled;
        }
        let plan = self.with(|c| {
            c.convs
                .get(conv)
                .and_then(|c| c.messages.last())
                .filter(|m| m.id == turn_id)
                .and_then(|m| m.permissions.iter().flatten().find(|p| p.request_id == request_id && p.tool == "ExitPlanMode"))
                .map(|p| serde_json::from_value::<Permissions>(p.input["mode"].clone()).unwrap_or(Permissions::Ask))
        });
        if let Some(mode) = plan {
            return self.approve_plan(conv, turn_id, request_id, allow.then_some(mode)).await;
        }
        self.answer_with(conv, turn_id, request_id, allow, always, None).await
    }

    /// Only the person can approve a plan. None keeps planning; a chosen mode approves and starts the work.
    pub async fn approve_plan(&self, conv: &str, turn_id: &str, request_id: &str, mode: Option<Permissions>) -> Result<(), String> {
        if matches!(mode, Some(Permissions::Plan | Permissions::Isolated)) {
            return Err(STALE.into());
        }
        if mode.is_none() && self.with(|c| c.plan_card(conv, turn_id, request_id).map(|(vendor, _)| vendor == Vendor::Claude))? {
            return self.answer_with(conv, turn_id, request_id, false, false, None).await;
        }
        self.with(|c| {
            let (vendor, revision) = c.plan_card(conv, turn_id, request_id)?;
            if mode.is_none() {
                c.remove_plan_card(conv, turn_id, request_id);
                c.save(conv);
                c.push(conv, Some(turn_id), true);
                c.drain(conv);
                return Ok(());
            }
            if revision != Some(serde_json::json!(c.store.settings().permissions_revision)) {
                return Err("error.permission.planChanged".into());
            }
            c.check_plan_send(conv)?;
            if c.convs.get(conv).is_some_and(|c| c.queued.len() >= MAX_QUEUED) {
                return Err("error.queueFull".into());
            }
            if vendor == Vendor::Claude {
                let live = c.live.get(conv).filter(|l| l.turn.as_deref() == Some(turn_id)).ok_or(STALE)?;
                // The permission answer only exits planning. Work belongs to a new ordinary turn.
                drop(live.session.answer(request_id, true));
                live.session.interrupt();
                live.session.close();
                c.stop_helpers(conv, false);
            }
            c.remove_plan_card(conv, turn_id, request_id);
            if let Some(m) = c.convs.get_mut(conv).and_then(|c| c.messages.last_mut()) {
                m.status = Some(Status::Done);
                m.permissions = None;
                m.finished_at.get_or_insert_with(now);
            }
            // The closing turn still reports its counters through the ordinary Done event.
            if let Err(why) = c.save_settings(Settings { permissions: mode, ..Settings::default() }) {
                if let Some(m) = c.convs.get_mut(conv).and_then(|c| c.messages.last_mut()) {
                    m.error = Some(why.clone());
                }
                c.save(conv);
                c.push(conv, Some(turn_id), true);
                return Err(why);
            }
            c.send_by_itself(conv, "Go ahead with the plan.".into(), crate::model::Wake::of("wake.approvedPlan"))?;
            c.drain(conv);
            c.save(conv);
            c.push(conv, Some(turn_id), true);
            Ok(())
        })
    }

    /// Answers a question the agent asked the user (`AskUserQuestion`) with what the user chose: each question's text to the
    /// label picked (several labels joined by a comma). It never remembers anything.
    pub async fn answer_question(&self, conv: &str, turn_id: &str, request_id: &str, answers: serde_json::Value) -> Result<(), String> {
        self.answer_with(conv, turn_id, request_id, true, false, Some(answers)).await
    }

    async fn answer_with(
        &self,
        conv: &str,
        turn_id: &str,
        request_id: &str,
        allow: bool,
        always: bool,
        reply: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let Handed { requests, tool, sid, always, epoch } =
            self.with(|c| c.prepare_answer(conv, turn_id, request_id, allow, always, reply))?;
        let deadline = Instant::now() + ACK_WAIT;
        let mut results = Vec::new();
        for (id, ack) in requests {
            // A queue that closed, or a session that said nothing in time, leaves the request waiting.
            results.push((id, timeout_at(deadline, ack).await.ok().and_then(|r| r.ok())));
        }
        let result = self.with(|c| c.settle_answer(conv, turn_id, sid, &tool, (allow && always).then_some(epoch), &results));
        // Claude may finish immediately after taking a planning denial; its confirmed delivery still succeeds.
        if matches!(&result, Err(why) if why == STALE)
            && !allow
            && tool == "ExitPlanMode"
            && matches!(results.first(), Some((_, Some(Delivery::Delivered))))
        {
            Ok(())
        } else {
            result
        }
    }
}

impl Core {
    /// Once another turn starts, an earlier Codex plan is no longer waiting for approval, including after restart.
    pub(super) fn clear_older_plan_cards(&mut self, conv: &str, reply: &str) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        let mut changed = Vec::new();
        for m in c.messages.iter_mut().filter(|m| m.id != reply && m.vendor == Some(Vendor::Codex)) {
            if let Some(waiting) = m.permissions.as_mut() {
                let before = waiting.len();
                waiting.retain(|p| p.tool != "ExitPlanMode");
                if waiting.len() != before {
                    changed.push(m.id.clone());
                }
            }
        }
        if !changed.is_empty() {
            self.save(conv);
            for id in changed {
                self.push(conv, Some(&id), true);
            }
        }
    }

    fn plan_card(&mut self, conv: &str, turn: &str, request: &str) -> Result<(Vendor, Option<serde_json::Value>), String> {
        let m = self.load(conv).and_then(|c| c.messages.last()).filter(|m| m.id == turn).ok_or(STALE)?;
        let p = m.permissions.iter().flatten().find(|p| p.tool == "ExitPlanMode" && p.request_id == request).ok_or(STALE)?;
        Ok((m.vendor.ok_or(STALE)?, p.input.get("modeRevision").cloned()))
    }

    fn remove_plan_card(&mut self, conv: &str, turn: &str, request: &str) {
        if let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == turn))
            && let Some(waiting) = m.permissions.as_mut()
        {
            waiting.retain(|p| p.request_id != request);
        }
    }

    fn check_plan_send(&self, conv: &str) -> Result<(), String> {
        if self.installing {
            return Err("error.update.activeOperations".into());
        }
        let conversation = self.convs.get(conv).ok_or(STALE)?;
        let choice = conversation.choice.as_ref().ok_or(STALE)?;
        let pick = pick_model(choice, &self.vendors(), self.codex_recommended.as_deref()).ok_or("error.noSubscription")?;
        if choice.vendor != "auto" {
            self.gate_turn(&pick, true)?;
        }
        self.cwd_of(conversation)?;
        Ok(())
    }

    /// Checks that the conversation is live, that `turn_id` is its current turn and that the request waits in it, then
    /// hands the answer to the session. It changes no permission. "Always" also answers the other waiting requests for
    /// the same tool.
    fn prepare_answer(
        &mut self,
        conv: &str,
        turn_id: &str,
        request_id: &str,
        allow: bool,
        always: bool,
        choices: Option<serde_json::Value>,
    ) -> Result<Handed, String> {
        let live = self.live.get(conv).filter(|l| l.turn.as_deref() == Some(turn_id)).ok_or(STALE)?;
        let reply = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == turn_id)).ok_or(STALE)?;
        let waiting = reply.permissions.as_deref().unwrap_or_default();
        let clicked = waiting.iter().find(|p| p.request_id == request_id).ok_or(STALE)?;
        let (tool, epoch) = (clicked.tool.clone(), clicked.epoch.clone());
        // The card of `offer_bigger` is Faber's own: the CLI never asked it, so nothing goes to the CLI as a control response. The
        // person's choice (or the first one, for Skip and for Keep going) goes back as the tool's result, and the answer counts as
        // delivered at once. Only the process that made the card gets it; any other is stale like an install card.
        let faber_card = tool == "AskUserQuestion" && clicked.input["bigger"] == true;
        if faber_card && !self.bigger.get(conv).is_some_and(|(call, sid)| call == request_id && *sid == live.sid) {
            // Stop, a restart or another process spent it: the agent was told already or is gone, so nothing is written to it.
            if let Some(waiting) =
                self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == turn_id)).and_then(|m| m.permissions.as_mut())
            {
                waiting.retain(|p| p.request_id != request_id);
            }
            self.save(conv);
            self.push(conv, Some(turn_id), true);
            return Err(STALE.into());
        }
        if faber_card {
            let typed =
                choices.as_ref().filter(|_| allow).and_then(|a| a.get(clicked.input["questions"][0]["question"].as_str()?)?.as_str());
            live.session.helper_reply(request_id, true, crate::toolbox::bigger_reply(&clicked.input, typed));
            let sid = live.sid;
            self.bigger.remove(conv);
            let (done, delivered) = oneshot::channel();
            let _ = done.send(Delivery::Delivered);
            return Ok(Handed { requests: vec![(request_id.to_string(), delivered)], tool, sid, always: false, epoch });
        }
        // Not in Plan, and not for a card made under an epoch that is gone: that answer lets only its own request through.
        // A question the agent asks is never remembered: each one is the person's to answer.
        let always = always
            && tool != "AskUserQuestion"
            && tool != "ExitPlanMode"
            && live.permissions != Permissions::Plan
            && epoch == self.store.settings().grants_epoch;
        let others = waiting.iter().filter(|p| allow && always && p.tool == tool && p.request_id != request_id);
        let ids = std::iter::once(request_id).chain(others.map(|p| p.request_id.as_str()));
        let requests = ids
            .map(|id| (id.to_string(), live.session.answer_with(id, allow, (id == request_id).then(|| choices.clone()).flatten())))
            .collect();
        Ok(Handed { requests, tool, sid: live.sid, always, epoch })
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
        remember: Option<Option<String>>,
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
        // The grants epoch changed while the card waited (a stricter mode, or settings that were unreadable): this answer lets the
        // one request through and remembers nothing.
        let remember = remember.is_some_and(|epoch| epoch == self.store.settings().grants_epoch);
        if remember && clicked == Some(Delivery::Delivered) {
            // The tool is allowed from now on, so a question that came in while the engine waited gets its answer too.
            if let Some(waiting) = reply.permissions.as_mut() {
                (late, *waiting) = std::mem::take(waiting).into_iter().partition(|p| p.tool == tool);
            }
            let list = c.allow_always.get_or_insert_default();
            if !list.iter().any(|t| t == tool) {
                list.push(tool.to_string());
            }
            // The epoch is renewed on every grant, so one made after a revocation counts again.
            match self.store.settings().grants_epoch {
                Some(epoch) => c.allow_always_epochs.insert(tool.to_string(), epoch),
                None => c.allow_always_epochs.remove(tool),
            };
            changed = true;
        }
        if changed {
            self.save(conv);
            self.weigh(conv);
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
    /// Answers a question the agent asked for the person, who is away ("keep going"): the same hand-over as their own answer,
    /// and once the CLI has it the choice is written on the reply, so the screen can say what Faber chose. Nothing is remembered.
    pub(super) fn answer_unattended(
        &mut self,
        conv: &str,
        turn_id: &str,
        request_id: &str,
        answers: serde_json::Value,
        chosen: Vec<AutoAnswer>,
    ) {
        let Ok(Handed { requests, tool, sid, .. }) = self.prepare_answer(conv, turn_id, request_id, true, false, Some(answers)) else {
            return;
        };
        let (me, conv, turn, request) = (self.me.clone(), conv.to_string(), turn_id.to_string(), request_id.to_string());
        tokio::spawn(async move {
            let deadline = Instant::now() + ACK_WAIT;
            let mut results = Vec::new();
            for (id, ack) in requests {
                results.push((id, timeout_at(deadline, ack).await.ok().and_then(|r| r.ok())));
            }
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            // The CLI took it: the choice is told even when the turn ended before this was settled.
            let delivered = matches!(results.first(), Some((_, Some(Delivery::Delivered))));
            if core.settle_answer(&conv, &turn, sid, &tool, None, &results).is_ok() || delivered {
                core.note_answered_for_person(&conv, &turn, chosen);
            } else {
                // The CLI did not take it: the question is on the list again, and gets another try after the same wait.
                core.watch_question(&conv, &turn, &request);
            }
        });
    }

    /// Allows the questions without asking the user. The answers are watched without the lock: one the CLI did not
    /// take, or did not confirm in time, becomes a question on the list again, so the turn is never left blocked behind
    /// a row that says it is working.
    pub(super) fn auto_answer(&self, conv: &str, asks: Vec<Permission>) {
        let asks: Vec<_> = asks.into_iter().filter(|p| p.tool != "ExitPlanMode").collect();
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
pub(super) mod tests {
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

    pub(in crate::engine) fn engine(dir: &crate::workspace::TestRoot) -> (Engine, Arc<Mutex<Vec<AppEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let opts = EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.work().into(),
            clis: BTreeMap::new(),
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no process in this test"))),
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        };
        (Engine::new(opts, Arc::new(move |e| sink.lock().unwrap().push(e))).unwrap(), events)
    }

    /// A chat whose replies are `turns`, each a message id with the (request id, tool) pairs it waits on. The last is
    /// the turn in flight, on a live `session`.
    pub(in crate::engine) fn seed(engine: &Engine, session: Session, turns: &[(&str, &[(&str, &str)])]) -> String {
        engine.with(|c| {
            let id = c.blank(Area::Chat, None);
            let revision = c.store.settings().permissions_revision;
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
                    input: if *tool == "ExitPlanMode" { json!({"modeRevision": revision}) } else { json!({}) },
                    epoch: None,
                };
                reply.permissions = (!waiting.is_empty()).then(|| waiting.iter().map(ask).collect());
                conv.messages.push(reply);
            }
            let live = Live {
                tool_kit: Default::default(),
                start_card: None,
                kit_identity: None,
                kit_resource_digest: None,
                guide: Default::default(),
                kit_pending: false,
                prepared: false,
                kit_resources: None,
                group_files: None,
                group_context: None,
                cwd: PathBuf::new(),
                visual_root: None,
                vendor: Vendor::Claude,
                model: None,
                running: None,
                capabilities: Default::default(),
                ended: None,
                effort: "medium".into(),
                sid: 1,
                session,
                turn: turns.last().map(|t| t.0.to_string()),
                parent_reported: None,
                break_pending: false,
                answer_started: false,
                settled: false,
                open_tools: Vec::new(),
                used: 0,
                cli: Cli { command: "claude".into(), prefix: vec![] },
                resumed: false,
                permissions: Permissions::Ask,
                tuning: Default::default(),
                research: None,
                retiring: false,
                was_idle: false,
                runs: Permissions::Ask,
                by_hand: true,
                approved: false,
                started: 0.0,
                served: false,
                unsaved_session: None,
                steering: vec![],
                held: None,
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
    async fn commit_receipts_require_success_and_survive_loading_an_older_conversation() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        for (call, command, ok, declined) in [
            ("commit", "git -C 'a folder' commit -m fix", true, false),
            ("quoted", "echo \"example && git commit -m demo\"", true, false),
            ("push", "git push", true, false),
            ("ready", "gh pr ready 2", true, false),
            ("failed", "git commit -m failed", false, false),
            ("refused", "git commit -m refused", true, true),
        ] {
            engine.with(|c| {
                c.handle(
                    &id,
                    1,
                    EngineEvent::Tool { id: call.into(), name: "Bash".into(), input: json!({ "command": command }), parent: None },
                );
                let result = EngineEvent::ToolResult { id: call.into(), ok, declined, text: String::new() };
                c.handle(&id, 1, result.clone());
                c.handle(&id, 1, result);
            });
        }
        assert_eq!(engine.open(&id).unwrap().messages.last().unwrap().commits, ["commit"]);
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().commits.clear();
            c.save(&id);
            c.convs.remove(&id);
        });
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        assert_eq!(reply.commits, ["commit"], "old saved tools are recognised once when loaded");
        assert_eq!(serde_json::to_value(reply).unwrap()["commits"], json!(["commit"]));
    }

    #[tokio::test]
    async fn reply_attention_is_the_latest_running_or_done_state_in_open_and_message_events() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, events) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| {
            c.turn(&id).unwrap().checks = Some(Checks { state: ChecksState::NeedsYou, passed: 0, failed: vec![] });
            c.emit_message(&id, "t1");
        });
        assert!(engine.open(&id).unwrap().messages.last().unwrap().needs_you);
        assert!(engine.state().running.items.iter().any(|i| i.turn_id == "t1" && i.state == RunState::NeedsYou));
        assert!(events.lock().unwrap().iter().any(|e| matches!(e, AppEvent::Message { message, .. } if message.needs_you)));
        engine.with(|c| {
            let conv = c.convs.get_mut(&id).unwrap();
            conv.messages.last_mut().unwrap().status = Some(Status::Done);
            conv.messages.push(Message::new("u2".into(), Role::User, "Leave it, explain what broke.".into(), 4));
            let mut reply = Message::new("t2".into(), Role::Assistant, "The greeting could not be read.".into(), 5);
            reply.status = Some(Status::Done);
            conv.messages.push(reply);
            c.emit_message(&id, "t2");
        });
        assert!(!engine.open(&id).unwrap().messages.last().unwrap().needs_you);
        assert!(engine.state().running.items.is_empty(), "the old needs-you is overtaken for every surface");
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().halt = Some("reason.keepGoing.cap".into());
            c.emit_message(&id, "t2");
        });
        assert!(engine.open(&id).unwrap().messages.last().unwrap().needs_you, "the done card's decision applies without a tool");
        engine.seen(&id).unwrap();
        assert!(engine.open(&id).unwrap().messages.last().unwrap().needs_you, "opening the card cannot erase a problem");
    }

    #[tokio::test]
    async fn a_commit_that_finishes_after_steering_stays_in_its_own_reply() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| {
            c.handle(
                &id,
                1,
                EngineEvent::Tool {
                    id: "save".into(),
                    name: "Bash".into(),
                    input: json!({ "command": "git commit -m fix" }),
                    parent: None,
                },
            );
            let conv = c.convs.get_mut(&id).unwrap();
            conv.messages.push(Message::new("u2".into(), Role::User, "Explain it too.".into(), 4));
            let mut reply = Message::new("t2".into(), Role::Assistant, String::new(), 5);
            reply.status = Some(Status::Streaming);
            conv.messages.push(reply);
            c.live.get_mut(&id).unwrap().turn = Some("t2".into());
            c.handle(&id, 1, EngineEvent::ToolResult { id: "save".into(), ok: true, declined: false, text: String::new() });
        });
        let c = engine.open(&id).unwrap();
        assert_eq!(c.messages.iter().find(|m| m.id == "t1").unwrap().commits, ["save"]);
        assert!(c.messages.last().unwrap().commits.is_empty());
    }

    #[tokio::test]
    async fn r160_review_plan_approval_names_the_clicked_request_and_refuses_a_stale_one() {
        for request in ["missing", "second"] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = engine(&dir);
            engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
            let s = stub(vec![]);
            let id = seed(&engine, s.session.clone(), &[("t1", &[("first", "ExitPlanMode"), ("second", "ExitPlanMode")])]);
            let result = engine.approve_plan(&id, "t1", request, None).await;
            if request == "missing" {
                assert_eq!(result, Err(STALE.into()));
                assert!(s.asked.lock().unwrap().is_empty());
                assert_eq!(waiting(&engine, &id, "t1"), ["first", "second"]);
            } else {
                assert_eq!(result, Ok(()));
                assert_eq!(*s.asked.lock().unwrap(), ["second"]);
                assert_eq!(waiting(&engine, &id, "t1"), ["first"]);
            }
        }
    }

    #[tokio::test]
    async fn r160_plan_card_without_a_revision_cannot_approve_but_can_keep_planning() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let session = stub(vec![]);
        let id = seed(&engine, session.session, &[("t1", &[("plan", "ExitPlanMode")])]);
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().permissions.as_mut().unwrap()[0].input = json!({});
        });
        assert_eq!(engine.approve_plan(&id, "t1", "plan", Some(Permissions::FullAccess)).await, Err("error.permission.planChanged".into()));
        assert!(session.asked.lock().unwrap().is_empty());
        engine.approve_plan(&id, "t1", "plan", None).await.unwrap();
        assert_eq!(*session.asked.lock().unwrap(), ["plan"]);
        engine.shutdown();
    }

    #[tokio::test]
    async fn r160_keep_planning_keeps_the_card_until_claude_confirms_delivery() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![Reply::Say(Delivery::WriteFailed), Reply::Say(Delivery::Delivered)]);
        let id = seed(&engine, s.session, &[("plan-turn", &[("plan", "ExitPlanMode")])]);
        assert_eq!(engine.approve_plan(&id, "plan-turn", "plan", None).await, Err(UNDELIVERED.into()));
        assert_eq!(waiting(&engine, &id, "plan-turn"), ["plan"]);
        assert_eq!(*s.asked.lock().unwrap(), ["plan"]);
        engine.approve_plan(&id, "plan-turn", "plan", None).await.unwrap();
        assert!(waiting(&engine, &id, "plan-turn").is_empty());
        assert_eq!(*s.asked.lock().unwrap(), ["plan", "plan"]);
        engine.shutdown();
    }

    #[tokio::test]
    async fn r160_keep_planning_accepts_confirmed_delivery_without_draining_a_newer_plan() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![Reply::Hold]);
        let id = seed(&engine, s.session, &[("plan-turn", &[("plan", "ExitPlanMode")])]);
        let release = async {
            while s.held.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
            engine.with(|c| {
                c.handle(
                    &id,
                    1,
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
                    },
                )
            });
            // A later Codex turn can already be waiting for its own approval when the old acknowledgement settles.
            engine.with(|c| {
                c.opts.clis.insert(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] });
                c.retire(&id);
                let conversation = c.convs.get_mut(&id).unwrap();
                let mut plan = Message::new("new-plan".into(), Role::Assistant, "Another plan.".into(), 3);
                plan.status = Some(Status::Done);
                plan.vendor = Some(Vendor::Codex);
                plan.permissions =
                    Some(vec![Permission { request_id: "new-request".into(), tool: "ExitPlanMode".into(), input: json!({}), epoch: None }]);
                conversation.messages.push(plan);
                conversation.queued.push(QueuedTurn {
                    id: "waiting".into(),
                    text: "Keep this behind the newer plan.".into(),
                    images: None,
                    attachments: None,
                    choice: Choice { vendor: "claude".into(), model: None, effort: None },
                    at: 4,
                    sending: false,
                    wake: None,
                });
            });
            s.held.lock().unwrap().pop().unwrap().send(Delivery::Delivered).unwrap();
        };
        let (result, ()) = tokio::join!(engine.approve_plan(&id, "plan-turn", "plan", None), release);
        assert_eq!(result, Ok(()), "the CLI confirmed the denial before the call returned");
        assert!(waiting(&engine, &id, "plan-turn").is_empty());
        let conversation = engine.open(&id).unwrap();
        assert_eq!(conversation.messages.last().unwrap().id, "new-plan", "an old denial must not drain a newer turn's queue");
        assert_eq!(conversation.queued.len(), 1);
        assert_eq!(waiting(&engine, &id, "new-plan"), ["new-request"]);
        engine.shutdown();
    }

    #[tokio::test]
    async fn r160_explicit_send_during_claudes_stopped_plan_close_starts_afterwards() {
        stop_holds_the_old_queue(Vendor::Claude, Some(Resume::NewMessage)).await;
    }

    #[tokio::test]
    async fn r160_send_now_during_claudes_stopped_plan_close_starts_afterwards() {
        stop_holds_the_old_queue(Vendor::Claude, Some(Resume::SendNow)).await;
    }

    #[tokio::test]
    async fn r160_explicit_send_during_codexs_stopped_plan_close_starts_afterwards() {
        stop_holds_the_old_queue(Vendor::Codex, Some(Resume::NewMessage)).await;
    }

    #[tokio::test]
    async fn r160_send_now_during_codexs_stopped_plan_close_starts_afterwards() {
        stop_holds_the_old_queue(Vendor::Codex, Some(Resume::SendNow)).await;
    }

    #[tokio::test]
    async fn r160_explicit_send_after_stop_never_steers_into_the_closing_turn() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let steered = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls = steered.clone();
        let session = Session::from_fn(move |cmd| {
            if matches!(cmd, SessionCommand::Steer { .. }) {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let id = seed(&engine, session, &[("ordinary-turn", &[])]);
        engine.with(|c| {
            c.opts.clis.insert(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] });
        });
        engine.interrupt(&id);
        engine
            .send(SendRequest {
                conversation_id: Some(id.clone()),
                area: Area::Chat,
                project_id: None,
                text: "Start after Stop.".into(),
                images: vec![],
                files: vec![],
                choice: Choice { vendor: "claude".into(), model: None, effort: None },
            })
            .unwrap();
        assert_eq!(steered.load(std::sync::atomic::Ordering::SeqCst), 0, "the stopped CLI must receive no more input");
        engine.with(|c| {
            c.handle(
                &id,
                1,
                EngineEvent::Done {
                    ok: false,
                    reason: "interrupted".into(),
                    cost_total_usd: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    cache_write_1h_tokens: 0,
                    incomplete: false,
                },
            )
        });
        let ended = engine.open(&id).unwrap();
        assert_eq!(ended.messages.len(), 4);
        assert_eq!(ended.messages[2].text, "Start after Stop.");
        assert!(ended.queued.is_empty());
        engine.shutdown();
    }

    #[tokio::test]
    async fn r160_stop_then_approve_claudes_plan_starts_work_when_its_held_result_arrives() {
        for stop_again in [false, true] {
            stop_then_approve(Vendor::Claude, true, stop_again).await;
        }
    }

    #[tokio::test]
    async fn r160_stop_then_approve_codexs_plan_starts_work_when_its_held_result_arrives() {
        for stop_again in [false, true] {
            stop_then_approve(Vendor::Codex, true, stop_again).await;
        }
    }

    #[tokio::test]
    async fn r160_stop_then_approve_codexs_finished_plan_sends_only_the_approved_work() {
        stop_then_approve(Vendor::Codex, false, false).await;
    }

    async fn stop_then_approve(vendor: Vendor, closing: bool, stop_again: bool) {
        let dir = crate::workspace::TestRoot::new();
        let (engine, events) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("plan-turn", &[("plan", "ExitPlanMode")])]);
        let launched = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        engine.with(|c| {
            let calls = launched.clone();
            c.opts.spawn = Arc::new(move |_, _, _| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(std::io::Error::other("this test never starts a process"))
            });
            c.opts.clis = Vendor::ALL.into_iter().map(|v| (v, Cli { command: v.name().into(), prefix: vec![] })).collect();
            let live = c.live.get_mut(&id).unwrap();
            live.vendor = vendor;
            live.permissions = Permissions::Plan;
            let conversation = c.convs.get_mut(&id).unwrap();
            conversation.choice = Some(Choice { vendor: vendor.name().into(), model: None, effort: None });
            conversation.messages.last_mut().unwrap().vendor = Some(vendor);
        });
        let other = if vendor == Vendor::Claude { Vendor::Codex } else { Vendor::Claude };
        engine
            .send(SendRequest {
                conversation_id: Some(id.clone()),
                area: Area::Chat,
                project_id: None,
                text: "Leave the older message waiting.".into(),
                images: vec![],
                files: vec![],
                choice: Choice { vendor: other.name().into(), model: None, effort: None },
            })
            .unwrap();
        let done = || EngineEvent::Done {
            ok: !closing,
            reason: if closing { "interrupted".into() } else { String::new() },
            cost_total_usd: Some(0.03),
            input_tokens: 100,
            output_tokens: 20,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        };
        if !closing {
            engine.with(|c| c.handle(&id, 1, done()));
        }
        engine.interrupt(&id);
        assert!(engine.open(&id).unwrap().stopped);
        events.lock().unwrap().clear();
        let approval = engine.approve_plan(&id, "plan-turn", "plan", Some(Permissions::Ask)).await;
        assert_eq!(engine.settings().permissions, Some(Permissions::Ask));
        assert!(waiting(&engine, &id, "plan-turn").is_empty());
        if closing {
            approval.unwrap();
            assert_eq!(launched.load(std::sync::atomic::Ordering::SeqCst), 0, "the old turn must close before work starts");
            assert_eq!(engine.open(&id).unwrap().queued.len(), 2);
            let state = engine.state();
            let work = state
                .running
                .items
                .iter()
                .find(|item| item.conversation_id == id)
                .expect("the screen must keep Stop available while approved work waits for closing");
            assert_eq!(work.state, RunState::Working);
            assert!(!work.settled);
            assert!(
                events.lock().unwrap().iter().all(|event| match event {
                    AppEvent::Running { items, .. } => items.iter().any(|item| item.conversation_id == id),
                    _ => true,
                }),
                "no running update may hide Stop during approval"
            );
            if stop_again {
                engine.interrupt(&id);
                assert!(engine.state().running.items.iter().all(|item| item.conversation_id != id));
                assert!(engine.open(&id).unwrap().send_after_stop.is_none());
            }
            engine.with(|c| c.handle(&id, 1, done()));
        }
        if stop_again {
            assert_eq!(launched.load(std::sync::atomic::Ordering::SeqCst), 0, "the second Stop cancels deferred work");
            let conversation = engine.open(&id).unwrap();
            assert_eq!(conversation.messages.len(), 2);
            assert_eq!(conversation.queued.len(), 1);
            assert_eq!(conversation.queued[0].text, "Leave the older message waiting.");
            assert!(conversation.stopped);
            assert!(conversation.ended.is_none());
            engine.shutdown();
            return;
        }
        // The launcher records dispatch and refuses it, so no real process runs in this unit test.
        assert_eq!(launched.load(std::sync::atomic::Ordering::SeqCst), 1, "approval after Stop is an explicit send");
        let conversation = engine.open(&id).unwrap();
        assert_eq!(conversation.messages.len(), 4);
        assert_eq!(conversation.messages[1].status, Some(Status::Done));
        assert_eq!(conversation.messages[1].input_tokens, Some(100));
        assert_eq!(conversation.messages[2].wake.as_ref().unwrap().why, "wake.approvedPlan");
        assert_eq!(conversation.messages[2].text, "Go ahead with the plan.");
        assert_eq!(conversation.queued.len(), 1);
        assert_eq!(conversation.queued[0].text, "Leave the older message waiting.");
        assert!(!conversation.stopped);
        assert!(conversation.send_after_stop.is_none());
        engine.shutdown();
    }

    enum Resume {
        NewMessage,
        SendNow,
    }

    #[tokio::test]
    async fn r160_stop_holds_the_old_queue_until_claudes_closing_plan_ends() {
        stop_holds_the_old_queue(Vendor::Claude, None).await;
    }

    #[tokio::test]
    async fn r160_stop_holds_the_old_queue_until_codexs_closing_plan_ends() {
        stop_holds_the_old_queue(Vendor::Codex, None).await;
    }

    async fn stop_holds_the_old_queue(vendor: Vendor, resume: Option<Resume>) {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("plan-turn", &[("plan", "ExitPlanMode")])]);
        let launched = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        engine.with(|c| {
            let calls = launched.clone();
            c.opts.spawn = Arc::new(move |_, _, _| {
                calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(std::io::Error::other("this test never starts a process"))
            });
            c.opts.clis = Vendor::ALL.into_iter().map(|v| (v, Cli { command: v.name().into(), prefix: vec![] })).collect();
            let live = c.live.get_mut(&id).unwrap();
            live.vendor = vendor;
            live.permissions = Permissions::Plan;
            let conversation = c.convs.get_mut(&id).unwrap();
            conversation.choice = Some(Choice { vendor: vendor.name().into(), model: None, effort: None });
            conversation.messages.last_mut().unwrap().vendor = Some(vendor);
        });
        let other = if vendor == Vendor::Claude { Vendor::Codex } else { Vendor::Claude };
        engine
            .send(SendRequest {
                conversation_id: Some(id.clone()),
                area: Area::Chat,
                project_id: None,
                text: "Read the older message.".into(),
                images: vec![],
                files: vec![],
                choice: Choice { vendor: other.name().into(), model: None, effort: None },
            })
            .unwrap();
        engine.approve_plan(&id, "plan-turn", "plan", Some(Permissions::Ask)).await.unwrap();
        assert_eq!(engine.open(&id).unwrap().queued.len(), 2);
        engine.interrupt(&id);
        let held = engine.open(&id).unwrap();
        assert_eq!(held.queued.len(), 1, "Stop removes approved work and leaves the older message");
        assert_eq!(held.queued[0].text, "Read the older message.");
        assert!(engine.state().running.items.iter().all(|item| item.conversation_id != id));
        match resume {
            Some(Resume::NewMessage) => {
                engine
                    .send(SendRequest {
                        conversation_id: Some(id.clone()),
                        area: Area::Chat,
                        project_id: None,
                        text: "Run the new message.".into(),
                        images: vec![],
                        files: vec![],
                        choice: Choice { vendor: other.name().into(), model: None, effort: None },
                    })
                    .unwrap();
            }
            Some(Resume::SendNow) => engine.send_queued(&id, &held.queued[0].id).unwrap(),
            None => {}
        }
        assert_eq!(launched.load(std::sync::atomic::Ordering::SeqCst), 0, "the closing turn still owns the session");
        // Release the completion only now: permission approval must not let its Done status restart the queue.
        engine.with(|c| {
            c.handle(
                &id,
                1,
                EngineEvent::Done {
                    ok: false,
                    reason: "interrupted".into(),
                    cost_total_usd: Some(0.03),
                    input_tokens: 100,
                    output_tokens: 20,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    cache_write_1h_tokens: 0,
                    incomplete: false,
                },
            )
        });
        let ended = engine.open(&id).unwrap();
        if let Some(resume) = resume {
            assert_eq!(launched.load(std::sync::atomic::Ordering::SeqCst), 1, "the explicitly sent message starts once closing ends");
            assert_eq!(ended.messages.len(), 4);
            let expected = match resume {
                Resume::NewMessage => "Run the new message.",
                Resume::SendNow => "Read the older message.",
            };
            assert_eq!(ended.messages[2].text, expected);
            if matches!(resume, Resume::NewMessage) {
                assert_eq!(ended.queued, held.queued, "the untouched older message stays held");
            } else {
                assert!(ended.queued.is_empty());
            }
            assert!(!ended.stopped);
            engine.shutdown();
            return;
        }
        assert_eq!(ended.messages.len(), 2, "the older message must not start when the interrupted result arrives");
        assert_eq!(ended.messages[1].status, Some(Status::Done));
        assert_eq!(ended.messages[1].input_tokens, Some(100));
        assert_eq!(ended.queued, held.queued);
        assert!(ended.ended.is_none(), "the person's Stop leaves no done card");
        assert_eq!(launched.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(engine.state().running.items.iter().all(|item| item.conversation_id != id));
        // A deliberate Send now starts a new run; the fake launcher refuses it after recording the launch.
        let _ = engine.send_queued(&id, &held.queued[0].id);
        assert_eq!(launched.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(!engine.open(&id).unwrap().stopped, "a new manual turn clears the previous Stop");
        engine.shutdown();
    }

    #[tokio::test]
    async fn r160_completed_plan_keeps_done_and_drains_work_if_its_closing_process_fails() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let session = stub(vec![]);
        let id = seed(&engine, session.session, &[("t1", &[("plan", "ExitPlanMode")])]);
        engine.with(|c| {
            c.opts.clis.insert(Vendor::Claude, Cli { command: "fake".into(), prefix: vec![] });
            c.convs.get_mut(&id).unwrap().choice = Some(Choice { vendor: "claude".into(), model: None, effort: None });
        });
        engine.answer(&id, "t1", "plan", true, false).await.unwrap();
        assert_eq!(engine.settings().permissions, Some(Permissions::Ask), "a legacy card without prior mode never loosens Ask");
        engine.with(|c| c.handle(&id, 1, EngineEvent::Error { message: "error.cli.interruptTimeout".into(), fatal: true }));
        let conversation = engine.open(&id).unwrap();
        assert_eq!(conversation.messages[1].status, Some(Status::Done), "the accepted plan stays finished");
        assert_eq!(conversation.messages[1].incomplete, Some(true), "the missing final counters are explicit");
        assert_eq!(conversation.messages.len(), 4, "the ordinary work turn was dispatched");
        assert_eq!(conversation.messages[2].wake.as_ref().unwrap().why, "wake.approvedPlan");
        assert!(conversation.queued.is_empty());
        engine.shutdown();
    }

    #[tokio::test]
    async fn r160_codex_plan_exit_is_saved_when_the_work_turn_is_accepted_even_if_it_stops() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().codex_plan = true;
            let live = c.live.get_mut(&id).unwrap();
            live.vendor = Vendor::Codex;
            live.permissions = Permissions::Ask;
            c.handle(&id, 1, EngineEvent::TurnStarted);
        });
        assert!(!engine.open(&id).unwrap().codex_plan, "acceptance already left the sticky Plan prompt");
        assert!(!Store::new(dir.path()).get(&id).unwrap().codex_plan, "a restart must not send a second default transition");
        engine.with(|c| {
            c.handle(
                &id,
                1,
                EngineEvent::Done {
                    ok: false,
                    reason: "interrupted".into(),
                    cost_total_usd: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    cache_write_1h_tokens: 0,
                    incomplete: true,
                },
            )
        });
        assert!(!engine.open(&id).unwrap().codex_plan);
        engine.shutdown();
    }

    #[tokio::test]
    async fn r160_shared_plan_defaults_never_loosen_an_agent_entered_plan_and_consume_history() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            for current in [Permissions::Ask, Permissions::AcceptEdits, Permissions::Plan] {
                let dir = crate::workspace::TestRoot::new();
                let (engine, _) = engine(&dir);
                let id = seed(&engine, stub(vec![]).session, &[("before", &[]), ("t1", &[])]);
                engine.with(|c| {
                    c.store
                        .save_settings(&Settings {
                            permissions: Some(current),
                            before_plan: Some(Permissions::FullAccess),
                            ..Settings::default()
                        })
                        .unwrap();
                    let live = c.live.get_mut(&id).unwrap();
                    live.vendor = vendor;
                    // Codex only supplies a completed plan when its turn runs in Plan.
                    live.permissions = if vendor == Vendor::Codex { Permissions::Plan } else { current };
                    if vendor == Vendor::Codex {
                        c.store
                            .save_settings(&Settings {
                                permissions: Some(Permissions::Plan),
                                before_plan: Some(current),
                                ..Settings::default()
                            })
                            .unwrap();
                    }
                    c.handle(
                        &id,
                        1,
                        EngineEvent::Permission {
                            request_id: "plan".into(),
                            tool: "ExitPlanMode".into(),
                            input: json!({"plan":"Check the parser."}),
                        },
                    );
                });
                let card = engine.open(&id).unwrap().messages.last().unwrap().permissions.as_ref().unwrap()[0].clone();
                let selected: Permissions = serde_json::from_value(card.input["mode"].clone()).unwrap();
                if current != Permissions::Plan {
                    assert_eq!(selected, current);
                }
                assert_eq!(engine.settings().before_plan, None, "history is consumed even when the agent entered Plan itself");
            }
        }
    }

    #[tokio::test]
    async fn r160_shared_the_card_omits_a_plan_already_carried_by_the_reply() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = engine(&dir);
            let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
            engine.with(|c| {
                let live = c.live.get_mut(&id).unwrap();
                live.vendor = vendor;
                live.permissions = Permissions::Plan;
                c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().text = "Check the parser.".into();
                c.handle(
                    &id,
                    1,
                    EngineEvent::Permission {
                        request_id: "plan".into(),
                        tool: "ExitPlanMode".into(),
                        input: json!({"plan":"Check the parser."}),
                    },
                );
            });
            let conversation = engine.open(&id).unwrap();
            let reply = conversation.messages.last().unwrap();
            assert_eq!(reply.text, "Check the parser.");
            assert!(reply.permissions.as_ref().unwrap()[0].input.get("plan").is_none(), "the card must not render a second copy");
        }
    }

    #[tokio::test]
    async fn r160_shared_a_later_mode_pick_clears_the_saved_plan_default() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
        assert_eq!(engine.settings().before_plan, Some(Permissions::Ask));
        engine.save_settings(Settings { permissions: Some(Permissions::AcceptEdits), ..Settings::default() }).unwrap();
        assert_eq!(engine.settings().before_plan, None);
        engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
        assert_eq!(engine.settings().before_plan, Some(Permissions::AcceptEdits));
        engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
        assert_eq!(engine.settings().before_plan, None, "even a later pick of Plan discards the old choice");
    }

    #[tokio::test]
    async fn keep_going_enabled_after_a_codex_plan_waits_for_the_person() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("plan:item", "ExitPlanMode")])]);
        engine.with(|c| {
            let conv = c.convs.get_mut(&id).unwrap();
            conv.area = Area::Project;
            let m = conv.messages.last_mut().unwrap();
            m.status = Some(Status::Done);
            m.vendor = Some(Vendor::Codex);
            m.wrap_up = Some(WrapUp {
                recap: "Ready.".into(),
                ask: Some("Start?".into()),
                actions: vec!["Start".into()],
                done: false,
                answered: None,
                ..Default::default()
            });
            c.live.get_mut(&id).unwrap().turn = None;
        });
        engine.keep_going(&id, true).unwrap();
        engine.with(|c| c.keep_going_after(&id, Some("t1".into())));
        let conv = engine.open(&id).unwrap();
        assert!(conv.keep_going.unwrap().waiting.is_none(), "a plan never arms the closing question's automatic answer");
        assert_eq!(conv.messages.len(), 2, "no automatic turn starts behind the plan card");
        assert!(conv.messages.last().unwrap().wrap_up.as_ref().unwrap().answered.is_none());
        assert!(s.asked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_delivered_answer_takes_the_question_off_the_list_and_the_row_goes_back_to_working() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, events) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![Reply::Say(Delivery::Unknown)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(STALE.to_string()));
        assert!(waiting(&engine, &id, "t1").is_empty(), "the CLI no longer waits for it");
    }

    #[tokio::test]
    async fn a_write_that_fails_keeps_the_question_and_the_revision_and_a_second_answer_can_land() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, events) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![Reply::Closed]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("0", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "0", true, false).await, Err(UNDELIVERED.to_string()));
        assert_eq!(waiting(&engine, &id, "t1"), ["0"]);
    }

    #[tokio::test(start_paused = true)]
    async fn no_acknowledgement_in_five_seconds_is_undelivered_and_a_late_one_changes_nothing() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, events) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![Reply::Say(Delivery::Delivered), Reply::Say(Delivery::Unknown)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write"), ("b", "Bash"), ("c", "Write")])]);

        assert_eq!(engine.answer(&id, "t1", "a", true, true).await, Ok(()));

        assert_eq!(*s.asked.lock().unwrap(), ["a", "c"], "the clicked request first, then the other Write");
        assert_eq!(waiting(&engine, &id, "t1"), ["b"], "c was unknown to the CLI, so it goes too");
        assert_eq!(engine.open(&id).unwrap().allow_always, Some(vec!["Write".to_string()]));
    }

    #[tokio::test]
    async fn deny_with_always_answers_only_the_clicked_request_and_remembers_nothing() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write"), ("c", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "a", false, true).await, Ok(()));
        assert_eq!(*s.asked.lock().unwrap(), ["a"]);
        assert_eq!(waiting(&engine, &id, "t1"), ["c"]);
        assert_eq!(engine.open(&id).unwrap().allow_always, None);
    }

    #[tokio::test]
    async fn an_always_that_was_not_delivered_does_not_join_the_allow_list() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![Reply::Say(Delivery::WriteFailed)]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[("a", "Write")])]);
        assert_eq!(engine.answer(&id, "t1", "a", true, true).await, Err(UNDELIVERED.to_string()));
        assert_eq!(engine.open(&id).unwrap().allow_always, None);
    }

    #[tokio::test]
    async fn two_clicks_on_the_two_copies_of_a_card_give_one_ok_and_one_stale() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
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

    #[tokio::test]
    async fn r140_plan_takes_no_remembered_grant_and_remembers_none() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        // A grant made under Ask, still on the conversation: Plan ignores it and shows the card.
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[])]);
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().allow_always = Some(vec!["Write".into(), "Bash".into()]);
            c.live.get_mut(&id).unwrap().permissions = Permissions::Plan;
        });
        ask(&engine, &id, "7", "Write");
        ask(&engine, &id, "8", "Bash");
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        assert!(s.asked.lock().unwrap().is_empty(), "nothing was answered for the person");
        assert_eq!(waiting(&engine, &id, "t1"), ["7", "8"]);
        // Allowing with Always under Plan allows that one request only: no grant, and the same tool waiting stays waiting.
        assert_eq!(engine.answer(&id, "t1", "7", true, true).await, Ok(()));
        assert_eq!(*s.asked.lock().unwrap(), ["7"]);
        assert_eq!(engine.open(&id).unwrap().allow_always, Some(vec!["Write".into(), "Bash".into()]), "the old list is untouched here");
        engine.with(|c| c.convs.get_mut(&id).unwrap().allow_always = None);
        ask(&engine, &id, "9", "Edit");
        assert_eq!(engine.answer(&id, "t1", "9", true, true).await, Ok(()));
        assert_eq!(engine.open(&id).unwrap().allow_always, None, "Plan remembers nothing");
        // The same grant under Ask is honoured, as before.
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().allow_always = Some(vec!["Write".into()]);
            c.live.get_mut(&id).unwrap().permissions = Permissions::Ask;
        });
        s.asked.lock().unwrap().clear();
        ask(&engine, &id, "10", "Write");
        assert_eq!(*s.asked.lock().unwrap(), ["10"]);
    }

    #[tokio::test]
    async fn r140_a_stricter_mode_forgets_every_always_allow_loaded_or_on_disk() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        // The default is Full access, so Ask is already a stricter step: move there before any grant exists.
        engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        let grant = |engine: &Engine| {
            engine.with(|c| {
                let id = c.blank(Area::Chat, None);
                c.convs.get_mut(&id).unwrap().allow_always = Some(vec!["Write".into()]);
                c.save(&id);
                id
            })
        };
        let loaded = grant(&engine);
        let on_disk = grant(&engine);
        engine.with(|c| c.convs.remove(&on_disk));
        let granted = |engine: &Engine, id: &str| engine.open(id).unwrap().allow_always;
        // Looser: kept. Ask to Accept edits loosens.
        engine.save_settings(Settings { permissions: Some(Permissions::AcceptEdits), ..Settings::default() }).unwrap();
        assert_eq!(granted(&engine, &loaded), Some(vec!["Write".into()]));
        engine.with(|c| c.convs.remove(&on_disk));
        assert_eq!(granted(&engine, &on_disk), Some(vec!["Write".into()]));
        engine.with(|c| c.convs.remove(&on_disk));
        // Stricter: gone, in memory and in the saved file of a conversation that was never opened.
        engine.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
        assert_eq!(granted(&engine, &loaded), None);
        assert_eq!(granted(&engine, &on_disk), None);
        let saved = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(dir.path().join("conversations").join(format!("{on_disk}.json"))).unwrap(),
        )
        .unwrap();
        assert!(saved.get("allowAlways").is_none(), "{saved}");
    }

    fn stands(engine: &Engine, id: &str, tool: &str) -> bool {
        engine.open(id).unwrap();
        engine.with(|c| c.grant_stands(id, tool))
    }

    #[tokio::test]
    async fn r140_a_revocation_holds_after_a_conversation_write_that_failed_and_a_restart() {
        let dir = crate::workspace::TestRoot::new();
        let (first, _) = engine(&dir);
        first.save_settings(Settings { permissions: Some(Permissions::Auto), ..Settings::default() }).unwrap();
        let epoch = first.settings().grants_epoch.unwrap();
        let id = first.with(|c| {
            let id = c.blank(Area::Chat, None);
            let conv = c.convs.get_mut(&id).unwrap();
            conv.allow_always = Some(vec!["Bash".into(), "Write".into()]);
            conv.allow_always_epochs.insert("Bash".into(), epoch.clone());
            // Made while the clock ran far ahead, or with a token nobody holds: neither is ever read as a time.
            conv.allow_always_epochs.insert("Write".into(), "9999999999999".into());
            c.save(&id);
            id
        });
        let file = dir.path().join("conversations").join(format!("{id}.json"));
        let saved = std::fs::read(&file).unwrap();
        assert!(stands(&first, &id, "Bash"), "made under the current epoch");
        assert!(!stands(&first, &id, "Write"), "made under another one");
        first.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        assert_ne!(first.settings().grants_epoch, Some(epoch), "the stricter move replaced the epoch in its own save");
        // The write that failed left the old file, grant included, on disk.
        std::fs::write(&file, &saved).unwrap();
        first.shutdown();
        let (restarted, _) = engine(&dir);
        assert!(!stands(&restarted, &id, "Bash"), "the grant belongs to the old epoch, so Ask asks");
        // A grant the person makes afterwards counts.
        restarted.with(|c| {
            let epoch = c.store.settings().grants_epoch.unwrap();
            c.convs.get_mut(&id).unwrap().allow_always_epochs.insert("Bash".into(), epoch);
        });
        assert!(stands(&restarted, &id, "Bash"));
        // A looser move keeps the epoch; a new stricter one replaces it.
        let kept = restarted.settings().grants_epoch;
        restarted.save_settings(Settings { permissions: Some(Permissions::AcceptEdits), ..Settings::default() }).unwrap();
        assert_eq!(restarted.settings().grants_epoch, kept);
        assert!(stands(&restarted, &id, "Bash"));
        restarted.save_settings(Settings { permissions: Some(Permissions::Plan), ..Settings::default() }).unwrap();
        assert_ne!(restarted.settings().grants_epoch, kept);
        assert!(!stands(&restarted, &id, "Bash"));
    }

    #[tokio::test]
    async fn r140_old_saved_grants_stand_until_the_first_revocation_and_the_card_shows_after_it() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[])]);
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().allow_always = Some(vec!["Write".into(), "Bash".into()]);
            c.live.get_mut(&id).unwrap().permissions = Permissions::Ask;
        });
        // A grant saved before epochs existed has none: it stands while the settings never held one.
        assert!(stands(&engine, &id, "Bash"));
        ask(&engine, &id, "1", "Bash");
        assert_eq!(*s.asked.lock().unwrap(), ["1"], "answered without a card");
        // The first stricter move: the same old grants stop counting, and the request waits for the person.
        // (Written straight to the store here: the real move also stops the running turn this test goes on using.)
        engine.with(|c| {
            let moved =
                Settings { permissions: Some(Permissions::Ask), grants_epoch: Some(crate::model::new_epoch()), ..Settings::default() };
            c.store.save_settings(&moved).unwrap();
        });
        assert!(!stands(&engine, &id, "Bash"));
        assert!(!stands(&engine, &id, "Write"));
        s.asked.lock().unwrap().clear();
        ask(&engine, &id, "2", "Bash");
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        assert!(s.asked.lock().unwrap().is_empty());
        assert_eq!(waiting(&engine, &id, "t1"), ["2"]);
        // Allowing with Always after the revocation counts, for that tool only.
        assert_eq!(engine.answer(&id, "t1", "2", true, true).await, Ok(()));
        assert!(stands(&engine, &id, "Bash"));
        assert!(!stands(&engine, &id, "Write"));
    }

    #[tokio::test]
    async fn r140_settings_that_cannot_be_read_and_the_next_save_never_revive_a_grant() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[])]);
        engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        let epoch = engine.settings().grants_epoch.unwrap();
        engine.with(|c| {
            let conv = c.convs.get_mut(&id).unwrap();
            conv.allow_always = Some(vec!["Bash".into()]);
            conv.allow_always_epochs.insert("Bash".into(), epoch);
        });
        assert!(stands(&engine, &id, "Bash"));
        std::fs::write(dir.path().join("settings.json"), "{ not json").unwrap();
        assert!(!stands(&engine, &id, "Bash"), "unreadable settings start a new epoch");
        // The next save writes that new epoch (never a time that could have passed), so the grant stays gone.
        engine.save_settings(Settings { language: Some("de".into()), ..Settings::default() }).unwrap();
        assert!(!stands(&engine, &id, "Bash"));
        assert!(engine.settings().grants_epoch.is_some());
        // A grant made now works.
        engine.with(|c| {
            let epoch = c.store.settings().grants_epoch.unwrap();
            c.convs.get_mut(&id).unwrap().allow_always_epochs.insert("Bash".into(), epoch);
        });
        assert!(stands(&engine, &id, "Bash"));
    }

    #[tokio::test]
    async fn r140_a_read_failure_that_passes_never_brings_the_old_epoch_back() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let file = dir.path().join("settings.json");
        engine.save_settings(Settings { permissions: Some(Permissions::Ask), ..Settings::default() }).unwrap();
        let old = engine.settings().grants_epoch.unwrap();
        let good = std::fs::read(&file).unwrap();
        std::fs::write(&file, "{ not json").unwrap();
        let recovery = engine.settings().grants_epoch.unwrap();
        assert_ne!(recovery, old);
        assert_eq!(engine.settings().grants_epoch.as_ref(), Some(&recovery), "the same epoch on every read while it lasts");
        // The failure passes: the file reads again, with the epoch it had. That epoch is not in force.
        std::fs::write(&file, &good).unwrap();
        assert_eq!(engine.settings().grants_epoch.as_ref(), Some(&recovery));
        // The next save writes the recovery epoch, and it stays after that.
        engine.save_settings(Settings { language: Some("de".into()), ..Settings::default() }).unwrap();
        assert_eq!(read_epoch(&file), recovery);
        assert_eq!(engine.settings().grants_epoch.as_ref(), Some(&recovery));
        assert_eq!(engine.settings().language.as_deref(), Some("de"));
    }

    fn read_epoch(file: &Path) -> String {
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(file).unwrap()).unwrap()["grantsEpoch"].as_str().unwrap().into()
    }

    #[tokio::test]
    async fn r140_an_always_allow_whose_epoch_changed_while_the_card_waited_lets_that_request_through_and_remembers_nothing() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let s = stub(vec![]);
        let id = seed(&engine, s.session.clone(), &[("t1", &[])]);
        let moved = |engine: &Engine| {
            engine.with(|c| {
                let moved =
                    Settings { permissions: Some(Permissions::Ask), grants_epoch: Some(crate::model::new_epoch()), ..Settings::default() };
                c.store.save_settings(&moved).unwrap();
            })
        };
        moved(&engine);
        engine.with(|c| c.live.get_mut(&id).unwrap().permissions = Permissions::Ask);
        ask(&engine, &id, "1", "Bash");
        ask(&engine, &id, "2", "Bash");
        assert_eq!(waiting(&engine, &id, "t1"), ["1", "2"]);
        moved(&engine);
        assert_eq!(engine.answer(&id, "t1", "1", true, true).await, Ok(()));
        assert_eq!(*s.asked.lock().unwrap(), ["1"], "only the clicked card");
        assert_eq!(waiting(&engine, &id, "t1"), ["2"], "the other card of the tool still waits for the person");
        assert!(!stands(&engine, &id, "Bash"), "nothing was remembered");
        // A card made under the current epoch is remembered as before, and answers the other waiting one of its tool.
        s.asked.lock().unwrap().clear();
        ask(&engine, &id, "3", "Bash");
        assert_eq!(engine.answer(&id, "t1", "3", true, true).await, Ok(()));
        assert!(stands(&engine, &id, "Bash"));
        assert_eq!(*s.asked.lock().unwrap(), ["3", "2"]);
    }

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

    fn failed() -> EngineEvent {
        let EngineEvent::Done {
            reason: _,
            ok: _,
            cost_total_usd,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            cache_write_1h_tokens,
            incomplete,
        } = done()
        else {
            unreachable!()
        };
        EngineEvent::Done {
            ok: false,
            reason: "rate_limit".into(),
            cost_total_usd,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            cache_write_1h_tokens,
            incomplete,
        }
    }

    fn tool(id: &str, parent: Option<&str>) -> EngineEvent {
        EngineEvent::Tool { id: id.into(), name: "Agent".into(), input: json!({}), parent: parent.map(String::from) }
    }

    /// The reply of the seeded conversation and its row in the running list, once the screen's next tick has passed.
    async fn settled(engine: &Engine, id: &str) -> (Option<u64>, Option<bool>) {
        tokio::time::sleep(Duration::from_millis(80)).await;
        // The screen's update waits 33 ms on a spawned task, which a busy machine may run late (macOS in CI, 2026-10-19):
        // send what is pending now, so the running list read below is never one update behind.
        engine.with(|c| {
            if let Some(message) = c.pending.remove(id) {
                c.emit_message(id, &message);
            }
        });
        let reply = engine.open(id).unwrap().messages.pop().unwrap();
        (reply.finished_at, engine.state().running.items.first().map(|i| i.settled))
    }

    #[tokio::test]
    async fn a_reply_whose_last_message_ended_reads_as_finished_while_the_cli_still_closes_its_turn() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| c.handle(&id, 1, EngineEvent::Text("All done.".into())));
        assert_eq!(settled(&engine, &id).await, (None, Some(false)), "still writing");

        engine.with(|c| c.handle(&id, 1, EngineEvent::Settled));
        let (finished, row) = settled(&engine, &id).await;
        assert!(finished.is_some(), "the reply says when the agent finished");
        assert_eq!(row, Some(true), "the row stays for updates and the awake lock, marked settled for the screen");
        assert_eq!(
            engine.open(&id).unwrap().messages.last().unwrap().status,
            Some(Status::Streaming),
            "the engine still waits for the result"
        );

        // More words after it mean it was not done.
        engine.with(|c| c.handle(&id, 1, EngineEvent::Text(" One more thing.".into())));
        assert_eq!(settled(&engine, &id).await, (None, Some(false)));

        engine.with(|c| c.handle(&id, 1, EngineEvent::Settled));
        engine.with(|c| c.handle(&id, 1, done()));
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        assert_eq!((reply.status, reply.finished_at.is_some(), reply.moved_at.is_some()), (Some(Status::Done), true, true));
        assert!(engine.state().running.items.is_empty());
    }

    #[tokio::test]
    async fn a_tool_call_still_open_keeps_the_reply_from_reading_as_finished() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| c.handle(&id, 1, tool("bg", None)));
        engine.with(|c| c.handle(&id, 1, EngineEvent::Settled));
        assert_eq!(settled(&engine, &id).await, (None, Some(false)), "a helper is still at work for the agent");

        engine.with(|c| c.handle(&id, 1, EngineEvent::ToolResult { id: "bg".into(), ok: true, declined: false, text: String::new() }));
        engine.with(|c| c.handle(&id, 1, EngineEvent::Settled));
        assert_eq!(settled(&engine, &id).await.1, Some(true));
    }

    #[tokio::test]
    async fn a_queued_message_or_keep_going_keeps_the_row_at_work_after_the_agent_finished() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| c.handle(&id, 1, EngineEvent::Settled));
        assert_eq!(settled(&engine, &id).await.1, Some(true));
        engine.with(|c| {
            c.convs.get_mut(&id).unwrap().queued.push(crate::model::QueuedTurn {
                id: "q".into(),
                text: "next".into(),
                images: None,
                attachments: None,
                choice: crate::router::Choice { vendor: "auto".into(), model: None, effort: None },
                at: 3,
                sending: false,
                wake: None,
            });
            c.refresh_running();
        });
        assert_eq!(settled(&engine, &id).await.1, Some(false), "a message waits for the result: the conversation is not idle");
    }

    #[tokio::test]
    async fn a_message_sent_after_the_agent_finished_waits_for_the_turn_to_close_instead_of_joining_it() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| c.handle(&id, 1, EngineEvent::Settled));
        assert_eq!(settled(&engine, &id).await.1, Some(true));
        let req = SendRequest {
            conversation_id: Some(id.clone()),
            area: Area::Chat,
            project_id: None,
            text: "next".into(),
            images: vec![],
            files: vec![],
            choice: crate::router::Choice { vendor: "auto".into(), model: None, effort: None },
        };
        engine.with(|c| c.enqueue(&id, req)).unwrap();
        let (steering, sending) =
            engine.with(|c| (c.live[&id].steering.clone(), c.convs[&id].queued.iter().map(|q| q.sending).collect::<Vec<_>>()));
        assert!(steering.is_empty(), "nothing is written into a turn whose agent already finished");
        assert_eq!(sending, [false], "the message waits for the next turn");
        assert_eq!(settled(&engine, &id).await.1, Some(false), "and the row is at work again, with Stop, without another event");
    }

    #[tokio::test]
    async fn a_turn_that_fails_after_the_agent_finished_is_not_done_so_a_wait_or_a_move_reads_as_work() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| c.handle(&id, 1, EngineEvent::Settled));
        engine.with(|c| c.handle(&id, 1, failed()));
        assert_eq!(engine.open(&id).unwrap().messages.pop().unwrap().finished_at, None);
    }

    fn written(id: &str) -> SendRequest {
        SendRequest {
            conversation_id: Some(id.into()),
            area: Area::Chat,
            project_id: None,
            text: "next".into(),
            images: vec![],
            files: vec![],
            choice: crate::router::Choice { vendor: "auto".into(), model: None, effort: None },
        }
    }

    fn handed(engine: &Engine, id: &str) -> (usize, Vec<bool>) {
        engine.with(|c| (c.live[id].steering.len(), c.convs[id].queued.iter().map(|q| q.sending).collect()))
    }

    fn bash(id: &str) -> EngineEvent {
        EngineEvent::Tool { id: id.into(), name: "Bash".into(), input: json!({ "command": "sleep 6" }), parent: None }
    }

    fn result(id: &str) -> EngineEvent {
        EngineEvent::ToolResult { id: id.into(), ok: true, declined: false, text: String::new() }
    }

    #[tokio::test]
    async fn a_message_written_while_tools_run_waits_until_the_last_of_them_returns() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| {
            c.handle(&id, 1, bash("a"));
            c.handle(&id, 1, bash("b"));
        });
        engine.with(|c| c.enqueue(&id, written(&id))).unwrap();
        assert_eq!(handed(&engine, &id), (0, vec![false]), "it waits, the person's to edit or remove");
        engine.with(|c| c.handle(&id, 1, result("a")));
        assert_eq!(handed(&engine, &id), (0, vec![false]), "one tool still runs: the step has not ended");
        // A steer that started a new reply leaves b on the old one; the step still waits for it.
        engine.with(|c| c.live.get_mut(&id).unwrap().turn = Some("t2".into()));
        engine.with(|c| c.handle(&id, 1, result("b")));
        assert_eq!(handed(&engine, &id), (1, vec![true]), "the step ended: it goes into the turn");
    }

    #[tokio::test]
    async fn only_an_idle_main_thread_takes_a_message_in_at_once() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&dir);
        let id = seed(&engine, stub(vec![]).session, &[("t1", &[])]);
        engine.with(|c| c.handle(&id, 1, EngineEvent::MainIdle));
        engine.with(|c| c.handle(&id, 1, EngineEvent::Thinking("more".into())));
        engine.with(|c| c.enqueue(&id, written(&id))).unwrap();
        assert_eq!(handed(&engine, &id), (0, vec![false]), "the agent thinks again: the message waits for its step");
        engine.with(|c| c.handle(&id, 1, EngineEvent::MainIdle));
        assert_eq!(handed(&engine, &id), (1, vec![true]), "with only helpers at work it goes in at once");
    }
}
