//! Small account views for the screen: the account's quota and Faber's own money have separate labels and fields.
use super::*;
use crate::budget::{Snap, State, elapsed};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowUsage {
    pub name: String,
    pub used: f64,
    pub elapsed: f64,
    pub expected: f64,
    pub resets_at: f64,
    /// When the vendor's counter was read, Unix seconds. `used` is that reading as it was, never adjusted.
    pub read_at: f64,
    /// The share Faber's own recent work adds that the reading does not hold yet; only where a measured conversion exists.
    pub uncounted: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorUsage {
    pub vendor: Vendor,
    pub plan: Option<String>,
    pub state: &'static str,
    pub learning: bool,
    pub relearning: bool,
    pub outside_share: Option<f64>,
    pub foreign_use: bool,
    pub overage_on: bool,
    pub billed_per_use: bool,
    pub billing_unknown: bool,
    pub ask_for_plan: bool,
    pub paid_possible: bool,
    pub own_spend_usd: f64,
    pub unmetered: u32,
    /// Faber's own recent work may not be in the readings yet (or is running now): they may still rise.
    pub counting: bool,
    pub windows: Vec<WindowUsage>,
}
impl Core {
    /// Whether a turn of this vendor is running now: the counter is about to rise by what it is spending.
    pub(super) fn turn_running(&self, vendor: Vendor) -> bool {
        self.live.values().any(|l| l.vendor == vendor && l.turn.is_some())
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
        self.vendors()
            .into_iter()
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
                            expected: e,
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
