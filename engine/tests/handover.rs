use faber_engine::handover::{self, Partial, Plan};
use faber_engine::model::{Message, Role, ToolUse};

fn message(role: Role, text: &str) -> Message {
    Message::new("m".into(), role, text.into(), 0)
}

#[test]
fn short_history_goes_whole_and_long_history_keeps_the_first_and_last_turns() {
    let short = vec![message(Role::User, &"a".repeat(10_000))];
    let Plan::Full(all) = handover::plan(&short) else { panic!("short history was summarised") };
    assert!(all.starts_with("User: ") && all.ends_with(&"a".repeat(10_000)));
    let long =
        vec![message(Role::User, "First requirement"), message(Role::Assistant, &"b".repeat(30_000)), message(Role::User, "Last turn")];
    let Plan::Summarise(input) = handover::plan(&long) else { panic!("long history was not summarised") };
    assert!(input.starts_with("User: First requirement"));
    assert!(input.ends_with("User: Last turn"));
}

#[test]
fn a_partial_answer_keeps_tool_effects_and_explicitly_continues_instead_of_repeating_them() {
    let mut reply = message(Role::Assistant, "Half the answer");
    reply.tools = Some(vec![ToolUse {
        id: "t".into(),
        name: "Write".into(),
        input: serde_json::json!({"file_path":"done.txt"}),
        ok: Some(true),
        output: Some("x".repeat(1000)),
        declined: false,
        parent: None,
        sign: None,
        facts: None,
        check: false,
    }]);
    let partial = Partial::of(&reply);
    assert!(!partial.is_empty());
    assert!(partial.tools[0].1.chars().count() <= 200);
    let handed = handover::with_full("earlier", "Do the task", &partial);
    assert!(handed.contains("Half the answer") && handed.contains("Write"));
    assert!(handed.contains("verify effects before repeating") && handed.ends_with("Continue from there."));
}
