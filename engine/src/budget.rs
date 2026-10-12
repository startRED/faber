//! Quota arithmetic: how fast a window is being used against the time it has left, what that does to a model's price, and
//! whether a vendor may be asked at all. Pure functions: no clock, no process, no disk. Every constant is a starting value
//! that the simulation (engine/tests/simulation.rs) is allowed to tune.

/// The safe share of a window to reach at its reset is `1 - MARGIN` scaled by the time left (see `m_of`).
pub const MARGIN: f64 = 0.3;
/// A window counts as at least this much elapsed, so a burst in its first minutes is not a runaway.
pub const ELAPSED_FLOOR: f64 = 0.10;
pub const M_MIN: f64 = 0.1;
pub const M_MAX: f64 = 25.0;
/// A reading older than this is not believed when it says there is quota to spare.
pub const STALE_SECS: f64 = 6.0 * 3600.0;
/// A step up is bought only in a vendor whose windows are all at or below this share.
pub const STEP_LIMIT: f64 = 0.90;
/// No request goes to a vendor with a window above this share.
pub const ADMIT_LIMIT: f64 = 0.98;
/// A window at or above this share is out.
pub const OUT: f64 = 0.99;
/// A message waits for quota only when the wait is this long or shorter.
pub const QUEUE_SECS: f64 = 6.0 * 3600.0;
/// `m` at or above this is "running low" on the screen.
pub const LOW_M: f64 = 4.0;
/// `m` at or below this is "plenty left" on the screen.
pub const SPARE_M: f64 = 0.6;

/// The price multiplier of one window: `p` is the share expected to be used at the reset, `left` the share of the
/// window still ahead. `p` of 0.7 is the pivot with a full window left (price 1); a window projected far under the pivot
/// is cheap, one projected over it is dear, and one on the daily line costs 25, with a gradient above the line.
pub fn m_of(p: f64, left: f64) -> f64 {
    if p.is_nan() {
        return 1.0;
    }
    if p >= 1.0 {
        return M_MAX * p.min(15.0);
    }
    let margin = MARGIN * left.clamp(0.0, 1.0).sqrt();
    (margin / (1.0 - p.max(0.0))).powi(2).clamp(M_MIN, M_MAX)
}

/// The length of a window, from its name. Claude names its windows; Codex reports `<n>_minutes`. None for a name Faber
/// does not know, which then gives no information (price 1) but still counts for exhaustion.
pub fn window_secs(name: &str) -> Option<f64> {
    if name.starts_with("five_hour") {
        Some(18_000.0)
    } else if name.starts_with("seven_day") {
        Some(604_800.0)
    } else {
        name.strip_suffix("_minutes")?.parse::<f64>().ok().filter(|m| *m > 0.0).map(|m| m * 60.0)
    }
}

/// Windows that cover the whole vendor; every other window is a family window.
pub fn vendor_wide(name: &str) -> bool {
    matches!(name, "five_hour" | "seven_day") || name.strip_suffix("_minutes").is_some_and(|n| n.parse::<u64>().is_ok())
}

/// The model family a family window belongs to, or the family of a Claude model id.
pub fn family_of(name: &str) -> Option<&'static str> {
    ["opus", "sonnet", "haiku"].into_iter().find(|f| name.contains(f))
}

/// How a week of use is spread over the hours of the week: weekday x hour in the PC's local time, Monday 00:00 first,
/// summing to 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape(pub [f64; 168]);

impl Shape {
    pub fn uniform() -> Shape {
        Shape([1.0 / 168.0; 168])
    }

    /// The shape's mass between two instants (Unix seconds). `offset` is the local time's distance from UTC in seconds.
    pub fn mass(&self, from: f64, to: f64, offset: i64) -> f64 {
        if !(from.is_finite() && to.is_finite()) || to <= from {
            return 0.0;
        }
        let to = to.min(from + 31.0 * 86_400.0);
        let (mut t, mut sum) = (from, 0.0);
        while t < to {
            let hour_end = ((t + offset as f64) / 3600.0).floor() + 1.0;
            let end = (hour_end * 3600.0 - offset as f64).max(t + 1.0).min(to);
            sum += self.0[bin(t, offset)] * (end - t) / 3600.0;
            t = end;
        }
        sum
    }
}

/// The weekday-hour bin of an instant: Monday 00:00 is 0 and Sunday 23:00 is 167.
pub fn bin(t: f64, offset: i64) -> usize {
    let local = (t + offset as f64).floor() as i64;
    let weekday = (local.div_euclid(86_400) + 3).rem_euclid(7); // 1 January 1970 was a Thursday
    (weekday * 24 + local.rem_euclid(86_400) / 3600) as usize
}

/// The share of a window already passed, weighted by the shape (so a quiet Sunday does not count as time gone).
pub fn elapsed(shape: &Shape, start: f64, now: f64, end: f64, offset: i64) -> f64 {
    let total = shape.mass(start, end, offset);
    if total <= 0.0 {
        return if end > start { ((now - start) / (end - start)).clamp(0.0, 1.0) } else { 1.0 };
    }
    (shape.mass(start, now.min(end), offset) / total).clamp(0.0, 1.0)
}

/// The latest reading of one quota window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    /// 0 to 1.
    pub used: f64,
    /// Unix seconds; a window whose reset has passed is ignored.
    pub resets_at: f64,
    /// When Faber read it, Unix seconds.
    pub read_at: f64,
    /// Seconds; 0 when the window's name says nothing about it.
    pub length: f64,
}

impl Window {
    fn live(&self, now: f64) -> bool {
        self.used.is_finite() && self.resets_at > now
    }
}

/// The multiplier of one window. 1 when there is nothing to go on; never below 1 when `discount_ok` is false or the
/// reading is older than six hours (or from the future, which a clock set backwards makes look like).
pub fn window_m(w: &Window, now: f64, shape: &Shape, offset: i64, discount_ok: bool) -> f64 {
    if !w.live(now) || w.length <= 0.0 {
        return 1.0;
    }
    let e = elapsed(shape, w.resets_at - w.length, now, w.resets_at, offset);
    let m = m_of(w.used / e.max(ELAPSED_FLOOR), 1.0 - e);
    if discount_ok && (0.0..=STALE_SECS).contains(&(now - w.read_at)) { m } else { m.max(1.0) }
}

/// Whether the login is a subscription. Auto requires a confirmed subscription even before the first quota reading.
/// An unknown or usage-billed login is available only for a hand-picked user turn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Billing {
    Subscription,
    UsageBilled,
    #[default]
    Unknown,
}

/// Whether the CLI itself said the vendor cannot answer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Reach {
    #[default]
    Clear,
    /// Out; the reset time in Unix seconds when the CLI gave one.
    Out(Option<f64>),
}

/// Everything the router and the gate know about one vendor at one moment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snap {
    pub windows: Vec<(String, Window)>,
    pub billing: Billing,
    pub reach: Reach,
    /// The CLI says it is spending paid extra usage now.
    pub overage_active: bool,
    /// The CLI may switch to paid use by itself (extra usage on, or credits available).
    pub paid_fallback: bool,
    /// Use outside Faber jumped in the last two hours.
    pub jumped: bool,
    /// The plan changed and the weights are being learned again.
    pub relearning: bool,
    /// A plan was detected less than 30 minutes ago, or the user typed one that no detection contradicts.
    pub plan_ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Spare,
    Normal,
    Tight,
}

impl Snap {
    fn wide(&self, now: f64) -> impl Iterator<Item = &Window> {
        self.windows.iter().filter(move |(n, w)| vendor_wide(n) && w.live(now)).map(|(_, w)| w)
    }

    /// Discounts (a multiplier below 1) are believed only while nothing makes the reading doubtful.
    pub fn discount_ok(&self) -> bool {
        self.plan_ok && !self.jumped && !self.relearning && self.billing == Billing::Subscription
    }

    /// The highest multiplier over the vendor-wide windows; 1 with no live reading.
    pub fn vendor_m(&self, shape: &Shape, offset: i64, now: f64) -> f64 {
        let ok = self.discount_ok();
        self.windows
            .iter()
            .filter(|(n, w)| vendor_wide(n) && w.live(now))
            .map(|(_, w)| window_m(w, now, shape, offset, ok))
            .reduce(f64::max)
            .unwrap_or(1.0)
    }

    /// The multiplier of a profile: its vendor's, or its family window's when that is higher.
    pub fn profile_m(&self, family: Option<&str>, shape: &Shape, offset: i64, now: f64) -> f64 {
        let ok = self.discount_ok();
        let own = self
            .windows
            .iter()
            .filter(|(n, w)| w.live(now) && !vendor_wide(n) && family.is_some() && family_of(n) == family)
            .map(|(_, w)| window_m(w, now, shape, offset, ok))
            .reduce(f64::max);
        self.vendor_m(shape, offset, now).max(own.unwrap_or(0.0))
    }

    /// True when every vendor-wide window is at or below the step limit.
    pub fn step_ok(&self, now: f64) -> bool {
        self.wide(now).all(|w| w.used <= STEP_LIMIT)
    }

    fn top(&self, now: f64) -> f64 {
        self.wide(now).map(|w| w.used).fold(0.0, f64::max)
    }

    /// The vendor cannot answer at all: a vendor-wide window is out, the CLI says so, or paid extra usage is running.
    pub fn exhausted(&self, now: f64) -> bool {
        self.wide(now).any(|w| w.used >= OUT)
            || self.overage_active
            || match self.reach {
                Reach::Out(Some(until)) => until > now,
                Reach::Out(None) => true,
                Reach::Clear => false,
            }
    }

    /// A model family is out while the rest of the vendor is not.
    pub fn family_out(&self, family: &str, now: f64) -> bool {
        self.windows.iter().any(|(n, w)| !vendor_wide(n) && family_of(n) == Some(family) && w.live(now) && w.used >= OUT)
    }

    /// May a profile of this family be chosen: its vendor is not exhausted, its family is not out, and no vendor-wide
    /// window is above the admission limit. Paid credits never make anything usable.
    pub fn usable(&self, family: Option<&str>, now: f64) -> bool {
        !self.exhausted(now) && !family.is_some_and(|f| self.family_out(f, now)) && self.top(now) <= ADMIT_LIMIT
    }

    /// When the vendor can answer again, as Unix seconds: the latest reset among everything that blocks it. None when a
    /// block has no reset time (it then counts as longer than any wait). Meaningful only when `exhausted`.
    pub fn recovery(&self, now: f64) -> Option<f64> {
        self.recovery_for(None, now)
    }

    /// A pinned family also waits for its own blocking window; another family's earlier reset cannot admit it.
    pub fn recovery_for(&self, family: Option<&str>, now: f64) -> Option<f64> {
        let mut latest = self
            .wide(now)
            .filter(|w| w.used > ADMIT_LIMIT || (self.paid_fallback && w.used > STEP_LIMIT))
            .map(|w| w.resets_at)
            .reduce(f64::max);
        for (name, w) in &self.windows {
            if family.is_some() && !vendor_wide(name) && family_of(name) == family && w.live(now) && w.used >= OUT {
                latest = Some(latest.map_or(w.resets_at, |at| at.max(w.resets_at)));
            }
        }
        match self.reach {
            Reach::Out(Some(until)) if until > now => latest = Some(latest.map_or(until, |a| a.max(until))),
            Reach::Out(None) => return None,
            _ => {}
        }
        if self.overage_active {
            return None;
        }
        latest
    }

    /// The word the screen shows for the vendor.
    pub fn state(&self, shape: &Shape, offset: i64, now: f64) -> State {
        let m = self.vendor_m(shape, offset, now);
        if m >= LOW_M || self.top(now) > 0.9 {
            State::Tight
        } else if m <= SPARE_M && self.discount_ok() {
            State::Spare
        } else {
            State::Normal
        }
    }
}

/// What to do with a message when the vendor it needs is out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wait {
    /// Send it itself at this time (Unix seconds).
    Queue(f64),
    /// Do not queue it. The earliest known recovery, for the screen.
    Refuse(Option<f64>),
}

/// The earliest recovery across the exhausted vendors; an unknown recovery counts as longer than the queue limit.
pub fn wait(exhausted: &[&Snap], now: f64) -> Wait {
    let earliest = exhausted.iter().filter_map(|s| s.recovery(now)).reduce(f64::min);
    match earliest {
        Some(t) if t - now <= QUEUE_SECS => Wait::Queue(t),
        other => Wait::Refuse(other),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// `by_hand`: the user picked this vendor themselves.
    UserTurn {
        by_hand: bool,
    },
    Router,
    Pulse,
    Summary,
    /// A local status command that spends no quota; listed so every request goes through the same door.
    PlanCheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Billing,
    Exhausted,
    Window,
    Paid,
}

/// The one door every request to a CLI goes through. `approved` is the user's one-message approval to spend paid credits
/// on this vendor; it lifts the quota rules for that message and nothing else.
pub fn gate(s: &Snap, family: Option<&str>, purpose: Purpose, approved: bool, now: f64) -> Result<(), Why> {
    if purpose == Purpose::PlanCheck {
        return Ok(());
    }
    if s.billing != Billing::Subscription && purpose != (Purpose::UserTurn { by_hand: true }) {
        return Err(Why::Billing);
    }
    if approved {
        return Ok(());
    }
    if s.exhausted(now) || family.is_some_and(|f| s.family_out(f, now)) {
        return Err(Why::Exhausted);
    }
    let top = s.top(now);
    if top > ADMIT_LIMIT {
        return Err(Why::Window);
    }
    if s.paid_fallback && top > STEP_LIMIT {
        return Err(Why::Paid);
    }
    Ok(())
}
