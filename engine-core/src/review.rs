//! Score read-only reviews and decide whether another round is useful.

use serde::{Deserialize, Serialize};

pub const TOOL: &str = "Review";
pub const NAME: &str = "review";
pub const CLAUDE_NAME: &str = "mcp__faber__review";
const DESCRIPTION: &str = "Score your changes by two independent reviewers. Fix only the problems the result lists as agreed; call it again after fixing and follow its last line (fix, put back, or stop). Use it when asked for a review and at the close of a task that changed five or more files, never for a small change.";

pub fn codex_tool() -> serde_json::Value {
    serde_json::json!({"type":"function", "name":NAME, "description":DESCRIPTION, "inputSchema":schema()})
}
pub fn mcp_tool() -> serde_json::Value {
    serde_json::json!({"name":NAME, "description":DESCRIPTION, "inputSchema":schema()})
}
fn schema() -> serde_json::Value {
    serde_json::json!({"type":"object", "properties":{"request":{"type":"string"}, "files":{"type":"array", "items":{"type":"string"}}}, "required":["request", "files"], "additionalProperties":false})
}

pub const LINES: [&str; 5] = ["review.line.asked", "review.line.correct", "review.line.safe", "review.line.simple", "review.line.checked"];
pub const TARGET: u32 = 21;
pub const MAX_ROUNDS: usize = 3;
pub const JESTER_GAP: u32 = 6;
pub const NOISE: u32 = 1;
pub const MAX_ROWS: usize = 20;
pub const MAX_ROW_CHARS: usize = 300;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct History {
    pub rounds: Vec<Round>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub line: u32,
    pub file: String,
    pub at: Option<u32>,
    pub what: String,
    pub should: String,
    /// What shows the problem is real: the code as it stands, the sentence of the request it breaks, or a failing run. A
    /// finding without it is kept as a note and never sends the agent back to fix.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub evidence: String,
    /// P0 (breaks or loses data) to P3 (polish), as the reviewer judged it. Absent on rows written before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// A person using the result would notice it on a first pass.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub visible: bool,
}

impl Row {
    /// What the agent must fix: anything a person would notice, whatever its severity, and any P0 or P1 problem nobody
    /// would see (owner's rule of 2026-11-13, "IA (Erick delegou)" for the technical part). A row with no severity blocks.
    pub fn blocks(&self) -> bool {
        self.visible || self.severity.as_deref().is_none_or(|s| matches!(s, "P0" | "P1"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub lines: [u32; 5],
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    #[serde(flatten)]
    pub row: Row,
    pub reviewers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    /// The agent's review call that asked for this round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<String>,
    pub lines: [u32; 5],
    pub total: u32,
    pub reviewers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scores: Vec<(String, [u32; 5])>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted: Vec<String>,
    pub jesters: Vec<String>,
    pub agreed: Vec<Finding>,
    pub notes: Vec<Finding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Target,
    NoAgreed,
    Limit,
    Flat,
}

impl Reason {
    /// The locale key the screen shows for why the rounds stopped.
    pub fn key(self) -> &'static str {
        match self {
            Reason::Target => "review.stop.target",
            Reason::NoAgreed => "review.stop.agreed",
            Reason::Limit => "review.stop.limit",
            Reason::Flat => "review.stop.flat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Stop(Reason),
    Revert,
    Fix,
}

/// The two review roles. The default one hunts defects; the second, given to a fresh session of the writer's own AI, is
/// skeptical and checks the request part by part (the fresh-session fallback of R146).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Defects,
    Skeptic,
}

/// What every reviewer is given: the person's request, the real changes, and the writer's own account marked as a claim to
/// check, never as a fact. `diff` is the changes against the copy base, cut to a bound by the engine.
pub fn brief(role: Role, request: &str, claim: &str, diff: &str) -> String {
    let role = match role {
        Role::Defects => "Find the defects the author missed.",
        Role::Skeptic => {
            "You did not write this change and its author may be wrong. Trust nothing it claims: take each part of the request in turn and check it against the changes, then look for what the change breaks."
        }
    };
    format!(
        r#"Review the changes against the copy base. Work read-only and change nothing. {role}
Score each line from 0 to 5 (5 is best): 1. Does what was asked; 2. Correct; 3. Safe; 4. Simple; 5. Checked.
For every lost point give a row with its line number, file, optional line number, what is there, what it should be, and evidence: the code as it stands, the sentence of the request it breaks, or a run that fails. A row without evidence is dropped. Give severity P0 (breaks or loses data), P1 (wrong result), P2 (worse but works) or P3 (polish), and visible: true when a person using the result would notice it on a first pass. Nothing to report (all fives and no rows) is a correct answer. Never guess at a problem you cannot point to.
Answer with one JSON object and nothing else: {{"lines":[5 ints],"rows":[{{"line":1-5,"file":"path","at":int|null,"what":"...","should":"...","evidence":"...","severity":"P0-P3","visible":bool}}]}}

The person's request:
{request}

The author's claim (check it, do not trust it):
{claim}

Changes against the copy base:
{diff}"#
    )
}

/// Read the first balanced object and reject scores without an explanation.
pub fn parse(text: &str) -> Result<Verdict, &'static str> {
    let start = text.find('{').ok_or("error.review.no_json")?;
    let mut depth = 0usize;
    let mut string = false;
    let mut escape = false;
    let mut end = None;
    for (offset, c) in text[start..].char_indices() {
        if string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                string = false;
            }
        } else if c == '"' {
            string = true;
        } else if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                end = Some(start + offset + 1);
                break;
            }
        }
    }
    let value: serde_json::Value =
        serde_json::from_str(&text[start..end.ok_or("error.review.no_json")?]).map_err(|_| "error.review.no_json")?;
    let lines = value.get("lines").and_then(|v| v.as_array()).ok_or("error.review.bad_lines")?;
    if lines.len() != 5 {
        return Err("error.review.bad_lines");
    }
    let mut scores = [0; 5];
    for (i, v) in lines.iter().enumerate() {
        scores[i] = v.as_u64().filter(|n| *n <= 5).ok_or("error.review.bad_lines")? as u32;
    }
    let rows: Vec<Row> = value
        .get("rows")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let line = v.get("line")?.as_u64()?;
            let file = v.get("file")?.as_str()?;
            let what = v.get("what").and_then(|v| v.as_str()).unwrap_or("");
            if !(1..=5).contains(&line) || file.trim().is_empty() || what.trim().is_empty() {
                return None;
            }
            Some(Row {
                line: line as u32,
                file: file.to_owned(),
                at: v.get("at").and_then(|n| n.as_u64()).and_then(|n| u32::try_from(n).ok()),
                what: what.chars().take(MAX_ROW_CHARS).collect(),
                should: v.get("should").and_then(|v| v.as_str()).unwrap_or("").chars().take(MAX_ROW_CHARS).collect(),
                evidence: v.get("evidence").and_then(|v| v.as_str()).unwrap_or("").trim().chars().take(MAX_ROW_CHARS).collect(),
                severity: v
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().to_uppercase())
                    .filter(|s| matches!(s.as_str(), "P0" | "P1" | "P2" | "P3")),
                visible: v.get("visible").and_then(|v| v.as_bool()).unwrap_or(false),
            })
        })
        .collect();
    // A point taken without a row is given back instead of throwing the whole verdict away: on the planted-bug bench of
    // 2026-11-13 a quarter of the verdicts lost every evidenced finding for one unexplained point (A3).
    for (i, score) in scores.iter_mut().enumerate() {
        if *score < 5 && !rows.iter().any(|row| row.line as usize == i + 1) {
            *score = 5;
        }
    }
    Ok(Verdict { lines: scores, rows })
}

/// Lines this far apart in one file can be one problem described by two reviewers.
const NEAR: u32 = 5;
/// Share of meaningful words two descriptions must have in common to be one problem.
const SAME_MEANING: f32 = 0.25;

fn same_file(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.replace('\\', "/").trim_start_matches("./").to_lowercase();
    let (a, b) = (norm(a), norm(b));
    a == b || a.ends_with(&format!("/{b}")) || b.ends_with(&format!("/{a}"))
}

/// The meaningful words of a description, without their endings and cut to four letters, so "writes", "writing" and
/// "written" meet.
fn words(row: &Row) -> std::collections::BTreeSet<String> {
    const COMMON: [&str; 24] = [
        "the", "and", "for", "with", "that", "this", "when", "from", "into", "not", "should", "must", "but", "are", "was", "has", "have",
        "its", "then", "than", "which", "will", "can", "instead",
    ];
    format!("{} {}", row.what, row.should)
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !COMMON.contains(w))
        .map(|w| {
            let w = ["ing", "ed", "es", "s"].iter().find_map(|end| w.strip_suffix(end).filter(|stem| stem.len() >= 3)).unwrap_or(w);
            w.chars().take(4).collect()
        })
        .collect()
}

/// Two reviewers raised the same problem: the same file, nearby lines (or a line only one names), and descriptions that
/// share enough meaningful words. A shared location alone is not agreement.
pub fn same_problem(a: &Row, b: &Row) -> bool {
    if a == b {
        return true;
    }
    if !same_file(&a.file, &b.file) || a.at.zip(b.at).is_some_and(|(x, y)| x.abs_diff(y) > NEAR) {
        return false;
    }
    let (x, y) = (words(a), words(b));
    let union = x.union(&y).count();
    union > 0 && x.intersection(&y).count() as f32 / union as f32 >= SAME_MEANING
}

fn median(values: &mut [u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[(values.len() - 1) / 2]
}

/// Combine reviewers, leaving out extreme totals when two remain.
pub fn score(by: &[(String, Verdict)]) -> Round {
    let totals: Vec<u32> = by.iter().map(|(_, v)| v.lines.iter().sum()).collect();
    let middle = median(&mut totals.clone());
    let mut excluded: Vec<bool> = totals.iter().map(|n| by.len() >= 3 && n.abs_diff(middle) > JESTER_GAP).collect();
    if excluded.iter().filter(|v| !**v).count() < 2 {
        excluded.fill(false);
    }
    let counted: Vec<_> = by.iter().zip(&excluded).filter_map(|(v, excluded)| (!excluded).then_some(v)).collect();
    let lines = std::array::from_fn(|i| median(&mut counted.iter().map(|(_, v)| v.lines[i]).collect::<Vec<_>>()));
    let mut findings: Vec<Finding> = Vec::new();
    for (name, verdict) in &counted {
        for row in &verdict.rows {
            if let Some(finding) = findings.iter_mut().find(|f| same_problem(&f.row, row)) {
                if !finding.reviewers.contains(name) {
                    finding.reviewers.push(name.clone());
                }
                // The agreed row carries evidence when either reviewer gave it, and the graver judgment.
                if finding.row.evidence.is_empty() {
                    finding.row.evidence = row.evidence.clone();
                }
                finding.row.visible |= row.visible;
                // A reviewer that gave no severity keeps the row blocking (`Row::blocks`); otherwise the graver one.
                finding.row.severity = match (&finding.row.severity, &row.severity) {
                    (Some(a), Some(b)) => Some(a.min(b).clone()),
                    _ => None,
                };
            } else {
                findings.push(Finding { row: row.clone(), reviewers: vec![name.clone()] });
            }
        }
    }
    // Two reviewers must agree; when only one answered (the other ran out of quota or failed), its findings with evidence
    // still go back to the agent instead of all becoming notes. A finding without evidence never sends the agent back.
    let needed = counted.len().clamp(1, 2);
    let (mut agreed, mut notes): (Vec<_>, Vec<_>) =
        findings.into_iter().partition(|f| f.reviewers.len() >= needed && !f.row.evidence.is_empty());
    agreed.truncate(MAX_ROWS);
    notes.truncate(MAX_ROWS - agreed.len());
    Round {
        call: None,
        lines,
        total: lines.iter().sum(),
        reviewers: counted.iter().map(|(name, _)| name.clone()).collect(),
        scores: counted.iter().map(|(name, verdict)| (name.clone(), verdict.lines)).collect(),
        omitted: vec![],
        jesters: by.iter().zip(excluded).filter_map(|((name, _), out)| out.then_some(name.clone())).collect(),
        agreed,
        notes,
    }
}

/// A round is kept unless its total or any line fell below the last kept round.
pub fn next(rounds: &[Round]) -> Next {
    let Some(newest) = rounds.last() else {
        return Next::Fix;
    };
    let mut kept: Option<&Round> = None;
    let mut flat = 0;
    let mut reverted = false;
    for round in rounds {
        reverted = kept.is_some_and(|best| round.total < best.total || round.lines.iter().zip(best.lines).any(|(a, b)| *a < b));
        if reverted {
            flat += 1;
            continue;
        }
        if kept.is_some_and(|best| round.total < best.total + NOISE) {
            flat += 1;
        } else {
            flat = 0;
        }
        kept = Some(round);
    }
    if reverted {
        return Next::Revert;
    }
    if newest.total >= TARGET {
        return Next::Stop(Reason::Target);
    }
    if !newest.agreed.iter().any(|f| f.row.blocks()) {
        return Next::Stop(Reason::NoAgreed);
    }
    if flat >= 2 {
        return Next::Stop(Reason::Flat);
    }
    if rounds.len() >= MAX_ROUNDS {
        return Next::Stop(Reason::Limit);
    }
    Next::Fix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(line: u32, file: &str) -> Row {
        Row {
            line,
            file: file.into(),
            at: None,
            what: "x".into(),
            should: "y".into(),
            evidence: "e".into(),
            severity: None,
            visible: false,
        }
    }
    fn said(file: &str, at: u32, what: &str, should: &str) -> Row {
        Row { at: Some(at), what: what.into(), should: should.into(), ..row(2, file) }
    }
    fn verdict(lines: [u32; 5], rows: Vec<Row>) -> Verdict {
        Verdict { lines, rows }
    }
    fn person(name: &str, lines: [u32; 5], rows: Vec<Row>) -> (String, Verdict) {
        (name.into(), verdict(lines, rows))
    }
    fn round(lines: [u32; 5]) -> Round {
        let mut r = score(&[person("a", lines, vec![row(1, "a")]), person("b", lines, vec![row(1, "a")])]);
        r.total = lines.iter().sum();
        r
    }

    #[test]
    fn parses_fences_and_explanations() {
        let v = parse("words ```json\n{\"lines\":[4,5,5,5,5],\"rows\":[{\"line\":1,\"file\":\"a\",\"what\":\"a \\\"}\\\"\",\"should\":\"b\"}]}\n``` tail").unwrap();
        assert_eq!(v.rows[0].what, "a \"}\"");
        assert_eq!(parse("{\"lines\":[4,5,5,5,5],\"rows\":[]}").unwrap().lines, [5; 5], "an unexplained point is given back");
        assert_eq!(parse("no object"), Err("error.review.no_json"));
        let kept = parse("{\"lines\":[1,4,5,5,5],\"rows\":[{\"line\":1,\"file\":\"a.rs\",\"what\":\"w\"},{\"line\":1,\"file\":\"a.rs\"}]}")
            .unwrap();
        assert_eq!((kept.lines, kept.rows.len()), ([1, 5, 5, 5, 5], 1), "the explained rows stay");
        assert_eq!(parse("{\"lines\":[6,5,5,5,5]}"), Err("error.review.bad_lines"));
    }

    #[test]
    fn median_and_jesters() {
        let a = person("a", [2; 5], vec![]);
        let b = person("b", [3; 5], vec![]);
        let c = person("c", [5; 5], vec![]);
        assert_eq!(score(&[a.clone(), b.clone()]).lines, [2; 5]);
        assert_eq!(score(&[a.clone(), b.clone(), c.clone()]).jesters, vec!["c"]);
        assert!(score(&[person("a", [0; 5], vec![]), b, c]).jesters.is_empty());
    }

    #[test]
    fn agreement_and_limit() {
        let many: Vec<_> = (0..25).map(|i| row(1, &i.to_string())).collect();
        let r = score(&[person("a", [4; 5], many.clone()), person("b", [4; 5], vec![row(2, "0"), row(1, "1")])]);
        assert_eq!(r.agreed.len(), 1);
        assert_eq!(r.agreed[0].row.file, "1");
        assert_eq!(r.agreed.len() + r.notes.len(), MAX_ROWS);
    }

    #[test]
    fn t515a_distinct_problems_in_one_file_never_become_consensus() {
        let first = Row { at: Some(10), what: "Save truncates the destination".into(), ..row(2, "store.rs") };
        let second = Row { at: Some(90), what: "Load drops queued attachments".into(), ..row(2, "store.rs") };
        let r = score(&[person("a", [4; 5], vec![first.clone()]), person("b", [4; 5], vec![second.clone()])]);
        assert!(r.agreed.is_empty(), "different problems are not agreement");
        assert_eq!(r.notes.iter().map(|f| f.row.clone()).collect::<Vec<_>>(), [first.clone(), second]);
        let other = Row { what: "Save publishes before syncing".into(), ..first.clone() };
        let r = score(&[person("a", [4; 5], vec![first.clone()]), person("b", [4; 5], vec![other])]);
        assert!(r.agreed.is_empty(), "a shared location alone is not agreement");
        let r = score(&[person("a", [4; 5], vec![first.clone(), first.clone()]), person("b", [4; 5], vec![first])]);
        assert_eq!(r.agreed.len(), 1);
        assert_eq!(r.agreed[0].reviewers, ["a", "b"]);
    }

    #[test]
    fn two_reviewers_describing_one_problem_in_their_own_words_agree() {
        let a = said("src/store.rs", 88, "save() writes without holding the lock", "take the lock before writing");
        let b =
            said("./src/store.rs", 90, "Writes in save are not locked, so two saves can interleave", "Hold the store lock while saving");
        let r = score(&[person("a", [4; 5], vec![a]), person("b", [4; 5], vec![b])]);
        assert_eq!(r.agreed.len(), 1, "{:?}", r.notes);
        let far = said("src/store.rs", 300, "save() writes without holding the lock", "take the lock before writing");
        let near = said("src/store.rs", 88, "save() writes without holding the lock", "take the lock before writing");
        assert!(!same_problem(&near, &far), "the same words far apart are two places");
    }

    #[test]
    fn one_answering_reviewer_still_sends_back_its_evidenced_findings() {
        let real = said("a.rs", 3, "divides by zero on an empty cart", "return 0 for an empty cart");
        let vague = Row { evidence: String::new(), ..said("b.rs", 9, "might be slow", "cache it") };
        let r = score(&[person("a", [3; 5], vec![real.clone(), vague.clone()])]);
        assert_eq!(r.agreed.iter().map(|f| &f.row).collect::<Vec<_>>(), [&real]);
        assert_eq!(r.notes.iter().map(|f| &f.row).collect::<Vec<_>>(), [&vague], "no evidence: a note, never a fix");
        assert_eq!(next(&[r]), Next::Fix, "before, a lone reviewer's findings all became notes and the agent was told to stop");
        let unproven = score(&[person("a", [4; 5], vec![vague.clone()]), person("b", [4; 5], vec![vague])]);
        assert!(unproven.agreed.is_empty(), "two reviewers agreeing without evidence is still no proof");
    }

    #[test]
    fn only_what_a_person_notices_or_p0_p1_sends_the_agent_back() {
        let polish = Row { severity: Some("P3".into()), ..said("a.css", 1, "spacing uneven", "even spacing") };
        let seen = Row { visible: true, ..polish.clone() };
        let grave = Row { severity: Some("P1".into()), ..said("a.rs", 1, "wrong total", "right total") };
        assert!(!polish.blocks() && seen.blocks() && grave.blocks() && row(1, "a").blocks());
        let r = score(&[person("a", [4; 5], vec![polish.clone()]), person("b", [4; 5], vec![polish])]);
        assert_eq!(r.agreed.len(), 1);
        assert_eq!(next(&[r]), Next::Stop(Reason::NoAgreed), "agreed polish nobody would notice is optional");
        for (a, b) in [(None, Some("P2")), (Some("P2"), None)] {
            let one = |s: Option<&str>| Row { severity: s.map(Into::into), ..said("a.css", 1, "spacing uneven", "even spacing") };
            let r = score(&[person("a", [4; 5], vec![one(a)]), person("b", [4; 5], vec![one(b)])]);
            assert!(r.agreed[0].row.blocks(), "a reviewer with no severity keeps the agreed row blocking");
        }
        let v = parse(r#"{"lines":[4,5,5,5,5],"rows":[{"line":1,"file":"a","what":"w","should":"s","evidence":" e ","severity":"p2","visible":true}]}"#).unwrap();
        assert_eq!((v.rows[0].evidence.as_str(), v.rows[0].severity.as_deref(), v.rows[0].visible), ("e", Some("P2"), true));
    }

    #[test]
    fn the_brief_marks_the_writers_account_as_a_claim() {
        let b = brief(Role::Skeptic, "Make login remember me", "All tests pass, it is safe.", "+ remember = true");
        let (request, claim, diff) = (b.find("Make login").unwrap(), b.find("All tests pass").unwrap(), b.find("+ remember").unwrap());
        assert!(request < claim && claim < diff);
        assert!(b[..claim].contains("check it, do not trust it"));
        assert!(b.contains("Trust nothing it claims") && !brief(Role::Defects, "r", "c", "d").contains("Trust nothing"));
    }

    #[test]
    fn next_decisions() {
        let base = round([3; 5]);
        assert_eq!(next(&[round([5; 5])]), Next::Stop(Reason::Target));
        assert_eq!(next(&[round([5, 5, 4, 4, 4]), round([4, 5, 5, 4, 4])]), Next::Revert);
        assert_eq!(next(&[base.clone(), round([2, 3, 3, 3, 3])]), Next::Revert);
        let mut no = base.clone();
        no.agreed.clear();
        assert_eq!(next(&[no]), Next::Stop(Reason::NoAgreed));
        assert_eq!(next(std::slice::from_ref(&base)), Next::Fix);
        assert_eq!(next(&[base.clone(), base.clone(), base.clone()]), Next::Stop(Reason::Flat));
        assert_eq!(next(&[base.clone(), round([4, 3, 3, 3, 3]), round([4, 4, 3, 3, 3])]), Next::Stop(Reason::Limit));
    }
}
