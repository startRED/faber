//! What a project conversation stands on: its separate copy (`copy`), the checks it runs (`checks`, `proof`), the git it
//! reads (`git`), the pull requests it follows (`remote`) and the budget of its unbounded turns (`budget`). The contracts
//! are in `docs/design/projects.md`.
//!
//! The records keep the fields a newer Faber wrote in `extra`, so an older build can read and rewrite them without dropping
//! anything.

pub mod budget;
pub mod checks;
pub mod copy;
pub mod git;
pub mod goals;
pub mod lease;
pub mod memory;
pub mod proof;
pub mod remote;
pub mod rules;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Fields a newer Faber wrote that this build does not know, kept as they were.
pub type Extra = BTreeMap<String, Value>;

/// Whether `id` can name a record file: 1 to 64 ASCII letters, digits, `-` or `_`.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A process by its id and the moment it started: the start time tells a reused id from the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessId {
    pub pid: u32,
    pub started_at: u64,
}
