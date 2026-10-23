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
/// How long a vendor's counter may trail real use: measured at 4.1 min (Claude) and under 0.5 min (Codex), with margin
/// (docs/product/quota-pacing.md "Quota lag, measured"). Work that ended
/// within this long ago may not be in the reading yet.
pub const LAG_CLAUDE: f64 = 10.0 * 60.0;
pub const LAG_CODEX: f64 = 5.0 * 60.0;
/// Two reset times this close are the same window: `/usage` prints the minute, the stream prints the second.
const SAME_RESET: f64 = 120.0;
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

/// One request Faber made, with the moment it really ended (a `Span`'s end is the moment of the reading taken during it, which
/// is what the learning needs). Kept until every live window was read after it ended plus the lag: it is what tells
/// whether the vendor's counter has caught up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Work {
    pub start: f64,
    pub finished: f64,
    pub model: String,
    pub cost: Option<f64>,
    pub incomplete: bool,
}

/// Requests kept one by one while the counter may not hold them yet. Past this the oldest are folded into one `Folded` (never
/// dropped: unsettled work keeps the floor up).
const KEEP_WORK: usize = 100;

/// Pending requests folded into one conservative record when there are too many to keep. It ends when the last of them ends
/// (it settles last), starts when the first began, carries the points of the part that could be converted, and says whether any
/// part could not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folded {
    pub start: f64,
    pub finished: f64,
    pub points: f64,
    pub unknown: bool,
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
    pub work: Vec<Work>,
    pub folded: Option<Folded>,
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
    /// Seconds the counter may trail real use; 0 switches the "still counting" logic off.
    #[serde(skip)]
    pub lag: f64,
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
        self.claude.lag = LAG_CLAUDE;
        self.codex.lag = LAG_CODEX;
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
        self.work.retain(|w| w.start.is_finite() && w.finished.is_finite() && w.cost.is_none_or(|c| c.is_finite() && c >= 0.0));
        self.folded =
            self.folded.take().filter(|f| f.start.is_finite() && f.finished.is_finite() && f.points.is_finite() && f.points >= 0.0);
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
        self.fold_overflow();
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

    pub(crate) fn canonical_reset(&self, window: &str, resets_at: f64, now: f64) -> f64 {
        self.readings
            .get(window)
            .filter(|held| resets_at > now && held.resets_at > now && (held.resets_at - resets_at).abs() <= SAME_RESET)
            .map_or(resets_at.round(), |held| held.resets_at)
    }

    /// A reading of a window. Readings that make no sense (not a number, below 0 or far above 1) are dropped.
    pub fn record(&mut self, window: &str, used: f64, resets_at: f64, now: f64, offset: i64) {
        self.own_spend.retain(|_, cost| cost.is_finite() && *cost >= 0.0);
        self.readings.retain(|_, r| r.resets_at > now);
        if !plausible(used, resets_at) || resets_at <= now {
            return;
        }
        // Whole seconds and hundredths are all a reading holds; the file stays small. A reset time within two minutes of the
        // one held is the same window's, in the form first seen, so the history keeps comparing like with like.
        let (now, resets_at, used) = (now.round(), self.canonical_reset(window, resets_at, now), (used * 1000.0).round() / 1000.0);
        self.readings.insert(window.to_string(), Reading { used, resets_at, read_at: now });
        self.prune(now);
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

    /// The windows with a live reading.
    fn live_windows(&self, now: f64) -> Vec<&str> {
        self.readings.iter().filter(|(_, r)| r.resets_at > now).map(|(n, _)| n.as_str()).collect()
    }

    /// Whether a request that ended at `finished` is known to be in this window's counter: the window has been read since it
    /// ended plus the lag. Only the window's own reading says so; reading another window settles nothing here, and time alone
    /// settles nothing, so when every refresh fails the floor stays up.
    fn settled(&self, finished: f64, window: &str, now: f64) -> bool {
        self.readings.get(window).is_some_and(|r| r.resets_at > now && r.read_at >= finished + self.lag)
    }

    /// Faber's own requests that this window's counter may not hold yet.
    fn pending(&self, window: &str, now: f64) -> Vec<&Work> {
        if self.lag <= 0.0 {
            return vec![];
        }
        self.work.iter().filter(|w| w.finished >= self.since && w.finished <= now && !self.settled(w.finished, window, now)).collect()
    }

    fn pending_folded(&self, window: &str, now: f64) -> Option<&Folded> {
        if self.lag <= 0.0 {
            return None;
        }
        self.folded.as_ref().filter(|f| f.finished >= self.since && f.finished <= now && !self.settled(f.finished, window, now))
    }

    /// Drops what every live window has settled; with no live window nothing is settled.
    fn prune(&mut self, now: f64) {
        let live: Vec<String> = self.live_windows(now).into_iter().map(String::from).collect();
        let settled_everywhere = |me: &Self, finished: f64| !live.is_empty() && live.iter().all(|w| me.settled(finished, w, now));
        let keep: Vec<bool> = self.work.iter().map(|w| w.finished >= self.since && !settled_everywhere(self, w.finished)).collect();
        let mut keep = keep.into_iter();
        self.work.retain(|_| keep.next().unwrap_or(true));
        if self.folded.as_ref().is_some_and(|f| f.finished < self.since || settled_everywhere(self, f.finished)) {
            self.folded = None;
        }
        self.fold_overflow();
    }

    /// Too many pending requests to keep one by one: the oldest become one `Folded` that keeps what is known of them (their
    /// points) and the latest end, so it settles last. Nothing pending is ever thrown away.
    fn fold_overflow(&mut self) {
        let extra = self.work.len().saturating_sub(KEEP_WORK);
        if extra == 0 {
            return;
        }
        let folding: Vec<Work> = self.work.drain(..extra).collect();
        let mut f = self.folded.take().unwrap_or(Folded { start: f64::MAX, finished: f64::MIN, points: 0.0, unknown: false });
        for w in &folding {
            f.start = f.start.min(w.start);
            f.finished = f.finished.max(w.finished);
            match self.known_points(w) {
                Some(p) => f.points += p,
                None => f.unknown = true,
            }
        }
        self.folded = Some(f);
    }

    /// The points of the primary window one request is known to have cost, if its cost and its model's conversion are known.
    fn known_points(&self, w: &Work) -> Option<f64> {
        if w.incomplete { None } else { Some(w.cost? * self.weights.raw.get(&w.model)?) }
    }

    /// A request Faber made has just ended: the counter may not hold it yet.
    pub fn worked(&mut self, work: Work, now: f64) {
        self.work.push(work);
        self.prune(now);
    }

    /// Whether any window's reading may still be rising because of Faber's own recent work. It holds whether or not the reading
    /// has moved yet: a counter that has not moved has not counted it either.
    pub fn counting(&self, now: f64) -> bool {
        let live = self.live_windows(now);
        let any = |window: &str| !self.pending(window, now).is_empty() || self.pending_folded(window, now).is_some();
        if live.is_empty() {
            self.lag > 0.0 && (self.work.iter().any(|w| w.finished >= self.since && w.finished <= now) || self.folded.is_some())
        } else {
            live.into_iter().any(any)
        }
    }

    /// The share (0 to 1) that Faber's own work pending for this window is known to add beyond what its reading holds, from the
    /// cost of each request and the points per dollar measured for its model. Work with no cost or no measured conversion adds
    /// nothing to the number but never takes the known part away: the floor only goes up. None when nothing is pending or
    /// none of it can be converted. What the reading already rose by since the work began is taken off only when all of the
    /// work is known (otherwise the rise may be the unknown part), so nothing counts twice and nothing is lost.
    pub fn uncounted(&self, window: &str, now: f64) -> Option<f64> {
        let work = self.pending(window, now);
        let folded = self.pending_folded(window, now);
        if (work.is_empty() && folded.is_none()) || self.primary(now) != Some(window) {
            return None;
        }
        let known: Vec<f64> = work.iter().filter_map(|w| self.known_points(w)).collect();
        let carried = folded.map_or(0.0, |f| f.points);
        if known.is_empty() && carried <= 0.0 {
            return None;
        }
        let all_known = known.len() == work.len() && !folded.is_some_and(|f| f.unknown);
        let begun = work.iter().map(|w| w.start).chain(folded.map(|f| f.start)).fold(f64::MAX, f64::min);
        let reading = self.readings.get(window)?;
        let before = self.history.get(window)?.iter().rev().find(|p| p.at <= begun && p.resets_at == reading.resets_at);
        let risen = if all_known { before.map_or(0.0, |p| ((reading.used - p.used) * 100.0).max(0.0)) } else { 0.0 };
        Some(((known.iter().sum::<f64>() + carried - risen) / 100.0).max(0.0))
    }

    pub fn snap(&self, now: f64) -> Snap {
        let windows = self
            .readings
            .iter()
            .filter(|(_, r)| r.resets_at > now)
            .map(|(name, r)| {
                // A window still counting Faber's own work is a floor: the reading plus what is known to be missing from it.
                let used = (r.used + self.uncounted(name, now).unwrap_or(0.0)).min(1.0).max(r.used);
                (name.clone(), Window { used, resets_at: r.resets_at, read_at: r.read_at, length: window_secs(name).unwrap_or(0.0) })
            })
            .collect();
        Snap {
            counting: self.counting(now),
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

#[cfg(test)]
mod canonical_reset_tests {
    use super::*;

    #[test]
    fn confirmed_reads_share_only_a_current_window_reset() {
        let mut ledger = VendorLedger::default();
        ledger.readings.insert("five_hour".into(), Reading { used: 0.1, resets_at: 1200.0, read_at: 900.0 });
        for (incoming, now, expected) in
            [(1080.0, 1000.0, 1200.0), (1079.0, 1000.0, 1079.0), (990.0, 1000.0, 990.0), (1200.0, 1200.0, 1200.0), (1250.0, 1200.0, 1250.0)]
        {
            assert_eq!(ledger.canonical_reset("five_hour", incoming, now), expected);
        }
        assert_eq!(ledger.canonical_reset("seven_day", 1500.4, 1000.0), 1500.0);
    }
}
