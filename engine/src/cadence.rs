//! When to read each vendor’s quota: a plain schedule over explicit times, so it is tested with a fake clock. The readings
//! themselves cost no model tokens (a status command, a rate-limit call; for a Claude Code that prints no percentages, one
//! request of about 500 cached tokens); this only decides how often to ask.
//!
//! Faber reads when it starts and then every 15 minutes, whether its window is visible or not. Things that happen in between
//! (a request ended, the window gained focus, the PC woke up) may ask for an earlier reading, but all of them share ONE
//! debounce per vendor: no reading sooner than a gap after the last one, whatever the last one was. The gap is longer for
//! Claude Code, whose reading starts a heavy Node process, than for Codex, whose server answers in a second.
//! A sleeping PC reads nothing: the loop is asleep too.

use crate::model::Vendor;
use std::collections::BTreeMap;

/// How often the loop looks at the clock.
pub const TICK: f64 = 30.0;
/// Between two timed readings. The owner’s ceiling is 30 minutes of delay; the vendors count within five (docs/product/quota-pacing.md).
pub const EVERY: f64 = 15.0 * 60.0;
/// The shared debounce of event readings (end of a request, focus, wake): at least this long since the last reading.
pub const CLAUDE_GAP: f64 = 5.0 * 60.0;
pub const CODEX_GAP: f64 = 60.0;
/// A loop that was not heard from for this long slept with the PC.
const SLEPT: f64 = 3.0 * TICK;

pub fn gap(vendor: Vendor) -> f64 {
    match vendor {
        Vendor::Claude => CLAUDE_GAP,
        Vendor::Codex => CODEX_GAP,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Start,
    Timer,
    Wake,
}

#[derive(Debug, Clone, Default)]
pub struct Cadence {
    last_read: BTreeMap<Vendor, f64>,
    last_tick: Option<f64>,
}

impl Cadence {
    /// Takes the reading if the last one is at least `gap` seconds old (or there is none, or the clock went back).
    fn take(&mut self, vendor: Vendor, now: f64, gap: f64) -> bool {
        let due = self.last_read.get(&vendor).is_none_or(|t| !(0.0..gap).contains(&(now - t)));
        if due {
            self.last_read.insert(vendor, now);
        }
        due
    }

    /// The loop’s look at the clock: who is due, and why. A clock that jumped far ahead (or back) means the PC slept, which
    /// is an event like any other and goes through the same debounce.
    pub fn tick(&mut self, now: f64, vendors: &[Vendor]) -> Vec<(Vendor, Reason)> {
        let woke = self.last_tick.is_some_and(|t| !(0.0..SLEPT).contains(&(now - t)));
        self.last_tick = Some(now);
        vendors
            .iter()
            .filter_map(|v| {
                let (reason, gap) = match self.last_read.get(v) {
                    None => (Reason::Start, 0.0),
                    Some(_) if woke => (Reason::Wake, gap(*v)),
                    Some(_) => (Reason::Timer, EVERY),
                };
                self.take(*v, now, gap).then_some((*v, reason))
            })
            .collect()
    }

    /// Something happened that makes the readings worth refreshing: who should be read now.
    pub fn event(&mut self, now: f64, vendors: &[Vendor]) -> Vec<Vendor> {
        vendors.iter().copied().filter(|v| self.take(*v, now, gap(*v))).collect()
    }

    /// The window gained focus: read, unless one was taken lately.
    pub fn attention(&mut self, now: f64, vendors: &[Vendor], visible: bool, focused: bool) -> Vec<Vendor> {
        if visible && focused { self.event(now, vendors) } else { vec![] }
    }
}
