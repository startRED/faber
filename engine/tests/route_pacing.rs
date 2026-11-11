use faber_engine::model::Vendor;
use faber_engine::route::{Asked, Factor, PROFILES, Profile, Reading, asked, candidates, choose, choose_paced, read_cost};

const BOTH: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

fn all() -> Vec<Profile> {
    candidates(&BOTH, &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()])
}

/// Every profile of a vendor gets that vendor's multiplier; `step_ok` as given for Claude.
fn by_vendor(claude: f64, codex: f64) -> impl Fn(&Profile) -> Factor {
    move |p| Factor { m: if p.vendor == Vendor::Claude { claude } else { codex }, w: 1.0, step_ok: true, read: 0.0 }
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
const FABLE_MEDIUM: (&str, &str) = ("claude-fable-5-2", "medium");
const OPUS_MEDIUM: (&str, &str) = ("claude-opus-5-5", "medium");
const SOL_MEDIUM: (&str, &str) = ("gpt-6.1-sol", "medium");
const SOL_HIGH: (&str, &str) = ("gpt-6.1-sol", "high");

#[test]
fn medium_code_steps_up_on_spare_claude_until_codex_is_cheaper() {
    let all = all();
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.14, 1.0)), OPUS_MEDIUM);
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.36, 1.0)), OPUS_MEDIUM);
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.5625, 1.0)), SOL_MEDIUM);
    // One day of the week left: Claude at the floor, Codex a little over the line.
    assert_eq!(pick("code", "medium", &all, &by_vendor(0.1, 1.29)), OPUS_MEDIUM);
}

#[test]
fn hard_code_steps_to_the_expert_bar() {
    let all = all();
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.14, 1.0)), FABLE_MEDIUM);
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.30, 1.0)), FABLE_MEDIUM);
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.44, 1.0)), SOL_MEDIUM);
    assert_eq!(pick("code", "hard", &all, &by_vendor(0.14, 0.14)), SOL_HIGH, "394 s is within the 1,200 s cap of hard code");
}

#[test]
fn medium_math_steps_to_the_expert_bar() {
    assert_eq!(pick("math", "medium", &all(), &by_vendor(0.18, 1.0)), FABLE_MEDIUM);
}

#[test]
fn a_task_that_does_not_matter_never_gets_a_discount() {
    assert_eq!(pick("chat", "easy", &all(), &by_vendor(0.14, 1.0)), ("claude-haiku-5-5", "medium"));
    let with_terra = candidates(&BOTH, &["gpt-6.1-sol".into(), "gpt-6.1-terra".into(), "gpt-6-luna".into()]);
    assert_eq!(
        pick("chat", "easy", &with_terra, &by_vendor(0.14, 1.0)),
        ("gpt-6.1-terra", "low"),
        "spare Claude must not discount Haiku below Terra for easy chat"
    );
}

#[test]
fn a_vendor_ahead_of_its_daily_line_is_priced_up_and_the_answer_says_it_is_fast_not_that_it_runs_out() {
    let (m, e, pace) = paced("code", "medium", &all(), &by_vendor(1.0, 25.0));
    assert_eq!((m, e), SONNET_HIGH);
    assert_eq!(pace, Some(("fast".to_string(), Vendor::Codex)));
    let (m, _, pace) = paced("chat", "easy", &all(), &by_vendor(1.0, 25.0));
    assert_eq!(m, "claude-haiku-5-5");
    assert_eq!(pace, None, "easy already used Haiku before quota changed");
}

#[test]
fn only_a_vendor_near_its_limit_is_said_to_be_running_low() {
    // A window past 90% leaves under one day of the daily line (R88: 100% / 7 = 14.3%): that is near the limit.
    let near = |p: &Profile| Factor {
        m: if p.vendor == Vendor::Claude { 1.0 } else { 25.0 },
        w: 1.0,
        step_ok: p.vendor == Vendor::Claude,
        read: 0.0,
    };
    let (m, _, pace) = paced("code", "medium", &all(), &near);
    assert_eq!(m, SONNET_HIGH.0);
    assert_eq!(pace, Some(("low".to_string(), Vendor::Codex)));
}

#[test]
fn a_step_over_the_time_cap_is_not_taken() {
    let codex = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string(), "gpt-6-luna".to_string()]);
    // Sol high (394 s) now fits the 600 s cap; Opus xhigh (976 s) still cannot be a research step.
    assert_eq!(pick("research", "hard", &codex, &by_vendor(1.0, 0.14)), SOL_HIGH);
    let claude = candidates(&[Vendor::Claude], &[]);
    assert_eq!(pick("research", "hard", &claude, &by_vendor(0.14, 1.0)), FABLE_MEDIUM);
    let capped = two(OPUS_MEDIUM, ("claude-opus-5-5", "xhigh"));
    assert_eq!(pick("research", "hard", &capped, &by_vendor(0.14, 1.0)), OPUS_MEDIUM);
    assert_eq!(pick("code", "hard", &capped, &by_vendor(0.14, 1.0)), ("claude-opus-5-5", "xhigh"), "976 s fits the code cap of 1,200 s");
}

#[test]
fn a_vendor_with_a_window_above_ninety_percent_gets_no_step() {
    let no_step = |p: &Profile| Factor {
        m: if p.vendor == Vendor::Claude { 0.1 } else { 1.0 },
        w: 1.0,
        step_ok: p.vendor != Vendor::Claude,
        read: 0.0,
    };
    assert_eq!(pick("code", "hard", &all(), &no_step), OPUS_MEDIUM);
}

#[test]
fn learned_weights_change_the_pick_but_never_show_a_reason() {
    let weighted = |p: &Profile| Factor { m: 1.0, w: if p.vendor == Vendor::Claude { 0.5 } else { 2.0 }, step_ok: true, read: 0.0 };
    let (m, e, pace) = paced("code", "medium", &all(), &weighted);
    assert_eq!((m, e), SOL_MEDIUM);
    assert_eq!(pace, None);
    let with_terra = candidates(&BOTH, &["gpt-6.1-terra".into()]);
    assert_eq!(pick("chat", "easy", &with_terra, &by_vendor(1.0, 1.0)), ("gpt-6.1-terra", "low"));
    assert_eq!(paced("chat", "easy", &with_terra, &weighted), ("claude-haiku-5-5", "medium", None));
}

#[test]
fn chat_buys_slower_quality_within_fifteen_percent_and_300_seconds() {
    let terra = candidates(&[Vendor::Codex], &["gpt-6.1-terra".into()]);
    for (level, weight, too_costly, baseline) in [("easy", 0.3, 0.32, "low"), ("medium", 0.7, 0.8, "medium")] {
        let weighted = |w| move |p: &Profile| Factor { w: if p.effort == "high" { w } else { 1.0 }, ..Factor::NEUTRAL };
        let base = terra.iter().find(|p| p.effort == baseline).unwrap();
        for seconds in [241.0, 300.0, 301.0] {
            let pool: Vec<_> = terra.iter().map(|p| if p.effort == "high" { Profile { secs: seconds, ..*p } } else { *p }).collect();
            let row = choose_paced("chat", level, "glance", 0.9, &pool, &weighted(weight)).unwrap();
            assert_eq!(row.profile.effort, if seconds <= 300.0 { "high" } else { baseline }, "{level}, {seconds} s");
            if seconds <= 300.0 {
                assert!(row.profile.iq > base.iq && row.profile.agent > base.agent);
                assert!(row.profile.secs > base.secs);
                assert!(row.price > base.cost as f64 && row.price <= base.cost as f64 * 1.15);
            }
            assert_eq!(pick("chat", level, &pool, &weighted(too_costly)), ("gpt-6.1-terra", baseline));
        }
    }
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
fn the_minimum_price_never_rises_and_extra_quality_stays_within_fifteen_percent() {
    let all = all();
    let steps = [25.0, 12.0, 6.0, 3.0, 1.5, 1.0, 0.8, 0.6, 0.45, 0.3, 0.2, 0.14, 0.1];
    for (kind, level) in
        [("code", "medium"), ("code", "hard"), ("math", "medium"), ("research", "hard"), ("writing", "medium"), ("chat", "easy")]
    {
        for codex in [0.1, 0.5, 1.0, 4.0] {
            let mut last_min = f64::INFINITY;
            let mut last_pick: Option<(f64, Profile)> = None;
            for claude in steps {
                let factors = by_vendor(claude, codex);
                let row = choose_paced(kind, level, "glance", 0.9, &all, &factors).unwrap();
                let floor = match (kind, level) {
                    ("code", "medium") => (24, 32),
                    ("code", _) => (46, 39),
                    ("math" | "research", _) => (46, 0),
                    ("writing", _) => (38, 0),
                    _ => (24, 0),
                };
                let min = all
                    .iter()
                    .filter(|p| p.iq >= floor.0 && p.agent >= floor.1)
                    .map(|p| choose_paced(kind, level, "glance", 0.9, &[*p], &factors).unwrap().price)
                    .min_by(f64::total_cmp)
                    .unwrap();
                assert!(min <= last_min + 1e-12, "{kind} {level}: minimum price rose at m {claude}, codex {codex}");
                assert!(row.price <= min * 1.15 + 1e-12, "{kind} {level}: premium exceeds 15%");
                if let Some((price, previous)) = last_pick.filter(|(price, _)| row.price > *price + 1e-12) {
                    assert!(row.profile.iq >= previous.iq && row.profile.agent >= previous.agent);
                    assert!(
                        row.profile.iq > previous.iq || row.profile.agent > previous.agent,
                        "extra cost {price} -> {} must buy quality",
                        row.price
                    );
                }
                last_min = min;
                last_pick = Some((row.price, row.profile));
            }
        }
    }
}

#[test]
fn the_router_line_is_read_without_the_table() {
    let a = asked("Sure.\n{\"kind\":\"code\",\"difficulty\":\"hard\",\"confidence\":0.8}").unwrap();
    assert_eq!(a, Asked { kind: "code", difficulty: "hard", check: "glance", scope: "tight", confidence: 0.8 });
    assert_eq!(asked("no verdict"), None);
}

#[test]
fn luna_at_medium_is_nineteen() {
    let luna = PROFILES.iter().find(|p| p.model == "gpt-6-luna" && p.effort == "medium").unwrap();
    assert_eq!(luna.iq, 19);
}

fn two(a: (&str, &str), b: (&str, &str)) -> Vec<Profile> {
    PROFILES.into_iter().filter(|p| (p.model, p.effort) == a || (p.model, p.effort) == b).collect()
}

fn reading_on(claude: f64, reading: Reading) -> impl Fn(&Profile) -> Factor {
    move |p| Factor { read: read_cost(p, &reading), ..by_vendor(claude, 1.0)(p) }
}

#[test]
fn what_a_model_already_holds_is_read_from_its_cache_and_fresh_everywhere_else() {
    let sol = *PROFILES.iter().find(|p| (p.model, p.effort) == ("gpt-6.1-sol", "medium")).unwrap();
    let opus = *PROFILES.iter().find(|p| (p.model, p.effort) == ("claude-opus-5-5", "medium")).unwrap();
    let long = Reading::of(0, 0, 0, 600_000, Some("gpt-6.1-sol"));
    assert!(read_cost(&sol, &long) * 10.0 < read_cost(&opus, &long), "a cached read is about a tenth of a fresh one");
    assert_eq!(read_cost(&sol, &Reading::default()), 0.0);
    assert!(
        (read_cost(&sol, &Reading { fresh: 1_000, ..Reading::default() }) - 0.0025).abs() < 1e-12,
        "reading is in US$, with no old index-cost conversion"
    );
    // A model the table does not know holds nothing Auto can price, so nothing counts as held.
    assert_eq!(Reading::of(0, 0, 0, 600_000, Some("sonnet")), Reading::default());
    // A page reads for almost nothing next to a task's own price; a book does not.
    let page = Reading::of(4_000, 0, 0, 0, None);
    let book = Reading::of(0, 0, 2_000_000, 0, None);
    assert!(read_cost(&opus, &page) < opus.cost as f64 / 100.0);
    assert!(read_cost(&opus, &book) > opus.cost as f64);
}

#[test]
fn spare_quota_does_not_move_a_long_conversation_off_the_model_that_holds_it() {
    let from = two(("claude-opus-5-5", "medium"), ("gpt-6.1-sol", "medium"));
    let spare = 0.1;
    assert_eq!(pick("research", "hard", &from, &by_vendor(spare, 1.0)).0, "claude-opus-5-5", "a short read takes the spare quota");
    let held = Reading::of(200, 0, 0, 600_000, Some("gpt-6.1-sol"));
    let (model, _, pace) = paced("research", "hard", &from, &reading_on(spare, held));
    assert_eq!((model, pace), ("gpt-6.1-sol", None), "reading 600k fresh on Opus costs more than the quota saves");
}

#[test]
fn a_long_read_goes_past_a_price_tier_only_when_it_has_to() {
    // Haiku costs five times as much a token above 100k: the same 300k document weighs far more there than 90k does.
    let haiku = *PROFILES.iter().find(|p| p.model == "claude-haiku-5-5").unwrap();
    let small = read_cost(&haiku, &Reading::of(0, 0, 360_000, 0, None));
    let large = read_cost(&haiku, &Reading::of(0, 0, 1_200_000, 0, None));
    assert!(large > small * 3.0 * 4.0, "{large} vs {small}");
}
