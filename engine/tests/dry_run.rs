use faber_engine::model::Conversation;
use faber_engine::results::Stuck;
use serde_json::json;

#[test]
fn dry_run_marker_survives_serialization() {
    let mut c: Conversation =
        serde_json::from_value(json!({"id":"a", "title":"a", "area":"chat", "createdAt":0, "updatedAt":0, "messages":[]})).unwrap();
    c.stuck = Some(Stuck { noticed_at: 2, since: 1, turns: 12, wall_ms: 1, usd: 1.0, asked_at: Some(2), ask_turns: 12 });
    let loaded: Conversation = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
    assert_eq!(loaded.stuck, c.stuck);
}
