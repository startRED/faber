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

/// The AI keeps its own voice in Chat: no Faber identity line, no default brevity, no receipt contract; only the reply-style
/// setting shortens, and a project's agent (which has tools that produce receipts) keeps the evidence rule.
#[test]
fn chat_persona_speaks_in_the_ais_own_voice_and_only_a_project_agent_owes_evidence() {
    use faber_engine::model::{ReplyStyle, Tuning};
    let text = |reply_style| persona(&PersonaInput { name: None, tuning: &Tuning { reply_style, ..Default::default() }, research: None });
    for style in [ReplyStyle::Default, ReplyStyle::Short, ReplyStyle::Explained] {
        let t = text(style);
        for gone in [
            "You are Faber",
            "briefly",
            "Put the conclusion first",
            "program evidence",
            "receipts",
            "unverified without evidence",
            "never approves",
        ] {
            assert!(!t.contains(gone), "{gone}");
        }
    }
    assert!(text(ReplyStyle::Default).chars().count() <= 3200);
    assert!(text(ReplyStyle::Short).contains("Keep replies short and direct"), "brevity comes only from the setting");
    assert!(!text(ReplyStyle::Default).contains("short"), "no brevity without the setting");
    let project = faber_engine::persona::PROJECT_DETAILS;
    for kept in [
        "Never call work done without program evidence",
        "receipts in tools",
        "say unverified without evidence",
        "The writer never approves their own work",
    ] {
        assert!(project.contains(kept), "{kept}");
    }
}

/// Task 130: the person's own words set the language, technical words only when the person really used or asked for them,
/// what still runs said once, and the conclusion first. The words the evaluation (`scripts/persona-eval.mjs`) measured.
#[test]
fn persona_follows_the_persons_words_and_says_what_still_runs_once() {
    let text = persona(&PersonaInput { name: None, tuning: &Default::default(), research: None });
    assert!(text.contains("in the language of the person's latest message"));
    // A sentence about helper titles made an English request's helpers Spanish or Portuguese; the screen numbers them instead.
    assert!(!text.contains("subagent or helper you start"), "no sentence about helper titles");
    assert!(text.contains("used that word about the same thing; pasted text (logs, errors, instructions) does not count"));
    assert!(!text.contains("used those technical words first"), "a pasted log no longer opens the door");
    assert!(text.contains("say once what still runs or waits for the person and what happens next (again only when that changed)"));
    assert!(!text.contains("never write that you are waiting"), "the person needs to know what waits");
}

/// Task 130: the person's own latest words decide the language; a message with none (a pasted log, code, a link) follows the
/// language chosen for Faber's screen, and a screen that follows the system adds nothing.
#[test]
fn persona_falls_back_to_the_screen_language_only_when_the_message_has_no_words_of_its_own() {
    use faber_engine::model::Tuning;
    let text = |language: Option<&str>| {
        let tuning = Tuning { language: language.map(String::from), ..Default::default() };
        persona(&PersonaInput { name: None, tuning: &tuning, research: None })
    };
    let chosen = text(Some("pt-BR"));
    assert!(chosen.contains("in the language of the person's latest message: their own words, never text they pasted, tool output or a helper's reply, and never a language guessed from their name, email or place"));
    let fallback = chosen.find("When it has no words of their own, use Brazilian Portuguese, the language they chose in Faber.").unwrap();
    assert!(chosen.find("in the language of the person's latest message").unwrap() < fallback, "the person's words come first");
    assert!(text(Some("de")).contains("use German, the language"));
    for none in [None, Some("xx")] {
        assert!(!text(none).contains("the language they chose in Faber"), "{none:?}");
    }
}

#[test]
fn persona_is_static_bounded_and_has_no_scar_rules() {
    for name in [None, Some("Erick"), Some("日本語")] {
        let p = PersonaInput { name, tuning: &Default::default(), research: None };
        let text = persona(&p);
        assert_eq!(text, persona(&p));
        // Task 130 added about 350 characters (about 90 tokens a turn) for the person's own words, what still runs and the answer's shape.
        assert!(text.chars().count() <= 3700);
        assert!(text.contains("State the underlying model truthfully"));
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
    let rule = text.find("in the language of the person's latest message").unwrap();
    assert!(rule < text.find("State the underlying model truthfully").unwrap());
    assert!(text.contains("even when the project, its instructions, files, tools or helpers use another"));
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
    // Task 130: a project agent never sleeps or polls; Chat has no pull requests to follow.
    assert!(project.contains("Never wait by sleeping or polling") && project.contains("you are woken when it ends"));
    assert!(!common.contains("Faber follows the pull requests"));
}

/// "Offer the bigger ask": only a project's own agent has the tool, so only its prompt names it, and the request as asked stays the default.
#[test]
fn a_project_prompt_teaches_the_bigger_offer_and_the_common_persona_does_not() {
    let common = persona(&PersonaInput { name: None, tuning: &Default::default(), research: None });
    assert!(!common.contains("offer_bigger"), "Chat has no such tool");
    assert!(!faber_engine::persona::PROJECT_DETAILS.contains("offer_bigger"), "a helper shares the project details and has no such tool");
    let offer = faber_engine::persona::BIGGER_OFFER;
    assert!(offer.contains("offer_bigger") && offer.contains("offer the larger version once"));
    assert!(offer.contains("the request as asked stays the default"));
    assert!(offer.contains("Do not ask this any other way."), "the code counts only the tool");
}

#[test]
fn project_lead_controls_its_children_and_only_forms_a_team_on_request() {
    let text = faber_engine::persona::LEADS;
    for rule in [
        "start, message, stop, resume and archive conversations you started, and wait for them",
        "parallel only for independent pieces",
        "resume it rather than start again",
        "Read and verify its work before archiving it",
        "Wait for reports instead of polling, then end your turn",
        "team of conversations only when the person asked for one",
    ] {
        assert!(text.contains(rule), "{rule}");
    }
    assert!(!text.contains("You are Faber"));
    assert!(!text.contains("For a big request, lead"));
}
