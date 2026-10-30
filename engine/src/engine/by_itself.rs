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
        self.send_by_itself_with_id(conv, text, wake, new_id())
    }

    pub(super) fn send_by_itself_with_id(&mut self, conv: &str, text: String, wake: Wake, id: String) -> Result<(), String> {
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
        let item = QueuedTurn { id, text, images: None, attachments: None, choice, at: now(), sending: false, wake: Some(wake) };
        // Faber's checks of the last reply count as a turn that runs (`verifying.rs`).
        if busy
            || c.stopped
            || !c.queued.is_empty()
            || verifying::checking(c)
            || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))
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
        let report = item.wake.as_ref().is_some_and(|w| w.why == "wake.childDone");
        let sent = self.dispatch_queued(conv, item);
        // A completion has no caller to retry it; other refused wakes are dropped so a caller's retry cannot duplicate them.
        if sent.is_err()
            && !report
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
    async fn a_child_report_retries_after_a_full_parent_inbox_has_room() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let parent = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("parent", &[])]);
        let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
        engine.with(|core| {
            core.live.clear();
            let c = core.convs.get_mut(&parent).unwrap();
            c.stopped = true;
            c.messages.last_mut().unwrap().status = Some(Status::Done);
            for i in 0..MAX_QUEUED {
                c.queued.push(QueuedTurn {
                    id: format!("queued-{i}"),
                    text: format!("Person's message {i}"),
                    images: None,
                    attachments: None,
                    choice: Choice { vendor: "auto".into(), model: None, effort: None },
                    at: now(),
                    sending: false,
                    wake: None,
                });
            }
            core.save(&parent);
            let c = core.convs.get_mut(&child).unwrap();
            c.started_by = Some(parent.clone());
            let reply = c.messages.last_mut().unwrap();
            reply.status = Some(Status::Done);
            reply.text = "The requested documentation is saved.".into();
            core.save(&child);
            core.tell_parent(&child);
            assert_eq!(core.store.get(&parent).unwrap().queued.len(), MAX_QUEUED);
            core.remove_queued(&parent, "queued-0");
            let saved = core.store.get(&parent).unwrap();
            assert_eq!(saved.queued.len(), MAX_QUEUED, "the deferred report must occupy the freed slot");
            assert!(saved.queued.last().unwrap().text.contains("The requested documentation is saved."));
            assert_eq!(saved.queued.last().unwrap().wake.as_ref().unwrap().why, "wake.childDone");
            assert_eq!(saved.queued[0].id, "queued-1", "existing messages keep their order");
            assert!(core.convs[&parent].stopped, "refilling the inbox must not resume stopped work");
            assert!(!core.store.get(&child).unwrap().pending_parent_report);
            // A crash after the parent save but before the child receipt save leaves this stale marker.
            core.convs.get_mut(&child).unwrap().pending_parent_report = true;
            core.save(&child);
            core.convs.get_mut(&parent).unwrap().queued.remove(0);
            core.save(&parent);
        });
        engine.shutdown();
        drop(engine);
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|core| {
            let saved = core.store.get(&parent).unwrap();
            assert_eq!(saved.queued.len(), MAX_QUEUED - 1, "startup must recognize the already saved report without duplication");
            assert_eq!(saved.queued.iter().filter(|q| q.wake.as_ref().is_some_and(|w| w.why == "wake.childDone")).count(), 1);
            assert!(!core.store.get(&child).unwrap().pending_parent_report);
            // A crash after saving a free slot, before refilling it, must not need another queue event.
            core.convs.get_mut(&child).unwrap().pending_parent_report = true;
            core.save(&child);
            core.convs.get_mut(&parent).unwrap().queued.retain(|q| q.wake.is_none());
            core.save(&parent);
        });
        engine.shutdown();
        drop(engine);
        let (engine, _) = super::super::answer::tests::engine(&dir);
        engine.with(|core| {
            let saved = core.store.get(&parent).unwrap();
            assert_eq!(saved.queued.len(), MAX_QUEUED - 1, "startup must retry a pending report when a slot is already free");
            assert!(saved.queued.last().unwrap().text.contains("The requested documentation is saved."));
            assert!(!core.store.get(&child).unwrap().pending_parent_report);
            core.tell_parent(&child);
            assert_eq!(
                core.store.get(&parent).unwrap().queued.len(),
                saved.queued.len(),
                "a repeated completion must not duplicate the notice"
            );
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn automatic_messages_hold_after_stop_even_when_the_conversation_is_idle() {
        for (why, status) in [("wake.asked", Status::Done), ("wake.childDone", Status::Stopped), ("wake.keepGoing", Status::Error)] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = super::super::answer::tests::engine(&dir);
            let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
            engine.with(|core| {
                core.live.clear();
                core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
                core.ledger.claude.billing = Billing::Subscription;
                let c = core.convs.get_mut(&id).unwrap();
                c.stopped = true;
                c.messages.last_mut().unwrap().status = Some(status);
                assert_eq!(core.send_by_itself(&id, "Synthetic held message".into(), Wake::of(why)), Ok(()));
                core.drain(&id);
                let saved = core.store.get(&id).unwrap();
                assert!(core.convs[&id].stopped, "{why} must keep the live Stop state");
                assert_eq!(saved.messages.len(), 2, "{why} must not start another turn after Stop");
                assert_eq!(saved.queued.len(), 1, "{why} stays available for the person's explicit resume");
                assert_eq!(saved.queued[0].text, "Synthetic held message");
                assert_eq!(saved.queued[0].wake.as_ref().unwrap().why, why);
                assert!(!saved.queued[0].sending);
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn an_explicit_plan_approval_starts_once_after_stop_in_an_idle_conversation() {
        let dir = crate::workspace::TestRoot::new();
        let (engine, _) = super::super::answer::tests::engine(&dir);
        let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
        engine.with(|core| {
            core.live.clear();
            core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
            core.ledger.claude.billing = Billing::Subscription;
            let c = core.convs.get_mut(&id).unwrap();
            c.stopped = true;
            c.messages.last_mut().unwrap().status = Some(Status::Stopped);
            core.send_by_itself(&id, "Go ahead with the plan.".into(), Wake::of("wake.approvedPlan")).unwrap();
            core.drain(&id);
            let saved = core.store.get(&id).unwrap();
            assert!(!core.convs[&id].stopped);
            assert!(saved.queued.is_empty());
            assert_eq!(saved.messages.len(), 4, "the person's approval starts exactly one turn");
            assert_eq!(saved.messages[2].wake.as_ref().unwrap().why, "wake.approvedPlan");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn refused_questions_and_continuations_do_not_leave_hidden_retries() {
        for why in ["wake.asked", "wake.keepGoing", "wake.approvedPlan"] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = super::super::answer::tests::engine(&dir);
            let id = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("reply", &[])]);
            engine.with(|core| {
                core.live.clear();
                core.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().status = Some(Status::Done);
                assert_eq!(core.send_by_itself(&id, "Synthetic refused message".into(), Wake::of(why)), Err("error.noSubscription".into()));
                assert!(core.convs[&id].queued.is_empty(), "{why} must not retry after telling its caller it failed");
                assert!(core.store.get(&id).unwrap().queued.is_empty());
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn child_reports_survive_refused_dispatch_to_an_idle_parent() {
        for installing in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = super::super::answer::tests::engine(&dir);
            let parent = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("parent", &[])]);
            let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
            engine.with(|core| {
                core.installing = installing;
                core.live.clear();
                core.convs.get_mut(&parent).unwrap().messages.last_mut().unwrap().status = Some(Status::Done);
                let c = core.convs.get_mut(&child).unwrap();
                c.started_by = Some(parent.clone());
                let reply = c.messages.last_mut().unwrap();
                reply.status = Some(Status::Done);
                reply.text = "The requested documentation is saved.".into();
                core.tell_parent(&child);
                let saved = core.store.get(&parent).unwrap();
                assert_eq!(saved.queued.len(), 1, "a refused start must not lose the completion report: installing={installing}");
                let report = &saved.queued[0];
                assert_eq!(report.wake.as_ref().unwrap().why, "wake.childDone");
                assert!(report.text.contains("The requested documentation is saved."));
                assert!(!report.sending);
                assert_eq!(saved.messages.len(), 2, "a refused start creates no extra turn");
                core.drain(&parent);
                let retried = core.store.get(&parent).unwrap();
                assert_eq!(retried.queued.len(), 1, "a still-refused retry keeps exactly one report");
                assert_eq!(retried.queued[0].id, report.id);
                core.installing = false;
                core.opts.clis.insert(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] });
                core.ledger.claude.billing = Billing::Subscription;
                core.drain(&parent);
                core.drain(&parent);
                let resumed = core.store.get(&parent).unwrap();
                assert!(resumed.queued.is_empty(), "an admitted retry consumes the saved report");
                assert_eq!(resumed.messages.len(), 4, "the report starts exactly one turn");
                assert_eq!(resumed.messages[2].id, report.id);
                assert_eq!(resumed.messages[2].text, report.text);
                assert_eq!(resumed.messages[2].wake.as_ref().unwrap().why, "wake.childDone");
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn a_refused_child_report_goes_out_by_itself_once_an_ai_can_answer_again() {
        // Each way an AI comes back unlocks the report on its own: the install ends, the CLI is found.
        for back in ["install", "clis"] {
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = super::super::answer::tests::engine(&dir);
            let parent = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("parent", &[])]);
            let child = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("child", &[])]);
            let held = super::super::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("held", &[])]);
            let claude = Cli { command: "claude".into(), prefix: vec![] };
            let guard = (back == "install").then(|| engine.install_admission_stopping().unwrap());
            engine.with(|core| {
                core.live.clear();
                if back != "clis" {
                    core.opts.clis.insert(Vendor::Claude, claude.clone());
                }
                core.ledger.claude.billing = Billing::Subscription;
                for id in [&parent, &held] {
                    core.convs.get_mut(id).unwrap().messages.last_mut().unwrap().status = Some(Status::Done);
                }
                // A message the person queued in a conversation nothing runs in stays theirs to send.
                let mine = QueuedTurn {
                    id: "mine".into(),
                    text: "Person's own message".into(),
                    images: None,
                    attachments: None,
                    choice: Choice { vendor: "auto".into(), model: None, effort: None },
                    at: now(),
                    sending: false,
                    wake: None,
                };
                core.convs.get_mut(&held).unwrap().queued.push(mine);
                core.save(&held);
                let c = core.convs.get_mut(&child).unwrap();
                c.started_by = Some(parent.clone());
                let reply = c.messages.last_mut().unwrap();
                reply.status = Some(Status::Done);
                reply.text = "The requested documentation is saved.".into();
                core.tell_parent(&child);
                assert_eq!(core.store.get(&parent).unwrap().queued.len(), 1, "the refused report waits: {back}");
            });
            drop(guard);
            engine.with(|core| {
                if back == "clis" {
                    let mut found = core.opts.clis.clone();
                    found.insert(Vendor::Claude, claude.clone());
                    core.replace_clis(found);
                }
                let resumed = core.store.get(&parent).unwrap();
                assert!(resumed.queued.is_empty(), "the report went out with no turn ending first: {back}");
                assert_eq!(resumed.messages[2].wake.as_ref().unwrap().why, "wake.childDone");
                assert_eq!(core.store.get(&held).unwrap().queued.len(), 1, "the person's own message is not sent for them");
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn agent_messages_reach_the_running_turn_once_and_keep_their_origin() {
        for why in ["wake.asked", "wake.childDone"] {
            for tools_open in [false, true] {
                let dir = crate::workspace::TestRoot::new();
                let (engine, _) = super::super::answer::tests::engine(&dir);
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
            let dir = crate::workspace::TestRoot::new();
            let (engine, _) = super::super::answer::tests::engine(&dir);
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
                let dir = crate::workspace::TestRoot::new();
                let (engine, _) = super::super::answer::tests::engine(&dir);
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
