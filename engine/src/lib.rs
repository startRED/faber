//! Faber's engine: drives the official AI CLIs (Claude Code, Codex) and keeps the conversations.
//! The app shell (Tauri) only calls into it; nothing here knows about windows or screens.

pub mod account;
pub mod background;
pub mod browser;
pub mod budget;
pub mod cadence;
pub mod changes;
pub mod claude;
pub mod claude_sessions;
pub mod cli;
pub mod codex;
pub mod engine;
pub mod events;
pub mod export;
pub mod fileview;
pub mod follow;
pub mod handover;
pub mod helper;
pub mod install;
pub mod learn;
pub mod ledger;
pub mod map;
pub mod mcp_servers;
pub mod mention;
pub mod model;
pub mod nudge;
pub mod permissions;
pub mod persona;
pub mod proc;
pub mod progress;
pub mod project;
mod prompt_history;
pub mod recents;
pub mod rewind;
pub mod route;
pub mod router;
pub mod screens;
pub mod search;
pub mod sibling;
pub mod signals;
pub mod store;
pub mod stuck;
pub mod tools;
pub mod usage;
pub mod verify;
pub mod visual;
pub mod workspace;

pub use account::AccountStatus;
pub use engine::{AppEvent, AppState, BranchRequest, Clock, Engine, EngineOptions, SendRequest, TerminalCommand, WarmRequest};

/// Lowercase hex of a digest or any bytes, the form every stored and compared hash uses.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
