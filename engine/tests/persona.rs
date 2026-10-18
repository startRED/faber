use faber_engine::persona::{PersonaInput, persona};
#[test]
fn persona_is_static_bounded_and_has_no_scar_rules() {
    for name in [None, Some("Erick"), Some("日本語")] {
        let p = PersonaInput { name };
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
    let text = persona(&PersonaInput { name: Some("Erick") });
    let rule = text.find("in the language of their latest message").unwrap();
    assert!(rule < text.find("You are Faber").unwrap());
    assert!(text.contains("even when the project, its instructions, files or tools use another"));
    assert!(text.contains("progress notes"));
}
