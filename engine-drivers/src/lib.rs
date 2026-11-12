//! The CLI drivers of Faber's engine, above `faber-engine-core`: the Claude Code and Codex sessions, the persona they
//! start with, the helper, the agent's own tools (`plan`, `status`, `wrap_up`, the conversation tool) and the account
//! readings. Edits here, frequent ones, rebuild this crate and the `Engine` only. `faber-engine` re-exports both.

// The core's modules under their own names, so `crate::model` reads as it did before the split.
pub(crate) use faber_engine_core::*;

pub mod account;
pub mod claude;
pub mod codex;
pub mod creative;
pub mod helper;
pub mod persona;
pub mod progress;
pub mod sibling;

pub use account::AccountStatus;
