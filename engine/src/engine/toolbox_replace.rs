//! Explicit, independently consented updates and repairs of held catalog tools.
use super::*;
use crate::toolbox as api;
use crate::tools::ReplacementPlan;
use std::sync::atomic::{AtomicBool, Ordering};

struct ReplacementRun {
    work: super::toolbox::Running,
    conv: String,
    cancelled: Arc<AtomicBool>,
}

impl Drop for ReplacementRun {
    fn drop(&mut self) {
        if let Some(core) = self.work.0.upgrade() {
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            if core.replacement_runs.get(&self.conv).is_some_and(|flag| Arc::ptr_eq(flag, &self.cancelled)) {
                core.replacement_runs.remove(&self.conv);
            }
        }
    }
}

pub(super) struct PendingReplacement {
    call: String,
    sid: u64,
    turn: String,
    repair: bool,
    plan: ReplacementPlan,
    input: serde_json::Value,
}

impl PendingReplacement {
    fn tool(&self) -> &'static str {
        if self.repair { api::SELF_REPAIR } else { api::SELF_UPDATE }
    }
}

async fn record(dir: PathBuf, conv: String, plan: ReplacementPlan, repair: bool, how: &'static str, result: &'static str) -> bool {
    tokio::task::spawn_blocking(move || {
        api::log_replacement(
            &dir,
            &api::SelfChange {
                at: now(),
                id: &plan.item().id,
                from: Some(plan.old_commit()),
                to: &plan.item().commit,
                how,
                result,
                conv: &conv,
            },
            repair,
        )
    })
    .await
    .unwrap_or(false)
}

impl Core {
    pub(super) fn revoke_replacement(&mut self, conv: &str) {
        self.replacements.remove(conv);
        if let Some(cancelled) = self.replacement_runs.remove(conv) {
            cancelled.store(true, Ordering::Release);
        }
    }

    pub(super) fn replacement_call(&mut self, conv: &str, sid: u64, call: String, wanted: &serde_json::Value, repair: bool) {
        let item = match self.install_target(conv, sid, wanted) {
            Ok(item) => item,
            Err(text) => return self.answer_toolbox(conv, sid, &call, false, text.into()),
        };
        let (store, missing) = (self.catalog_store(), self.node_missing);
        let (me, conv, turn) = (self.me.clone(), conv.to_string(), self.live.get(conv).and_then(|l| l.turn.clone()).unwrap_or_default());
        let checked = self.opts.runtime.spawn_blocking(move || {
            if store.requires_node(item) && (missing || store.lacks_node(item)) {
                return Err(api::NO_NODE.to_string());
            }
            let plan = store.capture_replacement(&item.id)?;
            if !repair && plan.old_commit() == plan.item().commit {
                return Err("This tool already has the current catalog pin. Use repair only if the person requested a repair.".into());
            }
            Ok(plan)
        });
        self.opts.runtime.spawn(async move {
            let result = checked.await.unwrap_or_else(|_| Err(api::FAILED.into()));
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            let plan = match result {
                Ok(plan) => plan,
                Err(text) => return core.answer_toolbox(&conv, sid, &call, false, text),
            };
            if core.live.get(&conv).and_then(|l| l.turn.as_deref()) != Some(&turn)
                || core.install_target(&conv, sid, &serde_json::json!(item.id)).is_err()
            {
                return core.answer_toolbox(&conv, sid, &call, false, "The turn or its permission changed.".into());
            }
            let store = core.catalog_store();
            let mut input = api::install_card(item, store.plugin_definition(&item.id).is_some(), store.requires_node(item));
            input["from"] = plan.old_commit().into();
            input["repair"] = repair.into();
            let asked = PendingReplacement { call, sid, turn: turn.clone(), repair, plan, input };
            if core.store.settings().faber_trust() {
                return core.run_replacement(&conv, asked, "trust");
            }
            if core.grant_stands(&conv, asked.tool()) {
                return core.run_replacement(&conv, asked, "always");
            }
            let card = Permission {
                request_id: asked.call.clone(),
                tool: asked.tool().into(),
                input: asked.input.clone(),
                epoch: core.store.settings().grants_epoch,
            };
            if let Some(message) = core.turn(&conv) {
                message.permissions.get_or_insert_default().push(card);
            }
            core.replacements.insert(conv.clone(), asked);
            core.weigh(&conv);
            core.push(&conv, Some(&turn), true);
        });
    }

    pub(super) fn answer_replacement(
        &mut self,
        conv: &str,
        turn: &str,
        call: &str,
        allow: bool,
        always: bool,
    ) -> Option<Result<(), String>> {
        let card = self
            .convs
            .get(conv)?
            .messages
            .iter()
            .find(|m| m.id == turn)?
            .permissions
            .iter()
            .flatten()
            .find(|p| p.request_id == call && matches!(p.tool.as_str(), api::SELF_UPDATE | api::SELF_REPAIR))?
            .clone();
        let asked = self.replacements.remove(conv).filter(|asked| {
            asked.call == call
                && asked.turn == turn
                && asked.tool() == card.tool
                && asked.input == card.input
                && self.live.get(conv).is_some_and(|l| l.sid == asked.sid && l.turn.as_deref() == Some(turn))
        });
        if let Some(message) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == turn)) {
            message
                .permissions
                .get_or_insert_default()
                .retain(|p| !(p.request_id == call && matches!(p.tool.as_str(), api::SELF_UPDATE | api::SELF_REPAIR)));
        }
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(turn), true);
        let Some(asked) = asked else { return Some(Err(super::answer::STALE.into())) };
        if self.install_target(conv, asked.sid, &serde_json::json!(asked.plan.item().id)).is_err() {
            self.answer_toolbox(conv, asked.sid, call, false, "The turn or its permission changed.".into());
            return Some(Err(super::answer::STALE.into()));
        }
        if allow {
            let remember = always && card.epoch == self.store.settings().grants_epoch;
            if remember && let Some(c) = self.convs.get_mut(conv) {
                let tool = asked.tool();
                let grants = c.allow_always.get_or_insert_default();
                if !grants.iter().any(|entry| entry == tool) {
                    grants.push(tool.into());
                }
                match card.epoch {
                    Some(epoch) => {
                        c.allow_always_epochs.insert(tool.into(), epoch);
                    }
                    None => {
                        c.allow_always_epochs.remove(tool);
                    }
                }
                self.save(conv);
            }
            self.run_replacement(conv, asked, if remember { "always" } else { "once" });
        } else {
            let (me, dir, conv) = (self.me.clone(), self.opts.data_dir.clone(), conv.to_string());
            self.opts.runtime.spawn(async move {
                record(dir, conv.clone(), asked.plan, asked.repair, "once", "declined").await;
                if let Some(core) = me.upgrade() {
                    core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(
                        &conv,
                        asked.sid,
                        &asked.call,
                        false,
                        "The person said no. Do not retry; they can manage the tool in Catalog.".into(),
                    );
                }
            });
        }
        Some(Ok(()))
    }

    fn run_replacement(&mut self, conv: &str, asked: PendingReplacement, how: &'static str) {
        if self.installing {
            return self.answer_toolbox(conv, asked.sid, &asked.call, false, api::UPDATING.into());
        }
        if self.replacement_runs.contains_key(conv) {
            return self.answer_toolbox(conv, asked.sid, &asked.call, false, api::BUSY.into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.replacement_runs.insert(conv.to_string(), cancelled.clone());
        self.agent_installs += 1;
        let (store, source, dir, me, conv) =
            (self.catalog_store(), self.setup_source.clone(), self.opts.data_dir.clone(), self.me.clone(), conv.to_string());
        self.opts.runtime.spawn(async move {
            let running = ReplacementRun { work: super::toolbox::Running(me.clone()), conv: conv.clone(), cancelled: cancelled.clone() };
            let read = store.clone();
            let plan = asked.plan.clone();
            let checked = tokio::task::spawn_blocking(move || read.check_replacement(&plan)).await;
            let outcome = if !matches!(checked, Ok(Ok(()))) {
                Err("The stored copy changed. Request confirmation again; nothing was replaced.".to_string())
            } else if !record(dir.clone(), conv.clone(), asked.plan.clone(), asked.repair, how, "allowed").await {
                Err(api::NO_RECORD.into())
            } else {
                match crate::tools::download_replacement(&store, &asked.plan, source.as_deref()).await {
                    Err(error) => {
                        eprintln!("faber: catalog replacement failed: {error:?}");
                        Err("Could not download the pinned replacement. The current copy was kept.".into())
                    }
                    Ok(downloaded) => {
                        let live = me.upgrade().is_some_and(|core| {
                            let core = core.lock().unwrap_or_else(|e| e.into_inner());
                            core.live.get(&conv).is_some_and(|l| l.sid == asked.sid && l.turn.as_deref() == Some(&asked.turn))
                                && core.install_target(&conv, asked.sid, &serde_json::json!(asked.plan.item().id)).is_ok()
                        });
                        if !live {
                            Err("The turn or its permission changed. Nothing was replaced.".into())
                        } else {
                            let plan = asked.plan.clone();
                            // Recheck admission inside the store lock after staging, immediately before publication.
                            tokio::task::spawn_blocking(move || downloaded.replace(&store, &plan, || !cancelled.load(Ordering::Acquire))).await
                                .unwrap_or_else(|_| Err("Could not finish replacement. Check the tool in Catalog.".into()))
                        }
                    }
                }
            };
            let ok = outcome.is_ok();
            record(dir, conv.clone(), asked.plan.clone(), asked.repair, how, if ok { "replaced" } else { "failed" }).await;
            let text = outcome.map_or_else(|text| text, |()| format!("{} {} to the catalog pin. The prior files were retained. New conversations receive the change; this conversation keeps its tools.", if asked.repair { "Repaired" } else { "Updated" }, asked.plan.item().id));
            drop(running);
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, asked.sid, &asked.call, ok, text);
            }
        });
    }
}
