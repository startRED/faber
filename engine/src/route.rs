//! Auto: a small model that is already running reads the request and says what kind of task it is and how hard; a fixed
//! table then picks the cheapest model that clears the bar for it, priced by the quota left on each subscription.
//!
//! The router never sees a model or company name, not even as anonymous candidates: each vendor's models tend to prefer
//! that vendor's models, and on 44 real requests (10/10/2026) a router choosing among anonymous profiles still overspent
//! on 5 of them. It judges requests well; weighing price against quality is the table's job.

use crate::model::Vendor;
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
// number. ponytail: compiled in; it moves to the live manifest (phase F11) so a new model joins without a release.
pub const PROFILES: [Profile; 9] = [
    p(Vendor::Claude, "claude-opus-5-5", "high", 54, 57, 44.2, 1.82),
    p(Vendor::Claude, "claude-opus-5-5", "medium", 51, 53, 27.9, 1.34),
    p(Vendor::Claude, "claude-sonnet-5-5", "high", 47, 44, 17.5, 0.88),
    p(Vendor::Claude, "claude-sonnet-5-5", "medium", 41, 30, 7.0, 0.48),
    p(Vendor::Claude, "claude-haiku-5-5", "medium", 34, 15, 16.6, 0.05),
    p(Vendor::Codex, "gpt-6.1-sol", "high", 50, 52, 64.8, 0.32),
    p(Vendor::Codex, "gpt-6.1-sol", "medium", 48, 48, 13.4, 0.21),
    p(Vendor::Codex, "gpt-6.1-sol", "low", 42, 31, 8.6, 0.13),
    p(Vendor::Codex, "gpt-6-luna", "medium", 29, 3, 8.8, 0.02),
];

/// The router itself: the fastest cheap model of each vendor.
pub const ROUTER: [(Vendor, &str, &str); 2] = [(Vendor::Codex, "gpt-6-luna", "medium"), (Vendor::Claude, "claude-haiku-5-5", "medium")];

pub const KINDS: [&str; 7] = ["chat", "writing", "code", "math", "research", "image", "other"];
pub const LEVELS: [&str; 3] = ["easy", "medium", "hard"];

pub const INSTRUCTIONS: &str = "You classify a user's request for a router. Never answer the request itself.
kind: chat (casual talk, greetings, short facts), writing (texts, emails, translation, stories, plans), code (programs, \
debugging, code review, changes to software), math (calculations, equations, proofs), research (analysis, comparisons, \
explanations, advice), image (making or editing pictures), other.
difficulty: easy (a quick answer any competent person gets right), medium (needs care, several steps or some \
expertise), hard (expert work where a subtle mistake is costly: proofs, multi-file changes, concurrency, architecture, \
legal, medical or financial analysis).
Reply with one line of JSON only: \
{\"kind\":\"chat|writing|code|math|research|image|other\",\"difficulty\":\"easy|medium|hard\",\"confidence\":<0..1>}";

/// The profiles this user can run: a connected CLI and, for Codex, a model the account lists.
pub fn candidates(available: &[Vendor], codex_models: &[String]) -> Vec<Profile> {
    PROFILES
        .into_iter()
        .filter(|p| available.contains(&p.vendor) && (p.vendor != Vendor::Codex || codex_models.iter().any(|m| m == p.model)))
        .collect()
}

/// `used` is the share of each vendor's quota already spent (0 to 1): a task costs more where little is left.
pub fn priced(candidates: &[Profile], used: &dyn Fn(Vendor) -> f64) -> Vec<Profile> {
    candidates
        .iter()
        .map(|p| {
            let left = (1.0 - used(p.vendor).clamp(0.0, 0.95)) as f32;
            Profile { cost: p.cost / (left * left), ..*p }
        })
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
    pub confidence: f64,
}

impl Routed {
    pub fn pick(&self) -> Pick {
        Pick {
            vendor: self.profile.vendor,
            model: Some(self.profile.model.into()),
            effort: self.profile.effort.into(),
            reason: format!("reason.route.{}.{}", self.kind, self.difficulty),
        }
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    kind: String,
    difficulty: String,
    #[serde(default)]
    confidence: f64,
}

/// Reads the router's JSON line; None when it is not one.
pub fn read(answer: &str, candidates: &[Profile]) -> Option<Routed> {
    let json = &answer[answer.find('{')?..=answer.rfind('}')?];
    let a: Answer = serde_json::from_str(json).ok()?;
    let kind = KINDS.into_iter().find(|k| *k == a.kind).unwrap_or("other");
    let difficulty = LEVELS.into_iter().find(|l| *l == a.difficulty)?;
    choose(kind, difficulty, a.confidence.clamp(0.0, 1.0), candidates)
}

/// The bar a task must clear: (general intelligence, agentic coding).
fn bar(kind: &str, difficulty: &str) -> (u8, u8) {
    match (difficulty, kind) {
        ("easy", "code") => (0, 15),
        ("easy", _) => (0, 0),
        ("medium", "code") => (0, 30),
        // A wrong number reads as right: math gets the hard bar a level early.
        ("medium", "math") => (47, 0),
        ("medium", _) => (41, 0),
        ("hard", "code") => (47, 44),
        _ => (47, 0),
    }
}

/// The cheapest candidate that clears the bar (the faster on a tie), or the strongest when none does. An unsure verdict
/// gets the next level's bar.
pub fn choose(kind: &'static str, difficulty: &'static str, confidence: f64, candidates: &[Profile]) -> Option<Routed> {
    let level = match (confidence < 0.5, difficulty) {
        (true, "easy") => "medium",
        (true, _) => "hard",
        (false, level) => level,
    };
    let (iq, agent) = bar(kind, level);
    let profile = candidates
        .iter()
        .filter(|p| p.iq >= iq && p.agent >= agent)
        .min_by(|a, b| a.cost.total_cmp(&b.cost).then(a.secs.total_cmp(&b.secs)))
        .or_else(|| candidates.iter().max_by_key(|p| (p.iq, p.agent)))?;
    Some(Routed { profile: *profile, kind, difficulty, confidence })
}
