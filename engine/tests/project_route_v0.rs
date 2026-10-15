use crate::support;
use faber_engine::model::Vendor;
use faber_engine::project::Role;
use faber_engine::project::run::{RouteStep, RouteV0};
use faber_engine::project::store::Journal;
use faber_engine::project::worker::ModelPin;
use support::tempdir;
#[test]
fn route_v0_keeps_typed_pins_cost_and_receipts_and_never_becomes_a_plan() {
    let repo = tempdir();
    let data = tempdir();
    let j = Journal::open(repo.path(), data.path(), "m1").unwrap();
    let plan = faber_engine::project::Plan::new(
        "m1",
        7,
        vec![faber_engine::project::Task::new("keep", "Existing plan", faber_engine::project::TaskKind::Research)],
    );
    j.save_plan(&plan).unwrap();
    let before = std::fs::read(j.records().join("plan.json")).unwrap();
    let mut route = RouteV0 {
        schema: 1,
        shape: "faber.code-change".into(),
        fixture: true,
        steps: vec![RouteStep {
            task: "w1".into(),
            role: Role::Writer,
            pin: ModelPin { vendor: Vendor::Codex, model: "gpt-6.1-sol".into(), effort: "medium".into(), cli_version: "0.162.1".into() },
            cost_micros: None,
            receipts: vec!["missing".into()],
        }],
    };
    assert!(route.record(&j).is_err());
    assert_eq!(std::fs::read(j.records().join("plan.json")).unwrap(), before);
    use faber_engine::project::{Intent, IntentKind, Outcome, Receipt};
    j.record_intent(&Intent::new("seen", "m1", IntentKind::Dispatch, 3, 1)).unwrap();
    j.record_receipt(&Receipt::new("seen", Outcome::Done, 2)).unwrap();
    route.steps[0].receipts = vec!["seen".into()];
    route.record(&j).unwrap();
    let report = std::fs::read_to_string(j.records().join("route-v0.json")).unwrap();
    assert!(report.contains("faber.code-change"));
    assert!(report.contains("cliVersion"));
    assert!(report.contains("receipts"));
    assert_eq!(std::fs::read(j.records().join("plan.json")).unwrap(), before);
    assert_eq!(j.plan().unwrap().unwrap(), plan);
}
