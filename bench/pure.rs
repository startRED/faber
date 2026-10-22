//! The benchmark's pure parts: what a request file says, who is "A" and "B", what the judge answered, what a CLI
//! reported, the spend cap and the report. Nothing here starts a process, reads a file or calls a CLI, so the engine's
//! tests run it (`engine/tests/all.rs`) and the runner (`engine/examples/bench.rs`) includes the same file.
//! How to run the benchmark: docs/dev/benchmark.md.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// --- Requests -------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Work in a folder: Projects on Faber's side, a folder on the plain CLI's.
    Project,
    /// A question: the Chat area on Faber's side, an empty folder on the plain CLI's.
    Chat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    /// `bench/fixtures/<name>.mjs` writes the starting folder; none starts from a README.
    pub fixture: Option<String>,
    /// `bench/proofs/<name>.mjs` checks the result; none leaves the verdict to the judge alone.
    pub proof: Option<String>,
    pub prompt: String,
}

/// A request file: `key: value` lines between two `---` lines, then the prompt exactly as the person would type it.
pub fn parse_request(text: &str) -> Result<Request, String> {
    let text = text.replace("\r\n", "\n");
    let rest = text.strip_prefix("---\n").ok_or("a request file starts with a --- line")?;
    let (head, prompt) = rest.split_once("\n---\n").ok_or("the header needs a closing --- line")?;
    let (mut id, mut title, mut kind, mut fixture, mut proof) = (None, None, None, None, None);
    for line in head.lines().filter(|l| !l.trim().is_empty()) {
        let (key, value) = line.split_once(':').ok_or_else(|| format!("not a `key: value` line: {line}"))?;
        let value = value.trim().to_string();
        match key.trim() {
            "id" => id = Some(value),
            "title" => title = Some(value),
            "kind" => {
                kind = Some(match value.as_str() {
                    "project" => Kind::Project,
                    "chat" => Kind::Chat,
                    other => return Err(format!("kind is project or chat, not {other}")),
                })
            }
            "fixture" => fixture = Some(value).filter(|v| v != "none"),
            "proof" => proof = Some(value).filter(|v| v != "none"),
            other => return Err(format!("unknown header key {other}")),
        }
    }
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("the request has no prompt".into());
    }
    let id = id.ok_or("the header needs an id")?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(format!("id {id} is not a lowercase word with dashes"));
    }
    Ok(Request { title: title.unwrap_or_else(|| id.clone()), id, kind: kind.ok_or("the header needs a kind")?, fixture, proof, prompt })
}

// --- Blind pairs ----------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Faber,
    Plain,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Faber => Side::Plain,
            Side::Plain => Side::Faber,
        }
    }
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// Who is shown as "A": the run picks the first request's side by its id, then the sides alternate, so a run never shows
/// one side first in every request (a judge leans toward the first answer) and the same run id always deals the same way.
pub fn a_side(run_id: &str, index: usize) -> Side {
    let first_is_faber = fnv(run_id).is_multiple_of(2);
    if first_is_faber == index.is_multiple_of(2) { Side::Faber } else { Side::Plain }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Pick {
    A,
    B,
    Tie,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verdict {
    pub pick: Pick,
    pub score_a: Option<f64>,
    pub score_b: Option<f64>,
    pub reason: String,
}

/// The judge's last JSON object: `{"winner":"A"|"B"|"tie","a":7,"b":5,"reason":"..."}`. Text before it is ignored.
pub fn parse_verdict(text: &str) -> Option<Verdict> {
    let mut end = text.len();
    while let Some(close) = text[..end].rfind('}') {
        let mut start = close;
        while let Some(open) = text[..start].rfind('{') {
            if let Ok(v) = serde_json::from_str::<Value>(&text[open..=close])
                && let Some(winner) = v.get("winner").and_then(Value::as_str)
            {
                let pick = match winner.trim().to_ascii_lowercase().as_str() {
                    "a" => Pick::A,
                    "b" => Pick::B,
                    "tie" => Pick::Tie,
                    _ => return None,
                };
                return Some(Verdict {
                    pick,
                    score_a: v.get("a").and_then(Value::as_f64),
                    score_b: v.get("b").and_then(Value::as_f64),
                    reason: v.get("reason").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
                });
            }
            start = open;
        }
        end = close;
    }
    None
}

/// The answer revealed after the vote: the side that won, none for a tie.
pub fn reveal(pick: Pick, a: Side) -> Option<Side> {
    match pick {
        Pick::A => Some(a),
        Pick::B => Some(a.other()),
        Pick::Tie => None,
    }
}

/// Text that names Faber would tell the judge which side it reads; the name goes before the judge sees the answer.
pub fn scrub(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find("faber") {
        out.push_str(&text[at..at + found]);
        out.push_str("the app");
        at += found + 5;
    }
    out.push_str(&text[at..]);
    out
}

const FILE_BYTES: usize = 6_000;
const FILES_BYTES: usize = 30_000;
const REPLY_CHARS: usize = 12_000;

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars).collect();
    cut.push_str("\n[cut: the rest is not shown]");
    cut
}

/// One side's work as the judge reads it: the final reply, then the folder (every path, and the text of the files that
/// fit the caps; `None` is a file that is not text), then the automatic check.
pub fn render_entry(reply: &str, files: &[(String, Option<String>)], proof: Option<&Proof>) -> String {
    let mut out = format!("### Final reply\n{}\n", clip(&scrub(reply), REPLY_CHARS));
    if !files.is_empty() {
        out.push_str("\n### Files in the folder\n");
        for (path, _) in files {
            out.push_str(&format!("- {}\n", scrub(path)));
        }
        let mut left = FILES_BYTES;
        for (path, content) in files {
            let Some(content) = content.as_deref().filter(|c| !c.is_empty()) else { continue };
            if left == 0 {
                out.push_str("\n[more files exist than are shown]\n");
                break;
            }
            let take = content.len().min(FILE_BYTES).min(left);
            let mut cut = take;
            while !content.is_char_boundary(cut) {
                cut -= 1;
            }
            out.push_str(&format!("\n#### {}\n```\n{}", scrub(path), scrub(&content[..cut])));
            if cut < content.len() {
                out.push_str("\n[cut]");
            }
            out.push_str("\n```\n");
            left -= cut;
        }
    }
    if let Some(proof) = proof {
        out.push_str(&format!("\n### Automatic check: {}\n", if proof.pass { "passed" } else { "FAILED" }));
        for check in &proof.checks {
            out.push_str(&format!("- {}: {}{}\n", if check.ok { "ok" } else { "FAILED" }, check.name, detail(&check.detail)));
        }
    }
    out
}

fn detail(text: &str) -> String {
    if text.is_empty() { String::new() } else { format!(" ({})", clip(text, 300)) }
}

pub const JUDGE_INSTRUCTIONS: &str = "You judge two answers, A and B, to the same request from a person. You do not know who or what wrote \
either, and names that appear inside an answer say nothing about it. Judge what the person receives: does it do what was asked, is it \
correct, complete and clear, without padding or invented facts. When work was done in a folder, the files matter more than the words \
about them. An automatic check is a fact: a failed check is a real defect, and a passed one does not make the work good. A longer answer \
is not a better one. Think briefly, then end with one line of JSON and nothing after it: \
{\"winner\":\"A\" or \"B\" or \"tie\",\"a\":<1-10>,\"b\":<1-10>,\"reason\":\"one or two plain sentences\"}. Say tie only when you \
cannot tell them apart.";

pub fn judge_prompt(request: &str, a: &str, b: &str) -> String {
    format!("## The request\n{}\n\n## Answer A\n{a}\n\n## Answer B\n{b}\n", request.trim())
}

// --- What a CLI reported --------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    /// What the model read or wrote for the first time; cache reads are the cheap, repeated part.
    pub fn fresh(&self) -> u64 {
        self.input + self.output + self.cache_write
    }
    pub fn add(&mut self, other: &Usage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Outcome {
    pub text: String,
    pub ok: bool,
    pub error: Option<String>,
    pub usage: Usage,
    /// API-equivalent dollars as the CLI reports them; none when it reports none (Codex).
    pub cost_usd: Option<f64>,
    pub models: Vec<String>,
    pub turns: Option<u64>,
}

fn number(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// `claude -p --output-format json`: one result object, or a list of events that ends with it.
pub fn parse_claude_result(stdout: &str) -> Result<Outcome, String> {
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|e| format!("the CLI's result is not JSON: {e}"))?;
    let result = match &value {
        Value::Array(events) => events.iter().rev().find(|e| e.get("type").and_then(Value::as_str) == Some("result")),
        other => Some(other),
    }
    .ok_or("the CLI's result has no result object")?;
    let usage = result.get("usage").cloned().unwrap_or(Value::Null);
    let is_error = result.get("is_error").and_then(Value::as_bool).unwrap_or(false);
    let subtype = result.get("subtype").and_then(Value::as_str).unwrap_or_default();
    Ok(Outcome {
        text: result.get("result").and_then(Value::as_str).unwrap_or_default().to_string(),
        ok: !is_error && (subtype.is_empty() || subtype == "success"),
        error: (is_error || (!subtype.is_empty() && subtype != "success")).then(|| format!("is_error {is_error}, subtype {subtype}")),
        usage: Usage {
            input: number(&usage, "input_tokens"),
            output: number(&usage, "output_tokens"),
            cache_read: number(&usage, "cache_read_input_tokens"),
            cache_write: number(&usage, "cache_creation_input_tokens"),
        },
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64),
        models: result.get("modelUsage").and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default(),
        turns: result.get("num_turns").and_then(Value::as_u64),
    })
}

/// `codex exec --json`: one event per line; the last agent message is the reply and each finished turn adds its usage.
/// Codex reports no price, so there is no cost. Written from Codex's documented event names and not yet run against a
/// live account (its quota was out on 2026-10-22), so a first real run must check it.
pub fn parse_codex_events(stdout: &str) -> Result<Outcome, String> {
    let mut out = Outcome::default();
    let (mut seen, mut failed) = (false, None);
    for line in stdout.lines().filter(|l| l.trim_start().starts_with('{')) {
        let Ok(event) = serde_json::from_str::<Value>(line) else { continue };
        seen = true;
        match event.get("type").and_then(Value::as_str) {
            Some("item.completed") => {
                let item = &event["item"];
                if item.get("type").and_then(Value::as_str) == Some("agent_message")
                    && let Some(text) = item.get("text").and_then(Value::as_str)
                {
                    out.text = text.to_string();
                }
            }
            Some("turn.completed") => {
                let u = &event["usage"];
                // Codex's input count includes what it read from the cache.
                let cached = number(u, "cached_input_tokens");
                out.usage.add(&Usage {
                    input: number(u, "input_tokens").saturating_sub(cached),
                    output: number(u, "output_tokens"),
                    cache_read: cached,
                    cache_write: 0,
                });
                out.turns = Some(out.turns.unwrap_or(0) + 1);
            }
            Some("turn.failed" | "error") => failed = Some(event.to_string()),
            _ => {}
        }
    }
    if !seen {
        return Err("Codex printed no events".into());
    }
    out.ok = failed.is_none() && !out.text.is_empty();
    out.error = failed.or_else(|| out.text.is_empty().then(|| "Codex gave no reply".into()));
    Ok(out)
}

/// The reply of a Faber answer from its UTF-16 offset (`Message.activity_end`): the part after the working notes.
pub fn from_utf16(text: &str, offset: usize) -> &str {
    let mut units = 0;
    for (at, c) in text.char_indices() {
        if units >= offset {
            return &text[at..];
        }
        units += c.len_utf16();
    }
    ""
}

// --- Proofs ---------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proof {
    pub pass: bool,
    pub checks: Vec<Check>,
}

/// A proof script prints its result as the last JSON line: `{"pass":true,"checks":[{"name":"..","ok":true,"detail":".."}]}`.
/// `pass` is true only when every check is, whatever the script claims.
pub fn parse_proof(stdout: &str) -> Option<Proof> {
    let v: Value = stdout.lines().rev().filter(|l| l.trim_start().starts_with('{')).find_map(|l| serde_json::from_str(l).ok())?;
    let checks: Vec<Check> = v
        .get("checks")?
        .as_array()?
        .iter()
        .map(|c| Check {
            name: c.get("name").and_then(Value::as_str).unwrap_or("check").to_string(),
            ok: c.get("ok").and_then(Value::as_bool).unwrap_or(false),
            detail: c.get("detail").and_then(Value::as_str).unwrap_or_default().to_string(),
        })
        .collect();
    (!checks.is_empty()).then(|| Proof { pass: checks.iter().all(|c| c.ok), checks })
}

// --- The cap --------------------------------------------------------------------------------------------------------

/// What a run may spend before it stops: API-equivalent dollars as the CLIs report them, and first-time tokens
/// (input, output, cache writes; cache reads are not counted).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cap {
    pub max_usd: f64,
    pub max_fresh_tokens: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Spend {
    pub usd: f64,
    pub usage: Usage,
}

impl Spend {
    pub fn add(&mut self, usage: &Usage, usd: Option<f64>) {
        self.usd += usd.unwrap_or(0.0);
        self.usage.add(usage);
    }
}

impl Cap {
    /// Why the run must stop now, or none while it is inside the cap.
    pub fn reached(&self, spent: &Spend) -> Option<String> {
        if spent.usd >= self.max_usd {
            Some(format!("US${:.2} spent, cap US${:.2}", spent.usd, self.max_usd))
        } else if spent.usage.fresh() >= self.max_fresh_tokens {
            Some(format!("{} first-time tokens spent, cap {}", spent.usage.fresh(), self.max_fresh_tokens))
        } else {
            None
        }
    }
    /// The dollars one more run may still spend, at most `per_run`.
    pub fn allowance(&self, spent: &Spend, per_run: f64) -> f64 {
        (self.max_usd - spent.usd).clamp(0.0, per_run)
    }
}

// --- The report -----------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SideResult {
    pub side: Side,
    pub ok: bool,
    pub error: Option<String>,
    /// The reply's own time: the plain CLI's run, or the reply's `took` in Faber (from the message sent to the end of the
    /// answer, the router and a project's copy included).
    pub secs: f64,
    /// From starting the side to its end: for Faber also the engine's start, its plan check and the wait for a project's checks.
    pub wall_secs: f64,
    pub usage: Usage,
    pub cost_usd: Option<f64>,
    pub models: Vec<String>,
    pub reply: String,
    pub folder: Option<String>,
    pub proof: Option<Proof>,
    pub stopped_by_cap: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub request: String,
    pub title: String,
    pub kind: Kind,
    pub a: Side,
    pub verdict: Option<Verdict>,
    pub winner: Option<Side>,
    pub faber: Option<SideResult>,
    pub plain: Option<SideResult>,
    pub judge_cost_usd: Option<f64>,
    pub judge_usage: Usage,
    pub skipped: Option<String>,
}

fn proof_word(side: &Option<SideResult>) -> String {
    match side.as_ref().and_then(|s| s.proof.as_ref()) {
        None => "none".into(),
        Some(p) => format!("{} ({}/{})", if p.pass { "pass" } else { "FAIL" }, p.checks.iter().filter(|c| c.ok).count(), p.checks.len()),
    }
}

fn tokens_word(side: &Option<SideResult>) -> String {
    side.as_ref().map_or("-".into(), |s| format!("{} ({} cached)", s.usage.fresh(), s.usage.cache_read))
}

fn usd_word(usd: Option<f64>) -> String {
    usd.map_or("n/a".into(), |u| format!("{u:.3}"))
}

fn secs_word(side: &Option<SideResult>) -> String {
    side.as_ref().map_or("-".into(), |s| format!("{:.0}", s.secs))
}

fn winner_word(row: &Row) -> &'static str {
    match (&row.verdict, row.winner) {
        (None, _) => "no verdict",
        (Some(_), Some(Side::Faber)) => "Faber",
        (Some(_), Some(Side::Plain)) => "plain",
        (Some(_), None) => "tie",
    }
}

/// The report's table and totals, in Markdown. The writer adds the words around it.
pub fn render_tables(rows: &[Row], spent: &Spend, cap: &Cap) -> String {
    let mut out = String::from(
        "| Request | Kind | Judge (blind) | Faber proof | Plain proof | Faber s | Plain s | Faber tokens | Plain tokens | Faber US$ | Plain US$ |\n\
         | :--- | :--- | :--- | :--- | :--- | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for r in rows {
        out.push_str(&format!(
            "| {} | {:?} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.request,
            r.kind,
            winner_word(r),
            proof_word(&r.faber),
            proof_word(&r.plain),
            secs_word(&r.faber),
            secs_word(&r.plain),
            tokens_word(&r.faber),
            tokens_word(&r.plain),
            usd_word(r.faber.as_ref().and_then(|s| s.cost_usd)),
            usd_word(r.plain.as_ref().and_then(|s| s.cost_usd)),
        ));
    }
    let side_total = |pick: fn(&Row) -> &Option<SideResult>| {
        let mut usage = Usage::default();
        let (mut usd, mut secs) = (0.0, 0.0);
        for r in rows {
            if let Some(s) = pick(r) {
                usage.add(&s.usage);
                usd += s.cost_usd.unwrap_or(0.0);
                secs += s.secs;
            }
        }
        (usage, usd, secs)
    };
    let (fu, fusd, fs) = side_total(|r| &r.faber);
    let (pu, pusd, ps) = side_total(|r| &r.plain);
    let wins = |side| rows.iter().filter(|r| r.verdict.is_some() && r.winner == Some(side)).count();
    let ties = rows.iter().filter(|r| r.verdict.is_some() && r.winner.is_none()).count();
    let judge_usd: f64 = rows.iter().filter_map(|r| r.judge_cost_usd).sum();
    out.push_str(&format!(
        "\n| Totals | Faber | Plain |\n| :--- | ---: | ---: |\n| Judge wins (ties {ties}) | {} | {} |\n| Time (s) | {fs:.0} | {ps:.0} |\n\
         | First-time tokens (input, output, cache writes) | {} | {} |\n| Cache-read tokens | {} | {} |\n| API-equivalent US$ | {fusd:.3} | {pusd:.3} |\n",
        wins(Side::Faber),
        wins(Side::Plain),
        fu.fresh(),
        pu.fresh(),
        fu.cache_read,
        pu.cache_read,
    ));
    if pusd > 0.0 {
        out.push_str(&format!("| Faber cost as a share of plain | {:.0}% | 100% |\n", fusd * 100.0 / pusd));
    }
    out.push_str(&format!(
        "\nSpend of the whole run: US${:.3} for the sides, US${judge_usd:.3} for the judge; cap US${:.2} or {} first-time tokens ({} used).\n",
        spent.usd - judge_usd,
        cap.max_usd,
        cap.max_fresh_tokens,
        spent.usage.fresh()
    ));
    for r in rows.iter().filter_map(|r| r.skipped.as_ref().map(|why| (r, why))) {
        out.push_str(&format!("\nNot run: {} ({}).\n", r.0.request, r.1));
    }
    out
}

// --- Card pairs: the token gate of the start card ---------------------------------------------------------------------
// Own work method, "Cost" (docs/superpowers/specs/2026-10-22-own-work-method.md); how to run: docs/dev/benchmark.md.

/// A paired run is Faber with the start card on or off; everything else in the two is the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    Card,
    NoCard,
}

impl Arm {
    pub fn name(self) -> &'static str {
        match self {
            Arm::Card => "card",
            Arm::NoCard => "no card",
        }
    }
    pub fn other(self) -> Arm {
        match self {
            Arm::Card => Arm::NoCard,
            Arm::NoCard => Arm::Card,
        }
    }
}

/// Which arm of pair `pair` runs first: they alternate, so the prompt cache the first arm warms helps each arm equally often.
/// It does not depend on the run, so a run that adds pairs (`--pair-start`) continues the alternation.
pub fn first_arm(pair: usize) -> Arm {
    if pair.is_multiple_of(2) { Arm::Card } else { Arm::NoCard }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Condition {
    /// The first conversation of a new engine, data folder and project.
    Cold,
    /// A second new conversation in the same project, right after the first.
    Warm,
    /// A follow-up message in the first conversation, after the engine (and its CLI processes) was closed and started again.
    Resumed,
}

impl Condition {
    pub fn name(self) -> &'static str {
        match self {
            Condition::Cold => "cold",
            Condition::Warm => "warm",
            Condition::Resumed => "resumed",
        }
    }
}

/// `cold,warm,resumed`, in any order and without repeats.
pub fn parse_conditions(text: &str) -> Result<Vec<Condition>, String> {
    let mut out = Vec::new();
    for word in text.split(',').map(str::trim).filter(|w| !w.is_empty()) {
        let condition = [Condition::Cold, Condition::Warm, Condition::Resumed]
            .into_iter()
            .find(|c| c.name() == word)
            .ok_or_else(|| format!("a condition is cold, warm or resumed, not {word}"))?;
        if !out.contains(&condition) {
            out.push(condition);
        }
    }
    if out.is_empty() { Err("name at least one condition".into()) } else { Ok(out) }
}

/// One session of one arm: what it cost and whether it succeeded. The gate reads only these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CardRun {
    pub vendor: String,
    pub request: String,
    pub condition: Condition,
    pub pair: usize,
    pub arm: Arm,
    /// The session ended well and its proof passed, or (no proof) the blind judge found the work acceptable.
    pub success: bool,
    /// For the condition's own turns only (the cold turn that led to a warm or resumed one is its own run).
    pub usage: Usage,
    /// Replies of the agent in these turns: more than one means Faber's checks sent it back to fix something.
    pub replies: u64,
    /// Tool uses in those replies. The CLIs do not report their model calls through the engine; this is the nearest count.
    pub tool_calls: u64,
    pub secs: f64,
    pub cost_usd: Option<f64>,
    pub proof: Option<bool>,
    pub accepted: Option<bool>,
    pub note: Option<String>,
}

/// What the gate compares in a run: input (not read from the cache), cache writes (input the model read for the first
/// time and the cache stored) and output. Cache reads are the repeated, cheap part and are reported apart, never in it.
pub fn gate_tokens(run: &CardRun) -> u64 {
    run.usage.fresh()
}

/// The judge of a request with no proof: one answer, with no sign of the arm, acceptable or not.
pub const ACCEPT_INSTRUCTIONS: &str = "You judge one answer to a request from a person. You do not know who or what wrote it, and names \
inside it say nothing about it. Decide whether the person got what they asked for: it does what was asked, is correct, complete and \
clear, without padding or invented facts, and they would not need to ask again or redo it. When work was done in a folder, the files \
matter more than the words about them. An automatic check is a fact: a failed one is a real defect. A longer answer is not a better \
one. Think briefly, then end with one line of JSON and nothing after it: \
{\"acceptable\":true or false,\"score\":<1-10>,\"reason\":\"one or two plain sentences\"}.";

pub fn accept_prompt(request: &str, entry: &str) -> String {
    format!("## The request\n{}\n\n## The answer\n{entry}\n", request.trim())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Accept {
    pub acceptable: bool,
    pub score: Option<f64>,
    pub reason: String,
}

/// The judge's last JSON object with an `acceptable` flag. Text before it is ignored.
pub fn parse_accept(text: &str) -> Option<Accept> {
    let mut end = text.len();
    while let Some(close) = text[..end].rfind('}') {
        let mut start = close;
        while let Some(open) = text[..start].rfind('{') {
            if let Ok(v) = serde_json::from_str::<Value>(&text[open..=close])
                && let Some(acceptable) = v.get("acceptable").and_then(Value::as_bool)
            {
                return Some(Accept {
                    acceptable,
                    score: v.get("score").and_then(Value::as_f64),
                    reason: v.get("reason").and_then(Value::as_str).unwrap_or_default().trim().to_string(),
                });
            }
            start = open;
        }
        end = close;
    }
    None
}

/// The pairs a group runs before "three more pairs" has been used: the spec runs three, then three more when too close.
pub const BASE_PAIRS: usize = 3;
/// The most the card may add to input plus output, as a share of what the runs without it used.
pub const ALLOWED_GROWTH: f64 = 0.05;

/// One measure over a group's usable pairs: the sums for each arm, their difference and the spread of the per-pair
/// differences (each pair's difference is summed over the requests; the spread is their sample standard deviation).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Delta {
    pub card: u64,
    pub no_card: u64,
    pub diff: i64,
    pub spread: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GateResult {
    Pass,
    Fail,
    /// Within the spread of the paired runs: three more pairs run.
    TooClose,
}

impl GateResult {
    pub fn word(self) -> &'static str {
        match self {
            GateResult::Pass => "PASS",
            GateResult::Fail => "FAIL",
            GateResult::TooClose => "TOO CLOSE (three more pairs)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GateRow {
    pub vendor: String,
    pub condition: Condition,
    pub requests: usize,
    pub pairs: usize,
    /// Runs left out because the other arm of their pair is missing, or the pair is incomplete in another request.
    pub left_out: usize,
    /// The gate's measure: input + cache writes + output.
    pub tokens: Delta,
    /// `tokens.diff` as a share of the runs without the card.
    pub percent: f64,
    pub input: Delta,
    pub cache_write: Delta,
    pub output: Delta,
    /// Reported apart; not in `tokens`.
    pub cache_read: Delta,
    pub replies: Delta,
    pub tool_calls: Delta,
    pub successes_card: usize,
    pub successes_no_card: usize,
    pub result: GateResult,
    pub why: String,
}

type PairedRuns<'a> = Vec<(Vec<&'a CardRun>, Vec<&'a CardRun>)>;

fn delta(pairs: &PairedRuns, measure: fn(&CardRun) -> u64) -> Delta {
    let sums: Vec<(u64, u64)> =
        pairs.iter().map(|(c, n)| (c.iter().map(|r| measure(r)).sum(), n.iter().map(|r| measure(r)).sum())).collect();
    let (card, no_card) = sums.iter().fold((0, 0), |a, s| (a.0 + s.0, a.1 + s.1));
    let diffs: Vec<f64> = sums.iter().map(|s| s.0 as f64 - s.1 as f64).collect();
    let spread = if diffs.len() < 2 {
        0.0
    } else {
        let mean = diffs.iter().sum::<f64>() / diffs.len() as f64;
        (diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (diffs.len() - 1) as f64).sqrt()
    };
    Delta { card, no_card, diff: card as i64 - no_card as i64, spread }
}

/// The gate, for each CLI and each condition apart. Only pairs that are complete in every request of the group count (the
/// sums are over the same requests in both arms). It passes when the card has no fewer successes and its excess over
/// the allowed 5% (the mean difference of the per-pair sums, less 5% of the mean tokens without the card) is below minus the
/// spread; it fails beyond plus the spread; in between it is too close to call. With more than `BASE_PAIRS` pairs the
/// three more have been run, and too close counts as failing.
pub fn card_gate(runs: &[CardRun]) -> Vec<GateRow> {
    let mut groups: Vec<(String, Condition)> = runs.iter().map(|r| (r.vendor.clone(), r.condition)).collect();
    groups.sort();
    groups.dedup();
    groups.into_iter().map(|(vendor, condition)| gate_group(&vendor, condition, runs)).collect()
}

fn gate_group(vendor: &str, condition: Condition, runs: &[CardRun]) -> GateRow {
    let group: Vec<&CardRun> = runs.iter().filter(|r| r.vendor == vendor && r.condition == condition).collect();
    let mut requests: Vec<&str> = group.iter().map(|r| r.request.as_str()).collect();
    requests.sort();
    requests.dedup();
    let mut indexes: Vec<usize> = group.iter().map(|r| r.pair).collect();
    indexes.sort();
    indexes.dedup();
    let at =
        |request: &str, pair: usize, arm: Arm| group.iter().rev().find(|r| r.request == request && r.pair == pair && r.arm == arm).copied();
    let usable: Vec<usize> = indexes
        .into_iter()
        .filter(|p| requests.iter().all(|q| at(q, *p, Arm::Card).is_some() && at(q, *p, Arm::NoCard).is_some()))
        .collect();
    let pairs: PairedRuns = usable
        .iter()
        .map(|p| {
            let side = |arm| requests.iter().filter_map(|q| at(q, *p, arm)).collect::<Vec<_>>();
            (side(Arm::Card), side(Arm::NoCard))
        })
        .collect();
    let left_out = group.len() - pairs.iter().map(|(c, n)| c.len() + n.len()).sum::<usize>();
    let tokens = delta(&pairs, gate_tokens);
    let percent = if tokens.no_card == 0 { 0.0 } else { tokens.diff as f64 * 100.0 / tokens.no_card as f64 };
    let successes = |card: bool| pairs.iter().flat_map(|p| if card { &p.0 } else { &p.1 }).filter(|r| r.success).count();
    let (successes_card, successes_no_card) = (successes(true), successes(false));
    let n = pairs.len();
    let (result, why) = if n == 0 {
        (GateResult::TooClose, "no complete pair yet".to_string())
    } else if successes_card < successes_no_card {
        (GateResult::Fail, format!("fewer successes with the card ({successes_card} against {successes_no_card})"))
    } else if tokens.no_card == 0 {
        (GateResult::Fail, "nothing was measured without the card".to_string())
    } else if n < 2 {
        (GateResult::TooClose, "one pair has no spread to judge by".to_string())
    } else {
        let excess = (tokens.diff as f64 - ALLOWED_GROWTH * tokens.no_card as f64) / n as f64;
        let spread = tokens.spread;
        let line = format!(
            "{percent:+.1}% tokens with the card; per pair {excess:+.0} beyond the allowed {:.0}%, spread {spread:.0}",
            ALLOWED_GROWTH * 100.0
        );
        if excess > spread {
            (GateResult::Fail, format!("more than 5% more tokens: {line}"))
        } else if excess <= -spread {
            (GateResult::Pass, format!("within 5% and no fewer successes: {line}"))
        } else if n > BASE_PAIRS {
            (GateResult::Fail, format!("still within the spread after more pairs, which counts as failing: {line}"))
        } else {
            (GateResult::TooClose, format!("within the spread: {line}"))
        }
    };
    GateRow {
        vendor: vendor.to_string(),
        condition,
        requests: requests.len(),
        pairs: n,
        left_out,
        percent,
        input: delta(&pairs, |r| r.usage.input),
        cache_write: delta(&pairs, |r| r.usage.cache_write),
        output: delta(&pairs, |r| r.usage.output),
        cache_read: delta(&pairs, |r| r.usage.cache_read),
        replies: delta(&pairs, |r| r.replies),
        tool_calls: delta(&pairs, |r| r.tool_calls),
        tokens,
        successes_card,
        successes_no_card,
        result,
        why,
    }
}

/// The gate's table, the measures apart, and every run, in Markdown.
pub fn render_card_tables(runs: &[CardRun], gate: &[GateRow]) -> String {
    let mut out = String::from(
        "| CLI | Condition | Requests | Pairs | Successes card / no card | Tokens card | Tokens no card | Difference | Spread | Result |\n\
         | :--- | :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | :--- |\n",
    );
    for g in gate {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} / {} | {} | {} | {:+} ({:+.1}%) | {:.0} | {} |\n",
            g.vendor,
            g.condition.name(),
            g.requests,
            g.pairs,
            g.successes_card,
            g.successes_no_card,
            g.tokens.card,
            g.tokens.no_card,
            g.tokens.diff,
            g.percent,
            g.tokens.spread,
            g.result.word()
        ));
    }
    out.push_str("\nTokens are input + cache writes + output; cache reads are listed apart and are not in them.\n");
    for g in gate {
        let left = if g.left_out > 0 { format!(", {} run(s) left out for want of a complete pair", g.left_out) } else { String::new() };
        out.push_str(&format!(
            "\n{} {}, {} pair(s) over {} request(s){left}: {}\n",
            g.vendor,
            g.condition.name(),
            g.pairs,
            g.requests,
            g.why
        ));
    }
    out.push_str("\n| CLI | Condition | Measure | Card | No card | Difference ± spread |\n| :--- | :--- | :--- | ---: | ---: | ---: |\n");
    for g in gate {
        for (name, d) in [
            ("input (not cached)", &g.input),
            ("cache writes", &g.cache_write),
            ("output", &g.output),
            ("cached input (cache reads, not in the gate)", &g.cache_read),
            ("agent replies (more than one a session: checks sent it back)", &g.replies),
            ("tool uses", &g.tool_calls),
        ] {
            out.push_str(&format!(
                "| {} | {} | {name} | {} | {} | {:+} ± {:.0} |\n",
                g.vendor,
                g.condition.name(),
                d.card,
                d.no_card,
                d.diff,
                d.spread
            ));
        }
    }
    out.push_str(
        "\n| CLI | Condition | Pair | Request | Arm | Success | Proof | Judge | Tokens | Cached | Replies | s | US$ |\n\
         | :--- | :--- | ---: | :--- | :--- | :--- | :--- | :--- | ---: | ---: | ---: | ---: | ---: |\n",
    );
    let mut sorted: Vec<&CardRun> = runs.iter().collect();
    sorted.sort_by(|a, b| (&a.vendor, a.condition, a.pair, &a.request, a.arm).cmp(&(&b.vendor, b.condition, b.pair, &b.request, b.arm)));
    let yes = |v: Option<bool>| v.map_or("-", |b| if b { "pass" } else { "FAIL" });
    for r in sorted {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.0} | {} |\n",
            r.vendor,
            r.condition.name(),
            r.pair + 1,
            r.request,
            r.arm.name(),
            if r.success { "yes" } else { "NO" },
            yes(r.proof),
            yes(r.accepted),
            gate_tokens(r),
            r.usage.cache_read,
            r.replies,
            r.secs,
            usd_word(r.cost_usd)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUEST: &str = "---\nid: organize-folder\ntitle: Organize a messy folder\nkind: project\nfixture: organize\nproof: organize\n---\n\nPlease organize this.\nKeep every file.\n";

    #[test]
    fn every_request_file_in_the_repository_parses() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../bench/requests");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "md") {
                let text = std::fs::read_to_string(&path).unwrap();
                parse_request(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                count += 1;
            }
        }
        assert!(count >= 5, "expected the five request files, found {count}");
    }

    #[test]
    fn a_request_file_gives_its_header_and_its_prompt_untouched() {
        let r = parse_request(REQUEST).unwrap();
        assert_eq!(
            (r.id.as_str(), r.kind, r.fixture.as_deref(), r.proof.as_deref()),
            ("organize-folder", Kind::Project, Some("organize"), Some("organize"))
        );
        assert_eq!(r.prompt, "Please organize this.\nKeep every file.");
        let chat =
            parse_request(&REQUEST.replace("kind: project", "kind: chat").replace("proof: organize", "proof: none").replace("\n", "\r\n"))
                .unwrap();
        assert_eq!((chat.kind, chat.proof), (Kind::Chat, None));
    }

    #[test]
    fn a_request_file_with_a_mistake_is_refused_with_the_reason() {
        for (text, why) in [
            ("id: x\n", "starts with"),
            ("---\nid: x\nkind: chat\n", "closing"),
            ("---\nid: x\nkind: chat\n---\n   \n", "no prompt"),
            ("---\nid: x\nkind: video\n---\nhi\n", "kind is"),
            ("---\nid: X Y\nkind: chat\n---\nhi\n", "lowercase"),
            ("---\nid: x\nkind: chat\nspeed: fast\n---\nhi\n", "unknown header key"),
            ("---\nkind: chat\n---\nhi\n", "an id"),
            ("---\nid: x\n---\nhi\n", "a kind"),
        ] {
            let error = parse_request(text).unwrap_err();
            assert!(error.contains(why), "{text:?} gave {error}");
        }
    }

    #[test]
    fn the_sides_alternate_as_a_and_b_and_the_same_run_deals_the_same_way() {
        for run in ["20261022-1", "20261022-2", "x", ""] {
            let sides: Vec<Side> = (0..5).map(|i| a_side(run, i)).collect();
            assert_eq!(sides, (0..5).map(|i| a_side(run, i)).collect::<Vec<_>>());
            assert!(sides.windows(2).all(|w| w[0] != w[1]), "{run}: {sides:?}");
        }
        let firsts: Vec<Side> = (0..40).map(|n| a_side(&format!("run-{n}"), 0)).collect();
        assert!(firsts.contains(&Side::Faber) && firsts.contains(&Side::Plain), "the first side depends on the run id");
    }

    #[test]
    fn the_judge_names_a_letter_and_the_reveal_turns_it_into_a_side() {
        let text = "A is wrong about the dates {not json}.\n{\"winner\":\"B\",\"a\":4,\"b\":8,\"reason\":\" B keeps every file. \"}";
        let v = parse_verdict(text).unwrap();
        assert_eq!((v.pick, v.score_a, v.score_b, v.reason.as_str()), (Pick::B, Some(4.0), Some(8.0), "B keeps every file."));
        assert_eq!(reveal(Pick::B, Side::Faber), Some(Side::Plain));
        assert_eq!(reveal(Pick::A, Side::Faber), Some(Side::Faber));
        assert_eq!(reveal(Pick::A, Side::Plain), Some(Side::Plain));
        assert_eq!(reveal(Pick::Tie, Side::Plain), None);
        assert_eq!(parse_verdict("{\"winner\":\"tie\"}").unwrap().pick, Pick::Tie);
        assert_eq!(parse_verdict("```json\n{\"winner\": \"a\"}\n```").unwrap().pick, Pick::A);
    }

    #[test]
    fn an_answer_that_is_not_a_verdict_is_not_taken_for_one() {
        for text in ["", "I prefer A.", "{\"winner\":\"C\"}", "{\"reason\":\"x\"}", "{\"winner\":"] {
            assert!(parse_verdict(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn what_the_judge_reads_never_names_the_app_and_is_cut_to_a_bounded_size() {
        assert_eq!(scrub("Faber and FABER and faber"), "the app and the app and the app");
        let files = vec![
            ("faber/notes.md".to_string(), Some("Written by Faber.".to_string())),
            ("pic.png".to_string(), None),
            ("big.txt".to_string(), Some("é".repeat(40_000))),
        ];
        let entry = render_entry("Done by Faber.", &files, None);
        assert!(!entry.to_ascii_lowercase().contains("faber"));
        assert!(entry.contains("- pic.png") && entry.contains("[cut]") && entry.len() < 45_000, "{}", entry.len());
        let proof = Proof { pass: false, checks: vec![Check { name: "a".into(), ok: false, detail: "x".into() }] };
        assert!(render_entry("r", &[], Some(&proof)).contains("Automatic check: FAILED"));
    }

    #[test]
    fn the_claude_result_gives_the_reply_the_tokens_and_the_cost() {
        let json = r#"{"type":"result","subtype":"success","is_error":false,"result":"Done.","num_turns":3,"total_cost_usd":0.1234,
            "usage":{"input_tokens":10,"output_tokens":20,"cache_creation_input_tokens":300,"cache_read_input_tokens":4000},
            "modelUsage":{"claude-sonnet-5-5":{}}}"#;
        let o = parse_claude_result(json).unwrap();
        assert_eq!(
            (o.text.as_str(), o.ok, o.cost_usd, o.turns, o.models.clone()),
            ("Done.", true, Some(0.1234), Some(3), vec!["claude-sonnet-5-5".to_string()])
        );
        assert_eq!((o.usage.fresh(), o.usage.cache_read), (330, 4000));
        let list = format!("[{{\"type\":\"system\"}},{json}]");
        assert_eq!(parse_claude_result(&list).unwrap(), o);
        let bad = parse_claude_result(r#"{"type":"result","subtype":"error_max_budget_usd","is_error":false,"result":""}"#).unwrap();
        assert!(!bad.ok && bad.error.unwrap().contains("error_max_budget_usd"));
        assert!(parse_claude_result("not json").is_err() && parse_claude_result("[]").is_err());
    }

    #[test]
    fn codex_events_give_the_last_message_and_the_usage_of_every_turn() {
        let lines = "{\"type\":\"thread.started\"}\nnoise\n{\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"first\"}}\n\
            {\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1000,\"cached_input_tokens\":800,\"output_tokens\":50}}\n\
            {\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"last\"}}\n\
            {\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":500,\"cached_input_tokens\":0,\"output_tokens\":5}}\n";
        let o = parse_codex_events(lines).unwrap();
        assert_eq!((o.text.as_str(), o.ok, o.turns, o.cost_usd), ("last", true, Some(2), None));
        assert_eq!((o.usage.input, o.usage.cache_read, o.usage.output), (700, 800, 55));
        assert!(!parse_codex_events("{\"type\":\"turn.failed\",\"error\":{}}\n").unwrap().ok);
        assert!(parse_codex_events("no events").is_err());
    }

    #[test]
    fn a_reply_is_cut_at_its_utf16_offset() {
        assert_eq!(from_utf16("notes. Answer", 6), " Answer");
        assert_eq!(from_utf16("é😀 rest", 3), " rest");
        assert_eq!(from_utf16("short", 99), "");
        assert_eq!(from_utf16("all", 0), "all");
    }

    #[test]
    fn a_proof_passes_only_when_every_check_does() {
        let out = "log line\n{\"pass\":true,\"checks\":[{\"name\":\"a\",\"ok\":true},{\"name\":\"b\",\"ok\":false,\"detail\":\"d\"}]}\n";
        let p = parse_proof(out).unwrap();
        assert!(!p.pass);
        assert_eq!((p.checks.len(), p.checks[1].detail.as_str()), (2, "d"));
        assert!(parse_proof("{\"pass\":true,\"checks\":[]}").is_none() && parse_proof("nothing").is_none());
        assert!(parse_proof("{\"checks\":[{\"name\":\"a\",\"ok\":true}]}").unwrap().pass);
    }

    #[test]
    fn the_cap_stops_a_run_on_dollars_or_on_first_time_tokens_and_never_gives_a_negative_allowance() {
        let cap = Cap { max_usd: 8.0, max_fresh_tokens: 1000 };
        let mut spent = Spend::default();
        assert!(cap.reached(&spent).is_none());
        spent.add(&Usage { input: 300, output: 200, cache_read: 9_000_000, cache_write: 400 }, Some(7.5));
        assert!(cap.reached(&spent).is_none(), "cache reads are not counted");
        assert_eq!(cap.allowance(&spent, 1.5), 0.5);
        spent.add(&Usage { input: 200, ..Default::default() }, None);
        assert!(cap.reached(&spent).unwrap().contains("first-time tokens"));
        spent.add(&Usage::default(), Some(1.0));
        assert!(cap.reached(&spent).unwrap().contains("US$8.50 spent"));
        assert_eq!(cap.allowance(&spent, 1.5), 0.0);
    }

    fn side(which: Side, usd: f64, pass: Option<bool>) -> SideResult {
        SideResult {
            side: which,
            ok: true,
            error: None,
            secs: 90.0,
            wall_secs: 120.0,
            usage: Usage { input: 10, output: 20, cache_read: 500, cache_write: 70 },
            cost_usd: Some(usd),
            models: vec![],
            reply: String::new(),
            folder: None,
            proof: pass.map(|p| Proof { pass: p, checks: vec![Check { name: "c".into(), ok: p, detail: String::new() }] }),
            stopped_by_cap: false,
        }
    }

    #[test]
    fn the_report_shows_the_verdict_by_side_the_proofs_and_the_totals() {
        let verdict = Some(Verdict { pick: Pick::B, score_a: None, score_b: None, reason: String::new() });
        let rows = vec![
            Row {
                request: "organize-folder".into(),
                title: "t".into(),
                kind: Kind::Project,
                a: Side::Plain,
                verdict: verdict.clone(),
                winner: Some(Side::Faber),
                faber: Some(side(Side::Faber, 0.2, Some(true))),
                plain: Some(side(Side::Plain, 0.4, Some(false))),
                judge_cost_usd: Some(0.3),
                judge_usage: Usage::default(),
                skipped: None,
            },
            Row {
                request: "research".into(),
                title: "t".into(),
                kind: Kind::Chat,
                a: Side::Faber,
                verdict: None,
                winner: None,
                faber: None,
                plain: None,
                judge_cost_usd: None,
                judge_usage: Usage::default(),
                skipped: Some("the cap was reached".into()),
            },
        ];
        let mut spent = Spend::default();
        spent.add(&Usage { input: 20, output: 40, cache_read: 1000, cache_write: 140 }, Some(0.9));
        let text = render_tables(&rows, &spent, &Cap { max_usd: 8.0, max_fresh_tokens: 3_000_000 });
        assert!(text.contains("| organize-folder | Project | Faber | pass (1/1) | FAIL (0/1) | 90 | 90 | 100 (500 cached) | 100 (500 cached) | 0.200 | 0.400 |"), "{text}");
        assert!(
            text.contains("| Judge wins (ties 0) | 1 | 0 |") && text.contains("| Faber cost as a share of plain | 50% | 100% |"),
            "{text}"
        );
        assert!(
            text.contains("Not run: research (the cap was reached)") && text.contains("US$0.600 for the sides, US$0.300 for the judge"),
            "{text}"
        );
    }

    fn card_run(request: &str, pair: usize, arm: Arm, tokens: u64, success: bool) -> CardRun {
        CardRun {
            vendor: "claude".into(),
            request: request.into(),
            condition: Condition::Cold,
            pair,
            arm,
            success,
            usage: Usage { input: tokens / 2, output: tokens / 4, cache_read: 9_000, cache_write: tokens - tokens / 2 - tokens / 4 },
            replies: 1,
            tool_calls: 3,
            secs: 10.0,
            cost_usd: None,
            proof: None,
            accepted: None,
            note: None,
        }
    }

    /// Two requests, one pair per entry of `pairs`: (tokens with the card, tokens without) of each request together.
    fn card_runs(pairs: &[(u64, u64)]) -> Vec<CardRun> {
        let mut runs = Vec::new();
        for (p, (with, without)) in pairs.iter().enumerate() {
            for request in ["a", "b"] {
                runs.push(card_run(request, p, Arm::Card, with / 2, true));
                runs.push(card_run(request, p, Arm::NoCard, without / 2, true));
            }
        }
        runs
    }

    #[test]
    fn the_arms_alternate_which_goes_first_and_the_conditions_parse() {
        assert_eq!((first_arm(0), first_arm(1), first_arm(2), first_arm(5)), (Arm::Card, Arm::NoCard, Arm::Card, Arm::NoCard));
        assert_eq!(Arm::Card.other(), Arm::NoCard);
        assert_eq!(parse_conditions("warm, cold,warm").unwrap(), vec![Condition::Warm, Condition::Cold]);
        assert!(parse_conditions("cold,hot").unwrap_err().contains("hot") && parse_conditions(" ").is_err());
    }

    #[test]
    fn the_acceptance_verdict_is_the_last_json_with_the_flag() {
        let a = parse_accept("It works. {not json}\n{\"acceptable\": false, \"score\": 4, \"reason\": \" Asks nothing. \"}").unwrap();
        assert_eq!((a.acceptable, a.score, a.reason.as_str()), (false, Some(4.0), "Asks nothing."));
        assert!(parse_accept("{\"acceptable\":true}").unwrap().acceptable);
        for text in ["", "yes", "{\"winner\":\"A\"}", "{\"acceptable\":\"yes\"}"] {
            assert!(parse_accept(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn the_gate_counts_input_cache_writes_and_output_and_never_cache_reads() {
        assert_eq!(gate_tokens(&card_run("a", 0, Arm::Card, 1000, true)), 1000, "a run's cache reads of 9000 are not in it");
        let row = &card_gate(&card_runs(&[(1000, 1000)]))[0];
        assert_eq!((row.cache_read.card, row.tokens.card), (18_000, 1000));
        assert_eq!(row.input.card + row.cache_write.card + row.output.card, row.tokens.card);
    }

    #[test]
    fn a_card_that_adds_clearly_less_than_five_percent_passes() {
        // Per pair (sums over the two requests): +2%, +1%, +3% against a spread of a pair's difference near 1% of the tokens.
        let rows = card_gate(&card_runs(&[(10_200, 10_000), (10_100, 10_000), (10_300, 10_000)]));
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].pairs, rows[0].requests, rows[0].left_out), (3, 2, 0));
        assert_eq!((rows[0].tokens.card, rows[0].tokens.no_card, rows[0].tokens.diff), (30_600, 30_000, 600));
        assert!((rows[0].tokens.spread - 100.0).abs() < 1e-9, "{}", rows[0].tokens.spread);
        assert_eq!(rows[0].result, GateResult::Pass, "{}", rows[0].why);
    }

    #[test]
    fn a_card_that_adds_clearly_more_than_five_percent_fails() {
        let rows = card_gate(&card_runs(&[(11_000, 10_000), (11_100, 10_000), (10_900, 10_000)]));
        assert_eq!(rows[0].result, GateResult::Fail, "{}", rows[0].why);
        assert!(rows[0].why.contains("more than 5%"), "{}", rows[0].why);
    }

    #[test]
    fn a_difference_near_the_line_is_too_close_then_fails_after_three_more_pairs() {
        // About +5% with a noisy spread: neither side of the line is clear.
        let noisy = [(10_900, 10_000), (10_100, 10_000), (10_800, 10_000)];
        let rows = card_gate(&card_runs(&noisy));
        assert_eq!(rows[0].result, GateResult::TooClose, "{}", rows[0].why);
        let mut six = noisy.to_vec();
        six.extend([(10_900, 10_000), (10_100, 10_000), (10_800, 10_000)]);
        let rows = card_gate(&card_runs(&six));
        assert_eq!((rows[0].pairs, rows[0].result), (6, GateResult::Fail), "{}", rows[0].why);
        assert!(rows[0].why.contains("after more pairs"));
        assert_eq!(card_gate(&card_runs(&[(10_000, 10_000)]))[0].result, GateResult::TooClose, "one pair has no spread");
    }

    #[test]
    fn a_card_with_fewer_successes_fails_whatever_it_saves() {
        let mut runs = card_runs(&[(5_000, 10_000), (5_000, 10_000), (5_000, 10_000)]);
        assert_eq!(card_gate(&runs)[0].result, GateResult::Pass);
        runs[0].success = false;
        let row = &card_gate(&runs)[0];
        assert_eq!((row.successes_card, row.successes_no_card, row.result), (5, 6, GateResult::Fail));
        runs[1].success = false;
        assert_eq!(card_gate(&runs)[0].result, GateResult::Pass, "as many successes with the card as without");
    }

    #[test]
    fn only_pairs_complete_in_every_request_count_and_each_cli_and_condition_is_apart() {
        let mut runs = card_runs(&[(10_000, 10_000), (10_000, 10_000), (10_000, 10_000)]);
        // The second request's card run of pair 3 is missing: that pair is left out of both requests.
        runs.push(card_run("a", 3, Arm::Card, 5_000, true));
        runs.push(card_run("a", 3, Arm::NoCard, 5_000, true));
        runs.push(card_run("b", 3, Arm::NoCard, 5_000, true));
        let rows = card_gate(&runs);
        assert_eq!((rows[0].pairs, rows[0].left_out), (3, 3));
        let mut codex = card_runs(&[(10_000, 10_000)]);
        for r in &mut codex {
            r.vendor = "codex".into();
            r.condition = Condition::Warm;
        }
        runs.extend(codex);
        let keys: Vec<_> = card_gate(&runs).iter().map(|g| (g.vendor.clone(), g.condition)).collect();
        assert_eq!(keys, vec![("claude".to_string(), Condition::Cold), ("codex".to_string(), Condition::Warm)]);
    }

    #[test]
    fn the_card_tables_name_the_gate_the_measures_apart_and_every_run() {
        let runs = card_runs(&[(10_200, 10_000), (10_100, 10_000), (10_300, 10_000)]);
        let text = render_card_tables(&runs, &card_gate(&runs));
        assert!(text.contains("| claude | cold | 2 | 3 | 6 / 6 | 30600 | 30000 | +600 (+2.0%) | 100 | PASS |"), "{text}");
        assert!(
            text.contains("cached input (cache reads, not in the gate)") && text.contains("| claude | cold | 1 | a | card | yes |"),
            "{text}"
        );
        let saved: Vec<CardRun> = serde_json::from_str(&serde_json::to_string(&runs).unwrap()).unwrap();
        assert_eq!(saved, runs, "results.json reads back into the same runs");
    }
}
