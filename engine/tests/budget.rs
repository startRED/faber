use faber_engine::budget::{
    Billing, Purpose, Reach, Shape, Snap, State, Wait, Why, Window, bin, elapsed, family_of, gate, m_of, vendor_wide, wait, window_m,
    window_secs,
};

/// 5 January 1970 00:00 UTC, a Monday.
const MONDAY: f64 = 345_600.0;
const HOUR: f64 = 3600.0;
const DAY: f64 = 86_400.0;
const NOW: f64 = MONDAY + 2.0 * DAY;

fn near(got: f64, want: f64, tolerance: f64) {
    assert!((got - want).abs() <= tolerance, "{got} is not {want}");
}

fn window(used: f64, resets_in: f64, length: f64) -> Window {
    Window { used, resets_at: NOW + resets_in, read_at: NOW - 60.0, length }
}

fn five(used: f64, resets_in: f64) -> (String, Window) {
    ("five_hour".into(), window(used, resets_in, 5.0 * HOUR))
}

fn seven(used: f64, resets_in: f64) -> (String, Window) {
    ("seven_day".into(), window(used, resets_in, 7.0 * DAY))
}

fn snap(windows: Vec<(String, Window)>) -> Snap {
    Snap { windows, billing: Billing::Subscription, plan_ok: true, ..Snap::default() }
}

fn m(s: &Snap) -> f64 {
    s.vendor_m(&Shape::uniform(), 0, NOW)
}

#[test]
fn the_multiplier_follows_the_projection() {
    near(m_of(0.7, 1.0), 1.0, 1e-9);
    near(m_of(0.5, 1.0), 0.36, 1e-9);
    near(m_of(0.4, 1.0), 0.25, 1e-9);
    near(m_of(0.2, 1.0), 0.140625, 1e-9);
    near(m_of(0.94, 1.0), 25.0, 1e-6);
    assert_eq!(m_of(1.0, 1.0), 25.0);
    assert_eq!(m_of(3.0, 0.2), 25.0);
    // One day of seven left: the margin shrinks, so a week at 45% is nearly all waste.
    near(m_of(0.45, 1.0 / 7.0), 0.1, 1e-9);
    near(m_of(0.887, 1.0 / 7.0), 1.0, 0.02);
    near(m_of(0.95, 1.0 / 7.0), 5.1, 0.1);
    assert_eq!(m_of(f64::NAN, 1.0), 1.0);
    assert_eq!(m_of(-4.0, 1.0), 0.1);
}

#[test]
fn windows_are_told_apart_by_name() {
    assert_eq!(window_secs("five_hour"), Some(18_000.0));
    assert_eq!(window_secs("seven_day"), Some(604_800.0));
    assert_eq!(window_secs("seven_day_opus"), Some(604_800.0));
    assert_eq!(window_secs("300_minutes"), Some(18_000.0));
    assert_eq!(window_secs("0_minutes"), None);
    assert_eq!(window_secs("odd"), None);
    assert!(vendor_wide("five_hour") && vendor_wide("10080_minutes") && !vendor_wide("seven_day_opus") && !vendor_wide("odd"));
    assert_eq!(family_of("seven_day_opus"), Some("opus"));
    assert_eq!(family_of("claude-sonnet-5-5"), Some("sonnet"));
    assert_eq!(family_of("gpt-6.1-sol"), None);
}

#[test]
fn the_week_starts_on_monday_in_local_time() {
    assert_eq!(bin(MONDAY, 0), 0);
    assert_eq!(bin(MONDAY + 6.0 * DAY + 23.0 * HOUR, 0), 167);
    assert_eq!(bin(0.0, 0), 3 * 24, "1 January 1970 was a Thursday");
    // Brazil: 03:00 UTC on Monday is midnight on Monday.
    assert_eq!(bin(MONDAY + 3.0 * HOUR, -3 * 3600), 0);
    assert_eq!(bin(MONDAY + 2.0 * HOUR, -3 * 3600), 167, "and an hour before local midnight it is still Sunday night");
}

#[test]
fn elapsed_time_is_weighted_by_the_shape() {
    let mut monday_only = [0.0; 168];
    monday_only[..24].fill(1.0 / 24.0);
    let shape = Shape(monday_only);
    let end = MONDAY + 7.0 * DAY;
    near(elapsed(&shape, MONDAY, MONDAY + 12.0 * HOUR, end, 0), 0.5, 1e-9);
    near(elapsed(&shape, MONDAY, MONDAY + 5.0 * DAY, end, 0), 1.0, 1e-9);
    near(elapsed(&Shape::uniform(), MONDAY, MONDAY + 3.5 * DAY, end, 0), 0.5, 1e-9);
}

#[test]
fn a_five_hour_window_across_the_end_of_the_week_wraps() {
    let start = MONDAY + 6.0 * DAY + 21.0 * HOUR; // Sunday 21:00
    near(elapsed(&Shape::uniform(), start, start + 2.5 * HOUR, start + 5.0 * HOUR, 0), 0.5, 1e-9);
}

#[test]
fn a_shape_with_no_mass_falls_back_to_plain_time() {
    let empty = Shape([0.0; 168]);
    near(elapsed(&empty, 0.0, 250.0, 1000.0, 0), 0.25, 1e-9);
    assert_eq!(elapsed(&Shape::uniform(), 100.0, 50.0, 100.0, 0), 1.0, "a window of no length is over");
}

#[test]
fn a_window_is_priced_from_its_projection() {
    let shape = Shape::uniform();
    // Half the five hours passed, 10% used: the projection is 20%, far under the pivot.
    let w = window(0.10, 2.5 * HOUR, 5.0 * HOUR);
    near(window_m(&w, NOW, &shape, 0, true), 0.1, 1e-9);
    // 80% used at the halfway mark runs out.
    assert_eq!(window_m(&window(0.8, 2.5 * HOUR, 5.0 * HOUR), NOW, &shape, 0, true), 25.0);
}

#[test]
fn a_discount_is_not_believed_when_the_reading_may_be_wrong() {
    let shape = Shape::uniform();
    let spare = window(0.10, 2.5 * HOUR, 5.0 * HOUR);
    assert_eq!(window_m(&spare, NOW, &shape, 0, false), 1.0, "guards hold");
    let old = Window { read_at: NOW - 7.0 * HOUR, ..spare };
    assert_eq!(window_m(&old, NOW, &shape, 0, true), 1.0, "older than six hours");
    let from_the_future = Window { read_at: NOW + 100.0, ..spare };
    assert_eq!(window_m(&from_the_future, NOW, &shape, 0, true), 1.0, "the clock went back");
    let over = Window { read_at: NOW - 7.0 * HOUR, ..window(0.8, 2.5 * HOUR, 5.0 * HOUR) };
    assert_eq!(window_m(&over, NOW, &shape, 0, true), 25.0, "a stale reading still prices up");
}

#[test]
fn a_reading_that_cannot_be_used_gives_no_information() {
    let shape = Shape::uniform();
    assert_eq!(window_m(&window(0.1, -10.0, 5.0 * HOUR), NOW, &shape, 0, true), 1.0, "the window already reset");
    assert_eq!(window_m(&window(f64::NAN, 100.0, 5.0 * HOUR), NOW, &shape, 0, true), 1.0);
    assert_eq!(window_m(&window(0.1, 100.0, 0.0), NOW, &shape, 0, true), 1.0, "no known length");
}

#[test]
fn the_vendor_takes_its_worst_vendor_wide_window() {
    let s = snap(vec![five(0.10, 2.5 * HOUR), seven(0.45, 3.5 * DAY)]);
    near(m(&s), 4.5, 1e-6);
    assert_eq!(m(&snap(vec![])), 1.0, "no reading, no information");
}

#[test]
fn a_family_window_prices_only_its_own_family() {
    let opus = ("seven_day_opus".to_string(), window(0.45, 3.5 * DAY, 7.0 * DAY));
    let s = snap(vec![five(0.10, 2.5 * HOUR), opus]);
    let shape = Shape::uniform();
    assert_eq!(s.vendor_m(&shape, 0, NOW), 0.1);
    near(s.profile_m(Some("opus"), &shape, 0, NOW), 4.5, 1e-6);
    assert_eq!(s.profile_m(Some("sonnet"), &shape, 0, NOW), 0.1);
    assert_eq!(s.profile_m(None, &shape, 0, NOW), 0.1);
}

#[test]
fn doubt_removes_discounts_but_not_the_warning() {
    let spare = vec![five(0.10, 2.5 * HOUR)];
    let high = vec![seven(0.45, 3.5 * DAY)];
    for doubt in [
        Snap { jumped: true, ..snap(spare.clone()) },
        Snap { relearning: true, ..snap(spare.clone()) },
        Snap { plan_ok: false, ..snap(spare.clone()) },
        Snap { billing: Billing::Unknown, ..snap(spare.clone()) },
    ] {
        assert_eq!(m(&doubt), 1.0);
    }
    near(m(&Snap { jumped: true, ..snap(high) }), 4.5, 1e-6); // running low still shows
}

#[test]
fn the_screen_words_follow_the_multiplier() {
    let shape = Shape::uniform();
    let state = |s: &Snap| s.state(&shape, 0, NOW);
    assert_eq!(state(&snap(vec![five(0.10, 2.5 * HOUR)])), State::Spare);
    assert_eq!(state(&snap(vec![five(0.40, 2.5 * HOUR)])), State::Normal);
    assert_eq!(state(&snap(vec![seven(0.45, 3.5 * DAY)])), State::Tight);
    assert_eq!(state(&snap(vec![seven(0.92, 6.0 * DAY)])), State::Tight, "less than 10% left");
    assert_eq!(state(&Snap { plan_ok: false, ..snap(vec![five(0.10, 2.5 * HOUR)]) }), State::Normal);
}

#[test]
fn a_vendor_is_exhausted_when_it_cannot_answer_at_all() {
    assert!(snap(vec![seven(0.99, DAY)]).exhausted(NOW));
    assert!(!snap(vec![seven(0.99, -5.0)]).exhausted(NOW), "its reset has passed");
    assert!(!snap(vec![seven(0.97, DAY)]).exhausted(NOW));
    assert!(Snap { reach: Reach::Out(None), ..snap(vec![]) }.exhausted(NOW));
    assert!(Snap { reach: Reach::Out(Some(NOW + 60.0)), ..snap(vec![]) }.exhausted(NOW));
    assert!(!Snap { reach: Reach::Out(Some(NOW - 60.0)), ..snap(vec![]) }.exhausted(NOW));
    assert!(Snap { overage_active: true, ..snap(vec![]) }.exhausted(NOW), "paid credits count as exhausted");
}

#[test]
fn a_full_family_window_removes_only_that_family() {
    let s = snap(vec![five(0.2, HOUR), ("seven_day_opus".into(), window(1.0, DAY, 7.0 * DAY))]);
    assert!(!s.exhausted(NOW));
    assert!(!s.usable(Some("opus"), NOW));
    assert!(s.usable(Some("sonnet"), NOW));
    assert!(s.usable(None, NOW));
}

#[test]
fn nothing_is_usable_above_the_admission_limit() {
    assert!(snap(vec![five(0.98, HOUR)]).usable(None, NOW));
    assert!(!snap(vec![five(0.985, HOUR)]).usable(None, NOW));
}

#[test]
fn a_step_needs_every_window_at_ninety_percent_or_less() {
    assert!(snap(vec![five(0.9, HOUR), seven(0.5, DAY)]).step_ok(NOW));
    assert!(!snap(vec![five(0.5, HOUR), seven(0.91, DAY)]).step_ok(NOW));
}

#[test]
fn recovery_is_the_latest_blocking_reset() {
    let s = snap(vec![five(1.0, 1800.0), seven(1.0, 2.0 * DAY)]);
    assert_eq!(s.recovery(NOW), Some(NOW + 2.0 * DAY));
    assert_eq!(Snap { reach: Reach::Out(None), ..s.clone() }.recovery(NOW), None, "a block with no reset time is unknown");
    assert_eq!(Snap { overage_active: true, ..s }.recovery(NOW), None);
    let five_only = snap(vec![five(1.0, 1800.0), seven(0.3, 2.0 * DAY)]);
    assert_eq!(five_only.recovery(NOW), Some(NOW + 1800.0), "an open weekly window does not delay the five hours");
}

#[test]
fn a_message_waits_for_the_first_vendor_back_if_that_is_soon() {
    let soon = snap(vec![five(1.0, 2.0 * HOUR)]);
    let late = snap(vec![seven(1.0, 2.0 * DAY)]);
    let unknown = Snap { reach: Reach::Out(None), ..snap(vec![]) };
    assert_eq!(wait(&[&soon, &late], NOW), Wait::Queue(NOW + 2.0 * HOUR));
    assert_eq!(wait(&[&unknown, &soon], NOW), Wait::Queue(NOW + 2.0 * HOUR));
    assert_eq!(wait(&[&unknown, &late], NOW), Wait::Refuse(Some(NOW + 2.0 * DAY)));
    assert_eq!(wait(&[&unknown], NOW), Wait::Refuse(None));
    let seven_hours = snap(vec![five(1.0, 7.0 * HOUR)]);
    assert_eq!(wait(&[&seven_hours], NOW), Wait::Refuse(Some(NOW + 7.0 * HOUR)));
}

const TURN: Purpose = Purpose::UserTurn { by_hand: false };

#[test]
fn the_gate_lets_a_vendor_with_no_reading_answer() {
    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary, Purpose::PlanCheck] {
        assert_eq!(gate(&snap(vec![]), None, purpose, false, NOW), Ok(()));
    }
}

#[test]
fn the_gate_denies_unknown_billing_for_every_automatic_request() {
    let unknown = Snap::default();
    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary] {
        assert_eq!(gate(&unknown, None, purpose, false, NOW), Err(Why::Billing));
        assert_eq!(gate(&unknown, None, purpose, true, NOW), Err(Why::Billing));
    }
    assert_eq!(gate(&unknown, None, Purpose::PlanCheck, false, NOW), Ok(()));
    assert_eq!(gate(&unknown, None, Purpose::UserTurn { by_hand: true }, false, NOW), Ok(()));
}

#[test]
fn the_gate_keeps_usage_billed_logins_for_the_user_alone() {
    let billed = Snap { billing: Billing::UsageBilled, ..snap(vec![]) };
    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary] {
        assert_eq!(gate(&billed, None, purpose, false, NOW), Err(Why::Billing));
    }
    assert_eq!(gate(&billed, None, Purpose::UserTurn { by_hand: true }, false, NOW), Ok(()));
    assert_eq!(gate(&billed, None, Purpose::PlanCheck, false, NOW), Ok(()));
    assert_eq!(gate(&billed, None, TURN, true, NOW), Err(Why::Billing), "an approval never lifts the billing rule");
}

#[test]
fn the_gate_denies_what_is_out_or_nearly_out() {
    assert_eq!(gate(&snap(vec![seven(1.0, DAY)]), None, TURN, false, NOW), Err(Why::Exhausted));
    let opus_out = snap(vec![five(0.2, HOUR), ("seven_day_opus".into(), window(1.0, DAY, 7.0 * DAY))]);
    assert_eq!(gate(&opus_out, Some("opus"), TURN, false, NOW), Err(Why::Exhausted));
    assert_eq!(gate(&opus_out, Some("sonnet"), TURN, false, NOW), Ok(()));
    assert_eq!(gate(&snap(vec![five(0.985, HOUR)]), None, Purpose::Pulse, false, NOW), Err(Why::Window));
}

#[test]
fn the_gate_is_stricter_where_the_cli_could_switch_to_paid_use_by_itself() {
    let near_limit = Snap { paid_fallback: true, ..snap(vec![five(0.91, HOUR)]) };
    for purpose in [TURN, Purpose::Router, Purpose::Pulse, Purpose::Summary] {
        assert_eq!(gate(&near_limit, None, purpose, false, NOW), Err(Why::Paid));
    }
    assert_eq!(gate(&Snap { paid_fallback: false, ..near_limit.clone() }, None, TURN, false, NOW), Ok(()));
    assert_eq!(gate(&Snap { paid_fallback: true, ..snap(vec![five(0.9, HOUR)]) }, None, TURN, false, NOW), Ok(()));
}

#[test]
fn a_one_message_approval_lifts_the_quota_rules() {
    let out = Snap { paid_fallback: true, ..snap(vec![seven(1.0, DAY)]) };
    assert_eq!(gate(&out, None, TURN, false, NOW), Err(Why::Exhausted));
    assert_eq!(gate(&out, None, TURN, true, NOW), Ok(()));
}

#[test]
fn expired_vendor_and_family_windows_do_not_mask_spare_live_quota() {
    let live = snap(vec![seven(0.05, DAY)]);
    let mut mixed = live.clone();
    mixed.windows.push(five(1.0, -HOUR));
    mixed.windows.push(("seven_day_opus".into(), window(1.0, -HOUR, 7.0 * DAY)));
    near(m(&mixed), m(&live), 1e-9);
    near(mixed.profile_m(Some("opus"), &Shape::uniform(), 0, NOW), m(&live), 1e-9);
    assert!(m(&mixed) < 1.0);
}
