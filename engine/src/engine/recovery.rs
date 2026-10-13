//! Move an interrupted turn once, or hold it until quota returns. Every dispatch re-enters the same gate.
use super::*;
use crate::budget::{QUEUE_SECS, Wait};
use crate::handover::{self, Partial, Plan};
use crate::route::{Asked, KINDS, LEVELS, ROUTER};

#[derive(Clone)]
pub(super) struct Transfer {
    pub reply: String,
    pub pick: Pick,
    pub text: String,
    pub images: Vec<Image>,
    pub by_hand: bool,
    pub retried: bool,
    pub moving: bool,
}

pub(super) fn with_attachments(text: &str, area: Area, shown: &[String]) -> String {
    if shown.is_empty() {
        return text.into();
    }
    let place = if area == Area::Chat { " (in your working folder)" } else { "" };
    format!("{text}\n\nAttached files{place}:\n{}", shown.iter().map(|p| format!("- {p}")).collect::<Vec<_>>().join("\n"))
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

impl Core {
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

    // The reason key does not keep the router's `check`, so a recovery re-pick drops the blind bump.
    fn verdict(pick: &Pick) -> Asked {
        let words: Vec<&str> = pick.reason.split('.').collect();
        Asked {
            kind: KINDS.into_iter().find(|k| words.contains(k)).unwrap_or("chat"),
            difficulty: LEVELS.into_iter().find(|k| words.contains(k)).unwrap_or("medium"),
            check: "glance",
            confidence: 0.95,
        }
    }

    fn continuation(&self, id: &str, t: &Transfer) -> String {
        let partial = self.convs.get(id).and_then(|c| c.messages.last()).map(Partial::of).unwrap_or_default();
        if partial.is_empty() { t.text.clone() } else { handover::with_full("", &t.text, &partial) }
    }

    /// Family recovery stays on this vendor. Otherwise move to a healthy subscription, or wait.
    pub(super) fn recover(&mut self, id: &str, explicit: bool) -> Result<(), String> {
        let Some(t) = self.transfers.get(id).cloned() else { return Err("error.quotaOut".into()) };
        if t.by_hand && !explicit {
            self.fail_reply(id, &t.reply, "error.quotaOut".into());
            return Ok(());
        }
        if t.retried {
            self.move_failed(id, &t.reply);
            return Ok(());
        }
        let verdict = Self::verdict(&t.pick);
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
    use super::plan_limit;
    #[test]
    fn only_plan_limits_trigger_a_handover() {
        assert!(plan_limit("You've hit your limit. Try again tomorrow."));
        assert!(plan_limit("usage_limit_reached"));
        assert!(!plan_limit("429: too many requests; retry after 30 seconds"));
        assert!(!plan_limit("connection closed"));
    }
}
