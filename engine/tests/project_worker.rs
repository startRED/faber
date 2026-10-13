//! Fixture-only worker contracts and reflexes; no process or model call.
use faber_engine::model::Vendor;
use faber_engine::project::worker::{ModelPin, Reply, Worker, render};
use faber_engine::project::{Brief, ContractPin, Role};
use serde_json::json;

fn brief() -> Brief {
    let mut b = Brief::new("w1", 1, Role::Writer, "Fix it", &"1".repeat(40), ContractPin { id: "c1".into(), digest: "a".repeat(64) });
    b.scope = vec!["engine/src/a.rs".into()];
    b.writes = b.scope.clone();
    b.lease = Some(1);
    b.grant_micros = Some(1000);
    b.stop = "Pass C1 or block".into();
    b.proofs = vec!["rust".into()];
    let stored: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/projects/worker-verbatim.json")).unwrap();
    b.requires = serde_json::from_value(stored["requires"].clone()).unwrap();
    b.checks = serde_json::from_value(stored["checks"].clone()).unwrap();
    b
}
fn pin() -> ModelPin {
    ModelPin { vendor: Vendor::Codex, model: "gpt-6.1-sol".into(), effort: "medium".into(), cli_version: "0.162.1".into() }
}
fn envelope() -> serde_json::Value {
    serde_json::from_str(include_str!("../../tests/fixtures/projects/worker-read.json")).unwrap()
}
fn worker() -> Worker {
    Worker::new(brief(), pin(), 12).unwrap()
}
#[test]
fn renderer_keeps_exact_utf8_and_separates_requirements_from_dependencies() {
    let b = brief();
    let text = render(&b).unwrap();
    for v in b.requires.iter().chain(&b.checks) {
        assert!(text.contains(&v.text), "verbatim bytes missing");
    }
    assert!(text.contains("R1"));
    assert!(text.contains("C1"));
    assert!(text.contains("stop"));
}
#[test]
fn missing_alias_model_effort_or_version_cannot_spawn() {
    for (model, effort, version) in [
        ("", "medium", "1"),
        ("default", "medium", "1"),
        ("gpt-latest", "medium", "1"),
        ("gpt-6.1-sol", "", "1"),
        ("gpt-6.1-sol", "medium", ""),
    ] {
        let mut p = pin();
        p.model = model.into();
        p.effort = effort.into();
        p.cli_version = version.into();
        assert!(Worker::new(brief(), p, 12).is_err());
    }
    let mut b = brief();
    b.grant_micros = None;
    assert!(Worker::new(b, pin(), 12).is_err());
    let mut b = brief();
    b.lease = None;
    assert!(Worker::new(b, pin(), 12).is_err());
}
#[test]
fn reported_identity_is_required_for_every_answered_turn() {
    let mut w = worker();
    let mut p = pin();
    p.cli_version.clear();
    assert!(w.begin_turn(&p).is_err());
    p = pin();
    p.model = "gpt-6-sol".into();
    assert!(w.begin_turn(&p).is_err());
    w.begin_turn(&pin()).unwrap();
    assert_eq!(w.answered.len(), 1);
    assert_eq!(w.answered[0], pin());
}
#[test]
fn two_malformed_bounces_block_without_a_model_call() {
    let mut w = worker();
    assert!(matches!(w.reply("not json"), Reply::Bounce(_)));
    assert!(matches!(w.reply("not json"), Reply::Blocked(_)));
    assert!(w.answered.is_empty());
}
#[test]
fn stale_packets_role_escapes_unknown_ids_and_oversized_notes_fail_closed() {
    for (key, value) in [
        ("attempt", json!(2)),
        ("lease", json!(2)),
        ("requires", json!(["not-R1"])),
        ("note", json!((0..61).map(|_| "x").collect::<Vec<_>>().join(" "))),
    ] {
        let mut e = envelope();
        e[key] = value;
        assert!(matches!(worker().reply(&e.to_string()), Reply::Bounce(_)));
    }
    for action in [
        json!({"spawn":{"peer":"w2"}}),
        json!({"grant":{"micros":99999}}),
        json!({"patch":{"ops":[{"write":{"path":"../outside","expect":null,"content":"x"}}]}}),
    ] {
        let mut e = envelope();
        e["action"] = action;
        assert!(matches!(worker().reply(&e.to_string()), Reply::Bounce(_)));
    }
    let mut b = brief();
    b.role = Role::TestProposal;
    b.writes.clear();
    b.lease = None;
    let mut w = Worker::new(b, pin(), 12).unwrap();
    let mut e = envelope();
    e["lease"] = json!(0);
    e["action"] = json!({"patch":{"ops":[{"write":{"path":"engine/src/a.rs","expect":null,"content":"x"}}]}});
    assert!(matches!(w.reply(&e.to_string()), Reply::Proposal(_)));
}
#[test]
fn three_equal_failed_proofs_or_same_hunk_patches_block_until_real_progress() {
    let mut w = worker();
    let turns: Vec<serde_json::Value> =
        include_str!("../../tests/fixtures/projects/loop-proof.jsonl").lines().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(turns.len(), 3);
    for (index, turn) in turns.iter().enumerate() {
        let result = w.failed_proof(turn["id"].as_str().unwrap(), turn["fingerprint"].as_str().unwrap());
        assert_eq!(result, if index == 2 { Err("reason.loop") } else { Ok(()) });
    }
    let mut w = worker();
    for _ in 0..2 {
        w.patched("engine/src/a.rs#1").unwrap();
    }
    assert_eq!(w.patched("engine/src/a.rs#1"), Err("reason.loop"));
    let mut w = worker();
    w.patched("engine/src/a.rs#1").unwrap();
    w.patched("engine/src/a.rs#1").unwrap();
    w.proof_progress();
    assert!(w.patched("engine/src/a.rs#1").is_ok());
}
#[test]
fn one_continue_then_stall_and_restart_never_refill_the_lifetime_grant() {
    let mut e = envelope();
    e["action"] = json!({"complete":{"summary":"I can continue","next":"continue"}});
    e["ping"] = json!("Found");
    let mut w = worker();
    assert_eq!(w.reply(&e.to_string()), Reply::Continue);
    let mut restored: Worker = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
    assert_eq!(restored.reply(&e.to_string()), Reply::Blocked("reason.stall"));
    let mut w = Worker::new(brief(), pin(), 1).unwrap();
    w.begin_turn(&pin()).unwrap();
    assert_eq!(w.begin_turn(&pin()), Err("reason.turnLimit"));
}
#[test]
fn failed_proof_raises_effort_then_model_in_fresh_bounded_attempts() {
    let mut w = worker();
    w.begin_turn(&pin()).unwrap();
    w.failed_proof("rust", "bad").unwrap();
    let mut higher = pin();
    higher.effort = "high".into();
    w.escalate(higher.clone()).unwrap();
    assert_eq!(w.turn_limit, 6);
    assert_eq!(w.spent_turns, 1);
    assert_eq!(w.brief.attempt, 2);
    assert!(w.fresh);
    w.failed_proof("rust", "bad").unwrap();
    let mut model = higher.clone();
    model.model = "gpt-6-astra".into();
    w.escalate(model).unwrap();
    assert_eq!(w.turn_limit, 3);
    assert_eq!(w.spent_turns, 1);
    assert!(w.escalate(higher).is_err());
}

#[test]
fn worker_uses_only_its_frozen_kit_receipt_and_keeps_role_controls() {
    use faber_engine::project::kit::{Kit, KitReceipt};
    let kit:Kit=serde_json::from_value(json!({"labels":{"verb":"build","needs":[],"risk":"low","fresh":"stable"},"needs":[],"items":[],"proofs":["rust"],"reviewLenses":[],"gaps":[],"dropped":[],"role":"writer","vendor":"codex","model":"gpt-6.1-sol","effort":"medium","inventoryHash":"a".repeat(64),"residentTokens":0,"tokenCap":4000,"overBudgetPin":false,"assembleMicros":0})).unwrap();
    let receipt = KitReceipt::new("m1", "w1", 1, kit, 1).unwrap();
    let mut b = brief();
    b.kit = Some(receipt.pin());
    let w = Worker::new(b, pin(), 12).unwrap();
    let prompt = w.kit_prompt(&receipt, &[]).unwrap();
    assert!(prompt.contains("Never execute tools"));
    assert!(prompt.contains("R1"));
    let mut stale = receipt;
    stale.kit.effort = "high".into();
    assert!(w.kit_prompt(&stale, &[]).is_err());
}
