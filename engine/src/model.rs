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

/// Chat never touches the user's files; a project works inside a folder the user opened.
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

/// Dispatch intent for a quota error that still offers explicit recovery after restart. Consent is never stored here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recovery {
    pub pick: crate::router::Pick,
    pub text: String,
    pub by_hand: bool,
    pub retried: bool,
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
}

/// A permission request waiting for the user.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permission {
    pub request_id: String,
    pub tool: String,
    pub input: Value,
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
    /// True when the token numbers cover only part of the turn (sub-model work, or no usage was seen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete: Option<bool>,
    /// Why the quota changed the model Auto chose, when it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pace: Option<PaceNote>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<Vendor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
}

impl Message {
    pub fn new(id: String, role: Role, text: String, at: u64) -> Self {
        Message {
            id,
            role,
            text,
            text_breaks: None,
            activity_end: None,
            at,
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
            incomplete: None,
            pace: None,
            wait_until: None,
            moved_from: None,
            recovery: None,
        }
    }
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    /// The choice for the next message, retained independently in each conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<crate::router::Choice>,
    pub area: Area,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// One CLI session per vendor, so switching models keeps each vendor's own history and cache.
    #[serde(default)]
    pub sessions: BTreeMap<Vendor, String>,
    /// The cumulative cost each CLI session last reported, so a turn costs the difference (`usage::settle`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cost_base: BTreeMap<Vendor, crate::usage::CostBase>,
    /// Tools the user chose to always allow in this conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_always: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_of: Option<BranchOf>,
    /// What the user asked to be called when this conversation started. Kept here so the CLI gets the same instructions every
    /// time the conversation resumes, whatever Settings says by then; a conversation saved before this field has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued: Vec<QueuedTurn>,
    pub messages: Vec<Message>,
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

/// The one line both CLIs get when a conversation starts with a name. The name only changes how the answer addresses the
/// user, so the line asks for nothing else.
pub fn name_instructions(name: &str) -> String {
    format!("The user would like to be called {name}. Use the name only where it feels natural, and never mention this note.")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    /// The repository this folder belongs to once it is enrolled for missions. A project saved before missions has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<crate::project::RepoIdentity>,
    /// What the person granted at mission start. None grants no effect: planning stays read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant: Option<crate::project::Grant>,
    /// Fields written by a newer Faber, kept as they were so this version never drops them.
    #[serde(flatten)]
    pub extra: crate::project::Extra,
}

impl Project {
    pub fn new(id: String, name: String, path: String) -> Self {
        Project { id, name, path, repo: None, grant: None, extra: Default::default() }
    }

    /// Whether this project may take `effect` under the grant revision the work was admitted with. A key says why not.
    pub fn allows(&self, effect: crate::project::Effect, revision: u64) -> Result<(), &'static str> {
        match (&self.repo, &self.grant) {
            (Some(repo), Some(grant)) => grant.allows(repo, revision, effect),
            _ => Err("reason.grant.none"),
        }
    }

    /// Accept the authority shown on the start card.
    pub fn accept_grant(&mut self, mut grant: crate::project::Grant) -> Result<(), &'static str> {
        if grant.schema > crate::project::SCHEMA || self.grant.as_ref().is_some_and(|g| g.schema > crate::project::SCHEMA) {
            return Err("reason.grant.newer");
        }
        let expected = self.grant.as_ref().map_or(Some(1), |g| g.revision.checked_add(1)).ok_or("reason.grant.stale")?;
        if grant.revision != expected {
            return Err("reason.grant.stale");
        }
        if self.repo.as_ref().is_none_or(|r| r.common_dir != grant.repo) {
            return Err("reason.grant.otherRepo");
        }
        // Publication is deferred in the self-development MVP (R103).
        if grant.mode != crate::project::GitMode::Seguro {
            return Err("reason.grant.mode");
        }
        if let Some(previous) = &self.grant {
            for (key, value) in &previous.extra {
                grant.extra.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        self.grant = Some(grant);
        Ok(())
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_until: Option<f64>,
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
    /// `step` (one rung up, bought with quota that would expire) or `low` (moved away from a vendor that is running low).
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Experience {
    Beginner,
    Experienced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Setup {
    Done,
    Skipped,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
