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

/// One way to run a model. Artificial Analysis per-task runs, republished by slopalytics.com on 2026-11-10.
/// Intelligence Index (`iq`) and the coding sub-index (`agent`) are rounded to whole numbers; `secs` is seconds per
/// task and `cost` US$ per task, all from the same run and scale. No `max`: its extra quota and time are outside Auto's
/// ladder. Fable starts at medium; Luna high is the maker's starting effort when only Luna is available.
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

// shortcut: compiled in until the live manifest lets measured models join without a release.
pub const PROFILES: [Profile; 24] = [
    p(Vendor::Claude, "claude-fable-5-2", "high", 62, 53, 1059.0, 9.078),
    p(Vendor::Claude, "claude-fable-5-2", "medium", 56, 47, 497.0, 3.803),
    p(Vendor::Claude, "claude-opus-5-5", "xhigh", 57, 50, 976.0, 6.047),
    p(Vendor::Claude, "claude-opus-5-5", "high", 52, 44, 573.0, 3.465),
    p(Vendor::Claude, "claude-opus-5-5", "medium", 48, 41, 427.0, 2.451),
    p(Vendor::Claude, "claude-opus-5-5", "low", 35, 22, 181.0, 0.976),
    p(Vendor::Claude, "claude-sonnet-5-5", "xhigh", 50, 44, 913.0, 4.092),
    p(Vendor::Claude, "claude-sonnet-5-5", "high", 42, 33, 427.0, 1.636),
    p(Vendor::Claude, "claude-sonnet-5-5", "medium", 31, 19, 221.0, 0.781),
    p(Vendor::Claude, "claude-sonnet-5-5", "low", 26, 13, 155.0, 0.511),
    p(Vendor::Claude, "claude-haiku-5-5", "xhigh", 35, 30, 584.0, 0.248),
    p(Vendor::Claude, "claude-haiku-5-5", "high", 30, 23, 389.0, 0.159),
    p(Vendor::Claude, "claude-haiku-5-5", "medium", 25, 16, 245.0, 0.092),
    p(Vendor::Claude, "claude-haiku-5-5", "low", 19, 10, 128.0, 0.044),
    p(Vendor::Codex, "gpt-6.1-sol", "xhigh", 53, 45, 556.0, 0.764),
    p(Vendor::Codex, "gpt-6.1-sol", "high", 50, 43, 394.0, 0.591),
    p(Vendor::Codex, "gpt-6.1-sol", "medium", 46, 39, 229.0, 0.359),
    p(Vendor::Codex, "gpt-6.1-sol", "low", 35, 24, 106.0, 0.161),
    p(Vendor::Codex, "gpt-6.1-terra", "xhigh", 45, 41, 364.0, 0.401),
    p(Vendor::Codex, "gpt-6.1-terra", "high", 42, 39, 241.0, 0.259),
    p(Vendor::Codex, "gpt-6.1-terra", "medium", 38, 34, 156.0, 0.169),
    p(Vendor::Codex, "gpt-6.1-terra", "low", 29, 21, 65.0, 0.071),
    p(Vendor::Codex, "gpt-6-luna", "high", 21, 13, 336.0, 0.073),
    p(Vendor::Codex, "gpt-6-luna", "medium", 19, 9, 202.0, 0.042),
];

/// The router itself: the fastest cheap model of each vendor.
pub const ROUTER: [(Vendor, &str, &str); 2] = [(Vendor::Codex, "gpt-6-luna", "medium"), (Vendor::Claude, "claude-haiku-5-5", "medium")];

pub const KINDS: [&str; 7] = ["chat", "writing", "code", "math", "research", "image", "other"];
pub const LEVELS: [&str; 3] = ["easy", "medium", "hard"];
pub const SCOPES: [&str; 2] = ["tight", "open"];
pub const CHECKS: [&str; 3] = ["machine", "glance", "blind"];
/// The scope of a request whose person asked outright for the strongest model, the highest effort or the best quality:
/// the router says so (`best`) and the table gives it the top rung (`scoped`). Never a router `scope` value.
pub const BEST: &str = "best";

pub const INSTRUCTIONS: &str = "You classify a user's request for a router. Never answer the request itself.
kind: chat (casual talk, greetings, short facts), writing (texts, emails, documents, slides, spreadsheets, translation, stories, plans), code (programs, \
debugging, code review, changes to software), math (calculations, equations, proofs), research (analysis, comparisons, \
explanations, advice), image (making or editing pictures), other.
difficulty: judge what the work is, not how long the message is. When two levels fit, choose the harder one.
easy: a greeting or short fact; sorting, tagging, extracting or summarising text already given; finding a file or reading a log; a one-line rewrite or rename; simple arithmetic.
medium: an everyday document, email, slide or spreadsheet; a translation; well-scoped code (a named bug, one function or file, a change from a clear spec, a small script); a routine investigation, review or explanation.
hard: engineering across several files, or a bug with no known cause; a feature that touches several parts; a long piece built in several stages (a video, an app, a site, a long report); reviewing a large change; reading many screenshots or operating a computer; a financial model or a document built from many sources; a proof; anything where a subtle mistake is costly (concurrency, security, legal, medical, money).
scope: tight (the finished result can be stated in one sentence: a named bug or one with clear steps to reproduce, one file or function, a spec, one deliverable, a feature whose parts are already decided even when it touches several files); open (explore and decide, choices nobody has written down, many services, sources or milestones, migration, major refactor, architecture, deep research, long unattended run). A clear spec does not make a migration or a major refactor tight.
check: machine (a test, exit code, schema or number the work can recount proves it), glance (the person sees at once \
whether it is right), blind (the person cannot easily tell a wrong answer from a right one: facts, advice, analysis, \
subtle code).
best: true only when the person explicitly asks for the strongest or best model, the highest effort or the highest \
quality; otherwise false.
Reply with one line of JSON only: \
{\"kind\":\"chat|writing|code|math|research|image|other\",\"difficulty\":\"easy|medium|hard\",\"check\":\"machine|glance|blind\",\
\"scope\":\"tight|open\",\"best\":true|false,\"confidence\":<0..1>}";

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
    pub scope: &'static str,
    pub project: bool,
    pub confidence: f64,
    /// What the table compared for the pick: cost x weight x quota factor. Kept for tests and logs.
    pub price: f64,
    /// Set only when the quota changed the pick (see `choose_paced`), or when what the person redid did (see `lifted`).
    pub pace: Option<PaceNote>,
    /// Per-message target before restart hysteresis; absent for a model selection.
    pub wanted_effort: Option<&'static str>,
}

impl Routed {
    pub fn clears_bar(&self) -> bool {
        clears(&self.profile, bar(self.kind, level_of(self.kind, self.difficulty, self.check, self.scope, self.confidence), self.project))
    }
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
    scope: serde_json::Value,
    #[serde(default)]
    confidence: f64,
    #[serde(default)]
    best: bool,
}

/// What the small model said about a request: the only thing kept from the router while the user types. The table runs
/// when the message is sent, on the quota of that moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Asked {
    pub kind: &'static str,
    pub difficulty: &'static str,
    /// How easily the person can tell a wrong answer from a right one (`CHECKS`); missing or unknown reads as `glance`.
    pub check: &'static str,
    pub scope: &'static str,
    pub confidence: f64,
}

/// Reads the router's JSON line; None when it is not one.
pub fn asked(answer: &str) -> Option<Asked> {
    let json = &answer[answer.find('{')?..=answer.rfind('}')?];
    let a: Answer = serde_json::from_str(json).ok()?;
    let kind = KINDS.into_iter().find(|k| *k == a.kind).unwrap_or("other");
    let difficulty = LEVELS.into_iter().find(|l| *l == a.difficulty)?;
    let check = CHECKS.into_iter().find(|c| *c == a.check).unwrap_or("glance");
    let scope = if a.best { BEST } else { SCOPES.into_iter().find(|c| Some(*c) == a.scope.as_str()).unwrap_or("tight") };
    Some(Asked { kind, difficulty, check, scope, confidence: a.confidence.clamp(0.0, 1.0) })
}

/// The router's line and the table's pick with no quota information.
pub fn read(answer: &str, candidates: &[Profile]) -> Option<Routed> {
    let a = asked(answer)?;
    choose_scoped(a.kind, a.difficulty, a.check, a.scope, a.confidence, false, candidates)
}

/// The bar a task must clear: (general intelligence, agentic work). Projects always have an agent bar.
pub fn bar(kind: &str, level: &str, project: bool) -> (u8, u8) {
    let (iq, agent): (u8, u8) = match (level, kind) {
        ("easy", "code") => (24, 15),
        ("easy", _) => (24, 0),
        ("medium", "code") => (24, 32),
        // A wrong number reads as right: math gets the hard bar a level early.
        ("medium", "math") => (46, 0),
        ("medium", "chat") => (31, 0),
        ("medium", _) => (38, 0),
        ("hard", "code") => (46, 39),
        ("frontier", "code") => (56, 46),
        ("frontier", _) => (56, 0),
        ("expert", "code") => (50, 42),
        ("expert", _) => (50, 0),
        ("summit", _) => (60, 50),
        _ => (46, 0),
    };
    (
        iq,
        if project {
            agent.max(match level {
                "easy" => 15,
                "medium" => 32,
                "hard" => 39,
                "expert" => 42,
                "summit" => 50,
                _ => 46,
            })
        } else {
            agent
        },
    )
}

const LADDER: [&str; 6] = ["easy", "medium", "hard", "expert", "frontier", "summit"];

/// The bar of the nearest higher level whose bar is different; a task already at the top keeps its own.
fn next_bar(kind: &str, level: &str, project: bool) -> (u8, u8) {
    let own = bar(kind, level, project);
    let at = LADDER.iter().position(|l| *l == level).unwrap_or(2);
    LADDER[at + 1..].iter().map(|l| bar(kind, l, project)).find(|b| *b != own).unwrap_or(own)
}

/// Seconds per task a step up may take: deep quality work gets room for the strongest measured effort.
fn cap_secs(kind: &str, level: &str) -> f32 {
    if (kind == "code" && matches!(level, "hard" | "expert" | "frontier" | "summit"))
        || (kind == "math" && level != "easy")
        || (matters(kind, level) && matches!(level, "expert" | "frontier" | "summit"))
    {
        1200.0
    } else {
        600.0
    }
}

/// Spare quota only buys quality where quality matters.
fn matters(kind: &str, level: &str) -> bool {
    matches!(kind, "code" | "math" | "research") && matches!(level, "medium" | "hard" | "expert" | "frontier" | "summit")
}

/// The level a task's scope gives it. The makers keep the smaller, faster models for well-defined work and start the
/// rest on the everyday model, so open medium work gets the hard bar and open hard work the expert one. Easy work stays
/// easy, and chat keeps its level: a reply a person waits on stays fast. A person who asked outright for the best gets the
/// top rung whatever the work (Erick, 2026-11-12): the strongest model the plan and the quota allow.
pub fn scoped(kind: &str, difficulty: &'static str, scope: &str) -> &'static str {
    match (difficulty, scope) {
        (_, BEST) => "summit",
        ("hard", "open") => "expert",
        ("medium", "open") if kind != "chat" => "hard",
        _ => difficulty,
    }
}

/// An unsure verdict gets the next level's bar, and so does a `blind` one where quality matters: self-checking pays most
/// where the person cannot verify the answer quickly, as with math's early hard bar (`bar`). The two are one step, never
/// two, and a `machine` check lowers nothing: this is raise-only.
fn level_of(kind: &str, difficulty: &'static str, check: &str, scope: &str, confidence: f64) -> &'static str {
    let level = scoped(kind, difficulty, scope);
    let step = confidence < 0.5 || (check == "blind" && matters(kind, level));
    let at = LADDER.iter().position(|l| *l == level).unwrap_or(2);
    LADDER[if step { (at + 1).min(LADDER.len() - 1) } else { at }]
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
    /// What reading the request costs on this profile before it can think, in `Profile::cost` units (`read_cost`). Added
    /// to the cost before the weight and the quota multiply it, so a long read goes to the model cheapest per token among
    /// those that clear the bar.
    pub read: f64,
}

impl Factor {
    pub const NEUTRAL: Factor = Factor { m: 1.0, w: 1.0, step_ok: true, read: 0.0 };
}

/// What an answer must read before it can think: the new message with its files and images, and the conversation its
/// agent already holds. Per-task prices (`Profile::cost`) assume a typical read; a message with a long document, or a long
/// conversation moved to another model, reads far more, and there the price per token decides.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reading {
    /// Tokens no model has cached yet.
    pub fresh: u64,
    /// Tokens of the conversation so far: cached on `on`, the model answering it, and read fresh by any other.
    pub held: u64,
    pub on: Option<&'static str>,
}

// ponytail: estimates, not counts: four characters or bytes a token, and a typical picture. Good enough to tell a page
// from a book; the CLI's own count replaces them once the answer runs.
const CHARS_PER_TOKEN: u64 = 4;
const IMAGE_TOKENS: u64 = 1_600;

impl Reading {
    /// `chars` of request text, `images`, `file_bytes` of attached files, and the `held` context of the conversation's
    /// model `on` (any model, in the table or not).
    pub fn of(chars: usize, images: usize, file_bytes: usize, held: u64, on: Option<&str>) -> Reading {
        let fresh = (chars + file_bytes) as u64 / CHARS_PER_TOKEN + images as u64 * IMAGE_TOKENS;
        let on = on.and_then(|m| PROFILES.iter().find(|p| p.model == m)).map(|p| p.model);
        Reading { fresh, held: if on.is_some() { held } else { 0 }, on }
    }
}

/// What `reading` costs on a profile, in US$, at its API price per token (`usage::api_price_for_prompt`,
/// with its long-prompt tiers): what its own model holds is read from cache, the rest is written to it. A model with no
/// known price reads for free, as before.
pub fn read_cost(p: &Profile, reading: &Reading) -> f64 {
    let (fresh, cached) = if reading.on == Some(p.model) { (reading.fresh, reading.held) } else { (reading.fresh + reading.held, 0) };
    let prompt = fresh + cached;
    // Claude Code writes its cache for an hour; Codex prices a write as plain input.
    let hour = if p.vendor == Vendor::Claude { fresh } else { 0 };
    crate::usage::api_price_for_prompt(p.model, prompt, 0, cached, fresh, hour, prompt).unwrap_or(0.0)
}

fn clears(p: &Profile, bar: (u8, u8)) -> bool {
    p.iq >= bar.0 && p.agent >= bar.1
}

const QUALITY_PRICE_PREMIUM: f64 = 1.15;
const CHAT_SECS: f32 = 300.0;

/// The cheapest sufficient row, or a dominating row within 15% of its factored price and the task's time cap.
fn cheapest(
    kind: &str,
    level: &str,
    candidates: &[Profile],
    factor: &dyn Fn(&Profile) -> Factor,
    steps: bool,
    project: bool,
) -> Option<(Profile, f64, bool)> {
    let (own, next, cap) = (bar(kind, level, project), next_bar(kind, level, project), cap_secs(kind, level));
    let matters = matters(kind, level);
    let fast_chat =
        kind == "chat" && matches!(level, "easy" | "medium") && candidates.iter().any(|p| clears(p, own) && p.secs <= CHAT_SECS);
    let priced = candidates.iter().filter(|p| clears(p, own) && (!fast_chat || p.secs <= CHAT_SECS)).map(|p| {
        let f = factor(p);
        let f_m = if matters { f.m } else { f.m.max(1.0) };
        let step = steps && matters && next != own && clears(p, next) && p.secs <= cap && f.step_ok;
        let price = (p.cost as f64 + f.read) * f.w * f_m * if step { f_m.min(1.0) } else { 1.0 };
        (*p, price, step)
    });
    let cheapest = priced.clone().min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.secs.total_cmp(&b.0.secs)))?;
    Some(
        priced
            .filter(|(p, price, _)| {
                p.iq >= cheapest.0.iq
                    && p.agent >= cheapest.0.agent
                    && (p.iq > cheapest.0.iq || p.agent > cheapest.0.agent)
                    && p.secs <= cap
                    && *price <= cheapest.1 * QUALITY_PRICE_PREMIUM
            })
            .min_by(|a, b| (b.0.iq, b.0.agent).cmp(&(a.0.iq, a.0.agent)).then(a.1.total_cmp(&b.1)).then(a.0.secs.total_cmp(&b.0.secs)))
            .unwrap_or(cheapest),
    )
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
    choose_scoped(kind, difficulty, check, "tight", confidence, false, candidates)
}

pub fn choose_scoped(
    kind: &'static str,
    difficulty: &'static str,
    check: &'static str,
    scope: &'static str,
    confidence: f64,
    project: bool,
    candidates: &[Profile],
) -> Option<Routed> {
    choose_reading(kind, difficulty, check, scope, confidence, project, candidates, &|_| 0.0)
}

/// Choose an effort within the current model, keeping uncertain or blind reductions at the previous effort.
///
/// Open medium work would have started on the everyday model (`scoped`). A smaller model already held does it at high at
/// most: past high one more effort costs as much as a bigger model, so the unmet bar offers that model instead
/// (`Routed::clears_bar`, `nudge`). Work that reached the top effort before this rule still does.
pub fn choose_effort(verdict: &Asked, project: bool, current: &Profile) -> Option<Routed> {
    let profiles: Vec<_> = PROFILES.iter().filter(|p| p.vendor == current.vendor && p.model == current.model).copied().collect();
    let pick =
        |scope, rows: &[Profile]| choose_scoped(verdict.kind, verdict.difficulty, verdict.check, scope, verdict.confidence, project, rows);
    let mut chosen = pick(verdict.scope, &profiles)?;
    if chosen.profile.effort == "xhigh"
        && scoped(verdict.kind, verdict.difficulty, verdict.scope) == "hard"
        && verdict.difficulty == "medium"
        && pick("tight", &profiles).is_some_and(|tight| tight.profile.effort != "xhigh")
    {
        let below: Vec<_> = profiles.iter().filter(|p| p.effort != "xhigh").copied().collect();
        if let Some(capped) = pick(verdict.scope, &below) {
            (chosen.profile, chosen.price) = (capped.profile, capped.price);
        }
    }
    if chosen.profile.cost < current.cost && (verdict.confidence < 0.7 || verdict.check == "blind") {
        chosen.profile = *current;
        chosen.price = current.cost as f64;
    }
    Some(chosen)
}

/// `choose` with what reading the request costs on each profile (`Factor::read`), and no quota.
#[allow(clippy::too_many_arguments)]
fn choose_reading(
    kind: &'static str,
    difficulty: &'static str,
    check: &'static str,
    scope: &'static str,
    confidence: f64,
    project: bool,
    candidates: &[Profile],
    read: &dyn Fn(&Profile) -> f64,
) -> Option<Routed> {
    let level = level_of(kind, difficulty, check, scope, confidence);
    let (profile, price) = match cheapest(kind, level, candidates, &|p| Factor { read: read(p), ..Factor::NEUTRAL }, false, project) {
        Some((p, price, _)) => (p, price),
        None => {
            let p = strongest(candidates)?;
            (p, p.cost as f64 + read(&p))
        }
    };
    Some(Routed { profile, kind, difficulty, check, scope, project, confidence, price, pace: None, wanted_effort: None })
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
    choose_paced_scoped(kind, difficulty, check, "tight", confidence, false, candidates, factor)
}

#[allow(clippy::too_many_arguments)]
pub fn choose_paced_scoped(
    kind: &'static str,
    difficulty: &'static str,
    check: &'static str,
    scope: &'static str,
    confidence: f64,
    project: bool,
    candidates: &[Profile],
    factor: &dyn Fn(&Profile) -> Factor,
) -> Option<Routed> {
    let level = level_of(kind, difficulty, check, scope, confidence);
    let usual = choose_reading(kind, difficulty, check, scope, confidence, project, candidates, &|p| factor(p).read)?;
    let Some((profile, price, step)) = cheapest(kind, level, candidates, factor, true, project) else {
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
    Some(Routed { profile, kind, difficulty, check, scope, project, confidence, price, pace, wanted_effort: None })
}

/// What the person's own redoing adds to the table's pick: for a kind of task they keep redoing (`auto_learned`), one rung up.
/// The rung is the next level's bar (`LADDER`), bought only where a plan limit, a vendor's pace and the time cap of the step up
/// already allow it, never from a vendor running fast (`LOW_M`: four times its daily line or more), and only from `candidates`, which are what the
/// account can run and the gate admits. When no such candidate exists, or the quota already spoke (`pace` is a `step`, `fast` or `low`),
/// the table's pick stands: the total is one rung over the table, never two, and a shown change always says it is learned.
pub fn lifted(base: Routed, candidates: &[Profile], factor: &dyn Fn(&Profile) -> Factor) -> Routed {
    if base.pace.is_some() {
        return base;
    }
    let level = level_of(base.kind, base.difficulty, base.check, base.scope, base.confidence);
    let Some(up) = LADDER.iter().position(|l| *l == level).and_then(|at| LADDER.get(at + 1)) else { return base };
    let (cap, here) = (cap_secs(base.kind, level), (base.profile.iq, base.profile.agent));
    let room: Vec<Profile> = candidates
        .iter()
        .filter(|p| {
            let f = factor(p);
            (p.iq, p.agent) > here && p.secs <= cap && f.step_ok && f.m < LOW_M
        })
        .copied()
        .collect();
    let Some((profile, price, _)) = cheapest(base.kind, up, &room, factor, false, base.project) else { return base };
    Routed { profile, price, pace: Some(PaceNote { kind: "learned".into(), vendor: profile.vendor }), ..base }
}

/// Quota is a brake, never a silent downgrade (Erick, 2026-11-12): when the quota gate left out every model this account
/// has that clears the task's bar, the pick is the strongest one left and the answer says which subscription is near its
/// limit (`limit`). `account` is what the account can run before the quota gate. A note the quota or learning already
/// wrote stands.
pub fn limit_note(routed: &Routed, account: &[Profile]) -> Option<PaceNote> {
    if routed.pace.is_some() || routed.clears_bar() {
        return None;
    }
    let need = bar(routed.kind, level_of(routed.kind, routed.difficulty, routed.check, routed.scope, routed.confidence), routed.project);
    let held = account.iter().filter(|p| clears(p, need)).min_by(|a, b| a.cost.total_cmp(&b.cost))?;
    Some(PaceNote { kind: "limit".into(), vendor: held.vendor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fifteen_percent_rule_changes_claude_expert_and_codex_medium_chat_in_the_210_pinned_picks() {
        let models = ["gpt-6.1-sol", "gpt-6.1-terra", "gpt-6-luna"].map(String::from);
        let mut changed = 0;
        let mut unchanged = 0;
        for vendors in [&[Vendor::Claude][..], &[Vendor::Codex][..], &[Vendor::Claude, Vendor::Codex][..]] {
            let pool = candidates(vendors, &models);
            for kind in KINDS {
                for level in &LADDER[..5] {
                    for project in [false, true] {
                        let floor = bar(kind, level, project);
                        let fast = kind == "chat"
                            && matches!(*level, "easy" | "medium")
                            && pool.iter().any(|p| clears(p, floor) && p.secs <= CHAT_SECS);
                        let plain = pool
                            .iter()
                            .filter(|p| clears(p, floor) && (!fast || p.secs <= CHAT_SECS))
                            .min_by(|a, b| a.cost.total_cmp(&b.cost).then(a.secs.total_cmp(&b.secs)))
                            .copied()
                            .or_else(|| strongest(&pool))
                            .unwrap();
                        let picked = choose_scoped(kind, level, "glance", "tight", 0.9, project, &pool).unwrap().profile;
                        if picked != plain {
                            changed += 1;
                            if vendors == &[Vendor::Claude][..] {
                                assert_eq!(*level, "expert");
                                assert_eq!((plain.model, plain.effort), ("claude-opus-5-5", "high"));
                                assert_eq!((picked.model, picked.effort), ("claude-fable-5-2", "medium"));
                            } else {
                                assert_eq!((kind, *level, project), ("chat", "medium", false));
                                assert_eq!((plain.model, plain.effort), ("gpt-6.1-sol", "low"));
                                assert_eq!((picked.model, picked.effort), ("gpt-6.1-terra", "medium"));
                            }
                        } else {
                            unchanged += 1;
                        }
                    }
                }
            }
        }
        assert_eq!((changed, unchanged), (16, 194));
    }

    #[test]
    fn the_quality_premium_uses_quota_weights_and_read_cost_across_vendors() {
        let pool = candidates(&[Vendor::Codex, Vendor::Claude], &["gpt-6.1-terra".into(), "gpt-6.1-sol".into()]);
        for sol in [Factor { m: 0.5, step_ok: false, ..Factor::NEUTRAL }, Factor { w: 0.5, step_ok: false, ..Factor::NEUTRAL }] {
            let factor = |p: &Profile| if p.model == "gpt-6.1-sol" { sol } else { Factor::NEUTRAL };
            let (picked, price, _) = cheapest("code", "medium", &pool, &factor, false, true).unwrap();
            assert_eq!((picked.model, picked.effort), ("gpt-6.1-sol", "medium"));
            assert!(price > 0.169 && price <= 0.169 * QUALITY_PRICE_PREMIUM);
        }
        // Opus medium is about 9% above Sol medium after quota, then the premium crosses vendors.
        let factor = |p: &Profile| Factor { m: if p.vendor == Vendor::Claude { 0.16 } else { 1.0 }, step_ok: false, ..Factor::NEUTRAL };
        let (picked, price, _) = cheapest("code", "hard", &pool, &factor, false, false).unwrap();
        assert_eq!((picked.model, picked.effort), ("claude-opus-5-5", "medium"));
        assert!(price > 0.359 && price <= 0.359 * QUALITY_PRICE_PREMIUM);
        let read = |p: &Profile| Factor { read: if p.model == "gpt-6.1-terra" { 0.17 } else { 0.0 }, ..Factor::NEUTRAL };
        assert_eq!(cheapest("code", "medium", &pool, &read, false, true).unwrap().0.model, "gpt-6.1-sol");
    }

    #[test]
    fn the_quality_premium_requires_both_scores_the_price_bound_and_the_time_cap_then_breaks_ties() {
        let base = PROFILES.iter().find(|p| p.model == "gpt-6.1-sol" && p.effort == "medium").copied().unwrap();
        let better = Profile { iq: 48, agent: 41, cost: base.cost * 1.1, ..base };
        let pick = |other| cheapest("code", "hard", &[base, other], &|_| Factor::NEUTRAL, false, false).unwrap().0;
        assert_eq!(pick(better), better);
        for worse in [
            Profile { agent: base.agent - 1, ..better },
            Profile { iq: base.iq - 1, ..better },
            Profile { cost: base.cost * 1.151, ..better },
            Profile { secs: 1201.0, ..better },
        ] {
            assert_eq!(pick(worse), base);
        }
        let faster = Profile { secs: better.secs - 1.0, ..better };
        let cheaper = Profile { cost: better.cost - 0.001, ..better };
        let strongest = Profile { iq: 50, agent: 43, cost: base.cost * 1.14, ..base };
        assert_eq!(cheapest("code", "hard", &[base, better, strongest], &|_| Factor::NEUTRAL, false, false).unwrap().0, strongest);
        assert_eq!(cheapest("code", "hard", &[base, better, faster, cheaper], &|_| Factor::NEUTRAL, false, false).unwrap().0, cheaper);
        assert_eq!(cheapest("code", "hard", &[base, better, faster], &|_| Factor::NEUTRAL, false, false).unwrap().0, faster);
        assert_eq!(cheapest("code", "hard", &[base, faster, better], &|_| Factor::NEUTRAL, false, false).unwrap().0, faster);
    }

    #[test]
    fn chat_prefers_rows_within_300_seconds_only_when_one_clears_its_own_bar() {
        let claude = candidates(&[Vendor::Claude], &[]);
        assert_eq!(choose("chat", "medium", "glance", 0.9, &claude).unwrap().profile.effort, "medium");
        let slow: Vec<_> = claude.iter().filter(|p| p.secs > CHAT_SECS).copied().collect();
        assert_eq!(choose("chat", "medium", "glance", 0.9, &slow).unwrap().profile.model, "claude-haiku-5-5");
        for kind in KINDS.into_iter().filter(|k| *k != "chat") {
            let row = choose(kind, "medium", "glance", 0.9, &claude).unwrap().profile;
            assert_eq!((row.model, row.effort), if kind == "math" { ("claude-opus-5-5", "medium") } else { ("claude-sonnet-5-5", "high") });
        }
    }

    #[test]
    fn verdicts_stop_at_frontier_and_only_the_existing_step_paths_reach_summit() {
        for kind in KINDS {
            assert_eq!(next_bar(kind, "frontier", false), (60, 50));
            assert_eq!(next_bar(kind, "summit", true), (60, 50));
            for difficulty in LEVELS {
                for scope in SCOPES {
                    for check in CHECKS {
                        for confidence in [0.2, 0.9] {
                            assert_ne!(level_of(kind, difficulty, check, scope, confidence), "summit");
                        }
                    }
                }
            }
        }
        assert_eq!(level_of("chat", "easy", "glance", BEST, 0.9), "summit", "only an outright ask for the best lands there");
        assert_eq!(cap_secs("research", "hard"), 600.0);
        for level in ["expert", "frontier"] {
            assert_eq!(cap_secs("research", level), 1200.0);
            assert_eq!(cap_secs("writing", level), 600.0);
        }
    }

    /// The 2026-11-11 video request said "use the strongest model and the highest effort" and ran on Sonnet medium.
    #[test]
    fn an_outright_ask_for_the_best_reaches_the_strongest_model_the_account_allows_and_says_when_quota_held_it_back() {
        let line = r#"{"kind":"writing","difficulty":"medium","check":"glance","scope":"tight","best":true,"confidence":0.9}"#;
        let best = asked(line).unwrap();
        assert_eq!(best.scope, BEST);
        assert_eq!(asked(&line.replace("true", "false")).unwrap().scope, "tight");
        assert_eq!(asked(&line.replace(r#""best":true,"#, "")).unwrap().scope, "tight", "an older router line still reads");
        let claude = candidates(&[Vendor::Claude], &[]);
        let both = candidates(&[Vendor::Claude, Vendor::Codex], &["gpt-6.1-sol".into()]);
        let pick = |pool: &[Profile]| {
            let r =
                choose_paced_scoped(best.kind, best.difficulty, best.check, best.scope, best.confidence, false, pool, &|_| Factor::NEUTRAL)
                    .unwrap();
            (r.profile.model, r.profile.effort, limit_note(&r, &claude).map(|n| (n.kind, n.vendor)))
        };
        assert_eq!(pick(&claude), ("claude-fable-5-2", "high", None));
        assert_eq!(pick(&both), ("claude-fable-5-2", "high", None), "spare quota elsewhere does not lower an outright ask");
        // The quota gate left Fable out: the strongest model left answers, and the person is told why.
        let no_fable: Vec<_> = claude.iter().filter(|p| p.model != "claude-fable-5-2").copied().collect();
        assert_eq!(pick(&no_fable), ("claude-opus-5-5", "xhigh", Some(("limit".to_string(), Vendor::Claude))));
        // A plan that never had the model is not a quota brake: nothing to say.
        let r = choose_scoped(best.kind, best.difficulty, best.check, best.scope, 0.9, false, &no_fable).unwrap();
        assert_eq!(limit_note(&r, &no_fable), None);
        // Without the ask, everyday writing keeps its everyday model.
        let plain = choose_scoped("writing", "medium", "glance", "tight", 0.9, false, &claude).unwrap().profile;
        assert_eq!((plain.model, plain.effort), ("claude-sonnet-5-5", "high"));
    }
}
