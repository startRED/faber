use faber_engine::persona::{PersonaInput, persona};
#[test]
fn persona_is_static_bounded_and_has_no_scar_rules() {
    for name in [None, Some("Erick"), Some("日本語")] {
        let p = PersonaInput { name, tuning: &Default::default() };
        let text = persona(&p);
        assert_eq!(text, persona(&p));
        assert!(text.chars().count() <= 1800);
        assert!(text.contains("You are Faber"));
        assert!(text.contains("unverified"));
        assert!(!text.contains("scar rule"));
    }
}
#[test]
fn persona_puts_the_reply_language_first_and_above_the_project() {
    // A Portuguese prompt in a project whose AGENTS.md says "write in English" got English replies while the rule was a
    // half sentence in the middle.
    let text = persona(&PersonaInput { name: Some("Erick"), tuning: &Default::default() });
    let rule = text.find("in the language of their latest message").unwrap();
    assert!(rule < text.find("You are Faber").unwrap());
    assert!(text.contains("even when the project, its instructions, files or tools use another"));
    assert!(text.contains("progress notes"));
}

/// R10: quality first, economy before speed. Both drivers get the persona, so the order is checked once here.
#[test]
fn persona_puts_economy_before_speed_as_r10_says() {
    let text = persona(&PersonaInput { name: None, tuning: &Default::default() });
    let (quality, economy, speed) =
        (text.find("Quality first").unwrap(), text.find("then economy").unwrap(), text.find("then speed").unwrap());
    assert!(quality < economy && economy < speed, "{text}");
    assert!(!text.contains("then speed, then cost"));
}

/// Every turn rereads the whole context, so the agent is told to keep its own small (R260).
#[test]
fn persona_asks_the_agent_to_keep_its_own_context_small() {
    let text = persona(&PersonaInput { name: Some("Erick"), tuning: &Default::default() });
    assert!(text.contains("Keep your own context small"), "{text}");
    assert!(text.contains("subagent when you can start one"));
}
