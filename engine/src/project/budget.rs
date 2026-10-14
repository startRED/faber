//! Mission attribution and bounded exposure on the shared Auto ledger. No processes or clock.
use super::{Extra, valid_id};
use crate::budget::{Purpose, STALE_SECS, family_of, gate, vendor_wide, window_secs};
use crate::events::EngineEvent;
use crate::learn::{ENOUGH, Span, stretches};
use crate::ledger::Ledger;
use crate::model::Vendor;
use crate::usage::{self, CostBase};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const KEY: &str = "missionBudget";
/// Spending records evolve independently of the wire/kit/bar contracts.
const SCHEMA: u32 = 1;
const DAY: f64 = 86_400.0;
const WEEK: f64 = 7.0 * DAY;
/// One percentage point covers rounded provider readings.
const GUARD: f64 = 0.01;
const BOOTSTRAP_CAP: u64 = 2_000_000;
const BOOTSTRAP_MARGIN: f64 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    Lead,
    Worker,
    Router,
    Summary,
    Review,
    Repair,
    Canary,
    Integration,
    Diagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub cap_micros: u64,
    pub worker_micros: u64,
    /// Optional aggregate purpose limits survive retries and worker identity changes.
    #[serde(default)]
    pub kind_caps: BTreeMap<Kind, u64>,
    /// Vendor subgrants count inside the mission cap, never on top of it.
    pub vendors: BTreeMap<Vendor, u64>,
    pub review_micros: u64,
    pub repair_micros: u64,
    pub canary_micros: u64,
    pub paused: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invocation {
    pub id: String,
    pub mission: String,
    pub worker: String,
    pub kind: Kind,
    pub vendor: Vendor,
    pub model: String,
    pub effort: String,
    pub session: String,
    pub fresh: bool,
    /// Complete invocation maximum at API-equivalent prices; None denies admission.
    pub exposure_micros: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reservation {
    pub request: Invocation,
    pub started_at: f64,
    pub base: Option<CostBase>,
    pub rates: String,
    pub paid_micros: u64,
    pub settled: bool,
    /// Accounted at an upper bound rather than settled vendor counters; never a calibration sample.
    #[serde(default)]
    pub bounded: bool,
    pub terminated: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Reservation {
    fn charge(&self) -> u64 {
        if self.settled { self.paid_micros } else { self.paid_micros.max(self.request.exposure_micros.unwrap_or(0)) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Allocation {
    pub reset: f64,
    pub starts_at: f64,
    pub cap_micros: u64,
    pub rates: String,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Calibration {
    pub before: BTreeMap<String, crate::learn::Point>,
    pub after: BTreeMap<String, crate::learn::Point>,
    pub costs: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Budget {
    pub schema: u32,
    pub allocations: BTreeMap<Vendor, Allocation>,
    pub grants: BTreeMap<String, Grant>,
    pub invocations: BTreeMap<String, Reservation>,
    pub bases: BTreeMap<String, CostBase>,
    pub disabled: BTreeSet<String>,
    /// Active Chat/auxiliary requests close Projects admission; they never close Chat admission.
    pub unbounded: BTreeMap<Vendor, u64>,
    #[serde(default)]
    pub unbounded_at: BTreeMap<Vendor, f64>,
    pub last_at: f64,
    #[serde(default)]
    pub calibrations: BTreeMap<String, BTreeMap<Vendor, Calibration>>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            allocations: BTreeMap::new(),
            grants: BTreeMap::new(),
            invocations: BTreeMap::new(),
            bases: BTreeMap::new(),
            disabled: BTreeSet::new(),
            unbounded: BTreeMap::new(),
            unbounded_at: BTreeMap::new(),
            last_at: 0.0,
            calibrations: BTreeMap::new(),
            extra: Extra::new(),
        }
    }
}

pub enum Change {
    Allocate { vendor: Vendor, cap_micros: u64 },
    AllocateModel { vendor: Vendor, model: String, cap_micros: u64 },
    Grant { mission: String, grant: Grant },
    Admit(Invocation),
    Continue { id: String },
    Report { id: String, event: EngineEvent, terminated: bool },
    Pause { mission: String, paused: bool },
    Calibrate { mission: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ready,
    Stop(&'static str),
}

fn micros(cost: f64) -> Result<u64, &'static str> {
    let value = (cost * 1e6).ceil();
    if !cost.is_finite() || cost < 0.0 || value >= u64::MAX as f64 {
        return Err("reason.budget.cost");
    }
    Ok(value as u64)
}

/// R117: last settled usage plus one uncached request and the documented maximum output.
/// Unknown CLI-built context uses the entire documented window, never an earlier turn's input count.
/// Sources checked 2026-10-14: developers.openai.com/api/docs/models/gpt-6-luna and
/// platform.claude.com/docs/en/models/overview. No supported universal smaller output cap is proven.
pub fn complete_request_maximum(model: &str, context: Option<u64>, settled_micros: u64) -> Result<u64, &'static str> {
    request_maximum(model, context, settled_micros, 128_000)
}

/// Requires the role launcher's version-bound control proof before dispatch.
pub fn role_request_maximum(model: &str, role: super::Role, context: Option<u64>, settled_micros: u64) -> Result<u64, &'static str> {
    let vendor = match model {
        "gpt-6-luna" => Vendor::Codex,
        "claude-haiku-5-5" => Vendor::Claude,
        _ => return Err("reason.budget.exposure"),
    };
    let request = request_maximum(model, context, 0, super::policy::output_maximum(vendor, role))?;
    let invocation = match vendor {
        // Client budget is checked between requests, so reserve one entire overshooting request.
        // The documented US data-residency premium is bounded at 10%; never assume the incoming geography.
        Vendor::Claude => request
            .checked_mul(11)
            .and_then(|v| v.checked_add(9))
            .map(|v| v / 10)
            // Published worst/base Haiku prices differ by at most 5.5x, including US geography.
            .and_then(|v| v.checked_add(27_500))
            .ok_or("reason.budget.cost")?,
        Vendor::Codex => request,
    };
    settled_micros.checked_add(invocation).ok_or("reason.budget.cost")
}

fn request_maximum(model: &str, context: Option<u64>, settled_micros: u64, output: u64) -> Result<u64, &'static str> {
    let window = match model {
        "gpt-6-luna" => 1_050_000,
        "claude-haiku-5-5" => 1_000_000,
        _ => return Err("reason.budget.exposure"),
    };
    let input = context.unwrap_or(window);
    if input > window {
        return Err("reason.budget.exposure");
    }
    let cost = usage::api_price(model, input, output, 0, 0, 0).ok_or("reason.budget.rate")?;
    settled_micros.checked_add(micros(cost)?).ok_or("reason.budget.cost")
}

fn total(mut values: impl Iterator<Item = u64>) -> Result<u64, &'static str> {
    values.try_fold(0_u64, |sum, n| sum.checked_add(n).ok_or("reason.budget.cost"))
}

fn session_key(r: &Invocation) -> String {
    format!("{}:{}", r.vendor.name(), r.session)
}

fn driver(r: &Invocation) -> String {
    format!("{}:{}:{}", r.vendor.name(), r.model, r.effort)
}

/// The lower measured cost-per-point bound, using the existing complete, non-overlapping stretches.
fn conversion(ledger: &Ledger, vendor: Vendor, window: &str, model: Option<&str>) -> Result<f64, &'static str> {
    let v = ledger.of(vendor);
    let points = v.history.get(window).map(Vec::as_slice).unwrap_or(&[]);
    // Each stretch stays within its own reset; the same plan's measured capacity survives renewal.
    let current: Vec<_> = points.iter().filter(|p| p.at >= v.since).copied().collect();
    let measured = stretches(&current, &v.spans);
    let mut samples: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for s in &measured {
        if model.is_none_or(|name| name == s.model) && s.ppd.is_finite() && s.ppd > 0.0 {
            samples.entry(&s.model).or_default().push(s.ppd);
        }
    }
    let normal = samples.values().filter(|v| v.len() >= ENOUGH).flatten().copied().reduce(f64::max);
    let bootstrap = Budget::load(ledger)?
        .calibrations
        .values()
        .filter_map(|vendors| {
            let sample = vendors.get(&vendor)?;
            if sample.costs.len() != 1 {
                return None;
            }
            let (name, cost) = sample.costs.first_key_value()?;
            let a = sample.before.get(window)?;
            let b = sample.after.get(window)?;
            if model.is_some_and(|m| m != name)
                || *cost == 0
                || a.at < v.since
                || a.resets_at != b.resets_at
                || b.at <= a.at
                || b.used <= a.used
            {
                return None;
            }
            Some((b.used - a.used) * 100.0 / (*cost as f64 / 1e6))
        })
        .reduce(f64::max);
    normal.into_iter().chain(bootstrap).reduce(f64::max).ok_or("reason.budget.calibration")
}

fn weekly(ledger: &Ledger, vendor: Vendor, now: f64) -> Result<(&str, f64, f64), &'static str> {
    let v = ledger.of(vendor);
    let mut windows = v.readings.iter().filter(|(name, r)| vendor_wide(name) && window_secs(name) == Some(WEEK) && r.resets_at > now);
    let (name, reading) = windows.next().ok_or("reason.budget.day")?;
    if windows.next().is_some() {
        return Err("reason.budget.day");
    }
    let start = crate::budget::daily_start(reading.resets_at, now).ok_or("reason.budget.day")?;
    Ok((name, reading.resets_at, start))
}

fn bootstrap(ledger: &Ledger, vendor: Vendor, model: Option<&str>, now: f64) -> Result<bool, &'static str> {
    let v = ledger.of(vendor);
    let (week, reset, start) = weekly(ledger, vendor, now)?;
    let missing = v.readings.iter().any(|(name, r)| {
        r.resets_at > now
            && (vendor_wide(name) || model.and_then(family_of).is_some_and(|f| family_of(name) == Some(f)))
            && conversion(ledger, vendor, name, model).is_err()
    });
    if !missing {
        return Ok(false);
    }
    let first = v.zero_turn_readings.get(week).ok_or("reason.budget.calibration")?;
    let latest = &v.readings[week];
    let line = (start + DAY - (reset - WEEK)) / WEEK;
    if first.resets_at != reset
        || first.read_at <= start
        || first.read_at < v.since.round()
        || !(0.0..=STALE_SECS).contains(&(now - first.read_at))
        || !(0.0..=STALE_SECS).contains(&(now - latest.read_at))
        || !(0.0..=1.0).contains(&first.used)
        || !(first.used..=1.0).contains(&latest.used)
        || line - first.used + f64::EPSILON < BOOTSTRAP_MARGIN
        || line - latest.used + f64::EPSILON < BOOTSTRAP_MARGIN
    {
        return Err("reason.budget.calibration");
    }
    Ok(true)
}

fn headroom(ledger: &Ledger, vendor: Vendor, model: Option<&str>, now: f64) -> Result<u64, &'static str> {
    let v = ledger.of(vendor);
    let snap = v.snap(now);
    gate(&snap, model.and_then(family_of), Purpose::UserTurn { by_hand: false }, false, now).map_err(|_| "reason.budget.quota")?;
    // Unlike a hand-picked Chat turn, automated roles must be unable to fall back to paid use.
    if v.paid_fallback != Some(false) || v.paid_fallback_at.is_none_or(|at| !(0.0..=STALE_SECS).contains(&(now - at))) {
        return Err("reason.budget.billing");
    }
    let (week, reset, start) = weekly(ledger, vendor, now)?;
    let bootstrapping = bootstrap(ledger, vendor, model, now)?;
    if snap.relearning && !bootstrapping {
        return Err("reason.budget.calibration");
    }
    let mut dollars = if let Ok(rate) = conversion(ledger, vendor, week, model) {
        let sample = v.day_baselines.get(week).ok_or("reason.budget.day")?;
        let baseline = &sample.reading;
        let bootstrap_baseline =
            sample.extra.get("bootstrap") == Some(&serde_json::json!(true)) && baseline.at > start && baseline.at <= now;
        if baseline.resets_at != reset
            || (!bootstrap_baseline && (baseline.at > start || !(0.0..=STALE_SECS).contains(&(start - baseline.at))))
        {
            return Err("reason.budget.day");
        }
        let room = crate::budget::daily_room(baseline.used, v.readings[week].used, reset, now).ok_or("reason.budget.day")? - GUARD;
        (room.max(0.0) * 100.0) / rate
    } else {
        BOOTSTRAP_CAP as f64 / 1e6
    };
    if bootstrapping {
        dollars = dollars.min(BOOTSTRAP_CAP as f64 / 1e6);
    }
    for (name, r) in &v.readings {
        if r.resets_at <= now {
            continue;
        }
        let relevant = vendor_wide(name) || model.and_then(family_of).is_some_and(|f| family_of(name) == Some(f));
        if !relevant {
            continue;
        }
        if !(0.0..=STALE_SECS).contains(&(now - r.read_at)) {
            return Err("reason.budget.stale");
        }
        if name != week
            && let Ok(rate) = conversion(ledger, vendor, name, model)
        {
            dollars = dollars.min((crate::budget::ADMIT_LIMIT - r.used - GUARD).max(0.0) * 100.0 / rate);
        }
    }
    // Flooring headroom and ceiling spend cannot manufacture fractional room.
    Ok(if dollars == BOOTSTRAP_CAP as f64 / 1e6 && bootstrapping { BOOTSTRAP_CAP } else { micros(dollars)?.saturating_sub(1) })
}

fn own_today(ledger: &Ledger, vendor: Vendor, week: &str, start: f64, now: f64) -> Result<u64, &'static str> {
    let v = ledger.of(vendor);
    let first = ((start - 14.0 * 3600.0) / DAY).floor() as i64;
    let last = ((now + 14.0 * 3600.0) / DAY).floor() as i64;
    let sample = v
        .day_baselines
        .get(week)
        .or_else(|| {
            v.daily_samples.get(week).filter(|sample| {
                v.zero_turn_readings.get(week).is_some_and(|r| {
                    r.read_at > start
                        && r.read_at == sample.reading.at
                        && r.used == sample.reading.used
                        && r.resets_at == sample.reading.resets_at
                })
            })
        })
        .ok_or("reason.budget.day")?;
    if v.unmetered.iter().any(|(day, n)| *n > sample.unmetered.get(day).copied().unwrap_or(0))
        || sample.unmetered.range(first..=last).any(|(day, n)| *n > v.unmetered.get(day).copied().unwrap_or(0))
    {
        return Err("reason.budget.unknown");
    }
    let baseline = &sample.own_spend;
    if baseline.iter().any(|(day, cost)| v.own_spend.get(day).is_some_and(|current| current < cost))
        || baseline.range(first..=last).any(|(day, cost)| *cost > 0.0 && !v.own_spend.contains_key(day))
    {
        return Err("reason.budget.unknown");
    }
    let delta: f64 = v.own_spend.iter().map(|(day, cost)| cost - baseline.get(day).copied().unwrap_or(0.0)).sum();
    micros(delta)
}

impl Budget {
    pub fn cost_view(&self, mission: &str) -> super::wire::CostView {
        let receipts: Vec<_> = self.invocations.values().filter(|r| r.request.mission == mission).collect();
        let complete = !receipts.is_empty() && receipts.iter().all(|r| r.settled && r.terminated && !r.bounded);
        let own_micros = complete.then(|| receipts.iter().try_fold(0u64, |sum, r| sum.checked_add(r.paid_micros))).flatten();
        let bound_micros =
            receipts.iter().any(|r| r.bounded).then(|| receipts.iter().try_fold(0u64, |sum, r| sum.checked_add(r.charge()))).flatten();
        super::wire::CostView {
            own_micros,
            bound_micros,
            cap_micros: self.grants.get(mission).map(|g| g.cap_micros),
            complete: own_micros.is_some(),
        }
    }

    pub fn load(ledger: &Ledger) -> Result<Self, &'static str> {
        let Some(value) = ledger.extra.get(KEY) else { return Ok(Self::default()) };
        let b: Self = serde_json::from_value(value.clone()).map_err(|_| "error.store.corrupt")?;
        if b.schema != SCHEMA || !b.last_at.is_finite() || b.last_at < 0.0 {
            return Err("error.store.corrupt");
        }
        for grant in b.grants.values() {
            if total(grant.kind_caps.values().copied()).map_err(|_| "error.store.corrupt")? > grant.cap_micros {
                return Err("error.store.corrupt");
            }
        }
        for (mission, vendors) in &b.calibrations {
            if !b.grants.contains_key(mission) {
                return Err("error.store.corrupt");
            }
            for sample in vendors.values() {
                if sample.before.is_empty()
                    || sample.after.keys().any(|window| !sample.before.contains_key(window))
                    || sample.before.values().chain(sample.after.values()).any(|p| {
                        !(0.0..=1.0).contains(&p.used) || !p.at.is_finite() || p.at < 0.0 || !p.resets_at.is_finite() || p.resets_at <= p.at
                    })
                    || sample.after.iter().any(|(name, p)| {
                        p.at <= sample.before[name].at || p.resets_at != sample.before[name].resets_at || p.used < sample.before[name].used
                    })
                    || total(sample.costs.values().copied()).is_err()
                {
                    return Err("error.store.corrupt");
                }
            }
        }
        for (id, r) in &b.invocations {
            if id != &r.request.id
                || !valid_id(id)
                || r.request.exposure_micros.is_none_or(|n| n == 0)
                || (r.bounded && (!r.settled || !r.terminated || r.paid_micros < r.request.exposure_micros.unwrap_or(0)))
                || !r.started_at.is_finite()
                || r.started_at < 0.0
                || !b.grants.contains_key(&r.request.mission)
            {
                return Err("error.store.corrupt");
            }
        }
        if b.unbounded_at.values().any(|at| !at.is_finite() || *at < 0.0)
            || b.allocations.values().any(|a| !a.reset.is_finite() || !a.starts_at.is_finite() || a.reset <= a.starts_at)
            || b.grants.keys().any(|id| !valid_id(id))
        {
            return Err("error.store.corrupt");
        }
        Ok(b)
    }

    fn clock(&self, now: f64) -> Result<(), &'static str> {
        if !now.is_finite() || now < self.last_at { Err("reason.budget.clock") } else { Ok(()) }
    }

    fn charges(&self, mission: &str, worker: Option<&str>, vendor: Option<Vendor>, kind: Option<Kind>) -> Result<u64, &'static str> {
        total(
            self.invocations
                .values()
                .filter(|r| {
                    r.request.mission == mission
                        && worker.is_none_or(|w| r.request.worker == w)
                        && vendor.is_none_or(|v| r.request.vendor == v)
                        && kind.is_none_or(|k| r.request.kind == k)
                })
                .map(Reservation::charge),
        )
    }

    fn commitment(&self, vendor: Vendor) -> Result<u64, &'static str> {
        total(
            self.grants
                .iter()
                .map(|(id, g)| {
                    let cap = g.vendors.get(&vendor).copied().unwrap_or(0);
                    let receipts: Vec<_> =
                        self.invocations.values().filter(|r| r.request.mission == *id && r.request.vendor == vendor).collect();
                    let paid = total(receipts.iter().filter(|r| r.settled).map(|r| r.paid_micros))?;
                    let pending = total(receipts.iter().filter(|r| !r.settled).map(|r| r.charge()))?;
                    // Revoking a cap never frees unsettled exposure.
                    Ok(cap.saturating_sub(paid).max(pending))
                })
                .collect::<Result<Vec<_>, &'static str>>()?
                .into_iter(),
        )
    }

    fn account(&self, ledger: &Ledger, vendor: Vendor, model: Option<&str>, now: f64) -> Result<(), &'static str> {
        self.clock(now)?;
        if self.unbounded.get(&vendor).copied().unwrap_or(0) > 0 {
            return Err("reason.budget.unknown");
        }
        let a = self.allocations.get(&vendor).ok_or("reason.budget.grant")?;
        let (week, reset, starts_at) = weekly(ledger, vendor, now)?;
        if reset != a.reset || starts_at != a.starts_at || a.rates != usage::RATE_DATE {
            return Err("reason.budget.day");
        }
        let room = headroom(ledger, vendor, model, now)?;
        let own = own_today(ledger, vendor, week, starts_at, now)?;
        let held = self.commitment(vendor)?;
        // Partial known usage is already in own_spend but stays inside its still-held exposure.
        let live_paid = total(self.invocations.values().filter(|r| !r.settled && r.request.vendor == vendor).map(|r| r.paid_micros))?;
        if total([own, held.saturating_sub(live_paid)].into_iter())? > a.cap_micros || held.saturating_sub(live_paid) > room {
            return Err("reason.budget.daily");
        }
        Ok(())
    }

    pub fn can_admit(&self, ledger: &Ledger, r: &Invocation, now: f64) -> Result<(), &'static str> {
        self.clock(now)?;
        if !valid_id(&r.id) || !valid_id(&r.mission) || !valid_id(&r.worker) || !valid_id(&r.session) || r.effort.is_empty() {
            return Err("reason.budget.identity");
        }
        if self.invocations.contains_key(&r.id) {
            return Err("reason.budget.duplicate");
        }
        // shortcut: retain at most 4096 invocation receipts; add safe archival before a mission exceeds this history.
        if self.invocations.len() >= 4096 {
            return Err("reason.budget.history");
        }
        if self.invocations.values().any(|old| (!old.settled || old.bounded) && session_key(&old.request) == session_key(r)) {
            return Err("reason.budget.unknown");
        }
        let g = self.grants.get(&r.mission).ok_or("reason.budget.grant")?;
        if g.paused {
            return Err("reason.budget.paused");
        }
        let bound = r.exposure_micros.filter(|n| *n > 0).ok_or("reason.budget.exposure")?;
        if self.disabled.contains(&driver(r)) {
            return Err("reason.budget.driver");
        }
        if !crate::route::PROFILES.iter().any(|p| p.vendor == r.vendor && p.model == r.model && p.effort == r.effort)
            || usage::api_price(&r.model, 1, 1, 0, 0, 0).is_none()
        {
            return Err("reason.budget.rate");
        }
        if bootstrap(ledger, r.vendor, Some(&r.model), now)? && g.cap_micros > BOOTSTRAP_CAP {
            return Err("reason.budget.cap");
        }
        self.account(ledger, r.vendor, Some(&r.model), now)?;
        for vendor in g.vendors.keys().copied().filter(|v| *v != r.vendor) {
            self.account(ledger, vendor, None, now)?;
        }
        let charge = total([self.charges(&r.mission, None, None, None)?, bound].into_iter())?;
        if charge > g.cap_micros
            || total([self.charges(&r.mission, None, None, Some(r.kind))?, bound].into_iter())?
                > if g.kind_caps.is_empty() { g.cap_micros } else { g.kind_caps.get(&r.kind).copied().unwrap_or(0) }
            || total([self.charges(&r.mission, Some(&r.worker), None, None)?, bound].into_iter())? > g.worker_micros
            || total([self.charges(&r.mission, None, Some(r.vendor), None)?, bound].into_iter())?
                > g.vendors.get(&r.vendor).copied().unwrap_or(0)
        {
            return Err("reason.budget.cap");
        }
        let protected = total(
            [(Kind::Review, g.review_micros), (Kind::Repair, g.repair_micros), (Kind::Canary, g.canary_micros)]
                .into_iter()
                .map(|(kind, reserve)| {
                    let used = self.charges(&r.mission, None, None, Some(kind))?;
                    Ok(reserve.saturating_sub(used.saturating_add(if kind == r.kind { bound } else { 0 })))
                })
                .collect::<Result<Vec<_>, &'static str>>()?
                .into_iter(),
        )?;
        if total([charge, protected].into_iter())? > g.cap_micros {
            return Err("reason.budget.reserve");
        }
        Ok(())
    }

    fn continuing(&self, ledger: &Ledger, id: &str, now: f64) -> Result<(), &'static str> {
        let r = self.invocations.get(id).ok_or("reason.budget.identity")?;
        let g = &self.grants[&r.request.mission];
        if r.rates != usage::RATE_DATE {
            return Err("reason.budget.rate");
        }
        if r.settled || r.terminated || g.paused {
            return Err("reason.budget.paused");
        }
        if self.disabled.contains(&driver(&r.request)) {
            return Err("reason.budget.driver");
        }
        self.account(ledger, r.request.vendor, Some(&r.request.model), now)?;
        if self.charges(&r.request.mission, None, None, None)? > g.cap_micros
            || self.charges(&r.request.mission, None, None, Some(r.request.kind))?
                > if g.kind_caps.is_empty() { g.cap_micros } else { g.kind_caps.get(&r.request.kind).copied().unwrap_or(0) }
            || self.charges(&r.request.mission, Some(&r.request.worker), None, None)? > g.worker_micros
            || self.charges(&r.request.mission, None, Some(r.request.vendor), None)?
                > g.vendors.get(&r.request.vendor).copied().unwrap_or(0)
        {
            return Err("reason.budget.cap");
        }
        Ok(())
    }
}

/// Applies one change to a snapshot; the engine saves it atomically under its existing shared lock before effects.
pub fn apply(ledger: &mut Ledger, change: Change, now: f64, offset: i64) -> Result<Status, &'static str> {
    let (change, allocation_model) = match change {
        Change::AllocateModel { vendor, model, cap_micros } => {
            if !crate::route::PROFILES.iter().any(|p| p.vendor == vendor && p.model == model) {
                return Err("reason.budget.rate");
            }
            (Change::Allocate { vendor, cap_micros }, Some(model))
        }
        change => (change, None),
    };
    let mut reconciled = None;
    let mut baseline = None;
    let mut b = Budget::load(ledger)?;
    // Reconciliation still records late spend after a clock reversal; it grants no new work.
    if !matches!(change, Change::Report { .. } | Change::Pause { .. }) {
        b.clock(now)?
    }
    if !now.is_finite() || now < 0.0 {
        return Err("reason.budget.clock");
    }
    let mut status = Status::Ready;
    let admits = !matches!(change, Change::Report { .. } | Change::Pause { .. } | Change::Calibrate { .. });
    match change {
        Change::Allocate { vendor, cap_micros }
            if b.allocations
                .get(&vendor)
                .is_some_and(|a| cap_micros <= a.cap_micros && crate::budget::daily_start(a.reset, now) == Some(a.starts_at)) =>
        {
            b.allocations.get_mut(&vendor).ok_or("reason.budget.grant")?.cap_micros = cap_micros;
        }
        Change::Allocate { vendor, cap_micros } => {
            let (week, reset, starts_at) = weekly(ledger, vendor, now)?;
            let mut current = ledger.clone();
            if bootstrap(ledger, vendor, allocation_model.as_deref(), now)? {
                let v = ledger.of(vendor);
                let sample = v.daily_samples.get(week).ok_or("reason.budget.day")?;
                let first = &v.zero_turn_readings[week];
                if sample.reading.at != first.read_at || sample.reading.used != first.used || sample.reading.resets_at != reset {
                    return Err("reason.budget.calibration");
                }
                let mut sample = sample.clone();
                sample.extra.insert("bootstrap".into(), serde_json::json!(true));
                if ledger.of(vendor).day_baselines.get(week).is_none_or(|old| old.reading.resets_at != reset || old.reading.at < starts_at)
                {
                    current.of_mut(vendor).day_baselines.insert(week.to_string(), sample.clone());
                    baseline = Some((vendor, week.to_string(), sample));
                }
            }
            let room = headroom(&current, vendor, allocation_model.as_deref(), now)?;
            let own = own_today(&current, vendor, week, starts_at, now)?;
            if cap_micros == 0 || cap_micros > total([own, room].into_iter())? || b.unbounded.get(&vendor).copied().unwrap_or(0) > 0 {
                return Err("reason.budget.daily");
            }
            b.allocations.insert(
                vendor,
                Allocation {
                    reset,
                    starts_at,
                    cap_micros,
                    rates: usage::RATE_DATE.into(),
                    extra: b.allocations.get(&vendor).map(|a| a.extra.clone()).unwrap_or_default(),
                },
            );
            b.account(&current, vendor, allocation_model.as_deref(), now)?;
        }
        Change::Grant { mission, mut grant } => {
            if !valid_id(&mission)
                || grant.vendors.is_empty()
                || grant.worker_micros > grant.cap_micros
                || total(grant.kind_caps.values().copied())? > grant.cap_micros
                || total(grant.vendors.values().copied())? != grant.cap_micros
                || total([grant.review_micros, grant.repair_micros, grant.canary_micros].into_iter())? > grant.cap_micros
            {
                return Err("reason.budget.grant");
            }
            if grant.cap_micros > BOOTSTRAP_CAP && grant.vendors.keys().any(|v| bootstrap(ledger, *v, None, now).unwrap_or(true)) {
                return Err("reason.budget.cap");
            }
            if !b.grants.contains_key(&mission) {
                let mut samples = BTreeMap::new();
                for vendor in grant.vendors.keys().copied() {
                    let needs_sample = bootstrap(ledger, vendor, None, now)?;
                    let start = weekly(ledger, vendor, now)?.2;
                    let fresh_sample = ledger
                        .of(vendor)
                        .zero_turn_readings
                        .values()
                        .any(|r| r.read_at > start && (0.0..=STALE_SECS).contains(&(now - r.read_at)) && r.resets_at > now);
                    // A grant precedes model selection; preserve real endpoints for a new model too.
                    if needs_sample || fresh_sample {
                        let v = ledger.of(vendor);
                        let before = v
                            .zero_turn_readings
                            .iter()
                            .filter(|(_, r)| r.read_at > start && (0.0..=STALE_SECS).contains(&(now - r.read_at)) && r.resets_at > now)
                            .map(|(name, r)| (name.clone(), crate::learn::Point { at: r.read_at, used: r.used, resets_at: r.resets_at }))
                            .collect();
                        samples.insert(vendor, Calibration { before, ..Default::default() });
                    }
                }
                if !samples.is_empty() {
                    b.calibrations.insert(mission.clone(), samples);
                }
            }
            if let Some(old) = b.grants.get(&mission) {
                let increased: Vec<_> =
                    grant.vendors.iter().filter(|(v, cap)| **cap > old.vendors.get(v).copied().unwrap_or(0)).map(|(v, _)| *v).collect();
                grant.extra.extend(old.extra.clone());
                grant.paused = old.paused;
                b.grants.insert(mission, grant);
                for vendor in increased {
                    b.account(ledger, vendor, None, now)?
                }
                // Smaller live caps close future admission without erasing receipts or refusing revocation.
            } else {
                let vendors: Vec<_> = grant.vendors.keys().copied().collect();
                b.grants.insert(mission, grant);
                for vendor in vendors {
                    b.account(ledger, vendor, None, now)?
                }
            }
        }
        Change::Admit(request) => {
            b.can_admit(ledger, &request, now)?;
            if bootstrap(ledger, request.vendor, Some(&request.model), now)?
                && !b.calibrations.get(&request.mission).is_some_and(|samples| samples.contains_key(&request.vendor))
            {
                let start = weekly(ledger, request.vendor, now)?.2;
                let before = ledger
                    .of(request.vendor)
                    .zero_turn_readings
                    .iter()
                    .filter(|(_, r)| r.read_at > start && (0.0..=STALE_SECS).contains(&(now - r.read_at)) && r.resets_at > now)
                    .map(|(name, r)| (name.clone(), crate::learn::Point { at: r.read_at, used: r.used, resets_at: r.resets_at }))
                    .collect();
                b.calibrations
                    .entry(request.mission.clone())
                    .or_default()
                    .insert(request.vendor, Calibration { before, ..Default::default() });
            }
            let base = b.bases.get(&session_key(&request)).cloned();
            b.invocations.insert(
                request.id.clone(),
                Reservation {
                    request,
                    base,
                    rates: usage::RATE_DATE.into(),
                    started_at: now,
                    paid_micros: 0,
                    settled: false,
                    bounded: false,
                    terminated: false,
                    extra: Extra::new(),
                },
            );
        }
        Change::Continue { id } => b.continuing(ledger, &id, now)?,
        Change::Pause { mission, paused } => {
            b.grants.get_mut(&mission).ok_or("reason.budget.grant")?.paused = paused;
        }
        Change::Calibrate { mission } => {
            let samples = b.calibrations.get_mut(&mission).ok_or("reason.budget.calibration")?;
            for (vendor, sample) in samples {
                if !sample.after.is_empty() {
                    continue;
                }
                let receipts: Vec<_> =
                    b.invocations.values().filter(|r| r.request.mission == mission && r.request.vendor == *vendor).collect();
                if receipts.is_empty() || receipts.iter().any(|r| !r.settled || !r.terminated || r.bounded) {
                    return Err("reason.budget.unknown");
                }
                let v = ledger.of(*vendor);
                let end = receipts.iter().try_fold(0.0_f64, |end, r| {
                    let span = v
                        .spans
                        .iter()
                        .find(|s| {
                            s.start == r.started_at.round()
                                && s.model == r.request.model
                                && !s.incomplete
                                && s.cost == Some(r.paid_micros as f64 / 1e6)
                        })
                        .ok_or("reason.budget.calibration")?;
                    Ok::<_, &'static str>(end.max(span.end))
                })?;
                for (name, before) in &sample.before {
                    let after = v.zero_turn_readings.get(name).ok_or("reason.budget.calibration")?;
                    if after.read_at < end
                        || after.read_at <= before.at
                        || !(0.0..=STALE_SECS).contains(&(now - after.read_at))
                        || after.resets_at != before.resets_at
                        || !(before.used..=1.0).contains(&after.used)
                    {
                        return Err("reason.budget.calibration");
                    }
                    sample
                        .after
                        .insert(name.clone(), crate::learn::Point { at: after.read_at, used: after.used, resets_at: after.resets_at });
                }
                for r in receipts {
                    let cost = sample.costs.entry(r.request.model.clone()).or_default();
                    *cost = cost.checked_add(r.paid_micros).ok_or("reason.budget.cost")?;
                }
            }
        }
        Change::Report { id, event, terminated } => {
            let r = b.invocations.get_mut(&id).ok_or("reason.budget.identity")?;
            if r.rates != usage::RATE_DATE {
                return Ok(Status::Stop("reason.budget.rate"));
            }
            let was_settled = r.settled;
            r.terminated |= terminated;
            let trusted = ledger.of(r.request.vendor).readings.values().map(|r| r.read_at).fold(b.last_at.max(r.started_at), f64::max);
            let at = now.max(b.last_at).max(r.started_at).min(trusted + STALE_SECS);
            if r.bounded {
                // A late or duplicate result cannot turn a conservative charge into measured spend.
                let late = match &event {
                    EngineEvent::Done { cost_total_usd: Some(total), .. } if r.request.vendor == Vendor::Claude => {
                        usage::settle(r.base.as_ref(), &r.request.session, r.request.fresh, Some(*total)).cost
                    }
                    _ => usage::fresh_price(&event, &r.request.model).0,
                };
                if let Some(cost) = late {
                    let paid = micros(cost)?;
                    if paid > r.paid_micros {
                        ledger.of_mut(r.request.vendor).spend(Some((paid - r.paid_micros) as f64 / 1e6), false, at, offset);
                        r.paid_micros = paid;
                        b.disabled.insert(driver(&r.request));
                        status = Status::Stop("reason.budget.breach");
                    }
                }
                let value = serde_json::to_value(&b).map_err(|_| "error.store.corrupt")?;
                ledger.extra.insert(KEY.into(), value);
                return Ok(status);
            }
            let cost_total_usd = match &event {
                EngineEvent::Done { cost_total_usd, .. } => *cost_total_usd,
                EngineEvent::Error { .. } => None,
                _ => return Err("reason.budget.cost"),
            };
            let (mut cost, mut incomplete) = usage::fresh_price(&event, &r.request.model);
            let cumulative = r.request.vendor == Vendor::Claude && cost_total_usd.is_some();
            if cumulative {
                // The session high-water mark prevents a late receipt overlapping a newer invocation's counters.
                let settled = usage::settle(
                    b.bases.get(&session_key(&r.request)).or(r.base.as_ref()),
                    &r.request.session,
                    r.request.fresh,
                    cost_total_usd,
                );
                cost = settled.cost;
                incomplete = cost.is_none();
                if cost.is_some()
                    && let Some(base) = settled.base
                {
                    b.bases.insert(session_key(&r.request), base);
                }
            }
            if let Some(cost) = cost {
                let paid = if cumulative { total([r.paid_micros, micros(cost)?].into_iter())? } else { micros(cost)? };
                if paid < r.paid_micros {
                    b.disabled.insert(driver(&r.request));
                    status = Status::Stop("reason.budget.breach");
                } else {
                    let delta = paid - r.paid_micros;
                    // Fresh quota extends trusted time for delayed receipts; a clock glitch alone cannot.
                    let v = reconciled.get_or_insert_with(|| (r.request.vendor, ledger.of(r.request.vendor).clone()));
                    v.1.spend(Some(delta as f64 / 1e6), false, at, offset);
                    r.paid_micros = paid;
                    if was_settled && delta > 0 {
                        b.disabled.insert(driver(&r.request));
                        status = Status::Stop("reason.budget.breach");
                        r.settled = terminated && !incomplete;
                    }
                    if !incomplete && terminated && !was_settled {
                        r.settled = true;
                        let v = &mut v.1;
                        v.span(Span {
                            start: r.started_at,
                            end: at,
                            model: r.request.model.clone(),
                            cost: Some(paid as f64 / 1e6),
                            incomplete: false,
                        });
                        v.learn_weights(at);
                    }
                }
            }
            if r.paid_micros > r.request.exposure_micros.unwrap_or(0) || b.disabled.contains(&driver(&r.request)) {
                b.disabled.insert(driver(&r.request));
                status = Status::Stop("reason.budget.breach");
            } else if !r.settled && r.terminated && status == Status::Ready {
                let charge =
                    r.paid_micros.checked_add(r.request.exposure_micros.ok_or("reason.budget.exposure")?).ok_or("reason.budget.cost")?;
                let delta = charge - r.paid_micros;
                let v = reconciled.get_or_insert_with(|| (r.request.vendor, ledger.of(r.request.vendor).clone()));
                v.1.spend(Some(delta as f64 / 1e6), false, at, offset);
                r.paid_micros = charge;
                r.bounded = true;
                r.settled = true;
                if charge > r.request.exposure_micros.unwrap_or(0) {
                    b.disabled.insert(driver(&r.request));
                    status = Status::Stop("reason.budget.breach");
                }
            }
        }
        Change::AllocateModel { .. } => unreachable!("normalized allocation"),
    }
    if admits {
        b.last_at = b.last_at.max(now);
    }
    let value = serde_json::to_value(&b).map_err(|_| "error.store.corrupt")?;
    if let Some((vendor, v)) = reconciled {
        *ledger.of_mut(vendor) = v;
    }
    if let Some((vendor, week, sample)) = baseline {
        ledger.of_mut(vendor).day_baselines.insert(week, sample);
    }
    ledger.extra.insert(KEY.into(), value);
    Ok(status)
}

/// Update only active-request metadata on Chat's hot path; mission receipts belong to Projects.
pub fn start_unbounded(ledger: &mut Ledger, vendor: Vendor, now: f64) -> Result<(), &'static str> {
    if !now.is_finite() || now < 0.0 {
        return Err("reason.budget.clock");
    }
    if !ledger.extra.contains_key(KEY) {
        ledger.extra.insert(KEY.into(), serde_json::to_value(Budget::default()).map_err(|_| "error.store.corrupt")?);
    }
    let b = ledger.extra.get_mut(KEY).and_then(serde_json::Value::as_object_mut).ok_or("error.store.corrupt")?;
    let active = b.get("unbounded").and_then(serde_json::Value::as_object).ok_or("error.store.corrupt")?;
    let n = active.get(vendor.name()).map_or(Ok(0), |v| v.as_u64().ok_or("error.store.corrupt"))?;
    let count = n.checked_add(1).ok_or("reason.budget.cost")?;
    let dates = b.entry("unboundedAt").or_insert_with(|| serde_json::json!({})).as_object_mut().ok_or("error.store.corrupt")?;
    dates.insert(vendor.name().into(), serde_json::json!(now));
    b["unbounded"][vendor.name()] = serde_json::json!(count);
    Ok(())
}

pub fn finish_unbounded(ledger: &mut Ledger, vendor: Vendor) -> Result<(), &'static str> {
    let Some(b) = ledger.extra.get_mut(KEY) else { return Ok(()) };
    let active = b.get_mut("unbounded").and_then(serde_json::Value::as_object_mut).ok_or("error.store.corrupt")?;
    let n = active.get(vendor.name()).map_or(Ok(0), |v| v.as_u64().ok_or("error.store.corrupt"))?;
    active.insert(vendor.name().into(), serde_json::json!(n.saturating_sub(1)));
    Ok(())
}

/// A persisted request from a dead engine is uncertain spend, not a permanently running process.
pub fn recover_unbounded(ledger: &mut Ledger) -> Result<bool, &'static str> {
    let b = Budget::load(ledger)?;
    let mut changed = false;
    for (vendor, n) in b.unbounded {
        if n == 0 {
            continue;
        }
        let at = b
            .unbounded_at
            .get(&vendor)
            .copied()
            .unwrap_or_else(|| ledger.of(vendor).readings.values().map(|r| r.read_at).reduce(f64::max).unwrap_or(b.last_at));
        ledger.of_mut(vendor).spend(None, true, at, 0);
        let count = ledger.extra.get_mut(KEY).ok_or("error.store.corrupt")?;
        count["unbounded"][vendor.name()] = serde_json::json!(0);
        changed = true;
    }
    Ok(changed)
}
