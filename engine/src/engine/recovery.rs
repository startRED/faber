//! Move an interrupted turn once, or hold it until quota returns. Every dispatch re-enters the same gate.
use super::*;
use crate::budget::{QUEUE_SECS, Wait};
use crate::handover::{self, Partial, Plan};
use crate::route::{Asked, CHECKS, KINDS, LEVELS, ROUTER};

#[derive(Clone)]
pub(super) struct Transfer {
    pub reply: String,
    pub pick: Pick,
    pub text: String,
    pub images: Vec<Image>,
    pub by_hand: bool,
    pub retried: bool,
    pub moving: bool,
    /// The router's `check` for this request (`route::CHECKS`); `glance` when no verdict was read.
    pub check: &'static str,
}

pub(super) fn with_attachments(text: &str, area: Area, shown: &[String]) -> String {
    if shown.is_empty() {
        return text.into();
    }
    let place = if area == Area::Chat { " (in your working folder)" } else { "" };
    format!("{text}\n\nAttached files{place}:\n{}", shown.iter().map(|p| format!("- {p}")).collect::<Vec<_>>().join("\n"))
}

/// The CLI refused an option on its command line (`error: unknown option '--permission-prompts'`): a flag it no longer has.
pub(super) fn flag_rejected(message: &str) -> bool {
    message.to_ascii_lowercase().contains("unknown option")
}

pub(super) fn plan_limit(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    [
        "usage limit",
        "quota exceeded",
        "quota exhausted",
        "weekly limit",
        "subscription limit",
        "plan limit",
        "limit reached",
        "out of credits",
        "usage_limit_reached",
        "you've hit your limit",
        "you have reached your usage limit",
    ]
    .iter()
    .any(|phrase| m.contains(phrase))
}

impl Engine {
    /// Consent belongs to this reply and this vendor, and is consumed by the actual user dispatch.
    pub fn approve_credits(&self, id: &str, reply: &str, vendor: Vendor) -> Result<(), String> {
        self.with(|c| {
            let t = c.pending_turn(id, reply)?.clone();
            if t.pick.vendor != vendor
                || c.ledger.of(vendor).billing != Billing::Subscription
                || !c.ledger.of(vendor).paid_fallback.unwrap_or(true)
            {
                return Err("error.permission.stale".into());
            }
            c.credit = Some((id.into(), reply.into(), vendor));
            let text = c.continuation(id, &t);
            let result = c.start(id, reply, &t.pick, text, t.images, t.by_hand);
            c.credit = None;
            result
        })
    }
    /// The explicit alternative for a model chosen by hand; Auto never needs this click.
    pub fn move_turn(&self, id: &str, reply: &str) -> Result<(), String> {
        self.with(|c| {
            c.pending_turn(id, reply)?;
            c.recover(id, true)
        })
    }
    pub fn wait_turn(&self, id: &str, reply: &str) -> Result<(), String> {
        self.with(|c| {
            c.pending_turn(id, reply)?;
            c.queue(id)
        })
    }
}

pub(super) fn session_lost(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    // Codex: `thread/resume` of a thread with no rollout file. Claude Code: `--resume` of a session it never saved.
    m.contains("no rollout found") || m.contains("no conversation found")
}

impl Core {
    /// Whether this CLI already answered anything in the conversation, other than the turn now running: then a lost session
    /// loses real context, and that must be shown, never papered over.
    pub(super) fn said_on(&self, conv: &str, vendor: Vendor, turn: Option<&str>) -> bool {
        self.convs.get(conv).is_some_and(|c| {
            c.messages.iter().any(|m| {
                m.role == Role::Assistant && m.vendor == Some(vendor) && m.status != Some(Status::Error) && Some(m.id.as_str()) != turn
            })
        })
    }

    /// Whether the reply of the running turn already holds work: text, tool actions, or a retry after the quota ran out.
    /// Such a turn is never replayed from its request, because the replay would repeat what already ran.
    pub(super) fn reply_has_work(&self, conv: &str, turn: Option<&str>) -> bool {
        let retried = self.transfers.get(conv).is_some_and(|t| t.retried || t.moving);
        retried
            || self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| Some(m.id.as_str()) == turn)).is_some_and(|m| {
                !m.text.trim().is_empty()
                    || m.tools.as_ref().is_some_and(|t| !t.is_empty())
                    || m.parked.as_ref().is_some_and(|p| !p.is_empty())
            })
    }

    /// Runs the turn again on a new session of the same CLI with the same settings (the lost one is already forgotten). The
    /// messages before this turn go along as text when other assistants spoke in the conversation. Always true: the turn
    /// either runs or its reply is failed here.
    pub(super) fn rerun_fresh(&mut self, conv: &str, turn: Option<&str>) -> bool {
        let Some(t) = self.transfers.get(conv).filter(|t| Some(t.reply.as_str()) == turn).cloned() else { return false };
        let Some(c) = self.convs.get(conv) else { return false };
        let at = c.messages.iter().position(|m| m.id == t.reply).unwrap_or(0);
        let history = &c.messages[..at.saturating_sub(1)];
        let (area, data_dir) = (c.area, self.opts.data_dir.clone());
        let resent = !history.is_empty();
        let text = if resent { with_history(history, &t.text, &|n| attachment_place(area, &data_dir, conv, n)) } else { t.text.clone() };
        if let Some(m) = self.convs.get_mut(conv).and_then(|c| c.messages.iter_mut().find(|m| m.id == t.reply)) {
            m.note = resent.then(|| "note.historyResent".to_string());
        }
        if let Some(l) = self.live.remove(conv) {
            l.session.close();
        }
        if let Err(error) = self.dispatch(conv, &t.reply, &t.pick, text, t.images, t.by_hand) {
            // The old process is gone, so `turn()` finds nothing to mark: fail the reply by its saved id.
            self.fail_reply(conv, &t.reply, error);
            self.finish(conv, Some(t.reply));
        }
        true
    }

    pub(super) fn restore_recovery(&mut self, c: &mut Conversation) {
        let Some(m) = c.messages.last() else { return };
        if m.status != Some(Status::Error) || !matches!(m.error.as_deref(), Some("error.quotaOut" | "error.quotaWait")) {
            return;
        }
        let Some(q) = c.messages.iter().rev().find(|m| m.role == Role::User) else { return };
        let Some(vendor) = m.vendor else { return };
        let saved = m.recovery.clone().unwrap_or_else(|| {
            let shown = q
                .attachments
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|name| {
                    if c.area == Area::Chat {
                        format!("attachments/{name}")
                    } else {
                        self.opts.data_dir.join("attachments").join(&c.id).join(name).to_string_lossy().into_owned()
                    }
                })
                .collect::<Vec<_>>();
            // Older files have no dispatch intent: require explicit recovery and preserve any saved effort.
            Recovery {
                pick: Pick {
                    vendor,
                    model: m.model.clone(),
                    effort: m.effort.clone().unwrap_or_else(|| "medium".into()),
                    reason: m.reason.clone().unwrap_or_else(|| "reason.chosen".into()),
                    pace: m.pace.clone(),
                },
                text: with_attachments(&q.text, c.area, &shown),
                by_hand: true,
                retried: false,
                check: "glance".into(),
            }
        });
        self.transfers.insert(
            c.id.clone(),
            Transfer {
                reply: m.id.clone(),
                pick: saved.pick,
                text: saved.text,
                images: q.images.clone().unwrap_or_default(),
                by_hand: saved.by_hand,
                retried: saved.retried,
                moving: false,
                check: CHECKS.into_iter().find(|c| *c == saved.check).unwrap_or("glance"),
            },
        );
        if let Some(m) = c.messages.last_mut() {
            m.recovery = None;
        }
    }

    fn pending_turn(&self, id: &str, reply: &str) -> Result<&Transfer, String> {
        let t = self.transfers.get(id).filter(|t| t.reply == reply).ok_or("error.permission.stale")?;
        let m = self.convs.get(id).and_then(|c| c.messages.last()).filter(|m| m.id == reply).ok_or("error.permission.stale")?;
        if self.live.get(id).is_some_and(|l| l.turn.is_some())
            || !(m.wait_until.is_some() || matches!(m.error.as_deref(), Some("error.quotaOut" | "error.quotaWait")))
        {
            return Err("error.permission.stale".into());
        }
        Ok(t)
    }

    pub(super) fn dispatch(
        &mut self,
        id: &str,
        reply: &str,
        pick: &Pick,
        text: String,
        images: Vec<Image>,
        by_hand: bool,
    ) -> Result<(), String> {
        if let Some(t) = self.transfers.get_mut(id) {
            t.pick = pick.clone();
        }
        match self.start(id, reply, pick, text, images, by_hand) {
            Err(e) if e == "error.quotaOut" => self.recover(id, false),
            other => other,
        }
    }

    // The reason key does not keep the router's `check`, so the transfer carries it.
    fn verdict(pick: &Pick, check: &'static str) -> Asked {
        let words: Vec<&str> = pick.reason.split('.').collect();
        Asked {
            kind: KINDS.into_iter().find(|k| words.contains(k)).unwrap_or("chat"),
            difficulty: LEVELS.into_iter().find(|k| words.contains(k)).unwrap_or("medium"),
            check,
            confidence: 0.95,
        }
    }

    fn continuation(&self, id: &str, t: &Transfer) -> String {
        let partial = self.convs.get(id).and_then(|c| c.messages.last()).map(Partial::of).unwrap_or_default();
        if partial.is_empty() { t.text.clone() } else { handover::with_full("", &t.text, &partial) }
    }

    /// Family recovery stays on this vendor. Otherwise move to a healthy subscription, or wait.
    pub(super) fn recover(&mut self, id: &str, explicit: bool) -> Result<(), String> {
        // The reply moves on or fails: helpers of the stopped turn stop with it, wherever recovery was called from.
        self.stop_helpers(id, true);
        let Some(t) = self.transfers.get(id).cloned() else { return Err("error.quotaOut".into()) };
        if t.by_hand && !explicit {
            self.fail_reply(id, &t.reply, "error.quotaOut".into());
            return Ok(());
        }
        if t.retried {
            self.move_failed(id, &t.reply);
            return Ok(());
        }
        let verdict = Self::verdict(&t.pick, t.check);
        let own = self.table(&verdict, Some(t.pick.vendor));
        let other = Vendor::ALL.into_iter().find(|v| *v != t.pick.vendor).and_then(|v| self.table(&verdict, Some(v)));
        let Some(routed) = own.or(other) else { return self.queue(id) };
        let pick = routed.pick();
        let moved = pick.vendor != t.pick.vendor;
        if let Some(t) = self.transfers.get_mut(id) {
            t.retried = true;
            t.moving = moved;
        }
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.last_mut()) {
            m.status = Some(Status::Streaming);
            m.error = None;
            m.permissions = None;
            m.wait_until = None;
            if m.reason.as_deref() == Some("reason.waiting") {
                m.reason = Some(t.pick.reason.clone());
            }
        }
        self.save(id);
        self.push(id, Some(&t.reply), true);
        if let Some(l) = self.live.remove(id) {
            l.session.close();
        }
        let partial = self.convs.get(id).and_then(|c| c.messages.last()).map(Partial::of).unwrap_or_default();
        if !moved {
            let text = self.continuation(id, &t);
            if let Err(e) = self.start(id, &t.reply, &pick, text, t.images, false) {
                self.fail_reply(id, &t.reply, e);
            } else if let Some(t) = self.transfers.get_mut(id) {
                t.pick = pick;
            }
            return Ok(());
        }
        let c = self.convs.get(id).ok_or("error.conversationGone")?;
        let history = &c.messages[..c.messages.len().saturating_sub(2)];
        match handover::plan(history) {
            Plan::Full(all) => {
                let text = handover::with_full(&all, &t.text, &partial);
                self.resume_move(id, &t.reply, pick, text, false);
            }
            Plan::Summarise(prompt) => {
                let Some((_, model, effort)) = ROUTER
                    .into_iter()
                    .find(|(v, m, _)| *v == pick.vendor && self.allows(*v, family_of(m), Purpose::Summary, false).is_ok())
                else {
                    self.move_failed(id, &t.reply);
                    return Ok(());
                };
                let instructions = "Summarise the conversation in at most 600 words. Keep goals, decisions, file changes, tool effects and unfinished work. Do not execute tools or answer the user.";
                let Some(spare) = self.open_aux(pick.vendor, model, effort, instructions, Purpose::Summary) else {
                    self.move_failed(id, &t.reply);
                    return Ok(());
                };
                if self.begin_unbounded(spare.vendor).is_err() {
                    spare.session.close();
                    self.move_failed(id, &t.reply);
                    return Ok(());
                }
                *spare.asked.lock().unwrap_or_else(|e| e.into_inner()) = Some(self.secs());
                spare.session.send(UserTurn::text(prompt));
                let (me, id, reply) = (self.me.clone(), id.to_string(), t.reply.clone());
                tokio::spawn(async move {
                    let answer = tokio::time::timeout(auto::ROUTE_BUDGET, spare.answer).await.ok().and_then(|r| r.ok()).flatten();
                    spare.session.close();
                    let Some(core) = me.upgrade() else { return };
                    let mut c = core.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(at) = spare.asked.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        c.note_span(spare.vendor, at, spare.model_name, None, true);
                    }
                    if !c.active_reply(&id, &reply) {
                        return;
                    }
                    match answer.filter(|a| !a.trim().is_empty()) {
                        Some(summary) => {
                            let summary = summary.split_whitespace().take(handover::SUMMARY_WORDS).collect::<Vec<_>>().join(" ");
                            let text = handover::with_summary(&summary, &t.text, &partial);
                            c.resume_move(&id, &reply, pick, text, true);
                        }
                        None => c.move_failed(&id, &reply),
                    }
                });
            }
        }
        Ok(())
    }

    fn resume_move(&mut self, id: &str, reply: &str, pick: Pick, text: String, summary: bool) {
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.last_mut()) {
            m.moved_from = m.vendor;
            m.reason = Some(if summary { "reason.moved.summary" } else { "reason.moved.full" }.into());
        }
        if self.start(id, reply, &pick, text, vec![], false).is_err() {
            self.move_failed(id, reply);
        }
    }

    pub(super) fn move_failed(&mut self, id: &str, reply: &str) {
        if let Some(l) = self.live.remove(id) {
            l.session.close();
        }
        if let Some(t) = self.transfers.get(id)
            && let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.last_mut())
        {
            m.vendor = Some(t.pick.vendor);
            m.model = t.pick.model.clone();
            m.reason = Some(t.pick.reason.clone());
            m.moved_from = None;
            m.wait_until = None;
            m.permissions = None;
        }
        self.fail_reply(id, reply, "error.moveFailed".into());
        self.transfers.remove(id);
    }

    fn queue(&mut self, id: &str) -> Result<(), String> {
        let t = self.transfers.get(id).cloned().ok_or("error.permission.stale")?;
        let now = self.secs();
        let vendors: Vec<_> = self
            .vendors()
            .into_iter()
            .filter(|v| !t.by_hand || *v == t.pick.vendor)
            .filter(|v| self.ledger.of(*v).billing == Billing::Subscription && !(*v == Vendor::Claude && self.opts.claude_env_billed))
            .collect();
        let earliest = if t.by_hand {
            self.ledger.of(t.pick.vendor).snap(now).recovery_for(t.pick.model.as_deref().and_then(family_of), now)
        } else {
            crate::route::candidates(&vendors, &self.codex_models)
                .iter()
                .filter_map(|p| self.ledger.of(p.vendor).snap(now).recovery_for(family_of(p.model), now))
                .reduce(f64::min)
        };
        let outcome = match earliest {
            Some(at) if at - now <= QUEUE_SECS => Wait::Queue(at),
            at => Wait::Refuse(at),
        };
        let (until, queued) = match outcome {
            Wait::Queue(at) => (Some(at), true),
            Wait::Refuse(at) => (at, false),
        };
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.last_mut()) {
            m.wait_until = until;
            m.status = Some(if queued { Status::Streaming } else { Status::Error });
            m.reason = Some("reason.waiting".into());
            m.error = (!queued).then(|| "error.quotaWait".into());
            m.permissions = None;
        }
        self.save(id);
        self.push(id, Some(&t.reply), true);
        if !queued {
            // Nothing resumes this reply: keep going stops on it.
            self.keep_going_failed(id, &t.reply, "error.quotaWait");
        }
        if let Wait::Queue(at) = outcome {
            let (me, id, reply) = (self.me.clone(), id.to_string(), t.reply);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs_f64((at - now).max(0.001))).await;
                if let Some(core) = me.upgrade() {
                    core.lock().unwrap_or_else(|e| e.into_inner()).wake_reply(&id, &reply, at);
                }
            });
        }
        Ok(())
    }

    fn active_reply(&self, id: &str, reply: &str) -> bool {
        self.transfers.get(id).is_some_and(|t| t.reply == reply)
            && self.convs.get(id).and_then(|c| c.messages.last()).is_some_and(|m| m.id == reply && m.status == Some(Status::Streaming))
    }

    fn wake_reply(&mut self, id: &str, reply: &str, at: f64) {
        if !self.active_reply(id, reply)
            || self.live.get(id).is_some_and(|l| l.turn.is_some())
            || self.convs.get(id).and_then(|c| c.messages.last()).and_then(|m| m.wait_until) != Some(at)
        {
            return;
        }
        let Some(t) = self.transfers.get(id).cloned() else { return };
        if at > self.secs() {
            let _ = self.queue(id);
            return;
        }
        // Pick only when the original vendor remains blocked; no move for balance.
        // A scheduled resend is automatic even when the original model was picked by hand.
        if self.gate_turn(&t.pick, false).is_ok() {
            let text = self.continuation(id, &t);
            if let Err(e) = self.start(id, reply, &t.pick, text, t.images, false) {
                self.fail_reply(id, reply, e);
            }
        } else if let Err(e) = self.recover(id, false) {
            self.fail_reply(id, reply, e);
        }
    }

    pub(super) fn wake_waiting(&mut self) {
        let due: Vec<_> = self
            .convs
            .iter()
            .filter_map(|(id, c)| {
                let m = c.messages.last()?;
                let at = m.wait_until?;
                (m.status == Some(Status::Streaming) && at <= self.secs()).then(|| (id.clone(), m.id.clone(), at))
            })
            .collect();
        for (id, reply, at) in due {
            self.wake_reply(&id, &reply, at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;
    #[test]
    fn a_lost_session_is_told_from_other_failures() {
        assert!(session_lost("Codex thread failed: no rollout found for thread id 01a1"));
        assert!(session_lost("Claude Code stopped (exit 1). No conversation found with session ID: abc"));
        assert!(!session_lost("Codex thread failed: unauthorized"));
    }

    #[test]
    fn only_plan_limits_trigger_a_handover() {
        assert!(plan_limit("You've hit your limit. Try again tomorrow."));
        assert!(plan_limit("usage_limit_reached"));
        assert!(!plan_limit("429: too many requests; retry after 30 seconds"));
        assert!(!plan_limit("connection closed"));
    }

    /// Starts a conversation whose resumed Codex session reports a lost session, with the reply shaped by `shape` and a transfer
    /// that was `retried` or not. Returns the reply after the error and how many processes the engine tried to start.
    fn lost(shape: impl FnOnce(&mut Message), retried: bool) -> (Message, usize) {
        use crate::events::EngineEvent;
        let spawned = Arc::new(Mutex::new(0));
        let counted = spawned.clone();
        let opts = EngineOptions {
            data_dir: tempfile::tempdir().unwrap().keep(),
            clis: BTreeMap::from([(Vendor::Codex, Cli { command: "codex".into(), prefix: vec![] })]),
            spawn: Arc::new(move |_, _, _| {
                *counted.lock().unwrap() += 1;
                Err(std::io::Error::other("no process in this test"))
            }),
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        };
        let engine = Engine::new(opts, Arc::new(|_| {}));
        let reply = engine.with(|c| {
            let id = c.blank(Area::Chat, None);
            let conv = c.convs.get_mut(&id).unwrap();
            conv.messages.push(Message::new("question".into(), Role::User, "question".into(), 1));
            let mut reply = Message::new("reply".into(), Role::Assistant, String::new(), 2);
            reply.vendor = Some(Vendor::Codex);
            reply.status = Some(Status::Streaming);
            shape(&mut reply);
            conv.messages.push(reply);
            let pick = Pick { vendor: Vendor::Codex, model: None, effort: "medium".into(), reason: "reason.test".into(), pace: None };
            c.transfers.insert(
                id.clone(),
                Transfer {
                    reply: "reply".into(),
                    pick,
                    text: "question".into(),
                    images: vec![],
                    by_hand: true,
                    retried,
                    moving: false,
                    check: "glance",
                },
            );
            c.live.insert(
                id.clone(),
                Live {
                    cwd: PathBuf::new(),
                    vendor: Vendor::Codex,
                    model: None,
                    running: None,
                    effort: "medium".into(),
                    sid: 1,
                    session: Session::from_fn(|_| {}),
                    turn: Some("reply".into()),
                    break_pending: false,
                    answer_started: false,
                    used: 0,
                    cli: Cli { command: "codex".into(), prefix: vec![] },
                    resumed: true,
                    permissions: Permissions::FullAccess,
                    tuning: Default::default(),
                    runs: Permissions::FullAccess,
                    by_hand: true,
                    approved: false,
                    started: 0.0,
                    served: false,
                    unsaved_session: None,
                    steering: vec![],
                    held: None,
                },
            );
            c.refresh_running();
            assert_eq!(c.running.items.len(), 1, "the control: the turn is on the running list before it fails");
            let message = "Codex thread failed: no rollout found for thread id x".to_string();
            c.handle(&id, 1, EngineEvent::Error { message, fatal: true });
            (c.convs[&id].messages[1].clone(), c.running.items.len())
        });
        let (reply, still_running) = reply;
        let spawns = *spawned.lock().unwrap();
        // A turn that failed without starting another process must have left the running list, or `Engine::is_working` stays
        // true and no update ever installs. (A replay is a turn that is genuinely running again.)
        assert!(spawns > 0 || still_running == 0, "a failed turn stays on the running list");
        (reply, spawns)
    }

    #[tokio::test]
    async fn a_reply_with_nothing_done_is_replayed_on_a_new_session() {
        let (_, spawns) = lost(|_| {}, false);
        assert!(spawns > 0, "the control: without work the turn runs again on a new session");
    }

    #[tokio::test]
    async fn a_quota_retry_is_never_replayed_even_before_any_text() {
        let (reply, spawns) = lost(|_| {}, true);
        assert_eq!((spawns, reply.error.as_deref()), (0, Some("error.sessionLost")));
    }

    #[tokio::test]
    async fn a_reply_with_only_tool_actions_is_never_replayed() {
        let tool = |m: &mut Message| {
            m.tools = Some(vec![crate::model::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({ "command": "git push" }),
                ok: Some(true),
                output: Some("pushed".into()),
                declined: false,
                parent: None,
                sign: None,
                facts: None,
                told: vec![],
                check: false,
                started: None,
            }]);
        };
        let (reply, spawns) = lost(tool, false);
        assert_eq!((spawns, reply.error.as_deref()), (0, Some("error.sessionLost")));
    }
}
