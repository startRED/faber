//! Auto: a small model that is already running reads the request and says what kind of task it is and how hard; a fixed
//! table then picks the cheapest model that clears the bar for it, priced by the quota left on each subscription.
//!
//! The router never sees a model or company name, not even as anonymous candidates: each vendor's models tend to prefer
//! that vendor's models, and on 44 real requests (10/10/2026) a router choosing among anonymous profiles still overspent
//! on 5 of them. It judges requests well; weighing price against quality is the table's job.

use crate::budget::LOW_M;
use crate::model::{PaceNote, Vendor};
use crate::router::Pick;
use serde::Deserialize;

/// One way to run a model. Numbers from Artificial Analysis (artificialanalysis.ai/models/<model>-<effort>), read
/// between 22/09 and 08/10/2026: `iq` is the Intelligence Index, `agent` Terminal-Bench (multi-step coding and terminal
/// work, %), `secs` a typical whole answer, `cost` US$ to run the index, which is how much a task weighs on the quota.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    pub vendor: Vendor,
    pub model: &'static str,
    pub effort: &'static str,
    pub iq: u8,
    pub agent: u8,
    pub secs: f32,
    pub cost: f32,
}

const fn p(vendor: Vendor, model: &'static str, effort: &'static str, iq: u8, agent: u8, secs: f32, cost: f32) -> Profile {
    Profile { vendor, model, effort, iq, agent, secs, cost }
}

// No maximum effort (minutes per answer at two to three times the cost) and no model another one here beats on every
// number. ponytail: compiled in; it moves to the live manifest (roadmap phase 4) so a new model joins without a release.
pub const PROFILES: [Profile; 9] = [
    p(Vendor::Claude, "claude-opus-5-5", "high", 54, 57, 44.2, 1.82),
    p(Vendor::Claude, "claude-opus-5-5", "medium", 51, 53, 27.9, 1.34),
    p(Vendor::Claude, "claude-sonnet-5-5", "high", 47, 44, 17.5, 0.88),
    p(Vendor::Claude, "claude-sonnet-5-5", "medium", 41, 30, 7.0, 0.48),
    p(Vendor::Claude, "claude-haiku-5-5", "medium", 34, 15, 16.6, 0.05),
    p(Vendor::Codex, "gpt-6.1-sol", "high", 50, 52, 64.8, 0.32),
    p(Vendor::Codex, "gpt-6.1-sol", "medium", 48, 48, 13.4, 0.21),
    p(Vendor::Codex, "gpt-6.1-sol", "low", 42, 31, 8.6, 0.13),
    p(Vendor::Codex, "gpt-6-luna", "medium", 30, 3, 8.8, 0.02),
];

/// The router itself: the fastest cheap model of each vendor.
pub const ROUTER: [(Vendor, &str, &str); 2] = [(Vendor::Codex, "gpt-6-luna", "medium"), (Vendor::Claude, "claude-haiku-5-5", "medium")];

pub const KINDS: [&str; 7] = ["chat", "writing", "code", "math", "research", "image", "other"];
pub const LEVELS: [&str; 3] = ["easy", "medium", "hard"];
pub const CHECKS: [&str; 3] = ["machine", "glance", "blind"];

pub const INSTRUCTIONS: &str = "You classify a user's request for a router. Never answer the request itself.
kind: chat (casual talk, greetings, short facts), writing (texts, emails, translation, stories, plans), code (programs, \
debugging, code review, changes to software), math (calculations, equations, proofs), research (analysis, comparisons, \
explanations, advice), image (making or editing pictures), other.
difficulty: easy (a quick answer any competent person gets right), medium (needs care, several steps or some \
expertise), hard (expert work where a subtle mistake is costly: proofs, multi-file changes, concurrency, architecture, \
legal, medical or financial analysis).
check: machine (a test, exit code, schema or number the work can recount proves it), glance (the person sees at once \
whether it is right), blind (the person cannot easily tell a wrong answer from a right one: facts, advice, analysis, \
subtle code).
Reply with one line of JSON only: \
{\"kind\":\"chat|writing|code|math|research|image|other\",\"difficulty\":\"easy|medium|hard\",\"check\":\"machine|glance|blind\",\
\"confidence\":<0..1>}";

/// The profiles this user can run: a connected CLI and, for Codex, a model the account lists.
pub fn candidates(available: &[Vendor], codex_models: &[String]) -> Vec<Profile> {
    PROFILES
        .into_iter()
        .filter(|p| available.contains(&p.vendor) && (p.vendor != Vendor::Codex || codex_models.iter().any(|m| m == p.model)))
        .collect()
}

// Requests longer than this are cut for routing only; the answer gets the whole text.
const REQUEST_CHARS: usize = 4000;

/// What the router reads: the request alone.
pub fn question(request: &str, attached: usize) -> String {
    let mut text: String = request.chars().take(REQUEST_CHARS).collect();
    if attached > 0 {
        text.push_str(&format!("\n[{attached} attached file(s) or image(s)]"));
    }
    text
}

/// The router's verdict and the model the table picked for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Routed {
    pub profile: Profile,
    pub kind: &'static str,
    pub difficulty: &'static str,
    /// The verdict's `check`, kept so a quota recovery re-picks with the same quality floor.
    pub check: &'static str,
    pub confidence: f64,
    /// What the table compared for the pick: cost x weight x quota factor. Kept for tests and logs.
    pub price: f64,
    /// Set only when the quota changed the pick (see `choose_paced`).
    pub pace: Option<PaceNote>,
}

impl Routed {
    pub fn pick(&self) -> Pick {
        Pick {
            vendor: self.profile.vendor,
            model: Some(self.profile.model.into()),
            effort: self.profile.effort.into(),
            reason: format!("reason.route.{}.{}", self.kind, self.difficulty),
            pace: self.pace.clone(),
        }
    }

    /// True when the picked model is weaker than the one a conversation already has.
    pub fn weaker_than(&self, current: &Pick) -> bool {
        PROFILES
            .iter()
            .find(|p| p.vendor == current.vendor && Some(p.model) == current.model.as_deref() && p.effort == current.effort)
            .is_some_and(|p| (p.iq, p.agent) > (self.profile.iq, self.profile.agent))
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    kind: String,
    difficulty: String,
    #[serde(default)]
    check: String,
    #[serde(default)]
    confidence: f64,
}

/// What the small model said about a request: the only thing kept from the router while the user types. The table runs
/// when the message is sent, on the quota of that moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Asked {
    pub kind: &'static str,
    pub difficulty: &'static str,
    /// How easily the person can tell a wrong answer from a right one (`CHECKS`); missing or unknown reads as `glance`.
    pub check: &'static str,
    pub confidence: f64,
}

/// Reads the router's JSON line; None when it is not one.
pub fn asked(answer: &str) -> Option<Asked> {
    let json = &answer[answer.find('{')?..=answer.rfind('}')?];
    let a: Answer = serde_json::from_str(json).ok()?;
    let kind = KINDS.into_iter().find(|k| *k == a.kind).unwrap_or("other");
    let difficulty = LEVELS.into_iter().find(|l| *l == a.difficulty)?;
    let check = CHECKS.into_iter().find(|c| *c == a.check).unwrap_or("glance");
    Some(Asked { kind, difficulty, check, confidence: a.confidence.clamp(0.0, 1.0) })
}

/// The router's line and the table's pick with no quota information.
pub fn read(answer: &str, candidates: &[Profile]) -> Option<Routed> {
    let a = asked(answer)?;
    choose(a.kind, a.difficulty, a.check, a.confidence, candidates)
}

/// The bar a task must clear: (general intelligence, agentic coding). `expert` is not a level the router can name; it is
/// the rung above `hard` that spare quota can buy.
fn bar(kind: &str, level: &str) -> (u8, u8) {
    match (level, kind) {
        ("easy", "code") => (0, 15),
        ("easy", _) => (0, 0),
        ("medium", "code") => (0, 30),
        // A wrong number reads as right: math gets the hard bar a level early.
        ("medium", "math") => (47, 0),
        ("medium", _) => (41, 0),
        ("hard", "code") => (47, 44),
        ("expert", "code") => (50, 52),
        ("expert", _) => (50, 0),
        _ => (47, 0),
    }
}

const LADDER: [&str; 4] = ["easy", "medium", "hard", "expert"];

/// The bar of the nearest higher level whose bar is different; a task already at the top keeps its own.
fn next_bar(kind: &str, level: &str) -> (u8, u8) {
    let own = bar(kind, level);
    let at = LADDER.iter().position(|l| *l == level).unwrap_or(2);
    LADDER[at + 1..].iter().map(|l| bar(kind, l)).find(|b| *b != own).unwrap_or(own)
}

/// Whole-answer time a step up may take: hard code and math of medium or hard take about 3 minutes, the rest about 1.
fn cap_secs(kind: &str, level: &str) -> f32 {
    if (kind == "code" && level == "hard") || (kind == "math" && level != "easy") { 180.0 } else { 60.0 }
}

/// Spare quota only buys quality where quality matters.
fn matters(kind: &str, level: &str) -> bool {
    matches!(kind, "code" | "math" | "research") && matches!(level, "medium" | "hard")
}

/// An unsure verdict gets the next level's bar, and so does a `blind` one where quality matters: self-checking pays most
/// where the person cannot verify the answer quickly, as with math's early hard bar (`bar`). The two are one step, never
/// two, and a `machine` check lowers nothing: this is raise-only.
fn level_of(kind: &str, difficulty: &'static str, check: &str, confidence: f64) -> &'static str {
    let step = confidence < 0.5 || (check == "blind" && matters(kind, difficulty));
    match (step, difficulty) {
        (true, "easy") => "medium",
        (true, _) => "hard",
        (false, level) => level,
    }
}

/// What the quota says about one profile's price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Factor {
    /// The price multiplier of the profile's windows (budget.rs): below 1 where quota would expire unused, above 1 where it
    /// runs low.
    pub m: f64,
    /// The learned weight of the model in its vendor's quota, 0.5 to 2.
    pub w: f64,
    /// False when any vendor-wide window is above 0.90: such a vendor gets no step up.
    pub step_ok: bool,
}

impl Factor {
    pub const NEUTRAL: Factor = Factor { m: 1.0, w: 1.0, step_ok: true };
}

fn clears(p: &Profile, bar: (u8, u8)) -> bool {
    p.iq >= bar.0 && p.agent >= bar.1
}

/// The cheapest candidate that clears the bar under `factor`, with its price and whether it was priced as a step up.
fn cheapest(
    kind: &str,
    level: &str,
    candidates: &[Profile],
    factor: &dyn Fn(&Profile) -> Factor,
    steps: bool,
) -> Option<(Profile, f64, bool)> {
    let (own, next, cap) = (bar(kind, level), next_bar(kind, level), cap_secs(kind, level));
    let matters = matters(kind, level);
    candidates
        .iter()
        .filter(|p| clears(p, own))
        .map(|p| {
            let f = factor(p);
            let f_m = if matters { f.m } else { f.m.max(1.0) };
            let step = steps && matters && next != own && clears(p, next) && p.secs <= cap && f.step_ok;
            let price = p.cost as f64 * f.w * f_m * if step { f_m.min(1.0) } else { 1.0 };
            (*p, price, step)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.secs.total_cmp(&b.0.secs)))
}

fn strongest(candidates: &[Profile]) -> Option<Profile> {
    candidates.iter().max_by_key(|p| (p.iq, p.agent)).copied()
}

/// The cheapest candidate that clears the bar (the faster on a tie), or the strongest when none does. An unsure verdict
/// gets the next level's bar, as does a blind one (`level_of`). This is the table with no quota information.
pub fn choose(
    kind: &'static str,
    difficulty: &'static str,
    check: &'static str,
    confidence: f64,
    candidates: &[Profile],
) -> Option<Routed> {
    let level = level_of(kind, difficulty, check, confidence);
    let (profile, price) = match cheapest(kind, level, candidates, &|_| Factor::NEUTRAL, false) {
        Some((p, price, _)) => (p, price),
        None => {
            let p = strongest(candidates)?;
            (p, p.cost as f64)
        }
    };
    Some(Routed { profile, kind, difficulty, check, confidence, price, pace: None })
}

/// The table with the quota in it. Every candidate that clears the task's own bar is in the pool at `cost x w x f`, where
/// `f` is the profile's `m` on tasks that matter and at least 1 on the others. On a task that matters, a candidate that
/// also clears the next bar, fits the time cap and is in a vendor with room gets the discount twice. With every factor
/// neutral the pick is exactly `choose`'s. `pace` says why the pick differs from `choose`'s, if the quota is the reason.
pub fn choose_paced(
    kind: &'static str,
    difficulty: &'static str,
    check: &'static str,
    confidence: f64,
    candidates: &[Profile],
    factor: &dyn Fn(&Profile) -> Factor,
) -> Option<Routed> {
    let level = level_of(kind, difficulty, check, confidence);
    let usual = choose(kind, difficulty, check, confidence, candidates)?;
    let Some((profile, price, step)) = cheapest(kind, level, candidates, factor, true) else {
        return Some(usual);
    };
    let pace = if profile == usual.profile {
        None
    } else if step && (profile.iq, profile.agent) > (usual.profile.iq, usual.profile.agent) {
        Some(PaceNote { kind: "step".into(), vendor: profile.vendor })
    } else if factor(&usual.profile).m >= LOW_M && profile.vendor != usual.profile.vendor {
        // "Running low" is said only when the vendor is really near its limit: a window past STEP_LIMIT, which leaves less than
        // one day of the daily line (R88: 100% / 7 = 14.3%). Being ahead of that line is not an alarm; it is "fast".
        let near = !factor(&usual.profile).step_ok;
        Some(PaceNote { kind: if near { "low" } else { "fast" }.into(), vendor: usual.profile.vendor })
    } else {
        None
    };
    Some(Routed { profile, kind, difficulty, check, confidence, price, pace })
}
