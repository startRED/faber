//! The bottom layer of Faber's engine: the conversation model, storage, routing, processes, permissions and the records a
//! project keeps (memory, rules, budget). `faber-engine-projects` and `faber-engine-drivers` build on it, and `faber-engine`
//! re-exports every module here, so callers keep one path (`faber_engine::model`). Nothing here knows about windows or screens.

pub mod act;
pub mod auto_learned;
pub mod background;
pub mod browser;
pub mod budget;
pub mod cadence;
pub mod change;
pub mod claude_sessions;
pub mod cli;
pub mod conversation_state;
pub mod events;
pub mod export;
pub mod facts;
pub mod failure;
pub mod follow;
pub mod handover;
pub mod install;
pub mod learn;
pub mod ledger;
pub mod look;
pub mod look_log;
pub mod map;
pub mod method_log;
pub mod model;
pub mod nudge;
pub mod permissions;
pub mod pieces;
pub mod points;
pub mod proc;
pub mod project_budget;
pub mod project_memory;
pub mod project_rules;
mod prompt_history;
pub mod recents;
pub mod results;
pub mod review;
pub mod route;
pub mod router;
pub mod screens;
pub mod search;
pub mod signals;
pub mod startup;
pub mod store;
pub mod stuck;
pub mod tls;
pub mod transcript;
pub mod usage;
pub mod worker_memory;
pub mod workspace;

/// Lowercase hex of a digest or any bytes, the form every stored and compared hash uses.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
