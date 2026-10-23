//! The agent's toolbox (`crate::toolbox`): `quota` answers from the readings the engine already holds, `catalog` from the shipped
//! catalog and the store's ledger. Searching starts no request, so both are offered to a project's own agent in any permission
//! mode. `catalog` with `install` changes Faber itself (guardrail 3): it waits for the person's card, then installs through the
//! one installer the Catalog screen uses (`tools::install_for_agent`, whose last check of what Faber holds runs under the store lock), and only a tool the store holds no copy of.
//! `memory_search` is answered in `memory_search.rs`. `offer_bigger` shows one question card per conversation and answers from the
//! person's choice (`answer.rs`).
use super::answer::STALE;
use super::*;
use crate::toolbox;
use crate::tools::{InstallError, Item};

/// Plan changes nothing by its nature, and Isolated (the router, the side questions, the helpers: no setting can hold it) has no
/// say over Faber at all.
fn may_change(mode: Permissions) -> bool {
    !matches!(mode, Permissions::Plan | Permissions::Isolated)
}

/// One decision or result line of the log, written away from the engine's lock.
async fn record(
    dir: PathBuf,
    conv: String,
    item: &'static Item,
    enabled: Option<bool>,
    remove: bool,
    how: &'static str,
    result: &'static str,
) -> bool {
    tokio::task::spawn_blocking(move || {
        // `from` is always empty: the agent installs only what the store holds nothing of, so no version is ever replaced.
        let change = toolbox::SelfChange { at: now(), id: &item.id, from: None, to: &item.commit, how, result, conv: &conv };
        if remove {
            return toolbox::log_remove(&dir, &change);
        }
        match enabled {
            Some(enabled) => toolbox::log_switch(&dir, &change, enabled),
            None => toolbox::log_self_change(&dir, &change),
        }
    })
    .await
    .unwrap_or(false)
}

/// Consent is bound to the original action, process and turn, never to facts supplied by a later card.
pub(super) struct PendingChange {
    call: String,
    sid: u64,
    turn: String,
    item: &'static Item,
    enabled: Option<bool>,
    remove: bool,
}

fn change_tool(enabled: Option<bool>, remove: bool) -> &'static str {
    if remove {
        toolbox::SELF_REMOVE
    } else if enabled.is_some() {
        toolbox::SELF_SWITCH
    } else {
        toolbox::SELF_INSTALL
    }
}

/// An allowed catalog change counts as work until the task ends, including a panic or Stop: an app update waits for it.
pub(super) struct Running(pub(super) std::sync::Weak<std::sync::Mutex<Core>>);

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(core) = self.0.upgrade() {
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            core.agent_installs = core.agent_installs.saturating_sub(1);
        }
    }
}

impl Core {
    pub(super) fn toolbox_call(&mut self, conv: &str, call: String, tool: &'static str, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        if tool == toolbox::QUOTA_TOOL {
            let text = toolbox::quota(&self.usage(), self.secs());
            return self.answer_toolbox(conv, sid, &call, true, text);
        }
        if tool == toolbox::OFFER_BIGGER_TOOL {
            return self.bigger_call(conv, sid, call, &input);
        }
        if tool == toolbox::MEMORY_SEARCH_TOOL {
            return self.memory_search_call(conv, sid, call, &input);
        }
        let install = input.get("install");
        let switch = input.get("set_enabled");
        let remove = input.get("remove");
        let replacement = input.get("repair").map(|id| (id, true)).or_else(|| input.get("update").map(|id| (id, false)));
        if (install.is_some() || switch.is_some() || remove.is_some() || replacement.is_some())
            && input.as_object().is_none_or(|args| args.len() != 1)
        {
            return self.answer_toolbox(
                conv,
                sid,
                &call,
                false,
                "Use exactly one catalog action: query, install, set_enabled, remove, update or repair.".into(),
            );
        }
        if let Some((wanted, repair)) = replacement {
            return self.replacement_call(conv, sid, call, wanted, repair);
        }
        if let Some(switch) = switch {
            let enabled = switch["enabled"].as_bool();
            if switch.as_object().is_none_or(|s| s.len() != 2) || enabled.is_none() || !switch["id"].is_string() {
                return self.answer_toolbox(
                    conv,
                    sid,
                    &call,
                    false,
                    "set_enabled requires only a catalog id and a boolean enabled state.".into(),
                );
            }
            return self.install_call(conv, sid, call, &switch["id"], enabled, false);
        }
        if let Some(install) = install {
            return self.install_call(conv, sid, call, install, None, false);
        }
        if let Some(remove) = remove {
            return self.install_call(conv, sid, call, remove, None, true);
        }
        // The ledger is a small file and a few checks of the store's folders: read away from the lock.
        let store = self.catalog_store();
        let (me, conv) = (self.me.clone(), conv.to_string());
        let read = self.opts.runtime.spawn_blocking(move || {
            let installed = store.installed();
            let entries: Vec<_> = crate::tools::catalog()
                .iter()
                .map(|item| toolbox::Entry {
                    item,
                    plugin: store.plugin_definition(&item.id).is_some(),
                    installed: installed.contains_key(&item.id),
                    on: installed.contains_key(&item.id) && store.enabled(&item.id),
                })
                .collect();
            toolbox::catalog(&entries, &input)
        });
        self.opts.runtime.spawn(async move {
            let (ok, text) = read.await.unwrap_or_else(|_| (false, "Faber could not read the catalog.".into()));
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, sid, &call, ok, text);
        });
    }

    /// The answer goes back only to the process that asked: a conversation that was stopped or moved on drops it.
    pub(super) fn answer_toolbox(&self, conv: &str, sid: u64, call: &str, ok: bool, text: String) {
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(call, ok, text);
        }
    }

    // ---- offer_bigger ----

    /// The agent offers the larger version of a small request. The card is an ordinary question of the person's, with the request as
    /// asked first, and it comes at most once per conversation: the code counts it (`bigger_offered`), the agent's own judgment
    /// cannot. A refusal that sets nothing leaves the offer for a later turn. Nothing is written to the CLI for the card: its answer
    /// goes back as this call's result (`prepare_answer`).
    fn bigger_call(&mut self, conv: &str, sid: u64, call: String, input: &serde_json::Value) {
        let refuse = |me: &Self, ok: bool, text: &str| me.answer_toolbox(conv, sid, &call, ok, text.into());
        let Some(c) = self.convs.get(conv).filter(|c| c.area == Area::Project) else {
            return refuse(self, false, toolbox::BIGGER_NOT_HERE);
        };
        if c.bigger_offered {
            return refuse(self, true, toolbox::BIGGER_ONCE);
        }
        let turn = self.live.get(conv).filter(|l| l.sid == sid).and_then(|l| l.turn.clone());
        let Some(reply) = c.messages.iter().find(|m| Some(&m.id) == turn.as_ref()).filter(|m| m.status == Some(Status::Streaming)) else {
            return refuse(self, false, "The turn has ended.");
        };
        // Nobody to ask while the person is away, and one question at a time.
        if c.keep_going.is_some() {
            return refuse(self, true, toolbox::BIGGER_AWAY);
        }
        if reply.permissions.iter().flatten().next().is_some() {
            return refuse(self, true, toolbox::BIGGER_WAITING);
        }
        let card = match toolbox::bigger_card(input) {
            Ok(card) => card,
            Err(text) => return refuse(self, false, text),
        };
        let (turn, epoch) = (reply.id.clone(), self.store.settings().grants_epoch);
        if let Some(c) = self.convs.get_mut(conv) {
            c.bigger_offered = true;
            if let Some(m) = c.messages.iter_mut().find(|m| m.id == turn) {
                let ask = Permission { request_id: call.clone(), tool: "AskUserQuestion".into(), input: card, epoch };
                m.permissions.get_or_insert_default().push(ask);
            }
        }
        self.bigger.insert(conv.to_string(), (call.clone(), sid));
        self.save(conv);
        self.weigh(conv);
        self.watch_question(conv, &turn, &call);
        self.push(conv, Some(&turn), true);
    }

    // ---- install ----

    /// Whether this conversation's agent may install now, and which tool of the catalog it named. The area and the mode are read
    /// from the process in use: Codex keeps the tool definitions of a thread, so being offered the tool proves nothing.
    pub(super) fn install_target(&self, conv: &str, sid: u64, wanted: &serde_json::Value) -> Result<&'static Item, &'static str> {
        let c = self.convs.get(conv).filter(|c| c.area == Area::Project).ok_or(toolbox::NOT_HERE)?;
        if c.stopped {
            return Err("The turn was stopped.");
        }
        let l = self.live.get(conv).filter(|l| l.sid == sid).ok_or(toolbox::NOT_HERE)?;
        if !may_change(l.permissions) {
            return Err(toolbox::PLAN);
        }
        if !c.messages.iter().any(|m| Some(&m.id) == l.turn.as_ref() && m.status == Some(Status::Streaming)) {
            return Err("The turn has ended.");
        }
        let waiting = c
            .messages
            .iter()
            .find(|m| Some(&m.id) == l.turn.as_ref())
            .is_some_and(|m| m.permissions.iter().flatten().any(|p| toolbox::is_self_change(&p.tool)));
        if waiting {
            return Err(toolbox::WAITING);
        }
        wanted.as_str().and_then(|id| self.catalog_item(id)).ok_or(toolbox::UNKNOWN)
    }

    /// The catalog item with this id: the shipped one, or the one a test gave.
    fn catalog_item(&self, id: &str) -> Option<&'static Item> {
        self.test_items.iter().copied().find(|item| item.id == id).or_else(|| crate::tools::find(id))
    }

    /// The agent asked to install a tool of the catalog by its id. What needs no answer from the person is answered now (an id
    /// that is not in the catalog, Plan mode, a tool already installed, no Node); the rest waits for them, away from the store's
    /// lock, as the other long calls do.
    fn install_call(&mut self, conv: &str, sid: u64, call: String, wanted: &serde_json::Value, enabled: Option<bool>, remove: bool) {
        let item = match self.install_target(conv, sid, wanted) {
            Ok(item) => item,
            Err(text) => return self.answer_toolbox(conv, sid, &call, false, text.into()),
        };
        let turn = self.live.get(conv).and_then(|l| l.turn.clone()).unwrap_or_default();
        let store = self.catalog_store();
        let node_missing = self.node_missing;
        let (me, conv) = (self.me.clone(), conv.to_string());
        let checked = self.opts.runtime.spawn_blocking(move || {
            if remove {
                return store.check_agent_remove(&item.id).map_err(|text| (false, text.to_string()));
            }
            if enabled.is_some() {
                return store.check_agent_switch(&item.id).map_err(|text| (false, text.to_string()));
            }
            if let Some((ok, text)) = store.held(item) {
                return Err((ok, text.to_string()));
            }
            if store.requires_node(item) && (node_missing || store.lacks_node(item)) {
                return Err((false, toolbox::NO_NODE.to_string()));
            }
            Ok(())
        });
        self.opts.runtime.spawn(async move {
            let checked = checked.await.unwrap_or_else(|_| Err((false, toolbox::FAILED.to_string())));
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            match checked {
                Ok(()) if core.live.get(&conv).and_then(|l| l.turn.as_ref()) == Some(&turn) => {
                    core.install_decide(&conv, sid, call, item, enabled, remove)
                }
                Ok(()) => core.answer_toolbox(&conv, sid, &call, false, "The turn has ended.".into()),
                Err((ok, text)) => core.answer_toolbox(&conv, sid, &call, ok, text),
            }
        });
    }

    /// Under the lock again: the person's standing trust or an "always" they gave installs at once; otherwise the card goes on the
    /// turn and the call waits for the answer (`answer_self_change`). Nothing is written to the CLI for the card.
    fn install_decide(&mut self, conv: &str, sid: u64, call: String, item: &'static Item, enabled: Option<bool>, remove: bool) {
        // The turn may have ended, the mode changed or a second call come in while the checks ran.
        if let Err(text) = self.install_target(conv, sid, &serde_json::json!(item.id)) {
            return self.answer_toolbox(conv, sid, &call, false, text.into());
        }
        if self.turn(conv).is_none_or(|m| m.status != Some(Status::Streaming)) {
            return self.answer_toolbox(conv, sid, &call, false, "The turn has ended.".into());
        }
        if self.store.settings().faber_trust() {
            return self.run_install(conv, sid, call, item, (enabled, remove), "trust");
        }
        if self.grant_stands(conv, change_tool(enabled, remove)) {
            return self.run_install(conv, sid, call, item, (enabled, remove), "always");
        }
        let store = self.catalog_store();
        let (plugin, runs_code) = (store.plugin_definition(&item.id).is_some(), store.requires_node(item));
        let epoch = self.store.settings().grants_epoch;
        let turn_id = self.live.get(conv).and_then(|l| l.turn.clone());
        let mut input = toolbox::install_card(item, plugin, runs_code);
        if let Some(enabled) = enabled {
            input["enabled"] = enabled.into();
        }
        if remove {
            input["remove"] = true.into();
        }
        let card = Permission { request_id: call.clone(), tool: change_tool(enabled, remove).into(), input, epoch };
        if let Some(m) = self.turn(conv) {
            m.permissions.get_or_insert_default().push(card);
        }
        self.installs
            .insert(conv.to_string(), PendingChange { call, sid, turn: turn_id.clone().unwrap_or_default(), item, enabled, remove });
        self.weigh(conv);
        self.push(conv, turn_id.as_deref(), true);
    }

    /// The person answered an install card. None when the request is not one (the ordinary path answers it). "Always" is kept
    /// like the other cards' (`allow_always` with the grants epoch), so a stricter mode revokes it and Plan never holds it.
    pub(super) fn answer_self_change(
        &mut self,
        conv: &str,
        turn_id: &str,
        request_id: &str,
        allow: bool,
        always: bool,
    ) -> Option<Result<(), String>> {
        if let Some(answered) = self.answer_replacement(conv, turn_id, request_id, allow, always) {
            return Some(answered);
        }
        let reply = self.convs.get(conv)?.messages.iter().find(|m| m.id == turn_id)?;
        let card = reply
            .permissions
            .iter()
            .flatten()
            .find(|p| {
                p.request_id == request_id
                    && (p.tool == toolbox::SELF_INSTALL || p.tool == toolbox::SELF_SWITCH || p.tool == toolbox::SELF_REMOVE)
            })?
            .clone();
        Some(self.settle_install(conv, turn_id, request_id, &card, allow, always))
    }

    fn settle_install(
        &mut self,
        conv: &str,
        turn_id: &str,
        request_id: &str,
        card: &Permission,
        allow: bool,
        always: bool,
    ) -> Result<(), String> {
        let live = self.live.get(conv).filter(|l| l.turn.as_deref() == Some(turn_id));
        let (sid, plan) = live.map_or((0, true), |l| (l.sid, l.permissions == Permissions::Plan));
        let asked = self.installs.remove(conv).filter(|asked| {
            asked.call == request_id
                && asked.sid == sid
                && asked.turn == turn_id
                && asked.item.id == card.input["id"].as_str().unwrap_or_default()
                && asked.enabled == card.input.get("enabled").and_then(|v| v.as_bool())
                && asked.remove == card.input.get("remove").and_then(|v| v.as_bool()).unwrap_or(false)
                && card.tool == change_tool(asked.enabled, asked.remove)
        });
        // Either way the card is spent: an answer to one that belonged to another process or turn does nothing.
        if let Some(waiting) =
            self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == turn_id)).and_then(|m| m.permissions.as_mut())
        {
            waiting.retain(|p| {
                !(p.request_id == request_id
                    && (p.tool == toolbox::SELF_INSTALL || p.tool == toolbox::SELF_SWITCH || p.tool == toolbox::SELF_REMOVE))
            });
        }
        let Some(asked) = asked else {
            self.save(conv);
            self.push(conv, Some(turn_id), true);
            return Err(STALE.into());
        };
        let (item, enabled, remove) = (asked.item, asked.enabled, asked.remove);
        if let Err(text) = self.install_target(conv, sid, &serde_json::json!(item.id)) {
            self.save(conv);
            self.push(conv, Some(turn_id), true);
            self.answer_toolbox(conv, sid, request_id, false, text.into());
            return Err(STALE.into());
        }
        let tool = change_tool(enabled, remove);
        // Not in Plan, and not for a card made under a grants epoch that is gone: that answer lets only this install through.
        let remember = allow && always && !plan && card.epoch == self.store.settings().grants_epoch;
        if remember && let Some(c) = self.convs.get_mut(conv) {
            let list = c.allow_always.get_or_insert_default();
            if !list.iter().any(|t| t == tool) {
                list.push(tool.to_string());
            }
            match &card.epoch {
                Some(epoch) => c.allow_always_epochs.insert(tool.to_string(), epoch.clone()),
                None => c.allow_always_epochs.remove(tool),
            };
        }
        self.save(conv);
        self.weigh(conv);
        self.push(conv, Some(turn_id), true);
        if allow {
            self.run_install(conv, sid, request_id.to_string(), item, (enabled, remove), if remember { "always" } else { "once" });
        } else {
            let (me, dir, conv, call) = (self.me.clone(), self.opts.data_dir.clone(), conv.to_string(), request_id.to_string());
            self.opts.runtime.spawn(async move {
                record(dir, conv.clone(), item, enabled, remove, "once", "declined").await;
                let Some(core) = me.upgrade() else { return };
                let text = if remove || enabled.is_some() {
                    "The person said no. Do not retry; they can change it in Catalog."
                } else {
                    toolbox::NOT_ALLOWED
                };
                core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, sid, &call, false, text.into());
            });
        }
        Ok(())
    }

    /// Writes the decision, installs through the one installer and writes the result, all away from the lock, then tells the agent
    /// how it ended. The decision line comes first: an install the person can read about is never one that happened unrecorded, and
    /// whatever the way it was allowed (once, always or trust), a decision that cannot be written is not acted on.
    fn run_install(&mut self, conv: &str, sid: u64, call: String, item: &'static Item, change: (Option<bool>, bool), how: &'static str) {
        let (enabled, remove) = change;
        if let Err(text) = self.install_target(conv, sid, &serde_json::json!(item.id)) {
            return self.answer_toolbox(conv, sid, &call, false, text.into());
        }
        if self.installing {
            return self.answer_toolbox(
                conv,
                sid,
                &call,
                false,
                if remove {
                    "Faber is about to restart for an update, so nothing was removed. Ask again afterwards.".into()
                } else {
                    toolbox::UPDATING.into()
                },
            );
        }
        self.agent_installs += 1;
        let (store, source, dir) = (self.catalog_store(), self.setup_source.clone(), self.opts.data_dir.clone());
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts.runtime.spawn(async move {
            let running = Running(me.clone());
            // The store may have changed while the card waited: the same check as before it.
            let read = store.clone();
            let refused = tokio::task::spawn_blocking(move || {
                if remove { return read.check_agent_remove(&item.id).err().map(|text| (false, text)); }
                if enabled.is_some() { read.check_agent_switch(&item.id).err().map(|text| (false, text)) } else { read.held(item) }
            })
            .await
            .unwrap_or(Some((false, toolbox::FAILED)));
            let (ok, text) = match (refused, enabled, remove) {
                (Some((ok, text)), _, _) => (ok, text.to_string()),
                (None, _, _) if !record(dir.clone(), conv.clone(), item, enabled, remove, how, "allowed").await => {
                    (false, if remove { "Nothing was changed: Faber could not write its record of the removal.".to_string() } else { toolbox::NO_RECORD.to_string() })
                }
                (None, _, true) => {
                    let changed = tokio::task::spawn_blocking(move || store.remove_for_agent(&item.id)).await;
                    let (ok, text, outcome) = match changed {
                        Ok(Ok(())) => (true, format!("Removed {}. Undo is available in Catalog until Faber restarts. New conversations receive the change; this conversation keeps its existing tools.", item.id), "removed"),
                        _ => (false, "Could not remove the tool. Check its state in Catalog.".into(), "failed"),
                    };
                    record(dir, conv.clone(), item, None, true, how, outcome).await;
                    (ok, text)
                }
                (None, Some(enabled), false) => {
                    let changed = tokio::task::spawn_blocking(move || store.set_enabled_for_agent(&item.id, enabled)).await;
                    let (ok, text, outcome) = match changed {
                        Ok(Ok(())) => (
                            true,
                            format!(
                                "{} {}. The change applies to new conversations; this conversation keeps its existing tools.",
                                if enabled { "Enabled" } else { "Disabled" },
                                item.id
                            ),
                            if enabled { "enabled" } else { "disabled" },
                        ),
                        _ => (false, "Could not change the catalog switch. Check its state in Catalog.".into(), "failed"),
                    };
                    record(dir, conv.clone(), item, Some(enabled), false, how, outcome).await;
                    (ok, text)
                }
                (None, None, false) => {
                    let (ok, text, outcome) = match crate::tools::install_for_agent(&store, item, source.as_deref()).await {
                        Ok(()) => (true, toolbox::installed(item, store.plugin_definition(&item.id).is_some()), "installed"),
                        Err(InstallError::Node) => (false, toolbox::NO_NODE.to_string(), "failed"),
                        Err(InstallError::Held(text)) => (text == toolbox::ALREADY, text.to_string(), "refused"),
                        Err(InstallError::Failed(detail)) => {
                            eprintln!("faber: the agent's install of {} failed: {detail}", item.id);
                            (false, toolbox::FAILED.to_string(), "failed")
                        }
                    };
                    record(dir, conv.clone(), item, enabled, remove, how, outcome).await;
                    (ok, text)
                }
            };
            drop(running);
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, sid, &call, ok, text);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Store;

    #[test]
    fn plan_and_isolated_change_nothing_and_every_other_mode_may_ask() {
        assert!(!may_change(Permissions::Plan) && !may_change(Permissions::Isolated));
        assert!([Permissions::Ask, Permissions::FullAccess].into_iter().all(may_change));
    }

    #[test]
    fn a_copy_the_catalog_marks_corrupt_is_to_be_reinstalled_by_the_person_and_never_replaced_by_the_agent() {
        use std::io::Write;
        const SKILL: &[u8] = b"---\nname: careful\ndescription: Be careful.\n---\nBe careful.";
        let files = tempfile::tempdir().unwrap();
        std::fs::write(files.path().join("SKILL.md"), SKILL).unwrap();
        let mut item = crate::tools::find("ponytail").unwrap().clone();
        (item.id, item.path, item.digest) = ("careful".into(), "skills/careful".into(), crate::tools::digest(files.path()).unwrap());
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("repo/skills/careful/SKILL.md", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(SKILL).unwrap();
        let archive = files.path().join("careful.zip");
        std::fs::write(&archive, zip.finish().unwrap().into_inner()).unwrap();

        let data = tempfile::tempdir().unwrap();
        let store = Store::with_catalog(data.path(), vec![item.clone()]);
        assert_eq!(store.held(&item), None, "nothing is held: the agent may ask");
        store.install_from(&item, &archive).unwrap();
        assert_eq!(store.held(&item), Some((true, toolbox::ALREADY)));
        store.mark_failed_items(&["careful".to_string()]);
        assert_eq!(store.held(&item), Some((false, toolbox::REINSTALL)));
    }
}
