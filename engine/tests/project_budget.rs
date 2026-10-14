use faber_engine::budget::Billing;
use faber_engine::events::EngineEvent;
use faber_engine::learn::{Point, Span};
use faber_engine::ledger::Ledger;
use faber_engine::model::Vendor;
use faber_engine::project::Extra;
use faber_engine::project::budget::{Budget, Change, Grant, Invocation, Kind, Status, apply};
use faber_engine::store::Store;
use std::collections::BTreeMap;

const DAY: f64 = 86_400.0;
const RESET: f64 = 7.0 * DAY;
const NOW: f64 = 2.0 * DAY + 3600.0;
const MODEL: &str = "gpt-6.1-sol";

#[test]
fn the_installed_unmetered_interrupt_cannot_return_reserved_spend() {
    let mut ledger = ready(200_000);
    apply(&mut ledger, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    let mut state = faber_engine::codex::CodexTurnState::default();
    let mut done = None;
    for line in include_str!("../../tests/fixtures/projects/interrupted-unmetered.jsonl").lines() {
        let entry: serde_json::Value = serde_json::from_str(line).unwrap();
        for event in faber_engine::codex::parse_codex_notification(&entry["msg"], &mut state) {
            if matches!(event, EngineEvent::Done { .. }) {
                done = Some(event);
            }
        }
    }
    let event = done.expect("fixture must finish the actual interrupted turn");
    assert!(matches!(event, EngineEvent::Done { cost_total_usd: None, incomplete: true, .. }));
    assert_eq!(
        apply(&mut ledger, Change::Report { id: "i1".into(), event, terminated: true }, NOW, 0).unwrap(),
        Status::Stop("reason.budget.unknown")
    );
    let budget = Budget::load(&ledger).unwrap();
    assert!(!budget.invocations["i1"].settled);
    assert_eq!(budget.cost_view("m1").own_micros, None);
    assert!(apply(&mut ledger, Change::Continue { id: "i1".into() }, NOW, 0).is_err());
}

#[test]
fn mission_screen_cost_reads_the_live_cap_and_never_calls_unsettled_exposure_measured_spend() {
    let mut b = Budget::default();
    let unknown = b.cost_view("m1");
    assert_eq!(unknown.cap_micros, None);
    assert_eq!(unknown.own_micros, None);
    b.grants.insert("m1".into(), grant(700_000));
    b.invocations.insert(
        "i1".into(),
        faber_engine::project::budget::Reservation {
            request: request("i1", "m1", "w1", Kind::Worker, 50_000),
            started_at: NOW,
            base: None,
            rates: "date".into(),
            paid_micros: 12_345,
            settled: true,
            terminated: true,
            extra: Extra::new(),
        },
    );
    let measured = b.cost_view("m1");
    assert_eq!(measured.cap_micros, Some(700_000));
    assert_eq!(measured.own_micros, Some(12_345));
    assert!(measured.complete);
    b.invocations.get_mut("i1").unwrap().settled = false;
    let pending = b.cost_view("m1");
    assert_eq!(pending.own_micros, None);
    assert!(!pending.complete);
}

fn ledger() -> Ledger {
    let mut l = Ledger::default();
    for (vendor, model) in [(Vendor::Codex, MODEL), (Vendor::Claude, "claude-sonnet-5-5")] {
        let v = l.of_mut(vendor);
        v.billing = Billing::Subscription;
        v.paid_fallback = Some(false);
        v.paid_fallback_at = Some(NOW);
        for i in 0..=8 {
            let at = 2.0 * DAY - (8 - i) as f64 * 1800.0;
            v.record("seven_day", i as f64 * 0.03, RESET, at, 0);
            if i > 0 {
                v.span(Span { start: at - 1800.0, end: at, model: model.into(), cost: Some(0.25), incomplete: false });
            }
        }
        v.record("seven_day", 0.25, RESET, NOW, 0);
    }
    l
}

fn grant(cap: u64) -> Grant {
    Grant {
        cap_micros: cap,
        worker_micros: cap,
        vendors: BTreeMap::from([(Vendor::Codex, cap)]),
        review_micros: 20_000,
        repair_micros: 20_000,
        canary_micros: 20_000,
        paused: false,
        extra: Extra::new(),
    }
}

fn request(id: &str, mission: &str, worker: &str, kind: Kind, bound: u64) -> Invocation {
    Invocation {
        id: id.into(),
        mission: mission.into(),
        worker: worker.into(),
        kind,
        vendor: Vendor::Codex,
        model: MODEL.into(),
        effort: "medium".into(),
        session: format!("s-{id}"),
        fresh: true,
        exposure_micros: Some(bound),
        extra: Extra::new(),
    }
}

fn ready(cap: u64) -> Ledger {
    let mut l = ledger();
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }, NOW, 0).unwrap();
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: grant(cap) }, NOW, 0).unwrap();
    l
}

fn done(cost: Option<f64>, incomplete: bool) -> EngineEvent {
    EngineEvent::Done {
        ok: true,
        reason: "completed".into(),
        cost_total_usd: cost,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        cache_write_1h_tokens: 0,
        incomplete,
    }
}

fn report(l: &mut Ledger, id: &str, cost: Option<f64>, incomplete: bool, terminated: bool) -> Status {
    apply(l, Change::Report { id: id.into(), event: done(cost, incomplete), terminated }, NOW, 0).unwrap()
}

#[test]
fn successive_mission_grants_cannot_overbook_the_shared_daily_allocation() {
    let mut l = ready(250_000);
    let before = serde_json::to_value(&l).unwrap();
    assert!(apply(&mut l, Change::Grant { mission: "m2".into(), grant: grant(200_000) }, NOW, 0).is_err());
    assert_eq!(serde_json::to_value(&l).unwrap(), before, "denial must not consume authority");
    apply(&mut l, Change::Grant { mission: "m2".into(), grant: grant(150_000) }, NOW, 0).unwrap();
}

#[test]
fn writers_cannot_borrow_review_repair_or_canary_reservations() {
    let mut l = ready(200_000);
    assert!(apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 150_000)), NOW, 0).is_err());
    apply(&mut l, Change::Admit(request("i2", "m1", "w1", Kind::Worker, 140_000)), NOW, 0).unwrap();
    assert!(apply(&mut l, Change::Admit(request("i3", "m1", "w2", Kind::Worker, 1)), NOW, 0).is_err());
    for (id, kind) in [("r", Kind::Review), ("fix", Kind::Repair), ("c", Kind::Canary)] {
        apply(&mut l, Change::Admit(request(id, "m1", id, kind, 20_000)), NOW, 0).unwrap();
    }
}

#[test]
fn every_spend_path_is_charged_once_through_the_existing_usage_ledger() {
    let mut l = ready(300_000);
    for (i, kind) in [
        Kind::Lead,
        Kind::Worker,
        Kind::Router,
        Kind::Summary,
        Kind::Review,
        Kind::Repair,
        Kind::Canary,
        Kind::Integration,
        Kind::Diagnostic,
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("i{i}");
        apply(&mut l, Change::Admit(request(&id, "m1", &id, kind, 10_000)), NOW, 0).unwrap();
        assert_eq!(report(&mut l, &id, Some(0.005), false, true), Status::Ready);
        report(&mut l, &id, Some(0.005), false, true);
    }
    assert!((l.codex.own_spend[&2] - 0.045).abs() < 1e-9);
    assert_eq!(l.codex.spans.len(), 17);
    assert!(Budget::load(&l).unwrap().invocations.values().all(|r| r.settled));
}

#[test]
fn late_unknown_and_live_cost_stay_reserved_across_stop_and_restart() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 140_000)), NOW, 0).unwrap();
    report(&mut l, "i1", None, true, true);
    apply(&mut l, Change::Pause { mission: "m1".into(), paused: true }, NOW, 0).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.save_ledger(&l).unwrap();
    let mut restored = store.ledger_checked().unwrap();
    assert!(!Budget::load(&restored).unwrap().invocations["i1"].settled);
    assert_eq!(Budget::load(&restored).unwrap().invocations["i1"].request.exposure_micros, Some(140_000));
    assert!(apply(&mut restored, Change::Continue { id: "i1".into() }, NOW, 0).is_err());
    restored.codex.billing = Billing::Subscription;
    let mut smaller = grant(60_000);
    smaller.review_micros = 0;
    smaller.repair_micros = 0;
    smaller.canary_micros = 0;
    apply(&mut restored, Change::Grant { mission: "m1".into(), grant: smaller }, NOW, 0).unwrap();
    assert!(apply(&mut restored, Change::Grant { mission: "m2".into(), grant: grant(300_000) }, NOW, 0).is_err());
    report(&mut restored, "i1", Some(0.01), false, false);
    assert!(!Budget::load(&restored).unwrap().invocations["i1"].settled, "cost alone cannot free a live process");
    report(&mut restored, "i1", Some(0.01), false, true);
    assert!(Budget::load(&restored).unwrap().invocations["i1"].settled);
    assert!((restored.codex.own_spend[&2] - 0.01).abs() < 1e-9);
}

#[test]
fn outside_use_chat_spend_and_smaller_caps_close_every_continuation() {
    for case in 0..3 {
        let mut l = ready(200_000);
        apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
        match case {
            0 => l.codex.record("seven_day", 0.425, RESET, NOW, 0),
            1 => l.codex.spend(Some(0.3), false, NOW, 0),
            _ => {
                let mut g = grant(90_000);
                g.worker_micros = 50_000;
                apply(&mut l, Change::Grant { mission: "m1".into(), grant: g }, NOW, 0).unwrap();
            }
        }
        assert!(apply(&mut l, Change::Continue { id: "i1".into() }, NOW, 0).is_err(), "case {case}");
        assert!(!Budget::load(&l).unwrap().invocations["i1"].settled);
    }
}

#[test]
fn absent_evidence_cannot_grant_free_work() {
    for case in 0..7 {
        let mut l = ledger();
        match case {
            0 => l.codex.billing = Billing::Unknown,
            1 => l.codex.paid_fallback = None,
            2 => l.codex.history.clear(),
            3 => l.codex.spans.iter_mut().for_each(|s| s.incomplete = true),
            4 => l.codex.record("seven_day", 0.26, RESET, NOW - 7.0 * 3600.0, 0),
            5 => l.codex.unmetered.insert(2, 1).map(|_| ()).unwrap_or(()),
            _ => l.codex.record("seven_day", 0.26, RESET + DAY, NOW, 0),
        }
        assert!(apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 100_000 }, NOW, 0).is_err(), "case {case}");
    }
    let mut l = ready(200_000);
    for case in 0..4 {
        let mut r = request(&format!("x{case}"), "m1", "w1", Kind::Worker, 10_000);
        match case {
            0 => r.exposure_micros = None,
            1 => r.exposure_micros = Some(0),
            2 => r.model = "unpriced-model".into(),
            _ => r.vendor = Vendor::Claude,
        }
        assert!(apply(&mut l, Change::Admit(r), NOW, 0).is_err(), "request {case}");
    }
}

#[test]
fn clock_reset_and_new_day_do_not_refill_a_lifetime_grant() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    assert!(apply(&mut l, Change::Continue { id: "i1".into() }, NOW - 1.0, 0).is_err());
    l.codex.record("seven_day", 0.28, RESET, 3.0 * DAY, 0);
    assert!(apply(&mut l, Change::Continue { id: "i1".into() }, 3.0 * DAY, 0).is_err());
    l.codex.record("seven_day", 0.01, RESET + 7.0 * DAY, RESET + 10.0, 0);
    assert!(apply(&mut l, Change::Continue { id: "i1".into() }, RESET + 10.0, 0).is_err());
    assert_eq!(Budget::load(&l).unwrap().grants["m1"].cap_micros, 200_000);
    assert_eq!(Budget::load(&l).unwrap().invocations["i1"].request.exposure_micros, Some(100_000));
}

#[test]
fn a_breach_charges_real_cost_and_disables_the_driver_configuration() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
    assert_eq!(report(&mut l, "i1", Some(0.02), false, true), Status::Stop("reason.budget.breach"));
    assert!((l.codex.own_spend[&2] - 0.02).abs() < 1e-9);
    assert!(apply(&mut l, Change::Admit(request("i2", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).is_err());
    let restored: Ledger = serde_json::from_value(serde_json::to_value(&l).unwrap()).unwrap();
    assert!(!Budget::load(&restored).unwrap().disabled.is_empty());
}

#[test]
fn duplicate_ids_cannot_replay_dispatch_or_move_spend_to_another_worker() {
    let mut l = ready(200_000);
    let r = request("i1", "m1", "w1", Kind::Worker, 10_000);
    apply(&mut l, Change::Admit(r.clone()), NOW, 0).unwrap();
    assert!(apply(&mut l, Change::Admit(r), NOW, 0).is_err());
    report(&mut l, "i1", Some(0.005), false, true);
    assert!(apply(&mut l, Change::Admit(request("i1", "m1", "w2", Kind::Worker, 10_000)), NOW, 0).is_err());
}

#[test]
fn session_resume_uses_the_saved_cumulative_base_instead_of_charging_it_twice() {
    let mut l = ledger();
    apply(&mut l, Change::Allocate { vendor: Vendor::Claude, cap_micros: 400_000 }, NOW, 0).unwrap();
    let mut g = grant(200_000);
    g.vendors = BTreeMap::from([(Vendor::Claude, 200_000)]);
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: g }, NOW, 0).unwrap();
    let mut r = request("i1", "m1", "w1", Kind::Worker, 50_000);
    r.vendor = Vendor::Claude;
    r.model = "claude-sonnet-5-5".into();
    r.session = "session".into();
    apply(&mut l, Change::Admit(r.clone()), NOW, 0).unwrap();
    report(&mut l, "i1", Some(0.03), true, true);
    r.id = "i2".into();
    r.fresh = false;
    apply(&mut l, Change::Admit(r), NOW, 0).unwrap();
    report(&mut l, "i2", Some(0.04), true, true);
    assert!((l.claude.own_spend[&2] - 0.04).abs() < 2e-6, "native totals include cache and nested work");
    assert_eq!(Budget::load(&l).unwrap().invocations["i2"].paid_micros, 10_001);
}

#[test]
fn shorter_family_windows_and_stale_readings_reduce_eligibility() {
    let mut l = ready(200_000);
    l.codex.record("300_minutes", 0.99, NOW + 3600.0, NOW, 0);
    assert!(apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).is_err());
    let mut l = ledger();
    apply(&mut l, Change::Allocate { vendor: Vendor::Claude, cap_micros: 400_000 }, NOW, 0).unwrap();
    let mut g = grant(200_000);
    g.vendors = BTreeMap::from([(Vendor::Claude, 200_000)]);
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: g }, NOW, 0).unwrap();
    l.claude.record("seven_day_sonnet", 1.0, RESET, NOW, 0);
    let mut r = request("i1", "m1", "w1", Kind::Worker, 10_000);
    r.vendor = Vendor::Claude;
    r.model = "claude-sonnet-5-5".into();
    assert!(apply(&mut l, Change::Admit(r), NOW, 0).is_err());
}

#[test]
fn daily_evidence_and_new_fields_survive_old_reader_round_trips() {
    let l = ready(200_000);
    let mut value = serde_json::to_value(&l).unwrap();
    value["missionBudget"]["future"] = serde_json::json!({"pinned": true});
    let old: Ledger = serde_json::from_value(value).unwrap();
    assert_eq!(serde_json::to_value(&old).unwrap()["missionBudget"]["future"]["pinned"], true);
    assert!(Budget::load(&old).is_ok());
}

#[test]
fn malformed_budget_authority_fails_closed() {
    for value in [serde_json::json!({}), serde_json::json!({"schema": 999}), serde_json::Value::Null] {
        let mut l = ledger();
        l.extra.insert("missionBudget".into(), value);
        assert!(Budget::load(&l).is_err());
        assert!(apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 10_000 }, NOW, 0).is_err());
    }
}

// Calibration evidence is taken from the existing points/spans, never from an invented capacity.
#[test]
fn an_empty_day_without_a_boundary_reading_is_not_a_budget() {
    let mut l = Ledger::default();
    l.codex.billing = Billing::Subscription;
    l.codex.paid_fallback = Some(false);
    l.codex.record("seven_day", 0.01, RESET, NOW, 0);
    l.codex.weights.capacity = Some(1_000_000.0);
    l.codex.history.insert("seven_day".into(), vec![Point { used: 0.01, at: NOW, resets_at: RESET }]);
    assert!(apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 1 }, NOW, 0).is_err());
}

#[test]
fn a_smaller_daily_cap_takes_effect_even_with_outstanding_reservations() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 50_000 }, NOW, 0).unwrap();
    assert_eq!(Budget::load(&l).unwrap().allocations[&Vendor::Codex].cap_micros, 50_000);
    assert!(apply(&mut l, Change::Continue { id: "i1".into() }, NOW, 0).is_err());
    assert!(!Budget::load(&l).unwrap().invocations["i1"].settled);
}

#[test]
fn a_late_higher_final_cost_cannot_disappear_behind_idempotency() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
    report(&mut l, "i1", Some(0.005), false, true);
    assert_eq!(report(&mut l, "i1", Some(0.02), false, true), Status::Stop("reason.budget.breach"));
    report(&mut l, "i1", Some(0.02), false, true);
    assert!((l.codex.own_spend[&2] - 0.02).abs() < 1e-9);
    assert!(apply(&mut l, Change::Admit(request("i2", "m1", "w2", Kind::Worker, 1)), NOW, 0).is_err());
}

#[test]
fn damaged_spend_or_budget_records_are_quarantined_before_sanitizing() {
    for case in 0..2 {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let mut l = ready(200_000);
        if case == 0 {
            l.codex.own_spend.insert(2, -1.0);
        } else {
            l.extra.insert("missionBudget".into(), serde_json::json!({}));
        }
        store.save_ledger(&l).unwrap();
        assert!(store.ledger_checked().is_err(), "case {case}");
        assert!(dir.path().join("budget.json.quarantine").is_file());
    }
}

#[test]
fn clock_reversal_during_settlement_cannot_hide_today_spend() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 200_000 }, NOW, 0).unwrap();
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    apply(&mut l, Change::Report { id: "i1".into(), event: done(Some(0.1), false), terminated: true }, NOW - DAY, 0).unwrap();
    assert_eq!(l.codex.own_spend.get(&2), Some(&0.1));
    assert!(!l.codex.own_spend.contains_key(&1));
    assert!(apply(&mut l, Change::Grant { mission: "m2".into(), grant: grant(100_000) }, NOW, 0).is_err());
}

#[test]
fn a_local_offset_change_cannot_make_recorded_spend_disappear() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 200_000 }, NOW, 0).unwrap();
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    apply(&mut l, Change::Report { id: "i1".into(), event: done(Some(0.1), false), terminated: true }, NOW, -14 * 3600).unwrap();
    assert_eq!(l.codex.own_spend.get(&1), Some(&0.1));
    assert!(apply(&mut l, Change::Grant { mission: "m2".into(), grant: grant(100_000) }, NOW, 0).is_err());
}

#[test]
fn five_hour_calibration_survives_a_reset_but_not_a_plan_change() {
    let mut l = ledger();
    for i in 0..=8 {
        let at = 2.0 * DAY - (8 - i) as f64 * 1800.0;
        l.codex.record("five_hour", i as f64 * 0.03, 2.0 * DAY + 100.0, at, 0);
    }
    l.codex.record("five_hour", 0.01, NOW + 18_000.0, NOW, 0);
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }, NOW, 0).unwrap();
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: grant(200_000) }, NOW, 0).unwrap();
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
    l.codex.since = NOW;
    assert!(apply(&mut l, Change::Admit(request("i2", "m1", "w2", Kind::Worker, 10_000)), NOW, 0).is_err());
}

#[test]
fn shorter_window_dollar_room_is_reached_before_the_quota_gate() {
    let mut l = ready(200_000);
    let points = l.codex.history["seven_day"].clone();
    l.codex.history.insert("five_hour".into(), points);
    l.codex.record("five_hour", 0.969, RESET, NOW, 0);
    assert_eq!(apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0), Err("reason.budget.daily"));
    l.codex.record("five_hour", 0.94, RESET, NOW, 0);
    apply(&mut l, Change::Admit(request("i2", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
}

#[test]
fn an_allocation_itself_cannot_exceed_measured_room() {
    let mut l = ledger();
    assert_eq!(apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 2_000_000 }, NOW, 0), Err("reason.budget.daily"));
    assert!(Budget::load(&l).unwrap().allocations.is_empty());
}

#[test]
fn refreshing_quota_without_paid_fallback_evidence_does_not_refresh_r86() {
    let l = ready(200_000);
    let mut value = serde_json::to_value(l).unwrap();
    value["codex"]["paid_fallback_at"] = serde_json::json!(NOW - 7.0 * 3600.0);
    let mut l: Ledger = serde_json::from_value(value).unwrap();
    l.codex.billing = Billing::Subscription;
    l.codex.record("seven_day", 0.25, RESET, NOW, 0);
    assert_eq!(apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0), Err("reason.budget.billing"));
}

#[test]
fn enabling_paid_fallback_closes_admission_and_live_continuation() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
    l.codex.paid_fallback = Some(true);
    assert_eq!(apply(&mut l, Change::Continue { id: "i1".into() }, NOW, 0), Err("reason.budget.billing"));
    assert_eq!(apply(&mut l, Change::Admit(request("i2", "m1", "w2", Kind::Worker, 10_000)), NOW, 0), Err("reason.budget.billing"));
}

#[test]
fn a_grant_update_cannot_override_the_persons_pause() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Pause { mission: "m1".into(), paused: true }, NOW, 0).unwrap();
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: grant(250_000) }, NOW, 0).unwrap();
    assert_eq!(apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0), Err("reason.budget.paused"));
    apply(&mut l, Change::Pause { mission: "m1".into(), paused: false }, NOW, 0).unwrap();
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
}

#[test]
fn an_error_records_termination_without_freeing_unknown_exposure() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
    let event = EngineEvent::Error { message: "error.cli.interruptTimeout".into(), fatal: true };
    assert_eq!(
        apply(&mut l, Change::Report { id: "i1".into(), event, terminated: true }, NOW, 0).unwrap(),
        Status::Stop("reason.budget.unknown")
    );
    let b = Budget::load(&l).unwrap();
    assert!(b.invocations["i1"].terminated);
    assert!(!b.invocations["i1"].settled);
    assert!(apply(&mut l, Change::Continue { id: "i1".into() }, NOW, 0).is_err());
}

#[test]
fn known_live_cost_holds_exposure_without_claiming_it_is_unknown() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
    assert_eq!(report(&mut l, "i1", Some(0.005), false, false), Status::Ready);
    assert!(!Budget::load(&l).unwrap().invocations["i1"].settled);
    assert_eq!(report(&mut l, "i1", None, true, false), Status::Stop("reason.budget.unknown"));
}

#[test]
fn pruned_old_unknown_cost_does_not_invalidate_a_current_baseline() {
    let mut l = ledger();
    l.codex.unmetered.insert(-40, 1);
    l.codex.day_baselines.get_mut("seven_day").unwrap().unmetered.insert(-40, 1);
    l.codex.spend(Some(0.001), false, NOW, 0);
    assert!(!l.codex.unmetered.contains_key(&-40));
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 100_000 }, NOW, 0).unwrap();
    l.codex.day_baselines.get_mut("seven_day").unwrap().unmetered.insert(2, 1);
    assert_eq!(apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 200_000 }, NOW, 0), Err("reason.budget.unknown"));
}

#[test]
fn a_late_report_with_fresh_quota_books_the_actual_day_and_span() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
    let later = 3.0 * DAY + 3600.0;
    l.codex.record("seven_day", 0.26, RESET, later, 0);
    apply(&mut l, Change::Report { id: "i1".into(), event: done(Some(0.005), false), terminated: true }, later, 0).unwrap();
    assert_eq!(l.codex.own_spend.get(&3), Some(&0.005));
    assert!(!l.codex.own_spend.contains_key(&2));
    assert_eq!(l.codex.spans.last().unwrap().end, later);
}

#[test]
fn future_report_or_pause_does_not_poison_the_admission_clock() {
    for pause in [false, true] {
        let mut l = ready(200_000);
        apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).unwrap();
        if pause {
            apply(&mut l, Change::Pause { mission: "m1".into(), paused: false }, RESET * 10.0, 0).unwrap();
        } else {
            apply(&mut l, Change::Report { id: "i1".into(), event: done(Some(0.005), false), terminated: true }, RESET * 10.0, 0).unwrap();
            assert_eq!(l.codex.own_spend.get(&2), Some(&0.005));
            assert_eq!(l.codex.own_spend.len(), 1, "a future clock cannot move settlement into a future day");
        }
        apply(&mut l, Change::Admit(request("i2", "m1", "w2", Kind::Worker, 10_000)), NOW, 0).unwrap();
    }
}

#[test]
fn a_new_day_allocation_keeps_worker_lifetime_cost_and_retry_history() {
    let mut l = ledger();
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }, NOW, 0).unwrap();
    let mut g = grant(200_000);
    g.worker_micros = 120_000;
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: g }, NOW, 0).unwrap();
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    report(&mut l, "i1", Some(0.1), false, true);
    l.codex.record("seven_day", 0.26, RESET, 3.0 * DAY, 0);
    l.codex.paid_fallback_at = Some(3.0 * DAY);
    apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }, 3.0 * DAY, 0).unwrap();
    assert!(apply(&mut l, Change::Admit(request("i2", "m1", "w1", Kind::Worker, 20_001)), 3.0 * DAY, 0).is_err());
    apply(&mut l, Change::Admit(request("i3", "m1", "w2", Kind::Worker, 20_000)), 3.0 * DAY, 0).unwrap();
    assert_eq!(Budget::load(&l).unwrap().invocations["i1"].paid_micros, 100_000);
}

#[test]
fn vendor_handover_keeps_one_mission_and_worker_lifetime_cap() {
    let mut l = ledger();
    for vendor in Vendor::ALL {
        apply(&mut l, Change::Allocate { vendor, cap_micros: 400_000 }, NOW, 0).unwrap();
    }
    let mut g = grant(300_000);
    g.worker_micros = 60_000;
    g.vendors = BTreeMap::from([(Vendor::Codex, 150_000), (Vendor::Claude, 150_000)]);
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: g }, NOW, 0).unwrap();
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 50_000)), NOW, 0).unwrap();
    report(&mut l, "i1", Some(0.05), false, true);
    let mut r = request("i2", "m1", "w1", Kind::Worker, 10_001);
    r.vendor = Vendor::Claude;
    r.model = "claude-sonnet-5-5".into();
    assert!(apply(&mut l, Change::Admit(r.clone()), NOW, 0).is_err());
    r.exposure_micros = Some(10_000);
    apply(&mut l, Change::Admit(r), NOW, 0).unwrap();
    report(&mut l, "i2", Some(0.01), true, true);
    assert!((l.codex.own_spend[&2] + l.claude.own_spend[&2] - 0.06).abs() < 1e-9);
}

#[test]
fn locally_priced_cache_counters_use_the_existing_rate_table_once() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 5_000)), NOW, 0).unwrap();
    let event = EngineEvent::Done {
        ok: true,
        reason: "completed".into(),
        cost_total_usd: None,
        input_tokens: 1_000,
        output_tokens: 100,
        cache_read_tokens: 600,
        cache_write_tokens: 200,
        cache_write_1h_tokens: 0,
        incomplete: false,
    };
    apply(&mut l, Change::Report { id: "i1".into(), event: event.clone(), terminated: true }, NOW, 0).unwrap();
    apply(&mut l, Change::Report { id: "i1".into(), event, terminated: true }, NOW, 0).unwrap();
    assert!((l.codex.own_spend[&2] - 0.00196).abs() < 1e-6);
    assert!(Budget::load(&l).unwrap().invocations["i1"].settled);
}

#[test]
fn a_resumed_session_without_its_cost_base_keeps_the_full_reservation() {
    let mut l = ledger();
    apply(&mut l, Change::Allocate { vendor: Vendor::Claude, cap_micros: 400_000 }, NOW, 0).unwrap();
    let mut g = grant(200_000);
    g.vendors = BTreeMap::from([(Vendor::Claude, 200_000)]);
    apply(&mut l, Change::Grant { mission: "m1".into(), grant: g }, NOW, 0).unwrap();
    let mut r = request("i1", "m1", "w1", Kind::Worker, 140_000);
    r.vendor = Vendor::Claude;
    r.model = "claude-sonnet-5-5".into();
    r.fresh = false;
    apply(&mut l, Change::Admit(r), NOW, 0).unwrap();
    assert_eq!(report(&mut l, "i1", Some(5.0), true, true), Status::Stop("reason.budget.unknown"));
    assert!(!Budget::load(&l).unwrap().invocations["i1"].settled);
    assert_eq!(Budget::load(&l).unwrap().invocations["i1"].paid_micros, 0);
}

#[test]
fn boundary_evidence_is_not_refilled_by_a_second_reading_in_the_same_second() {
    let mut l = ledger();
    l.codex.record("seven_day", 0.34, RESET, 2.0 * DAY, 0);
    assert_eq!(l.codex.day_baselines["seven_day"].reading.used, 0.24);
    assert!(apply(&mut l, Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }, 2.0 * DAY, 0).is_err());
}

#[test]
fn a_rate_pin_change_keeps_unsettled_exposure_instead_of_repricing_it() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    l.extra.get_mut("missionBudget").unwrap()["invocations"]["i1"]["rates"] = "older-rates".into();
    assert_eq!(report(&mut l, "i1", Some(0.005), false, true), Status::Stop("reason.budget.rate"));
    assert!(!Budget::load(&l).unwrap().invocations["i1"].settled);
}

#[test]
fn an_unknown_terminal_cost_cannot_reuse_the_same_invocation_allowance() {
    let mut l = ready(200_000);
    apply(&mut l, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 100_000)), NOW, 0).unwrap();
    report(&mut l, "i1", None, true, true);
    assert!(apply(&mut l, Change::Continue { id: "i1".into() }, NOW, 0).is_err());
    assert!(!Budget::load(&l).unwrap().invocations["i1"].settled);
}

#[test]
fn separate_turns_in_one_session_have_separate_once_only_receipts() {
    let mut l = ready(250_000);
    for (id, cost) in [("i1", 0.05), ("i2", 0.07)] {
        let mut r = request(id, "m1", "w1", Kind::Worker, 100_000);
        r.session = "same-session".into();
        r.fresh = id == "i1";
        apply(&mut l, Change::Admit(r), NOW, 0).unwrap();
        report(&mut l, id, Some(cost), false, true);
        report(&mut l, id, Some(cost), false, true);
        assert!(apply(&mut l, Change::Continue { id: id.into() }, NOW, 0).is_err());
    }
    assert!((l.codex.own_spend[&2] - 0.12).abs() < 1e-9);
    assert_eq!(Budget::load(&l).unwrap().invocations.len(), 2);
}

#[test]
fn crashed_chat_is_reconciled_once_then_a_fresh_day_can_be_allocated() {
    use faber_engine::project::budget::{recover_unbounded, start_unbounded};
    let mut l = ready(100_000);
    start_unbounded(&mut l, Vendor::Codex, NOW).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.save_ledger(&l).unwrap();
    let mut saved = store.ledger_checked().unwrap();
    saved.codex.billing = Billing::Subscription;
    assert!(recover_unbounded(&mut saved).unwrap());
    assert!(!recover_unbounded(&mut saved).unwrap());
    assert_eq!(Budget::load(&saved).unwrap().unbounded[&Vendor::Codex], 0);
    assert_eq!(saved.codex.unmetered[&2], 1);
    assert!(apply(&mut saved, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), NOW, 0).is_err());
    saved.codex.record("seven_day", 0.26, RESET, 3.0 * DAY, 0);
    saved.codex.paid_fallback_at = Some(3.0 * DAY);
    apply(&mut saved, Change::Allocate { vendor: Vendor::Codex, cap_micros: 400_000 }, 3.0 * DAY, 0).unwrap();
    apply(&mut saved, Change::Admit(request("i1", "m1", "w1", Kind::Worker, 10_000)), 3.0 * DAY, 0).unwrap();
}
