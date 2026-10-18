//! The native adapters the shell shares with its tests: start with Windows and keep awake.

// The guard of main.rs holds here too: no release build has the test hooks.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!("the `test-hooks` feature is for debug test builds only; it must not be enabled in a release build");

pub mod autostart;
pub mod awake;
