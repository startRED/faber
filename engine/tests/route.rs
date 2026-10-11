use faber_engine::model::Vendor;
use faber_engine::route::{INSTRUCTIONS, Profile, candidates, choose, priced, question, read};

const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

fn all() -> Vec<Profile> {
    candidates(&BOTH, &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()])
}

fn picked(kind: &'static str, level: &'static str, confidence: f64, from: &[Profile]) -> (&'static str, &'static str) {
    let r = choose(kind, level, confidence, from).unwrap();
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
fn a_task_moves_to_the_other_subscription_when_little_quota_is_left() {
    let codex_nearly_out = priced(&all(), &|v| if v == Vendor::Codex { 0.9 } else { 0.0 });
    assert_eq!(picked("chat", "easy", 0.9, &codex_nearly_out), ("claude-haiku-5-5", "medium"));
    let luna = priced(&candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]), &|_| 0.5);
    assert!((luna[0].cost - 0.08).abs() < 1e-6);
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
