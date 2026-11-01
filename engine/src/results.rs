//! What a conversation really delivered, next to what it cost (rework W8, "results, not activity"). Pure: it reads the
//! stored messages and nothing else, so the sidebar, a project, the AI's closing words and the `faber` command line all
//! show the same numbers, and no second copy exists to drift when a conversation is rewound or edited.
//!
//! Delivered is what a person can point at: a file the agent changed, a check that passed, a commit, a pull request, a file
//! it made for the person, an answer to something the person wrote, a closing box that says the work is done. Spent is what
//! the CLIs reported: tokens, the price those tokens would have at the API, the time the agent worked. Quota is a share of a
//! window Faber learns per vendor over days (`learn.rs`) and is not recorded per conversation, so what is kept here is the
//! API-price worth per vendor, which moves with the quota.
//! A run of turns that delivered nothing (`Dry`) is what the watchdog (`engine/src/engine/dry_run.rs`) measures.

use crate::model::{Conversation, Message, Role, Status, Vendor};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// What was delivered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivered {
    /// Distinct files an editing tool changed in this conversation (a refused or failed edit changed nothing). A project's total
    /// adds its conversations' numbers, so a file two of them changed counts twice there.
    pub files: u32,
    pub edits: u32,
    /// Commands that run the project's tests, lint or build, and how they ended.
    pub checks_run: u32,
    pub checks_passed: u32,
    pub checks_failed: u32,
    pub commits: u32,
    /// Distinct pull requests a reply opened or pushed (summed over a project's conversations, like files).
    pub prs: u32,
    /// Files made for the person (Chat's file cards, visual answers), summed over a project like files.
    pub artifacts: u32,
    /// Replies to something the person wrote (the latest person-side message before the reply is not one Faber sent by itself).
    pub answers: u32,
    /// The last reply's closing box says the work is done.
    pub concluded: bool,
}

/// What it cost.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub turns: u32,
    /// Time the agent worked, summed over its replies, in milliseconds.
    pub work_ms: u64,
    /// From the first reply to the last, in milliseconds: what the person waited, idle time included.
    pub span_ms: u64,
    /// What the tokens would cost at the API's price, in US$.
    pub usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Turns that reported no price and no tokens: when above zero, `usd` is "at least" (of 3,652 stored replies, 1,037 had none).
    #[serde(default)]
    pub unpriced: u32,
    pub by_vendor: BTreeMap<Vendor, VendorCost>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorCost {
    pub turns: u32,
    pub work_ms: u64,
    pub usd: f64,
    pub output_tokens: u64,
}

/// The unbroken end of a conversation that delivered nothing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dry {
    /// Finished replies since the last one that delivered.
    pub turns: u32,
    /// Of those, the turns Faber sent by itself (Keep going, a check, a wake).
    pub by_itself: u32,
    /// When the first of them started, in milliseconds.
    pub since: Option<u64>,
    pub work_ms: u64,
    pub usd: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Results {
    pub delivered: Delivered,
    pub cost: Cost,
    /// When a reply last delivered something, in milliseconds.
    pub last_delivery: Option<u64>,
    pub dry: Dry,
}

/// A conversation the watchdog found delivering nothing (`engine/src/engine/dry_run.rs`): kept with the conversation so it
/// survives a restart, and cleared the moment a reply delivers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stuck {
    /// When Faber noticed, in milliseconds.
    pub noticed_at: u64,
    /// When the first turn with nothing delivered started.
    pub since: u64,
    pub turns: u32,
    #[serde(default)]
    pub wall_ms: u64,
    pub usd: f64,
    /// When Faber asked the AI to change approach or stop; asked once per window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked_at: Option<u64>,
    /// How many dry turns there were when it asked: the next window counts from here.
    #[serde(default)]
    pub ask_turns: u32,
}

/// What a project's conversations delivered and cost together, and each one's own numbers, most spent first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectResults {
    pub total: Results,
    pub conversations: Vec<ConversationResults>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationResults {
    pub id: String,
    pub title: String,
    pub results: Results,
}

impl Results {
    /// Adds another conversation's numbers: the sums of a project. `dry` is the one that spent most, and `concluded` holds
    /// only when both are.
    pub fn add(&mut self, other: &Results) {
        let (a, b) = (&mut self.delivered, &other.delivered);
        a.files += b.files;
        a.edits += b.edits;
        a.checks_run += b.checks_run;
        a.checks_passed += b.checks_passed;
        a.checks_failed += b.checks_failed;
        a.commits += b.commits;
        a.prs += b.prs;
        a.artifacts += b.artifacts;
        a.answers += b.answers;
        a.concluded = a.concluded && b.concluded;
        let (a, b) = (&mut self.cost, &other.cost);
        a.turns += b.turns;
        a.work_ms += b.work_ms;
        a.span_ms += b.span_ms;
        a.usd += b.usd;
        a.input_tokens += b.input_tokens;
        a.output_tokens += b.output_tokens;
        a.unpriced += b.unpriced;
        for (vendor, v) in &b.by_vendor {
            let mine = a.by_vendor.entry(*vendor).or_default();
            mine.turns += v.turns;
            mine.work_ms += v.work_ms;
            mine.usd += v.usd;
            mine.output_tokens += v.output_tokens;
        }
        self.last_delivery = self.last_delivery.max(other.last_delivery);
        // The dry end shown is the one that spent most; among equals, the longest.
        if (other.dry.usd, other.dry.turns) > (self.dry.usd, self.dry.turns) {
            self.dry = other.dry.clone();
        }
    }
}

/// A project's numbers from its conversations' (id, title, results), most spent first. A conversation with no reply is left out.
pub fn project(rows: impl IntoIterator<Item = (String, String, Results)>) -> ProjectResults {
    let mut out = ProjectResults::default();
    let mut first = true;
    for (id, title, results) in rows {
        if results.cost.turns == 0 {
            continue;
        }
        if std::mem::take(&mut first) {
            out.total.delivered.concluded = true;
        }
        out.total.add(&results);
        out.conversations.push(ConversationResults { id, title, results });
    }
    out.conversations.sort_by(|a, b| b.results.cost.usd.total_cmp(&a.results.cost.usd));
    out
}

/// What one reply delivered. A turn Faber sent by itself (`after`, the person-side message before it, carries a `wake`) cannot
/// be an answer. A reply still streaming counts what it already did, never an answer or a conclusion.
struct Reply<'a> {
    files: BTreeSet<&'a str>,
    edits: u32,
    checks: (u32, u32, u32),
    commits: u32,
    pr: Option<&'a str>,
    artifacts: BTreeSet<&'a str>,
    answer: bool,
    concluded: bool,
}

impl Reply<'_> {
    fn of<'a>(m: &'a Message, after: Option<&Message>) -> Reply<'a> {
        let finished = m.status != Some(Status::Streaming);
        let mut r = Reply {
            files: BTreeSet::new(),
            edits: 0,
            checks: (0, 0, 0),
            commits: m.commits.len() as u32,
            pr: m.follow.as_ref().map(|f| f.url.as_str()),
            artifacts: m.files.iter().flatten().map(|f| f.path.as_str()).chain(m.visual_files.iter().map(|f| f.path.as_str())).collect(),
            answer: m.status == Some(Status::Done) && !m.text.trim().is_empty() && after.is_some_and(|a| a.wake.is_none()),
            concluded: finished && m.wrap_up.as_ref().is_some_and(|w| w.done),
        };
        for t in m.tools.iter().flatten().filter(|t| !t.declined && !t.interrupted) {
            let paths = crate::nudge::edited(t);
            // An edit counts once the CLI said it worked: a call with no outcome changed nothing anyone saw.
            if t.ok == Some(true) && !paths.is_empty() {
                r.edits += 1;
                r.files.extend(paths);
            }
            if t.check {
                r.checks.0 += 1;
                match t.ok {
                    Some(true) => r.checks.1 += 1,
                    Some(false) => r.checks.2 += 1,
                    None => {}
                }
            }
        }
        r
    }

    /// Something a person can point at. A failed check, a plan that did not move and a closing box that is not done are not.
    fn delivered(&self) -> bool {
        self.edits > 0
            || self.checks.1 > 0
            || self.commits > 0
            || self.pr.is_some()
            || !self.artifacts.is_empty()
            || self.answer
            || self.concluded
    }
}

/// Everything a conversation's stored messages say about what it delivered and what that cost. Messages a rewind took away are
/// not here, and neither is what they cost.
pub fn tally(c: &Conversation) -> Results {
    let mut r = Results::default();
    let (mut files, mut prs, mut artifacts) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let (mut first, mut last_end) = (None::<u64>, 0u64);
    let mut person_side: Option<&Message> = None;
    for m in &c.messages {
        if m.role == Role::User {
            person_side = Some(m);
            continue;
        }
        let running = m.status == Some(Status::Streaming);
        let reply = Reply::of(m, person_side);
        let took = m.took.unwrap_or(0);
        let usd = m.cost_usd.unwrap_or(0.0);
        r.cost.work_ms += took;
        r.cost.usd += usd;
        r.cost.input_tokens += m.input_tokens.unwrap_or(0);
        r.cost.output_tokens += m.output_tokens.unwrap_or(0);
        if !running {
            r.cost.turns += 1;
            // A turn that reported neither a price nor tokens (cut short, or a CLI that said nothing): the total is then "at least".
            r.cost.unpriced += u32::from(m.cost_usd.is_none() && m.input_tokens.is_none() && m.output_tokens.is_none());
            first = Some(first.map_or(m.at, |f| f.min(m.at)));
            last_end = last_end.max(m.at.saturating_add(took));
        }
        if let Some(vendor) = m.vendor {
            let v = r.cost.by_vendor.entry(vendor).or_default();
            v.turns += u32::from(!running);
            v.work_ms += took;
            v.usd += usd;
            v.output_tokens += m.output_tokens.unwrap_or(0);
        }
        let d = &mut r.delivered;
        d.edits += reply.edits;
        d.checks_run += reply.checks.0;
        d.checks_passed += reply.checks.1;
        d.checks_failed += reply.checks.2;
        d.commits += reply.commits;
        d.answers += u32::from(reply.answer);
        files.extend(reply.files.iter().copied());
        artifacts.extend(reply.artifacts.iter().copied());
        prs.extend(reply.pr);
        if reply.delivered() {
            r.last_delivery = r.last_delivery.max(Some(m.at));
            r.dry = Dry::default();
        } else if !running {
            r.dry.turns += 1;
            r.dry.by_itself += u32::from(person_side.is_some_and(|p| p.wake.is_some()));
            r.dry.since = Some(r.dry.since.map_or(m.at, |s| s.min(m.at)));
            r.dry.work_ms += took;
            r.dry.usd += usd;
        }
    }
    r.delivered.files = files.len() as u32;
    r.delivered.prs = prs.len() as u32;
    r.delivered.artifacts = artifacts.len() as u32;
    r.delivered.concluded = c
        .messages
        .iter()
        .rev()
        .find(|m| m.role == Role::Assistant && m.status != Some(Status::Streaming))
        .and_then(|m| m.wrap_up.as_ref())
        .is_some_and(|w| w.done);
    r.cost.span_ms = first.map_or(0, |f| last_end.saturating_sub(f));
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolUse;
    use serde_json::json;

    fn conv(messages: Vec<Message>) -> Conversation {
        let mut c: Conversation = serde_json::from_value(json!({
            "id": "c", "title": "t", "area": "project", "createdAt": 0, "updatedAt": 0, "messages": []
        }))
        .unwrap();
        c.messages = messages;
        c
    }

    fn person(at: u64) -> Message {
        Message::new(format!("u{at}"), Role::User, "Do it".into(), at)
    }

    fn faber(at: u64) -> Message {
        let mut m = person(at);
        m.wake = Some(crate::model::Wake::of("wake.keepGoing"));
        m
    }

    fn reply(at: u64, text: &str) -> Message {
        let mut m = Message::new(format!("r{at}"), Role::Assistant, text.into(), at);
        m.status = Some(Status::Done);
        m.vendor = Some(Vendor::Claude);
        m.took = Some(1000);
        m.cost_usd = Some(0.5);
        m.input_tokens = Some(100);
        m.output_tokens = Some(10);
        m
    }

    fn tool(id: &str, name: &str, input: serde_json::Value, ok: Option<bool>) -> ToolUse {
        serde_json::from_value(json!({ "id": id, "name": name, "input": input, "ok": ok })).unwrap()
    }

    fn with_tools(mut m: Message, tools: Vec<ToolUse>) -> Message {
        m.tools = Some(tools);
        m
    }

    #[test]
    fn edits_count_distinct_files_and_only_a_confirmed_edit_changed_something() {
        let tools = vec![
            tool("a", "Edit", json!({ "file_path": "src/a.rs" }), Some(true)),
            tool("b", "Edit", json!({ "file_path": "src/a.rs" }), Some(true)),
            tool("c", "Write", json!({ "file_path": "src/b.rs" }), Some(true)),
            tool("d", "Edit", json!({ "file_path": "src/c.rs" }), Some(false)),
            tool("e", "Edit", json!({ "file_path": "src/d.rs" }), None),
        ];
        let r = tally(&conv(vec![person(1), with_tools(reply(2, ""), tools)]));
        assert_eq!((r.delivered.files, r.delivered.edits), (2, 3));
    }

    #[test]
    fn checks_commits_pull_requests_and_artifacts_are_counted_once_each() {
        let mut passed = tool("t1", "Bash", json!({ "command": "cargo test" }), Some(true));
        passed.check = true;
        let mut failed = tool("t2", "Bash", json!({ "command": "npm run lint" }), Some(false));
        failed.check = true;
        let mut declined = tool("t3", "Bash", json!({ "command": "npm test" }), Some(false));
        declined.check = true;
        declined.declined = true;
        let mut m = with_tools(reply(2, ""), vec![passed, failed, declined]);
        m.commits = vec!["t1".into(), "t9".into()];
        m.files = Some(vec![crate::model::FileInfo { path: "out/report.pdf".into(), size: 3 }]);
        let r = tally(&conv(vec![person(1), m]));
        let d = r.delivered;
        assert_eq!((d.checks_run, d.checks_passed, d.checks_failed), (2, 1, 1), "a refused check never ran");
        assert_eq!((d.commits, d.artifacts), (2, 1));
    }

    #[test]
    fn only_a_reply_to_the_person_is_an_answer_and_a_closing_box_with_done_concludes() {
        let mut done = reply(6, "All finished.");
        done.wrap_up = Some(crate::model::WrapUp { recap: "ok".into(), ask: None, actions: vec![], done: true, answered: None });
        let c = conv(vec![person(1), reply(2, "Here is why."), faber(3), reply(4, "I looked again."), faber(5), done]);
        let r = tally(&c);
        assert_eq!(r.delivered.answers, 1, "turns Faber sent by itself are not answers");
        assert!(r.delivered.concluded);
        let c = conv(vec![person(1), reply(2, "Here is why."), faber(3), reply(4, "I looked again.")]);
        assert!(!tally(&c).delivered.concluded);
    }

    #[test]
    fn cost_adds_up_per_vendor_and_the_span_is_first_to_last() {
        let mut codex = reply(9_000, "x");
        codex.vendor = Some(Vendor::Codex);
        codex.cost_usd = Some(1.5);
        let r = tally(&conv(vec![person(1000), reply(2000, "a"), person(5000), codex]));
        assert_eq!(r.cost.turns, 2);
        assert_eq!(r.cost.work_ms, 2000);
        assert_eq!(r.cost.span_ms, 8000, "from the first reply's start to the last one's end");
        assert!((r.cost.usd - 2.0).abs() < 1e-9);
        assert_eq!((r.cost.input_tokens, r.cost.output_tokens), (200, 20));
        assert!((r.cost.by_vendor[&Vendor::Codex].usd - 1.5).abs() < 1e-9);
        assert_eq!(r.cost.by_vendor[&Vendor::Claude].turns, 1);
    }

    #[test]
    fn the_dry_end_counts_turns_since_the_last_delivery_and_ignores_a_reply_still_streaming() {
        let edit = vec![tool("a", "Edit", json!({ "file_path": "a.rs" }), Some(true))];
        let mut running = reply(9, "");
        running.status = Some(Status::Streaming);
        let c = conv(vec![
            person(1),
            with_tools(reply(2, "done a"), edit),
            faber(3),
            reply(4, "waiting"),
            faber(5),
            reply(6, "still waiting"),
            faber(7),
            running,
        ]);
        let r = tally(&c);
        assert_eq!((r.dry.turns, r.dry.by_itself), (2, 2));
        assert_eq!(r.dry.since, Some(4));
        assert_eq!(r.last_delivery, Some(2));
        assert!((r.dry.usd - 1.0).abs() < 1e-9);
        let delivering = conv(vec![person(1), reply(2, "answer"), faber(3), reply(4, "waiting")]);
        assert_eq!(tally(&delivering).dry.turns, 1, "the answer to the person broke the dry run");
    }

    #[test]
    fn a_closing_box_that_is_not_done_delivers_nothing() {
        let mut open = reply(4, "");
        open.wrap_up = Some(crate::model::WrapUp { recap: "not yet".into(), ask: None, actions: vec![], done: false, answered: None });
        let c = conv(vec![faber(3), open]);
        assert_eq!(tally(&c).dry.turns, 1);
        assert!(!tally(&c).delivered.concluded);
    }

    #[test]
    fn a_reply_still_streaming_counts_what_it_cost_and_did_but_is_not_a_turn_an_answer_or_a_conclusion() {
        let mut running = with_tools(reply(2, "so far"), vec![tool("a", "Edit", json!({ "file_path": "a.rs" }), Some(true))]);
        running.status = Some(Status::Streaming);
        running.wrap_up = Some(crate::model::WrapUp { recap: "ok".into(), ask: None, actions: vec![], done: true, answered: None });
        let r = tally(&conv(vec![person(1), running]));
        assert_eq!((r.cost.turns, r.delivered.files, r.delivered.answers), (0, 1, 0));
        assert!((r.cost.usd - 0.5).abs() < 1e-9, "its spend is already spent");
        assert!(!r.delivered.concluded);
        assert_eq!(r.dry.turns, 0, "it delivered, so nothing is dry");
    }

    #[test]
    fn a_reply_after_an_empty_one_still_answers_the_person_and_pull_requests_count_once() {
        let mut pushed = reply(4, "Pushed.");
        pushed.follow = Some(
            serde_json::from_value(
                json!({ "repo": "a/b", "number": 7, "url": "https://github.com/a/b/pull/7", "state": "pending", "since": 1 }),
            )
            .unwrap(),
        );
        let mut again = pushed.clone();
        again.id = "r5".into();
        let r = tally(&conv(vec![person(1), reply(2, ""), pushed, again]));
        assert_eq!(r.delivered.answers, 2, "both replies follow the person's message");
        assert_eq!(r.delivered.prs, 1);
    }

    #[test]
    fn a_turn_with_neither_price_nor_tokens_marks_the_total_as_at_least() {
        let mut silent = reply(2, "x");
        silent.cost_usd = None;
        silent.input_tokens = None;
        silent.output_tokens = None;
        let r = tally(&conv(vec![person(1), silent, reply(4, "y")]));
        assert_eq!(r.cost.unpriced, 1);
    }

    #[test]
    fn times_out_of_order_or_huge_do_not_panic_and_the_span_covers_them() {
        let mut late = reply(u64::MAX, "z");
        late.took = Some(5);
        let r = tally(&conv(vec![person(1), reply(10_000, "a"), person(2), reply(1_000, "b"), late]));
        assert_eq!(r.last_delivery, Some(u64::MAX));
        let r = tally(&conv(vec![person(1), reply(10_000, "a"), person(2), reply(1_000, "b")]));
        assert_eq!(r.cost.span_ms, 10_000, "from the earliest start to the latest end");
    }

    #[test]
    fn nothing_stored_is_zero_everywhere() {
        let r = tally(&conv(vec![]));
        assert_eq!(r, Results::default());
    }

    #[test]
    fn a_project_adds_its_conversations_and_lists_the_most_spent_first() {
        let edit = vec![tool("a", "Edit", json!({ "file_path": "a.rs" }), Some(true))];
        let small = tally(&conv(vec![person(1), with_tools(reply(2, ""), edit)]));
        let mut big_reply = reply(4, "waiting");
        big_reply.cost_usd = Some(3.0);
        let big = tally(&conv(vec![faber(3), big_reply]));
        let none = Results::default();
        let p = project([("s".into(), "Small".into(), small), ("b".into(), "Big".into(), big.clone()), ("n".into(), "Empty".into(), none)]);
        assert_eq!(p.conversations.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["b", "s"], "an empty conversation is left out");
        assert_eq!((p.total.delivered.files, p.total.cost.turns), (1, 2));
        assert!((p.total.cost.usd - 3.5).abs() < 1e-9);
        assert_eq!(p.total.dry, big.dry, "the dry end shown is the one that spent most");
    }
}
