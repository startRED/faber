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
    pub(super) hsid: u64,
    vendor: Vendor,
    model: String,
    session: Session,
    started: f64,
    /// The helper's latest block of text: its final words are the report.
    report: String,
    new_block: bool,
    /// Tools the helper was not allowed to use without the person's approval.
    refused: Vec<String>,
    /// Messages the person wrote to it that it has not taken in yet (their ids), and the words of those it took (R161 step 5).
    waiting: Vec<String>,
    told: Vec<String>,
    /// A turn that ended while a message was still on its way: the end, kept until the message is taken in (another turn
    /// follows, and its end is the helper's) or refused (this end was the last).
    held: Option<Ended>,
    /// A turn of this helper ended before the one now running: Claude's total then covers both, a price from tokens one.
    earlier_turn: bool,
    /// The agent stopped: the turn that ends now is the last, whatever is still on its way.
    stopping: bool,
}

struct Ended {
    ok: bool,
    error: Option<String>,
    cost: Option<f64>,
    incomplete: bool,
}

impl Core {
    pub(super) fn helpers_in(&self, conv: &str) -> bool {
        self.helpers.values().any(|h| h.conv == conv)
    }
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
        if self.rewind_blocked(conv) {
            return Err("error.rewind.busy");
        }
        let l = self.live.get(conv).ok_or("the conversation is gone")?;
        let (sid, vendor, permissions, tuning, cwd) = (
            l.sid,
            helper::other(l.vendor),
            l.permissions,
            Tuning { web: l.tuning.web, summary: l.tuning.summary, ..Default::default() },
            l.cwd.clone(),
        );
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
        self.rewind_helper_started(call, hsid, conv, cwd.clone());
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
                            conversations: false,
                            progress: false,
                            verify: false,
                            tools: Default::default(),
                        },
                        spawn: spawner(&self.opts.spawn, &cli),
                        sandbox: self.opts.clis.get(&Vendor::Codex).cloned(),
                    },
                    sink,
                )
            }
            Vendor::Codex => self.codex_server(Area::Project, user_hooks, permissions, &cli).open(
                CodexThread {
                    leave_plan: false,
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
                    conversations: false,
                    progress: false,
                    verify: false,
                },
                sink,
            ),
        };
        self.rewind_helper_session(call, hsid, session.clone());
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
            waiting: vec![],
            told: vec![],
            held: None,
            earlier_turn: false,
            stopping: false,
        };
        if let Some(old) = self.helpers.insert(call.to_string(), run) {
            self.rewind_helper_retire(call, old.hsid);
            old.session.close();
        }
        self.expire_helper(call, hsid, MAX_RUN, "it ran out of time");
        Ok(())
    }

    /// The person wrote to a helper at work (R161 step 5). Codex takes the message into its running turn; Claude Code reads it
    /// at its next step, or as a turn of its own right after the one that is ending. The message shows on the helper's call.
    pub(super) fn tell_helper(&mut self, conv: &str, call: &str, text: &str) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("error.helperEmpty".into());
        }
        self.rewind.bump(conv);
        let h = self.helpers.get_mut(call).filter(|h| h.conv == conv).ok_or("error.helperDone")?;
        let id = new_id();
        h.waiting.push(id.clone());
        h.session.steer(UserTurn::text(helper::from_person(text)), &id);
        self.mark_told(conv, call, |told| told.push(Told { id, text: text.to_string(), taken: None }));
        Ok(())
    }

    /// Changes what the person wrote to a helper, on its call in the conversation, and shows it.
    fn mark_told(&mut self, conv: &str, call: &str, change: impl FnOnce(&mut Vec<Told>)) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        let Some(m) = c.messages.iter_mut().rev().find(|m| m.tools.iter().flatten().any(|t| t.id == call)) else { return };
        let Some(tool) = m.tools.iter_mut().flatten().find(|t| t.id == call) else { return };
        change(&mut tool.told);
        let id = m.id.clone();
        self.save(conv);
        self.push(conv, Some(&id), true);
    }

    /// A message to the helper reached it, or did not: `taken` says which, and `tag` names it when the CLI echoes it.
    fn helper_took(&mut self, call: &str, tag: Option<String>, taken: bool) {
        let Some(h) = self.helpers.get_mut(call) else { return };
        let Some(at) = tag.and_then(|t| h.waiting.iter().position(|w| *w == t)).or((!h.waiting.is_empty()).then_some(0)) else { return };
        let id = h.waiting.remove(at);
        let conv = h.conv.clone();
        let mut words = None;
        self.mark_told(&conv, call, |told| {
            if let Some(t) = told.iter_mut().find(|t| t.id == id) {
                t.taken = Some(taken);
                words = Some(t.text.clone());
            }
        });
        let Some(h) = self.helpers.get_mut(call) else { return };
        if taken {
            h.told.extend(words);
            // A turn that ended before this message was read is not the last: the message is a turn of its own now.
            h.held = None;
        } else if h.waiting.is_empty()
            && let Some(e) = h.held.take()
        {
            self.end_helper(call, e.ok, e.error.as_deref(), e.cost, e.incomplete);
        }
    }

    /// What a helper's own session reports. Its tools show inside the agent's call; nobody answers its approvals, because the
    /// person sees only the agent: the agent hears what was refused and can do it itself, where the person approves it.
    fn helper_event(&mut self, call: &str, hsid: u64, event: EngineEvent) {
        self.rewind_helper_event(call, hsid, &event);
        if let EngineEvent::Background(update) = event {
            let owner = self
                .helpers
                .get(call)
                .filter(|h| h.hsid == hsid)
                .map(|h| (h.conv.clone(), h.vendor, h.session.clone()))
                .or_else(|| self.background.owner(hsid));
            if let Some((conv, vendor, session)) = owner
                && self.background.update(&conv, hsid, vendor, &session, update, now())
            {
                self.emit_background();
                self.refresh_running();
            }
            return;
        }
        if matches!(event, EngineEvent::WriterExit { confirmed: true })
            && let Some((conv, ..)) = self.background.owner(hsid)
            && self.background.exited(&conv, hsid)
        {
            self.emit_background();
            self.refresh_running();
        }
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
            EngineEvent::Injected { tag } => self.helper_took(call, tag, true),
            EngineEvent::SteerFailed { tag } => self.helper_took(call, Some(tag), false),
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
                // A helper's session is new, so Claude's running total is the cost of its turns so far; Codex prices each turn,
                // and a message steered into a Codex turn stays in it. A price from tokens covers only the last turn.
                let total = cost_total_usd.filter(|c| c.is_finite() && *c >= 0.0);
                let cost = total.or_else(|| {
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
                let incomplete = incomplete || cost.is_none() || (h.earlier_turn && total.is_none());
                let error = (!ok).then_some(reason);
                // A message to the helper still on its way: this end waits to see whether another turn follows.
                if let Some(h) = self.helpers.get_mut(call).filter(|h| !h.waiting.is_empty() && !h.stopping) {
                    (h.new_block, h.earlier_turn) = (true, true);
                    h.held = Some(Ended { ok, error, cost, incomplete });
                    return;
                }
                self.end_helper(call, ok, error.as_deref(), cost, incomplete);
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
        if self.background.end_turn(&h.conv, h.hsid) {
            self.emit_background();
        }
        self.rewind_helper_retire(call, h.hsid);
        h.session.close();
        self.note_span(h.vendor, h.started, &h.model, cost, incomplete);
        let (ok, mut text) = helper::report(ok, &h.report, &h.refused, error);
        if !h.told.is_empty() {
            text.push_str(&helper::told_note(&h.told));
        }
        self.never_read(&h.conv, call, &h.waiting);
        let Some(l) = self.live.get(&h.conv).filter(|l| l.sid == h.sid) else { return };
        l.session.helper_reply(call, ok, text);
        if let (Some(cost), Some(m)) = (cost, self.turn(&h.conv)) {
            m.cost_usd = Some(m.cost_usd.unwrap_or(0.0) + cost);
        }
    }

    /// A message still on its way when the helper ended never reached it.
    fn never_read(&mut self, conv: &str, call: &str, waiting: &[String]) {
        if !waiting.is_empty() {
            self.mark_told(conv, call, |told| {
                for t in told.iter_mut().filter(|t| waiting.contains(&t.id)) {
                    t.taken = Some(false);
                }
            });
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
        // The checks it was waiting for stop too (`verifying.rs`).
        self.stop_verify(conv);
        let calls: Vec<String> = self.helpers.iter().filter(|(_, h)| h.conv == conv).map(|(call, _)| call.clone()).collect();
        for call in calls {
            if close {
                if let Some(h) = self.helpers.remove(&call) {
                    self.rewind_helper_retire(&call, h.hsid);
                    h.session.close();
                    self.note_span(h.vendor, h.started, &h.model, None, true);
                    self.never_read(&h.conv, &call, &h.waiting);
                }
            } else if let Some(h) = self.helpers.get_mut(&call) {
                h.stopping = true;
                h.session.interrupt();
                // A helper whose turn had not begun cannot be interrupted: it ends here if it has not reported by then.
                let hsid = h.hsid;
                self.expire_helper(&call, hsid, STOP_GRACE, "it was stopped");
            }
        }
    }
}
