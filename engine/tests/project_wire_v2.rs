use faber_engine::project::wire::{self, Envelope, Expect, WireError};
use faber_engine::project::{Role, SCHEMA};
use serde_json::{Value, json};

fn expect() -> Expect {
    Expect { task: "a".into(), attempt: 1, base: "tree".into(), contract: "contract".into(), lease: 0 }
}

fn packet(action: Value) -> Value {
    json!({"schema": 2, "task": "a", "attempt": 1, "base": "tree", "contract": "contract", "lease": 0,
        "ping": "Found", "requires": ["R1"], "checks": ["C1"], "quotes": ["Q1"], "note": "Ready for proof.", "action": action})
}

#[test]
fn one_additive_wire_revision_keeps_the_durable_journal_version() {
    assert_eq!(wire::SCHEMA, 2);
    assert_eq!(SCHEMA, 1);
    let value = packet(json!({"complete": {"summary": "Ready", "next": "review", "receipt": "receipt1",
        "kitUsed": ["card1"], "needs": ["framework:tauri!"], "unconfirmed": ["No live session"],
        "assumptionRecords": [{"id": "A1", "text": "Personal use", "source": "request", "confirmed": false}]}}));
    let parsed = Envelope::parse(&value.to_string(), &expect(), Role::Writer).unwrap();
    let encoded = serde_json::to_value(parsed).unwrap();
    assert_eq!(encoded["ping"], "Found");
    assert_eq!(encoded["action"]["complete"]["receipt"], "receipt1");
    assert_eq!(encoded["action"]["complete"]["unconfirmed"], value["action"]["complete"]["unconfirmed"]);
    let mut old = value;
    old["schema"] = json!(1);
    assert_eq!(Envelope::parse(&old.to_string(), &expect(), Role::Writer), Err(WireError::Schema));
}

#[test]
fn labels_and_bar_sheet_round_trip_without_turning_requirements_into_dependencies() {
    let mut value = packet(json!({"plan": {"summary": "s", "acceptance": "a", "tasks": [{"id": "a", "title": "A", "kind": "writer",
        "labels": {"verb": "build", "needs": ["ui:component!"], "risk": "low", "fresh": "stable"},
        "checks": ["C1"], "paths": ["ui/App.tsx"], "kit": {"receipt": "a-1", "digest": "0".repeat(64)}}],
        "sheet": {"pieces": [{"id": "screen", "type": "desktop-app", "livesIn": "720 px", "paths": ["ui/App.tsx"]}],
            "facets": {"purpose": "personal", "audience": "self", "deliverable": "desktop", "stage": "build", "region": "BR"},
            "risk": {"public": false, "othersData": false, "money": false, "minors": false, "regulated": false, "commercial": false, "irreversible": []}},
        "bar": [{"id": "C1", "piece": "screen", "attribute": "keyboard", "check": "Keep keyboard focus", "kind": "program", "severity": "blocking",
            "ground": {"kind": "owner", "reference": "R1"}, "proof": "screen", "owner": "a", "paths": ["ui/App.tsx"]}]}}));
    value["ping"] = json!("Order");
    let incomplete = value.clone();
    assert!(Envelope::parse(&incomplete.to_string(), &expect(), Role::Lead).is_err());
    let sheet: wire::DeliverableSheet = serde_json::from_value(value["action"]["plan"]["sheet"].clone()).unwrap();
    let mut bar = faber_engine::project::bar::software_bar(&sheet);
    let mut tasks = Vec::new();
    for (i, check) in bar.iter_mut().enumerate() {
        let id = if i == 0 { "a".to_string() } else { format!("proof-{i}") };
        check.owner = id.clone();
        let mut task = value["action"]["plan"]["tasks"][0].clone();
        task["id"] = json!(id);
        task["checks"] = json!([check.id]);
        task["proofs"] = json!([check.proof]);
        tasks.push(task);
    }
    value["action"]["plan"]["tasks"] = json!(tasks);
    value["action"]["plan"]["bar"] = json!(bar);
    let env = Envelope::parse(&value.to_string(), &expect(), Role::Lead).unwrap();
    if let wire::Action::Plan(plan) = env.action {
        let task = plan.tasks[0].to_task();
        assert!(task.requires.is_empty());
        assert_eq!(serde_json::to_value(task).unwrap()["labels"]["needs"][0], "ui:component!");
        assert!(!plan.bar.is_empty());
    } else {
        panic!("not a plan")
    }
}

#[test]
fn nested_unknown_fields_paths_labels_and_notes_fail_closed() {
    let base = packet(json!({"complete": {"summary": "s", "next": "n"}}));
    for (field, bad) in [("ping", json!("Install")), ("note", json!("word ".repeat(61))), ("requires", json!(["../R1"]))] {
        let mut value = base.clone();
        value[field] = bad;
        assert!(Envelope::parse(&value.to_string(), &expect(), Role::Writer).is_err(), "{field}");
    }
    let mut value = base.clone();
    value["action"]["complete"]["assumptionRecords"] =
        json!([{"id": "A1", "text": "x", "source": "request", "confirmed": false, "grant": true}]);
    assert_eq!(Envelope::parse(&value.to_string(), &expect(), Role::Writer), Err(WireError::Malformed));
    value = base;
    value["action"]["complete"]["artifacts"] = json!([{"path": "../secret", "sha256": "0".repeat(64)}]);
    assert_eq!(Envelope::parse(&value.to_string(), &expect(), Role::Writer), Err(WireError::Path));
}

#[test]
fn old_durable_plan_fields_survive_without_new_authority() {
    let raw = json!({"schema":1,"mission":"m1","revision":1,"tasks":[{"id":"a","title":"A","kind":"research","future":{"keep":true}}],"futurePlan":{"keep":true}});
    let plan: faber_engine::project::Plan = serde_json::from_value(raw).unwrap();
    plan.validate().unwrap();
    let back = serde_json::to_value(plan).unwrap();
    assert_eq!(back["schema"], 1);
    assert_eq!(back["futurePlan"]["keep"], true);
    assert_eq!(back["tasks"][0]["future"]["keep"], true);
    assert!(back["tasks"][0].get("kit").is_none());
}

#[test]
fn check_receipt_claims_cannot_infer_a_pass_without_evidence() {
    let mut value = packet(
        json!({"complete":{"summary":"s","next":"review","receipt":"r1","checks":[{"id":"screen.keyboard","kind":"program","result":"pass","evidence":[]}]}}),
    );
    assert_eq!(Envelope::parse(&value.to_string(), &expect(), Role::Writer), Err(WireError::Malformed));
    value["action"]["complete"]["checks"][0]["result"] = json!("notRun");
    assert_eq!(Envelope::parse(&value.to_string(), &expect(), Role::Writer), Err(WireError::Malformed));
    value["action"]["complete"]["checks"][0]["reason"] = json!("No hidden app evidence");
    let env = Envelope::parse(&value.to_string(), &expect(), Role::Writer).unwrap();
    let back = serde_json::to_value(env).unwrap();
    assert_eq!(back["action"]["complete"]["checks"][0]["result"], "notRun");
    value["action"]["complete"]["checks"][0]["reason"] = json!(null);
    value["action"]["complete"]["checks"][0]["result"] = json!("pass");
    value["action"]["complete"]["checks"][0]["evidence"] = json!([{"path":"evidence/keyboard.json","sha256":"0".repeat(64)}]);
    assert!(Envelope::parse(&value.to_string(), &expect(), Role::Writer).is_ok());
}

#[test]
fn label_limits_and_nested_unknown_kit_fields_remain_strict() {
    let base = packet(
        json!({"plan":{"summary":"s","acceptance":"a","tasks":[{"id":"a","title":"A","kind":"research","labels":{"verb":"research","needs":["lang:rust"],"risk":"low","fresh":"stable"}}]}}),
    );
    for bad in [json!(["not-a-need"]), json!(["lang:rust!!"]), json!(vec!["lang:rust"; 9])] {
        let mut value = base.clone();
        value["action"]["plan"]["tasks"][0]["labels"]["needs"] = bad;
        assert!(Envelope::parse(&value.to_string(), &expect(), Role::Lead).is_err());
    }
    let mut value = base;
    value["action"]["plan"]["tasks"][0]["kit"] = json!({"receipt":"a-1","digest":"0".repeat(64),"install":true});
    assert_eq!(Envelope::parse(&value.to_string(), &expect(), Role::Lead), Err(WireError::Malformed));
}
