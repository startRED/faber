//! Faber's own test runner, and the fake CLI it replays recordings with.

pub mod app;
pub mod cdp;
pub mod checks;
pub mod contain;
#[cfg(windows)]
pub mod desktop;
pub mod fake;
pub mod gate;
pub mod page;
pub mod plan;
pub mod proctree;
pub mod replay;
pub mod report;
pub mod runner;
pub mod sandbox;
pub mod screen;
pub mod sentinel;
pub mod server;
pub mod testbuild;
