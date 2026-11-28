//! Command preflight scans every live Store, so hostile Store fixtures need their own process.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod project_security;
#[path = "../support/mod.rs"]
mod support;
