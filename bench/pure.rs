//! The benchmark's pure parts: what a request file says, who is "A" and "B", what the judge answered, what a CLI
//! reported, the spend cap and the report. Nothing here starts a process, reads a file or calls a CLI, so the engine's
//! tests run it (`engine/tests/all.rs`) and the runner (`engine/examples/bench.rs`) includes the same file.
//! How to run the benchmark: docs/dev/benchmark.md.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct QuietSample {
    pub key: String,
    pub epoch: f64,
    pub identity: String,
    pub at: f64,
    pub used: f64,
    pub reset: f64,
    pub lag: f64,
    pub max_age: f64,
    /// The latest own work's end plus the vendor's reporting lag.
    pub safe_after: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    Ready,
    Wait,
    Unknown,
    Aborted,
}

#[derive(Default)]
pub struct QuietGate {
    previous: std::collections::BTreeMap<String, QuietSample>,
    seen: std::collections::BTreeMap<String, QuietSample>,
    aborted: bool,
    after_work: f64,
}

impl QuietGate {
    pub fn after_work(&mut self, now: f64) {
        self.previous.clear();
        self.after_work = now;
    }
    pub fn observe(&mut self, samples: Option<&[QuietSample]>, now: f64, cancelled: bool) -> Quiet {
        if cancelled {
            self.aborted = true;
        }
        if self.aborted {
            return Quiet::Aborted;
        }
        let Some(samples) = samples.filter(|samples| !samples.is_empty()) else {
            self.previous.clear();
            return Quiet::Unknown;
        };
        let mut ready = true;
        for sample in samples {
            let valid = [now, sample.at, sample.used, sample.reset, sample.lag, sample.max_age, sample.safe_after, sample.epoch]
                .iter()
                .all(|v| v.is_finite())
                && (0.0..=1.5).contains(&sample.used)
                && sample.reset > now
                && sample.at <= now
                && now - sample.at <= sample.max_age
                && sample.lag > 0.0
                && sample.at >= sample.safe_after.max(self.after_work + sample.lag);
            if !valid {
                self.previous.clear();
                return Quiet::Unknown;
            }
            if self.seen.get(&sample.key).is_some_and(|seen| {
                sample.at < seen.at
                    || sample.epoch < seen.epoch
                    || (sample.at == seen.at
                        && (sample.used != seen.used
                            || sample.reset != seen.reset
                            || sample.epoch != seen.epoch
                            || sample.identity != seen.identity))
            }) {
                self.previous.clear();
                return Quiet::Unknown;
            }
            if !self.seen.contains_key(&sample.key) && self.seen.len() >= 128 {
                return Quiet::Unknown;
            }
            self.seen.insert(sample.key.clone(), sample.clone());
            let prior = self.previous.get(&sample.key);
            let same = prior.is_some_and(|p| {
                p.epoch == sample.epoch
                    && p.identity == sample.identity
                    && p.reset == sample.reset
                    && p.used == sample.used
                    && sample.at >= p.at
                    && now - p.at <= sample.max_age
                    && p.at >= sample.safe_after
            });
            if !same {
                self.previous.insert(sample.key.clone(), sample.clone());
                ready = false;
            } else if prior.is_none_or(|p| sample.at - p.at < sample.lag) {
                ready = false;
            }
        }
        self.previous.retain(|key, _| samples.iter().any(|s| s.key == *key));
        if ready { Quiet::Ready } else { Quiet::Wait }
    }
}

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
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
    pub usage_incomplete: bool,
    /// API-equivalent dollars; none when the native report or model/context pricing is unavailable.
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
        usage_incomplete: !["input_tokens", "output_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
            .iter()
            .all(|key| usage[*key].as_u64().is_some()),
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64).filter(|cost| cost.is_finite() && *cost >= 0.0),
        models: result.get("modelUsage").and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default(),
        turns: result.get("num_turns").and_then(Value::as_u64),
    })
}

/// `codex exec --json`: one event per line; the last agent message is the reply and each finished turn adds its usage.
/// Codex reports no price; `price_codex` applies the shared model prices when context evidence permits. Not yet run against a
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
                let written = number(u, "cache_write_input_tokens");
                out.usage_incomplete |= !["input_tokens", "output_tokens", "cached_input_tokens", "cache_write_input_tokens"]
                    .iter()
                    .all(|key| u[*key].as_u64().is_some())
                    || cached.checked_add(written).is_none_or(|cache| cache > number(u, "input_tokens"));
                out.usage.add(&Usage {
                    input: number(u, "input_tokens").saturating_sub(cached.saturating_add(written)),
                    output: number(u, "output_tokens"),
                    cache_read: cached,
                    cache_write: written,
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

/// CLI turn totals do not identify each request's context tier. Large totals stay unknown.
pub fn price_codex(out: &mut Outcome, model: &str, price: impl Fn(u64, u64, u64, u64) -> Option<f64>) {
    out.models = vec![model.to_string()];
    out.cost_usd = None;
    if !out.usage_incomplete && out.turns == Some(1) && out.usage.input + out.usage.cache_read + out.usage.cache_write <= 272_000 {
        out.cost_usd = price(
            out.usage.input + out.usage.cache_read + out.usage.cache_write,
            out.usage.output,
            out.usage.cache_read,
            out.usage.cache_write,
        );
    }
}

/// A private benchmark engine's ledger delta includes routing and auxiliary work, unlike messages alone.
pub fn ledger_cost(before: (f64, u64), after: (f64, u64)) -> Option<f64> {
    (before.0.is_finite() && after.0.is_finite() && after.0 >= before.0 && after.1 == before.1).then_some(after.0 - before.0)
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
    pub unknown_cost: bool,
    pub usage: Usage,
}

impl Spend {
    pub fn add(&mut self, usage: &Usage, usd: Option<f64>) {
        match usd.filter(|c| c.is_finite() && *c >= 0.0) {
            Some(cost) => self.usd += cost,
            None => self.unknown_cost = true,
        }
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
        } else if spent.unknown_cost {
            Some("cost unknown: no further paid work admitted".into())
        } else {
            None
        }
    }
    /// The dollars one more run may still spend, at most `per_run`.
    pub fn allowance(&self, spent: &Spend, per_run: f64) -> f64 {
        if self.reached(spent).is_some() { 0.0 } else { (self.max_usd - spent.usd).clamp(0.0, per_run) }
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
    /// Routing and auxiliary token counters are not exposed by the engine ledger.
    pub usage_incomplete: bool,
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
    pub judge_attempted: bool,
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
    side.as_ref().map_or("-".into(), |s| {
        format!("{} ({} cached){}", s.usage.fresh(), s.usage.cache_read, if s.usage_incomplete { " partial" } else { "" })
    })
}

fn usd_word(usd: Option<f64>) -> String {
    usd.map_or("unknown".into(), |u| format!("{u:.3}"))
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
        let (mut usd, mut secs) = (Some(0.0), 0.0);
        for r in rows {
            if let Some(s) = pick(r) {
                usage.add(&s.usage);
                usd = usd.zip(s.cost_usd).map(|(total, cost)| total + cost);
                secs += s.secs;
            }
        }
        (usage, usd, secs)
    };
    let (fu, fusd, fs) = side_total(|r| &r.faber);
    let (pu, pusd, ps) = side_total(|r| &r.plain);
    let wins = |side| rows.iter().filter(|r| r.verdict.is_some() && r.winner == Some(side)).count();
    let ties = rows.iter().filter(|r| r.verdict.is_some() && r.winner.is_none()).count();
    let judge_usd: Option<f64> =
        rows.iter().filter(|r| r.judge_attempted).try_fold(0.0, |total, r| r.judge_cost_usd.map(|cost| total + cost));
    let (fword, pword) = (usd_word(fusd), usd_word(pusd));
    out.push_str(&format!(
        "\n| Totals | Faber | Plain |\n| :--- | ---: | ---: |\n| Judge wins (ties {ties}) | {} | {} |\n| Time (s) | {fs:.0} | {ps:.0} |\n\
         | First-time tokens (input, output, cache writes) | {} | {} |\n| Cache-read tokens | {} | {} |\n| API-equivalent US$ | {fword} | {pword} |\n",
        wins(Side::Faber),
        wins(Side::Plain),
        fu.fresh(),
        pu.fresh(),
        fu.cache_read,
        pu.cache_read,
    ));
    if let Some((fusd, pusd)) = fusd.zip(pusd).filter(|(_, plain)| *plain > 0.0) {
        out.push_str(&format!("| Faber cost as a share of plain | {:.0}% | 100% |\n", fusd * 100.0 / pusd));
    }
    out.push_str(&format!(
        "\nSpend of the whole run: US${} for the sides, US${} for the judge; cap US${:.2} or {} first-time tokens ({} used).\n",
        usd_word(fusd.zip(pusd).map(|(f, p)| f + p)),
        usd_word(judge_usd),
        cap.max_usd,
        cap.max_fresh_tokens,
        spent.usage.fresh()
    ));
    if spent.unknown_cost {
        out.push_str("\nWhole-run cost unknown; reported dollars are only a lower bound. No further paid work was admitted.\n");
    }
    if rows.iter().any(|r| r.faber.as_ref().is_some_and(|s| s.usage_incomplete)) {
        out.push_str("\nFaber tokens are partial: routing and auxiliary token counters are unavailable; their costs are included through the ledger. Token totals are lower bounds.\n");
    }
    for r in rows.iter().filter_map(|r| r.skipped.as_ref().map(|why| (r, why))) {
        out.push_str(&format!("\nNot run: {} ({}).\n", r.0.request, r.1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_gate_needs_two_fresh_unchanged_readings_and_abort_is_terminal() {
        let mut gate = QuietGate::default();
        let mut sample = QuietSample {
            key: "claude/week".into(),
            epoch: 0.0,
            identity: "fixture".into(),
            at: 1000.0,
            used: 0.2,
            reset: 10000.0,
            lag: 600.0,
            max_age: 1500.0,
            safe_after: 0.0,
        };
        assert_eq!(gate.observe(None, 1000.0, false), Quiet::Unknown);
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1000.0, false), Quiet::Wait);
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1600.0, false), Quiet::Wait);
        sample.at = 1600.0;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1600.0, false), Quiet::Ready);
        sample.used = 0.21;
        sample.at = 1700.0;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1700.0, false), Quiet::Wait);
        sample.at = 2300.0;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 2300.0, false), Quiet::Ready);
        gate.after_work(2300.0);
        assert_eq!(
            gate.observe(Some(&[sample.clone()]), 2300.0, false),
            Quiet::Unknown,
            "a completed stage cannot release the next with old evidence"
        );
        sample.safe_after = 2400.0;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 2300.0, false), Quiet::Unknown);
        sample.safe_after = 0.0;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 5000.0, false), Quiet::Unknown);
        assert_eq!(gate.observe(None, 5000.0, true), Quiet::Aborted);
        assert_eq!(gate.observe(Some(&[sample]), 2300.0, false), Quiet::Aborted);
    }

    #[test]
    fn quiet_reset_requires_a_new_pair_instead_of_elapsed_time() {
        let mut gate = QuietGate::default();
        let mut sample = QuietSample {
            key: "codex/week".into(),
            epoch: 0.0,
            identity: "fixture".into(),
            at: 1000.0,
            used: 0.2,
            reset: 4000.0,
            lag: 300.0,
            max_age: 1200.0,
            safe_after: 0.0,
        };
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1000.0, false), Quiet::Wait);
        sample.at = 1300.0;
        sample.reset = 5000.0;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1300.0, false), Quiet::Wait);
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1600.0, false), Quiet::Wait);
        sample.at = 1600.0;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1600.0, false), Quiet::Ready);
        sample.at = 1700.0;
        sample.used = f64::NAN;
        assert_eq!(gate.observe(Some(&[sample]), 1700.0, false), Quiet::Unknown);
    }

    #[test]
    fn rolled_back_evidence_cannot_erase_observed_movement() {
        let mut gate = QuietGate::default();
        let mut sample = QuietSample {
            key: "claude/week".into(),
            epoch: 0.0,
            identity: "fixture".into(),
            at: 1000.0,
            used: 0.2,
            reset: 10000.0,
            lag: 600.0,
            max_age: 1500.0,
            safe_after: 0.0,
        };
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1000.0, false), Quiet::Wait);
        sample.at = 1700.0;
        sample.used = 0.3;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1700.0, false), Quiet::Wait);
        sample.at = 1000.0;
        sample.used = 0.2;
        assert_eq!(gate.observe(Some(&[sample.clone()]), 1700.0, false), Quiet::Unknown);
        sample.at = 1600.0;
        assert_eq!(gate.observe(Some(&[sample]), 1700.0, false), Quiet::Unknown);
    }

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
        assert!(o.usage_incomplete, "older events without cache-write counters have incomplete pricing");
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
    fn missing_cost_blocks_further_work_and_is_not_a_free_total() {
        let cap = Cap { max_usd: 8.0, max_fresh_tokens: 1000 };
        let mut spent = Spend::default();
        spent.add(&Usage::default(), None);
        assert!(cap.reached(&spent).is_some(), "unknown spending must close admission");
        assert_eq!(cap.allowance(&spent, 1.5), 0.0);
        let mut row = Row {
            request: "x".into(),
            title: "x".into(),
            kind: Kind::Chat,
            a: Side::Plain,
            verdict: None,
            winner: None,
            faber: Some(side(Side::Faber, 0.2, None)),
            plain: Some(side(Side::Plain, 0.4, None)),
            judge_cost_usd: None,
            judge_attempted: false,
            judge_usage: Usage::default(),
            skipped: None,
        };
        row.plain.as_mut().unwrap().cost_usd = None;
        let table = render_tables(&[row], &spent, &cap);
        assert!(table.contains("unknown"));
        assert!(!table.contains("Faber cost as a share"));
    }

    #[test]
    fn ledger_delta_includes_routing_and_unknown_work_is_never_free() {
        assert_eq!(ledger_cost((10.0, 2), (10.75, 2)), Some(0.75));
        assert_eq!(ledger_cost((10.0, 2), (10.75, 3)), None);
        assert_eq!(ledger_cost((10.0, 2), (9.0, 2)), None);
        let mut out =
            parse_codex_events(r#"{"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":20,"cache_write_input_tokens":0,"output_tokens":5}}"#)
                .unwrap();
        price_codex(&mut out, "known", |input, output, read, prompt| {
            assert_eq!((input, output, read, prompt), (100, 5, 20, 0));
            Some(0.25)
        });
        assert_eq!(out.cost_usd, Some(0.25));
        let mut missing =
            parse_codex_events(r#"{"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":5}}"#)
                .unwrap();
        price_codex(&mut missing, "known", |_, _, _, _| panic!("missing writes are not zero"));
        assert_eq!(missing.cost_usd, None);
        price_codex(&mut out, "unknown", |_, _, _, _| None);
        assert_eq!(out.cost_usd, None);
        out.usage.input = 300_000;
        price_codex(&mut out, "known", |_, _, _, _| panic!("unknown request tier must not be guessed"));
        assert_eq!(out.cost_usd, None);
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
            usage_incomplete: false,
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
                judge_attempted: true,
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
                judge_attempted: false,
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
}
