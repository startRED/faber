//! What Faber learns about an account from the readings it takes: when the week's use happens, how much of it came from
//! outside Faber, and how much quota each model really takes. Pure functions over plain records; the engine feeds them.

use crate::budget::{Shape, bin};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Two readings of one window further apart than this say nothing about when the use happened.
pub const GAP_MAX: f64 = 12.0 * 3600.0;
/// Outside movement under this many points is ignored: one point is the resolution of a reading.
pub const OUTSIDE_MIN: f64 = 2.0;
/// A stretch must have moved the window this many points to count toward a weight.
pub const STRETCH_MIN: f64 = 3.0;
/// Stretches kept per model.
pub const KEEP: usize = 20;
/// Stretches a model needs before its weight is anything but 1.
pub const ENOUGH: usize = 8;
/// A vendor's capacity moving by more than this factor is a plan change.
pub const CAPACITY_JUMP: f64 = 2.0;
const WEEK: f64 = 7.0 * 86_400.0;
const COVER_FULL: f64 = 336.0 * 3600.0;

/// One reading of one window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Unix seconds.
    pub at: f64,
    /// 0 to 1.
    pub used: f64,
    pub resets_at: f64,
}

/// A span in which Faber itself was using the vendor: a user turn, a router question, a pulse or a summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: f64,
    pub end: f64,
    pub model: String,
    /// What the span cost in the vendor's own unit (US dollars for Claude, credits for Codex); None when not known.
    pub cost: Option<f64>,
}

/// Movement in points between two readings of one window, when they say something: the same reset, a gap of at most 12
/// hours, and use that did not go down. Rounded to a thousandth of a point: readings are whole points held as fractions,
/// and 0.12 - 0.10 is a hair under 2 points in floating point.
fn movement(a: &Point, b: &Point) -> Option<f64> {
    let dt = b.at - a.at;
    (a.resets_at == b.resets_at && dt > 0.0 && dt <= GAP_MAX && b.used >= a.used).then(|| ((b.used - a.used) * 100_000.0).round() / 1000.0)
}

fn overlaps(span: &Span, a: f64, b: f64) -> bool {
    span.start < b && span.end > a
}

/// The usage profile as it is learned: where in the week the movement of a window happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Learned {
    /// Points moved in each weekday-hour bin.
    pub mass: Vec<f64>,
    /// Seconds of accepted gaps.
    pub covered: f64,
}

impl Default for Learned {
    fn default() -> Self {
        Learned { mass: vec![0.0; 168], covered: 0.0 }
    }
}

impl Learned {
    /// Adds the movement between two consecutive readings of one window, spread over the hours the gap overlaps in
    /// proportion to the seconds in each; it is never put on one hour.
    pub fn add(&mut self, a: &Point, b: &Point, offset: i64) {
        let Some(d) = movement(a, b) else { return };
        let dt = b.at - a.at;
        let mut t = a.at;
        while t < b.at {
            let hour_end = ((t + offset as f64) / 3600.0).floor() + 1.0;
            let end = (hour_end * 3600.0 - offset as f64).max(t + 1.0).min(b.at);
            self.mass[bin(t, offset)] += d * (end - t) / dt;
            t = end;
        }
        self.covered += dt;
    }

    /// The shape in use: uniform until there is data, then more and more of what was learned, all of it after two weeks.
    /// Every bin starts with a prior of one average hour, so an hour never seen is not read as an hour of no use.
    pub fn shape(&self) -> Shape {
        let total: f64 = self.mass.iter().sum();
        if self.covered <= 0.0 || total <= 0.0 {
            return Shape::uniform();
        }
        let average_hour = total / (self.covered / 3600.0);
        let sum = total + average_hour * 168.0;
        let c = (self.covered / COVER_FULL).min(1.0);
        let mut shape = [0.0; 168];
        for (i, slot) in shape.iter_mut().enumerate() {
            *slot = (1.0 - c) / 168.0 + c * (self.mass[i] + average_hour) / sum;
        }
        Shape(shape)
    }
}

/// What the readings say about use from outside Faber over the last seven days.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Outside {
    /// Points moved in gaps with no Faber turn in them (gaps under two points ignored): a lower bound.
    pub outside: f64,
    /// Points moved in all the gaps that said something.
    pub observed: f64,
    /// Hours of the gaps with no Faber turn in them.
    pub quiet_hours: f64,
    /// Outside movement in the last two hours.
    pub recent: f64,
    /// The average outside rate before those two hours, in points per hour.
    pub rate: f64,
}

impl Outside {
    /// The share of the observed movement that came from outside, for the screen ("at least"). None with nothing observed.
    pub fn share(&self) -> Option<f64> {
        (self.observed > 0.0).then(|| (self.outside / self.observed).min(1.0))
    }

    /// Use from outside just jumped: more than 3 points in the last two hours and more than 3 times the average rate;
    /// with no history any 3 points count.
    pub fn jumped(&self) -> bool {
        self.recent > 3.0 && (self.rate <= 0.0 || self.recent / 2.0 > 3.0 * self.rate)
    }
}

pub fn outside(points: &[Point], spans: &[Span], now: f64) -> Outside {
    let mut o = Outside::default();
    let (mut quiet_before, mut outside_before) = (0.0, 0.0);
    for pair in points.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if b.at < now - WEEK || b.at > now {
            continue;
        }
        let Some(d) = movement(a, b) else { continue };
        o.observed += d;
        if spans.iter().any(|s| overlaps(s, a.at, b.at)) {
            continue;
        }
        let hours = (b.at - a.at) / 3600.0;
        o.quiet_hours += hours;
        if d < OUTSIDE_MIN {
            if b.at <= now - 7200.0 {
                quiet_before += hours;
            }
            continue;
        }
        o.outside += d;
        let recent_share = ((b.at - a.at.max(now - 7200.0)) / (b.at - a.at)).clamp(0.0, 1.0);
        o.recent += d * recent_share;
        outside_before += d * (1.0 - recent_share);
        quiet_before += hours * (1.0 - recent_share);
    }
    o.rate = if quiet_before > 0.0 { outside_before / quiet_before } else { 0.0 };
    o
}

/// One stretch of Faber's own use that moved a window enough to measure: points of the window per unit of cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stretch {
    pub model: String,
    pub ppd: f64,
}

/// Stretches in a window's readings. A stretch is a pair of consecutive readings with, between them, Faber turns of one
/// model that do not overlap each other, with known costs, the window moving at least 3 points, and no outside movement
/// of 2 points or more in the gaps on either side (so use from elsewhere cannot hide in it).
pub fn stretches(points: &[Point], spans: &[Span]) -> Vec<Stretch> {
    let outside_moved =
        |a: &Point, b: &Point| movement(a, b).is_some_and(|d| d >= OUTSIDE_MIN) && !spans.iter().any(|s| overlaps(s, a.at, b.at));
    let mut found = vec![];
    for i in 0..points.len().saturating_sub(1) {
        let (a, b) = (&points[i], &points[i + 1]);
        let Some(d) = movement(a, b) else { continue };
        if d < STRETCH_MIN {
            continue;
        }
        let inside: Vec<&Span> = spans.iter().filter(|s| overlaps(s, a.at, b.at)).collect();
        let Some(first) = inside.first() else { continue };
        let one_model = inside.iter().all(|s| s.model == first.model);
        let apart = inside.iter().enumerate().all(|(j, s)| inside.iter().skip(j + 1).all(|t| s.end <= t.start || t.end <= s.start));
        let cost: Option<f64> = inside.iter().map(|s| s.cost).sum();
        let before = i > 0 && outside_moved(&points[i - 1], a);
        let after = i + 2 < points.len() && outside_moved(b, &points[i + 2]);
        if let (true, true, Some(cost), false, false) = (one_model, apart, cost, before, after)
            && cost > 0.0
        {
            found.push(Stretch { model: first.model.clone(), ppd: d / cost });
        }
    }
    found
}

/// What the stretches say about a vendor's models.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Weights {
    /// The weight of each model in its vendor's quota: raw points per cost over the vendor's median, 0.5 to 2. A model
    /// with too few stretches, or an unreliable estimate, has no entry and counts as 1.
    pub w: BTreeMap<String, f64>,
    /// The median raw points per cost over the models: the size of the plan in one number. Kept before normalising, so a
    /// plan four times larger shows.
    pub capacity: Option<f64>,
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v
}

fn quartile(v: &[f64]) -> f64 {
    v[(v.len() - 1) / 4]
}

fn median(v: &[f64]) -> f64 {
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

/// `stretches` are oldest first. Outside use can only add points, so the lower quartile of the last 20 is kept, not the
/// median; when the median is more than twice the lower quartile the estimate is unreliable and the model keeps 1.
pub fn weights(stretches: &[Stretch]) -> Weights {
    let mut per_model: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for s in stretches {
        per_model.entry(&s.model).or_default().push(s.ppd);
    }
    let mut raw: BTreeMap<String, f64> = BTreeMap::new();
    for (model, values) in per_model {
        let last = &values[values.len().saturating_sub(KEEP)..];
        if last.len() < ENOUGH {
            continue;
        }
        let v = sorted(last.to_vec());
        let low = quartile(&v);
        if low > 0.0 && median(&v) <= 2.0 * low {
            raw.insert(model.to_string(), low);
        }
    }
    if raw.is_empty() {
        return Weights::default();
    }
    let capacity = median(&sorted(raw.values().copied().collect()));
    let w = raw.into_iter().map(|(m, r)| (m, (r / capacity).clamp(0.5, 2.0))).collect();
    Weights { w, capacity: Some(capacity) }
}

/// Whether a vendor's capacity moved enough to be a different plan.
pub fn capacity_changed(old: f64, new: f64) -> bool {
    old > 0.0 && new > 0.0 && (new / old > CAPACITY_JUMP || old / new > CAPACITY_JUMP)
}
