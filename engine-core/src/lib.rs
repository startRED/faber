//! The layers of Faber's engine below the `Engine` itself: the conversation model, storage, the CLI drivers, routing,
//! tools, projects and checks. `faber-engine` builds the `Engine` on top and re-exports every module here, so callers keep
//! one path (`faber_engine::model`). Nothing here knows about windows or screens.

pub mod act;
pub mod auto_learned;
pub mod background;
pub mod browser;
pub mod budget;
pub mod cadence;
pub mod change;
pub mod changes;
pub mod claude_sessions;
pub mod cli;
pub mod conversation_state;
pub mod crash;
pub mod events;
pub mod export;
pub mod facts;
pub mod failure;
pub mod fileview;
pub mod follow;
pub mod guide;
pub mod handover;
pub mod install;
pub mod learn;
pub mod ledger;
pub mod look;
pub mod look_log;
pub mod map;
pub mod mcp_servers;
pub mod memory_search;
pub mod mention;
pub mod method_log;
pub mod model;
pub mod nudge;
pub mod permissions;
pub mod pieces;
pub mod proc;
pub mod project;
mod prompt_history;
pub mod recents;
pub mod results;
pub mod review;
pub mod rewind;
pub mod route;
pub mod router;
pub mod screens;
pub mod search;
pub mod signals;
pub mod startup;
pub mod store;
pub mod stuck;
pub mod tls;
pub mod tool_inventory;
pub mod toolbox;
pub mod tools;
pub mod transcript;
pub mod usage;
pub mod verify;
pub mod visual;
pub mod worker_memory;
pub mod workspace;

/// Lowercase hex of a digest or any bytes, the form every stored and compared hash uses.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
