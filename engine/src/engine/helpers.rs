//! Helpers (R161): when a project's agent calls Faber's helper tool, the other vendor runs that piece of work in a session of
//! its own, in the same folder and with the same permission mode. What the helper runs shows inside the agent's call, its
//! final words go back as the call's result, and its cost joins the reply's.
use super::*;
use crate::helper::{self, Helper};

/// The longest a helper may work; the agent and its turn wait for it all that time.
const MAX_RUN: Duration = Duration::from_secs(30 * 60);
/// How long a stopped helper has to report before it is ended.
const STOP_GRACE: Duration = Duration::from_secs(10);

pub(super) struct Run {
    /// The conversation and the agent's process (its `sid`) that called the helper; the report goes back only to that process.
    conv: String,
    sid: u64,
    /// Tells this helper's events apart from a helper that replaced it under the same call id.
    hsid: u64,
    vendor: Vendor,
    model: String,
    session: Session,
    started: f64,
    /// The helper's latest block of text: its final words are the report.
    report: String,
    new_block: bool,
    /// Tools the helper was not allowed to use without the person's approval.
    refused: Vec<String>,
}

impl Core {
    /// Whether a project conversation on `vendor` offers the helper tool: the other vendor's CLI is connected.
    pub(super) fn offers_helper(&self, area: Area, vendor: Vendor) -> Helper {
        if area == Area::Project && self.opts.clis.contains_key(&helper::other(vendor)) { Helper::Offers } else { Helper::Off }
    }

    /// The agent called the helper tool. A helper that cannot start answers the call at once with why, so the agent goes on alone.
    pub(super) fn start_helper(&mut self, conv: &str, call: String, input: serde_json::Value) {
        if let Err(why) = self.launch_helper(conv, &call, &input) {
            let (_, text) = helper::report(false, "", &[], Some(why));
            if let Some(l) = self.live.get(conv) {
                l.session.helper_reply(&call, false, text);
            }
        }
    }

    fn launch_helper(&mut self, conv: &str, call: &str, input: &serde_json::Value) -> Result<(), &'static str> {
        let l = self.live.get(conv).ok_or("the conversation is gone")?;
        let (sid, vendor, permissions, tuning, cwd) = (l.sid, helper::other(l.vendor), l.permissions, l.tuning, l.cwd.clone());
        if self.convs.get(conv).map(|c| c.area) != Some(Area::Project) {
            return Err("helpers work only in projects");
        }
        if self.helpers.values().filter(|h| h.conv == conv).count() >= helper::MAX_PER_CONVERSATION {
            return Err("two helpers are already working; wait for one to finish");
        }
        let prompt = helper::prompt(input).ok_or("the brief was empty")?;
        let cli = self.opts.clis.get(&vendor).cloned().ok_or("the other AI is not connected")?;
        // Before Codex has listed the account's models, its recommended one stands in.
        let (model, effort) = match helper::profile(&crate::route::candidates(&self.vendors(), &self.codex_models), vendor) {
            Some(p) => (Some(p.model.to_string()), p.effort.to_string()),
            None if vendor == Vendor::Codex && self.codex_recommended.is_some() => (self.codex_recommended.clone(), "medium".to_string()),
            None => return Err("the other AI has no model for this"),
        };
        let family = model.as_deref().and_then(family_of);
        self.allows(vendor, family, Purpose::UserTurn { by_hand: false }, false).map_err(|why| match why {
            Why::Billing => "the other AI may be billed per use, so Faber does not call it on its own",
            _ => "the other AI has no quota left right now",
        })?;
        self.begin_unbounded(vendor).map_err(|_| "the other AI cannot start right now")?;
        let user_hooks = self.store.settings().user_hooks == Some(true) && permissions.guarded();
        self.next_sid += 1;
        let hsid = self.next_sid;
        let (me, key) = (self.me.clone(), call.to_string());
        let sink: EventSink = Arc::new(move |event| {
            if let Some(core) = me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).helper_event(&key, hsid, event);
            }
        });
        let shown = model.clone().unwrap_or_else(|| vendor.name().to_string());
        let effort = Some(effort);
        let session = match vendor {
            Vendor::Claude => {
                let flags = self.claude_flags_for(&cli);
                start_claude(
                    ClaudeOptions {
                        bin: cli.command.clone(),
                        cwd,
                        launch: ClaudeLaunch {
                            area: Area::Project,
                            model,
                            effort,
                            resume: None,
                            user_hooks,
                            permissions,
                            instructions: None,
                            user_name: None,
                            flags,
                            tuning,
                            helper: Helper::Is,
                            browse: false,
                        },
                        spawn: spawner(&self.opts.spawn, &cli),
                        sandbox: self.opts.clis.get(&Vendor::Codex).cloned(),
                    },
                    sink,
                )
            }
            Vendor::Codex => self.codex_server(Area::Project, user_hooks, permissions, &cli).open(
                CodexThread {
                    cwd,
                    model,
                    effort,
                    resume: None,
                    instructions: None,
                    user_name: None,
                    permissions,
                    tuning,
                    helper: Helper::Is,
                    browse: false,
                },
                sink,
            ),
        };
        session.send(UserTurn::text(prompt));
        let started = self.secs();
        let run = Run {
            conv: conv.to_string(),
            sid,
            hsid,
            vendor,
            model: shown,
            session,
            started,
            report: String::new(),
            new_block: false,
            refused: vec![],
        };
        if let Some(old) = self.helpers.insert(call.to_string(), run) {
            old.session.close();
        }
        self.expire_helper(call, hsid, MAX_RUN, "it ran out of time");
        Ok(())
    }

    /// What a helper's own session reports. Its tools show inside the agent's call; nobody answers its approvals, because the
    /// person sees only the agent: the agent hears what was refused and can do it itself, where the person approves it.
    fn helper_event(&mut self, call: &str, hsid: u64, event: EngineEvent) {
        let Some(h) = self.helpers.get_mut(call).filter(|h| h.hsid == hsid) else { return };
        let (conv, sid, vendor) = (h.conv.clone(), h.sid, h.vendor);
        match event {
            EngineEvent::Tool { id, name, input, .. } => {
                self.handle(&conv, sid, EngineEvent::Tool { id, name, input, parent: Some(call.to_string()) })
            }
            EngineEvent::ToolResult { .. } => self.handle(&conv, sid, event),
            EngineEvent::Break { .. } => h.new_block = true,
            EngineEvent::Text(delta) => {
                if std::mem::take(&mut h.new_block) && !delta.is_empty() {
                    h.report.clear();
                }
                h.report.push_str(&delta);
            }
            EngineEvent::Permission { request_id, tool, .. } => {
                drop(h.session.answer(&request_id, false));
                if !h.refused.contains(&tool) {
                    h.refused.push(tool);
                }
            }
            EngineEvent::Quota { window, used, resets_at } => self.note_quota(vendor, &window, used, resets_at),
            EngineEvent::Signals(s) => self.apply_signals(vendor, s),
            EngineEvent::Done {
                ok,
                cost_total_usd,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                cache_write_1h_tokens,
                incomplete,
                reason,
            } => {
                let model = h.model.clone();
                // A helper's session is new, so Claude's running total is this turn's cost; Codex prices each turn.
                let cost = cost_total_usd.filter(|c| c.is_finite() && *c >= 0.0).or_else(|| {
                    usage::api_price_for_prompt(
                        &model,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        cache_write_1h_tokens,
                        0,
                    )
                });
                self.end_helper(call, ok, (!ok).then_some(reason.as_str()), cost, incomplete || cost.is_none());
            }
            // A turn Codex refused to start never ends on its own: it is the helper's end too.
            EngineEvent::Error { message, fatal } if fatal || message.starts_with("Codex turn/start failed") => {
                self.end_helper(call, false, Some(&message), None, true)
            }
            _ => {}
        }
    }

    fn end_helper(&mut self, call: &str, ok: bool, error: Option<&str>, cost: Option<f64>, incomplete: bool) {
        let Some(h) = self.helpers.remove(call) else { return };
        h.session.close();
        self.note_span(h.vendor, h.started, &h.model, cost, incomplete);
        let (ok, text) = helper::report(ok, &h.report, &h.refused, error);
        let Some(l) = self.live.get(&h.conv).filter(|l| l.sid == h.sid) else { return };
        l.session.helper_reply(call, ok, text);
        if let (Some(cost), Some(m)) = (cost, self.turn(&h.conv)) {
            m.cost_usd = Some(m.cost_usd.unwrap_or(0.0) + cost);
        }
    }

    /// Ends the helper `hsid` after `after` unless it has ended by itself, so the agent waiting on it always hears back.
    fn expire_helper(&self, call: &str, hsid: u64, after: Duration, why: &'static str) {
        let (me, call) = (self.me.clone(), call.to_string());
        self.opts.runtime.spawn(async move {
            tokio::time::sleep(after).await;
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            if core.helpers.get(&call).is_some_and(|h| h.hsid == hsid) {
                core.end_helper(&call, false, Some(why), None, true);
            }
        });
    }

    /// Whether a helper of this vendor is at work: its quota is being spent now.
    pub(super) fn helper_running(&self, vendor: Vendor) -> bool {
        self.helpers.values().any(|h| h.vendor == vendor)
    }

    /// A helper is never picked by hand nor approved for paid use, so it stops as soon as its vendor may bill beyond the
    /// subscription (R86), and the agent hears why.
    pub(super) fn stop_paid_helpers(&mut self, vendor: Vendor) {
        let calls: Vec<String> = self.helpers.iter().filter(|(_, h)| h.vendor == vendor).map(|(call, _)| call.clone()).collect();
        for call in calls {
            self.end_helper(&call, false, Some("the other AI would be billed beyond its subscription"), None, true);
        }
    }

    /// The engine is closing: every helper stops, and what it spent so far is noted as unknown.
    pub(super) fn close_helpers(&mut self) {
        let convs: HashSet<String> = self.helpers.values().map(|h| h.conv.clone()).collect();
        for conv in convs {
            self.stop_helpers(&conv, true);
        }
    }

    /// The agent's turn stopped or ended: its helpers stop with it. An interrupted helper still reports what it had.
    pub(super) fn stop_helpers(&mut self, conv: &str, close: bool) {
        let calls: Vec<String> = self.helpers.iter().filter(|(_, h)| h.conv == conv).map(|(call, _)| call.clone()).collect();
        for call in calls {
            if close {
                if let Some(h) = self.helpers.remove(&call) {
                    h.session.close();
                    self.note_span(h.vendor, h.started, &h.model, None, true);
                }
            } else if let Some(h) = self.helpers.get(&call) {
                h.session.interrupt();
                // A helper whose turn had not begun cannot be interrupted: it ends here if it has not reported by then.
                let hsid = h.hsid;
                self.expire_helper(&call, hsid, STOP_GRACE, "it was stopped");
            }
        }
    }
}
