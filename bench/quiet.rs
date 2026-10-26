//! Quiet intervals for benchmark admission; no I/O or model calls.
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub at: f64,
    pub used: f64,
    pub reset: f64,
}

#[derive(Default)]
pub struct Quiet {
    after: f64,
    previous: BTreeMap<String, Sample>,
}

impl Quiet {
    pub fn own_work_ended(&mut self, now: f64, lag: f64) {
        self.after = now + lag;
        self.previous.clear();
    }

    pub fn observe(&mut self, now: f64, gap: f64, samples: BTreeMap<String, Sample>) -> bool {
        if !now.is_finite()
            || !gap.is_finite()
            || gap <= 0.0
            || samples.is_empty()
            || samples.values().any(|s| {
                !s.at.is_finite()
                    || !s.reset.is_finite()
                    || !(0.0..=1.5).contains(&s.used)
                    || s.at <= self.after
                    || s.at > now
                    || now - s.at > gap
                    || s.reset <= now
            })
        {
            self.previous.clear();
            return false;
        }
        let quiet = samples.len() == self.previous.len()
            && samples.iter().all(|(key, s)| {
                self.previous
                    .get(key)
                    .is_some_and(|p| s.reset == p.reset && s.used == p.used && s.at - p.at >= gap && s.at - p.at <= 2.0 * gap)
            });
        self.previous = samples;
        quiet
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn samples(at: f64, used: f64, reset: f64) -> BTreeMap<String, Sample> {
        BTreeMap::from([("session".into(), Sample { at, used, reset })])
    }
    #[test]
    fn outside_use_pauses_until_a_new_quiet_interval() {
        let mut q = Quiet::default();
        assert!(!q.observe(100., 60., samples(100., 0.2, 5000.)));
        assert!(!q.observe(160., 60., samples(160., 0.25, 5000.)));
        assert!(q.observe(220., 60., samples(220., 0.25, 5000.)));
    }
    #[test]
    fn own_work_and_reporting_lag_need_new_baselines() {
        let mut q = Quiet::default();
        q.own_work_ended(100., 600.);
        assert!(!q.observe(700., 60., samples(700., 0.3, 5000.)));
        assert!(!q.observe(701., 60., samples(701., 0.3, 5000.)));
        assert!(q.observe(761., 60., samples(761., 0.3, 5000.)));
    }
    #[test]
    fn missing_stale_future_reset_and_new_windows_are_not_quiet() {
        let mut q = Quiet::default();
        assert!(!q.observe(100., 60., BTreeMap::new()));
        assert!(!q.observe(100., 60., samples(101., 0.2, 5000.)));
        assert!(!q.observe(1000., 60., samples(100., 0.2, 5000.)));
        assert!(!q.observe(1000., 60., samples(1000., 0.2, 5000.)));
        assert!(!q.observe(1060., 60., samples(1060., 0., 6000.)));
        let mut changed = samples(1120., 0., 6000.);
        changed.insert("week".into(), Sample { at: 1120., used: 0.1, reset: 9000. });
        assert!(!q.observe(1120., 60., changed));
    }
}
