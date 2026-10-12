use faber_engine::budget::Billing;
use faber_engine::cli::Cli;
use faber_engine::ledger::Ledger;
use faber_engine::model::Vendor;
use faber_engine::store::Store;
use faber_engine::{Engine, EngineOptions};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

const WHEN: f64 = 1_791_620_000.0;
fn engine(dir: &std::path::Path) -> Engine {
    let e = Engine::new(
        EngineOptions {
            data_dir: dir.into(),
            clis: BTreeMap::from([(Vendor::Claude, Cli { command: "claude".into(), prefix: vec![] })]),
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no process in this test"))),
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(30),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    e.set_time_source(Arc::new(|| WHEN));
    e
}
#[tokio::test]
async fn the_usage_view_separates_own_money_from_foreign_account_use() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = Ledger::default();
    let v = &mut ledger.claude;
    v.billing = Billing::Subscription;
    v.paid_fallback = Some(false);
    v.spend(Some(1.25), false, WHEN, 0);
    v.record("five_hour", 0.3, WHEN + 9000.0, WHEN - 3600.0, 0);
    v.record("five_hour", 0.35, WHEN + 9000.0, WHEN, 0);
    v.learned = Default::default();
    Store::new(dir.path()).save_ledger(&ledger);
    let e = engine(dir.path());
    let u = e.state().usage.remove(0);
    assert_eq!(u.own_spend_usd, 1.25);
    assert!(u.foreign_use);
    assert_eq!(u.outside_share, Some(1.0));
    assert_eq!(u.windows[0].used, 0.35);
    assert!((u.windows[0].expected - 0.5).abs() < 1e-9);
    e.shutdown();
}
#[tokio::test]
async fn a_typed_plan_cannot_make_unknown_billing_eligible_or_hide_the_auth_hint() {
    let dir = tempfile::tempdir().unwrap();
    let e = engine(dir.path());
    e.answer_plan(Vendor::Claude, "max20x");
    let u = e.state().usage.remove(0);
    assert_eq!(u.plan.as_deref(), Some("max20x"));
    assert!(u.billing_unknown);
    assert_eq!(u.state, "unknown");
    assert!(!u.ask_for_plan);
    assert!(!u.billed_per_use);
    e.shutdown();
}
