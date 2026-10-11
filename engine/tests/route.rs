use faber_engine::model::Vendor;
use faber_engine::route::{Blind, INSTRUCTIONS, PROFILES, Profile, blind, candidates, check, read};

const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

fn all() -> Vec<Profile> {
    candidates(&BOTH, &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()])
}

fn fresh(_: Vendor) -> f64 {
    0.0
}

fn letter_of(b: &Blind, model: &str, effort: &str) -> char {
    let i = b.letters.iter().position(|p| p.model == model && p.effort == effort).unwrap();
    (b'A' + i as u8) as char
}

#[test]
fn the_router_never_sees_a_model_or_company_name() {
    let b = blind("Fix the failing test in my Rust project", 0, &all(), &fresh, 7);
    let seen = format!("{INSTRUCTIONS}\n{}", b.prompt).to_lowercase();
    for name in ["claude", "anthropic", "opus", "sonnet", "haiku", "gpt", "openai", "codex", "chatgpt", "sol", "luna"] {
        assert!(!seen.split(|c: char| !c.is_alphanumeric()).any(|w| w == name), "the router saw {name:?}");
    }
    assert_eq!(b.letters.len(), PROFILES.len());
}

#[test]
fn candidates_are_shuffled_on_every_request_but_a_seed_repeats_its_order() {
    let order = |seed| blind("hi", 0, &all(), &fresh, seed).letters;
    assert_eq!(order(1), order(1));
    let distinct: std::collections::HashSet<String> = (1..20).map(|s| format!("{:?}", order(s))).collect();
    assert!(distinct.len() > 15, "only {} orders in 19 requests", distinct.len());
}

#[test]
fn only_connected_vendors_and_models_the_account_lists_are_candidates() {
    let codex = candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]);
    assert_eq!(codex.iter().map(|p| p.model).collect::<Vec<_>>(), ["gpt-6-luna"]);
    assert!(candidates(&[Vendor::Claude], &[]).iter().all(|p| p.vendor == Vendor::Claude));
}

#[test]
fn a_task_costs_more_where_little_quota_is_left() {
    let luna: Vec<Profile> = all().into_iter().filter(|p| p.model == "gpt-6-luna").collect();
    let cost = |used: f64| blind("hi", 0, &luna, &move |_| used, 1).prompt;
    assert!(cost(0.0).contains("cost 0.02"));
    assert!(cost(0.5).contains("cost 0.08"));
}

#[test]
fn the_answer_maps_its_letter_back_and_names_the_reason() {
    let b = blind("Write a birthday note for my mother", 0, &all(), &fresh, 3);
    let sonnet = letter_of(&b, "claude-sonnet-5-5", "medium");
    let answer = format!("Sure.\n{{\"pick\":\"{sonnet}\",\"kind\":\"writing\",\"difficulty\":\"easy\",\"confidence\":0.9}}");
    let routed = read(&answer, &b).unwrap();
    assert_eq!((routed.profile.model, routed.profile.effort), ("claude-sonnet-5-5", "medium"));
    let pick = routed.pick();
    assert_eq!(
        (pick.vendor, pick.model.as_deref(), pick.reason.as_str()),
        (Vendor::Claude, Some("claude-sonnet-5-5"), "reason.route.writing.easy")
    );
}

#[test]
fn an_answer_without_a_known_letter_is_no_route() {
    let b = blind("hi", 0, &all(), &fresh, 3);
    assert_eq!(read("I think the second one", &b), None);
    assert_eq!(read(r#"{"pick":"Z","kind":"chat","difficulty":"easy","confidence":1}"#, &b), None);
}

#[test]
fn unsure_picks_the_strongest_and_hard_code_never_goes_to_a_weak_model() {
    let b = blind("x", 0, &all(), &fresh, 5);
    let luna = letter_of(&b, "gpt-6-luna", "medium");
    let route = |kind: &str, level: &str, confidence: f64| {
        let answer = format!(r#"{{"pick":"{luna}","kind":"{kind}","difficulty":"{level}","confidence":{confidence}}}"#);
        let r = read(&answer, &b).unwrap();
        (r.profile.model, r.profile.effort)
    };
    assert_eq!(route("chat", "easy", 0.95), ("gpt-6-luna", "medium"));
    assert_eq!(route("chat", "easy", 0.3), ("claude-opus-5-5", "high"));
    // The cheapest model that clears the bar for hard code, not the most expensive one.
    assert_eq!(route("code", "hard", 0.9), ("gpt-6.1-sol", "medium"));
    assert_eq!(route("code", "medium", 0.9), ("gpt-6.1-sol", "low"));
}

#[test]
fn with_one_vendor_the_rules_stay_inside_it() {
    let claude = candidates(&[Vendor::Claude], &[]);
    let haiku = *claude.iter().find(|p| p.model == "claude-haiku-5-5").unwrap();
    let r = check(faber_engine::route::Routed { profile: haiku, kind: "code", difficulty: "hard", confidence: 0.9 }, &claude);
    assert_eq!((r.profile.model, r.profile.effort), ("claude-sonnet-5-5", "high"));
}
