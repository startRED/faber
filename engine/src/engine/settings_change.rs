//! Bounded, separately authorized changes to the person's web-search preference.
use super::*;
use crate::model::WebSearch;
use crate::toolbox as api;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    web_search: WebSearch,
}

#[derive(Clone)]
pub(super) struct PendingSetting {
    call: String,
    sid: u64,
    turn: String,
    from: WebSearch,
    to: WebSearch,
    revision: u64,
    epoch: Option<String>,
    permissions_revision: Option<String>,
    mode: Permissions,
}

/// Only constructed after the setting-specific admission check; never a generic persistence callback.
pub(super) struct SavedReceipt {
    conv: String,
    call: String,
    sid: u64,
    session: Session,
}

fn authorized(how: &str, remember: bool, trust: bool, grant: bool) -> bool {
    match how {
        "once" => true,
        "trust" => trust,
        "always" => remember || grant,
        _ => false,
    }
}

async fn record(dir: PathBuf, conv: String, from: WebSearch, to: WebSearch, how: &'static str, result: &'static str) -> bool {
    tokio::task::spawn_blocking(move || api::log_web_search(&dir, now(), &conv, from, to, how, result)).await.unwrap_or(false)
}

impl Core {
    fn setting_live(&self, conv: &str, asked: &PendingSetting) -> bool {
        let settings = self.store.settings();
        self.convs.get(conv).is_some_and(|c| {
            c.area == Area::Project && !c.stopped && c.messages.iter().any(|m| m.id == asked.turn && m.status == Some(Status::Streaming))
        }) && self.live.get(conv).is_some_and(|l| {
            l.sid == asked.sid
                && l.turn.as_deref() == Some(&asked.turn)
                && l.permissions == asked.mode
                && !matches!(l.permissions, Permissions::Plan | Permissions::Isolated)
        }) && settings.permissions_revision == asked.permissions_revision
            && settings.grants_epoch == asked.epoch
            && settings.tuning().web == asked.from
            && self.web_revision == asked.revision
    }

    pub(super) fn settings_call(&mut self, conv: &str, sid: u64, call: String, input: serde_json::Value) {
        let Ok(request) = serde_json::from_value::<Request>(input) else {
            return self.answer_toolbox(conv, sid, &call, false, "Use only web_search: off, cached or live.".into());
        };
        let Some(live) = self.live.get(conv) else { return };
        let settings = self.store.settings();
        let asked = PendingSetting {
            call,
            sid,
            turn: live.turn.clone().unwrap_or_default(),
            from: settings.tuning().web,
            to: request.web_search,
            revision: self.web_revision,
            epoch: settings.grants_epoch.clone(),
            permissions_revision: settings.permissions_revision.clone(),
            mode: live.permissions,
        };
        if !self.setting_live(conv, &asked) {
            return self.answer_toolbox(conv, sid, &asked.call, false, "This project turn cannot change Faber's settings.".into());
        }
        if self.settings_changes.contains_key(conv)
            || self.turn(conv).is_some_and(|m| m.permissions.iter().flatten().any(|p| api::is_self_change(&p.tool)))
        {
            return self.answer_toolbox(
                conv,
                sid,
                &asked.call,
                false,
                "Another change to Faber is already waiting or being saved. Finish it before requesting another.".into(),
            );
        }
        if settings.faber_trust() {
            return self.run_setting(conv, asked, true, "trust", false);
        }
        if self.grant_stands(conv, api::SELF_WEB_SEARCH) {
            return self.run_setting(conv, asked, true, "always", false);
        }
        let card = Permission {
            request_id: asked.call.clone(),
            tool: api::SELF_WEB_SEARCH.into(),
            input: serde_json::json!({ "from": asked.from, "to": asked.to }),
            epoch: asked.epoch.clone(),
        };
        if let Some(message) = self.turn(conv) {
            message.permissions.get_or_insert_default().push(card);
        }
        let turn = asked.turn.clone();
        self.settings_changes.insert(conv.into(), asked);
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(&turn), true);
    }

    pub(super) fn answer_setting(&mut self, conv: &str, turn: &str, call: &str, allow: bool, always: bool) -> Option<Result<(), String>> {
        let card = self
            .convs
            .get(conv)?
            .messages
            .iter()
            .find(|m| m.id == turn)?
            .permissions
            .iter()
            .flatten()
            .find(|p| p.request_id == call && p.tool == api::SELF_WEB_SEARCH)?
            .clone();
        let asked = self.settings_changes.remove(conv).filter(|a| {
            a.call == call && a.turn == turn && card.epoch == a.epoch && card.input == serde_json::json!({ "from": a.from, "to": a.to })
        });
        if let Some(message) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == turn)) {
            message.permissions.get_or_insert_default().retain(|p| !(p.request_id == call && p.tool == api::SELF_WEB_SEARCH));
        }
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(turn), true);
        let Some(asked) = asked else { return Some(Err(super::answer::STALE.into())) };
        if !self.setting_live(conv, &asked) {
            self.answer_toolbox(conv, asked.sid, call, false, "The turn or saved preference changed; request fresh permission.".into());
            return Some(Err(super::answer::STALE.into()));
        }
        self.run_setting(conv, asked, allow, if allow && always { "always" } else { "once" }, allow && always);
        Some(Ok(()))
    }

    fn run_setting(&mut self, conv: &str, asked: PendingSetting, allow: bool, how: &'static str, remember: bool) {
        if self.installing {
            return self.answer_toolbox(
                conv,
                asked.sid,
                &asked.call,
                false,
                "Faber is about to restart for an update; no setting was changed.".into(),
            );
        }
        self.settings_changes.insert(conv.into(), asked.clone());
        self.agent_installs += 1;
        let (me, dir, conv) = (self.me.clone(), self.opts.data_dir.clone(), conv.to_string());
        #[cfg(test)]
        let audit_barrier = self.settings_audit_barrier.take();
        self.opts.runtime.spawn(async move {
            let running = super::toolbox::Running(me.clone());
            let recorded = record(dir.clone(), conv.clone(), asked.from, asked.to, how, if allow { "allowed" } else { "declined" }).await;
            #[cfg(test)]
            if let Some((entered, release)) = audit_barrier {
                let _ = entered.send(());
                let _ = release.await;
            }
            let Some(core) = me.upgrade() else { return };
            let outcome = {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                let pending =
                    core.settings_changes.get(&conv).is_some_and(|a| a.call == asked.call && a.sid == asked.sid && a.turn == asked.turn);
                if pending {
                    core.settings_changes.remove(&conv);
                }
                if !recorded {
                    core.answer_toolbox(
                        &conv,
                        asked.sid,
                        &asked.call,
                        false,
                        "Nothing was changed: Faber could not record the web-search decision.".into(),
                    );
                    None
                } else if !allow {
                    core.answer_toolbox(
                        &conv,
                        asked.sid,
                        &asked.call,
                        false,
                        "The person said no. Do not retry; they can change web search in Settings.".into(),
                    );
                    None
                } else if !pending
                    || core.installing
                    || !core.setting_live(&conv, &asked)
                    || !authorized(how, remember, core.store.settings().faber_trust(), core.grant_stands(&conv, api::SELF_WEB_SEARCH))
                {
                    core.answer_toolbox(
                        &conv,
                        asked.sid,
                        &asked.call,
                        false,
                        "The turn or saved preference changed; nothing was changed.".into(),
                    );
                    Some(false)
                } else {
                    let receipt = SavedReceipt {
                        conv: conv.clone(),
                        call: asked.call.clone(),
                        sid: asked.sid,
                        session: core.live[&conv].session.clone(),
                    };
                    let result =
                        core.save_settings_with_receipt(Settings { web_search: Some(asked.to), ..Settings::default() }, Some(receipt));
                    match result {
                        Ok(_) => {
                            if remember && let Some(c) = core.convs.get_mut(&conv) {
                                let grants = c.allow_always.get_or_insert_default();
                                if !grants.iter().any(|t| t == api::SELF_WEB_SEARCH) {
                                    grants.push(api::SELF_WEB_SEARCH.into());
                                }
                                match &asked.epoch {
                                    Some(epoch) => {
                                        c.allow_always_epochs.insert(api::SELF_WEB_SEARCH.into(), epoch.clone());
                                    }
                                    None => {
                                        c.allow_always_epochs.remove(api::SELF_WEB_SEARCH);
                                    }
                                }
                                core.save(&conv);
                            }
                            Some(true)
                        }
                        Err(_) => {
                            core.answer_toolbox(
                                &conv,
                                asked.sid,
                                &asked.call,
                                false,
                                "Could not save web search. Nothing was changed.".into(),
                            );
                            Some(false)
                        }
                    }
                }
            };
            if let Some(saved) = outcome
                && !record(dir, conv, asked.from, asked.to, how, if saved { "saved" } else { "failed" }).await
            {
                eprintln!("faber: could not record the web-search change result");
            }
            drop(running);
        });
    }

    pub(super) fn settle_saved_setting(&mut self, receipt: SavedReceipt) {
        let text = "Saved Faber's web search preference. A stricter choice stops replies in progress, including this one. Future replies use the saved choice.".to_string();
        // Session commands are ordered: queue the result after persistence and before retirement's interrupt/close.
        receipt.session.helper_reply(&receipt.call, true, text.clone());
        self.settings_changes.remove(&receipt.conv);
        if self.tool_of(&receipt.conv, &receipt.call).is_none() {
            self.handle(
                &receipt.conv,
                receipt.sid,
                EngineEvent::Tool {
                    id: receipt.call.clone(),
                    name: api::SETTINGS_TOOL.into(),
                    input: serde_json::json!({ "web_search": self.store.settings().tuning().web }),
                    parent: None,
                },
            );
        }
        self.handle(&receipt.conv, receipt.sid, EngineEvent::ToolResult { id: receipt.call, ok: true, declined: false, text });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn revoked_during_audit(trust: bool) {
        use super::super::answer::tests::{engine, seed};
        let dir = crate::workspace::TestRoot::new();
        let (app, _) = engine(&dir);
        let (reply, mut replies) = tokio::sync::mpsc::unbounded_channel();
        let session = Session::from_fn(move |command| {
            if let crate::events::SessionCommand::HelperReply { id, ok, text, .. } = command {
                let _ = reply.send((id, ok, text));
            }
        });
        let id = seed(&app, session, &[("turn", &[])]);
        let (entered, paused) = tokio::sync::oneshot::channel();
        let (release, resume) = tokio::sync::oneshot::channel();
        app.with(|core| {
            core.convs.get_mut(&id).unwrap().area = Area::Project;
            core.save_settings(Settings { faber_trust: Some(trust), web_search: Some(WebSearch::Live), ..Settings::default() }).unwrap();
            if !trust {
                let epoch = core.store.settings().grants_epoch;
                let conv = core.convs.get_mut(&id).unwrap();
                conv.allow_always = Some(vec![api::SELF_WEB_SEARCH.into()]);
                if let Some(epoch) = epoch {
                    conv.allow_always_epochs.insert(api::SELF_WEB_SEARCH.into(), epoch);
                }
            }
            core.settings_audit_barrier = Some((entered, resume));
            core.settings_call(&id, 1, "web".into(), serde_json::json!({"web_search":"off"}));
        });
        tokio::time::timeout(Duration::from_secs(3), paused).await.unwrap().unwrap();
        app.with(|core| {
            if !trust {
                let conv = core.convs.get_mut(&id).unwrap();
                conv.allow_always = None;
                conv.allow_always_epochs.clear();
                core.save(&id);
            }
            // The grant case enables a different authority only after automatic admission.
            core.save_settings(Settings { faber_trust: Some(!trust), ..Settings::default() }).unwrap();
        });
        release.send(()).unwrap();
        let (call, ok, text) = tokio::time::timeout(Duration::from_secs(3), replies.recv()).await.unwrap().unwrap();
        assert_eq!(call, "web");
        assert!(!ok && text.contains("nothing was changed"), "revoked authority must fail explicitly: {text}");
        assert_eq!(app.settings().tuning().web, WebSearch::Live, "revocation prevents durable mutation");
        app.with(|core| {
            assert!(core.settings_changes.is_empty());
            let conv = &core.convs[&id];
            assert!(!conv.allow_always.iter().flatten().any(|grant| grant == api::SELF_WEB_SEARCH));
            assert!(!conv.messages.iter().flat_map(|m| m.tools.iter().flatten()).any(|tool| tool.id == "web" && tool.ok == Some(true)));
            assert!(core.live.contains_key(&id), "refusal does not retire the running turn");
        });
        assert!(replies.try_recv().is_err(), "one explicit failure settles the request");
        app.shutdown();
    }

    #[tokio::test]
    async fn settings_search_revoked_during_audit_trust() {
        revoked_during_audit(true).await;
    }

    #[tokio::test]
    async fn settings_search_revoked_during_audit_grant() {
        revoked_during_audit(false).await;
    }

    #[test]
    fn settings_search_rechecks_the_original_authorization_after_its_audit() {
        assert!(authorized("trust", false, true, false));
        assert!(!authorized("trust", false, false, true), "a catalog or remembered grant cannot rescue revoked trust");
        assert!(authorized("always", false, false, true));
        assert!(!authorized("always", false, true, false), "full trust cannot rescue a revoked remembered grant");
        assert!(authorized("always", true, false, false), "the person's explicit answer may create its own grant");
        assert!(authorized("once", false, false, false));
        assert!(!authorized("unknown", true, true, true));
    }
    #[tokio::test]
    async fn settings_search_lost_reply_receiver_keeps_durable_saved_outcome_and_strict_retirement() {
        use super::super::answer::tests::{engine, seed};
        let dir = crate::workspace::TestRoot::new();
        let (app, _) = engine(&dir);
        let (send, receive) = std::sync::mpsc::channel();
        drop(receive);
        let session = Session::from_fn(move |command| {
            let _ = send.send(command);
        });
        let id = seed(&app, session, &[("turn", &[])]);
        app.with(|core| {
            core.convs.get_mut(&id).unwrap().area = Area::Project;
            core.save_settings(Settings { faber_trust: Some(true), ..Settings::default() }).unwrap();
            core.handle(
                &id,
                1,
                EngineEvent::Tool {
                    id: "web".into(),
                    name: "Settings".into(),
                    input: serde_json::json!({"web_search":"off"}),
                    parent: None,
                },
            );
            core.settings_call(&id, 1, "web".into(), serde_json::json!({"web_search":"off"}));
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if app.settings().tuning().web == WebSearch::Off {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        app.with(|core| {
            assert!(!core.live.contains_key(&id), "failed reply queue never exempts stricter retirement");
            assert!(core.settings_changes.is_empty());
            let message = core.convs[&id].messages.last().unwrap();
            assert_eq!(message.status, Some(Status::Stopped));
            assert!(message.tools.iter().flatten().any(|t| t.id == "web" && t.ok == Some(true)));
        });
        assert_eq!(app.settings().tuning().web, WebSearch::Off);
        app.shutdown();
    }
}
