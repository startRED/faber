use faber_engine::learn::{Learned, Point, Span, Stretch, capacity_changed, outside, stretches, weights};

/// 5 January 1970 00:00 UTC, a Monday.
const MONDAY: f64 = 345_600.0;
const HOUR: f64 = 3600.0;
const DAY: f64 = 86_400.0;

fn near(got: f64, want: f64, tolerance: f64) {
    assert!((got - want).abs() <= tolerance, "{got} is not {want}");
}

fn at(t: f64, points: f64, resets_at: f64) -> Point {
    Point { at: t, used: points / 100.0, resets_at }
}

#[test]
fn without_data_the_week_is_flat() {
    assert_eq!(Learned::default().shape(), faber_engine::budget::Shape::uniform());
}

#[test]
fn movement_is_spread_over_the_hours_the_gap_overlaps() {
    let mut l = Learned::default();
    // 09:30 to 11:30 on a Monday: half an hour in the 09 bin, one in the 10, half in the 11; 4 points in all.
    l.add(&at(MONDAY + 9.5 * HOUR, 10.0, 1.0), &at(MONDAY + 11.5 * HOUR, 14.0, 1.0), 0);
    near(l.mass[9], 1.0, 1e-9);
    near(l.mass[10], 2.0, 1e-9);
    near(l.mass[11], 1.0, 1e-9);
    near(l.covered, 2.0 * HOUR, 1e-9);
}

#[test]
fn gaps_that_say_nothing_are_left_out() {
    let mut l = Learned::default();
    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + 13.0 * HOUR, 20.0, 1.0), 0);
    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + HOUR, 20.0, 2.0), 0);
    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + HOUR, 5.0, 1.0), 0);
    l.add(&at(MONDAY + HOUR, 10.0, 1.0), &at(MONDAY, 10.0, 1.0), 0);
    assert_eq!(l.covered, 0.0);
    assert!(l.mass.iter().all(|m| *m == 0.0));
    // No movement still counts as time covered: an idle night is information.
    l.add(&at(MONDAY, 10.0, 1.0), &at(MONDAY + HOUR, 10.0, 1.0), 0);
    assert_eq!(l.covered, HOUR);
}

#[test]
fn two_weeks_of_weekday_use_teach_the_shape() {
    let mut l = Learned::default();
    for week in 0..2 {
        let resets = MONDAY + (week + 1) as f64 * 7.0 * DAY;
        let mut used = 0.0;
        let mut previous = at(MONDAY + week as f64 * 7.0 * DAY, used, resets);
        for h in 1..=168 {
            let t = MONDAY + week as f64 * 7.0 * DAY + h as f64 * HOUR;
            let hour_of_day = ((h - 1) % 24) as f64;
            let weekday = (h - 1) / 24;
            if weekday < 5 && (9.0..17.0).contains(&hour_of_day) {
                used += 2.0;
            }
            let now = at(t, used, resets);
            l.add(&previous, &now, 0);
            previous = now;
        }
    }
    let shape = l.shape();
    assert!(shape.0[10] > 4.0 * shape.0[5 * 24 + 10], "Monday 10:00 against Saturday 10:00");
    assert!(shape.0[10] > 4.0 * shape.0[3], "Monday 10:00 against Monday 03:00");
    near(shape.0.iter().sum::<f64>(), 1.0, 1e-9);
}

#[test]
fn a_little_data_changes_the_shape_only_a_little() {
    let mut l = Learned::default();
    l.add(&at(MONDAY, 0.0, 1.0), &at(MONDAY + HOUR, 8.0, 1.0), 0);
    let shape = l.shape();
    assert!(shape.0[0] > 1.0 / 168.0);
    assert!(shape.0[0] < 2.0 / 168.0, "one hour of data out of two weeks does not make the week");
    near(shape.0.iter().sum::<f64>(), 1.0, 1e-9);
}

#[test]
fn movement_with_no_turn_of_faber_in_it_is_outside_use() {
    let now = MONDAY + 2.0 * DAY;
    let points = [
        at(now - 20.0 * HOUR, 10.0, 9.0),
        at(now - 18.0 * HOUR, 16.0, 9.0),
        at(now - 16.0 * HOUR, 25.0, 9.0),
        at(now - 14.0 * HOUR, 25.5, 9.0),
    ];
    let turn = Span { start: now - 17.0 * HOUR, end: now - 16.5 * HOUR, model: "m".into(), cost: Some(1.0) };
    let o = outside(&points, &[turn], now);
    near(o.outside, 6.0, 1e-9); // the first gap; the second has a turn in it; the third moved less than two points
    near(o.observed, 15.5, 1e-9);
    near(o.share().unwrap(), 6.0 / 15.5, 1e-9);
    assert_eq!(outside(&[], &[], now).share(), None);
}

#[test]
fn a_burst_from_outside_is_a_jump() {
    let now = MONDAY + 2.0 * DAY;
    // No history: any three points count.
    let burst = [at(now - 2.0 * HOUR, 10.0, 9.0), at(now, 15.0, 9.0)];
    assert!(outside(&burst, &[], now).jumped());
    let small = [at(now - 3.0 * HOUR, 10.0, 9.0), at(now - HOUR, 12.0, 9.0)];
    assert!(!outside(&small, &[], now).jumped());
    // A steady point an hour all day (read every two hours): five points in the last two hours are only a little over the
    // average, eight points are a jump.
    let mut steady: Vec<Point> = (0..13).map(|i| at(now - (26 - 2 * i) as f64 * HOUR, 10.0 + 2.0 * i as f64, 9.0)).collect();
    let mut calm = steady.clone();
    calm.push(at(now, 34.0 + 5.0, 9.0));
    let o = outside(&calm, &[], now);
    near(o.rate, 1.0, 1e-9);
    assert!(!o.jumped(), "{o:?}");
    steady.push(at(now, 34.0 + 8.0, 9.0));
    assert!(outside(&steady, &[], now).jumped());
}

fn quiet_week() -> Vec<Point> {
    (0..8).map(|i| at(MONDAY + i as f64 * HOUR, i as f64 * 5.0, 9.0)).collect()
}

#[test]
fn a_clean_run_of_turns_on_one_model_is_a_stretch() {
    let points = quiet_week();
    let spans = [Span { start: MONDAY + 2.1 * HOUR, end: MONDAY + 2.5 * HOUR, model: "a".into(), cost: Some(0.5) }];
    // The 5 points between 02:00 and 03:00 for 0.5 of cost: 10 points per cost unit. The neighbours (5 points each) have
    // no Faber turn in them, so use from elsewhere may hide in the stretch and nothing is accepted.
    assert_eq!(stretches(&points, &spans), vec![]);
    let mut points = quiet_week();
    // Make the neighbours quiet: less than two points moved on either side.
    points[2].used = points[1].used + 0.005;
    points[4].used = points[3].used + 0.005;
    let spans = [Span { start: MONDAY + 2.1 * HOUR, end: MONDAY + 2.5 * HOUR, model: "a".into(), cost: Some(0.5) }];
    let found = stretches(&points, &spans);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].model, "a");
    near(found[0].ppd, (points[3].used - points[2].used) * 100.0 / 0.5, 1e-9);
}

#[test]
fn parallel_turns_other_models_unknown_costs_and_small_moves_are_not_stretches() {
    let mut points = quiet_week();
    points[2].used = points[1].used + 0.005;
    points[4].used = points[3].used + 0.005;
    let span = |s: f64, e: f64, model: &str, cost| Span { start: MONDAY + s * HOUR, end: MONDAY + e * HOUR, model: model.into(), cost };
    let overlapping = [span(2.1, 2.6, "a", Some(0.5)), span(2.4, 2.9, "a", Some(0.5))];
    assert_eq!(stretches(&points, &overlapping), vec![]);
    let two_models = [span(2.1, 2.3, "a", Some(0.5)), span(2.4, 2.9, "b", Some(0.5))];
    assert_eq!(stretches(&points, &two_models), vec![]);
    assert_eq!(stretches(&points, &[span(2.1, 2.5, "a", None)]), vec![]);
    assert_eq!(stretches(&points, &[span(2.1, 2.5, "a", Some(0.0))]), vec![]);
    let mut small = points.clone();
    small[3].used = small[2].used + 0.02;
    assert_eq!(stretches(&small, &[span(2.1, 2.5, "a", Some(0.5))]), vec![], "fewer than three points");
}

fn run(model: &str, values: &[f64]) -> Vec<Stretch> {
    values.iter().map(|v| Stretch { model: model.into(), ppd: *v }).collect()
}

#[test]
fn a_weight_is_the_raw_cost_in_points_against_the_vendors_median() {
    let mut s = run("cheap", &[10.0; 8]);
    s.extend(run("dear", &[20.0; 8]));
    s.extend(run("middle", &[14.0; 8]));
    let w = weights(&s);
    near(w.capacity.unwrap(), 14.0, 1e-9);
    near(w.w["middle"], 1.0, 1e-9);
    near(w.w["cheap"], 10.0 / 14.0, 1e-9);
    near(w.w["dear"], 20.0 / 14.0, 1e-9);
}

#[test]
fn weights_wait_for_enough_stretches_and_are_clamped() {
    assert_eq!(weights(&run("a", &[10.0; 7])), Default::default(), "seven stretches are not enough");
    let mut s = run("a", &[10.0; 8]);
    s.extend(run("b", &[100.0; 8]));
    s.extend(run("c", &[11.0; 8]));
    let w = weights(&s);
    assert_eq!(w.w["b"], 2.0);
}

#[test]
fn the_lower_quartile_ignores_use_from_elsewhere_and_a_spread_estimate_is_unreliable() {
    // Mostly 10, with a few inflated by outside use: the lower quartile is 10.
    let mut values = vec![10.0; 12];
    values.extend([30.0, 35.0, 40.0]);
    let mut s = run("a", &values);
    s.extend(run("b", &[10.0; 8]));
    let w = weights(&s);
    near(w.w["a"], 1.0, 1e-9);
    // Median more than twice the lower quartile: no estimate for the model.
    let mut spread = run("a", &[5.0, 5.0, 12.0, 12.0, 13.0, 13.0, 14.0, 14.0]);
    spread.extend(run("b", &[10.0; 8]));
    let w = weights(&spread);
    assert!(!w.w.contains_key("a"));
    assert!(w.w.contains_key("b"));
}

#[test]
fn only_the_last_twenty_stretches_count() {
    let mut values = vec![100.0; 30];
    values.extend([10.0; 20]);
    near(weights(&run("a", &values)).capacity.unwrap(), 10.0, 1e-9);
}

#[test]
fn a_capacity_that_moves_more_than_twice_is_a_plan_change() {
    assert!(capacity_changed(10.0, 25.0));
    assert!(capacity_changed(10.0, 4.0));
    assert!(!capacity_changed(10.0, 19.0));
    assert!(!capacity_changed(10.0, 6.0));
    assert!(!capacity_changed(0.0, 6.0));
}

#[test]
fn an_overnight_gap_contributes_only_its_overlap_with_the_last_two_hours() {
    let now = MONDAY + 2.0 * DAY;
    let points = [at(now - 9.0 * HOUR, 40.0, 9.0), at(now, 45.0, 9.0)];
    let o = outside(&points, &[], now);
    near(o.recent, 5.0 * 2.0 / 9.0, 1e-9);
    near(o.rate, 5.0 / 9.0, 1e-9);
    assert!(!o.jumped());
}

#[test]
fn models_qualifying_later_do_not_fake_a_plan_change() {
    use faber_engine::learn::same_models_changed;
    let old = weights(&run("haiku", &[50.0; 8]));
    let mut expanded = run("haiku", &[50.0; 8]);
    expanded.extend(run("sonnet", &[10.0; 8]));
    expanded.extend(run("opus", &[12.0; 8]));
    let new = weights(&expanded);
    assert!(capacity_changed(old.capacity.unwrap(), new.capacity.unwrap()));
    assert!(!same_models_changed(&old, &new));
    assert!(same_models_changed(&old, &weights(&run("haiku", &[150.0; 8]))));
}
