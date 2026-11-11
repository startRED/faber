use faber_engine::model::Vendor;
use faber_engine::route::{INSTRUCTIONS, KINDS, LEVELS, Profile, asked, candidates, choose, question, read};
use faber_engine::router::Pick;

const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

#[test]
fn measured_picks_for_every_kind_level_and_connected_vendor() {
    let models = ["gpt-6.1-sol", "gpt-6.1-terra", "gpt-6-luna"].map(String::from);
    let sets = [candidates(&[Vendor::Claude], &models), candidates(&[Vendor::Codex], &models), candidates(&BOTH, &models)];
    let levels = ["easy", "medium", "hard", "expert", "frontier"];
    let claude = [
        ("claude-haiku-5-5", "medium"),
        ("claude-sonnet-5-5", "high"),
        ("claude-opus-5-5", "medium"),
        ("claude-fable-5-2", "low"),
        ("claude-fable-5-2", "medium"),
    ];
    let codex = [
        ("gpt-6.1-terra", "low"),
        ("gpt-6.1-terra", "medium"),
        ("gpt-6.1-sol", "medium"),
        ("gpt-6.1-sol", "high"),
        ("gpt-6.1-sol", "xhigh"),
    ];
    for kind in KINDS {
        for (i, level) in levels.into_iter().enumerate() {
            let at = if kind == "math" && level == "medium" { 2 } else { i };
            let both = if at == 4 { claude[at] } else { codex[at] };
            for project in [false, true] {
                for (set, expected) in sets.iter().zip([claude[at], codex[at], both]) {
                    let row = faber_engine::route::choose_scoped(kind, level, "glance", "tight", 0.9, project, set).unwrap();
                    assert_eq!((row.profile.model, row.profile.effort), expected, "{kind} {level} project={project} vendors={set:?}");
                }
            }
        }
    }
}

#[test]
fn measured_effort_ladder_inside_each_conversations_model() {
    use faber_engine::route::{Asked, PROFILES, choose_effort};
    let cases = [
        ("claude-fable-5-2", ["low", "low", "low", "low", "medium"]),
        ("claude-opus-5-5", ["low", "medium", "medium", "high", "xhigh"]),
        ("claude-sonnet-5-5", ["low", "high", "xhigh", "xhigh", "xhigh"]),
        ("gpt-6.1-sol", ["low", "medium", "medium", "high", "xhigh"]),
        ("gpt-6.1-terra", ["low", "medium", "xhigh", "xhigh", "xhigh"]),
    ];
    let tasks = [
        ("chat", "easy", "tight", "glance"),
        ("research", "medium", "tight", "glance"),
        ("code", "hard", "tight", "machine"),
        ("code", "hard", "open", "machine"),
        ("code", "hard", "open", "blind"),
    ];
    for (model, expected) in cases {
        let current = PROFILES.iter().find(|p| p.model == model && p.effort == "low").unwrap();
        for ((kind, difficulty, scope, check), effort) in tasks.into_iter().zip(expected) {
            let verdict = Asked { kind, difficulty, scope, check, confidence: 0.9 };
            let row = choose_effort(&verdict, false, current).unwrap().profile;
            assert_eq!((row.model, row.effort), (model, effort), "{model}: {verdict:?}");
        }
    }
}

#[test]
fn measured_profiles_have_no_max_duplicate_or_dominated_effort() {
    use faber_engine::route::PROFILES;
    for (i, row) in PROFILES.iter().enumerate() {
        assert_ne!(row.effort, "max");
        for (j, other) in PROFILES.iter().enumerate().filter(|(j, p)| *j != i && p.model == row.model) {
            assert_ne!(row.effort, other.effort, "duplicate rows {i} and {j}");
            let dominates = other.iq >= row.iq && other.agent >= row.agent && other.secs <= row.secs && other.cost <= row.cost;
            assert!(!dominates, "{other:?} dominates {row:?}");
        }
    }
}

#[test]
fn effort_stays_in_the_model_and_only_confident_visible_work_can_lower_it() {
    use faber_engine::route::{Asked, PROFILES, choose_effort};
    let current = PROFILES.iter().find(|p| p.model == "gpt-6.1-sol" && p.effort == "high").unwrap();
    let easy = Asked { kind: "chat", difficulty: "easy", check: "glance", scope: "tight", confidence: 0.9 };
    let selected = choose_effort(&easy, false, current).unwrap().profile;
    assert_eq!((selected.vendor, selected.model, selected.effort), (current.vendor, current.model, "low"));
    for verdict in [Asked { confidence: 0.69, ..easy }, Asked { check: "blind", ..easy }] {
        assert_eq!(choose_effort(&verdict, false, current).unwrap().profile, *current);
    }
    assert_eq!(choose_effort(&Asked { confidence: 0.7, ..easy }, false, current).unwrap().profile, selected);
    let low = &selected;
    let hard = Asked { kind: "code", difficulty: "hard", scope: "open", ..easy };
    assert_eq!(choose_effort(&hard, true, low).unwrap().profile.effort, "high");
    let frontier = Asked { check: "blind", ..hard };
    assert_eq!(choose_effort(&frontier, true, low).unwrap().profile.effort, "xhigh");
}

#[test]
fn ten_mixed_effort_choices_keep_the_vendor_and_model() {
    use faber_engine::route::{Asked, PROFILES, choose_effort};
    let mut current = *PROFILES.iter().find(|p| p.model == "gpt-6.1-sol" && p.effort == "medium").unwrap();
    let original = (current.vendor, current.model);
    for i in 0..10 {
        let verdict = if i % 2 == 0 {
            Asked { kind: "code", difficulty: "hard", check: "machine", scope: "open", confidence: 0.9 }
        } else {
            Asked { kind: "chat", difficulty: "easy", check: "glance", scope: "tight", confidence: 0.9 }
        };
        current = choose_effort(&verdict, true, &current).unwrap().profile;
        assert_eq!((current.vendor, current.model), original);
    }
}

#[test]
fn effort_uses_the_strongest_row_of_the_same_model_when_no_row_meets_the_bar() {
    use faber_engine::route::{Asked, PROFILES, choose_effort};
    let current = PROFILES.iter().find(|p| p.model == "gpt-6.1-terra" && p.effort == "low").unwrap();
    let verdict = Asked { kind: "code", difficulty: "hard", check: "blind", scope: "open", confidence: 0.9 };
    let selected = choose_effort(&verdict, true, current).unwrap().profile;
    assert_eq!((selected.vendor, selected.model, selected.effort), (current.vendor, current.model, "xhigh"));
}

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
    for name in ["claude", "anthropic", "fable", "opus", "sonnet", "haiku", "gpt", "openai", "codex", "chatgpt", "sol", "luna"] {
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
    assert_eq!(picked("chat", "easy", 0.9, &all), ("claude-haiku-5-5", "medium"));
    assert_eq!(picked("code", "easy", 0.9, &all), ("claude-haiku-5-5", "medium"));
    assert_eq!(picked("writing", "medium", 0.9, &all), ("gpt-6.1-sol", "medium"));
    assert_eq!(picked("code", "medium", 0.9, &all), ("gpt-6.1-sol", "medium"));
    assert_eq!(picked("math", "medium", 0.9, &all), ("gpt-6.1-sol", "medium"));
    assert_eq!(picked("research", "hard", 0.9, &all), ("gpt-6.1-sol", "medium"));
    assert_eq!(picked("code", "hard", 0.9, &all), ("gpt-6.1-sol", "medium"));
}

#[test]
fn an_account_that_lists_terra_sends_it_the_medium_work_sol_had() {
    let models = ["gpt-6.1-sol", "gpt-6.1-terra", "gpt-6-luna"].map(String::from);
    let all = candidates(&BOTH, &models);
    assert_eq!(picked("writing", "medium", 0.9, &all), ("gpt-6.1-terra", "medium"));
    assert_eq!(picked("code", "medium", 0.9, &all), ("gpt-6.1-terra", "medium"));
    assert_eq!(picked("code", "hard", 0.9, &all), ("gpt-6.1-sol", "medium"));
    let codex = candidates(&[Vendor::Codex], &models);
    assert_eq!(picked("code", "easy", 0.9, &codex), ("gpt-6.1-terra", "low"));
}

#[test]
fn an_unsure_verdict_gets_the_next_level() {
    assert_eq!(picked("chat", "easy", 0.3, &all()), ("gpt-6.1-sol", "medium"));
    assert_eq!(picked("code", "medium", 0.3, &all()), ("gpt-6.1-sol", "medium"));
}

#[test]
fn with_one_vendor_the_table_stays_inside_it() {
    let claude = candidates(&[Vendor::Claude], &[]);
    assert_eq!(picked("code", "hard", 0.9, &claude), ("claude-opus-5-5", "medium"));
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
        (Vendor::Claude, Some("claude-haiku-5-5"), "reason.route.writing.easy")
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
    assert!(easy_chat.weaker_than(&current("gpt-6.1-sol", "high")));
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
    // Terra makes the distinction visible: Sol medium already clears both bars without Terra.
    let models = ["gpt-6.1-sol", "gpt-6.1-terra"].map(String::from);
    let all = candidates(&BOTH, &models);
    assert_ne!(
        choose("code", "medium", "blind", 0.9, &all).unwrap().profile,
        choose("code", "medium", "glance", 0.9, &all).unwrap().profile
    );
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
                assert_eq!(
                    choose(kind, level, "machine", confidence, &all).unwrap().profile,
                    glance.profile,
                    "{kind} {level} {confidence}"
                );
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

#[test]
fn handoff_keeps_the_exhausted_model_and_loads_older_messages() {
    use faber_engine::model::{Message, Role};
    let mut message = Message::new("reply".into(), Role::Assistant, "Answer".into(), 1);
    message.vendor = Some(Vendor::Codex);
    message.model = Some("gpt-6.1-sol".into());
    message.moved_from = Some(Vendor::Claude);
    message.moved_from_model = Some("claude-sonnet-4-6".into());
    let mut saved = serde_json::to_value(&message).unwrap();
    assert_eq!(saved["movedFromModel"], "claude-sonnet-4-6");
    let loaded: Message = serde_json::from_value(saved.clone()).unwrap();
    assert_eq!(loaded.moved_from_model, message.moved_from_model);
    assert_eq!(loaded.moved_from, Some(Vendor::Claude));
    saved.as_object_mut().unwrap().remove("movedFromModel");
    let old: Message = serde_json::from_value(saved).unwrap();
    assert_eq!(old.moved_from_model, None);
    assert_eq!(old.moved_from, Some(Vendor::Claude));
}

#[test]
fn scoped_pick_table() {
    use faber_engine::route::choose_scoped;
    let models = ["gpt-6.1-sol", "gpt-6.1-terra", "gpt-6-luna"].map(String::from);
    let sets = [candidates(&BOTH, &models), candidates(&[Vendor::Claude], &models), candidates(&[Vendor::Codex], &models)];
    // kind, difficulty, scope, check, project, both, Claude only, Codex only (model and effort)
    let rows = [
        ("chat", "easy", "tight", "glance", false, ("gpt-6.1-terra", "low"), ("claude-haiku-5-5", "medium"), ("gpt-6.1-terra", "low")),
        (
            "writing",
            "medium",
            "tight",
            "glance",
            false,
            ("gpt-6.1-terra", "medium"),
            ("claude-sonnet-5-5", "high"),
            ("gpt-6.1-terra", "medium"),
        ),
        ("research", "hard", "tight", "glance", false, ("gpt-6.1-sol", "medium"), ("claude-opus-5-5", "medium"), ("gpt-6.1-sol", "medium")),
        (
            "research",
            "hard",
            "open",
            "blind",
            false,
            ("claude-fable-5-2", "medium"),
            ("claude-fable-5-2", "medium"),
            ("gpt-6.1-sol", "xhigh"),
        ),
        ("code", "easy", "tight", "machine", false, ("gpt-6.1-terra", "low"), ("claude-haiku-5-5", "medium"), ("gpt-6.1-terra", "low")),
        (
            "code",
            "medium",
            "tight",
            "machine",
            false,
            ("gpt-6.1-terra", "medium"),
            ("claude-sonnet-5-5", "high"),
            ("gpt-6.1-terra", "medium"),
        ),
        ("code", "hard", "tight", "machine", false, ("gpt-6.1-sol", "medium"), ("claude-opus-5-5", "medium"), ("gpt-6.1-sol", "medium")),
        ("code", "hard", "tight", "blind", false, ("gpt-6.1-sol", "high"), ("claude-fable-5-2", "low"), ("gpt-6.1-sol", "high")),
        ("code", "hard", "open", "machine", false, ("gpt-6.1-sol", "high"), ("claude-fable-5-2", "low"), ("gpt-6.1-sol", "high")),
        ("code", "hard", "open", "blind", false, ("claude-fable-5-2", "medium"), ("claude-fable-5-2", "medium"), ("gpt-6.1-sol", "xhigh")),
        ("math", "medium", "tight", "machine", false, ("gpt-6.1-sol", "medium"), ("claude-opus-5-5", "medium"), ("gpt-6.1-sol", "medium")),
        ("research", "hard", "open", "machine", true, ("gpt-6.1-sol", "high"), ("claude-fable-5-2", "low"), ("gpt-6.1-sol", "high")),
    ];
    for (kind, difficulty, scope, check, project, both, claude, codex) in rows {
        for (set, expected) in sets.iter().zip([both, claude, codex]) {
            let p = choose_scoped(kind, difficulty, check, scope, 0.9, project, set).unwrap().profile;
            assert_eq!((p.model, p.effort), expected, "{kind} {difficulty} {scope} {check} project={project}");
        }
        if kind == "code" {
            for set in &sets {
                assert_eq!(
                    choose_scoped(kind, difficulty, check, scope, 0.9, true, set).unwrap().profile,
                    choose_scoped(kind, difficulty, check, scope, 0.9, false, set).unwrap().profile
                );
            }
        }
    }
    for line in [r#"{"difficulty":"hard"}"#, r#"{"difficulty":"hard","scope":"unknown"}"#, r#"{"difficulty":"hard","scope":null}"#] {
        assert_eq!(asked(line).unwrap().scope, "tight");
    }
}

#[test]
fn frontier_does_not_step_up_again() {
    use faber_engine::route::{Factor, choose_paced_scoped, choose_scoped, lifted};
    let candidates = candidates(&BOTH, &["gpt-6.1-sol".into(), "gpt-6.1-terra".into(), "gpt-6-luna".into()]);
    let base = choose_scoped("code", "hard", "blind", "open", 0.2, true, &candidates).unwrap();
    assert_eq!(base.profile.model, "claude-fable-5-2");
    let factor = |_: &Profile| Factor { m: 0.01, ..Factor::NEUTRAL };
    let paced = choose_paced_scoped("code", "hard", "blind", "open", 0.2, true, &candidates, &factor).unwrap();
    assert_eq!(paced.profile, base.profile);
    assert!(paced.pace.is_none());
    assert_eq!(lifted(base.clone(), &candidates, &factor), base);
}
