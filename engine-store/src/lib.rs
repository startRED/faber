//! What Faber's engine keeps on disk, above `faber-engine-core`: the store of conversations and settings, permissions,
//! a project's memory, rules and budget records, search, transcripts, the chat workspaces and the CLI installs. Its own
//! crate so an edit here rebuilds only this crate and the ones above it.

// The core's modules under their own names, so `crate::model` reads as it did before the split.
pub(crate) use faber_engine_core::*;

pub mod auto_learned;
pub mod claude_sessions;
pub mod cli;
pub mod export;
pub mod install;
pub mod ledger;
pub mod look_log;
pub mod method_log;
pub mod permissions;
pub mod project_budget;
pub mod project_memory;
pub mod project_rules;
mod prompt_history;
pub mod recents;
pub mod search;
pub mod store;
pub mod transcript;
pub mod worker_memory;
pub mod workspace;
