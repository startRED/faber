//! Four weeks of a made-up account, 200 seeds, pacing on against pacing off. Pure: no clock, no process. The setup is
//! fixed here; the constants in budget.rs change only if this fails, and the change is logged in the spec.

use faber_engine::budget::{Billing, Shape, Snap, Window, family_of};
use faber_engine::model::Vendor;
use faber_engine::route::{Factor, Profile, candidates, choose, choose_paced};

const MONDAY: f64 = 345_600.0;
const HOUR: f64 = 3600.0;
const WEEKS: usize = 4;
const SEEDS: u64 = 200;
// Seed 12 exercises unpaced exhaustion; the original four seeds all tied at zero exhausted hours.
const SMOKE_SEEDS: [u64; 4] = [0, 1, 2, 12];
/// Weekly quota of each vendor in cost units: Claude is the small plan, Codex the large one.
const QUOTA: [f64; 2] = [100.0, 400.0];
/// The mix of the 22 real requests, as (kind, level, share in percent).
const MIX: [(&str, &str, u32); 8] = [
    ("chat", "easy", 20),
    ("writing", "medium", 15),
    ("code", "medium", 25),
    ("code", "hard", 10),
    ("math", "medium", 5),
    ("research", "medium", 15),
    ("research", "hard", 5),
    ("other", "easy", 5),
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn poisson(&mut self, lambda: f64) -> u32 {
        let (limit, mut k, mut p) = ((-lambda).exp(), 0, 1.0);
        loop {
            p *= self.next();
            if p <= limit {
                return k;
            }
            k += 1;
        }
    }
}

/// Busy on weekdays 9 to 18, quiet otherwise; sums to 1.
fn week_shape() -> Shape {
    let mut s = [0.0; 168];
    for (i, slot) in s.iter_mut().enumerate() {
        let (day, hour) = (i / 24, i % 24);
        *slot = if day < 5 && (9..18).contains(&hour) { 1.0 } else { 0.15 };
    }
    let sum: f64 = s.iter().sum();
    Shape(s.map(|v| v / sum))
}

#[derive(Default, Clone, Copy)]
struct Outcome {
    exhausted: bool,
    exhausted_hours: u32,
    unused_of_larger: f64,
    quality: f64,
    matters: f64,
    dropped: f64,
    tasks: f64,
}

fn vendor_index(v: Vendor) -> usize {
    if v == Vendor::Claude { 0 } else { 1 }
}

fn run(seed: u64, paced: bool) -> Outcome {
    let shape = week_shape();
    let cands = candidates(&[Vendor::Claude, Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]);
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0xD1B54A32D192ED03);
    // Faber's tasks per week: 65% of the combined capacity at the cost of what the plain table picks on average.
    let mean_cost: f64 =
        MIX.iter().map(|(k, l, s)| choose(k, l, "glance", 0.9, &cands).unwrap().profile.cost as f64 * *s as f64 / 100.0).sum();
    let tasks_per_week = 0.65 * (QUOTA[0] + QUOTA[1]) / mean_cost;
    let burst = seed.is_multiple_of(2);
    let mut out = Outcome::default();
    for week in 0..WEEKS {
        let resets_at = MONDAY + (week + 1) as f64 * 7.0 * 24.0 * HOUR;
        let mut used = [0.0_f64; 2]; // the truth, as a share of each quota
        let mut seen: [Option<(f64, f64)>; 2] = [None, None]; // (reading, when)
        let mut exhausted_this_week = false;
        for h in 0..168 {
            let now = MONDAY + (week * 168 + h) as f64 * HOUR;
            // Use from outside Faber: 20% of the combined capacity over a week, spread like Faber's, with a burst.
            for slot in used.iter_mut() {
                let boost = if burst && week == 1 && (58..64).contains(&h) { 3.0 } else { 1.0 };
                *slot += 0.20 * shape.0[h] * (0.5 + rng.next()) * boost;
            }
            for _ in 0..rng.poisson(tasks_per_week * shape.0[h]) {
                out.tasks += 1.0;
                let (kind, level, _) = {
                    let r = (rng.next() * 100.0) as u32;
                    let mut acc = 0;
                    *MIX.iter()
                        .find(|(_, _, s)| {
                            acc += s;
                            r < acc
                        })
                        .unwrap_or(&MIX[0])
                };
                // The quota pulse (Claude) and the free rate-limit read (Codex) keep both readings fresh.
                for i in 0..2 {
                    seen[i] = Some(((used[i].min(1.0) * 100.0).round() / 100.0, now));
                }
                let snaps: Vec<Snap> = (0..2)
                    .map(|i| Snap {
                        windows: seen[i]
                            .map(|(u, at)| {
                                vec![("seven_day".to_string(), Window { used: u, resets_at, read_at: at, length: 7.0 * 24.0 * HOUR })]
                            })
                            .unwrap_or_default(),
                        billing: Billing::Subscription,
                        plan_ok: true,
                        ..Snap::default()
                    })
                    .collect();
                // A vendor that has run out of quota cannot answer; the rest can.
                let open: Vec<Profile> = cands
                    .iter()
                    .filter(|p| used[vendor_index(p.vendor)] < 1.0 && snaps[vendor_index(p.vendor)].usable(family_of(p.model), now))
                    .copied()
                    .collect();
                if open.is_empty() {
                    out.dropped += 1.0;
                    continue;
                }
                let factor = |p: &Profile| {
                    let s = &snaps[vendor_index(p.vendor)];
                    Factor { m: s.profile_m(family_of(p.model), &shape, 0, now), w: 1.0, step_ok: s.step_ok(now) }
                };
                let routed = if paced {
                    choose_paced(kind, level, "glance", 0.9, &open, &factor)
                } else {
                    choose(kind, level, "glance", 0.9, &open)
                }
                .unwrap();
                let i = vendor_index(routed.profile.vendor);
                used[i] += routed.profile.cost as f64 / QUOTA[i] * (0.7 + 0.6 * rng.next());
                seen[i] = Some(((used[i].min(1.0) * 100.0).round() / 100.0, now));
                if matches!((kind, level), ("code", "medium" | "hard") | ("math", _) | ("research", "medium" | "hard")) {
                    out.quality += (routed.profile.iq as u32 + routed.profile.agent as u32) as f64;
                    out.matters += 1.0;
                }
            }
            let exhausted = used.iter().any(|u| *u >= 1.0);
            exhausted_this_week |= exhausted;
            out.exhausted_hours += u32::from(exhausted);
        }
        out.exhausted |= exhausted_this_week;
        out.unused_of_larger += (1.0 - used[1].min(1.0)) / WEEKS as f64;
    }
    out.quality /= out.matters.max(1.0);
    out
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// The full acceptance run: slow in a debug build, so it is ignored by default. Run it with
/// `cargo test --release -p faber-engine --test simulation -- --ignored --nocapture`.
#[test]
#[ignore = "slow in a debug build: run it with --release"]
fn pacing_spends_the_quota_that_would_expire_without_running_out() {
    judge(&(0..SEEDS).collect::<Vec<_>>());
}

/// The same setup on a few seeds, small enough for every run: pacing must not do worse than no pacing.
#[test]
fn pacing_does_no_worse_than_no_pacing_on_a_few_seeds() {
    judge(&SMOKE_SEEDS);
}

fn judge(seeds: &[u64]) {
    let (mut on, mut off) = (vec![], vec![]);
    for &seed in seeds {
        on.push(run(seed, true));
        off.push(run(seed, false));
    }
    let share = |v: &[Outcome]| v.iter().filter(|o| o.exhausted).count() as f64 / v.len() as f64;
    let hours = |v: &[Outcome]| v.iter().map(|o| o.exhausted_hours as f64).sum::<f64>() / v.len() as f64;
    eprintln!("exhausted hours per four-week run: on {:.3} off {:.3}", hours(&on), hours(&off));
    assert!(hours(&on) < hours(&off), "pacing must reduce exhausted hours");
    let unused_on = median(on.iter().map(|o| o.unused_of_larger).collect());
    let unused_off = median(off.iter().map(|o| o.unused_of_larger).collect());
    let quality_on = on.iter().map(|o| o.quality).sum::<f64>() / on.len() as f64;
    let quality_off = off.iter().map(|o| o.quality).sum::<f64>() / off.len() as f64;
    eprintln!(
        "dropped tasks per run: on {:.1} off {:.1}",
        on.iter().map(|o| o.dropped).sum::<f64>() / on.len() as f64,
        off.iter().map(|o| o.dropped).sum::<f64>() / off.len() as f64
    );
    eprintln!(
        "exhausted: on {:.3} off {:.3}; median unused of the larger quota: on {:.3} off {:.3}; quality: on {:.2} off {:.2}",
        share(&on),
        share(&off),
        unused_on,
        unused_off,
        quality_on,
        quality_off
    );
    let dropped = on.iter().map(|o| o.dropped).sum::<f64>() / on.iter().map(|o| o.tasks).sum::<f64>();
    let dropped_off = off.iter().map(|o| o.dropped).sum::<f64>() / off.iter().map(|o| o.tasks).sum::<f64>();
    eprintln!("dropped task share: on {:.5} off {:.5}", dropped, dropped_off);
    assert!(dropped <= 0.01, "too many tasks found both vendors closed: {dropped}");
    assert!(share(&on) <= 0.05, "a vendor ran out before its reset in too many seeds");
    assert!(share(&on) <= share(&off) + 1e-9);
    assert!(unused_on < 0.25, "too much of the larger quota expired unused");
    assert!(quality_on >= quality_off, "pacing must not lower the quality of the tasks that matter");
}
