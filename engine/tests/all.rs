//! The engine's integration tests, linked into one test program instead of one per file: on Windows each program is a
//! separate link and a separate process, and one changed file relinked all 27 at once (33 linkers, 7.8 GB). The files
//! stay where they are; a new one must be listed here, and `every_test_file_is_listed` fails until it is.
//! `install_memory.rs` keeps its own program: it reads the memory of its own process, which no other test may share.

// Shared by the files below, which `use crate::support` and `use crate::loopback`.
mod loopback;
mod support;

#[path = "account.rs"]
mod account;
#[path = "auto.rs"]
mod auto;
#[path = "book.rs"]
mod book;
#[path = "budget.rs"]
mod budget;
#[path = "cache_health.rs"]
mod cache_health;
#[path = "claude_session.rs"]
mod claude_session;
#[path = "codex_session.rs"]
mod codex_session;
#[path = "engine.rs"]
mod engine;
#[path = "handover.rs"]
mod handover;
#[path = "install.rs"]
mod install;
#[path = "learn.rs"]
mod learn;
#[path = "learning.rs"]
mod learning;
#[path = "ledger.rs"]
mod ledger;
#[path = "local_update.rs"]
mod local_update;
#[path = "pacing.rs"]
mod pacing;
#[path = "paid_guard.rs"]
mod paid_guard;
#[path = "persona.rs"]
mod persona;
#[path = "plan.rs"]
mod plan;
#[path = "product_map.rs"]
mod product_map;
#[path = "project.rs"]
mod project;
#[path = "project_bar.rs"]
mod project_bar;
#[path = "project_bar_regressions.rs"]
mod project_bar_regressions;
#[path = "project_budget.rs"]
mod project_budget;
#[path = "project_compile.rs"]
mod project_compile;
#[path = "project_containment.rs"]
mod project_containment;
#[path = "project_git.rs"]
mod project_git;
#[path = "project_git_t5.rs"]
mod project_git_t5;
#[path = "project_kit.rs"]
mod project_kit;
#[path = "project_lead.rs"]
mod project_lead;
#[path = "project_lease.rs"]
mod project_lease;
#[path = "project_native.rs"]
mod project_native;
#[path = "project_policy.rs"]
mod project_policy;
#[path = "project_proof_confirm.rs"]
mod project_proof_confirm;
#[path = "project_proof_settle.rs"]
mod project_proof_settle;
#[path = "project_qualification.rs"]
mod project_qualification;
#[path = "project_queue.rs"]
mod project_queue;
#[path = "project_r132.rs"]
mod project_r132;
#[path = "project_recovery.rs"]
mod project_recovery;
#[path = "project_report.rs"]
mod project_report;
#[path = "project_review.rs"]
mod project_review;
#[path = "project_route_v0.rs"]
mod project_route_v0;
#[path = "project_run.rs"]
mod project_run;
#[path = "project_screen_proofs.rs"]
mod project_screen_proofs;
#[path = "project_security.rs"]
mod project_security;
#[path = "project_wire_v2.rs"]
mod project_wire_v2;
#[path = "project_worker.rs"]
mod project_worker;
#[path = "quota.rs"]
mod quota;
#[path = "quota_live.rs"]
mod quota_live;
#[path = "recents.rs"]
mod recents;
#[path = "route.rs"]
mod route;
#[path = "route_pacing.rs"]
mod route_pacing;
#[path = "running.rs"]
mod running;
#[path = "session_resume.rs"]
mod session_resume;
#[path = "signals.rs"]
mod signals;
#[path = "simulation.rs"]
mod simulation;
#[path = "turn_usage.rs"]
mod turn_usage;
#[path = "usage_view.rs"]
mod usage_view;
#[path = "waiting.rs"]
mod waiting;
#[path = "workspace.rs"]
mod workspace;

#[test]
fn every_test_file_is_listed() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let this = include_str!("all.rs");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let alone = ["all.rs", "install_memory.rs"].contains(&name.as_str());
        if name.ends_with(".rs") && !alone && !this.contains(&format!("#[path = \"{name}\"]")) {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "add these to engine/tests/all.rs: {missing:?}");
}
