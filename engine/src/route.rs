//! Auto: a small model that is already running picks who answers, without ever seeing a model or company name.
//!
//! Each model a vendor makes tends to prefer that vendor's models, so candidates reach the router as letters with
//! measured numbers, shuffled on every request. A few fixed rules check the pick afterwards.

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

pub const INSTRUCTIONS: &str = "You route a user's request to the best candidate model. Never answer the request itself.
Candidates are anonymous letters with measured numbers: iq = general reasoning index (0-100); agent = multi-step coding \
and terminal work (0-100); secs = time for a typical answer; cost = how much one task weighs on the user's quota right now.
Choose the candidate that gives the best result for this request without wasting quota or time: casual chat, translation \
and short facts need a fast cheap one; writing and analysis need high iq; code changes, debugging and multi-file work \
need high agent; very hard or high-stakes work needs the strongest.
Reply with one line of JSON only: \
{\"pick\":\"<letter>\",\"kind\":\"chat|writing|code|math|research|image|other\",\"difficulty\":\"easy|medium|hard\",\"confidence\":<0..1>}";

/// The profiles this user can run: a connected CLI and, for Codex, a model the account lists.
pub fn candidates(available: &[Vendor], codex_models: &[String]) -> Vec<Profile> {
    PROFILES
        .into_iter()
        .filter(|p| available.contains(&p.vendor) && (p.vendor != Vendor::Codex || codex_models.iter().any(|m| m == p.model)))
        .collect()
}

/// What the router is asked, and which profile each letter stands for.
#[derive(Debug, Clone, PartialEq)]
pub struct Blind {
    pub prompt: String,
    pub letters: Vec<Profile>,
}

// Requests longer than this are cut for routing only; the answer gets the whole text.
const REQUEST_CHARS: usize = 4000;

/// `used` is the share of each vendor's quota already spent (0 to 1): a task costs more where little is left.
pub fn blind(request: &str, attached: usize, candidates: &[Profile], used: &dyn Fn(Vendor) -> f64, seed: u64) -> Blind {
    let mut letters = candidates.to_vec();
    // splitmix64: a fresh order on every request, the same order for the same seed.
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let z = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    for i in (1..letters.len()).rev() {
        letters.swap(i, (next() % (i as u64 + 1)) as usize);
    }
    let lines: Vec<String> = letters
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let left = (1.0 - used(p.vendor).clamp(0.0, 0.95)) as f32;
            let letter = (b'A' + i as u8) as char;
            format!("{letter}: iq {} agent {} secs {:.0} cost {:.2}", p.iq, p.agent, p.secs, p.cost / (left * left))
        })
        .collect();
    let mut text: String = request.chars().take(REQUEST_CHARS).collect();
    if attached > 0 {
        text.push_str(&format!("\n[{attached} attached file(s) or image(s)]"));
    }
    Blind { prompt: format!("Candidates:\n{}\n\nRequest:\n{text}", lines.join("\n")), letters }
}

/// The router's pick, checked.
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
    pick: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    difficulty: String,
    #[serde(default)]
    confidence: f64,
}

/// Reads the router's JSON line and applies the fixed rules; None when the answer names no candidate.
pub fn read(answer: &str, blind: &Blind) -> Option<Routed> {
    let json = &answer[answer.find('{')?..=answer.rfind('}')?];
    let a: Answer = serde_json::from_str(json).ok()?;
    let letter = a.pick.trim().bytes().next()?.to_ascii_uppercase();
    let profile = *blind.letters.get(letter.checked_sub(b'A')? as usize)?;
    let kind = KINDS.into_iter().find(|k| *k == a.kind).unwrap_or("other");
    let difficulty = LEVELS.into_iter().find(|l| *l == a.difficulty).unwrap_or("medium");
    Some(check(Routed { profile, kind, difficulty, confidence: a.confidence.clamp(0.0, 1.0) }, &blind.letters))
}

/// Unsure picks the strongest; hard work and code changes never go to a model below the bar for them.
pub fn check(mut r: Routed, candidates: &[Profile]) -> Routed {
    let strongest = candidates.iter().max_by_key(|p| (p.iq, p.agent)).copied().unwrap_or(r.profile);
    if r.confidence < 0.5 {
        r.profile = strongest;
        return r;
    }
    let (iq, agent) = match (r.difficulty, r.kind) {
        ("hard", "code") => (47, 44),
        ("hard", _) => (47, 0),
        ("medium", "code") => (0, 30),
        _ => (0, 0),
    };
    if r.profile.iq < iq || r.profile.agent < agent {
        let fit = candidates.iter().filter(|p| p.iq >= iq && p.agent >= agent).min_by(|a, b| a.cost.total_cmp(&b.cost));
        r.profile = fit.copied().unwrap_or(strongest);
    }
    r
}
