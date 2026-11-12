//! The bottom layer of Faber's engine: the conversation model and the types it names, processes, routing and events.
//! `faber-engine-store`, `faber-engine-projects` and `faber-engine-drivers` build on it, and `faber-engine` re-exports
//! every module here, so callers keep one path (`faber_engine::model`). Nothing here knows about windows or screens.

pub mod act;
pub mod background;
pub mod browser;
pub mod budget;
pub mod cadence;
pub mod change;
pub mod conversation_state;
pub mod events;
pub mod facts;
pub mod failure;
pub mod follow;
pub mod handover;
pub mod learn;
pub mod look;
pub mod map;
pub mod model;
pub mod nudge;
pub mod pieces;
pub mod points;
pub mod proc;
pub mod results;
pub mod review;
pub mod route;
pub mod router;
pub mod screens;
pub mod signals;
pub mod startup;
pub mod stuck;
pub mod tls;
pub mod usage;

/// Lowercase hex of a digest or any bytes, the form every stored and compared hash uses.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
