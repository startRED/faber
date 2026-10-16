use faber_engine::persona::{PersonaInput, persona, worker_contract};
use faber_engine::project::Role;
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
fn worker_contract_keeps_role_restrictions_and_receipts_without_the_chat_persona() {
    for role in [Role::Lead, Role::Research, Role::Writer, Role::TestProposal, Role::Reviewer] {
        let text = worker_contract(role);
        assert!(text.contains("envelope"));
        assert!(text.contains("receipt"));
        assert!(text.contains("Never"));
        for ping in ["Order", "Found", "Blocked", "NeedEyes", "DoneReady", "Disagree"] {
            // A model that is not told the values guesses one (an integer was seen from a real worker turn).
            assert!(text.contains(ping));
            serde_json::from_str::<faber_engine::project::wire::Ping>(&format!("\"{ping}\"")).unwrap();
        }
        assert!(!text.contains("You are Faber"));
        assert!(text.chars().count() <= 1800);
    }
}
