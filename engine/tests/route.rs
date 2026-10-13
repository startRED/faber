use faber_engine::model::Vendor;
use faber_engine::route::{INSTRUCTIONS, KINDS, LEVELS, Profile, asked, candidates, choose, question, read};
use faber_engine::router::Pick;

const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

fn all() -> Vec<Profile> {
    candidates(&BOTH, &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()])
}

fn picked(kind: &'static str, level: &'static str, confidence: f64, from: &[Profile]) -> (&'static str, &'static str) {
    let r = choose(kind, level, "glance", confidence, from).unwrap();
    (r.profile.model, r.profile.effort)
}

#[test]
fn the_router_never_sees_a_model_or_company_name() {
    let seen = format!("{INSTRUCTIONS}\n{}", question("Fix the failing test in my Rust project", 0)).to_lowercase();
    for name in ["claude", "anthropic", "opus", "sonnet", "haiku", "gpt", "openai", "codex", "chatgpt", "sol", "luna"] {
        assert!(!seen.split(|c: char| !c.is_alphanumeric()).any(|w| w == name), "the router saw {name:?}");
    }
}

#[test]
fn only_connected_vendors_and_models_the_account_lists_are_candidates() {
    let codex = candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]);
    assert_eq!(codex.iter().map(|p| p.model).collect::<Vec<_>>(), ["gpt-6-luna"]);
    assert!(candidates(&[Vendor::Claude], &[]).iter().all(|p| p.vendor == Vendor::Claude));
}

#[test]
fn each_task_goes_to_the_cheapest_model_that_clears_its_bar() {
    let all = all();
    assert_eq!(picked("chat", "easy", 0.9, &all), ("gpt-6-luna", "medium"));
    assert_eq!(picked("code", "easy", 0.9, &all), ("claude-haiku-5-5", "medium"));
    assert_eq!(picked("writing", "medium", 0.9, &all), ("gpt-6.1-sol", "low"));
    assert_eq!(picked("code", "medium", 0.9, &all), ("gpt-6.1-sol", "low"));
    assert_eq!(picked("math", "medium", 0.9, &all), ("gpt-6.1-sol", "medium"));
    assert_eq!(picked("research", "hard", 0.9, &all), ("gpt-6.1-sol", "medium"));
    assert_eq!(picked("code", "hard", 0.9, &all), ("gpt-6.1-sol", "medium"));
}

#[test]
fn an_unsure_verdict_gets_the_next_level() {
    assert_eq!(picked("chat", "easy", 0.3, &all()), ("gpt-6.1-sol", "low"));
    assert_eq!(picked("code", "medium", 0.3, &all()), ("gpt-6.1-sol", "medium"));
}

#[test]
fn with_one_vendor_the_table_stays_inside_it() {
    let claude = candidates(&[Vendor::Claude], &[]);
    assert_eq!(picked("code", "hard", 0.9, &claude), ("claude-sonnet-5-5", "high"));
    assert_eq!(picked("chat", "easy", 0.9, &claude), ("claude-haiku-5-5", "medium"));
    let luna_only = candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]);
    assert_eq!(picked("code", "hard", 0.9, &luna_only), ("gpt-6-luna", "medium"));
}

#[test]
fn the_answer_names_the_reason() {
    let routed = read("Sure.\n{\"kind\":\"writing\",\"difficulty\":\"easy\",\"confidence\":0.9}", &all()).unwrap();
    let pick = routed.pick();
    assert_eq!(
        (pick.vendor, pick.model.as_deref(), pick.reason.as_str()),
        (Vendor::Codex, Some("gpt-6-luna"), "reason.route.writing.easy")
    );
}

#[test]
fn an_answer_without_a_verdict_is_no_route() {
    assert_eq!(read("I think it is easy", &all()), None);
    assert_eq!(read(r#"{"kind":"chat","difficulty":"trivial","confidence":1}"#, &all()), None);
}

#[test]
fn a_conversation_under_way_never_moves_down() {
    let codex = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]);
    let current = |model: &str, effort: &str| Pick {
        vendor: Vendor::Codex,
        model: Some(model.into()),
        effort: effort.into(),
        reason: String::new(),
        pace: None,
    };
    let easy_chat = choose("chat", "easy", "glance", 0.9, &codex).unwrap();
    assert!(easy_chat.weaker_than(&current("gpt-6.1-sol", "low")));
    assert!(!easy_chat.weaker_than(&current("gpt-6-luna", "medium")));
    // A model with no profile is never treated as stronger.
    assert!(!easy_chat.weaker_than(&current("gpt-6.1-astra", "medium")));
}

fn checked(kind: &'static str, level: &'static str, check: &'static str, confidence: f64) -> Profile {
    choose(kind, level, check, confidence, &all()).unwrap().profile
}

#[test]
fn a_blind_medium_request_where_quality_matters_gets_the_hard_bar() {
    for kind in ["code", "math", "research"] {
        assert_eq!(checked(kind, "medium", "blind", 0.9), checked(kind, "hard", "glance", 0.9), "{kind}");
    }
    // The bump is real: blind medium code is not what glance medium code gets.
    assert_ne!(checked("code", "medium", "blind", 0.9), checked("code", "medium", "glance", 0.9));
}

#[test]
fn unsure_and_blind_together_are_one_step() {
    for kind in ["code", "math", "research"] {
        assert_eq!(checked(kind, "medium", "blind", 0.3), checked(kind, "medium", "glance", 0.3), "{kind}");
        assert_eq!(checked(kind, "hard", "blind", 0.3), checked(kind, "hard", "glance", 0.3), "{kind}");
    }
}

#[test]
fn a_blind_verdict_never_picks_a_cheaper_profile_and_a_machine_one_changes_nothing() {
    let all = all();
    for kind in KINDS {
        for level in LEVELS {
            for confidence in [0.3, 0.9] {
                let glance = choose(kind, level, "glance", confidence, &all).unwrap();
                let blind = choose(kind, level, "blind", confidence, &all).unwrap();
                assert!(blind.profile.cost >= glance.profile.cost, "{kind} {level} {confidence}");
                assert!((blind.profile.iq, blind.profile.agent) >= (glance.profile.iq, glance.profile.agent), "{kind} {level}");
                assert_eq!(choose(kind, level, "machine", confidence, &all).unwrap(), glance, "{kind} {level} {confidence}");
                // The reason key keeps the router's own word.
                assert_eq!(blind.pick().reason, glance.pick().reason);
            }
        }
    }
}

#[test]
fn chat_and_writing_ignore_the_check() {
    for kind in ["chat", "writing", "image", "other"] {
        for level in LEVELS {
            assert_eq!(checked(kind, level, "blind", 0.9), checked(kind, level, "glance", 0.9), "{kind} {level}");
        }
    }
}

#[test]
fn a_line_without_a_check_reads_as_glance() {
    let line = |extra: &str| format!("{{\"kind\":\"research\",\"difficulty\":\"medium\"{extra},\"confidence\":0.9}}");
    assert_eq!(asked(&line("")).unwrap().check, "glance");
    assert_eq!(asked(&line(",\"check\":\"nonsense\"")).unwrap().check, "glance");
    assert_eq!(asked(&line(",\"check\":\"blind\"")).unwrap().check, "blind");
    assert_eq!(read(&line(""), &all()).unwrap().profile, checked("research", "medium", "glance", 0.9));
}
