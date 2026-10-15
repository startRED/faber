//! When to read each vendor's quota: a plain schedule over explicit times, so it is tested with a fake clock. The readings
//! themselves cost no model tokens (a status command, a rate-limit call); this only decides how often to ask.
//!
//! Faber reads when it starts, every few minutes while its window is open, less often while it is hidden, right after a
//! request ends, when the window gains focus, and when the PC wakes up. A sleeping PC reads nothing: the loop is asleep too.

use crate::model::Vendor;
use std::collections::BTreeMap;

/// How often the loop looks at the clock.
pub const TICK: f64 = 30.0;
/// Between readings while the window is open, and while it is hidden. The owner's ceiling is 30 minutes of delay.
pub const EVERY_VISIBLE: f64 = 5.0 * 60.0;
pub const EVERY_HIDDEN: f64 = 15.0 * 60.0;
/// A request ends often in a busy chat; a reading this soon after the last one adds nothing.
pub const AFTER_TURN_GAP: f64 = 2.0 * 60.0;
/// Focus comes and goes with every click on another window.
pub const FOCUS_GAP: f64 = 30.0;
/// A loop that was not heard from for this long slept with the PC.
const SLEPT: f64 = 3.0 * TICK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Start,
    Timer,
    TurnEnded,
    Focus,
    Wake,
}

#[derive(Debug, Clone)]
pub struct Cadence {
    last_read: BTreeMap<Vendor, f64>,
    last_tick: Option<f64>,
    visible: bool,
}

impl Default for Cadence {
    fn default() -> Self {
        Cadence { last_read: BTreeMap::new(), last_tick: None, visible: true }
    }
}

impl Cadence {
    fn take(&mut self, vendor: Vendor, now: f64, gap: f64) -> bool {
        let due = self.last_read.get(&vendor).is_none_or(|t| !(0.0..gap).contains(&(now - t)));
        if due {
            self.last_read.insert(vendor, now);
        }
        due
    }

    /// The loop's look at the clock: who is due, and why. A clock that jumped far ahead (or back) means the PC slept.
    pub fn tick(&mut self, now: f64, vendors: &[Vendor]) -> Vec<(Vendor, Reason)> {
        let woke = self.last_tick.is_some_and(|t| !(0.0..SLEPT).contains(&(now - t)));
        self.last_tick = Some(now);
        let every = if self.visible { EVERY_VISIBLE } else { EVERY_HIDDEN };
        vendors
            .iter()
            .filter_map(|v| {
                let reason = match self.last_read.get(v) {
                    None => Reason::Start,
                    Some(_) if woke => Reason::Wake,
                    Some(_) => Reason::Timer,
                };
                self.take(*v, now, if matches!(reason, Reason::Timer) { every } else { 0.0 }).then_some((*v, reason))
            })
            .collect()
    }

    /// Something happened that makes the readings worth refreshing: who should be read now.
    pub fn event(&mut self, now: f64, vendors: &[Vendor], reason: Reason) -> Vec<Vendor> {
        let gap = match reason {
            Reason::TurnEnded => AFTER_TURN_GAP,
            Reason::Focus => FOCUS_GAP,
            _ => 0.0,
        };
        vendors.iter().copied().filter(|v| self.take(*v, now, gap)).collect()
    }

    /// The window was shown, hidden or focused. Gaining focus reads at once.
    pub fn attention(&mut self, now: f64, vendors: &[Vendor], visible: bool, focused: bool) -> Vec<Vendor> {
        self.visible = visible;
        if visible && focused { self.event(now, vendors, Reason::Focus) } else { vec![] }
    }
}
