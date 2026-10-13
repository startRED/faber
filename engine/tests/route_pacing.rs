use faber_engine::model::Vendor;
use faber_engine::route::{Asked, Factor, PROFILES, Profile, asked, candidates, choose, choose_paced};

const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

fn all() -> Vec<Profile> {
    candidates(&BOTH, &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()])
}

/// Every profile of a vendor gets that vendor's multiplier; `step_ok` as given for Claude.
fn by_vendor(claude: f64, codex: f64) -> impl Fn(&Profile) -> Factor {
    move |p| Factor { m: if p.vendor == Vendor::Claude { claude } else { codex }, w: 1.0, step_ok: true }
}

fn paced(
    kind: &'static str,
    level: &'static str,
    from: &[Profile],
    f: &dyn Fn(&Profile) -> Factor,
) -> (&'static str, &'static str, Option<(String, Vendor)>) {
    let r = choose_paced(kind, level, "glance", 0.9, from, f).unwrap();
    (r.profile.model, r.profile.effort, r.pace.map(|n| (n.kind, n.vendor)))
}

fn pick(kind: &'static str, level: &'static str, from: &[Profile], f: &dyn Fn(&Profile) -> Factor) -> (&'static str, &'static str) {
    let (m, e, _) = paced(kind, level, from, f);
    (m, e)
}

const SONNET_HIGH: (&str, &str) = ("claude-sonnet-5-5", "high");
const SONNET_MEDIUM: (&str, &str) = ("claude-sonnet-5-5", "medium");
const OPUS_MEDIUM: (&str, &str) = ("claude-opus-5-5", "medium");
const SOL_LOW: (&str, &str) = ("gpt-6.1-sol", "low");
const SOL_MEDIUM: (&str, &str) = ("gpt-6.1-sol", "medium");
const SOL_HIGH: (&str, &str) = ("gpt-6.1-sol", "high");

#[test]
fn medium_code_steps_up_on_spare_claude_until_codex_is_cheaper() {
    let all = all();
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.14, 1.0)), SONNET_HIGH);
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.36, 1.0)), SONNET_HIGH);
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.5625, 1.0)), SOL_LOW);
    // One day of the week left: Claude at the floor, Codex a little over the line.
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.1, 1.29)), SONNET_HIGH);
}

#[test]
fn hard_code_steps_to_the_expert_bar() {
    let all = all();
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.14, 1.0)), OPUS_MEDIUM);
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.30, 1.0)), OPUS_MEDIUM);
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.44, 1.0)), SOL_MEDIUM);
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.14, 0.14)), SOL_HIGH, "64.8 s is within the 180 s cap of hard code");
}

#[test]
fn medium_math_steps_to_the_expert_bar() {
    assert_eq!(pick("math", "medium", &all(), &by_vendor(0.18, 1.0)), OPUS_MEDIUM);
}

#[test]
fn a_task_that_does_not_matter_never_gets_a_discount() {
    assert_eq!(pick("chat", "easy", &all(), &by_vendor(0.14, 1.0)), ("gpt-6-luna", "medium"));
}

#[test]
fn a_vendor_running_low_is_priced_up_and_the_answer_says_so() {
    let (m, e, pace) = paced("code", "medium", &all(), &by_vendor(1.0, 25.0));
    assert_eq!((m, e), SONNET_MEDIUM);
    assert_eq!(pace, Some(("low".to_string(), Vendor::Codex)));
    let (m, _, pace) = paced("chat", "easy", &all(), &by_vendor(1.0, 25.0));
    assert_eq!(m, "claude-haiku-5-5");
    assert_eq!(pace, Some(("low".to_string(), Vendor::Codex)));
}

#[test]
fn a_step_over_the_time_cap_is_not_taken() {
    let codex = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]);
    // Hard research caps at 60 s: Sol high (64.8 s) is no step, so Sol medium wins on price.
    assert_eq!(pick("research", "hard", &codex, &by_vendor(1.0, 0.14)), SOL_MEDIUM);
    let claude = candidates(&[Vendor::Claude], &[]);
    assert_eq!(pick("research", "hard", &claude, &by_vendor(0.14, 1.0)), OPUS_MEDIUM);
}

#[test]
fn a_vendor_with_a_window_above_ninety_percent_gets_no_step() {
    let no_step =
        |p: &Profile| Factor { m: if p.vendor == Vendor::Claude { 0.1 } else { 1.0 }, w: 1.0, step_ok: p.vendor != Vendor::Claude };
    assert_eq!(pick("code", "hard", &all(), &no_step), SONNET_HIGH);
}

#[test]
fn learned_weights_change_the_pick_but_never_show_a_reason() {
    let weighted = |p: &Profile| Factor { m: 1.0, w: if p.vendor == Vendor::Claude { 0.5 } else { 2.0 }, step_ok: true };
    let (m, e, pace) = paced("code", "medium", &all(), &weighted);
    assert_eq!((m, e), SONNET_MEDIUM);
    assert_eq!(pace, None);
}

#[test]
fn a_step_up_names_the_vendor_that_has_quota_to_spare() {
    let (_, _, pace) = paced("code", "hard", &all(), &by_vendor(0.14, 1.0));
    assert_eq!(pace, Some(("step".to_string(), Vendor::Claude)));
}

#[test]
fn with_neutral_factors_the_picks_are_exactly_the_unpaced_table() {
    let sets: Vec<Vec<Profile>> = vec![
        all(),
        candidates(&[Vendor::Claude], &[]),
        candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]),
        candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]),
        candidates(&BOTH, &["gpt-6.1-sol".to_string()]),
    ];
    for set in &sets {
        for kind in ["chat", "writing", "code", "math", "research", "image", "other"] {
            for level in ["easy", "medium", "hard"] {
                for confidence in [0.2, 0.9] {
                    let plain = choose(kind, level, "glance", confidence, set).unwrap();
                    let neutral = choose_paced(kind, level, "glance", confidence, set, &|_| Factor::NEUTRAL).unwrap();
                    assert_eq!(plain.profile, neutral.profile, "{kind} {level} {confidence}");
                    assert_eq!(neutral.pace, None);
                }
            }
        }
    }
}

#[test]
fn the_best_price_never_rises_when_a_vendor_gets_cheaper() {
    let all = all();
    let steps = [25.0, 12.0, 6.0, 3.0, 1.5, 1.0, 0.8, 0.6, 0.45, 0.3, 0.2, 0.14, 0.1];
    for (kind, level) in
        [("code", "medium"), ("code", "hard"), ("math", "medium"), ("research", "hard"), ("writing", "medium"), ("chat", "easy")]
    {
        for codex in [0.1, 0.5, 1.0, 4.0] {
            let mut last = f64::INFINITY;
            for claude in steps {
                let price = choose_paced(kind, level, "glance", 0.9, &all, &by_vendor(claude, codex)).unwrap().price;
                assert!(price <= last + 1e-12, "{kind} {level}: price rose from {last} to {price} at m {claude} (codex {codex})");
                last = price;
            }
        }
    }
}

#[test]
fn the_router_line_is_read_without_the_table() {
    let a = asked("Sure.\n{\"kind\":\"code\",\"difficulty\":\"hard\",\"confidence\":0.8}").unwrap();
    assert_eq!(a, Asked { kind: "code", difficulty: "hard", check: "glance", confidence: 0.8 });
    assert_eq!(asked("no verdict"), None);
}

#[test]
fn luna_at_medium_is_thirty() {
    let luna = PROFILES.iter().find(|p| p.model == "gpt-6-luna").unwrap();
    assert_eq!(luna.iq, 30);
}
