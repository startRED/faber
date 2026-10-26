//! The engine's integration tests, linked into one test program instead of one per file: on Windows each program is a
//! separate link and a separate process, and one changed file relinked all 27 at once (33 linkers, 7.8 GB). The files
//! stay where they are; a new one must be listed here, and `every_test_file_is_listed` fails until it is.
//! Cost tests keep their own programs: they read the memory of their own process, which no other test may share.

// Real containment helpers must not create console hosts that outlive the simulated CLI workers.
#![cfg_attr(windows, windows_subsystem = "windows")]

// Shared by the files below, which `use crate::support` and `use crate::loopback`.
mod loopback;
#[path = "mcp_servers.rs"]
mod mcp_servers;
#[path = "prompt_history.rs"]
mod prompt_history;
mod support;
#[path = "tool_inventory.rs"]
mod tool_inventory;

#[path = "account.rs"]
mod account;
#[path = "approve_plan_review.rs"]
mod approve_plan_review;
#[path = "auto.rs"]
mod auto;
#[path = "auto_archive.rs"]
mod auto_archive;
#[path = "auto_learned.rs"]
mod auto_learned;
#[path = "background.rs"]
mod background;
#[path = "background_live.rs"]
mod background_live;
#[path = "browser_tabs.rs"]
mod browser_tabs;
#[path = "budget.rs"]
mod budget;
#[path = "cache_health.rs"]
mod cache_health;
#[path = "changes.rs"]
mod changes;
#[path = "claude_session.rs"]
mod claude_session;
#[path = "claude_sessions.rs"]
mod claude_sessions;
#[path = "codex_session.rs"]
mod codex_session;
#[path = "compact.rs"]
mod compact;
#[path = "conversation_copy.rs"]
mod conversation_copy;
#[path = "engine.rs"]
mod engine;
#[path = "export.rs"]
mod export;
#[path = "failed_writes.rs"]
mod failed_writes;
#[path = "goals.rs"]
mod goals;
#[path = "groups.rs"]
mod groups;
#[path = "guide.rs"]
mod guide;
#[path = "handover.rs"]
mod handover;
#[path = "install.rs"]
mod install;
#[path = "keep_going.rs"]
mod keep_going;
#[path = "kits.rs"]
mod kits;
#[path = "learn.rs"]
mod learn;
#[path = "learning.rs"]
mod learning;
#[path = "ledger.rs"]
mod ledger;
#[path = "locales.rs"]
mod locales;
#[path = "look.rs"]
mod look;
#[path = "pacing.rs"]
mod pacing;
#[path = "paid_guard.rs"]
mod paid_guard;
#[path = "persona.rs"]
mod persona;
#[path = "plain_words.rs"]
mod plain_words;
#[path = "plan.rs"]
mod plan;
#[path = "plugins.rs"]
mod plugins;
#[path = "pr_watch.rs"]
mod pr_watch;
#[path = "product_map.rs"]
mod product_map;
#[path = "project.rs"]
mod project;
#[path = "project_budget.rs"]
mod project_budget;
#[path = "project_git.rs"]
mod project_git;
#[path = "project_proof_confirm.rs"]
mod project_proof_confirm;
#[path = "project_review.rs"]
mod project_review;
#[path = "project_security.rs"]
mod project_security;
#[path = "quota.rs"]
mod quota;
#[path = "quota_live.rs"]
mod quota_live;
#[path = "recents.rs"]
mod recents;
#[path = "rewind.rs"]
mod rewind;
#[path = "route.rs"]
mod route;
#[path = "route_pacing.rs"]
mod route_pacing;
#[path = "running.rs"]
mod running;
#[path = "search.rs"]
mod search;
#[path = "session_resume.rs"]
mod session_resume;
#[path = "settings_change_retire.rs"]
mod settings_change_retire;
#[path = "signals.rs"]
mod signals;
#[path = "simulation.rs"]
mod simulation;
#[path = "stuck.rs"]
mod stuck;
#[path = "toolbox.rs"]
mod toolbox;
#[path = "tools.rs"]
mod tools;
#[path = "transcript.rs"]
mod transcript;
#[path = "turn_usage.rs"]
mod turn_usage;
#[path = "usage_view.rs"]
mod usage_view;
#[path = "verify.rs"]
mod verify;
#[path = "visual_files.rs"]
mod visual_files;
#[path = "visual_pieces.rs"]
mod visual_pieces;
#[path = "waiting.rs"]
mod waiting;
#[path = "workspace.rs"]
mod workspace;

// The benchmark's pure parts (`bench/pure.rs`, also built into `engine/examples/bench.rs`); their tests run here.
#[allow(dead_code)]
#[path = "../../bench/pure.rs"]
mod bench_pure;

#[allow(dead_code)]
#[path = "../../bench/inputs.rs"]
mod bench_inputs;

#[test]
fn every_test_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let this = include_str!("all.rs");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let alone = ["all.rs", "install_memory.rs", "rewind_cost.rs", "background_cost.rs", "verify_cost.rs"].contains(&name.as_str());
        if name.ends_with(".rs") && !alone && !this.contains(&format!("#[path = \"{name}\"]")) {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "add these to engine/tests/all.rs: {missing:?}");
}
