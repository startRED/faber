//! Faber's engine: drives the official AI CLIs (Claude Code, Codex) and keeps the conversations.
//! The app shell (Tauri) only calls into it; nothing here knows about windows or screens.

pub mod claude;
pub mod cli;
pub mod codex;
pub mod engine;
pub mod events;
pub mod model;
pub mod proc;
pub mod router;
pub mod store;
pub mod workspace;

pub use engine::{AppEvent, AppState, BranchRequest, Engine, EngineOptions, SendRequest, TerminalCommand, WarmRequest};
