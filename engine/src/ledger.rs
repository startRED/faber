//! What Faber remembers about each vendor's quota: the latest reading of every window, the readings over the last week, the
//! turns Faber itself ran, and what was learned from them. Plain data with explicit times, so it is testable without a clock;
//! the engine keeps one and writes it to `budget.json`.

use crate::budget::{Billing, Reach, Shape, Snap, Window, vendor_wide, window_secs};
use crate::learn::{Learned, Outside, Point, Span, Stretch, Weights, outside, same_models_changed, stretches, weights};
use crate::model::Vendor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Readings kept per window, and spans kept per vendor: bounds the file at a few tens of kilobytes.
const KEEP_POINTS: usize = 200;
const KEEP_SPANS: usize = 200;
const KEEP_STRETCHES: usize = 60;
/// A plan check younger than this lets a discount stand.
pub const PLAN_FRESH: f64 = 30.0 * 60.0;
/// After a plan change, discounts wait for this long or for enough stretches.
pub const RELEARN: f64 = 48.0 * 3600.0;
/// Failed checks in a row before the user is asked which plan they have.
pub const ASK_AFTER: u32 = 3;

/// Whether a reading can be real: a share between 0 and a little over 1, and a reset time that is a number.
pub fn plausible(used: f64, resets_at: f64) -> bool {
    (0.0..=1.5).contains(&used) && resets_at.is_finite() && resets_at > 0.0
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub used: f64,
    pub resets_at: f64,
    pub read_at: f64,
}

/// A quota reading with the existing own-spend counters at that instant. Their deltas survive local offset changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySample {
    pub reading: Point,
    pub own_spend: BTreeMap<i64, f64>,
    #[serde(default)]
    pub unmetered: BTreeMap<i64, u32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// What is kept for one vendor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VendorLedger {
    pub readings: BTreeMap<String, Reading>,
    pub history: BTreeMap<String, Vec<Point>>,
    /// Evidence at or just before a reset-relative day boundary; a missing baseline grants no automated work.
    pub day_baselines: BTreeMap<String, DailySample>,
    /// Last weekly sample, so a reading just before a boundary is evidence rather than an invented empty day.
    pub daily_samples: BTreeMap<String, DailySample>,
    /// Only the engine's no-model account check stamps this evidence; ordinary quota events do not.
    pub zero_turn_readings: BTreeMap<String, Reading>,
    pub spans: Vec<Span>,
    /// Own API-equivalent USD per local day, independent of account percentages.
    pub own_spend: BTreeMap<i64, f64>,
    pub unmetered: BTreeMap<i64, u32>,
    pub learned: Learned,
    pub weights: Weights,
    pub stretches: Vec<Stretch>,
    /// The plan as last detected, and the one the user typed when detection failed.
    pub plan: Option<String>,
    pub typed_plan: Option<String>,
    /// Readings and spans before this time are ignored for outside use and weights (a plan change starts them again).
    pub since: f64,
    pub relearn_until: f64,
    pub stretches_since: usize,
    // Billing and live reach are rechecked; paid-fallback evidence and own metering survive a restart.
    #[serde(skip)]
    pub billing: Billing,
    #[serde(skip)]
    pub reach: Reach,
    #[serde(skip)]
    pub overage_active: bool,
    /// None means paid fallback is still possible until the CLI explicitly rules it out.
    pub paid_fallback: Option<bool>,
    /// Only an explicit paid-fallback field renews this evidence; quota updates do not.
    pub paid_fallback_at: Option<f64>,
    #[serde(skip)]
    pub credit_balance: Option<f64>,
    /// When a plan check last succeeded, and how many failed in a row since.
    #[serde(skip)]
    pub plan_seen_at: Option<f64>,
    #[serde(skip)]
    pub failed_checks: u32,
    /// A check worked but named no plan.
    #[serde(skip)]
    pub nameless: bool,
    /// Fields written by a newer Faber, kept as they were: an older build must not drop what it does not know.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Ledger {
    pub claude: VendorLedger,
    pub codex: VendorLedger,
    /// Fields written by a newer Faber, kept as they were.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Ledger {
    /// Makes a ledger read from disk safe to use: a damaged or hand-edited file must neither crash anything nor make a number
    /// up. Whatever does not make sense is dropped, which only means that part is learned again.
    pub fn sanitize(&mut self) {
        self.claude.sanitize();
        self.codex.sanitize();
    }

    pub fn of(&self, vendor: Vendor) -> &VendorLedger {
        match vendor {
            Vendor::Claude => &self.claude,
            Vendor::Codex => &self.codex,
        }
    }

    pub fn of_mut(&mut self, vendor: Vendor) -> &mut VendorLedger {
        match vendor {
            Vendor::Claude => &mut self.claude,
            Vendor::Codex => &mut self.codex,
        }
    }
}

impl VendorLedger {
    fn sanitize(&mut self) {
        self.own_spend.retain(|_, cost| cost.is_finite() && *cost >= 0.0);
        self.readings.retain(|_, r| plausible(r.used, r.resets_at) && r.read_at.is_finite());
        for list in self.history.values_mut() {
            list.retain(|p| plausible(p.used, p.resets_at) && p.at.is_finite());
            list.drain(..list.len().saturating_sub(KEEP_POINTS));
        }
        self.history.retain(|_, list| !list.is_empty());
        self.spans
            .retain(|s| s.start.is_finite() && s.end.is_finite() && s.end >= s.start && s.cost.is_none_or(|c| c.is_finite() && c >= 0.0));
        self.spans.drain(..self.spans.len().saturating_sub(KEEP_SPANS));
        self.stretches.retain(|s| s.ppd.is_finite() && s.ppd > 0.0);
        self.stretches.drain(..self.stretches.len().saturating_sub(KEEP_STRETCHES));
        // The shape indexes 168 hours of the week; any other length would index out of range.
        if self.learned.mass.len() != 168
            || self.learned.mass.iter().any(|m| !m.is_finite() || *m < 0.0)
            || !self.learned.covered.is_finite()
        {
            self.learned = Learned::default();
        }
        self.weights.raw.retain(|_, r| r.is_finite() && *r > 0.0);
        self.weights.w.retain(|_, w| w.is_finite());
        self.weights.w.values_mut().for_each(|w| *w = w.clamp(0.5, 2.0));
        self.weights.capacity = self.weights.capacity.filter(|c| c.is_finite() && *c > 0.0);
        for t in [&mut self.since, &mut self.relearn_until] {
            if !t.is_finite() {
                *t = 0.0;
            }
        }
    }

    /// The window the week's shape and the weights are learned from: the shortest vendor-wide one, which moves the most
    /// points per unit of use. Only its readings are kept as a history.
    pub fn primary(&self, now: f64) -> Option<&str> {
        self.readings
            .iter()
            .filter(|(_, r)| r.resets_at > now)
            .map(|(n, _)| n)
            .filter(|n| vendor_wide(n))
            .filter_map(|n| Some((window_secs(n)?, n.as_str())))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, n)| n)
    }

    /// A reading of a window. Readings that make no sense (not a number, below 0 or far above 1) are dropped.
    pub fn record(&mut self, window: &str, used: f64, resets_at: f64, now: f64, offset: i64) {
        self.own_spend.retain(|_, cost| cost.is_finite() && *cost >= 0.0);
        self.readings.retain(|_, r| r.resets_at > now);
        if !plausible(used, resets_at) || resets_at <= now {
            return;
        }
        // Whole seconds and hundredths are all a reading holds; the file stays small.
        let (now, resets_at, used) = (now.round(), resets_at.round(), (used * 1000.0).round() / 1000.0);
        self.readings.insert(window.to_string(), Reading { used, resets_at, read_at: now });
        let point = Point { at: now, used, resets_at };
        if vendor_wide(window)
            && window_secs(window) == Some(7.0 * 86_400.0)
            && let Some(start) = crate::budget::daily_start(resets_at, now)
        {
            let sample = DailySample {
                reading: point,
                own_spend: self.own_spend.clone(),
                unmetered: self.unmetered.clone(),
                extra: BTreeMap::new(),
            };
            let prior = self.daily_samples.insert(window.into(), sample.clone());
            if now == start {
                let saved = self.day_baselines.entry(window.into()).or_insert(sample.clone());
                if saved.reading.at == start && saved.reading.resets_at == resets_at {
                    saved.reading.used = saved.reading.used.min(used);
                } else {
                    *saved = sample;
                }
            } else if let Some(prior) = prior
                && prior.reading.resets_at == resets_at
                && prior.reading.at <= start
                && (0.0..=crate::budget::STALE_SECS).contains(&(start - prior.reading.at))
            {
                self.day_baselines.insert(window.into(), prior);
            }
        }
        let primary = self.primary(now).map(String::from);
        self.day_baselines.retain(|name, _| self.readings.contains_key(name));
        self.daily_samples.retain(|name, _| self.readings.contains_key(name));
        // Keep reset history for calibration, but retire abandoned windows after a month.
        self.history
            .retain(|name, points| self.readings.contains_key(name) || points.last().is_some_and(|p| p.at >= now - 30.0 * 86_400.0));
        let list = self.history.entry(window.to_string()).or_default();
        let previous = list.last().copied();
        list.push(point);
        if list.len() > KEEP_POINTS {
            list.drain(..list.len() - KEEP_POINTS);
        }
        if primary.as_deref() == Some(window)
            && let Some(previous) = previous
            && previous.at >= self.since
        {
            self.learned.add(&previous, &point, offset);
        }
    }

    /// A turn Faber ran, for outside use and weights.
    pub fn span(&mut self, span: Span) {
        let span = Span { start: span.start.round(), end: span.end.round(), cost: span.cost.map(|c| (c * 1e6).round() / 1e6), ..span };
        self.spans.push(span);
        if self.spans.len() > KEEP_SPANS {
            self.spans.drain(..self.spans.len() - KEEP_SPANS);
        }
    }

    fn recent(&self, window: &str) -> Vec<Point> {
        self.history.get(window).map(|h| h.iter().filter(|p| p.at >= self.since).copied().collect()).unwrap_or_default()
    }

    fn recent_spans(&self) -> Vec<Span> {
        self.spans.iter().filter(|s| s.end >= self.since).cloned().collect()
    }

    /// Use from outside Faber over the last week, from the primary window.
    pub fn outside(&self, now: f64) -> Outside {
        self.primary(now).map(|w| outside(&self.recent(w), &self.recent_spans(), now)).unwrap_or_default()
    }

    /// Learns the weights again from the stretches in the history. Returns true when the vendor's capacity moved by more
    /// than a factor of two, which is a plan change the name did not announce.
    pub fn learn_weights(&mut self, now: f64) -> bool {
        let Some(window) = self.primary(now).map(String::from) else { return false };
        let found = stretches(&self.recent(&window), &self.recent_spans());
        self.stretches = found.iter().rev().take(KEEP_STRETCHES).rev().cloned().collect();
        self.stretches_since = found.len();
        let fresh = weights(&found);
        let changed = same_models_changed(&self.weights, &fresh);
        if changed {
            self.restart(now);
        } else {
            self.weights = fresh;
        }
        changed
    }

    /// A different plan: what was learned about weights, capacity and outside use is cleared, and discounts wait.
    fn restart(&mut self, now: f64) {
        self.weights = Weights::default();
        self.stretches.clear();
        self.stretches_since = 0;
        self.since = now;
        self.relearn_until = now + RELEARN;
    }

    /// A plan name, from a successful check. A change from one name to another restarts the learning.
    pub fn plan_detected(&mut self, plan: Option<String>, now: f64) {
        self.plan_seen_at = Some(now);
        self.failed_checks = 0;
        self.nameless = self.nameless && plan.is_none();
        if let (Some(old), Some(new)) = (&self.plan, &plan)
            && old != new
        {
            self.restart(now);
        }
        if plan.is_some() {
            self.plan = plan;
        }
        // A typed plan never outlives a detection that disagrees with it.
        if self.plan.is_some() && self.typed_plan != self.plan {
            self.typed_plan = None;
        }
    }

    pub fn plan_failed(&mut self) {
        self.failed_checks += 1;
    }

    /// A check worked but the answer named no plan.
    pub fn plan_missing(&mut self) {
        self.nameless = true;
        self.failed_checks = 0;
    }

    /// Whether to ask the user which plan they have: detection failed three times in a row, or worked but named no plan.
    pub fn ask_for_plan(&self) -> bool {
        self.plan.is_none() && self.typed_plan.is_none() && (self.failed_checks >= ASK_AFTER || self.nameless)
    }

    /// A discount needs a plan detected less than half an hour ago, or typed by the user.
    pub fn plan_ok(&self, now: f64) -> bool {
        self.typed_plan.is_some() || self.plan_seen_at.is_some_and(|t| (0.0..=PLAN_FRESH).contains(&(now - t)))
    }

    pub fn relearning(&self, now: f64) -> bool {
        now < self.relearn_until && self.stretches_since < crate::learn::ENOUGH
    }

    pub fn shape(&self) -> Shape {
        self.learned.shape()
    }

    pub fn snap(&self, now: f64) -> Snap {
        let windows = self
            .readings
            .iter()
            .filter(|(_, r)| r.resets_at > now)
            .map(|(name, r)| {
                (
                    name.clone(),
                    Window { used: r.used, resets_at: r.resets_at, read_at: r.read_at, length: window_secs(name).unwrap_or(0.0) },
                )
            })
            .collect();
        Snap {
            windows,
            billing: self.billing,
            reach: self.reach,
            overage_active: self.overage_active,
            paid_fallback: self.paid_fallback.unwrap_or(true),
            jumped: self.outside(now).jumped(),
            relearning: self.relearning(now),
            plan_ok: self.plan_ok(now),
        }
    }
}

impl VendorLedger {
    pub fn spend(&mut self, cost: Option<f64>, incomplete: bool, now: f64, offset: i64) {
        let day = ((now + offset as f64) / 86_400.0).floor() as i64;
        if let Some(cost) = cost.filter(|c| c.is_finite() && *c >= 0.0) {
            *self.own_spend.entry(day).or_default() += cost;
        }
        if incomplete || cost.is_none() {
            *self.unmetered.entry(day).or_default() += 1;
        }
        self.own_spend.retain(|d, _| *d >= day - 30);
        self.unmetered.retain(|d, _| *d >= day - 30);
        while self.own_spend.len() > 31 {
            self.own_spend.pop_first();
        }
        while self.unmetered.len() > 31 {
            self.unmetered.pop_first();
        }
    }
}
