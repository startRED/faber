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
}

impl Message {
    pub fn new(id: String, role: Role, text: String, at: u64) -> Self {
        Message {
            id,
            role,
            text,
            at,
            images: None,
            attachments: None,
            vendor: None,
            model: None,
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchOf {
    pub conversation_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub area: Area,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// One CLI session per vendor, so switching models keeps each vendor's own history and cache.
    #[serde(default)]
    pub sessions: BTreeMap<Vendor, String>,
    /// Tools the user chose to always allow in this conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_always: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_of: Option<BranchOf>,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
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

/// The user's choices, in one plain file. Appearance belongs to the screen, so the engine only keeps it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<serde_json::Map<String, Value>>,
    /// Projects run the user's own CLI hooks too (off by default: a hook can block or slow every turn).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_hooks: Option<bool>,
    /// The language of the screen ("en", "pt-BR"...); none follows the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// What the user did with the first-run guide; none means it has not run yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<Setup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Setup {
    Done,
    Skipped,
}
