use super::*;
pub(super) fn available(c: &Conversation, vendor: Vendor) -> bool {
    c.native_fork.as_ref().is_some_and(|f| f.vendor == vendor && f.point.valid(vendor))
}
pub(super) fn selected(history: &[Message], vendor: Vendor) -> Option<NativeFork> {
    let last = history.last()?;
    if last.role != Role::Assistant || last.vendor != Some(vendor) || last.status != Some(Status::Done) {
        return None;
    }
    last.native_point.as_ref().filter(|point| point.valid(vendor)).cloned().map(|point| NativeFork { vendor, point })
}
pub(super) fn inherit(source: &Conversation, target: &mut Conversation, vendor: Vendor) {
    target.native_fork = selected(&target.messages, vendor);
    target.user_name.clone_from(&source.user_name);
    target.persona_version = source.persona_version;
    target.fresh_after = source.fresh_after.clone().filter(|id| target.messages.iter().any(|m| &m.id == id));
    if let Some(fork) = &target.native_fork {
        if vendor == Vendor::Codex {
            target.codex_plan = source.codex_plan;
        }
        if let Some(kit) = source.session_kits.get(&vendor).filter(|kit| kit.0 == fork.point.session) {
            target.session_kits.insert(vendor, kit.clone());
            if let Some(card) = source.session_start_cards.get(&vendor) {
                target.session_start_cards.insert(vendor, card.clone());
            }
            if let Some(digest) = source.session_resource_digests.get(&vendor) {
                target.session_resource_digests.insert(vendor, digest.clone());
            }
            if let Some(digest) = source.session_guide_digests.get(&vendor) {
                target.session_guide_digests.insert(vendor, digest.clone());
            }
        }
    } else if !target.context_messages().is_empty() {
        target.rewind_notice = Some("note.rewind.savedMessages".into());
    }
}
impl Core {
    pub(super) fn native_fork_start(&mut self, id: &str, reply: &str, pick: &Pick, text: String) -> String {
        let Some(fork) = self.convs.get(id).and_then(|c| c.native_fork.clone()) else { return text };
        let supported = fork.vendor == pick.vendor && fork.point.valid(pick.vendor) && (pick.vendor == Vendor::Codex || self.claude_fork);
        if supported {
            return text;
        }
        let request = self.transfers.get(id).filter(|t| t.reply == reply).map(|t| t.request.clone()).unwrap_or(text);
        let c = self.convs.get_mut(id).expect("conversation exists");
        c.native_fork = None;
        c.rewind_notice = Some("note.rewind.savedMessages".into());
        if let Some(m) = c.messages.iter_mut().find(|m| m.id == reply) {
            m.note = Some("note.rewind.savedMessages".into());
        }
        let history = c.context_messages();
        let end = history.iter().position(|m| m.id == reply).unwrap_or(history.len()).saturating_sub(1);
        with_history(&history[..end], &request, &|n| attachment_place(c.area, &self.opts.data_dir, id, n))
    }
    pub(super) fn native_fork_event(&mut self, id: &str, event: &EngineEvent) -> bool {
        let Some(live) = self.live.get(id) else { return false };
        let vendor = live.vendor;
        let reply = live.turn.clone();
        if let EngineEvent::Cursor { entry, turn } = event {
            let session = live.unsaved_session.clone().or_else(|| self.convs.get(id)?.resumable(vendor).cloned());
            if let (Some(session), Some(reply)) = (session, reply.as_ref()) {
                let point = NativePoint { session, cursor: crate::rewind::Cursor { entry: entry.clone(), turn: turn.clone() } };
                if point.valid(vendor)
                    && let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| &m.id == reply))
                {
                    m.native_point = Some(point);
                }
            }
        }
        let Some(fork) = self.convs.get(id).and_then(|c| c.native_fork.clone()) else { return false };
        if let EngineEvent::Started { session_id, .. } = event {
            if session_id != &fork.point.session {
                let c = self.convs.get_mut(id).expect("known conversation");
                c.native_fork = None;
                c.tentative.insert(vendor, session_id.clone());
                self.save(id);
                return false;
            }
        } else if !matches!(event, EngineEvent::Error { fatal: true, .. }) {
            return false;
        }
        if !self.reply_has_work(id, reply.as_deref()) {
            let c = self.convs.get_mut(id).expect("known conversation");
            c.native_fork = None;
            c.forget_session(vendor);
            c.rewind_notice = Some("note.rewind.savedMessages".into());
            self.save(id);
            return self.rerun_fresh(id, reply.as_deref());
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::recovery::Transfer;
    use crate::rewind::Cursor;
    #[test]
    fn native_branch_selects_only_the_complete_exact_prefix_and_survives_storage() {
        let mut m = Message::new("a".into(), Role::Assistant, "done".into(), 0);
        m.vendor = Some(Vendor::Codex);
        m.status = Some(Status::Done);
        m.native_point = Some(NativePoint { session: "source".into(), cursor: Cursor { entry: None, turn: Some("kept".into()) } });
        let chosen = selected(&[m.clone()], Vendor::Codex).expect("native prefix");
        assert_eq!(chosen.point.cursor.turn.as_deref(), Some("kept"));
        let saved = serde_json::to_vec(&chosen).unwrap();
        assert_eq!(serde_json::from_slice::<NativeFork>(&saved).unwrap(), chosen);
        assert!(selected(&[m.clone()], Vendor::Claude).is_none());
        assert!(selected(&[Message { status: Some(Status::Streaming), ..m.clone() }], Vendor::Codex).is_none());
        assert!(selected(&[m, Message::new("q".into(), Role::User, "later".into(), 0)], Vendor::Codex).is_none());
    }
    #[test]
    fn native_branch_inherits_plan_state_but_never_source_sessions_or_grants() {
        let blank = || {
            serde_json::from_value::<Conversation>(serde_json::json!({
                "id":"source","title":"source","area":"chat","createdAt":0,"updatedAt":0,"messages":[]
            }))
            .unwrap()
        };
        let mut source = blank();
        source.codex_plan = true;
        source.sessions.insert(Vendor::Codex, "source".into());
        source.allow_always = Some(vec!["Read".into()]);
        let mut message = Message::new("answer".into(), Role::Assistant, "done".into(), 0);
        message.status = Some(Status::Done);
        message.vendor = Some(Vendor::Codex);
        message.native_point = Some(NativePoint { session: "source".into(), cursor: Cursor { entry: None, turn: Some("kept".into()) } });
        let mut target = blank();
        target.id = "branch".into();
        target.messages.push(message);
        inherit(&source, &mut target, Vendor::Codex);
        assert!(target.codex_plan && target.native_fork.is_some());
        assert!(target.sessions.is_empty() && target.tentative.is_empty() && target.allow_always.is_none());
    }

    #[tokio::test]
    async fn native_branch_move_to_a_project_discards_the_pending_source_context() {
        let data = crate::workspace::TestRoot::new();
        let (engine, _) = crate::engine::answer::tests::engine(&data);
        std::fs::create_dir_all(data.work()).unwrap();
        let project = engine.add_project(&data.work().to_string_lossy()).unwrap();
        let id = engine.with(|core| {
            let source = core.blank(Area::Chat, None);
            let c = core.convs.get_mut(&source).unwrap();
            let mut reply = Message::new("answer".into(), Role::Assistant, "Earlier context".into(), 1);
            reply.vendor = Some(Vendor::Codex);
            reply.status = Some(Status::Done);
            reply.native_point = Some(NativePoint { session: "source".into(), cursor: Cursor { entry: None, turn: Some("kept".into()) } });
            c.messages.push(reply);
            let source = c.clone();
            let id = core.blank(Area::Chat, None);
            let target = core.convs.get_mut(&id).unwrap();
            target.messages = source.messages.clone();
            inherit(&source, target, Vendor::Codex);
            assert!(available(target, Vendor::Codex));
            core.save(&id);
            id
        });
        assert_eq!(
            engine.chat_folder(Some(&id), Some(data.work())).unwrap_err(),
            "error.chatFolderLocked",
            "a branch with history cannot change its Chat folder"
        );
        let moved = engine.move_to_project(&id, &project.id).unwrap();
        assert!(moved.native_fork.is_none(), "a different work context must never fork the old folder's session");
        assert_eq!(moved.rewind_notice.as_deref(), Some("note.rewind.savedMessages"));
        assert_eq!(moved.messages[0].text, "Earlier context");
        engine.with(|core| assert!(core.store.get(&id).unwrap().native_fork.is_none(), "the reset must survive reload"));
        engine.shutdown();
    }

    fn transfer(request: &str, text: &str) -> Transfer {
        Transfer {
            reply: "retry".into(),
            pick: Pick { vendor: Vendor::Claude, model: None, effort: "medium".into(), reason: "reason.manual".into(), pace: None },
            request: request.into(),
            text: text.into(),
            images: vec![],
            by_hand: true,
            approved: None,
            retried: false,
            moving: false,
            check: "glance",
            scope: "tight",
            confidence: 0.95,
        }
    }

    #[tokio::test]
    async fn native_branch_rejects_stale_capabilities_and_invalid_points_before_tracking_a_process() {
        for stale_cli in [true, false] {
            let data = crate::workspace::TestRoot::new();
            let (engine, _) = crate::engine::answer::tests::engine(&data);
            engine.with(|core| {
                let id = core.blank(Area::Chat, None);
                let path = data.path().join("synthetic-cli.txt");
                std::fs::write(&path, "synthetic CLI metadata, never executed").unwrap();
                let cli = Cli { command: path.to_str().unwrap().into(), prefix: vec![] };
                core.claude_stamp = Some(if stale_cli { "previous CLI version".into() } else { cli.stamp() });
                core.claude_fork = true;
                core.opts.clis.insert(Vendor::Claude, cli);
                core.convs.get_mut(&id).unwrap().native_fork = Some(NativeFork {
                    vendor: Vendor::Claude,
                    point: NativePoint {
                        session: "13114353-380b-4dcb-8ee7-4df689be7c1d".into(),
                        cursor: Cursor {
                            entry: Some(if stale_cli { "8d292943-1c7f-49e2-9812-64063b277598" } else { "invalid-boundary" }.into()),
                            turn: None,
                        },
                    },
                });
                let pick = transfer("request", "request").pick;
                let mode = if stale_cli { kits::SessionStart::Turn } else { kits::SessionStart::Prepare };
                assert_eq!(core.launch_session(&id, &pick, mode), Err("error.sessionLost".into()));
                assert!(
                    !core.rewind_tracks_session(&id, core.next_sid),
                    "a rejected launch has no process whose exit could clear restore tracking"
                );
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn native_branch_unsupported_fork_rebuilds_only_the_kept_context_and_raw_request() {
        let data = crate::workspace::TestRoot::new();
        let (engine, _) = crate::engine::answer::tests::engine(&data);
        let id =
            crate::engine::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("forgotten", &[]), ("kept", &[]), ("retry", &[])]);
        engine.with(|core| {
            let c = core.convs.get_mut(&id).unwrap();
            c.messages[1].text = "FORGOTTEN_CONTEXT".into();
            c.messages[3].text = "KEPT_CONTEXT".into();
            c.messages[4].text = "CURRENT_QUESTION_MUST_NOT_BE_REPLAYED".into();
            c.fresh_after = Some("forgotten".into());
            c.native_fork = Some(NativeFork {
                vendor: Vendor::Claude,
                point: NativePoint {
                    session: "13114353-380b-4dcb-8ee7-4df689be7c1d".into(),
                    cursor: Cursor { entry: Some("8d292943-1c7f-49e2-9812-64063b277598".into()), turn: None },
                },
            });
            let pending = transfer("RAW_REQUEST_ONLY", "ALREADY_WRAPPED_HISTORY");
            let pick = pending.pick.clone();
            core.transfers.insert(id.clone(), pending);
            core.claude_fork = false;
            let text = core.native_fork_start(&id, "retry", &pick, "ALREADY_WRAPPED_HISTORY".into());
            assert_eq!(text.matches("KEPT_CONTEXT").count(), 1);
            assert_eq!(text.matches("RAW_REQUEST_ONLY").count(), 1);
            for excluded in ["FORGOTTEN_CONTEXT", "CURRENT_QUESTION_MUST_NOT_BE_REPLAYED", "ALREADY_WRAPPED_HISTORY"] {
                assert!(!text.contains(excluded), "fallback leaked or duplicated {excluded}");
            }
            assert!(core.convs[&id].native_fork.is_none());
            assert_eq!(core.convs[&id].rewind_notice.as_deref(), Some("note.rewind.savedMessages"));
            assert_eq!(core.convs[&id].messages.last().unwrap().note.as_deref(), Some("note.rewind.savedMessages"));
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn native_branch_source_id_and_prework_error_retire_the_old_session_before_normal_started() {
        for event in [
            EngineEvent::Started { session_id: "13114353-380b-4dcb-8ee7-4df689be7c1d".into(), model: "SOURCE_MUST_NOT_BE_ACCEPTED".into() },
            EngineEvent::Error { message: "unsupported native fork".into(), fatal: true },
        ] {
            let data = crate::workspace::TestRoot::new();
            let (engine, _) = crate::engine::answer::tests::engine(&data);
            let commands = Arc::new(Mutex::new(Vec::new()));
            let log = commands.clone();
            let session = Session::from_fn(move |command| {
                use crate::events::SessionCommand;
                let label = match command {
                    SessionCommand::Close => "close",
                    SessionCommand::Send(_) => "send",
                    _ => "other",
                };
                log.lock().unwrap().push(label);
            });
            let id = crate::engine::answer::tests::seed(&engine, session, &[("kept", &[]), ("retry", &[])]);
            engine.with(|core| {
                let c = core.convs.get_mut(&id).unwrap();
                c.messages[1].text = "KEPT_CONTEXT".into();
                c.native_fork = Some(NativeFork {
                    vendor: Vendor::Claude,
                    point: NativePoint {
                        session: "13114353-380b-4dcb-8ee7-4df689be7c1d".into(),
                        cursor: Cursor { entry: Some("8d292943-1c7f-49e2-9812-64063b277598".into()), turn: None },
                    },
                });
                core.transfers.insert(id.clone(), transfer("RAW_REQUEST_ONLY", "ORIGINAL_REQUEST"));
                assert!(!core.reply_has_work(&id, Some("retry")));
                core.handle(&id, 1, event);
                let c = &core.convs[&id];
                assert!(c.native_fork.is_none());
                assert!(!c.sessions.contains_key(&Vendor::Claude) && !c.tentative.contains_key(&Vendor::Claude));
                let reply = c.messages.last().unwrap();
                assert_ne!(reply.model.as_deref(), Some("SOURCE_MUST_NOT_BE_ACCEPTED"));
                assert_eq!(reply.note.as_deref(), Some("note.historyResent"), "rerun_fresh must reconstruct the saved prefix");
                assert!(!core.live.contains_key(&id), "no CLI is connected in this harness; failed fallback must leave no live source");
                core.handle(&id, 1, EngineEvent::Started { session_id: "late-source".into(), model: "LATE_SOURCE".into() });
                assert_ne!(core.convs[&id].messages.last().unwrap().model.as_deref(), Some("LATE_SOURCE"));
                core.try_save(&id).unwrap();
                assert!(core.store.get(&id).unwrap().native_fork.is_none());
            });
            assert!(commands.lock().unwrap().contains(&"close"));
            assert!(!commands.lock().unwrap().contains(&"send"), "the rejected session must receive no additional input");
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn native_branch_cursor_records_the_live_session_and_persists_the_exact_completed_prefix() {
        let data = crate::workspace::TestRoot::new();
        let (engine, _) = crate::engine::answer::tests::engine(&data);
        let id = crate::engine::answer::tests::seed(&engine, Session::from_fn(|_| {}), &[("kept", &[])]);
        engine.with(|core| {
            core.live.get_mut(&id).unwrap().unsaved_session = Some("13114353-380b-4dcb-8ee7-4df689be7c1d".into());
            core.handle(&id, 1, EngineEvent::Cursor { entry: Some("8d292943-1c7f-49e2-9812-64063b277598".into()), turn: None });
            let c = core.convs.get_mut(&id).unwrap();
            c.messages[1].status = Some(Status::Done);
            c.messages.push(Message::new("later-question".into(), Role::User, "Must not enter the kept prefix".into(), 3));
            core.try_save(&id).unwrap();
            let stored = core.store.get(&id).unwrap();
            let fork = selected(&stored.messages[..2], Vendor::Claude).expect("recorded completed boundary");
            assert_eq!(fork.point.session, "13114353-380b-4dcb-8ee7-4df689be7c1d");
            assert_eq!(fork.point.cursor.entry.as_deref(), Some("8d292943-1c7f-49e2-9812-64063b277598"));
            assert!(selected(&stored.messages, Vendor::Claude).is_none(), "a later user message is not an assistant boundary");
        });
        engine.shutdown();
    }
}
