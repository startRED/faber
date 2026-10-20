//! What Faber keeps on disk and shows on screen. Field names match the JSON files, so a conversation saved by any
//! version of Faber opens in any other.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Vendor {
    Claude,
    Codex,
}

impl Vendor {
    pub const ALL: [Vendor; 2] = [Vendor::Claude, Vendor::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Vendor::Claude => "claude",
            Vendor::Codex => "codex",
        }
    }
}

/// A command or skill advertised by the running CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Command {
    pub name: String,
    pub skill: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCommand {
    pub vendor: Vendor,
    #[serde(flatten)]
    pub command: Command,
}

/// Chat uses a chosen folder or a private scratch folder; a project uses the folder the user opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Area {
    Chat,
    Project,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Streaming,
    Done,
    Stopped,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub media_type: String,
    pub base64: String,
}

fn glance() -> String {
    "glance".into()
}

fn other() -> String {
    "other".into()
}

/// Parked items saved before `kind` existed hold it empty; the screen has no text for that, so it reads as `other`.
fn kind_or_other<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let kind = String::deserialize(d)?;
    Ok(if kind.is_empty() { other() } else { kind })
}

/// Dispatch intent for a quota error that still offers explicit recovery after restart. Consent is never stored here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recovery {
    pub pick: crate::router::Pick,
    pub text: String,
    pub by_hand: bool,
    pub retried: bool,
    /// The router's `check` for the request; files from before it was kept read as `glance`.
    #[serde(default = "glance")]
    pub check: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// The user (or the CLI's own approval) refused the tool, so it never ran; `ok` is false but nothing failed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub declined: bool,
    /// The subagent that ran this tool: the id of the agent tool call that started it. None for the agent's own tools.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// On a subagent's call while the reply runs: waiting for the person, or seeming stuck (`stuck.rs`). Never read back
    /// from disk, so a sign can outlive neither its reply nor Faber.
    #[serde(skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub sign: Option<crate::stuck::Sign>,
    /// On a subagent's call once it ended: what it changed and checked (`facts.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facts: Option<crate::engine::facts::Facts>,
    /// A command that runs the project's tests, lint or build (`nudge.rs`), marked when the call starts so the screen counts
    /// the checks a reply ran without a second list of what a check is (R161 step 9).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub check: bool,
    /// When the engine saw the call start, in Unix milliseconds, so the screen can say how long a long step has run without
    /// a clock of its own (R161 step 9). Absent on calls recorded before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<u64>,
    /// On a helper's call: what the person wrote to the helper while it worked (R161 step 5), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub told: Vec<Told>,
}

/// A message the person wrote to a helper at work. `taken` is empty while it is on its way, true once the helper took it
/// in, and false when the helper had finished first, so it never read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Told {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken: Option<bool>,
}

/// A helper the agent sent to the background (Claude Code's `Agent` call with `run_in_background`): its call returns when it
/// starts, and it finishes later.
pub fn runs_in_background(tool: &ToolUse) -> bool {
    tool.parent.is_none() && matches!(tool.name.as_str(), "Agent" | "Task") && tool.input["run_in_background"] == true
}

/// A request the CLI denied because nobody could answer it (full access runs with no prompts): it was not done, and it waits
/// here until the person says what to do. `tool_use_id` names it, with the turn. Only safe metadata is kept, never the request
/// itself: a command line or a header in it could carry a token (see `engine::parked::describe`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parked {
    pub tool_use_id: String,
    pub tool: String,
    #[serde(default = "other", deserialize_with = "kind_or_other")]
    /// "command", "file", "page" or "other".
    pub kind: String,
    #[serde(default)]
    /// A file path, or the host of a page; empty for a command and for any other tool. Cut short.
    pub label: String,
    #[serde(default)]
    /// When it was held back, in milliseconds.
    pub at: u64,
}

/// A permission request waiting for the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    pub request_id: String,
    pub tool: String,
    pub input: Value,
    /// The grants epoch current when the card was made. An "Always allow" on it is remembered only if the epoch is still
    /// that one when the answer settles. Never shown or saved.
    #[serde(skip)]
    pub epoch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub text: String,
    /// UTF-16 offsets where a new CLI text block starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_breaks: Option<Vec<usize>>,
    /// Start of an explicit final answer, or end of text preceding the last tool call when the CLI has no phases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_end: Option<usize>,
    /// Milliseconds since the Unix epoch.
    pub at: u64,
    /// On a reply that ended well: how long it took from its start (`at`), in milliseconds. Absent on older replies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub took: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<Image>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<Vendor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolUse>>,
    /// Oldest first; a model can ask for several tools at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<Permission>>,
    /// Chat only: files the answer created or changed in the conversation's workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileInfo>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Prompt tokens of the main response read from the CLI's cache, and written to it (the part with the 1-hour lifetime
    /// separately). Codex reports one cache-write kind and no separate one-hour counter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_1h_tokens: Option<u64>,
    /// When the prompt cache this turn wrote goes cold, in milliseconds. Claude only: Codex names no lifetime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_warm_until: Option<u64>,
    /// Requests the CLI denied in this turn for want of anyone to answer them (see `Parked`), oldest first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parked: Option<Vec<Parked>>,
    /// How many more were denied in the turn beyond the few kept (see `parked::MAX_PARKED`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parked_more: Option<u32>,
    /// True when the token numbers cover only part of the turn (sub-model work, or no usage was seen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<bool>,
    /// Why the quota changed the model Auto chose, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pace: Option<PaceNote>,
    /// The permission mode that really ran this turn, as an i18n key, only when it is not the one chosen (`mode.fallback.auto`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// A plain-language note under the reply (an i18n key): `note.historyResent` when the earlier conversation was sent again as text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Coverage of the file baseline immediately before this user message; unavailable history never blocks a turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewind: Option<crate::rewind::PointInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<Vendor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
    /// The next rung, offered when the turn's own tools show it stopped early (see `nudge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nudge: Option<crate::nudge::Nudge>,
    /// The pull request this reply opened or pushed, followed until its checks pass or it closes (`crate::follow`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow: Option<Follow>,
    /// A message Faber sent into the conversation by itself, and why: drawn as a quiet line, not as the person's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wake: Option<Wake>,
    /// On a reply: "keep going" stopped after it, and why (a `reason.keepGoing.*` key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halt: Option<String>,
    /// On a reply: questions Faber answered with their first choice because "keep going" was on and nobody answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_answers: Option<Vec<AutoAnswer>>,
    /// On the newest reply of a conversation "keep going" has run in for a while: what it did while the person was away.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub away: Option<Away>,
    /// On a project reply that changed files: Faber's own run of the fast checks once it ended (R146, "Faber decides done").
    /// A run that was going on when Faber closed is not read back: nothing would end it.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "settled_checks")]
    pub checks: Option<Checks>,
    /// On a reply: when its words, thinking or tools last changed, in milliseconds. The screen's "updated 3 min ago" counts from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_at: Option<u64>,
    /// On a reply: when the agent said all it will say (its last message ended with `end_turn` and no tool call was open, or
    /// the turn ended), in milliseconds. A reply that still streams with this set is finished for the person while the CLI
    /// closes its turn; more words or tools clear it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    /// On a reply: what the agent said it is doing now, in a few words (`crate::progress::status`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<String>,
    /// On a reply: the closing box of the task it ended (`crate::progress::wrap_up`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap_up: Option<WrapUp>,
}

/// Faber's checks of a reply (`engine/src/engine/verifying.rs`): running, passed, or what failed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checks {
    pub state: ChecksState,
    /// How many checks passed or were unchanged since they passed.
    #[serde(default)]
    pub passed: u32,
    /// The checks that failed or did not finish, in the order the project declares them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<FailedCheck>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChecksState {
    Running,
    Passed,
    /// Failed, and Faber asked the agent to fix it.
    Failed,
    /// Still failed after the automatic fixes: the person decides.
    NeedsYou,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedCheck {
    pub command: String,
    /// Its first short failures (R155), for the screen; the agent reads them all.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<String>,
}

fn settled_checks<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Checks>, D::Error> {
    Ok(Option::<Checks>::deserialize(d)?.filter(|c| c.state != ChecksState::Running))
}

/// A pull request a project conversation follows (R200): one line under the reply that opened or last pushed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Follow {
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
    pub url: String,
    pub state: FollowState,
    /// The checks that failed on the commit last read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<String>,
    /// The commit last read; a new one is the agent's fix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// What Faber already woke the agent for (the commit and why), so the same failure never spends a second turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub woke: Option<String>,
    /// The checks failed while the quota was tight: the person is told instead of the agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub held: bool,
    /// The person took the conversation back after it was archived, or wrote to it while Faber read the end of the pull
    /// request: Faber does not put it away for this pull request (`engine/src/engine/auto_archive.rs`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kept: bool,
    /// Why following ended early, as an i18n key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// When something last changed, in milliseconds.
    pub since: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FollowState {
    Pending,
    Failing,
    Conflict,
    Passing,
    Merged,
    Closed,
    /// Ended by the person, by a problem, or after long quiet.
    Stopped,
}

impl FollowState {
    /// Whether Faber still looks at the pull request.
    pub fn open(self) -> bool {
        matches!(self, FollowState::Pending | FollowState::Failing | FollowState::Conflict)
    }
}

/// Why Faber sent a message by itself (`engine/src/engine/by_itself.rs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wake {
    /// An i18n key (`wake.keepGoing`, `wake.checksFailed`, `wake.conflict`, `wake.asked`, `wake.compacted`).
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<String>,
}

impl Wake {
    pub fn of(why: &str) -> Wake {
        Wake { why: why.into(), pr: None, checks: vec![] }
    }
}

/// "Keep going" is on in a conversation (R199): Faber sends the next turn by itself whenever one ends, until a stop.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepGoing {
    /// Turns Faber sent by itself since it was turned on.
    pub turns: u32,
    /// Turns that did real work (not a quick answer with no tool); only these count toward the cap.
    #[serde(default)]
    pub work: u32,
    /// Failures in a row (an error, or an AI Faber could not reach); a turn that ends well sets it to zero.
    pub failed: u32,
    /// Every AI it may use is out of quota: it goes on by itself at this time (Unix seconds), and the screen says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<f64>,
    /// Turns in a row that ended in under a minute with no tool: Faber lets a longer rest pass before the next.
    #[serde(default)]
    pub idle: u32,
    /// The error of the last failure, so one that cannot mend by waiting stops it when it comes again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// The agent said nothing is left and Faber asked it once to check against the original request: a second stop is taken.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub checked: bool,
    /// When it was turned on, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub since: u64,
    /// Times Faber moved the work to another AI because the first had no quota left.
    #[serde(default)]
    pub switches: u32,
    /// Questions Faber answered with the first choice because the person was away.
    #[serde(default)]
    pub answered: u32,
    /// Moves to another AI inside the turn that runs (a plan limit hit again after a move); zero at the end of each turn.
    #[serde(default, skip)]
    pub moves: u8,
    /// Faber waits to send the next turn, and why. Saved, so a restart waits out the rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<KeepWait>,
    /// The reply whose closing question waited its ten minutes: only then Faber answers it for the person (task 017).
    #[serde(default, skip)]
    pub asked_on: Option<String>,
    /// The reply whose failure was already counted, so a reply reported twice counts once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_on: Option<String>,
}

/// Why "keep going" is waiting to send the next turn, and until when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepWait {
    /// `retry` (after an error), `rest` (the last turn only waited), `busy` (Faber was busy with an update), `start` (Faber
    /// just opened) or `question` (the closing box asks the person, task 017). A wait for quota is `wait_until` instead.
    pub why: String,
    /// Seconds since the Unix epoch.
    pub until: f64,
}

/// What "keep going" did since it was turned on, told on the newest reply once the person has been away a while.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Away {
    /// How long it has been on, in whole minutes.
    pub minutes: u32,
    /// Turns that did real work.
    pub turns: u32,
    /// Moves of the work to another AI.
    pub switches: u32,
    /// Questions Faber answered with the first choice.
    pub answered: u32,
}

/// How far a conversation that goes on by itself has come, for its row in the running list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepProgress {
    /// Turns that did real work.
    pub turns: u32,
    /// When it was turned on, in milliseconds since the Unix epoch; 0 when not known.
    pub since: u64,
    /// The last line the agent wrote, without its markup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
}

/// A project agent's plan for the request (`crate::progress`): a checklist that belongs to the conversation, so it carries
/// through Keep going's turns and across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub steps: Vec<PlanStep>,
    /// When the agent last sent it, in milliseconds.
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub text: String,
    pub state: StepState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepState {
    Pending,
    Active,
    Done,
}

impl Plan {
    /// How far it has come: steps done, and steps in all.
    pub fn count(&self) -> PlanCount {
        PlanCount { done: self.steps.iter().filter(|s| s.state == StepState::Done).count() as u32, total: self.steps.len() as u32 }
    }
}

/// "3 of 7": a plan's steps done and in all, for the running column and the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanCount {
    pub done: u32,
    pub total: u32,
}

/// The closing box an agent puts at the end of a reply when a task ends (`crate::progress::wrap_up`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrapUp {
    /// What is now true, then what is not done or not verified.
    pub recap: String,
    /// One question whose answer is the person's to give, with its replies (the recommendation first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    /// The person's whole request is finished and verified: Keep going stops on it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub done: bool,
    /// The reply Faber gave for the person while Keep going ran and nobody answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<String>,
}

/// A question the agent asked that Faber answered with the first choice while the person was away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoAnswer {
    pub question: String,
    pub answer: String,
    /// The question's other choices, so the person can change the answer later (task 018's "Change").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub others: Vec<String>,
}

impl Message {
    /// What the reply said to the person: its text, then its closing box's recap and question, which the agent does not
    /// repeat in words. Another AI taking over and a parent conversation read this, not the text alone.
    pub fn said(&self) -> String {
        let Some(w) = &self.wrap_up else { return self.text.clone() };
        let ask = w
            .ask
            .as_ref()
            .map(|q| {
                format!(
                    "
{q} ({})",
                    w.actions.join(" / ")
                )
            })
            .unwrap_or_default();
        format!(
            "{}

{}{ask}",
            self.text, w.recap
        )
        .trim()
        .to_string()
    }

    pub fn new(id: String, role: Role, text: String, at: u64) -> Self {
        Message {
            id,
            role,
            text,
            text_breaks: None,
            activity_end: None,
            at,
            took: None,
            images: None,
            attachments: None,
            vendor: None,
            model: None,
            effort: None,
            reason: None,
            thinking: None,
            tools: None,
            permissions: None,
            files: None,
            status: None,
            error: None,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            cache_read_tokens: None,
            cache_write_tokens: None,
            cache_write_1h_tokens: None,
            cache_warm_until: None,
            parked: None,
            parked_more: None,
            incomplete: None,
            pace: None,
            wait_until: None,
            moved_from: None,
            mode: None,
            note: None,
            rewind: None,
            recovery: None,
            nudge: None,
            follow: None,
            wake: None,
            halt: None,
            auto_answers: None,
            away: None,
            checks: None,
            moved_at: None,
            finished_at: None,
            now: None,
            wrap_up: None,
        }
    }
}

/// The versions of one message of the open conversation (parity T7): the conversations that took another way at it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Versions {
    /// The message in the open conversation.
    pub message_id: String,
    /// Which of `list` it is.
    pub at: usize,
    /// Every version, in the order they were written.
    pub list: Vec<VersionAt>,
}

/// One version: the conversation that shows it and the message there.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionAt {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOf {
    pub conversation_id: String,
    pub message_id: String,
}

/// A message written while a turn was running. It waits to be sent as the next turn, or, on a CLI that can take a message
/// into a running turn, it is handed to that turn and leaves the queue once the CLI has taken it in. `text` is what the
/// person wrote; the note about attached files is added when it goes to the CLI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedTurn {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<Image>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<String>>,
    pub choice: crate::router::Choice,
    pub at: u64,
    /// Handed to the running turn and waiting for the CLI to take it in: it can no longer be taken back. Nothing is on
    /// its way after a restart, so loading a conversation clears it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sending: bool,
    /// Faber queued it by itself: the message it becomes carries this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wake: Option<Wake>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    /// The commands advertised by each of this conversation's CLIs. Never saved.
    #[serde(skip)]
    pub commands: BTreeMap<Vendor, Vec<Command>>,
    pub id: String,
    pub title: String,
    /// The choice for the next message, retained independently in each conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<crate::router::Choice>,
    pub area: Area,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Chat's canonical folder, chosen before its first message. None uses Faber-owned scratch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_folder: Option<String>,
    /// A project conversation's own copy, made at its first message (R201). None works in the project folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy: Option<crate::project::copy::WorkCopy>,
    /// One CLI session per vendor, so switching models keeps each vendor's own history and cache.
    #[serde(default)]
    pub sessions: BTreeMap<Vendor, String>,
    /// A session id whose first turn was accepted but has not shown any output yet. It is not resumable like `sessions` (the CLI may
    /// not have saved it), but a restart after a crash resumes it instead of starting a session that knows nothing of the turn.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tentative: BTreeMap<Vendor, String>,
    /// Older sessions restart once with visible history to receive the host command tool.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub command_gate: bool,
    /// The cumulative cost each CLI session last reported, so a turn costs the difference (`usage::settle`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cost_base: BTreeMap<Vendor, crate::usage::CostBase>,
    /// Tools the user chose to always allow in this conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_always: Option<Vec<String>>,
    /// The grants epoch (`Settings::grants_epoch`) that was current when each tool in `allow_always` was allowed. A grant
    /// stands only while the settings hold that same epoch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub allow_always_epochs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_of: Option<BranchOf>,
    /// The conversation whose agent started this one with Faber's conversation tool (`crate::sibling`). Such a conversation
    /// cannot start conversations of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<String>,
    /// What the user asked to be called when this conversation started. Kept here so the CLI gets the same instructions every
    /// time the conversation resumes, whatever Settings says by then; a conversation saved before this field has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    /// Stable persona version used when this conversation began; legacy sessions keep None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona_version: Option<u32>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued: Vec<QueuedTurn>,
    /// "Keep going" is on (`engine/src/engine/keep_going.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_going: Option<KeepGoing>,
    /// When it was archived, by the person or by its own agent (`engine/src/engine/archive.rs`): it leaves the sidebar
    /// until the person brings it back or writes to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<u64>,
    /// When the person pinned it to the top of its sidebar list. Archiving removes the pin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<u64>,
    /// Its last run ended and the person has not opened it since (`engine/src/engine/ended.rs`): the running column's done card
    /// and the sidebar's dot read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<Ended>,
    /// How full the context of the conversation's own agent is, as its CLI last said (`engine/src/engine/compact.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextFill>,
    /// The agent's plan for the request (`crate::progress::plan`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    pub messages: Vec<Message>,
    /// Saved-message continuation after rewind; consumed by the next reply, never a native CLI resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewind_notice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewind_summary: Option<String>,
    /// What its CLI last reported, each on its own (a report may name only one): the prompt's size and the model's window,
    /// before the point where Faber's CLI flags make it compact. Never saved.
    #[serde(skip)]
    pub reported: (Option<u64>, Option<u64>),
    /// The context is being compacted: the conversation is at work meanwhile, like during Faber's checks. Never saved.
    #[serde(skip)]
    pub compacting: bool,
    /// Faber is asking git whether the last reply changed files with a command, before its checks would start
    /// (`engine/src/engine/verifying.rs`), the reply's id: the conversation is at work meanwhile. Never shown or saved.
    #[serde(skip)]
    pub weighing: Option<String>,
    /// The person pressed Stop on the run under way: its end leaves no done card (`engine/src/engine/ended.rs`). Never saved.
    #[serde(skip)]
    pub stopped: bool,
}

/// The context of a conversation's own agent: `used` tokens of a `window`, the point where its CLI compacts by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextFill {
    pub used: u64,
    pub window: u64,
}

/// One part of what fills a conversation's own agent's context, as its CLI counts it: `key` is a `ctx.part.*` key (the
/// system part, tools, files, the conversation, other).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPart {
    pub key: String,
    pub tokens: u64,
}

/// The longest name Faber keeps, in characters.
pub const NAME_LIMIT: usize = 40;

/// The name as Faber keeps it: control characters dropped, the ends trimmed, at most [`NAME_LIMIT`] characters. None when
/// nothing is left, which is how a cleared name is stored.
pub fn clean_name(raw: &str) -> Option<String> {
    let kept: String = raw.chars().filter(|c| !c.is_control()).collect();
    let name: String = kept.trim().chars().take(NAME_LIMIT).collect();
    let name = name.trim_end();
    (!name.is_empty()).then(|| name.to_string())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    /// Each conversation works in a separate copy of its own (R201): on by default, switched in Settings > Projects. It
    /// acts only in a git repository with a commit; elsewhere conversations work in the folder.
    #[serde(rename = "separateCopies", default = "separate_by_default")]
    pub separate_copies: bool,
    /// Fields written by a newer Faber, kept as they were so this version never drops them.
    #[serde(flatten)]
    pub extra: crate::project::Extra,
}

fn separate_by_default() -> bool {
    true
}

impl Project {
    pub fn new(id: String, name: String, path: String) -> Self {
        Project { id, name, path, separate_copies: separate_by_default(), extra: Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub area: Area,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub updated_at: u64,
    /// "Keep going" is on.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_going: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<u64>,
    /// The conversation whose agent started this one: the sidebar shows it under that one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_by: Option<String>,
    /// Its last run ended and nobody has opened it since, while that is recent (`Ended::current`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<Ended>,
    /// How far the agent's plan has come.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanCount>,
}

/// How a run ended that nobody saw end (task 016): a done card in the running column and a dot on the sidebar row until the
/// person opens the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ended {
    /// When, in milliseconds.
    pub at: u64,
    pub kind: EndKind,
    /// How long the run worked, from its first turn to its end, in milliseconds.
    pub work_ms: u64,
    /// The last line the agent wrote, as the running row showed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndKind {
    Done,
    /// It stopped on something only the person can settle: a question, Keep going's cap, an AI billed per use.
    NeedsYou,
    Failed,
}

impl Ended {
    /// A card nobody opens goes by itself: a failure after a day, the rest after a week (as the other apps' cards do).
    pub fn current(&self, now: u64) -> bool {
        let days = if self.kind == EndKind::Failed { 1 } else { 7 };
        now.saturating_sub(self.at) < days * 86_400_000
    }
}

/// Where a running conversation stands: answering, or waiting for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    Working,
    NeedsYou,
}

/// One conversation that is answering now or waits for the user: a row of the running list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningItem {
    pub conversation_id: String,
    /// The id of the turn's reply message. With the request id it names a question, because request ids repeat.
    pub turn_id: String,
    /// Empty while the conversation has none.
    pub title: String,
    pub area: Area,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// None while Auto is still choosing the model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<Vendor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub state: RunState,
    /// The `at` of the turn's reply message.
    pub since: u64,
    /// The questions the user has not answered yet.
    pub permissions: Vec<Permission>,
    /// Requests the CLI denied in the latest reply that wait for the person (empty when none).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parked: Vec<Parked>,
    #[serde(skip_serializing_if = "is_zero")]
    pub parked_more: u32,
    /// The subagent calls of the reply that seem stuck: each needs the person like a question does.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stuck: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<f64>,
    /// A followed pull request whose checks failed while the quota was tight: its number, and the commit, so each is told once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<(u64, String)>,
    /// "Keep going" is on: the conversation runs on its own.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_going: bool,
    /// "Keep going" waits to send the next turn: the row stays between turns, and says until when.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_wait: Option<KeepWait>,
    /// "Keep going" is on: how far it has come.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_progress: Option<KeepProgress>,
    /// Faber is asking git whether the last reply changed files with a command (`Conversation::weighing`), usually under a
    /// second: the reply says so and Stop ends it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub weighing: bool,
    /// The agent said all it will say and only the CLI's turn is still closing (its Stop hooks, say), with nothing else running
    /// for the conversation: the screen reads it as done and leaves it out of what it shows as running. Updates, the tray and
    /// the awake lock still count it, since a process is still at work.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub settled: bool,
    /// How far the agent's plan has come.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanCount>,
    /// What the agent said it is doing now, on the reply under way (`Message::now`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<String>,
}

/// The running list as the engine last sent it. The revision grows by one with every list the engine emits, so a
/// snapshot and an event can be told apart by age.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Running {
    pub revision: u64,
    pub items: Vec<RunningItem>,
}

/// The latest reading of one quota window of one vendor.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaReading {
    pub vendor: Vendor,
    pub window: String,
    pub used: f64,
    /// Seconds since the Unix epoch, as the CLI reports it; 0 when it did not say.
    pub resets_at: f64,
}

/// Why the quota changed the pick of Auto, shown under the model name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaceNote {
    /// `step` (one rung up, bought with quota that would expire), `fast` (moved away from a vendor that is being used faster than
    /// its daily line) or `low` (moved away from a vendor that is really near its limit).
    pub kind: String,
    pub vendor: Vendor,
}

/// The user's choices, in one plain file. Appearance belongs to the screen, so the engine only keeps it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<serde_json::Map<String, Value>>,
    /// Projects run the user's own CLI hooks too (off by default: a hook can block or slow every turn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_hooks: Option<bool>,
    /// Missing in older settings means commands run without asking in Chat and Projects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    /// Missing means live (R140).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_search: Option<WebSearch>,
    /// Codex only; missing means the model's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<ReasoningSummary>,
    /// Shared reply style; missing leaves the AI's own way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_style: Option<ReplyStyle>,
    /// The person's own words, kept when another style is selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_style_text: Option<String>,
    /// Attention notifications and their chime are both on unless explicitly disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notifications: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_sound: Option<bool>,
    /// The language of the screen ("en", "pt-BR"...); none follows the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// What the user did with the first-run guide; none means it has not run yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<Setup>,
    /// The answer to "Do you already use AI?"; none means no answer, which counts as a beginner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experience: Option<Experience>,
    /// False once the first run asked for the tutorial, true once it ended or was skipped; none before the first run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutorial_done: Option<bool>,
    /// How the user wants to be called, asked (optionally) at the first run. A patch with an empty text clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    /// Keep the computer awake while Faber works (on by default: SetThreadExecutionState with ES_CONTINUOUS | ES_SYSTEM_REQUIRED).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_awake: Option<bool>,
    /// Start Faber when signing in to Windows (off by default: registered in HKCU Run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autostart: Option<bool>,
    /// A random token that every stricter mode replaces, in the same save that changes the mode. A remembered "Always allow"
    /// stands only if it was made under the current token, wherever it is still stored, so a conversation file that failed to
    /// update cannot bring it back, and no clock is involved. A grant with no token stands only while the settings have never
    /// held one. Only the engine sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants_epoch: Option<String>,
    /// Receive previews: update checks read the preview channel (the newest build) instead of the weekly stable one (off by
    /// default). Turning it off never installs an older version: a running preview takes only the next stable line, never
    /// the stable of its own `X.Y.Z`, which may be built from an older preview (`src-tauri/src/update.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previews: Option<bool>,
}

/// A fresh grants epoch: 244 random bits as hex.
pub fn new_epoch() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Each switch's default lives here and nowhere else: the engine and the shell read these, and the screen is sent
/// [`Settings::shown`].
impl Settings {
    pub fn user_hooks(&self) -> bool {
        self.user_hooks.unwrap_or(false)
    }

    pub fn permissions(&self) -> Permissions {
        self.permissions.unwrap_or_default()
    }

    pub fn notifications(&self) -> bool {
        self.notifications.unwrap_or(true)
    }

    pub fn notification_sound(&self) -> bool {
        self.notification_sound.unwrap_or(true)
    }

    pub fn keep_awake(&self) -> bool {
        self.keep_awake.unwrap_or(true)
    }

    pub fn autostart(&self) -> bool {
        self.autostart.unwrap_or(false)
    }

    /// What the screen is sent: every switch with its value, so the screen keeps no default of its own. The file keeps
    /// only what the person changed, since saving is a patch.
    pub fn shown(self) -> Self {
        Settings {
            user_hooks: Some(self.user_hooks()),
            permissions: Some(self.permissions()),
            notifications: Some(self.notifications()),
            notification_sound: Some(self.notification_sound()),
            keep_awake: Some(self.keep_awake()),
            autostart: Some(self.autostart()),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Experience {
    Beginner,
    Experienced,
}

/// How a CLI session may act. `Ask`, `AcceptEdits`, `Plan`, `Auto` and `FullAccess` are the choices on screen (R121, R140);
/// `Isolated` exists only inside Faber.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permissions {
    /// Commands and file changes wait for a card the person answers.
    #[serde(rename = "ask")]
    Ask,
    /// File edits run, commands ask (R140).
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    /// Read and plan, no changes (R140).
    #[serde(rename = "plan")]
    Plan,
    /// The CLI's own reviewer approves the safe actions and the rest asks (R140).
    #[serde(rename = "auto")]
    Auto,
    /// "Do not ask": no cards, no sandbox around commands, network and the person's own integrations on. Only the CLI
    /// credential folders stay blocked, where the CLI can enforce it. Stored as `dontAsk`.
    #[default]
    #[serde(rename = "dontAsk")]
    FullAccess,
    /// Faber's own isolated mode (R97, R107, R109): commands run in a native sandbox with no network, only the working
    /// folder writable, integrations off and sensitive folders refused. For the model list, the router, internal checks and
    /// the self-development roles. Never stored, never chosen by the person.
    #[serde(skip)]
    Isolated,
}

impl Permissions {
    /// The modes that keep the person in the loop (everything the person can choose except full access): the CLI starts in
    /// its restricted setup, and the person's hooks stay off unless the switch is on.
    pub fn guarded(self) -> bool {
        matches!(self, Permissions::Ask | Permissions::AcceptEdits | Permissions::Plan | Permissions::Auto)
    }

    /// How far a mode lets the CLI act alone, from loosest to strictest. Moving to a higher number stops the turns in flight.
    pub fn strictness(self) -> u8 {
        match self {
            Permissions::FullAccess => 0,
            Permissions::Auto => 1,
            Permissions::AcceptEdits => 2,
            Permissions::Ask => 3,
            Permissions::Plan | Permissions::Isolated => 4,
        }
    }

    /// The mode Claude Code really runs. A CLI that does not list `auto` in its `--permission-mode` choices gets the nearest
    /// safer one, accept edits.
    pub fn for_claude(self, flags: crate::claude::ClaudeFlags) -> Permissions {
        match self {
            Permissions::Auto if !flags.auto_mode => Permissions::AcceptEdits,
            other => other,
        }
    }

    /// The locale key of the line that says a mode runs as another one. Only `for_claude` changes a mode, and only Auto.
    pub fn fallback_note(self, runs: Permissions) -> Option<String> {
        (self != runs).then(|| "mode.fallback.auto".to_string())
    }
}

/// Web search for Chat and the person's own project sessions (R140). Live is the default: the most capable agent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebSearch {
    /// What a launch gets when nobody chose: only the person's saved settings turn the web on (`Settings::tuning`).
    #[default]
    Off,
    /// Codex only; Claude Code has no cache, so it keeps searching and does not fetch pages.
    Cached,
    Live,
}

impl WebSearch {
    /// How little of the web it lets in: a higher number is stricter, and turning it up stops the turns under way.
    pub fn strictness(self) -> u8 {
        match self {
            WebSearch::Live => 0,
            WebSearch::Cached => 1,
            WebSearch::Off => 2,
        }
    }
}

/// Codex's reasoning summary. `Default` leaves the model's own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReasoningSummary {
    #[default]
    Default,
    Auto,
    Concise,
    Detailed,
    #[serde(rename = "none")]
    Hidden,
}

/// How the person wants replies in Chat and Projects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReplyStyle {
    #[default]
    Default,
    Short,
    Explained,
    Own,
}

pub fn clean_reply_style_text(text: &str) -> String {
    text.trim_matches(|c: char| c.is_whitespace() || c.is_control())
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(400)
        .collect()
}

/// What a launch takes from the settings besides the mode. Never reaches a project role.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Tuning {
    pub web: WebSearch,
    pub summary: ReasoningSummary,
    pub reply_style: ReplyStyle,
    pub reply_style_text: String,
}

impl Settings {
    pub fn tuning(&self) -> Tuning {
        Tuning {
            // Live is the default for the person's chat settings (R140); `Tuning::default()` is Off.
            web: self.web_search.unwrap_or(WebSearch::Live),
            summary: self.reasoning_summary.unwrap_or_default(),
            reply_style: self.reply_style.unwrap_or_default(),
            reply_style_text: self.reply_style_text.as_deref().map(clean_reply_style_text).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Setup {
    Done,
    Skipped,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl Conversation {
    /// The session to continue for this vendor: the one that has shown output, else one whose turn was accepted and then cut off.
    pub fn resumable(&self, vendor: Vendor) -> Option<&String> {
        self.sessions.get(&vendor).or_else(|| self.tentative.get(&vendor))
    }

    /// Forgets every session of the vendor.
    pub fn forget_session(&mut self, vendor: Vendor) {
        self.sessions.remove(&vendor);
        self.tentative.remove(&vendor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screen_is_sent_every_switch_and_the_file_keeps_only_what_the_person_changed() {
        assert_eq!(
            serde_json::to_value(Settings::default().shown()).unwrap(),
            serde_json::json!({ "userHooks": false, "permissions": "dontAsk", "notifications": true, "notificationSound": true, "keepAwake": true, "autostart": false })
        );
        let mine = Settings { keep_awake: Some(false), language: Some("de".into()), ..Settings::default() };
        assert_eq!(serde_json::to_value(&mine).unwrap(), serde_json::json!({ "keepAwake": false, "language": "de" }));
        let shown = mine.shown();
        assert_eq!((shown.keep_awake, shown.notifications, shown.language.as_deref()), (Some(false), Some(true), Some("de")));
    }

    #[test]
    fn a_name_is_trimmed_without_control_characters_and_cut_at_forty_characters() {
        assert_eq!(clean_name("  Erick  ").as_deref(), Some("Erick"));
        assert_eq!(clean_name("Er\u{0}ick\n\t\u{7f}").as_deref(), Some("Erick"));
        assert_eq!(clean_name("José da Silva").as_deref(), Some("José da Silva"));
        assert_eq!(clean_name(""), None);
        assert_eq!(clean_name(" \t\n\u{7} "), None);
        let long = "ã".repeat(50);
        assert_eq!(clean_name(&long).unwrap().chars().count(), NAME_LIMIT);
        assert_eq!(clean_name(&format!("{} x", "a".repeat(39))).as_deref(), Some(&*"a".repeat(39)), "no space left dangling at the cut");
    }

    #[test]
    fn a_parked_item_saved_with_an_empty_or_missing_kind_loads_as_other() {
        for json in [r#"{"toolUseId":"t","tool":"Read"}"#, r#"{"toolUseId":"t","tool":"Read","kind":""}"#] {
            assert_eq!(serde_json::from_str::<Parked>(json).unwrap().kind, "other", "{json}");
        }
        assert_eq!(serde_json::from_str::<Parked>(r#"{"toolUseId":"t","tool":"Bash","kind":"command"}"#).unwrap().kind, "command");
    }
}
