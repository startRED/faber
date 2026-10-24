//! Faber sends a turn into a conversation by itself (R199). "Keep going" uses it, and so can anything else that wakes an
//! agent while nobody is there. The message joins the conversation like one the person wrote, marked with why Faber sent
//! it (`Wake`), so the screen draws a quiet line in its place and the person's words stay theirs.

use super::*;
use crate::model::Wake;

impl Core {
    /// Sends `text` as the next turn of `conv`, with the choice its last message used. While a turn runs, or the person's
    /// own messages wait, it joins the queue behind them. Agent coordination enters the running turn at a safe step;
    /// other continuations wait for their own turn. Otherwise it starts now, subject to subscription and quota.
    pub(super) fn send_by_itself(&mut self, conv: &str, text: String, wake: Wake) -> Result<(), String> {
        self.load(conv).ok_or("error.conversationGone")?;
        // A goal's fix, or a conversation its agent started, does not go on by itself once the project's day is out of money
        // (the approval of a plan is the person's own, and goes on). The caller treats it like any refusal: the person decides.
        if wake.why != "wake.approvedPlan" && self.goal_out_of_money(conv) {
            return Err("reason.goal.cap".into());
        }
        let busy = self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let c = self.load(conv).ok_or("error.conversationGone")?;
        let choice = c.choice.clone().unwrap_or(Choice { vendor: "auto".into(), model: None, effort: None });
        let coordination = matches!(wake.why.as_str(), "wake.asked" | "wake.childDone");
        let item = QueuedTurn { id: new_id(), text, images: None, attachments: None, choice, at: now(), sending: false, wake: Some(wake) };
        // Faber's checks of the last reply count as a turn that runs (`verifying.rs`).
        if busy || !c.queued.is_empty() || verifying::checking(c) || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
        {
            if c.queued.len() >= MAX_QUEUED {
                return Err("error.queueFull".into());
            }
            // Approval is the person's explicit send, even though Faber supplies its visible line.
            let requested = (c.stopped && item.wake.as_ref().is_some_and(|w| w.why == "wake.approvedPlan")).then(|| item.id.clone());
            c.queued.push(item);
            if let Some(requested) = requested {
                return self.send_queued(conv, &requested);
            }
            self.save(conv);
            self.emit_queue(conv);
            if coordination {
                self.hand_over_queue(conv, true);
            }
            // An idle Error may hold older wakes: always drain the queue, never dispatch this new item ahead of them.
            self.drain(conv);
            return Ok(());
        }
        let id = item.id.clone();
        let sent = self.dispatch_queued(conv, item);
        // A message nobody could answer goes back to the queue; one Faber wrote is dropped instead of waiting there.
        if sent.is_err()
            && let Some(c) = self.convs.get_mut(conv)
            && c.queued.iter().any(|q| q.id == id)
        {
            c.queued.retain(|q| q.id != id);
            self.save(conv);
            self.emit_queue(conv);
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Session, SessionCommand};

    #[tokio::test]
    async fn agent_messages_reach_the_running_turn_once_and_keep_their_origin() {
        for why in ["wake.asked", "wake.childDone"] {
            for tools_open in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let (engine, _) = super::super::answer::tests::engine(dir.path());
                let delivered = Arc::new(Mutex::new(Vec::new()));
                let sink = delivered.clone();
                let stored = dir.path().to_path_buf();
                let session = Session::from_fn(move |command| {
                    if let SessionCommand::Steer { turn, tag } = command {
                        let store = Store::new(&stored);
                        assert!(
                            store
                                .list()
                                .iter()
                                .filter_map(|c| store.get(&c.id))
                                .any(|c| c.queued.iter().any(|q| q.id == tag && q.text == turn.text)),
                            "the message is durable before input"
                        );
                        sink.lock().unwrap().push((turn.text, tag));
                    }
                });
                let id = super::super::answer::tests::seed(&engine, session, &[("reply", &[])]);
                engine.with(|core| {
                    core.ledger.claude.billing = Billing::Subscription;
                    if tools_open {
                        core.live.get_mut(&id).unwrap().open_tools.push("read".into());
                    }
                    core.send_by_itself(&id, "Synthetic agent message".into(), Wake::of(why)).unwrap();
                    if tools_open {
                        assert!(delivered.lock().unwrap().is_empty(), "a running tool keeps its boundary");
                        core.live.get_mut(&id).unwrap().open_tools.clear();
                        core.hand_over_at_step(&id);
                    }
                    let sent = delivered.lock().unwrap().clone();
                    assert_eq!(sent.len(), 1, "the agent never needs the person's Send now: {why}");
                    assert_eq!(sent[0].0, "Synthetic agent message");
                    core.hand_over_at_step(&id);
                    assert_eq!(delivered.lock().unwrap().len(), 1, "already handed over is not sent twice");
                    core.taken_in(&id, Some(sent[0].1.clone()));
                    let c = &core.convs[&id];
                    assert!(c.queued.is_empty());
                    assert_eq!(c.messages[2].wake.as_ref().unwrap().why, why);
                    assert_eq!(c.messages[2].text, "Synthetic agent message");
                    assert!(!core.live[&id].by_hand, "an agent message is never a manual paid turn");
                });
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn agent_messages_preserve_stop_order_other_wakes_billing_quota_and_durable_input() {
        for blocked in ["stopped", "finished", "other-wake", "other-vendor", "older-wake", "unknown", "billed", "quota", "save", "goal"] {
            let dir = tempfile::tempdir().unwrap();
            let (engine, _) = super::super::answer::tests::engine(dir.path());
            let sent = Arc::new(Mutex::new(0));
            let output = sent.clone();
            let session = Session::from_fn(move |command| {
                if matches!(command, SessionCommand::Steer { .. }) {
                    *output.lock().unwrap() += 1;
                }
            });
            let id = super::super::answer::tests::seed(&engine, session, &[("reply", &[])]);
            engine.with(|core| {
                core.ledger.claude.billing = match blocked {
                    "unknown" => Billing::Unknown,
                    "billed" => Billing::UsageBilled,
                    _ => Billing::Subscription,
                };
                if blocked == "quota" {
                    let at = core.secs();
                    core.ledger.claude.record("seven_day", 1.0, at + 86400.0, at, 0);
                }
                let c = core.convs.get_mut(&id).unwrap();
                match blocked {
                    "stopped" => c.stopped = true,
                    "finished" => c.messages.last_mut().unwrap().finished_at = Some(3),
                    "other-vendor" => c.choice = Some(Choice { vendor: "codex".into(), model: None, effort: None }),
                    "older-wake" => c.queued.push(QueuedTurn {
                        id: "older".into(),
                        text: "Older continuation".into(),
                        choice: Choice { vendor: "auto".into(), model: None, effort: None },
                        images: None,
                        attachments: None,
                        at: 1,
                        sending: false,
                        wake: Some(Wake::of("wake.compacted")),
                    }),
                    "save" => std::fs::create_dir(dir.path().join("conversations").join(format!("{id}.json.tmp"))).unwrap(),
                    _ => {}
                }
                let why = if blocked == "other-wake" { "wake.keepGoing" } else { "wake.asked" };
                if blocked == "goal" {
                    core.live.get_mut(&id).unwrap().open_tools.push("read".into());
                }
                core.send_by_itself(&id, "Synthetic blocked message".into(), Wake::of(why)).unwrap();
                if blocked == "goal" {
                    core.convs.get_mut(&id).unwrap().goal_fix = Some("project".into());
                    std::fs::write(dir.path().join("projects.json"), b"unreadable goals").unwrap();
                    core.live.get_mut(&id).unwrap().open_tools.clear();
                }
                core.hand_over_at_step(&id);
                assert_eq!(*sent.lock().unwrap(), 0, "{blocked} still holds delivery");
                assert!(
                    core.convs[&id].queued.iter().any(|q| q.text == "Synthetic blocked message" && !q.sending),
                    "{blocked} keeps the message"
                );
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn agent_messages_never_deliver_older_human_input_before_its_step_boundary() {
        for why in ["wake.asked", "wake.childDone", "wake.keepGoing"] {
            for older_agent in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let (engine, _) = super::super::answer::tests::engine(dir.path());
                let delivered = Arc::new(Mutex::new(Vec::new()));
                let sink = delivered.clone();
                let session = Session::from_fn(move |command| {
                    if let SessionCommand::Steer { turn, .. } = command {
                        sink.lock().unwrap().push(turn.text);
                    }
                });
                let id = super::super::answer::tests::seed(&engine, session, &[("reply", &[])]);
                engine.with(|core| {
                    core.ledger.claude.billing = if older_agent { Billing::Unknown } else { Billing::Subscription };
                    if older_agent {
                        core.send_by_itself(&id, "Older agent message".into(), Wake::of("wake.asked")).unwrap();
                    }
                    core.enqueue(
                        &id,
                        SendRequest {
                            conversation_id: Some(id.clone()),
                            area: Area::Chat,
                            project_id: None,
                            text: "Older human input".into(),
                            images: vec![],
                            files: vec![],
                            choice: Choice { vendor: "claude".into(), model: None, effort: None },
                        },
                    )
                    .unwrap();
                    core.ledger.claude.billing = Billing::Subscription;
                    core.send_by_itself(&id, "Synthetic coordination".into(), Wake::of(why)).unwrap();
                    assert!(
                        delivered.lock().unwrap().iter().all(|text| text == "Older agent message"),
                        "{why} cannot take away the person's edit window"
                    );
                    let first = core.convs[&id].queued.iter().find(|q| q.wake.is_none()).unwrap().id.clone();
                    core.edit_queued(&id, &first, "Edited human input").unwrap();
                    core.hand_over_at_step(&id);
                    let sent = delivered.lock().unwrap();
                    assert_eq!(sent[usize::from(older_agent)], "Edited human input");
                    assert_eq!(sent.len(), usize::from(older_agent) + if why == "wake.keepGoing" { 1 } else { 2 });
                });
                engine.shutdown();
            }
        }
    }
}
