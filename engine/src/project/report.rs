//! Mission deliverable report (spec G8).
//! Persisted to `.faber/missions/<id>/report.json` upon landing.

use super::{Extra, SCHEMA};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Scalar reports reserve the literal string "unknown" for missing evidence; known strings are git object IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum UnknownOr<T> {
    Known(T),
    Unknown(String),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for UnknownOr<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum Marker {
            #[serde(rename = "unknown")]
            Unknown,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Metric<T> {
            Unknown(Marker),
            Known(T),
        }
        // The existing scalar format reserves "unknown" for missing evidence, including string metrics.
        Ok(match Metric::deserialize(deserializer)? {
            Metric::Unknown(_) => Self::Unknown("unknown".into()),
            Metric::Known(value) => Self::Known(value),
        })
    }
}

impl<T> Default for UnknownOr<T> {
    fn default() -> Self {
        UnknownOr::Unknown("unknown".into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RoleTokens {
    pub input: UnknownOr<u64>,
    pub cache_read: UnknownOr<u64>,
    pub cache_write: UnknownOr<u64>,
    pub output: UnknownOr<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub id: String,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitReceiptReport {
    pub intent: String,
    pub head: UnknownOr<String>,
    pub tree: UnknownOr<String>,
    pub parent: UnknownOr<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeReceiptReport {
    pub intent: String,
    pub old_head: UnknownOr<String>,
    pub new_head: UnknownOr<String>,
    pub tree: UnknownOr<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReportTimings {
    pub started_at: UnknownOr<u64>,
    pub landed_at: UnknownOr<u64>,
    pub duration_seconds: UnknownOr<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MissionReport {
    pub schema: u32,
    pub mission: String,
    pub goal: String,
    pub state: String,
    pub tokens_per_role: BTreeMap<String, RoleTokens>,
    pub checks: Vec<CheckReport>,
    #[serde(default)]
    pub tasks: BTreeMap<String, TaskReport>,
    pub commit_receipt: Option<CommitReceiptReport>,
    pub tree_receipt: Option<TreeReceiptReport>,
    pub timings: ReportTimings,
    #[serde(flatten)]
    pub extra: Extra,
}

impl MissionReport {
    pub fn new(mission: &str, goal: &str, state: &str) -> Self {
        Self {
            schema: SCHEMA,
            mission: mission.into(),
            goal: goal.into(),
            state: state.into(),
            tokens_per_role: BTreeMap::new(),
            checks: Vec::new(),
            tasks: BTreeMap::new(),
            commit_receipt: None,
            tree_receipt: None,
            timings: ReportTimings::default(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskReport {
    pub checks: Vec<CheckReport>,
    pub commit_receipt: CommitReceiptReport,
    pub tree_receipt: TreeReceiptReport,
    pub landed_at: u64,
}
