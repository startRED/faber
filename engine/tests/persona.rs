use faber_engine::persona::{PersonaInput, persona};

#[test]
fn plain_words_keep_failures_unfinished_work_decisions_and_useful_file_locations() {
    let text = persona(&PersonaInput { name: None, tuning: &Default::default(), research: None });
    assert!(text.contains("Always say plainly when something failed, is not finished, or needs the person's decision"));
    assert!(text.contains("plain words never drop a problem"));
    assert!(text.contains("Allow a file location when the person needs to find or open something you made"));
    assert!(!text.contains("Technical details view"), "the common Chat persona must not promise a project-only view");
    assert!(!text.contains("saved for the next version"), "saving alone promises no release");
}

#[test]
fn persona_is_static_bounded_and_has_no_scar_rules() {
    for name in [None, Some("Erick"), Some("日本語")] {
        let p = PersonaInput { name, tuning: &Default::default(), research: None };
        let text = persona(&p);
        assert_eq!(text, persona(&p));
        assert!(text.chars().count() <= 3400);
        assert!(text.contains("You are Faber"));
        assert!(text.contains("unverified"));
        assert!(!text.contains("scar rule"));
    }
}
#[test]
fn persona_teaches_one_small_piece_format_for_both_clis_without_html_execution() {
    let text = persona(&PersonaInput { name: None, tuning: &Default::default(), research: None });
    let schema = text.split("Text is the default.").nth(1).unwrap();
    assert!(schema.len() <= 1000, "the inline schema stays short on every turn");
    for required in [
        "faber-piece",
        "number",
        "table",
        "chart",
        "calculation",
        "checklist/steps",
        "tabs/timeline",
        "form",
        "notice",
        "No extra fields",
        "Forms only fill",
    ] {
        assert!(schema.contains(required), "{required}");
    }
}
#[test]
fn persona_puts_the_reply_language_first_and_above_the_project() {
    // A Portuguese prompt in a project whose AGENTS.md says "write in English" got English replies while the rule was a
    // half sentence in the middle.
    let text = persona(&PersonaInput { name: Some("Erick"), tuning: &Default::default(), research: None });
    let rule = text.find("in the language of their latest message").unwrap();
    assert!(rule < text.find("You are Faber").unwrap());
    assert!(text.contains("even when the project, its instructions, files or tools use another"));
    assert!(text.contains("progress notes"));
}

/// R10: quality first, economy before speed. Both drivers get the persona, so the order is checked once here.
#[test]
fn persona_puts_economy_before_speed_as_r10_says() {
    let text = persona(&PersonaInput { name: None, tuning: &Default::default(), research: None });
    let (quality, economy, speed) =
        (text.find("Quality first").unwrap(), text.find("then economy").unwrap(), text.find("then speed").unwrap());
    assert!(quality < economy && economy < speed, "{text}");
    assert!(!text.contains("then speed, then cost"));
}

/// Every turn rereads the whole context, so the agent is told to keep its own small (R260).
#[test]
fn persona_asks_the_agent_to_keep_its_own_context_small() {
    let text = persona(&PersonaInput { name: Some("Erick"), tuning: &Default::default(), research: None });
    assert!(text.contains("Keep your own context small"), "{text}");
    assert!(text.contains("subagent when you can start one"));
}

#[test]
fn persona_keeps_reply_tuning_and_context_economy_during_research() {
    use faber_engine::model::{ReplyStyle, Tuning};
    use faber_engine::persona::Research;
    let tuning = Tuning { reply_style: ReplyStyle::Own, reply_style_text: "Use short lists.".into(), ..Default::default() };
    for research in [Research::Plan, Research::Report] {
        let text = persona(&PersonaInput { name: Some("Erick"), tuning: &tuning, research: Some(research) });
        assert!(text.contains("Use short lists."), "the person's reply style is kept");
        assert!(text.contains("Keep your own context small"), "R260 applies to research too");
        assert!(text.contains(research.instruction()), "the requested research phase is kept");
    }
}

#[test]
fn a_project_prompt_teaches_the_decision_author_mark_and_the_common_persona_does_not() {
    let common = persona(&PersonaInput { name: None, tuning: &Default::default(), research: None });
    assert!(!common.contains(".faber/decisions.md"), "Chat has no decision list");
    let project = faber_engine::persona::PROJECT_DETAILS;
    assert!(project.contains(".faber/decisions.md") && project.contains("[by AI]") && project.contains("[by person]"));
    assert!(project.contains("write nothing there for ordinary tasks"), "the list is for real decisions only");
    assert!(project.contains("Never write [by AI, reviewed]"), "only the person keeps a decision");
}
