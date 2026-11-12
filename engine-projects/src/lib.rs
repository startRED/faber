//! Faber's engine for project work, above `faber-engine-core` and below the CLI drivers: projects (copies, git, goals,
//! remote checks), the tool catalog and its kits, project checks (`verify`), rewind points and file views. Its own crate
//! so an edit here rebuilds only this crate and the ones above it.

// The core's modules under their own names, so `crate::model` reads as it did before the split.
pub(crate) use faber_engine_core::*;

pub mod changes;
pub mod crash;
pub mod fileview;
pub mod guide;
pub mod mcp_servers;
pub mod memory_search;
pub mod mention;
pub mod project;
pub mod rewind;
pub mod tool_inventory;
pub mod toolbox;
pub mod tools;
pub mod verify;
pub mod visual;
