//! Native local lifecycle adapters shared with the independent first-party helper.

// The helper builds from this library alone, so the guard of main.rs holds here too: no release build has the test hooks
// (the faster update clock, R163).
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!("the `test-hooks` feature is for debug test builds only; it must not be enabled in a release build");

pub mod autostart;
pub mod awake;
pub mod local_update;
