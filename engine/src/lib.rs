//! Faber's engine: drives the official AI CLIs (Claude Code, Codex) and keeps the conversations.
//! The app shell only calls into it; nothing here knows about windows or screens.
//!
//! The `Engine` itself (`engine`) lives in this crate; every layer below it lives in `faber-engine-core` and is re-exported
//! here under the same names, so an edit to the `Engine` rebuilds only this crate and the ones above it.

pub use faber_engine_core::*;
pub use faber_engine_drivers::*;

pub mod engine;

pub use engine::{AppEvent, AppState, BranchRequest, Clock, Engine, EngineOptions, SendRequest, TabStep, TerminalCommand, WarmRequest};
