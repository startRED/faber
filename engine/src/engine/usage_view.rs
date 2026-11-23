//! Small account views for the screen: the account's quota and Faber's own money have separate labels and fields.
use super::*;
use crate::account::AccountStatus;
use crate::budget::{Snap, State, elapsed};

impl Core {
    /// Whether a turn of this vendor is running now: the counter is about to rise by what it is spending.
    pub(super) fn turn_running(&self, vendor: Vendor) -> bool {
        self.live.values().any(|l| l.vendor == vendor && l.turn.is_some()) || self.helper_running(vendor) || self.aside_running(vendor)
    }

    /// The vendor’s quota as every decision (and the screen) sees it: the ledger’s snapshot, still counting while a turn of
    /// the vendor runs. A reading taken now is a floor then, so no discount is believed on it.
    pub(super) fn snap(&self, vendor: Vendor, now: f64) -> Snap {
        let mut snap = self.ledger.of(vendor).snap(now);
        snap.counting |= self.turn_running(vendor);
        snap
    }

    pub(super) fn usage(&self) -> Vec<VendorUsage> {
        let now = self.secs();
        let day = ((now + self.utc_offset as f64) / 86_400.0).floor() as i64;
        // Antigravity is not one of the CLIs the engine runs yet (R531), but once its account check found it signed in its
        // limits are shown: plan and billing, no reading, Faber's own spend.
        let connected = self.ledger.gemini.plan_seen_at.is_some().then_some(Vendor::Gemini);
        self.vendors()
            .into_iter()
            .chain(connected.filter(|g| !self.opts.clis.contains_key(g)))
            .map(|vendor| {
                let v = self.ledger.of(vendor);
                let mut snap = self.snap(vendor, now);
                if vendor == Vendor::Claude && self.opts.claude_env_billed {
                    snap.billing = Billing::UsageBilled;
                }
                let shape = v.shape();
                let outside = v.outside(now);
                let state = if snap.billing == Billing::Unknown {
                    "unknown"
                } else if self.allows(vendor, None, Purpose::UserTurn { by_hand: false }, false).is_err() {
                    "out"
                } else if snap.windows.is_empty() {
                    "unknown"
                } else {
                    match snap.state(&shape, self.utc_offset, now) {
                        State::Spare => "spare",
                        State::Normal => "ok",
                        State::Fast => "fast",
                        State::Tight => "low",
                    }
                };
                let windows = snap
                    .windows
                    .iter()
                    .map(|(name, w)| {
                        let e = elapsed(&shape, w.resets_at - w.length, now, w.resets_at, self.utc_offset);
                        let uncounted = v.uncounted(name, now);
                        WindowUsage {
                            name: name.clone(),
                            used: v.readings.get(name).map_or(w.used, |r| r.used),
                            elapsed: e,
                            expected: if w.length > 0.0 { ((now - (w.resets_at - w.length)) / w.length).clamp(0.0, 1.0) } else { e },
                            resets_at: w.resets_at,
                            read_at: w.read_at,
                            uncounted,
                        }
                    })
                    .collect();
                VendorUsage {
                    vendor,
                    plan: v.typed_plan.clone().or_else(|| v.plan.clone()),
                    state,
                    learning: v.learned.covered < 336.0 * 3600.0,
                    relearning: v.relearning(now),
                    outside_share: outside.share(),
                    foreign_use: outside.foreign,
                    overage_on: v.overage_active || v.paid_fallback == Some(true),
                    paid_possible: v.paid_fallback.unwrap_or(true),
                    billed_per_use: snap.billing == Billing::UsageBilled,
                    billing_unknown: snap.billing == Billing::Unknown,
                    ask_for_plan: snap.billing == Billing::Subscription && v.ask_for_plan(),
                    own_spend_usd: v.own_spend.get(&day).copied().unwrap_or(0.0),
                    spend_days: v.spend_days(day),
                    unmetered: v.unmetered.get(&day).copied().unwrap_or(0),
                    counting: snap.counting,
                    windows,
                }
            })
            .collect()
    }
    pub(super) fn emit_usage(&self) {
        (self.emit)(AppEvent::Usage { vendors: self.usage() });
    }
}

impl Engine {
    /// What Antigravity's account check (or later its driver's own `session/new`) found: signed in, with the plan its tier
    /// names (`signals::antigravity_plan`), makes it connected, with subscription billing only for a tier Faber recognises;
    /// signed out makes it not connected; an unsure check refreshes nothing. No process is started here.
    pub fn note_antigravity_account(&self, status: AccountStatus, plan: Option<Signals>) {
        self.with(|c| c.antigravity_checked(status, plan))
    }
    /// A turn of Antigravity ended with its CLI's out-of-quota error: out until the reset the error names, or for
    /// `AGY_BACKOFF` when it names none or one already past. Antigravity has no reading that could show room sooner.
    pub fn note_antigravity_out(&self, resets_at: Option<f64>) {
        self.with(|c| c.antigravity_out(resets_at))
    }
}

impl Core {
    pub(super) fn antigravity_out(&mut self, resets_at: Option<f64>) {
        let now = self.secs();
        let until = resets_at.filter(|t| t.is_finite() && *t > now).unwrap_or(now + crate::budget::AGY_BACKOFF);
        self.apply_signals(Vendor::Gemini, Signals { reach: Some(Reach::Out(Some(until))), ..Signals::default() });
    }

    fn antigravity_checked(&mut self, status: AccountStatus, plan: Option<Signals>) {
        let now = self.secs();
        let v = self.ledger.of_mut(Vendor::Gemini);
        match (status, plan) {
            (AccountStatus::SignedIn, Some(s)) => {
                // Marks it connected (`plan_seen_at`) even when the tier names no plan, which then reads "plan unknown": a name
                // kept from an earlier tier would show a plan this account may not have.
                v.plan_detected(s.plan.clone(), now);
                if s.plan.is_none() {
                    v.plan = None;
                    v.plan_missing();
                }
                self.apply_signals(Vendor::Gemini, Signals { plan: None, ..s });
            }
            (AccountStatus::SignedOut, _) => {
                v.plan_seen_at = None;
                self.apply_signals(Vendor::Gemini, Signals { billing: Some(Billing::Unknown), ..Signals::default() });
            }
            _ => v.plan_failed(),
        }
        self.save_ledger(true);
    }
}
