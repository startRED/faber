use faber_engine::handover::{self, Partial, Plan};
use faber_engine::model::{Message, Role, ToolUse};

fn message(role: Role, text: &str) -> Message {
    Message::new("m".into(), role, text.into(), 0)
}

#[test]
fn an_interrupted_helpers_brief_reaches_both_history_and_partial_handover() {
    let brief = format!("Check the interrupted work. {} Preserve the final requirement.", "Read the original request. ".repeat(12));
    for (name, field) in [("Helper", "brief"), ("Agent", "prompt"), ("Task", "prompt")] {
        let mut reply = message(Role::Assistant, "Work started");
        reply.tools = Some(vec![
            serde_json::from_value(serde_json::json!({
                "id": "worker", "name": name, "input": {field: brief, "unrelated": "private-input-must-stay-out"}
            }))
            .unwrap(),
        ]);
        let history = handover::transcript(std::slice::from_ref(&reply));
        let partial = Partial::of(&reply);
        let continued = handover::with_summary("Earlier work", "Continue", &partial);
        for carried in [history, continued] {
            assert!(carried.contains(&brief), "{name}: the submitted brief must survive, including its final requirement");
            assert!(carried.contains("unfinished"), "{name}: the receiving AI must not mistake a brief for completed work");
            assert!(!carried.contains("private-input-must-stay-out"), "only the brief is carried");
        }
    }
}

#[test]
fn completed_helpers_and_other_tool_inputs_do_not_expand_the_handover() {
    for (name, ok) in [("Helper", Some(true)), ("Agent", Some(false)), ("Write", None)] {
        let mut reply = message(Role::Assistant, "Previous work");
        reply.tools = Some(vec![
            serde_json::from_value(serde_json::json!({
                "id": "tool", "name": name, "ok": ok, "output": "recorded result",
                "input": {"brief": "input-must-stay-out", "prompt": "input-must-stay-out"}
            }))
            .unwrap(),
        ]);
        let history = handover::transcript(std::slice::from_ref(&reply));
        let partial = Partial::of(&reply);
        for carried in [history, handover::with_full("Earlier work", "Continue", &partial)] {
            assert!(carried.contains("recorded result"));
            assert!(!carried.contains("input-must-stay-out"), "{name}: no completed brief or arbitrary tool input is needed");
        }
    }
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
        interrupted: false,
        parent: None,
        sign: None,
        facts: None,
        told: vec![],
        words: vec![],
        words_cut: false,
        words_chars: None,
        transcript: None,
        check: false,
        started: None,
        ended: None,
    }]);
    let partial = Partial::of(&reply);
    assert!(!partial.is_empty());
    assert!(partial.tools[0].1.chars().count() <= 200);
    let handed = handover::with_full("earlier", "Do the task", &partial);
    assert!(handed.contains("Half the answer") && handed.contains("Write"));
    assert!(handed.contains("verify effects before repeating") && handed.ends_with("Continue from there."));
}
